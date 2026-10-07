//! The oracle build's memory layout (5dba2a2f-02ae8471), and replaying its recorded calls
//! through the simulation. Offsets are data recovered in docs/oracle.md; nothing here moves
//! the player.

use crate::ltr::{f32_at, u32_at, vec3_at, Record};
use crate::sim::math::{len2, sub, V3};
use crate::sim::jump::{Lurch, Skip};
use crate::sim::{Globals, HullQuery, MoveData, Player, Settings, Stance, Trace, Tracer};

const STANCE_BLOCK: usize = 0x78;
const STANCE_STRIDE: usize = 0x68;
const SPRINT_SETTING: usize = 0xe21;

/// ConVar values read from the dump; the capture does not snapshot ConVars.
const SV_GRAVITY: f32 = 750.0;
const SV_MAXVELOCITY: f32 = 34000.0;
const EXTRA_AIR_ACCEL: f32 = 2.0;
const SKIP_TIME: f32 = 1.0;
const JUMP_GRACEPERIOD: f32 = 0.2;
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
        jump_enabled: byte(blob, 0xdf2) != 0,
        crouch: byte(blob, 0xde6) != 0,
        slide: byte(blob, 0xe12) != 0,
        slide_while_in_air: false,
        slide_auto_stand: false,
        slide_max_angle_dot: 0.6,
        slide_required_start_speed: f32_at(blob, 0xa5c),
        slide_required_start_speed_air: f32_at(blob, 0xa60),
        slide_boost_enabled: byte(blob, 0xe13) != 0,
        slide_boost_cooldown: f32_at(blob, 0xa48),
        slide_speed_boost: f32_at(blob, 0xa70),
        slide_speed_boost_cap: f32_at(blob, 0xa74),
        slide_max_stop_speed: f32_at(blob, 0xa68),
        slide_stop_speed: f32_at(blob, 0xa64),
        slide_accel: f32_at(blob, 0xa58),
        slide_decel: f32_at(blob, 0xa4c),
        slide_want_to_stop_decel: f32_at(blob, 0xa50),
        slide_velocity_decay: f32_at(blob, 0xa54),
        slide_step_velocity_reduction: 10.0,
        jump_height: f32_at(blob, 0x844),
        slide_jump_height: f32_at(blob, 0xa78),
        slide_max_jump_speed: f32_at(blob, 0xa6c),
        can_jump_crouched: byte(blob, 0xde8) != 0,
        anti_multi_jump_time_min: f32_at(blob, 0x84c),
        anti_multi_jump_time_max: f32_at(blob, 0x850),
        anti_multi_jump_height_frac: f32_at(blob, 0x848),
        jump_grace_period: JUMP_GRACEPERIOD,
        skip_replenish: true,
        lurch: Lurch { period_min: 0.2, period_max: 0.5, strength: 0.7, max: 0.7 },
        skip: Skip { time: SKIP_TIME, height_speed: 450.0, height_fraction: 1.0, speed_retain: -1.0, speed_reduce: 100.0 },
        wallrun: byte(blob, 0xe0b) != 0,
        sv_gravity: SV_GRAVITY,
        sv_maxvelocity: SV_MAXVELOCITY,
        extra_air_accel: EXTRA_AIR_ACCEL,
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
        traversal_state: u32_at(p, 0x262c),
        frozen: byte(p, 0x2184) != 0,
        dead: byte(p, 0x730) != 0,
        jetpack_active: byte(p, 0x2a70) != 0,
        duck_timer: u32_at(p, 0x1a44) as i32,
        ducked_in_air: byte(p, 0x2634) != 0,
        has_parent: u32_at(p, 0x118) != u32::MAX,
        jump_suppressed: byte(p, 0x1a4c) != 0,
        duck_held: byte(p, 0x1a80) != 0,
        duck_state: u32_at(p, 0x1a7c) as i32,
        left_ground_time: f32_at(p, 0x30bc),
        left_ground_up: vec3_at(p, 0x30d4),
        jumped_since_ground: byte(p, 0x30ec) != 0,
        jumped_recently: byte(p, 0x30ed) != 0,
        last_jump_time: f32_at(p, 0x30c0),
        slide_boost_pending: byte(p, 0x2816) != 0,
        slide_start: f32_at(p, 0x2818),
        slide_boost: f32_at(p, 0x281c),
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
        flags: byte(mv, 0),
        command_number,
        angles: vec3_at(mv, 0xc),
        buttons: u32_at(mv, 0x24),
        oldbuttons: u32_at(mv, 0x28),
        forward: f32_at(mv, 0x30),
        side: f32_at(mv, 0x34),
        up: f32_at(mv, 0x38),
        flat_forward: vec3_at(mv, 0x60),
        flat_right: vec3_at(mv, 0x6c),
        flat_wish: vec3_at(mv, 0x78),
        ground_wish: vec3_at(mv, 0xb4),
        jump_dir: vec3_at(mv, 0xa8),
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

