use lurch::ltr::{f32_at, open, u32_at, vec3_at};

fn main() {
    let path = std::env::args().nth(1).expect("usage: inspect FILE.ltr");
    let r = open(&path).expect("open");
    println!("map {}", r.map);
    for rec in r {
        let rec = rec.expect("record");
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
