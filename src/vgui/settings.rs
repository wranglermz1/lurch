//! The settings menu (`MiscMenu`) as its scripts run it: `menu_misc.nut` opens and closes it,
//! `lobby_panel_settings.nut` fills `SettingsPanel`, and `_tabs.nut` keeps its tabs. A tab
//! remembers itself: the menu opens on the tab last shown.
//!
//! Each tab's body is a scroll panel (`_scroll_panel.gnut`): the wheel moves its `ContentPanel`
//! 60 pixels a notch, and its `ScrollBar`'s handle can be dragged.
//!
//! Each tab's rows are named, titled and described by its script (`SetupSettingsButton`,
//! `SetupSettingsSlider`), read from the install at run time (`rows`); the details panel shows
//! the row in focus.
//!
//! Not modelled: the gamepad's shoulder hints (shown only with a gamepad active), tab dividers
//! and custom tab colours (the settings tabs use neither), a playlist forcing a tab's access,
//! the sounds, the tabs' own show handlers.

use std::collections::HashMap;
use std::path::Path;

use super::keybinds::{pairs, BindList, KeyBindings, DELETE};
use super::{Menu, Panel, Placed, Screen};
use crate::assets::vgui::Scheme;
use crate::rui::Ui;

/// Each tab's script, by `TABS` index (`_menus.nut` adds the panels with these inits).
pub const SCRIPTS: [&str; 5] = ["panel_hud_options.nut", "panel_controls_pc.nut", "panel_controls_gamepad.nut", "panel_video.nut", "panel_sound.nut"];

/// A row a tab's script sets up: the panel's name, its title and description, whether the
/// details panel shows the colour-blind info (`showAdditional`), and whether it is a slider
/// (whose child `BtnDropButton` takes the title).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub name: String,
    pub title: String,
    pub desc: String,
    pub additional: bool,
    pub slider: bool,
}

/// The conditions a UI script is compiled with on PC: `PC_PROG` and `UI` hold, every other
/// (`CONSOLE_PROG`, `DURANGO_PROG`, `DEVELOPER`, ...) does not.
fn defined(name: &str) -> bool {
    matches!(name, "PC_PROG" | "UI")
}

fn condition(expr: &str) -> bool {
    let expr = expr.trim().trim_start_matches('(').trim_end_matches(')').trim();
    expr.split("||").any(|any| any.split("&&").all(|t| match t.trim().strip_prefix('!') {
        Some(n) => !defined(n.trim()),
        None => defined(t.trim()),
    }))
}

/// A script's text as compiled on PC: comments out, and the lines of a false `#if` branch.
fn compiled(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        rest = rest[i..].find("*/").map_or("", |j| &rest[i + j + 2..]);
    }
    out.push_str(rest);
    let mut kept = String::new();
    let mut stack: Vec<(bool, bool)> = Vec::new();
    for line in out.lines() {
        let line = line.split("//").next().unwrap_or_default();
        let t = line.trim();
        let live = stack.last().is_none_or(|s| s.0);
        let parent = stack.len() < 2 || stack[stack.len() - 2].0;
        if let Some(e) = t.strip_prefix("#if") {
            let on = live && condition(e);
            stack.push((on, on));
        } else if let Some(e) = t.strip_prefix("#elseif") {
            if let Some(s) = stack.last_mut() {
                let on = parent && !s.1 && condition(e);
                *s = (on, s.1 || on);
            }
        } else if t.starts_with("#else") {
            if let Some(s) = stack.last_mut() {
                *s = (parent && !s.1, true);
            }
        } else if t.starts_with("#endif") {
            stack.pop();
        } else if live {
            kept.push_str(line);
            kept.push('\n');
        }
    }
    kept
}

