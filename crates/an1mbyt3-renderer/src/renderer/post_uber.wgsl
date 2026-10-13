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
    // (op count, time in seconds, -, -)
    info: vec4<f32>,
    ops: array<vec4<f32>, 24>,
    // The sampling op: (mode, params...), extra params.
    sample_a: vec4<f32>,
    sample_b: vec4<f32>,
    // The pass's Palette colours (linear), op mode 12.
    palette: array<vec4<f32>, 32>,
};

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> u: Uber;
@group(0) @binding(3) var bloom: texture_2d<f32>;
@group(0) @binding(4) var bloom_sampler: sampler;

const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

// A cheap hash: [0, 1) from a 2D point.
fn hash12(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + dot(p3, vec3<f32>(p3.y, p3.z, p3.x) + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// The 4×4 Bayer threshold at pixel `p`, in [0, 1).
fn bayer4(p: vec2<f32>) -> f32 {
    let x = u32(p.x) % 4u;
    let y = u32(p.y) % 4u;
    var m = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    return (m[y * 4u + x] + 0.5) / 16.0;
}

// Linear sRGB to Oklab (perceptual: equal distances look equally different).
fn oklab(c: vec3<f32>) -> vec3<f32> {
    let l = 0.4122214708 * c.r + 0.5363325363 * c.g + 0.0514459929 * c.b;
    let m = 0.2119034982 * c.r + 0.6806995451 * c.g + 0.1073969566 * c.b;
    let s = 0.0883024619 * c.r + 0.2817188376 * c.g + 0.6299787005 * c.b;
    let l_ = pow(max(l, 0.0), 1.0 / 3.0);
    let m_ = pow(max(m, 0.0), 1.0 / 3.0);
    let s_ = pow(max(s, 0.0), 1.0 / 3.0);
    return vec3<f32>(
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    );
}

fn tap(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(source, source_sampler, uv, 0.0);
}

// The source at `uv`, through the pass's sampling op.
fn sample_source(uv_in: vec2<f32>) -> vec4<f32> {
    let a = u.sample_a;
    let mode = a.x;
    let texel = u.texel.xy;
    var uv = uv_in;
    if (mode == 5.0) {
        // Wave: rows slide sideways.
        uv.x = uv.x + sin(uv.y * a.z * 6.2831853 + u.info.y * a.w) * a.y * texel.x;
    } else if (mode == 6.0) {
        // Lens: radial bulge (barrel) or pinch, cropped by zoom.
        let aspect = texel.y / texel.x;
        var p = (uv - vec2<f32>(0.5)) * vec2<f32>(aspect, 1.0);
        let r2 = dot(p, p);
        p = p * (1.0 + a.y * r2) / max(a.z, 0.01);
        uv = p / vec2<f32>(aspect, 1.0) + vec2<f32>(0.5);
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
            return vec4<f32>(0.0);
        }
    }
    let c = tap(uv);
    if (mode == 1.0) {
        // Anaglyph: red from one side, green/blue from the other.
        let off = vec2<f32>(a.y * texel.x, 0.0);
        var l = tap(uv - off);
        var r = tap(uv + off);
        let ml = dot(l.rgb, LUMA);
        let mr = dot(r.rgb, LUMA);
        l = vec4<f32>(mix(l.rgb, vec3<f32>(ml), a.z), l.a);
        r = vec4<f32>(mix(r.rgb, vec3<f32>(mr), a.z), r.a);
        return vec4<f32>(l.r, r.g, r.b, max(l.a, r.a));
    } else if (mode == 2.0) {
        // Chromatic aberration: radial red / blue split.
        let dir = (uv - vec2<f32>(0.5)) * 2.0;
        let off = dir * a.y * texel;
        let r = tap(uv + off);
        let b = tap(uv - off);
        return vec4<f32>(r.r, c.g, b.b, max(c.a, max(r.a, b.a)));
    } else if (mode == 3.0) {
        // Sharpen: unsharp mask against the 4 neighbours.
        let n = tap(uv + vec2<f32>(0.0, texel.y)) + tap(uv - vec2<f32>(0.0, texel.y)) + tap(uv + vec2<f32>(texel.x, 0.0)) + tap(uv - vec2<f32>(texel.x, 0.0));
        let s = c * (1.0 + 4.0 * a.y) - n * a.y;
        return vec4<f32>(clamp(s.rgb, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);
    } else if (mode == 4.0) {
        // Edges: Sobel on brightness × opacity.
        let o = texel * max(a.y, 0.5);
        var g = vec2<f32>(0.0);
        for (var y: i32 = -1; y <= 1; y = y + 1) {
            for (var x: i32 = -1; x <= 1; x = x + 1) {
                let s = tap(uv + vec2<f32>(f32(x), f32(y)) * o);
                let v = dot(s.rgb, LUMA) * s.a + s.a * 0.5;
                let wx = f32(x) * select(1.0, 2.0, y == 0);
                let wy = f32(y) * select(1.0, 2.0, x == 0);
                g = g + vec2<f32>(wx, wy) * v;
            }
        }
        let edge = smoothstep(a.z, a.z + 0.1, length(g));
        let ink = u.sample_b;
        if (a.w > 0.5) {
            return vec4<f32>(ink.rgb, ink.a * edge);
        }
        let k = edge * ink.a;
        let alpha = c.a * (1.0 - k) + k;
        return vec4<f32>((c.rgb * c.a * (1.0 - k) + ink.rgb * k) / max(alpha, 0.0001), alpha);
    }
    return c;
}

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
    } else if (mode == 8.0) {
        // Dither: Bayer thresholds, per channel or two-colour (b.w = mono).
        let px = uv / u.texel.xy / max(a.z, 1.0);
        let t = (bayer4(px) - 0.5) * a.w;
        if (b.w > 0.5) {
            let l = dot(c.rgb, LUMA) + t / max(a.y - 1.0, 1.0);
            let steps = max(a.y - 1.0, 1.0);
            let q = clamp(round(l * steps) / steps, 0.0, 1.0);
            c = vec4<f32>(mix(b.rgb, d.rgb, q), c.a);
        } else {
            let steps = max(a.y - 1.0, 1.0);
            c = vec4<f32>(clamp(round(c.rgb * steps + vec3<f32>(t)) / steps, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);
        }
    } else if (mode == 9.0) {
        // Scanlines + aperture grille.
        let px = uv / u.texel.xy;
        let line = 0.5 + 0.5 * cos(px.y / max(a.z, 1.0) * 6.2831853);
        var rgb = c.rgb * (1.0 - a.y * line);
        let col = u32(px.x) % 3u;
        var m = vec3<f32>(1.0 - a.w);
        if (col == 0u) { m.r = 1.0; } else if (col == 1u) { m.g = 1.0; } else { m.b = 1.0; }
        rgb = rgb * m * (1.0 + a.w * 0.6);
        c = vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);
    } else if (mode == 10.0) {
        // Film grain: noise per grain cell, re-rolled ~24 times a second.
        let px = floor(uv / u.texel.xy / max(a.z, 0.5));
        let n = hash12(px + floor(u.info.y * 24.0) * vec2<f32>(17.0, 59.0)) - 0.5;
        c = vec4<f32>(clamp(c.rgb + vec3<f32>(n * a.y), vec3<f32>(0.0), vec3<f32>(1.0)), c.a);
    } else if (mode == 12.0) {
        // Palette: nearest of a.y colours (Oklab); a.z dither between the
        // two nearest (Bayer, a.w px per cell); b.x mix with the original.
        let n = i32(a.y);
        if (n > 0) {
            let lab = oklab(c.rgb);
            var best = 0;
            var second = 0;
            var d1 = 1e9;
            var d2 = 1e9;
            for (var k: i32 = 0; k < 32; k = k + 1) {
                if (k >= n) {
                    break;
                }
                let diff = oklab(u.palette[k].rgb) - lab;
                let d = dot(diff, diff);
                if (d < d1) {
                    d2 = d1;
                    second = best;
                    d1 = d;
                    best = k;
                } else if (d < d2) {
                    d2 = d;
                    second = k;
                }
            }
            var pick = best;
            if (a.z > 0.0 && n > 1) {
                let r1 = sqrt(d1);
                let r2 = sqrt(d2);
                // 0 on a palette colour, 0.5 halfway to the next nearest.
                let ratio = r1 / max(r1 + r2, 1e-6);
                let px = uv / u.texel.xy / max(a.w, 1.0);
                if (bayer4(px) < ratio * a.z) {
                    pick = second;
                }
            }
            c = vec4<f32>(mix(c.rgb, u.palette[pick].rgb, clamp(b.x, 0.0, 1.0)), c.a);
        }
    } else if (mode == 11.0) {
        // Halftone: dots on a rotated grid, sized by darkness.
        let px = uv / u.texel.xy;
        let ang = radians(a.z);
        let rot = mat2x2<f32>(cos(ang), -sin(ang), sin(ang), cos(ang));
        let q = rot * px / max(a.y, 1.0);
        let cell = fract(q) - vec2<f32>(0.5);
        let darkness = 1.0 - dot(c.rgb, LUMA);
        let radius = sqrt(darkness) * 0.7;
        let dot_ = 1.0 - smoothstep(radius - 0.08, radius + 0.08, length(cell));
        let printed = mix(d.rgb, b.rgb, dot_);
        c = vec4<f32>(mix(printed, c.rgb, a.w), c.a);
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
    var c = sample_source(uv);
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
