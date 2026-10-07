//! `an1mbyt3-runtime`: timelines, playback, events, parameters.
//!
//! See AN1MBYT3_BUILD_PLAN.md §5 ("Animation & Interaction Features") — this
//! crate evaluates animation over time.
//!
//! Owns (so far):
//! - [`Playback`]: play/pause/seek/loop time-advancement, independent of any
//!   specific track (`playback.rs`) — runtime session state, not saved
//!   document content, unlike `Easing`/`Keyframe`/`Track`/`AnimationClip`/
//!   `LoopMode` below.
//! - [`StateMachine`]/[`Inputs`] (`state_machine.rs`): the *live* half of a
//!   state machine — built from a saved `an1mbyt3_core::StateMachineDef`
//!   via [`StateMachine::from_definition`], guarded transitions between
//!   named states (each referencing an `Animation` + `LoopMode`), and typed
//!   bool/number/enum/trigger inputs (§5). `Inputs` is live session state,
//!   like `Playback`; the state machine's actual *definition* (states,
//!   transitions, conditions) lives in `an1mbyt3-core` now — see
//!   `state_machine.rs`'s module doc for the Phase 8 split and why.
//!
//! Re-exports (moved to `an1mbyt3-core`, so they can persist as part of a
//! saved `.tea`/`.teab` document rather than only exist as hand-built
//! runtime demo content — see each type's own module doc in `an1mbyt3-core`
//! for the full reasoning):
//! - [`Easing`]: linear and cubic-Bézier easing curves.
//! - [`Lerp`]/[`Keyframe`]/[`Track`]: generic keyframe interpolation — the
//!   value type (`f32`, `glam::Vec2`, ...) is generic via `Lerp`, not
//!   hard-coded.
//! - [`AnimationClip`]: ties a `Track<Vec2>` to a node's transform and
//!   applies a sampled value back onto the document — still used directly
//!   by examples that just play one clip with no state machine involved.
//! - [`LoopMode`]/[`PlaybackDirection`]/[`ConditionDef`]/[`StateDef`]/
//!   [`TransitionDef`]/[`StateMachineDef`]/[`InputDef`]/[`InputKind`]: a
//!   state machine's authored, saved shape (Phase 8) — see
//!   `state_machine.rs`'s module doc.
//!
//! Does NOT own (yet):
//! - General property-path/parameter binding (`Node(42).rotation`, pointer-driven
//!   parameters) — state machine inputs are hand-set by the caller
//!   (`Inputs::set_bool` etc.); wiring an input directly to live
//!   pointer/scroll data is the parameter-binding layer §5 describes, still
//!   not built.
//! - Bone/IK/constraint solving — see `an1mbyt3-constraints`.
//! - Geometry generation or drawing — see `an1mbyt3-geometry`/`an1mbyt3-renderer`.
//!
//! Depends on `an1mbyt3-core` (document/node types, the keyframe/animation
//! data model, and now the state-machine definition types too) and
//! `an1mbyt3-geometry` (declared for the deformation work Phase 5 adds; not
//! yet used by this crate's own state-machine code).
//!
//! Status: Phase 2 (keyframes/playback), Phase 4 (keyframe data model
//! promoted to `an1mbyt3-core` for real persistence), Phase 6 (state
//! machines), and Phase 8 (state machine *definitions* promoted to
//! `an1mbyt3-core` for the same reason) are implemented
//! (AN1MBYT3_BUILD_PLAN.md §10).

mod block_interpreter;
mod graph;
mod keyboard;
mod particle_sim;
mod playback;
mod state_machine;

pub use an1mbyt3_core::{
    AnimationClip, ConditionDef, Easing, InputDef, InputKind, Keyframe, Lerp, PlaybackDirection, ProgramDef, StateDef, StateMachineDef, Track, TransitionDef,
};
pub use block_interpreter::{touching, BlockContext};
pub use graph::{evaluate_graph, GraphContext};
pub use keyboard::KeyboardState;
pub use particle_sim::{step as step_particle_system, Particle, ParticleSystemRuntime};
pub use playback::{LoopMode, Playback};
pub use state_machine::{Inputs, StateMachine};
