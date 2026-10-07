pub mod args;
pub mod binds;
pub mod cmd;
pub mod hud;
pub mod limiter;
pub mod look;
pub mod sim;
pub mod view;

use std::path::PathBuf;

use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy::window::WindowFocused;
use binds::{Binds, WheelMode};
use lurch::sim::buttons::{IN_FORWARD, IN_JUMP};
use lurch::sim::math::V3;

pub use cmd::CmdPlugin;
pub use hud::HudPlugin;
pub use limiter::LimiterPlugin;
pub use sim::SimPlugin;
pub use view::ViewPlugin;

/// Every flag, inserted by `main` or a test before the plugins are added.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Config {
    pub spot: String,
    pub shot: Option<(u64, PathBuf)>,
    /// Only scripted input drives the run: no pause, no focus loss, no window input.
    pub scripted: bool,
    pub sens: f32,
    pub binds: Binds,
    pub wheel_down: Option<u32>,
    pub wheel_up: Option<u32>,
    pub wheel_mode: WheelMode,
    /// The player's `cl_fovScale` option.
    pub fov_scale: f32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            spot: "wall 48".into(),
            shot: None,
            scripted: false,
            sens: 5.0,
            binds: Binds::default(),
            wheel_down: Some(IN_FORWARD),
            wheel_up: Some(IN_JUMP),
            wheel_mode: WheelMode::Event,
            fov_scale: 1.27216,
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
        .add_message::<Respawn>()
        .add_message::<LookReset>()
        .add_message::<WindowFocused>()
        .configure_sets(PreUpdate, (Frame::Pause, Frame::Read).chain().after(InputSystems))
        .configure_sets(Update, (Frame::Respawn, Frame::Make, Frame::Run, Frame::Pose, Frame::View).chain())
        .add_systems(Last, advance_frame);
}

pub fn advance_frame(mut frame: ResMut<FrameIndex>) {
    frame.0 += 1;
}
