//! The game's own RUI elements drawn over the view (`lurch::rui`, `docs/ui.md`): the install's
//! elements run each frame with arguments from the simulation, their quads drawn through the
//! game's UI shader (`rui.wgsl`) by the viewmodel's camera, after the arms: the quads are in
//! the target's pixels, so any camera can draw them, and a camera of their own would cost a
//! whole view each frame. Without a viewmodel this module adds an orthographic camera. Read on
//! a thread at start, like the rest of the game's files (`data.rs`). Without an install the
//! HUD keeps its own stand-ins (`hud.rs`).
//!
//! The grapple's clip (`cooldown.rs`) is what its crosshair's `ammoFrac` and the tactical
//! panel's `chargeFrac` show.
//!
//! The special image source draws from the screen blur (`blur.rs`), sized here each frame to
//! the target.
//!
//! The menu on top (`lurch::vgui::Menu`, `MenuOpen`) draws over the HUD: its RUI
//! through the same batches, its VGUI images through `vgui.wgsl`, both in its paint order.
//! Not checked against the game: the images blend in linear colour, as Bevy's target does, and
//! the menu's globals take real time, which runs while the game is paused. The globals' clock
//! (`+0x90`) is real time since the shell started; the game's counts from boot.

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::camera::{RenderTarget, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology, VertexFormat};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, RenderPipelineDescriptor, SpecializedMeshPipelineError, TextureDimension, TextureFormat};
use bevy::render::storage::ShaderBuffer;
use bevy::shader::ShaderRef;
use bevy::window::PrimaryWindow;
use lurch::assets::texture::Texture;
use lurch::rui::draw::{Draws, Quad, SPECIAL_ATLAS};
use lurch::rui::{Frame as RuiFrame, Globals, Rui, Topology, Ui, Value, RUI_DRAW_HUD, RUI_DRAW_POSTEFFECTS};
use lurch::vgui::settings::{self, SettingsMenu};
use lurch::assets::keyvalues::atof;
use lurch::vgui::slider::ConVarValue;
use lurch::vgui::{Menu, Paint, Screen};

use crate::blur::{BlurPlugin, BlurSource, ScreenBlur};
use crate::cooldown;
use crate::data::Data;
use crate::hud::GrappleRange;
use crate::sim::{Map, Pose, Sim};
use crate::view::ViewImage;
use crate::binds::{button_code, Input};
use crate::{common, Config, Frame, GameMenu, HudVisible, KeyTrap, MenuCursor, MenuOpen, MenuSound, Paused};

pub const UI_LAYER: usize = 3;
/// `HUD_Z_BASE` (`sh_consts.gnut`).
const HUD_Z_BASE: i16 = 300;
const GRAPPLE: &str = "mp_ability_grapple";
/// Pathfinder's ultimate, whose panel the Minecraft host shows uncharged.
const ULTIMATE: &str = "mp_weapon_zipline";
/// The space between the host's HUD and the game's panels beside it, over the target's height.
const HOST_GAP: f32 = 0.02;
const SHADER: &str = "embedded://lurch_game/rui.wgsl";
const VGUI_SHADER: &str = "embedded://lurch_game/vgui.wgsl";
const SYSTEM_MENU: &str = "scripts/resource/ui/menus/system.menu";

const ATTRIBUTE_UV: MeshVertexAttribute = MeshVertexAttribute::new("RuiUv", 0x5255_4901, VertexFormat::Float32x4);
const ATTRIBUTE_TEXT: MeshVertexAttribute = MeshVertexAttribute::new("RuiText", 0x5255_4902, VertexFormat::Float32x2);
const ATTRIBUTE_DATA: MeshVertexAttribute = MeshVertexAttribute::new("RuiData", 0x5255_4903, VertexFormat::Sint32x4);
const ATTRIBUTE_BOUNDS: MeshVertexAttribute = MeshVertexAttribute::new("RuiBounds", 0x5255_4904, VertexFormat::Float32x4);
const ATTRIBUTE_MASK_BOUNDS: MeshVertexAttribute = MeshVertexAttribute::new("RuiMaskBounds", 0x5255_4905, VertexFormat::Float32x4);
const ATTRIBUTE_PIXEL: MeshVertexAttribute = MeshVertexAttribute::new("RuiPixel", 0x5255_4906, VertexFormat::Float32x2);

/// The game's UI material: an atlas (or a font's), a mask atlas, this frame's styles
/// (`UiShaderStyle_s`, `rui.wgsl`) and a font's glyph bounds (`g_fontBounds`).
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct RuiMaterial {
    #[texture(0)]
    #[sampler(1)]
    image: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    mask: Handle<Image>,
    #[storage(4, read_only)]
    styles: Handle<ShaderBuffer>,
    #[storage(5, read_only)]
    glyphs: Handle<ShaderBuffer>,
    /// The run's place in draw order. Each run's mesh has its centre at the origin, so in the
    /// transparent pass this alone orders them.
    order: f32,
}

impl Material for RuiMaterial {
    fn vertex_shader() -> ShaderRef {
        SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    fn depth_bias(&self) -> f32 {
        self.order
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, layout: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        let attributes = [
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            ATTRIBUTE_UV.at_shader_location(1),
            ATTRIBUTE_TEXT.at_shader_location(2),
            ATTRIBUTE_DATA.at_shader_location(3),
            ATTRIBUTE_BOUNDS.at_shader_location(4),
            ATTRIBUTE_MASK_BOUNDS.at_shader_location(5),
            ATTRIBUTE_PIXEL.at_shader_location(6),
        ];
        descriptor.vertex.buffers = vec![layout.0.get_layout(&attributes)?];
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(false);
            depth.depth_compare = Some(bevy::render::render_resource::CompareFunction::Always);
        }
        Ok(())
    }
}

/// A VGUI image (`vgui.wgsl`): its texture, the draw colour and the clip rectangle, ordered
/// among the RUI's runs as they are.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct VguiMaterial {
    #[texture(0)]
    #[sampler(1)]
    image: Handle<Image>,
    #[uniform(2)]
    color: Vec4,
    #[uniform(3)]
    clip: Vec4,
    order: f32,
}

