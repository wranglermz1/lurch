//! Mantling: the traversal movetype. A walking or climbing player who pushes into a ledge
//! within reach is lifted onto it along a path set up when the mantle starts, and leaves it
//! at the top, by letting go, or by jumping off.
//!
//! The image holds all of it but the in-between position: with `traversal_anim 1` each call
//! places the player by the first-person model's traversal sequence, whose root motion lives
//! in the model files. Every exit puts the player back on the procedural path (begin, mid,
//! end), so the outcome does not depend on the animation; `RootMotion` supplies the part that
//! does.

use super::buttons::{IN_DUCK, IN_JUMP, IN_TOGGLE_DUCK};
use super::math::*;
use super::{HullQuery, Move, Player, Trace};

pub const MOVETYPE_TRAVERSE: u8 = 12;
const MOVETYPE_WALK: u8 = 2;
const NO_MANTLE_SURFACE: u32 = 0x10_0000;
const BLEND_OUT: f32 = 0.2;
pub const REST: i32 = 11;
pub const JUMP_OFF: i32 = 12;
const HAND_BOX: V3 = [0.0, 0.0, 8.0];

/// Player settings and ConVars the mantle reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mantle {
    pub enabled: bool,
    pub height: f32,
    /// mantleDurationBelow, Level, Above, High: by where the eye starts against the ledge.
    pub duration: [f32; 4],
    pub jump_up_speed: f32,
    pub jump_outward_speed: f32,
    pub forward_dist: f32,
    pub min_dist: f32,
    pub search_dist: f32,
    pub height_below: f32,
    pub height_level: f32,
    pub height_above: f32,
    pub rest_frac: f32,
    pub rest_frac_below: f32,
    pub jumpoff_duration: f32,
    pub jumpoff_anim_maxfrac: f32,
    pub backoff_anim_maxfrac: f32,
    pub dangle_required_space: f32,
    pub cooldown: f32,
    pub traversal_cooldown: f32,
    pub hand_required_width: f32,
    pub grapple_auto_mantle: f32,
    /// `automantle_gun_enable_height`: while the eye is further than this below the ledge
    /// top's share of the path, the gun stays up and holds the first-person hands back.
    pub gun_enable_height: f32,
}

/// What the movement takes from animations, which live in model files: the traversal
/// animation's root, placed at the ledge, for a traversal type at a cycle (`None` follows the
/// procedural path, as `traversal_anim 0` would); and the zipline mount's length when the
/// viewmodel has a mount sequence (`None`: `mountZiplineTime`).
pub trait RootMotion {
    fn place(&self, kind: i32, cycle: f32, ledge: V3, forward: V3) -> Option<V3>;

    fn zipline_mount_time(&self) -> Option<f32> {
        None
    }
}

pub struct Procedural;

impl RootMotion for Procedural {
    fn place(&self, _: i32, _: f32, _: V3, _: V3) -> Option<V3> {
        None
    }
}

/// The root motion recordings show (`super::rootmotion`), for the traversal types recorded so
/// far; other types follow the procedural path.
pub struct Recovered;

impl RootMotion for Recovered {
    fn place(&self, kind: i32, cycle: f32, ledge: V3, forward: V3) -> Option<V3> {
        let rows = super::rootmotion::CURVES.iter().find(|(k, _)| *k == kind)?.1;
        let i = rows.partition_point(|r| r[0] < cycle);
        let l = match i {
            0 => [rows[0][1], rows[0][2], rows[0][3]],
            i if i == rows.len() => [rows[i - 1][1], rows[i - 1][2], rows[i - 1][3]],
            i => {
                let (a, b) = (rows[i - 1], rows[i]);
                let t = (cycle - a[0]) / (b[0] - a[0]);
                [a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t]
            }
        };
        let left = [-forward[1], forward[0], 0.0];
        Some([
            ledge[0] + forward[0] * l[0] + left[0] * l[1],
            ledge[1] + forward[1] * l[0] + left[1] * l[1],
            ledge[2] + forward[2] * l[0] + l[2],
        ])
    }
}

pub static RECOVERED: Recovered = Recovered;

/// An entity's coordinate frame, 3x4. Only the world carries a player through a mantle here,
/// and its frame is the identity.
#[derive(Clone, Copy, Debug)]
pub struct Frame(pub [[f32; 4]; 3]);

pub const WORLD_FRAME: Frame = Frame([[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]]);

impl Frame {
    fn point(&self, p: V3) -> V3 {
        let m = &self.0;
        [
            ((p[1] * m[0][1] + p[0] * m[0][0]) + p[2] * m[0][2]) + m[0][3],
            ((m[1][0] * p[0] + m[1][1] * p[1]) + m[1][2] * p[2]) + m[1][3],
            ((m[2][0] * p[0] + m[2][1] * p[1]) + m[2][2] * p[2]) + m[2][3],
        ]
    }

    fn dir(&self, d: V3) -> V3 {
        let m = &self.0;
        [
            (m[0][1] * d[1] + d[0] * m[0][0]) + m[0][2] * d[2],
            (m[1][1] * d[1] + d[0] * m[1][0]) + m[1][2] * d[2],
            (m[2][1] * d[1] + d[0] * m[2][0]) + m[2][2] * d[2],
        ]
    }

