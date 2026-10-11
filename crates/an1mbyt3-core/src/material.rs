//! Reusable material — a document-level asset like [`crate::ImageAsset`],
//! referenced by any number of [`crate::SpriteRendererBehavior`]s across any
//! artboard, the Unity-style counterpart to a `Material` asset there: "how a
//! surface looks," swappable independently of which node wears it.

use serde::{Deserialize, Serialize};

use crate::ids::{ImageId, ModelId};
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
    /// A stylised lit 3D material ([`ToonMaterial`]): VRM's MToon look, or
    /// the painterly "Arcane" look. Every parameter is a plain value
    /// (exposed in the Inspector, sent to the GPU per draw).
    Toon(ToonMaterial),
}

/// Which shader a [`ToonMaterial`] runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ToonStyle {
    /// VRM's MToon: lit/shade colours split at a soft terminator, rim light.
    #[default]
    MToon,
    /// Painterly, hand-painted (Arcane-style): banded light with brushy,
    /// noise-broken edges, cool hatched shadows, warm highlights, canvas
    /// grain and a coloured rim.
    Arcane,
}

/// Which imported model's material slot a [`ToonMaterial`] dresses (its
/// glTF material index) — VRM import makes one material asset per slot so
/// every MToon setting the avatar shipped with is editable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelMaterialSlot {
    pub model: ModelId,
    pub slot: u32,
}

/// A toon / painterly 3D material. Colours are linear RGB.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ToonMaterial {
    pub style: ToonStyle,
    /// Multiplies the texture (the lit colour).
    pub base: [f32; 4],
    /// The shadowed side's colour (multiplied with the texture).
    pub shade: [f32; 3],
    /// Moves the light/shadow line: −1 = all shade … +1 = all lit.
    pub shift: f32,
    /// 0 = soft, smooth shading … 1 = a hard toon edge.
    pub toony: f32,
    /// Rim (edge) light colour; black = none.
    pub rim: [f32; 3],
    /// Rim tightness (higher = thinner rim).
    pub rim_power: f32,
    /// Rim lift: widens the rim toward the whole surface.
    pub rim_lift: f32,
    /// Self-lit colour added on top (glowing eyes, LEDs …).
    pub emission: [f32; 3],
    /// Ink outline width in the mesh's units (0 = none).
    pub outline_width: f32,
    pub outline: [f32; 3],
    /// Alpha cutoff (cut-out hair, lace): pixels below it are discarded;
    /// 0 = off.
    pub alpha_cutoff: f32,
    /// Darkest the shadow gets from lighting alone (0..1).
    pub light_min: f32,
    /// How much point lights add (0 = sun only).
    pub point_lights: f32,
    /// Arcane: light bands (2 = light/shadow, 3 adds a mid-tone …).
    pub bands: f32,
    /// Arcane: brush-stroke size (world metres) and how much it breaks up
    /// the light/shadow edge.
    pub brush_scale: f32,
    pub brush_strength: f32,
    /// Arcane: crosshatch lines in the shadows (0 = none).
    pub hatch: f32,
    /// Arcane: canvas / paper grain (0 = none).
    pub grain: f32,
    /// Arcane: tint added to lit areas (warm highlights).
    pub highlight: [f32; 3],
    /// Draw both faces (cloth, hair cards).
    pub double_sided: bool,
    /// The model slot this material came from (VRM import) — `None` for a
    /// free-standing material.
    pub source: Option<ModelMaterialSlot>,
    /// Replaces the mesh's own texture (`None` = keep the model's texture).
    pub texture: Option<ImageId>,
}

impl Default for ToonMaterial {
    fn default() -> Self {
        Self {
            style: ToonStyle::MToon,
            base: [1.0; 4],
            shade: [0.55, 0.5, 0.6],
            shift: 0.0,
            toony: 0.9,
            rim: [0.0; 3],
            rim_power: 5.0,
            rim_lift: 0.0,
            emission: [0.0; 3],
            outline_width: 0.0,
            outline: [0.05, 0.04, 0.06],
            alpha_cutoff: 0.0,
            light_min: 0.35,
            point_lights: 1.0,
            bands: 3.0,
            brush_scale: 0.22,
            brush_strength: 0.4,
            hatch: 0.25,
            grain: 0.2,
            highlight: [0.0; 3],
            double_sided: false,
            source: None,
            texture: None,
        }
    }
}

impl ToonMaterial {
    /// The painterly Arcane look: cool violet shadows, warm highlights, a
    /// teal rim, brushy edges, hatching and grain, dark ink outline.
    pub fn arcane() -> Self {
        Self {
            style: ToonStyle::Arcane,
            shade: [0.38, 0.36, 0.62],
            shift: 0.05,
            toony: 0.85,
            rim: [0.25, 0.75, 0.8],
            rim_power: 3.5,
            rim_lift: 0.05,
            outline_width: 0.012,
            outline: [0.06, 0.04, 0.07],
            light_min: 0.3,
            bands: 3.0,
            brush_scale: 0.22,
            brush_strength: 0.4,
            hatch: 0.25,
            grain: 0.2,
            highlight: [0.18, 0.1, 0.02],
            ..Self::default()
        }
    }

    /// The per-draw parameter block (`uniforms.material[0..6]` in the toon
    /// shaders) — keep in step with `TOON_PARAMS` in the WGSL.
    pub fn gpu_params(&self) -> [[f32; 4]; 6] {
        let [sr, sg, sb] = self.shade;
        let [rr, rg, rb] = self.rim;
        let [er, eg, eb] = self.emission;
        let [hr, hg, hb] = self.highlight;
        [
            [sr, sg, sb, self.shift],
            [rr, rg, rb, self.toony],
            [er, eg, eb, self.rim_power],
            [hr, hg, hb, self.rim_lift],
            [self.brush_scale, self.brush_strength, self.hatch, self.grain],
            [self.alpha_cutoff, self.point_lights, self.bands, self.light_min],
        ]
    }
}
