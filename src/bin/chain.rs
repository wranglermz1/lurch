//! Chained replay: carry the simulation's own state from call to call and report every time
//! it disagrees with the oracle's next input.
use std::collections::BTreeMap;

fn main() {
    for p in std::env::args().skip(1) {
        let c = lurch::replay::chain(&p).unwrap();
        let mut why: BTreeMap<String, (usize, u32, String)> = BTreeMap::new();
        for (seq, d) in &c.resyncs {
            for part in d.split("; ") {
                let k = part.split_whitespace().next().unwrap().to_string();
                let e = why.entry(k).or_insert((0, *seq, part.to_string()));
                e.0 += 1;
            }
        }
        println!("{p}: {} calls, {} resyncs, {} not recorded, {} changed outside movement", c.calls, c.resyncs.len(), c.unrecorded, c.external);
        for (k, (n, seq, ex)) in why {
            println!("    {k:22} {n:5}  first seq {seq:x}: {ex}");
        }
    }
}
