//! **3D two-bone IK** (legs, arms) on a model's local pose — used for foot
//! IK: plant an avatar's feet on the ground whatever its animation or
//! hips offset does.
//!
//! Given a chain `upper → lower → end` (thigh, shin, foot) and a target for
//! `end`, the analytic solve places the middle joint (knee) on the circle
//! of points that keep both bone lengths, on the side of a *pole* point
//! (in front of the knee, so knees bend forward), then turns the upper
//! bone to aim at that knee and the lower bone to aim at the target. Only
//! the two bones' rotations change; lengths never stretch (a target out of
//! reach is approached as far as the leg goes).

use glam::{Mat4, Quat, Vec3};

use crate::model3d::ModelData;

/// A two-bone chain, by node index.
#[derive(Debug, Clone, Copy)]
pub struct TwoBoneChain {
    pub upper: usize,
    pub lower: usize,
    pub end: usize,
}

fn position(m: &Mat4) -> Vec3 {
    m.transform_point3(Vec3::ZERO)
}

fn rotation(m: &Mat4) -> Quat {
    m.to_scale_rotation_translation().1
}

/// Turns bone `bone` by `delta` (model-space rotation, about its own joint)
/// by rewriting its local rotation: `q' = P⁻¹ · delta · P · q`, `P` its
/// parent's model-space rotation.
pub fn rotate_bone(data: &ModelData, world: &[Mat4], local: &mut [(Vec3, Quat, Vec3)], bone: usize, delta: Quat) {
    let parent = data.nodes.get(bone).and_then(|n| n.parent).map_or(Quat::IDENTITY, |p| rotation(&world[p]));
    if let Some(l) = local.get_mut(bone) {
        l.1 = (parent.inverse() * delta * parent * l.1).normalize();
    }
}

/// Solves `chain` so its end reaches `target` (model space), bending toward
/// `pole`. Returns how far the end still is from the target (0 = reached).
pub fn solve_two_bone(data: &ModelData, local: &mut [(Vec3, Quat, Vec3)], chain: TwoBoneChain, target: Vec3, pole: Vec3) -> f32 {
    let world = data.world_pose(local);
    let (a, b, c) = (position(&world[chain.upper]), position(&world[chain.lower]), position(&world[chain.end]));
    let upper_len = a.distance(b);
    let lower_len = b.distance(c);
    if upper_len < 1e-5 || lower_len < 1e-5 {
        return c.distance(target);
    }
    let to_target = target - a;
    let reach = (upper_len + lower_len) * 0.9999;
    let d = to_target.length().clamp((upper_len - lower_len).abs() + 1e-4, reach);
    let dir = to_target.normalize_or(Vec3::NEG_Y);
    // Where the knee goes: `x` along the hip→target line, `h` off it,
    // toward the pole.
    let x = (upper_len * upper_len - lower_len * lower_len + d * d) / (2.0 * d);
    let h = (upper_len * upper_len - x * x).max(0.0).sqrt();
    let to_pole = pole - a;
    let side = (to_pole - dir * to_pole.dot(dir)).normalize_or((b - a - dir * (b - a).dot(dir)).normalize_or(Vec3::Z));
    let knee = a + dir * x + side * h;
    // Upper bone: aim at the new knee.
    let turn_upper = Quat::from_rotation_arc((b - a).normalize_or(dir), (knee - a).normalize_or(dir));
    rotate_bone(data, &world, local, chain.upper, turn_upper);
    // Lower bone: aim at the target (positions after the upper turn).
    let world = data.world_pose(local);
    let (k, e) = (position(&world[chain.lower]), position(&world[chain.end]));
    let goal = a + dir * d;
    let turn_lower = Quat::from_rotation_arc((e - k).normalize_or(dir), (goal - k).normalize_or(dir));
    rotate_bone(data, &world, local, chain.lower, turn_lower);
    let world = data.world_pose(local);
    position(&world[chain.end]).distance(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model3d::ModelNode;

    fn leg() -> ModelData {
        let n = |name: &str, parent: Option<usize>, t: Vec3| ModelNode { name: name.into(), parent, translation: t, rotation: Quat::IDENTITY, scale: Vec3::ONE };
        ModelData {
            nodes: vec![n("hips", None, Vec3::new(0.0, 1.0, 0.0)), n("thigh", Some(0), Vec3::ZERO), n("shin", Some(1), Vec3::new(0.0, -0.45, 0.0)), n("foot", Some(2), Vec3::new(0.0, -0.45, 0.0))],
            ..ModelData::default()
        }
    }

    #[test]
    fn reaches_a_reachable_target_bending_toward_the_pole() {
        let data = leg();
        let mut local = data.rest_pose();
        let chain = TwoBoneChain { upper: 1, lower: 2, end: 3 };
        // Crouch: the foot 25 cm higher than a straight leg puts it.
        let miss = solve_two_bone(&data, &mut local, chain, Vec3::new(0.0, 0.35, 0.05), Vec3::new(0.0, 0.55, 1.0));
        assert!(miss < 1e-3, "missed by {miss}");
        let world = data.world_pose(&local);
        let knee = world[2].transform_point3(Vec3::ZERO);
        assert!(knee.z > 0.1, "knee bends forward toward the pole: {knee}");
        // Lengths kept.
        let hip = world[1].transform_point3(Vec3::ZERO);
        assert!((hip.distance(knee) - 0.45).abs() < 1e-3);
    }

    #[test]
    fn out_of_reach_extends_toward_the_target() {
        let data = leg();
        let mut local = data.rest_pose();
        let miss = solve_two_bone(&data, &mut local, TwoBoneChain { upper: 1, lower: 2, end: 3 }, Vec3::new(0.5, -0.5, 0.0), Vec3::new(0.0, 0.5, 1.0));
        assert!(miss > 0.0);
        let world = data.world_pose(&local);
        let foot = world[3].transform_point3(Vec3::ZERO);
        let toward = (Vec3::new(0.5, -0.5, 0.0) - Vec3::new(0.0, 1.0, 0.0)).normalize();
        assert!((foot - Vec3::new(0.0, 1.0, 0.0)).normalize().dot(toward) > 0.999);
    }
}
