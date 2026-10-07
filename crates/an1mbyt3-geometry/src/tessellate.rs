//! Path tessellation: turns a `an1mbyt3_core::Path` (a plain move/line/cubic/
//! close command list) into a [`Mesh`] via `lyon`'s fill tessellator.
//!
//! AN1MBYT3_BUILD_PLAN.md §2 picks `lyon` for this "initially, with custom
//! GPU/vector systems where needed" — this function is the seam where a future
//! custom tessellator would slot in without changing `an1mbyt3_core::Path` or
//! [`Mesh`], since both are already `lyon`-agnostic.
//!
//! This does a full re-tessellation on every call — the caching described in
//! §6 ("tessellation output is cached per shape... invalidated only when a
//! shape's actual path data changes") is the caller's responsibility (the
//! renderer's per-frame dirty-tracking, not this crate's), so this function
//! stays a pure, stateless `Path -> Mesh` conversion.

use an1mbyt3_core::{Path, PathCommand};
use lyon::math::point;
use lyon::path::Path as LyonPath;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, LineCap, LineJoin, StrokeOptions,
    StrokeTessellator, StrokeVertex, VertexBuffers,
};

use crate::mesh::Mesh;

/// How a stroke's ends are capped — the same three options every vector
/// tool exposes (SVG's `stroke-linecap`, Illustrator, Rive): a flat edge
/// exactly at the endpoint (`Butt`), a flat edge extended by half the line
/// width (`Square`), or a semicircle (`Round`). Matters most on a trimmed,
/// open stroke (a full closed shape's stroke has no visible ends to cap) —
/// `Round` is the common "loading spinner" trim-path look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeCap {
    Butt,
    Square,
    Round,
}

/// How two stroke segments meet at a corner — SVG's `stroke-linejoin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeJoin {
    Miter,
    Round,
    Bevel,
}

/// A stroke's rendering style: width plus the cap/join behavior above.
/// `width` has no reasonable default for a UI shape (see
/// [`tessellate_stroke`]'s doc comment), so callers building a `StrokeStyle`
/// via `..Default::default()` still have to set it explicitly; `cap`/`join`
/// default to `Butt`/`Miter` — `lyon`'s own defaults, and SVG's.
#[derive(Debug, Clone, Copy)]
pub struct StrokeStyle {
    pub width: f32,
    pub cap: StrokeCap,
    pub join: StrokeJoin,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self { width: 1.0, cap: StrokeCap::Butt, join: StrokeJoin::Miter }
    }
}

/// Tessellates `path` into a fill mesh. Returns an empty [`Mesh`] if `path` has
/// no subpaths (e.g. a freshly-constructed, never-drawn-into `Path`).
pub fn tessellate_path(path: &Path) -> Mesh {
    let lyon_path = to_lyon_path(path);

    let mut geometry: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();

    tessellator
        .tessellate_path(
            &lyon_path,
            &FillOptions::default(),
            &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
                let p = vertex.position();
                [p.x, p.y]
            }),
        )
        .expect("lyon fill tessellation failed — see PathCommand construction for malformed input");

    Mesh {
        positions: geometry.vertices,
        indices: geometry.indices,
        uvs: Vec::new(),
    }
}

/// Tessellates `path`'s *outline* (a stroke of constant `width`) rather than
/// its filled interior — via `lyon`'s `StrokeTessellator`, the counterpart to
/// [`tessellate_path`]'s `FillTessellator`. This exists specifically to make
/// trim paths (AN1MBYT3_BUILD_PLAN.md §6's "Vector path effects" note)
/// meaningful: a trimmed *fill* doesn't correspond to anything After
/// Effects/Rive users would recognize (cropping a filled polygon's area
/// arc-length-wise isn't "Trim Paths"), but a trimmed *stroke* — only drawing
/// the outline between two arc-length percentages — is exactly that effect.
/// See [`crate::trim::trim_path`] for the actual arc-length cropping step;
/// this function only turns whatever `Path` it's given (trimmed or not) into
/// stroke geometry.
///
/// `style` controls width plus cap/join behavior (see [`StrokeStyle`]) —
/// there's no reasonable "some outline, any width" default for a UI shape,
/// so every caller sets `width` explicitly even when using
/// `..Default::default()` for cap/join.
pub fn tessellate_stroke(path: &Path, style: &StrokeStyle) -> Mesh {
    let lyon_path = to_lyon_path(path);

    let mut geometry: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let mut tessellator = StrokeTessellator::new();
    let options = StrokeOptions::default()
        .with_line_width(style.width)
        .with_start_cap(to_lyon_cap(style.cap))
        .with_end_cap(to_lyon_cap(style.cap))
        .with_line_join(to_lyon_join(style.join));

    tessellator
        .tessellate_path(
            &lyon_path,
            &options,
            &mut BuffersBuilder::new(&mut geometry, |vertex: StrokeVertex| {
                let p = vertex.position();
                [p.x, p.y]
            }),
        )
        .expect("lyon stroke tessellation failed — see PathCommand construction for malformed input");

    Mesh {
        positions: geometry.vertices,
        indices: geometry.indices,
        uvs: Vec::new(),
    }
}

