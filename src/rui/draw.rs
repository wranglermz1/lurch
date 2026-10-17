//! An instance's draw jobs (its element's `+0x58` block) turned into quads, as the job walk
//! (`0x2fcff0`) and the job handlers at `0x1300bb0` do. Only what is ported draws; a job type
//! or feature not yet ported is listed in `Draws::unported`.

use super::transform::{maxps, recip, rsqrtps, Transform, TINY, V4};
use super::{Frame, Globals, Rui, Topology, Ui};
use crate::assets::rui::{Element, JOB_SIZES};

/// One quad in the instance's unit square and what the UI shader reads at its corners. The
/// corners are in the emitter's order (`0x2f6880`: `(x0, y0)`, `(x0, y1)`, `(x1, y1)`,
/// `(x1, y0)` of the clip rect through the job's transform, pairs swapped when the transform
/// mirrors). `uv` is the shader's `TEXCOORD1`: the image's frame coordinates in `xy`, the
/// job's crop coordinates in `zw`; `text` is `TEXCOORD2`; image, mask, style and flags are
/// `TEXCOORD3` (flags hold the draw mode from bit 12).
#[derive(Clone, Debug, PartialEq)]
pub struct Quad {
    pub corners: [[f32; 2]; 4],
    pub uv: [[f32; 4]; 4],
    pub text: [[f32; 2]; 4],
    /// The image's index in its atlas and the atlas, and the mask's (−1 for none). For text
    /// (mode 0): the two glyphs' places in the font atlas's glyph bounds, and the font atlas.
    pub image: i32,
    pub atlas: u8,
    pub mask: i32,
    pub mask_atlas: u8,
    /// Index into `Draws::styles`.
    pub style: u32,
    pub flags: u32,
}

/// One style as the shader's `UiShaderStyle_s`: `color[3]`, `colorxfrm[3]`, `tint`,
/// `blend`, `premul`, then the text or shape terms, `desaturate` at float 37.
pub type Style = [f32; 40];

#[derive(Default, Debug)]
pub struct Draws {
    pub quads: Vec<Quad>,
    pub styles: Vec<Style>,
    /// What was skipped for want of a port: job types, and features by name.
    pub unported: Vec<String>,
}

/// The luminance weights of the colour matrix (`0x13457f0`) and of the shader's desaturation.
pub const LUMINANCE: [f32; 3] = [0.2126, 0.7152, 0.0722];

/// The image index of the special source (`0x2f7c50`): the scene behind the HUD, blurred.
pub const SPECIAL_SOURCE: i32 = 0x2000;
/// The atlas a quad from the special source names (the engine's record at `0x270f58e0`); its
/// one image is the screen blur's texture.
pub const SPECIAL_ATLAS: u8 = u8::MAX;

/// What an image job draws from: the atlas, the image and their flags, the job's crop `cc`,
/// placement `p` and its span, the clip in the job's box, the rows taking the box to the
/// image's frame coordinates, and the image's nine-slice record.
struct Source<'a> {
    flags: u16,
    atlas: u8,
    index: usize,
    cc: [f32; 4],
    p: V4,
    span: V4,
    clip: V4,
    uv: (V4, V4),
    slice: Option<&'a [f32; 8]>,
}

/// The special source's map (`0x2f7c50`) from the job's box to the screen blur's texture
/// coordinates: a backbuffer `w` by `h` through the plane topology, which must lie flat in
/// the screen. None while the screen blur is off, in draw groups 1 to 6, or on any other
/// topology.
fn special_source(element: &Element, group: u8, frame: &Frame) -> Option<(V4, V4)> {
    let [w, h] = frame.screen_blur?;
    if (group as u16).wrapping_sub(1) <= 5 {
        return None;
    }
    let t = &frame.topology;
    if t.org[2] != 0.0 || t.right[1] != 0.0 || t.right[2] != 0.0 || t.down[0] != 0.0 || t.down[2] != 0.0 {
        return None;
    }
    let (rw, rh) = (1.0 / w as f32, 1.0 / h as f32);
    let r = [rw, rh, rw, rh];
    let s = frame.sizes[0];
    let k = [1.0 * s[0], 0.0 * s[1], 0.0 * s[2], 1.0 * s[3]];
    let axes = [t.right[0], t.down[1], t.right[0], t.down[1]];
    let inv = [element.inv_size[0], element.inv_size[1], element.inv_size[0], element.inv_size[1]];
    let basis = std::array::from_fn(|i| k[i] * (axes[i] * inv[i]) * r[i]);
    let org = [t.org[0], t.org[1], t.org[2], 0.0];
    Some((basis, std::array::from_fn(|i| org[i] * r[i])))
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    b.get(at..at + 2).map_or(0, |x| u16::from_le_bytes([x[0], x[1]]))
}

