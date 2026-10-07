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
