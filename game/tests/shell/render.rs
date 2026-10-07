use std::sync::{Arc, Mutex};

use bevy::app::PluginsState;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::render::RenderPlugin;
use bevy::tasks::tick_global_task_pools_on_main_thread;
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use lurch_game::view::{Target, ViewImage};
use lurch_game::{CmdPlugin, Config, SimPlugin, ViewPlugin};

const W: u32 = 640;
const H: u32 = 360;

/// Renders the first person view at wall 1024 offscreen and returns the RGBA bytes.
fn render() -> Vec<u8> {
    let mut app = App::new();
    app.insert_resource(Config { spot: "wall 1024".into(), fov_scale: 1.0, ..Default::default() }).add_plugins((
        DefaultPlugins
            .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
            .set(RenderPlugin { synchronous_pipeline_compilation: true, ..default() })
            .disable::<WinitPlugin>(),
        CmdPlugin,
        SimPlugin,
        ViewPlugin { target: Target::Image { w: W, h: H } },
    ));
    while app.plugins_state() == PluginsState::Adding {
        tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();
    let got: Arc<Mutex<Option<Vec<u8>>>> = Arc::default();
    for frame in 0..200 {
        app.update();
        if frame == 20 {
            let image = app.world().resource::<ViewImage>().0.clone();
            let sink = got.clone();
            app.world_mut().spawn(Screenshot::image(image)).observe(move |c: On<ScreenshotCaptured>| {
                *sink.lock().unwrap() = c.image.data.clone();
            });
        }
        if let Some(data) = got.lock().unwrap().take() {
            return data;
        }
    }
    panic!("no screenshot within 200 frames");
}

fn px(data: &[u8], x: u32, y: u32) -> [i32; 3] {
    let i = ((y * W + x) * 4) as usize;
    [data[i] as i32, data[i + 1] as i32, data[i + 2] as i32]
}

#[test]
#[ignore = "needs a Vulkan driver (lavapipe in the cloud)"]
fn first_person_view_at_wall_1024() {
    let data = render();
    assert_eq!(data.len(), (W * H * 4) as usize);
    let sky = [140, 178, 217];
    let (corner, centre, floor) = (px(&data, 0, 0), px(&data, W / 2, H / 2), px(&data, W / 2, H - 1));
    println!("top-left {corner:?} centre {centre:?} bottom-centre {floor:?}");
    assert!((0..3).all(|i| (corner[i] - sky[i]).abs() <= 1), "top-left is the sky: {corner:?}");
    assert!(centre != sky && centre[0] > centre[2] + 10, "centre is the warm face of wall 1024: {centre:?}");
    assert!(floor != sky && (floor[0] - floor[1]).abs() <= 6 && (floor[1] - floor[2]).abs() <= 6, "bottom-centre is floor grey: {floor:?}");
}
