//! VGUI's Label (vtable `0x137b380`) with its TextImage (vtable `0x138f1e0`), as R5Reloaded draws
//! them: with `rui_overrideVguiTextRendering` 1 (its image value) the text is not drawn from the
//! surface's font pages but by the RUI element `drawtext_vgui`, made for the frame by
//! `0x720570`, in the face `Label.RuiFont` names, measured by the same face's metrics
//! (`0x2f5190`).
//!
//! Not modelled: `^` colour codes and their `altColor` arguments (`0x4ff4c0`), wrapping
//! (`wrap`, `centerwrap`), extra images beside the text, `associate`, the disabled look (drawn
//! twice, offset), a `%var%` label, `ruiVisible` 0, and a text token the localization lacks
//! (shown as the token here; what the engine shows is not traced).

use super::{atoi, color, label_text, Screen};
use crate::assets::keyvalues::Key;
use crate::assets::rui::font_slot;
use crate::assets::vgui::Scheme;
use crate::rui::{Rui, Ui};

/// `rui_defaultFontFace` (image value): the face a missing one falls back to (`0x2f5100`).
const DEFAULT_FACE: &str = "DefaultRegularFont";

/// The element a TextImage draws with, `drawtext_vgui_centered` when it is centred (`+0x32a`,
/// which a Label's own text never is).
pub const ELEMENT: &str = "drawtext_vgui";

#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    /// The text as the TextImage holds it: localized when it starts with `#`.
    pub text: String,
    /// `allcaps` (the TextImage's flag `0x20`).
    pub caps: bool,
    /// `fontHeight`, else the scheme's `Label.RuiFontHeight` (`+0x304`, the TextImage's `+0x330`):
    /// in 1080ths of the screen.
    pub height: i32,
    /// The face's slot in the font table (`+0x300`, the TextImage's `+0x32c`).
    pub face: u16,
    pub color: [u8; 4],
    /// `textAlignment`, `north-west` 0 to `south-east` 8; `west` (3) without one (`0x4ab2c0`).
    pub alignment: u8,
    /// `textinsetx` and `textinsety`, and `use_proportional_insets`, which scales the first.
    pub inset: (i32, i32),
    pub proportional_insets: bool,
    /// `auto_wide_tocontents`, `auto_tall_tocontents`.
    pub auto: (bool, bool),
}

/// `0x2f5100`: the slot of the face `name`, or of the default face when that slot is empty.
pub fn face(ui: &Ui, name: &str) -> u16 {
    let slot = font_slot(name);
    if ui.font(slot).is_some() { slot } else { font_slot(DEFAULT_FACE) }
}

/// A scheme colour (GetSchemeColor): `BaseSettings`' value of `name`, itself a colour or the
/// name of one in `Colors`.
fn scheme_color(scheme: &Scheme, name: &str) -> Option<[u8; 4]> {
    let v = scheme.base.str(name)?;
    color(v).or_else(|| scheme.colors.str(v).and_then(color))
}

const ALIGNMENTS: [&str; 9] = ["north-west", "north", "north-east", "west", "center", "east", "south-west", "south", "south-east"];

impl Label {
    /// Label's ApplySettings (`0x4ad440`) and ApplySchemeSettings (`0x4acc20`) on the panel's
    /// settings: `labelText`, `textAlignment`, `fontHeight` (−1 is none), `allcaps`, the insets
    /// and `auto_*_tocontents`; the scheme's `Label.RuiFont`, `Label.RuiFontHeight` and
    /// `Label.TextColor`, which `fgcolor_override` replaces.
    pub fn new(ui: &Ui, s: &Key, scheme: &Scheme) -> Label {
        let int = |k: &str| s.str(k).map(atoi);
        let text = label_text(ui, s.str("labelText").unwrap_or_default());
        let height = int("fontHeight").filter(|&h| h != -1).unwrap_or_else(|| scheme.base.str("Label.RuiFontHeight").map_or(0, atoi));
        let alignment = s.str("textAlignment").and_then(|a| ALIGNMENTS.iter().position(|n| n.eq_ignore_ascii_case(a))).unwrap_or(3) as u8;
        let color = s.str("fgcolor_override").and_then(color).or_else(|| scheme_color(scheme, "Label.TextColor")).unwrap_or([255; 4]);
        Label {
            text,
            caps: int("allcaps").is_some_and(|v| v > 0),
            height,
            face: face(ui, scheme.base.str("Label.RuiFont").unwrap_or(DEFAULT_FACE)),
            color,
            alignment,
            inset: (int("textinsetx").unwrap_or(0), int("textinsety").unwrap_or(0)),
            proportional_insets: int("use_proportional_insets").is_some_and(|v| v > 0),
            auto: (int("auto_wide_tocontents").is_some_and(|v| v > 0), int("auto_tall_tocontents").is_some_and(|v| v > 0)),
        }
    }

    /// The text's height in pixels (`0x4ff7b0`, `0x500540`): the font height times the 16:9
    /// screen's height over 1080, in integers.
    pub fn pixels(&self, screen: Screen) -> i32 {
        (self.height * screen.fixed_aspect().1) / 1080
    }

