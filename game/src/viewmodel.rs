//! The first-person viewmodel: the fists (`lurch::anim::viewmodel`) play their sequences, and
//! the legend's arms follow the weapon's bones, both read from the R5Reloaded install when the
//! game starts. Each frame blends the main sequence, the sequences it is leaving and the
//! overlay layers, in StandardBlendingRules' order. The arms are skinned on the CPU with every
//! weight a vertex has, and a second camera draws them over the world with the viewmodel's FOV,
//! and the HUD over them: drawn by the first camera, the HUD would be painted over.
//!
//! Through a mantle or a wall climb the arms are the first-person proxy instead
//! (`lurch::anim::proxy`): the arms model plays its own mantle or climb sequence, placed in
//! the world on the ledge or at the wall, and the fists are put away for it (lowered, then
//! not drawn) and raised after.
//!
//! The arms' materials give the albedo, the normal map (DirectX's green, flipped for Bevy) and
//! ambient occlusion; tangents are generated at the bind pose and skinned with the normals.
//!
//! Not modelled yet: sway, bob and the view of the camera bone, the oracle's own bone
//! concatenation, the materials' gloss, spec, emissive and cavity textures and the game's own
//! shading. That a mantle puts the fists away is what R5Reloaded and retail show; which call
//! disables them (`0x7d0dc0`) is not settled.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::camera::visibility::RenderLayers as Layers;
use lurch::anim::bones::{self, BoneList, Mat, IDENTITY};
use lurch::anim::proxy::Proxy;
use lurch::anim::viewmodel::{Blends, Viewmodel};
use lurch::assets::texture::Texture;
use lurch::sim::cue::Cue;
use lurch::sim::grapple::FP_ANIM_GRAPPLE;
use lurch::sim::math::V3;

use crate::convert::{dir, pos, rot, vfov, INCH};
use crate::data::{Data, GameData};
use crate::sim::{FrameCues, Pose as EyePose, Sim};
use crate::view::{sky_light, MainCamera, ViewImage};
use crate::{common, Frame};

pub const VIEWMODEL_LAYER: usize = 1;
/// The viewmodel's FOV when the legend's settings do not give one: horizontal at 4:3, widened
/// for the aspect as the world FOV is (view setup, 0x5d4740, reads the player setting
/// `viewmodelfov`).
pub const DEFAULT_VIEWMODEL_FOV: f32 = 70.0;

#[derive(Resource)]
struct Vm {
    state: Viewmodel,
    proxy: Proxy,
    meshes: Vec<Handle<Mesh>>,
    /// Each mesh's tangents at the bind pose, in the model's space, with their handedness.
    tangents: Vec<Vec<[f32; 4]>>,
    /// The grapple had the weapons down after the last command.
    grapple_down: bool,
}

/// Where the rope leaves the hand this frame while the grapple's animation plays: the arms'
/// `grapple_rope` attachment, in the world.
#[derive(Resource, Default)]
pub struct GrappleHand(pub Option<V3>);

/// The proxy finished the grapple's last sequence: the next commands end the player's
/// first-person animation state (PostThink, `0x86ef40`).
#[derive(Resource, Default)]
pub struct GrappleAnimOver(pub bool);

/// Material texture slots of the arms' shader (`lurch::assets::texture::material`).
const ALBEDO: usize = 0;
const NORMAL: usize = 1;
const AO: usize = 5;

/// A texture as the pak holds it, its blocks handed to the GPU as they are.
pub fn gpu_image(t: &Texture) -> Option<Image> {
    use TextureFormat::*;
    let format = [Bc1RgbaUnorm, Bc1RgbaUnormSrgb, Bc2RgbaUnorm, Bc2RgbaUnormSrgb, Bc3RgbaUnorm, Bc3RgbaUnormSrgb, Bc4RUnorm, Bc4RSnorm, Bc5RgUnorm, Bc5RgSnorm, Bc6hRgbUfloat, Bc6hRgbFloat, Bc7RgbaUnorm, Bc7RgbaUnormSrgb].get(t.format as usize)?;
    let size = Extent3d { width: t.width, height: t.height, depth_or_array_layers: 1 };
    let mut image = Image::new(size, TextureDimension::D2, t.mips.concat(), *format, RenderAssetUsages::RENDER_WORLD);
    image.texture_descriptor.mip_level_count = t.mips.len() as u32;
    let wrap = ImageAddressMode::Repeat;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: wrap, address_mode_v: wrap, anisotropy_clamp: 8, ..ImageSamplerDescriptor::linear() });
    Some(image)
}

