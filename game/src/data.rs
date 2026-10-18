//! The game's files the shell plays with, read from the R5Reloaded install on a thread at start,
//! and again when the dev menu picks another heirloom: the legend's player settings and sounds,
//! the fists' or the heirloom's viewmodel and the legend's arms.
//! The test map's props are read before the simulation starts, since they collide.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use lurch::anim::bones::{Mat, Pose};
use lurch::anim::model::ModelAnims;
use lurch::anim::viewmodel::WeaponTimes;
use lurch::assets::model::{self as amodel, Bone};
use lurch::assets::settings::Settings;
use lurch::assets::texture::Texture;
use lurch::assets::{Heirloom, Install};
use lurch::sim::math::V3;
use lurch::sound::PlayerSounds;
use lurch::testmap::{Prop, PROP_PAK};
use lurch::world::Tri;

use crate::{common, Config};

pub struct Arms {
    pub bones: Vec<Bone>,
    /// For each arms bone, the weapon bone of the same name it follows.
    pub merge: Vec<Option<usize>>,
    pub meshes: Vec<amodel::Mesh>,
    /// Each mesh's material textures, in its shader's order (`lurch::assets::texture::material`).
    pub textures: Vec<Vec<Option<Texture>>>,
    /// Each mesh's material constants (`lurch::assets::texture::uber_static`).
    pub constants: Vec<Option<Vec<u8>>>,
    /// The bind pose of the arms model's own bones, which the first-person proxy plays from.
    pub defaults: Pose,
    /// The `ref`, `camera` and `camera_base` attachments: each one's bone and its place on it.
    pub reference: Option<(usize, Mat)>,
    pub camera: Option<(usize, Mat)>,
    pub camera_base: Option<(usize, Mat)>,
    /// The `grapple_rope` attachment, where the rope leaves the hand while the grapple's
    /// animation plays (`+0x134c`, `0x9c95ad`).
    pub grapple_rope: Option<(usize, Mat)>,
}

pub struct GameData {
    pub install: PathBuf,
    pub legend: Settings,
    /// The legend's character item flavour, for its HUD colours.
    pub character: Option<Settings>,
    pub sounds: PlayerSounds,
    /// The weapon's class (its script's name) and script times.
    pub weapon_class: &'static str,
    pub times: WeaponTimes,
    /// The melee weapon's `melee_anim_1p_number`: the offhand melee's (`melee_pilot_emptyhanded`,
    /// or `melee_` and the heirloom's name), 1, the registered default, where it gives none.
    pub melee_anim: i32,
    pub anims: ModelAnims,
    /// The arms model's own sequences, which the first-person proxy plays.
    pub arms_anims: ModelAnims,
    /// The weapon model's bind pose, which a sequence's untouched bones keep.
    pub defaults: Pose,
    pub arms: Arms,
    /// The weapon model's bones and the meshes drawn on them: the heirloom's, none for the fists.
    pub weapon_bones: Vec<Bone>,
    pub weapon_meshes: Vec<amodel::Mesh>,
    pub weapon_textures: Vec<Vec<Option<Texture>>>,
    pub weapon_constants: Vec<Option<Vec<u8>>>,
    /// The grapple's hook model and the rope's textures, when the install has them.
    pub grapple: Option<lurch::assets::Grapple>,
}

#[derive(Resource, Clone)]
pub struct Data(pub Arc<GameData>);

/// The game's files being read.
#[derive(Resource)]
pub struct Loading(Mutex<Receiver<Result<GameData, String>>>);

pub struct DataPlugin;

impl Plugin for DataPlugin {
    fn build(&self, app: &mut App) {
        common(app);
        let cfg = app.world().resource::<Config>().clone();
        let Some(install) = cfg.r5r.clone() else {
            warn!("no R5Reloaded install given: no sounds, no viewmodel");
            return;
        };
        app.insert_resource(start(install, cfg.legend, cfg.heirloom));
        reloading(app);
    }
}

/// Reads the files again whenever `Config` names another legend or heirloom, for an app that
/// read them itself.
pub fn reloading(app: &mut App) {
    app.add_systems(PreUpdate, (reload, finish).chain());
}

fn start(install: PathBuf, legend: String, heirloom: Option<&'static Heirloom>) -> Loading {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let t = std::time::Instant::now();
        let r = load(&install, &legend, heirloom);
        if r.is_ok() {
            info!("game files read in {:.1} s", t.elapsed().as_secs_f32());
        }
        let _ = tx.send(r);
    });
    Loading(Mutex::new(rx))
}

