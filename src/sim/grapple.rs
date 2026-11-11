//! Pathfinder's grapple (`pilot_survival_grapple`): the hook's flight, its catch, the pull and
//! letting go, all kept in the player's grapple data (`+0x2770`) and run from PlayerMove; the
//! hook catching a zipline and the grab it leads to; the first-person animation state the
//! grapple puts the player in; the suit power's refill; the view's roll toward the rope; and
//! the crosshair's in-range answer. docs/oracle.md (Grapple) maps each function here to its RVA.
//!
//! The hook that catches an entity (a `world::Body`) is parented to it and moves with it
//! (`0x61ae90` from `0x9d1b00`); nothing pulls the entity. Ours moves the rope's last point with
//! the hook: how the image's rope follows a parented hook is not read.
//!
//! Not modelled: swinging (`grapple_swing`, off in every pilot's settings); window hints,
//! no-grapple volumes, moving brush anchors, none of which the recorded maps or the test map's
//! world collision hold; status effects that slow the pull; the scripted grapple animation.

use super::buttons::{IN_DUCK, IN_TOGGLE_DUCK};
use super::cue::{Cue, GrappleAnim};
use super::math::*;
use super::{HullQuery, Move, Player, Settings, Trace};

/// ConVars read from the dump.
const GRAPPLE_SHOOTVEL: f32 = 2000.0;
const GRAPPLE_RETRACTVEL: f32 = 6000.0;
const GRAPPLE_FORCEDRETRACTVEL: f32 = 3000.0;
const GRAPPLE_FALLSPEED: f32 = 300.0;
const GRAPPLE_LIFT: f32 = 25.0;
const GRAPPLE_GRACEPERIOD: f32 = 0.25;
const GRAPPLE_PULLDELAY_HUMAN: f32 = 0.2;
const GRAPPLE_DETACHEXTRAALLOWEDLENGTH: f32 = 256.0;
const GRAPPLE_SPEEDRAMPMAX_HUMAN: f32 = 400.0;
const GRAPPLE_INITIALSLOWFRAC_HUMAN: f32 = 1.0;
const GRAPPLE_INITIALSLOWFRACVERT_HUMAN: f32 = 0.4;
const GRAPPLE_SPEEDRAMPMIN_HUMAN: f32 = 50.0;
const GRAPPLE_INITIALIMPULSE_HUMAN: f32 = 350.0;
const GRAPPLE_INITIALSPEEDMIN_HUMAN: f32 = 0.0;
const GRAPPLE_INITIALIMPULSEOFFGROUND_HUMAN: f32 = 50.0;
const GRAPPLE_DONTFIGHTGRAVITY: bool = true;
const GRAPPLE_LETGRAVITYHELPCOSANGLE: f32 = 0.8;
const GRAPPLE_GRAVITYPUSHUNDERCONTRIBUTION: f32 = 2.0;
const GRAPPLE_AROUND_OBSTACLE_ACCEL: f32 = 1000.0;
pub const GRAPPLE_JUMPFRAC: f32 = 1.0;

const MOVETYPE_WALK: u8 = 2;

/// The first-person animation state the grapple's sequences play in (`+0x2a4c`).
pub const FP_ANIM_GRAPPLE: u32 = 5;
/// The view's ease off the proxy once the grapple's animation lets it go (`0x9d5290`).
const GRAPPLE_ANIM_LERP_OUT: f32 = 0.3;

/// The hook's rays: what blocks the rope and what it can catch.
const MASK_HOOK: u32 = 0x4640_400b;
const MASK_RETRACT: u32 = 0x4640_420b;
/// Contents the hook never holds on.
const CONTENTS_NO_GRAPPLE: u32 = 0x800;
/// The crosshair's ray (`0x674cb0`): its mask and length, which the distance is measured in
/// double.
const MASK_IN_RANGE: u32 = 0x640_400b;
const IN_RANGE_RAY: f32 = 227023.36;
const IN_RANGE_RAY_F64: f64 = 227023.36344968397;

/// The grapple_* player settings, and the grapple weapon's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tuning {
    pub swing: bool,
    pub swing_type: i32,
    pub swing_gravity: f32,
    pub swing_hold_time: f32,
    pub swing_sticky: bool,
    pub attach_vertical_boost: f32,
    pub detach_vertical_boost: f32,
    pub detach_vertical_max_speed: f32,
    pub detach_gravity_time: f32,
    pub detach_gravity_scale: f32,
    pub detach_gravity_speed: f32,
    pub detach_length_min: f32,
    pub detach_length_max: f32,
    pub detach_away_speed: f32,
    pub gravity_frac_min: f32,
    pub gravity_frac_max: f32,
    pub detach_speed_loss: f32,
    pub detach_speed_loss_min: f32,
    pub detach_low_speed_threshold: f32,
    pub detach_low_speed_time: f32,
    pub detach_low_speed_wall_time: f32,
    pub detach_low_speed_ground_time: f32,
    pub detach_low_speed_swing_time: f32,
    pub impact_vertical_boost: f32,
    pub impact_vertical_max_speed: f32,
    pub speed_ramp_min: f32,
    pub speed_ramp_max: f32,
    pub speed_ramp_time: f32,
    pub swinging_speed_ramp_min: f32,
    pub swinging_speed_ramp_max: f32,
    pub swinging_speed_ramp_time: f32,
    pub accel: f32,
    pub decel: f32,
    pub air_speed_max: f32,
    pub air_accel: f32,
    pub detach_on_grapple: bool,
    pub detach_on_grapple_debounce_time: f32,
    /// How long after letting go of a zipline the hook passes through it.
    pub last_zipline_cooldown: f32,
    /// The view's roll toward the rope (`0x6851f0`): at most `roll_max` degrees, by
    /// `roll_strength`, in full from `roll_distance_max` away and `roll_view_angle_max` off it.
    pub roll_max: f32,
    pub roll_strength: f32,
    pub roll_distance_min: f32,
    pub roll_distance_max: f32,
    pub roll_view_angle_min: f32,
    pub roll_view_angle_max: f32,
    /// The suit's grapple power refills at `power_regen_rate` a second, `power_regen_delay`
    /// after the last pull started.
    pub power_regen_delay: f32,
    pub power_regen_rate: f32,
    /// `generalClass` is not a titan's (1 or 3).
    pub human: bool,
    pub weapon: Weapon,
}

/// The grapple weapon's settings (`mp_ability_grapple`), cached on the weapon entity
/// (`+0x27c4`–`+0x27d0`). Recordings do not hold the weapon; these are the weapon file's.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Weapon {
    pub power_use_rate: f32,
    pub power_required: f32,
    pub max_length: f32,
    pub max_length_vert: f32,
}

pub const WEAPON: Weapon = Weapon { power_use_rate: 0.0, power_required: 100.0, max_length: 850.0, max_length_vert: 850.0 };

/// What `0x80b320` gathers for one grapple update.
#[derive(Clone, Copy)]
struct Params {
    use_rate: f32,
    required: f32,
    low_speed: f32,
    length_max: f32,
    length_min: f32,
    max_length: f32,
    max_length_vert: f32,
}

