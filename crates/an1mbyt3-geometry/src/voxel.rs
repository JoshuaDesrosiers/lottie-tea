//! Block worlds (`Primitive3D::Voxels`): the built-in block types, their
//! procedurally painted 16×16 textures (one atlas), and the mesher that
//! turns a [`VoxelGrid`] region into a [`Mesh3DData`].
//!
//! **Meshing.** Only faces that can be seen are emitted: a face is drawn
//! when the neighbour on that side doesn't hide it ([`BlockDef::hides`]).
//! Every face is a quad of four vertices with
//! - UVs into the block atlas (the block's top / side / bottom tile), and
//! - per-vertex **ambient occlusion**: each corner looks at its three
//!   neighbours in the face's plane, one step out (two edges + the diagonal)
//!   and darkens with how many are solid — the classic voxel AO that makes
//!   creases and the ground under overhangs read. The quad's diagonal is
//!   flipped when that keeps the darkening symmetric.
//!
//! The AO factor is smuggled to the shader as the **length of the vertex
//! normal** (the mesh format has no colour channel): the 3D pass
//! normalizes normals before lighting, so the direction still lights the
//! face correctly, and the voxel material (`VOXEL_MATERIAL_WGSL`) reads
//! `length(normal)` back as the AO factor. That only holds for an
//! unscaled node, which a block world always is.
//!
//! **Coordinates.** Block `(x, y, z)` fills `[x, x+1] × [y, y+1] × [z, z+1]`
//! in the node's local space; outside the grid is air.

use crate::Mesh3DData;

/// Block id 0: nothing there.
pub const AIR: u8 = 0;
/// Pixels per block texture.
pub const TILE: u32 = 16;
/// Tiles per atlas row.
pub const ATLAS_COLUMNS: u32 = 16;
/// Atlas rows.
pub const ATLAS_ROWS: u32 = 2;

/// How a block type looks and behaves.
#[derive(Debug, Clone, Copy)]
pub struct BlockDef {
    pub name: &'static str,
    /// Atlas tiles for the top, the four sides and the bottom.
    pub top: u8,
    pub side: u8,
    pub bottom: u8,
    /// Fully covers its neighbours' faces (stone, dirt…). Leaves, glass and
    /// water don't: things behind them show through.
    pub opaque: bool,
    /// Faces between two blocks of this same type are skipped (glass panes
    /// and water read as one volume); leaves keep theirs, so a tree's
    /// canopy looks bushy through its holes.
    pub merge_same: bool,
}

impl BlockDef {
    /// Whether a block of this type at a face's far side hides a face of
    /// `other` (the block being meshed).
    pub fn hides(&self, self_id: u8, other_id: u8) -> bool {
        self_id != AIR && (self.opaque || (self.merge_same && self_id == other_id))
    }
}

const fn block(name: &'static str, top: u8, side: u8, bottom: u8, opaque: bool, merge_same: bool) -> BlockDef {
    BlockDef { name, top, side, bottom, opaque, merge_same }
}

/// Every block type, by id (the byte a script writes). Append-only: saved
/// worlds and scripts refer to blocks by these numbers.
pub const BLOCKS: &[BlockDef] = &[
    block("air", 0, 0, 0, false, false),
    block("grass", 0, 1, 2, true, false),
    block("dirt", 2, 2, 2, true, false),
    block("stone", 3, 3, 3, true, false),
    block("cobblestone", 4, 4, 4, true, false),
    block("sand", 5, 5, 5, true, false),
    block("gravel", 6, 6, 6, true, false),
    block("log", 8, 7, 8, true, false),
    block("leaves", 9, 9, 9, false, false),
    block("planks", 10, 10, 10, true, false),
    block("glass", 11, 11, 11, false, true),
    block("water", 12, 12, 12, false, true),
    block("bricks", 13, 13, 13, true, false),
    block("snow", 14, 21, 2, true, false),
    block("coal ore", 15, 15, 15, true, false),
    block("iron ore", 16, 16, 16, true, false),
    block("gold ore", 17, 17, 17, true, false),
    block("bedrock", 18, 18, 18, true, false),
    block("diamond ore", 19, 19, 19, true, false),
    block("stone bricks", 20, 20, 20, true, false),
    block("red wool", 22, 22, 22, true, false),
    block("lamp", 23, 23, 23, true, false),
];

