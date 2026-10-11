//! Finding the R5Reloaded install the game's files are read from: the `--r5r` flag, else
//! asked for once on the console with the usual install folder offered, then kept in
//! `lurch.cfg` beside the executable.

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

/// Where the R5Reloaded launcher installs the game.
pub const DEFAULT: &str = r"C:\Program Files\R5Reloaded\R5R Library\LIVE";

/// An install holds `paks/Win64/common.rpak`.
pub fn valid(p: &Path) -> bool {
    p.join("paks").join("Win64").join("common.rpak").is_file()
}

pub fn resolve(given: Option<PathBuf>, interactive: bool, cfg: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = given {
        if valid(&p) {
            return Some(p);
        }
        eprintln!("--r5r {}: no paks/Win64/common.rpak there", p.display());
    }
    let default = PathBuf::from(DEFAULT);
    if !interactive || !std::io::stdin().is_terminal() {
        return valid(&default).then_some(default);
    }
    let stdin = std::io::stdin();
    loop {
        print!("R5Reloaded install folder (the one with r5apex.exe) [{DEFAULT}], or - for none: ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            return None;
        }
        let line = line.trim().trim_matches('"');
        if line == "-" {
            return None;
        }
        let p = if line.is_empty() { default.clone() } else { PathBuf::from(line) };
        if valid(&p) {
            if let Some(cfg) = cfg {
                let mut f = std::fs::OpenOptions::new().create(true).append(true).open(cfg);
                if let Ok(f) = &mut f {
                    let _ = writeln!(f, "--r5r {}", p.display());
                }
            }
            return Some(p);
        }
        println!("{}: no paks/Win64/common.rpak there", p.display());
    }
}
