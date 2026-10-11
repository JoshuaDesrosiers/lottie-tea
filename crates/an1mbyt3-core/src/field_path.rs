//! **Field paths**: a string address for any value a node shows in the
//! Inspector — so *anything* can be keyframed (`PropertyTrack::Field`), not
//! just the handful of properties with their own track type.
//!
//! A path is a root plus a JSON pointer into that part of the node, as
//! `serde_json` sees it (the same names the editor's documents use):
//!
//! | Path | Addresses |
//! |---|---|
//! | `transform/pivot` | the 2D transform (`Transform2D`) |
//! | `node/visible`, `node/z_index` | the node's own flags |
//! | `kind/Node3D/transform/position` | the node's kind (`NodeKind`): its 3D transform, mesh colour, light intensity, camera FOV, text size … |
//! | `behavior/Light2D/intensity` | a component (`NodeBehavior`) — the first of that kind; `behavior/BoxCollider2D~1/…` the second |
//!
//! A value is up to 4 floats: a number (`[x, 0, 0, 0]`), a bool (`1` / `0`,
//! switching at 0.5 when blended), an integer (rounded) or a short numeric
//! array (vectors, colours). Writing keeps the field's own shape, so a
//! colour stays an `[f32; 4]` and a count stays an integer.
//!
//! Reads and writes go through `serde_json` (serialize the part, change one
//! leaf, deserialize it back) — generic over every component without a
//! hand-written accessor each, at a small per-write cost that's fine for
//! animation (a handful of tracks per node per frame).

use serde::Serialize;
use serde_json::Value;

use crate::node::Node;
use crate::timeline::PropertyKind;

/// What kind of value a field holds (how a sampled `[f32; 4]` is written
/// back, and how the editor shows it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FieldShape {
    Number,
    Integer,
    Bool,
    /// A numeric array of this many components (2..=4): a vector / colour.
    Vector(u8),
}

/// One animatable field of a node — the editor's "+ Property" list.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldInfo {
    pub path: String,
    /// Readable name parts, Inspector-style: the component / kind first,
    /// then field names — list entries by their own `name` / `bone`
    /// (`["VrmPoses", "=Guard", "=leftUpperArm", "rotation"]`, `=` marking
    /// an entry's own name), not index.
    pub label: Vec<String>,
    pub shape: FieldShape,
    pub value: [f32; 4],
}

/// A field path's track identity (64-bit FNV-1a of the path) — what
/// `PropertyKind::Field` carries so tracks stay `Copy`-keyed.
pub fn field_kind(path: &str) -> PropertyKind {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in path.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    PropertyKind::Field(h)
}

/// The field's current value, if the path exists on this node.
pub fn read(node: &Node, path: &str) -> Option<[f32; 4]> {
    let (root, pointer) = split(path)?;
    let value = root_value(node, root)?;
    leaf_read(resolve(&value, &pointer)?).map(|(v, _)| v)
}

/// Writes `v` into the field (keeping its shape). `false` if the path
/// doesn't exist on this node or the result wouldn't be valid.
pub fn write(node: &mut Node, path: &str, v: [f32; 4]) -> bool {
    let Some((root, pointer)) = split(path) else { return false };
    // Fast paths for the node's own flags.
    if root == "node" {
        return match pointer.as_str() {
            "/visible" => {
                node.visible = v[0] >= 0.5;
                true
            }
            "/z_index" => {
                node.z_index = v[0];
                true
            }
            _ => false,
        };
    }
    let Some(mut value) = root_value(node, root) else { return false };
    let Some(leaf) = resolve_mut(&mut value, &pointer) else { return false };
    if !leaf_write(leaf, v) {
        return false;
    }
    store_root(node, root, value)
}

