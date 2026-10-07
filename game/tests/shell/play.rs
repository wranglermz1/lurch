use bevy::input::InputPlugin;
use bevy::prelude::*;
use lurch_game::{CmdPlugin, FrameIndex, SimPlugin};

#[test]
fn headless_plugins_update() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, InputPlugin, CmdPlugin, SimPlugin));
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(app.world().resource::<FrameIndex>().0, 3);
}
