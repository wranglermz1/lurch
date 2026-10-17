//! The draw list's token table (`0x20686590`, built by `0x23f8b0`): what `%command%` in a
//! string reads, the key the command is bound to, found as the layout's substitution
//! (`0x2f4c70`) finds it. Only the keyboard and mouse table is built.
//!
//! Not modelled: the entries named after the gamepad's keys (`0x23f6a0` past `0x72`), and the
//! entries the engine adds for commands it aliases (`toggle_duck`, `ability N`, the table at
//! `0x132b910`).

use std::ffi::CString;

/// What a name with no entry reads (`0x132f778`).
const UNBOUND: &str = "%@ICON_UNBOUND%";
/// The longest command the table takes, and how many entries it holds.
const MAX_NAME: usize = 0x18;
const MAX_ENTRIES: usize = 0x118;
/// Key codes from here on are the mouse's (`0x23f550`).
const FIRST_MOUSE: u16 = 0x6c;
const LAST_MOUSE: u16 = 0x72;
/// What wraps a key's display string that is not an image or an embedded element.
const KEY_BORDER: (&str, &str) = ("%<%%$rui/borders/key_border%", "%>%");

struct Token {
    name: CString,
    text: CString,
    other: CString,
    hold: bool,
}

pub struct Tokens {
    entries: Vec<Token>,
    /// By name length, the first entry of that length.
    ranges: [u16; MAX_NAME + 2],
    min: u16,
    max: u16,
    default: CString,
}

impl Default for Tokens {
    fn default() -> Tokens {
        Tokens { entries: Vec::new(), ranges: [0; MAX_NAME + 2], min: 0, max: 0, default: CString::new(UNBOUND).unwrap() }
    }
}

/// `_strnicmp` as `0x47c3d0` does it: signed bytes, ASCII letters folded to upper case.
fn strnicmp(a: &[u8], b: &[u8], n: usize) -> i32 {
    for i in 0..n {
        let fold = |c: i32| if (c - 0x61) as u32 > 0x19 { c } else { c - 0x20 };
        let (x, y) = (a.get(i).map_or(0, |&c| c as i8 as i32), b.get(i).map_or(0, |&c| c as i8 as i32));
        let (x, y) = if x == y { (x, y) } else { (fold(x), fold(y)) };
        if x != y {
            return if x < y { -1 } else { 1 };
        }
        if x == 0 {
            break;
        }
    }
    0
}

impl Tokens {
    /// The keyboard and mouse table: `keys` is `scripts/keys_keyboard.rson` by key code, `binds`
    /// each bound key's code, whether its bind is the held one, and its command string;
    /// `press` and `hold` the localized `INPUT_PRESS` and `INPUT_HOLD`. First an entry per key
    /// (`0x23f6a0`), named as ButtonCodeToString names it, its text the key's display string.
    pub fn new(keys: &[String], binds: &[(u16, bool, &str)], press: &str, hold: &str) -> Tokens {
        let c = |s: &str| CString::new(s).unwrap_or_default();
        let mut entries = Vec::new();
        for (i, name) in crate::vgui::keybinds::KEY_NAMES.iter().enumerate() {
            if let Some(text) = display(keys, i as u16 + 1) {
                entries.push(Token { name: c(name), text: c(&text), other: c(""), hold: false });
            }
        }
        for &(code, held, bind) in binds {
            let Some(text) = display(keys, code) else { continue };
            let other = other_form(if held { hold } else { press }, &text);
            for name in commands(bind) {
                if entries.len() < MAX_ENTRIES && (1..=MAX_NAME).contains(&name.len()) {
                    entries.push(Token { name: c(name), text: c(&text), other: c(&other), hold: held });
                }
            }
        }
        entries.sort_by(|a, b| {
            let (x, y) = (a.name.as_bytes(), b.name.as_bytes());
            x.len().cmp(&y.len()).then(strnicmp(x, y, x.len()).cmp(&0))
        });
        let (mut min, mut max) = (MAX_NAME as u16, 1u16);
        let mut ranges = [0u16; MAX_NAME + 2];
        for e in &entries {
            let n = e.name.as_bytes().len();
            min = min.min(n as u16);
            max = max.max(n as u16);
            for r in &mut ranges[n + 1..] {
                *r += 1;
            }
        }
        if max < min {
            (min, max) = (0, 0);
        }
        Tokens { entries, ranges, min, max, ..Tokens::default() }
    }

