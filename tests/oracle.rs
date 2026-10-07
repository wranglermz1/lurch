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
    let built_in = lurch::sim::pilot::settings();
    let mut bad = Vec::new();
    for p in recordings() {
        let path = p.to_str().unwrap();
        if lurch::ltr::open(path).unwrap().map != "mp_rr_arena_empty" {
            continue;
        }
        for s in [None, Some(&built_in)] {
            let mut world = lurch::world::World::floor(ARENA_FLOOR);
            let (origin, velocity) = lurch::replay::chain_world(path, &mut world, s).unwrap();
            if origin > lurch::replay::TOL || velocity > lurch::replay::TOL {
                let which = if s.is_some() { "built-in settings" } else { "recorded settings" };
                bad.push(format!("{} on {which}: worst origin {origin:e}, velocity {velocity:e}", p.display()));
            }
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

#[test]
fn recordings_replay_on_the_built_in_settings() {
    let s = lurch::sim::pilot::settings();
    let mut bad = Vec::new();
    for p in recordings() {
        let path = p.to_str().unwrap();
        let steps = lurch::replay::file_with(path, &s).unwrap();
        let fails = steps.iter().filter(|s| !s.ok()).count();
        let resyncs = lurch::replay::chain_with(path, &s).unwrap().resyncs.len();
        if fails > 0 || resyncs > 0 {
            bad.push(format!("{}: {fails} of {} calls diverge, {resyncs} resyncs", p.display(), steps.len()));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// Nothing from the oracle but each command and the clock. Collision comes from the oracle's
/// log, and on mp_rr_arena_empty also from our own floor.
#[test]
fn recordings_replay_from_their_commands_alone() {
    let mut bad = Vec::new();
    for p in recordings() {
        let path = p.to_str().unwrap();
        let logged = lurch::replay::from_commands(path, None).unwrap();
        let fails = logged.iter().filter(|s| !s.ok() || s.got.origin != s.want.origin || s.got.velocity != s.want.velocity).count();
        if fails > 0 {
            bad.push(format!("{}: {fails} of {} calls not bit-exact", p.display(), logged.len()));
        }
        if lurch::ltr::open(path).unwrap().map == "mp_rr_arena_empty" {
            let mut world = lurch::world::World::floor(ARENA_FLOOR);
            let steps = lurch::replay::from_commands(path, Some(&mut world)).unwrap();
            let fails = steps.iter().filter(|s| !s.ok()).count();
            if fails > 0 {
                bad.push(format!("{} on our own floor: {fails} of {} calls diverge", p.display(), steps.len()));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn setup_move_builds_the_recorded_move_data() {
    use lurch::sim::setup::{setup_move, UserCmd};
    let s = lurch::sim::pilot::settings();
    let mut bad = Vec::new();
    for p in recordings() {
        let mut fails = 0;
        let mut first = None;
        for rec in lurch::ltr::open(p.to_str().unwrap()).unwrap() {
            let rec = rec.unwrap();
            if !lurch::oracle::input_recorded(&rec) {
                continue;
            }
            let want = lurch::oracle::move_data(&rec.mv_in, rec.seq);
            let cmd = UserCmd { command_number: rec.seq, angles: want.angles, forward: want.forward, side: want.side, up: want.up, buttons: want.buttons, frametime: 0.0, random_seed: 0 };
            let got = setup_move(&s, &cmd, want.oldbuttons, want.origin, want.velocity);
            let key = |m: &lurch::sim::MoveData| {
                format!(
                    "{:?}",
                    (m.angles, m.forward, m.side, m.up, m.view_forward, m.flat_forward, m.flat_right, m.flat_wish, m.ground_wish, m.jump_dir, m.hull_height, m.hull_half_width)
                )
            };
            if key(&got) != key(&want) {
                fails += 1;
                first.get_or_insert_with(|| format!("seq {:x}: got {} want {}", rec.seq, key(&got), key(&want)));
            }
        }
        if let Some(f) = first {
            bad.push(format!("{}: {fails} calls differ, first {f}", p.display()));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
