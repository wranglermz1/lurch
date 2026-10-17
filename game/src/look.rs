use lurch::sim::math::V3;

/// Degrees of turn per mouse count at sensitivity 1, the game's literal (docs/oracle.md).
pub const DEG_PER_COUNT: f32 = 0.022;
pub const PITCH_LIMIT: f32 = 89.0;

/// The view angles mouse motion accumulates into, as the engine keeps them between commands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub yaw: f32,
    pub pitch: f32,
    pub sens: f32,
    /// `m_invert_pitch`.
    pub invert: bool,
}

impl Look {
    pub fn new(sens: f32) -> Self {
        Look { yaw: 0.0, pitch: 0.0, sens, invert: false }
    }

    /// `dx`, `dy` are raw counts: right and down are positive.
    pub fn apply(&mut self, dx: f32, dy: f32) {
        self.yaw = wrap(self.yaw - dx * self.sens * DEG_PER_COUNT);
        let k = if self.invert { -DEG_PER_COUNT } else { DEG_PER_COUNT };
        self.pitch = (self.pitch + dy * self.sens * k).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    pub fn angles(&self) -> V3 {
        [self.pitch, self.yaw, 0.0]
    }

    pub fn reset(&mut self, yaw: f32) {
        self.yaw = wrap(yaw);
        self.pitch = 0.0;
    }
}

/// Into (-180, 180].
pub fn wrap(yaw: f32) -> f32 {
    let y = yaw % 360.0;
    if y > 180.0 {
        y - 360.0
    } else if y <= -180.0 {
        y + 360.0
    } else {
        y
    }
}