/// The viewmodel's animation, which the sound plays after.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Animate;

/// This frame's sequence events on the viewmodel and the first-person proxy, as (event,
/// options), for the sound layer.
#[derive(Resource, Default)]
pub struct VmEvents(pub Vec<(u16, String)>);

#[derive(Component)]
pub struct ViewmodelCamera;

#[derive(Component)]
struct ViewmodelMesh;

pub struct ViewmodelPlugin;

impl Plugin for ViewmodelPlugin {
    fn build(&self, app: &mut App) {
        common(app);
        app.init_resource::<VmEvents>().init_resource::<GrappleHand>().init_resource::<GrappleAnimOver>().add_systems(
            Update,
            (spawn_meshes.run_if(resource_added::<Data>), animate.run_if(resource_exists::<Vm>).in_set(Animate), sync_camera).chain().in_set(Frame::View),
        );
    }
}

#[allow(clippy::too_many_arguments)] // Bevy's system parameters
fn spawn_meshes(
    mut commands: Commands,
    data: Res<Data>,
    sim: Res<Sim>,
    pose: Res<EyePose>,
    image: Option<Res<ViewImage>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    main: Query<Entity, With<MainCamera>>,
) {
    for e in &main {
        commands.entity(e).remove::<IsDefaultUiCamera>();
    }
    let mut camera = commands.spawn((
        Camera3d::default(),
        Camera { order: 1, clear_color: ClearColorConfig::None, ..default() },
        IsDefaultUiCamera,
        Projection::Perspective(PerspectiveProjection { near: 0.01, far: 100.0, fov: vfov(data.0.legend.float("viewmodelfov").unwrap_or(DEFAULT_VIEWMODEL_FOV)), ..default() }),
        Transform::from_translation(pos(pose.0.origin)).with_rotation(rot(pose.0.angles)),
        RenderLayers::layer(VIEWMODEL_LAYER),
        ViewmodelCamera,
        sky_light(&mut images),
    ));
    if let Some(image) = image {
        camera.insert(RenderTarget::Image(image.0.clone().into()));
    }
    let mut handles = Vec::new();
    let mut tangents = Vec::new();
    for (m, textures) in data.0.arms.meshes.iter().zip(&data.0.arms.textures) {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions.iter().map(|&p| (dir(p) * INCH).to_array()).collect::<Vec<_>>())
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals.iter().map(|&n| dir(n).to_array()).collect::<Vec<_>>())
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs.clone())
            .with_inserted_indices(Indices::U16(m.indices.chunks(3).flat_map(|t| [t[0], t[2], t[1]]).collect()));
        if let Err(e) = mesh.generate_tangents() {
            warn!("arms mesh {}: no tangents: {e}", m.material);
        }
        let bind: Vec<[f32; 4]> = match mesh.attribute(Mesh::ATTRIBUTE_TANGENT) {
            Some(VertexAttributeValues::Float32x4(t)) => t.iter().map(|t| [-t[2], -t[0], t[1], t[3]]).collect(),
            _ => Vec::new(),
        };
        let mut texture = |slot: usize| textures.get(slot)?.as_ref().and_then(gpu_image).map(|i| images.add(i));
        let normal_map = if bind.is_empty() { None } else { texture(NORMAL) };
        let material = materials.add(StandardMaterial {
            base_color_texture: texture(ALBEDO),
            normal_map_texture: normal_map,
            flip_normal_map_y: true,
            occlusion_texture: texture(AO),
            perceptual_roughness: 0.8,
            ..default()
        });
        let h = meshes.add(mesh);
        commands.spawn((Mesh3d(h.clone()), MeshMaterial3d(material), RenderLayers::layer(VIEWMODEL_LAYER), NoFrustumCulling, NotShadowCaster, ViewmodelMesh, Transform::default()));
        handles.push(h);
        tangents.push(bind);
    }
    let class = data.0.legend.str("classActivityModifier").filter(|m| !m.is_empty()).map(str::to_string);
    let mut state = Viewmodel::new(class.into_iter().collect(), data.0.times);
    state.deploy(&data.0.anims, sim.pilot.player.sprinting, sim.view_now);
    info!("viewmodel: {} sequences, {} arms meshes", data.0.anims.seqs.len(), handles.len());
    let mut proxy = Proxy::default();
    proxy.poses = data.0.arms_anims.sk.default_poses();
    commands.insert_resource(Vm { state, proxy, meshes: handles, tangents, grapple_down: false });
}

