//! Text: a text job's string laid out in its styles' fonts (`0x2f55e0`, the callback at
//! `+0x20`), which leaves the job's line breaks, inline images and fit in the frame for the
//! text job (`0x2f7db0`) to draw. Lengths are in the units of the styles' sizes.
//!
//! A string is UTF-8 with escapes: `` `0 `` to `` `3 `` switch to the job's style of that
//! number, `%%` is a `%`, and `%...%` substitutes (`0x2f4c70`): `%$name%` an image (code point
//! U+F0000 plus its index), `%<%` and `%>%` the start and end of a span an image stretches over
//! (U+F2000, U+F2001), `%@name%` an element embedded from the registry (U+F2002 plus its
//! index), and anything else a command's key from the token table (`super::tokens`).
//!
//! Not modelled: the parent's colour matrix in an embedded element's styles.

use std::collections::HashMap;

use super::draw::{emit, nine_slice, pad_edges, Draws, Quad};
use super::transform::{recip, Transform, V4};
use super::{Frame, Rui, Ui};
use crate::assets::rui::{Element, Face, FontAtlas, Glyph, STYLE_SIZE};

pub const MAX_LINES: usize = 64;
pub const MAX_IMAGES: usize = 64;
pub const MAX_NESTED: usize = 8;
/// The substitutions a string can be inside at once (the layout's stack of 8 string positions).
const MAX_DEPTH: usize = 8;

/// A break between lines (`+0x35a4`, `0xc` bytes): the characters up to it, the width of the
/// line it ends, and, on a job's last break, the width of the line after it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Line {
    pub end: u32,
    pub width: f32,
    pub rest: f32,
}

/// An image in a string (`+0x38ac`, `0x14` bytes): its image table index, the style it was in,
/// and its box from the job's origin, `y` down from the first line's baseline.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Inline {
    pub image: u16,
    pub style: u16,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// An element embedded in a string (`+0x3db0`, `0x1c` bytes): the style it was in, its index
/// in the registry, the pen's `x` and the baseline's `y` where it starts, its box, and the
/// element's own size over its registered size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Nested {
    pub style: u16,
    pub index: u8,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub scale: [f32; 2],
}

/// What the layout leaves for its job (`+0x20 + offset / 16 * 0xc`): the scale that fits its
/// widest line in its box, where its lines and images are in the frame's arrays (first, count,
/// in bytes), the style that made a line tallest, and its nested elements.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Laid {
    pub scale: f32,
    pub first_line: u8,
    pub lines: u8,
    pub first_image: u8,
    pub images: u8,
    pub tallest: u8,
    pub first_nested: u8,
    pub nested: u8,
}

/// The draw scratch's text arrays. `Rui_Draw` zeroes the counts each frame (`0x2fc6a3`); the
/// jobs' records stay until a layout writes them again.
pub struct Scratch {
    pub lines: [Line; MAX_LINES],
    pub line_count: u32,
    pub images: [Inline; MAX_IMAGES],
    pub image_count: u32,
    pub nested: [Nested; MAX_NESTED],
    pub nested_count: u32,
    /// By job offset / 16.
    pub jobs: HashMap<u32, Laid>,
    #[cfg(test)]
    size: V4,
}

impl Default for Scratch {
    fn default() -> Scratch {
        Scratch {
            lines: [Line::default(); MAX_LINES],
            line_count: 0,
            images: [Inline::default(); MAX_IMAGES],
            image_count: 0,
            nested: [Nested::default(); MAX_NESTED],
            nested_count: 0,
            jobs: HashMap::new(),
            #[cfg(test)]
            size: [0.0; 4],
        }
    }
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    b.get(at..at + 2).map_or(0, |x| u16::from_le_bytes([x[0], x[1]]))
}

