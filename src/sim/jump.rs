//! Jumping, and the keyboard redirect shortly after a jump that makes lurch and tap strafe.
//!
//! Titanfall leftovers in the same code (dodge, double jump, super jump, boost, wall jump)
//! are treated as off, and the jump-through-window hint is not modelled.

use super::math::*;
use super::Move;

pub const IN_JUMP: u32 = 0x2;
const IN_FORWARD: u32 = 0x8;
const IN_BACK: u32 = 0x10;
const IN_MOVELEFT: u32 = 0x200;
const IN_MOVERIGHT: u32 = 0x400;
const IN_DODGE: u32 = 0x1000_0000;
const WALL_PUSH: f32 = 30.0;
const SLIDE_JUMP_WINDOW: f32 = 0.4;

#[derive(Default)]
pub struct Lurch {
    pub period_min: f32,
    pub period_max: f32,
    pub strength: f32,
    pub max: f32,
}

#[derive(Default)]
pub struct Skip {
    pub time: f32,
    pub height_speed: f32,
    pub height_fraction: f32,
    pub speed_retain: f32,
    pub speed_reduce: f32,
}

impl Move<'_> {
    fn grounded_for_jump(&self) -> Option<bool> {
        if self.on_ground() {
            return Some(true);
        }
        if self.g.curtime - self.p.left_ground_time < self.s.jump_grace_period && !self.p.jumped_since_ground {
            if !is_world_up(self.p.left_ground_up) {
                return None;
            }
            return Some(true);
        }
        Some(false)
    }

    /// Returns whether a jump happened.
    pub fn check_jump(&mut self) -> bool {
        if self.p.frozen || self.p.has_parent {
            return false;
        }
        if self.p.jump_suppressed {
            self.p.jump_suppressed = false;
            return false;
        }
        if self.mv.flags & 4 != 0 {
            return false;
        }
        if self.mv.pressed() & IN_JUMP == 0 {
            self.mv.oldbuttons &= !IN_JUMP;
            if self.mv.pressed() & IN_DODGE == 0 {
                return false;
            }
            return false;
        }
        let crouch_ok = self.p.sliding || self.s.can_jump_crouched;
        let Some(grounded) = self.grounded_for_jump() else { return false };
        if self.p.duck_held || self.p.duck_state == 2 {
            if !self.s.can_jump_crouched {
                self.p.duck_held = false;
            }
            if grounded && !crouch_ok {
                return false;
            }
        }
        if !grounded || (self.p.traversal_state == 3 && !crouch_ok) || !self.s.jump_enabled {
            return false;
        }
        self.p.last_ground_time = 0.0;

        if self.p.sliding && self.p.slide_boost_pending {
            let v = self.mv.velocity;
            let speed2 = v[0] * v[0] + v[1] * v[1];
            if speed2 > 0.0 {
                let speed = speed2.sqrt();
                let mut kept = speed;
                if self.p.traversal_state != 2 && self.g.curtime - SLIDE_JUMP_WINDOW < self.p.slide_start {
                    kept = (speed - self.p.slide_boost).max(0.0);
                    self.mv.velocity = scale(self.mv.velocity, kept / speed);
                }
                let cap = self.s.slide_max_jump_speed;
                if (cap * 0.85) * (cap * 0.85) < speed2 && speed2 < cap * cap {
                    self.mv.velocity = scale(self.mv.velocity, cap / kept);
                }
            }
            self.p.slide_boost_pending = false;
        }

        let up = self.p.up;
        let world = is_world_up(up);
        let mut height = if self.p.sliding { self.s.slide_jump_height } else { self.s.jump_height };
        let since_land = self.g.curtime - self.p.land_time;
        if since_land < self.s.skip.time && world {
            let v = self.mv.velocity;
            let speed2 = v[1] * v[1] + v[0] * v[0] + 0.0;
            if self.s.skip.height_speed * self.s.skip.height_speed < speed2 {
                height *= self.s.skip.height_fraction;
            }
            let retain = if self.s.skip.speed_retain < 0.0 {
                self.p.speed_scale * self.s.stances[0].sprint_speed
            } else {
                self.s.skip.speed_retain
            };
            if retain * retain < speed2 {
                let speed = speed2.sqrt();
                let inv = 1.0 / speed;
                let keep = (speed - self.s.skip.speed_reduce).max(retain);
                self.mv.velocity = [
                    keep * v[0] * inv + (v[0] - v[0]),
                    v[1] * keep * inv + (v[1] - v[1]),
                    keep * 0.0 * inv + (v[2] - 0.0),
                ];
            }
        }
        let max = self.s.anti_multi_jump_time_max;
        if since_land < max && self.p.jumped_recently {
            let min = self.s.anti_multi_jump_time_min;
            let f = if min <= since_land { (since_land - max) / (min - max) } else { 1.0 };
            height *= 1.0 - (1.0 - self.s.anti_multi_jump_height_frac) * f;
        }

        self.set_ground(None);
        self.p.jumped_since_ground = true;
        let speed = ((self.s.sv_gravity + self.s.sv_gravity) * height).sqrt();
        let surface = 1.0;
        self.mv.velocity = mad(self.mv.velocity, self.mv.jump_dir, surface * speed);
        let lp = self.p.last_plane;
        if dot(lp, self.mv.flat_wish) < 0.0 {
            self.mv.velocity = sub(self.mv.velocity, scale(lp, WALL_PUSH));
        }
        self.p.sliding = false;
        self.half_gravity(self.g.frametime);
        self.check_velocity();
        self.p.last_jump_time = self.g.curtime;
        self.p.jumped_recently = true;
        self.mv.oldbuttons |= IN_JUMP;
        true
    }

    /// Within the grace window after a jump, a newly pressed direction key turns horizontal
    /// velocity towards the held keys' direction, keeping speed.
    pub fn keyboard_redirect(&mut self) {
        if self.mv.pressed() & (IN_FORWARD | IN_BACK | IN_MOVELEFT | IN_MOVERIGHT) == 0 {
            return;
        }
        let l = &self.s.lurch;
        let since = self.g.curtime - self.p.last_jump_time;
        if !(since < l.period_max) {
            return;
        }
        let held = self.mv.buttons;
        let bit = |b: u32| if held & b != 0 { 1.0 } else { 0.0 };
        let f = bit(IN_FORWARD) - bit(IN_BACK);
        let s = bit(IN_MOVERIGHT) - bit(IN_MOVELEFT);
        let ff = self.mv.flat_forward;
        let fr = self.mv.flat_right;
        let dir = [f * ff[0] + s * fr[0], s * fr[1] + f * ff[1], s * fr[2] + f * ff[2]];
        let dlen = len(dir).max(TINY);
        let v = self.mv.velocity;
        let speed = (v[0] * v[0] + v[1] * v[1] + 0.0).sqrt();
        let vs = speed.max(TINY);
        let (vx, vy) = (v[0] * (1.0 / vs), v[1] * (1.0 / vs));
        let k = ((since - l.period_max) / (l.period_min - l.period_max)).clamp(0.0, 1.0) * l.strength;
        let mut nx = ((1.0 / dlen) * dir[0] - vx) * k + vx;
        let mut ny = ((1.0 / dlen) * dir[1] - vy) * k + vy;
        let nlen = (ny * ny + nx * nx).sqrt();
        if 1e-4 < nlen {
            let r = speed / nlen;
            nx *= r;
            ny *= r;
        }
        let mut dx = nx - v[0];
        let mut dy = ny - v[1];
        let cap = self.p.speed_scale * self.s.stances[0].sprint_speed * l.max;
        let d2 = dx * dx + dy * dy;
        if cap * cap < d2 {
            let r = cap / d2.sqrt();
            dx *= r;
            dy *= r;
        }
        self.mv.velocity = [v[0] + dx, v[1] + dy, v[2]];
    }
}
