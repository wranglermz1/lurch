//! Replaying a recording through the simulation and scoring it against the oracle.

use std::borrow::Cow;

use crate::ltr::{open, Record};
use crate::oracle::{self, outcome, LogTracer, Outcome};
use crate::sim::pilot::{self, Pilot};
use crate::sim::setup::UserCmd;
use crate::sim::{Move, MoveData, Player, Scratch, Settings, Tracer};

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
    run_as(rec, carried, None)
}

fn run_as(rec: &Record, carried: Option<&(Player, MoveData)>, s: Option<&Settings>) -> (Step, Player, MoveData) {
    let mut tr = LogTracer::new(&rec.traces);
    let (mut s, p, mv) = simulate_with(rec, carried, &mut tr, &settings_for(rec, s));
    s.missing = tr.missing.len();
    s.unused = tr.unused();
    (s, p, mv)
}

/// The recording's settings unless `s` replaces them.
fn settings_for<'a>(rec: &Record, s: Option<&'a Settings>) -> Cow<'a, Settings> {
    s.map_or_else(|| Cow::Owned(oracle::settings(&rec.settings)), Cow::Borrowed)
}

/// Run one call with collision answered by `tr` rather than by the oracle's log.
pub fn simulate(rec: &Record, carried: Option<&(Player, MoveData)>, tr: &mut dyn Tracer) -> (Step, Player, MoveData) {
    simulate_with(rec, carried, tr, &oracle::settings(&rec.settings))
}

/// The same with settings `s` in place of the recording's.
pub fn simulate_with(rec: &Record, carried: Option<&(Player, MoveData)>, tr: &mut dyn Tracer, s: &Settings) -> (Step, Player, MoveData) {
    let want = outcome(rec);
    let mut p = oracle::player(&rec.player_in);
    let mut mv = oracle::move_in(rec);
    if let Some((cp, cmv)) = carried {
        p = oracle::with_state(p, cp);
        mv.origin = cmv.origin;
        mv.velocity = cmv.velocity;
    }
    let g = oracle::globals(&rec.globals, p.lagged);
    let surface = |_: u32| pilot::SURFACE_FRICTION;
    {
        let mut m = Move { p: &mut p, mv: &mut mv, s, g, tr, surface: &surface, scratch: Scratch::default(), stance_index: None };
        m.pre_think();
        m.process_movement(pilot::SV_OPTIMIZEDMOVEMENT);
    }
    let mut diffs = oracle::diff_player(&p, &oracle::player(&rec.player_out));
    diffs.extend(oracle::diff_move(&mv, &oracle::move_data(&rec.mv_out, rec.seq)));
    let got = Outcome { origin: mv.origin, velocity: mv.velocity, on_ground: p.ground.is_some(), surface_friction: p.surface_friction };
    (Step { seq: rec.seq, want, got, missing: 0, unused: 0, diffs }, p, mv)
}

/// Every call of a recording whose input was recorded (see `oracle::input_recorded`).
pub fn file(path: &str) -> std::io::Result<Vec<Step>> {
    file_as(path, None)
}

/// The same, run on settings `s` rather than the recording's.
pub fn file_with(path: &str, s: &Settings) -> std::io::Result<Vec<Step>> {
    file_as(path, Some(s))
}

fn file_as(path: &str, s: Option<&Settings>) -> std::io::Result<Vec<Step>> {
    let mut steps = Vec::new();
    for rec in open(path)? {
        let rec = rec?;
        if !oracle::input_recorded(&rec) {
            continue;
        }
        steps.push(run_as(&rec, None, s).0);
    }
    Ok(steps)
}

pub struct Chain {
    pub calls: usize,
    /// Calls where the carried state had to be thrown away for the oracle's, and why.
    pub resyncs: Vec<(u32, String)>,
    /// Calls whose input was not recorded; the chain restarts after each.
    pub unrecorded: usize,
    /// Calls where something outside movement changed the player between calls, as when the
    /// game restores networked state; the chain restarts from the oracle without a resync.
    pub external: usize,
}

/// Replay a recording carrying the simulation's state from call to call. Before each call
/// the carried state is checked against the oracle's input; a mismatch (drift, or something
/// outside movement changing the player) is recorded and the chain restarts from the oracle.
pub fn chain(path: &str) -> std::io::Result<Chain> {
    chain_as(path, None)
}

/// The same, run on settings `s` rather than the recording's.
pub fn chain_with(path: &str, s: &Settings) -> std::io::Result<Chain> {
    chain_as(path, Some(s))
}

