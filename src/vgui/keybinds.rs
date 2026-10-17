//! The Mouse/Keyboard tab's key bind list, `listpanel_keybindlist`: KeyboardBindingsPanel's
//! constructor (`0x99f3a0`) makes it a VControlsListPanel (vtable `0x147c758`, over
//! SectionedListPanel `0x138c548`) and fills it from `scripts/kb_act.lst` (`0x9a02c0`). Each
//! item has an action, the command it binds and two key columns; a click on a key column
//! captures the next key pressed into it.
//!
//! The binds it reads and changes are the engine's records by key code (`KeyBindings`, at
//! `0x20676ee0`, 0x28 bytes each: the binding at `+0`, the column it was bound into at `+0x25`),
//! changed as the panel's `cmd1 bind`, `bind_held`, `unbind` and `unbind_held` commands change
//! them, and written as `settings.cfg` writes them.
//!
//! Not modelled: the gamepad's codes (`0x73` to `0x134`, which a capture ignores and the list
//! never shows), the list's ScrollBar's look (VGUI's own), the scripts' bind messages
//! (`UICodeCallback_KeyBindSet`, `UICodeCallback_KeyBindOverwritten`, `KeyBindMessage`), the
//! arrow keys, restoring the defaults (`0x9a18b0`), escaping `\` and `"` in a written bind, and
//! the text's `clipRect` (lurch's RUI does not clip `drawtext_vgui`'s glyphs, so a row cut by
//! the list's edge draws its whole text).
//!
//! Not traced: the header's height (its font's tall, `0x543280`, is in the font manager's
//! records on the heap; here the header Label's own text height plus 7), and the face and
//! height of the items' text (here the Label face and the list's `fontHeight`).

use super::label::{face, Label};
use super::{atoi, color, label_text, Paint, Placed, Screen};
use crate::assets::vgui::Scheme;
use crate::rui::Ui;

/// ButtonCodeToString (input system `+0xc8`, `0x39f250`) for codes 1 to `0x72`, the keyboard's
/// and the mouse's (`0x1309708`).
pub const KEY_NAMES: [&str; 0x72] = [
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9",
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z",
    "KP_INS", "KP_END", "KP_DOWNARROW", "KP_PGDN", "KP_LEFTARROW", "KP_5", "KP_RIGHTARROW", "KP_HOME", "KP_UPARROW", "KP_PGUP",
    "KP_SLASH", "KP_MULTIPLY", "KP_MINUS", "KP_PLUS", "KP_ENTER", "KP_DEL",
    "[[", "]", "SEMICOLON", "'", "`", ",", ".", "/", "\\", "-", "=",
    "ENTER", "SPACE", "BACKSPACE", "TAB", "CAPSLOCK", "NUMLOCK", "ESCAPE", "SCROLLLOCK", "INS", "DEL", "HOME", "END", "PGUP", "PGDN", "PAUSE",
    "LSHIFT", "RSHIFT", "LALT", "RALT", "LCTRL", "RCTRL", "LWIN", "RWIN", "APP",
    "UPARROW", "LEFTARROW", "DOWNARROW", "RIGHTARROW",
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
    "CAPSLOCKTOGGLE", "NUMLOCKTOGGLE", "SCROLLLOCKTOGGLE", "CREDITSIGN",
    "MOUSE1", "MOUSE2", "MOUSE3", "MOUSE4", "MOUSE5", "MWHEELUP", "MWHEELDOWN",
];
pub const ENTER: u16 = 0x40;
pub const ESCAPE: u16 = 0x46;
pub const DELETE: u16 = 0x49;
pub const MOUSE_LEFT: u16 = 0x6c;
pub const MOUSE_RIGHT: u16 = 0x6d;
/// The gamepad's codes, which a capture waits past (`0x9a22c0`) and the fill skips (`0x9a1370`).
const GAMEPAD: std::ops::RangeInclusive<i32> = 0x73..=0x134;

pub fn key_name(code: u16) -> Option<&'static str> {
    KEY_NAMES.get(usize::from(code).checked_sub(1)?).copied()
}

/// StringToButtonCode (input system `+0xd0`, `0x39f2f0`): the name compared with ASCII case
/// folded.
pub fn key_code(name: &str) -> Option<u16> {
    KEY_NAMES.iter().position(|n| n.eq_ignore_ascii_case(name)).map(|i| i as u16 + 1)
}

