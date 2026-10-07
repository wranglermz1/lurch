use lurch::movement::max_speed;
use lurch::replay::{TOL, replay};
use lurch::trace::{State, parse};
use std::process::ExitCode;

fn show(tag: &str, s: &State) -> String {
    format!(
        "{tag} origin {:?} velocity {:?} flags {:x} onground {}",
        s.origin, s.velocity, s.flags, s.onground
    )
}

fn main() -> ExitCode {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: replay TRACE...");
        return ExitCode::from(2);
    }
    let mut ok = true;
    for p in &paths {
        let t = match std::fs::read_to_string(p).map_err(|e| e.to_string()).and_then(|s| parse(&s)) {
            Ok(t) => t,
            Err(e) => {
                println!("{p}: {e}");
                ok = false;
                continue;
            }
        };
        let r = replay(&t);
        println!(
            "{p}: {} steps, {} single-step failures at tol {TOL}, max error origin {:.6} velocity {:.6}; trajectory max error origin {:.6} velocity {:.6}",
            r.steps, r.failures, r.max_origin, r.max_velocity, r.trajectory_max_origin, r.trajectory_max_velocity
        );
        if let Some((seq, why)) = r.unsupported {
            println!("  out of scope from seq {seq:x}: {why}");
        }
        if let Some((s, ours)) = r.first_failure {
            ok = false;
            println!("  first failure seq {:x} (line {}): {:?}", s.cmd.seq, s.line, s.cmd);
            println!("  {}", show("in   ", &s.input));
            println!("  {}", show("out  ", &s.output));
            println!("  {}", show("ours ", &ours));
            println!("  maxspeed oracle {} ours {}", s.maxspeed, max_speed(&t.settings, s.input.flags));
        }
    }
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
