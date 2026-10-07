use bevy::input::InputPlugin;
use bevy::prelude::*;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::render::RenderPlugin;
use bevy::window::PresentMode;
use bevy::winit::WinitSettings;
use lurch_game::script::ScriptPlugin;
use lurch_game::{args, CmdPlugin, HudPlugin, LimiterPlugin, Paused, SimPlugin, ViewPlugin};

fn main() {
    let cfg = args::parse(std::env::args().skip(1)).unwrap_or_else(|e| {
        eprintln!("{e}\n{}", args::USAGE);
        std::process::exit(2);
    });
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
        app.add_plugins((MinimalPlugins, InputPlugin, CmdPlugin, SimPlugin, LimiterPlugin));
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
        app.insert_resource(WinitSettings::continuous())
            .add_plugins(plugins)
            .add_plugins((CmdPlugin, SimPlugin, ViewPlugin::default(), HudPlugin, LimiterPlugin));
    }
    if let Some(script) = cfg.script.clone() {
        app.add_plugins(ScriptPlugin(script));
    }
    app.run();
}
