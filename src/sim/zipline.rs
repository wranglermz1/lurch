//! Ziplines: the use that grabs one, the mount onto the rope, the ride along it, and every way
//! off. A rope is the polyline its zipline entity holds this frame (`Zipline::points`), run
//! from the zipline to its `next` entity, which is another zipline (a chain) or an end.
//! docs/oracle.md, Zipline, maps each function to its RVA.

pub use super::buttons::IN_USE;
use super::cue::Cue;
use super::math::*;
use super::{HullQuery, Move, Trace};

const USE_BUTTONS: u32 = 0x8000_2020;
const DROP_BUTTONS: u32 = 0x400_0006;
pub const REQUESTED: i32 = 1;
pub const MOUNTING: i32 = 2;
pub const RIDING: i32 = 3;
pub const LINE: i32 = 0;
pub const VINE: i32 = 1;
pub const VERTICAL: i32 = 2;
const CONTEXT_ZIPLINE: i32 = 9;
const DETACH_SALT: u32 = 0x7670_4a58;
const SEARCH_HEIGHT: f32 = 72.0;
const SEARCH_RADIUS2: f32 = 14400.0;
const SEGMENT_REACH2: f32 = 16900.0;
const RAY_START: f32 = 7.0;
const RAY_NEAR: f32 = 0.1;
const END_PROBE: f32 = 200.0;
const VINE_STOP: f32 = 30.0;
const USE_REPEAT: f32 = 0.7;
const SKIN: f32 = 0.1;
const MOVETYPE_TRAVERSE: u8 = 12;
const MOVETYPE_WALK: u8 = 2;

/// The `zipline*` player settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tuning {
    pub mount_time: f32,
    pub use_cooldown: f32,
    pub speed: f32,
    pub acceleration: f32,
    pub jump_off_speed: f32,
    pub jump_off_damp: f32,
    pub view_offset: V3,
    pub roll_strength: f32,
    pub roll_max: f32,
    pub roll_view_angle_min: f32,
    pub roll_view_angle_max: f32,
    /// `context_action_can_use`: any zipline is usable, not only those flagged so.
    pub context_action_can_use: bool,
}

/// The rope ridden, as the camera reads it: its kind and its first and last points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RopeView {
    pub kind: i32,
    pub ends: [V3; 2],
}

impl RopeView {
    /// The rope of zipline `id` in `ziplines`, while it runs somewhere.
    pub fn of(ziplines: &[Zipline], id: Option<u32>) -> Option<Self> {
        let z = ziplines.iter().find(|z| Some(z.id) == id).filter(|z| z.rope)?;
        z.next.filter(|n| ziplines.iter().any(|e| e.id == *n))?;
        Some(RopeView { kind: z.kind, ends: [z.points[0], *z.points.last()?] })
    }
}

/// A zipline entity, or the zipline_end that finishes a rope. Ids are entity handles in replay.
#[derive(Clone, Debug, PartialEq)]
pub struct Zipline {
    pub id: u32,
    pub origin: V3,
    pub angles: V3,
    /// Starts a rope; an end entity only finishes one.
    pub rope: bool,
    pub next: Option<u32>,
    pub prev: Option<u32>,
    /// The rope this frame, at most 16 points from this entity to `next`, and each span's
    /// length (negative: measure it).
    pub points: Vec<V3>,
    pub spans: Vec<f32>,
    /// The map's rest shape, used by the use search while its ends sit on the entities.
    pub rest: Vec<V3>,
    pub kind: i32,
    pub enabled: bool,
    pub usable: bool,
    pub detach_end_on_use: bool,
    pub drop_to_bottom: bool,
    pub auto_detach_distance: f32,
    pub push_off: bool,
    pub preserve_velocity: bool,
    pub speed_scale: f32,
}

impl Zipline {
    /// A rope from `points[0]` (this entity) to `points.last()` (entity `next`).
    pub fn line(id: u32, points: Vec<V3>, next: u32, auto_detach_distance: f32) -> Self {
        let spans = points.windows(2).map(|w| span(w[0], w[1])).collect();
        Zipline {
            id,
            origin: points[0],
            angles: ZERO,
            rope: true,
            next: Some(next),
            prev: None,
            rest: points.clone(),
            points,
            spans,
            kind: LINE,
            enabled: true,
            usable: true,
            detach_end_on_use: false,
            drop_to_bottom: false,
            auto_detach_distance,
            push_off: false,
            preserve_velocity: false,
            speed_scale: 1.0,
        }
    }

    pub fn end(id: u32, origin: V3, prev: u32, auto_detach_distance: f32) -> Self {
        Zipline {
            id,
            origin,
            angles: ZERO,
            rope: false,
            next: None,
            prev: Some(prev),
            points: Vec::new(),
            spans: Vec::new(),
            rest: Vec::new(),
            kind: LINE,
            enabled: true,
            usable: true,
            detach_end_on_use: false,
            drop_to_bottom: false,
            auto_detach_distance,
            push_off: false,
            preserve_velocity: false,
            speed_scale: 1.0,
        }
    }

    fn vine(&self) -> bool {
        self.rope && self.kind == VINE
    }

    fn vertical(&self) -> bool {
        self.rope && self.kind == VERTICAL
    }

    /// Ropes that run out at their far end (not a vine or one whose end comes loose).
    fn fixed_ends(&self) -> bool {
        self.kind == VERTICAL || (!self.detach_end_on_use && self.kind != VINE)
    }

    fn auto_detach(&self) -> f32 {
        let d = self.auto_detach_distance;
        if 0.01 <= d { d } else { 0.01 }
    }
}

fn span(a: V3, b: V3) -> f32 {
    ((b[1] - a[1]) * (b[1] - a[1]) + (b[0] - a[0]) * (b[0] - a[0]) + (b[2] - a[2]) * (b[2] - a[2])).sqrt()
}

fn span_at(pts: &[V3], spans: &[f32], i: usize) -> f32 {
    match spans.get(i) {
        Some(&d) if d >= 0.0 => d,
        _ => span(pts[i], pts[i + 1]),
    }
}

fn rope_length(pts: &[V3], spans: &[f32]) -> f32 {
    (0..pts.len().saturating_sub(1)).fold(0.0, |l, i| l + span_at(pts, spans, i))
}

