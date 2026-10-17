//! Level collision: a swept player hull against triangles, answering the movement code's
//! traces the way the engine's do (Source conventions: a 1/32 skin, endpos = start + f·delta).
//! Triangles come from memory or from a HAVM `.mesh` file (format in HAVM `src/game/mapmesh.hpp`).

use crate::sim::math::{cross, dot, len, mad, scale, sub, V3};
use crate::sim::zipline::Zipline;
use crate::sim::{HullQuery, Trace, Tracer};
use std::fs::File;
use std::io;

const DIST_EPSILON: f32 = 0.031_25;
const WORLD_ENTITY: u64 = 1;
/// What a hit on the world reports: solid, as the recorded maps' brushes do.
const CONTENTS_SOLID: u32 = 0x1;
const TAG_WALK: u8 = 1;
const COARSE_N: usize = 48;
const HEADER: u64 = 64;

pub type Tri = [V3; 3];

struct Grid {
    cell: f32,
    min: [f32; 2],
    n: [usize; 2],
    /// nx·ny columns, then the oversize bucket, then the end.
    starts: Vec<u32>,
    index_at: u64,
    /// The oversize triangles bucketed again on a coarse grid, as HAVM does at load.
    coarse: Vec<Vec<u32>>,
    coarse_cell: [f32; 2],
}

/// The columns an in-memory world's triangles stand in, so a trace tests only those near it.
/// A triangle wider than `MEM_BIG` columns goes in `big`, which every trace tests.
struct MemGrid {
    min: [f32; 2],
    n: [usize; 2],
    cells: Vec<Vec<u32>>,
    big: Vec<u32>,
}

const MEM_CELL: f32 = 256.0;
const MEM_MAX_N: usize = 512;
const MEM_BIG: usize = 64;

impl MemGrid {
    fn new(tris: &[Tri]) -> Self {
        let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
        for v in tris.iter().flatten() {
            for i in 0..2 {
                min[i] = min[i].min(v[i]);
                max[i] = max[i].max(v[i]);
            }
        }
        let n = [0, 1].map(|i| if min[i] <= max[i] { (((max[i] - min[i]) / MEM_CELL) as usize + 1).min(MEM_MAX_N) } else { 1 });
        let mut g = MemGrid { min, n, cells: vec![Vec::new(); n[0] * n[1]], big: Vec::new() };
        for (k, t) in tris.iter().enumerate() {
            let lo = [0, 1].map(|i| t[0][i].min(t[1][i]).min(t[2][i]));
            let hi = [0, 1].map(|i| t[0][i].max(t[1][i]).max(t[2][i]));
            let (x0, y0, x1, y1) = g.span(lo, hi);
            if (x1 - x0 + 1) * (y1 - y0 + 1) > MEM_BIG {
                g.big.push(k as u32);
                continue;
            }
            for y in y0..=y1 {
                for x in x0..=x1 {
                    g.cells[y * n[0] + x].push(k as u32);
                }
            }
        }
        g
    }

    fn span(&self, lo: [f32; 2], hi: [f32; 2]) -> (usize, usize, usize, usize) {
        let col = |v: f32, i: usize| ((((v - self.min[i]) / MEM_CELL).floor()) as i64).clamp(0, self.n[i] as i64 - 1) as usize;
        (col(lo[0], 0), col(lo[1], 1), col(hi[0], 0), col(hi[1], 1))
    }

