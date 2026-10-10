// The post-process "uber" pass: one full-screen draw that does what used to
// take one draw per effect. In order:
//
// 1. Pixelate (optional): snap the sample position to a block grid.
// 2. Sample the source image.
// 3. Bloom (optional): add a `Glow`'s bloom texture (half resolution,
//    bilinear-upscaled here) times its intensity.
// 4. Up to 8 per-pixel colour ops, in the stack's order: colour grading,
//    posterize, gradient map, vignette.
//
// The same pass is the final present (writing the swapchain, alpha-blended
// over the editor chrome) when it's the last thing in the chain, so a
// typical stack (Glow + grading + vignette) costs one full-resolution pass
// plus the small bloom chain — instead of five full-resolution passes.
//
// Op encoding (3 vec4 per op): `a.x` = mode (2 grading, 3 posterize,
// 4 gradient map, 7 vignette — the same numbers as `post_process.wgsl`),
// then the op's parameters in `a.yzw`, `b`, `c`.

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let uv = vec2<f32>(f32((vertex_index << 1u) & 2u), f32(vertex_index & 2u));
    out.clip_position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    return out;
}

struct Uber {
    // (texel width, texel height) of the post targets (not of the source,
    // which may be a half-resolution blur), pixelate block size (0 = off),
    // bloom intensity (0 = no bloom).
    texel: vec4<f32>,
    // (op count, -, -, -)
    info: vec4<f32>,
    ops: array<vec4<f32>, 24>,
};

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> u: Uber;
@group(0) @binding(3) var bloom: texture_2d<f32>;
@group(0) @binding(4) var bloom_sampler: sampler;

const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

fn apply(c_in: vec4<f32>, i: i32, uv: vec2<f32>) -> vec4<f32> {
    var c = c_in;
    let a = u.ops[i * 3];
    let b = u.ops[i * 3 + 1];
    let d = u.ops[i * 3 + 2];
    let mode = a.x;
    if (mode == 2.0) {
        // Colour grading: (brightness, contrast, saturation) in a.yzw.
        let luminance = dot(c.rgb, LUMA);
        var rgb = mix(vec3<f32>(luminance), c.rgb, a.w);
        rgb = (rgb - vec3<f32>(0.5)) * a.z + vec3<f32>(0.5);
        rgb = rgb + vec3<f32>(a.y);
        c = vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);
    } else if (mode == 3.0) {
        // Posterize: levels in a.y.
        let steps = max(a.y - 1.0, 1.0);
        c = vec4<f32>(round(c.rgb * steps) / steps, c.a);
    } else if (mode == 4.0) {
        // Gradient map: shadow colour b, highlight colour d.
        let luminance = dot(c.rgb, LUMA);
        c = vec4<f32>(mix(b.rgb, d.rgb, luminance), c.a);
    } else if (mode == 7.0) {
        // Vignette: colour b.rgb, (intensity, radius, blur) in a.yzw.
        let texel = u.texel.xy;
        let aspect = texel.y / texel.x;
        var centered = (uv - vec2<f32>(0.5)) * 2.0;
        centered.x = centered.x * aspect;
        let distance = length(centered) / length(vec2<f32>(aspect, 1.0));
        let edge = smoothstep(a.z - max(a.w, 0.001), a.z, distance);
        let k = clamp(edge * a.y, 0.0, 1.0);
        let alpha = c.a * (1.0 - k) + k;
        let premultiplied = c.rgb * c.a * (1.0 - k) + b.rgb * k;
        c = vec4<f32>(premultiplied / max(alpha, 0.0001), alpha);
    }
    return c;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var uv = in.uv;
    let block = u.texel.z;
    if (block > 0.0) {
        let block_uv = u.texel.xy * block;
        uv = (floor(uv / block_uv) + vec2<f32>(0.5)) * block_uv;
    }
    var c = textureSample(source, source_sampler, uv);
    // (Sampled unconditionally: a texture sample must sit in uniform
    // control flow. With no bloom, it's a 1×1 black texture.)
    let glow = textureSample(bloom, bloom_sampler, uv);
    let intensity = u.texel.w;
    if (intensity > 0.0) {
        c = vec4<f32>(c.rgb + glow.rgb * intensity, clamp(c.a + glow.a * intensity, 0.0, 1.0));
    }
    let count = i32(u.info.x);
    for (var i: i32 = 0; i < 8; i = i + 1) {
        if (i >= count) {
            break;
        }
        c = apply(c, i, in.uv);
    }
    return c;
}
