//! The oracle build's memory layout (5dba2a2f-02ae8471), and replaying its recorded calls
//! through the simulation. Offsets are data recovered in docs/oracle.md; nothing here moves
//! the player.

use crate::ltr::{f32_at, u32_at, vec3_at, Record};
use crate::sim::math::{len2, sub, V3};
use crate::sim::climb::Climb;
use crate::sim::grapple;
use crate::sim::jump::{Lurch, Skip};
use crate::sim::cue::Probe;
use crate::sim::mantle::Mantle;
use crate::sim::zipline::{self, Zipline};
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

/// The automantle_* and traversal_* ConVars the mantle reads.
pub const MANTLE_CONVARS: Mantle = Mantle {
    enabled: false,
    height: 0.0,
    duration: [0.0; 4],
    jump_up_speed: 0.0,
    jump_outward_speed: 0.0,
    forward_dist: 26.0,
    min_dist: 18.0,
    search_dist: 5.0,
    height_below: -10.0,
    height_level: 10.0,
    height_above: 30.0,
    rest_frac: 0.4,
    rest_frac_below: 0.3,
    jumpoff_duration: 0.1,
    jumpoff_anim_maxfrac: 0.5,
    backoff_anim_maxfrac: 0.7,
    dangle_required_space: 60.0,
    cooldown: 0.25,
    traversal_cooldown: 0.5,
    hand_required_width: 6.0,
    grapple_auto_mantle: 0.25,
    gun_enable_height: 33.0,
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
            view_offset: vec3_at(blob, o + 0x18),
            low_speed: f32_at(blob, o + 0x44),
            speed: f32_at(blob, o + 0x48),
            sprint_speed: f32_at(blob, o + 0x4c),
            acceleration: f32_at(blob, o + 0x50),
            deceleration: f32_at(blob, o + 0x54),
            low_acceleration: f32_at(blob, o + 0x58),
            sprint_acceleration: f32_at(blob, o + 0x5c),
            sprint_deceleration: f32_at(blob, o + 0x60),
            footstep_interval: u32_at(blob, o + 0x30) as i32,
            footstep_interval_sprint: u32_at(blob, o + 0x34) as i32,
            footstep_min_speed: f32_at(blob, o + 0x40),
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
        mantle: Mantle {
            enabled: byte(blob, 0xdf5) != 0,
            height: f32_at(blob, 0x87c),
            duration: [f32_at(blob, 0x884), f32_at(blob, 0x888), f32_at(blob, 0x88c), f32_at(blob, 0x890)],
            jump_up_speed: f32_at(blob, 0x9dc),
            jump_outward_speed: f32_at(blob, 0x9e4),
            ..MANTLE_CONVARS
        },
        zipline: zipline::Tuning {
            mount_time: f32_at(blob, 0xc38),
            use_cooldown: f32_at(blob, 0xc3c),
            speed: f32_at(blob, 0xc40),
            acceleration: f32_at(blob, 0xc44),
            jump_off_speed: f32_at(blob, 0xc4c),
            jump_off_damp: f32_at(blob, 0xc50),
            view_offset: vec3_at(blob, 0xc64),
            roll_strength: f32_at(blob, 0xc54),
            roll_max: f32_at(blob, 0xc58),
            roll_view_angle_min: f32_at(blob, 0xc5c),
            roll_view_angle_max: f32_at(blob, 0xc60),
            context_action_can_use: byte(blob, 0xe1c) != 0,
        },
        grapple: grapple::Tuning {
            swing: byte(blob, 0xe18) != 0,
            swing_type: u32_at(blob, 0xa90) as i32,
            swing_gravity: f32_at(blob, 0xa94),
            swing_hold_time: f32_at(blob, 0xa98),
            swing_sticky: byte(blob, 0xe19) != 0,
            attach_vertical_boost: f32_at(blob, 0xac0),
            detach_vertical_boost: f32_at(blob, 0xac4),
            detach_vertical_max_speed: f32_at(blob, 0xac8),
            detach_gravity_time: f32_at(blob, 0xad0),
            detach_gravity_scale: f32_at(blob, 0xad4),
            detach_gravity_speed: f32_at(blob, 0xad8),
            detach_length_min: f32_at(blob, 0xadc),
            detach_length_max: f32_at(blob, 0xae0),
            detach_away_speed: f32_at(blob, 0xae4),
            gravity_frac_min: f32_at(blob, 0xae8),
            gravity_frac_max: f32_at(blob, 0xaec),
            detach_speed_loss: f32_at(blob, 0xaf0),
            detach_speed_loss_min: f32_at(blob, 0xaf4),
            detach_low_speed_threshold: f32_at(blob, 0xaf8),
            detach_low_speed_time: f32_at(blob, 0xb04),
            detach_low_speed_wall_time: f32_at(blob, 0xb08),
            detach_low_speed_ground_time: f32_at(blob, 0xb0c),
            detach_low_speed_swing_time: f32_at(blob, 0xb14),
            impact_vertical_boost: f32_at(blob, 0xb18),
            impact_vertical_max_speed: f32_at(blob, 0xb1c),
            speed_ramp_min: f32_at(blob, 0xb20),
            speed_ramp_max: f32_at(blob, 0xb24),
            speed_ramp_time: f32_at(blob, 0xb28),
            swinging_speed_ramp_min: f32_at(blob, 0xb2c),
            swinging_speed_ramp_max: f32_at(blob, 0xb30),
            swinging_speed_ramp_time: f32_at(blob, 0xb34),
            accel: f32_at(blob, 0xb38),
            decel: f32_at(blob, 0xb3c),
            air_speed_max: f32_at(blob, 0xb48),
            air_accel: f32_at(blob, 0xb4c),
            detach_on_grapple: byte(blob, 0xe14) != 0,
            detach_on_grapple_debounce_time: f32_at(blob, 0xa88),
            last_zipline_cooldown: f32_at(blob, 0xb50),
            roll_max: f32_at(blob, 0xaac),
            roll_strength: f32_at(blob, 0xaa8),
            roll_distance_min: f32_at(blob, 0xab8),
            roll_distance_max: f32_at(blob, 0xabc),
            roll_view_angle_min: f32_at(blob, 0xab0),
            roll_view_angle_max: f32_at(blob, 0xab4),
            power_regen_delay: f32_at(blob, 0xb40),
            power_regen_rate: f32_at(blob, 0xb44),
            human: (u32_at(blob, 0x7a8).wrapping_sub(1) & !2) != 0,
            weapon: grapple::WEAPON,
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
        sprint_start_fast_duration: f32_at(blob, 0xd78),
        sprint_start_duration: f32_at(blob, 0xd74),
        sprint_start_delay: f32_at(blob, 0xd70),
        sprint_end_duration: f32_at(blob, 0xd7c),
        sprint_view_offset: f32_at(blob, 0xd6c),
        fov: f32_at(blob, 0x7ac),
        pitch_offset_scale: f32_at(blob, 0xdd4),
        wall_start_kick: Kick { base: [f32_at(blob, 0x9ec), f32_at(blob, 0x9f4), f32_at(blob, 0x9fc)], random: [f32_at(blob, 0x9f0), f32_at(blob, 0x9f8), f32_at(blob, 0xa00)] },
        impact_speed: f32_at(blob, 0x7ec),
        hard_fall_dist: f32_at(blob, 0xdb8),
        land_dist_low: f32_at(blob, 0xdbc),
        land_dist_medium: f32_at(blob, 0xdc0),
        land_dist_high: f32_at(blob, 0xdc4),
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
        sprint_blocked: byte(p, 0x2605) != 0,
        sprint_suppressed: false,
        melee_active: byte(p, 0x2c7c) != 0,
        melee_state: u32_at(p, 0x2c90),
        melee_start: f32_at(p, 0x2c80),
        melee_hit: u32_at(p, 0x2c84) != u32::MAX,
        melee_push: vec3_at(p, 0x2c98),
        lunge_target: (u32_at(p, 0x3adc) != u32::MAX).then(|| u32_at(p, 0x3adc) as u64),
        lunge_start_offset: vec3_at(p, 0x3af0),
        lunge_end_offset: vec3_at(p, 0x3afc),
        lunge_start: f32_at(p, 0x3b08),
        lunge_abort: f32_at(p, 0x3b0c),
        lunge_flying: byte(p, 0x3b10) != 0,
        lunge_duration: f32_at(p, 0x3b18),
        lunge_max_time: f32_at(p, 0x3b1c),
        lunge_max_end_speed: f32_at(p, 0x3b20),
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
        climb_arms: byte(p, 0x276c) != 0,
        first_person_anim: u32_at(p, 0x2a4c),
        view_entity: u32_at(p, 0x2860) != u32::MAX,
        view_entity_lerp_in: f32_at(p, 0x2864),
        view_entity_lerp_out: f32_at(p, 0x2868),
        view_entity_flag: byte(p, 0x286c) != 0,
        view_entity_blend: f32_at(p, 0x2870),
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
        sprint_start_time: f32_at(p, 0x2608),
        sprint_start_frac: f32_at(p, 0x260c),
        sprint_end_time: f32_at(p, 0x2610),
        sprint_end_frac: f32_at(p, 0x2614),
        sprint_intent_time: f32_at(p, 0x2618),
        view_offset: vec3_at(p, 0xb8),
        step_smoothing: vec3_at(p, 0x2098),
        grapple_time: f32_at(p, 0x27d0),
        grapple_hook_vel: vec3_at(p, 0x2778),
        grapple_points: [vec3_at(p, 0x2784), vec3_at(p, 0x2790), vec3_at(p, 0x279c), vec3_at(p, 0x27a8)],
        grapple_count: u32_at(p, 0x27b4) as i32,
        grapple_attached: byte(p, 0x27b8) != 0,
        grapple_pulling: byte(p, 0x27b9) != 0,
        grapple_swinging: byte(p, 0x27ba) != 0,
        grapple_retracting: byte(p, 0x27bb) != 0,
        grapple_forced: byte(p, 0x27bc) != 0,
        grapple_touched: byte(p, 0x27bd) != 0,
        grapple_power_used: f32_at(p, 0x27c0),
        grapple_fire_time: f32_at(p, 0x27c4),
        grapple_pull_time: f32_at(p, 0x27c8),
        grapple_attach_time: f32_at(p, 0x27cc),
        grapple_fast: byte(p, 0x27dc) != 0,
        grapple_slow_time: f32_at(p, 0x27e0),
        grapple_swing_length: f32_at(p, 0x27e4),
        grapple_swing_hold: f32_at(p, 0x27e8),
        suit_grapple_power: f32_at(p, 0x1a64),
        grapple_hook_at: [0.0; 3],
        grapple_parent: None,
        grapple_zipline: None,
        mantle_blocked: (u32_at(p, 0x2cac) >> 11) & 1 != 0
            || u64_at(p, 0x2ac8) as i64 > 0
            || matches!(u32_at(p, 0x2a4c), 1 | 3 | 4),
        traversal_kind: u32_at(p, 0x26ec) as i32,
        traversal_type: u32_at(p, 0x26e8) as i32,
        traversal_begin: vec3_at(p, 0x26f0),
        traversal_mid: vec3_at(p, 0x26fc),
        traversal_end: vec3_at(p, 0x2708),
        traversal_mid_frac: f32_at(p, 0x2714),
        traversal_forward: vec3_at(p, 0x2718),
        traversal_ref: vec3_at(p, 0x2724),
        traversal_progress: f32_at(p, 0x2730),
        traversal_start_time: f32_at(p, 0x2734),
        traversal_release_time: f32_at(p, 0x273c),
        traversal_blend_time: f32_at(p, 0x2740),
        traversal_blend_offset: vec3_at(p, 0x2744),
        traversal_jump_time: f32_at(p, 0x275c),
        traversal_hang: byte(p, 0x2760) != 0,
        traversal_backoff: byte(p, 0x2761) != 0,
        traversal_push: byte(p, 0x2762) != 0,
        traversal_rest: f32_at(p, 0x2768),
        traversal_gun: u32_at(p, 0x2764) as i32,
        traversal_cycle: f32_at(p, 0x1eb4),
        context_action: u32_at(p, 0x1764) as i32,
        grappling: byte(p, 0x2800) != 0,
        use_time: f32_at(p, 0x30d0),
        use_retry: byte(p, 0x2ca8) != 0,
        zipline_allowed: byte(p, 0x2994) != 0,
        zipline: handle(p, 0x2998),
        last_zipline: handle(p, 0x299c),
        zipline_detach_time: f32_at(p, 0x29a0),
        zipline_state: u32_at(p, 0x29a8) as i32,
        zipline_mount_time: f32_at(p, 0x29bc),
        zipline_mount_alpha: f32_at(p, 0x29c0),
        zipline_start_time: f32_at(p, 0x29c4),
        zipline_end_time: f32_at(p, 0x29c8),
        zipline_from: vec3_at(p, 0x29cc),
        zipline_from_velocity: vec3_at(p, 0x29d8),
        zipline_target: vec3_at(p, 0x29e4),
        zipline_use_point: vec3_at(p, 0x29fc),
        zipline_param: f32_at(p, 0x2a08),
        zipline_reverse: byte(p, 0x2a18) != 0,
        zipline_view_offset: vec3_at(p, 0x2a20),
        zipline_view_offset_vel: vec3_at(p, 0x2a2c),
        zipline_roll: f32_at(p, 0x3110),
        zipline_roll_vel: f32_at(p, 0x3114),
        surface: (u64_at(p, 0x3a58) != 0).then_some(UNKNOWN_SURFACE),
        step_time: f32_at(p, 0x25f0),
        step_side: u32_at(p, 0x1a38) != 0,
        step_probe: pending(p, 0x1a24),
        land_speed: f32_at(p, 0x25c0),
        land_probe: pending(p, 0x19c8),
        heavy_land_speed: f32_at(p, 0x19cc),
    }
}

