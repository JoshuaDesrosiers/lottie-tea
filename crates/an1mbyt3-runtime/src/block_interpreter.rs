//! Live, resumable execution of a Programmable State's
//! [`an1mbyt3_core::BlockProgramDef`] — the runtime half of
//! `an1mbyt3_core::block_program` (which owns the authored data; see that
//! module's doc comment for the overall design and why this exists
//! alongside `graph.rs`'s pure-dataflow evaluator rather than replacing it).
//!
//! **Resumability**: `Wait`/`Repeat`/`Forever`/`ForEachNode` need to span
//! more than one frame — a `Forever` loop can't just run to (non-existent)
//! completion in one `step()` call, and `Wait` needs to suspend a script
//! mid-statement-list until real time passes. This is a small explicit-
//! stack tree-walking interpreter, the same "threads with a step function
//! that runs until it blocks" model Scratch's own VM uses for exactly this
//! problem. Each [`Frame`] owns a *cloned* copy of whichever `Vec<Statement>`
//! it's currently walking (an `If`'s branch, a `Repeat`/`Forever`/
//! `ForEachNode`'s body) rather than borrowing from the `BlockProgramDef`
//! passed into `step` — cloning a handful of small authored statements once
//! per loop-body-(re)entry (not per tick) is cheap at this scale and means
//! [`ScriptRuntime`] has no lifetime, so it can live inside `Layer` across
//! frames with no unsafe code and no borrow-checker fights against `step`'s
//! own `&BlockProgramDef` argument being freshly (re)borrowed every call.
//!
//! **`NodeRef::CurrentItem`**: a `ForEachNode` loop's "current item" isn't
//! threaded through `eval`/`execute` as an ordinary parameter update — it's
//! resolved fresh before every single statement by scanning the *whole*
//! frame stack for the innermost `Frame::ForEach` (`current_foreach_item`),
//! not just whichever frame happens to be on top. This is what makes a
//! nested `If`/`Repeat` *inside* a `ForEachNode`'s body still resolve
//! `CurrentItem` correctly — those frames don't carry an item of their own,
//! so the scan has to look past them to the loop that actually bound one.
//!
//! **Randomness** (`RandomInt`/`RandomFloat`/`NodeRef::RandomFromList`):
//! this crate takes no dependency on `rand`/`getrandom` (avoiding a new
//! wasm32 entropy-source concern this session didn't need to open) — a
//! small self-advancing integer hash (`next_random_unit`, the same
//! finalizer-style avalanche `graph.rs`'s own `hash_seed_to_unit_f32` uses,
//! just mutable/advancing instead of pure) seeded once per `BlockRuntime`
//! and mixed with live, per-call entropy (`ctx.mouse_artboard`/
//! `elapsed_seconds` bits) so a fresh session doesn't always draw the exact
//! same "random" sequence. Good enough for gameplay (dice rolls, picking a
//! spawn point), not a cryptographic or statistically-rigorous RNG.
//!
//! **Per-program state** ([`ProgramState`]): the RNG, the program's
//! number variables (`Statement::SetVariable`/`Expr::Variable`) and the
//! queue of pending state-machine input writes (`Statement::SetNumberInput`/
//! `SetBoolInput`/`FireTrigger`) all live in one struct, threaded through
//! `eval`/`execute` as a single `&mut ProgramState` instead of three
//! separate parameters. All three share the same lifetime (rebuilt fresh
//! with `BlockRuntime` on every state entry) and the same "every script in
//! this program sees the same one" scope, so bundling them keeps every
//! signature in this file stable as more such state gets added.
//!
//! **Input writes are deferred**: `BlockContext::inputs` is a shared
//! borrow of the machine's own `Inputs` (every layer reads the same set
//! while stepping), so a Blocks program can't write it directly mid-step.
//! Writes queue up as [`InputWrite`]s instead; `StateMachine::apply` drains
//! them via [`BlockRuntime::take_input_writes`] once every layer's program
//! has run, the same "consumed once per frame, by the caller that drives
//! everything" placement `KeyboardState::end_frame` already uses. Within
//! the same frame, `GetNumberInput`/`GetBoolInput` read through the queue
//! first, so a script always sees its own latest write. A fired trigger is
//! seen by the *next* `StateMachine::advance` (which runs before `apply`
//! each frame), then cleared like any other trigger.
//!
//! **Multi-frame motion** (`Statement::GlideTo`) is a `Frame::Glide` on
//! the explicit stack, exactly like a loop body: while it's on top,
//! `run_script` interpolates the node and yields for the frame instead of
//! running further statements, then pops it once the time is up, so a
//! glide composes with `Repeat`/`Forever`/`If` for free.
//!
//! A per-frame instruction budget (`MAX_INSTRUCTIONS_PER_STEP`) caps how
//! much of one script's stack a single `step()` call will unwind — an
//! author's `forever { }` with an empty, `Wait`-less body would otherwise
//! spin forever *inside* one `step()` call and never return control to
//! rendering/other scripts. Hitting the cap simply defers the rest to next
//! frame (the stack is left exactly where it was) rather than erroring —
//! same "an author's own mistake degrades to a well-defined but not
//! obviously meaningful result, never a panic" convention `graph.rs`'s own
//! cycle-handling doc comment already establishes for this workspace.

use std::collections::HashMap;

use an1mbyt3_core::{Artboard, BlockProgramDef, EventKind, Expr, ListExpr, MathFn, Node, NodeId, NodeKind, NodeRef, Statement};
use glam::Vec2;

use crate::graph::{is_truthy, normalize_bool, read_property, write_property};
use crate::keyboard::KeyboardState;
use crate::state_machine::Inputs;

const MAX_INSTRUCTIONS_PER_STEP: u32 = 10_000;

/// Live sources a Blocks program's expressions/statements read from —
/// mirrors `graph::GraphContext` almost exactly (same `elapsed_seconds`/
/// `mouse_artboard`/`inputs`), plus `keys`. `elapsed_seconds` is `Layer`'s
/// own dedicated `blocks_elapsed` clock (see that field's doc comment) —
/// deliberately *not* the state's `Playback::time`, which can freeze
/// (`duration <= 0.0`, or simply paused) or wrap/reverse (`LoopMode::Loop`/
/// `PingPong`), any of which would break `Wait`'s wake-time comparison.
/// `blocks_elapsed` instead just accumulates real per-frame `delta_seconds`
/// unconditionally, so it's always monotonically increasing while a state
/// is active — `Wait`'s comparison against it needs no additional per-frame
/// delta of its own.
pub struct BlockContext<'a> {
    pub elapsed_seconds: f32,
    pub mouse_artboard: Vec2,
    pub keys: &'a KeyboardState,
    pub inputs: &'a Inputs,
}

/// One level of a running script's explicit call stack. See this module's
/// doc comment for why each frame owns a clone of its statement list
/// instead of borrowing one.
#[derive(Debug, Clone)]
enum Frame {
    /// A plain top-to-bottom statement list (a script's own top level, or
    /// an `If`'s chosen branch) — finishes and pops once `index` reaches
    /// the end.
    Sequence { statements: Vec<Statement>, index: usize },
    /// A `Repeat`/`Forever` body — `remaining: None` means `Forever`
    /// (never pops on its own); `Some(n)` counts down once per full pass
    /// through `body` and pops when it reaches `0`.
    Loop { body: Vec<Statement>, index: usize, remaining: Option<u32> },
    /// A `ForEachNode` body — `items` resolved once, on entry; `item_index`
    /// (which node) and `stmt_index` (which statement of `body`, for that
    /// node) advance independently, `stmt_index` resetting to `0` every
    /// time `item_index` moves to the next node. Pops once `item_index`
    /// reaches `items.len()`.
    ForEach { items: Vec<NodeId>, item_index: usize, body: Vec<Statement>, stmt_index: usize },
    /// An in-progress `Statement::GlideTo`: linearly interpolates `node`'s
    /// translation from `from` to `to` between `start` and
    /// `start + duration` (both on `BlockContext::elapsed_seconds`'s clock),
    /// yielding the script for one frame each time it's on top of the
    /// stack, and pops once the time is up (landing exactly on `to`).
    /// `duration` is always `> 0` here; `execute` handles a zero/negative
    /// glide as an instant jump instead of pushing one of these.
    Glide { node: NodeId, from: Vec2, to: Vec2, start: f32, duration: f32 },
}

/// Scans `stack` from innermost (top) outward for the closest enclosing
/// `Frame::ForEach`, returning the node it's currently on — see this
/// module's own doc comment for why this is a full-stack scan rather than
/// just checking whichever frame happens to be on top (an `If`/`Repeat`
/// nested inside a `ForEachNode`'s body sits *above* the `ForEach` frame on
/// the stack while its own statements run, but `CurrentItem` inside that
/// nested block still needs to resolve to the outer loop's node).
fn current_foreach_item(stack: &[Frame]) -> Option<NodeId> {
    stack.iter().rev().find_map(|frame| match frame {
        Frame::ForEach { items, item_index, .. } => items.get(*item_index).copied(),
        _ => None,
    })
}

/// One *running instance* of a script — a call stack plus an in-progress
/// `Wait`'s wake time, if any. `idle` (empty stack, not waiting) means this
/// instance has run to completion — [`step`] prunes idle instances out of
/// their script's pool every frame, rather than leaving them to sit there
/// forever.
#[derive(Debug, Clone, Default)]
struct ScriptRuntime {
    stack: Vec<Frame>,
    waiting_until: Option<f32>,
    /// This instance's own `NodeRef::Myself` binding — `Some` only for a
    /// `CloneStarted` instance (see `step`'s own clone-spawning pass), the
    /// clone this particular run is "about." Constant for this instance's
    /// whole lifetime (never reassigned after `started_for_clone`), unlike
    /// `NodeRef::CurrentItem` (which changes every pass through a
    /// `ForEachNode` and so is resolved fresh via a stack scan instead of
    /// stored here) — one running script is always the same clone, or none.
    bound_node: Option<NodeId>,
}

impl ScriptRuntime {
    fn idle(&self) -> bool {
        self.stack.is_empty() && self.waiting_until.is_none()
    }

    fn started(statements: Vec<Statement>) -> Self {
        Self { stack: vec![Frame::Sequence { statements, index: 0 }], waiting_until: None, bound_node: None }
    }