/// The block type for an id (unknown ids draw as stone).
pub fn block_def(id: u8) -> &'static BlockDef {
    BLOCKS.get(id as usize).unwrap_or(&BLOCKS[3])
}

/// A `width × height × depth` box of block ids, x fastest, then z, then y
/// (index = `x + width * (z + depth * y)`), so a horizontal layer is one
/// contiguous run.
#[derive(Debug, Clone, Default)]
pub struct VoxelGrid {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub blocks: Vec<u8>,
}

impl VoxelGrid {
    /// An all-air grid.
    pub fn new(width: u32, height: u32, depth: u32) -> Self {
        Self { width, height, depth, blocks: vec![AIR; (width * height * depth) as usize] }
    }

    fn index(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        let (w, h, d) = (self.width as i32, self.height as i32, self.depth as i32);
        (x >= 0 && y >= 0 && z >= 0 && x < w && y < h && z < d).then(|| (x + w * (z + d * y)) as usize)
    }

    /// The block at `(x, y, z)`; air outside the grid.
    pub fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        self.index(x, y, z).map_or(AIR, |i| self.blocks[i])
    }

    /// Sets one block; `false` (and nothing changes) outside the grid.
    pub fn set(&mut self, x: i32, y: i32, z: i32, id: u8) -> bool {
        match self.index(x, y, z) {
            Some(i) => {
                self.blocks[i] = id;
                true
            }
            None => false,
        }
    }

}

/// Anything the mesher can read blocks from: a dense [`VoxelGrid`], or a
/// streamed world of loaded chunks (the wasm editor's chunked block
/// worlds). Unknown / unloaded places read as air.
pub trait BlockSource {
    fn block(&self, x: i32, y: i32, z: i32) -> u8;

    fn opaque(&self, x: i32, y: i32, z: i32) -> bool {
        block_def(self.block(x, y, z)).opaque
    }
}

impl BlockSource for VoxelGrid {
    fn block(&self, x: i32, y: i32, z: i32) -> u8 {
        self.get(x, y, z)
    }
}

/// One face direction: its normal and the two in-plane axes `u`, `v`
/// (`u × v = normal`, so corners in (−u−v, +u−v, +u+v, −u+v) order are
/// counter-clockwise from outside, matching the 3D pass's back-face culling).
struct Face {
    normal: [i32; 3],
    u: [i32; 3],
    v: [i32; 3],
    /// 0 = top tile, 1 = side, 2 = bottom.
    tile: u8,
}

const FACES: [Face; 6] = [
    Face { normal: [1, 0, 0], u: [0, 0, -1], v: [0, 1, 0], tile: 1 },
    Face { normal: [-1, 0, 0], u: [0, 0, 1], v: [0, 1, 0], tile: 1 },
    Face { normal: [0, 1, 0], u: [1, 0, 0], v: [0, 0, -1], tile: 0 },
    Face { normal: [0, -1, 0], u: [1, 0, 0], v: [0, 0, 1], tile: 2 },
    Face { normal: [0, 0, 1], u: [1, 0, 0], v: [0, 1, 0], tile: 1 },
    Face { normal: [0, 0, -1], u: [-1, 0, 0], v: [0, 1, 0], tile: 1 },
];

/// AO factor by how many of a corner's three neighbours are solid
/// (0 = open … 3 = tucked into a crease).
const AO: [f32; 4] = [1.0, 0.8, 0.64, 0.5];

/// The water surface sits this far below the block top (when there's no
/// water above), like a real water line.
const WATER_DROP: f32 = 0.12;

