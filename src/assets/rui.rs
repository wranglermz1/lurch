//! RUI ("Respawn UI") elements: `ui` assets (version 39) in `ui.rpak`, with the image atlases
//! (`uimg` version 10) and the colour palette (`datatable/colorpalette_table`) their code draws
//! with. An element's logic is a function of the same name in `ui.dll` (`crate::rui`); what
//! each block below is, and the code that reads it, is in `docs/ui.md`.

use std::collections::HashMap;
use std::io;
use std::path::Path;

use super::rpak::{self, Pak};
use super::texture::{self, Texture};
use super::Install;

fn bad(what: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.into())
}

/// Bytes in a draw job of each type (`0x1300a70`).
pub const JOB_SIZES: [usize; 6] = [0x16, 0x2e, 0x1c, 0x1c, 0x2e, 0xc];
pub const STYLE_SIZE: usize = 0x44;
pub const CLUSTER_SIZE: usize = 0x12;
pub const ARG_SIZE: usize = 8;

/// One `ui` asset's blocks, copied out of its pak.
#[derive(Clone, Debug)]
pub struct Element {
    pub name: String,
    /// The size the element is laid out at (`+0x18`) and its reciprocal (`+0x20`).
    pub size: [f32; 2],
    pub inv_size: [f32; 2],
    /// The data block's first bytes as the element starts (`+0x08`, `+0x46` bytes).
    pub defaults: Vec<u8>,
    /// The constant strings the defaults point at, by where the pointer is, each with its NUL:
    /// the pak's pointers, which the engine relocates on load.
    pub strings: Vec<(u16, Vec<u8>)>,
    /// The data block's size (`+0x44`).
    pub data_size: u16,
    /// The transform code run through the callback at `+0x30` (`+0x10`).
    pub code: Vec<u8>,
    /// Draw styles, `STYLE_SIZE` bytes each (`+0x50`, count `+0x48`).
    pub styles: Vec<u8>,
    /// Draw jobs, `JOB_SIZES[type]` bytes each (`+0x58`, count `+0x4c`).
    pub jobs: Vec<u8>,
    pub job_count: u16,
    /// Argument clusters, `CLUSTER_SIZE` bytes each (`+0x30`), and arguments, `ARG_SIZE` bytes
    /// each (`+0x38`, count `+0x40`).
    pub clusters: Vec<u8>,
    pub args: Vec<u8>,
    /// Curves read through the callbacks at `+0xa0` to `+0xb8` (`+0x60`, count `+0x42`).
    pub mappings: Vec<Mapping>,
    /// How many transforms the element's code makes, counting the three every element has
    /// (`+0x4a`).
    pub transforms: u16,
    /// How many elements of an argument cluster the data block holds (`+0x4e`).
    pub elements: u16,
}

/// A curve: `keys` sorted inputs, then `stride` outputs per key (twice that, value and slope,
/// when `cubic`), as the engine reads them in `0x2fc060`.
#[derive(Clone, Debug, PartialEq)]
pub struct Mapping {
    pub keys: u32,
    pub stride: u16,
    pub cubic: bool,
    pub data: Vec<f32>,
}

/// An argument's entry: its type, the cluster element it belongs to, where its value is in the
/// data block, and the hash check (`0x301840`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arg {
    pub kind: u8,
    pub element: u8,
    pub offset: u16,
    pub check: u16,
}

impl Element {
    fn u16_at(b: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([b[at], b[at + 1]])
    }

    pub fn arg(&self, i: usize) -> Arg {
        let e = &self.args[i * ARG_SIZE..];
        Arg { kind: e[0], element: e[1], offset: Self::u16_at(e, 2), check: Self::u16_at(e, 6) }
    }

