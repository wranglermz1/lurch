use lurch::ltr::{f32_at, open, u32_at, vec3_at};

fn main() {
    let path = std::env::args().nth(1).expect("usage: inspect FILE.ltr");
    let fields: Vec<usize> = std::env::var("FIELDS").ok().map(|f| f.split(',').map(|x| usize::from_str_radix(x.trim_start_matches("0x"), 16).unwrap()).collect()).unwrap_or_default();
    let r = open(&path).expect("open");
    println!("map {}", r.map);
    for rec in r {
        let rec = rec.expect("record");
        if std::env::var("FINDANG").is_ok() {
            let a = vec3_at(&rec.mv_in, 0xc);
            let hits: Vec<String> = (0..rec.player_in.len() - 8).step_by(4).filter(|&o| f32_at(&rec.player_in, o) == a[0] && f32_at(&rec.player_in, o + 4) == a[1]).map(|o| format!("{o:x}")).collect();
            println!("seq {:x} angles {:?} at {}", rec.seq, a, hits.join(" "));
            continue;
        }
        if !fields.is_empty() {
            let mvf = std::env::var("MV").is_ok();
            let (a, b) = if mvf { (&rec.mv_in, &rec.mv_out) } else { (&rec.player_in, &rec.player_out) };
            let v: Vec<String> = fields.iter().map(|&o| format!("{o:x}={}/{} ({:x}/{:x})", f32_at(a, o), f32_at(b, o), u32_at(a, o), u32_at(b, o))).collect();
            println!("seq {:x} {}", rec.seq, v.join(" "));
            continue;
        }
        println!(
            "seq {:x} btn {:x} fwd {} dt {:.5} traces {} origin {:?} -> {:?} vel {:?} -> {:?} ground {:x} -> {:x}",
            rec.seq,
            u32_at(&rec.mv_in, 0x24),
            f32_at(&rec.mv_in, 0x30),
            f32_at(&rec.globals, 0x30),
            rec.traces.len(),
            vec3_at(&rec.mv_in, 0x118),
            vec3_at(&rec.mv_out, 0x118),
            vec3_at(&rec.mv_in, 0x124),
            vec3_at(&rec.mv_out, 0x124),
            u32_at(&rec.player_in, 0x3dc),
            u32_at(&rec.player_out, 0x3dc),
        );
    }
}
