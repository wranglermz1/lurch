//! The RUI runtime against an R5Reloaded install named by `LURCH_R5R`: the install's
//! elements run from its own `ui.dll` through the ported callbacks. Without the variable these
//! tests pass without checking anything.

use std::path::Path;

use lurch::rui::{Frame, Globals, Topology, Ui, RUI_DRAW_HUD};

fn ui() -> Option<Ui> {
    let dir = std::env::var_os("LURCH_R5R")?;
    Some(Ui::open(Path::new(&dir)).unwrap())
}

#[test]
fn rpak_names_hash_to_their_asset_guids() {
    assert_eq!(lurch::assets::rpak::guid("ui/crosshair_grapple.rpak"), 0xc079_bfb4_a974_3945);
    assert_eq!(lurch::assets::rpak::guid("datatable/colorpalette_table.rpak"), 0xc803_bd35_abd0_972b);
    assert_eq!(lurch::assets::rpak::guid("UI\\Crosshair_Grapple.RPAK"), lurch::assets::rpak::guid("ui/crosshair_grapple.rpak"));
}

#[test]
fn crosshair_grapple_runs_from_the_install() {
    let Some(ui) = ui() else { return };
    let e = &ui.lib.elements["crosshair_grapple"];
    for name in ["ammoFrac", "crosshairMovementX", "crosshairMovementY"] {
        assert!(e.find_arg(0, name, 0, 5).is_some(), "{name}");
    }
    for name in ["isGrappleInRange", "weaponIsDisabled"] {
        assert!(e.find_arg(0, name, 0, 3).is_some(), "{name}");
    }
    for name in ["grappleInRangeTime", "grappleOutOfRangeTime"] {
        assert!(e.find_arg(0, name, 0, 9).is_some(), "{name}");
    }
    assert!(e.find_arg(0, "isGrappleInRange", 0, 5).is_none(), "the type is checked");

    let mut rui = ui.create("crosshair_grapple", RUI_DRAW_HUD, 0).unwrap();
    let globals = Globals::new(100.0, 1920.0, 1080.0);
    let topology = Topology::hud(1920.0, 1080.0);
    let mut frame = Frame::default();
    assert!(rui.set_float(&ui, "ammoFrac", 1.0));
    assert!(ui.update(&mut rui, &globals, &topology, &mut frame).unwrap(), "out of range: drawn");
    let image = rui.data.i32(0x54);
    assert_eq!(image, ui.image("rui/hud/crosshairs/grapple_indicator"));
    assert!(image >= 0);
    let yellow = [rui.data.f32(0x40), rui.data.f32(0x44), rui.data.f32(0x48)];
    let expect = lurch::rui::api::srgb_to_linear([1.0, 1.0, 0.0, 0.0]);
    assert_eq!(yellow, [expect[0], expect[1], expect[2]], "UNAVAILABLE_COLOR (255, 255, 0)");
    assert!((yellow[0] - 1.0).abs() < 1e-6);
    assert_eq!(rui.data.f32(0x3c), 0.15, "out of range alpha");
    let t = frame.transforms[3];
    assert_eq!(frame.count, 4);
    assert_eq!(t.basis, [24.0 / 1920.0, 0.0, 0.0, 24.0 / 1080.0]);
    assert_eq!(t.origin[0], 0.5 - 12.0 / 1920.0);

    assert!(rui.set_bool(&ui, "isGrappleInRange", true));
    assert!(rui.set_game_time(&ui, "grappleInRangeTime", 99.9));
    assert!(ui.update(&mut rui, &globals, &topology, &mut frame).unwrap(), "in range: drawn");
    let blue = [rui.data.f32(0x40), rui.data.f32(0x44), rui.data.f32(0x48)];
    assert!(blue[2] > blue[0] && blue[2] > blue[1], "AVAILABLE_COLOR is blue: {blue:?}");

    assert!(rui.set_bool(&ui, "weaponIsDisabled", true));
    assert!(!ui.update(&mut rui, &globals, &topology, &mut frame).unwrap(), "a disabled weapon hides it");
}

