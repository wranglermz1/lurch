//! One call of a recording: the oracle's logged traces, the simulation's own queries, and
//! every field that disagrees.
//!
//!   call FILE.ltr SEQ
use lurch::ltr::{f32_at, open, vec3_at};
use lurch::oracle::LogTracer;
use lurch::replay::simulate;
use lurch::sim::{HullQuery, Trace, Tracer};

struct Recorder<'a>(LogTracer<'a>, Vec<HullQuery>);

impl Tracer for Recorder<'_> {
    fn hull(&mut self, q: &HullQuery) -> Trace {
        self.1.push(*q);
        self.0.hull(q)
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(path), Some(seq)) = (args.next(), args.next()) else {
        eprintln!("usage: call FILE.ltr SEQ");
        std::process::exit(2);
    };
    let seq = u32::from_str_radix(&seq, 16).expect("hex seq");
    for rec in open(&path).expect("open") {
        let rec = rec.expect("record");
        if rec.seq != seq {
            continue;
        }
        for t in &rec.traces {
            let r = &t.ray;
            println!(
                "oracle slot {:x} centre {:?} delta {:?} extents {:?} -> end {:?} normal {:?} fraction {}",
                t.slot,
                vec3_at(r, 0),
                vec3_at(r, 0x10),
                vec3_at(r, 0x30),
                vec3_at(&t.result, 0x10),
                vec3_at(&t.result, 0x20),
                f32_at(&t.result, 0x30)
            );
        }
        let mut tr = Recorder(LogTracer::new(&rec.traces), Vec::new());
        let (s, _, mv) = simulate(&rec, None, &mut tr);
        for q in &tr.1 {
            let mid = |i: usize| q.start[i] + (q.mins[i] + q.maxs[i]) * 0.5;
            let half = |i: usize| (q.maxs[i] - q.mins[i]) * 0.5;
            println!("ours   centre {:?} delta {:?} extents {:?}", [mid(0), mid(1), mid(2)], [q.end[0] - q.start[0], q.end[1] - q.start[1], q.end[2] - q.start[2]], [half(0), half(1), half(2)]);
        }
        for q in &tr.0.missing {
            println!("not in log: {:?} -> {:?}", q.start, q.end);
        }
        for d in &s.diffs {
            println!("diff {d}");
        }
        println!("origin {:?} velocity {:?}", mv.origin, mv.velocity);
    }
}
