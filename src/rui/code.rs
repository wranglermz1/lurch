//! `ui.dll` from the install: one compiled function per RUI element, exported by the element's
//! name, which the engine looks up when `ui.rpak` loads (`0x2f3cc0`). The DLL imports nothing,
//! carries no relocations and addresses its own data RIP-relative, so it runs wherever it is
//! mapped. It is mapped here section by section and its functions are called with the Windows
//! x64 convention, on Windows and on x86-64 Linux alike.

use std::collections::HashMap;
use std::io;
use std::path::Path;

/// An element's function: the callback table, the frame's globals, the instance and its data
/// block (the instance plus `0x60`), as `Rui_Draw` (`0x2fc670`) calls it.
pub type ElementFn = unsafe extern "win64" fn(api: *const u8, globals: *const u8, instance: *mut u8, data: *mut u8);

fn bad(what: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.into())
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) mod sys {
    use std::arch::asm;

    pub const READ: usize = 1;
    pub const WRITE: usize = 2;
    pub const EXEC: usize = 4;

    unsafe fn syscall6(n: usize, a: [usize; 6]) -> isize {
        let ret: isize;
        // SAFETY: a raw Linux syscall; the caller passes arguments valid for syscall `n`.
        unsafe {
            asm!("syscall", inlateout("rax") n as isize => ret, in("rdi") a[0], in("rsi") a[1], in("rdx") a[2], in("r10") a[3], in("r8") a[4], in("r9") a[5], lateout("rcx") _, lateout("r11") _, options(nostack));
        }
        ret
    }

    pub fn alloc(len: usize) -> Option<*mut u8> {
        const MAP_PRIVATE_ANONYMOUS: usize = 0x22;
        // SAFETY: mmap of fresh anonymous memory touches nothing that exists.
        let r = unsafe { syscall6(9, [0, len, READ | WRITE, MAP_PRIVATE_ANONYMOUS, usize::MAX, 0]) };
        (!(-4096..0).contains(&r)).then_some(r as *mut u8)
    }

    /// # Safety
    /// `p..p + len` must be pages this module mapped.
    pub unsafe fn protect(p: *mut u8, len: usize, prot: usize) -> bool {
        // SAFETY: as the caller promises.
        unsafe { syscall6(10, [p as usize, len, prot, 0, 0, 0]) == 0 }
    }

    /// # Safety
    /// `p..p + len` must be a mapping from `alloc` that nothing uses any more.
    pub unsafe fn free(p: *mut u8, len: usize) {
        // SAFETY: as the caller promises.
        unsafe {
            syscall6(11, [p as usize, len, 0, 0, 0, 0]);
        }
    }
}

#[cfg(windows)]
pub(crate) mod sys {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn VirtualAlloc(addr: *mut c_void, size: usize, kind: u32, protect: u32) -> *mut c_void;
        fn VirtualProtect(addr: *mut c_void, size: usize, protect: u32, old: *mut u32) -> i32;
        fn VirtualFree(addr: *mut c_void, size: usize, kind: u32) -> i32;
    }

    pub const READ: usize = 1;
    pub const WRITE: usize = 2;
    pub const EXEC: usize = 4;

    pub fn alloc(len: usize) -> Option<*mut u8> {
        // SAFETY: allocates fresh committed pages; no existing memory is touched.
        let p = unsafe { VirtualAlloc(std::ptr::null_mut(), len, 0x3000, 0x04) };
        (!p.is_null()).then_some(p as *mut u8)
    }

    /// # Safety
    /// `p..p + len` must be pages this module allocated.
    pub unsafe fn protect(p: *mut u8, len: usize, prot: usize) -> bool {
        let flag = match prot {
            x if x & EXEC != 0 => 0x20,
            x if x & WRITE != 0 => 0x04,
            _ => 0x02,
        };
        let mut old = 0;
        // SAFETY: as the caller promises.
        unsafe { VirtualProtect(p as *mut c_void, len, flag, &mut old) != 0 }
    }

    /// # Safety
    /// `p` must be an allocation from `alloc` that nothing uses any more.
    pub unsafe fn free(p: *mut u8, _len: usize) {
        // SAFETY: as the caller promises.
        unsafe {
            VirtualFree(p as *mut c_void, 0, 0x8000);
        }
    }
}

/// `ui.dll`, mapped and ready to call.
pub struct Code {
    base: *mut u8,
    size: usize,
    exports: HashMap<String, u32>,
}

// SAFETY: the mapping is immutable once loaded; its functions are reentrant and touch only
// the memory they are given and their own read-only data.
unsafe impl Send for Code {}
unsafe impl Sync for Code {}

impl Drop for Code {
    fn drop(&mut self) {
        // SAFETY: the mapping is ours, and `Code` is dropped only when no function is running.
        unsafe { sys::free(self.base, self.size) }
    }
}

impl Code {
    /// Maps `paks/Win64/ui.dll` of an install.
    pub fn open(install: &Path) -> io::Result<Code> {
        Code::load(&install.join("paks").join("Win64").join("ui.dll"))
    }

