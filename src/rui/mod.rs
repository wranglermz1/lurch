//! The engine's RUI runtime: instances of the install's RUI elements (`crate::assets::rui`),
//! their compiled functions from `ui.dll` (`code`) run each frame against the callbacks the
//! engine gives them (`api`), their transform code (`transform`), and what they draw. Built
//! from the engine's own update (`Rui_Draw`, `0x2fc670`); recovered behaviour is in
//! `docs/ui.md`.

pub mod api;
pub mod code;
pub mod draw;
pub mod printf;
pub mod text;
pub mod tokens;
pub mod transform;

use std::collections::HashMap;
use std::io;
use std::path::Path;

use crate::assets::rui::{self as rasset, Element, Library};
use code::{Code, ElementFn};
use transform::{diagonal, Stop, Transform, V4};

/// Script draw groups (`0x77e284` onwards).
pub const RUI_DRAW_WORLD: u8 = 2;
pub const RUI_DRAW_VIEW_MODEL: u8 = 3;
pub const RUI_DRAW_COCKPIT: u8 = 5;
pub const RUI_DRAW_HUD: u8 = 6;
pub const RUI_DRAW_POSTEFFECTS: u8 = 7;
/// Draw groups inset by the safe area (`0x2fc705`).
const SAFE_AREA_GROUPS: u32 = 0xe40;
/// ConVar `rui_safeAreaFrac` (image value), scaled by 0.05 (`0x15bb3c0`) at `0x2fc725`.
pub const RUI_SAFE_AREA_FRAC: f32 = 0.0;
const SAFE_AREA_SCALE: f32 = 0.05;
/// The frame's transform slots: sizes at `+0x3e90` and transforms at `+0x4b40` of the
/// engine's draw scratch, up to the field at `+0x64a0`.
pub const MAX_TRANSFORMS: usize = 203;
/// The scratch string buffer at `+0x159c`, `0x2008` bytes (`0x2fb460`).
pub const STRING_BUFFER: usize = 0x2008;

/// The values every element of a draw group reads each frame (`0x720100`): `0x100` bytes,
/// copied from the template at `0xc05f400`, then the time (`+0x98`), the viewport (`+0x48`,
/// `+0x4c`) and the matrix from screen pixels to clip space (rows at `+0x50`, `+0x60`, `+0x70`,
/// `+0x80`) filled in. The template's clock (`+0x90`) is set once a frame (`0x22ab50`).
#[derive(Clone)]
#[repr(C, align(16))]
pub struct Globals(pub [u8; 0x100]);

impl Globals {
    /// The template as the image holds it in a match (its clock stopped at `0x4_32be_4adb`,
    /// `+0xa8` = 1), with `time` and a `width` by `height` viewport.
    pub fn new(time: f32, width: f32, height: f32) -> Globals {
        let mut g = Globals([0; 0x100]);
        g.set_clock(0x4_32be_4adb);
        g.set_u32(0xa8, 1);
        for (i, v) in [0.095_310_13, 0.309_470_1, 0.708_376_6, 1.0, 0.187_821_25, 0.014_443_844].into_iter().enumerate() {
            g.set_f32(0xc8 + 4 * i, v);
        }
        for (i, v) in [0.095_310_13, 0.309_470_1, 0.708_376_6, -60.0].into_iter().enumerate() {
            g.set_f32(0xe0 + 4 * i, v);
        }
        g.set_f32(0x00, 1.0);
        g.set_f32(0x10, 0.007_812_5);
        g.set_f32(0x48, width);
        g.set_f32(0x4c, height);
        let (sx, sy) = (1.0 / (0.0 - width), 1.0 / (height - 0.0));
        for (at, v) in [(0x50, sx * -2.0), (0x5c, (width + 0.0) * sx), (0x64, sy * -2.0), (0x6c, (height + 0.0) * sy), (0x78, 0.5), (0x7c, 0.5), (0x8c, 1.0)] {
            g.set_f32(at, v);
        }
        g.set_f32(0x98, time);
        g
    }

