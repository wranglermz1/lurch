//! What a movement call keeps for the camera: the sprint tilt, which rolls the view when
//! turning at sprint speed.

use super::Move;
use super::math::*;

/// The `sprinttilt_*` ConVars.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewTuning {
    pub sprint_tilt_accel: f32,
    pub sprint_tilt_max_vel: f32,
    pub sprint_tilt_turn_range: f32,
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
