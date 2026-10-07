// Flat-fill-or-gradient shader (AN1MBYT3_BUILD_PLAN.md §10/§16 "gradient
// fills"). One uniform block per draw call: an already-composed
// model-view-projection matrix, a flat fill color, and an optional gradient
// (kind/start/end/up to MAX_GRADIENT_STOPS stops) — `stop_count == 0` means
// "no gradient," and the fragment shader falls straight back to `color`
// with zero extra math, so every pre-gradient draw call (`stop_count` left
// at its zero-initialized default) renders exactly as it always did.
//
// Stops are packed as `array<vec4<f32>, MAX_GRADIENT_STOPS / 4>` (4 offsets
// per vec4) rather than `array<f32, MAX_GRADIENT_STOPS>` — WGSL's uniform
// address space requires array elements to be 16-byte aligned, so a plain
// `array<f32, N>` would pad every single scalar out to 16 bytes; packing 4
// unrelated floats into one vec4 avoids that entirely (same trick
// `post_process.wgsl`'s `PostUniforms` already uses for its own packed
// fields). Colors are already vec4-sized, so `array<vec4<f32>, MAX_GRADIENT_STOPS>`
// needs no such packing.

const MAX_GRADIENT_STOPS: u32 = 8u;
const GRADIENT_KIND_LINEAR: i32 = 1;
const GRADIENT_KIND_RADIAL: i32 = 2;
const GRADIENT_KIND_CONIC: i32 = 3;
const GRADIENT_KIND_BILINEAR: i32 = 4;
const TAU: f32 = 6.283185307179586;

struct Uniforms {
    mvp: mat4x4<f32>,
    color: vec4<f32>,
    // (start.x, start.y, end.x, end.y) — local (mesh/path) space; see
    // `an1mbyt3_core::GradientDef`'s own doc comment for how `start`/`end`
    // are interpreted per `gradient_kind`.
    gradient_start_end: vec4<f32>,
    // (kind as f32, stop_count as f32, unused, unused).
    gradient_meta: vec4<f32>,
    stop_offsets: array<vec4<f32>, 2>,
    stop_colors: array<vec4<f32>, 8>,
    // Affinity's draggable per-segment "midpoint" — see
    // `an1mbyt3_core::GradientStop::midpoint`'s own doc comment. Packed
    // the same 4-per-vec4 way as `stop_offsets`; the last stop's own slot
    // is present but unused (no segment follows it).
    stop_midpoints: array<vec4<f32>, 2>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec2<f32>,
    // Per-vertex UV — every mesh's vertex buffer at binding slot 1 carries
    // this now (`an1mbyt3_renderer::GpuMesh::uv_buffer`'s own doc comment),
    // zero-filled unless a mesh actually authored real per-vertex UVs (a
    // `Tilemap`'s many-sub-rects-of-one-tileset mesh). Only read when
    // `uniforms.gradient_meta.w` says to — see `compute_textured_color`.
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    // The raw, pre-MVP mesh position — `input.position` already *is* this
    // shape's local/path-space coordinate (the mesh vertex buffer holds
    // untransformed tessellated path points; the vertex shader below is the
    // only thing that ever composes them with `mvp`), so no separate
    // "inverse-transform the fragment back to local space" step is needed —
    // just forward it through as an interpolated varying.
    @location(0) local_position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = uniforms.mvp * vec4<f32>(input.position, 0.0, 1.0);
    out.local_position = input.position;
    out.uv = input.uv;
    return out;
}

fn stop_offset_at(i: u32) -> f32 {
    return uniforms.stop_offsets[i / 4u][i % 4u];
}

fn stop_color_at(i: u32) -> vec4<f32> {
    return uniforms.stop_colors[i];
}

fn stop_midpoint_at(i: u32) -> f32 {
    return uniforms.stop_midpoints[i / 4u][i % 4u];
}

/// Remaps a segment-local `0..1` position so the 50%-blend point lands at
/// `midpoint` instead of always at `0.5` — Affinity's draggable per-segment
/// "midpoint" diamond (see `an1mbyt3_core::GradientStop::midpoint`'s own
/// doc comment). Piecewise-linear: everything before the midpoint is
/// rescaled into the first half of the output range, everything after into
/// the second half, so `local_t == midpoint` always maps to exactly `0.5`
/// regardless of how far off-center `midpoint` is.
fn bias_segment(local_t: f32, midpoint: f32) -> f32 {
    let m = clamp(midpoint, 0.001, 0.999);
    if (local_t < m) {
        return 0.5 * local_t / m;
    }
    return 0.5 + 0.5 * (local_t - m) / (1.0 - m);
}

