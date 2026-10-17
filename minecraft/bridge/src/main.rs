//! lurch's movement for a Minecraft client, over one TCP connection of text lines.
//!
//! The mod sends `spawn X Y Z YAW` once, then `cmd FRAMETIME PITCH YAW HELD PRESSED` once per
//! rendered frame, and each `cmd` is answered with `pos X Y Z EYE_X EYE_Y EYE_Z GROUND`.
//! Positions and angles on the wire are Minecraft's (blocks, y up, its yaw); HELD and PRESSED
//! are usercmd button bits, PRESSED holding the keys that went down since the last `cmd`.
//!
//! Not modelled: any collision but a flat floor at the spawn height, the grapple, autosprint.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::str::{FromStr, SplitAsciiWhitespace};

use lurch::sim::buttons::*;
use lurch::sim::input::Keys;
use lurch::sim::math::V3;
use lurch::sim::pilot::{self, Clock, Pilot};
use lurch::sim::setup::{UserCmd, command_seed};
use lurch::sim::Settings;
use lurch::world::World;

const ADDR: &str = "127.0.0.1:25600";
/// A Source unit is an inch, a block a metre; 40 keeps block edges exact in f32.
const UNITS_PER_BLOCK: f64 = 40.0;
const RETAIL_USERCMD_FRAMETIME_MIN: f32 = 0.002857;
const RETAIL_USERCMD_FRAMETIME_MAX: f32 = 0.1;
const BUTTONS: u32 = IN_JUMP | IN_DUCK | IN_FORWARD | IN_BACK | IN_MOVELEFT | IN_MOVERIGHT | IN_SPEED;

/// One player, placed so that lurch's origin is `base` in Minecraft.
struct Session {
    base: [f64; 3],
    settings: Settings,
    world: World,
    pilot: Pilot,
    clock: Clock,
    keys: Keys,
    next_number: u32,
}

fn yaw_from_mc(yaw: f32) -> f32 {
    -yaw - 90.0
}

impl Session {
    fn spawn(base: [f64; 3], yaw: f32) -> Self {
        let mut pilot = Pilot::new([0.0; 3]);
        pilot.player.view_angles = [0.0, yaw_from_mc(yaw), 0.0];
        Session { base, settings: pilot::pathfinder(), world: World::floor(0.0), pilot, clock: Clock::default(), keys: Keys::default(), next_number: 0 }
    }

    fn to_mc(&self, v: V3) -> [f64; 3] {
        let b = |u: f32| u as f64 / UNITS_PER_BLOCK;
        [self.base[0] + b(v[0]), self.base[1] + b(v[2]), self.base[2] - b(v[1])]
    }

    #[allow(clippy::manual_clamp)] // the shell's clamp_frametime form: a NaN frametime becomes the minimum
    fn cmd(&mut self, frametime: f32, pitch: f32, yaw: f32, held: u32, pressed: u32) -> String {
        self.keys.press((held | pressed) & BUTTONS);
        self.keys.release(!held & BUTTONS);
        let n = self.next_number;
        self.next_number += 1;
        let frametime = frametime.max(RETAIL_USERCMD_FRAMETIME_MIN).min(RETAIL_USERCMD_FRAMETIME_MAX);
        let mut cmd = UserCmd { command_number: n, angles: [pitch, yaw_from_mc(yaw), 0.0], frametime, random_seed: command_seed(n), ..Default::default() };
        self.keys.fill(&mut cmd, None);
        self.pilot.step(&self.settings, &cmd, self.clock.now(), &mut self.world);
        self.clock.advance(frametime);
        self.pilot.view_frame(&self.settings, self.clock.now(), frametime);
        let [x, y, z] = self.to_mc(self.pilot.origin);
        let [ex, ey, ez] = self.to_mc(self.pilot.eye(&self.settings, 1.0, self.clock.now()).origin);
        let ground = self.pilot.player.ground.is_some() as u8;
        format!("pos {x} {y} {z} {ex} {ey} {ez} {ground}")
    }
}

fn arg<T: FromStr>(w: &mut SplitAsciiWhitespace) -> Result<T, &'static str> {
    w.next().and_then(|s| s.parse().ok()).ok_or("missing or bad argument")
}

fn handle(session: &mut Option<Session>, line: &str) -> Result<Option<String>, &'static str> {
    let mut w = line.split_ascii_whitespace();
    match w.next() {
        Some("spawn") => {
            let base = [arg(&mut w)?, arg(&mut w)?, arg(&mut w)?];
            *session = Some(Session::spawn(base, arg(&mut w)?));
            Ok(None)
        }
        Some("cmd") => {
            let s = session.as_mut().ok_or("cmd before spawn")?;
            Ok(Some(s.cmd(arg(&mut w)?, arg(&mut w)?, arg(&mut w)?, arg(&mut w)?, arg(&mut w)?)))
        }
        _ => Err("unknown message"),
    }
}

fn serve(stream: &TcpStream) -> io::Result<()> {
    let mut out = stream;
    let mut session = None;
    for line in BufReader::new(stream).lines() {
        let line = line?;
        let reply = handle(&mut session, &line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{e}: {line:?}")))?;
        if let Some(r) = reply {
            writeln!(out, "{r}")?;
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let listener = TcpListener::bind(ADDR)?;
    eprintln!("lurch-bridge: listening on {ADDR}");
    for stream in listener.incoming() {
        let stream = stream?;
        stream.set_nodelay(true)?;
        eprintln!("lurch-bridge: client connected");
        match serve(&stream) {
            Ok(()) => eprintln!("lurch-bridge: client left"),
            Err(e) => eprintln!("lurch-bridge: {e}"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(reply: &str) -> Vec<f64> {
        reply.split(' ').skip(1).map(|s| s.parse().unwrap()).collect()
    }

    #[test]
    fn forward_walks_where_minecraft_faces_and_stays_on_the_floor() {
        let mut s = None;
        handle(&mut s, "spawn 100.5 -60 -20.5 0").unwrap();
        handle(&mut s, "cmd 0.016 0 0 0 0").unwrap();
        let mut last = String::new();
        for _ in 0..60 {
            last = handle(&mut s, &format!("cmd 0.016 0 0 {IN_FORWARD} 0")).unwrap().unwrap();
        }
        let p = pos(&last);
        assert!(p[2] > -20.5 + 2.0, "facing yaw 0 is +z in Minecraft: {last}");
        assert!((p[0] - 100.5).abs() < 1e-3, "{last}");
        assert!((p[1] + 60.0).abs() < 0.01 && p[6] == 1.0, "{last}");
        assert!(p[4] > -60.0 + 1.0, "eye above the feet: {last}");
    }
}
