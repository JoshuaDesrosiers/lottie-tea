//! Serializable, document-owned state machine *definitions*.
//!
//! Phase 6 first built state machines as pure runtime constructs
//! (`an1mbyt3_runtime::StateMachine`/`State`/`Transition`/`Condition`),
//! hand-assembled directly in Rust code (`build_demo_state_machine` and
//! similar) rather than saved as part of a `Document` — every native
//! example, the editor's State Machine preview, and the web runtime's demo
//! player all built the *same* hardcoded machine from scratch, and nothing
//! about a state machine's states/transitions/inputs could survive a
//! save/load round-trip, or be loaded by a browser player from an arbitrary
//! `.teab`. `timeline.rs`'s own module doc called this out directly: "§4's
//! vision of 'state machines... referencing animations by ID' is a real gap
//! ... left for when state-machine authoring itself is revisited." This
//! module is that revisit.
//!
//! The types here are the *authored*, saved half — `an1mbyt3_runtime::StateMachine`
//! is the *live* half, built from one of these via `StateMachine::from_definition`
//! (a `StateMachineDef` plus the `Artboard` it belongs to, since a state's
//! duration comes from the `Animation` it references). Same "authored data
//! vs. live runtime state" split every other engine concept here already
//! draws: `Animation` (authored tracks) vs. `Playback` (current time/speed),
//! `Node::transform` (authored/current pose) vs. `WasmEditor::scrub_time`
//! (session state). `Inputs` (the live bool/number/enum/trigger values a
//! state machine's transitions check) stays in `an1mbyt3-runtime` for the
//! same reason `Playback` does — inputs are set by the host application at
//! runtime, never authored/saved content.
//!
//! **A state's animation is a reference to an existing [`crate::Animation`]
//! (by [`crate::AnimationId`]), not a separate embedded clip.** This reuses
//! the exact same multi-node, multi-property animation model the editor's
//! Timeline panel already authors and saves, rather than inventing a
//! second, narrower "clip" concept alongside it — AN1MBYT3_BUILD_PLAN.md
//! §4's single-source-of-truth principle. (`AnimationClip` still exists and
//! is still useful on its own — a handful of native examples use it to
//! drive one property track with no state machine involved at all — it's
//! just no longer what a `StateDef` points at.)

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::ids::{AnimationId, NodeId};
use crate::program::ProgramDef;

/// What happens when a state's animation reaches the end of its authored
/// duration while that state is still active. Moved here from
/// `an1mbyt3-runtime` (alongside this module) for the same reason
/// `Easing`/`Keyframe`/`Track`/`AnimationClip` moved earlier: a `StateDef`
/// needs to reference this to be part of the saved `Document`, and
/// `an1mbyt3-core` cannot depend on `an1mbyt3-runtime` (dependency arrows
/// flow one way — AN1MBYT3_BUILD_PLAN.md §3). `an1mbyt3_runtime`
/// re-exports this under the same name, so `Playback::loop_mode`'s type and
/// every existing call site keeps compiling unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoopMode {
    /// Stop at the end; further `advance` calls have no effect until `seek`/`play`.
    Once,
    /// Wrap back to the start.
    Loop,
    /// Reverse direction at each end, alternating forward/backward.
    PingPong,
}

/// Which way a transition plays its target state's animation — the
/// generalized version of "author a separate mirror clip to play backward":
/// `Forward` starts at time `0.0`; `Reverse` starts at the target
/// animation's own authored `duration`, playing backward, so the *same*
/// animation a forward transition plays can also be played backward by a
/// different transition into the same state (or a different one), with no
/// second, hand-mirrored animation required. Moved here from
/// `an1mbyt3-runtime` for the same reason as `LoopMode` above.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PlaybackDirection {
    #[default]
    Forward,
    Reverse,
}

/// A condition guarding a [`TransitionDef`], checked against a state
/// machine's live `Inputs` at runtime (`an1mbyt3_runtime::Inputs` — the
/// input *values* are live session state and never saved; this is only the
/// authored *check* against a named input, e.g. "is `speed` greater than
/// 5?" — the same shape `an1mbyt3_runtime::state_machine`'s original
/// `Condition` enum had, moved here so it can be part of a saved `Document`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConditionDef {
    BoolIs { input: String, value: bool },
    NumberGreaterThan { input: String, threshold: f32 },
    NumberLessThan { input: String, threshold: f32 },
    EnumIs { input: String, value: String },
    Trigger { input: String },
}

/// The four input types a state machine's transitions can check —
/// mirrors `an1mbyt3_runtime::Inputs`'s four value maps (bool/number/
/// enum/trigger), just as a tag rather than a value, since `InputDef` only
/// *declares* an input exists and what kind it is, it doesn't hold a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputKind {
    Bool,
    Number,
    Enum,
    Trigger,
}

/// One authored, named input a state machine's transitions can reference —
/// declared explicitly (rather than only ever inferred from whichever
/// `ConditionDef`s happen to mention a name) so a host application (the
/// runtime's JS API, or the editor's State Machine panel) can enumerate a
/// state machine's real input surface — name plus type — without needing to
/// walk every transition on every state first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputDef {
    pub name: String,
    pub kind: InputKind,
}