    /// The HUD draw groups' view (`0x768a45`): from the view's world-to-view rows (right, up
    /// and back, each with its translation) and the tangent of half the horizontal field of
    /// view, the rows that take a world point into the screen plane's space and the eye there,
    /// which `+0x78` projects through.
    pub fn set_view(&mut self, view: [[f32; 4]; 3], tan_half_fov: f32) {
        let (w, h) = (self.f32(0x48), self.f32(0x4c));
        let d = (w * -0.5) / if tan_half_fov > 1.0e-4 { tan_half_fov } else { 1.0e-4 };
        let add = [[0.0, 0.0, 0.0, w * 0.5], [0.0, 0.0, 0.0, h * 0.5], [0.0, 0.0, 0.0, d]];
        for (r, row) in view.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                self.set_f32(16 * r + 4 * c, if r == 0 { v + add[r][c] } else { add[r][c] - v });
            }
        }
        for (i, v) in [w * 0.5, h * 0.5, d].into_iter().enumerate() {
            self.set_f32(0x30 + 4 * i, v);
        }
    }

    /// The clock (`+0x90`): the performance counter in microseconds (`0x2f1c90`). Elements time
    /// their own transitions by it, `generic_button`'s state fades among them.
    pub fn set_clock(&mut self, micros: u64) {
        self.0[0x90..0x98].copy_from_slice(&micros.to_le_bytes());
    }

    pub fn set_f32(&mut self, at: usize, v: f32) {
        self.0[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    pub fn set_u32(&mut self, at: usize, v: u32) {
        self.0[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    pub fn f32(&self, at: usize) -> f32 {
        f32::from_le_bytes(self.0[at..at + 4].try_into().unwrap())
    }

    pub fn time(&self) -> f32 {
        self.f32(0x98)
    }
}

/// Where an instance's unit square lands: a plane in screen pixels, as
/// `RuiTopology_CreatePlane(org, right, down)` makes one (`cl_mapspawn.gnut` builds the HUD's).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Topology {
    pub org: [f32; 3],
    pub right: [f32; 3],
    pub down: [f32; 3],
}

impl Topology {
    /// The full-screen HUD plane `BuildTopologyCreateData` makes for a `width` by `height`
    /// screen: the virtual screen (1920 by 1080 at 16:9, wider at a wider aspect) scaled to the
    /// screen's height and centred.
    pub fn hud(width: f32, height: f32) -> Topology {
        let [vw, vh] = virtual_screen(width, height);
        let scale = height / vh;
        let (sw, sh) = ((vw * scale).trunc(), (vh * scale).trunc());
        Topology { org: [(width - sw) / 2.0, 0.0, 0.0], right: [sw, 0.0, 0.0], down: [0.0, sh, 0.0] }
    }

    /// Where a point of the unit square lands.
    pub fn place(&self, u: f32, v: f32) -> [f32; 3] {
        std::array::from_fn(|k| self.org[k] + u * self.right[k] + v * self.down[k])
    }

    /// Where the line from `eye` through `p` meets the plane, in the plane's units (`0x2fffd0`).
    /// When it meets it past the eye: that point, brought under 4096 across by a power of two.
    /// Otherwise: 4096 out from the plane's centre towards `p`, or `(0, 4096)` when `p` lies
    /// over the centre.
    pub fn project(&self, eye: [f32; 3], p: [f32; 3]) -> [f32; 2] {
        let (r, d, q) = (self.right, self.down, self.org);
        let dot = |a: [f32; 3], b: [f32; 3]| (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2];
        let cross = |a: [f32; 3], b: [f32; 3]| -> [f32; 3] { std::array::from_fn(|i| b[(i + 2) % 3] * a[(i + 1) % 3] - b[(i + 1) % 3] * a[(i + 2) % 3]) };
        let n = cross(r, d);
        let nn = dot(n, n);
        let to_p: [f32; 3] = std::array::from_fn(|k| p[k] - eye[k]);
        let to_q: [f32; 3] = std::array::from_fn(|k| q[k] - eye[k]);
        let along = dot(n, to_p);
        let sign = along.to_bits() & 0x8000_0000;
        let size = f32::from_bits(along.to_bits() & 0x7fff_ffff);
        let ahead = f32::from_bits(dot(n, to_q).to_bits() ^ sign);
        let in_plane = |x: [f32; 3]| {
            let c = cross(x, n).map(|v| v / nn);
            let sum = |a: [f32; 3]| (c[2] * a[2] + c[0] * a[0]) + c[1] * a[1];
            [-sum(d), sum(r)]
        };
        if 0.0 < if ahead < size { ahead } else { size } {
            let t = ahead / size;
            let uv = in_plane(std::array::from_fn(|k| t * to_p[k] - to_q[k]));
            let scale = |x: f32| f32::from_bits(0x8500_0000u32.wrapping_sub(x.to_bits() & 0x7f80_0000));
            let s = [scale(uv[0]), scale(uv[1])].into_iter().fold(1.0f32, |m, x| if m < x { m } else { x });
            return [s * uv[0], s * uv[1]];
        }
        let uv = in_plane(std::array::from_fn(|k| (p[k] - q[k]) - (d[k] + r[k]) * 0.5));
        let sq = [uv[0] * uv[0], uv[1] * uv[1]];
        if !(f32::EPSILON < sq[0] || f32::EPSILON < sq[1]) {
            return [0.0, OFF_PLANE];
        }
        // The engine's RSQRTPS: an estimate the CPU gives, kept rather than a true root.
        // SAFETY: SSE is part of every x86-64 CPU.
        let inv = unsafe {
            use core::arch::x86_64::*;
            _mm_cvtss_f32(_mm_rsqrt_ps(_mm_set1_ps(sq[1] + sq[0])))
        };
        [inv * (OFF_PLANE * uv[0]), inv * (OFF_PLANE * uv[1])]
    }
}

/// How far out of the plane `Topology::project` puts a point it cannot reach (`0x15bc230`).
const OFF_PLANE: f32 = 4096.0;

/// The HUD's virtual screen for a `width` by `height` screen (`GetCurrentVirtualScreenSize`):
/// 1920 by 1080 at 16:9, the side across the aspect widened to it.
pub fn virtual_screen(width: f32, height: f32) -> [f32; 2] {
    let aspect = width / height;
    if aspect < 16.0 / 9.0 { [1920.0, (1920.0 / aspect).trunc()] } else { [(1080.0 * aspect).trunc(), 1080.0] }
}

/// One entry of the engine's image table (`0xc03f298`): an image of an atlas, by the hash of
/// its name (`0x2f39c0` registers them; `0x2fc420` looks them up).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Image {
    pub hash: u32,
    pub atlas: u8,
    pub index: u16,
    pub flags: u8,
}

/// The RUI runtime: the install's elements, atlases and palette, and `ui.dll`.
pub struct Ui {
    pub lib: Library,
    pub code: Code,
    pub images: Vec<Image>,
    by_hash: HashMap<u32, u32>,
    /// Each palette colour as `0x2fc430` returns it: `/255`, then sRGB to linear (`0x270130`).
    pub colors: Vec<V4>,
    /// The font table (`0xc05f2d0`, its atlases at `0xc05f3d0`): by slot, the font atlas and
    /// the face there, a later atlas's face replacing an earlier one's (`0x2f3870`).
    pub fonts: [Option<(u8, u16)>; 32],
    /// The localized texts `+0x48` formats (`crate::assets::localize`).
    pub localize: crate::assets::localize::Table,
    /// What `%command%` in text reads: the key bound to it.
    pub tokens: tokens::Tokens,
    /// The elements text embeds by name (`%@name%`), in registry order.
    pub embedded: Vec<Embedded>,
}

/// An element registered for text to embed (`RuiCreateEmbedded`, the registry at
/// `0xd4152c0`): its name in `%@name%`, its element, the strings its arguments are set to, and
/// its size, the element's own.
pub struct Embedded {
    pub name: String,
    pub element: String,
    pub strings: Vec<(String, std::ffi::CString)>,
    pub size: [f32; 2],
}

impl Ui {
    pub fn open(install: &Path) -> io::Result<Ui> {
        let lib = rasset::library(install)?;
        let code = Code::open(install)?;
        let mut ui = Ui::new(lib, code);
        ui.localize = crate::assets::localize::load(install, "english");
        ui.embedded = rasset::embedded_text(install)
            .into_iter()
            .filter_map(|(element, name, text)| {
                let size = ui.lib.elements.get(&element)?.size;
                let strings = text.and_then(|t| std::ffi::CString::new(t).ok()).map(|t| ("displayText".to_string(), t)).into_iter().collect();
                Some(Embedded { name, element, strings, size })
            })
            .collect();
        Ok(ui)
    }

    pub fn new(lib: Library, code: Code) -> Ui {
        let mut images: Vec<Image> = Vec::new();
        let mut by_hash = HashMap::new();
        for (a, atlas) in lib.atlases.iter().enumerate() {
            for (i, &(hash, flags)) in atlas.names.iter().enumerate() {
                let img = Image { hash, atlas: a as u8, index: i as u16, flags: flags[0] };
                match by_hash.get(&hash) {
                    Some(&at) => images[at as usize] = img,
                    None => {
                        by_hash.insert(hash, images.len() as u32);
                        images.push(img);
                    }
                }
            }
        }
        let colors = lib.palette.iter().map(|(_, c)| api::srgb_to_linear([c[0] * (1.0 / 255.0), c[1] * (1.0 / 255.0), c[2] * (1.0 / 255.0), 0.0])).collect();
        let mut fonts = [None; 32];
        for (a, atlas) in lib.fonts.iter().enumerate() {
            for (i, face) in atlas.faces.iter().enumerate() {
                fonts[face.slot as usize & 31] = Some((a as u8, i as u16));
            }
        }
        Ui { lib, code, images, by_hash, colors, fonts, localize: Default::default(), tokens: Default::default(), embedded: Vec::new() }
    }

    /// The registry index of the embedded element `name` (`0x722970`).
    pub fn embedded(&self, name: &str) -> Option<usize> {
        self.embedded.iter().position(|e| e.name == name)
    }

    /// The localized text of a token, found without its leading `#` (`0x3a57c0`).
    pub fn localized(&self, token: &[u8]) -> Option<&[u16]> {
        let name = token.strip_prefix(b"#").unwrap_or(token);
        self.localize.get(&String::from_utf8_lossy(name).to_ascii_lowercase()).map(Vec::as_slice)
    }

    /// The font atlas and face in a slot of the font table.
    pub fn font(&self, slot: u16) -> Option<(&rasset::FontAtlas, &rasset::Face)> {
        let (a, f) = (*self.fonts.get(slot as usize)?)?;
        let atlas = &self.lib.fonts[a as usize];
        Some((atlas, &atlas.faces[f as usize]))
    }

    /// The image table index of `name`, or of `"missing"` when it has none, or -1 (`0x2fc420`).
    pub fn image(&self, name: &str) -> i32 {
        if name.is_empty() {
            return -1;
        }
        if let Some(&i) = self.by_hash.get(&rasset::image_hash(name)) {
            return i as i32;
        }
        if name.starts_with("missing") {
            return -1;
        }
        self.by_hash.get(&rasset::image_hash("missing")).map_or(-1, |&i| i as i32)
    }

    /// A new instance of the element `name` in draw group `group`, its data block filled from
    /// the defaults (`0x720fc0`, `0x2fc550`).
    pub fn create(&self, name: &str, group: u8, sort_key: i16) -> Option<Rui> {
        let element = self.lib.elements.get(name)?;
        let func = self.code.function(name)?;
        let mut data = vec![0u8; element.data_size as usize];
        data[..element.defaults.len()].copy_from_slice(&element.defaults);
        let mut rui = Rui {
            name: name.to_string(),
            func,
            group,
            sort_key,
            size: element.size,
            inv_size: element.inv_size,
            seed: 0,
            visible: true,
            no_safe_area: false,
            bbox: [0.0; 4],
            data: Data::new(element.data_size as usize),
            texts: HashMap::new(),
        };
        rui.data.bytes_mut().copy_from_slice(&data);
        for (at, s) in &element.strings {
            rui.data.write(*at as usize, &(s.as_ptr() as u64).to_le_bytes());
        }
        for i in 0..element.args.len() / rasset::ARG_SIZE {
            let a = element.arg(i);
            if matches!(a.kind, 1 | 2 | 12 | 13) {
                rui.data.write(a.offset as usize, &(EMPTY_STRING.as_ptr() as u64).to_le_bytes());
            }
        }
        Some(rui)
    }

    /// Runs one instance for a frame, as `Rui_Draw` (`0x2fc670`) does, and returns its
    /// transforms when it is to be drawn.
    pub fn update(&self, rui: &mut Rui, globals: &Globals, topology: &Topology, frame: &mut Frame) -> Result<bool, Stop> {
        let element = self.lib.elements.get(&rui.name).ok_or(Stop::Missing)?;
        let rect = [element.size[0], element.size[0], element.size[1], element.size[1]];
        self.update_at(rui, globals, topology, frame, rect, Transform { basis: [1.0, 0.0, 0.0, 1.0], origin: [0.0; 4] })
    }

    /// `Rui_Draw` with the box the caller places the instance in: its size `rect` and the
    /// transform of its unit square, which an element embedded in text gets from the text
    /// (`0x2f86cf`). Transforms 1 and 2 stay relative to the unit square.
    pub fn update_at(&self, rui: &mut Rui, globals: &Globals, topology: &Topology, frame: &mut Frame, rect: V4, place: Transform) -> Result<bool, Stop> {
        let element = self.lib.elements.get(&rui.name).ok_or(Stop::Missing)?;
        frame.reset(element);
        frame.ui = self;
        frame.data = rui.data.as_mut_ptr();
        frame.seeds = seeds(globals.time(), rui.seed);
        frame.sizes[0] = rect;
        frame.transforms[0] = place;
        if (1u32 << (rui.group & 31)) & SAFE_AREA_GROUPS != 0 && !rui.no_safe_area {
            let f = RUI_SAFE_AREA_FRAC * SAFE_AREA_SCALE;
            let s = 1.0 - (f + f);
            frame.sizes[1] = rect.map(|x| x * s);
            frame.transforms[1] = Transform { basis: diagonal([s; 4]), origin: [f; 4] };
        } else {
            frame.sizes[1] = rect;
            frame.transforms[1] = frame.transforms[0];
        }
        frame.unit = [rui.size[0], rui.size[1], rui.inv_size[0], rui.inv_size[1]];
        let (a, b) = (rect[0] * rui.inv_size[0], rect[2] * rui.inv_size[1]);
        let (rx, ry) = if a > b { (1.0, b / a) } else { (a / b, 1.0) };
        frame.transforms[2] = Transform { basis: [rx, 0.0, 0.0, ry], origin: [0.5 - rx * 0.5, 0.5 - ry * 0.5, 0.5 - rx * 0.5, 0.5 - ry * 0.5] };
        frame.sizes[2] = [rect[0] * rx, rect[1] * rx, rect[2] * ry, rect[3] * ry];
        frame.topology = *topology;
        frame.globals = globals.clone();
        let mut header = Header::new(rui, frame);
        let api = api::table();
        api::CURRENT.with(|c| c.set(frame as *mut Frame));
        // SAFETY: the element function reads the globals, the header it passes back to the
        // callbacks, and its data block, all live and sized as the engine sizes them.
        unsafe { (rui.func)(api.as_ptr() as *const u8, globals.0.as_ptr(), &mut header as *mut Header as *mut u8, rui.data.as_mut_ptr()) };
        api::CURRENT.with(|c| c.set(std::ptr::null_mut()));
        let stop = frame.stop.take();
        rui.bbox = frame.bbox(element);
        if let Some(s) = stop {
            return Err(s);
        }
        Ok(!header.hidden_now && !header.hidden)
    }
}

/// What a string or asset argument points at before it is set (`0x1721ba9`).
static EMPTY_STRING: [u8; 1] = [0];

/// The seeds of an instance's random numbers this frame (`0x2fc5b0`).
fn seeds(time: f32, seed: u64) -> [u32; 3] {
    let mut t = time * 1000.0;
    if t >= 9.223_372e18 {
        t -= 9.223_372e18;
    }
    let v = (t as i64 as u64).wrapping_mul(0x7a13_f074_99d0_a92d).wrapping_add(seed);
    let lo = v as u32;
    [
        ((lo.wrapping_mul(0x10_0000) & 0xf0_0000) | ((v >> 0x2b) as u32 & 0xf_ffff)).wrapping_add(0x2bd8_4418),
        ((lo.wrapping_mul(0x100) & 0xf_0000) | ((v >> 0xc) as u32 & 0xffff)).wrapping_add(0x61cc_1b32),
        (((v >> 0x1c) as u32 & 0x7fff) | (lo.wrapping_mul(0x800) & 0x7_8000)).wrapping_add(0x1cb7_94a6),
    ]
}

/// An instance's data block, 16-byte aligned as the engine's (`+0x60` of the instance).
pub struct Data {
    words: Vec<u128>,
    len: usize,
}

impl Data {
    fn new(len: usize) -> Data {
        Data { words: vec![0; len.div_ceil(16).max(1)], len }
    }

    pub fn bytes(&self) -> &[u8] {
        // SAFETY: `words` holds at least `len` initialised bytes.
        unsafe { std::slice::from_raw_parts(self.words.as_ptr() as *const u8, self.len) }
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: as above, uniquely borrowed.
        unsafe { std::slice::from_raw_parts_mut(self.words.as_mut_ptr() as *mut u8, self.len) }
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.words.as_mut_ptr() as *mut u8
    }

    pub fn write(&mut self, at: usize, v: &[u8]) {
        if let Some(d) = self.bytes_mut().get_mut(at..at + v.len()) {
            d.copy_from_slice(v);
        }
    }

    pub fn f32(&self, at: usize) -> f32 {
        self.bytes().get(at..at + 4).map_or(0.0, |b| f32::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn i32(&self, at: usize) -> i32 {
        self.bytes().get(at..at + 4).map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()))
    }

    pub fn u16(&self, at: usize) -> u16 {
        self.bytes().get(at..at + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u64(&self, at: usize) -> u64 {
        self.bytes().get(at..at + 8).map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap()))
    }
}

/// An instance of an element (`RuiCreate`).
pub struct Rui {
    pub name: String,
    func: ElementFn,
    pub group: u8,
    pub sort_key: i16,
    /// `RuiSetResolution`'s size, the element's own until set (`+0x08`, `+0x10`).
    pub size: [f32; 2],
    pub inv_size: [f32; 2],
    /// The time stamp counter at creation in the engine, which seeds the instance's random
    /// numbers (`+0x38`).
    pub seed: u64,
    pub visible: bool,
    /// The instance's `+0x4a`: draws ignore the safe area.
    pub no_safe_area: bool,
    /// Where its draws reached last frame, `x, y, w, h` in the unit square (`+0x20`).
    pub bbox: [f32; 4],
    pub data: Data,
    /// The strings and asset names its arguments point at, by the argument's offset.
    texts: HashMap<usize, std::ffi::CString>,
}

/// Argument types, as the script functions pass them to the lookup (`RuiSetString` 1,
/// `RuiSetAsset` 2, `RuiSetBool` 3, `RuiSetInt` 4, `RuiSetFloat` 5, `RuiSetFloat2` 6,
/// `RuiSetFloat3` 7, `RuiSetColorAlpha` 8, `RuiSetGameTime` 9, `RuiSetImage` 12; natives
/// `0x7a48b0` to `0x7a5330`).
pub mod arg {
    pub const STRING: u8 = 1;
    pub const ASSET: u8 = 2;
    pub const BOOL: u8 = 3;
    pub const INT: u8 = 4;
    pub const FLOAT: u8 = 5;
    pub const FLOAT2: u8 = 6;
    pub const FLOAT3: u8 = 7;
    pub const FLOAT4: u8 = 8;
    pub const GAMETIME: u8 = 9;
    pub const IMAGE: u8 = 12;
    /// A font face's slot, a u16 (`face` of `drawtext_vgui`, set by `0x720570`).
    pub const FACE: u8 = 14;
}

/// A value for an argument whatever its type, written the way its type is stored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i32),
    Float(f32),
    Vector(V4),
}

