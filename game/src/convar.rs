//! The game's ConVars the settings menu shows (`docs/ui.md`, "The pause and settings menus"),
//! kept as the engine keeps them and saved in its two files, `settings.cfg` and `profile.cfg`,
//! next to `lurch.cfg`. On the first run, with neither file there, they are read from the
//! game's own files (`Saved Games/Respawn/Apex`) and written as lurch's; the game's files are
//! never written.
//!
//! The key binds are `settings.cfg`'s bind lines as the engine keeps them by key
//! (`lurch::vgui::keybinds::KeyBindings`), written back as it writes them.
//!
//! Not modelled: the ConVars the engine registers outside the menus, which the game also
//! writes; hexadecimal numbers in `atof`; a ConVar command with more than one argument.

use std::io;
use std::path::PathBuf;

use bevy::prelude::*;
use lurch::assets::keyvalues::atof;

use crate::binds::Binds;
use lurch::vgui::keybinds::KeyBindings;
use crate::Config;

/// The flags that pick a ConVar's file (`0x22fcb0`), and the one that keeps no string.
pub const FCVAR_ARCHIVE: u32 = 0x80;
pub const FCVAR_ARCHIVE_PLAYERPROFILE: u32 = 0x100_0000;
pub const FCVAR_NEVER_AS_STRING: u32 = 0x1000;

/// Name, default, flags, least and greatest value.
type Row = (&'static str, &'static str, u32, Option<f32>, Option<f32>);

/// A file the engine reads larger than this is not read at all (`0x22ff0c`).
const MAX_FILE: usize = 0x3fff;