/// Where along the rope, 0 to 1 by length, the point nearest `q` lies.
pub fn rope_param(q: V3, pts: &[V3], spans: &[f32]) -> f32 {
    let total = rope_length(pts, spans);
    let (mut best, mut best_t, mut best_along) = (f32::MAX, f32::MAX, 0.0f32);
    let (mut a, mut b) = (0, 1);
    let mut along = 0.0f32;
    for i in 1..pts.len() {
        let p = pts[i - 1];
        let d = sub(pts[i], p);
        let l2 = (d[1] * d[1] + d[0] * d[0]) + d[2] * d[2];
        let t = if 1e-5 <= l2 {
            let t = (((d[1] * q[1] + d[0] * q[0]) + d[2] * q[2]) - ((d[0] * p[0] + d[1] * p[1]) + d[2] * p[2])) / l2;
            if 0.0 <= t { if 1.0 < t { 1.0 } else { t } } else { 0.0 }
        } else {
            0.0
        };
        let c = [d[0] * t + p[0], d[1] * t + p[1], d[2] * t + p[2]];
        let e = sub(q, c);
        let d2 = (e[1] * e[1] + e[0] * e[0]) + e[2] * e[2];
        if d2 <= best {
            best = d2;
            best_t = t;
            best_along = along;
            a = i - 1;
            b = i;
        }
        along += span_at(pts, spans, i - 1);
    }
    let d = sub(pts[b], pts[a]);
    let (x, y, z) = (d[0] * best_t, d[1] * best_t, d[2] * best_t);
    let r = (((y * y + x * x) + z * z).sqrt() + best_along) / total;
    let r = if r > 0.0 { r } else { 0.0 };
    if r < 1.0 { r } else { 1.0 }
}

/// One call's travel along the rope from `param`: the speed along it grows by `accel` from
/// what `vel` carries along the current span (at least 0.01), to at most `max_speed`, and
/// stops at the far end (the near end riding in reverse; 30 short of a vine's tail). Returns
/// the point reached, the velocity along the span there and the new parameter.
#[allow(clippy::too_many_arguments)]
pub fn rope_advance(reverse: bool, vine: bool, accel: f32, max_speed: f32, pts: &[V3], spans: &[f32], param: f32, vel: V3, dt: f32) -> (V3, V3, f32) {
    let n = pts.len();
    let total = rope_length(pts, spans);
    let segment = |at: f32| {
        let mut rem = at;
        for i in 0..n - 1 {
            let s = span_at(pts, spans, i);
            if s >= rem {
                return (i, i + 1, rem, s);
            }
            rem -= s;
        }
        (n - 2, n - 1, rem, -1.0)
    };
    let (a, b, _, _) = segment(total * param);
    let target = if reverse {
        0.0
    } else if vine {
        let x = total - VINE_STOP;
        (if 0.0 > x { 0.0 } else { x }) / total
    } else {
        1.0
    };
    let mut dir = unit(sub(pts[b], pts[a]));
    if target < param {
        dir = neg(dir);
    }
    let along = (dir[1] * vel[1] + dir[0] * vel[0]) + dir[2] * vel[2];
    let s = dt * accel + if 0.01 > along { 0.01 } else { along };
    let speed = if max_speed < s { max_speed } else { s };
    let step = dt * speed / total;
    let p = if param <= target {
        let x = step + param;
        if target < x { target } else { x }
    } else {
        let x = param - step;
        if target > x { target } else { x }
    };
    let total = rope_length(pts, spans);
    let (a, b, rem, s) = segment(total * p);
    let pos = if s < 0.0 {
        pts[n - 1]
    } else if 0.0 < s {
        let t = rem / s;
        let (u, v) = (pts[a], pts[b]);
        [(v[0] - u[0]) * t + u[0], (v[1] - u[1]) * t + u[1], (v[2] - u[2]) * t + u[2]]
    } else {
        pts[a]
    };
    let d = sub(pts[b], pts[a]);
    let l = ((d[0] * d[0] + d[1] * d[1]) + d[2] * d[2]).sqrt();
    let inv = 1.0 / if l > TINY { l } else { TINY };
    let mut dir = [d[0] * inv, d[1] * inv, d[2] * inv];
    if reverse {
        dir = neg(dir);
    }
    (pos, scale(dir, speed), p)
}

fn unit(d: V3) -> V3 {
    let l = ((d[1] * d[1] + d[0] * d[0]) + d[2] * d[2]).sqrt();
    let inv = 1.0 / if l > TINY { l } else { TINY };
    [inv * d[0], inv * d[1], inv * d[2]]
}

/// The squared distance from `q` to the polyline.
fn polyline_distance2(q: V3, pts: &[V3]) -> f32 {
    let e = sub(q, pts[0]);
    let mut best = (e[1] * e[1] + e[0] * e[0]) + e[2] * e[2];
    for w in pts.windows(2) {
        let (p, d) = (w[0], sub(w[1], w[0]));
        let l2 = (d[1] * d[1] + d[0] * d[0]) + d[2] * d[2];
        let t = if 1e-5 <= l2 {
            let t = (((d[1] * q[1] + d[0] * q[0]) + d[2] * q[2]) - ((d[0] * p[0] + d[1] * p[1]) + d[2] * p[2])) / l2;
            if 0.0 <= t { if 1.0 < t { 1.0 } else { t } } else { 0.0 }
        } else {
            0.0
        };
        let c = [t * d[0] + p[0], t * d[1] + p[1], t * d[2] + p[2]];
        let e = sub(c, q);
        let d2 = (e[1] * e[1] + e[0] * e[0]) + e[2] * e[2];
        if d2 < best {
            best = d2;
        }
    }
    best
}

/// Keeps the part of segment `a`..`b` on the side of the plane (normal `n` through `q`) it
/// faces; false when none of it does.
fn clip_to_plane(n: V3, q: V3, a: &mut V3, b: &mut V3) -> bool {
    let side = |p: V3| (((p[1] - q[1]) * n[1] + (p[0] - q[0]) * n[0]) + (p[2] - q[2]) * n[2]) + 0.0;
    let (da, db) = (side(*a), side(*b));
    if da >= 0.0 {
        if db < 0.0 {
            let k = 1.0 / (da - db);
            *b = std::array::from_fn(|i| (b[i] - a[i]) * da * k + a[i]);
        }
        true
    } else if db < 0.0 {
        false
    } else {
        let k = 1.0 / (db - da);
        *a = std::array::from_fn(|i| (a[i] - b[i]) * db * k + b[i]);
        true
    }
}

fn max_abs3(v: V3) -> f32 {
    let m = if v[0].abs() > v[1].abs() { v[0].abs() } else { v[1].abs() };
    if m > v[2].abs() { m } else { v[2].abs() }
}

/// Where along line `p1`..`p2` it passes closest to line `p3`..`p4`.
fn line_line(p1: V3, p2: V3, p3: V3, p4: V3) -> Option<f32> {
    let d43 = sub(p4, p3);
    let d13 = sub(p1, p3);
    if 1e-6 > max_abs3(d43) {
        return None;
    }
    let d21 = sub(p2, p1);
    if 1e-6 > max_abs3(d21) {
        return None;
    }
    let d1343 = (d43[1] * d13[1] + d43[0] * d13[0]) + d43[2] * d13[2];
    let d4321 = (d21[0] * d43[0] + d21[1] * d43[1]) + d21[2] * d43[2];
    let d4343 = (d43[1] * d43[1] + d43[0] * d43[0]) + d43[2] * d43[2];
    let d2121 = (d21[1] * d21[1] + d21[0] * d21[0]) + d21[2] * d21[2];
    let denom = d2121 * d4343 - d4321 * d4321;
    if 1e-6 > denom.abs() {
        return None;
    }
    let d1321 = (d21[1] * d13[1] + d21[0] * d13[0]) + d21[2] * d13[2];
    Some((d4321 * d1343 - d1321 * d4343) / denom)
}

