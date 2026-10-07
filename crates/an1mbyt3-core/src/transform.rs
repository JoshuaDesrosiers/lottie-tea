//! 2D affine transform: the transform every [`crate::Node`] carries.
//!
//! Stored as separate translation/rotation/scale components (a "TRS" transform)
//! rather than a raw matrix, because that's what the timeline needs to keyframe
//! and interpolate independently later (AN1MBYT3_BUILD_PLAN.md §5) — you can't
//! sanely lerp two arbitrary matrices and get a visually correct in-between pose,
//! but you can lerp translation/rotation/scale components independently. The
//! renderer/geometry crates convert to a `glam::Affine2` matrix only at the point
//! they actually need one (§6: "Transform-only animation should update matrices
//! rather than rebuild geometry").

use glam::{Affine2, Vec2};
use serde::{Deserialize, Serialize};

/// A node's position, rotation, and scale relative to its parent.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform2D {
    pub translation: Vec2,
    /// Rotation in radians, counter-clockwise, matching `glam`'s convention.
    pub rotation: f32,
    pub scale: Vec2,
    /// Local-space point that rotation/scale pivot around, instead of the
    /// always-implicit local `(0, 0)` every other field here used to be
    /// stuck with. `#[serde(default)]` (`ZERO`) for back-compat: every
    /// `.tea`/`.teab` saved before this field existed keeps rendering
    /// exactly as before, since `ZERO` reproduces `to_affine2()`'s old
    /// formula exactly. See `to_affine2`'s doc comment for why `translation`
    /// (composed all the way to world via `Artboard::world_transform`)
    /// already *is* "the world position of this node's own anchor point" —
    /// no other method on this type needed to change to support this field.
    #[serde(default)]
    pub anchor: Vec2,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self {
            translation: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::ONE,
            anchor: Vec2::ZERO,
        }
    }
}

impl Transform2D {
    pub fn from_translation(translation: Vec2) -> Self {
        Self {
            translation,
            ..Default::default()
        }
    }

    /// Composes this transform into a single 2D affine matrix, for the
    /// renderer/geometry crates to consume — see the module doc for why the
    /// document model itself stores TRS components instead of a matrix.
    ///
    /// Subtracting `anchor` before the scale/rotate/translate step (rather
    /// than folding it into `translation` some other way) is what makes
    /// `anchor` map to exactly `translation` under this transform — i.e.
    /// `to_affine2().transform_point2(self.anchor) == self.translation`,
    /// always, by construction. That single invariant is why nothing else
    /// in this crate needed to change to support an anchor: `compose`/
    /// `local_for` already route translation through `to_affine2()`, so
    /// `translation`, composed all the way to world via
    /// `crate::Artboard::world_transform`, already *is* "the world position
    /// of this node's own anchor point" — the on-canvas rotate handle
    /// already pivots around `worldTransform.translation` and therefore
    /// already pivots around the anchor, for free.
    pub fn to_affine2(&self) -> Affine2 {
        Affine2::from_scale_angle_translation(self.scale, self.rotation, self.translation) * Affine2::from_translation(-self.anchor)
    }

    /// Returns a copy with `anchor` set to `new_anchor`, compensating
    /// `translation` so the node's rendered geometry does not visibly move
    /// — the standard "reposition the pivot, not the shape" convention
    /// (After Effects/Rive's anchor-point tool). Used both by the on-canvas
    /// anchor-drag handle (`WasmEditor::commit_node_anchor`) and by SVG
    /// import's one-time default-centering of an off-center imported path
    /// (`insert_imported_node`) — see this crate's callers for each.
    ///
    /// Derivation: for a fixed local point `p`, this transform's old and new
    /// affine matrices must agree — `translation + RS·(p - old_anchor) ==
    /// new_translation + RS·(p - new_anchor)` — which solves to
    /// `new_translation = translation + RS·(new_anchor - old_anchor)`. `RS`
    /// (rotation+scale, no translation) is applied via `transform_vector2`,
    /// not `transform_point2`, since this is a direction/magnitude delta,
    /// not a point.
    pub fn with_anchor_preserving_world(&self, new_anchor: Vec2) -> Transform2D {
        let rs = Affine2::from_scale_angle_translation(self.scale, self.rotation, Vec2::ZERO);
        Transform2D {
            translation: self.translation + rs.transform_vector2(new_anchor - self.anchor),
            anchor: new_anchor,
            ..*self
        }
    }

