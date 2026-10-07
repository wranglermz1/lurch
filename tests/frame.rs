//! What a game shell's frame relies on: the camera a pilot shows, cloning and carrying a
//! pilot, the client's key buttons, command seeds and the clock. No recordings.

use lurch::sim::buttons::*;
use lurch::sim::input::{key_state, Keys, DOWN, IMPULSE_DOWN, IMPULSE_UP};
use lurch::sim::pilot::{self, Clock, Pilot};
use lurch::sim::setup::{command_seed, UserCmd};
use lurch::testmap;
use lurch::world::World;

const DT: f32 = 0.004;

fn cmd(n: u32, yaw: f32, buttons: u32) -> UserCmd {
    let forward = if buttons & IN_FORWARD != 0 { 1.0 } else { 0.0 };
    UserCmd { command_number: n, angles: [0.0, yaw, 0.0], forward, buttons, frametime: DT, random_seed: command_seed(n), ..Default::default() }
}

fn spot(name: &str) -> ([f32; 3], World) {
    let map = testmap::build();
    let origin = map.spot(name).origin;
    (origin, World::from_tris(map.tris))
}

fn filled(keys: &mut Keys) -> UserCmd {
    let mut c = UserCmd::default();
    keys.fill(&mut c);
    c
}

#[test]
fn spawned_pilot_sees_the_class_fov() {
    let s = pilot::settings();
    let (origin, mut world) = spot("wall 48");
    let mut clock = Clock::default();
    let mut p = Pilot::new(origin);
    assert_eq!(p.eye(&s, 1.0, clock.now()).fov, 70.0);
    let c = cmd(1, 0.0, 0);
    p.step(&s, &c, clock.now(), &mut world);
    clock.advance(c.frametime);
    p.player.view_frame(&s, p.velocity, clock.now(), c.frametime);
    assert_eq!(p.eye(&s, 1.0, clock.now()).fov, 70.0);
}

#[test]
fn a_cloned_pilot_replays_bit_for_bit() {
    let s = pilot::settings();
    let (origin, mut world) = spot("hill");
    let script = |n: u32| {
        let jump = if n.is_multiple_of(90) { IN_JUMP } else { 0 };
        let duck = if (150..190).contains(&n) { IN_DUCK } else { 0 };
        cmd(n, (n as f32 * 0.3).sin() * 40.0, IN_FORWARD | IN_SPEED | jump | duck)
    };
    let mut a = Pilot::new(origin);
    let mut clock = Clock::default();
    for n in 1..=100 {
        a.step(&s, &script(n), clock.now(), &mut world);
        clock.advance(DT);
    }
    let (mut b, mut clock_b) = (a.clone(), clock);
    for n in 101..=300 {
        a.step(&s, &script(n), clock.now(), &mut world);
        clock.advance(DT);
        b.step(&s, &script(n), clock_b.now(), &mut world);
        clock_b.advance(DT);
    }
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
    assert_eq!(clock, clock_b);
}

#[test]
fn held_jump_through_with_oldbuttons_does_not_jump() {
    let s = pilot::settings();
    let (origin, mut world) = spot("wall 48");
    let now = Clock::default().now();
    let mut held = Pilot::new(origin).with_oldbuttons(IN_JUMP);
    held.step(&s, &cmd(1, 0.0, IN_JUMP), now, &mut world);
    assert_eq!(held.velocity[2], 0.0);
    let mut fresh = Pilot::new(origin);
    fresh.step(&s, &cmd(1, 0.0, IN_JUMP), now, &mut world);
    assert!(fresh.velocity[2] > 0.0, "{:?}", fresh.velocity);
}

#[test]
fn a_tap_sets_its_bit_for_one_command() {
    let mut k = Keys::default();
    k.press(IN_FORWARD);
    k.release(IN_FORWARD);
    let c = filled(&mut k);
    assert_ne!(c.buttons & IN_FORWARD, 0);
    assert_eq!(c.forward, key_state(IMPULSE_DOWN | IMPULSE_UP));
    let c = filled(&mut k);
    assert_eq!(c.buttons & IN_FORWARD, 0);
    assert_eq!(c.forward, 0.0);
}