fn ray(start: V3, end: V3, mask: u32) -> HullQuery {
    HullQuery { start, end, mins: ZERO, maxs: ZERO, mask }
}

fn max3(a: V3) -> f32 {
    let (x, y, z) = ((a[0] as f64).abs() as f32, (a[1] as f64).abs() as f32, (a[2] as f64).abs() as f32);
    let m = if x <= y { y } else { x };
    if m <= z { z } else { m }
}

impl Player {
    /// The Grapple native (`0x86d160`), which the ability's script calls with the aim when
    /// the tactical fires: shoot the hook from the eye along `dir`, or, with the hook out,
    /// let go once the debounce has passed. Runs between movement calls.
    pub fn grapple(&mut self, s: &Settings, dir: V3, curtime: f32, cues: &mut Vec<Cue>) -> bool {
        let g = &s.grapple;
        if self.grappling {
            if g.detach_on_grapple {
                let since = curtime - self.grapple_fire_time;
                let since = if 0.0 <= since { since } else { 0.0 };
                if g.detach_on_grapple_debounce_time <= since {
                    let mut v = self.prev_velocity;
                    self.grapple_detach(s, &mut v, true, curtime, cues);
                }
            }
            return false;
        }
        if dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2] == 0.0 {
            return false;
        }
        if !self.may_grapple(s) {
            return false;
        }
        self.grappling = true;
        let eye = self.eye(s, self.abs_origin, self.view_angles, curtime);
        self.grapple_count = 1;
        self.grapple_parent = None;
        self.grapple_hook_vel = [GRAPPLE_SHOOTVEL * dir[0], GRAPPLE_SHOOTVEL * dir[1], GRAPPLE_SHOOTVEL * dir[2]];
        self.grapple_points[0] = eye;
        self.grapple_attached = false;
        self.grapple_pulling = false;
        self.grapple_swinging = false;
        self.grapple_retracting = false;
        self.grapple_forced = false;
        self.grapple_touched = false;
        self.grapple_power_used = 0.0;
        self.grapple_fire_time = curtime;
        self.grapple_fast = true;
        self.grapple_swing_length = 0.0;
        self.grapple_swing_hold = -1.0;
        self.grapple_hook_at = eye;
        self.grapple_zipline = None;
        cues.push(Cue::GrappleFire);
        if !self.climb_arms && (self.first_person_anim == 0 || self.first_person_anim == FP_ANIM_GRAPPLE) {
            self.grapple_anim_play();
            cues.push(Cue::GrappleAnim(GrappleAnim::Start));
        }
        true
    }

    /// `0x86c6b0`: not traversing, not in a melee attack, in no first-person animation but the
    /// grapple's, with the suit power the weapon needs. Its other refusals (`+0xa2e`,
    /// `0x870860`, `0x870920`, `0x8709e0`) are states ours never enters.
    pub fn may_grapple(&self, s: &Settings) -> bool {
        self.movetype != super::mantle::MOVETYPE_TRAVERSE
            && !self.melee_active
            && (self.first_person_anim == 0 || self.first_person_anim == FP_ANIM_GRAPPLE)
            && s.grapple.weapon.power_required <= self.suit_grapple_power
    }

    /// `0x9d5290`, the player's side: the proxy plays a grapple sequence and becomes the
    /// view-offset entity at once, easing out over 0.3 s when let go.
    fn grapple_anim_play(&mut self) {
        if self.climb_arms {
            self.climb_arms = false;
            self.view_entity = false;
        }
        self.view_entity = true;
        self.view_entity_lerp_in = 0.0;
        self.view_entity_lerp_out = GRAPPLE_ANIM_LERP_OUT;
        self.view_entity_flag = false;
        self.first_person_anim = FP_ANIM_GRAPPLE;
    }

    /// `0x9d5650`: the grapple's animation ends, and the view lets go of the proxy.
    pub fn grapple_anim_end(&mut self, cues: &mut Vec<Cue>) {
        if self.first_person_anim != FP_ANIM_GRAPPLE {
            return;
        }
        self.first_person_anim = 0;
        self.view_entity = false;
        cues.push(Cue::GrappleAnim(GrappleAnim::End));
    }

    /// PostThink (`0x86ef40`), for the grapple's animation: it moves on once the view no longer
    /// holds the proxy, or the grapple is neither out nor reeling in (the reel-in flag stays
    /// set after the hook is back, until the next shot). It also moves on when the proxy's
    /// sequence has finished, which only the animation knows: the last one finishing ends it
    /// through `grapple_anim_end`. Ours ends at once where the oracle would first play what is
    /// queued.
    pub fn grapple_anim_think(&mut self, cues: &mut Vec<Cue>) {
        if self.first_person_anim == FP_ANIM_GRAPPLE && (!self.view_entity || !self.grappling && !self.grapple_retracting) {
            self.grapple_anim_end(cues);
        }
    }

    /// PostThink (`0x8615c0`): the suit's grapple power refills once `grapple_power_regen_delay`
    /// has passed since the last pull started.
    pub fn grapple_power_regen(&mut self, s: &Settings, curtime: f32, frametime: f32) {
        let g = &s.grapple;
        if g.power_regen_delay + self.grapple_pull_time < curtime {
            let p = g.power_regen_rate * frametime + self.suit_grapple_power;
            self.suit_grapple_power = if p <= 100.0 { p } else { 100.0 };
        }
    }

    /// `0x86bfc0`: letting go of a pull costs speed, down to a floor.
    fn grapple_speed_loss(s: &Settings, v: &mut V3) {
        let g = &s.grapple;
        let min = g.detach_speed_loss_min;
        let l2 = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
        if min * min < l2 {
            let l = l2.sqrt();
            let k = l - g.detach_speed_loss;
            let k = if min <= k { k } else { min };
            let k = k / l;
            *v = [v[0] * k, v[1] * k, v[2] * k];
        }
    }

    /// `0x86c220`: let go of the anchor; the hook then reels back in. `loss` takes the
    /// speed loss when it was pulling.
    pub fn grapple_detach(&mut self, s: &Settings, v: &mut V3, loss: bool, curtime: f32, cues: &mut Vec<Cue>) {
        if self.grapple_count == 0 {
            return;
        }
        self.grapple_parent = None;
        if loss && self.grapple_pulling {
            Self::grapple_speed_loss(s, v);
        }
        let was_attached = self.grapple_attached;
        cues.push(Cue::GrappleDetach { extract: was_attached.then_some(self.grapple_hook_at) });
        self.grapple_retracting = true;
        self.grapple_forced = true;
        self.grapple_attached = false;
        self.grapple_pulling = false;
        self.grapple_swinging = false;
        self.grapple_time = curtime;
        self.grapple_swing_length = 0.0;
        self.grapple_swing_hold = -1.0;
        if was_attached {
            self.floor_height = self.abs_origin[2];
        }
        self.floor_frozen = false;
        if self.grappling && !self.climb_arms && (self.first_person_anim == 0 || self.first_person_anim == FP_ANIM_GRAPPLE) {
            self.grapple_anim_play();
            cues.push(Cue::GrappleAnim(GrappleAnim::Stop));
        } else {
            self.grapple_anim_end(cues);
        }
    }
}

