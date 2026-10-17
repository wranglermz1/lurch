//! KeyValues text as the engine reads it (`0x475dc0`): the format of VGUI's `.res` and `.menu`
//! layouts and of `basemodui_scheme.res`. A key holds a string or a block of keys; names repeat
//! and keep their order, and a lookup takes the first, without case. A condition after a key's
//! name or after its value (`[$WINDOWS]`, `[!$GAMECONSOLE && $WIN32WIDE]`) drops the key when it
//! is false, evaluated as on a PC playing in English.
//!
//! Not modelled: `#base` and `#include` (skipped with their file name), escapes in quoted
//! strings (VGUI's files load without them), and a value's type: a value stays the text read,
//! where the engine keeps an integer or a float when the whole text parses as one (`0x4765e0`).

const LANGUAGE: &str = "english";

#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    pub name: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Str(String),
    Block(Vec<Key>),
}

impl Key {
    pub fn keys(&self) -> &[Key] {
        match &self.value {
            Value::Block(keys) => keys,
            Value::Str(_) => &[],
        }
    }

    pub fn find(&self, name: &str) -> Option<&Key> {
        self.keys().iter().find(|k| k.name.eq_ignore_ascii_case(name))
    }

    pub fn str(&self, name: &str) -> Option<&str> {
        match &self.find(name)?.value {
            Value::Str(s) => Some(s),
            Value::Block(_) => None,
        }
    }

    pub fn is_block(&self) -> bool {
        matches!(self.value, Value::Block(_))
    }

    /// The first key of `name` set to `v`, or a new one at the end.
    pub fn set(&mut self, name: &str, v: &str) {
        let Value::Block(keys) = &mut self.value else { return };
        match keys.iter_mut().find(|k| k.name.eq_ignore_ascii_case(name)) {
            Some(k) => k.value = Value::Str(v.to_string()),
            None => keys.push(Key { name: name.to_string(), value: Value::Str(v.to_string()) }),
        }
    }
}

struct Token {
    text: String,
    quoted: bool,
    conditional: bool,
}

struct Buffer<'a> {
    text: &'a [u8],
    at: usize,
}

fn is_space(c: u8) -> bool {
    (9..=13).contains(&c) || c == b' '
}

impl Buffer<'_> {
    fn eat_comment(&mut self) -> bool {
        if !self.text[self.at..].starts_with(b"//") {
            return false;
        }
        self.at = self.text[self.at..].iter().position(|&c| c == b'\n').map_or(self.text.len(), |n| self.at + n + 1);
        true
    }

    /// `0x473350`.
    fn read_token(&mut self) -> Option<Token> {
        loop {
            while self.text.get(self.at).is_some_and(|&c| is_space(c)) {
                self.at += 1;
            }
            if !self.eat_comment() {
                break;
            }
        }
        let &first = self.text.get(self.at)?;
        let mut token = Token { text: String::new(), quoted: false, conditional: false };
        if first == b'"' {
            let rest = &self.text[self.at + 1..];
            let end = rest.iter().position(|&c| c == b'"').unwrap_or(rest.len());
            token.text = String::from_utf8_lossy(&rest[..end]).into_owned();
            token.quoted = true;
            self.at = (self.at + end + 2).min(self.text.len());
            return Some(token);
        }
        if first == b'{' || first == b'}' {
            token.text.push(first as char);
            self.at += 1;
            return Some(token);
        }
        let start = self.at;
        let mut bracket = false;
        while let Some(&c) = self.text.get(self.at) {
            if matches!(c, 0 | b'"' | b'{' | b'}') {
                break;
            }
            if c == b'[' {
                bracket = true;
            } else if c == b']' {
                if bracket {
                    bracket = false;
                    token.conditional = true;
                }
            } else if is_space(c) && !bracket {
                break;
            }
            self.at += 1;
        }
        token.text = String::from_utf8_lossy(&self.text[start..self.at.min(start + 1023)]).into_owned();
        Some(token)
    }
}

