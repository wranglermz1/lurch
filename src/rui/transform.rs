//! An element's transform code (the element's `+0x10` block), run by `0x2f31d0`: an opcode
//! byte, then its operands, each opcode adding transforms built from a parent's and from values
//! in the element's data block. A transform maps the unit square onto the element: a 2×2 basis
//! (the `x` axis in lanes 0 and 1, the `y` axis in lanes 2 and 3) and an origin kept twice
//! (`x, y, x, y`). Sizes run alongside, `w, w, h, h`. The handlers are the table at
//! `0x1300a00`; lane sums keep the engine's order.

use super::Frame;

pub type V4 = [f32; 4];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Transform {
    pub basis: V4,
    pub origin: V4,
}

/// `0x23fea5e0`: keeps lanes 0 and 3, a diagonal basis.
pub fn diagonal(v: V4) -> V4 {
    [v[0], 0.0, 0.0, v[3]]
}

/// MAXPS: the second operand wins a tie or a NaN.
pub fn maxps(a: V4, b: V4) -> V4 {
    std::array::from_fn(|i| if a[i] > b[i] { a[i] } else { b[i] })
}

/// CVTTPS2DQ: truncated, the integer indefinite (`i32::MIN`) when out of range or NaN.
fn cvttps2dq(x: f32) -> i32 {
    if (-2_147_483_648.0..2_147_483_648.0).contains(&x) { x as i32 } else { i32::MIN }
}

/// FLT_MIN in every lane (`0x1312b90`), the floor a divisor is clamped to.
pub const TINY: V4 = [f32::MIN_POSITIVE; 4];

pub fn rcpps(v: V4) -> V4 {
    use core::arch::x86_64::{_mm_rcp_ps, _mm_set_ps, _mm_storeu_ps};
    let mut out = [0.0; 4];
    // SAFETY: SSE is part of every x86-64 CPU.
    unsafe { _mm_storeu_ps(out.as_mut_ptr(), _mm_rcp_ps(_mm_set_ps(v[3], v[2], v[1], v[0]))) };
    out
}

pub fn rsqrtps(v: V4) -> V4 {
    use core::arch::x86_64::{_mm_rsqrt_ps, _mm_set_ps, _mm_storeu_ps};
    let mut out = [0.0; 4];
    // SAFETY: SSE is part of every x86-64 CPU.
    unsafe { _mm_storeu_ps(out.as_mut_ptr(), _mm_rsqrt_ps(_mm_set_ps(v[3], v[2], v[1], v[0]))) };
    out
}

/// `rcpps` refined the engine's way: `(e * e + e) * r + r` with `e = 1 - r * x`.
pub fn recip(x: V4) -> V4 {
    let r = rcpps(x);
    std::array::from_fn(|i| {
        let e = 1.0 - r[i] * x[i];
        (e * e + e) * r[i] + r[i]
    })
}

/// The parent's point at `(a, b)` of its unit square: `(a, a, b, b) * basis`, its halves
/// swapped and added (`pshufd 0x4e`, `addps`), then the origin.
fn point(p: &Transform, a: f32, b: f32) -> V4 {
    let m = [a * p.basis[0], a * p.basis[1], b * p.basis[2], b * p.basis[3]];
    [m[2] + m[0] + p.origin[0], m[3] + m[1] + p.origin[1], m[0] + m[2] + p.origin[2], m[1] + m[3] + p.origin[3]]
}

/// The origin that puts the new basis's point `(c, d)` on `at`.
fn place(at: V4, basis: V4, c: f32, d: f32) -> V4 {
    let n = [c * basis[0], c * basis[1], d * basis[2], d * basis[3]];
    [at[0] - (n[2] + n[0]), at[1] - (n[3] + n[1]), at[2] - (n[0] + n[2]), at[3] - (n[1] + n[3])]
}

/// Why a run stopped short.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// An opcode whose handler is not ported yet.
    Unported(u8),
    /// A callback not ported yet, by its offset in the table.
    Api(u8),
    /// No element of that name, or no function for it in `ui.dll`.
    Missing,
    /// More transforms than the frame holds.
    Full,
}