/// Meshes the blocks with `min <= (x, y, z) < max`, culling against
/// neighbours anywhere in `grid` (so region and chunk seams are invisible).
/// Positions are in the world's local space.
pub fn mesh_voxel_region<S: BlockSource + ?Sized>(grid: &S, min: [i32; 3], max: [i32; 3]) -> Mesh3DData {
    let mut m = Mesh3DData::default();
    let water = BLOCKS.iter().position(|b| b.name == "water").unwrap_or(usize::MAX) as u8;
    for y in min[1]..max[1] {
        for z in min[2]..max[2] {
            for x in min[0]..max[0] {
                let id = grid.block(x, y, z);
                if id == AIR {
                    continue;
                }
                let def = block_def(id);
                let surface_drop = if id == water && grid.block(x, y + 1, z) != water { WATER_DROP } else { 0.0 };
                for face in &FACES {
                    let [nx, ny, nz] = face.normal;
                    let neighbour = grid.block(x + nx, y + ny, z + nz);
                    if block_def(neighbour).hides(neighbour, id) {
                        continue;
                    }
                    let tile = [def.top, def.side, def.bottom][face.tile as usize];
                    emit_face(&mut m, grid, [x, y, z], face, tile, surface_drop);
                }
            }
        }
    }
    m
}

