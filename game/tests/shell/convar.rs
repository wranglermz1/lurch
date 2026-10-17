use bevy::prelude::{KeyCode, MouseButton};
use lurch_game::convar::{ConVars, FCVAR_ARCHIVE, FCVAR_ARCHIVE_PLAYERPROFILE};

/// A string out of range is clamped and printed again with `%f`; one in range keeps the text
/// as given; three numbers below 256 are a colour; a float is printed with `%f`, an int with
/// `%d`, each clamped.
#[test]
fn convars_take_values_as_the_engine_does() {
    let mut c = ConVars::default();
    let sens = c.get_mut("mouse_sensitivity").unwrap();
    assert_eq!((sens.float, sens.int, sens.string.as_str()), (5.0, 5, "5"));
    sens.set_string("50");
    assert_eq!((sens.float, sens.string.as_str()), (20.0, "20.000000"));
    sens.set_string("abc");
    assert_eq!((sens.float, sens.string.as_str()), (0.1, "0.100000"));
    sens.set_string(" 1.685000x");
    assert_eq!((sens.float, sens.int, sens.string.as_str()), (1.685, 1, " 1.685000x"));
    let curve = c.get_mut("GAMEPAD_custom_curve").unwrap();
    curve.set_string("1 2 3");
    assert_eq!(curve.int, i32::from_le_bytes([1, 2, 3, 255]));
    assert_eq!(curve.float, curve.int as f32);
    curve.set_string("1.0 2 3");
    assert_eq!(curve.float, 1.0);
    let ping = c.get_mut("hud_setting_pingAlpha").unwrap();
    ping.set_float(0.5);
    assert_eq!(ping.string, "0.500000");
    let aim = c.get_mut("gamepad_aim_speed").unwrap();
    aim.set_int(9);
    assert_eq!((aim.int, aim.string.as_str()), (7, "7"));
}

/// The files run as commands keep their bind lines and skip what is not lurch's; written back,
/// the binds come first by key code, then each file holds its flag's ConVars ordered by name
/// with case aside, as `name "value"` lines.
#[test]
fn settings_files_read_and_write_as_the_game_writes_them() {
    let mut c = ConVars::default();
    c.exec("bind_US_standard \"w\" \"+forward\" 0\r\nbind_held_US_standard \"g\" \"+strafe\" 0\r\nname \"x\"\r\nmouse_sensitivity \"1.685000\"\r\n", true);
    c.exec("cl_fovScale \"1.55\"\r\nbind_US_standard \"q\" \"+offhand1\" 0\r\n", false);
    let settings = c.file(FCVAR_ARCHIVE);
    assert!(settings.starts_with("bind_held_US_standard \"g\" \"+strafe\" 0\r\nbind_US_standard \"w\" \"+forward\" 0\r\nchroma_enable \"1\"\r\n"), "{settings}");
    assert!(settings.contains("\r\nmouse_sensitivity \"1.685000\"\r\n"));
    assert!(!settings.contains("name ") && !settings.contains("offhand1") && !settings.contains("cl_fovScale"));
    let profile = c.file(FCVAR_ARCHIVE_PLAYERPROFILE);
    let at = |n: &str| profile.find(&format!("{n} \"")).unwrap_or_else(|| panic!("{n}"));
    assert!(profile.starts_with("cc_text_size \"0\"\r\ncl_fovScale \"1.55\"\r\n"), "{profile}");
    assert!(at("cl_fovScale") < at("closecaption") && at("gameCursor_Velocity") < at("gamepad_aim_speed"));
}

/// Only a ConVar in `profile.cfg` set by a script asks for the files to be written.
#[test]
fn a_profile_convar_set_by_a_script_saves_the_files() {
    let mut c = ConVars::default();
    c.script_set("mouse_sensitivity", "2");
    assert!(!c.save);
    c.script_set("player_setting_autosprint", "1");
    assert!(c.save);
}

