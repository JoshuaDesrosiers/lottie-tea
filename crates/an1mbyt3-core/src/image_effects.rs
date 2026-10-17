//! **Image effects** — a per-node stack of pixel effects, like an image
//! editor's layer effects: the node (and everything under it) is drawn
//! into an offscreen image, the effects run on that image in order, and
//! the result is composited back where the node was.
//!
//! Unlike the scene-wide post stack ([`crate::PostEffect`]), these belong to
//! one node, move and scale with it (positions and sizes are in the node's
//! own units / its bounding box), show in Design as well as at runtime, and
//! every value is keyframeable through field tracks
//! (`behavior/ImageEffects/effects/<i>/effect/<Kind>/<field>`), so effects
//! animate like any other property.
//!
//! The effects:
//!
//! | Effect | What it does |
//! |---|---|
//! | [`ImageEffect::DropShadow`] | soft shadow from the image's alpha |
//! | [`ImageEffect::Stroke`] | outline around the alpha shape |
//! | [`ImageEffect::Pattern`] | two-colour pattern (checker, dots, lines, grid, waves) shaded by brightness |
//! | [`ImageEffect::Halftone`] | print dots / lines / squares by brightness |
//! | [`ImageEffect::Ascii`] | the image as text characters |
//! | [`ImageEffect::Grain`] | film grain (animated) |
//! | [`ImageEffect::Glow`] | bloom from the bright parts |
//! | [`ImageEffect::GradientMap`] | brightness → colour ramp |
//! | [`ImageEffect::Blur`] | plain, progressive (along a line) or depth (focus) blur |
//! | [`ImageEffect::DepthMap`] | shows the depth image (or an estimate) as grey |
//! | [`ImageEffect::RibbedGlass`] | refraction through ribbed / fluted glass |
//! | [`ImageEffect::ScannerDrag`] | the image dragged along a path while being scanned |
//! | [`ImageEffect::Code`] | your own shader code, `@param` comments become sliders |
//!
//! Serde: externally tagged enums and positional structs (postcard), new
//! effects and fields are only ever appended.

use serde::{Deserialize, Serialize};

use crate::ImageId;

/// The **Image Effects** component: the ordered effect stack, plus a
/// background behind the element and clipping to the element's shape.
///
/// - **Background**: a colour and / or image filling the element's box
///   (rounded by `background_radius`, grown by `background_padding`)
///   behind its content, before the effects run — so glass, blur, ASCII …
///   have something to work on in the element's empty areas.
/// - **Clip to shape**: the final image is masked by the element's own
///   silhouette (its drawn alpha), so effects never spill outside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImageEffects {
    pub effects: Vec<ImageEffectLayer>,
    /// Background colour (alpha 0 = none).
    pub background: [f32; 4],
    /// Background image, covering the box (drawn over the colour).
    pub background_image: Option<ImageId>,
    /// 0..1 opacity of the background image.
    pub background_image_opacity: f32,
    /// Corner radius of the background box (node units).
    pub background_radius: f32,
    /// Grows the background box on every side (node units).
    pub background_padding: f32,
    /// Mask the result to the element's own shape.
    pub clip_to_shape: bool,
}

impl Default for ImageEffects {
    fn default() -> Self {
        Self { effects: Vec::new(), background: [0.0; 4], background_image: None, background_image_opacity: 1.0, background_radius: 0.0, background_padding: 0.0, clip_to_shape: false }
    }
}

impl ImageEffects {
    /// Whether anything would actually render differently.
    pub fn is_active(&self) -> bool {
        self.effects.iter().any(|l| l.enabled) || self.has_background() || self.clip_to_shape
    }

    /// Whether a background is drawn.
    pub fn has_background(&self) -> bool {
        self.background[3] > 0.0 || (self.background_image.is_some() && self.background_image_opacity > 0.0)
    }
}

/// One effect in the stack, with its on / off switch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImageEffectLayer {
    pub effect: ImageEffect,
    pub enabled: bool,
}

impl Default for ImageEffectLayer {
    fn default() -> Self {
        Self { effect: ImageEffect::Grain(Grain::default()), enabled: true }
    }
}

/// One image effect. Append-only (postcard encodes the variant index).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ImageEffect {
    DropShadow(DropShadow),
    Stroke(Stroke),
    Pattern(Pattern),
    Halftone(Halftone),
    Ascii(Ascii),
    Grain(Grain),
    Glow(Glow),
    GradientMap(GradientMap),
    Blur(Blur),
    DepthMap(DepthMap),
    RibbedGlass(RibbedGlass),
    ScannerDrag(ScannerDrag),
    Code(CodeEffect),
}