fn mul_point(m: &Mat, v: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2] + m[r][3])
}

fn mul_dir(m: &Mat, v: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2])
}

#[allow(clippy::too_many_arguments)] // Bevy's system parameters
fn animate(
    mut vm: ResMut<Vm>,
    data: Res<Data>,
    sim: Res<Sim>,
    cues: Res<FrameCues>,
    mut eye: ResMut<EyePose>,
    mut events: ResMut<VmEvents>,
    mut hand: ResMut<GrappleHand>,
    mut over: ResMut<GrappleAnimOver>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut roots: Query<(&mut Transform, &mut Layers, &mut Visibility), With<ViewmodelMesh>>,
) {
    let vm = &mut *vm;
    let d = &*data.0;
    let grapple_lowers = d.legend.bool("grapple_disableWeaponsWhenActive").unwrap_or(false);
    for step in &cues.0 {
        let now = step.sound.curtime;
        let was = vm.proxy.visible;
        for cue in &step.cues {
            if let Cue::GrappleAnim(g) = cue {
                vm.proxy.grapple(&d.arms_anims, *g, now);
            }
        }
        if (step.traversal.is_some() || step.climb.is_some()) && !was {
            vm.state.holster(&d.anims, now);
        }
        let down = grapple_lowers && step.grapple_down;
        if down && !vm.grapple_down {
            vm.state.holster(&d.anims, now);
        } else if !down && vm.grapple_down {
            vm.state.unholster(&d.anims, now);
        }
        vm.grapple_down = down;
        vm.state.command(&d.anims, &step.cues, step.sprinting, now);
        let drawn = !vm.state.hidden(now);
        vm.proxy.command(&d.arms_anims, step.traversal, drawn, sim.settings.mantle.jumpoff_duration, now);
        vm.proxy.climb(&d.arms_anims, step.climb, now);
        if was && !vm.proxy.visible {
            vm.state.unholster(&d.anims, now);
        }
        over.0 |= vm.proxy.grapple_think(&d.arms_anims, now);
    }
    let (blends, fired) = vm.state.frame(&d.anims, sim.view_now);
    events.0.extend(fired);
    let (proxy_blends, fired) = vm.proxy.frame(&d.arms_anims, sim.view_now);
    events.0.extend(fired);
    let own = arms_pose(d, &proxy_blends, &vm.proxy.poses);
    let at = |a: Option<(usize, Mat)>| a.map_or(IDENTITY, |(b, local)| bones::concat(&own[b], &local));
    let reference = at(d.arms.reference);
    let pinned = vm.proxy.pinned(&reference, 1.0);
    let player = &sim.pilot.player;
    let view_angles = player.view_angles;
    let stand = &sim.settings.stances[0];
    let half_hull = (stand.hull_max[2] - stand.hull_min[2]) * 0.5;
    let a = eye.0.angles;
    let base_eye = eye.0.origin;
    vm.proxy.slide_frame(player.view_entity, player.view_entity_blend);
    let (origin, angles) = vm.proxy.climb_view(&at(d.arms.camera), &at(d.arms.camera_base), player.view_entity, player.view_entity_blend, eye.0.origin, view_angles);
    let (origin, angles) = vm.proxy.view(&d.arms_anims, &pinned, &at(d.arms.camera), &at(d.arms.camera_base), half_hull, origin, angles, sim.view_now);
    eye.0.origin = origin;
    eye.0.angles = [angles[0] + (a[0] - view_angles[0]), angles[1] + (a[1] - view_angles[1]), angles[2] + (a[2] - view_angles[2])];
    if vm.proxy.climbing() {
        let place = vm.proxy.climb_place(sim.pilot.origin, a, player.view_offset[2]);
        let skin: Vec<Mat> = d.arms.bones.iter().zip(&own).map(|(b, m)| bones::concat(&bones::concat(&place, m), &b.pose_to_bone)).collect();
        skin_meshes(&d.arms.meshes, &vm.tangents, &skin, &vm.meshes, &mut meshes);
        for (mut t, mut layers, mut shown) in &mut roots {
            *t = Transform::IDENTITY;
            *layers = Layers::layer(0);
            *shown = Visibility::Inherited;
        }
        return;
    }
    hand.0 = None;
    if vm.proxy.grappling() {
        let aims = player.first_person_anim == FP_ANIM_GRAPPLE && player.view_entity && (player.grappling || player.grapple_retracting);
        let (place, own) = if aims {
            let place = vm.proxy.grapple_place(&at(d.arms.camera_base), eye.0.origin, eye.0.angles);
            let frametime = cues.0.last().map_or(0.0, |s| s.frametime);
            vm.proxy.grapple_aim(&d.arms_anims, player.grapple_points[0], eye.0.origin, eye.0.angles, frametime);
            (place, arms_pose(d, &proxy_blends, &vm.proxy.poses))
        } else {
            (vm.proxy.eye_place(&at(d.arms.camera), base_eye, view_angles), own.clone())
        };
        let at = |a: Option<(usize, Mat)>| a.map_or(IDENTITY, |(b, local)| bones::concat(&own[b], &local));
        if d.arms.grapple_rope.is_some() {
            let r = bones::concat(&place, &at(d.arms.grapple_rope));
            hand.0 = Some([r[0][3], r[1][3], r[2][3]]);
        }
        let skin: Vec<Mat> = d.arms.bones.iter().zip(&own).map(|(b, m)| bones::concat(&bones::concat(&place, m), &b.pose_to_bone)).collect();
        skin_meshes(&d.arms.meshes, &vm.tangents, &skin, &vm.meshes, &mut meshes);
        for (mut t, mut layers, mut shown) in &mut roots {
            *t = Transform::IDENTITY;
            *layers = Layers::layer(0);
            *shown = Visibility::Inherited;
        }
        return;
    }
    if vm.proxy.hands_in(sim.view_now) {
        let forward = lurch::sim::math::angle_vectors(view_angles).0;
        let place = vm.proxy.place(&reference, 1.0, forward, sim.view_now);
        let skin: Vec<Mat> = d.arms.bones.iter().zip(&own).map(|(b, m)| bones::concat(&bones::concat(&place, m), &b.pose_to_bone)).collect();
        skin_meshes(&d.arms.meshes, &vm.tangents, &skin, &vm.meshes, &mut meshes);
        for (mut t, mut layers, mut shown) in &mut roots {
            *t = Transform::IDENTITY;
            *layers = Layers::layer(0);
            *shown = Visibility::Inherited;
        }
        return;
    }
    if vm.state.hidden(sim.view_now) {
        for (_, _, mut shown) in &mut roots {
            *shown = Visibility::Hidden;
        }
        return;
    }
    let sk = &d.anims.sk;
    let all = BoneList::all(sk.names.len());
    let poses = sk.default_poses();
    let mut pose = d.defaults.clone();
    if let Some((s, c)) = blends.main {
        bones::accumulate_pose(sk, &mut pose, s, c, 1.0, &poses, &all);
    }
    for (s, c, w) in blends.transitions.into_iter().chain(blends.layers) {
        bones::accumulate_pose(sk, &mut pose, s, c, w, &poses, &all);
    }
    let weapon = bones::build_transforms(sk, &pose, &IDENTITY);
    let arms = &d.arms;
    let mut am: Vec<Mat> = Vec::with_capacity(arms.bones.len());
    for (i, b) in arms.bones.iter().enumerate() {
        let m = match arms.merge[i] {
            Some(j) => weapon[j],
            None => {
                let local = bones::quat_matrix(b.quat, b.pos);
                if b.parent < 0 { local } else { bones::concat(&am[b.parent as usize], &local) }
            }
        };
        am.push(m);
    }
    let skin: Vec<Mat> = arms.bones.iter().zip(&am).map(|(b, m)| bones::concat(m, &b.pose_to_bone)).collect();
    skin_meshes(&arms.meshes, &vm.tangents, &skin, &vm.meshes, &mut meshes);
    let t = Transform::from_translation(pos(eye.0.origin)).with_rotation(rot(eye.0.angles));
    for (mut r, mut layers, mut shown) in &mut roots {
        *r = t;
        *layers = Layers::layer(VIEWMODEL_LAYER);
        *shown = Visibility::Inherited;
    }
}

