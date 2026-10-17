pub mod args;
pub mod blur;
pub mod audio;
pub mod data;
pub mod dev;
pub mod binds;
pub mod cmd;
pub mod convert;
pub mod convar;
pub mod cooldown;
pub mod hud;
pub mod install;
pub mod limiter;
pub mod miles;
pub mod rui;
pub mod look;
pub mod script;
pub mod sim;
pub mod view;
pub mod viewmodel;

use std::num::NonZeroU32;
use std::path::PathBuf;

use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy::window::WindowFocused;
use binds::{Binds, WheelMode};
use lurch::sim::buttons::{IN_FORWARD, IN_JUMP};
use lurch::sim::math::V3;
use script::{Script, SCRIPT_FRAMETIME};

pub use audio::AudioPlugin;
pub use cmd::CmdPlugin;
pub use data::DataPlugin;
pub use dev::DevPlugin;
pub use hud::HudPlugin;
pub use limiter::LimiterPlugin;
pub use rui::RuiPlugin;
pub use sim::SimPlugin;
pub use view::ViewPlugin;
pub use viewmodel::ViewmodelPlugin;

/// Every flag, inserted by `main` or a test before the plugins are added.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Config {
    pub spot: String,
    pub shot: Option<(u64, PathBuf)>,
    /// Only scripted input drives the run: no pause, no focus loss, no window input.
    pub scripted: bool,
    pub script: Option<Script>,
    pub frametime: f32,
    pub headless: bool,
    pub sens: f32,
    pub binds: Binds,
    pub wheel_down: Option<u32>,
    pub wheel_up: Option<u32>,
    pub wheel_mode: WheelMode,
    /// The player's `cl_fovScale` option.
    pub fov_scale: f32,
    /// 0 turns the cap off.
    pub fps_max: u32,
    pub vsync: bool,
    /// The sun casts shadows.
    pub shadows: bool,
    pub size: (u32, u32),
    pub frame_latency: Option<NonZeroU32>,
    pub pipelined: bool,
    /// The player's `player_setting_autosprint` option, on or off.
    pub autosprint: bool,
    /// The R5Reloaded install the game's models, animations and sounds are read from.
    pub r5r: Option<PathBuf>,
    /// The legend played, by its `footstep_type` (wraith, bangalore, ...): its sounds and arms.
    pub legend: String,
    /// The melee skin held in the fists' place, if any.
    pub heirloom: Option<&'static lurch::assets::Heirloom>,
    /// The player's `m_invert_pitch` option.
    pub invert_pitch: bool,
    /// The player's `fps_max` setting, when it caps the frame rate in place of `fps_max`.
    pub fps_max_setting: Option<f32>,
    /// `sound_volume`, then `sound_volume_voice`, `_music_lobby`, `_music_game`, `_dialogue`
    /// and `_sfx`, in the order of `audio::CONTROLLERS`.
    pub volumes: [f32; 6],
    /// The flags given on the command line itself, which win over the saved settings.
    pub cli: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            spot: "wall 48".into(),
            shot: None,
            scripted: false,
            script: None,
            frametime: SCRIPT_FRAMETIME,
            headless: false,
            sens: 5.0,
            binds: Binds::default(),
            wheel_down: Some(IN_FORWARD),
            wheel_up: Some(IN_JUMP),
            wheel_mode: WheelMode::Event,
            fov_scale: 1.27216,
            fps_max: 240,
            vsync: false,
            shadows: true,
            size: (1600, 900),
            frame_latency: None,
            pipelined: false,
            autosprint: false,
            r5r: None,
            legend: "wraith".into(),
            heirloom: None,
            invert_pitch: false,
            fps_max_setting: None,
            volumes: [1.0; 6],
            cli: Vec::new(),
        }
    }
}

/// Runs each schedule's systems one after another on the thread running it: in the guest,
/// waking the task pool's threads costs more than the systems they would run (unpipelined,
/// 68 fps multi-threaded against 110, 2026-10-13). Added last, so its `finish` sees every
/// schedule, the render app's included before pipelining takes it away.
pub struct SingleThreadedPlugin;

impl Plugin for SingleThreadedPlugin {
    fn build(&self, _: &mut App) {}

    fn finish(&self, app: &mut App) {
        let single = |world: &mut World| {
            for (_, schedule) in world.resource_mut::<bevy::ecs::schedule::Schedules>().iter_mut() {
                schedule.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
            }
        };
        single(app.world_mut());
        if let Some(render) = app.get_sub_app_mut(bevy::render::RenderApp) {
            single(render.world_mut());
        }
    }
}

/// Frame F is the F-th `app.update()`, counting from 0.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameIndex(pub u64);

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Paused(pub bool);

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudVisible(pub bool);

/// The game's menu on top (`uiGlobal.activeMenu`), none while playing: Escape opens
/// `SystemMenu`, its Settings button `MiscMenu`. The game is paused while one is open.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MenuOpen(pub Option<GameMenu>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameMenu {
    System,
    Misc,
}

impl MenuOpen {
    /// Escape in a match (`UICodeCallback_ToggleInGameMenu`): `SystemMenu` opens when no menu is
    /// open, else every menu closes (`CloseAllMenus`).
    pub fn toggle(&mut self) {
        self.0 = if self.0.is_some() { None } else { Some(GameMenu::System) };
    }
}

/// The shell's dev menu is open (`dev`); the game is paused while it is.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DevMenu(pub bool);

/// Where the cursor is over the menus, in the target's pixels: the window's cursor, or a
/// script's.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct MenuCursor(pub Option<(f32, f32)>);

/// Screenshots requested and not yet saved; a script waits for them before exiting.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PendingShots(pub usize);

impl Default for HudVisible {
    fn default() -> Self {
        HudVisible(true)
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Frame {
    Pause,
    Read,
    Respawn,
    Make,
    Run,
    Pose,
    View,
}

#[derive(Message, Clone, Debug, PartialEq)]
pub enum Respawn {
    Spot(isize),
    Named(String),
    At { origin: V3, yaw: f32 },
}

#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct LookReset {
    pub yaw: f32,
}

/// A sound name a menu handed the surface's PlaySound (`lurch::vgui::Menu::sounds`).
#[derive(Message, Clone, Debug, PartialEq)]
pub struct MenuSound(pub String);

/// The key bind list is capturing a key (the engine's key trap, input system `+0x608`): the
/// next key or button pressed is its, Escape included.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq)]
pub struct KeyTrap(pub bool);

pub fn config(app: &App) -> Config {
    app.world().get_resource::<Config>().cloned().unwrap_or_default()
}

/// What every plugin shares; runs once however many plugins call it.
pub fn common(app: &mut App) {
    if app.world().contains_resource::<FrameIndex>() {
        return;
    }
    app.init_resource::<Config>()
        .init_resource::<FrameIndex>()
        .init_resource::<Paused>()
        .init_resource::<HudVisible>()
        .init_resource::<MenuOpen>()
        .init_resource::<DevMenu>()
        .init_resource::<MenuCursor>()
        .init_resource::<KeyTrap>()
        .init_resource::<PendingShots>()
        .add_message::<Respawn>()
        .add_message::<LookReset>()
        .add_message::<MenuSound>()
        .add_message::<WindowFocused>()
        .configure_sets(PreUpdate, (Frame::Pause, Frame::Read).chain().after(InputSystems))
        .configure_sets(Update, (Frame::Respawn, Frame::Make, Frame::Run, Frame::Pose, Frame::View).chain())
        .add_systems(Last, advance_frame);
}

pub fn advance_frame(mut frame: ResMut<FrameIndex>) {
    frame.0 += 1;
}
