//! A plain map built in code: a large floor and, in lanes off it, walls, steps and ramps in
//! sizes either side of the limits the settings set, crates, a long hill to slide down, and
//! ziplines (down, up onto a ledge, up a tower, a chain of two, a sagging one), and a lane of
//! the game's own props. Each lane has a spot to start from, facing its features.

use crate::sim::math::V3;
use crate::sim::zipline::{Zipline, VERTICAL};
use crate::world::{Tri, World};

pub struct Spot {
    pub name: String,
    pub origin: V3,
    pub yaw: f32,
}

/// A static prop of `PROP_PAK`, read from the R5Reloaded install at run time; without one its
/// lane is empty.
pub struct Prop {
    pub model: &'static str,
    pub origin: V3,
    pub yaw: f32,
}

impl Prop {
    /// A direction in the model's space turned to the prop's yaw.
    pub fn turn(&self, v: V3) -> V3 {
        let (s, c) = self.yaw.to_radians().sin_cos();
        [v[0] * c - v[1] * s, v[0] * s + v[1] * c, v[2]]
    }

    /// A point in the model's space placed in the map.
    pub fn place(&self, v: V3) -> V3 {
        let t = self.turn(v);
        [self.origin[0] + t[0], self.origin[1] + t[1], self.origin[2] + t[2]]
    }
}

/// The Firing Range's pak: it holds loot bins, cargo containers and the Thunderdome's walls.
pub const PROP_PAK: &str = "mp_rr_canyonlands_staging.rpak";

pub struct TestMap {
    pub tris: Vec<Tri>,
    pub spots: Vec<Spot>,
    pub ziplines: Vec<Zipline>,
    pub props: Vec<Prop>,
}

impl TestMap {
    pub fn spot(&self, name: &str) -> &Spot {
        self.spots.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no spot {name}"))
    }

    pub fn world(&self) -> World {
        let mut w = World::from_tris(self.tris.clone());
        w.ziplines = self.ziplines.clone();
        w
    }
}

/// Ropes run straight between their ends unless a lane shapes them. The auto-detach distance
/// is the map's to set; these ride to the end.
const AUTO_DETACH: f32 = 0.0;
const SAG: f32 = 48.0;
const SAG_POINTS: usize = 9;

/// Ziplines from `points[0]` through each later point, one rope per span, the last ending
/// in a zipline_end. Ids follow on from those already in `out`.
fn chain(out: &mut Vec<Zipline>, ropes: &[Vec<V3>]) -> u32 {
    let first = out.len() as u32 + 1;
    for (i, pts) in ropes.iter().enumerate() {
        let id = first + i as u32;
        let mut z = Zipline::line(id, pts.clone(), id + 1, AUTO_DETACH);
        z.prev = (i > 0).then_some(id - 1);
        out.push(z);
    }
    let last = first + ropes.len() as u32 - 1;
    out.push(Zipline::end(last + 1, *ropes.last().unwrap().last().unwrap(), last, AUTO_DETACH));
    first
}

fn sagging(a: V3, b: V3) -> Vec<V3> {
    (0..SAG_POINTS)
        .map(|i| {
            let t = i as f32 / (SAG_POINTS - 1) as f32;
            [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t - SAG * 4.0 * t * (1.0 - t)]
        })
        .collect()
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
    let mut ziplines = Vec::new();
    let (y0, _) = next("zipline down".into(), 0.0, &mut spots);
    let y = y0 + HALF_WIDTH;
    chain(&mut ziplines, &[vec![[FEATURE_X, y, 160.0], [FEATURE_X + 3072.0, y, 96.0]]]);
    let (y0, y1) = next("zipline up".into(), 0.0, &mut spots);
    let y = y0 + HALF_WIDTH;
    let top = FEATURE_X + 2048.0;
    cuboid(&mut tris, [top, y0, 0.0], [top + 4.0 * PLATFORM, y1, 380.0]);
    chain(&mut ziplines, &[vec![[FEATURE_X, y, 128.0], [top + 64.0, y, 480.0]]]);
    let (y0, y1) = next("zipline vertical".into(), 0.0, &mut spots);
    let y = y0 + HALF_WIDTH;
    cuboid(&mut tris, [FEATURE_X + 24.0, y0, 0.0], [FEATURE_X + 24.0 + PLATFORM, y1, 600.0]);
    let v = chain(&mut ziplines, &[vec![[FEATURE_X, y, 16.0], [FEATURE_X, y, 680.0]]]);
    ziplines[v as usize - 1].kind = VERTICAL;
    let (y0, _) = next("zipline chain".into(), 0.0, &mut spots);
    let y = y0 + HALF_WIDTH;
    let bend = [FEATURE_X + 1024.0, y + 192.0, 140.0];
    chain(&mut ziplines, &[vec![[FEATURE_X, y, 160.0], bend], vec![bend, [FEATURE_X + 2048.0, y, 120.0]]]);
    let (y0, _) = next("zipline sag".into(), 0.0, &mut spots);
    let y = y0 + HALF_WIDTH;
    chain(&mut ziplines, &[sagging([FEATURE_X, y, 176.0], [FEATURE_X + 2048.0, y, 176.0])]);
    let (y0, _) = next("props".into(), 0.0, &mut spots);
    let y = y0 + HALF_WIDTH;
    let prop = |model: &'static str, x: f32, y: f32, z: f32, yaw: f32| Prop { model, origin: [x, y, z], yaw };
    let blue = "mdl/containers/underbelly_cargo_container_128_blue_02.rmdl";
    let red = "mdl/containers/underbelly_cargo_container_128_red_02.rmdl";
    let fence = "mdl/colony/farmland_fence_rusted_01.rmdl";
    let props = vec![
        prop("mdl/containers/loot_bin_01_static.rmdl", FEATURE_X + 64.0, y, 0.0, 0.0),
        prop("mdl/rocks/thunderdome_quarry_block_01_orange.rmdl", FEATURE_X + 128.0, y + 256.0, 49.1, 0.0),
        prop(fence, FEATURE_X + 128.0, y - 192.0, 0.0, 0.0),
        prop(fence, FEATURE_X + 128.0, y - 256.0, 0.0, 0.0),
        prop(blue, FEATURE_X + 640.0, y, 0.0, 0.0),
        prop(blue, FEATURE_X + 640.0, y - 256.0, 0.0, 0.0),
        prop(red, FEATURE_X + 640.0, y - 256.0, 96.0, 0.0),
        prop("mdl/rocks/thunderdome_quarry_block_stairs_01_orange.rmdl", FEATURE_X + 704.0, y + 208.0, 0.0, 0.0),
        prop(red, FEATURE_X + 640.0, y + 512.0, 0.0, 90.0),
        prop("mdl/thunderdome/thunderdome_barrier_01.rmdl", FEATURE_X + 1280.0, y + 256.0, 0.0, 90.0),
        prop("mdl/rocks/rock_white_chalk_modular_wallrun_02.rmdl", FEATURE_X + 1920.0, y - 320.0, 247.3, 0.0),
    ];
    TestMap { tris, spots, ziplines, props }
}
