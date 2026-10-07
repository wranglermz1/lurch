//! What a movement call keeps for the camera: the sprint tilt, which rolls the view when
//! turning at sprint speed, and the view punch, two damped springs that a landing kicks.

use super::Move;
use super::math::*;

/// The `sprinttilt_*` and `viewpunch_base_spring*` ConVars.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewTuning {
    pub sprint_tilt_accel: f32,
    pub sprint_tilt_max_vel: f32,
    pub sprint_tilt_turn_range: f32,
    pub punch_spring: Spring,
}

/// A view kick's fixed part and the spread of its random part, pitch, yaw, roll.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Kick {
    pub base: V3,
    pub random: V3,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spring {
    pub constant: V3,
    pub damping: V3,
}

const LANDING_KICK_MIN_SPEED: f32 = 80.0;
const LANDING_SALT: u32 = 0x7670_4c58;
const JUMP_SALT: u32 = 0x7670_4a58;
const WALL_START_SALT: u32 = 0x7657_5258;
const FEET: f32 = 12.0;

/// One frametime of a damped spring, solved exactly per axis rather than integrated.
pub fn spring_step(angle: &mut V3, vel: &mut V3, sp: &Spring, dt: f32) {
    let [a0, a1, a2] = *angle;
    let [v0, v1, v2] = *vel;
    if !(0.001 < a0 * a0 + a1 * a1 + a2 * a2 || 0.001 < v0 * v0 + v1 * v1 + v2 * v2) {
        *angle = [0.0; 3];
        *vel = [0.0; 3];
        return;
    }
    for i in 0..3 {
        let (k, c, x, v) = (sp.constant[i], sp.damping[i], angle[i], vel[i]);
        let disc = c * c - k * 4.0;
        let e = crt_expf(c * -0.5 * dt);
        let (nx, nv) = if disc > 1e-6 {
            let w = disc.sqrt();
            let b = ((c * 0.5 + w * 0.5) * x + v) / w;
            let p = crt_expf(w * -0.5 * dt) * (x - b);
            let q = crt_expf(w * 0.5 * dt) * b;
            let y = q + p;
            (y * e, (y * c - (q - p) * w) * e * -0.5)
        } else if -1e-6 <= disc {
            let g = c * 0.5 * x + v;
            let x1 = g * dt + x;
            (x1 * e, (g - x1 * c * 0.5) * e)
        } else {
            let w = (-disc).sqrt();
            let b = (x * c + (v + v)) / w;
            let t = w * 0.5 * dt;
            let (cs, sn) = (engine_cos(t), engine_sin(t + 0.0));
            let y = cs * x + sn * b;
            (y * e, (y * c - (cs * b - sn * x) * w) * e * -0.5)
        };
        angle[i] = nx;
        vel[i] = nv;
    }
}

/// Jenkins' one-at-a-time hash over the words' bytes, as the game mixes a seed with a salt.
pub fn mix(words: [u32; 3]) -> u32 {
    let mut h = 0u32;
    for b in words.iter().flat_map(|w| w.to_le_bytes()) {
        h = h.wrapping_add(b as u32);
        h = h.wrapping_add(h << 10);
        h ^= h >> 6;
    }
    h = h.wrapping_add(h << 3);
    h ^= h >> 11;
    h.wrapping_add(h << 15)
}

/// The game's uniform stream: Park and Miller's minimal standard generator.
pub fn random_float(seed: u32, lo: f32, hi: f32) -> f32 {
    let mut s = seed as i32;
    if s <= 0 {
        s = s.wrapping_neg();
        if s < 1 {
            s = 1;
        }
    }
    let mut s = s.wrapping_mul(48271).wrapping_sub((s / 44488).wrapping_mul(0x7fff_ffff));
    if s < 0 {
        s += 0x7fff_ffff;
    }
    (hi - lo) * (s as f32 * f32::from_bits(0x3000_0000)).min(f32::from_bits(0x3f7f_fffe)) + lo
}

/// A kick's three draws in [-0.5, 0.5): pitch, yaw, roll, each from the seed and its salt.
pub fn kick_draws(random_seed: u32, salt: u32) -> V3 {
    std::array::from_fn(|i| random_float(mix([random_seed, 0, salt + i as u32]), -0.5, 0.5))
}

/// How hard a landing at `fall` speed kicks, against the `viewkickFallDist*` heights.
pub fn landing_kick_scale(s: &super::Settings, fall: f32) -> f32 {
    let k = (s.sv_gravity + s.sv_gravity) * FEET;
    let lo = (k * s.viewkick_fall_dist_min).sqrt();
    let hi = (k * s.viewkick_fall_dist_max).sqrt();
    let span = hi - lo;
    let max_scale = s.viewkick_fall_dist_max_scale;
    if lo <= fall || lo <= 0.0 {
        if fall <= hi {
            if span != 0.0 { (fall - lo) / span * (max_scale - 1.0) + 1.0 } else { 1.0 }
        } else {
            max_scale
        }
    } else {
        fall / lo
    }
}

fn angle_diff(a: f32, b: f32) -> f32 {
    let d = (a - b) % 360.0;
    if a > b {
        if 180.0 <= d { d - 360.0 } else { d }
    } else if -180.0 >= d {
        d + 360.0
    } else {
        d
    }
}