impl Material for VguiMaterial {
    fn vertex_shader() -> ShaderRef {
        VGUI_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        VGUI_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn depth_bias(&self) -> f32 {
        self.order
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(_: &MaterialPipeline, descriptor: &mut RenderPipelineDescriptor, layout: &MeshVertexBufferLayoutRef, _: MaterialPipelineKey<Self>) -> Result<(), SpecializedMeshPipelineError> {
        let attributes = [Mesh::ATTRIBUTE_POSITION.at_shader_location(0), Mesh::ATTRIBUTE_UV_0.at_shader_location(1), ATTRIBUTE_PIXEL.at_shader_location(2)];
        descriptor.vertex.buffers = vec![layout.0.get_layout(&attributes)?];
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(false);
            depth.depth_compare = Some(bevy::render::render_resource::CompareFunction::Always);
        }
        Ok(())
    }
}

/// The camera that draws the game's UI when there is no viewmodel: the last over the view, so
/// it also hosts the shell's own Bevy UI (a later camera on the same target would resolve over
/// it).
#[derive(Component)]
pub struct RuiCamera;

#[derive(Resource)]
struct Loading(Mutex<Receiver<Result<Ui, String>>>);

/// A runtime read before the app starts (an offscreen tool), taken instead of reading one on
/// a thread.
#[derive(Resource)]
pub struct Preloaded(pub Mutex<Option<Ui>>);

/// What a run of quads is drawn with: its place in draw order, then text and a font atlas, or
/// an atlas and a mask atlas.
type BatchKey = (usize, bool, u8, i16);

/// One run of quads with the same material, drawn as one mesh in draw order.
struct Batch {
    entity: Entity,
    mesh: Handle<Mesh>,
    /// What the mesh holds, so an unchanged run is not uploaded again.
    quads: Vec<Quad>,
    size: (f32, f32),
}

/// One of a weapon's crosshairs (`RUI_CrosshairData`): its instance and the value each of its
/// arguments is bound to.
struct Crosshair {
    rui: Rui,
    args: Vec<(String, String)>,
}

/// The crosshairs of the weapons the HUD shows: the fists, in use, and the grapple, an
/// offhand. Which of a weapon's crosshairs show is read from the setting names: while in use,
/// `active_crosshair_count` from `rui_crosshair_index`; an offhand's from
/// `ordnance_crosshair_always_on_start_index` on, always. The engine's own choice is not
/// ported.
fn crosshairs(ui: &Ui, install: &std::path::Path, weapon: &str, in_use: bool) -> Vec<Crosshair> {
    let (Ok(script), Ok(data)) = (lurch::assets::script::weapon(install, weapon), lurch::assets::script::crosshairs(install, weapon)) else {
        warn!("{weapon}: no weapon script");
        return Vec::new();
    };
    let num = |k: &str, d: usize| script.get(k).and_then(|v| v.parse::<usize>().ok()).unwrap_or(d);
    let (first, count) = (num("rui_crosshair_index", 0), num("active_crosshair_count", data.crosshairs.len()));
    let always_from = script.get("ordnance_crosshair_always_on_start_index").and_then(|v| v.parse::<usize>().ok());
    data.crosshairs
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let shown = in_use && (first..first + count).contains(&i);
            let always = always_from.is_some_and(|k| i >= k && i < count);
            if !shown && !always {
                return None;
            }
            let name = c.ui.trim_start_matches("ui/").trim_end_matches(".rpak");
            let rui = ui.create(name, RUI_DRAW_HUD, 0).or_else(|| {
                warn!("{weapon}: no {name} in this install's UI");
                None
            })?;
            let mut args = data.default_args.clone();
            args.extend(c.args.iter().cloned());
            Some(Crosshair { rui, args })
        })
        .collect()
}

/// The player's panel (`SURVIVAL_HUD_PLAYER`) as `SURVIVAL_PopulatePlayerInfoRui`
/// (`cl_gamemode_survival.nut`) sets it up, at game time `now`, with the legend's portrait.
/// `name` is any text: the host draws over the name and the bars, and the name's first glyph
/// is where they start (`player_panel`).
fn player_panel(ui: &Ui, data: Option<&Data>, now: f32) -> Option<Rui> {
    let mut rui = ui.create("survival_player_hud_v2", RUI_DRAW_POSTEFFECTS, HUD_Z_BASE)?;
    rui.set_bool(ui, "useShields", true);
    rui.set_string(ui, "name", "lurch");
    rui.set_game_time(ui, "trackedPlayerChangeTime", now);
    for name in ["playerHealthFrac", "playerTargetHealthFrac"] {
        rui.set_float(ui, name, 1.0);
    }
    if let Some(icon) = data.and_then(|d| d.0.character.as_ref()).and_then(|c| c.str("galleryPortrait")) {
        rui.set_image(ui, "playerIcon", icon);
    }
    Some(rui)
}

/// The tactical's panel for the grapple, as `ClWeaponStatus_AddClient` creates it, at game time
/// `now`.
fn tactical(ui: &Ui, install: &std::path::Path, now: f32) -> Option<Rui> {
    let w = lurch::assets::script::weapon(install, GRAPPLE).ok()?;
    offhand(ui, &w, now, "tactical_ability_hud", [cooldown::CLIP_SIZE, cooldown::AMMO_PER_SHOT, cooldown::AMMO_MIN_TO_FIRE], "%offhand1%")
}

/// The ultimate's panel for Pathfinder's zipline as `ClWeaponStatus_AddClient` creates it
/// (`ability_hud`, without the playlist's `hud_ultimate_ready_video`) on `OFFHAND_INVENTORY`,
/// at game time `now`, uncharged: no ultimate is modelled.
fn ultimate(ui: &Ui, install: &std::path::Path, now: f32) -> Option<Rui> {
    let w = lurch::assets::script::weapon(install, ULTIMATE).ok()?;
    let ammo = |k: &str| w.get(k).and_then(|v| v.parse().ok());
    let mut rui = offhand(ui, &w, now, "ability_hud", [ammo("ammo_clip_size")?, ammo("ammo_per_shot")?, ammo("ammo_min_to_fire")?], "%offhand4%")?;
    rui.set_int(ui, "ultimateCredits", -1);
    rui.set_float(ui, "chargeFrac", 0.0);
    rui.set_float(ui, "refillRate", 0.0);
    Some(rui)
}

/// An offhand's panel as `InitOffhandRui` (`cl_weapon_status.gnut`) sets it up for a
/// `cooldown_type ammo` weapon with settings `w` and `[clip, per shot, min to fire]` ammo, with
/// the hint `UpdateOffhandRuiVisibility` gives its slot.
fn offhand(ui: &Ui, w: &HashMap<String, String>, now: f32, element: &str, [clip, per_shot, min]: [i32; 3], hint: &str) -> Option<Rui> {
    let mut rui = ui.create(element, RUI_DRAW_POSTEFFECTS, HUD_Z_BASE)?;
    rui.set_game_time(ui, "hintTime", now);
    for name in ["isTitan", "isReverseCharge", "isPaused"] {
        rui.set_bool(ui, name, false);
    }
    let floats = [("useFrac", 0.0), ("chargeMaxFrac", 1.0), ("minFireFrac", min as f32 / clip as f32), ("readyFrac", 0.0), ("dryfireFrac", 0.0), ("chargeFracCaution", 0.0), ("chargeFracAlert", 0.0), ("chargeFracAlertSpeed", 16.0), ("chargeFracAlertScale", 1.0), ("bleedoutEndTime", 0.0), ("reviveEndTime", 0.0)];
    for (name, v) in floats {
        rui.set_float(ui, name, v);
    }
    rui.set_value(ui, "segments", Value::Int(clip / per_shot));
    rui.set_value(ui, "ammoMinToFire", Value::Int(min));
    rui.set_image(ui, "hudIcon", w.get("hud_icon").map_or("", String::as_str));
    rui.set_string(ui, "hintText", hint);
    Some(rui)
}