/// One transition out of a [`StateDef`] — checked in authoring order (first
/// whose `condition` evaluates true wins), same convention
/// `an1mbyt3_runtime::StateMachine::advance` already established.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionDef {
    pub to: String,
    pub condition: ConditionDef,
    #[serde(default)]
    pub direction: PlaybackDirection,
    /// How long (seconds) this transition crossfades the outgoing state's
    /// pose into the incoming one, instead of switching instantly — the
    /// same "Duration" concept Rive/Unity/Unreal's own state-machine
    /// transitions all expose. `an1mbyt3_runtime::StateMachine` freezes the
    /// outgoing state's pose at the exact moment the transition fires
    /// (Rive's "Pause Source When Exiting", on) and linearly blends it
    /// (shortest-path for `Rotation`, see `KeyframeValue::lerp`) into the
    /// incoming state's own live-sampled pose over this window — a fixed
    /// design choice for this first pass, not a per-transition toggle;
    /// letting the outgoing state keep advancing underneath the blend
    /// instead (Rive's other option) is real, deferred future work if a rig
    /// ever needs it. `0.0` (instant, no crossfade) is both this field's
    /// `#[serde(default)]` — every `.tea`/`.teab` saved before this field
    /// existed already behaved exactly like an instant transition, so
    /// loading one as `0.0` changes nothing — and Rive's own default;
    /// `WasmEditor::add_transition`'s *authoring*-time default is different
    /// (the target state's own animation duration), a UI convenience for a
    /// brand-new transition, not this field's serde default.
    #[serde(default)]
    pub duration_seconds: f32,
    /// Rive's "Allow Exit During Transition" toggle: whether a *different*
    /// transition out of the target state is allowed to fire and interrupt
    /// *this* transition's own crossfade while it's still in progress.
    /// `true` (the default — see below) means fully interruptible: the
    /// instant some other transition's condition becomes true, this
    /// crossfade is abandoned and a new one starts from wherever this one
    /// had actually gotten to (the live composited pose at that instant —
    /// `an1mbyt3_runtime::Layer::composited_pose` — not a raw resample of
    /// either endpoint, which would visibly pop; see that function's own
    /// doc comment). `false` makes this crossfade run to completion first,
    /// same as Rive's own toggle turned off — no other transition out of
    /// the target state can fire until `duration_seconds` has fully
    /// elapsed. `#[serde(default = "default_interruptible")]` (`true`) for
    /// the usual back-compat reason: every `.tea`/`.teab` saved before this
    /// field existed already behaved as fully interruptible (there was no
    /// other option), so loading one as `true` changes nothing.
    #[serde(default = "default_interruptible")]
    pub interruptible: bool,
}

fn default_interruptible() -> bool {
    true
}

/// A continuous blend between two animations' extremes, driven by a live
/// number input instead of a playback clock — Unity's "Blend Tree"/
/// Unreal's "Blend Space" concept, one dimension only for this first pass
/// (two extremes, not an N-way blend graph). `StateDef::blend`, when
/// `Some`, overrides that state's `animation`/`loop_mode` entirely: the
/// state doesn't "play" at all — `an1mbyt3_runtime::StateMachine`
/// continuously re-samples wherever `input` currently sits, blending
/// `low`'s pose toward `high`'s pose (`crate::KeyframeValue::lerp`, the same
/// per-property blend a transition's own crossfade uses) instead of
/// advancing a `Playback` over time. `low`/`high` are each sampled at time
/// `0.0` — meant to be simple, static "extreme pose" animations (a single
/// keyframe each), not something with its own internal playback of its own.
///
/// This is the missing piece for "dynamic expression" rigs: a facial rig
/// authors two extreme poses (e.g. "mouth closed" / "mouth open") and maps
/// a `mouth_openness` number input across them, rather than needing a
/// discrete state (and a transition) per intermediate expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Blend1DDef {
    /// Which `Inputs` number this blend samples every frame.
    pub input: String,
    pub low: AnimationId,
    pub high: AnimationId,
    /// The `input` value corresponding to 100% `low` / 100% `high`
    /// respectively — lets an author map an arbitrary meaningful range
    /// (e.g. `-1.0..1.0`) rather than being locked to `0.0..1.0`. The blend
    /// weight is clamped to `[0.0, 1.0]` beyond this range (the input can
    /// still go further, it just doesn't extrapolate past either extreme).
    pub low_value: f32,
    pub high_value: f32,
}

