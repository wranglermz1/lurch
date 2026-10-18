//! Lists what the game's files hold for the viewmodel: `assets INSTALL [MODEL...]`, or
//! `--models PAK [FILTER]` (model names), `--seqs MODEL` (each sequence, an heirloom's with its rig; `LURCH_SEQDESC=1` adds its blend grid and first animation),
//! `--pose MODEL SEQUENCE CYCLE`, `--settings KEYS FILTER` (`*` for every key), `--decode PAK...`,
//! `--names PAK MAGIC NAME_AT FILTER` (assets of a type whose name, at `NAME_AT` in the header,
//! contains `FILTER`), `--material PAK NAME` (a material's textures and whether each loads).

use std::path::Path;

use lurch::anim::bones::{self, BoneList, Pose, Skeleton};
use lurch::assets::model;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let install = Path::new(&args[0]);
    let t = std::time::Instant::now();
    let inst = lurch::assets::Install::open(install).unwrap();
    eprintln!("build {:08x}-{:08x} (oracle: {})", inst.build.0, inst.build.1, inst.is_oracle_build());
    if args.get(1).map(String::as_str) == Some("--decode") {
        for n in &args[2..] {
            let t = std::time::Instant::now();
            match inst.pak(n) {
                Ok(p) => println!("{n}: {} assets, {} bytes, {:?}", p.assets.len(), p.data().len(), t.elapsed()),
                Err(e) => println!("{n}: {e}"),
            }
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--models") {
        let p = inst.pak(&args[2]).unwrap();
        for a in p.assets.iter().filter(|a| &a.magic == b"mdl_") {
            let n = p.name(a, 0x10).unwrap_or("");
            if n.contains(args.get(3).map_or("", String::as_str)) {
                println!("{n}");
            }
        }
        return;
    }
    let common = inst.pak("common.rpak").unwrap();
    let pak = &common;
    eprintln!("common.rpak: {} assets in {:?}", pak.assets.len(), t.elapsed());
    if args.get(1).map(String::as_str) == Some("--settings") {
        let keys: Vec<&str> = args[2].split(',').collect();
        let names = ["startup.rpak", "common_early.rpak", "common.rpak", "common_mp.rpak", "common_sdk.rpak"];
        let paks: Vec<_> = names.iter().map(|n| if *n == "common.rpak" { None } else { Some(inst.pak(n).unwrap()) }).collect();
        let paks: Vec<&lurch::assets::rpak::Pak> = paks.iter().map(|p| p.as_ref().unwrap_or(&common)).collect();
        let layouts: std::collections::HashMap<_, _> = paks.iter().flat_map(|p| lurch::assets::settings::layouts(p)).collect();
        for (p, n) in paks.iter().zip(names) {
            for a in p.assets.iter().filter(|a| &a.magic == b"stlt") {
                if p.name(a, 0).is_some_and(|x| x.contains("player")) {
                    eprintln!("layout {:#x} {:?} in {n}", a.guid, p.name(a, 0));
                }
            }
        }
        for (pak, name) in paks.iter().zip(names) {
            for n in lurch::assets::settings::names(pak).into_iter().filter(|n| n.contains(&args[3])) {
                let Some(s) = lurch::assets::settings::load(pak, &layouts, &n) else { println!("{name} {n}: no layout"); continue };
                let vals: Vec<String> = if keys == ["*"] {
                    let mut v: Vec<String> = s.values.iter().map(|(k, v)| format!("{k}={v:?}")).collect();
                    v.sort();
                    v
                } else {
                    keys.iter().map(|k| format!("{k}={:?}", s.values.get(*k))).collect()
                };
                println!("{name} {n}: {}", vals.join(" "));
            }
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--names") {
        let p = inst.pak(&args[2]).unwrap();
        let at = usize::from_str_radix(args[4].trim_start_matches("0x"), 16).unwrap();
        for a in p.assets.iter().filter(|a| a.magic == args[3].as_bytes()[..4]) {
            if let Some(n) = p.name(a, at).filter(|n| n.to_ascii_lowercase().contains(&args[5].to_ascii_lowercase())) {
                println!("{:#018x} v{} {n}", a.guid, a.version);
            }
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--material") {
        let p = inst.pak(&args[2]).unwrap();
        let m = p.find(b"matl", 0x18, &args[3]).expect("no such material");
        for g in lurch::assets::texture::material(&p, &p, m.guid).unwrap() {
            let t = lurch::assets::texture::load(&p, install, g);
            println!("{g:#018x} {}", t.map_or_else(|e| e.to_string(), |t| format!("{}x{} format {} mips {}", t.width, t.height, t.format, t.mips.len())));
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--seqs") {
        let heirloom = lurch::assets::HEIRLOOMS.iter().find(|h| h.viewmodel.eq_ignore_ascii_case(&args[2]));
        let own = heirloom.map(|h| inst.pak(h.pak).unwrap());
        let pak = own.as_ref().unwrap_or(pak);
        let mut m = model::load(pak, install, &args[2]).unwrap();
        if let Some(h) = heirloom {
            m.rigs.push(model::load_rig(pak, h.rig).unwrap());
        }
        let anims = lurch::anim::model::ModelAnims::new(&m);
        for (i, s) in anims.seqs.iter().enumerate() {
            let ev: Vec<String> = s.named_events.iter().map(|(c, n, o)| format!("{c:.3} {n} {o}")).collect();
            if let Some((_, q)) = anims.sk.seq(i).filter(|_| std::env::var("LURCH_SEQDESC").is_ok()) {
                let (d, a) = (&q.desc, bones::anim_desc(q, 0, 0));
                println!("    grid {}x{} anim fps {} flags {:#x} frames {} +0xc4 {}", d.i32(0x44), d.i32(0x48), d.f32(a + 8), d.i32(a + 0xc), d.i32(a + 0x10), d.i32(0xc4));
            }
            println!(
                "{i:3} {:40} {:28} w {:3} flags {:#06x} fade {}/{} nodes {}/{} {:.3}s mods {:?} {}",
                s.label, s.activity_name, s.weight, s.flags, s.fade_in, s.fade_out, s.entry_node, s.exit_node, s.duration, s.modifiers, ev.join("; ")
            );
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--pose") {
        let m = model::load(pak, install, &args[2]).unwrap();
        let sk = Skeleton::new(&m);
        let bones_ = m.studio.bones();
        let seq = sk.find(&args[3]).expect("no such sequence");
        let cycle: f32 = args[4].parse().unwrap();
        let mut pose = Pose::new(bones_.len());
        for (i, b) in bones_.iter().enumerate() {
            pose.pos[i] = b.pos;
            pose.q[i] = b.quat;
            pose.scale[i] = b.scale;
            pose.rot[i] = b.rot;
        }
        let ok = bones::accumulate_pose(&sk, &mut pose, seq, cycle, 1.0, &sk.default_poses(), &BoneList::all(bones_.len()));
        println!("seq {seq} {} ok {ok}", args[3]);
        for (i, b) in bones_.iter().enumerate() {
            let q = pose.q[i];
            let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
            println!("{i:3} {:28} pos {:?} q {:?} |q| {n:.6} bind pos {:?} q {:?}", b.name, pose.pos[i], q, b.pos, b.quat);
        }
        return;
    }
    for name in &args[1..] {
        let m = model::load(pak, install, name).unwrap();
        let bones = m.studio.bones();
        println!("{}: {} bones, {} meshes, {} rigs, body parts {:?}", m.name, bones.len(), m.meshes.len(), m.rigs.len(), m.studio.bodyparts());
        for (i, mesh) in m.meshes.iter().enumerate() {
            let maxw = mesh.weights.iter().map(Vec::len).max().unwrap_or(0);
            let sumerr = mesh.weights.iter().map(|w| (w.iter().map(|x| x.1).sum::<f32>() - 1.0).abs()).fold(0.0, f32::max);
            let lo = mesh.positions.iter().fold([f32::MAX; 3], |a, p| [a[0].min(p[0]), a[1].min(p[1]), a[2].min(p[2])]);
            let hi = mesh.positions.iter().fold([f32::MIN; 3], |a, p| [a[0].max(p[0]), a[1].max(p[1]), a[2].max(p[2])]);
            println!("  mesh {i}: part {} model {} material {} verts {} tris {} max weights {maxw} weight sum err {sumerr:.4} bounds {lo:?} {hi:?}", mesh.part, mesh.model, mesh.material, mesh.positions.len(), mesh.indices.len() / 3);
        }
        for (t, g) in m.studio.textures() {
            println!("  texture {t} {g:#x}");
        }
        for r in &m.rigs {
            let rb = r.studio.bones();
            println!("  rig {}: {} bones, {} sequences", r.name, rb.len(), r.seqs.len());
            let missing: Vec<_> = rb.iter().filter(|b| !bones.iter().any(|m| m.name.eq_ignore_ascii_case(&b.name))).map(|b| b.name.clone()).collect();
            let same_order = rb.iter().enumerate().all(|(i, b)| bones.get(i).is_some_and(|m| m.name == b.name));
            println!("    rig bones not in model: {missing:?}; same order: {same_order}");
            let extra: Vec<_> = bones.iter().filter(|b| !rb.iter().any(|m| m.name.eq_ignore_ascii_case(&b.name))).map(|b| b.name.clone()).collect();
            println!("    model bones not in rig: {extra:?}");
        }
    }
}
