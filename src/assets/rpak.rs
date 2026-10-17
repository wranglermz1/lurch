//! RPak version 8, the game's asset archive (`paks/Win64/*.rpak`), and the streamed data
//! files beside it (`*.starpak`). A pak is read whole into memory and decompressed; pointers
//! inside it are (page, offset) pairs into its memory pages.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::{rtech, zstd};

const HEADER: usize = 0x80;
const ENTRY: usize = 0x50;
const FLAG_RTECH: u16 = 0x100;
const FLAG_OODLE: u16 = 0x200;
const FLAG_ZSTD: u16 = 0x8000;

#[derive(Clone, Copy, Debug)]
pub struct Asset {
    pub guid: u64,
    /// The asset's header, in the pak's memory.
    pub head: usize,
    pub head_size: u32,
    /// Its CPU data, when it has any.
    pub cpu: Option<usize>,
    /// Streamed data: the starpak index in the low 12 bits, the offset above them; -1 for none.
    pub starpak: i64,
    pub opt_starpak: i64,
    pub version: u32,
    pub magic: [u8; 4],
}

pub struct Pak {
    pub path: PathBuf,
    d: Vec<u8>,
    pages: Vec<usize>,
    /// Where the pak's pointers are (its pointer descriptors, each a page and an offset), the
    /// fields the engine relocates when it loads the pak; sorted.
    pointers: Vec<usize>,
    pub assets: Vec<Asset>,
    by_guid: HashMap<u64, usize>,
    starpaks: Vec<String>,
    opt_starpaks: Vec<String>,
}

fn u32_at_d(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(d[at..at + 4].try_into().unwrap())
}

fn bad(what: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.into())
}

/// The engine's hash of an asset name (`0x46cd80`), which is the asset's GUID: four bytes at a
/// time, case folded, `\` read as `/`, ending at the NUL. RUI images are found by this hash of
/// their name too.
pub fn guid(name: &str) -> u64 {
    let bytes = name.as_bytes();
    let word = |at: usize| u32::from_le_bytes(std::array::from_fn(|i| bytes.get(at + i).copied().unwrap_or(0)));
    let (mut h, mut n, mut at) = (0u64, 0u32, 0);
    loop {
        let w = word(at);
        let zero = w.wrapping_sub(0x0101_0101) & !w & 0x8080_8080;
        let keep = zero.wrapping_sub(1) ^ zero;
        let s = w & keep;
        let x = s ^ 0x5c5c_5c5c;
        let slashes = x.wrapping_sub(0x0101_0101) & !x & 0x8080_8080;
        let mut lowest = slashes & slashes.wrapping_neg();
        if slashes != lowest {
            let mut m = 0xff00_0000u32;
            loop {
                if x & m == 0 {
                    lowest |= m & 0x8080_8080;
                }
                let prev = m;
                m >>= 8;
                if prev < 0x100 {
                    break;
                }
            }
        }
        let v = ((s.wrapping_sub((lowest >> 7).wrapping_mul(0x2d)) as u64) & 0xdfdf_dfdf).wrapping_mul(0xfb8_c4d9_6501) >> 24;
        let mixed = h.wrapping_mul(0x633_d5f1);
        if zero != 0 {
            let top = 31 - keep.leading_zeros() as i32;
            let len = ((top + ((top >> 31) & 7)) >> 3).wrapping_add(n as i32) as u32;
            return (len as u64).wrapping_mul(0xff51_afd7_ed55_8ccd).wrapping_add(mixed).wrapping_add(v);
        }
        h = mixed.wrapping_add(v);
        h ^= h >> 61;
        at += 4;
        n += 4;
    }
}

fn strings(b: &[u8]) -> Vec<String> {
    b.split(|&c| c == 0).filter(|s| !s.is_empty()).map(|s| String::from_utf8_lossy(s).into_owned()).collect()
}

impl Pak {
    /// Opens a pak; `rtech_lut` is the RTech decoder's table, needed for RTech-compressed paks.
    pub fn open(path: &Path, rtech_lut: Option<&[u8]>) -> io::Result<Pak> {
        let raw = std::fs::read(path)?;
        Pak::parse(path, raw, rtech_lut)
    }

