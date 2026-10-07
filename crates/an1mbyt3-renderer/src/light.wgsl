// Unity URP-style 2D lighting — every shader stage the lighting pass uses.
// See `renderer/lighting.rs` for the overall frame structure:
//
//   1. the scene is drawn unlit into an offscreen texture (as always);
//   2. each light is drawn into a *light map* (one for Multiply-blend-style
//      lights, one for Additive, one for volumetric glow), shaped by its
//      falloff and darkened by its shadow mask;
//   3. `fs_composite` combines scene x light maps into the lit image.
//
// Shadow masks are drawn per light by `vs_shadow`/`fs_shadow_*`: shadow
// "volumes" (each caster edge extruded away from the light) write 1 into an
// R8 mask, which the light's own fragment shader samples (blurred by the
// light's softness) to cut its contribution.

// ---------------------------------------------------------------------------
// Light quads
// ---------------------------------------------------------------------------

struct LightUniforms {
    // Light-local quad -> clip space (the light node's world transform,
    // composed with the camera — same `build_mvp` every draw uses).
    mvp: mat4x4<f32>,
    // rgb = color * intensity. a = 1.0 for AlphaBlend overlap (output
    // straight color + coverage alpha), 0.0 for Additive (output color
    // pre-scaled by coverage).
    color: vec4<f32>,
    // (kind: 1 spot / 2 freeform / 3 sprite, falloff exponent,
    //  freeform point count, freeform falloff size)
    shape: vec4<f32>,
    // Spot: (inner radius, outer radius, cos(inner half-angle), cos(outer half-angle))
    spot: vec4<f32>,
    // (shadow strength 0..1, shadow blur radius in target pixels,
    //  target width, target height)
    shadow: vec4<f32>,
    // Sprite: (half width, half height, unused, unused)
    sprite: vec4<f32>,
    // (blur 0..1, pixel size in light-local units, falloff steps, unused)
    pixel: vec4<f32>,
    // Freeform polygon, node-local: up to 32 points, two per vec4 (xy, zw).
    points: array<vec4<f32>, 16>,
}

@group(0) @binding(0) var<uniform> light: LightUniforms;
@group(1) @binding(0) var shadow_mask: texture_2d<f32>;
@group(1) @binding(1) var shadow_sampler: sampler;
@group(2) @binding(0) var sprite_texture: texture_2d<f32>;
@group(2) @binding(1) var sprite_sampler: sampler;

struct LightVaryings {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,
}

@vertex
fn vs_light(@location(0) position: vec2<f32>) -> LightVaryings {
    var out: LightVaryings;
    out.clip = light.mvp * vec4<f32>(position, 0.0, 1.0);
    out.local = position;
    return out;
}

fn polygon_point(i: u32) -> vec2<f32> {
    let packed = light.points[i / 2u];
    if (i % 2u == 0u) {
        return packed.xy;
    }
    return packed.zw;
}

// Signed distance to the freeform polygon (negative inside) — the standard
// even-odd winding + nearest-edge formulation (Inigo Quilez's sdPolygon).
fn sd_polygon(p: vec2<f32>, count: u32) -> f32 {
    var d = dot(p - polygon_point(0u), p - polygon_point(0u));
    var s = 1.0;
    var j = count - 1u;
    for (var i = 0u; i < count; i = i + 1u) {
        let vi = polygon_point(i);
        let vj = polygon_point(j);
        let e = vj - vi;
        let w = p - vi;
        let b = w - e * clamp(dot(w, e) / max(dot(e, e), 1e-6), 0.0, 1.0);
        d = min(d, dot(b, b));
        let c1 = p.y >= vi.y;
        let c2 = p.y < vj.y;
        let c3 = e.x * w.y > e.y * w.x;
        if ((c1 && c2 && c3) || (!c1 && !c2 && !c3)) {
            s = -s;
        }
        j = i;
    }
    return s * sqrt(d);
}

// `t` is 1 at full brightness, 0 where the light ends; the exponent comes
// from Unity's Falloff Strength (see `lighting.rs`'s `falloff_exponent`).
fn falloff(t: f32) -> f32 {
    return pow(clamp(t, 0.0, 1.0), light.shape.y);
}

// The falloff, drawn as `steps` crisp concentric rings (brightest at the
// center, faintest — never zero — at the edge), then blurred toward the
// smooth curve by Blur (0 = crisp rings, 1 = smooth gradient). Fewer than 2
// steps = one solid, hard-edged pool.
fn edge(t: f32) -> f32 {
    let gradient = falloff(t);
    let steps = light.pixel.z;
    var stepped = select(0.0, 1.0, t > 0.0);
    if (steps >= 2.0) {
        stepped = select(0.0, ceil(gradient * steps) / steps, t > 0.0);
    }
    return mix(stepped, gradient, light.pixel.x);
}

// 0 = fully lit, 1 = fully shadowed, softened by a small blur kernel.
fn shadow_amount(frag: vec2<f32>) -> f32 {
    let size = light.shadow.zw;
    let uv = frag / size;
    let radius = light.shadow.y;
    if (radius < 0.5) {
        return textureSampleLevel(shadow_mask, shadow_sampler, uv, 0.0).r;
    }
    let step = vec2<f32>(radius, radius) / size;
    var total = 0.0;
    for (var y = -2; y <= 2; y = y + 1) {
        for (var x = -2; x <= 2; x = x + 1) {
            total = total + textureSampleLevel(shadow_mask, shadow_sampler, uv + vec2<f32>(f32(x), f32(y)) * step * 0.5, 0.0).r;
        }
    }
    return total / 25.0;
}

