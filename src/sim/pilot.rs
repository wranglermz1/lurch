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
                view_offset: [0.0, 0.0, 60.0],
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
                view_offset: [0.0, 0.0, 32.0],
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
                view_offset: [0.0, 0.0, 14.0],
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
        mantle: mantle::Mantle {
            enabled: true,
            height: 80.0,
            duration: [0.7, 0.6, 0.5, 0.35],
            jump_up_speed: 240.0,
            jump_outward_speed: 188.0,
            ..crate::oracle::MANTLE_CONVARS
        },
        zipline: zipline::Tuning {
            mount_time: 0.5,
            use_cooldown: 0.4,
            speed: 600.0,
            acceleration: 400.0,
            jump_off_speed: 400.0,
            jump_off_damp: 1.0,
            view_offset: [-16.0, -6.0, 0.0],
            context_action_can_use: true,
        },
        sv_gravity: 750.0,
        sv_maxvelocity: 34000.0,
        extra_air_accel: 2.0,
        view: view::ViewTuning {
            sprint_tilt_accel: 35.0,
            sprint_tilt_max_vel: 2.0,
            sprint_tilt_turn_range: 120.0,
            punch_spring: view::Spring { constant: [65.0; 3], damping: [9.0; 3] },
            slide_tilt_side: 15.0,
            slide_tilt_player_speed: 400.0,
            slide_tilt_increase: 5.0,
            slide_tilt_decrease: 2.5,
        },
        viewkick_fall_dist_min: 10.0,
        viewkick_fall_dist_max: 70.0,
        viewkick_fall_dist_max_scale: 12.0,
        sprint_tilt_max_roll: 2.0,
        slide_fov_scale: 1.1,
        slide_fov_lerp_in: 0.25,
        slide_fov_lerp_out: 0.25,
        sprint_start_fast_duration: 0.2,
        sprint_start_duration: 0.8,
        sprint_start_delay: 0.2,
        sprint_end_duration: 0.15,
        sprint_view_offset: -6.0,
        fov: 70.0,
        pitch_offset_scale: 9.2,
        wall_start_kick: view::Kick { base: [-1.5, 0.0, 5.0], random: [0.5, 0.5, 0.5] },
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
        view_offset: stand.view_offset,
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
        zipline_allowed: true,
        fov_from: 1.0,
        fov_to: 1.0,
        ..Default::default()
    }
}

/// A pilot driven by commands, one movement call each, as RunCommand drives the player.
#[derive(Clone, Debug)]
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

    pub fn oldbuttons(&self) -> u32 {
        self.oldbuttons
    }

    /// Carries held buttons over to a new pilot, so keys held through a respawn make no edge.
    pub fn with_oldbuttons(mut self, oldbuttons: u32) -> Self {
        self.oldbuttons = oldbuttons;
        self
    }

    /// The camera at game time `now`, for the player's `cl_fovScale` option.
    pub fn eye(&self, s: &Settings, cl_fov_scale: f32, now: f32) -> Eye {
        let p = &self.player;
        Eye {
            origin: p.eye_position(s, self.origin, p.view_angles, now),
            angles: p.camera_angles(s, p.view_angles),
            fov: p.view_fov(s, cl_fov_scale, now),
        }
    }

    /// Runs `cmd` at game time `curtime`, with collision answered by `world`.
    pub fn step(&mut self, s: &Settings, cmd: &UserCmd, curtime: f32, world: &mut dyn Tracer) {
        self.step_with(s, cmd, curtime, world, &mantle::RECOVERED);
    }

    /// The same with a traversal's animation placed by `root`.
    pub fn step_with(&mut self, s: &Settings, cmd: &UserCmd, curtime: f32, world: &mut dyn Tracer, root: &dyn mantle::RootMotion) {
        let mut mv = setup_move(s, cmd, self.oldbuttons, self.origin, self.velocity);
        let p = &mut self.player;
        p.prev_velocity = self.velocity;
        p.abs_origin = self.origin;
        p.view_angles = mv.angles;
        let g = Globals { curtime, frametime: cmd.frametime * p.lagged };
        let surface = |_: u32| SURFACE_FRICTION;
        let mut m = Move { p, mv: &mut mv, s, g, tr: world, surface: &surface, scratch: Scratch::default(), stance_index: None, root };
        m.player_use();
        m.pre_think();
        m.process_movement(SV_OPTIMIZEDMOVEMENT);
        self.origin = mv.origin;
        self.velocity = mv.velocity;
        self.oldbuttons = mv.buttons;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Eye {
    pub origin: V3,
    pub angles: V3,
    pub fov: f32,
}

/// Our offline clock: an f64 sum of frametimes read as f32, as tests/testmap.rs drives, not
/// the game's tick-and-remainder rule (docs/oracle.md, Client input). It starts late because a
/// spawned player's event times are zero; the longest window measured from them is
/// `slide_boost_cooldown` (2 s).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clock(f64);

impl Clock {
    pub const START: f64 = 100.0;

    pub fn now(&self) -> f32 {
        self.0 as f32
    }

    pub fn advance(&mut self, frametime: f32) {
        self.0 += frametime as f64;
    }

    pub fn secs(&self) -> f64 {
        self.0
    }

    pub fn from_secs(secs: f64) -> Self {
        Clock(secs)
    }
}

impl Default for Clock {
    fn default() -> Self {
        Clock(Self::START)
    }
}
