//! Arc-length path operations: crops a path to a `[start, end]` fractional
//! range (`trim_path`/`trim_path_with_offset` — the "Trim Paths" effect from
//! Adobe After Effects/Lottie, and Rive's equivalent, AN1MBYT3_BUILD_PLAN.md
//! §6's "Vector path effects" note and §9's Lottie import priority #4), and
//! splits a path into a repeating on/off dash pattern (`dash_path` — SVG's
//! `stroke-dasharray`/`stroke-dashoffset` equivalent, §10's "stroke dashing").
//! Both native authoring and Lottie import ultimately need the same
//! arc-length primitives, just from different callers.
//!
//! Deliberately built on `lyon_algorithms::measure` rather than hand-rolling
//! arc-length walking: computing a cubic Bézier segment's arc length has no
//! closed-form solution (it's numerically approximated), and `lyon` already
//! implements that approximation correctly and efficiently — reimplementing
//! it here would just be a worse copy of code this crate already depends on.
//!
//! A trimmed/dashed path only makes sense to *stroke*, not fill — see
//! [`crate::tessellate::tessellate_stroke`]'s doc comment for why. This
//! module only produces the cropped/dashed [`Path`]; turning it into GPU
//! geometry is that function's job.

use glam::Vec2;
use lyon::algorithms::measure::{PathMeasurements, SampleType};
use lyon::path::{Event, Path as LyonPath};

use an1mbyt3_core::{Path, PathCommand};

use crate::tessellate::to_lyon_path;

/// Crops `path` to the arc-length range `start..end`, both fractions of the
/// total path length (`0.0` = the very start, `1.0` = the very end) — the
/// same two numbers After Effects' "Trim Paths" calls "Start"/"End" (its
/// third parameter, "Offset", is just `start`/`end` both shifted by a
/// constant before calling this, not a separate primitive).
///
/// `start > end` is not rejected — `lyon`'s sampler already treats
/// `start..end` as an ordinary `Range`, so passing `0.8..0.2` isn't a
/// documented "reverse" mode here, just whatever a plain reversed range does.
/// Deliberately not asserting `start`/`end` fall inside `0.0..=1.0`: `lyon`
/// clamps out-of-range distances to the path's own start/end, so an animated
/// "end" value briefly overshooting `1.0` due to easing overshoot degrades
/// gracefully (clamps to the full path) rather than panicking.
///
/// Returns an empty [`Path`] if `path` has no subpaths, matching
/// [`crate::tessellate::tessellate_path`]'s "empty in, empty out" contract
/// for the same input.
pub fn trim_path(path: &Path, start: f32, end: f32) -> Path {
    if path.commands.is_empty() {
        return Path::new();
    }

    let lyon_path = to_lyon_path(path);
    // Tolerance here only affects arc-length *measurement* accuracy (how
    // finely curved segments are approximated when computing where the
    // `start`/`end` cut points fall), not the output path's own geometric
    // fidelity — the cropped path keeps its original curve control points,
    // it's just cut at approximately the right point along them.
    let measurements = PathMeasurements::from_path(&lyon_path, 0.01);
    let mut sampler = measurements.create_sampler(&lyon_path, SampleType::Normalized);

    let mut builder = LyonPath::builder();
    sampler.split_range(start..end, &mut builder);
    let trimmed = builder.build();

    from_lyon_path(&trimmed)
}

/// [`trim_path`] plus After Effects' third "Trim Paths" parameter, "Offset" —
/// this module's own original doc comment already noted offset is "just
/// `start`/`end` both shifted by a constant before calling [`trim_path`]", so
/// that's exactly what this does, plus the wraparound that note glossed
/// over: shifting `start`/`end` by `offset` can push the visible range past
/// the path's end (or before its start), which should wrap around to the
/// other end of the path (a closed path has no "past the end" — arc-length
/// `1.0` and `0.0` are the same point) rather than just clamping/degenerating
/// the way passing an out-of-`0.0..=1.0` range straight to [`trim_path`]
/// would. When the shifted range wraps, this returns *two* trimmed
/// sub-paths concatenated into one [`Path`] (the tail-end piece, then the
/// head piece) — `tessellate_stroke` handles a multi-subpath `Path` natively
/// (each subpath strokes independently), so no special-casing is needed
/// downstream.
///
/// `offset` is a fraction of the path's total length, same unit as `start`/
/// `end` — an offset of `1.0` is a full lap (`wrapped_start == start`, no
/// visible change), matching how repeatedly nudging After Effects' Offset
/// slider cycles the visible segment around the path.
pub fn trim_path_with_offset(path: &Path, start: f32, end: f32, offset: f32) -> Path {
    let length = end - start;
    let wrapped_start = (start + offset).rem_euclid(1.0);
    let wrapped_end = wrapped_start + length;

    if wrapped_end <= 1.0 {
        trim_path(path, wrapped_start, wrapped_end)
    } else {
        let mut head = trim_path(path, wrapped_start, 1.0);
        let tail = trim_path(path, 0.0, wrapped_end - 1.0);
        head.commands.extend(tail.commands);
        head
    }
}

