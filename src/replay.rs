//! Replaying a recording through the simulation and scoring it against the oracle.

use crate::ltr::{open, Record};
use crate::oracle::{self, outcome, LogTracer, Outcome};
use crate::sim::cue::Cue;
use crate::sim::{Move, MoveData, Player, Scratch, Tracer};

pub const TOL: f32 = 1e-3;

/// Every surface stood on in the recordings (indices 20, 22, 87) gives the cap, 1.0, so one
/// default stands in for the game's surface property table until that is extracted.
const DEFAULT_SURFACE_FRICTION: f32 = 0.8;

pub struct Step {
    pub seq: u32,
    pub want: Outcome,
    pub got: Outcome,
    pub missing: usize,
    pub unused: usize,
    /// Modelled fields beyond the outcome that disagree with the oracle.
    pub diffs: Vec<String>,
    pub cues: Vec<Cue>,
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
    let mut tr = LogTracer::new(&rec.traces);
    let (mut s, p, mv) = simulate(rec, carried, &mut tr);
    s.missing = tr.missing.len();
    s.unused = tr.unused();
    (s, p, mv)
}

/// Run one call with collision answered by `tr` rather than by the oracle's log.
pub fn simulate(rec: &Record, carried: Option<&(Player, MoveData)>, tr: &mut dyn Tracer) -> (Step, Player, MoveData) {
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
    let surface = |_: u32| DEFAULT_SURFACE_FRICTION;
    let cues = {
        let mut m = Move { p: &mut p, mv: &mut mv, s: &s, g, tr, surface: &surface, scratch: Scratch::default(), stance_index: None, cues: Vec::new() };
        m.pre_think();
        m.process_movement(oracle::SV_OPTIMIZEDMOVEMENT);
        m.cues
    };
    let mut diffs = oracle::diff_player(&p, &oracle::player(&rec.player_out));
    diffs.extend(oracle::diff_move(&mv, &oracle::move_data(&rec.mv_out, rec.seq)));
    let got = Outcome { origin: mv.origin, velocity: mv.velocity, on_ground: p.ground.is_some(), surface_friction: p.surface_friction };
    (Step { seq: rec.seq, want, got, missing: 0, unused: 0, diffs, cues }, p, mv)
}

pub fn file(path: &str) -> std::io::Result<Vec<Step>> {
    open(path)?.map(|r| r.map(|r| step(&r))).collect()
}

pub struct Chain {
    pub calls: usize,
    /// Calls where the carried state had to be thrown away for the oracle's, and why.
    pub resyncs: Vec<(u32, String)>,
    /// Calls where a server update replaced a networked timestamp (`network_update`).
    pub network_updates: usize,
}

/// What happens to the carried player between two movement calls: FinishMove, and the part
/// of RunCommand before ProcessMovement. Standing, the fall velocity comes back from outside
/// movement (a networked copy, e.g. 0.03125), so it is taken from the oracle.
fn between_calls(cp: &mut Player, cmv: &MoveData, rec: &Record, theirs: &Player) {
    let g = oracle::globals(&rec.globals, cp.lagged);
    cp.prev_velocity = cmv.velocity;
    if cp.ground.is_some() {
        cp.left_ground_time = g.curtime;
        cp.left_ground_up = cp.up;
        cp.fall_velocity = theirs.fall_velocity;
    } else if !cp.has_parent {
        let v = cmv.velocity;
        cp.fall_velocity = (v[1] * 0.0 + v[0] * 0.0) - v[2] * 1.0;
    }
}

/// The sprint timestamps are sent to the local player (`DT_LocalPlayerExclusive`), so a
/// server update replaces the predicted ones with the server's, stamped on its 0.05 s tick.
/// Movement writes them once per change, so the server's stay until the next change.
fn network_update(cp: &mut Player, theirs: &Player) -> bool {
    let fields = [
        (&mut cp.sprint_intent_time, theirs.sprint_intent_time),
        (&mut cp.sprint_start_time, theirs.sprint_start_time),
        (&mut cp.sprint_start_fraction, theirs.sprint_start_fraction),
        (&mut cp.sprint_end_time, theirs.sprint_end_time),
        (&mut cp.sprint_end_fraction, theirs.sprint_end_fraction),
    ];
    let mut any = false;
    for (ours, theirs) in fields {
        if ours.to_bits() != theirs.to_bits() {
            *ours = theirs;
            any = true;
        }
    }
    any
}

/// Replay a recording carrying the simulation's state from call to call. Before each call
/// the carried state is checked against the oracle's input; a mismatch (drift, or something
/// outside movement changing the player) is recorded and the chain restarts from the oracle.
pub fn chain(path: &str) -> std::io::Result<Chain> {
    let mut c = Chain { calls: 0, resyncs: Vec::new(), network_updates: 0 };
    let mut carried: Option<(Player, MoveData)> = None;
    for rec in open(path)? {
        let rec = rec?;
        c.calls += 1;
        if let Some((cp, cmv)) = &mut carried {
            let theirs = oracle::player(&rec.player_in);
            between_calls(cp, cmv, &rec, &theirs);
            c.network_updates += network_update(cp, &theirs) as usize;
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

/// Replay a whole recording with the simulation carrying its own state and answering its own
/// collision from `world`; inputs, settings and view angles still come from the recording.
/// Returns the worst origin and velocity error against the oracle over the run.
pub fn chain_world(path: &str, world: &mut dyn Tracer) -> std::io::Result<(f32, f32)> {
    let mut carried: Option<(Player, MoveData)> = None;
    let (mut worst_origin, mut worst_velocity) = (0.0f32, 0.0f32);
    for rec in open(path)? {
        let rec = rec?;
        if let Some((cp, cmv)) = &mut carried {
            between_calls(cp, cmv, &rec, &oracle::player(&rec.player_in));
        }
        let (s, p, mv) = simulate(&rec, carried.as_ref(), world);
        worst_origin = worst_origin.max(s.origin_err());
        worst_velocity = worst_velocity.max(s.velocity_err());
        carried = Some((p, mv));
    }
    Ok((worst_origin, worst_velocity))
}
