//! The third-person body: the oracle's player animation state, as far as movement drives it.
//! Each frame it reads the player, picks a main activity, selects that activity's sequence
//! under the player's modifiers, turns the body towards where it should face, and sets the
//! aim and movement pose parameters and the playback rate.
//!
//! Not modelled: melee, weapons and gestures, being downed or dead, ziplines, grapples,
//! skydiving, vehicles, staggering, dodging, wall running and climbing.

use super::*;
use crate::sim::math::{angle_vectors, is_world_up, V3};
use crate::sim::{Player, Settings};

/// ConVars `mp_maxbodyyaw`, `anim_playerMovementAngleMargin`, `movement_anim_playback_minrate`,
/// `movement_anim_playback_maxrate`, `movement_anim_sprint_playback_maxrate`.
pub const MAX_BODY_YAW: f32 = 60.0;
pub const MOVEMENT_ANGLE_MARGIN: f32 = 10.0;
pub const PLAYBACK_MIN: f32 = 0.25;
pub const PLAYBACK_MAX: f32 = 10.0;
pub const SPRINT_PLAYBACK_MAX: f32 = 1.25;

const UNSET_YAW: f32 = 720.0;
const MOVING_SPEED: f32 = 0.5;
const GRACE_IN_AIR: f32 = 0.2;
const JUMP_AIRBORNE: f32 = 0.5;
const JUMP_HOLD: f32 = 0.2;
const SLIDE_END_WINDOW: f32 = 5.0;
const SLIDE_END_PICK: f32 = 0.1;
const SPRINT_MARGIN: f32 = 1.05;
const WALK_FRACTION: f32 = 0.45;
const TURN_RATE: f32 = -7.0;
const FEET: f32 = 12.0;
const MOVETYPE_TRAVERSE: u8 = 12;

/// Player settings the body animation reads beyond movement's.
#[derive(Clone, Debug, Default)]
pub struct BodySettings {
    /// `quadMoveAnims`: four directional sets, and the body turns to the movement.
    pub quad_move_anims: bool,
    /// `sprintAnimOrientToMoveDir`.
    pub sprint_orients_to_move: bool,
    /// `forceAimYawToEye`.
    pub force_aim_yaw_to_eye: bool,
    /// `aimAngleBackwardEnd`, `aimAngleForwardStart`, `aimAngleForwardEnd`,
    /// `aimAngleBackwardStart`: eye yaw minus movement yaw bounds of each quadrant.
    pub aim_angles: [f32; 4],
    /// `classActivityModifier`, added to the modifiers when set.
    pub class_modifier: String,
}

/// What the body reads from the player each frame.
pub struct Frame<'a> {
    pub p: &'a Player,
    pub s: &'a Settings,
    pub velocity: V3,
    /// Eye angles: pitch, yaw, roll.
    pub eye: V3,
    pub curtime: f32,
    pub frametime: f32,
    /// The mantle's kind while traversing (`m_traversalType`); 0 otherwise.
    pub traversal_type: u32,
}

#[derive(Clone, Debug)]
pub struct Body {
    pub settings: BodySettings,
    /// The body's own yaw: where the model faces.
    pub yaw: f32,
    pub activity: Option<Activity>,
    pub playback: Playback,
    pub modifiers: Vec<String>,
    pub aim_pitch: f32,
    pub aim_yaw: f32,
    pub move_yaw: f32,
    pub move_yaw_backward: f32,
    pub crouch_fraction: f32,
    eye_yaw: f32,
    eye_pitch: f32,
    velocity_yaw: f32,
    speed: f32,
    forward: V3,
    right: V3,
    moving: bool,
    direction: Direction,
    quadrant: Direction,
    wall_up: Option<V3>,
    goal_yaw: f32,
    yaw_override: Option<f32>,
    idle_yaw: f32,
    heading: f32,
    jumping: bool,
    jump_new: bool,
    jump_time: f32,
    jump_activity: Activity,
    landing: bool,
    was_in_air: bool,
    land_time: f32,
    in_air: bool,
    land_speed: f32,
    slide_yaw: f32,
    slide_time: f32,
}

