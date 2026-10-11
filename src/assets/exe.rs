//! The install's `r5apex.exe`, read as a file: its build (PE timestamp and checksum, the same
//! pair that names the oracle) and data the asset readers need from it.

use std::io;
use std::path::Path;

/// The oracle's build, `docs/ground-truth.md`.
pub const ORACLE_BUILD: (u32, u32) = (0x5dba_2a2f, 0x02ae_8471);

/// RVA of the RTech pak decoder's lookup table in the oracle build (`docs/assets.md`).
pub const RTECH_LUT_RVA: u32 = 0x131_3180;
pub const RTECH_LUT_LEN: usize = 0x500;

pub struct Exe(Vec<u8>);

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_string())
}

impl Exe {
    pub fn open(install: &Path) -> io::Result<Exe> {
        let d = std::fs::read(install.join("r5apex.exe"))?;
        if d.len() < 0x400 || &d[..2] != b"MZ" {
            return Err(bad("r5apex.exe is not a PE file"));
        }
        Ok(Exe(d))
    }

    fn u32(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.0[at..at + 4].try_into().unwrap())
    }

    fn nt(&self) -> usize {
        self.u32(0x3c) as usize
    }

    /// (TimeDateStamp, CheckSum).
    pub fn build(&self) -> (u32, u32) {
        let nt = self.nt();
        (self.u32(nt + 8), self.u32(nt + 24 + 64))
    }

    /// The file bytes behind `len` bytes at an RVA.
    pub fn at_rva(&self, rva: u32, len: usize) -> io::Result<&[u8]> {
        let nt = self.nt();
        let n = u16::from_le_bytes([self.0[nt + 6], self.0[nt + 7]]) as usize;
        let opt = u16::from_le_bytes([self.0[nt + 20], self.0[nt + 21]]) as usize;
        for i in 0..n {
            let s = nt + 24 + opt + 40 * i;
            let (vsize, va, raw_size, raw) = (self.u32(s + 8), self.u32(s + 12), self.u32(s + 16), self.u32(s + 20));
            if va <= rva && rva + len as u32 <= va + vsize.max(raw_size) {
                let off = (raw + (rva - va)) as usize;
                return self.0.get(off..off + len).ok_or_else(|| bad("RVA past end of file"));
            }
        }
        Err(bad("RVA in no section"))
    }
}
