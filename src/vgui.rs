//! VGUI's panels laid out on a screen. Panel's ApplySettings (`0x4ca510`) gives a panel its
//! position, size, `zpos`, `visible` and `clip` from its settings, scaled from the 1920 × 1080
//! the layouts are made for; VPanel's Solve (`0x52a570`) then puts it on the screen, inside its
//! parent or pinned to a sibling.
//!
//! Not modelled: `AutoResize` and its corner offsets (`0x4c9d50`) but for a slider's drop
//! button, `usetitlesafe` (only with `panel_test_title_safe`, 0), a scheme's own proportional
//! panel (the screen is used), a panel's inset and minimum size (both taken as 0), and the size
//! of a panel with no `wide` or `tall` (a new panel's 64 × 24, `0x4c2d50`; taken as 0 here, as
//! `auto_wide_tocontents` sizes such panels later and is not modelled either).
//! Every panel is taken as proportional, as the menus are made. Painting: an ImagePanel's
//! image only as `scaleImage` draws it (all the menus' do), no tiling, rotation or frames;
//! no panel backgrounds or borders; a RUI drawn where its panel paints.

use crate::assets::keyvalues::{self, atof, atoi, strtod, Key, Kind};
use crate::assets::rpak;
use crate::assets::vgui::Panel;
use std::collections::HashMap;

use crate::rui::draw::Draws;
use crate::rui::{Frame, Globals, Rui, Topology, Ui};

pub mod keybinds;
pub mod label;
pub mod settings;
pub mod slider;

use label::Label;
use slider::{ConVarValue, Slider};

/// The size the layouts are made for (`0x530d60`).
pub const BASE: (i32, i32) = (1920, 1080);

/// The pin corners by name, and where each sits as a fraction of a panel's size (`0x1314f10`,
/// `0x1314f90`).
const CORNERS: [(&str, f32, f32); 9] = [
    ("TOP_LEFT", 0.0, 0.0),
    ("TOP_RIGHT", 1.0, 0.0),
    ("BOTTOM_LEFT", 0.0, 1.0),
    ("BOTTOM_RIGHT", 1.0, 1.0),
    ("TOP", 0.5, 0.0),
    ("RIGHT", 1.0, 0.5),
    ("BOTTOM", 0.5, 1.0),
    ("LEFT", 0.0, 0.5),
    ("CENTER", 0.5, 0.5),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    pub wide: i32,
    pub tall: i32,
}

impl Screen {
    /// The largest 16:9 inside the screen (`0x5458f0`), what scaling reads while
    /// `vgui_EnableFixedAspectScaling` is 1, its default.
    pub fn fixed_aspect(self) -> (i32, i32) {
        let (w, h) = (self.wide as f32, self.tall as f32);
        if 1080.0 / h <= 1920.0 / w { (self.wide, (1080.0 / (1920.0 / w)) as i32) } else { ((1920.0 / (1080.0 / h)) as i32, self.tall) }
    }

    /// GetProportionalScaledValue (`0x523600`, `0x5236d0`).
    pub fn scale(self, value: i32) -> i32 {
        ((self.fixed_aspect().1 as f32 / BASE.1 as f32) * value as f32).floor() as i32
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pin {
    pub sibling: String,
    pub corner: u8,
    pub sibling_corner: u8,
}

/// A panel as laid out: `pos` is its `xpos` and `ypos` (an offset from the corner when it is
/// pinned), `abs` where Solve put it on the screen, `clip_rect` what it may draw in.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub name: String,
    pub pos: (i32, i32),
    pub size: (i32, i32),
    pub zpos: i32,
    pub visible: bool,
    pub clip: bool,
    pub pin: Option<Pin>,
    pub abs: (i32, i32),
    pub clip_rect: (i32, i32, i32, i32),
    pub children: Vec<Placed>,
}

impl Placed {
    pub fn child(&self, name: &str) -> Option<&Placed> {
        self.children.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }
}

/// The panels of a layout on a screen, solved until no pin moves.
pub fn layout(panels: &[Panel], screen: Screen) -> Vec<Placed> {
    layout_with(panels, screen, &HashMap::new())
}

/// A panel's position or size set in pixels since its settings were applied, by the scripts
/// (`Hud_SetY`) or by its parent's code (SetPos, SetBounds).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Moved {
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub w: Option<i32>,
    pub h: Option<i32>,
}

/// `layout` with the panels moved since.
pub fn layout_with(panels: &[Panel], screen: Screen, moved: &HashMap<Vec<usize>, Moved>) -> Vec<Placed> {
    let mut placed: Vec<Placed> = panels.iter().map(|p| apply_settings(p, (screen.wide, screen.tall), screen)).collect();
    for (path, m) in moved {
        let Some((first, rest)) = path.split_first() else { continue };
        if let Some(p) = rest.iter().try_fold(placed.get_mut(*first), |p, &i| Some(p?.children.get_mut(i))).flatten() {
            p.pos = (m.x.unwrap_or(p.pos.0), m.y.unwrap_or(p.pos.1));
            p.size = (m.w.unwrap_or(p.size.0), m.h.unwrap_or(p.size.1));
        }
    }
    let root = (0, 0, screen.wide as i16 as i32, screen.tall as i16 as i32);
    for _ in 0..16 {
        let before = placed.clone();
        solve_traverse(&mut placed, (0, 0), root);
        if placed == before {
            break;
        }
    }
    placed
}

/// `xpos` and `ypos`: `r` from the far side, `c` from the middle, `%` of the reference, else
/// scaled.
fn position(value: &str, reference: i32, screen: Screen) -> i32 {
    match value.as_bytes().first() {
        Some(b'r' | b'R') => reference - screen.scale(atoi(&value[1..])),
        Some(b'c' | b'C') => screen.scale(atoi(&value[1..])) + reference / 2,
        Some(b'%') => ((atof(&value[1..]) as f32 / 100.0) * reference as f32) as i32,
        _ => screen.scale(atoi(value)),
    }
}

/// `wide` and `tall`: `f` is the reference less the value, `%` of the reference, else scaled.
fn size(value: &str, reference: i32, screen: Screen) -> i32 {
    match value.as_bytes().first() {
        Some(b'f' | b'F') => reference - screen.scale(atoi(&value[1..])),
        Some(b'%') => ((atof(&value[1..]) as f32 / 100.0) * reference as f32) as i32,
        _ => screen.scale(atoi(value)),
    }
}

/// The layout part of `0x4ca510`, then the panel's children with its size as their parent's.
/// The reference for `r`, `c`, `f` and `%` is the screen, or the parent with
/// `proportionalToParent` 1.
fn apply_settings(p: &Panel, parent: (i32, i32), screen: Screen) -> Placed {
    let s = &p.settings;
    let int = |key: &str| s.str(key).map(atoi);
    let reference = if int("proportionalToParent") == Some(1) { parent } else { (screen.wide, screen.tall) };
    let x = s.str("xpos").map_or(0, |v| position(v, reference.0, screen));
    let y = s.str("ypos").map_or(0, |v| position(v, reference.1, screen));
    let w = s.str("wide").map_or(0, |v| size(v, reference.0, screen));
    let h = s.str("tall").map_or(0, |v| size(v, reference.1, screen));
    let corner = |key: &str| {
        let v = s.str(key).unwrap_or_default();
        CORNERS.iter().position(|c| c.0 == v).map_or(atoi(v) as u8, |i| i as u8)
    };
    let pin = s.str("pin_to_sibling").map(|name| Pin { sibling: name.to_string(), corner: corner("pin_corner_to_sibling"), sibling_corner: corner("pin_to_sibling_corner") });
    let size = (w.max(0) as i16 as i32, h.max(0) as i16 as i32);
    Placed {
        name: p.name.clone(),
        pos: (x as i16 as i32, y as i16 as i32),
        size,
        zpos: int("zpos").unwrap_or(0),
        visible: int("visible").is_none_or(|v| v != 0),
        clip: int("clip").is_none_or(|v| v != 0),
        pin,
        abs: (0, 0),
        clip_rect: (0, 0, 0, 0),
        children: p.children.iter().map(|c| apply_settings(c, size, screen)).collect(),
    }
}

