//! Keyframes and tracks: a [`Track`] is an ordered list of [`Keyframe`]s for
//! one animatable property; sampling it at a point in time is how
//! AN1MBYT3_BUILD_PLAN.md §5's "traditional timeline animation with
//! keyframes, interpolation, Bézier/easing curves" is actually evaluated.
//!
//! Generic over the property's value type `T` (`f32` for a number parameter,
//! `glam::Vec2` for a position) via the [`Lerp`] trait, rather than one
//! hard-coded value type — Phase 2 only wires up `Vec2` (node translation,
//! via `animation.rs`), but the interpolation machinery itself doesn't care.
//!
//! Lives in `an1mbyt3-core`, not `an1mbyt3-runtime` — see `animation.rs`'s
//! module doc for why authored keyframe data is document content, evaluated
//! (not just modeled) alongside the rest of the document model.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::easing::Easing;
use crate::path::{Path, PathCommand};

/// A value type that can be linearly interpolated between two instances.
///
/// A hand-written trait rather than relying on the standard library, since
/// `f32::lerp` is not yet stable — see the module doc for why this stays
/// generic instead of hard-coding `f32`/`Vec2` into [`Track`] directly.
///
/// `Clone`, not `Copy` — `Path` (the Timeline's path-shape track) can't be
/// `Copy` (it owns a `Vec<PathCommand>`), so `Track::sample` clones out of
/// its keyframe slice instead of relying on an implicit bitwise copy; every
/// other implementor here (`f32`/`Vec2`/`[f32; 4]`) is `Copy` too, so
/// cloning them is exactly as cheap as the copy it replaces.
pub trait Lerp: Clone {
    fn lerp(self, other: Self, t: f32) -> Self;

    /// How far apart two values of this type are, as a single non-negative
    /// scalar — used by `an1mbyt3-cli`'s keyframe-stripping optimization
    /// pass (Phase 9) to decide whether a keyframe's value is close enough
    /// to what its neighbors would already reconstruct to be redundant.
    /// Not a mathematical distance metric requirement beyond "0 means equal,
    /// bigger means further apart" — each `T` picks whatever's natural for
    /// comparing against a caller-supplied tolerance in that type's own units.
    fn distance(self, other: Self) -> f32;
}

impl Lerp for f32 {
    fn lerp(self, other: Self, t: f32) -> Self {
        self + (other - self) * t
    }

    fn distance(self, other: Self) -> f32 {
        (self - other).abs()
    }
}

impl Lerp for Vec2 {
    fn lerp(self, other: Self, t: f32) -> Self {
        Vec2::lerp(self, other, t)
    }

    fn distance(self, other: Self) -> f32 {
        Vec2::distance(self, other)
    }
}

/// RGBA fill color, matching `an1mbyt3_core::ShapeNode::fill_color`'s own
/// `[f32; 4]` representation — plain componentwise interpolation, same as
/// every mainstream animation/design tool does for a solid color keyframe
/// (no perceptual color-space blending; revisit only if a designer actually
/// reports a color animation looking wrong through the RGB midpoint).
impl Lerp for [f32; 4] {
    fn lerp(self, other: Self, t: f32) -> Self {
        [
            self[0] + (other[0] - self[0]) * t,
            self[1] + (other[1] - self[1]) * t,
            self[2] + (other[2] - self[2]) * t,
            self[3] + (other[3] - self[3]) * t,
        ]
    }

    /// Max per-channel absolute difference, not Euclidean distance — a
    /// tolerance in "how far off is the worst channel" reads more naturally
    /// for a color (AN1MBYT3_BUILD_PLAN.md §9's keyframe-stripping caller
    /// picks one tolerance value across every property kind) than a
    /// combined 4D vector length would.
    fn distance(self, other: Self) -> f32 {
        (0..4).map(|i| (self[i] - other[i]).abs()).fold(0.0, f32::max)
    }
}