/// An affine map of the plane in the engine's lane layout: `out.x = b0 * x + b2 * y + o0`,
/// `out.y = b1 * x + b3 * y + o1`.
fn apply(b: V4, o: [f32; 2], p: [f32; 2]) -> [f32; 2] {
    [b[0] * p[0] + b[2] * p[1] + o[0], b[1] * p[0] + b[3] * p[1] + o[1]]
}

impl Ui {
    /// The quads of an instance updated this frame (`Ui::update` returned true), and its
    /// styles appended to the frame's (`0x2fc900`).
    pub fn draws(&self, rui: &Rui, frame: &Frame, out: &mut Draws) {
        let Some(element) = self.lib.elements.get(&rui.name) else { return };
        let base = out.styles.len() as u32;
        for i in 0..element.styles.len() / crate::assets::rui::STYLE_SIZE {
            match self.style(element, &rui.data, i) {
                Ok(s) => out.styles.push(s),
                Err(why) => {
                    out.unported.push(format!("{}: {why}", rui.name));
                    out.styles.push([0.0; 40]);
                }
            }
        }
        let mut at = 0;
        for _ in 0..element.job_count {
            let kind = u16_at(&element.jobs, at) as usize;
            let Some(&size) = JOB_SIZES.get(kind) else { break };
            let job = &element.jobs[at..(at + size).min(element.jobs.len())];
            match kind {
                0 => self.text_job(element, rui, frame, job, at, base, out),
                1 => self.image_job(element, rui, frame, job, base, out),
                2 => self.shape_job(element, rui, frame, job, base, out),
                _ => out.unported.push(format!("{}: job type {kind}", rui.name)),
            }
            at += size;
        }
    }

