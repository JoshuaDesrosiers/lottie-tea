//! Live evaluation of a Programmable State's node graph
//! ([`an1mbyt3_core::NodeGraphDef`]) — the runtime half of `program.rs`
//! (`an1mbyt3-core`, which owns the authored data; see its module doc for
//! the overall design and v1 scope).
//!
//! Deliberately writes straight into the target nodes' own `Transform2D`/
//! fill-color fields (`read_property`/`write_property` below) rather than
//! building a `(NodeId, PropertyKind) -> KeyframeValue` pose map the way
//! clip/blend states do (`Layer::sample_current_state_pose`) — that map is
//! tuned for whole-vector values sampled from a track; a graph computes
//! individual scalar components (one `GraphProperty` axis at a time), and
//! merging a lone axis back into a whole `Vec2` would need to read the
//! artboard's current value anyway, so evaluating directly against it is
//! simpler, not a shortcut. This is also why cross-layer masked blending
//! isn't supported for a Programmable state's output in this first pass —
//! deferred, same as the plan calling it out.

use std::collections::{HashMap, VecDeque};

use an1mbyt3_core::{Artboard, GraphNodeKind, GraphProperty, Node, NodeGraphDef, NodeId, Transform2D};
use glam::Vec2;

use crate::state_machine::Inputs;

/// Live inputs a graph's source nodes read from — `Time` reads
/// `elapsed_seconds` (a `Layer`'s own already-ticking `Playback::time`, no
/// new clock needed), `MousePosition` reads `mouse_artboard`, `GetNumberInput`/
/// `GetBoolInput` read `inputs` — the same `Inputs` a state machine's own
/// `ConditionDef`s check, shared (not copied) since it's already live,
/// mutable session state owned by the enclosing `StateMachine`.
pub struct GraphContext<'a> {
    pub elapsed_seconds: f32,
    pub mouse_artboard: Vec2,
    pub inputs: &'a Inputs,
}

/// Deterministic per-seed pseudo-random value in `0.0..1.0` — a plain
/// integer hash, not reseeded/advanced every frame, so `Random { seed: 7 }`
/// always reads the same number until the author changes `seed` itself
/// (see `GraphNodeKind::Random`'s own doc comment for why: an author dials
/// in a specific-looking randomness rather than the graph flickering every
/// evaluation).
fn hash_seed_to_unit_f32(seed: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E3779B1);
    x ^= x >> 15;
    x = x.wrapping_mul(0x85EBCA6B);
    x ^= x >> 13;
    (x as f32) / (u32::MAX as f32)
}

pub(crate) fn read_property(node: &Node, property: GraphProperty) -> f32 {
    match property {
        GraphProperty::TranslationX => node.transform.translation.x,
        GraphProperty::TranslationY => node.transform.translation.y,
        GraphProperty::Rotation => node.transform.rotation,
        GraphProperty::ScaleX => node.transform.scale.x,
        GraphProperty::ScaleY => node.transform.scale.y,
        GraphProperty::AnchorX => node.transform.anchor.x,
        GraphProperty::AnchorY => node.transform.anchor.y,
        GraphProperty::FillColorR | GraphProperty::FillColorG | GraphProperty::FillColorB | GraphProperty::FillColorA => {
            let color = node.fill_color().unwrap_or([0.0; 4]);
            fill_color_channel(color, property)
        }
        GraphProperty::Visible => normalize_bool(node.visible),
    }
}

pub(crate) fn write_property(node: &mut Node, property: GraphProperty, value: f32) {
    match property {
        GraphProperty::TranslationX => node.transform.translation.x = value,
        GraphProperty::TranslationY => node.transform.translation.y = value,
        GraphProperty::Rotation => node.transform.rotation = value,
        GraphProperty::ScaleX => node.transform.scale.x = value,
        GraphProperty::ScaleY => node.transform.scale.y = value,
        GraphProperty::AnchorX => node.transform.anchor.x = value,
        GraphProperty::AnchorY => node.transform.anchor.y = value,
        GraphProperty::FillColorR | GraphProperty::FillColorG | GraphProperty::FillColorB | GraphProperty::FillColorA => {
            let Some(color) = node.fill_color_mut() else { return };
            let index = fill_color_index(property);
            color[index] = value;
        }
        GraphProperty::Visible => node.visible = is_truthy(value),
    }
}

