//! Deciding whether the player stands on something, and what changes when that answer does.

use super::math::*;
use super::{Move, MoveTraceInfo, Trace};

const WALKABLE: f32 = 0.7;
const LEAVE_SPEED: f32 = 140.0;
const AIRBORNE_FRICTION: f32 = 0.25;
const MOVETYPE_WALK: u8 = 2;
const MOVETYPE_LADDER: u8 = 9;
const MOVETYPE_TRAVERSE: u8 = 12;

impl Move<'_> {
    /// How much of a landing's speed survives, recovering over landSlowdownDuration.
    fn landing_slowdown(&self) -> f32 {
        let s = self.s;
        let since = self.g.curtime - self.p.land_time;
        if s.land_slowdown_duration <= since || self.p.land_height <= s.land_slowdown_height_min {
            return 1.0;
        }
        let height = if self.p.land_height < s.land_slowdown_height_max {
            (self.p.land_height - s.land_slowdown_height_min) / (s.land_slowdown_height_max - s.land_slowdown_height_min)
        } else {
            1.0
        };
        let t = (since / s.land_slowdown_duration).max(0.0).powf(s.land_slowdown_time_power);
        1.0 - (1.0 - t) * height * (1.0 - s.land_slowdown_frac)
    }

    pub fn set_ground(&mut self, t: Option<&Trace>) {
        let new = t.map(|t| t.entity).filter(|&e| e != 0);
        if self.p.ground.is_some() != new.is_some() {
            if let (Some(t), None) = (t, self.p.ground) {
                let vn = dot(self.mv.velocity, t.normal);
                let f = ((self.g.curtime - self.p.land_time) / self.s.land_slowdown_duration).clamp(0.0, 1.0);
                let h = vn * vn * 0.5 / self.s.sv_gravity;
                self.p.land_height = ((1.0 - f) * self.p.land_height).max(h);
                self.p.land_time = self.g.curtime;
                let up = self.p.up;
                let v = self.mv.velocity;
                let vu = scale(up, dot(v, up));
                let k = self.landing_slowdown();
                self.mv.velocity = add(scale(sub(v, vu), k), vu);
            }
            if new.is_none() && self.p.movetype == MOVETYPE_TRAVERSE {
                self.p.movetype = MOVETYPE_WALK;
            }
            self.p.flags = if new.is_some() { self.p.flags | 1 } else { self.p.flags & !1 };
        }
        self.p.ground = new;
        if let Some(t) = t.filter(|_| new.is_some()) {
            if self.p.movetype != MOVETYPE_LADDER {
                self.mv.velocity = reject(self.mv.velocity, self.p.up);
            }
            self.p.ground_normal = t.normal;
        } else {
            self.p.ground_normal = ZERO;
        }
    }

    fn stand_on(&mut self, t: &Trace) {
        self.set_ground(Some(t));
        self.p.surface_friction = ((self.surface)(t.surface) * 1.25).min(1.0);
        let kind = if is_world_up(self.p.up) { 0 } else { 2 };
        self.p.move_trace = MoveTraceInfo { endpos: t.endpos, normal: t.normal, kind };
    }

    /// Probe below the player and stand on what is there, or go airborne.
    pub fn categorize(&mut self, leaving_hint: bool) {
        self.p.surface_friction = 1.0;
        self.p.push = ZERO;
        let mt = self.p.movetype;
        if mt == MOVETYPE_LADDER || mt == MOVETYPE_TRAVERSE {
            return;
        }
        let up = self.p.up;
        let world_up = is_world_up(up);
        let (walking, reach) = if mt != MOVETYPE_WALK || (!self.on_ground() && world_up) {
            (false, 2.0)
        } else {
            (true, self.p.step_height)
        };
        let origin = self.mv.origin;
        let end = sub(origin, scale(up, reach));
        let vup = dot(up, self.mv.velocity);
        let leaving = (LEAVE_SPEED < vup && world_up) || (leaving_hint && (world_up || 1.0 < vup));

        let mut probe = None;
        if !leaving {
            let cached = self.scratch.probe.filter(|_| len2(sub(end, self.scratch.probe_end)) < 4.0);
            let t = match cached {
                Some(t) => t,
                None => {
                    let mut t = self.trace(origin, end);
                    if t.fraction < 1.0 && t.normal[2] < WALKABLE && self.p.ledge_origin == t.endpos {
                        t.normal = [0.0, 0.0, 1.0];
                    }
                    t
                }
            };
            let nz = dot(up, t.normal);
            let stands = t.entity != 0 && !(nz < WALKABLE && world_up);
            if stands {
                self.stand_on(&t);
            } else {
                if self.on_ground() || self.g.curtime - self.p.last_ground_time < 0.1 {
                    self.p.last_ground_time = self.g.curtime;
                }
                self.set_ground(None);
                if 0.0 < vup {
                    self.p.surface_friction = AIRBORNE_FRICTION;
                }
            }
            if walking && stands && !t.startsolid && 0.0 < t.fraction && t.fraction < 1.0 && self.g.frametime != 0.0 {
                self.mv.origin = t.endpos;
            }
            probe = Some(t);
        } else {
            self.set_ground(None);
        }
        self.p.ground_plane = match probe {
            Some(t) if self.on_ground() && !(1.0 <= t.fraction && !t.allsolid && !t.startsolid) => t.normal,
            _ => ZERO,
        };
    }
}