    /// The TextImage's content size (`0x500540` through surface `+0x2f8`, `0x543750`): the
    /// text's size in ems times its height, truncated.
    pub fn content(&self, ui: &Ui, screen: Screen) -> (i32, i32) {
        let h = self.pixels(screen) as f32;
        let Ok(c) = std::ffi::CString::new(self.text.as_str()) else { return (0, 0) };
        // SAFETY: a CString is NUL-terminated.
        let [w, lines] = unsafe { ui.text_size(c.as_ptr() as *const u8, self.face, self.caps) };
        ((w * h) as i32, (lines * h) as i32)
    }

    /// The Label's own content size (`0x4ab3a0`), what `auto_wide_tocontents` and
    /// `auto_tall_tocontents` size it to (`0x4ae230`): the text's, the inset added across, and
    /// down at least the inset plus the text.
    pub fn size_to_contents(&self, content: (i32, i32), screen: Screen) -> (i32, i32) {
        let inset = self.inset(screen);
        (inset.0 + content.0, content.1.max(inset.1 + content.1))
    }

    /// The insets in pixels (`0x4ad440`: only the first scaled).
    pub fn inset(&self, screen: Screen) -> (i32, i32) {
        (if self.proportional_insets { screen.scale(self.inset.0) } else { self.inset.0 }, self.inset.1)
    }

    /// Where the text goes in a panel of `size` (PerformLayout `0x4ae010`, ComputeAlignment
    /// `0x4ab9f0`, Paint `0x4abbc0`): the TextImage at the content's width, no wider than the
    /// panel less the inset; placed by the alignment's column (west when it is wider than the
    /// panel) and row; the inset added on the west, taken off on the east, and added down.
    pub fn origin(&self, size: (i32, i32), content: (i32, i32), screen: Screen) -> (i32, i32) {
        let inset = self.inset(screen);
        let w = content.0.min(size.0 - inset.0);
        let h = content.1;
        let col = if size.0 < w { 0 } else { self.alignment % 3 };
        let x = match col {
            0 => 0,
            1 => (size.0 - w) / 2,
            _ => size.0 - w,
        };
        let y = match self.alignment / 3 {
            0 => 0,
            1 => (size.1 - h) / 2,
            _ => size.1 - h,
        };
        let dx = match self.alignment % 3 {
            0 => inset.0,
            2 => -inset.0,
            _ => 0,
        };
        (x + dx, y + inset.1)
    }

    /// The arguments `0x720570` sets on the frame's `drawtext_vgui` from the TextImage (`0x4ff7b0`):
    /// the text at `pos` (the panel's absolute position plus the origin), its height in pixels,
    /// the colour (sRGB to linear, `0x270130`; the panel's alpha is 1), `clipRect` the panel's
    /// clip rectangle as position and size, no wrap, `textAlign` 0. The instance is the size of
    /// the target in pixels (`0x720100`).
    pub fn paint(&self, ui: &Ui, rui: &mut Rui, screen: Screen, target: (i32, i32), pos: (i32, i32), clip: (i32, i32, i32, i32)) {
        rui.size = [target.0 as f32, target.1 as f32];
        rui.inv_size = [1.0 / rui.size[0], 1.0 / rui.size[1]];
        let own = ui.lib.elements.get(&rui.name).map_or(0.0, |e| e.size[0]);
        rui.set_string(ui, "text", &self.text);
        rui.set_face(ui, "face", self.face);
        rui.set_floats(ui, "pos", &[pos.0 as f32, pos.1 as f32]);
        rui.set_float(ui, "height", self.pixels(screen) as f32);
        rui.set_float(ui, "lineBreakWidth", (own / rui.size[0]) * f32::MAX);
        let c = srgb_to_linear(self.color.map(|b| b as f32 * (1.0 / 255.0)));
        rui.set_floats(ui, "color", &c);
        rui.set_floats(ui, "rcpActualRes", &rui.inv_size.clone());
        rui.set_bool(ui, "allCaps", self.caps);
        let (x0, y0, x1, y1) = clip;
        rui.set_floats(ui, "clipRect", &[x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32]);
        rui.set_float(ui, "textAlign", 0.0);
        rui.set_int(ui, "underlineStyle", 0);
        for name in ["altColor1", "altColor2", "altColor3"] {
            rui.set_floats(ui, name, &[0.0, 0.0, 0.0, 1.0]);
        }
    }
}

/// A RuiButton's content width with `behave_as_label` (`0x939100`): its text measured in its
/// `ruiFont` at its `fontHeight` as is (surface `+0x300`, `0x543830`, not upper case), truncated,
/// then times its height over `ruiDefaultHeight` (or its height when it was applied).
pub fn button_width(ui: &Ui, text: &str, face: u16, font_height: i32, default_height: i32, tall: i32) -> i32 {
    let Ok(c) = std::ffi::CString::new(text) else { return 0 };
    // SAFETY: a CString is NUL-terminated.
    let [w, _] = unsafe { ui.text_size(c.as_ptr() as *const u8, face, false) };
    let w = (w * font_height as f32) as i32;
    (w as f32 * (tall as f32 / default_height as f32)) as i32
}