    /// MatrixInvert for a rigid frame: the rotation transposed, the translation taken back.
    fn invert(&self) -> Frame {
        let m = &self.0;
        let mut r = [[m[0][0], m[1][0], m[2][0], 0.0], [m[0][1], m[1][1], m[2][1], 0.0], [m[0][2], m[1][2], m[2][2], 0.0]];
        let (t0, t1, t2) = (m[0][3], m[1][3], m[2][3]);
        r[0][3] = -((t0 * r[0][0] + t1 * r[0][1]) + t2 * r[0][2]);
        r[1][3] = -((t1 * r[1][1] + t0 * r[1][0]) + t2 * r[1][2]);
        r[2][3] = -((t1 * r[2][1] + t0 * r[2][0]) + t2 * r[2][2]);
        Frame(r)
    }

    fn local(&self, p: V3) -> V3 {
        let m = &self.0;
        [
            ((m[0][0] * p[0] + p[1] * m[0][1]) + p[2] * m[0][2]) + m[0][3],
            ((m[1][0] * p[0] + m[1][1] * p[1]) + m[1][2] * p[2]) + m[1][3],
            ((m[2][0] * p[0] + m[2][1] * p[1]) + m[2][2] * p[2]) + m[2][3],
        ]
    }

    fn local_dir(&self, d: V3) -> V3 {
        let m = &self.0;
        [
            (m[0][1] * d[1] + m[0][0] * d[0]) + m[0][2] * d[2],
            (m[1][0] * d[0] + m[1][1] * d[1]) + m[1][2] * d[2],
            (m[2][0] * d[0] + m[2][1] * d[1]) + m[2][2] * d[2],
        ]
    }
}

fn round_32nd(v: f32) -> f32 {
    ((v * 32.0).floor() as i32) as f32 * 0.03125
}

/// Where the procedural path puts a player `progress` of the way through: up from begin to
/// mid, then over to end.
pub fn traversal_pos(p: &Player, frame: &Frame) -> V3 {
    let (t, mf) = (p.traversal_progress, p.traversal_mid_frac);
    let (a, b, k) = if mf <= t {
        (p.traversal_mid, p.traversal_end, (t - mf) / (1.0 - mf))
    } else {
        (p.traversal_begin, p.traversal_mid, t / mf)
    };
    let (a, b) = (frame.point(a), frame.point(b));
    [(b[0] - a[0]) * k + a[0], (b[1] - a[1]) * k + a[1], (b[2] - a[2]) * k + a[2]]
}

/// Which way the ledge faces the player, unit length along the horizontal.
fn flat_normal(n: V3) -> V3 {
    let l = (n[0] * n[0] + n[1] * n[1]) + 0.0;
    let l = l.sqrt();
    let k = 1.0 / if l > TINY { l } else { TINY };
    [n[0] * k, n[1] * k, k * 0.0]
}

