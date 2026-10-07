//! Artboard: one animatable canvas within a [`crate::Document`].
//!
//! Owns the actual `NodeId -> Node` arena (via `slotmap::SlotMap`) — node IDs
//! are only unique/meaningful within the artboard that created them (see
//! `ids.rs`), so the arena lives here rather than on `Document` directly.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use slotmap::SlotMap;

use crate::{
    ids::AnimationId,
    ids::NodeId,
    ids::StateMachineId,
    node::{Node, NodeKind},
    post_effect::PostEffectLayer,
    state_machine::StateMachineDef,
    timeline::Animation,
    transform::Transform2D,
};

/// How many `Node::parent` hops any ancestor-chain walk in this module will
/// follow before giving up — a generous backstop far beyond any real
/// scene's nesting depth, purely against a cyclic parent chain (which
/// nothing that constructs `Node::parent` links is supposed to ever
/// produce, but several of these walks run from `render()`'s per-shape,
/// per-frame path, where an *unbounded* walk into a cycle would hang the
/// whole render loop — and since wasm calls run synchronously on the JS
/// thread, the whole UI — solid, rather than just rendering one node wrong).
const MAX_ANCESTOR_DEPTH: u32 = 1000;

fn default_resolution_scale() -> f32 {
    1.0
}

/// Unity's default 2D gravity (-9.81 m/s²) at 32 art pixels per meter.
fn default_gravity() -> glam::Vec2 {
    glam::Vec2::new(0.0, -9.81 * 32.0)
}

/// One artboard: a fixed-size canvas containing a tree of [`Node`]s.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artboard {
    /// Scene gravity for Rigidbody 2D simulation, world units (art pixels)
    /// per second squared — Unity's Physics 2D Gravity. Default: falling
    /// down at roughly Unity's -9.81 with 32 px per meter. Top-down games
    /// set it to zero.
    #[serde(default = "default_gravity")]
    pub gravity: glam::Vec2,
    pub name: String,
    pub width: f32,
    pub height: f32,
    /// This artboard's pixelation block size, in pixels — `1.0` (the
    /// default, and every artboard's value before this field existed) is
    /// native/off; a larger value (e.g. `8.0`) divides the whole canvas down
    /// by that factor before the scene (and every post effect) renders, then
    /// nearest-upscales back out, giving the whole canvas a chunky/pixelated
    /// look driven by one number instead of needing a per-shape
    /// `PostEffect::Pixelate` layer — and letting every post effect run over
    /// fewer pixels rather than needlessly at full native resolution only to
    /// be thrown away by the pixelation anyway. Unlike a fraction of surface
    /// size, this is an absolute pixel count independent of the
    /// canvas/window's own size — scaling the canvas never changes how
    /// chunky this reads. See
    /// `an1mbyt3_renderer::Renderer::render_with_post_effects` for where
    /// this actually resizes the offscreen render target.
    /// `#[serde(default = "default_resolution_scale")]` for the usual
    /// back-compat reason: every artboard saved before this field existed
    /// already rendered at native resolution.
    #[serde(default = "default_resolution_scale")]
    pub resolution_scale: f32,
    nodes: SlotMap<NodeId, Node>,
    /// Node IDs with `parent == None`, in draw order (later entries draw on
    /// top). This is the artboard's actual scene-graph root list — a node's
    /// `Node::parent` field points up the tree, this list is what lets you walk
    /// down from the artboard.
    root_nodes: Vec<NodeId>,
    /// Draw-order list of a `Some(parent)` node's children, keyed by that
    /// parent's own id — `root_nodes`'s per-parent counterpart, added for
    /// "Bring Forward"/"Send Backward"/"Bring to Front"/"Send to Back"
    /// (`Command::MoveNodeForward` and friends). `#[serde(default)]` so a
    /// `.tea`/`.teab` saved before this field existed still loads (empty
    /// map) — `Artboard::ordering_list_mut` lazily materializes an entry the
    /// *first* time a given parent's children are actually reordered, seeded
    /// from `children_of`'s existing fallback-scan order, so an unmigrated
    /// document's nested layers keep rendering in exactly the order they
    /// always have until someone actually reorders them. Not present at all
    /// for a parent whose children have never been reordered — `children_of`
    /// falls back to the old linear-scan-over-the-arena order in that case,
    /// same as before this field existed.
    #[serde(default)]
    child_order: HashMap<NodeId, Vec<NodeId>>,
    /// Named, authored animations — see [`Animation`]'s module doc
    /// (`timeline.rs`) for why this lives here (artboard-scoped, since a
    /// track's `NodeId`s only make sense within one artboard) rather than on
    /// `Document`. `#[serde(default)]` so a `.tea`/`.teab` file saved before
    /// this field existed still loads (empty `SlotMap`), same precedent
    /// `Document::fonts` already established.
    #[serde(default)]
    animations: SlotMap<AnimationId, Animation>,
    /// Named, authored state machines — see [`StateMachineDef`]'s module doc
    /// (`state_machine.rs`) for why this lives here (a state references an
    /// [`AnimationId`], which only makes sense within one artboard) rather
    /// than on `Document`. `#[serde(default)]` for the same back-compat
    /// reason `animations` has one — a `.tea`/`.teab` saved before this
    /// field existed still loads (empty `SlotMap`).
    #[serde(default)]
    state_machines: SlotMap<StateMachineId, StateMachineDef>,
    /// The ordered stack of post-processing effects applied when this
    /// artboard renders — AN1MBYT3_BUILD_PLAN.md §11's "layered approach,
    /// allow for duplicate effects" post-processing panel. A plain `Vec`,
    /// not a `SlotMap`, since nothing ever needs to reference one entry by a
    /// stable id across a reorder/removal the way `NodeId`/`AnimationId`
    /// do — the Effects panel always replaces the *whole* list on any edit
    /// (`Command::SetPostEffects`), the same "whole-list swap, simplest
    /// command shape" convention `StateMachineDef::states`/`layers` already
    /// established. `#[serde(default)]` so a `.tea`/`.teab` saved before
    /// this field existed still loads (empty list — renders exactly as
    /// before, no post-processing).
    #[serde(default)]
    post_effects: Vec<PostEffectLayer>,
    /// Counter behind `Node::id`'s default `shape(<serial>)` form — see
    /// `add_node`'s own doc comment for why a fresh serial (not the node's
    /// own `NodeId`) is what backs the default id. Monotonically
    /// increasing, never reused even across deletes (same "never reuse a
    /// freed slot's identity" precedent `slotmap`'s own generational keys
    /// already establish for `NodeId` itself), so two nodes can never
    /// collide on their *auto-generated* id even if one was deleted first.
    /// `#[serde(default)]` (`0`) for the usual back-compat reason — every
    /// `.tea`/`.teab` saved before this field existed has every node's `id`
    /// already empty (`Node::id`'s own `#[serde(default)]`), so starting
    /// fresh at `0` can't collide with anything real.
    #[serde(default)]
    next_node_serial: u32,
    /// Which `Camera` node (if any) `WasmEditor::render` actually views this
    /// artboard through while Playground runs — Unity's "Main Camera" tag,
    /// an authored scene property (saved in `.tea`/`.teab`), not an editor-
    /// session preference. `None` (`#[serde(default)]`, the only possible
    /// value before `NodeKind::Camera` existed) falls back to the editor's
    /// own free-roam pan/zoom, exactly today's existing behavior — nothing
    /// changes for a document with no authored camera. Doesn't validate that
    /// the referenced node still exists/is still a `Camera` — same "a stale
    /// reference degrades gracefully, not a validity concern this data model
    /// enforces" convention `ComponentInstance::variant` already established;
    /// a stale id just means Playground falls back to free-roam, same as
    /// `None`.
    #[serde(default)]
    active_camera: Option<NodeId>,
    /// Soft-delete flag — same reasoning as `ComponentDefinition::deleted`/
    /// `Node::deleted`: undo/redo needs a deleted artboard's `ArtboardId` to
    /// stay valid for any earlier command still on the undo stack (e.g. an
    /// edit made to one of its nodes before the whole scene was deleted).
    /// `Document::artboards` (the Scenes panel's listing, and everything
    /// `dto::summarize_document` builds from it) skips deleted artboards;
    /// `Document::artboard`/`artboard_mut` still resolve them. `#[serde(default)]`
    /// for the usual back-compat reason — every artboard saved before this
    /// field existed is implicitly `false` (never deleted).
    #[serde(default)]
    pub deleted: bool,
}