/// VPanel's Solve for each panel, then its children (the surface's SolveTraverse). A pinned
/// panel reads its sibling where the last solve left it.
fn solve_traverse(panels: &mut [Placed], parent_abs: (i32, i32), parent_clip: (i32, i32, i32, i32)) {
    for i in 0..panels.len() {
        let sibling = panels[i].pin.as_ref().and_then(|pin| {
            let s = panels.iter().position(|s| s.name.eq_ignore_ascii_case(&pin.sibling))?;
            Some((panels[s].abs, panels[s].size, pin.corner, pin.sibling_corner))
        });
        let p = &mut panels[i];
        let mut local = p.pos;
        if let Some((abs, size, corner, sibling_corner)) = sibling {
            local = (
                pin_axis(abs.0 - parent_abs.0, size.0, p.size.0, CORNERS.get(sibling_corner as usize).map_or(0.0, |c| c.1), CORNERS.get(corner as usize).map_or(0.0, |c| c.1), if matches!(sibling_corner, 0 | 2 | 7) { -1 } else { 1 }, p.pos.0),
                pin_axis(abs.1 - parent_abs.1, size.1, p.size.1, CORNERS.get(sibling_corner as usize).map_or(0.0, |c| c.2), CORNERS.get(corner as usize).map_or(0.0, |c| c.2), if matches!(sibling_corner, 0 | 1 | 4) { -1 } else { 1 }, p.pos.1),
            );
        }
        let clamp = |v: i32| v.clamp(-0x7fff, 0x7fff);
        p.abs = (clamp(local.0 + parent_abs.0), clamp(local.1 + parent_abs.1));
        let mut rect = (p.abs.0, p.abs.1, clamp(p.abs.0 + p.size.0), clamp(p.abs.1 + p.size.1));
        rect.0 = rect.0.max(parent_clip.0);
        rect.1 = rect.1.max(parent_clip.1);
        rect.2 = rect.2.min(parent_clip.2).max(rect.0);
        rect.3 = rect.3.min(parent_clip.3).max(rect.1);
        p.clip_rect = if p.clip { rect } else { parent_clip };
        let (abs, clip) = (p.abs, p.clip_rect);
        solve_traverse(&mut p.children, abs, clip);
    }
}

/// One axis of a pinned panel (`0x52a570`): the sibling's corner, less the panel's own, then
/// the offset, away from the sibling on its left and top corners.
fn pin_axis(sibling_pos: i32, sibling_size: i32, own_size: i32, sibling_frac: f32, own_frac: f32, sign: i32, offset: i32) -> i32 {
    let corner = (sibling_size as f32 * sibling_frac + sibling_pos as f32) as i32;
    (((corner as f32 - own_size as f32 * own_frac) as i32) as i16).wrapping_add((sign * offset) as i16) as i32
}

/// What a panel paints, in paint order (`paint_list`). A RUI is the panel's instance, kept
/// by the caller under the panel's path (child indices from the layout's root list).
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Fill { rect: (i32, i32, i32, i32), color: [u8; 4], clip: (i32, i32, i32, i32) },
    Image { texture: String, rect: (i32, i32, i32, i32), color: [u8; 4], clip: (i32, i32, i32, i32) },
    Rui { path: Vec<usize> },
    /// A Label's text, drawn by a RUI made for the frame (`label::ELEMENT`).
    Text { path: Vec<usize> },
    /// A SectionedListPanel: what its rows draw (`keybinds::BindList::paints`).
    List { path: Vec<usize> },
    /// A text a panel's code draws itself, at a place in the screen.
    Label { label: Label, pos: (i32, i32), clip: (i32, i32, i32, i32) },
}

/// The paints of the visible panels: each panel, then its children in `zpos` order, kept
/// stable as VPanel's SetZPos (`0x52b6b0`) moves a panel past siblings of a greater or lesser
/// `zpos`. A panel whose clip rectangle is empty paints nothing itself, its children still
/// traversed (Panel's PaintTraverse, `0x4c4130`: the clip at `0x4c42eb`).
pub fn paint_list(panels: &[Panel], placed: &[Placed], root_clip: (i32, i32, i32, i32)) -> Vec<Paint> {
    let mut out = Vec::new();
    paint_children(panels, placed, root_clip, &mut Vec::new(), &mut out);
    out
}

fn paint_children(panels: &[Panel], placed: &[Placed], parent_clip: (i32, i32, i32, i32), path: &mut Vec<usize>, out: &mut Vec<Paint>) {
    let mut order: Vec<usize> = (0..panels.len().min(placed.len())).collect();
    order.sort_by_key(|&i| placed[i].zpos as i16);
    for i in order {
        let (panel, p) = (&panels[i], &placed[i]);
        if !p.visible {
            continue;
        }
        path.push(i);
        let (x0, y0, x1, y1) = p.clip_rect;
        match panel.class.as_str() {
            _ if x1 <= x0 || y1 <= y0 => {}
            "ImagePanel" => out.extend(image(p, &panel.settings, parent_clip)),
            "RuiPanel" | "RuiButton" => out.push(Paint::Rui { path: path.clone() }),
            "Label" => out.push(Paint::Text { path: path.clone() }),
            "SectionedListPanel" => out.push(Paint::List { path: path.clone() }),
            _ => {}
        }
        paint_children(&panel.children, &p.children, p.clip_rect, path, out);
        path.pop();
    }
}

/// `r g b a` as `%d %d %d %d`, alpha 255 when left out; None with fewer than three, which
/// the engine reads as a scheme colour's name.
fn color(text: &str) -> Option<[u8; 4]> {
    let mut v = [0i32, 0, 0, 255];
    let mut rest = text;
    let mut n = 0;
    while n < 4 {
        let t = rest.trim_start();
        let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
        if !digits.starts_with(|c: char| c.is_ascii_digit()) {
            break;
        }
        v[n] = atoi(t);
        let used = t.len() - digits.len() + digits.bytes().take_while(u8::is_ascii_digit).count();
        rest = &t[used..];
        n += 1;
    }
    (3 <= n).then(|| v.map(|c| c as u8))
}

/// An ImagePanel's paint (`0x4a94f0`): `fillcolor` over the panel, then the image stretched
/// to it with `scaleImage`, tinted by `drawColor`, both clipped to the parent's clip
/// rectangle. The image is the texture `texture/NAME.rpak` (scheme GetImage `0x523a00` drops a
/// leading `material/` and a `_gen.rpak`; the texture's name is not traced, but each the menus
/// use is there).
fn image(p: &Placed, s: &Key, parent_clip: (i32, i32, i32, i32)) -> Vec<Paint> {
    let rect = (p.abs.0, p.abs.1, p.abs.0 + p.size.0, p.abs.1 + p.size.1);
    let mut out = Vec::new();
    if let Some(c) = s.str("fillcolor").and_then(color).filter(|c| c[3] != 0) {
        out.push(Paint::Fill { rect, color: c, clip: parent_clip });
    }
    if let Some(name) = s.str("image").filter(|n| !n.is_empty()) {
        let name = name.strip_prefix("material/").or_else(|| name.strip_prefix("material\\")).unwrap_or(name);
        let name = name.find("_gen.rpak").map_or(name, |at| &name[..at]);
        let color = s.str("drawColor").and_then(color).unwrap_or([255; 4]);
        out.push(Paint::Image { texture: format!("texture/{name}.rpak"), rect, color, clip: parent_clip });
    }
    out
}

/// A label's text as its TextImage keeps it (Label's ApplySettings `0x4ad440`, `0x4fee10`): a
/// `#TOKEN` localized when the token is found, else the text as written. A `%name%` dialog
/// variable is not modelled.
pub fn label_text(ui: &Ui, text: &str) -> String {
    text.strip_prefix('#').and_then(|_| ui.localized(text.as_bytes())).map_or_else(|| text.to_string(), String::from_utf16_lossy)
}

/// The bool argument each button state sets (`0x131a9d0`), by the state's index, the same as
/// its child group's in `CHILD_GROUPS`.
pub const STATE_ARGS: [Option<&str>; 7] = [None, None, Some("isDisabled"), Some("isFocused"), Some("isSelected"), Some("isLocked"), Some("isNew")];

/// A button state's argument on an instance (`0x9388b0`): set on a RuiButton's own instance and
/// on its Always group's (`0x949880`, `0x9390b0`).
pub fn set_state(ui: &Ui, rui: &mut Rui, state: usize, on: bool) {
    if let Some(Some(name)) = STATE_ARGS.get(state) {
        rui.set_bool(ui, name, on);
    }
}

