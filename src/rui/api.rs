//! The 28 callbacks the engine hands every element function (`0x1300ad0`), in the table's
//! order. Each is `extern "win64"` because `ui.dll` calls it so. A callback not ported yet
//! records its slot and stops the frame, so the instance is not drawn from bad values.

use core::arch::x86_64::__m128;
use std::ffi::{c_char, CStr};

use super::transform::{Stop, V4};
use super::{Frame, Header, EMPTY_STRING};

pub const SLOTS: usize = 28;

/// The slots of the table by offset.
pub mod slot {
    pub const HIDE: usize = 0x00;
    pub const HIDE_NOW: usize = 0x08;
    pub const ERROR: usize = 0x10;
    pub const SIZES: usize = 0x18;
    pub const TEXT: usize = 0x20;
    pub const FIT: usize = 0x28;
    pub const RUN: usize = 0x30;
    pub const RUN_UNIT: usize = 0x38;
    pub const SPRINTF: usize = 0x40;
    pub const MAP1: usize = 0xa0;
    pub const IMAGE: usize = 0x90;
    pub const FONT: usize = 0xc8;
    pub const COLOR: usize = 0xd0;
}

fn header<'a>(inst: *mut u8) -> &'a mut Header {
    // SAFETY: `inst` is the header `Ui::update` passed to the element function, which hands
    // it back to us unchanged while the frame lasts.
    unsafe { &mut *(inst as *mut Header) }
}

fn frame<'a>(inst: *mut u8) -> &'a mut Frame {
    // SAFETY: the header's frame lives for the whole call.
    unsafe { &mut *header(inst).frame }
}

/// `+0x00` (`0x2fb430`): hidden this frame and marked for destruction (`+0x49`, `+0x48`;
/// `0x71e460` destroys an instance with `+0x48` set).
extern "win64" fn hide(inst: *mut u8) {
    let h = header(inst);
    h.hidden = true;
    h.hidden_now = true;
}

/// `+0x08` (`0x2fb440`): hidden this frame (`+0x49`).
extern "win64" fn hide_now(inst: *mut u8) {
    header(inst).hidden_now = true;
}

/// `+0x10`: an error the element's code found, such as a division by zero, with the source
/// line in the message. The image's entry points outside the executable; the element returns
/// without drawing after it, so the frame is hidden.
extern "win64" fn error(inst: *mut u8, msg: *const c_char) {
    let text = if msg.is_null() { String::new() } else {
        // SAFETY: the element passes a NUL-terminated string from its own data.
        unsafe { CStr::from_ptr(msg) }.to_string_lossy().into_owned()
    };
    let f = frame(inst);
    if f.errors.len() < 16 {
        f.errors.push(text.trim_end().to_string());
    }
    header(inst).hidden_now = true;
}

/// `+0x18` (`0x2fb450`): the frame's sizes, which the element fills before its code runs.
extern "win64" fn sizes(inst: *mut u8) -> *mut f32 {
    frame(inst).sizes.as_mut_ptr() as *mut f32
}

// The engine returns the size as an `__m128` in xmm0, as LLVM does for the Windows x64
// convention.
#[allow(improper_ctypes_definitions)]
/// `+0x20` (`0x2f55e0`): lays out the text job at `offset` in the element's jobs and returns
/// its size, `w, w, h, h`. Not run in a frame already stopped, whose strings may come from a
/// callback not ported.
extern "win64" fn text(inst: *mut u8, offset: u32) -> __m128 {
    if frame(inst).stop.is_some() {
        return m128([0.0; 4]);
    }
    let h = header(inst);
    // SAFETY: the element and the runtime outlive the frame.
    let (element, ui) = unsafe { (&*h.element, &*h.ui) };
    m128(ui.lay_out(element, frame(inst), offset))
}