impl Rui {
    fn arg_at(&self, ui: &Ui, name: &str, kind: u8) -> Option<usize> {
        let e = ui.lib.elements.get(&self.name)?;
        e.find_arg(0, name, 0, kind).map(|a| a.offset as usize)
    }

    pub fn set_float(&mut self, ui: &Ui, name: &str, v: f32) -> bool {
        self.set(ui, name, arg::FLOAT, &v.to_le_bytes())
    }

    pub fn set_bool(&mut self, ui: &Ui, name: &str, v: bool) -> bool {
        self.set(ui, name, arg::BOOL, &(v as u32).to_le_bytes())
    }

    pub fn set_int(&mut self, ui: &Ui, name: &str, v: i32) -> bool {
        self.set(ui, name, arg::INT, &v.to_le_bytes())
    }

    /// `RuiSetFloat2`, `RuiSetFloat3` or the colour and alpha, by how many floats `v` holds.
    pub fn set_floats(&mut self, ui: &Ui, name: &str, v: &[f32]) -> bool {
        let kind = match v.len() {
            2 => arg::FLOAT2,
            3 => arg::FLOAT3,
            4 => arg::FLOAT4,
            _ => arg::FLOAT,
        };
        let bytes: Vec<u8> = v.iter().flat_map(|f| f.to_le_bytes()).collect();
        self.set(ui, name, kind, &bytes)
    }

