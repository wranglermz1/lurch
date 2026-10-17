use std::time::Duration;

use bevy::input::InputPlugin;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use lurch::sim::buttons::*;
use lurch::sim::pilot::{self, Clock, Pilot};
use lurch::sim::setup::{command_seed, UserCmd};
use lurch::testmap;
use lurch::world::World as CollisionWorld;
use lurch_game::cmd::{CmdBuilder, CmdLog, CmdQueue, CmdPlugin};
use lurch_game::sim::{without_view, History, Map, Pose, Sim, SimPlugin, Spots, DIST_EPSILON, HISTORY};
use lurch_game::{Config, Paused, Respawn};

use super::input::{down, motion, up};

pub const FT: f32 = 0.004;

pub fn sim_app(cfg: Config) -> App {
    let mut app = App::new();
    app.insert_resource(cfg)
        .add_plugins((MinimalPlugins, InputPlugin, CmdPlugin, SimPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(FT as f64)))
        .init_resource::<CmdLog>();
    app
}

fn hill() -> Config {
    Config { spot: "hill".into(), ..Default::default() }
}

/// Sprint across the hill top with a hop, slide down the slope with the view swinging, then
/// a slide jump.
fn drive(app: &mut App, frame: u64) {
    match frame {
        1 => {
            down(app, KeyCode::KeyW);
            down(app, KeyCode::ShiftLeft);
        }
        100 | 450 => down(app, KeyCode::Space),
        104 | 454 => up(app, KeyCode::Space),
        280 => down(app, KeyCode::ControlLeft),
        470 => up(app, KeyCode::ControlLeft),
        _ => {}
    }
    if (300..560).contains(&frame) {
        motion(app, ((frame as f32) * 0.05).sin() * 30.0, ((frame as f32) * 0.03).cos() * 4.0);
    }
}

/// A bare pilot loop over the shell's logged commands: each frame's commands stepped, then
/// one view frame.
pub struct Reference {
    pub pilot: Pilot,
    pub clock: Clock,
    world: CollisionWorld,
}

impl Reference {
    pub fn at(spot: &str) -> Self {
        let map = testmap::build();
        let o = map.spot(spot).origin;
        Reference { pilot: Pilot::new([o[0], o[1], o[2] + DIST_EPSILON]), clock: Clock::default(), world: map.world() }
    }

    pub fn frame(&mut self, cmds: &[UserCmd], view_each: bool) {
        let s = pilot::settings();
        for (i, c) in cmds.iter().enumerate() {
            self.pilot.step(&s, c, self.clock.now(), &mut self.world);
            self.clock.advance(c.frametime);
            if view_each || i + 1 == cmds.len() {
                self.pilot.view_frame(&s, self.clock.now(), c.frametime);
            }
        }
    }
}

fn commands_of(app: &App, frame: u64) -> Vec<UserCmd> {
    app.world().resource::<CmdLog>().0.iter().filter(|(f, _)| *f == frame).map(|(_, c)| *c).collect()
}

fn sim(app: &App) -> &Sim {
    app.world().resource::<Sim>()
}

#[test]
fn shell_equals_pilot() {
    let mut app = sim_app(hill());
    let mut r = Reference::at("hill");
    let (mut compared, mut slid, mut flew) = (0, false, false);
    for frame in 0..600 {
        drive(&mut app, frame);
        app.update();
        let cmds = commands_of(&app, frame);
        if cmds.is_empty() {
            continue;
        }
        r.frame(&cmds, false);
        assert_eq!(format!("{:?}", r.pilot), format!("{:?}", sim(&app).pilot), "frame {frame}");
        assert_eq!(r.clock, sim(&app).clock);
        compared += 1;
        slid |= r.pilot.player.sliding;
        flew |= !r.pilot.player.on_ground();
    }
    assert_eq!(compared, 599);
    assert!(slid && flew, "the drive slid {slid}, left the ground {flew}");
    assert!(sim(&app).pilot.origin[0] > 600.0, "{:?}", sim(&app).pilot.origin);
}