fn handle(p: &[u8], off: usize) -> Option<u32> {
    Some(u32_at(p, off)).filter(|&h| h != u32::MAX)
}

/// The oracle keeps a pointer to the surface's data, not its index.
pub const UNKNOWN_SURFACE: u32 = u32::MAX;

fn u64_at(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(b[off..off + 8].try_into().unwrap())
}

/// A queued ground probe, by its job handle. Where it was aimed is not kept on the player, so
/// it is aimed again from where the player is now, with its collision bounds' height.
fn pending(p: &[u8], off: usize) -> Option<Probe> {
    (u32_at(p, off) != u32::MAX).then(|| Probe::below(vec3_at(p, 0x14c), vec3_at(p, 0x266c), f32_at(p, 0x47c) - f32_at(p, 0x470)))
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
        pressed: u32_at(mv, 0x2c),
        view_wish: vec3_at(mv, 0x84),
        random_seed: None,
        grapple_stash: vec3_at(mv, 0xe4),
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
    ziplines: Vec<Zipline>,
}

impl<'a> LogTracer<'a> {
    pub fn new(log: &'a [crate::ltr::Trace]) -> Self {
        let entities = log.iter().take_while(|t| t.slot & ENTITY_SLOT != 0).count();
        LogTracer { log: &log[entities..], next: 0, skipped: 0, lost: 0, missing: Vec::new(), ziplines: ziplines(log) }
    }

