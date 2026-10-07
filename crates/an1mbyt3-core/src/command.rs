//! `Command`: the only way document mutations happen, per
//! AN1MBYT3_BUILD_PLAN.md §4's "Undo/Redo Architecture" — a node move, a
//! rotation edit, a node creation, a node deletion are each a `Command`
//! value, not ad hoc field writes scattered through editor/IPC handler code.
//!
//! An enum (not a trait object) because the mutation set is small and known
//! right now. A `dyn Command` trait (apply/invert as separate methods,
//! arbitrary mutation types) is the right design once far more variants
//! exist (§4 describes this as the long-term shape), but building that
//! generality now, when most variants still follow the same "swap in a new
//! value, return the old one" pattern, would be abstraction with no current
//! payoff.
//!
//! Every variant's `apply` returns its own inverse (the command that undoes
//! it) rather than a separate `invert` step — capturing the *old* value at
//! the moment of mutation is the only correct time to do it (reading it
//! before vs. after the swap is exactly the difference between "this undoes
//! me" and "this repeats me"), so folding that into `apply` itself removes
//! a whole class of bug where the two steps could be called out of order.
//! The inverse doesn't have to be the *same* variant as the command that
//! produced it — see `AddNode`'s doc comment for why its inverse is a
//! `SetDeleted`, not another `AddNode`.
//!
//! `apply` takes `self` by value (not `&self`): every call site (`Document::apply`/
//! `undo`/`redo`) already owns the `Command` it's applying and never needs it
//! again afterward, and taking it by value is what lets `AddNode` hold a
//! non-`Copy` `Node` payload without the enum needing to derive `Copy`.

use glam::Vec2;