/// The image's values (`tools/convars.py`), but for
/// the nine R5Reloaded registers itself (`docs/ui.md`), from its mods' `mod.vdf` and from
/// `gamesdk.dll`.
const TABLE: [Row; 85] = [
    ("cc_text_size", "0", 0x1000000, None, None),
    ("chroma_enable", "1", 0x80, None, None),
    ("cl_fovScale", "1.27216005", 0x41000200, Some(1.0), Some(1.7)),
    ("cl_showfps_archived", "0", 0x80080, Some(0.0), Some(1.0)),
    ("cl_showpos_archived", "0", 0x80080, Some(0.0), Some(1.0)),
    ("closecaption", "0", 0x1000000, None, None),
    ("colorblind_mode", "0", 0x41000000, Some(0.0), Some(3.0)),
    ("damage_indicator_style_pilot", "2", 0x1000000, None, None),
    ("enable_healthbar", "1", 0x80080, Some(0.0), Some(1.0)),
    ("enable_motd", "1", 0x80080, Some(0.0), Some(1.0)),
    ("enable_movement_overlay", "0", 0x80080, Some(0.0), Some(1.0)),
    ("fps_max", "-1", 0x80080, None, None),
    ("gameCursor_Velocity", "1300.0", 0x1000000, None, None),
    ("gamepad_aim_speed", "2", 0x1000000, Some(0.0), Some(7.0)),
    ("gamepad_aim_speed_ads_0", "-1", 0x1000000, Some(-1.0), Some(7.0)),
    ("gamepad_custom_ads_pitch", "75.0", 0x1000000, None, None),
    ("gamepad_custom_ads_turn_delay", "0.25", 0x1000000, None, None),
    ("gamepad_custom_ads_turn_pitch", "30.0", 0x1000000, None, None),
    ("gamepad_custom_ads_turn_time", "1.0", 0x1000000, None, None),
    ("gamepad_custom_ads_turn_yaw", "30.0", 0x1000000, None, None),
    ("gamepad_custom_ads_yaw", "110.0", 0x1000000, None, None),
    ("gamepad_custom_assist_on", "1", 0x1000000, None, None),
    ("gamepad_custom_curve", "10.0", 0x1000000, None, None),
    ("gamepad_custom_deadzone_in", "0.15", 0x1000000, None, None),
    ("gamepad_custom_deadzone_out", "0.02", 0x1000000, None, None),
    ("gamepad_custom_enabled", "0", 0x1000000, None, None),
    ("gamepad_custom_hip_pitch", "120.0", 0x1000000, None, None),
    ("gamepad_custom_hip_turn_delay", "0.0", 0x1000000, None, None),
    ("gamepad_custom_hip_turn_pitch", "0.0", 0x1000000, None, None),
    ("gamepad_custom_hip_turn_time", "0.33", 0x1000000, None, None),
    ("gamepad_custom_hip_turn_yaw", "220.0", 0x1000000, None, None),
    ("gamepad_custom_hip_yaw", "160.0", 0x1000000, None, None),
    ("gamepad_deadzone_index_look", "1", 0x1000000, None, None),
    ("gamepad_deadzone_index_move", "1", 0x1000000, None, None),
    ("gamepad_look_curve", "0", 0x1000000, Some(0.0), Some(4.0)),
    ("gamepad_stick_layout", "0", 0x1000000, None, None),
    ("gamepad_toggle_ads", "0", 0x1000000, None, None),
    ("gamepad_togglecrouch_hold", "0", 0x1000000, None, None),
    ("gamepad_trigger_threshold", "30", 0x1000000, None, None),
    ("gamepad_use_type", "2", 0x1000000, None, None),
    ("hud_setting_accessibleChat", "0", 0x1000000, None, None),
    ("hud_setting_damageIndicatorStyle", "2", 0x1000000, None, None),
    ("hud_setting_damageTextStyle", "1", 0x1000000, None, None),
    ("hud_setting_minimapRotate", "0", 0x1000000, None, None),
    ("hud_setting_pingAlpha", "1.0", 0x1000000, None, None),
    ("hud_setting_showButtonHints", "1", 0x1000000, None, None),
    ("hud_setting_showMedals", "1", 0x1000000, None, None),
    ("hud_setting_showObituary", "1", 0x1000000, None, None),
    ("hud_setting_streamerMode", "0", 0x1000000, None, None),
    ("hudchat_play_text_to_speech", "0", 0x1000000, None, None),
    ("joy_inverty", "0", 0x1000000, None, None),
    ("joy_rumble", "1", 0x1000000, None, None),
    ("m_acceleration", "0", 0x80, None, None),
    ("m_invert_pitch", "0", 0x1000000, None, None),
    ("miles_channels", "0", 0x40000000, None, None),
    ("miles_language", "", 0x1000000, None, None),
    ("mouse_sensitivity", "5", 0x80, Some(0.1), Some(20.0)),
    ("mouse_use_per_scope_sensitivity_scalars", "0", 0x80, None, None),
    ("mouse_zoomed_sensitivity_scalar_0", "1.0", 0x80, Some(0.1), Some(20.0)),
    ("mouse_zoomed_sensitivity_scalar_1", "1.0", 0x80, Some(0.1), Some(20.0)),
    ("mouse_zoomed_sensitivity_scalar_2", "1.0", 0x80, Some(0.1), Some(20.0)),
    ("mouse_zoomed_sensitivity_scalar_3", "1.0", 0x80, Some(0.1), Some(20.0)),
    ("mouse_zoomed_sensitivity_scalar_4", "1.0", 0x80, Some(0.1), Some(20.0)),
    ("mouse_zoomed_sensitivity_scalar_5", "1.0", 0x80, Some(0.1), Some(20.0)),
    ("mouse_zoomed_sensitivity_scalar_6", "1.0", 0x80, Some(0.1), Some(20.0)),
    ("mouse_zoomed_sensitivity_scalar_7", "1.0", 0x80, Some(0.1), Some(20.0)),
    ("open_motd_once_per_server", "1", 0x80080, Some(0.0), Some(1.0)),
    ("pin_opt_in", "1", 0x1000000, None, None),
    ("player_setting_autosprint", "0", 0x41000000, None, None),
    ("player_setting_damage_closes_deathbox_menu", "1", 0x1000000, None, None),
    ("r_lod_switch_scale", "1", 0x40000000, None, None),
    ("settings_antilag", "1", 0x80000, None, None),
    ("settings_reflex", "1", 0x80000, None, None),
    ("show_dev_warning_dialogue", "1", 0x80080, Some(0.0), Some(1.0)),
    ("sound_volume", "1", 0x1000000, None, None),
    ("sound_volume_dialogue", "1", 0x1000000, None, None),
    ("sound_volume_music_game", "1", 0x1000000, None, None),
    ("sound_volume_music_lobby", "1", 0x1000000, None, None),
    ("sound_volume_sfx", "1", 0x1000000, None, None),
    ("sound_volume_voice", "1", 0x80, None, None),
    ("sound_without_focus", "0", 0x1000000, None, None),
    ("speechtotext_enabled", "0", 0x1000000, None, None),
    ("speex_quiet_threshold", "1300", 0x80, None, None),
    ("TalkIsStream", "0", 0x80, None, None),
    ("weapon_setting_autocycle_on_empty", "1", 0x41000200, None, None),
];

pub struct ConVar {
    pub name: &'static str,
    pub default: &'static str,
    pub flags: u32,
    min: Option<f32>,
    max: Option<f32>,
    pub string: String,
    pub float: f32,
    pub int: i32,
}

