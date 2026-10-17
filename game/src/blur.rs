//! The screen blur (`0x756a00`), which runs each frame while ConVar `mat_screen_blur_enabled`
//! is on: the frame shrunk to a quarter each way into `_rt_SmallFB0` (material
//! `dev/downsample_backbuffer`), blurred across into `_rt_SmallFB1` and back down into
//! `_rt_SmallFB0` (material `dev/blurfilter`). The HUD's special image source draws from it
//! (`docs/ui.md`). Here it runs on the camera that draws the HUD, between its opaque and
//! transparent passes, so it holds the scene and the arms but not the HUD.
//!
//! Not checked against the game: whether the game's frame and `_rt_SmallFB0` are sRGB (the
//! blur here averages the target's linear values) and the engine's screen rectangle draw,
//! taken to map each target pixel's centre straight onto the source.

use std::num::NonZero;

use bevy::core_pipeline::core_3d::{main_opaque_pass_3d, main_transparent_pass_3d};
use bevy::core_pipeline::{Core3d, FullscreenShader};
use bevy::prelude::*;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::binding_types::{sampler, texture_2d, uniform_buffer_sized};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, Buffer, BufferDescriptor, BufferUsages, CachedRenderPipelineId, ColorTargetState, ColorWrites, FilterMode, FragmentState, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages, TextureFormat, TextureSampleType, TextureView,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::texture::GpuImage;
use bevy::render::view::ViewTarget;
use bevy::render::{RenderApp, RenderStartup};

const SHADER: &str = "embedded://lurch_game/blur.wgsl";
/// `_rt_SmallFB0`'s format: the target's.
const FORMAT: TextureFormat = TextureFormat::Rgba8UnormSrgb;
/// `dev/blurfilter`'s `$kernel 4`: 25 texels wide in the kernel table (`0x1362fc0`), not scaled
/// with the screen (`0x1363048`), and capped at 18 when the table is built (`0x41c8da`).
const KERNEL_WIDTH: i32 = 25;
const MAX_KERNEL_WIDTH: i32 = 18;
/// The most taps the table takes from the generator (`0x41c8df`).
const MAX_TAPS: usize = 19;
/// `CBufBlurFilter` then `CBufBlurFilterDynamic`, as `blur.wgsl` reads them in one buffer.
const UNIFORM_SIZE: u64 = 192;

pub struct BlurPlugin;

impl Plugin for BlurPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "blur.wgsl");
        app.add_plugins((ExtractResourcePlugin::<ScreenBlur>::default(), ExtractComponentPlugin::<BlurSource>::default()));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app.add_systems(RenderStartup, init_pipelines).add_systems(Core3d, screen_blur.after(main_opaque_pass_3d).before(main_transparent_pass_3d));
    }
}

/// `_rt_SmallFB0` and `_rt_SmallFB1`, a quarter of the backbuffer each way, for a backbuffer
/// `backbuffer` pixels across and down.
#[derive(Resource, Clone, ExtractResource)]
pub struct ScreenBlur {
    pub small: [Handle<Image>; 2],
    pub size: [u32; 2],
    pub backbuffer: [u32; 2],
}

impl ScreenBlur {
    pub fn new(images: &mut Assets<Image>, backbuffer: [u32; 2]) -> ScreenBlur {
        let size = backbuffer.map(|x| (x / 4).max(1));
        let mut make = || images.add(Image::new_target_texture(size[0], size[1], FORMAT, None));
        ScreenBlur { small: [make(), make()], size, backbuffer }
    }

    /// The special atlas's one image in its texture (`0x2f4a73`): mins `1 / size`, then the
    /// size, `1 - mins`.
    pub fn bounds(&self) -> [f32; 4] {
        let m = self.size.map(|x| 1.0 / x as f32);
        [m[0], m[1], 1.0 - m[0], 1.0 - m[1]]
    }
}

/// The camera whose frame the blur reads: the one that draws the HUD.
#[derive(Component, Clone, ExtractComponent)]
pub struct BlurSource;

