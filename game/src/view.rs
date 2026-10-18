use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraOutputMode, RenderTarget};
use bevy::light::cluster::ClusterConfig;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{BlendState, TextureFormat, TextureUsages};
use bevy::render::renderer::RenderAdapterInfo;
use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use lurch::sim::math::V3;
use lurch::testmap;
use lurch::world::Tri;

use crate::convert::{dir, pos, rot, vfov, INCH};
use crate::data::Props;
use crate::sim::{Pose, Sim};
use crate::viewmodel::{gpu_image, host_lit, HostLight, ViewmodelCamera, ViewmodelMaterial};
use crate::{advance_frame, common, Config, Frame, FrameIndex, Paused, PendingShots};

pub const SKY: Color = Color::srgb(0.55, 0.70, 0.85);
pub const BODY_LAYER: usize = 2;
const TILE: f32 = 256.0;
const FLOOR: f32 = 16384.0;
const ROPE_RADIUS: f32 = 1.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Window,
    Image { w: u32, h: u32 },
    /// An image of only what the player holds and sees over the world (the viewmodel, the HUD
    /// and the grapple's rope), on a transparent background: for another game's world under it.
    Overlay { w: u32, h: u32 },
}

pub struct ViewPlugin {
    pub target: Target,
}

impl Default for ViewPlugin {
    fn default() -> Self {
        ViewPlugin { target: Target::Window }
    }
}

#[derive(Component)]
pub struct MainCamera;

#[derive(Component)]
pub struct Body;

/// Pathfinder's grapple rope, rebuilt each frame from the rope's points.
#[derive(Component)]
struct GrappleRope;

/// The grapple's hook entity, drawn where it is.
#[derive(Component)]
struct GrappleHook;

/// The image the main camera renders into, when it is not a window.
#[derive(Resource, Clone)]
pub struct ViewImage(pub Handle<Image>);

#[derive(Resource, Clone, Copy)]
struct ViewTarget(Target);

/// The present mode asked for and the adapter in use, for the log and the HUD. Whether the
/// driver honoured the mode is not readable here.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct DisplayInfo {
    pub present: String,
    pub adapter: String,
}

impl Plugin for ViewPlugin {
    fn build(&self, app: &mut App) {
        common(app);
        app.insert_resource(ViewTarget(self.target))
            .insert_resource(ClearColor(if matches!(self.target, Target::Overlay { .. }) { Color::NONE } else { SKY }))
            .insert_resource(GlobalAmbientLight::NONE)
            .init_resource::<DisplayInfo>()
            .add_systems(Startup, (setup, spawn_props, log_display))
            .add_systems(Update, (sync_camera.after(crate::viewmodel::Animate), sync_body, sync_rope.after(crate::viewmodel::Animate), overlay_cameras.after(sync_rope), apply_cursor).in_set(Frame::View))
            .add_systems(Update, grapple_look.run_if(resource_added::<crate::data::Data>))
            .add_systems(Last, take_shots.before(advance_frame));
    }
}

/// Ambient light from the sky above and the floor below, so a face is shaded by where it
/// points even out of the sun.
pub fn sky_light(images: &mut Assets<Image>) -> EnvironmentMapLight {
    let (sky, ground) = (Color::srgb(0.80, 0.85, 0.92), Color::srgb(0.45, 0.43, 0.40));
    EnvironmentMapLight { intensity: 1500.0, ..EnvironmentMapLight::hemispherical_gradient(images, sky, sky.mix(&ground, 0.5), ground) }
}

fn srgb(r: f32, g: f32, b: f32) -> [f32; 4] {
    Color::srgb(r, g, b).to_linear().to_f32_array()
}

/// Walls are tinted by the axis their normal faces, not its sign: the test map's winding is
/// not consistent.
fn tint(t: &Tri) -> [f32; 4] {
    let (a, b, c) = (t[0], t[1], t[2]);
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]].map(f32::abs);
    if n[2] >= n[0] && n[2] >= n[1] {
        srgb(0.75, 0.75, 0.75)
    } else if n[0] >= n[1] {
        srgb(0.80, 0.64, 0.50)
    } else {
        srgb(0.50, 0.62, 0.80)
    }
}

