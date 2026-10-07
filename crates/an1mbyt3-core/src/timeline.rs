//! Authored, multi-node, multi-property animation content — what the
//! editor's Timeline panel actually creates/edits/saves, and the literal
//! shape AN1MBYT3_BUILD_PLAN.md §4 describes: "Animations hold timelines →
//! tracks → keyframes → curves(Bézier/easing) → events, keyed by `NodeId` +
//! property path."
//!
//! Distinct from [`crate::AnimationClip`] (`animation.rs`): that type is a
//! single node's translation(+fill_color) clip, used internally to drive
//! Phase 6's state machines and every native example's demo animation.
//! [`Animation`] here is a *named, artboard-level* asset — potentially many
//! nodes, many properties per node, switchable via the editor's animation
//! dropdown, and the thing actually saved as authored keyframe content in a
//! `.tea`/`.teab` file. The two aren't yet unified (a `State` still embeds
//! an `AnimationClip` directly, not a reference to one of these) — §4's
//! vision of "state machines... referencing animations by ID" is a real gap
//! this doesn't close, left for when state-machine authoring itself is
//! revisited.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{artboard::Artboard, ids::NodeId, keyframe::Track, node::Node, node::NodeKind, path::Path, transform::Transform2D};

/// Which of a node's properties a [`PropertyTrack`]/keyframe drives. Every
/// variant corresponds to a field that already exists somewhere on `Node`/
/// `NodeKind` — this doesn't invent new animatable surface, it just lets any
/// of them be keyframed instead of only `Transform2D::translation` (Phase
/// 2's original, deliberately narrow scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PropertyKind {
    Translation,
    /// Radians, matching `Transform2D::rotation`'s own unit.
    Rotation,
    Scale,
    /// Only meaningful on `NodeKind::Shape`/`NodeKind::Text` — see
    /// [`PropertyTrack::apply_to`]'s panic for any other kind.
    FillColor,
    /// Only meaningful on `NodeKind::Shape` — see
    /// [`PropertyTrack::apply_to`]'s doc comment for why this degrades to a
    /// no-op rather than panicking on any other kind. See `Lerp for Path`
    /// (`keyframe.rs`) for what "smoothly animated" means for a path: a
    /// structural (command-count/kind) mismatch between two keyframes holds
    /// instead of morphing, since there's no way to correspond arbitrary
    /// points between two differently-shaped paths.
    Path,
    /// Progress (`0.0..=1.0`) along a *guide* node's own `Path`
    /// (`Path::point_at_progress`) — only meaningful on
    /// `PropertyTrack::MotionPath`, never constructed as a bare
    /// `PropertyKind` for another track kind. Kept as its own `PropertyKind`
    /// (rather than reusing `Translation`) so the timeline UI can tell "this
    /// track drives position via a guide path" apart from "this track drives
    /// position directly" — see `PropertyTrack::MotionPath`'s doc comment for
    /// why the write itself can't happen in `KeyframeValue::write_to`.
    MotionPath,
}

/// A keyframe's value, tagged with which property it belongs to — lets a
/// single `Command` (`AddKeyframe`) accept a value for *any* animatable
/// property without the command enum needing one variant per property.
///
/// Not `Copy` — `Path`'s `Vec<PathCommand>` rules that out for the enum as a
/// whole, so every other variant's usual copy-by-value now needs an
/// explicit `.clone()` where it's actually shared, not just moved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum KeyframeValue {
    Translation(Vec2),
    Rotation(f32),
    Scale(Vec2),
    FillColor([f32; 4]),
    Path(Path),
    /// Progress `0.0..=1.0` along a guide node's path — see
    /// `PropertyTrack::MotionPath`'s doc comment. `write_to` deliberately
    /// treats this as a no-op (see there) rather than writing anything
    /// itself; only `Animation::apply`'s dedicated `MotionPath` branch (which
    /// alone has the `Artboard` access needed to resolve the guide) ever
    /// turns this into an actual translation.
    MotionPath(f32),
}