/// One authored state: which [`crate::Animation`] plays while this state is
/// active (by [`AnimationId`] — see this module's doc comment for why
/// that's a reference, not an embedded clip), how it loops, and its
/// outgoing transitions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateDef {
    pub name: String,
    /// For a plain (non-blend) state, which animation plays. For a blend
    /// state (`blend: Some`), this is functionally unused by the runtime —
    /// see `Blend1DDef`'s own doc comment — but still required rather than
    /// made optional, to avoid the much larger, more invasive change of
    /// making every existing reader of this field handle its absence; the
    /// authoring UI just reuses `blend.low` here when creating a blend
    /// state, so it's never a meaningless/dangling reference in practice.
    pub animation: AnimationId,
    pub loop_mode: LoopMode,
    #[serde(default)]
    pub transitions: Vec<TransitionDef>,
    /// This state's box position on the State Machine panel's node-graph
    /// canvas (editor-authored layout, not anything the runtime reads —
    /// `an1mbyt3_runtime::StateMachine::from_definition` never looks at
    /// this field). Saved as part of the document anyway, same as any other
    /// authored value, so a rig's graph layout survives a save/load
    /// round-trip instead of re-scattering every time the file reopens.
    /// `#[serde(default)]` for the same back-compat reason every additive
    /// field here gets one — places new states at the origin, which the
    /// panel's "Add State" action immediately overrides with a cascaded
    /// position anyway (see `StateMachineGraph.tsx`).
    #[serde(default)]
    pub position: Vec2,
    /// `Some` turns this into a continuous blend state instead of a plain
    /// played-back one — see [`Blend1DDef`]'s own doc comment. `#[serde(default)]`
    /// (`None`) for the usual back-compat reason: every `.tea`/`.teab` saved
    /// before this field existed is a plain state, exactly as it always was.
    #[serde(default)]
    pub blend: Option<Blend1DDef>,
    /// `Some` turns this into a Programmable State: instead of sampling
    /// `animation` (or blending, if `blend` is also set — `program` wins
    /// over both when present, same "an alternate mode entirely replaces
    /// the plain-clip path" precedent `blend` itself already set), the
    /// runtime runs this program every frame — see [`ProgramDef`]'s own
    /// module doc for the two authoring modes and what each can/can't run
    /// under. `#[serde(default)]` (`None`) for the usual back-compat reason.
    #[serde(default)]
    pub program: Option<ProgramDef>,
}

/// One additional, node-masked layer within a [`StateMachineDef`] — Unity's
/// "Animator Layer"/Unreal's "Anim Layer" concept: an independent little
/// state machine (its own `states`/`initial_state`, checked against the
/// *same* shared `Inputs` every layer of one machine draws from) whose
/// resulting pose only ever writes to a specific, authored subset of nodes
/// (`mask`) instead of the whole artboard.
///
/// This is the missing piece for "independent rig parts driven by one
/// machine" (this feature's own motivating example: a facial rig's
/// eyebrows and mouth transitioning independently, but still able to
/// reference each other's state through a shared input, within one
/// authored machine) — the alternative, entirely separate `StateMachineDef`s
/// per rig part, would mean un-synchronized machines with no shared-input
/// capability at all.
///
/// Composited by `an1mbyt3_runtime::StateMachine::apply`: the machine's
/// own top-level `states` (the pre-existing, single-layer machine every
/// `.tea`/`.teab` saved before this struct existed already has) apply
/// first, unmasked, exactly as they always have — effectively "layer 0" —
/// then each `layers` entry applies in order, but only to `(node,
/// property)` pairs whose node is in that layer's own `mask`; a later
/// layer overrides an earlier one (or the base) wherever their masks
/// overlap, matching Unity's own top-to-bottom Animator layer compositing
/// (`Override` blend mode — `Additive` is real, deferred future work if a
/// rig ever needs it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateMachineLayerDef {
    pub name: String,
    pub states: Vec<StateDef>,
    pub initial_state: String,
    /// Which nodes this layer's pose is allowed to write to — a `(node,
    /// property)` pair this layer's active state would otherwise produce,
    /// but whose node isn't in this set, is simply never written. Empty by
    /// default (a freshly-added layer touches nothing until its mask is
    /// authored) rather than defaulting to "every node," so a designer
    /// can't accidentally have a brand-new layer silently fight the base
    /// layer for every node in the artboard.
    #[serde(default)]
    pub mask: Vec<NodeId>,
}

/// A whole authored state machine: its declared inputs, its states, and
/// which one is active immediately after load. Lives on `Artboard`
/// (`state_machines`), alongside `animations` — an artboard can have zero
/// or more, addressed by name (AN1MBYT3_BUILD_PLAN.md §8's
/// `animation.stateMachine("Character")`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateMachineDef {
    pub name: String,
    #[serde(default)]
    pub inputs: Vec<InputDef>,
    pub states: Vec<StateDef>,
    pub initial_state: String,
    /// Additional node-masked layers composited on top of `states` — see
    /// [`StateMachineLayerDef`]'s own doc comment for the full design.
    /// `#[serde(default)]` (empty): every `.tea`/`.teab` saved before this
    /// field existed is exactly today's single, unmasked machine, unchanged.
    #[serde(default)]
    pub layers: Vec<StateMachineLayerDef>,
    /// Soft-delete flag — same reasoning as `Animation::deleted`/`Node::deleted`:
    /// undo/redo needs a deleted state machine's `StateMachineId` to stay
    /// valid for any earlier command still on the stack. `#[serde(default)]`
    /// for the same back-compat reason every additive field in this
    /// workspace gets one — a `.tea`/`.teab` saved before this field existed
    /// still loads.
    #[serde(default)]
    pub deleted: bool,
}