fn floor_tiles(push: &mut impl FnMut(V3, [f32; 4])) {
    let n = (2.0 * FLOOR / TILE) as i32;
    for i in 0..n {
        for j in 0..n {
            let (x0, y0) = (-FLOOR + i as f32 * TILE, -FLOOR + j as f32 * TILE);
            let (x1, y1) = (x0 + TILE, y0 + TILE);
            let grey = if (i + j) % 2 == 0 { 0.42 } else { 0.50 };
            let c = srgb(grey, grey, grey);
            for v in [[x0, y0, 0.0], [x1, y0, 0.0], [x1, y1, 0.0], [x0, y0, 0.0], [x1, y1, 0.0], [x0, y1, 0.0]] {
                push(v, c);
            }
        }
    }
}

/// A rope's span drawn as a thin square tube: drawing only, ropes do not collide.
fn rope_tris(a: V3, b: V3, radius: f32) -> Vec<Tri> {
    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let side = if d[0].abs() < 1e-3 && d[1].abs() < 1e-3 { [1.0, 0.0, 0.0] } else { [-d[1], d[0], 0.0] };
    let l = (side[0] * side[0] + side[1] * side[1] + side[2] * side[2]).sqrt();
    let u = side.map(|c| c / l * radius);
    let w = [d[1] * u[2] - d[2] * u[1], d[2] * u[0] - d[0] * u[2], d[0] * u[1] - d[1] * u[0]];
    let lw = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
    let w = w.map(|c| c / lw * radius);
    let corner = |p: V3, s: f32, t: f32| [p[0] + u[0] * s + w[0] * t, p[1] + u[1] * s + w[1] * t, p[2] + u[2] * s + w[2] * t];
    let ring = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];
    let mut out = Vec::new();
    for i in 0..4 {
        let ((s0, t0), (s1, t1)) = (ring[i], ring[(i + 1) % 4]);
        let (p0, p1, q0, q1) = (corner(a, s0, t0), corner(a, s1, t1), corner(b, s0, t0), corner(b, s1, t1));
        out.push([p0, p1, q1]);
        out.push([p0, q1, q0]);
    }
    out
}

/// The test map as one flat-shaded mesh: the floor drawn as a checker (collision keeps its
/// two triangles), everything else tinted, and the ziplines' ropes.
pub fn map_mesh() -> Mesh {
    let map = testmap::build();
    let tris = map.tris;
    let (mut positions, mut colors) = (Vec::new(), Vec::new());
    let mut push = |v: V3, c: [f32; 4]| {
        positions.push(pos(v).to_array());
        colors.push(c);
    };
    floor_tiles(&mut push);
    for t in &tris[2..] {
        let c = tint(t);
        for v in t {
            push(*v, c);
        }
    }
    let rope = srgb(0.15, 0.15, 0.15);
    for z in map.ziplines.iter().filter(|z| z.rope) {
        for w in z.points.windows(2) {
            for t in rope_tris(w[0], w[1], ROPE_RADIUS) {
                for v in t {
                    push(v, rope);
                }
            }
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_computed_flat_normals()
}

#[allow(clippy::too_many_arguments)] // Bevy's system parameters
fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut images: ResMut<Assets<Image>>, target: Res<ViewTarget>, pose: Res<Pose>, cfg: Res<Config>) {
    let material = StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 1.0, cull_mode: None, double_sided: true, ..default() };
    if !matches!(target.0, Target::Overlay { .. }) {
        commands.spawn((Mesh3d(meshes.add(map_mesh())), MeshMaterial3d(materials.add(material))));
    }
    commands.spawn((
        DirectionalLight { shadow_maps_enabled: cfg.shadows, ..default() },
        Transform::default().looking_to(Vec3::new(0.55, -0.65, -0.52), Vec3::Y),
        RenderLayers::from_layers(&[0, 1, BODY_LAYER]),
    ));
    let projection = Projection::Perspective(PerspectiveProjection { near: 0.05, far: 2000.0, fov: vfov(pose.0.fov), ..default() });
    let transform = Transform::from_translation(pos(pose.0.origin)).with_rotation(rot(pose.0.angles));
    // One light cluster: the scene has no point or spot lights to sort into more, and each
    // cluster costs the frame.
    let mut camera = commands.spawn((Camera3d::default(), MainCamera, IsDefaultUiCamera, projection, transform, sky_light(&mut images), ClusterConfig::Single));
    if let Target::Image { w, h } | Target::Overlay { w, h } = target.0 {
        let mut image = Image::new_target_texture(w, h, TextureFormat::Rgba8UnormSrgb, None);
        image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let handle = images.add(image);
        camera.insert(RenderTarget::Image(handle.clone().into()));
        commands.insert_resource(ViewImage(handle));
    }
    let rope = StandardMaterial { base_color: Color::srgb(0.15, 0.15, 0.15), perceptual_roughness: 1.0, ..default() };
    commands.spawn((GrappleRope, Mesh3d(meshes.add(Cuboid::default())), MeshMaterial3d(materials.add(rope)), Visibility::Hidden));
    let grey = StandardMaterial { base_color: Color::srgb(0.6, 0.6, 0.6), ..default() };
    commands.spawn((Body, Mesh3d(meshes.add(Cuboid::default())), MeshMaterial3d(materials.add(grey)), RenderLayers::layer(BODY_LAYER), NotShadowCaster, Transform::default()));
}

