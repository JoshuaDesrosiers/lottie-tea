// Post-processing shader (Phase 11, AN1MBYT3_BUILD_PLAN.md §10): samples a
// rendered scene texture and outputs a plain copy, a blur, a
// thresholded-then-blurred "bright pass" (the glow effect's second half —
// see `renderer.rs`'s `render_with_post_effect` for how the bright pass gets
// additively composited back over the plain scene to form actual glow), a
// pixelation mosaic, a brightness/contrast/saturation grade, a posterize
// (channel quantization), or a two-color luminance gradient map (duotone).
//
// One flexible shader covering every effect (rather than one shader per
// effect) — `params.texel_size_radius.w` is a mode discriminant Rust writes
// per `PostEffect` variant (see `renderer.rs`'s `PostUniforms` doc comment):
// `0.0` is a plain present/copy, `1.0` is pixelate, `2.0` is color grading,
// `3.0` is posterize, `4.0` is gradient map, `5.0`/`6.0` are the horizontal/
// vertical halves of a real separable Gaussian blur (`Blur`/`Glow`'s bright
// pass both ride on these two — see `separable_blur`'s own doc comment for
// why a blur is two passes now instead of one nested-loop box blur).

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// Full-screen triangle from 3 bare vertex indices — no vertex buffer needed.
// A triangle (not two triangles/a quad) because it's the standard trick for
// covering the viewport with the fewest vertices/draw calls; the corners
// outside the visible viewport just get clipped.
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let uv = vec2<f32>(f32((vertex_index << 1u) & 2u), f32(vertex_index & 2u));
    out.clip_position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    // Texture v=0 is the top row in wgpu's convention, but clip-space y=+1
    // is also "up" — flipping here (not the vertex position) is what makes
    // the sampled image come out right-side-up.
    out.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    return out;
}

struct PostParams {
    // (texel_width, texel_height, blur_radius, unused) — packed into one
    // vec4 (rather than separate scalar fields) purely to sidestep WGSL
    // uniform-buffer alignment rules by construction: two vec4s are
    // naturally 16-byte aligned with no padding math required.
    texel_size_radius: vec4<f32>,
    // (brightness_threshold, output_intensity, apply_threshold_flag, unused)
    // in glow mode; (brightness, contrast, saturation, unused) in
    // color-grading mode; the gradient map's `shadow` color in gradient-map
    // mode.
    threshold_intensity: vec4<f32>,
    // Gradient-map mode only: the `highlight` color. Unused (zeroed) by
    // every other mode.
    gradient_map_highlight: vec4<f32>,
};

@group(0) @binding(0) var scene_texture: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;
@group(0) @binding(2) var<uniform> params: PostParams;

// Tap count/spread for the separable Gaussian below — a fixed budget (not
// derived from `radius`, which only scales each tap's *spacing*) so this
// never becomes a variable-length/unbounded loop: 17 taps per direction (34
// total for a full blur) vs. the old single-pass box blur's 81, for a
// visually equivalent spread (the old kernel's own 9x9 footprint) but with
// a real bell-curve falloff instead of a flat average (no banding) and
// under half the total texture reads.
const BLUR_TAPS: i32 = 8;
const BLUR_SIGMA: f32 = 3.0;

fn gaussian_weight(i: f32) -> f32 {
    return exp(-(i * i) / (2.0 * BLUR_SIGMA * BLUR_SIGMA));
}