/// Splits `path` into a repeating dash/gap pattern along its arc length —
/// the same "give me the geometry between two arc-length points" primitive
/// [`trim_path`] uses, just applied repeatedly instead of once. Unlike
/// `trim_path`'s fractional `start`/`end`, `dash_length`/`gap_length` are in
/// `path`'s own local units (matching SVG's `stroke-dasharray` convention of
/// absolute lengths — a dash pattern's visual size shouldn't change just
/// because the same path got scaled up elsewhere in the scene graph).
/// `phase` shifts where the first dash begins (also in local units),
/// wrapping around the pattern's period the same way [`trim_path_with_offset`]
/// wraps around the path's own length.
///
/// Returns one [`Path`] with one subpath per visible dash segment (not a
/// `Vec<Path>`) — `tessellate_stroke` already strokes each of a `Path`'s
/// subpaths independently, so a single combined `Path` is all one
/// `tessellate_stroke` call needs to produce the whole dashed stroke's mesh
/// in one pass, rather than the caller stitching together N separate meshes.
///
/// Returns an empty `Path` for a degenerate pattern (`dash_length <= 0.0`) or
/// an empty/zero-length input path, matching [`trim_path`]'s "empty in,
/// empty out" contract.
pub fn dash_path(path: &Path, dash_length: f32, gap_length: f32, phase: f32) -> Path {
    if path.commands.is_empty() || dash_length <= 0.0 {
        return Path::new();
    }

    let lyon_path = to_lyon_path(path);
    let measurements = PathMeasurements::from_path(&lyon_path, 0.01);
    let total_length = measurements.length();
    if total_length <= 0.0 {
        return Path::new();
    }
    let mut sampler = measurements.create_sampler(&lyon_path, SampleType::Distance);

    let period = dash_length + gap_length.max(0.0);
    // Start walking from wherever the *previous* period boundary before 0.0
    // falls, so a `phase` that shifts the first dash to start before the
    // path's own beginning still shows that dash's visible tail rather than
    // skipping straight to the second one.
    let mut cursor = -(phase.rem_euclid(period.max(f32::EPSILON)));
    let mut commands = Vec::new();

    while cursor < total_length {
        let dash_start = cursor.max(0.0);
        let dash_end = (cursor + dash_length).min(total_length);
        if dash_end > dash_start {
            let mut builder = LyonPath::builder();
            sampler.split_range(dash_start..dash_end, &mut builder);
            commands.extend(from_lyon_path(&builder.build()).commands);
        }
        cursor += period;
    }

    Path { commands }
}

