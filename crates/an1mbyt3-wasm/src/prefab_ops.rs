//! Editor-side prefab operations — the document edits behind the Inspector's
//! "Create Prefab / Apply / Revert / Unpack" buttons, placing a prefab from
//! the Resources panel, and scripts' runtime `ctx.spawn`. The data model
//! itself (definitions, links, uid-based node references) lives in
//! `an1mbyt3_core::prefab`; read its module doc first.
//!
//! Every undoable operation here applies its commands one by one with the
//! raw `Command::apply` (later commands need ids earlier ones produced) and
//! returns the inverses in undo order, for the caller to fold into one undo
//! step with `Document::commit_batch` — the same pattern `duplicate_nodes`
//! uses.

use std::collections::HashMap;

use an1mbyt3_core::{remap_node_refs, ref_uid, Artboard, ArtboardId, Command, Document, Node, NodeId, PrefabDefinition, PrefabId, PrefabLink, Transform2D};

/// The node `node` belongs to as a prefab instance: walks up to the nearest
/// ancestor-or-self that is the *root* of an instance (linked, with the
/// prefab's root uid). `None` if `node` isn't inside any instance.
pub fn instance_root(document: &Document, artboard: &Artboard, node: NodeId) -> Option<NodeId> {
    let mut cursor = Some(node);
    while let Some(id) = cursor {
        let current = artboard.node(id)?;
        if let Some(link) = current.prefab {
            if document.prefab(link.prefab).is_some_and(|def| def.root_uid() == link.uid) {
                return Some(id);
            }
        }
        cursor = current.parent;
    }
    None
}

/// Every live node of the instance rooted at `root` that is linked to
/// `prefab`, by uid (first one wins if a uid appears twice). Descends
/// through unlinked children too — a node the author added inside an
/// instance can itself contain linked nodes only in odd cases, but walking
/// everything keeps this simple and total.
fn instance_nodes(artboard: &Artboard, root: NodeId, prefab: PrefabId) -> HashMap<u32, NodeId> {
    let mut found = HashMap::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let Some(node) = artboard.node(id) else { continue };
        if node.deleted {
            continue;
        }
        if let Some(link) = node.prefab.filter(|l| l.prefab == prefab) {
            found.entry(link.uid).or_insert(id);
        }
        stack.extend(artboard.children_of(id).map(|(child, _)| child));
    }
    found
}

/// Turns the subtree at `root` into a new prefab and links those very nodes
/// to it — like dragging a GameObject into Unity's Project window, the
/// original becomes the first instance.
pub fn create_prefab(document: &mut Document, artboard: ArtboardId, root: NodeId, name: String) -> Result<(PrefabId, Vec<Command>), String> {
    let artboard_ref = document.artboard(artboard).ok_or("unknown scene")?;
    if artboard_ref.node(root).is_none_or(|n| n.deleted) {
        return Err("unknown node".into());
    }
    let (definition, links) = PrefabDefinition::capture(artboard_ref, root, name, None);
    let mut inverses = Vec::new();
    let inverse = Command::AddPrefab { definition }.apply(document);
    let Command::SetPrefabDeleted { prefab, .. } = inverse else { unreachable!("AddPrefab's inverse is SetPrefabDeleted") };
    inverses.push(inverse);
    link_nodes(document, artboard, prefab, &links, &mut inverses);
    inverses.reverse();
    Ok((prefab, inverses))
}

/// Sets `node.prefab` on each `(node, uid)` (one `ReplaceNode` each).
fn link_nodes(document: &mut Document, artboard: ArtboardId, prefab: PrefabId, links: &[(NodeId, u32)], inverses: &mut Vec<Command>) {
    for &(node_id, uid) in links {
        let Some(node) = document.artboard(artboard).and_then(|a| a.node(node_id)) else { continue };
        let wanted = Some(PrefabLink { prefab, uid });
        if node.prefab == wanted {
            continue;
        }
        let mut value = node.clone();
        value.prefab = wanted;
        inverses.push(Command::ReplaceNode { artboard, node: node_id, value: Box::new(value) }.apply(document));
    }
}

