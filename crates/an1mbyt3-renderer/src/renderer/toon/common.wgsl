// Shared by the Toon material shaders (`toon_mtoon.wgsl`,
// `toon_arcane.wgsl`): parameter unpacking, lighting terms and noise.
//
// TOON_PARAMS — `uniforms.material[i]`, filled from
// `an1mbyt3_core::ToonMaterial::gpu_params`:
//   [0] = shade.rgb, shift        [1] = rim.rgb, toony
//   [2] = emission.rgb, rim_power [3] = highlight.rgb, rim_lift
//   [4] = brush_scale, brush_strength, hatch, grain
//   [5] = alpha_cutoff, point_lights, bands, light_min

// MToon's light term: N·L (+ shift) mapped through a window that narrows
// to a hard edge as `toony` goes to 1.
fn tn_term(ndl: f32, shift: f32, toony: f32) -> f32 {
    let t = clamp(toony, 0.0, 0.999);
    let lo = -1.0 + t;
    let hi = 1.0 - t;
    return clamp((ndl + shift - lo) / max(hi - lo, 0.0001), 0.0, 1.0);
}

fn tn_hash(p: vec3<f32>) -> f32 {
    let q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    let r = q + dot(q, q.yxz + 33.33);
    return fract((r.x + r.y) * r.z);
}

// Smooth 3D value noise, 0..1.
fn tn_noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(mix(tn_hash(i), tn_hash(i + vec3<f32>(1.0, 0.0, 0.0)), u.x), mix(tn_hash(i + vec3<f32>(0.0, 1.0, 0.0)), tn_hash(i + vec3<f32>(1.0, 1.0, 0.0)), u.x), u.y);
    let b = mix(mix(tn_hash(i + vec3<f32>(0.0, 0.0, 1.0)), tn_hash(i + vec3<f32>(1.0, 0.0, 1.0)), u.x), mix(tn_hash(i + vec3<f32>(0.0, 1.0, 1.0)), tn_hash(i + vec3<f32>(1.0, 1.0, 1.0)), u.x), u.y);
    return mix(a, b, u.z);
}

// The surface's base colour (texture × tint), alpha-cut, and its normal
// turned toward the eye (back faces of double-sided cloth / hair).
fn tn_base(in: VertexOutput) -> vec4<f32> {
    let base = textureSample(image_texture, image_sampler, in.uv) * uniforms.color;
    let cutoff = uniforms.material[5].x;
    if cutoff > 0.0 && base.a < cutoff {
        discard;
    }
    return base;
}

fn tn_normal(in: VertexOutput) -> vec3<f32> {
    var n = normalize(in.normal);
    if dot(n, scene.camera_pos.xyz - in.world) < 0.0 {
        n = -n;
    }
    return n;
}

// Point light `i` at this surface: (direction to it, colour × falloff).
fn tn_point(i: u32, world: vec3<f32>) -> array<vec4<f32>, 2> {
    let offset = scene.point_pos[i].xyz - world;
    let dist = length(offset);
    let range = max(scene.point_pos[i].w, 0.001);
    let falloff = pow(clamp(1.0 - dist / range, 0.0, 1.0), 2.0);
    return array<vec4<f32>, 2>(vec4<f32>(offset / max(dist, 0.0001), 0.0), vec4<f32>(scene.point_color[i].rgb * falloff, 0.0));
}

fn tn_fog(rgb: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    if scene.fog.a > 0.0 {
        let d = length(scene.camera_pos.xyz - world) * scene.fog.a;
        return mix(scene.fog.rgb, rgb, clamp(exp(-d * d), 0.0, 1.0));
    }
    return rgb;
}
