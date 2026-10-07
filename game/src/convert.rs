//! Source (X forward, Y left, Z up, inches) to Bevy (X right, Y up, -Z forward, metres), at
//! the render boundary only.

use bevy::math::{EulerRot, Quat, Vec3};
use lurch::sim::math::V3;

pub const INCH: f32 = 0.0254;

pub fn dir(v: V3) -> Vec3 {
    Vec3::new(-v[1], v[2], -v[0])
}

pub fn pos(v: V3) -> Vec3 {
    dir(v) * INCH
}

/// `a` is pitch, yaw, roll in degrees; positive pitch looks down.
pub fn rot(a: V3) -> Quat {
    Quat::from_euler(EulerRot::YXZ, a[1].to_radians(), (-a[0]).to_radians(), (-a[2]).to_radians())
}

/// Bevy's vertical FOV in radians for a horizontal FOV in degrees defined at 4:3 (Hor+).
pub fn vfov(hfov43_deg: f32) -> f32 {
    2.0 * (0.75 * (hfov43_deg.to_radians() * 0.5).tan()).atan()
}