fn emit_face<S: BlockSource + ?Sized>(m: &mut Mesh3DData, grid: &S, at: [i32; 3], face: &Face, tile: u8, surface_drop: f32) {
    let add = |a: [i32; 3], b: [i32; 3]| [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
    let neg = |a: [i32; 3]| [-a[0], -a[1], -a[2]];
    // The block's centre, and the cell just outside this face (AO looks
    // at the ring of cells around it).
    let centre = [at[0] as f32 + 0.5, at[1] as f32 + 0.5, at[2] as f32 + 0.5];
    let out = add(at, face.normal);
    let base = m.positions.len() as u32;
    // Tile rectangle in the atlas (inset a hair so nearest sampling never
    // reads the neighbouring tile at an edge).
    let (col, row) = (tile as u32 % ATLAS_COLUMNS, tile as u32 / ATLAS_COLUMNS);
    let (tw, th) = (1.0 / ATLAS_COLUMNS as f32, 1.0 / ATLAS_ROWS as f32);
    let inset = 0.02 / (TILE * ATLAS_COLUMNS) as f32;
    let (u0, v0) = (col as f32 * tw + inset, row as f32 * th + inset);
    let (u1, v1) = ((col + 1) as f32 * tw - inset, (row + 1) as f32 * th - inset);
    let mut ao = [0usize; 4];
    for (k, (su, sv)) in [(-1, -1), (1, -1), (1, 1), (-1, 1)].into_iter().enumerate() {
        let du = if su < 0 { neg(face.u) } else { face.u };
        let dv = if sv < 0 { neg(face.v) } else { face.v };
        let side1 = grid.opaque(out[0] + du[0], out[1] + du[1], out[2] + du[2]);
        let side2 = grid.opaque(out[0] + dv[0], out[1] + dv[1], out[2] + dv[2]);
        let corner = grid.opaque(out[0] + du[0] + dv[0], out[1] + du[1] + dv[1], out[2] + du[2] + dv[2]);
        // Both edges solid: fully tucked in regardless of the diagonal.
        ao[k] = if side1 && side2 { 3 } else { side1 as usize + side2 as usize + corner as usize };
        let mut p = [0.0f32; 3];
        for i in 0..3 {
            p[i] = centre[i] + 0.5 * (face.normal[i] as f32 + su as f32 * face.u[i] as f32 + sv as f32 * face.v[i] as f32);
        }
        // Water's surface sits a little lower (top face, and the top edge
        // of its sides).
        if surface_drop > 0.0 && p[1] > centre[1] + 0.25 {
            p[1] -= surface_drop;
        }
        let shade = AO[ao[k]];
        m.positions.push(p);
        m.normals.push([face.normal[0] as f32 * shade, face.normal[1] as f32 * shade, face.normal[2] as f32 * shade]);
        // +v is the texture's top row (lower v coordinate).
        m.uvs.push([if su < 0 { u0 } else { u1 }, if sv < 0 { v1 } else { v0 }]);
    }
    // Split along the diagonal whose corners are less occluded, so AO
    // gradients don't show a seam (the anisotropy fix).
    if ao[0] + ao[2] > ao[1] + ao[3] {
        m.indices.extend([base + 1, base + 2, base + 3, base + 1, base + 3, base]);
    } else {
        m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// The 3D material block worlds draw with: the atlas texel (holes in
/// leaves / glass cut out) × the mesh colour × the AO factor carried in the
/// normal's length (see the module docs). Linked into the 3D pass like any
/// compiled shader graph (`compile_material_3d`).
pub const VOXEL_MATERIAL_WGSL: &str = r#"
fn material_fragment(in: VertexOutput) -> vec4<f32> {
    let texel = textureSample(image_texture, image_sampler, in.uv);
    if texel.a < 0.5 {
        discard;
    }
    let ao = clamp(length(in.normal), 0.0, 1.0);
    return vec4<f32>(texel.rgb * uniforms.color.rgb * ao, 1.0);
}
"#;

/// A small, fast integer hash → [0, 1).
fn hash(tile: u32, x: u32, y: u32, salt: u32) -> f32 {
    let mut h = tile.wrapping_mul(0x9E37_79B1) ^ x.wrapping_mul(0x85EB_CA77) ^ y.wrapping_mul(0xC2B2_AE3D) ^ salt.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h & 0xFF_FFFF) as f32 / 16_777_216.0
}

fn rgb(c: [f32; 3], light: f32) -> [u8; 4] {
    let f = |v: f32| ((v * light).clamp(0.0, 1.0) * 255.0).round() as u8;
    [f(c[0]), f(c[1]), f(c[2]), 255]
}

/// One texel of a block tile (sRGB bytes, like a PNG would hold). Painted
/// from hashes, so the atlas is identical every run.
fn tile_pixel(tile: u32, x: u32, y: u32) -> [u8; 4] {
    let n = hash(tile, x, y, 1);
    // A coarser 2×2 noise for chunkier pixel-art variation.
    let n2 = hash(tile, x / 2, y / 2, 2);
    let grain = 0.86 + n * 0.14 + (n2 - 0.5) * 0.12;
    const DIRT: [f32; 3] = [0.53, 0.38, 0.25];
    const GRASS: [f32; 3] = [0.38, 0.66, 0.24];
    const STONE: [f32; 3] = [0.52, 0.52, 0.53];
    let stone = |x: u32, y: u32| {
        let n = hash(3, x, y, 1);
        let blot = hash(3, x / 3, y / 3, 7);
        rgb(STONE, 0.84 + n * 0.16 - if blot > 0.8 { 0.12 } else { 0.0 })
    };
    let ore = |color: [f32; 3], x: u32, y: u32| {
        // Clusters: a few 2×2-ish blobs on stone.
        let blob = hash(tile, x / 3, y / 3, 11);
        let speck = hash(tile, x, y, 12);
        if blob > 0.62 && speck > 0.3 { rgb(color, 0.8 + speck * 0.3) } else { stone(x, y) }
    };
    match tile {
        // grass top
        0 => rgb(GRASS, grain),
        // grass side: dirt with a ragged grass fringe on top
        1 => {
            let fringe = 3 + (hash(1, x, 0, 3) * 2.5) as u32;
            if y < fringe { rgb(GRASS, grain * 0.95) } else { rgb(DIRT, grain) }
        }
        2 => rgb(DIRT, grain - if hash(2, x, y, 4) > 0.9 { 0.15 } else { 0.0 }),
        3 => stone(x, y),
        // cobblestone: rounded stones with dark mortar between
        4 => {
            let (cx, cy) = ((x + if (y / 5) % 2 == 0 { 0 } else { 3 }) % 16, y);
            let edge = cx % 6 == 0 || cy % 5 == 0;
            let tone = hash(4, cx / 6, cy / 5, 5);
            if edge { rgb(STONE, 0.45 + n * 0.1) } else { rgb(STONE, 0.72 + tone * 0.28 + (n - 0.5) * 0.1) }
        }
        5 => rgb([0.86, 0.8, 0.56], 0.92 + n * 0.08),
        // gravel: mixed grey pebbles
        6 => {
            let p = hash(6, x / 2, y / 2, 6);
            rgb(if p > 0.7 { [0.62, 0.58, 0.55] } else if p > 0.35 { [0.48, 0.46, 0.45] } else { [0.36, 0.34, 0.34] }, 0.9 + n * 0.15)
        }
        // log side: vertical bark grooves
        7 => {
            let groove = hash(7, x, y / 4, 8) > 0.72 || x % 5 == 0;
            rgb([0.42, 0.31, 0.18], if groove { 0.7 } else { 0.95 + n * 0.1 })
        }
        // log top: growth rings inside a bark border
        8 => {
            let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
            if x == 0 || y == 0 || x == 15 || y == 15 {
                rgb([0.42, 0.31, 0.18], 0.85)
            } else {
                let ring = ((dx * dx + dy * dy).sqrt() as u32) % 3 == 0;
                rgb([0.72, 0.58, 0.36], if ring { 0.82 } else { 0.98 + n * 0.05 })
            }
        }
        // leaves: dappled green with see-through holes
        9 => {
            if hash(9, x, y, 9) < 0.16 {
                [0, 0, 0, 0]
            } else {
                rgb([0.22, 0.5, 0.17], 0.7 + n2 * 0.35 + n * 0.1)
            }
        }
        // planks: horizontal boards with staggered seams
        10 => {
            let board = y / 4;
            let seam_x = if board % 2 == 0 { 3 } else { 11 };
            let line = y % 4 == 3 || x == seam_x;
            rgb([0.72, 0.56, 0.33], if line { 0.68 } else { 0.92 + hash(10, x / 4, board, 10) * 0.08 + (n - 0.5) * 0.06 })
        }
        // glass: a pale frame, clear middle with a couple of glints
        11 => {
            let frame = x == 0 || y == 0 || x == 15 || y == 15;
            let glint = (x + y == 6 || x + y == 7) && x > 1 && x < 7;
            if frame { [214, 236, 245, 255] } else if glint { [240, 250, 255, 255] } else { [0, 0, 0, 0] }
        }
        // water
        12 => rgb([0.17, 0.4, 0.82], 0.88 + n2 * 0.18),
        // bricks
        13 => {
            let row = y / 4;
            let offset = if row % 2 == 0 { 0 } else { 4 };
            let mortar = y % 4 == 3 || (x + offset) % 8 == 7;
            if mortar { rgb([0.72, 0.7, 0.66], 0.9 + n * 0.1) } else { rgb([0.64, 0.28, 0.21], 0.86 + n * 0.14) }
        }
        14 => rgb([0.95, 0.97, 1.0], 0.95 + n * 0.05),
        15 => ore([0.12, 0.12, 0.12], x, y),
        16 => ore([0.84, 0.64, 0.5], x, y),
        17 => ore([0.98, 0.82, 0.2], x, y),
        // bedrock: harsh dark noise
        18 => rgb([0.3, 0.3, 0.3], 0.4 + n2 * 0.9),
        19 => ore([0.45, 0.92, 0.95], x, y),
        // stone bricks: big blocks with offset seams
        20 => {
            let row = y / 8;
            let offset = if row % 2 == 0 { 0 } else { 8 };
            let seam = y % 8 == 7 || (x + offset) % 16 == 15;
            if seam { rgb(STONE, 0.5) } else { rgb(STONE, 0.86 + n * 0.12) }
        }
        // snowy side: dirt with snow on top
        21 => {
            let fringe = 3 + (hash(21, x, 0, 3) * 2.5) as u32;
            if y < fringe { rgb([0.95, 0.97, 1.0], 0.95 + n * 0.05) } else { rgb(DIRT, grain) }
        }
        22 => rgb([0.8, 0.18, 0.18], 0.9 + n2 * 0.12),
        // lamp: a warm glowing panel in an iron frame
        23 => {
            let frame = x == 0 || y == 0 || x == 15 || y == 15 || x == 7 || y == 7;
            if frame { rgb([0.35, 0.33, 0.3], 1.0) } else { rgb([1.0, 0.86, 0.5], 0.95 + n * 0.05) }
        }
        _ => [255, 0, 255, 255],
    }
}

/// The block atlas: `ATLAS_COLUMNS × ATLAS_ROWS` tiles of `TILE` px, RGBA8
/// (sRGB). Returns `(width, height, pixels)`.
pub fn voxel_atlas() -> (u32, u32, Vec<u8>) {
    let (w, h) = (ATLAS_COLUMNS * TILE, ATLAS_ROWS * TILE);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for tile in 0..ATLAS_COLUMNS * ATLAS_ROWS {
        let (col, row) = (tile % ATLAS_COLUMNS, tile / ATLAS_COLUMNS);
        for y in 0..TILE {
            for x in 0..TILE {
                let px = tile_pixel(tile, x, y);
                let i = (((row * TILE + y) * w + col * TILE + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&px);
            }
        }
    }
    (w, h, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_id(name: &str) -> u8 {
        BLOCKS.iter().position(|b| b.name == name).unwrap() as u8
    }

    #[test]
    fn a_lone_block_has_six_faces_and_a_buried_one_none() {
        let mut grid = VoxelGrid::new(3, 3, 3);
        grid.set(1, 1, 1, block_id("stone"));
        assert_eq!(mesh_voxel_region(&grid, [0; 3], [3; 3]).triangle_count(), 12);
        // Fill everything: only the 6 outer sides of the 3×3×3 box remain.
        for b in grid.blocks.iter_mut() {
            *b = block_id("dirt");
        }
        assert_eq!(mesh_voxel_region(&grid, [0; 3], [3; 3]).triangle_count(), 6 * 9 * 2);
        // The centre block alone (region = just it) is fully hidden.
        assert_eq!(mesh_voxel_region(&grid, [1; 3], [2; 3]).triangle_count(), 0);
    }

    #[test]
    fn see_through_blocks_cull_only_what_they_should() {
        let mut grid = VoxelGrid::new(2, 1, 1);
        // Glass next to glass: the shared face is skipped on both sides.
        grid.set(0, 0, 0, block_id("glass"));
        grid.set(1, 0, 0, block_id("glass"));
        assert_eq!(mesh_voxel_region(&grid, [0; 3], [2, 1, 1]).triangle_count(), 10 * 2);
        // Leaves next to leaves keep both inner faces (bushy canopies).
        grid.set(0, 0, 0, block_id("leaves"));
        grid.set(1, 0, 0, block_id("leaves"));
        assert_eq!(mesh_voxel_region(&grid, [0; 3], [2, 1, 1]).triangle_count(), 12 * 2);
        // Stone next to glass: the stone's face shows through the glass, the
        // glass face against the stone is hidden.
        grid.set(0, 0, 0, block_id("stone"));
        grid.set(1, 0, 0, block_id("glass"));
        assert_eq!(mesh_voxel_region(&grid, [0; 3], [2, 1, 1]).triangle_count(), 11 * 2);
    }

    #[test]
    fn ambient_occlusion_darkens_a_crease() {
        // A floor with one block standing on it: the floor's top corners
        // that touch the block are darker (shorter normal) than open ones.
        let mut grid = VoxelGrid::new(3, 2, 3);
        for x in 0..3 {
            for z in 0..3 {
                grid.set(x, 0, z, block_id("stone"));
            }
        }
        grid.set(1, 1, 1, block_id("stone"));
        let m = mesh_voxel_region(&grid, [0; 3], [3, 1, 3]);
        let lengths: Vec<f32> = m.normals.iter().filter(|n| n[1] > 0.0).map(|n| n[1]).collect();
        assert!(lengths.iter().any(|&l| (l - 1.0).abs() < 1e-6));
        assert!(lengths.iter().any(|&l| l < 0.9));
    }

    #[test]
    fn the_atlas_is_deterministic_and_leaves_have_holes() {
        let (w, h, a) = voxel_atlas();
        assert_eq!((w, h), (256, 32));
        assert_eq!(a, voxel_atlas().2);
        let leaves = BLOCKS[block_id("leaves") as usize].side as u32;
        let transparent = (0..TILE * TILE).filter(|i| a[((((leaves / ATLAS_COLUMNS) * TILE + i / TILE) * w + (leaves % ATLAS_COLUMNS) * TILE + i % TILE) * 4 + 3) as usize] == 0).count();
        assert!(transparent > 10 && transparent < 100, "{transparent}");
    }
}