use crate::{
    artboard::Artboard,
    component::{ComponentDefinition, ComponentVariant},
    easing::Easing,
    ids::{AnimationId, ComponentId, PrefabId, ScriptId, StateMachineId},
    prefab::PrefabDefinition,
    keyframe::{Keyframe, Lerp, Track},
    node::{Node, NodeKind},
    program::ProgramDef,
    script::ScriptDefinition,
    script::ScriptFieldDef,
    state_machine::{InputDef, StateDef, StateMachineDef, StateMachineLayerDef, TransitionDef},
    timeline::{Animation, KeyframeValue, PropertyKind, PropertyTrack},
    ArtboardId, Document, NodeId,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    SetTranslation { artboard: ArtboardId, node: NodeId, value: Vec2 },
    /// Radians, matching `Transform2D::rotation`'s unit — degree conversion
    /// is a display/IPC-boundary concern (see `apps/editor/src-tauri`'s DTO
    /// layer), not something the command itself should know about.
    SetRotation { artboard: ArtboardId, node: NodeId, value: f32 },
    SetScale { artboard: ArtboardId, node: NodeId, value: Vec2 },
    /// Changes a node's anchor (pivot point), in local space — the on-canvas
    /// anchor handle's commit (`WasmEditor::commit_node_anchor` always
    /// applies this alongside a `SetTranslation` in one `Batch`, so the
    /// pivot moves without the shape visibly jumping — see
    /// `Transform2D::with_anchor_preserving_world`'s doc comment for the
    /// math). This variant alone is a plain "swap in a new value, return
    /// the old one" edit, same shape as `SetTranslation` — it does not
    /// itself compensate translation; that's the caller's job.
    SetAnchor { artboard: ArtboardId, node: NodeId, value: Vec2 },
    /// Inserts `node` as a new node in `artboard`. Unlike the `Set*` variants,
    /// there's no "previous value" to capture — the node didn't exist before
    /// — so this can't return another `AddNode` as its own inverse (it
    /// doesn't know what `NodeId` `slotmap` will assign until it actually
    /// inserts). Its inverse is `SetDeleted { value: true }` targeting the
    /// freshly assigned id instead: undoing a creation soft-deletes it, redo
    /// (applying that inverse's own inverse) un-deletes it — see
    /// `SetDeleted`'s doc comment for why soft-delete, not a real removal.
    AddNode { artboard: ArtboardId, node: Node },
    /// Flips a node's `Node::deleted` flag. This is *the* deletion primitive
    /// (`value: true` from the UI's "delete this node" action) and also
    /// `AddNode`'s inverse (`value: true`) and that inverse's own inverse
    /// (`value: false`, i.e. redo of a delete / undo of... itself) — one
    /// variant covers both directions because both are "set this bool,
    /// return the command that sets it back."
    ///
    /// Deletion is soft (flag flip, not `Artboard`-arena removal) because
    /// `slotmap` has no API to reinsert a value at a specific
    /// previously-freed key: undoing a delete must restore the *exact same*
    /// `NodeId` so any earlier command still sitting on the undo/redo stack
    /// (e.g. a transform edit applied before the delete) keeps resolving to
    /// the right node. `Artboard::shapes`/`texts` and `dto::summarize_document`
    /// skip `deleted` nodes; `Artboard::node`/`node_mut` still resolve them,
    /// since this command (and anything undoing/redoing it) needs to reach a
    /// deleted node to flip its flag back.
    SetDeleted { artboard: ArtboardId, node: NodeId, value: bool },
    /// Sets `Node::visible` directly — distinct from `SetDeleted`: a hidden
    /// node is still fully present (selectable, editable, has live
    /// children), a deleted one is soft-removed from every reader. No
    /// frontend/Inspector UI drives this yet; it exists for script-driven
    /// show/hide (a Code-mode state machine's own "scene" concept, toggling
    /// a group of nodes on/off) where going through the undo stack is
    /// actually wanted (unlike continuous per-frame changes, which use a
    /// `preview_*` method instead — see `WasmEditor::preview_fill_color`'s
    /// own doc comment for that distinction).
    SetNodeVisible { artboard: ArtboardId, node: NodeId, value: bool },
    /// Renames a node — the Layers panel's double-click-to-rename action.
    /// Captures the old name as its own inverse, same "swap in a new value,
    /// return the old one" shape as `SetTranslation`.
    SetNodeName { artboard: ArtboardId, node: NodeId, value: String },
    /// Changes a node's `id` — the Inspector's id field (`getCompByID`'s
    /// own lookup key, `an1mbyt3-core::node::Node::id`'s own doc comment).
    /// Same "swap in a new value, return the old one" shape as
    /// `SetNodeName`; doesn't itself check for uniqueness against other
    /// nodes' ids — same "caller's own mistake, not a validity concern
    /// this data model enforces" convention `Node::id`'s own doc comment
    /// already states.
    SetNodeId { artboard: ArtboardId, node: NodeId, value: String },
    /// Replaces a node's whole `classes` list — `getCompByClass`'s
    /// selector tags. Whole-list swap, same "the UI already has to compute
    /// the new full list either way" convention `SetStateMachineInputs`
    /// established for add/remove-one-entry-at-a-time editing.
    SetNodeClasses { artboard: ArtboardId, node: NodeId, value: Vec<String> },
    /// Toggles a node's `clip_children` — see that field's own doc comment
    /// (`an1mbyt3-core::node::Node`) for what it does and how it differs
    /// from `SetGroupMask`. Works on any node kind, not just `Group`, unlike
    /// `SetGroupMask` which panics on a non-`Group` target. Same "swap in a
    /// new value, return the old one" shape as `SetNodeId`.
    SetNodeClipChildren { artboard: ArtboardId, node: NodeId, value: bool },
    /// Inserts a new, empty, named [`Animation`] into `artboard` — the
    /// editor's "New Animation" dropdown action. Same "doesn't know the
    /// freshly assigned id until `slotmap` inserts" shape as `AddNode` — its
    /// inverse is `SetAnimationDeleted { value: true }`.
    AddAnimation { artboard: ArtboardId, animation: Animation },
    /// Soft-deletes (`value: true`) or restores (`value: false`) a whole
    /// `Animation` — same "flag flip, not real removal" reasoning as
    /// `SetDeleted` for nodes: `slotmap` can't reinsert at a freed key, and
    /// undo/redo needs the `AnimationId` to stay valid for any earlier
    /// command (a keyframe edit made before the delete) still on the stack.
    SetAnimationDeleted { artboard: ArtboardId, animation: AnimationId, value: bool },
    /// Renames an animation — the editor's rename-in-dropdown action.
    SetAnimationName { artboard: ArtboardId, animation: AnimationId, value: String },
    /// Resizes an animation's fixed, authored time range — the Timeline's
    /// duration input. Clamped to a minimum of `0.1` (never zero/negative —
    /// `Track::sample` and every ratio computed from `duration` assume a
    /// positive range). Existing keyframes are left wherever they are, even
    /// if this shrinks the range past one of them — `WasmEditor`'s
    /// `add_keyframe`/`preview_keyframe_time`/`commit_keyframe_time` are what
    /// keep *new* keyframe times inside `duration` (see their doc comments);
    /// this command only changes the range itself.
    SetAnimationDuration { artboard: ArtboardId, animation: AnimationId, value: f32 },
    /// Resizes the artboard's canvas — the Inspector's "Canvas" section
    /// width/height fields. Clamped to a minimum of `1.0` (a zero/negative
    /// canvas has no sensible render target size).
    SetArtboardSize { artboard: ArtboardId, width: f32, height: f32 },
    /// Sets this artboard's pixelation block size, in pixels — see
    /// `Artboard::resolution_scale`'s own doc comment. Clamped to
    /// `[1.0, 64.0]`: `1.0` is native/off (below that has no sensible
    /// block size), `64.0` is an arbitrary "chunky enough" ceiling.
    SetArtboardResolutionScale { artboard: ArtboardId, value: f32 },
    /// Sets the artboard's screen behaviour (Fixed / Responsive, scale
    /// match, compact breakpoint) — see `crate::ScreenSettings`.
    SetArtboardScreen { artboard: ArtboardId, value: crate::ScreenSettings },
    /// Inserts (or, at an already-keyframed `time`, *replaces*) a keyframe on
    /// `node`'s `value.property_kind()` track within `animation`, creating
    /// that track if this is its first keyframe. Replacing instead of
    /// inserting a duplicate at an existing time covers the common "re-key
    /// this same moment with a new value" workflow (scrub to a keyframe's
    /// exact time, reposition the node, hit "Add Keyframe" again) with the
    /// same command — see `apply`'s two branches for the resulting inverses
    /// (`RemoveKeyframe` for a genuine insert, another `AddKeyframe` holding
    /// the replaced keyframe's old value for an overwrite).
    AddKeyframe { artboard: ArtboardId, animation: AnimationId, node: NodeId, time: f32, value: KeyframeValue, easing: Easing },
    /// Creates `node`'s `PropertyTrack::MotionPath` track within `animation`
    /// (empty, no keyframes yet) bound to `guide`, or — if that track already
    /// exists — rebinds it to a different `guide` in place, keeping whatever
    /// progress keyframes it already has. Unlike every other property, a
    /// `MotionPath` track can't be lazily created by `AddKeyframe` alone (see
    /// `KeyframeValue::MotionPath`'s doc comment): the guide `NodeId` has to
    /// come from somewhere, and this is that somewhere — the editor calls
    /// this once (when the user picks a guide shape for a node) before any
    /// `AddKeyframe { value: KeyframeValue::MotionPath(_), .. }` calls make
    /// sense. Inverse is `RemoveMotionPathTrack` if this created a fresh
    /// track, or another `SetMotionPathGuide` holding the old guide if it
    /// only rebound an existing one.
    SetMotionPathGuide { artboard: ArtboardId, animation: AnimationId, node: NodeId, guide: NodeId },
    /// Removes `node`'s entire `PropertyTrack::MotionPath` track (guide
    /// binding and any progress keyframes) from `animation`. Panics if no
    /// such track exists (caller bug, same convention as `RemoveKeyframe`).
    /// Inverse is `SetMotionPathGuide` with the removed track's guide — note
    /// this only restores the *guide binding*, not any progress keyframes
    /// the track held; only ever actually produced as the inverse of a
    /// `SetMotionPathGuide` that created a fresh (keyframe-less) track, so
    /// that's never a real loss in practice — see that variant's own inverse.
    RemoveMotionPathTrack { artboard: ArtboardId, animation: AnimationId, node: NodeId },
    /// Removes the keyframe at exactly `time` from `node`'s `property` track
    /// within `animation`. Panics if the track or that exact keyframe
    /// doesn't exist (caller bug, same convention as every other variant
    /// here). If this empties the track, the whole `(node, property)` entry
    /// is removed from `Animation::tracks` rather than left as a
    /// zero-keyframe track — `Track::sample` panics on an empty track, so
    /// nothing downstream (rendering, scrubbing) ever needs to guard against
    /// that case. Its inverse is `AddKeyframe` with the removed keyframe's
    /// captured value/easing.
    RemoveKeyframe { artboard: ArtboardId, animation: AnimationId, node: NodeId, property: PropertyKind, time: f32 },
    /// Retimes the keyframe currently at `old_time` to `new_time` on `node`'s
    /// `property` track within `animation` — the Timeline's keyframe-drag
    /// action, meant to be applied exactly once per whole drag gesture (see
    /// `editor.rs`'s `commit_keyframe_time` doc comment for the same
    /// preview/commit split `commit_node_translation` already established
    /// for on-canvas dragging). Its inverse simply swaps `old_time`/`new_time`.
    MoveKeyframe { artboard: ArtboardId, animation: AnimationId, node: NodeId, property: PropertyKind, old_time: f32, new_time: f32 },
    /// Adds `delta` to every keyframe's value on `node`'s `property` track,
    /// in *every* animation on `artboard` that keys it (not just whichever
    /// one is active in the editor right now) — "moving a node's Design-mode
    /// rest pose carries its whole animated motion along with it," rather
    /// than the animation staying pinned to wherever it was first authored.
    /// This is what makes re-dragging a keyframed node in Design mode (or
    /// editing its rotation/scale in the Inspector, whether keyframed or
    /// not) rebase the *entire* path by the same delta instead of only
    /// affecting whatever single moment the playhead happened to be on.
    /// Only ever issued alongside the plain `Set*` command that produced
    /// `delta` (as a `Command::Batch`, one undo step for both) — see
    /// `editor.rs`'s `apply_transform_edit`. Its inverse negates `delta`.
    ShiftPropertyKeyframes { artboard: ArtboardId, node: NodeId, property: PropertyKind, delta: KeyframeValue },
    /// Applies every command in order and returns their inverses, reversed,
    /// as another `Batch` — undoing a batch undoes its effects in the
    /// correct (last-applied-first) order, same as the outer undo stack's
    /// own semantics, just nested. Exists for edits that are conceptually
    /// one user action but need more than one primitive command to express
    /// (e.g. `SetTranslation` + `ShiftPropertyKeyframes` together) — without
    /// this, such an action would either need its own bespoke `Command`
    /// variant (one per combination) or cost the user two separate undo
    /// presses for what felt like one edit.
    Batch(Vec<Command>),
    /// Inserts a new, named [`StateMachineDef`] into `artboard` — the
    /// State Machine panel's "New State Machine" action. Same
    /// "doesn't know the freshly assigned id until `slotmap` inserts" shape
    /// as `AddNode`/`AddAnimation` — its inverse is
    /// `SetStateMachineDeleted { value: true }`.
    AddStateMachine { artboard: ArtboardId, state_machine: StateMachineDef },
    /// Soft-deletes (`value: true`) or restores (`value: false`) a whole
    /// state machine — same "flag flip, not real removal" reasoning as
    /// `SetAnimationDeleted`.
    SetStateMachineDeleted { artboard: ArtboardId, state_machine: StateMachineId, value: bool },
    /// Changes which state a state machine starts in — swaps in `value`,
    /// captures the old name as its own inverse, same shape as
    /// `SetAnimationName`. Does not validate that `value` actually names one
    /// of the machine's own states — same "caller bug, not a recoverable
    /// runtime condition" convention `Command`'s doc comment already
    /// establishes; the editor UI only ever offers a dropdown of real state
    /// names, so this can't reach a stale one in practice.
    SetStateMachineInitialState { artboard: ArtboardId, state_machine: StateMachineId, value: String },
    /// Replaces a state machine's whole declared-`Inputs` list — the State
    /// Machine panel's "declare an input" / "remove an input" actions both
    /// go through this rather than separate add/remove variants: the editor
    /// UI already has to compute "the new full list" either way (clone the
    /// old one, push or filter), and swapping the whole `Vec` at once keeps
    /// undo trivial (restore the old `Vec`) instead of needing index
    /// bookkeeping across two different mutation shapes.
    SetStateMachineInputs { artboard: ArtboardId, state_machine: StateMachineId, value: Vec<InputDef> },
    /// Replaces a state machine's whole `states` list — same "whole-list
    /// swap" reasoning as `SetStateMachineInputs`, covering add/remove/
    /// reorder-a-state in one command shape. Deliberately doesn't cascade
    /// into removing transitions elsewhere that target a removed state's
    /// name — a dangling `TransitionDef::to` just never matches during
    /// `StateMachine::advance` (harmless no-op, per that method's own
    /// `unwrap_or_else` panic only firing on an actual *taken* transition to
    /// a missing state) rather than this command needing to understand the
    /// whole machine's graph structure to clean up after itself.
    SetStateMachineStates { artboard: ArtboardId, state_machine: StateMachineId, value: Vec<StateDef> },
    /// Replaces a state machine's whole `layers` list — same "whole-list
    /// swap" reasoning as `SetStateMachineStates`, covering add/remove/
    /// reorder-a-layer *and* any edit to one layer's own `states`/
    /// `initial_state`/`mask` in one command shape (the editor UI clones
    /// the current `layers`, finds the one layer it's editing, mutates
    /// just that entry, and swaps the whole `Vec` back in — see
    /// `an1mbyt3_core::StateMachineLayerDef`'s own doc comment for what a
    /// layer is). Kept as one command rather than a separate variant per
    /// layer-field (mask vs. states vs. initial_state) for the same reason
    /// `SetStateMachineStates` itself is one command, not one per
    /// state-field: undo only ever needs to restore "the old whole list."
    SetStateMachineLayers { artboard: ArtboardId, state_machine: StateMachineId, value: Vec<StateMachineLayerDef> },
    /// Replaces one *named* state's whole `transitions` list — same
    /// "whole-list swap" shape as `SetStateMachineInputs`/`SetStateMachineStates`.
    /// Addressed by the state's name (not an index into `states`) since
    /// `StateDef`s have no independent id of their own — same addressing
    /// `StateMachine::transition_to` already uses.
    SetStateTransitions { artboard: ArtboardId, state_machine: StateMachineId, state_name: String, value: Vec<TransitionDef> },
    /// Replaces one *named* state's whole `program` — the node-graph/code
    /// editor's save action, same "whole-value swap" shape as
    /// `SetStateTransitions` (the frontend computes the entire new
    /// `ProgramDef` after every edit and calls this once, rather than one
    /// command per node/wire/override edit).
    SetStateProgram { artboard: ArtboardId, state_machine: StateMachineId, state_name: String, value: Option<ProgramDef> },
    /// Moves one named state's box on the State Machine panel's node-graph
    /// canvas — drag-and-drop layout, same "swap in a new value, return the
    /// old one" shape as `SetStateMachineInitialState`. Purely a layout
    /// concern (`StateDef::position`'s own doc comment: the runtime never
    /// reads this), so unlike every `Set*` variant that mutates a node's
    /// `Transform2D`, this has nothing to do with rendering the state
    /// machine's actual *animated* content.
    SetStatePosition { artboard: ArtboardId, state_machine: StateMachineId, state_name: String, value: Vec2 },
    /// Renames a state machine — the State Machine panel's "Edit" button.
    /// Same "swap in a new value, return the old one" shape as
    /// `SetAnimationName`.
    SetStateMachineName { artboard: ArtboardId, state_machine: StateMachineId, value: String },
    /// Resizes a bone's reach (`BoneNode::length`) — the Bones panel's length
    /// field. Bone *creation* reuses `AddNode` as-is (`NodeKind` is already
    /// generic there), and a bone's joint pose reuses `SetTranslation`/
    /// `SetRotation` like any other node — this is the one bone-specific
    /// field neither of those already covers. Panics via `node_mut` if `node`
    /// isn't a `NodeKind::Bone` — same "caller bug" convention as every other
    /// variant; the editor only ever offers this field on a selected bone.
    SetBoneLength { artboard: ArtboardId, node: NodeId, value: f32 },
    /// Changes a shape's blend mode — the Inspector's blend-mode dropdown
    /// (`Shape` nodes only). Panics via `node_mut` if `node` isn't a
    /// `NodeKind::Shape` — same "caller bug" convention as `SetBoneLength`.
    SetShapeBlendMode { artboard: ArtboardId, node: NodeId, value: crate::blend_mode::BlendMode },
    /// Changes a node's own `fill_color` — the Inspector's fill-color
    /// swatch, for `Shape` and `Text` nodes (both carry a `fill_color`
    /// field directly, unlike `Instance`, whose effective color is instead
    /// resolved from `variant`/`fill_color_override`/the definition's
    /// default — see `SetInstanceFillColorOverride` for that one). Panics
    /// via `node_mut` if `node` is neither — same "caller bug" convention
    /// as `SetShapeBlendMode`; the Inspector only ever offers this swatch
    /// on a selected `Shape`/`Text` node.
    SetFillColor { artboard: ArtboardId, node: NodeId, value: [f32; 4] },
    /// Sets (`Some`) or clears (`None`) a `Shape`/`Text` node's gradient —
    /// the Fill tool's solid/gradient toggle and its on-canvas handle both
    /// go through this (the handle drags `GradientDef::start`/`end` and
    /// commits the whole `GradientDef` back here, same "commit on release"
    /// convention `SetStatePosition` established for a drag gesture — see
    /// `crate::gradient`'s module doc comment for the overall design).
    /// `None` doesn't touch `fill_color` — clearing a gradient reveals
    /// whatever flat color was already there, not a random default. Panics
    /// via `node_mut` if `node` is neither — same "caller bug" convention
    /// as `SetFillColor`.
    SetGradient { artboard: ArtboardId, node: NodeId, value: Option<crate::gradient::GradientDef> },
    /// Sets (`Some`) or clears (`None`) a `Shape` node's `image` — the
    /// Inspector's asset-reference swatch (see `ShapeNode::image`'s own doc
    /// comment). Shape only, unlike `SetFillColor`/`SetGradient` (a `Text`
    /// node's own glyph fill textured with an image isn't a case this
    /// covers yet). Panics via `node_mut` if `node` isn't a `NodeKind::Shape`.
    SetShapeImage { artboard: ArtboardId, node: NodeId, value: Option<crate::ids::ImageId> },
    /// Replaces a `Shape` node's whole path — the path-editing overlay's
    /// commit action (drag a point/handle, insert a point, delete a point),
    /// one undoable edit per gesture, same whole-value-swap shape as
    /// `SetShapeImage`. Panics via `node_mut` if `node` isn't a `NodeKind::Shape`.
    SetShapePath { artboard: ArtboardId, node: NodeId, value: crate::path::Path },
    /// Sets (`Some`) or clears (`None`) a `Shape` node's `stroke` — the
    /// Inspector's Stroke section. Panics via `node_mut` if `node` isn't a
    /// `NodeKind::Shape`.
    SetShapeStroke { artboard: ArtboardId, node: NodeId, value: Option<crate::node::Stroke> },
    /// Replaces a `ParticleSystem` node's whole authored config — the
    /// Inspector's Particle System section, edited all at once (same "one
    /// nested object, not a dozen flattened sub-commands" reasoning
    /// `an1mbyt3-wasm`'s own `ParticleSystemDto` doc comment gives). Panics
    /// via `node_mut` if `node` isn't a `NodeKind::ParticleSystem`.
    SetParticleSystem { artboard: ArtboardId, node: NodeId, value: crate::ParticleSystemNode },
    /// Paints (`Some`) or erases (`None`) one cell of a `Tilemap` node — the
    /// Tilemap panel's paint tool, one undoable edit per cell click (not one
    /// per whole brush stroke — a drag-paint gesture is just many of these
    /// in quick succession, same granularity `SetFillColor` already has for
    /// a color drag). A no-op (not a panic) if `(x, y)` is outside the
    /// authored grid — `TilemapNode::set`'s own "caller's job to clamp
    /// first" convention, and a paint click can legitimately land just past
    /// the grid's edge. Panics via `node_mut` if `node` isn't a
    /// `NodeKind::Tilemap`.
    SetTile { artboard: ArtboardId, node: NodeId, x: u32, y: u32, value: Option<u32> },
    /// Replaces a `Tilemap` node's whole grid (size + tile data) in one
    /// shot — the Inspector's "Map Width"/"Map Height" resize fields.
    /// Whole-value-swap shape (like `SetParticleSystem`/`SetShapePath`), not
    /// a bare `(width, height)` pair, specifically so the inverse is exact:
    /// a resize that shrinks the grid is lossy (`TilemapNode::resize`'s own
    /// doc comment), so undo has to restore the *exact* previous tile data,
    /// not just re-resize back to the old dimensions (which would compute
    /// different, data-losing content). The caller (`WasmEditor::resize_tilemap`)
    /// computes `tiles` via `TilemapNode::resize`'s own overlap-preserving
    /// algorithm before issuing this. Panics via `node_mut` if `node` isn't
    /// a `NodeKind::Tilemap`.
    SetTilemapGrid { artboard: ArtboardId, node: NodeId, map_width: u32, map_height: u32, tiles: Vec<Option<u32>> },
    /// Changes a `Tilemap` node's on-canvas cell size (world units) — the
    /// Inspector's "Cell Size" field. Panics via `node_mut` if `node` isn't
    /// a `NodeKind::Tilemap`.
    SetTilemapCellSize { artboard: ArtboardId, node: NodeId, value: f32 },
    /// Changes a `Text` node's authored string — the Inspector's content
    /// field. Panics via `node_mut` if `node` isn't a `NodeKind::Text`.
    SetTextContent { artboard: ArtboardId, node: NodeId, value: String },
    /// Changes a `Text` node's `size_px` — the Inspector's size field.
    /// Panics via `node_mut` if `node` isn't a `NodeKind::Text`.
    SetTextSize { artboard: ArtboardId, node: NodeId, value: f32 },
    /// Inserts a new, named [`ComponentDefinition`] into the document
    /// (component-scoped, not `artboard`-scoped — see that type's own doc
    /// comment for why: a component is meant to be instantiable from any
    /// artboard). Same "doesn't know the freshly assigned id until `slotmap`
    /// inserts" shape as `AddNode`/`AddAnimation`/`AddStateMachine` — its
    /// inverse is `SetComponentDeleted { value: true }`.
    AddComponent { definition: ComponentDefinition },
    /// Inserts a new [`PrefabDefinition`] (see `crate::prefab`). Inverse:
    /// `SetPrefabDeleted { value: true }`, same shape as `AddComponent`.
    AddPrefab { definition: PrefabDefinition },
    /// Soft-deletes / restores a prefab — same reasoning as
    /// `SetComponentDeleted`.
    SetPrefabDeleted { prefab: PrefabId, value: bool },
    /// Replaces a prefab's whole definition (Apply-to-prefab, rename).
    /// Inverse: the previous definition.
    SetPrefab { prefab: PrefabId, value: Box<PrefabDefinition> },
    /// Replaces one node's whole data in place — prefab sync/instantiate
    /// (copying a prefab node's kind, behaviors, links…) in one undoable
    /// step. The node keeps its current `parent` and `deleted` flag no
    /// matter what `value` says: hierarchy bookkeeping (`root_nodes`,
    /// `child_order`) only changes through `ReparentNode`/`AddNode`, and
    /// letting this rewrite `parent` would silently desync it. Inverse: the
    /// previous node.
    ReplaceNode { artboard: ArtboardId, node: NodeId, value: Box<Node> },
    /// Soft-deletes (`value: true`) or restores (`value: false`) a whole
    /// component definition — same "flag flip, not real removal" reasoning
    /// as `SetStateMachineDeleted`: undo/redo needs the `ComponentId` to stay
    /// valid for any earlier command (an instance created from it) still on
    /// the stack.
    SetComponentDeleted { component: ComponentId, value: bool },
    /// Changes a component definition's default fill color — the Components
    /// panel's default-color field. Swaps in `value`, captures the old color
    /// as its own inverse, same shape as `SetAnimationName`.
    SetComponentDefaultFillColor { component: ComponentId, value: [f32; 4] },
    /// Replaces a component definition's whole `variants` list — the
    /// Components panel's "add/rename/remove a variant" actions all go
    /// through this rather than separate variants, same "whole-list swap"
    /// reasoning `SetStateMachineInputs` already established.
    SetComponentVariants { component: ComponentId, value: Vec<ComponentVariant> },
    /// Changes one instance's direct fill-color override — the Inspector's
    /// per-instance color field (`Instance` nodes only). `None` clears the
    /// override, falling back to the instance's `variant`/the definition's
    /// default (see `ComponentInstance::resolved_fill_color`'s precedence
    /// order). Panics via `node_mut` if `node` isn't a `NodeKind::Instance`
    /// — same "caller bug" convention as `SetBoneLength`/`SetShapeBlendMode`.
    SetInstanceFillColorOverride { artboard: ArtboardId, node: NodeId, value: Option<[f32; 4]> },
    /// Changes which named variant an instance references — the Inspector's
    /// per-instance variant dropdown (`Instance` nodes only). Doesn't
    /// validate that `value` actually names one of the definition's
    /// variants — same "caller bug, not a recoverable runtime condition"
    /// convention as `SetStateMachineInitialState`; a stale/missing name
    /// just falls through to the definition's default color at resolve
    /// time, per `ComponentInstance::resolved_fill_color`'s own doc comment.
    SetInstanceVariant { artboard: ArtboardId, node: NodeId, value: Option<String> },
    /// Moves `node` to a new parent (`Some`) or to the artboard's root
    /// (`None`) — the Layers panel's drag-to-reparent action, and also how
    /// SVG import attaches an imported shape/group under the `Group` node
    /// created for its enclosing `<g>`. Captures the old parent as its own
    /// inverse, same "swap in a new value, return the old one" shape as
    /// `SetNodeName`, but goes through `Artboard::reparent_node` rather than
    /// a direct field write since `root_nodes` bookkeeping has to move too.
    /// Doesn't validate against cycles itself — see that method's doc
    /// comment for why that check lives at the wasm-editor edit-time layer
    /// instead.
    ReparentNode { artboard: ArtboardId, node: NodeId, value: Option<NodeId> },
    /// `ReparentNode`'s positional counterpart — moves `node` into `parent`'s
    /// children (or `root_nodes`, if `parent` is `None`) at exactly `index`
    /// instead of always appending to the end. The Layers panel's
    /// drag-to-reorder gesture (`WasmEditor::reparent_node_before`) needs
    /// this specifically: `ReparentNode` alone can only ever move a node to
    /// the *back* of its new list, so it can reparent between groups but
    /// can't actually reorder — dropping a layer between two others (or
    /// even just above/below a sibling) always landed it at the very end
    /// regardless of where it was dropped. `ReparentNode` stays as the
    /// simpler "just move it, position doesn't matter" primitive every
    /// other caller (component grouping, Programmable-state demo rigs, SVG
    /// import) already uses. Its own inverse restores *both* the old parent
    /// and the exact old index — same "an exact prior position, not just
    /// one step" reasoning `SetNodeIndex`'s own doc comment gives for why it
    /// can't invert to some simpler opposite command.
    ReparentNodeAt { artboard: ArtboardId, node: NodeId, parent: Option<NodeId>, index: usize },
    /// Sets (or clears, via `None`) which child clips the rest of a
    /// `Group` node's contents — the Layers panel's "use as mask"
    /// drag-and-drop action landing on `GroupNode::mask`. Panics via
    /// `node_mut` if `node` isn't a `NodeKind::Group` — same "caller bug"
    /// convention as `SetBoneLength`/`SetShapeBlendMode`. Doesn't validate
    /// that `value` is actually one of `node`'s own children (same
    /// "caller bug, not a recoverable runtime condition" convention as
    /// `SetStateMachineInitialState`) — the renderer's `render_with_mask`
    /// only ever looks up a mask by walking a group's real children anyway,
    /// so a stale/foreign id just fails to resolve there, harmlessly.
    SetGroupMask { artboard: ArtboardId, node: NodeId, value: Option<NodeId> },
    /// Swaps `node` one step later in its draw-order list (its parent's
    /// `Artboard::child_order`, or `root_nodes` if it's a root node) — "Bring
    /// Forward." Goes through `Artboard::move_node_forward` rather than a
    /// direct list edit for the same "the ordering bookkeeping has to move
    /// too" reason `ReparentNode` goes through `Artboard::reparent_node`.
    /// Its own inverse is `SendNodeBackward` *if* the swap actually happened,
    /// or itself unchanged if `node` was already last — re-running the same
    /// boundary check on a no-op is itself a no-op, so no special sentinel
    /// is needed to make undo of a no-op safe.
    MoveNodeForward { artboard: ArtboardId, node: NodeId },
    /// `MoveNodeForward`'s mirror — one step earlier ("Send Backward").
    SendNodeBackward { artboard: ArtboardId, node: NodeId },
    /// Moves `node` to the very end of its draw-order list ("Bring to
    /// Front" — later entries draw on top, per `Artboard::root_nodes`'s own
    /// doc comment). Always actually removes-and-reinserts even if `node`
    /// is already last, so its inverse (`SetNodeIndex`, capturing the exact
    /// prior position) is always well-defined — same "harmless idempotent
    /// inverse" convention `ReparentNode` already uses for "reparent to the
    /// parent it already had".
    BringNodeToFront { artboard: ArtboardId, node: NodeId },
    /// `BringNodeToFront`'s mirror — moves to the very start of the list
    /// ("Send to Back").
    SendNodeToBack { artboard: ArtboardId, node: NodeId },
    /// Restores `node` to an exact position within its current draw-order
    /// list — `BringNodeToFront`/`SendNodeToBack`'s own inverse (undoing
    /// "bring to front" needs the *exact* prior index, not just "one step
    /// back", which is why they don't invert to `SendNodeToBack`/
    /// `BringNodeToFront` the way `MoveNodeForward`/`SendNodeBackward`
    /// invert to each other). Its own inverse is itself, with `value` set to
    /// whatever index `node` had immediately before this was applied.
    SetNodeIndex { artboard: ArtboardId, node: NodeId, value: usize },
    /// Replaces an artboard's whole post-processing effect stack — the
    /// Effects panel's add/remove/reorder/toggle/duplicate-effect action,
    /// all funneling through one command shape for the same "undo only
    /// needs the old whole list" reason `SetStateMachineStates` is one
    /// command too, not one per add/remove/reorder.
    SetPostEffects { artboard: ArtboardId, value: Vec<crate::post_effect::PostEffectLayer> },
    /// Replaces a node's whole attached-behaviors list — the Inspector's
    /// "Add Component"/remove/reorder/toggle actions, all funneling through
    /// one command shape for the same "undo only needs the old whole list"
    /// reason `SetPostEffects` is one command too. Also how a `Script`
    /// instance's own field values get edited: the Inspector reads the
    /// node's current `behaviors`, mutates the one `NodeBehavior::Script`
    /// entry's `ScriptInstance::values` client-side, then issues this with
    /// the whole updated list — same "whole-value-swap, not a narrower
    /// per-field command" reasoning `SetParticleSystem` already uses for its
    /// own nested, multi-field struct.
    SetNodeBehaviors { artboard: ArtboardId, node: NodeId, value: Vec<crate::behavior::NodeBehaviorEntry> },
    /// Inserts a new, named [`ScriptDefinition`] into the document
    /// (document-scoped, not `artboard`-scoped — see that type's own doc
    /// comment for why). Same "doesn't know the freshly assigned id until
    /// `slotmap` inserts" shape as `AddComponent`; its inverse is
    /// `SetScriptDeleted { value: true }`.
    AddScript { definition: ScriptDefinition },
    /// Soft-deletes (`value: true`) or restores (`value: false`) a whole
    /// script definition — same "flag flip, not real removal" reasoning as
    /// `SetComponentDeleted`: undo/redo needs the `ScriptId` to stay valid
    /// for any earlier command (an instance attached from it) still on the
    /// stack.
    SetScriptDeleted { script: ScriptId, value: bool },
    /// Replaces a script definition's whole `fields` list — the Inspector's
    /// script-authoring "add/rename/remove/retype a field" actions, all
    /// funneling through one command shape, same "whole-list swap" reasoning
    /// `SetComponentVariants` already establishes.
    SetScriptFields { script: ScriptId, value: Vec<ScriptFieldDef> },
    /// Replaces a `Camera` node's whole authored config (zoom, background
    /// color, follow target/smoothing) — the Inspector's Camera section,
    /// edited all at once, same "one nested object, not a dozen flattened
    /// sub-commands" shape `SetParticleSystem` uses. Panics via `node_mut`
    /// if `node` isn't a `NodeKind::Camera`.
    SetCamera { artboard: ArtboardId, node: NodeId, value: crate::node::CameraNode },
    /// Names which `Camera` node (`None` clears it, falling back to the
    /// editor's own free-roam pan/zoom) Playground actually views this
    /// artboard through — the Inspector's Camera section's "Set as Active
    /// Camera" action, and `Artboard::active_camera`'s own doc comment for
    /// why this is a saved scene property, not an editor-session preference.
    /// Doesn't validate that `value` is actually a `Camera` node — same
    /// "stale reference degrades gracefully" convention that field's doc
    /// comment already establishes.
    SetActiveCamera { artboard: ArtboardId, value: Option<NodeId> },
    /// Toggles a `Group`'s `GroupNode::y_sort_enabled` — the Inspector's "Y
    /// Sort" checkbox. Panics via `node_mut` if `node` isn't a
    /// `NodeKind::Group`, same convention `SetGroupMask` already has.
    SetGroupYSort { artboard: ArtboardId, node: NodeId, value: bool },
    /// Changes a node's `Node::z_index` — the Inspector's "Z Index" field,
    /// any node kind (unlike `SetGroupYSort`, which only makes sense on a
    /// `Group`, a manual draw-order override is meaningful on anything —
    /// see that field's own doc comment).
    SetNodeZIndex { artboard: ArtboardId, node: NodeId, value: f32 },
    /// Replaces a `Light2D` node's whole settings block — the Inspector's
    /// Light 2D section, same "one nested object" shape as `SetCamera`.
    /// Panics via `node_mut` if `node` isn't a `NodeKind::Light2D`.
    SetLight2D { artboard: ArtboardId, node: NodeId, value: crate::Light2DNode },
    /// Replaces a `Trail` node's whole settings block — the Inspector's
    /// Trail Renderer section. Panics if `node` isn't a `NodeKind::Trail`.
    SetTrail { artboard: ArtboardId, node: NodeId, value: crate::TrailNode },
    /// Adds/edits (`Some`) or removes (`None`) a node's Shadow Caster 2D.
    SetShadowCaster { artboard: ArtboardId, node: NodeId, value: Option<crate::ShadowCaster2D> },
    /// Toggles `Node::unlit` (Unity's unlit sprite material).
    SetNodeUnlit { artboard: ArtboardId, node: NodeId, value: bool },
    /// Changes a scene's Physics 2D gravity (`Artboard::gravity`).
    SetArtboardGravity { artboard: ArtboardId, value: glam::Vec2 },
    /// Inserts a new, empty `Artboard` (a "Scene," in the Scenes panel's own
    /// words) into the document. Carries just the constructor arguments, not
    /// a whole `Artboard` — `Artboard` holds `SlotMap`s, which don't
    /// implement `PartialEq`, so it can't sit inside this `PartialEq` enum
    /// the way `AddNode`'s `Node` payload can. Same "doesn't know the freshly
    /// assigned id until `slotmap` inserts" shape as `AddNode`/`AddComponent`;
    /// its inverse is `SetArtboardDeleted { value: true }`.
    AddArtboard { name: String, width: f32, height: f32 },
    /// Soft-deletes (`value: true`) or restores (`value: false`) a whole
    /// artboard — the Scenes panel's "Delete Scene" action. Same "flag flip,
    /// not real removal" reasoning as `SetComponentDeleted`: undo/redo needs
    /// the `ArtboardId` to stay valid for any earlier command (an edit to one
    /// of its own nodes) still on the stack.
    SetArtboardDeleted { artboard: ArtboardId, value: bool },
    /// Renames an artboard — the Scenes panel's rename action. Swaps in
    /// `value`, captures the old name as its own inverse, same shape as
    /// `SetComponentDefaultFillColor`.
    SetArtboardName { artboard: ArtboardId, value: String },
    /// Replaces a 3D node's payload (transform + content) — the Inspector's
    /// 3D fields. Panics if `node` isn't a `NodeKind::Node3D`.
    SetNode3D { artboard: ArtboardId, node: NodeId, value: crate::Node3D },
    /// Inserts a new [`crate::Material`] (a "New Material" in the Shader
    /// tab). Inverse: `SetMaterialDeleted { value: true }`.
    AddMaterial { material: crate::Material },
    /// Soft-deletes / restores a material.
    SetMaterialDeleted { material: crate::MaterialId, value: bool },
    /// Replaces a material wholesale (a shader-graph edit, a rename).
    SetMaterial { material: crate::MaterialId, value: Box<crate::Material> },
}

