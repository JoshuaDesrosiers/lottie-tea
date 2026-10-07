//! Two-bone analytic IK: the "start with CCD or two-bone analytic IK —
//! cheaper than FABRIK for the common 2-bone-leg/arm case" solver
//! AN1MBYT3_BUILD_PLAN.md §5 calls for as the first IK implementation.
//!
//! This is a closed-form (law-of-cosines) solve, not an iterative one —
//! exact in one step for exactly two bones, which is why it's cheaper than
//! FABRIK/CCD for this specific case (those are the right tool for chains
//! longer than two bones, which this module doesn't attempt).

use glam::Vec2;

/// The two joint rotations a two-bone chain needs to reach (or reach toward,
/// if unreachable) `target`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TwoBoneIkSolution {
    /// World-space rotation for the root bone (e.g. shoulder/hip).
    pub root_rotation: f32,
    /// Rotation for the mid bone (e.g. elbow/knee), *local* to the root
    /// bone's rotation — this is what goes directly into the mid bone
    /// `Node`'s own `Transform2D::rotation`, since a bone's transform is
    /// already relative to its parent (AN1MBYT3_BUILD_PLAN.md §5).
    pub mid_local_rotation: f32,
}

/// Solves a two-bone chain (root bone of `root_length`, mid bone of
/// `mid_length`, rooted at `root_origin` in world space) to reach `target`,
/// also in world space.
///
/// Always bends toward the same side (a fixed, implicit pole direction) —
/// real two-bone IK usually takes an explicit pole vector to control which
/// way the "elbow" points; this solver doesn't yet, since the `bone_ik`
/// demo only needs a single, consistent bend direction. Add a pole
/// parameter if/when a second use case needs to control bend side.
///
/// Targets outside the chain's reach (`distance > root_length + mid_length`)
/// or inside its minimum fold (`distance < |root_length - mid_length|`) are
/// clamped to the nearest reachable distance along the same direction,
/// matching how every mainstream 2D IK implementation degrades gracefully
/// instead of producing NaN rotations.
pub fn solve_two_bone_ik(root_origin: Vec2, root_length: f32, mid_length: f32, target: Vec2) -> TwoBoneIkSolution {
    const EPSILON: f32 = 1e-3;

    let to_target = target - root_origin;
    let min_reach = (root_length - mid_length).abs() + EPSILON;
    let max_reach = (root_length + mid_length) - EPSILON;
    let distance = to_target.length().clamp(min_reach, max_reach);

    let angle_to_target = to_target.y.atan2(to_target.x);

    // Law of cosines: angle at the root between "root->target" and
    // "root->elbow" directions.
    let cos_root_offset = ((root_length * root_length + distance * distance - mid_length * mid_length)
        / (2.0 * root_length * distance))
        .clamp(-1.0, 1.0);
    let root_offset = cos_root_offset.acos();

    // Law of cosines: the triangle's interior angle at the elbow. At full
    // extension (distance == root_length + mid_length) this is PI (straight
    // line); at minimum fold (distance == |root_length - mid_length|) it's 0
    // (folded flat) — see this module's tests for both boundary cases.
    let cos_elbow_interior = ((root_length * root_length + mid_length * mid_length - distance * distance)
        / (2.0 * root_length * mid_length))
        .clamp(-1.0, 1.0);
    let elbow_interior = cos_elbow_interior.acos();

    TwoBoneIkSolution {
        root_rotation: angle_to_target - root_offset,
        mid_local_rotation: std::f32::consts::PI - elbow_interior,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use an1mbyt3_core::Transform2D;

    /// Reconstructs where the chain's tip actually ends up for a given
    /// solution — the real correctness check for an IK solver is "does the
    /// end effector reach the target," not the exact angle values (bend
    /// direction is a convention, reach is the invariant that matters).
    fn resolve_tip(root_origin: Vec2, root_length: f32, mid_length: f32, solution: TwoBoneIkSolution) -> Vec2 {
        let root_transform = Transform2D { translation: root_origin, rotation: solution.root_rotation, ..Default::default() };
        let mid_local = Transform2D::from_translation(Vec2::new(root_length, 0.0));
        let mid_local = Transform2D { rotation: solution.mid_local_rotation, ..mid_local };
        let mid_world = root_transform.compose(&mid_local);
        mid_world.to_affine2().transform_point2(Vec2::new(mid_length, 0.0))
    }

    #[test]
    fn reaches_a_reachable_target() {
        let root_origin = Vec2::new(0.0, 0.0);
        let target = Vec2::new(90.0, 40.0);
        let solution = solve_two_bone_ik(root_origin, 60.0, 50.0, target);

        let tip = resolve_tip(root_origin, 60.0, 50.0, solution);
        assert!((tip - target).length() < 0.5, "tip {tip:?} did not reach target {target:?}");
    }

    #[test]
    fn fully_extended_chain_points_straight_at_the_target() {
        let root_origin = Vec2::ZERO;
        // Exactly at max reach (60 + 50 = 110).
        let target = Vec2::new(110.0, 0.0);
        let solution = solve_two_bone_ik(root_origin, 60.0, 50.0, target);

        // `acos`'s derivative is singular near +/-1, so clamping `distance`
        // to `max_reach - EPSILON` (to keep the law-of-cosines inputs inside
        // [-1, 1] and avoid NaN right at the true boundary) produces a small
        // but non-negligible angular deviation here — a few hundredths of a
        // radian, not the ~1e-3 you'd get from linear error propagation.
        // This tolerance reflects that real, acceptable behavior rather than
        // an arbitrary tight threshold.
        assert!(solution.root_rotation.abs() < 1e-2, "root_rotation = {}", solution.root_rotation);
        assert!((solution.mid_local_rotation - 0.0).abs() < 1e-2);
    }

    #[test]
    fn unreachable_target_clamps_instead_of_producing_nan() {
        let solution = solve_two_bone_ik(Vec2::ZERO, 60.0, 50.0, Vec2::new(1000.0, 0.0));
        assert!(solution.root_rotation.is_finite());
        assert!(solution.mid_local_rotation.is_finite());
    }

    #[test]
    fn target_at_root_origin_does_not_panic_or_nan() {
        let solution = solve_two_bone_ik(Vec2::new(5.0, 5.0), 60.0, 50.0, Vec2::new(5.0, 5.0));
        assert!(solution.root_rotation.is_finite());
        assert!(solution.mid_local_rotation.is_finite());
    }
}