impl Frame {
    fn data_f32(&self, data: *const u8, off: u16) -> f32 {
        // SAFETY: `data` is the instance's data block, `data_size` bytes, and the offset is
        // checked against it.
        if off as usize + 4 > self.data_size {
            return 0.0;
        }
        unsafe { (data.add(off as usize) as *const f32).read_unaligned() }
    }

    fn take_u8(&mut self, code: &[u8]) -> u8 {
        let b = code.get(self.pc).copied().unwrap_or(0);
        self.pc += 1;
        b
    }

    fn take_u16(&mut self, code: &[u8]) -> u16 {
        let v = u16::from_le_bytes([code.get(self.pc).copied().unwrap_or(0), code.get(self.pc + 1).copied().unwrap_or(0)]);
        self.pc += 2;
        v
    }

    fn next(&mut self) -> Result<usize, Stop> {
        let i = self.count as usize;
        if i >= self.transforms.len() {
            return Err(Stop::Full);
        }
        self.count += 1;
        Ok(i)
    }

    /// Runs the code from where it stands to `end` (`0x2f31d0`).
    pub fn run(&mut self, code: &[u8], end: usize, data: *mut u8) -> Result<(), Stop> {
        while self.pc < end.min(code.len()) {
            match self.take_u8(code) {
                0 => self.op_box()?,
                1 => self.op_copy(code)?,
                2 => self.op_sized(code, data)?,
                4 => self.op_relative(code, data)?,
                5 => self.op_screen(code, data, false)?,
                6 => self.op_screen(code, data, true)?,
                7 => {
                    // SAFETY: the element is set for the frame being run.
                    let size = unsafe { &*self.element }.size;
                    let by_height = self.unit[1] * size[0] < self.unit[0] * size[1];
                    self.op_screen(code, data, by_height)?
                }
                8 => self.op_span(code, data)?,
                9 => self.op_between(code, data)?,
                10 => self.op_along(code, data)?,
                11 => self.op_basis(code, data)?,
                12 => self.op_turn(code, data),
                13 => self.op_into(code),
                op => return Err(Stop::Unported(op)),
            }
        }
        Ok(())
    }

    /// Opcode 0 (`0x2f1d90`), no operands: a transform the size's own diagonal, at the origin.
    fn op_box(&mut self) -> Result<(), Stop> {
        let i = self.next()?;
        self.transforms[i] = Transform { basis: diagonal(self.sizes[i]), origin: [0.0; 4] };
        Ok(())
    }

    /// Opcode 1 (`0x2f1dd0`): copies of existing transforms.
    fn op_copy(&mut self, code: &[u8]) -> Result<(), Stop> {
        for _ in 0..self.take_u8(code) {
            let src = self.take_u16(code) as usize;
            let i = self.next()?;
            self.transforms[i] = self.transforms[src.min(self.transforms.len() - 1)];
        }
        Ok(())
    }

    fn operands5(&mut self, code: &[u8], data: *const u8) -> (usize, V4) {
        let parent = (self.take_u16(code) as usize).min(self.transforms.len() - 1);
        let v = std::array::from_fn(|_| {
            let o = self.take_u16(code);
            self.data_f32(data, o)
        });
        (parent, v)
    }

    /// Opcode 2 (`0x2f1e40`): the new size in the element's units (times the reciprocal size
    /// at `+0x64a8`), placed by the parent's point `(v0, v1)` and the new transform's own point
    /// `(v2, v3)`.
    fn op_sized(&mut self, code: &[u8], data: *const u8) -> Result<(), Stop> {
        let inv = [self.unit[2], self.unit[2], self.unit[3], self.unit[3]];
        for _ in 0..self.take_u8(code) {
            let (parent, v) = self.operands5(code, data);
            let i = self.next()?;
            let s = self.sizes[i];
            let basis = diagonal([s[0] * inv[0], s[1] * inv[1], s[2] * inv[2], s[3] * inv[3]]);
            let at = point(&self.transforms[parent], v[0], v[1]);
            self.transforms[i] = Transform { basis, origin: place(at, basis, v[2], v[3]) };
        }
        Ok(())
    }