/// Commands for a slide when pushed straight into the queue: sprint, then duck and look around.
fn slide_cmd(n: u32) -> UserCmd {
    let duck = if n > 280 { IN_DUCK } else { 0 };
    let yaw = if n > 300 { (n as f32 * 0.05).sin() * 60.0 } else { 0.0 };
    UserCmd { command_number: n, angles: [0.0, yaw, 0.0], forward: 1.0, buttons: IN_FORWARD | IN_SPEED | duck, frametime: FT, random_seed: command_seed(n), ..Default::default() }
}

#[test]
fn two_commands_in_one_frame_run_the_view_once() {
    let mut app = sim_app(Config { spot: "hill".into(), ..Default::default() });
    app.insert_resource(Paused(true));
    let mut once = Reference::at("hill");
    let mut each = Reference::at("hill");
    let (mut differed, mut slid) = (false, false);
    let mut n = 1;
    for _ in 0..350 {
        let cmds = [slide_cmd(n), slide_cmd(n + 1)];
        n += 2;
        app.world_mut().resource_mut::<CmdQueue>().0.extend(cmds);
        app.update();
        once.frame(&cmds, false);
        each.frame(&cmds, true);
        let shell = format!("{:?}", sim(&app).pilot);
        assert_eq!(format!("{:?}", once.pilot), shell);
        differed |= format!("{:?}", each.pilot) != shell;
        slid |= once.pilot.player.sliding;
    }
    assert!(slid, "the drive never slid");
    assert!(differed, "a view frame per command was never different: the test is not sensitive");
}

#[test]
fn history_replays_bit_for_bit() {
    let mut app = sim_app(hill());
    for frame in 0..1300 {
        drive(&mut app, frame % 600);
        app.update();
    }
    let history = app.world().resource::<History>().0.clone();
    assert_eq!(history.len(), HISTORY);
    let mut pilot = history[0].pilot.clone();
    let mut clock = history[0].clock;
    let mut world = testmap::build().world();
    let s = pilot::settings();
    for snap in history.iter().skip(1) {
        pilot.step(&s, &snap.cmd, clock.now(), &mut world);
        clock.advance(snap.cmd.frametime);
    }
    assert_eq!(format!("{:?}", without_view(&pilot)), format!("{:?}", without_view(&sim(&app).pilot)));
    assert_eq!(clock, sim(&app).clock);
}

/// A floor of 40-unit tiles, as Minecraft's blocks make one: a hull exactly touching such a tile
/// starts every trace solid.
#[test]
fn an_empty_first_command_after_a_spawn_leaves_the_pilot_free_to_move() {
    let mut app = sim_app(Config::default());
    let tile = |x: f32, y: f32| [[[x, y, 0.0], [x + 40.0, y, 0.0], [x + 40.0, y + 40.0, 0.0]], [[x, y, 0.0], [x + 40.0, y + 40.0, 0.0], [x, y + 40.0, 0.0]]];
    let tris = (-20..20).flat_map(|i| (-20..20).flat_map(move |j| tile(i as f32 * 40.0, j as f32 * 40.0))).collect();
    app.world_mut().resource_mut::<Map>().0 = CollisionWorld::from_tris(tris);
    app.world_mut().write_message(Respawn::At { origin: [20.0, 20.0, 0.0], yaw: 0.0 });
    for _ in 0..3 {
        app.update();
    }
    let start = sim(&app).pilot.origin;
    down(&mut app, KeyCode::KeyW);
    for _ in 0..50 {
        app.update();
    }
    let o = sim(&app).pilot.origin;
    assert!((o[0] - start[0]).hypot(o[1] - start[1]) > 10.0, "{start:?} to {o:?}");
}

#[test]
fn respawn_resets_clock_not_command_numbers() {
    let mut app = sim_app(Config::default());
    down(&mut app, KeyCode::KeyW);
    for _ in 0..50 {
        app.update();
    }
    let before = app.world().resource::<CmdBuilder>().next_number;
    assert!(sim(&app).clock.now() > Clock::START as f32);
    app.world_mut().write_message(Respawn::Spot(0));
    app.update();
    let after = app.world().resource::<CmdLog>().0.last().unwrap().1;
    assert_eq!(after.command_number, before);
    let mut clock = Clock::default();
    clock.advance(after.frametime);
    assert_eq!(sim(&app).clock, clock);
    assert_eq!(app.world().resource::<History>().0.len(), 1);
}

