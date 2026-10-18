//! A melee attack on the player: PlayerMelee_StartAttack (`0x868280`) and EndAttack (`0x868350`),
//! and the push a punch on the ground gets when it has no lunge target (`0x826540`, from the
//! floor path of `0x818320`).
//!
//! The script starts the attack (`CodeCallback_OnMeleePressed` in `sh_melee.gnut`) and the melee
//! weapon ends it; neither is code movement runs, so `Pilot::melee` and `end_melee` are called
//! from outside the command.
//!
//! The lunge toward an entity: `Lunge_SetTargetEntity` (`0x868920`), which the script calls when
//! the attack's activity starts with the first entity `PlayerMelee_LungeConeTrace` (`0x9daf10`)
//! finds; PlayerMove's switch to movetype 15 while it lasts (`0x9dc860`) and the move it runs
//! there (`0x8257f0`). The blow's trace (`PlayerMelee_AttackTrace`, `0x9dbd20`) and KnockBack
//! (`0x8713f0`), which the script calls on what the blow hits.
//!
//! Not modelled: the titans' branch of the push; `+0x2c88`, which the push needs at or below 0
//! and nothing ours sets; `+0x2c7e` (sprinting when the attack started), `+0x2c94` and
//! `+0x1bdd`. A lunge to a position (doors, `Lunge_SetTargetPosition`), the lunge's pitch lock,
//! lag compensation of the target, the cylinders' closest distance its abort measures with
//! `melee_lunge_use_closest_distance_between_cylinders` (`0x825640`; ours measures from the
//! origins) and the ground probe at its end (`0x806cf0`, which only acts upside down). The cone
//! trace's entity filter (`0x14aed80`, with `melee_cone_trace_box_check` and
//! `melee_lunge_dot_check`) and its order (`0xa1dd20`) are not read: ours takes a candidate whose
//! centre is in range and inside the cone's four planes, nearest first. The blow's trace length
//! (`0x9da540`), its mask, and its last, level try are not read either: ours traces `range`
//! along the view, then the four rays 5° off it, with Source's `MASK_SHOT`. KnockBack's four
//! slots (`m_playerKnockBacks`, `+0x4120`), which movement never reads.

use super::math::*;
use super::{Move, Player, Tracer};
use crate::world::Body;

/// `PLAYER_MELEE_STATE_HUMAN_KICK_ATTACK`, the state a pilot's punch starts in.
pub const HUMAN_KICK_ATTACK: u32 = 1;

/// The melee weapon's `melee_lunge_time` and `melee_lunge_no_target_range` (`_base_melee.txt`,
/// which the fists, the offhand melee `melee_pilot_emptyhanded` and every heirloom ours draws
/// keep).
pub const LUNGE_TIME: f32 = 0.35;
pub const LUNGE_NO_TARGET_RANGE: f32 = 60.0;
/// The fists' `melee_lunge_target_range` and `melee_lunge_target_angle` (`_base_melee.txt`), which
/// the cone trace takes over the player's `melee_cone_trace_*` when the attack has a weapon.
pub const LUNGE_TARGET_RANGE: f32 = 60.0;
pub const LUNGE_TARGET_ANGLE: f32 = 30.0;
/// The fists' `melee_range` (`melee_pilot_emptyhanded.txt`): the blow's reach.
pub const MELEE_RANGE: f32 = 60.0;

/// ConVars read from the dump.
const MELEE_LUNGE_FORCE_ENABLE_FLYING: bool = true;
const MELEE_LUNGE_SLIDE: bool = true;
const MELEE_LUNGE_ABORT_IF_BLOCKED: bool = true;
const MELEE_LUNGE_ABORT_DISTANCE: f32 = 0.25;

/// PlayerMove's movetype while a lunge moves the player.
pub const MOVETYPE_LUNGE: u8 = 15;
const MOVETYPE_WALK: u8 = 2;

