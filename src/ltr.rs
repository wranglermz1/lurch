//! Reader for capture/drain's binary trace (`.ltr`); the format is described in drain.cpp.

use std::io::{self, Read};

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
    prev: Option<Record>,
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
        if &magic != b"lurchtr3" {
            return Err(bad("not a lurchtr3 trace"));
        }
        let mut sizes = [0; 6];
        for s in &mut sizes {
            *s = u32_of(&mut r)? as usize;
        }
        let mut map = [0; 32];
        r.read_exact(&mut map)?;
        let map = String::from_utf8_lossy(&map).trim_end_matches('\0').to_string();
        Ok(Reader { r, map, sizes, prev: None })
    }

    fn runs(&mut self, mut buf: Vec<u8>) -> io::Result<Vec<u8>> {
        loop {
            let off = u32_of(&mut self.r)? as usize;
            let len = u32_of(&mut self.r)? as usize;
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
        let mv_in = self.runs(base(|p| &p.mv_in, mv))?;
        let mv_out = self.runs(mv_in.clone())?;
        let player_in = self.runs(base(|p| &p.player_in, player))?;
        let player_out = self.runs(player_in.clone())?;
        let mut traces = Vec::with_capacity(ntraces);
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
                mv_out: Vec::new(),
                player_in: rec.player_in.clone(),
                player_out: Vec::new(),
                traces: Vec::new(),
            });
            rec
        }))
    }
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