    /// The triangles in the columns `lo`–`hi` crosses, each once, in the world's order.
    fn near(&self, lo: V3, hi: V3) -> Vec<u32> {
        let (x0, y0, x1, y1) = self.span([lo[0], lo[1]], [hi[0], hi[1]]);
        let mut out = self.big.clone();
        for y in y0..=y1 {
            for x in x0..=x1 {
                out.extend(&self.cells[y * self.n[0] + x]);
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

enum Source {
    Mem(Vec<Tri>, MemGrid),
    // ponytail: one pread per candidate triangle; mmap or a page cache if traces get slow.
    File(File, Grid, Option<Vec<u8>>),
}

pub struct World {
    src: Source,
    pub ziplines: Vec<Zipline>,
}

impl World {
    pub fn from_tris(tris: Vec<Tri>) -> Self {
        let grid = MemGrid::new(&tris);
        World { src: Source::Mem(tris, grid), ziplines: Vec::new() }
    }

    /// A flat floor at height `z`, wide enough for any arena.
    pub fn floor(z: f32) -> Self {
        let r = 65536.0;
        let (a, b, c, d) = ([-r, -r, z], [r, -r, z], [r, r, z], [-r, r, z]);
        World::from_tris(vec![[a, b, c], [a, c, d]])
    }

    pub fn open(path: &str) -> io::Result<Self> {
        let f = File::open(path)?;
        let mut h = [0u8; HEADER as usize];
        read_exact_at(&f, &mut h, 0)?;
        let u = |o: usize| u32::from_le_bytes(h[o..o + 4].try_into().unwrap());
        let fl = |o: usize| f32::from_le_bytes(h[o..o + 4].try_into().unwrap());
        if &h[..8] != b"HAVMMSH\0" || u(8) != 1 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not a HAVMMSH v1 mesh"));
        }
        let tri_n = u(12) as u64;
        let n = [u(32) as usize, u(36) as usize];
        let cells = n[0] * n[1] + 2;
        let starts_at = HEADER + tri_n * 36;
        let mut raw = vec![0u8; cells * 4];
        read_exact_at(&f, &mut raw, starts_at)?;
        let starts = raw.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect();
        let cell = fl(20);
        let mut g = Grid {
            cell,
            min: [fl(24), fl(28)],
            n,
            starts,
            index_at: starts_at + cells as u64 * 4,
            coarse: vec![Vec::new(); COARSE_N * COARSE_N],
            coarse_cell: [cell * n[0] as f32 / COARSE_N as f32, cell * n[1] as f32 / COARSE_N as f32],
        };
        let tags = std::fs::read(format!("{path}.tags")).ok().filter(|t| t.len() as u64 == tri_n);
        let over = g.n[0] * g.n[1];
        for t in read_index(&f, &g, over)? {
            let [a, b, c] = read_tri(&f, t)?;
            let lo = [a[0].min(b[0]).min(c[0]), a[1].min(b[1]).min(c[1])];
            let hi = [a[0].max(b[0]).max(c[0]), a[1].max(b[1]).max(c[1])];
            let (x0, y0, x1, y1) = g.coarse_box(lo, hi);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    g.coarse[y * COARSE_N + x].push(t);
                }
            }
        }
        Ok(World { src: Source::File(f, g, tags), ziplines: Vec::new() })
    }

    /// Every triangle that might meet the box `lo`–`hi`; a triangle may come more than once.
    fn candidates(&self, lo: V3, hi: V3, mut each: impl FnMut(&Tri)) {
        match &self.src {
            Source::Mem(tris, grid) => grid
                .near(lo, hi)
                .into_iter()
                .map(|k| &tris[k as usize])
                .filter(|t| (0..3).all(|i| t.iter().any(|v| v[i] >= lo[i]) && t.iter().any(|v| v[i] <= hi[i])))
                .for_each(each),
            Source::File(f, g, tags) => {
                let mut visit = |t: u32| {
                    if tags.as_ref().is_some_and(|tags| tags[t as usize] & TAG_WALK == 0) {
                        return;
                    }
                    if let Ok(tri) = read_tri(f, t) {
                        each(&tri);
                    }
                };
                let col = |v: f32, i: usize| ((v - g.min[i]) / g.cell).floor() as i64;
                let (x0, x1) = (col(lo[0], 0).max(0), col(hi[0], 0).min(g.n[0] as i64 - 1));
                let (y0, y1) = (col(lo[1], 1).max(0), col(hi[1], 1).min(g.n[1] as i64 - 1));
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        for t in read_index(f, g, y as usize * g.n[0] + x as usize).unwrap_or_default() {
                            visit(t);
                        }
                    }
                }
                let (x0, y0, x1, y1) = g.coarse_box([lo[0], lo[1]], [hi[0], hi[1]]);
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        g.coarse[y * COARSE_N + x].iter().for_each(|&t| visit(t));
                    }
                }
            }
        }
    }
}