/// A Gaussian `n` texels wide as bilinear taps (`0x459e60`): `(offset, weight)` pairs, the
/// centre's first. The weights `exp(-(i k)^2 / 2)` for `i` up to `n / 2`, with
/// `k = 4 / ((n + 1) / 2)`, are scaled to sum to one over both sides; then each pair of
/// neighbours from 1 on becomes one tap between them, and an odd one out a tap of its own, up
/// to `max` taps.
pub fn gaussian_taps(n: i32, max: usize) -> Vec<[f32; 2]> {
    let count = (n / 2 + 1) as usize;
    let k = 4.0 / ((n + 1) as f32 * 0.5);
    let mut w: Vec<f32> = (0..count)
        .map(|i| {
            let x = (i as f32 - 0.0) * k;
            (x * -0.5 * x).exp()
        })
        .collect();
    let mut sum = w[0];
    for &x in &w[1..] {
        sum += x + x;
    }
    let r = 1.0 / sum;
    for x in &mut w {
        *x *= r;
    }
    let mut taps = vec![[0.0, w[0]]];
    let mut i = 1;
    while i < count && taps.len() < max {
        let a = i as f32;
        taps.push(if i + 1 < count {
            let both = w[i + 1] + w[i];
            [(w[i + 1] / both) * ((i + 1) as f32 - a) + a, both]
        } else {
            [a, w[i]]
        });
        i += 2;
    }
    taps
}

/// One kernel of the table `BlurFilter` builds (`0x41c720`): the count of side tap packs, the
/// centre's weight, and the packs, two taps each, an odd last tap's pack finished with a tap of
/// no weight at its offset.
fn kernel(taps: &[[f32; 2]]) -> (i32, f32, [[f32; 4]; 9]) {
    let mut packs = [[0.0; 4]; 9];
    let side = &taps[1..];
    for (i, t) in side.iter().enumerate() {
        packs[i / 2][(i % 2) * 2] = t[0];
        packs[i / 2][(i % 2) * 2 + 1] = t[1];
    }
    if side.len() % 2 == 1 {
        let last = side[side.len() - 1];
        packs[side.len() / 2] = [last[0], last[1], last[0], 0.0];
    }
    ((taps.len() / 2) as i32, taps[0][1], packs)
}

/// The blur shader's constants for one pass (`0x41c9b0`, `0x41cb70`): a texel's step across
/// (`direction` 0) or down (1) the source, the kernel, no clear colour (bias 0, scale 1), and
/// the range the taps are mirrored into (`0x756b49`): from half a texel in, twice
/// `1 - 1 / size` wide.
fn uniforms(size: [u32; 2], direction: u32, (packs, centre, taps): (i32, f32, [[f32; 4]; 9])) -> Vec<u8> {
    let (w, h) = (size[0] as i32, size[1] as i32);
    let step = if direction == 0 { [1.0 / w as f32, 0.0] } else { [0.0, 1.0 / h as f32] };
    let min_uv = [1.0 / (w + w) as f32, 1.0 / (h + h) as f32];
    let range = [(1.0f64 - (1.0 / w as f32) as f64) as f32, 1.0 - 1.0 / h as f32];
    let mut b = Vec::with_capacity(UNIFORM_SIZE as usize);
    for f in step {
        b.extend(f.to_le_bytes());
    }
    b.extend(packs.to_le_bytes());
    b.extend(centre.to_le_bytes());
    for f in [0.0f32, 0.0, 0.0, 1.0].into_iter().chain(taps.into_iter().flatten()).chain(min_uv).chain(range.map(|x| x + x)) {
        b.extend(f.to_le_bytes());
    }
    b
}

#[derive(Resource)]
struct BlurPipelines {
    downsample: CachedRenderPipelineId,
    blur: CachedRenderPipelineId,
    down_layout: BindGroupLayoutDescriptor,
    blur_layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    /// The across pass's constants, then the down pass's.
    constants: [Buffer; 2],
    kernel: (i32, f32, [[f32; 4]; 9]),
}

