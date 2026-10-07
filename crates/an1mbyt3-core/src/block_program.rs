//! `ProgramDef::Blocks` — a statement-based, Scratch-style authoring mode
//! for a Programmable State, siblings with `Graph`'s pure-dataflow node graph
//! and `Code`'s raw JS string (see `program.rs`'s own module doc for the
//! full comparison).
//!
//! `Graph` is deliberately continuous-dataflow-only (sockets and wires, no
//! statement sequencing, no control flow, no "when X happens" event) — a
//! good fit for wiring live sources into properties, but structurally
//! unable to hold `if`/`repeat`/`forever`/`wait` or an event trigger. This
//! module is the third variant that actually can, reusing `Graph`'s own
//! [`crate::GraphProperty`] vocabulary for "which scalar on which node" so
//! authors don't learn a second property-naming scheme, and following
//! `Graph`'s runtime-support promise, not `Code`'s: a `Blocks` program is
//! real Rust, interpreted by `an1mbyt3_runtime::block_interpreter`, and runs
//! everywhere `Graph` does (desktop editor, wasm player, any future embed) —
//! not editor-frontend-only like `Code`.
//!
//! Not included yet (deliberately, not forgotten): broadcast messages,
//! sprite cloning, user-declared variables (only the state machine's own
//! declared `Inputs` are readable, via `GetNumberInput`/`GetBoolInput`),
//! general-purpose numeric lists (only node lists exist so far, via
//! [`ListExpr`] — see its own doc comment for why).

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{ids::NodeId, program::GraphProperty};

/// Something that resolves to a single [`NodeId`] at run time — every place
/// in this module that used to hold a bare `NodeId` directly (`GoToXY`'s
/// `node`, `GetProperty`'s `node`, `Touching`'s `a`/`b`, ...) holds one of
/// these instead, so a statement/expression can address either an
/// explicitly-picked node (`Literal`, what every block already did) *or*
/// whichever node a [`Statement::ForEachNode`] is currently iterating
/// (`CurrentItem`) *or* a randomly-picked one out of a [`ListExpr`]
/// (`RandomFromList`) — without needing three near-identical copies of
/// every statement/expression that takes a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeRef {
    Literal(NodeId),
    /// The current item of the *innermost* enclosing `ForEachNode` loop —
    /// resolves to nothing (same "degrades harmlessly" convention this
    /// whole module follows) if used outside one; see
    /// `an1mbyt3_runtime::block_interpreter`'s own doc comment for how the
    /// interpreter tracks "innermost enclosing" across nested loops.
    CurrentItem,
    /// Picks one node uniformly at random out of `list`, resolved fresh
    /// every time this `NodeRef` is evaluated (not cached) — Scratch's
    /// "pick random" idea applied to a node list instead of a number range.
    RandomFromList(ListExpr),
    /// The node this *running script instance's own clone* is bound to —
    /// Scratch's "myself"/clone-local identity. Resolves to nothing (same
    /// "degrades harmlessly" convention this whole module follows) for a
    /// script instance that isn't a clone's own `CloneStarted` run — see
    /// `an1mbyt3_runtime::block_interpreter`'s `ScriptRuntime::bound_node`.
    Myself,
}

/// A value-producing list of nodes — deliberately its own small type
/// alongside [`Expr`] (which only ever produces a single `f32`) rather than
/// widening `Expr` itself to carry either a number or a list: every
/// existing numeric operator (`Add`, comparisons, ...) would otherwise need
/// to decide what "adding a list" even means. Scoped to *nodes* specifically
/// (not general-purpose numeric/string arrays) because the concrete,
/// motivating use case — "for each node with class X, do something to it,"
/// mirroring `scriptRuntime.ts`'s `getCompByClass` — only ever needs node
/// lists; a general array-of-numbers type is real future scope, not
/// something this pass silently forecloses, just doesn't build speculatively.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ListExpr {
    /// Every node whose `classes` (`Node::classes`) contains `class` —
    /// same selector `scriptRuntime.ts`'s `getCompByClass` already uses,
    /// applied artboard-wide.
    NodesWithClass { class: String },
}

/// Which named function [`Expr::MathFn`] applies — Scratch's own "[ ] of
/// ( )" operator block, the same one-dropdown-many-functions shape rather
/// than a dozen near-identical unary `Expr` variants.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MathFn {
    Abs,
    Floor,
    Ceiling,
    Round,
    Sqrt,
    Sin,
    Cos,
    Tan,
    /// Natural log.
    Ln,
    Log10,
}