#[allow(improper_ctypes_definitions)]
/// `+0x28` (`0x2f6540`): the box transforms `[a, b)` cover, then transforms `[c, b)` moved by
/// its corner so it starts at the origin; returns its size, `w, w, h, h`.
extern "win64" fn fit(inst: *mut u8, a: i32, b: i32, c: i32) -> __m128 {
    let f = frame(inst);
    let n = f.transforms.len() as i32;
    let (a, b, c) = (a.clamp(0, n), b.clamp(0, n), c.clamp(0, n));
    let (mut lo, mut hi) = ([f32::MAX; 4], [f32::MIN; 4]);
    let (min, max) = (|x: f32, y: f32| if x < y { x } else { y }, |x: f32, y: f32| if x > y { x } else { y });
    for t in &f.transforms[a as usize..b.max(a) as usize] {
        let (p, q) = ([t.basis[0], t.basis[1], t.basis[0], t.basis[1]], [t.basis[2], t.basis[3], t.basis[2], t.basis[3]]);
        lo = std::array::from_fn(|k| min(lo[k], (min(p[k], 0.0) + t.origin[k]) + min(q[k], 0.0)));
        hi = std::array::from_fn(|k| max(hi[k], (max(p[k], 0.0) + t.origin[k]) + max(q[k], 0.0)));
    }
    for t in &mut f.transforms[c as usize..b.max(c) as usize] {
        t.origin = std::array::from_fn(|k| t.origin[k] - lo[k]);
    }
    let d: V4 = std::array::from_fn(|k| hi[k] - lo[k]);
    m128([d[0], d[2], d[1], d[3]])
}

/// `+0x30` (`0x2f31d0`): runs the transform code up to `end`.
extern "win64" fn run(inst: *mut u8, end: i32) {
    // SAFETY: the element outlives the frame.
    let element = unsafe { &*header(inst).element };
    let f = frame(inst);
    let data = f.data;
    if f.stop.is_none()
        && let Err(s) = f.run(&element.code, end as usize, data)
    {
        f.stop = Some(s);
    }
    if std::mem::take(&mut f.hide_now) {
        header(inst).hidden_now = true;
    }
}

/// `+0x38` (`0x2f3230`): runs it with the unit size and reciprocal at one, then restores them.
extern "win64" fn run_unit(inst: *mut u8, end: i32) {
    frame(inst).unit = [1.0; 4];
    run(inst, end);
    let h = header(inst);
    frame(inst).unit = [h.size[0], h.size[1], h.inv_size[0], h.inv_size[1]];
}

/// What the string callbacks return when the buffer is full or the format fails (`0x133ffd0`).
static FAILED: &[u8; 4] = b"***\0";

/// `+0x40` (`0x2fb460`): `vsnprintf` (`printf`) into what is left of the frame's string
/// buffer, returning where it starts; `***` when it does not fit or fails. The variadic
/// arguments are read as the slots they are passed in: two in registers, the rest from the
/// caller's stack, of which the format uses as many as it names.
#[allow(clippy::too_many_arguments)]
extern "win64" fn sprintf(inst: *mut u8, fmt: *const c_char, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64, a6: u64, a7: u64, a8: u64, a9: u64, a10: u64, a11: u64) -> *const u8 {
    let f = frame(inst);
    let room = super::STRING_BUFFER - f.strings_used;
    if room <= 1 || fmt.is_null() {
        return FAILED.as_ptr();
    }
    // SAFETY: the element passes a NUL-terminated format.
    let fmt = unsafe { CStr::from_ptr(fmt) }.to_bytes();
    let Some(s) = super::printf::format(fmt, &[a0, a1, a2, a3, a4, a5, a6, a7, a8, a9, a10, a11]) else { return FAILED.as_ptr() };
    if s.len() >= room {
        return FAILED.as_ptr();
    }
    let at = f.strings_used;
    f.strings[at..at + s.len()].copy_from_slice(&s);
    f.strings[at + s.len()] = 0;
    f.strings_used += s.len() + 1;
    f.strings[at..].as_ptr()
}

