//! Whole-scene post-processing effects — moved here from `an1mbyt3-renderer`
//! (Phase 11 originally defined this purely as a per-render-call rendering
//! concept, passed in fresh by whatever code called `render_with_post_effect`)
//! for the same reason `BlendMode` moved out of `an1mbyt3-renderer` earlier:
//! an `Artboard` needs to reference this to save an authored effect stack as
//! part of the document (AN1MBYT3_BUILD_PLAN.md §11's "layered approach,
//! allow for duplicate effects" post-processing panel), and `an1mbyt3-core`
//! cannot depend on `an1mbyt3-renderer` (dependency arrows flow one way —
//! AN1MBYT3_BUILD_PLAN.md §3). Unlike `BlendMode`, this type has no
//! renderer-side behavior to leave behind at all (`PostEffect` is pure data;
//! all the actual GPU work lives in `an1mbyt3-renderer`'s
//! `render_with_post_effects`/`post_process.wgsl`) — `an1mbyt3-renderer`
//! just re-exports this exact type (`pub use an1mbyt3_core::PostEffect;`)
//! rather than keeping a parallel copy to convert between.

use serde::{Deserialize, Serialize};

/// One whole-scene post-processing effect. A property of *how a frame is
/// rendered*, not of any individual shape/node — every shape still draws
/// through the exact same flat-fill pipeline it always has; this only
/// affects what happens to the combined result afterward.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum PostEffect {
    /// No-op placeholder — a stack entry can be *disabled* instead of
    /// deleted (see [`PostEffectLayer::enabled`]) without needing this
    /// variant for that; this exists for API completeness (a caller that
    /// wants "no effect" as a value, e.g. a UI's "not yet chosen" state)
    /// and degrades a whole stack to nothing rendered on top at all —
    /// see `an1mbyt3_renderer::Renderer::render_with_post_effects`. Also
    /// `PostEffect`'s `Default` — the identity value for anything that
    /// needs a starting/no-op effect, e.g. `ImageProcessing::effect`.
    #[default]
    None,
    /// Blurs the whole rendered scene. `radius` scales the blur kernel's
    /// sample spacing in texel units — `1.0` samples adjacent texels,
    /// larger values spread the kernel wider (blurrier, but also more
    /// likely to show banding/undersampling artifacts at extreme values).
    Blur { radius: f32 },
    /// A bright-pass-and-blur layer, additively composited back over the
    /// unblurred scene — the standard real-time "glow"/bloom technique.
    /// `threshold` (in `0.0..=1.0`, compared against each sampled pixel's
    /// max channel) is how bright a pixel must be to contribute to the glow
    /// at all; `intensity` scales the glow layer's brightness before it's
    /// added back.
    Glow { radius: f32, threshold: f32, intensity: f32 },
    /// Snaps the whole rendered scene onto a grid of solid-color blocks —
    /// the standard retro/"censor mosaic" look. `block_size` is a block's
    /// edge length in screen pixels (`8.0` means 8x8-pixel blocks).
    Pixelate { block_size: f32 },
    /// Adjusts the whole rendered scene's brightness/contrast/saturation —
    /// `1.0`/`1.0`/`0.0` for contrast/saturation/brightness respectively are
    /// each that property's identity (no change). `brightness` is an
    /// additive offset in `0.0..=1.0` RGB units; `contrast` scales
    /// deviation from mid-gray (`0.5`); `saturation` scales deviation from
    /// the sampled pixel's own luminance (`0.0` = grayscale, `1.0` =
    /// unchanged, `>1.0` = oversaturated).
    ColorGrading { brightness: f32, contrast: f32, saturation: f32 },
    /// Quantizes each RGB channel to `levels` evenly-spaced steps — the
    /// classic flat-banded "posterize" look. `2.0` is stark black/white-per-
    /// channel; higher values keep more of the original gradation. Alpha is
    /// left untouched (only color is posterized).
    Posterize { levels: f32 },
    /// Remaps the whole scene's *luminance* onto a two-color gradient —
    /// `shadow` for the darkest pixels, `highlight` for the brightest, with
    /// everything between linearly interpolated — the standard "duotone"/
    /// gradient-map look. Scoped to exactly two colors (not an arbitrary
    /// multi-stop gradient) since `PostUniforms` (the post-process shader's
    /// uniform buffer, `an1mbyt3-renderer`) is a small fixed-size struct;
    /// each color packs into one of its existing `vec4` slots the same way
    /// `ColorGrading`'s three scalars already do. A real 3-4 channel color
    /// (alpha kept at `1.0` and unused) rather than a `GradientDef`
    /// reference — this maps luminance to color, it doesn't sample an
    /// authored `crate::gradient::GradientDef` (that type's `stops` are
    /// unbounded-length, which doesn't fit a fixed uniform slot without a
    /// storage buffer — a possible future upgrade, not this pass).
    GradientMap { shadow: [f32; 4], highlight: [f32; 4] },
    /// Darkens/tints the frame's edges toward `color` — URP's Vignette.
    /// `radius` is the clear area's reach (1 ~ half-diagonal), `blur` its
    /// edge softness, `intensity` how strongly the edge color covers.
    /// Appended last (variant index stability for `.teab`).
    Vignette { color: [f32; 4], intensity: f32, radius: f32, blur: f32 },
    // ---- Appended (variant index stability for `.teab`) ----
    /// Red / cyan 3D-glasses look: the red channel shifted one way, green
    /// and blue the other, by `offset` pixels. `mono` (0..1) greys the
    /// image first (classic anaglyph prints).
    Anaglyph { offset: f32, mono: f32 },
    /// Ordered (Bayer) dithering: quantizes to `levels` steps per channel
    /// with a 4×4 threshold pattern, `scale` screen pixels per pattern
    /// pixel (match your pixel size), `strength` 0..1. `mono` (0/1) makes it
    /// a 1-bit-style two-colour dither between `dark` and `light`
    /// (Game Boy / Obra Dinn looks).
    Dither { levels: f32, scale: f32, strength: f32, mono: f32, dark: [f32; 4], light: [f32; 4] },
    /// Lens colour fringing: red and blue split radially from the centre,
    /// `strength` pixels at the corners.
    ChromaticAberration { strength: f32 },
    /// CRT scanlines: dark lines every `spacing` pixels at `intensity`
    /// (0..1), plus an RGB aperture-grille `mask` (0..1).
    Scanlines { intensity: f32, spacing: f32, mask: f32 },
    /// Film grain: animated noise, `amount` 0..1, grain `size` in pixels.
    FilmGrain { amount: f32, size: f32 },
    /// Sharpens edges (unsharp mask), `amount` 0..2.
    Sharpen { amount: f32 },
    /// Ink outlines where brightness or opacity changes sharply —
    /// `thickness` pixels, `threshold` 0..1 sensitivity, in `color`; `only`
    /// (0/1) draws the lines alone on transparent.
    EdgeDetect { thickness: f32, threshold: f32, color: [f32; 4], only: f32 },
    /// Print halftone: the image as dots of `ink` on `paper`, dots
    /// `size` pixels apart at `angle` degrees; `mix` 0..1 blends with the
    /// original colours.
    Halftone { size: f32, angle: f32, ink: [f32; 4], paper: [f32; 4], mix: f32 },
    /// Wavy distortion (heat haze, underwater, dream): rows shift by up to
    /// `amplitude` pixels, `frequency` waves per screen height, moving at
    /// `speed`.
    WaveDistortion { amplitude: f32, frequency: f32, speed: f32 },
    /// Lens distortion: barrel (`strength` > 0, fisheye / CRT bulge) or
    /// pincushion (< 0); `zoom` > 1 crops in to hide the edges.
    LensDistortion { strength: f32, zoom: f32 },
    /// **Palette**: every pixel snaps to the nearest of the first `count`
    /// of `colors` (perceptual / Oklab distance) — a fixed retro palette
    /// (PICO-8, Game Boy, NES …) over the whole frame. `dither` (0..1)
    /// mixes the two nearest colours with an ordered (Bayer) pattern,
    /// `scale` screen pixels per pattern cell; `mix` (0..1) blends with the
    /// original colours. A fixed-size array (so the effect stays `Copy`
    /// and fits one uniform block); up to [`PALETTE_MAX`] colours.
    Palette { colors: [[f32; 4]; PALETTE_MAX], count: u32, dither: f32, scale: f32, mix: f32 },
    /// **Pixel Scatter**: every grain of `size` px shows the image from a
    /// random spot up to `amount` px away — frosted glass, sand, static,
    /// dissolving. `speed` re-rolls the scatter that many times a second
    /// (0 = still).
    PixelScatter { amount: f32, size: f32, speed: f32 },
    /// **Gradient Map** with any number of stops (up to
    /// [`GRADIENT_MAP_MAX`]): the image's brightness picks a colour along
    /// the first `count` stops (`offsets` ascending, 0 = darkest, 1 =
    /// brightest); `mix` (0..1) blends with the original. Supersedes the
    /// two-colour [`PostEffect::GradientMap`] (which still loads and is
    /// edited as two stops).
    GradientMapStops { colors: [[f32; 4]; GRADIENT_MAP_MAX], offsets: [f32; GRADIENT_MAP_MAX], count: u32, mix: f32 },
    /// **Global texture**: a [`crate::image_effects::Texture`] laid over the
    /// whole frame (canvas, paper, grain …) for a finished, tactile look.
    /// Not a post-chain op: the editor draws the scene as one layer and
    /// runs the texture on it (`renderer/layer_fx.rs`). (Appended last.)
    Texture(crate::image_effects::Texture),
}

