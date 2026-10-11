//! The Miles Sound System the game ships (`mileswin64.dll`, `binkawin64.dll` in the install),
//! loaded at run time and set up the way the oracle's CSOM_Initialize (0x95a140) sets it up:
//! the driver on the default output, Bink Audio registered, the project `audio\ship\audio.mprj`,
//! each bank with its streams and patches, one command queue. Sound names resolve the way
//! CSOM_AddEventToQueue (0x95b480) resolves them: FNV-1 64 over the name lowercased with `.`
//! read as `_`, against every event and source the banks list (table built in 0x9afb8b).
//!
//! Windows only: elsewhere `Miles::open` fails and the game is silent.

use std::collections::HashMap;
use std::path::Path;

/// The name hash of 0x95b486 and 0x9af740.
pub fn name_hash(name: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in name.as_bytes() {
        let c = match b {
            b'A'..=b'Z' => b + 0x20,
            b'.' => b'_',
            _ => b,
        };
        h = h.wrapping_mul(0x0000_0100_0000_01b3) ^ c as u64;
    }
    h
}

/// The banks `scripts/audio/banks.rson` lists in this install.
pub const BANKS: &[&str] = &["general"];
pub const LANGUAGE: &str = "english";
/// `MilesEventSetStreamingCacheLimit` as the oracle sets it.
pub const STREAMING_CACHE: u64 = 0x400_0000;
/// ConVars `miles_channels` and `miles_samplerate` (image values), which 0x9587e0 passes to
/// MilesDriverCreate when no `-channels` or `-rate` is given.
pub const MILES_CHANNELS: u32 = 2;
pub const MILES_SAMPLERATE: u32 = 48000;

#[cfg(windows)]
mod sys {
    use std::ffi::{c_char, c_void, CString};
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryExW(name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
        fn SetDllDirectoryW(path: *const u16) -> i32;
    }

    const LOAD_WITH_ALTERED_SEARCH_PATH: u32 = 0x8;

    pub fn wide(p: &Path) -> Vec<u16> {
        p.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    pub struct Lib(*mut c_void);
    unsafe impl Send for Lib {}
    unsafe impl Sync for Lib {}

    impl Lib {
        pub fn open(dir: &Path, name: &str) -> Result<Lib, String> {
            let w = wide(dir);
            // SAFETY: both strings are NUL-terminated wide strings that outlive the calls.
            unsafe {
                SetDllDirectoryW(w.as_ptr());
                let h = LoadLibraryExW(wide(&dir.join(name)).as_ptr(), std::ptr::null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH);
                if h.is_null() {
                    return Err(format!("could not load {}", dir.join(name).display()));
                }
                Ok(Lib(h))
            }
        }

        pub fn sym<T: Copy>(&self, name: &str) -> Result<T, String> {
            let c = CString::new(name).unwrap();
            // SAFETY: the module handle is live; T is the export's function pointer type.
            let p = unsafe { GetProcAddress(self.0, c.as_ptr()) };
            if p.is_null() {
                return Err(format!("missing export {name}"));
            }
            assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<*mut c_void>());
            // SAFETY: a non-null export address read as a function pointer of the same size.
            Ok(unsafe { std::mem::transmute_copy(&p) })
        }
    }
}

#[cfg(windows)]
pub use imp::Miles;

#[cfg(windows)]
mod imp {
    use super::sys::Lib;
    use super::*;
    use std::ffi::{c_char, c_void, CStr, CString};

    type H = *mut c_void;

    /// The driver configuration MilesDriverCreate takes (0x9587e0 fills it): the output
    /// provider (none: Miles' default), DirectSound parameters, channels and rate.
    #[repr(C)]
    struct DriverConfig {
        output: usize,
        dsound: usize,
        channels: u32,
        rate: u32,
        rest: [u64; 3],
    }