impl Move<'_> {
    pub(super) fn grapple_eye(&self) -> V3 {
        self.p.eye(self.s, self.p.abs_origin, self.p.view_angles, self.g.curtime)
    }

    fn grapple_ray(&mut self, start: V3, end: V3, mask: u32) -> Trace {
        self.tr.hull(&ray(start, end, mask))
    }

    fn grapple_hook(&self) -> V3 {
        self.p.grapple_points[(self.p.grapple_count - 1) as usize]
    }

    /// The crosshair's grapple indicator (client, `0x674cb0`), for a ray from the eye along the
    /// view: it ends on something the hook holds (a zipline it passes counts), within the
    /// weapon's reach, which shrinks by how far the player moves away from it while the hook
    /// flies there. The client asks with the ray it queued a frame earlier.
    pub fn grapple_in_range(&mut self, velocity: V3) -> bool {
        if !self.p.may_grapple(self.s) {
            return false;
        }
        let eye = self.grapple_eye();
        let f = angle_vectors(self.p.view_angles).0;
        let end = [f[0] * IN_RANGE_RAY + eye[0], f[1] * IN_RANGE_RAY + eye[1], f[2] * IN_RANGE_RAY + eye[2]];
        let mut t = self.grapple_ray(eye, end, MASK_IN_RANGE);
        let zipline = if t.startsolid { None } else { self.zipline_on_ray(&mut t) };
        if t.contents & CONTENTS_NO_GRAPPLE != 0 || (zipline.is_none() && t.contents & MASK_HOOK == 0) {
            return false;
        }
        let w = &self.s.grapple.weapon;
        let (maxl, maxv) = (w.max_length, w.max_length_vert);
        let dist = (t.fraction as f64 * IN_RANGE_RAY_F64) as f32;
        let longest = if maxl <= maxv { maxv } else { maxl };
        if !(dist < longest) {
            return false;
        }
        let k = maxl / maxv;
        let mut reach = maxl / ((k * k - 1.0) * (f[2] * f[2]) + 1.0).sqrt();
        if !(dist < reach) {
            return false;
        }
        let away = f[1] * velocity[1] + f[0] * velocity[0] + f[2] * velocity[2];
        if away < 0.0 {
            reach = (reach / GRAPPLE_SHOOTVEL) * away + reach;
        }
        dist < reach
    }

    /// `0x80b320`.
    fn grapple_params(&self) -> Params {
        let g = &self.s.grapple;
        Params {
            use_rate: g.weapon.power_use_rate,
            required: g.weapon.power_required,
            low_speed: g.detach_low_speed_threshold,
            length_max: g.detach_length_max,
            length_min: g.detach_length_min,
            max_length: g.weapon.max_length,
            max_length_vert: g.weapon.max_length_vert,
        }
    }

    /// `0x9d77e0`: the rope from just above the feet to its first point, lifted.
    fn grapple_rope(&self) -> V3 {
        let t = self.p.grapple_points[0];
        let o = self.p.abs_origin;
        let z = (self.p.step_height + 1.0) + o[2];
        [t[0] - o[0], t[1] - o[1], GRAPPLE_LIFT + (t[2] - z)]
    }

    /// The rope against a wall's normal, which a climb start refuses when positive (`0x81bb60`).
    pub(super) fn grapple_rope_along(&self, n: V3) -> f32 {
        let r = self.grapple_rope();
        (r[1] * n[1] + r[0] * n[0]) + r[2] * n[2]
    }

    /// `0x80caa0`: let go with the move's velocity, as a duck press does.
    pub fn grapple_release(&mut self) {
        let mut v = self.mv.velocity;
        if self.p.grappling {
            self.p.grapple_detach(self.s, &mut v, true, self.g.curtime, &mut self.cues);
        }
        self.mv.velocity = v;
    }

    /// Letting go through the player's own call (`0x86c5b0`): it works on the player's abs
    /// velocity, which FinishMove overwrites, so only the grapple's state changes.
    pub(super) fn grapple_drop(&mut self) {
        let mut v = self.p.prev_velocity;
        if self.p.grappling {
            self.p.grapple_detach(self.s, &mut v, true, self.g.curtime, &mut self.cues);
        }
    }

    /// `0x80c030`, from PlayerMove after the duck: the hook's flight or reel, the anchor's
    /// checks, and letting go on a duck press.
    pub fn grapple_think(&mut self) {
        if self.p.grappling && self.p.grapple_zipline.is_some() {
            self.grapple_zipline_grab();
        }
        if !self.p.grappling {
            return;
        }
        if let Some((id, held)) = self.p.grapple_parent {
            match self.tr.body(id) {
                Some(b) => {
                    let at = add(b.mins, held);
                    self.p.grapple_hook_at = at;
                    self.p.grapple_points[(self.p.grapple_count - 1) as usize] = at;
                }
                None => self.p.grapple_parent = None,
            }
        }
        let g = &self.s.grapple;
        if (!g.swing || g.swing_type != 3) && self.mv.pressed & (IN_DUCK | IN_TOGGLE_DUCK) != 0 {
            self.grapple_release();
        }
        if self.p.frozen {
            self.grapple_release();
        }
        if self.p.grapple_pulling {
            let was = self.p.grapple_swinging;
            let swing = false;
            if swing == was {
                if 0.0 <= self.p.grapple_swing_hold {
                    self.p.grapple_swing_hold = -1.0;
                }
            } else {
                let hold = self.p.grapple_swing_hold;
                if hold < 0.0 {
                    self.p.grapple_swing_hold = self.g.curtime;
                }
                if g.swing_hold_time <= self.g.curtime - self.p.grapple_swing_hold {
                    self.p.grapple_swinging = swing;
                    let v = self.mv.velocity;
                    if g.swing_sticky && 0.0 < v[2] {
                        let n = self.mv.jump_dir;
                        let d = (v[0] * n[0] + v[1] * n[1] + v[2] * n[2]) * 0.75;
                        self.mv.velocity = [v[0] - d * n[0], v[1] - d * n[1], v[2] - d * n[2]];
                    }
                }
            }
        }
        let params = self.grapple_params();
        let mut vel = self.mv.velocity;
        self.grapple_unwrap();
        let dt = self.g.frametime;
        if self.p.grapple_attached {
            self.grapple_anchored(&params, dt);
        }
        if !self.p.grapple_retracting && self.p.grapple_count <= 3 {
            if !self.p.grapple_attached {
                self.grapple_fly(&params, dt, &mut vel);
            }
        } else {
            self.grapple_retract(dt, &mut vel);
        }
        self.mv.grapple_stash = self.mv.velocity;
        self.p.floor_frozen = false;
        self.p.grappling = 0 < self.p.grapple_count;
        self.mv.velocity = vel;
        if 0.0 < vel[2] {
            self.set_ground(None);
        }
    }

    /// `0x9d3430`: drop a bend in the rope once both its sides see past it.
    fn grapple_unwrap(&mut self) {
        let mut i: i32 = 0;
        while i < self.p.grapple_count - 1 {
            let start = if i == 0 { self.grapple_eye() } else { self.p.grapple_points[(i - 1) as usize] };
            let end = self.p.grapple_points[(i + 1) as usize];
            let d = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
            let l2 = d[1] * d[1] + d[0] * d[0] + d[2] * d[2];
            let t = self.grapple_ray(start, end, MASK_HOOK);
            if t.fraction == 1.0 {
                let mid = self.p.grapple_points[i as usize];
                let f = if 1e-5 <= l2 {
                    let f = ((mid[1] * d[1] + mid[0] * d[0] + mid[2] * d[2]) - (d[1] * start[1] + start[0] * d[0] + d[2] * start[2])) / l2;
                    if 0.0 <= f { f.min(1.0) } else { 0.0 }
                } else {
                    0.0
                };
                let c = [d[0] * f + start[0], d[1] * f + start[1], d[2] * f + start[2]];
                let e = [c[0] - mid[0], c[1] - mid[1], c[2] - mid[2]];
                let el = (e[1] * e[1] + e[0] * e[0] + e[2] * e[2]).sqrt();
                let inv = 1.0 / if el <= TINY { TINY } else { el };
                let from = [mid[0] + inv * e[0] * 2.5, mid[1] + inv * e[1] * 2.5, mid[2] + inv * e[2] * 2.5];
                let t2 = self.grapple_ray(from, c, MASK_HOOK);
                if !t2.startsolid && t2.fraction == 1.0 {
                    self.p.grapple_count -= 1;
                    let mut k = i;
                    while k < self.p.grapple_count {
                        self.p.grapple_points[k as usize] = self.p.grapple_points[(k + 1) as usize];
                        k += 1;
                    }
                    i -= 1;
                }
            }
            i += 1;
        }
    }

    /// `0x9d3d20`: on the anchor. Decides when the pull starts (after `grapple_pullDelay`) and
    /// whether to let go.
    fn grapple_anchored(&mut self, params: &Params, dt: f32) {
        let now = self.g.curtime;
        let rope = self.grapple_rope();
        let mut fast = false;
        if !self.p.grapple_pulling {
            fast = true;
            self.p.grapple_fast = true;
        } else {
            if GRAPPLE_GRACEPERIOD + self.p.grapple_pull_time < now {
                let room = (params.required - self.p.grapple_power_used) - 0.001;
                let used = if room <= dt * params.use_rate { room } else { dt * params.use_rate };
                self.p.grapple_power_used += used;
                let left = self.p.suit_grapple_power - used;
                self.p.suit_grapple_power = if left < 0.0 { 0.0 } else { left };
                if !self.p.grapple_touched {
                    self.p.grapple_touched = true;
                }
            }
            if !self.p.grapple_swinging {
                let v = self.mv.velocity;
                let along = rope[0] * v[0] + v[1] * rope[1] + v[2] * rope[2];
                if 0.0 < along {
                    fast = params.low_speed * params.low_speed < (along * along) / (rope[1] * rope[1] + rope[0] * rope[0] + rope[2] * rope[2]);
                }
            }
        }
        if fast != self.p.grapple_fast {
            if !fast {
                self.p.grapple_slow_time = now;
            }
            self.p.grapple_fast = fast;
        }
        if !self.grapple_should_detach(params) {
            if self.p.grapple_pulling || now - self.p.grapple_attach_time <= GRAPPLE_PULLDELAY_HUMAN {
                return;
            }
            self.grapple_start_pull();
            return;
        }
        let mut v = self.mv.velocity;
        if !self.p.grappling {
            return;
        }
        self.p.grapple_detach(self.s, &mut v, true, now, &mut self.cues);
        self.mv.velocity = v;
    }

    /// `0x9d2c20`: too far, arrived, or `0x9d2680` says the pull is over.
    fn grapple_should_detach(&mut self, params: &Params) -> bool {
        let ca = self.p.context_action;
        if ca != 0 && 1 < ca.wrapping_sub(6) as u32 && self.p.grapple_pulling {
            return true;
        }
        let i = (self.p.grapple_count - 1).min(2);
        let p = self.p.grapple_points[i as usize];
        let o = self.p.abs_origin;
        let (dx, dz, dy) = (p[0] - o[0], p[2] - o[2], p[1] - o[1]);
        let eye = self.grapple_eye();
        let ex = p[0] - eye[0];
        let d1 = dy * dy + dx * dx + dz * dz;
        let d2 = (p[1] - eye[1]) * (p[1] - eye[1]) + ex * ex + (p[2] - eye[2]) * (p[2] - eye[2]);
        let dist = (if d1 <= d2 { d1 } else { d2 }).sqrt();
        let rope = self.grapple_rope();
        let longest = if params.max_length <= params.max_length_vert { params.max_length_vert } else { params.max_length };
        if longest + GRAPPLE_DETACHEXTRAALLOWEDLENGTH < dist {
            return true;
        }
        if dist < params.length_max {
            let arrived = dist < params.length_min || {
                let v = self.mv.velocity;
                let along = (rope[1] * v[1] + rope[0] * v[0]) + rope[2] * v[2];
                let rl = (rope[0] * rope[0] + rope[1] * rope[1] + rope[2] * rope[2]).sqrt();
                dist < ((along / rl) / GRAPPLE_SPEEDRAMPMAX_HUMAN) * 1.5 * (params.length_max - params.length_min) + params.length_min
            };
            if arrived {
                let g = &self.s.grapple;
                if self.p.grapple_pulling {
                    let max = g.impact_vertical_max_speed;
                    let vz = self.mv.velocity[2];
                    if vz < max {
                        let vz = vz + g.impact_vertical_boost;
                        self.mv.velocity[2] = if max < vz { max } else { vz };
                    }
                }
                return true;
            }
        }
        if !self.p.grapple_pulling {
            return false;
        }
        self.grapple_pull_over(p, rope)
    }

    /// `0x9d2680`: the pull ends on looking away from the anchor, on moving away from it, or
    /// after too long below `grapple_detachLowSpeedThreshold`; looking or moving away also
    /// pops the player up by `grapple_detachVerticalBoost`.
    fn grapple_pull_over(&mut self, p: V3, rope: V3) -> bool {
        let g = &self.s.grapple;
        let v = self.mv.velocity;
        let along = rope[0] * v[0] + rope[1] * v[1] + rope[2] * v[2];
        let away = !(0.0 <= along);
        let swinging = self.p.grapple_swinging;
        let mut looking_away = false;
        if !swinging {
            let f = self.mv.view_forward;
            looking_away = rope[0] * f[0] + rope[1] * f[1] + rope[2] * f[2] < 0.0;
            if f[2] < 0.0 {
                let ff = self.mv.flat_forward;
                looking_away = looking_away && rope[0] * ff[0] + rope[1] * ff[1] < 0.0;
                let eye = self.grapple_eye();
                looking_away = looking_away && !(0.0 <= (p[1] - eye[1]) * ff[1] + (p[0] - eye[0]) * ff[0]);
            }
        }
        let since = self.g.curtime - self.p.grapple_slow_time;
        let st = self.mv.grapple_stash;
        let climbing = st[2] <= v[2]
            && 0.0 < st[1] * v[1] + st[0] * v[0]
            && (st[2] < 0.0 || (st[0] * st[0] + st[1] * st[1]) * st[2] * st[2] < (v[1] * v[1] + v[0] * v[0]) * v[2] * v[2]);
        let (on_look, on_look_away) = if !looking_away || climbing { (false, false) } else { (true, away) };
        let fast_away = away && !((along * along) / (rope[0] * rope[0] + rope[1] * rope[1] + rope[2] * rope[2]) <= g.detach_away_speed * g.detach_away_speed);
        let boost = if fast_away {
            true
        } else if !on_look {
            return self.grapple_too_slow(since);
        } else {
            on_look_away || swinging
        };
        if boost {
            let max = g.detach_vertical_max_speed;
            let vz = v[2];
            if vz < max {
                let vz = vz + g.detach_vertical_boost;
                self.mv.velocity[2] = if max < vz { max } else { vz };
            }
        }
        true
    }

    /// The low-speed half of `0x9d2680`.
    fn grapple_too_slow(&self, since: f32) -> bool {
        if self.p.grapple_fast {
            return false;
        }
        let g = &self.s.grapple;
        let limit = if !self.on_ground() {
            if self.p.grapple_swinging { g.detach_low_speed_swing_time } else { g.detach_low_speed_time }
        } else if is_world_up(self.p.up) {
            g.detach_low_speed_ground_time
        } else {
            g.detach_low_speed_wall_time
        };
        limit <= since && since != limit
    }

    /// `0x9d1970`: the pull starts, lifting the player off the ground and off a zipline ridden.
    fn grapple_start_pull(&mut self) {
        self.p.grapple_pulling = true;
        self.p.grapple_swinging = false;
        if self.p.grapple_zipline.is_some() {
            let used = self.grapple_params().required;
            self.p.grapple_power_used = used;
            let left = self.p.suit_grapple_power - used;
            self.p.suit_grapple_power = if left < 0.0 { 0.0 } else { left };
        }
        self.p.grapple_pull_time = self.g.curtime;
        let mut v = self.mv.velocity;
        self.grapple_lift_off(false, &mut v);
        self.mv.velocity = v;
        if self.zipline(self.p.zipline).is_some() {
            self.detach_zipline();
        }
        self.cues.push(Cue::GrapplePull);
    }

    /// `0x9d1610`: at the pull's start the initial impulse (`0x9d7b40`); on a new anchor while
    /// pulling, only the push off the ground (`0x9d7960`). Either leaves the ground.
    fn grapple_lift_off(&mut self, again: bool, v: &mut V3) {
        if !again {
            if !self.grapple_impulse(v) {
                return;
            }
        } else {
            if !self.on_ground() {
                return;
            }
            self.grapple_off_ground(v);
        }
        if self.on_ground() {
            if self.p.movetype == super::mantle::MOVETYPE_TRAVERSE {
                self.p.movetype = MOVETYPE_WALK;
            }
            self.p.ground = None;
            self.p.flags &= !1;
        }
    }

    /// `0x9d7b40`: velocity across the rope is kept (vertical by `grapple_initialSlowFracVert`);
    /// along it, the speed is raised toward `grapple_speedRampMin` by `grapple_initialImpulse`.
    fn grapple_impulse(&mut self, v: &mut V3) -> bool {
        let r = self.grapple_rope();
        let l = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        let inv = 1.0 / if l <= TINY { TINY } else { l };
        let d = [r[0] * inv, r[1] * inv, r[2] * inv];
        let along = d[1] * v[1] + d[0] * v[0] + d[2] * v[2];
        let along = if along <= 0.0 { 0.0 } else { along };
        let x = (v[0] - d[0] * along) * GRAPPLE_INITIALSLOWFRAC_HUMAN + d[0] * along;
        let y = (v[1] - d[1] * along) * GRAPPLE_INITIALSLOWFRAC_HUMAN + d[1] * along;
        let z = (v[2] - d[2] * along) * GRAPPLE_INITIALSLOWFRACVERT_HUMAN + d[2] * along;
        *v = [x, y, z];
        let a = x * d[0] + y * d[1] + z * d[2];
        let cap = (1.0 - 0.0) * GRAPPLE_SPEEDRAMPMIN_HUMAN;
        if !(cap <= a) {
            let n = (1.0 - 0.0) * GRAPPLE_INITIALIMPULSE_HUMAN + a;
            let n = if n <= GRAPPLE_INITIALSPEEDMIN_HUMAN { GRAPPLE_INITIALSPEEDMIN_HUMAN } else { n };
            let n = if cap <= n { cap } else { n };
            let n = n - a;
            *v = [n * d[0] + v[0], n * d[1] + v[1], n * d[2] + v[2]];
        }
        *v = [v[0] + 0.0, v[1] + 0.0, v[2] + 0.0];
        self.grapple_off_ground(v)
    }

    /// `0x9d7960`: standing, the push off the ground along the up vector.
    fn grapple_off_ground(&mut self, v: &mut V3) -> bool {
        if !self.on_ground() {
            return true;
        }
        let up = self.p.up;
        let into = -(v[0] * up[0] + v[1] * up[1] + v[2] * up[2]);
        let k = if 0.0 <= into { into } else { 0.0 };
        let push = GRAPPLE_INITIALIMPULSEOFFGROUND_HUMAN * (1.0 - 0.0);
        let k = k + push;
        *v = [up[0] * k + v[0], up[1] * k + v[1], up[2] * k + v[2]];
        0.0 < push
    }

    /// `0x9d57d0`: the hook flies at `grapple_shootVel` to `grapple_maxLength`, and catches
    /// what it hits unless that is too close.
    fn grapple_fly(&mut self, params: &Params, dt: f32, vel: &mut V3) {
        let eye = self.grapple_eye();
        let hook = self.grapple_hook();
        let (dy, dx, dz) = (hook[1] - eye[1], hook[0] - eye[0], hook[2] - eye[2]);
        let cur = (dy * dy + dx * dx + dz * dz).sqrt();
        let hv = self.p.grapple_hook_vel;
        let speed = (hv[2] * hv[2] + hv[0] * hv[0] + hv[1] * hv[1]).sqrt();
        let mut step = speed * dt;
        let inv = 1.0 / if speed <= TINY { TINY } else { speed };
        let new_len = step + cur;
        let dir = [hv[0] * inv, hv[1] * inv, inv * hv[2]];
        let (maxl, maxv) = (params.max_length, params.max_length_vert);
        let shortest = if maxv <= maxl { maxv } else { maxl };
        let mut reached = false;
        if shortest <= new_len {
            let k = ((maxl / maxv) * (maxl / maxv) - 1.0) * dir[2] * dir[2] + 1.0;
            if maxl / k.sqrt() < new_len {
                step = maxl / k.sqrt() - cur;
                reached = true;
            }
        }
        if step <= 0.0 {
            self.grapple_drop();
            self.p.grapple_forced = false;
            return;
        }
        let end = [step * dir[0] + hook[0], step * dir[1] + hook[1], dir[2] * step + hook[2]];
        let mut t = self.grapple_ray(hook, end, MASK_HOOK);
        let zipline = if t.startsolid { None } else { self.zipline_on_ray(&mut t) };
        let i = (self.p.grapple_count - 1) as usize;
        self.p.grapple_points[i] = t.endpos;
        self.p.grapple_hook_at = t.endpos;
        if 1.0 <= t.fraction {
            if reached {
                self.grapple_drop();
                self.p.grapple_forced = false;
            }
            return;
        }
        let eye = self.grapple_eye();
        let h = self.grapple_hook();
        let (dy, dx, dz) = (h[1] - eye[1], h[0] - eye[0], h[2] - eye[2]);
        if dy * dy + dx * dx + dz * dz < params.length_min * params.length_min {
            self.grapple_drop();
            return;
        }
        if t.contents & CONTENTS_NO_GRAPPLE != 0 {
            self.grapple_drop();
            return;
        }
        let parent = self.tr.body(t.entity).map(|b| (b.id, sub(t.endpos, b.mins)));
        self.grapple_catch(vel, t.endpos, zipline);
        self.p.grapple_parent = parent;
    }

    /// `0x9d1b00`: the hook holds. The first catch lifts the player to at least
    /// `grapple_attachVerticalBoost`; a hook caught while reeling in starts pulling at once.
    /// Where it holds moves only the hook entity: the rope's points are the caller's. On a
    /// zipline the hook moves to the nearest point of the rope and remembers the zipline.
    fn grapple_catch(&mut self, vel: &mut V3, at: V3, zipline: Option<u32>) {
        let at = match zipline {
            Some(id) => self.zipline_nearest(id, at),
            None => at,
        };
        self.p.grapple_hook_at = at;
        if zipline.is_some() {
            self.p.grapple_zipline = zipline;
        }
        self.cues.push(Cue::GrappleCatch { at });
        let first = !self.p.grapple_attached;
        if first {
            self.p.grapple_attach_time = self.g.curtime;
        }
        let was_retracting = self.p.grapple_retracting;
        self.p.grapple_attached = true;
        self.p.grapple_retracting = false;
        if first {
            let boost = self.s.grapple.attach_vertical_boost;
            let vz = vel[2];
            let raise = if boost <= 0.0 { boost < 0.0 && !(vz < boost) } else { !(boost < vz) };
            if raise && boost != vz {
                vel[2] = boost;
            }
        }
        self.p.repel_count = 0;
        if !self.p.grapple_pulling {
            if was_retracting {
                self.grapple_start_pull();
            }
        } else {
            let mut v = self.mv.velocity;
            self.grapple_lift_off(true, &mut v);
            self.mv.velocity = v;
            self.cues.push(Cue::GrapplePull);
        }
    }

    /// `0x9d4760`: reel the hook back along the rope's bends to the eye. Reeling after a miss
    /// it falls by `grapple_fallSpeed` and can still catch; reeling past a bend it was wrapped
    /// on catches there.
    fn grapple_retract(&mut self, dt: f32, vel: &mut V3) {
        let forced = self.p.grapple_forced;
        let speed = if forced || 1 < self.p.grapple_count { GRAPPLE_FORCEDRETRACTVEL } else { GRAPPLE_RETRACTVEL };
        let mut hook = self.grapple_hook();
        let mut step = speed * dt;
        let mut target = None;
        while 1 < self.p.grapple_count {
            let prev = self.p.grapple_points[(self.p.grapple_count - 2) as usize];
            let d2 = (prev[1] - hook[1]) * (prev[1] - hook[1]) + (prev[0] - hook[0]) * (prev[0] - hook[0]) + (prev[2] - hook[2]) * (prev[2] - hook[2]);
            if step * step < d2 {
                target = Some(prev);
                break;
            }
            self.p.grapple_count -= 1;
            hook = prev;
            step -= d2.sqrt();
            if self.p.grapple_count < 4 && !forced {
                self.grapple_catch(vel, hook, None);
                return;
            }
        }
        let t = match target {
            Some(t) => t,
            None => {
                let eye = self.grapple_eye();
                if (eye[1] - hook[1]) * (eye[1] - hook[1]) + (eye[0] - hook[0]) * (eye[0] - hook[0]) + (eye[2] - hook[2]) * (eye[2] - hook[2]) <= step * step {
                    self.p.grapple_count = 0;
                    self.cues.push(Cue::GrappleReset);
                    return;
                }
                eye
            }
        };
        let d = [t[0] - hook[0], t[1] - hook[1], t[2] - hook[2]];
        let l = (d[2] * d[2] + d[1] * d[1] + d[0] * d[0]).sqrt();
        let inv = 1.0 / if l <= TINY { TINY } else { l };
        let mut new = [hook[0] + d[0] * inv * step, d[1] * inv * step + hook[1], hook[2] + d[2] * inv * step];
        if self.p.grapple_count == 1 && !forced {
            new[2] -= GRAPPLE_FALLSPEED * dt;
            let mut tr = self.grapple_ray(hook, new, MASK_RETRACT);
            let zipline = if tr.startsolid { None } else { self.zipline_on_ray(&mut tr) };
            if tr.fraction < 1.0 {
                if tr.contents & CONTENTS_NO_GRAPPLE == 0 {
                    self.grapple_catch(vel, tr.endpos, zipline);
                    return;
                }
                self.grapple_drop();
                return;
            }
        }
        let i = (self.p.grapple_count - 1) as usize;
        self.p.grapple_points[i] = new;
        self.p.grapple_hook_at = new;
    }

    /// `0x80cb20`, in the walk move after climb_think: pull along the rope; a pull that leaves
    /// the ground ends standing, and a pull into a wall is helped around it. Says whether it
    /// pulled.
    pub fn grapple_pull(&mut self) -> bool {
        let mut dir = ZERO;
        let mut pulled = false;
        if self.p.grappling && self.p.grapple_pulling {
            let mut v = self.mv.velocity;
            if self.grapple_pull_velocity(&mut v, f32::MAX, self.g.frametime) {
                pulled = true;
                let r = self.grapple_rope();
                let l = (r[1] * r[1] + r[0] * r[0] + r[2] * r[2]).sqrt();
                let inv = 1.0 / if l <= TINY { TINY } else { l };
                dir = [r[0] * inv, r[1] * inv, r[2] * inv];
            }
            self.mv.velocity = v;
        }
        if !pulled {
            return false;
        }
        if self.on_ground() && self.s.grapple.human {
            let gp = self.p.ground_plane;
            let d = dir[1] * gp[1] + dir[0] * gp[0] + dir[2] * gp[2];
            if -0.3 <= d && (is_world_up(self.p.up) || -0.001 <= d) {
                self.set_ground(None);
            }
        }
        let n = if max3(self.p.last_plane) <= 0.01 && self.on_ground() { self.p.up } else { self.p.last_plane };
        if -0.5 <= (n[0] * dir[0] + n[1] * dir[1]) + dir[2] * n[2] {
            return true;
        }
        let (h, half) = (self.mv.hull_height, self.mv.hull_half_width);
        let (o, step) = match self.mv.command_number & 3 {
            0 => ([0.0, 0.0, 1.0], h * 0.4),
            1 => ([n[1], -n[0], 0.0], half * 0.4),
            2 => ([-n[1], n[0], 0.0], half * 0.4),
            _ => ([0.0, 0.0, -1.0], h * 0.15),
        };
        for i in 1..=2 {
            let d = i as f32 * step;
            let org = self.mv.origin;
            let a = [o[0] * d + org[0], o[1] * d + org[1], o[2] * d + org[2]];
            let half = self.mv.hull_half_width;
            let b = [half * dir[0] + a[0], half * dir[1] + a[1], half * dir[2] + a[2]];
            if self.trace(a, b).fraction == 1.0 && self.trace(org, a).fraction == 1.0 {
                let k = (GRAPPLE_AROUND_OBSTACLE_ACCEL * 4.0 * self.g.frametime) / d;
                let v = self.mv.velocity;
                self.mv.velocity = [v[0] + k * o[0] * d, v[1] + k * o[1] * d, v[2] + k * o[2] * d];
                return true;
            }
        }
        true
    }

    /// `0x9d8270`: the part of the velocity across the rope decays by `grapple_decel` (what
    /// points along the view is spared, and nothing falls slower for it); along the rope it
    /// accelerates by `grapple_accel` to the ramped speed; the result never falls faster than
    /// before, up to `cap`.
    fn grapple_pull_velocity(&mut self, v: &mut V3, cap: f32, dt: f32) -> bool {
        let g = &self.s.grapple;
        let r = self.grapple_rope();
        let l = (r[1] * r[1] + r[0] * r[0] + r[2] * r[2]).sqrt();
        let inv = 1.0 / if l <= TINY { TINY } else { l };
        let d = [r[0] * inv, r[1] * inv, r[2] * inv];
        let along = d[1] * v[1] + v[0] * d[0] + v[2] * d[2];
        let mut p = [v[0] - d[0] * along, v[1] - d[1] * along, v[2] - d[2] * along];
        let mut kept = ZERO;
        let below = if GRAPPLE_DONTFIGHTGRAVITY { if 0.0 <= p[2] { 0.0 } else { p[2] } } else { 0.0 };
        kept[2] = below;
        p = [p[0] - 0.0, p[1] - 0.0, p[2] - below];
        let f = self.grapple_view_forward();
        let c = f[0] * p[0] + f[1] * p[1] + f[2] * p[2];
        if 0.0 < c {
            kept = [f[0] * c + 0.0, f[1] * c + 0.0, below + f[2] * c];
            p = [p[0] - f[0] * c, p[1] - f[1] * c, p[2] - f[2] * c];
        }
        let pl = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        let inv2 = 1.0 / if pl <= TINY { TINY } else { pl };
        let pl = pl - g.decel * dt;
        let pl = if pl <= 0.0 { 0.0 } else { pl };
        let p = [inv2 * p[0] * pl, inv2 * p[1] * pl, inv2 * p[2] * pl];
        let target = self.grapple_speed_ramp();
        let mut along = along;
        if along < target {
            let n = g.accel * dt + along;
            along = if n <= target { n } else { target };
        }
        let old_z = v[2];
        let mut z = d[2] * along + p[2] + kept[2];
        let x = d[0] * along + p[0] + kept[0];
        let y = d[1] * along + p[1] + kept[1];
        if GRAPPLE_DONTFIGHTGRAVITY {
            let m = if cap <= z { cap } else { z };
            z = if old_z <= m { m } else { old_z };
        }
        *v = [0.0 + x, 0.0 + y, 0.0 + z];
        true
    }

    /// `0x858e70`: forward from the eye angles, as `0x42c150` computes it one lane at a time.
    fn grapple_view_forward(&self) -> V3 {
        let a = if self.p.precise_view { self.p.view_angles } else { self.p.eye_angles };
        let (p, y) = (deg2rad(a[0]), deg2rad(a[1]));
        let cp = engine_cos(p);
        [cp * engine_cos(y), cp * engine_sin(y + 0.0), -engine_sin(p + 0.0)]
    }

    /// `0x9d7f60`: the pull's top speed ramps from `grapple_speedRampMin` to `…Max` over
    /// `…Time` from the catch or the pull's start, whichever is later.
    fn grapple_speed_ramp(&self) -> f32 {
        let g = &self.s.grapple;
        let (max, min, time) = if self.p.grapple_swinging {
            (g.swinging_speed_ramp_max, g.swinging_speed_ramp_min, g.swinging_speed_ramp_time)
        } else {
            (g.speed_ramp_max, g.speed_ramp_min, g.speed_ramp_time)
        };
        let from = if self.p.grapple_attach_time <= self.p.grapple_pull_time { self.p.grapple_pull_time } else { self.p.grapple_attach_time };
        let f = (self.g.curtime - from) / time;
        let f = if f <= 0.0 { 0.0 } else { f };
        let f = if 1.0 <= f { 1.0 } else { f };
        (1.0 - 0.0) * 1.0 * ((max - min) * f + min)
    }

    /// `0x808550`: gravity while pulled, from `grapple_gravityFracMin` up to `…Max` as the
    /// input pushes under the rope; full gravity when the rope hangs steeply down.
    pub fn grapple_gravity(&self) -> f32 {
        let g = &self.s.grapple;
        if self.p.grapple_swinging {
            return g.swing_gravity;
        }
        if !g.human {
            return 1.0;
        }
        let r = self.grapple_rope();
        if !(0.0 <= r[2] || (r[2] * r[2]) / (r[1] * r[1] + r[0] * r[0] + r[2] * r[2]) <= GRAPPLE_LETGRAVITYHELPCOSANGLE * GRAPPLE_LETGRAVITYHELPCOSANGLE) {
            return 1.0;
        }
        let mut frac = g.gravity_frac_min;
        let a = -(r[1] * r[2]);
        let b = r[1] * r[1] + r[0] * r[0];
        let c = -(r[0] * r[2]);
        let (fwd, mv) = (self.mv.view_forward, self.mv.forward);
        let w = self.mv.flat_wish;
        let push = -(r[1] * w[1] + r[0] * w[0]);
        let k = mv * fwd[2] * -10.0;
        let k = if k <= 0.0 { 0.0 } else { k };
        let k = if 1.0 <= k { 1.0 } else { k };
        let under = k * -(((a * fwd[1] + c * fwd[0]) + b * fwd[2]) * mv);
        if 0.0 < under || 0.0 < push {
            let under = if 0.0 <= under { under } else { 0.0 };
            let push = if 0.0 <= push { push } else { 0.0 };
            let mut t = (under / (a * a + c * c + b * b).sqrt()) * GRAPPLE_GRAVITYPUSHUNDERCONTRIBUTION;
            if 1.0 <= t {
                t = 1.0;
            }
            let ahead = push / b.sqrt();
            if t <= ahead {
                t = ahead;
            }
            frac = (g.gravity_frac_max - frac) * t + frac;
        }
        frac
    }

    /// The grapple's share of half_gravity's scale: `0x808550` while pulled, then
    /// `grapple_detachGravityScale` easing out over `…Time` after letting go, below
    /// `…Speed` up.
    pub fn grapple_gravity_scale(&self, wall: f32) -> f32 {
        let g = &self.s.grapple;
        let mut k = wall;
        if self.p.grappling && self.p.grapple_pulling {
            k *= self.grapple_gravity();
        }
        let n = self.mv.jump_dir;
        let v = self.mv.velocity;
        let up = n[1] * v[1] + n[0] * v[0] + n[2] * v[2];
        let mut d = 1.0;
        if up <= g.detach_gravity_speed {
            let since = self.g.curtime - self.p.grapple_time;
            let (time, scale) = (g.detach_gravity_time, g.detach_gravity_scale);
            if time == 0.0 {
                if since - time < 0.0 {
                    d = scale;
                }
            } else {
                let f = (since - 0.0) / (time - 0.0);
                let f = if f <= 0.0 { 0.0 } else { f };
                let f = if 1.0 <= f { 1.0 } else { f };
                d = (1.0 - scale) * f + scale;
            }
        }
        k * d
    }

    /// `0x80dd50`, at the end of PlayerMove: where the rope from the eye to its first point is
    /// blocked, it bends around the corner (up to four points; a fourth reels the hook in).
    pub fn grapple_wrap(&mut self) {
        if !self.p.grappling || self.p.grapple_count == 0 {
            return;
        }
        let first = self.p.grapple_points[0];
        let eye = self.grapple_eye();
        let t1 = self.grapple_ray(eye, first, MASK_HOOK);
        if 0.999 < t1.fraction {
            return;
        }
        let eye = self.grapple_eye();
        let t2 = self.grapple_ray(first, eye, MASK_HOOK);
        if 0.999 < t2.fraction {
            return;
        }
        if (t2.contents | t1.contents) & CONTENTS_NO_GRAPPLE != 0 {
            self.grapple_release();
        }
        let mut at = t1.endpos;
        let (n1, n2, e2) = (t1.normal, t2.normal, t2.endpos);
        let lx = n2[2] * n1[1] - n2[1] * n1[2];
        let ly = n2[0] * n1[2] - n1[0] * n2[2];
        let lz = n1[0] * n2[1] - n2[0] * n1[1];
        if 0.1 <= ly * ly + lx * lx + lz * lz {
            let (gy, gx, gz) = (e2[1] - at[1], e2[0] - at[0], e2[2] - at[2]);
            if gy * gy + gx * gx + gz * gz <= 900.0 {
                let ax = ly * n1[2] - n1[1] * lz;
                let ay = n1[0] * lz - n1[2] * lx;
                let az = n1[1] * lx - ly * n1[0];
                let k1 = (gy * n2[1] + gx * n2[0] + gz * n2[2]) / (ay * n2[1] + ax * n2[0] + az * n2[2]);
                let a = [ax * k1 + at[0], ay * k1 + at[1], az * k1 + at[2]];
                let bx = ly * n2[2] - n2[1] * lz;
                let bz = n2[1] * lx - ly * n2[0];
                let by = n2[0] * lz - n2[2] * lx;
                let k2 = ((at[0] - e2[0]) * n1[0] + (at[1] - e2[1]) * n1[1] + (at[2] - e2[2]) * n1[2]) / (n1[0] * bx + by * n1[1] + n1[2] * bz);
                let b = [e2[0] + k2 * bx, by * k2 + e2[1], bz * k2 + e2[2]];
                let p = match closest_on_lines(at, e2, a, b) {
                    Some((_, t)) => [(b[0] - a[0]) * t + a[0], (b[1] - a[1]) * t + a[1], (b[2] - a[2]) * t + a[2]],
                    None => [(b[0] + a[0]) * 0.5, (b[1] + a[1]) * 0.5, (b[2] + a[2]) * 0.5],
                };
                let eye = self.grapple_eye();
                let t3 = self.grapple_ray(eye, p, MASK_HOOK);
                let e3 = t3.endpos;
                let near = (p[1] - e3[1]) * (p[1] - e3[1]) + (p[0] - e3[0]) * (p[0] - e3[0]) + (p[2] - e3[2]) * (p[2] - e3[2]);
                let was = (p[1] - at[1]) * (p[1] - at[1]) + (p[0] - at[0]) * (p[0] - at[0]) + (p[2] - at[2]) * (p[2] - at[2]);
                if near <= was {
                    at = e3;
                }
            }
        }
        if self.p.grapple_count < 4 {
            self.p.grapple_count += 1;
        }
        let mut k = self.p.grapple_count - 1;
        while 0 < k {
            self.p.grapple_points[k as usize] = self.p.grapple_points[(k - 1) as usize];
            k -= 1;
        }
        self.p.grapple_points[0] = at;
    }
}

