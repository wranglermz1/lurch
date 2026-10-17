use std::num::NonZeroU32;
use std::path::PathBuf;

use lurch::assets::{Heirloom, HEIRLOOMS};
use lurch::sim::buttons::{IN_DUCK, IN_FORWARD, IN_JUMP};

use crate::binds::WheelMode;
use crate::script::{self, parse_frametime};
use crate::Config;

pub const USAGE: &str = "usage: lurch-game [--spot NAME] [--shot FRAME:PATH] [--script PATH [--frametime SECS] [--headless]]
                  [--sens F] [--fov-scale F] [--fps-max N] [--vsync] [--no-shadows] [--size WxH]
                  [--wheel-down A] [--wheel-up A] [--wheel-mode event|notch] [--frame-latency N] [--pipelined] [--autosprint]
                  [--r5r INSTALL] [--legend NAME] [--heirloom H]
  A is forward, jump, duck or none. --fps-max is 1 to 350, or 0 for no cap: above 350 fps each
  command still counts 1/350 s, so movement runs fast. INSTALL is an R5Reloaded install folder
  (the one holding r5apex.exe), asked for on the first run; NAME is a legend (wraith, bangalore, ...); H is none or wraith_kunai.";

/// No frame may be shorter than the retail `usercmd_frametime_min`.
pub const FPS_MAX_LIMIT: u32 = 350;

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
            "--sens" => cfg.sens = positive(&flag, &value()?)?,
            "--fov-scale" => cfg.fov_scale = positive(&flag, &value()?)?,
            "--fps-max" => {
                let v = value()?;
                cfg.fps_max = v.parse().ok().filter(|n| (0..=FPS_MAX_LIMIT).contains(n)).ok_or_else(|| format!("--fps-max wants 0 to {FPS_MAX_LIMIT}, got {v}"))?;
            }
            "--vsync" => cfg.vsync = true,
            "--no-shadows" => cfg.shadows = false,
            "--size" => cfg.size = size(&value()?)?,
            "--wheel-down" => cfg.wheel_down = wheel_action(&value()?)?,
            "--wheel-up" => cfg.wheel_up = wheel_action(&value()?)?,
            "--wheel-mode" => {
                cfg.wheel_mode = match value()?.as_str() {
                    "event" => WheelMode::Event,
                    "notch" => WheelMode::Notch,
                    v => return Err(format!("--wheel-mode wants event or notch, got {v}")),
                }
            }
            "--frame-latency" => {
                let v = value()?;
                cfg.frame_latency = Some(v.parse::<NonZeroU32>().map_err(|_| format!("--frame-latency wants a whole number above 0, got {v}"))?);
            }
            "--pipelined" => cfg.pipelined = true,
            "--autosprint" => cfg.autosprint = true,
            "--r5r" => cfg.r5r = Some(PathBuf::from(value()?)),
            "--legend" => cfg.legend = value()?,
            "--heirloom" => cfg.heirloom = heirloom(&value()?)?,
            _ => return Err(format!("unknown flag {flag}")),
        }
    }
    if !cfg.scripted && (cfg.headless || frametime.is_some()) {
        return Err("--headless and --frametime need --script".into());
    }
    if cfg.headless && cfg.script.as_ref().is_some_and(|s| s.exit_frame().is_none()) {
        return Err("--headless needs a script that ends with exit".into());
    }
    if let Some(ft) = frametime {
        cfg.frametime = ft;
    }
    Ok(cfg)
}

/// Arguments from `lurch.cfg`: whitespace-separated, `#` starts a comment line, and an
/// `--r5r` line takes the rest of the line as its path, spaces and all.
pub fn file_args(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.starts_with('#')) {
        if let Some(path) = line.strip_prefix("--r5r ") {
            out.extend(["--r5r".to_string(), path.trim().to_string()]);
        } else {
            out.extend(line.split_whitespace().map(String::from));
        }
    }
    out
}

fn shot(v: &str) -> Result<(u64, PathBuf), String> {
    let (frame, path) = v.split_once(':').ok_or_else(|| format!("--shot wants FRAME:PATH, got {v}"))?;
    let frame = frame.parse().map_err(|_| format!("--shot frame is not a number: {frame}"))?;
    if path.is_empty() {
        return Err("--shot needs a path".into());
    }
    Ok((frame, PathBuf::from(path)))
}

fn positive(flag: &str, v: &str) -> Result<f32, String> {
    v.parse::<f32>().ok().filter(|f| f.is_finite() && *f > 0.0).ok_or_else(|| format!("{flag} wants a number above 0, got {v}"))
}

fn size(v: &str) -> Result<(u32, u32), String> {
    let bad = || format!("--size wants WxH, got {v}");
    let (w, h) = v.split_once('x').ok_or_else(bad)?;
    match (w.parse(), h.parse()) {
        (Ok(w), Ok(h)) if w > 0 && h > 0 => Ok((w, h)),
        _ => Err(bad()),
    }
}

fn heirloom(v: &str) -> Result<Option<&'static Heirloom>, String> {
    if v == "none" {
        return Ok(None);
    }
    let names: Vec<&str> = HEIRLOOMS.iter().map(|h| h.name).collect();
    HEIRLOOMS.iter().find(|h| h.name == v).map(Some).ok_or_else(|| format!("--heirloom wants none or {}, got {v}", names.join(", ")))
}

fn wheel_action(v: &str) -> Result<Option<u32>, String> {
    match v {
        "forward" => Ok(Some(IN_FORWARD)),
        "jump" => Ok(Some(IN_JUMP)),
        "duck" => Ok(Some(IN_DUCK)),
        "none" => Ok(None),
        _ => Err(format!("wheel actions are forward, jump, duck or none, got {v}")),
    }
}