    /// Opcodes 5 and 6 (`0x2f21b0`, `0x2f23f0`): a size kept square on the screen. The new
    /// basis follows one of the parent's axes (5 the x axis and the parent's width, 6 the y
    /// axis and its height) and builds the other at right angles, scaled by the topology's
    /// axis lengths. Opcode 7 (`0x2f2630`) picks 6 when the instance is narrower than its
    /// element. Only a plane topology is ported.
    fn op_screen(&mut self, code: &[u8], data: *const u8, by_height: bool) -> Result<(), Stop> {
        let (r, d) = (self.topology.right, self.topology.down);
        let len = |v: [f32; 3]| ((0.0 * 0.0 + v[1] * v[1]) + (v[2] * v[2] + v[0] * v[0])).sqrt();
        let (lr, ld) = (len(r), len(d));
        for _ in 0..self.take_u8(code) {
            let (parent, v) = self.operands5(code, data);
            let i = self.next()?;
            let (s, ps, p) = (self.sizes[i], self.sizes[parent], self.transforms[parent]);
            let b = [p.basis[0] * lr, p.basis[1] * ld, p.basis[2] * lr, p.basis[3] * ld];
            let (num, den) = if by_height {
                ([s[0] * b[3], s[1] * b[2], s[2] * b[2], s[3] * b[3]], [ps[3] * lr, -(ps[3] * ld), ps[3] * lr, ps[3] * ld])
            } else {
                ([s[0] * b[0], s[1] * b[1], s[2] * b[1], s[3] * b[0]], [ps[0] * lr, ps[0] * ld, -(ps[0] * lr), ps[0] * ld])
            };
            let basis = std::array::from_fn(|k| if den[k] != 0.0 { num[k] / den[k] } else { 0.0 });
            let at = point(&p, v[0], v[1]);
            self.transforms[i] = Transform { basis, origin: place(at, basis, v[2], v[3]) };
        }
        Ok(())
    }

    /// Opcode 13 (`0x2f30f0`): transforms `[first, end)`, made in the unit square, carried
    /// into a parent's: each basis and origin through the parent's basis over its size, then
    /// the parent's origin added. Operands per entry: the parent, first, end.
    fn op_into(&mut self, code: &[u8]) {
        for _ in 0..self.take_u8(code) {
            let p = (self.take_u16(code) as usize).min(self.transforms.len() - 1);
            let (first, end) = (self.take_u16(code) as usize, self.take_u16(code) as usize);
            let r = recip(maxps(TINY, self.sizes[p]));
            let t = self.transforms[p];
            let k: V4 = std::array::from_fn(|i| r[i] * t.basis[i]);
            let (a, b) = ([k[0], k[3], k[0], k[3]], [k[2], k[1], k[2], k[1]]);
            let turn = |v: V4| -> V4 {
                let w = [v[1], v[0], v[3], v[2]];
                std::array::from_fn(|i| w[i] * b[i] + v[i] * a[i])
            };
            for i in first..end.min(self.transforms.len()) {
                let basis = turn(self.transforms[i].basis);
                let o = turn(self.transforms[i].origin);
                self.transforms[i] = Transform { basis, origin: std::array::from_fn(|k| o[k] + t.origin[k]) };
            }
        }
    }

    /// Opcode 8 (`0x2f2670`): an upright box from A's point `(a0, a1)` to B's `(b0, b1)`,
    /// scaled by `(c0, c1)`, its point `(d0, d1)` on A's point. Operands as opcode 9's.
    fn op_span(&mut self, code: &[u8], data: *const u8) -> Result<(), Stop> {
        for _ in 0..self.take_u8(code) {
            let (a, pa) = self.operands2(code, data);
            let (b, pb) = self.operands2(code, data);
            let d = [self.take_u16(code), self.take_u16(code)].map(|o| self.data_f32(data, o));
            let c = [self.take_u16(code), self.take_u16(code)].map(|o| self.data_f32(data, o));
            let i = self.next()?;
            let at = point(&self.transforms[a], pa[0], pa[1]);
            let to = point(&self.transforms[b], pb[0], pb[1]);
            let scale = [c[0], 0.0, 0.0, c[1]];
            let basis: V4 = std::array::from_fn(|k| (to[k] - at[k]) * scale[k]);
            self.transforms[i] = Transform { basis, origin: place(at, basis, d[0], d[1]) };
        }
        Ok(())
    }

