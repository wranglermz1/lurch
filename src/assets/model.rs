//! Models (`mdl_` asset version 10, studio version 54), their animation rigs (`arig` 4) and
//! sequences (`aseq` 7), and their vertex data (`0tVG` version 1, streamed). Structures keep
//! the game's bytes and read fields at the game's offsets, so the animation code can follow
//! the oracle's own reads.

use std::io;
use std::path::Path;

use super::rpak::{Asset, Pak};

fn bad(what: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.into())
}

/// A copy of one structure from a pak, with offsets relative to its start.
#[derive(Clone, Debug, Default)]
pub struct Blob(pub Vec<u8>);

impl Blob {
    pub fn u8(&self, at: usize) -> u8 {
        self.0[at]
    }
    pub fn u16(&self, at: usize) -> u16 {
        u16::from_le_bytes(self.0[at..at + 2].try_into().unwrap())
    }
    pub fn i16(&self, at: usize) -> i16 {
        self.u16(at) as i16
    }
    pub fn i32(&self, at: usize) -> i32 {
        i32::from_le_bytes(self.0[at..at + 4].try_into().unwrap())
    }
    pub fn u32(&self, at: usize) -> u32 {
        self.i32(at) as u32
    }
    pub fn u64(&self, at: usize) -> u64 {
        u64::from_le_bytes(self.0[at..at + 8].try_into().unwrap())
    }
    pub fn i64(&self, at: usize) -> i64 {
        self.u64(at) as i64
    }
    pub fn f32(&self, at: usize) -> f32 {
        f32::from_bits(self.u32(at))
    }
    pub fn v3(&self, at: usize) -> [f32; 3] {
        [self.f32(at), self.f32(at + 4), self.f32(at + 8)]
    }
    pub fn cstr(&self, at: usize) -> &str {
        let s = &self.0[at.min(self.0.len())..];
        let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        std::str::from_utf8(&s[..end]).unwrap_or("")
    }
    /// An `int` offset field that counts from `base`: the string or array it points at.
    pub fn rel(&self, base: usize, field: usize) -> usize {
        (base as i64 + self.i32(base + field) as i64) as usize
    }
}

/// studiohdr_t, rmdl version 54 as model asset version 10 lays it out.
pub mod hdr {
    pub const LENGTH: usize = 0x50;
    pub const FLAGS: usize = 0x9c;
    pub const NUMBONES: usize = 0xa0;
    pub const BONEINDEX: usize = 0xa4;
    pub const NUMLOCALSEQ: usize = 0xc0;
    pub const LOCALSEQINDEX: usize = 0xc4;
    pub const NUMTEXTURES: usize = 0xd0;
    pub const TEXTUREINDEX: usize = 0xd4;
    pub const NUMSKINREF: usize = 0xe0;
    pub const SKININDEX: usize = 0xe8;
    pub const NUMBODYPARTS: usize = 0xec;
    pub const BODYPARTINDEX: usize = 0xf0;
    pub const NUMLOCALATTACHMENTS: usize = 0xf4;
    pub const LOCALATTACHMENTINDEX: usize = 0xf8;
    pub const NUMLOCALPOSEPARAMETERS: usize = 0x134;
    pub const LOCALPOSEPARAMINDEX: usize = 0x138;
    pub const LINEARBONEINDEX: usize = 0x194;
    pub const BONE_SIZE: usize = 0xb8;
    /// STUDIOHDR_FLAGS_USES_EXTRA_BONE_WEIGHTS: vertices index more than three bones.
    pub const EXTRA_WEIGHTS: u32 = 0x4000;
}

#[derive(Clone, Debug)]
pub struct Bone {
    pub name: String,
    pub parent: i32,
    pub pos: [f32; 3],
    pub quat: [f32; 4],
    pub rot: [f32; 3],
    pub scale: [f32; 3],
    pub pose_to_bone: [[f32; 4]; 3],
    pub flags: u32,
    pub proc_type: u32,
}

#[derive(Clone, Debug)]
pub struct PoseParam {
    pub name: String,
    pub flags: i32,
    pub start: f32,
    pub end: f32,
    pub looping: f32,
}

#[derive(Clone, Debug)]
pub struct Attachment {
    pub name: String,
    pub flags: i32,
    pub bone: i32,
    pub local: [[f32; 4]; 3],
}

/// A studio header (a model's or a rig's): bones and the rest, read from the game's bytes.
#[derive(Clone, Debug, Default)]
pub struct Studio(pub Blob);