    /// Returns a new transform that positions something at `offset` in this
    /// transform's *local* space (i.e. `offset` gets rotated/scaled/translated
    /// by this transform, same as any other point in this node's local
    /// coordinate system), keeping this transform's own rotation and scale.
    ///
    /// This is how `an1mbyt3-text` (Phase 3) positions each glyph of a text
    /// node relative to that node's transform — a glyph's shaped-text offset
    /// is a local-space point, not a separate node, so rotating/scaling the
    /// text node correctly rotates/scales every glyph's position along with
    /// it. This is deliberately narrower than a general parent-to-child
    /// transform-propagation system for the scene graph's `Node::parent`
    /// hierarchy (AN1MBYT3_BUILD_PLAN.md §4) — that's separate, not-yet-built
    /// work for walking arbitrary nested nodes; this only composes a node's
    /// own transform with a local offset for that same node's own sub-parts.
    pub fn with_local_offset(&self, offset: Vec2) -> Transform2D {
        Transform2D {
            translation: self.to_affine2().transform_point2(offset),
            rotation: self.rotation,
            scale: self.scale,
            anchor: Vec2::ZERO,
        }
    }

    /// Composes this transform as *parent* with `child`'s transform,
    /// returning `child`'s effective transform in this transform's space —
    /// the general parent-to-child propagation `with_local_offset`'s doc
    /// comment says is separate, not-yet-built work. It's needed for real
    /// starting Phase 5 (AN1MBYT3_BUILD_PLAN.md §10): a bone chain's second
    /// bone is a *node* (not just a local offset) parented to the first, so
    /// computing its world transform means composing two full `Transform2D`s,
    /// not just carrying a point through one.
    ///
    /// This is TRS composition (translation/rotation/scale combined
    /// component-wise), not `to_affine2()` matrix-multiply-then-decompose —
    /// deliberately, per this module's doc comment on why the document model
    /// avoids raw matrices: composing rotation as a sum and scale as a
    /// product is exact and unambiguous for the rigid + uniform-scale
    /// transforms this engine uses; decomposing an arbitrary composed matrix
    /// back into TRS is not always unambiguous in general (shear, negative
    /// scale). `child.translation` is composed through this transform's full
    /// affine matrix (so it's correctly rotated/scaled by the parent), but
    /// rotation/scale themselves compose as plain sum/product.
    pub fn compose(&self, child: &Transform2D) -> Transform2D {
        Transform2D {
            translation: self.to_affine2().transform_point2(child.translation),
            rotation: self.rotation + child.rotation,
            scale: self.scale * child.scale,
            // The composed result is `child`'s *world* transform — it must
            // carry `child`'s own anchor forward (not this parent's, and
            // not zeroed) so that world transform's `to_affine2()` still
            // pivots around `child`'s real anchor, per this method's own
            // doc comment on what it's used for.
            anchor: child.anchor,
        }
    }