fn chain_as(path: &str, s: Option<&Settings>) -> std::io::Result<Chain> {
    let mut c = Chain { calls: 0, resyncs: Vec::new(), unrecorded: 0, external: 0 };
    let mut carried: Option<(Player, MoveData)> = None;
    let mut last_out: Option<Player> = None;
    for rec in open(path)? {
        let rec = rec?;
        c.calls += 1;
        if !oracle::input_recorded(&rec) {
            c.unrecorded += 1;
            carried = None;
            continue;
        }
        let out = oracle::player(&rec.player_out);
        if let Some((cp, cmv)) = &mut carried {
            let g = oracle::globals(&rec.globals, cp.lagged);
            cp.prev_velocity = cmv.velocity;
            cp.abs_origin = cmv.origin;
            cp.pre_think(g.curtime);
            let theirs = oracle::player(&rec.player_in);
            let mut d = oracle::diff_player(cp, &theirs);
            let mvin = oracle::move_in(&rec);
            let moved = cmv.origin != mvin.origin || cmv.velocity != mvin.velocity;
            if cmv.origin != mvin.origin {
                d.push(format!("origin {:?} want {:?}", cmv.origin, mvin.origin));
            }
            if cmv.velocity != mvin.velocity {
                d.push(format!("velocity {:?} want {:?}", cmv.velocity, mvin.velocity));
            }
            if !d.is_empty() {
                let external = last_out.as_mut().map_or(Vec::new(), |lo| {
                    lo.pre_think(g.curtime);
                    oracle::diff_player_values(lo, &theirs).iter().map(|e| e.split(' ').next().unwrap().to_string()).collect()
                });
                let ours = oracle::diff_player_values(cp, &theirs);
                if moved || ours.iter().any(|e| !external.iter().any(|f| e.split(' ').next() == Some(f.as_str()))) {
                    c.resyncs.push((rec.seq, d.join("; ")));
                } else {
                    c.external += 1;
                }
                carried = None;
            }
        }
        let (st, p, mv) = run_as(&rec, carried.as_ref(), s);
        carried = if st.missing == 0 { Some((p, mv)) } else { None };
        last_out = Some(out);
    }
    Ok(c)
}

/// Replay a whole recording with the simulation carrying its own state and answering its own
/// collision from `world`; inputs and view angles still come from the recording, and settings
/// too unless `s` replaces them. Returns the worst origin and velocity error against the oracle over the run.
pub fn chain_world(path: &str, world: &mut dyn Tracer, s: Option<&Settings>) -> std::io::Result<(f32, f32)> {
    let mut carried: Option<(Player, MoveData)> = None;
    let (mut worst_origin, mut worst_velocity) = (0.0f32, 0.0f32);
    for rec in open(path)? {
        let rec = rec?;
        if let Some((cp, cmv)) = &mut carried {
            let g = oracle::globals(&rec.globals, cp.lagged);
            cp.prev_velocity = cmv.velocity;
            cp.abs_origin = cmv.origin;
            cp.pre_think(g.curtime);
        }
        let (st, p, mv) = simulate_with(&rec, carried.as_ref(), world, &settings_for(&rec, s));
        worst_origin = worst_origin.max(st.origin_err());
        worst_velocity = worst_velocity.max(st.velocity_err());
        carried = Some((p, mv));
    }
    Ok((worst_origin, worst_velocity))
}

/// Replay a recording from its commands and clock alone: a pilot spawned where the recording
/// starts, the built-in settings, and move data built from each command. Collision comes from
/// `world`, or from the oracle's log without one. Only the outcome is scored, as a spawned
/// pilot has none of the recorded player's history. The run ends at the first call whose
/// command was not recorded.
pub fn from_commands(path: &str, mut world: Option<&mut dyn Tracer>) -> std::io::Result<Vec<Step>> {
    let s = pilot::settings();
    let mut steps = Vec::new();
    let mut pilot: Option<Pilot> = None;
    for rec in open(path)? {
        let rec = rec?;
        if !oracle::input_recorded(&rec) {
            break;
        }
        let pl = pilot.get_or_insert_with(|| Pilot::new(oracle::move_in(&rec).origin));
        let input = oracle::move_data(&rec.mv_in, rec.seq);
        let clock = oracle::globals(&rec.globals, 1.0);
        let cmd = UserCmd { command_number: rec.seq, angles: input.angles, forward: input.forward, side: input.side, up: input.up, buttons: input.buttons, frametime: clock.frametime };
        let mut log = LogTracer::new(&rec.traces);
        let tr: &mut dyn Tracer = match world.as_deref_mut() {
            Some(w) => w,
            None => &mut log,
        };
        pl.step(&s, &cmd, clock.curtime, tr);
        let got = Outcome { origin: pl.origin, velocity: pl.velocity, on_ground: pl.player.ground.is_some(), surface_friction: pl.player.surface_friction };
        steps.push(Step { seq: rec.seq, want: outcome(&rec), got, missing: log.missing.len(), unused: log.unused(), diffs: Vec::new() });
    }
    Ok(steps)
}