impl Artboard {
    /// A [`crate::Node3D`]'s local-to-world 3D matrix: its own transform
    /// composed with every *Node3D* ancestor's (2D ancestors are passed
    /// through — they don't move 3D content). `None` for a non-3D node.
    pub fn world_matrix_3d(&self, id: NodeId) -> Option<glam::Mat4> {
        let node = self.node(id)?;
        let NodeKind::Node3D(own) = &node.kind else { return None };
        let mut matrix = own.transform.matrix();
        let mut parent = node.parent;
        while let Some(parent_id) = parent {
            let Some(parent_node) = self.node(parent_id) else { break };
            if let NodeKind::Node3D(p) = &parent_node.kind {
                matrix = p.transform.matrix() * matrix;
            }
            parent = parent_node.parent;
        }
        Some(matrix)
    }

    pub fn new(name: impl Into<String>, width: f32, height: f32) -> Self {
        Self {
            name: name.into(),
            width,
            height,
            gravity: default_gravity(),
            resolution_scale: default_resolution_scale(),
            nodes: SlotMap::with_key(),
            root_nodes: Vec::new(),
            child_order: HashMap::new(),
            animations: SlotMap::with_key(),
            state_machines: SlotMap::with_key(),
            post_effects: Vec::new(),
            next_node_serial: 0,
            active_camera: None,
            deleted: false,
        }
    }

    pub fn active_camera(&self) -> Option<NodeId> {
        self.active_camera
    }

    /// `Command::SetActiveCamera`'s own mutation point — `pub(crate)`, not
    /// `pub`, same "every edit goes through a command" convention
    /// `post_effects_mut` already establishes.
    pub(crate) fn active_camera_mut(&mut self) -> &mut Option<NodeId> {
        &mut self.active_camera
    }

    /// The artboard's current post-processing stack, in render order (index
    /// `0` applies first) — every entry, including disabled ones (the
    /// Effects panel needs to render a disabled row too, not just skip it;
    /// `an1mbyt3_renderer::Renderer::render_with_post_effects`'s own caller
    /// is what filters to only `enabled` entries before rendering).
    pub fn post_effects(&self) -> &[PostEffectLayer] {
        &self.post_effects
    }

    /// `Command::SetPostEffects`'s own mutation point — `pub(crate)`, not
    /// `pub`, since every edit to this list is meant to go through that
    /// command (undo/redo), never a direct field write from outside this
    /// crate.
    pub(crate) fn post_effects_mut(&mut self) -> &mut Vec<PostEffectLayer> {
        &mut self.post_effects
    }

    /// Inserts `node` into this artboard's arena. If `node.parent` is `None`,
    /// it's also appended to `root_nodes` so it actually gets drawn/walked;
    /// if it's `Some(parent)` *and* `parent` already has a materialized
    /// `child_order` entry (i.e. its children have been reordered at least
    /// once before), the new node is appended there too, so it draws on top
    /// of that parent's existing children rather than silently falling
    /// through to `children_of`'s arena-order fallback in a different spot
    /// than a freshly-inserted node actually belongs. A parent with no
    /// `child_order` entry yet needs no bookkeeping here — `children_of`'s
    /// arena-scan fallback already includes every child regardless, new or
    /// old, since it re-scans the whole arena on every call.
    pub fn add_node(&mut self, mut node: Node) -> NodeId {
        // `Node::new` always leaves `id` empty — this is the one place that
        // actually fills in the default `shape(<serial>)` form (see
        // `Node::id`'s own doc comment for why it can't happen in
        // `Node::new` itself: generating a genuinely unique serial needs
        // this artboard's own counter). A caller that *did* set a custom
        // `id` before calling this (or a duplicated node whose id was
        // deliberately cleared first — see `duplicate_node_recursive`,
        // `an1mbyt3-wasm`) is left alone.
        if node.id.is_empty() {
            node.id = format!("shape({})", self.next_node_serial);
            self.next_node_serial += 1;
        }
        let parent = node.parent;
        // A parent without an explicit `child_order` draws its children in
        // arena (slot) order — fine while slots are only ever appended, but
        // `despawn` frees slots and the next insert reuses them, which would
        // slot a brand-new child *underneath* its older siblings (a spawned
        // prefab's HP fill drawing beneath its own HP-bar background). So the
        // parent's current order is recorded first, and the new node is
        // always appended — it draws on top, as a freshly added node should.
        if let Some(p) = parent {
            if !self.child_order.contains_key(&p) {
                let existing: Vec<NodeId> = self.children_of(p).map(|(child, _)| child).collect();
                self.child_order.insert(p, existing);
            }
        }
        let id = self.nodes.insert(node);
        match parent {
            None => self.root_nodes.push(id),
            Some(p) => {
                if let Some(order) = self.child_order.get_mut(&p) {
                    order.push(id);
                }
            }
        }
        id
    }

    /// Spawns a live, ephemeral copy of `source` — a single node, not its
    /// children if it's a `Group` (same "one node, not a subtree" scope
    /// limit `an1mbyt3-wasm::add_component` already draws elsewhere in this
    /// codebase) — for a Blocks program's `Statement::CreateCloneOf`. Unlike
    /// `add_node` (used by every undo-tracked document edit, via
    /// `Command::AddNode`), this never goes through `Command`: a clone is
    /// pure runtime state, gone the instant it's `despawn`ed or the owning
    /// state resets, never serialized into the saved document and never
    /// part of undo history — same category as every other per-frame write
    /// `an1mbyt3_runtime::block_interpreter`/`graph` already make directly
    /// against a live `Artboard`. Returns `None` if `source` doesn't exist.
    pub fn spawn_clone_of(&mut self, source: NodeId) -> Option<NodeId> {
        let mut node = self.nodes.get(source)?.clone();
        // A clone must not keep the original's `id` — `add_node` only mints
        // a fresh `shape(<serial>)` id when it's empty, and copying the
        // source's id verbatim would leave two live nodes answering to the
        // same `getCompByID` lookup (`scriptRuntime.ts`). `classes`
        // deliberately *does* carry over — `for each node with class X`
        // (`ListExpr::NodesWithClass`) matching a clone the same way it
        // matches its original is the whole point (a spawned bullet/enemy
        // should be swept up by the same class-based scripts that already
        // act on its prototype).
        node.id = String::new();
        Some(self.add_node(node))
    }

    /// Permanently removes `id` — the clone-lifecycle counterpart to
    /// `spawn_clone_of` (`Statement::DeleteThisClone`). Unlike
    /// `Command::SetDeleted` (a soft flag, kept forever so undo/redo can
    /// still resolve the id), this actually frees the slotmap slot: a clone
    /// was never a document edit, so there's nothing to undo back to, and
    /// hard-freeing means a long spawn/despawn-heavy Playground session
    /// doesn't leak slots forever. Not recursive (same "one node, not a
    /// subtree" scope `spawn_clone_of` already draws) — a cloned `Group`'s
    /// own (shared, non-cloned) children are untouched. A no-op if `id`
    /// doesn't exist (already removed, or never existed).
    pub fn despawn(&mut self, id: NodeId) {
        let Some(node) = self.nodes.remove(id) else { return };
        match node.parent {
            None => self.root_nodes.retain(|&n| n != id),
            Some(p) => {
                if let Some(order) = self.child_order.get_mut(&p) {
                    order.retain(|&n| n != id);
                }
            }
        }
        self.child_order.remove(&id);
    }