/// RuiButton's paint before RuiPanel's (`0x938d80`): with `behave_as_label`, `buttonText` is the
/// label's text.
pub fn paint_button_text(ui: &Ui, rui: &mut Rui, settings: &Key, text: &str) {
    if settings.str("behave_as_label").is_some_and(|v| atoi(v) != 0) {
        rui.set_string(ui, "buttonText", text);
    }
}

/// The panel under a point (Panel's IsWithinTraverse `0x4c7fd0`): a visible panel the point is
/// in (`0x4c7f00`: its `polyShape`, else its clip rectangle), then its children from the last
/// painted to the first, else itself unless it has `mouseEnabled` 0. A RuiPanel never is: its
/// A point within a panel's clip rectangle, left and top included, right and bottom not
/// (`0x4c7f00` without a shape).
fn within(p: &Placed, (x, y): (i32, i32)) -> bool {
    let (x0, y0, x1, y1) = p.clip_rect;
    x0 <= x && x < x1 && y0 <= y && y < y1
}

/// constructor (`0x938950`) sets the flag `mouseEnabled` 0 sets, and no setting clears it.
pub fn panel_at(panels: &[Panel], placed: &[Placed], x: i32, y: i32) -> Option<Vec<usize>> {
    let mut path = Vec::new();
    within_children(panels, placed, x, y, &mut path).then_some(path)
}

fn within_children(panels: &[Panel], placed: &[Placed], x: i32, y: i32, path: &mut Vec<usize>) -> bool {
    let mut order: Vec<usize> = (0..panels.len().min(placed.len())).collect();
    order.sort_by_key(|&i| placed[i].zpos as i16);
    for &i in order.iter().rev() {
        let (panel, p) = (&panels[i], &placed[i]);
        let (x0, y0, x1, y1) = p.clip_rect;
        let within = match panel.settings.str("polyShape").filter(|s| !s.is_empty()) {
            Some(shape) => within_shape(&poly_shape(shape), p, x, y),
            None => !(x < x0 || y < y0 || x1 <= x || y1 <= y),
        };
        if !p.visible || !within {
            continue;
        }
        path.push(i);
        if within_children(&panel.children, &p.children, x, y, path) {
            return true;
        }
        if panel.class != "RuiPanel" && panel.settings.str("mouseEnabled").is_none_or(|v| atoi(v) != 0) {
            return true;
        }
        path.pop();
    }
    false
}

/// A `polyShape` as VPanel keeps it (ApplySettings `0x4cb03c`, `0x52af90`, `0x52a260`): the
/// points, fractions of the panel's size, read as `x y` pairs of `atof`s split at spaces and
/// commas; then for each edge in turn, its unit normal `(−dy, dx) / max(|d|, FLT_MIN)` unless
/// one kept already is within 0.001 of parallel (`1 − |dot|`), each with the span of the points
/// along it.
pub fn poly_shape(text: &str) -> Vec<([f32; 2], [f32; 2])> {
    let numbers: Vec<f32> = text.split([' ', ',']).filter(|t| !t.is_empty()).map(|t| atof(t) as f32).collect();
    let points: Vec<[f32; 2]> = numbers.chunks(2).map(|c| [c[0], c.get(1).copied().unwrap_or(0.0)]).collect();
    let mut axes: Vec<[f32; 2]> = Vec::new();
    for (i, p) in points.iter().enumerate() {
        let q = points[(i + 1) % points.len()];
        let (dy, dx) = (q[1] - p[1], q[0] - p[0]);
        let ny = -dy;
        let len = (dx * dx + ny * ny).sqrt();
        let inv = 1.0 / if len > f32::MIN_POSITIVE { len } else { f32::MIN_POSITIVE };
        let n = [ny * inv, dx * inv];
        if !axes.iter().any(|a| 0.001 > (1.0 - (n[1] * a[1] + n[0] * a[0]).abs()) as f64) {
            axes.push(n);
        }
    }
    axes.into_iter()
        .map(|a| {
            let mut span = [f32::MAX, -f32::MAX];
            for p in &points {
                let d = p[1] * a[1] + p[0] * a[0];
                if span[0] > d {
                    span[0] = d;
                }
                if d > span[1] {
                    span[1] = d;
                }
            }
            (a, span)
        })
        .collect()
}

/// VPanel's IsWithin (`0x52b0e0`): the point over the panel's rectangle in 0 to 1 both ways,
/// edges in, then within each axis's span.
fn within_shape(shape: &[([f32; 2], [f32; 2])], p: &Placed, x: i32, y: i32) -> bool {
    let u = (x - p.abs.0) as f32 / p.size.0 as f32;
    let v = (y - p.abs.1) as f32 / p.size.1 as f32;
    if 0.0 > u || u > 1.0 || 0.0 > v || v > 1.0 {
        return false;
    }
    shape.iter().all(|(a, span)| {
        let d = v * a[1] + u * a[0];
        !(span[0] > d) && !(d > span[1])
    })
}

/// A button's state bits, by `CHILD_GROUPS` index, as Button's constructor sets them
/// (`0x497820`): Always only.
pub const NEW_BUTTON: u32 = 1;

/// The cursor entering a button (`0x498450`) or its taking the keyboard focus (`0x498650`):
/// Normal off, Focused when enabled, Disabled when not. The button becomes the one focused
/// button; the one before it gets `unfocused` (`0x498040`).
pub fn focused(state: u32, enabled: bool) -> u32 {
    with(with(with(state, 1, false), 3, enabled), 2, !enabled)
}

/// The cursor leaving a button (`0x498500`), or its losing the focus (`0x497fc0`): Normal when
/// enabled, Focused off, Disabled when not.
pub fn unfocused(state: u32, enabled: bool) -> u32 {
    with(with(with(state, 1, enabled), 3, false), 2, !enabled)
}

fn with(state: u32, bit: u32, on: bool) -> u32 {
    if on { state | 1 << bit } else { state & !(1 << bit) }
}

/// A BaseModHybridButton's sound keys (`0x131aa60`), in its slots' order. Its constructor
/// (`0x944b10`) leaves every slot `__invalid__`; ApplySettings (`0x946bf0`) takes each key the
/// block (or its template) has, an empty one included.
const SOUND_KEYS: [&str; 10] = [
    "sound_back",
    "sound_accept",
    "sound_invalid",
    "sound_countdown",
    "sound_focus",
    "sound_click",
    "sound_deny",
    "sound_mutedeny",
    "sound_title_click1",
    "sound_title_click2",
];
/// What a slot still `__invalid__` plays (`0x92d7b0`): none for the countdown and the title clicks.
const DEFAULT_SOUNDS: [Option<&str>; 8] = [
    Some("UI_Menu_Back"),
    Some("UI_Menu_Accept"),
    Some("Menu.Invalid"),
    None,
    Some("UI_Menu_Focus"),
    Some("Menu.Click"),
    Some("UI_Menu_Deny"),
    Some("Menu.Mute.Deny"),
];
const ACCEPT: usize = 1;
const INVALID: usize = 2;
const FOCUS: usize = 4;
const DENY: usize = 6;
/// The Locked state's bit (`childGroupLocked`).
const LOCKED: u32 = 5;
/// The sound a SliderControl plays when the cursor enters it (`0x94f870`) and when its value
/// changes other than by a drag (SetValue, `0x94dd90`).
const SLIDER_FOCUS: &str = "UI_Menu_Focus";
const SLIDER_ACCEPT: &str = "UI_Menu_Accept";

fn is_hybrid(class: &str) -> bool {
    matches!(class, "BaseModHybridButton" | "RuiButton")
}

/// The element a RuiPanel's `rui` names (`0x720ea0`): `ui/` first, `.rpak` last, found by the
/// name's GUID.
pub fn element<'a>(ui: &'a Ui, path: &str) -> Option<&'a str> {
    let ok = path.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("ui/")) && path.rsplit(['/', '\\']).next().and_then(|f| f.rfind('.').map(|d| &f[d..])).is_some_and(|e| e.eq_ignore_ascii_case(".rpak"));
    let g = rpak::guid(path);
    ok.then(|| ui.lib.elements.keys().find(|n| rpak::guid(&format!("ui/{n}.rpak")) == g).map(String::as_str)).flatten()
}