    /// Opcode 9 (`0x2f27d0`): a box laid along the line between two parents' points, A's
    /// `(a0, a1)` and B's `(b0, b1)`, measured in the element's units. Its own vector
    /// `(c0 · w, c1 · h)` turns onto that line, and its point `(d0, d1)` sits on A's point.
    /// Operands: A, a0, a1, B, b0, b1, d0, d1, c0, c1.
    fn op_between(&mut self, code: &[u8], data: *const u8) -> Result<(), Stop> {
        let size = [self.unit[0], self.unit[1], self.unit[0], self.unit[1]];
        let inv = [self.unit[2], self.unit[3], self.unit[2], self.unit[3]];
        for _ in 0..self.take_u8(code) {
            let (a, pa) = self.operands2(code, data);
            let (b, pb) = self.operands2(code, data);
            let d = [self.take_u16(code), self.take_u16(code)].map(|o| self.data_f32(data, o));
            let c = [self.take_u16(code), self.take_u16(code)].map(|o| self.data_f32(data, o));
            let i = self.next()?;
            let s = self.sizes[i];
            let q = [c[0] * s[0], c[0] * s[1], c[1] * s[2], c[1] * s[3]];
            let unit = [s[0] * inv[0], s[1] * inv[1], s[2] * inv[2], s[3] * inv[3]];
            let at = point(&self.transforms[a], pa[0], pa[1]);
            let to = point(&self.transforms[b], pb[0], pb[1]);
            let qq = q.map(|x| x * x);
            let len2 = maxps(TINY, [qq[3] + qq[0], qq[2] + qq[1], qq[1] + qq[2], qq[0] + qq[3]]);
            let r = recip(len2);
            let v: V4 = std::array::from_fn(|k| (to[k] - at[k]) * size[k] * q[k] * r[k]);
            let flip = [v[3], -v[2], -v[1], v[0]];
            let basis: V4 = std::array::from_fn(|k| (flip[k] + v[k]) * unit[k]);
            self.transforms[i] = Transform { basis, origin: place(at, basis, d[0], d[1]) };
        }
        Ok(())
    }

    /// Opcode 10 (`0x2f2a00`): opcode 9's box, its vector `q = (c0 · w, c1 · h)` along the line
    /// between the points `v`, with the part turned across the line scaled by `|q| / |v|` (a
    /// refined `rsqrtps`), so the box keeps its own height. Operands as opcode 9's.
    fn op_along(&mut self, code: &[u8], data: *const u8) -> Result<(), Stop> {
        let size = [self.unit[0], self.unit[1], self.unit[0], self.unit[1]];
        let inv = [self.unit[2], self.unit[3], self.unit[2], self.unit[3]];
        let sum = |x: V4| -> V4 { [x[3] + x[0], x[2] + x[1], x[1] + x[2], x[0] + x[3]] };
        for _ in 0..self.take_u8(code) {
            let (a, pa) = self.operands2(code, data);
            let (b, pb) = self.operands2(code, data);
            let d = [self.take_u16(code), self.take_u16(code)].map(|o| self.data_f32(data, o));
            let c = [self.take_u16(code), self.take_u16(code)].map(|o| self.data_f32(data, o));
            let i = self.next()?;
            let s = self.sizes[i];
            let q = [c[0] * s[0], c[0] * s[1], c[1] * s[2], c[1] * s[3]];
            let unit = [s[0] * inv[0], s[1] * inv[1], s[2] * inv[2], s[3] * inv[3]];
            let at = point(&self.transforms[a], pa[0], pa[1]);
            let to = point(&self.transforms[b], pb[0], pb[1]);
            let r = recip(maxps(TINY, sum(q.map(|x| x * x))));
            let v: V4 = std::array::from_fn(|k| (to[k] - at[k]) * size[k]);
            let qv: V4 = std::array::from_fn(|k| q[k] * v[k]);
            let ratio = maxps(TINY, std::array::from_fn(|k| sum(v.map(|x| x * x))[k] * r[k]));
            let e = rsqrtps(ratio);
            let y: V4 = std::array::from_fn(|k| (1.5 - ratio[k] * 0.5 * e[k] * e[k]) * e[k]);
            let turned = [y[0], -y[1], -y[2], y[3]];
            let rev = [qv[3], qv[2], qv[1], qv[0]];
            let basis: V4 = std::array::from_fn(|k| ((turned[k] * rev[k] + qv[k]) * r[k]) * unit[k]);
            self.transforms[i] = Transform { basis, origin: place(at, basis, d[0], d[1]) };
        }
        Ok(())
    }