/// Another heirloom picked (`crate::dev`), or another legend: the files are read again for it
/// once the reading under way ends.
fn reload(mut commands: Commands, cfg: Res<Config>, loading: Option<Res<Loading>>, mut asked: Local<Option<(String, Option<&'static Heirloom>)>>) {
    let asked = asked.get_or_insert_with(|| (cfg.legend.clone(), cfg.heirloom));
    if loading.is_some() || (asked.0 == cfg.legend && asked.1 == cfg.heirloom) {
        return;
    }
    *asked = (cfg.legend.clone(), cfg.heirloom);
    if let Some(install) = cfg.r5r.clone() {
        commands.insert_resource(start(install, cfg.legend.clone(), cfg.heirloom));
    }
}

fn finish(mut commands: Commands, loading: Option<Res<Loading>>) {
    let Some(loading) = loading else { return };
    let Ok(result) = loading.0.lock().unwrap().try_recv() else { return };
    commands.remove_resource::<Loading>();
    match result {
        Ok(d) => commands.insert_resource(Data(Arc::new(d))),
        Err(e) => error!("reading the game's files: {e}"),
    }
}

pub fn load(install: &Path, legend: &str, heirloom: Option<&Heirloom>) -> Result<GameData, String> {
    let inst = Install::open(install).map_err(|e| e.to_string())?;
    if !inst.is_oracle_build() {
        warn!("r5apex.exe is build {:08x}-{:08x}, not the oracle's 5dba2a2f-02ae8471: names and settings may differ", inst.build.0, inst.build.1);
    }
    let l = lurch::assets::loadout(install, legend, heirloom)?;
    let wb = l.weapon.studio.bones();
    let mut defaults = Pose::new(wb.len());
    for (i, b) in wb.iter().enumerate() {
        (defaults.pos[i], defaults.q[i], defaults.scale[i], defaults.rot[i]) = (b.pos, b.quat, b.scale, b.rot);
    }
    let ab = l.arms.studio.bones();
    let merge = ab.iter().map(|b| wb.iter().position(|w| w.name.eq_ignore_ascii_case(&b.name))).collect();
    let mut arms_defaults = Pose::new(ab.len());
    for (i, b) in ab.iter().enumerate() {
        (arms_defaults.pos[i], arms_defaults.q[i], arms_defaults.scale[i], arms_defaults.rot[i]) = (b.pos, b.quat, b.scale, b.rot);
    }
    let attachments = l.arms.studio.attachments();
    let attachment = |name: &str| attachments.iter().find(|a| a.name.eq_ignore_ascii_case(name)).map(|a| (a.bone as usize, a.local));
    let (reference, camera, camera_base) = (attachment("ref"), attachment("camera"), attachment("camera_base"));
    let grapple_rope = attachment("grapple_rope");
    let grapple = lurch::assets::grapple(install).inspect_err(|e| warn!("the grapple's hook and rope: {e}")).ok();
    Ok(GameData {
        install: install.to_path_buf(),
        sounds: PlayerSounds::from_settings(&l.legend),
        weapon_class: heirloom.map_or(lurch::assets::FISTS, |h| h.weapon),
        times: WeaponTimes::from_script(|k| l.weapon_script.get(k)?.parse().ok()),
        melee_anim: melee_anim(install, heirloom),
        legend: l.legend,
        character: l.character,
        anims: ModelAnims::new(&l.weapon),
        arms_anims: ModelAnims::new(&l.arms),
        defaults,
        arms: Arms { bones: ab, merge, meshes: l.arms.meshes, textures: l.arms_textures, constants: l.arms_constants, defaults: arms_defaults, reference, camera, camera_base, grapple_rope },
        grapple,
        weapon_bones: wb,
        weapon_meshes: l.weapon.meshes,
        weapon_textures: l.weapon_textures,
        weapon_constants: l.weapon_constants,
    })
}

fn melee_anim(install: &Path, heirloom: Option<&Heirloom>) -> i32 {
    let name = heirloom.map_or("melee_pilot_emptyhanded".to_string(), |h| format!("melee_{}", h.name));
    let script = lurch::assets::script::weapon(install, &name).unwrap_or_default();
    script.get("melee_anim_1p_number").and_then(|n| n.parse().ok()).unwrap_or(1)
}

/// One mesh of a test map prop, placed in the map.
pub struct PropMesh {
    pub positions: Vec<V3>,
    pub normals: Vec<V3>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[u8; 4]>,
    pub indices: Vec<u16>,
    pub albedo: Option<Texture>,
}

/// The test map's props. Their drawn triangles are also their collision, not the models' own
/// collision.
#[derive(Resource)]
pub struct Props(pub Vec<PropMesh>);

impl Props {
    pub fn tris(&self) -> impl Iterator<Item = Tri> + '_ {
        self.0.iter().flat_map(|m| m.indices.as_chunks::<3>().0.iter().map(|t| t.map(|i| m.positions[i as usize])))
    }
}

pub fn load_props(install: &Path, props: &[Prop]) -> Result<Props, String> {
    let inst = Install::open(install).map_err(|e| e.to_string())?;
    let pak = inst.pak(PROP_PAK).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for p in props {
        let m = amodel::load(&pak, install, p.model).map_err(|e| e.to_string())?;
        for (mesh, textures) in m.meshes.iter().zip(lurch::assets::mesh_textures(&pak, &pak, install, &m)) {
            out.push(PropMesh {
                positions: mesh.positions.iter().map(|&v| p.place(v)).collect(),
                normals: mesh.normals.iter().map(|&n| p.turn(n)).collect(),
                uvs: mesh.uvs.clone(),
                colors: mesh.colors.clone(),
                indices: mesh.indices.clone(),
                albedo: textures.into_iter().next().flatten(),
            });
        }
    }
    Ok(Props(out))
}
