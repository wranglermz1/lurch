use bevy::input::mouse::MouseScrollUnit;
use bevy::prelude::*;
use lurch::sim::buttons::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Bits(u32),
    Respawn,
    PrevSpot,
    NextSpot,
    ToggleHud,
    /// Opens and closes the shell's dev menu (`crate::dev`).
    DevMenu,
    /// Does nothing in lurch: bound as the game binds it, so prompts show its key.
    Ping,
    /// Does nothing in lurch, which models no ultimate: bound so the ultimate's panel shows
    /// its key.
    Ultimate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Input {
    Key(KeyCode),
    Mouse(MouseButton),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Binds(pub Vec<(Input, Action)>);

impl Default for Binds {
    fn default() -> Self {
        use Input::{Key, Mouse};
        Binds(vec![
            (Key(KeyCode::KeyW), Action::Bits(IN_FORWARD)),
            (Key(KeyCode::KeyS), Action::Bits(IN_BACK)),
            (Key(KeyCode::KeyA), Action::Bits(IN_MOVELEFT)),
            (Key(KeyCode::KeyD), Action::Bits(IN_MOVERIGHT)),
            (Key(KeyCode::Space), Action::Bits(IN_JUMP)),
            (Key(KeyCode::ControlLeft), Action::Bits(IN_DUCK)),
            (Key(KeyCode::KeyC), Action::Bits(IN_TOGGLE_DUCK)),
            (Key(KeyCode::ShiftLeft), Action::Bits(IN_SPEED)),
            (Key(KeyCode::KeyE), Action::Bits(IN_USE)),
            (Key(KeyCode::KeyQ), Action::Bits(IN_OFFHAND1)),
            (Key(KeyCode::KeyR), Action::Respawn),
            (Key(KeyCode::BracketLeft), Action::PrevSpot),
            (Key(KeyCode::BracketRight), Action::NextSpot),
            (Key(KeyCode::F1), Action::ToggleHud),
            (Key(KeyCode::F2), Action::DevMenu),
            (Mouse(MouseButton::Left), Action::Bits(IN_ATTACK)),
            (Mouse(MouseButton::Middle), Action::Ping),
        ])
    }
}

impl Binds {
    pub fn get(&self, input: Input) -> Option<Action> {
        self.0.iter().rev().find(|(i, _)| *i == input).map(|(_, a)| *a)
    }

    /// Binds `action` to `input` alone, or to nothing.
    pub fn rebind(&mut self, action: Action, input: Option<Input>) {
        self.0.retain(|(_, a)| *a != action);
        if let Some(input) = input {
            self.set(input, action);
        }
    }

    pub fn set(&mut self, input: Input, action: Action) {
        self.0.retain(|(i, _)| *i != input);
        self.0.push((input, action));
    }

    /// The binds as the game's key-name table takes them (`lurch::rui::tokens`): each bound
    /// key's code, by code, with the bind the game's default config (`config_default_pc.cfg`)
    /// gives the command. Actions the game has no command for are left out.
    pub fn game_binds(&self) -> Vec<(u16, bool, &'static str)> {
        let mut out: Vec<(u16, bool, &'static str)> = Vec::new();
        for &(input, _) in &self.0 {
            let (Some(code), Some(cmd)) = (button_code(input), self.get(input).and_then(command)) else { continue };
            if !out.iter().any(|b| b.0 == code) {
                out.push((code, false, cmd));
            }
        }
        out.sort_by_key(|b| b.0);
        out
    }
}

/// The game's command for an action, bound as its default config binds it.
fn command(action: Action) -> Option<&'static str> {
    let bits = match action {
        Action::Bits(bits) => bits,
        Action::Ping => return Some("+ping"),
        Action::Ultimate => return Some("+offhand4"),
        _ => return None,
    };
    Some(match bits {
        IN_FORWARD => "+forward",
        IN_BACK => "+backward",
        IN_MOVELEFT => "+moveleft",
        IN_MOVERIGHT => "+moveright",
        IN_JUMP => "+jump",
        IN_DUCK => "+duck",
        IN_TOGGLE_DUCK => "+toggle_duck",
        IN_SPEED => "+speed",
        IN_USE => "+use; +use_long",
        IN_OFFHAND1 => "+offhand1",
        IN_ATTACK => "+attack",
        _ => return None,
    })
}

use lurch::vgui::keybinds::KEY_NAMES;

const MWHEELUP: u16 = 0x71;
const MWHEELDOWN: u16 = 0x72;

/// What lurch does for a command of the game's: the first part of a bind's `;` list it knows.
fn action(command: &str) -> Option<Action> {
    command.split(';').find_map(|c| {
        Some(match c.trim() {
            "+forward" => Action::Bits(IN_FORWARD),
            "+backward" => Action::Bits(IN_BACK),
            "+moveleft" => Action::Bits(IN_MOVELEFT),
            "+moveright" => Action::Bits(IN_MOVERIGHT),
            "+jump" => Action::Bits(IN_JUMP),
            "+duck" => Action::Bits(IN_DUCK),
            "+toggle_duck" => Action::Bits(IN_TOGGLE_DUCK),
            "+speed" => Action::Bits(IN_SPEED),
            "+use" => Action::Bits(IN_USE),
            "+offhand1" => Action::Bits(IN_OFFHAND1),
            "+attack" => Action::Bits(IN_ATTACK),
            "+ping" => Action::Ping,
            "+offhand4" => Action::Ultimate,
            _ => return None,
        })
    })
}

impl Binds {
    /// The binds from the game's `bind_US_standard "KEY" "COMMAND" N` lines (key, command):
    /// lurch's own actions where they are, every movement and ping bind replaced by the game's.
    /// A command lurch does not model leaves its key as it is. Also the wheel's bits, up then
    /// down, when the lines bind the wheel to movement.
    pub fn from_game(lines: &[(String, String)]) -> (Binds, Option<u32>, Option<u32>) {
        let mut binds = Binds(Binds::default().0.into_iter().filter(|(_, a)| !matches!(a, Action::Bits(_) | Action::Ping)).collect());
        let (mut up, mut down) = (None, None);
        for (key, command) in lines {
            let Some(code) = KEY_NAMES.iter().position(|n| n == key).map(|i| i as u16 + 1) else { continue };
            let Some(a) = action(command) else { continue };
            match code {
                MWHEELUP => up = if let Action::Bits(b) = a { Some(b) } else { None },
                MWHEELDOWN => down = if let Action::Bits(b) = a { Some(b) } else { None },
                _ => {
                    if let Some(input) = input(code) {
                        binds.set(input, a);
                    }
                }
            }
        }
        (binds, up, down)
    }
}

/// The input of a key code, but the wheel's.
fn input(code: u16) -> Option<Input> {
    let key = |k: &KeyCode| button_code(Input::Key(*k)) == Some(code);
    KEYS.iter().find(|k| key(k)).map(|&k| Input::Key(k)).or_else(|| MOUSE.iter().find(|&&(c, _)| c == code).map(|&(_, b)| Input::Mouse(b)))
}

const MOUSE: [(u16, MouseButton); 5] = [(0x6c, MouseButton::Left), (0x6d, MouseButton::Right), (0x6e, MouseButton::Middle), (0x6f, MouseButton::Back), (0x70, MouseButton::Forward)];

/// The keyboard's keys by code from 1, as `scripts/keys_keyboard.rson` lists them.
const KEYS: [KeyCode; 103] = {
    use KeyCode::*;
    [
        Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
        Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9,
        NumpadDivide, NumpadMultiply, NumpadSubtract, NumpadAdd, NumpadEnter, NumpadDecimal,
        BracketLeft, BracketRight, Semicolon, Quote, Backquote, Comma, Period, Slash, Backslash, Minus, Equal,
        Enter, Space, Backspace, Tab, CapsLock, NumLock, Escape, ScrollLock, Insert, Delete, Home, End, PageUp, PageDown, Pause,
        ShiftLeft, ShiftRight, AltLeft, AltRight, ControlLeft, ControlRight, SuperLeft, SuperRight, ContextMenu,
        ArrowUp, ArrowLeft, ArrowDown, ArrowRight,
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    ]
};

/// An input's code in the game's key list (`ButtonCode_t`: the line of
/// `scripts/keys_keyboard.rson` that names it).
pub fn button_code(input: Input) -> Option<u16> {
    match input {
        Input::Key(k) => KEYS.iter().position(|&c| c == k).map(|i| i as u16 + 1),
        Input::Mouse(b) => MOUSE.iter().find(|m| m.1 == b).map(|m| m.0),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WheelMode {
    #[default]
    Event,
    Notch,
}

/// One wheel tap per this many pixel-unit scroll (touchpads). Ours.
pub const PIXELS_PER_NOTCH: f32 = 120.0;

/// The physical inputs holding button bits down, so a bit is released only when no input
/// bound to it is still held.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Held(Vec<(Input, u32)>);

impl Held {
    pub fn press(&mut self, input: Input, bits: u32) -> bool {
        if self.0.iter().any(|(i, _)| *i == input) {
            return false;
        }
        self.0.push((input, bits));
        true
    }

    pub fn release(&mut self, input: Input) -> Option<u32> {
        let at = self.0.iter().position(|(i, _)| *i == input)?;
        Some(self.0.swap_remove(at).1)
    }

    pub fn bits(&self) -> u32 {
        self.0.iter().fold(0, |b, (_, bits)| b | bits)
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

/// Turns wheel messages into taps: positive counts are up, negative down.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wheel {
    lines: f32,
    pixels: f32,
}

impl Wheel {
    pub fn taps(&mut self, mode: WheelMode, unit: MouseScrollUnit, y: f32) -> i32 {
        match (unit, mode) {
            (MouseScrollUnit::Line, WheelMode::Event) => y.signum() as i32 * (y != 0.0) as i32,
            (MouseScrollUnit::Line, WheelMode::Notch) => notches(&mut self.lines, y, 1.0),
            (MouseScrollUnit::Pixel, _) => notches(&mut self.pixels, y, PIXELS_PER_NOTCH),
        }
    }

    pub fn reset(&mut self) {
        *self = Wheel::default();
    }
}

fn notches(acc: &mut f32, y: f32, per: f32) -> i32 {
    if y == 0.0 {
        return 0;
    }
    if *acc != 0.0 && acc.signum() != y.signum() {
        *acc = 0.0;
    }
    *acc += y;
    let n = (*acc / per).trunc();
    *acc -= n * per;
    n as i32
}