/// The keys of a file, top-level ones first (`0x475dc0`).
pub fn parse(text: &[u8]) -> Vec<Key> {
    let mut buf = Buffer { text, at: 0 };
    let mut out = Vec::new();
    while let Some(tok) = buf.read_token() {
        if !tok.quoted && tok.text.is_empty() {
            break;
        }
        if tok.text.eq_ignore_ascii_case("#include") || tok.text.eq_ignore_ascii_case("#base") {
            buf.read_token();
            continue;
        }
        let mut key = Key { name: tok.text, value: Value::Block(Vec::new()) };
        let mut keep = true;
        let mut next = buf.read_token();
        if let Some(t) = next.as_ref().filter(|t| t.conditional) {
            keep = evaluate_conditional(&t.text);
            next = buf.read_token();
        }
        if next.is_some_and(|t| t.text == "{" && !t.quoted) {
            recursive_load(&mut buf, &mut key);
        }
        if keep {
            out.push(key);
        }
    }
    out
}

/// A block's keys up to its `}` (`0x4765e0`). A malformed key ends the block and stays in it,
/// as in the engine, which adds a key before reading its value.
fn recursive_load(buf: &mut Buffer, block: &mut Key) {
    let mut keys = Vec::new();
    while let Some(name) = buf.read_token() {
        if name.text.is_empty() || (name.text == "}" && !name.quoted) {
            break;
        }
        let mut key = Key { name: name.text, value: Value::Block(Vec::new()) };
        match load_key(buf, &mut key) {
            Ok(keep) => keys.extend(keep.then_some(key)),
            Err(()) => {
                keys.push(key);
                break;
            }
        }
    }
    block.value = Value::Block(keys);
}

/// A key's value or block after its name, and whether its conditions keep it.
fn load_key(buf: &mut Buffer, key: &mut Key) -> Result<bool, ()> {
    let mut tok = buf.read_token().ok_or(())?;
    let mut keep = true;
    if tok.conditional {
        keep = evaluate_conditional(&tok.text);
        tok = buf.read_token().ok_or(())?;
    }
    if tok.text == "{" && !tok.quoted {
        recursive_load(buf, key);
        return Ok(keep);
    }
    if (tok.text == "}" && !tok.quoted) || tok.conditional {
        return Err(());
    }
    key.value = Value::Str(tok.text);
    let at = buf.at;
    match buf.read_token() {
        Some(t) if t.conditional => keep = evaluate_conditional(&t.text),
        _ => buf.at = at,
    }
    Ok(keep)
}

/// `0x475d60` with the engine's default symbols (`symbol`): `!`, `(...)`, and `&&` and `||` of
/// the same precedence taken left to right (`0x4723d0`), over `$NAME`s and numbers. A condition
/// that does not parse is false.
pub fn evaluate_conditional(text: &str) -> bool {
    let body = text.strip_prefix('[').map_or(text, |t| t.strip_suffix(']').unwrap_or(t));
    let mut p = Expr { text: body.as_bytes(), at: 0, cur: 0 };
    p.advance();
    p.binary().is_ok_and(|v| v.unwrap_or(false))
}

struct Expr<'a> {
    text: &'a [u8],
    at: usize,
    cur: u8,
}

impl Expr<'_> {
    fn byte(&self, at: usize) -> u8 {
        self.text.get(at).copied().unwrap_or(0)
    }

    fn advance(&mut self) {
        while self.byte(self.at) == b' ' {
            self.at += 1;
        }
        self.cur = self.byte(self.at);
        self.at += 1;
    }

    /// `0x4721b0`. A value of None is the empty node a lone `!` leaves, false when evaluated.
    fn binary(&mut self) -> Result<Option<bool>, ()> {
        let mut left = self.not()?;
        while matches!(self.cur, b'|' | b'&') {
            let op = self.cur;
            let second = self.byte(self.at);
            self.at += 1;
            if second & op != op {
                return Err(());
            }
            self.advance();
            let right = self.not()?;
            let (l, r) = (left.unwrap_or(false), right.unwrap_or(false));
            left = Some(if op == b'&' { l && r } else { l || r });
        }
        Ok(left)
    }

    /// `0x4720d0`: a `!` node's value is its right side's, negated; what stood before it is
    /// dropped.
    fn not(&mut self) -> Result<Option<bool>, ()> {
        let mut value = self.primary()?;
        while self.cur == b'!' {
            self.advance();
            value = Some(!self.primary()?.unwrap_or(false));
        }
        Ok(value)
    }

    /// `0x471e70`.
    fn primary(&mut self) -> Result<Option<bool>, ()> {
        let value = match self.cur {
            b'(' => {
                self.advance();
                let v = self.binary()?;
                self.advance();
                return Ok(v);
            }
            c if c == b'$' || c.is_ascii_digit() => {
                let part = |b: u8| if c == b'$' { b.is_ascii_alphanumeric() || b == b'_' } else { b.is_ascii_digit() };
                let mut name = String::from(c as char);
                while name.len() < 0x80 && part(self.byte(self.at)) {
                    name.push(self.byte(self.at) as char);
                    self.at += 1;
                }
                if name.len() >= 0x7f {
                    return Err(());
                }
                if c == b'$' { symbol(&name) } else { name.bytes().any(|d| d != b'0') }
            }
            b'!' => return Ok(None),
            _ => return Err(()),
        };
        self.advance();
        Ok(Some(value))
    }
}

