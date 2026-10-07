use lurch::ltr::{f32_at, open, u32_at};

fn main() {
    let path = std::env::args().nth(1).expect("usage: traces FILE.ltr [N]");
    let every: usize = std::env::args().nth(2).map_or(50, |s| s.parse().unwrap());
    for (i, rec) in open(&path).expect("open").enumerate() {
        let rec = rec.expect("record");
        if i % every != 0 {
            continue;
        }
        for t in &rec.traces {
            let p: Vec<String> = (0..0x5c / 4).map(|k| format!("{:.3}", f32_at(&t.ray, k * 4))).collect();
            let r: Vec<String> = (0..0x70 / 4).map(|k| {
                let f = f32_at(&t.result, k * 4);
                if f.is_finite() && f.abs() < 1e7 && (f == 0.0 || f.abs() > 1e-6) { format!("{f:.4}") } else { format!("#{:x}", u32_at(&t.result, k * 4)) }
            }).collect();
            println!("seq {:x} slot {:x} mask {:x} flags {:?}\n  ray {}\n  result {}", rec.seq, t.slot, t.mask, &t.ray[0x5c..0x60], p.join(" "), r.join(" "));
        }
    }
}
