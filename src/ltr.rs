//! capture/drain's binary trace (`.ltr`). Version 3 is described in drain.cpp. Version 4
//! differs only in its references and run encoding, which makes it several times smaller:
//! a call's input snapshots are stored against the previous call's outputs (one call ends
//! where the next begins), each trace's ray and result against the previous trace's, and a
//! run is { u16 offset, u16 length, bytes }, terminated by length 0.

use std::io::{self, Read, Write};

pub struct Trace {
    pub slot: u32,
    pub mask: u32,
    pub ray: Vec<u8>,
    pub result: Vec<u8>,
}

pub struct Record {
    pub seq: u32,
    pub traces_lost: u32,
    pub globals: Vec<u8>,
    pub settings: Vec<u8>,
    pub mv_in: Vec<u8>,
    pub mv_out: Vec<u8>,
    pub player_in: Vec<u8>,
    pub player_out: Vec<u8>,
    pub traces: Vec<Trace>,
}

pub struct Reader<R> {
    r: R,
    pub map: String,
    sizes: [usize; 6],
    version: u8,
    prev: Option<Record>,
    prev_trace: (Vec<u8>, Vec<u8>),
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

fn u32_of<R: Read>(r: &mut R) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn vec3(b: &[u8]) -> [f32; 3] {
    [0, 4, 8].map(|o| f32::from_le_bytes(b[o..o + 4].try_into().unwrap()))
}

impl<R: Read> Reader<R> {
    pub fn new(mut r: R) -> io::Result<Self> {
        let mut magic = [0; 8];
        r.read_exact(&mut magic)?;
        let version = match &magic {
            b"lurchtr3" => 3,
            b"lurchtr4" => 4,
            _ => return Err(bad("not a lurchtr3 or lurchtr4 trace")),
        };
        let mut sizes = [0; 6];
        for s in &mut sizes {
            *s = u32_of(&mut r)? as usize;
        }
        let mut map = [0; 32];
        r.read_exact(&mut map)?;
        let map = String::from_utf8_lossy(&map).trim_end_matches('\0').to_string();
        let prev_trace = (vec![0; sizes[4]], vec![0; sizes[5]]);
        Ok(Reader { r, map, sizes, version, prev: None, prev_trace })
    }

    fn runs(&mut self, mut buf: Vec<u8>) -> io::Result<Vec<u8>> {
        loop {
            let (off, len) = if self.version == 3 {
                (u32_of(&mut self.r)? as usize, u32_of(&mut self.r)? as usize)
            } else {
                let mut b = [0; 4];
                self.r.read_exact(&mut b)?;
                (u16::from_le_bytes([b[0], b[1]]) as usize, u16::from_le_bytes([b[2], b[3]]) as usize)
            };
            if len == 0 {
                return Ok(buf);
            }
            let dst = buf.get_mut(off..off + len).ok_or_else(|| bad("run outside its snapshot"))?;
            self.r.read_exact(dst)?;
        }
    }

    fn record(&mut self, seq: u32) -> io::Result<Record> {
        let [player, mv, settings, globals, tray, tresult] = self.sizes;
        let ntraces = u32_of(&mut self.r)? as usize;
        let traces_lost = u32_of(&mut self.r)?;
        let prev = self.prev.take();
        let base = |f: fn(&Record) -> &Vec<u8>, n| prev.as_ref().map_or(vec![0; n], |p| f(p).clone());
        let globals = self.runs(base(|p| &p.globals, globals))?;
        let settings = self.runs(base(|p| &p.settings, settings))?;
        let v4 = self.version == 4;
        let mv_in = self.runs(base(if v4 { |p: &Record| &p.mv_out } else { |p: &Record| &p.mv_in }, mv))?;
        let mv_out = self.runs(mv_in.clone())?;
        let player_in = self.runs(base(if v4 { |p: &Record| &p.player_out } else { |p: &Record| &p.player_in }, player))?;
        let player_out = self.runs(player_in.clone())?;
        let mut traces = Vec::with_capacity(ntraces);
        if v4 {
            for _ in 0..ntraces {
                let slot = u32_of(&mut self.r)?;
                let mask = u32_of(&mut self.r)?;
                let ray = self.runs(self.prev_trace.0.clone())?;
                let result = self.runs(self.prev_trace.1.clone())?;
                self.prev_trace = (ray.clone(), result.clone());
                traces.push(Trace { slot, mask, ray, result });
            }
        } else {
            let mut raw = vec![0; 8 + tray + tresult];
            for _ in 0..ntraces {
                self.r.read_exact(&mut raw)?;
                traces.push(Trace {
                    slot: u32_at(&raw, 0),
                    mask: u32_at(&raw, 4),
                    ray: raw[8..8 + tray].to_vec(),
                    result: raw[8 + tray..].to_vec(),
                });
            }
        }
        Ok(Record { seq, traces_lost, globals, settings, mv_in, mv_out, player_in, player_out, traces })
    }
}

impl<R: Read> Iterator for Reader<R> {
    type Item = io::Result<Record>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut b = [0; 4];
        match self.r.read(&mut b[..1]) {
            Ok(0) => return None,
            Ok(_) => {}
            Err(e) => return Some(Err(e)),
        }
        if let Err(e) = self.r.read_exact(&mut b[1..]) {
            return Some(Err(e));
        }
        let rec = self.record(u32::from_le_bytes(b));
        Some(rec.map(|rec| {
            self.prev = Some(Record {
                seq: rec.seq,
                traces_lost: 0,
                globals: rec.globals.clone(),
                settings: rec.settings.clone(),
                mv_in: rec.mv_in.clone(),
                mv_out: rec.mv_out.clone(),
                player_in: rec.player_in.clone(),
                player_out: rec.player_out.clone(),
                traces: Vec::new(),
            });
            rec
        }))
    }
}

