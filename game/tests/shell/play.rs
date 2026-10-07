use std::num::NonZeroU32;
use std::path::PathBuf;
use std::time::Instant;

use bevy::input::InputPlugin;
use bevy::prelude::*;
use lurch::sim::buttons::{IN_DUCK, IN_FORWARD, IN_JUMP};
use lurch_game::args::parse;
use lurch_game::binds::WheelMode;
use lurch_game::limiter::Limiter;
use lurch_game::script::SCRIPT_FRAMETIME;
use lurch_game::{CmdPlugin, Config, FrameIndex, LimiterPlugin, SimPlugin};

fn args(s: &str) -> Result<Config, String> {
    parse(s.split_whitespace().map(String::from))
}

#[test]
fn headless_plugins_update() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, InputPlugin, CmdPlugin, SimPlugin));
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(app.world().resource::<FrameIndex>().0, 3);
}

#[test]
fn args_parse() {
    let d = args("").unwrap();
    assert_eq!(d, Config::default());
    assert_eq!((d.spot.as_str(), d.sens, d.fov_scale, d.fps_max, d.vsync, d.size), ("wall 48", 5.0, 1.27216, 240, false, (1600, 900)));
    assert_eq!((d.wheel_down, d.wheel_up, d.wheel_mode, d.frame_latency, d.pipelined), (Some(IN_FORWARD), Some(IN_JUMP), WheelMode::Event, None, false));
    assert_eq!((d.scripted, d.headless, d.frametime, d.shot.clone()), (false, false, SCRIPT_FRAMETIME, None));

    let script = format!("{}/scripts/tour.txt", env!("CARGO_MANIFEST_DIR"));
    let c = parse(
        [
            "--spot", "hill", "--shot", "30:/tmp/x.png", "--script", &script, "--frametime", "0.0041666665", "--headless", "--sens", "1.9", "--fov-scale", "1.55", "--fps-max", "350", "--vsync",
            "--size", "1280x720", "--wheel-down", "duck", "--wheel-up", "none", "--wheel-mode", "notch", "--frame-latency", "1", "--pipelined",
        ]
        .map(String::from)
        .into_iter(),
    )
    .unwrap();
    assert_eq!(c.spot, "hill");
    assert_eq!(c.shot, Some((30, PathBuf::from("/tmp/x.png"))));
    assert!(c.scripted && c.headless && c.script.as_ref().unwrap().exit_frame().is_some());
    assert_eq!(c.frametime.to_bits(), 0.0041666665f32.to_bits());
    assert_eq!((c.sens, c.fov_scale, c.fps_max, c.vsync, c.size), (1.9, 1.55, 350, true, (1280, 720)));
    assert_eq!((c.wheel_down, c.wheel_up, c.wheel_mode), (Some(IN_DUCK), None, WheelMode::Notch));
    assert_eq!((c.frame_latency, c.pipelined), (NonZeroU32::new(1), true));

    for bad in [
        "--bogus", "--spot", "--shot 30", "--shot x:/a.png", "--shot 30:", "--script /no/such/file", "--headless", "--frametime 0.004", "--sens 0", "--sens x", "--fov-scale -1",
        "--fps-max 0", "--fps-max 351", "--fps-max x", "--size 1280", "--size 0x720", "--wheel-down left", "--wheel-mode wheel", "--frame-latency 0",
    ] {
        assert!(args(bad).is_err(), "{bad} should be refused");
    }
}

#[test]
fn limiter_holds_the_period() {
    let mut l = Limiter::new(200);
    let t = Instant::now();
    for _ in 0..50 {
        l.wait();
    }
    assert!(t.elapsed().as_secs_f64() >= 0.245, "{:?}", t.elapsed());
}

#[test]
fn limiter_plugin_headless() {
    let mut app = App::new();
    app.insert_resource(Config { fps_max: 200, ..Default::default() }).add_plugins((MinimalPlugins, LimiterPlugin));
    let t = Instant::now();
    for _ in 0..50 {
        app.update();
    }
    assert!(t.elapsed().as_secs_f64() >= 0.24, "{:?}", t.elapsed());
}

fn spot_name(app: &App) -> String {
    app.world().resource::<lurch_game::sim::Spots>().spot().name.clone()
}

fn tap(app: &mut App, k: KeyCode) {
    super::input::down(app, k);
    super::input::up(app, k);
    app.update();
}

