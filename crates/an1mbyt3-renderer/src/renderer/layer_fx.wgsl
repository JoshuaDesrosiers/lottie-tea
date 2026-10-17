// **Layer effects** (`renderer/layer_fx.rs`): image effects run on a
// flattened layer — a node drawn offscreen — before it's composited back.
//
// One full-screen pass per call. `tex_a` is the pass's main input, `tex_b` a
// second input (the untouched image for composites), `aux_*` an optional
// image (a depth map). Colours in the layer textures are PREMULTIPLIED; the
// effects work on straight colour (`unpm`) and write premultiplied (`pm`).
//
// Coordinates: `pos.xy` is the target pixel. `u.rect` is the node's box in
// target pixels (x, y, w, h — y down), so "local" uv 0..1 spans the node;
// `u.k.x` is target pixels per node unit (sizes in the effect's settings
// are node units).

struct Fx {
    // x mode, y time (s), z target width, w target height
    a: vec4<f32>,
    // node box in target px: x, y, w, h
    rect: vec4<f32>,
    // x px per node unit, y amount, z variant, w has aux image (0/1)
    k: vec4<f32>,
    p: array<vec4<f32>, 6>,
    c: array<vec4<f32>, 8>,
};

@group(0) @binding(0) var tex_a: texture_2d<f32>;
@group(0) @binding(1) var tex_b: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
@group(0) @binding(3) var<uniform> u: Fx;
@group(1) @binding(0) var aux_tex: texture_2d<f32>;
@group(1) @binding(1) var aux_samp: sampler;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

fn tsize() -> vec2<f32> { return u.a.zw; }
fn sa(px: vec2<f32>) -> vec4<f32> { return textureSampleLevel(tex_a, samp, px / tsize(), 0.0); }
fn sb(px: vec2<f32>) -> vec4<f32> { return textureSampleLevel(tex_b, samp, px / tsize(), 0.0); }
fn unpm(c: vec4<f32>) -> vec4<f32> {
    if (c.a <= 0.00001) { return vec4<f32>(0.0); }
    return vec4<f32>(c.rgb / c.a, c.a);
}
fn pm(c: vec4<f32>) -> vec4<f32> { return vec4<f32>(c.rgb * c.a, c.a); }
fn lum(c: vec3<f32>) -> f32 { return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)); }
fn local_uv(px: vec2<f32>) -> vec2<f32> { return (px - u.rect.xy) / max(u.rect.zw, vec2<f32>(1.0)); }
fn local_px(uv: vec2<f32>) -> vec2<f32> { return u.rect.xy + uv * u.rect.zw; }
fn units(v: f32) -> f32 { return v * u.k.x; }
fn rot(v: vec2<f32>, deg: f32) -> vec2<f32> {
    let a = radians(deg);
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(c * v.x - s * v.y, s * v.x + c * v.y);
}
fn hash12(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.xyx) * 0.1031);
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

