//! Whether the player is sprinting this command.
//!
//! Weapon state (firing or aiming blocks sprint) and status effects that slow the player are
//! not modelled: both are taken as not blocking.

use super::math::*;
use super::Move;

const IN_SPEED: u32 = 0x8000;
const IN_WALK: u32 = 0x800;
const HOLD_WINDOW: f32 = 0.1;
const PRESS_WINDOW: f32 = 3.0;
const MIN_INPUT2: f32 = 0.64000005;
const SPRINT_STAMINA: f32 = 10.0;

impl Move<'_> {
    fn input(&self) -> (f32, f32) {
        (self.mv.forward, self.mv.side)
    }

    /// The input still reads as a run in a direction that allows sprinting.
    fn can_keep_sprinting(&self) -> bool {
        let (f, s) = self.input();
        let m2 = f * f + s * s;
        m2 >= MIN_INPUT2 && (m2 == 0.0 || -0.707 <= f / m2.sqrt()) && (!self.on_ground() || self.p.sliding || 1 < self.p.traversal_state.wrapping_sub(1))
    }

    fn sprint_allowed(&self) -> bool {
        let p = &*self.p;
        if !self.s.sprint
            || (!self.on_ground() && 0.25 < self.g.curtime - p.last_ground_time)
            || p.movetype != 2
            || self.mv.buttons & IN_WALK != 0
            || !is_world_up(p.up)
        {
            return false;
        }
        let (f, s) = self.input();
        let m2 = f * f + s * s;
        if !(MIN_INPUT2 <= m2 && m2 != 0.0 && 0.707 <= f / m2.sqrt()) {
            return false;
        }
        if p.sprint_blocked || 1 >= p.traversal_state.wrapping_sub(1) || p.sprint_suppressed {
            return false;
        }
        if self.landing_slowdown() < self.s.land_slowdown_no_sprint_frac {
            return false;
        }
        let (fwd, _, _) = angle_vectors(p.eye_angles);
        let v = p.prev_velocity;
        v[1] * fwd[1] + v[0] * fwd[0] > 0.0
    }

    pub fn update_sprint(&mut self) {
        let now = self.g.curtime;
        let held = self.mv.buttons;
        let pressed = self.mv.buttons & !self.mv.oldbuttons;
        let until = if pressed & IN_SPEED != 0 {
            Some(now + PRESS_WINDOW)
        } else if held & IN_SPEED != 0 {
            Some(now + HOLD_WINDOW)
        } else {
            None
        };
        if let Some(t) = until {
            if pressed & IN_SPEED != 0 && !self.p.sprint_toggle_lock {
                self.p.sprint_started_fresh = false;
            }
            self.p.sprint_until = self.p.sprint_until.max(t);
        }
        if !self.p.sprint_intent {
            if now <= self.p.sprint_until && self.can_keep_sprinting() {
                self.p.sprint_intent = true;
            }
        } else if !self.can_keep_sprinting() {
            self.p.sprint_intent = false;
            self.p.sprint_until = 0.0;
        }
        let want = (held & IN_SPEED != 0 || self.p.sprint_intent) && self.sprint_allowed();
        if self.p.sprinting != want {
            if !want {
                self.p.sprinting = false;
            } else if self.p.sprint_started_fresh || SPRINT_STAMINA <= self.p.stamina {
                self.p.sprinting = true;
                self.p.sprint_started_fresh = true;
            }
        }
    }
}
