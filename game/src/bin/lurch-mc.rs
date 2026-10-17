//! The game shell as the other half of a Minecraft passthrough mod (`minecraft/mod`): Minecraft
//! keeps its world, window and server, and this runs the shell offscreen, one update per
//! Minecraft frame, with Minecraft's blocks as the collision world. Takes lurch-game's flags.
//!
//! One TCP connection on 127.0.0.1:25600 carries text lines from the mod:
//! - `k CODE 1|0`: a key went down or up; CODE is an SDL scancode, a USB HID usage.
//! - `m BUTTON 1|0`: a mouse button, in SDL's numbering (1 left, 2 middle, 3 right, 4, 5).
//! - `w Y`: the wheel turned Y notches.
//! - `l DX DY`: mouse motion, in counts.
//! - `r`: Minecraft took the input away, so every key is let go.
//! - `spawn X Y Z YAW MINY`: the player stands at X Y Z facing YAW; MINY is the world's floor.
//! - `blocks`, lines `f X Y Z MASK` (a full block and which of its faces are open: bits west,
//!   east, down, up, north, south) and `p X Y Z X0 Y0 Z0 X1 Y1 Z1` (a box of any other shape, in
//!   the block's coordinates), then `end`: every block near the player.
//! - `bind NAME k|m CODE`: Minecraft's key for one of lurch's actions, `tactical`, `ultimate`,
//!   `jump` or `crouch`, a key or a mouse button numbered as in `k` and `m`; `-` for none. It
//!   replaces every other key bound to that action.
//! - `hands 1|0`: the player's main hand is empty, or holds something. Holding something puts
//!   the fists away (`viewmodel::HostHands`) and moves at a speed scale of 1.0, a weapon's
//!   without `move_speed_modifier`, in place of the fists' 1.15.
//! - `hotbar X`: the left edge of Minecraft's hotbar, its offhand slot included, in window
//!   pixels. The tactical's panel moves left of it when it would reach it.
//! - `light R G B NETHER`: the lightmap colour Minecraft's own hand would be multiplied by where
//!   the player's eye is, gamma-encoded in 0 to 1, and 1 in the Nether's cardinal lighting.
//! - `frame FRAMETIME W H`: one frame at Minecraft's size, answered with
//!   `pose X Y Z EX EY EZ PITCH YAW ROLL FOV VPITCH VYAW GROUND ARMS`: the feet, the camera, its
//!   angles and vertical FOV in degrees, the view angles the player aims with, whether the
//!   pilot is on the ground and whether the shell drew arms this frame. When the HUD's
//!   layout changed, `hud X0 Y0 X1 Y1 ROOM` comes first: the box in the Apex player panel that
//!   Minecraft's armour, hearts and hunger go in (where the game draws the name and the bars;
//!   Y1 is the panel's bottom), and how far left the hotbar may reach and leave room for the
//!   tactical's panel beside the player panel, in window pixels; `-` for each one the shell
//!   does not draw.
//!
//! Positions and angles on the wire are Minecraft's (blocks, y up, its yaw). Each frame's image,
//! premultiplied RGBA rows `STRIDE` bytes apart, goes to `/dev/shm/lurch-mc` after a 20-byte
//! header: `LMC1`, a sequence number that is odd while the image is written, then the width,
//! height and stride, all little-endian u32.
//!
//! A spawn where the hull would start inside a block moves to the nearest clear place, up to
//! half a block away. `LURCH_MC_TRACE=1` prints every spawn and every frame's answer, with the
//! pilot's state, to stderr.
//!
//! The shell draws the player panel's frame and portrait where the game puts them, the
//! ultimate's panel at the right edge as their mirror, and the tactical's panel where the game
//! puts it or left of the hotbar (`rui::HostHud`).
//!
//! Not modelled: sounds, the shell's menus, the test map's respawn binds (dropped here).

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpListener;
use std::os::unix::fs::FileExt;
use std::str::{FromStr, SplitAsciiWhitespace};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::app::PluginsState;
use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::input::touch::TouchPhase;
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::render::render_resource::{TextureFormat, TextureUsages};
use bevy::render::RenderPlugin;
use bevy::tasks::tick_global_task_pools_on_main_thread;
use bevy::time::TimeUpdateStrategy;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use lurch::sim::math::V3;
use lurch::sim::pilot;
use lurch::sim::{HullQuery, Tracer};
use lurch::world::Tri;
use lurch::sim::buttons::{IN_DUCK, IN_JUMP, IN_OFFHAND1};
use lurch_game::binds::{Action, Input};
use lurch_game::cmd::{clamp_frametime, CmdBuilder, ScriptFrametime};
use lurch_game::convar::{self, ConVars};
use lurch_game::convert::vfov;
use lurch_game::data::{self, Data};
use lurch_game::rui::{HostBar, HostHud, Preloaded};
use lurch_game::sim::{CollisionWorld, Map, Pose, Sim, DIST_EPSILON};
use lurch_game::view::{Target, ViewImage};
use lurch_game::viewmodel::{ArmsShown, HostHands, HostLight};
use lurch_game::{args, install, CmdPlugin, Config, HudPlugin, Respawn, RuiPlugin, SimPlugin, SingleThreadedPlugin, ViewPlugin, ViewmodelPlugin};