/// What the cone trace's visibility rays stop on, and the blow's trace (Source's `MASK_SHOT`).
const MASK_VISIBLE: u32 = 0x1;
const MASK_SHOT: u32 = 0x4600_400b;

/// An entity's origin as the engine keeps one for a box standing on its floor: the middle of its
/// bottom.
pub fn body_origin(b: &Body) -> V3 {
    [(b.mins[0] + b.maxs[0]) * 0.5, (b.mins[1] + b.maxs[1]) * 0.5, b.mins[2]]
}

/// `WorldSpaceCenter`.
pub fn body_center(b: &Body) -> V3 {
    [(b.mins[0] + b.maxs[0]) * 0.5, (b.mins[1] + b.maxs[1]) * 0.5, (b.mins[2] + b.maxs[2]) * 0.5]
}

/// What the blow hit (`PlayerMelee_AttackTrace`'s table): the entity, none for the world, where
/// and on which face.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blow {
    pub entity: Option<u64>,
    pub at: V3,
    pub normal: V3,
}

impl Player {
    /// PlayerMelee_StartAttack (`0x868280`): the climbing arms stop (`0x866e00`) and the attack
    /// starts now, having hit nothing yet.
    pub fn start_melee(&mut self, state: u32, curtime: f32) {
        if self.climb_arms {
            self.climb_arms = false;
            self.view_entity = false;
        }
        self.melee_active = true;
        self.melee_start = curtime;
        self.melee_hit = false;
        self.melee_state = state;
    }

    /// PlayerMelee_EndAttack (`0x868350`).
    pub fn end_melee(&mut self) {
        self.melee_active = false;
        self.melee_start = 0.0;
        self.melee_state = 0;
    }

    /// KnockBack (`0x8713f0`): the player's velocity becomes `velocity` at once, and a player on
    /// the ground leaves it. A zero velocity or no time does nothing.
    pub fn knock_back(&mut self, velocity: V3, duration: f32) -> Option<V3> {
        let m = max3(velocity);
        if m == 0.0 || !(0.0 < duration) {
            return None;
        }
        if self.ground.is_some() {
            if self.movetype == super::mantle::MOVETYPE_TRAVERSE {
                self.movetype = MOVETYPE_WALK;
            }
            self.ground = None;
            self.flags &= !1;
        }
        Some(velocity)
    }

    /// `0x868f80`: no lunge.
    pub fn lunge_clear(&mut self) {
        self.lunge_target = None;
        self.lunge_start_offset = ZERO;
        self.lunge_end_offset = ZERO;
        self.lunge_start = 0.0;
        self.lunge_abort = -1.0;
        self.lunge_flying = false;
        self.lunge_duration = 0.5;
        self.lunge_max_time = -1.0;
        self.lunge_max_end_speed = 0.0;
    }

    /// `Lunge_SetTargetEntity` (`0x868920`) for the player at `origin`: the lunge starts now and
    /// ends against the target's side nearest the player, as far from its origin as the two
    /// boxes' half widths, at the height the move picks.
    pub fn lunge_set_target(&mut self, target: &Body, origin: V3, curtime: f32) -> bool {
        self.lunge_clear();
        self.lunge_target = Some(target.id);
        let t = body_origin(target);
        let so = [origin[0] - t[0], origin[1] - t[1], origin[2] - t[2]];
        self.lunge_start_offset = so;
        let (tx, ty) = (target.maxs[0] - target.mins[0], target.maxs[1] - target.mins[1]);
        let theirs = if tx <= ty { ty } else { tx };
        let (lo, hi) = self.hull();
        let (px, py) = (hi[0] - lo[0], hi[1] - lo[1]);
        let ours = if px <= py { py } else { px };
        let r = ours * 0.5 + theirs * 0.5;
        let l = (so[1] * so[1] + so[0] * so[0] + so[2] * so[2]).sqrt();
        let inv = 1.0 / if l <= TINY { TINY } else { l };
        self.lunge_end_offset = [so[0] * inv * r, so[1] * inv * r, 0.0];
        self.lunge_start = curtime;
        self.lunge_abort = -1.0;
        self.lunge_duration = self.lunge_time();
        true
    }

