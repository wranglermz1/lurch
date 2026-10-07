//! Rewrite recordings in the current (version 4) format, in place.
use lurch::ltr::{open, sizes, Writer};

fn main() {
    for path in std::env::args().skip(1) {
        let r = open(&path).expect("open");
        let map = r.map.clone();
        let recs: Vec<_> = r.map(|r| r.expect("record")).collect();
        let Some(first) = recs.first() else { continue };
        let tmp = format!("{path}.tmp");
        let f = std::io::BufWriter::new(std::fs::File::create(&tmp).unwrap());
        let mut w = Writer::new(f, &map, sizes(first)).unwrap();
        let n = recs.len();
        for r in recs {
            w.write(r).unwrap();
        }
        w.finish().unwrap();
        let before = std::fs::metadata(&path).unwrap().len();
        std::fs::rename(&tmp, &path).unwrap();
        let after = std::fs::metadata(&path).unwrap().len();
        println!("{path}: {n} records, {before} -> {after} bytes");
    }
}