const ADDR: &str = "127.0.0.1:25600";
const SHM: &str = "/dev/shm/lurch-mc";
const HEADER: u64 = 20;
/// A Source unit is an inch, a block a metre; 40 keeps block edges exact in f32 and makes the
/// standing hull (72) as tall as Steve.
const UNITS_PER_BLOCK: f64 = 40.0;
/// Below the world's floor, so the shell's kill height (`sim::KILL_Z`) is never reached above
/// Minecraft's own void.
const BASE_BELOW_FLOOR: f64 = 64.0;
/// The speed scale while the player holds something: a weapon's `move_speed_modifier` when its
/// settings give none.
const HOLDING_SPEED_SCALE: f32 = 1.0;
/// How far a spawn is moved to get clear of a block, in units: half a block.
const UNSTICK_REACH: u32 = 20;

/// Where lurch's origin is in Minecraft.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Base([f64; 3]);

impl Base {
    fn in_mc(self, v: V3) -> [f64; 3] {
        let b = |u: f32| u as f64 / UNITS_PER_BLOCK;
        [self.0[0] + b(v[0]), self.0[1] + b(v[2]), self.0[2] - b(v[1])]
    }

    fn in_lurch(self, p: [f64; 3]) -> V3 {
        let u = |d: f64| (d * UNITS_PER_BLOCK) as f32;
        [u(p[0] - self.0[0]), u(self.0[2] - p[2]), u(p[1] - self.0[1])]
    }
}

