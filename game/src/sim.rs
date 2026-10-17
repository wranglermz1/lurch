use std::collections::VecDeque;

use bevy::prelude::*;
use lurch::sim::buttons::IN_OFFHAND1;
use lurch::sim::math::V3;
use lurch::sim::pilot::{self, Clock, Eye, Pilot};
use lurch::sim::setup::UserCmd;
use lurch::sim::Settings;
use lurch::testmap::{self, Spot};
pub use lurch::world::World as CollisionWorld;

use crate::cmd::CmdQueue;
use crate::cooldown::GrappleClip;
use crate::{common, config, Config, Frame, LookReset, Respawn};

pub const KILL_Z: f32 = -2048.0;
pub const HISTORY: usize = 1024;

/// The only state the simulation keeps; written by `apply_respawn` and `run_cmds` alone.
#[derive(Resource, Clone, Debug)]
pub struct Sim {
    pub pilot: Pilot,
    pub clock: Clock,
    pub settings: Settings,
    /// The clock the last view frame ran at: after its commands' advance.
    pub view_now: f32,
    pub grapple_clip: GrappleClip,
}

/// Mutable only because a trace takes `&mut self`.
#[derive(Resource)]
pub struct Map(pub CollisionWorld);

#[derive(Resource)]
pub struct Spots {
    pub list: Vec<Spot>,
    pub current: usize,
}

impl Spots {
    pub fn index(&self, name: &str) -> Option<usize> {
        self.list.iter().position(|s| s.name == name)
    }

    pub fn spot(&self) -> &Spot {
        &self.list[self.current]
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub cmd: UserCmd,
    pub pilot: Pilot,
    pub clock: Clock,
}

/// The last commands run and the movement state each left, oldest first.
#[derive(Resource, Clone, Debug, Default)]
pub struct History(pub VecDeque<Snapshot>);

impl History {
    pub fn push(&mut self, s: Snapshot) {
        if self.0.len() == HISTORY {
            self.0.pop_front();
        }
        self.0.push_back(s);
    }
}

/// One command's call to movement as the sound and animation layers see it: the cues it
/// raised and the state it left.
/// How far the hook must be from the eye before the weapons go down (`0x7d0dc0`), squared.
const GRAPPLE_WEAPONS_DOWN2: f32 = 16384.0;

#[derive(Clone, Debug)]
pub struct Step {
    pub cues: Vec<lurch::sim::cue::Cue>,
    pub sound: lurch::sound::Frame,
    pub sprinting: bool,
    /// The mantle while the player is in one.
    pub traversal: Option<lurch::anim::proxy::Traversal>,
    /// The wall climb while its arms are on.
    pub climb: Option<lurch::anim::proxy::Climb>,
    /// The grapple would put the weapons down (`0x7d0dc0`, with `grapple_disableWeaponsWhenActive`):
    /// the hook is out, not reeling in, and more than 128 from the eye.
    pub grapple_down: bool,
    /// The player's activity modifiers after the command.
    pub modifiers: lurch::anim::viewmodel::Modifiers,
    pub frametime: f32,
}

/// This frame's commands, in order.
#[derive(Resource, Clone, Debug, Default)]
pub struct FrameCues(pub Vec<Step>);

fn sound_frame(pilot: &Pilot, curtime: f32) -> lurch::sound::Frame {
    let p = &pilot.player;
    let (lo, hi) = if p.ducked() { (p.hull_duck_min, p.hull_duck_max) } else { (p.hull_min, p.hull_max) };
    lurch::sound::Frame {
        origin: pilot.origin,
        hull_height: hi[2] - lo[2],
        velocity: pilot.velocity,
        up: p.up,
        on_ground: p.ground.is_some(),
        sliding: p.sliding,
        dead: p.dead,
        ladder: p.movetype == 9,
        curtime,
        left_ground_time: p.left_ground_time,
    }
}

/// The camera this frame, from `Pilot::eye`.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Pose(pub Eye);

/// A pilot with the view setup's own fields reset, which movement never reads: what two runs
/// with different view-frame timing still agree on.
pub fn without_view(p: &Pilot) -> Pilot {
    let mut p = p.clone();
    let spawned = pilot::spawn([0.0; 3]);
    p.player.slide_tilt = spawned.slide_tilt;
    p.player.fov_start = spawned.fov_start;
    p.player.fov_duration = spawned.fov_duration;
    p.player.fov_from = spawned.fov_from;
    p.player.fov_to = spawned.fov_to;
    p.player.zipline_roll = spawned.zipline_roll;
    p.player.zipline_roll_vel = spawned.zipline_roll_vel;
    p
}

fn spawn_pilot(origin: V3, yaw: f32, oldbuttons: u32) -> Pilot {
    let mut p = Pilot::new(origin).with_oldbuttons(oldbuttons);
    p.player.view_angles = [0.0, yaw, 0.0];
    p
}

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        common(app);
        let cfg = config(app);
        let mut map = testmap::build();
        if let Some(install) = &cfg.r5r {
            match crate::data::load_props(install, &map.props) {
                Ok(props) => {
                    map.tris.extend(props.tris());
                    app.insert_resource(props);
                }
                Err(e) => warn!("no props: {e}"),
            }
        }
        let current = map.spots.iter().position(|s| s.name == cfg.spot).unwrap_or_else(|| panic!("no spot {:?}", cfg.spot));
        let (origin, yaw) = (map.spots[current].origin, map.spots[current].yaw);
        let settings = pilot::pathfinder();
        let pilot = spawn_pilot(origin, yaw, 0);
        let clock = Clock::default();
        let pose = Pose(pilot.eye(&settings, cfg.fov_scale, clock.now()));
        app.insert_resource(Sim { pilot, clock, settings, view_now: clock.now(), grapple_clip: GrappleClip::default() })
            .insert_resource(Map(map.world()))
            .insert_resource(Spots { list: map.spots, current })
            .init_resource::<History>()
            .init_resource::<FrameCues>()
            .insert_resource(pose)
            .add_systems(Update, apply_respawn.in_set(Frame::Respawn))
            .add_systems(Update, run_cmds.in_set(Frame::Run))
            .add_systems(Update, pose_system.in_set(Frame::Pose));
        app.world_mut().write_message(LookReset { yaw });
    }
}

