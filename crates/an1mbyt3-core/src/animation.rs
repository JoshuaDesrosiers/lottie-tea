//! Ties a [`Track`] to an actual node property, and applies a sampled value
//! back onto the document.
//!
//! Lives in `an1mbyt3-core`, not `an1mbyt3-runtime`, even though it
//! "evaluates" a track (samples + writes into a node) — unlike `Playback`
//! (runtime session state: current play position, loop mode, play/pause),
//! an `AnimationClip`'s keyframes are *authored content* a designer creates
//! and expects to persist in a saved `.tea`/`.teab` file (AN1MBYT3_BUILD_PLAN.md
//! §10 Phase 4's "keyframe edits" are only meaningfully undoable/savable if
//! `Document`/`Node` — both in this crate — can actually hold them). Nothing
//! here touches tessellation, rendering, or wall-clock time, so this doesn't
//! violate `an1mbyt3-core`'s "no rendering/animation-evaluation" boundary
//! any more than `Transform2D::compose` already does — it's pure data plus
//! pure interpolation math, operating only on this crate's own `Artboard`/`NodeKind`.
//!
//! Phase 2 wired up one property — a node's `Transform2D::translation` —
//! rather than a general property-path system (`Node(42).rotation`,
//! arbitrary parameter binding). AN1MBYT3_BUILD_PLAN.md §5 describes a full
//! parameter-binding layer; that's the Phase 6 state-machine work. Phase 10
//! adds a second, optional property (fill color, via `with_fill_color`) —
//! still not the general property-path system, just proof that `Keyframe<T>`/
//! `Track<T>`'s `Lerp`-based genericism (`keyframe.rs`'s module doc) extends
//! to more than one concrete property without changing how `Track`/`Keyframe`
//! themselves work.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{artboard::Artboard, ids::NodeId, keyframe::Track};

/// An animation clip targeting one node's translation, and optionally (via
/// [`AnimationClip::with_fill_color`]) its fill color too.
///
/// `node` is only meaningful within whichever `Artboard` it was created
/// against (AN1MBYT3_BUILD_PLAN.md §4's `NodeId` scoping) — `apply` takes
/// that `Artboard` explicitly rather than a whole `Document` so it can't
/// accidentally look up the ID in the wrong artboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationClip {
    pub node: NodeId,
    pub translation: Track<Vec2>,
    /// `None` unless [`AnimationClip::with_fill_color`] was used — most
    /// clips only animate position, and forcing every clip to carry an
    /// (often single-keyframe, unused) color track would be needless noise
    /// on every existing caller.
    pub fill_color: Option<Track<[f32; 4]>>,
}

impl AnimationClip {
    pub fn new(node: NodeId, translation: Track<Vec2>) -> Self {
        Self { node, translation, fill_color: None }
    }

    /// Opts this clip into also animating its target node's fill color —
    /// e.g. a state that should both slide into place *and* flash a
    /// different color, rather than position and color needing two separate
    /// clips/mechanisms. Builder-style (like `State::with_transition`)
    /// rather than a second constructor parameter, since most clips never
    /// need this.
    pub fn with_fill_color(mut self, fill_color: Track<[f32; 4]>) -> Self {
        self.fill_color = Some(fill_color);
        self
    }

    /// The longer of the two tracks' durations — a clip's own played length
    /// can't be shorter than either property it's authored to animate.
    pub fn duration(&self) -> f32 {
        let fill_color_duration = self.fill_color.as_ref().map(Track::duration).unwrap_or(0.0);
        self.translation.duration().max(fill_color_duration)
    }

