//! Footsteps and landings: the timers and probes movement keeps for them, and the cues they
//! raise. The sounds themselves are the client's (`crate::sound`).

use super::cue::{Cue, Gait, LandHeight, Probe, StepSurface};
use super::math::*;
use super::Move;

const FL_ONGROUND: u32 = 1;
const FL_DUCKING: u32 = 2;
const FL_NO_STEPS: u32 = 0x30;
const MOVETYPE_LADDER: u8 = 9;
const MOVETYPE_OBSERVER: u8 = 10;
const DUCKED_STEP_MS: f32 = 100.0;
const SLOW_STEP_SPEED: f32 = 0.9;
const MIN_STEP_SPEED: f32 = 1e-4;
const IMPACT_STEP_MS: f32 = 400.0;
const IMPACT_SOFT: f32 = 0.5;
const MIN_LAND_SPEED: f32 = 80.0;
const FEET: f32 = 12.0;

impl Move<'_> {
    fn probe(&self) -> Probe {
        let (mins, maxs) = self.hull();
        Probe::below(self.scratch.start_origin, self.p.up, maxs[2] - mins[2])
    }

    pub fn gait(&self) -> Gait {
        if !is_world_up(self.p.up) {
            Gait::Wallrun
        } else if self.p.traversal_state == 2 {
            Gait::Crouch
        } else if self.p.sprinting {
            Gait::Sprint
        } else {
            Gait::Jog
        }
    }

    fn play_step(&mut self, surface: StepSurface) {
        if self.p.dead || self.p.sliding {
            return;
        }
        self.p.step_side = !self.p.step_side;
        self.cues.push(Cue::Footstep { gait: self.gait(), surface, side: self.p.step_side });
    }

    /// Runs down the footstep timer, faster the closer to full speed; when it runs out on the
    /// ground, the next step is due and its ground probe goes out.
    pub fn update_step_sound(&mut self) {
        if let Some(probe) = self.p.step_probe.take() {
            self.play_step(StepSurface::Probe(probe));
        }
        let s = self.s;
        let st = &s.stances[if self.p.stance_gate < 1 { 2 } else { ((self.p.flags >> 1) & 1) as usize }];
        let world_up = is_world_up(self.p.up);
        let v = self.mv.velocity;
        if 0.0 < self.p.step_time {
            let full = self.p.speed_scale * st.speed;
            let mut speed2 = v[1] * v[1] + v[0] * v[0];
            if !world_up {
                speed2 += v[2] * v[2];
            }
            let mut rate = 1.0;
            if speed2 < full * SLOW_STEP_SPEED * full * SLOW_STEP_SPEED {
                rate = (speed2.sqrt() / full) * 0.5 + 0.5;
            }
            let t = self.p.step_time - self.g.frametime * 1000.0 * rate;
            self.p.step_time = t;
            if t < 0.0 {
                self.p.step_time = 0.0;
            } else if 0.0 < t {
                return;
            }
        }
        let flags = self.p.flags;
        if flags & FL_NO_STEPS != 0 || matches!(self.p.movetype, MOVETYPE_LADDER | MOVETYPE_OBSERVER) {
            return;
        }
        let flat2 = v[0] * v[0] + v[1] * v[1];
        let speed = (v[2] * v[2] + flat2).sqrt();
        let along = if world_up || v[2] <= 0.0 { flat2.sqrt() } else { speed };
        if !(st.footstep_min_speed <= speed && flags & FL_ONGROUND != 0 && MIN_STEP_SPEED < along && self.p.surface.is_some()) {
            return;
        }
        let interval = if (world_up && !self.p.sprinting) || st.footstep_interval_sprint < 1 {
            st.footstep_interval
        } else {
            st.footstep_interval_sprint
        };
        let t = self.p.step_time;
        self.p.step_time = interval as f32 + t;
        if flags & FL_DUCKING != 0 {
            self.p.step_time = interval as f32 + t + DUCKED_STEP_MS;
        }
        if !self.p.dead && !self.p.sliding {
            self.p.step_probe = Some(self.probe());
        }
    }

    /// Horizontal speed lost to a wall during the move: past half of `impactSpeed` it sounds
    /// as a footstep on the last ground and restarts the step timer.
    pub fn wall_impact(&mut self, before: V3) {
        let g = GRAVITY_DIR;
        let d = before[0] * g[0] + before[1] * g[1] + before[2] * g[2];
        let b = [before[0] - g[0] * d, before[1] - g[1] * d, before[2] - g[2] * d];
        let v = self.mv.velocity;
        let d = v[0] * g[0] + v[1] * g[1] + v[2] * g[2];
        let a = [v[0] - g[0] * d, v[1] - g[1] * d, v[2] - g[2] * d];
        let lost = (b[1] * b[1] + b[0] * b[0] + b[2] * b[2]).sqrt() - (a[1] * a[1] + a[0] * a[0] + a[2] * a[2]).sqrt();
        let threshold = self.s.impact_speed;
        if !(threshold < lost || threshold * IMPACT_SOFT < lost) {
            return;
        }
        self.p.step_time = IMPACT_STEP_MS;
        if self.p.surface.is_some() {
            self.play_step(StepSurface::Ground(self.p.surface));
        }
    }

    fn fall_speed_for(&self, feet: f32) -> f32 {
        ((self.s.sv_gravity + self.s.sv_gravity) * FEET * feet).sqrt()
    }

    /// The end of the walk move: resolve the last landing's sound, then decide whether this
    /// call landed.
    pub fn land_check(&mut self) {
        if let Some(probe) = self.p.land_probe.take() {
            self.cues.push(Cue::LandSound { heavy: self.p.heavy_land_speed != 0.0, probe });
        }
        if !self.on_ground() {
            return;
        }
        if self.p.dead {
            self.p.fall_velocity = 0.0;
            return;
        }
        let mut speed = self.p.fall_velocity;
        let up = self.p.up;
        if speed != 0.0 && up[2] != 1.0 {
            let v = self.scratch.start_velocity;
            speed = -up[1] * v[1] + -up[0] * v[0] + -up[2] * v[2];
        }
        if speed < MIN_LAND_SPEED {
            self.p.fall_velocity = 0.0;
            return;
        }
        if self.p.surface.is_some() {
            self.p.land_probe = Some(self.probe());
            self.p.heavy_land_speed = if self.fall_speed_for(self.s.hard_fall_dist) <= speed { speed } else { 0.0 };
        }
        self.p.fall_velocity = 0.0;
        let height = if speed < self.fall_speed_for(self.s.land_dist_high) {
            if speed < self.fall_speed_for(self.s.land_dist_medium) {
                if self.fall_speed_for(self.s.land_dist_low) <= speed { LandHeight::Low } else { LandHeight::Normal }
            } else {
                LandHeight::Medium
            }
        } else {
            LandHeight::High
        };
        self.cues.push(Cue::Land { speed, height });
        self.p.land_speed = speed;
    }
}