impl Studio {
    pub fn bones(&self) -> Vec<Bone> {
        let b = &self.0;
        (0..b.i32(hdr::NUMBONES) as usize)
            .map(|i| {
                let o = b.i32(hdr::BONEINDEX) as usize + i * hdr::BONE_SIZE;
                let m = |r: usize| [b.f32(o + 0x54 + r * 16), b.f32(o + 0x58 + r * 16), b.f32(o + 0x5c + r * 16), b.f32(o + 0x60 + r * 16)];
                Bone {
                    name: b.cstr(b.rel(o, 0)).to_string(),
                    parent: b.i32(o + 4),
                    pos: b.v3(o + 0x20),
                    quat: [b.f32(o + 0x2c), b.f32(o + 0x30), b.f32(o + 0x34), b.f32(o + 0x38)],
                    rot: b.v3(o + 0x3c),
                    scale: b.v3(o + 0x48),
                    pose_to_bone: [m(0), m(1), m(2)],
                    flags: b.u32(o + 0x94),
                    proc_type: b.u32(o + 0x98),
                }
            })
            .collect()
    }

    pub fn pose_params(&self) -> Vec<PoseParam> {
        let b = &self.0;
        (0..b.i32(hdr::NUMLOCALPOSEPARAMETERS) as usize)
            .map(|i| {
                let o = b.i32(hdr::LOCALPOSEPARAMINDEX) as usize + i * 20;
                PoseParam { name: b.cstr(b.rel(o, 0)).to_string(), flags: b.i32(o + 4), start: b.f32(o + 8), end: b.f32(o + 12), looping: b.f32(o + 16) }
            })
            .collect()
    }

    pub fn attachments(&self) -> Vec<Attachment> {
        let b = &self.0;
        (0..b.i32(hdr::NUMLOCALATTACHMENTS) as usize)
            .map(|i| {
                let o = b.i32(hdr::LOCALATTACHMENTINDEX) as usize + i * 0x3c;
                let m = |r: usize| [b.f32(o + 12 + r * 16), b.f32(o + 16 + r * 16), b.f32(o + 20 + r * 16), b.f32(o + 24 + r * 16)];
                Attachment { name: b.cstr(b.rel(o, 0)).to_string(), flags: b.i32(o + 4), bone: b.i32(o + 8), local: [m(0), m(1), m(2)] }
            })
            .collect()
    }

    /// The material each texture slot names: (path, asset GUID).
    pub fn textures(&self) -> Vec<(String, u64)> {
        let b = &self.0;
        (0..b.i32(hdr::NUMTEXTURES) as usize)
            .map(|i| {
                let o = b.i32(hdr::TEXTUREINDEX) as usize + i * 12;
                (b.cstr(b.rel(o, 0)).to_string(), b.u64(o + 4))
            })
            .collect()
    }

    /// The texture slot a mesh's material reference draws with in skin family 0.
    pub fn skin(&self, slot: i32) -> i32 {
        let b = &self.0;
        if !(0..b.i32(hdr::NUMSKINREF)).contains(&slot) {
            return slot;
        }
        b.i16(b.i32(hdr::SKININDEX) as usize + 2 * slot as usize) as i32
    }

    /// For each mesh of each model of each body part, in order: (material slot, mesh id, body
    /// part, model in the part).
    pub fn meshes(&self) -> Vec<(i32, i32, usize, usize)> {
        let b = &self.0;
        let mut out = Vec::new();
        for p in 0..b.i32(hdr::NUMBODYPARTS) as usize {
            let bp = b.i32(hdr::BODYPARTINDEX) as usize + p * 16;
            for m in 0..b.i32(bp + 4) as usize {
                let model = b.rel(bp, 12) + m * 0x88;
                for k in 0..b.i32(model + 0x4c) as usize {
                    let mesh = b.rel(model, 0x50) + k * 0x5c;
                    out.push((b.i32(mesh), b.i32(mesh + 0x20), p, m));
                }
            }
        }
        out
    }

    /// The body parts (bodygroups) by name, as FindBodygroupByName matches them.
    pub fn bodyparts(&self) -> Vec<String> {
        let b = &self.0;
        (0..b.i32(hdr::NUMBODYPARTS) as usize).map(|p| b.cstr(b.rel(b.i32(hdr::BODYPARTINDEX) as usize + p * 16, 0)).to_string()).collect()
    }
}

/// One sequence (`aseq`): its seqdesc and everything after it in its page.
#[derive(Clone, Debug)]
pub struct Seq {
    pub name: String,
    pub desc: Blob,
    /// The `aseq` asset's GUID, by which auto-layers name their sequence.
    pub guid: u64,
}

/// An animation rig (`arig`): a skeleton the sequences are authored against, and the sequences.
#[derive(Clone, Debug)]
pub struct Rig {
    pub name: String,
    pub studio: Studio,
    pub seqs: Vec<Seq>,
}

