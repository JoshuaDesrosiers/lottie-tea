//! Vector path representation.
//!
//! This is deliberately a plain, tessellation-library-agnostic command list —
//! `an1mbyt3-core` does not depend on `lyon` or know anything about
//! tessellation (AN1MBYT3_BUILD_PLAN.md §3: `an1mbyt3-geometry` owns that).
//! Keeping `Path` free of `lyon` types means the document model can change its
//! tessellation backend later (§6 mentions a possible custom GPU/vector system)
//! without touching the document schema or its serialized `.tea` format.

use glam::Vec2;
use serde::{Deserialize, Serialize};

/// The standard cubic Bézier evaluation (De Casteljau's formula, expanded) —
/// `p0`/`p3` the curve's endpoints, `p1`/`p2` its control points, `t` the
/// curve-local parameter in `0.0..=1.0`. Shared by `Path::point_at_progress`'s
/// two uses: sampling a `CubicTo` at fixed steps to approximate its arc
/// length, and evaluating it at the exact local `t` a target global length
/// resolves to.
fn cubic_bezier_point(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, t: f32) -> Vec2 {
    let mt = 1.0 - t;
    p0 * (mt * mt * mt) + p1 * (3.0 * mt * mt * t) + p2 * (3.0 * mt * t * t) + p3 * (t * t * t)
}

/// One drawing instruction in a vector path, in the same vocabulary as SVG/PDF
/// path data: move, line, cubic Bézier curve, close.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PathCommand {
    /// Starts a new subpath at `point`, without connecting it to the previous one.
    MoveTo(Vec2),
    /// Straight line from the current point to `point`.
    LineTo(Vec2),
    /// Cubic Bézier curve from the current point to `to`, via two control points.
    CubicTo { control1: Vec2, control2: Vec2, to: Vec2 },
    /// Closes the current subpath with a straight line back to its start point.
    Close,
}

