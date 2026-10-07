//! How a shape's fill composites against whatever's already drawn behind it
//! — moved here from `an1mbyt3-renderer` (Phase 10 originally defined this
//! purely as a per-`DrawCall` rendering concept, set by whatever code built
//! the call) for the same reason `LoopMode`/`PlaybackDirection` moved out of
//! `an1mbyt3-runtime` earlier: a `ShapeNode` needs to reference this to be
//! part of the saved `Document`, and `an1mbyt3-core` cannot depend on
//! `an1mbyt3-renderer` (dependency arrows flow one way — AN1MBYT3_BUILD_PLAN.md
//! §3). Unlike those two enums, this one's *behavior* (mapping a mode to a
//! `wgpu::BlendState`) can't move here too — `an1mbyt3-core` has no `wgpu`
//! dependency and shouldn't gain one just for this — so `an1mbyt3-renderer`
//! keeps that logic as a free function (`wgpu_blend_state`) over this type
//! instead of the inherent-`impl` it used to be (Rust's orphan rule forbids
//! an inherent `impl` on a type defined in a different crate; a free
//! function has no such restriction).

use serde::{Deserialize, Serialize};

/// See this module's doc comment for why this moved here from
/// `an1mbyt3-renderer`, and why the actual blend-state math stays there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum BlendMode {
    /// Standard "over" alpha compositing — the only mode that existed before
    /// Phase 10, and every shape's default.
    #[default]
    Normal,
    /// Darkens: multiplies color channels against the destination.
    Multiply,
    /// Lightens: the color-inverted complement of `Multiply`.
    Screen,
    /// Additive ("linear dodge"): sums color channels, clamping at white.
    Add,
}