/// A RuiPanel's or RuiButton's instance (`0x938b10`, `0x937ef0`): the element in draw group 0
/// with sort key 0, its `ruiArgs` set by their KeyValues type.
pub fn create_rui(ui: &Ui, settings: &Key) -> Option<Rui> {
    let mut rui = ui.create(element(ui, settings.str("rui")?)?, 0, 0)?;
    for k in settings.find("ruiArgs").map_or(&[][..], Key::keys) {
        let keyvalues::Value::Str(v) = &k.value else { continue };
        match keyvalues::kind(v) {
            Kind::Str => {
                let _ = rui.set_string(ui, &k.name, v) || [4, 3, 2].into_iter().any(|n| floats(v, n).is_some_and(|f| rui.set_floats(ui, &k.name, &f[..n])));
            }
            Kind::Int(i) => {
                let _ = rui.set_int(ui, &k.name, i) || rui.set_bool(ui, &k.name, i != 0);
            }
            Kind::Float(f) => {
                rui.set_float(ui, &k.name, f);
            }
            Kind::Uint64(_) => {}
        }
    }
    Some(rui)
}

/// `sscanf` of `n` `%f`s: all of them, or None.
fn floats(text: &str, n: usize) -> Option<[f32; 4]> {
    let mut out = [0.0; 4];
    let mut rest = text;
    for v in out.iter_mut().take(n) {
        let (f, used) = strtod(rest);
        if used == 0 {
            return None;
        }
        *v = f as f32;
        rest = &rest[used..];
    }
    Some(out)
}

/// What RuiPanel's paint sets each frame (`0x938b00`; RuiButton's `0x938d80` the same after
/// its `buttonText`): the resolution, `actualRes`, `clipRect` with `clipRui`, and the topology
/// the frame runs with. `target` is the render target's size.
pub fn paint_rui(ui: &Ui, rui: &mut Rui, p: &Placed, settings: &Key, screen: Screen, target: (i32, i32)) -> Topology {
    let (x, y) = (p.abs.0 as f32, p.abs.1 as f32);
    let (w, h) = p.size;
    if 0 < w && 0 < h {
        let tall = screen.fixed_aspect().1 as f32;
        rui.size = [w as f32 * (1080.0 / tall), h as f32 * (1080.0 / tall)];
        rui.inv_size = [1.0 / rui.size[0], 1.0 / rui.size[1]];
    }
    let res = [((1920.0 / target.0 as f32) * w as f32).round_ties_even() as i32 as f32, ((1080.0 / target.1 as f32) * h as f32).round_ties_even() as i32 as f32];
    rui.set_floats(ui, "actualRes", &res);
    if settings.str("clipRui").is_none_or(|v| atoi(v) != 0) {
        let own = ui.lib.elements.get(&rui.name).map_or([0.0; 2], |e| e.size);
        let (cx0, cy0, cx1, cy1) = p.clip_rect;
        let (wf, hf) = (w as f32, h as f32);
        rui.set_floats(ui, "clipRect", &[(cx0 as f32 - x) / wf, (cy0 as f32 - y) / hf, (own[0] / wf) * (cx1 - cx0) as f32, (own[1] / hf) * (cy1 - cy0) as f32]);
    }
    Topology { org: [x, y, 0.0], right: [w as f32, 0.0, 0.0], down: [0.0, h as f32, 0.0] }
}

/// A menu as the engine keeps it between frames: its panels, the RUI instance each RuiPanel
/// and RuiButton made, the buttons' states and the button under the cursor.
pub struct Menu {
    pub panels: Vec<Panel>,
    pub placed: Vec<Placed>,
    ruis: HashMap<Vec<usize>, Rui>,
    texts: HashMap<Vec<usize>, String>,
    states: HashMap<Vec<usize>, u32>,
    pub lists: HashMap<Vec<usize>, ListButton>,
    pub sliders: HashMap<Vec<usize>, Slider>,
    /// `SliderControl.InsetX` from the scheme's `BaseSettings` (`0x94ee50`), before scaling; 0
    /// until `apply_scheme`, as a slider's constructor leaves it.
    slider_inset: i32,
    /// Each Label's text as its ApplySettings and ApplySchemeSettings leave it, and the
    /// scheme's `Label.RuiFontHeight` for buttons without a `fontHeight`.
    pub labels: HashMap<Vec<usize>, Label>,
    label_font_height: i32,
    moved: HashMap<Vec<usize>, Moved>,
    /// The button with the focus: the one under the cursor, or a slider's `BtnDropButton`.
    pub hovered: Option<Vec<usize>>,
    /// The slider under the cursor, highlighted, and the one the left button went down on.
    under_slider: Option<Vec<usize>>,
    captured: Option<Vec<usize>>,
    /// The names handed to the surface's PlaySound (`0x5460a0`) since the shell last took them.
    pub sounds: Vec<String>,
    pub bind_list: Option<keybinds::BindList>,
}

/// A DialogListButton: a BaseModHybridButton whose `style` is 3 in the scheme
/// (`DialogListButton.Style`, the only style that is), the switch rows of the settings menu.
/// ApplySettings (`0x946bf0`) reads its `ConVar` and the choices of its `list`, and finds its
/// children named `RightButton`, `LeftButton` and `ValueButton` (the case aside).
#[derive(Clone, Debug, PartialEq)]
pub struct ListButton {
    pub convar: Option<String>,
    /// Each choice's label (the key's name) and value (as `GetString` (`0x474ae0`) gives it:
    /// a number typed by the KeyValues as `%d` or `%f`).
    pub items: Vec<(String, String)>,
    /// The choice shown (`+0x4a8`).
    pub index: i32,
}

impl ListButton {
    fn new(settings: &Key) -> Option<ListButton> {
        let convar = settings.str("ConVar").map(String::from);
        let items = match settings.find("list").map(|k| &k.value) {
            Some(keyvalues::Value::Block(keys)) => keys
                .iter()
                .map(|k| {
                    let text = match &k.value {
                        keyvalues::Value::Str(s) => s.as_str(),
                        keyvalues::Value::Block(_) => "",
                    };
                    let value = match keyvalues::kind(text) {
                        Kind::Int(i) => i.to_string(),
                        Kind::Float(f) => format!("{:.6}", f as f64),
                        Kind::Uint64(u) => (u as i64).to_string(),
                        Kind::Str => text.to_string(),
                    };
                    (k.name.clone(), value)
                })
                .collect(),
            _ => Vec::new(),
        };
        Some(ListButton { convar, items, index: 0 })
    }

    /// The choice the ConVar holds (`0x9495a0`, run from RuiButton's paint, `0x938d95`): the
    /// first whose value is the ConVar's string, else the one nearest its float. A value whose
    /// `atof` and the ConVar's float are both 0 counts only when it holds the ConVar's string
    /// (`strstr`), and is then nearest.
    pub fn sync(&mut self, string: &str, float: f32) {
        let mut best = f32::MAX;
        self.index = 0;
        for (i, (_, value)) in self.items.iter().enumerate() {
            if value == string {
                self.index = i as i32;
                return;
            }
            let v = atof(value) as f32;
            if float == 0.0 && v == 0.0 {
                if value.contains(string) {
                    self.index = i as i32;
                    best = 0.0;
                }
            } else {
                let d = (v - float).abs();
                if d < best {
                    self.index = i as i32;
                    best = d;
                }
            }
        }
    }

    /// A command from a child (`0x948300`): `DialogListNext` and `DialogListPrev` step to the
    /// next or previous choice round the list, `DialogListSelect<n>` picks choice `n`. Whether
    /// the choice is made (`0x948d70`): a step that lands where it started is not, a pick
    /// always is; none for a command it does not take.
    pub fn command(&mut self, command: &str) -> Option<bool> {
        let count = self.items.len() as i32;
        let step = if command.eq_ignore_ascii_case("DialogListNext") {
            1
        } else if command.eq_ignore_ascii_case("DialogListPrev") {
            -1
        } else {
            0
        };
        if step == 0 {
            let n = command.get(..16).filter(|p| p.eq_ignore_ascii_case("DialogListSelect")).map(|_| atoi(&command[16..]))?;
            if command.len() < 17 || n < 0 || count <= n {
                return None;
            }
            self.index = n;
        } else {
            if count <= 0 {
                return None;
            }
            let i = (count + step + self.index) % count;
            if i == self.index {
                return Some(false);
            }
            self.index = i;
        }
        Some(true)
    }

    /// The ConVar and the value of the choice shown.
    pub fn choice(&self) -> Option<(String, String)> {
        let value = self.items.get(self.index as usize)?.1.clone();
        self.convar.clone().map(|c| (c, value))
    }
}

/// The classes Button's state handling runs for.
fn is_button(class: &str) -> bool {
    matches!(class, "Button" | "BaseModHybridButton" | "RuiButton")
}

