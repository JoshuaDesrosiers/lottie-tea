//! `an1mbyt3-text`: font shaping, glyph outline extraction, and per-glyph
//! mesh generation for text nodes.
//!
//! See AN1MBYT3_BUILD_PLAN.md §6 and §10 (Phase 3). Pipeline:
//! [`shape::shape_text`] (rustybuzz — correct positions/kerning/ligatures) →
//! [`outline::glyph_outline`] (glyph ID → `an1mbyt3_core::Path`, the same
//! vector-path type every shape node uses) → `an1mbyt3_geometry::tessellate_path`
//! (glyph outline → mesh, reusing Phase 1's tessellation pipeline exactly).
//! [`run::shape_and_tessellate`] is the one call most callers need — it chains
//! all three steps for a whole string.
//!
//! **This crate does not implement a glyph atlas or SDF rendering** — see
//! `outline.rs`'s module doc for why that's a deliberate Phase 3 scope
//! decision (vector-outline tessellation reuses the existing renderer
//! entirely; an atlas is a future optimization once profiling calls for it),
//! not an oversight relative to AN1MBYT3_BUILD_PLAN.md §6's original framing.
//!
//! Does NOT own: how a text node's per-glyph meshes actually get positioned
//! in the document/scene graph, or when re-shaping should happen versus
//! reusing a cached [`run::ShapedText`] — both are caller responsibilities
//! (currently the `text_shape` example in `an1mbyt3-renderer`; the editor
//! and a real `TextNode` in `an1mbyt3-core` are later work, since `NodeKind`
//! still only has `Shape`/`Group` variants — see `an1mbyt3-core`'s lib.rs).
//!
//! Depends on `an1mbyt3-core` (the `Path`/`Transform2D` types), `an1mbyt3-geometry`
//! (tessellation), and `an1mbyt3-renderer` (declared per AN1MBYT3_BUILD_PLAN.md
//! §3's dependency graph; not yet used directly by this crate's own code — the
//! `text_shape` demo uploads/draws the meshes this crate produces itself,
//! through `an1mbyt3-renderer`'s existing `Renderer`/`GpuMesh` API).
//!
//! Status: Phase 3 (AN1MBYT3_BUILD_PLAN.md §10) — shaping, outline
//! extraction, and mesh generation are implemented. A real `NodeKind::Text`
//! variant, glyph-mesh caching keyed on (font, size, string), and atlas-based
//! rendering are not implemented yet.

mod font;
mod outline;
mod run;
mod shape;

pub use font::{Font, FontError};
pub use outline::glyph_outline;
pub use run::{shape_and_tessellate, PositionedGlyphMesh, ShapedText};
pub use shape::{shape_text, ShapedGlyph};