/// What an offhand's panel takes each frame besides its charge: the legend's ultimate colours
/// and the target's virtual screen.
fn offhand_frame(ui: &Ui, t: &mut Rui, data: Option<&Data>, width: f32, height: f32) {
    if let Some(c) = data.and_then(|d| d.0.character.as_ref()) {
        for (arg, key) in [("ultimateColor", "hudUltimateColor"), ("ultimateColorHighlight", "hudUltimateColorHighlight")] {
            if let Some(v) = c.float3(key).map(script_srgb_to_linear) {
                t.set_value(ui, arg, Value::Vector([v[0], v[1], v[2], 1.0]));
            }
        }
    }
    let [vw, vh] = lurch::rui::virtual_screen(width, height);
    (t.size, t.inv_size) = ([vw, vh], [1.0 / vw, 1.0 / vh]);
}

/// The script's `SrgbToLinear` (native `0x7a5d20`, each lane `0x7158c0`): the engine's curve
/// (`0x270130`), but the straight scale at the threshold as well as below it.
fn script_srgb_to_linear(c: [f32; 3]) -> [f32; 3] {
    let curve = lurch::rui::api::srgb_to_linear([c[0], c[1], c[2], 0.0]);
    std::array::from_fn(|i| if c[i] <= 0.040_45 { c[i] * 0.077_399_38 } else { curve[i] })
}

/// The value the engine binds a crosshair argument to (the names at `0x1477700`), from what
/// lurch has. None for the ones it does not model.
fn weapon_value(var: &str, sim: &Sim, range: &GrappleRange, grapple: bool) -> Option<Value> {
    let p = &sim.pilot.player;
    Some(match var {
        "player_is_sprinting" => Value::Bool(p.sprinting),
        "weapon_is_reloading" | "weapon_is_amped" | "weapon_is_disabled" | "weapon_is_firing" => Value::Bool(false),
        "weapon_spread" | "player_zoomFrac" | "crosshair_movement_x" | "crosshair_movement_y" | "weapon_script_time_0" | "eWeaponVar.custom_float_0" => Value::Float(0.0),
        "crosshair_team_color" => Value::Vector([1.0, 1.0, 1.0, 1.0]),
        "progress_weapon_clip_ammo_frac" => Value::Float(if grapple { sim.grapple_clip.fraction() } else { 1.0 }),
        "grapple_in_range" => Value::Bool(range.shown),
        "grapple_in_range_time" => Value::Float(range.in_time),
        "grapple_out_of_range_time" => Value::Float(range.out_time),
        _ => return None,
    })
}

/// Another game's HUD drawn over the shell's, for the shell drawn over that game. The shell
/// draws the player panel's frame and portrait and leaves its contents to the host, mirrors
/// the player panel with the ultimate's at the right edge, and leaves the middle of the
/// bottom to the host's own bar (`HostBar`), with the tactical's panel left of it. Written each
/// frame, in the target's pixels.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct HostHud {
    /// The box the player panel's name and bars fill in the game, as tall as the panel:
    /// left, top, right, bottom.
    pub panel: Option<[f32; 4]>,
    /// How far left the host's bar may reach and leave room for the tactical's panel between
    /// it and the player panel.
    pub room: Option<f32>,
}

/// The left edge of the host's bar in the middle of the bottom, in the target's pixels: the
/// tactical's panel moves left of it when it would reach it.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct HostBar(pub f32);

fn shift_x(quads: &mut [Quad], dx: f32) {
    for c in quads.iter_mut().flat_map(|q| q.corners.iter_mut()) {
        c[0] += dx;
    }
}

/// The bounds of the quads' corners, `[x0, y0, x1, y1]` in the quads' own units.
fn bounds(quads: &[Quad]) -> Option<[f32; 4]> {
    let mut b = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
    for c in quads.iter().flat_map(|q| q.corners) {
        b = [b[0].min(c[0]), b[1].min(c[1]), b[2].max(c[0]), b[3].max(c[1])];
    }
    (b[0] <= b[2]).then_some(b)
}

/// The runtime and the instances the HUD shows.
#[derive(Resource)]
pub struct Runtime {
    pub ui: Arc<Ui>,
    frame: RuiFrame,
    /// The fists' crosshairs, then the grapple's.
    crosshairs: Vec<(Crosshair, bool)>,
    tactical: Option<Rui>,
    /// The player's panel and the ultimate's, with a `HostHud` only.
    player: Option<Rui>,
    ultimate: Option<Rui>,
    atlases: Vec<Option<Handle<Image>>>,
    /// Each font atlas's texture and glyph bounds.
    fonts: Vec<(Option<Handle<Image>>, Handle<ShaderBuffer>)>,
    white: Handle<Image>,
    no_glyphs: Handle<ShaderBuffer>,
    /// This frame's styles, in a buffer that keeps its size: a buffer that changes size is
    /// made anew on the GPU, and the materials' bind groups would keep the old one. It grows
    /// as a new buffer, with new materials.
    styles: Handle<ShaderBuffer>,
    styles_size: usize,
    styles_bytes: Vec<u8>,
    materials: HashMap<BatchKey, Handle<RuiMaterial>>,
    batches: Vec<Batch>,
    /// What the special image source draws from, made for this backbuffer's size.
    blur: Option<ScreenBlur>,
    reported: std::collections::HashSet<String>,
    /// The in-match menu, the settings menu, and the textures their images name.
    system_menu: Option<Menu>,
    misc_menu: Option<SettingsMenu>,
    /// The menu on top last frame, to run a menu's open and close handlers.
    last_menu: Option<GameMenu>,
    menu_wheel: crate::binds::Wheel,
    vgui_textures: HashMap<String, Handle<Image>>,
    images: Vec<(Entity, Handle<Mesh>, Handle<VguiMaterial>)>,
}

pub struct RuiPlugin;

impl Plugin for RuiPlugin {
    fn build(&self, app: &mut App) {
        common(app);
        bevy::asset::embedded_asset!(app, "rui.wgsl");
        bevy::asset::embedded_asset!(app, "vgui.wgsl");
        let cfg = app.world().resource::<Config>().clone();
        let (tx, rx) = channel();
        let preloaded = app.world_mut().remove_resource::<Preloaded>().and_then(|p| p.0.into_inner().unwrap());
        if let Some(ui) = preloaded {
            let _ = tx.send(Ok(ui));
        } else {
            let Some(install) = cfg.r5r.clone() else { return };
            std::thread::spawn(move || {
                let t = std::time::Instant::now();
                let r = Ui::open(&install).map_err(|e| e.to_string());
                if r.is_ok() {
                    info!("the game's UI read in {:.1} s", t.elapsed().as_secs_f32());
                }
                let _ = tx.send(r);
            });
        }
        app.add_plugins((MaterialPlugin::<RuiMaterial>::default(), MaterialPlugin::<VguiMaterial>::default(), BlurPlugin))
            .insert_resource(Loading(Mutex::new(rx)))
            .add_systems(PreUpdate, finish)
            .add_systems(Update, (draw.run_if(resource_exists::<Runtime>).after(crate::hud::GrappleRangeSet), own_camera.run_if(resource_exists::<Runtime>), host_bevy_ui).in_set(Frame::View));
        if !cfg.scripted {
            app.add_systems(Update, menu_cursor.in_set(Frame::Make));
        }
    }
}