    /// The image job (`0x2f9570`) and the emitter's plain path (`0x2f6880`).
    fn image_job(&self, element: &Element, rui: &Rui, frame: &Frame, job: &[u8], style_base: u32, out: &mut Draws) {
        let data = &rui.data;
        let style = job[0x2c] as usize * crate::assets::rui::STYLE_SIZE;
        let Some(s) = element.styles.get(style..style + crate::assets::rui::STYLE_SIZE) else { return };
        let alpha = data.f32(u16_at(s, 8) as usize);
        if !(0.0 < alpha) {
            return;
        }
        let t: Transform = frame.transforms[(u16_at(job, 2) as usize).min(frame.transforms.len() - 1)];
        let b = t.basis;
        let det = b[3] * b[0] - b[1] * b[2];
        if det == 0.0 {
            return;
        }
        let image = data.i32(u16_at(job, 6) as usize);
        if image == -1 {
            return;
        }
        let inv = [b[3] / det, -b[1] / det, -b[2] / det, b[0] / det];
        let o = [t.origin[0], t.origin[2], t.origin[1], t.origin[3]];
        let n = [-inv[0] * o[0], -inv[1] * o[1], -inv[2] * o[2], -inv[3] * o[3]];
        let inv_origin = [n[2] + n[0], n[3] + n[1], n[0] + n[2], n[1] + n[3]];
        let src = if image == SPECIAL_SOURCE {
            let Some(uv) = special_source(element, rui.group, frame) else { return };
            Source { flags: u16_at(job, 0x2a) | 4, atlas: SPECIAL_ATLAS, index: 0, cc: [0.0, 0.0, 1.0, 1.0], p: [0.0; 4], span: [1.0; 4], clip: [0.0, 0.0, 1.0, 1.0], uv, slice: None }
        } else {
            let Some(entry) = self.images.get(image as usize) else { return };
            let flags = u16_at(job, 0x2a) | entry.flags as u16;
            let Some(atlas) = self.lib.atlases.get(entry.atlas as usize) else { return };
            let index = entry.index as usize;
            let (Some(frame_rec), Some(_)) = (atlas.frames.get(index), atlas.uv.get(index)) else { return };
            let f = |k: usize| data.f32(u16_at(job, k) as usize);
            let (c0, c1, c2, c3) = (f(0xa), f(0xc), f(0xe), f(0x10));
            let crop = [-c0, -c1, c2, c3];
            let p = [f(0x12), f(0x14), f(0x12), f(0x14)];
            let q = [f(0x16), f(0x18), f(0x16), f(0x18)];
            let lo: V4 = std::array::from_fn(|k| if p[k] < q[k] { p[k] } else { q[k] });
            let repeat = ((flags as i16 >> 4) & 3) as usize;
            let mask: [bool; 4] = REPEAT_MASKS[repeat];
            let span: V4 = std::array::from_fn(|k| q[k] - p[k]);
            let shifted: V4 = std::array::from_fn(|k| {
                let m = if mask[k] { lo[k] } else { 0.0 };
                if k < 2 { -m } else { m }
            });
            let div: V4 = std::array::from_fn(|k| if mask[k] { span[k].abs() } else { 1.0 });
            let a: V4 = std::array::from_fn(|k| (frame_rec[k] - shifted[k]) / div[k]);
            if a.iter().any(|&x| x < -f32::MAX) {
                return;
            }
            let m5: V4 = std::array::from_fn(|k| if crop[k] < a[k] { crop[k] } else { a[k] });
            let clip = [-m5[0], -m5[1], m5[2], m5[3]];
            if !(clip[0] < clip[2] && clip[1] < clip[3]) {
                return;
            }
            let f67 = [frame_rec[6], frame_rec[7], frame_rec[6], frame_rec[7]];
            let uv_basis: V4 = std::array::from_fn(|k| f67[k] * (inv[k] * span[k]));
            let uv_origin: V4 = std::array::from_fn(|k| f67[k] * (inv_origin[k] * span[k] + p[k]) + if k < 2 { frame_rec[4 + k] } else { 0.0 });
            Source { flags, atlas: entry.atlas, index, cc: [c0, c1, c2, c3], p, span, clip, uv: (uv_basis, uv_origin), slice: atlas.slices.get(index) }
        };
        let Source { mut flags, atlas, index, cc: [c0, c1, c2, c3], p, span, clip, uv: (uv_basis, uv_origin), slice } = src;
        let mask_image = data.i32(u16_at(job, 8) as usize);
        let (mask, (crop_basis, crop_origin)) = if mask_image == SPECIAL_SOURCE {
            let Some(map) = special_source(element, rui.group, frame) else { return };
            flags |= 8;
            (Some((SPECIAL_ATLAS, 0)), map)
        } else if mask_image != -1 {
            let Some(m) = self.images.get(mask_image as usize) else { return };
            flags |= (m.flags as u16) << 2;
            let Some(frame) = self.lib.atlases.get(m.atlas as usize).and_then(|a| a.frames.get(m.index as usize)) else { return };
            let screen = data.bytes().get(u16_at(job, 0x28) as usize).is_some_and(|&b| b != 0);
            let Some(map) = mask_map(job, data, frame, screen, inv, inv_origin, span, [p[0], p[1], p[0], p[1]]) else { return };
            (Some((m.atlas, m.index as i32)), map)
        } else {
            let cc = [c0, c1, c0, c1];
            let d = maxps(TINY, [c2 - c0, c3 - c1, c2 - c0, c3 - c1]);
            let r = recip(d);
            (None, (std::array::from_fn(|k| inv[k] * r[k]), std::array::from_fn(|k| (inv_origin[k] - cc[k]) * r[k])))
        };
        let rows = [
            [uv_basis[0], uv_basis[1], crop_basis[0], crop_basis[1]],
            [uv_basis[2], uv_basis[3], crop_basis[2], crop_basis[3]],
            [uv_origin[0], uv_origin[1], crop_origin[0], crop_origin[1]],
        ];
        let proto = Quad {
            corners: [[0.0; 2]; 4],
            uv: [[0.0; 4]; 4],
            text: [[0.0; 2]; 4],
            image: index as i32,
            atlas,
            mask: mask.map_or(-1, |m| m.1),
            mask_atlas: mask.map_or(0, |m| m.0),
            style: style_base + job[0x2c] as u32,
            flags: flags as u32,
        };
        let edges = ((!(flags as i16 as i32)) >> 8) & 0xf;
        match slice {
            Some(slice) => {
                for s in nine_slice(slice, rui.size, &t, span, p, clip, [-c0, -c1, c2, c3], rows) {
                    emit(frame, element.size, &t, s.xs, s.ys, edges & s.edges, s.rows, &proto, out);
                }
            }
            None => emit(frame, element.size, &t, [clip[0], clip[0], clip[2], clip[2]], [clip[1], clip[3], clip[3], clip[1]], edges, rows, &proto, out),
        }
    }
}