/// MAXSS and MINSS: the second operand wins a tie or a NaN.
fn maxss(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

fn minss(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}

/// One code point of UTF-8 at `p` and the bytes it took, as the layout decodes: a malformed
/// sequence, a surrogate or a code point past U+10FFFF reads as 0 and takes nothing.
///
/// # Safety
/// `p` points into a NUL-terminated string.
unsafe fn decode(p: *const u8) -> (u32, usize) {
    // SAFETY: each byte is read only after the one before it is not NUL.
    let at = |k: usize| unsafe { *p.add(k) } as u32;
    let b = at(0);
    if b < 0x80 {
        return (b, 1);
    }
    let b1 = at(1);
    if b1 & 0xc0 != 0x80 {
        return (0, 0);
    }
    if b < 0xe0 {
        return if b > 0xc1 { ((b & 0x3f) << 6 | b1 & 0x3f, 2) } else { (0, 0) };
    }
    let b2 = at(2);
    if b2 & 0xc0 != 0x80 {
        return (0, 0);
    }
    let c = ((b & 0xf) << 6 | b1 & 0x3f) << 6 | b2 & 0x3f;
    if b < 0xf0 {
        return if c.wrapping_sub(0xd800) < 0x800 { (0, 0) } else { (c, 3) };
    }
    let b3 = at(3);
    if b3 & 0xc0 != 0x80 {
        return (0, 0);
    }
    let c = b3 & 0x3f | c << 6;
    if c < 0x11_0000 { (c, 4) } else { (0, 0) }
}

/// A private-use code point as the substitution writes it: four bytes of UTF-8 and a NUL.
fn encode(c: u32, buf: &mut [u8; 8]) {
    *buf = [(c >> 18) as u8 | 0xf0, (c >> 12) as u8 & 0x3f | 0x80, (c >> 6) as u8 & 0x3f | 0x80, c as u8 & 0x3f | 0x80, 0, 0, 0, 0];
}

/// The string `%...%` at `*p` (past its first `%`) stands for (`0x2f4c70`), with `*p` moved
/// past its closing `%`; None ends the layout.
///
/// # Safety
/// `*p` points into a NUL-terminated string.
unsafe fn substitute(ui: &Ui, p: &mut *const u8, buf: &mut [u8; 8]) -> Option<*const u8> {
    // SAFETY: the string is NUL-terminated, so the search stops inside it.
    let s = unsafe { std::ffi::CStr::from_ptr(*p as *const std::ffi::c_char) }.to_bytes();
    let end = s.iter().position(|&c| c == b'%')?;
    *p = unsafe { p.add(end + 1) };
    let name = |s: &[u8]| {
        let n = if end > 0 && s[end - 1] == b'[' { end - 1 } else { end };
        (n.wrapping_sub(1) <= 99).then(|| String::from_utf8_lossy(&s[1..n]).into_owned())
    };
    let embed = |name: &[u8], buf: &mut [u8; 8]| {
        let i = ui.embedded(&String::from_utf8_lossy(name))?;
        encode(0xf_2002 + (i as u32 & 0xff), buf);
        Some(buf.as_ptr())
    };
    let c = *s.first()?;
    match c {
        b'$' => {
            let i = ui.image(&name(s)?);
            if i == -1 {
                return Some(EMPTY.as_ptr());
            }
            encode(0xf_0000 + i as u32, buf);
            return Some(buf.as_ptr());
        }
        b'@' => {
            let n = name(s)?;
            return embed(n.as_bytes(), buf);
        }
        b'<' | b'>' => {
            if end != 1 {
                return None;
            }
            encode(if c == b'<' { 0xf_2000 } else { 0xf_2001 }, buf);
            return Some(buf.as_ptr());
        }
        _ => {}
    }
    // A command's key from the token table: `{a|b}` and `[a|b]` take `b` with the keyboard's
    // table (its input mode, `+0`, is 0) unless it is empty; `&` asks for the other form.
    let (mut start, mut len, mut other) = (0, end, c == b'&');
    if c | 0x20 == b'{' {
        if s.get(1) != Some(&c) {
            if end == 0 || s[end - 1] != c + 2 {
                return None;
            }
            let bar = s.iter().position(|&b| b == b'|')?;
            if end < bar {
                return None;
            }
            (start, len) = if end - bar > 2 { (bar + 1, end - bar - 2) } else { (1, bar - 1) };
        }
        other = false;
    } else if other {
        (start, len) = (1, end - 1);
    }
    if s.get(start) == Some(&b'&') {
        (start, len, other) = (start + 1, len.wrapping_sub(1), true);
    }
    if s.get(start) == Some(&b'@') {
        if len.wrapping_sub(1) > 0x63 {
            return None;
        }
        return embed(&s[start + 1..start + len], buf);
    }
    Some(ui.tokens.find(s.get(start..start + len)?, other))
}

static EMPTY: [u8; 1] = [0];

impl Ui {
    /// Lays out the text job at `offset` in the element's jobs (`0x2f55e0`), and returns the
    /// size it takes, `w, w, h, h`: its widest line times its fit scale, and the height of its
    /// lines.
    pub fn lay_out(&self, element: &Element, frame: &mut Frame, offset: u32) -> V4 {
        let Some(job) = element.jobs.get(offset as usize..offset as usize + 0x16) else { return [0.0; 4] };
        let style = |k: usize| element.styles.get(job[6 + k] as usize * STYLE_SIZE..(job[6 + k] as usize + 1) * STYLE_SIZE).unwrap_or(&[]);
        let field = |frame: &Frame, k: usize, at: usize| frame.f32(u16_at(style(k), at) as usize);
        let mut faces: [Option<(&FontAtlas, &Face)>; 4] = [None; 4];
        let (mut size, mut adv, mut asc, mut desc) = ([0.0f32; 4], [0.0f32; 4], [0.0f32; 4], [0.0f32; 4]);
        for k in 0..4 {
            let Some(f) = self.font(frame.u16(u16_at(style(k), 0x2c) as usize)) else { return [0.0; 4] };
            faces[k] = Some(f);
            size[k] = field(frame, k, 0x36);
            adv[k] = size[k] * field(frame, k, 0x38);
            asc[k] = size[k] * f.1.metrics[2] - field(frame, k, 0x40);
        }
        for k in 0..4 {
            desc[k] = size[k] - asc[k];
        }
        let faces = faces.map(|f| f.unwrap());
        let spacing = frame.f32(u16_at(job, 0x14) as usize);
        let wrap = frame.f32(u16_at(job, 0x10) as usize);
        let align = frame.f32(u16_at(job, 0x12) as usize);
        let fit = frame.f32(u16_at(job, 0xc) as usize);
        let mut p = frame.u64(u16_at(job, 0xa) as usize) as *const u8;
        if p.is_null() {
            return [0.0; 4];
        }

        let s = &mut frame.text;
        let (first_line, first_image, first_nested) = (s.line_count, s.image_count, s.nested_count);
        let mut image_starts = [0u32; MAX_LINES + 2];
        image_starts[0] = first_image;
        let mut image_mark = first_image;
        let mut nested_starts = [0u32; MAX_LINES + 2];
        nested_starts[0] = first_nested;
        let mut nested_mark = first_nested;
        // The line-break table is the first style's atlas's, whichever style a glyph is in.
        let breaks = faces[0].0;
        let mut prev_class = faces[0].1.glyphs.first().map_or(0, |g| g.class) as u32;
        let (mut cur, mut face) = (0usize, faces[0].1);
        let (mut x, mut line_x, mut y, mut widest) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let (mut break_x, mut break_at, mut break_char) = (0.0f32, 0.0f32, 0u32);
        let mut space = 0u32;
        let mut stack = [std::ptr::null::<u8>(); MAX_DEPTH];
        let mut depth = 0;
        let (mut chars, mut prev) = (0u32, 0u32);
        let mut span: Option<usize> = None;
        let (mut used, mut tallest) = (0u32, 0u32);
        let mut line_h = desc[0] + asc[0];
        let mut buf = [0u8; 8];

        let new_line = |s: &mut Scratch, end: u32, width: f32| {
            let n = s.line_count;
            s.line_count += 1;
            if (n as usize) < MAX_LINES {
                s.lines[n as usize] = Line { end, width, rest: 0.0 };
            }
        };
        loop {
            // SAFETY: `p` stays inside the job's string or a substitution's, both NUL-terminated.
            let (c, took) = unsafe { decode(p) };
            p = unsafe { p.add(took) };
            chars += 1;
            if c.wrapping_sub(1) < 0xe_ffff && c != 0x60 {
                // SAFETY: as above.
                if c == b'%' as u32 && (unsafe { *p } as i8) > b' ' as i8 {
                    if unsafe { *p } != b'%' {
                        if depth == MAX_DEPTH {
                            break;
                        }
                        let Some(sub) = (unsafe { substitute(self, &mut p, &mut buf) }) else { break };
                        stack[depth] = p;
                        depth += 1;
                        p = sub;
                        continue;
                    }
                    p = unsafe { p.add(1) };
                }
                let g = face.glyph(c).unwrap_or(0) as usize;
                let (Some(glyph), Some(after)) = (face.glyphs.get(g), face.glyphs.get(g + 1)) else { break };
                let kern = face.kerning.get(glyph.kerning as usize..after.kerning as usize).and_then(|k| k.iter().find(|k| k.0 == prev)).map_or(0.0, |k| k.1);
                let kerned = kern * adv[cur] + x;
                x = adv[cur] * glyph.advance + kerned;
                if span.is_none() {
                    if c == b' ' as u32 {
                        space = 1;
                    } else {
                        if c == b'\n' as u32 {
                            new_line(s, chars, line_x);
                            let rel = s.line_count - first_line;
                            image_mark = s.image_count;
                            nested_mark = s.nested_count;
                            let step = spacing + line_h;
                            widest = maxss(widest, line_x);
                            x = 0.0;
                            line_h = desc[cur] + asc[cur];
                            y += step;
                            if (rel as usize) < MAX_LINES {
                                image_starts[rel as usize] = image_mark;
                                nested_starts[rel as usize] = nested_mark;
                            }
                        } else {
                            let bit = space + (breaks.classes as u32 * prev_class + glyph.class as u32) * 2;
                            if breaks.breaks.get((bit >> 3) as usize).is_some_and(|b| b & (1 << (bit & 7)) != 0) {
                                image_mark = s.image_count;
                                nested_mark = s.nested_count;
                                (break_x, break_at, break_char) = (line_x, kerned, chars);
                            }
                            if wrap < x {
                                let step = spacing + line_h;
                                let shift = -break_at;
                                line_h = desc[cur] + asc[cur];
                                for i in image_mark as usize..(s.image_count as usize).min(MAX_IMAGES) {
                                    let m = &mut s.images[i];
                                    m.x0 += shift;
                                    m.y0 += step;
                                    m.x1 += shift;
                                    m.y1 += step;
                                }
                                for n in &mut s.nested[nested_mark as usize..(s.nested_count as usize).min(MAX_NESTED)] {
                                    n.x += shift;
                                    n.y += step;
                                }
                                new_line(s, break_char, break_x);
                                let rel = s.line_count - first_line;
                                x += shift;
                                y += step;
                                widest = maxss(widest, break_x);
                                if (rel as usize) < MAX_LINES {
                                    image_starts[rel as usize] = image_mark;
                                    nested_starts[rel as usize] = nested_mark;
                                }
                                image_mark = s.image_count;
                                nested_mark = s.nested_count;
                            }
                        }
                        prev_class = glyph.class as u32;
                        space = 0;
                        line_x = x;
                    }
                }
                prev = c;
                continue;
            }
            if c == 0 {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                p = stack[depth];
                continue;
            }
            if c == 0x60 {
                // SAFETY: as above.
                let d = unsafe { *p }.wrapping_sub(b'0') as usize;
                if d > 3 {
                    break;
                }
                p = unsafe { p.add(1) };
                let h = desc[d] + asc[d];
                face = faces[d].1;
                let before = line_h;
                if h > line_h {
                    line_h = h;
                }
                used |= 1 << d;
                if h > before {
                    tallest = d as u32;
                }
                cur = d;
                prev = 0;
                continue;
            }
            if c.wrapping_sub(0xf_0000) < 0x2000 {
                let n = s.image_count as usize;
                s.image_count += 1;
                let at = n.min(MAX_IMAGES - 1);
                s.images[at].x0 = x;
                s.images[at].image = c as u16;
                s.images[at].style = cur as u16;
                used = 1 << cur;
                let entry = self.images.get(c as u16 as usize);
                let atlas = entry.and_then(|e| self.lib.atlases.get(e.atlas as usize));
                let index = entry.map_or(0, |e| e.index as i16 as isize as usize);
                let px = atlas.and_then(|a| a.sizes.get(index)).copied().unwrap_or([0, 0]);
                if prev == 0xf_2000 {
                    span = Some(at);
                    if let Some(sl) = atlas.and_then(|a| a.slices.get(index)) {
                        x += px[0] as f32 * sl[0];
                    }
                } else {
                    let top = y - asc[cur];
                    s.images[at].y0 = top;
                    x += (px[0] as f32 / px[1] as f32) * adv[cur];
                    s.images[at].x1 = x;
                    s.images[at].y1 = top + size[cur];
                }
                prev_class = 0;
                space = 0;
                line_x = x;
            } else if c.wrapping_sub(0xf_2002) < 0x100 {
                if (s.nested_count as usize) < MAX_NESTED
                    && let Some(e) = self.embedded.get((c - 0xf_2002) as usize)
                {
                    let unit = |v: f32| if v == 0.0 { 1.0 } else { v };
                    let (w, h) = (unit(e.size[0]), unit(e.size[1]));
                    let own = self.lib.elements.get(&e.element).map_or([0.0; 2], |el| el.size);
                    let k = size[cur] / h;
                    s.nested[s.nested_count as usize] = Nested { style: cur as u16, index: (c - 0xf_2002) as u8, x, y, w: k * w, h: k * h, scale: [own[0] / w, own[1] / h] };
                    s.nested_count += 1;
                    x += k * w;
                }
                prev_class = 0;
                space = 0;
                line_x = x;
            } else if c == 0xf_2001
                && let Some(at) = span
            {
                let (mut lo, mut hi) = (y, y);
                let mut bits = used;
                while bits != 0 {
                    let d = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    let t = y - asc[d];
                    lo = minss(lo, t);
                    hi = maxss(hi, t + size[d]);
                }
                let entry = self.images.get(s.images[at].image as usize);
                let atlas = entry.and_then(|e| self.lib.atlases.get(e.atlas as usize));
                let index = entry.map_or(0, |e| e.index as i16 as isize as usize);
                if let (Some(px), Some(sl)) = (atlas.and_then(|a| a.sizes.get(index)), atlas.and_then(|a| a.slices.get(index))) {
                    x += px[0] as f32 * sl[2];
                }
                s.images[at].x1 = x;
                s.images[at].y0 = lo;
                s.images[at].y1 = hi;
                span = None;
                prev_class = 0;
                space = 0;
                line_x = x;
            }
            prev = c;
        }

        let n = s.line_count;
        widest = maxss(widest, x);
        if n != first_line && n < MAX_LINES as u32 + 1 {
            let rel = (n - first_line) as usize;
            s.lines[n as usize - 1].rest = x;
            image_starts[rel + 1] = s.image_count;
            nested_starts[rel + 1] = s.nested_count;
            for i in 0..=rel {
                let w = if i < rel { s.lines[first_line as usize + i].width } else { s.lines[first_line as usize + rel - 1].rest };
                let dx = (widest - w) * align;
                for k in image_starts[i] as usize..(image_starts[i + 1] as usize).min(MAX_IMAGES) {
                    s.images[k].x0 += dx;
                    s.images[k].x1 += dx;
                }
                for k in nested_starts[i] as usize..(nested_starts[i + 1] as usize).min(MAX_NESTED) {
                    s.nested[k].x += dx;
                }
            }
        }
        let scale = fit / maxss(fit, widest);
        s.jobs.insert(
            offset >> 4,
            Laid {
                scale,
                first_line: first_line as u8,
                lines: (s.line_count as u8).wrapping_sub(first_line as u8),
                first_image: first_image as u8,
                images: (s.image_count as u8).wrapping_sub(first_image as u8),
                tallest: tallest as u8,
                first_nested: first_nested as u8,
                nested: (s.nested_count as u8).wrapping_sub(first_nested as u8),
            },
        );
        if scale < 1.0 {
            for i in (first_image as u8) as usize..(s.image_count as usize).min(MAX_IMAGES) {
                let m = &mut s.images[i];
                let h = m.y1 - m.y0;
                let y1 = m.y1 - (h - h * scale) * 0.5;
                m.y1 = y1;
                m.y0 = y1 - h;
            }
            for n in &mut s.nested[(first_nested as u8) as usize..(s.nested_count as usize).min(MAX_NESTED)] {
                n.y += (n.h - scale * n.h) * 0.5;
            }
        }
        let (w, h) = (scale * widest, y + line_h);
        [w, w, h, h]
    }

    /// A string's size in ems in the face at `slot` (`0x2f5190`): its widest line, and how many
    /// lines it has (a newline at its end not counted). `caps` measures it upper case, as the
    /// C runtime's `towupper` makes it in the C locale (`a` to `z` only, `0x11c331c`). Unlike
    /// the layout, it does not wrap, a `` ` `` escape changes no style, and an image adds its
    /// width over its height.
    ///
    /// # Safety
    /// `text` points at a NUL-terminated string.
    pub unsafe fn text_size(&self, text: *const u8, slot: u16, caps: bool) -> [f32; 2] {
        let Some((_, face)) = self.font(slot) else { return [0.0, 1.0] };
        let (mut x, mut widest, mut lines, mut span) = (0.0f32, 0.0f32, 1.0f32, 0.0f32);
        let (mut p, mut prev) = (text, 0u32);
        let mut stack: Vec<*const u8> = Vec::new();
        let mut buf = [0u8; 8];
        loop {
            // SAFETY: `p` stays inside the string or a substitution's, both NUL-terminated.
            let (c, took) = unsafe { decode(p) };
            p = unsafe { p.add(took) };
            if c.wrapping_sub(1) < 0xe_ffff && c != 0x60 {
                if c == b'%' as u32 {
                    // SAFETY: as above.
                    let next = unsafe { *p };
                    if next == b'%' {
                        p = unsafe { p.add(1) };
                    } else if next != 0 {
                        let Some(sub) = (unsafe { substitute(self, &mut p, &mut buf) }) else { break };
                        stack.push(p);
                        p = sub;
                        continue;
                    }
                }
                let cu = if caps && (c as u16).wrapping_sub(b'a' as u16) < 26 { (c as u16 - 0x20) as u32 } else { c as u16 as u32 };
                let g = face.glyph(if caps { cu } else { c }).unwrap_or(0) as usize;
                let (Some(glyph), Some(after)) = (face.glyphs.get(g), face.glyphs.get(g + 1)) else { break };
                let kern = face.kerning.get(glyph.kerning as usize..after.kerning as usize).and_then(|k| k.iter().find(|k| k.0 == prev)).map_or(0.0, |k| k.1);
                x = x + kern + glyph.advance;
                widest = if x <= widest { widest } else { x };
                prev = c;
                if c == b'\n' as u32 {
                    x = 0.0;
                    lines += 1.0;
                }
                continue;
            }
            if c == 0 {
                let Some(back) = stack.pop() else { break };
                p = back;
                continue;
            }
            if c == 0x60 {
                // SAFETY: as above.
                if unsafe { *p } != 0 {
                    let (d, took) = unsafe { decode(p) };
                    p = unsafe { p.add(took.max(1)) };
                    if d.wrapping_sub(b'0' as u32) > 3 && d != 0x60 {
                        break;
                    }
                }
                continue;
            }
            if c.wrapping_sub(0xf_0000) < 0x2000 {
                let entry = self.images.get(c as u16 as usize);
                let atlas = entry.and_then(|e| self.lib.atlases.get(e.atlas as usize));
                let index = entry.map_or(0, |e| e.index as i16 as isize as usize);
                let px = atlas.and_then(|a| a.sizes.get(index)).copied().unwrap_or([0, 0]);
                if prev == 0xf_2000 {
                    if let Some(sl) = atlas.and_then(|a| a.slices.get(index)) {
                        span = px[0] as f32 * sl[2];
                        x += px[0] as f32 * sl[0];
                    }
                } else {
                    x += px[0] as f32 / px[1] as f32;
                }
            } else if c.wrapping_sub(0xf_2002) < 0x100 {
                if let Some(e) = self.embedded.get((c - 0xf_2002) as usize)
                    && e.size[1] != 0.0
                {
                    x += e.size[0] / e.size[1];
                }
            } else if c == 0xf_2001 {
                x += span;
                span = 0.0;
            }
        }
        if prev == b'\n' as u32 {
            lines -= 1.0;
        }
        [widest, lines]
    }
}

/// A glyph the text job has placed: where its pen stood, its index in its face, the glyph.
#[derive(Clone, Copy)]
struct Placed<'a> {
    x: f32,
    index: u32,
    glyph: Option<&'a Glyph>,
}