/// Walks the (ascending-offset) stop list to find `t`'s bracketing pair,
/// biases the segment-local position by the leading stop's own `midpoint`,
/// and linearly interpolates between them — clamps to the first/last
/// stop's color outside their range, rather than extrapolating or wrapping.
fn sample_gradient(t: f32, stop_count: u32) -> vec4<f32> {
    let clamped = clamp(t, 0.0, 1.0);
    if (clamped <= stop_offset_at(0u)) {
        return stop_color_at(0u);
    }
    for (var i: u32 = 0u; i < stop_count - 1u; i = i + 1u) {
        let o0 = stop_offset_at(i);
        let o1 = stop_offset_at(i + 1u);
        if (clamped >= o0 && clamped <= o1) {
            let span = max(o1 - o0, 0.00001);
            let local_t = bias_segment((clamped - o0) / span, stop_midpoint_at(i));
            return mix(stop_color_at(i), stop_color_at(i + 1u), local_t);
        }
    }
    return stop_color_at(stop_count - 1u);
}

/// `Multiply`/`Screen`/`Add`'s fixed-function GPU blend factors
/// (`Renderer::wgpu_blend_state`/`BlendState::ADDITIVE`) have no alpha
/// weighting at all — see `Renderer::blend_alpha_correction`'s own doc
/// comment for the full reasoning. This is the shader-side half:
/// `uniforms.gradient_meta.z` (`1.0` multiply, `2.0` screen *or* add, `0.0`
/// neither) tells `fs_main` to pre-blend its own output color toward that
/// mode's identity color (white for multiply, black for screen/add —
/// blending with the identity is a no-op) by `(1 - alpha)`, so the
/// *shader's* output already carries the alpha-weighted fade the
/// fixed-function blend state can't express on its own, and sets alpha to
/// `1.0` so the (alpha-blind) blend state doesn't also apply its own
/// un-weighted alpha math on top.
fn apply_blend_correction(color: vec4<f32>) -> vec4<f32> {
    let correction = uniforms.gradient_meta.z;
    if (correction == 1.0) {
        return vec4<f32>(mix(vec3<f32>(1.0), color.rgb, color.a), 1.0);
    } else if (correction == 2.0) {
        return vec4<f32>(mix(vec3<f32>(0.0), color.rgb, color.a), 1.0);
    }
    return color;
}

/// The flat-fill-or-gradient color this draw would show — factored out of
/// `fs_main` so `fs_mask_write` (this shader's *other* consumer, the mask
/// pass — see that function's own doc comment) can compute the exact same
/// color and alpha-discard on it, instead of a mask always clipping to its
/// raw tessellated geometry regardless of what it would actually *look*
/// like drawn (a gradient fading to transparent, or a shape with partial
/// fill alpha) — see `an1mbyt3_core::GroupNode`'s own doc comment and
/// `Renderer::mask_write_pipeline`'s for the fuller picture of why per-pixel
/// alpha matters here specifically, not just geometric coverage.
fn compute_flat_or_gradient_color(in: VertexOutput) -> vec4<f32> {
    let stop_count = u32(uniforms.gradient_meta.y);
    if (stop_count == 0u) {
        return apply_blend_correction(uniforms.color);
    }

    let start = uniforms.gradient_start_end.xy;
    let end = uniforms.gradient_start_end.zw;
    let kind = i32(uniforms.gradient_meta.x);
    let to_frag = in.local_position - start;

    var t: f32;
    if (kind == GRADIENT_KIND_RADIAL) {
        let radius = length(end - start);
        t = select(length(to_frag) / radius, 0.0, radius < 0.00001);
    } else if (kind == GRADIENT_KIND_CONIC) {
        let reference = end - start;
        let delta_angle = atan2(to_frag.y, to_frag.x) - atan2(reference.y, reference.x);
        t = (delta_angle - TAU * floor(delta_angle / TAU)) / TAU;
    } else if (kind == GRADIENT_KIND_BILINEAR) {
        // Chebyshev (max-norm) distance from `start`, along the
        // `start -> end` axis and its perpendicular — square, corner-
        // anchored isocontours instead of radial's circular ones (see
        // `an1mbyt3_core::GradientKind::Bilinear`'s own doc comment for
        // why that reads as "L-shaped" bands).
        let radius = length(end - start);
        if (radius < 0.00001) {
            t = 0.0;
        } else {
            let axis_x = (end - start) / radius;
            let axis_y = vec2<f32>(-axis_x.y, axis_x.x);
            let proj_x = abs(dot(to_frag, axis_x));
            let proj_y = abs(dot(to_frag, axis_y));
            t = max(proj_x, proj_y) / radius;
        }
    } else {
        // Linear (the default/fallback for any unrecognized kind value).
        let axis = end - start;
        let len_sq = dot(axis, axis);
        t = select(dot(to_frag, axis) / len_sq, 0.0, len_sq < 0.00001);
    }

    return apply_blend_correction(sample_gradient(t, stop_count));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return compute_flat_or_gradient_color(in);
}