    /// The hull the player stands in now.
    fn hull(&self) -> (V3, V3) {
        if self.flags & 2 != 0 { (self.hull_duck_min, self.hull_duck_max) } else { (self.hull_min, self.hull_max) }
    }

    /// `0x869210`: the lunge has a target still there, has not run past its limit, and has not
    /// given up while the attack is on.
    fn lunge_active(&self, target: Option<Body>, curtime: f32) -> bool {
        if target.is_none() {
            return false;
        }
        (self.lunge_max_time <= 0.0 || curtime - self.lunge_start <= self.lunge_max_time)
            && (!self.melee_active || self.lunge_abort <= 0.0 || curtime <= self.lunge_abort)
    }

    /// `0x9dc860`: whether PlayerMove runs the lunge. Once the attack outlasts the lunge, it
    /// gives up a tenth of a second later.
    pub fn lunge_running(&mut self, target: Option<Body>, curtime: f32) -> bool {
        if !self.lunge_active(target, curtime) {
            return false;
        }
        if self.lunge_abort <= 0.0 {
            let since = curtime - self.lunge_start;
            if self.melee_active && self.lunge_duration <= since && since != self.lunge_duration {
                self.lunge_abort = curtime + 0.1;
            }
        } else if self.lunge_abort < curtime {
            return false;
        }
        true
    }

    /// `0x868750`: the melee weapon's lunge time while an attack is on, else 0.5.
    fn lunge_time(&self) -> f32 {
        if self.melee_active && 0.0 < LUNGE_TIME { LUNGE_TIME } else { 0.5 }
    }

    /// `0x868890`: the acceleration that covers the no-target range in the lunge time from rest,
    /// worked in double.
    fn lunge_accel(&self) -> f32 {
        let t = self.lunge_time();
        if 0.0 >= t {
            return 0.0;
        }
        let d = LUNGE_NO_TARGET_RANGE as f64;
        ((d + d) / (t * t) as f64) as f32
    }
}

/// An angle in degrees brought into -180 to 180.
fn normalize_angle(a: f32) -> f32 {
    let a = a % 360.0;
    if 180.0 < a {
        a - 360.0
    } else if a < -180.0 {
        a + 360.0
    } else {
        a
    }
}

fn max3(a: V3) -> f32 {
    let (x, y, z) = (a[0].abs(), a[1].abs(), a[2].abs());
    let m = if x <= y { y } else { x };
    if m <= z { z } else { m }
}

/// `PlayerMelee_LungeConeTrace` for the fists (`0x9db3c0`, `0x9daf10`, `0x9da6e0`): from the eye,
/// looking `angles` with the pitch dipped toward 15° below level, the nearest body whose centre
/// is within `LUNGE_TARGET_RANGE` and `LUNGE_TARGET_ANGLE` of the view and which a ray from the
/// eye reaches, at its centre or else at its top.
pub fn lunge_cone_trace(world: &mut dyn Tracer, eye: V3, angles: V3) -> Option<Body> {
    let pitch = angles[0];
    let f = ((normalize_angle(pitch) as f64).abs() as f32 / 90.0).clamp(0.0, 1.0);
    let (fwd, right, up) = angle_vectors([pitch + (1.0 - f) * 15.0, angles[1], angles[2]]);
    let half = (deg2rad(LUNGE_TARGET_ANGLE) * 0.5).tan();
    let mut found: Vec<(f32, Body)> = world
        .bodies()
        .iter()
        .filter_map(|b| {
            let v = sub(body_center(b), eye);
            let (d, along) = (dot(v, v), dot(v, fwd));
            let inside = d <= LUNGE_TARGET_RANGE * LUNGE_TARGET_RANGE && 0.0 < along && dot(v, right).abs() <= along * half && dot(v, up).abs() <= along * half;
            inside.then_some((d, *b))
        })
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut sees = |to: V3| 1.0 <= world.hull(&super::HullQuery { start: eye, end: to, mins: ZERO, maxs: ZERO, mask: MASK_VISIBLE }).fraction;
    found.into_iter().map(|(_, b)| b).find(|b| sees(body_center(b)) || sees([body_center(b)[0], body_center(b)[1], b.maxs[2]]))
}