/// Depth at local uv (1 = near): the depth image if there is one, else a
/// rough estimate (lower in the frame and brighter = nearer).
fn depth_at(uv: vec2<f32>) -> f32 {
    if (u.k.w > 0.5) {
        return lum(textureSampleLevel(aux_tex, aux_samp, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb);
    }
    let c = unpm(sa(local_px(uv)));
    return clamp(0.15 + uv.y * 0.55 + lum(c.rgb) * 0.2 + c.a * 0.1, 0.0, 1.0);
}

// ---- Blur (separable, per-pixel radius) -----------------------------------
// p0: x radius (px), y mode (0 plain, 1 progressive, 2 depth), z focus, w range
// p1: xy start, zw end (local uv)
// k.z: 0 colour, 1 glow (threshold p2.x on the first pass), 2 shadow alpha
//      (offset p2.yz px, spread p2.w px), 3 already-prepared input
fn blur_factor(px: vec2<f32>) -> f32 {
    let mode = u.p[0].y;
    if (mode < 0.5) { return 1.0; }
    let uv = local_uv(px);
    if (mode < 1.5) {
        let s = u.p[1].xy;
        let e = u.p[1].zw;
        let d = e - s;
        let t = clamp(dot(uv - s, d) / max(dot(d, d), 0.00001), 0.0, 1.0);
        return smoothstep(0.0, 1.0, t);
    }
    let dd = abs(depth_at(uv) - u.p[0].z);
    return smoothstep(u.p[0].w, u.p[0].w + 0.35, dd);
}

fn blur_input(px: vec2<f32>) -> vec4<f32> {
    let variant = u.k.z;
    if (variant > 0.5 && variant < 1.5) {
        // Glow: keep only what's brighter than the threshold.
        let c = unpm(sa(px));
        let l = lum(c.rgb);
        let k = smoothstep(u.p[2].x, u.p[2].x + 0.15, l);
        return pm(vec4<f32>(c.rgb * k, c.a * k));
    }
    if (variant > 1.5 && variant < 2.5) {
        // Shadow: the alpha, offset; spread fattens it.
        let a = sa(px - u.p[2].yz).a;
        let s = clamp(a * (1.0 + u.p[2].w * 0.15), 0.0, 1.0);
        return vec4<f32>(0.0, 0.0, 0.0, s);
    }
    return sa(px);
}

fn blur(px: vec2<f32>, dir: vec2<f32>) -> vec4<f32> {
    let r = u.p[0].x * blur_factor(px);
    if (r < 0.5) { return blur_input(px); }
    // Taps scale with the radius (smooth at any size, cheap when small).
    let taps = i32(clamp(r * 0.75, 8.0, 40.0));
    var acc = vec4<f32>(0.0);
    var wsum = 0.0;
    for (var i = 0; i <= taps; i = i + 1) {
        let t = f32(i) / f32(taps) * 2.0 - 1.0;
        let w = exp(-t * t * 4.5);
        acc = acc + blur_input(px + dir * t * r) * w;
        wsum = wsum + w;
    }
    return acc / wsum;
}

// ---- Stroke -----------------------------------------------------------------
// p0: x width px, y position (0 out, 1 centre, 2 in); c0 colour
fn stroke(px: vec2<f32>) -> vec4<f32> {
    let src = sa(px);
    let w = u.p[0].x;
    let pos = u.p[0].y;
    var grow = 0.0;
    var shrink = 1.0;
    for (var ring = 1; ring <= 3; ring = ring + 1) {
        let r = w * f32(ring) / 3.0;
        for (var i = 0; i < 16; i = i + 1) {
            let a = f32(i) / 16.0 * 6.2831853 + f32(ring) * 0.4;
            let o = vec2<f32>(cos(a), sin(a)) * r;
            let s = sa(px + o).a;
            grow = max(grow, s);
            shrink = min(shrink, s);
        }
    }
    let col = u.c[0];
    if (pos < 0.5) {
        // Outside: the dilated ring under the image.
        let ring_a = clamp(grow - src.a, 0.0, 1.0) * col.a;
        return src + pm(vec4<f32>(col.rgb, ring_a)) * (1.0 - src.a);
    }
    if (pos < 1.5) {
        let outer = clamp(grow - src.a, 0.0, 1.0);
        let inner = clamp(src.a - shrink, 0.0, 1.0);
        let base = src + pm(vec4<f32>(col.rgb, outer * col.a)) * (1.0 - src.a);
        let k = inner * col.a;
        return base * (1.0 - k) + pm(vec4<f32>(col.rgb, 1.0)) * k * src.a;
    }
    let inner = clamp(src.a - shrink, 0.0, 1.0) * col.a;
    return src * (1.0 - inner) + pm(vec4<f32>(col.rgb, 1.0)) * inner * src.a;
}

// ---- Pattern ----------------------------------------------------------------
// p0: x scale px, y radius, z smooth, w angle; p1: x power, y form; c0 colour, c1 bg
fn box_sdf(f: vec2<f32>, half: f32, r: f32) -> f32 {
    let rr = min(r, half);
    let q = abs(f) - vec2<f32>(half - rr);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - rr;
}

fn pattern(px: vec2<f32>) -> vec4<f32> {
    let src = unpm(sa(px));
    let scale = max(u.p[0].x, 1.0);
    let q = rot(px - u.rect.xy, u.p[0].w) / scale;
    let f = fract(q) - 0.5;
    let l = lum(src.rgb);
    let power = u.p[1].x;
    let cov = mix(0.5, l, power);
    let form = u.p[1].y;
    let aa = max(fwidth(q.x) * 1.5, u.p[0].z * 0.5 + 0.0001);
    var d = 0.0;
    if (form < 0.5) {
        // Checker: alternate cells, each a (rounded) square sized by brightness.
        let odd = (i32(floor(q.x)) + i32(floor(q.y))) & 1;
        let half = 0.5 * mix(1.0, cov * 1.6, power);
        let inside = box_sdf(f, half, u.p[0].y * half);
        d = select(-inside, inside, odd == 0);
    } else if (form < 1.5) {
        d = length(f) - sqrt(cov) * 0.62;
    } else if (form < 2.5) {
        d = abs(f.y) - cov * 0.5;
    } else if (form < 3.5) {
        d = min(abs(f.x), abs(f.y)) - (1.0 - cov) * 0.25;
    } else {
        d = abs(f.y + sin(q.x * 6.2831853) * 0.25) - cov * 0.5;
    }
    let inside_k = 1.0 - smoothstep(-aa, aa, d);
    let col = mix(u.c[1], u.c[0], inside_k);
    let out = vec4<f32>(col.rgb, col.a * src.a);
    return pm(mix(src, out, u.k.y));
}

// ---- Halftone ---------------------------------------------------------------
// p0: x size px, y angle, z shape (0 dots, 1 lines, 2 squares), w original colours
fn halftone(px: vec2<f32>) -> vec4<f32> {
    let src = unpm(sa(px));
    let s = max(u.p[0].x, 2.0);
    let q = rot(px - u.rect.xy, u.p[0].y) / s;
    let cell = floor(q) + 0.5;
    let center = u.rect.xy + rot(cell * s, -u.p[0].y);
    let cs = unpm(sa(center));
    let dark = 1.0 - lum(cs.rgb);
    let f = q - cell;
    let aa = fwidth(q.x) * 1.2 + 0.0001;
    var d = 0.0;
    let shape = u.p[0].z;
    if (shape < 0.5) {
        d = length(f) - sqrt(dark) * 0.7;
    } else if (shape < 1.5) {
        d = abs(f.y) - dark * 0.5;
    } else {
        d = max(abs(f.x), abs(f.y)) - sqrt(dark) * 0.5;
    }
    let k = 1.0 - smoothstep(-aa, aa, d);
    var ink = u.c[0];
    if (u.p[0].w > 0.5) { ink = vec4<f32>(cs.rgb * 0.85, 1.0); }
    let paper = u.c[1];
    let col = mix(paper, ink, k);
    let out = vec4<f32>(col.rgb, col.a * max(src.a, cs.a));
    return pm(mix(src, out, u.k.y));
}

// ---- ASCII ------------------------------------------------------------------
// Glyphs are small vector drawings (distance fields: strokes, dots, rings) in
// a cell from -0.5 to 0.5 (y down), so they stay crisp and smooth at any size.
// Indices: 0 space . : - = + * # % @ 0 1, then 12-15 shade blocks.
const SET_STANDARD = array<u32, 10>(0u, 1u, 2u, 3u, 4u, 5u, 6u, 7u, 8u, 9u);
const SET_SIMPLE = array<u32, 5>(0u, 1u, 2u, 6u, 7u);
const SET_BINARY = array<u32, 3>(0u, 10u, 11u);
const SET_BLOCKS = array<u32, 5>(0u, 12u, 13u, 14u, 15u);

fn glyph_index(cset: f32, l: f32) -> u32 {
    if (cset < 0.5) { return SET_STANDARD[u32(clamp(round(l * 9.0), 0.0, 9.0))]; }
    if (cset < 1.5) { return SET_SIMPLE[u32(clamp(round(l * 4.0), 0.0, 4.0))]; }
    if (cset < 2.5) { return SET_BINARY[u32(clamp(round(l * 2.0), 0.0, 2.0))]; }
    return SET_BLOCKS[u32(clamp(round(l * 4.0), 0.0, 4.0))];
}

fn sd_seg(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

/// Distance to glyph `g`'s ink (strokes are ~0.11 wide).
fn glyph_dist(g: u32, p: vec2<f32>) -> f32 {
    let w = 0.055;
    switch g {
        case 1u: { return length(p - vec2<f32>(0.0, 0.28)) - 0.075; }
        case 2u: { return min(length(p - vec2<f32>(0.0, -0.1)), length(p - vec2<f32>(0.0, 0.26))) - 0.075; }
        case 3u: { return sd_seg(p, vec2<f32>(-0.22, 0.06), vec2<f32>(0.22, 0.06)) - w; }
        case 4u: { return min(sd_seg(p, vec2<f32>(-0.22, -0.06), vec2<f32>(0.22, -0.06)), sd_seg(p, vec2<f32>(-0.22, 0.17), vec2<f32>(0.22, 0.17))) - w; }
        case 5u: { return min(sd_seg(p, vec2<f32>(-0.22, 0.06), vec2<f32>(0.22, 0.06)), sd_seg(p, vec2<f32>(0.0, -0.17), vec2<f32>(0.0, 0.29))) - w; }
        case 6u: {
            let c = vec2<f32>(0.0, 0.03);
            let a = sd_seg(p, c + vec2<f32>(0.0, -0.22), c + vec2<f32>(0.0, 0.22));
            let b = sd_seg(p, c + vec2<f32>(-0.19, -0.11), c + vec2<f32>(0.19, 0.11));
            let d = sd_seg(p, c + vec2<f32>(-0.19, 0.11), c + vec2<f32>(0.19, -0.11));
            return min(a, min(b, d)) - w;
        }
        case 7u: {
            let v1 = sd_seg(p, vec2<f32>(-0.06, -0.33), vec2<f32>(-0.13, 0.35));
            let v2 = sd_seg(p, vec2<f32>(0.14, -0.33), vec2<f32>(0.07, 0.35));
            let h1 = sd_seg(p, vec2<f32>(-0.27, -0.08), vec2<f32>(0.27, -0.08));
            let h2 = sd_seg(p, vec2<f32>(-0.29, 0.13), vec2<f32>(0.25, 0.13));
            return min(min(v1, v2), min(h1, h2)) - w * 0.9;
        }
        case 8u: {
            let r1 = abs(length(p - vec2<f32>(-0.15, -0.19)) - 0.08) - w * 0.8;
            let r2 = abs(length(p - vec2<f32>(0.15, 0.23)) - 0.08) - w * 0.8;
            let d = sd_seg(p, vec2<f32>(0.21, -0.32), vec2<f32>(-0.21, 0.36)) - w * 0.8;
            return min(min(r1, r2), d);
        }
        case 9u: {
            let c = p - vec2<f32>(0.0, 0.03);
            let outer = abs(length(c) - 0.29) - w;
            let inner = abs(length(c) - 0.11) - w;
            let tail = sd_seg(p, vec2<f32>(0.11, -0.05), vec2<f32>(0.11, 0.15)) - w;
            // Gap in the outer ring at the lower right (the @'s opening).
            let open = c.x > 0.08 && c.y > 0.05 && c.y < 0.2;
            let ring = select(outer, 1.0, open);
            return min(min(ring, inner), tail);
        }
        case 10u: { return abs(length(vec2<f32>(p.x * 1.45, p.y - 0.02)) - 0.31) - w; }
        case 11u: {
            let s1 = sd_seg(p, vec2<f32>(0.02, -0.32), vec2<f32>(0.02, 0.34));
            let s2 = sd_seg(p, vec2<f32>(0.02, -0.32), vec2<f32>(-0.12, -0.18));
            let s3 = sd_seg(p, vec2<f32>(-0.13, 0.34), vec2<f32>(0.17, 0.34));
            return min(s1, min(s2, s3)) - w;
        }
        default: { return 1.0; }
    }
}

// p0: x size px, y density, z contrast, w charset; p1: x mono; c0 ink, c1 background
fn ascii(px: vec2<f32>) -> vec4<f32> {
    let src = unpm(sa(px));
    let s = max(u.p[0].x, 4.0);
    let q = (px - u.rect.xy) / s;
    let cell = floor(q);
    let cc = u.rect.xy + (cell + 0.5) * s;
    var c = vec4<f32>(0.0);
    for (var i = 0; i < 4; i = i + 1) {
        let o = vec2<f32>(f32(i & 1) - 0.5, f32(i >> 1u) - 0.5) * s * 0.5;
        c = c + unpm(sa(cc + o));
    }
    c = c * 0.25;
    var l = lum(c.rgb) * c.a;
    l = (l - 0.5) * (1.0 + u.p[0].z * 4.0) + 0.5;
    l = clamp(l + (u.p[0].y - 0.25), 0.0, 1.0);
    let g = glyph_index(u.p[0].w, l);
    let f = fract(q) - 0.5;
    var bit = 0.0;
    if (g >= 12u) {
        // Shade blocks: solid at 25 / 50 / 75 / 100 %.
        bit = f32(g - 11u) * 0.25;
    } else if (g > 0u) {
        let aa = 1.0 / s;
        bit = 1.0 - smoothstep(-aa, aa, glyph_dist(g, f));
    }
    var ink = u.c[0];
    if (u.p[1].x < 0.5) { ink = vec4<f32>(c.rgb, 1.0); }
    let col = mix(u.c[1], ink, bit);
    // Only where the image is: the cell's coverage limits both the
    // characters and the background (else the background would paint over
    // the whole screen).
    let cover = clamp(c.a * 1.5, 0.0, 1.0);
    let out = vec4<f32>(col.rgb, col.a * cover);
    return pm(mix(src, out, u.k.y));
}

// ---- Grain ------------------------------------------------------------------
// p0: x size px, y animated, z mono
fn grain(px: vec2<f32>) -> vec4<f32> {
    let src = unpm(sa(px));
    let cell = floor(px / max(u.p[0].x, 1.0));
    let frame = select(0.0, floor(u.a.y * 24.0), u.p[0].y > 0.5);
    let seed = cell + vec2<f32>(frame * 17.13, frame * 3.71);
    var n = vec3<f32>(hash12(seed) - 0.5);
    if (u.p[0].z < 0.5) {
        n = vec3<f32>(hash12(seed + 11.0), hash12(seed + 23.0), hash12(seed + 37.0)) - 0.5;
    }
    let out = vec4<f32>(clamp(src.rgb + n * u.k.y, vec3<f32>(0.0), vec3<f32>(1.0)), src.a);
    return pm(out);
}

// ---- Gradient map -----------------------------------------------------------
// p0: x stop count, y smooth, z offset; p1/p2: 8 positions; c0..c7 colours
fn stop_pos(i: i32) -> f32 {
    let v = u.p[1 + i / 4];
    let j = i % 4;
    if (j == 0) { return v.x; }
    if (j == 1) { return v.y; }
    if (j == 2) { return v.z; }
    return v.w;
}

fn gradient_map(px: vec2<f32>) -> vec4<f32> {
    let src = unpm(sa(px));
    let n = i32(u.p[0].x);
    if (n <= 0) { return sa(px); }
    let l = clamp(lum(src.rgb) + u.p[0].z, 0.0, 1.0);
    var col = u.c[0];
    if (l >= stop_pos(n - 1)) { col = u.c[n - 1]; }
    for (var i = 0; i < n - 1; i = i + 1) {
        let a = stop_pos(i);
        let b = stop_pos(i + 1);
        if (l >= a && l < b) {
            var t = (l - a) / max(b - a, 0.0001);
            let hard = step(0.5, t);
            t = mix(hard, smoothstep(0.0, 1.0, t), clamp(u.p[0].y, 0.0, 1.0));
            col = mix(u.c[i], u.c[i + 1], t);
        }
    }
    let out = vec4<f32>(col.rgb, src.a * col.a);
    return pm(mix(src, out, u.k.y));
}

// ---- Depth map --------------------------------------------------------------
// p0: x contrast, y invert
fn depth_map(px: vec2<f32>) -> vec4<f32> {
    let src = unpm(sa(px));
    var d = depth_at(local_uv(px));
    d = clamp((d - 0.5) * (1.0 + u.p[0].x * 4.0) + 0.5, 0.0, 1.0);
    if (u.p[0].y > 0.5) { d = 1.0 - d; }
    var a = src.a;
    if (u.k.w > 0.5) {
        let uv = local_uv(px);
        if (all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0))) { a = 1.0; }
    }
    let out = vec4<f32>(vec3<f32>(d), a);
    return pm(mix(src, out, u.k.y));
}

