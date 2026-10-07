//! Easing curves: how a [`crate::Track`] interpolates *between* two keyframes,
//! not just *what* it interpolates (that's the [`crate::Lerp`] trait).
//!
//! AN1MBYT3_BUILD_PLAN.md §5 calls for "Bézier/easing curves" — `CubicBezier`
//! implements the same two-control-point curve model as CSS's
//! `cubic-bezier()` and most animation tools (After Effects' "easy ease",
//! Rive's own curve editor): control points at `(0,0)` and `(1,1)` are fixed,
//! `(x1,y1)`/`(x2,y2)` are the two free handles.
//!
//! Lives in `an1mbyt3-core`, not `an1mbyt3-runtime`, alongside [`crate::Keyframe`]/
//! [`crate::Track`]/[`crate::AnimationClip`] — see `animation.rs`'s module doc
//! for why authored keyframe data belongs in the document model, not just
//! runtime evaluation state.

use serde::{Deserialize, Serialize};

/// How progress through a keyframe segment (`0.0..=1.0`, linear in time) maps
/// to eased progress (`0.0..=1.0`, used as the `t` in a [`crate::Lerp::lerp`] call).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Easing {
    /// No easing: eased progress equals linear progress.
    Linear,
    /// A cubic Bézier curve from `(0,0)` to `(1,1)` via two control points —
    /// e.g. `CubicBezier { x1: 0.42, y1: 0.0, x2: 0.58, y2: 1.0 }` is the
    /// classic "ease-in-out" curve.
    CubicBezier { x1: f32, y1: f32, x2: f32, y2: f32 },
}

impl Easing {
    /// Maps linear progress `t` (expected in `0.0..=1.0`) to eased progress.
    pub fn ease(&self, t: f32) -> f32 {
        match *self {
            Easing::Linear => t,
            Easing::CubicBezier { x1, y1, x2, y2 } => cubic_bezier_ease(t, x1, y1, x2, y2),
        }
    }
}

/// Solves `t` such that the Bézier curve's X component equals `x_target`,
/// then returns the curve's Y component at that `t`.
///
/// This is the standard CSS-`cubic-bezier()` evaluation approach: the curve
/// is parametric in an internal `t` that does *not* itself equal elapsed
/// time, so we Newton's-method-refine an initial linear guess until the
/// curve's X matches the requested progress, then read off Y. A fixed
/// iteration count (rather than a convergence-threshold loop) keeps this
/// branch-predictable and allocation-free, which matters since it runs once
/// per animated property per frame.
fn cubic_bezier_ease(x_target: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    // Bézier(t) coefficients for a curve from (0,0) through (x1,y1),(x2,y2) to (1,1).
    let cx = 3.0 * x1;
    let bx = 3.0 * (x2 - x1) - cx;
    let ax = 1.0 - cx - bx;

    let cy = 3.0 * y1;
    let by = 3.0 * (y2 - y1) - cy;
    let ay = 1.0 - cy - by;

    let sample_x = |t: f32| ((ax * t + bx) * t + cx) * t;
    let sample_derivative_x = |t: f32| (3.0 * ax * t + 2.0 * bx) * t + cx;
    let sample_y = |t: f32| ((ay * t + by) * t + cy) * t;

    let mut t = x_target;
    for _ in 0..8 {
        let x = sample_x(t) - x_target;
        let derivative = sample_derivative_x(t);
        if derivative.abs() < 1e-6 {
            break;
        }
        t -= x / derivative;
    }

    sample_y(t.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_is_identity() {
        assert_eq!(Easing::Linear.ease(0.0), 0.0);
        assert_eq!(Easing::Linear.ease(0.5), 0.5);
        assert_eq!(Easing::Linear.ease(1.0), 1.0);
    }

    #[test]
    fn cubic_bezier_hits_endpoints() {
        let ease_in_out = Easing::CubicBezier { x1: 0.42, y1: 0.0, x2: 0.58, y2: 1.0 };
        assert!((ease_in_out.ease(0.0) - 0.0).abs() < 1e-3);
        assert!((ease_in_out.ease(1.0) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn ease_in_out_is_slower_at_the_ends_than_linear() {
        // Classic ease-in-out is *behind* linear progress early in the segment.
        let ease_in_out = Easing::CubicBezier { x1: 0.42, y1: 0.0, x2: 0.58, y2: 1.0 };
        assert!(ease_in_out.ease(0.1) < 0.1);
    }
}
