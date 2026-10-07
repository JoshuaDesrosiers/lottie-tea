//! `an1mbyt3-cli optimize`: a pre-encoding compaction pass over a
//! `Document`, run before `save_teab` — AN1MBYT3_BUILD_PLAN.md §7's
//! "strips unused nodes/keyframes... optimizes curves (reduces redundant
//! keyframes within tolerance)". This is the first slice of that: redundant
//! keyframe stripping. Geometry dedup, string tables, and unused-node
//! stripping are later increments on top of this, not part of it (same
//! "narrow first slice, not the whole compiler pipeline" scoping
//! `an1mbyt3-format`'s own module doc already established for the binary
//! encoding step).
//!
//! **Redundant keyframe stripping**: an interior keyframe is removed if
//! *both* of the following hold:
//! - its own easing, and its (already-kept) previous neighbor's easing, are
//!   both [`an1mbyt3_core::Easing::Linear`] — this pass never touches an
//!   eased curve's shape, only genuinely straight-line stretches, so it can
//!   never make an authored ease-in/ease-out look different;
//! - linearly interpolating between its kept-previous and next neighbor, at
//!   its own time, reproduces its value within `tolerance` ([`Lerp::distance`]).
//!
//! This is a local, single-point check (not a full curve-deviation
//! analysis over the whole span) — a deliberate, documented simplification:
//! it catches the common real-world case (an imported Lottie file, or
//! hand-authored data, emitting a keyframe every frame along an already-
//! straight stretch) without the complexity of integrating error over an
//! interval. Revisit if a real optimized file is later found to have
//! visibly different playback from its un-optimized source.

use an1mbyt3_core::{Animation, Document, Easing, Keyframe, Lerp, PropertyTrack, Track};

/// How many keyframes `optimize_document` removed, across every animation
/// on every artboard — printed by the CLI so a user can see the pass
/// actually did something (or didn't, if there was nothing redundant to remove).
#[derive(Debug, Default, Clone, Copy)]
pub struct OptimizeStats {
    pub keyframes_removed: usize,
}

/// Strips redundant keyframes from every track of every animation on every
/// artboard in `document`, in place. See this module's doc comment for the
/// exact redundancy rule.
pub fn optimize_document(document: &mut Document, tolerance: f32) -> OptimizeStats {
    let mut stats = OptimizeStats::default();

    // Collect ids first, then look each artboard up mutably by id — avoids
    // needing a new `artboards_mut()` iterator on `Document` just for this
    // one CLI-only caller (see this module's doc comment on staying a thin
    // pass over the existing document API, not growing core's surface for it).
    let artboard_ids: Vec<_> = document.artboards().map(|(id, _)| id).collect();
    for artboard_id in artboard_ids {
        let artboard = document.artboard_mut(artboard_id).expect("id just collected from this same document");
        for (_, animation) in artboard.animations_mut() {
            stats.keyframes_removed += optimize_animation(animation, tolerance);
        }
    }

    stats
}

fn optimize_animation(animation: &mut Animation, tolerance: f32) -> usize {
    let mut removed = 0;
    for (_, track) in &mut animation.tracks {
        removed += match track {
            PropertyTrack::Translation(t) => strip_redundant_keyframes(t, tolerance),
            PropertyTrack::Rotation(t) => strip_redundant_keyframes(t, tolerance),
            PropertyTrack::Scale(t) => strip_redundant_keyframes(t, tolerance),
            PropertyTrack::FillColor(t) => strip_redundant_keyframes(t, tolerance),
            PropertyTrack::Path(t) => strip_redundant_keyframes(t, tolerance),
            PropertyTrack::MotionPath { track: t, .. } => strip_redundant_keyframes(t, tolerance),
        };
    }
    removed
}

/// Strips redundant interior keyframes from one track in place — see this
/// module's doc comment for the exact rule. Returns how many were removed.
fn strip_redundant_keyframes<T: Lerp>(track: &mut Track<T>, tolerance: f32) -> usize {
    if track.keyframes.len() < 3 {
        return 0;
    }

    let original_len = track.keyframes.len();
    let mut kept: Vec<Keyframe<T>> = vec![track.keyframes[0].clone()];

    for i in 1..track.keyframes.len() - 1 {
        let candidate = track.keyframes[i].clone();
        let previous = kept.last().unwrap().clone();
        let next = track.keyframes[i + 1].clone();

        let span = next.time - previous.time;
        let t = if span > 0.0 { (candidate.time - previous.time) / span } else { 0.0 };
        let reconstructed = previous.value.clone().lerp(next.value, t);

        let is_redundant = previous.easing == Easing::Linear
            && candidate.easing == Easing::Linear
            && reconstructed.distance(candidate.value.clone()) <= tolerance;

        if !is_redundant {
            kept.push(candidate);
        }
    }

    kept.push(track.keyframes.last().unwrap().clone());
    let removed = original_len - kept.len();
    track.keyframes = kept;
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;
    use an1mbyt3_core::{Artboard, Node, NodeKind, Path, ShapeNode};

    #[test]
    fn strips_a_collinear_middle_keyframe_on_a_linear_track() {
        let mut track = Track::new(vec![
            Keyframe::new(0.0, 0.0, Easing::Linear),
            Keyframe::new(0.5, 5.0, Easing::Linear), // exactly on the line from (0,0) to (1,10)
            Keyframe::new(1.0, 10.0, Easing::Linear),
        ]);
        let removed = strip_redundant_keyframes(&mut track, 0.001);
        assert_eq!(removed, 1);
        assert_eq!(track.keyframes.len(), 2);
        assert_eq!(track.sample(0.5), 5.0);
    }

    #[test]
    fn keeps_a_keyframe_that_deviates_beyond_tolerance() {
        let mut track = Track::new(vec![
            Keyframe::new(0.0, 0.0, Easing::Linear),
            Keyframe::new(0.5, 8.0, Easing::Linear), // well off the (0,0)->(1,10) line
            Keyframe::new(1.0, 10.0, Easing::Linear),
        ]);
        let removed = strip_redundant_keyframes(&mut track, 0.001);
        assert_eq!(removed, 0);
        assert_eq!(track.keyframes.len(), 3);
    }

    #[test]
    fn never_strips_an_eased_keyframe_even_if_it_looks_reconstructible() {
        let ease = Easing::CubicBezier { x1: 0.42, y1: 0.0, x2: 0.58, y2: 1.0 };
        let mut track = Track::new(vec![
            Keyframe::new(0.0, 0.0, ease),
            Keyframe::new(0.5, 5.0, ease), // sits on the straight line, but this track is eased
            Keyframe::new(1.0, 10.0, ease),
        ]);
        let removed = strip_redundant_keyframes(&mut track, 0.001);
        assert_eq!(removed, 0, "an eased track's shape must never be altered by this pass");
    }

    #[test]
    fn optimize_document_walks_every_animation_on_every_artboard() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let artboard = document.artboard_mut(artboard_id).unwrap();
        let node_id = artboard.add_node(Node::new(
            "Square",
            None,
            NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
        ));

        let mut animation = Animation::new("Walk");
        animation.tracks.push((
            node_id,
            PropertyTrack::Translation(Track::new(vec![
                Keyframe::new(0.0, Vec2::ZERO, Easing::Linear),
                Keyframe::new(0.5, Vec2::new(50.0, 0.0), Easing::Linear),
                Keyframe::new(1.0, Vec2::new(100.0, 0.0), Easing::Linear),
            ])),
        ));
        artboard.add_animation(animation);

        let stats = optimize_document(&mut document, 0.001);
        assert_eq!(stats.keyframes_removed, 1);
    }
}