    pub fn unused(&self) -> usize {
        self.skipped + self.log.len() - self.next
    }
}

/// The capture snapshots the zipline ridden, the entity its rope runs to, that one's next and
/// the zipline's previous ahead of a call's traces, one trace entry per chunk:
/// slot `ENTITY_SLOT | index << 4 | chunk`.
const ENTITY_SLOT: u32 = 0x1000;
const ENTITY_AT: usize = 0xa00;
const ENTITY_HEADER: usize = 32;

/// The zipline entities a call's log carries (`capture/lurchcap.c`).
pub fn ziplines(log: &[crate::ltr::Trace]) -> Vec<Zipline> {
    let mut blobs: Vec<(u32, Vec<u8>)> = Vec::new();
    for t in log.iter().take_while(|t| t.slot & ENTITY_SLOT != 0) {
        let index = (t.slot >> 4) & 0xff;
        if blobs.last().is_none_or(|b| b.0 != index) {
            blobs.push((index, Vec::new()));
        }
        let b = &mut blobs.last_mut().unwrap().1;
        b.extend_from_slice(&t.ray);
        b.extend_from_slice(&t.result);
    }
    blobs.iter().filter_map(|(_, b)| entity(b)).collect()
}

fn entity(b: &[u8]) -> Option<Zipline> {
    let id = u32_at(b, 0);
    let kind = u32_at(b, 4);
    let (origin, angles) = (vec3_at(b, 8), vec3_at(b, 20));
    let at = |off: usize| ENTITY_HEADER + off - ENTITY_AT;
    match kind {
        1 => {
            let points = |count: usize, first: usize| (0..u32_at(b, at(count)).min(16) as usize).map(|i| vec3_at(b, at(first) + i * 12)).collect::<Vec<V3>>();
            let pts = points(0xe40, 0xe44);
            let spans = (0..pts.len().saturating_sub(1)).map(|i| f32_at(b, at(0xfc4) + i * 4)).collect();
            Some(Zipline {
                id,
                origin,
                angles,
                rope: true,
                next: handle(b, at(0xd50)),
                prev: handle(b, at(0xd4c)),
                points: pts,
                spans,
                rest: points(0xe28, 0xd68),
                kind: u32_at(b, at(0xa0c)) as i32,
                enabled: byte(b, at(0xd64)) != 0,
                usable: true,
                detach_end_on_use: byte(b, at(0xd54)) != 0,
                drop_to_bottom: byte(b, at(0xd55)) != 0,
                auto_detach_distance: f32_at(b, at(0xd58)),
                push_off: byte(b, at(0xd5c)) != 0,
                preserve_velocity: byte(b, at(0xd5d)) != 0,
                speed_scale: f32_at(b, at(0xe30)),
            })
        }
        2 => {
            let mut z = Zipline::end(id, origin, handle(b, at(0xa08))?, f32_at(b, at(0xa00)));
            z.angles = angles;
            z.push_off = byte(b, at(0xa04)) != 0;
            Some(z)
        }
        _ => None,
    }
}

