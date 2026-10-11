//! Textures (`txtr` asset version 8) and the materials that bind them (`matl` 15, with its
//! shader set `shds` 11). A texture's mips are split three ways: the smallest in the pak's CPU
//! data, the next in the starpak, the largest in the optional starpak (`*.opt.starpak`, which
//! R5Reloaded does not ship). Each part holds its mips smallest first, each padded to 16 bytes.

use std::io;
use std::path::Path;

use super::rpak::Pak;

fn bad(what: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.into())
}

/// A texture as the GPU takes it.
#[derive(Clone, Debug)]
pub struct Texture {
    /// The pak's format index, which the oracle turns into a DXGI format through the table at
    /// `0x130a760`: 0 and 1 BC1 (1 sRGB), 2 to 5 BC2 and BC3, 6 and 7 BC4, 8 and 9 BC5, 10
    /// and 11 BC6H, 12 and 13 BC7 (13 sRGB). Only these block formats are read.
    pub format: u16,
    /// The size of `mips[0]`.
    pub width: u32,
    pub height: u32,
    /// From the largest mip held down to 1×1, without the padding.
    pub mips: Vec<Vec<u8>>,
}

/// Bytes per 4×4 block of a pak format.
fn block_bytes(format: u16) -> Option<usize> {
    match format {
        0 | 1 | 6 | 7 => Some(8),
        2..=5 | 8..=13 => Some(16),
        _ => None,
    }
}

/// Loads the mips of a texture that are on disk.
pub fn load(pak: &Pak, install: &Path, guid: u64) -> io::Result<Texture> {
    let a = pak.asset(guid).ok_or_else(|| bad(format!("texture {guid:#x} not in {}", pak.path.display())))?;
    if &a.magic != b"txtr" || a.version != 8 {
        return Err(bad(format!("{guid:#x}: {} version {} (txtr 8 expected)", String::from_utf8_lossy(&a.magic), a.version)));
    }
    let h = a.head;
    let (width, height, format) = (pak.u16(h + 0x10) as usize, pak.u16(h + 0x12) as usize, pak.u16(h + 0x16));
    let bpb = block_bytes(format).ok_or_else(|| bad(format!("{guid:#x}: format {format} is not a block format")))?;
    if pak.u8(h + 0x1e) > 1 {
        return Err(bad(format!("{guid:#x}: texture arrays are not read")));
    }
    let (optional, permanent, streamed) = (pak.u8(h + 0x1d) as usize, pak.u8(h + 0x21) as usize, pak.u8(h + 0x22) as usize);
    let size = |mip: usize| (width >> mip).max(1).div_ceil(4) * (height >> mip).max(1).div_ceil(4) * bpb;
    let split = |data: &[u8], mips: std::ops::Range<usize>| -> io::Result<Vec<Vec<u8>>> {
        let mut out = Vec::new();
        let mut at = 0;
        for mip in mips.rev() {
            let part = data.get(at..at + size(mip)).ok_or_else(|| bad(format!("{guid:#x}: mip {mip} runs past its data")))?;
            out.push(part.to_vec());
            at += size(mip).next_multiple_of(16);
        }
        Ok(out)
    };
    let stored = |mips: std::ops::Range<usize>| mips.map(|m| size(m).next_multiple_of(16)).sum::<usize>();
    let total = optional + streamed + permanent;
    let cpu = a.cpu.ok_or_else(|| bad(format!("{guid:#x}: texture without data")))?;
    let mut mips = split(&pak.data()[cpu..], total - permanent..total)?;
    let mut top = total - permanent;
    let parts = [(a.starpak, false, optional..top), (a.opt_starpak, true, 0..optional)];
    for (field, opt, range) in parts {
        if range.is_empty() || top != range.end {
            continue;
        }
        let Ok(data) = pak.stream(install, field, stored(range.clone()), opt) else { break };
        mips.extend(split(&data, range.clone())?);
        top = range.start;
    }
    mips.reverse();
    Ok(Texture { format, width: (width >> top).max(1) as u32, height: (height >> top).max(1) as u32, mips })
}

/// A material's textures in its shader's binding order. The arms' skinned shader binds
/// `albedoTexture`, `normalTexture`, `glossTexture`, `specTexture`, `emissiveTexture`,
/// `aoTexture`, `cavityTexture` (its pixel shader's resource names).
pub fn material(pak: &Pak, guid: u64) -> io::Result<Vec<u64>> {
    let a = pak.asset(guid).ok_or_else(|| bad(format!("material {guid:#x} not in {}", pak.path.display())))?;
    if &a.magic != b"matl" || a.version != 15 {
        return Err(bad(format!("{guid:#x}: {} version {} (matl 15 expected)", String::from_utf8_lossy(&a.magic), a.version)));
    }
    let shader = pak.asset(pak.u64(a.head + 0x58)).filter(|s| &s.magic == b"shds").ok_or_else(|| bad(format!("{guid:#x}: no shader set")))?;
    let n = pak.u16(shader.head + 0x18) as usize;
    let Some(table) = pak.ptr(a.head + 0x60) else { return Ok(Vec::new()) };
    Ok((0..n).map(|i| pak.u64(table + 8 * i)).collect())
}