/// The point of segment `a`..`b` the view ray from `p0` along `d` passes nearest, and how far
/// from the ray it is, measured from `near` along it.
fn nearest_to_ray(p0: V3, d: V3, near: f32, a: V3, b: V3) -> Option<(V3, f32)> {
    let r0 = [d[0] * near + p0[0], d[1] * near + p0[1], d[2] * near + p0[2]];
    let (mut ca, mut cb) = (a, b);
    if !clip_to_plane(d, r0, &mut ca, &mut cb) {
        return None;
    }
    let a1 = sub(ca, p0);
    let b1 = sub(cb, p0);
    let k = (d[1] * r0[1] + d[0] * r0[0]) + d[2] * r0[2];
    let pd = || (p0[1] * d[1] + p0[0] * d[0]) + p0[2] * d[2];
    let da = (d[1] * a1[1] + d[0] * a1[0]) + d[2] * a1[2];
    let ta = if da == 0.0 { 0.0 } else { (k - pd()) * (1.0 / da) };
    let db = (d[1] * b1[1] + d[0] * b1[0]) + d[2] * b1[2];
    let tb = if db == 0.0 { 0.0 } else { (k - pd()) * (1.0 / db) };
    let clamp01 = |t: f32| {
        let t = if t > 0.0 { t } else { 0.0 };
        if t < 1.0 { t } else { 1.0 }
    };
    let (ta, tb) = (clamp01(ta), clamp01(tb));
    let pa = [a1[0] * ta + p0[0], a1[1] * ta + p0[1], a1[2] * ta + p0[2]];
    let pb = [b1[0] * tb + p0[0], b1[1] * tb + p0[1], b1[2] * tb + p0[2]];
    let e = sub(pb, pa);
    let l2 = (e[1] * e[1] + e[0] * e[0]) + e[2] * e[2];
    let t = if 1e-5 > l2 {
        0.0
    } else {
        let t = (((r0[1] * e[1] + r0[0] * e[0]) + r0[2] * e[2]) - ((e[1] * pa[1] + e[0] * pa[0]) + e[2] * pa[2])) / l2;
        if 0.0 > t { 0.0 } else if 1.0 < t { 1.0 } else { t }
    };
    let p = [e[0] * t + pa[0], e[1] * t + pa[1], e[2] * t + pa[2]];
    let s = line_line(a, b, p0, p)?;
    let s = clamp01(s);
    let point = [(b[0] - a[0]) * s + a[0], (b[1] - a[1]) * s + a[1], (b[2] - a[2]) * s + a[2]];
    let f = sub(p, r0);
    Some((point, ((f[1] * f[1] + f[0] * f[0]) + f[2] * f[2]).sqrt()))
}

/// The part of segment `a`..`b` within 130 of `c`, as parameters along it.
fn within_reach(c: V3, a: V3, b: V3) -> Option<(f32, f32)> {
    let d = sub(b, a);
    let ca = sub(a, c);
    let aa = (d[1] * d[1] + d[0] * d[0]) + d[2] * d[2];
    let cc = (ca[0] * ca[0] + ca[1] * ca[1]) + ca[2] * ca[2];
    let (t0, t1) = if aa == 0.0 {
        if cc > SEGMENT_REACH2 {
            return None;
        }
        (0.0, 0.0)
    } else {
        let bb = (ca[1] * d[1] + ca[0] * d[0]) + ca[2] * d[2];
        let bb = bb + bb;
        let disc = bb * bb - (cc - SEGMENT_REACH2) * aa * 4.0;
        if !(0.0 <= disc) {
            return None;
        }
        let r = disc.sqrt();
        let (t0, t1) = ((-bb - r) * (0.5 / aa), (r - bb) * (0.5 / aa));
        if !(t0 <= 1.0 && 0.0 <= t1) {
            return None;
        }
        (t0, t1)
    };
    Some((if 0.0 <= t0 { t0 } else { 0.0 }, if t1 <= 1.0 { t1 } else { 1.0 }))
}

