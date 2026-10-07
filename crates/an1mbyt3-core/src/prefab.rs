//! Prefabs — Unity's reusable GameObject templates.
//!
//! A [`PrefabDefinition`] is a document-level asset (like an image or a
//! component) holding a whole node *subtree*: its hierarchy, every node's
//! kind, transform, behaviors (physics, lights, particles, scripts, joints…)
//! and visibility. Placing it in a scene ("instantiating") copies that
//! subtree in as ordinary, fully-featured scene nodes — so everything that
//! works on a hand-built object works on a prefab instance, with no special
//! render/physics/script path.
//!
//! What makes those copies an *instance* rather than a plain duplicate is
//! [`PrefabLink`]: every instantiated node remembers which prefab and which
//! prefab node (`uid`) it came from. That link is what powers the three
//! Unity workflows:
//!
//! - **Apply** — push one instance's current state back into the prefab
//!   (`PrefabDefinition::capture` with the instance's existing uids), then
//!   re-sync every other instance from it.
//! - **Revert** — re-sync one instance from the prefab, discarding its
//!   local changes (the root's own transform and name are kept: where an
//!   instance sits, and what it's called, are per-instance by nature).
//! - **Unpack** — drop the links; the nodes become plain scene nodes.
//!
//! ## Stable uids, not positions
//!
//! Prefab nodes are identified by a `uid` that never changes for the
//! lifetime of the prefab (`next_uid` only grows). Positional indices would
//! break on Apply: adding a child to one instance and applying it would
//! shift every later index and mis-match the *other* instances' links.
//!
//! ## Internal node references
//!
//! Some node data points at other nodes — a group's mask, a camera's follow
//! target, a hinge's connected body, an IK target, a skin's bones. Inside a
//! definition those are stored as *synthetic* `NodeId`s encoding the
//! target's prefab uid ([`uid_ref`]), and remapped to the new scene ids on
//! instantiate/sync ([`remap_node_refs`]). A reference to something outside
//! the captured subtree can't survive being copied elsewhere, so it is
//! dropped (`None`) — except skin bones, which are positional with their
//! bind poses and so are left untouched rather than shifting the list.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use slotmap::{Key, KeyData};

use crate::{Artboard, Node, NodeBehavior, NodeId, NodeKind, PrefabId};

/// Which prefab (and which node inside it) a scene node was instantiated
/// from — see the module doc. Stored on [`Node::prefab`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefabLink {
    pub prefab: PrefabId,
    /// The prefab node's stable [`PrefabNode::uid`]. The instance's root is
    /// whichever linked node has `uid == PrefabDefinition::root_uid()`.
    pub uid: u32,
}

/// One node of a prefab's subtree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefabNode {
    /// Stable identity within this prefab — see the module doc.
    pub uid: u32,
    /// The parent's `uid`; `None` only for the root (always `nodes[0]`).
    pub parent: Option<u32>,
    /// The node's full data. Its own `parent` field is unused (always
    /// `None` here — hierarchy lives in `PrefabNode::parent`), its `prefab`
    /// link is cleared, and node references are synthetic ([`uid_ref`]).
    pub node: Node,
}

/// A reusable node-subtree template — see the module doc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefabDefinition {
    pub name: String,
    /// Parents before children (depth-first), root first.
    pub nodes: Vec<PrefabNode>,
    /// Next uid to hand out — only ever grows, so a removed node's uid is
    /// never reused for a different node.
    pub next_uid: u32,
    /// Soft delete, same convention as `ComponentDefinition::deleted`
    /// (undo must still resolve the id). Instances of a deleted prefab keep
    /// working as plain nodes.
    #[serde(default)]
    pub deleted: bool,
}

/// Encodes a prefab uid as a synthetic `NodeId` (version 1, index = uid) —
/// how node references are stored inside a definition. Never resolves in a
/// real artboard because it is always remapped before use.
pub fn uid_ref(uid: u32) -> NodeId {
    NodeId::from(KeyData::from_ffi((1u64 << 32) | uid as u64))
}

/// Inverse of [`uid_ref`].
pub fn ref_uid(id: NodeId) -> u32 {
    id.data().as_ffi() as u32
}

