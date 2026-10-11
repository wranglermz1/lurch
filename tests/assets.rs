//! The readers of the game's files: the zstd decoder against the oracle image, and, with an
//! R5Reloaded install named by `LURCH_R5R`, what the game shell loads from one. Without the
//! variable the install test passes without checking anything.

use std::io::Read;

use lurch::anim::bones::{accumulate_pose, BoneList, Pose};
use lurch::anim::model::ModelAnims;
use lurch::anim::{Activity, AnimSet};

#[test]
fn zstd_unpacks_the_oracle_image() {
    let root = env!("CARGO_MANIFEST_DIR");
    let out = lurch::assets::zstd::decompress(&std::fs::read(format!("{root}/dumps/current.bin.zst")).unwrap()).unwrap();
    assert_eq!(&out[..2], b"MZ");
    let Ok(mut f) = std::fs::File::open(format!("{root}/dumps/current.bin")) else { return };
    let mut at = 0;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        assert!(out.get(at..at + n) == Some(&buf[..n]), "differs in the MiB at {at:#x}");
        at += n;
    }
    assert_eq!(at, out.len());
}

#[test]
fn an_install_yields_the_legend_the_fists_and_their_animation() {
    let Some(dir) = std::env::var_os("LURCH_R5R") else { return };
    let l = lurch::assets::loadout(std::path::Path::new(&dir), "wraith").unwrap();
    assert!(l.legend.str("footstep_type").is_some_and(|t| t.eq_ignore_ascii_case("wraith")));
    assert!(l.legend.float("viewmodelfov").is_some());
    assert!(l.weapon_script.contains_key("viewmodel"));
    for m in &l.arms.meshes {
        assert!(m.weights.iter().all(|w| (w.iter().map(|x| x.1).sum::<f32>() - 1.0).abs() < 1e-3), "{}", m.material);
    }
    assert_eq!(l.arms_textures.len(), l.arms.meshes.len());
    for textures in &l.arms_textures {
        assert_eq!(textures.len(), 7, "albedo, normal, gloss, spec, emissive, ao, cavity");
        let albedo = textures[0].as_ref().unwrap();
        assert_eq!(albedo.format, 1, "BC1 sRGB");
        assert_eq!(albedo.mips.len(), albedo.width.max(albedo.height).ilog2() as usize + 1, "down to 1x1");
        for (i, mip) in albedo.mips.iter().enumerate() {
            let blocks = (albedo.width >> i).max(1).div_ceil(4) * (albedo.height >> i).max(1).div_ceil(4);
            assert_eq!(mip.len(), blocks as usize * 8);
        }
    }
    let anims = ModelAnims::new(&l.weapon);
    let bones = l.weapon.studio.bones();
    for act in [Activity::VM_DRAWFIRST, Activity::VM_SPRINT, Activity::VM_RAISE_FROM_SPRINT, Activity::VM_LAND_MEDIUM] {
        let seq = anims.select(act, &[], None).unwrap_or_else(|| panic!("no {act:?}"));
        let mut pose = Pose::new(bones.len());
        for (i, b) in bones.iter().enumerate() {
            (pose.pos[i], pose.q[i]) = (b.pos, b.quat);
        }
        assert!(accumulate_pose(&anims.sk, &mut pose, seq.0 as usize, 0.5, 1.0, &anims.sk.default_poses(), &BoneList::all(bones.len())));
        assert!(pose.q.iter().all(|q| (q.iter().map(|c| c * c).sum::<f32>() - 1.0).abs() < 1e-3), "{act:?}");
    }
}

/// The grapple's hook model and textures, and the rope material's, come from two paks.
#[test]
fn an_install_yields_the_grapple_hook_and_rope() {
    let Some(dir) = std::env::var_os("LURCH_R5R") else { return };
    let g = lurch::assets::grapple(std::path::Path::new(&dir)).unwrap();
    assert!(!g.hook.meshes.is_empty());
    assert!(g.hook_textures.iter().all(|t| t.first().is_some_and(Option::is_some)), "hook albedo");
    assert!(g.rope.first().is_some_and(Option::is_some), "rope albedo");
}
