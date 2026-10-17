// The game's UI pixel shader (`code_private/ui`, pixel shader 0xa84403b56d2e69e3 in ui.rpak),
// ported from its disassembly: the text, image and shape modes and the tail every mode shares.
// The video mode draws nothing yet (docs/ui.md). Output is premultiplied: a style's
// `blend` of 0 adds.

#import bevy_pbr::mesh_view_bindings::view

struct Style {
    color: array<vec4<f32>, 3>,
    colorxfrm: array<vec4<f32>, 3>,
    tint: vec4<f32>,
    blend: f32,
    premul: f32,
    terms: array<f32, 7>,
    desaturate: f32,
    pad: vec2<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var image_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var bilinear: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var mask_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var mask_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var<storage, read> styles: array<Style>;
// A font's glyph bounds in its texture (`g_fontBounds`): mins, maxs.
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var<storage, read> glyphs: array<vec4<f32>>;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) uv: vec4<f32>,
    @location(2) text: vec2<f32>,
    @location(3) data: vec4<i32>,
    @location(4) bounds: vec4<f32>,
    @location(5) mask_bounds: vec4<f32>,
    @location(6) pixel: vec2<f32>,
};

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec4<f32>,
    @location(1) text: vec2<f32>,
    @location(2) @interpolate(flat) data: vec4<i32>,
    @location(3) @interpolate(flat) bounds: vec4<f32>,
    @location(4) @interpolate(flat) mask_bounds: vec4<f32>,
};

@vertex
fn vertex(v: Vertex) -> Varyings {
    var o: Varyings;
    // The quads are in the target's pixels, whatever camera draws them.
    let ndc = (v.pixel - view.viewport.xy) / view.viewport.zw * 2.0 - 1.0;
    o.clip = vec4(ndc.x, -ndc.y, 0.5, 1.0);
    o.uv = v.uv;
    o.text = v.text;
    o.data = v.data;
    o.bounds = v.bounds;
    o.mask_bounds = v.mask_bounds;
    return o;
}

// A frame coordinate wrapped where its repeat flag is set, else clamped (`round_ni`, `movc`,
// `add_sat`), then placed in the atlas.
fn place(p: vec2<f32>, repeat: vec2<bool>, bounds: vec4<f32>) -> vec2<f32> {
    let f = select(vec2(-0.0), -floor(p), repeat);
    return clamp(f + p, vec2(0.0), vec2(1.0)) * bounds.zw + bounds.xy;
}

// Four taps a quarter of a pixel's derivatives from `p`, averaged.
fn four_taps(t: texture_2d<f32>, s: sampler, p: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, repeat: vec2<bool>, bounds: vec4<f32>) -> vec4<f32> {
    let a = p - dx * 0.25;
    var sum = textureSampleLevel(t, s, place(a - dy * 0.25, repeat, bounds), 0.0);
    sum += textureSampleLevel(t, s, place(a + dy * 0.25, repeat, bounds), 0.0);
    let b = p + dx * 0.25;
    sum += textureSampleLevel(t, s, place(b - dy * 0.25, repeat, bounds), 0.0);
    sum += textureSampleLevel(t, s, place(b + dy * 0.25, repeat, bounds), 0.0);
    return sum;
}

// The font texture's signed distance at `p`, in its units (`sample × 12.75 + off − 10`).
fn distance(p: vec2<f32>, off: f32) -> f32 {
    return (textureSampleLevel(image_texture, bilinear, p, 0.0).x * 12.75 + off) + -10.0;
}

// One glyph of a text quad: whether the pixel is inside its bounds, then its fill (`main`),
// its fill grown by the style's outline (`outer`) and how far into the outline it is (`mix`),
// each over five taps (the centre and four at ±0.4 and ±0.2 of a pixel). `sharp` is the
// texels per pixel, `cap` the style's softness over the glyph's scale, `off` and `grow` the
// style's distance offset and outline width times that scale, `s` the glyph's scale.
fn glyph(p: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, b: vec4<f32>, sharp: f32, cap: f32, off: f32, grow: f32, t34: f32, s: f32) -> vec3<f32> {
    if !(all(p >= b.xy) && all(b.zw >= p)) {
        return vec3(0.0);
    }
    let wide = sharp * 2.236068;
    let k = min(wide, cap);
    let d = distance(p, off);
    let inner = k * d;
    if grow * k + inner < -1.0 {
        return vec3(0.0);
    }
    if 1.0 < inner {
        return vec3(1.0);
    }
    let mixv = clamp(d / max(grow, 0.0) + 1.0, 0.0, 1.0);
    var main = clamp(d * k + 0.5, 0.0, 1.0);
    var outer = clamp((t34 * s + d) * k + 0.5, 0.0, 1.0);
    let a = p - dx * 0.4;
    let b2 = p - dx * 0.2;
    let taps = array<vec2<f32>, 4>(a - dy * 0.2, dy * 0.4 + b2, (dx * 0.2 + p) - dy * 0.4, dy * 0.2 + (dx * 0.4 + p));
    for (var i = 0; i < 4; i++) {
        let di = distance(taps[i], off);
        main += clamp(di * k + 0.5, 0.0, 1.0);
        outer += clamp((t34 * s + di) * wide + 0.5, 0.0, 1.0);
    }
    return vec3(main * 0.2, outer * 0.2, mixv);
}