/// What the text job draws with in one of the job's styles (the set-up at `0x2f88f0`).
struct Run<'a> {
    face: &'a Face,
    style: u32,
    visible: bool,
    /// The pen's advance per em, and an em's height, in the job's box.
    adv: f32,
    vs: f32,
    /// How far each side of a glyph's quad reaches past it: left, top, right, bottom.
    margin: V4,
    /// The box to the font texture, times the box's inverse, and its origin.
    tex: V4,
    tex_inv: V4,
    tex_origin: V4,
    spacing: f32,
}

impl Ui {
    /// The text job (`0x2f7db0`): the glyphs the layout placed, two to a quad. Each quad runs
    /// from halfway between the glyph before its first and its second glyph's neighbour, and
    /// carries both glyphs for the shader to test each pixel against.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn text_job(&self, element: &Element, rui: &Rui, frame: &Frame, job: &[u8], offset: usize, style_base: u32, out: &mut Draws) {
        let data = &rui.data;
        let f = |at: u16| data.f32(at as usize);
        let ti = (u16_at(job, 2) as usize).min(frame.transforms.len() - 1);
        let (t, size) = (frame.transforms[ti], frame.sizes[ti]);
        if size.contains(&0.0) {
            return;
        }
        let b = t.basis;
        let det = b[3] * b[0] - b[1] * b[2];
        if det == 0.0 {
            return;
        }
        let inv = [b[3] / det, -b[1] / det, -b[2] / det, b[0] / det];
        let mirrored = det.is_sign_negative();
        let rs = recip(size);
        let o = [t.origin[0], t.origin[2], t.origin[1], t.origin[3]];
        let n = [-inv[0] * o[0], -inv[1] * o[1], -inv[2] * o[2], -inv[3] * o[3]];
        let inv_origin = [n[2] + n[0], n[3] + n[1], n[0] + n[2], n[1] + n[3]];
        let record = |k: usize| element.styles.get(job[6 + k] as usize * STYLE_SIZE..(job[6 + k] as usize + 1) * STYLE_SIZE).unwrap_or(&[]);
        let mut faces: [Option<(&FontAtlas, &Face)>; 4] = [None; 4];
        for (k, face) in faces.iter_mut().enumerate() {
            *face = self.font(data.u16(u16_at(record(k), 0x2c) as usize));
        }
        let Some((atlas, _)) = faces[0] else { return };
        let atlas_index = self.lib.fonts.iter().position(|a| std::ptr::eq(a, atlas)).unwrap_or(0) as u8;
        let Some(&laid) = frame.text.jobs.get(&((offset >> 4) as u32)) else { return };
        let s = &frame.text;
        let asc = |k: usize| faces[k].map_or(0.0, |(_, face)| f(u16_at(record(k), 0x36)) * face.metrics[2] - f(u16_at(record(k), 0x40)));
        let mut by = rs[3] * asc(laid.tallest as usize & 3);
        let scale = laid.scale;
        // The images the layout placed (`0x2f7f60` on): each image's box in the job's box, its
        // frame mapped across it, through the emitter with no anti-aliased side.
        for m in s.images.iter().skip(laid.first_image as usize).take(laid.images as usize) {
            let Some(entry) = self.images.get(m.image as usize) else { continue };
            let Some(atlas) = self.lib.atlases.get(entry.atlas as usize) else { continue };
            let Some(fr) = atlas.frames.get(entry.index as usize) else { continue };
            let p0 = [rs[0] * m.x0 * scale, rs[2] * m.y0 * scale, rs[1] * m.x0 * scale, rs[3] * m.y0 * scale];
            let p1 = [rs[0] * m.x1 * scale, rs[2] * m.y1 * scale, rs[1] * m.x1 * scale, rs[3] * m.y1 * scale];
            let d: V4 = std::array::from_fn(|k| p1[k] - p0[k]);
            let p0 = [p0[0] + 0.0, p0[1] + by, p0[2] + 0.0, p0[3] + by];
            let r = recip(d);
            let clip = [-fr[0] * d[0] + p0[0], -fr[1] * d[1] + p0[1], fr[2] * d[2] + p0[2], fr[3] * d[3] + p0[3]];
            let f67 = [fr[6], fr[7], fr[6], fr[7]];
            let uv: V4 = std::array::from_fn(|k| inv[k] * r[k] * f67[k]);
            let at: V4 = std::array::from_fn(|k| (inv_origin[k] - p0[k]) * r[k] * f67[k] + if k < 2 { fr[4 + k] } else { 0.0 });
            let rows = [[uv[0], uv[1], uv[0], uv[1]], [uv[2], uv[3], uv[2], uv[3]], at];
            let proto = Quad {
                corners: [[0.0; 2]; 4],
                uv: [[0.0; 4]; 4],
                text: [[0.0; 2]; 4],
                image: entry.index as i32,
                atlas: entry.atlas,
                mask: -1,
                mask_atlas: 0,
                style: style_base + job[6 + (m.style as usize & 3)] as u32,
                flags: 0x1f00,
            };
            let (xs, ys) = ([clip[0], clip[0], clip[2], clip[2]], [clip[1], clip[3], clip[3], clip[1]]);
            match atlas.slices.get(entry.index as usize) {
                Some(slice) => {
                    let p: V4 = std::array::from_fn(|k| -(r[k] * p0[k]));
                    for q in nine_slice(slice, rui.size, &t, r, p, clip, [0.0, 0.0, 1.0, 1.0], rows) {
                        emit(frame, element.size, &t, q.xs, q.ys, 0, q.rows, &proto, out);
                    }
                }
                None => emit(frame, element.size, &t, xs, ys, 0, rows, &proto, out),
            }
        }
        // The elements embedded in it (`0x2f8348` on): a new instance of each, with the
        // registry's arguments and its style's colour as `parentColor`, drawn into its box.
        // Not modelled: the parent's colour matrix carried into the embedded styles.
        for n in s.nested.iter().skip(laid.first_nested as usize).take(laid.nested as usize) {
            let Some(e) = self.embedded.get(n.index as usize) else { continue };
            let Some(el) = self.lib.elements.get(&e.element) else { continue };
            if el.data_size as usize + 0x60 >= 0x271 {
                continue;
            }
            let Some(mut inst) = self.create(&e.element, rui.group, rui.sort_key) else { continue };
            inst.no_safe_area = true;
            for (name, v) in &e.strings {
                inst.set_string(self, name, &v.to_string_lossy());
            }
            let st = record(n.style as usize & 3);
            let c = |k: usize| f(u16_at(st, k));
            inst.set_value(self, "parentColor", super::Value::Vector([c(2), c(4), c(6), c(8)]));
            let (w, h) = (rs[0] * n.w * n.scale[0] * scale, rs[2] * n.h * n.scale[1] * scale);
            let (x, y) = (rs[0] * n.x * 1.0 * scale, rs[2] * n.y * 1.0 * scale);
            let basis = [w * b[0], w * b[1], h * b[2], h * b[3]];
            let o = t.origin;
            let origin = [(x * b[0] + y * b[2]) + o[0], (x * b[1] + y * b[3]) + o[1], (y * b[2] + x * b[0]) + o[2], (y * b[3] + x * b[1]) + o[3]];
            let rect = [el.size[0], el.size[0], el.size[1], el.size[1]];
            let mut nf = Frame::default();
            if let Ok(true) = self.update_at(&mut inst, &frame.globals, &frame.topology, &mut nf, rect, Transform { basis, origin }) {
                self.draws(&inst, &nf, out);
            }
        }
        let line_spacing = rs[3] * f(u16_at(job, 0x14));
        let align = rs[0] * f(u16_at(job, 0x12));
        let box_size = [size[0], size[2], size[0], size[2]];