impl KeyframeValue {
    pub fn property_kind(&self) -> PropertyKind {
        match self {
            KeyframeValue::Translation(_) => PropertyKind::Translation,
            KeyframeValue::Rotation(_) => PropertyKind::Rotation,
            KeyframeValue::Scale(_) => PropertyKind::Scale,
            KeyframeValue::FillColor(_) => PropertyKind::FillColor,
            KeyframeValue::Path(_) => PropertyKind::Path,
            KeyframeValue::MotionPath(_) => PropertyKind::MotionPath,
        }
    }

    /// `self - other`. Panics if the variants don't match — only ever called
    /// with two values already known to be the same property (see
    /// `Command::ShiftPropertyKeyframes`'s doc comment). `ShiftPropertyKeyframes`
    /// is only ever actually issued for `Translation`/`Rotation`/`Scale`/
    /// `FillColor` (nudging a node's whole animated motion when its rest
    /// pose moves) — `Path` never goes through it in practice, but panics
    /// here rather than silently no-opping like `write_to` does, matching
    /// every other variant's own "caller bug" convention, not the
    /// per-frame-playback-safety one `write_to`/`apply_to` need instead.
    pub fn subtract(self, other: Self) -> Self {
        match (self, other) {
            (KeyframeValue::Translation(a), KeyframeValue::Translation(b)) => KeyframeValue::Translation(a - b),
            (KeyframeValue::Rotation(a), KeyframeValue::Rotation(b)) => KeyframeValue::Rotation(a - b),
            (KeyframeValue::Scale(a), KeyframeValue::Scale(b)) => KeyframeValue::Scale(a - b),
            (KeyframeValue::FillColor(a), KeyframeValue::FillColor(b)) => {
                KeyframeValue::FillColor([a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]])
            }
            _ => panic!("KeyframeValue::subtract: mismatched variants"),
        }
    }

    pub fn negate(self) -> Self {
        match self {
            KeyframeValue::Translation(v) => KeyframeValue::Translation(-v),
            KeyframeValue::Rotation(v) => KeyframeValue::Rotation(-v),
            KeyframeValue::Scale(v) => KeyframeValue::Scale(-v),
            KeyframeValue::FillColor(v) => KeyframeValue::FillColor([-v[0], -v[1], -v[2], -v[3]]),
            KeyframeValue::Path(_) => panic!("KeyframeValue::negate: Path has no meaningful negation, see subtract's doc comment"),
            KeyframeValue::MotionPath(_) => panic!("KeyframeValue::negate: MotionPath never goes through ShiftPropertyKeyframes, see subtract's doc comment"),
        }
    }

    /// Writes this value into whichever field of `node` it belongs to —
    /// `PropertyTrack::apply_to`'s own per-variant write logic, factored out
    /// so a caller that already has a `KeyframeValue` in hand (e.g. a state
    /// machine transition's crossfaded/lerped result — see
    /// `an1mbyt3_runtime::StateMachine`) can apply it without re-deriving
    /// which `Node`/`NodeKind` field it maps to. Silently does nothing for
    /// `FillColor` against a node kind with no fill color — same "one
    /// mismatched track degrades harmlessly" reasoning `apply_to`'s own doc
    /// comment gives.
    pub fn write_to(self, node: &mut Node) {
        match self {
            KeyframeValue::Translation(v) => node.transform.translation = v,
            KeyframeValue::Rotation(v) => node.transform.rotation = v,
            KeyframeValue::Scale(v) => node.transform.scale = v,
            KeyframeValue::FillColor(v) => {
                if let Some(color) = node.fill_color_mut() {
                    *color = v;
                }
            }
            KeyframeValue::Path(v) => {
                if let NodeKind::Shape(shape) = &mut node.kind {
                    shape.path = v;
                }
            }
            // Deliberate no-op — resolving a guide's path into a world
            // translation needs `Artboard` access this method doesn't have.
            // Only reached if a `MotionPath` track is ever sampled through
            // the generic `apply_to`/`write_to` path instead of
            // `Animation::apply`'s dedicated branch; see this variant's doc
            // comment.
            KeyframeValue::MotionPath(_) => {}
        }
    }

    /// Linearly interpolates from `self` toward `other` by `t` (`0.0` =
    /// `self`, `1.0` = `other`, unclamped beyond that range like every other
    /// lerp in this codebase) — what a state machine transition's crossfade
    /// (`an1mbyt3_runtime::StateMachine`) blends the outgoing and incoming
    /// states' sampled poses with. `Rotation` takes the *shortest* angular
    /// path (wrapping the delta into `[-PI, PI]` before scaling by `t`)
    /// rather than a naive component lerp — interpolating raw radians
    /// straight across (e.g. ~170° toward ~-170°) would otherwise spin the
    /// long way around (~340°) instead of the ~20° hop actually wanted, the
    /// standard rotation-blending gotcha every animation-blending reference
    /// (Rive/Unity/Unreal included) calls out. Panics if the variants don't
    /// match — same convention as `subtract`.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        match (self, other) {
            (KeyframeValue::Translation(a), KeyframeValue::Translation(b)) => KeyframeValue::Translation(a.lerp(b, t)),
            (KeyframeValue::Rotation(a), KeyframeValue::Rotation(b)) => {
                let mut delta = (b - a) % std::f32::consts::TAU;
                if delta > std::f32::consts::PI {
                    delta -= std::f32::consts::TAU;
                } else if delta < -std::f32::consts::PI {
                    delta += std::f32::consts::TAU;
                }
                KeyframeValue::Rotation(a + delta * t)
            }
            (KeyframeValue::Scale(a), KeyframeValue::Scale(b)) => KeyframeValue::Scale(a.lerp(b, t)),
            (KeyframeValue::FillColor(a), KeyframeValue::FillColor(b)) => KeyframeValue::FillColor([
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
                a[3] + (b[3] - a[3]) * t,
            ]),
            (KeyframeValue::Path(a), KeyframeValue::Path(b)) => KeyframeValue::Path(crate::keyframe::Lerp::lerp(a, b, t)),
            (KeyframeValue::MotionPath(a), KeyframeValue::MotionPath(b)) => KeyframeValue::MotionPath(a + (b - a) * t),
            _ => panic!("KeyframeValue::lerp: mismatched variants"),
        }
    }
}

