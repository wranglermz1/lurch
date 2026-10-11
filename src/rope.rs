//! The grapple's rope as the client draws it for the local player (`0x6c0810`): from the hook
//! entity through the rope's bends to the hand, each span cut into pieces that wiggle, loose
//! while the hook flies and tight once it pulls (`0x6beb50`), then drawn as a tube of
//! `cable/grapple` (`0x6bf3e0`). The hand is the arms' `grapple_rope` attachment while the
//! grapple's animation plays, else a point by the eye, eased back to over 0.3 s.
//!
//! Not modelled: other players' ropes (20 pieces, 4 sides, their model's attachment).

use crate::sim::math::{angle_vectors, V3};

/// ConVars `rope_wiggle_rotate_speed`, `rope_wiggle_oscillate_speed`,
/// `rope_wiggle_magnitude_loose`, `rope_wiggle_magnitude_tight`,
/// `rope_wiggle_harmonic_falloff` and `rope_texels_per_world_unit` (the dump's values).
pub const ROPE_WIGGLE_ROTATE_SPEED: f32 = 2.0;
pub const ROPE_WIGGLE_OSCILLATE_SPEED: f32 = 64.0;
pub const ROPE_WIGGLE_MAGNITUDE_LOOSE: f32 = 0.04;
pub const ROPE_WIGGLE_MAGNITUDE_TIGHT: f32 = 0.002;
pub const ROPE_WIGGLE_HARMONIC_FALLOFF: f32 = 0.75;
pub const ROPE_TEXELS_PER_WORLD_UNIT: f32 = 8.0;
/// The local player's rope: pieces per span, the tube's sides, and its width.
pub const PIECES: usize = 50;
pub const SIDES: usize = 6;
pub const WIDTH: f32 = 1.0;
/// The hand without the grapple's animation: ahead of the eye, right of it and below it.
const HAND_FORWARD: f32 = 7.0;
const HAND_RIGHT: f32 = 3.0;
const HAND_DOWN: f32 = 12.0;
/// How long the hand eases from the attachment's last place back to `HAND`.
pub const HAND_EASE: f32 = 0.3;
const TINY: f32 = 1.0e-30;

/// The rope's end at the player: the attachment's place while the animation plays (`held`,
/// which is remembered with the time), else the point by the eye, eased in from where the
/// attachment last was.
#[derive(Clone, Copy, Debug, Default)]
pub struct Hand {
    last: V3,
    since: f32,
}

impl Hand {
    pub fn at(&mut self, held: Option<V3>, eye: V3, angles: V3, now: f32) -> V3 {
        if let Some(p) = held {
            (self.last, self.since) = (p, now);
            return p;
        }
        let (f, r, u) = angle_vectors(angles);
        let p = [0, 1, 2].map(|i| (f[i] * HAND_FORWARD + eye[i] + r[i] * HAND_RIGHT) - u[i] * HAND_DOWN);
        let t = now - self.since;
        if !(t < HAND_EASE) {
            return p;
        }
        let k = t / HAND_EASE;
        let k = if k <= 0.0 { 0.0 } else if 1.0 <= k { 1.0 } else { k };
        [0, 1, 2].map(|i| (p[i] - self.last[i]) * k + self.last[i])
    }
}

/// How tight the rope is: 1 pulling or reeling in, from 0 to 1 over `pull_delay` after the
/// hook caught, 0 while it flies.
pub fn tightness(pulling: bool, retracting: bool, attached: bool, attach_time: f32, pull_delay: f32, now: f32) -> f32 {
    if retracting || pulling {
        return 1.0;
    }
    if !attached {
        return 0.0;
    }
    let t = (now - attach_time) / pull_delay;
    if 1.0 <= t { 1.0 } else { t }
}

/// One span's wiggle (`0x6beb50`): its length, two directions across it, and four harmonics'
/// turn and size.
struct Wiggle {
    angle: [f32; 4],
    amp: [f32; 4],
    u: V3,
    w: V3,
}