/// A value-producing expression — Scratch's "reporter"/"boolean" blocks,
/// the part that plugs into another block's input slot rather than
/// stacking above/below it. Recursive (boxed) so arithmetic/logic compose
/// arbitrarily deep, the same shape any small expression AST takes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Number(f32),
    /// The constant π — a plain convenience reporter (equivalent to typing
    /// `3.14159265...` by hand, `std::f32::consts::PI` precision) rather
    /// than a value with any deeper meaning of its own.
    Pi,
    /// Reads a live property component off a node in the same artboard —
    /// mirrors `GraphNodeKind::GetProperty` exactly, same [`GraphProperty`]
    /// vocabulary.
    GetProperty { node: NodeRef, property: GraphProperty },
    /// Live pointer position in artboard space.
    MouseX,
    MouseY,
    /// `true` while `key` (a browser `KeyboardEvent.key` value, e.g.
    /// `"space"`, `"ArrowLeft"`, lowercased) is currently held down — the
    /// "key [ ] pressed?" sensing reporter. Level-triggered (queries current
    /// state), unlike [`EventKind::KeyPressed`]'s edge-triggered hat.
    KeyPressed { key: String },
    /// `true` while `a`'s and `b`'s world-space axis-aligned bounding boxes
    /// overlap — the "touching [ ]?" sensing reporter. Both sides are
    /// authored explicitly (no implicit "current sprite," unlike Scratch's
    /// own version of this block) — this engine's state machines aren't
    /// bound to one owning node the way a Scratch script is implicitly
    /// bound to its sprite, so "which node" is always an explicit pick here,
    /// the same convention `GraphNodeKind::GetProperty`/`SetProperty`
    /// already established. An intentional over-approximation (not pixel-
    /// perfect hit-testing), the same "safe over-approximation" precedent
    /// `Path::bounds` and `PathEditOverlay`'s live bounding box already
    /// establish elsewhere in this codebase; see
    /// `an1mbyt3_runtime::block_interpreter::touching`'s own doc comment for
    /// the exact geometry.
    Touching { a: NodeRef, b: NodeRef },
    /// `true` if `node`'s world-space bounding box overlaps *any* node
    /// tagged with `class` (`Node::classes`, the same selector
    /// `ListExpr::NodesWithClass`/`getCompByClass` already use) — `node`
    /// itself is excluded even if it happens to carry that same class (a
    /// node never counts as touching itself). The "is X touching any enemy"
    /// pattern `Touching` alone needs a `ForEachNode` loop plus an `Or`
    /// chain for, built directly into one reporter instead — the actual
    /// feature request this answers: avoiding repetitive per-pair
    /// `Touching` checks for "one thing vs. a whole group" collision, the
    /// single most common real game-logic pattern (bullet vs. any enemy,
    /// player vs. any coin, ...).
    TouchingClass { node: NodeRef, class: String },
    Add(Box<Expr>, Box<Expr>),
    Subtract(Box<Expr>, Box<Expr>),
    Multiply(Box<Expr>, Box<Expr>),
    /// `a / b`; `b == 0.0` yields `0.0` rather than propagating `inf`/`NaN`
    /// — same convention as `GraphNodeKind::Divide`.
    Divide(Box<Expr>, Box<Expr>),
    /// Floating-point remainder (`a - b * (a / b).floor()`, i.e. always
    /// same-sign-as-`b`, matching Scratch's own `mod` rather than Rust's
    /// `%` truncating-toward-zero behavior); `b == 0.0` yields `0.0`, same
    /// "no inf/NaN propagation" convention as `Divide`.
    Mod(Box<Expr>, Box<Expr>),
    GreaterThan(Box<Expr>, Box<Expr>),
    LessThan(Box<Expr>, Box<Expr>),
    Equals(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    /// A named [`MathFn`] applied to `value` — trig/rounding/etc.
    MathFn { op: MathFn, value: Box<Expr> },
    /// A uniform random integer in `[min, max]` inclusive (both floored
    /// first, then swapped if authored backwards) — Scratch's "pick random
    /// _ to _" for whole numbers.
    RandomInt { min: Box<Expr>, max: Box<Expr> },
    /// A uniform random value in `[min, max]` (swapped if authored
    /// backwards), no rounding.
    RandomFloat { min: Box<Expr>, max: Box<Expr> },
    /// Reads a live *number* input off this state machine — same source
    /// `GraphNodeKind::GetNumberInput`/`ConditionDef::NumberGreaterThan`
    /// already read.
    GetNumberInput { name: String },
    GetBoolInput { name: String },
}