impl Move<'_> {
    pub(super) fn zipline(&self, id: Option<u32>) -> Option<Zipline> {
        let id = id?;
        self.tr.ziplines().iter().find(|z| z.id == id).cloned()
    }

    fn next_of(&self, z: &Zipline) -> Option<Zipline> {
        if z.rope { self.zipline(z.next) } else { None }
    }

    fn prev_of(&self, z: &Zipline) -> Option<Zipline> {
        self.zipline(z.prev)
    }

    /// The rope's shape for the use search: its rest shape while that still meets both ends.
    fn search_shape(&self, z: &Zipline) -> Vec<V3> {
        let rest_holds = !z.rest.is_empty()
            && z.rest[0] == z.origin
            && self.next_of(z).is_some_and(|e| *z.rest.last().unwrap() == e.origin);
        if rest_holds { z.rest.clone() } else { z.points.clone() }
    }

    /// Riding a zipline that runs somewhere.
    pub fn on_zipline(&self) -> bool {
        self.zipline(self.p.zipline).is_some_and(|z| self.next_of(&z).is_some())
    }

    fn on_vine(&self) -> bool {
        self.zipline(self.p.zipline).is_some_and(|z| self.next_of(&z).is_some() && z.vine())
    }

    /// Nothing about the player forbids a zipline: alive, unparented, in no other context
    /// action, and allowed.
    fn may_zipline(&self) -> bool {
        !self.p.dead && !self.p.has_parent && (self.p.context_action == CONTEXT_ZIPLINE || self.p.context_action == 0) && self.p.zipline_allowed
    }

    /// The rider hangs a standing hull's height below the rope.
    fn hang(&self) -> f32 {
        self.p.hull_max[2] - self.p.hull_min[2]
    }

    fn standing_trace(&mut self, start: V3, end: V3, skin: f32) -> Trace {
        let (mins, maxs) = (self.p.hull_min, self.p.hull_max);
        let mins = [mins[0] - skin, mins[1] - skin, mins[2] - skin];
        let maxs = [maxs[0] + skin, maxs[1] + skin, maxs[2] + skin];
        self.tr.hull(&HullQuery { start, end, mins, maxs, mask: self.p.mask })
    }

    /// A standing hull sweeps from `from` to `to` untouched.
    fn zipline_clear(&mut self, from: V3, to: V3) -> bool {
        let t = self.standing_trace(from, to, 0.0);
        !(t.fraction < 1.0 || t.allsolid || t.startsolid)
    }

    fn facing(&self) -> V3 {
        if self.p.precise_view { self.p.view_angles } else { self.p.eye_angles }
    }

    /// PlayerMove's branch for a player on a zipline.
    pub fn zipline_move(&mut self) {
        let state = self.p.zipline_state;
        let (mut origin, mut vel) = (self.mv.origin, self.mv.velocity);
        match state {
            REQUESTED => self.mount_start(&mut origin, &mut vel),
            MOUNTING => self.mounting(&mut origin, &mut vel),
            _ => self.ride(&mut origin, &mut vel),
        }
        self.mv.origin = origin;
        self.mv.velocity = vel;
        self.check_velocity();
        if state != 0 {
            self.categorize(false);
        }
    }

    /// The first call after a grab: leave any climb, stand, and start the mount from where the
    /// use found the player toward the point under the rope.
    fn mount_start(&mut self, origin: &mut V3, vel: &mut V3) {
        let z = self.zipline(self.p.zipline);
        self.end_climb();
        self.p.duck_state = 1;
        self.p.zipline_mount_time = self.root.zipline_mount_time().unwrap_or(self.s.zipline.mount_time);
        let hang = self.hang();
        self.p.zipline_state = MOUNTING;
        self.p.zipline_mount_alpha = 0.0;
        self.p.zipline_start_time = self.g.curtime;
        let u = self.p.zipline_use_point;
        self.p.zipline_target = [u[0] - 0.0, u[1] - 0.0, u[2] - hang];
        *origin = self.p.zipline_from;
        *vel = self.p.zipline_from_velocity;
        if let Some(z) = z {
            self.p.zipline_param = rope_param(*origin, &z.points, &z.spans);
        }
        if self.p.context_action == 0 {
            self.p.context_action = CONTEXT_ZIPLINE;
        }
        self.cues.push(Cue::ZiplineMount);
    }

    /// The mount: alpha runs 0 to 1 over the mount time, the target slides along the rope as a
    /// rider would from the grab, and the player closes alpha of the way to it each call.
    fn mounting(&mut self, origin: &mut V3, vel: &mut V3) {
        let z = self.zipline(self.p.zipline);
        let Some(z) = z.filter(|z| self.may_zipline() && !self.jump_off(z, vel) && z.enabled) else {
            return self.detach_zipline();
        };
        let Some(end) = self.next_of(&z) else {
            return self.detach_zipline();
        };
        let dur = self.p.zipline_mount_time;
        let a = self.g.frametime / if 0.01 > dur { 0.01 } else { dur } + self.p.zipline_mount_alpha;
        let a = if a > 0.0 { a } else { 0.0 };
        self.p.zipline_mount_alpha = if a < 1.0 { a } else { 1.0 };
        let hang = self.hang();
        let from = self.p.abs_origin;
        let d = sub(end.origin, z.origin);
        let l2 = (d[1] * d[1] + d[0] * d[0]) + d[2] * d[2];
        if 0.01 > l2 {
            return self.detach_zipline();
        }
        let reverse = self.p.zipline_reverse;
        let inv = 1.0 / l2.sqrt();
        let dir = [d[0] * inv, d[1] * inv, d[2] * inv];
        let vine = z.vine();
        let (accel, speed) = (self.s.zipline.acceleration, self.s.zipline.speed);
        let lift = if vine { hang * 0.5 } else { hang };
        let t = self.p.zipline_target;
        let param = rope_param([t[0] + 0.0, t[1] + 0.0, lift + t[2]], &z.points, &z.spans);
        let sd = if reverse { neg(dir) } else { dir };
        let v = self.p.zipline_from_velocity;
        let along = (sd[1] * v[1] + sd[0] * v[0]) + sd[2] * v[2];
        let k = self.p.zipline_mount_alpha * 0.5 * speed;
        let s = if k > along { k } else { along };
        let (pos, v1, p1) = rope_advance(reverse, vine, accel, speed, &z.points, &z.spans, param, scale(sd, s), self.g.frametime);
        if self.end_of_line(vel) {
            return self.detach_zipline();
        }
        let target = [pos[0], pos[1], pos[2] - lift];
        if self.zipline_clear(from, target) {
            self.p.zipline_target = target;
        } else if !self.zipline_clear(from, self.p.zipline_target) {
            return self.detach_zipline();
        }
        let a = self.p.zipline_mount_alpha;
        let t = self.p.zipline_target;
        self.p.zipline_param = p1;
        *vel = v1;
        *origin = [(t[0] - from[0]) * a + from[0], (t[1] - from[1]) * a + from[1], (t[2] - from[2]) * a + from[2]];
        if 1.0 <= a {
            self.p.zipline_state = RIDING;
            self.p.zipline_mount_time = 0.0;
            self.p.zipline_mount_alpha = 0.0;
            self.reset_air_counts();
            self.cues.push(Cue::ZiplineStart);
        }
    }

    /// What touching ground resets, which reaching the rope resets too.
    fn reset_air_counts(&mut self) {
        self.p.jumped_since_ground = false;
        self.p.retry_time = 0.0;
        if is_world_up(self.p.up) {
            self.p.repel_count = 0;
            self.p.same_wall = true;
        }
    }

    /// The ride: along the rope at up to `ziplineSpeed` (scaled by the zipline), the hull
    /// hanging below it, onto the next zipline of a chain, off at the end of the line.
    fn ride(&mut self, origin: &mut V3, vel: &mut V3) {
        let z = self.zipline(self.p.zipline);
        let Some(z) = z.filter(|z| self.may_zipline() && z.rope && z.enabled) else {
            return self.detach_zipline();
        };
        let end = self.next_of(&z);
        if self.jump_off(&z, vel) {
            return self.detach_zipline();
        }
        let Some(end) = end else {
            return self.detach_zipline();
        };
        let d = sub(end.origin, z.origin);
        if 0.01 > (d[1] * d[1] + d[0] * d[0]) + d[2] * d[2] {
            return self.detach_zipline();
        }
        let reverse = self.p.zipline_reverse;
        let accel = self.s.zipline.acceleration;
        let ss = z.speed_scale;
        let speed = self.s.zipline.speed * if 0.0 > ss { 0.0 } else { ss };
        let param = self.p.zipline_param;
        let hang = self.hang();
        let vine = z.vine();
        let from = *origin;
        let (target, p1, v1) = if vine && !z.drop_to_bottom {
            let lift = hang * 0.5;
            ([from[0] + 0.0 - 0.0, from[1] + 0.0 - 0.0, (lift + from[2]) - lift], param, None)
        } else {
            let (pos, v1, p1) = rope_advance(reverse, vine, accel, speed, &z.points, &z.spans, param, *vel, self.g.frametime);
            let lift = if vine { hang * 0.5 } else { hang };
            ([pos[0] - 0.0, pos[1] - 0.0, pos[2] - lift], p1, (!vine).then_some(v1))
        };
        if !self.zipline_clear(from, target) {
            if !vine {
                self.detach_zipline();
            }
            return;
        }
        self.p.zipline_param = p1;
        if let Some(v1) = v1 {
            *vel = v1;
        }
        *origin = target;
        if vine {
            return;
        }
        let (further, onto) = if reverse { (self.prev_of(&z), None) } else { (self.next_of(&end), Some(end)) };
        match further {
            None => {
                if self.end_of_line(vel) {
                    self.detach_zipline();
                }
            }
            Some(f) => {
                if p1 == if reverse { 0.0 } else { 1.0 } {
                    let to = onto.unwrap_or(f);
                    self.p.zipline = Some(to.id);
                    self.p.zipline_param = if reverse { 1.0 } else { 0.0 };
                }
            }
        }
    }

    /// Jump or crouch lets go: a crouch drops away (at most `ziplineJumpOffSpeed`); a jump
    /// leaves along the view at that speed, then jumps. On a vertical zipline either pushes
    /// off along the view, flattened.
    fn jump_off(&mut self, z: &Zipline, vel: &mut V3) -> bool {
        if self.mv.pressed() & DROP_BUTTONS == 0 {
            return false;
        }
        let (fwd, right, _) = angle_vectors(self.mv.angles);
        let damp = self.s.zipline.jump_off_damp;
        let j = self.s.zipline.jump_off_speed;
        let jump = self.mv.pressed() >> 1 & 1 != 0;
        let v0 = *vel;
        let mut v = if z.vertical() {
            let f = fwd;
            let d = (f[0] * 0.0 + f[1] * 0.0) - f[2] * 1.0;
            let f = [f[0] - d * 0.0, f[1] - d * 0.0, f[2] - d * -1.0];
            let l = ((f[1] * f[1] + f[0] * f[0]) + f[2] * f[2]).sqrt();
            let inv = 1.0 / if l > TINY { l } else { TINY };
            [f[0] * inv * j, f[1] * inv * j, f[2] * inv * j]
        } else if jump && z.vine() {
            let (fm, sm) = (self.mv.forward, self.mv.side);
            let wx = fwd[0] * fm + right[0] * sm;
            let wy = fwd[1] * fm + right[1] * sm;
            let k = (wy * wy + wx * wx).sqrt();
            let k = if k > 0.0 { k } else { 0.0 };
            let k = if k < 1.0 { k } else { 1.0 };
            let f = [fwd[0] + (wx - fwd[0]) * k, fwd[1] + (wy - fwd[1]) * k, fwd[2] + (0.0 - fwd[2]) * k];
            let s = (f[1] * v0[1] + f[0] * v0[0]) + f[2] * v0[2];
            let s = if s > j { s } else { j };
            let s = if s < j + j { s } else { j + j };
            [f[0] * s, f[1] * s, f[2] * s]
        } else {
            let base = if jump { [fwd[0] * j, fwd[1] * j, fwd[2] * j] } else { [0.0, 0.0, -j] };
            let v = [(v0[0] - base[0]) * damp + base[0], (v0[1] - base[1]) * damp + base[1], (v0[2] - base[2]) * damp + base[2]];
            let l2 = (v[0] * v[0] + v[1] * v[1]) + v[2] * v[2];
            if l2 > j * j {
                let inv = 1.0 / l2.sqrt();
                [inv * v[0] * j, inv * v[1] * j, inv * v[2] * j]
            } else {
                v
            }
        };
        if jump {
            let s = ((self.s.sv_gravity + self.s.sv_gravity) * self.s.jump_height).sqrt();
            let u = self.p.up;
            v = [v[0] + u[0] * s, v[1] + u[1] * s, v[2] + u[2] * s];
        }
        *vel = v;
        true
    }

    /// At the end of the line, or within the end's auto-detach distance of it, let go; off
    /// the top of a vertical zipline, push off along the end's facing (or the view).
    fn end_of_line(&mut self, vel: &mut V3) -> bool {
        let Some(z) = self.zipline(self.p.zipline) else { return false };
        let Some(end) = self.next_of(&z) else { return false };
        if z.vine() {
            return false;
        }
        let reverse = self.p.zipline_reverse;
        let (arrive, leave) = if reverse { (z.clone(), end) } else { (end, z.clone()) };
        let further = if reverse { self.prev_of(&arrive) } else { self.next_of(&arrive) };
        if further.is_some() {
            return false;
        }
        let p = self.p.zipline_param;
        if p != if reverse { 0.0 } else { 1.0 } {
            let eye = self.p.eye(self.s, self.p.abs_origin, self.p.view_angles, self.g.curtime);
            let o = self.p.abs_origin;
            let a = arrive.origin;
            let (e, f) = (sub(a, eye), sub(a, o));
            let to_eye = (e[1] * e[1] + e[0] * e[0]) + e[2] * e[2];
            let to_origin = (f[1] * f[1] + f[0] * f[0]) + f[2] * f[2];
            let m = if to_eye < to_origin { to_eye } else { to_origin };
            let d = arrive.auto_detach();
            if m > d * d {
                return false;
            }
        }
        if !z.vertical() || arrive.origin[2] < leave.origin[2] {
            return true;
        }
        let f = if arrive.push_off { angle_vectors(arrive.angles).0 } else { angle_vectors(self.facing()).0 };
        let d = (f[0] * 0.0 + f[1] * 0.0) - f[2] * 1.0;
        let f = [f[0] - d * 0.0, f[1] - d * 0.0, f[2] - d * -1.0];
        let l = ((f[0] * f[0] + f[1] * f[1]) + f[2] * f[2]).sqrt();
        let inv = 1.0 / if l > TINY { l } else { TINY };
        let j = self.s.zipline.jump_off_speed;
        let mut v = [f[0] * inv * j, f[1] * inv * j, f[2] * inv * j];
        let keeper = if arrive.rope { Some(arrive) } else { self.prev_of(&arrive) };
        if keeper.is_some_and(|k| k.preserve_velocity) {
            v = add(v, *vel);
        }
        *vel = v;
        true
    }

    /// Lets go of the zipline: remember it and when, back to walking, stand freely again.
    pub fn detach_zipline(&mut self) {
        if self.p.zipline_state == 0 {
            return;
        }
        self.p.last_zipline = self.p.zipline;
        self.p.zipline_detach_time = self.g.curtime;
        self.p.zipline = None;
        if self.p.zipline_state != MOUNTING && !self.p.dead {
            self.kick(DETACH_SALT, true, |r| [r[0] * 3.0 + 10.0, r[1] * 10.0 + 0.0, r[2] * 10.0 + 0.0]);
        }
        self.p.zipline_state = 0;
        if self.p.duck_state == 1 {
            self.p.duck_state = 0;
        }
        self.p.movetype = MOVETYPE_WALK;
        self.p.zipline_end_time = self.g.curtime;
        if self.p.context_action == CONTEXT_ZIPLINE {
            self.p.context_action = 0;
        }
        self.p.floor_frozen = false;
        self.p.floor_height = self.p.abs_origin[2];
        self.cues.push(Cue::ZiplineStop);
    }

    /// PreThink's use, before the movement call: a press of use, or holding it while the last
    /// press found nothing or 0.7 s after the last use, grabs the zipline the search finds.
    pub fn player_use(&mut self) {
        let held = self.mv.buttons & USE_BUTTONS;
        let pressed = self.mv.pressed() & USE_BUTTONS;
        let now = self.g.curtime;
        let repeat = held != 0 && !(now - self.p.use_time <= USE_REPEAT);
        if !(repeat || pressed != 0 || self.p.use_retry) || self.p.movetype == MOVETYPE_TRAVERSE {
            if held == 0 {
                self.p.use_retry = false;
            }
            return;
        }
        let found = self.use_target();
        match found {
            None => {
                if pressed & IN_USE != 0 {
                    self.p.use_retry = true;
                }
            }
            Some(_) => {
                if (pressed | held) & IN_USE != 0 {
                    self.try_zipline();
                }
                self.p.use_time = now;
                self.p.use_retry = false;
            }
        }
        if held == 0 {
            self.p.use_retry = false;
        }
    }

    /// The use target, when it is a zipline, and the point on it the search found: the
    /// search's pick, if its point lies anywhere but straight behind the view.
    pub(crate) fn use_target(&mut self) -> Option<(Zipline, V3)> {
        if (self.p.context_action != 0 && !self.on_vine()) || self.p.grappling {
            return None;
        }
        let facing = self.facing();
        let eye = self.p.eye(self.s, self.p.abs_origin, self.p.view_angles, self.g.curtime);
        let fwd = angle_vectors(facing).0;
        let (z, point, _, _) = self.find_zipline(fwd)?;
        let d = sub(point, eye);
        let l = ((d[1] * d[1] + d[0] * d[0]) + d[2] * d[2]).sqrt();
        let inv = 1.0 / if l > TINY { l } else { TINY };
        (-1.0 < (d[1] * inv * fwd[1] + d[0] * inv * fwd[0]) + d[2] * inv * fwd[2]).then_some((z, point))
    }

    /// Grabs the zipline the search finds: the mount starts next movement call.
    fn try_zipline(&mut self) -> bool {
        if !self.may_zipline() {
            return false;
        }
        let fwd = angle_vectors(self.facing()).0;
        let Some((z, point, reverse, from)) = self.find_zipline(fwd) else { return false };
        if Some(z.id) == self.p.zipline {
            return false;
        }
        self.detach_zipline();
        self.p.zipline = Some(z.id);
        self.p.zipline_state = REQUESTED;
        self.p.zipline_from = from;
        self.p.zipline_from_velocity = self.mv.velocity;
        self.p.zipline_use_point = point;
        self.p.zipline_reverse = reverse;
        self.p.sliding = false;
        true
    }

    /// The use search: ziplines within 120 of a point 72 above the origin, each span's part
    /// within 130 of it, and on those the point the view ray passes nearest. The grab needs a
    /// clear sweep from a step above the origin to the hull hanging there.
    fn find_zipline(&mut self, fwd: V3) -> Option<(Zipline, V3, bool, V3)> {
        if !self.may_zipline() {
            return None;
        }
        let o = self.p.abs_origin;
        let c = [o[0] + 0.0, o[1] + 0.0, o[2] + SEARCH_HEIGHT];
        let ray = [fwd[0] * RAY_START + c[0], fwd[1] * RAY_START + c[1], fwd[2] * RAY_START + c[2]];
        let hang = self.hang();
        let now = self.g.curtime;
        let mut best: Option<(f32, Zipline, V3, bool)> = None;
        for z in self.candidates(c) {
            if !(self.s.zipline.context_action_can_use || z.usable) || Some(z.id) == self.p.zipline {
                continue;
            }
            if Some(z.id) == self.p.last_zipline && !(self.s.zipline.use_cooldown < now - self.p.zipline_detach_time) {
                continue;
            }
            let mut pts = self.search_shape(&z);
            if !(polyline_distance2(c, &pts) < SEARCH_RADIUS2) {
                continue;
            }
            self.trim_ends(&z, &mut pts, hang);
            for w in pts.windows(2) {
                let (a, b) = (w[0], w[1]);
                let Some((t0, t1)) = within_reach(c, a, b) else { continue };
                let d = sub(b, a);
                let near = [d[0] * t0 + a[0], d[1] * t0 + a[1], d[2] * t0 + a[2]];
                let far = [d[0] * t1 + a[0], d[1] * t1 + a[1], d[2] * t1 + a[2]];
                let Some((point, dist)) = nearest_to_ray(ray, fwd, RAY_NEAR, near, far) else { continue };
                if best.as_ref().is_none_or(|b| dist < b.0) {
                    let reverse = self.ride_reverse(&z, point, hang);
                    best = Some((dist, z.clone(), point, reverse));
                }
            }
        }
        let (_, z, point, reverse) = best?;
        let from = [o[0], o[1], o[2] + self.p.step_height];
        let to = [point[0] - 0.0, point[1] - 0.0, point[2] - hang];
        self.zipline_clear(from, to).then_some((z, point, reverse, from))
    }

    /// Enabled ropes within the search radius of `c`: first across the floor plan, then along
    /// the rope.
    fn candidates(&self, c: V3) -> Vec<Zipline> {
        self.candidates_within(c, SEARCH_RADIUS2)
    }

    /// `0xa0f850`: enabled ropes whose floor plan passes within √`r2` of `c`, then whose
    /// polyline does.
    fn candidates_within(&self, c: V3, r2: f32) -> Vec<Zipline> {
        let mut out = Vec::new();
        for z in self.tr.ziplines().iter().filter(|z| z.rope && z.enabled) {
            let Some(end) = self.next_of(z) else { continue };
            if end.id == z.id {
                continue;
            }
            let pts = self.search_shape(z);
            let (s, e) = (pts[0], *pts.last().unwrap());
            let (dx, dy) = (e[0] - s[0], e[1] - s[1]);
            let l2 = dy * dy + dx * dx;
            let (mut px, mut py) = (s[0], s[1]);
            if 0.01 <= l2 {
                let t = if 1e-5 <= l2 {
                    let t = ((dy * c[1] + dx * c[0]) - (s[1] * dy + s[0] * dx)) / l2;
                    if 0.0 <= t { if 1.0 < t { 1.0 } else { t } } else { 0.0 }
                } else {
                    0.0
                };
                px = dx * t + s[0];
                py = dy * t + s[1];
            }
            let flat = (c[1] - py) * (c[1] - py) + (c[0] - px) * (c[0] - px);
            if flat < r2 && 0.01 <= (e[2] - s[2]) * (e[2] - s[2]) + l2 && polyline_distance2(c, &pts) < r2 {
                out.push(z.clone());
            }
        }
        out
    }

    /// Pulls each fixed end of the search's rope in to where a hull hanging from it, swept back
    /// from 200 along the rope, first touches something.
    fn trim_ends(&mut self, z: &Zipline, pts: &mut [V3], hang: f32) {
        let n = pts.len();
        let probe = |m: &mut Self, a: V3, b: V3| {
            let t = m.standing_trace(a, b, SKIN);
            (!t.startsolid && t.fraction < 1.0).then_some(t.fraction)
        };
        let p0 = pts[0];
        let d = sub(pts[1], p0);
        let l = ((d[1] * d[1] + d[0] * d[0]) + d[2] * d[2]).sqrt();
        let inv = 1.0 / if l > TINY { l } else { TINY };
        let a = [d[0] * inv * END_PROBE + p0[0] + 0.0, d[1] * inv * END_PROBE + p0[1] + 0.0, d[2] * inv * END_PROBE + p0[2] + -hang];
        let b = [p0[0] + 0.0, p0[1] + 0.0, p0[2] + -hang];
        if let Some(f) = probe(self, a, b) {
            pts[0] = [(b[0] - a[0]) * f + a[0], (b[1] - a[1]) * f + a[1], (b[2] - a[2]) * f + a[2] + hang];
        }
        if !z.fixed_ends() {
            return;
        }
        let (q, p) = (pts[n - 2], pts[n - 1]);
        let d = sub(p, q);
        let l = ((d[1] * d[1] + d[0] * d[0]) + d[2] * d[2]).sqrt();
        let inv = 1.0 / if l > TINY { l } else { TINY };
        let a = [(p[0] - inv * d[0] * END_PROBE) + 0.0, (p[1] - inv * d[1] * END_PROBE) + 0.0, (p[2] - inv * d[2] * END_PROBE) + -hang];
        let b = [p[0] + 0.0, p[1] + 0.0, p[2] + -hang];
        if let Some(f) = probe(self, a, b) {
            pts[n - 1] = [(b[0] - a[0]) * f + a[0], (b[1] - a[1]) * f + a[1], (b[2] - a[2]) * f + a[2] + hang];
        }
    }

    /// Which way a grab at `point` rides: away from where the view faces along the rope, and
    /// never into an end it would leave at once.
    fn ride_reverse(&self, z: &Zipline, point: V3, hang: f32) -> bool {
        let Some(end) = self.next_of(z) else { return false };
        let d = sub(end.origin, z.origin);
        let inv = 1.0 / ((d[1] * d[1] + d[0] * d[0]) + d[2] * d[2]).sqrt();
        let dz = inv * d[2];
        let f = angle_vectors(self.facing()).0;
        let mut reverse = false;
        if z.kind == VERTICAL {
            let dot = (inv * d[0] * f[0] + inv * d[1] * f[1]) + f[2] * dz;
            reverse = if dz <= 0.0 { !(dot >= 0.5) } else { !(dot >= -0.5) };
        } else if !z.detach_end_on_use && z.kind != VINE {
            let dot = (inv * d[0] * f[0] + inv * d[1] * f[1]) + f[2] * dz;
            reverse = !(dot >= 0.0);
        }
        let pts = self.search_shape(z);
        let near = |p: V3| {
            let at = |q: V3| (q[1] - p[1]) * (q[1] - p[1]) + (q[0] - p[0]) * (q[0] - p[0]) + (q[2] - p[2]) * (q[2] - p[2]);
            let m = at([point[0], point[1], point[2]]);
            let up = at([point[0] + 0.0, point[1] + 0.0, point[2] + hang]);
            let m = if up <= m { up } else { m };
            let down = at([point[0] - 0.0, point[1] - 0.0, point[2] - hang]);
            if down <= m { down } else { m }
        };
        if !reverse {
            if self.next_of(&end).is_none() && z.fixed_ends() {
                let d = end.auto_detach();
                if near(*pts.last().unwrap()) <= d * d {
                    reverse = true;
                }
            }
        } else if self.prev_of(z).is_none() {
            let d = z.auto_detach();
            if near(pts[0]) <= d * d {
                reverse = false;
            }
        }
        reverse
    }
}