/// The arms' bones in the model's space as the proxy plays `blends` with pose parameters
/// `poses`.
fn arms_pose(d: &GameData, blends: &Blends, poses: &[f32]) -> Vec<Mat> {
    let sk = &d.arms_anims.sk;
    let all = BoneList::all(sk.names.len());
    let mut pose = d.arms.defaults.clone();
    if let Some((s, c)) = blends.main {
        bones::accumulate_pose(sk, &mut pose, s, c, 1.0, poses, &all);
    }
    for &(s, c, w) in &blends.transitions {
        bones::accumulate_pose(sk, &mut pose, s, c, w, poses, &all);
    }
    bones::build_transforms(sk, &pose, &IDENTITY)
}

/// Skins the arms on the CPU with every weight a vertex has: `skin` takes each bone from the
/// bind pose to where it is drawn.
fn skin_meshes(arms: &[lurch::assets::model::Mesh], bind_tangents: &[Vec<[f32; 4]>], skin: &[Mat], handles: &[Handle<Mesh>], meshes: &mut Assets<Mesh>) {
    for ((mesh, bind), handle) in arms.iter().zip(bind_tangents).zip(handles) {
        let mut positions = Vec::with_capacity(mesh.positions.len());
        let mut normals = Vec::with_capacity(mesh.positions.len());
        let mut tangents = Vec::with_capacity(bind.len());
        for (k, v) in mesh.positions.iter().enumerate() {
            let (mut p, mut n, mut t) = ([0.0f32; 3], [0.0f32; 3], [0.0f32; 3]);
            let bt = bind.get(k).map_or([0.0; 3], |t| [t[0], t[1], t[2]]);
            for &(b, w) in &mesh.weights[k] {
                let s = &skin[b as usize];
                let (pv, nv, tv) = (mul_point(s, *v), mul_dir(s, mesh.normals[k]), mul_dir(s, bt));
                for c in 0..3 {
                    p[c] += w * pv[c];
                    n[c] += w * nv[c];
                    t[c] += w * tv[c];
                }
            }
            positions.push((dir(p) * INCH).to_array());
            normals.push(dir(n).normalize_or_zero().to_array());
            if let Some(b) = bind.get(k) {
                tangents.push(dir(t).normalize_or_zero().extend(b[3]).to_array());
            }
        }
        if let Some(mut m) = meshes.get_mut(handle) {
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            if !bind.is_empty() {
                m.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
            }
        }
    }
}

fn sync_camera(eye: Res<EyePose>, mut cams: Query<&mut Transform, With<ViewmodelCamera>>) {
    for mut t in &mut cams {
        *t = Transform::from_translation(pos(eye.0.origin)).with_rotation(rot(eye.0.angles));
    }
}
