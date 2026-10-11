//! Zstandard decompression (RFC 8878). R5Reloaded rebuilds its paks with it: header flag
//! `0x8000`, the 0x80-byte header in the clear and the rest one zstd stream. Dictionaries and
//! checksum verification are not supported; neither occurs in the install.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub &'static str);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "zstd: {}", self.0)
    }
}

type Result<T> = std::result::Result<T, Error>;

const MAGIC: u32 = 0xFD2F_B528;

pub fn decompress(src: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    decompress_into(src, &mut out)?;
    Ok(out)
}

/// Appends every frame in `src` to `out`.
pub fn decompress_into(mut src: &[u8], out: &mut Vec<u8>) -> Result<()> {
    while !src.is_empty() {
        let magic = le(src, 0, 4)? as u32;
        if magic & 0xFFFF_FFF0 == 0x184D_2A50 {
            let size = le(src, 4, 4)? as usize;
            src = src.get(8 + size..).ok_or(Error("truncated skippable frame"))?;
        } else if magic == MAGIC {
            src = frame(&src[4..], out)?;
        } else {
            return Err(Error("bad magic"));
        }
    }
    Ok(())
}

fn le(src: &[u8], at: usize, n: usize) -> Result<u64> {
    let b = src.get(at..at + n).ok_or(Error("truncated"))?;
    Ok(b.iter().rev().fold(0, |v, &x| v << 8 | x as u64))
}

#[derive(Default)]
struct State {
    reps: [usize; 3],
    huf: Option<Huffman>,
    ll: Option<Fse>,
    of: Option<Fse>,
    ml: Option<Fse>,
    lits: Vec<u8>,
}

fn frame<'a>(src: &'a [u8], out: &mut Vec<u8>) -> Result<&'a [u8]> {
    let fhd = *src.first().ok_or(Error("truncated"))?;
    if fhd & 8 != 0 {
        return Err(Error("reserved bit set"));
    }
    let single = fhd & 0x20 != 0;
    let mut at = 1 + usize::from(!single);
    at += [0, 1, 2, 4][(fhd & 3) as usize];
    let fcs_len = match fhd >> 6 {
        0 => usize::from(single),
        1 => 2,
        2 => 4,
        _ => 8,
    };
    if fcs_len > 0 {
        let fcs = le(src, at, fcs_len)? + if fcs_len == 2 { 256 } else { 0 };
        out.reserve(fcs as usize);
    }
    at += fcs_len;
    let start = out.len();
    let mut st = State { reps: [1, 4, 8], ..State::default() };
    loop {
        let h = le(src, at, 3)? as usize;
        at += 3;
        let size = h >> 3;
        match (h >> 1) & 3 {
            0 => {
                out.extend_from_slice(src.get(at..at + size).ok_or(Error("truncated raw block"))?);
                at += size;
            }
            1 => {
                let b = *src.get(at).ok_or(Error("truncated rle block"))?;
                out.resize(out.len() + size, b);
                at += 1;
            }
            2 => {
                block(src.get(at..at + size).ok_or(Error("truncated block"))?, out, start, &mut st)?;
                at += size;
            }
            _ => return Err(Error("reserved block type")),
        }
        if h & 1 != 0 {
            break;
        }
    }
    if fhd & 4 != 0 {
        at += 4;
    }
    src.get(at..).ok_or(Error("truncated checksum"))
}

fn block(src: &[u8], out: &mut Vec<u8>, start: usize, st: &mut State) -> Result<()> {
    let rest = literals(src, st)?;
    sequences(&src[rest..], out, start, st)
}

