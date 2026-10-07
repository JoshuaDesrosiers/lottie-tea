//! GPU-friendly triangle mesh: the output of tessellation.
//!
//! Deliberately renderer-agnostic (no `wgpu` types here) — `an1mbyt3-renderer`
//! decides how to upload this into actual GPU buffers. Keeping this crate free
//! of `wgpu` means geometry code (and its tests) never need a GPU device.

/// A flat, already-triangulated 2D mesh: positions plus a triangle index list.
///
/// Positions are in the shape's local space (i.e. before the node's
/// `Transform2D` is applied) — AN1MBYT3_BUILD_PLAN.md §6: "Transform-only
/// animation should update matrices rather than rebuild geometry" only holds if
/// the mesh itself never bakes a node's transform into its vertex positions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<[f32; 2]>,
    /// Triangle list indices (every 3 entries = one triangle) into `positions`.
    pub indices: Vec<u32>,
    /// Per-vertex texture coordinates — empty (the default, every existing
    /// mesh-building function here) means "none authored": `Renderer::upload_mesh`
    /// pads the GPU-side buffer with zeros and the shader's textured path
    /// keeps deriving UV from local position normalized against the mesh's
    /// own bounds instead (today's only behavior, unchanged for every Shape/
    /// Image/stroke draw). A real, populated `uvs` (same length as
    /// `positions`) is what a multi-sub-rect draw — a `Tilemap` sampling many
    /// different cells out of one shared tileset texture — needs instead, and
    /// is the one thing position-derived UV structurally cannot express
    /// (it only ever spans a mesh's whole bounding box). See
    /// `an1mbyt3_renderer::shader.wgsl`'s `compute_textured_color` for the
    /// per-draw-call switch between the two.
    pub uvs: Vec<[f32; 2]>,
}

/// A plain quad spanning `-0.5..0.5` on both axes — the fixed local geometry
/// every raster `Image` node draws through. Deliberately never scaled to a
/// node's actual `width`/`height` here: that scaling happens via the node's
/// own `Transform2D` at draw time (same "geometry never bakes a node's
/// transform" rule this module's own doc comment already states for
/// tessellated paths), which is also what lets every `Image` node share this
/// exact one mesh instead of needing its own uniquely-sized geometry.
/// `an1mbyt3-renderer`'s textured shader relies on this exact `±0.5` range
/// to derive UV coordinates directly from the local vertex position
/// (`uv = local_position + vec2(0.5, 0.5)`) with no separate UV attribute.
pub fn unit_quad_mesh() -> Mesh {
    Mesh {
        positions: vec![[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]],
        indices: vec![0, 1, 2, 0, 2, 3],
        uvs: Vec::new(),
    }
}