impl ConVar {
    /// `ConVar::Create` (`0x470540`): the default taken as the value, not clamped.
    fn new((name, default, flags, min, max): Row) -> ConVar {
        let mut c = ConVar { name, default, flags, min, max, string: default.into(), float: 0.0, int: 0 };
        if !c.set_color(default) {
            let f = finite(atof(default) as f32);
            (c.float, c.int) = (f, truncate(f));
        }
        c
    }

    /// The value as a string (`0x470040`): a colour, else a number clamped to the range, its
    /// string then printed again when clamped.
    pub fn set_string(&mut self, value: &str) {
        let mut text = value.to_string();
        if !self.set_color(value) {
            let mut f = finite(atof(value) as f32);
            if self.clamp(&mut f) {
                text = format!("{:.6}", f as f64);
            }
            (self.float, self.int) = (f, truncate(f));
        }
        if self.flags & FCVAR_NEVER_AS_STRING == 0 {
            self.string = text;
        }
    }

    /// The value as a float (`0x470340`).
    pub fn set_float(&mut self, value: f32) {
        if value == self.float {
            return;
        }
        let mut f = value;
        self.clamp(&mut f);
        (self.float, self.int) = (f, truncate(f));
        if self.flags & FCVAR_NEVER_AS_STRING == 0 {
            self.string = format!("{:.6}", f as f64);
        }
    }

    /// The value as an int (`0x470420`).
    pub fn set_int(&mut self, value: i32) {
        if value == self.int {
            return;
        }
        let (mut f, mut i) = (value as f32, value);
        if self.clamp(&mut f) {
            i = truncate(f);
        }
        (self.float, self.int) = (f, i);
        if self.flags & FCVAR_NEVER_AS_STRING == 0 {
            self.string = i.to_string();
        }
    }

    /// `0x470300`.
    fn clamp(&self, f: &mut f32) -> bool {
        if let Some(lo) = self.min
            && *f <= lo
            && lo != *f
        {
            *f = lo;
            return true;
        }
        if let Some(hi) = self.max
            && hi < *f
        {
            *f = hi;
            return true;
        }
        false
    }

    /// `0x46ff90`: three or four numbers below 256 (`%i %i %i %i`) are a colour, its bytes
    /// the int and the int the float; the alpha is 255 when left out.
    fn set_color(&mut self, s: &str) -> bool {
        let v = scan_ints(s);
        if v.len() < 3 {
            return false;
        }
        let a = if v.len() == 3 { 255 } else { v[3] };
        if [v[0], v[1], v[2], a].iter().any(|&x| x as u32 >= 0x100) {
            return false;
        }
        self.int = i32::from_le_bytes([v[0] as u8, v[1] as u8, v[2] as u8, a as u8]);
        self.float = self.int as f32;
        true
    }
}

/// An infinity or NaN is taken as the largest float (`0x4700f8`).
fn finite(f: f32) -> f32 {
    if f.is_finite() { f } else { f32::MAX }
}

/// `cvttss2si`: out of range is the least int.
fn truncate(f: f32) -> i32 {
    if (-2147483648.0..2147483648.0).contains(&f) { f as i32 } else { i32::MIN }
}