/// A path is only smoothly (point-by-point) interpolatable against another
/// path with the exact same command sequence shape (same length, same
/// command variant at each index) — e.g. two keyframes both built by
/// dragging the same shape's existing points, never adding/removing one.
/// That's the common "reshape this preset over time" case the Timeline's
/// path track exists for. A structural mismatch (a point added/removed, or
/// a command's *kind* changed at some index) can't be corresponded
/// point-for-point at all, so it holds instead — same "harmless degrade on
/// a mismatch the data model doesn't forbid" convention this codebase uses
/// everywhere else (an unknown graph-input name, a `FillColor` track on a
/// node with no fill) rather than a panic or a guess at retopologizing.
impl Lerp for Path {
    fn lerp(self, other: Self, t: f32) -> Self {
        if self.commands.len() != other.commands.len() {
            return if t < 1.0 { self } else { other };
        }
        let commands = self
            .commands
            .iter()
            .zip(other.commands.iter())
            .map(|(a, b)| match (*a, *b) {
                (PathCommand::MoveTo(p1), PathCommand::MoveTo(p2)) => PathCommand::MoveTo(p1.lerp(p2, t)),
                (PathCommand::LineTo(p1), PathCommand::LineTo(p2)) => PathCommand::LineTo(p1.lerp(p2, t)),
                (
                    PathCommand::CubicTo { control1: c1a, control2: c2a, to: toa },
                    PathCommand::CubicTo { control1: c1b, control2: c2b, to: tob },
                ) => PathCommand::CubicTo { control1: c1a.lerp(c1b, t), control2: c2a.lerp(c2b, t), to: toa.lerp(tob, t) },
                (PathCommand::Close, PathCommand::Close) => PathCommand::Close,
                // Same length, but a different command *kind* at this index
                // (e.g. a `LineTo` became a `CubicTo`) — just as structurally
                // incompatible as a length mismatch, so this one command
                // holds too, same reasoning as the whole-path fallback above.
                (a, _) => a,
            })
            .collect();
        Path { commands }
    }

    /// Max per-point distance across corresponding commands, or a large
    /// sentinel if the paths are structurally mismatched — never mistake "a
    /// point moved a little" for "the topology changed entirely" (the
    /// keyframe-stripping optimizer this exists for should never treat a
    /// structural change as a redundant, droppable keyframe).
    fn distance(self, other: Self) -> f32 {
        if self.commands.len() != other.commands.len() {
            return f32::MAX;
        }
        self.commands
            .iter()
            .zip(other.commands.iter())
            .map(|(a, b)| match (*a, *b) {
                (PathCommand::MoveTo(p1), PathCommand::MoveTo(p2)) => p1.distance(p2),
                (PathCommand::LineTo(p1), PathCommand::LineTo(p2)) => p1.distance(p2),
                (
                    PathCommand::CubicTo { control1: c1a, control2: c2a, to: toa },
                    PathCommand::CubicTo { control1: c1b, control2: c2b, to: tob },
                ) => c1a.distance(c1b).max(c2a.distance(c2b)).max(toa.distance(tob)),
                (PathCommand::Close, PathCommand::Close) => 0.0,
                _ => f32::MAX,
            })
            .fold(0.0, f32::max)
    }
}

/// One keyframe: a value at a point in time, plus the easing used to
/// interpolate *out of* this keyframe toward the next one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Keyframe<T> {
    /// Time in seconds from the start of the track.
    pub time: f32,
    pub value: T,
    pub easing: Easing,
}

impl<T> Keyframe<T> {
    pub fn new(time: f32, value: T, easing: Easing) -> Self {
        Self { time, value, easing }
    }
}

/// An ordered list of [`Keyframe`]s for one animatable property.
///
/// Keyframes are expected in ascending `time` order — `Track::sample` does a
/// linear scan (not a binary search) since tracks are short in practice;
/// revisit if/when tracks with hundreds of keyframes show up in profiling.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Track<T> {
    pub keyframes: Vec<Keyframe<T>>,
}

impl<T: Lerp> Track<T> {
    pub fn new(keyframes: Vec<Keyframe<T>>) -> Self {
        Self { keyframes }
    }

    /// The track's duration: the last keyframe's time, or `0.0` for an empty track.
    pub fn duration(&self) -> f32 {
        self.keyframes.last().map(|k| k.time).unwrap_or(0.0)
    }

