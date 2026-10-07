//! Posing bone chains on a live [`Artboard`]: the shared solver behind the
//! editor's bone drag tools (FK aim, IK tip drag) and the runtime
//! `IkConstraint2D` component.
//!
//! A *chain* is a run of `Bone` nodes, each the direct child of the one
//! before (root first, tip last). The solver works in world space on the
//! chain's joint positions, then converts the result back into the
//! parent-relative rotations a bone's `Transform2D` stores:
//! `local_i = world_angle_i - world_angle_{i-1}` along the chain, and
//! `local_0 = world_angle_0 - world_rotation(parent of the root)`.
//!
//! - 1 bone: aim straight at the target.
//! - 2 bones: analytic two-bone (law of cosines), keeping whichever way the
//!   chain currently bends (or a forced side), so a knee never flips.
//! - 3+ bones: FABRIK, seeded from the current pose so motion is smooth.

use an1mbyt3_core::{Artboard, NodeId, NodeKind};
use glam::Vec2;

/// The chain of up to `max_bones` bones ending at `tip` (root first). Walks
/// up through parents while they are bones; stops early at a non-bone.
pub fn bone_chain(artboard: &Artboard, tip: NodeId, max_bones: usize) -> Vec<NodeId> {
    let mut chain = Vec::new();
    let mut cursor = Some(tip);
    while let Some(id) = cursor {
        if chain.len() >= max_bones.max(1) {
            break;
        }
        let Some(node) = artboard.node(id) else { break };
        if !matches!(node.kind, NodeKind::Bone(_)) {
            break;
        }
        chain.push(id);
        cursor = node.parent;
    }
    chain.reverse();
    chain
}

fn bone_length(artboard: &Artboard, id: NodeId) -> f32 {
    match artboard.node(id).map(|n| &n.kind) {
        Some(NodeKind::Bone(bone)) => bone.length,
        _ => 0.0,
    }
}

/// World position of `id`'s local point `local`.
fn world_point(artboard: &Artboard, id: NodeId, local: Vec2) -> Vec2 {
    artboard.world_transform(id).to_affine2().transform_point2(local)
}

/// World rotation of the parent of `id` (0 for a root-level node).
fn parent_world_rotation(artboard: &Artboard, id: NodeId) -> f32 {
    artboard.node(id).and_then(|n| n.parent).map_or(0.0, |p| artboard.world_transform(p).rotation)
}

/// Which way a two-bone chain bends: the sign of the cross product of its
/// two segments (`> 0` = counter-clockwise elbow).
fn bend_sign(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    let s = (b - a).perp_dot(c - b);
    if s.abs() < 1e-4 { 1.0 } else { s.signum() }
}

/// Solves `chain` (root first) so its tip reaches toward `target` (world
/// space). Returns each bone's new *local* rotation (radians). `bend`:
/// force a two-bone chain's bend side (`Some(true)` = counter-clockwise),
/// or `None` to keep the current one. `iterations` only matters for 3+.
pub fn solve_chain(artboard: &Artboard, chain: &[NodeId], target: Vec2, bend: Option<bool>, iterations: usize) -> Vec<(NodeId, f32)> {
    if chain.is_empty() {
        return Vec::new();
    }
    // Current joints (n + 1 points: every bone's origin, then the tip end).
    let mut points: Vec<Vec2> = chain.iter().map(|&id| world_point(artboard, id, Vec2::ZERO)).collect();
    let last = *chain.last().unwrap();
    points.push(world_point(artboard, last, Vec2::new(bone_length(artboard, last), 0.0)));
    let lengths: Vec<f32> = points.windows(2).map(|w| (w[1] - w[0]).length().max(1e-4)).collect();

    let angles: Vec<f32> = match chain.len() {
        1 => {
            let d = target - points[0];
            vec![d.y.atan2(d.x)]
        }
        2 => {
            let (l1, l2) = (lengths[0], lengths[1]);
            let to = target - points[0];
            let dist = to.length().clamp((l1 - l2).abs() + 1e-3, l1 + l2 - 1e-3);
            let base = to.y.atan2(to.x);
            let cos_a = ((l1 * l1 + dist * dist - l2 * l2) / (2.0 * l1 * dist)).clamp(-1.0, 1.0);
            let side = match bend {
                Some(ccw) => if ccw { 1.0 } else { -1.0 },
                None => bend_sign(points[0], points[1], points[2]),
            };
            // A counter-clockwise elbow means the upper bone swings
            // clockwise of the straight line to the target.
            let upper = base - side * cos_a.acos();
            let elbow = points[0] + Vec2::new(upper.cos(), upper.sin()) * l1;
            let tip_dir = points[0] + to.normalize_or_zero() * dist - elbow;
            vec![upper, tip_dir.y.atan2(tip_dir.x)]
        }
        _ => {
            // FABRIK: alternate backward (from the target) and forward
            // (from the fixed root) passes until the tip settles.
            let root = points[0];
            let total: f32 = lengths.iter().sum();
            if (target - root).length() >= total {
                // Out of reach: lay the chain straight toward the target.
                let dir = (target - root).normalize_or_zero();
                let mut p = root;
                for (i, l) in lengths.iter().enumerate() {
                    p += dir * *l;
                    points[i + 1] = p;
                }
            } else {
                let n = points.len();
                for _ in 0..iterations.max(1) {
                    points[n - 1] = target;
                    for i in (0..n - 1).rev() {
                        let dir = (points[i] - points[i + 1]).normalize_or_zero();
                        points[i] = points[i + 1] + dir * lengths[i];
                    }
                    points[0] = root;
                    for i in 0..n - 1 {
                        let dir = (points[i + 1] - points[i]).normalize_or_zero();
                        points[i + 1] = points[i] + dir * lengths[i];
                    }
                    if (points[n - 1] - target).length() < 0.05 {
                        break;
                    }
                }
            }
            points.windows(2).map(|w| (w[1].y - w[0].y).atan2(w[1].x - w[0].x)).collect()
        }
    };

    // World angles -> parent-relative rotations along the chain.
    let mut out = Vec::with_capacity(chain.len());
    let mut parent_angle = parent_world_rotation(artboard, chain[0]);
    for (i, &id) in chain.iter().enumerate() {
        out.push((id, wrap(angles[i] - parent_angle)));
        parent_angle = angles[i];
    }
    out
}

