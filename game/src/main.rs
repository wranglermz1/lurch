use bevy::input::InputPlugin;
use bevy::prelude::*;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::render::RenderPlugin;
use bevy::window::PresentMode;
use bevy::winit::WinitSettings;
use lurch_game::convar::{self, ConVarPlugin, ConVars};
use lurch_game::script::ScriptPlugin;
use lurch_game::{args, install, AudioPlugin, CmdPlugin, DataPlugin, DevPlugin, HudPlugin, LimiterPlugin, Paused, RuiPlugin, SimPlugin, SingleThreadedPlugin, ViewPlugin, ViewmodelPlugin};

fn main() {
    let cfg_path = std::env::current_exe().ok().map(|p| p.with_file_name("lurch.cfg"));
    let file = cfg_path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    let mut cfg = args::parse(args::file_args(&file).into_iter().chain(std::env::args().skip(1))).unwrap_or_else(|e| {
        eprintln!("{e}\n{}", args::USAGE);
        std::process::exit(2);
    });
    cfg.cli = std::env::args().skip(1).filter(|a| a.starts_with("--")).collect();
    if cfg.fps_max == 0 && cfg.cli.iter().any(|f| f == "--fps-max") {
        eprintln!("--fps-max 0: no fps cap; above 350 fps each command still counts 1/350 s, so movement runs fast");
    }
    if !cfg.headless {
        cfg.r5r = install::resolve(cfg.r5r.take(), !cfg.scripted, cfg_path.as_deref());
    }
    let spots: Vec<String> = lurch::testmap::build().spots.into_iter().map(|s| s.name).collect();
    if !spots.contains(&cfg.spot) {
        eprintln!("no spot {:?}; spots: {}", cfg.spot, spots.join(", "));
        std::process::exit(2);
    }
    let mut app = App::new();
    app.insert_resource(cfg.clone());
    if cfg.headless {
        if cfg.shot.is_some() {
            eprintln!("--shot is ignored with --headless");
        }
        app.add_plugins((MinimalPlugins, InputPlugin, CmdPlugin, SimPlugin, LimiterPlugin, SingleThreadedPlugin));
    } else {
        let window = Window {
            title: "lurch".into(),
            resolution: cfg.size.into(),
            present_mode: if cfg.vsync { PresentMode::Fifo } else { PresentMode::AutoNoVsync },
            desired_maximum_frame_latency: cfg.frame_latency,
            ..default()
        };
        let sync = cfg.shot.is_some() || cfg.scripted;
        let mut plugins = DefaultPlugins
            .build()
            .set(WindowPlugin { primary_window: Some(window), ..default() })
            .set(RenderPlugin { synchronous_pipeline_compilation: sync, ..default() });
        if !cfg.pipelined {
            plugins = plugins.disable::<PipelinedRenderingPlugin>();
        }
        if !cfg.scripted {
            app.insert_resource(Paused(true));
        }
        if let Some(dir) = cfg_path.as_ref().and_then(|p| p.parent()).filter(|_| !cfg.scripted) {
            let convars = ConVars::load(dir.to_path_buf(), convar::game_dir());
            convars.apply(&mut cfg);
            app.insert_resource(convars).insert_resource(cfg.clone());
        }
        app.insert_resource(WinitSettings::continuous())
            .add_plugins(plugins)
            .add_plugins((CmdPlugin, SimPlugin, ViewPlugin::default(), DataPlugin, ViewmodelPlugin, AudioPlugin, HudPlugin, RuiPlugin, LimiterPlugin, DevPlugin, SingleThreadedPlugin, ConVarPlugin));
    }
    if let Some(script) = cfg.script.clone() {
        app.add_plugins(ScriptPlugin(script));
    }
    app.run();
}
