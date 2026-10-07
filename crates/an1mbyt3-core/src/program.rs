//! Programmable States: an alternative to a plain played-back animation for
//! a [`crate::StateDef`] — instead of sampling a clip, the state runs a
//! *program* every frame, driven by live sources (time, mouse, random) and
//! math nodes, writing straight into the artboard's own node properties.
//! This is the authored half only (`an1mbyt3-runtime`'s `graph.rs` is the
//! live evaluator); same "authored data vs. live runtime state" split
//! `state_machine.rs`'s own module doc already draws.
//!
//! Three authoring modes, an author's choice per state, not a capability
//! ladder — `Graph` and `Blocks` are real Rust and run everywhere (desktop
//! editor, wasm player, any future embed); `Code` only ever runs inside the
//! desktop editor's own frontend loop (`apps/editor/src/scriptRuntime.ts`
//! compiles and calls it — nothing in this crate or `an1mbyt3-runtime`
//! parses or executes the string), the same "holds an input, not a derived
//! artifact" discipline `ScriptBehavior` used before it (now removed —
//! Programmable States supersede that never-wired-up concept). `Blocks`
//! (`crate::block_program`) is the Scratch-style statement/event mode —
//! control flow, "when X happens" triggers, touching/key-pressed sensing —
//! that `Graph`'s pure continuous-dataflow model can't represent; see that
//! module's own doc comment for the full comparison.
//!
//! v1 scope, deliberately: continuous property values only, numeric sockets
//! only (a `Vec2` property like translation is two independently-wireable
//! scalar sockets, one per component — `GraphProperty::TranslationX`/`Y` —
//! matching "every value a node can have gets a node output channel"
//! literally, per-component rather than per-property). Booleans are
//! numbers too (`0.0` false, anything else true, always normalized to
//! exactly `0.0`/`1.0` on the way *out* of a logic node) — no separate
//! boolean socket type, so a comparison/logic node's output can feed
//! straight into `Lerp`/`Select`/`SetProperty` without a wrapper. This
//! graph still can never itself fire a transition/cause a state change —
//! the existing `ConditionDef` system still owns transitions entirely;
//! `GetNumberInput`/`GetBoolInput` only *read* a state machine's declared
//! `Inputs`, the same live values `ConditionDef` checks, as one more
//! continuous source a graph can react to.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{block_program::BlockProgramDef, ids::NodeId};

/// A state's program — an author's choice of raw code, visual dataflow
/// graph, or Scratch-style blocks. See this module's doc comment for the
/// runtime-support difference between the three; that difference is a
/// deliberate, stated tradeoff, not a bug.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProgramDef {
    Code(String),
    Graph(NodeGraphDef),
    Blocks(BlockProgramDef),
}

/// A whole authored dataflow graph: its nodes and the wires between their
/// sockets.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeGraphDef {
    pub nodes: Vec<GraphNodeDef>,
    pub edges: Vec<GraphEdgeDef>,
}

/// One node in the graph: its behavior (`kind`), its canvas layout
/// position (same "authored, editor-only layout" convention as
/// `StateDef::position` — the evaluator never reads this), and any
/// input-socket overrides authored on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNodeDef {
    pub id: u32,
    pub kind: GraphNodeKind,
    #[serde(default)]
    pub position: Vec2,
    #[serde(default)]
    pub overrides: Vec<SocketOverride>,
}

/// An authored value for one of a node's input sockets, used whenever that
/// socket either isn't wired at all, or is wired but the author has pinned
/// it to a manual value anyway (`use_override: true` beats a live wire) —
/// the literal "active value or value override" behavior the user asked
/// for. `socket_index` addresses a specific input socket on the owning
/// `GraphNodeDef` by position (e.g. `Lerp`'s three inputs are sockets
/// `0..3`) since sockets aren't independently named/identified elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SocketOverride {
    pub socket_index: u32,
    pub value: f32,
    pub use_override: bool,
}

