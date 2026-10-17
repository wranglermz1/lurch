//! The game's own files, read from an R5Reloaded install at run time: paks, models,
//! animations and settings. Nothing from them is kept in this repository.

pub mod exe;
pub mod keyvalues;
pub mod localize;
pub mod model;
pub mod rpak;
pub mod rtech;
pub mod rui;
pub mod script;
pub mod settings;
pub mod texture;
pub mod vgui;
pub mod zstd;

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

/// An R5Reloaded install: the folder holding `r5apex.exe` and `paks/`.
pub struct Install {
    pub root: PathBuf,
    /// The executable's (timestamp, checksum); the oracle's is `exe::ORACLE_BUILD`.
    pub build: (u32, u32),
    rtech_lut: Option<Vec<u8>>,
}

impl Install {
    pub fn open(root: &Path) -> io::Result<Install> {
        let exe = exe::Exe::open(root)?;
        let build = exe.build();
        let rtech_lut = (build == exe::ORACLE_BUILD).then(|| exe.at_rva(exe::RTECH_LUT_RVA, exe::RTECH_LUT_LEN).ok().map(<[u8]>::to_vec)).flatten();
        Ok(Install { root: root.to_path_buf(), build, rtech_lut })
    }

    pub fn is_oracle_build(&self) -> bool {
        self.build == exe::ORACLE_BUILD
    }

    /// A pak by file name under `paks/Win64`.
    pub fn pak(&self, name: &str) -> io::Result<rpak::Pak> {
        rpak::Pak::open(&self.root.join("paks").join("Win64").join(name), self.rtech_lut.as_deref())
    }
}

/// The weapon a pilot holds with no gun: the fists.
pub const FISTS: &str = "mp_weapon_melee_survival";

/// A melee skin held in the fists' place: its weapon script, its viewmodel, that model's rig and
/// the pak holding both. R5Reloaded's scripts name the fists' viewmodel for most, and most
/// models list no rig, so all are named here: the pak's one first-person model of each and the
/// rig beside it.
#[derive(Debug, PartialEq, Eq)]
pub struct Heirloom {
    pub name: &'static str,
    pub weapon: &'static str,
    pub viewmodel: &'static str,
    pub rig: &'static str,
    pub pak: &'static str,
}

pub const HEIRLOOMS: &[Heirloom] = &[
    Heirloom {
        name: "wraith_kunai",
        weapon: "mp_weapon_wraith_kunai_primary",
        viewmodel: "mdl/Weapons/kunai/ptpov_kunai_wraith.rmdl",
        rig: "animrig/weapons/kunai/ptpov_kunai_wraith.rrig",
        pak: "common.rpak",
    },
    Heirloom {
        name: "bloodhound_axe",
        weapon: "mp_weapon_bloodhound_axe_primary",
        viewmodel: "mdl/Weapons/bloodhound_axe/ptpov_bloodhound_axe.rmdl",
        rig: "animrig/weapons/bloodhound_axe/ptpov_axe_bloodhound.rrig",
        pak: "common.rpak",
    },
    Heirloom {
        name: "lifeline_baton",
        weapon: "mp_weapon_lifeline_baton_primary",
        viewmodel: "mdl/Weapons/drumstick/ptpov_baton_lifeline.rmdl",
        rig: "animrig/weapons/drumstick/ptpov_baton_lifeline.rrig",
        pak: "common.rpak",
    },
    Heirloom {
        name: "octane_knife",
        weapon: "mp_weapon_macks_knife_primary",
        viewmodel: "mdl/Weapons/octane_knife/ptpov_macks_knife.rmdl",
        rig: "animrig/weapons/octane_knife/ptpov_knife_octane.rrig",
        pak: "common_sdk.rpak",
    },
];