    /// Moves an already-inserted node to a new parent (or to root, via
    /// `None`), fixing up `root_nodes`/`child_order` bookkeeping in both
    /// directions — `add_node` only ever manages those lists at insertion
    /// time, so anything that later changes `Node::parent` (drag-to-reparent
    /// in the Layers panel, SVG group-hierarchy import) has to go through
    /// here instead of writing `node.parent` directly. `Command::ReparentNode`
    /// is the only caller. Doesn't itself guard against creating a cycle
    /// (reparenting a node under its own descendant, which would make
    /// `world_transform` recurse forever) — same "caller bug, not a
    /// recoverable runtime condition" convention as everywhere else in this
    /// layer; the wasm editor's `reparent_node` method is what actually
    /// checks the ancestor chain before issuing this.
    pub fn reparent_node(&mut self, node: NodeId, new_parent: Option<NodeId>) {
        let old_parent = self.nodes.get(node).expect("Artboard::reparent_node: node not found").parent;

        match old_parent {
            None => self.root_nodes.retain(|&id| id != node),
            Some(p) => {
                if let Some(order) = self.child_order.get_mut(&p) {
                    order.retain(|&id| id != node);
                }
            }
        }
        match new_parent {
            None => self.root_nodes.push(node),
            Some(p) => {
                if let Some(order) = self.child_order.get_mut(&p) {
                    order.push(node);
                }
            }
        }

        self.nodes.get_mut(node).unwrap().parent = new_parent;
    }

    /// `reparent_node`'s positional counterpart — inserts `node` at exactly
    /// `index` within `new_parent`'s children (or `root_nodes`, clamped to
    /// the resulting list's length) instead of always appending to the end.
    /// What the Layers panel's drag-to-reorder gesture actually needs:
    /// `reparent_node` alone can only ever move a node to the *back* of its
    /// new list, so dropping a dragged layer between two others (or even
    /// just reordering within the same parent) always shoved it to the very
    /// end regardless of where it was dropped — indistinguishable from "did
    /// nothing" whenever it was already last.
    ///
    /// Lazily materializes `new_parent`'s `child_order` the same way
    /// `ordering_list_mut` does (same back-compat reason: a document that
    /// predates `child_order` keeps its existing nesting order until
    /// something actually reorders it) — explicitly excluding `node` itself
    /// from that materialized scan, since `node` may already be one of
    /// `new_parent`'s children (a same-parent reorder) and would otherwise
    /// appear twice: once from the fallback scan, once from the explicit
    /// insert below.
    pub fn reparent_node_at(&mut self, node: NodeId, new_parent: Option<NodeId>, index: usize) {
        let old_parent = self.nodes.get(node).expect("Artboard::reparent_node_at: node not found").parent;
        match old_parent {
            None => self.root_nodes.retain(|&id| id != node),
            Some(p) => {
                if let Some(order) = self.child_order.get_mut(&p) {
                    order.retain(|&id| id != node);
                }
            }
        }
        match new_parent {
            None => {
                let clamped = index.min(self.root_nodes.len());
                self.root_nodes.insert(clamped, node);
            }
            Some(p) => {
                if !self.child_order.contains_key(&p) {
                    let initial: Vec<NodeId> = self.nodes.iter().filter(|&(id, n)| n.parent == Some(p) && id != node).map(|(id, _)| id).collect();
                    self.child_order.insert(p, initial);
                }
                let order = self.child_order.get_mut(&p).expect("just inserted or already present above");
                let clamped = index.min(order.len());
                order.insert(clamped, node);
            }
        }
        self.nodes.get_mut(node).unwrap().parent = new_parent;
    }

