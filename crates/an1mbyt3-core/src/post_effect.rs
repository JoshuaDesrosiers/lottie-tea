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
}

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
