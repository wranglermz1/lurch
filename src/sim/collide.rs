//! Moving the player hull through the world: the slide loop, stepping up and over, sliding
//! along walkable slopes, and keeping a walking player glued to the ground.

use super::math::*;
use super::{HullQuery, Move, Trace};

const OVERCLIP: f32 = 1.0 / 128.0;
const MAX_BUMPS: usize = 8;
const WALKABLE: f32 = 0.7;
const STEP_SLACK: f64 = 1.0 / 32.0;

/// The horizontal direction along both `n` and the floor `g`: a shallow wall hit while
/// walking is treated as a vertical wall.
pub fn pin_vertical(n: V3, g: V3) -> V3 {
    let x = -(n[2] * g[0] - g[2] * n[0]);
    let y = g[2] * n[1] - n[2] * g[1];
    let k = 1.0 / (x * x + y * y + 0.0).sqrt().max(TINY);
    [x * k, y * k, k * 0.0]
}

impl Move<'_> {
    pub fn hull(&self) -> (V3, V3) {
        if self.ducked() { (self.p.hull_duck_min, self.p.hull_duck_max) } else { (self.p.hull_min, self.p.hull_max) }
    }

    pub fn trace(&mut self, start: V3, end: V3) -> Trace {
        let (mins, maxs) = self.hull();
        self.tr.hull(&HullQuery { start, end, mins, maxs, mask: self.p.mask })
    }

    pub fn half_width(&self) -> f32 {
        let (mins, maxs) = self.hull();
        (maxs[0] - mins[0]) * 0.5
    }

    /// Move `pos` by `vel` over `time`, clipping against what it hits, up to 8 bumps.
    /// `first` is a trace already made along this move. `ground` pins a shallow hit against
    /// the steepest known plane to vertical. `ceiling` refuses motion along that direction.
    pub fn slide(&mut self, pos: &mut V3, vel: &mut V3, time: f32, mut first: Option<Trace>, ground: Option<V3>, ceiling: Option<V3>) {
        let original = *vel;
        let mut primal = *vel;
        let mut last = *pos;
        let mut planes: Vec<V3> = Vec::with_capacity(2);
        let mut moved = false;
        let mut left = time;
        for _ in 0..MAX_BUMPS {
            let t = match first.take() {
                Some(t) => t,
                None => match ceiling {
                    Some(c) if dot(c, *vel) > 0.0 => {
                        Trace { start: *pos, endpos: *pos, normal: neg(c), fraction: 0.0, ..Default::default() }
                    }
                    _ => {
                        let mut dest = mad(*pos, *vel, left);
                        let delta = sub(dest, *pos);
                        if len2(delta) < 1e-4 {
                            break;
                        }
                        for p in &planes {
                            let d = dot(*p, delta);
                            if d < OVERCLIP {
                                dest = mad(dest, *p, OVERCLIP - d);
                            }
                        }
                        let mut t = self.trace(*pos, dest);
                        if let Some(g) = ground.filter(|_| t.normal[2] < 0.5 && t.normal[2] > 0.0) {
                            let steep = planes.iter().fold(g, |a, p| if a[2] < p[2] { *p } else { a });
                            let pinned = pin_vertical(t.normal, steep);
                            if !planes.iter().any(|p| p[2] == 0.0 && pinned[0] * p[0] + pinned[1] * p[1] > 0.99) {
                                t.normal = pinned;
                            }
                        }
                        t
                    }
                },
            };
            if t.fraction > 0.0 {
                moved = true;
                *pos = t.endpos;
                if t.fraction == 1.0 {
                    return;
                }
                if len2(sub(last, t.endpos)) > 1.0 {
                    planes.clear();
                    primal = *vel;
                    last = t.endpos;
                }
            }
            left -= t.fraction * left;
            let n = t.normal;
            *vel = mad(primal, n, OVERCLIP - dot(n, primal));
            match planes.len() {
                0 => planes.push(n),
                1 if dot(planes[0], *vel) >= 0.0 => planes[0] = n,
                _ => {
                    if planes.len() == 1 {
                        planes.push(n);
                    } else {
                        let (d0, d1) = (dot(planes[0], n), dot(planes[1], n));
                        if d0.max(d1) < 0.866 {
                            *vel = ZERO;
                            return;
                        }
                        planes[if d0 < d1 { 1 } else { 0 }] = n;
                    }
                    let crease = cross(planes[0], planes[1]);
                    let sum = add(planes[0], planes[1]);
                    let k = dot(crease, primal) / len2(crease).max(TINY);
                    let s = (1.0 / len2(sum).max(TINY)).sqrt() * OVERCLIP;
                    *vel = add(scale(sum, s), scale(crease, k));
                }
            }
            if dot(original, *vel) <= 0.0 {
                *vel = ZERO;
                return;
            }
        }
        if !moved {
            *vel = ZERO;
        }
    }

    /// The slide used to settle back down after a step: it stops on the first floor-like plane
    /// and leaves that trace in `t`.
    fn settle(&mut self, pos: &mut V3, vel: &mut V3, t: &mut Trace) {
        let up = self.p.up;
        let original = *vel;
        let mut primal = *vel;
        let mut last = *pos;
        let mut planes: Vec<V3> = Vec::with_capacity(2);
        let mut left = 1.0f32;
        for bump in 0..MAX_BUMPS {
            let cur = if bump == 0 {
                *t
            } else {
                let mut dest = mad(*pos, *vel, left);
                let delta = sub(dest, *pos);
                if len2(delta) < 1e-4 {
                    return;
                }
                for p in &planes {
                    let d = dot(*p, delta);
                    if d < OVERCLIP {
                        dest = mad(dest, *p, OVERCLIP - d);
                    }
                }
                self.trace(*pos, dest)
            };
            if cur.fraction > 0.0 {
                *pos = cur.endpos;
                if cur.fraction == 1.0 {
                    return;
                }
                if len2(sub(last, cur.endpos)) > 1.0 {
                    planes.clear();
                    primal = *vel;
                    last = cur.endpos;
                }
            }
            if dot(cur.normal, up) > 0.1 {
                *t = cur;
                return;
            }
            left -= cur.fraction * left;
            let n = cur.normal;
            *vel = mad(primal, n, OVERCLIP - dot(n, primal));
            match planes.len() {
                0 => planes.push(n),
                1 if dot(planes[0], *vel) >= 0.0 => planes[0] = n,
                _ => {
                    if planes.len() == 1 {
                        planes.push(n);
                    } else {
                        let (d0, d1) = (dot(planes[0], n), dot(planes[1], n));
                        if d0.max(d1) < 0.866 {
                            return;
                        }
                        planes[if d0 < d1 { 1 } else { 0 }] = n;
                    }
                    let crease = cross(planes[0], planes[1]);
                    let sum = add(planes[0], planes[1]);
                    let sign = if dot(primal, crease) < 0.0 { -1.0 } else { 1.0 };
                    let k = sign * (len2(crease) / (len2(sum) + f32::MIN_POSITIVE)).sqrt() * OVERCLIP;
                    let dir = add(scale(sum, k), crease);
                    *vel = scale(dir, dot(primal, dir) / (len2(dir) + f32::MIN_POSITIVE));
                }
            }
            if dot(original, *vel) <= 0.0 {
                return;
            }
        }
    }

    /// Slide, then try the same move raised by the step height and dropped back down; keep
    /// the stepped result only when it got further along the horizontal direction.
    /// Returns how far the step moved the player, zero when it did not step.
    pub fn step_slide(&mut self, pos: &mut V3, vel: &mut V3, dt: f32, step: f32, first: Trace, ground: Option<V3>) -> V3 {
        if len2(*vel) == 0.0 {
            return ZERO;
        }
        let up = self.p.up;
        let start = *pos;
        let start_vel = *vel;
        self.slide(pos, vel, dt, Some(first), ground, None);
        let slid = *pos;

        let stepf = (step as f64 + STEP_SLACK) as f32;
        let t = self.trace(start, mad(start, up, stepf));
        let (raised, climbed) = if !t.allsolid && !t.startsolid { (t.endpos, t.fraction * stepf) } else { (start, 0.0) };

        let d = dot(up, start_vel);
        let d = (d * 1.001).max(d * 0.999) + OVERCLIP;
        let flat = sub(start_vel, scale(up, d));
        let flat_len = len(flat);
        if flat_len == 0.0 {
            return ZERO;
        }
        let mut p2 = raised;
        let mut v2 = flat;
        self.slide(&mut p2, &mut v2, dt, None, None, Some(up));

        let horiz = |a: V3| reject(sub(a, start), up);
        let unstepped = len2(horiz(slid)) * flat_len * flat_len;
        let gained = dot(flat, horiz(p2));
        if !(gained > 0.0 && unstepped < ((gained as f64 * 0.999) as f32).powi(2)) {
            return ZERO;
        }
        let grounded = self.on_ground();
        let mut down = (climbed as f64 + STEP_SLACK) as f32;
        if grounded {
            down += step;
        }
        let mut land = self.trace(p2, sub(p2, scale(up, down)));
        if land.fraction < 1.0 {
            let mut at = p2;
            let mut v = sub(sub(p2, scale(up, down)), p2);
            self.settle(&mut at, &mut v, &mut land);
        }
        let first_land = land;
        let nz = dot(up, land.normal);
        if !(land.flag3b || (1.0 - nz) * self.half_width() <= down * land.fraction) {
            return ZERO;
        }
        let along = dot(horiz(land.endpos), flat);
        if !(along > 0.0 && unstepped < ((along as f64 * 0.999) as f32).powi(2)) {
            return ZERO;
        }
        if nz <= WALKABLE {
            let dl = reject(sub(land.endpos, slid), up);
            if len2(dl) <= 0.0 {
                return ZERO;
            }
            let mut walkable = false;
            let reach = ((flat_len * dt + step * 1.0202) as f64 + STEP_SLACK) as f32;
            let mut latest = land;
            let mut dl_full = sub(land.endpos, slid);
            if len2(dl) < reach * reach {
                let mut p3 = raised;
                let mut v3 = flat;
                self.slide(&mut p3, &mut v3, reach / flat_len, None, None, Some(up));
                let end = sub(p3, scale(up, down));
                latest = self.trace(p3, end);
                if latest.fraction < 1.0 {
                    let mut at = p3;
                    let mut v = sub(end, p3);
                    self.settle(&mut at, &mut v, &mut latest);
                }
                dl_full = sub(latest.endpos, slid);
                walkable = WALKABLE < dot(up, latest.normal);
            }
            let lnz = dot(latest.normal, up);
            if lnz > 0.0 && grounded {
                let du = dot(dl_full, up);
                if du <= 0.0 {
                    walkable = true;
                } else {
                    let h = len2(sub(dl_full, scale(up, du)));
                    if 0.49 < h / (du * du + h) {
                        walkable = true;
                    }
                }
            }
            let base = dot(start, up);
            let rise_latest = ((dot(latest.endpos, up) - base) as f64).abs() as f32 * 2.0;
            let rise_first = ((dot(first_land.endpos, up) - base) as f64).abs() as f32;
            if !(rise_first <= rise_latest.max(step * 0.25) && walkable) {
                return ZERO;
            }
            if lnz < WALKABLE
                && 0.9 < dot(latest.normal, first_land.normal)
                && (dot(sub(latest.endpos, first_land.endpos), first_land.normal) as f64).abs() < 5.0
            {
                return ZERO;
            }
        }
        let delta = sub(first_land.endpos, start);
        *pos = first_land.endpos;
        *vel = start_vel;
        if up[2] == 1.0 {
            return delta;
        }
        let n = first_land.normal;
        let d = dot(n, delta);
        if d <= 0.0 { ZERO } else { scale(n, d) }
    }

    /// Step over what blocked the move if this player can step, else plain slide.
    pub fn step_move(&mut self, t: Trace) {
        let mut pos = self.mv.origin;
        let mut vel = self.mv.velocity;
        let dt = self.g.frametime;
        if !self.p.can_step {
            self.slide(&mut pos, &mut vel, dt, Some(t), None, None);
        } else {
            let ground = if self.on_ground() && is_world_up(self.p.up) { Some(self.p.ground_plane) } else { None };
            let step = self.p.step_height;
            let stepped = self.step_slide(&mut pos, &mut vel, dt, step, t, ground);
            if self.p.sliding && 0.0 < stepped[2] {
                let speed = (vel[0] * vel[0] + vel[1] * vel[1]).sqrt();
                if 0.0 < speed {
                    let k = (speed - stepped[2] * self.s.slide_step_velocity_reduction).max(0.0) / speed;
                    vel = [vel[0] * k, vel[1] * k, vel[2]];
                }
            }
        }
        self.mv.origin = pos;
        self.mv.velocity = vel;
    }

    /// A blocked move that hit a walkable slope keeps going along it for the rest of the
    /// time (or towards `dest`), else falls back to stepping.
    pub fn along_slope(&mut self, mut t: Trace, grounded: bool, dest: Option<V3>, refuse: bool) -> bool {
        let up = self.p.up;
        let n = t.normal;
        if dot(up, n) > WALKABLE && !refuse {
            let e = t.endpos;
            let un = dot(up, n);
            let (d, timed) = match dest {
                Some(d) => (sub(d, e), false),
                None => (self.mv.velocity, true),
            };
            let dn = dot(d, n);
            let along = sub(scale(d, un), scale(up, dn));
            let rest = ((1.0 - t.fraction as f64) * self.g.frametime as f64) as f32;
            let k = if !grounded && timed {
                let a = un * un * len2(d);
                if a <= len2(along) { (len2(d) / len2(along)).min(f32::MAX).sqrt() * rest } else { rest / un }
            } else if !grounded {
                let a = un * un * len2(d);
                if a <= len2(along) { (len2(d) / len2(along)).min(f32::MAX).sqrt() } else { 1.0 / un }
            } else if timed {
                (len2(d) / len2(along)).min(f32::MAX).sqrt() * rest
            } else {
                (len2(d) / len2(along)).min(f32::MAX).sqrt()
            };
            let mut target = mad(e, along, k);
            target[2] += 0.01;
            let f = t.fraction;
            t = self.trace(e, target);
            if t.fraction == 1.0 {
                self.mv.origin = target;
                return true;
            }
            t.fraction = (1.0 - f) * t.fraction + f;
        }
        self.step_move(t);
        false
    }

    /// Probe down by the step height and report whether there is floor to stand on; the probe
    /// is kept for the categorize that follows.
    fn probe_ground(&mut self, origin: V3, step: f32, up: V3) -> Option<Trace> {
        let end = sub(origin, scale(up, step));
        self.scratch.probe_end = end;
        let mut t = self.trace(origin, end);
        if t.startsolid {
            t = self.trace(add(origin, up), end);
            if t.startsolid {
                self.scratch.probe = Some(t);
                return None;
            }
        }
        self.scratch.probe = Some(t);
        if t.fraction > 0.0 {
            if t.fraction >= 1.0 || dot(up, t.normal) < WALKABLE {
                return None;
            }
            if (dot(sub(t.endpos, origin), up) as f64).abs() <= 0.015625 {
                t.endpos = origin;
                self.scratch.probe = Some(t);
            }
        }
        Some(t)
    }

    pub fn stay_on_ground(&mut self) -> bool {
        let up = self.p.up;
        let origin = self.mv.origin;
        let Some(t) = self.probe_ground(origin, self.p.step_height, up) else { return false };
        if is_world_up(up) {
            self.mv.origin = t.endpos;
        }
        true
    }
}