/// `sscanf(s, "%i %i %i %i")`: the ints read before the first that fails.
fn scan_ints(s: &str) -> Vec<i32> {
    let b = s.as_bytes();
    let mut at = 0;
    let mut out = Vec::new();
    while out.len() < 4 {
        while b.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        let neg = b.get(at) == Some(&b'-');
        if matches!(b.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let hex = b.get(at) == Some(&b'0') && matches!(b.get(at + 1), Some(b'x' | b'X')) && b.get(at + 2).is_some_and(u8::is_ascii_hexdigit);
        let (radix, start) = if hex { (16, at + 2) } else if b.get(at) == Some(&b'0') { (8, at) } else { (10, at) };
        let mut end = start;
        while b.get(end).is_some_and(|&c| (c as char).is_digit(radix)) {
            end += 1;
        }
        if end == start {
            break;
        }
        let v = i64::from_str_radix(&s[start..end], radix).unwrap_or(i64::MAX).min(i32::MAX as i64 + 1);
        let v = if neg { -v } else { v };
        out.push(v.clamp(i32::MIN as i64, i32::MAX as i64) as i32);
        at = end;
    }
    out
}

/// Each file the engine reads at start and writes on `savePlayerConfig`, with the flag its
/// ConVars carry (`0x22fcb0`, `0xc93ed0`).
pub const FILES: [(&str, u32); 2] = [("settings.cfg", FCVAR_ARCHIVE), ("profile.cfg", FCVAR_ARCHIVE_PLAYERPROFILE)];

#[derive(Resource)]
pub struct ConVars {
    vars: Vec<ConVar>,
    /// The key binds `settings.cfg`'s bind lines made.
    pub keys: KeyBindings,
    /// Where the files are kept; none, and nothing is written.
    dir: Option<PathBuf>,
    /// `savePlayerConfig` ran (`0x230170`): both files are written on the next frame.
    pub save: bool,
}

impl Default for ConVars {
    fn default() -> Self {
        ConVars { vars: TABLE.into_iter().map(ConVar::new).collect(), keys: KeyBindings::default(), dir: None, save: false }
    }
}

impl ConVars {
    /// The ConVars from lurch's files in `dir`; with neither there, from the game's in `seed`
    /// (`local/settings.cfg`, `profile/profile.cfg`), then written to `dir`.
    pub fn load(dir: PathBuf, seed: Option<PathBuf>) -> ConVars {
        let mut c = ConVars { dir: Some(dir.clone()), ..ConVars::default() };
        let ours = FILES.iter().any(|(name, _)| dir.join(name).exists());
        for (name, flag) in FILES {
            let sub = if flag == FCVAR_ARCHIVE { "local" } else { "profile" };
            let path = if ours { Some(dir.join(name)) } else { seed.as_ref().map(|s| s.join(sub).join(name)) };
            if let Some(bytes) = path.and_then(|p| std::fs::read(p).ok()).filter(|b| b.len() < MAX_FILE) {
                c.exec(&String::from_utf8_lossy(&bytes), flag == FCVAR_ARCHIVE);
            }
        }
        c.save = !ours;
        c
    }

    /// A ConVar by name, the case of its letters aside.
    pub fn get(&self, name: &str) -> Option<&ConVar> {
        self.vars.iter().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut ConVar> {
        self.vars.iter_mut().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// A script's `SetConVarString` (`0x8d48b0`): the value set, and `savePlayerConfig` run
    /// when the ConVar is in `profile.cfg`.
    pub fn script_set(&mut self, name: &str, value: &str) {
        let Some(c) = self.get_mut(name) else { return };
        c.set_string(value);
        if c.flags & FCVAR_ARCHIVE_PLAYERPROFILE != 0 {
            self.save = true;
        }
    }

    /// A file run as console commands (`0x22fea0`, `0x20de80`): each line a ConVar's name and
    /// its value. A bind line is kept when `binds`; any other is not lurch's and does nothing.
    pub fn exec(&mut self, text: &str, binds: bool) {
        let mut lines = self.keys.lines();
        for line in text.split(|c: char| c < ' ' && c != '\t') {
            let words = tokens(line);
            match words.as_slice() {
                [cmd, ..] if binds && ["bind_US_standard", "bind_held_US_standard"].iter().any(|b| cmd.eq_ignore_ascii_case(b)) => lines.push(line.to_string()),
                [name, value] => {
                    if let Some(c) = self.get_mut(name) {
                        c.set_string(value);
                    }
                }
                _ => {}
            }
        }
        self.keys = KeyBindings::from_lines(&lines);
    }

    /// A file as the engine writes it (`0x212260`): for `settings.cfg` the binds first, then
    /// each ConVar with the file's flag, ordered by name with case aside, as `name "value"`.
    pub fn file(&self, flag: u32) -> String {
        let mut out = String::new();
        if flag == FCVAR_ARCHIVE {
            for b in self.keys.lines() {
                out += &b;
                out += "\r\n";
            }
        }
        let mut vars: Vec<&ConVar> = self.vars.iter().filter(|c| c.flags & flag != 0).collect();
        vars.sort_by(|a, b| a.name.bytes().map(|c| c.to_ascii_lowercase()).cmp(b.name.bytes().map(|c| c.to_ascii_lowercase())));
        for c in vars {
            let value = if c.flags & FCVAR_NEVER_AS_STRING != 0 { "FCVAR_NEVER_AS_STRING" } else { c.string.as_str() };
            out += &format!("{} \"{value}\"\r\n", c.name);
        }
        out
    }

    /// The settings lurch acts on, into `cfg`, but where a flag on the command line set one:
    /// sensitivity, invert, field of view, autosprint (mode 1 only: 2 and 3 are taken as
    /// off), the frame cap, the volumes, and the key binds when the file has any.
    pub fn apply(&self, cfg: &mut Config) {
        let ours = |flag: &str| !cfg.cli.iter().any(|f| f == flag);
        let float = |n: &str| self.get(n).map_or(0.0, |c| c.float);
        let int = |n: &str| self.get(n).map_or(0, |c| c.int);
        if ours("--sens") {
            cfg.sens = float("mouse_sensitivity");
        }
        if ours("--fov-scale") {
            cfg.fov_scale = float("cl_fovScale");
        }
        if ours("--autosprint") {
            cfg.autosprint = int("player_setting_autosprint") == 1;
        }
        cfg.fps_max_setting = ours("--fps-max").then(|| float("fps_max"));
        cfg.invert_pitch = int("m_invert_pitch") != 0;
        cfg.volumes = ["sound_volume", "sound_volume_voice", "sound_volume_music_lobby", "sound_volume_music_game", "sound_volume_dialogue", "sound_volume_sfx"].map(float);
        let lines: Vec<(String, String)> = self
            .keys
            .lines()
            .iter()
            .filter_map(|l| match tokens(l).as_slice() {
                [cmd, key, command, ..] if cmd.eq_ignore_ascii_case("bind_US_standard") => Some((key.clone(), command.clone())),
                _ => None,
            })
            .collect();
        if !lines.is_empty() {
            let (binds, up, down) = Binds::from_game(&lines);
            cfg.binds = binds;
            if ours("--wheel-up") {
                cfg.wheel_up = up;
            }
            if ours("--wheel-down") {
                cfg.wheel_down = down;
            }
        }
    }

    pub fn write(&self) -> io::Result<()> {
        let Some(dir) = &self.dir else { return Ok(()) };
        for (name, flag) in FILES {
            std::fs::write(dir.join(name), self.file(flag))?;
        }
        Ok(())
    }
}

/// A command line's words: a quoted run, or a run up to white space.
fn tokens(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line.trim_start();
    while !rest.is_empty() {
        if let Some(q) = rest.strip_prefix('"') {
            let end = q.find('"').unwrap_or(q.len());
            out.push(q[..end].to_string());
            rest = q.get(end + 1..).unwrap_or("");
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            out.push(rest[..end].to_string());
            rest = &rest[end..];
        }
        rest = rest.trim_start();
    }
    out
}

/// The game's own folder for its files (`0xc93dd0`): the Saved Games known folder, then
/// `Respawn/Apex`.
#[cfg(windows)]
pub fn game_dir() -> Option<PathBuf> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStringExt;
    #[repr(C)]
    struct Guid(u32, u16, u16, [u8; 8]);
    const FOLDERID_SAVED_GAMES: Guid = Guid(0x4c5c_32ff, 0xbb9d, 0x43b0, [0xb5, 0xb4, 0x2d, 0x72, 0xe5, 0x4e, 0xaa, 0xa4]);
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHGetKnownFolderPath(id: *const Guid, flags: u32, token: *mut c_void, path: *mut *mut u16) -> i32;
    }
    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoTaskMemFree(p: *mut c_void);
    }
    let mut p = std::ptr::null_mut();
    // SAFETY: the out pointer is ours; the string it gets is read up to its NUL and freed.
    unsafe {
        let ok = SHGetKnownFolderPath(&FOLDERID_SAVED_GAMES, 0, std::ptr::null_mut(), &mut p) >= 0;
        let dir = ok.then(|| {
            let n = (0..).take_while(|&i| *p.add(i) != 0).count();
            PathBuf::from(std::ffi::OsString::from_wide(std::slice::from_raw_parts(p, n)))
        });
        CoTaskMemFree(p.cast());
        dir.map(|d| d.join("Respawn").join("Apex"))
    }
}

#[cfg(not(windows))]
pub fn game_dir() -> Option<PathBuf> {
    None
}

pub struct ConVarPlugin;

impl Plugin for ConVarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(First, (act, save).chain().run_if(resource_exists::<ConVars>));
    }
}

/// What the engine does with the settings each frame, then the settings into `Config` when
/// they changed. An `fps_max` above 0 and below 30 is raised to 30 (`0x297192`; without
/// `sv_cheats`, which lurch never sets, and a flag at `0xb37b885` not identified).
fn act(mut convars: ResMut<ConVars>, mut cfg: ResMut<Config>) {
    if convars.get("fps_max").is_some_and(|c| 0.0 < c.float && c.float < 30.0)
        && let Some(c) = convars.get_mut("fps_max")
    {
        c.set_float(30.0);
    }
    if !convars.is_changed() {
        return;
    }
    let mut next = cfg.clone();
    convars.apply(&mut next);
    if *cfg != next {
        *cfg = next;
    }
}

fn save(mut convars: ResMut<ConVars>) {
    if !convars.save {
        return;
    }
    convars.save = false;
    if let Err(e) = convars.write() {
        warn!("writing settings.cfg and profile.cfg: {e}");
    }
}
