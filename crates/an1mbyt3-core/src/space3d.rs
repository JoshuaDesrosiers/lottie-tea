//! 3D scene nodes — the start of an1mbyt3's 3D support.
//!
//! A [`Node3D`] is the Godot `Node3D` counterpart: a node with its own
//! *3D* transform ([`Transform3D`]: position, Euler rotation, scale) and one
//! piece of content — nothing (a pivot), a primitive [`Mesh3D`], a
//! [`Camera3D`] or a [`Light3D`]. Node3Ds live in the same scene tree as
//! every 2D node; a Node3D's transform composes only through *Node3D*
//! ancestors ([`Artboard::world_matrix_3d`](crate::Artboard::world_matrix_3d)),
//! so a 3D sub-tree can sit anywhere in the hierarchy.
//!
//! **Rendering** (`an1mbyt3-renderer`'s 3D pass): if the scene has a
//! Camera3D, every mesh is drawn depth-tested through it *before* the 2D
//! layer — so 2D shapes, text and sprites draw on top, like a HUD over the
//! 3D world. Coordinates are right-handed, +Y up, the camera looks down its
//! local −Z (glTF / Godot convention); one world unit is one "meter" (unlike
//! 2D's pixels).

use glam::{EulerRot, Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

/// Position / rotation / scale in 3D. Rotation is Euler **degrees**, applied
/// Y (yaw), then X (pitch), then Z (roll) — Godot's default order, and the
/// one that keeps "turn left/right, then look up/down" independent.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transform3D {
    pub position: Vec3,
    pub rotation_degrees: Vec3,
    pub scale: Vec3,
}

impl Default for Transform3D {
    fn default() -> Self {
        Self { position: Vec3::ZERO, rotation_degrees: Vec3::ZERO, scale: Vec3::ONE }
    }
}

impl Transform3D {
    pub fn at(position: Vec3) -> Self {
        Self { position, ..Self::default() }
    }

    pub fn rotation(&self) -> Quat {
        let r = self.rotation_degrees * (std::f32::consts::PI / 180.0);
        Quat::from_euler(EulerRot::YXZ, r.y, r.x, r.z)
    }

    /// The local-to-parent matrix.
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation(), self.position)
    }
}

/// A built-in mesh shape. Sizes are in world units; every primitive is
/// centred on its origin. Appended-last-only (`.teab`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape")]
pub enum Primitive3D {
    Cube { size: Vec3 },
    Sphere { radius: f32, segments: u32, rings: u32 },
    /// Flat on XZ, facing +Y.
    Plane { size: f32, subdivisions: u32 },
    Cylinder { radius: f32, height: f32, segments: u32 },
    Cone { radius: f32, height: f32, segments: u32 },
    Torus { radius: f32, tube: f32, segments: u32, sides: u32 },
    /// An imported glTF / GLB model (see [`crate::ModelAsset`]), drawn with
    /// its own meshes, textures and (optionally) one of its animations.
    Model { model: crate::ModelId },
}

impl Primitive3D {
    pub fn cube() -> Self {
        Self::Cube { size: Vec3::ONE }
    }
    pub fn sphere() -> Self {
        Self::Sphere { radius: 0.5, segments: 32, rings: 16 }
    }
    pub fn plane() -> Self {
        Self::Plane { size: 10.0, subdivisions: 1 }
    }
    pub fn cylinder() -> Self {
        Self::Cylinder { radius: 0.5, height: 1.0, segments: 32 }
    }
    pub fn cone() -> Self {
        Self::Cone { radius: 0.5, height: 1.0, segments: 32 }
    }
    pub fn torus() -> Self {
        Self::Torus { radius: 0.5, tube: 0.18, segments: 40, sides: 16 }
    }
}