// ---- Ribbed glass -----------------------------------------------------------
// p0: x size px, y curve, z echo, w widening; p1: x angle, y dispersion,
// z wave, w wavelength px; p2: x shape (0 circles, 1 lines, 2 squares),
// yz centre (local uv); k.y = distortion
//
// Each rib is a cylindrical lens: `f` is the position across it (-0.5..0.5).
// Curve 0 = a flat, angled facet (a straight ramp); 1 = a round lens whose
// slope steepens towards its edges. Echo repeats the image inside each rib.
fn rib_profile(f: f32) -> f32 {
    let curve = clamp(u.p[0].y, 0.0, 1.0);
    let lens = f / sqrt(max(0.3 - f * f, 0.02)) * 0.55;
    return mix(f * 2.0, lens, curve);
}

struct Rib {
    offset: vec2<f32>,
    f: f32,
};

fn rib_at(px: vec2<f32>) -> Rib {
    let s = max(u.p[0].x, 2.0);
    let center = local_px(u.p[2].yz);
    let rel = px - center;
    let extent = max(max(u.rect.z, u.rect.w), 1.0);
    let widen = 1.0 + u.p[0].w * clamp(length(rel) / extent, 0.0, 1.0);
    let size = s * max(widen, 0.1);
    let shape = u.p[2].x;
    let k = u.k.y * size * 0.45;
    let echo = u.p[0].z * size * 1.5;
    var out: Rib;
    if (shape < 0.5) {
        let r = length(rel);
        let dir = rel / max(r, 0.0001);
        let f = fract(r / size) - 0.5;
        out.offset = dir * (-rib_profile(f) * k - f * echo);
        out.f = f;
    } else {
        let q = rot(rel, -u.p[1].x);
        let wave = sin(q.y / max(u.p[1].w, 1.0) * 6.2831853) * u.p[1].z * size * 0.6;
        let fx = fract((q.x + wave) / size) - 0.5;
        var o = vec2<f32>(-rib_profile(fx) * k - fx * echo, 0.0);
        var fy = 0.0;
        if (shape > 1.5) {
            fy = fract(q.y / size) - 0.5;
            o.y = -rib_profile(fy) * k - fy * echo;
        }
        out.offset = rot(o, u.p[1].x);
        out.f = max(abs(fx), abs(fy));
    }
    return out;
}