impl ImageEffect {
    /// The effect's display name (also what the editor's Add menu shows).
    pub fn label(&self) -> &'static str {
        match self {
            ImageEffect::DropShadow(_) => "Drop Shadow",
            ImageEffect::Stroke(_) => "Stroke",
            ImageEffect::Pattern(_) => "Pattern",
            ImageEffect::Halftone(_) => "Halftone",
            ImageEffect::Ascii(_) => "ASCII",
            ImageEffect::Grain(_) => "Grain",
            ImageEffect::Glow(_) => "Glow",
            ImageEffect::GradientMap(_) => "Gradient Map",
            ImageEffect::Blur(_) => "Blur",
            ImageEffect::DepthMap(_) => "Depth Map",
            ImageEffect::RibbedGlass(_) => "Ribbed Glass",
            ImageEffect::ScannerDrag(_) => "Slit Scan",
            ImageEffect::Code(_) => "Code",
        }
    }

    /// The depth image this effect reads, if any.
    pub fn depth_image(&self) -> Option<ImageId> {
        match self {
            ImageEffect::Blur(b) => b.depth_image,
            ImageEffect::DepthMap(d) => d.depth_image,
            _ => None,
        }
    }
}

/// **Drop Shadow**: the image's alpha, blurred, tinted and offset, behind it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DropShadow {
    pub color: [f32; 4],
    /// Offset in the node's units.
    pub offset: [f32; 2],
    /// Blur radius in the node's units.
    pub blur: f32,
    /// Grows the shadow before blurring (node units).
    pub spread: f32,
}

impl Default for DropShadow {
    fn default() -> Self {
        Self { color: [0.0, 0.0, 0.0, 0.55], offset: [6.0, -8.0], blur: 12.0, spread: 0.0 }
    }
}

/// Where a stroke sits relative to the shape's edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StrokePosition {
    #[default]
    Outside,
    Center,
    Inside,
}

/// **Stroke**: an outline around the image's alpha shape.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Stroke {
    pub color: [f32; 4],
    /// Width in the node's units.
    pub width: f32,
    pub position: StrokePosition,
}

impl Default for Stroke {
    fn default() -> Self {
        Self { color: [1.0, 1.0, 1.0, 1.0], width: 4.0, position: StrokePosition::Outside }
    }
}

/// A [`Pattern`]'s shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PatternForm {
    #[default]
    Checker,
    Dots,
    Lines,
    Grid,
    Waves,
}

/// **Pattern**: the image recoloured with two colours, the split between
/// them drawn as a pattern whose coverage follows brightness.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Pattern {
    /// 0 = original, 1 = fully patterned.
    pub amount: f32,
    pub form: PatternForm,
    /// Cell size in the node's units.
    pub scale: f32,
    /// Corner rounding of cells, 0..1.
    pub radius: f32,
    /// Edge softness, 0..1.
    pub smooth: f32,
    /// Rotation in degrees.
    pub angle: f32,
    /// How strongly brightness grows / shrinks the pattern, 0..1.
    pub power: f32,
    pub color: [f32; 4],
    pub background: [f32; 4],
}

impl Default for Pattern {
    fn default() -> Self {
        Self {
            amount: 1.0,
            form: PatternForm::Checker,
            scale: 24.0,
            radius: 0.0,
            smooth: 0.0,
            angle: 0.0,
            power: 0.6,
            color: [1.0, 0.45, 0.38, 1.0],
            background: [0.04, 0.22, 0.27, 1.0],
        }
    }
}

/// A [`Halftone`]'s mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HalftoneShape {
    #[default]
    Dots,
    Lines,
    Squares,
}

/// **Halftone**: printed-dot look; mark size follows darkness.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Halftone {
    pub amount: f32,
    pub shape: HalftoneShape,
    /// Cell size in the node's units.
    pub size: f32,
    pub angle: f32,
    /// Keep the image's colours (else `ink` on `paper`).
    pub original_colors: bool,
    pub ink: [f32; 4],
    pub paper: [f32; 4],
}

impl Default for Halftone {
    fn default() -> Self {
        Self { amount: 1.0, shape: HalftoneShape::Dots, size: 10.0, angle: 45.0, original_colors: true, ink: [0.05, 0.05, 0.07, 1.0], paper: [0.97, 0.95, 0.9, 1.0] }
    }
}

/// The characters an [`Ascii`] effect draws with, light to dark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AsciiCharset {
    /// ` .:-=+*#%@`
    #[default]
    Standard,
    /// ` .:*#`
    Simple,
    /// ` 01`
    Binary,
    /// ` ░▒▓█` (shaded blocks)
    Blocks,
}