/// What a graph node computes. Source nodes take no inputs; math nodes take
/// the obvious fixed arity (documented per-variant only where not obvious);
/// `GetProperty`/`SetProperty` are the graph's only connection to the rest
/// of the artboard — everything else is pure math over floats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GraphNodeKind {
    /// A fixed authored number — no inputs, one output.
    Constant(f32),
    /// Seconds since this state became active (`Layer`'s own `Playback`,
    /// already ticking for free — see this module's sibling `graph.rs` in
    /// `an1mbyt3-runtime`). No inputs, one output.
    Time,
    /// Live pointer position in artboard space, X and Y as two separate
    /// outputs (socket `0` = X, socket `1` = Y) — same per-component
    /// convention as `GraphProperty`'s split axes. No inputs.
    MousePosition,
    /// A stable pseudo-random value in `0.0..1.0`, reseeded only when
    /// `seed` itself changes (not every frame) — deterministic per authored
    /// seed, matching "an author can dial in a specific-looking randomness"
    /// rather than flicker every evaluation. No inputs, one output.
    Random { seed: u32 },
    /// Two inputs (sockets `0`, `1`), one output.
    Add,
    /// Two inputs (sockets `0`, `1`), one output: `0 - 1`.
    Subtract,
    /// Two inputs (sockets `0`, `1`), one output.
    Multiply,
    /// Two inputs (sockets `0`, `1`), one output: `0 / 1` (`1 == 0.0`
    /// yields `0.0` rather than propagating `inf`/`NaN` into the rest of
    /// the graph).
    Divide,
    /// One input (socket `0`), one output.
    Sin,
    /// One input (socket `0`), one output.
    Cos,
    /// Three inputs — `a` (socket `0`), `b` (socket `1`), `t` (socket `2`)
    /// — one output: `a + (b - a) * t.clamp(0, 1)`.
    Lerp,
    /// Three inputs — `value` (socket `0`), `min` (socket `1`), `max`
    /// (socket `2`) — one output.
    Clamp,
    /// Reads a live property component off another node in the same
    /// artboard — one output, no inputs (the *target* node is authored
    /// directly on this variant, not wired, since it's a structural pick
    /// from the scene, not a computed number).
    GetProperty { node: NodeId, property: GraphProperty },
    /// Writes a live property component onto another node in the same
    /// artboard — one input (socket `0`, the value to write), no outputs.
    ///
    /// `global`: for `TranslationX`/`TranslationY` only, treats the
    /// incoming value as a *world*-space coordinate instead of a value for
    /// `target`'s own local `Transform2D::translation` — this is the
    /// abstraction for "make this deeply-nested child/grandchild follow the
    /// mouse (or any other world-space source)" without the author having
    /// to hand-derive the inverse of every ancestor's translation/rotation/
    /// scale themselves. `an1mbyt3-runtime::graph::evaluate_graph` converts
    /// world -> local via `Transform2D::local_for` against the target's
    /// *parent's* current world transform (identity for a root node) before
    /// writing, preserving the node's current world value on whichever axis
    /// this particular `SetProperty` isn't driving — so wiring both a
    /// TranslationX(global) and a TranslationY(global) `SetProperty` at the
    /// target composes correctly regardless of evaluation order, and a
    /// single globalized axis alone still behaves sensibly (see that
    /// function's own doc comment). `#[serde(default)]` (`false`) for the
    /// usual back-compat reason: every graph authored before this existed
    /// already meant "local," unchanged.
    SetProperty {
        node: NodeId,
        property: GraphProperty,
        #[serde(default)]
        global: bool,
    },
    /// Two inputs (sockets `0`, `1`), one output: the smaller of the two.
    Min,
    /// Two inputs (sockets `0`, `1`), one output: the larger of the two.
    Max,
    /// Two inputs (sockets `0`, `1`), one output: `1.0` if `0 > 1`, else `0.0`.
    GreaterThan,
    /// Two inputs (sockets `0`, `1`), one output: `1.0` if `0 < 1`, else `0.0`.
    LessThan,
    /// Two inputs (sockets `0`, `1`), one output: `1.0` if the two are
    /// exactly equal, else `0.0` — no epsilon tolerance (an author comparing
    /// two computed floats for exact equality is already choosing a strict
    /// check; approximate comparison is `LessThan`/`GreaterThan` composed
    /// with `Subtract`/`Abs`-style math instead, not this node's job).
    Equals,
    /// Two inputs (sockets `0`, `1`), one output — logical AND, `!= 0.0`
    /// treated as true on the way in, normalized to exactly `1.0`/`0.0` on
    /// the way out (see this module's own doc comment).
    And,
    /// Two inputs (sockets `0`, `1`), one output — logical OR, same
    /// true/false normalization as `And`.
    Or,
    /// One input (socket `0`), one output — logical NOT.
    Not,
    /// Three inputs — `condition` (socket `0`), `if_true` (socket `1`),
    /// `if_false` (socket `2`) — one output: the "logical filter" node,
    /// picking one of two values based on a condition rather than blending
    /// them (`Lerp` blends continuously; this switches discretely).
    Select,
    /// Reads a live *number* input off the state machine this graph belongs
    /// to — the same named `Inputs::number` value `ConditionDef::NumberGreaterThan`/
    /// `NumberLessThan` already check, exposed here as one more continuous
    /// graph source. No inputs, one output. A name that doesn't match any
    /// declared input simply reads `0.0` (`Inputs::number`'s own default),
    /// same "unknown name is harmless, not a validity concern this data
    /// model enforces" convention `ConditionDef`'s own input names already
    /// have.
    GetNumberInput { name: String },
    /// Reads a live *bool* input, normalized to `1.0`/`0.0` — see
    /// `GetNumberInput`'s own doc comment for the rest of this reasoning.
    GetBoolInput { name: String },
}

/// One scalar component of a node's properties a graph can read or write —
/// see this module's doc comment for why a `Vec2`/color property is split
/// into independent per-axis/per-channel entries rather than one
/// vector-valued socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphProperty {
    TranslationX,
    TranslationY,
    Rotation,
    ScaleX,
    ScaleY,
    AnchorX,
    AnchorY,
    FillColorR,
    FillColorG,
    FillColorB,
    FillColorA,
    /// `Node::visible`, as `1.0`/`0.0` — the same "booleans are numbers too"
    /// convention every comparison/logic node already uses, so a Blocks/
    /// Graph program's existing `SetProperty`/`GetProperty`/`ChangeProperty`
    /// vocabulary reaches "show"/"hide" for free, no dedicated
    /// statement/command needed.
    Visible,
}

/// One wire: `from_node`'s `from_socket`-th output feeds `to_node`'s
/// `to_socket`-th input. Multiple edges may target the same input socket in
/// authoring order without this type forbidding it — the evaluator (see
/// `an1mbyt3-runtime::graph`) resolves that by simply taking the last one
/// wired, same "author's own mistake, not a validity concern this data
/// model enforces" convention `TransitionDef`'s authoring-order tie-break
/// already establishes elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEdgeDef {
    pub from_node: u32,
    pub from_socket: u32,
    pub to_node: u32,
    pub to_socket: u32,
}
