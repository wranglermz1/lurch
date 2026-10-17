use std::time::Duration;

use bevy::input::keyboard::{Key, KeyboardFocusLost, KeyboardInput, NativeKey};
use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::input::touch::TouchPhase;
use bevy::input::{ButtonState, InputPlugin};
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::WindowFocused;
use lurch::sim::buttons::*;
use lurch::sim::input::{key_state, DOWN, IMPULSE_DOWN, IMPULSE_UP};
use lurch::sim::setup::{command_seed, UserCmd};
use lurch_game::binds::{Action, Input, WheelMode};
use lurch_game::cmd::{clamp_frametime, CmdBuilder, CmdLog, CmdPlugin};
use lurch_game::{Config, GameMenu, MenuOpen, Paused, Respawn};

pub const DT: f64 = 0.004;

pub fn app(cfg: Config) -> App {
    let mut app = App::new();
    app.insert_resource(cfg)
        .add_plugins((MinimalPlugins, InputPlugin, CmdPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(DT)))
        .init_resource::<CmdLog>();
    app
}

pub fn key(app: &mut App, key_code: KeyCode, state: ButtonState) {
    app.world_mut().write_message(KeyboardInput {
        key_code,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
}

pub fn down(app: &mut App, k: KeyCode) {
    key(app, k, ButtonState::Pressed);
}

pub fn up(app: &mut App, k: KeyCode) {
    key(app, k, ButtonState::Released);
}

pub fn mouse(app: &mut App, button: MouseButton, state: ButtonState) {
    app.world_mut().write_message(MouseButtonInput { button, state, window: Entity::PLACEHOLDER });
}

pub fn wheel(app: &mut App, unit: MouseScrollUnit, y: f32) {
    app.world_mut().write_message(MouseWheel { unit, x: 0.0, y, window: Entity::PLACEHOLDER, phase: TouchPhase::Moved });
}

pub fn motion(app: &mut App, dx: f32, dy: f32) {
    app.world_mut().write_message(MouseMotion { delta: Vec2::new(dx, dy) });
}

pub fn log(app: &App) -> Vec<(u64, UserCmd)> {
    app.world().resource::<CmdLog>().0.clone()
}

/// Runs one update and returns the command it made, if any.
pub fn step(app: &mut App) -> Option<UserCmd> {
    let before = app.world().resource::<CmdLog>().0.len();
    app.update();
    let log = &app.world().resource::<CmdLog>().0;
    (log.len() > before).then(|| log.last().unwrap().1)
}

/// An app past its first update, which makes no command.
fn ready() -> App {
    let mut a = app(Config::default());
    assert!(step(&mut a).is_none());
    a
}

#[test]
fn numbering_starts_after_the_first_update_one_per_update() {
    let mut a = app(Config::default());
    assert!(step(&mut a).is_none());
    for n in 1..=3 {
        assert_eq!(step(&mut a).unwrap().command_number, n);
    }
    let frames: Vec<u64> = log(&a).iter().map(|(f, _)| *f).collect();
    assert_eq!(frames, [1, 2, 3]);
}

#[test]
fn a_tap_of_space_jumps_on_one_command() {
    let mut a = ready();
    down(&mut a, KeyCode::Space);
    up(&mut a, KeyCode::Space);
    assert_ne!(step(&mut a).unwrap().buttons & IN_JUMP, 0);
    assert_eq!(step(&mut a).unwrap().buttons & IN_JUMP, 0);
}

#[test]
fn a_held_key_reads_press_then_held() {
    let mut a = ready();
    down(&mut a, KeyCode::KeyW);
    assert_eq!(step(&mut a).unwrap().forward, key_state(DOWN | IMPULSE_DOWN));
    assert_eq!(step(&mut a).unwrap().forward, key_state(DOWN));
    assert_eq!(step(&mut a).unwrap().forward, key_state(DOWN));
    down(&mut a, KeyCode::KeyS);
    step(&mut a);
    assert_eq!(step(&mut a).unwrap().forward, key_state(DOWN) - key_state(DOWN));
}

#[test]
fn repeat_messages_are_ignored() {
    let mut a = ready();
    down(&mut a, KeyCode::KeyW);
    step(&mut a);
    up(&mut a, KeyCode::KeyW);
    a.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::KeyW,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state: ButtonState::Pressed,
        text: None,
        repeat: true,
        window: Entity::PLACEHOLDER,
    });
    let c = step(&mut a).unwrap();
    assert_eq!(c.buttons & IN_FORWARD, 0);
    assert_eq!(c.forward, key_state(IMPULSE_UP));
}

