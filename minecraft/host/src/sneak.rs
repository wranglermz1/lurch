//! Crouching on the ground stops at ledges as Minecraft's sneaking does. Apex has nothing like
//! it, so the host keeps it out of movement and fences the step instead (`sim::HostStep`).

use lurch::sim::buttons::{IN_DUCK, IN_JUMP};
use lurch::sim::math::V3;
use lurch::sim::pilot::Pilot;
use lurch::sim::setup::UserCmd;
use lurch::sim::{HullQuery, Settings, Trace, Tracer};
use lurch_game::sim::{CollisionWorld, DIST_EPSILON};

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

/// One command for a pilot crouching on the ground: the command runs, and a move that would leave the hull over nothing runs again with a fence where Minecraft's
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

/// Whether the command is one the fences hold: walking on the ground no faster than the
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

/// The host's `sim::HostStep`: a sneaking command runs fenced, any other is movement's.
pub fn step(pilot: &mut Pilot, s: &Settings, cmd: &UserCmd, now: f32, world: &mut CollisionWorld) -> bool {
    let sneak = sneaking(pilot, s, cmd);
    if sneak {
        sneak_step(pilot, s, cmd, now, world);
    }
    sneak
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
    use bevy::input::{ButtonState, InputPlugin};
    use bevy::prelude::*;
    use bevy::time::TimeUpdateStrategy;
    use lurch_game::cmd::CmdLog;
    use lurch_game::sim::{HostStep, Map, Sim};
    use lurch_game::{CmdPlugin, Config, Respawn, SimPlugin};

    use super::*;

    fn sim_app(cfg: Config) -> App {
        let mut app = App::new();
        app.insert_resource(cfg).add_plugins((MinimalPlugins, InputPlugin, CmdPlugin, SimPlugin)).init_resource::<CmdLog>();
        app
    }

    fn down(app: &mut App, key_code: KeyCode) {
        app.world_mut().write_message(KeyboardInput { key_code, logical_key: Key::Unidentified(NativeKey::Unidentified), state: ButtonState::Pressed, text: None, repeat: false, window: Entity::PLACEHOLDER });
    }

    fn sim(app: &App) -> &Sim {
        app.world().resource::<Sim>()
    }

    /// A pilot crouch-walking along +x for twelve seconds of Minecraft's frames from x = -120 (-300 to slide), on a
    /// floor that ends at x = 0 over nothing, with or without the ledge stop, sprinting into a slide
    /// first or not; where it ends up, and the lowest it went.
    fn crouch_walk_to_a_ledge(sneak: bool, slide: bool) -> (lurch::sim::pilot::Pilot, f32) {
        let mut app = sim_app(Config { autosprint: slide, ..Default::default() });
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 75.0)));
        if sneak {
            app.insert_resource(HostStep(step));
        }
        let tile = |x: f32, y: f32| [[[x, y, 0.0], [x + 40.0, y, 0.0], [x + 40.0, y + 40.0, 0.0]], [[x, y, 0.0], [x + 40.0, y + 40.0, 0.0], [x, y + 40.0, 0.0]]];
        let tris = (-20..0).flat_map(|i| (-20..20).flat_map(move |j| tile(i as f32 * 40.0, j as f32 * 40.0))).collect();
        app.world_mut().resource_mut::<Map>().0 = CollisionWorld::from_tris(tris);
        app.world_mut().write_message(Respawn::At { origin: [if slide { -300.0 } else { -120.0 }, 0.0, 0.0], yaw: 0.0 });
        for _ in 0..3 {
            app.update();
        }
        down(&mut app, KeyCode::KeyW);
        for _ in 0..if slide { 60 } else { 0 } {
            app.update();
        }
        down(&mut app, KeyCode::ControlLeft);
        let mut lowest = f32::MAX;
        for _ in 0..900 {
            app.update();
            lowest = lowest.min(sim(&app).pilot.origin[2]);
        }
        (sim(&app).pilot.clone(), lowest)
    }

    #[test]
    fn crouching_stops_at_a_ledge_as_minecraft_sneaking_does() {
        let (p, lowest) = crouch_walk_to_a_ledge(true, false);
        assert!(p.player.ground.is_some() && -1.0 < lowest, "{:?} down to {lowest}", p.origin);
        let half = p.player.hull_duck_max[0];
        assert!(p.origin[0] > half - 2.0 * 2.0 - 1.0 && p.origin[0] < half, "the hull hangs over the edge until it barely overlaps it: {:?}", p.origin);
        let (_, lowest) = crouch_walk_to_a_ledge(false, false);
        assert!(lowest < -40.0, "without it the pilot walks off");
        let (_, lowest) = crouch_walk_to_a_ledge(true, true);
        assert!(lowest < -40.0, "a slide goes over");
    }
}
