//! Loads the game's Miles project from an install and lists or plays its events:
//! `milescheck INSTALL [list FILTER | play NAME...]`.

use std::path::Path;

use lurch_game::miles::Miles;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let t = std::time::Instant::now();
    let m = match Miles::open(Path::new(&a[1])) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    eprintln!("{} events and sources in {:?}", m.events.len(), t.elapsed());
    match a.get(2).map(String::as_str) {
        Some("list") => {
            let f = a.get(3).map(|s| s.to_lowercase()).unwrap_or_default();
            let mut names: Vec<&str> = m.events.values().map(|(_, n)| n.as_str()).filter(|n| n.to_lowercase().contains(&f)).collect();
            names.sort();
            for n in names {
                println!("{n}");
            }
        }
        Some("play") => {
            m.listener([0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
            for (name, value) in lurch_game::audio::CONTROLLERS {
                m.controller(name, value);
            }
            for n in &a[3..] {
                println!("{n}: {:?}", m.play(n, [0.0, 0.0, 0.0], 1.0));
                m.submit();
                let until = std::time::Instant::now() + std::time::Duration::from_millis(1500);
                while std::time::Instant::now() < until {
                    if !m.service() {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                }
            }
        }
        _ => {}
    }
}