/// What the viewmodel and the player's sounds are read from: the legend's player settings, the
/// fists' or the heirloom's script and viewmodel, and the legend's arms.
pub struct Loadout {
    pub legend: settings::Settings,
    /// The legend's character (`settings/itemflav/character/...`, the one whose `setFile` is
    /// the legend's settings): its HUD colours.
    pub character: Option<settings::Settings>,
    pub weapon_script: HashMap<String, String>,
    pub weapon: model::Model,
    /// For each of the heirloom's meshes, its material's textures; empty for the fists, whose
    /// own meshes (the data knife) are not drawn.
    pub weapon_textures: Vec<Vec<Option<texture::Texture>>>,
    pub weapon_constants: Vec<Option<Vec<u8>>>,
    /// The legend's arms, with body 0's meshes: the first model of each body part, as they are
    /// drawn while nothing sets a bodygroup.
    pub arms: model::Model,
    /// For each of the arms' meshes, its material's textures in the shader's order; a texture
    /// that does not load is None.
    pub arms_textures: Vec<Vec<Option<texture::Texture>>>,
    /// For each of the arms' meshes, its material's shader constants (`texture::uber_static`).
    pub arms_constants: Vec<Option<Vec<u8>>>,
}

/// The player settings whose `footstep_type` is the legend's name, from `common` or else the
/// SDK's own pak.
fn legend_settings(inst: &Install, common: &rpak::Pak, layouts: &mut HashMap<u64, settings::Layout>, legend: &str) -> Result<settings::Settings, String> {
    let find = |pak: &rpak::Pak, layouts: &HashMap<u64, settings::Layout>| {
        settings::names(pak)
            .into_iter()
            .filter(|n| n.starts_with("settings/player/mp/pilot_survival_"))
            .filter_map(|n| settings::load(pak, layouts, &n))
            .find(|s| s.str("footstep_type").is_some_and(|f| f.eq_ignore_ascii_case(legend)))
    };
    if let Some(s) = find(common, layouts) {
        return Ok(s);
    }
    let sdk = inst.pak("common_sdk.rpak").map_err(|e| e.to_string())?;
    layouts.extend(settings::layouts(&sdk));
    find(&sdk, layouts).ok_or_else(|| format!("no legend with footstep_type {legend}"))
}

/// For each of a model's meshes, its material's textures in the shader's order; a texture
/// that does not load is None. `shaders` (`common.rpak`) holds the shader sets and shared
/// textures another pak's materials use.
pub fn mesh_textures(pak: &rpak::Pak, shaders: &rpak::Pak, install: &Path, m: &model::Model) -> Vec<Vec<Option<texture::Texture>>> {
    let slots = m.studio.textures();
    m.meshes
        .iter()
        .map(|mesh| {
            let guids = slots.get(m.studio.skin(mesh.material) as usize).and_then(|&(_, g)| texture::material(pak, shaders, g).ok()).unwrap_or_default();
            guids.into_iter().map(|g| texture::load(pak, install, g).or_else(|_| texture::load(shaders, install, g)).ok()).collect()
        })
        .collect()
}

/// For each of a model's meshes, its material's shader constants (`texture::uber_static`).
pub fn mesh_constants(pak: &rpak::Pak, m: &model::Model) -> Vec<Option<Vec<u8>>> {
    let slots = m.studio.textures();
    m.meshes.iter().map(|mesh| slots.get(m.studio.skin(mesh.material) as usize).and_then(|&(_, g)| texture::uber_static(pak, g))).collect()
}

fn character_settings(inst: &Install, common: &rpak::Pak, layouts: &mut HashMap<u64, settings::Layout>, set_file: &str) -> Option<settings::Settings> {
    let find = |pak: &rpak::Pak, layouts: &HashMap<u64, settings::Layout>| {
        settings::names(pak)
            .into_iter()
            .filter(|n| n.starts_with("settings/itemflav/character/"))
            .filter_map(|n| settings::load(pak, layouts, &n))
            .find(|s| s.str("setFile") == Some(set_file))
    };
    find(common, layouts).or_else(|| {
        let sdk = inst.pak("common_sdk.rpak").ok()?;
        layouts.extend(settings::layouts(&sdk));
        find(&sdk, layouts)
    })
}

