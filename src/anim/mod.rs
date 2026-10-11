//! Animation: which sequence the game plays on the body (third person) and the viewmodel
//! (first person) in answer to movement. The choice is by activity and activity modifiers,
//! recovered from the oracle. The sequences themselves (their activities, modifiers, length,
//! root motion and events) are art in the game's model files; they arrive through `AnimSet`,
//! which a later session implements over those files. `NoAnims` stands in until then.
//!
//! Activity, modifier and event numbers are the oracle's registration order
//! (`tools/tables.py`): sequences name them, the code compares numbers.

pub mod body;
pub mod bones;
pub mod model;
pub mod proxy;
pub mod transition;
pub mod viewmodel;

use crate::sim::cue::{Cue, LandHeight};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Activity(pub u16);

macro_rules! activities {
    ($($name:ident = $id:literal,)*) => {
        impl Activity {
            $(pub const $name: Activity = Activity($id);)*
        }
        /// The movement activities, by the names sequences use.
        pub const ACTIVITIES: &[(Activity, &str)] = &[$((Activity($id), concat!("ACT_", stringify!($name))),)*];
    };
}

activities! {
    VM_DRAW = 449,
    VM_DRAW_TO_SPRINT = 450,
    VM_DRAWFIRST = 451,
    VM_HOLSTER = 453,
    VM_HOLSTER_FROM_SPRINT = 454,
    VM_RAISE = 455,
    VM_RAISE_FROM_SPRINT = 456,
    VM_RAISE_FROM_MELEE = 457,
    VM_LOWER = 458,
    VM_IDLE = 468,
    VM_IDLE_ALT1 = 469,
    VM_IDLE_ALT2 = 470,
    VM_IDLE_ALT3 = 471,
    VM_SPRINT = 478,
    VM_MELEE_ATTACK1 = 493,
    VM_MELEE_ATTACK2 = 494,
    VM_MELEE_ATTACK3 = 495,
    VM_GRAPPLE_START = 554,
    VM_GRAPPLE_IDLE = 555,
    VM_GRAPPLE_STOP = 556,
    VM_WEAPON_INSPECT = 557,
    VM_JUMP = 558,
    VM_DOUBLEJUMP = 559,
    VM_LAND = 560,
    VM_LAND_LOW = 561,
    VM_LAND_MEDIUM = 562,
    VM_LAND_HIGH = 563,
    VM_MANTLE_BELOW = 569,
    VM_MANTLE_LEVEL = 570,
    VM_MANTLE_ABOVE = 571,
    VM_MANTLE_HIGH = 572,
    VM_MANTLE_CROUCH_BELOW = 573,
    VM_MANTLE_CROUCH_LEVEL = 574,
    VM_MANTLE_CROUCH_ABOVE = 575,
    VM_MANTLE_CROUCH_HIGH = 576,
    VM_MANTLE_IDLE = 577,
    VM_MANTLE_JUMPOFF = 578,
    VM_ONEHANDED_SPRINT = 592,
    MP_STAND_IDLE = 649,
    MP_CROUCH_IDLE = 650,
    MP_CROUCH_DEPLOYED_IDLE = 651,
    MP_CROUCH_DEPLOYED = 652,
    MP_DEPLOYED_IDLE = 653,
    MP_RUN = 654,
    MP_RUN_FORWARD = 655,
    MP_RUN_BACKWARD = 656,
    MP_RUN_LEFT = 657,
    MP_RUN_RIGHT = 658,
    MP_STAGGERED_RUN = 659,
    MP_STAGGERED_RUN_FORWARD = 660,
    MP_STAGGERED_RUN_BACKWARD = 661,
    MP_STAGGERED_RUN_LEFT = 662,
    MP_STAGGERED_RUN_RIGHT = 663,
    MP_WALK = 664,
    MP_WALK_FORWARD = 665,
    MP_WALK_BACKWARD = 666,
    MP_WALK_LEFT = 667,
    MP_WALK_RIGHT = 668,
    MP_STAGGERED_WALK = 669,
    MP_STAGGERED_WALK_FORWARD = 670,
    MP_STAGGERED_WALK_BACKWARD = 671,
    MP_STAGGERED_WALK_LEFT = 672,
    MP_STAGGERED_WALK_RIGHT = 673,
    MP_AIRWALK = 674,
    MP_CROUCHWALK = 675,
    MP_CROUCHWALK_FORWARD = 676,
    MP_CROUCHWALK_BACKWARD = 677,
    MP_CROUCHWALK_LEFT = 678,
    MP_CROUCHWALK_RIGHT = 679,
    MP_STAGGERED_CROUCHWALK = 680,
    MP_STAGGERED_CROUCHWALK_FORWARD = 681,
    MP_STAGGERED_CROUCHWALK_BACKWARD = 682,
    MP_STAGGERED_CROUCHWALK_LEFT = 683,
    MP_STAGGERED_CROUCHWALK_RIGHT = 684,
    MP_SPRINT = 685,
    MP_STAGGERED_SPRINT = 686,
    MP_SLIDE = 687,
    MP_SLIDE_FORWARD = 688,
    MP_SLIDE_BACKWARD = 689,
    MP_SLIDE_LEFT = 690,
    MP_SLIDE_RIGHT = 691,
    MP_JUMP = 692,
    MP_JUMP_START = 693,
    MP_JUMP_FLOAT = 694,
    MP_JUMP_LAND = 695,
    MP_JUMP_LAND_FORWARD = 696,
    MP_JUMP_LAND_BACKWARD = 697,
    MP_JUMP_LAND_LEFT = 698,
    MP_JUMP_LAND_RIGHT = 699,
    MP_JUMP_LAND_LOW = 700,
    MP_JUMP_LAND_LOW_FORWARD = 701,
    MP_JUMP_LAND_LOW_BACKWARD = 702,
    MP_JUMP_LAND_LOW_LEFT = 703,
    MP_JUMP_LAND_LOW_RIGHT = 704,
    MP_JUMP_LAND_MEDIUM = 705,
    MP_JUMP_LAND_MEDIUM_FORWARD = 706,
    MP_JUMP_LAND_MEDIUM_BACKWARD = 707,
    MP_JUMP_LAND_MEDIUM_LEFT = 708,
    MP_JUMP_LAND_MEDIUM_RIGHT = 709,
    MP_JUMP_LAND_HIGH = 710,
    MP_JUMP_LAND_HIGH_FORWARD = 711,
    MP_JUMP_LAND_HIGH_BACKWARD = 712,
    MP_JUMP_LAND_HIGH_LEFT = 713,
    MP_JUMP_LAND_HIGH_RIGHT = 714,
    MP_JUMP_IMPACT_N = 715,
    MP_JUMP_IMPACT_E = 716,
    MP_JUMP_IMPACT_W = 717,
    MP_JUMP_IMPACT_S = 718,
    MP_JUMP_IMPACT_TOP = 719,
    MP_DOUBLEJUMP = 720,
    MP_DEPLOYED = 721,
    MP_VCD = 722,
    MP_DODGE = 723,
    MP_DODGE_FORWARD = 724,
    MP_DODGE_BACKWARD = 725,
    MP_DODGE_LEFT = 726,
    MP_DODGE_RIGHT = 727,
    MP_DODGE_FLOAT = 728,
    MP_DODGE_FLOAT_FORWARD = 729,
    MP_DODGE_FLOAT_BACKWARD = 730,
    MP_DODGE_FLOAT_LEFT = 731,
    MP_DODGE_FLOAT_RIGHT = 732,
    MP_DODGE_LAND = 733,
    MP_DODGE_LAND_FORWARD = 734,
    MP_DODGE_LAND_BACKWARD = 735,
    MP_DODGE_LAND_LEFT = 736,
    MP_DODGE_LAND_RIGHT = 737,
    MP_WALLRUN_UP = 738,
    MP_WALLRUN_LEFT = 739,
    MP_WALLRUN_RIGHT = 740,
    MP_WALLRUN_FALL_BACK = 741,
    MP_WALLRUN_FALL_FRONT = 742,
    MP_WALLRUN_FALL_LEFT = 743,
    MP_WALLRUN_FALL_RIGHT = 744,
    MP_WALLHANG_IDLE = 745,
    MP_WALLHANG_LEFT = 746,
    MP_WALLHANG_RIGHT = 747,
    MP_WALLHANG_UP = 748,
    MP_WALLHANG_FRONT = 749,
    MP_WALLJUMP_LEFT = 750,
    MP_WALLJUMP_RIGHT = 751,
    MP_WALLJUMP_UP = 752,
    MP_CROUCHSLIDE = 753,
    MP_CROUCHSLIDE_END_FORWARD = 754,
    MP_CROUCHSLIDE_END_BACKWARD_LEFT = 755,
    MP_CROUCHSLIDE_END_BACKWARD_RIGHT = 756,
    MP_CROUCHSLIDE_END_LEFT = 757,
    MP_CROUCHSLIDE_END_RIGHT = 758,
    MP_CROUCHSLIDE_AIR = 759,
    MP_GRAPPLE = 760,
    MP_GESTURE_GRAPPLE_FIRE = 761,
    MP_FREEFALL = 762,
    MP_FREEFALL_ANTICIPATE = 763,
    MP_FREEFALL_TRANSITION_TO_ANTICIPATE = 764,
    MP_FREEFALL_TRANSITION_FROM_ANTICIPATE = 765,
    MP_VEHICLE_DRIVER_IDLE = 766,
    MP_TRAVERSAL_WINDOW_LEFT = 767,
    MP_TRAVERSAL_WINDOW_RIGHT = 768,
    MP_MANTLE_BELOW = 769,
    MP_MANTLE_LEVEL = 770,
    MP_MANTLE_ABOVE = 771,
    MP_MANTLE_HIGH = 772,
    MP_MANTLE_CROUCH_BELOW = 773,
    MP_MANTLE_CROUCH_LEVEL = 774,
    MP_MANTLE_CROUCH_ABOVE = 775,
    MP_MANTLE_CROUCH_HIGH = 776,
    MP_MANTLE_IDLE = 777,
    MP_MANTLE_JUMPOFF = 778,
}

