//! Unity-style 2D physics components: `Rigidbody 2D` and the `Box`,
//! `Circle`, `Capsule` and `Polygon Collider 2D`s. Plain authored data —
//! simulation lives in `an1mbyt3-physics` (Rapier under the hood).
//!
//! Units are this engine's world units (one art pixel). Like Unity:
//! - a collider on a node *with* a Rigidbody 2D (or under an ancestor that
//!   has one) moves with that body;
//! - a collider with no Rigidbody 2D anywhere above it is static scenery;
//! - `is_trigger` colliders detect overlaps (events) but don't push.
//!
//! Collider `offset`s are in the node's own local space — the same space as
//! its art (an Image is centered on its local origin), so `offset: 0` sits
//! on the art regardless of where the node's pivot/anchor is.

use glam::Vec2;
use serde::{Deserialize, Serialize};

/// Unity's `Body Type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RigidbodyType2D {
    /// Fully simulated: gravity, forces, collisions.
    #[default]
    Dynamic,
    /// Moved only by script (its node's transform); pushes dynamic bodies
    /// but isn't pushed back.
    Kinematic,
    /// Never moves.
    Static,
}

/// Unity's `Collision Detection`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CollisionDetection2D {
    #[default]
    Discrete,
    /// Prevents fast bodies tunneling through thin colliders (costs more).
    Continuous,
}

/// Unity's `Rigidbody 2D` component.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rigidbody2D {
    pub body_type: RigidbodyType2D,
    /// Kilograms-ish; only relative masses matter.
    pub mass: f32,
    /// Unity's `Linear Drag` (velocity damping).
    pub linear_drag: f32,
    /// Unity's `Angular Drag`.
    pub angular_drag: f32,
    /// Multiplies the scene's gravity (`0` = floats; top-down games).
    pub gravity_scale: f32,
    pub collision_detection: CollisionDetection2D,
    /// Unity's `Constraints > Freeze Position X/Y`.
    pub freeze_position_x: bool,
    pub freeze_position_y: bool,
    /// Unity's `Constraints > Freeze Rotation`.
    pub freeze_rotation: bool,
}

impl Default for Rigidbody2D {
    fn default() -> Self {
        Self {
            body_type: RigidbodyType2D::Dynamic,
            mass: 1.0,
            linear_drag: 0.0,
            angular_drag: 0.05,
            gravity_scale: 1.0,
            collision_detection: CollisionDetection2D::Discrete,
            freeze_position_x: false,
            freeze_position_y: false,
            freeze_rotation: false,
        }
    }
}

/// Unity's `Physics Material 2D`, inlined per collider.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicsMaterial2D {
    pub friction: f32,
    /// 0 = no bounce, 1 = perfectly bouncy.
    pub bounciness: f32,
}

impl Default for PhysicsMaterial2D {
    fn default() -> Self {
        Self { friction: 0.4, bounciness: 0.0 }
    }
}

/// Unity's `Box Collider 2D`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxCollider2D {
    pub offset: Vec2,
    pub size: Vec2,
    pub is_trigger: bool,
    #[serde(default)]
    pub material: PhysicsMaterial2D,
}

impl Default for BoxCollider2D {
    fn default() -> Self {
        Self { offset: Vec2::ZERO, size: Vec2::splat(32.0), is_trigger: false, material: PhysicsMaterial2D::default() }
    }
}

/// Unity's `Circle Collider 2D`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CircleCollider2D {
    pub offset: Vec2,
    pub radius: f32,
    pub is_trigger: bool,
    #[serde(default)]
    pub material: PhysicsMaterial2D,
}

impl Default for CircleCollider2D {
    fn default() -> Self {
        Self { offset: Vec2::ZERO, radius: 16.0, is_trigger: false, material: PhysicsMaterial2D::default() }
    }
}

/// Unity's capsule `Direction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CapsuleDirection2D {
    #[default]
    Vertical,
    Horizontal,
}

/// Unity's `Capsule Collider 2D`: a box with fully rounded ends, `size` is
/// its overall width x height.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapsuleCollider2D {
    pub offset: Vec2,
    pub size: Vec2,
    pub direction: CapsuleDirection2D,
    pub is_trigger: bool,
    #[serde(default)]
    pub material: PhysicsMaterial2D,
}

impl Default for CapsuleCollider2D {
    fn default() -> Self {
        Self { offset: Vec2::ZERO, size: Vec2::new(16.0, 32.0), direction: CapsuleDirection2D::Vertical, is_trigger: false, material: PhysicsMaterial2D::default() }
    }
}

/// Unity's `Polygon Collider 2D` (convex: concave outlines are wrapped in
/// their convex hull).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolygonCollider2D {
    pub offset: Vec2,
    pub points: Vec<Vec2>,
    pub is_trigger: bool,
    #[serde(default)]
    pub material: PhysicsMaterial2D,
}

impl Default for PolygonCollider2D {
    fn default() -> Self {
        Self {
            offset: Vec2::ZERO,
            points: vec![Vec2::new(-16.0, -16.0), Vec2::new(16.0, -16.0), Vec2::new(0.0, 16.0)],
            is_trigger: false,
            material: PhysicsMaterial2D::default(),
        }
    }
}

/// Liquid 2D — a particle fluid (water, slime, lava) that pours, splashes,
/// settles, flows around every 2D collider and pushes dynamic bodies; bodies
/// it covers float (buoyancy) and slow down (drag). Simulated in Playground
/// by `an1mbyt3_physics` (double-density relaxation, Clavet et al. 2005);
/// at rest in the editor it shows its starting volume.
///
/// The node's position is the center of the starting volume (`size`). With
/// `emit_rate > 0` it is also a faucet: new particles stream out of the
/// node's origin along `emit_velocity` until `max_particles` exist.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Liquid2D {
    /// Starting volume, px (width x height), filled with particles.
    pub size: glam::Vec2,
    /// Distance between particles at rest, px. Smaller = finer, costlier.
    pub particle_spacing: f32,
    /// Fill color (straight linear RGBA). Foam / thin spots are lighter.
    pub color: [f32; 4],
    /// 0 = water-thin, 1 = honey-thick.
    pub viscosity: f32,
    /// Pressure stiffness: how hard it resists being squashed (0.2..2).
    pub stiffness: f32,
    /// The liquid's density, in kg per 32x32 px of area (the same units a
    /// Rigidbody 2D's mass spreads over its collider). A body lighter per
    /// area than this floats, a denser one sinks — a 24 px crate of mass 1
    /// is about 1.8, so the default 2.5 floats it while a mass-4 stone
    /// sinks. 0 = no buoyancy.
    pub buoyancy: f32,
    /// Slows bodies moving through it (per second, when submerged).
    pub drag: f32,
    /// Faucet: particles per second (0 = only the starting volume).
    pub emit_rate: f32,
    /// Faucet stream velocity, px/s.
    pub emit_velocity: glam::Vec2,
    /// Hard cap on live particles (performance guard).
    pub max_particles: u32,
}

impl Default for Liquid2D {
    fn default() -> Self {
        Self {
            size: glam::Vec2::new(160.0, 80.0),
            particle_spacing: 7.0,
            color: [0.2, 0.55, 1.0, 0.9],
            viscosity: 0.15,
            stiffness: 1.0,
            buoyancy: 2.5,
            drag: 2.0,
            emit_rate: 0.0,
            emit_velocity: glam::Vec2::new(0.0, -120.0),
            max_particles: 900,
        }
    }
}