fn to_lyon_cap(cap: StrokeCap) -> LineCap {
    match cap {
        StrokeCap::Butt => LineCap::Butt,
        StrokeCap::Square => LineCap::Square,
        StrokeCap::Round => LineCap::Round,
    }
}

fn to_lyon_join(join: StrokeJoin) -> LineJoin {
    match join {
        StrokeJoin::Miter => LineJoin::Miter,
        StrokeJoin::Round => LineJoin::Round,
        StrokeJoin::Bevel => LineJoin::Bevel,
    }
}

/// Converts our engine-agnostic [`PathCommand`] list into a `lyon_path::Path`.
///
/// `lyon`'s builder requires an explicit `begin`/`end` pair around each
/// subpath (unlike our command list, where a bare `MoveTo` implicitly starts a
/// new subpath and may follow a still-open previous one) — `open` tracks
/// whether a subpath is currently between `begin` and `end` so we can insert
/// the `end(false)` lyon needs before starting the next one.
///
/// `pub(crate)` (not private) so [`crate::trim`] can reuse it instead of
/// duplicating this conversion — trim paths and stroking both start from the
/// same `lyon_path::Path` this function already builds.
pub(crate) fn to_lyon_path(path: &Path) -> LyonPath {
    let mut builder = LyonPath::builder();
    let mut open = false;

    for command in &path.commands {
        match *command {
            PathCommand::MoveTo(p) => {
                if open {
                    builder.end(false);
                }
                builder.begin(point(p.x, p.y));
                open = true;
            }
            PathCommand::LineTo(p) => {
                debug_assert!(open, "LineTo before any MoveTo — malformed Path");
                builder.line_to(point(p.x, p.y));
            }
            PathCommand::CubicTo { control1, control2, to } => {
                debug_assert!(open, "CubicTo before any MoveTo — malformed Path");
                builder.cubic_bezier_to(
                    point(control1.x, control1.y),
                    point(control2.x, control2.y),
                    point(to.x, to.y),
                );
            }
            PathCommand::Close => {
                builder.end(true);
                open = false;
            }
        }
    }

    if open {
        builder.end(false);
    }

    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_tessellates_to_two_triangles() {
        let mesh = tessellate_path(&Path::rect(100.0, 50.0));
        // A convex quad fills as exactly 2 triangles: 6 indices, 4 unique positions.
        assert_eq!(mesh.indices.len(), 6);
        assert_eq!(mesh.positions.len(), 4);
    }

    #[test]
    fn stroke_of_a_rect_produces_geometry() {
        // A stroke is a ring of quads around the path's outline — not a
        // meaningful count to pin exactly (join/cap geometry varies), just
        // confirming it produces real triangles rather than an empty mesh
        // (the failure mode a wrong `StrokeOptions`/degenerate path would hit).
        let mesh = tessellate_stroke(&Path::rect(100.0, 50.0), &StrokeStyle { width: 4.0, ..Default::default() });
        assert!(!mesh.positions.is_empty());
        assert!(!mesh.indices.is_empty());
    }

    #[test]
    fn empty_path_tessellates_to_empty_mesh() {
        let mesh = tessellate_path(&Path::new());
        assert!(mesh.positions.is_empty());
        assert!(mesh.indices.is_empty());
    }
}