impl Body {
    pub fn new(settings: BodySettings) -> Self {
        Body {
            settings,
            yaw: 0.0,
            activity: None,
            playback: Playback::default(),
            modifiers: Vec::new(),
            aim_pitch: 0.0,
            aim_yaw: 0.0,
            move_yaw: 0.0,
            move_yaw_backward: 0.0,
            crouch_fraction: 0.0,
            eye_yaw: 0.0,
            eye_pitch: 0.0,
            velocity_yaw: 0.0,
            speed: 0.0,
            forward: [1.0, 0.0, 0.0],
            right: [0.0, -1.0, 0.0],
            moving: false,
            direction: Direction::Forward,
            quadrant: Direction::Forward,
            wall_up: None,
            goal_yaw: 0.0,
            yaw_override: None,
            idle_yaw: UNSET_YAW,
            heading: 0.0,
            jumping: false,
            jump_new: false,
            jump_time: f32::NEG_INFINITY,
            jump_activity: Activity::MP_JUMP_START,
            landing: false,
            was_in_air: false,
            land_time: 0.0,
            in_air: false,
            land_speed: 0.0,
            slide_yaw: 0.0,
            slide_time: f32::NEG_INFINITY,
        }
    }

    /// A player animation event, as movement raises it.
    pub fn event(&mut self, ev: PlayerEvent, now: f32) {
        match ev {
            PlayerEvent::Jump | PlayerEvent::DoubleJump | PlayerEvent::Dodge | PlayerEvent::WallJump => {
                self.jumping = true;
                self.jump_new = true;
                self.jump_time = now;
                self.in_air = false;
                self.landing = false;
                self.was_in_air = false;
                self.jump_activity = match ev {
                    PlayerEvent::Jump => Activity::MP_JUMP_START,
                    PlayerEvent::DoubleJump => Activity::MP_DOUBLEJUMP,
                    PlayerEvent::Dodge => Activity::MP_DODGE,
                    _ => self.wall_jump(),
                };
            }
            _ => {}
        }
    }

    /// Movement's landing speed (`Cue::Land`), which picks the land animation.
    pub fn landed(&mut self, speed: f32) {
        self.land_speed = speed;
    }

    fn wall_jump(&self) -> Activity {
        let (f, n) = (self.forward, self.wall_up.unwrap_or([0.0, 0.0, 1.0]));
        let along = f[0] * n[0] + f[1] * n[1] + f[2] * n[2];
        let across = (f[1] * n[0] - f[0] * n[1]) + f[2] * 0.0;
        if across <= along || -across <= along {
            Activity(Activity::MP_WALLJUMP_LEFT.0 + (0.0 < across) as u16)
        } else {
            Activity::MP_WALLJUMP_UP
        }
    }

    /// Standing, for animation: on the ground, or in the air under 0.2 s and not just jumped.
    fn grounded(&self, f: &Frame) -> bool {
        if f.p.flags & 1 != 0 {
            return true;
        }
        f.curtime - f.p.left_ground_time < GRACE_IN_AIR && JUMP_AIRBORNE < f.curtime - self.jump_time
    }

    /// Whether the sequence for `lo..=hi` started at `start` has played out. One that is not
    /// playing (yet) counts as done 0.2 s after `start`.
    fn finished(&self, set: &dyn AnimSet, lo: Activity, hi: Activity, start: f32, now: f32) -> bool {
        match self.playback.seq.and_then(|s| set.activity(s)) {
            Some(a) if lo.0 <= a.0 && a.0 <= hi.0 => self.playback.finished,
            _ => JUMP_HOLD < now - start,
        }
    }

