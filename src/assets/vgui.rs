//! VGUI's layouts: the panels a `.menu` or `.res` file makes, read from the install's
//! `platform/` folder with the scheme `resource/basemodui_scheme.res`. A panel is a block with a
//! `ControlName`; its settings are its own keys, then those of the scheme's
//! `InheritableProperties` template it names in `InheritProperties` that it lacks. An
//! EditablePanel's blocks are its children, a CNestedPanel's `controlSettingsFile` adds the
//! file's, a SliderControl's children come from `SLIDER` first, and a button's `childGroup*`
//! keys add those of the scheme's `ButtonChildGroups`.
//!
//! Not modelled: a second block for a panel already made (the engine applies it over the
//! first; here it replaces the settings), and which classes the engine's factory refuses
//! (here every `ControlName` makes a panel).

use std::io;
use std::path::{Path, PathBuf};

use super::keyvalues::{self, Key};

pub const SCHEME: &str = "resource/basemodui_scheme.res";

/// The classes deriving from EditablePanel (by RTTI), whose ApplySettings applies their blocks
/// as children (`0x49f980`, Frame's `0x4a6cc0`, CNestedPanel's `0x979760`).
const EDITABLE: [&str; 5] = ["EditablePanel", "Frame", "CNestedPanel", "GridButtonListPanel", "SliderControl"];

/// The classes whose ApplySettings ends in Button's (`0x499450`; RuiButton's `0x938ee0` through
/// BaseModHybridButton's `0x946bf0`).
const BUTTONS: [&str; 3] = ["Button", "BaseModHybridButton", "RuiButton"];

/// A button's child groups, in the order Button's ApplySettings makes them; a child made from
/// one has its index as `Panel::group`.
pub const CHILD_GROUPS: [&str; 7] = [
    "childGroupAlways",
    "childGroupNormal",
    "childGroupDisabled",
    "childGroupFocused",
    "childGroupSelected",
    "childGroupLocked",
    "childGroupNew",
];

const INHERIT: &str = "InheritProperties";

/// The file a SliderControl loads into its children, in its constructor (`0x94d9b0`) and again
/// at the start of its ApplySettings (`0x94e570`), before its own blocks: the engine's
/// `Resource/UI/BaseModUI/SliderControl.res`, as the install spells it.
const SLIDER: &str = "resource/ui/basemodui/slidercontrol.res";

/// How deep files may nest: the engine only warns when a nested panel repeats an ancestor's
/// name (`0x979760`).
const MAX_DEPTH: usize = 32;

#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub name: String,
    pub class: String,
    pub settings: Key,
    pub group: Option<usize>,
    pub children: Vec<Panel>,
}

impl Panel {
    pub fn child(&self, name: &str) -> Option<&Panel> {
        self.children.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }
}

/// The scheme's `InheritableProperties`, each with its own `InheritProperties` taken in, and
/// its `ButtonChildGroups`.
pub struct Scheme {
    pub inheritable: Key,
    pub child_groups: Key,
    /// `BaseSettings`: the values panels look up by name (GetResourceString, scheme `+0x8`).
    pub base: Key,
    /// `Colors`: the colours a setting may name.
    pub colors: Key,
}

impl Scheme {
    /// `0x524510`: the file's first block holds both.
    pub fn load(platform: &Path) -> io::Result<Scheme> {
        let root = read(platform, SCHEME)?;
        let mut inheritable = root.find("InheritableProperties").cloned().unwrap_or_else(empty);
        let child_groups = root.find("ButtonChildGroups").cloned().unwrap_or_else(empty);
        if let keyvalues::Value::Block(templates) = &mut inheritable.value {
            for at in 0..templates.len() {
                inherit_template(templates, at, &mut Vec::new());
            }
        }
        let base = root.find("BaseSettings").cloned().unwrap_or_else(empty);
        let colors = root.find("Colors").cloned().unwrap_or_else(empty);
        Ok(Scheme { inheritable, child_groups, base, colors })
    }
}

fn empty() -> Key {
    Key { name: String::new(), value: keyvalues::Value::Block(Vec::new()) }
}

fn read(platform: &Path, file: &str) -> io::Result<Key> {
    let text = std::fs::read(platform.join(file))?;
    keyvalues::parse(&text).into_iter().next().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("{file}: no keys")))
}

/// The keys of `from` that `into` lacks, appended in order (`0x4ca3c0`, `0x524790`).
fn take_missing(into: &mut Key, from: &Key) {
    for k in from.keys() {
        if into.find(&k.name).is_none()
            && let keyvalues::Value::Block(keys) = &mut into.value
        {
            keys.push(k.clone());
        }
    }
}