    /// Same as `started`, but bound to a specific clone — see `bound_node`'s
    /// own doc comment. The one caller is `step`'s clone-spawning pass,
    /// building a fresh `CloneStarted` instance for a brand-new clone.
    fn started_for_clone(statements: Vec<Statement>, clone: NodeId) -> Self {
        Self { stack: vec![Frame::Sequence { statements, index: 0 }], waiting_until: None, bound_node: Some(clone) }
    }
}

/// A whole `BlockProgramDef`'s live execution state — one *pool* of
/// concurrently-running [`ScriptRuntime`] instances per authored
/// [`BlockScript`], index-aligned with `BlockProgramDef::scripts`, plus the
/// running RNG state every `RandomInt`/`RandomFloat`/`NodeRef::RandomFromList`
/// in this program draws from (see this module's own doc comment on why
/// it's a plain advancing hash, not a `rand` crate generator).
///
/// A pool rather than a single `ScriptRuntime` because
/// [`EventKind::KeyPressed`]'s `overlapping: true` needs several independent
/// instances of the *same* script alive at once (a new key-press spawns a
/// new instance alongside any still finishing up — real Scratch's actual
/// hat-block behavior). Every other case ([`EventKind::StateTriggered`], a
/// non-overlapping `KeyPressed`, [`EventKind::KeyHeld`]) just happens to
/// only ever hold zero or one instance — see [`step`]'s own per-event
/// (re)trigger logic for the actual rules, not this type.
///
/// Lives inside `Layer` (an1mbyt3-runtime's `state_machine.rs`), reset
/// (rebuilt fresh via `block_runtime_for`) every time that layer enters a
/// state — both `Layer::from_states`' initial entry and
/// `Layer::transition_to` call it, so a state (re-)entry always restarts
/// every `StateTriggered` script from the top, exactly like a fresh "green
/// flag click" would (and gets a fresh RNG seed too).
#[derive(Debug, Clone, Default)]
pub struct BlockRuntime {
    scripts: Vec<Vec<ScriptRuntime>>,
    /// RNG, variables and pending input writes; see [`ProgramState`].
    state: ProgramState,
    /// The next `ctx.elapsed_seconds` at which `step` is allowed to
    /// actually run scripts, when `BlockProgramDef::ticks_per_second` is
    /// set — see `step`'s own throttle check. `0.0` (the derived default)
    /// means "run immediately," matching an uncapped program's existing
    /// day-one behavior of ticking the very first time `step` is called.
    next_tick_at: f32,
}

impl BlockRuntime {
    /// Builds a fresh runtime for `program`, immediately starting every
    /// `StateTriggered` script (the "when the game starts"/green-flag analog
    /// — see `EventKind::StateTriggered`'s own doc comment for why a state
    /// becoming active is this engine's closest equivalent) as its pool's
    /// one instance. `KeyPressed`/`KeyHeld` scripts start with an empty pool;
    /// `step` (re)populates them on their own key's edge/level.
    pub fn fresh_for(program: &BlockProgramDef) -> Self {
        let scripts = program
            .scripts
            .iter()
            .map(|script| if matches!(script.event, EventKind::StateTriggered) { vec![ScriptRuntime::started(script.statements.clone())] } else { Vec::new() })
            .collect();
        // A fixed, non-zero starting state (splitmix64's own usual golden-
        // ratio-derived constant, truncated to 32 bits) — `next_random_unit`
        // mixes in live per-call entropy on top of this anyway, so the
        // exact starting value only matters in that it's non-zero (an
        // all-zero state would need at least one entropy-mix before the
        // hash's own avalanche kicks in meaningfully).
        Self { scripts, state: ProgramState { rng: 0x9E3779B1, ..ProgramState::default() }, next_tick_at: 0.0 }
    }

    /// Drains every state-machine input write this program queued since
    /// the last call. See this module's doc comment ("Input writes are
    /// deferred") for why they're queued rather than applied in place.
    /// `StateMachine::apply` calls this once per layer per frame, right
    /// after `step`, and applies the result to its own `Inputs` in order.
    pub fn take_input_writes(&mut self) -> Vec<InputWrite> {
        std::mem::take(&mut self.state.input_writes)
    }
}

/// One queued write to the owning state machine's `Inputs`, produced by
/// `Statement::SetNumberInput`/`SetBoolInput`/`FireTrigger` and drained by
/// [`BlockRuntime::take_input_writes`]. See this module's doc comment for
/// why these are deferred rather than written straight into `Inputs`.
#[derive(Debug, Clone, PartialEq)]
pub enum InputWrite {
    Number(String, f32),
    Bool(String, bool),
    Trigger(String),
}

impl InputWrite {
    /// Applies this write to `inputs`: the one place a queued write turns
    /// into a real `Inputs` mutation.
    pub fn apply_to(self, inputs: &mut Inputs) {
        match self {
            InputWrite::Number(name, value) => inputs.set_number(name, value),
            InputWrite::Bool(name, value) => inputs.set_bool(name, value),
            InputWrite::Trigger(name) => inputs.fire_trigger(name),
        }
    }
}

/// Everything one Blocks program's scripts share and mutate while running,
/// besides the artboard itself. See this module's doc comment
/// ("Per-program state") for why it's one struct. Lives in
/// [`BlockRuntime`], so it's reset on every state entry.
#[derive(Debug, Clone, Default)]
struct ProgramState {
    /// The advancing hash state behind every random draw; see
    /// `next_random_unit`.
    rng: u32,
    /// Number variables by name (`Statement::SetVariable` et al.); an
    /// absent name reads as `0.0`.
    variables: HashMap<String, f32>,
    /// Input writes queued since the last drain, oldest first; see
    /// [`InputWrite`].
    input_writes: Vec<InputWrite>,
}

impl ProgramState {
    /// The latest *queued* (not yet applied) number write to `name`, if
    /// any: what lets `GetNumberInput` read a script's own write back in
    /// the same frame it made it.
    fn pending_number(&self, name: &str) -> Option<f32> {
        self.input_writes.iter().rev().find_map(|w| match w {
            InputWrite::Number(n, v) if n == name => Some(*v),
            _ => None,
        })
    }

    /// Bool counterpart to `pending_number`.
    fn pending_bool(&self, name: &str) -> Option<bool> {
        self.input_writes.iter().rev().find_map(|w| match w {
            InputWrite::Bool(n, v) if n == name => Some(*v),
            _ => None,
        })
    }
}

/// One (very small, non-cryptographic) step of a self-advancing integer
/// hash — see this module's own doc comment on why this exists instead of
/// a `rand` crate dependency. `entropy` (caller-supplied, typically live
/// mouse-position/elapsed-time bits) is mixed in *before* hashing, so
/// successive calls within the same frame still diverge (the state
/// advances every call) and different frames/sessions draw different
/// sequences (the entropy differs).
pub(crate) fn next_random_unit(rng_state: &mut u32, entropy: u32) -> f32 {
    *rng_state = rng_state.wrapping_add(0x9E3779B1).wrapping_add(entropy);
    let mut x = *rng_state;
    x ^= x >> 15;
    x = x.wrapping_mul(0x85EBCA6B);
    x ^= x >> 13;
    x = x.wrapping_mul(0xC2B2AE35);
    x ^= x >> 16;
    (x as f32) / (u32::MAX as f32)
}

fn random_entropy(ctx: &BlockContext) -> u32 {
    ctx.mouse_artboard.x.to_bits() ^ ctx.mouse_artboard.y.to_bits() ^ ctx.elapsed_seconds.to_bits()
}

/// Resolves `list` against `artboard`'s current contents — the only
/// variant so far (`NodesWithClass`) mirrors `scriptRuntime.ts`'s
/// `getCompByClass` selector, applied artboard-wide via the same real-
/// draw-order walk (`Artboard::pickable_nodes`) hit-testing/rendering
/// already use, so a list's iteration order matches what an author would
/// see scanning the Layers panel top to bottom.
fn resolve_list(list: &ListExpr, artboard: &Artboard) -> Vec<NodeId> {
    match list {
        ListExpr::NodesWithClass { class } => artboard.pickable_nodes().filter(|(_, node)| node.classes.iter().any(|c| c == class)).map(|(id, _)| id).collect(),
    }
}

/// Resolves a `NodeRef` to a concrete `NodeId`, or `None` if it can't be
/// (an empty `RandomFromList`, or a `CurrentItem` used outside any
/// `ForEachNode` — both degrade harmlessly, same convention this whole
/// module follows elsewhere).
fn resolve_node_ref(node_ref: &NodeRef, artboard: &Artboard, current_item: Option<NodeId>, bound_node: Option<NodeId>, ctx: &BlockContext, state: &mut ProgramState) -> Option<NodeId> {
    match node_ref {
        NodeRef::Literal(id) => Some(*id),
        NodeRef::CurrentItem => current_item,
        NodeRef::RandomFromList(list) => {
            let items = resolve_list(list, artboard);
            if items.is_empty() {
                return None;
            }
            let t = next_random_unit(&mut state.rng, random_entropy(ctx));
            let index = ((t * items.len() as f32) as usize).min(items.len() - 1);
            items.get(index).copied()
        }
        NodeRef::Myself => bound_node,
    }
}

