//! The usercmd button bits the simulation reads or the client sets.

pub const IN_JUMP: u32 = 0x2;
pub const IN_DUCK: u32 = 0x4;
pub const IN_FORWARD: u32 = 0x8;
pub const IN_BACK: u32 = 0x10;
pub const IN_LEFT: u32 = 0x80;
pub const IN_RIGHT: u32 = 0x100;
pub const IN_MOVELEFT: u32 = 0x200;
pub const IN_MOVERIGHT: u32 = 0x400;
pub const IN_WALK: u32 = 0x800;
pub const IN_SPEED: u32 = 0x8000;
pub const IN_TOGGLE_DUCK: u32 = 0x400_0000;
pub const IN_DODGE: u32 = 0x1000_0000;
/// Our name. CreateMove sets it on the command where IN_SPEED or IN_DODGE goes down; the sim
/// never reads it.
pub const IN_SPEED_PRESSED: u32 = 0x2000_0000;