impl Expr {
    /// `0.0` for every variant with no sensible truthiness beyond "nonzero"
    /// — matches `Graph`'s own "booleans are numbers too, nonzero is true"
    /// convention (`program.rs`'s module doc), so `If`/`Repeat`'s condition/
    /// count expressions don't need a separate boolean `Expr` variant set.
    pub fn is_truthy(value: f32) -> bool {
        value != 0.0
    }

    pub fn from_bool(value: bool) -> f32 {
        if value { 1.0 } else { 0.0 }
    }
}

/// A stack block — Scratch's "does something, connects top/bottom to the
/// next one" vocabulary, as opposed to [`Expr`]'s "plugs into a slot."
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    /// Sets `node`'s translation directly (not relative) — Scratch's
    /// "go to x: _ y: _".
    GoToXY { node: NodeRef, x: Expr, y: Expr },
    /// Writes `value` onto `node`'s `property` — Scratch's "set [ ] to _".
    SetProperty { node: NodeRef, property: GraphProperty, value: Expr },
    /// Formats `value` (rounded to `decimals` places) as a decimal string and
    /// writes it onto `node`'s `TextNode::content` — a no-op (degrades
    /// harmlessly, same convention as every other `NodeRef`-addressed
    /// statement here) if `node` doesn't resolve to a real `Text` node.
    /// `GraphProperty`'s vocabulary has no "string" slot at all (every value
    /// in this whole module is an `f32` — see `Expr::is_truthy`'s own doc
    /// comment), so a live numeric HUD display (a percentage, a counter)
    /// can't be done through `SetProperty`/`ChangeProperty` the way every
    /// other property can; this is the one statement that reaches past that
    /// and actually rewrites a node's displayed text, specifically so a
    /// counter can read as real text updating in place rather than needing
    /// a "digit mama" — ten pre-authored glyph nodes per digit, visibility-
    /// swapped each tick — the only other way to show a live number at all
    /// before this existed.
    SetTextNumber { node: NodeRef, value: Expr, decimals: u32 },
    /// Adds `delta` to `node`'s current `property` value — Scratch's
    /// "change [ ] by _" (e.g. the dino example's "change y by 0.3").
    ChangeProperty { node: NodeRef, property: GraphProperty, delta: Expr },
    /// Suspends this script for `seconds` real time before its next
    /// statement runs — the one statement that spans more than one frame
    /// by itself; see `block_interpreter`'s doc comment for how that's
    /// actually implemented (a resumable, explicit-stack interpreter, not
    /// a blocking sleep).
    Wait { seconds: Expr },
    If { condition: Expr, then_branch: Vec<Statement>, else_branch: Vec<Statement> },
    /// Runs `body` `count` times (evaluated once, on entry, and floored/
    /// clamped to `0..`) before continuing past this statement.
    Repeat { count: Expr, body: Vec<Statement> },
    /// Runs `body` in an endless loop — never falls through to whatever
    /// follows it in the same statement list (matching Scratch's own
    /// "forever" exactly; any statements authored after a `Forever` in the
    /// same `Vec` are simply unreachable, an author's own mistake, not a
    /// validity concern this data model enforces — same convention
    /// `GraphEdgeDef`'s doc comment already establishes for a different
    /// kind of unreachable-authoring case).
    Forever { body: Vec<Statement> },
    /// Runs `body` once per node in `list` (evaluated once, on entry —
    /// nodes added to/removed from the matching class mid-loop don't
    /// retroactively change this pass), with `NodeRef::CurrentItem`
    /// resolving to that node for the whole of one pass through `body`.
    /// Scratch has no exact equivalent (its lists are numeric/string, not
    /// node-valued) — this is the concrete feature the user asked for
    /// directly: "for each [node] with class X, do something."
    ForEachNode { list: ListExpr, body: Vec<Statement> },
    /// Spawns a live, ephemeral copy of `node` — Scratch's "create clone of
    /// [ ]". Fires `EventKind::CloneStarted` for the new clone (see that
    /// variant's own doc comment); never blocks, never fails loudly (a
    /// `node` that doesn't resolve just spawns nothing, same "author's own
    /// mistake degrades harmlessly" convention this whole module follows).
    /// A clone is one node only, not a subtree — see
    /// `an1mbyt3_core::Artboard::spawn_clone_of`'s own doc comment for why.
    CreateCloneOf { node: NodeRef },
    /// Removes this script instance's own `NodeRef::Myself` node from the
    /// artboard and ends this script instance — Scratch's "delete this
    /// clone". A no-op (beyond ending the script) outside a clone's own
    /// `CloneStarted` run, where there's no bound node to remove — same
    /// "degrades harmlessly" convention as everywhere else `NodeRef::Myself`
    /// can resolve to nothing.
    DeleteThisClone,
}