    /// Read-only counterpart to `ordering_list_mut`/`reparent_node_at` — the
    /// current draw-order list for `parent`'s children (or `root_nodes` if
    /// `None`), without materializing anything (unlike `ordering_list_mut`,
    /// which is `&mut` and lazily persists the fallback scan): just reports
    /// what that scan would currently produce, or the already-materialized
    /// `child_order` if reordering has happened at least once before. Used
    /// to resolve "insert before this specific sibling" into a concrete
    /// index (`WasmEditor::reparent_node_before`) without needing a `&mut
    /// self` borrow just to look.
    pub fn sibling_order(&self, parent: Option<NodeId>) -> Vec<NodeId> {
        match parent {
            None => self.root_nodes.clone(),
            Some(p) => match self.child_order.get(&p) {
                Some(order) => order.clone(),
                None => self.nodes.iter().filter(|(_, n)| n.parent == Some(p)).map(|(id, _)| id).collect(),
            },
        }
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id)
    }

    pub fn root_nodes(&self) -> &[NodeId] {
        &self.root_nodes
    }

    /// Every node id in the artboard, any `NodeKind`, root or nested,
    /// deleted or not — unlike `shapes`/`texts`, which filter to one kind
    /// and skip deleted nodes for rendering/UI purposes, this is for callers
    /// that need to reach every node regardless (e.g. scrubbing every node's
    /// `Node::translation_track`, whatever kind it is — see `editor.rs`'s
    /// `set_scrub_time`).
    pub fn node_ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes.keys()
    }

    /// Every node whose `parent` is exactly `id`, in draw order — the
    /// down-the-tree counterpart to `Node::parent`'s up-the-tree pointer
    /// (`node.rs`'s note on why only the upward direction is stored). Order
    /// comes from `child_order[id]` if that parent's children have ever been
    /// reordered (`Command::MoveNodeForward` and friends materialize it
    /// lazily, see `ordering_list_mut`); otherwise falls back to a linear
    /// scan over the arena, in whatever order nodes happened to be inserted
    /// — fine at the node counts this engine deals with, and exactly this
    /// method's entire behavior before `child_order` existed.
    pub fn children_of(&self, id: NodeId) -> impl Iterator<Item = (NodeId, &Node)> {
        match self.child_order.get(&id) {
            Some(order) => order.iter().filter_map(|&cid| self.nodes.get(cid).map(|n| (cid, n))).collect::<Vec<_>>(),
            None => self.nodes.iter().filter(|(_, node)| node.parent == Some(id)).map(|(id, n)| (id, n)).collect::<Vec<_>>(),
        }
        .into_iter()
    }

    /// The list `node` currently lives in — `root_nodes` if it's a root
    /// node, else `child_order[parent]` (lazily materialized from
    /// `children_of`'s own fallback-scan order the first time this parent's
    /// children are actually reordered, so a document that predates
    /// `child_order` keeps rendering its existing nesting in exactly the
    /// order it always has until someone actually reorders it). The one
    /// place `Command::MoveNodeForward`/`SendNodeBackward`/`BringNodeToFront`/
    /// `SendNodeToBack`/`SetNodeIndex` all funnel through to find the list
    /// they need to mutate.
    fn ordering_list_mut(&mut self, node: NodeId) -> Option<&mut Vec<NodeId>> {
        let parent = self.nodes.get(node)?.parent;
        match parent {
            None => Some(&mut self.root_nodes),
            Some(p) => {
                if !self.child_order.contains_key(&p) {
                    let initial: Vec<NodeId> = self.nodes.iter().filter(|(_, n)| n.parent == Some(p)).map(|(id, _)| id).collect();
                    self.child_order.insert(p, initial);
                }
                self.child_order.get_mut(&p)
            }
        }
    }

    /// `node`'s current position within whichever ordering list it lives in
    /// — the read-only counterpart to `ordering_list_mut`, used by
    /// `Command::SetNodeIndex`'s apply to capture its own inverse (the
    /// index `node` had *before* the set, not just "one step" the way
    /// `move_node_forward`/`move_node_backward` need).
    pub fn node_order_index(&self, node: NodeId) -> Option<usize> {
        let parent = self.nodes.get(node)?.parent;
        let list: Vec<NodeId> = match parent {
            None => self.root_nodes.clone(),
            Some(p) => match self.child_order.get(&p) {
                Some(order) => order.clone(),
                None => self.nodes.iter().filter(|(_, n)| n.parent == Some(p)).map(|(id, _)| id).collect(),
            },
        };
        list.iter().position(|&id| id == node)
    }

    /// Swaps `node` one step later in its draw-order list ("Bring Forward")
    /// — returns `false` (a harmless no-op) if it's already last. See
    /// `Command::MoveNodeForward`'s doc comment for why a no-op here needs
    /// no special-casing on the command layer: undoing a no-op by re-running
    /// the same boundary check is itself a no-op.
    pub fn move_node_forward(&mut self, node: NodeId) -> bool {
        let Some(list) = self.ordering_list_mut(node) else { return false };
        let Some(idx) = list.iter().position(|&id| id == node) else { return false };
        if idx + 1 >= list.len() {
            return false;
        }
        list.swap(idx, idx + 1);
        true
    }

    /// `move_node_forward`'s mirror — one step earlier ("Send Backward").
    pub fn move_node_backward(&mut self, node: NodeId) -> bool {
        let Some(list) = self.ordering_list_mut(node) else { return false };
        let Some(idx) = list.iter().position(|&id| id == node) else { return false };
        if idx == 0 {
            return false;
        }
        list.swap(idx, idx - 1);
        true
    }

    /// Moves `node` to the end of its draw-order list ("Bring to Front" —
    /// later entries draw on top, per `root_nodes`'s own doc comment).
    /// Always actually removes-and-reinserts (even a no-op "already last"
    /// case), so it always returns a real prior index for
    /// `Command::BringNodeToFront`'s inverse (`SetNodeIndex`) to restore —
    /// same "harmless idempotent inverse" convention `Command::ReparentNode`
    /// already uses for "reparent to the same parent it already had".
    pub fn bring_node_to_front(&mut self, node: NodeId) -> usize {
        let list = self.ordering_list_mut(node).expect("Artboard::bring_node_to_front: node not found");
        let idx = list.iter().position(|&id| id == node).expect("node must be in its own ordering list");
        let id = list.remove(idx);
        list.push(id);
        idx
    }

    /// `bring_node_to_front`'s mirror — moves to the start of the list
    /// ("Send to Back").
    pub fn send_node_to_back(&mut self, node: NodeId) -> usize {
        let list = self.ordering_list_mut(node).expect("Artboard::send_node_to_back: node not found");
        let idx = list.iter().position(|&id| id == node).expect("node must be in its own ordering list");
        let id = list.remove(idx);
        list.insert(0, id);
        idx
    }

    /// Moves `node` to an exact position within its current ordering list,
    /// clamped to the list's (post-removal) bounds — `Command::SetNodeIndex`'s
    /// only use, restoring "Bring to Front"/"Send to Back"'s own inverse.
    pub fn set_node_index(&mut self, node: NodeId, index: usize) {
        let Some(list) = self.ordering_list_mut(node) else { return };
        let Some(idx) = list.iter().position(|&id| id == node) else { return };
        let id = list.remove(idx);
        let clamped = index.min(list.len());
        list.insert(clamped, id);
    }

    /// Every node id in the artboard, depth-first, parent-before-children,
    /// in draw order (`root_nodes` for the top level, `children_of` — which
    /// already prefers `child_order` when present — for each nesting level)
    /// — what `shapes`/`texts` actually walk so a `MoveNodeForward`/
    /// `BringNodeToFront`/etc. edit has a real, visible effect on what draws
    /// on top of what, not just on the Layers panel's own listing order.
    /// Bounded (`MAX_ANCESTOR_DEPTH` iterations of the explicit stack, not
    /// recursion) for the same cyclic-parent-chain reason `world_transform`
    /// is — this walks *down* the tree via `children_of` rather than up via
    /// `Node::parent`, but a cycle would hang it identically.
    fn ordered_node_ids(&self) -> Vec<NodeId> {
        let mut result = Vec::with_capacity(self.nodes.len());
        let mut stack: Vec<NodeId> = self.sorted_children(None).into_iter().rev().collect();
        // A genuine tree visits every node exactly once, so this many pops
        // is already generous; a cyclic parent chain (which nothing in this
        // codebase is supposed to ever produce) would otherwise keep this
        // stack growing forever instead of just draw-ordering one subtree
        // wrong.
        for _ in 0..=self.nodes.len() {
            let Some(id) = stack.pop() else { break };
            result.push(id);
            let children = self.sorted_children(Some(id));
            stack.extend(children.into_iter().rev());
        }
        result
    }

    /// `parent`'s own children (`root_nodes` for `None`), resolved to this
    /// frame's actual draw order — `children_of`'s/`root_nodes`' authored
    /// order, re-sorted by descending world Y if `parent` is a `Group` with
    /// `GroupNode::y_sort_enabled` (see that field's own doc comment for why
    /// a dynamic, position-driven order is a separate mechanism), else
    /// stable-sorted by `Node::z_index` (`sort_by` is a stable sort, so an
    /// all-default-`0.0` scene — everything before this field existed —
    /// keeps exactly its prior authored order; only an actually-authored
    /// z-index changes anything). The one place both ordering mechanisms are
    /// applied, so `ordered_node_ids` itself stays a plain, unordered-aware
    /// DFS walk over whatever this returns.
    pub fn sorted_children(&self, parent: Option<NodeId>) -> Vec<NodeId> {
        let mut children: Vec<NodeId> = match parent {
            None => self.root_nodes.clone(),
            Some(id) => self.children_of(id).map(|(cid, _)| cid).collect(),
        };
        let y_sort = parent.and_then(|id| self.node(id)).is_some_and(|node| matches!(&node.kind, NodeKind::Group(g) if g.y_sort_enabled));
        if y_sort {
            children.sort_by(|&a, &b| {
                let ya = self.try_world_transform(a).map(|t| t.translation.y).unwrap_or(0.0);
                let yb = self.try_world_transform(b).map(|t| t.translation.y).unwrap_or(0.0);
                yb.partial_cmp(&ya).unwrap_or(std::cmp::Ordering::Equal)
            });
        } else {
            children.sort_by(|&a, &b| {
                let za = self.node(a).map(|n| n.z_index).unwrap_or(0.0);
                let zb = self.node(b).map(|n| n.z_index).unwrap_or(0.0);
                za.partial_cmp(&zb).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        children
    }

    /// One O(N) pass that answers the two questions a full scene walk asks
    /// per node — "your children, in draw order" and "are you some group's
    /// mask" — without `children_of`'s / `is_mask_target`'s per-call scan of
    /// every node (which made walking a big scene O(N²)). Build once per
    /// frame, then query freely; same ordering rules as `sorted_children`.
    pub fn scene_index(&self) -> SceneIndex {
        let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        let mut mask_targets = std::collections::HashSet::new();
        for (id, node) in self.nodes.iter() {
            if let Some(parent) = node.parent {
                // Arena order — exactly `children_of`'s fallback order.
                if !self.child_order.contains_key(&parent) {
                    children.entry(parent).or_default().push(id);
                }
            }
            if let NodeKind::Group(group) = &node.kind {
                if let Some(mask) = group.mask {
                    mask_targets.insert(mask);
                }
            }
        }
        for (parent, order) in &self.child_order {
            children.insert(*parent, order.iter().copied().filter(|cid| self.nodes.get(*cid).is_some()).collect());
        }
        let mut roots = self.root_nodes.clone();
        self.sort_draw_order(None, &mut roots);
        let parents: Vec<NodeId> = children.keys().copied().collect();
        for parent in parents {
            let mut list = children.remove(&parent).unwrap_or_default();
            self.sort_draw_order(Some(parent), &mut list);
            children.insert(parent, list);
        }
        SceneIndex { children, roots, mask_targets }
    }

    /// `sorted_children`'s ordering rule, with each sort key computed once
    /// per child (not per comparison).
    fn sort_draw_order(&self, parent: Option<NodeId>, list: &mut Vec<NodeId>) {
        let y_sort = parent.and_then(|id| self.node(id)).is_some_and(|node| matches!(&node.kind, NodeKind::Group(g) if g.y_sort_enabled));
        let mut keyed: Vec<(f32, NodeId)> = list
            .iter()
            .map(|&id| {
                let key = if y_sort {
                    -self.try_world_transform(id).map(|t| t.translation.y).unwrap_or(0.0)
                } else {
                    self.node(id).map(|n| n.z_index).unwrap_or(0.0)
                };
                (key, id)
            })
            .collect();
        // Stable, like `sorted_children`.
        keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        list.clear();
        list.extend(keyed.into_iter().map(|(_, id)| id));
    }

    /// `true` if `id` or *any* of its ancestors has `Node::deleted` set —
    /// what actually determines whether a node is visible/hittable, now that
    /// nodes can nest under a `Group` (`Node::parent`). Soft-deleting a group
    /// (`Command::SetDeleted { value: true }`) only ever flips *that one
    /// node's* own flag — it doesn't (and shouldn't) cascade into flipping
    /// every descendant's flag too, since undoing the delete then only has
    /// to restore that one flag for the whole subtree to reappear correctly,
    /// rather than needing a whole batch of `SetDeleted`s kept in sync with
    /// the tree's shape. But that means checking only `node.deleted` itself
    /// (as `shapes`/`texts` used to) misses "deleted because an ancestor is
    /// deleted" entirely: a deleted group's children keep `deleted: false`
    /// forever, so a flat per-node check still renders/hit-tests them as
    /// live, orphaned content — exactly the "deleted the group, its shapes
    /// still draw" bug this method exists to fix. `pub`, not private, since
    /// `NodeKind::Instance` rendering (`editor.rs`'s `render`, which walks
    /// `node_ids()` directly rather than through `shapes`/`texts` — there's
    /// no `Artboard::instances()` helper) needs the same check and lives in
    /// a different crate.
    pub fn is_effectively_deleted(&self, id: NodeId) -> bool {
        let mut cursor = Some(id);
        // Bounds the walk against a cyclic parent chain — nothing that
        // constructs a `Node::parent` link is supposed to ever produce one
        // (`Command::ReparentNode`'s caller in `editor.rs` explicitly walks
        // the ancestor chain to reject exactly this before issuing the
        // command), but this runs from `shapes()`/`texts()`, which `render()`
        // calls for *every* shape on *every* frame — an unbounded loop here
        // would hang the whole render loop (and, since wasm calls run
        // synchronously on the JS thread, the whole UI) solid the moment any
        // cycle ever slipped through, instead of just rendering one node
        // wrong. `MAX_ANCESTOR_DEPTH` is generously above any real scene's
        // nesting depth.
        for _ in 0..MAX_ANCESTOR_DEPTH {
            let Some(current) = cursor else { return false };
            let Some(node) = self.node(current) else { return false };
            if node.deleted {
                return true;
            }
            cursor = node.parent;
        }
        false
    }

    /// `true` if `id` is *any* `Group`'s current `GroupNode::mask` target —
    /// a plain linear scan over every `Group` node, fine at the node counts
    /// this engine deals with. A mask's geometry exists purely to clip its
    /// parent group's other contents (`Renderer::render_with_mask`'s own
    /// doc comment) — nothing composites that clip into the normal
    /// multi-object scene this artboard's own flat `shapes`/`texts`
    /// iteration draws yet, so a mask node has no business appearing there
    /// as if it were ordinary visible content. Before this existed, a mask
    /// shape rendered (and hit-tested) as a perfectly normal, fully opaque
    /// shape sitting on top of/around whatever it was supposed to be
    /// clipping — often *worse* than doing nothing, since a clip shape is
    /// frequently authored with a throwaway fill (or none at all, which
    /// resolves to whatever ambient fill happens to be in scope) that was
    /// never meant to be seen.
    pub fn is_mask_target(&self, id: NodeId) -> bool {
        self.nodes.values().any(|node| matches!(&node.kind, NodeKind::Group(group) if group.mask == Some(id)))
    }

    /// Iterates every non-deleted, non-mask `NodeKind::Shape` node in the
    /// artboard, root or not, in actual draw order (`ordered_node_ids` —
    /// root-to-leaf, respecting `Command::MoveNodeForward`/`BringNodeToFront`/
    /// etc.'s reordering) — this is what the renderer walks to build each
    /// frame's `DrawCall`s. Still doesn't account for grouping/masking
    /// composition (Phase 10's `GroupNode`) beyond flattening every shape
    /// into one draw list, and still can't interleave with `texts()`/
    /// instance draws (the render loop draws all shapes, then all instances,
    /// then all texts, as three separate concatenated passes) — reordering
    /// only ever visibly moves a node relative to others *of the same kind*
    /// until that partition itself is unified into one pass. Skips any node
    /// that's itself deleted *or* nested under a deleted ancestor
    /// (`is_effectively_deleted`) — see `Command::SetDeleted`'s doc comment
    /// for why deletion is a flag, not real removal, and why every render/UI-
    /// facing listing has to filter it out itself — and skips a mask target
    /// too (`is_mask_target`), for the reasons that method's doc comment gives.
    pub fn shapes(&self) -> impl Iterator<Item = (NodeId, &Node)> + '_ {
        self.ordered_node_ids()
            .into_iter()
            .filter(|&id| !self.is_effectively_deleted(id) && !self.is_mask_target(id))
            .filter_map(|id| self.node(id).map(|n| (id, n)))
            .filter(|(_, node)| matches!(node.kind, NodeKind::Shape(_)))
    }

    /// Same scope note as [`Artboard::shapes`], for `NodeKind::Text` nodes —
    /// Phase 3's counterpart to Phase 1's shape-walking helper.
    pub fn texts(&self) -> impl Iterator<Item = (NodeId, &Node)> + '_ {
        self.ordered_node_ids()
            .into_iter()
            .filter(|&id| !self.is_effectively_deleted(id) && !self.is_mask_target(id))
            .filter_map(|id| self.node(id).map(|n| (id, n)))
            .filter(|(_, node)| matches!(node.kind, NodeKind::Text(_)))
    }

    /// Same scope note as [`Artboard::shapes`], for `NodeKind::Image` nodes.
    pub fn images(&self) -> impl Iterator<Item = (NodeId, &Node)> + '_ {
        self.ordered_node_ids()
            .into_iter()
            .filter(|&id| !self.is_effectively_deleted(id) && !self.is_mask_target(id))
            .filter_map(|id| self.node(id).map(|n| (id, n)))
            .filter(|(_, node)| matches!(node.kind, NodeKind::Image(_)))
    }

    /// Every node in the artboard, *any* kind, in real draw order — the
    /// unified, cross-kind counterpart to `shapes`/`texts`/`images` above,
    /// for a caller that needs to hit-test topmost-first regardless of node
    /// kind (`WasmEditor::pick_node_at`). Checking `images()` then
    /// `shapes()` as two separate passes (this method's reason for
    /// existing) meant an `Image` always won picking over a `Shape`
    /// regardless of which one actually draws on top — a real bug once
    /// `Shape`/`Image`/etc. can be freely nested/reordered relative to each
    /// other, not just within their own kind. Same deleted/mask-target
    /// filtering as `shapes`/`texts`/`images`.
    pub fn pickable_nodes(&self) -> impl Iterator<Item = (NodeId, &Node)> + '_ {
        self.ordered_node_ids()
            .into_iter()
            .filter(|&id| !self.is_effectively_deleted(id) && !self.is_mask_target(id))
            .filter_map(|id| self.node(id).map(|n| (id, n)))
    }

    pub fn add_animation(&mut self, animation: Animation) -> AnimationId {
        self.animations.insert(animation)
    }

    pub fn animation(&self, id: AnimationId) -> Option<&Animation> {
        self.animations.get(id)
    }

    pub fn animation_mut(&mut self, id: AnimationId) -> Option<&mut Animation> {
        self.animations.get_mut(id)
    }

    /// Every non-deleted animation, for the editor's animation dropdown —
    /// same `Node::deleted`-skipping convention as `shapes`/`texts`.
    pub fn animations(&self) -> impl Iterator<Item = (AnimationId, &Animation)> {
        self.animations.iter().filter(|(_, animation)| !animation.deleted)
    }

    /// Every animation regardless of `deleted`, mutably — for
    /// `Command::ShiftPropertyKeyframes`, which rebases a node's keyframed
    /// path in *every* animation that keys it (not just the currently
    /// active one), including deleted ones (harmless — nothing reads a
    /// deleted animation's tracks, and shifting them costs nothing anyone
    /// notices, but skipping them would need extra bookkeeping for no
    /// benefit). Same "reach everything regardless" shape as `Artboard::node_ids`.
    pub fn animations_mut(&mut self) -> impl Iterator<Item = (AnimationId, &mut Animation)> {
        self.animations.iter_mut()
    }

    /// Adds a state machine definition, returning the id it can be looked up
    /// by. No soft-delete flag yet (unlike `Animation`/`Node`) — nothing
    /// authors or deletes one through `Command` yet, so there's nothing that
    /// needs a stable id to survive an undo; add one if/when a `Command`
    /// actually removes a state machine.
    pub fn add_state_machine(&mut self, state_machine: StateMachineDef) -> StateMachineId {
        self.state_machines.insert(state_machine)
    }

    pub fn state_machine(&self, id: StateMachineId) -> Option<&StateMachineDef> {
        self.state_machines.get(id)
    }

    pub fn state_machine_mut(&mut self, id: StateMachineId) -> Option<&mut StateMachineDef> {
        self.state_machines.get_mut(id)
    }

    /// Every non-deleted state machine defined on this artboard, for a
    /// host/editor that wants to list them by name (AN1MBYT3_BUILD_PLAN.md
    /// §8's `animation.stateMachine("Character")` needs to resolve a name to
    /// one of these) — same `deleted`-skipping convention as `animations()`.
    pub fn state_machines(&self) -> impl Iterator<Item = (StateMachineId, &StateMachineDef)> {
        self.state_machines.iter().filter(|(_, sm)| !sm.deleted)
    }

    /// A node's *world* transform: its own local transform composed through
    /// every ancestor's, up to the root (AN1MBYT3_BUILD_PLAN.md §10 Phase
    /// 5's first real consumer of `Node::parent` beyond `root_nodes()`
    /// listing — a bone chain's second bone is parented to the first, and
    /// its actual on-screen position/rotation depends on both).
    ///
    /// Walks up via `Transform2D::compose` rather than requiring a
    /// `children` list to walk down — see `node.rs`'s note on why `parent`
    /// is the only direction this graph is stored in. Iterative (collects
    /// the chain of ancestor transforms, then composes root-to-leaf), not a
    /// direct recursive walk, specifically to stay bounded (`MAX_ANCESTOR_DEPTH`)
    /// against a cyclic parent chain instead of recursing forever/overflowing
    /// the stack — this is called from `render()`'s per-shape, per-frame
    /// path, so an unbounded walk into a cycle would hang the whole render
    /// loop (and, since wasm calls run synchronously on the JS thread, the
    /// whole UI) rather than just render one node wrong.
    ///
    /// Panics if `id` doesn't exist in this artboard — same "caller bug, not
    /// a recoverable condition" convention as `Command::apply`.
    pub fn world_transform(&self, id: NodeId) -> Transform2D {
        self.try_world_transform(id).expect("Artboard::world_transform: node not found")
    }

    /// Same walk as `world_transform`, but returns `None` instead of
    /// panicking if `id` — or any ancestor above it — doesn't resolve,
    /// rather than only checking `id` itself. For a selection-driven caller
    /// (the frontend's current selection, not a guaranteed-valid invariant
    /// the way iterating `shapes()`/`texts()` is — those ids always resolve
    /// by construction), a stale id can genuinely happen (e.g. left over
    /// from before a document reload swapped in a brand-new artboard), and
    /// this runs from `render()`'s own per-frame path — a panic there would
    /// throw across the wasm boundary and permanently kill the caller's
    /// `requestAnimationFrame` loop, not just mis-render one frame. Use
    /// `world_transform` only where every id involved is already known-valid
    /// (draw-call construction over `shapes()`/`node_ids()`); use this for
    /// anything sourced from outside that guarantee.
    pub fn try_world_transform(&self, id: NodeId) -> Option<Transform2D> {
        let mut chain = Vec::new();
        let mut cursor = Some(id);
        for _ in 0..MAX_ANCESTOR_DEPTH {
            let Some(current) = cursor else { break };
            let node = self.node(current)?;
            chain.push(node.transform);
            cursor = node.parent;
        }
        let mut result = chain.pop()?;
        while let Some(transform) = chain.pop() {
            result = result.compose(&transform);
        }
        Some(result)
    }

    /// `id`'s transform expressed relative to `ancestor` instead of the
    /// artboard root — the same parent-chain composition `world_transform`
    /// does (including the same bounded-walk reasoning — see its doc
    /// comment), just stopping at `ancestor` rather than continuing all the
    /// way up. Doesn't verify `ancestor` actually *is* an ancestor of `id`
    /// (same "caller bug, not recoverable" convention as `world_transform`)
    /// — if it isn't, this silently walks past it to the real root instead,
    /// same as `world_transform` would. Used by `dto::summarize_document`'s
    /// `Group` bounding-box computation: showing a group's *actual content*
    /// extent (not a fixed placeholder box, see that function's own doc
    /// comment) means expressing every descendant shape's geometry in the
    /// group's own coordinate frame, not the whole artboard's.
    pub fn transform_relative_to(&self, id: NodeId, ancestor: NodeId) -> Transform2D {
        let mut chain = Vec::new();
        let mut cursor = Some(id);
        for _ in 0..MAX_ANCESTOR_DEPTH {
            let Some(current) = cursor else { break };
            let node = self.node(current).expect("Artboard::transform_relative_to: node not found");
            chain.push(node.transform);
            if node.parent == Some(ancestor) {
                break;
            }
            cursor = node.parent;
        }
        let mut result = chain.pop().expect("transform_relative_to: chain is never empty, `id` itself always pushes once");
        while let Some(transform) = chain.pop() {
            result = result.compose(&transform);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_child_added_into_a_reused_slot_still_draws_last() {
        use crate::node::GroupNode;
        let mut a = Artboard::new("Main", 10.0, 10.0);
        let parent = a.add_node(Node::new("P", None, NodeKind::Group(GroupNode::default())));
        // A slot freed *before* `first`'s: the next insert reuses it, so in
        // plain arena order the newer child would sort first.
        let gone = a.add_node(Node::new("Tmp", None, NodeKind::Group(GroupNode::default())));
        let first = a.add_node(Node::new("A", Some(parent), NodeKind::Group(GroupNode::default())));
        a.despawn(gone);
        let second = a.add_node(Node::new("B", Some(parent), NodeKind::Group(GroupNode::default())));
        let order: Vec<NodeId> = a.children_of(parent).map(|(id, _)| id).collect();
        assert_eq!(order, [first, second]);
    }

    #[test]
    fn scene_index_matches_sorted_children_and_mask_targets() {
        use crate::{GroupNode, Node};
        let mut artboard = Artboard::new("index", 100.0, 100.0);
        let group = artboard.add_node(Node::new("G", None, NodeKind::Group(GroupNode { y_sort_enabled: true, ..GroupNode::default() })));
        let mut kids = Vec::new();
        for (i, y) in [5.0f32, -3.0, 9.0, 0.0].into_iter().enumerate() {
            let mut n = Node::new(format!("k{i}"), Some(group), NodeKind::Group(GroupNode::default()));
            n.transform.translation.y = y;
            kids.push(artboard.add_node(n));
        }
        let mut z = Node::new("z", None, NodeKind::Group(GroupNode::default()));
        z.z_index = -1.0;
        let z = artboard.add_node(z);
        let masker = artboard.add_node(Node::new("M", None, NodeKind::Group(GroupNode { mask: Some(kids[1]), ..GroupNode::default() })));
        let index = artboard.scene_index();
        assert_eq!(index.children(None), artboard.sorted_children(None).as_slice());
        assert_eq!(index.children(Some(group)), artboard.sorted_children(Some(group)).as_slice());
        assert_eq!(index.children(None)[0], z);
        assert!(index.is_mask_target(kids[1]) && !index.is_mask_target(masker));
        artboard.move_node_forward(kids[0]);
        let index = artboard.scene_index();
        assert_eq!(index.children(Some(group)), artboard.sorted_children(Some(group)).as_slice());
    }

    use super::*;
    use crate::node::{BoneNode, GroupNode, Node, NodeKind};
    use glam::Vec2;

    #[test]
    fn world_transform_of_a_root_node_is_its_own_transform() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let id = artboard.add_node(Node::new("Bone", None, NodeKind::Bone(BoneNode { length: 50.0 })));
        artboard.node_mut(id).unwrap().transform.translation = Vec2::new(10.0, 20.0);

        assert_eq!(artboard.world_transform(id).translation, Vec2::new(10.0, 20.0));
    }

    #[test]
    fn add_node_auto_assigns_a_serial_id_when_none_was_given() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let first = artboard.add_node(Node::new("Bone", None, NodeKind::Bone(BoneNode { length: 10.0 })));
        let second = artboard.add_node(Node::new("Bone", None, NodeKind::Bone(BoneNode { length: 10.0 })));

        assert_eq!(artboard.node(first).unwrap().id, "shape(0)");
        assert_eq!(artboard.node(second).unwrap().id, "shape(1)");
    }

    #[test]
    fn add_node_leaves_a_custom_id_alone() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let mut node = Node::new("Bone", None, NodeKind::Bone(BoneNode { length: 10.0 }));
        node.id = "myBone".into();
        let id = artboard.add_node(node);

        assert_eq!(artboard.node(id).unwrap().id, "myBone");
    }

    #[test]
    fn node_ids_yields_every_node_regardless_of_kind_or_deleted_state() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let bone_id = artboard.add_node(Node::new("Bone", None, NodeKind::Bone(crate::node::BoneNode { length: 10.0 })));
        let shape_id = artboard.add_node(Node::new(
            "Square",
            None,
            NodeKind::Shape(crate::node::ShapeNode { path: crate::path::Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
        ));
        artboard.node_mut(shape_id).unwrap().deleted = true;

        let mut found: Vec<NodeId> = artboard.node_ids().collect();
        found.sort_by_key(|id| format!("{id:?}"));
        let mut expected = vec![bone_id, shape_id];
        expected.sort_by_key(|id| format!("{id:?}"));
        assert_eq!(found, expected);
    }

    #[test]
    fn z_index_reorders_siblings_ascending_with_ties_keeping_authored_order() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let a = artboard.add_node(Node::new("A", None, NodeKind::Bone(BoneNode { length: 1.0 })));
        let b = artboard.add_node(Node::new("B", None, NodeKind::Bone(BoneNode { length: 1.0 })));
        let c = artboard.add_node(Node::new("C", None, NodeKind::Bone(BoneNode { length: 1.0 })));
        // Authored order is A, B, C; give C the lowest z-index so it draws
        // first (behind), leaving A/B tied at 0.0 — a stable sort keeps
        // their own relative authored order unchanged.
        artboard.node_mut(c).unwrap().z_index = -1.0;

        assert_eq!(artboard.ordered_node_ids(), vec![c, a, b]);
    }

    #[test]
    fn y_sort_enabled_group_orders_children_by_descending_world_y() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let group = artboard.add_node(Node::new("Sorted", None, NodeKind::Group(GroupNode { mask: None, y_sort_enabled: true })));
        let low = artboard.add_node(Node::new("Low", Some(group), NodeKind::Bone(BoneNode { length: 1.0 })));
        let high = artboard.add_node(Node::new("High", Some(group), NodeKind::Bone(BoneNode { length: 1.0 })));
        artboard.node_mut(low).unwrap().transform.translation = Vec2::new(0.0, -10.0);
        artboard.node_mut(high).unwrap().transform.translation = Vec2::new(0.0, 10.0);

        // Larger world Y (higher/further back in this +Y-up engine) draws
        // first; smaller Y (lower/nearer the viewer) draws last, on top —
        // regardless of insertion order (`low` was added before `high`).
        assert_eq!(artboard.ordered_node_ids(), vec![group, high, low]);
    }

    #[test]
    fn texts_yields_only_text_nodes() {
        use crate::{document::Document, font_asset::FontAsset, node::TextNode};

        let mut document = Document::new();
        let font_id = document.add_font(FontAsset { name: "Demo".into(), data: Vec::new() });

        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        artboard.add_node(Node::new(
            "Square",
            None,
            NodeKind::Shape(crate::node::ShapeNode { path: crate::path::Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
        ));
        let text_id = artboard.add_node(Node::new(
            "Label",
            None,
            NodeKind::Text(TextNode { content: "Hi".into(), font: font_id, size_px: 16.0, fill_color: [1.0; 4], gradient: None }),
        ));

        let found: Vec<NodeId> = artboard.texts().map(|(id, _)| id).collect();
        assert_eq!(found, vec![text_id]);
    }

    #[test]
    fn world_transform_composes_through_a_parent_chain() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);

        let root = artboard.add_node(Node::new("Root", None, NodeKind::Bone(BoneNode { length: 50.0 })));
        artboard.node_mut(root).unwrap().transform.translation = Vec2::new(100.0, 0.0);

        let child = artboard.add_node(Node::new(
            "Child",
            Some(root),
            NodeKind::Bone(BoneNode { length: 30.0 }),
        ));
        artboard.node_mut(child).unwrap().transform.translation = Vec2::new(50.0, 0.0);

        // Child's local offset (50, 0) is carried through the root's
        // transform (translated by (100, 0)) — world position (150, 0).
        assert_eq!(artboard.world_transform(child).translation, Vec2::new(150.0, 0.0));
    }

    #[test]
    fn transform_relative_to_stops_composing_at_the_given_ancestor() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);

        let grandparent = artboard.add_node(Node::new("Grandparent", None, NodeKind::Bone(BoneNode { length: 10.0 })));
        artboard.node_mut(grandparent).unwrap().transform.translation = Vec2::new(1000.0, 0.0);

        let parent = artboard.add_node(Node::new("Parent", Some(grandparent), NodeKind::Bone(BoneNode { length: 10.0 })));
        artboard.node_mut(parent).unwrap().transform.translation = Vec2::new(100.0, 0.0);

        let child = artboard.add_node(Node::new("Child", Some(parent), NodeKind::Bone(BoneNode { length: 10.0 })));
        artboard.node_mut(child).unwrap().transform.translation = Vec2::new(10.0, 0.0);

        // Relative to the direct parent, nothing composes yet — just the
        // child's own local transform.
        assert_eq!(artboard.transform_relative_to(child, parent).translation, Vec2::new(10.0, 0.0));

        // Relative to the grandparent, parent's own local transform composes
        // with child's — grandparent's own transform must not factor in.
        assert_eq!(artboard.transform_relative_to(child, grandparent).translation, Vec2::new(110.0, 0.0));

        // An `ancestor` that never actually appears in the chain falls
        // through to the true root exactly like `world_transform` would.
        let unrelated = artboard.add_node(Node::new("Unrelated", None, NodeKind::Bone(BoneNode { length: 1.0 })));
        assert_eq!(artboard.transform_relative_to(child, unrelated).translation, artboard.world_transform(child).translation);
    }

    fn sample_shape(name: &str, parent: Option<NodeId>) -> Node {
        Node::new(
            name,
            parent,
            NodeKind::Shape(crate::node::ShapeNode { path: crate::path::Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
        )
    }

    #[test]
    fn deleting_a_group_hides_its_children_from_shapes_without_flipping_their_own_flag() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let group_id = artboard.add_node(Node::new("Group", None, NodeKind::Group(Default::default())));
        let child_id = artboard.add_node(sample_shape("Child", Some(group_id)));

        assert_eq!(artboard.shapes().map(|(id, _)| id).collect::<Vec<_>>(), vec![child_id]);

        // Soft-deleting the *group* only flips the group's own flag — this
        // is the one-flag-flip, no-cascade design `is_effectively_deleted`'s
        // doc comment describes.
        artboard.node_mut(group_id).unwrap().deleted = true;
        assert!(!artboard.node(child_id).unwrap().deleted, "the child's own flag must be untouched");

        // But it must still disappear from `shapes()` — an ancestor being
        // deleted is what actually matters, not just the node's own flag
        // (the "deleted the group, its shapes still render" bug).
        assert_eq!(artboard.shapes().map(|(id, _)| id).collect::<Vec<_>>(), Vec::<NodeId>::new());
        assert!(artboard.is_effectively_deleted(child_id));

        // Restoring just the group's flag brings the whole subtree back —
        // no batch of per-child undo needed.
        artboard.node_mut(group_id).unwrap().deleted = false;
        assert_eq!(artboard.shapes().map(|(id, _)| id).collect::<Vec<_>>(), vec![child_id]);
    }

    #[test]
    fn deleting_a_deeply_nested_ancestor_still_hides_a_grandchild() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let outer = artboard.add_node(Node::new("Outer", None, NodeKind::Group(Default::default())));
        let inner = artboard.add_node(Node::new("Inner", Some(outer), NodeKind::Group(Default::default())));
        let leaf = artboard.add_node(sample_shape("Leaf", Some(inner)));

        artboard.node_mut(outer).unwrap().deleted = true;
        assert!(artboard.is_effectively_deleted(leaf), "a grandchild of a deleted ancestor must count as deleted too");
        assert!(artboard.shapes().next().is_none());
    }

    #[test]
    fn a_groups_mask_target_is_excluded_from_shapes_but_still_a_real_node() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let group_id = artboard.add_node(Node::new("Group", None, NodeKind::Group(Default::default())));
        let visible_id = artboard.add_node(sample_shape("Visible", Some(group_id)));
        let mask_id = artboard.add_node(sample_shape("Mask", Some(group_id)));

        // Before a mask is assigned, both children draw normally.
        let mut before: Vec<NodeId> = artboard.shapes().map(|(id, _)| id).collect();
        before.sort_by_key(|id| format!("{id:?}"));
        let mut expected_before = vec![visible_id, mask_id];
        expected_before.sort_by_key(|id| format!("{id:?}"));
        assert_eq!(before, expected_before);

        let NodeKind::Group(group) = &mut artboard.node_mut(group_id).unwrap().kind else { unreachable!() };
        group.mask = Some(mask_id);

        assert!(artboard.is_mask_target(mask_id));
        assert!(!artboard.is_mask_target(visible_id));
        assert_eq!(artboard.shapes().map(|(id, _)| id).collect::<Vec<_>>(), vec![visible_id], "the mask shape must not render as if it were ordinary content");
        // Still a real, addressable node (Layers panel, mask badge, etc.) —
        // only excluded from the *drawing* iteration.
        assert!(artboard.node(mask_id).is_some());
    }

    /// Nothing that constructs a `Node::parent` link is supposed to ever
    /// produce a cycle, but these three walks all run from `render()`'s
    /// per-shape, per-frame path — if one ever slipped through anyway
    /// (a bug elsewhere, a hand-edited `.tea` file), an *unbounded* walk
    /// would hang the whole render loop solid instead of just rendering one
    /// node wrong. This constructs an actual two-node cycle directly (bypassing
    /// `reparent_node`'s own guard, which is the thing that's supposed to
    /// prevent this from ever happening for real) and asserts every walk
    /// that could hit it returns promptly instead of hanging or overflowing.
    #[test]
    fn a_cyclic_parent_chain_does_not_hang_any_ancestor_walk() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let a = artboard.add_node(sample_shape("A", None));
        let b = artboard.add_node(sample_shape("B", Some(a)));
        // Force an actual cycle: A's parent becomes B, so A -> B -> A -> ...
        artboard.node_mut(a).unwrap().parent = Some(b);

        // None of these must hang — that's the whole point of the test;
        // reaching this assertion at all is the pass condition for the first
        // two. The exact returned value for a cycle is unspecified/best-effort.
        let _ = artboard.is_effectively_deleted(a);
        let _ = artboard.world_transform(a);
        let _ = artboard.transform_relative_to(a, NodeId::default());
        // `shapes()`/`texts()` now walk `ordered_node_ids()`, a brand-new
        // down-the-tree DFS — the one ancestor-walk-shaped thing here that
        // predates this test, so cover it explicitly rather than trusting
        // the up-the-tree walks above to stand in for it.
        let _: Vec<_> = artboard.shapes().collect();
    }

    #[test]
    fn move_node_forward_and_backward_swap_with_the_adjacent_sibling_and_are_no_ops_at_the_ends() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let a = artboard.add_node(sample_shape("A", None));
        let b = artboard.add_node(sample_shape("B", None));
        let c = artboard.add_node(sample_shape("C", None));
        assert_eq!(artboard.shapes().map(|(id, _)| id).collect::<Vec<_>>(), vec![a, b, c]);

        assert!(artboard.move_node_forward(a));
        assert_eq!(artboard.shapes().map(|(id, _)| id).collect::<Vec<_>>(), vec![b, a, c]);

        assert!(artboard.move_node_backward(c));
        assert_eq!(artboard.shapes().map(|(id, _)| id).collect::<Vec<_>>(), vec![b, c, a]);

        // `a` is now last — moving it forward again is a no-op, not a panic
        // or a wraparound.
        assert!(!artboard.move_node_forward(a));
        assert_eq!(artboard.shapes().map(|(id, _)| id).collect::<Vec<_>>(), vec![b, c, a]);

        // `b` is now first — moving it backward again is a no-op.
        assert!(!artboard.move_node_backward(b));
        assert_eq!(artboard.shapes().map(|(id, _)| id).collect::<Vec<_>>(), vec![b, c, a]);
    }

    #[test]
    fn bring_to_front_and_send_to_back_move_to_the_ends_and_report_the_prior_index() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let a = artboard.add_node(sample_shape("A", None));
        let b = artboard.add_node(sample_shape("B", None));
        let c = artboard.add_node(sample_shape("C", None));

        let prior = artboard.bring_node_to_front(a);
        assert_eq!(prior, 0);
        assert_eq!(artboard.shapes().map(|(id, _)| id).collect::<Vec<_>>(), vec![b, c, a]);

        let prior = artboard.send_node_to_back(a);
        assert_eq!(prior, 2);
        assert_eq!(artboard.shapes().map(|(id, _)| id).collect::<Vec<_>>(), vec![a, b, c]);

        // `set_node_index` (what the two above report as their own undo)
        // restores an exact position, not just "one step".
        artboard.set_node_index(c, 0);
        assert_eq!(artboard.shapes().map(|(id, _)| id).collect::<Vec<_>>(), vec![c, a, b]);
    }

    #[test]
    fn reordering_within_a_group_changes_children_of_order_and_leaves_root_nodes_alone() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let group = artboard.add_node(Node::new("Group", None, NodeKind::Group(Default::default())));
        let a = artboard.add_node(sample_shape("A", Some(group)));
        let b = artboard.add_node(sample_shape("B", Some(group)));
        let c = artboard.add_node(sample_shape("C", Some(group)));

        assert!(artboard.move_node_backward(c));
        assert_eq!(artboard.children_of(group).map(|(id, _)| id).collect::<Vec<_>>(), vec![a, c, b]);
        assert_eq!(artboard.root_nodes(), &[group], "reordering a group's children must never touch root_nodes");
    }

    #[test]
    fn reparenting_between_two_groups_removes_from_the_old_childorder_and_appends_to_the_new() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let group_a = artboard.add_node(Node::new("Group A", None, NodeKind::Group(Default::default())));
        let group_b = artboard.add_node(Node::new("Group B", None, NodeKind::Group(Default::default())));
        let x = artboard.add_node(sample_shape("X", Some(group_a)));
        let y = artboard.add_node(sample_shape("Y", Some(group_a)));
        // Materialize `child_order` for both groups (a fresh group with
        // never-reordered children has no entry yet — see
        // `ordering_list_mut`'s doc comment) so this test actually exercises
        // the bookkeeping, not just the arena-scan fallback.
        artboard.move_node_forward(x);
        artboard.move_node_forward(y);

        artboard.reparent_node(x, Some(group_b));

        assert_eq!(artboard.children_of(group_a).map(|(id, _)| id).collect::<Vec<_>>(), vec![y]);
        assert_eq!(artboard.children_of(group_b).map(|(id, _)| id).collect::<Vec<_>>(), vec![x]);
    }

    #[test]
    fn a_node_added_into_an_already_reordered_groups_children_appends_after_them() {
        let mut artboard = Artboard::new("Main", 400.0, 400.0);
        let group = artboard.add_node(Node::new("Group", None, NodeKind::Group(Default::default())));
        let a = artboard.add_node(sample_shape("A", Some(group)));
        let b = artboard.add_node(sample_shape("B", Some(group)));
        artboard.move_node_forward(a); // materializes child_order for `group`, as [b, a]

        let c = artboard.add_node(sample_shape("C", Some(group)));
        assert_eq!(artboard.children_of(group).map(|(id, _)| id).collect::<Vec<_>>(), vec![b, a, c]);
    }
}

/// A frame's snapshot of the scene tree's draw order and mask targets —
/// see `Artboard::scene_index`.
#[derive(Debug, Default)]
pub struct SceneIndex {
    children: HashMap<NodeId, Vec<NodeId>>,
    roots: Vec<NodeId>,
    mask_targets: std::collections::HashSet<NodeId>,
}

impl SceneIndex {
    /// `parent`'s children in draw order (`None` = the root nodes).
    pub fn children(&self, parent: Option<NodeId>) -> &[NodeId] {
        match parent {
            None => &self.roots,
            Some(id) => self.children.get(&id).map_or(&[], |v| v.as_slice()),
        }
    }

    /// Whether `id` is some `Group`'s mask (never drawn as content).
    pub fn is_mask_target(&self, id: NodeId) -> bool {
        self.mask_targets.contains(&id)
    }
}