/// `0x42fb90`: where the lines `p1`–`p2` and `a`–`b` pass closest, as parameters along each,
/// or none when either is degenerate or they run parallel.
pub(super) fn closest_on_lines(p1: V3, p2: V3, a: V3, b: V3) -> Option<(f32, f32)> {
    let (d43y, d43x, d43z) = (b[1] - a[1], b[0] - a[0], b[2] - a[2]);
    let (d13x, d13y, d13z) = (p1[0] - a[0], p1[1] - a[1], p1[2] - a[2]);
    let big = |x: f32, y: f32, z: f32| {
        let (x, y, z) = ((x as f64).abs() as f32, (y as f64).abs() as f32, (z as f64).abs() as f32);
        let m = if x <= y { y } else { x };
        if m <= z { z } else { m }
    };
    if !(1e-6 <= big(d43x, d43y, d43z)) {
        return None;
    }
    let (d21x, d21y, d21z) = (p2[0] - p1[0], p2[1] - p1[1], p2[2] - p1[2]);
    if !(1e-6 <= big(d21x, d21y, d21z)) {
        return None;
    }
    let d1343 = d43y * d13y + d43x * d13x + d43z * d13z;
    let d4321 = d21x * d43x + d21y * d43y + d21z * d43z;
    let d4343 = d43y * d43y + d43x * d43x + d43z * d43z;
    let denom = (d21y * d21y + d21x * d21x + d21z * d21z) * d4343 - d4321 * d4321;
    if !(1e-6 <= (denom as f64).abs() as f32) {
        return None;
    }
    let s = (d4321 * d1343 - (d21y * d13y + d21x * d13x + d21z * d13z) * d4343) / denom;
    Some((s, (s * d4321 + d1343) / d4343))
}