    pub fn update(&mut self, f: &Frame, set: &dyn AnimSet) -> Vec<u16> {
        self.eye_yaw = angle_normalize(f.eye[1]);
        self.eye_pitch = angle_normalize(f.eye[0]);
        let (fwd, _, _) = angle_vectors([0.0, self.eye_yaw, 0.0]);
        self.forward = [fwd[0], fwd[1], 0.0];
        self.right = [fwd[1], -fwd[0], 0.0];
        let v = f.velocity;
        self.velocity_yaw = vec_to_yaw(v);
        self.speed = (v[0] * v[0] + v[1] * v[1]).sqrt();
        self.moving = MOVING_SPEED < (v[1] * v[1] + v[0] * v[0]).sqrt();
        self.wall_up = (!is_world_up(f.p.up)).then_some(f.p.up);
        self.update_direction(f.p.sprinting);
        self.update_modifiers(f.p);

        let activity = self.main_activity(f, set);
        self.activity = Some(activity);
        let mods: Vec<&str> = self.modifiers.iter().map(String::as_str).collect();
        self.playback.play(set.select(activity, &mods));

        self.aim_pitch = self.eye_pitch;
        self.turn_body(f, set);
        self.crouch_fraction = crouch_fraction(f.p);
        self.update_move_yaw(f);
        self.playback.rate = (self.playback_rate(set) * 8.0).round_ties_even() * 0.125;
        self.playback.advance(set, f.frametime)
    }

    fn update_direction(&mut self, sprinting: bool) {
        if !self.moving {
            if self.settings.quad_move_anims {
                self.quadrant = Direction::None;
                self.direction = Direction::None;
            } else {
                if self.quadrant != Direction::Backward {
                    self.quadrant = Direction::Forward;
                }
                if self.direction != Direction::Backward {
                    self.direction = Direction::Forward;
                }
            }
            return;
        }
        let a = angle_normalize(self.eye_yaw - self.velocity_yaw);
        let [back_end, fwd_start, fwd_end, back_start] = self.settings.aim_angles;
        let m = MOVEMENT_ANGLE_MARGIN;
        let keep = match self.quadrant {
            Direction::Backward => !(m + back_end <= a && a <= back_start - m),
            Direction::Left => back_end - m < a && a < m + fwd_start,
            Direction::Right => fwd_end - m < a && a < m + back_start,
            Direction::Forward => fwd_start - m < a && a < m + fwd_end,
            Direction::None => false,
        };
        if !keep {
            self.quadrant = if a < back_end || back_start < a {
                Direction::Backward
            } else if fwd_start <= a {
                if a <= fwd_end { Direction::Forward } else { Direction::Right }
            } else {
                Direction::Left
            };
        }
        if self.settings.quad_move_anims {
            self.direction = self.quadrant;
        } else {
            let keep = match self.direction {
                Direction::Backward => !(m - 90.0 <= a && a <= 90.0 - m),
                Direction::Forward => -90.0 - m < a && a < m + 90.0,
                _ => false,
            };
            if !keep {
                self.direction = if !(-90.0..=90.0).contains(&a) { Direction::Backward } else { Direction::Forward };
            }
        }
        if sprinting {
            self.direction = Direction::Forward;
        }
    }

    fn update_modifiers(&mut self, p: &Player) {
        let mut m: Vec<usize> = Vec::new();
        if p.traversal_state.wrapping_sub(1) < 2 {
            m.push(MOD_CROUCH);
        }
        if self.direction != Direction::None {
            m.push(MOD_MOVING[self.direction as usize - 1]);
        }
        if p.sprinting {
            m.push(MOD_SPRINTING);
        }
        if p.ground.is_none() {
            m.push(MOD_AIR);
        }
        if p.sliding {
            m.push(MOD_SLIDING);
        }
        self.modifiers = m.into_iter().map(|i| MODIFIERS[i].to_string()).collect();
        if !self.settings.class_modifier.is_empty() {
            self.modifiers.push(self.settings.class_modifier.clone());
        }
    }

    fn toward(&self, a: Activity) -> Activity {
        if self.moving { a.toward(self.direction) } else { a }
    }