/// `+0x48` (`0x2fb6e0`): the localized text of `token` with `%s1` to `%s5` replaced by the
/// arguments (`0x2fb4e0`), into what is left of the frame's string buffer, returning where it
/// starts; the token itself when it has no text, `***` when it does not fit. An empty token
/// comes back as it is.
#[allow(clippy::too_many_arguments)]
extern "win64" fn format(inst: *mut u8, token: *const c_char, a0: *const c_char, a1: *const c_char, a2: *const c_char, a3: *const c_char, a4: *const c_char) -> *const u8 {
    if token.is_null() {
        return EMPTY_STRING.as_ptr();
    }
    // SAFETY: the element passes a NUL-terminated token.
    let name = unsafe { CStr::from_ptr(token) }.to_bytes();
    if name.is_empty() {
        return token as *const u8;
    }
    let f = frame(inst);
    let size = super::STRING_BUFFER - f.strings_used;
    // SAFETY: the runtime outlives the frame.
    let ui = unsafe { &*header(inst).ui };
    let out = match ui.localized(name) {
        Some(text) => substitute(text, &[a0, a1, a2, a3, a4], size),
        None => (name.len() + 1 < size).then(|| name.to_vec()),
    };
    let Some(out) = out else { return FAILED.as_ptr() };
    let at = f.strings_used;
    f.strings[at..at + out.len()].copy_from_slice(&out);
    f.strings[at + out.len()] = 0;
    f.strings_used += out.len() + 1;
    f.strings[at..].as_ptr()
}

/// `0x2fb4e0`'s copy of a localized text into UTF-8: a surrogate takes the next unit with it
/// unchecked, `%s1` to `%s9` naming an argument there is becomes it, and each other code point
/// needs room for four bytes. None when it does not fit in `size` with its terminator.
fn substitute(text: &[u16], args: &[*const c_char], size: usize) -> Option<Vec<u8>> {
    let unit = |i: usize| text.get(i).copied().unwrap_or(0) as u32;
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        let mut c = unit(i);
        i += 1;
        if c.wrapping_sub(0xd800) < 0x800 {
            c = (c * 0x400 + unit(i)).wrapping_sub(0x35f_dc00);
            i += 1;
        }
        if c == 0 {
            return Some(out);
        }
        let n = unit(i + 1).wrapping_sub(b'1' as u32) as u16 as usize;
        if c == b'%' as u32 && unit(i) == b's' as u32 && n <= 8 && n < args.len() {
            i += 2;
            if !args[n].is_null() {
                // SAFETY: the element passes NUL-terminated strings.
                for &b in unsafe { CStr::from_ptr(args[n]) }.to_bytes() {
                    if size <= out.len() + 1 {
                        return None;
                    }
                    out.push(b);
                }
            }
            continue;
        }
        if size <= out.len() + 4 {
            return None;
        }
        let tail = |shift: u32| (c >> shift) as u8 & 0x3f | 0x80;
        match c {
            0..0x80 => out.push(c as u8),
            0x80..0x800 => out.extend([(c >> 6) as u8 | 0xc0, tail(0)]),
            0x800..0x10000 => out.extend([(c >> 12) as u8 | 0xe0, tail(6), tail(0)]),
            _ => out.extend([(c >> 18) as u8 | 0xf0, tail(12), tail(6), tail(0)]),
        }
    }
}

/// `+0x50` (`0x2fb7b0`): `s` upper-cased into what is left of the frame's string buffer, or
/// `s` itself when nothing changes or it does not fit. Each code point (`0x437dd0`) goes
/// through the CRT's `towupper` (`0x11c33e8`) as 16 bits, in the C locale: `a` to `z` only. A
/// sequence the decoder refuses stops nothing but advances nothing, so the buffer fills and
/// `s` comes back.
extern "win64" fn upper(inst: *mut u8, s: *const u8) -> *const u8 {
    // SAFETY: the element passes a NUL-terminated string.
    let bytes = unsafe { CStr::from_ptr(s as *const c_char) }.to_bytes();
    if bytes.is_empty() {
        return s;
    }
    let f = frame(inst);
    let start = f.strings_used;
    let (mut at, mut i, mut changed) = (start, 0, false);
    loop {
        if 0x2003 < at {
            return s;
        }
        let (c, next) = decode(bytes, i);
        i = next;
        let u = c as u16;
        let up = if (b'a' as u16..=b'z' as u16).contains(&u) { u - 0x20 } else { u };
        changed |= up as u32 != c;
        let tail = |shift: u32| (up >> shift) as u8 & 0x3f | 0x80;
        let n = match up {
            0..0x80 => {
                f.strings[at] = up as u8;
                1
            }
            0x80..0x800 => {
                f.strings[at..at + 2].copy_from_slice(&[(up >> 6) as u8 | 0xc0, tail(0)]);
                2
            }
            _ => {
                f.strings[at..at + 3].copy_from_slice(&[(up >> 12) as u8 | 0xe0, tail(6), tail(0)]);
                3
            }
        };
        at += n;
        if bytes.get(i).is_none_or(|&b| b == 0) {
            break;
        }
    }
    if !changed {
        return s;
    }
    f.strings[at] = 0;
    f.strings_used = at + 1;
    f.strings[start..].as_ptr()
}