/// Every modelled player field that differs between the simulation and the oracle's result.
pub fn diff_player(ours: &Player, theirs: &Player) -> Vec<String> {
    let mut out = Vec::new();
    macro_rules! field {
        ($f:ident) => {
            let (a, b) = (format!("{:?}", ours.$f), format!("{:?}", theirs.$f));
            if a != b {
                out.push(format!("{} {} want {}", stringify!($f), a, b));
            }
        };
    }
    field!(flags);
    field!(movetype);
    field!(up);
    field!(surface_friction);
    field!(sprinting);
    field!(sprint_intent);
    field!(sprint_until);
    field!(sprint_started_fresh);
    field!(traversal_state);
    field!(duck_timer);
    field!(ducked_in_air);
    field!(jump_suppressed);
    field!(duck_held);
    field!(duck_state);
    field!(left_ground_time);
    field!(left_ground_up);
    field!(jumped_since_ground);
    field!(jumped_recently);
    field!(last_jump_time);
    field!(slide_boost_pending);
    field!(slide_start);
    field!(slide_boost);
    field!(stamina);
    field!(sliding);
    field!(push);
    field!(last_plane);
    field!(last_ground_time);
    field!(land_time);
    field!(land_height);
    field!(ground_normal);
    field!(ground_plane);
    field!(ledge_origin);
    field!(step_height);
    if ours.ground.is_some() != theirs.ground.is_some() {
        out.push(format!("ground {} want {}", ours.ground.is_some(), theirs.ground.is_some()));
    }
    out
}

pub fn diff_move(ours: &MoveData, theirs: &MoveData) -> Vec<String> {
    let mut out = Vec::new();
    macro_rules! field {
        ($f:ident) => {
            let (a, b) = (format!("{:?}", ours.$f), format!("{:?}", theirs.$f));
            if a != b {
                out.push(format!("mv.{} {} want {}", stringify!($f), a, b));
            }
        };
    }
    field!(origin);
    field!(velocity);
    field!(oldbuttons);
    field!(maxspeed);
    out
}

/// `base` with every modelled field taken from `state`: the simulation's own state carried
/// into a call whose other inputs come from the oracle.
pub fn with_state(mut base: Player, state: &Player) -> Player {
    base.flags = state.flags.clone();
    base.movetype = state.movetype.clone();
    base.up = state.up.clone();
    base.surface_friction = state.surface_friction.clone();
    base.sprinting = state.sprinting.clone();
    base.sprint_intent = state.sprint_intent.clone();
    base.sprint_until = state.sprint_until.clone();
    base.sprint_started_fresh = state.sprint_started_fresh.clone();
    base.traversal_state = state.traversal_state.clone();
    base.duck_timer = state.duck_timer.clone();
    base.ducked_in_air = state.ducked_in_air.clone();
    base.jump_suppressed = state.jump_suppressed.clone();
    base.duck_held = state.duck_held.clone();
    base.duck_state = state.duck_state.clone();
    base.left_ground_time = state.left_ground_time.clone();
    base.left_ground_up = state.left_ground_up.clone();
    base.jumped_since_ground = state.jumped_since_ground.clone();
    base.jumped_recently = state.jumped_recently.clone();
    base.last_jump_time = state.last_jump_time.clone();
    base.slide_boost_pending = state.slide_boost_pending.clone();
    base.slide_start = state.slide_start.clone();
    base.slide_boost = state.slide_boost.clone();
    base.stamina = state.stamina.clone();
    base.sliding = state.sliding.clone();
    base.push = state.push.clone();
    base.last_plane = state.last_plane.clone();
    base.last_ground_time = state.last_ground_time.clone();
    base.land_time = state.land_time.clone();
    base.land_height = state.land_height.clone();
    base.ground_normal = state.ground_normal.clone();
    base.ground_plane = state.ground_plane.clone();
    base.ledge_origin = state.ledge_origin.clone();
    base.step_height = state.step_height.clone();
    base.ground = state.ground;
    base
}