    #[allow(non_snake_case)]
    struct Api {
        MilesSetStartupParameters: unsafe extern "C" fn(*const [u16; 2]),
        MilesStartup: unsafe extern "C" fn(unsafe extern "C" fn(i64, *const c_char)) -> i32,
        MilesDriverCreate: unsafe extern "C" fn(*const DriverConfig) -> H,
        MilesDriverSetMasterVolume: unsafe extern "C" fn(H, f32),
        MilesEventSetStreamingCacheLimit: unsafe extern "C" fn(H, u64),
        MilesProjectLoad: unsafe extern "C" fn(H, *const c_char, *const c_char, *const c_char) -> i32,
        MilesProjectGetStatus: unsafe extern "C" fn(H) -> i32,
        MilesBankLoad: unsafe extern "C" fn(H, *const c_char, *const c_char, *const c_char, *const c_char) -> H,
        MilesBankGetStatus: unsafe extern "C" fn(H, *mut i32) -> i32,
        MilesBankPatch: unsafe extern "C" fn(H, *const c_char, *const c_char) -> i32,
        MilesBankGetEventCount: unsafe extern "C" fn(H) -> u32,
        MilesBankGetEventTemplateId: unsafe extern "C" fn(H, u32, *mut u64) -> i32,
        MilesBankGetSourceCount: unsafe extern "C" fn(H) -> u32,
        MilesBankGetSourceTemplateId: unsafe extern "C" fn(H, u32, *mut u64) -> i32,
        /// The install's Miles (10.0.57) has a seventh parameter, a release list, that the
        /// engine (built against 10.0.42) never passes; it must be null (R5Reloaded's shim).
        MilesEventGetDetails: unsafe extern "C" fn(H, *const u64, *mut *const c_char, *mut u64, *mut u64, *mut u64, *mut c_void) -> i32,
        MilesQueueCreate: unsafe extern "C" fn(H) -> H,
        MilesQueueSubmit: unsafe extern "C" fn(H) -> i32,
        MilesQueueListener3DPosition: unsafe extern "C" fn(H, i32, f32, f32, f32),
        MilesQueueListener3DOrientation: unsafe extern "C" fn(H, i32, f32, f32, f32, f32, f32, f32),
        MilesQueueEvent3DPosition: unsafe extern "C" fn(H, f32, f32, f32),
        MilesQueueEventVolume: unsafe extern "C" fn(H, f32),
        MilesQueueEventRunByTemplateId: unsafe extern "C" fn(H, *const u64) -> u64,
        MilesQueueEventFilterId: unsafe extern "C" fn(H, u64),
        MilesQueueControllerValue: unsafe extern "C" fn(H, *const c_char, f32),
        MilesQueueEventRun: unsafe extern "C" fn(H, *const c_char) -> u64,
        MilesServiceDrivers: unsafe extern "C" fn() -> i32,
        MilesDriverRegisterBinkAudio: unsafe extern "C" fn(H) -> i32,
    }

    pub struct Miles {
        api: Api,
        driver: H,
        queue: H,
        _libs: (Lib, Lib),
        /// Event and source template ids by name hash, and the names.
        pub events: HashMap<u64, (u64, String)>,
    }

    unsafe impl Send for Miles {}
    unsafe impl Sync for Miles {}

    unsafe extern "C" fn log(level: i64, msg: *const c_char) {
        if !msg.is_null() {
            // SAFETY: Miles passes a NUL-terminated message.
            eprintln!("miles[{level}]: {}", unsafe { CStr::from_ptr(msg) }.to_string_lossy());
        }
    }

    fn trace(what: &str) {
        if std::env::var_os("LURCH_MILES_TRACE").is_some() {
            eprintln!("miles: {what}");
        }
    }

    fn c(s: &str) -> CString {
        CString::new(s).unwrap()
    }

    fn path(p: &Path) -> CString {
        c(&p.to_string_lossy())
    }