/// `0x437dd0`, and the caller's own step over an ASCII byte: the code point at `i` and where the
/// next starts. A sequence it refuses is 0, and `i` stays.
fn decode(b: &[u8], i: usize) -> (u32, usize) {
    let at = |k: usize| b.get(i + k).copied().unwrap_or(0) as u32;
    let lead = at(0);
    if lead < 0x80 {
        return (lead, i + 1);
    }
    let cont = |k: usize| at(k) & 0xc0 == 0x80;
    if !cont(1) {
        return (0, i);
    }
    if lead < 0xe0 {
        return if 0xc1 < lead { ((lead & 0x3f) << 6 | at(1) & 0x3f, i + 2) } else { (0, i) };
    }
    if !cont(2) {
        return (0, i);
    }
    let c = ((lead & 0xf) << 6 | at(1) & 0x3f) << 6 | at(2) & 0x3f;
    if lead < 0xf0 {
        return if 0x7ff < c.wrapping_sub(0xd800) { (c, i + 3) } else { (0, i) };
    }
    let c = c << 6 | at(3) & 0x3f;
    if cont(3) && c < 0x11_0000 { (c, i + 4) } else { (0, i) }
}

/// `+0xc8` (`0x2f3430`): the font table's slot for a name, in the low 16 bits of the name's
/// hash (`0x46cd80`), whose upper 48 bits come back above it.
extern "win64" fn font(name: *const c_char) -> u64 {
    let name = if name.is_null() { Default::default() } else {
        // SAFETY: the element passes a NUL-terminated string.
        unsafe { CStr::from_ptr(name) }.to_string_lossy()
    };
    (crate::assets::rpak::guid(&name) & !0xffff) | crate::assets::rui::font_slot(&name) as u64
}

// The engine returns the point as an `__m128` in xmm0, as LLVM does for the Windows x64
// convention.
#[allow(improper_ctypes_definitions)]
/// `+0x78` (`0x2fbcf0`): the point at `p` (its fourth lane read as one) through the first three
/// rows of the matrix at `m` (the draw group's, the globals' first `0x40` bytes), then from the
/// matrix's eye (`m[12..15]`) onto the instance's topology (`0x1345860`; a plane here).
extern "win64" fn project(inst: *mut u8, m: *const f32, p: *const f32) -> __m128 {
    // SAFETY: the element passes the globals and a point of four floats of its own.
    let (m, p): ([f32; 16], [f32; 3]) = unsafe { (std::array::from_fn(|i| m.add(i).read_unaligned()), std::array::from_fn(|i| p.add(i).read_unaligned())) };
    let uv = project_point(&m, p, &frame(inst).topology);
    m128([uv[0], uv[1], 0.0, 0.0])
}

/// What `+0x78` computes: `p` through the matrix's rows, then from its eye onto `topology`.
pub fn project_point(m: &[f32; 16], p: [f32; 3], topology: &super::Topology) -> [f32; 2] {
    let row = |r: &[f32]| (p[2] * r[2] + r[3]) + (p[0] * r[0] + p[1] * r[1]);
    let point = [row(&m[0..4]), row(&m[4..8]), row(&m[8..12])];
    topology.project([m[12], m[13], m[14]], point)
}