    pub fn parse(path: &Path, raw: Vec<u8>, rtech_lut: Option<&[u8]>) -> io::Result<Pak> {
        if raw.len() < HEADER || &raw[..4] != b"RPak" {
            return Err(bad(format!("{}: not an rpak", path.display())));
        }
        let u16_at = |o: usize| u16::from_le_bytes([raw[o], raw[o + 1]]);
        let u32_at = |o: usize| u32::from_le_bytes(raw[o..o + 4].try_into().unwrap());
        if u16_at(4) != 8 {
            return Err(bad(format!("{}: rpak version {} (8 expected)", path.display(), u16_at(4))));
        }
        let flags = u16_at(6);
        let (sref, oref, nvseg, npage) = (u16_at(72) as usize, u16_at(74) as usize, u16_at(76) as usize, u16_at(78) as usize);
        let (patch, ndesc, nasset, nguid, nrel) =
            (u32_at(80) as usize, u32_at(84) as usize, u32_at(88) as usize, u32_at(92) as usize, u32_at(96) as usize);
        if patch != 0 {
            return Err(bad(format!("{}: patch paks are not supported", path.display())));
        }
        let d = if flags & FLAG_ZSTD != 0 {
            let mut d = raw[..HEADER].to_vec();
            zstd::decompress_into(&raw[HEADER..], &mut d).map_err(|e| bad(format!("{}: {e}", path.display())))?;
            d
        } else if flags & FLAG_RTECH != 0 {
            let lut = rtech_lut.ok_or_else(|| bad(format!("{}: RTech pak and no decoder table", path.display())))?;
            let size = u64::from_le_bytes(raw[48..56].try_into().unwrap()) as usize;
            rtech::decompress(&raw, lut, size).map_err(|e| bad(format!("{}: {e}", path.display())))?
        } else if flags & FLAG_OODLE != 0 {
            return Err(bad(format!("{}: Oodle compression is not supported", path.display())));
        } else {
            raw
        };
        let mut o = HEADER;
        let starpaks = strings(&d[o..o + sref]);
        o += sref;
        let opt_starpaks = strings(&d[o..o + oref]);
        o += oref;
        o += nvseg * 16;
        let page_sizes: Vec<usize> = (0..npage).map(|i| u32::from_le_bytes(d[o + i * 12 + 8..o + i * 12 + 12].try_into().unwrap()) as usize).collect();
        o += npage * 12;
        let descriptors = o;
        o += ndesc * 8;
        let entries = o;
        o += nasset * ENTRY + nguid * 8 + nrel * 4;
        let total: usize = page_sizes.iter().sum();
        if o + total != d.len() {
            return Err(bad(format!("{}: page data does not follow the header ({} + {} != {})", path.display(), o, total, d.len())));
        }
        let mut pages = Vec::with_capacity(npage);
        let mut at = o;
        for s in &page_sizes {
            pages.push(at);
            at += s;
        }
        let at = |page: i32, off: i32| (page >= 0).then(|| pages.get(page as usize).map(|p| p + off as usize)).flatten();
        let mut pointers: Vec<usize> = (0..ndesc).filter_map(|i| at(u32_at_d(&d, descriptors + 8 * i) as i32, u32_at_d(&d, descriptors + 8 * i + 4) as i32)).collect();
        pointers.sort_unstable();
        let mut assets = Vec::with_capacity(nasset);
        for i in 0..nasset {
            let e = &d[entries + i * ENTRY..entries + (i + 1) * ENTRY];
            let i32_at = |o: usize| i32::from_le_bytes(e[o..o + 4].try_into().unwrap());
            let i64_at = |o: usize| i64::from_le_bytes(e[o..o + 8].try_into().unwrap());
            let Some(head) = at(i32_at(0x10), i32_at(0x14)) else {
                return Err(bad(format!("{}: asset {i} has no header", path.display())));
            };
            assets.push(Asset {
                guid: i64_at(0) as u64,
                head,
                head_size: i32_at(0x44) as u32,
                cpu: at(i32_at(0x18), i32_at(0x1c)),
                starpak: i64_at(0x20),
                opt_starpak: i64_at(0x28),
                version: i32_at(0x48) as u32,
                magic: e[0x4c..0x50].try_into().unwrap(),
            });
        }
        let by_guid = assets.iter().enumerate().map(|(i, a)| (a.guid, i)).collect();
        Ok(Pak { path: path.to_path_buf(), d, pages, pointers, assets, by_guid, starpaks, opt_starpaks })
    }