    /// The argument named `name` of cluster `cluster` and type `kind`, as `0x301840` finds it.
    /// Type 1 also matches type 12.
    pub fn find_arg(&self, cluster: usize, name: &str, element: u8, kind: u8) -> Option<Arg> {
        let c = self.clusters.get(cluster * CLUSTER_SIZE..(cluster + 1) * CLUSTER_SIZE)?;
        let (first, count) = (Self::u16_at(c, 0) as u32, Self::u16_at(c, 2) as u32);
        if count == 0 {
            return None;
        }
        let (mul, add) = (c[4] as u32, c[5] as u32);
        let mut h = element as u32;
        for &b in name.as_bytes() {
            h = ((b as i8 as i32 as u32).wrapping_add(mul.wrapping_mul(h)).wrapping_add(add)) ^ (h >> 20);
        }
        let a = self.arg(((count - 1) & h).wrapping_add(first) as usize);
        (a.check == (h >> 4) as u16 && (a.kind == kind || (kind == 1 && a.kind == 12)) && a.element == element).then_some(a)
    }

    /// The bytes per cluster element of the data block (the cluster's `+0xa`).
    pub fn element_stride(&self, cluster: usize) -> u16 {
        Self::u16_at(&self.clusters, cluster * CLUSTER_SIZE + 0xa)
    }
}

/// Reads every `ui` asset of a pak.
pub fn elements(pak: &Pak) -> HashMap<String, Element> {
    pak.assets.iter().filter(|a| &a.magic == b"ui\0\0" && a.version == 39).filter_map(|a| element(pak, a.head).ok()).map(|e| (e.name.clone(), e)).collect()
}

fn element(pak: &Pak, h: usize) -> io::Result<Element> {
    let name = pak.ptr(h).map(|p| pak.cstr(p).to_string()).ok_or_else(|| bad("ui asset without a name"))?;
    let u = |o: usize| pak.u16(h + o) as usize;
    let block = |o: usize, len: usize| pak.ptr(h + o).map(|p| pak.data()[p..p + len].to_vec()).unwrap_or_default();
    let code_len = match (pak.ptr(h + 0x10), pak.ptr(h + 0x50).or(pak.ptr(h + 0x58)).or(pak.ptr(h + 0x30))) {
        (Some(c), Some(next)) if next > c => next - c,
        _ => 0,
    };
    let mut jobs_len = 0;
    if let Some(j) = pak.ptr(h + 0x58) {
        for _ in 0..u(0x4c) {
            let t = pak.u16(j + jobs_len) as usize;
            jobs_len += *JOB_SIZES.get(t).ok_or_else(|| bad(format!("{name}: job type {t}")))?;
        }
    }
    let clusters_len = match (pak.ptr(h + 0x30), pak.ptr(h + 0x38)) {
        (Some(c), Some(a)) if a > c => a - c,
        (Some(_), _) => CLUSTER_SIZE,
        _ => 0,
    };
    let mut mappings = Vec::new();
    if let Some(m) = pak.ptr(h + 0x60) {
        for i in 0..u(0x42) {
            let r = m + 16 * i;
            let (keys, stride, cubic) = (pak.u32(r), pak.u16(r + 4), pak.u16(r + 6) != 0);
            let n = keys as usize * (1 + stride as usize * if cubic { 2 } else { 1 });
            let p = pak.ptr(r + 8).ok_or_else(|| bad(format!("{name}: mapping {i} without data")))?;
            mappings.push(Mapping { keys, stride, cubic, data: (0..n).map(|k| pak.f32(p + 4 * k)).collect() });
        }
    }
    let strings = match pak.ptr(h + 8) {
        Some(d) => pak.pointers_in(d, u(0x46)).iter().filter_map(|&p| Some(((p - d) as u16, [pak.cstr(pak.ptr(p)?).as_bytes(), b"\0"].concat()))).collect(),
        None => Vec::new(),
    };
    Ok(Element {
        strings,
        size: [pak.f32(h + 0x18), pak.f32(h + 0x1c)],
        inv_size: [pak.f32(h + 0x20), pak.f32(h + 0x24)],
        defaults: block(0x08, u(0x46)),
        data_size: u(0x44) as u16,
        code: block(0x10, code_len),
        styles: block(0x50, u(0x48) * STYLE_SIZE),
        jobs: block(0x58, jobs_len),
        job_count: u(0x4c) as u16,
        clusters: block(0x30, clusters_len),
        args: block(0x38, u(0x40) * ARG_SIZE),
        mappings,
        transforms: u(0x4a) as u16,
        elements: u(0x4e) as u16,
        name,
    })
}