/// Decodes the literals section into `st.lits`; returns where the sequences section starts.
fn literals(src: &[u8], st: &mut State) -> Result<usize> {
    let b0 = *src.first().ok_or(Error("empty block"))? as usize;
    let (ty, sf) = (b0 & 3, (b0 >> 2) & 3);
    st.lits.clear();
    if ty < 2 {
        let (size, hl) = match sf {
            0 | 2 => (b0 >> 3, 1),
            1 => ((le(src, 0, 2)? as usize) >> 4, 2),
            _ => ((le(src, 0, 3)? as usize) >> 4, 3),
        };
        if ty == 0 {
            st.lits.extend_from_slice(src.get(hl..hl + size).ok_or(Error("truncated literals"))?);
            return Ok(hl + size);
        }
        let b = *src.get(hl).ok_or(Error("truncated literals"))?;
        st.lits.resize(size, b);
        return Ok(hl + 1);
    }
    let (regen, comp, hl, four) = match sf {
        0 | 1 => {
            let v = le(src, 0, 3)? as usize;
            ((v >> 4) & 0x3FF, (v >> 14) & 0x3FF, 3, sf == 1)
        }
        2 => {
            let v = le(src, 0, 4)? as usize;
            ((v >> 4) & 0x3FFF, (v >> 18) & 0x3FFF, 4, true)
        }
        _ => {
            let v = le(src, 0, 5)? as usize;
            ((v >> 4) & 0x3_FFFF, (v >> 22) & 0x3_FFFF, 5, true)
        }
    };
    let mut data = src.get(hl..hl + comp).ok_or(Error("truncated literals"))?;
    if ty == 2 {
        let (huf, used) = Huffman::read(data)?;
        st.huf = Some(huf);
        data = &data[used..];
    }
    let huf = st.huf.as_ref().ok_or(Error("treeless literals without a table"))?;
    st.lits.reserve(regen);
    if four {
        let s1 = le(data, 0, 2)? as usize;
        let s2 = le(data, 2, 2)? as usize;
        let s3 = le(data, 4, 2)? as usize;
        let body = data.get(6..).ok_or(Error("truncated jump table"))?;
        let s4 = body.len().checked_sub(s1 + s2 + s3).ok_or(Error("bad jump table"))?;
        let seg = regen.div_ceil(4);
        let last = regen.checked_sub(3 * seg).ok_or(Error("bad literal size"))?;
        let mut at = 0;
        for (size, n) in [(s1, seg), (s2, seg), (s3, seg), (s4, last)] {
            huf.decode(&body[at..at + size], n, &mut st.lits)?;
            at += size;
        }
    } else {
        huf.decode(data, regen, &mut st.lits)?;
    }
    Ok(hl + comp)
}

/// A bitstream read from its end, as Huffman and FSE streams are written.
struct BackBits<'a> {
    d: &'a [u8],
    /// Bits not yet read; negative once reads have gone past the start.
    pos: isize,
}

impl<'a> BackBits<'a> {
    fn new(d: &'a [u8]) -> Result<Self> {
        let last = *d.last().ok_or(Error("empty bitstream"))?;
        if last == 0 {
            return Err(Error("bitstream without end mark"));
        }
        Ok(BackBits { d, pos: ((d.len() - 1) * 8) as isize + 7 - last.leading_zeros() as isize })
    }

    fn load(&self, byte: usize) -> u64 {
        if let Some(b) = self.d.get(byte..byte + 8) {
            return u64::from_le_bytes(b.try_into().unwrap());
        }
        let mut b = [0u8; 8];
        let n = self.d.len().saturating_sub(byte).min(8);
        b[..n].copy_from_slice(&self.d[byte..byte + n]);
        u64::from_le_bytes(b)
    }

    fn peek(&self, n: u32) -> u64 {
        let p = self.pos - n as isize;
        let mask = (1u64 << n) - 1;
        if p >= 0 {
            (self.load((p >> 3) as usize) >> (p & 7)) & mask
        } else {
            let avail = self.pos;
            if avail <= 0 {
                return 0;
            }
            (self.load(0) & ((1u64 << avail) - 1)) << -p
        }
    }

    fn read(&mut self, n: u32) -> u64 {
        if n == 0 {
            return 0;
        }
        let v = self.peek(n);
        self.pos -= n as isize;
        v
    }

    fn overflowed(&self) -> bool {
        self.pos < 0
    }
}

struct Huffman {
    max_bits: u32,
    /// Indexed by the next `max_bits` bits: (symbol, code length).
    table: Vec<(u8, u8)>,
}