fn close(a: V3, b: V3) -> bool {
    len2(sub(a, b)) < 1e-6
}

impl Tracer for LogTracer<'_> {
    fn ziplines(&self) -> &[Zipline] {
        &self.ziplines
    }

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

/// What the oracle took from animations on this call: the traversal animation's root as it
/// placed it (its origin less the share of the blend-out offset still applied), and the
/// zipline mount's length when this call started one.
pub struct RecordedRoot(Option<V3>, Option<f32>);

impl RecordedRoot {
    pub fn of(rec: &Record) -> Self {
        let q = &rec.player_out;
        let mount = (u32_at(&rec.player_in, 0x29a8) as i32 == zipline::REQUESTED).then(|| f32_at(q, 0x29bc));
        if byte(q, 0x452) != crate::sim::mantle::MOVETYPE_TRAVERSE {
            return RecordedRoot(None, mount);
        }
        let o = vec3_at(&rec.mv_out, 0x118);
        let off = vec3_at(q, 0x2744);
        let k = (f32_at(&rec.globals, 0x10) - f32_at(q, 0x2740)) / 0.2;
        let k = if 0.0 >= k { 0.0 } else if 1.0 <= k { 1.0 } else { k };
        let left = 1.0 - k;
        RecordedRoot(Some([o[0] - left * off[0], o[1] - left * off[1], o[2] - left * off[2]]), mount)
    }