/// What starts a [`BlockScript`] running.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EventKind {
    /// Fires once, the instant this state becomes active — the "when green
    /// flag clicked" analog (a state entering is this engine's closest
    /// equivalent to a game/session starting, since there's no separate
    /// "game" concept above the state machine). Named `StateTriggered`
    /// (not `GameStart`, an earlier name for this), since "this state just
    /// became active" is the actual, more general semantics — a
    /// Programmable State isn't only ever the very first thing a game does.
    StateTriggered,
    /// Fires once on `key`'s down-edge (pressed this frame, not held over
    /// from a previous one) while this state is active — edge-triggered,
    /// unlike [`Expr::KeyPressed`]'s level-triggered reporter.
    ///
    /// `overlapping`: `false` (the default, `#[serde(default)]` for older
    /// saved programs) matches this event's original, single-instance
    /// behavior — a new press is ignored while the previous run is still
    /// going. `true` matches Scratch's own actual hat-block semantics
    /// instead (a new press always spawns a *new*, independent run
    /// alongside any still in progress — real Scratch never suppresses a
    /// retrigger) — an author-facing toggle since both are legitimate:
    /// "one jump at a time" wants `false`; "every tap spawns another
    /// particle burst, even mid-animation" wants `true`.
    KeyPressed {
        key: String,
        #[serde(default)]
        overlapping: bool,
    },
    /// Active for as long as `key` is held down — starts `body` the instant
    /// the key goes down, keeps resuming it (across `Wait`s, same as any
    /// other script) every frame it's still held, and *immediately discards*
    /// whatever's in progress (even mid-`Wait`) the instant it's released —
    /// unlike `KeyPressed`, there's no "let the current run finish" grace
    /// period, since the whole point is "only while actually held." If
    /// `body` runs to completion while the key is still down (anything
    /// short of its own `Forever`), it simply restarts from the top the
    /// very next frame — "while held, keep doing this" needs no explicit
    /// `Forever` wrapper of its own to get that repeating behavior.
    KeyHeld { key: String },
    /// Fires once for a freshly spawned clone, the instant `Statement::CreateCloneOf`
    /// creates it — Scratch's "when I start as a clone" hat. `NodeRef::Myself`
    /// inside this script's own body resolves to that clone's own `NodeId` —
    /// see `an1mbyt3_runtime::block_interpreter`'s own doc comment for how a
    /// script instance's `bound_node` gets set. Never fires for the
    /// non-clone scripts already running when a clone is created — only the
    /// brand-new instance spawned specifically for it.
    CloneStarted,
}

/// One event and the statements it runs — a state can author several
/// (multiple independent "when X" scripts), matching Scratch's own "many
/// hat-block stacks per sprite."
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockScript {
    pub event: EventKind,
    pub statements: Vec<Statement>,
    /// This script's own hat block's position on the Blocks tab's canvas —
    /// purely an authoring-tool concern (`an1mbyt3_runtime::block_interpreter`
    /// never reads it), same reasoning as `GraphNodeDef::position`. `None`
    /// for a script never explicitly positioned (brand new, or saved before
    /// this field existed) — only then does the Blocks editor fall back to
    /// Blockly's own default auto-placement; a script genuinely placed at
    /// the origin round-trips as `Some(Vec2::ZERO)`, not re-auto-placed.
    #[serde(default)]
    pub position: Option<Vec2>,
}

/// A whole authored Blocks program for one state — see this module's own
/// doc comment for how it fits alongside `Graph`/`Code`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockProgramDef {
    pub scripts: Vec<BlockScript>,
    /// How often this program's scripts actually tick, independent of the
    /// render frame rate — `None` (the default, `#[serde(default)]` so
    /// older saved programs load unchanged) ticks every render frame, same
    /// as before this field existed. `Some(n)` throttles to `n` times per
    /// second (see `an1mbyt3_runtime::block_interpreter::step`'s own
    /// throttle logic) — useful for deliberately chunky, low-fps game logic
    /// (an old-school "the game itself runs at 4fps" look) or just for
    /// controlling CPU cost independent of however fast the display
    /// happens to redraw.
    #[serde(default)]
    pub ticks_per_second: Option<f32>,
}