impl Activity {
    pub fn name(self) -> Option<&'static str> {
        ACTIVITIES.iter().find(|(a, _)| *a == self).map(|(_, n)| *n)
    }

    pub fn by_name(name: &str) -> Option<Activity> {
        ACTIVITIES.iter().find(|(_, n)| *n == name).map(|(a, _)| *a)
    }

    /// The directional variant: the four after a base activity are forward, backward, left
    /// and right, in that order.
    pub fn toward(self, direction: Direction) -> Activity {
        Activity(self.0 + direction as u16)
    }
}

/// Activity modifiers by number. A sequence lists the modifiers it suits; the choice prefers
/// the sequence that matches the most of the player's.
pub const MODIFIERS: [&str; 35] = [
    "ads", "crouch", "movingforward", "movingbackward", "movingleft", "movingright", "stagger", "crawl", "lean",
    "doomed", "onehanded", "dualwield", "althand", "unarmed", "pistol", "rocket", "satchel", "zipline",
    "zipline_vertical", "sprintattack", "wallrun", "wallhang", "sprinting", "rodeo", "air", "airfast", "sliding",
    "groundexecute", "melee_as_primary", "enraged", "grapple", "ordnance", "airsprint", "onehand_was_sprinting", "dive",
];

pub const MOD_CROUCH: usize = 1;
pub const MOD_MOVING: [usize; 4] = [2, 3, 4, 5];
pub const MOD_SPRINTING: usize = 22;
pub const MOD_AIR: usize = 24;
pub const MOD_SLIDING: usize = 26;