#[test]
fn crosshair_grapple_draws_one_quad_at_the_centre() {
    let Some(ui) = ui() else { return };
    let mut rui = ui.create("crosshair_grapple", RUI_DRAW_HUD, 0).unwrap();
    let (globals, topology, mut frame) = (Globals::new(100.0, 1920.0, 1080.0), Topology::hud(1920.0, 1080.0), Frame::default());
    rui.set_float(&ui, "ammoFrac", 1.0);
    assert!(ui.update(&mut rui, &globals, &topology, &mut frame).unwrap());
    let mut draws = lurch::rui::draw::Draws::default();
    ui.draws(&rui, &frame, &mut draws);
    assert!(draws.unported.is_empty(), "{:?}", draws.unported);
    assert_eq!(draws.quads.len(), 1);
    let q = &draws.quads[0];
    let px = q.corners.map(|c| topology.place(c[0], c[1]));
    let near = |a: [f32; 3], b: [f32; 2]| (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3;
    assert!(near(px[0], [948.0, 528.0]) && near(px[2], [972.0, 552.0]), "24 px square at the centre: {px:?}");
    let atlas = &ui.lib.atlases[q.atlas as usize];
    let rect = atlas.uv[q.image as usize];
    let texel = |uv: [f32; 4]| [(rect[0] + uv[0] * rect[2]) * atlas.width as f32, (rect[1] + uv[1] * rect[3]) * atlas.height as f32];
    let (a, b) = (texel(q.uv[0]), texel(q.uv[2]));
    assert!((b[0] - a[0] - 64.0).abs() < 1e-2 && (b[1] - a[1] - 64.0).abs() < 1e-2, "the whole 64-texel image: {a:?} {b:?}");
    let crop = [q.uv[0][2], q.uv[0][3], q.uv[2][2], q.uv[2][3]];
    assert!(crop.iter().zip([0.0, 0.0, 1.0, 1.0]).all(|(a, b)| (a - b).abs() < 1e-5), "crop coordinates span the quad: {crop:?}");
    assert_eq!(draws.styles[q.style as usize][3], 0.15, "colour 0's alpha");
    assert_eq!(q.flags >> 12, 1, "the shader's image mode");
}

#[test]
fn fonts_fill_the_table_as_the_image_holds_it() {
    let Some(ui) = ui() else { return };
    let names = ["DefaultRegularFont", "DefaultBoldFont", "TitleRegularFont", "TitleBoldFont"];
    let slots: Vec<u16> = names.iter().map(|n| lurch::assets::rui::font_slot(n)).collect();
    let mut atlas_of = [5u8; 32];
    for (a, f) in ui.lib.fonts.iter().enumerate() {
        for face in &f.faces {
            atlas_of[face.slot as usize] = a as u8;
        }
    }
    assert!(slots.iter().all(|&s| atlas_of[s as usize] != 5), "the names ui.dll passes find faces: {slots:?}");
    let face = ui.lib.fonts.iter().flat_map(|f| &f.faces).find(|f| f.slot == slots[0]).unwrap();
    let a = face.glyph('A' as u32).unwrap() as usize;
    assert!(face.glyphs[a].advance > 0.0 && face.glyph(0x10ffff) == face.glyph(0x25a1));
    assert!(ui.lib.fonts.iter().flat_map(|f| &f.faces).all(|f| f.glyphs.last().unwrap().kerning as usize == f.kerning.len()), "one glyph past the count ends the last one's kerning");

    // The table at 0xc05f2d0 (a face pointer per slot) and 0xc05f3d0 (its atlas, 5 for none) in
    // the running game the image was taken from.
    let Ok(image) = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/dumps/current.bin")) else { return };
    for slot in 0..32 {
        let face = u64::from_le_bytes(image[0xc05f2d0 + 8 * slot..][..8].try_into().unwrap());
        assert_eq!(image[0xc05f3d0 + slot], atlas_of[slot], "slot {slot}");
        assert_eq!(face != 0, atlas_of[slot] != 5, "slot {slot}");
    }
}

#[test]
fn menu_panels_make_their_elements_with_their_arguments() {
    use lurch::assets::vgui::{load, Scheme};
    use lurch::vgui::{create_rui, layout, paint_rui, Screen};
    let Some(ui) = ui() else { return };
    let platform = Path::new(&std::env::var_os("LURCH_R5R").unwrap()).join("platform");
    let scheme = Scheme::load(&platform).unwrap();
    let screen = Screen { wide: 1920, tall: 1080 };
    let panels = load(&platform, &scheme, "scripts/resource/ui/menus/misc.menu").unwrap();
    let placed = layout(&panels, screen);
    let row = ["SettingsPanel", "HudOptionsPanel", "ContentPanel", "SwitchLootPromptStyle"];
    let panel = row.iter().fold(panels[0].clone(), |p, n| p.child(n).unwrap().clone());
    let place = row.iter().fold(placed[0].clone(), |p, n| p.child(n).unwrap().clone());
    let mut rui = create_rui(&ui, &panel.settings).expect("settings_base_button");
    assert_eq!(rui.name, "settings_base_button");
    paint_rui(&ui, &mut rui, &place, &panel.settings, screen, (1920, 1080));
    assert_eq!(rui.size, [1040.0, 60.0]);
    let e = &ui.lib.elements[&rui.name];
    let clip = e.find_arg(0, "clipRect", 0, 8).expect("clipRect");
    assert_eq!(rui.data.f32(clip.offset as usize), 0.0);

    let right = panel.child("RightButton").unwrap();
    let rui = create_rui(&ui, &right.settings).expect("settings_choice_button");
    let e = &ui.lib.elements[&rui.name];
    let a = [4, 3].into_iter().find_map(|k| e.find_arg(0, "isRightOption", 0, k)).expect("isRightOption");
    assert_eq!(rui.data.i32(a.offset as usize), 1, "an int value sets an int, else a bool");
    assert!(create_rui(&ui, &lurch::assets::keyvalues::parse(b"x { rui \"ui/generic_button\" }")[0]).is_none(), "no .rpak, no element");
}

#[test]
fn buttons_take_their_label_and_states() {
    use lurch::assets::keyvalues::parse;
    use lurch::vgui::{create_rui, label_text, paint_button_text, set_state};
    let Some(ui) = ui() else { return };
    assert_eq!(label_text(&ui, "#SETTINGS"), String::from_utf16_lossy(ui.localized(b"#SETTINGS").unwrap()));
    assert_eq!(label_text(&ui, "#NO_SUCH_TOKEN_HERE"), "#NO_SUCH_TOKEN_HERE");
    assert_eq!(label_text(&ui, "DEFAULT"), "DEFAULT");
    let settings = &parse(b"b { rui \"ui/footer_button.rpak\" behave_as_label 1 }")[0];
    let mut rui = create_rui(&ui, settings).unwrap();
    let e = &ui.lib.elements[&rui.name];
    let text = e.find_arg(0, "buttonText", 0, 1).unwrap().offset as usize;
    let before = rui.data.u64(text);
    paint_button_text(&ui, &mut rui, settings, "Back");
    assert_ne!(rui.data.u64(text), before, "behave_as_label sets buttonText");
    let settings = &parse(b"b { rui \"ui/generic_button.rpak\" }")[0];
    let mut rui = create_rui(&ui, settings).unwrap();
    let e = &ui.lib.elements[&rui.name];
    let focused = e.find_arg(0, "isFocused", 0, 3).expect("isFocused").offset as usize;
    set_state(&ui, &mut rui, 3, true);
    assert_eq!(rui.data.i32(focused), 1);
}

#[test]
fn the_system_menu_shows_two_buttons_and_focuses_under_the_cursor() {
    use lurch::assets::vgui::{load, Scheme};
    use lurch::rui::draw::Draws;
    use lurch::vgui::{system_menu, Paint, Screen};
    let Some(ui) = ui() else { return };
    let platform = Path::new(&std::env::var_os("LURCH_R5R").unwrap()).join("platform");
    let scheme = Scheme::load(&platform).unwrap();
    let mut menu = system_menu(&ui, load(&platform, &scheme, "scripts/resource/ui/menus/system.menu").unwrap());
    let screen = Screen { wide: 1920, tall: 1080 };
    menu.layout(&ui, screen);
    let b0 = menu.find(&["SystemPanel", "Button0"]).unwrap();
    let b2 = menu.find(&["SystemPanel", "Button2"]).unwrap();
    assert_eq!(menu.panel(&b2).unwrap().settings.str("visible"), Some("0"), "an empty label hides a button");
    let footer = |name: &str| menu.placed_at(&menu.find(&["FooterButtons", name]).unwrap()).unwrap().clone();
    let (back, dev) = (footer("LeftRuiFooterButton0"), footer("LeftRuiFooterButton1"));
    assert!(back.visible && dev.visible && !footer("LeftRuiFooterButton2").visible && !footer("RightRuiFooterButton0").visible);
    assert!(30 < back.size.0 && back.size.0 < dev.size.0, "sized to their text: {} {}", back.size.0, dev.size.0);
    assert_eq!(dev.abs.0, back.abs.0 + back.size.0 + 22, "pinned 22 after the one before");
    let hud = Topology::hud(1920.0, 1080.0);
    let mut frame = Frame::default();
    frame.screen_blur = Some([1920, 1080]);
    let mut draws = Draws::default();
    let (images, stops) = menu.draw(&ui, &Globals::new(0.0, 1920.0, 1080.0), screen, (1920, 1080), &hud, &mut frame, &mut draws);
    assert!(stops.is_empty(), "{stops:?}");
    assert!(matches!(&images[0].1, Paint::Image { texture, .. } if texture == "texture/vgui/HUD/white.rpak"));
    assert!(0 < images[0].0, "the blur draws before the cover");
    assert!(images[0].0 < draws.quads.len(), "the buttons draw after it");
    let mut quads = |menu: &mut lurch::vgui::Menu, secs: u64| {
        let mut globals = Globals::new(0.0, 1920.0, 1080.0);
        globals.set_clock(secs * 1_000_000);
        let mut draws = Draws::default();
        menu.draw(&ui, &globals, screen, (1920, 1080), &hud, &mut frame, &mut draws);
        (draws.quads, draws.styles.iter().map(|s| s.map(f32::to_bits)).collect::<Vec<_>>())
    };
    quads(&mut menu, 10);
    let unfocused = quads(&mut menu, 11);
    menu.hover(&ui, 960, 556);
    assert_eq!(menu.hovered.as_deref(), Some(&b0[..]));
    assert_eq!(menu.sounds, ["UI_Menu_Focus"], "a button the cursor enters plays its focus sound");
    let fading = quads(&mut menu, 11);
    let focused = quads(&mut menu, 12);
    assert_ne!(focused, fading, "the focus fades in by the globals' clock");
    assert_ne!(focused, unfocused);
    menu.hover(&ui, 960, 300);
    assert_eq!(menu.hovered, None);
    quads(&mut menu, 12);
    assert_eq!(quads(&mut menu, 13), unfocused, "and out once the cursor leaves");
    menu.sounds.clear();
    menu.hover(&ui, 960, 556);
    assert_eq!(menu.click(), None);
    assert_eq!(menu.sounds, ["UI_Menu_Focus", "UI_Menu_Accept"], "a press its accept sound");
}

#[test]
fn the_settings_menu_opens_on_its_tabs_and_a_click_switches_them() {
    use lurch::assets::vgui::Scheme;
    use lurch::rui::draw::Draws;
    use lurch::vgui::settings::SettingsMenu;
    use lurch::vgui::{paint_list, Paint, Screen};
    let Some(ui) = ui() else { return };
    let platform = Path::new(&std::env::var_os("LURCH_R5R").unwrap()).join("platform");
    let scheme = Scheme::load(&platform).unwrap();
    let install = Path::new(&std::env::var_os("LURCH_R5R").unwrap()).to_path_buf();
    let rows: Vec<_> = lurch::vgui::settings::SCRIPTS.iter().map(|f| lurch::vgui::settings::rows(&install, f)).collect();
    assert_eq!(rows[0].len(), 26, "the Gameplay tab's rows");
    assert_eq!((rows[0][0].name.as_str(), rows[0][0].title.as_str()), ("SwitchLootPromptStyle", "#HUD_SETTING_LOOTPROMPTYSTYLE"));
    assert!(rows[0].iter().any(|r| r.name == "SwchColorBlindMode" && r.additional));
    assert!(rows[4].iter().any(|r| r.name == "SldMasterVolume" && r.slider), "the Audio tab's sliders");
    let mut s = SettingsMenu::load(&ui, &install, &scheme).unwrap();
    s.open(&ui);
    let screen = Screen { wide: 1920, tall: 1080 };
    s.menu.layout(&ui, screen);
    let painted = |s: &lurch::vgui::settings::SettingsMenu| -> Vec<String> {
        paint_list(&s.menu.panels, &s.menu.placed, (0, 0, 1920, 1080))
            .iter()
            .filter_map(|p| if let Paint::Rui { path } = p { s.menu.panel(path).map(|p| p.name.clone()) } else { None })
            .collect()
    };
    let ruis = painted(&s);
    for name in ["ScreenFrame", "DetailsPanel", "Background", "Tab0", "Tab4", "Logo"] {
        assert!(ruis.iter().any(|r| r == name), "{name} in {ruis:?}");
    }
    assert!(!ruis.iter().any(|r| r == "Tab5" || r == "LeftNavButton"), "unused tabs and the shoulder hints hide: {ruis:?}");
    let selected = |s: &mut SettingsMenu, i: usize| {
        let tab = s.menu.find(&["SettingsPanel", "TabsCommon", &format!("Tab{i}")]).unwrap();
        let rui = s.menu.rui(&ui, &tab).unwrap();
        let at = ui.lib.elements[&rui.name].find_arg(0, "isSelected", 0, 3).unwrap().offset as usize;
        rui.data.i32(at) != 0
    };
    assert!(selected(&mut s, 0) && !selected(&mut s, 1));
    let text = |s: &mut SettingsMenu, path: &[usize], arg: &str| {
        let rui = s.menu.rui(&ui, path).unwrap();
        let at = ui.lib.elements[&rui.name].find_arg(0, arg, 0, 1).unwrap().offset as usize;
        // SAFETY: a string argument points at a NUL-terminated copy the instance keeps.
        unsafe { std::ffi::CStr::from_ptr(rui.data.u64(at) as *const std::ffi::c_char) }.to_string_lossy().into_owned()
    };
    let row = s.menu.descendant(&s.menu.find(&["SettingsPanel", "HudOptionsPanel"]).unwrap(), "SwitchShowFPS").unwrap();
    assert_eq!(text(&mut s, &row, "buttonText"), "#HUD_SHOW_FPS", "a row takes its script's title");
    let details = s.menu.find(&["SettingsPanel", "DetailsPanel"]).unwrap();
    let first = s.menu.descendant(&s.menu.find(&["SettingsPanel", "HudOptionsPanel"]).unwrap(), "SwitchLootPromptStyle").unwrap();
    let r = s.menu.placed_at(&first).unwrap().clone();
    s.hover(&ui, r.abs.0 + 10, r.abs.1 + r.size.1 / 2);
    assert_eq!(s.menu.hovered.as_deref(), Some(&first[..]));
    assert_eq!(text(&mut s, &details, "descText"), "#HUD_SETTING_LOOTPROMPTYSTYLE_DESC", "the details panel describes the row in focus");
    let right = s.menu.child(&first, "RightButton").unwrap();
    let rp = s.menu.placed_at(&right).unwrap().clone();
    s.menu.sounds.clear();
    s.hover(&ui, rp.abs.0 + rp.size.0 / 2, rp.abs.1 + rp.size.1 / 2);
    assert_eq!(s.menu.hovered.as_deref(), Some(&right[..]));
    assert!(s.menu.click().is_some(), "a choice picked sets its ConVar");
    assert_eq!(s.menu.sounds, ["UI_Menu_Focus", "UI_Menu_Accept"], "the choice's own accept is silent, its list's plays");
    s.hover(&ui, 5, 1075);
    assert_eq!(text(&mut s, &details, "descText"), "");
    let body = |s: &SettingsMenu, name: &str| s.menu.panel(&s.menu.find(&["SettingsPanel", name]).unwrap()).unwrap().settings.str("visible") == Some("1");
    assert!(body(&s, "HudOptionsPanel") && !body(&s, "SoundPanel"));
    let hud = s.menu.find(&["SettingsPanel", "HudOptionsPanel"]).unwrap();
    let (content, bar) = (s.menu.child(&hud, "ContentPanel").unwrap(), s.menu.child(&hud, "ScrollBar").unwrap());
    let content_y = |s: &mut SettingsMenu| {
        s.menu.layout(&ui, screen);
        s.menu.placed_at(&content).unwrap().abs.1 - s.menu.placed_at(&hud).unwrap().abs.1
    };
    s.wheel(&ui, screen, false);
    assert_eq!(content_y(&mut s), -60, "a wheel notch scrolls 60");
    for _ in 0..100 {
        s.wheel(&ui, screen, true);
    }
    assert_eq!(content_y(&mut s), 0, "not above the top");
    let b = s.menu.placed_at(&bar).unwrap().clone();
    let (x, y) = (b.abs.0 + b.size.0 / 2, b.abs.1 + 5);
    s.drag(&ui, screen, (x, y), true, true);
    s.drag(&ui, screen, (x, y + 100), false, true);
    let travel = -content_y(&mut s);
    s.drag(&ui, screen, (x, y + 100), false, false);
    s.drag(&ui, screen, (x, y + 300), false, true);
    assert_eq!(-content_y(&mut s), travel, "a released handle stays");
    assert!(travel > 100, "the content moves by its height over the bar's: {travel}");
    s.wheel(&ui, screen, true);
    s.wheel(&ui, screen, true);
    let tab1 = s.menu.find(&["SettingsPanel", "TabsCommon", "Tab1"]).unwrap();
    let p = s.menu.placed_at(&tab1).unwrap().clone();
    let tab0 = s.menu.find(&["SettingsPanel", "TabsCommon", "Tab0"]).unwrap();
    s.hover(&ui, p.abs.0 + 2, p.abs.1 + p.size.1 - 2);
    assert_eq!(s.menu.hovered.as_deref(), Some(&tab0[..]), "a tab's slanted left edge leaves its lower corner to the tab before it");
    s.menu.sounds.clear();
    s.hover(&ui, p.abs.0 + p.size.0 / 2, p.abs.1 + p.size.1 / 2);
    assert_eq!(s.menu.hovered.as_deref(), Some(&tab1[..]));
    assert_eq!(s.menu.click(), None);
    assert_eq!(s.menu.sounds, ["UI_Menu_Focus_Large", "UI_Menu_SettingsTab_Select"], "a tab's sounds are its template's");
    s.click(&ui);
    assert!(selected(&mut s, 1) && !selected(&mut s, 0));
    assert!(body(&s, "ControlsPCPanelContainer") && !body(&s, "HudOptionsPanel"));
    s.menu.layout(&ui, screen);
    let ruis = painted(&s);
    assert!(ruis.iter().any(|r| r == "BtnLookSensitivityMenu") && ruis.iter().any(|r| r == "PrgValue"), "the engine-made body's rows: {ruis:?}");
    let mut frame = Frame::default();
    frame.screen_blur = Some([1920, 1080]);
    let mut draws = Draws::default();
    let (images, stops) = s.menu.draw(&ui, &Globals::new(0.0, 1920.0, 1080.0), screen, (1920, 1080), &Topology::hud(1920.0, 1080.0), &mut frame, &mut draws);
    assert!(stops.is_empty(), "{stops:?}");
    assert!(images.iter().any(|(_, p)| matches!(p, Paint::Image { texture, .. } if texture == "texture/rui/menu/common/menu_vignette.rpak")));
    s.close();
    s.open(&ui);
    assert!(selected(&mut s, 1), "the tab last shown opens again");
}

#[test]
fn sliders_follow_their_convar_and_the_cursor() {
    use lurch::assets::vgui::Scheme;
    use lurch::vgui::settings::SettingsMenu;
    use lurch::vgui::slider::ConVarValue;
    use lurch::vgui::Screen;
    let Some(ui) = ui() else { return };
    let install = Path::new(&std::env::var_os("LURCH_R5R").unwrap()).to_path_buf();
    let platform = install.join("platform");
    let scheme = Scheme::load(&platform).unwrap();
    let mut s = SettingsMenu::load(&ui, &install, &scheme).unwrap();
    s.open(&ui);
    s.activate(&ui, 4);
    let screen = Screen { wide: 1920, tall: 1080 };
    let volume = std::cell::Cell::new(0.4f32);
    let convar = |name: &str| (name == "sound_volume").then(|| ConVarValue { float: volume.get(), int: 0, default: 1.0 });
    assert!(s.menu.sync_sliders(&ui, &convar).is_empty());
    s.menu.layout(&ui, screen);
    let sound = s.menu.find(&["SettingsPanel", "SoundPanel"]).unwrap();
    let slider = s.menu.descendant(&sound, "SldMasterVolume").unwrap();
    let part = |s: &SettingsMenu, name: &str| s.menu.placed_at(&s.menu.child(&slider, name).unwrap()).unwrap().clone();
    let whole = s.menu.placed_at(&slider).unwrap().clone();
    let (drop, bar, mark) = (part(&s, "BtnDropButton"), part(&s, "PrgValue"), part(&s, "PnlDefaultMark"));
    assert_eq!((drop.abs, drop.size), (whole.abs, whole.size), "the drop button spans the slider");
    assert_eq!(bar.abs.0 + bar.size.0, whole.abs.0 + whole.size.0 - 68, "the bar's right edge 68 in, the scheme's inset");
    assert_eq!(bar.abs.1, whole.abs.1 + whole.size.1 / 2 - bar.size.1 / 2);
    assert_eq!(mark.abs.0, bar.abs.0 + bar.size.0 - mark.size.0 / 2, "the mark at the default, the bar's end");
    let arg = |s: &mut SettingsMenu, name: &str, arg: &str, kind: u8| {
        let rui = s.menu.rui(&ui, &s.menu.child(&slider, name).unwrap()).unwrap();
        let at = ui.lib.elements[&rui.name].find_arg(0, arg, 0, kind).unwrap().offset as usize;
        (rui.data.f32(at), rui.data.u64(at))
    };
    assert_eq!(arg(&mut s, "PrgValue", "progress", 5).0, 0.4);
    let label = arg(&mut s, "LblSliderText", "buttonText", 1).1;
    // SAFETY: a string argument points at a NUL-terminated copy the instance keeps.
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(label as *const std::ffi::c_char) }.to_str().unwrap(), "40%");
    let y = bar.abs.1 + bar.size.1 / 2;
    s.menu.hover(&ui, bar.abs.0 + bar.size.0 / 4, y);
    assert_eq!(s.menu.sounds, ["UI_Menu_Focus"], "a slider the cursor enters plays its focus sound");
    assert_eq!(s.menu.press(&ui, bar.abs.0 + bar.size.0 / 4, y), Some(("sound_volume".into(), 0.25)), "a press on the bar sets the value under it");
    assert_eq!(s.menu.drag(&ui, bar.abs.0 + bar.size.0 + 300, y + 200), Some(("sound_volume".into(), 1.0)), "a drag follows the cursor off the bar");
    assert_eq!(s.menu.sounds, ["UI_Menu_Focus", "UI_Menu_Accept"], "the press's change plays the accept, the drag's does not");
    s.menu.release(&ui, 0, 0);
    assert_eq!(s.menu.drag(&ui, bar.abs.0, y), None, "released, the cursor no longer sets it");
    assert_eq!(s.menu.press(&ui, whole.abs.0 + 10, y), None, "a press beside the bar sets nothing");
    s.menu.release(&ui, 0, 0);
    volume.set(2.0);
    assert_eq!(s.menu.sync_sliders(&ui, &convar), [("sound_volume".to_string(), 1.0)], "a ConVar out of range is put back in it");
}