/// Each prop mesh with its albedo tinted by its vertex colours; the models' winding is
/// clockwise.
fn spawn_props(mut commands: Commands, props: Option<Res<Props>>, target: Res<ViewTarget>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut images: ResMut<Assets<Image>>) {
    let Some(props) = props.filter(|_| !matches!(target.0, Target::Overlay { .. })) else { return };
    for m in &props.0 {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions.iter().map(|&p| pos(p).to_array()).collect::<Vec<_>>())
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals.iter().map(|&n| dir(n).to_array()).collect::<Vec<_>>())
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs.clone())
            .with_inserted_indices(Indices::U16(m.indices.chunks(3).flat_map(|t| [t[0], t[2], t[1]]).collect()));
        if !m.colors.is_empty() {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, m.colors.iter().map(|&[r, g, b, a]| Color::srgba_u8(r, g, b, a).to_linear().to_f32_array()).collect::<Vec<_>>());
        }
        let base_color_texture = m.albedo.as_ref().and_then(gpu_image).map(|i| images.add(i));
        let material = materials.add(StandardMaterial { base_color_texture, perceptual_roughness: 0.8, ..default() });
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material)));
    }
}

fn sync_camera(pose: Res<Pose>, mut cams: Query<(&mut Transform, &mut Projection), With<MainCamera>>) {
    for (mut t, mut proj) in &mut cams {
        *t = Transform::from_translation(pos(pose.0.origin)).with_rotation(rot(pose.0.angles));
        if let Projection::Perspective(p) = &mut *proj {
            p.fov = vfov(pose.0.fov);
        }
    }
}

fn sync_body(sim: Res<Sim>, mut bodies: Query<&mut Transform, With<Body>>) {
    let p = &sim.pilot.player;
    let (lo, hi) = if p.ducked() { (p.hull_duck_min, p.hull_duck_max) } else { (p.hull_min, p.hull_max) };
    let o = sim.pilot.origin;
    let centre = [o[0] + (lo[0] + hi[0]) * 0.5, o[1] + (lo[1] + hi[1]) * 0.5, o[2] + (lo[2] + hi[2]) * 0.5];
    let size = dir([hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]).abs() * INCH;
    for mut t in &mut bodies {
        *t = Transform::from_translation(pos(centre)).with_scale(size);
    }
}

/// The rope from the hand through its bends to the hook, while the hook is out.
/// The rope drawn with the game's `cable/grapple` textures and the hook with its own model,
/// once the install's files are read.
/// Over another game's world (`HostLight`), both take that game's light, as the arms do.
#[allow(clippy::too_many_arguments)] // Bevy's system parameters
fn grapple_look(
    mut commands: Commands,
    data: Res<crate::data::Data>,
    host: Option<Res<HostLight>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut host_materials: ResMut<Assets<ViewmodelMaterial>>,
    mut images: ResMut<Assets<Image>>,
    ropes: Query<Entity, With<GrappleRope>>,
) {
    let Some(g) = &data.0.grapple else { return };
    let mut material = |e: &mut EntityCommands, m: StandardMaterial| match host {
        Some(_) => drop(e.remove::<MeshMaterial3d<StandardMaterial>>().insert(MeshMaterial3d(host_materials.add(host_lit(m))))),
        None => drop(e.insert(MeshMaterial3d(materials.add(m)))),
    };
    let albedo = g.rope.first().and_then(|t| t.as_ref()).and_then(gpu_image).map(|i| images.add(i));
    let normal = g.rope.get(1).and_then(|t| t.as_ref()).and_then(gpu_image).map(|i| images.add(i));
    for e in &ropes {
        material(&mut commands.entity(e), StandardMaterial { base_color_texture: albedo.clone(), normal_map_texture: normal.clone(), flip_normal_map_y: true, perceptual_roughness: 0.6, ..default() });
    }
    for (m, textures) in g.hook.meshes.iter().zip(&g.hook_textures) {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions.iter().map(|&p| pos(p).to_array()).collect::<Vec<_>>())
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals.iter().map(|&n| dir(n).to_array()).collect::<Vec<_>>())
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs.clone())
            .with_inserted_indices(Indices::U16(m.indices.chunks(3).flat_map(|t| [t[0], t[2], t[1]]).collect()));
        let _ = mesh.generate_tangents();
        let base_color_texture = textures.first().and_then(|t| t.as_ref()).and_then(gpu_image).map(|i| images.add(i));
        let mut hook = commands.spawn((GrappleHook, Mesh3d(meshes.add(mesh)), NotShadowCaster, Visibility::Hidden, Transform::default()));
        material(&mut hook, StandardMaterial { base_color_texture, perceptual_roughness: 0.5, ..default() });
    }
}