#[test]
fn a_held_key_reads_press_then_held() {
    let mut k = Keys::default();
    k.press(IN_FORWARD);
    assert_eq!(filled(&mut k).forward, key_state(DOWN | IMPULSE_DOWN));
    for _ in 0..3 {
        let c = filled(&mut k);
        assert_eq!(c.forward, key_state(DOWN));
        assert_ne!(c.buttons & IN_FORWARD, 0);
    }
    k.release(IN_FORWARD);
    let c = filled(&mut k);
    assert_eq!(c.forward, key_state(IMPULSE_UP));
    assert_eq!(c.buttons & IN_FORWARD, 0);
    assert_eq!(filled(&mut k).forward, 0.0);
    k.press(IN_FORWARD | IN_BACK);
    filled(&mut k);
    assert_eq!(filled(&mut k).forward, 0.0);
}

#[test]
fn release_and_repress_in_one_command() {
    let mut k = Keys::default();
    k.press(IN_MOVERIGHT);
    filled(&mut k);
    k.release(IN_MOVERIGHT);
    k.press(IN_MOVERIGHT);
    let c = filled(&mut k);
    assert_eq!(c.side, key_state(DOWN | IMPULSE_DOWN | IMPULSE_UP));
    assert_ne!(c.buttons & IN_MOVERIGHT, 0);
}

#[test]
fn taps_in_consecutive_commands_edge_once() {
    let mut k = Keys::default();
    let mut tap = || {
        k.press(IN_JUMP);
        k.release(IN_JUMP);
        filled(&mut k).buttons
    };
    let before = 0;
    let (first, second) = (tap(), tap());
    assert_ne!(first & IN_JUMP, 0);
    assert_ne!(second & IN_JUMP, 0);
    assert_ne!(first & !before & IN_JUMP, 0);
    assert_eq!(second & !first & IN_JUMP, 0);
}

#[test]
fn sprint_press_is_flagged_on_its_first_command_only() {
    let mut k = Keys::default();
    assert_eq!(filled(&mut k).buttons & IN_SPEED_PRESSED, 0);
    k.press(IN_SPEED);
    let first = filled(&mut k).buttons;
    assert_ne!(first & IN_SPEED, 0);
    assert_ne!(first & IN_SPEED_PRESSED, 0);
    for _ in 0..3 {
        let b = filled(&mut k).buttons;
        assert_ne!(b & IN_SPEED, 0);
        assert_eq!(b & IN_SPEED_PRESSED, 0);
    }
    k.release(IN_SPEED);
    filled(&mut k);
    k.press(IN_SPEED);
    assert_ne!(filled(&mut k).buttons & IN_SPEED_PRESSED, 0);
}

#[test]
fn release_all_releases_every_held_bit() {
    let mut k = Keys::default();
    k.press(IN_FORWARD | IN_MOVELEFT | IN_JUMP | IN_DUCK | IN_SPEED);
    filled(&mut k);
    k.release_all();
    for bit in [IN_FORWARD, IN_MOVELEFT, IN_JUMP, IN_DUCK, IN_SPEED] {
        assert_eq!(k.state(bit), IMPULSE_UP);
    }
    let c = filled(&mut k);
    assert_eq!(c.buttons, 0);
    assert_eq!(filled(&mut k), UserCmd::default());
}

#[test]
fn command_seeds_are_distinct_with_the_top_bit_clear() {
    let mut seeds: Vec<u32> = (0..10_000).map(command_seed).collect();
    assert!(seeds.iter().all(|s| s >> 31 == 0));
    seeds.sort_unstable();
    seeds.dedup();
    assert_eq!(seeds.len(), 10_000);
}

#[test]
fn clock_starts_at_100_and_round_trips() {
    let mut clock = Clock::default();
    assert_eq!(clock.now(), 100.0);
    assert_eq!(Clock::START, 100.0);
    let mut drive = 100.0f64;
    for i in 0..5000 {
        let dt = if i % 3 == 0 { DT } else { 0.0041666665 };
        clock.advance(dt);
        drive += dt as f64;
        assert_eq!(clock.now(), drive as f32);
    }
    assert_eq!(Clock::from_secs(clock.secs()), clock);
}