impl Huffman {
    /// Reads a tree description; returns the table and the bytes it took.
    fn read(src: &[u8]) -> Result<(Huffman, usize)> {
        let hb = *src.first().ok_or(Error("truncated huffman header"))? as usize;
        let mut weights = [0u8; 256];
        let (mut n, used);
        if hb < 128 {
            let comp = src.get(1..1 + hb).ok_or(Error("truncated huffman weights"))?;
            let (fse, desc) = Fse::read(comp, 255, 6)?;
            let mut br = BackBits::new(&comp[desc..])?;
            let (mut s1, mut s2) = (br.read(fse.log) as usize, br.read(fse.log) as usize);
            n = 0;
            loop {
                if n >= 254 {
                    return Err(Error("too many huffman weights"));
                }
                weights[n] = fse.symbol(s1);
                n += 1;
                s1 = fse.next(s1, &mut br);
                if br.overflowed() {
                    weights[n] = fse.symbol(s2);
                    n += 1;
                    break;
                }
                weights[n] = fse.symbol(s2);
                n += 1;
                s2 = fse.next(s2, &mut br);
                if br.overflowed() {
                    weights[n] = fse.symbol(s1);
                    n += 1;
                    break;
                }
            }
            used = 1 + hb;
        } else {
            n = hb - 127;
            let bytes = src.get(1..1 + n.div_ceil(2)).ok_or(Error("truncated huffman weights"))?;
            for (i, w) in weights.iter_mut().take(n).enumerate() {
                *w = if i % 2 == 0 { bytes[i / 2] >> 4 } else { bytes[i / 2] & 15 };
            }
            used = 1 + n.div_ceil(2);
        }
        let total: u32 = weights[..n].iter().filter(|&&w| w > 0).map(|&w| 1u32 << (w - 1)).sum();
        if total == 0 {
            return Err(Error("empty huffman tree"));
        }
        let max_bits = 32 - total.leading_zeros();
        let left = (1u32 << max_bits) - total;
        if !left.is_power_of_two() || max_bits > 11 {
            return Err(Error("bad huffman weights"));
        }
        weights[n] = (left.trailing_zeros() + 1) as u8;
        n += 1;
        let mut start = [0u32; 13];
        let mut next = 0;
        for (w, first) in start.iter_mut().enumerate().take(max_bits as usize + 1).skip(1) {
            *first = next;
            next += weights[..n].iter().filter(|&&x| x as usize == w).count() as u32 * (1 << (w - 1));
        }
        let mut table = vec![(0u8, 0u8); 1 << max_bits];
        for (s, &w) in weights[..n].iter().enumerate() {
            if w == 0 {
                continue;
            }
            let span = 1u32 << (w - 1);
            let at = start[w as usize] as usize;
            table[at..at + span as usize].fill((s as u8, (max_bits + 1 - w as u32) as u8));
            start[w as usize] += span;
        }
        Ok((Huffman { max_bits, table }, used))
    }

    fn decode(&self, src: &[u8], n: usize, out: &mut Vec<u8>) -> Result<()> {
        let mut br = BackBits::new(src)?;
        for _ in 0..n {
            let (s, nb) = self.table[br.peek(self.max_bits) as usize];
            out.push(s);
            br.pos -= nb as isize;
        }
        if br.pos != 0 {
            return Err(Error("huffman stream not consumed exactly"));
        }
        Ok(())
    }
}

#[derive(Clone)]
struct Fse {
    log: u32,
    /// Per state: symbol, bits to read, base of the next state.
    table: Vec<(u8, u8, u16)>,
}

impl Fse {
    fn symbol(&self, state: usize) -> u8 {
        self.table[state].0
    }

    fn next(&self, state: usize, br: &mut BackBits) -> usize {
        let (_, nb, base) = self.table[state];
        base as usize + br.read(nb as u32) as usize
    }

