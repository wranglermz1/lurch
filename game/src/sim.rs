use std::collections::VecDeque;

use bevy::prelude::*;
use lurch::anim::viewmodel::MeleeSwing;
use lurch::sim::buttons::{IN_ATTACK, IN_DUCK, IN_JUMP, IN_OFFHAND1};
use lurch::sim::math::V3;
use lurch::sim::{HullQuery, Trace, Tracer};
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

/// The punch the viewmodel plays, once it has started it: the commands it spans end the attack
/// and count its blow. Present while a viewmodel can time a punch; without one, nothing punches.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct Swing(pub Option<MeleeSwing>);

/// How many punches have landed their blow (`AE_MELEE_ATTACK`), and what each blow this frame
/// hit, in order (none for a blow that hit nothing).
#[derive(Resource, Clone, Debug, Default)]
pub struct MeleeAttacks(pub u32, pub Vec<Option<lurch::sim::melee::Blow>>);

/// Crouching on the ground stops at ledges as Minecraft's sneaking does, for the Minecraft host:
/// Apex has nothing like it, so the shell keeps it out of movement and fences the step instead.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SneakEdges;

/// How far below the hull Minecraft's sneaking still counts as ground: its step height, 0.6
/// blocks.
const SNEAK_DROP: f32 = 24.0;
/// How finely the move is cut back toward a ledge: Minecraft's 0.05 blocks.
const SNEAK_BACK_OFF: f32 = 2.0;

/// A limit on a moving hull's origin along one axis: no further than `at` in the direction
/// `sign` (1 or -1) on axis `axis`.
#[derive(Clone, Copy)]
struct Fence {
    axis: usize,
    sign: f32,
    at: f32,
}

/// The world with fences that only a moving hull hits: what holds a sneaking pilot in for one
/// command.
struct Fenced<'a> {
    world: &'a mut CollisionWorld,
    fences: &'a [Fence],
}