#[test]
fn a_wheel_notch_taps_forward_for_one_command() {
    let mut a = ready();
    wheel(&mut a, MouseScrollUnit::Line, -1.0);
    let c = step(&mut a).unwrap();
    assert_ne!(c.buttons & IN_FORWARD, 0);
    assert_eq!(c.forward, key_state(IMPULSE_DOWN | IMPULSE_UP));
    let c = step(&mut a).unwrap();
    assert_eq!(c.buttons & IN_FORWARD, 0);
    assert_eq!(c.forward, 0.0);
}

#[test]
fn two_wheel_messages_in_one_frame_make_one_tap() {
    let mut a = ready();
    wheel(&mut a, MouseScrollUnit::Line, -1.0);
    wheel(&mut a, MouseScrollUnit::Line, -1.0);
    let c = step(&mut a).unwrap();
    assert_ne!(c.buttons & IN_FORWARD, 0);
    assert_eq!(c.forward, key_state(IMPULSE_DOWN | IMPULSE_UP));
    assert_eq!(step(&mut a).unwrap().buttons & IN_FORWARD, 0);
}

#[test]
fn wheel_on_consecutive_frames_sets_the_bit_on_both() {
    let mut a = ready();
    wheel(&mut a, MouseScrollUnit::Line, -1.0);
    let first = step(&mut a).unwrap();
    wheel(&mut a, MouseScrollUnit::Line, -1.0);
    let second = step(&mut a).unwrap();
    assert_ne!(first.buttons & IN_FORWARD, 0);
    assert_ne!(second.buttons & IN_FORWARD, 0);
    assert_eq!(second.buttons & !first.buttons & IN_FORWARD, 0);
}

#[test]
fn a_wheel_tap_while_w_is_held_changes_nothing() {
    let mut a = ready();
    down(&mut a, KeyCode::KeyW);
    step(&mut a);
    wheel(&mut a, MouseScrollUnit::Line, -1.0);
    let c = step(&mut a).unwrap();
    assert_ne!(c.buttons & IN_FORWARD, 0);
    assert_eq!(c.forward, key_state(DOWN));
    let c = step(&mut a).unwrap();
    assert_ne!(c.buttons & IN_FORWARD, 0);
    assert_eq!(c.forward, key_state(DOWN));
}

#[test]
fn wheel_up_taps_jump() {
    let mut a = ready();
    wheel(&mut a, MouseScrollUnit::Line, 1.0);
    let c = step(&mut a).unwrap();
    assert_ne!(c.buttons & IN_JUMP, 0);
    assert_eq!(c.buttons & IN_FORWARD, 0);
}

#[test]
fn wheel_notch_mode_accumulates_fractional_lines_ours() {
    let mut a = app(Config { wheel_mode: WheelMode::Notch, ..Default::default() });
    step(&mut a);
    let mut hits = Vec::new();
    for _ in 0..4 {
        wheel(&mut a, MouseScrollUnit::Line, -0.25);
        hits.push(step(&mut a).unwrap().buttons & IN_FORWARD != 0);
    }
    assert_eq!(hits, [false, false, false, true]);
    for y in [-0.5, 0.5, -0.5] {
        wheel(&mut a, MouseScrollUnit::Line, y);
        assert_eq!(step(&mut a).unwrap().buttons & (IN_FORWARD | IN_JUMP), 0, "a flip resets the accumulator");
    }
    wheel(&mut a, MouseScrollUnit::Line, -0.5);
    assert_ne!(step(&mut a).unwrap().buttons & IN_FORWARD, 0);
}