@fragment
fn fs_light(in: LightVaryings) -> @location(0) vec4<f32> {
    let kind = light.shape.x;
    var coverage = 0.0;
    var color = light.color.rgb;
    // Pixel size: evaluate the light at the center of a chunky block of
    // light-local units, so its shape is built from blocks.
    let pixel_size = light.pixel.y;
    var local = in.local;
    if (pixel_size > 0.0) {
        local = (floor(in.local / pixel_size) + vec2<f32>(0.5, 0.5)) * pixel_size;
    }

    if (kind == 1.0) {
        // Spot: radial falloff from inner to outer radius...
        let r = length(local);
        let inner = light.spot.x;
        let outer = max(light.spot.y, inner + 0.001);
        var radial = 1.0;
        if (r > inner) {
            radial = 1.0 - (r - inner) / (outer - inner);
        }
        coverage = edge(radial);
        // ...times the cone, aimed along local +Y. cos(outer half) <= -1
        // means a full 360 degrees (a point light): no cone at all.
        let cos_inner = light.spot.z;
        let cos_outer = light.spot.w;
        if (cos_outer > -0.9999 && r > 0.0001) {
            let c = local.y / r;
            var cone = 0.0;
            if (cos_inner - cos_outer > 0.0001) {
                cone = smoothstep(cos_outer, cos_inner, c);
            } else {
                cone = select(0.0, 1.0, c >= cos_outer);
            }
            coverage = coverage * cone;
        }
    } else if (kind == 2.0) {
        // Freeform: full inside the polygon, fading out over falloff size.
        let count = u32(light.shape.z);
        if (count >= 3u) {
            let d = sd_polygon(local, count);
            if (d <= 0.0) {
                coverage = 1.0;
            } else {
                coverage = edge(1.0 - d / max(light.shape.w, 0.001));
            }
        }
    } else if (kind == 3.0) {
        // Sprite: the texture's own pixels are the light (its alpha shapes
        // it, its color tints it). Local +Y is up, texture V is down.
        let half = light.sprite.xy;
        let uv = vec2<f32>(local.x / (2.0 * half.x) + 0.5, 0.5 - local.y / (2.0 * half.y));
        let texel = textureSample(sprite_texture, sprite_sampler, uv);
        color = color * texel.rgb;
        coverage = texel.a;
    }

    coverage = coverage * (1.0 - light.shadow.x * shadow_amount(in.clip.xy));

    if (light.color.a > 0.5) {
        // AlphaBlend overlap: straight color, coverage as alpha.
        return vec4<f32>(color, coverage);
    }
    return vec4<f32>(color * coverage, coverage);
}

// ---------------------------------------------------------------------------
// Shadow masks
// ---------------------------------------------------------------------------

struct ShadowUniforms {
    mvp: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> shadow_uniforms: ShadowUniforms;

@vertex
fn vs_shadow(@location(0) position: vec2<f32>) -> @builtin(position) vec4<f32> {
    return shadow_uniforms.mvp * vec4<f32>(position, 0.0, 1.0);
}

@fragment
fn fs_shadow_on() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.0, 0.0, 1.0);
}

@fragment
fn fs_shadow_off() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, 1.0);
}

// ---------------------------------------------------------------------------
// Composite: scene x light maps -> lit image
// ---------------------------------------------------------------------------

@group(0) @binding(0) var scene_tex: texture_2d<f32>;
@group(0) @binding(1) var multiply_map: texture_2d<f32>;
@group(0) @binding(2) var additive_map: texture_2d<f32>;
@group(0) @binding(3) var volumetric_map: texture_2d<f32>;
@group(0) @binding(4) var composite_sampler: sampler;

struct FullscreenVaryings {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// The usual single oversized triangle covering the whole target.
@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> FullscreenVaryings {
    var out: FullscreenVaryings;
    let x = f32((index << 1u) & 2u);
    let y = f32(index & 2u);
    out.clip = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

@fragment
fn fs_composite(in: FullscreenVaryings) -> @location(0) vec4<f32> {
    let scene = textureSample(scene_tex, composite_sampler, in.uv);
    let multiply = textureSample(multiply_map, composite_sampler, in.uv).rgb;
    let additive = textureSample(additive_map, composite_sampler, in.uv).rgb;
    let volumetric = textureSample(volumetric_map, composite_sampler, in.uv).rgb;

    // Lit surface color: Multiply-style light scales it, Additive-style
    // light adds on top (only where there's a surface to light).
    // Capped at full brightness: a lit sprite never goes brighter than its
    // own art, however many lights pile onto it (no blown-out hotspots).
    let surface = min(scene.rgb * min(multiply, vec3<f32>(1.0)) + additive * scene.a, vec3<f32>(1.0));
    // Volumetric glow is "in the air": it shows over empty areas too, so it
    // is composited in premultiplied space and contributes its own alpha.
    let glow_alpha = clamp(max(volumetric.r, max(volumetric.g, volumetric.b)), 0.0, 1.0);
    let alpha = clamp(scene.a + glow_alpha * (1.0 - scene.a), 0.0, 1.0);
    let premultiplied = surface * scene.a + volumetric;
    return vec4<f32>(premultiplied / max(alpha, 0.0001), alpha);
}
