//! Bone setup: how the game turns a sequence at a cycle into bone transforms. This is the
//! studio library's code in the oracle (RVAs in `docs/feedback.md`, Bone setup): decoding the
//! compressed animation tracks, building a local pose, blending poses into each other, and
//! concatenating the hierarchy. The float forms are the oracle's, including its own sine and
//! cosine and the hardware reciprocal square root it normalises quaternions with.
//!
//! Not modelled: IK rules and locks, world-space blending (`STUDIO_WORLD`), procedural bones,
//! the pose cache, looping pose parameters (none in the rigs read), and the time that
//! real-time sequences and layers take their cycle from (they play at the cycle given).

use crate::assets::model::{hdr, Blob, Model, PoseParam, Seq, Studio};

pub type V3 = [f32; 3];
pub type Quat = [f32; 4];

/// Sequence flags (seqdesc +0xc) and animation flags (animdesc +0xc).
pub const STUDIO_LOOPING: u32 = 0x1;
pub const STUDIO_SNAP: u32 = 0x2;
pub const STUDIO_DELTA: u32 = 0x4;
pub const STUDIO_POST: u32 = 0x10;
pub const STUDIO_ALLZEROS: u32 = 0x20;
pub const STUDIO_CYCLEPOSE: u32 = 0x80;
pub const STUDIO_REALTIME: u32 = 0x100;
pub const STUDIO_WORLD: u32 = 0x4000;
pub const STUDIO_ANIM_DATA: u32 = 0x20000;

/// Fixed-point scales of rotation and scale tracks (0x15bb274, 0x15bb2c4).
const ROT_SCALE: f32 = f32::from_bits(0x3949_116d);
const SCALE_SCALE: f32 = f32::from_bits(0x3b48_0190);
/// Quaternion64's component scale (0x15bb224): just under 2^-20.
const QUAT64_SCALE: f32 = f32::from_bits(0x357f_fff8);

/// How far a run of animation values reaches, by its kind: (base, add, per frame), in 16-bit
/// words, the per-frame term in sixteenths (0x172d9f0).
const SEEK: [[i8; 3]; 20] = [
    [1, 15, 16],
    [2, 7, 8],
    [2, 15, 0],
    [3, 15, 0],
    [4, 15, 0],
    [5, 15, 0],
    [6, 15, 0],
    [7, 15, 0],
    [2, 15, 2],
    [3, 15, 2],
    [4, 15, 2],
    [5, 15, 2],
    [6, 15, 2],
    [7, 15, 2],
    [2, 15, 4],
    [3, 15, 4],
    [4, 15, 4],
    [5, 15, 4],
    [6, 15, 4],
    [7, 15, 4],
];
/// Bits per frame of a run's packed adjustments, and the bias they carry (0x12fc154, 0x12fed60).
const FRAME_BITS: [i8; 4] = [0, 2, 4, 0];
const FRAME_BIAS: [f32; 4] = [0.0, 3.0, 15.0, 0.0];

#[derive(Clone, Debug, Default)]
pub struct Pose {
    pub pos: Vec<V3>,
    pub q: Vec<Quat>,
    pub scale: Vec<V3>,
    pub rot: Vec<V3>,
}

impl Pose {
    pub fn new(n: usize) -> Pose {
        Pose { pos: vec![[0.0; 3]; n], q: vec![[0.0, 0.0, 0.0, 1.0]; n], scale: vec![[1.0; 3]; n], rot: vec![[0.0; 3]; n] }
    }
}

/// The bones a pose is computed for, ascending, with a mask for membership.
#[derive(Clone, Debug, Default)]
pub struct BoneList {
    pub bones: Vec<u8>,
    pub mask: [u32; 8],
}

impl BoneList {
    pub fn all(n: usize) -> BoneList {
        let mut l = BoneList::default();
        for b in 0..n.min(256) {
            l.push(b as u8);
        }
        l
    }
    fn push(&mut self, b: u8) {
        self.bones.push(b);
        self.mask[(b >> 5) as usize] |= 1 << (b & 31);
    }
    pub fn has(&self, b: i32) -> bool {
        (0..256).contains(&b) && self.mask[(b >> 5) as usize] >> (b & 31) & 1 != 0
    }
}

/// One studio header a sequence's bones are indexed in, mapped onto the model's bones by name
/// as the engine's virtual model maps an included rig.
#[derive(Clone, Debug)]
pub struct Group {
    pub studio: Studio,
    pub seqs: Vec<Seq>,
    /// For each model bone, its index in this group, or -1.
    pub to_group: Vec<i32>,
    /// For each group bone, the model bone, or -1.
    pub to_model: Vec<i32>,
    /// For each of the group's pose parameters, the model's, or -1.
    pub to_pose: Vec<i32>,
}

/// A model with its rigs: the bones, and every sequence the model can play, numbered as the
/// game numbers them (the model's own `ref` first, then each rig's in order).
#[derive(Clone, Debug, Default)]
pub struct Skeleton {
    pub names: Vec<String>,
    pub parents: Vec<i32>,
    pub groups: Vec<Group>,
    /// (group, index in group); `None` for the model's own `ref`, which holds no animation.
    pub seqs: Vec<Option<(usize, usize)>>,
    /// The pose parameters, the model's own then each rig's not yet named, as the virtual
    /// model shares them.
    pub pose_params: Vec<PoseParam>,
}

