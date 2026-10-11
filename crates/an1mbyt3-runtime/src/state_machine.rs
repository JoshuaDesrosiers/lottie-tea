//! State machines: named states (each referencing an [`Animation`] +
//! [`LoopMode`]), guarded transitions between them, and typed inputs —
//! AN1MBYT3_BUILD_PLAN.md §5's "State machines with boolean, number, enum,
//! and trigger inputs" and §10 Phase 6's exit criterion ("A state machine
//! with 2+ states and a trigger transitions correctly at runtime").
//!
//! Phase 8 split this into an *authored*, serializable half
//! (`an1mbyt3_core::StateMachineDef`/`StateDef`/`TransitionDef`/
//! `ConditionDef`) and this *live* half. Originally (Phase 6) a `State`
//! embedded an `AnimationClip` directly, hand-built in Rust code for every
//! demo/example/editor preview — nothing about a state machine's states,
//! transitions, or inputs could survive a save/load round-trip, and a
//! browser player could never load one from an arbitrary `.teab`.
//! [`StateMachine::from_definition`] is the fix: it builds a live instance
//! from a `StateMachineDef` plus the `Artboard` it belongs to, resolving
//! each state's referenced [`Animation`] (by [`AnimationId`]) once, up
//! front — the *same* multi-node, multi-property `Animation` the editor's
//! Timeline panel authors, not a separate embedded clip
//! (AN1MBYT3_BUILD_PLAN.md §4's single-source-of-truth principle;
//! `timeline.rs`'s own module doc flagged this exact gap before Phase 8
//! closed it). `Inputs` is unchanged by any of this — live, never-saved
//! session state, set by whatever's driving the state machine (a host
//! application, a native example, a JS caller), same as before.
//!
//! Phase 10 added [`an1mbyt3_core::PlaybackDirection`]/reversed
//! transitions — a transition can play its target state's animation
//! backward (from its authored `duration` down to `0.0`, via
//! `Playback::speed = -1.0`) instead of always forward from `0.0`. This is
//! what lets two states share one animation (one transition into a state
//! plays it forward, another plays the same animation backward) rather
//! than needing a hand-authored mirror animation to "undo" it.

use std::collections::{HashMap, HashSet};

use an1mbyt3_core::{Animation, AnimationId, Artboard, ConditionDef, KeyframeValue, LoopMode, NodeId, PlaybackDirection, ProgramDef, PropertyKind, StateDef, StateMachineDef};
use glam::Vec2;

use crate::block_interpreter::{self, BlockContext, BlockRuntime};
use crate::graph::{self, GraphContext};
use crate::keyboard::KeyboardState;
use crate::playback::Playback;

/// An in-progress transition crossfade — see `TransitionDef::duration_seconds`'s
/// doc comment (`an1mbyt3-core`) for the overall design this implements
/// (industry-standard terminology/behavior researched against Rive/Unity/
/// Unreal's own state-machine transitions before building this).
struct Blend {
    /// The outgoing state's pose, frozen at the exact instant the
    /// transition fired ("Pause Source When Exiting", in Rive's own
    /// terminology for this same choice) — keyed by `(node, property)` so
    /// it can be looked up against the incoming animation's own tracks
    /// property-by-property, exactly what `Animation::sample_pose` returns.
    /// Captured via `Layer::composited_pose` (the *actually-visible* pose
    /// at that instant — itself possibly still mid-crossfade from an
    /// earlier interruption), never a raw resample of either endpoint —
    /// see `Layer::transition_to`'s doc comment for why that distinction
    /// is exactly what keeps a rapid-fire interruption from visibly
    /// popping.
    frozen_pose: HashMap<(NodeId, PropertyKind), KeyframeValue>,
    elapsed: f32,
    duration: f32,
    /// Copied from the firing `TransitionDef::interruptible` — Rive's
    /// "Allow Exit During Transition." While `true` (the default), a
    /// *different* transition out of the target state can fire and
    /// abandon this crossfade early; `Layer::advance` simply skips
    /// checking for a new transition at all while `false` and this blend
    /// is still in progress.
    interruptible: bool,
}

/// The current value of every input a state machine's transitions can check.
///
/// Triggers are pulses, not persistent state (AN1MBYT3_BUILD_PLAN.md §5:
/// "trigger inputs" are "fire-once, auto-resets") — `fire_trigger` marks one
/// as fired, [`StateMachine::advance`] consumes fired triggers when checking
/// transitions and clears them at the end of that same call, so a trigger is
/// only ever "seen" as fired for exactly one `advance`.
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    bools: HashMap<String, bool>,
    numbers: HashMap<String, f32>,
    enums: HashMap<String, String>,
    fired_triggers: HashSet<String>,
}

impl Inputs {
    pub fn set_bool(&mut self, name: impl Into<String>, value: bool) {
        self.bools.insert(name.into(), value);
    }

    pub fn set_number(&mut self, name: impl Into<String>, value: f32) {
        self.numbers.insert(name.into(), value);
    }

    pub fn set_enum(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.enums.insert(name.into(), value.into());
    }

    pub fn fire_trigger(&mut self, name: impl Into<String>) {
        self.fired_triggers.insert(name.into());
    }

    pub fn bool(&self, name: &str) -> bool {
        *self.bools.get(name).unwrap_or(&false)
    }

    pub fn number(&self, name: &str) -> f32 {
        *self.numbers.get(name).unwrap_or(&0.0)
    }

    pub fn enum_value(&self, name: &str) -> Option<&str> {
        self.enums.get(name).map(String::as_str)
    }

    pub fn trigger_fired(&self, name: &str) -> bool {
        self.fired_triggers.contains(name)
    }

    fn clear_fired_triggers(&mut self) {
        self.fired_triggers.clear();
    }
}

/// Evaluates a [`ConditionDef`] against live [`Inputs`] — an extension
/// trait rather than an inherent method, since `ConditionDef` is defined in
/// `an1mbyt3-core` (which knows nothing about `Inputs`, a purely runtime,
/// never-saved concept). Rust's orphan rule allows implementing this
/// *foreign* type against a *local* trait; the reverse (a foreign trait) wouldn't be.
trait ConditionEval {
    fn evaluate(&self, inputs: &Inputs) -> bool;
}

impl ConditionEval for ConditionDef {
    fn evaluate(&self, inputs: &Inputs) -> bool {
        match self {
            ConditionDef::BoolIs { input, value } => inputs.bool(input) == *value,
            ConditionDef::NumberGreaterThan { input, threshold } => inputs.number(input) > *threshold,
            ConditionDef::NumberLessThan { input, threshold } => inputs.number(input) < *threshold,
            ConditionDef::EnumIs { input, value } => inputs.enum_value(input) == Some(value.as_str()),
            ConditionDef::Trigger { input } => inputs.trigger_fired(input),
        }
    }
}

/// One transition out of a [`ResolvedState`], resolved from a
/// [`an1mbyt3_core::TransitionDef`] at [`StateMachine::from_definition`]
/// time — just an owned copy of the same data, since a `TransitionDef`
/// carries no `Artboard`-dependent reference (only a `StateDef` does, via
/// `animation: AnimationId`) that needs resolving up front.
struct ResolvedTransition {
    to: String,
    condition: ConditionDef,
    direction: PlaybackDirection,
    /// Copied straight from `TransitionDef::duration_seconds` — see that
    /// field's own doc comment.
    duration: f32,
    /// Copied straight from `TransitionDef::interruptible` — see that
    /// field's own doc comment.
    interruptible: bool,
}

/// One resolved blend-tree state, resolved from a
/// [`an1mbyt3_core::Blend1DDef`] at `from_definition` time — just an owned
/// copy, same as `ResolvedTransition`; unlike a plain state's `animation`
/// (whose `duration` gets looked up once up front for `Playback::new`),
/// `low`/`high` are re-looked-up and re-sampled fresh on every `apply` call
/// instead, since they're meant to be simple, single-keyframe "extreme
/// pose" animations with no playback/duration semantics of their own worth
/// resolving once — see `Blend1DDef`'s own doc comment (`an1mbyt3-core`).
struct ResolvedBlend1D {
    input: String,
    low: AnimationId,
    high: AnimationId,
    low_value: f32,
    high_value: f32,
}

