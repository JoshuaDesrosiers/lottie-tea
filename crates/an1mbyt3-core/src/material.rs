//! Reusable material — a document-level asset like [`crate::ImageAsset`],
//! referenced by any number of [`crate::SpriteRendererBehavior`]s across any
//! artboard, the Unity-style counterpart to a `Material` asset there: "how a
//! surface looks," swappable independently of which node wears it.

use serde::{Deserialize, Serialize};

use crate::ids::ImageId;
use crate::shader_graph::ShaderGraphDef;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub name: String,
    pub kind: MaterialKind,
    /// Soft-deleted (undo of "New Material"), same convention as
    /// `PrefabDefinition::deleted`.
    #[serde(default)]
    pub deleted: bool,
}

impl Material {
    /// A new node-graph material with the starter graph (the sprite drawn
    /// as-is, tinted).
    pub fn shader_graph(name: impl Into<String>) -> Self {
        Self { name: name.into(), kind: MaterialKind::ShaderGraph(ShaderGraphDef::starter()), deleted: false }
    }
}

/// What a [`Material`] actually draws. 2D only for now — a mesh/shader-based
/// 3D material is a real future variant this enum will grow, not something
/// this pass builds (`SpriteRendererBehavior`'s doc comment has the same
/// note); appended-last-only applies here too if that day comes, same
/// `.teab` index-encoding reason `crate::NodeKind` already documents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MaterialKind {
    /// A flat tint, no image.
    Color([f32; 4]),
    /// An image, multiplied by `tint` (`[1.0; 4]` = no tint, same convention
    /// as `ShapeNode::fill_color`).
    Textured { image: ImageId, tint: [f32; 4] },
    /// A node-based shader ([`crate::shader_graph`]): its compiled WGSL is
    /// the fragment stage for every sprite / shape wearing it.
    ShaderGraph(ShaderGraphDef),
}