    /// The inverse of `compose`: given `self` as a prospective *parent* and
    /// `world` as a node's *current* world transform, returns the local
    /// transform that node would need under `self` for its world transform
    /// to stay exactly `world` — i.e. `self.compose(&self.local_for(world))
    /// == world`. This is what a reparent has to compute to preserve a
    /// node's on-canvas position/rotation/scale: `Node::parent` alone
    /// doesn't touch `Node::transform`, so without this, moving a node under
    /// a new parent silently *reinterprets* its existing (unchanged)
    /// `Transform2D` as relative to that new parent instead of the old one
    /// — a shape authored at world position (40, -40) under the artboard
    /// root, dropped onto a group that itself sits at (200, 100), would jump
    /// to world (240, 60) instead of staying at (40, -40). `editor.rs`'s
    /// `reparent_node` is the only caller, batching this alongside
    /// `Command::ReparentNode` as one undo step.
    pub fn local_for(&self, world: &Transform2D) -> Transform2D {
        Transform2D {
            translation: self.to_affine2().inverse().transform_point2(world.translation),
            rotation: world.rotation - self.rotation,
            scale: Vec2::new(world.scale.x / self.scale.x, world.scale.y / self.scale.y),
            // Anchor is intrinsic to the node, not something reparenting
            // should change — preserved from `world` unchanged, the inverse
            // of `compose` carrying `child.anchor` forward.
            anchor: world.anchor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_identity() {
        let t = Transform2D::default();
        assert_eq!(t.to_affine2(), Affine2::IDENTITY);
    }

    #[test]
    fn translation_moves_origin() {
        let t = Transform2D::from_translation(Vec2::new(10.0, 5.0));
        let moved = t.to_affine2().transform_point2(Vec2::ZERO);
        assert_eq!(moved, Vec2::new(10.0, 5.0));
    }

    #[test]
    fn with_local_offset_translates_by_parent_transform() {
        let t = Transform2D::from_translation(Vec2::new(100.0, 0.0));
        let offset_transform = t.with_local_offset(Vec2::new(10.0, 0.0));
        assert_eq!(offset_transform.translation, Vec2::new(110.0, 0.0));
        assert_eq!(offset_transform.rotation, t.rotation);
        assert_eq!(offset_transform.scale, t.scale);
    }

    #[test]
    fn compose_combines_translation_rotation_and_scale() {
        let parent = Transform2D::from_translation(Vec2::new(100.0, 0.0));
        let child = Transform2D::from_translation(Vec2::new(10.0, 0.0));

        let world = parent.compose(&child);

        assert_eq!(world.translation, Vec2::new(110.0, 0.0));
        assert_eq!(world.rotation, 0.0);
        assert_eq!(world.scale, Vec2::ONE);
    }

    #[test]
    fn local_for_inverts_compose_round_trip() {
        let parent = Transform2D { translation: Vec2::new(200.0, 100.0), rotation: 0.4, scale: Vec2::new(1.5, 1.5), anchor: Vec2::ZERO };
        let original_world = Transform2D { translation: Vec2::new(40.0, -40.0), rotation: 0.1, scale: Vec2::new(2.0, 2.0), anchor: Vec2::ZERO };

        let local = parent.local_for(&original_world);
        let recomposed_world = parent.compose(&local);

        assert!((recomposed_world.translation - original_world.translation).length() < 1e-4);
        assert!((recomposed_world.rotation - original_world.rotation).abs() < 1e-5);
        assert!((recomposed_world.scale - original_world.scale).length() < 1e-4);
    }

    #[test]
    fn local_for_against_an_identity_parent_returns_the_world_transform_unchanged() {
        let parent = Transform2D::default();
        let world = Transform2D { translation: Vec2::new(40.0, -40.0), rotation: 0.3, scale: Vec2::new(2.0, 3.0), anchor: Vec2::ZERO };
        let local = parent.local_for(&world);
        assert_eq!(local.translation, world.translation);
        assert_eq!(local.rotation, world.rotation);
        assert_eq!(local.scale, world.scale);
    }

    #[test]
    fn compose_rotates_child_translation_by_parent_rotation() {
        // Parent rotated 90 degrees CCW; a child offset of (1, 0) in the
        // parent's local space should land at world (0, 1), not (1, 1).
        let parent = Transform2D { rotation: std::f32::consts::FRAC_PI_2, ..Default::default() };
        let child = Transform2D::from_translation(Vec2::new(1.0, 0.0));

        let world = parent.compose(&child);

        assert!((world.translation.x - 0.0).abs() < 1e-5);
        assert!((world.translation.y - 1.0).abs() < 1e-5);
    }

    #[test]
    fn to_affine2_maps_the_anchor_point_to_translation() {
        // The invariant `to_affine2`'s doc comment relies on: whatever
        // `anchor` is, that exact local point always lands at `translation`.
        let t = Transform2D { translation: Vec2::new(50.0, -20.0), rotation: 0.6, scale: Vec2::new(2.0, 0.5), anchor: Vec2::new(3.0, 4.0) };
        let mapped = t.to_affine2().transform_point2(t.anchor);
        assert!((mapped - t.translation).length() < 1e-4);
    }

    #[test]
    fn nonzero_anchor_rotates_a_point_around_the_anchor_not_the_origin() {
        // `translation` is set equal to `anchor` so the pivot itself stays
        // fixed in world space (per `to_affine2_maps_the_anchor_point_to_translation`,
        // the anchor always maps to `translation` — leaving `translation` at
        // the default `(0,0)` here would move the pivot itself to the
        // origin, not keep it at `(1,0)`, which is exactly the mistake this
        // test's first draft made). With the pivot held fixed at world
        // (1, 0), rotating a point at local (2, 0) — one unit to the right
        // of the anchor — by 180 degrees should land it one unit to the
        // *left* of the anchor: world (0, 0).
        let t = Transform2D { rotation: std::f32::consts::PI, anchor: Vec2::new(1.0, 0.0), translation: Vec2::new(1.0, 0.0), ..Default::default() };
        let rotated = t.to_affine2().transform_point2(Vec2::new(2.0, 0.0));
        assert!((rotated - Vec2::new(0.0, 0.0)).length() < 1e-4, "expected (0,0), got {rotated:?}");
    }

    #[test]
    fn with_anchor_preserving_world_does_not_move_a_fixed_local_point() {
        let t = Transform2D { translation: Vec2::new(50.0, -20.0), rotation: 0.6, scale: Vec2::new(2.0, 0.5), anchor: Vec2::ZERO };
        let fixed_local_point = Vec2::new(7.0, -3.0);
        let world_before = t.to_affine2().transform_point2(fixed_local_point);

        let recentered = t.with_anchor_preserving_world(Vec2::new(3.0, 4.0));
        let world_after = recentered.to_affine2().transform_point2(fixed_local_point);

        assert!((world_after - world_before).length() < 1e-4, "expected {world_before:?}, got {world_after:?}");
        assert_eq!(recentered.anchor, Vec2::new(3.0, 4.0));
    }
}