fn runs_v4(w: &mut impl Write, now: &[u8], reference: &[u8]) -> io::Result<()> {
    const MERGE_GAP: usize = 6;
    let n = now.len();
    let mut i = 0;
    while i < n {
        if now[i] == reference[i] {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        let mut same = 0;
        while j < n && j - i < 0xffff {
            if now[j] == reference[j] {
                same += 1;
            } else {
                same = 0;
            }
            j += 1;
            if same > MERGE_GAP {
                break;
            }
        }
        let end = j - same;
        w.write_all(&(i as u16).to_le_bytes())?;
        w.write_all(&((end - i) as u16).to_le_bytes())?;
        w.write_all(&now[i..end])?;
        i = end;
    }
    w.write_all(&[0; 4])
}

/// Writes version 4.
pub struct Writer<W: Write> {
    w: W,
    prev: Option<Record>,
    prev_trace: (Vec<u8>, Vec<u8>),
}

impl<W: Write> Writer<W> {
    pub fn new(mut w: W, map: &str, sizes: [u32; 6]) -> io::Result<Self> {
        w.write_all(b"lurchtr4")?;
        for s in sizes {
            w.write_all(&s.to_le_bytes())?;
        }
        let mut name = [0u8; 32];
        let b = map.as_bytes();
        name[..b.len().min(31)].copy_from_slice(&b[..b.len().min(31)]);
        w.write_all(&name)?;
        let prev_trace = (vec![0; sizes[4] as usize], vec![0; sizes[5] as usize]);
        Ok(Writer { w, prev: None, prev_trace })
    }

    pub fn write(&mut self, r: Record) -> io::Result<()> {
        let w = &mut self.w;
        w.write_all(&r.seq.to_le_bytes())?;
        w.write_all(&(r.traces.len() as u32).to_le_bytes())?;
        w.write_all(&r.traces_lost.to_le_bytes())?;
        let zero = |n| vec![0u8; n];
        let p = self.prev.as_ref();
        runs_v4(w, &r.globals, p.map_or(&zero(r.globals.len()), |p| &p.globals))?;
        runs_v4(w, &r.settings, p.map_or(&zero(r.settings.len()), |p| &p.settings))?;
        runs_v4(w, &r.mv_in, p.map_or(&zero(r.mv_in.len()), |p| &p.mv_out))?;
        runs_v4(w, &r.mv_out, &r.mv_in)?;
        runs_v4(w, &r.player_in, p.map_or(&zero(r.player_in.len()), |p| &p.player_out))?;
        runs_v4(w, &r.player_out, &r.player_in)?;
        for t in &r.traces {
            w.write_all(&t.slot.to_le_bytes())?;
            w.write_all(&t.mask.to_le_bytes())?;
            runs_v4(w, &t.ray, &self.prev_trace.0)?;
            runs_v4(w, &t.result, &self.prev_trace.1)?;
            self.prev_trace = (t.ray.clone(), t.result.clone());
        }
        self.prev = Some(r);
        Ok(())
    }

    pub fn finish(mut self) -> io::Result<W> {
        self.w.flush()?;
        Ok(self.w)
    }
}

pub fn sizes(r: &Record) -> [u32; 6] {
    let t = r.traces.first();
    [
        r.player_in.len() as u32,
        r.mv_in.len() as u32,
        r.settings.len() as u32,
        r.globals.len() as u32,
        t.map_or(0x68, |t| t.ray.len() as u32),
        t.map_or(0x180, |t| t.result.len() as u32),
    ]
}

pub fn open(path: &str) -> io::Result<Reader<io::BufReader<std::fs::File>>> {
    Reader::new(io::BufReader::new(std::fs::File::open(path)?))
}

pub fn f32_at(b: &[u8], off: usize) -> f32 {
    f32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}

pub fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}

pub fn vec3_at(b: &[u8], off: usize) -> [f32; 3] {
    vec3(&b[off..])
}
