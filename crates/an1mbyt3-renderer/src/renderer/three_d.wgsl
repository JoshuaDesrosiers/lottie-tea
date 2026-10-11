// The 3D pass (`renderer/three_d.rs`): depth-tested, back-face-culled
// triangle meshes lit by a hemisphere ambient, one directional "sun" and up
// to MAX_POINT_LIGHTS point lights, with a Blinn-Phong highlight and
// distance fog. Colours are linear; the target is an sRGB view.

const MAX_POINT_LIGHTS: u32 = 8u;

struct Scene {
    view_proj: mat4x4<f32>,
    // xyz = camera world position.
    camera_pos: vec4<f32>,
    // xyz = direction the sun's light *travels* (normalized), w = unused.
    sun_dir: vec4<f32>,
    // rgb = sun colour * intensity.
    sun_color: vec4<f32>,
    // rgb = sky ambient (from above); the ground ambient is a darker share.
    ambient: vec4<f32>,
    // rgb = fog colour, a = fog density (0 = off).
    fog: vec4<f32>,
    // x = point light count.
    counts: vec4<f32>,
    // xyz = position, w = range.
    point_pos: array<vec4<f32>, 8>,
    // rgb = colour * intensity.
    point_color: array<vec4<f32>, 8>,
    // The camera's world right / up axes (xyz).
    camera_right: vec4<f32>,
    camera_up: vec4<f32>,
};
@group(0) @binding(0)
var<uniform> scene: Scene;

struct Draw {
    model: mat4x4<f32>,
    // Inverse-transpose of `model` (non-uniform scale keeps normals right).
    normal_matrix: mat4x4<f32>,
    color: vec4<f32>,
    // x = shininess (0..1), y = unlit (1 = ignore lights), z = outline width.
    params: vec4<f32>,
    // The material's own parameters (Toon: shade, rim, brush …).
    material: array<vec4<f32>, 6>,
};
// Named `uniforms` (not `draw`) so shader-graph materials — whose generated
// code reads `uniforms.color` for the Tint node — link against this shader
// unchanged (see `MATERIAL_GLUE_3D` in `three_d.rs`).
@group(1) @binding(0)
var<uniform> uniforms: Draw;

// Skinned meshes: this draw's skeleton — each joint's `world × inverse
// bind` matrix in model space (a dynamic-offset slot per skeleton; unused
// by unskinned draws). Uniform, not storage, so WebGL2 runs it too.
const MAX_JOINTS: u32 = 256u;
struct Joints {
    m: array<mat4x4<f32>, 256>,
};
@group(1) @binding(1)
var<uniform> joints: Joints;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    // World-space planar coordinates (meters) picked by the dominant normal
    // axis — what a material's "Local Position" node reads in 3D, so a
    // brick / noise pattern tiles at world scale on walls and floors of any
    // size instead of stretching with each mesh's 0..1 UV.
    @location(3) local_position: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world = uniforms.model * vec4<f32>(in.position, 1.0);
    out.world = world.xyz;
    out.clip = scene.view_proj * world;
    out.normal = (uniforms.normal_matrix * vec4<f32>(in.normal, 0.0)).xyz;
    out.uv = in.uv;
    let n = abs(out.normal);
    if n.y >= n.x && n.y >= n.z {
        out.local_position = world.xz;
    } else if n.x >= n.z {
        out.local_position = world.zy;
    } else {
        out.local_position = world.xy;
    }
    return out;
}

// ---- skinned meshes (characters): skinned on the GPU ----

struct SkinnedInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    // Up to four joints (indices into `joints`) and their weights.
    @location(3) joint_ids: vec4<u32>,
    @location(4) weights: vec4<f32>,
};

fn skin_matrix(j: vec4<u32>, w: vec4<f32>) -> mat4x4<f32> {
    let total = max(w.x + w.y + w.z + w.w, 0.0001);
    let m = joints.m[min(j.x, MAX_JOINTS - 1u)] * w.x
        + joints.m[min(j.y, MAX_JOINTS - 1u)] * w.y
        + joints.m[min(j.z, MAX_JOINTS - 1u)] * w.z
        + joints.m[min(j.w, MAX_JOINTS - 1u)] * w.w;
    return m * (1.0 / total);
}

fn finish_vertex(model_pos: vec4<f32>, model_normal: vec3<f32>, uv: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    let world = uniforms.model * model_pos;
    out.world = world.xyz;
    out.clip = scene.view_proj * world;
    out.normal = (uniforms.normal_matrix * vec4<f32>(model_normal, 0.0)).xyz;
    out.uv = uv;
    let n = abs(out.normal);
    if n.y >= n.x && n.y >= n.z {
        out.local_position = world.xz;
    } else if n.x >= n.z {
        out.local_position = world.zy;
    } else {
        out.local_position = world.xy;
    }
    return out;
}

@vertex
fn vs_skinned(in: SkinnedInput) -> VertexOutput {
    let s = skin_matrix(in.joint_ids, in.weights);
    return finish_vertex(s * vec4<f32>(in.position, 1.0), (s * vec4<f32>(in.normal, 0.0)).xyz, in.uv);
}

// Toon outlines (inverted hull): the skinned mesh pushed out along its
// normals by `uniforms.params.z` (model units); drawn with front faces
// culled, so only a rim round the silhouette shows.
@vertex
fn vs_skinned_outline(in: SkinnedInput) -> VertexOutput {
    let s = skin_matrix(in.joint_ids, in.weights);
    let n = normalize((s * vec4<f32>(in.normal, 0.0)).xyz);
    let p = s * vec4<f32>(in.position, 1.0) + vec4<f32>(n * uniforms.params.z, 0.0);
    return finish_vertex(p, n, in.uv);
}