fn apply_respawn(mut respawns: MessageReader<Respawn>, mut sim: ResMut<Sim>, mut spots: ResMut<Spots>, mut history: ResMut<History>, mut look: MessageWriter<LookReset>) {
    let Some(r) = respawns.read().last().cloned() else { return };
    let n = spots.list.len() as isize;
    let (origin, yaw) = match r {
        Respawn::Spot(k) => {
            spots.current = (spots.current as isize + k).rem_euclid(n) as usize;
            (spots.spot().origin, spots.spot().yaw)
        }
        Respawn::Named(name) => match spots.index(&name) {
            Some(i) => {
                spots.current = i;
                (spots.spot().origin, spots.spot().yaw)
            }
            None => {
                warn!("no spot {name:?}");
                return;
            }
        },
        Respawn::At { origin, yaw } => (origin, yaw),
    };
    let old = sim.pilot.oldbuttons();
    sim.pilot = spawn_pilot(origin, yaw, old);
    sim.grapple_clip = GrappleClip::default();
    sim.clock = Clock::default();
    sim.view_now = sim.clock.now();
    history.0.clear();
    look.write(LookReset { yaw });
}

#[allow(clippy::too_many_arguments)] // Bevy's system parameters
fn run_cmds(
    mut sim: ResMut<Sim>,
    mut map: ResMut<Map>,
    mut queue: ResMut<CmdQueue>,
    mut history: ResMut<History>,
    mut cues: ResMut<FrameCues>,
    mut respawn: MessageWriter<Respawn>,
    over: Option<ResMut<crate::viewmodel::GrappleAnimOver>>,
) {
    let Sim { pilot, clock, settings, view_now, grapple_clip } = &mut *sim;
    let mut last = None;
    cues.0.clear();
    if let Some(mut over) = over.filter(|o| o.0) {
        over.0 = false;
        pilot.player.grapple_anim_end(&mut Vec::new());
    }
    while let Some(cmd) = queue.0.pop_front() {
        let now = clock.now();
        let rising = pilot.velocity[2] > f32::EPSILON;
        let old = pilot.oldbuttons();
        pilot.step(settings, &cmd, now, &mut map.0);
        if cmd.buttons & !old & IN_OFFHAND1 != 0 && (pilot.player.grappling || grapple_clip.ready()) {
            pilot.grapple(settings, now);
        }
        grapple_clip.think(now, cmd.frametime, pilot);
        let p = &pilot.player;
        let traversal = (p.movetype == lurch::sim::mantle::MOVETYPE_TRAVERSE).then_some(lurch::anim::proxy::Traversal {
            ty: p.traversal_type,
            cycle: p.traversal_cycle,
            gun_up: p.traversal_gun != 0,
            start_time: p.traversal_start_time,
            ledge: p.traversal_ref,
            forward: p.traversal_forward,
            mid: p.traversal_mid,
            end: p.traversal_end,
        });
        let climb = p.climb_arms.then_some(lurch::anim::proxy::Climb { normal: p.up, rising });
        let eye = p.eye_position(settings, pilot.origin, p.view_angles, now);
        let h = p.grapple_hook_at;
        let (dx, dy, dz) = (h[0] - eye[0], h[1] - eye[1], h[2] - eye[2]);
        let grapple_down = p.grappling && !p.grapple_retracting && GRAPPLE_WEAPONS_DOWN2 < dy * dy + dx * dx + dz * dz;
        cues.0.push(Step { cues: pilot.cues.clone(), sound: sound_frame(pilot, now), sprinting: p.sprinting, traversal, climb, grapple_down, modifiers: lurch::anim::viewmodel::player_modifiers(p), frametime: cmd.frametime });
        clock.advance(cmd.frametime);
        history.push(Snapshot { cmd, pilot: pilot.clone(), clock: *clock });
        if pilot.origin[2] < KILL_Z {
            respawn.write(Respawn::Spot(0));
        }
        last = Some(cmd);
    }
    if let Some(cmd) = last {
        *view_now = clock.now();
        pilot.view_frame(settings, *view_now, cmd.frametime);
    }
}

fn pose_system(sim: Res<Sim>, cfg: Res<Config>, mut pose: ResMut<Pose>) {
    pose.0 = sim.pilot.eye(&sim.settings, cfg.fov_scale, sim.view_now);
}
