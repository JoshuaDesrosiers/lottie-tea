//! Linear blend skinning: deforming one continuous mesh by more than one
//! bone, so it bends smoothly across a joint instead of reading as two
//! separate rigid pieces — AN1MBYT3_BUILD_PLAN.md §5's "weighted
//! deformation/skinning," the one piece of Phase 5 left open after bones/IK
//! landed (see `an1mbyt3-constraints`'s `bone_ik` example, which uses two
//! separate rigid meshes with matching color/width as a cheaper illusion of
//! this — this module is the real thing).
//!
//! This is CPU-side skinning (recomputing vertex positions every frame in
//! Rust, then re-uploading just the position buffer — see
//! `an1mbyt3_renderer::Renderer::update_mesh_positions`) rather than
//! GPU-side skinning (bone matrices as a shader uniform, deformation done in
//! the vertex shader). CPU-side is simpler and needs no new shader; GPU-side
//! is the right optimization once profiling shows CPU skinning cost matters
//! for real documents with many skinned meshes — not needed to prove this
//! works, matching this project's prove-the-pipeline-before-optimizing
//! approach (AN1MBYT3_BUILD_PLAN.md §11).

use glam::{Affine2, Vec2};

use crate::mesh::Mesh;

/// One vertex's bone influence: up to two bones with normalized weights.
/// Two bones (not an arbitrary list) because that's what linear blend
/// skinning across a single joint needs — a vertex only ever sits between
/// the two bones on either side of the one joint nearest it. Extend to an
/// arbitrary influence count only once a real document needs a vertex
/// blended across more than two bones (e.g. a mesh spanning a full multi-
/// joint limb) — not needed for the two-bone-arm case this proves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VertexWeight {
    pub bone_a: usize,
    pub bone_b: usize,
    /// `bone_a`'s weight; `bone_b`'s is implicitly `1.0 - weight_a`. Kept as
    /// a single scalar (not two independent weights) so blending can never
    /// fail to sum to 1 — a common skinning bug when weights are stored and
    /// edited independently.
    pub weight_a: f32,
}

/// A mesh's binding to a set of bones: one [`VertexWeight`] per vertex
/// (same order/length as the [`Mesh`] it was computed for), plus each bone's
/// inverse bind-pose (rest-pose) world matrix.
///
/// The inverse bind matrix is what lets a bone's *current* world transform
/// correctly re-carry a vertex that was authored in the mesh's *rest* pose:
/// `inverse_bind_matrices[bone]` maps a rest-pose world-space point into
/// that bone's local space (undoing whatever pose the bone was in when the
/// mesh was authored), and the bone's current world matrix then carries that
/// local point to wherever the bone is *now*.
pub struct SkinBinding {
    pub weights: Vec<VertexWeight>,
    pub inverse_bind_matrices: Vec<Affine2>,
}

/// Deforms `mesh` according to `binding` and each bone's *current* world
/// matrix, returning a new [`Mesh`] with the same `indices` (topology never
/// changes — skinning only moves vertices) and recomputed `positions`.
///
/// Panics if `binding.weights.len() != mesh.positions.len()`, or if a
/// `VertexWeight` references a bone index outside `bone_world_matrices` —
/// both are caller bugs (a `SkinBinding` built for the wrong mesh, or a
/// bone list that doesn't match the binding), not recoverable conditions.
pub fn skin_mesh(mesh: &Mesh, binding: &SkinBinding, bone_world_matrices: &[Affine2]) -> Mesh {
    assert_eq!(
        binding.weights.len(),
        mesh.positions.len(),
        "skin_mesh: binding has a different vertex count than the mesh"
    );

    let positions = mesh
        .positions
        .iter()
        .zip(&binding.weights)
        .map(|(&[x, y], weight)| {
            let rest_position = Vec2::new(x, y);

            let world_a = skin_single_bone(rest_position, weight.bone_a, binding, bone_world_matrices);
            let world_b = skin_single_bone(rest_position, weight.bone_b, binding, bone_world_matrices);

            let blended = world_a * weight.weight_a + world_b * (1.0 - weight.weight_a);
            [blended.x, blended.y]
        })
        .collect();

    Mesh { positions, indices: mesh.indices.clone(), uvs: mesh.uvs.clone() }
}

fn skin_single_bone(rest_position: Vec2, bone: usize, binding: &SkinBinding, bone_world_matrices: &[Affine2]) -> Vec2 {
    let bind_local = binding.inverse_bind_matrices[bone].transform_point2(rest_position);
    bone_world_matrices[bone].transform_point2(bind_local)
}

#[cfg(test)]
mod tests {
    use super::*;
    use an1mbyt3_core::Transform2D;

    fn identity_bind(translation: Vec2) -> Affine2 {
        Transform2D::from_translation(translation).to_affine2().inverse()
    }

    #[test]
    fn a_vertex_fully_weighted_to_one_bone_follows_only_that_bone() {
        let mesh = Mesh { positions: vec![[5.0, 0.0]], indices: vec![], uvs: Vec::new() };
        let binding = SkinBinding {
            weights: vec![VertexWeight { bone_a: 0, bone_b: 1, weight_a: 1.0 }],
            inverse_bind_matrices: vec![identity_bind(Vec2::ZERO), identity_bind(Vec2::ZERO)],
        };

        // Bone 0 moved by (100, 0); bone 1 moved by (0, 999) — since weight_a
        // is 1.0, only bone 0's motion should show up.
        let bone_0_world = Transform2D::from_translation(Vec2::new(100.0, 0.0)).to_affine2();
        let bone_1_world = Transform2D::from_translation(Vec2::new(0.0, 999.0)).to_affine2();

        let skinned = skin_mesh(&mesh, &binding, &[bone_0_world, bone_1_world]);
        assert_eq!(skinned.positions[0], [105.0, 0.0]);
    }

    #[test]
    fn a_half_weighted_vertex_averages_both_bones() {
        let mesh = Mesh { positions: vec![[0.0, 0.0]], indices: vec![], uvs: Vec::new() };
        let binding = SkinBinding {
            weights: vec![VertexWeight { bone_a: 0, bone_b: 1, weight_a: 0.5 }],
            inverse_bind_matrices: vec![identity_bind(Vec2::ZERO), identity_bind(Vec2::ZERO)],
        };

        let bone_0_world = Transform2D::from_translation(Vec2::new(10.0, 0.0)).to_affine2();
        let bone_1_world = Transform2D::from_translation(Vec2::new(0.0, 10.0)).to_affine2();

        let skinned = skin_mesh(&mesh, &binding, &[bone_0_world, bone_1_world]);
        assert_eq!(skinned.positions[0], [5.0, 5.0]);
    }

    #[test]
    fn skinning_preserves_mesh_topology() {
        let mesh = Mesh { positions: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], indices: vec![0, 1, 2], uvs: Vec::new() };
        let binding = SkinBinding {
            weights: vec![
                VertexWeight { bone_a: 0, bone_b: 0, weight_a: 1.0 },
                VertexWeight { bone_a: 0, bone_b: 0, weight_a: 1.0 },
                VertexWeight { bone_a: 0, bone_b: 0, weight_a: 1.0 },
            ],
            inverse_bind_matrices: vec![identity_bind(Vec2::ZERO)],
        };

        let skinned = skin_mesh(&mesh, &binding, &[Affine2::IDENTITY]);
        assert_eq!(skinned.indices, mesh.indices);
    }
}