/// `0x466d30`: compared with ASCII letters folded.
fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// A line's words: a quoted run, or a run up to white space.
fn words(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = line.trim_start();
    while !rest.is_empty() {
        if let Some(q) = rest.strip_prefix('"') {
            let end = q.find('"').unwrap_or(q.len());
            out.push(&q[..end]);
            rest = q.get(end + 1..).unwrap_or("");
        } else {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            out.push(&rest[..end]);
            rest = &rest[end..];
        }
        rest = rest.trim_start();
    }
    out
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Record {
    binding: String,
    held: String,
    column: u8,
}

/// The keyboard's and mouse's binds by code, and the bind lines of any other key as read.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyBindings {
    records: Vec<Record>,
    other: Vec<String>,
}

impl Default for KeyBindings {
    fn default() -> KeyBindings {
        KeyBindings { records: vec![Record::default(); KEY_NAMES.len() + 1], other: Vec::new() }
    }
}

const BIND: &str = "bind_US_standard";
const BIND_HELD: &str = "bind_held_US_standard";

impl KeyBindings {
    /// `settings.cfg`'s bind lines run: `bind_US_standard "KEY" "COMMAND" N` and
    /// `bind_held_US_standard` for the held binding, N the column.
    pub fn from_lines<S: AsRef<str>>(lines: &[S]) -> KeyBindings {
        let mut k = KeyBindings::default();
        for line in lines {
            let line = line.as_ref();
            match words(line).as_slice() {
                [cmd, key, binding, rest @ ..] if same(cmd, BIND) || same(cmd, BIND_HELD) => {
                    let Some(code) = key_code(key) else {
                        k.other.push(line.to_string());
                        continue;
                    };
                    let r = &mut k.records[code as usize];
                    if same(cmd, BIND) {
                        r.binding = binding.to_string();
                        r.column = rest.first().map_or(0, |n| atoi(n) as u8);
                    } else {
                        r.held = binding.to_string();
                    }
                }
                _ => {}
            }
        }
        k
    }

    /// The bind lines as `0x241030` writes them (`%s "%s" "%s" %i`): each bound key by code, its
    /// held binding after it, then the other keys' lines.
    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (code, r) in self.records.iter().enumerate().skip(1) {
            let name = KEY_NAMES[code - 1];
            if !r.binding.is_empty() {
                out.push(format!("{BIND} \"{name}\" \"{}\" {}", r.binding, r.column));
            }
            if !r.held.is_empty() {
                out.push(format!("{BIND_HELD} \"{name}\" \"{}\" {}", r.held, r.column));
            }
        }
        out.extend(self.other.iter().cloned());
        out
    }

    /// The key's binding (the binds' `+8`, `0x204340`), empty when it has none.
    pub fn binding(&self, code: u16) -> &str {
        self.records.get(code as usize).map_or("", |r| r.binding.as_str())
    }

    pub fn column(&self, code: u16) -> u8 {
        self.records.get(code as usize).map_or(0, |r| r.column)
    }

    fn record(&mut self, code: u16) -> Option<&mut Record> {
        self.records.get_mut(code as usize).filter(|_| code > 0)
    }
}

/// The list's columns (AddColumnToSection `0x4e9c60`): the item's key, its header, its width
/// before scaling and its flags.
pub const COLUMNS: [(&str, &str, i32, u32); 3] = [("Action", "Action", 286, 0x44), ("Key1", "#GameUI_Key1", 128, 0x2c), ("Key2", "#GameUI_Key2", 128, 0x2c)];
const BRIGHT: u32 = 4;
const CENTRED: u32 = 8;
const NAMED: u32 = 0x20;
const NO_BACKGROUND: u32 = 0x40;

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub section: usize,
    pub action: String,
    pub binding: String,
    pub keys: [String; 2],
    /// `LastBoundColumn`.
    pub last_bound: Option<usize>,
}