impl Menu {
    pub fn new(panels: Vec<Panel>) -> Menu {
        let (mut lists, mut sliders) = (HashMap::new(), HashMap::new());
        fn walk(panels: &[Panel], path: &mut Vec<usize>, lists: &mut HashMap<Vec<usize>, ListButton>, sliders: &mut HashMap<Vec<usize>, Slider>) {
            for (i, p) in panels.iter().enumerate() {
                path.push(i);
                if is_button(&p.class) && p.settings.str("style").is_some_and(|s| s.eq_ignore_ascii_case("DialogListButton")) {
                    lists.extend(ListButton::new(&p.settings).map(|l| (path.clone(), l)));
                }
                if p.class == "SliderControl" {
                    sliders.insert(path.clone(), Slider::new(&p.settings));
                }
                walk(&p.children, path, lists, sliders);
                path.pop();
            }
        }
        walk(&panels, &mut Vec::new(), &mut lists, &mut sliders);
        Menu {
            panels,
            placed: Vec::new(),
            ruis: HashMap::new(),
            texts: HashMap::new(),
            states: HashMap::new(),
            lists,
            sliders,
            slider_inset: 0,
            labels: HashMap::new(),
            label_font_height: 0,
            moved: HashMap::new(),
            hovered: None,
            under_slider: None,
            captured: None,
            sounds: Vec::new(),
            bind_list: None,
        }
    }

    /// What the panels' ApplySchemeSettings read from the scheme: a slider's inset, each
    /// Label's face, height and colour.
    pub fn apply_scheme(&mut self, ui: &Ui, scheme: &crate::assets::vgui::Scheme) {
        self.slider_inset = scheme.base.str("SliderControl.InsetX").map_or(0, atoi);
        self.label_font_height = scheme.base.str("Label.RuiFontHeight").map_or(0, atoi);
        fn walk(ui: &Ui, scheme: &crate::assets::vgui::Scheme, panels: &[Panel], path: &mut Vec<usize>, out: &mut HashMap<Vec<usize>, Label>) {
            for (i, p) in panels.iter().enumerate() {
                path.push(i);
                if p.class == "Label" {
                    out.insert(path.clone(), Label::new(ui, &p.settings, scheme));
                }
                walk(ui, scheme, &p.children, path, out);
                path.pop();
            }
        }
        let mut labels = HashMap::new();
        walk(ui, scheme, &self.panels, &mut Vec::new(), &mut labels);
        self.labels = labels;
    }

    /// Each DialogListButton's choice from its ConVar (`0x9495a0`), `convar` giving a ConVar's
    /// string and float; then its children's arguments (`0x9456e0`, from BaseModHybridButton's
    /// paint, `0x94651d`): the `ValueButton` shows the choice's label with `itemCount` and
    /// `selectedIndex`; `LeftButton` and `RightButton` are selected for choice 0 and 1 and,
    /// with more than one choice, show those two labels.
    pub fn sync_lists(&mut self, ui: &Ui, convar: &dyn Fn(&str) -> Option<(String, f32)>) {
        let paths: Vec<Vec<usize>> = self.lists.keys().cloned().collect();
        for path in paths {
            let list = self.lists.get_mut(&path).unwrap();
            if let Some((s, f)) = list.convar.as_deref().and_then(convar) {
                list.sync(&s, f);
            }
            let list = list.clone();
            let Some(item) = usize::try_from(list.index).ok().and_then(|i| list.items.get(i)) else { continue };
            let child = |m: &Menu, name: &str| m.panel(&path).and_then(|p| p.children.iter().position(|c| c.name.eq_ignore_ascii_case(name))).map(|i| [&path[..], &[i]].concat());
            if let Some(v) = child(self, "ValueButton")
                && let Some(rui) = self.rui(ui, &v)
            {
                rui.set_string(ui, "buttonText", &item.0);
                rui.set_int(ui, "itemCount", list.items.len() as i32);
                rui.set_int(ui, "selectedIndex", list.index);
            }
            for (name, choice) in [("RightButton", 1), ("LeftButton", 0)] {
                if let Some(b) = child(self, name)
                    && let Some(rui) = self.rui(ui, &b)
                {
                    rui.set_bool(ui, "isSelected", list.index == choice);
                    if list.items.len() > 1 {
                        rui.set_string(ui, "buttonText", &list.items[choice as usize].0);
                    }
                }
            }
        }
    }

    /// A click on the button under the cursor. Going down on it (BaseModHybridButton's
    /// OnMousePressed, `0x945050`): when enabled, its accept sound, or its deny sound while
    /// Locked, unless `hasScriptSound` leaves the sound to the scripts. Then its `command` to the
    /// DialogListButton it belongs to (the children BaseModHybridButton finds send it their
    /// actions), which plays its own accept sound on a choice made and its invalid sound on a
    /// step that lands where it started (`0x948d70`); the ConVar write that makes, if any.
    ///
    /// Not modelled: a press on a DialogListButton itself, whose sides step its choice.
    pub fn click(&mut self) -> Option<(String, String)> {
        let path = self.hovered.clone().filter(|_| self.under_slider.is_none())?;
        let script_sound = self.panel(&path)?.settings.str("hasScriptSound").is_some_and(|v| atoi(v) != 0);
        if self.enabled(&path) && !script_sound && !self.lists.contains_key(&path) {
            let locked = self.states.get(&path).unwrap_or(&NEW_BUTTON) >> LOCKED & 1 != 0;
            self.play(&path, if locked { DENY } else { ACCEPT });
        }
        let command = self.panel(&path)?.settings.str("command")?.to_string();
        let parent = &path[..path.len() - 1];
        if self.panel(parent).is_none() || !self.enabled(parent) {
            return None;
        }
        let made = self.lists.get_mut(parent)?.command(&command)?;
        self.play(parent, if made { ACCEPT } else { INVALID });
        made.then(|| self.lists[parent].choice()).flatten()
    }

    fn enabled(&self, path: &[usize]) -> bool {
        self.panel(path).is_some_and(|p| p.settings.str("enabled").is_none_or(|v| atoi(v) != 0))
    }

    /// A button's sound for a slot (`0x9499f0`): its key's name, or the slot's default while the
    /// key is unset; an empty key plays nothing.
    fn play(&mut self, path: &[usize], slot: usize) {
        let Some(p) = self.panel(path).filter(|p| is_hybrid(&p.class)) else { return };
        let name = match p.settings.str(SOUND_KEYS[slot]).unwrap_or("__invalid__") {
            "__invalid__" => DEFAULT_SOUNDS.get(slot).copied().flatten(),
            "" => None,
            s => Some(s),
        };
        self.sounds.extend(name.map(String::from));
    }

    /// The path of the panel reached by `names` from the first root.
    pub fn find(&self, names: &[&str]) -> Option<Vec<usize>> {
        let mut path = vec![0];
        let mut list = &self.panels.first()?.children;
        for n in names {
            let i = list.iter().position(|p| p.name.eq_ignore_ascii_case(n))?;
            path.push(i);
            list = &list[i].children;
        }
        Some(path)
    }

    pub fn panel(&self, path: &[usize]) -> Option<&Panel> {
        let (first, rest) = path.split_first()?;
        rest.iter().try_fold(self.panels.get(*first)?, |p, &i| p.children.get(i))
    }

    fn panel_mut(&mut self, path: &[usize]) -> Option<&mut Panel> {
        let (first, rest) = path.split_first()?;
        rest.iter().try_fold(self.panels.get_mut(*first)?, |p, &i| p.children.get_mut(i))
    }

    pub fn placed_at(&self, path: &[usize]) -> Option<&Placed> {
        let (first, rest) = path.split_first()?;
        rest.iter().try_fold(self.placed.get(*first)?, |p, &i| p.children.get(i))
    }

    /// `Hud_SetVisible`.
    pub fn set_visible(&mut self, path: &[usize], on: bool) {
        if let Some(p) = self.panel_mut(path) {
            p.settings.set("visible", if on { "1" } else { "0" });
        }
    }

    /// `RHud_SetText` (`_menu_utility.nut`) on a RUI panel: the text into its `buttonText`.
    pub fn set_text(&mut self, ui: &Ui, path: &[usize], text: &str) {
        if let Some(rui) = self.ruis.get_mut(path) {
            rui.set_string(ui, "buttonText", text);
        }
        self.texts.insert(path.to_vec(), text.to_string());
    }