/// One state, resolved from a [`an1mbyt3_core::StateDef`]: the animation's
/// duration is looked up from the `Artboard` once at `from_definition` time
/// (rather than on every transition) since `Playback::new` needs a duration
/// up front and an `Animation`'s `duration` field never changes without an
/// explicit edit `StateMachine` has no way to observe anyway (a live
/// instance doesn't watch the document for changes — building a new one
/// after an edit is the caller's responsibility, same as any other
/// snapshot-at-construction-time runtime state in this workspace).
struct ResolvedState {
    name: String,
    animation: AnimationId,
    duration: f32,
    loop_mode: LoopMode,
    transitions: Vec<ResolvedTransition>,
    /// `Some` turns this into a continuous blend state — see
    /// `sample_current_state_pose`'s doc comment for how this changes
    /// `apply`'s behavior for this state.
    blend: Option<ResolvedBlend1D>,
    /// `Some` turns this into a Programmable State — see `crate::graph`'s
    /// module doc and `sample_current_state_pose`'s early-return for how
    /// this changes `apply`'s behavior for this state. Wins over `blend`
    /// when both happen to be set, same precedence `an1mbyt3_core::StateDef::program`'s
    /// own doc comment establishes.
    program: Option<ProgramDef>,
}

/// One independent little state machine layer: which state is active, its
/// own [`Playback`], and any in-progress transition crossfade. Every
/// [`StateMachine`] has exactly one base `Layer` (unmasked — built from
/// `StateMachineDef::states`/`initial_state`, exactly what every
/// single-layer machine already was before layers existed) plus zero or
/// more extra ones (`NamedLayer`, each with its own node `mask`) built from
/// `StateMachineDef::layers` — see `StateMachineLayerDef`'s own doc comment
/// (`an1mbyt3-core`) for the overall multi-layer design. `Layer` itself
/// knows nothing about masking; `StateMachine::apply` is the only thing
/// that applies a mask, since only it knows whether a given `Layer` is the
/// unmasked base or a restricted extra one.
struct Layer {
    states: Vec<ResolvedState>,
    current: usize,
    playback: Playback,
    /// `Some` for as long as a transition's crossfade is still in progress
    /// — see `Blend`'s own doc comment.
    blend: Option<Blend>,
    /// Live, resumable execution state for `states[current]`'s own
    /// `ProgramDef::Blocks` program, if it has one — rebuilt fresh
    /// (`BlockRuntime::fresh_for`) every time this layer (re)enters a state
    /// (`from_states`'s initial entry, and every `transition_to`), the same
    /// "a state's own live sub-state resets on entry" convention
    /// `playback`/`blend` already follow. An empty `BlockRuntime` (no
    /// scripts) for any state whose `program` isn't `Blocks` — see
    /// `block_runtime_for`.
    block_runtime: BlockRuntime,
    /// A dedicated, always-advancing wall-clock for `run_blocks`'s
    /// `BlockContext::elapsed_seconds` (what `Wait` schedules against) —
    /// deliberately *not* `self.playback.time`. `Playback::advance` is a
    /// no-op while paused or while `duration <= 0.0` (exactly what a
    /// Programmable State authored purely for game logic, with no actual
    /// animation clip, has), and wraps/reverses under `LoopMode::Loop`/
    /// `PingPong` — any of which would silently freeze or corrupt `Wait`'s
    /// wake-time comparison. This field instead just accumulates real
    /// `delta_seconds` unconditionally in `advance`, reset to `0.0` on the
    /// same state-(re)entry points `block_runtime` already resets on.
    blocks_elapsed: f32,
}

impl Layer {
    /// Resolves already-`ResolvedState`s and an `initial_state` name into a
    /// live `Layer` — shared by the base layer and every extra one, since
    /// both are built the exact same way once their states are resolved.
    ///
    /// Falls back to `states[0]` if `initial_state` doesn't name any state
    /// (rather than panicking) — a defensively-tolerant reading of a
    /// possibly-stale `initial_state` string (e.g. an older save, or any
    /// editor-side edit that didn't happen to update it) beats bricking the
    /// whole editor: a Rust panic crossing the wasm boundary traps the
    /// module, so every *subsequent* call — including the very "Set
    /// Initial" action a caller reaches for to fix it — would silently stop
    /// working for the rest of the session. This also gives the obvious
    /// "only one state" case its automatic behavior for free: with exactly
    /// one state, any mismatched `initial_state` still resolves to it.
    /// Panics only if `states` is itself empty, which every caller already
    /// guards against when it removes a state (a machine/layer always keeps
    /// at least one).
    fn from_states(states: Vec<ResolvedState>, initial_state: &str) -> Self {
        assert!(!states.is_empty(), "StateMachine::from_definition: a layer needs at least one state");
        let current = states.iter().position(|s| s.name == initial_state).unwrap_or(0);
        let mut playback = Playback::new(states[current].duration, states[current].loop_mode);
        playback.play();
        let block_runtime = block_runtime_for(&states[current]);
        Self { states, current, playback, blend: None, block_runtime, blocks_elapsed: 0.0 }
    }

    fn current_state_name(&self) -> &str {
        &self.states[self.current].name
    }

    fn state_names(&self) -> impl Iterator<Item = &str> {
        self.states.iter().map(|s| s.name.as_str())
    }

    fn available_triggers(&self) -> impl Iterator<Item = &str> {
        self.states[self.current].transitions.iter().filter_map(|t| match &t.condition {
            ConditionDef::Trigger { input } => Some(input.as_str()),
            _ => None,
        })
    }

    /// Checks this layer's current state's transitions against `inputs`
    /// (first matching condition wins), switches state if one fires, then
    /// advances playback (and any in-progress crossfade — see `Blend`)
    /// within whichever state is now active. `inputs` is owned by the
    /// enclosing `StateMachine` and shared by every layer — see
    /// `StateMachineLayerDef`'s doc comment for why a layer's transitions
    /// can check the same named inputs any other layer does.
    ///
    /// Skips checking for a new transition entirely while a non-interruptible
    /// blend is in progress (`Blend::interruptible: false` — Rive's "Allow
    /// Exit During Transition" turned off) — that crossfade runs to
    /// completion before anything else out of the target state can fire.
    fn advance(&mut self, delta_seconds: f32, inputs: &Inputs, artboard: &Artboard) {
        let locked = self.blend.as_ref().is_some_and(|blend| !blend.interruptible);
        if !locked {
            if let Some((target, direction, duration, interruptible)) = self.states[self.current]
                .transitions
                .iter()
                .find(|t| t.condition.evaluate(inputs))
                .map(|t| (t.to.clone(), t.direction, t.duration, t.interruptible))
            {
                self.transition_to(&target, direction, duration, interruptible, inputs, artboard);
            }
        }

        self.playback.advance(delta_seconds);
        self.blocks_elapsed += delta_seconds;
        if let Some(blend) = &mut self.blend {
            blend.elapsed += delta_seconds;
            if blend.elapsed >= blend.duration {
                self.blend = None;
            }
        }
    }

    /// Freezes the outgoing pose (`Blend::frozen_pose`, "Pause Source When
    /// Exiting" in Rive's own terminology for this exact choice) *before*
    /// switching `self.current`, then proceeds with the state switch/
    /// playback reset exactly as before this crossfade feature existed.
    /// Only actually builds a `Blend` if `duration > 0.0` — an instant
    /// transition (the pre-existing default, and every `.tea`/`.teab`
    /// saved before `TransitionDef::duration_seconds` existed) has nothing
    /// to blend, so `self.blend` is simply left/set to `None`.
    ///
    /// Captures the frozen pose via `self.composited_pose` — the actual,
    /// currently-*visible* pose — rather than a raw resample of whatever
    /// `self.states[self.current]`'s own animation happens to say at
    /// `self.playback.time`. This distinction only matters when this
    /// transition is itself *interrupting* an already-in-progress
    /// crossfade (`self.blend.is_some()` on entry): the visible pose at
    /// that instant is a blend of the *previous* frozen pose and the
    /// outgoing state's live pose, not either one alone — resampling just
    /// the outgoing state's raw animation would silently discard however
    /// far the previous crossfade had actually gotten, snapping to a
    /// different starting point than what was on screen and producing a
    /// visible "pop" on any quick, repeated interruption (Rive avoids this
    /// exact failure mode the same way: capture the live computed value,
    /// never a raw endpoint). `advance` already guards entry to this method
    /// against a non-interruptible in-progress `Blend` — see its own doc
    /// comment.
    fn transition_to(&mut self, name: &str, direction: PlaybackDirection, duration: f32, interruptible: bool, inputs: &Inputs, artboard: &Artboard) {
        self.blend = (duration > 0.0).then(|| Blend { frozen_pose: self.composited_pose(inputs, artboard), elapsed: 0.0, duration, interruptible });

        let index = self
            .states
            .iter()
            .position(|s| s.name == name)
            .unwrap_or_else(|| panic!("StateMachine: transition to unknown state `{name}`"));
        self.current = index;

        self.block_runtime = block_runtime_for(&self.states[index]);
        self.blocks_elapsed = 0.0;

        let duration = self.states[index].duration;
        let mut playback = Playback::new(duration, self.states[index].loop_mode);
        if direction == PlaybackDirection::Reverse {
            // Start at the end, moving backward — `Playback::advance`'s
            // `LoopMode::Once` branch already clamps and stops at `0.0` once
            // a negative-speed `Playback` reaches it, so this needs no
            // support from `Playback` itself beyond the `speed` field it
            // already has.
            playback.seek(duration);
            playback.speed = -1.0;
        }
        playback.play();
        self.playback = playback;
    }