    /// Samples the track's value at `time` (seconds). Clamps to the first/last
    /// keyframe's value outside the track's time range, rather than
    /// extrapolating — matches how every mainstream animation tool treats
    /// out-of-range time.
    pub fn sample(&self, time: f32) -> T {
        match self.keyframes.as_slice() {
            [] => panic!("Track::sample called on a track with no keyframes"),
            [only] => only.value.clone(),
            keyframes => {
                if time <= keyframes[0].time {
                    return keyframes[0].value.clone();
                }
                if time >= keyframes[keyframes.len() - 1].time {
                    return keyframes[keyframes.len() - 1].value.clone();
                }

                let segment_end_index = keyframes
                    .iter()
                    .position(|k| k.time > time)
                    .expect("time is within range, so some keyframe must be after it");
                let start = &keyframes[segment_end_index - 1];
                let end = &keyframes[segment_end_index];

                let segment_duration = end.time - start.time;
                let linear_t = if segment_duration > 0.0 {
                    (time - start.time) / segment_duration
                } else {
                    1.0
                };
                let eased_t = start.easing.ease(linear_t);

                start.value.clone().lerp(end.value.clone(), eased_t)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear_track() -> Track<f32> {
        Track::new(vec![
            Keyframe::new(0.0, 0.0, Easing::Linear),
            Keyframe::new(1.0, 10.0, Easing::Linear),
        ])
    }

    #[test]
    fn samples_at_keyframes_return_exact_values() {
        let track = linear_track();
        assert_eq!(track.sample(0.0), 0.0);
        assert_eq!(track.sample(1.0), 10.0);
    }

    #[test]
    fn samples_between_keyframes_interpolate_linearly() {
        let track = linear_track();
        assert_eq!(track.sample(0.5), 5.0);
    }

    #[test]
    fn samples_outside_range_clamp() {
        let track = linear_track();
        assert_eq!(track.sample(-1.0), 0.0);
        assert_eq!(track.sample(5.0), 10.0);
    }

    #[test]
    fn duration_is_last_keyframe_time() {
        assert_eq!(linear_track().duration(), 1.0);
    }

    #[test]
    fn path_lerp_with_matching_topology_interpolates_each_point() {
        let a = Path { commands: vec![PathCommand::MoveTo(Vec2::new(0.0, 0.0)), PathCommand::LineTo(Vec2::new(10.0, 0.0)), PathCommand::Close] };
        let b = Path { commands: vec![PathCommand::MoveTo(Vec2::new(0.0, 0.0)), PathCommand::LineTo(Vec2::new(10.0, 20.0)), PathCommand::Close] };
        let halfway = a.lerp(b, 0.5);
        assert_eq!(halfway.commands[0], PathCommand::MoveTo(Vec2::new(0.0, 0.0)));
        assert_eq!(halfway.commands[1], PathCommand::LineTo(Vec2::new(10.0, 10.0)));
        assert_eq!(halfway.commands[2], PathCommand::Close);
    }

    #[test]
    fn path_lerp_with_mismatched_topology_holds_instead_of_morphing() {
        let a = Path { commands: vec![PathCommand::MoveTo(Vec2::ZERO), PathCommand::Close] };
        let b = Path {
            commands: vec![PathCommand::MoveTo(Vec2::ZERO), PathCommand::LineTo(Vec2::new(10.0, 0.0)), PathCommand::Close],
        };
        assert_eq!(a.clone().lerp(b.clone(), 0.0), a);
        assert_eq!(a.clone().lerp(b.clone(), 0.99), a);
        assert_eq!(a.lerp(b.clone(), 1.0), b);
    }

    #[test]
    fn path_distance_is_max_sentinel_on_a_structural_mismatch() {
        let a = Path { commands: vec![PathCommand::MoveTo(Vec2::ZERO)] };
        let b = Path { commands: vec![PathCommand::MoveTo(Vec2::ZERO), PathCommand::Close] };
        assert_eq!(a.distance(b), f32::MAX);
    }
}
