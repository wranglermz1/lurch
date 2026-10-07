use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
use bevy::render::RenderPlugin;
use lurch_game::{args, CmdPlugin, Config, FrameIndex, HudPlugin, LimiterPlugin, SimPlugin, ViewPlugin};

fn main() {
    let cfg = args::parse(std::env::args().skip(1)).unwrap_or_else(|e| {
        eprintln!("{e}\n{}", args::USAGE);
        std::process::exit(2);
    });
    let window = Window { title: "lurch".into(), ..default() };
    let mut app = App::new();
    app.insert_resource(cfg.clone())
        .insert_resource(ClearColor(Color::srgb(0.55, 0.70, 0.85)))
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin { primary_window: Some(window), ..default() })
                .set(RenderPlugin { synchronous_pipeline_compilation: cfg.shot.is_some(), ..default() }),
        )
        .add_plugins((CmdPlugin, SimPlugin, ViewPlugin, HudPlugin, LimiterPlugin))
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(Camera3d::default());
        })
        .add_systems(Last, take_shot.before(lurch_game::advance_frame));
    app.run();
}

fn take_shot(mut commands: Commands, frame: Res<FrameIndex>, cfg: Res<Config>) {
    let Some((at, path)) = &cfg.shot else { return };
    if frame.0 != *at {
        return;
    }
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path.clone()))
        .observe(|_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            exit.write(AppExit::Success);
        });
}