fn ribbed(px: vec2<f32>) -> vec4<f32> {
    let disp = u.p[1].y * 0.35;
    var acc = vec4<f32>(0.0);
    var shade = 0.0;
    // 4 samples per pixel: the rib edges are hard steps, this smooths them.
    for (var i = 0; i < 4; i = i + 1) {
        let o = vec2<f32>(f32(i & 1) - 0.5, f32(i >> 1u) - 0.5) * 0.5;
        let p = px + o;
        let rib = rib_at(p);
        let r = sa(p + rib.offset * (1.0 + disp));
        let g = sa(p + rib.offset);
        let b = sa(p + rib.offset * (1.0 - disp));
        acc = acc + vec4<f32>(r.r, g.g, b.b, max(max(r.a, g.a), b.a));
        // Light catches the middle of each rib; the seams darken.
        let e = abs(rib.f) * 2.0;
        shade = shade + (0.06 * (1.0 - e) - 0.22 * smoothstep(0.82, 1.0, e)) * u.k.y;
    }
    let c = acc * 0.25;
    let k = 1.0 + shade * 0.25;
    return vec4<f32>(clamp(c.rgb * k, vec3<f32>(0.0), vec3<f32>(c.a)), c.a);
}

// ---- Scanner drag -----------------------------------------------------------
// p0: x point count, y intensity, z fringe, w timing; p1: x sweep angle;
// c0..c7: xy points (local uv)
fn path_point(t: f32) -> vec2<f32> {
    let n = i32(u.p[0].x);
    if (n <= 1) { return u.c[0].xy; }
    var total = 0.0;
    for (var i = 0; i < n - 1; i = i + 1) { total = total + length(u.c[i + 1].xy - u.c[i].xy); }
    var want = clamp(t, 0.0, 1.0) * total;
    for (var i = 0; i < n - 1; i = i + 1) {
        let a = u.c[i].xy;
        let b = u.c[i + 1].xy;
        let len = length(b - a);
        if (want <= len || i == n - 2) {
            return mix(a, b, clamp(want / max(len, 0.00001), 0.0, 1.0));
        }
        want = want - len;
    }
    return u.c[n - 1].xy;
}