impl Move<'_> {
    fn trace_ducked(&mut self, start: V3, end: V3) -> Trace {
        let (state, flags) = (self.p.traversal_state, self.p.flags);
        self.p.traversal_state = 2;
        self.p.flags |= 2;
        let t = self.trace(start, end);
        self.p.flags = flags;
        self.p.traversal_state = state;
        t
    }

    fn trace_box(&mut self, start: V3, end: V3, maxs: V3) -> Trace {
        self.tr.hull(&HullQuery { start, end, mins: ZERO, maxs, mask: self.p.mask })
    }

    /// PlayerMove's check before the walk move: start a traversal instead.
    pub fn try_traversal(&mut self) -> bool {
        let until = self.p.traversal_release_time + self.s.mantle.traversal_cooldown;
        if self.p.movetype == MOVETYPE_TRAVERSE || self.g.curtime < until {
            return false;
        }
        self.automantle()
    }

    /// 0x811650: look for a ledge ahead along the view's yaw and start a mantle onto it.
    fn automantle(&mut self) -> bool {
        let m = self.s.mantle.clone();
        if !m.enabled || self.p.mantle_blocked {
            return false;
        }
        let jump = self.mv.buttons & IN_JUMP != 0;
        if self.on_ground() && is_world_up(self.p.up) && !jump {
            return false;
        }
        let now = self.g.curtime;
        if 0.25 > now - self.p.last_ground_time && !jump {
            return false;
        }
        if (self.p.grappling && self.p.grapple_count > 0) || self.p.melee_active || m.cooldown > now - self.p.traversal_start_time {
            return false;
        }
        let o = self.mv.origin;
        let r = deg2rad(self.mv.angles[1]);
        let (sin, cos) = (engine_sin(r + 0.0), engine_cos(r));
        let since = now - self.p.grapple_time;
        let since = if 0.0 > since { 0.0 } else { since };
        let wants = self.mv.forward >= 0.3 || m.grapple_auto_mantle > since;
        let v = self.mv.velocity;
        let ahead = (sin * v[1] + cos * v[0]) + 0.0 * v[2];
        if !wants && is_world_up(self.p.up) && !(0.01 < ahead) {
            return false;
        }
        let closing = (if ahead > 0.0 { ahead } else { 0.0 }) * self.g.frametime;
        let search = m.search_dist - 0.5;
        let dist = if 0.5 > search { 0.5 } else { search };
        let dist = if dist > closing { dist } else { closing };
        let mut end = [cos * dist + o[0], sin * dist + o[1], 0.0 * dist + o[2]];
        let first = self.trace(o, end);
        let up = self.mv.jump_dir;
        let n = first.normal;
        let nz = (n[1] * up[1] + n[0] * up[0]) + n[2] * up[2];
        let mut wall = first;
        if nz > 0.7 {
            end[2] -= ((n[1] * sin + n[0] * cos) + n[2] * 0.0) / n[2] * dist;
            wall = self.trace(o, end);
        }
        if wall.fraction >= 1.0 || nz >= 0.7 || wall.contents & NO_MANTLE_SURFACE != 0 {
            return false;
        }
        let Some(found) = self.find_ledge([cos, sin, 0.0], o, 0.0, &wall, true) else { return false };
        if !found.hang && !wants {
            return false;
        }
        self.mantle_onto(o, &found)
    }

    /// The rest of 0x811650 once a ledge is found: the height to rise to, the path, and the
    /// traversal's start.
    fn mantle_onto(&mut self, o: V3, found: &Ledge) -> bool {
        let m = self.s.mantle.clone();
        let h = m.height;
        let (ox, oy, oz) = (o[0], o[1], o[2]);
        let n = found.normal;
        let ledge = found.top;
        let mut z = oz;
        let mut cur = ledge;
        let mut i = 0;
        'rise: loop {
            let (cn, ce) = (cur.normal, cur.endpos);
            let mut lift = cn[2] < 0.7;
            if !lift {
                let p = ((((ce[1] - oy) * cn[1] + (ce[0] - ox) * cn[0]) + (ce[2] - z) * cn[2]) / cn[2] + z) + 0.015625;
                let d = p - ce[2];
                if 6.0 > d {
                    z = p;
                    if !(0.0 < d) {
                        z = ce[2];
                        if cur.fraction == 0.0 && i > 0 {
                            z = ledge.start[2];
                            break 'rise;
                        }
                    }
                    if z > ledge.start[2] {
                        z = ledge.start[2];
                        break 'rise;
                    }
                } else {
                    lift = true;
                }
            }
            if lift {
                z = ce[2] + 4.0;
                if z > ledge.start[2] {
                    z = ledge.start[2];
                    break 'rise;
                }
                cur.endpos[2] = z;
                cur.normal = [0.0, 0.0, 1.0];
            }
            cur = self.trace_ducked(cur.endpos, [ox, oy, z]);
            if cur.fraction == 1.0 {
                break 'rise;
            }
            i += 1;
            if i >= 1000 {
                return false;
            }
        }
        let mut at = o;
        if self.p.traversal_state != 2 {
            let k = found.reach * 0.99;
            let t = self.trace([ox, oy, z], [ox - k * n[0], oy - k * n[1], z - k * n[2]]);
            if t.startsolid || 1.0 > t.fraction {
                self.mv.origin = o;
                self.force_duck();
                at = self.mv.origin;
            }
        }
        let begin_z = (z as f64 - (h as f64 + 0.03125)) as f32;
        let begin = [ox, oy, begin_z];
        let mut origin = at;
        if begin_z > at[2] {
            origin = [ox, oy, begin_z];
        }
        let ln = ledge.normal;
        let le = ledge.endpos;
        let mut top = z;
        if (ln[1] * n[1] + ln[0] * n[0]) + ln[2] * n[2] > 0.0 {
            top = (((le[1] - oy) * ln[1] + (le[0] - ox) * ln[0]) + (le[2] - z) * ln[2]) / ln[2] + z;
        }
        let top = top - self.mv.hull_half_width * 0.5;
        let sz = if top > origin[2] { top } else { origin[2] };
        let d = found.reach;
        let t = self.trace([ox, oy, sz], [ox - n[0] * d, oy - n[1] * d, sz - n[2] * d]);
        let room = t.fraction * d;
        let limit = d - m.min_dist;
        let room = if room < limit { room } else { limit };
        let k = room + self.mv.hull_half_width;
        let probe = [ox - n[0] * k, oy - n[1] * k, z - n[2] * k];
        let mid = [ox, oy, z];
        let t = self.trace(mid, probe);
        let f = t.fraction;
        let end = [(probe[0] - ox) * f + ox, (probe[1] - oy) * f + oy, (probe[2] - z) * f + z];
        self.p.traversal_ref = [ox - n[0] * k, oy - n[1] * k, z - n[2] * k];
        self.place_hands(n);
        let mid_frac = h / (k + h);
        if m.dangle_required_space > f * h || !found.hang {
            self.p.traversal_hang = false;
        } else {
            let r = self.p.traversal_ref;
            let a = ((oy - r[1]) * n[1] + (ox - r[0]) * n[0]) + (z - r[2]) * n[2];
            if a > (room + self.mv.hull_half_width) + 4.0 {
                self.p.traversal_hang = false;
            }
        }
        let forward = [-n[0], -n[1], -n[2]];
        self.start_traversal(forward, ledge.entity, begin, mid, end, mid_frac, origin);
        let p = (origin[2] - begin_z) / h;
        let p = if 1.0 < p { 1.0 } else { p };
        self.p.traversal_progress = p * mid_frac;
        self.p.traversal_cycle = 0.0;
        let t = self.trace(mid, begin);
        self.p.traversal_rest = (1.0 - t.fraction) * mid_frac;
        true
    }

    /// 0x80f360: whether there is a ledge above the wall `wall` hit moving along `view`, and
    /// where. `None` covers both "not here" and "not this call".
    fn find_ledge(&mut self, view: V3, o: V3, extra: f32, wall: &Trace, full: bool) -> Option<Ledge> {
        let m = self.s.mantle.clone();
        let n = flat_normal(wall.normal);
        let we = wall.endpos;
        let d0 = ((o[1] - we[1]) * n[1] + (o[0] - we[0]) * n[0]) + (o[2] - we[2]) * n[2];
        if !self.reaching_for_ledge(view, n) {
            return None;
        }
        let fd = m.forward_dist;
        let reach = fd + d0;
        let total = (if fd > m.min_dist { fd } else { m.min_dist }) + d0;
        let h = extra + m.height;
        let into = [n[0] * -total, n[1] * -total, n[2] * -total];
        let hh = h as f64 + 0.03125;
        let k = h / 80.0;
        let ratio = reach / total;
        let probe = match (self.mv.command_number as i32) % 7 {
            0 | 4 => hh as f32,
            1 => k * 66.5,
            2 => k * 26.5,
            3 => k * 16.5,
            5 => k * 34.5,
            _ => return None,
        };
        let pz = probe + o[2];
        let start = [o[0], o[1], pz];
        let end = [o[0] + into[0], o[1] + into[1], pz + into[2]];
        let t = self.trace_ducked(start, end);
        let reached = t.fraction * total;
        if m.min_dist > reached - d0 {
            return None;
        }
        let ratio = if ratio < t.fraction { ratio } else { t.fraction };
        let a = [(end[0] - o[0]) * ratio + o[0], (end[1] - o[1]) * ratio + o[1], (end[2] - pz) * ratio + pz];
        let top = self.trace_ducked(a, [a[0], a[1], (a[2] as f64 - hh) as f32]);
        let up = self.mv.jump_dir;
        if !(0.7 <= (up[1] * top.normal[1] + up[0] * top.normal[0]) + up[2] * top.normal[2]) {
            return None;
        }
        let rise = top.endpos[2] - o[2];
        if 0.03125 > rise || top.contents & NO_MANTLE_SURFACE != 0 {
            return None;
        }
        let head = self.trace_ducked(o, start);
        if 1.0 > head.fraction {
            return None;
        }
        let mut found = Ledge { top, normal: n, reach: 0.0, hang: false };
        if !full {
            return Some(found);
        }
        let v = self.mv.velocity;
        let closing = -10.0 > (v[1] * n[1] + v[0] * n[0]) + v[2] * n[2];
        if self.p.step_height as f64 + 0.03125 > rise as f64 && is_world_up(self.p.up) && closing {
            if self.p.traversal_state == 2 {
                return None;
            }
            let e = top.endpos;
            if !self.trace(e, e).startsolid {
                return None;
            }
        }
        if reached + reached > rise && closing {
            let lift = [o[0], o[1], o[2] + rise * 0.5];
            let end = [o[0] - (reached * n[0]) * 0.35, o[1] - (reached * n[1]) * 0.35, lift[2] - (reached * n[2]) * 0.35];
            let t = self.trace_ducked(lift, end);
            if t.fraction == 1.0 {
                let wn = wall.normal;
                let a = ((up[1] * wn[1] + up[0] * wn[0]) + up[2] * wn[2]) as f64;
                let b = ((t.normal[0] * up[0] + t.normal[1] * up[1]) + t.normal[2] * up[2]) as f64;
                if (a.abs() as f32 > 0.1) == (b.abs() as f32 > 0.1) {
                    return None;
                }
            }
        }
        let hw2 = self.mv.hull_half_width + self.mv.hull_half_width;
        let sz = top.endpos[2] - (m.dangle_required_space - 5.0);
        let s = [o[0], o[1], sz];
        let below = self.trace_box(s, [o[0] - hw2 * n[0], o[1] - hw2 * n[1], sz - hw2 * n[2]], ZERO);
        found.hang = 1.0 > below.fraction;
        found.reach = if reached < reach { reached } else { reach };
        Some(found)
    }

    /// 0x812770: put the hands on the ledge. Probes either side of the reference point; if
    /// only one side finds the ledge, slides the point over to where there is room for a
    /// hand, then fans line traces down and back over the edge to find where wall meets top.
    /// Moves `traversal_ref` there and sets `traversal_hang` when the hands fit.
    fn place_hands(&mut self, n: V3) {
        let hw = self.mv.hull_half_width;
        let rw = self.s.mantle.hand_required_width;
        let r = self.p.traversal_ref;
        self.p.traversal_hang = false;
        let mut ok = true;
        let up = [0.0f32, 0.0, 1.0];
        let a = [r[0] + n[0] * hw, r[1] + n[1] * hw, (r[2] + n[2] * hw) + hw];
        let b = [r[0] - n[0] * hw, r[1] - n[1] * hw, (r[2] - n[2] * hw) - hw * 1.5];
        let side = [-n[1], n[0], 0.0];
        let w = hw - 0.1;
        let signs = [-1.0f32, 1.0];
        let mut starts = [ZERO; 2];
        let mut ends = [ZERO; 2];
        let mut fr = [0.0f32; 2];
        let (mut hits, mut idx, mut last) = (0, 0usize, 0.0f32);
        for i in 0..2 {
            let k = w * signs[i];
            let s = [side[0] * k, side[1] * k, side[2] * k];
            starts[i] = [a[0] + s[0], a[1] + s[1], a[2] + s[2]];
            ends[i] = [b[0] + s[0], b[1] + s[1], b[2] + s[2]];
            let t = self.trace_box(starts[i], ends[i], HAND_BOX);
            fr[i] = t.fraction;
            if 1.0 > t.fraction {
                last = t.fraction;
                idx = i;
                hits += 1;
            }
        }
        if hits == 2 {
            if fr[1] - 0.3 > fr[0] {
                (idx, last, hits) = (0, fr[0], 1);
            } else if fr[0] - 0.3 > fr[1] {
                (idx, last, hits) = (1, fr[1], 1);
            }
        }
        let mut shifted = r;
        if hits == 1 {
            let t1 = last + 0.1;
            let t1 = if t1 < 1.0 { t1 } else { 1.0 };
            let o = idx ^ 1;
            let dx = ends[o][0] - starts[o][0];
            let dy = ends[o][1] - starts[o][1];
            let dz = ends[o][2] - starts[o][2];
            let p = [dx * t1 + starts[o][0], dy * t1 + starts[o][1], dz * t1 + starts[o][2]];
            let q = [
                (ends[idx][0] - starts[idx][0]) * t1 + starts[idx][0],
                (ends[idx][1] - starts[idx][1]) * t1 + starts[idx][1],
                (ends[idx][2] - starts[idx][2]) * t1 + starts[idx][2],
            ];
            let across = self.trace_box(p, q, HAND_BOX);
            let w2 = w + w;
            let mut span = w2 * across.fraction + rw;
            if span > w2 - 15.0 {
                let back = last - 0.2;
                let back = t1 - if back > 0.0 { back } else { 0.0 };
                let d = ((dy * up[1] + dx * up[0]) + dz * up[2]) * back;
                let s = [p[0] - d * up[0], p[1] - d * up[1], p[2] - d * up[2]];
                let f = (span + 15.0) / w2;
                let e = [(q[0] - p[0]) * f + s[0], (q[1] - p[1]) * f + s[1], (q[2] - p[2]) * f + s[2]];
                let t2 = self.trace_box(s, e, HAND_BOX);
                if t2.startsolid {
                    span = 0.0;
                    ok = false;
                } else {
                    let x = t2.fraction * (span + 15.0) - 15.0;
                    if span > x {
                        ok = !(rw * 0.5 + x > x);
                        span = x;
                    }
                }
            }
            let g = span - w;
            let g = (if 0.0 > g { 0.0 } else { g }) * signs[idx];
            shifted = [r[0] + side[0] * g, r[1] + side[1] * g, r[2] + side[2] * g];
        } else {
            ok = hits != 0;
        }
        let c0 = [hw * n[0] + hw * up[0], hw * n[1] + hw * up[1], hw * up[2] + hw * n[2]];
        let c = [c0[0] + shifted[0], c0[1] + shifted[1], c0[2] + shifted[2]];
        let two_hw = hw + hw;
        let (near, far, low) = (hw * -0.5, hw * 2.5, hw * -4.0);
        let ax = (near * n[0] - far * up[0]) - c0[0];
        let ay = (near * n[1] - far * up[1]) - c0[1];
        let az = (near * n[2] - far * up[2]) - c0[2];
        let bx = (low * n[0] - c0[0]) - ax;
        let by = (low * n[1] - c0[1]) - ay;
        let bz = (low * n[2] - c0[2]) - az;
        let lo = hw * 1.5;
        let span = hw * 5.0 - lo;
        let mut t = 0.5f32;
        let mut misses = 0;
        let mut last_t = 0.0f32;
        let (mut wall_seen, mut top_seen) = (false, false);
        let (mut wall_t, mut wall_end, mut wall_n) = (0.0f32, ZERO, ZERO);
        let (mut top_t, mut top_end, mut top_n) = (0.0f32, ZERO, ZERO);
        let accept: V3;
        loop {
            let len = span * t + lo;
            let k = if len > two_hw { two_hw / len } else { 1.0 };
            let e = [(bx * t + ax) * k + c[0], (by * t + ay) * k + c[1], (bz * t + az) * k + c[2]];
            let hit = self.trace_box(c, e, ZERO);
            if hit.fraction == 1.0 {
                if wall_seen == top_seen {
                    if wall_seen {
                        return;
                    }
                    misses += 1;
                    t = match misses {
                        1 => 0.6,
                        2 => 0.4,
                        _ => return,
                    };
                } else {
                    let gap = ((t - last_t) as f64).abs() as f32;
                    if 0.01 > gap {
                        if !top_seen {
                            return;
                        }
                        accept = top_end;
                        self.p.traversal_ref = accept;
                        break;
                    }
                    t = (last_t + t) * 0.5;
                }
                continue;
            }
            let hn = hit.normal;
            let walkable = (up[1] * hn[1] + up[0] * hn[0]) + up[2] * hn[2] > 0.7;
            if walkable {
                top_seen = true;
                top_end = hit.endpos;
                top_t = t;
                top_n = hn;
                if !wall_seen {
                    last_t = t;
                    t *= 0.7;
                    if 0.05 > t {
                        return;
                    }
                    continue;
                }
            } else {
                let d = (side[1] * hn[1] + side[0] * hn[0]) + side[2] * hn[2];
                if d * d > (hn[0] * hn[0] + hn[1] * hn[1]) * 0.49 {
                    return;
                }
                wall_seen = true;
                wall_end = hit.endpos;
                wall_t = t;
                wall_n = hn;
                if !top_seen {
                    last_t = t;
                    t = 1.0 - (1.0 - t) * 0.7;
                    if t > 0.95 {
                        return;
                    }
                    continue;
                }
            }
            let gap = top_t - wall_t;
            t = (top_t + wall_t) * 0.5;
            if !(0.05 > gap) {
                continue;
            }
            let wn = wall_n;
            let cx = up[1] * wn[2] - wn[1] * up[2];
            let cz = up[0] * wn[1] - up[1] * wn[0];
            let cy = wn[0] * up[2] - up[0] * wn[2];
            let dx = wn[1] * cz - cy * wn[2];
            let dy = cx * wn[2] - wn[0] * cz;
            let dz = wn[0] * cy - wn[1] * cx;
            let denom = (top_n[1] * dy + top_n[0] * dx) + top_n[2] * dz;
            if denom == 0.0 {
                return;
            }
            let s = (((top_end[0] - wall_end[0]) * top_n[0] + (top_end[1] - wall_end[1]) * top_n[1]) + (top_end[2] - wall_end[2]) * top_n[2]) / denom;
            let p = [dx * s + wall_end[0], dy * s + wall_end[1], dz * s + wall_end[2]];
            self.p.traversal_ref = p;
            let up_wall = (up[1] * wall_end[1] + wall_end[0] * up[0]) + up[2] * wall_end[2];
            let up_p = (up[0] * p[0] + up[1] * p[1]) + up[2] * p[2];
            let below_wall = up_wall > up_p;
            let n_p = (n[0] * p[0] + n[1] * p[1]) + n[2] * p[2];
            let n_top = (top_end[0] * n[0] + top_end[1] * n[1]) + top_end[2] * n[2];
            let past_top = n_top > n_p;
            if gap > 0.01 {
                if below_wall || past_top {
                    continue;
                }
                accept = p;
            } else if below_wall {
                accept = wall_end;
                self.p.traversal_ref = accept;
            } else if past_top {
                accept = top_end;
                self.p.traversal_ref = accept;
            } else {
                accept = p;
            }
            break;
        }
        let d = [shifted[0] - accept[0], shifted[1] - accept[1], shifted[2] - accept[2]];
        if (d[1] * d[1] + d[0] * d[0]) + d[2] * d[2] > 6400.0 {
            return;
        }
        if ok {
            self.p.traversal_hang = true;
        }
    }

    /// 0x8212e0 in part: duck at once, the origin moved so the feet stay where they were.
    fn force_duck(&mut self) {
        self.p.flags |= 2;
        self.p.traversal_state = 2;
        self.p.view_offset = self.s.stances[1].view_offset;
        let shift = self.hull_shift();
        let dz = if shift[2] > 0.0 { shift[2] } else { 0.0 };
        let o = self.mv.origin;
        let lifted = [(shift[0] - shift[0]) + o[0], (shift[1] - shift[1]) + o[1], (0.0 - dz) + o[2]];
        let mut at = lifted;
        if 0.0 < dz {
            let t = self.trace(lifted, [lifted[0] + 0.0, lifted[1] + 0.0, lifted[2] + dz]);
            at = t.endpos;
        }
        self.mv.origin = at;
        self.p.surface_friction = 1.0;
        self.p.push = ZERO;
        self.categorize(false);
    }

    /// 0x8100f0: begin a traversal along begin, mid and end, the player moved to `origin`.
    #[allow(clippy::too_many_arguments)]
    fn start_traversal(&mut self, forward: V3, entity: u64, begin: V3, mid: V3, end: V3, mid_frac: f32, origin: V3) {
        let now = self.g.curtime;
        self.p.grapple_anim_end(&mut self.cues);
        let p = &mut *self.p;
        p.traversal_kind = 0;
        p.traversal_type = 0;
        p.traversal_progress = 0.0;
        p.traversal_cycle = 0.0;
        p.traversal_start_time = now;
        p.traversal_blend_time = now;
        let o = self.mv.origin;
        p.traversal_blend_offset = [origin[0] - o[0], origin[1] - o[1], origin[2] - o[2]];
        let inv = WORLD_FRAME.invert();
        p.traversal_ref = inv.local(p.traversal_ref);
        p.traversal_forward = inv.local_dir(forward);
        p.traversal_ref = p.traversal_ref.map(round_32nd);
        p.traversal_begin = inv.local(begin);
        p.traversal_mid = inv.local(mid);
        p.traversal_end = inv.local(end);
        p.traversal_mid_frac = mid_frac;
        p.traversal_backoff = false;
        p.traversal_jump_time = 0.0;
        p.traversal_push = false;
        p.dodge_lock = false;
        p.dodged = false;
        p.sliding = false;
        p.jumped_recently = false;
        p.jumped_since_ground = false;
        p.retry_time = 0.0;
        if is_world_up(p.up) {
            p.repel_count = 0;
            p.same_wall = true;
        }
        self.mv.origin = origin;
        if self.p.ground.is_none() {
            self.p.ground = Some(entity.max(1));
            self.p.flags |= 1;
        }
        self.mv.velocity = ZERO;
        if self.p.grappling {
            let mut v = self.mv.velocity;
            self.p.grapple_detach(self.s, &mut v, true, now, &mut self.cues);
            self.mv.velocity = v;
        }
        self.p.movetype = MOVETYPE_TRAVERSE;
        self.end_climb();
    }

    /// 0x8139e0: one call of a traversal. `first` is the call that started it.
    pub fn traversal_move(&mut self, first: bool) {
        let frame = WORLD_FRAME;
        if self.p.ground.is_none() {
            let o = self.mv.origin;
            if self.p.movetype == MOVETYPE_TRAVERSE {
                self.p.movetype = MOVETYPE_WALK;
            }
            self.p.flags &= !1;
            self.end_traversal(&frame);
            self.p.traversal_release_time = self.g.curtime;
            self.mv.origin = o;
            return;
        }
        let m = self.s.mantle.clone();
        let now = self.g.curtime;
        let ft = self.g.frametime;
        let h = m.height;
        let kind = self.p.traversal_kind;
        let ty = self.p.traversal_type;
        let mut base = ty;
        let mut rest = 0.0f32;
        if kind == 0 {
            base = if (ty - 7) as u32 > 3 { ty } else { ty - 4 };
            rest = if (ty - 3) & !4 == 0 { m.rest_frac_below } else { m.rest_frac };
        }
        let progress = self.p.traversal_progress;
        self.p.traversal_release_time = 0.0;
        self.mv.velocity = ZERO;
        let mid_frac = self.p.traversal_mid_frac;
        let f = frame.dir(self.p.traversal_forward);
        let w = self.mv.view_wish;
        let push_in = (w[1] * f[1] + f[0] * w[0]) + w[2] * f[2];
        let pushing = push_in > 0.1;
        let pulling = -0.3 > push_in;
        let jump = self.mv.pressed & IN_JUMP != 0;
        let duck_held = self.mv.buttons & (IN_DUCK | IN_TOGGLE_DUCK) != 0;
        let since = self.p.traversal_start_time - self.p.grapple_time;
        let since = if 0.0 > since { 0.0 } else { since };
        let grappled = m.grapple_auto_mantle > since;
        if pushing {
            self.p.traversal_push = true;
        } else if pulling {
            self.p.traversal_push = false;
        }
        let eye_z = self.p.view_offset[2];
        let mut fwd;
        let mut back;
        let rate;
        if kind == 0 {
            if jump && !self.p.traversal_backoff && (!pushing || !(progress < mid_frac)) && self.p.traversal_jump_time == 0.0 {
                self.p.traversal_jump_time = now;
            }
            let idle = self.p.traversal_jump_time == 0.0;
            fwd = idle && pushing;
            back = idle && pulling;
            if progress > rest && !fwd && !back && !jump {
                fwd = if progress > mid_frac { true } else { self.p.traversal_push };
            }
            if self.p.traversal_backoff {
                back = true;
                fwd = false;
            }
            let (zref, i) = match base {
                4 => (m.height_below - eye_z, 1),
                5 => (m.height_level - eye_z, 2),
                6 => (m.height_above - eye_z, 3),
                _ => (-h, 0),
            };
            rate = (1.0 - ((zref + h) / h) * mid_frac) / m.duration[i];
        } else {
            fwd = true;
            back = pulling;
            rate = 0.0;
        }
        let mut target;
        let mut newp = progress;
        if self.p.traversal_jump_time == 0.0 {
            let floor = self.p.traversal_rest;
            target = if kind != 0 {
                1.0
            } else if fwd || (!self.p.traversal_hang && grappled) {
                if floor > 1.0 { floor } else { 1.0 }
            } else if back || !self.p.traversal_hang || !(((frame.0[2][2] as f64).abs() as f32) > 0.99) {
                if floor > 0.0 { floor } else { 0.0 }
            } else if floor > rest {
                floor
            } else {
                rest
            };
            if target > progress {
                let x = rate * ft + progress;
                newp = if target < x { target } else { x };
            } else if progress > target {
                let x = progress - rate * ft;
                newp = if target > x { target } else { x };
            }
            if kind == 0 && newp == target && target == rest {
                rest = m.rest_frac;
                target = rest;
                newp = rest;
            }
        } else {
            target = progress;
        }
        self.p.traversal_gun = if kind == 0 {
            let frac = ((m.gun_enable_height - eye_z) + h) / (h + self.mv.hull_half_width);
            (!(target >= frac) || !(newp >= frac)) as i32
        } else {
            0
        };
        let mut changed = first;
        let mut new_type = base;
        if first && kind == 0 {
            let o = self.mv.origin;
            let vo = self.p.view_offset;
            let r = frame.point(self.p.traversal_ref);
            let dx = (vo[0] + o[0]) - r[0];
            let dy = (vo[1] + o[1]) - r[1];
            let dz = (vo[2] + o[2]) - r[2];
            let behind = (dy * f[1] + dx * f[0]) + dz * f[2];
            new_type = if m.height_below > dz && behind > -self.mv.hull_half_width - 4.0 {
                3
            } else if m.height_level > dz {
                4
            } else if m.height_above <= dz {
                6
            } else {
                5
            };
        }
        if kind == 0 {
            if target == rest && 0.001 > ((newp - m.rest_frac) as f64).abs() as f32 {
                new_type = REST;
            } else if self.p.traversal_hang && ((!fwd && newp > rest) || new_type == REST) {
                new_type = 4;
            }
            if self.p.traversal_state == 2 && (new_type - REST) as u32 > 1 {
                new_type += 4;
            }
            if self.p.traversal_jump_time > now {
                new_type = JUMP_OFF;
            }
        }
        if first || new_type != self.p.traversal_type {
            changed = true;
            self.p.traversal_type = new_type;
        }
        let p0 = if kind == 0 {
            let zref = match self.p.traversal_type {
                4 => m.height_below - eye_z,
                5 => m.height_level - eye_z,
                6 => m.height_above - eye_z,
                _ => -h,
            };
            ((zref + h) / h) * mid_frac
        } else {
            0.0
        };
        let aim = (newp - p0) / (1.0 - p0);
        let aim = if aim > 0.0 { aim } else { 0.0 };
        let aim = if aim < 1.0 { aim } else { 1.0 };
        let catch = 1.0 / (1.0 - p0);
        if changed && !first {
            self.p.traversal_cycle = aim;
        }
        let step = rate * ft * 1.5 * catch;
        let c = self.p.traversal_cycle;
        if aim > c {
            let x = step + c;
            self.p.traversal_cycle = if x > aim { aim } else { x };
        } else {
            let x = c - step;
            self.p.traversal_cycle = if aim > x { aim } else { x };
        }
        let c = self.p.traversal_cycle;
        let c = if c > 0.0 { c } else { 0.0 };
        self.p.traversal_cycle = if c < 1.0 { c } else { 1.0 };

        if newp == 1.0 {
            self.p.traversal_progress = 1.0;
            self.end_traversal(&frame);
            self.mv.origin = frame.point(self.p.traversal_end);
            if self.p.ground.is_none() {
                self.p.movetype = MOVETYPE_WALK;
            }
            if pushing {
                let speed = self.exit_speed();
                let w = self.mv.flat_wish;
                self.mv.velocity = [speed * w[0], speed * w[1], speed * w[2]];
            }
            return;
        }

        let r = frame.point(self.p.traversal_ref);
        let placed = if (self.p.traversal_type - REST) as u32 > 1 {
            self.root.place(self.p.traversal_type, self.p.traversal_cycle, r, f)
        } else {
            None
        };
        match placed {
            Some(a) => {
                if changed {
                    let o = self.mv.origin;
                    let mut off = [o[0] - a[0], o[1] - a[1], o[2] - a[2]];
                    let d = (f[1] * off[1] + f[0] * off[0]) + f[2] * off[2];
                    if 0.0 < d {
                        off = [off[0] - f[0] * d, off[1] - f[1] * d, off[2] - f[2] * d];
                    }
                    self.p.traversal_blend_offset = off;
                    self.p.traversal_blend_time = now;
                }
                let k = (now - self.p.traversal_blend_time) / BLEND_OUT;
                let k = if 0.0 >= k { 0.0 } else { k };
                let k = if 1.0 <= k { 1.0 } else { k };
                let left = 1.0 - k;
                let off = self.p.traversal_blend_offset;
                self.mv.origin = [left * off[0] + a[0], left * off[1] + a[1], left * off[2] + a[2]];
            }
            None => self.mv.origin = traversal_pos(self.p, &frame),
        }

        if kind == 0 {
            let jt = self.p.traversal_jump_time;
            if jt != 0.0 && (jt + m.jumpoff_duration <= now || m.jumpoff_anim_maxfrac < newp) {
                self.end_traversal(&frame);
                self.p.traversal_release_time = now;
                self.p.jumped_since_ground = true;
                if newp < mid_frac {
                    let out = -m.jump_outward_speed;
                    let u = self.mv.jump_dir;
                    let lift = m.jump_up_speed - ((u[0] * f[0] * out + u[1] * f[1] * out) + u[2] * f[2] * out);
                    self.mv.velocity = [lift * u[0] + f[0] * out, lift * u[1] + f[1] * out, lift * u[2] + f[2] * out];
                    return;
                }
                let speed = self.exit_speed();
                let g = self.mv.ground_wish;
                self.mv.velocity = [speed * g[0], speed * g[1], speed * g[2]];
                self.check_jump();
                return;
            }
            let cycle = self.p.traversal_cycle;
            let let_go = (back && (mid_frac <= newp || m.backoff_anim_maxfrac <= newp))
                || ((newp <= self.p.traversal_rest || (target == 0.0 && cycle <= 0.0)) && ((!pushing && !grappled) || self.p.traversal_backoff));
            if let_go {
                self.release_traversal(&frame);
                return;
            }
            if back || (self.p.traversal_type == REST && duck_held) {
                if base == REST {
                    self.p.traversal_backoff = true;
                } else if !self.p.traversal_backoff {
                    self.release_traversal(&frame);
                    return;
                }
            }
        }
        self.p.traversal_progress = newp;
    }

    fn release_traversal(&mut self, frame: &Frame) {
        self.end_traversal(frame);
        self.p.traversal_release_time = self.g.curtime;
    }

    /// Run or sprint speed for this stance, as a traversal leaves at.
    fn exit_speed(&self) -> f32 {
        let st = self.stance();
        self.p.speed_scale * if self.p.sprint_intent { st.sprint_speed } else { st.speed }
    }

    /// 0x815210: back on the procedural path at the stored progress, walking.
    fn end_traversal(&mut self, frame: &Frame) {
        let pos = traversal_pos(self.p, frame);
        self.p.traversal_cycle = 0.0;
        self.p.traversal_gun = 0;
        self.mv.origin = pos;
        self.p.movetype = MOVETYPE_WALK;
        self.categorize(false);
    }
}

/// What `find_ledge` found: the trace down onto the top, the wall's flat normal, how far
/// past the wall face the player may go, and whether there is room below to hang.
#[derive(Clone, Copy, Debug)]
struct Ledge {
    top: Trace,
    normal: V3,
    reach: f32,
    hang: bool,
}