/// Places a new instance of `prefab` under `parent` (scene root if `None`),
/// its root at `root_transform` (the prefab's own root transform if `None`).
/// Undoable as one step via the returned inverses. Returns the new root.
pub fn instantiate(
    document: &mut Document,
    artboard: ArtboardId,
    prefab: PrefabId,
    parent: Option<NodeId>,
    root_transform: Option<Transform2D>,
) -> Result<(NodeId, Vec<Command>), String> {
    let definition = document.prefab(prefab).filter(|d| !d.deleted).ok_or("unknown prefab")?.clone();
    if definition.nodes.is_empty() {
        return Err("prefab is empty".into());
    }
    let mut inverses = Vec::new();
    let mut fixups: Vec<(NodeId, Node)> = Vec::new();
    let ids = definition.instantiate_with(
        prefab,
        parent,
        root_transform,
        |node| {
            let inverse = Command::AddNode { artboard, node }.apply(document);
            let Command::SetDeleted { node: id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
            inverses.push(inverse);
            id
        },
        |id, node| fixups.push((id, node)),
    );
    for (id, mut node) in fixups {
        // `scene_node` cleared the element id; keep the one minted on insert.
        if let Some(existing) = document.artboard(artboard).and_then(|a| a.node(id)) {
            node.id = existing.id.clone();
        }
        inverses.push(Command::ReplaceNode { artboard, node: id, value: Box::new(node) }.apply(document));
    }
    inverses.reverse();
    Ok((ids[0], inverses))
}

/// Makes the instance rooted at `root` match its prefab again (Revert, and
/// the second half of Apply for every *other* instance):
///
/// - prefab nodes the instance lacks are added (under their prefab parent),
/// - linked nodes whose prefab node is gone are deleted,
/// - every linked node's data is replaced by the prefab's, keeping only
///   per-instance identity: its element id, and on the root its transform
///   and name.
///
/// Nodes the author added inside the instance (unlinked) are left alone —
/// Unity's "added GameObject" override.
pub fn sync_instance(document: &mut Document, artboard: ArtboardId, root: NodeId, inverses: &mut Vec<Command>) {
    let Some(link) = document.artboard(artboard).and_then(|a| a.node(root)).and_then(|n| n.prefab) else { return };
    let Some(definition) = document.prefab(link.prefab).filter(|d| !d.deleted).cloned() else { return };
    let prefab = link.prefab;
    let mut nodes = instance_nodes(document.artboard(artboard).expect("checked above"), root, prefab);

    // Delete linked nodes the prefab no longer has.
    let stale: Vec<NodeId> = nodes.iter().filter(|(uid, _)| definition.node(**uid).is_none()).map(|(_, id)| *id).collect();
    for id in stale {
        inverses.push(Command::SetDeleted { artboard, node: id, value: true }.apply(document));
    }
    nodes.retain(|uid, _| definition.node(*uid).is_some());

    // Add what's missing (definition order is parents-first, so the parent
    // always exists by the time a child is added).
    for prefab_node in &definition.nodes {
        if nodes.contains_key(&prefab_node.uid) {
            continue;
        }
        let parent = match prefab_node.parent {
            Some(p) => nodes.get(&p).copied(),
            None => continue, // the root is `root` itself
        };
        let Some(node) = definition.scene_node(prefab, prefab_node.uid, parent) else { continue };
        let inverse = Command::AddNode { artboard, node }.apply(document);
        let Command::SetDeleted { node: id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
        inverses.push(inverse);
        nodes.insert(prefab_node.uid, id);
    }

    // Overwrite each linked node's data from the prefab.
    for prefab_node in &definition.nodes {
        let Some(&id) = nodes.get(&prefab_node.uid) else { continue };
        let Some(current) = document.artboard(artboard).and_then(|a| a.node(id)).cloned() else { continue };
        let Some(mut value) = definition.scene_node(prefab, prefab_node.uid, current.parent) else { continue };
        remap_node_refs(&mut value, &mut |target| nodes.get(&ref_uid(target)).copied());
        value.id = current.id.clone();
        if id == root {
            value.transform = current.transform;
            value.name = current.name.clone();
        }
        if value != current {
            inverses.push(Command::ReplaceNode { artboard, node: id, value: Box::new(value) }.apply(document));
        }
    }
}

/// Apply: writes the instance at `root` back into its prefab (keeping the
/// prefab's name), links any nodes the author added, then re-syncs every
/// other instance of that prefab in every scene.
pub fn apply_instance(document: &mut Document, artboard: ArtboardId, root: NodeId) -> Result<Vec<Command>, String> {
    let link = document.artboard(artboard).and_then(|a| a.node(root)).and_then(|n| n.prefab).ok_or("not a prefab instance")?;
    let previous = document.prefab(link.prefab).ok_or("unknown prefab")?.clone();
    let (definition, links) = PrefabDefinition::capture(document.artboard(artboard).expect("checked above"), root, previous.name.clone(), Some((link.prefab, &previous)));
    let mut inverses = Vec::new();
    inverses.push(Command::SetPrefab { prefab: link.prefab, value: Box::new(definition) }.apply(document));
    link_nodes(document, artboard, link.prefab, &links, &mut inverses);

    // Every other instance root, across all scenes.
    let root_uid = document.prefab(link.prefab).expect("just set").root_uid();
    let others: Vec<(ArtboardId, NodeId)> = document
        .artboards()
        .flat_map(|(artboard_id, a)| {
            a.node_ids()
                .filter(move |&id| a.node(id).is_some_and(|n| !n.deleted && n.prefab == Some(PrefabLink { prefab: link.prefab, uid: root_uid })))
                .map(move |id| (artboard_id, id))
                .collect::<Vec<_>>()
        })
        .filter(|&(a, id)| !(a == artboard && id == root))
        .collect();
    for (other_artboard, other_root) in others {
        sync_instance(document, other_artboard, other_root, &mut inverses);
    }
    inverses.reverse();
    Ok(inverses)
}

/// Unpack: clears the prefab links on the whole instance at `root`.
pub fn unpack_instance(document: &mut Document, artboard: ArtboardId, root: NodeId) -> Vec<Command> {
    let Some(link) = document.artboard(artboard).and_then(|a| a.node(root)).and_then(|n| n.prefab) else { return Vec::new() };
    let nodes: Vec<NodeId> = instance_nodes(document.artboard(artboard).expect("checked above"), root, link.prefab).into_values().collect();
    let mut inverses = Vec::new();
    for id in nodes {
        let Some(mut value) = document.artboard(artboard).and_then(|a| a.node(id)).cloned() else { continue };
        value.prefab = None;
        inverses.push(Command::ReplaceNode { artboard, node: id, value: Box::new(value) }.apply(document));
    }
    inverses.reverse();
    inverses
}

/// Runtime spawn (scripts' `ctx.spawn`, Playground only): instantiates
/// straight into the artboard — no commands, no undo history. The caller
/// records the returned ids and `despawn`s them when play ends, so a play
/// session never leaves spawned objects behind in the document.
pub fn spawn_runtime(artboard: &mut Artboard, definition: &PrefabDefinition, prefab: PrefabId, parent: Option<NodeId>, root_transform: Transform2D) -> Vec<NodeId> {
    let mut fixups: Vec<(NodeId, Node)> = Vec::new();
    let ids = definition.instantiate_with(prefab, parent, Some(root_transform), |node| artboard.add_node(node), |id, node| fixups.push((id, node)));
    for (id, mut node) in fixups {
        if let Some(existing) = artboard.node_mut(id) {
            node.id = existing.id.clone();
            node.parent = existing.parent;
            *existing = node;
        }
    }
    ids
}

/// Hard-removes `root` and its whole subtree (runtime despawn).
pub fn despawn_subtree(artboard: &mut Artboard, root: NodeId) {
    let mut order = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if artboard.node(id).is_none() {
            continue;
        }
        order.push(id);
        stack.extend(artboard.children_of(id).map(|(child, _)| child));
    }
    // Children first, so no parent disappears from under a live child.
    for id in order.into_iter().rev() {
        artboard.despawn(id);
    }
}