/// Rewrites every node reference held by `node` through `map`. An optional
/// reference `map` can't resolve becomes `None`; skin bones it can't
/// resolve are left as-is (see the module doc).
pub fn remap_node_refs(node: &mut Node, map: &mut impl FnMut(NodeId) -> Option<NodeId>) {
    let opt = |slot: &mut Option<NodeId>, map: &mut dyn FnMut(NodeId) -> Option<NodeId>| {
        if let Some(id) = *slot {
            *slot = map(id);
        }
    };
    match &mut node.kind {
        NodeKind::Group(group) => opt(&mut group.mask, map),
        NodeKind::Camera(camera) => opt(&mut camera.follow_target, map),
        _ => {}
    }
    for entry in &mut node.behaviors {
        match &mut entry.behavior {
            NodeBehavior::Camera(camera) => opt(&mut camera.follow_target, map),
            NodeBehavior::HingeJoint2D(hinge) => opt(&mut hinge.connected_body, map),
            NodeBehavior::IkConstraint2D(ik) => opt(&mut ik.target, map),
            NodeBehavior::SpriteSkin2D(skin) => {
                for bone in &mut skin.bones {
                    if let Some(mapped) = map(*bone) {
                        *bone = mapped;
                    }
                }
            }
            _ => {}
        }
    }
}

impl PrefabDefinition {
    /// The root node's uid (`nodes[0]`).
    pub fn root_uid(&self) -> u32 {
        self.nodes.first().map_or(0, |n| n.uid)
    }

    pub fn node(&self, uid: u32) -> Option<&PrefabNode> {
        self.nodes.iter().find(|n| n.uid == uid)
    }

    /// Snapshots the live subtree under `root` (deleted nodes skipped) as a
    /// prefab named `name`.
    ///
    /// `existing`: when re-capturing an instance for **Apply**, the prefab it
    /// came from (`Some((id, previous definition))`) — nodes already linked
    /// to it keep their uid, so every other instance still matches up;
    /// anything new gets a fresh uid. `None` for a brand-new prefab.
    ///
    /// Returns the definition plus, for every captured scene node, the uid
    /// it was given — the caller links those scene nodes to the prefab.
    pub fn capture(artboard: &Artboard, root: NodeId, name: impl Into<String>, existing: Option<(PrefabId, &PrefabDefinition)>) -> (Self, Vec<(NodeId, u32)>) {
        let mut next_uid = existing.map_or(0, |(_, def)| def.next_uid);
        let mut order: Vec<NodeId> = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let Some(node) = artboard.node(id) else { continue };
            if node.deleted {
                continue;
            }
            order.push(id);
            // Reverse so children come out in their real order.
            let children: Vec<NodeId> = artboard.children_of(id).map(|(child, _)| child).collect();
            stack.extend(children.into_iter().rev());
        }

        let mut uids: HashMap<NodeId, u32> = HashMap::new();
        for &id in &order {
            let node = artboard.node(id).expect("collected above");
            let reused = existing.and_then(|(prefab, def)| node.prefab.filter(|link| link.prefab == prefab && def.node(link.uid).is_some()).map(|link| link.uid));
            // A uid may only be claimed once (two nodes duplicated from the
            // same prefab node inside one instance both carry its link).
            let uid = match reused {
                Some(uid) if !uids.values().any(|&u| u == uid) => uid,
                _ => {
                    next_uid += 1;
                    next_uid - 1
                }
            };
            uids.insert(id, uid);
        }

        let nodes = order
            .iter()
            .map(|&id| {
                let mut node = artboard.node(id).expect("collected above").clone();
                let parent = if id == root { None } else { node.parent.and_then(|p| uids.get(&p).copied()) };
                node.parent = None;
                node.prefab = None;
                remap_node_refs(&mut node, &mut |target| uids.get(&target).map(|&uid| uid_ref(uid)));
                PrefabNode { uid: uids[&id], parent, node }
            })
            .collect();

        let links = order.iter().map(|id| (*id, uids[id])).collect();
        (Self { name: name.into(), nodes, next_uid, deleted: false }, links)
    }

    /// The scene-ready copy of prefab node `uid` — linked to `prefab`, its
    /// element id cleared (a fresh one is minted on insert so lookups by id
    /// stay unique), parent set to `parent`. Node references are still
    /// synthetic; fix them with [`remap_node_refs`] once every node of the
    /// instance exists.
    pub fn scene_node(&self, prefab: PrefabId, uid: u32, parent: Option<NodeId>) -> Option<Node> {
        let source = self.node(uid)?;
        let mut node = source.node.clone();
        node.parent = parent;
        node.id = String::new();
        node.deleted = false;
        node.prefab = Some(PrefabLink { prefab, uid });
        Some(node)
    }

    /// Instantiates the whole prefab, inserting nodes through `insert`
    /// (parents first) and then finishing their node references through
    /// `finish(new_id, node_with_refs_remapped)`. Two callbacks so the same
    /// logic serves both the undoable editor path (`Command::AddNode` +
    /// `Command::ReplaceNode`) and the runtime spawn path (direct artboard
    /// writes). Returns every new node id, root first.
    pub fn instantiate_with(
        &self,
        prefab: PrefabId,
        parent: Option<NodeId>,
        root_transform: Option<crate::Transform2D>,
        mut insert: impl FnMut(Node) -> NodeId,
        mut finish: impl FnMut(NodeId, Node),
    ) -> Vec<NodeId> {
        let mut ids: HashMap<u32, NodeId> = HashMap::new();
        let mut created = Vec::new();
        for (i, prefab_node) in self.nodes.iter().enumerate() {
            let scene_parent = match prefab_node.parent {
                None => parent,
                Some(p) => ids.get(&p).copied(),
            };
            let Some(mut node) = self.scene_node(prefab, prefab_node.uid, scene_parent) else { continue };
            if i == 0 {
                if let Some(transform) = root_transform {
                    node.transform = transform;
                }
            }
            let id = insert(node);
            ids.insert(prefab_node.uid, id);
            created.push((id, prefab_node.uid));
        }
        for &(id, uid) in &created {
            let source = &self.node(uid).expect("instantiated above").node;
            if !has_node_refs(source) {
                continue;
            }
            let mut node = self.scene_node(prefab, uid, None).expect("instantiated above");
            remap_node_refs(&mut node, &mut |target| ids.get(&ref_uid(target)).copied());
            finish(id, node);
        }
        created.into_iter().map(|(id, _)| id).collect()
    }
}