/// **ASCII**: the image redrawn as characters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Ascii {
    pub amount: f32,
    /// Character cell size in the node's units.
    pub size: f32,
    /// Shifts which characters get used: higher = denser, darker text.
    pub density: f32,
    /// Brightness contrast before picking characters, 0..1.
    pub contrast: f32,
    pub charset: AsciiCharset,
    /// `Mono`: `ink` characters on `background`; else the image's own colours.
    pub mono: bool,
    pub ink: [f32; 4],
    pub background: [f32; 4],
}

impl Default for Ascii {
    fn default() -> Self {
        Self { amount: 1.0, size: 14.0, density: 0.25, contrast: 0.2, charset: AsciiCharset::Standard, mono: true, ink: [1.0; 4], background: [0.0, 0.0, 0.0, 1.0] }
    }
}

/// **Grain**: film grain noise.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Grain {
    pub amount: f32,
    /// Grain size in the node's units.
    pub size: f32,
    /// Re-rolls every frame (else frozen).
    pub animated: bool,
    /// Grey grain (else coloured).
    pub mono: bool,
}

impl Default for Grain {
    fn default() -> Self {
        Self { amount: 0.25, size: 1.5, animated: true, mono: true }
    }
}

/// **Glow**: bloom from the bright parts.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Glow {
    /// Brightness of the glow.
    pub intensity: f32,
    /// Spread in the node's units.
    pub radius: f32,
    /// Only brighter than this glows, 0..1.
    pub threshold: f32,
    /// Multiplies the glow's colour.
    pub tint: [f32; 4],
}

impl Default for Glow {
    fn default() -> Self {
        Self { intensity: 1.2, radius: 18.0, threshold: 0.55, tint: [1.0; 4] }
    }
}

/// One colour on a [`GradientMap`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GradientMapStop {
    pub position: f32,
    pub color: [f32; 4],
}

impl Default for GradientMapStop {
    fn default() -> Self {
        Self { position: 0.0, color: [0.0, 0.0, 0.0, 1.0] }
    }
}

/// **Gradient Map**: brightness picks a colour from the ramp (≤ 8 stops).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GradientMap {
    pub amount: f32,
    /// 0 = hard bands between stops, 1 = smooth blends.
    pub smooth: f32,
    /// Shifts brightness before mapping, -1..1.
    pub offset: f32,
    pub stops: Vec<GradientMapStop>,
}

impl Default for GradientMap {
    fn default() -> Self {
        Self {
            amount: 1.0,
            smooth: 0.5,
            offset: 0.0,
            stops: vec![
                GradientMapStop { position: 0.0, color: [0.05, 0.07, 0.16, 1.0] },
                GradientMapStop { position: 0.5, color: [0.1, 0.62, 0.62, 1.0] },
                GradientMapStop { position: 1.0, color: [0.98, 0.94, 0.82, 1.0] },
            ],
        }
    }
}

/// How a [`Blur`] varies across the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BlurMode {
    /// The same everywhere.
    #[default]
    Plain,
    /// Sharp at `start`, full blur at `end` (a line across the node).
    Progressive,
    /// Sharp at the `focus` depth, blurrier further from it.
    Depth,
}

/// **Blur**.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Blur {
    pub mode: BlurMode,
    /// Full blur radius in the node's units.
    pub radius: f32,
    /// Progressive: where it's sharp / fully blurred, in the node's box
    /// (0..1, y down).
    pub start: [f32; 2],
    pub end: [f32; 2],
    /// Depth: the in-focus depth (0 = far, 1 = near) …
    pub focus: f32,
    /// … and how much depth around it stays sharp.
    pub focus_range: f32,
    /// Depth: the depth image (white = near). None = estimated.
    pub depth_image: Option<ImageId>,
}

impl Default for Blur {
    fn default() -> Self {
        Self { mode: BlurMode::Plain, radius: 12.0, start: [0.5, 0.35], end: [0.5, 1.0], focus: 0.8, focus_range: 0.1, depth_image: None }
    }
}

/// **Depth Map**: shows depth as grey (white = near).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DepthMap {
    pub amount: f32,
    pub contrast: f32,
    pub invert: bool,
    /// The depth image; None = an estimate from the image itself.
    pub depth_image: Option<ImageId>,
}

impl Default for DepthMap {
    fn default() -> Self {
        Self { amount: 1.0, contrast: 0.0, invert: false, depth_image: None }
    }
}