/// Shared lookup for every `ComponentDefinition`-targeting command below —
/// same "caller bug, not recoverable" panicking convention as
/// `Document::node_mut`.
fn component_mut(document: &mut Document, component: ComponentId) -> &mut ComponentDefinition {
    document.component_mut(component).expect("Command: component not found")
}

/// Shared lookup for every `ScriptDefinition`-targeting command below — same
/// panicking convention as `component_mut`.
fn script_mut(document: &mut Document, script: ScriptId) -> &mut ScriptDefinition {
    document.script_mut(script).expect("Command: script not found")
}

/// Shared lookup for every `Animation`-targeting command below — panics with
/// the same "caller bug, not recoverable" convention as `Document::node_mut`
/// if either id doesn't resolve.
fn animation_mut<'a>(document: &'a mut Document, artboard: ArtboardId, animation: AnimationId) -> &'a mut Animation {
    document
        .artboard_mut(artboard)
        .expect("Command: artboard not found")
        .animation_mut(animation)
        .expect("Command: animation not found")
}

/// Inserts a keyframe into `track` at `time`, replacing whatever was already
/// there if `time` exactly matches an existing keyframe. Returns the
/// replaced keyframe (`Some`) or `None` for a genuine fresh insert — shared
/// by every `PropertyTrack` variant's `Command::AddKeyframe` branch, since
/// this logic doesn't care what `T` actually is.
fn upsert_keyframe<T: Lerp>(track: &mut Track<T>, time: f32, value: T, easing: Easing) -> Option<Keyframe<T>> {
    match track.keyframes.iter().position(|k| k.time == time) {
        Some(index) => {
            let old = std::mem::replace(&mut track.keyframes[index], Keyframe::new(time, value, easing));
            Some(old)
        }
        None => {
            track.keyframes.push(Keyframe::new(time, value, easing));
            track.keyframes.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
            None
        }
    }
}

/// Shared lookup for every `StateMachineDef`-targeting command below — same
/// panicking convention as `animation_mut`.
fn state_machine_mut<'a>(document: &'a mut Document, artboard: ArtboardId, state_machine: StateMachineId) -> &'a mut StateMachineDef {
    document
        .artboard_mut(artboard)
        .expect("Command: artboard not found")
        .state_machine_mut(state_machine)
        .expect("Command: state machine not found")
}

/// Retimes the keyframe at `old_time` to `new_time` and re-sorts — shared by
/// every `PropertyTrack` variant's `Command::MoveKeyframe` branch, since
/// retiming never touches the keyframe's value, only its time.
fn move_keyframe_time<T: Lerp>(track: &mut Track<T>, old_time: f32, new_time: f32) {
    let index = track
        .keyframes
        .iter()
        .position(|k| k.time == old_time)
        .expect("Command::MoveKeyframe: no keyframe at old_time");
    track.keyframes[index].time = new_time;
    track.keyframes.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
}

