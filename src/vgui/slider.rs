//! BaseModUI's SliderControl (vtable `0x148d608`), the settings menu's slider rows: an
//! EditablePanel whose children come from `Resource/UI/BaseModUI/SliderControl.res`
//! (`BtnDropButton`, `LblSliderText`, `PrgValue`, `PnlDefaultMark`, all RuiPanels), its value
//! bound to the ConVar `conCommand` names.
//!
//! Not modelled: `playlistVarName` and `gamemodeIdx` (no settings slider has them), a Label as
//! `LblSliderText` (`SliderControl.res` makes a RuiPanel), the scheme's colours for the mark and
//! the bar (RuiPanels paint none of them), the arrow keys and the gamepad (`0x94f160`), and the
//! signals a change sends.

use crate::assets::keyvalues::{atof, atoi, kind, Key, Kind};

/// The value a slider holds and how it maps onto its bar.
#[derive(Clone, Debug, PartialEq)]
pub struct Slider {
    pub convar: Option<String>,
    pub default_convar: Option<String>,
    min: f32,
    max: f32,
    step: f32,
    inverse: bool,
    clamped_to_step: bool,
    show_label: i32,
    /// `+0x320`.
    pub value: f32,
    /// `+0x2d3`: the cursor has set the value since the button went down.
    pub dragging: bool,
    /// The default the mark shows, once the ConVars are read.
    pub default: Option<f32>,
}

/// The parts of a ConVar a slider reads: its float, its int and its default as `atof` reads it.
#[derive(Clone, Copy, Debug)]
pub struct ConVarValue {
    pub float: f32,
    pub int: i32,
    pub default: f32,
}

/// KeyValues' GetFloat (`0x474a20`) on a panel's settings.
fn get_float(s: &Key, name: &str, default: f32) -> f32 {
    match s.str(name).map(kind) {
        None => default,
        Some(Kind::Int(i)) => i as f32,
        Some(Kind::Float(f)) => f,
        Some(Kind::Uint64(u)) => u as f32,
        Some(Kind::Str) => atof(s.str(name).unwrap_or_default()) as f32,
    }
}

/// The GetInt ApplySettings inlines: a string through `atoi`, a float truncated.
fn get_int(s: &Key, name: &str) -> i32 {
    match s.str(name).map(kind) {
        None => 0,
        Some(Kind::Int(i)) => i,
        Some(Kind::Float(f)) => f as i32,
        Some(Kind::Uint64(u)) => u as i32,
        Some(Kind::Str) => atoi(s.str(name).unwrap_or_default()),
    }
}

impl Slider {
    /// The constructor's values (`0x94d9b0`: 0 to 1, step 1, at 0.5), then ApplySettings
    /// (`0x94e570`): `inverseFill` and `isValueClampedToStepSize` when 1, `stepSize` (1),
    /// `minValue` (0) and `maxValue` (100), each bound putting the value back through `set`,
    /// `conCommand`, `conCommandDefault` and `showLabel`.
    pub fn new(s: &Key) -> Slider {
        let mut slider = Slider {
            convar: None,
            default_convar: None,
            min: 0.0,
            max: 1.0,
            step: 1.0,
            inverse: get_int(s, "inverseFill") == 1,
            clamped_to_step: get_int(s, "isValueClampedToStepSize") == 1,
            show_label: 0,
            value: 0.5,
            dragging: false,
            default: None,
        };
        slider.step = get_float(s, "stepSize", 1.0);
        slider.min = get_float(s, "minValue", 0.0);
        slider.set(slider.value, true);
        slider.max = get_float(s, "maxValue", 100.0);
        slider.set(slider.value, true);
        slider.convar = s.str("conCommand").filter(|c| !c.is_empty()).map(String::from);
        slider.default_convar = s.str("conCommandDefault").filter(|c| !c.is_empty()).map(String::from);
        slider.show_label = get_int(s, "showLabel");
        slider
    }

    /// SetValue (`0x94dd90`): the value rounded to the step when clamped to it, then kept within
    /// the bounds. Unless forced, the same value does nothing, and a new one plays
    /// `UI_Menu_Accept` when not dragging. Returns whether the value changed and whether the
    /// sound plays; the caller writes the value to the ConVar and the bar and label follow it.
    pub fn set(&mut self, v: f32, force: bool) -> (bool, bool) {
        let mut v = v;
        if self.clamped_to_step && self.step != 0.0 {
            v = ((v / self.step) as f64 + 0.5) as i32 as f32 * self.step;
        }
        let below = if v < self.max { v } else { self.max };
        let v = if self.min < below { if self.max <= v { self.max } else { v } } else { self.min };
        if !force && v == self.value {
            return (false, false);
        }
        let sound = !force && !self.dragging;
        let changed = self.value != v;
        self.value = v;
        (changed, sound)
    }