        let run = |k: usize| -> Option<Run> {
            let r = record(k);
            let (_, face) = faces[k]?;
            let g = |at: usize| f(u16_at(r, at));
            let sz = g(0x36);
            let shadow = { let x = g(0x3c); if x > 0.0 { x } else { 0.0 } };
            let adv = (laid.scale * g(0x38)) * sz;
            let fitted = (sz * laid.scale - sz) * (f(u16_at(job, 0xe)) - 0.0) + sz;
            let alpha = { let a = g(0x10); let b = g(0x18); if a > b { a } else { b } };
            let capped = { let c = g(0x3a); if alpha < c { alpha } else { c } };
            let visible = { let a = g(8); if a > capped { a } else { capped } } > 0.0;
            let reach = g(0x3a) + shadow;
            let (half, floor) = (g(0x34) * 0.5, g(0x3e) * 0.5);
            let off = [-g(0x30), -g(0x32), g(0x30), g(0x32)];
            let rsb = [rs[0], rs[2], rs[1], rs[3]];
            let margin: V4 = std::array::from_fn(|i| {
                let m = off[i] + half;
                let m = if m > floor { m } else { floor } + reach;
                (if i < 2 { -m } else { m }) * rsb[i]
            });
            let inv_size = recip([adv, fitted, adv, fitted]);
            let em = [face.metrics[0], face.metrics[1], face.metrics[0], face.metrics[1]];
            let tex: V4 = std::array::from_fn(|i| inv_size[i] * box_size[i] * em[i]);
            Some(Run {
                face,
                style: job[6 + k] as u32 + style_base,
                visible,
                adv: adv * rs[0],
                vs: rs[3] * fitted,
                margin,
                tex,
                tex_inv: std::array::from_fn(|i| inv[i] * tex[i]),
                tex_origin: std::array::from_fn(|i| tex[i] * inv_origin[i]),
                spacing: g(0x42),
            })
        };