    /// Reads a table description; returns the table and the bytes it took.
    fn read(src: &[u8], max_symbol: usize, max_log: u32) -> Result<(Fse, usize)> {
        let mut bits = 0usize;
        let get = |bits: usize, n: u32| -> u32 {
            let mut v = 0u32;
            for i in 0..n as usize {
                let b = bits + i;
                v |= ((src.get(b / 8).copied().unwrap_or(0) >> (b % 8)) as u32 & 1) << i;
            }
            v
        };
        let log = get(0, 4) + 5;
        bits += 4;
        if log > max_log {
            return Err(Error("fse accuracy too high"));
        }
        let mut norm = [0i32; 256];
        let mut remaining = (1i32 << log) + 1;
        let mut threshold = 1i32 << log;
        let mut nb = log + 1;
        let mut sym = 0;
        while remaining > 1 && sym <= max_symbol {
            let max = 2 * threshold - 1 - remaining;
            let low = get(bits, nb - 1) as i32;
            let mut count = if low < max {
                bits += nb as usize - 1;
                low
            } else {
                let v = get(bits, nb) as i32;
                bits += nb as usize;
                if v >= threshold { v - max } else { v }
            };
            count -= 1;
            remaining -= count.abs();
            norm[sym] = count;
            sym += 1;
            if count == 0 {
                loop {
                    let r = get(bits, 2) as usize;
                    bits += 2;
                    sym += r;
                    if r != 3 {
                        break;
                    }
                }
            }
            while remaining < threshold {
                nb -= 1;
                threshold >>= 1;
            }
        }
        if remaining != 1 || sym > max_symbol + 1 || bits.div_ceil(8) > src.len() {
            return Err(Error("bad fse table description"));
        }
        Ok((Fse::build(&norm[..sym], log)?, bits.div_ceil(8)))
    }

    fn build(norm: &[i32], log: u32) -> Result<Fse> {
        let size = 1usize << log;
        let mut table = vec![(0u8, 0u8, 0u16); size];
        let mut high = size - 1;
        let mut next = vec![0u32; norm.len()];
        for (s, &p) in norm.iter().enumerate() {
            if p == -1 {
                table[high].0 = s as u8;
                high = high.wrapping_sub(1);
                next[s] = 1;
            } else {
                next[s] = p.max(0) as u32;
            }
        }
        let step = (size >> 1) + (size >> 3) + 3;
        let mut pos = 0;
        for (s, &p) in norm.iter().enumerate() {
            for _ in 0..p.max(0) {
                table[pos].0 = s as u8;
                loop {
                    pos = (pos + step) & (size - 1);
                    if pos <= high {
                        break;
                    }
                }
            }
        }
        if pos != 0 {
            return Err(Error("fse table did not spread evenly"));
        }
        for e in table.iter_mut() {
            let n = next[e.0 as usize];
            next[e.0 as usize] += 1;
            let nb = log - (31 - n.leading_zeros());
            e.1 = nb as u8;
            e.2 = ((n << nb) as usize - size) as u16;
        }
        Ok(Fse { log, table })
    }

    fn rle(symbol: u8) -> Fse {
        Fse { log: 0, table: vec![(symbol, 0, 0)] }
    }
}

const LL_DEFAULT: [i32; 36] = [4, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 2, 1, 1, 1, 1, 1, -1, -1, -1, -1];
const ML_DEFAULT: [i32; 53] = [
    1, 4, 3, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, -1, -1, -1, -1, -1, -1, -1,
];
const OF_DEFAULT: [i32; 29] = [1, 1, 1, 1, 1, 1, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, -1, -1, -1, -1, -1];

const LL_BASE: [u32; 36] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 18, 20, 22, 24, 28, 32, 40, 48, 64, 128, 256, 512, 1024, 2048, 4096, 8192,
    16384, 32768, 65536,
];
const LL_BITS: [u8; 36] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 3, 3, 4, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
const ML_BASE: [u32; 53] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 37, 39,
    41, 43, 47, 51, 59, 67, 83, 99, 131, 259, 515, 1027, 2051, 4099, 8195, 16387, 32771, 65539,
];
const ML_BITS: [u8; 53] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 7, 8, 9,
    10, 11, 12, 13, 14, 15, 16,
];