fn fill_color_index(property: GraphProperty) -> usize {
    match property {
        GraphProperty::FillColorR => 0,
        GraphProperty::FillColorG => 1,
        GraphProperty::FillColorB => 2,
        GraphProperty::FillColorA => 3,
        _ => unreachable!("fill_color_index called with a non-color GraphProperty"),
    }
}

fn fill_color_channel(color: [f32; 4], property: GraphProperty) -> f32 {
    color[fill_color_index(property)]
}

/// Converts a `GraphNodeKind::SetProperty { global: true, .. }` write's
/// incoming value from world space to `target`'s full local
/// `Transform2D::translation` — the "abstract the annoying math of a deeply-
/// nested child following the mouse (or any other world-space source)"
/// feature `SetProperty::global`'s own doc comment describes. `None` for
/// any `property` this doesn't apply to (only `TranslationX`/`TranslationY`
/// have a "global" meaning today — Rotation/Scale could later, but aren't
/// wired up yet) or for a `target` that doesn't resolve, in which case the
/// caller just uses the raw value unconverted, same "an author's own
/// mistake degrades harmlessly" convention every other graph-evaluation
/// edge case in this module already follows.
///
/// Returns the *whole* local translation, not just the one axis this
/// particular `SetProperty` node targets — under a rotated (or non-uniformly
/// scaled) parent, world axes don't correspond 1:1 to local axes, so
/// "change world X" can require changing *both* local x and y (a lone-axis
/// write here would silently drop that mixing and only be correct against
/// an unrotated parent chain). The untouched world axis is read back from
/// `target`'s own *current* `Artboard::world_transform` (not its old local
/// value) before being carried through the same conversion — this is what
/// makes a single globalized axis alone still land correctly, and makes
/// wiring both axes' own `SetProperty` nodes converge to the intended world
/// point regardless of which one `evaluate_graph` visits first: whichever
/// runs second reads back the world position the first one just wrote.
fn global_translation_to_local(artboard: &Artboard, target: NodeId, property: GraphProperty, new_world_axis_value: f32) -> Option<Vec2> {
    if !matches!(property, GraphProperty::TranslationX | GraphProperty::TranslationY) {
        return None;
    }
    let node = artboard.node(target)?;
    let parent_world = match node.parent {
        Some(parent_id) => artboard.world_transform(parent_id),
        None => Transform2D::default(),
    };
    let node_world = artboard.world_transform(target);

    let mut desired_world_translation = node_world.translation;
    match property {
        GraphProperty::TranslationX => desired_world_translation.x = new_world_axis_value,
        GraphProperty::TranslationY => desired_world_translation.y = new_world_axis_value,
        _ => unreachable!("checked above"),
    }
    let desired_world = Transform2D { translation: desired_world_translation, ..node_world };
    Some(parent_world.local_for(&desired_world).translation)
}

/// How many input sockets a node kind exposes — used only to know which
/// socket indices `resolve_input` may be asked about; the evaluator itself
/// doesn't loop over this (each variant's own match arm asks for exactly
/// the sockets it needs).
fn input_arity(kind: &GraphNodeKind) -> u32 {
    match kind {
        GraphNodeKind::Constant(_)
        | GraphNodeKind::Time
        | GraphNodeKind::MousePosition
        | GraphNodeKind::Random { .. }
        | GraphNodeKind::GetProperty { .. }
        | GraphNodeKind::GetNumberInput { .. }
        | GraphNodeKind::GetBoolInput { .. } => 0,
        GraphNodeKind::Sin | GraphNodeKind::Cos | GraphNodeKind::SetProperty { .. } | GraphNodeKind::Not => 1,
        GraphNodeKind::Add
        | GraphNodeKind::Subtract
        | GraphNodeKind::Multiply
        | GraphNodeKind::Divide
        | GraphNodeKind::Min
        | GraphNodeKind::Max
        | GraphNodeKind::GreaterThan
        | GraphNodeKind::LessThan
        | GraphNodeKind::Equals
        | GraphNodeKind::And
        | GraphNodeKind::Or => 2,
        GraphNodeKind::Lerp | GraphNodeKind::Clamp | GraphNodeKind::Select => 3,
    }
}

