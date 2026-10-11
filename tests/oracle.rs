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

/// The built-in root motion (`src/sim/rootmotion.rs`, made by `src/bin/rootmotion.rs`) puts
/// every recorded mantle's animation root where the oracle did. Types never recorded fall back
/// to the procedural path and are not covered.
#[test]
fn recovered_root_motion_places_the_recorded_mantles() {
    use lurch::sim::mantle::{RootMotion, RECOVERED};
    let mut bad = Vec::new();
    for p in recordings() {
        for rec in lurch::ltr::open(p.to_str().unwrap()).unwrap() {
            let rec = rec.unwrap();
            let Some(want) = lurch::oracle::RecordedRoot::of(&rec).at() else { continue };
            let q = lurch::oracle::player(&rec.player_out);
            let Some(got) = RECOVERED.place(q.traversal_type, q.traversal_cycle, q.traversal_ref, q.traversal_forward) else { continue };
            let err = (0..3).map(|i| (got[i] - want[i]).abs()).fold(0.0, f32::max);
            if err > lurch::replay::TOL {
                bad.push(format!("{} seq {:x}: {got:?} want {want:?}", p.display(), rec.seq));
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
                    (m.angles, m.forward, m.side, m.up, m.view_forward, m.flat_forward, m.flat_right, m.flat_wish, m.ground_wish, m.jump_dir, m.hull_height, (m.hull_half_width, m.view_wish, m.pressed))
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

/// The view setup runs once a frame between movement calls, on the frame's clock: from the
/// oracle's state after one call it must reach the oracle's state before the next.
#[test]
fn view_frames_between_calls_match() {
    let mut bad = Vec::new();
    for p in recordings() {
        let (mut frames, mut fails, mut first) = (0, 0, None);
        let mut prev: Option<lurch::ltr::Record> = None;
        for rec in lurch::ltr::open(p.to_str().unwrap()).unwrap() {
            let rec = rec.unwrap();
            if let Some(pr) = prev.as_ref().filter(|pr| lurch::oracle::input_recorded(pr) && lurch::oracle::input_recorded(&rec)) {
                let s = lurch::oracle::settings(&pr.settings);
                let mut ours = lurch::oracle::player(&pr.player_out);
                let (t, dt) = lurch::oracle::view_clock(pr);
                ours.view_frame(&s, lurch::oracle::move_in(&rec).velocity, t, dt);
                let theirs = lurch::oracle::player(&rec.player_in);
                let key = |p: &lurch::sim::Player| format!("{:?}", (p.slide_tilt, p.fov_start, p.fov_duration, p.fov_from, p.fov_to, p.view_entity_blend));
                frames += 1;
                if key(&ours) != key(&theirs) {
                    fails += 1;
                    first.get_or_insert_with(|| format!("after seq {:x}: {} want {}", pr.seq, key(&ours), key(&theirs)));
                }
            }
            prev = Some(rec);
        }
        if let Some(f) = first {
            bad.push(format!("{}: {fails} of {frames} frames differ, first {f}", p.display()));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// A zipline snapshot laid out as `capture/lurchcap.c` writes it decodes to the entities it
/// describes, and stays out of the trace matching.
#[test]
fn zipline_snapshots_decode_and_stay_out_of_trace_matching() {
    use lurch::ltr::Trace;
    use lurch::sim::Tracer;
    let put = |b: &mut Vec<u8>, off: usize, bytes: &[u8]| b[off..off + bytes.len()].copy_from_slice(bytes);
    let f = |v: f32| v.to_le_bytes();
    let mut rope = vec![0u8; 32 + 0x600];
    put(&mut rope, 0, &0x20172u32.to_le_bytes());
    put(&mut rope, 4, &1u32.to_le_bytes());
    put(&mut rope, 8, &[f(10.0), f(20.0), f(30.0)].concat());
    let at = |off: usize| 32 + off - 0xa00;
    put(&mut rope, at(0xa0c), &2u32.to_le_bytes());
    put(&mut rope, at(0xd4c), &u32::MAX.to_le_bytes());
    put(&mut rope, at(0xd50), &0x30173u32.to_le_bytes());
    put(&mut rope, at(0xd58), &f(64.0));
    put(&mut rope, at(0xd64), &[1]);
    put(&mut rope, at(0xe30), &f(1.5));
    put(&mut rope, at(0xe40), &2u32.to_le_bytes());
    put(&mut rope, at(0xe44), &[f(10.0), f(20.0), f(30.0), f(10.0), f(20.0), f(530.0)].concat());
    put(&mut rope, at(0xfc4), &f(500.0));
    let mut end = vec![0u8; 32 + 0x10];
    put(&mut end, 0, &0x30173u32.to_le_bytes());
    put(&mut end, 4, &2u32.to_le_bytes());
    put(&mut end, 8, &[f(10.0), f(20.0), f(530.0)].concat());
    put(&mut end, at(0xa00), &f(32.0));
    put(&mut end, at(0xa04), &[1]);
    put(&mut end, at(0xa08), &0x20172u32.to_le_bytes());
    let mut log = Vec::new();
    for (index, blob) in [rope, end].iter().enumerate() {
        for (c, chunk) in blob.chunks(0x68 + 0x180).enumerate() {
            let mut b = chunk.to_vec();
            b.resize(0x68 + 0x180, 0);
            log.push(Trace { slot: 0x1000 | (index as u32) << 4 | c as u32, mask: 0, ray: b[..0x68].to_vec(), result: b[0x68..].to_vec() });
        }
    }
    let tr = lurch::oracle::LogTracer::new(&log);
    assert_eq!(tr.unused(), 0);
    let z = tr.ziplines();
    assert_eq!(z.len(), 2);
    assert_eq!((z[0].id, z[0].kind, z[0].next, z[0].prev, z[0].speed_scale, z[0].auto_detach_distance), (0x20172, 2, Some(0x30173), None, 1.5, 64.0));
    assert_eq!((z[0].points.clone(), z[0].spans.clone(), z[0].enabled), (vec![[10.0, 20.0, 30.0], [10.0, 20.0, 530.0]], vec![500.0], true));
    assert_eq!((z[1].id, z[1].rope, z[1].prev, z[1].auto_detach_distance, z[1].push_off, z[1].origin), (0x30173, false, Some(0x20172), 32.0, true, [10.0, 20.0, 530.0]));
}

/// The cues movement raises line up with the oracle's own record of the same moments: a
/// footstep where it flipped its step side, a landing where it queued a landing probe, a jump
/// where it stamped a jump time.
#[test]
fn cues_fall_where_the_oracle_acted() {
    use lurch::sim::cue::Cue;
    use lurch::sim::zipline::RIDING;
    let mut bad = Vec::new();
    for p in recordings() {
        for rec in lurch::ltr::open(p.to_str().unwrap()).unwrap() {
            let rec = rec.unwrap();
            if !lurch::oracle::input_recorded(&rec) {
                continue;
            }
            let (before, after) = (lurch::oracle::player(&rec.player_in), lurch::oracle::player(&rec.player_out));
            let cues = lurch::replay::step(&rec).cues;
            let count = |f: fn(&Cue) -> bool| cues.iter().filter(|c| f(c)).count();
            let want = [
                ("footstep", (before.step_side != after.step_side) as usize, count(|c| matches!(c, Cue::Footstep { .. }))),
                ("land", (before.land_probe.is_none() && after.land_probe.is_some()) as usize, count(|c| matches!(c, Cue::Land { .. }))),
                ("jump", (before.last_jump_time != after.last_jump_time) as usize, count(|c| matches!(c, Cue::Jump { .. }))),
                ("sprint end", (before.sprinting && !after.sprinting) as usize, count(|c| matches!(c, Cue::SprintEnd))),
                ("zipline mount", (before.zipline_start_time != after.zipline_start_time) as usize, count(|c| matches!(c, Cue::ZiplineMount))),
                ("zipline start", (before.zipline_state != RIDING && after.zipline_state == RIDING) as usize, count(|c| matches!(c, Cue::ZiplineStart))),
                ("zipline stop", (before.zipline_detach_time != after.zipline_detach_time) as usize, count(|c| matches!(c, Cue::ZiplineStop))),
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
