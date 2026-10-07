//! The pilot class as the oracle plays it: player settings and ConVars, a player as it
//! spawns, and the loop that runs it one command at a time. Every recording agrees on these,
//! except `land_slowdown_height_min` in the four earliest walking takes, which never land.

use super::setup::{setup_move, UserCmd};
use super::*;

pub const SV_OPTIMIZEDMOVEMENT: bool = true;

/// Every surface stood on in the recordings (indices 20, 22, 87) gives the cap, 1.0, so one
/// default stands in for the game's surface property table until that is extracted.
pub const SURFACE_FRICTION: f32 = 0.8;

pub fn settings() -> Settings {
    Settings {
        stances: [
            Stance {
                hull_min: [-16.0, -16.0, 0.0],
                hull_max: [16.0, 16.0, 72.0],
                low_speed: 120.0,
                speed: 173.5,
                sprint_speed: 260.0,
                acceleration: 450.0,
                deceleration: 1250.0,
                low_acceleration: 2500.0,
                sprint_acceleration: 100.0,
                sprint_deceleration: 1250.0,
            },
            Stance {
                hull_min: [-16.0, -16.0, 0.0],
                hull_max: [16.0, 16.0, 47.0],
                low_speed: -1.0,
                speed: 80.0,
                sprint_speed: 0.0,
                acceleration: 2500.0,
                deceleration: -1.0,
                low_acceleration: -1.0,
                sprint_acceleration: -1.0,
                sprint_deceleration: -1.0,
            },
            Stance {
                hull_min: [-16.0, -16.0, 0.0],
                hull_max: [16.0, 16.0, 72.0],
                low_speed: -1.0,
                speed: 120.0,
                sprint_speed: 0.0,
                acceleration: 2500.0,
                deceleration: -1.0,
                low_acceleration: -1.0,
                sprint_acceleration: -1.0,
                sprint_deceleration: -1.0,
            },
        ],
        speed_scale_side: 1.0,
        speed_scale_back: 1.0,
        gravity_scale: 1.0,
        air_drag: 0.0,
        air_drag_constant: 0.0,
        air_strafe_disabled: false,
        boost_enabled: false,
        land_slowdown_duration: 1.0,
        land_slowdown_frac: 0.0,
        land_slowdown_time_power: 2.0,
        land_slowdown_height_min: 300.0,
        land_slowdown_height_max: 800.0,
        land_slowdown_no_sprint_frac: 0.7,
        sprint: true,
        jump_enabled: true,
        crouch: true,
        slide: true,
        slide_while_in_air: false,
        slide_auto_stand: false,
        slide_max_angle_dot: 0.6,
        slide_required_start_speed: 200.0,
        slide_required_start_speed_air: 90.0,
        slide_boost_enabled: true,
        slide_boost_cooldown: 2.0,
        slide_speed_boost: 150.0,
        slide_speed_boost_cap: 400.0,
        slide_max_stop_speed: 350.0,
        slide_stop_speed: 125.0,
        slide_accel: 50.0,
        slide_decel: 100.0,
        slide_want_to_stop_decel: 400.0,
        slide_velocity_decay: 0.7,
        slide_step_velocity_reduction: 10.0,
        jump_height: 56.0,
        slide_jump_height: 50.0,
        slide_max_jump_speed: 350.0,
        can_jump_crouched: false,
        anti_multi_jump_time_min: 0.15,
        anti_multi_jump_time_max: 0.75,
        anti_multi_jump_height_frac: 0.3,
        jump_grace_period: 0.2,
        skip_replenish: true,
        lurch: jump::Lurch {
            period_min: 0.2,
            period_max: 0.5,
            strength: 0.7,
            max: 0.7,
        },
        skip: jump::Skip {
            time: 1.0,
            height_speed: 450.0,
            height_fraction: 1.0,
            speed_retain: -1.0,
            speed_reduce: 100.0,
        },
        wallrun: false,
        climb: climb::Climb {
            enabled: true,
            height: 100.0,
            final_jump_up_height: 28.0,
            speed_start: 200.0,
            speed_end: 50.0,
            accel: 500.0,
            decel: 300.0,
            horz_speed: 100.0,
            horz_accel: 100.0,
            horz_decel: 500.0,
            vertical_gain_cutoff: 175.0,
            allowed_wall_distance: 17.0,
            ceiling_limit: 0.5,
            gravity_ramp_up_time: 1.0,
            duck_causes_fall_off: true,
            same_wall_allowed: true,
            same_wall_height: 0.0,
            no_gravity_ramp: false,
            mantle_angle_scale: 1.0,
            jump_outward_speed: 188.0,
            wall_max_speed_vertical: 112.0,
            wall_accel_vertical: 450.0,
            wall_decel_vertical: 300.0,
            wall_max_speed_horizontal: 225.0,
            wall_max_speed_horizontal_back: 50.0,
            wall_accel_horizontal: 200.0,
            wall_decel_horizontal: 500.0,
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
        },
        sv_gravity: 750.0,
        sv_maxvelocity: 34000.0,
        extra_air_accel: 2.0,
    }
}

