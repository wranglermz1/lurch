//! Runs every RUI element of an install once with its defaults and counts what stops each:
//! the transform opcodes, callbacks and draw jobs not ported yet. A measure of how much of the
//! UI the runtime can draw (`docs/ui.md`).
//!
//!   cargo run --release --bin ruicensus -- INSTALL [NAME...]

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use lurch::rui::draw::Draws;
use lurch::rui::{Frame, Globals, Topology, Ui, RUI_DRAW_HUD};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let ui = Ui::open(Path::new(&args[1])).unwrap();
    let mut names: Vec<String> = if args.len() > 2 { args[2..].to_vec() } else { ui.lib.elements.keys().cloned().collect() };
    names.sort();
    let (globals, topology) = (Globals::new(100.0, 1920.0, 1080.0), Topology::hud(1920.0, 1080.0));
    let mut stops: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut drawn = 0;
    for name in &names {
        eprint!("{name}\r");
        let _ = std::io::stderr().flush();
        let Some(mut rui) = ui.create(name, RUI_DRAW_HUD, 0) else {
            stops.entry("no function in ui.dll".into()).or_default().push(name.clone());
            continue;
        };
        let mut frame = Frame::default();
        frame.screen_blur = Some([1920, 1080]);
        match ui.update(&mut rui, &globals, &topology, &mut frame) {
            Err(stop) => stops.entry(format!("{stop:?}")).or_default().push(name.clone()),
            Ok(visible) => {
                let mut draws = Draws::default();
                ui.draws(&rui, &frame, &mut draws);
                if draws.unported.is_empty() {
                    drawn += 1;
                    if !visible {
                        stops.entry("hidden with its defaults".into()).or_default().push(name.clone());
                    }
                }
                for u in draws.unported {
                    let what = u.split_once(": ").map_or(u.clone(), |(_, w)| w.to_string());
                    stops.entry(what).or_default().push(name.clone());
                }
            }
        }
    }
    let mut by_count: Vec<_> = stops.into_iter().collect();
    by_count.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    println!("{} elements, {drawn} ran and drew without an unported step", names.len());
    for (why, who) in by_count {
        println!("{:5}  {why}  (e.g. {})", who.len(), who.iter().take(4).cloned().collect::<Vec<_>>().join(", "));
    }
}