/// Every animatable field on this node, with its current value.
pub fn list(node: &Node) -> Vec<FieldInfo> {
    let mut out = Vec::new();
    out.push(FieldInfo { path: "node/visible".into(), label: vec!["Node".into(), "visible".into()], shape: FieldShape::Bool, value: [if node.visible { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0] });
    out.push(FieldInfo { path: "node/z_index".into(), label: vec!["Node".into(), "z_index".into()], shape: FieldShape::Number, value: [node.z_index, 0.0, 0.0, 0.0] });
    if let Ok(v) = serde_json::to_value(&node.transform) {
        walk(&v, "transform", &["Transform".to_string()], 0, &mut out);
    }
    if let Ok(v) = serde_json::to_value(&node.kind) {
        // `{"Node3D": {...}}`: the kind's name heads the label.
        match &v {
            Value::Object(map) if map.len() == 1 => {
                let (kind, inner) = map.iter().next().expect("one entry");
                walk(inner, &format!("kind/{}", escape(kind)), std::slice::from_ref(kind), 0, &mut out);
            }
            _ => walk(&v, "kind", &["Kind".to_string()], 0, &mut out),
        }
    }
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for entry in &node.behaviors {
        let Ok(Value::Object(map)) = serde_json::to_value(&entry.behavior) else { continue };
        let Some((name, inner)) = map.into_iter().next() else { continue };
        let n = seen.entry(name.clone()).or_insert(0);
        let token = if *n == 0 { name.clone() } else { format!("{name}~{n}") };
        *n += 1;
        let head = if token == name { name.clone() } else { format!("{name} #{}", *n) };
        walk(&inner, &format!("behavior/{}", escape(&token)), &[head], 0, &mut out);
    }
    out
}

/// `"root/rest/of/it"` → (`"root"`, `"/rest/of/it"` as a JSON pointer).
/// Behaviour roots keep their component token: (`"behavior/Light2D"`, …).
fn split(path: &str) -> Option<(&str, String)> {
    let (root, rest) = path.split_once('/')?;
    if root == "behavior" {
        let (token, rest) = rest.split_once('/')?;
        let root_len = "behavior/".len() + token.len();
        return Some((&path[..root_len], format!("/{rest}")));
    }
    Some((root, format!("/{rest}")))
}

/// The JSON of one part of the node a path starts at.
fn root_value(node: &Node, root: &str) -> Option<Value> {
    match root {
        "transform" => serde_json::to_value(&node.transform).ok(),
        "kind" => serde_json::to_value(&node.kind).ok(),
        "node" => Some(serde_json::json!({ "visible": node.visible, "z_index": node.z_index })),
        _ => {
            let (name, nth) = behavior_token(root)?;
            let entry = node.behaviors.iter().filter(|e| behavior_name(&e.behavior).as_deref() == Some(name.as_str())).nth(nth)?;
            match serde_json::to_value(&entry.behavior).ok()? {
                Value::Object(map) => map.into_iter().next().map(|(_, v)| v),
                _ => None,
            }
        }
    }
}

/// Puts an edited root back into the node.
fn store_root(node: &mut Node, root: &str, value: Value) -> bool {
    match root {
        "transform" => serde_json::from_value(value).map(|t| node.transform = t).is_ok(),
        "kind" => serde_json::from_value(value).map(|k| node.kind = k).is_ok(),
        _ => {
            let Some((name, nth)) = behavior_token(root) else { return false };
            let Some(entry) = node.behaviors.iter_mut().filter(|e| behavior_name(&e.behavior).as_deref() == Some(name.as_str())).nth(nth) else { return false };
            let mut map = serde_json::Map::new();
            map.insert(name, value);
            serde_json::from_value(Value::Object(map)).map(|b| entry.behavior = b).is_ok()
        }
    }
}

/// `"behavior/BoxCollider2D~1"` → (`"BoxCollider2D"`, 1).
fn behavior_token(root: &str) -> Option<(String, usize)> {
    let token = unescape(root.strip_prefix("behavior/")?);
    Some(match token.rsplit_once('~') {
        Some((name, n)) if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => (name.to_string(), n.parse().ok()?),
        _ => (token, 0),
    })
}

fn behavior_name(b: &crate::NodeBehavior) -> Option<String> {
    match serde_json::to_value(b).ok()? {
        Value::Object(map) => map.into_iter().next().map(|(k, _)| k),
        Value::String(s) => Some(s),
        _ => None,
    }
}

/// A list entry's own name (`name` / `bone` / `label` / `id`), if it has one.
fn own_name(v: &Value) -> Option<&str> {
    let obj = v.as_object()?;
    ["name", "bone", "label", "id"].iter().find_map(|k| obj.get(*k).and_then(Value::as_str).filter(|s| !s.is_empty()))
}

/// Follows a pointer like a JSON pointer, plus one extension: in a list, a
/// segment `=name` picks the entry with that own name — so a track on
/// "pose Guard, bone leftUpperArm" keeps pointing at it when entries are
/// added, removed or reordered (a plain index wouldn't).
fn resolve<'a>(root: &'a Value, pointer: &str) -> Option<&'a Value> {
    let mut v = root;
    for raw in pointer.split('/').skip(1) {
        let seg = unescape(raw);
        v = match v {
            Value::Object(map) => map.get(&seg)?,
            Value::Array(items) => match seg.strip_prefix('=') {
                Some(name) => items.iter().find(|i| own_name(i) == Some(name))?,
                None => items.get(seg.parse::<usize>().ok()?)?,
            },
            _ => return None,
        };
    }
    Some(v)
}