    /// Opcode 11 (`0x2f2c60`): a box from three parents' points P, Q and R (A's `(a0, a1)`,
    /// B's, C's): its x axis `k3 · (Q − P) + k1 · (R − P)`, its y axis `k2 · (Q − P) + k0 · (R −
    /// P)`, its point `(d0, d1)` on P. A basis with a lane not finite (`andnps` of the sign,
    /// `cmpltps` infinity) is zeroed, its origin left as it was, and hides the instance this
    /// frame (`+0x49`). Operands: A, a0, a1, B, b0, b1, C, c0, c1, d0, d1, k0, k1, k2, k3.
    fn op_basis(&mut self, code: &[u8], data: *const u8) -> Result<(), Stop> {
        for _ in 0..self.take_u8(code) {
            let (a, pa) = self.operands2(code, data);
            let (b, pb) = self.operands2(code, data);
            let (c, pc) = self.operands2(code, data);
            let d = [self.take_u16(code), self.take_u16(code)].map(|o| self.data_f32(data, o));
            let k = [self.take_u16(code), self.take_u16(code), self.take_u16(code), self.take_u16(code)].map(|o| self.data_f32(data, o));
            let i = self.next()?;
            let p = point(&self.transforms[a], pa[0], pa[1]);
            let q = point(&self.transforms[b], pb[0], pb[1]);
            let r = point(&self.transforms[c], pc[0], pc[1]);
            let (kq, kr) = ([k[3], k[3], k[2], k[2]], [k[1], k[1], k[0], k[0]]);
            let basis: V4 = std::array::from_fn(|n| (q[n] - p[n]) * kq[n] + (r[n] - p[n]) * kr[n]);
            if basis.iter().all(|v| v.abs() < f32::INFINITY) {
                self.transforms[i] = Transform { basis, origin: place(p, basis, d[0], d[1]) };
            } else {
                self.transforms[i].basis = [0.0; 4];
                self.hide_now = true;
            }
        }
        Ok(())
    }

    /// Opcode 12 (`0x2f2ea0`): a transform turned in place by an angle in turns about its own
    /// point `(p0, p1)`, which stays put. Only the angle's fraction counts (none beyond ±2³¹);
    /// cosine and sine are the engine's sine of `4 · (f − 0.75)` and `4 · f` quarter turns. The
    /// axis turned across is scaled by the element's aspect (`+0x64a0`), so the turn is round
    /// on the screen. Operands: the transform, the angle, p0, p1.
    fn op_turn(&mut self, code: &[u8], data: *const u8) {
        let u = self.unit;
        let m = [-(u[1] * u[2]), u[0] * u[3], -(u[1] * u[2]), u[0] * u[3]];
        for _ in 0..self.take_u8(code) {
            let t = (self.take_u16(code) as usize).min(self.transforms.len() - 1);
            let [a, p0, p1] = [self.take_u16(code), self.take_u16(code), self.take_u16(code)].map(|o| self.data_f32(data, o));
            let f = if a.abs() <= 2_147_483_648.0 { a - cvttps2dq(a) as f32 } else { 0.0 };
            let s = super::draw::sin_quarter_turns([(f + -0.75) * 4.0, (f + 0.0) * 4.0, (0.0 + 0.25) * 4.0, (0.0 + 0.0) * 4.0]);
            let Transform { basis: b, origin: o } = self.transforms[t];
            let swapped = [b[1], b[0], b[3], b[2]];
            let basis: V4 = std::array::from_fn(|k| swapped[k] * s[1] * m[k] + s[0] * b[k]);
            let pivot = [p0, p0, p1, p1];
            let d: V4 = std::array::from_fn(|k| pivot[k] * (b[k] - basis[k]));
            let origin = [(d[2] + d[0]) + o[0], (d[3] + d[1]) + o[1], (d[0] + d[2]) + o[2], (d[1] + d[3]) + o[3]];
            self.transforms[t] = Transform { basis, origin };
        }
    }