/// One property's keyframe track, tagged with which property it drives so
/// [`Animation::tracks`] can hold tracks of different value types (`Vec2`
/// for translation/scale, `f32` for rotation, `[f32; 4]` for fill color) in
/// one uniform collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PropertyTrack {
    Translation(Track<Vec2>),
    Rotation(Track<f32>),
    Scale(Track<Vec2>),
    FillColor(Track<[f32; 4]>),
    Path(Track<Path>),
    /// Drives a node's translation indirectly: the keyframed value is a
    /// scalar *progress* (`0.0..=1.0`) along `guide`'s own `Path`
    /// (`Path::point_at_progress`), not a raw `Vec2`. Resolving progress into
    /// an actual translation needs three pieces of `Artboard` state this
    /// variant can't reach on its own — the guide's `Path`, the guide's
    /// world transform (the guide may itself be moved/rotated/scaled
    /// elsewhere in the tree), and the *target* node's parent's world
    /// transform (to convert the resulting world point back into the
    /// target's own local space) — so unlike every other variant here,
    /// `PropertyTrack::apply_to`/`sample_value` cannot do the real work for
    /// this one; `Animation::apply` special-cases `MotionPath` in its own
    /// loop instead, where it already has `&mut Artboard` in hand. This
    /// variant's `sample_value`/`apply_to` still exist (returning/writing the
    /// raw progress via `KeyframeValue::MotionPath`) purely so every track
    /// kind keeps a uniform interface — real playback never relies on them
    /// for this variant.
    MotionPath { guide: NodeId, track: Track<f32> },
}

impl PropertyTrack {
    pub fn kind(&self) -> PropertyKind {
        match self {
            PropertyTrack::Translation(_) => PropertyKind::Translation,
            PropertyTrack::Rotation(_) => PropertyKind::Rotation,
            PropertyTrack::Scale(_) => PropertyKind::Scale,
            PropertyTrack::FillColor(_) => PropertyKind::FillColor,
            PropertyTrack::Path(_) => PropertyKind::Path,
            PropertyTrack::MotionPath { .. } => PropertyKind::MotionPath,
        }
    }

