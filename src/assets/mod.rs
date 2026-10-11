//! The game's own files, read from an R5Reloaded install at run time: paks, models,
//! animations and settings. Nothing from them is kept in this repository.

pub mod exe;
pub mod model;
pub mod rpak;
pub mod rtech;
pub mod script;
pub mod settings;
pub mod texture;
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

/// What the viewmodel and the player's sounds are read from: the legend's player settings, the
/// fists' script and viewmodel, and the legend's arms.
pub struct Loadout {
    pub legend: settings::Settings,
    pub weapon_script: HashMap<String, String>,
    pub weapon: model::Model,
    /// The legend's arms, with body 0's meshes: the first model of each body part, as they are
    /// drawn while nothing sets a bodygroup.
    pub arms: model::Model,
    /// For each of the arms' meshes, its material's textures in the shader's order; a texture
    /// that does not load is None.
    pub arms_textures: Vec<Vec<Option<texture::Texture>>>,
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
/// that does not load is None.
pub fn mesh_textures(pak: &rpak::Pak, install: &Path, m: &model::Model) -> Vec<Vec<Option<texture::Texture>>> {
    let slots = m.studio.textures();
    m.meshes
        .iter()
        .map(|mesh| {
            let guids = slots.get(m.studio.skin(mesh.material) as usize).and_then(|&(_, g)| texture::material(pak, g).ok()).unwrap_or_default();
            guids.into_iter().map(|g| texture::load(pak, install, g).ok()).collect()
        })
        .collect()
}

pub fn loadout(install: &Path, legend: &str) -> Result<Loadout, String> {
    let inst = Install::open(install).map_err(|e| e.to_string())?;
    let early = inst.pak("common_early.rpak").map_err(|e| e.to_string())?;
    let mut layouts = settings::layouts(&early);
    drop(early);
    let common = inst.pak("common.rpak").map_err(|e| e.to_string())?;
    layouts.extend(settings::layouts(&common));
    let legend = legend_settings(&inst, &common, &mut layouts, legend)?;
    let weapon_script = script::weapon(install, FISTS).map_err(|e| format!("{FISTS}: {e}"))?;
    let vm = weapon_script.get("viewmodel").ok_or(format!("{FISTS} names no viewmodel"))?;
    let weapon = model::load(&common, install, vm).map_err(|e| format!("{vm}: {e}"))?;
    let arms_name = legend.str("armsModel").ok_or("the legend has no armsModel")?.to_string();
    let mut arms = model::load(&common, install, &arms_name).map_err(|e| format!("{arms_name}: {e}"))?;
    arms.meshes.retain(|m| m.model == 0);
    let arms_textures = mesh_textures(&common, install, &arms);
    Ok(Loadout { legend, weapon_script, weapon, arms, arms_textures })
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
        let Some((pak, guids)) = paks.iter().find_map(|p| Some((*p, texture::material(p, material).ok()?))) else { return Vec::new() };
        guids.into_iter().map(|g| texture::load(pak, install, g).ok()).collect()
    };
    let slots = hook.studio.textures();
    let hook_textures = hook.meshes.iter().map(|m| slots.get(hook.studio.skin(m.material) as usize).map(|&(_, g)| textures(g)).unwrap_or_default()).collect();
    let rope = early.find(b"matl", 0x18, GRAPPLE_ROPE).map(|a| textures(a.guid)).ok_or(format!("{GRAPPLE_ROPE} not in common_early.rpak"))?;
    Ok(Grapple { hook, hook_textures, rope })
}