/// `0x524790`: a template's `InheritProperties` is removed and its parent's keys taken in, the
/// parent's own first; a chain stops at 16 or where it would repeat.
fn inherit_template(templates: &mut [Key], at: usize, chain: &mut Vec<usize>) {
    let Some(pos) = templates[at].keys().iter().position(|k| k.name.eq_ignore_ascii_case(INHERIT)) else { return };
    if chain.len() >= 16 {
        return;
    }
    chain.push(at);
    let parent = templates[at].str(INHERIT).and_then(|name| templates.iter().position(|t| t.name.eq_ignore_ascii_case(name)));
    if let keyvalues::Value::Block(keys) = &mut templates[at].value {
        keys.remove(pos);
    }
    let Some(parent) = parent else { return };
    if chain.contains(&parent) {
        return;
    }
    inherit_template(templates, parent, chain);
    let from = templates[parent].clone();
    take_missing(&mut templates[at], &from);
}

struct Loader<'a> {
    platform: PathBuf,
    scheme: &'a Scheme,
}

/// The panels a layout file makes: one for each block of its first key, as LoadControlSettings
/// applies a file. A `.menu`'s is the block `menu`, the menu itself.
pub fn load(platform: &Path, scheme: &Scheme, file: &str) -> io::Result<Vec<Panel>> {
    let loader = Loader { platform: platform.to_path_buf(), scheme };
    let mut panels = Vec::new();
    loader.build_group_apply(&mut panels, &read(platform, file)?, 0)?;
    Ok(panels)
}

impl Loader<'_> {
    /// `0x48f500`: each block goes to the panel of its name, or to a new one. A block named
    /// `menu` goes to a panel whose name starts with `menu`.
    fn build_group_apply(&self, panels: &mut Vec<Panel>, block: &Key, depth: usize) -> io::Result<()> {
        for k in block.keys().iter().filter(|k| k.is_block()) {
            let lower = k.name.to_ascii_lowercase();
            let existing = panels.iter_mut().find(|p| {
                let name = p.name.to_ascii_lowercase();
                name.starts_with(&lower) && (name.len() == lower.len() || lower == "menu")
            });
            match existing {
                Some(p) => self.apply_settings(p, k, depth)?,
                None => panels.extend(self.new_control(k, None, depth)?),
            }
        }
        Ok(())
    }

    /// `0x48f760`: the class is the block's own `ControlName`, before any template.
    fn new_control(&self, block: &Key, group: Option<usize>, depth: usize) -> io::Result<Option<Panel>> {
        let class = block.str("ControlName").unwrap_or_default();
        if class.is_empty() {
            return Ok(None);
        }
        let mut p = Panel { name: block.name.clone(), class: class.to_string(), settings: empty(), group, children: Vec::new() };
        self.apply_settings(&mut p, block, depth)?;
        Ok(Some(p))
    }

    /// `0x4ca3c0`, then the class's ApplySettings for what makes children.
    fn apply_settings(&self, p: &mut Panel, block: &Key, depth: usize) -> io::Result<()> {
        let mut settings = block.clone();
        if let Some(template) = block.str(INHERIT).and_then(|name| self.scheme.inheritable.find(name)) {
            take_missing(&mut settings, template);
        }
        p.settings = settings;
        if depth >= MAX_DEPTH {
            return Ok(());
        }
        if p.class == "SliderControl" {
            let file = read(&self.platform, SLIDER)?;
            self.build_group_apply(&mut p.children, &file, depth + 1)?;
        }
        if EDITABLE.contains(&p.class.as_str()) {
            let settings = p.settings.clone();
            self.build_group_apply(&mut p.children, &settings, depth + 1)?;
        }
        if p.class == "CNestedPanel"
            && let Some(file) = p.settings.str("controlSettingsFile").filter(|f| !f.is_empty())
        {
            let nested = read(&self.platform, file)?;
            self.build_group_apply(&mut p.children, &nested, depth + 1)?;
        }
        if BUTTONS.contains(&p.class.as_str()) {
            self.child_groups(p, depth + 1)?;
        }
        Ok(())
    }

    /// `0x499450`'s end and `0x4998c0`: each group's blocks become new children.
    fn child_groups(&self, p: &mut Panel, depth: usize) -> io::Result<()> {
        for (i, key) in CHILD_GROUPS.iter().enumerate() {
            let Some(group) = p.settings.str(key).filter(|g| !g.is_empty()).and_then(|g| self.scheme.child_groups.find(g)) else { continue };
            for block in group.keys().iter().filter(|k| k.is_block()) {
                p.children.extend(self.new_control(block, Some(i), depth)?);
            }
        }
        Ok(())
    }
}