/// The rope as the client builds it (`lurch::rope`, `0x6c0810`), from the hook through its
/// bends to the hand, as a tube; the hook faces away from the eye (`0x9d1b00`).
#[allow(clippy::type_complexity)]
fn sync_rope(
    sim: Res<Sim>,
    data: Option<Res<crate::data::Data>>,
    held: Res<crate::viewmodel::GrappleHand>,
    mut hand: Local<lurch::rope::Hand>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut ropes: Query<(&Mesh3d, &mut Visibility), (With<GrappleRope>, Without<GrappleHook>)>,
    mut hooks: Query<(&mut Transform, &mut Visibility), With<GrappleHook>>,
) {
    let p = &sim.pilot.player;
    let now = sim.view_now;
    let n = p.grapple_count.clamp(0, 4) as usize;
    let eye = p.eye_position(&sim.settings, sim.pilot.origin, p.view_angles, now);
    let held = held.0.filter(|_| p.first_person_anim == lurch::sim::grapple::FP_ANIM_GRAPPLE);
    let at = hand.at(held, eye, p.view_angles, now);
    let shown = if p.grappling && n > 0 { Visibility::Visible } else { Visibility::Hidden };
    for (mut t, mut vis) in &mut hooks {
        let h = p.grapple_hook_at;
        let d = [h[0] - eye[0], h[1] - eye[1], h[2] - eye[2]];
        let near = d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < HOOK_HIDDEN * HOOK_HIDDEN;
        *vis = if near { Visibility::Hidden } else { shown };
        let pitch = (-d[2]).atan2((d[0] * d[0] + d[1] * d[1]).sqrt()).to_degrees();
        *t = Transform::from_translation(pos(h)).with_rotation(rot([pitch, d[1].atan2(d[0]).to_degrees(), 0.0]));
    }
    for (mesh, mut vis) in &mut ropes {
        *vis = shown;
        if shown == Visibility::Hidden {
            continue;
        }
        let g = &sim.settings.grapple;
        let tight = lurch::rope::tightness(p.grapple_pulling, p.grapple_retracting, p.grapple_attached, p.grapple_attach_time, GRAPPLE_PULL_DELAY, now);
        let points = lurch::rope::points(p.grapple_hook_at, &p.grapple_points[..n], at, tight, g.weapon.max_length, now);
        let width = data.as_ref().and_then(|d| d.0.grapple.as_ref()?.rope.first()?.as_ref().map(|t| t.width)).unwrap_or(64) as f32;
        let _ = meshes.insert(&mesh.0, tube(&points, lurch::rope::WIDTH * 0.5, lurch::rope::SIDES, lurch::rope::ROPE_TEXELS_PER_WORLD_UNIT / width));
    }
}

/// Over another game's world the main camera draws only the grapple's rope and hook, so while
/// neither shows it is off and the viewmodel's camera clears the image in its place: a camera's
/// passes fewer each frame. The viewmodel's camera blends its output as the second camera on the
/// image does, alone too, so the image's edges stay as they were.
#[allow(clippy::type_complexity)]
fn overlay_cameras(target: Res<ViewTarget>, grapple: Query<&Visibility, Or<(With<GrappleRope>, With<GrappleHook>)>>, mut main: Query<&mut Camera, (With<MainCamera>, Without<ViewmodelCamera>)>, mut viewmodel: Query<&mut Camera, With<ViewmodelCamera>>) {
    if !matches!(target.0, Target::Overlay { .. }) || viewmodel.is_empty() {
        return;
    }
    let drawn = grapple.iter().any(|v| *v != Visibility::Hidden);
    for mut c in &mut main {
        if c.is_active != drawn {
            c.is_active = drawn;
        }
    }
    for mut c in &mut viewmodel {
        if matches!(c.clear_color, ClearColorConfig::None) != drawn {
            c.clear_color = if drawn { ClearColorConfig::None } else { ClearColorConfig::Custom(Color::NONE) };
        }
        if !matches!(c.output_mode, CameraOutputMode::Write { blend_state: Some(_), .. }) {
            c.output_mode = CameraOutputMode::Write { blend_state: Some(BlendState::ALPHA_BLENDING), clear_color: ClearColorConfig::Default };
        }
    }
}