/// What the list draws with, from its settings (ApplySettings `0x4e9310`) and the scheme (the
/// header's `0x4e62b0`, the items' `0x4e7270`).
#[derive(Clone, Debug, PartialEq)]
struct Look {
    line_spacing: i32,
    line_padding: i32,
    font_height: i32,
    header_height: i32,
    single_click_edit: bool,
    face: u16,
    header: [u8; 4],
    divider: [u8; 4],
    text: [u8; 4],
    bright: [u8; 4],
    selected: [u8; 4],
    mouse_over: [u8; 4],
    column: [u8; 4],
}

/// A row of the layout (LayoutPanels `0x4e8b50`), in the list's own pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row {
    Header { section: usize, y: i32, h: i32 },
    Item { item: usize, y: i32, h: i32 },
}

pub struct BindList {
    /// The list's path in the menu.
    pub path: Vec<usize>,
    pub sections: Vec<String>,
    pub items: Vec<Item>,
    /// `KeyBindings_AddTappedHeldPair`: a key bound to the first also gets the second as its held
    /// binding.
    pub pairs: Vec<(String, String)>,
    /// GetItemOfInterest (`+0x3ac`), and while capturing (`+0x3a8`) the column the key goes in
    /// (`+0x3f8`).
    pub interest: Option<usize>,
    pub capture: Option<usize>,
    pub selected: Option<usize>,
    /// The item and column the cursor was last over (each item's `+0x38c`), for the focus sound.
    focus: Option<(usize, usize)>,
    /// The ScrollBar's value.
    pub scroll: i32,
    /// The cursor, in screen pixels.
    pub cursor: Option<(i32, i32)>,
    pub sounds: Vec<String>,
    look: Look,
}

/// The sounds the panel plays (`0x9a22c0`, `0x4e7ac0`, `0x4e74a0`).
const BACK: &str = "UI_Menu_Back";
const ACCEPT: &str = "UI_Menu_Accept";
const FOCUS: &str = "UI_Menu_Focus";

impl BindList {
    /// `0x9a02c0` on the list file's text: pairs of strings, a binding then a description. A
    /// `blank` binding starts a section named by its description unless that starts with `=`;
    /// any other adds an item with both key columns empty.
    pub fn new(ui: &Ui, path: Vec<usize>, text: &str, settings: &crate::assets::keyvalues::Key, scheme: &Scheme) -> BindList {
        let (sections, items) = parse(text);
        let int = |k: &str| settings.str(k).map(atoi);
        let scheme_color = |name: &str| scheme.base.str(name).and_then(|v| color(v).or_else(|| scheme.colors.str(v).and_then(color))).unwrap_or([0; 4]);
        let label_height = scheme.base.str("Label.RuiFontHeight").map_or(0, atoi);
        let look = Look {
            line_spacing: int("linespacing").filter(|&v| v != 0).unwrap_or(20),
            line_padding: int("linePadding").unwrap_or(0),
            font_height: int("fontHeight").unwrap_or(label_height),
            header_height: label_height,
            single_click_edit: int("single_click_edit").is_some_and(|v| v != 0),
            face: face(ui, scheme.base.str("Label.RuiFont").unwrap_or_default()),
            header: scheme_color("SectionedListPanel.HeaderTextColor"),
            divider: scheme_color("SectionedListPanel.DividerColor"),
            text: scheme_color("SectionedListPanel.TextColor"),
            bright: scheme_color("SectionedListPanel.BrightTextColor"),
            selected: scheme_color("SectionedListPanel.SelectedTextColor"),
            mouse_over: scheme_color("SectionedListPanel.MouseOverBgColor"),
            column: scheme_color("SectionedListPanel.ColumnBgColor"),
        };
        BindList { path, sections, items, pairs: Vec::new(), interest: None, capture: None, selected: None, focus: None, scroll: 0, cursor: None, sounds: Vec::new(), look }
    }

    /// The item binding `binding` (`0x9a0870`).
    fn item_for(&self, binding: &str) -> Option<usize> {
        self.items.iter().position(|i| same(&i.binding, binding))
    }

    /// KeyBindings_FillInCurrent (`0x9a1370`): every key column emptied (`0x9a0d50`), then each
    /// keyboard and mouse key bound to an item's command into it, while it has an empty column:
    /// into the column the bind names.
    pub fn fill_in_current(&mut self, keys: &mut KeyBindings) {
        for item in &mut self.items {
            item.keys = Default::default();
        }
        for code in 0..=0x134i32 {
            if GAMEPAD.contains(&code) {
                continue;
            }
            let code = code as u16;
            let binding = keys.binding(code).to_string();
            if binding.is_empty() {
                continue;
            }
            let (Some(item), Some(name)) = (self.item_for(&binding), key_name(code)) else { continue };
            if self.items[item].keys.iter().any(String::is_empty) {
                let column = self.column_for(item, Some(code), keys);
                self.add_binding(item, name, column, false, keys);
            }
        }
    }