/// A player standing at `origin` on world-up ground, as a pilot spawns.
pub fn spawn(origin: V3) -> Player {
    let s = settings();
    let (stand, duck) = (&s.stances[0], &s.stances[1]);
    Player {
        flags: 0x41,
        ground: Some(1),
        movetype: 2,
        up: [0.0, 0.0, 1.0],
        stance_gate: 100,
        collision_group: 11,
        mask: 0x201_400b,
        hull_min: stand.hull_min,
        hull_max: stand.hull_max,
        hull_duck_min: duck.hull_min,
        hull_duck_max: duck.hull_max,
        step_height: 22.0,
        can_step: true,
        speed_scale: 1.15,
        accel_scale: 1.0,
        surface_friction: 1.0,
        friction_scale: 1.0,
        lagged: 1.0,
        gravity: 1.0,
        stamina: 100.0,
        sprint_started_fresh: true,
        air_speed: 60.0,
        air_acceleration: 500.0,
        ramp_end: -100.0,
        left_ground_up: [0.0, 0.0, 1.0],
        ground_normal: [0.0, 0.0, 1.0],
        ground_plane: [0.0, 0.0, 1.0],
        abs_origin: origin,
        floor_height: origin[2],
        prev_up: [0.0, 0.0, 1.0],
        same_wall: true,
        wall_decel_scale: 1.0,
        precise_view: true,
        ..Default::default()
    }
}

/// A pilot driven by commands, one movement call each, as RunCommand drives the player.
pub struct Pilot {
    pub player: Player,
    pub origin: V3,
    pub velocity: V3,
    oldbuttons: u32,
}

impl Pilot {
    /// The spawned player's event times are all zero, so a clock starting at zero would
    /// read the first seconds as just after a jump, a landing and a slide: start it later.
    pub fn new(origin: V3) -> Self {
        Pilot { player: spawn(origin), origin, velocity: [0.0; 3], oldbuttons: 0 }
    }

    /// Runs `cmd` at game time `curtime`, with collision answered by `world`.
    pub fn step(&mut self, s: &Settings, cmd: &UserCmd, curtime: f32, world: &mut dyn Tracer) {
        let mut mv = setup_move(s, cmd, self.oldbuttons, self.origin, self.velocity);
        let p = &mut self.player;
        p.prev_velocity = self.velocity;
        p.abs_origin = self.origin;
        p.view_angles = mv.angles;
        let g = Globals { curtime, frametime: cmd.frametime * p.lagged };
        let surface = |_: u32| SURFACE_FRICTION;
        let mut m = Move { p, mv: &mut mv, s, g, tr: world, surface: &surface, scratch: Scratch::default(), stance_index: None };
        m.pre_think();
        m.process_movement(SV_OPTIMIZEDMOVEMENT);
        self.origin = mv.origin;
        self.velocity = mv.velocity;
        self.oldbuttons = mv.buttons;
    }
}
