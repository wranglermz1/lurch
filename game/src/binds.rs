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
        use Input::Key;
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
        ])
    }
}

impl Binds {
    pub fn get(&self, input: Input) -> Option<Action> {
        self.0.iter().rev().find(|(i, _)| *i == input).map(|(_, a)| *a)
    }

    pub fn set(&mut self, input: Input, action: Action) {
        self.0.retain(|(i, _)| *i != input);
        self.0.push((input, action));
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