    /// The text `%name%` reads, or with `other` (`%&name%`) the entry's other form: a press
    /// bind's before a held one's. The table's default when nothing is found.
    pub fn find(&self, name: &[u8], other: bool) -> *const u8 {
        let n = name.len();
        if n.wrapping_sub(self.min as usize) > (self.max - self.min) as usize || n + 1 >= self.ranges.len() {
            return self.default.as_ptr() as *const u8;
        }
        let at = |i: usize| self.entries.get(i).map_or(&[][..], |e| e.name.as_bytes_with_nul());
        let (mut lo, mut hi) = (self.ranges[n] as usize, self.ranges[n + 1] as usize);
        let end = hi;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let c = strnicmp(name, at(mid), n);
            if c < 0 {
                hi = mid;
            } else if c > 0 {
                lo = mid + 1;
            } else {
                let e = &self.entries[mid];
                if !other || e.other.as_bytes().is_empty() {
                    return e.text.as_ptr() as *const u8;
                }
                if !e.hold {
                    return e.other.as_ptr() as *const u8;
                }
                let press = |i: usize| self.entries.get(i).filter(|e| !e.hold).map(|e| e.other.as_ptr() as *const u8);
                let mut i = mid;
                while i > self.ranges[n] as usize {
                    i -= 1;
                    if strnicmp(name, at(i), n) != 0 {
                        break;
                    }
                    if let Some(p) = press(i) {
                        return p;
                    }
                }
                let mut i = mid;
                while i < end {
                    i += 1;
                    if strnicmp(name, at(i), n) != 0 {
                        break;
                    }
                    if let Some(p) = press(i) {
                        return p;
                    }
                }
                return e.other.as_ptr() as *const u8;
            }
        }
        self.default.as_ptr() as *const u8
    }
}

/// A key's display string (`0x23f550`): its line of the keys list, wrapped in the key border
/// unless it is an embedded element (`%@`) or, for the mouse, an image (`%$`).
fn display(keys: &[String], code: u16) -> Option<String> {
    let s = keys.get(code as usize)?;
    let plain = !s.contains("%@") && !(code >= FIRST_MOUSE && s.contains("%$"));
    Some(if plain && code <= LAST_MOUSE { format!("{}{s}{}", KEY_BORDER.0, KEY_BORDER.1) } else { s.clone() })
}

/// A command's other form (`0x23f330`): the localized format with its `%...%` replaced by the
/// key's display string, or `#` and the format when it has no `%`.
fn other_form(format: &str, text: &str) -> String {
    let Some(i) = format.find('%') else { return format!("#{format}") };
    let rest = format[i + 1..].find('%').map_or("", |j| &format[i + 1 + j + 1..]);
    format!("{}{text}{rest}", &format[..i])
}

/// The commands of a bind string as the builder splits it: at each `;`, leading control
/// characters and spaces and one `+` dropped.
fn commands(bind: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let b = bind.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && b[i] == b';' {
            i += 1;
        }
        while i < b.len() && b[i].wrapping_sub(1) < 0x20 {
            i += 1;
        }
        if i < b.len() && b[i] == b'+' {
            i += 1;
        }
        let start = i;
        while i < b.len() && b[i] != b';' {
            i += 1;
        }
        if i > start {
            out.push(&bind[start..i]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::Tokens;
    use std::ffi::CStr;

    fn read(p: *const u8) -> String {
        // SAFETY: `find` returns a pointer to one of the table's NUL-terminated strings.
        unsafe { CStr::from_ptr(p as *const std::ffi::c_char) }.to_string_lossy().into_owned()
    }

    #[test]
    fn a_command_reads_its_key_and_its_press_form() {
        let mut keys = vec![String::new(); 0x73];
        keys[0xf] = "%@KEY_E%".into();
        keys[0x58] = "%$vgui/HUD/pc_arrow_up%".into();
        keys[0x6e] = "%$vgui/HUD/pc_mouse_mb%".into();
        let t = Tokens::new(&keys, &[(0xf, false, "+use; +use_long"), (0x6e, false, "+ping"), (0x58, true, "+jump")], "%command%", "HOLD %command%");
        assert_eq!(read(t.find(b"use", true)), "%@KEY_E%");
        assert_eq!(read(t.find(b"USE_LONG", false)), "%@KEY_E%");
        assert_eq!(read(t.find(b"ping", false)), "%$vgui/HUD/pc_mouse_mb%");
        assert_eq!(read(t.find(b"jump", false)), "%<%%$rui/borders/key_border%%$vgui/HUD/pc_arrow_up%%>%");
        assert_eq!(read(t.find(b"jump", true)), "HOLD %<%%$rui/borders/key_border%%$vgui/HUD/pc_arrow_up%%>%");
        assert_eq!(read(t.find(b"offhand1", true)), "%@ICON_UNBOUND%");
        assert_eq!(read(t.find(b"E", false)), "%@KEY_E%", "a key reads its own display string");
        assert_eq!(read(t.find(b"MOUSE3", true)), "%$vgui/HUD/pc_mouse_mb%");
    }
}
