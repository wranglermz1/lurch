//! Every recording must replay through the simulation exactly: the committed ones in traces/
//! and any local ones (traces/local/, too large to commit, e.g. fuzz runs). Recordings of
//! mechanics not ported yet wait in pending/ (and local/pending/); move one up once it passes.

fn recordings() -> Vec<std::path::PathBuf> {
    in_dirs(&["", "local"])
}

fn in_dirs(dirs: &[&str]) -> Vec<std::path::PathBuf> {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/traces");
    let mut out = Vec::new();
    for dir in dirs.iter().map(|d| format!("{root}/{d}")) {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        out.extend(rd.map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "ltr")));
    }
    out.sort();
    out
}

#[test]
fn recordings_replay_exactly() {
    let paths = recordings();
    assert!(!paths.is_empty(), "no recordings in traces/");
    let mut bad = Vec::new();
    for p in &paths {
        let steps = lurch::replay::file(p.to_str().unwrap()).unwrap();
        let fails = steps.iter().filter(|s| !s.ok()).count();
        if fails > 0 {
            bad.push(format!("{}: {fails} of {} calls diverge", p.display(), steps.len()));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn recordings_hold_when_the_simulation_carries_its_own_state() {
    let mut bad = Vec::new();
    for p in recordings() {
        let c = lurch::replay::chain(p.to_str().unwrap()).unwrap();
        if let Some((seq, why)) = c.resyncs.first() {
            bad.push(format!("{}: {} resyncs, first at seq {seq:x}: {why}", p.display(), c.resyncs.len()));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// mp_rr_arena_empty's floor, the only geometry its recordings touch.
const ARENA_FLOOR: f32 = -2688.0;

#[test]
fn arena_recordings_hold_with_our_own_collision() {
    let mut bad = Vec::new();
    for p in recordings() {
        let path = p.to_str().unwrap();
        if lurch::ltr::open(path).unwrap().map != "mp_rr_arena_empty" {
            continue;
        }
        let mut world = lurch::world::World::floor(ARENA_FLOOR);
        let (origin, velocity) = lurch::replay::chain_world(path, &mut world).unwrap();
        if origin > lurch::replay::TOL || velocity > lurch::replay::TOL {
            bad.push(format!("{}: worst origin {origin:e}, velocity {velocity:e}", p.display()));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// The work queue: `cargo test --release -- --ignored` reports how far each pending recording
/// is from replaying.
#[test]
#[ignore]
fn pending_recordings() {
    let mut bad = Vec::new();
    for p in in_dirs(&["pending", "local/pending"]) {
        let steps = lurch::replay::file(p.to_str().unwrap()).unwrap();
        let fails = steps.iter().filter(|s| !s.ok()).count();
        let resyncs = lurch::replay::chain(p.to_str().unwrap()).unwrap().resyncs.len();
        if fails > 0 || resyncs > 0 {
            bad.push(format!("{}: {fails} of {} calls diverge, {resyncs} resyncs", p.display(), steps.len()));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// The cues movement raises line up with the oracle's own record of the same moments: a
/// footstep where it flipped its step side, a landing where it queued a landing probe, a jump
/// where it stamped a jump time.
#[test]
fn cues_fall_where_the_oracle_acted() {
    use lurch::sim::cue::Cue;
    let mut bad = Vec::new();
    for p in recordings() {
        for rec in lurch::ltr::open(p.to_str().unwrap()).unwrap() {
            let rec = rec.unwrap();
            let (before, after) = (lurch::oracle::player(&rec.player_in), lurch::oracle::player(&rec.player_out));
            let cues = lurch::replay::step(&rec).cues;
            let count = |f: fn(&Cue) -> bool| cues.iter().filter(|c| f(c)).count();
            let want = [
                ("footstep", (before.step_side != after.step_side) as usize, count(|c| matches!(c, Cue::Footstep { .. }))),
                ("land", (before.land_probe.is_none() && after.land_probe.is_some()) as usize, count(|c| matches!(c, Cue::Land { .. }))),
                ("jump", (before.last_jump_time != after.last_jump_time) as usize, count(|c| matches!(c, Cue::Jump { .. }))),
            ];
            for (what, oracle, ours) in want {
                if oracle != ours {
                    bad.push(format!("{} seq {:x}: {what} oracle {oracle} ours {ours}", p.display(), rec.seq));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad[..bad.len().min(20)].join("\n"));
}