impl Tracer for Fenced<'_> {
    fn hull(&mut self, q: &HullQuery) -> Trace {
        let mut t = self.world.hull(q);
        if q.mins == q.maxs || t.startsolid {
            return t;
        }
        let d = [q.end[0] - q.start[0], q.end[1] - q.start[1], q.end[2] - q.start[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        for f in self.fences {
            let toward = d[f.axis] * f.sign;
            if toward <= 0.0 {
                continue;
            }
            let room = (f.at - q.start[f.axis]) * f.sign;
            let frac = if room <= 0.0 { 0.0 } else { ((room / toward) - DIST_EPSILON / len).max(0.0) };
            if frac < t.fraction {
                let mut n = [0.0; 3];
                n[f.axis] = -f.sign;
                (t.fraction, t.normal, t.entity, t.contents) = (frac, n, 1, 1);
            }
        }
        t.endpos = std::array::from_fn(|i| q.start[i] + d[i] * t.fraction);
        t
    }

    fn ziplines(&self) -> &[lurch::sim::zipline::Zipline] {
        self.world.ziplines()
    }

    fn bodies(&self) -> &[lurch::world::Body] {
        self.world.bodies()
    }
}

/// Whether the hull at `at` has something under it within `SNEAK_DROP`.
fn supported(world: &mut CollisionWorld, at: V3, mins: V3, maxs: V3, mask: u32) -> bool {
    world.hull(&HullQuery { start: at, end: [at[0], at[1], at[2] - SNEAK_DROP], mins, maxs, mask }).fraction < 1.0
}

/// Minecraft's `maybeBackOffFromEdge` on a move `d` from `o`: each axis, then both, cut back
/// toward zero until the hull there stays over ground.
fn back_off(world: &mut CollisionWorld, o: V3, d: [f32; 2], mins: V3, maxs: V3, mask: u32) -> [f32; 2] {
    let toward_zero = |v: f32| if v.abs() <= SNEAK_BACK_OFF { 0.0 } else { v - SNEAK_BACK_OFF * v.signum() };
    let mut on = |x: f32, y: f32| supported(world, [o[0] + x, o[1] + y, o[2]], mins, maxs, mask);
    if on(d[0], d[1]) {
        return d;
    }
    let [mut x, mut y] = d;
    while x != 0.0 && !on(x, 0.0) {
        x = toward_zero(x);
    }
    while y != 0.0 && !on(0.0, y) {
        y = toward_zero(y);
    }
    while x != 0.0 && y != 0.0 && !on(x, y) {
        (x, y) = (toward_zero(x), toward_zero(y));
    }
    [x, y]
}

/// One command for a pilot crouching on the ground with `SneakEdges`: the command runs, and a
/// move that would leave the hull over nothing runs again with a fence where Minecraft's
/// sneaking would stop it on each axis, so the move slides along it and stays on the ground.
/// The pilot never ends a command further over an edge than the fences allow: when the
/// fences have not held it after a few tries, it is put back where they would have, at rest
/// along each axis cut back.
fn sneak_step(pilot: &mut Pilot, s: &Settings, cmd: &UserCmd, now: f32, world: &mut CollisionWorld) {
    const TRIES: usize = 4;
    let o = pilot.origin;
    let p = &pilot.player;
    let (mins, maxs) = if p.ducked() { (p.hull_duck_min, p.hull_duck_max) } else { (p.hull_min, p.hull_max) };
    let mask = p.mask;
    let start = pilot.clone();
    let mut fences = Vec::new();
    for _ in 0..TRIES {
        let mut trial = start.clone();
        trial.step(s, cmd, now, &mut Fenced { world, fences: &fences });
        let d = [trial.origin[0] - o[0], trial.origin[1] - o[1]];
        let kept = back_off(world, o, d, mins, maxs, mask);
        if kept == d {
            *pilot = trial;
            return;
        }
        for i in 0..2 {
            if kept[i] != d[i] && !fences.iter().any(|f: &Fence| f.axis == i && f.sign == d[i].signum()) {
                fences.push(Fence { axis: i, sign: d[i].signum(), at: o[i] + kept[i] });
            }
        }
        *pilot = trial;
    }
    let d = [pilot.origin[0] - o[0], pilot.origin[1] - o[1]];
    let kept = back_off(world, o, d, mins, maxs, mask);
    for i in 0..2 {
        if kept[i] != d[i] {
            pilot.origin[i] = o[i] + kept[i];
            pilot.velocity[i] = 0.0;
        }
    }
    if supported(world, pilot.origin, mins, maxs, mask) && pilot.player.ground.is_none() {
        pilot.origin[2] = o[2];
        pilot.velocity[2] = 0.0;
        pilot.player.ground = Some(1);
        pilot.player.flags |= 1;
    }
}

/// Whether the command is one `SneakEdges` fences: walking on the ground no faster than the
/// standing walk, crouched or crouching, not sliding, jumping or on a zipline.
fn sneaking(pilot: &Pilot, s: &Settings, cmd: &UserCmd) -> bool {
    let p = &pilot.player;
    let v = pilot.velocity;
    let walk = s.stances[0].speed * p.speed_scale;
    p.ground.is_some()
        && p.movetype == 2
        && !p.sliding
        && p.zipline.is_none()
        && v[0] * v[0] + v[1] * v[1] <= walk * walk
        && (p.ducked() || p.duck_held || cmd.buttons & IN_DUCK != 0)
        && cmd.buttons & IN_JUMP == 0
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
    /// A punch started after the command.
    pub melee: bool,
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

/// Source's DIST_EPSILON: a trace leaves a hull resting this far above the surface it stops on.
/// A pilot spawned exactly touching the floor starts every trace solid, and after a first
/// command with no input it hangs in the air for good, so it spawns where a trace would leave it.
pub const DIST_EPSILON: f32 = 0.03125;

/// A pilot spawned with the hulls `settings` give its stances.
fn spawn_pilot(settings: &Settings, origin: V3, yaw: f32, oldbuttons: u32) -> Pilot {
    let mut p = Pilot::new([origin[0], origin[1], origin[2] + DIST_EPSILON]).with_oldbuttons(oldbuttons);
    p.player.view_angles = [0.0, yaw, 0.0];
    let (stand, duck) = (&settings.stances[0], &settings.stances[1]);
    (p.player.hull_min, p.player.hull_max, p.player.hull_duck_min, p.player.hull_duck_max) = (stand.hull_min, stand.hull_max, duck.hull_min, duck.hull_max);
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
        let pilot = spawn_pilot(&settings, origin, yaw, 0);
        let clock = Clock::default();
        let pose = Pose(pilot.eye(&settings, cfg.fov_scale, clock.now()));
        app.insert_resource(Sim { pilot, clock, settings, view_now: clock.now(), grapple_clip: GrappleClip::default() })
            .insert_resource(Map(map.world()))
            .insert_resource(Spots { list: map.spots, current })
            .init_resource::<History>()
            .init_resource::<FrameCues>()
            .init_resource::<MeleeAttacks>()
            .insert_resource(pose)
            .add_systems(Update, apply_respawn.in_set(Frame::Respawn))
            .add_systems(Update, run_cmds.in_set(Frame::Run))
            .add_systems(Update, pose_system.in_set(Frame::Pose));
        app.world_mut().write_message(LookReset { yaw });
    }
}

fn apply_respawn(mut respawns: MessageReader<Respawn>, mut sim: ResMut<Sim>, mut spots: ResMut<Spots>, mut history: ResMut<History>, mut look: MessageWriter<LookReset>, swing: Option<ResMut<Swing>>) {
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
    sim.pilot = spawn_pilot(&sim.settings, origin, yaw, old);
    sim.grapple_clip = GrappleClip::default();
    if let Some(mut swing) = swing {
        swing.0 = None;
    }
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
    (mut swing, mut attacks, hands): (Option<ResMut<Swing>>, ResMut<MeleeAttacks>, Option<Res<crate::viewmodel::HostHands>>),
    sneak: Option<Res<SneakEdges>>,
) {
    let Sim { pilot, clock, settings, view_now, grapple_clip } = &mut *sim;
    let mut last = None;
    cues.0.clear();
    attacks.1.clear();
    if let Some(mut over) = over.filter(|o| o.0) {
        over.0 = false;
        pilot.player.grapple_anim_end(&mut Vec::new());
    }
    while let Some(cmd) = queue.0.pop_front() {
        let now = clock.now();
        if let Some(Swing(punch)) = swing.as_deref_mut() {
            if let Some(s) = punch.as_mut().filter(|s| s.attack.is_some_and(|t| t <= now)) {
                s.attack = None;
                attacks.0 += 1;
                let blow = pilot.melee_blow(settings, now, lurch::sim::melee::MELEE_RANGE, &mut map.0);
                attacks.1.push(blow);
            }
            if punch.is_some_and(|s| s.end <= now) {
                pilot.player.end_melee();
                *punch = None;
            }
        }
        let rising = pilot.velocity[2] > f32::EPSILON;
        let old = pilot.oldbuttons();
        if sneak.is_some() && sneaking(pilot, settings, &cmd) {
            sneak_step(pilot, settings, &cmd, now, &mut map.0);
        } else {
            pilot.step(settings, &cmd, now, &mut map.0);
        }
        if cmd.buttons & !old & IN_OFFHAND1 != 0 && (pilot.player.grappling || grapple_clip.ready()) {
            pilot.grapple(settings, now);
        }
        let fists = swing.is_some() && hands.as_ref().is_none_or(|h| h.0);
        let melee = fists && cmd.buttons & !old & IN_ATTACK != 0 && pilot.melee(now);
        if melee {
            pilot.melee_start_activity(settings, now, &mut map.0);
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
        cues.0.push(Step { cues: pilot.cues.clone(), sound: sound_frame(pilot, now), sprinting: p.sprinting, traversal, climb, grapple_down, modifiers: lurch::anim::viewmodel::player_modifiers(p), melee, frametime: cmd.frametime });
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