impl Grid {
    fn coarse_box(&self, lo: [f32; 2], hi: [f32; 2]) -> (usize, usize, usize, usize) {
        let c = |v: f32, i: usize| (((v - self.min[i]) / self.coarse_cell[i]).floor() as i64).clamp(0, COARSE_N as i64 - 1) as usize;
        (c(lo[0], 0), c(lo[1], 1), c(hi[0], 0), c(hi[1], 1))
    }
}

#[cfg(unix)]
fn read_exact_at(f: &File, buf: &mut [u8], at: u64) -> io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(f, buf, at)
}

#[cfg(windows)]
fn read_exact_at(f: &File, mut buf: &mut [u8], mut at: u64) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        match f.seek_read(buf, at)? {
            0 => return Err(io::ErrorKind::UnexpectedEof.into()),
            n => (buf, at) = (&mut buf[n..], at + n as u64),
        }
    }
    Ok(())
}

fn read_index(f: &File, g: &Grid, bucket: usize) -> io::Result<Vec<u32>> {
    let (a, b) = (g.starts[bucket] as u64, g.starts[bucket + 1] as u64);
    let mut raw = vec![0u8; (b.saturating_sub(a) * 4) as usize];
    read_exact_at(f, &mut raw, g.index_at + a * 4)?;
    Ok(raw.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect())
}

fn read_tri(f: &File, t: u32) -> io::Result<Tri> {
    let mut raw = [0u8; 36];
    read_exact_at(f, &mut raw, HEADER + t as u64 * 36)?;
    let v = |k: usize| f32::from_le_bytes(raw[k * 4..k * 4 + 4].try_into().unwrap());
    Ok([[v(0), v(1), v(2)], [v(3), v(4), v(5)], [v(6), v(7), v(8)]])
}

/// The result of one hull sweep against one triangle, as Source's ClipBoxToBrush gives it.
struct Clip {
    fraction: f32,
    normal: V3,
    startsolid: bool,
    allsolid: bool,
}

/// Sweep a box (centre `c`, half extents `e`, moving `d`) against a triangle. The planes of
/// their Minkowski sum lie among the separating axes: the box axes, the face normal, and each
/// triangle edge crossed with each box axis.
fn clip(c: V3, e: V3, d: V3, tri: &Tri) -> Option<Clip> {
    let c = sub(c, tri[0]);
    let v = [[0.0; 3], sub(tri[1], tri[0]), sub(tri[2], tri[0])];
    let face = cross(v[1], v[2]);
    if len(face) < 1e-6 {
        return None;
    }
    let edges = [v[1], sub(v[2], v[1]), v[2]];
    let mut axes = vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], scale(face, 1.0 / len(face))];
    for ed in edges {
        for k in 0..3 {
            let mut u = [0.0; 3];
            u[k] = 1.0;
            let a = cross(ed, u);
            let l = len(a);
            if l > 1e-6 {
                axes.push(scale(a, 1.0 / l));
            }
        }
    }
    let end = [c[0] + d[0], c[1] + d[1], c[2] + d[2]];
    let (mut enter, mut leave) = (-1.0f32, 1.0f32);
    let (mut startout, mut getout) = (false, false);
    let mut normal = [0.0; 3];
    for a in axes {
        let r = a[0].abs() * e[0] + a[1].abs() * e[1] + a[2].abs() * e[2];
        let p = v.map(|w| dot(a, w));
        let (lo, hi) = (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
        for (n, dist) in [(a, hi + r), ([-a[0], -a[1], -a[2]], r - lo)] {
            let d1 = dot(c, n) - dist;
            let d2 = dot(end, n) - dist;
            if d2 > 0.0 {
                getout = true;
            }
            if d1 > 0.0 {
                startout = true;
            }
            if d1 > 0.0 && (d2 >= DIST_EPSILON || d2 >= d1) {
                return None;
            }
            if d1 <= 0.0 && d2 <= 0.0 {
                continue;
            }
            if d1 > d2 {
                let f = (d1 - DIST_EPSILON) / (d1 - d2);
                if f > enter {
                    enter = f;
                    normal = n;
                }
            } else {
                // No skin on the way out: a triangle has no thickness for one to fit in.
                leave = leave.min(d1 / (d1 - d2));
            }
        }
    }
    if !startout {
        return Some(Clip { fraction: if getout { 1.0 } else { 0.0 }, normal, startsolid: true, allsolid: !getout });
    }
    (enter < leave && enter > -1.0).then(|| Clip { fraction: enter.max(0.0), normal, startsolid: false, allsolid: false })
}

