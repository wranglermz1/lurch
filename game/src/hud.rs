use std::fmt::Write;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use lurch::sim::math::is_world_up;

use crate::binds::WheelMode;
use crate::cmd::CmdBuilder;
use crate::sim::{Pose, Sim, Spots};
use crate::view::DisplayInfo;
use crate::{common, Config, Frame, HudVisible, Paused};

/// One inch per second in km/h.
pub const KMH_PER_UPS: f32 = 0.09144;
const FPS_WINDOW: f64 = 0.5;

#[derive(Component)]
pub struct HudText;

#[derive(Component)]
pub struct Crosshair;

/// Frames and real time since the fps figure was last refreshed.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct FpsMeter {
    frames: u32,
    secs: f64,
    pub fps: f64,
    pub frame_ms: f64,
}

impl FpsMeter {
    pub fn tick(&mut self, dt: f64) {
        self.frames += 1;
        self.secs += dt;
        if self.secs >= FPS_WINDOW {
            self.fps = self.frames as f64 / self.secs;
            self.frame_ms = self.secs * 1000.0 / self.frames as f64;
            self.frames = 0;
            self.secs = 0.0;
        }
    }
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        common(app);
        app.init_resource::<FpsMeter>().init_resource::<DisplayInfo>().add_systems(Startup, spawn_hud).add_systems(Update, hud.in_set(Frame::View));
    }
}

fn spawn_hud(mut commands: Commands) {
    commands.spawn((HudText, Text::new(""), TextFont::from_font_size(16.0), Node { position_type: PositionType::Absolute, top: px(8), left: px(8), ..default() }));
    commands.spawn((
        Crosshair,
        Node { position_type: PositionType::Absolute, left: percent(50), top: percent(50), width: px(4), height: px(4), margin: UiRect::all(px(-2)), ..default() },
        BackgroundColor(Color::WHITE),
    ));
}

/// Everything the HUD reads.
pub struct HudView<'a> {
    pub sim: &'a Sim,
    pub pose: &'a Pose,
    pub builder: &'a CmdBuilder,
    pub cfg: &'a Config,
    pub spots: &'a Spots,
    pub fps: &'a FpsMeter,
    pub display: &'a DisplayInfo,
    pub paused: bool,
}

/// The HUD's text for this frame.
pub fn hud_text(h: &HudView) -> String {
    let (cfg, fps) = (h.cfg, h.fps);
    let p = &h.sim.pilot;
    let v = p.velocity;
    let speed = v[0].hypot(v[1]);
    let on = |set: bool, name: &'static str| if set { name } else { "" };
    let place = if !p.player.on_ground() {
        "air"
    } else if is_world_up(p.player.up) {
        "ground"
    } else {
        "wall"
    };
    let wheel = match cfg.wheel_mode {
        WheelMode::Event => "event",
        WheelMode::Notch => "notch",
    };
    let mut s = String::new();
    let _ = writeln!(s, "speed {speed:.0} u/s  {:.1} km/h   vz {:.0}", speed * KMH_PER_UPS, v[2]);
    let _ = writeln!(s, "{place}   {}  {}  {}", on(p.player.sliding, "slide"), on(p.player.sprinting, "sprint"), on(p.player.ducked(), "duck"));
    let _ = writeln!(s, "fov {:.0}  sens {:.1}  fov-scale {:.3}  wheel {wheel}", h.pose.0.fov, cfg.sens, cfg.fov_scale);
    let _ = writeln!(s, "fps {:.0}  ft {:.2} ms  cmd {}  spot {}", fps.fps, fps.frame_ms, h.builder.next_number - 1, h.spots.spot().name);
    let _ = write!(s, "present {}  adapter {}", h.display.present, h.display.adapter);
    if h.paused {
        s.push_str("\nclick to play");
    }
    s
}

#[derive(SystemParam)]
struct HudState<'w> {
    sim: Res<'w, Sim>,
    pose: Res<'w, Pose>,
    builder: Res<'w, CmdBuilder>,
    cfg: Res<'w, Config>,
    spots: Res<'w, Spots>,
    display: Res<'w, DisplayInfo>,
    paused: Res<'w, Paused>,
    visible: Res<'w, HudVisible>,
}

fn hud(time: Res<Time<Real>>, mut fps: ResMut<FpsMeter>, st: HudState, mut text: Query<(&mut Text, &mut Visibility), With<HudText>>, mut cross: Query<&mut Visibility, (With<Crosshair>, Without<HudText>)>) {
    fps.tick(time.delta_secs_f64());
    let shown = if st.visible.0 { Visibility::Inherited } else { Visibility::Hidden };
    for (mut t, mut vis) in &mut text {
        *vis = shown;
        if st.visible.0 {
            let view = HudView { sim: &st.sim, pose: &st.pose, builder: &st.builder, cfg: &st.cfg, spots: &st.spots, fps: &fps, display: &st.display, paused: st.paused.0 };
            t.0 = hud_text(&view);
        }
    }
    for mut vis in &mut cross {
        *vis = shown;
    }
}