impl Ui {
    /// The shape job (type 2, `0x2f9d50`): one quad over the job's box drawing its image, its
    /// crop lanes carrying the box as `[-1, 1]` for the shader's procedural shape (a ring or an
    /// arc from the style's terms). The image's crop is widened by the style's `+0x38` term,
    /// across in proportion to the box's height over its width. No side is anti-aliased.
    fn shape_job(&self, element: &Element, rui: &Rui, frame: &Frame, job: &[u8], style_base: u32, out: &mut Draws) {
        let data = &rui.data;
        let style = job[0x1a] as usize * crate::assets::rui::STYLE_SIZE;
        let Some(s) = element.styles.get(style..style + crate::assets::rui::STYLE_SIZE) else { return };
        if !(0.0 < data.f32(u16_at(s, 8) as usize)) {
            return;
        }
        let ti = (u16_at(job, 2) as usize).min(frame.transforms.len() - 1);
        let t = frame.transforms[ti];
        let b = t.basis;
        let det = b[3] * b[0] - b[1] * b[2];
        if det == 0.0 {
            return;
        }
        let inv = [b[3] / det, -b[1] / det, -b[2] / det, b[0] / det];
        let o = t.origin;
        let inv_origin = [-inv[2] * o[1] + -inv[0] * o[0], -inv[3] * o[3] + -inv[1] * o[2]];
        let image = data.i32(u16_at(job, 6) as usize);
        if image == -1 {
            return;
        }
        if image == SPECIAL_SOURCE {
            out.unported.push(format!("{}: shape job from a special source", rui.name));
            return;
        }
        let Some(entry) = self.images.get(image as usize) else { return };
        let flags = u16_at(job, 0x18) | entry.flags as u16;
        let size = frame.sizes[ti];
        let (w, h) = (size[0], size[2]);
        if !(0.0 < if h <= w { h } else { w }) {
            return;
        }
        let Some(frame_rec) = self.lib.atlases.get(entry.atlas as usize).and_then(|a| a.frames.get(entry.index as usize)) else { return };
        let f = |k: usize| data.f32(u16_at(job, k) as usize);
        let k38 = data.f32(u16_at(s, 0x38) as usize);
        let (p, q) = ([f(0x10), f(0x12), f(0x10), f(0x12)], [f(0x14), f(0x16), f(0x14), f(0x16)]);
        let lo: V4 = std::array::from_fn(|i| if p[i] < q[i] { p[i] } else { q[i] });
        let mask = REPEAT_MASKS[((flags as i16 >> 4) & 3) as usize];
        let span = maxps(std::array::from_fn(|i| q[i] - p[i]), TINY);
        let wide = (h * k38 * (q[0] - p[0])) / w;
        let tall = (q[1] - p[1]) * k38;
        let extra = [wide, tall, wide, tall];
        let a: V4 = std::array::from_fn(|i| {
            let m = if mask[i] { lo[i] } else { 0.0 };
            let shifted = if i < 2 { -m } else { m };
            let div = if mask[i] { span[i].abs() } else { 1.0 };
            ((frame_rec[i] - shifted) + extra[i]) / div
        });
        if a.iter().any(|&x| x < -f32::MAX) {
            return;
        }
        let crop = [-f(8), -f(0xa), f(0xc), f(0xe)];
        let m: V4 = std::array::from_fn(|i| if crop[i] < a[i] { crop[i] } else { a[i] });
        let clip = [-m[0], -m[1], m[2], m[3]];
        if !(clip[0] < clip[2] && clip[1] < clip[3]) {
            return;
        }
        let f67 = [frame_rec[6], frame_rec[7], frame_rec[6], frame_rec[7]];
        let uv: V4 = std::array::from_fn(|i| inv[i] * span[i] * f67[i]);
        let uv_origin = [(inv_origin[0] * span[0] + p[0]) * f67[0] + frame_rec[4], (inv_origin[1] * span[1] + p[1]) * f67[1] + frame_rec[5]];
        let rows = [
            [uv[0], uv[1], inv[0] * 2.0, inv[1] * 2.0],
            [uv[2], uv[3], inv[2] * 2.0, inv[3] * 2.0],
            [uv_origin[0], uv_origin[1], inv_origin[0] * 2.0 - 1.0, inv_origin[1] * 2.0 - 1.0],
        ];
        let proto = Quad {
            corners: [[0.0; 2]; 4],
            uv: [[0.0; 4]; 4],
            text: [[0.0; 2]; 4],
            image: entry.index as i32,
            atlas: entry.atlas,
            mask: -1,
            mask_atlas: 0,
            style: style_base + job[0x1a] as u32,
            flags: flags as u32,
        };
        emit(frame, element.size, &t, [clip[0], clip[0], clip[2], clip[2]], [clip[1], clip[3], clip[3], clip[1]], 0, rows, &proto, out);
    }
}

/// One quad of the emitter (`0x2f6880`): corners `xs`, `ys` in the job's box through its
/// transform, the anti-aliased sides among `edges` pushed out, mirrored when the transform
/// mirrors, and at each corner the template `rows` (per `x`, per `y`, constant) give the
/// shader's frame and crop coordinates.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit(frame: &Frame, size: [f32; 2], t: &Transform, xs: V4, ys: V4, edges: i32, rows: [V4; 3], proto: &Quad, out: &mut Draws) {
    let b = t.basis;
    let det = b[3] * b[0] - b[1] * b[2];
    let x = |u: f32, v: f32| apply(b, [t.origin[0], t.origin[1]], [u, v]);
    let k: [[f32; 2]; 4] = std::array::from_fn(|i| x(xs[i], ys[i]));
    let mut c8 = [k[0][0], k[1][0], k[2][0], k[3][0], k[0][1], k[1][1], k[2][1], k[3][1]];
    if edges != 0 && !pad_edges(&frame.globals, &frame.topology, size, b, PAD_SIGNS[edges as usize], &mut c8) {
        return;
    }
    let k = |i: usize| [c8[i], c8[4 + i]];
    let corners = if det < 0.0 { [k(1), k(0), k(3), k(2)] } else { [k(0), k(1), k(2), k(3)] };
    let uv = corners.map(|c| std::array::from_fn(|l| rows[0][l] * c[0] + rows[1][l] * c[1] + rows[2][l]));
    out.quads.push(Quad { corners, uv, ..proto.clone() });
}