    pub fn data(&self) -> &[u8] {
        &self.d
    }

    /// Where the memory page holding `at` ends.
    pub fn page_end(&self, at: usize) -> usize {
        let i = self.pages.partition_point(|&p| p <= at);
        self.pages.get(i).copied().unwrap_or(self.d.len())
    }

    pub fn asset(&self, guid: u64) -> Option<&Asset> {
        self.by_guid.get(&guid).map(|&i| &self.assets[i])
    }

    pub fn u8(&self, at: usize) -> u8 {
        self.d[at]
    }
    pub fn u16(&self, at: usize) -> u16 {
        u16::from_le_bytes(self.d[at..at + 2].try_into().unwrap())
    }
    pub fn i16(&self, at: usize) -> i16 {
        self.u16(at) as i16
    }
    pub fn u32(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.d[at..at + 4].try_into().unwrap())
    }
    pub fn i32(&self, at: usize) -> i32 {
        self.u32(at) as i32
    }
    pub fn u64(&self, at: usize) -> u64 {
        u64::from_le_bytes(self.d[at..at + 8].try_into().unwrap())
    }
    pub fn f32(&self, at: usize) -> f32 {
        f32::from_bits(self.u32(at))
    }

    /// The pointer fields in `at..at + len`, the ones the engine relocates on load.
    pub fn pointers_in(&self, at: usize, len: usize) -> &[usize] {
        let lo = self.pointers.partition_point(|&p| p < at);
        let hi = self.pointers.partition_point(|&p| p < at + len);
        &self.pointers[lo..hi]
    }

    /// A pointer field: (page, offset), or None when null or out of the pak.
    pub fn ptr(&self, at: usize) -> Option<usize> {
        let (page, off) = (self.u32(at) as usize, self.u32(at + 4) as usize);
        if page == 0 && off == 0 {
            return None;
        }
        self.pages.get(page).map(|p| p + off).filter(|&a| a < self.d.len())
    }

    pub fn cstr(&self, at: usize) -> &str {
        let s = &self.d[at.min(self.d.len())..];
        let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        std::str::from_utf8(&s[..end]).unwrap_or("")
    }

    /// The name an asset's header points at, `at` bytes in (it differs by type and version).
    pub fn name(&self, a: &Asset, at: usize) -> Option<&str> {
        self.ptr(a.head + at).map(|p| self.cstr(p))
    }

    /// The first asset of type `magic` whose name (at `name_at` in its header) is `name`,
    /// compared without regard to case or slash direction, as the game's lookups are.
    pub fn find(&self, magic: &[u8; 4], name_at: usize, name: &str) -> Option<&Asset> {
        let norm = |s: &str| s.replace('\\', "/").to_ascii_lowercase();
        let want = norm(name);
        self.assets.iter().find(|a| &a.magic == magic && self.name(a, name_at).is_some_and(|n| norm(n) == want))
    }

    /// Reads `size` bytes of streamed data from the starpak an asset's field names.
    pub fn stream(&self, install: &Path, field: i64, size: usize, optional: bool) -> io::Result<Vec<u8>> {
        if field < 0 {
            return Err(bad("asset has no streamed data"));
        }
        let list = if optional { &self.opt_starpaks } else { &self.starpaks };
        let name = list.get((field & 0xFFF) as usize).ok_or_else(|| bad("starpak index out of range"))?;
        let path = install.join(name.replace('\\', "/"));
        let mut f = File::open(&path).map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))?;
        f.seek(SeekFrom::Start((field & !0xFFF) as u64))?;
        let mut out = vec![0; size];
        f.read_exact(&mut out)?;
        Ok(out)
    }
}