/// Evaluates one expression against live `ctx`/`artboard` state.
/// `current_item`/`bound_node`/`state` are threaded through purely so
/// `NodeRef` resolution (`GetProperty`'s `node`, `Touching`'s `a`/`b`),
/// `RandomInt`/`RandomFloat` and `Variable` have what they need at
/// arbitrary recursion depth (see this module's own doc comment for each).
/// `state` is `&mut` only because random draws advance the RNG; nothing
/// else an expression does writes to it.
fn eval(expr: &Expr, ctx: &BlockContext, artboard: &Artboard, current_item: Option<NodeId>, bound_node: Option<NodeId>, state: &mut ProgramState) -> f32 {
    let recurse = |e: &Expr, state: &mut ProgramState| eval(e, ctx, artboard, current_item, bound_node, state);
    match expr {
        Expr::Number(v) => *v,
        Expr::Pi => std::f32::consts::PI,
        Expr::GetProperty { node, property } => resolve_node_ref(node, artboard, current_item, bound_node, ctx, state)
            .and_then(|id| artboard.node(id))
            .map(|n| read_property(n, *property))
            .unwrap_or(0.0),
        Expr::MouseX => ctx.mouse_artboard.x,
        Expr::MouseY => ctx.mouse_artboard.y,
        Expr::KeyPressed { key } => normalize_bool(ctx.keys.is_down(key)),
        Expr::Touching { a, b } => {
            let resolved_a = resolve_node_ref(a, artboard, current_item, bound_node, ctx, state);
            let resolved_b = resolve_node_ref(b, artboard, current_item, bound_node, ctx, state);
            match (resolved_a, resolved_b) {
                (Some(a), Some(b)) => normalize_bool(touching(artboard, a, b)),
                _ => 0.0,
            }
        }
        Expr::TouchingClass { node, class } => {
            let resolved = resolve_node_ref(node, artboard, current_item, bound_node, ctx, state);
            let touches_any = resolved.is_some_and(|id| {
                artboard
                    .pickable_nodes()
                    .any(|(candidate_id, candidate)| candidate_id != id && candidate.classes.iter().any(|c| c == class) && touching(artboard, id, candidate_id))
            });
            normalize_bool(touches_any)
        }
        Expr::Add(a, b) => recurse(a, state) + recurse(b, state),
        Expr::Subtract(a, b) => recurse(a, state) - recurse(b, state),
        Expr::Multiply(a, b) => recurse(a, state) * recurse(b, state),
        Expr::Divide(a, b) => {
            let divisor = recurse(b, state);
            if divisor == 0.0 { 0.0 } else { recurse(a, state) / divisor }
        }
        Expr::Mod(a, b) => {
            let divisor = recurse(b, state);
            if divisor == 0.0 {
                0.0
            } else {
                let dividend = recurse(a, state);
                dividend - divisor * (dividend / divisor).floor()
            }
        }
        Expr::GreaterThan(a, b) => normalize_bool(recurse(a, state) > recurse(b, state)),
        Expr::LessThan(a, b) => normalize_bool(recurse(a, state) < recurse(b, state)),
        Expr::Equals(a, b) => normalize_bool(recurse(a, state) == recurse(b, state)),
        Expr::And(a, b) => normalize_bool(is_truthy(recurse(a, state)) && is_truthy(recurse(b, state))),
        Expr::Or(a, b) => normalize_bool(is_truthy(recurse(a, state)) || is_truthy(recurse(b, state))),
        Expr::Not(a) => normalize_bool(!is_truthy(recurse(a, state))),
        Expr::MathFn { op, value } => {
            let v = recurse(value, state);
            match op {
                MathFn::Abs => v.abs(),
                MathFn::Floor => v.floor(),
                MathFn::Ceiling => v.ceil(),
                MathFn::Round => v.round(),
                MathFn::Sqrt => v.max(0.0).sqrt(),
                // Radians, not degrees — matches `Transform2D::rotation`'s
                // own unit (and `GraphProperty::Rotation`'s), the internal-
                // consistency choice over Scratch's own degree-based trig
                // (Scratch's whole rotation model is degrees; ours is
                // radians throughout, so degree-based trig here would be
                // the one inconsistent unit in this engine).
                MathFn::Sin => v.sin(),
                MathFn::Cos => v.cos(),
                MathFn::Tan => v.tan(),
                MathFn::Ln => if v > 0.0 { v.ln() } else { 0.0 },
                MathFn::Log10 => if v > 0.0 { v.log10() } else { 0.0 },
            }
        }
        Expr::RandomInt { min, max } => {
            let a = recurse(min, state).floor();
            let b = recurse(max, state).floor();
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            let t = next_random_unit(&mut state.rng, random_entropy(ctx));
            (lo + (t * (hi - lo + 1.0)).floor()).min(hi)
        }
        Expr::RandomFloat { min, max } => {
            let a = recurse(min, state);
            let b = recurse(max, state);
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            let t = next_random_unit(&mut state.rng, random_entropy(ctx));
            lo + (hi - lo) * t
        }
        // Queued-but-unapplied writes win over `ctx.inputs`; see
        // `ProgramState::pending_number`.
        Expr::GetNumberInput { name } => state.pending_number(name).unwrap_or_else(|| ctx.inputs.number(name)),
        Expr::GetBoolInput { name } => normalize_bool(state.pending_bool(name).unwrap_or_else(|| ctx.inputs.bool(name))),
        Expr::DistanceToMouse { node } => resolve_node_ref(node, artboard, current_item, bound_node, ctx, state)
            .and_then(|id| world_origin(artboard, id))
            .map(|origin| origin.distance(ctx.mouse_artboard))
            .unwrap_or(0.0),
        Expr::DistanceTo { a, b } => {
            let a = resolve_node_ref(a, artboard, current_item, bound_node, ctx, state).and_then(|id| world_origin(artboard, id));
            let b = resolve_node_ref(b, artboard, current_item, bound_node, ctx, state).and_then(|id| world_origin(artboard, id));
            match (a, b) {
                (Some(a), Some(b)) => a.distance(b),
                _ => 0.0,
            }
        }
        // `elapsed_seconds` is the layer's own per-state clock, reset to 0
        // on every state entry: exactly "seconds since this state started."
        Expr::Timer => ctx.elapsed_seconds,
        Expr::Min(a, b) => recurse(a, state).min(recurse(b, state)),
        Expr::Max(a, b) => recurse(a, state).max(recurse(b, state)),
        Expr::Variable { name } => state.variables.get(name).copied().unwrap_or(0.0),
    }
}

/// `node`'s local (pre-transform) axis-aligned bounding box — `Shape`'s own
/// `Path::bounds`, or an `Image`'s centered `width`x`height` rect (the exact
/// same two-kind convention `WasmEditor::pick_node_at` already hit-tests
/// against); every other kind has no natural bounds and reads as an empty
/// box at the origin (never touching anything, a harmless default — same
/// "an author's own mistake degrades harmlessly" convention this whole
/// module follows).
fn local_bounds(node: &Node) -> (Vec2, Vec2) {
    match &node.kind {
        NodeKind::Shape(shape) => shape.path.bounds(),
        NodeKind::Image(image) => (Vec2::new(-image.width / 2.0, -image.height / 2.0), Vec2::new(image.width / 2.0, image.height / 2.0)),
        NodeKind::Tilemap(tilemap) => {
            let half = Vec2::new(tilemap.map_width as f32 * tilemap.cell_size / 2.0, tilemap.map_height as f32 * tilemap.cell_size / 2.0);
            (-half, half)
        }
        NodeKind::Group(_) | NodeKind::Bone(_) | NodeKind::Instance(_) | NodeKind::Text(_) | NodeKind::ParticleSystem(_) | NodeKind::Camera(_) | NodeKind::Light2D(_) | NodeKind::Trail(_) | NodeKind::Node3D(_) => (Vec2::ZERO, Vec2::ZERO),
    }
}

/// `true` while `a` and `b`'s world-space axis-aligned bounding boxes
/// overlap — the "touching [ ]?" sensing reporter's real geometry. Computed
/// by transforming each node's own `local_bounds` corners through its
/// `Artboard::world_transform`'s `to_affine2` and taking the min/max of the
/// four transformed corners (so a rotated shape's world AABB still fully
/// contains it, not just its unrotated footprint), then a plain rectangle-
/// overlap test between the two resulting world AABBs.
///
/// This is an intentional over-approximation, not pixel-perfect hit-testing
/// — a rotated box's world AABB is larger than the box itself, so two
/// diagonally-rotated shapes whose *corners* nearly touch may read as
/// touching slightly before their actual edges do. Same "safe over-
/// approximation" precedent `Path::bounds` and `PathEditOverlay`'s own live
/// bounding box already establish elsewhere in this codebase; upgradeable
/// to real polygon/SAT overlap later if a game actually needs pixel-tight
/// collision.
pub fn touching(artboard: &Artboard, a: NodeId, b: NodeId) -> bool {
    let Some(box_a) = world_aabb(artboard, a) else { return false };
    let Some(box_b) = world_aabb(artboard, b) else { return false };
    box_a.0.x <= box_b.1.x && box_a.1.x >= box_b.0.x && box_a.0.y <= box_b.1.y && box_a.1.y >= box_b.0.y
}

fn world_aabb(artboard: &Artboard, id: NodeId) -> Option<(Vec2, Vec2)> {
    let node = artboard.node(id)?;
    let (local_min, local_max) = local_bounds(node);
    let affine = artboard.world_transform(id).to_affine2();
    let corners = [
        affine.transform_point2(local_min),
        affine.transform_point2(Vec2::new(local_max.x, local_min.y)),
        affine.transform_point2(Vec2::new(local_min.x, local_max.y)),
        affine.transform_point2(local_max),
    ];
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for corner in corners {
        min = min.min(corner);
        max = max.max(corner);
    }
    Some((min, max))
}

/// `id`'s world-space origin: its anchor point, which is exactly what a
/// composed world `Transform2D::translation` already is (see that field's
/// doc comment). What "distance to"/"point towards" measure from. `None`
/// for a node (or ancestor) that doesn't resolve.
fn world_origin(artboard: &Artboard, id: NodeId) -> Option<Vec2> {
    artboard.try_world_transform(id).map(|t| t.translation)
}

/// The world-space angle (radians, CCW from +x) of `id`'s *parent's* own
/// +x axis, `0.0` for a root node. Subtracted from a desired world facing
/// to get the local `rotation` that produces it, so "point towards" still
/// aims correctly inside a rotated group. Read off the composed affine's
/// x axis rather than summing `rotation`s, so a parent's scale can't skew it.
fn parent_world_angle(artboard: &Artboard, id: NodeId) -> f32 {
    let Some(parent) = artboard.node(id).and_then(|n| n.parent) else { return 0.0 };
    let Some(world) = artboard.try_world_transform(parent) else { return 0.0 };
    let x_axis = world.to_affine2().matrix2.x_axis;
    x_axis.y.atan2(x_axis.x)
}

/// Moves `id` by a *world-space* offset `delta`, converting it into the
/// parent's local space first (via the inverse of the parent's composed
/// linear map) so the node lands exactly `delta` away on screen even when
/// nested inside a scaled/rotated group. A degenerate (zero-scale) parent
/// can't be inverted, so the offset is applied unconverted then: a harmless
/// fallback for an already-invisible node.
fn nudge_world(artboard: &mut Artboard, id: NodeId, delta: Vec2) {
    let parent_linear = artboard.node(id).and_then(|n| n.parent).and_then(|p| artboard.try_world_transform(p)).map(|t| t.to_affine2().matrix2);
    let local_delta = match parent_linear {
        Some(m) if m.determinant().abs() > f32::EPSILON => m.inverse() * delta,
        _ => delta,
    };
    if let Some(node) = artboard.node_mut(id) {
        node.transform.translation += local_delta;
    }
}