fn scanner(px: vec2<f32>) -> vec4<f32> {
    let uv = local_uv(px);
    let dir = rot(vec2<f32>(0.0, 1.0), u.p[1].x);
    let s = clamp(dot(uv - 0.5, dir) + 0.5, 0.0, 1.0);
    let timing = clamp(u.p[0].w, 0.02, 0.98);
    let t = pow(s, log(timing) / log(0.5));
    let origin = path_point(0.0);
    let k = u.p[0].y;
    let fr = u.p[0].z * 0.08;
    let dg = (path_point(t) - origin) * k;
    let dr = (path_point(t - fr) - origin) * k;
    let db = (path_point(t + fr) - origin) * k;
    let g = sa(local_px(uv - dg));
    let r = sa(local_px(uv - dr));
    let b = sa(local_px(uv - db));
    return vec4<f32>(r.r, g.g, b.b, max(max(r.a, g.a), b.a));
}

// ---- Composites -------------------------------------------------------------
// glow: a = image, b = blurred bright parts; p0: x intensity; c0 tint
fn composite_glow(px: vec2<f32>) -> vec4<f32> {
    let a = sa(px);
    let g = sb(px);
    let add = g.rgb * u.p[0].x * u.c[0].rgb;
    let out_a = clamp(a.a + lum(add) * (1.0 - a.a), 0.0, 1.0);
    return vec4<f32>(min(a.rgb + add, vec3<f32>(out_a)), out_a);
}

// shadow: a = image, b = blurred alpha; c0 colour
fn composite_shadow(px: vec2<f32>) -> vec4<f32> {
    let a = sa(px);
    let sh = sb(px).a * u.c[0].a;
    let shadow = vec4<f32>(u.c[0].rgb * sh, sh);
    return a + shadow * (1.0 - a.a);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let px = pos.xy;
    let mode = i32(u.a.x + 0.5);
    switch mode {
        case 1: { return blur(px, vec2<f32>(1.0, 0.0)); }
        case 2: { return blur(px, vec2<f32>(0.0, 1.0)); }
        case 3: { return composite_glow(px); }
        case 4: { return composite_shadow(px); }
        case 5: { return stroke(px); }
        case 6: { return pattern(px); }
        case 7: { return halftone(px); }
        case 8: { return ascii(px); }
        case 9: { return grain(px); }
        case 10: { return gradient_map(px); }
        case 11: { return depth_map(px); }
        case 12: { return ribbed(px); }
        case 13: { return scanner(px); }
        default: { return sa(px); }
    }
}
