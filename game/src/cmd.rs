use std::collections::VecDeque;

use bevy::input::keyboard::{KeyboardFocusLost, KeyboardInput};
use bevy::input::mouse::{AccumulatedMouseMotion, MouseButtonInput, MouseWheel};
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowFocused};
use lurch::sim::input::Keys;
use lurch::sim::setup::{command_seed, UserCmd};

use crate::binds::{Action, Held, Input, Wheel};
use crate::look::Look;
use crate::sim::Sim;
use crate::{common, config, Config, DevMenu, Frame, FrameIndex, HudVisible, KeyTrap, LookReset, MenuOpen, Paused, Respawn};

/// The retail `usercmd_frametime_min`/`_max` ConVars. The oracle's own clamp lives in
/// R5Reloaded's SDK, outside the image (docs/oracle.md), and these bound every recording.
pub const RETAIL_USERCMD_FRAMETIME_MIN: f32 = 0.002857;
pub const RETAIL_USERCMD_FRAMETIME_MAX: f32 = 0.1;

/// Never 0 (the sprint tilt divides by it) and never NaN, which `f32::clamp` would pass on.
#[allow(clippy::manual_clamp)]
pub fn clamp_frametime(dt: f64) -> f32 {
    (dt as f32).max(RETAIL_USERCMD_FRAMETIME_MIN).min(RETAIL_USERCMD_FRAMETIME_MAX)
}

#[derive(Resource, Clone, Debug)]
pub struct CmdBuilder {
    pub keys: Keys,
    pub held: Held,
    pub look: Look,
    pub wheel: Wheel,
    pub next_number: u32,
    /// The last command made, which the pilot has run by the time the next is made.
    pub last: UserCmd,
}

impl CmdBuilder {
    pub fn new(sens: f32) -> Self {
        CmdBuilder { keys: Keys::default(), held: Held::default(), look: Look::new(sens), wheel: Wheel::default(), next_number: 1, last: UserCmd::default() }
    }

    pub fn release_all(&mut self) {
        self.held.clear();
        self.keys.release_all();
        self.wheel.reset();
    }

    fn tap(&mut self, bits: u32) {
        self.keys.press(bits);
        self.keys.release(bits & !self.held.bits());
    }
}

/// Commands waiting for the simulation: whatever pushes here drives the pilot.
#[derive(Resource, Clone, Debug, Default)]
pub struct CmdQueue(pub VecDeque<UserCmd>);

/// Every command made, with the frame that made it. Only tests, scripts and headless runs
/// insert it, so play does not grow it without bound.
#[derive(Resource, Clone, Debug, Default)]
pub struct CmdLog(pub Vec<(u64, UserCmd)>);

/// The exact frametime a script asked for; Bevy's Duration would round it.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct ScriptFrametime(pub f32);

pub struct CmdPlugin;

impl Plugin for CmdPlugin {
    fn build(&self, app: &mut App) {
        common(app);
        let cfg = config(app);
        app.insert_resource(CmdBuilder::new(cfg.sens))
            .init_resource::<CmdQueue>()
            .add_systems(PreUpdate, (look_settings, read_input).chain().in_set(Frame::Read))
            .add_systems(Update, (apply_look_reset, make_cmd).chain().in_set(Frame::Make));
        if !cfg.scripted {
            app.add_systems(PreUpdate, pause_input.in_set(Frame::Pause));
        }
    }
}

/// Escape opens the in-match menu (`MenuOpen::toggle`), which pauses, and closes whichever menu
/// is open, but while the key bind list captures a key. Opening puts the cursor at the
/// screen's centre (`UpdateSystemPanel`). Losing the focus pauses without the menu, and a click
/// then resumes, unless the dev menu is open.
#[allow(clippy::too_many_arguments)] // Bevy's system parameters
pub fn pause_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut focus: MessageReader<WindowFocused>,
    mut paused: ResMut<Paused>,
    mut menu: ResMut<MenuOpen>,
    mut b: ResMut<CmdBuilder>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    trap: Res<KeyTrap>,
    dev: Res<DevMenu>,
) {
    let lost = focus.read().fold(false, |lost, f| lost | !f.focused);
    if keys.just_pressed(KeyCode::Escape) && !trap.0 {
        menu.toggle();
        paused.set_if_neq(Paused(menu.0.is_some()));
        if menu.0.is_some() {
            b.release_all();
            for mut w in &mut windows {
                let centre = Vec2::new(w.width(), w.height()) * 0.5;
                w.set_cursor_position(Some(centre));
            }
        }
    } else if lost {
        paused.set_if_neq(Paused(true));
        b.release_all();
    } else if paused.0 && menu.0.is_none() && !dev.0 && mouse.just_pressed(MouseButton::Left) {
        paused.0 = false;
    }
}