    pub fn duration(&self) -> f32 {
        match self {
            PropertyTrack::Translation(t) => t.duration(),
            PropertyTrack::Rotation(t) => t.duration(),
            PropertyTrack::Scale(t) => t.duration(),
            PropertyTrack::FillColor(t) => t.duration(),
            PropertyTrack::Path(t) => t.duration(),
            PropertyTrack::MotionPath { track, .. } => track.duration(),
        }
    }

    /// Samples this track at `time` and writes the result into `node`.
    /// Silently does nothing if this is a `FillColor` track and `node.kind`
    /// has no fill color to write into — every other variant applies to any
    /// node kind unconditionally, since every node has a `Transform2D`.
    /// Deliberately a no-op here rather than the "panic on caller bug"
    /// convention most of this codebase uses (`Command::apply`'s doc comment,
    /// `AnimationClip::apply`'s own prior behavior this replaced): this runs
    /// from `Animation::apply`, called every frame while an animation is
    /// actively *playing* (`TimelinePanel`'s rAF tick loop, via
    /// `WasmEditor::set_scrub_time`), not from a one-shot editor command — a
    /// panic here crosses the wasm boundary as a thrown exception from
    /// *inside* that tick loop's own `requestAnimationFrame` callback, which
    /// silently breaks the loop's own reschedule (nothing after the throw
    /// ever runs, including the `requestAnimationFrame(tick)` call that
    /// keeps it going) — from the user's side, that looks exactly like
    /// "pressed Play, the icon flipped, and then nothing ever happens
    /// again," with no visible error at all. A stale/mismatched track
    /// (e.g. a `FillColor` track left on a node whose `NodeKind::Group`
    /// only got there because it used to be a `Shape` and one was
    /// authored against it before that changed) should degrade to "this one
    /// track has no visible effect this frame," not "freeze the whole
    /// player."
    fn apply_to(&self, node: &mut Node, time: f32) {
        self.sample_value(time).write_to(node);
    }

    /// Samples this track at `time` and returns the tagged result, without
    /// writing it anywhere — `apply_to`'s "get the value" half, on its own,
    /// for a caller (a state machine transition's crossfade,
    /// `an1mbyt3_runtime::StateMachine`) that needs to combine two tracks'
    /// sampled values (`KeyframeValue::lerp`) before writing anything.
    pub fn sample_value(&self, time: f32) -> KeyframeValue {
        match self {
            PropertyTrack::Translation(t) => KeyframeValue::Translation(t.sample(time)),
            PropertyTrack::Rotation(t) => KeyframeValue::Rotation(t.sample(time)),
            PropertyTrack::Scale(t) => KeyframeValue::Scale(t.sample(time)),
            PropertyTrack::FillColor(t) => KeyframeValue::FillColor(t.sample(time)),
            PropertyTrack::Path(t) => KeyframeValue::Path(t.sample(time)),
            PropertyTrack::MotionPath { track, .. } => KeyframeValue::MotionPath(track.sample(time)),
        }
    }
}

/// Every new [`Animation`] starts with this fixed duration (seconds) — a
/// deliberately round, edit-it-if-you-need-something-else default, the same
/// role `Artboard::new`'s caller-supplied width/height play for a fresh
/// canvas.
pub const DEFAULT_ANIMATION_DURATION: f32 = 5.0;