#[test]
fn held_jump_through_respawn_does_not_jump() {
    let mut app = sim_app(Config::default());
    app.update();
    down(&mut app, KeyCode::Space);
    app.update();
    assert!(sim(&app).pilot.velocity[2] > 0.0, "the first press jumps");
    for _ in 0..10 {
        app.update();
    }
    app.world_mut().write_message(Respawn::Spot(0));
    app.update();
    let p = &sim(&app).pilot;
    assert_ne!(app.world().resource::<CmdLog>().0.last().unwrap().1.buttons & IN_JUMP, 0);
    assert_eq!(p.velocity[2], 0.0);
}

#[test]
fn kill_plane_respawns() {
    let mut app = sim_app(Config::default());
    app.world_mut().write_message(Respawn::At { origin: [20000.0, 0.0, 0.0], yaw: 0.0 });
    let mut fell = false;
    for _ in 0..700 {
        app.update();
        fell |= sim(&app).pilot.origin[2] < -1000.0;
    }
    assert!(fell);
    let spot = {
        let spots = app.world().resource::<Spots>();
        spots.spot().origin
    };
    let p = sim(&app).pilot.origin;
    assert!((0..3).all(|i| (p[i] - spot[i]).abs() < 0.1), "{p:?} vs {spot:?}");
    let k = app.world().resource::<History>().0.len();
    assert!(k > 0 && k < 200, "{k} commands since the respawn");
    let mut clock = Clock::default();
    for _ in 0..k {
        clock.advance(FT);
    }
    assert_eq!(sim(&app).clock, clock);
    let log = &app.world().resource::<CmdLog>().0;
    assert_eq!(log.last().unwrap().1.command_number as usize, log.len());
}

#[test]
fn pose_before_the_first_command() {
    let app = sim_app(Config { fov_scale: 1.0, ..Default::default() });
    assert_eq!(app.world().resource::<Pose>().0.fov, 70.0);
    assert_eq!(sim(&app).view_now, Clock::START as f32);
}

#[test]
fn a_respawn_resets_the_look_to_its_yaw() {
    let mut app = sim_app(Config::default());
    motion(&mut app, 300.0, 200.0);
    app.update();
    app.update();
    app.world_mut().write_message(Respawn::At { origin: [0.0, 0.0, 0.0], yaw: 90.0 });
    app.update();
    let c = app.world().resource::<CmdLog>().0.last().unwrap().1;
    assert_eq!(c.angles, [0.0, 90.0, 0.0]);
    assert_eq!(sim(&app).pilot.player.view_angles, [0.0, 90.0, 0.0]);
}

#[test]
fn named_respawns_select_a_spot_and_unknown_names_are_ignored() {
    let mut app = sim_app(Config::default());
    app.update();
    app.world_mut().write_message(Respawn::Named("ramp 45".into()));
    app.update();
    assert_eq!(app.world().resource::<Spots>().spot().name, "ramp 45");
    let before = format!("{:?}", sim(&app).pilot.origin);
    app.world_mut().write_message(Respawn::Named("nowhere".into()));
    app.update();
    assert_eq!(app.world().resource::<Spots>().spot().name, "ramp 45");
    assert_eq!(format!("{:?}", sim(&app).pilot.origin), before);
}

#[test]
fn without_view_drops_only_the_view_fields() {
    let a = Pilot::new([0.0; 3]);
    let mut b = a.clone();
    b.player.slide_tilt = [0.3, -0.2, 0.0];
    (b.player.fov_start, b.player.fov_duration, b.player.fov_from, b.player.fov_to) = (101.0, 0.25, 1.0, 1.1);
    assert_ne!(format!("{a:?}"), format!("{b:?}"));
    assert_eq!(format!("{:?}", without_view(&a)), format!("{:?}", without_view(&b)));
    b.player.sprint_tilt = 0.5;
    assert_ne!(format!("{:?}", without_view(&a)), format!("{:?}", without_view(&b)));
}
