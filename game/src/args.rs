use std::path::PathBuf;

use crate::script::{self, parse_frametime};
use crate::Config;

pub const USAGE: &str = "usage: lurch-game [--spot NAME] [--shot FRAME:PATH] [--script PATH [--frametime SECS] [--headless]]";

pub fn parse(mut args: impl Iterator<Item = String>) -> Result<Config, String> {
    let mut cfg = Config::default();
    let mut frametime = None;
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--spot" => cfg.spot = value()?,
            "--shot" => cfg.shot = Some(shot(&value()?)?),
            "--script" => {
                let path = value()?;
                let text = std::fs::read_to_string(&path).map_err(|e| format!("--script {path}: {e}"))?;
                cfg.script = Some(script::parse(&text).map_err(|e| format!("{path}: {e}"))?);
                cfg.scripted = true;
            }
            "--frametime" => frametime = Some(parse_frametime(&value()?)?),
            "--headless" => cfg.headless = true,
            _ => return Err(format!("unknown flag {flag}")),
        }
    }
    if !cfg.scripted && (cfg.headless || frametime.is_some()) {
        return Err("--headless and --frametime need --script".into());
    }
    if let Some(ft) = frametime {
        cfg.frametime = ft;
    }
    Ok(cfg)
}

fn shot(v: &str) -> Result<(u64, PathBuf), String> {
    let (frame, path) = v.split_once(':').ok_or_else(|| format!("--shot wants FRAME:PATH, got {v}"))?;
    let frame = frame.parse().map_err(|_| format!("--shot frame is not a number: {frame}"))?;
    if path.is_empty() {
        return Err("--shot needs a path".into());
    }
    Ok((frame, PathBuf::from(path)))
}