fn wiggle(a: V3, b: V3, magnitude: f32, now: f32, oscillate: bool) -> Wiggle {
    let (dx, dy, dz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
    let l = (dy * dy + dx * dx + dz * dz).sqrt();
    let lu = (dx * dx + -dy * -dy + 0.0).sqrt();
    let inv = 1.0 / if lu <= TINY { TINY } else { lu };
    let u = [-dy * inv, dx * inv, inv * 0.0];
    let w = [u[2] * dy - u[1] * dz, dz * u[0] - dx * u[2], dx * u[1] - dy * u[0]];
    let lw = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
    let inv = 1.0 / if lw <= TINY { TINY } else { lw };
    let w = [w[0] * inv, w[1] * inv, w[2] * inv];
    let mut out = Wiggle { angle: [0.0; 4], amp: [0.0; 4], u, w };
    let mut size = l * magnitude;
    for k in 0..4 {
        let odd = (k & 1) as f32;
        out.amp[k] = size;
        out.angle[k] = ((odd + odd) - 1.0) * now * ROPE_WIGGLE_ROTATE_SPEED * (k as f32 + 1.0);
        if oscillate {
            out.amp[k] = ((k as f32 + 1.0) * now * ROPE_WIGGLE_OSCILLATE_SPEED).sin() * size;
        }
        size *= ROPE_WIGGLE_HARMONIC_FALLOFF;
    }
    out
}

/// The rope's points from the hook entity at `hook` through the bends (`bends`, the grapple's
/// points from the player's end, the last being the hook's) to `hand`: `PIECES` a span, then
/// the hand. `max_length` is the weapon's `grapple_maxLength`.
pub fn points(hook: V3, bends: &[V3], hand: V3, tight: f32, max_length: f32, now: f32) -> Vec<V3> {
    let n = bends.len();
    let mut ends: Vec<V3> = bends.iter().rev().copied().collect();
    ends.push(hand);
    if let Some(e) = ends.first_mut() {
        *e = hook;
    }
    let (h, e) = (ends[n], ends[0]);
    let q = ((h[1] - e[1]) * (h[1] - e[1]) + (h[0] - e[0]) * (h[0] - e[0]) + (h[2] - e[2]) * (h[2] - e[2])) / (max_length * max_length);
    let (loose, taut) = (ROPE_WIGGLE_MAGNITUDE_LOOSE, ROPE_WIGGLE_MAGNITUDE_TIGHT);
    let magnitude = (1.0 - (1.0 - q * q) * (1.0 - tight)) * (taut - loose) + loose;
    let mut out = Vec::with_capacity(n * PIECES + 1);
    for s in ends.windows(2) {
        let (a, b) = (s[0], s[1]);
        let wg = wiggle(a, b, magnitude, now, tight == 1.0);
        let (sc, cs): (Vec<f32>, Vec<f32>) = wg.angle.iter().map(|x| (x.sin(), x.cos())).unzip();
        for j in 0..PIECES {
            let t = j as f32 / (PIECES + 1) as f32;
            let x = t * std::f32::consts::PI;
            let mut p = [0, 1, 2].map(|i| (b[i] - a[i]) * t + a[i]);
            for k in 0..4 {
                let h = (x * (k as f32 + 1.0)).sin();
                let (along_w, along_u) = (h * wg.amp[k] * sc[k], cs[k] * h * wg.amp[k]);
                for ((c, w), u) in p.iter_mut().zip(wg.w).zip(wg.u) {
                    *c += w * along_w + u * along_u;
                }
            }
            out.push(p);
        }
    }
    out.push(hand);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rope_runs_from_the_hook_to_the_hand_and_wiggles_less_once_tight() {
        let (hook, hand) = ([400.0, 0.0, 100.0], [0.0, 0.0, 50.0]);
        let spread = |tight: f32| {
            let pts = points(hook, &[hook], hand, tight, 850.0, 1.25);
            assert_eq!(pts.len(), PIECES + 1);
            assert_eq!((pts[0], *pts.last().unwrap()), (hook, hand));
            pts.iter().map(|p| p[1].abs()).fold(0.0, f32::max)
        };
        let (loose, taut) = (spread(0.0), spread(1.0));
        assert!(loose > 1.0 && taut < loose * 0.1, "{loose} {taut}");
    }
}
