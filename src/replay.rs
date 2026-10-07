//! Replaying a recording through the simulation and scoring it against the oracle.

use crate::ltr::{open, Record};
use crate::oracle::{self, outcome, LogTracer, Outcome};
use crate::sim::{Move, Scratch};

pub const TOL: f32 = 1e-3;

pub struct Step {
    pub seq: u32,
    pub want: Outcome,
    pub got: Outcome,
    pub missing: usize,
    pub unused: usize,
}

impl Step {
    pub fn origin_err(&self) -> f32 {
        err(self.got.origin, self.want.origin)
    }

    pub fn velocity_err(&self) -> f32 {
        err(self.got.velocity, self.want.velocity)
    }

    pub fn ok(&self) -> bool {
        self.origin_err() <= TOL && self.velocity_err() <= TOL && self.got.on_ground == self.want.on_ground && self.missing == 0
    }
}

fn err(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0, f32::max)
}

pub fn step(rec: &Record) -> Step {
    let want = outcome(rec);
    let mut p = oracle::player(&rec.player_in);
    let mut mv = oracle::move_data(&rec.mv_in, rec.seq);
    let s = oracle::settings(&rec.settings);
    let g = oracle::globals(&rec.globals, p.lagged);
    let mut tr = LogTracer::new(&rec.traces);
    let out_friction = want.surface_friction;
    let surface = move |_: u32| out_friction / 1.25;
    {
        let mut m = Move { p: &mut p, mv: &mut mv, s: &s, g, tr: &mut tr, surface: &surface, scratch: Scratch::default() };
        m.pre_think();
        m.player_move(oracle::SV_OPTIMIZEDMOVEMENT);
    }
    let got = Outcome { origin: mv.origin, velocity: mv.velocity, on_ground: p.ground.is_some(), surface_friction: p.surface_friction };
    Step { seq: rec.seq, want, got, missing: tr.missing.len(), unused: tr.unused() }
}

pub fn file(path: &str) -> std::io::Result<Vec<Step>> {
    open(path)?.map(|r| r.map(|r| step(&r))).collect()
}