// Outlines on unskinned meshes: pushed out along the normals.
@vertex
fn vs_outline(in: VertexInput) -> VertexOutput {
    let n = normalize(in.normal);
    return finish_vertex(vec4<f32>(in.position + n * uniforms.params.z, 1.0), n, in.uv);
}

@fragment
fn fs_outline(in: VertexOutput) -> @location(0) vec4<f32> {
    var rgb = uniforms.color.rgb;
    if scene.fog.a > 0.0 {
        let d = length(scene.camera_pos.xyz - in.world) * scene.fog.a;
        rgb = mix(scene.fog.rgb, rgb, clamp(exp(-d * d), 0.0, 1.0));
    }
    return vec4<f32>(rgb, 1.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return shade(in, uniforms.color);
}

/// Lights `base` (the surface colour: the draw's colour, or a material's
/// output) — shared by the plain and the material fragment stages.
fn shade(in: VertexOutput, base: vec4<f32>) -> vec4<f32> {
    if uniforms.params.y > 0.5 {
        return base;
    }
    let n = normalize(in.normal);
    let to_eye = normalize(scene.camera_pos.xyz - in.world);
    // Glossier = tighter, brighter highlight.
    let shininess = clamp(uniforms.params.x, 0.0, 1.0);
    let spec_power = mix(8.0, 160.0, shininess);

    // Hemisphere ambient: sky colour from above, a darker ground bounce below.
    let up = n.y * 0.5 + 0.5;
    var light = scene.ambient.rgb * mix(0.35, 1.0, up);
    var specular = vec3<f32>(0.0);

    // The sun.
    let l = -scene.sun_dir.xyz;
    let ndl = max(dot(n, l), 0.0);
    light += scene.sun_color.rgb * ndl;
    if ndl > 0.0 {
        let h = normalize(l + to_eye);
        specular += scene.sun_color.rgb * pow(max(dot(n, h), 0.0), spec_power) * shininess;
    }

    // Point lights (smooth falloff to zero at `range`).
    let count = u32(scene.counts.x);
    for (var i = 0u; i < MAX_POINT_LIGHTS; i = i + 1u) {
        if i >= count {
            break;
        }
        let offset = scene.point_pos[i].xyz - in.world;
        let dist = length(offset);
        let range = max(scene.point_pos[i].w, 0.001);
        let falloff = pow(clamp(1.0 - dist / range, 0.0, 1.0), 2.0);
        let pl = offset / max(dist, 0.0001);
        let pndl = max(dot(n, pl), 0.0);
        light += scene.point_color[i].rgb * pndl * falloff;
        if pndl > 0.0 {
            let h = normalize(pl + to_eye);
            specular += scene.point_color[i].rgb * pow(max(dot(n, h), 0.0), spec_power) * shininess * falloff;
        }
    }

    var color = base.rgb * light + specular;
    // Exponential-squared fog.
    if scene.fog.a > 0.0 {
        let d = length(scene.camera_pos.xyz - in.world) * scene.fog.a;
        let f = clamp(exp(-d * d), 0.0, 1.0);
        color = mix(scene.fog.rgb, color, f);
    }
    return vec4<f32>(color, base.a);
}

// ---- particles (`ParticleQuad3D`) ----
// Camera-facing quads built on the CPU (world-space corners), coloured with
// premultiplied RGBA so additive (a = 0) and smoky (a > 0) particles share
// one blend state. The fragment draws a soft round glow with a hotter core.

struct ParticleIn {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) hardness: f32,
};

struct ParticleOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) hardness: f32,
};

@vertex
fn vs_particle(in: ParticleIn) -> ParticleOut {
    var out: ParticleOut;
    out.clip = scene.view_proj * vec4<f32>(in.position, 1.0);
    out.color = in.color;
    out.uv = in.uv;
    out.hardness = in.hardness;
    return out;
}

// Fluid density splat: a smooth metaball field, (1 - r²)³ — continuous
// with its first two derivatives, so where neighbours' fields add up the
// liquid's outline bulges and necks smoothly (no creases), and fades to
// exactly zero at the quad's edge. The composite pass draws the surface
// where the summed density crosses the threshold. `color.a` scales how
// much this particle adds.
@fragment
fn fs_density(in: ParticleOut) -> @location(0) vec4<f32> {
    let d = length(in.uv * 2.0 - vec2<f32>(1.0, 1.0));
    let f = clamp(1.0 - d * d, 0.0, 1.0);
    let k = f * f * f * in.color.a;
    return vec4<f32>(k, k, k, k);
}

@fragment
fn fs_particle(in: ParticleOut) -> @location(0) vec4<f32> {
    let d = length(in.uv * 2.0 - vec2<f32>(1.0, 1.0));
    let f = clamp(1.0 - d, 0.0, 1.0);
    // Soft: smooth falloff plus a brighter core — glowing "stuff" that
    // stacks into thick bands when many overlap.
    let glow = f * f * (3.0 - 2.0 * f);
    let soft = glow + f * f * f * 0.8;
    // Hard: a solid shape with an anti-aliased rim (ink, droplets), its
    // edge sharper the harder it is.
    let rim = max(fwidth(d) * 1.5, mix(0.6, 0.02, in.hardness));
    let solid = 1.0 - smoothstep(1.0 - rim, 1.0, d);
    let h = clamp(in.hardness, 0.0, 1.0);
    return in.color * mix(soft, solid, h);
}