impl Command {
    /// Applies this command to `document` and returns the command that
    /// undoes it.
    ///
    /// Panics if `artboard`/`node` don't exist in `document` — a command
    /// referencing a missing node is a caller bug (e.g. a stale IPC request
    /// racing a delete), not a recoverable runtime condition, matching
    /// `AnimationClip::apply`'s same panicking convention in
    /// `an1mbyt3-runtime`.
    pub fn apply(self, document: &mut Document) -> Command {
        match self {
            Command::SetTranslation { artboard, node, value } => {
                let transform = &mut document.node_mut(artboard, node).transform;
                let previous = transform.translation;
                transform.translation = value;
                Command::SetTranslation { artboard, node, value: previous }
            }
            Command::SetRotation { artboard, node, value } => {
                let transform = &mut document.node_mut(artboard, node).transform;
                let previous = transform.rotation;
                transform.rotation = value;
                Command::SetRotation { artboard, node, value: previous }
            }
            Command::SetScale { artboard, node, value } => {
                let transform = &mut document.node_mut(artboard, node).transform;
                let previous = transform.scale;
                transform.scale = value;
                Command::SetScale { artboard, node, value: previous }
            }
            Command::SetAnchor { artboard, node, value } => {
                let transform = &mut document.node_mut(artboard, node).transform;
                let previous = transform.anchor;
                transform.anchor = value;
                Command::SetAnchor { artboard, node, value: previous }
            }
            Command::AddNode { artboard, node } => {
                let artboard_mut = document
                    .artboard_mut(artboard)
                    .expect("Command::AddNode: artboard not found");
                let node_id = artboard_mut.add_node(node);
                Command::SetDeleted { artboard, node: node_id, value: true }
            }
            Command::SetDeleted { artboard, node, value } => {
                let target = &mut document.node_mut(artboard, node).deleted;
                let previous = *target;
                *target = value;
                Command::SetDeleted { artboard, node, value: previous }
            }
            Command::SetNodeVisible { artboard, node, value } => {
                let target = &mut document.node_mut(artboard, node).visible;
                let previous = *target;
                *target = value;
                Command::SetNodeVisible { artboard, node, value: previous }
            }
            Command::SetNodeName { artboard, node, value } => {
                let previous = std::mem::replace(&mut document.node_mut(artboard, node).name, value);
                Command::SetNodeName { artboard, node, value: previous }
            }
            Command::SetNodeId { artboard, node, value } => {
                let previous = std::mem::replace(&mut document.node_mut(artboard, node).id, value);
                Command::SetNodeId { artboard, node, value: previous }
            }
            Command::SetNodeClasses { artboard, node, value } => {
                let previous = std::mem::replace(&mut document.node_mut(artboard, node).classes, value);
                Command::SetNodeClasses { artboard, node, value: previous }
            }
            Command::SetNodeClipChildren { artboard, node, value } => {
                let target = &mut document.node_mut(artboard, node).clip_children;
                let previous = *target;
                *target = value;
                Command::SetNodeClipChildren { artboard, node, value: previous }
            }
            Command::AddAnimation { artboard, animation } => {
                let artboard_mut = document
                    .artboard_mut(artboard)
                    .expect("Command::AddAnimation: artboard not found");
                let animation_id = artboard_mut.add_animation(animation);
                Command::SetAnimationDeleted { artboard, animation: animation_id, value: true }
            }
            Command::SetAnimationDeleted { artboard, animation, value } => {
                let anim = animation_mut(document, artboard, animation);
                let previous = anim.deleted;
                anim.deleted = value;
                Command::SetAnimationDeleted { artboard, animation, value: previous }
            }
            Command::SetAnimationName { artboard, animation, value } => {
                let anim = animation_mut(document, artboard, animation);
                let previous = std::mem::replace(&mut anim.name, value);
                Command::SetAnimationName { artboard, animation, value: previous }
            }
            Command::SetAnimationDuration { artboard, animation, value } => {
                let anim = animation_mut(document, artboard, animation);
                let previous = anim.duration;
                anim.duration = value.max(0.1);
                Command::SetAnimationDuration { artboard, animation, value: previous }
            }
            Command::SetArtboardSize { artboard, width, height } => {
                let artboard_mut = document.artboard_mut(artboard).expect("Command::SetArtboardSize: artboard not found");
                let previous = (artboard_mut.width, artboard_mut.height);
                artboard_mut.width = width.max(1.0);
                artboard_mut.height = height.max(1.0);
                Command::SetArtboardSize { artboard, width: previous.0, height: previous.1 }
            }
            Command::SetArtboardResolutionScale { artboard, value } => {
                let artboard_mut = document.artboard_mut(artboard).expect("Command::SetArtboardResolutionScale: artboard not found");
                let previous = artboard_mut.resolution_scale;
                artboard_mut.resolution_scale = value.clamp(1.0, 64.0);
                Command::SetArtboardResolutionScale { artboard, value: previous }
            }
            Command::SetArtboardScreen { artboard, value } => {
                let artboard_mut = document.artboard_mut(artboard).expect("Command::SetArtboardScreen: artboard not found");
                let previous = std::mem::replace(&mut artboard_mut.screen, value);
                Command::SetArtboardScreen { artboard, value: previous }
            }
            Command::AddKeyframe { artboard, animation, node, time, value, easing } => {
                let property = value.property_kind();
                let anim = animation_mut(document, artboard, animation);
                let existing = anim.tracks.iter().position(|(n, t)| *n == node && t.kind() == property);

                match existing {
                    Some(index) => {
                        let (_, track) = &mut anim.tracks[index];
                        match track {
                            PropertyTrack::Translation(t) => {
                                let KeyframeValue::Translation(v) = value else { unreachable!("property_kind mismatch") };
                                match upsert_keyframe(t, time, v, easing) {
                                    Some(old) => Command::AddKeyframe {
                                        artboard, animation, node, time: old.time,
                                        value: KeyframeValue::Translation(old.value), easing: old.easing,
                                    },
                                    None => Command::RemoveKeyframe { artboard, animation, node, property, time },
                                }
                            }
                            PropertyTrack::Rotation(t) => {
                                let KeyframeValue::Rotation(v) = value else { unreachable!("property_kind mismatch") };
                                match upsert_keyframe(t, time, v, easing) {
                                    Some(old) => Command::AddKeyframe {
                                        artboard, animation, node, time: old.time,
                                        value: KeyframeValue::Rotation(old.value), easing: old.easing,
                                    },
                                    None => Command::RemoveKeyframe { artboard, animation, node, property, time },
                                }
                            }
                            PropertyTrack::Scale(t) => {
                                let KeyframeValue::Scale(v) = value else { unreachable!("property_kind mismatch") };
                                match upsert_keyframe(t, time, v, easing) {
                                    Some(old) => Command::AddKeyframe {
                                        artboard, animation, node, time: old.time,
                                        value: KeyframeValue::Scale(old.value), easing: old.easing,
                                    },
                                    None => Command::RemoveKeyframe { artboard, animation, node, property, time },
                                }
                            }
                            PropertyTrack::FillColor(t) => {
                                let KeyframeValue::FillColor(v) = value else { unreachable!("property_kind mismatch") };
                                match upsert_keyframe(t, time, v, easing) {
                                    Some(old) => Command::AddKeyframe {
                                        artboard, animation, node, time: old.time,
                                        value: KeyframeValue::FillColor(old.value), easing: old.easing,
                                    },
                                    None => Command::RemoveKeyframe { artboard, animation, node, property, time },
                                }
                            }
                            PropertyTrack::Path(t) => {
                                let KeyframeValue::Path(v) = value else { unreachable!("property_kind mismatch") };
                                match upsert_keyframe(t, time, v, easing) {
                                    Some(old) => Command::AddKeyframe {
                                        artboard, animation, node, time: old.time,
                                        value: KeyframeValue::Path(old.value), easing: old.easing,
                                    },
                                    None => Command::RemoveKeyframe { artboard, animation, node, property, time },
                                }
                            }
                            PropertyTrack::MotionPath { guide: _, track: t } => {
                                let KeyframeValue::MotionPath(v) = value else { unreachable!("property_kind mismatch") };
                                match upsert_keyframe(t, time, v, easing) {
                                    Some(old) => Command::AddKeyframe {
                                        artboard, animation, node, time: old.time,
                                        value: KeyframeValue::MotionPath(old.value), easing: old.easing,
                                    },
                                    None => Command::RemoveKeyframe { artboard, animation, node, property, time },
                                }
                            }
                        }
                    }
                    None => {
                        let new_track = match value {
                            KeyframeValue::Translation(v) => PropertyTrack::Translation(Track::new(vec![Keyframe::new(time, v, easing)])),
                            KeyframeValue::Rotation(v) => PropertyTrack::Rotation(Track::new(vec![Keyframe::new(time, v, easing)])),
                            KeyframeValue::Scale(v) => PropertyTrack::Scale(Track::new(vec![Keyframe::new(time, v, easing)])),
                            KeyframeValue::FillColor(v) => PropertyTrack::FillColor(Track::new(vec![Keyframe::new(time, v, easing)])),
                            KeyframeValue::Path(v) => PropertyTrack::Path(Track::new(vec![Keyframe::new(time, v, easing)])),
                            KeyframeValue::MotionPath(_) => panic!(
                                "Command::AddKeyframe: no MotionPath track exists for this node yet — call SetMotionPathGuide first"
                            ),
                        };
                        anim.tracks.push((node, new_track));
                        Command::RemoveKeyframe { artboard, animation, node, property, time }
                    }
                }
            }
            Command::RemoveKeyframe { artboard, animation, node, property, time } => {
                let anim = animation_mut(document, artboard, animation);
                let index = anim
                    .tracks
                    .iter()
                    .position(|(n, t)| *n == node && t.kind() == property)
                    .expect("Command::RemoveKeyframe: no track for this node/property");

                let (value, removed_time, removed_easing, empty) = {
                    let (_, track) = &mut anim.tracks[index];
                    match track {
                        PropertyTrack::Translation(t) => {
                            let i = t.keyframes.iter().position(|k| k.time == time).expect("Command::RemoveKeyframe: no keyframe at the given time");
                            let kf = t.keyframes.remove(i);
                            (KeyframeValue::Translation(kf.value), kf.time, kf.easing, t.keyframes.is_empty())
                        }
                        PropertyTrack::Rotation(t) => {
                            let i = t.keyframes.iter().position(|k| k.time == time).expect("Command::RemoveKeyframe: no keyframe at the given time");
                            let kf = t.keyframes.remove(i);
                            (KeyframeValue::Rotation(kf.value), kf.time, kf.easing, t.keyframes.is_empty())
                        }
                        PropertyTrack::Scale(t) => {
                            let i = t.keyframes.iter().position(|k| k.time == time).expect("Command::RemoveKeyframe: no keyframe at the given time");
                            let kf = t.keyframes.remove(i);
                            (KeyframeValue::Scale(kf.value), kf.time, kf.easing, t.keyframes.is_empty())
                        }
                        PropertyTrack::FillColor(t) => {
                            let i = t.keyframes.iter().position(|k| k.time == time).expect("Command::RemoveKeyframe: no keyframe at the given time");
                            let kf = t.keyframes.remove(i);
                            (KeyframeValue::FillColor(kf.value), kf.time, kf.easing, t.keyframes.is_empty())
                        }
                        PropertyTrack::Path(t) => {
                            let i = t.keyframes.iter().position(|k| k.time == time).expect("Command::RemoveKeyframe: no keyframe at the given time");
                            let kf = t.keyframes.remove(i);
                            (KeyframeValue::Path(kf.value), kf.time, kf.easing, t.keyframes.is_empty())
                        }
                        PropertyTrack::MotionPath { track: t, .. } => {
                            let i = t.keyframes.iter().position(|k| k.time == time).expect("Command::RemoveKeyframe: no keyframe at the given time");
                            let kf = t.keyframes.remove(i);
                            (KeyframeValue::MotionPath(kf.value), kf.time, kf.easing, t.keyframes.is_empty())
                        }
                    }
                };

                if empty {
                    anim.tracks.remove(index);
                }
                Command::AddKeyframe { artboard, animation, node, time: removed_time, value, easing: removed_easing }
            }
            Command::MoveKeyframe { artboard, animation, node, property, old_time, new_time } => {
                let anim = animation_mut(document, artboard, animation);
                let index = anim
                    .tracks
                    .iter()
                    .position(|(n, t)| *n == node && t.kind() == property)
                    .expect("Command::MoveKeyframe: no track for this node/property");
                let (_, track) = &mut anim.tracks[index];
                match track {
                    PropertyTrack::Translation(t) => move_keyframe_time(t, old_time, new_time),
                    PropertyTrack::Rotation(t) => move_keyframe_time(t, old_time, new_time),
                    PropertyTrack::Scale(t) => move_keyframe_time(t, old_time, new_time),
                    PropertyTrack::FillColor(t) => move_keyframe_time(t, old_time, new_time),
                    PropertyTrack::Path(t) => move_keyframe_time(t, old_time, new_time),
                    PropertyTrack::MotionPath { track: t, .. } => move_keyframe_time(t, old_time, new_time),
                }
                Command::MoveKeyframe { artboard, animation, node, property, old_time: new_time, new_time: old_time }
            }
            Command::ShiftPropertyKeyframes { artboard, node, property, delta } => {
                let artboard_mut = document
                    .artboard_mut(artboard)
                    .expect("Command::ShiftPropertyKeyframes: artboard not found");
                for (_, animation) in artboard_mut.animations_mut() {
                    for (track_node, track) in animation.tracks.iter_mut() {
                        if *track_node != node || track.kind() != property {
                            continue;
                        }
                        match (track, &delta) {
                            (PropertyTrack::Translation(t), KeyframeValue::Translation(d)) => {
                                for k in t.keyframes.iter_mut() {
                                    k.value += *d;
                                }
                            }
                            (PropertyTrack::Rotation(t), KeyframeValue::Rotation(d)) => {
                                for k in t.keyframes.iter_mut() {
                                    k.value += *d;
                                }
                            }
                            (PropertyTrack::Scale(t), KeyframeValue::Scale(d)) => {
                                for k in t.keyframes.iter_mut() {
                                    k.value += *d;
                                }
                            }
                            (PropertyTrack::FillColor(t), KeyframeValue::FillColor(d)) => {
                                for k in t.keyframes.iter_mut() {
                                    for i in 0..4 {
                                        k.value[i] += d[i];
                                    }
                                }
                            }
                            _ => unreachable!("ShiftPropertyKeyframes: delta's property must match track.kind() by construction"),
                        }
                    }
                }
                Command::ShiftPropertyKeyframes { artboard, node, property, delta: delta.negate() }
            }
            Command::SetMotionPathGuide { artboard, animation, node, guide } => {
                let anim = animation_mut(document, artboard, animation);
                let existing = anim.tracks.iter().position(|(n, t)| *n == node && matches!(t, PropertyTrack::MotionPath { .. }));
                match existing {
                    Some(index) => {
                        let (_, track) = &mut anim.tracks[index];
                        let PropertyTrack::MotionPath { guide: old_guide, .. } = track else {
                            unreachable!("position() already filtered to MotionPath tracks")
                        };
                        let previous = std::mem::replace(old_guide, guide);
                        Command::SetMotionPathGuide { artboard, animation, node, guide: previous }
                    }
                    None => {
                        anim.tracks.push((node, PropertyTrack::MotionPath { guide, track: Track::new(Vec::new()) }));
                        Command::RemoveMotionPathTrack { artboard, animation, node }
                    }
                }
            }
            Command::RemoveMotionPathTrack { artboard, animation, node } => {
                let anim = animation_mut(document, artboard, animation);
                let index = anim
                    .tracks
                    .iter()
                    .position(|(n, t)| *n == node && matches!(t, PropertyTrack::MotionPath { .. }))
                    .expect("Command::RemoveMotionPathTrack: no MotionPath track for this node");
                let (_, track) = anim.tracks.remove(index);
                let PropertyTrack::MotionPath { guide, .. } = track else {
                    unreachable!("position() already filtered to MotionPath tracks")
                };
                Command::SetMotionPathGuide { artboard, animation, node, guide }
            }
            Command::Batch(commands) => {
                let inverses: Vec<Command> = commands.into_iter().map(|c| c.apply(document)).collect();
                Command::Batch(inverses.into_iter().rev().collect())
            }
            Command::AddStateMachine { artboard, state_machine } => {
                let artboard_mut = document
                    .artboard_mut(artboard)
                    .expect("Command::AddStateMachine: artboard not found");
                let state_machine_id = artboard_mut.add_state_machine(state_machine);
                Command::SetStateMachineDeleted { artboard, state_machine: state_machine_id, value: true }
            }
            Command::SetStateMachineDeleted { artboard, state_machine, value } => {
                let sm = state_machine_mut(document, artboard, state_machine);
                let previous = sm.deleted;
                sm.deleted = value;
                Command::SetStateMachineDeleted { artboard, state_machine, value: previous }
            }
            Command::SetStateMachineInitialState { artboard, state_machine, value } => {
                let sm = state_machine_mut(document, artboard, state_machine);
                let previous = std::mem::replace(&mut sm.initial_state, value);
                Command::SetStateMachineInitialState { artboard, state_machine, value: previous }
            }
            Command::SetStateMachineInputs { artboard, state_machine, value } => {
                let sm = state_machine_mut(document, artboard, state_machine);
                let previous = std::mem::replace(&mut sm.inputs, value);
                Command::SetStateMachineInputs { artboard, state_machine, value: previous }
            }
            Command::SetStateMachineStates { artboard, state_machine, value } => {
                let sm = state_machine_mut(document, artboard, state_machine);
                let previous = std::mem::replace(&mut sm.states, value);
                Command::SetStateMachineStates { artboard, state_machine, value: previous }
            }
            Command::SetStateMachineLayers { artboard, state_machine, value } => {
                let sm = state_machine_mut(document, artboard, state_machine);
                let previous = std::mem::replace(&mut sm.layers, value);
                Command::SetStateMachineLayers { artboard, state_machine, value: previous }
            }
            Command::SetStatePosition { artboard, state_machine, state_name, value } => {
                let state = state_machine_mut(document, artboard, state_machine)
                    .states
                    .iter_mut()
                    .find(|s| s.name == state_name)
                    .expect("Command::SetStatePosition: no state with this name");
                let previous = state.position;
                state.position = value;
                Command::SetStatePosition { artboard, state_machine, state_name, value: previous }
            }
            Command::SetStateMachineName { artboard, state_machine, value } => {
                let previous = std::mem::replace(&mut state_machine_mut(document, artboard, state_machine).name, value);
                Command::SetStateMachineName { artboard, state_machine, value: previous }
            }
            Command::SetBoneLength { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let NodeKind::Bone(bone) = &mut target.kind else {
                    panic!("Command::SetBoneLength: node is not a Bone");
                };
                let previous = bone.length;
                bone.length = value;
                Command::SetBoneLength { artboard, node, value: previous }
            }
            Command::SetShapeBlendMode { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let NodeKind::Shape(shape) = &mut target.kind else {
                    panic!("Command::SetShapeBlendMode: node is not a Shape");
                };
                let previous = shape.blend_mode;
                shape.blend_mode = value;
                Command::SetShapeBlendMode { artboard, node, value: previous }
            }
            Command::SetFillColor { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let previous = match target.fill_color_mut() {
                    Some(color) => std::mem::replace(color, value),
                    None => panic!("Command::SetFillColor: node has no fill color (Shape/Text/Image) or Light 2D"),
                };
                Command::SetFillColor { artboard, node, value: previous }
            }
            Command::SetGradient { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let previous = match &mut target.kind {
                    NodeKind::Shape(shape) => std::mem::replace(&mut shape.gradient, value),
                    NodeKind::Text(text) => std::mem::replace(&mut text.gradient, value),
                    NodeKind::Group(_) | NodeKind::Bone(_) | NodeKind::Instance(_) | NodeKind::Image(_) | NodeKind::ParticleSystem(_) | NodeKind::Tilemap(_) | NodeKind::Camera(_) | NodeKind::Light2D(_) | NodeKind::Trail(_) | NodeKind::Node3D(_) => {
                        panic!("Command::SetGradient: node is neither a Shape nor a Text")
                    }
                };
                Command::SetGradient { artboard, node, value: previous }
            }
            Command::SetShapeImage { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let NodeKind::Shape(shape) = &mut target.kind else {
                    panic!("Command::SetShapeImage: node is not a Shape");
                };
                let previous = std::mem::replace(&mut shape.image, value);
                Command::SetShapeImage { artboard, node, value: previous }
            }
            Command::SetShapePath { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let NodeKind::Shape(shape) = &mut target.kind else {
                    panic!("Command::SetShapePath: node is not a Shape");
                };
                let previous = std::mem::replace(&mut shape.path, value);
                Command::SetShapePath { artboard, node, value: previous }
            }
            Command::SetShapeStroke { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let NodeKind::Shape(shape) = &mut target.kind else {
                    panic!("Command::SetShapeStroke: node is not a Shape");
                };
                let previous = std::mem::replace(&mut shape.stroke, value);
                Command::SetShapeStroke { artboard, node, value: previous }
            }
            Command::SetParticleSystem { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let Some(particle_system) = target.particle_system_mut() else {
                    panic!("Command::SetParticleSystem: node has no Particle System");
                };
                let previous = std::mem::replace(particle_system, value);
                Command::SetParticleSystem { artboard, node, value: previous }
            }
            Command::SetTile { artboard, node, x, y, value } => {
                let target = document.node_mut(artboard, node);
                let NodeKind::Tilemap(tilemap) = &mut target.kind else {
                    panic!("Command::SetTile: node is not a Tilemap");
                };
                let previous = tilemap.get(x, y);
                tilemap.set(x, y, value);
                Command::SetTile { artboard, node, x, y, value: previous }
            }
            Command::SetTilemapGrid { artboard, node, map_width, map_height, tiles } => {
                let target = document.node_mut(artboard, node);
                let NodeKind::Tilemap(tilemap) = &mut target.kind else {
                    panic!("Command::SetTilemapGrid: node is not a Tilemap");
                };
                let previous_width = std::mem::replace(&mut tilemap.map_width, map_width);
                let previous_height = std::mem::replace(&mut tilemap.map_height, map_height);
                let previous_tiles = std::mem::replace(&mut tilemap.tiles, tiles);
                Command::SetTilemapGrid { artboard, node, map_width: previous_width, map_height: previous_height, tiles: previous_tiles }
            }
            Command::SetTilemapCellSize { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let NodeKind::Tilemap(tilemap) = &mut target.kind else {
                    panic!("Command::SetTilemapCellSize: node is not a Tilemap");
                };
                let previous = std::mem::replace(&mut tilemap.cell_size, value);
                Command::SetTilemapCellSize { artboard, node, value: previous }
            }
            Command::SetTextContent { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let NodeKind::Text(text) = &mut target.kind else {
                    panic!("Command::SetTextContent: node is not a Text");
                };
                let previous = std::mem::replace(&mut text.content, value);
                Command::SetTextContent { artboard, node, value: previous }
            }
            Command::SetTextSize { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let NodeKind::Text(text) = &mut target.kind else {
                    panic!("Command::SetTextSize: node is not a Text");
                };
                let previous = text.size_px;
                text.size_px = value;
                Command::SetTextSize { artboard, node, value: previous }
            }
            Command::AddComponent { definition } => {
                let component_id = document.add_component(definition);
                Command::SetComponentDeleted { component: component_id, value: true }
            }
            Command::AddPrefab { definition } => {
                let prefab = document.add_prefab(definition);
                Command::SetPrefabDeleted { prefab, value: true }
            }
            Command::SetPrefabDeleted { prefab, value } => {
                let target = &mut document.prefab_mut(prefab).expect("Command: prefab not found").deleted;
                let previous = std::mem::replace(target, value);
                Command::SetPrefabDeleted { prefab, value: previous }
            }
            Command::SetPrefab { prefab, value } => {
                let target = document.prefab_mut(prefab).expect("Command: prefab not found");
                let previous = std::mem::replace(target, *value);
                Command::SetPrefab { prefab, value: Box::new(previous) }
            }
            Command::ReplaceNode { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let mut value = *value;
                value.parent = target.parent;
                value.deleted = target.deleted;
                let previous = std::mem::replace(target, value);
                Command::ReplaceNode { artboard, node, value: Box::new(previous) }
            }
            Command::SetComponentDeleted { component, value } => {
                let target = &mut component_mut(document, component).deleted;
                let previous = *target;
                *target = value;
                Command::SetComponentDeleted { component, value: previous }
            }
            Command::SetComponentDefaultFillColor { component, value } => {
                let previous = std::mem::replace(&mut component_mut(document, component).default_fill_color, value);
                Command::SetComponentDefaultFillColor { component, value: previous }
            }
            Command::SetComponentVariants { component, value } => {
                let previous = std::mem::replace(&mut component_mut(document, component).variants, value);
                Command::SetComponentVariants { component, value: previous }
            }
            Command::SetInstanceFillColorOverride { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let NodeKind::Instance(instance) = &mut target.kind else {
                    panic!("Command::SetInstanceFillColorOverride: node is not an Instance");
                };
                let previous = std::mem::replace(&mut instance.fill_color_override, value);
                Command::SetInstanceFillColorOverride { artboard, node, value: previous }
            }
            Command::SetInstanceVariant { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let NodeKind::Instance(instance) = &mut target.kind else {
                    panic!("Command::SetInstanceVariant: node is not an Instance");
                };
                let previous = std::mem::replace(&mut instance.variant, value);
                Command::SetInstanceVariant { artboard, node, value: previous }
            }
            Command::SetStateTransitions { artboard, state_machine, state_name, value } => {
                let sm = state_machine_mut(document, artboard, state_machine);
                let state = sm
                    .states
                    .iter_mut()
                    .find(|s| s.name == state_name)
                    .expect("Command::SetStateTransitions: no state with this name");
                let previous = std::mem::replace(&mut state.transitions, value);
                Command::SetStateTransitions { artboard, state_machine, state_name, value: previous }
            }
            Command::SetStateProgram { artboard, state_machine, state_name, value } => {
                let sm = state_machine_mut(document, artboard, state_machine);
                let state = sm
                    .states
                    .iter_mut()
                    .find(|s| s.name == state_name)
                    .expect("Command::SetStateProgram: no state with this name");
                let previous = std::mem::replace(&mut state.program, value);
                Command::SetStateProgram { artboard, state_machine, state_name, value: previous }
            }
            Command::ReparentNode { artboard, node, value } => {
                let artboard_mut = document
                    .artboard_mut(artboard)
                    .expect("Command::ReparentNode: artboard not found");
                let previous = artboard_mut.node(node).expect("Command::ReparentNode: node not found").parent;
                artboard_mut.reparent_node(node, value);
                Command::ReparentNode { artboard, node, value: previous }
            }
            Command::ReparentNodeAt { artboard, node, parent, index } => {
                let artboard_mut = document
                    .artboard_mut(artboard)
                    .expect("Command::ReparentNodeAt: artboard not found");
                let previous_parent = artboard_mut.node(node).expect("Command::ReparentNodeAt: node not found").parent;
                let previous_index = artboard_mut.node_order_index(node).unwrap_or(0);
                artboard_mut.reparent_node_at(node, parent, index);
                Command::ReparentNodeAt { artboard, node, parent: previous_parent, index: previous_index }
            }
            Command::SetGroupMask { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let NodeKind::Group(group) = &mut target.kind else {
                    panic!("Command::SetGroupMask: node is not a Group");
                };
                let previous = std::mem::replace(&mut group.mask, value);
                Command::SetGroupMask { artboard, node, value: previous }
            }
            Command::MoveNodeForward { artboard, node } => {
                let artboard_mut = document.artboard_mut(artboard).expect("Command::MoveNodeForward: artboard not found");
                if artboard_mut.move_node_forward(node) {
                    Command::SendNodeBackward { artboard, node }
                } else {
                    Command::MoveNodeForward { artboard, node }
                }
            }
            Command::SendNodeBackward { artboard, node } => {
                let artboard_mut = document.artboard_mut(artboard).expect("Command::SendNodeBackward: artboard not found");
                if artboard_mut.move_node_backward(node) {
                    Command::MoveNodeForward { artboard, node }
                } else {
                    Command::SendNodeBackward { artboard, node }
                }
            }
            Command::BringNodeToFront { artboard, node } => {
                let artboard_mut = document.artboard_mut(artboard).expect("Command::BringNodeToFront: artboard not found");
                let previous_index = artboard_mut.bring_node_to_front(node);
                Command::SetNodeIndex { artboard, node, value: previous_index }
            }
            Command::SendNodeToBack { artboard, node } => {
                let artboard_mut = document.artboard_mut(artboard).expect("Command::SendNodeToBack: artboard not found");
                let previous_index = artboard_mut.send_node_to_back(node);
                Command::SetNodeIndex { artboard, node, value: previous_index }
            }
            Command::SetNodeIndex { artboard, node, value } => {
                let artboard_mut = document.artboard_mut(artboard).expect("Command::SetNodeIndex: artboard not found");
                let previous_index = artboard_mut.node_order_index(node).expect("Command::SetNodeIndex: node not found in its own ordering list");
                artboard_mut.set_node_index(node, value);
                Command::SetNodeIndex { artboard, node, value: previous_index }
            }
            Command::SetPostEffects { artboard, value } => {
                let artboard_mut = document.artboard_mut(artboard).expect("Command::SetPostEffects: artboard not found");
                let previous = std::mem::replace(artboard_mut.post_effects_mut(), value);
                Command::SetPostEffects { artboard, value: previous }
            }
            Command::SetNodeBehaviors { artboard, node, value } => {
                let previous = std::mem::replace(&mut document.node_mut(artboard, node).behaviors, value);
                Command::SetNodeBehaviors { artboard, node, value: previous }
            }
            Command::AddScript { definition } => {
                let script_id = document.add_script(definition);
                Command::SetScriptDeleted { script: script_id, value: true }
            }
            Command::SetScriptDeleted { script, value } => {
                let target = &mut script_mut(document, script).deleted;
                let previous = *target;
                *target = value;
                Command::SetScriptDeleted { script, value: previous }
            }
            Command::SetScriptFields { script, value } => {
                let previous = std::mem::replace(&mut script_mut(document, script).fields, value);
                Command::SetScriptFields { script, value: previous }
            }
            Command::SetCamera { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let Some(camera) = target.camera_mut() else {
                    panic!("Command::SetCamera: node has no Camera");
                };
                let previous = std::mem::replace(camera, value);
                Command::SetCamera { artboard, node, value: previous }
            }
            Command::SetActiveCamera { artboard, value } => {
                let artboard_mut = document.artboard_mut(artboard).expect("Command::SetActiveCamera: artboard not found");
                let previous = std::mem::replace(artboard_mut.active_camera_mut(), value);
                Command::SetActiveCamera { artboard, value: previous }
            }
            Command::SetGroupYSort { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let NodeKind::Group(group) = &mut target.kind else {
                    panic!("Command::SetGroupYSort: node is not a Group");
                };
                let previous = std::mem::replace(&mut group.y_sort_enabled, value);
                Command::SetGroupYSort { artboard, node, value: previous }
            }
            Command::SetNodeZIndex { artboard, node, value } => {
                let previous = std::mem::replace(&mut document.node_mut(artboard, node).z_index, value);
                Command::SetNodeZIndex { artboard, node, value: previous }
            }
            Command::SetLight2D { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let Some(light) = target.light2d_mut() else {
                    panic!("Command::SetLight2D: node has no Light 2D");
                };
                let previous = std::mem::replace(light, value);
                Command::SetLight2D { artboard, node, value: previous }
            }
            Command::SetTrail { artboard, node, value } => {
                let target = document.node_mut(artboard, node);
                let Some(trail) = target.trail_mut() else {
                    panic!("Command::SetTrail: node has no Trail Renderer");
                };
                let previous = std::mem::replace(trail, value);
                Command::SetTrail { artboard, node, value: previous }
            }
            Command::SetShadowCaster { artboard, node, value } => {
                // Shadow Caster 2D is a component: replace (or remove) it,
                // folding in a legacy `shadow_caster` field if one's there.
                let target = document.node_mut(artboard, node);
                let mut previous = target.shadow_caster.take();
                if let Some(index) = target.behaviors.iter().position(|e| matches!(e.behavior, crate::NodeBehavior::ShadowCaster2D(_))) {
                    if let crate::NodeBehavior::ShadowCaster2D(caster) = target.behaviors.remove(index).behavior {
                        previous = previous.or(Some(caster));
                    }
                }
                if let Some(caster) = value {
                    target.behaviors.push(crate::NodeBehaviorEntry { behavior: crate::NodeBehavior::ShadowCaster2D(caster), enabled: true });
                }
                Command::SetShadowCaster { artboard, node, value: previous }
            }
            Command::SetArtboardGravity { artboard, value } => {
                let target = &mut document.artboard_mut(artboard).expect("Command::SetArtboardGravity: artboard not found").gravity;
                let previous = std::mem::replace(target, value);
                Command::SetArtboardGravity { artboard, value: previous }
            }
            Command::SetNodeUnlit { artboard, node, value } => {
                let previous = std::mem::replace(&mut document.node_mut(artboard, node).unlit, value);
                Command::SetNodeUnlit { artboard, node, value: previous }
            }
            Command::AddArtboard { name, width, height } => {
                let artboard_id = document.add_artboard(Artboard::new(name, width, height));
                Command::SetArtboardDeleted { artboard: artboard_id, value: true }
            }
            Command::SetArtboardDeleted { artboard, value } => {
                let target = &mut document.artboard_mut(artboard).expect("Command::SetArtboardDeleted: artboard not found").deleted;
                let previous = *target;
                *target = value;
                Command::SetArtboardDeleted { artboard, value: previous }
            }
            Command::SetNode3D { artboard, node, value } => {
                let NodeKind::Node3D(target) = &mut document.node_mut(artboard, node).kind else { panic!("Command::SetNode3D: node is not a Node3D") };
                let previous = std::mem::replace(target, value);
                Command::SetNode3D { artboard, node, value: previous }
            }
            Command::AddMaterial { material } => {
                let material = document.add_material(material);
                Command::SetMaterialDeleted { material, value: true }
            }
            Command::SetMaterialDeleted { material, value } => {
                let target = &mut document.material_mut(material).expect("Command: material not found").deleted;
                let previous = std::mem::replace(target, value);
                Command::SetMaterialDeleted { material, value: previous }
            }
            Command::SetMaterial { material, value } => {
                let target = document.material_mut(material).expect("Command: material not found");
                let previous = std::mem::replace(target, *value);
                Command::SetMaterial { material, value: Box::new(previous) }
            }
            Command::SetArtboardName { artboard, value } => {
                let previous = std::mem::replace(
                    &mut document.artboard_mut(artboard).expect("Command::SetArtboardName: artboard not found").name,
                    value,
                );
                Command::SetArtboardName { artboard, value: previous }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Artboard, FontAsset, GradientDef, GradientKind, GradientStop, Node, NodeKind, Path, ShapeNode, TextNode};

    fn document_with_one_node() -> (Document, ArtboardId, NodeId) {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let artboard = document.artboard_mut(artboard_id).unwrap();
        let node_id = artboard.add_node(Node::new(
            "Square",
            None,
            NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
        ));
        (document, artboard_id, node_id)
    }

    /// A document with one node and one empty named animation, ready for
    /// keyframe commands to target.
    fn document_with_one_node_and_animation() -> (Document, ArtboardId, NodeId, AnimationId) {
        let (mut document, artboard_id, node_id) = document_with_one_node();
        let animation_id = document.artboard_mut(artboard_id).unwrap().add_animation(Animation::new("Animation 1"));
        (document, artboard_id, node_id, animation_id)
    }

    #[test]
    fn apply_sets_the_value_and_returns_the_inverse() {
        let (mut document, artboard_id, node_id) = document_with_one_node();

        let inverse = Command::SetTranslation {
            artboard: artboard_id,
            node: node_id,
            value: Vec2::new(10.0, 20.0),
        }
        .apply(&mut document);

        assert_eq!(
            document.artboard(artboard_id).unwrap().node(node_id).unwrap().transform.translation,
            Vec2::new(10.0, 20.0)
        );
        assert_eq!(
            inverse,
            Command::SetTranslation { artboard: artboard_id, node: node_id, value: Vec2::ZERO }
        );
    }

    #[test]
    fn applying_the_inverse_restores_the_original_value() {
        let (mut document, artboard_id, node_id) = document_with_one_node();

        let inverse = Command::SetTranslation {
            artboard: artboard_id,
            node: node_id,
            value: Vec2::new(10.0, 20.0),
        }
        .apply(&mut document);
        inverse.apply(&mut document);

        assert_eq!(
            document.artboard(artboard_id).unwrap().node(node_id).unwrap().transform.translation,
            Vec2::ZERO
        );
    }

    #[test]
    fn add_node_inserts_it_and_its_inverse_soft_deletes_it() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));

