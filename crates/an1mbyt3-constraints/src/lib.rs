//! `an1mbyt3-constraints`: bones, IK, aim/look-at, follow, distance, transform constraints.
//!
//! See AN1MBYT3_BUILD_PLAN.md §5 — constraints are first-class document objects
//! (§4), evaluated in a topologically-sorted pass after base transforms and before
//! rendering, so a bone's constraint can safely depend on another bone already
//! resolved that frame.
//!
//! Owns (so far): [`solve_two_bone_ik`] — closed-form two-bone IK (§5's "start
//! with CCD or two-bone analytic IK" for the common leg/arm case). It's a
//! pure function over positions/lengths, not tied to the document model —
//! callers (currently the `bone_ik` example) read a chain's bone lengths and
//! world origin from `an1mbyt3_core::Artboard`/`NodeKind::Bone`, solve, and
//! write the resulting rotations back onto the bones' own `Transform2D`.
//!
//! Does NOT own (yet): aim/look-at/follow/distance constraints, CCD/FABRIK
//! for chains longer than two bones, vertex skinning math itself (lives in
//! `an1mbyt3-geometry`, since it's mesh deformation, not constraint
//! solving), or animation playback timing (see `an1mbyt3-runtime`) — see
//! AN1MBYT3_BUILD_PLAN.md §10 Phase 5's remaining scope.
//!
//! Depends on `an1mbyt3-core` (bone/transform types) and `an1mbyt3-geometry`
//! (declared for the skinning work Phase 5 still has left; not yet used by
//! the IK solver itself, which only needs `Transform2D` composition).
//!
//! Status: Phase 5 (AN1MBYT3_BUILD_PLAN.md §10) — two-bone IK implemented
//! and demoed (`examples/bone_ik.rs`). Skinning/deformation, longer IK
//! chains, and the other constraint types are not implemented yet.

mod chain;
mod ik;

pub use chain::{aim_bone, apply_pose, bone_chain, solve_chain, solve_ik_constraints};
pub use ik::{solve_two_bone_ik, TwoBoneIkSolution};
