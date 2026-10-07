//! The oracle build's memory layout (5dba2a2f-02ae8471), and replaying its recorded calls
//! through the simulation. Offsets are data recovered in docs/oracle.md; nothing here moves
//! the player.

use crate::ltr::{f32_at, u32_at, vec3_at, Record};
use crate::sim::math::{len2, sub, V3};
use crate::sim::{Globals, HullQuery, MoveData, Player, Settings, Stance, Trace, Tracer};

const STANCE_BLOCK: usize = 0x78;
const STANCE_STRIDE: usize = 0x68;
const SPRINT_SETTING: usize = 0xe21;

/// ConVar values read from the dump; the capture does not snapshot ConVars.
const SV_GRAVITY: f32 = 750.0;
const SV_MAXVELOCITY: f32 = 34000.0;
const EXTRA_AIR_ACCEL: f32 = 2.0;
const SKIP_TIME: f32 = 1.0;
pub const SV_OPTIMIZEDMOVEMENT: bool = true;

fn byte(b: &[u8], off: usize) -> u8 {
    b[off]
}

pub fn settings(blob: &[u8]) -> Settings {
    let stance = |i: usize| {
        let o = STANCE_BLOCK + STANCE_STRIDE * i;
        Stance {
            hull_min: vec3_at(blob, o),
            hull_max: vec3_at(blob, o + 0xc),
            low_speed: f32_at(blob, o + 0x44),
            speed: f32_at(blob, o + 0x48),
            sprint_speed: f32_at(blob, o + 0x4c),
            acceleration: f32_at(blob, o + 0x50),
            deceleration: f32_at(blob, o + 0x54),
            low_acceleration: f32_at(blob, o + 0x58),
            sprint_acceleration: f32_at(blob, o + 0x5c),
            sprint_deceleration: f32_at(blob, o + 0x60),
        }
    };
    Settings {
        stances: [stance(0), stance(1), stance(2)],
        speed_scale_side: f32_at(blob, 0x7b8),
        speed_scale_back: f32_at(blob, 0x7bc),
        gravity_scale: f32_at(blob, 0xdd8),
        air_drag: f32_at(blob, 0x7e8),
        air_drag_constant: f32_at(blob, 0x7e4),
        air_strafe_disabled: byte(blob, 0xe07) != 0,
        boost_enabled: byte(blob, 0xdf8) != 0,
        land_slowdown_duration: f32_at(blob, 0x868),
        land_slowdown_frac: f32_at(blob, 0x864),
        land_slowdown_time_power: f32_at(blob, 0x86c),
        land_slowdown_height_min: f32_at(blob, 0x870),
        land_slowdown_height_max: f32_at(blob, 0x874),
        land_slowdown_no_sprint_frac: f32_at(blob, 0x878),
        sprint: byte(blob, SPRINT_SETTING) != 0,
        wallrun: byte(blob, 0xe0b) != 0,
        sv_gravity: SV_GRAVITY,
        sv_maxvelocity: SV_MAXVELOCITY,
        extra_air_accel: EXTRA_AIR_ACCEL,
        skip_time: SKIP_TIME,
    }
}

pub fn ground_of(p: &[u8]) -> Option<u64> {
    (u32_at(p, 0x3dc) != u32::MAX).then_some(1)
}

pub fn player(p: &[u8]) -> Player {
    Player {
        flags: u32_at(p, 0x98),
        ground: ground_of(p),
        movetype: byte(p, 0x452),
        up: vec3_at(p, 0x266c),
        stance_gate: u32_at(p, 0x3e0) as i32,
        collision_group: byte(p, 0x484),
        mask: 0,
        hull_min: vec3_at(p, 0x2638),
        hull_max: vec3_at(p, 0x2644),
        hull_duck_min: vec3_at(p, 0x2650),
        hull_duck_max: vec3_at(p, 0x265c),
        step_height: f32_at(p, 0x1a6c),
        can_step: byte(p, 0x1a82) != 0,
        speed_scale: f32_at(p, 0x2cb8),
        accel_scale: f32_at(p, 0x1a84),
        surface_friction: f32_at(p, 0x3a60),
        friction_scale: f32_at(p, 0x26ac),
        lagged: f32_at(p, 0x358c),
        gravity: f32_at(p, 0x3c8),
        sprinting: byte(p, 0x2604) != 0,
        sprint_intent: byte(p, 0x2824) != 0,
        sprint_until: f32_at(p, 0x25f8),
        sprint_started_fresh: byte(p, 0x25fc) != 0,
        sprint_toggle_lock: byte(p, 0x1881) != 0,
        sprint_blocked: byte(p, 0x2605) != 0 || u32_at(p, 0x1764) != 0 || byte(p, 0x2c7c) != 0,
        sprint_suppressed: false,
        sprint_lock: u32_at(p, 0x262c),
        stamina: f32_at(p, 0x1a5c),
        prev_velocity: vec3_at(p, 0x41c),
        eye_angles: vec3_at(p, 0x1ebc),
        sliding: byte(p, 0x2815) != 0,
        no_accel: byte(p, 0x2184) != 0 || byte(p, 0x2a7c) != 0,
        air_speed: f32_at(p, 0x3108),
        air_acceleration: f32_at(p, 0x310c),
        ramp_end: f32_at(p, 0x261c),
        push: vec3_at(p, 0x4024),
        last_plane: vec3_at(p, 0x3088),
        last_ground_time: f32_at(p, 0x3084),
        land_time: f32_at(p, 0x30c4),
        land_height: f32_at(p, 0x30c8),
        ground_normal: vec3_at(p, 0x1d04),
        ground_plane: vec3_at(p, 0x3e60),
        ledge_origin: vec3_at(p, 0x30e0),
        move_trace: Default::default(),
    }
}

