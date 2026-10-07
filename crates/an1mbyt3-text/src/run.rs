//! Ties shaping (`shape.rs`) and outline extraction (`outline.rs`) together
//! into per-glyph meshes, ready for `an1mbyt3-renderer` to upload and draw.
//!
//! This is the crate's one entry point most callers need: [`shape_and_tessellate`]
//! does the (relatively) expensive work — shaping + outline extraction +
//! tessellation — exactly once per (font, text, size) combination. Callers
//! (the `text_shape` example, later the editor/runtime) are responsible for
//! only calling this again when the string/font/size actually changes and
//! reusing the result otherwise — AN1MBYT3_BUILD_PLAN.md §6's "re-shaping
//! only happens when the string/font/size actually changes; a moving text
//! node is a transform-only update like any other node" is a caller
//! discipline this function enables, not something it enforces itself (it's
//! a pure function with no cache of its own).

use glam::Vec2;

use an1mbyt3_geometry::{tessellate_path, Mesh};

use crate::font::Font;
use crate::outline::glyph_outline;
use crate::shape::shape_text;

/// One glyph's tessellated mesh, positioned relative to the text run's origin.
pub struct PositionedGlyphMesh {
    pub mesh: Mesh,
    /// This glyph's position relative to the text's origin (baseline start),
    /// in the same local units as whatever node's transform ends up
    /// composed with it via [`an1mbyt3_core::Transform2D::with_local_offset`].
    pub offset: Vec2,
}

/// A fully shaped and tessellated text run: one mesh per glyph, plus the
/// total pen advance (useful for alignment — e.g. centering text by offsetting
/// every glyph by `-advance / 2`).
pub struct ShapedText {
    pub glyphs: Vec<PositionedGlyphMesh>,
    pub advance: Vec2,
}

/// Shapes `text` with `font` at `size_px` and tessellates every resulting
/// glyph's outline into a mesh, positioned along the shaped baseline.
pub fn shape_and_tessellate(font: &Font, text: &str, size_px: f32) -> ShapedText {
    let shaped_glyphs = shape_text(font, text, size_px);

    let mut pen = Vec2::ZERO;
    let mut glyphs = Vec::with_capacity(shaped_glyphs.len());
    for glyph in &shaped_glyphs {
        let path = glyph_outline(font, glyph.glyph_id, size_px);
        let mesh = tessellate_path(&path);
        glyphs.push(PositionedGlyphMesh { mesh, offset: pen + glyph.offset });
        pen += glyph.advance;
    }

    ShapedText { glyphs, advance: pen }
}