/// A part of a nine-slice image: its corners in the job's box, the sides that keep their
/// anti-aliasing, and its template.
pub(super) struct Slice {
    pub xs: V4,
    pub ys: V4,
    pub edges: i32,
    pub rows: [V4; 3],
}

/// The emitter's nine-slice path (`0x2f6a08` on). The image's slice record holds its borders
/// as fractions of its frame (left, top, right, bottom), then a size and a least size in
/// pixels. The borders keep their size on screen: `K`, the image's span in pixels (at least the
/// least size), sets where they end inside the job's box, and each of the nine parts maps the
/// frame through the template scaled one of three ways per axis, the near border's, the
/// middle's (stretched) or the far border's. A part the job's crop cuts away entirely is not
/// drawn; each part keeps only its outer sides' anti-aliasing.
#[allow(clippy::too_many_arguments)]
pub(super) fn nine_slice(slice: &[f32; 8], size: [f32; 2], t: &Transform, span: V4, p: V4, clip: V4, crop: V4, rows: [V4; 3]) -> Vec<Slice> {
    let r = recip(span);
    let lt = [slice[0], slice[1], 0.0, 0.0];
    let signed = [slice[0], slice[1], -slice[2], -slice[3]];
    let sides = [slice[2] + slice[0], slice[3] + slice[1], slice[2] - slice[2], slice[3] - slice[3]];
    let wh = [size[0], size[0], size[1], size[1]];
    let bs = [t.basis[0], t.basis[2], t.basis[1], t.basis[3]];
    let sq: V4 = std::array::from_fn(|i| {
        let a = (wh[i] * bs[i]) * r[i];
        a * a
    });
    let s45 = [slice[4], slice[5], slice[4], slice[5]];
    let s67 = [slice[6], slice[7], slice[6], slice[7]];
    let k = maxps(std::array::from_fn(|i| (sq[(i + 2) % 4] + sq[i]).sqrt() * s45[i]), s67);
    let m = recip(std::array::from_fn(|i| k[i] - sides[i]));
    let kk = [k[0], k[1], 1.0, 1.0];
    let rest: V4 = std::array::from_fn(|i| 1.0 - sides[i]);
    let a: V4 = std::array::from_fn(|i| if i < 2 { (rest[i] * k[i]) * m[i] } else { 1.0 });
    let scaled = |v: V4, s: V4| -> V4 { std::array::from_fn(|i| v[i] * s[i]) };
    let near = [scaled(rows[0], kk), scaled(rows[1], kk), scaled(rows[2], kk)];
    let far_origin: V4 = std::array::from_fn(|i| (1.0 - kk[i]) + near[2][i]);
    let mid_origin: V4 = std::array::from_fn(|i| (lt[i] - (lt[i] * rest[i]) * m[i]) + rows[2][i] * a[i]);
    let mid = [scaled(rows[0], a), scaled(rows[1], a), mid_origin];
    let far = [near[0], near[1], far_origin];
    let inv_k = [m[2], m[3], m[2], m[3]];
    let edge = [0.0, 0.0, 1.0, 1.0];
    let bnd: V4 = std::array::from_fn(|i| ((inv_k[i] * signed[i] + edge[i]) - p[i]) * r[i]);
    let x = [clip[0], bnd[0], clip[2], bnd[2]];
    let y = [clip[1], bnd[1], clip[3], bnd[3]];
    let bits = |v: [bool; 4]| v.iter().enumerate().fold(0, |m, (i, &b)| m | (b as i32) << i);
    let m1 = bits([crop[0] <= -bnd[0], crop[1] <= -bnd[1], crop[2] <= bnd[2], crop[3] <= bnd[3]]);
    let m2 = bits([crop[0] <= -bnd[2], crop[1] <= -bnd[3], crop[2] <= bnd[0], crop[3] <= bnd[1]]);
    let cols = [([x[0], x[0], x[1], x[1]], m1 & 1, 1), ([x[1], x[1], x[3], x[3]], m2 & 5, 0), ([x[3], x[3], x[2], x[2]], m1 & 4, 2)];
    let lines = [([y[0], y[1], y[1], y[0]], m1 & 2, 4), ([y[1], y[3], y[3], y[1]], m2 & 0xa, 0), ([y[3], y[2], y[2], y[3]], m1 & 8, 8)];
    let sets = [&near, &mid, &far];
    let mut out = Vec::new();
    for (row, (ys, row_cut, row_edge)) in lines.into_iter().enumerate() {
        for (col, (xs, col_cut, col_edge)) in cols.into_iter().enumerate() {
            if col_cut | row_cut != 0 {
                continue;
            }
            let (u, v) = (sets[col], sets[row]);
            // Which set the crop lanes come from, as the engine assembles each part; the sets
            // agree there but for the sign of a zero.
            let (cr, co) = match (col, row) {
                (1, 1) => (&mid, &mid),
                (2, 1) | (1, 2) | (2, 2) => (&near, &far),
                _ => (&near, &near),
            };
            let pick = |set: usize| -> V4 {
                let c = if set < 2 { cr } else { co };
                [u[set][0], v[set][1], c[set][2], c[set][3]]
            };
            out.push(Slice { xs, ys, edges: col_edge | row_edge, rows: [pick(0), pick(1), pick(2)] });
        }
    }
    out
}

