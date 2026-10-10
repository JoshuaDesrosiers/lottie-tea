// Bloom (`PostEffect::Glow`) the way real-time engines do it: a mip chain,
// not a full-resolution blur. The bright part of the image is thresholded
// while it's downsampled to 1/2 size, then repeatedly halved (1/4, 1/8 …),
// then walked back up, each level blurred a little more and *added* onto
// the next larger one. The result (at 1/2 size) is a wide, smooth glow for
// a small fraction of the cost of blurring at full resolution: every level
// is a quarter the pixels of the one above it.
//
// The filters are the "dual Kawase" pair (Marius Bjørge, SIGGRAPH 2015):
// 5 bilinear taps down, 8 up. Bilinear taps at half-texel offsets read four
// texels each, so the kernels are wider than their tap counts suggest.
//
// Bind group: the shared post-process layout (`post_process.wgsl`):
// `params.texel_size_radius` = (source texel width, height, spread, -),
// `params.threshold_intensity` = (threshold, -, apply-threshold flag, -).

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

struct PostParams {
    texel_size_radius: vec4<f32>,
    threshold_intensity: vec4<f32>,
    unused: vec4<f32>,
};

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> params: PostParams;

/// The bright pass, with a short soft knee so pixels hovering at the
/// threshold don't flicker in and out of the glow frame to frame.
fn bright(c: vec4<f32>) -> vec4<f32> {
    if (params.threshold_intensity.z < 0.5) {
        return c;
    }
    let threshold = params.threshold_intensity.x;
    let level = max(c.r, max(c.g, c.b));
    let keep = smoothstep(threshold - 0.04, threshold + 0.04, level);
    return c * keep;
}

fn tap(uv: vec2<f32>) -> vec4<f32> {
    return bright(textureSample(source, source_sampler, uv));
}

/// Downsample: the centre weighted 4, four diagonal half-texel taps.
@fragment
fn fs_down(in: VertexOutput) -> @location(0) vec4<f32> {
    let h = params.texel_size_radius.xy * 0.5;
    var sum = tap(in.uv) * 4.0;
    sum = sum + tap(in.uv - h);
    sum = sum + tap(in.uv + h);
    sum = sum + tap(in.uv + vec2<f32>(h.x, -h.y));
    sum = sum + tap(in.uv - vec2<f32>(h.x, -h.y));
    return sum / 8.0;
}

/// Upsample (tent): eight taps round the pixel, the diagonals doubled.
/// `spread` widens it (the effect's radius above the default).
@fragment
fn fs_up(in: VertexOutput) -> @location(0) vec4<f32> {
    let h = params.texel_size_radius.xy * 0.5 * max(params.texel_size_radius.z, 0.25);
    let s = source_sampler;
    var sum = textureSample(source, s, in.uv + vec2<f32>(-h.x * 2.0, 0.0));
    sum = sum + textureSample(source, s, in.uv + vec2<f32>(-h.x, h.y)) * 2.0;
    sum = sum + textureSample(source, s, in.uv + vec2<f32>(0.0, h.y * 2.0));
    sum = sum + textureSample(source, s, in.uv + vec2<f32>(h.x, h.y)) * 2.0;
    sum = sum + textureSample(source, s, in.uv + vec2<f32>(h.x * 2.0, 0.0));
    sum = sum + textureSample(source, s, in.uv + vec2<f32>(h.x, -h.y)) * 2.0;
    sum = sum + textureSample(source, s, in.uv + vec2<f32>(0.0, -h.y * 2.0));
    sum = sum + textureSample(source, s, in.uv + vec2<f32>(-h.x, -h.y)) * 2.0;
    return sum / 12.0;
}
