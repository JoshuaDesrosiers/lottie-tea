//! **GPU-rendered liquids** (Goo 2D groups and Particle Fluid 2D): the
//! liquid's field (see `metaball2d`) is uploaded as a small texture and a
//! generated shader draws it per pixel — the technique Valve published for
//! crisp text from low-resolution distance fields ("Improved Alpha-Tested
//! Magnification", SIGGRAPH 2007), which is also why vector tools like
//! Rive keep soft/merged edges analytic on the GPU instead of tessellating.
//!
//! Compared with tracing outlines on the CPU (marching squares → smoothing
//! → tessellation → a fresh mesh every frame), this:
//! - costs the CPU only the field itself (and that's cached for still
//!   members), plus one tiny texture write — no outlines, no triangles;
//! - draws the edge at *pixel* precision with a one-pixel anti-aliased
//!   ramp (`fwidth`), so curves are perfectly smooth at any zoom — no grid
//!   facets, no smoothing pass;
//! - samples the field with a cubic B-spline (four bilinear taps), so even
//!   a coarse grid reads as one continuous surface;
//! - keeps the same look: the same field, the same rim / body / highlight
//!   levels and highlight offset as the outline renderer, with fill and rim
//!   gradients evaluated per pixel in the group owner's space.
//!
//! **Packing.** Each field value is stored as 16 bits split across two
//! 8-bit channels (high byte, low byte). Bilinear filtering is linear, and
//! `256·hi + lo` is linear in both, so filtering the two channels and then
//! recombining them is exactly filtering the 16-bit value. R,G hold the
//! main field; B,A an optional second field on the same grid (the particle
//! fluid's highlight drops). A border of empty texels keeps every filter
//! tap inside written data.

use glam::{Affine2, Vec2};

use crate::metaball2d::Field;

/// Largest field value the texture keeps (overlapping members add up).
pub const RANGE: f32 = 4.0;
/// Empty texels round the field (≥ the bicubic filter's reach + offsets).
pub const BORDER: usize = 3;

/// A field (and optional second field on its grid) as RGBA8 texels.
/// Returns `(width, height, bytes)`.
pub fn pack(field: &Field, second: Option<&[f32]>) -> (u32, u32, Vec<u8>) {
    let (w, h, _, _) = field.layout();
    let (tw, th) = (w + 2 * BORDER, h + 2 * BORDER);
    let mut bytes = vec![0u8; tw * th * 4];
    let enc = |v: f32| -> (u8, u8) {
        let q = ((v / RANGE).clamp(0.0, 1.0) * 65535.0).round() as u32;
        ((q >> 8) as u8, (q & 0xff) as u8)
    };
    let main = field.samples();
    for j in 0..h {
        for i in 0..w {
            let at = ((j + BORDER) * tw + (i + BORDER)) * 4;
            let (hi, lo) = enc(main[j * w + i]);
            bytes[at] = hi;
            bytes[at + 1] = lo;
            if let Some(s) = second {
                let (hi, lo) = enc(s[j * w + i]);
                bytes[at + 2] = hi;
                bytes[at + 3] = lo;
            }
        }
    }
    (tw as u32, th as u32, bytes)
}

/// The quad covering the field, in the draw's space (`to_draw` maps world
/// → that space), with UVs onto a texture of `capacity` texels.
pub fn quad(field: &Field, capacity: (u32, u32), to_draw: Affine2) -> an1mbyt3_geometry::Mesh {
    let (w, h, origin, cell) = field.layout();
    let (cw, ch) = (capacity.0 as f32, capacity.1 as f32);
    // Sample (i, j) sits at texel (i + BORDER, j + BORDER); its centre is
    // (+0.5) / capacity in UV. The quad spans the samples' full extent.
    let corner = |i: f32, j: f32| {
        let world = origin + Vec2::new(i, j) * cell;
        let p = to_draw.transform_point2(world);
        ([p.x, p.y], [(i + BORDER as f32 + 0.5) / cw, (j + BORDER as f32 + 0.5) / ch])
    };
    let (x1, y1) = ((w.max(1) - 1) as f32, (h.max(1) - 1) as f32);
    let c = [corner(0.0, 0.0), corner(x1, 0.0), corner(x1, y1), corner(0.0, y1)];
    an1mbyt3_geometry::Mesh { positions: c.iter().map(|v| v.0).collect(), indices: vec![0, 1, 2, 0, 2, 3], uvs: c.iter().map(|v| v.1).collect() }
}