/// Rotation (local) that points `bone`'s +x axis at `target`.
pub fn aim_bone(artboard: &Artboard, bone: NodeId, target: Vec2) -> f32 {
    let origin = world_point(artboard, bone, Vec2::ZERO);
    let d = target - origin;
    wrap(d.y.atan2(d.x) - parent_world_rotation(artboard, bone))
}

/// Writes solved rotations into the artboard.
pub fn apply_pose(artboard: &mut Artboard, pose: &[(NodeId, f32)]) {
    for &(id, rotation) in pose {
        if let Some(node) = artboard.node_mut(id) {
            node.transform.rotation = rotation;
        }
    }
}

/// Solves every enabled `IkConstraint2D` in the artboard (in node order):
/// each bends the chain ending at its bone toward its target node, blended
/// by `weight`. Call after animation, before physics.
pub fn solve_ik_constraints(artboard: &mut Artboard) {
    let constraints: Vec<(NodeId, an1mbyt3_core::IkConstraint2D)> = artboard
        .node_ids()
        .filter(|&id| !artboard.is_effectively_deleted(id))
        .filter_map(|id| artboard.node(id).and_then(|n| n.ik_constraint()).map(|c| (id, *c)))
        .collect();
    for (tip, c) in constraints {
        let Some(target) = c.target.filter(|t| artboard.node(*t).is_some() && !artboard.is_effectively_deleted(*t)) else { continue };
        let goal = artboard.world_transform(target).to_affine2().transform_point2(Vec2::ZERO);
        let chain = bone_chain(artboard, tip, c.chain_length.max(1) as usize);
        // A ragdolled (Dynamic) chain belongs to physics, not to IK.
        let simulated = chain
            .iter()
            .any(|id| artboard.node(*id).and_then(|n| n.rigidbody()).is_some_and(|rb| rb.body_type == an1mbyt3_core::RigidbodyType2D::Dynamic));
        if simulated {
            continue;
        }
        let solved = solve_chain(artboard, &chain, goal, Some(!c.flip), 12);
        let weight = c.weight.clamp(0.0, 1.0);
        let blended: Vec<(NodeId, f32)> = solved
            .into_iter()
            .map(|(id, rot)| {
                let current = artboard.node(id).map_or(rot, |n| n.transform.rotation);
                (id, current + wrap(rot - current) * weight)
            })
            .collect();
        apply_pose(artboard, &blended);
    }
}

/// Into `(-PI, PI]` so stored rotations don't wind up over many solves.
fn wrap(a: f32) -> f32 {
    let mut a = a % std::f32::consts::TAU;
    if a > std::f32::consts::PI {
        a -= std::f32::consts::TAU;
    } else if a <= -std::f32::consts::PI {
        a += std::f32::consts::TAU;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use an1mbyt3_core::{BoneNode, Node};

    /// A straight chain of `n` bones of length 10 along +x, from the origin.
    fn arm(n: usize) -> (Artboard, Vec<NodeId>) {
        let mut artboard = Artboard::new("test", 100.0, 100.0);
        let mut ids = Vec::new();
        let mut parent = None;
        for i in 0..n {
            let mut node = Node::new(format!("b{i}"), parent, NodeKind::Bone(BoneNode { length: 10.0 }));
            if i > 0 {
                node.transform.translation = Vec2::new(10.0, 0.0);
            }
            let id = artboard.add_node(node);
            ids.push(id);
            parent = Some(id);
        }
        (artboard, ids)
    }

    fn tip(artboard: &Artboard, chain: &[NodeId]) -> Vec2 {
        world_point(artboard, *chain.last().unwrap(), Vec2::new(10.0, 0.0))
    }

    #[test]
    fn chain_walks_up_through_bone_parents() {
        let (artboard, ids) = arm(4);
        assert_eq!(bone_chain(&artboard, ids[3], 2), vec![ids[2], ids[3]]);
        assert_eq!(bone_chain(&artboard, ids[3], 10), ids);
    }

    #[test]
    fn one_two_and_many_bone_chains_reach_a_reachable_target() {
        for n in [1usize, 2, 4] {
            let (mut artboard, ids) = arm(n);
            let target = if n == 1 { Vec2::new(0.0, 10.0) } else { Vec2::new(5.0, (n as f32) * 5.0) };
            let pose = solve_chain(&artboard, &ids, target, None, 20);
            apply_pose(&mut artboard, &pose);
            assert!((tip(&artboard, &ids) - target).length() < 0.2, "n={n}: tip {:?}", tip(&artboard, &ids));
        }
    }

    #[test]
    fn two_bone_keeps_its_bend_side() {
        let (mut artboard, ids) = arm(2);
        // Pre-bend clockwise, then solve: the elbow must stay below the line.
        artboard.node_mut(ids[1]).unwrap().transform.rotation = -0.6;
        let pose = solve_chain(&artboard, &ids, Vec2::new(12.0, 0.0), None, 1);
        apply_pose(&mut artboard, &pose);
        let elbow = world_point(&artboard, ids[1], Vec2::ZERO);
        assert!(elbow.y > 0.0, "clockwise bend keeps the elbow on the + side of the root->target line: {elbow:?}");
    }
}
