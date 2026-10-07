//! The oracle build's memory layout (5dba2a2f-02ae8471), and replaying its recorded calls
//! through the simulation. Offsets are data recovered in docs/oracle.md; nothing here moves
//! the player.

use crate::ltr::{f32_at, u32_at, vec3_at, Record};
use crate::sim::math::{len2, sub, V3};
use crate::sim::climb::Climb;
use crate::sim::jump::{Lurch, Skip};
use crate::sim::view::{Kick, Spring, ViewTuning};
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
const VIEW_CONVARS: ViewTuning = ViewTuning {
    sprint_tilt_accel: 35.0,
    sprint_tilt_max_vel: 2.0,
    sprint_tilt_turn_range: 120.0,
    punch_spring: Spring { constant: [65.0; 3], damping: [9.0; 3] },
    slide_tilt_side: 15.0,
    slide_tilt_player_speed: 400.0,
    slide_tilt_increase: 5.0,
    slide_tilt_decrease: 2.5,
};

/// The wallrun_*, wallclimb_* and automantle_* ConVars the climb reads.
const WALL_CONVARS: Climb = Climb {
    enabled: false,
    height: 0.0,
    final_jump_up_height: 0.0,
    speed_start: 0.0,
    speed_end: 0.0,
    accel: 0.0,
    decel: 0.0,
    horz_speed: 0.0,
    horz_accel: 0.0,
    horz_decel: 0.0,
    vertical_gain_cutoff: 0.0,
    allowed_wall_distance: 0.0,
    ceiling_limit: 0.0,
    gravity_ramp_up_time: 0.0,
    duck_causes_fall_off: false,
    same_wall_allowed: false,
    same_wall_height: 0.0,
    no_gravity_ramp: false,
    mantle_angle_scale: 0.0,
    jump_outward_speed: 0.0,
    wall_max_speed_vertical: 0.0,
    wall_accel_vertical: 0.0,
    wall_decel_vertical: 0.0,
    wall_max_speed_horizontal: 0.0,
    wall_max_speed_horizontal_back: 0.0,
    wall_accel_horizontal: 0.0,
    wall_decel_horizontal: 0.0,
    retry_interval: 0.07,
    same_wall_dot: 0.9,
    same_wall_dist: 100.0,
    same_wall_slope: 0.0,
    min_angle_air: 180.0,
    rotate_max_rate: 3.0,
    upward_auto_push: 0.65,
    slip_start_time: 1.5,
    slip_duration: 1.0,
    no_input_slip_frac: 0.7,
    strength_loss_start: 1000.0,
    strength_loss_end: 1000.0,
    fall_away_speed: 70.0,
    push_away_fall_off_time: 0.05,
    avoid_wall_top_decel: 3000.0,
    vertical_gain_reduction: 128.0,
    angle_change_min_cos: 0.8,
    mantle_max_angle_push: 50.0,
    mantle_max_angle_view: 50.0,
    mantle_wallrun_max_angle_view: 45.0,
};

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
        climb: Climb {
            enabled: byte(blob, 0xe0a) != 0,
            height: f32_at(blob, 0x97c),
            final_jump_up_height: f32_at(blob, 0x980),
            speed_start: f32_at(blob, 0x984),
            speed_end: f32_at(blob, 0x988),
            accel: f32_at(blob, 0x98c),
            decel: f32_at(blob, 0x990),
            horz_speed: f32_at(blob, 0x994),
            horz_accel: f32_at(blob, 0x998),
            horz_decel: f32_at(blob, 0x99c),
            vertical_gain_cutoff: f32_at(blob, 0x81c),
            allowed_wall_distance: f32_at(blob, 0x9b0),
            ceiling_limit: f32_at(blob, 0x9ac),
            gravity_ramp_up_time: f32_at(blob, 0x9d8),
            duck_causes_fall_off: byte(blob, 0xe0d) != 0,
            same_wall_allowed: byte(blob, 0xe10) != 0,
            same_wall_height: f32_at(blob, 0x9e8),
            no_gravity_ramp: byte(blob, 0xe0c) != 0,
            mantle_angle_scale: f32_at(blob, 0x880),
            jump_outward_speed: f32_at(blob, 0x9e4),
            wall_max_speed_vertical: f32_at(blob, 0x9bc),
            wall_accel_vertical: f32_at(blob, 0x9c8),
            wall_decel_vertical: f32_at(blob, 0x9cc),
            wall_max_speed_horizontal: f32_at(blob, 0x9c0),
            wall_max_speed_horizontal_back: f32_at(blob, 0x9c4),
            wall_accel_horizontal: f32_at(blob, 0x9d0),
            wall_decel_horizontal: f32_at(blob, 0x9d4),
            ..WALL_CONVARS
        },
        sv_gravity: SV_GRAVITY,
        sv_maxvelocity: SV_MAXVELOCITY,
        extra_air_accel: EXTRA_AIR_ACCEL,
        view: VIEW_CONVARS,
        viewkick_fall_dist_min: f32_at(blob, 0x894),
        viewkick_fall_dist_max: f32_at(blob, 0x898),
        viewkick_fall_dist_max_scale: f32_at(blob, 0x89c),
        sprint_tilt_max_roll: f32_at(blob, 0xd80),
        slide_fov_scale: f32_at(blob, 0xa7c),
        slide_fov_lerp_in: f32_at(blob, 0xa80),
        slide_fov_lerp_out: f32_at(blob, 0xa84),
        wall_start_kick: Kick { base: [f32_at(blob, 0x9ec), f32_at(blob, 0x9f4), f32_at(blob, 0x9fc)], random: [f32_at(blob, 0x9f0), f32_at(blob, 0x9f8), f32_at(blob, 0xa00)] },
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
        traversal_blocked: byte(p, 0x2c7c) != 0,
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
        view_angles: vec3_at(p, 0x2188),
        precise_view: byte(p, 0x3a92) != 0 && byte(p, 0x3a93) != 0,
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
        abs_origin: vec3_at(p, 0x14c),
        floor_height: f32_at(p, 0x1d00),
        floor_frozen: u32_at(p, 0x19d0) == 1,
        prev_up: vec3_at(p, 0x2678),
        wall_start: vec3_at(p, 0x2684),
        wall_count: u32_at(p, 0x2690) as i32,
        same_wall: byte(p, 0x2694) != 0,
        push_away_since: f32_at(p, 0x26a4),
        wall_time: f32_at(p, 0x307c),
        wall_leave_time: f32_at(p, 0x3080),
        retry_time: f32_at(p, 0x26b0),
        retry_origin: vec3_at(p, 0x26b4),
        retry_normal: vec3_at(p, 0x26c0),
        repel_normal: vec3_at(p, 0x1bb4),
        repel_time: f32_at(p, 0x1bcc),
        repel_count: u32_at(p, 0x1bd0) as i32,
        hanging: byte(p, 0x276d) != 0,
        dodge_lock: byte(p, 0x30ee) != 0,
        dodged: byte(p, 0x3094) != 0,
        wall_decel_scale: f32_at(p, 0x26a8),
        sprint_tilt: f32_at(p, 0x1eb8),
        sprint_tilt_vel: f32_at(p, 0x282c),
        sprint_tilt_yaw: f32_at(p, 0x2828),
        punch_angle: vec3_at(p, 0x20a4),
        punch_vel: vec3_at(p, 0x20b0),
        weapon_punch_angle: vec3_at(p, 0x20bc),
        weapon_punch_vel: vec3_at(p, 0x20c8),
        fall_velocity: f32_at(p, 0x1a68),
        slide_tilt: vec3_at(p, 0x30fc),
        fov_start: f32_at(p, 0x4050),
        fov_duration: f32_at(p, 0x4054),
        fov_from: f32_at(p, 0x4058),
        fov_to: f32_at(p, 0x405c),
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
        view_forward: vec3_at(mv, 0x3c),
        flat_forward: vec3_at(mv, 0x60),
        flat_right: vec3_at(mv, 0x6c),
        flat_wish: vec3_at(mv, 0x78),
        ground_wish: vec3_at(mv, 0xb4),
        jump_dir: vec3_at(mv, 0xa8),
        maxspeed: f32_at(mv, 0xd4),
        hull_height: f32_at(mv, 0xc8),
        hull_half_width: f32_at(mv, 0xd0),
        random_seed: None,
        origin: vec3_at(mv, 0x118),
        velocity: vec3_at(mv, 0x124),
    }
}