/// A UI atlas as the GPU takes it: its blocks as they are, clamped at the edges.
fn atlas_image(t: &Texture) -> Option<Image> {
    let mut image = crate::viewmodel::gpu_image(t)?;
    let clamp = ImageAddressMode::ClampToEdge;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: clamp, address_mode_v: clamp, ..ImageSamplerDescriptor::linear() });
    Some(image)
}

#[allow(clippy::too_many_arguments)] // Bevy's system parameters
fn finish(mut commands: Commands, loading: Option<Res<Loading>>, cfg: Res<Config>, sim: Option<Res<Sim>>, data: Option<Res<Data>>, host: Option<Res<HostHud>>, mut images: ResMut<Assets<Image>>, mut buffers: ResMut<Assets<ShaderBuffer>>) {
    let Some(loading) = loading else { return };
    let Ok(result) = loading.0.lock().unwrap().try_recv() else { return };
    commands.remove_resource::<Loading>();
    let ui = match result {
        Ok(ui) => ui,
        Err(e) => {
            error!("reading the game's UI: {e}");
            return;
        }
    };
    let atlases = ui
        .lib
        .textures
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let h = t.as_ref().and_then(atlas_image).map(|img| images.add(img));
            if h.is_none() {
                warn!("UI atlas {i}: texture not loaded");
            }
            h
        })
        .collect();
    let white = images.add(Image::new(Extent3d { width: 1, height: 1, depth_or_array_layers: 1 }, TextureDimension::D2, vec![255; 4], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD));
    let styles_size = 160 * 64;
    let styles = buffers.add(ShaderBuffer::new(&vec![0; styles_size], RenderAssetUsages::default()));
    let no_glyphs = buffers.add(ShaderBuffer::new(&[0; 16], RenderAssetUsages::default()));
    let fonts = ui
        .lib
        .fonts
        .iter()
        .zip(&ui.lib.font_textures)
        .enumerate()
        .map(|(i, (font, t))| {
            let image = t.as_ref().and_then(atlas_image).map(|img| images.add(img));
            if image.is_none() {
                warn!("UI font {i}: texture not loaded");
            }
            let bytes: Vec<u8> = font.bounds().iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
            (image, buffers.add(ShaderBuffer::new(if bytes.is_empty() { &[0; 16] } else { &bytes }, RenderAssetUsages::default())))
        })
        .collect();
    let mut crosshair_list = Vec::new();
    let mut tactical_hud = None;
    if let Some(install) = cfg.r5r.as_deref() {
        crosshair_list.extend(crosshairs(&ui, install, lurch::assets::FISTS, true).into_iter().map(|c| (c, false)));
        crosshair_list.extend(crosshairs(&ui, install, GRAPPLE, false).into_iter().map(|c| (c, true)));
        tactical_hud = tactical(&ui, install, sim.as_ref().map_or(0.0, |s| s.view_now));
        if tactical_hud.is_none() {
            warn!("no tactical_ability_hud in this install's UI");
        }
    }
    let now = sim.as_ref().map_or(0.0, |s| s.view_now);
    let (player, ultimate) = match (host.is_some(), cfg.r5r.as_deref()) {
        (true, Some(install)) => (player_panel(&ui, data.as_deref(), now), ultimate(&ui, install, now)),
        _ => (None, None),
    };
    let (system_menu, misc_menu, vgui_textures) = cfg.r5r.as_deref().map(|install| menus(&ui, install, &mut images)).unwrap_or_default();
    let mut ui = ui;
    let localized = |token: &str| String::from_utf16_lossy(ui.localized(token.as_bytes()).unwrap_or(&[])).trim_end_matches('\0').to_string();
    let (press, hold) = (localized("INPUT_PRESS"), localized("INPUT_HOLD"));
    ui.tokens = lurch::rui::tokens::Tokens::new(&ui.lib.keys, &cfg.binds.game_binds(), &press, &hold);
    commands.insert_resource(Runtime {
        ui: Arc::new(ui),
        frame: RuiFrame::default(),
        crosshairs: crosshair_list,
        tactical: tactical_hud,
        player,
        ultimate,
        atlases,
        fonts,
        white,
        no_glyphs,
        styles,
        styles_size,
        styles_bytes: Vec::new(),
        materials: HashMap::new(),
        batches: Vec::new(),
        blur: None,
        reported: Default::default(),
        system_menu,
        misc_menu,
        last_menu: None,
        menu_wheel: Default::default(),
        vgui_textures,
        images: Vec::new(),
    });
}

/// `SystemMenu` and `MiscMenu` from the install's layouts, and the textures of their images
/// (`texture/NAME.rpak` in `common_early.rpak`), each clamped at its edges.
fn menus(ui: &Ui, install: &std::path::Path, images: &mut Assets<Image>) -> (Option<Menu>, Option<SettingsMenu>, HashMap<String, Handle<Image>>) {
    let platform = install.join("platform");
    let scheme = match lurch::assets::vgui::Scheme::load(&platform) {
        Ok(s) => s,
        Err(e) => {
            warn!("the menus' scheme: {e}");
            return Default::default();
        }
    };
    let load = |file: &str| lurch::assets::vgui::load(&platform, &scheme, file).map_err(|e| warn!("{file}: {e}")).ok();
    let system = load(SYSTEM_MENU);
    let misc = SettingsMenu::load(ui, install, &scheme).map_err(|e| warn!("{}: {e}", settings::MENU)).ok();
    let mut names = Vec::new();
    fn walk(p: &lurch::assets::vgui::Panel, names: &mut Vec<String>) {
        if p.class == "ImagePanel" {
            names.extend(p.settings.str("image").filter(|n| !n.is_empty()).map(|n| format!("texture/{n}.rpak")));
        }
        p.children.iter().for_each(|c| walk(c, names));
    }
    system.iter().flatten().chain(misc.iter().flat_map(|m| &m.menu.panels)).for_each(|p| walk(p, &mut names));
    let mut textures = HashMap::new();
    match lurch::assets::Install::open(install).and_then(|inst| inst.pak("common_early.rpak")) {
        Ok(pak) => {
            for name in names {
                match lurch::assets::texture::load(&pak, install, lurch::assets::rpak::guid(&name)) {
                    Ok(t) => {
                        if let Some(img) = atlas_image(&t) {
                            textures.insert(name, images.add(img));
                        }
                    }
                    Err(e) => warn!("{name}: {e}"),
                }
            }
        }
        Err(e) => warn!("common_early.rpak: {e}"),
    }
    (system.map(|p| lurch::vgui::system_menu(ui, p)), misc, textures)
}