/// `Renderer::mask_write_pipeline`'s own fragment entry — same flat/gradient
/// color `fs_main` computes, but `discard`s a fragment whose alpha is
/// (near-)zero instead of writing it. A discarded fragment skips its
/// stencil write entirely (the whole point) — without this, a mask made
/// from a shape with transparent regions (a gradient fading out, a partly-
/// transparent fill) still clipped to its full tessellated *geometry*, i.e.
/// its bounding silhouette, ignoring which parts of it were ever meant to
/// be see-through. `0.001`, not exactly `0.0`, so a numerically-near-zero
/// alpha from gradient interpolation still discards rather than surviving
/// on a rounding technicality.
@fragment
fn fs_mask_write(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = compute_flat_or_gradient_color(in);
    if (color.a <= 0.001) {
        discard;
    }
    return color;
}

// Textured-quad path (raster `Image` nodes, and any `Shape` with its own
// `image` set — see `an1mbyt3_core::ShapeNode::image`'s doc comment) —
// shares `vs_main`/`Uniforms` above. `uniforms.gradient_start_end` is
// repurposed here as this draw's own mesh's local bounds (`min.xy, max.xy`
// — `Renderer::DrawUniforms::from_draw` packs it that way whenever
// `draw.texture` is set, never a real gradient axis at the same time),
// so `local_position` maps to `0..1` UV space by that mesh's own extent
// rather than a hardcoded `+0.5` — correct for an `Image` node's fixed
// ±0.5 unit quad *and* for an arbitrary tessellated `Shape` path, with no
// separate vertex attribute needed either way. Bound as a second bind group
// (group 1) alongside the existing per-draw uniform (group 0) rather than
// folded into `fs_main`, since every non-textured draw has no texture to
// bind at all — keeping this a distinct pipeline/entry point means
// `fs_main`'s pipelines never need a dummy bound texture.
@group(1) @binding(0)
var image_texture: texture_2d<f32>;
@group(1) @binding(1)
var image_sampler: sampler;

fn compute_textured_color(in: VertexOutput) -> vec4<f32> {
    // `gradient_meta.w` — `1.0` for a mesh with real per-vertex UVs (a
    // `Tilemap`'s many-sub-rects-of-one-tileset mesh, `DrawUniforms::from_draw`'s
    // own doc comment), `0.0` (every other textured draw: an `Image` node's
    // `unit_quad_mesh`, a `Shape` node's image fill) for the original
    // position-derived path below. A vertex-authored UV is expected to
    // already be in WebGPU's top-down `v` convention (the mesh builder's own
    // job — `an1mbyt3-wasm`'s tilemap mesh builder bakes the flip in once,
    // at authoring time, rather than redoing it per-fragment here), so it's
    // used as-is with no further flip.
    if uniforms.gradient_meta.w > 0.5 {
        return textureSample(image_texture, image_sampler, in.uv) * uniforms.color;
    }
    let bounds_min = uniforms.gradient_start_end.xy;
    let bounds_max = uniforms.gradient_start_end.zw;
    let size = max(bounds_max - bounds_min, vec2<f32>(0.0001, 0.0001));
    let normalized = (in.local_position - bounds_min) / size;
    // This engine's local/world space is Y-*up* (every other coordinate
    // convention in this codebase agrees: +y is up), but WebGPU's texture
    // sampling treats `v = 0` as the texture's *top* row (`v` increases
    // downward) — the opposite of OpenGL's bottom-up `v`. Mapping local Y
    // straight onto `v` (`v = normalized.y`) put the source image's top row
    // at the shape's local-space *bottom* and vice versa: a real, always-
    // upside-down image, not something dependent on any particular
    // shape/mesh. Flipping `v` here is the one place that needs to know
    // about this mismatch — `bounds_min`/`bounds_max` themselves stay in
    // plain Y-up local space, matching every other local-position use in
    // this shader (the gradient math above, `fs_mask_write`'s discard).
    let uv = vec2<f32>(normalized.x, 1.0 - normalized.y);
    return textureSample(image_texture, image_sampler, uv) * uniforms.color;
}

@fragment
fn fs_textured(in: VertexOutput) -> @location(0) vec4<f32> {
    return compute_textured_color(in);
}

/// `fs_mask_write`'s textured counterpart — an `Image` node, or a `Shape`
/// with its own `image` set, used as a mask (or as content clipped by one)
/// needs its actual sampled-texture alpha to matter, not just its
/// tessellated quad's full rectangular coverage. Discards the same way
/// `fs_mask_write` does, so a transparent pixel in the source image doesn't
/// contribute to the stencil at all.
@fragment
fn fs_textured_mask_write(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = compute_textured_color(in);
    if (color.a <= 0.001) {
        discard;
    }
    return color;
}