    impl Miles {
        pub fn open(install: &Path) -> Result<Miles, String> {
            let miles = Lib::open(install, "mileswin64.dll")?;
            let binka = Lib::open(install, "binkawin64.dll")?;
            macro_rules! api {
                ($($n:ident),*) => { Api { $($n: if stringify!($n) == "MilesDriverRegisterBinkAudio" { binka.sym(stringify!($n))? } else { miles.sym(stringify!($n))? },)* } };
            }
            let api = api!(
                MilesSetStartupParameters, MilesStartup, MilesDriverCreate, MilesDriverSetMasterVolume,
                MilesEventSetStreamingCacheLimit, MilesProjectLoad, MilesProjectGetStatus, MilesBankLoad,
                MilesBankGetStatus, MilesBankPatch, MilesBankGetEventCount, MilesBankGetEventTemplateId,
                MilesBankGetSourceCount, MilesBankGetSourceTemplateId, MilesEventGetDetails, MilesQueueCreate,
                MilesQueueSubmit, MilesQueueListener3DPosition, MilesQueueListener3DOrientation,
                MilesQueueEvent3DPosition, MilesQueueEventVolume, MilesQueueEventRunByTemplateId,
                MilesQueueEventFilterId, MilesQueueControllerValue, MilesQueueEventRun, MilesServiceDrivers,
                MilesDriverRegisterBinkAudio
            );
            let ship = install.join("audio").join("ship");
            // SAFETY: each call follows the oracle's own sequence and argument types; strings
            // are NUL-terminated and live across the calls that read them.
            unsafe {
                (api.MilesSetStartupParameters)(&[1, 0]);
                trace("startup");
                if (api.MilesStartup)(log) == 0 {
                    return Err("MilesStartup failed".into());
                }
                trace("driver");
                let cfg = DriverConfig { output: 0, dsound: 0, channels: super::MILES_CHANNELS, rate: super::MILES_SAMPLERATE, rest: [0; 3] };
                let driver = (api.MilesDriverCreate)(&cfg);
                if driver.is_null() {
                    return Err("MilesDriverCreate failed".into());
                }
                (api.MilesDriverRegisterBinkAudio)(driver);
                (api.MilesEventSetStreamingCacheLimit)(driver, STREAMING_CACHE);
                trace("project");
                let mprj = path(&ship.join("audio.mprj"));
                let (lang, name) = (c(LANGUAGE), c("audio"));
                if (api.MilesProjectLoad)(driver, mprj.as_ptr(), lang.as_ptr(), name.as_ptr()) == 0 {
                    return Err("MilesProjectLoad failed".into());
                }
                let mut status = (api.MilesProjectGetStatus)(driver);
                while status == 0 {
                    if (api.MilesServiceDrivers)() == 0 {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                    status = (api.MilesProjectGetStatus)(driver);
                }
                if status != 1 {
                    return Err(format!("MilesProjectGetStatus {status}"));
                }
                trace("banks");
                let mut banks = Vec::new();
                for b in BANKS {
                    let mbnk = path(&ship.join(format!("{b}.mbnk")));
                    let stream = path(&ship.join(format!("{b}_stream.mstr")));
                    let loc = path(&ship.join(format!("{b}_{LANGUAGE}.mstr")));
                    let bname = c(b);
                    let bank = (api.MilesBankLoad)(driver, mbnk.as_ptr(), stream.as_ptr(), loc.as_ptr(), bname.as_ptr());
                    if bank.is_null() {
                        return Err(format!("MilesBankLoad {b} failed"));
                    }
                    let mut patches = -1;
                    while patches == -1 {
                        if (api.MilesBankGetStatus)(bank, &mut patches) < 0 {
                            return Err(format!("MilesBankGetStatus {b}"));
                        }
                        if patches == -1 && (api.MilesServiceDrivers)() == 0 {
                            std::thread::sleep(std::time::Duration::from_millis(1));
                        }
                    }
                    for i in 1..=patches {
                        let sp = path(&ship.join(format!("{b}_stream_patch_{i}.mstr")));
                        let lp = path(&ship.join(format!("{b}_{LANGUAGE}_patch_{i}.mstr")));
                        if (api.MilesBankPatch)(bank, sp.as_ptr(), lp.as_ptr()) == 0 {
                            return Err(format!("MilesBankPatch {b} {i}"));
                        }
                    }
                    let mut p = 0;
                    let mut st = (api.MilesBankGetStatus)(bank, &mut p);
                    while st == 0 {
                        if (api.MilesServiceDrivers)() == 0 {
                            std::thread::sleep(std::time::Duration::from_millis(1));
                        }
                        st = (api.MilesBankGetStatus)(bank, &mut p);
                    }
                    if st != 1 {
                        return Err(format!("MilesBankGetStatus {b}: {st}"));
                    }
                    trace(&format!("bank {b}: {patches} patches, {} sources, {} events", (api.MilesBankGetSourceCount)(bank), (api.MilesBankGetEventCount)(bank)));
                    banks.push(bank);
                }
                let mut events = HashMap::new();
                for &bank in &banks {
                    let lists: [(u32, unsafe extern "C" fn(H, u32, *mut u64) -> i32); 2] = [
                        ((api.MilesBankGetSourceCount)(bank), api.MilesBankGetSourceTemplateId),
                        ((api.MilesBankGetEventCount)(bank), api.MilesBankGetEventTemplateId),
                    ];
                    for (k, (n, get)) in lists.into_iter().enumerate() {
                        trace(&format!("list {k}: {n}"));
                        for i in 0..n {
                            let mut id = 0u64;
                            if get(bank, i, &mut id) == 0 {
                                continue;
                            }
                            let (mut name, mut a, mut b2, mut cc) = (std::ptr::null(), 0u64, 0u64, 0u64);
                            if (api.MilesEventGetDetails)(driver, &id, &mut name, &mut a, &mut b2, &mut cc, std::ptr::null_mut()) == 0 || name.is_null() {
                                continue;
                            }
                            let s = CStr::from_ptr(name).to_string_lossy().into_owned();
                            events.entry(name_hash(&s)).or_insert((id, s));
                        }
                    }
                }
                trace("queue");
                let queue = (api.MilesQueueCreate)(driver);
                if queue.is_null() {
                    return Err("MilesQueueCreate failed".into());
                }
                Ok(Miles { api, driver, queue, _libs: (miles, binka), events })
            }
        }

        /// The service the oracle's sound thread runs (0x959500): returns whether Miles has
        /// more to do at once.
        pub fn service(&self) -> bool {
            // SAFETY: Miles is started.
            unsafe { (self.api.MilesServiceDrivers)() != 0 }
        }

        pub fn set_master_volume(&self, v: f32) {
            // SAFETY: the driver is live.
            unsafe { (self.api.MilesDriverSetMasterVolume)(self.driver, v) }
        }

        /// MilesQueueListener3DPosition and Orientation for listener 0 (0x956720).
        pub fn listener(&self, origin: [f32; 3], forward: [f32; 3], up: [f32; 3]) {
            // SAFETY: the queue is live.
            unsafe {
                (self.api.MilesQueueListener3DPosition)(self.queue, 0, origin[0], origin[1], origin[2]);
                (self.api.MilesQueueListener3DOrientation)(self.queue, 0, forward[0], forward[1], forward[2], up[0], up[1], up[2]);
            }
        }

        /// MilesQueueControllerValue: a global controller, as 0x956b50 sends one that changed.
        pub fn controller(&self, name: &'static CStr, value: f32) {
            // SAFETY: the queue is live; the name outlives the queue.
            unsafe { (self.api.MilesQueueControllerValue)(self.queue, name.as_ptr(), value) }
        }

        /// Queues an event by its sound name at a position; false when no event has the name.
        pub fn play(&self, name: &str, at: [f32; 3], volume: f32) -> Option<u64> {
            let &(id, _) = self.events.get(&name_hash(name))?;
            // SAFETY: the queue is live; the id came from the banks.
            unsafe {
                (self.api.MilesQueueEvent3DPosition)(self.queue, at[0], at[1], at[2]);
                if volume != 1.0 {
                    (self.api.MilesQueueEventVolume)(self.queue, volume);
                }
                Some((self.api.MilesQueueEventRunByTemplateId)(self.queue, &id))
            }
        }

        /// Stops an event instance: its filter id, then the `STOP` event (0x954c60).
        pub fn stop(&self, instance: u64) {
            let stop = c("STOP");
            // SAFETY: the queue is live.
            unsafe {
                (self.api.MilesQueueEventFilterId)(self.queue, instance);
                (self.api.MilesQueueEventRun)(self.queue, stop.as_ptr());
            }
        }

        pub fn submit(&self) {
            // SAFETY: the queue is live.
            unsafe {
                (self.api.MilesQueueSubmit)(self.queue);
            }
        }
    }
}

#[cfg(not(windows))]
pub struct Miles {
    pub events: HashMap<u64, (u64, String)>,
}

#[cfg(not(windows))]
impl Miles {
    pub fn open(_: &Path) -> Result<Miles, String> {
        Err("the game's Miles libraries run on Windows only".into())
    }
    pub fn service(&self) -> bool {
        false
    }
    pub fn set_master_volume(&self, _: f32) {}
    pub fn listener(&self, _: [f32; 3], _: [f32; 3], _: [f32; 3]) {}
    pub fn controller(&self, _: &'static std::ffi::CStr, _: f32) {}
    pub fn play(&self, _: &str, _: [f32; 3], _: f32) -> Option<u64> {
        None
    }
    pub fn stop(&self, _: u64) {}
    pub fn submit(&self) {}
}

#[cfg(test)]
mod tests {
    #[test]
    fn hash_reads_dots_as_underscores_and_ignores_case() {
        assert_eq!(super::name_hash("Wraith.Sprint.Concrete"), super::name_hash("wraith_sprint_concrete"));
        assert_ne!(super::name_hash("a"), super::name_hash("b"));
    }
}