pub fn loadout(install: &Path, legend: &str, heirloom: Option<&Heirloom>) -> Result<Loadout, String> {
    let inst = Install::open(install).map_err(|e| e.to_string())?;
    let early = inst.pak("common_early.rpak").map_err(|e| e.to_string())?;
    let mut layouts = settings::layouts(&early);
    drop(early);
    let common = inst.pak("common.rpak").map_err(|e| e.to_string())?;
    layouts.extend(settings::layouts(&common));
    let legend = legend_settings(&inst, &common, &mut layouts, legend)?;
    let character = character_settings(&inst, &common, &mut layouts, &legend.name);
    let script_name = heirloom.map_or(FISTS, |h| h.weapon);
    let weapon_script = script::weapon(install, script_name).map_err(|e| format!("{script_name}: {e}"))?;
    let vm = match heirloom {
        Some(h) => h.viewmodel,
        None => weapon_script.get("viewmodel").ok_or(format!("{FISTS} names no viewmodel"))?,
    };
    let other = match heirloom {
        Some(h) if h.pak != "common.rpak" => Some(inst.pak(h.pak).map_err(|e| format!("{}: {e}", h.pak))?),
        _ => None,
    };
    let pak = other.as_ref().unwrap_or(&common);
    let mut weapon = model::load(pak, install, vm).map_err(|e| format!("{vm}: {e}"))?;
    let (mut weapon_textures, mut weapon_constants) = (Vec::new(), Vec::new());
    if let Some(h) = heirloom {
        if !weapon.rigs.iter().any(|r| r.name.eq_ignore_ascii_case(h.rig)) {
            weapon.rigs.push(model::load_rig(pak, h.rig).map_err(|e| e.to_string())?);
        }
        weapon_textures = mesh_textures(pak, &common, install, &weapon);
        weapon_constants = mesh_constants(pak, &weapon);
    }
    drop(other);
    let arms_name = legend.str("armsModel").ok_or("the legend has no armsModel")?.to_string();
    let mut arms = model::load(&common, install, &arms_name).map_err(|e| format!("{arms_name}: {e}"))?;
    arms.meshes.retain(|m| m.model == 0);
    let arms_textures = mesh_textures(&common, &common, install, &arms);
    let arms_constants = mesh_constants(&common, &arms);
    Ok(Loadout { legend, character, weapon_script, weapon, weapon_textures, weapon_constants, arms, arms_textures, arms_constants })
}

/// The grapple hook entity's model (CGrappleHook, `0x9d7250`).
pub const GRAPPLE_HOOK: &str = "mdl/weapons/projectiles/projectile_grapple_hook_anchor_bolt.rmdl";
/// The rope's material (`0x6be7e0`).
pub const GRAPPLE_ROPE: &str = "cable/grapple";

/// What the grapple is drawn with: the hook's model and its meshes' textures, and the rope
/// material's textures in its shader's order. The hook is in `common.rpak`; its material and
/// the rope's are in `common_early.rpak`.
pub struct Grapple {
    pub hook: model::Model,
    pub hook_textures: Vec<Vec<Option<texture::Texture>>>,
    pub rope: Vec<Option<texture::Texture>>,
}

pub fn grapple(install: &Path) -> Result<Grapple, String> {
    let inst = Install::open(install).map_err(|e| e.to_string())?;
    let common = inst.pak("common.rpak").map_err(|e| e.to_string())?;
    let early = inst.pak("common_early.rpak").map_err(|e| e.to_string())?;
    let paks = [&common, &early];
    let hook = model::load(&common, install, GRAPPLE_HOOK).map_err(|e| format!("{GRAPPLE_HOOK}: {e}"))?;
    let textures = |material: u64| -> Vec<Option<texture::Texture>> {
        let Some((pak, guids)) = paks.iter().find_map(|p| Some((*p, texture::material(p, p, material).ok()?))) else { return Vec::new() };
        guids.into_iter().map(|g| texture::load(pak, install, g).ok()).collect()
    };
    let slots = hook.studio.textures();
    let hook_textures = hook.meshes.iter().map(|m| slots.get(hook.studio.skin(m.material) as usize).map(|&(_, g)| textures(g)).unwrap_or_default()).collect();
    let rope = early.find(b"matl", 0x18, GRAPPLE_ROPE).map(|a| textures(a.guid)).ok_or(format!("{GRAPPLE_ROPE} not in common_early.rpak"))?;
    Ok(Grapple { hook, hook_textures, rope })
}
