use bevy::input::InputPlugin;
use bevy::prelude::*;
use lurch::sim::buttons::*;
use lurch::sim::input::{key_state, DOWN, IMPULSE_DOWN, IMPULSE_UP};
use lurch::sim::setup::UserCmd;
use lurch_game::cmd::{CmdBuilder, CmdLog, CmdPlugin};
use lurch_game::script::{final_line, parse, parse_frametime, Script, ScriptPlugin, SCRIPT_FRAMETIME};
use lurch_game::sim::{Sim, SimPlugin};
use lurch_game::Config;

use super::sim::Reference;

pub fn script_app(script: &Script, frametime: f32) -> App {
    let mut app = App::new();
    app.insert_resource(Config { scripted: true, frametime, ..Default::default() })
        .add_plugins((MinimalPlugins, InputPlugin, CmdPlugin, SimPlugin, ScriptPlugin(script.clone())));
    app
}

/// Runs a script to its exit; calls `each` after every update.
pub fn run(script: &Script, frametime: f32, mut each: impl FnMut(&App, u64)) -> App {
    let mut app = script_app(script, frametime);
    for frame in 0..100_000 {
        app.update();
        each(&app, frame);
        if app.should_exit().is_some() {
            return app;
        }
    }
    panic!("the script never exits");
}

fn file(name: &str) -> Script {
    let path = format!("{}/scripts/{name}", env!("CARGO_MANIFEST_DIR"));
    parse(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn final_of(app: &App) -> String {
    final_line(app.world().resource::<Sim>(), app.world().resource::<CmdBuilder>())
}

#[test]
fn parser_rejects_bad_lines() {
    assert!(parse("1 down KeyQ").unwrap_err().contains("unknown key"));
    assert!(parse("1 jump").is_err());
    assert!(parse("1 dance KeyW").is_err());
    assert!(parse("5 down KeyW\n4 up KeyW").unwrap_err().contains("goes back"));
    assert!(parse("x down KeyW").is_err());
    assert!(parse("1 look 3").is_err());
    assert!(parse("1 wheel nope").is_err());
    assert!(parse("1 spot").is_err());
    assert!(parse("1 exit now").is_err());
    let ok = parse("# a comment\n\n0 spot wall 128  # trailing\n1 tap Space\n2 exit\n").unwrap();
    assert_eq!(ok.exit_frame(), Some(2));
    assert_eq!(ok.0.len(), 3);
    assert!(parse_frametime("0").is_err() && parse_frametime("-1").is_err() && parse_frametime("nan").is_err());
}

#[test]
fn commands_match_the_script() {
    let script = parse(
        "1 down KeyW
         3 tap Space
         4 up KeyW
         5 down ShiftLeft
         6 wheel -1
         7 wheel -1
         8 down KeyD
         9 up KeyD
         9 down KeyD
         11 exit",
    )
    .unwrap();
    let app = run(&script, SCRIPT_FRAMETIME, |_, _| {});
    let got: Vec<(u32, u32, f32, f32)> = app.world().resource::<CmdLog>().0.iter().map(|(_, c)| (c.command_number, c.buttons, c.forward, c.side)).collect();
    let held = key_state(DOWN);
    let pressed = key_state(DOWN | IMPULSE_DOWN);
    let tapped = key_state(IMPULSE_DOWN | IMPULSE_UP);
    let released = key_state(IMPULSE_UP);
    let repressed = key_state(DOWN | IMPULSE_DOWN | IMPULSE_UP);
    let want = [
        (1, IN_FORWARD, pressed, 0.0),
        (2, IN_FORWARD, held, 0.0),
        (3, IN_FORWARD | IN_JUMP, held, 0.0),
        (4, 0, released, 0.0),
        (5, IN_SPEED | IN_SPEED_PRESSED, 0.0, 0.0),
        (6, IN_SPEED | IN_FORWARD, tapped, 0.0),
        (7, IN_SPEED | IN_FORWARD, tapped, 0.0),
        (8, IN_SPEED | IN_MOVERIGHT, 0.0, pressed),
        (9, IN_SPEED | IN_MOVERIGHT, 0.0, repressed),
        (10, IN_SPEED | IN_MOVERIGHT, 0.0, held),
        (11, IN_SPEED | IN_MOVERIGHT, 0.0, held),
    ];
    assert_eq!(got, want);
}

#[test]
fn same_script_same_result() {
    for name in ["tour.txt", "tapstrafe.txt"] {
        let a = final_of(&run(&file(name), SCRIPT_FRAMETIME, |_, _| {}));
        let b = final_of(&run(&file(name), SCRIPT_FRAMETIME, |_, _| {}));
        assert_eq!(a, b, "{name}");
        assert!(a.starts_with("final cmd="), "{a}");
    }
}

#[test]
fn shell_equals_pilot_for_each_script() {
    for (name, spot) in [("tour.txt", "hill"), ("tapstrafe.txt", "wall 48")] {
        let mut r = Reference::at(spot);
        let mut compared = 0;
        drop(run(&file(name), SCRIPT_FRAMETIME, |app, frame| {
            let cmds: Vec<UserCmd> = app.world().resource::<CmdLog>().0.iter().filter(|(f, _)| *f == frame).map(|(_, c)| *c).collect();
            if cmds.is_empty() {
                return;
            }
            r.frame(&cmds, false);
            assert_eq!(format!("{:?}", r.pilot), format!("{:?}", app.world().resource::<Sim>().pilot), "{name} frame {frame}");
            compared += 1;
        }));
        assert!(compared > 400, "{name}: {compared}");
    }
}

#[test]
fn script_frametime_bits_are_exact() {
    let script = parse("1 down KeyW\n40 exit").unwrap();
    for ft in [0.004f32, 0.0041666665] {
        let app = run(&script, ft, |_, _| {});
        let log = &app.world().resource::<CmdLog>().0;
        assert_eq!(log.len(), 40);
        assert!(log.iter().all(|(_, c)| c.frametime.to_bits() == ft.to_bits()), "{ft}");
    }
}