    /// Running, walking, sprinting, or sliding without crouching (moving faster than sprint
    /// speed), by speed with side and back movement scaled up to forward terms.
    fn ground_activity(&mut self, f: &Frame, act: &mut Activity) {
        if !self.grounded(f) {
            self.in_air = true;
            return;
        }
        if !self.moving {
            return;
        }
        let (v, s, k) = (f.velocity, f.s, f.p.speed_scale);
        let (fw, r) = (self.forward, self.right);
        let mut ahead = v[1] * fw[1] + v[0] * fw[0] + v[2] * fw[2];
        let side = (v[1] * r[1] + v[0] * r[0] + v[2] * r[2]) / s.speed_scale_side;
        if ahead < 0.0 {
            ahead /= s.speed_scale_back;
        }
        let speed = (side * side + ahead * ahead).sqrt();
        let run = s.stances[0].speed;
        let sprint = if s.stances[0].sprint_speed < 0.0 { run } else { s.stances[0].sprint_speed };
        let sprint = sprint * k * SPRINT_MARGIN;
        *act = if sprint <= speed || (self.direction != Direction::Forward && run * k * SPRINT_MARGIN <= speed) {
            Activity::MP_SLIDE
        } else if f.p.sprinting {
            Activity::MP_SPRINT
        } else if speed < run * WALK_FRACTION {
            Activity::MP_WALK
        } else {
            Activity::MP_RUN
        };
        if *act != Activity::MP_SPRINT {
            *act = act.toward(self.direction);
        }
    }

    /// The land animation, by how hard the player came down, until it has played out.
    fn land_activity(&mut self, f: &Frame, set: &dyn AnimSet, act: &mut Activity) -> bool {
        if !(self.landing || self.was_in_air) || !self.grounded(f) || f.p.dead {
            return false;
        }
        self.jumping = false;
        self.in_air = false;
        if self.wall_up.is_some() {
            return false;
        }
        if self.was_in_air {
            self.land_time = f.curtime;
            self.playback.restart();
            self.was_in_air = false;
            self.landing = true;
        }
        let k = (f.s.sv_gravity + f.s.sv_gravity) * FEET;
        let speed = self.land_speed;
        let a = if speed < (k * f.s.land_dist_high).sqrt() {
            if speed < (k * f.s.land_dist_medium).sqrt() {
                if (k * f.s.land_dist_low).sqrt() <= speed { Activity::MP_JUMP_LAND_LOW } else { Activity::MP_JUMP_LAND }
            } else {
                Activity::MP_JUMP_LAND_MEDIUM
            }
        } else {
            Activity::MP_JUMP_LAND_HIGH
        };
        if self.finished(set, a, Activity(a.0 + 4), self.land_time, f.curtime) {
            self.landing = false;
            self.land_time = 0.0;
            return false;
        }
        *act = self.toward(a);
        true
    }

    fn slide_end(&self) -> Activity {
        let d = angle_diff(self.eye_yaw, self.slide_yaw);
        let end = |i: u16| Activity(Activity::MP_CROUCHSLIDE_END_FORWARD.0 + i);
        if d < -135.0 {
            end(2)
        } else if d < -45.0 {
            end(4)
        } else if d < 45.0 {
            end(0)
        } else if 135.0 <= d {
            end(1)
        } else {
            end(3)
        }
    }