/// With no files of its own, lurch reads the game's and writes them as its own; then it reads
/// its own.
#[test]
fn the_first_run_takes_the_games_settings() {
    let root = std::env::temp_dir().join(format!("lurch-convar-{}", std::process::id()));
    let (ours, game) = (root.join("lurch"), root.join("Apex"));
    for d in [&ours, &game.join("local"), &game.join("profile")] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(game.join("local/settings.cfg"), "mouse_sensitivity \"1.685000\"\r\n").unwrap();
    std::fs::write(game.join("profile/profile.cfg"), "cl_fovScale \"1.55\"\r\n").unwrap();
    let c = ConVars::load(ours.clone(), Some(game.clone()));
    assert!(c.save);
    assert_eq!(c.get("cl_fovscale").unwrap().string, "1.55");
    c.write().unwrap();
    std::fs::write(game.join("profile/profile.cfg"), "cl_fovScale \"1.7\"\r\n").unwrap();
    let again = ConVars::load(ours.clone(), Some(game));
    assert!(!again.save);
    assert_eq!((again.get("mouse_sensitivity").unwrap().float, again.get("cl_fovScale").unwrap().float), (1.685, 1.55));
    std::fs::remove_dir_all(root).unwrap();
}

/// The saved settings drive lurch's, but a flag on the command line itself wins; the game's
/// movement binds replace lurch's, which keeps its own actions.
#[test]
fn saved_settings_drive_the_shell_unless_the_command_line_says() {
    use lurch::sim::buttons::{IN_DUCK, IN_FORWARD, IN_OFFHAND1, IN_SPEED};
    use lurch_game::binds::{Action, Input};
    use lurch_game::Config;
    let mut c = ConVars::default();
    c.exec("bind_US_standard \"w\" \"+forward\" 0\r\nbind_US_standard \"MOUSE4\" \"+offhand1\" 1\r\nbind_US_standard \"LSHIFT\" \"+speed\" 0\r\nbind_US_standard \"r\" \"+reload\" 0\r\nbind_US_standard \"MWHEELUP\" \"+duck\" 1\r\nbind_held_US_standard \"q\" \"+jump\" 0\r\nmouse_sensitivity \"1.685000\"\r\nm_invert_pitch \"1\"\r\n", true);
    c.exec("player_setting_autosprint \"1\"\r\ncl_fovScale \"1.55\"\r\nsound_volume_sfx \"0.5\"\r\n", false);
    let mut cfg = Config { cli: vec!["--fov-scale".into()], ..Config::default() };
    c.apply(&mut cfg);
    assert_eq!((cfg.sens, cfg.fov_scale, cfg.autosprint, cfg.invert_pitch, cfg.fps_max_setting), (1.685, Config::default().fov_scale, true, true, Some(-1.0)));
    assert_eq!(cfg.volumes, [1.0, 1.0, 1.0, 1.0, 1.0, 0.5]);
    let b = &cfg.binds;
    assert_eq!(b.get(Input::Key(KeyCode::KeyW)), Some(Action::Bits(IN_FORWARD)));
    assert_eq!(b.get(Input::Mouse(MouseButton::Back)), Some(Action::Bits(IN_OFFHAND1)));
    assert_eq!(b.get(Input::Key(KeyCode::ShiftLeft)), Some(Action::Bits(IN_SPEED)));
    assert_eq!(b.get(Input::Key(KeyCode::KeyR)), Some(Action::Respawn));
    assert_eq!(b.get(Input::Key(KeyCode::Space)), None);
    assert_eq!(b.get(Input::Key(KeyCode::KeyQ)), None);
    assert_eq!((cfg.wheel_up, cfg.wheel_down), (Some(IN_DUCK), None));
}

/// `fps_max` caps frames as the engine does: -1 the desktop's refresh (at least 144 without
/// vsync), 0 or less 300.
#[test]
fn fps_max_caps_as_the_engine_does() {
    use lurch_game::limiter::engine_cap;
    assert_eq!(engine_cap(-1.0, Some(240), false), 240.0);
    assert_eq!(engine_cap(-1.0, Some(60), false), 144.0);
    assert_eq!(engine_cap(-1.0, Some(60), true), 59.94);
    assert_eq!(engine_cap(-1.0, Some(59), true), 59.94);
    assert_eq!(engine_cap(0.0, None, false), 300.0);
    assert_eq!(engine_cap(190.0, Some(240), true), 190.0);
    assert_eq!(engine_cap(20000.0, None, false), 10000.0);
}