/// The arguments of the call whose `(` is at `open`, split at top-level commas, and the index
/// past its `)`.
fn arguments(text: &str, open: usize) -> (Vec<&str>, usize) {
    let (mut depth, mut quoted, mut start, mut args) = (0, false, open + 1, Vec::new());
    for (i, c) in text[open..].char_indices().map(|(i, c)| (i + open, c)) {
        match c {
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => {
                depth -= 1;
                if depth == 0 {
                    args.push(text[start..i].trim());
                    return (args, i + 1);
                }
            }
            ',' if !quoted && depth == 1 => {
                args.push(text[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    (args, text.len())
}

fn quoted(arg: &str) -> Option<String> {
    let s = arg.trim_start_matches('$');
    Some(s.strip_prefix('"')?.strip_suffix('"')?.to_string())
}

/// The rows a tab's script (`scripts/vscripts/ui/<file>`) sets up, in order: each
/// `SetupSettingsButton` or `SetupSettingsSlider` call as compiled on PC, its panel named in the
/// call (`Hud_GetChild( _, "Name" )`) or by the last `Hud_GetChild` assigned to the variable it
/// passes.
pub fn rows(install: &Path, file: &str) -> Vec<Row> {
    let path = install.join("platform/scripts/vscripts/ui").join(file);
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let text = compiled(&text);
    let mut vars: HashMap<String, String> = HashMap::new();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = text[at..].find(['=', 'S']).map(|i| i + at) {
        at = i + 1;
        let tail = &text[i..];
        let call = ["SetupSettingsButton", "SetupSettingsSlider"].into_iter().find(|c| tail.starts_with(c) && !text[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_'));
        if let Some(call) = call {
            let Some(open) = tail.find('(').map(|o| o + i) else { break };
            let (args, end) = arguments(&text, open);
            at = end;
            let name = if args[0].starts_with("Hud_GetChild") { args[0].rsplit('"').nth(1).map(String::from) } else { vars.get(args[0]).cloned() };
            if let (Some(name), Some(title), Some(desc)) = (name, args.get(1).and_then(|a| quoted(a)), args.get(2).and_then(|a| quoted(a))) {
                out.push(Row { name, title, desc, additional: args.get(4).is_some_and(|a| *a == "true"), slider: call == "SetupSettingsSlider" });
            }
        } else if let Some(rhs) = tail.strip_prefix('=').map(str::trim_start).filter(|r| r.starts_with("Hud_GetChild") && !tail.starts_with("==")) {
            let lhs = text[..i].trim_end().rsplit(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.')).next().unwrap_or_default();
            let open = i + tail.len() - rhs.len() + rhs.find('(').unwrap_or(0);
            let (args, _) = arguments(&text, open);
            if let Some(name) = args.get(1).and_then(|a| quoted(a)) {
                vars.insert(lhs.to_string(), name);
            }
        }
    }
    out
}

/// The tabs `InitSettingsPanel` adds on PC, in order: the tab's body, its title and the
/// details panel's image while nothing is focused.
pub const TABS: [(&str, &str, &str); 5] = [
    ("HudOptionsPanel", "#HUD", "rui/menu/settings/settings_hud"),
    ("ControlsPCPanelContainer", "#MOUSE_KEYBOARD", "rui/menu/settings/settings_pc"),
    ("ControlsGamepadPanel", "#CONTROLLER", "rui/menu/settings/settings_gamepad"),
    ("VideoPanelContainer", "#VIDEO", "rui/menu/settings/settings_video"),
    ("SoundPanel", "#AUDIO", "rui/menu/settings/settings_audio"),
];

/// `SCROLL_SPEED`, and the pixels a wheel notch scrolls (`MouseWheelUp`, `MouseWheelDown`).
const SCROLL_SPEED: f32 = 10.0;
const WHEEL: f32 = 60.0;

/// A tab's `ScrollPanelData`: `listOffset`, and the drag of the scroll bar's handle
/// (`scrollStartPos.y` and `scrollListOffset`) while the button is held.
#[derive(Clone, Copy, Debug, Default)]
struct Scroll {
    offset: f32,
    drag: Option<(i32, f32)>,
}

/// `GetCursorPosition` (`0x7a7d20`) with the mouse: the game cursor, which a mouse move puts at
/// the mouse in 1920 × 1080 units (`0x6f0130` through `0x201ce0`).
fn cursor_position(screen: Screen, (x, y): (i32, i32)) -> (f32, f32) {
    let unit = |base: f32, size: i32, v: i32| ((base / size as f32) * v as f32).round_ties_even();
    (unit(1920.0, screen.wide, x), unit(1080.0, screen.tall, y))
}

/// `MAX_TABS`: the tab buttons `tabs_settings.res` has.
const MAX_TABS: usize = 8;

pub struct SettingsMenu {
    pub menu: Menu,
    /// `TabData.activeTabIdx`.
    pub active: usize,
    /// The rows by tab, and each button the details panel describes: the row's tab and index,
    /// and whether it is a slider's drop button (`DropButton_GetFocus`).
    rows: Vec<Vec<Row>>,
    details: HashMap<Vec<usize>, (usize, usize, bool)>,
    scrolls: [Scroll; TABS.len()],
    /// The Mouse/Keyboard tab was shown (`OnControlsPCPanel_Show`: `KeyBindings_FillInCurrent`)
    /// or hidden (`OnControlsPCPanel_Hide`: `SavePlayerSettings`) since the shell last looked.
    pub fill_binds: bool,
    pub save: bool,
}

/// The key bind list's actions, and the script that pairs some (`InitControlsPCPanelForCode`).
const BIND_LIST: &str = "scripts/kb_act.lst";
const BINDS_TAB: usize = 1;

/// The menu's layout.
pub const MENU: &str = "scripts/resource/ui/menus/misc.menu";

/// The tab bodies the engine makes (`InitControlsPCPanelForCode`, `InitVideoPanelForCode`):
/// the tab, the class and its file.
const CODE_PANELS: [(&str, &str, &str); 2] = [
    ("ControlsPCPanelContainer", "KeyboardBindingsPanel", "scripts/resource/ui/menus/panels/controls_pc.res"),
    ("VideoPanelContainer", "VideoOptionsPanel", "scripts/resource/ui/menus/panels/video.res"),
];

impl SettingsMenu {
    /// The menu from the install: `MENU`, each tab's rows from its script, and the tab bodies
    /// the engine makes (`CreateKeyBindingPanel` `0x786340`, `CreateVideoOptionsPanel`
    /// `0x7864a0`): a panel named `ContentPanel` in the tab, its children from its file. Its
    /// constructor (`0x99f3a0`, `0x9a9d30`) sizes it `wide f0`, `tall f0`; the script then puts it
    /// at 0, 0 at its `PanelFrame`'s size, which is its own width (`%100` of it) and the
    /// frame's `tall`, so those are its settings here. The script hides it but while its tab
    /// shows, which its tab's own visibility already does. The bindings panel's constructor makes
    /// `ruipanel_keybindlist` a RuiPanel before the file names it a RuiButton. Not modelled: what
    /// else the constructors make and fill (the key bind list's rows, the video options' lists).
    pub fn load(ui: &Ui, install: &Path, scheme: &Scheme) -> std::io::Result<SettingsMenu> {
        let platform = install.join("platform");
        let mut panels = crate::assets::vgui::load(&platform, scheme, MENU)?;
        for (tab, class, file) in CODE_PANELS {
            let mut children = crate::assets::vgui::load(&platform, scheme, file)?;
            let tall = children.iter().find(|c| c.name == "PanelFrame").and_then(|c| c.settings.str("tall")).unwrap_or("0").to_string();
            for c in children.iter_mut().filter(|c| c.name == "ruipanel_keybindlist") {
                c.class = "RuiPanel".into();
            }
            let mut settings = crate::assets::keyvalues::parse(b"ContentPanel { }").remove(0);
            for (k, v) in [("xpos", "0"), ("ypos", "0"), ("wide", "f0"), ("tall", tall.as_str())] {
                settings.set(k, v);
            }
            let Some(body) = panels.first_mut().and_then(|m| m.children.iter_mut().find(|p| p.name == "SettingsPanel")).and_then(|p| p.children.iter_mut().find(|p| p.name == tab)) else { continue };
            body.children.push(Panel { name: "ContentPanel".into(), class: class.into(), settings, group: None, children });
        }
        let rows = SCRIPTS.iter().map(|f| rows(install, f)).collect();
        let mut s = SettingsMenu::new(ui, panels, rows, scheme);
        let list = s.menu.find(&["SettingsPanel", TABS[BINDS_TAB].0]).and_then(|b| s.menu.descendant(&b, "listpanel_keybindlist"));
        if let Some(path) = list {
            let text = std::fs::read_to_string(platform.join(BIND_LIST))?;
            let settings = s.menu.panel(&path).map(|p| p.settings.clone()).unwrap_or_else(|| crate::assets::keyvalues::parse(b"x { }").remove(0));
            let mut list = BindList::new(ui, path, &text, &settings, scheme);
            let script = std::fs::read_to_string(platform.join("scripts/vscripts/ui").join(SCRIPTS[BINDS_TAB])).unwrap_or_default();
            list.pairs = pairs(&compiled(&script));
            s.menu.bind_list = Some(list);
        }
        Ok(s)
    }

    /// The menu as `InitTabs`, `InitSettingsPanel` and the tabs' inits leave it: every tab
    /// button hidden, then one shown for each tab (`AddTab`), the first active; each row titled
    /// (`SetButtonRuiText` on the row, or on a slider's `BtnDropButton`), and it and its
    /// `LeftButton` and `RightButton` described in the details panel when focused.
    pub fn new(ui: &Ui, panels: Vec<Panel>, rows: Vec<Vec<Row>>, scheme: &Scheme) -> SettingsMenu {
        let mut s = SettingsMenu { menu: Menu::new(panels), active: 0, rows, details: HashMap::new(), scrolls: Default::default(), fill_binds: false, save: false };
        s.menu.apply_scheme(ui, scheme);
        for i in 0..MAX_TABS {
            if let Some(tab) = s.tab_button(i) {
                s.menu.set_visible(&tab, i < TABS.len());
            }
        }
        for (tab, rows) in s.rows.iter().enumerate() {
            let Some(body) = s.menu.find(&["SettingsPanel", TABS[tab].0]) else { continue };
            for (r, row) in rows.iter().enumerate() {
                let Some(mut path) = s.menu.descendant(&body, &row.name) else { continue };
                if row.slider {
                    let Some(drop) = s.menu.child(&path, "BtnDropButton") else { continue };
                    path = drop;
                }
                s.menu.set_text(ui, &path, &row.title);
                s.details.insert(path.clone(), (tab, r, row.slider));
                if !row.slider {
                    for name in ["RightButton", "LeftButton"] {
                        if let Some(c) = s.menu.child(&path, name) {
                            s.details.insert(c, (tab, r, false));
                        }
                    }
                }
            }
        }
        s
    }

    /// The cursor at `x`, `y`: the focus moves as `Menu::hover` moves it, and the details panel
    /// follows (`SettingsButton_LoseFocus`, then `SettingsButton_GetFocus` or
    /// `DropButton_GetFocus`, which a slider's drop button also runs on losing the focus).
    pub fn hover(&mut self, ui: &Ui, x: i32, y: i32) {
        let old = self.menu.hovered.clone();
        self.menu.hover(ui, x, y);
        if old == self.menu.hovered {
            return;
        }
        if let Some(&(tab, r, slider)) = old.as_ref().and_then(|p| self.details.get(p)) {
            if slider {
                self.describe(ui, Some((tab, r)));
            } else {
                self.describe(ui, None);
            }
        }
        if let Some(&(tab, r, _)) = self.menu.hovered.as_ref().and_then(|p| self.details.get(p)) {
            self.describe(ui, Some((tab, r)));
        }
    }

    /// `DisplaySettingInfoForButton` for a row, or the details cleared to the active tab's image.
    fn describe(&mut self, ui: &Ui, row: Option<(usize, usize)>) {
        let Some(p) = self.menu.find(&["SettingsPanel", "DetailsPanel"]) else { return };
        let (title, desc, image, additional) = match row {
            Some((tab, r)) => {
                let row = &self.rows[tab][r];
                (row.title.clone(), row.desc.clone(), TABS[tab].2, row.additional)
            }
            None => (String::new(), String::new(), TABS[self.active].2, false),
        };
        if let Some(rui) = self.menu.rui(ui, &p) {
            rui.set_string(ui, "selectionText", &title);
            rui.set_string(ui, "descText", &desc);
            rui.set_asset(ui, "detailImage", image);
            rui.set_bool(ui, "showCbInfo", additional);
        }
    }

    fn tab_button(&self, i: usize) -> Option<Vec<usize>> {
        self.menu.find(&["SettingsPanel", "TabsCommon", &format!("Tab{i}")])
    }

    fn body(&self, i: usize) -> Option<Vec<usize>> {
        self.menu.find(&["SettingsPanel", TABS[i].0])
    }

    /// `OnMiscMenu_Open`: `SettingsPanel` shown, whose show handler (`OnSettingsPanel_Show`)
    /// activates the active tab again.
    pub fn open(&mut self, ui: &Ui) {
        if let Some(p) = self.menu.find(&["SettingsPanel"]) {
            self.menu.set_visible(&p, true);
        }
        self.activate(ui, self.active);
    }

    /// `OnMiscMenu_Close`: `SettingsPanel` hidden, and with it the active tab's body
    /// (`OnSettingsPanel_Hide`, `DeactivateTab`).
    pub fn close(&mut self) {
        for p in [self.menu.find(&["SettingsPanel"]), self.body(self.active)].into_iter().flatten() {
            self.menu.set_visible(&p, false);
        }
        self.save |= self.active == BINDS_TAB;
    }

    /// `ActivateTab`: the tabs updated, every body hidden, then the tab's shown, which runs its
    /// show handler (`OnSettingsTab_Show`).
    pub fn activate(&mut self, ui: &Ui, i: usize) {
        self.save |= self.active == BINDS_TAB && i != BINDS_TAB;
        self.fill_binds |= i == BINDS_TAB;
        self.active = i;
        self.update_tabs(ui);
        for b in 0..TABS.len() {
            if let Some(p) = self.body(b) {
                self.menu.set_visible(&p, false);
            }
        }
        if let Some(p) = self.body(i) {
            self.menu.set_visible(&p, true);
        }
        self.tab_shown(ui);
        self.scroll(ui, 0.0);
        // The tab's body is the top active panel; with the mouse its footer shows Back and Restore
        // Defaults (the Audio and Video tabs' Apply only once a setting changed, not modelled).
        self.menu.set_footer(ui, &["#B_BUTTON_BACK", "#RESTORE_DEFAULTS"]);
    }

    /// `UpdateMenuTabs` without a gamepad: a tab button past the last tab loses its text,
    /// width and `ypos`; each other is selected when active and shows its title. Both
    /// shoulder hints hide (their text, emptied by the native `SetLabelRuiText`, is not).
    fn update_tabs(&mut self, ui: &Ui) {
        for i in 0..MAX_TABS {
            let Some(tab) = self.tab_button(i) else { continue };
            let Some(&(_, title, _)) = TABS.get(i) else {
                if let Some(rui) = self.menu.rui(ui, &tab) {
                    rui.set_string(ui, "buttonText", "");
                }
                self.menu.set(&tab, "wide", "0");
                self.menu.set(&tab, "ypos", "0");
                continue;
            };
            self.menu.set_selected(ui, &tab, i == self.active);
            if let Some(rui) = self.menu.rui(ui, &tab) {
                rui.set_string(ui, "buttonText", title);
                rui.set_bool(ui, "useCustomColors", false);
                rui.set_bool(ui, "isInactive", false);
            }
        }
        for name in ["LeftNavButton", "RightNavButton"] {
            if let Some(p) = self.menu.find(&["SettingsPanel", "TabsCommon", name]) {
                self.menu.set_visible(&p, false);
            }
        }
    }

    /// `OnSettingsTab_Show`: the details panel cleared, with the tab's own image.
    fn tab_shown(&mut self, ui: &Ui) {
        let Some(p) = self.menu.find(&["SettingsPanel", "DetailsPanel"]) else { return };
        let image = TABS[self.active].2;
        if let Some(rui) = self.menu.rui(ui, &p) {
            for arg in ["headerText", "selectionText", "descText"] {
                rui.set_string(ui, arg, "");
            }
            rui.set_asset(ui, "detailImage", image);
        }
    }

    /// The content's height as `ScrollPanel_InitPanel` sets it: its `PanelBottom`'s `y` within
    /// it when above 0, else its own.
    fn content_height(&self, content: &[usize]) -> Option<i32> {
        let placed = self.menu.placed_at(content)?;
        Some(placed.child("PanelBottom").map(|b| b.abs.1 - placed.abs.1).filter(|&y| y > 0).unwrap_or(placed.size.1))
    }

    /// The active tab's scroll panel moved by `value` (`UpdatePanelElements`): `listOffset`
    /// clamped to the content's height less the panel's, not below 0; the content placed at
    /// minus that, and the scroll bar's handle set (`UpdateScrollBar`), hidden when the content
    /// fits.
    pub fn scroll(&mut self, ui: &Ui, value: f32) {
        let Some(body) = self.body(self.active) else { return };
        let (Some(content), Some(bar)) = (self.menu.child(&body, "ContentPanel"), self.menu.child(&body, "ScrollBar")) else { return };
        let (Some(panel), Some(height)) = (self.menu.placed_at(&body).map(|p| p.size.1), self.content_height(&content)) else { return };
        let s = &mut self.scrolls[self.active];
        let (min, max) = (0.0, (height - panel) as f32);
        let v = s.offset + value * SCROLL_SPEED;
        s.offset = if v < min { min } else if v > max { max } else { v };
        s.offset = s.offset.max(0.0);
        let offset = s.offset;
        self.menu.set_y(&content, -offset as i32);
        self.menu.set_visible(&bar, panel < height);
        if let Some(rui) = self.menu.rui(ui, &bar) {
            rui.set_float(ui, "handleOffset", offset / height as f32);
            rui.set_float(ui, "handleFrac", panel as f32 / height as f32);
        }
    }

    /// A wheel notch, up or down, over the active tab (`MouseWheelUp`, `MouseWheelDown`), which
    /// the scripts take whatever is under the cursor; the key bind list under the cursor scrolls
    /// too.
    pub fn wheel(&mut self, ui: &Ui, screen: Screen, up: bool) {
        self.scroll(ui, if up { -(WHEEL / SCROLL_SPEED) } else { WHEEL / SCROLL_SPEED });
        let Some((list, placed)) = self.bind_list() else { return };
        if list.cursor.is_some_and(|c| super::within(&placed, c)) {
            list.wheel(&placed, screen, up);
        }
    }

    /// The key bind list, while its tab shows, and where it is.
    fn bind_list(&mut self) -> Option<(&mut BindList, Placed)> {
        if self.active != BINDS_TAB {
            return None;
        }
        let path = self.menu.bind_list.as_ref()?.path.clone();
        let placed = self.menu.placed_at(&path)?.clone();
        Some((self.menu.bind_list.as_mut()?, placed))
    }

    /// Whether the key bind list is capturing a key, which then takes the next key or button.
    pub fn capturing(&self) -> bool {
        self.active == BINDS_TAB && self.menu.bind_list.as_ref().is_some_and(|l| l.capture.is_some())
    }

    /// The cursor over the key bind list.
    pub fn list_hover(&mut self, screen: Screen, cursor: (i32, i32)) {
        if let Some((list, placed)) = self.bind_list() {
            list.hover(&placed, screen, cursor);
        }
        self.take_sounds();
    }

    /// A mouse button (by key code) going down over the key bind list, or taken by its capture.
    pub fn list_press(&mut self, ui: &Ui, screen: Screen, cursor: (i32, i32), code: u16, keys: &mut KeyBindings) {
        if let Some((list, placed)) = self.bind_list() {
            list.press(&placed, screen, cursor, code, keys);
        }
        self.overlay(ui);
    }

    /// A key going down: the capture's key while capturing, else Delete clears the item of
    /// interest (`0x9a2560`).
    pub fn list_key(&mut self, ui: &Ui, code: u16, keys: &mut KeyBindings) {
        if let Some((list, _)) = self.bind_list() {
            if list.capture.is_some() {
                list.finish(code, keys);
            } else if code == DELETE {
                list.delete(keys);
            }
        }
        self.overlay(ui);
    }

    /// `ruipanel_keybindlist` while capturing (`0x99f9c0`, `0x9a22c0`): shown, its `isVisible`
    /// set and its `keyName` the item's action; hidden again after.
    fn overlay(&mut self, ui: &Ui) {
        let shown = self.menu.bind_list.as_ref().filter(|l| l.capture.is_some()).and_then(|l| Some(l.items[l.interest?].action.clone()));
        if let Some(path) = self.menu.find(&["SettingsPanel", TABS[BINDS_TAB].0]).and_then(|b| self.menu.descendant(&b, "ruipanel_keybindlist")) {
            self.menu.set_visible(&path, shown.is_some());
            if let Some(rui) = self.menu.rui(ui, &path) {
                rui.set_bool(ui, "isVisible", shown.is_some());
                if let Some(name) = &shown {
                    rui.set_string(ui, "keyName", name);
                }
            }
        }
        self.take_sounds();
    }

    fn take_sounds(&mut self) {
        if let Some(l) = self.menu.bind_list.as_mut() {
            self.menu.sounds.append(&mut l.sounds);
        }
    }

    /// The left button pressed with the cursor on the scroll bar starts a drag
    /// (`ScrollHandlePress`, `TEMP_CursorInElementBounds`: edges included); while it is held the
    /// content moves by the cursor's travel over the bar's height times the content's
    /// (`ScrollPanelScrollUpdate`); released, it stops. The travel is in `GetCursorPosition`'s
    /// units and the bar's height in pixels, so off 1080 lines the content does not keep pace
    /// with the cursor, as in the game.
    pub fn drag(&mut self, ui: &Ui, screen: Screen, cursor: (i32, i32), pressed: bool, held: bool) {
        let Some(body) = self.body(self.active) else { return };
        let (Some(content), Some(bar)) = (self.menu.child(&body, "ContentPanel"), self.menu.child(&body, "ScrollBar")) else { return };
        if !held {
            self.scrolls[self.active].drag = None;
            return;
        }
        let units = cursor_position(screen, cursor);
        let Some(p) = self.menu.placed_at(&bar).cloned() else { return };
        if pressed && self.scrolls[self.active].drag.is_none() {
            let (x, y) = (units.0 * (screen.wide as f32 / 1920.0), units.1 * (screen.tall as f32 / 1080.0));
            let (left, top, right, bottom) = (p.abs.0 as f32, p.abs.1 as f32, (p.abs.0 + p.size.0) as f32, (p.abs.1 + p.size.1) as f32);
            if !(x < left || y < top || x > right || y > bottom) {
                self.scrolls[self.active].drag = Some((units.1 as i32, self.scrolls[self.active].offset));
            }
        }
        let Some((start, from)) = self.scrolls[self.active].drag else { return };
        let Some(height) = self.content_height(&content) else { return };
        self.scrolls[self.active].offset = from;
        let frac = (units.1 as i32 - start) as f32 / p.size.1 as f32;
        self.scroll(ui, (frac * height as f32) / SCROLL_SPEED);
    }

    /// A click (`UIE_CLICK`) on the button under the cursor: on another tab, `OnTab_Activate`.
    pub fn click(&mut self, ui: &Ui) {
        let Some(hovered) = self.menu.hovered.clone() else { return };
        if let Some(i) = (0..TABS.len()).find(|&i| self.tab_button(i).as_ref() == Some(&hovered))
            && i != self.active
        {
            self.activate(ui, i);
        }
    }
}
