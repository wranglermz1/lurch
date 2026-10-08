//! Crouching and sliding.
//!
//! The duck is a timed state machine on `traversal_state`: 0 standing, 1 going down, 2 down,
//! 3 coming up. The timer counts milliseconds; a slide uses the 200 ms timing, else 400.

use super::cue::Cue;
use super::math::*;
use super::Move;

pub const IN_DUCK: u32 = 0x4;
const IN_TOGGLE_DUCK: u32 = 0x400_0000;
const DUCK_MS: i32 = 400;
const SLIDE_DUCK_MS: i32 = 200;
const UNDUCK_MS: i32 = 200;
const SLIDE_DOWNHILL_AIR: f32 = -200.0;

const STANDING: u32 = 0;
const GOING_DOWN: u32 = 1;
const DOWN: u32 = 2;
const COMING_UP: u32 = 3;

impl Move<'_> {
    fn hull_shift(&self) -> V3 {
        let p = &*self.p;
        let stand = sub(p.hull_max, p.hull_min);
        let duck = sub(p.hull_duck_max, p.hull_duck_min);
        sub(stand, duck)
    }

    /// Can the standing hull fit where a ducked player would stand up?
    fn room_to_stand(&mut self) -> bool {
        if !self.ducked() {
            return true;
        }
        if self.p.movetype == 12 {
            return false;
        }
        let mut start = self.mv.origin;
        let mut end = start;
        if !self.on_ground() {
            if self.p.last_plane[2] <= 0.0 {
                let mut d = neg(self.hull_shift());
                if self.p.last_plane[2] == 0.0 {
                    d = scale(d, 0.5);
                }
                start = add(start, d);
            }
            end = sub(start, scale(self.p.up, 2.0));
        }
        let (state, flags) = (self.p.traversal_state, self.p.flags);
        self.p.traversal_state = STANDING;
        self.p.flags &= !2;
        let t = self.trace(start, end);
        self.p.flags = flags | 2;
        self.p.traversal_state = state;
        !t.startsolid && t.fraction == 1.0
    }

    pub fn start_slide(&mut self, plane: V3, from_air: bool) {
        if plane[2] <= 0.7 && !self.s.slide_while_in_air {
            return;
        }
        let v = self.mv.velocity;
        let mut speed2 = v[1] * v[1] + v[0] * v[0] + 0.0;
        if self.p.duck_state == 3 {
            speed2 = speed2.max(0.01);
        } else {
            if !self.s.slide {
                return;
            }
            let required = if from_air && !(SLIDE_DOWNHILL_AIR < dot(v, self.mv.jump_dir)) {
                self.s.slide_required_start_speed_air
            } else {
                self.s.slide_required_start_speed
            };
            if speed2 < required * required {
                return;
            }
            let fw = self.mv.flat_wish;
            let ff = self.mv.flat_forward;
            if self.on_ground() && fw[1] * ff[1] + fw[0] * ff[0] + fw[2] * ff[2] < self.s.slide_max_angle_dot {
                return;
            }
        }
        self.p.sliding = true;
        self.p.flags |= 2;
        let since = self.g.curtime - self.p.slide_start;
        if self.s.slide_boost_enabled && self.s.slide_boost_cooldown < since {
            let speed = speed2.sqrt();
            let k = self.p.speed_scale;
            let boosted = (k * self.s.slide_speed_boost + speed).min(k * self.s.slide_speed_boost_cap);
            if boosted <= speed {
                self.p.slide_boost = 0.0;
            } else {
                self.p.slide_boost = boosted - speed;
                let r = boosted / speed;
                let v = self.mv.velocity;
                self.mv.velocity = [v[0] * r, v[1] * r, v[2]];
            }
            self.p.slide_boost_pending = true;
            self.cues.push(Cue::SlideBoost);
        }
        self.p.slide_start = self.g.curtime;
    }

    pub fn end_slide(&mut self) {
        if self.p.sliding {
            self.p.sliding = false;
            if self.s.slide_auto_stand {
                self.p.duck_held = false;
            }
        }
    }

    fn finish_unduck(&mut self) {
        self.p.flags &= !2;
        self.p.traversal_state = COMING_UP;
        self.p.ducked_in_air = false;
        if self.on_ground() || 0.0 < self.p.last_plane[2] {
            return;
        }
        let mut d = neg(self.hull_shift());
        if self.p.last_plane[2] == 0.0 {
            d = scale(d, 0.5);
        }
        self.mv.origin = add(self.mv.origin, d);
        self.p.ducked_in_air = true;
        self.categorize(false);
    }

    pub fn duck(&mut self) {
        if self.p.dead {
            return;
        }
        let was_down = self.p.traversal_state.wrapping_sub(1) < 2;
        let changed = (self.mv.oldbuttons ^ self.mv.buttons) & IN_DUCK != 0;
        if !changed && !self.p.jetpack_active {
            if self.mv.pressed() & IN_TOGGLE_DUCK != 0 && is_world_up(self.p.up) {
                self.p.duck_held = !self.p.duck_held;
            }
        } else {
            self.p.duck_held = false;
        }
        if self.p.duck_state == 2 {
            self.p.duck_held = true;
        } else if self.p.duck_state == 3 && self.on_ground() {
            self.p.duck_held = true;
            if !self.p.sliding && was_down {
                self.start_slide(self.p.ground_plane, false);
            }
        }
        let v = self.mv.velocity;
        let mut want = self.mv.buttons & IN_DUCK != 0 || self.p.duck_held;
        let stop = self.s.slide_max_stop_speed;
        if self.p.sliding && stop * stop < v[0] * v[0] + v[1] * v[1] {
            want = true;
        }
        if self.p.duck_state == 1 {
            want = false;
            self.p.duck_held = false;
        }
        if !is_world_up(self.p.up) && self.p.duck_state == 0 {
            want = false;
        }
        let mut want = self.s.crouch && want;
        let mut sliding = self.p.sliding;
        if !want && !self.room_to_stand() {
            sliding = true;
            want = true;
        }
        if !want {
            match self.p.traversal_state {
                STANDING => return self.after_duck(was_down),
                DOWN => {
                    self.p.traversal_state = COMING_UP;
                    self.p.duck_timer = UNDUCK_MS;
                    self.finish_unduck();
                }
                GOING_DOWN => {
                    self.p.traversal_state = COMING_UP;
                    let f = self.duck_fraction(self.p.sliding);
                    self.p.duck_timer = ((1.0 - f) * UNDUCK_MS as f32) as i32;
                    if self.ducked() {
                        self.finish_unduck();
                    }
                }
                _ => {}
            }
            if self.p.traversal_state == COMING_UP && self.p.duck_timer < 1 {
                self.p.traversal_state = STANDING;
            }
        } else {
            if !was_down {
                self.cues.push(Cue::Crouch);
                let plane = self.p.ground_plane;
                self.start_slide(plane, false);
                if self.p.sliding {
                    sliding = true;
                }
            }
            let full = if sliding { SLIDE_DUCK_MS } else { DUCK_MS };
            match self.p.traversal_state {
                STANDING => {
                    self.p.traversal_state = GOING_DOWN;
                    self.p.duck_timer = full;
                    self.p.ducked_in_air = !self.on_ground();
                }
                COMING_UP => {
                    self.p.traversal_state = GOING_DOWN;
                    let f = ((UNDUCK_MS - self.p.duck_timer) as f32 / UNDUCK_MS as f32).clamp(0.0, 1.0);
                    self.p.duck_timer = (full as f32 * f) as i32;
                }
                _ => {}
            }
            if self.p.traversal_state == GOING_DOWN && self.p.duck_timer < 1 {
                let was = self.p.flags;
                self.p.flags |= 2;
                self.p.traversal_state = DOWN;
                if was & 2 == 0 && self.p.ducked_in_air {
                    self.mv.origin = add(scale(self.hull_shift(), 0.5), self.mv.origin);
                }
                self.categorize(false);
            }
        }
        self.after_duck(was_down);
    }

    fn duck_fraction(&self, sliding: bool) -> f32 {
        let full = if sliding { SLIDE_DUCK_MS } else { DUCK_MS };
        (self.p.duck_timer as f32 / full as f32).clamp(0.0, 1.0)
    }

    fn after_duck(&mut self, was_down: bool) {
        if 1 < self.p.traversal_state.wrapping_sub(1) && was_down {
            self.cues.push(Cue::Stand);
            self.end_slide();
        }
    }
}
