use lurch::ltr::open;
use lurch::oracle::{self, outcome, LogTracer, Outcome};
use lurch::sim::{Move, Scratch};
use std::process::ExitCode;

const TOL: f32 = 1e-3;

fn err(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0, f32::max)
}

fn main() -> ExitCode {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    let verbose = std::env::var("VERBOSE").is_ok();
    let mut ok = true;
    for path in &paths {
        let reader = match open(path) {
            Ok(r) => r,
            Err(e) => {
                println!("{path}: {e}");
                ok = false;
                continue;
            }
        };
        let (mut n, mut fails, mut missing, mut unused) = (0, 0, 0, 0);
        let (mut eo, mut ev) = (0.0f32, 0.0f32);
        let mut first: Option<String> = None;
        for rec in reader {
            let rec = rec.expect("record");
            n += 1;
            let want = outcome(&rec);
            let mut p = oracle::player(&rec.player_in);
            let mut mv = oracle::move_data(&rec.mv_in);
            let s = oracle::settings(&rec.settings);
            let g = oracle::globals(&rec.globals, p.lagged);
            let mut tr = LogTracer::new(&rec.traces);
            let out_friction = want.surface_friction;
            let surface = move |_: u32| out_friction / 1.25;
            {
                let mut m = Move { p: &mut p, mv: &mut mv, s: &s, g, tr: &mut tr, surface: &surface, scratch: Scratch::default() };
                m.player_move(oracle::SV_OPTIMIZEDMOVEMENT);
            }
            let got = Outcome { origin: mv.origin, velocity: mv.velocity, on_ground: p.ground.is_some(), surface_friction: p.surface_friction };
            let (o, v) = (err(got.origin, want.origin), err(got.velocity, want.velocity));
            eo = eo.max(o);
            ev = ev.max(v);
            missing += tr.missing.len();
            unused += tr.unused();
            let bad = o > TOL || v > TOL || got.on_ground != want.on_ground || !tr.missing.is_empty();
            if bad {
                fails += 1;
            }
            if (bad && first.is_none()) || (verbose && bad) {
                let msg = format!(
                    "seq {:x}: origin err {o:.5} vel err {v:.5}\n    in  {:?} {:?} ground {}\n    want {want:?}\n    got  {got:?}\n    traces logged {} missing {:?}",
                    rec.seq,
                    oracle::move_data(&rec.mv_in).origin,
                    oracle::move_data(&rec.mv_in).velocity,
                    oracle::ground_of(&rec.player_in).is_some(),
                    rec.traces.len(),
                    tr.missing
                );
                if verbose {
                    println!("{msg}");
                }
                first.get_or_insert(msg);
            }
        }
        println!("{path}: {n} calls, {fails} failing at {TOL}, max err origin {eo:.5} velocity {ev:.5}, {missing} traces not in log, {unused} logged traces unused");
        if let Some(f) = first {
            ok = false;
            println!("  first failure {f}");
        }
    }
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