impl Tracer for World {
    fn ziplines(&self) -> &[Zipline] {
        &self.ziplines
    }

    fn hull(&mut self, q: &HullQuery) -> Trace {
        let e: V3 = std::array::from_fn(|i| (q.maxs[i] - q.mins[i]) * 0.5);
        let c: V3 = std::array::from_fn(|i| q.start[i] + (q.mins[i] + q.maxs[i]) * 0.5);
        let d = sub(q.end, q.start);
        let lo: V3 = std::array::from_fn(|i| q.start[i].min(q.end[i]) + q.mins[i] - 1.0);
        let hi: V3 = std::array::from_fn(|i| q.start[i].max(q.end[i]) + q.maxs[i] + 1.0);
        let mut t = Trace { start: q.start, fraction: 1.0, ..Default::default() };
        self.candidates(lo, hi, |tri| {
            let Some(k) = clip(c, e, d, tri) else { return };
            t.startsolid |= k.startsolid;
            t.allsolid |= k.allsolid;
            if k.allsolid {
                t.fraction = 0.0;
            } else if !k.startsolid && k.fraction < t.fraction {
                t.fraction = k.fraction;
                t.normal = k.normal;
                t.entity = WORLD_ENTITY;
                t.contents = CONTENTS_SOLID;
            }
        });
        if t.startsolid {
            t.entity = WORLD_ENTITY;
            t.contents = CONTENTS_SOLID;
        }
        t.endpos = mad(q.start, d, t.fraction);
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The grid hands a trace the same triangles, in the same order, as testing every one.
    #[test]
    fn the_grid_keeps_every_triangle_a_trace_would_test() {
        let mut tris = crate::testmap::build().tris;
        let mut s = 0x9e37_79b9u32;
        let mut next = |r: f32| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (s >> 8) as f32 / (1u32 << 24) as f32 * 2.0 * r - r
        };
        for _ in 0..3000 {
            let a = [next(8000.0), next(8000.0), next(500.0)];
            tris.push([a, [a[0] + next(300.0), a[1] + next(300.0), a[2] + next(300.0)], [a[0] + next(300.0), a[1] + next(300.0), a[2]]]);
        }
        let grid = MemGrid::new(&tris);
        let overlaps = |t: &Tri, lo: V3, hi: V3| (0..3).all(|i| t.iter().any(|v| v[i] >= lo[i]) && t.iter().any(|v| v[i] <= hi[i]));
        for _ in 0..5000 {
            let c = [next(9000.0), next(9000.0), next(600.0)];
            let r = [next(400.0).abs() + 1.0, next(400.0).abs() + 1.0, next(100.0).abs() + 1.0];
            let (lo, hi) = (std::array::from_fn(|i| c[i] - r[i]), std::array::from_fn(|i| c[i] + r[i]));
            let all: Vec<usize> = (0..tris.len()).filter(|&k| overlaps(&tris[k], lo, hi)).collect();
            let near: Vec<usize> = grid.near(lo, hi).into_iter().map(|k| k as usize).filter(|&k| overlaps(&tris[k], lo, hi)).collect();
            assert_eq!(near, all, "box {lo:?} {hi:?}");
        }
    }
}
