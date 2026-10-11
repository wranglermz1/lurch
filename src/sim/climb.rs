//! Wall climbing: touching a wall while airborne makes it the ground. The player's up vector
//! turns to the wall's normal, and the ground move runs its wall branch with the climb
//! settings until the climb ends at the top, on release, or when the wall runs out.
//!
//! The code is the oracle's wallrun code; with the `wallrun` setting off only its climb
//! paths run, and only those are ported.

use super::duck::{IN_DUCK, IN_TOGGLE_DUCK};
use super::math::*;
use super::{HullQuery, Move, Trace};

/// Player settings and ConVars the climb reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Climb {
    pub enabled: bool,
    pub height: f32,
    pub final_jump_up_height: f32,
    pub speed_start: f32,
    pub speed_end: f32,
    pub accel: f32,
    pub decel: f32,
    pub horz_speed: f32,
    pub horz_accel: f32,
    pub horz_decel: f32,
    pub vertical_gain_cutoff: f32,
    pub allowed_wall_distance: f32,
    pub ceiling_limit: f32,
    pub gravity_ramp_up_time: f32,
    pub duck_causes_fall_off: bool,
    pub same_wall_allowed: bool,
    pub same_wall_height: f32,
    pub no_gravity_ramp: bool,
    pub mantle_angle_scale: f32,
    pub jump_outward_speed: f32,
    pub wall_max_speed_vertical: f32,
    pub wall_accel_vertical: f32,
    pub wall_decel_vertical: f32,
    pub wall_max_speed_horizontal: f32,
    pub wall_max_speed_horizontal_back: f32,
    pub wall_accel_horizontal: f32,
    pub wall_decel_horizontal: f32,
    pub retry_interval: f32,
    pub same_wall_dot: f32,
    pub same_wall_dist: f32,
    pub same_wall_slope: f32,
    pub min_angle_air: f32,
    pub rotate_max_rate: f32,
    pub upward_auto_push: f32,
    pub slip_start_time: f32,
    pub slip_duration: f32,
    pub no_input_slip_frac: f32,
    pub strength_loss_start: f32,
    pub strength_loss_end: f32,
    pub fall_away_speed: f32,
    pub push_away_fall_off_time: f32,
    pub avoid_wall_top_decel: f32,
    pub vertical_gain_reduction: f32,
    pub angle_change_min_cos: f32,
    pub mantle_max_angle_push: f32,
    pub mantle_max_angle_view: f32,
    pub mantle_wallrun_max_angle_view: f32,
}

const CLIMB: u32 = 3;
/// How long the view takes onto the climbing arms: 0.2 s plus the active weapon's `+0x1e3c`,
/// which is 0 for the fists in every recording.
const CLIMB_ARMS_LERP_IN: f32 = 0.2;
const NO_CLIMB_SURFACE: u32 = 0x10_0000;
const PROBE_SLACK: f32 = 0.2929;

/// The flat facing `f` points into the wall `n`, within about 45 degrees.
fn faces(f: V3, n: V3) -> bool {
    let d = n[1] * f[1] + n[0] * f[0];
    d < 0.0 && (n[0] * n[0] + n[1] * n[1]) * 0.48999998 < d * d
}

