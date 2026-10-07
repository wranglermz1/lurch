use bevy::prelude::*;
use lurch::sim::math::angle_vectors;
use lurch_game::convert::{dir, rot, vfov};

fn close(a: Vec3, b: Vec3) -> bool {
    (a - b).abs().max_element() < 1e-5
}

#[test]
fn rotation_matches_source_angle_vectors() {
    let mut s = 0x2545_f491u32;
    let mut next = |range: f32| {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (s >> 8) as f32 / (1u32 << 24) as f32 * 2.0 * range - range
    };
    for _ in 0..10_000 {
        let a = [next(89.0), next(180.0), next(45.0)];
        let (f, r, u) = angle_vectors(a);
        let q = rot(a);
        assert!(close(q * Vec3::NEG_Z, dir(f)), "{a:?} forward");
        assert!(close(q * Vec3::X, dir(r)), "{a:?} right");
        assert!(close(q * Vec3::Y, dir(u)), "{a:?} up");
    }
}

#[test]
fn positive_roll_lowers_the_right_side() {
    assert!((rot([0.0, 0.0, 10.0]) * Vec3::X).y < 0.0);
}

#[test]
fn vertical_fov_from_the_4_3_horizontal() {
    assert!((vfov(70.0) - 55.413f32.to_radians()).abs() < 1e-4);
    assert!((vfov(90.0) - 73.740f32.to_radians()).abs() < 1e-4);
}
