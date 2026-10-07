//! [`NodeKind::Tilemap`] — a Unity-`Tilemap`-style grid of tiles, all drawn
//! from one shared tileset image. Each cell holds either `None` (empty, drawn
//! as nothing) or `Some(index)` into the tileset's own flat left-to-right,
//! top-to-bottom tile list (`index = row * tileset_columns() + col`).
//!
//! Deliberately simple relative to Unity's own Tilemap (no per-tile
//! collision shape, no multiple sorted layers, no tile-rotation/flip,  no
//! "rule tiles" auto-matching neighbors) — one flat grid of plain tile
//! indices is what an authored top-down area actually needs, and is the
//! "core" slice this pass covers (same disclosed-scope-limit convention
//! `an1mbyt3_core::particle`'s own module doc already uses for
//! `ParticleSystemNode`). Upgradeable later, not designed away.
//!
//! Rendering is the real reason this exists as its own node kind rather than
//! "many `Image` nodes snapped to a grid": one `Tilemap` draws as a single
//! batched mesh (one quad per non-empty cell, all in one `GpuMesh`), each
//! quad's own per-vertex UV pointing at its own tile's sub-rect of the
//! shared tileset texture — the engine's first use of real per-vertex UV
//! (every other textured draw still derives UV from a mesh's whole local
//! bounds; see `an1mbyt3_renderer::shader.wgsl`'s `compute_textured_color`).
//! A naive "one draw call per tile" design would mean hundreds of draw calls
//! for one modestly-sized area — the actual performance reason this is one
//! mesh, not N image nodes.

use serde::{Deserialize, Serialize};

use crate::ImageId;

/// Payload for `NodeKind::Tilemap`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TilemapNode {
    /// Which `ImageAsset` every tile is cropped from.
    pub tileset: ImageId,
    /// The tileset image's own full pixel size — needed to convert a tile's
    /// pixel-space sub-rect into `0..1` UV fractions at mesh-build time.
    /// Duplicated from the `ImageAsset`'s own (lazily-read) dimensions
    /// rather than looked up at render time, the same "don't make the hot
    /// path depend on a side lookup" reasoning `GpuImageTexture` not storing
    /// its own dimensions already established (see this type's own module
    /// doc comment / the engine-wide research that found this gap) — an
    /// author changing the tileset image without updating these would
    /// produce a visibly wrong (stretched/misaligned) tilemap, not a panic.
    pub tileset_width_px: u32,
    pub tileset_height_px: u32,
    /// One tile's pixel size within the tileset (every tile is the same
    /// size — no variable-size tiles this pass).
    pub tile_width_px: u32,
    pub tile_height_px: u32,
    /// The authored grid's size, in cells.
    pub map_width: u32,
    pub map_height: u32,
    /// One cell's on-canvas size, in this workspace's own world units —
    /// independent of `tile_width_px`/`tile_height_px` (the source pixel
    /// size), the same "authored display size vs. source pixel size" split
    /// `ImageNode::width`/`height` already draws against `ImageAsset`.
    pub cell_size: f32,
    /// Row-major (`tiles[y * map_width + x]`), `map_width * map_height`
    /// entries — `None` is an empty cell (nothing drawn), `Some(index)` is
    /// this tile's position in the tileset's own flat left-to-right,
    /// top-to-bottom tile list (`index = row * tileset_columns() + col`,
    /// see `Self::tileset_columns`).
    pub tiles: Vec<Option<u32>>,
}

impl TilemapNode {
    /// How many whole tiles fit across the tileset's own width — the divisor
    /// `Self::tiles`' flat index is interpreted against.
    pub fn tileset_columns(&self) -> u32 {
        self.tileset_width_px / self.tile_width_px.max(1)
    }