    pub fn set_face(&mut self, ui: &Ui, name: &str, slot: u16) -> bool {
        self.set(ui, name, arg::FACE, &slot.to_le_bytes())
    }

    pub fn set_game_time(&mut self, ui: &Ui, name: &str, v: f32) -> bool {
        self.set(ui, name, arg::GAMETIME, &v.to_le_bytes())
    }

    /// `RuiSetString`: the argument points at a copy of `v` the instance keeps.
    pub fn set_string(&mut self, ui: &Ui, name: &str, v: &str) -> bool {
        self.set_text(ui, name, arg::STRING, v)
    }

    /// `RuiSetAsset`: the argument points at the asset's name.
    pub fn set_asset(&mut self, ui: &Ui, name: &str, v: &str) -> bool {
        self.set_text(ui, name, arg::ASSET, v)
    }

    /// `RuiSetImage`: the argument points at the image's name.
    pub fn set_image(&mut self, ui: &Ui, name: &str, v: &str) -> bool {
        self.set_text(ui, name, arg::IMAGE, v)
    }

    fn set_text(&mut self, ui: &Ui, name: &str, kind: u8, v: &str) -> bool {
        let Some(at) = self.arg_at(ui, name, kind) else { return false };
        let Ok(c) = std::ffi::CString::new(v) else { return false };
        self.data.write(at, &(c.as_ptr() as u64).to_le_bytes());
        self.texts.insert(at, c);
        true
    }

