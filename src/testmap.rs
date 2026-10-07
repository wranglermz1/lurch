//! A plain map built in code: a large floor and, in lanes off it, walls, steps and ramps in
//! sizes either side of the limits the settings set, crates, and a long hill to slide down.
//! Each lane has a spot to start from, facing its features.

use crate::sim::math::V3;
use crate::world::Tri;

pub struct Spot {
    pub name: String,
    pub origin: V3,
    pub yaw: f32,
}

pub struct TestMap {
    pub tris: Vec<Tri>,
    pub spots: Vec<Spot>,
}

impl TestMap {
    pub fn spot(&self, name: &str) -> &Spot {
        self.spots.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no spot {name}"))
    }
}

const LANE: f32 = 512.0;
const HALF_WIDTH: f32 = 128.0;
/// Where each lane's first feature starts along x; its spot is at x = 0.
pub const FEATURE_X: f32 = 384.0;
const PLATFORM: f32 = 256.0;
const FLOOR: f32 = 16384.0;

pub const WALL_HEIGHTS: [f32; 8] = [48.0, 96.0, 128.0, 160.0, 192.0, 256.0, 384.0, 1024.0];
pub const STEP_HEIGHTS: [f32; 6] = [8.0, 16.0, 22.0, 23.0, 32.0, 48.0];
pub const RAMP_DEGREES: [f32; 6] = [15.0, 30.0, 45.0, 46.0, 50.0, 60.0];
pub const RAMP_RISE: f32 = 128.0;
const HILL_DEGREES: f32 = 15.0;
const HILL_RISE: f32 = 512.0;
const CRATE: f32 = 64.0;

fn quad(t: &mut Vec<Tri>, a: V3, b: V3, c: V3, d: V3) {
    t.push([a, b, c]);
    t.push([a, c, d]);
}

/// An axis-aligned box on the floor, without the face against it.
fn cuboid(t: &mut Vec<Tri>, lo: V3, hi: V3) {
    let [x0, y0, z0] = lo;
    let [x1, y1, z1] = hi;
    quad(t, [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]);
    quad(t, [x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]);
    quad(t, [x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]);
    quad(t, [x0, y0, z0], [x0, y1, z0], [x0, y1, z1], [x0, y0, z1]);
    quad(t, [x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]);
}

/// A solid under a plane rising along x from height `z0` at `x0` to `z1` at `x1`.
fn ramp(t: &mut Vec<Tri>, x0: f32, x1: f32, y0: f32, y1: f32, z0: f32, z1: f32) {
    quad(t, [x0, y0, z0], [x1, y0, z1], [x1, y1, z1], [x0, y1, z0]);
    for y in [y0, y1] {
        quad(t, [x0, y, 0.0], [x1, y, 0.0], [x1, y, z1], [x0, y, z0]);
    }
    for (x, z) in [(x0, z0), (x1, z1)] {
        if z > 0.0 {
            quad(t, [x, y0, 0.0], [x, y1, 0.0], [x, y1, z], [x, y0, z]);
        }
    }
}

pub fn build() -> TestMap {
    let mut tris = Vec::new();
    let mut spots = Vec::new();
    let f = FLOOR;
    quad(&mut tris, [-f, -f, 0.0], [f, -f, 0.0], [f, f, 0.0], [-f, f, 0.0]);
    let mut lane = 0.0;
    let mut next = |name: String, origin_z: f32, spots: &mut Vec<Spot>| {
        let y = lane;
        lane += LANE;
        spots.push(Spot { name, origin: [0.0, y, origin_z], yaw: 0.0 });
        (y - HALF_WIDTH, y + HALF_WIDTH)
    };
    for h in WALL_HEIGHTS {
        let (y0, y1) = next(format!("wall {h}"), 0.0, &mut spots);
        cuboid(&mut tris, [FEATURE_X, y0, 0.0], [FEATURE_X + 64.0, y1, h]);
    }
    for h in STEP_HEIGHTS {
        let (y0, y1) = next(format!("step {h}"), 0.0, &mut spots);
        cuboid(&mut tris, [FEATURE_X, y0, 0.0], [FEATURE_X + PLATFORM, y1, h]);
    }
    for deg in RAMP_DEGREES {
        let (y0, y1) = next(format!("ramp {deg}"), 0.0, &mut spots);
        let run = RAMP_RISE / deg.to_radians().tan();
        ramp(&mut tris, FEATURE_X, FEATURE_X + run, y0, y1, 0.0, RAMP_RISE);
        cuboid(&mut tris, [FEATURE_X + run, y0, 0.0], [FEATURE_X + run + PLATFORM, y1, RAMP_RISE]);
    }
    let (y0, y1) = next("hill".into(), HILL_RISE, &mut spots);
    let run = HILL_RISE / HILL_DEGREES.to_radians().tan();
    cuboid(&mut tris, [-PLATFORM, y0, 0.0], [PLATFORM, y1, HILL_RISE]);
    ramp(&mut tris, PLATFORM, PLATFORM + run, y0, y1, HILL_RISE, 0.0);
    let (y0, _) = next("crates".into(), 0.0, &mut spots);
    for i in 0..8 {
        for j in 0..2 {
            let (x, y) = (FEATURE_X + i as f32 * 3.0 * CRATE, y0 + j as f32 * 3.0 * CRATE);
            cuboid(&mut tris, [x, y, 0.0], [x + CRATE, y + CRATE, CRATE]);
        }
    }
    TestMap { tris, spots }
}
