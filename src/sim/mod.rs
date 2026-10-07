//! The movement simulation. Names are ours; the structure follows the oracle's call tree so a
//! divergence can be pinned to one function (docs/oracle.md maps each to its RVA).

pub mod collide;
pub mod duck;
pub mod ground;
pub mod jump;
pub mod math;
pub mod sprint;
pub mod walk;

use math::V3;

/// Per-stance block of the player settings.
#[derive(Clone, Debug, Default)]
pub struct Stance {
    pub hull_min: V3,
    pub hull_max: V3,
    pub low_speed: f32,
    pub speed: f32,
    pub sprint_speed: f32,
    pub acceleration: f32,
    pub deceleration: f32,
    pub low_acceleration: f32,
    pub sprint_acceleration: f32,
    pub sprint_deceleration: f32,
}

#[derive(Default)]
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
    pub sv_gravity: f32,
    pub sv_maxvelocity: f32,
    pub extra_air_accel: f32,
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
    pub stamina: f32,
    pub prev_velocity: V3,
    pub eye_angles: V3,
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
    pub flat_forward: V3,
    pub flat_right: V3,
    pub flat_wish: V3,
    /// The wish direction laid along the ground plane, before scaling.
    pub ground_wish: V3,
    pub jump_dir: V3,
    pub maxspeed: f32,
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
}

/// Whatever the movement code knows about where it is in one ProcessMovement call: the
/// ground probe it can reuse, and where that probe ended.
#[derive(Clone, Copy, Debug, Default)]
pub struct Scratch {
    pub probe: Option<Trace>,
    pub probe_end: V3,
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
}

const OPTIMIZED_LEAVE_SPEED: f32 = 200.0;

impl Move<'_> {
    /// Runs before each movement call: while standing, remember when and with which up
    /// vector, for the jump grace period.
    pub fn pre_think(&mut self) {
        if self.on_ground() {
            self.p.left_ground_time = self.g.curtime;
            self.p.left_ground_up = self.p.up;
        }
    }

    /// C_GameMovement::ProcessMovement: the whole movement call for one command.
    pub fn process_movement(&mut self, optimized: bool) {
        self.mv.maxspeed = self.max_speed();
        self.player_move(optimized);
        self.mv.oldbuttons = self.mv.buttons;
    }

    pub fn player_move(&mut self, optimized: bool) {
        self.stance_index = Some(if self.p.stance_gate < 1 { 2 } else { ((self.p.flags >> 1) & 1) as usize });
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
        self.duck();
        self.update_sprint();
        self.mv.maxspeed = self.max_speed();
        if matches!(self.p.movetype, 2 | 7) {
            self.full_walk_move();
        }
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
