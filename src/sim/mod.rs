//! The movement simulation. Names are ours; the structure follows the oracle's call tree so a
//! divergence can be pinned to one function (docs/oracle.md maps each to its RVA).

pub mod buttons;
pub mod climb;
pub mod collide;
pub mod duck;
pub mod ground;
pub mod input;
pub mod jump;
pub mod mantle;
pub mod math;
pub mod pilot;
pub mod rootmotion;
pub mod setup;
pub mod sprint;
pub mod view;
pub mod walk;
pub mod zipline;

use math::V3;

/// Per-stance block of the player settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stance {
    pub hull_min: V3,
    pub hull_max: V3,
    /// The eye above the origin.
    pub view_offset: V3,
    pub low_speed: f32,
    pub speed: f32,
    pub sprint_speed: f32,
    pub acceleration: f32,
    pub deceleration: f32,
    pub low_acceleration: f32,
    pub sprint_acceleration: f32,
    pub sprint_deceleration: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settings {
    pub stances: [Stance; 3],
    pub speed_scale_side: f32,
    pub speed_scale_back: f32,
    pub gravity_scale: f32,
    pub air_drag: f32,
    pub air_drag_constant: f32,
    pub air_strafe_disabled: bool,
    pub boost_enabled: bool,
    pub land_slowdown_duration: f32,
    pub land_slowdown_frac: f32,
    pub land_slowdown_time_power: f32,
    pub land_slowdown_height_min: f32,
    pub land_slowdown_height_max: f32,
    pub land_slowdown_no_sprint_frac: f32,
    pub sprint: bool,
    pub jump_enabled: bool,
    pub crouch: bool,
    pub slide: bool,
    pub slide_while_in_air: bool,
    pub slide_auto_stand: bool,
    pub slide_max_angle_dot: f32,
    pub slide_required_start_speed: f32,
    pub slide_required_start_speed_air: f32,
    pub slide_boost_enabled: bool,
    pub slide_boost_cooldown: f32,
    pub slide_speed_boost: f32,
    pub slide_speed_boost_cap: f32,
    pub slide_max_stop_speed: f32,
    pub slide_stop_speed: f32,
    pub slide_accel: f32,
    pub slide_decel: f32,
    pub slide_want_to_stop_decel: f32,
    pub slide_velocity_decay: f32,
    pub slide_step_velocity_reduction: f32,
    pub jump_height: f32,
    pub slide_jump_height: f32,
    pub slide_max_jump_speed: f32,
    pub can_jump_crouched: bool,
    pub anti_multi_jump_time_min: f32,
    pub anti_multi_jump_time_max: f32,
    pub anti_multi_jump_height_frac: f32,
    pub jump_grace_period: f32,
    pub skip_replenish: bool,
    pub lurch: jump::Lurch,
    pub skip: jump::Skip,
    pub wallrun: bool,
    pub climb: climb::Climb,
    pub mantle: mantle::Mantle,
    pub zipline: zipline::Tuning,
    pub sv_gravity: f32,
    pub sv_maxvelocity: f32,
    pub extra_air_accel: f32,
    pub view: view::ViewTuning,
    pub viewkick_fall_dist_min: f32,
    pub viewkick_fall_dist_max: f32,
    pub viewkick_fall_dist_max_scale: f32,
    /// `wallrun_viewpunch_onStart_*`.
    pub wall_start_kick: view::Kick,
    pub sprint_tilt_max_roll: f32,
    pub slide_fov_scale: f32,
    pub slide_fov_lerp_in: f32,
    pub slide_fov_lerp_out: f32,
    pub sprint_start_fast_duration: f32,
    pub sprint_start_duration: f32,
    pub sprint_start_delay: f32,
    pub sprint_end_duration: f32,
    pub sprint_view_offset: f32,
    pub fov: f32,
    pub pitch_offset_scale: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Globals {
    pub curtime: f32,
    pub frametime: f32,
}

/// The last move trace, which the oracle keeps on the player.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MoveTraceInfo {
    pub endpos: V3,
    pub normal: V3,
    pub kind: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Player {
    pub flags: u32,
    pub ground: Option<u64>,
    pub movetype: u8,
    pub up: V3,
    pub stance_gate: i32,
    pub collision_group: u8,
    pub mask: u32,
    pub hull_min: V3,
    pub hull_max: V3,
    pub hull_duck_min: V3,
    pub hull_duck_max: V3,
    pub step_height: f32,
    pub can_step: bool,
    pub speed_scale: f32,
    pub accel_scale: f32,
    /// Friction of the surface stood on, scaled by 1.25 and capped at 1; 0.25 just after
    /// leaving the ground upwards.
    pub surface_friction: f32,
    pub friction_scale: f32,
    pub lagged: f32,
    pub gravity: f32,
    pub sprinting: bool,
    pub frozen: bool,
    pub dead: bool,
    pub jetpack_active: bool,
    pub duck_timer: i32,
    pub ducked_in_air: bool,
    pub has_parent: bool,
    pub jump_suppressed: bool,
    pub duck_held: bool,
    pub duck_state: i32,
    pub traversal_state: u32,
    pub left_ground_time: f32,
    pub left_ground_up: V3,
    pub jumped_since_ground: bool,
    pub jumped_recently: bool,
    pub last_jump_time: f32,
    pub slide_boost_pending: bool,
    pub slide_start: f32,
    pub slide_boost: f32,
    pub sprint_intent: bool,
    pub sprint_until: f32,
    pub sprint_started_fresh: bool,
    pub sprint_toggle_lock: bool,
    pub sprint_blocked: bool,
    pub sprint_suppressed: bool,
    pub traversal_blocked: bool,
    pub stamina: f32,
    pub prev_velocity: V3,
    pub eye_angles: V3,
    /// The command's own view angles, which some checks prefer over the networked eye angles.
    pub view_angles: V3,
    pub precise_view: bool,
    pub sliding: bool,
    pub no_accel: bool,
    pub air_speed: f32,
    pub air_acceleration: f32,
    pub ramp_end: f32,
    pub push: V3,
    pub last_plane: V3,
    pub last_ground_time: f32,
    pub land_time: f32,
    pub land_height: f32,
    pub ground_normal: V3,
    /// The walkable plane under a walking player, used to keep the slide loop off it.
    pub ground_plane: V3,
    pub ledge_origin: V3,
    pub move_trace: MoveTraceInfo,
    /// Where the player stood when this call began; the oracle's abs origin, which the
    /// movement code reads while `mv.origin` moves.
    pub abs_origin: V3,
    /// The height climbs are measured from: drops with the player, rises only on floors.
    pub floor_height: f32,
    pub floor_frozen: bool,
    /// `up` as it was at the end of the last climb update.
    pub prev_up: V3,
    pub wall_start: V3,
    pub wall_count: i32,
    pub same_wall: bool,
    pub push_away_since: f32,
    pub wall_time: f32,
    pub wall_leave_time: f32,
    pub retry_time: f32,
    pub retry_origin: V3,
    pub retry_normal: V3,
    /// The last wall left, which the next climb must not be: count 0 or 1.
    pub repel_normal: V3,
    pub repel_time: f32,
    pub repel_count: i32,
    pub hanging: bool,
    pub dodge_lock: bool,
    pub dodged: bool,
    pub wall_decel_scale: f32,
    /// Sprint tilt, -1 to 1, its rate, and the yaw it was last measured from.
    pub sprint_tilt: f32,
    pub sprint_tilt_vel: f32,
    pub sprint_tilt_yaw: f32,
    pub punch_angle: V3,
    pub punch_vel: V3,
    pub weapon_punch_angle: V3,
    pub weapon_punch_vel: V3,
    /// Downward speed while airborne, which a landing reports to the view kick.
    pub fall_velocity: f32,
    /// The view setup's state: the slide tilt, and the FOV scale's ease (start, duration,
    /// from, to).
    pub slide_tilt: V3,
    pub fov_start: f32,
    pub fov_duration: f32,
    pub fov_from: f32,
    pub fov_to: f32,
    /// When sprinting started and stopped, the view's sprint fraction then, and when the
    /// wish to sprint began.
    pub sprint_start_time: f32,
    pub sprint_start_frac: f32,
    pub sprint_end_time: f32,
    pub sprint_end_frac: f32,
    pub sprint_intent_time: f32,
    /// The eye above the origin, eased through a duck.
    pub view_offset: V3,
    /// Where the eye lags the origin after a step up or a snap down, easing back to zero.
    pub step_smoothing: V3,
    /// When a grapple last pulled the player, which lets a mantle start without forward input.
    pub grapple_time: f32,
    /// Something outside movement forbids mantling now (a view mode, touching slip entities).
    pub mantle_blocked: bool,
    /// The traversal (mantle). Kind 0 is a mantle (1 a window). Type: 3 to 6 by where the eye
    /// starts against the ledge (below, level, above, high), 4 more when ducked, 11 hanging at
    /// rest, 12 jumping off. Begin, mid and end are the procedural path in the ledge entity's
    /// frame, reached at progress 0, mid_frac and 1.
    pub traversal_kind: i32,
    pub traversal_type: i32,
    pub traversal_begin: V3,
    pub traversal_mid: V3,
    pub traversal_end: V3,
    pub traversal_mid_frac: f32,
    pub traversal_forward: V3,
    pub traversal_ref: V3,
    pub traversal_progress: f32,
    pub traversal_start_time: f32,
    pub traversal_release_time: f32,
    pub traversal_blend_time: f32,
    pub traversal_blend_offset: V3,
    pub traversal_jump_time: f32,
    pub traversal_hang: bool,
    pub traversal_backoff: bool,
    pub traversal_push: bool,
    pub traversal_rest: f32,
    /// The first-person animation's cycle, which chases progress past the type's start.
    pub traversal_cycle: f32,
    /// A context action holds the player (9: a zipline); it blocks sprinting and other uses.
    pub context_action: i32,
    pub grappling: bool,
    /// When use last found something, and whether a press that found nothing retries while
    /// use stays held.
    pub use_time: f32,
    pub use_retry: bool,
    /// The zipline ridden (an entity id), the last one left and when. State: 1 grabbed, 2
    /// mounting, 3 riding. The mount runs `mount_alpha` 0 to 1 over `mount_time` from where
    /// the grab found the player (`from`, `from_velocity`) toward `target`, under the rope's
    /// `use_point`. `param` is where along the rope, by length, the ride is.
    pub zipline_allowed: bool,
    pub zipline: Option<u32>,
    pub last_zipline: Option<u32>,
    pub zipline_detach_time: f32,
    pub zipline_state: i32,
    pub zipline_mount_time: f32,
    pub zipline_mount_alpha: f32,
    pub zipline_start_time: f32,
    pub zipline_end_time: f32,
    pub zipline_from: V3,
    pub zipline_from_velocity: V3,
    pub zipline_target: V3,
    pub zipline_use_point: V3,
    pub zipline_param: f32,
    pub zipline_reverse: bool,
    /// The camera's sway while riding, a spring toward a noisy target, and its rate.
    pub zipline_view_offset: V3,
    pub zipline_view_offset_vel: V3,
    /// The view's lean on a zipline, a spring toward the roll the rope asks for, and its rate.
    pub zipline_roll: f32,
    pub zipline_roll_vel: f32,
}

#[derive(Clone, Debug, Default)]
pub struct MoveData {
    /// The player's origin was moved since the last movement call by something else.
    pub origin_changed: bool,
    /// CMoveData's leading flag byte; bit 2 forbids jumping this command.
    pub flags: u8,
    pub command_number: u32,
    pub angles: V3,
    pub buttons: u32,
    pub oldbuttons: u32,
    pub forward: f32,
    pub side: f32,
    pub up: f32,
    pub view_forward: V3,
    pub flat_forward: V3,
    pub flat_right: V3,
    pub flat_wish: V3,
    /// The wish direction laid along the ground plane, before scaling.
    pub ground_wish: V3,
    pub jump_dir: V3,
    pub maxspeed: f32,
    pub hull_height: f32,
    pub hull_half_width: f32,
    /// Buttons that went down this command, as SetupMove saw them.
    pub pressed: u32,
    /// The input's wish along the view, pitch included, at most unit length.
    pub view_wish: V3,
    /// The command's random seed, which view kicks draw from. Recordings do not hold it.
    pub random_seed: Option<u32>,
    pub origin: V3,
    pub velocity: V3,
}

impl MoveData {
    pub fn pressed(&self) -> u32 {
        self.buttons & !self.oldbuttons
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Trace {
    pub start: V3,
    pub endpos: V3,
    pub normal: V3,
    pub fraction: f32,
    pub allsolid: bool,
    pub startsolid: bool,
    /// trace_t +0x3b: lets a step land whatever the slope.
    pub flag3b: bool,
    pub surface: u32,
    /// trace_t +0x34; 0x100000 marks a surface nothing may climb.
    pub contents: u32,
    pub entity: u64,
}

/// A swept player hull, as the oracle's trace wrapper builds it.
#[derive(Clone, Copy, Debug)]
pub struct HullQuery {
    pub start: V3,
    pub end: V3,
    pub mins: V3,
    pub maxs: V3,
    pub mask: u32,
}

pub trait Tracer {
    fn hull(&mut self, q: &HullQuery) -> Trace;

    fn ziplines(&self) -> &[zipline::Zipline] {
        &[]
    }
}

/// Whatever the movement code knows about where it is in one ProcessMovement call: the
/// ground probe it can reuse, and where that probe ended.
#[derive(Clone, Copy, Debug, Default)]
pub struct Scratch {
    pub probe: Option<Trace>,
    pub probe_end: V3,
    /// A view kick was due but the command carried no random seed to draw it from.
    pub unseeded_kick: bool,
}

pub struct Move<'a> {
    pub p: &'a mut Player,
    pub mv: &'a mut MoveData,
    pub s: &'a Settings,
    pub g: Globals,
    pub tr: &'a mut dyn Tracer,
    /// Friction of a collision surface by its index; world data, like the collision itself.
    pub surface: &'a dyn Fn(u32) -> f32,
    pub scratch: Scratch,
    /// The stance block PlayerMove picked on entry; it holds for the whole call.
    pub stance_index: Option<usize>,
    /// Where a traversal's animation puts the player; model data, not in the image.
    pub root: &'a dyn mantle::RootMotion,
}

const OPTIMIZED_LEAVE_SPEED: f32 = 200.0;

impl Player {
    /// Runs before each movement call: while standing, remember when and with which up
    /// vector, for the jump grace period; and keep the height climbs are measured from.
    /// Airborne, the fall speed is taken from the velocity the call starts with.
    pub fn pre_think(&mut self, curtime: f32, velocity: V3) {
        if self.ground.is_none() {
            self.fall_velocity = (velocity[1] * 0.0 + velocity[0] * 0.0) - velocity[2] * 1.0;
        }
        if self.ground.is_some() {
            self.left_ground_time = curtime;
            self.left_ground_up = self.up;
        }
        self.update_floor_height();
    }

    pub fn ducked(&self) -> bool {
        self.flags & 2 != 0
    }

    pub fn on_ground(&self) -> bool {
        self.ground.is_some()
    }
}

impl Move<'_> {
    pub fn pre_think(&mut self) {
        self.p.pre_think(self.g.curtime, self.mv.velocity);
    }

    /// C_GameMovement::ProcessMovement: the whole movement call for one command.
    pub fn process_movement(&mut self, optimized: bool) {
        self.mv.maxspeed = self.max_speed();
        self.player_move(optimized);
        self.mv.oldbuttons = self.mv.buttons;
    }

    /// FinishMove's part that later movement reads: the velocity the call ended with.
    pub fn finish_move(&mut self) {
        self.p.prev_velocity = self.mv.velocity;
    }

    pub fn player_move(&mut self, optimized: bool) {
        self.stance_index = Some(if self.p.stance_gate < 1 { 2 } else { ((self.p.flags >> 1) & 1) as usize });
        self.punch_springs();
        if 0 < self.p.duck_timer {
            self.p.duck_timer = (self.p.duck_timer - (self.g.frametime * 1000.0) as i32).max(0);
        }
        if self.p.movetype != 12 {
            if self.p.movetype == 2 && !self.mv.origin_changed && optimized {
                if OPTIMIZED_LEAVE_SPEED < math::dot(self.p.up, self.mv.velocity) {
                    self.set_ground(None);
                }
            } else if self.p.movetype != 2 || !self.on_ground() || !self.stay_on_ground() {
                self.categorize(false);
            }
        }
        if !self.on_ground() {
            let v = self.mv.velocity;
            self.p.fall_velocity = (0.0 * v[1] + 0.0 * v[0]) + -1.0 * v[2];
        }
        self.duck();
        self.update_sprint();
        self.mv.maxspeed = self.max_speed();
        self.sprint_tilt();
        match self.p.movetype {
            2 | 7 if self.on_zipline() => self.zipline_move(),
            2 | 7 => {
                if self.try_traversal() {
                    self.traversal_move(true);
                } else {
                    self.full_walk_move();
                }
            }
            mantle::MOVETYPE_TRAVERSE => self.traversal_move(false),
            _ => {}
        }
        self.p.decay_step_smoothing(self.g.frametime);
    }

    pub fn stance(&self) -> &Stance {
        let i = self.stance_index.unwrap_or(if self.p.stance_gate < 1 { 2 } else { ((self.p.flags >> 1) & 1) as usize });
        &self.s.stances[i]
    }

    pub fn ducked(&self) -> bool {
        self.p.flags & 2 != 0
    }

    pub fn on_ground(&self) -> bool {
        self.p.ground.is_some()
    }
}
