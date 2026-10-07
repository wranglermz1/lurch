//! Every committed recording must replay through the simulation inside tolerance.

#[test]
fn recordings_replay_exactly() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/traces");
    let mut paths: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "ltr")).collect();
    paths.sort();
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
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/traces");
    let mut bad = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|e| e != "ltr") {
            continue;
        }
        let c = lurch::replay::chain(p.to_str().unwrap()).unwrap();
        if let Some((seq, why)) = c.resyncs.first() {
            bad.push(format!("{}: {} resyncs, first at seq {seq:x}: {why}", p.display(), c.resyncs.len()));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
