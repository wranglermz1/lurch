//! What a movement call asks the client to play. The oracle calls its sound, animation and
//! rumble code from inside movement; each cue is raised at the point where that call is made,
//! and the layers in `crate::sound` and `crate::anim` turn it into something heard or seen.

use super::math::V3;

/// The ground probe the oracle queues for a footstep or a landing: a box of
/// `PROBE_HALF_WIDTH` swept from 0.9 hull heights above the origin to 200 units below it. Its
/// surface names the sound. The answer arrives one call later, which is when the sound plays.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Probe {
    pub start: V3,
    pub end: V3,
}

pub const PROBE_HALF_WIDTH: f32 = 12.0;
pub const PROBE_DEPTH: f32 = 200.0;

impl Probe {
    pub fn below(origin: V3, up: V3, hull_height: f32) -> Probe {
        let (o, h) = (origin, hull_height);
        Probe {
            start: [up[0] * 0.9 * h + o[0], up[1] * 0.9 * h + o[1], up[2] * 0.9 * h + o[2]],
            end: [o[0] - up[0] * PROBE_DEPTH, o[1] - up[1] * PROBE_DEPTH, o[2] - up[2] * PROBE_DEPTH],
        }
    }
}

/// Where a footstep's surface comes from: a probe, or the last surface stood on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StepSurface {
    Probe(Probe),
    Ground(Option<u32>),
}

/// The middle part of a footstep's sound name: `<footstep_type>.<gait>.<surface>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gait {
    Jog,
    Sprint,
    Crouch,
    Wallrun,
    Down,
}

impl Gait {
    pub fn name(self) -> &'static str {
        match self {
            Gait::Jog => "jog",
            Gait::Sprint => "sprint",
            Gait::Crouch => "crouch",
            Gait::Wallrun => "wallrun",
            Gait::Down => "down",
        }
    }
}

/// How far the player fell, by the settings `landDistLow/Medium/High` (feet).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandHeight {
    Normal,
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cue {
    /// A footstep sounds. Raised when the step's probe resolves, one call after the step.
    Footstep { gait: Gait, surface: StepSurface, side: bool },
    /// The jump left the ground. `wall` when the up vector was not the world's.
    Jump { wall: bool, skip: bool, surface: Option<u32> },
    /// Landed at `speed` units/s. Its sound follows a call later, as `LandSound`.
    Land { speed: f32, height: LandHeight },
    LandSound { heavy: bool, probe: Probe },
    Crouch,
    Stand,
    /// A slide started with its speed boost available.
    SlideBoost,
}