fn resolve_mut<'a>(root: &'a mut Value, pointer: &str) -> Option<&'a mut Value> {
    let mut v = root;
    for raw in pointer.split('/').skip(1) {
        let seg = unescape(raw);
        v = match v {
            Value::Object(map) => map.get_mut(&seg)?,
            Value::Array(items) => match seg.strip_prefix('=') {
                Some(name) => items.iter_mut().find(|i| own_name(i) == Some(name))?,
                None => items.get_mut(seg.parse::<usize>().ok()?)?,
            },
            _ => return None,
        };
    }
    Some(v)
}

/// JSON-pointer escaping for one segment (`~` → `~0`, `/` → `~1`) — only
/// needed for odd map keys; component names never contain either.
fn escape(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

fn unescape(s: &str) -> String {
    s.replace("~1", "/").replace("~0", "~")
}

/// A leaf's value and shape (numbers, bools, short numeric arrays).
fn leaf_read(v: &Value) -> Option<([f32; 4], FieldShape)> {
    match v {
        Value::Bool(b) => Some(([if *b { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0], FieldShape::Bool)),
        Value::Number(n) => {
            let shape = if n.is_f64() { FieldShape::Number } else { FieldShape::Integer };
            Some(([n.as_f64()? as f32, 0.0, 0.0, 0.0], shape))
        }
        Value::Array(items) if (2..=4).contains(&items.len()) && items.iter().all(Value::is_number) => {
            let mut out = [0.0; 4];
            for (o, item) in out.iter_mut().zip(items) {
                *o = item.as_f64()? as f32;
            }
            Some((out, FieldShape::Vector(items.len() as u8)))
        }
        _ => None,
    }
}

/// Writes `v` into a leaf in its own shape.
fn leaf_write(leaf: &mut Value, v: [f32; 4]) -> bool {
    let Some((_, shape)) = leaf_read(leaf) else { return false };
    let num = |x: f32| serde_json::Number::from_f64(x as f64).map(Value::Number);
    *leaf = match shape {
        FieldShape::Bool => Value::Bool(v[0] >= 0.5),
        FieldShape::Number => match num(v[0]) {
            Some(n) => n,
            None => return false,
        },
        FieldShape::Integer => {
            let r = v[0].round();
            if leaf.as_u64().is_some() {
                Value::from(r.max(0.0) as u64)
            } else {
                Value::from(r as i64)
            }
        }
        FieldShape::Vector(n) => {
            let mut items = Vec::with_capacity(n as usize);
            for x in v.iter().take(n as usize) {
                match num(*x) {
                    Some(n) => items.push(n),
                    None => return false,
                }
            }
            Value::Array(items)
        }
    };
    true
}

/// Collects the animatable leaves under `v` (at `path`), naming each
/// (`label`) the way the Inspector shows it.
fn walk(v: &Value, path: &str, label: &[String], depth: usize, out: &mut Vec<FieldInfo>) {
    if depth > 10 {
        return;
    }
    if let Some((value, shape)) = leaf_read(v) {
        out.push(FieldInfo { path: path.to_string(), label: label.to_vec(), shape, value });
        return;
    }
    match v {
        Value::Object(map) => {
            // Ids (`{ idx, version }`) and soft-delete flags aren't animatable.
            if map.contains_key("idx") && map.contains_key("version") {
                return;
            }
            for (k, child) in map {
                // Data, not settings: paths (own track type), tile grids.
                if matches!(k.as_str(), "deleted" | "path" | "commands" | "tiles" | "points") {
                    continue;
                }
                let mut next = label.to_vec();
                // Structural layers the Inspector doesn't show as headings.
                if !matches!(k.as_str(), "transform" | "content" | "primitive") {
                    next.push(k.clone());
                }
                walk(child, &format!("{path}/{}", escape(k)), &next, depth + 1, out);
            }
        }
        // Lists of structs (poses, bones, gradient stops …): each entry by
        // its own name when it has one (`name` / `bone` / `label`).
        Value::Array(items) if items.len() <= 256 => {
            for (i, child) in items.iter().enumerate() {
                if child.is_object() {
                    // Addressed by name when it's unique in the list.
                    let own = own_name(child).filter(|n| items.iter().filter(|o| own_name(o) == Some(*n)).count() == 1);
                    // A named entry stands for its list ("Guard", not
                    // "poses › Guard").
                    let mut next = if own.is_some() && label.len() > 1 { label[..label.len() - 1].to_vec() } else { label.to_vec() };
                    // `=` marks an entry's own name (shown verbatim, not
                    // reworded like a field key).
                    next.push(own.map_or_else(|| format!("#{}", i + 1), |n| format!("={n}")));
                    let seg = own.map_or_else(|| i.to_string(), |n| format!("={}", escape(n)));
                    walk(child, &format!("{path}/{seg}"), &next, depth + 1, out);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Node, NodeKind};

    fn node3d() -> Node {
        let t = crate::Transform3D { position: glam::Vec3::new(1.0, 2.0, 3.0), rotation_degrees: glam::Vec3::ZERO, scale: glam::Vec3::ONE };
        let mesh = crate::Mesh3D::new(crate::Primitive3D::cube(), [1.0, 0.5, 0.25, 1.0]);
        Node::new("cube", None, NodeKind::Node3D(crate::Node3D::new(t, crate::Node3DContent::Mesh(mesh))))
    }

    #[test]
    fn lists_reads_and_writes_3d_fields() {
        let mut node = node3d();
        let fields = list(&node);
        let pos = fields.iter().find(|f| f.path.ends_with("transform/position")).expect("3D position listed");
        assert_eq!(pos.shape, FieldShape::Vector(3));
        assert_eq!(&pos.value[..3], &[1.0, 2.0, 3.0]);
        assert!(write(&mut node, &pos.path, [4.0, 5.0, 6.0, 0.0]));
        assert_eq!(read(&node, &pos.path).map(|v| v[..3].to_vec()), Some(vec![4.0, 5.0, 6.0]));
        assert_eq!(pos.label, vec!["Node3D".to_string(), "position".to_string()]);
        let color = fields.iter().find(|f| f.path.ends_with("/color")).expect("mesh colour listed");
        assert!(write(&mut node, &color.path, [0.0, 1.0, 0.0, 1.0]));
        assert_eq!(node.fill_color(), Some([0.0, 1.0, 0.0, 1.0]));
    }

    #[test]
    fn writes_components_and_flags() {
        let mut node = node3d();
        node.behaviors.push(crate::NodeBehaviorEntry { behavior: crate::NodeBehavior::Antialias2D(crate::Antialias2D { softness: 1.0 }), enabled: true });
        assert!(write(&mut node, "behavior/Antialias2D/softness", [3.5, 0.0, 0.0, 0.0]));
        assert_eq!(read(&node, "behavior/Antialias2D/softness"), Some([3.5, 0.0, 0.0, 0.0]));
        assert!(write(&mut node, "node/visible", [0.2, 0.0, 0.0, 0.0]));
        assert!(!node.visible);
        assert!(!write(&mut node, "behavior/Light2D/intensity", [1.0; 4]), "missing component");
        // List entries are named by their own name, not index.
        node.behaviors.push(crate::NodeBehaviorEntry { behavior: crate::NodeBehavior::VrmPoses(crate::VrmPoses::default()), enabled: true });
        if let crate::NodeBehavior::VrmPoses(p) = &mut node.behaviors[1].behavior {
            p.poses[0].bones.push(crate::BonePose { bone: "leftUpperArm".into(), rotation: [0.0, 0.0, -70.0], offset: [0.0; 3] });
        }
        let fields = list(&node);
        let arm = fields.iter().find(|f| f.path == "behavior/VrmPoses/poses/=Rest/bones/=leftUpperArm/rotation").expect("pose bone listed by name");
        assert_eq!(arm.label, vec!["VrmPoses", "=Rest", "=leftUpperArm", "rotation"]);
        // Still found after another bone is inserted before it.
        if let crate::NodeBehavior::VrmPoses(p) = &mut node.behaviors[1].behavior {
            p.poses[0].bones.insert(0, crate::BonePose { bone: "head".into(), rotation: [5.0, 0.0, 0.0], offset: [0.0; 3] });
        }
        assert!(write(&mut node, &arm.path, [0.0, 0.0, -30.0, 0.0]));
        assert_eq!(read(&node, &arm.path), Some([0.0, 0.0, -30.0, 0.0]));
        assert_eq!(read(&node, "behavior/VrmPoses/poses/0/bones/0/rotation"), Some([5.0, 0.0, 0.0, 0.0]), "old index paths still resolve");
        assert_ne!(field_kind("a"), field_kind("b"));
    }
}