    /// Sets the argument `name` from `v`, converted to the argument's own type. False when the
    /// element has no plain argument of that name.
    pub fn set_value(&mut self, ui: &Ui, name: &str, v: Value) -> bool {
        let Some(e) = ui.lib.elements.get(&self.name) else { return false };
        let Some(a) = [arg::BOOL, arg::INT, arg::FLOAT, arg::GAMETIME, arg::FLOAT2, arg::FLOAT3, arg::FLOAT4].into_iter().find_map(|k| e.find_arg(0, name, 0, k)) else { return false };
        let f = match v {
            Value::Bool(b) => [b as u32 as f32, 0.0, 0.0, 0.0],
            Value::Int(i) => [i as f32, 0.0, 0.0, 0.0],
            Value::Float(x) => [x, 0.0, 0.0, 0.0],
            Value::Vector(c) => c,
        };
        let at = a.offset as usize;
        match a.kind {
            arg::BOOL => self.data.write(at, &((f[0] != 0.0) as u32).to_le_bytes()),
            arg::INT => self.data.write(at, &(f[0] as i32).to_le_bytes()),
            arg::FLOAT | arg::GAMETIME => self.data.write(at, &f[0].to_le_bytes()),
            k => {
                let n = (k - arg::FLOAT2 + 2) as usize;
                for (i, x) in f.iter().take(n).enumerate() {
                    self.data.write(at + 4 * i, &x.to_le_bytes());
                }
            }
        }
        true
    }

