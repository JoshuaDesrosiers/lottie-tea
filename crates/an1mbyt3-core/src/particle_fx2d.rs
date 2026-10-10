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