/// `0.0`/`false` is false, anything else is true — the input-side half of
/// this module's "booleans are numbers too" convention (see `program.rs`'s
/// own module doc). The output side (`normalize_bool`) always produces
/// exactly `1.0`/`0.0`, never a passthrough of whatever nonzero value came
/// in, so a chain of logic nodes can't accumulate a non-canonical "truthy"
/// value that later breaks an exact-equality check.
pub(crate) fn is_truthy(value: f32) -> bool {
    value != 0.0
}

pub(crate) fn normalize_bool(value: bool) -> f32 {
    if value {
        1.0
    } else {
        0.0
    }
}

/// Evaluates `graph` once (one frame) and writes every `SetProperty` node's
/// result straight into `artboard`. Nodes are visited in dependency order
/// (a topological sort of `graph.edges`) wherever one exists; any node left
/// over after that sort (part of a cycle, or downstream of one) is still
/// evaluated exactly once afterward, in `id` order, simply treating
/// whichever of its own inputs come from a not-yet-resolved node as
/// unwired for that evaluation (falls back to that socket's own override,
/// or `0.0`) — a cycle can't wedge evaluation or leave a node dark forever,
/// it just produces a well-defined (if not obviously meaningful) result,
/// same "an author's own mistake, not a validity concern this evaluator
/// panics over" convention `TransitionDef`'s authoring-order tie-break
/// already establishes elsewhere in this workspace.
pub fn evaluate_graph(graph: &NodeGraphDef, ctx: &GraphContext, artboard: &mut Artboard) {
    let mut in_degree: HashMap<u32, usize> = graph.nodes.iter().map(|n| (n.id, 0usize)).collect();
    let mut dependents: HashMap<u32, Vec<u32>> = HashMap::new();
    for edge in &graph.edges {
        if !in_degree.contains_key(&edge.from_node) || !in_degree.contains_key(&edge.to_node) {
            continue; // dangling edge — a stale/authoring-time mistake, ignored.
        }
        *in_degree.get_mut(&edge.to_node).unwrap() += 1;
        dependents.entry(edge.from_node).or_default().push(edge.to_node);
    }

    let mut ready: Vec<u32> = in_degree.iter().filter(|&(_, &d)| d == 0).map(|(&id, _)| id).collect();
    ready.sort_unstable(); // deterministic order among independent nodes
    let mut queue: VecDeque<u32> = ready.into_iter().collect();
    let mut order = Vec::with_capacity(graph.nodes.len());
    let mut remaining = in_degree;
    while let Some(id) = queue.pop_front() {
        order.push(id);
        if let Some(deps) = dependents.get(&id) {
            for &dep in deps {
                let entry = remaining.get_mut(&dep).unwrap();
                *entry -= 1;
                if *entry == 0 {
                    queue.push_back(dep);
                }
            }
        }
    }

    // Anything left over is part of a cycle (or only reachable through
    // one) — still give it exactly one evaluation, in a stable order, so a
    // cycle can't leave a downstream node permanently unevaluated.
    if order.len() < graph.nodes.len() {
        let mut leftover: Vec<u32> = graph.nodes.iter().map(|n| n.id).filter(|id| !order.contains(id)).collect();
        leftover.sort_unstable();
        order.extend(leftover);
    }

    let mut outputs: HashMap<u32, Vec<f32>> = HashMap::new();

    for id in order {
        let node = graph.nodes.iter().find(|n| n.id == id).expect("node id came from graph.nodes itself");

        let resolve_input = |socket: u32, outputs: &HashMap<u32, Vec<f32>>| -> f32 {
            debug_assert!(socket < input_arity(&node.kind), "graph.rs: resolve_input called with an out-of-range socket");
            let wired = graph
                .edges
                .iter()
                .rev()
                .find(|e| e.to_node == id && e.to_socket == socket)
                .and_then(|e| outputs.get(&e.from_node).and_then(|o| o.get(e.from_socket as usize).copied()));
            let overridden = node.overrides.iter().find(|o| o.socket_index == socket);
            match (wired, overridden) {
                (Some(_), Some(o)) if o.use_override => o.value,
                (Some(w), _) => w,
                (None, Some(o)) => o.value,
                (None, None) => 0.0,
            }
        };

        let mut result = Vec::new();
        match &node.kind {
            GraphNodeKind::Constant(v) => result.push(*v),
            GraphNodeKind::Time => result.push(ctx.elapsed_seconds),
            GraphNodeKind::MousePosition => {
                result.push(ctx.mouse_artboard.x);
                result.push(ctx.mouse_artboard.y);
            }
            GraphNodeKind::Random { seed } => result.push(hash_seed_to_unit_f32(*seed)),
            GraphNodeKind::Add => result.push(resolve_input(0, &outputs) + resolve_input(1, &outputs)),
            GraphNodeKind::Subtract => result.push(resolve_input(0, &outputs) - resolve_input(1, &outputs)),
            GraphNodeKind::Multiply => result.push(resolve_input(0, &outputs) * resolve_input(1, &outputs)),
            GraphNodeKind::Divide => {
                let divisor = resolve_input(1, &outputs);
                result.push(if divisor == 0.0 { 0.0 } else { resolve_input(0, &outputs) / divisor });
            }
            GraphNodeKind::Sin => result.push(resolve_input(0, &outputs).sin()),
            GraphNodeKind::Cos => result.push(resolve_input(0, &outputs).cos()),
            GraphNodeKind::Lerp => {
                let a = resolve_input(0, &outputs);
                let b = resolve_input(1, &outputs);
                let t = resolve_input(2, &outputs).clamp(0.0, 1.0);
                result.push(a + (b - a) * t);
            }
            GraphNodeKind::Clamp => {
                let value = resolve_input(0, &outputs);
                let min = resolve_input(1, &outputs);
                let max = resolve_input(2, &outputs);
                result.push(value.clamp(min.min(max), min.max(max)));
            }
            GraphNodeKind::GetProperty { node: target, property } => {
                result.push(artboard.node(*target).map(|n| read_property(n, *property)).unwrap_or(0.0));
            }
            GraphNodeKind::SetProperty { node: target, property, global } => {
                let value = resolve_input(0, &outputs);
                let global_local_translation = if *global { global_translation_to_local(artboard, *target, *property, value) } else { None };
                if let Some(target_node) = artboard.node_mut(*target) {
                    match global_local_translation {
                        Some(local_translation) => target_node.transform.translation = local_translation,
                        None => write_property(target_node, *property, value),
                    }
                }
            }
            GraphNodeKind::Min => result.push(resolve_input(0, &outputs).min(resolve_input(1, &outputs))),
            GraphNodeKind::Max => result.push(resolve_input(0, &outputs).max(resolve_input(1, &outputs))),
            GraphNodeKind::GreaterThan => result.push(normalize_bool(resolve_input(0, &outputs) > resolve_input(1, &outputs))),
            GraphNodeKind::LessThan => result.push(normalize_bool(resolve_input(0, &outputs) < resolve_input(1, &outputs))),
            GraphNodeKind::Equals => result.push(normalize_bool(resolve_input(0, &outputs) == resolve_input(1, &outputs))),
            GraphNodeKind::And => result.push(normalize_bool(is_truthy(resolve_input(0, &outputs)) && is_truthy(resolve_input(1, &outputs)))),
            GraphNodeKind::Or => result.push(normalize_bool(is_truthy(resolve_input(0, &outputs)) || is_truthy(resolve_input(1, &outputs)))),
            GraphNodeKind::Not => result.push(normalize_bool(!is_truthy(resolve_input(0, &outputs)))),
            GraphNodeKind::Select => {
                let condition = resolve_input(0, &outputs);
                let if_true = resolve_input(1, &outputs);
                let if_false = resolve_input(2, &outputs);
                result.push(if is_truthy(condition) { if_true } else { if_false });
            }
            GraphNodeKind::GetNumberInput { name } => result.push(ctx.inputs.number(name)),
            GraphNodeKind::GetBoolInput { name } => result.push(normalize_bool(ctx.inputs.bool(name))),
        }
        outputs.insert(id, result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use an1mbyt3_core::{GraphEdgeDef, GraphNodeDef, NodeId, NodeKind, Path, ShapeNode, SocketOverride};

    fn artboard_with_one_shape() -> (Artboard, NodeId) {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let node = artboard.add_node(Node::new("Square", None, NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None})));
        (artboard, node)
    }

    fn ctx(inputs: &Inputs) -> GraphContext {
        GraphContext { elapsed_seconds: 0.0, mouse_artboard: Vec2::ZERO, inputs }
    }

    #[test]
    fn a_wired_constant_writes_through_to_the_target_property() {
        let (mut artboard, node) = artboard_with_one_shape();
        let inputs = Inputs::default();
        let graph = NodeGraphDef {
            nodes: vec![
                GraphNodeDef { id: 0, kind: GraphNodeKind::Constant(42.0), position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 1, kind: GraphNodeKind::SetProperty { node, property: GraphProperty::TranslationX, global: false }, position: Vec2::ZERO, overrides: vec![] },
            ],
            edges: vec![GraphEdgeDef { from_node: 0, from_socket: 0, to_node: 1, to_socket: 0 }],
        };

        evaluate_graph(&graph, &ctx(&inputs), &mut artboard);

        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 42.0);
    }

    #[test]
    fn an_unwired_input_falls_back_to_its_override_value() {
        let (mut artboard, node) = artboard_with_one_shape();
        let inputs = Inputs::default();
        let graph = NodeGraphDef {
            nodes: vec![GraphNodeDef {
                id: 0,
                kind: GraphNodeKind::SetProperty { node, property: GraphProperty::TranslationY, global: false },
                position: Vec2::ZERO,
                overrides: vec![SocketOverride { socket_index: 0, value: 7.0, use_override: false }],
            }],
            edges: vec![],
        };

        evaluate_graph(&graph, &ctx(&inputs), &mut artboard);

        assert_eq!(artboard.node(node).unwrap().transform.translation.y, 7.0);
    }

    #[test]
    fn use_override_true_beats_a_live_wire() {
        let (mut artboard, node) = artboard_with_one_shape();
        let inputs = Inputs::default();
        let graph = NodeGraphDef {
            nodes: vec![
                GraphNodeDef { id: 0, kind: GraphNodeKind::Constant(100.0), position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef {
                    id: 1,
                    kind: GraphNodeKind::SetProperty { node, property: GraphProperty::Rotation, global: false },
                    position: Vec2::ZERO,
                    overrides: vec![SocketOverride { socket_index: 0, value: 1.5, use_override: true }],
                },
            ],
            edges: vec![GraphEdgeDef { from_node: 0, from_socket: 0, to_node: 1, to_socket: 0 }],
        };

        evaluate_graph(&graph, &ctx(&inputs), &mut artboard);

        assert_eq!(artboard.node(node).unwrap().transform.rotation, 1.5);
    }

    #[test]
    fn time_source_feeds_through_sin_into_a_property() {
        let (mut artboard, node) = artboard_with_one_shape();
        let inputs = Inputs::default();
        let graph = NodeGraphDef {
            nodes: vec![
                GraphNodeDef { id: 0, kind: GraphNodeKind::Time, position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 1, kind: GraphNodeKind::Sin, position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 2, kind: GraphNodeKind::SetProperty { node, property: GraphProperty::TranslationY, global: false }, position: Vec2::ZERO, overrides: vec![] },
            ],
            edges: vec![
                GraphEdgeDef { from_node: 0, from_socket: 0, to_node: 1, to_socket: 0 },
                GraphEdgeDef { from_node: 1, from_socket: 0, to_node: 2, to_socket: 0 },
            ],
        };

        let ctx = GraphContext { elapsed_seconds: std::f32::consts::FRAC_PI_2, mouse_artboard: Vec2::ZERO, inputs: &inputs };
        evaluate_graph(&graph, &ctx, &mut artboard);

        assert!((artboard.node(node).unwrap().transform.translation.y - 1.0).abs() < 1e-5);
    }

    #[test]
    fn a_cycle_evaluates_once_with_unresolved_inputs_treated_as_unwired_rather_than_panicking() {
        let (mut artboard, node) = artboard_with_one_shape();
        let inputs = Inputs::default();
        // 0 -> 1 -> 0 is a cycle. Neither has a real predecessor value
        // available, so both settle at their unwired-input default (0.0);
        // node 2 (SetProperty) is genuinely wired to node 1's (garbage but
        // well-defined) output, so its own override does NOT apply — a
        // wired socket always beats its override unless `use_override` is
        // set, cycle or not.
        let graph = NodeGraphDef {
            nodes: vec![
                GraphNodeDef { id: 0, kind: GraphNodeKind::Add, position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 1, kind: GraphNodeKind::Add, position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef {
                    id: 2,
                    kind: GraphNodeKind::SetProperty { node, property: GraphProperty::TranslationX, global: false },
                    position: Vec2::ZERO,
                    overrides: vec![SocketOverride { socket_index: 0, value: 9.0, use_override: false }],
                },
            ],
            edges: vec![
                GraphEdgeDef { from_node: 0, from_socket: 0, to_node: 1, to_socket: 0 },
                GraphEdgeDef { from_node: 1, from_socket: 0, to_node: 0, to_socket: 0 },
                GraphEdgeDef { from_node: 1, from_socket: 0, to_node: 2, to_socket: 0 },
            ],
        };

        evaluate_graph(&graph, &ctx(&inputs), &mut artboard);

        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 0.0);
    }

    #[test]
    fn a_cyclic_input_still_falls_back_to_override_when_pinned() {
        let (mut artboard, node) = artboard_with_one_shape();
        let inputs = Inputs::default();
        let graph = NodeGraphDef {
            nodes: vec![
                GraphNodeDef { id: 0, kind: GraphNodeKind::Add, position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 1, kind: GraphNodeKind::Add, position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef {
                    id: 2,
                    kind: GraphNodeKind::SetProperty { node, property: GraphProperty::TranslationX, global: false },
                    position: Vec2::ZERO,
                    overrides: vec![SocketOverride { socket_index: 0, value: 9.0, use_override: true }],
                },
            ],
            edges: vec![
                GraphEdgeDef { from_node: 0, from_socket: 0, to_node: 1, to_socket: 0 },
                GraphEdgeDef { from_node: 1, from_socket: 0, to_node: 0, to_socket: 0 },
                GraphEdgeDef { from_node: 1, from_socket: 0, to_node: 2, to_socket: 0 },
            ],
        };

        evaluate_graph(&graph, &ctx(&inputs), &mut artboard);

        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 9.0);
    }

    #[test]
    fn get_property_reads_a_live_value_off_another_node() {
        let (mut artboard, node) = artboard_with_one_shape();
        let inputs = Inputs::default();
        artboard.node_mut(node).unwrap().transform.translation.x = 5.0;
        let other = artboard.add_node(Node::new("Other", None, NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None})));

        let graph = NodeGraphDef {
            nodes: vec![
                GraphNodeDef { id: 0, kind: GraphNodeKind::GetProperty { node, property: GraphProperty::TranslationX }, position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 1, kind: GraphNodeKind::SetProperty { node: other, property: GraphProperty::TranslationX, global: false }, position: Vec2::ZERO, overrides: vec![] },
            ],
            edges: vec![GraphEdgeDef { from_node: 0, from_socket: 0, to_node: 1, to_socket: 0 }],
        };

        evaluate_graph(&graph, &ctx(&inputs), &mut artboard);

        assert_eq!(artboard.node(other).unwrap().transform.translation.x, 5.0);
    }

    #[test]
    fn min_and_max_pick_the_smaller_and_larger_of_two_inputs() {
        let (mut artboard, node) = artboard_with_one_shape();
        let inputs = Inputs::default();
        let graph = NodeGraphDef {
            nodes: vec![
                GraphNodeDef { id: 0, kind: GraphNodeKind::Constant(3.0), position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 1, kind: GraphNodeKind::Constant(7.0), position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 2, kind: GraphNodeKind::Min, position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 3, kind: GraphNodeKind::Max, position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 4, kind: GraphNodeKind::SetProperty { node, property: GraphProperty::TranslationX, global: false }, position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 5, kind: GraphNodeKind::SetProperty { node, property: GraphProperty::TranslationY, global: false }, position: Vec2::ZERO, overrides: vec![] },
            ],
            edges: vec![
                GraphEdgeDef { from_node: 0, from_socket: 0, to_node: 2, to_socket: 0 },
                GraphEdgeDef { from_node: 1, from_socket: 0, to_node: 2, to_socket: 1 },
                GraphEdgeDef { from_node: 0, from_socket: 0, to_node: 3, to_socket: 0 },
                GraphEdgeDef { from_node: 1, from_socket: 0, to_node: 3, to_socket: 1 },
                GraphEdgeDef { from_node: 2, from_socket: 0, to_node: 4, to_socket: 0 },
                GraphEdgeDef { from_node: 3, from_socket: 0, to_node: 5, to_socket: 0 },
            ],
        };

        evaluate_graph(&graph, &ctx(&inputs), &mut artboard);

        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 3.0);
        assert_eq!(artboard.node(node).unwrap().transform.translation.y, 7.0);
    }

    #[test]
    fn logic_nodes_normalize_to_exactly_one_or_zero() {
        let (mut artboard, node) = artboard_with_one_shape();
        let inputs = Inputs::default();
        let graph = NodeGraphDef {
            nodes: vec![
                GraphNodeDef { id: 0, kind: GraphNodeKind::Constant(5.0), position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 1, kind: GraphNodeKind::Constant(2.0), position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 2, kind: GraphNodeKind::GreaterThan, position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 3, kind: GraphNodeKind::Not, position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef {
                    id: 4,
                    kind: GraphNodeKind::Select,
                    position: Vec2::ZERO,
                    overrides: vec![SocketOverride { socket_index: 1, value: 100.0, use_override: false }, SocketOverride { socket_index: 2, value: 200.0, use_override: false }],
                },
                GraphNodeDef { id: 5, kind: GraphNodeKind::SetProperty { node, property: GraphProperty::TranslationX, global: false }, position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 6, kind: GraphNodeKind::SetProperty { node, property: GraphProperty::TranslationY, global: false }, position: Vec2::ZERO, overrides: vec![] },
            ],
            edges: vec![
                GraphEdgeDef { from_node: 0, from_socket: 0, to_node: 2, to_socket: 0 },
                GraphEdgeDef { from_node: 1, from_socket: 0, to_node: 2, to_socket: 1 },
                GraphEdgeDef { from_node: 2, from_socket: 0, to_node: 3, to_socket: 0 },
                GraphEdgeDef { from_node: 3, from_socket: 0, to_node: 6, to_socket: 0 },
                GraphEdgeDef { from_node: 2, from_socket: 0, to_node: 4, to_socket: 0 },
                GraphEdgeDef { from_node: 4, from_socket: 0, to_node: 5, to_socket: 0 },
            ],
        };

        evaluate_graph(&graph, &ctx(&inputs), &mut artboard);

        // `5 > 2` is true (Select picks its "if_true" override, 100.0);
        // `Not` of that same true is exactly 0.0, not some other falsy value.
        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 100.0);
        assert_eq!(artboard.node(node).unwrap().transform.translation.y, 0.0);
    }

    #[test]
    fn get_number_and_bool_input_read_the_state_machines_live_inputs() {
        let (mut artboard, node) = artboard_with_one_shape();
        let mut inputs = Inputs::default();
        inputs.set_number("speed", 12.5);
        inputs.set_bool("active", true);

        let graph = NodeGraphDef {
            nodes: vec![
                GraphNodeDef { id: 0, kind: GraphNodeKind::GetNumberInput { name: "speed".into() }, position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 1, kind: GraphNodeKind::GetBoolInput { name: "active".into() }, position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 2, kind: GraphNodeKind::SetProperty { node, property: GraphProperty::TranslationX, global: false }, position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 3, kind: GraphNodeKind::SetProperty { node, property: GraphProperty::TranslationY, global: false }, position: Vec2::ZERO, overrides: vec![] },
            ],
            edges: vec![
                GraphEdgeDef { from_node: 0, from_socket: 0, to_node: 2, to_socket: 0 },
                GraphEdgeDef { from_node: 1, from_socket: 0, to_node: 3, to_socket: 0 },
            ],
        };

        evaluate_graph(&graph, &ctx(&inputs), &mut artboard);

        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 12.5);
        assert_eq!(artboard.node(node).unwrap().transform.translation.y, 1.0);
    }

    /// A grandchild nested under a rotated, scaled parent chain — the exact
    /// "annoying math" scenario `SetProperty::global` exists to abstract
    /// away. Setting a global `TranslationX`/`TranslationY` should land the
    /// grandchild's *world* position at the authored value, not its local
    /// one, regardless of the parent chain's own rotation/scale.
    fn artboard_with_rotated_scaled_grandchild() -> (Artboard, NodeId) {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let root = artboard.add_node(Node::new("Root", None, NodeKind::Bone(an1mbyt3_core::BoneNode { length: 1.0 })));
        artboard.node_mut(root).unwrap().transform =
            Transform2D { translation: Vec2::new(100.0, 50.0), rotation: std::f32::consts::FRAC_PI_2, scale: Vec2::new(2.0, 2.0), anchor: Vec2::ZERO };

        let child = artboard.add_node(Node::new("Child", Some(root), NodeKind::Bone(an1mbyt3_core::BoneNode { length: 1.0 })));
        artboard.node_mut(child).unwrap().transform.translation = Vec2::new(5.0, 0.0);

        let grandchild = artboard.add_node(Node::new(
            "Grandchild",
            Some(child),
            NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None }),
        ));
        (artboard, grandchild)
    }

    #[test]
    fn global_translation_lands_the_grandchild_at_the_authored_world_point() {
        let (mut artboard, grandchild) = artboard_with_rotated_scaled_grandchild();
        let inputs = Inputs::default();
        let graph = NodeGraphDef {
            nodes: vec![
                GraphNodeDef { id: 0, kind: GraphNodeKind::Constant(300.0), position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 1, kind: GraphNodeKind::Constant(80.0), position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 2, kind: GraphNodeKind::SetProperty { node: grandchild, property: GraphProperty::TranslationX, global: true }, position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 3, kind: GraphNodeKind::SetProperty { node: grandchild, property: GraphProperty::TranslationY, global: true }, position: Vec2::ZERO, overrides: vec![] },
            ],
            edges: vec![
                GraphEdgeDef { from_node: 0, from_socket: 0, to_node: 2, to_socket: 0 },
                GraphEdgeDef { from_node: 1, from_socket: 0, to_node: 3, to_socket: 0 },
            ],
        };

        evaluate_graph(&graph, &ctx(&inputs), &mut artboard);

        let world = artboard.world_transform(grandchild);
        assert!((world.translation.x - 300.0).abs() < 1e-3, "expected world x ~300, got {}", world.translation.x);
        assert!((world.translation.y - 80.0).abs() < 1e-3, "expected world y ~80, got {}", world.translation.y);
    }

    #[test]
    fn a_single_globalized_axis_alone_preserves_the_other_axiss_current_world_value() {
        let (mut artboard, grandchild) = artboard_with_rotated_scaled_grandchild();
        let world_before = artboard.world_transform(grandchild).translation;
        let inputs = Inputs::default();
        let graph = NodeGraphDef {
            nodes: vec![
                GraphNodeDef { id: 0, kind: GraphNodeKind::Constant(300.0), position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 1, kind: GraphNodeKind::SetProperty { node: grandchild, property: GraphProperty::TranslationX, global: true }, position: Vec2::ZERO, overrides: vec![] },
            ],
            edges: vec![GraphEdgeDef { from_node: 0, from_socket: 0, to_node: 1, to_socket: 0 }],
        };

        evaluate_graph(&graph, &ctx(&inputs), &mut artboard);

        let world_after = artboard.world_transform(grandchild).translation;
        assert!((world_after.x - 300.0).abs() < 1e-3, "expected world x ~300, got {}", world_after.x);
        assert!((world_after.y - world_before.y).abs() < 1e-3, "y should be unchanged: was {}, now {}", world_before.y, world_after.y);
    }

    #[test]
    fn global_true_on_a_root_node_behaves_like_local_since_the_parent_is_identity() {
        let (mut artboard, node) = artboard_with_one_shape();
        let inputs = Inputs::default();
        let graph = NodeGraphDef {
            nodes: vec![
                GraphNodeDef { id: 0, kind: GraphNodeKind::Constant(42.0), position: Vec2::ZERO, overrides: vec![] },
                GraphNodeDef { id: 1, kind: GraphNodeKind::SetProperty { node, property: GraphProperty::TranslationX, global: true }, position: Vec2::ZERO, overrides: vec![] },
            ],
            edges: vec![GraphEdgeDef { from_node: 0, from_socket: 0, to_node: 1, to_socket: 0 }],
        };

        evaluate_graph(&graph, &ctx(&inputs), &mut artboard);

        assert_eq!(artboard.node(node).unwrap().transform.translation.x, 42.0);
    }
}
