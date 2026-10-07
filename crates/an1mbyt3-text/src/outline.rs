//! Glyph outline extraction: turns one glyph ID into a `an1mbyt3_core::Path`
//! (the same vector-path representation every other shape node uses).
//!
//! **Architecture note — deliberate deviation from a texture atlas:**
//! AN1MBYT3_BUILD_PLAN.md §6 describes glyphs eventually being cached "the
//! same way tessellated paths" are — via a rasterized/SDF glyph atlas. Phase 3
//! implements the simpler half of that idea first: extract each glyph's
//! *vector* outline and feed it through the exact same
//! `an1mbyt3_geometry::tessellate_path` pipeline every other shape uses, with
//! no rasterization or atlas at all. This reuses 100% of the Phase 1
//! tessellation/mesh/renderer machinery, renders crisply at any scale (no
//! atlas resolution limit), and still satisfies the caching rule — a glyph's
//! mesh only needs to be regenerated when the string/font/size changes, not
//! on every frame or on transform-only movement (see `run.rs`). A textured
//! atlas remains the right future optimization once profiling shows
//! per-glyph tessellation cost (not text volume itself) is the bottleneck —
//! not needed to prove the Phase 3 pipeline works, matching the
//! prove-the-pipeline-before-gold-plating approach in §11.

use glam::Vec2;
use rustybuzz::ttf_parser::{GlyphId, OutlineBuilder};

use an1mbyt3_core::{Path, PathCommand};

use crate::font::Font;

/// Extracts glyph `glyph_id`'s outline from `font` as a [`Path`], scaled so
/// `size_px` matches the same pixel size `shape_text` positions glyphs at.
///
/// Returns an empty [`Path`] for glyphs with no outline (e.g. the space
/// character) — `an1mbyt3_geometry::tessellate_path` already handles an
/// empty `Path` by producing an empty [`an1mbyt3_geometry::Mesh`] (see that
/// crate's tests), so callers don't need a special case here.
pub fn glyph_outline(font: &Font, glyph_id: u16, size_px: f32) -> Path {
    let face = font.outline_face();
    let scale = size_px / face.units_per_em() as f32;

    let mut builder = PathOutlineBuilder {
        commands: Vec::new(),
        current: Vec2::ZERO,
        scale,
    };
    // Deliberately ignoring the `Option<Rect>` bounding box this returns —
    // `None` just means "no outline" (e.g. space), which already produces the
    // empty `Path` we want via `builder.commands` staying empty.
    let _ = face.outline_glyph(GlyphId(glyph_id), &mut builder);

    Path { commands: builder.commands }
}

/// Converts `ttf_parser`'s outline callbacks into a [`PathCommand`] list.
///
/// TrueType/OpenType outlines can use quadratic Bézier curves (`quad_to`),
/// but `an1mbyt3_core::PathCommand` only has a cubic variant (matching every
/// other vector shape in the document model, which has no reason to support
/// two curve degrees) — `quad_to` is elevated to an equivalent cubic curve
/// via the standard degree-elevation formula (`c1 = p0 + 2/3(c-p0)`,
/// `c2 = p1 + 2/3(c-p1)`), which is exact, not an approximation.
struct PathOutlineBuilder {
    commands: Vec<PathCommand>,
    /// The current point, needed only for quadratic-to-cubic elevation —
    /// `ttf_parser`'s callbacks don't hand back the "current point" a curve
    /// starts from, so this tracks it ourselves.
    current: Vec2,
    scale: f32,
}

impl OutlineBuilder for PathOutlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = Vec2::new(x, y) * self.scale;
        self.commands.push(PathCommand::MoveTo(p));
        self.current = p;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = Vec2::new(x, y) * self.scale;
        self.commands.push(PathCommand::LineTo(p));
        self.current = p;
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let control = Vec2::new(x1, y1) * self.scale;
        let to = Vec2::new(x, y) * self.scale;
        let control1 = self.current + (control - self.current) * (2.0 / 3.0);
        let control2 = to + (control - to) * (2.0 / 3.0);
        self.commands.push(PathCommand::CubicTo { control1, control2, to });
        self.current = to;
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let control1 = Vec2::new(x1, y1) * self.scale;
        let control2 = Vec2::new(x2, y2) * self.scale;
        let to = Vec2::new(x, y) * self.scale;
        self.commands.push(PathCommand::CubicTo { control1, control2, to });
        self.current = to;
    }

    fn close(&mut self) {
        self.commands.push(PathCommand::Close);
    }
}