    /// `0x9a09d0`: for a key, the column its bind names; otherwise Key1 when empty, else Key2
    /// when empty, else the one `LastBoundColumn` is not (Key1 when it is unset).
    fn column_for(&self, item: usize, code: Option<u16>, keys: &KeyBindings) -> usize {
        if let Some(code) = code.filter(|&c| c > 0) {
            return usize::from(keys.column(code) != 0);
        }
        let [k1, k2] = &self.items[item].keys;
        if k1.is_empty() {
            0
        } else if k2.is_empty() {
            1
        } else {
            usize::from(self.items[item].last_bound == Some(0))
        }
    }

    /// `0x9a1040`: the key taken off every item that shows it, but one binding what `item` binds;
    /// an item left with its Key2 that is another key moves it to Key1 (`add_binding`).
    fn remove_key(&mut self, item: Option<usize>, key: &str, keys: &mut KeyBindings) {
        if key.is_empty() {
            return;
        }
        let key = key.to_string();
        for j in 0..self.items.len() {
            for col in 0..2 {
                if !same(&self.items[j].keys[col], &key) {
                    continue;
                }
                if item.is_some_and(|i| same(&self.items[i].binding, &self.items[j].binding)) {
                    continue;
                }
                self.items[j].keys[col].clear();
                let k2 = self.items[j].keys[1].clone();
                if !k2.is_empty() && !same(&k2, &key) {
                    self.add_binding(j, &k2, 0, true, keys);
                }
            }
        }
    }

    /// `0x9a0aa0`: the key off the other items (`remove_key`), then when `apply` the column's
    /// old key unbound and the new one bound to the item's command; then every item binding the
    /// same command shows the key in the column, and not in its other.
    fn add_binding(&mut self, item: usize, key: &str, column: usize, apply: bool, keys: &mut KeyBindings) {
        self.remove_key(Some(item), key, keys);
        let binding = self.items[item].binding.clone();
        if apply {
            if let Some(code) = key_code(&self.items[item].keys[column]) {
                unbind_key(code, keys);
            }
            if let Some(code) = key_code(key) {
                self.bind_key(code, &binding, column, keys);
            }
        }
        for it in self.items.iter_mut().filter(|i| same(&i.binding, &binding)) {
            it.keys[column] = key.to_string();
            let other = 1 - column;
            if same(&it.keys[other], key) {
                it.keys[other].clear();
            }
        }
    }

    /// `0x9a1470`: `bind "KEY" "BINDING" "COLUMN"`, then `bind_held` with the held half of a pair
    /// whose tapped half is the binding, else `unbind_held`.
    fn bind_key(&self, code: u16, binding: &str, column: usize, keys: &mut KeyBindings) {
        let held = self.pairs.iter().find(|(tapped, _)| tapped == binding).map(|(_, h)| h.clone()).unwrap_or_default();
        let Some(r) = keys.record(code) else { return };
        r.binding = binding.to_string();
        r.column = column as u8;
        r.held = held;
    }

    /// The column a click is on (`0x4e7bc0`, the item's `+0x330`) as a key column: Key1 or Key2;
    /// none for the action or, without `single_click_edit`, the first free one.
    fn clicked_column(&self, item: usize, clicked: Option<usize>, keys: &KeyBindings) -> Option<usize> {
        match clicked {
            Some(c) if c >= 1 => Some(c - 1),
            _ if !self.look.single_click_edit => Some(self.column_for(item, None, keys)),
            _ => None,
        }
    }

    /// `ChangeKey` (KeyboardBindingsPanel `0x99f9c0`, from a click's Enter): the item becomes the
    /// one of interest and its column captures the next key (StartCaptureMode `0x926620`).
    fn change_key(&mut self, item: usize, clicked: Option<usize>, keys: &KeyBindings) {
        if self.capture.is_some() {
            return;
        }
        if let Some(col) = self.clicked_column(item, clicked, keys) {
            self.interest = Some(item);
            self.capture = Some(col);
        }
    }