    fn set(&mut self, ui: &Ui, name: &str, kind: u8, v: &[u8]) -> bool {
        match self.arg_at(ui, name, kind) {
            Some(at) => {
                self.data.write(at, v);
                true
            }
            None => false,
        }
    }
}

/// The instance as the element's function and the callbacks see it: the engine keeps the
/// asset, the size, the draw scratch and the hide flags here, with the data block `0x60` on.
/// Only the callbacks read it; the function passes it back to them untouched.
#[repr(C)]
pub struct Header {
    rui: *mut Rui,
    size: [f32; 2],
    inv_size: [f32; 2],
    frame: *mut Frame,
    /// `+0x48`: hidden until shown again; `+0x49`: hidden this frame.
    hidden: bool,
    hidden_now: bool,
    element: *const Element,
    ui: *const Ui,
}

impl Header {
    fn new(rui: &mut Rui, frame: &mut Frame) -> Header {
        Header { rui, size: rui.size, inv_size: rui.inv_size, frame, hidden: false, hidden_now: false, element: frame.element, ui: frame.ui }
    }
}

/// The engine's per-draw scratch (the instance's `+0x18`): the transform code's position,
/// the transforms and sizes, the random seeds and the strings formatted this frame.
pub struct Frame {
    pub pc: usize,
    pub count: u32,
    pub seeds: [u32; 3],
    pub sizes: Box<[V4; MAX_TRANSFORMS]>,
    pub transforms: Box<[Transform; MAX_TRANSFORMS]>,
    /// The instance's size and its reciprocal (`+0x64a0`).
    pub unit: V4,
    pub strings: Box<[u8; STRING_BUFFER]>,
    pub strings_used: usize,
    pub topology: Topology,
    pub globals: Globals,
    /// The backbuffer's size while the screen blur runs (`0xd40b2a0`, set when ConVar
    /// `mat_screen_blur_enabled` is on), which the special image source draws from.
    pub screen_blur: Option<[i32; 2]>,
    pub data_size: usize,
    /// What the text layout leaves for the text jobs.
    pub text: text::Scratch,
    pub stop: Option<Stop>,
    /// The transform code hid the instance for this frame (`+0x49`).
    pub hide_now: bool,
    /// Messages the element reported through the callback at `+0x10`.
    pub errors: Vec<String>,
    pub(crate) element: *const Element,
    ui: *const Ui,
    data: *mut u8,
}