/// The most stops a [`PostEffect::GradientMapStops`] holds.
pub const GRADIENT_MAP_MAX: usize = 16;

/// The most colours a [`PostEffect::Palette`] holds.
pub const PALETTE_MAX: usize = 32;

/// PICO-8's 16 colours — the Palette effect's starting palette.
pub const PICO8_PALETTE: [[f32; 4]; 16] = [
    [0.0, 0.0, 0.0, 1.0],
    [0.114, 0.169, 0.325, 1.0],
    [0.494, 0.145, 0.325, 1.0],
    [0.0, 0.529, 0.318, 1.0],
    [0.671, 0.322, 0.212, 1.0],
    [0.373, 0.341, 0.310, 1.0],
    [0.761, 0.765, 0.780, 1.0],
    [1.0, 0.945, 0.910, 1.0],
    [1.0, 0.0, 0.302, 1.0],
    [1.0, 0.639, 0.0, 1.0],
    [1.0, 0.925, 0.153, 1.0],
    [0.0, 0.894, 0.212, 1.0],
    [0.161, 0.678, 1.0, 1.0],
    [0.514, 0.463, 0.612, 1.0],
    [1.0, 0.467, 0.659, 1.0],
    [1.0, 0.8, 0.667, 1.0],
];

impl PostEffect {
    /// A freshly-added stack entry's starting parameters — the Effects
    /// panel's "+ Add Effect" action picks a kind, then starts it here
    /// rather than at all-zero fields (a `radius: 0.0` blur/glow, or a
    /// `block_size: 0.0` pixelate, would visibly do nothing until the user
    /// happened to drag its slider away from zero first).
    pub fn default_for_kind(kind: &str) -> Option<PostEffect> {
        match kind {
            "Blur" => Some(PostEffect::Blur { radius: 4.0 }),
            "Glow" => Some(PostEffect::Glow { radius: 6.0, threshold: 0.7, intensity: 1.0 }),
            "Pixelate" => Some(PostEffect::Pixelate { block_size: 8.0 }),
            "ColorGrading" => Some(PostEffect::ColorGrading { brightness: 0.0, contrast: 1.0, saturation: 1.0 }),
            "Posterize" => Some(PostEffect::Posterize { levels: 4.0 }),
            "GradientMap" => Some(PostEffect::GradientMap { shadow: [0.05, 0.0, 0.2, 1.0], highlight: [1.0, 0.8, 0.3, 1.0] }),
            "Vignette" => Some(PostEffect::Vignette { color: [0.0, 0.0, 0.0, 1.0], intensity: 0.45, radius: 0.75, blur: 0.5 }),
            "Anaglyph" => Some(PostEffect::Anaglyph { offset: 4.0, mono: 0.0 }),
            "Dither" => Some(PostEffect::Dither { levels: 4.0, scale: 1.0, strength: 1.0, mono: 0.0, dark: [0.06, 0.22, 0.06, 1.0], light: [0.61, 0.74, 0.06, 1.0] }),
            "ChromaticAberration" => Some(PostEffect::ChromaticAberration { strength: 4.0 }),
            "Scanlines" => Some(PostEffect::Scanlines { intensity: 0.35, spacing: 3.0, mask: 0.25 }),
            "FilmGrain" => Some(PostEffect::FilmGrain { amount: 0.12, size: 1.5 }),
            "Sharpen" => Some(PostEffect::Sharpen { amount: 0.6 }),
            "EdgeDetect" => Some(PostEffect::EdgeDetect { thickness: 1.0, threshold: 0.2, color: [0.05, 0.04, 0.08, 1.0], only: 0.0 }),
            "Halftone" => Some(PostEffect::Halftone { size: 6.0, angle: 45.0, ink: [0.08, 0.08, 0.12, 1.0], paper: [0.98, 0.96, 0.9, 1.0], mix: 0.0 }),
            "WaveDistortion" => Some(PostEffect::WaveDistortion { amplitude: 4.0, frequency: 6.0, speed: 2.0 }),
            "LensDistortion" => Some(PostEffect::LensDistortion { strength: 0.25, zoom: 1.05 }),
            "PixelScatter" => Some(PostEffect::PixelScatter { amount: 3.0, size: 1.0, speed: 0.0 }),
            "Palette" => {
                let mut colors = [[0.0, 0.0, 0.0, 1.0]; PALETTE_MAX];
                colors[..PICO8_PALETTE.len()].copy_from_slice(&PICO8_PALETTE);
                Some(PostEffect::Palette { colors, count: PICO8_PALETTE.len() as u32, dither: 0.0, scale: 1.0, mix: 1.0 })
            }
            _ => None,
        }
    }
}

/// One entry in an artboard's post-processing stack — see
/// `Artboard::post_effects`'s own doc comment for why this lives in a plain
/// ordered `Vec` rather than an id-keyed collection.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PostEffectLayer {
    pub effect: PostEffect,
    /// Lets an entry be temporarily switched off without losing its
    /// authored parameters/position in the stack — "delete, but not
    /// really" would otherwise be the only way to A/B an effect, the same
    /// reasoning most stacked-effect tools (Photoshop adjustment layers,
    /// Affinity's own effect stack) expose a per-layer visibility toggle.
    pub enabled: bool,
}