/// The artboard's own rectangle in world space: centered on the origin,
/// `width` x `height` (the renderer's world space is origin-centered with
/// +y up; see `an1mbyt3_renderer`'s `build_mvp` doc comment).
fn scene_rect(artboard: &Artboard) -> (Vec2, Vec2) {
    let half = Vec2::new(artboard.width / 2.0, artboard.height / 2.0);
    (-half, half)
}

/// The world-space shift that would bring the box `(min, max)` fully inside
/// `(lo, hi)`: zero on an axis it already fits on, and "center it" on an
/// axis where the box is bigger than the scene (no position fits, so
/// centering is the least-surprising compromise).
fn shift_inside(min: Vec2, max: Vec2, lo: Vec2, hi: Vec2) -> Vec2 {
    let axis = |min: f32, max: f32, lo: f32, hi: f32| {
        if max - min > hi - lo {
            (lo + hi) / 2.0 - (min + max) / 2.0
        } else if min < lo {
            lo - min
        } else if max > hi {
            hi - max
        } else {
            0.0
        }
    };
    Vec2::new(axis(min.x, max.x, lo.x, hi.x), axis(min.y, max.y, lo.y, hi.y))
}

/// `Statement::KeepInsideScene`'s whole effect; see its doc comment.
fn keep_inside_scene(artboard: &mut Artboard, id: NodeId) {
    let Some((min, max)) = world_aabb(artboard, id) else { return };
    let (lo, hi) = scene_rect(artboard);
    let shift = shift_inside(min, max, lo, hi);
    if shift != Vec2::ZERO {
        nudge_world(artboard, id, shift);
    }
}

/// `Statement::BounceOnEdge`'s whole effect: mirrors the node's world
/// facing off whichever edge(s) its box crosses, but only if it's heading
/// *outward* through that edge (so a node sitting on an edge doesn't flip
/// every frame), then fences it back inside with `keep_inside_scene`.
/// Mirroring off a vertical edge (left/right) maps angle `θ` to `π - θ`;
/// off a horizontal edge (top/bottom), to `-θ`.
fn bounce_on_edge(artboard: &mut Artboard, id: NodeId) {
    let Some((min, max)) = world_aabb(artboard, id) else { return };
    let Some(rotation) = artboard.node(id).map(|n| n.transform.rotation) else { return };
    let (lo, hi) = scene_rect(artboard);
    let parent_angle = parent_world_angle(artboard, id);
    let mut world_angle = parent_angle + rotation;
    let dir = Vec2::new(world_angle.cos(), world_angle.sin());
    let mut flipped = false;
    if (min.x < lo.x && dir.x < 0.0) || (max.x > hi.x && dir.x > 0.0) {
        world_angle = std::f32::consts::PI - world_angle;
        flipped = true;
    }
    if (min.y < lo.y && dir.y < 0.0) || (max.y > hi.y && dir.y > 0.0) {
        world_angle = -world_angle;
        flipped = true;
    }
    if flipped {
        if let Some(node) = artboard.node_mut(id) {
            node.transform.rotation = world_angle - parent_angle;
        }
    }
    keep_inside_scene(artboard, id);
}

/// Rotates `id` so its +x axis faces the world point `target`: the shared
/// body of `PointTowardsMouse`/`PointTowards`. A target exactly on the
/// node's own origin leaves rotation untouched (no meaningful direction).
fn point_towards(artboard: &mut Artboard, id: NodeId, target: Vec2) {
    let Some(origin) = world_origin(artboard, id) else { return };
    let to = target - origin;
    if to.length_squared() <= f32::EPSILON {
        return;
    }
    let local = to.y.atan2(to.x) - parent_world_angle(artboard, id);
    if let Some(node) = artboard.node_mut(id) {
        node.transform.rotation = local;
    }
}

/// What running one statement did — `execute` pushes a new `Frame` for
/// compound statements (`If`/`Repeat`/`Forever`/`ForEachNode`) rather than
/// recursing, so the caller's explicit stack (not Rust's own call stack) is
/// what actually grows/shrinks as the script descends/returns — the
/// mechanism that makes this whole interpreter resumable across `step()`
/// calls in the first place.
enum Effect {
    Continue,
    Push(Frame),
    Wait(f32),
    /// `Statement::CreateCloneOf` just spawned a new clone — bubbles up
    /// through `run_script` (which has no visibility into sibling scripts/
    /// pools) to `step`, which does and is what actually spawns the new
    /// `CloneStarted` instance for it. Never blocks the spawning script
    /// itself — execution continues immediately after, same as `Continue`.
    SpawnClone(NodeId),
    /// `Statement::DeleteThisClone` ran — ends this script instance right
    /// now (clears its whole stack, not just the current frame), after
    /// already despawning its bound node, if any. Distinct from a plain
    /// empty stack: an ordinary script finishes by popping its way back to
    /// empty one frame at a time, but a mid-loop delete needs to unwind
    /// every enclosing `Repeat`/`Forever`/`ForEachNode` frame at once rather
    /// than let them keep running against a node that no longer exists.
    Terminate,
}

fn execute(statement: &Statement, ctx: &BlockContext, artboard: &mut Artboard, current_item: Option<NodeId>, bound_node: Option<NodeId>, state: &mut ProgramState) -> Effect {
    let eval_here = |e: &Expr, artboard: &Artboard, state: &mut ProgramState| eval(e, ctx, artboard, current_item, bound_node, state);
    let resolve_here = |node_ref: &NodeRef, artboard: &Artboard, state: &mut ProgramState| resolve_node_ref(node_ref, artboard, current_item, bound_node, ctx, state);
    match statement {
        Statement::GoToXY { node, x, y } => {
            let (x, y) = (eval_here(x, artboard, state), eval_here(y, artboard, state));
            if let Some(target) = resolve_here(node, artboard, state).and_then(|id| artboard.node_mut(id)) {
                target.transform.translation = Vec2::new(x, y);
            }
            Effect::Continue
        }
        Statement::SetProperty { node, property, value } => {
            let value = eval_here(value, artboard, state);
            if let Some(target) = resolve_here(node, artboard, state).and_then(|id| artboard.node_mut(id)) {
                write_property(target, *property, value);
            }
            Effect::Continue
        }
        Statement::SetTextNumber { node, value, decimals } => {
            let value = eval_here(value, artboard, state);
            if let Some(NodeKind::Text(text)) = resolve_here(node, artboard, state).and_then(|id| artboard.node_mut(id)).map(|n| &mut n.kind) {
                text.content = format!("{:.*}", *decimals as usize, value);
            }
            Effect::Continue
        }
        Statement::ChangeProperty { node, property, delta } => {
            let delta = eval_here(delta, artboard, state);
            if let Some(target) = resolve_here(node, artboard, state).and_then(|id| artboard.node_mut(id)) {
                let current = read_property(target, *property);
                write_property(target, *property, current + delta);
            }
            Effect::Continue
        }
        Statement::Wait { seconds } => Effect::Wait(ctx.elapsed_seconds + eval_here(seconds, artboard, state).max(0.0)),
        Statement::If { condition, then_branch, else_branch } => {
            let branch = if is_truthy(eval_here(condition, artboard, state)) { then_branch } else { else_branch };
            Effect::Push(Frame::Sequence { statements: branch.clone(), index: 0 })
        }
        Statement::Repeat { count, body } => {
            let n = eval_here(count, artboard, state).max(0.0) as u32;
            if n == 0 {
                Effect::Continue
            } else {
                Effect::Push(Frame::Loop { body: body.clone(), index: 0, remaining: Some(n) })
            }
        }
        Statement::Forever { body } => Effect::Push(Frame::Loop { body: body.clone(), index: 0, remaining: None }),
        Statement::ForEachNode { list, body } => {
            let items = resolve_list(list, artboard);
            if items.is_empty() {
                Effect::Continue
            } else {
                Effect::Push(Frame::ForEach { items, item_index: 0, body: body.clone(), stmt_index: 0 })
            }
        }
        Statement::CreateCloneOf { node } => match resolve_here(node, artboard, state).and_then(|id| artboard.spawn_clone_of(id)) {
            Some(new_id) => Effect::SpawnClone(new_id),
            None => Effect::Continue,
        },
        Statement::DeleteThisClone => {
            if let Some(id) = bound_node {
                artboard.despawn(id);
            }
            Effect::Terminate
        }
        Statement::MoveSteps { node, steps } => {
            let steps = eval_here(steps, artboard, state);
            if let Some(target) = resolve_here(node, artboard, state).and_then(|id| artboard.node_mut(id)) {
                let rotation = target.transform.rotation;
                target.transform.translation += Vec2::new(rotation.cos(), rotation.sin()) * steps;
            }
            Effect::Continue
        }
        Statement::TurnDegrees { node, degrees } => {
            let radians = eval_here(degrees, artboard, state).to_radians();
            if let Some(target) = resolve_here(node, artboard, state).and_then(|id| artboard.node_mut(id)) {
                target.transform.rotation += radians;
            }
            Effect::Continue
        }
        Statement::PointTowardsMouse { node } => {
            if let Some(id) = resolve_here(node, artboard, state) {
                point_towards(artboard, id, ctx.mouse_artboard);
            }
            Effect::Continue
        }
        Statement::PointTowards { node, target } => {
            let id = resolve_here(node, artboard, state);
            let aim = resolve_here(target, artboard, state).and_then(|t| world_origin(artboard, t));
            if let (Some(id), Some(aim)) = (id, aim) {
                point_towards(artboard, id, aim);
            }
            Effect::Continue
        }
        Statement::GlideTo { node, x, y, seconds } => {
            let to = Vec2::new(eval_here(x, artboard, state), eval_here(y, artboard, state));
            let duration = eval_here(seconds, artboard, state);
            let Some(id) = resolve_here(node, artboard, state) else { return Effect::Continue };
            let Some(from) = artboard.node(id).map(|n| n.transform.translation) else { return Effect::Continue };
            if duration > 0.0 {
                Effect::Push(Frame::Glide { node: id, from, to, start: ctx.elapsed_seconds, duration })
            } else {
                if let Some(target) = artboard.node_mut(id) {
                    target.transform.translation = to;
                }
                Effect::Continue
            }
        }
        Statement::KeepInsideScene { node } => {
            if let Some(id) = resolve_here(node, artboard, state) {
                keep_inside_scene(artboard, id);
            }
            Effect::Continue
        }
        Statement::BounceOnEdge { node } => {
            if let Some(id) = resolve_here(node, artboard, state) {
                bounce_on_edge(artboard, id);
            }
            Effect::Continue
        }
        Statement::SetText { node, text } => {
            if let Some(NodeKind::Text(text_node)) = resolve_here(node, artboard, state).and_then(|id| artboard.node_mut(id)).map(|n| &mut n.kind) {
                text_node.content.clone_from(text);
            }
            Effect::Continue
        }
        Statement::SetVariable { name, value } => {
            let value = eval_here(value, artboard, state);
            state.variables.insert(name.clone(), value);
            Effect::Continue
        }
        Statement::ChangeVariable { name, delta } => {
            let delta = eval_here(delta, artboard, state);
            *state.variables.entry(name.clone()).or_insert(0.0) += delta;
            Effect::Continue
        }
        Statement::SetNumberInput { name, value } => {
            let value = eval_here(value, artboard, state);
            state.input_writes.push(InputWrite::Number(name.clone(), value));
            Effect::Continue
        }
        Statement::SetBoolInput { name, value } => {
            let value = is_truthy(eval_here(value, artboard, state));
            state.input_writes.push(InputWrite::Bool(name.clone(), value));
            Effect::Continue
        }
        Statement::FireTrigger { name } => {
            state.input_writes.push(InputWrite::Trigger(name.clone()));
            Effect::Continue
        }
    }
}

