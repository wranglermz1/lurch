use lurch::replay::{file, TOL};
use std::process::ExitCode;

fn main() -> ExitCode {
    let only = std::env::var("SEQ").ok();
    let mut ok = true;
    for path in std::env::args().skip(1) {
        let steps = match file(&path) {
            Ok(s) => s,
            Err(e) => {
                println!("{path}: {e}");
                ok = false;
                continue;
            }
        };
        let steps: Vec<_> = steps.into_iter().filter(|s| only.as_ref().is_none_or(|q| *q == format!("{:x}", s.seq))).collect();
        let fails: Vec<_> = steps.iter().filter(|s| !s.ok()).collect();
        let eo = steps.iter().map(|s| s.origin_err()).fold(0.0, f32::max);
        let ev = steps.iter().map(|s| s.velocity_err()).fold(0.0, f32::max);
        let missing: usize = steps.iter().map(|s| s.missing).sum();
        println!("{path}: {} calls, {} failing at {TOL}, max err origin {eo:.5} velocity {ev:.5}, {missing} traces not in log", steps.len(), fails.len());
        if let Some(f) = fails.first() {
            ok = false;
            println!("  first failure seq {:x}\n    want {:?}\n    got  {:?}\n    traces not in log {}", f.seq, f.want, f.got, f.missing);
        }
    }
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
