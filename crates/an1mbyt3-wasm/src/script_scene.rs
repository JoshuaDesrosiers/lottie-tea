//! The scene as Code-mode scripts see it — a lean, fast alternative to the
//! full `DocumentSummary` for the per-frame script loop.
//!
//! The full summary carries everything the editor's panels need (every
//! animation track, gradients, bounds, components…) for every node, and
//! re-serializing it each frame cost ~100 ms at 4k nodes. Scripts only
//! need a few fields, and most never change frame to frame, so this is
//! split in two:
//!
//! - **static table** (`script_scene`): ids, names, classes, kind, parent,
//!   text, path, body type — re-sent only when the scene's *structure*
//!   changes (`script_revision` — nodes added/removed/reparented).
//! - **dynamic block** (`script_dynamic`): one flat `Float32Array`, `STRIDE`
//!   floats per node in the static table's order — local transform, world
//!   transform, visibility, z-index, fill color, image — a single memory
//!   copy per frame.

use an1mbyt3_core::{Artboard, Document, ImageId, NodeId, NodeKind, RigidbodyType2D};
use serde::Serialize;

use crate::dto::PathDto;

/// Floats per node in `script_dynamic`:
/// `[0..7)` local `tx, ty, rot, sx, sy, ax, ay`, `[7..14)` the same for the
/// world transform, `14` visible (0/1), `15` z-index, `[16..20)` fill RGBA
/// (NaN = no fill), `20..22` image id `idx, version` (NaN = none),
/// `[22..31)` a Node3D's position xyz, rotation (degrees) xyz, scale xyz
/// (NaN for 2D nodes).
pub const STRIDE: usize = 31;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptNodeDto {
    pub id: NodeId,
    pub element_id: String,
    pub name: String,
    pub classes: Vec<String>,
    pub kind: &'static str,
    pub parent: Option<NodeId>,
    pub text_content: Option<String>,
    pub path: Option<PathDto>,
    pub body_type: Option<RigidbodyType2D>,
}

#[derive(Serialize)]
pub struct ScriptImageDto {
    pub id: ImageId,
    pub name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptSceneDto {
    pub revision: u32,
    pub nodes: Vec<ScriptNodeDto>,
    pub images: Vec<ScriptImageDto>,
}

pub fn kind_name(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Shape(_) => "Shape",
        NodeKind::Group(_) => "Group",
        NodeKind::Bone(_) => "Bone",
        NodeKind::Instance(_) => "Instance",
        NodeKind::Text(_) => "Text",
        NodeKind::Image(_) => "Image",
        NodeKind::ParticleSystem(_) => "ParticleSystem",
        NodeKind::Tilemap(_) => "Tilemap",
        NodeKind::Camera(_) => "Camera",
        NodeKind::Light2D(_) => "Light2D",
        NodeKind::Trail(_) => "Trail",
        NodeKind::Node3D(_) => "Node3D",
    }
}

/// The live (non-deleted) nodes, in the order both tables use.
fn live_nodes(artboard: &Artboard) -> impl Iterator<Item = NodeId> + '_ {
    artboard.node_ids().filter(|id| !artboard.is_effectively_deleted(*id))
}

/// A cheap fingerprint of the scene's structure (which nodes exist, their
/// parents and kinds) plus the image count — when it changes, the script
/// side refetches the static table. Kept to 24 bits so it survives a
/// round-trip through `f64` comparisons exactly.
pub fn revision(document: &Document, artboard: &Artboard) -> u32 {
    let mut h: u32 = 2166136261;
    let mut mix = |v: u32| {
        h ^= v;
        h = h.wrapping_mul(16777619);
    };
    for id in live_nodes(artboard) {
        let node = artboard.node(id).expect("live node exists");
        let (idx, version) = id_parts(id);
        mix(idx);
        mix(version);
        mix(node.parent.map_or(u32::MAX, |p| id_parts(p).0));
        mix(kind_name(&node.kind).len() as u32);
        mix(node.rigidbody().map_or(9, |rb| rb.body_type as u32));
    }
    mix(document.images().count() as u32);
    h & 0x00ff_ffff
}

fn id_parts<K: slotmap::Key>(id: K) -> (u32, u32) {
    let ffi = id.data().as_ffi();
    (ffi as u32, (ffi >> 32) as u32)
}

pub fn scene(document: &Document, artboard: &Artboard) -> ScriptSceneDto {
    let nodes = live_nodes(artboard)
        .filter_map(|id| {
            let node = artboard.node(id)?;
            Some(ScriptNodeDto {
                id,
                element_id: node.id.clone(),
                name: node.name.clone(),
                classes: node.classes.clone(),
                kind: kind_name(&node.kind),
                parent: node.parent,
                text_content: match &node.kind {
                    NodeKind::Text(text) => Some(text.content.clone()),
                    _ => None,
                },
                path: match &node.kind {
                    NodeKind::Shape(shape) => Some(PathDto::from(&shape.path)),
                    _ => None,
                },
                body_type: node.rigidbody().map(|rb| rb.body_type),
            })
        })
        .collect();
    let images = document.images().map(|(id, asset)| ScriptImageDto { id, name: asset.name.clone() }).collect();
    ScriptSceneDto { revision: revision(document, artboard), nodes, images }
}

pub fn dynamic(artboard: &Artboard) -> Vec<f32> {
    let ids: Vec<NodeId> = live_nodes(artboard).collect();
    let mut out = vec![f32::NAN; ids.len() * STRIDE];
    for (i, id) in ids.into_iter().enumerate() {
        let Some(node) = artboard.node(id) else { continue };
        let o = i * STRIDE;
        let put = |out: &mut Vec<f32>, at: usize, t: &an1mbyt3_core::Transform2D| {
            out[at..at + 7].copy_from_slice(&[t.translation.x, t.translation.y, t.rotation, t.scale.x, t.scale.y, t.anchor.x, t.anchor.y]);
        };
        put(&mut out, o, &node.transform);
        let world = artboard.try_world_transform(id).unwrap_or(node.transform);
        put(&mut out, o + 7, &world);
        out[o + 14] = if node.visible { 1.0 } else { 0.0 };
        out[o + 15] = node.z_index;
        if let Some(fill) = node.fill_color() {
            out[o + 16..o + 20].copy_from_slice(&fill);
        }
        if let NodeKind::Image(image) = &node.kind {
            let (idx, version) = id_parts(image.image);
            out[o + 20] = idx as f32;
            out[o + 21] = version as f32;
        }
        if let NodeKind::Node3D(n) = &node.kind {
            let t = &n.transform;
            out[o + 22..o + 31].copy_from_slice(&[
                t.position.x,
                t.position.y,
                t.position.z,
                t.rotation_degrees.x,
                t.rotation_degrees.y,
                t.rotation_degrees.z,
                t.scale.x,
                t.scale.y,
                t.scale.z,
            ]);
        }
    }
    out
}