/// An image atlas (`uimg` version 10): one texture and the images packed in it.
#[derive(Clone, Debug)]
pub struct Atlas {
    pub width: u16,
    pub height: u16,
    /// Each image's place in the texture, `[u, v, width, height]` in texture units, inset by
    /// half a texel (the asset's CPU data, which the engine uploads as a buffer, `0x2f3970`).
    pub uv: Vec<[f32; 4]>,
    /// Each image's mapping from the quad onto its texels (`+0x10`, `0x20` bytes each).
    pub frames: Vec<[f32; 8]>,
    /// Each image's size in pixels (`+0x18`).
    pub sizes: Vec<[u16; 2]>,
    /// The nine-slice borders of the first images (`+0x20`, count `+0x0e`).
    pub slices: Vec<[f32; 8]>,
    /// Each image's name hash and flag bytes (`+0x28`).
    pub names: Vec<(u32, [u8; 4])>,
    pub texture: u64,
}

pub fn atlases(pak: &Pak) -> Vec<Atlas> {
    pak.assets
        .iter()
        .filter(|a| &a.magic == b"uimg" && a.version == 10)
        .filter_map(|a| {
            let h = a.head;
            let n = pak.u16(h + 0xc) as usize;
            let f8 = |p: usize, i: usize| -> [f32; 8] { std::array::from_fn(|k| pak.f32(p + 0x20 * i + 4 * k)) };
            let (frames, sizes, names) = (pak.ptr(h + 0x10)?, pak.ptr(h + 0x18)?, pak.ptr(h + 0x28)?);
            let cpu = a.cpu?;
            Some(Atlas {
                width: pak.u16(h + 8),
                height: pak.u16(h + 0xa),
                uv: (0..n).map(|i| std::array::from_fn(|k| pak.f32(cpu + 16 * i + 4 * k))).collect(),
                frames: (0..n).map(|i| f8(frames, i)).collect(),
                sizes: (0..n).map(|i| [pak.u16(sizes + 4 * i), pak.u16(sizes + 4 * i + 2)]).collect(),
                slices: pak.ptr(h + 0x20).map(|s| (0..pak.u16(h + 0xe) as usize).map(|i| f8(s, i)).collect()).unwrap_or_default(),
                names: (0..n).map(|i| (pak.u32(names + 8 * i), std::array::from_fn(|k| pak.u8(names + 8 * i + 4 + k)))).collect(),
                texture: pak.u64(h + 0x38),
            })
        })
        .collect()
}

/// A font atlas (`font` version 7, header `0x28`): one R8 texture of signed distances holding
/// the glyphs of its faces. Loading it (`0x2f3870`) puts each face in the engine's font table
/// at the face's slot and copies the header to the atlas's place in the table at `0x270f4c30`.
#[derive(Clone, Debug)]
pub struct FontAtlas {
    /// `+4`, `+6`, and their reciprocals as stored (`+8`, `+0xc`).
    pub width: u16,
    pub height: u16,
    pub inv_size: [f32; 2],
    /// Where a line may break (`+0x18`): a bit per pair of glyph classes, previous by current,
    /// `classes` (`+2`) to a row, each pair twice: without and with a space between.
    pub classes: u16,
    pub breaks: Vec<u8>,
    /// `+0x10`, count `+0`, `0x60` bytes each.
    pub faces: Vec<Face>,
    pub texture: u64,
}