#[test]
fn labels_draw_their_text_through_rui() {
    use lurch::assets::vgui::Scheme;
    use lurch::rui::draw::Draws;
    use lurch::vgui::settings::SettingsMenu;
    use lurch::vgui::Screen;
    let Some(ui) = ui() else { return };
    let install = Path::new(&std::env::var_os("LURCH_R5R").unwrap()).to_path_buf();
    let scheme = Scheme::load(&install.join("platform")).unwrap();
    let mut s = SettingsMenu::load(&ui, &install, &scheme).unwrap();
    s.open(&ui);
    s.activate(&ui, 4);
    let screen = Screen { wide: 1920, tall: 1080 };
    s.menu.layout(&ui, screen);
    let sound = s.menu.find(&["SettingsPanel", "SoundPanel"]).unwrap();
    let header = s.menu.descendant(&sound, "VoiceChatHeaderText").unwrap();
    let label = s.menu.labels[&header].clone();
    assert!(!label.text.is_empty() && !label.text.starts_with('#'), "localized: {:?}", label.text);
    assert!(label.caps && label.height == 30 && label.color == [245, 245, 245, 255]);
    let content = label.content(&ui, screen);
    assert!(content.0 > 50 && content.1 == 30, "{content:?}");
    assert_eq!(s.menu.placed_at(&header).unwrap().size.0, content.0, "auto_wide_tocontents");
    let quads = |s: &mut SettingsMenu| {
        let mut frame = Frame::default();
        let mut draws = Draws::default();
        let (_, stops) = s.menu.draw(&ui, &Globals::new(0.0, 1920.0, 1080.0), screen, (1920, 1080), &Topology::hud(1920.0, 1080.0), &mut frame, &mut draws);
        assert!(stops.is_empty(), "{stops:?}");
        draws.quads.len()
    };
    let with = quads(&mut s);
    s.menu.labels.retain(|p, _| *p != header);
    let without = quads(&mut s);
    let glyphs = label.text.chars().filter(|c| !c.is_whitespace()).count();
    assert!(with - without >= glyphs / 2, "the header's text draws: {} quads for {glyphs} glyphs", with - without);
}