/// One direction of a separable Gaussian blur — `direction` is a unit step
/// in texel units (`vec2(texel_size.x, 0.0)` for the horizontal pass,
/// `vec2(0.0, texel_size.y)` for the vertical one); `radius` scales that
/// step the same way the old box blur scaled its own sample spacing, so
/// existing authored `radius` values still read the same. Two calls to this
/// (one per direction, chained through an intermediate texture — see
/// `renderer.rs`'s `render_with_post_effects`) is mathematically equivalent
/// to one full 2D Gaussian blur, the standard "separable" identity every
/// real-time blur implementation relies on to avoid an O(radius²) full 2D
/// kernel.
fn separable_blur(uv: vec2<f32>, direction: vec2<f32>, radius: f32) -> vec4<f32> {
    var sum = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    var weight_sum = 0.0;
    for (var i: i32 = -BLUR_TAPS; i <= BLUR_TAPS; i = i + 1) {
        let w = gaussian_weight(f32(i));
        let sampled = textureSample(scene_texture, scene_sampler, uv + direction * f32(i) * radius);
        sum = sum + sampled * w;
        weight_sum = weight_sum + w;
    }
    return sum / weight_sum;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel_size = params.texel_size_radius.xy;
    let mode = params.texel_size_radius.w;

    if (mode == 1.0) {
        // Pixelate: snap the sample UV onto a grid of `block_size`-pixel
        // cells (square in *pixel* space, per `PostEffect::Pixelate`'s doc
        // comment) — sampling the center of each cell is what makes every
        // pixel within a block read the same solid color.
        let block_size_uv = texel_size * params.texel_size_radius.z;
        let snapped_uv = (floor(in.uv / block_size_uv) + vec2<f32>(0.5, 0.5)) * block_size_uv;
        return textureSample(scene_texture, scene_sampler, snapped_uv);
    }

    if (mode == 2.0) {
        // Color grading: brightness (additive), contrast (scaled deviation
        // from mid-gray), saturation (scaled deviation from the sampled
        // pixel's own luminance) — see `PostEffect::ColorGrading`'s doc
        // comment for why these three knobs specifically.
        let sampled = textureSample(scene_texture, scene_sampler, in.uv);
        let brightness = params.threshold_intensity.x;
        let contrast = params.threshold_intensity.y;
        let saturation = params.threshold_intensity.z;

        let luminance = dot(sampled.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        var rgb = mix(vec3<f32>(luminance, luminance, luminance), sampled.rgb, saturation);
        rgb = (rgb - vec3<f32>(0.5, 0.5, 0.5)) * contrast + vec3<f32>(0.5, 0.5, 0.5);
        rgb = rgb + vec3<f32>(brightness, brightness, brightness);
        return vec4<f32>(clamp(rgb, vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(1.0, 1.0, 1.0)), sampled.a);
    }

    if (mode == 3.0) {
        // Posterize: quantize each channel to `levels` evenly-spaced steps
        // (e.g. `levels = 4.0` allows exactly the values 0, 1/3, 2/3, 1).
        // `max(levels - 1.0, 1.0)` avoids a divide-by-zero for a
        // degenerate `levels <= 1.0`.
        let sampled = textureSample(scene_texture, scene_sampler, in.uv);
        let steps = max(params.texel_size_radius.z - 1.0, 1.0);
        let posterized = round(sampled.rgb * steps) / steps;
        return vec4<f32>(posterized, sampled.a);
    }

    if (mode == 7.0) {
        // Vignette (URP-style): fade toward `color` with distance from the
        // frame center. Distance is aspect-corrected (so the clear area is
        // round, not stretched to the frame's shape) and normalized so 1.0
        // is the half-diagonal. `radius` is where the fade *finishes*;
        // `blur` widens the band it fades over. Composited in premultiplied
        // space so the edge color also covers transparent areas (empty
        // background still darkens at the corners, like a lens would).
        let sampled = textureSample(scene_texture, scene_sampler, in.uv);
        let color = params.threshold_intensity.rgb;
        let intensity = params.gradient_map_highlight.x;
        let radius = params.gradient_map_highlight.y;
        let blur = max(params.gradient_map_highlight.z, 0.001);
        let aspect = texel_size.y / texel_size.x;
        var centered = (in.uv - vec2<f32>(0.5, 0.5)) * 2.0;
        centered.x = centered.x * aspect;
        let distance = length(centered) / length(vec2<f32>(aspect, 1.0));
        let edge = smoothstep(radius - blur, radius, distance);
        let k = clamp(edge * intensity, 0.0, 1.0);
        let alpha = sampled.a * (1.0 - k) + k;
        let premultiplied = sampled.rgb * sampled.a * (1.0 - k) + color * k;
        return vec4<f32>(premultiplied / max(alpha, 0.0001), alpha);
    }

    if (mode == 4.0) {
        // Gradient map (duotone): remap luminance to a two-color gradient.
        let sampled = textureSample(scene_texture, scene_sampler, in.uv);
        let luminance = dot(sampled.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
        let mapped = mix(params.threshold_intensity.rgb, params.gradient_map_highlight.rgb, luminance);
        return vec4<f32>(mapped, sampled.a);
    }

    if (mode == 5.0) {
        // Horizontal half of a separable blur. Also where `Glow`'s
        // bright-pass threshold gets applied (per-tap, before blurring) —
        // fusing threshold into this first pass, rather than a separate
        // threshold-only pass, avoids a third full-screen draw for glow.
        let radius = params.texel_size_radius.z;
        let threshold = params.threshold_intensity.x;
        let apply_threshold = params.threshold_intensity.z;

        if (radius <= 0.0) {
            var sampled = textureSample(scene_texture, scene_sampler, in.uv);
            if (apply_threshold > 0.5) {
                let brightness = max(sampled.r, max(sampled.g, sampled.b));
                if (brightness < threshold) {
                    sampled = vec4<f32>(0.0, 0.0, 0.0, 0.0);
                }
            }
            return sampled;
        }

        if (apply_threshold > 0.5) {
            var sum = vec4<f32>(0.0, 0.0, 0.0, 0.0);
            var weight_sum = 0.0;
            let direction = vec2<f32>(texel_size.x, 0.0);
            for (var i: i32 = -BLUR_TAPS; i <= BLUR_TAPS; i = i + 1) {
                let w = gaussian_weight(f32(i));
                var sampled = textureSample(scene_texture, scene_sampler, in.uv + direction * f32(i) * radius);
                let brightness = max(sampled.r, max(sampled.g, sampled.b));
                if (brightness < threshold) {
                    sampled = vec4<f32>(0.0, 0.0, 0.0, 0.0);
                }
                sum = sum + sampled * w;
                weight_sum = weight_sum + w;
            }
            return sum / weight_sum;
        }

        return separable_blur(in.uv, vec2<f32>(texel_size.x, 0.0), radius);
    }

    if (mode == 6.0) {
        // Vertical half of a separable blur — reads the horizontal pass's
        // output (already thresholded, for glow) and finishes the blur.
        // `intensity` scales the result here (not in the horizontal pass)
        // since this is the one value that ends up composited: `Blur`
        // writes this straight to the destination (`intensity` pinned to
        // `1.0` — see `renderer.rs`), `Glow` additively blends it over the
        // plain scene already drawn there.
        let radius = params.texel_size_radius.z;
        let intensity = params.threshold_intensity.y;

        if (radius <= 0.0) {
            let sampled = textureSample(scene_texture, scene_sampler, in.uv);
            return vec4<f32>(sampled.rgb * intensity, sampled.a);
        }

        let blurred = separable_blur(in.uv, vec2<f32>(0.0, texel_size.y), radius);
        return vec4<f32>(blurred.rgb * intensity, blurred.a);
    }

    // mode == 0.0: plain present/copy.
    return textureSample(scene_texture, scene_sampler, in.uv);
}