#[allow(clippy::too_many_arguments)]
fn read_input(
    mut keys: MessageReader<KeyboardInput>,
    mut buttons: MessageReader<MouseButtonInput>,
    mut wheel: MessageReader<MouseWheel>,
    mut lost: MessageReader<KeyboardFocusLost>,
    motion: Res<AccumulatedMouseMotion>,
    cfg: Res<Config>,
    paused: Res<Paused>,
    mut b: ResMut<CmdBuilder>,
    mut respawn: MessageWriter<Respawn>,
    mut hud: ResMut<HudVisible>,
) {
    let ours = |window: Entity| !cfg.scripted || window == Entity::PLACEHOLDER;
    let mut actions = Vec::new();
    for k in keys.read().filter(|k| ours(k.window) && !k.repeat) {
        actions.extend(on_input(&mut b, &cfg, paused.0, Input::Key(k.key_code), k.state));
    }
    for m in buttons.read().filter(|m| ours(m.window)) {
        actions.extend(on_input(&mut b, &cfg, paused.0, Input::Mouse(m.button), m.state));
    }
    for w in wheel.read().filter(|w| ours(w.window)) {
        let taps = b.wheel.taps(cfg.wheel_mode, w.unit, w.y);
        let bits = if taps < 0 { cfg.wheel_down } else if taps > 0 { cfg.wheel_up } else { None };
        if let (Some(bits), false) = (bits, paused.0) {
            b.tap(bits);
        }
    }
    if lost.read().count() > 0 && !cfg.scripted {
        b.release_all();
    }
    if !paused.0 {
        b.look.apply(motion.delta.x, motion.delta.y);
    }
    for action in actions {
        match action {
            Action::Respawn => drop(respawn.write(Respawn::Spot(0))),
            Action::PrevSpot => drop(respawn.write(Respawn::Spot(-1))),
            Action::NextSpot => drop(respawn.write(Respawn::Spot(1))),
            Action::ToggleHud => hud.0 = !hud.0,
            Action::Bits(_) | Action::Ping | Action::Ultimate | Action::DevMenu => {}
        }
    }
}

/// The mouse settings, taken again when they change.
fn look_settings(cfg: Res<Config>, mut b: ResMut<CmdBuilder>) {
    if cfg.is_changed() {
        (b.look.sens, b.look.invert) = (cfg.sens, cfg.invert_pitch);
    }
}

/// Applies a press or release to the held set and the keys; returns an action to perform.
fn on_input(b: &mut CmdBuilder, cfg: &Config, paused: bool, input: Input, state: ButtonState) -> Option<Action> {
    match state {
        ButtonState::Pressed => {
            let action = cfg.binds.get(input).filter(|_| !paused)?;
            let bits = if let Action::Bits(bits) = action { bits } else { 0 };
            if !b.held.press(input, bits) {
                return None;
            }
            b.keys.press(bits);
            Some(action)
        }
        ButtonState::Released => {
            let bits = b.held.release(input)?;
            b.keys.release(bits & !b.held.bits());
            None
        }
    }
}

fn apply_look_reset(mut resets: MessageReader<LookReset>, mut b: ResMut<CmdBuilder>) {
    if let Some(r) = resets.read().last() {
        b.look.reset(r.yaw);
    }
}

#[allow(clippy::too_many_arguments)]
fn make_cmd(time: Res<Time<Real>>, script: Option<Res<ScriptFrametime>>, paused: Res<Paused>, frame: Res<FrameIndex>, cfg: Res<Config>, sim: Option<Res<Sim>>, mut b: ResMut<CmdBuilder>, mut queue: ResMut<CmdQueue>, log: Option<ResMut<CmdLog>>) {
    if paused.0 || time.delta().is_zero() {
        return;
    }
    let frametime = match script {
        Some(f) => f.0,
        None => clamp_frametime(time.delta_secs_f64()),
    };
    let n = b.next_number;
    b.next_number += 1;
    let mut cmd = UserCmd { command_number: n, angles: b.look.angles(), frametime, random_seed: command_seed(n), ..default() };
    let last = b.last;
    b.keys.fill(&mut cmd, sim.as_ref().filter(|_| cfg.autosprint).map(|s| (&s.pilot.player, &last)));
    b.last = cmd;
    if let Some(mut log) = log {
        log.0.push((frame.0, cmd));
    }
    queue.0.push_back(cmd);
}