/// Ours: the hook leaves from the eye, and closer than this to it is not drawn, so it does not
/// fill the view as it fires and comes back.
const HOOK_HIDDEN: f32 = 48.0;

/// ConVar `grapple_pullDelay_human`, which the rope tightens over once the hook holds.
const GRAPPLE_PULL_DELAY: f32 = 0.2;

/// A tube of `sides` round `points` at `radius`, its texture along the rope at `v_per_unit`.
fn tube(points: &[V3], radius: f32, sides: usize, v_per_unit: f32) -> Mesh {
    let (mut positions, mut normals, mut uvs, mut indices) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut along = 0.0;
    let mut side = [0.0f32, 0.0, 1.0];
    for (i, &p) in points.iter().enumerate() {
        let (a, b) = (points[i.saturating_sub(1)], points[(i + 1).min(points.len() - 1)]);
        let t = Vec3::from_array(b) - Vec3::from_array(a);
        let t = t.try_normalize().unwrap_or(Vec3::X);
        if i > 0 {
            along += (Vec3::from_array(p) - Vec3::from_array(points[i - 1])).length();
        }
        let mut s = Vec3::from_array(side) - t * t.dot(Vec3::from_array(side));
        if s.length_squared() < 1e-6 {
            s = t.any_orthonormal_vector();
        }
        let s = s.normalize();
        side = s.to_array();
        let w = t.cross(s);
        for k in 0..=sides {
            let a = k as f32 / sides as f32 * std::f32::consts::TAU;
            let n = s * a.cos() + w * a.sin();
            positions.push(pos((Vec3::from_array(p) + n * radius).to_array()).to_array());
            normals.push(dir(n.to_array()).to_array());
            uvs.push([k as f32 / sides as f32, along * v_per_unit]);
        }
    }
    let ring = sides as u32 + 1;
    for i in 0..points.len().saturating_sub(1) as u32 {
        for k in 0..sides as u32 {
            let (a, b, c, d) = (i * ring + k, i * ring + k + 1, (i + 1) * ring + k, (i + 1) * ring + k + 1);
            indices.extend([a, c, b, b, c, d]);
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

fn log_display(windows: Query<&Window, With<PrimaryWindow>>, adapter: Option<Res<RenderAdapterInfo>>, mut info: ResMut<DisplayInfo>) {
    info.present = windows.iter().next().map_or("none".into(), |w| format!("{:?}", w.present_mode));
    info.adapter = adapter.map_or("none".into(), |a| a.name.clone());
    info!("present mode {} (requested), adapter {}", info.present, info.adapter);
}

/// Paused shows the cursor and frees it; playing hides and locks it (X11 falls back to
/// confined). A focus change drops the grab, so resuming re-grabs.
fn apply_cursor(paused: Res<Paused>, mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    if !paused.is_changed() {
        return;
    }
    for mut c in &mut cursors {
        (c.visible, c.grab_mode) = if paused.0 { (true, CursorGrabMode::None) } else { (false, CursorGrabMode::Locked) };
    }
}

/// Without a script the app exits once its screenshot is saved; with one, `script_end` waits
/// for it.
fn take_shots(mut commands: Commands, frame: Res<FrameIndex>, cfg: Res<Config>, mut pending: ResMut<PendingShots>) {
    let Some((at, path)) = &cfg.shot else { return };
    if frame.0 != *at || cfg.headless {
        return;
    }
    pending.0 += 1;
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone())).observe(
        |_: On<ScreenshotCaptured>, mut pending: ResMut<PendingShots>, cfg: Res<Config>, mut exit: MessageWriter<AppExit>| {
            pending.0 -= 1;
            if pending.0 == 0 && !cfg.scripted {
                exit.write(AppExit::Success);
            }
        },
    );
}