    fn main_activity(&mut self, f: &Frame, set: &dyn AnimSet) -> Activity {
        let (p, now) = (f.p, f.curtime);
        if p.movetype == MOVETYPE_TRAVERSE && f.traversal_type != 0 {
            self.was_in_air = false;
            return Activity(Activity::MP_VEHICLE_DRIVER_IDLE.0 + f.traversal_type as u16);
        }
        let mut act = Activity::MP_STAND_IDLE;
        let mut jumping = false;
        if self.jumping {
            let mut done = false;
            if !self.jump_new {
                if self.finished(set, self.jump_activity, self.jump_activity, self.jump_time, now) {
                    self.jumping = false;
                    self.in_air = true;
                    done = true;
                }
            } else {
                self.jump_new = false;
                self.playback.restart();
            }
            if !done && JUMP_HOLD < now - self.jump_time && self.grounded(f) {
                self.jumping = false;
                self.playback.restart();
                done = true;
            }
            if !done {
                act = self.jump_activity;
                jumping = true;
            }
        }
        let mut crouching = false;
        if !jumping && p.traversal_state.wrapping_sub(1) < 2 && !p.sliding {
            crouching = true;
            act = if self.moving { Activity::MP_CROUCHWALK.toward(self.direction) } else { Activity::MP_CROUCH_IDLE };
        }
        let landing = self.land_activity(f, set, &mut act);
        if !landing {
            self.land_speed = 0.0;
        }
        if p.sliding {
            let a = if self.grounded(f) {
                if now - self.slide_time > SLIDE_END_PICK {
                    self.slide_yaw = self.eye_yaw;
                }
                Activity::MP_CROUCHSLIDE
            } else {
                self.slide_yaw = self.eye_yaw;
                Activity::MP_CROUCHSLIDE_AIR
            };
            self.yaw_override = Some(self.slide_yaw);
            self.slide_time = now;
            return a;
        }
        if now - self.slide_time < SLIDE_END_WINDOW && self.grounded(f) && !self.jumping {
            let ends = Activity::MP_CROUCHSLIDE_END_FORWARD.0..=Activity::MP_CROUCHSLIDE_END_RIGHT.0;
            if let Some(a) = self.playback.seq.and_then(|s| set.activity(s))
                && ends.contains(&a.0)
                && !self.playback.finished
            {
                return a;
            }
            if now - self.slide_time < SLIDE_END_PICK {
                return self.slide_end();
            }
        }
        if !jumping && !crouching && !landing {
            self.ground_activity(f, &mut act);
        }
        if !self.grounded(f) {
            self.was_in_air = true;
            if !p.dead && self.in_air {
                self.jumping = false;
                return Activity::MP_JUMP_FLOAT;
            }
        } else {
            self.in_air = false;
        }
        act
    }

    /// The body turns to its goal yaw at a rate of 7 per second, never so far from the eyes
    /// that `aim_yaw` would leave its range. Standing still, it waits until the eyes are more
    /// than `MAX_BODY_YAW` away and then turns.
    fn turn_body(&mut self, f: &Frame, set: &dyn AnimSet) {
        let traversing = f.p.movetype == MOVETYPE_TRAVERSE;
        let mut idle = false;
        self.goal_yaw = if f.p.dead || traversing {
            self.yaw
        } else if self.settings.force_aim_yaw_to_eye {
            self.eye_yaw
        } else if let Some(y) = self.yaw_override {
            y
        } else if !self.moving {
            if self.idle_yaw == UNSET_YAW {
                self.idle_yaw = self.yaw;
            }
            if MAX_BODY_YAW < angle_normalize(self.eye_yaw - self.idle_yaw).abs() {
                self.idle_yaw = self.eye_yaw;
            }
            idle = true;
            self.idle_yaw
        } else if !f.p.sprinting {
            if !self.settings.quad_move_anims {
                self.eye_yaw
            } else {
                match self.direction {
                    Direction::Forward => self.velocity_yaw,
                    Direction::Backward => angle_normalize(self.velocity_yaw + 180.0),
                    Direction::Left => angle_normalize(self.velocity_yaw - 90.0),
                    Direction::Right => angle_normalize(self.velocity_yaw + 90.0),
                    Direction::None => self.eye_yaw,
                }
            }
        } else if self.settings.sprint_orients_to_move {
            self.velocity_yaw
        } else {
            self.eye_yaw
        };
        if !idle {
            self.idle_yaw = UNSET_YAW;
        }
        let goal = angle_normalize(self.goal_yaw);
        if goal != self.yaw {
            if self.settings.force_aim_yaw_to_eye {
                self.yaw = goal;
            } else {
                let k = (TURN_RATE * f.frametime).exp();
                self.yaw = self.yaw + angle_diff(goal, self.yaw) * (1.0 - k) + k * 0.0;
            }
        }
        let (lo, hi) = set.pose_range(PoseParam::AimYaw).unwrap_or((0.0, 1.0));
        let mut d = angle_normalize(self.eye_yaw - self.yaw);
        if d <= lo {
            d = lo;
        }
        if hi <= d {
            d = hi;
        }
        if self.yaw_override.is_none() && !f.p.dead {
            self.yaw = angle_normalize(self.eye_yaw - d);
        }
        self.aim_yaw = angle_normalize(self.eye_yaw - self.yaw);
        self.yaw_override = None;
    }