        let new_node = Node::new(
            "Circle",
            None,
            NodeKind::Shape(ShapeNode { path: Path::rect(20.0, 20.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
        );
        let inverse = Command::AddNode { artboard: artboard_id, node: new_node }.apply(&mut document);

        let artboard = document.artboard(artboard_id).unwrap();
        assert_eq!(artboard.root_nodes().len(), 1);
        let node_id = artboard.root_nodes()[0];
        assert!(!artboard.node(node_id).unwrap().deleted);

        let Command::SetDeleted { artboard: inv_artboard, node: inv_node, value } = inverse else {
            panic!("AddNode's inverse must be a SetDeleted command");
        };
        assert_eq!(inv_artboard, artboard_id);
        assert_eq!(inv_node, node_id);
        assert!(value, "undoing a creation must soft-delete the created node");
    }

    #[test]
    fn undoing_then_redoing_a_creation_round_trips_through_document() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));

        document.apply(Command::AddNode {
            artboard: artboard_id,
            node: Node::new(
                "Circle",
                None,
                NodeKind::Shape(ShapeNode { path: Path::rect(20.0, 20.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
            ),
        });
        let node_id = document.artboard(artboard_id).unwrap().root_nodes()[0];
        assert!(!document.artboard(artboard_id).unwrap().node(node_id).unwrap().deleted);

        assert!(document.undo());
        assert!(document.artboard(artboard_id).unwrap().node(node_id).unwrap().deleted);

        assert!(document.redo());
        assert!(!document.artboard(artboard_id).unwrap().node(node_id).unwrap().deleted);
    }

    #[test]
    fn deleting_then_undoing_restores_the_same_node_id() {
        let (mut document, artboard_id, node_id) = document_with_one_node();

        document.apply(Command::SetDeleted { artboard: artboard_id, node: node_id, value: true });
        assert!(document.artboard(artboard_id).unwrap().node(node_id).unwrap().deleted);

        assert!(document.undo());
        assert!(!document.artboard(artboard_id).unwrap().node(node_id).unwrap().deleted);
    }

    #[test]
    fn set_node_name_and_undo_restores_the_old_name() {
        let (mut document, artboard_id, node_id) = document_with_one_node();

        document.apply(Command::SetNodeName { artboard: artboard_id, node: node_id, value: "Renamed".into() });
        assert_eq!(document.artboard(artboard_id).unwrap().node(node_id).unwrap().name, "Renamed");

        assert!(document.undo());
        assert_eq!(document.artboard(artboard_id).unwrap().node(node_id).unwrap().name, "Square");
    }

    #[test]
    fn add_animation_inserts_it_and_its_inverse_soft_deletes_it() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));