/// A mesh with a simple lit surface.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mesh3D {
    pub primitive: Primitive3D,
    /// Base colour (straight linear RGBA, like every colour in the document).
    pub color: [f32; 4],
    /// 0 = matte, 1 = sharp highlight.
    #[serde(default = "default_shininess")]
    pub shininess: f32,
    /// Ignores lights (drawn at full `color`) — glowing objects, skyboxes.
    #[serde(default)]
    pub unlit: bool,
    /// A shader-graph material for the surface (its output is the colour
    /// that then gets lit; `color` is its Tint). In 3D the graph's *Local
    /// Position* is world-space meters on the surface's plane, so patterns
    /// tile at world scale; *UV* is the mesh's own 0..1.
    #[serde(default)]
    pub material: Option<crate::MaterialId>,
    /// `Model` meshes: the clip playing (by name, e.g. `"walk"`), looped
    /// unless `animation_loop` is off (then it holds its last frame).
    #[serde(default)]
    pub animation: Option<AnimationName>,
    #[serde(default = "default_true")]
    pub animation_loop: bool,
    /// A second clip layered on top (its channels override the base clip's
    /// — e.g. `"walk"` + `"holding-right-shoot"` = walking with the gun up).
    #[serde(default)]
    pub animation_overlay: Option<AnimationName>,
}

/// An animation clip's name, inline (≤ 23 bytes) so `Mesh3D` stays `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AnimationName {
    bytes: [u8; 23],
    len: u8,
}

impl AnimationName {
    pub fn new(name: &str) -> Self {
        let mut bytes = [0u8; 23];
        let mut len = 0;
        for ch in name.chars() {
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            if len + s.len() > 23 {
                break;
            }
            bytes[len..len + s.len()].copy_from_slice(s.as_bytes());
            len += s.len();
        }
        Self { bytes, len: len as u8 }
    }
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

impl Serialize for AnimationName {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AnimationName {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(Self::new(&s))
    }
}

fn default_true() -> bool {
    true
}

fn default_shininess() -> f32 {
    0.3
}

impl Mesh3D {
    pub fn new(primitive: Primitive3D, color: [f32; 4]) -> Self {
        Self { primitive, color, shininess: default_shininess(), unlit: false, material: None, animation: None, animation_loop: true, animation_overlay: None }
    }
}

/// A perspective camera, looking down its local −Z.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Camera3D {
    /// Vertical field of view, degrees.
    pub fov_degrees: f32,
    pub near: f32,
    pub far: f32,
    /// The camera the scene renders through (the first `current` one; if
    /// none is, the first camera).
    pub current: bool,
    /// Background colour behind the 3D world; `None` keeps the 2D scene's.
    #[serde(default)]
    pub clear_color: Option<[f32; 4]>,
}

impl Default for Camera3D {
    fn default() -> Self {
        Self { fov_degrees: 60.0, near: 0.05, far: 500.0, current: true, clear_color: Some([0.45, 0.62, 0.85, 1.0]) }
    }
}

/// What kind of light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Light3DKind {
    /// Parallel rays along the node's local −Z (the sun).
    Directional,
    /// Radiates from the node's position, fading out by `range`.
    Point,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Light3D {
    pub kind: Light3DKind,
    pub color: [f32; 3],
    pub intensity: f32,
    /// Point lights: distance at which the light reaches zero.
    pub range: f32,
}

impl Default for Light3D {
    fn default() -> Self {
        Self { kind: Light3DKind::Directional, color: [1.0, 0.97, 0.9], intensity: 1.0, range: 10.0 }
    }
}

/// What a [`Node3D`] holds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Node3DContent {
    /// A pivot / group for other Node3Ds.
    Empty,
    Mesh(Mesh3D),
    Camera(Camera3D),
    Light(Light3D),
}

/// Payload for `NodeKind::Node3D`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node3D {
    pub transform: Transform3D,
    pub content: Node3DContent,
}

impl Node3D {
    pub fn new(transform: Transform3D, content: Node3DContent) -> Self {
        Self { transform, content }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaw_then_pitch_rotation_order() {
        // Yaw 90° turns -Z (forward) to -X; pitch is then about the new right axis.
        let t = Transform3D { rotation_degrees: Vec3::new(0.0, 90.0, 0.0), ..Default::default() };
        let forward = t.matrix().transform_vector3(Vec3::NEG_Z);
        assert!((forward - Vec3::NEG_X).length() < 1e-5, "{forward}");
        let t = Transform3D { rotation_degrees: Vec3::new(-30.0, 90.0, 0.0), ..Default::default() };
        let forward = t.matrix().transform_vector3(Vec3::NEG_Z);
        assert!(forward.y < -0.4 && forward.x < -0.8, "looks down while facing -X: {forward}");
    }
}