/// The engine's condition symbols on a PC (`0x471d40`), then those the video mode sets
/// (`0x406fa0`: wide and high-definition, not anamorphic, 16:9 as at 1920 × 1080) and the
/// language (`0x59d920`). Any other symbol is false.
fn symbol(name: &str) -> bool {
    let name = name.strip_prefix('$').unwrap_or(name);
    ["WIN32", "WIN64", "WINDOWS", "PC", "WIN32WIDE", "WIN32HIDEF", "WIDESCREEN_16_9", LANGUAGE].iter().any(|s| name.eq_ignore_ascii_case(s))
}

/// The CRT's `atof`: the longest number at the start, after white space, or 0.
pub fn atof(s: &str) -> f64 {
    strtod(s).0
}

/// The CRT's `strtod`: the number at the start, after white space, and the bytes it took (0
/// when there is none).
pub fn strtod(s: &str) -> (f64, usize) {
    let t = s.trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r']);
    let skipped = s.len() - t.len();
    let b = t.as_bytes();
    let mut at = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let lower = t[at..].to_ascii_lowercase();
    if lower.starts_with("inf") || lower.starts_with("nan") {
        let v = if lower.starts_with("inf") { f64::INFINITY } else { f64::NAN };
        let len = if lower.starts_with("infinity") { 8 } else { 3 };
        return (if b[0] == b'-' { -v } else { v }, skipped + at + len);
    }
    let digits = |at: &mut usize| {
        let start = *at;
        while b.get(*at).is_some_and(u8::is_ascii_digit) {
            *at += 1;
        }
        *at - start
    };
    let mut n = digits(&mut at);
    if b.get(at) == Some(&b'.') {
        at += 1;
        n += digits(&mut at);
    }
    if n == 0 {
        return (0.0, 0);
    }
    if matches!(b.get(at), Some(b'e' | b'E')) {
        let mut e = at + 1 + usize::from(matches!(b.get(at + 1), Some(b'+' | b'-')));
        if digits(&mut e) > 0 {
            at = e;
        }
    }
    (t[..at].parse().unwrap_or(0.0), skipped + at)
}

/// The CRT's `strtol` in base 10: the number at the start, the bytes it took (0 when there is
/// none) and whether it was out of range.
fn strtol(s: &str) -> (i32, usize, bool) {
    let t = s.trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r']);
    let sign = usize::from(matches!(t.as_bytes().first(), Some(b'+' | b'-')));
    let digits = t[sign..].bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return (0, 0, false);
    }
    let n = t[sign..sign + digits].bytes().fold(0i64, |n, d| (n * 10 + (d - b'0') as i64).min(1 << 32));
    let n = if t.starts_with('-') { -n } else { n };
    let out = n.clamp(i32::MIN as i64, i32::MAX as i64);
    (out as i32, s.len() - t.len() + sign + digits, out != n)
}

/// The CRT's `atoi`: an optional sign and the digits at the start, after white space, or 0.
pub fn atoi(s: &str) -> i32 {
    strtol(s).0
}

/// What a value is kept as (`0x4765e0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Str,
    Int(i32),
    Float(f32),
    Uint64(u64),
}