    /// The panels laid out, then each slider's children placed as its code places them, and the
    /// panels solved again.
    pub fn layout(&mut self, ui: &Ui, screen: Screen) {
        self.placed = layout_with(&self.panels, screen, &self.moved);
        self.size_labels(ui, screen);
        let paths: Vec<Vec<usize>> = self.sliders.keys().cloned().collect();
        let before = self.moved.clone();
        for path in paths {
            self.place_slider(&path, screen);
        }
        if self.moved != before {
            self.placed = layout_with(&self.panels, screen, &self.moved);
        }
    }

    /// Each Label with `auto_wide_tocontents` or `auto_tall_tocontents` sized to its content
    /// (`0x4ae230`), and each RuiButton with `behave_as_label` and `auto_wide_tocontents`
    /// (RuiButton's ApplySettings `0x938ee0`, its content `0x939100`), then the panels solved
    /// again.
    fn size_labels(&mut self, ui: &Ui, screen: Screen) {
        let before = self.moved.clone();
        let mut buttons = Vec::new();
        fn walk(panels: &[Panel], path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
            for (i, p) in panels.iter().enumerate() {
                path.push(i);
                let on = |k: &str| p.settings.str(k).is_some_and(|v| atoi(v) > 0);
                if p.class == "RuiButton" && on("behave_as_label") && on("auto_wide_tocontents") {
                    out.push(path.clone());
                }
                walk(&p.children, path, out);
                path.pop();
            }
        }
        walk(&self.panels, &mut Vec::new(), &mut buttons);
        for path in buttons {
            let (Some(panel), Some(p)) = (self.panel(&path), self.placed_at(&path)) else { continue };
            let s = &panel.settings;
            let text = self.texts.get(&path).cloned().unwrap_or_else(|| label_text(ui, s.str("labelText").unwrap_or_default()));
            let face = label::face(ui, s.str("ruiFont").unwrap_or_default());
            let font_height = s.str("fontHeight").map(atoi).filter(|&h| h != -1).unwrap_or(self.label_font_height);
            let default_height = s.str("ruiDefaultHeight").map_or(p.size.1, atoi);
            let w = label::button_width(ui, &text, face, font_height, default_height, p.size.1);
            self.moved.entry(path).or_default().w = Some(w);
        }
        for (path, label) in &self.labels {
            if !(label.auto.0 || label.auto.1) {
                continue;
            }
            let (w, h) = label.size_to_contents(label.content(ui, screen), screen);
            let m = self.moved.entry(path.clone()).or_default();
            if label.auto.0 {
                m.w = Some(w);
            }
            if label.auto.1 {
                m.h = Some(h);
            }
        }
        if self.moved != before {
            self.placed = layout_with(&self.panels, screen, &self.moved);
        }
    }

    /// `UpdateFooterElems` with the mouse (`_footer.nut`): the mouse labels of the footer options
    /// the top panel shows, each localized onto the next `LeftRuiFooterButton` (`SetFooterText`:
    /// `Hud_SetText`, shown, enabled as each is clickable), the rest hidden, and every
    /// `RightRuiFooterButton` hidden, as no right option shows in a match.
    pub fn set_footer(&mut self, ui: &Ui, labels: &[&str]) {
        for i in 0.. {
            let Some(path) = self.find(&["FooterButtons", &format!("RightRuiFooterButton{i}")]) else { break };
            self.set_visible(&path, false);
        }
        for i in 0.. {
            let Some(path) = self.find(&["FooterButtons", &format!("LeftRuiFooterButton{i}")]) else { break };
            match labels.get(i) {
                Some(l) => {
                    self.set_text(ui, &path, &label_text(ui, l));
                    self.set_visible(&path, true);
                    self.set(&path, "enabled", "1");
                }
                None => self.set_visible(&path, false),
            }
        }
    }

    /// `Hud_SetY`: the panel's `y` in pixels, kept over its layout's.
    pub fn set_y(&mut self, path: &[usize], y: i32) {
        self.moved.entry(path.to_vec()).or_default().y = Some(y);
    }

    /// A slider's children as its code sets them, in pixels. ApplySettings (`0x94e570`) gives the
    /// slider `BtnDropButton`'s height, centred on where it was. EditablePanel's PerformLayout
    /// (`0x49ebc0`) stretches `BtnDropButton` across it: `autoResize` 1 from the top left, its
    /// offsets taken (`0x4c9d50`) while it and the slider were both a new panel's 64 wide
    /// (`0x4c2d50`). Then `0x94e240`: `PrgValue` at the right, its right edge the inset from the
    /// slider's, centred down it; `PnlDefaultMark` centred on the default's place along the bar,
    /// at the top.
    fn place_slider(&mut self, path: &[usize], screen: Screen) {
        let Some(p) = self.placed_at(path).cloned() else { return };
        let child = |name: &str| p.children.iter().position(|c| c.name.eq_ignore_ascii_case(name)).map(|i| ([path, &[i]].concat(), p.children[i].clone()));
        let (w, mut h) = p.size;
        if let Some((_, drop)) = child("BtnDropButton") {
            let m = self.moved.entry(path.to_vec()).or_default();
            if m.h.is_none() {
                m.y = Some(m.y.unwrap_or(p.pos.1) - (drop.size.1 - h) / 2);
                m.h = Some(drop.size.1);
            }
            h = drop.size.1;
        }
        if let Some((at, _)) = child("BtnDropButton") {
            let m = self.moved.entry(at).or_default();
            (m.x, m.y, m.w) = (Some(0), Some(0), Some(w));
        }
        let Some((prg, bar)) = child("PrgValue") else { return };
        let x = (screen.scale(self.slider_inset) - bar.size.0) + w;
        let m = self.moved.entry(prg).or_default();
        (m.x, m.y) = (Some(x), Some(h / 2 - bar.size.1 / 2));
        let Some(slider) = self.sliders.get(path) else { return };
        let f = slider.default_fraction();
        if let Some((at, mark)) = child("PnlDefaultMark") {
            let m = self.moved.entry(at).or_default();
            (m.x, m.y) = (Some((bar.size.0 as f32 * f + x as f32 - (mark.size.0 / 2) as f32) as i32), Some(0));
        }
    }

    /// The instance a RuiPanel or RuiButton made (`0x937ef0`), made now if it was not yet: with
    /// the text and state arguments set before.
    pub fn rui(&mut self, ui: &Ui, path: &[usize]) -> Option<&mut Rui> {
        if !self.ruis.contains_key(path) {
            let panel = self.panel(path)?;
            let mut rui = create_rui(ui, &panel.settings)?;
            if let Some(t) = self.texts.get(path) {
                rui.set_string(ui, "buttonText", t);
            }
            let state = *self.states.get(path).unwrap_or(&NEW_BUTTON);
            for bit in 0..STATE_ARGS.len() {
                if state >> bit & 1 != 0 {
                    set_state(ui, &mut rui, bit, true);
                }
            }
            self.ruis.insert(path.to_vec(), rui);
        }
        self.ruis.get_mut(path)
    }

    /// The first panel named `name` under `path`, depth first.
    pub fn descendant(&self, path: &[usize], name: &str) -> Option<Vec<usize>> {
        let p = self.panel(path)?;
        for (i, c) in p.children.iter().enumerate() {
            let at = [path, &[i]].concat();
            if c.name.eq_ignore_ascii_case(name) {
                return Some(at);
            }
            if let Some(found) = self.descendant(&at, name) {
                return Some(found);
            }
        }
        None
    }

    /// `Hud_GetChild`: the child of `path` named `name`.
    pub fn child(&self, path: &[usize], name: &str) -> Option<Vec<usize>> {
        let i = self.panel(path)?.children.iter().position(|c| c.name.eq_ignore_ascii_case(name))?;
        Some([path, &[i]].concat())
    }

    /// `Hud_SetSelected` (SetSelected `0x497b30`): the Selected state.
    pub fn set_selected(&mut self, ui: &Ui, path: &[usize], on: bool) {
        self.change_state(ui, path, |s, _| with(s, 4, on));
    }

    /// A setting the scripts change (`Hud_SetWidth`, `Hud_SetY`, ...), in the layout's units.
    pub fn set(&mut self, path: &[usize], key: &str, value: &str) {
        if let Some(p) = self.panel_mut(path) {
            p.settings.set(key, value);
        }
    }

