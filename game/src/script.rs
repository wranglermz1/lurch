use std::time::Duration;

use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::input::touch::TouchPhase;
use bevy::input::{ButtonState, InputSystems};
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;

use crate::cmd::{CmdBuilder, CmdLog, ScriptFrametime};
use crate::sim::Sim;
use crate::{advance_frame, common, config, FrameIndex, MenuCursor, MenuOpen, Paused, PendingShots, Respawn};

/// The frametime of `tests/testmap.rs`.
pub const SCRIPT_FRAMETIME: f32 = 0.004;

pub const KEYS: [(&str, KeyCode); 14] = [
    ("KeyW", KeyCode::KeyW),
    ("KeyA", KeyCode::KeyA),
    ("KeyS", KeyCode::KeyS),
    ("KeyD", KeyCode::KeyD),
    ("Space", KeyCode::Space),
    ("ControlLeft", KeyCode::ControlLeft),
    ("ShiftLeft", KeyCode::ShiftLeft),
    ("KeyC", KeyCode::KeyC),
    ("KeyE", KeyCode::KeyE),
    ("KeyQ", KeyCode::KeyQ),
    ("KeyR", KeyCode::KeyR),
    ("BracketLeft", KeyCode::BracketLeft),
    ("BracketRight", KeyCode::BracketRight),
    ("F1", KeyCode::F1),
];

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Down(KeyCode),
    Up(KeyCode),
    Tap(KeyCode),
    Wheel(f32),
    Look(f32, f32),
    Spot(String),
    /// Escape as the menus take it: the in-match menu opens, or closes.
    Menu,
    /// The cursor over the menus, in the target's pixels.
    Cursor(f32, f32),
    /// The left mouse button pressed and released.
    Click,
    Exit,
}

/// Input by frame: frame F is the F-th update, and frame 0 makes no command.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Script(pub Vec<(u64, Step)>);

impl Script {
    pub fn exit_frame(&self) -> Option<u64> {
        self.0.iter().find(|(_, s)| *s == Step::Exit).map(|(f, _)| *f)
    }
}

pub fn parse(text: &str) -> Result<Script, String> {
    let mut steps = Vec::new();
    let mut last = 0;
    for (i, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let at = |e: String| format!("line {}: {e}", i + 1);
        let mut words = line.split_whitespace();
        let frame: u64 = words.next().unwrap_or("").parse().map_err(|_| at("frame is not a number".into()))?;
        if frame < last {
            return Err(at(format!("frame {frame} goes back from {last}")));
        }
        last = frame;
        let action = words.next().ok_or_else(|| at("no action".into()))?;
        let args: Vec<&str> = words.collect();
        let step = match (action, args.as_slice()) {
            ("down", [k]) => Step::Down(key(k).map_err(at)?),
            ("up", [k]) => Step::Up(key(k).map_err(at)?),
            ("tap", [k]) => Step::Tap(key(k).map_err(at)?),
            ("wheel", [n]) => Step::Wheel(number(n).map_err(at)?),
            ("look", [dx, dy]) => Step::Look(number(dx).map_err(at)?, number(dy).map_err(at)?),
            ("spot", name) if !name.is_empty() => Step::Spot(name.join(" ")),
            ("menu", []) => Step::Menu,
            ("cursor", [x, y]) => Step::Cursor(number(x).map_err(at)?, number(y).map_err(at)?),
            ("click", []) => Step::Click,
            ("exit", []) => Step::Exit,
            _ => return Err(at(format!("bad action: {line}"))),
        };
        steps.push((frame, step));
    }
    Ok(Script(steps))
}

fn key(name: &str) -> Result<KeyCode, String> {
    KEYS.iter().find(|(n, _)| *n == name).map(|(_, k)| *k).ok_or_else(|| format!("unknown key {name}"))
}

fn number(s: &str) -> Result<f32, String> {
    s.parse::<f32>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("not a number: {s}"))
}