/// Whether the call's move input is this player's: CMoveData's view angles are the
/// command's (`+0x2188`). Some recordings hold calls where the capture read another
/// caller's CMoveData; those cannot be replayed.
pub fn input_recorded(rec: &Record) -> bool {
    let a = vec3_at(&rec.mv_in, 0xc);
    let v = vec3_at(&rec.player_in, 0x2188);
    a[0] == v[0] && a[1] == v[1]
}

/// The move data a call starts from. ProcessMovement copies the player's abs origin and
/// velocity into it on entry; what CMoveData held before then can be another call's.
pub fn move_in(rec: &Record) -> MoveData {
    let mut mv = move_data(&rec.mv_in, rec.seq);
    mv.origin = vec3_at(&rec.player_in, 0x14c);
    mv.velocity = vec3_at(&rec.player_in, 0x140);
    mv
}

/// The clock the view setup runs on after this call: the frame's time (`+0x24`), not the
/// command's, and the frame's frametime.
pub fn view_clock(rec: &Record) -> (f32, f32) {
    (f32_at(&rec.globals, 0x24), f32_at(&rec.globals, 0x30))
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
        contents: u32_at(result, 0x34),
        entity: u64::from_le_bytes(result[0x60..0x68].try_into().unwrap()),
    }
}