    /// `ClearKey` (`0x99f9c0`, from a right click): the column's key unbound and taken off every
    /// item.
    fn clear_key(&mut self, item: usize, clicked: Option<usize>, keys: &mut KeyBindings) {
        if self.capture.is_some() {
            return;
        }
        let Some(col) = self.clicked_column(item, clicked, keys) else { return };
        let key = self.items[item].keys[col].clone();
        if key.is_empty() {
            return;
        }
        if let Some(code) = key_code(&key) {
            unbind_key(code, keys);
        }
        self.remove_key(None, &key, keys);
    }

    /// Delete on the panel while not capturing (`0x9a2560`): each key column of the item of
    /// interest, in turn, as `clear_key` clears one. Clearing Key1 moves Key2 there first, so
    /// only the first key goes.
    pub fn delete(&mut self, keys: &mut KeyBindings) {
        let Some(item) = self.interest.filter(|_| self.capture.is_none()) else { return };
        for col in 0..2 {
            let key = self.items[item].keys[col].clone();
            if key.is_empty() {
                continue;
            }
            if let Some(code) = key_code(&key) {
                unbind_key(code, keys);
            }
            self.remove_key(None, &key, keys);
        }
    }

    /// A key pressed while capturing (OnThink `0x9a24f0` through `0x9a22c0`): a gamepad's is
    /// passed over (false: the capture goes on). Otherwise the capture ends: Escape (or no key)
    /// cancels with `UI_Menu_Back`; any other key is bound into the column, Key1 instead of
    /// Key2 when Key1 is empty or that key, with `UI_Menu_Accept`.
    pub fn finish(&mut self, code: u16, keys: &mut KeyBindings) -> bool {
        if GAMEPAD.contains(&i32::from(code)) {
            return false;
        }
        let Some(mut col) = self.capture.take() else { return true };
        let Some(item) = self.interest else { return true };
        let name = key_name(code).filter(|_| code != ESCAPE);
        let Some(name) = name else {
            self.sounds.push(BACK.into());
            return true;
        };
        if col == 1 && (self.items[item].keys[0].is_empty() || same(&self.items[item].keys[0], name)) {
            col = 0;
        }
        self.add_binding(item, name, col, true, keys);
        self.items[item].last_bound = Some(col);
        self.sounds.push(ACCEPT.into());
        true
    }

    /// The rows from the top (LayoutPanels `0x4e8b50`): from 5 less the scroll, each section's
    /// header, its items `linespacing` tall and `linePadding` apart, then 8 more. The content's
    /// height is where the last ends, the scroll added back.
    pub fn rows(&self, screen: Screen) -> (Vec<Row>, i32) {
        let header = Label::pixels(&self.label(String::new(), self.look.header_height, [0; 4]), screen) + 7;
        let spacing = screen.scale(self.look.line_spacing);
        let mut y = 5 - self.scroll;
        let mut out = Vec::new();
        for (s, _) in self.sections.iter().enumerate() {
            out.push(Row::Header { section: s, y, h: header });
            y += header;
            for (i, _) in self.items.iter().enumerate().filter(|(_, it)| it.section == s) {
                out.push(Row::Item { item: i, y, h: spacing });
                y += spacing + self.look.line_padding;
            }
            y += 8;
        }
        (out, y + self.scroll)
    }

    /// The ScrollBar's value kept within its range, the content less the list's height.
    fn clamp_scroll(&mut self, list: &Placed, screen: Screen) {
        let (_, content) = self.rows(screen);
        self.scroll = self.scroll.min(content - list.size.1).max(0);
    }

    /// A wheel notch over the list (`0x4ea7c0`): 60 a notch.
    pub fn wheel(&mut self, list: &Placed, screen: Screen, up: bool) {
        self.scroll += if up { -60 } else { 60 };
        self.clamp_scroll(list, screen);
    }

    fn widths(screen: Screen) -> [i32; 3] {
        COLUMNS.map(|c| screen.scale(c.2))
    }