/// `+0x90` (`0x2fc420`): the image table index of a name.
extern "win64" fn image(inst: *mut u8, name: *const c_char) -> i32 {
    if name.is_null() {
        return -1;
    }
    // SAFETY: the element passes a NUL-terminated string.
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
    // SAFETY: the runtime outlives the frame.
    unsafe { &*header(inst).ui }.image(&name)
}

/// `+0xa0` (`0x2fc1b0`): mapping `index` at `x`, one output.
extern "win64" fn map1(inst: *mut u8, index: i32, x: f32) -> f32 {
    mapping::<1>(inst, index, x)[0]
}

// The engine returns these as an `__m128` in xmm0, as LLVM does for the Windows x64
// convention.
#[allow(improper_ctypes_definitions)]
/// `+0xa8` (`0x2fc220`): two outputs.
extern "win64" fn map2(inst: *mut u8, index: i32, x: f32) -> __m128 {
    m128(mapping::<2>(inst, index, x))
}

#[allow(improper_ctypes_definitions)]
/// `+0xb0` (`0x2fc2c0`): three outputs.
extern "win64" fn map3(inst: *mut u8, index: i32, x: f32) -> __m128 {
    m128(mapping::<3>(inst, index, x))
}

#[allow(improper_ctypes_definitions)]
/// `+0xb8` (`0x2fc380`): four outputs.
extern "win64" fn map4(inst: *mut u8, index: i32, x: f32) -> __m128 {
    m128(mapping::<4>(inst, index, x))
}

/// The first `N` outputs of mapping `index` at `x`, the rest zero: a line's
/// `w1 · q1 + w0 · q0`, a cubic's `((w0 · v0 + w2 · s0) + w1 · v1) + w3 · s1` over each key's
/// `N` values then `N` slopes. The callbacks differ only in operand order, which IEEE addition
/// does not see.
fn mapping<const N: usize>(inst: *mut u8, index: i32, x: f32) -> V4 {
    // SAFETY: the element outlives the frame.
    let element = unsafe { &*header(inst).element };
    let Some(m) = element.mappings.get(index as usize) else { return [0.0; 4] };
    let (cubic, at, w) = lookup(m, x);
    let p = |k: usize| m.data.get(at + k).copied().unwrap_or(0.0);
    std::array::from_fn(|i| {
        if i >= N {
            return 0.0;
        }
        if cubic {
            ((w[0] * p(i) + w[2] * p(N + i)) + w[1] * p(2 * N + i)) + w[3] * p(3 * N + i)
        } else {
            w[1] * p(N + i) + w[0] * p(i)
        }
    })
}

/// The segment of a mapping that holds `x` (`0x2fc060`): whether it is cubic, where its
/// outputs start in the mapping's data, and the weights: `1 - t, t` for a line, the Hermite
/// basis `h00, h01, h10, h11` (the slopes scaled by the key gap) for a cubic.
pub fn lookup(m: &crate::assets::rui::Mapping, x: f32) -> (bool, usize, [f32; 4]) {
    let keys = &m.data[..m.keys as usize];
    let n = m.keys as usize;
    let (mut seg, t, d);
    if keys[0] >= x {
        (seg, t, d) = (0usize, 0.0f32, 0.0f32);
    } else if x >= keys[n - 1] {
        (seg, t, d) = (n - 2, 1.0, 1.0);
    } else {
        seg = 0;
        let mut lo = keys[0];
        let mut hi = keys[1];
        let mut i = 2;
        while x > hi {
            lo = hi;
            seg += 1;
            hi = keys[i];
            i += 1;
        }
        let gap = hi - lo;
        (t, d) = ((x - lo) / gap, gap);
    }
    let base = m.stride as usize * seg;
    if m.cubic {
        let two_t = t + t;
        let tm1 = t - 1.0;
        let tt = t * t;
        let a = two_t + 1.0;
        let sq = tm1 * tm1;
        let b = 3.0 - two_t;
        (true, base * 2 + n, [a * sq, b * tt, sq * d * t, tt * d * tm1])
    } else {
        (false, base + n, [1.0 - t, t, 0.0, 0.0])
    }
}