/// A face of a font atlas.
#[derive(Clone, Debug)]
pub struct Face {
    /// Its slot in the font table (`+8`): where a lookup of the font's name lands (`0x2f3430`).
    pub slot: u16,
    /// Glyphs by code point (`0x2f4bc0`): from `first` (`+0x14`), each 64 code points name an
    /// entry (`+0x30`, `+0x0e` of them) of a bit per code point present (`+0x40`) and the glyph
    /// of its lowest bit (`+0x38`).
    pub first: u32,
    pub blocks: Vec<u16>,
    pub masks: Vec<u64>,
    pub bases: Vec<u16>,
    /// `+0x1c` to `+0x28`: an em's width and height in the texture's units, then the two
    /// heights the layout offsets a line by, in ems.
    pub metrics: [f32; 4],
    /// Where its glyphs start in the atlas's glyph bounds (`+0x2c`).
    pub base: u32,
    /// Per glyph size class, the scale it is stored at in the texture and the margin around
    /// it (`+0x48`, count `+0x0a`).
    pub scales: Vec<[f32; 2]>,
    /// `+0x50`, count `+0x18`, and one more whose kerning index ends the last glyph's.
    pub glyphs: Vec<Glyph>,
    /// Pairs of the code point before and the advance added after it, in ems (`+0x58`, count
    /// `+0x0c`).
    pub kerning: Vec<(u32, f32)>,
}

/// A glyph (`0x20` bytes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    /// `+0`, in ems.
    pub advance: f32,
    /// Its first kerning pair (`+4`); the next glyph's ends them.
    pub kerning: u16,
    /// Its class for line breaks (`+6`).
    pub class: u8,
    /// Its size class, into the face's `scales` (`+7`).
    pub size: u8,
    /// Its place in the texture (`+8`, `+0xc`).
    pub uv: [f32; 2],
    /// Its left, top, right and bottom in ems (`+0x10` to `+0x1c`); empty when left and right
    /// match.
    pub extent: [f32; 4],
}

impl Face {
    /// The glyph of `c`, or of U+25A1 when the face has none, or None (`0x2f4bc0`).
    pub fn glyph(&self, c: u32) -> Option<u32> {
        let find = |c: u32| {
            let u = c.wrapping_sub(self.first);
            let entry = *self.blocks.get((u >> 6) as usize)? as usize;
            let (mask, bit) = (*self.masks.get(entry)?, 1u64 << (u & 63));
            let base = *self.bases.get(entry)? as u32;
            (mask & bit != 0).then(|| ((bit - 1) & mask).count_ones() + base)
        };
        find(c).or_else(|| find(0x25a1))
    }
}

pub fn fonts(pak: &Pak) -> Vec<FontAtlas> {
    pak.assets
        .iter()
        .filter(|a| &a.magic == b"font" && a.version == 7)
        .filter_map(|a| {
            let h = a.head;
            let classes = pak.u16(h + 2);
            let breaks = pak.ptr(h + 0x18)?;
            let at = pak.ptr(h + 0x10)?;
            let faces = (0..pak.u16(h) as usize)
                .map(|i| {
                    let r = at + 0x60 * i;
                    let blocks: Vec<u16> = pak.ptr(r + 0x30).map(|p| (0..pak.u16(r + 0xe) as usize).map(|k| pak.u16(p + 2 * k)).collect()).unwrap_or_default();
                    let entries = blocks.iter().map(|&b| b as usize + 1).max().unwrap_or(0);
                    let glyphs = pak.ptr(r + 0x50).map(|p| {
                        (0..=pak.u32(r + 0x18) as usize)
                            .map(|k| {
                                let g = p + 0x20 * k;
                                Glyph {
                                    advance: pak.f32(g),
                                    kerning: pak.u16(g + 4),
                                    class: pak.u8(g + 6),
                                    size: pak.u8(g + 7),
                                    uv: [pak.f32(g + 8), pak.f32(g + 0xc)],
                                    extent: std::array::from_fn(|j| pak.f32(g + 0x10 + 4 * j)),
                                }
                            })
                            .collect()
                    });
                    Face {
                        slot: pak.u16(r + 8),
                        first: pak.u32(r + 0x14),
                        masks: pak.ptr(r + 0x40).map(|p| (0..entries).map(|k| pak.u64(p + 8 * k)).collect()).unwrap_or_default(),
                        bases: pak.ptr(r + 0x38).map(|p| (0..entries).map(|k| pak.u16(p + 2 * k)).collect()).unwrap_or_default(),
                        blocks,
                        metrics: std::array::from_fn(|k| pak.f32(r + 0x1c + 4 * k)),
                        base: pak.u32(r + 0x2c),
                        scales: pak.ptr(r + 0x48).map(|p| (0..pak.u16(r + 0xa) as usize).map(|k| [pak.f32(p + 8 * k), pak.f32(p + 8 * k + 4)]).collect()).unwrap_or_default(),
                        glyphs: glyphs.unwrap_or_default(),
                        kerning: pak.ptr(r + 0x58).map(|p| (0..pak.u16(r + 0xc) as usize).map(|k| (pak.u32(p + 8 * k), pak.f32(p + 8 * k + 4))).collect()).unwrap_or_default(),
                    }
                })
                .collect();
            Some(FontAtlas {
                width: pak.u16(h + 4),
                height: pak.u16(h + 6),
                inv_size: [pak.f32(h + 8), pak.f32(h + 0xc)],
                classes,
                breaks: (0..(classes as usize * classes as usize * 2).div_ceil(8)).map(|k| pak.u8(breaks + k)).collect(),
                faces,
                texture: pak.u64(h + 0x20),
            })
        })
        .collect()
}