        let mut p = data.u64(u16_at(job, 0xa) as usize) as *const u8;
        if p.is_null() {
            return;
        }
        let (mut count, mut k) = (0u32, 0usize);
        // SAFETY: the string is NUL-terminated, so a byte read stops at its end.
        while unsafe { *p } == b'`' && unsafe { *p.add(1) }.wrapping_sub(b'0') < 4 {
            k = (unsafe { *p.add(1) } - b'0') as usize;
            p = unsafe { p.add(2) };
            count += 1;
        }
        let (mut line, end_line) = (laid.first_line as u32, laid.first_line as u32 + laid.lines as u32);
        let mut line_end = u32::MAX;
        let mut x = 0.0f32;
        if line < end_line {
            let l = s.lines[(line as usize).min(MAX_LINES - 1)];
            x = (box_size[0] - l.width) * align;
            line_end = l.end;
            line += 1;
        }
        let Some(mut r) = run(k) else { return };
        let none = Placed { x: 0.0, index: 0, glyph: None };
        let (mut cur, mut a, mut bq) = (none, none, none);
        let (mut pending, mut signs) = (0u32, START_SIGNS);
        let (mut left, mut lt, mut lb) = (0.0f32, 0.0f32, 0.0f32);
        let (mut prev, mut span_right) = (0u32, 0.0f32);
        let mut stack = [std::ptr::null::<u8>(); MAX_DEPTH];
        let mut depth = 0;
        let mut buf = [0u8; 8];
        loop {
            // SAFETY: `p` stays inside the job's string or a substitution's, both NUL-terminated.
            let (c, took) = unsafe { decode(p) };
            p = unsafe { p.add(took) };
            count += 1;
            if c == b'%' as u32 && (unsafe { *p } as i8) > b' ' as i8 {
                if unsafe { *p } != b'%' {
                    if depth == MAX_DEPTH {
                        return;
                    }
                    let Some(sub) = (unsafe { substitute(self, &mut p, &mut buf) }) else { return };
                    stack[depth] = p;
                    depth += 1;
                    p = sub;
                    continue;
                }
                p = unsafe { p.add(1) };
            } else if c == 0 && depth > 0 {
                depth -= 1;
                p = stack[depth];
                continue;
            }
            let mut glyph = None;
            let mut adv_g = 0.0f32;
            let is_glyph = c.wrapping_sub(1) < 0xe_ffff && c != 0x60;
            if is_glyph {
                let index = r.face.glyph(c).unwrap_or(0);
                let (Some(g), Some(after)) = (r.face.glyphs.get(index as usize), r.face.glyphs.get(index as usize + 1)) else { return };
                let kern = r.face.kerning.get(g.kerning as usize..after.kerning as usize).and_then(|k| k.iter().find(|k| k.0 == prev)).map_or(0.0, |k| k.1);
                adv_g = r.adv * g.advance;
                x += (r.spacing + kern) * r.adv;
                cur = Placed { x, index, glyph: Some(g) };
                glyph = Some(g);
            }
            let mut emit: Option<(f32, f32, f32)> = None;
            if r.visible {
                if count >= line_end || !is_glyph {
                    if pending != 0 {
                        if pending == 1 {
                            a = bq;
                            signs[0] += 1.0;
                        }
                        let (ga, gb) = (a.glyph.unwrap(), bq.glyph.unwrap());
                        let right = r.adv * gb.extent[2] + bq.x + r.margin[2] + rs[0];
                        pending = 0;
                        signs[1] += 1.0;
                        let top = minss(ga.extent[1], gb.extent[1]) * r.vs + r.margin[1];
                        let bottom = maxss(ga.extent[3], gb.extent[3]) * r.vs + r.margin[3];
                        emit = Some((right, top, bottom));
                    }
                } else {
                    let g = glyph.unwrap();
                    if g.extent[0] == g.extent[2] {
                        x += adv_g;
                        prev = c;
                        continue;
                    }
                    if pending <= 1 {
                        let top = r.vs * g.extent[1] + r.margin[1];
                        let bottom = r.vs * g.extent[3] + r.margin[3];
                        if pending == 0 {
                            signs[0] -= 1.0;
                            left = (g.extent[0] * r.adv + r.margin[0]) + x;
                            (lt, lb) = (top, bottom);
                        } else {
                            lt = minss(lt, top);
                            lb = maxss(lb, bottom);
                        }
                        x += adv_g;
                        pending += 1;
                        (a, bq) = (bq, cur);
                        prev = c;
                        continue;
                    }
                    let (ga, gb) = (a.glyph.unwrap(), bq.glyph.unwrap());
                    let right = ((ga.extent[2] + g.extent[0]) * r.adv + (a.x + cur.x)) * 0.5;
                    let bottom = maxss(maxss(ga.extent[3], gb.extent[3]), g.extent[3]) * r.vs + r.margin[3];
                    let top = minss(minss(ga.extent[1], gb.extent[1]), g.extent[1]) * r.vs + r.margin[1];
                    emit = Some((right, top, bottom));
                }
            }
            if let Some((right, rt, rb)) = emit {
                self.glyph_quad(frame, element, &t, mirrored, &r, atlas_index, (a, bq), [left, lt, lb], [right, rt, rb], by, signs, out);
                signs = [0.0, 0.0, signs[2], signs[3]];
                (left, lt, lb) = (right, rt, rb);
            }
            if count >= line_end {
                by += r.vs + line_spacing;
                let width;
                if line < end_line {
                    let l = s.lines[(line as usize).min(MAX_LINES - 1)];
                    (line_end, width) = (l.end, l.width);
                    line += 1;
                } else {
                    line_end = u32::MAX;
                    width = s.lines[(end_line as usize).saturating_sub(1).min(MAX_LINES - 1)].rest;
                }
                x = (box_size[0] - width) * align;
                match glyph {
                    Some(g) if g.extent[0] != g.extent[2] => {
                        pending = 1;
                        left = (g.extent[0] * r.adv + r.margin[0]) + x;
                        cur.x = x;
                        lt = r.vs * g.extent[1] + r.margin[1];
                        lb = r.vs * g.extent[3] + r.margin[3];
                    }
                    _ => pending = 0,
                }
            }
            x += adv_g;
            if is_glyph {
                (a, bq) = (bq, cur);
                prev = c;
                continue;
            }
            match c {
                0 => return,
                0x60 => {
                    // SAFETY: as above.
                    let d = unsafe { *p }.wrapping_sub(b'0') as usize;
                    if d > 3 {
                        return;
                    }
                    p = unsafe { p.add(1) };
                    let Some(next) = run(d) else { return };
                    r = next;
                    (pending, signs) = (0, START_SIGNS);
                }
                _ if c.wrapping_sub(0xf_0000) < 0x2000 => {
                    let entry = self.images.get(c as u16 as usize);
                    let atlas = entry.and_then(|e| self.lib.atlases.get(e.atlas as usize));
                    let i = entry.map_or(0, |e| e.index as i16 as isize as usize);
                    let px = atlas.and_then(|a| a.sizes.get(i)).copied().unwrap_or([0, 0]);
                    if prev == 0xf_2000 {
                        match atlas.and_then(|a| a.slices.get(i)) {
                            Some(sl) => {
                                let w = px[0] as f32 * rs[0];
                                span_right = w * sl[2];
                                x += w * sl[0];
                            }
                            None => span_right = 0.0,
                        }
                    } else {
                        x += (px[0] as f32 / px[1] as f32) * r.adv;
                    }
                }
                0xf_2001 => {
                    x += span_right;
                    span_right = 0.0;
                }
                _ => {}
            }
            prev = c;
        }
    }

    /// One quad of two glyphs (`0x2f8f95`): the box from `left` to `right`, each side's top
    /// and bottom below the line's baseline `by`, through the job's transform, padded on the
    /// sides `signs` says, with both glyphs' texture coordinates over it.
    #[allow(clippy::too_many_arguments)]
    fn glyph_quad(&self, frame: &Frame, element: &Element, t: &super::transform::Transform, mirrored: bool, r: &Run, atlas: u8, (a, b): (Placed, Placed), [left, lt, lb]: [f32; 3], [right, rt, rb]: [f32; 3], by: f32, signs: V4, out: &mut Draws) {
        let (Some(ga), Some(gb)) = (a.glyph, b.glyph) else { return };
        let scale = |g: &Glyph| r.face.scales.get(g.size as usize).map_or(0.0, |s| s[0]);
        let (sa, sb) = (scale(ga), scale(gb));
        let s4 = [sa, sa, sb, sb];
        let pos = [a.x * r.tex[0], by * r.tex[1], b.x * r.tex[2], by * r.tex[3]];
        let uv0 = [ga.uv[0], ga.uv[1], gb.uv[0], gb.uv[1]];
        let origin: V4 = std::array::from_fn(|i| (r.tex_origin[i] - pos[i]) * s4[i] + uv0[i]);
        let ddx = [sa * r.tex_inv[0], sa * r.tex_inv[1], sb * r.tex_inv[0], sb * r.tex_inv[1]];
        let ddy = [sa * r.tex_inv[2], sa * r.tex_inv[3], sb * r.tex_inv[2], sb * r.tex_inv[3]];
        let xs = [left, left, right, right];
        let ys = [lt + by, lb + by, rb + by, rt + by];
        let bs = t.basis;
        let mut c8 = [0.0f32; 8];
        for i in 0..4 {
            c8[i] = (bs[0] * xs[i] + bs[2] * ys[i]) + t.origin[0];
            c8[4 + i] = (bs[1] * xs[i] + bs[3] * ys[i]) + t.origin[1];
        }
        if !pad_edges(&frame.globals, &frame.topology, element.size, bs, signs, &mut c8) {
            return;
        }
        let k = |i: usize| [c8[i], c8[4 + i]];
        let corners = if mirrored { [k(1), k(0), k(3), k(2)] } else { [k(0), k(1), k(2), k(3)] };
        out.quads.push(Quad {
            corners,
            uv: corners.map(|c| std::array::from_fn(|i| ddx[i] * c[0] + ddy[i] * c[1] + origin[i])),
            text: [[sa, sb]; 4],
            image: (r.face.base + a.index) as i32,
            atlas,
            mask: (r.face.base + b.index) as i32,
            mask_atlas: atlas,
            style: r.style,
            flags: 0,
        });
    }
}