/// `0x270130`: sRGB to linear in four lanes, the fourth (alpha) kept: below 0.04045 times
/// 1 / 12.92, else `((c + 0.055) / 1.055)^2.4`, the power as the cube of the fourth power of a
/// fifth root, the root from a guess in the float's bits (`0x4c2a6698` less the high half of
/// its bits times `0x33333333`) and three Newton steps.
pub fn srgb_to_linear(c: [f32; 4]) -> [f32; 4] {
    let lane = |c: f32, last: bool| {
        let x = (c + f32::from_bits(0x3d6147ae)) * f32::from_bits(0x3f72a76f);
        let mut y = f32::from_bits(0x4c2a6698u32.wrapping_sub(((0x3333_3333u64 * x.to_bits() as u64) >> 32) as u32));
        for _ in 0..3 {
            let yy = y * y;
            let a = (yy * yy) * (y * x);
            y = y + (1.0 - a) * (f32::from_bits(0x3e4ccccd) * y);
        }
        let y = y * x;
        let threshold = if last { f32::MAX } else { f32::from_bits(0x3d25aee6) };
        let k = if last { 1.0 } else { f32::from_bits(0x3d9e8391) };
        if c < threshold { c * k } else { (y * y) * y }
    };
    [lane(c[0], false), lane(c[1], false), lane(c[2], false), lane(c[3], true)]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `srgb_to_linear` against the engine's own `0x270130`, run from `dumps/current.bin` with
    /// the pages it reads laid out at their RVAs, for every byte value and a few others.
    #[test]
    #[cfg(target_arch = "x86_64")]
    fn colours_go_from_srgb_to_linear_as_the_engine_does() {
        use crate::rui::code::sys;
        use std::io::{Read, Seek, SeekFrom};
        use std::arch::x86_64::__m128;
        const IMAGE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/dumps/current.bin");
        const PAGES: [(usize, usize); 5] = [(0x270000, 0x271000), (0x1312000, 0x1313000), (0x1337000, 0x1338000), (0x133a000, 0x133b000), (0x15bd000, 0x15be000)];
        const SPAN: usize = 0x15be000;
        let Ok(mut file) = std::fs::File::open(IMAGE) else { return };
        let base = sys::alloc(SPAN).unwrap();
        for (from, to) in PAGES {
            let mut page = vec![0u8; to - from];
            file.seek(SeekFrom::Start(from as u64)).unwrap();
            file.read_exact(&mut page).unwrap();
            // SAFETY: the pages lie inside the fresh mapping.
            unsafe { std::ptr::copy_nonoverlapping(page.as_ptr(), base.add(from), page.len()) };
        }
        // SAFETY: the code page is part of the mapping.
        assert!(unsafe { sys::protect(base.add(PAGES[0].0), PAGES[0].1 - PAGES[0].0, sys::READ | sys::EXEC) });
        #[allow(improper_ctypes_definitions)]
        type Convert = extern "win64" fn(*const f32) -> __m128;
        // SAFETY: the function is the engine's own, laid out with the constants it reads; it
        // reads four floats at its argument and returns four in xmm0.
        let engine = unsafe { std::mem::transmute::<*mut u8, Convert>(base.add(0x270130)) };
        let mut inputs: Vec<f32> = (0..=255).map(|b| b as f32 * (1.0 / 255.0)).collect();
        inputs.extend([0.04045, 0.0404, 2.0, -0.5]);
        for c in inputs {
            let v = [c, c, c, c];
            // SAFETY: an __m128 is four f32s.
            let theirs: [f32; 4] = unsafe { std::mem::transmute(engine(v.as_ptr())) };
            let ours = srgb_to_linear(v);
            assert!(theirs.iter().zip(&ours).all(|(a, b)| a.to_bits() == b.to_bits()), "{c}: engine {theirs:?}, ours {ours:?}");
        }
        // SAFETY: the whole mapping, unused from here.
        unsafe { sys::free(base, SPAN) };
    }

    #[test]
    fn a_label_places_its_text_by_alignment_and_inset() {
        let screen = Screen { wide: 1920, tall: 1080 };
        let l = Label { text: String::new(), caps: false, height: 26, face: 0, color: [0; 4], alignment: 3, inset: (8, 2), proportional_insets: false, auto: (false, false) };
        assert_eq!(l.origin((200, 40), (100, 26), screen), (8, 7 + 2), "west: the inset across, centred down");
        let l = Label { alignment: 5, ..l };
        assert_eq!(l.origin((200, 40), (100, 26), screen), (92, 9), "east: the inset off the far side");
        assert_eq!(l.origin((50, 40), (100, 26), screen), (0, 9), "no wider than the panel less the inset");
        assert_eq!(l.size_to_contents((100, 26), screen), (108, 28));
        let big = Screen { wide: 2560, tall: 1440 };
        assert_eq!(l.pixels(big), 34, "26 × 1440 / 1080, truncated");
        assert_eq!(Label { proportional_insets: true, ..l }.inset(big), (10, 2), "only across is scaled");
    }
}