    /// Computes this layer's current state's pose as a `(node, property) ->
    /// value` map, without writing it anywhere — `composited_pose`'s
    /// "figure out the incoming pose" half.
    ///
    /// A plain state just samples its own `Animation` at the current
    /// playback time (`Animation::sample_pose`), exactly as `apply` did
    /// before this helper existed. A blend state (`ResolvedState::blend`,
    /// `Some`) instead continuously re-samples its `low`/`high` animations
    /// at time `0.0` each (they're meant to be simple, single-keyframe
    /// "extreme pose" animations — see `Blend1DDef`'s own doc comment,
    /// `an1mbyt3-core`) and blends them (`KeyframeValue::lerp`, same
    /// per-property union-then-blend shape a transition's own crossfade
    /// uses) by a weight derived from the live `Inputs::number` value the
    /// blend's `input` names, mapped from `[low_value, high_value]` onto
    /// `[0.0, 1.0]` and clamped there (the input can go further, it just
    /// doesn't extrapolate past either extreme) — this ignores
    /// `self.playback` entirely, since a blend state doesn't "play".
    ///
    /// Returns `None` if a referenced animation no longer resolves (e.g.
    /// deleted after this `StateMachine` was built) — a plausible reachable
    /// state after an edit, not a programming bug, same reasoning this
    /// method's pre-refactor form already gave that exact lookup.
    fn sample_current_state_pose(&self, inputs: &Inputs, artboard: &Artboard) -> Option<HashMap<(NodeId, PropertyKind), KeyframeValue>> {
        let state = &self.states[self.current];

        // A Programmable State contributes nothing to the clip-sampled pose
        // map at all — `run_program` (below) writes its output straight
        // into the artboard instead, after this map has already been
        // applied, so `animation`/`blend` on a program state are pure
        // unused filler (same "required but ignored" precedent `blend`
        // itself already established for `animation`).
        if state.program.is_some() {
            return Some(HashMap::new());
        }

        // A blend with no input to drive it (a half-set-up Blend state) has
        // nothing to blend by — play the state's own animation instead of
        // sitting frozen on the low clip's first frame.
        let Some(blend) = state.blend.as_ref().filter(|b| !b.input.is_empty()) else {
            return Some(artboard.animation(state.animation)?.sample_pose(self.playback.time));
        };

        let low_pose = artboard.animation(blend.low)?.sample_pose(0.0);
        let high_pose = artboard.animation(blend.high)?.sample_pose(0.0);
        let weight = if blend.high_value != blend.low_value {
            ((inputs.number(&blend.input) - blend.low_value) / (blend.high_value - blend.low_value)).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let mut keys: HashSet<(NodeId, PropertyKind)> = low_pose.keys().copied().collect();
        keys.extend(high_pose.keys().copied());

        Some(
            keys.into_iter()
                .map(|key| {
                    let value = match (low_pose.get(&key), high_pose.get(&key)) {
                        (Some(low), Some(high)) => low.clone().lerp(high.clone(), weight),
                        (Some(low), None) => low.clone(),
                        (None, Some(high)) => high.clone(),
                        (None, None) => unreachable!("key came from the union of the two maps it was built from"),
                    };
                    (key, value)
                })
                .collect(),
        )
    }

    /// This layer's fully-composited pose (transition crossfade included)
    /// as a `(node, property) -> value` map — `StateMachine::apply`'s
    /// per-layer building block. While a transition crossfade is in
    /// progress (`self.blend`), blends the frozen outgoing pose into the
    /// freshly-sampled incoming one, property by property
    /// (`KeyframeValue::lerp`, linear — Rive's own transition
    /// "Interpolation" default, per the research behind this feature) — a
    /// `(node, property)` pair only one side actually animates just uses
    /// that side's value unblended (nothing to blend it *against* on the
    /// other side), same as how a mainstream engine's clip-blending treats
    /// an unkeyed property. An empty map (not an `Option`) stands in for
    /// "nothing to apply" (`sample_current_state_pose` returning `None`) —
    /// writing nothing and writing an empty map are behaviorally identical,
    /// and `StateMachine::apply` needs a plain map to union across layers.
    fn composited_pose(&self, inputs: &Inputs, artboard: &Artboard) -> HashMap<(NodeId, PropertyKind), KeyframeValue> {
        let incoming_pose = self.sample_current_state_pose(inputs, artboard).unwrap_or_default();

        let Some(blend) = &self.blend else { return incoming_pose };

        let weight = (blend.elapsed / blend.duration).clamp(0.0, 1.0);
        let mut keys: HashSet<(NodeId, PropertyKind)> = blend.frozen_pose.keys().copied().collect();
        keys.extend(incoming_pose.keys().copied());

        keys.into_iter()
            .map(|key| {
                let value = match (blend.frozen_pose.get(&key), incoming_pose.get(&key)) {
                    (Some(outgoing), Some(incoming)) => outgoing.clone().lerp(incoming.clone(), weight),
                    (Some(outgoing), None) => outgoing.clone(),
                    (None, Some(incoming)) => incoming.clone(),
                    (None, None) => unreachable!("key came from the union of the two maps it was built from"),
                };
                (key, value)
            })
            .collect()
    }

    /// Runs this layer's current state's program (if it has one, and it's a
    /// `Graph` — `Code` is a deliberate no-op here, see `an1mbyt3_core::program`'s
    /// module doc for why) directly against `artboard`, writing its
    /// `SetProperty` outputs straight onto the target nodes. Called from
    /// `StateMachine::apply`, after the clip-sampled pose map has already
    /// been written, so a graph's `GetProperty` sees the final composited
    /// values other layers/clips produced this frame.
    fn run_program(&self, artboard: &mut Artboard, mouse_artboard: Vec2, inputs: &Inputs) {
        let state = &self.states[self.current];
        let Some(ProgramDef::Graph(node_graph)) = &state.program else { return };
        let ctx = GraphContext { elapsed_seconds: self.playback.time, mouse_artboard, inputs };
        graph::evaluate_graph(node_graph, &ctx, artboard);
    }

    /// The `Blocks`-mode counterpart to `run_program` — needs `&mut self`
    /// (unlike `run_program`) since stepping a Blocks program advances its
    /// own persistent, resumable `block_runtime`, not just a stateless
    /// evaluation. Called from `StateMachine::apply`, same place/gate
    /// `run_program` is.
    fn run_blocks(&mut self, artboard: &mut Artboard, mouse_artboard: Vec2, inputs: &Inputs, keys: &KeyboardState) {
        let state = &self.states[self.current];
        let Some(ProgramDef::Blocks(block_program)) = &state.program else { return };
        let ctx = BlockContext { elapsed_seconds: self.blocks_elapsed, mouse_artboard, keys, inputs };
        block_interpreter::step(block_program, &mut self.block_runtime, &ctx, artboard);
    }
}

/// `BlockRuntime::fresh_for` when `state.program` is actually a `Blocks`
/// program, or a plain empty `BlockRuntime` (no scripts, `run_blocks` reads
/// `state.program` again and no-ops anyway) otherwise — the "reset a
/// state's own live sub-state on entry" helper `from_states`/`transition_to`
/// both call, factored out since both need the exact same logic.
fn block_runtime_for(state: &ResolvedState) -> BlockRuntime {
    match &state.program {
        Some(ProgramDef::Blocks(block_program)) => BlockRuntime::fresh_for(block_program),
        _ => BlockRuntime::default(),
    }
}

/// One extra, node-masked layer inside a live [`StateMachine`] — resolved
/// from a [`an1mbyt3_core::StateMachineLayerDef`]. `mask` is a `HashSet`
/// (not the authored `Vec`) purely for `O(1)` membership checks in
/// `StateMachine::apply`'s per-key compositing loop.
struct NamedLayer {
    name: String,
    mask: HashSet<NodeId>,
    layer: Layer,
}

/// A state machine: a base [`Layer`] (unmasked — every `.tea`/`.teab`
/// saved before layers existed is exactly this, unchanged) plus zero or
/// more extra, node-masked [`NamedLayer`]s composited on top of it, and the
/// [`Inputs`] every layer's transitions/blends are checked against. See
/// `StateMachineLayerDef`'s own doc comment (`an1mbyt3-core`) for the
/// overall design this implements.
pub struct StateMachine {
    base: Layer,
    layers: Vec<NamedLayer>,
    pub inputs: Inputs,
    /// Whether a Programmable State's `program` is actually allowed to run
    /// against `artboard` when `apply` is called — `false` by default, so
    /// dropping a Programmable State into an ordinary State Machine preview
    /// doesn't silently start running (possibly side-effecting, author-
    /// authored) code/graphs anywhere they weren't explicitly opted into.
    /// The desktop editor exposes this as a toggle on the State Machine
    /// panel, and always forces it `true` for its dedicated Playground tab
    /// — see `WasmEditor`'s own doc comment for the split.
    programs_enabled: bool,
    /// Live pointer position in artboard space, for a `MousePosition` graph
    /// source node — see `crate::graph::GraphContext::mouse_artboard`.
    mouse_artboard: Vec2,
    /// Live held-key state, for `Blocks` mode's "key [ ] pressed?" sensing
    /// reporter and "when key [ ] pressed" hat — the `Blocks` counterpart to
    /// `inputs`/`mouse_artboard` above (same "a host forwards real events
    /// in, gated at the read side" convention). Public for the same reason
    /// `inputs` is: a caller (the desktop editor's `WasmEditor`, a wasm
    /// player embed) forwards real keyboard events into it directly.
    pub keys: KeyboardState,
}

impl StateMachine {
    /// Resolves a `StateDef` slice (either `StateMachineDef::states` or one
    /// layer's own `states`) into `ResolvedState`s — the per-layer half of
    /// `from_definition`'s work, shared by the base layer and every extra one.
    fn resolve_states(states: &[StateDef], artboard: &Artboard) -> Vec<ResolvedState> {
        states
            .iter()
            .map(|state_def| {
                let animation: &Animation = artboard.animation(state_def.animation).unwrap_or_else(|| {
                    panic!(
                        "StateMachine::from_definition: state `{}` references an animation not found on this artboard",
                        state_def.name
                    )
                });
                ResolvedState {
                    name: state_def.name.clone(),
                    animation: state_def.animation,
                    duration: animation.duration,
                    loop_mode: state_def.loop_mode,
                    transitions: state_def
                        .transitions
                        .iter()
                        .map(|t| ResolvedTransition { to: t.to.clone(), condition: t.condition.clone(), direction: t.direction, duration: t.duration_seconds, interruptible: t.interruptible })
                        .collect(),
                    blend: state_def.blend.as_ref().map(|b| ResolvedBlend1D {
                        input: b.input.clone(),
                        low: b.low,
                        high: b.high,
                        low_value: b.low_value,
                        high_value: b.high_value,
                    }),
                    program: state_def.program.clone(),
                }
            })
            .collect()
    }