// SAFETY: the raw pointers are set at the start of an update and read only inside it, on the
// thread running it.
unsafe impl Send for Frame {}
unsafe impl Sync for Frame {}

impl Default for Frame {
    fn default() -> Frame {
        Frame {
            pc: 0,
            count: 3,
            seeds: [0; 3],
            sizes: Box::new([[0.0; 4]; MAX_TRANSFORMS]),
            transforms: Box::new([Transform::default(); MAX_TRANSFORMS]),
            unit: [1.0; 4],
            strings: Box::new([0; STRING_BUFFER]),
            strings_used: 0,
            topology: Topology { org: [0.0; 3], right: [1.0, 0.0, 0.0], down: [0.0, 1.0, 0.0] },
            globals: Globals::new(0.0, 1.0, 1.0),
            screen_blur: None,
            data_size: 0,
            text: text::Scratch::default(),
            stop: None,
            hide_now: false,
            errors: Vec::new(),
            element: std::ptr::null(),
            ui: std::ptr::null(),
            data: std::ptr::null_mut(),
        }
    }
}

impl Frame {
    fn reset(&mut self, element: &Element) {
        self.pc = 0;
        self.count = 3;
        self.strings_used = 0;
        self.data_size = element.data_size as usize;
        (self.text.line_count, self.text.image_count, self.text.nested_count) = (0, 0, 0);
        self.stop = None;
        self.element = element;
    }