    /// A parent and the data offsets of its point.
    fn operands2(&mut self, code: &[u8], data: *const u8) -> (usize, [f32; 2]) {
        let parent = (self.take_u16(code) as usize).min(self.transforms.len() - 1);
        let p = [self.take_u16(code), self.take_u16(code)].map(|o| self.data_f32(data, o));
        (parent, p)
    }

    /// Opcode 4 (`0x2f2070`): the parent's basis scaled by the ratio of the sizes, a lane
    /// cleared where the parent's size is zero (`divps`, `mulps`, then `cmpneqps` mask).
    fn op_relative(&mut self, code: &[u8], data: *const u8) -> Result<(), Stop> {
        for _ in 0..self.take_u8(code) {
            let (parent, v) = self.operands5(code, data);
            let i = self.next()?;
            let (s, ps, p) = (self.sizes[i], self.sizes[parent], self.transforms[parent]);
            let basis = std::array::from_fn(|k| if ps[k] != 0.0 { s[k] / ps[k] * p.basis[k] } else { 0.0 });
            let at = point(&p, v[0], v[1]);
            self.transforms[i] = Transform { basis, origin: place(at, basis, v[2], v[3]) };
        }
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
mod tests {
    use super::*;
    use crate::rui::code::sys;

    const IMAGE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/dumps/current.bin");
    /// The pages the handlers run from and read: their code, then their constants.
    const PAGES: [(usize, usize); 4] = [(0x2f2000, 0x2f4000), (0x1300000, 0x1314000), (0x15bc000, 0x15bf000), (0x23fea000, 0x23feb000)];
    const SPAN: usize = 0x23feb000;
    const DATA: usize = 0x80;

    /// The engine's scratch as the handlers address it: the code pointer at `+0`, the count at
    /// `+8`, transforms from `+0x4b40`, the unit size at `+0x64a0`.
    #[repr(C, align(16))]
    struct Scratch([u8; 0x6500]);

    #[repr(C, align(16))]
    struct Instance([u8; 0x60 + DATA]);

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn float(&mut self) -> f32 {
            match self.next() % 16 {
                0 => 0.0,
                1 => -0.0,
                2 => 3.0e38,
                _ => (self.next() % 20_000) as f32 / 1000.0 - 10.0,
            }
        }
    }

    fn same(a: V4, b: V4) -> bool {
        a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits() || x.is_nan() && y.is_nan())
    }