/// The ribs of [`RibbedGlass`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RibShape {
    /// Rings around `center`.
    Circles,
    /// Parallel flutes at `angle`.
    #[default]
    Lines,
    /// A grid of square lenses.
    Squares,
}

/// **Ribbed Glass**: the image seen through fluted glass.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RibbedGlass {
    pub shape: RibShape,
    /// Refraction strength, 0..1.
    pub distortion: f32,
    /// Rib width in the node's units.
    pub size: f32,
    /// Rib profile: 0 = flat bevel, 1 = round.
    pub curve: f32,
    /// Repeats the refracted image inside each rib, 0..1.
    pub echo: f32,
    /// Ribs widen (+) or narrow (-) away from the centre, -1..1.
    pub widening: f32,
    pub angle: f32,
    /// Colour split through the glass, 0..1.
    pub dispersion: f32,
    /// Ribs wave sideways, 0..1 …
    pub wave: f32,
    /// … with this wavelength (node units).
    pub wavelength: f32,
    /// Centre of rings / widening, in the node's box (0..1, y down).
    pub center: [f32; 2],
}

impl Default for RibbedGlass {
    fn default() -> Self {
        Self { shape: RibShape::Lines, distortion: 0.5, size: 40.0, curve: 0.8, echo: 0.1, widening: 0.0, angle: 20.0, dispersion: 0.15, wave: 0.25, wavelength: 320.0, center: [0.5, 0.5] }
    }
}

/// **Slit Scan** (`ScannerDrag`): as if the picture were moved along `points` while a
/// scanner swept down it — each row is captured at a different point of
/// the drag, smearing / bending the image along the path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScannerDrag {
    /// How far the drag moves the image, 0..1 (of the node's size).
    pub intensity: f32,
    /// Colour fringing on the moving parts, 0..1.
    pub fringe: f32,
    /// How the sweep spends its time along the path: 0.5 = even, lower =
    /// early in the path, higher = late. Animate it to replay the drag.
    pub timing: f32,
    /// The drag path in the node's box (0..1, y down), ≤ 8 points.
    pub points: Vec<[f32; 2]>,
    /// Sweep direction in degrees (0 = top to bottom).
    pub angle: f32,
}

impl Default for ScannerDrag {
    fn default() -> Self {
        Self { intensity: 0.4, fringe: 0.2, timing: 0.5, points: vec![[0.3, 0.8], [0.5, 0.6], [0.45, 0.35], [0.65, 0.2]], angle: 0.0 }
    }
}

/// A `@param` slider of a [`CodeEffect`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CodeParam {
    pub name: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
}

impl Default for CodeParam {
    fn default() -> Self {
        Self { name: String::new(), value: 0.0, min: 0.0, max: 1.0 }
    }
}

/// A `@color` swatch of a [`CodeEffect`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CodeColor {
    pub name: String,
    pub value: [f32; 4],
}

impl Default for CodeColor {
    fn default() -> Self {
        Self { name: String::new(), value: [1.0; 4] }
    }
}

/// **Code**: a custom effect written as a shader function
/// `effect(uv) -> color` (WGSL, or the HLSL-ish `float4 effect(float2 uv)`
/// style — see the renderer's `layer_fx` for the dialect). `// @param name
/// min max default` / `// @color name #hex` lines become `params` /
/// `colors` (≤ 8 of each), shown as sliders and keyframeable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CodeEffect {
    pub amount: f32,
    pub code: String,
    pub params: Vec<CodeParam>,
    pub colors: Vec<CodeColor>,
}

/// The starter code a new Code effect gets.
pub const CODE_EFFECT_TEMPLATE: &str = "// @param ripple 0 1 0.5
// @param speed 0 4 1
// @param rings 2 30 10
// @param tint_mix 0 1 0.35
// @color tint #FFB37A

// Rings ripple out from the middle; the colour warms where they crest.
float4 effect(float2 uv) {
    float2 d = uv - float2(0.5, 0.5);
    float r = length(d);
    float wave = sin(r * rings * 6.2832 - time * speed * 3.0);
    float2 push = d / max(r, 0.001) * wave * ripple * 0.015;
    float4 c = sample(uv + push);
    float crest = smoothstep(0.6, 1.0, wave) * tint_mix;
    c.rgb = mix(c.rgb, c.rgb * tint.rgb * 1.3, crest);
    return c;
}
";

impl Default for CodeEffect {
    fn default() -> Self {
        let (params, colors) = parse_code_params(CODE_EFFECT_TEMPLATE);
        Self { amount: 1.0, code: CODE_EFFECT_TEMPLATE.into(), params, colors }
    }
}