/// Which way the player moves relative to where it looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    None = 0,
    Forward = 1,
    Backward = 2,
    Left = 3,
    Right = 4,
}

/// Player animation events: what movement tells the body and the viewmodel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerEvent {
    Jump = 6,
    DoubleJump = 7,
    Dodge = 8,
    WallJump = 9,
    Land = 11,
    LandLow = 12,
    LandMedium = 13,
    LandHigh = 14,
}

impl PlayerEvent {
    pub fn of(cue: &Cue) -> Option<PlayerEvent> {
        Some(match *cue {
            Cue::Jump { wall: false, .. } => PlayerEvent::Jump,
            Cue::Jump { wall: true, .. } => PlayerEvent::WallJump,
            Cue::Land { height: LandHeight::Normal, .. } => PlayerEvent::Land,
            Cue::Land { height: LandHeight::Low, .. } => PlayerEvent::LandLow,
            Cue::Land { height: LandHeight::Medium, .. } => PlayerEvent::LandMedium,
            Cue::Land { height: LandHeight::High, .. } => PlayerEvent::LandHigh,
            _ => return None,
        })
    }
}

/// Sequence events (`AE_*`) movement cares about. Other players' footsteps sound on these;
/// the local player's own come from the step timer.
pub const AE_CL_PLAYSOUND: u16 = 35;
pub const AE_CL_PLAYSOUND_FOR_TYPE: u16 = 36;
pub const AE_SV_PLAYSOUND: u16 = 39;
pub const AE_CL_STOPSOUND: u16 = 40;
pub const AE_WPN_RUMBLE: u16 = 98;
pub const AE_CL_VSCRIPT_CALLBACK: u16 = 120;
pub const AE_FOOTSTEP_L: u16 = 125;
pub const AE_FOOTSTEP_R: u16 = 126;
pub const AE_GROUND_CONTACT_L: u16 = 127;
pub const AE_GROUND_CONTACT_R: u16 = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoseParam {
    AimPitch,
    AimYaw,
    MoveYaw,
    MoveYawBackward,
    CrouchFraction,
}