// The engine returns the colour as an `__m128` in xmm0, which is how LLVM returns it for the
// Windows x64 convention too.
#[allow(improper_ctypes_definitions)]
/// `+0xd0` (`0x2fc430`): palette colour `a + b` (each below zero read as zero), linear, alpha
/// zero; the table's first colour for an id past its end.
extern "win64" fn color(inst: *mut u8, a: i32, b: i32) -> core::arch::x86_64::__m128 {
    // SAFETY: the runtime outlives the frame.
    let ui = unsafe { &*header(inst).ui };
    let id = a.max(0) as u32 + b.max(0) as u32;
    let c = ui.colors.get(id as usize).or(ui.colors.first()).copied().unwrap_or_else(|| srgb_to_linear([0.0; 4]));
    // SAFETY: SSE is part of every x86-64 CPU.
    unsafe { core::arch::x86_64::_mm_set_ps(c[3], c[2], c[1], c[0]) }
}

fn m128(v: V4) -> __m128 {
    // SAFETY: SSE is part of every x86-64 CPU.
    unsafe { core::arch::x86_64::_mm_set_ps(v[3], v[2], v[1], v[0]) }
}

/// `0x270130`: sRGB to linear, lane by lane: below 0.04045 a straight scale, else
/// `((x + 0.055) / 1.055)^2.4` from a bit-trick guess at the fifth root and three Newton
/// steps, then cubed. Lane 3 (alpha) is scaled by one.
pub fn srgb_to_linear(x: V4) -> V4 {
    const SEED: [u32; 2] = [0x3333_3333, 0x4c2a_6698];
    std::array::from_fn(|i| {
        let a = (x[i] + 0.055) * 0.947_867_3;
        let mut y = f32::from_bits(SEED[1].wrapping_sub(((SEED[0] as u64 * a.to_bits() as u64) >> 32) as u32));
        for _ in 0..3 {
            let y4 = (y * y) * (y * y);
            let e = y4 * (y * a);
            y += (1.0 - e) * (0.2 * y);
        }
        let r = y * a;
        let threshold = if i == 3 { f32::MAX } else { 0.040_45 };
        let scale = if i == 3 { 1.0 } else { 0.077_399_38 };
        if x[i] < threshold { x[i] * scale } else { r * r * r }
    })
}

thread_local! {
    /// The frame of the update running on this thread, for the callbacks that are not handed
    /// the instance.
    pub(super) static CURRENT: std::cell::Cell<*mut Frame> = const { std::cell::Cell::new(std::ptr::null_mut()) };
}

/// Marks the running frame stopped at a callback not ported yet.
fn stop_at(slot: usize) {
    let f = CURRENT.with(|c| c.get());
    if !f.is_null() {
        // SAFETY: set by `Ui::update` for the duration of the element's call on this thread.
        let f = unsafe { &mut *f };
        f.stop.get_or_insert(Stop::Api(slot as u8));
    }
}

// Each takes no arguments it reads, so any signature's arguments are left alone, and returns a
// value harmless whatever the caller reads: zero, or the empty string where a string is due.
macro_rules! unported {
    ($($name:ident = $slot:expr => $ret:ty, $val:expr;)*) => {$(
        extern "win64" fn $name() -> $ret {
            stop_at($slot);
            $val
        }
    )*};
}

unported! {
    number = 0x58 => *const u8, EMPTY_STRING.as_ptr();
    random = 0x70 => f32, 0.0;
    aspect = 0x80 => u64, 0;
    nested_scale = 0x88 => u64, 0;
    glyph = 0x98 => *const u8, EMPTY_STRING.as_ptr();
    nested_size = 0xc0 => u64, 0;
    screen_scale = 0xd8 => f32, 0.0;
}

