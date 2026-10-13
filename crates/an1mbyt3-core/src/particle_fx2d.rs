//! Extras for 2D particle effects — the same tools `ParticleSystem3D` has,
//! as components that sit next to a 2D **Particle System** (or a **Liquid
//! 2D**) on the same node:
//!
//! - [`ParticleFluid2D`] draws the particles as one **liquid**: metaballs —
//!   blobs that join with a smooth neck as they near each other and pull
//!   apart like drops of goo (the vector "gooey" effect), with a rim band
//!   and a glossy highlight. The outline is real vector geometry, so it's
//!   crisp at any zoom and sits in the normal draw order.
//! - [`ParticleBehaviors2D`] adds **behaviours** — Converge, Attract,
//!   Repel, Drag — each with a strength curve over particle life and a
//!   point in Local (follows the emitter) or World space. Same types as the
//!   3D component's; the point's `z` is ignored.
//!
//! New components rather than new fields on `ParticleSystemNode`, so files
//! saved before them load unchanged (postcard is positional).

use serde::{Deserialize, Serialize};

use crate::particle3d::ParticleBehavior3D;

/// Liquid rendering for a node's 2D particles — see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ParticleFluid2D {
    /// The liquid's colour (linear RGBA).
    pub color: [f32; 4],
    /// The rim band's colour (alpha 0 = no rim).
    pub edge_color: [f32; 4],
    /// Rim thickness, `0..1` (how much thinner the body is than the rim).
    pub edge_width: f32,
    /// How far each particle's pull reaches, × its drawn size: larger =
    /// stickier — drops join from further apart and stretch longer necks.
    pub stickiness: f32,
    /// Field level that counts as liquid (lower = fatter, more merged).
    pub threshold: f32,
    /// Glossy highlight strength (0 = flat).
    pub gloss: f32,
    /// Outline smoothness: grid cell size as a share of a drop's reach
    /// (smaller = smoother, costlier).
    pub detail: f32,
}

impl Default for ParticleFluid2D {
    fn default() -> Self {
        Self { color: [0.15, 0.45, 1.0, 1.0], edge_color: [0.05, 0.15, 0.45, 1.0], edge_width: 0.25, stickiness: 2.0, threshold: 0.5, gloss: 0.5, detail: 0.2 }
    }
}

/// Behaviours for a node's 2D particles — see the module docs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ParticleBehaviors2D {
    pub behaviors: Vec<ParticleBehavior3D>,
}

/// **Goo 2D** — makes any node part of a liquid: shapes, text, sprites and
/// particle systems. Every node in the same `group` melts into one body:
/// each keeps its exact outline while it's alone, and as two get within
/// `reach` of each other a smooth neck grows between them until they merge
/// (signed-distance metaballs — the "gooey" vector effect, for anything).
///
/// The group is drawn once, at its first member's place in the draw order,
/// with that member's look (colour or gradient fill, rim band colour or
/// gradient, gloss). Gradients are in that first member's local space,
/// like a shape's own gradient. `hide_source` hides each member's normal
/// drawing (the goo replaces it); turn it off to keep, say, a text label
/// drawn on top of the liquid it's part of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Goo2D {
    /// Members with the same number are one liquid.
    pub group: u32,
    /// Body colour, or a gradient (wins when set).
    pub color: [f32; 4],
    pub gradient: Option<crate::gradient::GradientDef>,
    /// Rim band colour / gradient (alpha 0 and no gradient = no rim).
    pub edge_color: [f32; 4],
    pub edge_gradient: Option<crate::gradient::GradientDef>,
    /// Rim thickness, world units.
    pub edge_width: f32,
    /// How far (world units) outside its outline a member starts pulling
    /// on its neighbours — bigger = joins from further, longer necks.
    pub reach: f32,
    /// Glossy highlight strength (0 = flat).
    pub gloss: f32,
    /// Outline detail: grid cell size as a share of `reach` (smaller =
    /// smoother, costlier).
    pub detail: f32,
    /// Hide the member's own drawing (the goo stands in for it).
    pub hide_source: bool,
}

