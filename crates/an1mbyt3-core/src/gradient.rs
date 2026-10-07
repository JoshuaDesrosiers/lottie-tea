//! Gradient fills for [`crate::ShapeNode`]/[`crate::TextNode`] — Affinity
//! Designer's Fill tool is the reference: a shape's fill is either a flat
//! color (`fill_color`, unchanged) or a gradient (`gradient: Some(..)`)
//! defined by two points (`start`/`end`) and an ordered list of color
//! [`GradientStop`]s along the line/ray/sweep between them, in one of three
//! [`GradientKind`]s.
//!
//! Additive, not a restructure: `fill_color` stays exactly what it always
//! was (the flat-fill value, and the fallback if `gradient` is ever cleared)
//! rather than becoming an enum payload — the same "don't invasively
//! restructure an existing field, add an optional one that overrides it"
//! choice `StateDef::blend`/`Blend1DDef` already made for the identical
//! reason (every existing construction site, `.tea`/`.teab` file, and
//! reader of `fill_color` keeps working unchanged).

use glam::Vec2;
use serde::{Deserialize, Serialize};

/// Which of Affinity's gradient shapes this is — each interprets
/// `GradientDef::start`/`end` differently (see that struct's own doc
/// comment for exactly how) but shares the same stop list and 0..1
/// parameterization along whichever axis/region it defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GradientKind {
    /// Color varies linearly along the `start -> end` axis; perpendicular
    /// to that axis it's constant. Affinity's default gradient type.
    Linear,
    /// Color varies with distance from `start`, out to the radius
    /// `|end - start|` — concentric rings, `end` marking where the last
    /// stop's color is fully reached.
    Radial,
    /// Color sweeps a full turn around `start`, with `end` marking the
    /// reference direction (angle `0.0`) the first stop starts at —
    /// Affinity's "Conic"/"Angle" gradient.
    Conic,
    /// Color varies with the *larger* of the two distances along the
    /// `start -> end` axis and its perpendicular (a Chebyshev/max-norm
    /// distance from `start`, both normalized by `|end - start|`) —
    /// square, corner-anchored isocontours rather than radial's circular
    /// ones, which is what gives this its "L-shaped" band look when `start`
    /// sits at (or near) a corner of the filled shape. Affinity's
    /// "Bilinear" gradient.
    Bilinear,
}

/// One color stop along a gradient's `0.0..=1.0` parameter range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    /// Where along the gradient this stop's color sits — clamped to
    /// `0.0..=1.0` by every authoring path (`WasmEditor::add_gradient_stop`
    /// and friends), not enforced here (a plain data struct, same
    /// "authoring UI's job, not this type's" convention as `Keyframe::time`).
    pub offset: f32,
    /// Straight (non-premultiplied) linear RGBA, same convention as
    /// `ShapeNode::fill_color`.
    pub color: [f32; 4],
    /// Where the *perceptual* 50%-blend point sits within the segment
    /// running from this stop to the *next* one (by `offset`), as a
    /// `0.0..=1.0` fraction of that segment's own span — Affinity's
    /// draggable "midpoint" diamond between two adjacent stops. `0.5`
    /// (the plain, unbiased lerp every stop defaults to) means the blend
    /// is exactly halfway between the two colors at the segment's own
    /// midpoint; pulling it toward `0.0` or `1.0` compresses the transition
    /// toward the *other* stop, making this stop's color perceptually
    /// dominate more of the segment (see `KeyframeValue`-style easing bias,
    /// though this biases a color lerp, not a keyframe's time). Unused on
    /// the *last* stop in `GradientDef::stops` (there's no segment after
    /// it) — kept as a field on every stop anyway (rather than a separate
    /// `stops.len() - 1`-sized array) so reordering/inserting/removing
    /// stops never has a second array to keep in sync. `#[serde(default)]`
    /// (`0.5`) for back-compat: every gradient saved before this field
    /// existed already behaved as an unbiased lerp.
    #[serde(default = "default_midpoint")]
    pub midpoint: f32,
}

fn default_midpoint() -> f32 {
    0.5
}

/// A gradient fill — see this module's own doc comment for why it's an
/// additive `Option<GradientDef>` alongside `fill_color`, not a
/// restructure of it.
///
/// `stops` should have at least two entries and stay sorted by ascending
/// `offset` for the renderer's per-fragment lerp to produce a sane result
/// (see `an1mbyt3-renderer`'s `shader.wgsl`) — the authoring UI
/// (`WasmEditor::set_gradient`/`add_gradient_stop`) is responsible for
/// keeping that invariant; this type itself doesn't enforce it, same
/// "caller's job" convention as `StateMachineDef::states`' own ordering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradientDef {
    pub kind: GradientKind,
    /// Local (pre-transform, path-space) coordinates — the same space a
    /// `ShapeNode::path`'s own points live in, so the gradient moves/scales/
    /// rotates with the shape exactly like its geometry does, with no
    /// separate "gradient transform" to keep in sync.
    pub start: Vec2,
    /// See [`GradientKind`]'s own doc comment for how this is interpreted
    /// per kind (axis endpoint / radius point / angle reference).
    pub end: Vec2,
    pub stops: Vec<GradientStop>,
}