fn init_pipelines(mut commands: Commands, device: Res<RenderDevice>, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>, cache: Res<PipelineCache>) {
    let texture = || texture_2d(TextureSampleType::Float { filterable: true });
    let down_layout = BindGroupLayoutDescriptor::new("screen_blur_downsample", &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (texture(), sampler(SamplerBindingType::Filtering))));
    let blur_layout = BindGroupLayoutDescriptor::new(
        "screen_blur_filter",
        &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (texture(), sampler(SamplerBindingType::Filtering), uniform_buffer_sized(false, NonZero::new(UNIFORM_SIZE)))),
    );
    let shader = assets.load(SHADER);
    let pipeline = |label: &'static str, entry: &'static str, layout: &BindGroupLayoutDescriptor| {
        cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some(label.into()),
            layout: vec![layout.clone()],
            vertex: fullscreen.to_vertex_state(),
            fragment: Some(FragmentState { shader: shader.clone(), entry_point: Some(entry.into()), targets: vec![Some(ColorTargetState { format: FORMAT, blend: None, write_mask: ColorWrites::ALL })], ..default() }),
            ..default()
        })
    };
    let buffer = || device.create_buffer(&BufferDescriptor { label: Some("screen_blur_constants"), size: UNIFORM_SIZE, usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST, mapped_at_creation: false });
    commands.insert_resource(BlurPipelines {
        downsample: pipeline("screen_blur_downsample", "downsample", &down_layout),
        blur: pipeline("screen_blur_filter", "blur", &blur_layout),
        down_layout,
        blur_layout,
        sampler: device.create_sampler(&SamplerDescriptor { mag_filter: FilterMode::Linear, min_filter: FilterMode::Linear, ..default() }),
        constants: [buffer(), buffer()],
        kernel: kernel(&gaussian_taps(KERNEL_WIDTH.min(MAX_KERNEL_WIDTH), MAX_TAPS)),
    });
}

#[allow(clippy::too_many_arguments)] // Bevy's system parameters
fn screen_blur(view: ViewQuery<&ViewTarget, With<BlurSource>>, blur: Option<Res<ScreenBlur>>, pipes: Option<Res<BlurPipelines>>, cache: Res<PipelineCache>, images: Res<RenderAssets<GpuImage>>, device: Res<RenderDevice>, queue: Res<RenderQueue>, mut ctx: RenderContext) {
    let target = view.into_inner();
    let (Some(blur), Some(pipes)) = (blur, pipes) else { return };
    let (Some(downsample), Some(filter)) = (cache.get_render_pipeline(pipes.downsample), cache.get_render_pipeline(pipes.blur)) else { return };
    let (Some(fb0), Some(fb1)) = (images.get(&blur.small[0]), images.get(&blur.small[1])) else { return };
    for (direction, buffer) in pipes.constants.iter().enumerate() {
        queue.write_buffer(buffer, 0, &uniforms(blur.size, direction as u32, pipes.kernel));
    }
    let down = device.create_bind_group("screen_blur_downsample", &cache.get_bind_group_layout(&pipes.down_layout), &BindGroupEntries::sequential((target.main_texture_view(), &pipes.sampler)));
    let filter_from = |source: &TextureView, buffer: &Buffer| device.create_bind_group("screen_blur_filter", &cache.get_bind_group_layout(&pipes.blur_layout), &BindGroupEntries::sequential((source, &pipes.sampler, buffer.as_entire_binding())));
    let across = filter_from(&fb0.texture_view, &pipes.constants[0]);
    let back = filter_from(&fb1.texture_view, &pipes.constants[1]);
    for (label, pipeline, group, dest) in [("screen_blur_downsample", downsample, &down, &fb0.texture_view), ("screen_blur_across", filter, &across, &fb1.texture_view), ("screen_blur_down", filter, &back, &fb0.texture_view)] {
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(RenderPassColorAttachment { view: dest, depth_slice: None, resolve_target: None, ops: Operations::default() })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
}