/// Runs one script's stack until it either finishes (stack empties),
/// suspends on a `Wait`, or hits [`MAX_INSTRUCTIONS_PER_STEP`] — see this
/// module's doc comment for why that cap exists and what it does on hit.
/// Any clone(s) spawned along the way are appended to `spawned` — see
/// `Effect::SpawnClone`'s own doc comment for why `run_script` can't just
/// start their `CloneStarted` instance itself.
fn run_script(runtime: &mut ScriptRuntime, ctx: &BlockContext, artboard: &mut Artboard, state: &mut ProgramState, spawned: &mut Vec<NodeId>) {
    if let Some(until) = runtime.waiting_until {
        if ctx.elapsed_seconds < until {
            return;
        }
        runtime.waiting_until = None;
    }

    for _ in 0..MAX_INSTRUCTIONS_PER_STEP {
        let Some(frame) = runtime.stack.last_mut() else { return }; // stack empty — script finished.

        let next_statement = match frame {
            Frame::Sequence { statements, index } => {
                if *index >= statements.len() {
                    runtime.stack.pop();
                    continue;
                }
                let statement = statements[*index].clone();
                *index += 1;
                statement
            }
            Frame::Loop { body, index, remaining } => {
                if *index >= body.len() {
                    match remaining {
                        None => *index = 0, // Forever — loop again.
                        Some(n) => {
                            // Decrement *before* deciding whether to loop
                            // again — this pass through `body` just
                            // consumed one of the `n` allowed iterations,
                            // so reaching exactly `0` here means this was
                            // the last one: pop without resetting `index`
                            // for another pass. Resetting index and
                            // checking "was it 0" only on the *next* pass
                            // (the original, buggy version of this) ran one
                            // extra iteration every time.
                            *n -= 1;
                            if *n == 0 {
                                runtime.stack.pop();
                                continue;
                            }
                            *index = 0;
                        }
                    }
                    continue;
                }
                let statement = body[*index].clone();
                *index += 1;
                statement
            }
            Frame::ForEach { items, item_index, body, stmt_index } => {
                if *item_index >= items.len() {
                    runtime.stack.pop();
                    continue;
                }
                if *stmt_index >= body.len() {
                    *item_index += 1;
                    *stmt_index = 0;
                    continue;
                }
                let statement = body[*stmt_index].clone();
                *stmt_index += 1;
                statement
            }
            // Not a statement list at all: a glide in progress owns the
            // script until it finishes. Place the node for this frame, then
            // either pop (time's up, carry on with the next statement this
            // same frame) or yield until next frame.
            &mut Frame::Glide { node, from, to, start, duration } => {
                let t = ((ctx.elapsed_seconds - start) / duration).clamp(0.0, 1.0);
                if let Some(target) = artboard.node_mut(node) {
                    target.transform.translation = from.lerp(to, t);
                }
                if t >= 1.0 {
                    runtime.stack.pop();
                    continue;
                }
                return;
            }
        };

        // Computed fresh for every statement (not cached across the loop)
        // — a fresh immutable borrow of `runtime.stack`, taken *after* the
        // match above's mutable borrow (`frame`) has already gone out of
        // scope (the match only produced an owned `next_statement` by this
        // point) — see this module's own doc comment for why this has to
        // scan the whole stack rather than just the frame just matched on.
        let current_item = current_foreach_item(&runtime.stack);

        match execute(&next_statement, ctx, artboard, current_item, runtime.bound_node, state) {
            Effect::Continue => {}
            Effect::Push(new_frame) => runtime.stack.push(new_frame),
            Effect::Wait(until) => {
                runtime.waiting_until = Some(until);
                return;
            }
            Effect::SpawnClone(id) => spawned.push(id),
            Effect::Terminate => {
                runtime.stack.clear();
                return;
            }
        }
    }
}