    /// Where the value sits along the bar, as SetValue hands it to `PrgValue`'s `progress`.
    pub fn fraction(&self) -> f32 {
        let f = (self.value - self.min) / (self.max - self.min);
        if self.inverse { 1.0 - f } else { f }
    }

    /// The mark's place along the bar (`0x94e240`): the default of `conCommandDefault`'s ConVar,
    /// else of `conCommand`'s, else the maximum, clamped to the bar.
    pub fn default_fraction(&self) -> f32 {
        let v = self.default.unwrap_or(self.max);
        let f = (v - self.min) / (self.max - self.min);
        let mut c = 0.0;
        if 0.0 <= f {
            c = 1.0;
            if f <= 1.0 {
                c = f;
            }
        }
        if self.inverse { 1.0 - c } else { c }
    }

    /// `LblSliderText`'s text (`0x94ff80`), by `showLabel`: 1 the ConVar's int, 2 its float to
    /// one place, 3 its float as a whole percentage, truncated; without a ConVar, `value` in its
    /// place. Any other is empty.
    pub fn label(&self, convar: Option<ConVarValue>, value: f32) -> String {
        match self.show_label {
            1 => convar.map_or(value as i32, |c| c.int).to_string(),
            2 => format!("{:.1}", convar.map_or(value, |c| c.float) as f64),
            3 => format!("{}%", (convar.map_or(value, |c| c.float) * 100.0) as i32),
            _ => String::new(),
        }
    }

    /// The value the cursor at `x` gives, when it is on the bar (`bar`: its absolute
    /// position and size, edges included) or the slider is being dragged (`0x94f6f0`): the
    /// cursor's place along the bar, clamped, onto the range. None for a bar with no width.
    pub fn at_cursor(&self, (x, y): (i32, i32), bar: ((i32, i32), (i32, i32))) -> Option<f32> {
        let ((bx, by), (bw, bh)) = bar;
        let within = bx <= x && x <= bw + bx && by <= y && y <= by + bh;
        if !(self.dragging || within) || bw <= 0 {
            return None;
        }
        let f = (x - bx) as f32 / bw as f32;
        let mut c = 0.0;
        if 0.0 <= f {
            c = 1.0;
            if f <= 1.0 {
                c = f;
            }
        }
        if self.inverse {
            c = 1.0 - c;
        }
        Some((self.max - self.min) * c + self.min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::keyvalues;

    fn slider(text: &str) -> Slider {
        Slider::new(&keyvalues::parse(text.as_bytes()).remove(0))
    }

    #[test]
    fn a_slider_keeps_its_value_within_its_range_and_labels_it() {
        let mut s = slider("S { conCommand sound_volume minValue 0.0 maxValue 1.0 stepSize 0.05 showLabel 3 }");
        assert_eq!((s.value, s.convar.as_deref()), (0.5, Some("sound_volume")), "the constructor's 0.5");
        assert_eq!(s.set(0.5, false), (false, false));
        assert_eq!(s.set(1.5, false), (true, true));
        assert_eq!(s.value, 1.0, "not above the maximum");
        s.dragging = true;
        assert_eq!(s.set(-1.0, false), (true, false), "no sound while dragging");
        assert_eq!(s.value, 0.0);
        let c = ConVarValue { float: 0.129, int: 0, default: 1.0 };
        assert_eq!(s.label(Some(c), 0.0), "12%", "truncated, not rounded");
        let s = slider("S { minValue 0 maxValue 10000 stepSize 50 showLabel 1 isValueClampedToStepSize 1 inverseFill 1 }");
        assert_eq!(s.label(None, 4321.7), "4321");
        let mut s = s;
        s.default = Some(2500.0);
        assert_eq!(s.default_fraction(), 0.75, "the mark from the far end when inverse");
        assert_eq!(s.at_cursor((150, 30), ((100, 0), (200, 60))), Some(7500.0), "a quarter along an inverse bar");
        assert_eq!(s.at_cursor((350, 30), ((100, 0), (200, 60))), None, "off the bar and not dragging");
        let mut s = slider("S { minValue 0 maxValue 10000 stepSize 50 isValueClampedToStepSize 1 }");
        s.set(1234.0, false);
        assert_eq!(s.value, 1250.0, "rounded to the step");
    }
}
