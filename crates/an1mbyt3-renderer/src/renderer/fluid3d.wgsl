// Screen-space fluid composite (`renderer/three_d.rs`, `FluidLayer3D`).
//
// The density pass has splatted every fluid particle of one layer into
// `density` (red channel, additive). Here, per screen pixel:
//
// - coverage: density thresholded with a soft edge — touching particles
//   merge into one smooth blob, like metaballs / liquid;
// - a surface normal from the density's slope (central differences): steep
//   at the blob's edges, flat in its thick middle;
// - shading: the body colour, a glossy highlight from that normal (wet
//   ink), and the edge colour where the surface turns away (a rim).
//
// Output is premultiplied alpha.

struct Fluid {
    body: vec4<f32>,
    edge: vec4<f32>,
    // x = threshold, y = edge softness, z = gloss, w = texel size x
    params: vec4<f32>,
    // x = texel size y
    params2: vec4<f32>,
};

@group(0) @binding(0) var density: texture_2d<f32>;
@group(0) @binding(1) var density_sampler: sampler;
@group(0) @binding(2) var<uniform> fluid: Fluid;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) i: u32) -> Out {
    // One triangle that covers the screen.
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: Out;
    out.clip = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(p.x, 1.0 - p.y);
    return out;
}

fn d(uv: vec2<f32>) -> f32 {
    return textureSampleLevel(density, density_sampler, uv, 0.0).r;
}

@fragment
fn fs_composite(in: Out) -> @location(0) vec4<f32> {
    let threshold = fluid.params.x;
    let texel = vec2<f32>(fluid.params.w, fluid.params2.x) * 2.0;
    let c = d(in.uv);
    // A clean, pixel-wide anti-aliased edge (screen-space derivative), not
    // a blurry fade — the liquid's outline stays crisp at any zoom.
    let aa = max(fwidth(c), 0.0005);
    let coverage = smoothstep(threshold - aa, threshold + aa, c);
    if coverage <= 0.001 {
        discard;
    }
    // Surface normal from the density slope (screen space).
    let gx = d(in.uv + vec2<f32>(texel.x, 0.0)) - d(in.uv - vec2<f32>(texel.x, 0.0));
    let gy = d(in.uv + vec2<f32>(0.0, texel.y)) - d(in.uv - vec2<f32>(0.0, texel.y));
    // (The z term sets how "thick" the liquid looks: larger = flatter,
    // so only the very edge catches the rim colour.)
    let n = normalize(vec3<f32>(-gx, gy, 1.1 * threshold));
    // Light from the upper left, towards the viewer.
    let l = normalize(vec3<f32>(-0.5, 0.6, 0.65));
    let h = normalize(l + vec3<f32>(0.0, 0.0, 1.0));
    let spec = pow(max(dot(n, h), 0.0), 48.0) * fluid.params.z;
    let diffuse = 0.75 + 0.25 * max(dot(n, l), 0.0);
    // Rim: where the surface tilts away (thin edges of the ink).
    let rim = pow(1.0 - clamp(n.z, 0.0, 1.0), 1.5);
    var color = fluid.body.rgb * diffuse;
    color = mix(color, fluid.edge.rgb, clamp(rim * fluid.edge.a, 0.0, 1.0));
    color += vec3<f32>(spec);
    let a = coverage * fluid.body.a;
    return vec4<f32>(color * a, a);
}