#[test]
fn wheel_pixels_tap_every_120_in_either_mode() {
    for mode in [WheelMode::Event, WheelMode::Notch] {
        let mut a = app(Config { wheel_mode: mode, ..Default::default() });
        step(&mut a);
        wheel(&mut a, MouseScrollUnit::Pixel, -60.0);
        assert_eq!(step(&mut a).unwrap().buttons & IN_FORWARD, 0, "{mode:?}");
        wheel(&mut a, MouseScrollUnit::Pixel, -60.0);
        assert_ne!(step(&mut a).unwrap().buttons & IN_FORWARD, 0, "{mode:?}");
        wheel(&mut a, MouseScrollUnit::Pixel, -120.0);
        assert_ne!(step(&mut a).unwrap().buttons & IN_FORWARD, 0, "{mode:?}");
    }
}

#[test]
fn focus_loss_releases_held_keys() {
    let mut a = ready();
    down(&mut a, KeyCode::KeyW);
    step(&mut a);
    a.world_mut().write_message(KeyboardFocusLost);
    up(&mut a, KeyCode::KeyW);
    let c = step(&mut a).unwrap();
    assert_eq!(c.buttons & IN_FORWARD, 0);
    assert_eq!(step(&mut a).unwrap().buttons & IN_FORWARD, 0);
}

#[test]
fn focus_loss_releases_mouse_bound_bits_without_a_mouse_release() {
    let mut cfg = Config::default();
    cfg.binds.set(Input::Mouse(MouseButton::Back), Action::Bits(IN_FORWARD));
    let mut a = app(cfg);
    step(&mut a);
    down(&mut a, KeyCode::KeyW);
    mouse(&mut a, MouseButton::Back, ButtonState::Pressed);
    step(&mut a);
    a.world_mut().write_message(KeyboardFocusLost);
    up(&mut a, KeyCode::KeyW);
    step(&mut a);
    down(&mut a, KeyCode::KeyW);
    assert_ne!(step(&mut a).unwrap().buttons & IN_FORWARD, 0);
    up(&mut a, KeyCode::KeyW);
    assert_eq!(step(&mut a).unwrap().buttons & IN_FORWARD, 0);
    assert_eq!(step(&mut a).unwrap().buttons & IN_FORWARD, 0);
}

#[test]
fn escape_opens_the_menu_which_pauses_until_escape_closes_it() {
    let mut a = ready();
    down(&mut a, KeyCode::Escape);
    assert!(step(&mut a).is_none());
    assert!(a.world().resource::<Paused>().0);
    assert_eq!(a.world().resource::<MenuOpen>().0, Some(GameMenu::System));
    up(&mut a, KeyCode::Escape);
    for _ in 0..3 {
        assert!(step(&mut a).is_none());
    }
    mouse(&mut a, MouseButton::Left, ButtonState::Pressed);
    assert!(step(&mut a).is_none(), "a click off the menu's buttons does not resume");
    mouse(&mut a, MouseButton::Left, ButtonState::Released);
    down(&mut a, KeyCode::Escape);
    assert!(step(&mut a).is_some());
    assert!(!a.world().resource::<Paused>().0 && a.world().resource::<MenuOpen>().0.is_none());
    up(&mut a, KeyCode::Escape);
    step(&mut a);
    a.world_mut().resource_mut::<MenuOpen>().0 = Some(GameMenu::Misc);
    a.world_mut().resource_mut::<Paused>().0 = true;
    down(&mut a, KeyCode::Escape);
    assert!(step(&mut a).is_some(), "Escape closes the settings menu back to the game");
    assert!(a.world().resource::<MenuOpen>().0.is_none());
}

#[test]
fn a_click_resumes_after_focus_loss_without_the_menu() {
    let mut a = ready();
    a.world_mut().write_message(WindowFocused { window: Entity::PLACEHOLDER, focused: false });
    step(&mut a);
    assert!(a.world().resource::<MenuOpen>().0.is_none());
    mouse(&mut a, MouseButton::Left, ButtonState::Pressed);
    assert!(step(&mut a).is_some());
}