/// A colour: flat, or a gradient over the draw's local space.
#[derive(Debug, Clone)]
pub enum Paint {
    Solid([f32; 4]),
    Gradient(an1mbyt3_core::GradientDef),
}

/// What one liquid draws, back to front. Levels are field values.
#[derive(Debug, Clone)]
pub struct Look {
    /// The whole outline in this paint (the rim band shows round the body).
    pub rim: Option<(f32, Paint)>,
    pub body: (f32, Paint),
    /// The highlight: its level, colour, offset in *texels*, and whether it
    /// reads the second field (particle fluids) or the main one (goo).
    pub highlight: Option<Highlight>,
    /// `None`: hard edges (in or out per pixel — crisp at any resolution,
    /// pixel-art friendly, and the cheapest). `Some(px)`: an anti-aliased
    /// ramp `px` screen pixels wide.
    pub antialias: Option<f32>,
    /// A user material (shader-graph WGSL defining `material_fragment`)
    /// whose colour multiplies the body paint.
    pub material: Option<String>,
    /// A halo round the liquid: colour, strength, and the outermost level
    /// it reaches (a field value below the rim / body level).
    pub glow: Option<Glow>,
}

#[derive(Debug, Clone)]
pub struct Glow {
    pub color: [f32; 4],
    pub strength: f32,
    /// Field level where the glow has faded out, and the level of the
    /// liquid's outer edge (where it's strongest).
    pub outer: f32,
    pub edge: f32,
}

#[derive(Debug, Clone)]
pub struct Highlight {
    pub level: f32,
    pub color: [f32; 4],
    pub offset_texels: Vec2,
    pub second_field: bool,
}

fn f(v: f32) -> String {
    let s = format!("{v:?}");
    if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") { s } else { format!("{s}.0") }
}

/// A document colour (authored in sRGB, like every fill) as the linear
/// WGSL constant the renderer shades with — the same conversion normal
/// fills and gradient stops get (`an1mbyt3_renderer::linear_rgba`).
fn vec4(c: [f32; 4]) -> String {
    let c = an1mbyt3_renderer::linear_rgba(c);
    format!("vec4<f32>({}, {}, {}, {})", f(c[0]), f(c[1]), f(c[2]), f(c[3]))
}

/// WGSL for a paint, as an expression of `p` (local position).
fn paint_expr(name: &str, paint: &Paint, out: &mut String) {
    use an1mbyt3_core::GradientKind as K;
    match paint {
        Paint::Solid(c) => out.push_str(&format!("fn {name}(p: vec2<f32>) -> vec4<f32> {{ return {}; }}\n", vec4(*c))),
        Paint::Gradient(g) => {
            let (s, e) = (g.start, g.end);
            let t = match g.kind {
                K::Radial => format!("length(p - vec2<f32>({}, {})) / max(length(vec2<f32>({}, {})), 0.0001)", f(s.x), f(s.y), f(e.x - s.x), f(e.y - s.y)),
                K::Conic => format!(
                    "fract((atan2(p.y - {sy}, p.x - {sx}) - atan2({dy}, {dx})) / 6.2831853 + 1.0)",
                    sx = f(s.x),
                    sy = f(s.y),
                    dx = f(e.x - s.x),
                    dy = f(e.y - s.y)
                ),
                _ => format!("dot(p - vec2<f32>({}, {}), vec2<f32>({}, {})) / max({}, 0.0001)", f(s.x), f(s.y), f(e.x - s.x), f(e.y - s.y), f((e - s).length_squared())),
            };
            let mut stops = g.stops.clone();
            stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
            out.push_str(&format!("fn {name}(p: vec2<f32>) -> vec4<f32> {{\n    let t = clamp({t}, 0.0, 1.0);\n"));
            match stops.as_slice() {
                [] => out.push_str("    return vec4<f32>(1.0, 1.0, 1.0, 1.0);\n"),
                [only] => out.push_str(&format!("    return {};\n", vec4(only.color))),
                _ => {
                    out.push_str(&format!("    var c = {};\n", vec4(stops[0].color)));
                    for pair in stops.windows(2) {
                        let (a, b) = (&pair[0], &pair[1]);
                        out.push_str(&format!(
                            "    c = mix(c, {cb}, clamp((t - {ta}) / max({tb} - {ta}, 0.0001), 0.0, 1.0));\n",
                            cb = vec4(b.color),
                            ta = f(a.offset),
                            tb = f(b.offset)
                        ));
                    }
                    out.push_str("    return c;\n");
                }
            }
            out.push_str("}\n");
        }
    }
}