    /// Builds a live state machine from an authored `StateMachineDef` and
    /// the `Artboard` it belongs to (needed to look up each referenced
    /// `Animation`'s `duration` — see [`ResolvedState`]'s doc comment).
    ///
    /// Panics if `def.states` (or any entry in `def.layers`'s own `states`)
    /// is empty, no state is named the relevant `initial_state`, or a state
    /// references an `AnimationId` not present on `artboard` — all three
    /// are malformed-definition bugs (the definition should never have
    /// been saved/loaded in that shape), not recoverable runtime
    /// conditions, matching this module's other panicking conventions.
    pub fn from_definition(def: &StateMachineDef, artboard: &Artboard) -> Self {
        let base = Layer::from_states(Self::resolve_states(&def.states, artboard), &def.initial_state);
        let layers = def
            .layers
            .iter()
            .map(|layer_def| NamedLayer {
                name: layer_def.name.clone(),
                mask: layer_def.mask.iter().copied().collect(),
                layer: Layer::from_states(Self::resolve_states(&layer_def.states, artboard), &layer_def.initial_state),
            })
            .collect();

        Self { base, layers, inputs: Inputs::default(), programs_enabled: false, mouse_artboard: Vec2::ZERO, keys: KeyboardState::default() }
    }

    /// See `programs_enabled`'s own doc comment.
    pub fn set_programs_enabled(&mut self, enabled: bool) {
        self.programs_enabled = enabled;
    }

    pub fn programs_enabled(&self) -> bool {
        self.programs_enabled
    }

    /// See `mouse_artboard`'s own doc comment.
    pub fn set_mouse_position(&mut self, position: Vec2) {
        self.mouse_artboard = position;
    }

    /// The base layer's active state name — see this struct's own doc
    /// comment for why every pre-existing (single-layer) query method
    /// addresses the base layer specifically: it's the one every machine
    /// saved before layers existed already had, and every existing native/
    /// wasm caller already addresses "the" state machine this way.
    pub fn current_state_name(&self) -> &str {
        self.base.current_state_name()
    }

    /// Resumes advancing the base layer's current state's playback — a
    /// no-op if it's already playing. Does not itself change which state is
    /// active or where its playback sits; see `seek` for jumping to a
    /// specific time.
    pub fn play(&mut self) {
        self.base.playback.play();
    }

    /// Halts the base layer's current state's playback in place —
    /// `advance` still checks every layer's transitions every call (a
    /// paused machine can still react to an input and switch state), it
    /// just stops moving the base layer's `playback.time` forward until
    /// `play` is called again.
    pub fn pause(&mut self) {
        self.base.playback.pause();
    }

    pub fn is_playing(&self) -> bool {
        self.base.playback.playing
    }

    /// Jumps the base layer's current state's playback directly to `time`
    /// (clamped to that state's animation's own duration) — does not
    /// change `playing`, matching `Playback::seek`'s own convention (an
    /// editor timeline scrubbing while paused shouldn't start playback).
    pub fn seek(&mut self, time: f32) {
        self.base.playback.seek(time);
    }

    /// Every base-layer state's name, in definition order — lets a UI (an
    /// editor's state-machine preview panel, etc.) list all states, not
    /// just whichever one happens to be active right now.
    pub fn state_names(&self) -> impl Iterator<Item = &str> {
        self.base.state_names()
    }

    /// Trigger-input names the base layer's *current* state's own
    /// transitions check — exactly the triggers a UI could usefully offer
    /// as buttons right now, as opposed to every trigger name the whole
    /// machine references on some other, currently-inactive state.
    pub fn available_triggers(&self) -> impl Iterator<Item = &str> {
        self.base.available_triggers()
    }

    /// Every extra layer's own current state name, paired with that
    /// layer's own name — lets a UI (the State Machine panel's graph) show
    /// a live "this box is active" highlight for whichever layer is
    /// currently selected there, not just the base layer `current_state_name`
    /// already covers. Doesn't include the base layer itself (already
    /// available via `current_state_name`) — this is purely the *extra*
    /// layers' own live state.
    pub fn layer_active_states(&self) -> impl Iterator<Item = (&str, &str)> {
        self.layers.iter().map(|layer| (layer.name.as_str(), layer.layer.current_state_name()))
    }

    /// Advances the base layer and every extra layer identically — each
    /// checks its own current state's transitions against the *same*
    /// shared `self.inputs` (see `StateMachineLayerDef`'s doc comment for
    /// why layers share one input set) and advances its own playback/
    /// crossfade independently. Fired triggers are cleared once, after
    /// every layer has had a chance to see them — see [`Inputs`]'s doc
    /// comment for why.
    ///
    /// `artboard` is needed because a transition firing (on any layer)
    /// needs to sample that layer's outgoing state's animation at the exact
    /// instant it fires, to freeze into a crossfade (see `Layer::transition_to`'s
    /// doc comment). Every real caller already has an `&Artboard` in hand at
    /// the point it calls this (the same one `apply` needs moments later).
    pub fn advance(&mut self, delta_seconds: f32, artboard: &Artboard) {
        self.base.advance(delta_seconds, &self.inputs, artboard);
        for layer in &mut self.layers {
            layer.layer.advance(delta_seconds, &self.inputs, artboard);
        }
        self.inputs.clear_fired_triggers();
    }