/// One drawn mesh of the highest level of detail.
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub material: i32,
    /// The body part it belongs to and which of the part's models (bodygroup value) it draws in.
    pub part: usize,
    pub model: usize,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// RGBA, when the mesh's vertices carry a colour.
    pub colors: Vec<[u8; 4]>,
    /// Up to `MAX_WEIGHTS` (bone, weight) pairs per vertex.
    pub weights: Vec<Vec<(u16, f32)>>,
    pub indices: Vec<u16>,
}

#[derive(Clone, Debug)]
pub struct Model {
    pub name: String,
    pub studio: Studio,
    pub rigs: Vec<Rig>,
    pub meshes: Vec<Mesh>,
}

/// A copy of the structure at `at` running to the end of its memory page.
fn page_blob(pak: &Pak, at: usize, len: Option<usize>) -> Blob {
    let end = len.map_or_else(|| pak.page_end(at), |l| at + l);
    Blob(pak.data()[at..end.min(pak.data().len())].to_vec())
}

fn seq(pak: &Pak, a: &Asset) -> io::Result<Seq> {
    if a.version != 7 || a.head_size != 48 {
        return Err(bad(format!("aseq version {} size {} (7, 48 expected)", a.version, a.head_size)));
    }
    let d = pak.ptr(a.head).ok_or_else(|| bad("aseq without data"))?;
    Ok(Seq { name: pak.name(a, 8).unwrap_or("").to_string(), desc: page_blob(pak, d, None), guid: a.guid })
}

fn rig(pak: &Pak, a: &Asset) -> io::Result<Rig> {
    if a.version != 4 {
        return Err(bad(format!("arig version {} (4 expected)", a.version)));
    }
    let data = pak.ptr(a.head).ok_or_else(|| bad("arig without data"))?;
    let studio = Studio(page_blob(pak, data, Some(pak.i32(data + hdr::LENGTH) as usize)));
    let (n, table) = (pak.i32(a.head + 0x14) as usize, pak.ptr(a.head + 0x18));
    let mut seqs = Vec::with_capacity(n);
    for i in 0..n {
        let guid = pak.u64(table.ok_or_else(|| bad("arig without a sequence table"))? + 8 * i);
        let s = pak.asset(guid).ok_or_else(|| bad(format!("sequence {guid:#x} not in {}", pak.path.display())))?;
        seqs.push(seq(pak, s)?);
    }
    Ok(Rig { name: pak.name(a, 8).unwrap_or("").to_string(), studio, seqs })
}

/// Loads a model by path (`mdl/...rmdl`) with its rigs, sequences and level-0 meshes.
pub fn load(pak: &Pak, install: &Path, name: &str) -> io::Result<Model> {
    let a = pak.find(b"mdl_", 0x10, name).ok_or_else(|| bad(format!("{name} not in {}", pak.path.display())))?;
    if a.version != 10 {
        return Err(bad(format!("{name}: model version {} (10 expected)", a.version)));
    }
    let data = pak.ptr(a.head).ok_or_else(|| bad("model without data"))?;
    let studio = Studio(page_blob(pak, data, Some(pak.i32(data + hdr::LENGTH) as usize)));
    let mut rigs = Vec::new();
    if let Some(t) = pak.ptr(a.head + 0x38) {
        for i in 0..pak.u32(a.head + 0x40) as usize {
            let guid = pak.u64(t + 8 * i);
            let r = pak.asset(guid).ok_or_else(|| bad(format!("{name}: rig {guid:#x} not in pak")))?;
            rigs.push(rig(pak, r)?);
        }
    }
    let size = pak.u32(a.head + 0x48) as usize;
    let vg = if let Some(p) = pak.ptr(a.head + 0x30) {
        pak.data()[p..p + size].to_vec()
    } else {
        pak.stream(install, a.starpak, size, false)?
    };
    let meshes = vg_meshes(&studio, &Blob(vg))?;
    Ok(Model { name: name.to_string(), studio, rigs, meshes })
}

pub const MAX_WEIGHTS: usize = 16;