    /// The item and column under a point (`0x4e7bc0`): within the item's height, then within a
    /// column, the columns `linePadding` apart; none for a column with flag `0x40`.
    fn at(&self, list: &Placed, screen: Screen, (x, y): (i32, i32)) -> Option<(usize, Option<usize>)> {
        let (x0, y0, x1, y1) = list.clip_rect;
        if x < x0 || x >= x1 || y < y0 || y >= y1 {
            return None;
        }
        let (rows, _) = self.rows(screen);
        let widths = Self::widths(screen);
        for row in rows {
            let Row::Item { item, y: top, h } = row else { continue };
            let top = list.abs.1 + top;
            if y < top || top + h < y {
                continue;
            }
            let mut at = list.abs.0 + 5;
            for (c, w) in widths.iter().enumerate() {
                if at <= x && x <= at + w {
                    return Some((item, (COLUMNS[c].3 & NO_BACKGROUND == 0).then_some(c)));
                }
                at += w + self.look.line_padding;
            }
            return Some((item, None));
        }
        None
    }

    /// The cursor over the list (PaintBackground `0x4e74a0`): entering a key column plays
    /// `UI_Menu_Focus`, but while capturing.
    pub fn hover(&mut self, list: &Placed, screen: Screen, cursor: (i32, i32)) {
        self.cursor = Some(cursor);
        let over = self.at(list, screen, cursor).and_then(|(i, c)| Some((i, c?)));
        if self.capture.is_some() {
            return;
        }
        if over.is_some() && over != self.focus {
            self.sounds.push(FOCUS.into());
        }
        self.focus = over;
    }

    /// A mouse button going down on the list (CItemButton's OnMousePressed `0x4e7ac0`): the item
    /// selected, then on a column the left button changes its key and the right clears it, with
    /// `UI_Menu_Accept` on a key column. While capturing the press is the key captured instead
    /// (`finish`).
    pub fn press(&mut self, list: &Placed, screen: Screen, cursor: (i32, i32), code: u16, keys: &mut KeyBindings) {
        if self.capture.is_some() {
            self.finish(code, keys);
            return;
        }
        let Some((item, col)) = self.at(list, screen, cursor) else { return };
        self.selected = Some(item);
        if !self.look.single_click_edit || col.is_none() {
            return;
        }
        match code {
            MOUSE_LEFT => self.change_key(item, col, keys),
            MOUSE_RIGHT => self.clear_key(item, col, keys),
            _ => return,
        }
        if col.is_some_and(|c| c >= 1) {
            self.sounds.push(ACCEPT.into());
        }
    }

    fn label(&self, text: String, height: i32, color: [u8; 4]) -> Label {
        Label { text, caps: false, height, face: self.look.face, color, alignment: 3, inset: (0, 0), proportional_insets: false, auto: (false, false) }
    }

