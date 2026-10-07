//! Font shaping: turns a string into positioned glyph IDs via `rustybuzz`
//! (AN1MBYT3_BUILD_PLAN.md §9's Phase 3 entry: "font shaping via rustybuzz").
//!
//! This is what makes text correct for real scripts — ligatures, kerning, and
//! non-Latin shaping all happen here, before any glyph is drawn. Positions
//! and advances come out of `rustybuzz` in font design units (since this
//! module never calls `Face::set_pixels_per_em`) and are scaled to pixels
//! here, using the same `size_px / units_per_em` factor `outline.rs` uses for
//! glyph outlines — keeping both in the same unit space is what makes a
//! glyph's shaped position and its own outline line up correctly.

use glam::Vec2;

use crate::font::Font;

/// One shaped glyph: which glyph to draw, and where.
#[derive(Debug, Clone, Copy)]
pub struct ShapedGlyph {
    pub glyph_id: u16,
    /// This glyph's offset from the current pen position, in pixels.
    pub offset: Vec2,
    /// How far the pen advances after this glyph, in pixels.
    pub advance: Vec2,
}

/// Shapes `text` at `size_px`, returning one [`ShapedGlyph`] per glyph in
/// shaping order (which may reorder or merge characters relative to `text`'s
/// original code points — e.g. ligatures, right-to-left scripts).
pub fn shape_text(font: &Font, text: &str, size_px: f32) -> Vec<ShapedGlyph> {
    let face = font.shaping_face();
    let scale = size_px / face.units_per_em() as f32;

    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();

    let glyph_buffer = rustybuzz::shape(&face, &[], buffer);

    glyph_buffer
        .glyph_infos()
        .iter()
        .zip(glyph_buffer.glyph_positions())
        .map(|(info, pos)| ShapedGlyph {
            glyph_id: info.glyph_id as u16,
            offset: Vec2::new(pos.x_offset as f32, pos.y_offset as f32) * scale,
            advance: Vec2::new(pos.x_advance as f32, pos.y_advance as f32) * scale,
        })
        .collect()
}