        let inverse = Command::AddAnimation { artboard: artboard_id, animation: Animation::new("Walk") }.apply(&mut document);
        let (animation_id, animation) = document.artboard(artboard_id).unwrap().animations().next().unwrap();
        assert_eq!(animation.name, "Walk");

        let Command::SetAnimationDeleted { animation: inv_animation, value, .. } = inverse else {
            panic!("AddAnimation's inverse must be a SetAnimationDeleted command");
        };
        assert_eq!(inv_animation, animation_id);
        assert!(value);
    }

    #[test]
    fn rename_animation_and_undo_restores_the_old_name() {
        let (mut document, artboard_id, _node_id, animation_id) = document_with_one_node_and_animation();

        document.apply(Command::SetAnimationName { artboard: artboard_id, animation: animation_id, value: "Bounce".into() });
        assert_eq!(document.artboard(artboard_id).unwrap().animation(animation_id).unwrap().name, "Bounce");

        assert!(document.undo());
        assert_eq!(document.artboard(artboard_id).unwrap().animation(animation_id).unwrap().name, "Animation 1");
    }

    #[test]
    fn set_animation_duration_and_undo_restores_the_old_value() {
        let (mut document, artboard_id, _node_id, animation_id) = document_with_one_node_and_animation();
        let original = document.artboard(artboard_id).unwrap().animation(animation_id).unwrap().duration;

        document.apply(Command::SetAnimationDuration { artboard: artboard_id, animation: animation_id, value: 12.0 });
        assert_eq!(document.artboard(artboard_id).unwrap().animation(animation_id).unwrap().duration, 12.0);

        assert!(document.undo());
        assert_eq!(document.artboard(artboard_id).unwrap().animation(animation_id).unwrap().duration, original);
    }

    #[test]
    fn set_animation_duration_clamps_to_a_positive_minimum() {
        let (mut document, artboard_id, _node_id, animation_id) = document_with_one_node_and_animation();
        document.apply(Command::SetAnimationDuration { artboard: artboard_id, animation: animation_id, value: -5.0 });
        assert_eq!(document.artboard(artboard_id).unwrap().animation(animation_id).unwrap().duration, 0.1);
    }

    #[test]
    fn set_artboard_size_and_undo_restores_the_old_size() {
        let (mut document, artboard_id, _node_id, _animation_id) = document_with_one_node_and_animation();
        let original = (document.artboard(artboard_id).unwrap().width, document.artboard(artboard_id).unwrap().height);

        document.apply(Command::SetArtboardSize { artboard: artboard_id, width: 800.0, height: 600.0 });
        assert_eq!(document.artboard(artboard_id).unwrap().width, 800.0);
        assert_eq!(document.artboard(artboard_id).unwrap().height, 600.0);

        assert!(document.undo());
        assert_eq!(document.artboard(artboard_id).unwrap().width, original.0);
        assert_eq!(document.artboard(artboard_id).unwrap().height, original.1);
    }

    #[test]
    fn set_artboard_size_clamps_to_a_positive_minimum() {
        let (mut document, artboard_id, _node_id, _animation_id) = document_with_one_node_and_animation();
        document.apply(Command::SetArtboardSize { artboard: artboard_id, width: -10.0, height: 0.0 });
        assert_eq!(document.artboard(artboard_id).unwrap().width, 1.0);
        assert_eq!(document.artboard(artboard_id).unwrap().height, 1.0);
    }

    #[test]
    fn set_artboard_resolution_scale_and_undo_restores_the_old_value() {
        let (mut document, artboard_id, _node_id, _animation_id) = document_with_one_node_and_animation();
        assert_eq!(document.artboard(artboard_id).unwrap().resolution_scale, 1.0);

        document.apply(Command::SetArtboardResolutionScale { artboard: artboard_id, value: 8.0 });
        assert_eq!(document.artboard(artboard_id).unwrap().resolution_scale, 8.0);

        assert!(document.undo());
        assert_eq!(document.artboard(artboard_id).unwrap().resolution_scale, 1.0);
    }

    #[test]
    fn set_artboard_resolution_scale_clamps_into_range() {
        let (mut document, artboard_id, _node_id, _animation_id) = document_with_one_node_and_animation();
        document.apply(Command::SetArtboardResolutionScale { artboard: artboard_id, value: -1.0 });
        assert_eq!(document.artboard(artboard_id).unwrap().resolution_scale, 1.0);
        document.apply(Command::SetArtboardResolutionScale { artboard: artboard_id, value: 500.0 });
        assert_eq!(document.artboard(artboard_id).unwrap().resolution_scale, 64.0);
    }

    #[test]
    fn add_keyframe_creates_the_track_on_first_use() {
        let (mut document, artboard_id, node_id, animation_id) = document_with_one_node_and_animation();

        document.apply(Command::AddKeyframe {
            artboard: artboard_id,
            animation: animation_id,
            node: node_id,
            time: 0.0,
            value: KeyframeValue::Translation(Vec2::new(5.0, 5.0)),
            easing: Easing::Linear,
        });

        let animation = document.artboard(artboard_id).unwrap().animation(animation_id).unwrap();
        assert_eq!(animation.tracks.len(), 1);
        let PropertyTrack::Translation(track) = &animation.tracks[0].1 else { panic!("expected a Translation track") };
        assert_eq!(track.keyframes[0].value, Vec2::new(5.0, 5.0));
    }

    #[test]
    fn different_properties_on_the_same_node_get_separate_tracks() {
        let (mut document, artboard_id, node_id, animation_id) = document_with_one_node_and_animation();

        document.apply(Command::AddKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, time: 0.0,
            value: KeyframeValue::Translation(Vec2::new(5.0, 5.0)), easing: Easing::Linear,
        });
        document.apply(Command::AddKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, time: 0.0,
            value: KeyframeValue::Rotation(1.5), easing: Easing::Linear,
        });
        document.apply(Command::AddKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, time: 0.0,
            value: KeyframeValue::FillColor([1.0, 0.0, 0.0, 1.0]), easing: Easing::Linear,
        });

        let animation = document.artboard(artboard_id).unwrap().animation(animation_id).unwrap();
        assert_eq!(animation.tracks.len(), 3, "one track per (node, property) pair, not one shared track");
    }

    #[test]
    fn add_keyframe_at_an_existing_time_replaces_it() {
        let (mut document, artboard_id, node_id, animation_id) = document_with_one_node_and_animation();
        document.apply(Command::AddKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, time: 1.0,
            value: KeyframeValue::Translation(Vec2::new(1.0, 1.0)), easing: Easing::Linear,
        });
        document.apply(Command::AddKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, time: 1.0,
            value: KeyframeValue::Translation(Vec2::new(9.0, 9.0)), easing: Easing::Linear,
        });

        let animation = document.artboard(artboard_id).unwrap().animation(animation_id).unwrap();
        assert_eq!(animation.tracks.len(), 1, "overwriting an existing time must not insert a duplicate track");
        let PropertyTrack::Translation(track) = &animation.tracks[0].1 else { panic!("expected a Translation track") };
        assert_eq!(track.keyframes.len(), 1);
        assert_eq!(track.keyframes[0].value, Vec2::new(9.0, 9.0));

        assert!(document.undo());
        let animation = document.artboard(artboard_id).unwrap().animation(animation_id).unwrap();
        let PropertyTrack::Translation(track) = &animation.tracks[0].1 else { panic!("expected a Translation track") };
        assert_eq!(track.keyframes[0].value, Vec2::new(1.0, 1.0), "undo must restore the overwritten value");
    }

    #[test]
    fn remove_keyframe_clears_the_track_when_it_was_the_last_one() {
        let (mut document, artboard_id, node_id, animation_id) = document_with_one_node_and_animation();
        document.apply(Command::AddKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, time: 0.0,
            value: KeyframeValue::Translation(Vec2::ZERO), easing: Easing::Linear,
        });

        document.apply(Command::RemoveKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, property: PropertyKind::Translation, time: 0.0,
        });
        assert!(document.artboard(artboard_id).unwrap().animation(animation_id).unwrap().tracks.is_empty());

        assert!(document.undo());
        assert_eq!(document.artboard(artboard_id).unwrap().animation(animation_id).unwrap().tracks.len(), 1);
    }

    #[test]
    fn set_motion_path_guide_creates_an_empty_track_and_undo_removes_it() {
        let (mut document, artboard_id, node_id, animation_id) = document_with_one_node_and_animation();
        let guide_id = document.artboard_mut(artboard_id).unwrap().add_node(Node::new(
            "Guide", None,
            NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
        ));

        document.apply(Command::SetMotionPathGuide { artboard: artboard_id, animation: animation_id, node: node_id, guide: guide_id });
        let animation = document.artboard(artboard_id).unwrap().animation(animation_id).unwrap();
        assert_eq!(animation.tracks.len(), 1);
        let PropertyTrack::MotionPath { guide, track } = &animation.tracks[0].1 else { panic!("expected a MotionPath track") };
        assert_eq!(*guide, guide_id);
        assert!(track.keyframes.is_empty());

        assert!(document.undo());
        assert!(document.artboard(artboard_id).unwrap().animation(animation_id).unwrap().tracks.is_empty());
    }

    #[test]
    fn set_motion_path_guide_rebinds_an_existing_track_and_undo_restores_the_old_guide() {
        let (mut document, artboard_id, node_id, animation_id) = document_with_one_node_and_animation();
        let artboard = document.artboard_mut(artboard_id).unwrap();
        let first_guide = artboard.add_node(Node::new(
            "Guide A", None,
            NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
        ));
        let second_guide = artboard.add_node(Node::new(
            "Guide B", None,
            NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
        ));

        document.apply(Command::SetMotionPathGuide { artboard: artboard_id, animation: animation_id, node: node_id, guide: first_guide });
        document.apply(Command::AddKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, time: 0.0,
            value: KeyframeValue::MotionPath(0.5), easing: Easing::Linear,
        });
        document.apply(Command::SetMotionPathGuide { artboard: artboard_id, animation: animation_id, node: node_id, guide: second_guide });

        let animation = document.artboard(artboard_id).unwrap().animation(animation_id).unwrap();
        assert_eq!(animation.tracks.len(), 1, "rebinding must not create a second track");
        let PropertyTrack::MotionPath { guide, track } = &animation.tracks[0].1 else { panic!("expected a MotionPath track") };
        assert_eq!(*guide, second_guide);
        assert_eq!(track.keyframes.len(), 1, "rebinding must keep existing keyframes");

        assert!(document.undo());
        let animation = document.artboard(artboard_id).unwrap().animation(animation_id).unwrap();
        let PropertyTrack::MotionPath { guide, track } = &animation.tracks[0].1 else { panic!("expected a MotionPath track") };
        assert_eq!(*guide, first_guide);
        assert_eq!(track.keyframes.len(), 1);
    }

    #[test]
    fn move_keyframe_retimes_it_and_undo_moves_it_back() {
        let (mut document, artboard_id, node_id, animation_id) = document_with_one_node_and_animation();
        document.apply(Command::AddKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, time: 0.0,
            value: KeyframeValue::Translation(Vec2::new(3.0, 4.0)), easing: Easing::Linear,
        });

        document.apply(Command::MoveKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, property: PropertyKind::Translation, old_time: 0.0, new_time: 2.0,
        });
        let animation = document.artboard(artboard_id).unwrap().animation(animation_id).unwrap();
        let PropertyTrack::Translation(track) = &animation.tracks[0].1 else { panic!("expected a Translation track") };
        assert_eq!(track.keyframes[0].time, 2.0);
        assert_eq!(track.keyframes[0].value, Vec2::new(3.0, 4.0));

        assert!(document.undo());
        let animation = document.artboard(artboard_id).unwrap().animation(animation_id).unwrap();
        let PropertyTrack::Translation(track) = &animation.tracks[0].1 else { panic!("expected a Translation track") };
        assert_eq!(track.keyframes[0].time, 0.0);
    }

    #[test]
    fn move_keyframe_keeps_the_track_sorted_by_time() {
        let (mut document, artboard_id, node_id, animation_id) = document_with_one_node_and_animation();
        document.apply(Command::AddKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, time: 0.0,
            value: KeyframeValue::Translation(Vec2::ZERO), easing: Easing::Linear,
        });
        document.apply(Command::AddKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, time: 1.0,
            value: KeyframeValue::Translation(Vec2::new(10.0, 0.0)), easing: Easing::Linear,
        });

        // Drags the first keyframe past the second one in time.
        document.apply(Command::MoveKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, property: PropertyKind::Translation, old_time: 0.0, new_time: 2.0,
        });

        let animation = document.artboard(artboard_id).unwrap().animation(animation_id).unwrap();
        let PropertyTrack::Translation(track) = &animation.tracks[0].1 else { panic!("expected a Translation track") };
        assert_eq!(track.keyframes.iter().map(|k| k.time).collect::<Vec<_>>(), vec![1.0, 2.0]);
    }

    #[test]
    fn shift_property_keyframes_adds_delta_to_every_keyframe_and_undo_reverses_it() {
        let (mut document, artboard_id, node_id, animation_id) = document_with_one_node_and_animation();
        document.apply(Command::AddKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, time: 0.0,
            value: KeyframeValue::Translation(Vec2::new(10.0, 0.0)), easing: Easing::Linear,
        });
        document.apply(Command::AddKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, time: 1.0,
            value: KeyframeValue::Translation(Vec2::new(20.0, 0.0)), easing: Easing::Linear,
        });

        document.apply(Command::ShiftPropertyKeyframes {
            artboard: artboard_id, node: node_id, property: PropertyKind::Translation,
            delta: KeyframeValue::Translation(Vec2::new(5.0, 5.0)),
        });

        let animation = document.artboard(artboard_id).unwrap().animation(animation_id).unwrap();
        let PropertyTrack::Translation(track) = &animation.tracks[0].1 else { panic!("expected a Translation track") };
        assert_eq!(track.keyframes[0].value, Vec2::new(15.0, 5.0));
        assert_eq!(track.keyframes[1].value, Vec2::new(25.0, 5.0));

        assert!(document.undo());
        let animation = document.artboard(artboard_id).unwrap().animation(animation_id).unwrap();
        let PropertyTrack::Translation(track) = &animation.tracks[0].1 else { panic!("expected a Translation track") };
        assert_eq!(track.keyframes[0].value, Vec2::new(10.0, 0.0));
        assert_eq!(track.keyframes[1].value, Vec2::new(20.0, 0.0));
    }

    #[test]
    fn shift_property_keyframes_only_touches_the_matching_node_and_property() {
        let (mut document, artboard_id, node_id, animation_id) = document_with_one_node_and_animation();
        document.apply(Command::AddKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, time: 0.0,
            value: KeyframeValue::Rotation(1.0), easing: Easing::Linear,
        });
        document.apply(Command::AddKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, time: 0.0,
            value: KeyframeValue::Translation(Vec2::new(1.0, 1.0)), easing: Easing::Linear,
        });

        document.apply(Command::ShiftPropertyKeyframes {
            artboard: artboard_id, node: node_id, property: PropertyKind::Rotation,
            delta: KeyframeValue::Rotation(2.0),
        });

        let animation = document.artboard(artboard_id).unwrap().animation(animation_id).unwrap();
        let rotation = animation.tracks.iter().find_map(|(_, t)| match t { PropertyTrack::Rotation(t) => Some(t), _ => None }).unwrap();
        assert_eq!(rotation.keyframes[0].value, 3.0);
        let translation = animation.tracks.iter().find_map(|(_, t)| match t { PropertyTrack::Translation(t) => Some(t), _ => None }).unwrap();
        assert_eq!(translation.keyframes[0].value, Vec2::new(1.0, 1.0), "shifting Rotation must not touch the Translation track");
    }

    #[test]
    fn batch_applies_every_command_and_undoes_them_all_as_one_step() {
        let (mut document, artboard_id, node_id) = document_with_one_node();

        document.apply(Command::Batch(vec![
            Command::SetTranslation { artboard: artboard_id, node: node_id, value: Vec2::new(10.0, 0.0) },
            Command::SetRotation { artboard: artboard_id, node: node_id, value: 1.0 },
        ]));

        let transform = document.artboard(artboard_id).unwrap().node(node_id).unwrap().transform;
        assert_eq!(transform.translation, Vec2::new(10.0, 0.0));
        assert_eq!(transform.rotation, 1.0);

        assert!(document.undo());
        let transform = document.artboard(artboard_id).unwrap().node(node_id).unwrap().transform;
        assert_eq!(transform.translation, Vec2::ZERO);
        assert_eq!(transform.rotation, 0.0);

        assert!(document.redo());
        let transform = document.artboard(artboard_id).unwrap().node(node_id).unwrap().transform;
        assert_eq!(transform.translation, Vec2::new(10.0, 0.0));
        assert_eq!(transform.rotation, 1.0);
    }

    fn sample_state_machine() -> StateMachineDef {
        StateMachineDef {
            name: "Main".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Idle".into(),
                animation: AnimationId::default(),
                loop_mode: crate::state_machine::LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: None,
            }],
            initial_state: "Idle".into(),
            layers: Vec::new(),
            deleted: false,
        }
    }

    #[test]
    fn add_state_machine_inserts_it_and_its_inverse_soft_deletes_it() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));

        let inverse = Command::AddStateMachine { artboard: artboard_id, state_machine: sample_state_machine() }.apply(&mut document);
        let (sm_id, sm) = document.artboard(artboard_id).unwrap().state_machines().next().unwrap();
        assert_eq!(sm.name, "Main");

        let Command::SetStateMachineDeleted { state_machine: inv_id, value, .. } = inverse else {
            panic!("AddStateMachine's inverse must be a SetStateMachineDeleted command");
        };
        assert_eq!(inv_id, sm_id);
        assert!(value);

        inverse.apply(&mut document);
        assert_eq!(document.artboard(artboard_id).unwrap().state_machines().count(), 0, "deleted state machines are skipped by the listing iterator");
    }

    #[test]
    fn set_state_machine_initial_state_and_undo_restores_the_old_value() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let inverse = Command::AddStateMachine { artboard: artboard_id, state_machine: sample_state_machine() }.apply(&mut document);
        let Command::SetStateMachineDeleted { state_machine: sm_id, .. } = inverse else { unreachable!() };

        document.apply(Command::SetStateMachineInitialState { artboard: artboard_id, state_machine: sm_id, value: "Active".into() });
        assert_eq!(document.artboard(artboard_id).unwrap().state_machine(sm_id).unwrap().initial_state, "Active");

        assert!(document.undo());
        assert_eq!(document.artboard(artboard_id).unwrap().state_machine(sm_id).unwrap().initial_state, "Idle");
    }

    #[test]
    fn set_state_machine_states_replaces_the_whole_list_and_undo_restores_it() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let inverse = Command::AddStateMachine { artboard: artboard_id, state_machine: sample_state_machine() }.apply(&mut document);
        let Command::SetStateMachineDeleted { state_machine: sm_id, .. } = inverse else { unreachable!() };

        let mut new_states = document.artboard(artboard_id).unwrap().state_machine(sm_id).unwrap().states.clone();
        new_states.push(StateDef {
            name: "Active".into(),
            animation: AnimationId::default(),
            loop_mode: crate::state_machine::LoopMode::Once,
            transitions: Vec::new(),
            position: Vec2::ZERO,
            blend: None,
            program: None,
        });
        document.apply(Command::SetStateMachineStates { artboard: artboard_id, state_machine: sm_id, value: new_states });
        assert_eq!(document.artboard(artboard_id).unwrap().state_machine(sm_id).unwrap().states.len(), 2);

        assert!(document.undo());
        assert_eq!(document.artboard(artboard_id).unwrap().state_machine(sm_id).unwrap().states.len(), 1);
    }

    #[test]
    fn set_state_transitions_targets_the_named_state_and_undo_restores_it() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let inverse = Command::AddStateMachine { artboard: artboard_id, state_machine: sample_state_machine() }.apply(&mut document);
        let Command::SetStateMachineDeleted { state_machine: sm_id, .. } = inverse else { unreachable!() };

        let new_transitions = vec![TransitionDef {
            to: "Active".into(),
            condition: crate::state_machine::ConditionDef::Trigger { input: "toggle".into() },
            direction: crate::state_machine::PlaybackDirection::Forward,
            duration_seconds: 0.0,
            interruptible: true,
        }];
        document.apply(Command::SetStateTransitions {
            artboard: artboard_id,
            state_machine: sm_id,
            state_name: "Idle".into(),
            value: new_transitions,
        });
        assert_eq!(document.artboard(artboard_id).unwrap().state_machine(sm_id).unwrap().states[0].transitions.len(), 1);

        assert!(document.undo());
        assert_eq!(document.artboard(artboard_id).unwrap().state_machine(sm_id).unwrap().states[0].transitions.len(), 0);
    }

    #[test]
    fn set_state_position_moves_it_and_undo_restores_the_old_position() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let inverse = Command::AddStateMachine { artboard: artboard_id, state_machine: sample_state_machine() }.apply(&mut document);
        let Command::SetStateMachineDeleted { state_machine: sm_id, .. } = inverse else { unreachable!() };

        let undo = Command::SetStatePosition { artboard: artboard_id, state_machine: sm_id, state_name: "Idle".into(), value: Vec2::new(50.0, 60.0) }
            .apply(&mut document);
        assert_eq!(
            document.artboard(artboard_id).unwrap().state_machine(sm_id).unwrap().states[0].position,
            Vec2::new(50.0, 60.0)
        );

        undo.apply(&mut document);
        assert_eq!(document.artboard(artboard_id).unwrap().state_machine(sm_id).unwrap().states[0].position, Vec2::ZERO);
    }

    #[test]
    fn set_state_machine_name_renames_it_and_undo_restores_the_old_name() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let inverse = Command::AddStateMachine { artboard: artboard_id, state_machine: sample_state_machine() }.apply(&mut document);
        let Command::SetStateMachineDeleted { state_machine: sm_id, .. } = inverse else { unreachable!() };

        document.apply(Command::SetStateMachineName { artboard: artboard_id, state_machine: sm_id, value: "Renamed".into() });
        assert_eq!(document.artboard(artboard_id).unwrap().state_machine(sm_id).unwrap().name, "Renamed");

        assert!(document.undo());
        assert_eq!(document.artboard(artboard_id).unwrap().state_machine(sm_id).unwrap().name, "Main");
    }

    #[test]
    fn set_shape_blend_mode_changes_it_and_undo_restores_the_old_mode() {
        let (mut document, artboard_id, node_id) = document_with_one_node();

        let inverse = Command::SetShapeBlendMode { artboard: artboard_id, node: node_id, value: crate::blend_mode::BlendMode::Multiply }
            .apply(&mut document);
        let NodeKind::Shape(shape) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected a Shape") };
        assert_eq!(shape.blend_mode, crate::blend_mode::BlendMode::Multiply);

        inverse.apply(&mut document);
        let NodeKind::Shape(shape) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected a Shape") };
        assert_eq!(shape.blend_mode, crate::blend_mode::BlendMode::Normal);
    }

    #[test]
    fn set_fill_color_changes_a_shapes_color_and_undo_restores_the_old_one() {
        let (mut document, artboard_id, node_id) = document_with_one_node();

        let inverse = Command::SetFillColor { artboard: artboard_id, node: node_id, value: [0.0, 1.0, 0.0, 1.0] }.apply(&mut document);
        let NodeKind::Shape(shape) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected a Shape") };
        assert_eq!(shape.fill_color, [0.0, 1.0, 0.0, 1.0]);

        inverse.apply(&mut document);
        let NodeKind::Shape(shape) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected a Shape") };
        assert_eq!(shape.fill_color, [1.0; 4]);
    }

    #[test]
    fn set_gradient_sets_it_on_a_shape_and_undo_restores_the_old_value() {
        let (mut document, artboard_id, node_id) = document_with_one_node();

        let gradient = GradientDef {
            kind: GradientKind::Linear,
            start: Vec2::new(0.0, 0.0),
            end: Vec2::new(10.0, 0.0),
            stops: vec![
                GradientStop { offset: 0.0, color: [1.0, 0.0, 0.0, 1.0], midpoint: 0.5 },
                GradientStop { offset: 1.0, color: [0.0, 0.0, 1.0, 1.0], midpoint: 0.5 },
            ],
        };
        let inverse = Command::SetGradient { artboard: artboard_id, node: node_id, value: Some(gradient.clone()) }.apply(&mut document);
        let NodeKind::Shape(shape) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected a Shape") };
        assert_eq!(shape.gradient, Some(gradient));
        // Setting a gradient must not touch the flat fallback color.
        assert_eq!(shape.fill_color, [1.0; 4]);

        inverse.apply(&mut document);
        let NodeKind::Shape(shape) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected a Shape") };
        assert_eq!(shape.gradient, None);
    }

    #[test]
    fn set_shape_path_replaces_it_and_undo_restores_the_old_one() {
        let (mut document, artboard_id, node_id) = document_with_one_node();
        let original = {
            let NodeKind::Shape(shape) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected a Shape") };
            shape.path.clone()
        };

        let edited = Path::rect(20.0, 30.0);
        let inverse = Command::SetShapePath { artboard: artboard_id, node: node_id, value: edited.clone() }.apply(&mut document);
        let NodeKind::Shape(shape) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected a Shape") };
        assert_eq!(shape.path, edited);

        inverse.apply(&mut document);
        let NodeKind::Shape(shape) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected a Shape") };
        assert_eq!(shape.path, original);
    }

    #[test]
    fn set_shape_stroke_sets_and_clears_it_with_undo_restoring_the_old_value() {
        let (mut document, artboard_id, node_id) = document_with_one_node();

        let stroke = crate::node::Stroke { width: 2.0, color: [0.0, 0.0, 0.0, 1.0], gradient: None, image: None, cap: Default::default(), join: Default::default() };
        let inverse = Command::SetShapeStroke { artboard: artboard_id, node: node_id, value: Some(stroke.clone()) }.apply(&mut document);
        let NodeKind::Shape(shape) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected a Shape") };
        assert_eq!(shape.stroke, Some(stroke));

        inverse.apply(&mut document);
        let NodeKind::Shape(shape) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected a Shape") };
        assert_eq!(shape.stroke, None);
    }

    fn document_with_one_text_node() -> (Document, ArtboardId, NodeId) {
        let mut document = Document::new();
        let font_id = document.add_font(FontAsset { name: "Demo".into(), data: Vec::new() });
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        document.apply(Command::AddNode {
            artboard: artboard_id,
            node: Node::new("Label", None, NodeKind::Text(TextNode { content: "Hi".into(), font: font_id, size_px: 16.0, fill_color: [1.0; 4], gradient: None })),
        });
        let node_id = document.artboard(artboard_id).unwrap().root_nodes()[0];
        (document, artboard_id, node_id)
    }

    #[test]
    fn set_fill_color_changes_a_texts_color_and_undo_restores_the_old_one() {
        let (mut document, artboard_id, node_id) = document_with_one_text_node();

        let inverse = Command::SetFillColor { artboard: artboard_id, node: node_id, value: [0.0, 0.0, 1.0, 1.0] }.apply(&mut document);
        let NodeKind::Text(text) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected a Text") };
        assert_eq!(text.fill_color, [0.0, 0.0, 1.0, 1.0]);

        inverse.apply(&mut document);
        let NodeKind::Text(text) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected a Text") };
        assert_eq!(text.fill_color, [1.0; 4]);
    }

    fn document_with_one_image_node() -> (Document, ArtboardId, NodeId) {
        use crate::image_asset::ImageAsset;
        use crate::node::ImageNode;

        let mut document = Document::new();
        let image_id = document.add_image(ImageAsset { name: "Sprite".into(), source: Vec::new(), processing: Default::default(), file: None });
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        document.apply(Command::AddNode {
            artboard: artboard_id,
            node: Node::new("Sprite", None, NodeKind::Image(ImageNode { image: image_id, width: 64.0, height: 64.0, fill_color: [1.0; 4], blend_mode: Default::default() })),
        });
        let node_id = document.artboard(artboard_id).unwrap().root_nodes()[0];
        (document, artboard_id, node_id)
    }

    #[test]
    fn set_fill_color_changes_an_images_tint_and_undo_restores_the_old_one() {
        let (mut document, artboard_id, node_id) = document_with_one_image_node();

        let inverse = Command::SetFillColor { artboard: artboard_id, node: node_id, value: [1.0, 0.0, 0.0, 1.0] }.apply(&mut document);
        let NodeKind::Image(image) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected an Image") };
        assert_eq!(image.fill_color, [1.0, 0.0, 0.0, 1.0]);

        inverse.apply(&mut document);
        let NodeKind::Image(image) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected an Image") };
        assert_eq!(image.fill_color, [1.0; 4]);
    }

    #[test]
    fn set_text_content_changes_it_and_undo_restores_the_old_string() {
        let (mut document, artboard_id, node_id) = document_with_one_text_node();

        let inverse = Command::SetTextContent { artboard: artboard_id, node: node_id, value: "Bye".into() }.apply(&mut document);
        let NodeKind::Text(text) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected a Text") };
        assert_eq!(text.content, "Bye");

        inverse.apply(&mut document);
        let NodeKind::Text(text) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected a Text") };
        assert_eq!(text.content, "Hi");
    }

    #[test]
    fn set_text_size_changes_it_and_undo_restores_the_old_size() {
        let (mut document, artboard_id, node_id) = document_with_one_text_node();

        let inverse = Command::SetTextSize { artboard: artboard_id, node: node_id, value: 48.0 }.apply(&mut document);
        let NodeKind::Text(text) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected a Text") };
        assert_eq!(text.size_px, 48.0);

        inverse.apply(&mut document);
        let NodeKind::Text(text) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected a Text") };
        assert_eq!(text.size_px, 16.0);
    }

    #[test]
    fn set_bone_length_resizes_it_and_undo_restores_the_old_length() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        document.apply(Command::AddNode {
            artboard: artboard_id,
            node: Node::new("Root bone", None, NodeKind::Bone(crate::node::BoneNode { length: 50.0 })),
        });
        let node_id = document.artboard(artboard_id).unwrap().root_nodes()[0];

        let inverse = Command::SetBoneLength { artboard: artboard_id, node: node_id, value: 80.0 }.apply(&mut document);
        let NodeKind::Bone(bone) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else {
            panic!("expected a Bone")
        };
        assert_eq!(bone.length, 80.0);

        inverse.apply(&mut document);
        let NodeKind::Bone(bone) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else {
            panic!("expected a Bone")
        };
        assert_eq!(bone.length, 50.0);
    }

    fn sample_component_definition() -> ComponentDefinition {
        ComponentDefinition {
            name: "Dot".into(),
            path: crate::path::Path::rect(20.0, 20.0),
            default_fill_color: [1.0, 0.0, 0.0, 1.0],
            variants: Vec::new(),
            deleted: false,
        }
    }

    #[test]
    fn add_component_inserts_it_and_its_inverse_soft_deletes_it() {
        let mut document = Document::new();
        let inverse = Command::AddComponent { definition: sample_component_definition() }.apply(&mut document);

        let Command::SetComponentDeleted { component: component_id, value } = inverse else {
            panic!("AddComponent's inverse must be a SetComponentDeleted command");
        };
        assert!(value);
        assert_eq!(document.components().count(), 1);

        inverse.apply(&mut document);
        assert_eq!(document.components().count(), 0, "deleted components are skipped by the listing iterator");
        assert!(document.component(component_id).is_some(), "soft-delete keeps the ComponentId resolvable");
    }

    #[test]
    fn set_component_default_fill_color_and_undo_restores_the_old_color() {
        let mut document = Document::new();
        let inverse = Command::AddComponent { definition: sample_component_definition() }.apply(&mut document);
        let Command::SetComponentDeleted { component: component_id, .. } = inverse else { unreachable!() };

        document.apply(Command::SetComponentDefaultFillColor { component: component_id, value: [0.0, 1.0, 0.0, 1.0] });
        assert_eq!(document.component(component_id).unwrap().default_fill_color, [0.0, 1.0, 0.0, 1.0]);

        assert!(document.undo());
        assert_eq!(document.component(component_id).unwrap().default_fill_color, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn set_component_variants_replaces_the_whole_list_and_undo_restores_it() {
        let mut document = Document::new();
        let inverse = Command::AddComponent { definition: sample_component_definition() }.apply(&mut document);
        let Command::SetComponentDeleted { component: component_id, .. } = inverse else { unreachable!() };

        document.apply(Command::SetComponentVariants {
            component: component_id,
            value: vec![ComponentVariant { name: "Gold".into(), fill_color: [1.0, 0.8, 0.0, 1.0] }],
        });
        assert_eq!(document.component(component_id).unwrap().variants.len(), 1);

        assert!(document.undo());
        assert_eq!(document.component(component_id).unwrap().variants.len(), 0);
    }

    #[test]
    fn set_instance_fill_color_override_and_undo_restores_the_old_value() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let inverse = Command::AddComponent { definition: sample_component_definition() }.apply(&mut document);
        let Command::SetComponentDeleted { component: component_id, .. } = inverse else { unreachable!() };

        document.apply(Command::AddNode {
            artboard: artboard_id,
            node: Node::new("Dot instance", None, NodeKind::Instance(crate::component::ComponentInstance::new(component_id))),
        });
        let node_id = document.artboard(artboard_id).unwrap().root_nodes()[0];

        document.apply(Command::SetInstanceFillColorOverride { artboard: artboard_id, node: node_id, value: Some([0.0, 0.0, 1.0, 1.0]) });
        let NodeKind::Instance(instance) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected an Instance") };
        assert_eq!(instance.fill_color_override, Some([0.0, 0.0, 1.0, 1.0]));

        assert!(document.undo());
        let NodeKind::Instance(instance) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected an Instance") };
        assert_eq!(instance.fill_color_override, None);
    }

    #[test]
    fn set_instance_variant_and_undo_restores_the_old_value() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let inverse = Command::AddComponent { definition: sample_component_definition() }.apply(&mut document);
        let Command::SetComponentDeleted { component: component_id, .. } = inverse else { unreachable!() };

        document.apply(Command::AddNode {
            artboard: artboard_id,
            node: Node::new("Dot instance", None, NodeKind::Instance(crate::component::ComponentInstance::new(component_id))),
        });
        let node_id = document.artboard(artboard_id).unwrap().root_nodes()[0];

        document.apply(Command::SetInstanceVariant { artboard: artboard_id, node: node_id, value: Some("Gold".into()) });
        let NodeKind::Instance(instance) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected an Instance") };
        assert_eq!(instance.variant.as_deref(), Some("Gold"));

        assert!(document.undo());
        let NodeKind::Instance(instance) = &document.artboard(artboard_id).unwrap().node(node_id).unwrap().kind else { panic!("expected an Instance") };
        assert_eq!(instance.variant, None);
    }

    #[test]
    fn reparent_node_moves_it_and_undo_restores_the_old_parent() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let group_inverse = Command::AddNode {
            artboard: artboard_id,
            node: Node::new("Group", None, NodeKind::Group(Default::default())),
        }.apply(&mut document);
        let Command::SetDeleted { node: group_id, .. } = group_inverse else { unreachable!() };

        let child_inverse = Command::AddNode {
            artboard: artboard_id,
            node: Node::new("Square", None, NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None})),
        }.apply(&mut document);
        let Command::SetDeleted { node: child_id, .. } = child_inverse else { unreachable!() };

        assert_eq!(document.artboard(artboard_id).unwrap().root_nodes().len(), 2);

        let inverse = Command::ReparentNode { artboard: artboard_id, node: child_id, value: Some(group_id) }.apply(&mut document);
        assert_eq!(document.artboard(artboard_id).unwrap().node(child_id).unwrap().parent, Some(group_id));
        assert_eq!(document.artboard(artboard_id).unwrap().root_nodes(), &[group_id], "reparented child must leave root_nodes");

        inverse.apply(&mut document);
        assert_eq!(document.artboard(artboard_id).unwrap().node(child_id).unwrap().parent, None);
        assert_eq!(document.artboard(artboard_id).unwrap().root_nodes().len(), 2, "undo must restore it to root_nodes");
    }

    fn add_square(document: &mut Document, artboard_id: ArtboardId, name: &str) -> NodeId {
        let inverse = Command::AddNode {
            artboard: artboard_id,
            node: Node::new(name, None, NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None})),
        }
        .apply(document);
        let Command::SetDeleted { node, .. } = inverse else { unreachable!() };
        node
    }

    #[test]
    fn move_node_forward_and_undo_restores_the_old_order() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let a = add_square(&mut document, artboard_id, "A");
        let b = add_square(&mut document, artboard_id, "B");

        document.apply(Command::MoveNodeForward { artboard: artboard_id, node: a });
        assert_eq!(document.artboard(artboard_id).unwrap().shapes().map(|(id, _)| id).collect::<Vec<_>>(), vec![b, a]);

        assert!(document.undo());
        assert_eq!(document.artboard(artboard_id).unwrap().shapes().map(|(id, _)| id).collect::<Vec<_>>(), vec![a, b]);
    }

    #[test]
    fn bring_node_to_front_and_undo_restores_the_exact_prior_index() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let a = add_square(&mut document, artboard_id, "A");
        let b = add_square(&mut document, artboard_id, "B");
        let c = add_square(&mut document, artboard_id, "C");

        document.apply(Command::BringNodeToFront { artboard: artboard_id, node: a });
        assert_eq!(document.artboard(artboard_id).unwrap().shapes().map(|(id, _)| id).collect::<Vec<_>>(), vec![b, c, a]);

        assert!(document.undo());
        assert_eq!(
            document.artboard(artboard_id).unwrap().shapes().map(|(id, _)| id).collect::<Vec<_>>(),
            vec![a, b, c],
            "undo must restore a's exact original index, not just move it back one step"
        );
    }

    #[test]
    fn set_group_mask_and_undo_restores_the_old_value() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let group_inverse = Command::AddNode {
            artboard: artboard_id,
            node: Node::new("Group", None, NodeKind::Group(Default::default())),
        }.apply(&mut document);
        let Command::SetDeleted { node: group_id, .. } = group_inverse else { unreachable!() };
        let child_inverse = Command::AddNode {
            artboard: artboard_id,
            node: Node::new("Mask shape", Some(group_id), NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None})),
        }.apply(&mut document);
        let Command::SetDeleted { node: child_id, .. } = child_inverse else { unreachable!() };

        let inverse = Command::SetGroupMask { artboard: artboard_id, node: group_id, value: Some(child_id) }.apply(&mut document);
        let NodeKind::Group(group) = &document.artboard(artboard_id).unwrap().node(group_id).unwrap().kind else { panic!("expected a Group") };
        assert_eq!(group.mask, Some(child_id));

        inverse.apply(&mut document);
        let NodeKind::Group(group) = &document.artboard(artboard_id).unwrap().node(group_id).unwrap().kind else { panic!("expected a Group") };
        assert_eq!(group.mask, None);
    }

    #[test]
    fn set_post_effects_replaces_the_whole_stack_and_undo_restores_it() {
        use crate::post_effect::{PostEffect, PostEffectLayer};

        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        assert!(document.artboard(artboard_id).unwrap().post_effects().is_empty());

        let stack = vec![
            PostEffectLayer { effect: PostEffect::Blur { radius: 4.0 }, enabled: true },
            PostEffectLayer { effect: PostEffect::Glow { radius: 6.0, threshold: 0.7, intensity: 1.0 }, enabled: false },
        ];
        let inverse = Command::SetPostEffects { artboard: artboard_id, value: stack.clone() }.apply(&mut document);
        assert_eq!(document.artboard(artboard_id).unwrap().post_effects(), stack.as_slice());

        inverse.apply(&mut document);
        assert!(document.artboard(artboard_id).unwrap().post_effects().is_empty());
    }

    #[test]
    fn commit_batch_folds_manually_applied_commands_into_one_undo_step() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));

        let mut inverses = Vec::new();
        let parent_inverse = Command::AddNode {
            artboard: artboard_id,
            node: Node::new("Group", None, NodeKind::Group(Default::default())),
        }.apply(&mut document);
        let Command::SetDeleted { node: parent_id, .. } = &parent_inverse else { unreachable!() };
        let parent_id = *parent_id;
        inverses.push(parent_inverse);

        let child_inverse = Command::AddNode {
            artboard: artboard_id,
            node: Node::new("Square", Some(parent_id), NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None})),
        }.apply(&mut document);
        inverses.push(child_inverse);

        inverses.reverse();
        document.commit_batch(inverses);

        assert_eq!(document.artboard(artboard_id).unwrap().root_nodes().len(), 1, "only the Group is a root; the Square is parented under it");

        assert!(document.undo(), "the whole two-node insert must undo as one step");
        assert!(document.artboard(artboard_id).unwrap().node(parent_id).unwrap().deleted, "undo soft-deletes the group (root_nodes keeps its id per AddNode's soft-delete convention)");
        assert!(!document.undo(), "there must be nothing left to undo after the one combined step");
    }

    fn document_with_one_camera() -> (Document, ArtboardId, NodeId) {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let artboard = document.artboard_mut(artboard_id).unwrap();
        let node_id = artboard.add_node(Node::with_components("Camera", None, vec![crate::NodeBehavior::Camera(crate::CameraNode::default())]));
        (document, artboard_id, node_id)
    }

    #[test]
    fn set_camera_applies_and_inverts() {
        let (mut document, artboard_id, node_id) = document_with_one_camera();
        let edited = crate::CameraNode {
            zoom: 2.5,
            background_color: [1.0, 0.0, 0.0, 1.0],
            follow_target: Some(node_id),
            follow_smoothing: 0.0,
            vignette: crate::CameraVignette { enabled: true, ..Default::default() },
            pixel_perfect: crate::PixelPerfect { enabled: true, ..Default::default() },
        };

        let inverse = Command::SetCamera { artboard: artboard_id, node: node_id, value: edited }.apply(&mut document);
        let camera = document.artboard(artboard_id).unwrap().node(node_id).unwrap().camera().copied().unwrap();
        assert_eq!(camera, edited);

        inverse.apply(&mut document);
        let camera = document.artboard(artboard_id).unwrap().node(node_id).unwrap().camera().copied().unwrap();
        assert_eq!(camera, crate::CameraNode::default());
    }

    #[test]
    fn add_artboard_inserts_and_undo_soft_deletes_it_out_of_the_listing() {
        let mut document = Document::new();
        document.add_artboard(Artboard::new("Main", 400.0, 400.0));

        let inverse = Command::AddArtboard { name: "Level 2".into(), width: 800.0, height: 600.0 }.apply(&mut document);
        let Command::SetArtboardDeleted { artboard: new_id, value: true } = inverse else { panic!("AddArtboard's inverse must be SetArtboardDeleted(true)") };
        assert_eq!(document.artboards().count(), 2);
        assert_eq!(document.artboard(new_id).unwrap().name, "Level 2");

        inverse.apply(&mut document);
        assert_eq!(document.artboards().count(), 1, "a deleted artboard drops out of the listing");
        assert!(document.artboard(new_id).is_some(), "but still resolves by id, for redo");
    }

    #[test]
    fn set_artboard_name_applies_and_inverts() {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));

        let inverse = Command::SetArtboardName { artboard: artboard_id, value: "Forest".into() }.apply(&mut document);
        assert_eq!(document.artboard(artboard_id).unwrap().name, "Forest");

        inverse.apply(&mut document);
        assert_eq!(document.artboard(artboard_id).unwrap().name, "Main");
    }

    #[test]
    fn set_active_camera_applies_and_inverts() {
        let (mut document, artboard_id, node_id) = document_with_one_camera();
        assert_eq!(document.artboard(artboard_id).unwrap().active_camera(), None);

        let inverse = Command::SetActiveCamera { artboard: artboard_id, value: Some(node_id) }.apply(&mut document);
        assert_eq!(document.artboard(artboard_id).unwrap().active_camera(), Some(node_id));

        inverse.apply(&mut document);
        assert_eq!(document.artboard(artboard_id).unwrap().active_camera(), None);
    }
}