/// A hook's ray catches a zipline passing within 30 of it (`0xa12820`), searching ropes within
/// that much of the ray's half length from its middle. A span reaching within 10 of the eye is
/// cut to its part inside that sphere.
const HOOK_SEARCH_PAD: f32 = 30.0;
const HOOK_CATCH2: f32 = 900.0;
const HOOK_EYE2: f32 = 100.0;
const MASK_HOOK: u32 = 0x4640_400b;
/// The grab from the grapple (`0xa13ab0`): the hook within 120 of the eye or the origin.
const GRAPPLE_GRAB2: f32 = 14400.0;

/// The part of span `a`..`b` a hook's ray is tested against: all of it, unless it reaches
/// within 10 of the eye. Reaching in over an end leaves the part inside; passing through
/// leaves nothing.
fn eye_cut(eye: V3, a: V3, b: V3) -> Option<(V3, V3)> {
    let (dx, dy, dz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
    let (fx, fy, fz) = (a[0] - eye[0], a[1] - eye[1], a[2] - eye[2]);
    let dd = dy * dy + dx * dx + dz * dz;
    let ff = fy * fy + fx * fx + fz * fz;
    let (t0, t1) = if dd != 0.0 {
        let bq = fy * dy + fx * dx + fz * dz;
        let bq = bq + bq;
        let disc = bq * bq - (ff - HOOK_EYE2) * dd * 4.0;
        if disc < 0.0 {
            return Some((a, b));
        }
        let r = disc.sqrt();
        let (t0, t1) = ((-bq - r) * (0.5 / dd), (r - bq) * (0.5 / dd));
        if 1.0 < t0 || t1 < 0.0 {
            return Some((a, b));
        }
        let t0 = if t0 < 0.0 { 0.0 } else { t0 };
        let t1 = if 1.0 < t1 { 1.0 } else { t1 };
        if !(t0 <= 0.0 || 1.0 <= t1) {
            return None;
        }
        (t0, t1)
    } else {
        if !(ff <= HOOK_EYE2) {
            return Some((a, b));
        }
        (0.0, 0.0)
    };
    Some(([t0 * dx + a[0], a[1] + t0 * dy, t0 * dz + a[2]], [t1 * dx + a[0], t1 * dy + a[1], a[2] + t1 * dz]))
}

impl Move<'_> {
    /// `0xa12820`: the hook's ray `t` (from its start to where it stopped) catches a zipline it
    /// passes within 30 of, where a ray between the two is clear; it then ends at its point
    /// nearest the rope, on contents 0. Not the zipline ridden, nor the last one within
    /// `grapple_lastZiplineCooldown` of letting go.
    pub(super) fn zipline_on_ray(&mut self, t: &mut Trace) -> Option<u32> {
        let (s0, e0) = (t.start, t.endpos);
        let h = [(e0[0] - s0[0]) * 0.5, (e0[1] - s0[1]) * 0.5, (e0[2] - s0[2]) * 0.5];
        let c = [s0[0] + h[0], s0[1] + h[1], s0[2] + h[2]];
        let r = (h[1] * h[1] + h[0] * h[0] + h[2] * h[2]).sqrt() + HOOK_SEARCH_PAD;
        let ridden = self.p.zipline;
        let since = self.g.curtime - self.p.zipline_detach_time;
        let last = self.p.last_zipline.filter(|_| !(self.s.grapple.last_zipline_cooldown < since));
        let eye = self.grapple_eye();
        let mut caught = None;
        for z in self.candidates_within(c, r * r) {
            if Some(z.id) == ridden || Some(z.id) == last {
                continue;
            }
            for w in z.points.windows(2) {
                let Some((a, b)) = eye_cut(eye, w[0], w[1]) else { continue };
                if self.hook_ray_meets(t, a, b) {
                    caught = Some(z.id);
                }
            }
        }
        caught
    }

    /// One span against the hook's ray: where they pass within 30 of each other and a ray
    /// between those points is clear, the hook's ray ends there.
    fn hook_ray_meets(&mut self, t: &mut Trace, a: V3, b: V3) -> bool {
        let (s0, e0) = (t.start, t.endpos);
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let clear = |m: &mut Self, from: V3, to: V3| {
            let r = m.tr.hull(&HullQuery { start: from, end: to, mins: ZERO, maxs: ZERO, mask: MASK_HOOK });
            !(r.fraction < 1.0 || r.allsolid || r.startsolid)
        };
        let Some((ta, tb)) = super::grapple::closest_on_lines(s0, e0, a, b) else {
            let l2 = d[1] * d[1] + d[0] * d[0] + d[2] * d[2];
            let u = if 1e-5 <= l2 {
                let u = ((s0[1] * d[1] + s0[0] * d[0] + s0[2] * d[2]) - (a[1] * d[1] + a[0] * d[0] + a[2] * d[2])) / l2;
                if 0.0 <= u { if u <= 1.0 { u } else { 1.0 } } else { 0.0 }
            } else {
                0.0
            };
            let q = [u * d[0] + a[0], u * d[1] + a[1], u * d[2] + a[2]];
            let (gx, gy, gz) = (s0[0] - q[0], s0[1] - q[1], s0[2] - q[2]);
            if !(gy * gy + gx * gx + gz * gz < HOOK_CATCH2) || !clear(self, s0, q) {
                return false;
            }
            let l = (gy * gy + gx * gx + gz * gz).sqrt();
            let inv = 1.0 / if l <= TINY { TINY } else { l };
            t.endpos = s0;
            t.normal = [inv * gx, inv * gy, inv * gz];
            t.fraction = 0.0;
            t.contents = 0;
            t.surface = 0;
            t.flag3b = false;
            return true;
        };
        let ta = if ta <= 0.0 { 0.0 } else if 1.0 <= ta { 1.0 } else { ta };
        let tb = if tb <= 0.0 { 0.0 } else if 1.0 <= tb { 1.0 } else { tb };
        let p = [(e0[0] - s0[0]) * ta + s0[0], (e0[1] - s0[1]) * ta + s0[1], (e0[2] - s0[2]) * ta + s0[2]];
        let q = [d[0] * tb + a[0], d[1] * tb + a[1], d[2] * tb + a[2]];
        let (qx, qy, qz) = (q[0] - p[0], q[1] - p[1], q[2] - p[2]);
        let dd2 = qy * qy + qx * qx + qz * qz;
        if !(dd2 < HOOK_CATCH2) {
            return false;
        }
        let (sx, sy, sz) = (e0[0] - s0[0], e0[1] - s0[1], e0[2] - s0[2]);
        let l2 = sy * sy + sx * sx + sz * sz;
        if !(0.01 < l2) || !clear(self, p, q) {
            return false;
        }
        let l = l2.sqrt();
        let n = if dd2 <= 0.01 {
            let inv = 1.0 / l;
            [sx * inv, sy * inv, sz * inv]
        } else {
            let inv = 1.0 / dd2.sqrt();
            [qx * inv, qy * inv, inv * qz]
        };
        let (px, py, pz) = (p[0] - s0[0], p[1] - s0[1], p[2] - s0[2]);
        t.fraction = ((py * py + px * px + pz * pz).sqrt() * t.fraction) / l;
        t.endpos = p;
        t.normal = [-n[0], -n[1], -n[2]];
        t.contents = 0;
        t.surface = 0;
        t.flag3b = false;
        true
    }

    /// The point of zipline `id`'s rope, its fixed ends pulled in, nearest `at`: where a hook
    /// that caught it holds (`0x9d1b00`).
    pub(super) fn zipline_nearest(&mut self, id: u32, at: V3) -> V3 {
        let Some(z) = self.zipline(Some(id)) else { return at };
        let mut pts = z.points.clone();
        let hang = self.hang();
        self.trim_ends(&z, &mut pts, hang);
        let mut best = f32::MAX;
        let mut q = ZERO;
        for w in pts.windows(2) {
            let (p, d) = (w[0], sub(w[1], w[0]));
            let l2 = d[1] * d[1] + d[0] * d[0] + d[2] * d[2];
            let u = if 1e-5 <= l2 {
                let u = ((at[1] * d[1] + at[0] * d[0] + at[2] * d[2]) - (d[0] * p[0] + d[1] * p[1] + d[2] * p[2])) / l2;
                if 0.0 <= u { if u <= 1.0 { u } else { 1.0 } } else { 0.0 }
            } else {
                0.0
            };
            let c = [d[0] * u + p[0], d[1] * u + p[1], d[2] * u + p[2]];
            let (ex, ez) = (at[0] - c[0], at[2] - c[2]);
            let d2 = (at[1] - c[1]) * (at[1] - c[1]) + ex * ex + ez * ez;
            if d2 < best {
                best = d2;
                q = c;
            }
        }
        q
    }

    /// `0xa13ab0` from the grapple: once the hook holds a zipline and the eye or the origin is
    /// within 120 of it, with a clear standing hull from the origin to hanging below it, the
    /// zipline is grabbed there and the grapple lets go. The mount starts next movement call.
    pub(super) fn grapple_zipline_grab(&mut self) -> bool {
        if !self.may_zipline() || !self.p.grappling {
            return false;
        }
        let Some(z) = self.zipline(self.p.grapple_zipline) else { return false };
        let eye = self.grapple_eye();
        let hook = self.p.grapple_hook_at;
        let (ex, ey, ez) = (hook[0] - eye[0], hook[1] - eye[1], hook[2] - eye[2]);
        let to_eye = ey * ey + ex * ex + ez * ez;
        let o = self.p.abs_origin;
        let oz = hook[2] - o[2];
        let to_origin = (hook[1] - o[1]) * (hook[1] - o[1]) + (hook[0] - o[0]) * (hook[0] - o[0]) + oz * oz;
        let near = if to_origin <= to_eye { to_origin } else { to_eye };
        if GRAPPLE_GRAB2 < near {
            return false;
        }
        let hang = self.hang();
        let to = [hook[0] - 0.0, hook[1] - 0.0, hook[2] - hang];
        if !self.zipline_clear(o, to) {
            return false;
        }
        let reverse = self.ride_reverse(&z, hook, hang);
        if Some(z.id) == self.p.zipline {
            return false;
        }
        self.grapple_drop();
        self.detach_zipline();
        self.p.zipline = Some(z.id);
        self.p.zipline_state = REQUESTED;
        self.p.zipline_from = o;
        self.p.zipline_from_velocity = self.mv.velocity;
        self.p.zipline_use_point = hook;
        self.p.zipline_reverse = reverse;
        self.p.sliding = false;
        true
    }
}