impl FontAtlas {
    /// The glyph bounds the shader tests each pixel against (`g_fontBounds`, built at load by
    /// `0x2f3460`): per glyph of each face at the face's `base`, `mins, maxs` in the texture,
    /// or `1, 1, 0, 0` for an empty glyph.
    pub fn bounds(&self) -> Vec<[f32; 4]> {
        let count = |f: &Face| f.glyphs.len().saturating_sub(1);
        let mut out = vec![[1.0, 1.0, 0.0, 0.0]; self.faces.iter().map(|f| f.base as usize + count(f)).max().unwrap_or(0)];
        for f in &self.faces {
            let (sx, sy) = (f.metrics[0], f.metrics[1]);
            for (i, g) in f.glyphs.iter().take(count(f)).enumerate() {
                let [left, top, right, bottom] = g.extent;
                if left == right {
                    continue;
                }
                let [s, p] = f.scales.get(g.size as usize).copied().unwrap_or_default();
                let (x, y) = (sx * s, sy * s);
                let (px, py) = (p * x, p * y);
                out[f.base as usize + i] = [(left * x + g.uv[0]) - px, (y * top + g.uv[1]) - py, x * right + g.uv[0] + px, bottom * y + g.uv[1] + py];
            }
        }
        out
    }
}

/// The font table's slot for a font's name (`0x2f3430`): its asset hash folded to five bits.
pub fn font_slot(name: &str) -> u16 {
    let g = rpak::guid(name);
    (g as u16 ^ (g >> 11) as u16) & 0x1f
}

/// The engine's image lookup key for a name: the halves of its asset hash folded together
/// (`0x2fc420`).
pub fn image_hash(name: &str) -> u32 {
    let g = rpak::guid(name);
    (g as u32) ^ (g >> 32) as u32
}

/// `datatable/colorpalette_table`: per colour id, a name and its `default` column (0 to 255),
/// the column `0x2fc430` reads.
pub fn palette(pak: &Pak) -> io::Result<Vec<(String, [f32; 3])>> {
    let a = pak.asset(rpak::guid("datatable/colorpalette_table.rpak")).ok_or_else(|| bad("no colorpalette_table"))?;
    let h = a.head;
    let (columns, rows) = (pak.ptr(h + 8).ok_or_else(|| bad("palette without columns"))?, pak.ptr(h + 0x10).ok_or_else(|| bad("palette without rows"))?);
    let (count, stride) = (pak.u32(h + 4) as usize, pak.u32(h + 0x20) as usize);
    let column = |c: usize| (pak.u32(columns + 0x10 * c + 8), pak.u32(columns + 0x10 * c + 0xc) as usize);
    let ((name_type, name_at), (default_type, default_at)) = (column(0), column(1));
    if name_type != 4 || default_type != 3 {
        return Err(bad("colorpalette_table columns are not (string, vector)"));
    }
    Ok((0..count)
        .map(|r| {
            let row = rows + r * stride;
            let name = pak.ptr(row + name_at).map(|p| pak.cstr(p).to_string()).unwrap_or_default();
            (name, std::array::from_fn(|k| pak.f32(row + default_at + 4 * k)))
        })
        .collect())
}