/// A value's type, from its text: `0x` and 16 hex digits is a 64-bit integer; a float when
/// `strtod` reads further than `strtol` and to the end; an integer when `strtol` reads to the
/// end in range; else a string, as is an empty value.
pub fn kind(text: &str) -> Kind {
    if text.is_empty() {
        return Kind::Str;
    }
    if text.len() == 18 && text.starts_with("0x") {
        return Kind::Uint64(text.as_bytes()[2..].iter().fold(0u64, |n, &c| {
            let d = if c < b'a' { if b'@' < c { c - 7 } else { c } } else { c - 0x27 };
            n.wrapping_mul(16).wrapping_add(d.wrapping_sub(b'0') as u64)
        }));
    }
    let (i, i_end, overflow) = strtol(text);
    let (f, f_end) = strtod(text);
    if f_end > i_end && f_end == text.len() {
        Kind::Float(f as f32)
    } else if i_end == text.len() && !overflow {
        Kind::Int(i)
    } else {
        Kind::Str
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditions_read_as_the_engine_reads_them() {
        let t = |s: &str| evaluate_conditional(s);
        assert!(t("[$WINDOWS]") && t("[$PC]") && t("[!$GAMECONSOLE]") && t("[$ENGLISH]"));
        assert!(!t("[$GAMECONSOLE]") && !t("[!$ENGLISH]") && !t("[$NX || $NX_UI_PC]") && !t("[$JAPANESE || $TCHINESE]"));
        assert!(t("[!$GAMECONSOLE && $WIN32WIDE]") && !t("[!$GAMECONSOLE && !$WIN32WIDE]"));
        assert!(!t("[$GAMECONSOLE && ($GAMECONSOLEWIDE && !$ANAMORPHIC)]"));
        assert!(t("[$DURANGO || $WINDOWS]") && !t("[!$ENGLISH && !$PORTUGUESE && !$TCHINESE]"));
        assert!(!t("[$PC || $PC && $PS4]"), "same precedence, left to right");
        assert!(t("[1]") && !t("[0]") && !t("[]") && !t("[$PC &| $PC]"));
        assert!(!t("[$PC !$PC]"), "a ! drops what stood before it");
    }

    #[test]
    fn values_take_the_type_their_text_parses_as() {
        assert_eq!([kind("12"), kind("-3"), kind("1.5"), kind("1300.0"), kind("0 0 0 255"), kind("%100"), kind(""), kind("9999999999")], [Kind::Int(12), Kind::Int(-3), Kind::Float(1.5), Kind::Float(1300.0), Kind::Str, Kind::Str, Kind::Str, Kind::Str]);
        assert_eq!(kind("0x00000000000000ff"), Kind::Uint64(255));
        assert_eq!((atoi("564//-1568"), atoi(" -12x"), atof("-2.2"), atof("x")), (564, -12, -2.2, 0.0));
    }

    #[test]
    fn a_layout_keeps_order_repeats_and_true_conditions() {
        let text = b"\"f.res\"\r\n{\r\n\tA\r\n\t{\r\n\t\tControlName\t\tLabel // c\r\n\t\typos 0\r\n\t\typos 4\r\n\t\twide %100 [$GAMECONSOLE]\r\n\t\twide 40 [$WINDOWS]\r\n\t\tlabelText #SETTINGS\r\n\t\tlist\r\n\t\t{\r\n\t\t\t\"#SETTING_OFF\"\t0\r\n\t\t}\r\n\t}\r\n\tB [!$WIN32]\r\n\t{\r\n\t}\r\n\tC [$GAMECONSOLE && ($ENGLISH || $GAMECONSOLEWIDE)] { x 1 }\r\n}\r\n";
        let keys = parse(text);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].name, "f.res");
        let a = &keys[0].keys()[0];
        assert_eq!(keys[0].keys().len(), 1);
        assert_eq!(a.keys().iter().map(|k| k.name.as_str()).collect::<Vec<_>>(), ["ControlName", "ypos", "ypos", "wide", "labelText", "list"]);
        assert_eq!(a.str("YPOS"), Some("0"));
        assert_eq!(a.str("wide"), Some("40"));
        assert_eq!(a.str("labeltext"), Some("#SETTINGS"));
        assert_eq!(a.find("list").unwrap().str("#setting_off"), Some("0"));
    }
}