    /// `move_yaw` is the movement's direction relative to the body, and `move_yaw_backward`
    /// the same for the backward set; both ease in at a rate of 7 per second.
    fn update_move_yaw(&mut self, f: &Frame) {
        let dt = f.frametime;
        if dt != 0.0 {
            let v = f.velocity;
            self.heading = if v[0] == 0.0 && v[1] == 0.0 {
                let k = if dt < 0.25 { dt * 4.0 } else { dt };
                angle_normalize(self.yaw - self.heading) * k + self.heading
            } else {
                angle_normalize(vec_to_yaw(v))
            };
        }
        let rel = angle_normalize(self.heading - angle_normalize(self.yaw));
        let (ahead, back) = if !(-90.0..=90.0).contains(&rel) {
            let b = angle_normalize(rel + 180.0);
            (-b, b)
        } else {
            (rel, -rel)
        };
        let k = (TURN_RATE * dt).exp();
        self.move_yaw = (1.0 - k) * ahead + k * self.move_yaw;
        self.move_yaw_backward = (1.0 - k) * back + k * self.move_yaw_backward;
    }

    /// Movement sequences play at the rate that makes their root motion match the player's
    /// speed, within limits; anything else plays at 1.
    fn playback_rate(&self, set: &dyn AnimSet) -> f32 {
        if !self.moving || self.jumping || self.in_air || self.landing || self.wall_up.is_some() {
            return 1.0;
        }
        let Some(seq) = self.playback.seq else { return 1.0 };
        let mut speed = set.ground_speed(seq);
        if speed < 0.001 {
            return 1.0;
        }
        if !self.settings.quad_move_anims {
            let r = self.move_yaw.to_radians();
            speed *= r.cos().abs() + r.sin().abs();
        }
        let r = self.speed / speed;
        let mut rate = if (PLAYBACK_MIN..=PLAYBACK_MAX).contains(&r) {
            r
        } else if PLAYBACK_MIN <= r {
            PLAYBACK_MAX
        } else {
            PLAYBACK_MIN
        };
        if self.activity == Some(Activity::MP_SPRINT) && SPRINT_PLAYBACK_MAX <= rate {
            rate = SPRINT_PLAYBACK_MAX;
        }
        rate
    }
}

/// UTIL_VecToYaw: 0 for no horizontal direction, else in [0, 360).
fn vec_to_yaw(v: V3) -> f32 {
    if v[0] == 0.0 && v[1] == 0.0 {
        return 0.0;
    }
    let yaw = v[1].atan2(v[0]).to_degrees();
    if yaw < 0.0 { yaw + 360.0 } else { yaw }
}