    /// `None` if `(x, y)` is outside the authored grid — a caller's bug
    /// (the editor's own paint tool clamps to the grid before ever calling
    /// this), not a recoverable runtime condition worth a `Result` over.
    pub fn get(&self, x: u32, y: u32) -> Option<u32> {
        if x >= self.map_width || y >= self.map_height {
            return None;
        }
        self.tiles[(y * self.map_width + x) as usize]
    }

    /// Does nothing if `(x, y)` is outside the authored grid — same
    /// "caller's job to clamp first" convention as `Self::get`.
    pub fn set(&mut self, x: u32, y: u32, tile: Option<u32>) {
        if x >= self.map_width || y >= self.map_height {
            return;
        }
        let i = (y * self.map_width + x) as usize;
        self.tiles[i] = tile;
    }

    /// Resizes the grid in place to `new_width x new_height` — every cell
    /// that exists in both the old and new grid keeps its own tile;
    /// shrinking drops whatever falls outside the new bounds, growing fills
    /// the new area with `None`. The Inspector's "Map Width"/"Map Height"
    /// fields (Unity's own Tilemap-size-change behavior: resizing never
    /// discards tiles that still fit).
    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        let mut tiles = vec![None; (new_width * new_height) as usize];
        for y in 0..self.map_height.min(new_height) {
            for x in 0..self.map_width.min(new_width) {
                tiles[(y * new_width + x) as usize] = self.get(x, y);
            }
        }
        self.tiles = tiles;
        self.map_width = new_width;
        self.map_height = new_height;
    }

    /// A freshly-authored, entirely empty grid of the given size — the
    /// Shapes-menu/editor starting point before any tile is painted.
    pub fn empty(tileset: ImageId, tileset_width_px: u32, tileset_height_px: u32, tile_width_px: u32, tile_height_px: u32, map_width: u32, map_height: u32, cell_size: f32) -> Self {
        Self {
            tileset,
            tileset_width_px,
            tileset_height_px,
            tile_width_px,
            tile_height_px,
            map_width,
            map_height,
            cell_size,
            tiles: vec![None; (map_width * map_height) as usize],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny() -> TilemapNode {
        TilemapNode::empty(ImageId::default(), 64, 32, 16, 16, 3, 2, 32.0)
    }

    #[test]
    fn tileset_columns_divides_pixel_width_by_tile_width() {
        assert_eq!(tiny().tileset_columns(), 4);
    }

    #[test]
    fn get_set_round_trips_within_bounds() {
        let mut map = tiny();
        assert_eq!(map.get(1, 1), None);
        map.set(1, 1, Some(5));
        assert_eq!(map.get(1, 1), Some(5));
    }

    #[test]
    fn get_set_outside_bounds_is_a_harmless_no_op() {
        let mut map = tiny();
        assert_eq!(map.get(99, 99), None);
        map.set(99, 99, Some(5)); // must not panic
        assert_eq!(map.tiles.iter().filter(|t| t.is_some()).count(), 0);
    }

    #[test]
    fn empty_grid_has_the_right_cell_count_and_is_all_none() {
        let map = tiny();
        assert_eq!(map.tiles.len(), 6);
        assert!(map.tiles.iter().all(|t| t.is_none()));
    }

    #[test]
    fn resize_preserves_overlapping_tiles_when_growing() {
        let mut map = tiny(); // 3x2
        map.set(2, 1, Some(7));
        map.resize(5, 4);
        assert_eq!(map.map_width, 5);
        assert_eq!(map.map_height, 4);
        assert_eq!(map.tiles.len(), 20);
        assert_eq!(map.get(2, 1), Some(7));
        assert_eq!(map.get(4, 3), None);
    }

    #[test]
    fn resize_drops_tiles_outside_new_bounds_when_shrinking() {
        let mut map = tiny(); // 3x2
        map.set(2, 1, Some(7));
        map.set(0, 0, Some(1));
        map.resize(2, 1);
        assert_eq!(map.tiles.len(), 2);
        assert_eq!(map.get(0, 0), Some(1));
    }
}
