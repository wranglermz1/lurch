//! The client's key buttons, and how one command's buttons and move values are read from
//! them (docs/oracle.md, Client input).

use super::buttons::*;
use super::setup::UserCmd;

pub const DOWN: u8 = 1;
pub const IMPULSE_DOWN: u8 = 2;
pub const IMPULSE_UP: u8 = 4;

/// What a key adds to a move value this command, from its kbutton state.
pub fn key_state(state: u8) -> f32 {
    let down = (state & DOWN) as f32;
    let up = ((state & IMPULSE_UP) >> 2) as f32;
    if state & IMPULSE_DOWN != 0 {
        up * 0.25 + down * 0.5
    } else if state & IMPULSE_UP == 0 && state & DOWN != 0 {
        1.0
    } else {
        0.0
    }
}

/// Every button's kbutton state at once, one bit per button in each mask.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Keys {
    down: u32,
    impulse_down: u32,
    impulse_up: u32,
    last: u32,
}

impl Keys {
    pub fn press(&mut self, bits: u32) {
        let new = bits & !self.down;
        self.down |= new;
        self.impulse_down |= new;
    }

    pub fn release(&mut self, bits: u32) {
        let gone = bits & self.down;
        self.down &= !gone;
        self.impulse_up |= gone;
    }

    pub fn release_all(&mut self) {
        self.release(self.down);
    }

    pub fn state(&self, bit: u32) -> u8 {
        let has = |mask: u32, s: u8| if mask & bit != 0 { s } else { 0 };
        has(self.down, DOWN) | has(self.impulse_down, IMPULSE_DOWN) | has(self.impulse_up, IMPULSE_UP)
    }

    /// Writes this command's buttons and move values, then starts the next command's impulses.
    /// A button pressed since the last command keeps its bit even if already released: the
    /// game's press latch, which is what lets a scroll-wheel tap reach the movement code.
    pub fn fill(&mut self, cmd: &mut UserCmd) {
        let mut buttons = self.down | self.impulse_down;
        if (buttons & !self.last) & (IN_SPEED | IN_DODGE) != 0 {
            buttons |= IN_SPEED_PRESSED;
        }
        cmd.buttons = buttons;
        cmd.forward = key_state(self.state(IN_FORWARD)) - key_state(self.state(IN_BACK));
        cmd.side = key_state(self.state(IN_MOVERIGHT)) - key_state(self.state(IN_MOVELEFT));
        self.impulse_down = 0;
        self.impulse_up = 0;
        self.last = buttons;
    }
}