    #[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
    pub fn load(path: &Path) -> io::Result<Code> {
        let d = std::fs::read(path)?;
        let u16_at = |o: usize| d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        let u32_at = |o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
        let nt = u32_at(0x3c).ok_or_else(|| bad("not a PE file"))? as usize;
        if d.get(nt..nt + 4) != Some(b"PE\0\0") || u16_at(nt + 4) != Some(0x8664) {
            return Err(bad(format!("{}: not an x86-64 PE file", path.display())));
        }
        let (nsec, optsz) = (u16_at(nt + 6).unwrap() as usize, u16_at(nt + 20).unwrap() as usize);
        let opt = nt + 24;
        let size = u32_at(opt + 56).ok_or_else(|| bad("truncated optional header"))? as usize;
        let headers = u32_at(opt + 60).unwrap() as usize;
        if u32_at(opt + 112 + 8).is_some_and(|n| n != 0) && u32_at(opt + 112 + 12).is_some_and(|n| n != 0) {
            return Err(bad(format!("{}: has imports, which are not resolved", path.display())));
        }
        let base = sys::alloc(size).ok_or_else(|| io::Error::other("could not map ui.dll"))?;
        let mut code = Code { base, size, exports: HashMap::new() };
        // SAFETY: `base..base + size` is our fresh writable mapping; every copy is bounds-checked
        // against both the file and the image size.
        let image = unsafe { std::slice::from_raw_parts_mut(base, size) };
        image[..headers.min(d.len())].copy_from_slice(&d[..headers.min(d.len())]);
        let mut protections = Vec::new();
        for i in 0..nsec {
            let s = opt + optsz + 40 * i;
            let (vsize, va, rsize, raw, flags) = (u32_at(s + 8).unwrap() as usize, u32_at(s + 12).unwrap() as usize, u32_at(s + 16).unwrap() as usize, u32_at(s + 20).unwrap() as usize, u32_at(s + 36).unwrap());
            let n = rsize.min(if vsize == 0 { rsize } else { vsize });
            let (Some(dst), Some(src)) = (image.get_mut(va..va + n), d.get(raw..raw + n)) else {
                return Err(bad(format!("{}: section {i} runs past the image", path.display())));
            };
            dst.copy_from_slice(src);
            let prot = if flags & 0x2000_0000 != 0 { sys::READ | sys::EXEC } else if flags & 0x8000_0000 != 0 { sys::READ | sys::WRITE } else { sys::READ };
            protections.push((va, vsize.max(rsize).next_multiple_of(0x1000), prot));
        }
        let exports = &mut code.exports;
        let (edir, esize) = (u32_at(opt + 112).unwrap_or(0) as usize, u32_at(opt + 116).unwrap_or(0) as usize);
        if esize != 0 {
            let r32 = |o: usize| image.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).unwrap_or(0) as usize;
            let (names, funcs, ords, count) = (r32(edir + 32), r32(edir + 28), r32(edir + 36), r32(edir + 24));
            for i in 0..count {
                let at = r32(names + 4 * i);
                let end = image[at..].iter().position(|&c| c == 0).map_or(at, |e| at + e);
                let ord = image.get(ords + 2 * i..ords + 2 * i + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]])) as usize;
                exports.insert(String::from_utf8_lossy(&image[at..end]).into_owned(), r32(funcs + 4 * ord) as u32);
            }
        }
        for (va, len, prot) in protections {
            // SAFETY: each range is a section inside our mapping.
            if !unsafe { sys::protect(base.add(va), len.min(size - va), prot) } {
                return Err(io::Error::other("could not protect ui.dll's sections"));
            }
        }
        Ok(code)
    }

    #[cfg(not(any(windows, all(target_os = "linux", target_arch = "x86_64"))))]
    pub fn load(path: &Path) -> io::Result<Code> {
        Err(io::Error::new(io::ErrorKind::Unsupported, format!("{}: ui.dll runs on Windows and x86-64 Linux only", path.display())))
    }

    pub fn exports(&self) -> impl Iterator<Item = &str> {
        self.exports.keys().map(String::as_str)
    }

    /// The element function exported as `name`.
    pub fn function(&self, name: &str) -> Option<ElementFn> {
        let rva = *self.exports.get(name)? as usize;
        // SAFETY: the export is a function in our executable mapping with the element signature.
        (rva < self.size).then(|| unsafe { std::mem::transmute::<*mut u8, ElementFn>(self.base.add(rva)) })
    }

    /// The bytes of the mapped image at `rva`, as the code sees them.
    pub fn bytes(&self, rva: usize, len: usize) -> Option<&[u8]> {
        // SAFETY: bounds-checked against the mapping, which lives as long as `self`.
        (rva.checked_add(len)? <= self.size).then(|| unsafe { std::slice::from_raw_parts(self.base.add(rva), len) })
    }
}