    /// The cursor at `x`, `y` (`0x498450`, `0x498500`): a button it enters takes the focus and the
    /// one it left loses it, each with its state arguments set. An enabled BaseModHybridButton
    /// the cursor enters plays its focus sound (`0x948f70`).
    ///
    /// A slider the cursor enters is highlighted (`0x94fc10`, `0x94fc60`), plays its focus sound
    /// (`0x94f870`) and its `BtnDropButton` takes the focus (NavigateTo, `0x4d3460`); one it
    /// leaves is not highlighted (`0x94fc30`, `0x94fe00`).
    ///
    /// Not modelled: the focus sound's other conditions (`0x948f70`): no flyout open (`+0x484`),
    /// the button not already holding the keyboard focus, which the cursor's buttons here never
    /// do, and the focused panel's leave (`+0x238`, true for every panel here).
    pub fn hover(&mut self, ui: &Ui, x: i32, y: i32) {
        let under = panel_at(&self.panels, &self.placed, x, y);
        let slider = under.clone().filter(|p| self.sliders.contains_key(p));
        if slider != self.under_slider {
            if let Some(old) = self.under_slider.take() {
                self.highlight(ui, &old, false);
            }
            if let Some(new) = &slider {
                self.highlight(ui, new, true);
                self.sounds.push(SLIDER_FOCUS.into());
            }
            self.under_slider = slider.clone();
        }
        let entered = under.filter(|p| self.panel(p).is_some_and(|p| is_button(&p.class)));
        let at = match entered.clone() {
            Some(button) => Some(button),
            None => slider.and_then(|s| self.child(&s, "BtnDropButton")),
        };
        if at == self.hovered {
            return;
        }
        if let Some(old) = self.hovered.take() {
            self.change_state(ui, &old, unfocused);
        }
        if let Some(new) = &at {
            self.change_state(ui, new, focused);
        }
        if let Some(b) = entered.filter(|b| self.enabled(b)) {
            self.play(&b, FOCUS);
        }
        self.hovered = at;
    }

    /// A slider highlighted or not (`0x94fc60`, `0x94fe00`): `isFocused` on `BtnDropButton`'s
    /// and `PrgValue`'s instances.
    fn highlight(&mut self, ui: &Ui, slider: &[usize], on: bool) {
        for name in ["BtnDropButton", "PrgValue"] {
            if let Some(at) = self.child(slider, name)
                && let Some(rui) = self.rui(ui, &at)
            {
                rui.set_bool(ui, "isFocused", on);
            }
        }
    }

    /// Each slider as its paint leaves it (`0x950180`): the value taken from its ConVar when that
    /// differs (SetValue forced, so kept within the range); then `PrgValue`'s `progress` and
    /// `LblSliderText`'s text from it. The ConVar writes that makes: SetValue hands the ConVar
    /// the value, which differs from the ConVar's only when the range clamped it.
    pub fn sync_sliders(&mut self, ui: &Ui, convar: &dyn Fn(&str) -> Option<ConVarValue>) -> Vec<(String, f32)> {
        let mut writes = Vec::new();
        let paths: Vec<Vec<usize>> = self.sliders.keys().cloned().collect();
        for path in paths {
            let slider = self.sliders.get_mut(&path).unwrap();
            let c = slider.convar.as_deref().and_then(convar);
            slider.default = slider.default_convar.as_deref().and_then(convar).or(c).map(|c| c.default);
            if let Some(c) = c
                && c.float != slider.value
            {
                slider.set(c.float, true);
                if slider.value != c.float {
                    writes.extend(slider.convar.clone().map(|n| (n, slider.value)));
                }
            }
            let slider = slider.clone();
            self.paint_slider(ui, &path, &slider, c);
        }
        writes
    }

    /// What SetValue and its label (`0x94ff80`) set on a slider's instances.
    fn paint_slider(&mut self, ui: &Ui, path: &[usize], slider: &Slider, c: Option<ConVarValue>) {
        if let Some(at) = self.child(path, "PrgValue")
            && let Some(rui) = self.rui(ui, &at)
        {
            rui.set_float(ui, "progress", slider.fraction());
        }
        if let Some(at) = self.child(path, "LblSliderText")
            && let Some(rui) = self.rui(ui, &at)
        {
            rui.set_string(ui, "buttonText", &slider.label(c, slider.fraction()));
        }
    }

    /// `PrgValue`'s absolute rectangle on a slider.
    fn slider_bar(&self, path: &[usize]) -> Option<((i32, i32), (i32, i32))> {
        let p = self.placed_at(&self.child(path, "PrgValue")?)?;
        Some((p.abs, p.size))
    }

    /// The left button going down on a slider: its press handler (`0x94f600`, reached through
    /// Panel's `0x4d2db0` because PerformLayout `0x94f040` enables dragging) takes the cursor's
    /// moves from then on and sets the value from the cursor, with SetValue's accept sound when
    /// that changes it; the drag after is silent. The ConVar write that makes.
    pub fn press(&mut self, ui: &Ui, x: i32, y: i32) -> Option<(String, f32)> {
        let path = panel_at(&self.panels, &self.placed, x, y).filter(|p| self.sliders.contains_key(p))?;
        self.captured = Some(path);
        self.drag(ui, x, y)
    }

    /// The cursor moved while a slider has it (`0x4c5290` to `0x94f6f0`): on the bar, or anywhere
    /// once it set the value, the value follows the cursor along the bar.
    pub fn drag(&mut self, ui: &Ui, x: i32, y: i32) -> Option<(String, f32)> {
        let path = self.captured.clone()?;
        let bar = self.slider_bar(&path)?;
        let slider = self.sliders.get_mut(&path)?;
        let v = slider.at_cursor((x, y), bar)?;
        let (changed, sound) = slider.set(v, false);
        if changed && sound {
            self.sounds.push(SLIDER_ACCEPT.into());
        }
        let slider = self.sliders.get_mut(&path)?;
        slider.dragging = true;
        let slider = slider.clone();
        let written = slider.convar.as_ref().map(|_| ConVarValue { float: slider.value, int: slider.value as i32, default: slider.default.unwrap_or_default() });
        self.paint_slider(ui, &path, &slider, written);
        changed.then(|| slider.convar.clone().map(|c| (c, slider.value))).flatten()
    }

    /// The left button released (`0x94f710`): the drag ends, and off the bar the slider loses
    /// the focus (`0x94f9b0`), so it is not highlighted.
    pub fn release(&mut self, ui: &Ui, x: i32, y: i32) {
        let Some(path) = self.captured.take() else { return };
        let Some(bar) = self.slider_bar(&path) else { return };
        let Some(slider) = self.sliders.get_mut(&path) else { return };
        if !slider.dragging {
            return;
        }
        slider.dragging = false;
        let ((bx, by), (bw, bh)) = bar;
        if x < bx || bx + bw < x || y < by || by + bh < y {
            self.highlight(ui, &path, false);
        }
    }

    fn change_state(&mut self, ui: &Ui, path: &[usize], change: impl Fn(u32, bool) -> u32) {
        let Some(panel) = self.panel(path) else { return };
        let enabled = panel.settings.str("enabled").is_none_or(|v| atoi(v) != 0);
        let always: Vec<Vec<usize>> = panel.children.iter().enumerate().filter(|(_, c)| c.group == Some(0)).map(|(i, _)| [path, &[i]].concat()).collect();
        let before = *self.states.get(path).unwrap_or(&NEW_BUTTON);
        let after = change(before, enabled);
        self.states.insert(path.to_vec(), after);
        for bit in 0..STATE_ARGS.len() {
            let on = after >> bit & 1 != 0;
            if (before >> bit & 1 != 0) == on {
                continue;
            }
            for p in std::iter::once(path.to_vec()).chain(always.iter().cloned()) {
                if let Some(rui) = self.ruis.get_mut(&p) {
                    set_state(ui, rui, bit, on);
                }
            }
        }
    }