#[test]
fn r_respawns_at_the_current_spot() {
    let mut app = super::sim::sim_app(Config::default());
    app.update();
    super::input::down(&mut app, KeyCode::KeyW);
    for _ in 0..100 {
        app.update();
    }
    super::input::up(&mut app, KeyCode::KeyW);
    app.update();
    let spot = app.world().resource::<lurch_game::sim::Spots>().spot().origin;
    assert!(app.world().resource::<lurch_game::sim::Sim>().pilot.origin[0] > spot[0] + 50.0);
    tap(&mut app, KeyCode::KeyR);
    let p = app.world().resource::<lurch_game::sim::Sim>().pilot.origin;
    assert!((0..3).all(|i| (p[i] - spot[i]).abs() < 0.1), "{p:?} vs {spot:?}");
    assert_eq!(spot_name(&app), "wall 48");
}

#[test]
fn brackets_move_between_spots_and_wrap() {
    let mut app = super::sim::sim_app(Config::default());
    app.update();
    let names: Vec<String> = app.world().resource::<lurch_game::sim::Spots>().list.iter().map(|s| s.name.clone()).collect();
    tap(&mut app, KeyCode::BracketLeft);
    assert_eq!(spot_name(&app), *names.last().unwrap());
    tap(&mut app, KeyCode::BracketRight);
    assert_eq!(spot_name(&app), names[0]);
    tap(&mut app, KeyCode::BracketRight);
    assert_eq!(spot_name(&app), names[1]);
    let spot = app.world().resource::<lurch_game::sim::Spots>().spot().origin;
    let p = app.world().resource::<lurch_game::sim::Sim>().pilot.origin;
    assert!((0..3).all(|i| (p[i] - spot[i]).abs() < 0.1), "{p:?} vs {spot:?}");
}

#[test]
fn f1_flips_the_hud() {
    let mut app = super::sim::sim_app(Config::default());
    app.update();
    assert!(app.world().resource::<lurch_game::HudVisible>().0);
    tap(&mut app, KeyCode::F1);
    assert!(!app.world().resource::<lurch_game::HudVisible>().0);
    tap(&mut app, KeyCode::F1);
    assert!(app.world().resource::<lurch_game::HudVisible>().0);
}

#[test]
fn hud_text_reports_speed_state_and_pause() {
    use lurch_game::hud::{hud_text, FpsMeter, HudView};
    use lurch_game::sim::{Pose, Sim, Spots};
    use lurch_game::view::DisplayInfo;
    let mut app = super::sim::sim_app(Config::default());
    app.world_mut().resource_mut::<Sim>().pilot.velocity = [299.0, 0.0, -120.0];
    let w = app.world();
    let (fps, display) = (FpsMeter::default(), DisplayInfo::default());
    let text = |paused| {
        hud_text(&HudView {
            sim: w.resource::<Sim>(),
            pose: w.resource::<Pose>(),
            builder: w.resource::<lurch_game::cmd::CmdBuilder>(),
            cfg: w.resource::<Config>(),
            spots: w.resource::<Spots>(),
            fps: &fps,
            display: &display,
            paused,
        })
    };
    let playing = text(false);
    assert!(playing.starts_with("speed 299 u/s  27.3 km/h   vz -120\nground"), "{playing}");
    assert!(playing.contains("spot wall 48") && playing.contains("wheel event") && playing.contains("cmd 0"), "{playing}");
    assert!(!playing.contains("click to play"));
    assert!(text(true).ends_with("\nclick to play"));
}

#[test]
fn the_binary_runs_a_script_headless_and_refuses_bad_input() {
    use std::process::Command;
    let exe = env!("CARGO_BIN_EXE_lurch-game");
    let script = format!("{}/scripts/tour.txt", env!("CARGO_MANIFEST_DIR"));
    let out = Command::new(exe).args(["--headless", "--script", &script]).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    let finals: Vec<&str> = stdout.lines().filter(|l| l.starts_with("final ")).collect();
    let parsed = lurch_game::script::parse(&std::fs::read_to_string(&script).unwrap()).unwrap();
    let app = super::script::run(&parsed, SCRIPT_FRAMETIME, |_, _| {});
    let inside = lurch_game::script::final_line(app.world().resource::<lurch_game::sim::Sim>(), app.world().resource::<lurch_game::cmd::CmdBuilder>());
    assert_eq!(finals, [inside.as_str()]);
    for bad in [&["--bogus"][..], &["--spot", "nowhere"], &["--headless"], &["--fps-max", "1000"]] {
        let out = Command::new(exe).args(bad).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{bad:?}");
    }
}
