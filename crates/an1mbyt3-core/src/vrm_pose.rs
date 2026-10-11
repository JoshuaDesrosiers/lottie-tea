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

/// **Ragdoll 3D** on a VRM avatar: its humanoid bones become physics
/// capsules joined by ball joints, so it can collapse, tumble and be
/// knocked around (Playground; `ref.ragdoll3(true)` from a script, or
/// `start_ragdolled` to drop it at once).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Ragdoll3D {
    /// Go limp the moment Playground starts.
    pub start_ragdolled: bool,
    /// Total weight (kg), shared out over the limbs.
    pub mass: f32,
    /// How far joints bend: 1 = natural limits, lower = stiffer.
    pub flexibility: f32,
    /// Limb thickness (1 = average build).
    pub thickness: f32,
}

impl Default for Ragdoll3D {
    fn default() -> Self {
        Self { start_ragdolled: false, mass: 60.0, flexibility: 1.0, thickness: 1.0 }
    }
}

/// **Humanoid Animator** on a VRM avatar: animation clips imported from
/// other files (VRMA, or a glTF / VRM with a humanoid skeleton — e.g. a
/// Mixamo character exported to GLB), *retargeted* onto this avatar's
/// bones, so any humanoid motion plays on any VRM. One clip plays at a
/// time (crossfading on switch); scripts switch with `ref.playAnim3`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HumanoidAnimator {
    pub clips: Vec<HumanoidClipRef>,
    /// The clip playing (by name); `""` = none.
    pub playing: String,
    pub looping: bool,
    /// Playback speed (1 = as authored).
    pub speed: f32,
    /// Crossfade seconds when the clip changes.
    pub blend: f32,
    /// Also play in Design (so you see it while you build the scene).
    pub preview: bool,
}

impl Default for HumanoidAnimator {
    fn default() -> Self {
        Self { clips: Vec::new(), playing: String::new(), looping: true, speed: 1.0, blend: 0.25, preview: true }
    }
}

/// One clip an animator can play: its name here, the imported file it
/// comes from (a model asset) and which clip in that file.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HumanoidClipRef {
    pub name: String,
    pub source: Option<crate::ModelId>,
    /// The clip's name in the source file (`""` = its first clip).
    pub clip: String,
}
