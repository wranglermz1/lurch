use lurch::replay::{TOL, replay};
use lurch::trace::parse;

const RECORDINGS: [&str; 8] = ["idle", "walk_fwd", "walk_back", "walk_diag", "stop", "strafe_circle", "fall", "air_strafe"];

#[test]
fn every_recording_replays_inside_tolerance() {
    let mut bad = Vec::new();
    for name in RECORDINGS {
        let path = format!("{}/traces/{name}.trace", env!("CARGO_MANIFEST_DIR"));
        let t = match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| parse(&s)) {
            Ok(t) => t,
            Err(e) => {
                bad.push(format!("{name}: {e}"));
                continue;
            }
        };
        let r = replay(&t);
        println!(
            "{name}: trajectory max error origin {} velocity {}",
            r.trajectory_max_origin, r.trajectory_max_velocity
        );
        if r.failures > 0 {
            bad.push(format!("{name}: {} of {} steps outside {TOL}", r.failures, r.steps));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn idle_is_still_and_contiguous() {
    let path = format!("{}/traces/idle.trace", env!("CARGO_MANIFEST_DIR"));
    let t = parse(&std::fs::read_to_string(path).expect("traces/idle.trace")).unwrap();
    for w in t.steps.windows(2) {
        assert_eq!(w[1].cmd.seq, w[0].cmd.seq + 1, "seq gap after line {}", w[0].line);
    }
    for s in &t.steps {
        assert_eq!(s.input.origin, s.output.origin, "line {}", s.line);
        assert_eq!(s.input.velocity, s.output.velocity, "line {}", s.line);
    }
}