    /// Runs `code` (an opcode and its operands) through the engine's handler and through ours
    /// from the same transforms, unit size and data, and compares what they leave.
    fn compare(handler: usize, base: *mut u8, code: &[u8], rng: &mut Rng) {
        let mut ours = Frame::default();
        let mut scratch = Box::new(Scratch([0; 0x6500]));
        let mut inst = Instance([0; 0x60 + DATA]);
        for i in 0..8 {
            let t = Transform { basis: std::array::from_fn(|_| rng.float()), origin: std::array::from_fn(|_| rng.float()) };
            ours.transforms[i] = t;
            let at = 0x4b40 + 0x20 * i;
            for (k, v) in t.basis.iter().chain(&t.origin).enumerate() {
                scratch.0[at + 4 * k..at + 4 * k + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        let (w, h) = (rng.float().abs() + 1.0, rng.float().abs() + 1.0);
        ours.unit = [w, h, 1.0 / w, 1.0 / h];
        for (k, v) in ours.unit.iter().enumerate() {
            scratch.0[0x64a0 + 4 * k..0x64a4 + 4 * k].copy_from_slice(&v.to_le_bytes());
        }
        for k in 0..DATA / 4 {
            inst.0[0x60 + 4 * k..0x64 + 4 * k].copy_from_slice(&rng.float().to_le_bytes());
        }
        ours.count = 4;
        ours.data_size = DATA;
        scratch.0[8..12].copy_from_slice(&4u32.to_le_bytes());
        scratch.0[0..8].copy_from_slice(&(code[1..].as_ptr() as u64).to_le_bytes());
        let scratch_ptr = scratch.0.as_mut_ptr();
        inst.0[0x18..0x20].copy_from_slice(&(scratch_ptr as u64).to_le_bytes());
        // SAFETY: the handler is the engine's own, laid out at its RVA with the pages it reads;
        // it reads the instance, its data block and the scratch, all live and sized for it.
        unsafe {
            let f = std::mem::transmute::<*mut u8, extern "win64" fn(*mut u8)>(base.add(handler));
            f(inst.0.as_mut_ptr());
        }
        let data = inst.0[0x60..].as_mut_ptr();
        ours.run(code, code.len(), data).unwrap();
        let count = u32::from_le_bytes(scratch.0[8..12].try_into().unwrap());
        assert_eq!(count, ours.count);
        assert_eq!(u64::from_le_bytes(scratch.0[0..8].try_into().unwrap()), code.as_ptr() as u64 + ours.pc as u64);
        assert_eq!(inst.0[0x49] != 0, ours.hide_now, "{code:?}");
        for i in 0..count as usize {
            let at = 0x4b40 + 0x20 * i;
            let read = |o: usize| -> V4 { std::array::from_fn(|k| f32::from_le_bytes(scratch.0[at + o + 4 * k..at + o + 4 * k + 4].try_into().unwrap())) };
            let t = ours.transforms[i];
            assert!(same(read(0), t.basis) && same(read(16), t.origin), "transform {i} of {code:?}: engine {:?} {:?}, ours {t:?}", read(0), read(16));
        }
    }

    #[test]
    fn opcodes_11_and_12_match_the_engines_handlers() {
        let Ok(image) = std::fs::read(IMAGE) else { return };
        let base = sys::alloc(SPAN).unwrap();
        for (from, to) in PAGES {
            // SAFETY: the pages lie inside the fresh mapping.
            unsafe { std::ptr::copy_nonoverlapping(image[from..to].as_ptr(), base.add(from), to - from) };
        }
        // SAFETY: the code pages are part of the mapping.
        assert!(unsafe { sys::protect(base.add(PAGES[0].0), PAGES[0].1 - PAGES[0].0, sys::READ | sys::EXEC) });
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let offset = |rng: &mut Rng| ((rng.next() % (DATA as u64 / 4)) * 4) as u16;
        for _ in 0..2000 {
            let mut code = vec![11, 2];
            for _ in 0..2 {
                for slot in 0..15 {
                    let v = if slot % 3 == 0 && slot < 9 { (rng.next() % 4) as u16 } else { offset(&mut rng) };
                    code.extend(v.to_le_bytes());
                }
            }
            compare(0x2f2c60, base, &code, &mut rng);
            let mut code = vec![12, 2];
            for _ in 0..2 {
                code.extend(((rng.next() % 4) as u16).to_le_bytes());
                for _ in 0..3 {
                    code.extend(offset(&mut rng).to_le_bytes());
                }
            }
            compare(0x2f2ea0, base, &code, &mut rng);
        }
        // SAFETY: nothing runs from the mapping any more.
        unsafe { sys::free(base, SPAN) };
    }
}
