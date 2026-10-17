//! Renders a script's frames offscreen with the viewmodel and saves some as PNG:
//! `vmshot INSTALL SCRIPT OUTDIR FRAME[,FRAME...] [LEGEND [HEIRLOOM]]`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::app::PluginsState;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::render::RenderPlugin;
use bevy::tasks::tick_global_task_pools_on_main_thread;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use lurch_game::data::{self, Data};
use lurch_game::script::{self, ScriptPlugin};
use lurch_game::view::{Target, ViewImage};
use lurch_game::convar::ConVars;
use lurch_game::rui::Preloaded;
use lurch_game::{CmdPlugin, Config, HudPlugin, RuiPlugin, SimPlugin, ViewPlugin, ViewmodelPlugin};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let install = PathBuf::from(&a[1]);
    let script = script::parse(&std::fs::read_to_string(&a[2]).unwrap()).unwrap();
    let out = PathBuf::from(&a[3]);
    let frames: Vec<u64> = a[4].split(',').map(|f| f.parse().unwrap()).collect();
    let legend = a.get(5).cloned().unwrap_or_else(|| "wraith".into());
    let t = std::time::Instant::now();
    let heirloom = a.get(6).and_then(|h| lurch::assets::HEIRLOOMS.iter().find(|x| x.name == h));
    let d = data::load(&install, &legend, heirloom).unwrap();
    eprintln!("data in {:?}", t.elapsed());
    let ui = lurch::rui::Ui::open(&install).map_err(|e| eprintln!("no UI: {e}")).ok();
    let last = *frames.iter().max().unwrap();
    let mut app = App::new();
    let cfg = Config { scripted: true, script: Some(script.clone()), r5r: Some(install), legend, heirloom, ..Default::default() };
    app.insert_resource(cfg).insert_resource(Data(Arc::new(d))).insert_resource(Preloaded(std::sync::Mutex::new(ui))).add_plugins((
        DefaultPlugins
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(RenderPlugin { synchronous_pipeline_compilation: true, ..default() })
            .disable::<WinitPlugin>(),
        CmdPlugin,
        SimPlugin,
        ViewPlugin { target: Target::Image { w: 960, h: 540 } },
        ViewmodelPlugin,
        HudPlugin,
        RuiPlugin,
        ScriptPlugin(script),
    ));
    // The image's defaults, for the settings menu to show; nothing acts on them here.
    app.init_resource::<ConVars>();
    while app.plugins_state() == PluginsState::Adding {
        tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();
    for frame in 0..=last + 30 {
        app.update();
        if frames.contains(&frame) {
            let image = app.world().resource::<ViewImage>().0.clone();
            let path: PathBuf = Path::new(&out).join(format!("f{frame:05}.png"));
            app.world_mut().spawn(Screenshot::image(image)).observe(save_to_disk(path));
        }
    }
    for _ in 0..30 {
        app.update();
    }
}