/// `PlayerMelee_AttackTrace` (`0x9dbd20`, `0x9db670`): the blow from the eye, `range` along the
/// view, or toward the lunge's target while the lunge is on, where only that target counts. A
/// ray that misses is tried again 5° off the view on four sides.
pub fn attack_trace(world: &mut dyn Tracer, eye: V3, angles: V3, range: f32, lunge: Option<Body>) -> Option<Blow> {
    let mut ray = |dir: V3| {
        let end = [dir[0] * range + eye[0], dir[1] * range + eye[1], dir[2] * range + eye[2]];
        let t = world.hull(&super::HullQuery { start: eye, end, mins: ZERO, maxs: ZERO, mask: MASK_SHOT });
        (t.fraction < 1.0 && !t.startsolid).then(|| Blow { entity: (t.entity != 1).then_some(t.entity), at: t.endpos, normal: t.normal })
    };
    let around = |ray: &mut dyn FnMut(V3) -> Option<Blow>, d: V3| {
        ray(d).or_else(|| {
            let axis = normalize([d[1] - d[2] * 0.0, d[2] * 0.0 - d[0], d[0] * 0.0 - d[1] * 0.0]);
            let tilted = rotate(d, axis, -0.087_266_46);
            (0..4).find_map(|i| ray(rotate(tilted, d, deg2rad(i as f32 * 90.0))))
        })
    };
    if let Some(b) = lunge {
        let to = sub(body_center(&b), eye);
        let dir = scale(to, 1.0 / dot(to, to).sqrt());
        if let Some(hit) = around(&mut ray, dir).filter(|h| h.entity == Some(b.id)) {
            return Some(hit);
        }
    }
    around(&mut ray, angle_vectors(angles).0)
}

/// `v` turned by `a` radians about the unit `axis` (Rodrigues' formula).
fn rotate(v: V3, axis: V3, a: f32) -> V3 {
    let (s, c) = a.sin_cos();
    let k = dot(axis, v) * (1.0 - c);
    let x = cross(axis, v);
    [v[0] * c + x[0] * s + axis[0] * k, v[1] * c + x[1] * s + axis[1] * k, v[2] * c + x[2] * s + axis[2] * k]
}