/// A named, artboard-level animation: any number of (node, property) tracks,
/// within a fixed, authored time range. AN1MBYT3_BUILD_PLAN.md §4's "keyed
/// by NodeId + property path" — the `Vec<(NodeId, PropertyTrack)>` shape
/// lets the same node appear more than once (one entry per animated
/// property), and different nodes be keyframed within the same named
/// animation, matching how a real timeline (Rive, After Effects) drives many
/// layers/properties from one timeline. An artboard can hold many
/// `Animation`s (`Artboard::animations`), switchable via the editor's
/// animation dropdown — this is the "different animations" half of Phase 4's
/// timeline; [`Artboard::animations`]'s doc comment covers the rest.
///
/// `duration` is authored (`Command::SetAnimationDuration`), not derived from
/// keyframes — same "fixed range you resize deliberately, keyframes clamp to
/// fit inside it" convention every mainstream keyframe tool (After Effects,
/// Rive, Lottie itself) uses, rather than a range that silently grows every
/// time a keyframe gets dragged past the current end. [`Animation::content_extent`]
/// is the old derived value (the longest track's last keyframe time) kept
/// around for callers that want "how much of the authored range is actually
/// used" rather than the fixed range itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Animation {
    pub name: String,
    pub tracks: Vec<(NodeId, PropertyTrack)>,
    #[serde(default = "default_duration_for_deserialize")]
    pub duration: f32,
    /// Soft-delete flag — same reasoning as `Node::deleted` (`node.rs`):
    /// undo/redo needs a deleted animation's `AnimationId` to stay valid.
    #[serde(default)]
    pub deleted: bool,
}

fn default_duration_for_deserialize() -> f32 {
    DEFAULT_ANIMATION_DURATION
}

impl Animation {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), tracks: Vec::new(), duration: DEFAULT_ANIMATION_DURATION, deleted: false }
    }

    /// The longest track's last keyframe time — how much of `duration` is
    /// actually keyframed, not the authored range itself (see the struct's
    /// doc comment).
    pub fn content_extent(&self) -> f32 {
        self.tracks.iter().map(|(_, track)| track.duration()).fold(0.0, f32::max)
    }

    /// Samples every track at `time` and writes the results into `artboard`.
    /// Skips (rather than panicking on) a track whose node doesn't exist in
    /// `artboard` — same "a per-frame playback path degrades one track at a
    /// time instead of crashing the whole player" reasoning as
    /// `PropertyTrack::apply_to`'s own doc comment; nothing in this codebase
    /// actually removes a node from the arena for real (soft-delete only
    /// flips a flag, `Command::SetDeleted`'s doc comment), so this should
    /// never trigger in practice, but this is called every frame during
    /// active playback and the consequence of being wrong about that is
    /// severe enough to guard anyway.
    pub fn apply(&self, artboard: &mut Artboard, time: f32) {
        for (node_id, track) in &self.tracks {
            if let PropertyTrack::MotionPath { guide, track } = track {
                apply_motion_path(artboard, *node_id, *guide, track.sample(time));
                continue;
            }
            let Some(node) = artboard.node_mut(*node_id) else { continue };
            track.apply_to(node, time);
        }
    }

    /// Samples every track at `time` and returns the results as a lookup
    /// keyed by `(node_id, property_kind)`, instead of writing them
    /// anywhere — `apply`'s "compute, don't write yet" counterpart, for a
    /// caller (a state machine transition's crossfade,
    /// `an1mbyt3_runtime::StateMachine`) that needs to combine *two*
    /// animations' sampled poses (`KeyframeValue::lerp`) property-by-
    /// property before writing anything into the artboard.
    pub fn sample_pose(&self, time: f32) -> std::collections::HashMap<(NodeId, PropertyKind), KeyframeValue> {
        self.tracks.iter().map(|(node_id, track)| ((*node_id, track.kind()), track.sample_value(time))).collect()
    }
}