    /// Writes the fully-composited pose into `artboard`: the base layer's
    /// pose first, unmasked (exactly what a single-layer machine already
    /// wrote before layers existed), then each extra layer's own
    /// fully-composited pose on top, restricted to that layer's own `mask`
    /// — a later layer overrides an earlier one (or the base) wherever
    /// their masks overlap, matching Unity's own top-to-bottom Animator
    /// layer compositing. See `StateMachineLayerDef`'s doc comment
    /// (`an1mbyt3-core`) for the full design.
    pub fn apply(&mut self, artboard: &mut Artboard) {
        let mut pose = self.base.composited_pose(&self.inputs, artboard);
        for layer in &self.layers {
            for (key, value) in layer.layer.composited_pose(&self.inputs, artboard) {
                if layer.mask.contains(&key.0) {
                    pose.insert(key, value);
                }
            }
        }

        for (key, value) in pose {
            let Some(node) = artboard.node_mut(key.0) else { continue };
            value.write_to(node);
        }

        if self.programs_enabled {
            self.base.run_program(artboard, self.mouse_artboard, &self.inputs);
            for layer in &self.layers {
                layer.layer.run_program(artboard, self.mouse_artboard, &self.inputs);
            }

            // `Blocks` mode's counterpart — needs `&mut self.base`/`&mut
            // layer.layer` (unlike `run_program`) since it advances each
            // layer's own resumable interpreter state; see `run_blocks`'s
            // own doc comment.
            self.base.run_blocks(artboard, self.mouse_artboard, &self.inputs, &self.keys);
            for layer in &mut self.layers {
                layer.layer.run_blocks(artboard, self.mouse_artboard, &self.inputs, &self.keys);
            }
            // Once every layer has had a chance to see this frame's edges —
            // same "consumed once per frame, by the same caller that drives
            // everything else" placement `clear_fired_triggers` already
            // uses for `Inputs`'s own per-frame reset.
            self.keys.end_frame();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;
    use an1mbyt3_core::{
        Blend1DDef, BlockProgramDef, BlockScript, Easing, EventKind, Expr, GraphEdgeDef, GraphNodeDef, GraphNodeKind, GraphProperty, Keyframe, Node, NodeGraphDef, NodeKind, NodeRef, Path,
        ProgramDef, PropertyTrack, ShapeNode, StateDef, StateMachineLayerDef, Statement, Track, TransitionDef,
    };

    /// A one-track (translation), single-keyframe `Animation` named after
    /// the state it's built for — the test-suite equivalent of the old
    /// `flat_clip` helper, now producing something that has to be added to
    /// an `Artboard` (via `add_animation`) rather than embedded directly in
    /// a `State`.
    fn flat_animation(name: &str, node: an1mbyt3_core::NodeId, x: f32) -> Animation {
        let mut animation = Animation::new(name);
        animation.duration = 0.0;
        animation.tracks.push((node, PropertyTrack::Translation(Track::new(vec![Keyframe::new(0.0, Vec2::new(x, 0.0), Easing::Linear)]))));
        animation
    }

    fn add_square(artboard: &mut Artboard) -> an1mbyt3_core::NodeId {
        artboard.add_node(Node::new("Square", None, NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None})))
    }

    fn two_state_machine(artboard: &mut Artboard, node: an1mbyt3_core::NodeId) -> StateMachine {
        let idle_animation = artboard.add_animation(flat_animation("Idle", node, 0.0));
        let active_animation = artboard.add_animation(flat_animation("Active", node, 100.0));

        let def = StateMachineDef {
            name: "Main".into(),
            inputs: Vec::new(),
            states: vec![
                StateDef {
                    name: "Idle".into(),
                    animation: idle_animation,
                    loop_mode: LoopMode::Once,
                    transitions: vec![TransitionDef {
                        to: "Active".into(),
                        condition: ConditionDef::BoolIs { input: "active".into(), value: true },
                        direction: PlaybackDirection::Forward,
                        duration_seconds: 0.0,
                        interruptible: true,
                    }],
                    position: glam::Vec2::ZERO,
                    blend: None,
                    program: None,
                },
                StateDef {
                    name: "Active".into(),
                    animation: active_animation,
                    loop_mode: LoopMode::Once,
                    transitions: vec![TransitionDef {
                        to: "Idle".into(),
                        condition: ConditionDef::BoolIs { input: "active".into(), value: false },
                        direction: PlaybackDirection::Forward,
                        duration_seconds: 0.0,
                        interruptible: true,
                    }],
                    position: glam::Vec2::ZERO,
                    blend: None,
                    program: None,
                },
            ],
            initial_state: "Idle".into(),
            layers: Vec::new(),
            deleted: false,
        };

        StateMachine::from_definition(&def, artboard)
    }