/// The window's cursor in the target's pixels, for the menus.
fn menu_cursor(windows: Query<&Window, With<PrimaryWindow>>, view: Option<Res<ViewImage>>, images: Res<Assets<Image>>, mut cursor: ResMut<MenuCursor>) {
    let Ok(w) = windows.single() else { return };
    let target = view.and_then(|v| images.get(&v.0)).map_or(Vec2::new(w.width(), w.height()), |i| Vec2::new(i.width() as f32, i.height() as f32));
    cursor.0 = w.cursor_position().map(|p| p / Vec2::new(w.width(), w.height()) * target).map(|p| (p.x, p.y));
}

/// A VGUI image's rect as a mesh in the target's pixels, its texture across it.
fn image_mesh((x0, y0, x1, y1): (i32, i32, i32, i32)) -> Mesh {
    let (x0, y0, x1, y1) = (x0 as f32, y0 as f32, x1 as f32, y1 as f32);
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]])
        .with_inserted_attribute(ATTRIBUTE_PIXEL, vec![[x0, y0], [x0, y1], [x1, y1], [x1, y0]])
        .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}

/// A VGUI colour's bytes as the surface's draw colour: the colour taken from sRGB, alpha as it
/// is (not checked against the game's surface).
fn vgui_color(c: [u8; 4]) -> Vec4 {
    let l = script_srgb_to_linear([c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]);
    Vec4::new(l[0], l[1], l[2], c[3] as f32 / 255.0)
}

/// A camera for the UI once it is clear there is no viewmodel's: the game's files are read
/// and gave none.
fn own_camera(mut commands: Commands, view: Option<Res<ViewImage>>, data: Option<Res<Data>>, pending: Option<Res<crate::data::Loading>>, ours: Query<(), With<RuiCamera>>, viewmodel: Query<(), With<crate::viewmodel::ViewmodelCamera>>) {
    if data.is_some() || pending.is_some() || !ours.is_empty() || !viewmodel.is_empty() {
        return;
    }
    let mut camera = commands.spawn((
        Camera3d::default(),
        Camera { order: 2, clear_color: ClearColorConfig::None, ..default() },
        Projection::Orthographic(OrthographicProjection { scaling_mode: ScalingMode::WindowSize, near: -10.0, far: 10.0, ..OrthographicProjection::default_3d() }),
        Tonemapping::None,
        RuiCamera,
        BlurSource,
        RenderLayers::layer(UI_LAYER),
    ));
    if let Some(view) = view {
        camera.insert(RenderTarget::Image(view.0.clone().into()));
    }
}

fn host_bevy_ui(mut commands: Commands, ours: Query<(Entity, Has<IsDefaultUiCamera>), With<RuiCamera>>, others: Query<Entity, (With<IsDefaultUiCamera>, Without<RuiCamera>)>) {
    let Ok((ours, hosting)) = ours.single() else { return };
    for e in &others {
        commands.entity(e).remove::<IsDefaultUiCamera>();
    }
    if !hosting {
        commands.entity(ours).insert(IsDefaultUiCamera);
    }
}

/// A run of quads as one mesh, in the target's pixels, with what the UI shader reads at each
/// corner; a degenerate triangle when empty, as an empty mesh upsets the renderer's allocator.
/// Its positions are all at the origin, so every run sorts by its material's order alone.
fn quads_mesh(ui: &Ui, quads: &[Quad], topology: &Topology, special: [f32; 4]) -> Mesh {
    let n = quads.len().max(1) * 4;
    let mut pos = Vec::with_capacity(n);
    let mut uv = Vec::with_capacity(n);
    let mut text = Vec::with_capacity(n);
    let mut data = Vec::with_capacity(n);
    let mut bounds = Vec::with_capacity(n);
    let mut mask_bounds = Vec::with_capacity(n);
    let mut idx = Vec::new();
    let rect = |atlas: u8, image: i32| -> [f32; 4] {
        if atlas == SPECIAL_ATLAS {
            return special;
        }
        ui.lib.atlases.get(atlas as usize).and_then(|a| a.uv.get(image as usize)).copied().unwrap_or([0.0; 4])
    };
    for q in quads {
        let base = pos.len() as u32;
        let is_text = q.flags >> 12 == 0;
        let (b, m) = if is_text { ([0.0; 4], [0.0; 4]) } else { (rect(q.atlas, q.image), if q.mask >= 0 { rect(q.mask_atlas, q.mask) } else { [0.0; 4] }) };
        for k in 0..4 {
            let p = topology.place(q.corners[k][0], q.corners[k][1]);
            pos.push([p[0], p[1]]);
            uv.push(q.uv[k]);
            text.push(q.text[k]);
            data.push([q.image, q.mask, q.style as i32, q.flags as i32]);
            bounds.push(b);
            mask_bounds.push(m);
        }
        idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    if quads.is_empty() {
        pos = vec![[0.0; 2]; 3];
        uv = vec![[0.0; 4]; 3];
        text = vec![[0.0; 2]; 3];
        data = vec![[0; 4]; 3];
        bounds = vec![[0.0; 4]; 3];
        mask_bounds = vec![[0.0; 4]; 3];
        idx = vec![0, 1, 2];
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; pos.len()])
        .with_inserted_attribute(ATTRIBUTE_PIXEL, pos)
        .with_inserted_attribute(ATTRIBUTE_UV, uv)
        .with_inserted_attribute(ATTRIBUTE_TEXT, text)
        .with_inserted_attribute(ATTRIBUTE_DATA, data)
        .with_inserted_attribute(ATTRIBUTE_BOUNDS, bounds)
        .with_inserted_attribute(ATTRIBUTE_MASK_BOUNDS, mask_bounds)
        .with_inserted_indices(Indices::U32(idx))
}

/// The use prompt on a zipline (`0x99eab0`): `zipline_flyout`, made anew each frame the use
/// search finds a zipline, pointing at the point it found. It draws in draw group 9 on the
/// screen in pixels with the HUD's globals, their view set from the camera.
#[allow(clippy::too_many_arguments)]
fn flyout(ui: &Ui, sim: &Sim, map: &mut Map, pose: &Pose, globals: &Globals, hud: &Topology, (width, height): (f32, f32), frame: &mut RuiFrame, draws: &mut Draws, reported: &mut std::collections::HashSet<String>) {
    let Some(point) = sim.pilot.use_point(&sim.settings, sim.view_now, &mut map.0) else { return };
    let Some(mut rui) = ui.create("zipline_flyout", 9, 0) else { return };
    let eye = pose.0.origin;
    let (fwd, right, up) = lurch::sim::math::angle_vectors(pose.0.angles);
    let row = |v: [f32; 3], s: f32| [s * v[0], s * v[1], s * v[2], -s * (v[0] * eye[0] + v[1] * eye[1] + v[2] * eye[2])];
    let mut g = globals.clone();
    g.set_view([row(right, 1.0), row(up, 1.0), row(fwd, -1.0)], (crate::convert::vfov(pose.0.fov) * 0.5).tan() * (width / height));
    rui.set_value(ui, "usePos", Value::Vector([point[0], point[1], point[2], 0.0]));
    let screen = Topology { org: [0.0; 3], right: [width, 0.0, 0.0], down: [0.0, height, 0.0] };
    let first = draws.quads.len();
    match ui.update(&mut rui, &g, &screen, frame) {
        Ok(true) => ui.draws(&rui, frame, draws),
        Ok(false) => {}
        Err(stop) => {
            if reported.insert(format!("{}: {stop:?}", rui.name)) {
                warn!("{}: {stop:?}", rui.name);
            }
        }
    }
    // The shell places every quad through the HUD's plane.
    for q in &mut draws.quads[first..] {
        for c in &mut q.corners {
            let p = screen.place(c[0], c[1]);
            *c = [(p[0] - hud.org[0]) / hud.right[0], (p[1] - hud.org[1]) / hud.down[1]];
        }
    }
}

