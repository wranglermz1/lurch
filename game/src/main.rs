use bevy::input::InputPlugin;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
use bevy::render::RenderPlugin;
use lurch_game::script::ScriptPlugin;
use lurch_game::{args, CmdPlugin, Config, FrameIndex, HudPlugin, LimiterPlugin, PendingShots, SimPlugin, ViewPlugin};

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
        app.add_plugins((MinimalPlugins, InputPlugin, CmdPlugin, SimPlugin));
    } else {
        let window = Window { title: "lurch".into(), ..default() };
        let sync = cfg.shot.is_some() || cfg.scripted;
        app.insert_resource(ClearColor(Color::srgb(0.55, 0.70, 0.85)))
            .add_plugins(
                DefaultPlugins
                    .set(WindowPlugin { primary_window: Some(window), ..default() })
                    .set(RenderPlugin { synchronous_pipeline_compilation: sync, ..default() }),
            )
            .add_plugins((CmdPlugin, SimPlugin, ViewPlugin, HudPlugin, LimiterPlugin))
            .add_systems(Startup, |mut commands: Commands| {
                commands.spawn(Camera3d::default());
            })
            .add_systems(Last, take_shot.before(lurch_game::advance_frame));
    }
    if let Some(script) = cfg.script.clone() {
        app.add_plugins(ScriptPlugin(script));
    }
    app.run();
}

/// Without a script the app exits once its screenshot is saved; with one, the script's
/// `exit` waits for it.
fn take_shot(mut commands: Commands, frame: Res<FrameIndex>, cfg: Res<Config>, mut pending: ResMut<PendingShots>) {
    let Some((at, path)) = &cfg.shot else { return };
    if frame.0 != *at {
        return;
    }
    pending.0 += 1;
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone())).observe(
        |_: On<ScreenshotCaptured>, mut pending: ResMut<PendingShots>, cfg: Res<Config>, mut exit: MessageWriter<AppExit>| {
            pending.0 -= 1;
            if pending.0 == 0 && !cfg.scripted {
                exit.write(AppExit::Success);
            }
        },
    );
}