/// Where the mask image's texels fall over the quad (the image job's mask path, `0x2f98f6`):
/// the job's box (or with `screen`, the element's unit square) turned by the job's angle
/// `+0x1e` (in turns), scaled by `+0x24`, `+0x26`, offset by `+0x1a`, `+0x1c` within the box
/// and by `+0x20`, `+0x22` after, then into the mask's frame. Returns the basis and origin the
/// emitter takes for the shader's `zw` coordinates; None when a `screen` mask's map cannot be
/// inverted (the engine then reads stale values).
#[allow(clippy::too_many_arguments)]
fn mask_map(job: &[u8], data: &super::Data, frame: &[f32; 8], screen: bool, inv: V4, inv_origin: V4, span: V4, p: V4) -> Option<(V4, V4)> {
    let f = |k: usize| data.f32(u16_at(job, k) as usize);
    let (inv, span, p, inv_origin) = if screen { ([1.0, 0.0, 0.0, 1.0], [1.0; 4], [0.0; 4], [0.0; 4]) } else { (inv, span, p, inv_origin) };
    let a: V4 = std::array::from_fn(|k| inv_origin[k] * span[k] + p[k]);
    let b: V4 = std::array::from_fn(|k| inv[k] * span[k]);
    let angle = f(0x1e);
    let r = sin_quarter_turns([(angle + 0.25) * 4.0, (angle + 0.0) * 4.0, (-angle + 0.0) * 4.0, (angle + 0.25) * 4.0]);
    let off = [f(0x1a), f(0x1c), f(0x1a), f(0x1c)];
    let c: V4 = std::array::from_fn(|k| off[k] * span[k] + p[k]);
    let scale = [f(0x24), f(0x26), f(0x24), f(0x26)];
    let d: V4 = std::array::from_fn(|k| a[k] - c[k]);
    let rs: V4 = std::array::from_fn(|k| r[k] * scale[k]);
    let m = [b[0] * rs[0] + b[1] * rs[2], b[0] * rs[1] + b[1] * rs[3], b[3] * rs[2] + b[2] * rs[0], b[3] * rs[3] + b[2] * rs[1]];
    let e = [d[0] * rs[0], d[2] * rs[1], d[1] * rs[2], d[3] * rs[3]];
    let after = [f(0x20), f(0x22), 0.0, 0.0];
    let n = [(after[0] + c[0]) + (e[2] + e[0]), (after[1] + c[1]) + (e[3] + e[1]), (after[2] + c[2]) + (e[0] + e[2]), (after[3] + c[3]) + (e[1] + e[3])];
    let (basis, origin) = if screen {
        let det = m[3] * m[0] - m[1] * m[2];
        if det == 0.0 {
            return None;
        }
        let i = [m[3] / det, -m[1] / det, -m[2] / det, m[0] / det];
        let t = [-i[0] * n[0], -i[1] * n[2], -i[2] * n[1], -i[3] * n[3]];
        (i, [t[2] + t[0], t[3] + t[1], t[0] + t[2], t[1] + t[3]])
    } else {
        (m, n)
    };
    let f67 = [frame[6], frame[7], frame[6], frame[7]];
    let f45 = [frame[4], frame[5], 0.0, 0.0];
    Some((std::array::from_fn(|k| basis[k] * f67[k]), std::array::from_fn(|k| origin[k] * f67[k] + f45[k])))
}

