use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::RenderTarget;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use bevy::render::render_resource::{TextureFormat, TextureUsages};
use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
use lurch::sim::math::V3;
use lurch::testmap;
use lurch::world::Tri;

use crate::convert::{dir, pos, rot, vfov, INCH};
use crate::sim::{Pose, Sim};
use crate::{advance_frame, common, Config, Frame, FrameIndex, PendingShots};

pub const SKY: Color = Color::srgb(0.55, 0.70, 0.85);
pub const BODY_LAYER: usize = 2;
const TILE: f32 = 256.0;
const FLOOR: f32 = 16384.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Window,
    Image { w: u32, h: u32 },
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

/// The image the main camera renders into, when it is not a window.
#[derive(Resource, Clone)]
pub struct ViewImage(pub Handle<Image>);

#[derive(Resource, Clone, Copy)]
struct ViewTarget(Target);

impl Plugin for ViewPlugin {
    fn build(&self, app: &mut App) {
        common(app);
        app.insert_resource(ViewTarget(self.target))
            .insert_resource(ClearColor(SKY))
            .insert_resource(GlobalAmbientLight { brightness: 2000.0, ..default() })
            .add_systems(Startup, setup)
            .add_systems(Update, (sync_camera, sync_body).in_set(Frame::View))
            .add_systems(Last, take_shots.before(advance_frame));
    }
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

/// The test map as one flat-shaded mesh: the floor drawn as a checker (collision keeps its
/// two triangles), everything else tinted.
pub fn map_mesh() -> Mesh {
    let tris = testmap::build().tris;
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
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_computed_flat_normals()
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut images: ResMut<Assets<Image>>, target: Res<ViewTarget>, pose: Res<Pose>) {
    let material = StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 1.0, cull_mode: None, double_sided: true, ..default() };
    commands.spawn((Mesh3d(meshes.add(map_mesh())), MeshMaterial3d(materials.add(material))));
    commands.spawn((
        DirectionalLight { shadow_maps_enabled: false, ..default() },
        Transform::default().looking_to(Vec3::new(-0.4, -1.0, -0.3), Vec3::Y),
        RenderLayers::from_layers(&[0, 1, BODY_LAYER]),
    ));
    let projection = Projection::Perspective(PerspectiveProjection { near: 0.05, far: 2000.0, fov: vfov(pose.0.fov), ..default() });
    let transform = Transform::from_translation(pos(pose.0.origin)).with_rotation(rot(pose.0.angles));
    let mut camera = commands.spawn((Camera3d::default(), MainCamera, projection, transform));
    if let Target::Image { w, h } = target.0 {
        let mut image = Image::new_target_texture(w, h, TextureFormat::Rgba8UnormSrgb, None);
        image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let handle = images.add(image);
        camera.insert(RenderTarget::Image(handle.clone().into()));
        commands.insert_resource(ViewImage(handle));
    }
    let grey = StandardMaterial { base_color: Color::srgb(0.6, 0.6, 0.6), ..default() };
    commands.spawn((Body, Mesh3d(meshes.add(Cuboid::default())), MeshMaterial3d(materials.add(grey)), RenderLayers::layer(BODY_LAYER), Transform::default()));
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