/// The material fragment (`fn material_fragment`) drawing `look` from a
/// packed field texture. Pure function of `look`: equal looks share one
/// compiled pipeline.
pub fn shader(look: &Look) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        r#"
{fields}

{cover}

// Straight-alpha "over".
fn goo_over(dst: vec4<f32>, src: vec4<f32>) -> vec4<f32> {{
    let a = src.a + dst.a * (1.0 - src.a);
    if a <= 0.00001 {{
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }}
    return vec4<f32>((src.rgb * src.a + dst.rgb * dst.a * (1.0 - src.a)) / a, a);
}}
"#,
        fields = field_sampling(),
        cover = match look.antialias {
            // Hard edge: a pixel is liquid or it isn't.
            None => "fn goo_cover(v: f32, level: f32) -> f32 {
    return select(0.0, 1.0, v >= level);
}".to_string(),
            Some(px) => format!(
                "// Coverage of `v >= level` with a {px}-pixel anti-aliased ramp.
fn goo_cover(v: f32, level: f32) -> f32 {{
    let aa = max(fwidth(v) * {w}, 0.00001);
    return clamp((v - level) / aa + 0.5, 0.0, 1.0);
}}",
                px = f(px),
                w = f(px.max(0.05))
            ),
        }
    ));
    if let Some((_, paint)) = &look.rim {
        paint_expr("goo_rim_paint", paint, &mut s);
    }
    paint_expr("goo_body_paint", &look.body.1, &mut s);
    if let Some(user) = &look.material {
        // The user's graph, its entry point renamed.
        s.push('\n');
        s.push_str(&user.replace("fn material_fragment(", "fn goo_user_material("));
        s.push('\n');
    }
    s.push_str("\nfn material_fragment(in: VertexOutput) -> vec4<f32> {\n    let fields = goo_fields(in.uv);\n    let v = fields.x;\n    var c = vec4<f32>(0.0, 0.0, 0.0, 0.0);\n");
    if let Some((level, _)) = &look.rim {
        s.push_str(&format!("    let rim = goo_rim_paint(in.local_position);\n    c = goo_over(c, vec4<f32>(rim.rgb, rim.a * goo_cover(v, {})));\n", f(*level)));
    }
    let body = if look.material.is_some() { "goo_body_paint(in.local_position) * goo_user_material(in)" } else { "goo_body_paint(in.local_position)" };
    s.push_str(&format!("    let body_cover = goo_cover(v, {});\n    let body = {body};\n    c = goo_over(c, vec4<f32>(body.rgb, body.a * body_cover));\n", f(look.body.0)));
    if let Some(hl) = &look.highlight {
        let sample = if hl.second_field {
            "fields.y".to_string()
        } else {
            format!("goo_fields(in.uv + vec2<f32>({}, {}) / vec2<f32>(textureDimensions(image_texture))).x", f(hl.offset_texels.x), f(hl.offset_texels.y))
        };
        s.push_str(&format!(
            "    let shine = {color};\n    c = goo_over(c, vec4<f32>(shine.rgb, shine.a * goo_cover({sample}, {level}) * body_cover));\n",
            color = vec4(hl.color),
            level = f(hl.level)
        ));
    }
    // The draw's own opacity (fades, clip fade) scales the whole liquid.
    s.push_str("    return vec4<f32>(c.rgb, c.a * uniforms.color.a);\n}\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metaball2d::Blob;

    #[test]
    fn packing_keeps_sixteen_bits_and_a_clear_border() {
        let field = Field::from_blobs(&[Blob { center: Vec2::ZERO, radius: 10.0, weight: 1.0 }], 1.0);
        let (tw, th, bytes) = pack(&field, None);
        let (w, h, _, _) = field.layout();
        assert_eq!((tw as usize, th as usize), (w + 2 * BORDER, h + 2 * BORDER));
        // The border is empty.
        assert!(bytes[..tw as usize * 4 * BORDER].iter().all(|b| *b == 0));
        // A sample decodes back within one 16-bit step.
        let (i, j) = (w / 2, h / 2);
        let at = ((j + BORDER) * tw as usize + i + BORDER) * 4;
        let back = (bytes[at] as f32 * 256.0 + bytes[at + 1] as f32) / 65535.0 * RANGE;
        assert!((back - field.samples()[j * w + i]).abs() < RANGE / 65535.0 + 1e-6);
    }
}