    /// Samples this clip at `time` and writes the result(s) into the target
    /// node. Panics if `self.node` doesn't exist in `artboard`, or if a
    /// `fill_color` track is set but the target node is a kind with no fill
    /// color to animate (both are caller bugs — an animation clip built
    /// against the wrong node — not recoverable runtime conditions).
    pub fn apply(&self, artboard: &mut Artboard, time: f32) {
        let translation = self.translation.sample(time);
        let node = artboard
            .node_mut(self.node)
            .expect("AnimationClip::apply: target node not found in artboard");
        node.transform.translation = translation;

        if let Some(fill_color_track) = &self.fill_color {
            let sampled = fill_color_track.sample(time);
            match node.fill_color_mut() {
                Some(color) => *color = sampled,
                None => panic!("AnimationClip::apply: fill_color track set but target node has no fill color"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::easing::Easing;
    use crate::font_asset::FontAsset;
    use crate::keyframe::Keyframe;
    use crate::node::{GroupNode, Node, NodeKind, ShapeNode, TextNode};
    use crate::path::Path;

    #[test]
    fn apply_moves_the_target_node() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node_id = artboard.add_node(Node::new(
            "Square",
            None,
            NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
        ));

        let clip = AnimationClip::new(
            node_id,
            Track::new(vec![
                Keyframe::new(0.0, Vec2::new(-100.0, 0.0), Easing::Linear),
                Keyframe::new(1.0, Vec2::new(100.0, 0.0), Easing::Linear),
            ]),
        );

        clip.apply(&mut artboard, 0.5);
        assert_eq!(artboard.node(node_id).unwrap().transform.translation, Vec2::new(0.0, 0.0));
    }

    #[test]
    fn with_fill_color_animates_the_target_nodes_color() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node_id = artboard.add_node(Node::new(
            "Square",
            None,
            NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [0.0, 0.0, 0.0, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
        ));

        let clip = AnimationClip::new(
            node_id,
            Track::new(vec![Keyframe::new(0.0, Vec2::ZERO, Easing::Linear)]),
        )
        .with_fill_color(Track::new(vec![
            Keyframe::new(0.0, [0.0, 0.0, 0.0, 1.0], Easing::Linear),
            Keyframe::new(1.0, [1.0, 0.0, 0.0, 1.0], Easing::Linear),
        ]));

        clip.apply(&mut artboard, 0.5);
        let NodeKind::Shape(shape) = &artboard.node(node_id).unwrap().kind else {
            unreachable!("node was constructed as a Shape above")
        };
        assert_eq!(shape.fill_color, [0.5, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn with_fill_color_animates_a_text_nodes_color_too() {
        let mut document = crate::document::Document::new();
        let font_id = document.add_font(FontAsset { name: "Demo".into(), data: Vec::new() });

        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node_id = artboard.add_node(Node::new(
            "Label",
            None,
            NodeKind::Text(TextNode {
                content: "Hi".into(),
                font: font_id,
                size_px: 32.0,
                fill_color: [0.0, 0.0, 0.0, 1.0],
                gradient: None,
            }),
        ));

        let clip = AnimationClip::new(
            node_id,
            Track::new(vec![Keyframe::new(0.0, Vec2::ZERO, Easing::Linear)]),
        )
        .with_fill_color(Track::new(vec![
            Keyframe::new(0.0, [0.0, 0.0, 0.0, 1.0], Easing::Linear),
            Keyframe::new(1.0, [1.0, 0.0, 0.0, 1.0], Easing::Linear),
        ]));

        clip.apply(&mut artboard, 0.5);
        let NodeKind::Text(text) = &artboard.node(node_id).unwrap().kind else {
            unreachable!("node was constructed as a Text node above")
        };
        assert_eq!(text.fill_color, [0.5, 0.0, 0.0, 1.0]);
    }

    #[test]
    #[should_panic(expected = "target node has no fill color")]
    fn with_fill_color_panics_against_a_node_kind_with_no_fill_color() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node_id = artboard.add_node(Node::new("Group", None, NodeKind::Group(GroupNode::default())));

        let clip = AnimationClip::new(node_id, Track::new(vec![Keyframe::new(0.0, Vec2::ZERO, Easing::Linear)]))
            .with_fill_color(Track::new(vec![Keyframe::new(0.0, [0.0, 0.0, 0.0, 1.0], Easing::Linear)]));

        clip.apply(&mut artboard, 0.0);
    }
}
