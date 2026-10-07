//! Replaying a recording through the simulation and scoring it against the oracle.

use crate::ltr::{open, Record};
use crate::oracle::{self, outcome, LogTracer, Outcome};
use crate::sim::{Move, MoveData, Player, Scratch};

pub const TOL: f32 = 1e-3;

pub struct Step {
    pub seq: u32,
    pub want: Outcome,
    pub got: Outcome,
    pub missing: usize,
    pub unused: usize,
    /// Modelled fields beyond the outcome that disagree with the oracle.
    pub diffs: Vec<String>,
}

impl Step {
    pub fn origin_err(&self) -> f32 {
        err(self.got.origin, self.want.origin)
    }

    pub fn velocity_err(&self) -> f32 {
        err(self.got.velocity, self.want.velocity)
    }

    pub fn ok(&self) -> bool {
        self.origin_err() <= TOL
            && self.velocity_err() <= TOL
            && self.got.on_ground == self.want.on_ground
            && self.missing == 0
            && self.diffs.is_empty()
    }
}

fn err(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0, f32::max)
}

pub fn step(rec: &Record) -> Step {
    run(rec, None).0
}

/// Run one call. With `carried`, the simulation's own player state and position from the
/// previous call replace the oracle's; everything else still comes from the recording.
pub fn run(rec: &Record, carried: Option<&(Player, MoveData)>) -> (Step, Player, MoveData) {
    let want = outcome(rec);
    let mut p = oracle::player(&rec.player_in);
    let mut mv = oracle::move_data(&rec.mv_in, rec.seq);
    if let Some((cp, cmv)) = carried {
        p = oracle::with_state(p, cp);
        mv.origin = cmv.origin;
        mv.velocity = cmv.velocity;
    }
    let s = oracle::settings(&rec.settings);
    let g = oracle::globals(&rec.globals, p.lagged);
    let mut tr = LogTracer::new(&rec.traces);
    let out_friction = want.surface_friction;
    let surface = move |_: u32| out_friction / 1.25;
    {
        let mut m = Move { p: &mut p, mv: &mut mv, s: &s, g, tr: &mut tr, surface: &surface, scratch: Scratch::default(), stance_index: None };
        m.pre_think();
        m.process_movement(oracle::SV_OPTIMIZEDMOVEMENT);
    }
    let mut diffs = oracle::diff_player(&p, &oracle::player(&rec.player_out));
    diffs.extend(oracle::diff_move(&mv, &oracle::move_data(&rec.mv_out, rec.seq)));
    let got = Outcome { origin: mv.origin, velocity: mv.velocity, on_ground: p.ground.is_some(), surface_friction: p.surface_friction };
    (Step { seq: rec.seq, want, got, missing: tr.missing.len(), unused: tr.unused(), diffs }, p, mv)
}

pub fn file(path: &str) -> std::io::Result<Vec<Step>> {
    open(path)?.map(|r| r.map(|r| step(&r))).collect()
}

pub struct Chain {
    pub calls: usize,
    /// Calls where the carried state had to be thrown away for the oracle's, and why.
    pub resyncs: Vec<(u32, String)>,
}

/// Replay a recording carrying the simulation's state from call to call. Before each call
/// the carried state is checked against the oracle's input; a mismatch (drift, or something
/// outside movement changing the player) is recorded and the chain restarts from the oracle.
pub fn chain(path: &str) -> std::io::Result<Chain> {
    let mut c = Chain { calls: 0, resyncs: Vec::new() };
    let mut carried: Option<(Player, MoveData)> = None;
    for rec in open(path)? {
        let rec = rec?;
        c.calls += 1;
        if let Some((cp, cmv)) = &mut carried {
            let g = oracle::globals(&rec.globals, cp.lagged);
            if cp.ground.is_some() {
                cp.left_ground_time = g.curtime;
                cp.left_ground_up = cp.up;
            }
            let theirs = oracle::player(&rec.player_in);
            let mut d = oracle::diff_player(cp, &theirs);
            let mvin = oracle::move_data(&rec.mv_in, rec.seq);
            if cmv.origin != mvin.origin {
                d.push(format!("origin {:?} want {:?}", cmv.origin, mvin.origin));
            }
            if cmv.velocity != mvin.velocity {
                d.push(format!("velocity {:?} want {:?}", cmv.velocity, mvin.velocity));
            }
            if !d.is_empty() {
                c.resyncs.push((rec.seq, d.join("; ")));
                carried = None;
            }
        }
        let (s, p, mv) = run(&rec, carried.as_ref());
        carried = if s.missing == 0 { Some((p, mv)) } else { None };
    }
    Ok(c)
}
