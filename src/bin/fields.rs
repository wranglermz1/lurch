//! How often each modelled field disagrees with the oracle, over the given recordings.
use std::collections::BTreeMap;

fn main() {
    let mut count: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut n = 0;
    for p in std::env::args().skip(1) {
        for s in lurch::replay::file(&p).unwrap() {
            n += 1;
            for d in &s.diffs {
                let key = d.split_whitespace().next().unwrap().to_string();
                let e = count.entry(key).or_insert((0, String::new()));
                if e.0 == 0 {
                    e.1 = format!("{p} seq {:x}: {d}", s.seq);
                }
                e.0 += 1;
            }
        }
    }
    println!("{n} calls");
    for (k, (c, first)) in count {
        println!("{k:24} {c:6}  first: {first}");
    }
}