/// What the UI draws with, read from an install: every element, every atlas with its texture,
/// the fonts with theirs, the colour palette, and each key's display string.
pub struct Library {
    pub elements: HashMap<String, Element>,
    pub atlases: Vec<Atlas>,
    pub textures: Vec<Option<Texture>>,
    pub fonts: Vec<FontAtlas>,
    pub font_textures: Vec<Option<Texture>>,
    pub palette: Vec<(String, [f32; 3])>,
    /// `scripts/keys_keyboard.rson` by key code (`ButtonCode_t`): `%@KEY_E%` for E.
    pub keys: Vec<String>,
}

/// An `rson` asset that is a list of strings (type `0x1002`, its count, then a pointer to the
/// strings' pointers), as the engine checks `scripts/keys_*.rson` (`0x236e40`).
fn string_list(pak: &Pak, name: &str) -> Vec<String> {
    let Some(a) = pak.asset(rpak::guid(name)) else { return Vec::new() };
    let h = a.head;
    if pak.u32(h) != 0x1002 {
        return Vec::new();
    }
    let Some(list) = pak.ptr(h + 8) else { return Vec::new() };
    (0..pak.u32(h + 4) as usize).map(|i| pak.ptr(list + 8 * i).map(|p| pak.cstr(p).to_string()).unwrap_or_default()).collect()
}

/// The elements the UI script registers for text to embed (`ui/_embedded_text.nut`): each
/// `InitEmbeddedKeyRui( $"ui/<element>.rpak", "<name>", "<displayText>" )` and
/// `RuiCreateEmbedded( $"ui/<element>.rpak", "<name>" )` call outside a comment, in order.
pub fn embedded_text(install: &Path) -> Vec<(String, String, Option<String>)> {
    let path = install.join("platform/scripts/vscripts/ui/_embedded_text.nut");
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let quoted = |s: &str| -> Vec<String> { s.split('"').skip(1).step_by(2).map(str::to_string).collect() };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("//"))
        .filter_map(|l| {
            let q = quoted(l);
            let element = q.first()?.strip_prefix("ui/")?.strip_suffix(".rpak")?.to_string();
            if l.starts_with("InitEmbeddedKeyRui(") && q.len() == 3 {
                Some((element, q[1].clone(), Some(q[2].clone())))
            } else if l.starts_with("RuiCreateEmbedded(") && q.len() == 2 {
                Some((element, q[1].clone(), None))
            } else {
                None
            }
        })
        .collect()
}

/// The paks whose atlases are registered, in order: a later pak's image replaces an earlier
/// one of the same name, as `0x2f39c0` does.
pub const ATLAS_PAKS: &[&str] = &["ui.rpak", "ui_mp.rpak"];

pub fn library(install: &Path) -> io::Result<Library> {
    let inst = Install::open(install)?;
    let mut elements = HashMap::new();
    let (mut atlas_list, mut textures) = (Vec::new(), Vec::new());
    let (mut font_list, mut font_textures) = (Vec::new(), Vec::new());
    for (i, name) in ATLAS_PAKS.iter().enumerate() {
        let pak = match inst.pak(name) {
            Ok(p) => p,
            Err(e) if i == 0 => return Err(e),
            Err(_) => continue,
        };
        if i == 0 {
            elements = self::elements(&pak);
            font_list = fonts(&pak);
            font_textures = font_list.iter().map(|f| texture::load(&pak, install, f.texture).ok()).collect();
        }
        for a in atlases(&pak) {
            textures.push(texture::load(&pak, install, a.texture).ok());
            atlas_list.push(a);
        }
    }
    let early = inst.pak("common_early.rpak")?;
    let keys = string_list(&early, "scripts/keys_keyboard.rson");
    Ok(Library { elements, atlases: atlas_list, textures, fonts: font_list, font_textures, palette: palette(&early)?, keys })
}
