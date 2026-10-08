//! Scripted runs across the test map. No recording covers this geometry, so these hold the
//! simulation to the limits its settings set, not to the game.

use lurch::sim::duck::IN_DUCK;
use lurch::sim::ground::WALKABLE;
use lurch::sim::jump::{IN_FORWARD, IN_JUMP};
use lurch::sim::mantle::MOVETYPE_TRAVERSE;
use lurch::sim::math::{is_world_up, V3};
use lurch::sim::pilot::{self, Pilot};
use lurch::sim::setup::UserCmd;
use lurch::sim::sprint::IN_SPEED;
use lurch::testmap::{self, FEATURE_X, RAMP_DEGREES, RAMP_RISE, STEP_HEIGHTS};
use lurch::world::World;

const DT: f32 = 0.004;
const RUN: u32 = IN_FORWARD | IN_SPEED;

struct State {
    origin: V3,
    velocity: V3,
    /// On ground the right way up, not on a wall.
    standing: bool,
    mantling: bool,
}

/// Runs forward from a spot for `steps` commands, with buttons from `press`.
fn drive(spot: &str, steps: u32, mut press: impl FnMut(&Pilot) -> u32) -> Vec<State> {
    let map = testmap::build();
    let (origin, yaw) = (map.spot(spot).origin, map.spot(spot).yaw);
    let mut world = World::from_tris(map.tris);
    let s = pilot::settings();
    let mut p = Pilot::new(origin);
    let mut clock = 100.0f64;
    let mut out = Vec::new();
    for i in 0..steps {
        let cmd = UserCmd { command_number: i, angles: [0.0, yaw, 0.0], forward: 1.0, side: 0.0, up: 0.0, buttons: press(&p), frametime: DT, random_seed: i };
        p.step(&s, &cmd, clock as f32, &mut world);
        clock += DT as f64;
        let mantling = p.player.movetype == MOVETYPE_TRAVERSE;
        out.push(State { origin: p.origin, velocity: p.velocity, standing: !mantling && p.player.ground.is_some() && is_world_up(p.player.up), mantling });
    }
    out
}

fn stood_at(run: &[State], z: f32) -> bool {
    run.iter().any(|s| s.standing && (s.origin[2] - z).abs() < 0.1)
}

#[test]
fn steps_up_no_higher_than_the_step_height() {
    let step = pilot::spawn([0.0; 3]).step_height;
    for h in STEP_HEIGHTS {
        let run = drive(&format!("step {h}"), 500, |_| RUN);
        assert_eq!(stood_at(&run, h), h <= step, "step {h}");
    }
}

/// Slopes up to walkable are walked up. A steeper one is climbed, and the top reached only by
/// mantling onto it; 60 degrees is not climbed at all.
#[test]
fn walks_up_slopes_no_steeper_than_walkable() {
    for deg in RAMP_DEGREES {
        let run = drive(&format!("ramp {deg}"), 800, |_| RUN);
        let walkable = deg.to_radians().cos() >= WALKABLE;
        let mantled = run.iter().any(|s| s.mantling);
        assert_eq!(stood_at(&run, RAMP_RISE), walkable || deg < 60.0, "ramp {deg}");
        assert_eq!(mantled, !walkable && deg < 60.0, "ramp {deg}");
    }
}

/// Sprints at the wall and jumps 80 units short of it.
fn run_at_wall(h: f32) -> Vec<State> {
    let mut jumped = false;
    drive(&format!("wall {h}"), 1500, |p| {
        let jump = !jumped && p.player.ground.is_some() && p.origin[0] > FEATURE_X - 96.0;
        jumped |= jump;
        RUN | if jump { IN_JUMP } else { 0 }
    })
}

#[test]
fn a_wall_under_jump_height_is_jumped() {
    assert!(48.0 < pilot::settings().jump_height);
    let run = run_at_wall(48.0);
    assert!(run.last().unwrap().origin[0] > FEATURE_X + 64.0);
}

/// A wall whose top the climb brings within mantling reach is mantled, and the run carries on
/// over it.
#[test]
fn walls_in_reach_are_mantled() {
    for h in [96.0, 128.0, 160.0, 192.0, 256.0] {
        let run = run_at_wall(h);
        assert!(run.iter().any(|s| s.mantling), "wall {h}");
        assert!(stood_at(&run, h), "wall {h}");
        assert!(run.last().unwrap().origin[0] > FEATURE_X + 64.0, "wall {h}");
    }
}

#[test]
fn every_wall_too_tall_to_climb_tops_out_at_the_same_height() {
    let peaks: Vec<f32> = [384.0, 1024.0]
        .map(|h| {
            let run = run_at_wall(h);
            let last = run.last().unwrap();
            assert!(last.standing && last.origin[2] < 0.1 && last.origin[0] < FEATURE_X, "wall {h} ends at {:?}", last.origin);
            run.iter().map(|s| s.origin[2]).fold(0.0, f32::max)
        })
        .to_vec();
    assert!(peaks.iter().all(|&p| p == peaks[0]), "{peaks:?}");
    let s = pilot::settings();
    assert!(peaks[0] > s.jump_height + s.climb.height, "{peaks:?}");
}

#[test]
fn sliding_down_the_hill_outruns_sprinting() {
    let s = pilot::settings();
    let sprint = s.stances[0].sprint_speed * pilot::spawn([0.0; 3]).speed_scale;
    let speed = |st: &State| (st.velocity[0] * st.velocity[0] + st.velocity[1] * st.velocity[1]).sqrt();
    let edge = 256.0 - 64.0;
    let ran = drive("hill", 2000, |_| RUN);
    let slid = drive("hill", 2000, |p| RUN | if p.origin[0] > edge { IN_DUCK } else { 0 });
    let downhill = |run: &[State]| run.iter().filter(|s| s.origin[2] > 32.0 && s.origin[0] > 256.0).map(speed).fold(f32::INFINITY, f32::min);
    assert!((downhill(&ran) - sprint).abs() < 0.1, "{} vs {sprint}", downhill(&ran));
    assert!(downhill(&slid) > sprint * 0.85 && slid.iter().map(speed).fold(0.0, f32::max) > sprint * 1.3);
}

#[test]
fn a_spawned_pilot_sees_the_class_fov() {
    let s = pilot::settings();
    let mut p = Pilot::new([0.0; 3]);
    assert_eq!(p.player.view_fov(&s, 1.0, 100.0), s.fov);
    p.player.view_frame(&s, p.velocity, 100.0, DT);
    assert_eq!(p.player.view_fov(&s, 1.0, 100.0 + DT), s.fov);
}