#[test]
fn losing_window_focus_pauses() {
    let mut a = ready();
    a.world_mut().write_message(WindowFocused { window: Entity::PLACEHOLDER, focused: false });
    assert!(step(&mut a).is_none());
    assert!(a.world().resource::<Paused>().0);
}

#[test]
fn a_key_pressed_while_paused_is_not_held_after_resuming() {
    let mut a = ready();
    down(&mut a, KeyCode::Escape);
    step(&mut a);
    up(&mut a, KeyCode::Escape);
    down(&mut a, KeyCode::KeyW);
    step(&mut a);
    down(&mut a, KeyCode::Escape);
    let c = step(&mut a).unwrap();
    assert_eq!(c.buttons & IN_FORWARD, 0);
    assert_eq!(c.forward, 0.0);
}

#[test]
fn mouse_motion_turns_at_the_game_scale() {
    let mut a = ready();
    motion(&mut a, 100.0, 0.0);
    let c = step(&mut a).unwrap();
    assert!((c.angles[1] - -11.0).abs() < 1e-4, "{:?}", c.angles);
}

#[test]
fn pitch_stops_at_89_and_yaw_wraps() {
    let mut a = ready();
    motion(&mut a, 0.0, 10_000.0);
    assert_eq!(step(&mut a).unwrap().angles[0], 89.0);
    motion(&mut a, 0.0, -100_000.0);
    assert_eq!(step(&mut a).unwrap().angles[0], -89.0);
    for _ in 0..10 {
        motion(&mut a, -1000.0, 0.0);
        let yaw = step(&mut a).unwrap().angles[1];
        assert!(yaw > -180.0 && yaw <= 180.0, "{yaw}");
    }
    let b = a.world().resource::<CmdBuilder>().look;
    assert!(b.yaw > -180.0 && b.yaw <= 180.0);
}

#[test]
fn command_fields_frametime_and_seed() {
    let mut a = ready();
    for _ in 0..5 {
        step(&mut a);
    }
    for (_, c) in log(&a) {
        assert_eq!(c.frametime.to_bits(), clamp_frametime(DT).to_bits());
        assert_eq!(c.random_seed, command_seed(c.command_number));
    }
}

#[test]
fn respawn_and_spot_keys_write_respawn() {
    let mut a = ready();
    let mut cursor = a.world().resource::<Messages<Respawn>>().get_cursor_current();
    for (k, want) in [(KeyCode::KeyR, Respawn::Spot(0)), (KeyCode::BracketLeft, Respawn::Spot(-1)), (KeyCode::BracketRight, Respawn::Spot(1))] {
        down(&mut a, k);
        up(&mut a, k);
        a.update();
        let got: Vec<Respawn> = cursor.read(a.world().resource::<Messages<Respawn>>()).cloned().collect();
        assert_eq!(got, [want]);
    }
}

#[test]
fn frametime_clamp_is_never_zero_or_nan() {
    use lurch_game::cmd::{RETAIL_USERCMD_FRAMETIME_MAX as MAX, RETAIL_USERCMD_FRAMETIME_MIN as MIN};
    assert_eq!(clamp_frametime(0.0), MIN);
    assert_eq!(clamp_frametime(f64::NAN), MIN);
    assert_eq!(clamp_frametime(5.0), MAX);
    assert_eq!(clamp_frametime(0.004), 0.004);
}

#[test]
fn a_wheel_bound_to_none_does_nothing() {
    let mut a = app(Config { wheel_down: None, ..Default::default() });
    step(&mut a);
    wheel(&mut a, MouseScrollUnit::Line, -1.0);
    let c = step(&mut a).unwrap();
    assert_eq!((c.buttons, c.forward), (0, 0.0));
    wheel(&mut a, MouseScrollUnit::Line, 1.0);
    assert_ne!(step(&mut a).unwrap().buttons & IN_JUMP, 0);
}