/// Level-0 meshes from a `0tVG` version 1 vertex group, in studio mesh order.
fn vg_meshes(studio: &Studio, vg: &Blob) -> io::Result<Vec<Mesh>> {
    if vg.0.len() < 0xd0 || &vg.0[..4] != b"0tVG" || vg.i32(4) != 1 {
        return Err(bad("vertex data is not 0tVG version 1"));
    }
    let extra = studio.0.u32(hdr::FLAGS) & hdr::EXTRA_WEIGHTS != 0;
    let bone_map = vg.i64(0x10) as usize;
    let nmap = vg.i64(0x18) as usize;
    let mesh_at = vg.i64(0x20) as usize;
    let index_at = vg.i64(0x30) as usize;
    let vert_at = vg.i64(0x40) as usize;
    let weight_at = vg.i64(0x50) as usize;
    let lod_at = vg.i64(0x70) as usize;
    if vg.i64(0x78) < 1 {
        return Err(bad("vertex data has no levels of detail"));
    }
    let lod_first = vg.u16(lod_at) as usize;
    let map = |b: u8| -> u16 { if nmap > 0 { vg.u8(bone_map + b as usize) as u16 } else { b as u16 } };
    let mut out = Vec::new();
    for (material, id, part, model) in studio.meshes() {
        let mh = mesh_at + (lod_first + id as usize) * 0x48;
        let flags = vg.u64(mh);
        if flags == 0 {
            continue;
        }
        let (voff, vsize, vcount) = (vg.u32(mh + 8) as usize, vg.u32(mh + 12) as usize, vg.u32(mh + 16) as usize);
        let wextra = weight_at + vg.i32(mh + 0x18) as usize;
        let (ioff, icount) = (vg.i32(mh + 0x20) as usize, vg.i32(mh + 0x24) as usize);
        let mut m = Mesh { material, part, model, ..Mesh::default() };
        for v in 0..vcount {
            let mut o = vert_at + voff + v * vsize;
            match flags & 3 {
                1 => {
                    m.positions.push(vg.v3(o));
                    o += 12;
                }
                2 => {
                    let p = vg.u64(o);
                    let f = |bits: u64| bits as f32 * 0.0009765625;
                    m.positions.push([f(p & 0x1F_FFFF) - 1024.0, f((p >> 21) & 0x1F_FFFF) - 1024.0, f(p >> 42) - 2048.0]);
                    o += 8;
                }
                _ => return Err(bad("unsupported vertex position format")),
            }
            let mut w = Vec::with_capacity(4);
            if flags & (0x1000 | 0x4000) != 0 {
                let (w0, w1) = (vg.u16(o), vg.u16(o + 2));
                let unpack = |x: u16| (x as f32 + 1.0) / 32768.0;
                let mut left: u16 = 32767;
                if extra {
                    let idx = vg.u32(o + 4);
                    let (first, last, count) = (idx as u8, (idx >> 8) as u8, (idx >> 24) as u8 as usize);
                    w.push((map(first), unpack(w0)));
                    left = left.wrapping_sub(w0);
                    for k in 1..count {
                        let e = wextra + (w1 as usize + k - 1) * 4;
                        let (ew, eb) = (vg.u16(e), vg.u16(e + 2));
                        w.push((map(eb as u8), unpack(ew)));
                        left = left.wrapping_sub(ew);
                    }
                    if count > 0 {
                        w.push((map(last), unpack(left)));
                    }
                } else {
                    let count = vg.u8(o + 7) as usize;
                    for k in 0..count {
                        let wk = if k == 0 { w0 } else { w1 };
                        w.push((map(vg.u8(o + 4 + k)), unpack(wk)));
                        left = left.wrapping_sub(wk);
                    }
                    w.push((map(vg.u8(o + 4 + count)), unpack(left)));
                }
                o += 8;
            } else {
                w.push((0, 1.0));
            }
            m.weights.push(w);
            m.normals.push(normal32(vg.u32(o)));
            o += 4;
            if flags & 0x10 != 0 {
                m.colors.push(vg.u32(o).to_le_bytes());
                o += 4;
            }
            if flags & (0xF << 24) != 0 {
                m.uvs.push([vg.f32(o), vg.f32(o + 4)]);
            } else {
                m.uvs.push([0.0, 0.0]);
            }
        }
        m.indices = (0..icount).map(|k| vg.u16(index_at + (ioff + k) * 2)).collect();
        out.push(m);
    }
    Ok(out)
}

/// The packed normal: the dominant axis by index, its sign, and two 9-bit components.
fn normal32(p: u32) -> [f32; 3] {
    let v1 = if (p >> 28) & 1 != 0 { -255.0 } else { 255.0 };
    let v2 = ((p >> 19) & 0x1FF) as f32 - 256.0;
    let v3 = ((p >> 10) & 0x1FF) as f32 - 256.0;
    let i1 = ((p >> 29) & 3) as usize;
    let i2 = ((0x124u32 >> (2 * i1 + 2)) & 3) as usize;
    let i3 = ((0x124u32 >> (2 * i1 + 4)) & 3) as usize;
    let r = 1.0 / (255.0f32 * 255.0 + v2 * v2 + v3 * v3).sqrt();
    let mut n = [0.0; 3];
    n[i1] = v1 * r;
    n[i2] = v2 * r;
    n[i3] = v3 * r;
    n
}
