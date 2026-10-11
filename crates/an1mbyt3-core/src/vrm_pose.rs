//! **VRM Poses** — named poses ("states") for an avatar, authored in the
//! Inspector: per humanoid bone a rotation (Euler degrees, applied in the
//! bone's own space on top of its rest / animated pose) and an offset
//! (metres, e.g. the hips dropping into a crouch), plus expression weights.
//!
//! The `active` pose shows in Design (so you see what you're posing), and
//! Playground starts in it; scripts switch with `ref.setPose3(name)`, which
//! crossfades over `blend` seconds. Script bone poses (`ref.setBone3`) go
//! on top. A component (not fields on the model mesh) so files without it
//! load unchanged.

use serde::{Deserialize, Serialize};

/// One bone's part of a pose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BonePose {
    /// Humanoid bone name (`leftUpperArm` …) or a node name.
    pub bone: String,
    /// Euler degrees (X, Y, Z; applied Y-X-Z) on top of the bone's pose.
    pub rotation: [f32; 3],
    /// Metres added to the bone's position (its parent's space).
    pub offset: [f32; 3],
}

/// One expression's weight in a pose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ExpressionWeight {
    pub name: String,
    pub weight: f32,
}

/// A named pose (state).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct VrmPose {
    pub name: String,
    pub bones: Vec<BonePose>,
    pub expressions: Vec<ExpressionWeight>,
}

/// The component: an avatar's poses, which one is showing, and how fast
/// switching between them blends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VrmPoses {
    pub poses: Vec<VrmPose>,
    pub active: String,
    /// Crossfade seconds when a script switches pose.
    pub blend: f32,
}

impl Default for VrmPoses {
    fn default() -> Self {
        Self { poses: vec![VrmPose { name: "Rest".into(), ..VrmPose::default() }], active: "Rest".into(), blend: 0.12 }
    }
}

impl VrmPoses {
    /// A pose by name (case-insensitive).
    pub fn pose(&self, name: &str) -> Option<&VrmPose> {
        self.poses.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }
}