impl Move<'_> {
    fn climbing(&self) -> bool {
        self.s.climb.enabled
    }

    /// Sweep a cube from `p` into the wall `n` (flat): CLIMB when a wall is within the allowed
    /// distance, else `miss`, or 0 for a surface that cannot be climbed.
    fn wall_probe(&mut self, p: V3, reach: f32, n: V3, half: f32, out: Option<&mut V3>, miss: u32) -> u32 {
        let allowed = self.s.climb.allowed_wall_distance;
        let dist = (reach + allowed) + half * PROBE_SLACK;
        let end = [p[0] - dist * n[0], p[1] - dist * n[1], p[2]];
        let t = self.tr.hull(&HullQuery { start: p, end, mins: [-half; 3], maxs: [half; 3], mask: self.p.mask });
        if t.fraction == 1.0 {
            return miss;
        }
        if t.contents & NO_CLIMB_SURFACE != 0 {
            return 0;
        }
        let mut d = t.fraction * dist;
        if allowed < d && 0.0 < t.normal[2] && !t.startsolid {
            let c = ((t.normal[1] * n[1] + t.normal[0] * n[0]) + 0.0).max(0.0).min(1.0);
            let r = 1.0 / c;
            d = ((r - 1.0) * half + d) - (t.normal[2] * r) * reach;
        }
        if let Some(o) = out {
            *o = t.normal;
        }
        if allowed < d { miss } else { CLIMB }
    }

    /// Moving along `dir` and pushing into `n` closely enough to count as reaching for a
    /// ledge above.
    pub(super) fn reaching_for_ledge(&self, dir: V3, n: V3) -> bool {
        let w = self.mv.flat_wish;
        let into = -((w[0] * n[0] + n[1] * w[1]) + n[2] * w[2]);
        if into < 0.0 {
            return false;
        }
        let c = engine_cos(deg2rad(self.s.climb.mantle_max_angle_push));
        if into * into < ((w[0] * w[0] + w[1] * w[1]) + w[2] * w[2]) * c * c {
            return false;
        }
        let facing = -((n[1] * dir[1] + n[0] * dir[0]) + n[2] * dir[2]) * self.s.climb.mantle_angle_scale;
        if is_world_up(self.p.up) {
            return !(facing < engine_cos(deg2rad(self.s.climb.mantle_max_angle_view)));
        }
        if facing < engine_cos(deg2rad(self.s.climb.mantle_wallrun_max_angle_view)) {
            return false;
        }
        let v = self.mv.velocity;
        !((n[1] * v[1] + n[0] * v[0]) + n[2] * v[2] < -100.0)
    }

    /// What touching the wall `normal` at `point` means: CLIMB to climb it (or keep climbing),
    /// 2 or 1 when a probe at foot or head height finds no wall, 0 for nothing.
    fn classify_wall(&mut self, point: V3, normal: V3, t: Option<&Trace>, vel: V3) -> u32 {
        let s = self.s;
        let c = &s.climb;
        let up = self.p.up;
        if (self.on_ground() || self.g.curtime - self.p.last_ground_time < 0.5) && is_world_up(up) {
            return 0;
        }
        if !c.enabled || !faces(self.mv.flat_forward, normal) || c.speed_start * 0.5 < self.mv.velocity[2] {
            return 0;
        }
        if (c.vertical_gain_cutoff + self.p.floor_height) - c.final_jump_up_height < point[2] || self.p.traversal_blocked {
            return 0;
        }
        let ceiling = c.ceiling_limit.max(0.0).min(1.0);
        if 0.7 < normal[2] || normal[2] < -ceiling {
            return 0;
        }
        if self.p.grappling && self.p.grapple_attached && self.p.grapple_count > 0 && 0.0 < self.grapple_rope_along(normal) {
            return 0;
        }
        if t.is_some_and(|t| t.startsolid || t.contents & NO_CLIMB_SURFACE != 0) {
            return 0;
        }
        if self.p.traversal_state != 0 && !self.room_to_stand() && self.p.duck_state != 3 {
            return 0;
        }
        if !c.no_gravity_ramp && self.p.repel_count > 0 && self.repelled_by(normal) {
            return 0;
        }
        if is_world_up(up) {
            let l = ((vel[0] * vel[0] + vel[1] * vel[1]) + vel[2] * vel[2]).sqrt().max(TINY);
            let inv = 1.0 / l;
            let into = ((vel[1] * inv) * normal[1] + (vel[0] * inv) * normal[0]) + (vel[2] * inv) * normal[2];
            if engine_sin(deg2rad(c.min_angle_air * 0.5) + HALF_PI) * -1.0 <= into {
                return 0;
            }
        }
        let o = self.mv.origin;
        let behind = ((o[1] - point[1]) * normal[1] + (o[0] - point[0]) * normal[0]) + (o[2] - point[2]) * normal[2];
        let far = behind < -0.01;
        let mut base = if far { o } else { point };
        if self.p.traversal_state != 0 && self.ducked() && !self.on_ground() && self.p.last_plane[2] <= 0.0 {
            let mut d = neg(self.hull_shift());
            if self.p.last_plane[2] == 0.0 {
                d = scale(d, 0.5);
            }
            base = add(base, d);
        }
        let half = self.mv.hull_half_width;
        let heights = [base[2] + 16.0, (self.mv.hull_height * 0.5 + half) + base[2]];
        let mut all_floor = true;
        for (i, z) in heights.into_iter().enumerate() {
            let mut n = ZERO;
            let r = self.wall_probe([base[0], base[1], z], z - base[2], normal, half, Some(&mut n), if i == 0 { 2 } else { 1 });
            if r != CLIMB {
                return r;
            }
            if n[2] < 0.7 {
                all_floor = false;
            }
        }
        if far && all_floor {
            return 0;
        }
        if is_world_up(up) {
            let yaw = deg2rad(self.mv.angles[1]);
            let dir = [engine_cos(yaw), engine_sin(yaw), 0.0];
            if self.reaching_for_ledge(dir, normal) {
                let h = self.mv.hull_height;
                let r = self.wall_probe([base[0], base[1], (h - half) + base[2]], h, normal, half, None, 0);
                if r != CLIMB {
                    return r;
                }
            }
        }
        CLIMB
    }

    fn repelled_by(&self, normal: V3) -> bool {
        let s = self.s;
        let c = &s.climb;
        let l = (normal[0] * normal[0] + normal[1] * normal[1] + 0.0).sqrt().max(TINY);
        let inv = 1.0 / l;
        let e = self.p.repel_normal;
        if !(c.same_wall_dot < (normal[1] * inv) * e[1] + (normal[0] * inv) * e[0] + (inv * 0.0) * e[2]) {
            return false;
        }
        if !c.same_wall_allowed {
            return true;
        }
        if c.same_wall_height < 0.0 {
            return false;
        }
        let o = self.mv.origin;
        let floor = self.p.wall_start[2] + c.same_wall_height;
        if o[2] < floor {
            return false;
        }
        let (dx, dy) = (o[0] - self.p.wall_start[0], o[1] - self.p.wall_start[1]);
        let d2 = dy * dy + dx * dx;
        let dist = c.same_wall_dist;
        d2 <= dist * dist || ((d2.sqrt() - dist) * c.same_wall_slope <= o[2] - floor)
    }

    /// The oracle's wall contact handler, called wherever a move is blocked: starts a climb,
    /// keeps the up vector on the wall during one, or ends one on a floor.
    pub fn wall_contact(&mut self, point: V3, normal: V3, t: Option<&Trace>, vel: V3) {
        self.p.retry_time = 0.0;
        let r = self.classify_wall(point, normal, t, vel);
        if r == CLIMB {
            if is_world_up(self.p.up) {
                self.start_climb(normal);
            } else {
                self.turn_up_toward(normal);
            }
            return;
        }
        let nz = normal[2];
        if r != 0 && !self.on_ground() && nz <= 0.7 {
            self.p.retry_time = self.s.climb.retry_interval + self.g.curtime;
            self.p.retry_origin = self.mv.origin;
            self.p.retry_normal = normal;
        }
        if nz <= 0.7 {
            if !is_world_up(self.p.up) && self.mv.velocity[2] <= 140.0 && r & !2 == 0 {
                let o = self.mv.origin;
                let step = self.p.step_height;
                let below = self.trace(o, [o[0] + step * 0.0, o[1] + step * 0.0, o[2] + step * -1.0]);
                if below.fraction < 1.0 && 0.7 < below.normal[2] {
                    self.end_climb();
                }
            }
        } else {
            let o = self.mv.origin;
            if 0.0 <= ((o[1] - point[1]) * normal[1] + (o[0] - point[0]) * normal[0]) + (o[2] - point[2]) * normal[2] {
                self.end_climb();
            }
        }
    }

    fn start_climb(&mut self, normal: V3) {
        self.p.up = normal;
        self.p.wall_count += 1;
        if self.p.repel_count > 0 {
            let e = self.p.repel_normal;
            if self.s.climb.same_wall_dot < (normal[1] * e[1] + normal[0] * e[0]) + normal[2] * e[2] {
                self.p.same_wall = true;
            }
        }
        if 0.25 < self.g.curtime - self.p.wall_time {
            self.wall_start_kick();
        }
        self.p.wall_start = self.mv.origin;
        self.p.wall_time = self.g.curtime;
        self.p.push_away_since = 0.0;
        self.p.hanging = false;
        self.p.dodge_lock = false;
        self.p.dodged = false;
        self.p.duck_held = false;
        if self.p.sliding {
            self.p.sliding = false;
            if self.s.slide_auto_stand {
                self.p.duck_held = false;
            }
        }
        if self.climbing() {
            self.climb_arms();
        }
    }

    /// The climbing arms (`0x866880`, with `traversal_anim` 1): while a ray from the eye along
    /// the wall's normal finds the wall within the allowed distance, the first-person proxy
    /// plays its climb and is the view-offset entity; once it does not, they stop.
    pub(super) fn climb_arms(&mut self) {
        if self.p.traversal_blocked || self.p.first_person_anim == super::grapple::FP_ANIM_GRAPPLE {
            return;
        }
        let eye = self.p.eye(self.s, self.p.abs_origin, self.p.view_angles, self.g.curtime);
        let (mins, maxs) = self.hull();
        let allowed = self.s.climb.allowed_wall_distance;
        let (dy, dx) = (maxs[1] - mins[1], maxs[0] - mins[0]);
        let dist = (dy * dy + dx * dx + 0.0).sqrt() * 0.5 + allowed;
        let u = self.p.up;
        let end = [-(dist * u[0]) + eye[0], -(dist * u[1]) + eye[1], -(dist * u[2]) + eye[2]];
        let t = self.tr.hull(&HullQuery { start: eye, end, mins: [0.0; 3], maxs: [0.0; 3], mask: self.p.mask });
        if !(t.fraction * dist <= allowed) {
            self.stop_climb_arms();
            return;
        }
        if !self.p.climb_arms {
            self.p.view_entity = true;
            self.p.view_entity_lerp_in = CLIMB_ARMS_LERP_IN;
            self.p.view_entity_lerp_out = 0.0;
            self.p.view_entity_flag = false;
            self.p.climb_arms = true;
        }
    }

    /// `0x866e00`: the arms stop and the view lets go of them.
    fn stop_climb_arms(&mut self) {
        if self.p.climb_arms {
            self.p.climb_arms = false;
            self.p.view_entity = false;
        }
    }

    /// Turn the up vector toward `n`, at most `rotate_max_rate` radians a second.
    fn turn_up_toward(&mut self, n: V3) {
        let a = self.s.climb.rotate_max_rate * self.g.frametime;
        let (ca, sa) = (engine_sin(a + HALF_PI), engine_sin(a + 0.0));
        let u = self.p.up;
        if ca < (u[0] * n[0] + u[1] * n[1]) + u[2] * n[2] {
            self.p.up = n;
            return;
        }
        let c = [u[1] * n[2] - u[2] * n[1], u[2] * n[0] - u[0] * n[2], u[0] * n[1] - u[1] * n[0]];
        let mut d = [u[2] * c[1] - u[1] * c[2], u[0] * c[2] - u[2] * c[0], u[1] * c[0] - u[0] * c[1]];
        if (d[1] * d[1] + d[0] * d[0]) + d[2] * d[2] == 0.0 {
            if (n[0] * n[0] + n[1] * n[1]) + n[2] * n[2] == 0.0 {
                return;
            }
            d = [u[1], -u[0], 0.0];
        }
        let k = sa / ((d[0] * d[0] + d[1] * d[1]) + d[2] * d[2]).sqrt();
        let x = u[0] * ca + d[0] * k;
        let y = u[1] * ca + d[1] * k;
        let z = u[2] * ca + d[2] * k;
        let inv = 1.0 / ((y * y + x * x) + z * z).sqrt().max(TINY);
        self.p.up = [x * inv, y * inv, z * inv];
    }

    pub fn end_climb(&mut self) {
        if !is_world_up(self.p.up) {
            self.p.wall_leave_time = self.g.curtime;
        }
        self.p.up = [-0.0, -0.0, 1.0];
        self.p.prev_up = self.p.up;
        self.p.hanging = false;
        self.p.push_away_since = 0.0;
        self.stop_climb_arms();
    }

    /// How hard the wall still holds: 0 on the wall just left or high above the floor.
    fn wall_strength(&self) -> f32 {
        let s = self.s;
        let c = &s.climb;
        if self.p.same_wall {
            return 0.0;
        }
        if 0.0 <= c.vertical_gain_cutoff && c.vertical_gain_cutoff <= self.p.abs_origin[2] - self.p.floor_height {
            return 0.0;
        }
        let n = self.p.wall_count as f32;
        if n <= c.strength_loss_start {
            1.0
        } else if c.strength_loss_end <= n {
            0.0
        } else {
            (n - c.strength_loss_start) / (c.strength_loss_end - c.strength_loss_start)
        }
    }

    /// Gravity's share on a wall, growing over `gravity_ramp_up_time` after the climb began;
    /// none while pushing into the wall.
    pub fn wall_gravity(&self, since: f32) -> f32 {
        let s = self.s;
        let c = &s.climb;
        if c.gravity_ramp_up_time < 0.0 {
            return 0.0;
        }
        let w = self.mv.flat_wish;
        let u = self.p.up;
        if self.climbing() && (u[1] * w[1] + u[0] * w[0]) + u[2] * w[2] < -0.1 {
            return 0.0;
        }
        let ramp = if c.no_gravity_ramp { 100.0 } else { c.gravity_ramp_up_time };
        let t = since.max(0.0);
        if t < ramp { 1.0 * (1.0 - self.wall_strength() * (1.0 - t / ramp)) } else { 1.0 }
    }

    /// The highest the climb may take the origin, less `height`.
    fn climb_top(&self) -> f32 {
        let s = self.s;
        let c = &s.climb;
        let z = self.p.wall_start[2];
        if 0.0 <= c.vertical_gain_cutoff {
            let lim = ((c.vertical_gain_cutoff + self.p.floor_height) - c.final_jump_up_height) - c.height;
            if lim <= z {
                return lim;
            }
        }
        z
    }

    fn remember_wall(&mut self, n: V3) {
        let l = (n[0] * n[0] + n[1] * n[1] + 0.0).sqrt().max(TINY);
        let inv = 1.0 / l;
        self.p.repel_normal = [n[0] * inv, n[1] * inv, inv * 0.0];
        self.p.repel_time = self.g.curtime;
        self.p.repel_count = 1;
    }

    /// Every call during a climb: decide whether it goes on, and let go if not.
    pub fn climb_think(&mut self) {
        self.p.prev_up = self.p.up;
        if is_world_up(self.p.up) {
            return;
        }
        let s = self.s;
        let c = &s.climb;
        let mut fall = false;
        let mut top = false;
        if c.duck_causes_fall_off {
            fall = self.mv.buttons & (IN_DUCK | IN_TOGGLE_DUCK) != 0;
            if fall {
                self.p.jumped_since_ground = true;
            }
            if (self.p.duck_state as u32).wrapping_sub(2) < 2 {
                fall = true;
            }
        }
        let climbing = self.climbing();
        if climbing {
            if !faces(self.mv.flat_forward, self.p.up) {
                fall = true;
            }
            if self.mv.velocity[2] < 10.0 && 1.0 < self.g.curtime - self.p.wall_time {
                fall = true;
            }
            if self.climb_top() + c.height <= self.mv.origin[2] {
                top = true;
                fall = true;
            }
        }
        let u = self.p.up;
        let w = self.mv.flat_wish;
        if 0.7 < (u[1] * w[1] + u[0] * w[0]) + u[2] * w[2] {
            if self.p.push_away_since == 0.0 {
                self.p.push_away_since = self.g.curtime;
            }
            if c.push_away_fall_off_time < self.g.curtime - self.p.push_away_since {
                fall = true;
            }
        } else {
            self.p.push_away_since = 0.0;
        }
        let a = self.p.abs_origin;
        let half = self.mv.hull_half_width;
        match (self.mv.command_number as i32) % 4 {
            0 => {
                if self.wall_probe([a[0], a[1], a[2] + 16.0], 16.0, u, half, None, 0) != CLIMB {
                    fall = true;
                }
            }
            2 if 0.0 < self.mv.velocity[2] && self.wall_probe([a[0], a[1], a[2] + 32.0], 16.0, u, half, None, 0) != CLIMB => {
                let v = self.mv.velocity;
                self.mv.velocity = [v[0], v[1], (v[2] - self.g.frametime * 3.0 * c.avoid_wall_top_decel).max(0.0)];
            }
            _ => {}
        }
        if fall {
            let away = c.fall_away_speed;
            let v = self.mv.velocity;
            self.mv.velocity = [v[0] + u[0] * away, v[1] + u[1] * away, v[2] + u[2] * away];
            self.remember_wall(u);
            self.end_climb();
            self.p.same_wall = true;
            self.set_ground(None);
            if top {
                let d = (self.climb_top() + self.s.climb.height - self.mv.origin[2]).min(10.0) + self.s.climb.final_jump_up_height;
                if 0.0 < d {
                    let g = self.s.sv_gravity;
                    let v = self.mv.velocity;
                    self.mv.velocity = [v[0] + 0.0, v[1] + 0.0, ((g + g) * d).sqrt() + v[2]];
                }
            }
            if climbing {
                self.p.floor_height -= self.s.climb.vertical_gain_reduction;
            }
        }
        self.p.prev_up = self.p.up;
    }

    /// The ground move's input on a wall: forward and side keep their own axes, and
    /// forward input pushes up along the wall. Returns the wish and the input it came from.
    pub fn wall_wish(&self, fd: V3, r: V3) -> (V3, f32, f32) {
        let u = self.p.up;
        let (mut fwd, mut side) = (self.mv.forward, self.mv.side);
        let inv = 1.0 / ((u[1] * u[1] + -u[0] * -u[0]) + 0.0).sqrt().max(TINY);
        let h = [u[1] * inv, inv * -u[0], inv * 0.0];
        let d = (h[1] * fd[1] + h[0] * fd[0]) + h[2] * fd[2];
        let up_push = fwd.max(0.0) * d * self.s.climb.upward_auto_push;
        let along = (d * side).max(0.0) * fwd;
        side += up_push;
        fwd += along;
        if (r[2] < -0.01 && 0.0 < side) || (0.01 < r[2] && side < 0.0) {
            side = 0.0;
        }
        let w = [fd[0] * fwd + r[0] * side, fd[1] * fwd + r[1] * side, fd[2] * fwd + r[2] * side];
        (reject(w, u), fwd, side)
    }

    /// The ground move's acceleration on a wall: vertical and horizontal each accelerate
    /// toward their own top speed and decelerate beyond it.
    pub fn wall_accelerate(&mut self, w: V3, fwd: f32, side: f32) {
        let s = self.s;
        let c = &s.climb;
        let hang = 1.0;
        let since = self.g.curtime - self.p.wall_time;
        let mut slip = ((since - c.slip_start_time) / c.slip_duration).max(0.0).min(1.0);
        if fwd.abs() < 0.1 && side.abs() < 0.1 {
            slip = slip.max(c.no_input_slip_frac);
        }
        let k = self.p.wall_decel_scale;
        let (g, vmax, vaccel, vdecel, hmax, hmax_back, haccel, hdecel) = if self.climbing() && 0.0 < w[2] {
            let f = (((self.climb_top() + c.height) - self.mv.origin[2]) / c.height).max(0.0).min(1.0);
            let vmax = (1.0 - f) * c.speed_end + f * c.speed_start;
            self.climb_arms();
            (0.0, vmax, c.accel, c.decel, c.horz_speed, c.horz_speed, c.horz_accel, c.horz_decel)
        } else {
            let g = self.wall_gravity(since);
            let vmax = (1.0 - g) * c.wall_max_speed_vertical;
            (g, vmax, c.wall_accel_vertical, k * c.wall_decel_vertical, c.wall_max_speed_horizontal, c.wall_max_speed_horizontal_back, c.wall_accel_horizontal, k * c.wall_decel_horizontal)
        };
        let ramp = self.ramp() * self.p.surface_friction;
        let hdecel = hdecel * ramp;
        let mut vstep = ramp * vdecel * self.g.frametime;
        let a = self.mv.jump_dir;
        let shed = |v: V3, n: f32| [v[0] - a[0] * n, v[1] - a[1] * n, v[2] - a[2] * n];
        let v = self.mv.velocity;
        if !(0.0001 < w[2].abs()) {
            let vn = (a[0] * v[0] + v[1] * a[1]) + v[2] * a[2];
            if vn < 0.0 {
                vstep *= (1.0 - g) * (1.0 - slip);
            }
            let n = if vstep > vn.abs() { vn } else { 1.0f32.copysign(vn) * vstep };
            self.mv.velocity = shed(v, n);
        } else {
            let mag = (w[2] * w[2] + 0.0).sqrt();
            let m1 = 1.0f32.min(mag);
            let inv = 1.0 / mag.max(TINY);
            let dir = [inv * 0.0, inv * 0.0, w[2] * inv];
            let speed = if c.no_gravity_ramp { 0.0 } else { m1 * (1.0 * self.p.speed_scale * vmax * hang) };
            let vd = (dir[0] * v[0] + dir[1] * v[1]) + dir[2] * v[2];
            let decel = if vd < 0.0 {
                vstep *= (1.0 - g) * (1.0 - slip);
                true
            } else {
                speed < vd
            };
            if decel {
                self.mv.velocity = shed(v, 1.0f32.copysign(vd) * vstep);
            }
            self.ground_accelerate(dir, speed, (1.0 - slip) * vaccel);
        }
        let hstep = if c.no_gravity_ramp { 100.0 } else { hdecel * self.g.frametime };
        let hw2 = (w[0] * w[0] + w[1] * w[1]) + 0.0;
        let v = self.mv.velocity;
        if !(0.0001 < hw2) {
            let d = (a[0] * v[0] + a[1] * v[1]) + a[2] * v[2];
            let mut h = [v[0] - a[0] * d, v[1] - a[1] * d, v[2] - a[2] * d];
            let l = ((h[1] * h[1] + h[0] * h[0]) + h[2] * h[2]).sqrt();
            if hstep < l {
                let k = hstep / l;
                h = [k * h[0], k * h[1], k * h[2]];
            }
            self.mv.velocity = [v[0] - h[0], v[1] - h[1], v[2] - h[2]];
            return;
        }
        let hm = hw2.sqrt();
        let inv = 1.0 / hm.max(TINY);
        let hd = [w[0] * inv, w[1] * inv, inv * 0.0];
        let mut speed = 0.0;
        if !c.no_gravity_ramp {
            let f = (self.mv.view_forward[1] * hd[1] + self.mv.view_forward[0] * hd[0]) + hd[2] * 0.0;
            speed = 1.0f32.min(hm) * if f <= -0.6 { hmax_back } else { hmax };
        }
        let scale = 1.0 * self.p.speed_scale;
        let speed = speed * (scale * hang);
        let inv = 1.0 / ((hd[1] * hd[1] + hd[0] * hd[0]) + 0.0).sqrt().max(TINY);
        let n = [hd[0] * inv, hd[1] * inv, inv * 0.0];
        let vh = (v[1] * n[1] + v[0] * n[0]) + v[2] * n[2];
        if vh < 0.0 || speed < vh {
            let s = 1.0f32.copysign(vh) * hstep;
            self.mv.velocity = [v[0] - s * n[0], v[1] - s * n[1], v[2] - s * n[2]];
        }
        self.ground_accelerate(hd, speed, haccel * self.p.accel_scale * scale);
    }

    /// Jumping off a wall: away from it and up by `final_jump_up_height`, scaled by how hard
    /// the wall still holds.
    pub fn wall_jump(&mut self) {
        let s = self.s;
        let c = &s.climb;
        let w = self.mv.flat_wish;
        let g = self.s.sv_gravity;
        let up_speed = ((g + g) * c.final_jump_up_height).sqrt();
        self.p.floor_height -= c.vertical_gain_reduction;
        let u = self.p.up;
        let ff = self.mv.flat_forward;
        let look = ((u[1] * ff[1] + u[0] * ff[0]) + u[2] * ff[2]).min(0.0);
        let push = ((w[0] * u[0] + u[1] * w[1]) + u[2] * w[2]).min(0.0);
        let out = self.s.climb.jump_outward_speed * (1.0 - look * push * 0.8);
        let mut add = [w[0] * 0.0, w[1] * 0.0, w[2] * 0.0];
        let along = (add[0] * u[0] + add[1] * u[1]) + u[2] * add[2];
        if along < out {
            let k = out - along;
            add = [u[0] * k + add[0], u[1] * k + add[1], u[2] * k + add[2]];
        }
        self.remember_wall(u);
        let v = self.mv.velocity;
        self.mv.velocity = [v[0] + add[0], v[1] + add[1], v[2] + add[2]];
        self.p.same_wall = false;
        let s = up_speed * self.wall_strength();
        let vz = self.mv.velocity[2];
        if vz < s {
            let boosted = vz + s * 1.5;
            self.mv.velocity[2] = if s < boosted { s } else { boosted };
        }
    }
}

impl super::Player {
    /// Before each call: the floor height drops with the player and rises on floors only.
    pub fn update_floor_height(&mut self) {
        if self.floor_frozen {
            return;
        }
        let z = self.abs_origin[2];
        if z < self.floor_height {
            self.floor_height = z;
            return;
        }
        if self.floor_height == z || self.ground.is_none() || !is_world_up(self.up) || self.hanging {
            return;
        }
        let g = self.ground_normal;
        if !(((g[0] * 0.0 + g[1] * 0.0) + g[2]) < 0.7) {
            self.floor_height = z;
        }
    }
}