impl Skeleton {
    pub fn new(m: &Model) -> Skeleton {
        let bones = m.studio.bones();
        let names: Vec<String> = bones.iter().map(|b| b.name.clone()).collect();
        let parents = bones.iter().map(|b| b.parent).collect();
        let mut seqs = vec![None; m.studio.0.i32(hdr::NUMLOCALSEQ) as usize];
        let mut groups = Vec::new();
        let mut pose_params = m.studio.pose_params();
        for (g, r) in m.rigs.iter().enumerate() {
            let mut to_pose = Vec::new();
            for p in r.studio.pose_params() {
                let i = pose_params.iter().position(|q| q.name.eq_ignore_ascii_case(&p.name)).unwrap_or_else(|| {
                    pose_params.push(p);
                    pose_params.len() - 1
                });
                to_pose.push(i as i32);
            }
            let rb = r.studio.bones();
            let find = |n: &str| rb.iter().position(|b| b.name.eq_ignore_ascii_case(n)).map_or(-1, |i| i as i32);
            let to_group = names.iter().map(|n| find(n)).collect();
            let to_model = rb.iter().map(|b| names.iter().position(|n| n.eq_ignore_ascii_case(&b.name)).map_or(-1, |i| i as i32)).collect();
            seqs.extend((0..r.seqs.len()).map(|i| Some((g, i))));
            groups.push(Group { studio: r.studio.clone(), seqs: r.seqs.clone(), to_group, to_model, to_pose });
        }
        Skeleton { names, parents, groups, seqs, pose_params }
    }