/// Steps every script in `program` once (one frame) against `runtime`,
/// writing straight into `artboard` — called from `StateMachine`'s own
/// per-frame entry point for whichever state (if any) is currently a
/// Blocks program, the same place/gate `Layer::run_program` already runs
/// `Graph` mode from.
pub fn step(program: &BlockProgramDef, runtime: &mut BlockRuntime, ctx: &BlockContext, artboard: &mut Artboard) {
    if runtime.scripts.len() != program.scripts.len() {
        // Author edited the program (added/removed a script) while it was
        // live — resync rather than index out of bounds; a freshly-added
        // script starts idle (or immediately, if `StateTriggered`) same as
        // `BlockRuntime::fresh_for`, an edited-away one's runtime is simply
        // dropped. (This does reseed the RNG — an acceptable side effect of
        // an edit-while-running resync, not something worth preserving
        // `rng_state` across specially for.)
        //
        // Variables and any not-yet-drained input writes are carried over
        // the same way (an edit shouldn't silently zero a score mid-play-test).
        let state = std::mem::take(&mut runtime.state);
        let next_tick_at = runtime.next_tick_at;
        *runtime = BlockRuntime::fresh_for(program);
        runtime.state = state;
        runtime.next_tick_at = next_tick_at;
    }

    if let Some(rate) = program.ticks_per_second {
        if rate > 0.0 {
            if ctx.elapsed_seconds < runtime.next_tick_at {
                return;
            }
            runtime.next_tick_at = ctx.elapsed_seconds + 1.0 / rate;
        }
    }

    let mut spawned: Vec<NodeId> = Vec::new();

    for (script, pool) in program.scripts.iter().zip(runtime.scripts.iter_mut()) {
        match &script.event {
            EventKind::StateTriggered => {}
            // Non-overlapping: only (re)trigger while the pool's empty (its
            // one previous instance, if any, has already run to completion
            // and been pruned below) — the original "ignore a retrigger
            // while still busy" behavior. Overlapping: always spawn another
            // instance on the edge, regardless of the pool's current
            // contents — real Scratch's own behavior, see `overlapping`'s
            // own doc comment.
            EventKind::KeyPressed { key, overlapping } => {
                if ctx.keys.just_pressed(key) && (*overlapping || pool.is_empty()) {
                    pool.push(ScriptRuntime::started(script.statements.clone()));
                }
            }
            // Level-triggered, not edge-triggered: starts the instant the
            // key goes down (if nothing's already running), and the pool is
            // unconditionally cleared the instant it's *not* held —
            // discarding any in-progress run (even mid-`Wait`) rather than
            // letting it finish, unlike `KeyPressed`. See `KeyHeld`'s own
            // doc comment for why a finished-while-still-held run restarts
            // on its own (pruned to empty below, then this same "start if
            // empty" branch fires again next frame).
            EventKind::KeyHeld { key } => {
                if !ctx.keys.is_down(key) {
                    pool.clear();
                } else if pool.is_empty() {
                    pool.push(ScriptRuntime::started(script.statements.clone()));
                }
            }
            // Never self-triggers here — a `CloneStarted` instance is only
            // ever created by the clone-spawning pass below, for a specific
            // brand-new clone. Nothing about the state becoming active (or
            // any key edge) starts one on its own.
            EventKind::CloneStarted => {}
        }

        for script_runtime in pool.iter_mut() {
            run_script(script_runtime, ctx, artboard, &mut runtime.state, &mut spawned);
        }
        pool.retain(|script_runtime| !script_runtime.idle());
    }

    // Every clone spawned by *any* script this frame (`Statement::CreateCloneOf`,
    // bubbled up via `Effect::SpawnClone` — see that variant's own doc
    // comment for why `run_script` can't just start this instance itself)
    // gets its own fresh `CloneStarted` instance here, once per such script
    // in the program — mirrors real Scratch: every "when I start as a
    // clone" hat runs for every newly created clone, not just one.
    for new_clone in spawned {
        for (script, pool) in program.scripts.iter().zip(runtime.scripts.iter_mut()) {
            if matches!(script.event, EventKind::CloneStarted) {
                pool.push(ScriptRuntime::started_for_clone(script.statements.clone(), new_clone));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use an1mbyt3_core::{BlockScript, GraphProperty, Node, NodeKind, Path, ShapeNode};

    fn square(artboard: &mut Artboard, x: f32) -> NodeId {
        let node = artboard.add_node(Node::new(
            "Square",
            None,
            NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None }),
        ));
        artboard.node_mut(node).unwrap().transform.translation.x = x;
        node
    }

    fn ctx<'a>(keys: &'a KeyboardState, inputs: &'a Inputs, elapsed_seconds: f32) -> BlockContext<'a> {
        BlockContext { elapsed_seconds, mouse_artboard: Vec2::ZERO, keys, inputs }
    }

    #[test]
    fn touching_is_true_for_overlapping_boxes_and_false_for_separate_ones() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let a = square(&mut artboard, 0.0);
        let b = square(&mut artboard, 5.0); // 10x10 boxes centered at 0 and 5 — overlap
        let c = square(&mut artboard, 100.0); // far away — no overlap

        assert!(touching(&artboard, a, b));
        assert!(!touching(&artboard, a, c));
    }

    #[test]
    fn touching_class_is_true_against_any_node_with_the_class_and_false_against_itself() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let player = square(&mut artboard, 0.0);
        let far_enemy = square(&mut artboard, 100.0); // far away — no overlap regardless of class
        let near_enemy = square(&mut artboard, 5.0); // overlaps `player`
        artboard.node_mut(far_enemy).unwrap().classes.push("enemy".to_string());
        artboard.node_mut(near_enemy).unwrap().classes.push("enemy".to_string());
        artboard.node_mut(player).unwrap().classes.push("enemy".to_string()); // player itself also tagged — must not count as touching itself

        let keys = KeyboardState::default();
        let inputs = Inputs::default();
        let c = ctx(&keys, &inputs, 0.0);
        let mut rng = ProgramState::default();

        let expr = Expr::TouchingClass { node: NodeRef::Literal(player), class: "enemy".to_string() };
        assert_eq!(eval(&expr, &c, &artboard, None, None, &mut rng), 1.0, "should be touching near_enemy");

        // Remove the near one — only the far, non-overlapping enemy (and
        // itself) remain tagged.
        artboard.node_mut(near_enemy).unwrap().classes.clear();
        assert_eq!(eval(&expr, &c, &artboard, None, None, &mut rng), 0.0, "no remaining enemy actually overlaps, and self-touching never counts");
    }

    #[test]
    fn state_triggered_script_runs_once_immediately_on_first_step() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = square(&mut artboard, 0.0);
        let program = BlockProgramDef {
            ticks_per_second: None,
            scripts: vec![BlockScript { position: None,
                event: EventKind::StateTriggered,
                statements: vec![Statement::SetProperty { node: NodeRef::Literal(node), property: GraphProperty::TranslationX, value: Expr::Number(42.0) }],
            }],
        };
        let mut runtime = BlockRuntime::fresh_for(&program);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();

        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);

        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 42.0);
    }

    #[test]
    fn forever_with_wait_advances_once_per_second_across_multiple_steps() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = square(&mut artboard, 0.0);
        let program = BlockProgramDef {
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
        };
        let mut runtime = BlockRuntime::fresh_for(&program);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();

        // t=0: StateTriggered already queued the Forever's first pass — first
        // ChangeProperty runs immediately, then blocks on Wait until t=1.
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 1.0);

        // Not yet 1 second — must stay suspended, no second increment.
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.5), &mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 1.0);

        // Past the 1-second mark — resumes, loops, increments again, waits again.
        step(&program, &mut runtime, &ctx(&keys, &inputs, 1.2), &mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 2.0);
    }

    #[test]
    fn ticks_per_second_throttles_execution_independent_of_the_render_frame_rate() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = square(&mut artboard, 0.0);
        // A `Wait`-less `Forever` — every `step()` call that's actually
        // allowed to run will exhaust the whole `MAX_INSTRUCTIONS_PER_STEP`
        // budget in one go (there's nothing to suspend it early), so the
        // exact resulting `x` after an allowed tick is deterministic and
        // large — the point isn't the magnitude, it's whether a *second*
        // `step()` call within the same throttle window changes it at all.
        let program = BlockProgramDef {
            ticks_per_second: Some(1.0),
            scripts: vec![BlockScript { position: None,
                event: EventKind::StateTriggered,
                statements: vec![Statement::Forever { body: vec![Statement::ChangeProperty { node: NodeRef::Literal(node), property: GraphProperty::TranslationX, delta: Expr::Number(1.0) }] }],
            }],
        };
        let mut runtime = BlockRuntime::fresh_for(&program);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();

        // t=0.0: `next_tick_at` starts at 0.0, so this first call is allowed
        // — runs the per-step instruction budget (the exact resulting value
        // isn't this test's concern — a 1-statement `Forever` body spends
        // half its budget re-checking the loop's own wraparound each pass —
        // only that it's clearly nonzero, i.e. the tick actually ran).
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);
        let after_first_tick = artboard.node(node).unwrap().transform.translation.x;
        assert!(after_first_tick > 0.0, "the first, un-throttled tick must have run the script");

        // t=0.3: still inside the same 1-tick-per-second window (next tick
        // isn't due until t=1.0) — must be a total no-op, not just "skip
        // new scripts" — the throttle gates `step` before anything else runs.
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.3), &mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, after_first_tick, "still inside the throttle window — must not advance");

        // t=1.5: past the window — allowed again, runs another full budget.
        step(&program, &mut runtime, &ctx(&keys, &inputs, 1.5), &mut artboard);
        assert!(
            artboard.node(node).unwrap().transform.translation.x > after_first_tick,
            "past the throttle window — the second allowed tick must have advanced the script again"
        );
    }

    #[test]
    fn repeat_runs_its_body_exactly_n_times_in_one_step_when_nothing_waits() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = square(&mut artboard, 0.0);
        let program = BlockProgramDef {
            ticks_per_second: None,
            scripts: vec![BlockScript { position: None,
                event: EventKind::StateTriggered,
                statements: vec![Statement::Repeat {
                    count: Expr::Number(5.0),
                    body: vec![Statement::ChangeProperty { node: NodeRef::Literal(node), property: GraphProperty::TranslationX, delta: Expr::Number(2.0) }],
                }],
            }],
        };
        let mut runtime = BlockRuntime::fresh_for(&program);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();

        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);

        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 10.0, "5 iterations * 2.0 each, all within one step()");
    }

    #[test]
    fn key_pressed_repeat_with_wait_finishes_and_retriggers_on_a_second_press() {
        // Reproduces an author's exact reported script: "when key [space]
        // pressed" -> "repeat 10 [change x by 2; wait 0.01 seconds]" — a
        // `Repeat`'s `Some(n)` countdown interacting with a `Wait`
        // suspension *inside* its own body, which none of this module's
        // other tests exercise together (`forever_with_wait_...` covers
        // `Wait` inside a loop, but only `Forever`'s `None` countdown;
        // `repeat_runs_its_body_...` covers `Repeat`, but with no `Wait` to
        // suspend across steps).
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = square(&mut artboard, 0.0);
        let program = BlockProgramDef {
            ticks_per_second: None,
            scripts: vec![BlockScript { position: None,
                event: EventKind::KeyPressed { key: "space".into(), overlapping: false },
                statements: vec![Statement::Repeat {
                    count: Expr::Number(10.0),
                    body: vec![
                        Statement::ChangeProperty { node: NodeRef::Literal(node), property: GraphProperty::TranslationX, delta: Expr::Number(2.0) },
                        Statement::Wait { seconds: Expr::Number(0.01) },
                    ],
                }],
            }],
        };
        let mut runtime = BlockRuntime::fresh_for(&program);
        let inputs = Inputs::default();

        let mut keys = KeyboardState::default();
        keys.set_down("space", true);
        let mut t = 0.0f32;
        // Down-edge — starts the script. Drive it forward in small steps
        // (smaller than the 0.01s wait) the way a real ~60fps render loop
        // would, holding the key down throughout (no new edges after the
        // first) so the *only* thing making further progress possible is
        // the interpreter correctly resuming past each `Wait`.
        for _ in 0..50 {
            step(&program, &mut runtime, &ctx(&keys, &inputs, t), &mut artboard);
            keys.end_frame();
            t += 0.005;
        }
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 20.0, "all 10 iterations must have completed (10 * 2.0)");

        // Release, then press again — must retrigger from scratch, proving
        // the script actually went idle (stack fully popped) rather than
        // getting stuck non-idle after its last `Wait`.
        keys.set_down("space", false);
        keys.end_frame();
        keys.set_down("space", true);
        for _ in 0..50 {
            step(&program, &mut runtime, &ctx(&keys, &inputs, t), &mut artboard);
            keys.end_frame();
            t += 0.005;
        }
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 40.0, "a second press must run all 10 iterations again (20.0 + 20.0)");
    }

    #[test]
    fn overlapping_key_pressed_spawns_a_new_instance_on_every_press_instead_of_ignoring_a_retrigger() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = square(&mut artboard, 0.0);
        // A `Wait` in the body so the first press's instance is still
        // in-flight (not yet idle/pruned) when the second press arrives —
        // the only way to actually distinguish "spawned a second instance
        // alongside the first" from "ignored because still busy."
        let program = BlockProgramDef {
            ticks_per_second: None,
            scripts: vec![BlockScript { position: None,
                event: EventKind::KeyPressed { key: "space".into(), overlapping: true },
                statements: vec![
                    Statement::ChangeProperty { node: NodeRef::Literal(node), property: GraphProperty::TranslationX, delta: Expr::Number(1.0) },
                    Statement::Wait { seconds: Expr::Number(1.0) },
                    Statement::ChangeProperty { node: NodeRef::Literal(node), property: GraphProperty::TranslationX, delta: Expr::Number(1.0) },
                ],
            }],
        };
        let mut runtime = BlockRuntime::fresh_for(&program);
        let inputs = Inputs::default();
        let mut keys = KeyboardState::default();

        keys.set_down("space", true);
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 1.0, "first press: first change ran, now waiting");

        // Release and press again *before* the first instance's Wait has
        // resolved — a non-overlapping script would ignore this (see
        // `key_pressed_repeat_with_wait_...`); an overlapping one must spawn
        // a second, independent instance right alongside the first.
        keys.end_frame();
        keys.set_down("space", false);
        keys.end_frame();
        keys.set_down("space", true);
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.1), &mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 2.0, "second press: a *new* instance's first change also ran, on top of the first instance still waiting");

        // Past 1 second — both instances' Waits resolve, each running its
        // own trailing change.
        keys.end_frame();
        step(&program, &mut runtime, &ctx(&keys, &inputs, 1.2), &mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 4.0, "both instances' trailing changes ran (2.0 + 1.0 + 1.0)");
    }

    #[test]
    fn key_held_runs_while_down_restarts_on_early_finish_and_aborts_mid_wait_on_release() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = square(&mut artboard, 0.0);
        // No `Forever` — `KeyHeld`'s own restart-on-finish behavior is what
        // must make this repeat for as long as the key stays down.
        let program = BlockProgramDef {
            ticks_per_second: None,
            scripts: vec![BlockScript { position: None,
                event: EventKind::KeyHeld { key: "space".into() },
                statements: vec![
                    Statement::ChangeProperty { node: NodeRef::Literal(node), property: GraphProperty::TranslationX, delta: Expr::Number(1.0) },
                    Statement::Wait { seconds: Expr::Number(1.0) },
                ],
            }],
        };
        let mut runtime = BlockRuntime::fresh_for(&program);
        let inputs = Inputs::default();
        let mut keys = KeyboardState::default();
        keys.set_down("space", true);

        // Held for 2.5s straight — the body (change, then wait 1s) should
        // have restarted itself twice on its own (no Forever authored),
        // landing a third change partway into the third cycle.
        let mut t = 0.0f32;
        while t <= 2.5 {
            step(&program, &mut runtime, &ctx(&keys, &inputs, t), &mut artboard);
            keys.end_frame();
            t += 0.1;
        }
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 3.0, "held continuously: change ran 3 times (t=0, ~1.0, ~2.0) with no Forever authored");

        // Release mid-`Wait` (the 3rd cycle is currently waiting) — must
        // abort immediately, not let it finish first.
        keys.set_down("space", false);
        keys.end_frame();
        step(&program, &mut runtime, &ctx(&keys, &inputs, t + 5.0), &mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 3.0, "released mid-wait — the in-flight instance must be discarded, not resumed");

        // Press again — starts fresh from the top, proving the abort really
        // cleared it rather than leaving it stuck non-idle.
        keys.set_down("space", true);
        step(&program, &mut runtime, &ctx(&keys, &inputs, t + 5.1), &mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 4.0, "a fresh press starts the body again from the top");
    }

    #[test]
    fn if_else_picks_the_branch_matching_the_condition() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let a = square(&mut artboard, 0.0);
        let b = square(&mut artboard, 5.0); // overlapping `a`
        let target = square(&mut artboard, 0.0);
        let program = BlockProgramDef {
            ticks_per_second: None,
            scripts: vec![BlockScript { position: None,
                event: EventKind::StateTriggered,
                statements: vec![Statement::If {
                    condition: Expr::Touching { a: NodeRef::Literal(a), b: NodeRef::Literal(b) },
                    then_branch: vec![Statement::SetProperty { node: NodeRef::Literal(target), property: GraphProperty::TranslationY, value: Expr::Number(1.0) }],
                    else_branch: vec![Statement::SetProperty { node: NodeRef::Literal(target), property: GraphProperty::TranslationY, value: Expr::Number(-1.0) }],
                }],
            }],
        };
        let mut runtime = BlockRuntime::fresh_for(&program);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();

        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);

        assert_eq!(artboard.node(target).unwrap().transform.translation.y, 1.0, "a and b overlap, so the then-branch should have run");
    }

    #[test]
    fn key_pressed_script_starts_only_on_the_down_edge_not_while_held() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = square(&mut artboard, 0.0);
        let program = BlockProgramDef {
            ticks_per_second: None,
            scripts: vec![BlockScript { position: None,
                event: EventKind::KeyPressed { key: "space".into(), overlapping: false },
                statements: vec![Statement::ChangeProperty { node: NodeRef::Literal(node), property: GraphProperty::TranslationX, delta: Expr::Number(1.0) }],
            }],
        };
        let mut runtime = BlockRuntime::fresh_for(&program);
        let inputs = Inputs::default();

        let mut keys = KeyboardState::default();
        keys.set_down("space", true);
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 1.0, "down-edge starts the script");

        keys.end_frame();
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.1), &mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 1.0, "still held (no new edge) must not re-trigger");

        keys.set_down("space", false);
        keys.end_frame();
        keys.set_down("space", true);
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.2), &mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 2.0, "release then press again is a new edge");
    }

    #[test]
    fn for_each_node_runs_body_once_per_matching_node_binding_current_item() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let a = square(&mut artboard, 0.0);
        let b = square(&mut artboard, 0.0);
        let c = square(&mut artboard, 0.0); // no "enemy" class — must be skipped
        artboard.node_mut(a).unwrap().classes.push("enemy".into());
        artboard.node_mut(b).unwrap().classes.push("enemy".into());

        let program = BlockProgramDef {
            ticks_per_second: None,
            scripts: vec![BlockScript { position: None,
                event: EventKind::StateTriggered,
                statements: vec![Statement::ForEachNode {
                    list: ListExpr::NodesWithClass { class: "enemy".into() },
                    body: vec![Statement::SetProperty { node: NodeRef::CurrentItem, property: GraphProperty::TranslationY, value: Expr::Number(9.0) }],
                }],
            }],
        };
        let mut runtime = BlockRuntime::fresh_for(&program);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();

        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);

        assert_eq!(artboard.node(a).unwrap().transform.translation.y, 9.0);
        assert_eq!(artboard.node(b).unwrap().transform.translation.y, 9.0);
        assert_eq!(artboard.node(c).unwrap().transform.translation.y, 0.0, "not in the class — must be untouched");
    }

    #[test]
    fn for_each_node_current_item_resolves_correctly_through_a_nested_if() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let a = square(&mut artboard, 0.0);
        artboard.node_mut(a).unwrap().classes.push("enemy".into());

        let program = BlockProgramDef {
            ticks_per_second: None,
            scripts: vec![BlockScript { position: None,
                event: EventKind::StateTriggered,
                statements: vec![Statement::ForEachNode {
                    list: ListExpr::NodesWithClass { class: "enemy".into() },
                    // The `If` frame sits *above* the `ForEach` frame on the
                    // stack while this branch runs — proves `CurrentItem`
                    // still resolves to the loop's node through it.
                    body: vec![Statement::If {
                        condition: Expr::Number(1.0),
                        then_branch: vec![Statement::SetProperty { node: NodeRef::CurrentItem, property: GraphProperty::TranslationX, value: Expr::Number(7.0) }],
                        else_branch: vec![],
                    }],
                }],
            }],
        };
        let mut runtime = BlockRuntime::fresh_for(&program);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();

        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);

        assert_eq!(artboard.node(a).unwrap().transform.translation.x, 7.0);
    }

    #[test]
    fn random_int_stays_within_the_inclusive_range_regardless_of_authoring_order() {
        let artboard = Artboard::new("Main", 400.0, 400.0);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();
        let mut rng = ProgramState { rng: 12345, ..ProgramState::default() };
        for i in 0..200 {
            let c = ctx(&keys, &inputs, i as f32 * 0.01);
            let v = eval(&Expr::RandomInt { min: Box::new(Expr::Number(3.0)), max: Box::new(Expr::Number(7.0)) }, &c, &artboard, None, None, &mut rng);
            assert!((3.0..=7.0).contains(&v) && v.fract() == 0.0, "expected a whole number in 3..=7, got {v}");

            // Authored backwards (max < min) — must still land in range.
            let v = eval(&Expr::RandomInt { min: Box::new(Expr::Number(7.0)), max: Box::new(Expr::Number(3.0)) }, &c, &artboard, None, None, &mut rng);
            assert!((3.0..=7.0).contains(&v) && v.fract() == 0.0, "expected a whole number in 3..=7 even authored backwards, got {v}");
        }
    }

    #[test]
    fn math_fn_and_pi_compute_the_expected_values() {
        let artboard = Artboard::new("Main", 400.0, 400.0);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();
        let c = ctx(&keys, &inputs, 0.0);
        let mut rng = ProgramState::default();

        let pi = eval(&Expr::Pi, &c, &artboard, None, None, &mut rng);
        assert!((pi - std::f32::consts::PI).abs() < 1e-6);

        let sin_half_pi = eval(&Expr::MathFn { op: MathFn::Sin, value: Box::new(Expr::Pi) }, &c, &artboard, None, None, &mut rng);
        // sin(pi) ~ 0, not a very interesting check on its own, so check
        // sin(pi/2) instead via Divide.
        let sin_pi_over_2 = eval(
            &Expr::MathFn { op: MathFn::Sin, value: Box::new(Expr::Divide(Box::new(Expr::Pi), Box::new(Expr::Number(2.0)))) },
            &c,
            &artboard,
            None,
            None,
            &mut rng,
        );
        assert!(sin_half_pi.abs() < 1e-3);
        assert!((sin_pi_over_2 - 1.0).abs() < 1e-3);

        let neg_abs = eval(&Expr::MathFn { op: MathFn::Abs, value: Box::new(Expr::Number(-4.5)) }, &c, &artboard, None, None, &mut rng);
        assert_eq!(neg_abs, 4.5);
    }

    #[test]
    fn create_clone_of_spawns_a_node_and_runs_when_i_start_as_a_clone_with_myself_bound_to_it() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let proto = square(&mut artboard, 0.0);
        let program = BlockProgramDef {
            ticks_per_second: None,
            scripts: vec![
                BlockScript { position: None, event: EventKind::StateTriggered, statements: vec![Statement::CreateCloneOf { node: NodeRef::Literal(proto) }] },
                BlockScript {
                    position: None,
                    event: EventKind::CloneStarted,
                    statements: vec![Statement::SetProperty { node: NodeRef::Myself, property: GraphProperty::TranslationX, value: Expr::Number(99.0) }],
                },
            ],
        };
        let mut runtime = BlockRuntime::fresh_for(&program);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();

        // Step 1: `StateTriggered` runs, spawning the clone — its own
        // `CloneStarted` instance is queued but doesn't run until the next
        // `step()` (see `step`'s own clone-spawning pass, which runs after
        // this frame's main script loop, not inside it).
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);
        let clone_id = artboard.pickable_nodes().map(|(id, _)| id).find(|&id| id != proto).expect("a clone should have been spawned");
        assert_ne!(artboard.node(clone_id).unwrap().transform.translation.x, 99.0, "CloneStarted shouldn't have run yet this same step");

        // Step 2: the `CloneStarted` instance actually runs now, writing to
        // the clone via `NodeRef::Myself` — not the prototype.
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.1), &mut artboard);
        assert_eq!(artboard.node(clone_id).unwrap().transform.translation.x, 99.0);
        assert_eq!(artboard.node(proto).unwrap().transform.translation.x, 0.0, "the prototype itself must be untouched");
    }

    #[test]
    fn delete_this_clone_removes_the_bound_node_and_ends_the_script() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let proto = square(&mut artboard, 0.0);
        let program = BlockProgramDef {
            ticks_per_second: None,
            scripts: vec![
                BlockScript { position: None, event: EventKind::StateTriggered, statements: vec![Statement::CreateCloneOf { node: NodeRef::Literal(proto) }] },
                BlockScript { position: None, event: EventKind::CloneStarted, statements: vec![Statement::DeleteThisClone] },
            ],
        };
        let mut runtime = BlockRuntime::fresh_for(&program);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();

        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);
        let clone_id = artboard.pickable_nodes().map(|(id, _)| id).find(|&id| id != proto).expect("a clone should have been spawned");

        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.1), &mut artboard);
        assert!(artboard.node(clone_id).is_none(), "the clone should have deleted itself");
        assert!(artboard.node(proto).is_some(), "the prototype itself must survive");
    }

    // ---- Motion / looks / sensing / variables / input blocks ----

    /// A single-`StateTriggered`-script program running `statements` —
    /// shared setup for the tests below, which only care about what one
    /// straight-line script does.
    fn one_script(statements: Vec<Statement>) -> BlockProgramDef {
        BlockProgramDef { ticks_per_second: None, scripts: vec![BlockScript { position: None, event: EventKind::StateTriggered, statements }] }
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn move_steps_follows_rotation_and_turn_degrees_adds_radians() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = square(&mut artboard, 0.0);
        let program = one_script(vec![
            // Face +y (90° CCW), then move 10: should end at (0, 10).
            Statement::TurnDegrees { node: NodeRef::Literal(node), degrees: Expr::Number(90.0) },
            Statement::MoveSteps { node: NodeRef::Literal(node), steps: Expr::Number(10.0) },
        ]);
        let mut runtime = BlockRuntime::fresh_for(&program);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);

        let t = artboard.node(node).unwrap().transform;
        assert!(approx(t.rotation, std::f32::consts::FRAC_PI_2), "rotation {}", t.rotation);
        assert!(approx(t.translation.x, 0.0) && approx(t.translation.y, 10.0), "translation {:?}", t.translation);
    }

    #[test]
    fn point_towards_mouse_and_node_aim_the_plus_x_axis() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = square(&mut artboard, 0.0);
        let target = square(&mut artboard, 0.0);
        artboard.node_mut(target).unwrap().transform.translation = Vec2::new(0.0, -50.0);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();

        let program = one_script(vec![Statement::PointTowardsMouse { node: NodeRef::Literal(node) }]);
        let mut runtime = BlockRuntime::fresh_for(&program);
        let mut c = ctx(&keys, &inputs, 0.0);
        c.mouse_artboard = Vec2::new(-30.0, 0.0); // straight left
        step(&program, &mut runtime, &c, &mut artboard);
        assert!(approx(artboard.node(node).unwrap().transform.rotation.abs(), std::f32::consts::PI), "facing left");

        let program = one_script(vec![Statement::PointTowards { node: NodeRef::Literal(node), target: NodeRef::Literal(target) }]);
        let mut runtime = BlockRuntime::fresh_for(&program);
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);
        assert!(approx(artboard.node(node).unwrap().transform.rotation, -std::f32::consts::FRAC_PI_2), "facing straight down at the target");
    }

    #[test]
    fn glide_to_interpolates_across_frames_and_then_continues_the_script() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = square(&mut artboard, 0.0);
        let program = one_script(vec![
            Statement::GlideTo { node: NodeRef::Literal(node), x: Expr::Number(100.0), y: Expr::Number(50.0), seconds: Expr::Number(2.0) },
            // Only runs once the glide has finished.
            Statement::SetProperty { node: NodeRef::Literal(node), property: GraphProperty::Rotation, value: Expr::Number(1.0) },
        ]);
        let mut runtime = BlockRuntime::fresh_for(&program);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();

        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);
        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::ZERO, "t=0: still at the start");

        step(&program, &mut runtime, &ctx(&keys, &inputs, 1.0), &mut artboard);
        let halfway = artboard.node(node).unwrap().transform;
        assert!(approx(halfway.translation.x, 50.0) && approx(halfway.translation.y, 25.0), "t=1 of 2: halfway, got {:?}", halfway.translation);
        assert_eq!(halfway.rotation, 0.0, "the statement after the glide must not have run yet");

        step(&program, &mut runtime, &ctx(&keys, &inputs, 2.5), &mut artboard);
        let done = artboard.node(node).unwrap().transform;
        assert_eq!(done.translation, Vec2::new(100.0, 50.0), "past the end: lands exactly on the target");
        assert_eq!(done.rotation, 1.0, "the script carries on in the same frame the glide finishes");
    }

    #[test]
    fn keep_inside_scene_clamps_the_bounding_box_to_the_artboard() {
        // 400x400 artboard spans -200..200 on both axes; the square is 10x10.
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = square(&mut artboard, 500.0);
        artboard.node_mut(node).unwrap().transform.translation.y = -999.0;
        let program = one_script(vec![Statement::KeepInsideScene { node: NodeRef::Literal(node) }]);
        let mut runtime = BlockRuntime::fresh_for(&program);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);

        let t = artboard.node(node).unwrap().transform.translation;
        assert!(approx(t.x, 195.0) && approx(t.y, -195.0), "box edge flush with the scene edge, got {t:?}");
    }

    #[test]
    fn bounce_on_edge_mirrors_an_outward_heading_and_pushes_back_inside() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = square(&mut artboard, 198.0); // poking past the right edge (x max = 203)
        artboard.node_mut(node).unwrap().transform.rotation = 0.0; // heading right, i.e. outward
        let program = one_script(vec![Statement::BounceOnEdge { node: NodeRef::Literal(node) }]);
        let mut runtime = BlockRuntime::fresh_for(&program);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);

        let t = artboard.node(node).unwrap().transform;
        assert!(approx(t.rotation, std::f32::consts::PI), "now heading left, got {}", t.rotation);
        assert!(approx(t.translation.x, 195.0), "pushed back inside, got {}", t.translation.x);

        // Already heading inward: a second bounce on the same edge must
        // not flip it back outward.
        artboard.node_mut(node).unwrap().transform.translation.x = 198.0;
        let mut runtime = BlockRuntime::fresh_for(&program);
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);
        assert!(approx(artboard.node(node).unwrap().transform.rotation, std::f32::consts::PI), "inward heading is left alone");
    }

    #[test]
    fn set_text_replaces_a_text_nodes_content() {
        use an1mbyt3_core::{FontId, TextNode};
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let label = artboard.add_node(Node::new(
            "Label",
            None,
            NodeKind::Text(TextNode { content: "old".into(), font: FontId::default(), size_px: 16.0, fill_color: [1.0; 4], gradient: None }),
        ));
        let program = one_script(vec![Statement::SetText { node: NodeRef::Literal(label), text: "Game Over".into() }]);
        let mut runtime = BlockRuntime::fresh_for(&program);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);

        let NodeKind::Text(text) = &artboard.node(label).unwrap().kind else { panic!("still a text node") };
        assert_eq!(text.content, "Game Over");
    }

    #[test]
    fn variables_set_change_and_read_back_and_reset_on_a_fresh_runtime() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = square(&mut artboard, 0.0);
        let program = one_script(vec![
            Statement::SetVariable { name: "score".into(), value: Expr::Number(10.0) },
            Statement::ChangeVariable { name: "score".into(), delta: Expr::Number(5.0) },
            Statement::ChangeVariable { name: "never_set".into(), delta: Expr::Number(2.0) },
            Statement::SetProperty {
                node: NodeRef::Literal(node),
                property: GraphProperty::TranslationX,
                value: Expr::Add(Box::new(Expr::Variable { name: "score".into() }), Box::new(Expr::Variable { name: "never_set".into() })),
            },
        ]);
        let mut runtime = BlockRuntime::fresh_for(&program);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);

        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 17.0, "15 (set 10, +5) + 2 (unset starts at 0)");
        assert_eq!(runtime.state.variables.get("score"), Some(&15.0));
        assert!(BlockRuntime::fresh_for(&program).state.variables.is_empty(), "a state re-entry starts with no variables");
    }

    #[test]
    fn input_writes_are_queued_readable_in_frame_and_drained_in_order() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = square(&mut artboard, 0.0);
        let program = one_script(vec![
            Statement::SetNumberInput { name: "speed".into(), value: Expr::Number(3.0) },
            Statement::SetBoolInput { name: "jumping".into(), value: Expr::Number(1.0) },
            Statement::FireTrigger { name: "hit".into() },
            // Read-your-own-write within the same frame.
            Statement::SetProperty { node: NodeRef::Literal(node), property: GraphProperty::TranslationX, value: Expr::GetNumberInput { name: "speed".into() } },
            Statement::SetProperty { node: NodeRef::Literal(node), property: GraphProperty::TranslationY, value: Expr::GetBoolInput { name: "jumping".into() } },
        ]);
        let mut runtime = BlockRuntime::fresh_for(&program);
        let keys = KeyboardState::default();
        let inputs = Inputs::default();
        step(&program, &mut runtime, &ctx(&keys, &inputs, 0.0), &mut artboard);

        assert_eq!(artboard.node(node).unwrap().transform.translation, Vec2::new(3.0, 1.0));
        let writes = runtime.take_input_writes();
        assert_eq!(writes, vec![InputWrite::Number("speed".into(), 3.0), InputWrite::Bool("jumping".into(), true), InputWrite::Trigger("hit".into())]);
        assert!(runtime.take_input_writes().is_empty(), "draining empties the queue");

        let mut applied = Inputs::default();
        for write in writes {
            write.apply_to(&mut applied);
        }
        assert_eq!(applied.number("speed"), 3.0);
        assert!(applied.bool("jumping"));
        assert!(applied.trigger_fired("hit"));
    }

    #[test]
    fn distance_timer_min_and_max_reporters() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let a = square(&mut artboard, 0.0);
        let b = square(&mut artboard, 3.0);
        artboard.node_mut(b).unwrap().transform.translation.y = 4.0;
        let keys = KeyboardState::default();
        let inputs = Inputs::default();
        let mut c = ctx(&keys, &inputs, 2.5);
        c.mouse_artboard = Vec2::new(0.0, -6.0);
        let mut state = ProgramState::default();
        let mut ev = |e: Expr| eval(&e, &c, &artboard, None, None, &mut state);

        assert!(approx(ev(Expr::DistanceTo { a: NodeRef::Literal(a), b: NodeRef::Literal(b) }), 5.0), "3-4-5 triangle");
        assert!(approx(ev(Expr::DistanceToMouse { node: NodeRef::Literal(a) }), 6.0));
        assert_eq!(ev(Expr::DistanceToMouse { node: NodeRef::CurrentItem }), 0.0, "unresolvable node degrades to 0");
        assert_eq!(ev(Expr::Timer), 2.5);
        assert_eq!(ev(Expr::Min(Box::new(Expr::Number(4.0)), Box::new(Expr::Number(-2.0)))), -2.0);
        assert_eq!(ev(Expr::Max(Box::new(Expr::Number(4.0)), Box::new(Expr::Number(-2.0)))), 4.0);
    }
}
