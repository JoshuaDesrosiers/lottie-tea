//! [`Tree2D`] — a procedural tree: pick a species and a seed and it grows
//! trunk, branches and foliage (`an1mbyt3_runtime::tree`), sways in the
//! wind, can grow in when play starts, drops leaves, and shakes when a
//! script pokes it (`ref.shakeTree(strength)`).
//!
//! Drawn in the node's local space with the trunk's foot at the node's
//! origin, growing up. Flat toon colours: bark with a shadow strip, foliage
//! clumps in a dark and a light tone, an accent colour for blossoms (and
//! fruit).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TreeSpecies {
    /// Broad, forking, big leafy clumps.
    #[default]
    Oak,
    /// A straight trunk with drooping tiers of needles.
    Pine,
    /// Slender pale trunk, small light leaves.
    Birch,
    /// Crooked branches covered in blossom.
    Sakura,
    /// A curved trunk with a crown of fronds and coconuts.
    Palm,
    /// Arching limbs with long hanging strands.
    Willow,
}

impl TreeSpecies {
    /// The species' natural colours: (bark, foliage, accent).
    pub fn palette(self) -> ([f32; 4], [f32; 4], [f32; 4]) {
        match self {
            TreeSpecies::Oak => ([0.42, 0.28, 0.18, 1.0], [0.36, 0.66, 0.26, 1.0], [0.85, 0.3, 0.25, 1.0]),
            TreeSpecies::Pine => ([0.38, 0.24, 0.16, 1.0], [0.16, 0.45, 0.32, 1.0], [0.55, 0.38, 0.2, 1.0]),
            TreeSpecies::Birch => ([0.92, 0.9, 0.86, 1.0], [0.62, 0.8, 0.3, 1.0], [0.95, 0.85, 0.3, 1.0]),
            TreeSpecies::Sakura => ([0.3, 0.2, 0.2, 1.0], [1.0, 0.72, 0.82, 1.0], [1.0, 0.88, 0.93, 1.0]),
            TreeSpecies::Palm => ([0.62, 0.46, 0.28, 1.0], [0.3, 0.68, 0.3, 1.0], [0.45, 0.3, 0.15, 1.0]),
            TreeSpecies::Willow => ([0.36, 0.28, 0.2, 1.0], [0.5, 0.72, 0.3, 1.0], [0.8, 0.9, 0.5, 1.0]),
        }
    }
}

/// A procedural tree — see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Tree2D {
    pub species: TreeSpecies,
    /// Same seed + settings = the same tree.
    pub seed: u32,
    /// Roughly how tall, world units.
    pub height: f32,
    /// How wide the branches fan out (1 = natural).
    pub spread: f32,
    /// How much foliage (1 = natural, 0 = bare).
    pub density: f32,
    pub bark_color: [f32; 4],
    pub leaf_color: [f32; 4],
    /// Blossom / fruit colour.
    pub accent_color: [f32; 4],
    /// Steady wind: leans the tree and drifts falling leaves (−1..1, + = right).
    pub wind: f32,
    /// How much it sways (0 = still).
    pub wind_sway: f32,
    /// Leaves dropping per second.
    pub falling_leaves: f32,
    /// In play, grows in from a sapling over `grow_time` seconds.
    pub grow_on_play: bool,
    pub grow_time: f32,
}

impl Default for Tree2D {
    fn default() -> Self {
        let (bark, leaf, accent) = TreeSpecies::Oak.palette();
        Self {
            species: TreeSpecies::Oak,
            seed: 7,
            height: 180.0,
            spread: 1.0,
            density: 1.0,
            bark_color: bark,
            leaf_color: leaf,
            accent_color: accent,
            wind: 0.2,
            wind_sway: 1.0,
            falling_leaves: 0.3,
            grow_on_play: false,
            grow_time: 3.0,
        }
    }
}

impl Tree2D {
    /// A tree of `species` with its natural colours.
    pub fn of(species: TreeSpecies, seed: u32, height: f32) -> Self {
        let (bark_color, leaf_color, accent_color) = species.palette();
        Self { species, seed, height, bark_color, leaf_color, accent_color, ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json_with_natural_colours() {
        let tree = Tree2D::of(TreeSpecies::Sakura, 3, 150.0);
        assert_eq!(tree.leaf_color, TreeSpecies::Sakura.palette().1);
        let json = serde_json::to_string(&tree).unwrap();
        assert!(json.contains("\"species\":\"Sakura\""));
        assert_eq!(serde_json::from_str::<Tree2D>(&json).unwrap(), tree);
    }
}
