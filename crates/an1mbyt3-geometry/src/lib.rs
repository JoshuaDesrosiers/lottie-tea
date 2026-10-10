//! `an1mbyt3-geometry`: paths, Bézier curves, tessellation, meshes, deformation.
//!
//! See AN1MBYT3_BUILD_PLAN.md §3 and §6 ("Rendering Strategy") — this crate is
//! where vector paths become GPU-friendly triangle meshes. Tessellation output
//! is meant to be cached per shape by the caller, invalidated only when the
//! shape's actual path data changes (transform-only animation must never
//! trigger re-tessellation) — see `tessellate.rs` for why that caching lives
//! outside this crate rather than in it.
//!
//! Owns: [`tessellate_path`] (path → [`Mesh`] via `lyon`'s fill tessellator),
//! [`tessellate_stroke`] (path outline → [`Mesh`] via `lyon`'s stroke
//! tessellator), [`trim_path`]/[`trim_path_with_offset`] (arc-length-cropping
//! a path — the "Trim Paths" effect) and [`dash_path`] (arc-length dash/gap
//! splitting — "stroke dashing", both in `trim.rs`), and [`skin_mesh`]
//! (linear blend skinning — deforming a mesh's vertices by more than one
//! bone's current transform, `skin.rs`).
//!
//! Depends on `an1mbyt3-core` for the [`an1mbyt3_core::Path`]/[`an1mbyt3_core::PathCommand`]
//! types it converts from and (for skinning) [`an1mbyt3_core::Transform2D`]'s
//! affine-matrix conversion.
//!
//! Status: Phase 1 (tessellation), Phase 5 (skinning), and Phase 10 (trim
//! paths/stroke tessellation) are implemented (AN1MBYT3_BUILD_PLAN.md §10).

mod mesh;
mod mesh3d;
mod model3d;
mod skin;
mod tessellate;
mod trim;
pub mod glass;
pub mod voxel;

pub use mesh::{unit_quad_mesh, Mesh};
pub use glass::{GlassPane, ShatterParams, ShatterResult};
pub use mesh3d::{primitive_mesh, Mesh3DData};
pub use voxel::{mesh_voxel_region, voxel_atlas, BlockDef, BlockSource, VoxelGrid, BLOCKS, VOXEL_MATERIAL_WGSL};
pub use model3d::{parse_gltf, ChannelPath, ModelChannel, ModelClip, ModelData, ModelNode, ModelPrimitive};
pub use skin::{skin_mesh, SkinBinding, VertexWeight};
pub use tessellate::{discs_mesh, tessellate_path, tessellate_polygons, tessellate_stroke, StrokeCap, StrokeJoin, StrokeStyle};
pub use trim::{dash_path, trim_path, trim_path_with_offset};