/// The engine's sine of angles in quarter turns, four at once (`0x2f9a88` on): rounded to the
/// nearest quarter, the rest by one polynomial for the sine and one for the cosine, the
/// quadrant picking which and its sign.
pub fn sin_quarter_turns(t: V4) -> V4 {
    std::array::from_fn(|k| {
        let n = t[k].round_ties_even() as i32;
        let f = t[k] - n as f32;
        let f2 = f * f;
        let cos = 1.0 - (f2 - (((0.000_915_952_26 * f2 + -0.020_863_468) * f2 + 0.253_669_44) * f2 + -0.233_700_54) * f2);
        let sin = (((-0.004_600_245 * f2 + 0.079_678_98) * f2 + -0.645_963_25) * f2 + 0.570_796_3) * f + f;
        let v = if n & 1 == 0 { sin } else { cos };
        f32::from_bits(v.to_bits() ^ (((n & 2) as u32) << 30))
    })
}

/// `0x270f5930`: by the job's repeat bits (flags `>> 4 & 3`: repeat in x, in y), the lanes
/// the image's own extent clips.
const REPEAT_MASKS: [[bool; 4]; 4] = [[true; 4], [false, true, false, true], [true, false, true, false], [false; 4]];

/// ConVar `rui_padDist` (image value): how far, in pixels, an anti-aliased edge is pushed out
/// (twice this, `0x301320`), so its soft fall-off has room.
pub const RUI_PAD_DIST: f32 = 0.7;

/// `0x1300c00`: by the edges whose anti-aliasing is on (flags `!x >> 8 & 0xf`: left, right,
/// top, bottom), which way each of the quad's sides moves.
const PAD_SIGNS: [[f32; 4]; 16] = {
    let mut t = [[0.0; 4]; 16];
    let mut i = 0;
    while i < 16 {
        t[i] = [if i & 1 != 0 { -1.0 } else { 0.0 }, if i & 2 != 0 { 1.0 } else { 0.0 }, if i & 4 != 0 { -1.0 } else { 0.0 }, if i & 8 != 0 { 1.0 } else { 0.0 }];
        i += 1;
    }
    t
};

/// Pushes a quad's anti-aliased sides out by `2 × rui_padDist` screen pixels through the
/// topology and the view (`0x301320`, the plane case), each corner along its two sides'
/// unit directions (`0x2f6950` builds them from the transform at the element's size) with the
/// corner's diagonal lengthened by √2 − 1. Corners `x0..x3, y0..y3`. False when more than one
/// corner would fold over, and the quad is not drawn.
pub(super) fn pad_edges(g: &Globals, topo: &Topology, size: [f32; 2], basis: V4, s: [f32; 4], c: &mut [f32; 8]) -> bool {
    let sq = {
        let v = [size[0] * basis[0], size[0] * basis[1], size[1] * basis[2], size[1] * basis[3]];
        let p = [v[0] * v[0], v[1] * v[1], v[2] * v[2], v[3] * v[3]];
        maxps(TINY, [p[1] + p[0], p[0] + p[1], p[3] + p[2], p[2] + p[3]])
    };
    let r = rsqrtps(sq);
    let n: V4 = std::array::from_fn(|i| (1.5 - sq[i] * 0.5 * r[i] * r[i]) * r[i] * basis[i]);
    const BEVEL: f32 = 0.414_213_57;
    let (o, rv, dv) = (topo.org, topo.right, topo.down);
    let (xs, ys) = ([c[0], c[1], c[2], c[3]], [c[4], c[5], c[6], c[7]]);
    let wx: V4 = std::array::from_fn(|i| rv[0] * xs[i] + o[0] + dv[0] * ys[i]);
    let wy: V4 = std::array::from_fn(|i| rv[1] * xs[i] + o[1] + dv[1] * ys[i]);
    let wz: V4 = std::array::from_fn(|i| rv[2] * xs[i] + o[2] + dv[2] * ys[i]);
    let dx = [n[0] * s[0] + n[2] * s[2], n[0] * s[0] + n[2] * s[3], n[0] * s[1] + n[2] * s[3], n[0] * s[1] + n[2] * s[2]];
    let dy = [n[1] * s[0] + n[3] * s[2], n[1] * s[0] + n[3] * s[3], n[1] * s[1] + n[3] * s[3], n[1] * s[1] + n[3] * s[2]];
    let pair = [s[2] * s[0], s[3] * s[0], s[3] * s[1], s[2] * s[1]];
    let pad = RUI_PAD_DIST + RUI_PAD_DIST;
    let m = |at: usize| g.f32(at);
    let (vw, vh) = (m(0x48), m(0x4c));
    let mut d = [0.0f32; 4];
    for i in 0..4 {
        let ex = rv[0] * dx[i] + dv[0] * dy[i];
        let ey = rv[1] * dx[i] + dv[1] * dy[i];
        let ez = rv[2] * dx[i] + dv[2] * dy[i];
        let wc = wx[i] * m(0x80) + wy[i] * m(0x84) + wz[i] * m(0x88) + m(0x8c);
        let wd = ey * m(0x84) + ex * m(0x80) + ez * m(0x88);
        let ax = ((ey * m(0x54) + ex * m(0x50) + ez * m(0x58)) * wc - (wx[i] * m(0x50) + wy[i] * m(0x54) + wz[i] * m(0x58) + m(0x5c)) * wd) * vw;
        let ay = ((ey * m(0x64) + ex * m(0x60) + ez * m(0x68)) * wc - (wx[i] * m(0x60) + wy[i] * m(0x64) + wz[i] * m(0x68) + m(0x6c)) * wd) * vh;
        let k = (pair[i].abs() * BEVEL * pad * pad + pad * pad) * wc * wc;
        let q = (ay * ay + ax * ax) - k * wd * wd;
        let rq = recip(maxps(TINY, [q; 4]))[0];
        let t = rq * k * wc;
        let u = t * wd;
        if 0.0 < q {
            d[i] = (u * u + t * wc).sqrt() + u;
        }
    }
    let ny: V4 = std::array::from_fn(|i| d[i] * dy[i] + ys[i]);
    let nx: V4 = std::array::from_fn(|i| d[i] * dx[i] + xs[i]);
    let det = n[3] * n[0] - n[1] * n[2];
    let fold: [bool; 4] = std::array::from_fn(|i| {
        let (j, k) = ((i + 1) % 4, (i + 3) % 4);
        det * ((ny[i] - ny[k]) * (nx[j] - nx[i]) - (nx[i] - nx[k]) * (ny[j] - ny[i])) <= 0.0
    });
    if fold.iter().filter(|&&f| f).count() > 1 {
        return false;
    }
    for i in 0..4 {
        let j = if fold[i] { (i + 1) % 4 } else { i };
        c[i] = nx[j];
        c[4 + i] = ny[j];
    }
    true
}