pub fn move_data(mv: &[u8], command_number: u32) -> MoveData {
    MoveData {
        origin_changed: byte(mv, 0) & 2 != 0,
        command_number,
        angles: vec3_at(mv, 0xc),
        buttons: u32_at(mv, 0x24),
        oldbuttons: u32_at(mv, 0x28),
        forward: f32_at(mv, 0x30),
        side: f32_at(mv, 0x34),
        up: f32_at(mv, 0x38),
        flat_wish: vec3_at(mv, 0x78),
        ground_plane: vec3_at(mv, 0xa8),
        maxspeed: f32_at(mv, 0xd4),
        origin: vec3_at(mv, 0x118),
        velocity: vec3_at(mv, 0x124),
    }
}

pub fn globals(g: &[u8], lagged: f32) -> Globals {
    Globals { curtime: f32_at(g, 0x10), frametime: f32_at(g, 0x30) * lagged }
}

/// What the oracle wrote that the simulation is checked against.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub origin: V3,
    pub velocity: V3,
    pub on_ground: bool,
    pub surface_friction: f32,
}

pub fn outcome(rec: &Record) -> Outcome {
    Outcome {
        origin: vec3_at(&rec.mv_out, 0x118),
        velocity: vec3_at(&rec.mv_out, 0x124),
        on_ground: ground_of(&rec.player_out).is_some(),
        surface_friction: f32_at(&rec.player_out, 0x3a60),
    }
}

pub fn decode(result: &[u8]) -> Trace {
    Trace {
        start: vec3_at(result, 0),
        endpos: vec3_at(result, 0x10),
        normal: vec3_at(result, 0x20),
        fraction: f32_at(result, 0x30),
        allsolid: result[0x38] != 0,
        startsolid: result[0x39] != 0,
        flag3b: result[0x3b] != 0,
        surface: result[0x48] as u32,
        entity: u64::from_le_bytes(result[0x60..0x68].try_into().unwrap()),
    }
}

/// Answers the simulation's hull traces from the oracle's log. Each query must match a logged
/// ray (centre, sweep and extents) at or after the last one used; oracle traces the simulation
/// never asked for are counted, not fatal.
pub struct LogTracer<'a> {
    log: &'a [crate::ltr::Trace],
    next: usize,
    pub skipped: usize,
    pub missing: Vec<HullQuery>,
}

impl<'a> LogTracer<'a> {
    pub fn new(log: &'a [crate::ltr::Trace]) -> Self {
        LogTracer { log, next: 0, skipped: 0, missing: Vec::new() }
    }

    pub fn unused(&self) -> usize {
        self.skipped + self.log.len() - self.next
    }
}

fn close(a: V3, b: V3) -> bool {
    len2(sub(a, b)) < 1e-6
}

impl Tracer for LogTracer<'_> {
    fn hull(&mut self, q: &HullQuery) -> Trace {
        let half = |i: usize| (q.maxs[i] - q.mins[i]) * 0.5;
        let mid = |i: usize| (q.mins[i] + q.maxs[i]) * 0.5;
        let centre = [q.start[0] + mid(0), q.start[1] + mid(1), q.start[2] + mid(2)];
        let delta = sub(q.end, q.start);
        let extents = [half(0), half(1), half(2)];
        for i in self.next..self.log.len() {
            let r = &self.log[i];
            if r.slot == 0x40 {
                continue;
            }
            if close(vec3_at(&r.ray, 0), centre) && close(vec3_at(&r.ray, 0x10), delta) && close(vec3_at(&r.ray, 0x30), extents) {
                self.skipped += i - self.next;
                self.next = i + 1;
                return decode(&r.result);
            }
        }
        self.missing.push(*q);
        Trace { start: q.start, endpos: q.end, fraction: 1.0, ..Default::default() }
    }
}