/// `PropertyTrack::MotionPath`'s real per-frame work: sample `guide`'s own
/// `Path` at `progress`, carry that local point through `guide`'s world
/// transform to get a world-space point, then carry *that* back down through
/// `target`'s parent's world transform to get the local translation `target`
/// itself needs — the same "world value in, parent-relative local value out"
/// shape as `an1mbyt3_runtime::graph`'s `global_translation_to_local`, reused
/// here for consistency. Silently does nothing if `target` doesn't exist or
/// `guide` doesn't exist/isn't a `Shape` (no path to sample) — same
/// per-frame-playback-safety convention as `PropertyTrack::apply_to`'s own
/// doc comment.
fn apply_motion_path(artboard: &mut Artboard, target: NodeId, guide: NodeId, progress: f32) {
    let Some(guide_node) = artboard.node(guide) else { return };
    let NodeKind::Shape(guide_shape) = &guide_node.kind else { return };
    let local_point = guide_shape.path.point_at_progress(progress);
    let guide_world = artboard.world_transform(guide);
    let world_point = guide_world.to_affine2().transform_point2(local_point);

    let Some(target_node) = artboard.node(target) else { return };
    let parent_world = match target_node.parent {
        Some(parent_id) => artboard.world_transform(parent_id),
        None => Transform2D::default(),
    };
    let target_world = artboard.world_transform(target);
    let desired_world = Transform2D { translation: world_point, ..target_world };
    let local_translation = parent_world.local_for(&desired_world).translation;

    if let Some(target_node) = artboard.node_mut(target) {
        target_node.transform.translation = local_translation;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        easing::Easing,
        keyframe::Keyframe,
        node::{GroupNode, ShapeNode},
        path::Path,
    };

    #[test]
    fn lerp_translation_scale_and_fill_color_are_plain_componentwise_lerps() {
        let a = KeyframeValue::Translation(Vec2::new(0.0, 0.0));
        let b = KeyframeValue::Translation(Vec2::new(10.0, -20.0));
        assert_eq!(a.lerp(b, 0.25), KeyframeValue::Translation(Vec2::new(2.5, -5.0)));

        let a = KeyframeValue::Scale(Vec2::new(1.0, 1.0));
        let b = KeyframeValue::Scale(Vec2::new(3.0, 5.0));
        assert_eq!(a.lerp(b, 0.5), KeyframeValue::Scale(Vec2::new(2.0, 3.0)));

        let a = KeyframeValue::FillColor([0.0, 0.0, 0.0, 1.0]);
        let b = KeyframeValue::FillColor([1.0, 1.0, 1.0, 0.0]);
        assert_eq!(a.lerp(b, 0.5), KeyframeValue::FillColor([0.5, 0.5, 0.5, 0.5]));
    }

    #[test]
    fn lerp_rotation_takes_the_shortest_angular_path_across_the_wraparound() {
        // ~170deg toward ~-170deg is a ~20deg hop the short way, not a
        // ~340deg spin the long way around if you lerp the raw radians
        // straight across — the standard rotation-blending gotcha.
        let a = KeyframeValue::Rotation(170.0_f32.to_radians());
        let b = KeyframeValue::Rotation((-170.0_f32).to_radians());
        let KeyframeValue::Rotation(halfway) = a.lerp(b, 0.5) else { unreachable!() };
        assert!((halfway.to_degrees() - 180.0).abs() < 1e-3, "expected ~180deg, got {}deg", halfway.to_degrees());

        // An ordinary, non-wrapping case still behaves like a plain lerp.
        let a = KeyframeValue::Rotation(0.0);
        let b = KeyframeValue::Rotation(90.0_f32.to_radians());
        let KeyframeValue::Rotation(quarter) = a.lerp(b, 0.5) else { unreachable!() };
        assert!((quarter.to_degrees() - 45.0).abs() < 1e-3);
    }

    fn shape_artboard() -> (Artboard, NodeId) {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node_id = artboard.add_node(Node::new(
            "Square",
            None,
            NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [0.0, 0.0, 0.0, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
        ));
        (artboard, node_id)
    }

    #[test]
    fn animation_apply_drives_multiple_properties_on_the_same_node() {
        let (mut artboard, node_id) = shape_artboard();

        let mut animation = Animation::new("Test");
        animation.tracks.push((
            node_id,
            PropertyTrack::Translation(Track::new(vec![
                Keyframe::new(0.0, Vec2::ZERO, Easing::Linear),
                Keyframe::new(1.0, Vec2::new(100.0, 0.0), Easing::Linear),
            ])),
        ));
        animation.tracks.push((
            node_id,
            PropertyTrack::Rotation(Track::new(vec![
                Keyframe::new(0.0, 0.0, Easing::Linear),
                Keyframe::new(1.0, 3.0, Easing::Linear),
            ])),
        ));
        animation.tracks.push((
            node_id,
            PropertyTrack::FillColor(Track::new(vec![
                Keyframe::new(0.0, [0.0, 0.0, 0.0, 1.0], Easing::Linear),
                Keyframe::new(1.0, [1.0, 0.0, 0.0, 1.0], Easing::Linear),
            ])),
        ));

        animation.apply(&mut artboard, 0.5);

        let node = artboard.node(node_id).unwrap();
        assert_eq!(node.transform.translation, Vec2::new(50.0, 0.0));
        assert_eq!(node.transform.rotation, 1.5);
        let NodeKind::Shape(shape) = &node.kind else { unreachable!() };
        assert_eq!(shape.fill_color, [0.5, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn new_animation_starts_at_the_default_duration() {
        assert_eq!(Animation::new("Test").duration, DEFAULT_ANIMATION_DURATION);
    }

    #[test]
    fn content_extent_is_the_longest_track() {
        let mut animation = Animation::new("Test");
        let node_id = NodeId::default();
        animation.tracks.push((node_id, PropertyTrack::Translation(Track::new(vec![Keyframe::new(2.0, Vec2::ZERO, Easing::Linear)]))));
        animation.tracks.push((node_id, PropertyTrack::Rotation(Track::new(vec![Keyframe::new(5.0, 0.0, Easing::Linear)]))));
        assert_eq!(animation.content_extent(), 5.0);
    }

    #[test]
    fn fill_color_track_against_a_node_kind_with_no_fill_color_is_a_harmless_no_op() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node_id = artboard.add_node(Node::new("Group", None, NodeKind::Group(GroupNode::default())));

        let mut animation = Animation::new("Test");
        animation.tracks.push((
            node_id,
            PropertyTrack::FillColor(Track::new(vec![Keyframe::new(0.0, [1.0, 0.0, 0.0, 1.0], Easing::Linear)])),
        ));

        // Must not panic — see `PropertyTrack::apply_to`'s doc comment for
        // why this degrades to "no visible effect" instead of crashing
        // whatever's actively playing the animation this frame.
        animation.apply(&mut artboard, 0.0);
        assert!(matches!(artboard.node(node_id).unwrap().kind, NodeKind::Group(_)), "the node itself must be untouched");
    }

    #[test]
    fn motion_path_track_drives_a_target_along_its_guides_own_path() {
        use crate::path::PathCommand;

        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        // A straight guide from (0,0) to (100,0) in its own local space,
        // offset 10 units to the right so `world_transform` actually matters.
        let guide_id = artboard.add_node(Node::new(
            "Guide",
            None,
            NodeKind::Shape(ShapeNode {
                path: Path { commands: vec![PathCommand::MoveTo(Vec2::ZERO), PathCommand::LineTo(Vec2::new(100.0, 0.0))] },
                fill_color: [0.0, 0.0, 0.0, 1.0],
                blend_mode: Default::default(),
                gradient: None,
                image: None,
                stroke: None,
            }),
        ));
        artboard.node_mut(guide_id).unwrap().transform.translation = Vec2::new(10.0, 0.0);

        let target_id = artboard.add_node(Node::new(
            "Target",
            None,
            NodeKind::Shape(ShapeNode { path: Path::rect(5.0, 5.0), fill_color: [0.0, 0.0, 0.0, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None }),
        ));

        let mut animation = Animation::new("Test");
        animation.tracks.push((
            target_id,
            PropertyTrack::MotionPath {
                guide: guide_id,
                track: Track::new(vec![Keyframe::new(0.0, 0.0, Easing::Linear), Keyframe::new(1.0, 1.0, Easing::Linear)]),
            },
        ));

        animation.apply(&mut artboard, 0.5);
        // Halfway along the guide's own 0..100 span, then shifted by the
        // guide's own +10 world offset.
        assert_eq!(artboard.node(target_id).unwrap().transform.translation, Vec2::new(60.0, 0.0));

        animation.apply(&mut artboard, 0.0);
        assert_eq!(artboard.node(target_id).unwrap().transform.translation, Vec2::new(10.0, 0.0));
    }
}