impl Ui {
/// Style `index` of an element as the shader's style buffer holds it (`0x2fc900`), for an
/// instance not nested in another.
pub fn style(&self, element: &Element, data: &super::Data, index: usize) -> Result<Style, String> {
    let r = &element.styles[index * crate::assets::rui::STYLE_SIZE..(index + 1) * crate::assets::rui::STYLE_SIZE];
    let f = |at: usize| data.f32(u16_at(r, at) as usize);
    let mut o = [0.0f32; 40];
    for (k, c) in o[..12].iter_mut().enumerate() {
        *c = f(2 + 2 * k);
    }
    let (hue, bright, sat) = (f(0x26), f(0x2a), f(0x28));
    if hue != 0.0 || hue.is_nan() {
        return Err("style with a hue rotation (0x2f3d60)".into());
    }
    let low = if bright < 0.0 { bright } else { 0.0 };
    let k = [low + 1.0, low + 1.0, low + 1.0, 0.0 + 1.0];
    let lum = [LUMINANCE[0], LUMINANCE[1], LUMINANCE[2], 0.0];
    let add = if bright > 0.0 { bright } else { 0.0 };
    let l: V4 = std::array::from_fn(|i| (lum[i] - sat * lum[i]) * k[i] + if i == 3 { add } else { 0.0 });
    let diag = sat * k[0];
    for row in 0..3 {
        for col in 0..4 {
            o[12 + 4 * row + col] = if col == row { diag + l[col] } else { 0.0 + l[col] };
        }
    }
    for k in 0..4 {
        o[24 + k] = f(0x1a + 2 * k);
    }
    o[28] = f(0x22);
    o[29] = f(0x24);
    match u16_at(r, 0) {
        0 => {
            let Some((atlas, face)) = self.font(data.u16(u16_at(r, 0x2c) as usize)) else { return Err("text style with an empty font slot".into()) };
            let tiny = |x: f32| if f32::MIN_POSITIVE > x { f32::MIN_POSITIVE } else { x };
            let k = atlas.width as f32 * face.metrics[0] / f(0x36);
            o[30] = k * f(0x30) * atlas.inv_size[0];
            o[31] = k * f(0x32) * atlas.inv_size[1];
            o[32] = f(0x2e);
            o[33] = 1.0 / tiny(k * f(0x34));
            o[34] = k * f(0x3a);
            o[35] = k * f(0x3c);
            o[36] = 1.0 / tiny(f(0x3e) * k);
        }
        t if (1u32 << (t & 31)) & 0x3a == 0 => {
            o[30] = f(0x2c);
            o[31] = f(0x2e) * std::f32::consts::TAU;
            o[32] = f(0x30) * std::f32::consts::TAU;
            o[33] = f(0x32);
            o[34] = f(0x34);
            o[35] = f(0x36);
            let w = f(0x38);
            o[36] = 1.0 / if f32::MIN_POSITIVE > w { f32::MIN_POSITIVE } else { w };
        }
        _ => {}
    }
    Ok(o)
}
}