/// The sides a glyph run pads at first (`0x15bd860`): top and bottom, and the left once its
/// first glyph is placed, the right at its last.
const START_SIGNS: V4 = [0.0, 0.0, -1.0, 1.0];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::rui::{font_slot, Mapping};

    /// One text job in one style, its fields at fixed places in a data block.
    fn element() -> Element {
        let mut style = vec![0u8; STYLE_SIZE];
        for (at, off) in [(0x2c, 0x08u16), (0x36, 0x0c), (0x38, 0x10), (0x40, 0x14)] {
            style[at..at + 2].copy_from_slice(&off.to_le_bytes());
        }
        let mut job = vec![0u8; 0x16];
        for (at, off) in [(0xa, 0x00u16), (0xc, 0x18), (0x10, 0x1c), (0x12, 0x20), (0x14, 0x24)] {
            job[at..at + 2].copy_from_slice(&off.to_le_bytes());
        }
        Element {
            name: "test".into(),
            size: [1920.0, 1080.0],
            inv_size: [1.0 / 1920.0, 1.0 / 1080.0],
            defaults: Vec::new(),
            strings: Vec::new(),
            data_size: 0x28,
            code: Vec::new(),
            styles: style,
            jobs: job,
            job_count: 1,
            clusters: Vec::new(),
            args: Vec::new(),
            mappings: Vec::<Mapping>::new(),
            transforms: 3,
            elements: 0,
        }
    }

    fn lay_out(ui: &Ui, text: &str, wrap: f32) -> Scratch {
        let s = std::ffi::CString::new(text).unwrap();
        let mut data = [0u8; 0x28];
        data[0..8].copy_from_slice(&(s.as_ptr() as u64).to_le_bytes());
        data[8..10].copy_from_slice(&font_slot("DefaultRegularFont").to_le_bytes());
        for (at, v) in [(0x0c, 40.0f32), (0x10, 1.0), (0x14, 0.0), (0x18, 1e9), (0x1c, wrap), (0x20, 0.0), (0x24, 0.0)] {
            data[at..at + 4].copy_from_slice(&v.to_le_bytes());
        }
        let e = element();
        let mut frame = Frame { data: data.as_mut_ptr(), data_size: data.len(), ..Frame::default() };
        let size = ui.lay_out(&e, &mut frame, 0);
        frame.text.size = size;
        frame.text
    }

    #[test]
    fn lines_break_at_newlines_and_at_the_wrap_width() {
        let Some(dir) = std::env::var_os("LURCH_R5R") else { return };
        let ui = Ui::open(std::path::Path::new(&dir)).unwrap();
        let face = ui.font(font_slot("DefaultRegularFont")).unwrap().1;
        let advance = |c: char| 40.0 * face.glyphs[face.glyph(c as u32).unwrap() as usize].advance;
        let width = |s: &str| s.chars().map(advance).sum::<f32>();

        let t = lay_out(&ui, "II\nIII", 1e9);
        assert_eq!(t.line_count, 1);
        assert_eq!((t.lines[0].end, t.lines[0].width), (3, width("II")));
        assert_eq!(t.lines[0].rest, width("III"));
        assert_eq!(t.jobs[&0].lines, 1);
        let h = 40.0 + 40.0;
        assert_eq!(t.size, [width("III"), width("III"), h, h], "the widest line, and two lines of the size");

        let t = lay_out(&ui, "IIII IIII", width("IIII I"));
        assert_eq!(t.line_count, 1, "one break, before the second word");
        assert_eq!((t.lines[0].end, t.lines[0].width), (6, width("IIII")));
        assert!((t.lines[0].rest - width("IIII")).abs() < 1e-3, "{}", t.lines[0].rest);

        let t = lay_out(&ui, "``5", 1e9);
        assert_eq!(t.jobs[&0], Laid { scale: 1.0, ..Laid::default() }, "a bad style code ends the string");
    }
}