    pub fn at(&self) -> Option<V3> {
        self.0
    }
}

impl crate::sim::mantle::RootMotion for RecordedRoot {
    fn place(&self, _: i32, _: f32, _: V3, _: V3) -> Option<V3> {
        self.0
    }

    fn zipline_mount_time(&self) -> Option<f32> {
        self.1
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
        $m!(climb_arms);
        $m!(first_person_anim);
        $m!(view_entity);
        $m!(view_entity_lerp_in);
        $m!(view_entity_lerp_out);
        $m!(view_entity_flag);
        $m!(view_entity_blend);
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
        $m!(sprint_start_time);
        $m!(sprint_start_frac);
        $m!(sprint_end_time);
        $m!(sprint_end_frac);
        $m!(sprint_intent_time);
        $m!(view_offset);
        $m!(step_smoothing);
        $m!(traversal_kind);
        $m!(traversal_type);
        $m!(traversal_begin);
        $m!(traversal_mid);
        $m!(traversal_end);
        $m!(traversal_mid_frac);
        $m!(traversal_forward);
        $m!(traversal_ref);
        $m!(traversal_progress);
        $m!(traversal_start_time);
        $m!(traversal_release_time);
        $m!(traversal_blend_time);
        $m!(traversal_blend_offset);
        $m!(traversal_jump_time);
        $m!(traversal_hang);
        $m!(traversal_backoff);
        $m!(traversal_push);
        $m!(traversal_rest);
        $m!(traversal_gun);
        $m!(traversal_cycle);
        $m!(context_action);
        $m!(grappling);
        $m!(grapple_time);
        $m!(grapple_hook_vel);
        $m!(grapple_points);
        $m!(grapple_count);
        $m!(grapple_attached);
        $m!(grapple_pulling);
        $m!(grapple_swinging);
        $m!(grapple_retracting);
        $m!(grapple_forced);
        $m!(grapple_touched);
        $m!(grapple_power_used);
        $m!(grapple_fire_time);
        $m!(grapple_pull_time);
        $m!(grapple_attach_time);
        $m!(grapple_fast);
        $m!(grapple_slow_time);
        $m!(grapple_swing_length);
        $m!(grapple_swing_hold);
        $m!(suit_grapple_power);
        $m!(zipline);
        $m!(last_zipline);
        $m!(zipline_detach_time);
        $m!(zipline_state);
        $m!(zipline_mount_time);
        $m!(zipline_mount_alpha);
        $m!(zipline_start_time);
        $m!(zipline_end_time);
        $m!(zipline_from);
        $m!(zipline_from_velocity);
        $m!(zipline_target);
        $m!(zipline_use_point);
        $m!(zipline_param);
        $m!(zipline_reverse);
        $m!(step_time);
        $m!(step_side);
        $m!(land_speed);
        $m!(heavy_land_speed);
        $m!(melee_push);
        $m!(lunge_start_offset);
        $m!(lunge_end_offset);
        $m!(lunge_start);
        $m!(lunge_abort);
        $m!(lunge_flying);
        $m!(lunge_duration);
        $m!(lunge_max_time);
        $m!(lunge_max_end_speed);
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
    macro_rules! present {
        ($f:ident) => {
            if ours.$f.is_some() != theirs.$f.is_some() {
                out.push(format!("{} {} want {}", stringify!($f), ours.$f.is_some(), theirs.$f.is_some()));
            }
        };
    }
    present!(ground);
    present!(surface);
    present!(step_probe);
    present!(land_probe);
    present!(lunge_target);
    present!(grapple_parent);
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
    field!(grapple_stash);
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
    base.surface = state.surface;
    base.step_probe = state.step_probe;
    base.land_probe = state.land_probe;
    base.lunge_target = state.lunge_target;
    base.grapple_parent = state.grapple_parent;
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
