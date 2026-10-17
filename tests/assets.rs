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
    let l = lurch::assets::loadout(std::path::Path::new(&dir), "wraith", None).unwrap();
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

/// Each heirloom's viewmodel loads with its rig, plays what the fists play, and has textures.
/// The draw is `ACT_VM_DRAW`: Octane's knife has no `ACT_VM_DRAWFIRST`, and Deploy falls back.
#[test]
fn an_install_yields_each_heirloom_and_its_animation() {
    let Some(dir) = std::env::var_os("LURCH_R5R") else { return };
    for h in lurch::assets::HEIRLOOMS {
        let l = lurch::assets::loadout(std::path::Path::new(&dir), "wraith", Some(h)).unwrap();
        assert!(!l.weapon.meshes.is_empty(), "{}", h.name);
        assert_eq!(l.weapon.rigs.len(), 1, "{}: its rig once, though the model may list it too", h.name);
        assert_eq!(l.weapon_textures.len(), l.weapon.meshes.len());
        assert!(l.weapon_textures.iter().all(|t| t.first().is_some_and(Option::is_some)), "{} albedo", h.name);
        let anims = ModelAnims::new(&l.weapon);
        for act in [Activity::VM_DRAW, Activity::VM_IDLE, Activity::VM_SPRINT, Activity::VM_RAISE_FROM_SPRINT, Activity::VM_LOWER, Activity::VM_RAISE, Activity::VM_LAND_MEDIUM] {
            assert!(anims.select(act, &[], None).is_some(), "{} has no {act:?}", h.name);
        }
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

#[test]
fn menus_build_their_panels_from_the_install() {
    use lurch::assets::vgui::{load, Panel, Scheme};
    let Some(dir) = std::env::var_os("LURCH_R5R") else { return };
    let platform = std::path::Path::new(&dir).join("platform");
    let scheme = Scheme::load(&platform).unwrap();
    let path = |root: &Panel, names: &[&str]| -> Panel { names.iter().fold(root.clone(), |p, n| p.child(n).unwrap_or_else(|| panic!("{n}")).clone()) };

    let system = load(&platform, &scheme, "scripts/resource/ui/menus/system.menu").unwrap();
    assert_eq!(system.len(), 1);
    assert_eq!((system[0].name.as_str(), system[0].class.as_str()), ("menu", "Frame"));
    let button = path(&system[0], &["SystemPanel", "Button0"]);
    assert_eq!((button.class.as_str(), button.settings.str("wide")), ("RuiButton", Some("376")));
    let footer = path(&system[0], &["FooterButtons"]);
    assert_eq!(footer.settings.str("controlSettingsFile"), Some("scripts/resource/ui/menus/panels/footer_buttons.res"));
    assert_eq!(footer.settings.str("zpos"), Some("501"), "a panel's own key beats its template's");
    assert!(footer.child("LeftRuiFooterButton0").is_some());

    let misc = load(&platform, &scheme, "scripts/resource/ui/menus/misc.menu").unwrap();
    let row = path(&misc[0], &["SettingsPanel", "HudOptionsPanel", "ContentPanel", "SwitchLootPromptStyle"]);
    assert_eq!(row.settings.str("ConVar"), Some("hud_setting_showMedals"));
    assert_eq!(row.settings.str("rui"), Some("ui/settings_base_button.rpak"));
    assert_eq!(row.children.iter().map(|c| (c.name.as_str(), c.group)).collect::<Vec<_>>(), [("FULL", Some(0)), ("RightButton", Some(0)), ("LeftButton", Some(0)), ("ValueButton", Some(0))]);
    assert_eq!(row.children[0].settings.str("rui"), Some("ui/basic_image.rpak"), "a child group's panel takes its template");
    assert!(path(&misc[0], &["SettingsPanel", "SoundPanel", "ContentPanel"]).child("SldMasterVolume").is_some(), "audio.res [$WINDOWS], not the console's");
}

#[test]
fn menus_lay_out_as_vpanel_solves_them() {
    use lurch::assets::vgui::{load, Scheme};
    use lurch::vgui::{layout, Placed, Screen};
    let Some(dir) = std::env::var_os("LURCH_R5R") else { return };
    let platform = std::path::Path::new(&dir).join("platform");
    let scheme = Scheme::load(&platform).unwrap();
    let screen = Screen { wide: 1920, tall: 1080 };
    let path = |root: &Placed, names: &[&str]| -> Placed { names.iter().fold(root.clone(), |p, n| p.child(n).unwrap_or_else(|| panic!("{n}")).clone()) };

    let system = layout(&load(&platform, &scheme, "scripts/resource/ui/menus/system.menu").unwrap(), screen);
    let b0 = path(&system[0], &["SystemPanel", "Button0"]);
    let b1 = path(&system[0], &["SystemPanel", "Button1"]);
    assert_eq!((b0.abs, b0.size), ((772, 526), (376, 60)), "16 below the centre");
    assert_eq!(b1.abs, (772, 594), "8 below the first");

    let misc = layout(&load(&platform, &scheme, "scripts/resource/ui/menus/misc.menu").unwrap(), screen);
    let hud = path(&misc[0], &["SettingsPanel", "HudOptionsPanel"]);
    assert_eq!((hud.abs, hud.size), ((160, 128), (1728, 836)), "ypos -128 from a TOP corner is below it");
    let row = path(&hud, &["ContentPanel", "SwitchLootPromptStyle"]);
    assert_eq!((row.abs, row.size), ((160, 128), (1040, 60)));
    let next = path(&hud, &["ContentPanel", "SwitchShotButtonHints"]);
    assert_eq!(next.abs, (160, 190), "2 below, the SwitchButton template's ypos");
}

#[test]
fn menus_paint_in_zpos_order() {
    use lurch::assets::vgui::{load, Scheme};
    use lurch::vgui::{layout, paint_list, Paint, Screen};
    let Some(dir) = std::env::var_os("LURCH_R5R") else { return };
    let platform = std::path::Path::new(&dir).join("platform");
    let scheme = Scheme::load(&platform).unwrap();
    let panels = load(&platform, &scheme, "scripts/resource/ui/menus/system.menu").unwrap();
    let placed = layout(&panels, Screen { wide: 1920, tall: 1080 });
    let paints = paint_list(&panels, &placed, (0, 0, 1920, 1080));
    assert_eq!(paints[0], Paint::Rui { path: vec![0, 1] }, "ScreenFrame's blur first");
    assert_eq!(paints[1], Paint::Image { texture: "texture/vgui/HUD/white.rpak".into(), rect: (-960, -540, 2880, 1620), color: [0, 0, 0, 200], clip: (0, 0, 1920, 1080) });
    let ruis: Vec<_> = paints.iter().filter_map(|p| if let Paint::Rui { path } = p { Some(path.clone()) } else { None }).collect();
    let system_panel = panels[0].children.iter().position(|p| p.name == "SystemPanel").unwrap();
    let b0 = panels[0].children[system_panel].children.iter().position(|p| p.name == "Button0").unwrap();
    assert!(ruis.contains(&vec![0, system_panel, b0]));
}