impl Move<'_> {
    /// Turn rate against `sprinttilt_turnrange`, scaled by how far speed is from walking to
    /// sprinting; zero off the ground or below walking speed.
    fn sprint_tilt_target(&mut self) -> f32 {
        if !self.on_ground() || !is_world_up(self.p.up) {
            return 0.0;
        }
        let st = &self.s.stances[if self.p.stance_gate < 1 { 2 } else { ((self.p.flags >> 1) & 1) as usize }];
        let walk = self.p.speed_scale * st.speed;
        let sprint = self.p.speed_scale * st.sprint_speed;
        let v = self.mv.velocity;
        let speed2 = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
        let w = walk * 1.05;
        if w * w > speed2 {
            return 0.0;
        }
        let yaw = self.mv.angles[1];
        let turned = angle_diff(self.p.sprint_tilt_yaw, yaw);
        let turns = yaw / 360.0;
        self.p.sprint_tilt_yaw = (turns - turns.floor()) * 360.0;
        let rate = (turned / self.g.frametime / self.s.view.sprint_tilt_turn_range).max(-1.0).min(1.0);
        let fast = ((speed2.sqrt() - walk) / (sprint - walk)).max(0.0).min(1.0);
        fast * rate
    }

    /// Moves the tilt toward its target at a velocity that brakes in time to stop on it.
    pub fn sprint_tilt(&mut self) {
        if self.g.curtime == 0.0 {
            return;
        }
        let target = self.sprint_tilt_target();
        let (ft, accel, max_vel) = (self.g.frametime, self.s.view.sprint_tilt_accel, self.s.view.sprint_tilt_max_vel);
        let (f, w) = (self.p.sprint_tilt, self.p.sprint_tilt_vel);
        let d = target - f;
        let slack = (d.abs() - (w * ft).abs()).max(0.0);
        let step = accel * ft;
        let v = (1.0f32.copysign(d) * ((slack + slack) * accel).sqrt()).max(-max_vel).min(max_vel);
        let v = if v - w > step {
            step + w
        } else if -step > v - w {
            w - step
        } else {
            v
        };
        self.p.sprint_tilt_vel = v;
        let mut nf = (w + v) * 0.5 * ft + f;
        if 0.0 >= (f - target) * (nf - target) && 0.1 > v.abs() {
            nf = target;
            self.p.sprint_tilt_vel = 0.0;
        }
        self.p.sprint_tilt = nf.max(-1.0).min(1.0);
    }
}

impl Move<'_> {
    /// Runs at the start of every movement call.
    pub fn punch_springs(&mut self) {
        let (sp, dt) = (self.s.view.punch_spring, self.g.frametime);
        spring_step(&mut self.p.punch_angle, &mut self.p.punch_vel, &sp, dt);
        spring_step(&mut self.p.weapon_punch_angle, &mut self.p.weapon_punch_vel, &sp, dt);
    }

    /// The fall speed a landing this call reports, once, and clears.
    fn landed_from(&mut self) -> Option<f32> {
        if !self.on_ground() {
            return None;
        }
        let mut fall = self.p.fall_velocity;
        let u = self.p.up;
        let (x, y, z) = (-u[0], -u[1], -u[2]);
        if fall != 0.0 && (y * 0.0 + x * 0.0) - z * 1.0 != 1.0 {
            let v = self.p.prev_velocity;
            fall = y * v[1] + x * v[0] + z * v[2];
        }
        self.p.fall_velocity = 0.0;
        (LANDING_KICK_MIN_SPEED <= fall).then_some(fall)
    }

    /// Adds `shape(draws)` to the base punch's angular velocity and a fifth of it to the
    /// weapon's, whose roll is left alone unless `weapon_roll`.
    fn kick(&mut self, salt: u32, weapon_roll: bool, shape: impl Fn(V3) -> V3) {
        let Some(seed) = self.mv.random_seed else {
            self.scratch.unseeded_kick = true;
            return;
        };
        let k = shape(kick_draws(seed, salt));
        for (i, k) in k.into_iter().enumerate() {
            self.p.punch_vel[i] += k * 20.0;
            self.p.weapon_punch_vel[i] += if i < 2 || weapon_roll { k * 0.2 * 20.0 } else { 0.0 };
        }
    }

    pub fn jump_kick(&mut self) {
        self.kick(JUMP_SALT, true, |r| [r[0] + 2.0, r[1] + 0.0, r[2] * 0.5 + 0.9]);
    }

    /// On taking to a wall: yaw and roll lean with how far the wall is to the view's side.
    pub fn wall_start_kick(&mut self) {
        let facing = if self.p.precise_view { self.p.view_angles } else { self.p.eye_angles };
        let (_, r, _) = angle_vectors(facing);
        let n = self.p.up;
        let side = r[1] * n[1] + r[0] * n[0] + r[2] * n[2];
        let k = self.s.wall_start_kick;
        self.kick(WALL_START_SALT, false, |d| {
            [d[0] * k.random[0] + k.base[0], k.base[1] * side + d[1] * k.random[1], d[2] * k.random[2] + k.base[2] * side]
        });
    }

    pub fn landing_kick(&mut self) {
        let Some(fall) = self.landed_from() else { return };
        if !(0.0 <= fall) {
            return;
        }
        let scale = landing_kick_scale(self.s, fall);
        self.kick(LANDING_SALT, true, |r| [scale * (r[0] * 0.75 + 2.0), scale * (r[1] * 0.0 + 0.0), scale * (r[2] + 0.0)]);
    }
}