pub fn parse_frametime(s: &str) -> Result<f32, String> {
    s.parse::<f32>().ok().filter(|v| v.is_finite() && *v > 0.0).ok_or_else(|| format!("--frametime wants seconds above 0, got {s}"))
}

pub fn final_line(sim: &Sim, b: &CmdBuilder) -> String {
    format!("final cmd={} origin={:?} velocity={:?} clock={:?}", b.next_number - 1, sim.pilot.origin, sim.pilot.velocity, sim.clock.secs())
}

pub struct ScriptPlugin(pub Script);

#[derive(Resource, Clone, Debug)]
struct Running {
    script: Script,
    next: usize,
    ended: bool,
}

impl Plugin for ScriptPlugin {
    fn build(&self, app: &mut App) {
        common(app);
        let ft = config(app).frametime;
        app.insert_resource(ScriptFrametime(ft))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(ft as f64)))
            .init_resource::<CmdLog>()
            .insert_resource(Running { script: self.0.clone(), next: 0, ended: false })
            .add_systems(PreUpdate, inject_script.before(InputSystems))
            .add_systems(Last, script_end.before(advance_frame));
    }
}

fn press(key_code: KeyCode, state: ButtonState) -> KeyboardInput {
    KeyboardInput { key_code, logical_key: Key::Unidentified(NativeKey::Unidentified), state, text: None, repeat: false, window: Entity::PLACEHOLDER }
}

#[allow(clippy::too_many_arguments)] // Bevy's system parameters
fn inject_script(
    frame: Res<FrameIndex>,
    mut run: ResMut<Running>,
    mut keys: MessageWriter<KeyboardInput>,
    mut wheel: MessageWriter<MouseWheel>,
    mut motion: MessageWriter<MouseMotion>,
    mut buttons: MessageWriter<MouseButtonInput>,
    mut respawn: MessageWriter<Respawn>,
    (mut menu, mut paused, mut cursor): (ResMut<MenuOpen>, ResMut<Paused>, ResMut<MenuCursor>),
) {
    while let Some((f, step)) = run.script.0.get(run.next).cloned() {
        if f > frame.0 {
            break;
        }
        run.next += 1;
        if f < frame.0 {
            continue;
        }
        match step {
            Step::Down(k) => drop(keys.write(press(k, ButtonState::Pressed))),
            Step::Up(k) => drop(keys.write(press(k, ButtonState::Released))),
            Step::Tap(k) => {
                keys.write(press(k, ButtonState::Pressed));
                keys.write(press(k, ButtonState::Released));
            }
            Step::Wheel(y) => drop(wheel.write(MouseWheel { unit: MouseScrollUnit::Line, x: 0.0, y, window: Entity::PLACEHOLDER, phase: TouchPhase::Moved })),
            Step::Look(dx, dy) => drop(motion.write(MouseMotion { delta: Vec2::new(dx, dy) })),
            Step::Spot(name) => drop(respawn.write(Respawn::Named(name))),
            Step::Menu => {
                menu.toggle();
                paused.0 = menu.0.is_some();
            }
            Step::Cursor(x, y) => cursor.0 = Some((x, y)),
            Step::Click => {
                for state in [ButtonState::Pressed, ButtonState::Released] {
                    buttons.write(MouseButtonInput { button: MouseButton::Left, state, window: Entity::PLACEHOLDER });
                }
            }
            Step::Exit => {}
        }
    }
}

fn script_end(frame: Res<FrameIndex>, mut run: ResMut<Running>, sim: Res<Sim>, b: Res<CmdBuilder>, shots: Res<PendingShots>, mut paused: ResMut<Paused>, mut exit: MessageWriter<AppExit>) {
    let Some(end) = run.script.exit_frame() else { return };
    if frame.0 < end {
        return;
    }
    if !run.ended {
        run.ended = true;
        println!("{}", final_line(&sim, &b));
        paused.0 = true;
    }
    if shots.0 == 0 {
        exit.write(AppExit::Success);
    }
}