/// The batches' visibility and what the menus need, as one of `draw`'s parameters: a system
/// takes at most sixteen.
type DrawRest<'w, 's> = (
    Query<'w, 's, (&'static mut Visibility, &'static mut MeshMaterial3d<RuiMaterial>)>,
    Query<'w, 's, &'static mut Visibility, (With<MeshMaterial3d<VguiMaterial>>, Without<MeshMaterial3d<RuiMaterial>>)>,
    ResMut<'w, Assets<VguiMaterial>>,
    ResMut<'w, MenuOpen>,
    ResMut<'w, Paused>,
    Res<'w, Time<Real>>,
    Res<'w, ButtonInput<MouseButton>>,
    Res<'w, MenuCursor>,
    Option<ResMut<'w, crate::convar::ConVars>>,
    MessageReader<'w, 's, bevy::input::mouse::MouseWheel>,
    MessageWriter<'w, MenuSound>,
    Res<'w, ButtonInput<KeyCode>>,
    ResMut<'w, KeyTrap>,
    (Option<ResMut<'w, HostHud>>, Option<Res<'w, HostBar>>),
);

#[allow(clippy::too_many_arguments)] // Bevy's system parameters
fn draw(
    mut commands: Commands,
    mut rt: ResMut<Runtime>,
    sim: Res<Sim>,
    mut map: ResMut<Map>,
    pose: Res<Pose>,
    data: Option<Res<Data>>,
    range: Res<GrappleRange>,
    visible: Res<HudVisible>,
    windows: Query<&Window, With<PrimaryWindow>>,
    view: Option<Res<ViewImage>>,
    mut images: ResMut<Assets<Image>>,
    cfg: Res<Config>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<RuiMaterial>>,
    mut buffers: ResMut<Assets<ShaderBuffer>>,
    (mut batches, mut shown, mut vgui_materials, mut menu_open, mut paused, real, mouse, cursor, mut convars, mut wheel, mut sounds, keyboard, mut trap, (mut host, host_bar)): DrawRest,
) {
    let target = view.and_then(|v| images.get(&v.0)).map(|i| (i.width() as f32, i.height() as f32));
    let (width, height) = target.or_else(|| windows.single().ok().map(|w| (w.width(), w.height()))).unwrap_or((cfg.size.0 as f32, cfg.size.1 as f32));
    let now = sim.view_now;
    let mut globals = Globals::new(now, width, height);
    globals.set_clock(real.elapsed().as_micros() as u64);
    let topology = Topology::hud(width, height);
    let rt = &mut *rt;
    // The token table is rebuilt when a bind changes (`0x23f8b0`).
    if cfg.is_changed()
        && let Some(ui) = Arc::get_mut(&mut rt.ui)
    {
        let localized = |token: &str| String::from_utf16_lossy(ui.localized(token.as_bytes()).unwrap_or(&[])).trim_end_matches('\0').to_string();
        let (press, hold) = (localized("INPUT_PRESS"), localized("INPUT_HOLD"));
        ui.tokens = lurch::rui::tokens::Tokens::new(&ui.lib.keys, &cfg.binds.game_binds(), &press, &hold);
    }
    let backbuffer = [width as u32, height as u32];
    if rt.blur.as_ref().is_none_or(|b| b.backbuffer != backbuffer) {
        let blur = ScreenBlur::new(&mut images, backbuffer);
        commands.insert_resource(blur.clone());
        rt.blur = Some(blur);
        rt.materials.clear();
    }
    rt.frame.screen_blur = Some(backbuffer.map(|x| x as i32));
    let ui = rt.ui.clone();
    let mut draws = Draws::default();
    for (c, grapple) in rt.crosshairs.iter_mut().filter(|_| visible.0) {
        for (arg, var) in &c.args {
            match weapon_value(var, &sim, &range, *grapple) {
                Some(v) => {
                    c.rui.set_value(&ui, arg, v);
                }
                None => {
                    if rt.reported.insert(format!("value {var}")) {
                        warn!("{}: no value for {arg} ({var})", c.rui.name);
                    }
                }
            }
        }
        match ui.update(&mut c.rui, &globals, &topology, &mut rt.frame) {
            Ok(true) => ui.draws(&c.rui, &rt.frame, &mut draws),
            Ok(false) => {}
            Err(stop) => {
                if rt.reported.insert(format!("{}: {stop:?}", c.rui.name)) {
                    warn!("{}: {stop:?}", c.rui.name);
                }
            }
        }
    }
    let mut tactical_quads = None;
    if let Some(t) = rt.tactical.as_mut() {
        t.set_bool(&ui, "isVisible", visible.0);
        t.set_float(&ui, "chargeFrac", sim.grapple_clip.fraction());
        t.set_float(&ui, "refillRate", sim.grapple_clip.refill_rate());
        offhand_frame(&ui, t, data.as_deref(), width, height);
        match ui.update(t, &globals, &topology, &mut rt.frame) {
            Ok(true) => {
                let from = draws.quads.len();
                ui.draws(t, &rt.frame, &mut draws);
                tactical_quads = Some(from..draws.quads.len());
            }
            Ok(false) => {}
            Err(stop) => {
                if rt.reported.insert(format!("{}: {stop:?}", t.name)) {
                    warn!("{}: {stop:?}", t.name);
                }
            }
        }
    }
    if let Some(host) = host.as_deref_mut() {
        let size = [width, height, width, height];
        let gap = HOST_GAP * height / width;
        let mut frame = None;
        host.panel = None;
        if let Some(t) = rt.player.as_mut() {
            t.set_bool(&ui, "isVisible", visible.0);
            let [vw, vh] = lurch::rui::virtual_screen(width, height);
            (t.size, t.inv_size) = ([vw, vh], [1.0 / vw, 1.0 / vh]);
            if let Ok(true) = ui.update(t, &globals, &topology, &mut rt.frame) {
                let from = draws.quads.len();
                ui.draws(t, &rt.frame, &mut draws);
                let name = draws.quads[from..].iter().position(|q| q.flags >> 12 == 0).map_or(draws.quads.len(), |i| from + i);
                frame = bounds(&draws.quads[from..name]);
                if let (Some(panel), Some(contents)) = (frame, bounds(&draws.quads[name..])) {
                    host.panel = Some(std::array::from_fn(|i| [contents[0], panel[1], contents[2], panel[3]][i] * size[i]));
                }
                draws.quads.truncate(name);
            }
        }
        if let (Some(t), Some(panel)) = (rt.ultimate.as_mut(), frame) {
            t.set_bool(&ui, "isVisible", visible.0);
            offhand_frame(&ui, t, data.as_deref(), width, height);
            if let Ok(true) = ui.update(t, &globals, &topology, &mut rt.frame) {
                let from = draws.quads.len();
                ui.draws(t, &rt.frame, &mut draws);
                if let Some(b) = bounds(&draws.quads[from..]) {
                    shift_x(&mut draws.quads[from..], (1.0 - panel[0]) - b[2]);
                }
            }
        }
        let tactical = tactical_quads.and_then(|r| Some((bounds(&draws.quads[r.clone()])?, r)));
        host.room = match (frame, &tactical) {
            (Some(panel), Some((b, _))) => Some((panel[2] + gap + (b[2] - b[0]) + gap) * width),
            (Some(panel), None) => Some((panel[2] + gap) * width),
            _ => None,
        };
        if let (Some(bar), Some((b, r))) = (host_bar, tactical) {
            let over = b[2] - (bar.0 / width - gap);
            if over > 0.0 {
                shift_x(&mut draws.quads[r], -over);
            }
        }
    }
    if visible.0 {
        flyout(&ui, &sim, &mut map, &pose, &globals, &topology, (width, height), &mut rt.frame, &mut draws, &mut rt.reported);
    }
    let mut menu_images = Vec::new();
    if rt.last_menu != menu_open.0
        && let Some(settings) = rt.misc_menu.as_mut()
    {
        if rt.last_menu == Some(GameMenu::Misc) {
            settings.close();
        }
        if menu_open.0 == Some(GameMenu::Misc) {
            settings.open(&ui);
        }
    }
    rt.last_menu = menu_open.0;
    // Hiding the Mouse/Keyboard tab saves (`OnControlsPCPanel_Hide`: `SavePlayerSettings`).
    if let Some(settings) = rt.misc_menu.as_mut()
        && std::mem::take(&mut settings.save)
        && let Some(c) = convars.as_mut()
    {
        c.save = true;
    }
    let mut writes = Vec::new();
    let mut capturing = false;
    if let (Some(GameMenu::Misc), Some(settings)) = (menu_open.0, rt.misc_menu.as_mut()) {
        let screen = Screen { wide: width as i32, tall: height as i32 };
        {
            let convars = convars.as_deref();
            settings.menu.sync_lists(&ui, &|name| convars.and_then(|c| c.get(name)).map(|c| (c.string.clone(), c.float)));
            writes = settings.menu.sync_sliders(&ui, &|name| convars.and_then(|c| c.get(name)).map(|c| ConVarValue { float: c.float, int: c.int, default: atof(c.default) as f32 }));
        }
        settings.menu.layout(&ui, screen);
        settings.scroll(&ui, 0.0);
        if std::mem::take(&mut settings.fill_binds)
            && let (Some(c), Some(list)) = (convars.as_mut(), settings.menu.bind_list.as_mut())
        {
            list.fill_in_current(&mut c.bypass_change_detection().keys);
        }
        // While the key bind list captures, the next key or button is the capture's (the
        // engine's key trap), not the menu's.
        capturing = settings.capturing();
        let at = cursor.0.map(|(x, y)| (x as i32, y as i32));
        if let Some((x, y)) = at {
            settings.hover(&ui, x, y);
            settings.list_hover(screen, (x, y));
            if !capturing {
                settings.drag(&ui, screen, (x, y), mouse.just_pressed(MouseButton::Left), mouse.pressed(MouseButton::Left));
                if mouse.just_pressed(MouseButton::Left) {
                    writes.extend(settings.menu.press(&ui, x, y));
                } else if mouse.pressed(MouseButton::Left) {
                    writes.extend(settings.menu.drag(&ui, x, y));
                }
                if mouse.just_released(MouseButton::Left) {
                    settings.menu.release(&ui, x, y);
                }
            }
        }
        let mut taps = 0;
        for w in wheel.read() {
            taps += rt.menu_wheel.taps(cfg.wheel_mode, w.unit, w.y);
        }
        if let Some(c) = convars.as_mut() {
            let before = c.keys.clone();
            let mut keys = before.clone();
            let pressed = mouse.get_just_pressed().filter_map(|&b| button_code(Input::Mouse(b)));
            for code in pressed {
                settings.list_press(&ui, screen, at.unwrap_or((-1, -1)), code, &mut keys);
            }
            for code in keyboard.get_just_pressed().filter_map(|&k| button_code(Input::Key(k))) {
                settings.list_key(&ui, code, &mut keys);
            }
            if capturing && taps != 0 {
                settings.list_key(&ui, if taps > 0 { 0x71 } else { 0x72 }, &mut keys);
            }
            if keys != before {
                c.keys = keys;
            }
        }
        if !capturing {
            for _ in 0..taps.abs() {
                settings.wheel(&ui, screen, taps > 0);
            }
        }
        capturing |= settings.capturing();
    }
    trap.set_if_neq(KeyTrap(capturing));
    wheel.clear();
    // A slider sets its ConVar's float (`0x94dd90`).
    for (name, v) in writes {
        if let Some(c) = convars.as_mut().and_then(|c| c.get_mut(&name)) {
            c.set_float(v);
        }
    }
    let open = match menu_open.0 {
        Some(GameMenu::System) => rt.system_menu.as_mut(),
        Some(GameMenu::Misc) => rt.misc_menu.as_mut().map(|s| &mut s.menu),
        None => None,
    };
    if let Some(menu) = open {
        let screen = Screen { wide: width as i32, tall: height as i32 };
        menu.layout(&ui, screen);
        if menu_open.0 == Some(GameMenu::System)
            && let Some((x, y)) = cursor.0
        {
            menu.hover(&ui, x as i32, y as i32);
        }
        let mut menu_globals = Globals::new(real.elapsed_secs(), width, height);
        menu_globals.set_clock(real.elapsed().as_micros() as u64);
        let (images_here, stops) = menu.draw(&ui, &menu_globals, screen, (width as i32, height as i32), &topology, &mut rt.frame, &mut draws);
        menu_images = images_here;
        for stop in stops {
            if rt.reported.insert(stop.clone()) {
                warn!("{stop}");
            }
        }
        let click = mouse.just_pressed(MouseButton::Left) && !capturing;
        let clicked = click.then(|| menu.click()).flatten();
        // `OnButton_Activate`: a button of `SystemMenu` closes it, then acts: Settings opens
        // `MiscMenu` (`OpenSettingsMenu`); Leave Match does nothing in lurch.
        if let (Some(GameMenu::System), Some(button), true) = (menu_open.0, menu.hovered.clone(), click) {
            menu_open.0 = (menu.find(&["SystemPanel", "Button0"]) == Some(button)).then_some(GameMenu::Misc);
            paused.0 = menu_open.0.is_some();
        }
        // A DialogListButton's ConVar is set through its ConVarRef (`0x948e9b`), which queues no
        // `savePlayerConfig`.
        if menu_open.0 == Some(GameMenu::Misc)
            && click
            && let Some(settings) = rt.misc_menu.as_mut()
        {
            settings.click(&ui);
            if let Some((name, value)) = clicked
                && let Some(c) = convars.as_mut().and_then(|c| c.get_mut(&name))
            {
                c.set_string(&value);
            }
        }
    }
    for menu in [rt.system_menu.as_mut(), rt.misc_menu.as_mut().map(|s| &mut s.menu)].into_iter().flatten() {
        sounds.write_batch(menu.sounds.drain(..).map(MenuSound));
    }
    for u in draws.unported.drain(..) {
        if rt.reported.insert(u.clone()) {
            warn!("not drawn, not ported yet: {u}");
        }
    }
    let mut bytes: Vec<u8> = draws.styles.iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
    if bytes.len() > rt.styles_size {
        rt.styles_size = bytes.len().next_power_of_two();
        rt.styles = buffers.add(ShaderBuffer::new(&vec![0; rt.styles_size], RenderAssetUsages::default()));
        rt.materials.clear();
    }
    bytes.resize(rt.styles_size, 0);
    if bytes != rt.styles_bytes {
        if let Some(mut buffer) = buffers.get_mut(&rt.styles) {
            *buffer = ShaderBuffer::new(&bytes, RenderAssetUsages::default());
        }
        rt.styles_bytes = bytes;
    }
    let special = rt.blur.as_ref().map_or([0.0; 4], ScreenBlur::bounds);
    // Runs and the menu's images share one draw order: an image ends the run before it.
    let mut runs: Vec<(BatchKey, Vec<Quad>)> = Vec::new();
    let mut image_runs: Vec<(usize, Paint)> = Vec::new();
    let mut pending = menu_images.into_iter().peekable();
    let mut order = 0;
    let mut after_image = false;
    for (qi, q) in draws.quads.into_iter().enumerate() {
        while let Some((_, p)) = pending.next_if(|(at, _)| *at <= qi) {
            image_runs.push((order, p));
            order += 1;
            after_image = true;
        }
        let key = if q.flags >> 12 == 0 { (true, q.atlas, -1) } else { (false, q.atlas, if q.mask >= 0 { q.mask_atlas as i16 } else { -1 }) };
        match runs.last_mut() {
            Some(run) if !after_image && (run.0 .1, run.0 .2, run.0 .3) == key => run.1.push(q),
            _ => {
                runs.push(((order, key.0, key.1, key.2), vec![q]));
                order += 1;
                after_image = false;
            }
        }
    }
    for (_, p) in pending {
        image_runs.push((order, p));
        order += 1;
    }
    for (i, (key, quads)) in runs.iter().enumerate() {
        let (atlases, fonts, white, styles, no_glyphs, blur) = (&rt.atlases, &rt.fonts, &rt.white, &rt.styles, &rt.no_glyphs, &rt.blur);
        let material = rt
            .materials
            .entry(*key)
            .or_insert_with(|| {
                let (order, text, atlas, mask) = *key;
                let order = order as f32;
                if text {
                    let (image, glyphs) = fonts.get(atlas as usize).cloned().unwrap_or((None, no_glyphs.clone()));
                    return materials.add(RuiMaterial { image: image.unwrap_or_else(|| white.clone()), mask: white.clone(), styles: styles.clone(), glyphs, order });
                }
                let texture = |atlas: u8| if atlas == SPECIAL_ATLAS { blur.as_ref().map(|b| b.small[0].clone()) } else { atlases.get(atlas as usize).cloned().flatten() };
                let image = texture(atlas).unwrap_or_else(|| white.clone());
                let mask = if mask >= 0 { texture(mask as u8) } else { None }.unwrap_or_else(|| white.clone());
                materials.add(RuiMaterial { image, mask, styles: styles.clone(), glyphs: no_glyphs.clone(), order })
            })
            .clone();
        if i == rt.batches.len() {
            let mesh = meshes.add(quads_mesh(&ui, &[], &topology, special));
            let entity = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), RenderLayers::layer(UI_LAYER), NoFrustumCulling, Transform::default(), Visibility::Hidden)).id();
            rt.batches.push(Batch { entity, mesh, quads: Vec::new(), size: (0.0, 0.0) });
        }
        let batch = &mut rt.batches[i];
        if batch.quads != *quads || batch.size != (width, height) {
            if let Some(mut m) = meshes.get_mut(&batch.mesh) {
                *m = quads_mesh(&ui, quads, &topology, special);
            }
            batch.quads.clone_from(quads);
            batch.size = (width, height);
        }
        if let Ok((mut vis, mut mat)) = batches.get_mut(batch.entity) {
            vis.set_if_neq(Visibility::Inherited);
            if mat.0 != material {
                mat.0 = material;
            }
        }
    }
    for batch in &rt.batches[runs.len()..] {
        if let Ok((mut vis, _)) = batches.get_mut(batch.entity) {
            vis.set_if_neq(Visibility::Hidden);
        }
    }
    let mut drawn = 0;
    for (order, paint) in image_runs {
        let (rect, color, clip, texture) = match &paint {
            Paint::Fill { rect, color, clip } => (*rect, *color, *clip, rt.white.clone()),
            Paint::Image { texture, rect, color, clip } => match rt.vgui_textures.get(texture) {
                Some(t) => (*rect, *color, *clip, t.clone()),
                None => {
                    if rt.reported.insert(texture.clone()) {
                        warn!("{texture}: not loaded");
                    }
                    continue;
                }
            },
            Paint::Rui { .. } | Paint::Text { .. } | Paint::List { .. } | Paint::Label { .. } => continue,
        };
        if color[3] == 0 {
            continue;
        }
        let material = VguiMaterial { image: texture, color: vgui_color(color), clip: Vec4::new(clip.0 as f32, clip.1 as f32, clip.2 as f32, clip.3 as f32), order: order as f32 };
        if drawn == rt.images.len() {
            let mesh = meshes.add(image_mesh(rect));
            let handle = vgui_materials.add(material.clone());
            let entity = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(handle.clone()), RenderLayers::layer(UI_LAYER), NoFrustumCulling, Transform::default(), Visibility::Inherited)).id();
            rt.images.push((entity, mesh, handle));
        } else {
            let (entity, mesh, handle) = &rt.images[drawn];
            if let Some(mut m) = meshes.get_mut(mesh) {
                *m = image_mesh(rect);
            }
            if let Some(mut m) = vgui_materials.get_mut(handle) {
                *m = material;
            }
            if let Ok(mut vis) = shown.get_mut(*entity) {
                vis.set_if_neq(Visibility::Inherited);
            }
        }
        drawn += 1;
    }
    for (entity, _, _) in &rt.images[drawn..] {
        if let Ok(mut vis) = shown.get_mut(*entity) {
            vis.set_if_neq(Visibility::Hidden);
        }
    }
}