    /// `N` bytes of the running instance's data block at `at`; zeros past its end.
    fn read<const N: usize>(&self, at: usize) -> [u8; N] {
        if self.data.is_null() || at + N > self.data_size {
            return [0; N];
        }
        // SAFETY: `data` is the instance's data block, `data_size` bytes, for the update.
        unsafe { std::ptr::read_unaligned(self.data.add(at) as *const [u8; N]) }
    }

    pub fn f32(&self, at: usize) -> f32 {
        f32::from_le_bytes(self.read(at))
    }

    pub fn u16(&self, at: usize) -> u16 {
        u16::from_le_bytes(self.read(at))
    }

    pub fn u64(&self, at: usize) -> u64 {
        u64::from_le_bytes(self.read(at))
    }

    /// The box the element's draws cover this frame, over each job's transform
    /// (`0x2fc85d`): `x, y, w, h`.
    fn bbox(&self, element: &Element) -> [f32; 4] {
        let (mut lo, mut hi) = ([f32::MAX; 4], [f32::MIN; 4]);
        let mut at = 0;
        for _ in 0..element.job_count {
            let Some(head) = element.jobs.get(at..at + 4) else { break };
            let (kind, t) = (u16::from_le_bytes([head[0], head[1]]) as usize, u16::from_le_bytes([head[2], head[3]]) as usize);
            let tr = self.transforms[t.min(MAX_TRANSFORMS - 1)];
            let b = [tr.basis[0], tr.basis[1], tr.basis[0], tr.basis[1]];
            let c = [tr.basis[2], tr.basis[3], tr.basis[2], tr.basis[3]];
            for k in 0..4 {
                let low = b[k].min(0.0) + tr.origin[k] + c[k].min(0.0);
                let high = b[k].max(0.0) + tr.origin[k] + c[k].max(0.0);
                if low < lo[k] {
                    lo[k] = low;
                }
                if high > hi[k] {
                    hi[k] = high;
                }
            }
            at += rasset::JOB_SIZES.get(kind).copied().unwrap_or(0);
        }
        [lo[0], lo[1], hi[0] - lo[0], hi[1] - lo[1]]
    }
}