#[cfg(test)]
mod component_tests {
    use crate::{Node, NodeBehavior, NodeKind};

    #[test]
    fn legacy_kinds_migrate_into_components_on_an_empty_group() {
        let mut node = Node::new("Light", None, NodeKind::Light2D(crate::Light2DNode::default()));
        node.shadow_caster = Some(crate::ShadowCaster2D::default());
        assert!(node.light2d().is_some(), "accessor reads the legacy kind");
        assert!(node.migrate_legacy_kind_to_behaviors());
        assert!(matches!(node.kind, NodeKind::Group(_)));
        assert!(matches!(node.behaviors[0].behavior, NodeBehavior::Light2D(_)));
        assert!(node.light2d().is_some() && node.shadow_caster().is_some());
        assert!(!node.migrate_legacy_kind_to_behaviors(), "idempotent");
    }

    #[test]
    fn disabled_components_are_ignored_and_light_color_is_the_fill() {
        let mut node = Node::with_components("Lamp", None, vec![NodeBehavior::Light2D(crate::Light2DNode::default())]);
        *node.fill_color_mut().unwrap() = [1.0, 0.0, 0.0, 1.0];
        assert_eq!(node.light2d().unwrap().color, [1.0, 0.0, 0.0, 1.0]);
        node.behaviors[0].enabled = false;
        assert!(node.light2d().is_none());
    }
}