/// The crouch pose: eases in over the duck (400 ms, 200 when sliding) and out over the
/// unduck (200 ms), smoothstepped.
pub fn crouch_fraction(p: &Player) -> f32 {
    let smooth = |t: f32| (3.0 - (t + t)) * t * t;
    let clamp = |t: f32| if 0.0 <= t { if t <= 1.0 { t } else { 1.0 } } else { 0.0 };
    match p.traversal_state {
        1 => 1.0 - smooth(clamp(p.duck_timer as f32 / if p.sliding { 200.0 } else { 400.0 })),
        2 => 1.0,
        3 => smooth(clamp(p.duck_timer as f32 / 200.0)),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::Stance;

    /// Every activity has a one-second sequence numbered after it; locomotion loops and moves
    /// at 200 units/s.
    struct Seqs;
    impl AnimSet for Seqs {
        fn select(&self, a: Activity, _: &[&str]) -> Option<Sequence> {
            Some(Sequence(a.0 as u32))
        }
        fn activity(&self, s: Sequence) -> Option<Activity> {
            Some(Activity(s.0 as u16))
        }
        fn duration(&self, _: Sequence) -> f32 {
            1.0
        }
        fn ground_speed(&self, s: Sequence) -> f32 {
            match s.0 as u16 {
                a if a == Activity::MP_SPRINT.0 => 150.0,
                _ if self.looping(s) => 200.0,
                _ => 0.0,
            }
        }
        fn looping(&self, s: Sequence) -> bool {
            (Activity::MP_RUN.0..=Activity::MP_SPRINT.0).contains(&(s.0 as u16))
        }
        fn pose_range(&self, _: PoseParam) -> Option<(f32, f32)> {
            Some((-90.0, 90.0))
        }
        fn events(&self, _: Sequence) -> &[(f32, u16)] {
            &[]
        }
    }

    fn settings() -> Settings {
        let stand = Stance { speed: 173.5, sprint_speed: 260.0, ..Default::default() };
        Settings {
            stances: [stand.clone(), stand.clone(), stand],
            speed_scale_side: 1.0,
            speed_scale_back: 0.9,
            sv_gravity: 750.0,
            land_dist_low: 3.0,
            land_dist_medium: 11.0,
            land_dist_high: 33.0,
            ..Default::default()
        }
    }

    fn player() -> Player {
        Player { flags: 1, ground: Some(1), up: [0.0, 0.0, 1.0], speed_scale: 1.0, ..Default::default() }
    }

    fn body() -> Body {
        Body::new(BodySettings { aim_angles: [-120.0, -60.0, 60.0, 120.0], sprint_orients_to_move: true, ..Default::default() })
    }

    fn step(b: &mut Body, p: &Player, v: V3, yaw: f32, t: f32) -> Activity {
        let s = settings();
        let f = Frame { p, s: &s, velocity: v, eye: [0.0, yaw, 0.0], curtime: t, frametime: 0.01, traversal_type: 0 };
        b.update(&f, &Seqs);
        b.activity.unwrap()
    }

    #[test]
    fn ground_activity_by_speed_and_direction() {
        let mut p = player();
        let mut b = body();
        assert_eq!(step(&mut b, &p, [0.0; 3], 0.0, 1.0), Activity::MP_STAND_IDLE);
        assert_eq!(step(&mut b, &p, [60.0, 0.0, 0.0], 0.0, 1.01), Activity::MP_WALK_FORWARD);
        assert_eq!(step(&mut b, &p, [150.0, 0.0, 0.0], 0.0, 1.02), Activity::MP_RUN_FORWARD);
        assert_eq!(step(&mut b, &p, [-150.0, 0.0, 0.0], 0.0, 1.03), Activity::MP_RUN_BACKWARD);
        assert_eq!(step(&mut b, &p, [0.0, 150.0, 0.0], 0.0, 1.04), Activity::MP_RUN_BACKWARD, "sideways stays backward: the margin");
        assert_eq!(step(&mut b, &p, [300.0, 0.0, 0.0], 0.0, 1.05), Activity::MP_SLIDE_FORWARD, "faster than sprint speed");
        p.sprinting = true;
        assert_eq!(step(&mut b, &p, [250.0, 0.0, 0.0], 0.0, 1.06), Activity::MP_SPRINT);
    }

    #[test]
    fn quad_direction_holds_inside_the_margin() {
        let p = player();
        let mut b = body();
        b.settings.quad_move_anims = true;
        let toward = |deg: f32| {
            let (s, c) = deg.to_radians().sin_cos();
            [c * 150.0, s * 150.0, 0.0]
        };
        assert_eq!(step(&mut b, &p, toward(0.0), 0.0, 1.0), Activity::MP_RUN_FORWARD);
        assert_eq!(step(&mut b, &p, toward(-65.0), 0.0, 1.01), Activity::MP_RUN_FORWARD, "within 10 of the 60 degree edge");
        assert_eq!(step(&mut b, &p, toward(-75.0), 0.0, 1.02), Activity::MP_RUN_RIGHT);
        assert_eq!(step(&mut b, &p, toward(-55.0), 0.0, 1.03), Activity::MP_RUN_RIGHT, "and back the other way");
        assert_eq!(step(&mut b, &p, toward(-45.0), 0.0, 1.04), Activity::MP_RUN_FORWARD);
    }

    #[test]
    fn jump_floats_then_lands_by_height() {
        let mut p = player();
        let mut b = body();
        step(&mut b, &p, [0.0; 3], 0.0, 1.0);
        b.event(PlayerEvent::Jump, 1.0);
        p.flags = 0;
        p.ground = None;
        p.left_ground_time = 1.0;
        assert_eq!(step(&mut b, &p, [0.0, 0.0, 300.0], 0.0, 1.01), Activity::MP_JUMP_START);
        b.playback.finished = true;
        assert_eq!(step(&mut b, &p, [0.0, 0.0, 100.0], 0.0, 1.5), Activity::MP_JUMP_FLOAT);
        p.flags = 1;
        p.ground = Some(1);
        b.landed(300.0);
        assert_eq!(step(&mut b, &p, [0.0; 3], 0.0, 2.0), Activity::MP_JUMP_LAND_LOW);
        assert_eq!(step(&mut b, &p, [0.0; 3], 0.0, 2.1), Activity::MP_JUMP_LAND_LOW, "until it has played out");
        b.playback.finished = true;
        assert_eq!(step(&mut b, &p, [0.0; 3], 0.0, 2.2), Activity::MP_STAND_IDLE);
    }

    #[test]
    fn slide_ends_by_where_the_eyes_turned() {
        let mut p = player();
        let mut b = body();
        p.sliding = true;
        assert_eq!(step(&mut b, &p, [300.0, 0.0, 0.0], 0.0, 1.0), Activity::MP_CROUCHSLIDE);
        assert_eq!(step(&mut b, &p, [250.0, 0.0, 0.0], 30.0, 1.01), Activity::MP_CROUCHSLIDE);
        p.sliding = false;
        assert_eq!(step(&mut b, &p, [100.0, 0.0, 0.0], 90.0, 1.02), Activity::MP_CROUCHSLIDE_END_LEFT, "slide yaw held from its start");
    }

    #[test]
    fn standing_body_turns_only_past_max_body_yaw() {
        let p = player();
        let mut b = body();
        step(&mut b, &p, [0.0; 3], 0.0, 1.0);
        step(&mut b, &p, [0.0; 3], 50.0, 1.01);
        assert_eq!(b.yaw, 0.0);
        assert_eq!(b.aim_yaw, 50.0);
        step(&mut b, &p, [0.0; 3], 70.0, 1.02);
        assert!(0.0 < b.yaw && b.yaw < 70.0, "turning towards the eyes, not snapping: {}", b.yaw);
    }

    #[test]
    fn playback_matches_root_motion_to_speed() {
        let mut p = player();
        let mut b = body();
        step(&mut b, &p, [150.0, 0.0, 0.0], 0.0, 1.0);
        assert_eq!(b.playback.rate, 0.75);
        p.sprinting = true;
        step(&mut b, &p, [250.0, 0.0, 0.0], 0.0, 1.01);
        assert_eq!(b.activity, Some(Activity::MP_SPRINT));
        assert_eq!(b.playback.rate, SPRINT_PLAYBACK_MAX);
    }

    #[test]
    fn crouch_pose_follows_the_duck_timer() {
        let mut p = player();
        assert_eq!(crouch_fraction(&p), 0.0);
        p.traversal_state = 1;
        p.duck_timer = 200;
        assert_eq!(crouch_fraction(&p), 0.5);
        p.traversal_state = 2;
        assert_eq!(crouch_fraction(&p), 1.0);
        p.traversal_state = 3;
        p.duck_timer = 0;
        assert_eq!(crouch_fraction(&p), 0.0);
    }
}
