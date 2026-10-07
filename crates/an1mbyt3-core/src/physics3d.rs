//! Unity-style 3D physics components, for [`crate::Node3D`]s — the 3D
//! counterparts of `physics2d.rs`' Rigidbody 2D / colliders, simulated by
//! `an1mbyt3-physics` (Rapier 3D).
//!
//! - **[`Rigidbody3D`]** makes a Node3D a body: Dynamic (gravity, forces,
//!   collisions push it), Kinematic (moved by scripts / animation, pushes
//!   others) or Static.
//! - **[`Collider3D`]** gives it a shape. `Auto` fits the node's own mesh
//!   (a cube's box, a sphere's ball, a plane's slab, a model's bounds).
//!   Colliders without a Rigidbody 3D on themselves or a Node3D ancestor are
//!   static scenery.
//!
//! 3D units are meters (Rapier's native scale), so nothing is converted.

use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RigidbodyType3D {
    #[default]
    Dynamic,
    Kinematic,
    Static,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rigidbody3D {
    pub body_type: RigidbodyType3D,
    /// Kilograms (spread over the body's colliders by volume).
    pub mass: f32,
    /// Multiplies the scene's 3D gravity (0 = floats).
    pub gravity_scale: f32,
    /// Slows linear / angular motion over time (Unity's Drag / Angular Drag).
    pub linear_damping: f32,
    pub angular_damping: f32,
    /// Never rotates from collisions (a character controller's capsule).
    pub freeze_rotation: bool,
    /// Continuous collision detection — fast bullets don't tunnel through
    /// thin walls (costs more).
    pub continuous: bool,
}

impl Default for Rigidbody3D {
    fn default() -> Self {
        Self { body_type: RigidbodyType3D::Dynamic, mass: 1.0, gravity_scale: 1.0, linear_damping: 0.0, angular_damping: 0.05, freeze_rotation: false, continuous: false }
    }
}

/// A [`Collider3D`]'s shape, in the node's own (unscaled) space; the node's
/// world scale is applied on top.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum Collider3DShape {
    /// Fit the node's mesh: Cube → box, Sphere → ball, Plane → thin slab,
    /// Cylinder / Cone → cylinder / cone, Torus / Model → their bounding box.
    #[default]
    Auto,
    Box { size: Vec3 },
    Sphere { radius: f32 },
    /// Upright (along local Y); `height` includes the rounded ends.
    Capsule { radius: f32, height: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collider3D {
    pub shape: Collider3DShape,
    /// The shape's centre, in the node's local space.
    pub offset: Vec3,
    /// Detects overlaps (events) without blocking anything.
    pub is_trigger: bool,
    pub friction: f32,
    /// Bounciness, 0..1.
    pub restitution: f32,
}

impl Default for Collider3D {
    fn default() -> Self {
        Self { shape: Collider3DShape::Auto, offset: Vec3::ZERO, is_trigger: false, friction: 0.5, restitution: 0.0 }
    }
}

/// Earth gravity, m/s².
pub fn default_gravity_3d() -> Vec3 {
    Vec3::new(0.0, -9.81, 0.0)
}