/// Answers the simulation's hull traces from the oracle's log. Each query must match a logged
/// ray (centre, sweep and extents) at or after the last one used; oracle traces the simulation
/// never asked for are counted, not fatal. A logged trace the capture lost (all zero) answers
/// a query nothing else matches with a miss; the call's outcome still has to agree.
pub struct LogTracer<'a> {
    log: &'a [crate::ltr::Trace],
    next: usize,
    pub skipped: usize,
    pub lost: usize,
    pub missing: Vec<HullQuery>,
}

impl<'a> LogTracer<'a> {
    pub fn new(log: &'a [crate::ltr::Trace]) -> Self {
        LogTracer { log, next: 0, skipped: 0, lost: 0, missing: Vec::new() }
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
        let miss = Trace { start: q.start, endpos: q.end, fraction: 1.0, ..Default::default() };
        if let Some(i) = (self.next..self.log.len()).find(|&i| self.log[i].slot == 0 && self.log[i].ray.iter().all(|&b| b == 0)) {
            self.skipped += i - self.next;
            self.next = i + 1;
            self.lost += 1;
            return miss;
        }
        self.missing.push(*q);
        miss
    }
}

/// Every modelled player field but `ground`, in one list for diffing, carrying and adopting.
macro_rules! modelled_fields {
    ($m:ident) => {
        $m!(flags);
        $m!(movetype);
        $m!(up);
        $m!(surface_friction);
        $m!(sprinting);
        $m!(sprint_intent);
        $m!(sprint_until);
        $m!(sprint_started_fresh);
        $m!(traversal_state);
        $m!(duck_timer);
        $m!(ducked_in_air);
        $m!(jump_suppressed);
        $m!(duck_held);
        $m!(duck_state);
        $m!(left_ground_time);
        $m!(left_ground_up);
        $m!(jumped_since_ground);
        $m!(jumped_recently);
        $m!(last_jump_time);
        $m!(slide_boost_pending);
        $m!(slide_start);
        $m!(slide_boost);
        $m!(stamina);
        $m!(sliding);
        $m!(push);
        $m!(last_plane);
        $m!(last_ground_time);
        $m!(land_time);
        $m!(land_height);
        $m!(ground_normal);
        $m!(ground_plane);
        $m!(ledge_origin);
        $m!(step_height);
        $m!(prev_velocity);
        $m!(floor_height);
        $m!(prev_up);
        $m!(wall_start);
        $m!(wall_count);
        $m!(same_wall);
        $m!(push_away_since);
        $m!(wall_time);
        $m!(wall_leave_time);
        $m!(retry_time);
        $m!(retry_origin);
        $m!(retry_normal);
        $m!(repel_normal);
        $m!(repel_time);
        $m!(repel_count);
        $m!(hanging);
        $m!(dodge_lock);
        $m!(dodged);
        $m!(sprint_tilt);
        $m!(sprint_tilt_vel);
        $m!(sprint_tilt_yaw);
        $m!(punch_angle);
        $m!(punch_vel);
        $m!(weapon_punch_angle);
        $m!(weapon_punch_vel);
        $m!(fall_velocity);
        $m!(slide_tilt);
        $m!(fov_start);
        $m!(fov_duration);
        $m!(fov_from);
        $m!(fov_to);
    };
}

fn unsign_zeros(s: String) -> String {
    let s = s.replace("-0.0,", "0.0,").replace("-0.0]", "0.0]");
    if s == "-0.0" { "0.0".to_string() } else { s }
}

/// Every modelled player field that differs between the simulation and the oracle's result.
pub fn diff_player(ours: &Player, theirs: &Player) -> Vec<String> {
    diff_player_as(ours, theirs, |s| s)
}

/// The same, blind to the sign of zero: what changes between calls when the game restores
/// networked state.
pub fn diff_player_values(ours: &Player, theirs: &Player) -> Vec<String> {
    diff_player_as(ours, theirs, unsign_zeros)
}

fn diff_player_as(ours: &Player, theirs: &Player, norm: fn(String) -> String) -> Vec<String> {
    let mut out = Vec::new();
    macro_rules! field {
        ($f:ident) => {
            let (a, b) = (norm(format!("{:?}", ours.$f)), norm(format!("{:?}", theirs.$f)));
            if a != b {
                out.push(format!("{} {} want {}", stringify!($f), a, b));
            }
        };
    }
    modelled_fields!(field);
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
    macro_rules! take {
        ($f:ident) => {
            base.$f = state.$f;
        };
    }
    modelled_fields!(take);
    base.ground = state.ground;
    base
}

/// Takes the named fields from `theirs`: what the game changed between calls.
pub fn adopt(ours: &mut Player, theirs: &Player, names: &[&str]) {
    macro_rules! one {
        ($f:ident) => {
            if names.contains(&stringify!($f)) {
                ours.$f = theirs.$f;
            }
        };
    }
    modelled_fields!(one);
}