    /// What the list draws, in its order: per section the header's text (HeaderTextColor) and
    /// its divider (CSectionHeader's Paint `0x4e63b0`: from 1, height less 2 to width less 2,
    /// height less 1); per item each key column's background (`0x4e74a0`: MouseOverBgColor
    /// under the cursor but while capturing, else ColumnBgColor), then each column's text
    /// (PerformLayout `0x4e6b00`): the first at 6, a centred one centred in its width, the
    /// columns' widths added without the padding; a key as `%KEY%`, bright for flag 4,
    /// SelectedTextColor on the selected item. Rows sit at 5 across, as wide as the list less 10.
    /// A row out of the list's clip rectangle paints nothing (each is a panel, PaintTraverse
    /// `0x4c4130`).
    pub fn paints(&self, ui: &Ui, list: &Placed, screen: Screen) -> Vec<Paint> {
        let clip = list.clip_rect;
        let (rows, _) = self.rows(screen);
        let widths = Self::widths(screen);
        let (x, w) = (list.abs.0 + 5, list.size.0 - 10);
        let hovered = self.cursor.and_then(|c| self.at(list, screen, c)).filter(|_| self.capture.is_none());
        let mut out = Vec::new();
        let text = |out: &mut Vec<Paint>, label: Label, at: (i32, i32), h: i32| {
            let content = label.content(ui, screen);
            out.push(Paint::Label { pos: (at.0, at.1 + (h - content.1) / 2), label, clip });
        };
        for row in rows {
            let (Row::Header { y, h, .. } | Row::Item { y, h, .. }) = row;
            if list.abs.1 + y + h <= clip.1 || clip.3 <= list.abs.1 + y {
                continue;
            }
            match row {
                Row::Header { section, y, h } => {
                    let y = list.abs.1 + y;
                    let l = self.label(label_text(ui, &self.sections[section]), self.look.header_height, self.look.header);
                    text(&mut out, l, (x, y), h);
                    out.push(Paint::Fill { rect: (x + 1, y + h - 2, x + w - 2, y + h - 1), color: self.look.divider, clip });
                }
                Row::Item { item, y, h } => {
                    let y = list.abs.1 + y;
                    let it = &self.items[item];
                    let mut at = x;
                    for (c, cw) in widths.iter().enumerate() {
                        if COLUMNS[c].3 & NO_BACKGROUND == 0 {
                            let color = if hovered == Some((item, Some(c))) { self.look.mouse_over } else { self.look.column };
                            out.push(Paint::Fill { rect: (at, y, at + cw, y + h), color, clip });
                        }
                        at += cw + self.look.line_padding;
                    }
                    let mut cum = 0;
                    for (c, cw) in widths.iter().enumerate() {
                        let flags = COLUMNS[c].3;
                        let value = if c == 0 { label_text(ui, &it.action) } else { it.keys[c - 1].clone() };
                        let value = if flags & NAMED != 0 && !value.is_empty() { format!("%{value}%") } else { value };
                        let color = if self.selected == Some(item) {
                            self.look.selected
                        } else if flags & BRIGHT != 0 {
                            self.look.bright
                        } else {
                            self.look.text
                        };
                        let l = self.label(value, self.look.font_height, color);
                        let tx = if c == 0 {
                            cum + 6
                        } else if flags & CENTRED != 0 {
                            cum + cw / 2 - l.content(ui, screen).0 / 2
                        } else {
                            cum
                        };
                        if !l.text.is_empty() {
                            text(&mut out, l, (x + tx, y), h);
                        }
                        cum += cw;
                    }
                }
            }
        }
        out
    }
}

/// The list file's sections and items.
fn parse(text: &str) -> (Vec<String>, Vec<Item>) {
    let (mut sections, mut items) = (Vec::new(), Vec::new());
    for &[binding, desc] in words(text).as_chunks::<2>().0 {
        if same(binding, "blank") {
            if !desc.starts_with('=') {
                sections.push(desc.to_string());
            }
        } else if let Some(section) = sections.len().checked_sub(1) {
            items.push(Item { section, action: desc.to_string(), binding: binding.to_string(), keys: Default::default(), last_bound: None });
        }
    }
    (sections, items)
}

/// `0x9a17e0`: `unbind` and `unbind_held` on the key.
fn unbind_key(code: u16, keys: &mut KeyBindings) {
    if let Some(r) = keys.record(code) {
        r.binding.clear();
        r.held.clear();
    }
}