#[test]
fn the_key_bind_list_shows_the_binds_and_captures_a_new_key() {
    use lurch::assets::vgui::Scheme;
    use lurch::rui::draw::Draws;
    use lurch::vgui::keybinds::{key_code, KeyBindings, Row, ESCAPE, MOUSE_LEFT};
    use lurch::vgui::settings::SettingsMenu;
    use lurch::vgui::{Paint, Screen};
    let Some(ui) = ui() else { return };
    let install = Path::new(&std::env::var_os("LURCH_R5R").unwrap()).to_path_buf();
    if !install.join("platform/scripts/kb_act.lst").exists() {
        return;
    }
    let scheme = Scheme::load(&install.join("platform")).unwrap();
    let mut s = SettingsMenu::load(&ui, &install, &scheme).unwrap();
    s.open(&ui);
    s.activate(&ui, 1);
    assert!(std::mem::take(&mut s.fill_binds), "showing the tab fills the list in");
    let screen = Screen { wide: 1920, tall: 1080 };
    s.menu.layout(&ui, screen);
    let mut keys = KeyBindings::from_lines(&["bind_US_standard \"w\" \"+forward\" 0", "bind_US_standard \"SPACE\" \"+jump\" 0"]);
    s.menu.bind_list.as_mut().unwrap().fill_in_current(&mut keys);
    let list = s.menu.bind_list.as_ref().unwrap();
    assert_eq!((list.sections.len(), list.items.len()), (4, 44), "kb_act.lst's sections and items");
    let forward = list.items.iter().position(|i| i.binding == "+forward").unwrap();
    assert_eq!(list.items[forward].keys[0], "w");
    let placed = s.menu.placed_at(&list.path).unwrap().clone();
    let (rows, _) = list.rows(screen);
    let Some(&Row::Item { y, h, .. }) = rows.iter().find(|r| matches!(r, Row::Item { item, .. } if *item == forward)) else { panic!() };
    let paints = list.paints(&ui, &placed, screen);
    assert!(paints.iter().any(|p| matches!(p, Paint::Label { label, .. } if label.text == "%w%")), "a key shows as its token");
    let key2 = (placed.abs.0 + 5 + screen.scale(286) + 4 + screen.scale(128) + 4 + 10, placed.abs.1 + y + h / 2);
    s.menu.sounds.clear();
    s.list_hover(screen, key2);
    s.list_press(&ui, screen, key2, MOUSE_LEFT, &mut keys);
    assert!(s.capturing(), "a click on a key column captures");
    assert_eq!(s.menu.sounds, ["UI_Menu_Focus", "UI_Menu_Accept"]);
    let overlay = s.menu.descendant(&s.menu.find(&["SettingsPanel", "ControlsPCPanelContainer"]).unwrap(), "ruipanel_keybindlist").unwrap();
    assert_eq!(s.menu.panel(&overlay).unwrap().settings.str("visible"), Some("1"), "the overlay shows while capturing");
    s.list_key(&ui, key_code("UPARROW").unwrap(), &mut keys);
    assert!(!s.capturing());
    assert_eq!(s.menu.bind_list.as_ref().unwrap().items[forward].keys, ["w".to_string(), "UPARROW".to_string()]);
    assert!(keys.lines().contains(&"bind_US_standard \"UPARROW\" \"+forward\" 1".to_string()));
    s.list_press(&ui, screen, key2, MOUSE_LEFT, &mut keys);
    s.list_key(&ui, ESCAPE, &mut keys);
    assert_eq!(s.menu.panel(&overlay).unwrap().settings.str("visible"), Some("0"));
    let mut frame = Frame::default();
    frame.screen_blur = Some([1920, 1080]);
    let mut draws = Draws::default();
    let (images, stops) = s.menu.draw(&ui, &Globals::new(0.0, 1920.0, 1080.0), screen, (1920, 1080), &Topology::hud(1920.0, 1080.0), &mut frame, &mut draws);
    assert!(stops.is_empty(), "{stops:?}");
    assert!(images.iter().filter(|(_, p)| matches!(p, Paint::Fill { .. })).count() > 10, "the key columns' backgrounds");
}