    /// The menu's paints for this frame: each RUI run with its panel's topology, its quads
    /// placed in `into`'s unit square and appended to `out`; each image with the number of quads
    /// before it. A RUI that stops is named in the second list.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, ui: &Ui, globals: &Globals, screen: Screen, target: (i32, i32), into: &Topology, frame: &mut Frame, out: &mut Draws) -> (Vec<(usize, Paint)>, Vec<String>) {
        let (mut images, mut stops) = (Vec::new(), Vec::new());
        let screen_plane = Topology { org: [0.0; 3], right: [screen.wide as f32, 0.0, 0.0], down: [0.0, screen.tall as f32, 0.0] };
        let mut paints = Vec::new();
        for paint in paint_list(&self.panels, &self.placed, (0, 0, screen.wide, screen.tall)) {
            match paint {
                Paint::List { path } => {
                    if let (Some(list), Some(p)) = (self.bind_list.as_ref().filter(|l| l.path == path), self.placed_at(&path)) {
                        paints.extend(list.paints(ui, p, screen));
                    }
                }
                Paint::Text { path } => {
                    let (Some(label), Some(p)) = (self.labels.get(&path), self.placed_at(&path)) else { continue };
                    let o = label.origin(p.size, label.content(ui, screen), screen);
                    paints.push(Paint::Label { label: label.clone(), pos: (p.abs.0 + o.0, p.abs.1 + o.1), clip: p.clip_rect });
                }
                other => paints.push(other),
            }
        }
        for paint in paints {
            if let Paint::Label { label, pos, clip } = &paint {
                if label.text.is_empty() {
                    continue;
                }
                let Some(mut rui) = ui.create(label::ELEMENT, 0, 0) else { continue };
                label.paint(ui, &mut rui, screen, target, *pos, *clip);
                let first = out.quads.len();
                match ui.update(&mut rui, globals, &screen_plane, frame) {
                    Ok(true) => ui.draws(&rui, frame, out),
                    Ok(false) => {}
                    Err(stop) => stops.push(format!("{}: {stop:?}", rui.name)),
                }
                for q in &mut out.quads[first..] {
                    for c in &mut q.corners {
                        let at = screen_plane.place(c[0], c[1]);
                        *c = [(at[0] - into.org[0]) / into.right[0], (at[1] - into.org[1]) / into.down[1]];
                    }
                }
                continue;
            }
            let Paint::Rui { path } = paint else {
                images.push((out.quads.len(), paint));
                continue;
            };
            let (Some(panel), Some(p)) = (self.panel(&path).cloned(), self.placed_at(&path).cloned()) else { continue };
            if self.rui(ui, &path).is_none() {
                continue;
            }
            if panel.class == "RuiButton" {
                let text = self.texts.get(&path).cloned().unwrap_or_else(|| label_text(ui, panel.settings.str("labelText").unwrap_or_default()));
                let Some(rui) = self.ruis.get_mut(&path) else { continue };
                paint_button_text(ui, rui, &panel.settings, &text);
            }
            let Some(rui) = self.ruis.get_mut(&path) else { continue };
            let topology = paint_rui(ui, rui, &p, &panel.settings, screen, target);
            let first = out.quads.len();
            match ui.update(rui, globals, &topology, frame) {
                Ok(true) => ui.draws(rui, frame, out),
                Ok(false) => {}
                Err(stop) => stops.push(format!("{}: {stop:?}", rui.name)),
            }
            for q in &mut out.quads[first..] {
                for c in &mut q.corners {
                    let at = topology.place(c[0], c[1]);
                    *c = [(at[0] - into.org[0]) / into.right[0], (at[1] - into.org[1]) / into.down[1]];
                }
            }
        }
        (images, stops)
    }
}

/// `SystemMenu` as `menu_system.nut` sets it up in a match (`UpdateSystemPanel`,
/// `SetButtonData`): `#SETTINGS` and `#LEAVE_MATCH` on the first two buttons, every other
/// button hidden for its empty label; the footer `SystemPanel`'s options leave in a match
/// (`InitSystemPanelMain`: Back, and the developer menu outside the lobby).
pub fn system_menu(ui: &Ui, panels: Vec<Panel>) -> Menu {
    let mut m = Menu::new(panels);
    m.set_footer(ui, &["#B_BUTTON_BACK", "#DEV_MENU"]);
    let labels = ["#SETTINGS", "#LEAVE_MATCH"];
    for i in 0.. {
        let Some(path) = m.find(&["SystemPanel", &format!("Button{i}")]) else { break };
        let label = labels.get(i).copied().unwrap_or_default();
        m.set_text(ui, &path, label);
        m.set_visible(&path, !label.is_empty());
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::keyvalues;

    fn panel(text: &str, children: Vec<Panel>) -> Panel {
        let settings = keyvalues::parse(text.as_bytes()).remove(0);
        Panel { name: settings.name.clone(), class: "Label".into(), settings, group: None, children }
    }

    #[test]
    fn panels_pin_and_scale_as_vpanel_solves_them() {
        let frame = panel("Frame { wide %100 tall %100 }", vec![]);
        let a = panel("A { wide 376 tall 60 ypos 16 pin_to_sibling Frame pin_corner_to_sibling CENTER pin_to_sibling_corner CENTER }", vec![]);
        let b = panel("B { wide 376 tall 60 xpos 4 ypos 8 pin_to_sibling A pin_corner_to_sibling TOP_RIGHT pin_to_sibling_corner TOP_LEFT }", vec![]);
        let c = panel("C { xpos r40 ypos c-10 wide f100 tall 20 zpos 3 visible 0 }", vec![]);
        let at = |screen: Screen| layout(&[b.clone(), frame.clone(), a.clone(), c.clone()], screen);
        let p = at(Screen { wide: 1920, tall: 1080 });
        assert_eq!((p[2].abs, p[2].size), ((772, 526), (376, 60)));
        assert_eq!(p[0].abs, (772 - 376 - 4, 526 - 8), "offsets point away from a sibling's left and top corners");
        assert_eq!((p[3].abs, p[3].size, p[3].zpos, p[3].visible), ((1880, 530), (1820, 20), 3, false));
        let p = at(Screen { wide: 2560, tall: 1440 });
        assert_eq!((p[2].abs, p[2].size), ((1029, 701), (501, 80)), "scaled by the height over 1080, floored");
        assert_eq!(Screen { wide: 2560, tall: 1440 }.scale(17), 22, "floored, not rounded");
        assert_eq!(Screen { wide: 3440, tall: 1440 }.scale(1080), 1440);
        assert_eq!(Screen { wide: 1920, tall: 1200 }.scale(1080), 1080, "a 16:9 inside the screen");
    }

    #[test]
    fn the_cursor_finds_the_topmost_panel_and_buttons_change_state() {
        let frame = panel("Frame { wide %100 tall %100 mouseEnabled 0 }", vec![panel("A { wide 100 tall 100 }", vec![]), panel("B { xpos 50 wide 100 tall 100 zpos 2 }", vec![]), panel("C { xpos 75 wide 100 tall 100 zpos 1 }", vec![])]);
        let panels = [frame];
        let placed = layout(&panels, Screen { wide: 1920, tall: 1080 });
        assert_eq!(panel_at(&panels, &placed, 80, 10), Some(vec![0, 1]), "the greater zpos is on top");
        assert_eq!(panel_at(&panels, &placed, 10, 10), Some(vec![0, 0]));
        assert_eq!(panel_at(&panels, &placed, 500, 500), None, "mouseEnabled 0 is not hit itself");
        assert_eq!(panel_at(&panels, &placed, 150, 10), Some(vec![0, 2]), "B's right edge is out, C is under it");
        let s = focused(NEW_BUTTON | 1 << 1, true);
        assert_eq!(s, 1 | 1 << 3);
        assert_eq!(unfocused(s, true), 1 | 1 << 1);
        assert_eq!(focused(NEW_BUTTON, false), 1 | 1 << 2);
    }

    #[test]
    fn panels_paint_by_zpos_then_file_order() {
        let mk = |name: &str, z: i32, class: &str| {
            let mut p = panel(&format!("{name} {{ wide 10 tall 10 zpos {z} image vgui/{name} drawColor \"1 2 3\" }}"), vec![]);
            p.class = class.into();
            p
        };
        let mut empty = mk("e", 0, "RuiPanel");
        empty.settings.set("wide", "0");
        let panels = [mk("a", 5, "ImagePanel"), mk("b", 1, "ImagePanel"), mk("c", 1, "RuiPanel"), mk("d", 0, "Label"), empty];
        let placed = layout(&panels, Screen { wide: 1920, tall: 1080 });
        let paints = paint_list(&panels, &placed, (0, 0, 1920, 1080));
        let image = |n: &str| Paint::Image { texture: format!("texture/vgui/{n}.rpak"), rect: (0, 0, 10, 10), color: [1, 2, 3, 255], clip: (0, 0, 1920, 1080) };
        assert_eq!(paints, [Paint::Text { path: vec![3] }, image("b"), Paint::Rui { path: vec![2] }, image("a")], "a Label paints its text; a panel with an empty clip rectangle paints nothing");
    }
}