/// Whether `node` holds any node reference [`remap_node_refs`] would touch
/// — lets instantiation skip the second pass for the common case.
pub fn has_node_refs(node: &Node) -> bool {
    let mut any = false;
    let mut probe = node.clone();
    remap_node_refs(&mut probe, &mut |id| {
        any = true;
        Some(id)
    });
    any
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::GroupNode;
    use crate::{Document, PrefabDefinition};

    fn group(name: &str, parent: Option<NodeId>) -> Node {
        Node::new(name, parent, NodeKind::Group(GroupNode::default()))
    }

    #[test]
    fn capture_then_instantiate_round_trips_hierarchy_and_refs() {
        let mut artboard = Artboard::new("Main", 100.0, 100.0);
        let root = artboard.add_node(group("Goblin", None));
        let body = artboard.add_node(group("Body", Some(root)));
        let _hat = artboard.add_node(group("Hat", Some(body)));
        // The root masks with its body — an internal reference.
        if let NodeKind::Group(g) = &mut artboard.node_mut(root).unwrap().kind {
            g.mask = Some(body);
        }

        let mut doc = Document::new();
        let placeholder = doc.add_prefab(PrefabDefinition { name: String::new(), nodes: vec![], next_uid: 0, deleted: false });
        let (def, links) = PrefabDefinition::capture(&artboard, root, "Goblin", None);
        assert_eq!(def.nodes.len(), 3);
        assert_eq!(links.len(), 3);
        assert_eq!(def.nodes[0].parent, None);
        assert_eq!(def.nodes[2].parent, Some(def.nodes[1].uid));

        let mut target = Artboard::new("Other", 100.0, 100.0);
        let mut finished = Vec::new();
        let ids = def.instantiate_with(placeholder, None, None, |n| target.add_node(n), |id, n| finished.push((id, n)));
        for (id, n) in finished {
            *target.node_mut(id).unwrap() = n;
        }
        assert_eq!(ids.len(), 3);
        let new_root = target.node(ids[0]).unwrap();
        assert_eq!(new_root.prefab.map(|l| l.uid), Some(def.root_uid()));
        // The mask now points at the *new* body, not the source's.
        match &new_root.kind {
            NodeKind::Group(g) => assert_eq!(g.mask, Some(ids[1])),
            _ => panic!("root is a group"),
        }
        assert_eq!(target.node(ids[2]).unwrap().parent, Some(ids[1]));
    }

    #[test]
    fn recapture_keeps_existing_uids() {
        let mut artboard = Artboard::new("Main", 100.0, 100.0);
        let root = artboard.add_node(group("Root", None));
        artboard.add_node(group("A", Some(root)));
        let mut doc = Document::new();
        let prefab = doc.add_prefab(PrefabDefinition { name: String::new(), nodes: vec![], next_uid: 0, deleted: false });
        let (def, links) = PrefabDefinition::capture(&artboard, root, "P", None);
        for (id, uid) in links {
            artboard.node_mut(id).unwrap().prefab = Some(PrefabLink { prefab, uid });
        }
        // Add a new child to the instance, then re-capture (Apply).
        artboard.add_node(group("B", Some(root)));
        let (def2, _) = PrefabDefinition::capture(&artboard, root, "P", Some((prefab, &def)));
        assert_eq!(def2.nodes.len(), 3);
        assert_eq!(def2.nodes[0].uid, def.nodes[0].uid);
        assert_eq!(def2.nodes[1].uid, def.nodes[1].uid);
        assert_eq!(def2.nodes[2].uid, def.next_uid);
        assert_eq!(def2.next_uid, def.next_uid + 1);
    }
}