/// Minecraft's yaw for a Source yaw, and the other way round.
fn flip_yaw(yaw: f32) -> f32 {
    -yaw - 90.0
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Block {
    Full([i32; 3], u8),
    Part([i32; 3], [f64; 6]),
}

/// The open faces of every block, each two triangles wound counter-clockwise seen from outside.
fn tris(blocks: &[Block], base: Base) -> Vec<Tri> {
    let mut out = Vec::new();
    let mut face = |lo: [f64; 3], hi: [f64; 3], axis: usize, positive: bool| {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        let corner = |a: f64, b: f64| {
            let mut p = [0.0; 3];
            p[axis] = if positive { hi[axis] } else { lo[axis] };
            (p[u], p[v]) = (a, b);
            base.in_lurch(p)
        };
        let q = [corner(lo[u], lo[v]), corner(hi[u], lo[v]), corner(hi[u], hi[v]), corner(lo[u], hi[v])];
        let q = if positive { q } else { [q[0], q[3], q[2], q[1]] };
        out.push([q[0], q[1], q[2]]);
        out.push([q[0], q[2], q[3]]);
    };
    for b in blocks {
        let (at, lo, hi, mask) = match *b {
            Block::Full(at, mask) => (at, [0.0; 3], [1.0; 3], mask),
            Block::Part(at, s) => (at, [s[0], s[1], s[2]], [s[3], s[4], s[5]], 0x3f),
        };
        let at = at.map(f64::from);
        let lo = std::array::from_fn(|i| at[i] + lo[i]);
        let hi = std::array::from_fn(|i| at[i] + hi[i]);
        for f in 0..6 {
            if mask & (1 << f) != 0 {
                face(lo, hi, f / 2, f % 2 == 1);
            }
        }
    }
    out
}

/// The nearest place to `origin` where a standing hull spawns clear of the blocks: Steve is
/// narrower than the hull (0.6 blocks against 0.8), so where he stands the hull can be inside
/// a block, and a hull that starts inside one never moves.
fn free(world: &mut CollisionWorld, origin: V3) -> V3 {
    let p = pilot::spawn(origin);
    let mut clear = |o: V3| {
        let o = [o[0], o[1], o[2] + DIST_EPSILON];
        !world.hull(&HullQuery { start: o, end: o, mins: p.hull_min, maxs: p.hull_max, mask: p.mask }).startsolid
    };
    const DIRS: [V3; 10] = [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [1.0, 1.0, 0.0], [1.0, -1.0, 0.0], [-1.0, 1.0, 0.0], [-1.0, -1.0, 0.0], [0.0, 0.0, -1.0]];
    if clear(origin) {
        return origin;
    }
    (1..=UNSTICK_REACH).flat_map(|r| DIRS.map(|d| [origin[0] + d[0] * r as f32, origin[1] + d[1] * r as f32, origin[2] + d[2] * r as f32])).find(|&o| clear(o)).unwrap_or(origin)
}

/// Minecraft's two diffuse directions for entities in a level (`Lighting`), turned into Bevy's
/// world space, where Minecraft's (x, y, z) is (z, y, -x).
fn entity_light(nether: bool) -> [Vec3; 2] {
    let bevy = |x: f32, y: f32, z: f32| Vec3::new(z, y, -x).normalize();
    let below = if nether { -1.0 } else { 1.0 };
    [bevy(0.2, 1.0, -0.7), bevy(-0.2, below, 0.7)]
}

/// Bevy's key for an SDL scancode.
fn key(code: u32) -> Option<KeyCode> {
    use KeyCode::*;
    const LETTERS: [KeyCode; 26] = [KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ];
    const DIGITS: [KeyCode; 10] = [Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9, Digit0];
    const F: [KeyCode; 12] = [F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12];
    const NUMPAD: [KeyCode; 16] = [NumpadDivide, NumpadMultiply, NumpadSubtract, NumpadAdd, NumpadEnter, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9, Numpad0, NumpadDecimal];
    const MODS: [KeyCode; 8] = [ControlLeft, ShiftLeft, AltLeft, SuperLeft, ControlRight, ShiftRight, AltRight, SuperRight];
    let i = code as usize;
    Some(match code {
        4..=29 => LETTERS[i - 4],
        30..=39 => DIGITS[i - 30],
        40 => Enter,
        41 => Escape,
        42 => Backspace,
        43 => Tab,
        44 => Space,
        45 => Minus,
        46 => Equal,
        47 => BracketLeft,
        48 => BracketRight,
        49 => Backslash,
        51 => Semicolon,
        52 => Quote,
        53 => Backquote,
        54 => Comma,
        55 => Period,
        56 => Slash,
        57 => CapsLock,
        58..=69 => F[i - 58],
        73 => Insert,
        74 => Home,
        75 => PageUp,
        76 => Delete,
        77 => End,
        78 => PageDown,
        79 => ArrowRight,
        80 => ArrowLeft,
        81 => ArrowDown,
        82 => ArrowUp,
        84..=99 => NUMPAD[i - 84],
        224..=231 => MODS[i - 224],
        _ => return None,
    })
}

fn button(n: u32) -> Option<MouseButton> {
    Some(match n {
        1 => MouseButton::Left,
        2 => MouseButton::Middle,
        3 => MouseButton::Right,
        4 => MouseButton::Back,
        5 => MouseButton::Forward,
        _ => return None,
    })
}

/// The overlay image's size and the file it goes to, written whenever a readback lands.
#[derive(Resource)]
struct Overlay {
    size: (u32, u32),
    file: File,
    seq: u32,
}

impl Overlay {
    fn write(&mut self, data: &[u8]) {
        let (w, h) = self.size;
        let stride = (w * 4).next_multiple_of(256);
        if data.len() != (stride * h) as usize {
            return;
        }
        self.seq = self.seq.wrapping_add(1) | 1;
        let mut head = b"LMC1".to_vec();
        for v in [self.seq, w, h, stride] {
            head.extend(v.to_le_bytes());
        }
        let _ = self.file.write_at(&head, 0);
        let _ = self.file.write_at(data, HEADER);
        self.seq = self.seq.wrapping_add(1);
        let _ = self.file.write_at(&self.seq.to_le_bytes(), 4);
    }
}

fn target_image(w: u32, h: u32) -> Image {
    let mut image = Image::new_target_texture(w, h, TextureFormat::Rgba8UnormSrgb, None);
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    image
}

fn arg<T: FromStr>(w: &mut SplitAsciiWhitespace) -> io::Result<T> {
    w.next().and_then(|s| s.parse().ok()).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing or bad argument"))
}

fn input_message(world: &mut World, bound: impl Fn(Input) -> Option<Action>, input: Input, state: ButtonState) {
    if matches!(bound(input), Some(Action::Respawn | Action::PrevSpot | Action::NextSpot | Action::DevMenu)) {
        return;
    }
    match input {
        Input::Key(key_code) => drop(world.write_message(KeyboardInput { key_code, logical_key: Key::Unidentified(NativeKey::Unidentified), state, text: None, repeat: false, window: Entity::PLACEHOLDER })),
        Input::Mouse(button) => drop(world.write_message(MouseButtonInput { button, state, window: Entity::PLACEHOLDER })),
    }
}

fn pose(world: &World, base: Base) -> String {
    let sim = world.resource::<Sim>();
    let eye = world.resource::<Pose>().0;
    let [x, y, z] = base.in_mc(sim.pilot.origin);
    let [ex, ey, ez] = base.in_mc(eye.origin);
    let (a, v) = (eye.angles, sim.pilot.player.view_angles);
    let fov = vfov(eye.fov).to_degrees();
    let ground = sim.pilot.player.ground.is_some() as u8;
    let arms = world.get_resource::<ArmsShown>().is_some_and(|a| a.0) as u8;
    format!("pose {x} {y} {z} {ex} {ey} {ez} {} {} {} {fov} {} {} {ground} {arms}", a[0], flip_yaw(a[1]), a[2], v[0], flip_yaw(v[1]))
}

/// The `hud` line for a layout.
fn hud(h: &HostHud) -> String {
    let num = |v: Option<f32>| v.map_or("-".to_string(), |v| v.to_string());
    let panel = h.panel.map_or_else(|| ["-"; 4].join(" "), |p| p.map(|v| v.to_string()).join(" "));
    format!("hud {panel} {}", num(h.room))
}

/// Runs one connection's frames until the mod hangs up. `hands_speed` is the pilot's own
/// speed scale, the fists'.
fn serve(app: &mut App, stream: &std::net::TcpStream, hands_speed: f32) -> io::Result<()> {
    let mut out = stream;
    let mut lines = BufReader::new(stream).lines();
    let mut base = Base::default();
    let mut blocks = Vec::new();
    let mut sent_hud = None;
    let trace = std::env::var_os("LURCH_MC_TRACE").is_some();
    let bound = { let binds = app.world().resource::<Config>().binds.clone(); move |i| binds.get(i) };
    app.world_mut().resource_mut::<CmdBuilder>().release_all();
    while let Some(line) = lines.next() {
        let line = line?;
        let mut w = line.split_ascii_whitespace();
        let state = |down: u8| if down == 1 { ButtonState::Pressed } else { ButtonState::Released };
        let world = app.world_mut();
        match w.next() {
            Some("k") => {
                let (code, down): (u32, u8) = (arg(&mut w)?, arg(&mut w)?);
                if let Some(k) = key(code) {
                    input_message(world, &bound, Input::Key(k), state(down));
                }
            }
            Some("m") => {
                let (n, down): (u32, u8) = (arg(&mut w)?, arg(&mut w)?);
                if let Some(b) = button(n) {
                    input_message(world, &bound, Input::Mouse(b), state(down));
                }
            }
            Some("w") => drop(world.write_message(MouseWheel { unit: MouseScrollUnit::Line, x: 0.0, y: arg(&mut w)?, window: Entity::PLACEHOLDER, phase: TouchPhase::Moved })),
            Some("l") => drop(world.write_message(MouseMotion { delta: Vec2::new(arg(&mut w)?, arg(&mut w)?) })),
            Some("r") => world.resource_mut::<CmdBuilder>().release_all(),
            Some("bind") => {
                let action = match w.next() {
                    Some("tactical") => Action::Bits(IN_OFFHAND1),
                    Some("ultimate") => Action::Ultimate,
                    Some("jump") => Action::Bits(IN_JUMP),
                    Some("crouch") => Action::Bits(IN_DUCK),
                    _ => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("bad bind: {line:?}"))),
                };
                let input = match w.next() {
                    Some("k") => arg(&mut w).ok().and_then(key).map(Input::Key),
                    Some("m") => arg(&mut w).ok().and_then(button).map(Input::Mouse),
                    _ => None,
                };
                world.resource_mut::<Config>().binds.rebind(action, input);
            }
            Some("hands") => {
                let empty = arg::<u8>(&mut w)? == 1;
                world.insert_resource(HostHands(empty));
            }
            Some("hotbar") => {
                let left = arg(&mut w)?;
                world.insert_resource(HostBar(left));
            }
            Some("light") => {
                let color = Vec3::new(arg(&mut w)?, arg(&mut w)?, arg(&mut w)?);
                let nether: u8 = arg(&mut w)?;
                world.insert_resource(HostLight { color, dirs: entity_light(nether == 1) });
            }
            Some("spawn") => {
                if trace {
                    eprintln!("trace {line}");
                }
                let p: [f64; 3] = [arg(&mut w)?, arg(&mut w)?, arg(&mut w)?];
                let (yaw, floor): (f32, f64) = (arg(&mut w)?, arg(&mut w)?);
                base = Base([p[0].floor(), floor - BASE_BELOW_FLOOR, p[2].floor()]);
                let mut map = CollisionWorld::from_tris(tris(&blocks, base));
                let origin = free(&mut map, base.in_lurch(p));
                world.resource_mut::<Map>().0 = map;
                world.write_message(Respawn::At { origin, yaw: flip_yaw(yaw) });
            }
            Some("blocks") => {
                blocks.clear();
                for line in lines.by_ref() {
                    let line = line?;
                    let mut w = line.split_ascii_whitespace();
                    match w.next() {
                        Some("f") => blocks.push(Block::Full([arg(&mut w)?, arg(&mut w)?, arg(&mut w)?], arg(&mut w)?)),
                        Some("p") => blocks.push(Block::Part([arg(&mut w)?, arg(&mut w)?, arg(&mut w)?], std::array::from_fn(|_| arg(&mut w).unwrap_or(0.0)))),
                        _ => break,
                    }
                }
                app.world_mut().resource_mut::<Map>().0 = CollisionWorld::from_tris(tris(&blocks, base));
            }
            Some("frame") => {
                let (ft, width, height): (f64, u32, u32) = (arg(&mut w)?, arg(&mut w)?, arg(&mut w)?);
                world.insert_resource(ScriptFrametime(clamp_frametime(ft)));
                world.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(ft.max(1e-6))));
                if world.resource::<Overlay>().size != (width, height) && width > 0 && height > 0 {
                    let handle = world.resource::<ViewImage>().0.clone();
                    let _ = world.resource_mut::<Assets<Image>>().insert(handle.id(), target_image(width, height));
                    world.resource_mut::<Overlay>().size = (width, height);
                }
                if let Some(h) = world.get_resource::<HostHands>().copied() {
                    world.resource_mut::<Sim>().pilot.player.speed_scale = if h.0 { hands_speed } else { HOLDING_SPEED_SCALE };
                }
                app.update();
                let layout = *app.world().resource::<HostHud>();
                if sent_hud != Some(layout) {
                    writeln!(out, "{}", hud(&layout))?;
                    sent_hud = Some(layout);
                }
                let answer = pose(app.world(), base);
                if trace {
                    let sim = app.world().resource::<Sim>();
                    let p = &sim.pilot.player;
                    eprintln!("trace {answer} | origin {:?} movetype {} traversal {} {} view_entity {} {} ground {:?}", sim.pilot.origin, p.movetype, p.traversal_state, p.traversal_type, p.view_entity, p.view_entity_blend, p.ground);
                }
                writeln!(out, "{answer}")?;
            }
            _ => return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unknown message: {line:?}"))),
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let cfg_path = std::env::current_exe().ok().map(|p| p.with_file_name("lurch.cfg"));
    let file = cfg_path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    let mut cfg = args::parse(args::file_args(&file).into_iter().chain(std::env::args().skip(1))).unwrap_or_else(|e| {
        eprintln!("{e}\n{}", args::USAGE);
        std::process::exit(2);
    });
    cfg.cli = std::env::args().skip(1).filter(|a| a.starts_with("--")).collect();
    cfg.r5r = install::resolve(cfg.r5r.take(), false, cfg_path.as_deref());
    let convars = match cfg_path.as_ref().and_then(|p| p.parent()) {
        Some(dir) => ConVars::load(dir.to_path_buf(), convar::game_dir()),
        None => ConVars::default(),
    };
    convars.apply(&mut cfg);
    cfg.scripted = true;
    let data = cfg.r5r.as_deref().and_then(|i| data::load(i, &cfg.legend, cfg.heirloom).map_err(|e| eprintln!("lurch-mc: no game data: {e}")).ok());
    let ui = cfg.r5r.as_deref().and_then(|i| lurch::rui::Ui::open(i).map_err(|e| eprintln!("lurch-mc: no UI: {e}")).ok());
    let (w, h) = (854, 480);
    let mut app = App::new();
    app.insert_resource(cfg).insert_resource(convars).insert_resource(Preloaded(Mutex::new(ui))).insert_resource(HostLight { color: Vec3::ONE, dirs: entity_light(false) }).init_resource::<HostHud>();
    if let Some(d) = data {
        app.insert_resource(Data(Arc::new(d)));
    }
    app.insert_resource(Overlay { size: (w, h), file: File::options().read(true).write(true).create(true).truncate(true).open(SHM)?, seq: 0 }).add_plugins((
        DefaultPlugins
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(RenderPlugin { synchronous_pipeline_compilation: true, ..default() })
            .disable::<WinitPlugin>()
            .disable::<PipelinedRenderingPlugin>(),
        CmdPlugin,
        SimPlugin,
        ViewPlugin { target: Target::Overlay { w, h } },
        ViewmodelPlugin,
        HudPlugin,
        RuiPlugin,
        SingleThreadedPlugin,
    ));
    while app.plugins_state() == PluginsState::Adding {
        tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();
    app.update();
    let image = app.world().resource::<ViewImage>().0.clone();
    app.world_mut().spawn(Readback::texture(image)).observe(|done: On<ReadbackComplete>, mut overlay: ResMut<Overlay>| overlay.write(&done.data));
    let hands_speed = app.world().resource::<Sim>().pilot.player.speed_scale;
    let listener = TcpListener::bind(ADDR)?;
    eprintln!("lurch-mc: listening on {ADDR}, overlay in {SHM}");
    for stream in listener.incoming() {
        let stream = stream?;
        stream.set_nodelay(true)?;
        eprintln!("lurch-mc: Minecraft connected");
        match serve(&mut app, &stream, hands_speed) {
            Ok(()) => eprintln!("lurch-mc: Minecraft left"),
            Err(e) => eprintln!("lurch-mc: {e}"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blocks_top_face_is_floor_facing_up_at_its_height() {
        let base = Base([10.0, -128.0, -5.0]);
        let t = tris(&[Block::Full([10, -61, -5], 1 << 3)], base);
        assert_eq!(t.len(), 2);
        for [a, b, c] in t {
            let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
            assert!(u[0] * v[1] - u[1] * v[0] > 0.0, "normal points up");
            assert!([a, b, c].iter().all(|p| p[2] == 68.0 * 40.0));
        }
        let p = [10.5, -60.0, -4.25];
        assert_eq!(base.in_mc(base.in_lurch(p)), p);
        assert_eq!(flip_yaw(flip_yaw(30.0)), 30.0);
    }

    #[test]
    fn a_spawn_where_steve_fits_but_the_hull_does_not_moves_clear() {
        let base = Base([0.0, -128.0, 0.0]);
        let mut blocks: Vec<Block> = (-3..3).flat_map(|x| (-3..3).map(move |z| Block::Full([x, -61, z], 1 << 3))).collect();
        blocks.push(Block::Full([1, -60, 0], 0x3f));
        let mut world = CollisionWorld::from_tris(tris(&blocks, base));
        let steve = base.in_lurch([0.65, -60.0, 0.5]);
        let o = free(&mut world, steve);
        assert_ne!(o, steve);
        assert!(base.in_mc(o)[0] <= 0.6 + 1e-6, "{:?}", base.in_mc(o));
    }
}

