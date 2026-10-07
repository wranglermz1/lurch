//! SetupMove: a user command and the player become the move data one movement call runs on.

use super::math::*;
use super::{MoveData, Settings};

/// What a client sends for one movement call.
#[derive(Clone, Copy, Debug, Default)]
pub struct UserCmd {
    pub command_number: u32,
    /// Pitch, yaw, roll in degrees.
    pub angles: V3,
    /// Forward, side and up input, each in [-1, 1].
    pub forward: f32,
    pub side: f32,
    pub up: f32,
    pub buttons: u32,
    pub frametime: f32,
}

fn clamp_move(v: f32) -> f32 {
    let v = if v <= -1.0 { -1.0 } else { v };
    if 1.0 <= v { 1.0 } else { v }
}

fn pitch(p: f32) -> f32 {
    let mut p = p % 360.0;
    if p > 180.0 {
        p += -360.0;
    } else if p < -180.0 {
        p += 360.0;
    }
    let p = if p <= -89.0 { -89.0 } else { p };
    if 89.0 <= p { 89.0 } else { p }
}

fn unit(v: V3) -> V3 {
    let inv = 1.0 / ((v[0] * v[0] + v[1] * v[1]) + v[2] * v[2]).sqrt().max(TINY);
    [inv * v[0], inv * v[1], inv * v[2]]
}

fn at_most_unit(v: V3) -> V3 {
    let l2 = (v[0] * v[0] + v[1] * v[1]) + v[2] * v[2];
    if 1.0 < l2 {
        let inv = 1.0 / l2.sqrt().max(TINY);
        [v[0] * inv, v[1] * inv, v[2] * inv]
    } else {
        v
    }
}

/// Builds the move data for `cmd` from where the player stands. `oldbuttons` are the
/// previous call's buttons.
pub fn setup_move(s: &Settings, cmd: &UserCmd, oldbuttons: u32, origin: V3, velocity: V3) -> MoveData {
    let angles = [pitch(cmd.angles[0]), cmd.angles[1], cmd.angles[2]];
    let (forward, side, up) = (clamp_move(cmd.forward), clamp_move(cmd.side), clamp_move(cmd.up));
    let (f, r, _) = angle_vectors(angles);
    let flat_forward = unit([f[0], f[1], 0.0]);
    let flat_right = unit([r[0], r[1], 0.0]);
    let flat_wish = at_most_unit([
        forward * flat_forward[0] + side * flat_right[0],
        forward * flat_forward[1] + side * flat_right[1],
        forward * flat_forward[2] + side * flat_right[2],
    ]);
    let jump_dir = [0.0, 0.0, 1.0];
    let d = (f[0] * 0.0 + f[1] * 0.0) + f[2];
    let gf = [f[0] - d * 0.0, f[1] - d * 0.0, f[2] - d];
    let ginv = 1.0 / ((gf[1] * gf[1] + gf[0] * gf[0]) + gf[2] * gf[2]).sqrt().max(TINY);
    let gf = [gf[0] * ginv, gf[1] * ginv, ginv * gf[2]];
    let a = jump_dir;
    let d = (a[0] * r[0] + a[1] * r[1]) + a[2] * r[2];
    let gr = [r[0] - d * a[0], r[1] - d * a[1], r[2] - d * a[2]];
    let rinv = 1.0 / ((gr[1] * gr[1] + gr[0] * gr[0]) + gr[2] * gr[2]).sqrt().max(TINY);
    let gr = [gr[0] * rinv, gr[1] * rinv, rinv * gr[2]];
    let ground_wish = at_most_unit([side * gr[0] + forward * gf[0], forward * gf[1] + side * gr[1], forward * gf[2] + side * gr[2]]);
    let st = &s.stances[0];
    MoveData {
        origin_changed: false,
        flags: 1,
        command_number: cmd.command_number,
        angles,
        buttons: cmd.buttons,
        oldbuttons,
        forward,
        side,
        up,
        view_forward: f,
        flat_forward,
        flat_right,
        flat_wish,
        ground_wish,
        jump_dir,
        maxspeed: 0.0,
        hull_height: st.hull_max[2] - st.hull_min[2],
        hull_half_width: (st.hull_max[0] - st.hull_min[0]) * 0.5,
        origin,
        velocity,
    }
}