    #[test]
    fn starts_in_the_named_initial_state() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);
        let sm = two_state_machine(&mut artboard, node);
        assert_eq!(sm.current_state_name(), "Idle");
    }

    #[test]
    fn transitions_when_a_bool_condition_becomes_true() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);
        let mut sm = two_state_machine(&mut artboard, node);

        sm.inputs.set_bool("active", true);
        sm.advance(0.0, &artboard);

        assert_eq!(sm.current_state_name(), "Active");
        sm.apply(&mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(100.0, 0.0));
    }

    #[test]
    fn transitions_back_when_the_condition_reverses() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);
        let mut sm = two_state_machine(&mut artboard, node);

        sm.inputs.set_bool("active", true);
        sm.advance(0.0, &artboard);
        sm.inputs.set_bool("active", false);
        sm.advance(0.0, &artboard);

        assert_eq!(sm.current_state_name(), "Idle");
    }

    #[test]
    fn trigger_fires_once_and_then_auto_resets() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);

        let a_animation = artboard.add_animation(flat_animation("A", node, 0.0));
        let b_animation = artboard.add_animation(flat_animation("B", node, 50.0));
        let def = StateMachineDef {
            name: "Main".into(),
            inputs: Vec::new(),
            states: vec![
                StateDef {
                    name: "A".into(),
                    animation: a_animation,
                    loop_mode: LoopMode::Once,
                    transitions: vec![TransitionDef {
                        to: "B".into(),
                        condition: ConditionDef::Trigger { input: "toggle".into() },
                        direction: PlaybackDirection::Forward,
                        duration_seconds: 0.0,
                        interruptible: true,
                    }],
                    position: glam::Vec2::ZERO,
                    blend: None,
                    program: None,
                },
                StateDef {
                    name: "B".into(),
                    animation: b_animation,
                    loop_mode: LoopMode::Once,
                    transitions: vec![TransitionDef {
                        to: "A".into(),
                        condition: ConditionDef::Trigger { input: "toggle".into() },
                        direction: PlaybackDirection::Forward,
                        duration_seconds: 0.0,
                        interruptible: true,
                    }],
                    position: glam::Vec2::ZERO,
                    blend: None,
                    program: None,
                },
            ],
            initial_state: "A".into(),
            layers: Vec::new(),
            deleted: false,
        };
        let mut sm = StateMachine::from_definition(&def, &artboard);

        sm.inputs.fire_trigger("toggle");
        sm.advance(0.0, &artboard);
        assert_eq!(sm.current_state_name(), "B");

        // Trigger auto-reset after the previous `advance` — a second
        // `advance` with no new `fire_trigger` call must NOT transition again.
        sm.advance(0.0, &artboard);
        assert_eq!(sm.current_state_name(), "B");

        sm.inputs.fire_trigger("toggle");
        sm.advance(0.0, &artboard);
        assert_eq!(sm.current_state_name(), "A");
    }

    #[test]
    fn state_names_lists_every_state_regardless_of_which_is_active() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);
        let sm = two_state_machine(&mut artboard, node);
        assert_eq!(sm.state_names().collect::<Vec<_>>(), vec!["Idle", "Active"]);
    }

    #[test]
    fn available_triggers_only_lists_the_current_states_own_trigger_transitions() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);

        let a_animation = artboard.add_animation(flat_animation("A", node, 0.0));
        let b_animation = artboard.add_animation(flat_animation("B", node, 50.0));
        let def = StateMachineDef {
            name: "Main".into(),
            inputs: Vec::new(),
            states: vec![
                StateDef {
                    name: "A".into(),
                    animation: a_animation,
                    loop_mode: LoopMode::Once,
                    transitions: vec![TransitionDef {
                        to: "B".into(),
                        condition: ConditionDef::Trigger { input: "toggle".into() },
                        direction: PlaybackDirection::Forward,
                        duration_seconds: 0.0,
                        interruptible: true,
                    }],
                    position: glam::Vec2::ZERO,
                    blend: None,
                    program: None,
                },
                // "B" has no trigger transitions at all (a bool-based one
                // instead) — proves `available_triggers` reflects the
                // *current* state, not every trigger name anywhere in the
                // machine.
                StateDef {
                    name: "B".into(),
                    animation: b_animation,
                    loop_mode: LoopMode::Once,
                    transitions: vec![TransitionDef {
                        to: "A".into(),
                        condition: ConditionDef::BoolIs { input: "back".into(), value: true },
                        direction: PlaybackDirection::Forward,
                        duration_seconds: 0.0,
                        interruptible: true,
                    }],
                    position: glam::Vec2::ZERO,
                    blend: None,
                    program: None,
                },
            ],
            initial_state: "A".into(),
            layers: Vec::new(),
            deleted: false,
        };
        let mut sm = StateMachine::from_definition(&def, &artboard);

        assert_eq!(sm.available_triggers().collect::<Vec<_>>(), vec!["toggle"]);

        sm.inputs.fire_trigger("toggle");
        sm.advance(0.0, &artboard);
        assert_eq!(sm.current_state_name(), "B");
        assert_eq!(sm.available_triggers().collect::<Vec<_>>(), Vec::<&str>::new());
    }

    #[test]
    fn reversed_transition_starts_playback_at_the_end_moving_backward() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);

        // Both states reference the exact same animation — the whole point
        // of a reversed transition is not needing a second, hand-mirrored one.
        let mut shared_animation = Animation::new("Shared");
        shared_animation.duration = 1.0;
        shared_animation.tracks.push((
            node,
            PropertyTrack::Translation(Track::new(vec![
                Keyframe::new(0.0, Vec2::new(0.0, 0.0), Easing::Linear),
                Keyframe::new(1.0, Vec2::new(100.0, 0.0), Easing::Linear),
            ])),
        ));
        let shared_animation_id = artboard.add_animation(shared_animation);

        let def = StateMachineDef {
            name: "Main".into(),
            inputs: Vec::new(),
            states: vec![
                StateDef {
                    name: "A".into(),
                    animation: shared_animation_id,
                    loop_mode: LoopMode::Once,
                    transitions: vec![TransitionDef {
                        to: "B".into(),
                        condition: ConditionDef::Trigger { input: "toggle".into() },
                        direction: PlaybackDirection::Forward,
                        duration_seconds: 0.0,
                        interruptible: true,
                    }],
                    position: glam::Vec2::ZERO,
                    blend: None,
                    program: None,
                },
                StateDef {
                    name: "B".into(),
                    animation: shared_animation_id,
                    loop_mode: LoopMode::Once,
                    transitions: vec![TransitionDef {
                        to: "A".into(),
                        condition: ConditionDef::Trigger { input: "toggle".into() },
                        direction: PlaybackDirection::Reverse,
                        duration_seconds: 0.0,
                        interruptible: true,
                    }],
                    position: glam::Vec2::ZERO,
                    blend: None,
                    program: None,
                },
            ],
            initial_state: "A".into(),
            layers: Vec::new(),
            deleted: false,
        };
        let mut sm = StateMachine::from_definition(&def, &artboard);

        sm.inputs.fire_trigger("toggle");
        sm.advance(0.0, &artboard);
        assert_eq!(sm.current_state_name(), "B");
        sm.apply(&mut artboard);
        // Forward transition into B starts at its animation's own time 0.
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(0.0, 0.0));

        sm.inputs.fire_trigger("toggle");
        sm.advance(0.0, &artboard);
        assert_eq!(sm.current_state_name(), "A");
        sm.apply(&mut artboard);
        // Reversed transition into A starts at the animation's *end* instead.
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(100.0, 0.0));

        // Advancing plays backward toward 0, not forward toward the
        // animation's duration — proof `speed` actually flipped, not just the seek.
        sm.advance(0.5, &artboard);
        sm.apply(&mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(50.0, 0.0));
    }

    fn two_flat_state_machine(artboard: &mut Artboard, node: an1mbyt3_core::NodeId, transition_duration: f32) -> StateMachine {
        let a_animation = artboard.add_animation(flat_animation("A", node, 0.0));
        let b_animation = artboard.add_animation(flat_animation("B", node, 100.0));
        let def = StateMachineDef {
            name: "Main".into(),
            inputs: Vec::new(),
            states: vec![
                StateDef {
                    name: "A".into(),
                    animation: a_animation,
                    loop_mode: LoopMode::Once,
                    transitions: vec![TransitionDef {
                        to: "B".into(),
                        condition: ConditionDef::Trigger { input: "toggle".into() },
                        direction: PlaybackDirection::Forward,
                        duration_seconds: transition_duration,
                        interruptible: true,
                    }],
                    position: glam::Vec2::ZERO,
                    blend: None,
                    program: None,
                },
                StateDef { name: "B".into(), animation: b_animation, loop_mode: LoopMode::Once, transitions: vec![], position: glam::Vec2::ZERO, blend: None, program: None },
            ],
            initial_state: "A".into(),
            layers: Vec::new(),
            deleted: false,
        };
        StateMachine::from_definition(&def, artboard)
    }

    #[test]
    fn a_transition_with_zero_duration_cuts_instantly_with_no_blend() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);
        let mut sm = two_flat_state_machine(&mut artboard, node, 0.0);

        sm.inputs.fire_trigger("toggle");
        sm.advance(0.0, &artboard);
        sm.apply(&mut artboard);
        // No crossfade at all — straight to B's value, matching every
        // pre-existing (pre-`duration_seconds`) transition's behavior.
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(100.0, 0.0));
    }

    #[test]
    fn a_transition_with_nonzero_duration_crossfades_linearly_then_settles_on_the_incoming_state() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);
        let mut sm = two_flat_state_machine(&mut artboard, node, 1.0);

        sm.inputs.fire_trigger("toggle");
        sm.advance(0.0, &artboard);
        assert_eq!(sm.current_state_name(), "B", "the state itself switches immediately — only the *pose* blends");
        sm.apply(&mut artboard);
        // Weight 0 at the instant the transition fires — still entirely the
        // frozen outgoing (A) pose.
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(0.0, 0.0));

        sm.advance(0.5, &artboard);
        sm.apply(&mut artboard);
        // Halfway through a 1s crossfade: exactly between A's frozen 0.0 and
        // B's (flat, unchanging) 100.0.
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(50.0, 0.0));

        sm.advance(0.5, &artboard);
        sm.apply(&mut artboard);
        // Crossfade window fully elapsed — settles on B's value exactly,
        // with the blend cleared (not stuck at some rounding-error weight
        // just under 1.0).
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(100.0, 0.0));

        // A later `advance` with nothing left to blend must stay stable —
        // proof `self.blend` was actually cleared, not just coincidentally
        // at weight 1.0 this one time.
        sm.advance(1.0, &artboard);
        sm.apply(&mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(100.0, 0.0));
    }

    #[test]
    fn rotation_crossfades_take_the_shortest_angular_path() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);

        let mut a_animation = Animation::new("A");
        a_animation.duration = 0.0;
        a_animation.tracks.push((node, PropertyTrack::Rotation(Track::new(vec![Keyframe::new(0.0, 170.0_f32.to_radians(), Easing::Linear)]))));
        let a_animation = artboard.add_animation(a_animation);

        let mut b_animation = Animation::new("B");
        b_animation.duration = 0.0;
        b_animation.tracks.push((node, PropertyTrack::Rotation(Track::new(vec![Keyframe::new(0.0, (-170.0_f32).to_radians(), Easing::Linear)]))));
        let b_animation = artboard.add_animation(b_animation);

        let def = StateMachineDef {
            name: "Main".into(),
            inputs: Vec::new(),
            states: vec![
                StateDef {
                    name: "A".into(),
                    animation: a_animation,
                    loop_mode: LoopMode::Once,
                    transitions: vec![TransitionDef {
                        to: "B".into(),
                        condition: ConditionDef::Trigger { input: "toggle".into() },
                        direction: PlaybackDirection::Forward,
                        duration_seconds: 1.0,
                        interruptible: true,
                    }],
                    position: glam::Vec2::ZERO,
                    blend: None,
                    program: None,
                },
                StateDef { name: "B".into(), animation: b_animation, loop_mode: LoopMode::Once, transitions: vec![], position: glam::Vec2::ZERO, blend: None, program: None },
            ],
            initial_state: "A".into(),
            layers: Vec::new(),
            deleted: false,
        };
        let mut sm = StateMachine::from_definition(&def, &artboard);

        sm.inputs.fire_trigger("toggle");
        sm.advance(0.5, &artboard);
        sm.apply(&mut artboard);
        // The short way from 170deg to -170deg passes through 180deg (a
        // ~20deg hop), not back down through 0deg (a ~340deg spin) — proves
        // the crossfade actually goes through `KeyframeValue::lerp`, not a
        // naive raw-radians lerp.
        let rotation_deg = artboard.node(node).unwrap().transform.rotation.to_degrees();
        assert!((rotation_deg - 180.0).abs() < 1e-3, "expected ~180deg, got {rotation_deg}deg");
    }

    /// Builds a single-state machine whose one state is a blend (`Blend1DDef`)
    /// between two flat "extreme pose" animations, mapping `"amount"` across
    /// `[0.0, 10.0]` onto `low`/`high` — the smallest possible harness for
    /// exercising `sample_current_state_pose`'s blend branch without any
    /// transition machinery in the way.
    fn blend_state_machine(artboard: &mut Artboard, node: an1mbyt3_core::NodeId) -> StateMachine {
        let low_animation = artboard.add_animation(flat_animation("Low", node, 0.0));
        let high_animation = artboard.add_animation(flat_animation("High", node, 100.0));
        let def = StateMachineDef {
            name: "Main".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Expression".into(),
                animation: low_animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: glam::Vec2::ZERO,
                blend: Some(Blend1DDef { input: "amount".into(), low: low_animation, high: high_animation, low_value: 0.0, high_value: 10.0 }),
                program: None,
            }],
            initial_state: "Expression".into(),
            layers: Vec::new(),
            deleted: false,
        };
        StateMachine::from_definition(&def, artboard)
    }

    #[test]
    fn a_blend_state_interpolates_between_low_and_high_by_the_mapped_input_value() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);
        let mut sm = blend_state_machine(&mut artboard, node);

        sm.inputs.set_number("amount", 0.0);
        sm.apply(&mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(0.0, 0.0));

        sm.inputs.set_number("amount", 10.0);
        sm.apply(&mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(100.0, 0.0));

        sm.inputs.set_number("amount", 5.0);
        sm.apply(&mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(50.0, 0.0));
    }

    #[test]
    fn a_blend_states_weight_clamps_beyond_the_authored_input_range() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);
        let mut sm = blend_state_machine(&mut artboard, node);

        // Below `low_value` — must not extrapolate past 100% `low`.
        sm.inputs.set_number("amount", -50.0);
        sm.apply(&mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(0.0, 0.0));

        // Above `high_value` — must not extrapolate past 100% `high`.
        sm.inputs.set_number("amount", 500.0);
        sm.apply(&mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(100.0, 0.0));
    }

    #[test]
    fn a_blend_states_rotation_takes_the_shortest_angular_path() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);

        let mut low_animation = Animation::new("Low");
        low_animation.duration = 0.0;
        low_animation.tracks.push((node, PropertyTrack::Rotation(Track::new(vec![Keyframe::new(0.0, 170.0_f32.to_radians(), Easing::Linear)]))));
        let low_animation = artboard.add_animation(low_animation);

        let mut high_animation = Animation::new("High");
        high_animation.duration = 0.0;
        high_animation.tracks.push((node, PropertyTrack::Rotation(Track::new(vec![Keyframe::new(0.0, (-170.0_f32).to_radians(), Easing::Linear)]))));
        let high_animation = artboard.add_animation(high_animation);

        let def = StateMachineDef {
            name: "Main".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Expression".into(),
                animation: low_animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: glam::Vec2::ZERO,
                blend: Some(Blend1DDef { input: "amount".into(), low: low_animation, high: high_animation, low_value: 0.0, high_value: 1.0 }),
                program: None,
            }],
            initial_state: "Expression".into(),
            layers: Vec::new(),
            deleted: false,
        };
        let mut sm = StateMachine::from_definition(&def, &artboard);

        sm.inputs.set_number("amount", 0.5);
        sm.apply(&mut artboard);
        let rotation_deg = artboard.node(node).unwrap().transform.rotation.to_degrees();
        assert!((rotation_deg - 180.0).abs() < 1e-3, "expected ~180deg, got {rotation_deg}deg");
    }

    #[test]
    fn an_extra_layers_pose_is_restricted_to_its_own_mask() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let head = add_square(&mut artboard);
        let mouth = add_square(&mut artboard);

        // Base layer: one state, head parked at (10, 0).
        let base_animation = artboard.add_animation(flat_animation("HeadIdle", head, 10.0));

        // The extra layer's one state animates BOTH head and mouth —
        // proving the mask (mouth only) is what keeps its head write from
        // ever reaching the artboard, not just an accidental lack of a
        // head track on this animation.
        let mut open_animation = Animation::new("Open");
        open_animation.duration = 0.0;
        open_animation.tracks.push((head, PropertyTrack::Translation(Track::new(vec![Keyframe::new(0.0, Vec2::new(999.0, 0.0), Easing::Linear)]))));
        open_animation.tracks.push((mouth, PropertyTrack::Translation(Track::new(vec![Keyframe::new(0.0, Vec2::new(50.0, 0.0), Easing::Linear)]))));
        let open_animation = artboard.add_animation(open_animation);

        let def = StateMachineDef {
            name: "Main".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Idle".into(),
                animation: base_animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: glam::Vec2::ZERO,
                blend: None,
                program: None,
            }],
            initial_state: "Idle".into(),
            layers: vec![StateMachineLayerDef {
                name: "Mouth".into(),
                states: vec![StateDef {
                    name: "Open".into(),
                    animation: open_animation,
                    loop_mode: LoopMode::Once,
                    transitions: Vec::new(),
                    position: glam::Vec2::ZERO,
                    blend: None,
                    program: None,
                }],
                initial_state: "Open".into(),
                mask: vec![mouth],
            }],
            deleted: false,
        };
        let mut sm = StateMachine::from_definition(&def, &artboard);
        sm.apply(&mut artboard);

        assert_eq!(artboard.node(head).unwrap().transform.translation, Vec2::new(10.0, 0.0), "extra layer's head write must be blocked by its mask");
        assert_eq!(artboard.node(mouth).unwrap().transform.translation, Vec2::new(50.0, 0.0), "extra layer's masked mouth write must land");
    }

    #[test]
    fn extra_layers_transition_independently_of_the_base_layer_via_shared_inputs() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let head = add_square(&mut artboard);
        let mouth = add_square(&mut artboard);

        let head_idle = artboard.add_animation(flat_animation("HeadIdle", head, 0.0));
        let head_nod = artboard.add_animation(flat_animation("HeadNod", head, 20.0));
        let mouth_closed = artboard.add_animation(flat_animation("MouthClosed", mouth, 0.0));
        let mouth_open = artboard.add_animation(flat_animation("MouthOpen", mouth, 50.0));

        let def = StateMachineDef {
            name: "Main".into(),
            inputs: Vec::new(),
            states: vec![
                StateDef {
                    name: "HeadIdle".into(),
                    animation: head_idle,
                    loop_mode: LoopMode::Once,
                    transitions: vec![TransitionDef {
                        to: "HeadNod".into(),
                        condition: ConditionDef::Trigger { input: "toggleHead".into() },
                        direction: PlaybackDirection::Forward,
                        duration_seconds: 0.0,
                        interruptible: true,
                    }],
                    position: glam::Vec2::ZERO,
                    blend: None,
                    program: None,
                },
                StateDef { name: "HeadNod".into(), animation: head_nod, loop_mode: LoopMode::Once, transitions: Vec::new(), position: glam::Vec2::ZERO, blend: None, program: None },
            ],
            initial_state: "HeadIdle".into(),
            layers: vec![StateMachineLayerDef {
                name: "Mouth".into(),
                states: vec![
                    StateDef {
                        name: "Closed".into(),
                        animation: mouth_closed,
                        loop_mode: LoopMode::Once,
                        transitions: vec![TransitionDef {
                            to: "Open".into(),
                            condition: ConditionDef::Trigger { input: "toggleMouth".into() },
                            direction: PlaybackDirection::Forward,
                            duration_seconds: 0.0,
                            interruptible: true,
                        }],
                        position: glam::Vec2::ZERO,
                        blend: None,
                        program: None,
                    },
                    StateDef { name: "Open".into(), animation: mouth_open, loop_mode: LoopMode::Once, transitions: Vec::new(), position: glam::Vec2::ZERO, blend: None, program: None },
                ],
                initial_state: "Closed".into(),
                mask: vec![mouth],
            }],
            deleted: false,
        };
        let mut sm = StateMachine::from_definition(&def, &artboard);

        // Firing only the mouth layer's own trigger must move the mouth
        // without the base layer reacting to it at all — different
        // layers' transitions only ever check *their own* current state's
        // transition list, even though both read from the same `Inputs`.
        sm.inputs.fire_trigger("toggleMouth");
        sm.advance(0.0, &artboard);
        sm.apply(&mut artboard);
        assert_eq!(artboard.node(mouth).unwrap().transform.translation, Vec2::new(50.0, 0.0));
        assert_eq!(artboard.node(head).unwrap().transform.translation, Vec2::new(0.0, 0.0), "base layer must not react to the mouth layer's own trigger");
        assert_eq!(sm.current_state_name(), "HeadIdle", "base layer's own current state must be unaffected");

        // Firing only the base layer's own trigger must move the head
        // without resetting the mouth layer's already-transitioned state.
        sm.inputs.fire_trigger("toggleHead");
        sm.advance(0.0, &artboard);
        sm.apply(&mut artboard);
        assert_eq!(artboard.node(head).unwrap().transform.translation, Vec2::new(20.0, 0.0));
        assert_eq!(artboard.node(mouth).unwrap().transform.translation, Vec2::new(50.0, 0.0), "mouth layer must stay in its own already-transitioned state");
    }

    /// A<->B machine whose A->B transition's `interruptible` flag is
    /// configurable (B->A is always interruptible) — the harness for both
    /// interrupt-handling tests below.
    fn interruptible_two_state_machine(artboard: &mut Artboard, node: an1mbyt3_core::NodeId, forward_interruptible: bool) -> StateMachine {
        let a_animation = artboard.add_animation(flat_animation("A", node, 0.0));
        let b_animation = artboard.add_animation(flat_animation("B", node, 100.0));
        let def = StateMachineDef {
            name: "Main".into(),
            inputs: Vec::new(),
            states: vec![
                StateDef {
                    name: "A".into(),
                    animation: a_animation,
                    loop_mode: LoopMode::Once,
                    transitions: vec![TransitionDef {
                        to: "B".into(),
                        condition: ConditionDef::Trigger { input: "toggle".into() },
                        direction: PlaybackDirection::Forward,
                        duration_seconds: 1.0,
                        interruptible: forward_interruptible,
                    }],
                    position: glam::Vec2::ZERO,
                    blend: None,
                    program: None,
                },
                StateDef {
                    name: "B".into(),
                    animation: b_animation,
                    loop_mode: LoopMode::Once,
                    transitions: vec![TransitionDef {
                        to: "A".into(),
                        condition: ConditionDef::Trigger { input: "back".into() },
                        direction: PlaybackDirection::Forward,
                        duration_seconds: 1.0,
                        interruptible: true,
                    }],
                    position: glam::Vec2::ZERO,
                    blend: None,
                    program: None,
                },
            ],
            initial_state: "A".into(),
            layers: Vec::new(),
            deleted: false,
        };
        StateMachine::from_definition(&def, artboard)
    }

    #[test]
    fn interrupting_a_crossfade_captures_the_live_blended_pose_not_a_raw_resample() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);
        let mut sm = interruptible_two_state_machine(&mut artboard, node, true);

        sm.inputs.fire_trigger("toggle");
        sm.advance(0.5, &artboard);
        sm.apply(&mut artboard);
        assert_eq!(sm.current_state_name(), "B");
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(50.0, 0.0), "halfway through the first crossfade");

        // Interrupt immediately (no time passes) — the new blend's frozen
        // pose must be the just-asserted composited 50.0, not B's raw flat
        // value (100.0) a buggy raw resample would have captured instead.
        sm.inputs.fire_trigger("back");
        sm.advance(0.0, &artboard);
        sm.apply(&mut artboard);
        assert_eq!(sm.current_state_name(), "A");
        assert_eq!(
            artboard.node(node).unwrap().transform.translation,
            Vec2::new(50.0, 0.0),
            "interrupting must freeze the live blended value, not pop to either endpoint"
        );

        sm.advance(0.5, &artboard);
        sm.apply(&mut artboard);
        assert_eq!(
            artboard.node(node).unwrap().transform.translation,
            Vec2::new(25.0, 0.0),
            "halfway through the second crossfade, from the interrupted 50.0 toward A's 0.0"
        );
    }

    #[test]
    fn a_non_interruptible_blend_ignores_a_new_transition_until_it_completes() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);
        let mut sm = interruptible_two_state_machine(&mut artboard, node, false);

        sm.inputs.fire_trigger("toggle");
        sm.advance(0.5, &artboard);
        sm.apply(&mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(50.0, 0.0));

        // Fires while the non-interruptible A->B blend is still in progress
        // — must be ignored entirely (and, since triggers auto-reset, lost).
        sm.inputs.fire_trigger("back");
        sm.advance(0.1, &artboard);
        sm.apply(&mut artboard);
        assert_eq!(sm.current_state_name(), "B", "the blocked interruption must not switch state");
        let x = artboard.node(node).unwrap().transform.translation.x;
        assert!((x - 60.0).abs() < 1e-3, "must keep progressing along the original A->B crossfade, undisturbed, got x={x}");

        // Let the original crossfade finish.
        sm.advance(0.4, &artboard);
        sm.apply(&mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(100.0, 0.0));

        // Now that the lock is released, a freshly-fired "back" can go through.
        sm.inputs.fire_trigger("back");
        sm.advance(0.0, &artboard);
        assert_eq!(sm.current_state_name(), "A", "the lock only blocks interruption *during* the blend, not transitions afterward");
    }

    /// A single-state machine whose one state is a Programmable State (a
    /// Constant wired straight into `SetProperty(TranslationX)`) — used by
    /// the `programs_enabled` tests below. Reuses `flat_animation` as the
    /// state's required-but-ignored `animation` filler, same convention a
    /// blend state's `animation` field already established.
    fn program_state_machine(artboard: &mut Artboard, node: an1mbyt3_core::NodeId, target: f32) -> StateMachine {
        let filler_animation = artboard.add_animation(flat_animation("Filler", node, 0.0));
        let def = StateMachineDef {
            name: "Main".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Programmed".into(),
                animation: filler_animation,
                loop_mode: LoopMode::Once,
                transitions: vec![],
                position: glam::Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Graph(NodeGraphDef {
                    nodes: vec![
                        GraphNodeDef { id: 0, kind: GraphNodeKind::Constant(target), position: glam::Vec2::ZERO, overrides: vec![] },
                        GraphNodeDef { id: 1, kind: GraphNodeKind::SetProperty { node, property: GraphProperty::TranslationX, global: false }, position: glam::Vec2::ZERO, overrides: vec![] },
                    ],
                    edges: vec![GraphEdgeDef { from_node: 0, from_socket: 0, to_node: 1, to_socket: 0 }],
                })),
            }],
            initial_state: "Programmed".into(),
            layers: Vec::new(),
            deleted: false,
        };
        StateMachine::from_definition(&def, artboard)
    }

    #[test]
    fn a_programmable_states_graph_does_not_run_unless_programs_are_enabled() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);
        let mut sm = program_state_machine(&mut artboard, node, 42.0);

        sm.apply(&mut artboard);

        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 0.0, "must not run while programs_enabled defaults to false");

        sm.set_programs_enabled(true);
        sm.apply(&mut artboard);

        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 42.0, "must run once enabled");
    }

    #[test]
    fn disabling_programs_again_stops_it_from_running_but_leaves_its_last_written_value_alone() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);
        let mut sm = program_state_machine(&mut artboard, node, 42.0);

        sm.set_programs_enabled(true);
        sm.apply(&mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 42.0);

        sm.set_programs_enabled(false);
        artboard.node_mut(node).unwrap().transform.translation.x = 7.0; // simulate a Design-mode edit
        sm.apply(&mut artboard);

        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 7.0, "a disabled program must not overwrite anything");
    }

    #[test]
    fn a_blocks_programs_wait_advances_on_the_layers_own_clock_not_the_zero_duration_filler_animations() {
        // Reproduces a real reported bug: a Programmable State authored
        // purely for `Blocks` game logic has no actual animation of its own
        // — its `duration` is `0.0` (`flat_animation`'s own filler
        // convention, same as `program_state_machine`'s Graph-mode
        // equivalent above). `Playback::advance` is a no-op whenever
        // `duration <= 0.0`, so `self.playback.time` — what `run_blocks`
        // used to source `BlockContext::elapsed_seconds` from — froze at
        // `0.0` forever, and `Wait`'s wake-time comparison against a frozen
        // clock never resolves: a "wait 1 second" block would then never
        // wait *and never resume*, looking like nothing after the first
        // change ever ran. `Layer::blocks_elapsed` (this test's actual
        // subject) accumulates real time unconditionally instead.
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = add_square(&mut artboard);
        let filler_animation = artboard.add_animation(flat_animation("Filler", node, 0.0));
        let def = StateMachineDef {
            name: "Main".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Programmed".into(),
                animation: filler_animation,
                loop_mode: LoopMode::Once,
                transitions: vec![],
                position: glam::Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Blocks(BlockProgramDef {
                    ticks_per_second: None,
                    scripts: vec![BlockScript { position: None,
                        event: EventKind::StateTriggered,
                        statements: vec![Statement::Forever {
                            body: vec![
                                Statement::ChangeProperty { node: NodeRef::Literal(node), property: GraphProperty::TranslationX, delta: Expr::Number(1.0) },
                                Statement::Wait { seconds: Expr::Number(1.0) },
                            ],
                        }],
                    }],
                })),
            }],
            initial_state: "Programmed".into(),
            layers: Vec::new(),
            deleted: false,
        };
        let mut sm = StateMachine::from_definition(&def, &artboard);
        sm.set_programs_enabled(true);

        // Drive ~0.6s in small steps — the zero-duration filler animation
        // never advances its own `Playback`, but `blocks_elapsed` must.
        for _ in 0..12 {
            sm.advance(0.05, &artboard);
            sm.apply(&mut artboard);
        }
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 1.0, "the first change (before the first Wait) must have run");

        // Cross the 1-second mark — the `Wait` must resolve and loop again.
        for _ in 0..12 {
            sm.advance(0.05, &artboard);
            sm.apply(&mut artboard);
        }
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 2.0, "past 1 second of real time, the Wait must have resolved and the loop advanced again");
    }
}