// A drop shadow's cover of one glyph at `p` less the style's shadow offset.
fn shadow(p: vec2<f32>, b: vec4<f32>, t33: f32, inv_s: f32, sharp: f32, off: f32) -> f32 {
    if !(all(p >= b.xy) && all(b.zw >= p)) {
        return 0.0;
    }
    let d = distance(p, off);
    return clamp(d * min(t33 * inv_s, sharp * 2.236068) + 0.5, 0.0, 1.0);
}

@fragment
fn fragment(in: Varyings) -> @location(0) vec4<f32> {
    let flags = in.data.w;
    let mode = flags >> 12u;
    let st = styles[in.data.z];
    var color = vec3(0.0);
    var tex_a = 0.0;
    var aa = 1.0;
    var alpha = 0.0;
    var premul = 1.0;
    let ddx = dpdxCoarse(in.uv);
    let ddy = dpdyCoarse(in.uv);
    // The shape mode's distances, here for their derivatives: from the disc's edge, and
    // across the two angles' half-planes.
    let s = vec4(st.terms[3], st.terms[4], st.terms[5], st.terms[6]);
    let r = vec2(s.y * in.uv.z, s.x * in.uv.w);
    let a0 = st.terms[1];
    let a1 = st.terms[2];
    let sd = vec3(s.x * s.y - sqrt(dot(r, r)), dot(in.uv.zw, vec2(cos(a0), sin(a0))), dot(in.uv.zw, vec2(-cos(a1), sin(-a1))));
    let sdx = dpdxCoarse(sd);
    let sdy = dpdyCoarse(sd);
    if mode == 0 {
        let ga = glyphs[in.data.x & 0xffff];
        let gb = glyphs[in.data.y & 0xffff];
        let t = st.terms;
        let size = vec2<f32>(textureDimensions(image_texture));
        let px = ddx.xzyw * size.xxyy;
        let py = ddy.xzyw * size.xxyy;
        let qx = px * px;
        let qy = py * py;
        let sharp = inverseSqrt(max(qy.zw + qy.xy, qx.zw + qx.xy));
        let inv_s = 1.0 / in.text;
        let cap = t[6] * inv_s;
        let a = glyph(in.uv.xy, ddx.xy, ddy.xy, ga, sharp.x, cap.x, t[5] * in.text.x, t[4] * in.text.x, t[4], in.text.x);
        let b = glyph(in.uv.zw, ddx.zw, ddy.zw, gb, sharp.y, cap.y, t[5] * in.text.y, t[4] * in.text.y, t[4], in.text.y);
        let m = max(b, a);
        let glow = m.y - m.x;
        let outline = m.z * (st.color[1] - st.color[2]) + st.color[2];
        let glow_a = glow * outline.w;
        let main_a = m.x * st.color[0].w;
        color = outline.xyz * glow_a + main_a * st.color[0].xyz;
        tex_a = outline.w * glow + main_a;
        if tex_a < 1.0 && 0.0 < t[2] {
            let off = vec2(t[0], t[1]);
            let sa = shadow(in.uv.xy - off * in.text.x, ga, t[3], inv_s.x, sharp.x, t[5] * in.text.x);
            let sb = shadow(in.uv.zw - off * in.text.y, gb, t[3], inv_s.y, sharp.y, t[5] * in.text.y);
            let sh = t[2] * max(sa, sb);
            tex_a = (sh - tex_a * sh) + tex_a;
        }
        alpha = 1.0;
    } else if mode == 1 {
        premul = st.premul;
        let repeat = vec2((flags & 16) != 0, (flags & 32) != 0);
        let sum = four_taps(image_texture, bilinear, in.uv.xy, ddx.xy, ddy.xy, repeat, in.bounds);
        var img = vec4(sum.xyz * 0.25, clamp(sum.w * 0.25 + f32(flags & 4), 0.0, 1.0));
        if in.data.y >= 0 {
            let mrepeat = vec2((flags & 64) != 0, (flags & 128) != 0);
            let m = four_taps(mask_texture, mask_sampler, in.uv.zw, ddx.zw, ddy.zw, mrepeat, in.mask_bounds);
            let mv = vec4(m.xyz * 0.25, clamp(m.w * 0.25 + f32(flags & 8), 0.0, 1.0));
            let tinted = mv * (st.color[2] - st.color[1]) + st.color[1];
            img = select(img * tinted, img + tinted, (flags & 1) != 0);
        }
        if (flags & 3840) != 3840 {
            let inv = inverseSqrt(ddy.zw * ddy.zw + ddx.zw * ddx.zw);
            let off = vec4(f32((flags >> 8u) & 1), f32((flags >> 9u) & 1), f32((flags >> 10u) & 1), f32((flags >> 11u) & 1));
            let near = clamp(inv * in.uv.zw * 0.7 + 0.5, vec2(0.0), vec2(1.0));
            let far = clamp(inv * (1.0 - in.uv.zw) * 0.7 + 0.5, vec2(0.0), vec2(1.0));
            let lo = max(off.xz, near);
            let hi = max(far, off.yw);
            let cover = max(hi + lo - 1.0, vec2(0.0));
            aa = cover.y * cover.x;
        }
        color = st.color[0].xyz * img.xyz;
        alpha = st.color[0].w;
        tex_a = img.w;
    } else if mode != 3 {
        // A procedural shape in the crop lanes' [-1, 1] box: a disc of radius `s.x * s.y`
        // over an ellipse scaled by `s.y`, `s.x`, soft by `s.w` and by a pixel; within it the
        // colour runs from `color[1]` (or the `color[2]` times term 0) inside the ring of
        // width `s.z` to `color[0]` (or `color[2]`) outside, and between the half-planes of
        // the angles in terms 1 and 2 to the one or the other.
        premul = st.premul;
        let d = sd.x;
        let ig = inverseSqrt(dot(vec2(sdx.x, sdy.x), vec2(sdx.x, sdy.x)));
        let edge = min(clamp(d * s.w + 0.5, 0.0, 1.0), clamp(ig * d, 0.0, 1.0));
        if 0.0 >= edge {
            discard;
        }
        let ring = clamp((d - s.z * (s.y * s.x)) * ig + 0.5, 0.0, 1.0);
        let g0v = vec2(sdx.y, sdy.y);
        let g1v = vec2(sdx.z, sdy.z);
        let g0 = clamp(sd.y * inverseSqrt(dot(g0v, g0v)) + 0.5, 0.0, 1.0);
        let g1 = clamp(sd.z * inverseSqrt(dot(g1v, g1v)) + 0.5, 0.0, 1.0);
        let side = select(min(g1, g0), min(g1 + g0, 1.0), a1 - a0 >= 3.141593);
        let far = vec4(st.terms[0] * st.color[2].xyz, min(st.color[1].w, st.color[2].w));
        let outer = ring * (st.color[2] - st.color[0]) + st.color[0];
        let inner = ring * (far - st.color[1]) + st.color[1];
        let c = side * (outer - inner) + inner;
        let repeat = vec2((flags & 16) != 0, (flags & 32) != 0);
        let sum = four_taps(image_texture, bilinear, in.uv.xy, ddx.xy, ddy.xy, repeat, in.bounds);
        color = sum.xyz * c.xyz * 0.25;
        tex_a = clamp(sum.w * 0.25 + f32(flags & 4), 0.0, 1.0);
        alpha = c.w * edge;
    } else {
        discard;
    }
    let p = premul - tex_a * premul;
    let keep = 1.0 - p;
    let cover = p + tex_a;
    let xf = vec3(
        dot(st.colorxfrm[0].xyz, color) + keep * st.colorxfrm[0].w,
        dot(st.colorxfrm[1].xyz, color) + keep * st.colorxfrm[1].w,
        dot(st.colorxfrm[2].xyz, color) + keep * st.colorxfrm[2].w,
    );
    let a = st.tint.w * (aa * alpha);
    let tinted = st.tint.xyz * xf;
    let lum = dot(tinted, vec3(0.2126, 0.7152, 0.0722));
    let desat = select(tinted, st.desaturate * (lum - xf * st.tint.xyz) + tinted, st.desaturate != 0.0);
    return vec4(cover * a * desat, st.blend * (a * tex_a));
}
