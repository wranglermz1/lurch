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
