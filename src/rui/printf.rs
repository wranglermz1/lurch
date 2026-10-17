//! C99 `vsnprintf` as the engine's RUI sprintf (`0x2fb460`) calls it: the Universal CRT's
//! `__stdio_common_vsprintf` with standard snprintf behaviour (`0x2292c0`). The arguments come
//! as the Windows x64 convention passes variadic ones: one 8-byte slot each, a float as the
//! bits of its double.
//!
//! Not modelled: `%n`, `%a`, wide strings (`%ls`, `%S`, `%C`), and the locale (the decimal
//! point is always `.`). A conversion the CRT rejects ends the call with `None`, as its -1.

use std::ffi::CStr;

/// Formats `fmt` with `args`, taken in order as the conversions ask.
pub fn format(fmt: &[u8], args: &[u64]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut next = args.iter().copied();
    let mut arg = || next.next().unwrap_or(0);
    let mut i = 0;
    while i < fmt.len() {
        let c = fmt[i];
        i += 1;
        if c != b'%' {
            out.push(c);
            continue;
        }
        let mut f = Spec::default();
        while let Some(&c) = fmt.get(i) {
            match c {
                b'-' => f.left = true,
                b'+' => f.plus = true,
                b' ' => f.space = true,
                b'#' => f.alt = true,
                b'0' => f.zero = true,
                _ => break,
            }
            i += 1;
        }
        if fmt.get(i) == Some(&b'*') {
            let w = arg() as i32;
            f.left |= w < 0;
            f.width = w.unsigned_abs() as usize;
            i += 1;
        } else {
            while let Some(d) = fmt.get(i).filter(|c| c.is_ascii_digit()) {
                f.width = f.width * 10 + (d - b'0') as usize;
                i += 1;
            }
        }
        if fmt.get(i) == Some(&b'.') {
            i += 1;
            let mut p = 0usize;
            if fmt.get(i) == Some(&b'*') {
                let v = arg() as i32;
                i += 1;
                f.precision = (v >= 0).then_some(v as usize);
            } else {
                while let Some(d) = fmt.get(i).filter(|c| c.is_ascii_digit()) {
                    p = p * 10 + (d - b'0') as usize;
                    i += 1;
                }
                f.precision = Some(p);
            }
        }
        let rest = &fmt[i..];
        let (bits, skip) = if rest.starts_with(b"I64") || rest.starts_with(b"ll") {
            (64, if rest[0] == b'I' { 3 } else { 2 })
        } else if rest.starts_with(b"I32") {
            (32, 3)
        } else if rest.starts_with(b"hh") {
            (8, 2)
        } else {
            match rest.first() {
                Some(b'h') => (16, 1),
                Some(b'l' | b'L' | b'w') => (32, 1),
                Some(b'I' | b'z' | b'j' | b't') => (64, 1),
                _ => (32, 0),
            }
        };
        i += skip;
        let &conv = fmt.get(i)?;
        i += 1;
        let body = match conv {
            b'%' => {
                out.push(b'%');
                continue;
            }
            b'd' | b'i' => {
                let v = arg();
                let v = match bits {
                    8 => v as i8 as i64,
                    16 => v as i16 as i64,
                    32 => v as i32 as i64,
                    _ => v as i64,
                };
                let sign = if v < 0 { "-" } else if f.plus { "+" } else if f.space { " " } else { "" };
                f.integer(sign, v.unsigned_abs().to_string())
            }
            b'u' | b'x' | b'X' | b'o' => {
                let v = match bits {
                    8 => arg() as u8 as u64,
                    16 => arg() as u16 as u64,
                    32 => arg() as u32 as u64,
                    _ => arg(),
                };
                let (digits, prefix) = match conv {
                    b'u' => (v.to_string(), ""),
                    b'x' => (format!("{v:x}"), if f.alt && v != 0 { "0x" } else { "" }),
                    b'X' => (format!("{v:X}"), if f.alt && v != 0 { "0X" } else { "" }),
                    _ => (format!("{v:o}"), if f.alt && v != 0 { "0" } else { "" }),
                };
                f.integer(prefix, digits)
            }
            b'c' => {
                f.zero = false;
                f.pad(String::new(), vec![arg() as u8])
            }
            b's' => {
                let p = arg() as *const std::ffi::c_char;
                let s = if p.is_null() {
                    b"(null)".to_vec()
                } else {
                    // SAFETY: the element passes a NUL-terminated string for `%s`.
                    unsafe { CStr::from_ptr(p) }.to_bytes().to_vec()
                };
                f.zero = false;
                let n = f.precision.map_or(s.len(), |p| p.min(s.len()));
                f.pad(String::new(), s[..n].to_vec())
            }
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' => f.float(conv, f64::from_bits(arg())),
            b'p' => {
                f.zero = false;
                f.pad(String::new(), format!("{:016X}", arg()).into_bytes())
            }
            _ => return None,
        };
        out.extend(body);
    }
    Some(out)
}

#[derive(Default)]
struct Spec {
    left: bool,
    plus: bool,
    space: bool,
    alt: bool,
    zero: bool,
    width: usize,
    precision: Option<usize>,
}

impl Spec {
    /// `prefix` then `body`, padded to the width: on the right when left-justified, else with
    /// zeros between them when asked, else with spaces before both.
    fn pad(&self, prefix: String, body: Vec<u8>) -> Vec<u8> {
        let len = prefix.len() + body.len();
        let fill = self.width.saturating_sub(len);
        let mut out = Vec::with_capacity(len + fill);
        if self.left {
            out.extend(prefix.bytes());
            out.extend(body);
            out.resize(out.len() + fill, b' ');
        } else if self.zero {
            out.extend(prefix.bytes());
            out.resize(out.len() + fill, b'0');
            out.extend(body);
        } else {
            out.resize(fill, b' ');
            out.extend(prefix.bytes());
            out.extend(body);
        }
        out
    }