/// The inverse of `tessellate.rs`'s `to_lyon_path` — converts a `lyon_path::Path`
/// back into our own [`PathCommand`] list. Only ever called on a path built
/// from [`to_lyon_path`]'s own output (via `PathSampler::split_range`), so
/// every event is one `to_lyon_path` itself can emit (`Begin`/`Line`/`Cubic`/
/// `End`) — `Quadratic` is unreachable, not just unused, for that reason.
fn from_lyon_path(lyon_path: &LyonPath) -> Path {
    let mut commands = Vec::new();

    for event in lyon_path.iter() {
        match event {
            Event::Begin { at } => commands.push(PathCommand::MoveTo(Vec2::new(at.x, at.y))),
            Event::Line { to, .. } => commands.push(PathCommand::LineTo(Vec2::new(to.x, to.y))),
            Event::Cubic { ctrl1, ctrl2, to, .. } => commands.push(PathCommand::CubicTo {
                control1: Vec2::new(ctrl1.x, ctrl1.y),
                control2: Vec2::new(ctrl2.x, ctrl2.y),
                to: Vec2::new(to.x, to.y),
            }),
            Event::Quadratic { .. } => {
                unreachable!("to_lyon_path never emits quadratic segments, so split_range can't produce one either")
            }
            Event::End { close, .. } => {
                if close {
                    commands.push(PathCommand::Close);
                }
            }
        }
    }

    Path { commands }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_range_trim_preserves_the_shape() {
        // start=0.0, end=1.0 should crop nothing — the same rect back out,
        // just re-expressed through lyon's path builder.
        let original = Path::rect(100.0, 50.0);
        let trimmed = trim_path(&original, 0.0, 1.0);
        assert_eq!(trimmed.commands.len(), original.commands.len());
    }

    #[test]
    fn half_range_trim_produces_fewer_commands_than_the_full_path() {
        let original = Path::rect(100.0, 50.0);
        let trimmed = trim_path(&original, 0.0, 0.5);
        // Half the rect's perimeter is strictly less path than the whole
        // thing — and critically, no longer closed (a half-perimeter arc
        // isn't a closed shape), so it must have fewer commands overall.
        assert!(trimmed.commands.len() < original.commands.len());
    }

    #[test]
    fn zero_length_range_trims_to_an_empty_path() {
        let original = Path::rect(100.0, 50.0);
        let trimmed = trim_path(&original, 0.25, 0.25);
        assert!(trimmed.commands.is_empty());
    }

    #[test]
    fn empty_input_path_trims_to_an_empty_path() {
        let trimmed = trim_path(&Path::new(), 0.0, 1.0);
        assert!(trimmed.commands.is_empty());
    }

    #[test]
    fn zero_offset_matches_plain_trim_path() {
        let original = Path::rect(100.0, 50.0);
        let plain = trim_path(&original, 0.1, 0.4);
        let offset = trim_path_with_offset(&original, 0.1, 0.4, 0.0);
        assert_eq!(plain.commands.len(), offset.commands.len());
    }

    #[test]
    fn a_full_lap_offset_is_the_same_as_no_offset() {
        let original = Path::rect(100.0, 50.0);
        let plain = trim_path(&original, 0.1, 0.4);
        let full_lap = trim_path_with_offset(&original, 0.1, 0.4, 1.0);
        assert_eq!(plain.commands.len(), full_lap.commands.len());
    }

    #[test]
    fn an_offset_that_pushes_past_the_end_wraps_into_two_subpaths() {
        let original = Path::rect(100.0, 50.0);
        // start=0.0, end=0.2 (a length-0.2 window) shifted by +0.9:
        // wrapped_start = (0.0 + 0.9).rem_euclid(1.0) = 0.9,
        // wrapped_end = wrapped_start + length = 1.1 > 1.0 -> wraps,
        // splitting into [0.9, 1.0] + [0.0, 0.1].
        let wrapped = trim_path_with_offset(&original, 0.0, 0.2, 0.9);
        // Two subpaths concatenated means two MoveTo commands, not one.
        let move_to_count = wrapped.commands.iter().filter(|c| matches!(c, PathCommand::MoveTo(_))).count();
        assert_eq!(move_to_count, 2);
    }

    #[test]
    fn dash_path_produces_multiple_disjoint_segments() {
        let original = Path::rect(100.0, 50.0);
        // Perimeter is 300 units; a 20-on/10-off pattern should produce
        // several separate dash segments, not one continuous path.
        let dashed = dash_path(&original, 20.0, 10.0, 0.0);
        let move_to_count = dashed.commands.iter().filter(|c| matches!(c, PathCommand::MoveTo(_))).count();
        assert!(move_to_count > 1, "expected multiple dash segments, got {move_to_count}");
    }

    #[test]
    fn zero_gap_dash_path_is_one_continuous_segment() {
        let original = Path::rect(100.0, 50.0);
        // No gap at all means the "dashes" are contiguous — still produced as
        // repeated calls internally, but each abuts the next with no visible
        // break, which is a real, valid (if degenerate) dash pattern.
        let dashed = dash_path(&original, 300.0, 0.0, 0.0);
        assert!(!dashed.commands.is_empty());
    }

    #[test]
    fn non_positive_dash_length_produces_an_empty_path() {
        let original = Path::rect(100.0, 50.0);
        assert!(dash_path(&original, 0.0, 10.0, 0.0).commands.is_empty());
        assert!(dash_path(&original, -5.0, 10.0, 0.0).commands.is_empty());
    }

    #[test]
    fn dash_path_of_an_empty_input_is_empty() {
        assert!(dash_path(&Path::new(), 10.0, 5.0, 0.0).commands.is_empty());
    }

    #[test]
    fn phase_shifts_where_the_first_dash_starts() {
        let original = Path::rect(100.0, 50.0);
        let unshifted = dash_path(&original, 20.0, 10.0, 0.0);
        let shifted = dash_path(&original, 20.0, 10.0, 15.0);
        // The very *first* dash segment always starts at the path's own
        // arc-length-0 point for any `phase` strictly between 0 and the
        // period (it's clipped at the path's start, same reasoning
        // `dash_path`'s doc comment gives for walking from before 0.0) — so
        // comparing first-segment start points can't distinguish phases.
        // The *second* segment isn't clipped that way, and its start point
        // (arc-length `period - phase` vs. `period` here) genuinely differs,
        // which is what this checks instead.
        let second_move_to = |path: &Path| {
            path.commands
                .iter()
                .filter(|c| matches!(c, PathCommand::MoveTo(_)))
                .nth(1)
                .and_then(|c| match c {
                    PathCommand::MoveTo(p) => Some(*p),
                    _ => None,
                })
                .expect("expected at least two dash segments")
        };
        assert_ne!(second_move_to(&unshifted), second_move_to(&shifted));
    }
}