impl Move<'_> {
    /// PlayerMove's movetype switch (`0x822b81` chunk) for the lunge: walking turns into the
    /// lunge's movetype while one runs (`0x9dc860`), and back, the lunge cleared, when it stops.
    pub(super) fn lunge_movetype(&mut self) {
        let target = self.p.lunge_target.and_then(|id| self.tr.body(id));
        if self.p.movetype == MOVETYPE_WALK {
            if self.p.lunge_running(target, self.g.curtime) {
                self.p.movetype = MOVETYPE_LUNGE;
            }
        } else if self.p.movetype == MOVETYPE_LUNGE && !self.p.lunge_running(target, self.g.curtime) {
            self.p.lunge_clear();
            self.p.movetype = MOVETYPE_WALK;
        }
    }

    /// LungeMove (`0x8257f0`): the player moves toward the end beside the target, the share of
    /// the way covered growing as the square of the lunge's elapsed share, at no more than the
    /// stance's speed, sliding along what blocks it while that still heads for the target. A
    /// lunge that falls behind by more than half the hull's width, and barely moved, gives up.
    /// The velocity ends at zero.
    pub(super) fn lunge_move(&mut self) {
        let Some(target) = self.p.lunge_target.and_then(|id| self.tr.body(id)) else { return };
        let o = self.mv.origin;
        let t = body_origin(&target);
        let (lo, hi) = self.p.hull();
        let ey = t[1] + self.p.lunge_end_offset[1];
        let centre = ((target.maxs[2] - target.mins[2]) + 0.0) * 0.5 + t[2];
        let half = (hi[2] - lo[2]) * 0.5 + t[2];
        let cap = if half <= centre { half } else { centre };
        let mut z = o[2];
        if o[2] <= t[2] {
            z = t[2];
        }
        if cap <= z {
            z = cap;
        }
        let ex = t[0] + self.p.lunge_end_offset[0];
        let mut ez = z + self.p.lunge_end_offset[2];
        if !MELEE_LUNGE_FORCE_ENABLE_FLYING && !self.p.lunge_flying {
            ez = self.mv.origin[2];
        }
        let (t0, now) = (self.p.lunge_start, self.g.curtime);
        let t1 = t0 + self.p.lunge_duration;
        let frac = if t0 == t1 {
            if now - t1 < 0.0 { 0.0 } else { 1.0 }
        } else {
            ((now - t0) / (t1 - t0)).clamp(0.0, 1.0) + 0.0
        };
        let f2 = frac * frac;
        let mut want = [(ex - o[0]) * f2 + o[0], (ey - o[1]) * f2 + o[1], (ez - o[2]) * f2 + o[2]];
        let inv = 1.0 / self.g.frametime;
        let cur = self.mv.origin;
        let mut v = [(want[0] - cur[0]) * inv, (want[1] - cur[1]) * inv, (want[2] - cur[2]) * inv];
        let s2 = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
        if 0.0 < s2 {
            let st = &self.s.stances[if self.p.stance_gate < 1 { 2 } else { ((self.p.flags >> 1) & 1) as usize }];
            let cap = self.p.speed_scale * if self.p.sprinting { st.sprint_speed } else { st.speed };
            if cap * cap < s2 {
                let k = 1.0 / s2.sqrt();
                v = [k * v[0] * cap, k * v[1] * cap, k * v[2] * cap];
            }
        }
        self.mv.velocity = v;
        if self.p.ground.is_some() {
            let n = self.p.ground_normal;
            let d = (want[1] - cur[1]) * n[1] + (want[0] - cur[0]) * n[0] + (want[2] - cur[2]) * n[2];
            if d < 0.0078125 {
                let k = 0.0078125 - d;
                want = [want[0] + k * n[0], want[1] + k * n[1], want[2] + k * n[2]];
            }
        }
        let mut tr = self.trace(cur, want);
        if tr.fraction != 1.0 {
            self.along_slope(&mut tr, true, Some(want), false);
            let c = self.mv.origin;
            let (dy, dx, dz) = (want[1] - c[1], want[0] - c[0], want[2] - c[2]);
            if MELEE_LUNGE_SLIDE && 1.0 <= dy * dy + dx * dx + dz * dz {
                let again = self.trace(c, want);
                if again.fraction != 1.0 {
                    let e = again.endpos;
                    let n = again.normal;
                    let d = [want[0] - e[0], want[1] - e[1], want[2] - e[2]];
                    let dn = d[1] * n[1] + d[0] * n[0] + d[2] * n[2];
                    let dd = d[1] * d[1] + d[0] * d[0] + d[2] * d[2];
                    let r = dd - dn * dn;
                    if 0.0 < r {
                        let k = (dd / r).sqrt();
                        let m = k * dn * 1.000_015_3;
                        let slide = [d[0] * k - m * n[0], d[1] * k - m * n[1], d[2] * k - m * n[2]];
                        let so = self.p.lunge_start_offset;
                        let l = (so[0] * so[0] + so[1] * so[1] + so[2] * so[2]).sqrt();
                        let li = 1.0 / if l <= TINY { TINY } else { l };
                        let toward = -(li * so[1]) * slide[1] + -(li * so[0]) * slide[0] + -(li * so[2]) * slide[2];
                        if 0.0 < toward {
                            let slid = self.trace(e, add(slide, e));
                            self.mv.origin = slid.endpos;
                        }
                    }
                }
            }
        } else {
            self.mv.origin = tr.endpos;
        }
        if MELEE_LUNGE_ABORT_IF_BLOCKED && t0 + 0.05 < now && 0.05 < t1 - now {
            let c = self.mv.origin;
            let (so, eo) = (self.p.lunge_start_offset, self.p.lunge_end_offset);
            let whole = ((eo[1] - so[1]) * (eo[1] - so[1]) + (eo[0] - so[0]) * (eo[0] - so[0]) + (eo[2] - so[2]) * (eo[2] - so[2])).sqrt();
            let (ry, rx, rz) = (ey - c[1], ex - c[0], ez - c[2]);
            let left = (ry * ry + rx * rx + rz * rz).sqrt();
            let (my, mx, mz) = (c[1] - o[1], c[0] - o[0], c[2] - o[2]);
            let need = (left / (t1 - now)) * self.g.frametime * MELEE_LUNGE_ABORT_DISTANCE;
            if ((0.0 - whole) * f2 + whole) - left < -self.mv.hull_half_width && my * my + mx * mx + mz * mz < need * need {
                self.p.lunge_abort = now;
            }
        }
        let max = self.p.lunge_max_end_speed;
        if 0.0 < max {
            let v = self.mv.velocity;
            let l2 = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
            if 0.0 < l2 && max * max < l2 {
                let k = 1.0 / l2.sqrt();
                self.mv.velocity = [k * v[0] * max, k * v[1] * max, k * v[2] * max];
            }
        } else {
            self.mv.velocity = ZERO;
        }
    }

    /// `0x826540`: through the lunge time of a punch on the ground with no slide and no hit, the
    /// push adds speed along the view at the lunge's acceleration until the pushed velocity's
    /// speed along the view reaches `(forward input / 2 + 1/2) × lunge time × acceleration`,
    /// no faster than the pushed velocity was or that speed. The push is kept apart from the
    /// velocity: the ground move moves by both, and the velocity keeps none of it.
    pub(super) fn melee_push(&mut self) {
        if !self.p.melee_active || self.p.melee_state == 0 {
            self.p.melee_push = ZERO;
            return;
        }
        if self.p.ground.is_some() && !self.p.sliding && !self.p.melee_hit && 0.0 < LUNGE_NO_TARGET_RANGE {
            let t = self.p.lunge_time();
            if t <= 0.0 || t < self.g.curtime - self.p.melee_start {
                self.p.melee_push = ZERO;
                return;
            }
            let (push, vel) = (self.p.melee_push, self.mv.velocity);
            let v = [push[0] + vel[0], push[1] + vel[1], push[2] + vel[2]];
            let f = self.mv.view_forward;
            let accel = if 0.0 < t { (LUNGE_NO_TARGET_RANGE + LUNGE_NO_TARGET_RANGE) / (t * t) } else { 0.0 };
            let input = (self.mv.forward - 0.0).clamp(0.0, 1.0);
            let target = (input * 0.5 + 0.5) * self.p.lunge_time() * accel;
            if target <= (v[1] * f[1] + v[0] * f[0]) + v[2] * f[2] {
                return;
            }
            let a = self.p.lunge_accel() * self.g.frametime;
            let mut n = [a * f[0] + v[0], a * f[1] + v[1], a * f[2] + v[2]];
            let mut cap = (v[1] * v[1] + v[0] * v[0]) + v[2] * v[2];
            if cap <= target * target {
                cap = target * target;
            }
            let l2 = (n[1] * n[1] + n[0] * n[0]) + n[2] * n[2];
            if cap < l2 {
                let (s, k) = (cap.sqrt(), 1.0 / l2.sqrt());
                n = [k * n[0] * s, k * n[1] * s, k * n[2] * s];
            }
            self.p.melee_push = [(n[0] - v[0]) + push[0], (n[1] - v[1]) + push[1], (n[2] - v[2]) + push[2]];
            return;
        }
        self.p.melee_push = ZERO;
    }
}