/// Reads `// @param name min max default` and `// @color name #rrggbb`
/// lines (≤ 8 each) — the editor calls this on Apply and keeps the current
/// value of any param whose name didn't change.
pub fn parse_code_params(code: &str) -> (Vec<CodeParam>, Vec<CodeColor>) {
    let mut params = Vec::new();
    let mut colors = Vec::new();
    for line in code.lines() {
        let Some(rest) = line.trim().strip_prefix("//") else { continue };
        let words: Vec<&str> = rest.split_whitespace().collect();
        match words.as_slice() {
            ["@param", name, rest @ ..] if params.len() < 8 && is_ident(name) => {
                let n: Vec<f32> = rest.iter().filter_map(|w| w.parse().ok()).collect();
                let (min, max) = (n.first().copied().unwrap_or(0.0), n.get(1).copied().unwrap_or(1.0));
                let value = n.get(2).copied().unwrap_or(min);
                params.push(CodeParam { name: (*name).into(), value, min, max });
            }
            ["@color", name, hex, ..] if colors.len() < 8 && is_ident(name) => {
                colors.push(CodeColor { name: (*name).into(), value: parse_hex(hex).unwrap_or([1.0; 4]) });
            }
            _ => {}
        }
    }
    (params, colors)
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa` → linear-ish 0..1 floats (stored as
/// authored, like every other colour field).
pub fn parse_hex(hex: &str) -> Option<[f32; 4]> {
    let h = hex.trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok().map(|b| b as f32 / 255.0);
    match h.len() {
        3 => {
            let d = |i: usize| u8::from_str_radix(h.get(i..i + 1)?, 16).ok().map(|b| (b * 17) as f32 / 255.0);
            Some([d(0)?, d(1)?, d(2)?, 1.0])
        }
        6 => Some([byte(0)?, byte(2)?, byte(4)?, 1.0]),
        8 => Some([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => None,
    }
}

/// A new effect of the named kind (`"Blur"`, `"Ribbed Glass"`'s
/// `"RibbedGlass"` …) with its defaults — the editor's Add menu.
pub fn default_image_effect(kind: &str) -> Option<ImageEffect> {
    Some(match kind {
        "DropShadow" => ImageEffect::DropShadow(DropShadow::default()),
        "Stroke" => ImageEffect::Stroke(Stroke::default()),
        "Pattern" => ImageEffect::Pattern(Pattern::default()),
        "Halftone" => ImageEffect::Halftone(Halftone::default()),
        "Ascii" => ImageEffect::Ascii(Ascii::default()),
        "Grain" => ImageEffect::Grain(Grain::default()),
        "Glow" => ImageEffect::Glow(Glow::default()),
        "GradientMap" => ImageEffect::GradientMap(GradientMap::default()),
        "Blur" => ImageEffect::Blur(Blur::default()),
        "DepthMap" => ImageEffect::DepthMap(DepthMap::default()),
        "RibbedGlass" => ImageEffect::RibbedGlass(RibbedGlass::default()),
        "ScannerDrag" => ImageEffect::ScannerDrag(ScannerDrag::default()),
        "Code" => ImageEffect::Code(CodeEffect::default()),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_params_parse() {
        let (p, c) = parse_code_params(CODE_EFFECT_TEMPLATE);
        assert_eq!(p.len(), 4);
        assert_eq!(p[0].name, "ripple");
        assert_eq!((p[0].min, p[0].max, p[0].value), (0.0, 1.0, 0.5));
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].name, "tint");
        assert!((c[0].value[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn every_kind_has_a_default() {
        for k in ["DropShadow", "Stroke", "Pattern", "Halftone", "Ascii", "Grain", "Glow", "GradientMap", "Blur", "DepthMap", "RibbedGlass", "ScannerDrag", "Code"] {
            assert!(default_image_effect(k).is_some(), "{k}");
        }
    }

    #[test]
    fn postcard_round_trip() {
        let fx = ImageEffects { background: [0.1, 0.2, 0.3, 1.0], clip_to_shape: true, background_radius: 12.0, ..Default::default() };
        let back: ImageEffects = postcard::from_bytes(&postcard::to_allocvec(&fx).unwrap()).unwrap();
        assert_eq!(fx, back);
        let fx = ImageEffects { effects: ["Blur", "RibbedGlass", "ScannerDrag", "Code", "GradientMap"].iter().map(|k| ImageEffectLayer { effect: default_image_effect(k).unwrap(), enabled: true }).collect(), ..Default::default() };
        let bytes = postcard::to_allocvec(&fx).unwrap();
        let back: ImageEffects = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(fx, back);
    }
}