fn table(mode: u8, src: &[u8], at: &mut usize, prev: &mut Option<Fse>, default: (&[i32], u32), max_symbol: usize, max_log: u32) -> Result<()> {
    *prev = Some(match mode {
        0 => Fse::build(default.0, default.1)?,
        1 => {
            let s = *src.get(*at).ok_or(Error("truncated rle table"))?;
            *at += 1;
            Fse::rle(s)
        }
        2 => {
            let (f, used) = Fse::read(src.get(*at..).ok_or(Error("truncated fse table"))?, max_symbol, max_log)?;
            *at += used;
            f
        }
        _ => prev.take().ok_or(Error("repeat table without a previous one"))?,
    });
    Ok(())
}

fn sequences(src: &[u8], out: &mut Vec<u8>, start: usize, st: &mut State) -> Result<()> {
    let b0 = *src.first().ok_or(Error("truncated sequences"))? as usize;
    let (n, mut at) = match b0 {
        0 => (0, 1),
        1..=127 => (b0, 1),
        128..=254 => (((b0 - 128) << 8) + le(src, 1, 1)? as usize, 2),
        _ => (le(src, 1, 2)? as usize + 0x7F00, 3),
    };
    if n == 0 {
        out.extend_from_slice(&st.lits);
        return Ok(());
    }
    let modes = *src.get(at).ok_or(Error("truncated sequences"))?;
    at += 1;
    table(modes >> 6, src, &mut at, &mut st.ll, (&LL_DEFAULT, 6), 35, 9)?;
    table((modes >> 4) & 3, src, &mut at, &mut st.of, (&OF_DEFAULT, 5), 31, 8)?;
    table((modes >> 2) & 3, src, &mut at, &mut st.ml, (&ML_DEFAULT, 6), 52, 9)?;
    let (ll, of, ml) = (st.ll.as_ref().unwrap(), st.of.as_ref().unwrap(), st.ml.as_ref().unwrap());
    let mut br = BackBits::new(src.get(at..).ok_or(Error("truncated sequences"))?)?;
    let mut ls = br.read(ll.log) as usize;
    let mut os = br.read(of.log) as usize;
    let mut ms = br.read(ml.log) as usize;
    let mut lit = 0usize;
    let reps = &mut st.reps;
    for i in 0..n {
        let oc = of.symbol(os) as u32;
        let mc = ml.symbol(ms) as usize;
        let lc = ll.symbol(ls) as usize;
        if oc > 31 || mc >= ML_BASE.len() || lc >= LL_BASE.len() {
            return Err(Error("bad sequence code"));
        }
        let ov = (1u64 << oc) as usize + br.read(oc) as usize;
        let mlen = ML_BASE[mc] as usize + br.read(ML_BITS[mc] as u32) as usize;
        let llen = LL_BASE[lc] as usize + br.read(LL_BITS[lc] as u32) as usize;
        let offset = if ov > 3 {
            let o = ov - 3;
            *reps = [o, reps[0], reps[1]];
            o
        } else {
            let idx = ov - 1 + usize::from(llen == 0);
            match idx {
                0 => reps[0],
                1 => {
                    let o = reps[1];
                    *reps = [o, reps[0], reps[2]];
                    o
                }
                2 => {
                    let o = reps[2];
                    *reps = [o, reps[0], reps[1]];
                    o
                }
                _ => {
                    let o = reps[0].checked_sub(1).filter(|&o| o > 0).ok_or(Error("zero offset"))?;
                    *reps = [o, reps[0], reps[1]];
                    o
                }
            }
        };
        out.extend_from_slice(st.lits.get(lit..lit + llen).ok_or(Error("literals overrun"))?);
        lit += llen;
        if offset > out.len() - start {
            return Err(Error("offset before frame start"));
        }
        let from = out.len() - offset;
        let mut left = mlen;
        while left > 0 {
            let k = left.min(out.len() - from);
            out.extend_from_within(from..from + k);
            left -= k;
        }
        if i + 1 < n {
            ls = ll.next(ls, &mut br);
            ms = ml.next(ms, &mut br);
            os = of.next(os, &mut br);
        }
    }
    if br.pos != 0 {
        return Err(Error("sequence stream not consumed exactly"));
    }
    out.extend_from_slice(&st.lits[lit..]);
    Ok(())
}
