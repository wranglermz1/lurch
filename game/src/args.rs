use std::path::PathBuf;

use crate::Config;

pub const USAGE: &str = "usage: lurch-game [--spot NAME] [--shot FRAME:PATH]";

pub fn parse(mut args: impl Iterator<Item = String>) -> Result<Config, String> {
    let mut cfg = Config::default();
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--spot" => cfg.spot = value()?,
            "--shot" => cfg.shot = Some(shot(&value()?)?),
            _ => return Err(format!("unknown flag {flag}")),
        }
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