/// The tapped and held pairs a script adds (`KeyBindings_AddTappedHeldPair( panel, "a", "b" )`).
pub fn pairs(script: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = script;
    while let Some(i) = rest.find("KeyBindings_AddTappedHeldPair") {
        rest = &rest[i + 1..];
        let line = rest.lines().next().unwrap_or_default();
        let quoted: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
        if let [a, b, ..] = quoted.as_slice() {
            out.push((a.to_string(), b.to_string()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(keys: &mut KeyBindings) -> BindList {
        let text = "\"blank\" \"===\" \"blank\" \"#MOVE\" \"blank\" \"===\" \"+forward\" \"#FWD\" \"+jump\" \"#JUMP\" \"+duck\" \"#DUCK\" \"weaponSelectOrdnance\" \"#NADE\"";
        let (sections, items) = parse(text);
        let look = Look { line_spacing: 40, line_padding: 4, font_height: 30, header_height: 26, single_click_edit: true, face: 0, header: [0; 4], divider: [0; 4], text: [0; 4], bright: [0; 4], selected: [0; 4], mouse_over: [0; 4], column: [0; 4] };
        let mut l = BindList { path: Vec::new(), sections, items, pairs: vec![("weaponSelectOrdnance".into(), "+strafe".into())], interest: None, capture: None, selected: None, focus: None, scroll: 0, cursor: None, sounds: Vec::new(), look };
        l.fill_in_current(keys);
        l
    }

    #[test]
    fn binds_round_trip_by_code() {
        let lines = ["bind_US_standard \"w\" \"+forward\" 0", "bind_US_standard \"SPACE\" \"+jump\" 0", "bind_US_standard \"MWHEELDOWN\" \"+jump\" 1", "bind_US_standard \"A_BUTTON\" \"+jump\" 0"];
        let k = KeyBindings::from_lines(&lines);
        assert_eq!(k.lines(), ["bind_US_standard \"w\" \"+forward\" 0", "bind_US_standard \"SPACE\" \"+jump\" 0", "bind_US_standard \"MWHEELDOWN\" \"+jump\" 1", "bind_US_standard \"A_BUTTON\" \"+jump\" 0"]);
        assert_eq!(key_code("space"), Some(0x41));
        assert_eq!(key_name(ESCAPE), Some("ESCAPE"));
    }

    #[test]
    fn the_list_fills_in_and_binds_as_the_panel_does() {
        let mut keys = KeyBindings::from_lines(&["bind_US_standard \"w\" \"+forward\" 0", "bind_US_standard \"SPACE\" \"+jump\" 0", "bind_US_standard \"MWHEELDOWN\" \"+jump\" 1"]);
        let mut l = list(&mut keys);
        assert_eq!(l.items[0].keys, ["w".to_string(), String::new()]);
        assert_eq!(l.items[1].keys, ["SPACE".to_string(), "MWHEELDOWN".to_string()], "each key in the column its bind names");
        l.change_key(2, Some(1), &keys);
        assert!(!l.finish(0x80, &mut keys) && l.capture.is_some(), "a gamepad's button is passed over");
        assert!(l.finish(key_code("SPACE").unwrap(), &mut keys));
        assert_eq!(l.items[2].keys, ["SPACE".to_string(), String::new()]);
        assert_eq!(l.items[1].keys, ["MWHEELDOWN".to_string(), String::new()], "taken off jump, whose Key2 moves up");
        assert_eq!((keys.binding(0x41), keys.binding(0x72), keys.column(0x72)), ("+duck", "+jump", 0));
        assert_eq!(l.sounds, ["UI_Menu_Accept"]);
        l.change_key(2, Some(2), &keys);
        l.finish(key_code("c").unwrap(), &mut keys);
        assert_eq!(l.items[2].keys, ["SPACE".to_string(), "c".to_string()]);
        assert_eq!((keys.column(key_code("c").unwrap()), l.items[2].last_bound), (1, Some(1)));
        l.change_key(2, Some(1), &keys);
        l.finish(ESCAPE, &mut keys);
        assert_eq!(l.items[2].keys, ["SPACE".to_string(), "c".to_string()], "Escape cancels");
        assert_eq!(l.sounds.last().map(String::as_str), Some("UI_Menu_Back"));
        l.change_key(3, Some(2), &keys);
        l.finish(key_code("g").unwrap(), &mut keys);
        assert_eq!(l.items[3].keys, ["g".to_string(), String::new()], "Key2 goes to Key1 when Key1 is empty");
        assert!(keys.lines().contains(&"bind_held_US_standard \"g\" \"+strafe\" 0".to_string()), "a tapped and held pair");
        l.clear_key(2, Some(1), &mut keys);
        assert_eq!(l.items[2].keys, ["c".to_string(), String::new()], "a right click clears, Key2 moving up");
        assert_eq!((keys.binding(0x41), keys.column(key_code("c").unwrap())), ("", 0));
        l.interest = Some(1);
        l.delete(&mut keys);
        assert_eq!(l.items[1].keys, [String::new(), String::new()]);
        assert_eq!(keys.binding(0x72), "");
    }

    #[test]
    fn rows_stack_with_spacing_padding_and_section_gaps() {
        let mut keys = KeyBindings::default();
        let l = list(&mut keys);
        let screen = Screen { wide: 1920, tall: 1080 };
        let (rows, total) = l.rows(screen);
        assert_eq!(rows[0], Row::Header { section: 0, y: 5, h: 33 });
        assert_eq!(rows[1], Row::Item { item: 0, y: 38, h: 40 });
        assert_eq!(rows[2], Row::Item { item: 1, y: 82, h: 40 });
        assert_eq!(total, 38 + 4 * 44 + 8);
    }
}