    /// An integer's digits at least the precision long; a precision drops the `0` flag, and
    /// a zero printed at precision zero prints nothing.
    fn integer(&mut self, prefix: &str, digits: String) -> Vec<u8> {
        let digits = match self.precision {
            Some(0) if digits == "0" => String::new(),
            Some(p) => format!("{digits:0>p$}"),
            None => digits,
        };
        if self.precision.is_some() {
            self.zero = false;
        }
        self.pad(prefix.to_string(), digits.into_bytes())
    }

    fn float(&mut self, conv: u8, v: f64) -> Vec<u8> {
        let upper = conv.is_ascii_uppercase();
        let sign = if v.is_sign_negative() { "-" } else if self.plus { "+" } else if self.space { " " } else { "" };
        if !v.is_finite() {
            self.zero = false;
            let s = if v.is_infinite() {
                "inf"
            } else if v.is_sign_negative() && v.to_bits() & 0x000f_ffff_ffff_ffff == 0x0008_0000_0000_0000 {
                "nan(ind)"
            } else {
                "nan"
            };
            let s = if upper { s.to_uppercase() } else { s.to_string() };
            return self.pad(sign.to_string(), s.into_bytes());
        }
        let a = v.abs();
        let p = self.precision.unwrap_or(6);
        let body = match conv.to_ascii_lowercase() {
            b'f' => fixed(a, p, self.alt),
            b'e' => exponent(a, p, self.alt),
            _ => {
                let p = p.max(1);
                let x = exponent_of(a, p - 1);
                let s = if (-4..p as i32).contains(&x) { fixed(a, (p as i32 - 1 - x) as usize, self.alt) } else { exponent(a, p - 1, self.alt) };
                if self.alt { s } else { trim_zeros(&s) }
            }
        };
        let body = if upper { body.to_uppercase() } else { body };
        self.pad(sign.to_string(), body.into_bytes())
    }
}

fn fixed(a: f64, p: usize, alt: bool) -> String {
    let s = format!("{a:.p$}");
    if alt && p == 0 { s + "." } else { s }
}

/// `d.ddde+XX`: at least two exponent digits.
fn exponent(a: f64, p: usize, alt: bool) -> String {
    let s = format!("{a:.p$e}");
    let (m, e) = s.split_once('e').unwrap();
    let e: i32 = e.parse().unwrap();
    let m = if alt && p == 0 { format!("{m}.") } else { m.to_string() };
    format!("{m}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
}

/// The exponent `%e` would print at precision `p`, after its rounding.
fn exponent_of(a: f64, p: usize) -> i32 {
    let s = format!("{a:.p$e}");
    s.split_once('e').unwrap().1.parse().unwrap()
}

/// `%g` without `#`: no trailing zeros after the point, and no point left bare.
fn trim_zeros(s: &str) -> String {
    let (m, e) = match s.find('e') {
        Some(at) => s.split_at(at),
        None => (s, ""),
    };
    let m = if m.contains('.') { m.trim_end_matches('0').trim_end_matches('.') } else { m };
    format!("{m}{e}")
}

#[cfg(test)]
mod tests {
    use super::format;

    fn f(fmt: &str, args: &[u64]) -> String {
        String::from_utf8(format(fmt.as_bytes(), args).unwrap()).unwrap()
    }

    #[test]
    fn formats_as_the_crt_does() {
        let d = |x: f64| x.to_bits();
        assert_eq!(f("%i%%", &[42]), "42%");
        assert_eq!(f("%02i:%02i", &[7, (-3i32) as u32 as u64]), "07:-3");
        assert_eq!(f("%03i|%2i|%06i|%-4i|", &[5, 5, (-42i32) as u32 as u64, 9]), "005| 5|-00042|9   |");
        assert_eq!(f("%u %x %X %#x %o", &[u32::MAX as u64, 255, 255, 255, 8]), "4294967295 ff FF 0xff 10");
        assert_eq!(f("%.0f %.0f %.0f", &[d(0.5), d(1.5), d(2.5)]), "0 2 2");
        assert_eq!(f("%05.2f|%.2f|%04.1f|%+05.1f|%02.0f", &[d(3.14259), d(-0.005), d(9.96), d(2.25), d(7.0)]), "03.14|-0.01|10.0|+02.2|07");
        assert_eq!(f("%f %.1f", &[d(1.0 / 3.0), d(0.05)]), "0.333333 0.1");
        assert_eq!(f("%.8g|%g|%g|%g|%G", &[d(1.0 / 3.0), d(100000.0), d(1e6), d(1e-5), d(1e-5)]), "0.33333333|100000|1e+06|1e-05|1E-05");
        assert_eq!(f("%g|%#g|%g", &[d(0.0001), d(1.0), d(123456789.0)]), "0.0001|1.00000|1.23457e+08");
        assert_eq!(f("[%s|%5s|%-5s|%.2s]", &[c"ab".as_ptr() as u64, c"ab".as_ptr() as u64, c"ab".as_ptr() as u64, c"abc".as_ptr() as u64]), "[ab|   ab|ab   |ab]");
        assert_eq!(f("%f %e", &[d(f64::INFINITY), d(-0.0)]), "inf -0.000000e+00");
        assert!(format(b"%y", &[]).is_none());
    }
}
