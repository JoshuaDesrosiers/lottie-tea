//! 2D rigging components: joints for ragdolls, live IK, and sprite skinning.
//! Plain authored data, like `physics2d`. Joints are simulated by
//! `an1mbyt3-physics` (Rapier); IK is solved every frame by
//! `an1mbyt3-constraints`; skinning is evaluated by the editor/player
//! renderer (`an1mbyt3-geometry::skin`).

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::NodeId;

/// Unity's `Hinge Joint 2D`: pins this node's Rigidbody 2D to another body
/// at `anchor`, free to rotate about it (elbows, knees, swinging doors,
/// chains). The node needs a Rigidbody 2D (or a collider under one).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HingeJoint2D {
    /// The other body. `None` = the nearest *ancestor* with a Rigidbody 2D
    /// (a ragdoll limb hinges to its parent bone), or the world itself when
    /// there is none (a pendulum pinned in place).
    #[serde(default)]
    pub connected_body: Option<NodeId>,
    /// The hinge point, in this node's own local space (a bone's joint is
    /// its origin, `0,0`).
    #[serde(default)]
    pub anchor: Vec2,
    /// Limit the swing to `lower_angle..=upper_angle` degrees, measured
    /// from the pose the joint was created in.
    pub use_limits: bool,
    pub lower_angle: f32,
    pub upper_angle: f32,
    /// Drive the hinge toward `motor_speed` (degrees/second) with at most
    /// `max_motor_torque`.
    #[serde(default)]
    pub use_motor: bool,
    #[serde(default)]
    pub motor_speed: f32,
    #[serde(default = "default_motor_torque")]
    pub max_motor_torque: f32,
    /// Whether the two connected bodies collide with each other.
    #[serde(default)]
    pub enable_collision: bool,
}

fn default_motor_torque() -> f32 {
    100.0
}

impl Default for HingeJoint2D {
    fn default() -> Self {
        Self {
            connected_body: None,
            anchor: Vec2::ZERO,
            use_limits: true,
            lower_angle: -60.0,
            upper_angle: 60.0,
            use_motor: false,
            motor_speed: 0.0,
            max_motor_torque: default_motor_torque(),
            enable_collision: false,
        }
    }
}

/// Unity 2D IK's `Limb Solver` / `Chain (FABRIK) Solver`, as a component on
/// the chain's *tip* bone: every frame (after animation, before physics)
/// the chain of `chain_length` bones ending here is bent so the tip reaches
/// toward `target`. Two bones solve analytically (feet, arms), longer
/// chains with FABRIK (tails, tentacles).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IkConstraint2D {
    /// The node to reach for (move it, animate it, or drive it from a
    /// script). `None` = inactive.
    pub target: Option<NodeId>,
    /// How many bones (tip included) the solver may bend.
    pub chain_length: u32,
    /// Two-bone chains: bend the other way.
    #[serde(default)]
    pub flip: bool,
    /// 0 = ignore the IK (pure animation), 1 = fully solved.
    #[serde(default = "one")]
    pub weight: f32,
}

fn one() -> f32 {
    1.0
}

impl Default for IkConstraint2D {
    fn default() -> Self {
        Self { target: None, chain_length: 2, flip: false, weight: 1.0 }
    }
}

/// Unity's `Sprite Skin`: deforms this node's image with its bones instead
/// of moving it rigidly — limbs bend smoothly, tails curl. The image is cut
/// into a `columns` x `rows` grid; each grid vertex follows the bones near
/// it (weights computed from distance to each bone segment at bind time).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpriteSkin2D {
    pub bones: Vec<NodeId>,
    /// Each bone's world matrix when bound, `[m00, m10, m01, m11, tx, ty]`
    /// (glam `Affine2` column order), same order as `bones`.
    pub bind_poses: Vec<[f32; 6]>,
    /// This node's own world matrix when bound.
    pub bind_node: [f32; 6],
    pub columns: u32,
    pub rows: u32,
    /// How softly influence blends between neighboring bones (world px).
    #[serde(default = "default_smoothness")]
    pub smoothness: f32,
}

fn default_smoothness() -> f32 {
    12.0
}

impl Default for SpriteSkin2D {
    fn default() -> Self {
        Self { bones: Vec::new(), bind_poses: Vec::new(), bind_node: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], columns: 8, rows: 8, smoothness: default_smoothness() }
    }
}

/// `glam::Affine2` <-> the `[f32; 6]` stored above.
pub fn affine_to_array(a: glam::Affine2) -> [f32; 6] {
    let m = a.matrix2;
    [m.x_axis.x, m.x_axis.y, m.y_axis.x, m.y_axis.y, a.translation.x, a.translation.y]
}

pub fn array_to_affine(a: [f32; 6]) -> glam::Affine2 {
    glam::Affine2::from_cols(Vec2::new(a[0], a[1]), Vec2::new(a[2], a[3]), Vec2::new(a[4], a[5]))
}