    /// The pose parameter named `name`.
    pub fn pose_param(&self, name: &str) -> Option<usize> {
        self.pose_params.iter().position(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Studio_SetPoseParameter (0x1d4800) for a parameter that does not loop: `value` placed in
    /// its range, clamped, as the entity keeps it.
    pub fn set_pose_parameter(&self, i: usize, value: f32) -> f32 {
        let Some(p) = self.pose_params.get(i) else { return 0.0 };
        if p.end == p.start {
            return 0.0;
        }
        let v = (value - p.start) / (p.end - p.start);
        let v = if v <= 0.0 { 0.0 } else { v };
        if 1.0 <= v { 1.0 } else { v }
    }

    /// Every pose parameter at 0, as the entities lurch animates keep them unless set.
    pub fn default_poses(&self) -> Vec<f32> {
        (0..self.pose_params.len()).map(|i| self.set_pose_parameter(i, 0.0)).collect()
    }

    /// The sequence an auto-layer names by its asset GUID.
    fn seq_by_guid(&self, guid: u64) -> Option<usize> {
        (0..self.seqs.len()).find(|&s| self.seq(s).is_some_and(|(_, q)| q.guid == guid))
    }

    pub fn seq(&self, seq: usize) -> Option<(&Group, &Seq)> {
        let (g, i) = (*self.seqs.get(seq)?)?;
        Some((&self.groups[g], &self.groups[g].seqs[i]))
    }

    /// The model's sequence number with this label (the part of its path after the last `/`,
    /// without `.rseq`).
    pub fn find(&self, label: &str) -> Option<usize> {
        (0..self.seqs.len()).find(|&s| self.seq(s).is_some_and(|(_, q)| seq_label(q) == label))
    }
}

pub fn seq_label(s: &Seq) -> &str {
    let l = s.desc.cstr(s.desc.rel(0, 4));
    let l = l.rsplit(['/', '\\']).next().unwrap_or(l);
    l.strip_suffix(".rseq").unwrap_or(l)
}

/// A sequence's flags.
pub fn seq_flags(s: &Seq) -> u32 {
    s.desc.u32(0xc)
}

/// The animation for blend position (x, y): its animdesc's offset in the sequence (0x1c8ad0).
pub fn anim_desc(s: &Seq, x: i32, y: i32) -> usize {
    let d = &s.desc;
    let (gx, gy) = (d.i32(0x44), d.i32(0x48));
    let x = if gx <= x { gx - 1 } else { x };
    let y = if gy <= y { gy - 1 } else { y };
    d.i32(d.i32(0x3c) as usize + (gx * y + x) as usize * 4) as usize
}

/// Cycles per second at rate 1 (Studio_CPS, 0x1d6000): the first animation's frame rate over
/// its frames less one, none for a single frame.
pub fn cycle_rate(s: &Seq) -> f32 {
    let a = anim_desc(s, 0, 0);
    let (fps, frames) = (s.desc.f32(a + 8), s.desc.i32(a + 0x10));
    if frames <= 1 { 0.0 } else { fps / (frames - 1) as f32 * 1.0 + 0.0 }
}

/// Seconds at rate 1 (Studio_Duration, 0x1d6100).
pub fn duration(s: &Seq) -> f32 {
    let r = cycle_rate(s);
    if r == 0.0 { 0.0 } else { 1.0 / r }
}

/// ClampCycle (0xb4cfb0): a looping cycle wrapped into [0, 1), any other held in [0, 0.999].
pub fn clamp_cycle(c: f32, looping: bool) -> f32 {
    if looping {
        let d = c as f64;
        let c = (d - (d as i32) as f64) as f32;
        if 0.0 > c { c + 1.0 } else { c }
    } else if 0.0 > c {
        0.0
    } else {
        let hold = f32::from_bits(0x3f7f_be77);
        if hold < c { hold } else { c }
    }
}

/// Where the next run of values starts, in bytes from this one (the table at 0x172d9f0).
fn next_run(d: &Blob, at: usize) -> usize {
    let (valid, total) = (d.u8(at) as usize, d.u8(at + 1) as i32);
    let [base, add, per] = SEEK[valid.min(19)];
    let v = per as i32 * total + add as i32;
    (((v + (v >> 31 & 15)) >> 4) + base as i32) as usize * 2
}

/// The run holding `frame`, and the frame within it.
fn seek(d: &Blob, mut at: usize, mut frame: i32) -> (usize, i32) {
    while d.u8(at + 1) as i32 <= frame {
        frame -= d.u8(at + 1) as i32;
        at += next_run(d, at);
    }
    (at, frame)
}

/// One value of a run at `frame` (0x1c8c30).
fn extract(d: &Blob, at: usize, frame: i32, scale: f32) -> f32 {
    let valid = d.u8(at);
    let word = |i: usize| d.i16(at + 2 + 2 * i);
    match valid {
        0 => word(frame as usize) as i32 as f32 * scale,
        1 => {
            let mut v = word(0);
            if 0 < frame {
                v = v.wrapping_add(d.u8(at + frame as usize + 3) as i8 as i16);
            }
            v as i32 as f32 * scale
        }
        _ => {
            let n = valid as i32 - 2;
            let (bits, extra) = ((n / 6) as usize, (n % 6) as usize);
            let inv = 1.0 / (d.u8(at + 1) as i32 - 1) as f32;
            let mut value = word(0) as i32 as f32;
            if extra != 0 {
                let c = frame as f32 * inv;
                let mut f = 1.0f32;
                for k in 1..=extra {
                    f *= c;
                    value += word(k) as i32 as f32 * f;
                }
            }
            if bits != 0 {
                let nb = FRAME_BITS[bits] as i32;
                let off = nb * frame;
                let w = d.u16(at + 4 + 2 * (extra + (off >> 4) as usize)) as u32;
                let v = (w >> (off & 15)) & ((1u32 << nb) - 1);
                value = (v * 2) as i32 as f32 + (value - FRAME_BIAS[bits]);
            }
            value * scale
        }
    }
}

/// The values at `frame` and the frame after, the second from the next run at a run's end
/// (0x1c8e50).
fn extract2(d: &Blob, at: usize, frame: i32, scale: f32) -> (f32, f32) {
    let (at, k) = seek(d, at, frame);
    let total = d.u8(at + 1) as i32;
    if total - 1 <= k {
        return (extract(d, at, k, scale), extract(d, at + next_run(d, at), 0, scale));
    }
    if d.u8(at) == 1 {
        let base = d.i16(at + 2);
        let v2 = (d.u8(at + k as usize + 4) as i8 as i32 + base as i32) as f32 * scale;
        return (extract(d, at, k, scale), v2);
    }
    (extract(d, at, k, scale), extract(d, at, k + 1, scale))
}

fn extract1(d: &Blob, at: usize, frame: i32, scale: f32) -> f32 {
    let (at, k) = seek(d, at, frame);
    extract(d, at, k, scale)
}

/// Half-precision to single, exactly (the SSE conversion in 0x1c97b0 and 0x1c9b40).
pub fn half(h: u16) -> f32 {
    let sign = (h as u32 & 0x8000) << 16;
    let em = h as u32 & 0x7fff;
    let bits = if em < 0x400 {
        (f32::from_bits((em << 13) + 0x3880_0000) - f32::from_bits(0x3880_0000)).to_bits()
    } else if em >= 0x7c00 {
        (em << 13) + 0x7000_0000
    } else {
        (em << 13) + 0x3800_0000
    };
    f32::from_bits(bits | sign)
}

/// The game's SIMD sine and cosine of half an angle (inside 0x42e940): Cody-Waite reduction by
/// quarter turns and two polynomials.
fn sincos_half(a: f32) -> (f32, f32) {
    let h = a * 0.5;
    let q = (f32::from_bits(0x3f22_f983) * h).round_ties_even() as i32;
    let qf = q as f32;
    let r = ((h - f32::from_bits(0x3fc9_1000) * qf) - f32::from_bits(0xb695_7000) * qf) - f32::from_bits(0xb06f_4b9e) * qf;
    let r2 = r * r;
    let sin = ((f32::from_bits(0xb94d_6102) * r2 + f32::from_bits(0x3c08_85d2)) * r2 + f32::from_bits(0xbe2a_aaa8)) * (r2 * r) + r;
    let cos =
        (((f32::from_bits(0x37cf_14c2) * r2 + f32::from_bits(0xbab6_0b22)) * r2 + f32::from_bits(0x3d2a_aaa9)) * r2 + -0.5) * r2 + 1.0;
    let even = q & 1 == 0;
    let s = if even { sin } else { cos };
    let c = if even { cos } else { sin };
    let s = f32::from_bits(s.to_bits() ^ (((q & 2) as u32) << 30));
    let c = f32::from_bits(c.to_bits() ^ ((((q + 1) & 2) as u32) << 30));
    (s, c)
}

/// AngleQuaternion (0x42e940): RadianEuler (roll, pitch, yaw) to a quaternion.
pub fn angle_quaternion(a: V3) -> Quat {
    let (sr, cr) = sincos_half(a[0]);
    let (sp, cp) = sincos_half(a[1]);
    let (sy, cy) = sincos_half(a[2]);
    let sr_cp = cp * sr;
    let cr_sp = cr * sp;
    let cr_cp = cp * cr;
    let sr_sp = sp * sr;
    [cy * sr_cp - sy * cr_sp, cy * cr_sp + sy * sr_cp, sy * cr_cp - cy * sr_sp, cy * cr_cp + sy * sr_sp]
}

/// rsqrtps: the hardware's approximate reciprocal square root, as the game gets it.
#[cfg(target_arch = "x86_64")]
fn rsqrt4(v: [f32; 4]) -> [f32; 4] {
    use std::arch::x86_64::{_mm_loadu_ps, _mm_rsqrt_ps, _mm_storeu_ps};
    let mut out = [0.0f32; 4];
    // SAFETY: SSE is part of the x86_64 baseline; both pointers are to four floats.
    unsafe { _mm_storeu_ps(out.as_mut_ptr(), _mm_rsqrt_ps(_mm_loadu_ps(v.as_ptr()))) };
    out
}

#[cfg(not(target_arch = "x86_64"))]
fn rsqrt4(v: [f32; 4]) -> [f32; 4] {
    v.map(|x| 1.0 / x.sqrt())
}

/// The four lanes of a horizontal sum as the SIMD code forms them: pairs, then reversed pairs.
fn hsum(p: [f32; 4]) -> [f32; 4] {
    let a = [p[1] + p[0], p[0] + p[1], p[3] + p[2], p[2] + p[3]];
    [a[3] + a[0], a[2] + a[1], a[1] + a[2], a[0] + a[3]]
}

/// q scaled to unit length with one Newton step on rsqrtps; `zero_keeps` leaves a zero-length
/// quaternion alone (0x1c9450), otherwise the length is floored at FLT_MIN (0x1d2dc0).
fn normalize(q: Quat, zero_keeps: bool) -> Quat {
    let mut len = hsum([q[0] * q[0], q[1] * q[1], q[2] * q[2], q[3] * q[3]]);
    if !zero_keeps {
        len = len.map(|l| l.max(f32::MIN_POSITIVE));
    }
    let r = rsqrt4(len);
    let mut out = [0.0; 4];
    for i in 0..4 {
        let n = (3.0 - r[i] * len[i] * r[i]) * (r[i] * 0.5) * q[i];
        out[i] = if zero_keeps && len[i] == 0.0 { q[i] } else { n };
    }
    out
}

/// `±a * q1 + b * q2` with the sign of `a` taken per lane from dot(q2, q1), then normalised:
/// the blend in 0x1c9450 and 0x1d2dc0.
fn blend(q1: Quat, a: f32, q2: Quat, b: f32, zero_keeps: bool) -> Quat {
    let dot = hsum([q2[0] * q1[0], q2[1] * q1[1], q2[2] * q1[2], q2[3] * q1[3]]);
    let mut out = [0.0; 4];
    for i in 0..4 {
        let sa = f32::from_bits((dot[i].to_bits() & 0x8000_0000) ^ a.to_bits());
        out[i] = sa * q1[i] + b * q2[i];
    }
    normalize(out, zero_keeps)
}

/// CalcBoneQuaternion (0x1c9450): a bone's rotation at `frame` + `s`.
fn calc_bone_quaternion(frame: i32, s: f32, d: &Blob, rle: usize, cur: &mut usize, base: V3, q: &mut Quat) {
    let at = *cur;
    if d.u16(rle) >> 14 & 1 == 0 {
        let bits = d.u64(at);
        let c = |v: u64| ((v & 0x1f_ffff) as i32 - 0x10_0000) as f32 * QUAT64_SCALE;
        let (x, y, z) = (c(bits), c(bits >> 21), c(bits >> 42));
        let mut w = (1.0 - (y * y + x * x + z * z)).sqrt();
        if (bits as i64) < 0 {
            w = -w;
        }
        *q = [x, y, z, w];
        *cur = at + 8;
        return;
    }
    let vp = d.u16(at);
    let base_at = at + (vp & 0x1fff) as usize;
    let tracks = [base_at, base_at + d.u8(at + 2) as usize * 2, base_at + d.u8(at + 3) as usize * 2];
    *cur = at + 4;
    if s == 0.0 {
        let mut a = [0.0; 3];
        for j in 0..3 {
            a[j] = if vp >> (15 - j) & 1 != 0 { extract1(d, tracks[j], frame, ROT_SCALE) } else { base[j] };
        }
        *q = angle_quaternion(a);
        return;
    }
    let (mut a1, mut a2) = ([0.0; 3], [0.0; 3]);
    for j in 0..3 {
        if vp >> (15 - j) & 1 != 0 {
            (a1[j], a2[j]) = extract2(d, tracks[j], frame, ROT_SCALE);
        } else {
            (a1[j], a2[j]) = (base[j], base[j]);
        }
    }
    if a1 == a2 {
        *q = angle_quaternion(a1);
        return;
    }
    let q1 = angle_quaternion(a1);
    let q2 = angle_quaternion(a2);
    *q = blend(q2, s, q1, 1.0 - s, true);
}

/// CalcBonePosition (0x1c97b0) and CalcBoneScale (0x1c9b40): animated tracks add to what is
/// there, a fixed value replaces it.
#[allow(clippy::too_many_arguments)] // the oracle's own arguments, which both callers share
fn calc_bone_vector(frame: i32, s: f32, d: &Blob, rle: usize, cur: &mut usize, flag_bit: u32, scaled: bool, out: &mut V3) {
    let at = *cur;
    if (d.u16(rle) as u32) >> flag_bit & 1 == 0 {
        *out = [half(d.u16(at)), half(d.u16(at + 2)), half(d.u16(at + 4))];
        *cur = at + 6;
        return;
    }
    let (scale, vp_at) = if scaled { (d.f32(at), at + 4) } else { (SCALE_SCALE, at) };
    let vp = d.u16(vp_at);
    let base_at = vp_at + (vp & 0x1fff) as usize;
    let tracks = [base_at, base_at + d.u8(vp_at + 2) as usize * 2, base_at + d.u8(vp_at + 3) as usize * 2];
    *cur = vp_at + 4;
    for j in 0..3 {
        if vp >> (15 - j) & 1 == 0 {
            continue;
        }
        if s == 0.0 {
            out[j] += extract1(d, tracks[j], frame, scale);
        } else {
            let (v1, v2) = extract2(d, tracks[j], frame, scale);
            out[j] += (1.0 - s) * v1 + v2 * s;
        }
    }
}

/// CalcAnimation for a rig's sequence (0x1ca3b0): one animation of `seq` at `cycle` into
/// `pose`, for the bones in `bones`.
pub fn calc_animation(sk: &Skeleton, g: &Group, seq: &Seq, anim: usize, cycle: f32, bones: &BoneList, pose: &mut Pose) {
    let d = &seq.desc;
    let numframes = d.i32(anim + 0x10);
    let flags = d.u32(anim + 0xc);
    let f = (numframes - 1) as f32 * cycle;
    let mut frame = f as i32;
    let s = f - frame as f32;
    let st = &g.studio.0;
    let lin = st.i32(hdr::LINEARBONEINDEX) as usize;
    for &b in &bones.bones {
        let b = b as usize;
        if flags & STUDIO_DELTA != 0 {
            pose.q[b] = [0.0, 0.0, 0.0, 1.0];
            pose.pos[b] = [0.0; 3];
            pose.scale[b] = [1.0; 3];
            pose.rot[b] = [0.0; 3];
            continue;
        }
        let gb = g.to_group[b] as usize;
        if lin != 0 {
            let (pos_at, quat_at, rot_at) = (lin + st.i32(lin + 0xc) as usize, lin + st.i32(lin + 0x10) as usize, lin + st.i32(lin + 0x14) as usize);
            pose.q[b] = [st.f32(quat_at + gb * 16), st.f32(quat_at + gb * 16 + 4), st.f32(quat_at + gb * 16 + 8), st.f32(quat_at + gb * 16 + 12)];
            pose.pos[b] = st.v3(pos_at + gb * 12);
            pose.scale[b] = [1.0; 3];
            pose.rot[b] = st.v3(rot_at + gb * 12);
        } else {
            let o = st.i32(hdr::BONEINDEX) as usize + gb * hdr::BONE_SIZE;
            pose.q[b] = [st.f32(o + 0x2c), st.f32(o + 0x30), st.f32(o + 0x34), st.f32(o + 0x38)];
            pose.pos[b] = st.v3(o + 0x20);
            pose.scale[b] = st.v3(o + 0x48);
            pose.rot[b] = st.v3(o + 0x3c);
        }
    }
    if flags & STUDIO_ANIM_DATA == 0 {
        return;
    }
    let (sectionframes, mut index) = (d.i32(anim + 0x30), d.i32(anim + 0x20));
    if sectionframes != 0 {
        let section = if sectionframes < numframes && frame == numframes - 1 {
            let s = (numframes - 1) / sectionframes + 1;
            frame = 0;
            s
        } else {
            let s = frame / sectionframes;
            frame -= s * sectionframes;
            s
        };
        index = d.i32(anim + d.i32(anim + 0x2c) as usize + section as usize * 4);
    }
    let data = (anim as i64 + index as i64) as usize;
    let nbones = st.i32(hdr::NUMBONES);
    let mut rle = data + ((((nbones * 4 + 7) >> 3) + 1) & !1) as usize;
    let _ = sk;
    for gb in 0..nbones as usize {
        let nib = d.u8(data + gb / 2) >> ((gb & 1) * 4) & 7;
        if nib == 0 {
            continue;
        }
        let mb = g.to_model[gb];
        if bones.has(mb) {
            let mb = mb as usize;
            let mut cur = rle + 2;
            if nib & 1 != 0 {
                calc_bone_vector(frame, s, d, rle, &mut cur, 15, true, &mut pose.pos[mb]);
            }
            if nib & 2 != 0 {
                let base = pose.rot[mb];
                calc_bone_quaternion(frame, s, d, rle, &mut cur, base, &mut pose.q[mb]);
            }
            if nib & 4 != 0 {
                calc_bone_vector(frame, s, d, rle, &mut cur, 13, false, &mut pose.scale[mb]);
            }
        }
        rle += (d.u16(rle) & 0x1fff) as usize;
    }
}

/// The bones a sequence touches: those of `bones` with a weight above zero in it (0x1db160).
pub fn seq_bone_list(bones: &BoneList, g: &Group, seq: &Seq) -> BoneList {
    let d = &seq.desc;
    let weights = d.i32(0x9c) as usize;
    let mut out = BoneList::default();
    for &b in &bones.bones {
        let gb = g.to_group[b as usize];
        if gb >= 0 && 0.0 < d.f32(weights + gb as usize * 4) {
            out.push(b);
        }
    }
    out
}

/// Studio_LocalPoseParameter (0x1d44b0): where the entity's pose parameters (`poses`, each
/// normalised to its range) put sequence `s` along its blend axis `axis`: the grid index and
/// the fraction past it. Without a parameter on that axis, (0, 0).
fn local_pose_parameter(sk: &Skeleton, g: &Group, s: &Seq, poses: &[f32], axis: usize) -> (i32, f32) {
    let d = &s.desc;
    let local = d.i32(0x4c + 4 * axis);
    let shared = if local == -1 { -1 } else { g.to_pose.get(local as usize).copied().unwrap_or(-1) };
    let Some(p) = usize::try_from(shared).ok().and_then(|i| sk.pose_params.get(i)) else { return (0, 0.0) };
    let v = poses.get(shared as usize).copied().unwrap_or(0.0);
    let size = d.i32(0x44 + 4 * axis);
    let keys = d.i32(0xa0);
    if keys == 0 {
        let start = p.start;
        let range = p.end - start;
        let lo = (d.f32(0x54 + 4 * axis) - start) / range;
        let v = (v - lo) / ((d.f32(0x5c + 4 * axis) - start) / range - lo);
        let v = if v <= 0.0 { 0.0 } else { v };
        let v = if 1.0 <= v { 1.0 } else { v };
        let v = v * (size - 1) as f32;
        let i = v as i32;
        return (i, v - i as f32);
    }
    let v = (p.end - p.start) * v + p.start;
    let at = keys as usize + (d.i32(0x44) as usize * axis) * 4;
    let mut k1 = d.f32(at + 4);
    let mut s = (v - d.f32(at)) / (k1 - d.f32(at));
    let mut i = 0;
    while 1.0 <= s {
        if size - 2 <= i {
            return (i, 1.0);
        }
        i += 1;
        let k2 = d.f32(at + 4 + 4 * i as usize);
        s = (v - k1) / (k2 - k1);
        k1 = k2;
    }
    (i, if 0.0 <= s { s } else { 0.0 })
}

fn all_zeros(s: &Seq, x: i32, y: i32) -> bool {
    s.desc.u32(anim_desc(s, x, y) + 0xc) & STUDIO_ALLZEROS != 0
}

const BLEND_EPS: f32 = f32::from_bits(0x3400_0000);
const BLEND_ONE: f32 = f32::from_bits(0x3f7f_fffe);
/// A triangle corner weight under this is dropped (0x15bb26c).
const BLEND_DROP: f32 = f32::from_bits(0x38d1_b717);

/// CalcPose (0x1d8e40): the cycle wrapped or clamped as the sequence says (a cycle-pose
/// sequence takes it from its pose parameter), then the animation where the pose parameters
/// put the sequence on its blend grid: one grid point, two along an edge, or the three corners
/// of the grid cell's triangle the point is in, the cell cut along the diagonal its parity
/// picks. False when the one animation it would play is all zeros.
pub fn calc_pose(sk: &Skeleton, seq: usize, mut cycle: f32, poses: &[f32], bones: &BoneList, pose: &mut Pose) -> bool {
    let Some((g, s)) = sk.seq(seq) else { return false };
    let (i0, s0) = local_pose_parameter(sk, g, s, poses, 0);
    let (i1, s1) = local_pose_parameter(sk, g, s, poses, 1);
    let flags = seq_flags(s);
    if flags & STUDIO_REALTIME == 0 {
        if flags & STUDIO_CYCLEPOSE != 0 {
            let local = s.desc.i32(0xb4);
            let shared = if local == -1 { -1 } else { g.to_pose.get(local as usize).copied().unwrap_or(-1) };
            cycle = usize::try_from(shared).ok().and_then(|i| poses.get(i)).copied().unwrap_or(0.0);
        } else if cycle < 0.0 || 1.0 <= cycle {
            if flags & STUDIO_LOOPING == 0 {
                if cycle <= 0.0 {
                    cycle = 0.0;
                }
                if 1.0 <= cycle {
                    cycle = 1.0;
                }
            } else {
                cycle -= cycle.floor();
            }
        }
    }
    let at = |x: i32, y: i32, out: &mut Pose| calc_animation(sk, g, s, anim_desc(s, x, y), cycle, bones, out);
    let mut tmp = pose.clone();
    if s0 < BLEND_EPS {
        if s1 < BLEND_EPS {
            if all_zeros(s, i0, i1) {
                return false;
            }
            at(i0, i1, pose);
        } else if s1 > BLEND_ONE {
            at(i0, i1 + 1, pose);
        } else {
            at(i0, i1, pose);
            at(i0, i1 + 1, &mut tmp);
            blend_bones(pose, &tmp, s1, bones);
        }
        return true;
    }
    if s0 > BLEND_ONE {
        if s1 < BLEND_EPS {
            if all_zeros(s, i0 + 1, i1) {
                return false;
            }
            at(i0 + 1, i1, pose);
        } else if s1 > BLEND_ONE {
            at(i0 + 1, i1 + 1, pose);
        } else {
            at(i0 + 1, i1, pose);
            at(i0 + 1, i1 + 1, &mut tmp);
            blend_bones(pose, &tmp, s1, bones);
        }
        return true;
    }
    if s1 < BLEND_EPS {
        if all_zeros(s, i0 + 1, i1) {
            at(i0, i1, pose);
            scale_bones(s, pose, 1.0 - s0, bones);
        } else if all_zeros(s, i0, i1) {
            at(i0 + 1, i1, pose);
            scale_bones(s, pose, s0, bones);
        } else {
            at(i0, i1, pose);
            at(i0 + 1, i1, &mut tmp);
            blend_bones(pose, &tmp, s0, bones);
        }
        return true;
    }
    if s1 > BLEND_ONE {
        at(i0, i1 + 1, pose);
        at(i0 + 1, i1 + 1, &mut tmp);
        blend_bones(pose, &tmp, s0, bones);
        return true;
    }
    // Corners p, q and r with weights u, v and the rest.
    let (u, mut v, a, c, d, r9);
    if (i0 + i1) & 1 == 0 {
        if s0 > s1 {
            (u, v) = (1.0 - s0, s0 - s1);
        } else {
            (v, u) = (s1 - s0, s0);
        }
        (a, d) = ((s0 <= s1) as i32, (s0 > s1) as i32);
        (r9, c) = (a, d);
    } else if s1 + s0 > 1.0 {
        (v, u) = ((s0 - 1.0) + s1, 1.0 - s1);
        (a, c, r9, d) = (1, 1, 0, 0);
    } else {
        (v, u) = ((1.0 - s0) - s1, s1);
        (a, r9, c, d) = (0, 1, 0, 1);
    }
    if BLEND_DROP > v {
        v = 0.0;
    }
    let k = (1.0 - u) - v;
    at(i0 + a, i1 + r9, pose);
    if BLEND_EPS > v {
        at(i0 + d, i1 + c, &mut tmp);
        blend_bones(pose, &tmp, k / (u + k), bones);
    } else {
        at(i0 + c, i1 + a, &mut tmp);
        blend_bones(pose, &tmp, v / (u + v), bones);
        at(i0 + d, i1 + c, &mut tmp);
        blend_bones(pose, &tmp, k, bones);
    }
    true
}

/// BlendBones (0x1d3770, ConVar `BlendBonesMode` 0, the image's value): `p2` into `p1` by
/// `weight` for every bone of `bones`, rotations by the sign-aligned normalised blend,
/// positions and scales linearly.
fn blend_bones(p1: &mut Pose, p2: &Pose, weight: f32, bones: &BoneList) {
    let a = 1.0 - weight;
    let b = 1.0 - a;
    for &i in &bones.bones {
        let i = i as usize;
        p1.q[i] = blend(p1.q[i], a, p2.q[i], b, true);
        for k in 0..3 {
            p1.pos[i][k] = a * p1.pos[i][k] + weight * p2.pos[i][k];
            p1.scale[i][k] = a * p1.scale[i][k] + weight * p2.scale[i][k];
        }
    }
}

/// QuaternionNormalize (0x42e090): left alone at zero length.
fn quaternion_normalize(q: Quat) -> Quat {
    let l2 = ((q[0] * q[0] + q[1] * q[1]) + q[2] * q[2]) + q[3] * q[3];
    if l2 == 0.0 {
        return q;
    }
    let inv = 1.0 / l2.sqrt();
    q.map(|c| c * inv)
}

/// ScaleBones (0x1d41e0): `pose` (an animation of `seq`) taken `weight` of the way from rest,
/// for a blend whose other animation is all zeros.
fn scale_bones(seq: &Seq, pose: &mut Pose, weight: f32, bones: &BoneList) {
    let rest = 1.0 - weight;
    let k = 1.0 - rest;
    let scaled = seq_flags(seq) & STUDIO_ANIM_DATA != 0;
    for &i in &bones.bones {
        let i = i as usize;
        let q = pose.q[i];
        let w = q[3] * k;
        let w = if 0.0 <= q[3] { w + rest } else { w - rest };
        pose.q[i] = quaternion_normalize([k * q[0], k * q[1], k * q[2], w]);
        for c in 0..3 {
            pose.pos[i][c] *= weight;
            if scaled && pose.scale[i][c] != 1.0 {
                pose.scale[i][c] = crate::sim::math::pow(pose.scale[i][c], weight);
            }
        }
    }
}

/// SlerpBones (0x1d2dc0): blends `p2` (a pose of `seq`) into `p1` by `weight` times each bone's
/// weight in the sequence; a delta sequence adds instead.
pub fn slerp_bones(g: &Group, seq: &Seq, p1: &mut Pose, p2: &Pose, weight: f32, bones: &BoneList) {
    let d = &seq.desc;
    let flags = seq_flags(seq);
    let w = if weight <= 1.0 { weight } else { 1.0 };
    let wl = d.i32(0x9c) as usize;
    let bw = |b: u8| w * d.f32(wl + g.to_group[b as usize] as usize * 4);
    if flags & STUDIO_DELTA == 0 {
        for &b in &bones.bones {
            let s2 = bw(b);
            let s1 = 1.0 - s2;
            let i = b as usize;
            p1.q[i] = blend(p1.q[i], s1, p2.q[i], 1.0 - s1, false);
            for k in 0..3 {
                p1.pos[i][k] = s1 * p1.pos[i][k] + s2 * p2.pos[i][k];
                p1.scale[i][k] = s1 * p1.scale[i][k] + s2 * p2.scale[i][k];
            }
        }
        return;
    }
    for &b in &bones.bones {
        let s2 = bw(b);
        let i = b as usize;
        let q2 = p2.q[i];
        let mut len = (q2[0] * q2[0] + q2[1] * q2[1] + q2[2] * q2[2]).sqrt();
        if 1.0 <= len {
            len = 1.0;
        }
        let k = (len.asin() * s2).sin() / (len + f32::EPSILON);
        let (x, y, z) = (q2[0] * k, q2[1] * k, q2[2] * k);
        let mut qw = (1.0 - (x * x + y * y + z * z)).sqrt();
        if q2[3] < 0.0 {
            qw = -qw;
        }
        let qs = [x, y, z, qw];
        let q1 = p1.q[i];
        p1.q[i] = if flags & STUDIO_POST == 0 { quat_mult_aligned(qs, q1) } else { quat_mult_aligned(q1, qs) };
        for k in 0..3 {
            p1.pos[i][k] += s2 * p2.pos[i][k];
        }
        if flags & 0x20000 != 0 {
            for k in 0..3 {
                if p2.scale[i][k] != 1.0 {
                    p1.scale[i][k] *= p2.scale[i][k].powf(s2);
                }
            }
        }
    }
}

/// QuaternionMult with q aligned to p first, then normalised (inside 0x1d2dc0).
fn quat_mult_aligned(p: Quat, q: Quat) -> Quat {
    let dot = hsum([p[0] * q[0], p[1] * q[1], p[2] * q[2], p[3] * q[3]]);
    let mut a = [0.0; 4];
    for i in 0..4 {
        a[i] = f32::from_bits((dot[i].to_bits() & 0x8000_0000) ^ q[i].to_bits());
    }
    let r = [
        ((p[1] * a[2] + p[0] * a[3]) - p[2] * a[1]) + p[3] * a[0],
        (p[1] * a[3] - p[2] * a[0]) + p[3] * a[1] + p[0] * a[2],
        (p[2] * a[3] - p[0] * a[1]) + p[3] * a[2] + p[1] * a[0],
        p[3] * a[3] - (p[2] * a[2] + p[0] * a[0] + p[1] * a[1]),
    ];
    normalize(r, true)
}

/// AccumulatePose (0x1da7c0) without its cache and IK: `seq` at `cycle` blended into `pose`,
/// with its local auto-layers added to it first and its other auto-layers after, the entity's
/// pose parameters `poses` placing each on its blend grid.
pub fn accumulate_pose(sk: &Skeleton, pose: &mut Pose, seq: usize, cycle: f32, weight: f32, poses: &[f32], bones: &BoneList) -> bool {
    let w = weight.max(0.0).min(1.0);
    let Some((g, s)) = sk.seq(seq) else { return false };
    let list = seq_bone_list(bones, g, s);
    let mut p2 = pose.clone();
    let mut ok = calc_pose(sk, seq, cycle, poses, &list, &mut p2);
    if seq_flags(s) & STUDIO_LOCAL != 0 {
        ok |= add_layers(sk, &mut p2, s, cycle, 1.0, poses, bones, true);
    }
    if ok && 0.0 < w {
        slerp_bones(g, s, pose, &p2, w, &list);
    }
    ok | add_layers(sk, pose, s, cycle, w, poses, bones, false)
}

/// Sequence flag: it has local auto-layers.
const STUDIO_LOCAL: u32 = 0x200;
/// Auto-layer flags (mstudioautolayer_t +0xc).
const AL_SPLINE: u32 = 0x40;
const AL_XFADE: u32 = 0x80;
const AL_NOBLEND: u32 = 0x200;
const AL_LOCAL: u32 = 0x1000;
/// Played only when the bone setup asks for them (its `+0x620`); lurch's never does.
const AL_OPTIONAL: u32 = 0x2000;
const AL_POSE: u32 = 0x4000;
const AL_REALTIME: u32 = 0x8000;

/// AddSequenceLayers (0x1d9700) or, `local`, AddLocalLayers (0x1d9ad0): each auto-layer of
/// `s` of that kind accumulated into `pose`. Between its start and end a layer fades in to its
/// peak and out from its tail over `cycle` (eased with a spline, or as a cross-fade past the
/// tail), and plays the part of the cycle between them; a pose layer is weighted by its pose
/// parameter instead.
#[allow(clippy::too_many_arguments)] // the oracle's arguments
fn add_layers(sk: &Skeleton, pose: &mut Pose, s: &Seq, cycle: f32, weight: f32, poses: &[f32], bones: &BoneList, local: bool) -> bool {
    let d = &s.desc;
    let mut ok = false;
    for i in 0..d.i32(0x94).max(0) as usize {
        let o = d.i32(0x98) as usize + i * 0x20;
        let flags = d.u32(o + 0xc);
        if (flags & AL_LOCAL != 0) != local || flags & AL_OPTIONAL != 0 {
            continue;
        }
        let Some(layer) = sk.seq_by_guid(d.u64(o)) else { continue };
        let (start, peak, tail, end) = (d.f32(o + 0x10), d.f32(o + 0x14), d.f32(o + 0x18), d.f32(o + 0x1c));
        let (mut w, mut c) = (weight, cycle);
        if start != end {
            if cycle < start || end < cycle {
                continue;
            }
            let mut k = if peak <= cycle || start == peak {
                if tail < cycle && end != tail { (end - cycle) / (end - tail) } else { 1.0 }
            } else {
                (cycle - start) / (peak - start)
            };
            if flags & AL_SPLINE != 0 {
                k = (3.0 - (k + k)) * k * k;
            }
            if flags & AL_XFADE == 0 || cycle <= tail {
                if flags & AL_NOBLEND == 0 {
                    k *= weight;
                }
            } else {
                k = (k * weight) / ((1.0 - weight) + k * weight);
            }
            w = k;
            if flags & AL_REALTIME == 0 {
                c = (cycle - start) / (end - start);
            }
        }
        if flags & AL_POSE != 0 {
            let local_pose = d.i32(o + 8);
            let Some((g, _)) = sk.seq(layer) else { continue };
            let Some(p) = usize::try_from(local_pose).ok().and_then(|j| g.to_pose.get(j)).and_then(|&j| usize::try_from(j).ok()) else { continue };
            let pp = &sk.pose_params[p];
            let v = (pp.end - pp.start) * poses.get(p).copied().unwrap_or(0.0) + pp.start;
            let v = if v <= 0.0 { 0.0 } else { v };
            w *= if 1.0 <= v { 1.0 } else { v };
        }
        if w != 0.0 {
            ok |= accumulate_pose(sk, pose, layer, c, w, poses, bones);
        }
    }
    ok
}

/// A 3x4 matrix, rows of (x, y, z, translation).
pub type Mat = [[f32; 4]; 3];

pub const IDENTITY: Mat = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];

/// QuaternionMatrix with a position.
pub fn quat_matrix(q: Quat, p: V3) -> Mat {
    let [x, y, z, w] = q;
    [
        [1.0 - 2.0 * y * y - 2.0 * z * z, 2.0 * x * y - 2.0 * w * z, 2.0 * x * z + 2.0 * w * y, p[0]],
        [2.0 * x * y + 2.0 * w * z, 1.0 - 2.0 * x * x - 2.0 * z * z, 2.0 * y * z - 2.0 * w * x, p[1]],
        [2.0 * x * z - 2.0 * w * y, 2.0 * y * z + 2.0 * w * x, 1.0 - 2.0 * x * x - 2.0 * y * y, p[2]],
    ]
}

/// ConcatTransforms: a then b, as `a * b`.
pub fn concat(a: &Mat, b: &Mat) -> Mat {
    let mut out = [[0.0; 4]; 3];
    for r in 0..3 {
        for c in 0..4 {
            out[r][c] = a[r][0] * b[0][c] + a[r][1] * b[1][c] + a[r][2] * b[2][c] + if c == 3 { a[r][3] } else { 0.0 };
        }
    }
    out
}

/// Each bone's transform into the space `root` maps the model into.
pub fn build_transforms(sk: &Skeleton, pose: &Pose, root: &Mat) -> Vec<Mat> {
    let mut out: Vec<Mat> = Vec::with_capacity(sk.parents.len());
    for (b, &parent) in sk.parents.iter().enumerate() {
        let mut local = quat_matrix(pose.q[b], pose.pos[b]);
        for row in &mut local {
            for (v, k) in row.iter_mut().zip(pose.scale[b]) {
                *v *= k;
            }
        }
        let m = if parent < 0 { concat(root, &local) } else { concat(&out[parent as usize], &local) };
        out.push(m);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_floats_convert_exactly() {
        let cases = [(0x3c00, 1.0), (0xc000, -2.0), (0x0001, 2f32.powi(-24)), (0x7bff, 65504.0), (0x8000, -0.0)];
        for (h, want) in cases {
            assert_eq!(half(h).to_bits(), f32::to_bits(want), "{h:#06x}");
        }
        assert!(half(0x7c00).is_infinite() && half(0x7e00).is_nan());
    }

    #[test]
    fn angle_quaternion_follows_the_euler_formula() {
        for a in [[0.0, 0.0, 0.0], [0.3, -1.2, 2.9], [-3.1, 0.7, -0.4], [6.0, -6.0, 12.5]] {
            let h = a.map(|x: f32| x as f64 * 0.5);
            let (sr, cr, sp, cp, sy, cy) = (h[0].sin(), h[0].cos(), h[1].sin(), h[1].cos(), h[2].sin(), h[2].cos());
            let want = [sr * cp * cy - cr * sp * sy, cr * sp * cy + sr * cp * sy, cr * cp * sy - sr * sp * cy, cr * cp * cy + sr * sp * sy];
            let got = angle_quaternion(a);
            assert!((0..4).all(|i| (got[i] as f64 - want[i]).abs() < 1e-6), "{a:?}: {got:?} want {want:?}");
        }
    }

    #[test]
    fn clamp_cycle_wraps_loops_and_holds_the_rest() {
        assert_eq!(clamp_cycle(1.25, true), 0.25);
        assert_eq!(clamp_cycle(-0.25, true), 0.75);
        assert_eq!(clamp_cycle(1.5, false), f32::from_bits(0x3f7f_be77));
        assert_eq!(clamp_cycle(-0.1, false), 0.0);
        assert_eq!(clamp_cycle(0.5, false), 0.5);
    }
}