/// A vector path: an ordered list of [`PathCommand`]s, possibly containing
/// multiple subpaths (each starting with its own `MoveTo`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Path {
    pub commands: Vec<PathCommand>,
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a closed rectangular path centered on the origin, `width` x
    /// `height` in size — the minimal shape needed for the Phase 1 vertical
    /// slice (AN1MBYT3_BUILD_PLAN.md §11): draw a shape, animate it, render it.
    pub fn rect(width: f32, height: f32) -> Self {
        let hw = width / 2.0;
        let hh = height / 2.0;
        Self {
            commands: vec![
                PathCommand::MoveTo(Vec2::new(-hw, -hh)),
                PathCommand::LineTo(Vec2::new(hw, -hh)),
                PathCommand::LineTo(Vec2::new(hw, hh)),
                PathCommand::LineTo(Vec2::new(-hw, hh)),
                PathCommand::Close,
            ],
        }
    }

    /// Builds a closed elliptical path centered on the origin, `width` x
    /// `height` in size, from four cubic Bézier arcs — the standard
    /// circle/ellipse Bézier approximation (control-point offset ≈
    /// `0.5522847498 * radius`, the constant that makes a 4-arc Bézier
    /// circle deviate from a true circle by well under a pixel at any size
    /// this editor draws shapes at). Passing equal `width`/`height` gives a
    /// circle — there's no separate `Path::circle`, since a circle is just
    /// this with one fewer degree of freedom, same "don't invent a second
    /// concept for what one parameterized shape already covers" reasoning
    /// `ComponentVariant` documents elsewhere.
    pub fn ellipse(width: f32, height: f32) -> Self {
        const KAPPA: f32 = 0.5522847498;
        let rx = width / 2.0;
        let ry = height / 2.0;
        let (cx, cy) = (rx * KAPPA, ry * KAPPA);
        Self {
            commands: vec![
                PathCommand::MoveTo(Vec2::new(rx, 0.0)),
                PathCommand::CubicTo { control1: Vec2::new(rx, cy), control2: Vec2::new(cx, ry), to: Vec2::new(0.0, ry) },
                PathCommand::CubicTo { control1: Vec2::new(-cx, ry), control2: Vec2::new(-rx, cy), to: Vec2::new(-rx, 0.0) },
                PathCommand::CubicTo { control1: Vec2::new(-rx, -cy), control2: Vec2::new(-cx, -ry), to: Vec2::new(0.0, -ry) },
                PathCommand::CubicTo { control1: Vec2::new(cx, -ry), control2: Vec2::new(rx, -cy), to: Vec2::new(rx, 0.0) },
                PathCommand::Close,
            ],
        }
    }

    /// Builds a closed regular-polygon path centered on the origin,
    /// `sides` vertices (`sides < 3` is nonsensical for a closed polygon and
    /// produces however-many straight lines the caller asked for — not
    /// worth a `Result`/panic for a shape-preset button that only ever
    /// passes a fixed, valid `sides` anyway), inscribed in a circle of
    /// `radius`. The first vertex sits straight up (`-90°`, i.e. `(0,
    /// -radius)`) rather than at angle `0°` so a triangle preset renders
    /// point-up like every design tool's triangle icon, not point-right.
    pub fn polygon(sides: u32, radius: f32) -> Self {
        let mut commands = Vec::with_capacity(sides as usize + 1);
        for i in 0..sides {
            let angle = -std::f32::consts::FRAC_PI_2 + (i as f32) * std::f32::consts::TAU / (sides as f32);
            let point = Vec2::new(angle.cos() * radius, angle.sin() * radius);
            commands.push(if i == 0 { PathCommand::MoveTo(point) } else { PathCommand::LineTo(point) });
        }
        commands.push(PathCommand::Close);
        Self { commands }
    }

    /// Builds a closed `points`-pointed star path centered on the origin,
    /// alternating between `outer_radius` (each point's tip) and
    /// `inner_radius` (each inward notch) — `2 * points` vertices total.
    /// Same "first vertex straight up" convention as [`Path::polygon`].
    pub fn star(points: u32, outer_radius: f32, inner_radius: f32) -> Self {
        let vertex_count = points * 2;
        let mut commands = Vec::with_capacity(vertex_count as usize + 1);
        for i in 0..vertex_count {
            let angle = -std::f32::consts::FRAC_PI_2 + (i as f32) * std::f32::consts::PI / (points as f32);
            let radius = if i % 2 == 0 { outer_radius } else { inner_radius };
            let point = Vec2::new(angle.cos() * radius, angle.sin() * radius);
            commands.push(if i == 0 { PathCommand::MoveTo(point) } else { PathCommand::LineTo(point) });
        }
        commands.push(PathCommand::Close);
        Self { commands }
    }

    /// Axis-aligned bounding box (`min`, `max`) of this path's *control
    /// polygon* — every `MoveTo`/`LineTo` point plus both control points and
    /// the endpoint of every `CubicTo` — in the path's own local coordinate
    /// space. A cubic Bézier curve can never extend beyond its control
    /// polygon's bounds, so this is a safe (occasionally slightly generous)
    /// over-approximation of the curve's true extent — exactly what click/
    /// hit-testing needs (`WasmEditor::pick_node_at`), not exact rendering
    /// bounds. Returns `(INFINITY, NEG_INFINITY)` for an empty path — no
    /// point ever falls inside that degenerate box, correctly making an
    /// empty shape unclickable rather than a special case callers must check.
    /// A point along the path's own length, parameterized `0.0` (start) to
    /// `1.0` (end) — motion-path animation's core primitive: placing a node
    /// exactly on this path's outline at a given fraction of its total
    /// length, giving constant-speed motion regardless of how many/how
    /// complex the individual commands are (unlike naively giving every
    /// command an equal share of `t`, which would speed up through short
    /// segments and crawl through long ones).
    ///
    /// Every `LineTo`/`CubicTo` contributes to total length; `MoveTo` only
    /// relocates the current point (starts a new subpath) and contributes
    /// none. `Close` is a *real* line segment back to the most recent
    /// `MoveTo`'s point (SVG/PDF's own definition of what closing a subpath
    /// means) — treating it as zero-length was an earlier bug here: any
    /// closed preset shape (`Path::rect`, `Path::ellipse`, anything the pen
    /// tool draws and closes) always ends in a real `Close`, so a guide
    /// built from one would silently stop just short of a full lap, landing
    /// on the last real anchor instead of tracing the whole outline back to
    /// the start — the "progress 1.0 doesn't cycle the whole shape" report
    /// this fixes. Multiple subpaths are walked back-to-back as one
    /// concatenated length, not independent `0..1` ranges each. A `CubicTo`'s
    /// own length is approximated by sampling it at `CURVE_LENGTH_SAMPLES`
    /// evenly-spaced parameter steps and summing the straight-line distances
    /// between them — a standard, cheap arc-length approximation, accurate
    /// enough for animation placement (as opposed to exact rendering).
    ///
    /// Clamps `t` to `0.0..=1.0`. Returns the path's first `MoveTo` point
    /// (or `Vec2::ZERO` if there isn't one) for a path with no measurable
    /// length at all.
    pub fn point_at_progress(&self, t: f32) -> Vec2 {
        const CURVE_LENGTH_SAMPLES: usize = 16;
        let t = t.clamp(0.0, 1.0);

        let mut current = Vec2::ZERO;
        let mut subpath_start = Vec2::ZERO;
        let mut segment_lengths: Vec<f32> = Vec::with_capacity(self.commands.len());
        let mut total_length = 0.0;
        for command in &self.commands {
            let length = match *command {
                PathCommand::MoveTo(p) => {
                    current = p;
                    subpath_start = p;
                    0.0
                }
                PathCommand::LineTo(p) => {
                    let length = current.distance(p);
                    current = p;
                    length
                }
                PathCommand::CubicTo { control1, control2, to } => {
                    let mut length = 0.0;
                    let mut previous = current;
                    for i in 1..=CURVE_LENGTH_SAMPLES {
                        let s = i as f32 / CURVE_LENGTH_SAMPLES as f32;
                        let sample = cubic_bezier_point(current, control1, control2, to, s);
                        length += previous.distance(sample);
                        previous = sample;
                    }
                    current = to;
                    length
                }
                PathCommand::Close => {
                    let length = current.distance(subpath_start);
                    current = subpath_start;
                    length
                }
            };
            segment_lengths.push(length);
            total_length += length;
        }

        if total_length <= 0.0001 {
            return self
                .commands
                .iter()
                .find_map(|c| match *c { PathCommand::MoveTo(p) => Some(p), _ => None })
                .unwrap_or(Vec2::ZERO);
        }

        let target_length = t * total_length;
        let mut accumulated = 0.0;
        current = Vec2::ZERO;
        subpath_start = Vec2::ZERO;
        for (command, &length) in self.commands.iter().zip(segment_lengths.iter()) {
            let segment_start = current;
            match *command {
                PathCommand::MoveTo(p) => {
                    current = p;
                    subpath_start = p;
                }
                PathCommand::LineTo(p) => {
                    if target_length <= accumulated + length || length <= 0.0001 {
                        let local_t = if length > 0.0001 { (target_length - accumulated) / length } else { 0.0 };
                        return segment_start.lerp(p, local_t.clamp(0.0, 1.0));
                    }
                    current = p;
                }
                PathCommand::CubicTo { control1, control2, to } => {
                    if target_length <= accumulated + length || length <= 0.0001 {
                        let local_t = if length > 0.0001 { (target_length - accumulated) / length } else { 0.0 };
                        return cubic_bezier_point(segment_start, control1, control2, to, local_t.clamp(0.0, 1.0));
                    }
                    current = to;
                }
                PathCommand::Close => {
                    if target_length <= accumulated + length || length <= 0.0001 {
                        let local_t = if length > 0.0001 { (target_length - accumulated) / length } else { 0.0 };
                        return segment_start.lerp(subpath_start, local_t.clamp(0.0, 1.0));
                    }
                    current = subpath_start;
                }
            }
            accumulated += length;
        }

        current
    }

    pub fn bounds(&self) -> (Vec2, Vec2) {
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        let mut expand = |p: Vec2| {
            min = min.min(p);
            max = max.max(p);
        };
        for command in &self.commands {
            match *command {
                PathCommand::MoveTo(p) | PathCommand::LineTo(p) => expand(p),
                PathCommand::CubicTo { control1, control2, to } => {
                    expand(control1);
                    expand(control2);
                    expand(to);
                }
                PathCommand::Close => {}
            }
        }
        (min, max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_has_four_corners_and_closes() {
        let path = Path::rect(100.0, 50.0);
        assert_eq!(path.commands.len(), 5);
        assert!(matches!(path.commands[0], PathCommand::MoveTo(_)));
        assert!(matches!(path.commands.last(), Some(PathCommand::Close)));
    }

    #[test]
    fn bounds_of_a_rect_matches_its_half_extents() {
        let (min, max) = Path::rect(100.0, 50.0).bounds();
        assert_eq!(min, Vec2::new(-50.0, -25.0));
        assert_eq!(max, Vec2::new(50.0, 25.0));
    }

    #[test]
    fn bounds_of_an_empty_path_never_contains_a_point() {
        let (min, max) = Path::new().bounds();
        assert!(min.x > max.x || min.y > max.y);
    }

    #[test]
    fn ellipse_is_closed_and_bounded_by_its_radii() {
        let path = Path::ellipse(100.0, 50.0);
        assert!(matches!(path.commands.last(), Some(PathCommand::Close)));
        let (_min, max) = path.bounds();
        // The control polygon extends slightly beyond the true ellipse (the
        // Bézier approximation's control points sit outside the curve), so
        // this checks "close to" the half-extents, not exact equality.
        assert!((max.x - 50.0).abs() < 1.0);
        assert!((max.y - 25.0).abs() < 1.0);
    }

    #[test]
    fn polygon_has_one_vertex_per_side_plus_a_close() {
        let path = Path::polygon(3, 50.0);
        assert_eq!(path.commands.len(), 4);
        assert!(matches!(path.commands[0], PathCommand::MoveTo(_)));
        assert!(matches!(path.commands.last(), Some(PathCommand::Close)));
    }

    #[test]
    fn polygons_first_vertex_points_straight_up() {
        let PathCommand::MoveTo(first) = Path::polygon(3, 50.0).commands[0] else { panic!("expected MoveTo") };
        assert!(first.x.abs() < 1e-4, "first vertex should have x ≈ 0, got {first:?}");
        assert!(first.y < 0.0, "first vertex should be above the origin (-y), got {first:?}");
    }

    #[test]
    fn star_alternates_outer_and_inner_radii() {
        let path = Path::star(5, 50.0, 20.0);
        // 5 points -> 10 vertices + one Close.
        assert_eq!(path.commands.len(), 11);
        let PathCommand::MoveTo(tip) = path.commands[0] else { panic!("expected MoveTo") };
        let PathCommand::LineTo(notch) = path.commands[1] else { panic!("expected LineTo") };
        assert!(tip.length() > notch.length());
    }

    #[test]
    fn point_at_progress_zero_and_one_land_on_the_endpoints() {
        let path = Path { commands: vec![PathCommand::MoveTo(Vec2::new(0.0, 0.0)), PathCommand::LineTo(Vec2::new(100.0, 0.0))] };
        assert_eq!(path.point_at_progress(0.0), Vec2::new(0.0, 0.0));
        assert_eq!(path.point_at_progress(1.0), Vec2::new(100.0, 0.0));
    }

    #[test]
    fn point_at_progress_is_constant_speed_across_segments_of_different_lengths() {
        // A short segment (length 10) then a long one (length 100) — equal
        // *time* per command would put the halfway point at the short
        // segment's own midpoint (5, 0); constant-speed-by-length instead
        // puts it partway through the long segment, near the actual
        // physical midpoint of the whole 110-length path.
        let path = Path {
            commands: vec![
                PathCommand::MoveTo(Vec2::new(0.0, 0.0)),
                PathCommand::LineTo(Vec2::new(10.0, 0.0)),
                PathCommand::LineTo(Vec2::new(110.0, 0.0)),
            ],
        };
        let halfway = path.point_at_progress(0.5);
        assert!((halfway.x - 55.0).abs() < 0.5, "expected x ~55 (physical midpoint), got {halfway:?}");
    }

    #[test]
    fn point_at_progress_on_an_empty_path_returns_zero() {
        assert_eq!(Path::new().point_at_progress(0.5), Vec2::ZERO);
    }

    #[test]
    fn point_at_progress_clamps_out_of_range_input() {
        let path = Path { commands: vec![PathCommand::MoveTo(Vec2::new(0.0, 0.0)), PathCommand::LineTo(Vec2::new(10.0, 0.0))] };
        assert_eq!(path.point_at_progress(-1.0), path.point_at_progress(0.0));
        assert_eq!(path.point_at_progress(2.0), path.point_at_progress(1.0));
    }

    #[test]
    fn point_at_progress_on_a_closed_square_completes_the_full_loop() {
        // Same shape `Path::rect` builds — closed via `Close`, the way every
        // preset shape (and anything the pen tool closes) ends. Regression
        // for the "Close treated as zero-length" bug: `t=1.0` used to land
        // on the last real anchor (the fourth corner) instead of completing
        // the loop back to the start.
        let path = Path::rect(10.0, 10.0);
        let start = Vec2::new(-5.0, -5.0);
        assert_eq!(path.point_at_progress(0.0), start);
        assert_eq!(path.point_at_progress(1.0), start, "t=1.0 must complete the full loop back to the start point");

        // A square's 4 sides are equal length, so t=0.9 sits 60% along the
        // closing edge (from the 4th corner (-5,5) back to the start
        // (-5,-5)): x pinned at -5 (the closing edge is vertical), y = -1 —
        // well past the corner (y=5), not still stuck there.
        let near_end = path.point_at_progress(0.9);
        assert!((near_end.x - (-5.0)).abs() < 0.01 && (near_end.y - (-1.0)).abs() < 0.01, "expected (-5, -1) partway along the closing edge, got {near_end:?}");
    }

    #[test]
    fn bounds_includes_cubic_control_points() {
        let path = Path {
            commands: vec![
                PathCommand::MoveTo(Vec2::new(0.0, 0.0)),
                PathCommand::CubicTo {
                    control1: Vec2::new(-20.0, 10.0),
                    control2: Vec2::new(20.0, 10.0),
                    to: Vec2::new(0.0, 20.0),
                },
            ],
        };
        let (min, max) = path.bounds();
        assert_eq!(min, Vec2::new(-20.0, 0.0));
        assert_eq!(max, Vec2::new(20.0, 20.0));
    }
}