// The engine returns the colour as an `__m128` in xmm0, as LLVM does for the Windows x64
// convention.
#[allow(improper_ctypes_definitions)]
/// `+0x60` (`0x270130`): sRGB to linear of the four floats at `p`.
extern "win64" fn linear(p: *const f32) -> core::arch::x86_64::__m128 {
    // SAFETY: the element passes a pointer to four floats of its own.
    let v: V4 = unsafe { std::array::from_fn(|i| p.add(i).read_unaligned()) };
    let c = srgb_to_linear(v);
    // SAFETY: SSE is part of every x86-64 CPU.
    unsafe { core::arch::x86_64::_mm_set_ps(c[3], c[2], c[1], c[0]) }
}

/// `+0x68` (`0x2fbbc0`): does nothing.
extern "win64" fn nothing(_inst: *mut u8) {}

/// The table as the element functions index it, by byte offset / 8.
pub fn table() -> [usize; SLOTS] {
    [
        hide as *const () as usize,
        hide_now as *const () as usize,
        error as *const () as usize,
        sizes as *const () as usize,
        text as *const () as usize,
        fit as *const () as usize,
        run as *const () as usize,
        run_unit as *const () as usize,
        sprintf as *const () as usize,
        format as *const () as usize,
        upper as *const () as usize,
        number as *const () as usize,
        linear as *const () as usize,
        nothing as *const () as usize,
        random as *const () as usize,
        project as *const () as usize,
        aspect as *const () as usize,
        nested_scale as *const () as usize,
        image as *const () as usize,
        glyph as *const () as usize,
        map1 as *const () as usize,
        map2 as *const () as usize,
        map3 as *const () as usize,
        map4 as *const () as usize,
        nested_size as *const () as usize,
        font as *const () as usize,
        color as *const () as usize,
        screen_scale as *const () as usize,
    ]
}

#[cfg(test)]
mod tests {
    use super::{decode, project_point, substitute};
    use crate::rui::{Globals, Topology};

    #[test]
    fn utf8_decodes_as_the_engine_decodes_it() {
        assert_eq!(decode(b"a", 0), (0x61, 1));
        assert_eq!(decode("\u{e9}".as_bytes(), 0), (0xe9, 2));
        assert_eq!(decode("\u{20ac}".as_bytes(), 0), (0x20ac, 3));
        assert_eq!(decode("\u{1f600}".as_bytes(), 0), (0x1f600, 4));
        assert_eq!(decode(&[0xc1, 0x81], 0), (0, 0), "an overlong lead");
        assert_eq!(decode(&[0xed, 0xa0, 0x80], 0), (0, 0), "a surrogate");
        assert_eq!(decode(&[0xe2, 0x41], 0), (0, 0));
    }

    #[test]
    fn a_point_projects_through_the_view_onto_the_screen() {
        let screen = Topology { org: [0.0; 3], right: [1920.0, 0.0, 0.0], down: [0.0, 1080.0, 0.0] };
        let mut g = Globals::new(0.0, 1920.0, 1080.0);
        // The eye at the origin looking along +x: right is -y, up +z, back -x; 90 degrees across.
        g.set_view([[0.0, -1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [-1.0, 0.0, 0.0, 0.0]], 1.0);
        let m: [f32; 16] = std::array::from_fn(|i| g.f32(4 * i));
        assert_eq!(project_point(&m, [100.0, 0.0, 0.0], &screen), [0.5, 0.5]);
        assert_eq!(project_point(&m, [100.0, -50.0, 25.0], &screen), [0.75, 0.5 - 240.0 / 1080.0]);
        let behind = project_point(&m, [-100.0, -50.0, 0.0], &screen);
        assert!((behind[0].hypot(behind[1]) - 4096.0).abs() < 8.0, "{behind:?}");
    }

    #[test]
    fn localized_text_takes_its_arguments_and_needs_room() {
        let text: Vec<u16> = "%s1 sec, %s6%, é😀".encode_utf16().collect();
        let args = [c"12".as_ptr()];
        assert_eq!(substitute(&text, &args, 64).unwrap(), "12 sec, %s6%, é😀".as_bytes());
        assert_eq!(substitute(&text, &args, 20), None);
    }
}
