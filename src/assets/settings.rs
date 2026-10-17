//! Settings assets (`stgs` version 1) and their layouts (`stlt`): the baked key-value blocks the
//! game keeps player, weapon and other settings in. A settings asset is values at offsets; its
//! layout names them.

use std::collections::HashMap;

use super::rpak::Pak;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i32),
    Float(f32),
    Float2([f32; 2]),
    Float3([f32; 3]),
    /// Strings and asset names.
    Str(String),
    Other,
}

/// A layout's fields: (name, type, offset into the values).
#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub fields: Vec<(String, u16, usize)>,
}

#[derive(Clone, Debug, Default)]
pub struct Settings {
    pub name: String,
    pub layout: u64,
    pub values: HashMap<String, Value>,
}

impl Settings {
    pub fn str(&self, key: &str) -> Option<&str> {
        match self.values.get(key)? {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn float(&self, key: &str) -> Option<f32> {
        match self.values.get(key)? {
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }
    pub fn float3(&self, key: &str) -> Option<[f32; 3]> {
        match self.values.get(key)? {
            Value::Float3(v) => Some(*v),
            _ => None,
        }
    }
    pub fn bool(&self, key: &str) -> Option<bool> {
        match self.values.get(key)? {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// Every settings layout in a pak, by GUID.
pub fn layouts(pak: &Pak) -> HashMap<u64, Layout> {
    let mut out = HashMap::new();
    for a in pak.assets.iter().filter(|a| &a.magic == b"stlt") {
        let h = a.head;
        let (Some(fields), Some(strings)) = (pak.ptr(h + 8), pak.ptr(h + 0x38)) else { continue };
        let buckets = pak.u32(h + 0x18) as usize;
        let mut l = Layout::default();
        for i in 0..buckets {
            let f = fields + i * 8;
            let (ty, name_off, packed) = (pak.u16(f), pak.u16(f + 2) as usize, pak.u32(f + 4));
            if name_off == 0 {
                continue;
            }
            l.fields.push((pak.cstr(strings + name_off).to_string(), ty, (packed & 0xff_ffff) as usize));
        }
        out.insert(a.guid, l);
    }
    out
}

/// The settings asset named `name` (`settings/...rpak`), its values read through its layout.
pub fn load(pak: &Pak, layouts: &HashMap<u64, Layout>, name: &str) -> Option<Settings> {
    let a = pak.find(b"stgs", 0x10, name)?;
    if a.head_size != 72 {
        return None;
    }
    let layout_guid = pak.u64(a.head);
    let layout = layouts.get(&layout_guid)?;
    let values = pak.ptr(a.head + 8)?;
    let mut s = Settings { name: name.to_string(), layout: layout_guid, values: HashMap::new() };
    for (n, ty, off) in &layout.fields {
        let at = values + off;
        let v = match ty {
            0 => Value::Bool(pak.u8(at) != 0),
            1 => Value::Int(pak.i32(at)),
            2 => Value::Float(pak.f32(at)),
            3 => Value::Float2([pak.f32(at), pak.f32(at + 4)]),
            4 => Value::Float3([pak.f32(at), pak.f32(at + 4), pak.f32(at + 8)]),
            5..=7 => Value::Str(pak.ptr(at).map(|p| pak.cstr(p).to_string()).unwrap_or_default()),
            _ => Value::Other,
        };
        s.values.insert(n.clone(), v);
    }
    Some(s)
}

/// The names of every settings asset in a pak.
pub fn names(pak: &Pak) -> Vec<String> {
    pak.assets.iter().filter(|a| &a.magic == b"stgs").filter_map(|a| pak.name(a, 0x10)).map(String::from).collect()
}