impl Default for Goo2D {
    fn default() -> Self {
        Self {
            group: 0,
            color: [0.2, 0.55, 1.0, 1.0],
            gradient: None,
            edge_color: [0.05, 0.18, 0.5, 1.0],
            edge_gradient: None,
            edge_width: 4.0,
            reach: 28.0,
            gloss: 0.4,
            detail: 0.2,
            hide_source: true,
        }
    }
}

/// **Antialias** — opt-in smooth edges for a node's GPU-drawn liquid (its
/// Goo 2D group, if it's the group's first member, or its Particle Fluid
/// 2D). Without it, liquid edges are **hard**: a pixel is in or out, so
/// low-resolution / pixel-art scenes stay crisp. With it, edges get a
/// `softness`-pixel anti-aliased ramp (1 = standard antialiasing, more =
/// softer). A separate component (not a field) so files saved before it
/// load unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Antialias2D {
    pub softness: f32,
}

impl Default for Antialias2D {
    fn default() -> Self {
        Self { softness: 1.0 }
    }
}

/// **Goo Material** on a Goo 2D group's owner (its first member): makes the
/// liquid look richer than a flat fill.
/// - `material`: a shader-graph material painting the liquid's body (its
///   colour multiplies the goo's own fill / gradient — keep the fill white
///   to see the material as authored). Graphs read the liquid's local
///   position and UV (0..1 across the liquid's box); a graph's texture
///   input isn't available here.
/// - Glow: a soft halo round the liquid, fading out over `glow_spread`
///   (0 = tight, 1 = as far as the goo's reach), at `glow` strength (0 =
///   off), in `glow_color` — neon, magic, lava.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GooMaterial2D {
    pub material: Option<crate::MaterialId>,
    pub glow_color: [f32; 4],
    pub glow: f32,
    pub glow_spread: f32,
}

/// **Anti Goo 2D** — carves into a Goo 2D group instead of joining it.
/// This node (shape, text, sprite, particles — anything Goo 2D accepts)
/// becomes a *negative* member of goo group `group`: where it sits the
/// liquid is pushed away with the same soft, gooey falloff it melts
/// together with, so the goo bends round it and splits apart where it
/// cuts through.
/// - `strength`: how hard it carves (1 = cancels one goo member exactly;
///   raise it to cut through thick, overlapping goo).
/// - `hide_source`: the carving node isn't drawn itself while the goo runs
///   (still shown while designing, so it can be seen and moved).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AntiGoo2D {
    pub group: u32,
    pub strength: f32,
    pub hide_source: bool,
}

impl Default for AntiGoo2D {
    fn default() -> Self {
        Self { group: 0, strength: 1.5, hide_source: true }
    }
}

/// **Goo Gloss 2D** — places and shapes a Goo 2D group's shine (put it on
/// any member of the group; the group's Gloss slider still sets whether
/// there's a shine at all and scales its strength).
/// - `depth` (world px): how far inside the liquid's edge the shine
///   starts — any size, so big blobs can keep it well clear of the edge
///   (only parts thicker than twice this catch a shine; corners round off).
/// - `angle` (degrees, 0 = right, 90 = up) and `distance` (px): which way
///   and how far the shine is nudged.
/// - `opacity`: the shine's strength.
/// - `softness` (0..1): 0 a crisp edge (pixel art), higher a soft fade.
/// - `color` / `blend`: the shine's colour, and how far the liquid's own
///   colour moves toward it (1 = exactly `color`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GooGloss2D {
    pub depth: f32,
    pub angle: f32,
    pub distance: f32,
    pub opacity: f32,
    pub softness: f32,
    pub color: [f32; 4],
    pub blend: f32,
}

impl Default for GooGloss2D {
    fn default() -> Self {
        Self { depth: 24.0, angle: 135.0, distance: 8.0, opacity: 0.4, softness: 0.0, color: [1.0; 4], blend: 0.6 }
    }
}

impl Default for GooMaterial2D {
    fn default() -> Self {
        Self { material: None, glow_color: [0.4, 0.85, 1.0, 1.0], glow: 0.8, glow_spread: 0.5 }
    }
}