/// The field sampling every liquid shader shares (`goo_fields`).
fn field_sampling() -> String {
    format!(r#"const GOO_RANGE: f32 = {range};

fn goo_tap(uv: vec2<f32>) -> vec4<f32> {{
    return textureSampleLevel(image_texture, image_sampler, uv, 0.0);
}}

// Both packed fields at `uv`, cubic B-spline filtered (four bilinear taps).
fn goo_fields(uv: vec2<f32>) -> vec2<f32> {{
    let dims = vec2<f32>(textureDimensions(image_texture));
    let st = uv * dims - vec2<f32>(0.5, 0.5);
    let i = floor(st);
    let t = st - i;
    let t2 = t * t;
    let t3 = t2 * t;
    let w0 = (1.0 - 3.0 * t + 3.0 * t2 - t3) / 6.0;
    let w1 = (4.0 - 6.0 * t2 + 3.0 * t3) / 6.0;
    let w2 = (1.0 + 3.0 * t + 3.0 * t2 - 3.0 * t3) / 6.0;
    let w3 = t3 / 6.0;
    let g0 = w0 + w1;
    let g1 = w2 + w3;
    let h0 = (i - vec2<f32>(1.0, 1.0) + w1 / g0 + vec2<f32>(0.5, 0.5)) / dims;
    let h1 = (i + vec2<f32>(1.0, 1.0) + w3 / g1 + vec2<f32>(0.5, 0.5)) / dims;
    let s = g0.y * (g0.x * goo_tap(vec2<f32>(h0.x, h0.y)) + g1.x * goo_tap(vec2<f32>(h1.x, h0.y)))
          + g1.y * (g0.x * goo_tap(vec2<f32>(h0.x, h1.y)) + g1.x * goo_tap(vec2<f32>(h1.x, h1.y)));
    let k = 255.0 * GOO_RANGE / 65535.0;
    return vec2<f32>((s.r * 256.0 + s.g) * k, (s.b * 256.0 + s.a) * k);
}}
"#, range = f(RANGE))
}

/// The liquid's **glow**, as its own shader — drawn *after* the liquid
/// with additive blending, so it lights up the liquid's edge and whatever
/// is around and beneath it (rather than sitting under the liquid as a
/// tint). Brightest at the outline, fading out to `glow.outer` outside it
/// and easing off toward the inside, so the body keeps its colour with a
/// luminous rim. `None` when the look has no glow.
pub fn glow_shader(look: &Look) -> Option<String> {
    let glow = look.glow.as_ref()?;
    let c = an1mbyt3_renderer::linear_rgba(glow.color);
    Some(format!(
        r#"
{fields}

fn material_fragment(in: VertexOutput) -> vec4<f32> {{
    let v = goo_fields(in.uv).x;
    let outside = clamp((v - {outer}) / max({edge} - {outer}, 0.00001), 0.0, 1.0);
    let inside = clamp((v - {edge}) / max({edge} * 0.8, 0.00001), 0.0, 1.0);
    let a = outside * outside * mix(1.0, 0.3, inside);
    return vec4<f32>(vec3<f32>({r}, {g}, {b}) * {strength}, {alpha} * a * uniforms.color.a);
}}
"#,
        fields = field_sampling(),
        outer = f(glow.outer),
        edge = f(glow.edge),
        r = f(c[0]),
        g = f(c[1]),
        b = f(c[2]),
        alpha = f(c[3].clamp(0.0, 1.0)),
        strength = f(glow.strength.max(0.0) * 1.6),
    ))
}