impl PoseParam {
    pub fn name(self) -> &'static str {
        match self {
            PoseParam::AimPitch => "aim_pitch",
            PoseParam::AimYaw => "aim_yaw",
            PoseParam::MoveYaw => "move_yaw",
            PoseParam::MoveYawBackward => "move_yaw_backward",
            PoseParam::CrouchFraction => "crouchFraction",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sequence(pub u32);

/// A model's sequences, as the game's model files describe them.
pub trait AnimSet {
    /// The sequence for `activity` whose modifiers best match `modifiers` (by name), given the
    /// one playing.
    fn select(&self, activity: Activity, modifiers: &[&str], current: Option<Sequence>) -> Option<Sequence>;
    fn activity(&self, seq: Sequence) -> Option<Activity>;
    /// Seconds at playback rate 1.
    fn duration(&self, seq: Sequence) -> f32;
    /// Root motion per second at playback rate 1: what the playback rate is matched against.
    fn ground_speed(&self, seq: Sequence) -> f32;
    fn looping(&self, seq: Sequence) -> bool;
    fn pose_range(&self, param: PoseParam) -> Option<(f32, f32)>;
    /// Events by cycle, 0 to 1.
    fn events(&self, seq: Sequence) -> &[(f32, u16)];
}

/// No model loaded: no sequences, so activities are chosen but nothing plays.
pub struct NoAnims;

impl AnimSet for NoAnims {
    fn select(&self, _: Activity, _: &[&str], _: Option<Sequence>) -> Option<Sequence> {
        None
    }
    fn activity(&self, _: Sequence) -> Option<Activity> {
        None
    }
    fn duration(&self, _: Sequence) -> f32 {
        0.0
    }
    fn ground_speed(&self, _: Sequence) -> f32 {
        0.0
    }
    fn looping(&self, _: Sequence) -> bool {
        false
    }
    fn pose_range(&self, _: PoseParam) -> Option<(f32, f32)> {
        None
    }
    fn events(&self, _: Sequence) -> &[(f32, u16)] {
        &[]
    }
}

/// One playing sequence. Advancing it is Source's frame advance, not recovered: cycle by
/// rate over duration, wrapping when the sequence loops and holding at the end otherwise.
#[derive(Clone, Debug, Default)]
pub struct Playback {
    pub seq: Option<Sequence>,
    pub cycle: f32,
    pub rate: f32,
    pub finished: bool,
}

impl Playback {
    pub fn play(&mut self, seq: Option<Sequence>) {
        if seq != self.seq {
            self.seq = seq;
            self.restart();
        }
    }

    pub fn restart(&mut self) {
        self.cycle = 0.0;
        self.finished = false;
    }

    /// Moves the cycle on by `dt` seconds and returns the events crossed.
    pub fn advance(&mut self, set: &dyn AnimSet, dt: f32) -> Vec<u16> {
        let Some(seq) = self.seq else { return Vec::new() };
        let duration = set.duration(seq);
        if duration <= 0.0 || self.finished {
            return Vec::new();
        }
        let from = self.cycle;
        let mut to = from + dt * self.rate / duration;
        let looping = set.looping(seq);
        let crossed = |c: f32| if to < 1.0 || !looping { from < c && c <= to } else { from < c || c <= to - 1.0 };
        let events = set.events(seq).iter().filter(|(c, _)| crossed(*c)).map(|(_, e)| *e).collect();
        if 1.0 <= to {
            if looping {
                to -= to.floor();
            } else {
                to = 1.0;
                self.finished = true;
            }
        }
        self.cycle = to;
        events
    }
}

/// The game's AngleNormalize: into (-180, 180].
pub fn angle_normalize(a: f32) -> f32 {
    let a = a % 360.0;
    if 180.0 < a {
        a + -360.0
    } else if a < -180.0 {
        a + 360.0
    } else {
        a
    }
}

/// The game's AngleDiff: `dest - src` the short way round.
pub fn angle_diff(dest: f32, src: f32) -> f32 {
    let d = (dest - src) % 360.0;
    if src < dest {
        if 180.0 <= d { d + -360.0 } else { d }
    } else if d <= -180.0 {
        d + 360.0
    } else {
        d
    }
}
