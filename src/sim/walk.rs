//! Walking and falling: the walk movetype's move, ground acceleration and friction, air
//! acceleration and drag.

use super::math::*;
use super::{Move, Trace};

const SPRINT_INPUT: f32 = 1.25;
const MIN_GROUND_SPEED: f32 = 1.0;
const GRAVITY_DIR: V3 = [0.0, 0.0, -1.0];
const MAX_COORD: f32 = 65536.0;
const BACKUP_DECEL: f32 = 0.6;

impl Move<'_> {
    pub fn max_speed(&self) -> f32 {
        let st = self.stance();
        let speed = if self.p.sprinting { st.sprint_speed } else { st.speed };
        self.p.speed_scale * speed * 1.0
    }

    /// Half of this frame's gravity: applied before the move and again after it.
    pub fn half_gravity(&mut self, t: f32) {
        let g = if self.p.gravity == 0.0 { 1.0 } else { self.p.gravity };
        let k = if self.s.gravity_scale * g != 0.0 { self.s.gravity_scale * g } else { 1.0 };
        let dv = ((k * self.s.sv_gravity) as f64 * 0.5 * t as f64 * 1.0) as f32;
        let v = self.mv.velocity;
        self.mv.velocity = [GRAVITY_DIR[0] * dv + v[0], GRAVITY_DIR[1] * dv + v[1], v[2] + GRAVITY_DIR[2] * dv];
    }

    /// Clamp to sv_maxvelocity per axis and keep the origin inside the world.
    pub fn check_velocity(&mut self) {
        let m = self.s.sv_maxvelocity;
        for i in 0..3 {
            if !self.mv.velocity[i].is_finite() {
                self.mv.velocity[i] = 0.0;
            }
            if !self.mv.origin[i].is_finite() {
                self.mv.origin[i] = 0.0;
            }
            self.mv.origin[i] = self.mv.origin[i].clamp(-MAX_COORD, MAX_COORD);
            let v = self.mv.velocity[i];
            if v > m || -v > m {
                let k = if v > m { m / v } else { -(m / v) };
                self.mv.velocity = scale(self.mv.velocity, k);
            }
        }
    }

    pub fn full_walk_move(&mut self) {
        let dt = self.g.frametime;
        self.half_gravity(dt);
        self.check_velocity();
        self.check_jump();
        self.check_velocity();
        if self.on_ground() {
            self.ground_move(dt);
        } else {
            self.air_move(dt);
        }
        self.categorize(false);
        self.half_gravity(dt);
        if self.on_ground() {
            self.mv.velocity = reject(self.mv.velocity, self.p.up);
        }
        self.check_velocity();
    }

    fn ground_wish(&self) -> V3 {
        let up = self.p.up;
        let (f, _, _) = angle_vectors(self.mv.angles);
        let r = normalize(cross(f, up));
        let fwd_dir = cross(up, r);
        let k = if self.p.sprinting { SPRINT_INPUT } else { 1.0 };
        let (mut fwd, mut side) = (self.mv.forward * k, self.mv.side * k);
        let m2 = side * side + fwd * fwd;
        if m2 > 0.0 {
            let m = m2.sqrt();
            let shaped = if m2 <= 1.0 { (2.0 - m) * m } else { 1.0 };
            fwd *= shaped / m;
            side *= shaped / m;
        }
        side *= self.s.speed_scale_side;
        if fwd < 0.0 {
            fwd *= self.s.speed_scale_back;
        }
        let w = [
            fwd_dir[0] * fwd + r[0] * side,
            fwd_dir[1] * fwd + r[1] * side,
            fwd_dir[2] * fwd + r[2] * side,
        ];
        let w = reject(w, up);
        if !self.p.sliding {
            return w;
        }
        let v = self.mv.velocity;
        let d = dot(v, w);
        if 0.0 < d || (d < 0.0 && self.p.duck_state == 3) {
            let inv = 1.0 / (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
            [w[0] - v[0] * d * inv, w[1] - v[1] * d * inv, w[2] - v[2] * d * inv]
        } else {
            w
        }
    }

    fn ground_accel(&self) -> f32 {
        if self.p.sliding {
            return self.s.slide_accel * self.p.accel_scale * self.p.speed_scale;
        }
        let v = self.mv.velocity;
        let speed2 = v[0] * v[0] + v[1] * v[1];
        let st = self.stance();
        let k = self.p.speed_scale;
        let a = if st.sprint_acceleration >= 0.0 && (k * st.speed - 1.0).powi(2) < speed2 {
            st.sprint_acceleration
        } else {
            let low = k * st.low_speed;
            if low * low <= speed2 || st.low_acceleration < 0.0 { st.acceleration } else { st.low_acceleration }
        };
        a * self.p.accel_scale * k
    }

    fn ground_friction(&mut self, wishdir: V3, wishspeed: f32) {
        let up = self.p.up;
        let v = self.mv.velocity;
        let mut keep = scale(up, dot(up, v));
        let mut vt = sub(v, keep);
        let along = dot(vt, wishdir);
        if along > 0.0 {
            let f = (along * wishspeed / len2(vt)).min(1.0);
            let lp = self.p.last_plane;
            let protect = if dot(lp, wishdir) < 0.0 {
                let w = mad(wishdir, lp, OVERCLIP_FLOOR - dot(lp, wishdir));
                let w = normalize(w);
                scale(w, dot(w, vt).max(0.0) * f)
            } else {
                scale(wishdir, f * along)
            };
            keep = add(keep, protect);
            vt = sub(vt, protect);
        }
        let speed = len(vt);
        let st = self.stance();
        let stance_speed = self.p.speed_scale * st.speed;
        let mut decel = -1.0;
        if stance_speed * stance_speed < len2(v) {
            if st.sprint_deceleration >= 0.0 {
                decel = st.sprint_deceleration;
            } else if st.sprint_acceleration >= 0.0 {
                decel = st.sprint_acceleration;
            }
        }
        if self.p.sliding {
            decel = if self.p.duck_state == 3 {
                0.0
            } else if (!self.p.duck_held && self.mv.buttons & super::duck::IN_DUCK == 0) || dot(v, self.mv.ground_wish) < 0.0 {
                self.s.slide_want_to_stop_decel
            } else {
                self.s.slide_decel
            };
        }
        if decel < 0.0 {
            decel = st.deceleration;
            if decel < 0.0 {
                decel = st.acceleration * BACKUP_DECEL;
            }
        }
        let drop = self.ramp() * decel * self.p.accel_scale * self.p.surface_friction * self.p.friction_scale * self.g.frametime;
        let mut newspeed = (speed - drop).max(0.0);
        if self.p.sliding {
            newspeed *= self.s.slide_velocity_decay.powf(1.0 * self.g.frametime);
        }
        if std::env::var("DBG").is_ok() { eprintln!("friction seq {:x} v {:?} speed {speed} decel {decel} drop {drop} newspeed {newspeed} sliding {} duckheld {} btn {:x} dt {}", self.mv.command_number, v, self.p.sliding, self.p.duck_held, self.mv.buttons, self.g.frametime); }
        if newspeed < speed {
            self.mv.velocity = add(keep, scale(vt, newspeed / speed));
        }
    }

    fn ramp(&self) -> f32 {
        let now = self.g.curtime;
        if now >= self.p.ramp_end {
            return 1.0;
        }
        let start = self.p.ramp_end - 0.5;
        if now <= start { 0.0 } else { (now - start) * 2.0 }
    }

    fn ground_accelerate(&mut self, wishdir: V3, wishspeed: f32, accel: f32) {
        if self.p.no_accel {
            return;
        }
        let v = self.mv.velocity;
        let cap = len2(v).max(wishspeed * wishspeed);
        let add_speed = wishspeed - dot(wishdir, v);
        if add_speed <= 0.0 {
            return;
        }
        let out = add(v, scale(wishdir, add_speed.min(accel * self.g.frametime * self.p.surface_friction)));
        let l2 = len2(out);
        self.mv.velocity = if cap < l2 { scale(out, (cap / l2).sqrt()) } else { out };
    }

    pub fn ground_move(&mut self, dt: f32) {
        let up = self.p.up;
        self.mv.velocity = reject(self.mv.velocity, up);
        let wish = self.ground_wish();
        let l = len(wish);
        let wishdir = scale(wish, 1.0 / l.max(TINY));
        let wishspeed = l.min(1.0) * self.mv.maxspeed;
        let accel = self.ground_accel();
        self.ground_friction(wishdir, wishspeed);
        let vf = self.mv.velocity;
        self.ground_accelerate(wishdir, wishspeed, accel);
        if std::env::var("DBG").is_ok() { eprintln!("ground seq {:x} wish {:?} ws {wishspeed} accel {accel} after friction {:?} after accel {:?}", self.mv.command_number, wish, vf, self.mv.velocity); }

        let speed = len(self.mv.velocity);
        if self.p.sliding && speed < self.s.slide_stop_speed && self.p.duck_state != 3 {
            self.end_slide();
        }
        if speed < MIN_GROUND_SPEED {
            self.mv.velocity = ZERO;
            return;
        }
        let origin = self.mv.origin;
        let mut vel = self.mv.velocity;
        let gp = self.p.ground_plane;
        let plane = if len2(gp) != 0.0 && is_world_up(up) { Some(gp) } else { None };
        if let Some(gp) = plane {
            let vn = dot(gp, vel);
            let un = dot(up, gp);
            let along = sub(scale(vel, un), scale(up, vn));
            let k = (len2(vel) / len2(along)).min(f32::MAX).sqrt();
            vel = scale(along, k);
        }
        let mut dest = mad(origin, vel, dt);
        if let Some(gp) = plane {
            let d = dot(sub(dest, origin), gp);
            if d < OVERCLIP_FLOOR {
                dest = mad(dest, gp, OVERCLIP_FLOOR - d);
            }
        }
        self.p.last_plane = ZERO;
        let t = self.trace(origin, dest);
        self.p.move_trace = super::MoveTraceInfo { endpos: t.endpos, normal: t.normal, kind: 1 };
        if t.fraction == 1.0 {
            self.mv.origin = t.endpos;
        } else if !self.along_slope(pin_shallow(t, plane), true, Some(dest), false) {
            let o = self.mv.origin;
            if -0.03125 < dot(sub(o, t.endpos), t.normal) {
                self.p.last_plane = t.normal;
            }
        }
        self.stay_on_ground();
    }

    fn air_wish(&self) -> V3 {
        self.mv.flat_wish
    }

    fn air_accelerate(&mut self, wishdir: V3, wishspeed: f32, accel: f32, dt: f32) {
        let v = self.mv.velocity;
        let add_speed = wishspeed - dot(wishdir, v);
        if add_speed > 0.0 {
            self.mv.velocity = add(v, scale(wishdir, add_speed.min(accel * dt)));
            return;
        }
        let cap = len2(v).max(wishspeed * wishspeed);
        let ft = self.g.frametime;
        let out = add(v, scale(wishdir, ft * self.s.extra_air_accel));
        let l2 = len2(out);
        self.mv.velocity = if cap < l2 { scale(out, (cap / l2).sqrt()) } else { out };
    }

    pub fn air_move(&mut self, dt: f32) {
        let wish = self.air_wish();
        let l = len(wish);
        let mag = l.min(1.0);
        let wishdir = scale(wish, 1.0 / l.max(TINY));
        let air_speed = if self.s.air_strafe_disabled { 0.0 } else { self.p.air_speed };
        let mag = mag * air_speed;
        let accel = self.p.air_acceleration;
        let v = self.mv.velocity;
        let speed = (v[1] * v[1] + v[0] * v[0] + 0.0).sqrt();
        if speed > 0.0 {
            let drop = (speed * self.s.air_drag + self.s.air_drag_constant) * self.ramp() * speed * dt;
            let k = (speed - drop).max(0.0) / speed;
            self.mv.velocity = [(v[0] - v[0]) + k * v[0], (v[1] - v[1]) + k * v[1], (v[2] - 0.0) + k * 0.0];
        }
        let wishspeed = 1.0 * self.p.speed_scale * mag;
        if !self.p.no_accel {
            self.air_accelerate(wishdir, wishspeed, accel * self.p.accel_scale, dt);
        }
        self.keyboard_redirect();

        let v = add(self.mv.velocity, scale(self.p.push, dt));
        self.mv.velocity = v;
        let origin = self.mv.origin;
        let dest = mad(origin, v, dt);
        self.p.last_plane = ZERO;
        let step = self.p.step_height;
        if self.s.boost_enabled {
            self.p.step_height = step + step;
        }
        let t = self.trace(origin, dest);
        self.p.move_trace = super::MoveTraceInfo { endpos: t.endpos, normal: t.normal, kind: 2 };
        if t.fraction == 1.0 {
            self.mv.origin = t.endpos;
        } else {
            self.air_blocked(t, origin[2]);
        }
        self.p.step_height = step;
    }

    fn air_blocked(&mut self, t: Trace, start_z: f32) {
        let v = self.mv.velocity;
        let nz = t.normal[2];
        let mut along = self.g.curtime - self.p.last_ground_time <= 0.1
            || v[2] >= 0.0
            || 90000.0 <= v[1] * v[1] + v[0] * v[0] + 0.0
            || WALKABLE_AIR <= nz;
        if !along {
            if 0.0 < nz {
                along = (0.0078125 - dot(t.normal, v)) * nz + v[2] >= 0.0;
            } else {
                let up = self.p.up;
                let probe = self.trace(t.endpos, sub(t.endpos, scale(up, self.p.step_height)));
                along = probe.fraction < 1.0 && WALKABLE_AIR <= probe.normal[2];
            }
            if !along {
                let mut pos = self.mv.origin;
                let mut vel = self.mv.velocity;
                self.slide(&mut pos, &mut vel, self.g.frametime, Some(t), None, None);
                self.mv.origin = pos;
                self.mv.velocity = vel;
            }
        }
        if along && self.along_slope(t, false, None, false) {
            return;
        }
        let vz = self.mv.velocity[2];
        if 0.0 < nz && nz < WALKABLE_AIR && self.mv.origin[2] == start_z && 0.0 <= vz && vz <= 0.013531647 {
            let up = self.p.up;
            let mut ledge = t;
            ledge.normal = up;
            self.scratch.probe = Some(ledge);
            self.scratch.probe_end = sub(self.mv.origin, scale(up, 2.0));
            self.p.ledge_origin = self.mv.origin;
        }
        self.p.last_plane = t.normal;
    }
}

fn pin_shallow(mut t: Trace, plane: Option<V3>) -> Trace {
    if let Some(g) = plane {
        if t.normal[2] < 0.5 && 0.0 < t.normal[2] {
            t.normal = super::collide::pin_vertical(t.normal, g);
        }
    }
    t
}

const OVERCLIP_FLOOR: f32 = 0.0078125;
const WALKABLE_AIR: f32 = 0.7;
