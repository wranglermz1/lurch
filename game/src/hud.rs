use std::fmt::Write;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use lurch::sim::mantle::MOVETYPE_TRAVERSE;
use lurch::sim::math::is_world_up;

use crate::binds::WheelMode;
use crate::cmd::CmdBuilder;
use crate::sim::{Map, Pose, Sim, Spots};
use crate::view::DisplayInfo;
use crate::{common, Config, Frame, HudVisible, Paused};

/// One inch per second in km/h.
pub const KMH_PER_UPS: f32 = 0.09144;
const FPS_WINDOW: f64 = 0.5;

#[derive(Component)]
pub struct HudText;

#[derive(Component)]
pub struct Crosshair;

/// Pathfinder's grapple indicator when the game's UI is not loaded: a ring on the crosshair that
/// turns blue while the grapple is in range, drawn after the wiki's description. With the
/// install's UI, `rui.rs` draws the game's own `ui/crosshair_grapple` instead.
#[derive(Component)]
pub struct GrappleRing;

const RING: f32 = 26.0;
const RING_EASE: f32 = 0.1;

/// The client's in-range answer, a frame late as the game's queued ray is, and when it last
/// turned in and out of range (the RUI's `grapple_in_range_time`, `grapple_out_of_range_time`).
#[derive(Resource, Default)]
pub struct GrappleRange {
    queued: bool,
    pub shown: bool,
    since: f32,
    pub in_time: f32,
    pub out_time: f32,
}

/// Where this frame's `GrappleRange` is settled.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct GrappleRangeSet;

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
        app.init_resource::<FpsMeter>()
            .init_resource::<DisplayInfo>()
            .init_resource::<GrappleRange>()
            .add_systems(Startup, spawn_hud)
            .add_systems(Update, (hud, grapple_ring.in_set(GrappleRangeSet)).in_set(Frame::View));
    }
}

fn spawn_hud(mut commands: Commands) {
    commands.spawn((HudText, Text::new(""), TextFont::from_font_size(16.0), Node { position_type: PositionType::Absolute, top: px(8), left: px(8), ..default() }));
    commands.spawn((
        Crosshair,
        Node { position_type: PositionType::Absolute, left: percent(50), top: percent(50), width: px(4), height: px(4), margin: UiRect::all(px(-2)), ..default() },
        BackgroundColor(Color::WHITE),
    ));
    commands.spawn((
        GrappleRing,
        Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            top: percent(50),
            width: px(RING),
            height: px(RING),
            margin: UiRect::all(px(-RING * 0.5)),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BorderColor::all(Color::NONE),
    ));
}

fn grapple_ring(
    sim: Res<Sim>,
    mut map: ResMut<Map>,
    visible: Res<HudVisible>,
    rui: Option<Res<crate::rui::Runtime>>,
    mut range: ResMut<GrappleRange>,
    mut rings: Query<(&mut BorderColor, &mut Visibility), With<GrappleRing>>,
) {
    let now = sim.view_now;
    let p = &sim.pilot.player;
    let ready = p.may_grapple(&sim.settings);
    let was = range.shown;
    range.shown = ready && range.queued;
    range.queued = sim.pilot.grapple_in_range(&sim.settings, now, &mut map.0);
    if range.shown != was {
        range.since = now;
        if range.shown {
            range.in_time = now;
        } else {
            range.out_time = now;
        }
    }
    let k = ((now - range.since) / RING_EASE).clamp(0.0, 1.0);
    let k = if range.shown { k } else { 1.0 - k };
    let out = Color::srgba(1.0, 1.0, 1.0, if ready { 0.55 } else { 0.2 });
    let color = out.mix(&Color::srgb(0.25, 0.66, 1.0), k);
    for (mut border, mut vis) in &mut rings {
        *vis = if visible.0 && rui.is_none() { Visibility::Inherited } else { Visibility::Hidden };
        *border = BorderColor::all(color);
    }
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
    let place = if p.player.movetype == MOVETYPE_TRAVERSE {
        "mantle"
    } else if p.player.zipline.is_some() {
        "zipline"
    } else if !p.player.on_ground() {
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
    let _ = writeln!(s, "{place}   {}  {}  {}   grapple power {:.0}", on(p.player.sliding, "slide"), on(p.player.sprinting, "sprint"), on(p.player.ducked(), "duck"), p.player.suit_grapple_power);
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

fn hud(
    time: Res<Time<Real>>,
    mut fps: ResMut<FpsMeter>,
    st: HudState,
    rui: Option<Res<crate::rui::Runtime>>,
    mut text: Query<(&mut Text, &mut Visibility), With<HudText>>,
    mut cross: Query<&mut Visibility, (With<Crosshair>, Without<HudText>)>,
) {
    fps.tick(time.delta_secs_f64());
    let shown = if st.visible.0 { Visibility::Inherited } else { Visibility::Hidden };
    for (mut t, mut vis) in &mut text {
        vis.set_if_neq(shown);
        if st.visible.0 {
            let view = HudView { sim: &st.sim, pose: &st.pose, builder: &st.builder, cfg: &st.cfg, spots: &st.spots, fps: &fps, display: &st.display, paused: st.paused.0 };
            let s = hud_text(&view);
            if t.0 != s {
                t.0 = s;
            }
        }
    }
    for mut vis in &mut cross {
        vis.set_if_neq(if rui.is_some() { Visibility::Hidden } else { shown });
    }
}
