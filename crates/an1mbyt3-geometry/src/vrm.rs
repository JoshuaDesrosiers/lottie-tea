//! VRM avatars (VRM 0.x `VRM` and VRM 1.0 `VRMC_vrm` / `VRMC_springBone` /
//! `VRMC_materials_mtoon`) on top of the glTF importer ([`crate::model3d`]).
//!
//! A VRM is a glTF/GLB of a skinned humanoid plus extensions saying what
//! its parts *are*. This module reads them from the file's JSON:
//!
//! - **Humanoid bones**: the standard names (`hips`, `spine`, `head`,
//!   `leftUpperArm`, `rightFoot` …) mapped to glTF nodes, so scripts can
//!   pose any avatar the same way (a fighting game's punch works on every
//!   model).
//! - **Expressions**: named sets of morph-target weights (`happy`, `angry`,
//!   `blink`, mouth shapes `aa`/`ih`/`ou`/`ee`/`oh` …). VRM 0.x's preset
//!   names are mapped to VRM 1.0's.
//! - **Spring bones**: hair, skirts, ribbons — chains of joints that lag,
//!   swing and settle under stiffness, gravity and drag, pushed out of
//!   collider spheres/capsules on the body. Simulated by [`SpringSim`] in
//!   world space, so they react to the avatar moving.
//! - **MToon** materials: the anime cel-shading model — lit and shade
//!   colours split by a soft terminator, a rim, and an outline width — as
//!   [`Toon`] parameters for the renderer to build a toon shader from.
//!
//! VRM 0.x avatars face −Z, VRM 1.0 +Z ([`Vrm::forward_z`]).

use std::collections::HashMap;

use glam::{Mat4, Quat, Vec3};
use serde_json::Value;

/// One expression: morph-target weights to apply at full strength.
#[derive(Debug, Clone)]
pub struct Expression {
    /// VRM 1.0 preset name (`happy`, `aa`, `blink` …) or the custom name.
    pub name: String,
    /// Binary expressions snap on above 0.5.
    pub is_binary: bool,
    /// (glTF mesh index, morph target index, weight 0..1).
    pub binds: Vec<(usize, usize, f32)>,
}

/// MToon's cel-shading parameters for one material.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Toon {
    /// The colour lit parts fall to in shadow (linear RGB).
    pub shade: [f32; 3],
    /// Moves the lit/shade terminator (−1 … 1).
    pub shift: f32,
    /// How hard the terminator is (0 soft … 1 a hard line).
    pub toony: f32,
    /// Rim light colour (linear RGB) and its fresnel power / lift.
    pub rim: [f32; 3],
    pub rim_power: f32,
    pub rim_lift: f32,
    /// Outline width in metres (0 = none) and colour (linear RGB).
    pub outline_width: f32,
    pub outline: [f32; 3],
}

/// A spring-bone joint.
#[derive(Debug, Clone, Copy)]
pub struct SpringJoint {
    pub node: usize,
    pub hit_radius: f32,
    pub stiffness: f32,
    pub gravity_power: f32,
    pub gravity_dir: Vec3,
    pub drag: f32,
}

/// A chain of spring joints (each joint's tail is the next one) and the
/// colliders it can't pass through.
#[derive(Debug, Clone)]
pub struct SpringChain {
    pub joints: Vec<SpringJoint>,
    pub colliders: Vec<usize>,
}

/// A sphere (or a capsule, with a `tail`) on a node, in its local space.
#[derive(Debug, Clone, Copy)]
pub struct Collider {
    pub node: usize,
    pub offset: Vec3,
    pub radius: f32,
    pub tail: Option<Vec3>,
}

/// Everything VRM-specific about a model.
#[derive(Debug, Clone, Default)]
pub struct Vrm {
    /// 0 for VRM 0.x, 1 for VRM 1.0.
    pub version: u8,
    pub name: String,
    /// Humanoid bone name → glTF node.
    pub humanoid: HashMap<String, usize>,
    pub expressions: Vec<Expression>,
    pub springs: Vec<SpringChain>,
    pub colliders: Vec<Collider>,
    /// MToon parameters by glTF material index.
    pub toon: Vec<Option<Toon>>,
    /// glTF material names and emissive colours (linear), by index — for
    /// naming / seeding the editable material assets made on import.
    pub material_names: Vec<String>,
    pub emissive: Vec<[f32; 3]>,
    /// +1 if the avatar faces +Z (VRM 1.0), −1 for −Z (VRM 0.x).
    pub forward_z: f32,
}

/// The JSON part of a `.glb` / `.vrm` (or a `.gltf` file as a whole).
pub fn gltf_json(bytes: &[u8]) -> Option<Value> {
    if bytes.len() >= 20 && &bytes[0..4] == b"glTF" {
        let len = u32::from_le_bytes(bytes[12..16].try_into().ok()?) as usize;
        serde_json::from_slice(bytes.get(20..20 + len)?).ok()
    } else {
        serde_json::from_slice(bytes).ok()
    }
}

fn f(v: &Value, key: &str, default: f32) -> f32 {
    v.get(key).and_then(Value::as_f64).map_or(default, |x| x as f32)
}

fn vec3(v: Option<&Value>, default: Vec3) -> Vec3 {
    match v {
        Some(Value::Array(a)) if a.len() >= 3 => Vec3::new(a[0].as_f64().unwrap_or(0.0) as f32, a[1].as_f64().unwrap_or(0.0) as f32, a[2].as_f64().unwrap_or(0.0) as f32),
        Some(Value::Object(o)) => Vec3::new(
            o.get("x").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            o.get("y").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            o.get("z").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        ),
        _ => default,
    }
}

fn rgb(v: Option<&Value>, default: [f32; 3]) -> [f32; 3] {
    match v {
        Some(Value::Array(a)) if a.len() >= 3 => [a[0].as_f64().unwrap_or(0.0) as f32, a[1].as_f64().unwrap_or(0.0) as f32, a[2].as_f64().unwrap_or(0.0) as f32],
        _ => default,
    }
}

/// VRM 0.x → VRM 1.0 expression names.
fn vrm0_preset(name: &str) -> String {
    match name.to_ascii_lowercase().as_str() {
        "a" => "aa".into(),
        "i" => "ih".into(),
        "u" => "ou".into(),
        "e" => "ee".into(),
        "o" => "oh".into(),
        "joy" => "happy".into(),
        "sorrow" => "sad".into(),
        "fun" => "relaxed".into(),
        "blink_l" => "blinkLeft".into(),
        "blink_r" => "blinkRight".into(),
        "lookup" => "lookUp".into(),
        "lookdown" => "lookDown".into(),
        "lookleft" => "lookLeft".into(),
        "lookright" => "lookRight".into(),
        other => other.into(),
    }
}

/// Reads the VRM extensions from a glTF's JSON (`None` for a plain glTF).
/// `children[n]` lists node `n`'s children; `materials` the material count.
pub fn parse(json: &Value, children: &[Vec<usize>]) -> Option<Vrm> {
    let ext = json.get("extensions")?;
    let material_count = json.get("materials").and_then(Value::as_array).map_or(0, Vec::len);
    let node_mesh = |node: usize| json.get("nodes").and_then(|n| n.get(node)).and_then(|n| n.get("mesh")).and_then(Value::as_u64).map(|m| m as usize);
    let mut vrm = Vrm { toon: vec![None; material_count], ..Vrm::default() };
    if let Some(ms) = json.get("materials").and_then(Value::as_array) {
        vrm.material_names = ms.iter().map(|m| m.get("name").and_then(Value::as_str).unwrap_or("").to_string()).collect();
        vrm.emissive = ms
            .iter()
            .map(|m| {
                let f = |i: usize| m.get("emissiveFactor").and_then(|e| e.get(i)).and_then(Value::as_f64).unwrap_or(0.0) as f32;
                [f(0), f(1), f(2)]
            })
            .collect();
    }
    if let Some(v1) = ext.get("VRMC_vrm") {
        vrm.version = 1;
        vrm.forward_z = 1.0;
        vrm.name = v1.pointer("/meta/name").and_then(Value::as_str).unwrap_or("").to_string();
        if let Some(bones) = v1.pointer("/humanoid/humanBones").and_then(Value::as_object) {
            for (name, bone) in bones {
                if let Some(node) = bone.get("node").and_then(Value::as_u64) {
                    vrm.humanoid.insert(name.clone(), node as usize);
                }
            }
        }
        for group in ["preset", "custom"] {
            let Some(map) = v1.pointer(&format!("/expressions/{group}")).and_then(Value::as_object) else { continue };
            for (name, e) in map {
                let binds = e
                    .get("morphTargetBinds")
                    .and_then(Value::as_array)
                    .map(|binds| {
                        binds
                            .iter()
                            .filter_map(|b| Some((node_mesh(b.get("node")?.as_u64()? as usize)?, b.get("index")?.as_u64()? as usize, f(b, "weight", 1.0))))
                            .collect()
                    })
                    .unwrap_or_default();
                vrm.expressions.push(Expression { name: name.clone(), is_binary: e.get("isBinary").and_then(Value::as_bool).unwrap_or(false), binds });
            }
        }
        if let Some(sb) = ext.get("VRMC_springBone") {
            let colliders: Vec<Collider> = sb
                .get("colliders")
                .and_then(Value::as_array)
                .map(|cs| {
                    cs.iter()
                        .filter_map(|c| {
                            let node = c.get("node")?.as_u64()? as usize;
                            let shape = c.get("shape")?;
                            if let Some(s) = shape.get("sphere") {
                                Some(Collider { node, offset: vec3(s.get("offset"), Vec3::ZERO), radius: f(s, "radius", 0.0), tail: None })
                            } else {
                                let c = shape.get("capsule")?;
                                Some(Collider { node, offset: vec3(c.get("offset"), Vec3::ZERO), radius: f(c, "radius", 0.0), tail: Some(vec3(c.get("tail"), Vec3::ZERO)) })
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            let groups: Vec<Vec<usize>> = sb
                .get("colliderGroups")
                .and_then(Value::as_array)
                .map(|gs| gs.iter().map(|g| g.get("colliders").and_then(Value::as_array).map(|c| c.iter().filter_map(|i| i.as_u64().map(|i| i as usize)).collect()).unwrap_or_default()).collect())
                .unwrap_or_default();
            vrm.colliders = colliders;
            for spring in sb.get("springs").and_then(Value::as_array).into_iter().flatten() {
                let joints: Vec<SpringJoint> = spring
                    .get("joints")
                    .and_then(Value::as_array)
                    .map(|js| {
                        js.iter()
                            .filter_map(|j| {
                                Some(SpringJoint {
                                    node: j.get("node")?.as_u64()? as usize,
                                    hit_radius: f(j, "hitRadius", 0.0),
                                    stiffness: f(j, "stiffness", 1.0),
                                    gravity_power: f(j, "gravityPower", 0.0),
                                    gravity_dir: vec3(j.get("gravityDir"), Vec3::NEG_Y),
                                    drag: f(j, "dragForce", 0.5),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let colliders = spring
                    .get("colliderGroups")
                    .and_then(Value::as_array)
                    .map(|gs| gs.iter().filter_map(|g| groups.get(g.as_u64()? as usize)).flatten().copied().collect())
                    .unwrap_or_default();
                if joints.len() >= 2 {
                    vrm.springs.push(SpringChain { joints, colliders });
                }
            }
        }
        for (i, material) in json.get("materials").and_then(Value::as_array).into_iter().flatten().enumerate() {
            let Some(m) = material.pointer("/extensions/VRMC_materials_mtoon") else { continue };
            vrm.toon[i] = Some(Toon {
                shade: rgb(m.get("shadeColorFactor"), [0.6, 0.6, 0.6]),
                shift: f(m, "shadingShiftFactor", 0.0),
                toony: f(m, "shadingToonyFactor", 0.9),
                rim: rgb(m.get("parametricRimColorFactor"), [0.0; 3]),
                rim_power: f(m, "parametricRimFresnelPowerFactor", 5.0),
                rim_lift: f(m, "parametricRimLiftFactor", 0.0),
                outline_width: if m.get("outlineWidthMode").and_then(Value::as_str) == Some("worldCoordinates") { f(m, "outlineWidthFactor", 0.0) } else { f(m, "outlineWidthFactor", 0.0) * 0.5 },
                outline: rgb(m.get("outlineColorFactor"), [0.0; 3]),
            });
        }
        return Some(vrm);
    }

    // ---- VRM 0.x ----
    let v0 = ext.get("VRM")?;
    vrm.version = 0;
    vrm.forward_z = -1.0;
    vrm.name = v0.pointer("/meta/title").and_then(Value::as_str).unwrap_or("").to_string();
    for bone in v0.pointer("/humanoid/humanBones").and_then(Value::as_array).into_iter().flatten() {
        if let (Some(name), Some(node)) = (bone.get("bone").and_then(Value::as_str), bone.get("node").and_then(Value::as_u64)) {
            vrm.humanoid.insert(name.to_string(), node as usize);
        }
    }
    for group in v0.pointer("/blendShapeMaster/blendShapeGroups").and_then(Value::as_array).into_iter().flatten() {
        let preset = group.get("presetName").and_then(Value::as_str).unwrap_or("unknown");
        let name = if preset == "unknown" || preset.is_empty() { group.get("name").and_then(Value::as_str).unwrap_or("").to_string() } else { vrm0_preset(preset) };
        let binds = group
            .get("binds")
            .and_then(Value::as_array)
            .map(|bs| bs.iter().filter_map(|b| Some((b.get("mesh")?.as_u64()? as usize, b.get("index")?.as_u64()? as usize, f(b, "weight", 100.0) / 100.0))).collect())
            .unwrap_or_default();
        vrm.expressions.push(Expression { name, is_binary: group.get("isBinary").and_then(Value::as_bool).unwrap_or(false), binds });
    }
    // Collider groups: spheres on nodes (offsets in Unity space: flip z).
    let mut group_colliders: Vec<Vec<usize>> = Vec::new();
    for group in v0.pointer("/secondaryAnimation/colliderGroups").and_then(Value::as_array).into_iter().flatten() {
        let node = group.get("node").and_then(Value::as_u64).unwrap_or(0) as usize;
        let mut ids = Vec::new();
        for c in group.get("colliders").and_then(Value::as_array).into_iter().flatten() {
            let o = vec3(c.get("offset"), Vec3::ZERO);
            ids.push(vrm.colliders.len());
            vrm.colliders.push(Collider { node, offset: Vec3::new(o.x, o.y, -o.z), radius: f(c, "radius", 0.0), tail: None });
        }
        group_colliders.push(ids);
    }
    // Bone groups: every root bone's descendants form chains.
    for group in v0.pointer("/secondaryAnimation/boneGroups").and_then(Value::as_array).into_iter().flatten() {
        let colliders: Vec<usize> = group
            .get("colliderGroups")
            .and_then(Value::as_array)
            .map(|gs| gs.iter().filter_map(|g| group_colliders.get(g.as_u64()? as usize)).flatten().copied().collect())
            .unwrap_or_default();
        let gravity = vec3(group.get("gravityDir"), Vec3::NEG_Y);
        let joint = |node: usize| SpringJoint {
            node,
            hit_radius: f(group, "hitRadius", 0.02),
            stiffness: f(group, "stiffiness", 1.0),
            gravity_power: f(group, "gravityPower", 0.0),
            gravity_dir: Vec3::new(gravity.x, gravity.y, -gravity.z),
            drag: f(group, "dragForce", 0.4),
        };
        for root in group.get("bones").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64) {
            // Walk each branch: a chain per path from the root to a leaf.
            let mut stack = vec![vec![root as usize]];
            while let Some(path) = stack.pop() {
                let last = *path.last().unwrap();
                let kids = children.get(last).cloned().unwrap_or_default();
                if kids.is_empty() || path.len() > 32 {
                    if path.len() >= 2 {
                        vrm.springs.push(SpringChain { joints: path.iter().map(|&n| joint(n)).collect(), colliders: colliders.clone() });
                    }
                } else {
                    for (k, kid) in kids.iter().enumerate() {
                        // Branches after the first start at the fork (so a
                        // node isn't simulated twice).
                        let mut next = if k == 0 { path.clone() } else { vec![last] };
                        next.push(*kid);
                        stack.push(next);
                    }
                }
            }
        }
    }
    let names: Vec<String> = json.get("materials").and_then(Value::as_array).map(|ms| ms.iter().map(|m| m.get("name").and_then(Value::as_str).unwrap_or("").to_string()).collect()).unwrap_or_default();
    for props in v0.get("materialProperties").and_then(Value::as_array).into_iter().flatten() {
        if props.get("shader").and_then(Value::as_str) != Some("VRM/MToon") {
            continue;
        }
        let name = props.get("name").and_then(Value::as_str).unwrap_or("");
        let Some(i) = names.iter().position(|n| n == name) else { continue };
        let floats = props.get("floatProperties").cloned().unwrap_or(Value::Null);
        let vectors = props.get("vectorProperties").cloned().unwrap_or(Value::Null);
        // VRM 0.x MToon colours are sRGB; the renderer works in linear.
        let lin = |c: [f32; 3]| c.map(|v| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) });
        let shade_toony = f(&floats, "_ShadeToony", 0.9);
        let shade_shift = f(&floats, "_ShadeShift", 0.0);
        vrm.toon[i] = Some(Toon {
            shade: lin(rgb(vectors.get("_ShadeColor"), [0.8, 0.8, 0.8])),
            shift: shade_shift,
            toony: shade_toony,
            rim: lin(rgb(vectors.get("_RimColor"), [0.0; 3])),
            rim_power: f(&floats, "_RimFresnelPower", 1.0),
            rim_lift: f(&floats, "_RimLift", 0.0),
            // `_OutlineWidth` is in centimetres in world mode.
            outline_width: if f(&floats, "_OutlineWidthMode", 0.0) > 0.5 { f(&floats, "_OutlineWidth", 0.0) * 0.01 } else { 0.0 },
            outline: lin(rgb(vectors.get("_OutlineColor"), [0.0; 3])),
        });
    }
    Some(vrm)
}

impl Vrm {
    /// An expression's index by name (case-insensitive).
    pub fn expression(&self, name: &str) -> Option<usize> {
        self.expressions.iter().position(|e| e.name.eq_ignore_ascii_case(name))
    }
    /// A humanoid bone's node (exact name first, then case-insensitive).
    pub fn bone(&self, name: &str) -> Option<usize> {
        self.humanoid.get(name).copied().or_else(|| self.humanoid.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| *v))
    }
}

// --------------------------------------------------------------- shaders --

/// [`outline_hull`] for interleaved vertices (position, normal, UV): the
/// hull's vertices, written into `out` (same layout); its flipped indices
/// are [`outline_indices`].
pub fn outline_hull_interleaved(vertices: &[f32], width: f32, out: &mut Vec<f32>) {
    out.resize(vertices.len(), 0.0);
    for (src, dst) in vertices.chunks_exact(8).zip(out.chunks_exact_mut(8)) {
        dst[0] = src[0] + src[3] * width;
        dst[1] = src[1] + src[4] * width;
        dst[2] = src[2] + src[5] * width;
        dst[3] = -src[3];
        dst[4] = -src[4];
        dst[5] = -src[5];
        dst[6] = src[6];
        dst[7] = src[7];
    }
}

/// A mesh's triangles with their winding flipped (an outline hull's).
pub fn outline_indices(indices: &[u32]) -> Vec<u32> {
    indices.chunks(3).flat_map(|t| if t.len() == 3 { [t[0], t[2], t[1]] } else { [0, 0, 0] }).collect()
}

/// Builds the inverted hull for an outline from a (deformed) mesh: every
/// vertex pushed `width` along its normal, every triangle's winding
/// flipped, so only the shell's inside — a rim round the silhouette —
/// faces the camera.
pub fn outline_hull(mesh: &crate::Mesh3DData, width: f32, out: &mut crate::Mesh3DData) {
    out.positions.clear();
    out.positions.extend(mesh.positions.iter().zip(&mesh.normals).map(|(p, n)| [p[0] + n[0] * width, p[1] + n[1] * width, p[2] + n[2] * width]));
    out.normals.clear();
    out.normals.extend(mesh.normals.iter().map(|n| [-n[0], -n[1], -n[2]]));
    if out.indices.len() != mesh.indices.len() || out.uvs.len() != mesh.uvs.len() {
        out.uvs = mesh.uvs.clone();
        out.indices = mesh.indices.chunks(3).flat_map(|t| if t.len() == 3 { [t[0], t[2], t[1]] } else { [0, 0, 0] }).collect();
    }
}

// --------------------------------------------------------------- springs --

/// One simulated joint's state (world space).
#[derive(Debug, Clone, Copy)]
struct JointState {
    tail: Vec3,
    prev_tail: Vec3,
    /// The joint's rest local rotation, its bone direction (local, unit)
    /// and length to the tail.
    rest_rotation: Quat,
    axis: Vec3,
    length: f32,
}

/// The spring-bone simulation for one avatar instance.
#[derive(Debug, Clone, Default)]
pub struct SpringSim {
    /// Per chain, per joint (the last joint of a chain has no tail).
    state: Vec<Vec<JointState>>,
    ready: bool,
}

fn decompose(m: Mat4) -> (Vec3, Quat, Vec3) {
    let (s, r, t) = m.to_scale_rotation_translation();
    (t, if r.is_finite() { r.normalize() } else { Quat::IDENTITY }, s)
}

impl SpringSim {
    /// Starts (or restarts) from the current pose.
    pub fn reset(&mut self) {
        self.ready = false;
    }

    /// Advances the springs `dt` seconds and writes their joints' local
    /// rotations into `local` (the model's local node pose this frame).
    /// `world_of(local)` recomputes model-space node matrices; `instance`
    /// places the model in the world (so motion of the avatar swings its
    /// hair).
    pub fn step(&mut self, vrm: &Vrm, rest: &[(Vec3, Quat, Vec3)], local: &mut [(Vec3, Quat, Vec3)], instance: Mat4, dt: f32, world_of: &dyn Fn(&[(Vec3, Quat, Vec3)]) -> Vec<Mat4>, parents: &[Option<usize>]) {
        let dt = dt.clamp(0.0, 1.0 / 20.0);
        let mut world: Vec<Mat4> = world_of(local).into_iter().map(|m| instance * m).collect();
        if !self.ready {
            self.state = vrm
                .springs
                .iter()
                .map(|chain| {
                    chain
                        .joints
                        .iter()
                        .enumerate()
                        .map(|(k, j)| {
                            let head = world.get(j.node).map_or(Vec3::ZERO, |m| m.transform_point3(Vec3::ZERO));
                            // The tail: the next joint, or past the last one
                            // along its parent's direction.
                            let tail = match chain.joints.get(k + 1) {
                                Some(next) => world.get(next.node).map_or(head, |m| m.transform_point3(Vec3::ZERO)),
                                None => {
                                    let parent = parents.get(j.node).copied().flatten().and_then(|p| world.get(p)).map_or(head, |m| m.transform_point3(Vec3::ZERO));
                                    head + (head - parent).normalize_or_zero() * 0.07
                                }
                            };
                            let (_, rot, _) = decompose(*world.get(j.node).unwrap_or(&Mat4::IDENTITY));
                            let axis_world = (tail - head).normalize_or(Vec3::NEG_Y);
                            let axis = (rot.inverse() * axis_world).normalize_or(Vec3::NEG_Y);
                            JointState { tail, prev_tail: tail, rest_rotation: rest.get(j.node).map_or(Quat::IDENTITY, |r| r.1), axis, length: (tail - head).length().max(1e-4) }
                        })
                        .collect()
                })
                .collect();
            self.ready = true;
        }
        for (chain, states) in vrm.springs.iter().zip(self.state.iter_mut()) {
            for (k, joint) in chain.joints.iter().enumerate() {
                let Some(state) = states.get_mut(k) else { continue };
                let Some(&node_world) = world.get(joint.node) else { continue };
                let head = node_world.transform_point3(Vec3::ZERO);
                let parent_rot = parents.get(joint.node).copied().flatten().and_then(|p| world.get(p)).map_or(Quat::IDENTITY, |m| decompose(*m).1);
                // Where the bone would point at rest under its parent now.
                let rest_dir = (parent_rot * state.rest_rotation * state.axis).normalize_or_zero();
                let inertia = (state.tail - state.prev_tail) * (1.0 - joint.drag.clamp(0.0, 1.0));
                let stiffness = rest_dir * joint.stiffness * dt;
                let gravity = joint.gravity_dir.normalize_or_zero() * joint.gravity_power * dt;
                let mut next = state.tail + inertia + stiffness + gravity;
                // Keep the bone's length.
                next = head + (next - head).normalize_or(rest_dir) * state.length;
                // Out of the colliders.
                for &c in &chain.colliders {
                    let Some(col) = vrm.colliders.get(c) else { continue };
                    let Some(m) = world.get(col.node) else { continue };
                    let a = m.transform_point3(col.offset);
                    let center = match col.tail {
                        Some(t) => {
                            let b = m.transform_point3(t);
                            let ab = b - a;
                            a + ab * ((next - a).dot(ab) / ab.length_squared().max(1e-8)).clamp(0.0, 1.0)
                        }
                        None => a,
                    };
                    let scale = m.x_axis.truncate().length();
                    let reach = col.radius * scale + joint.hit_radius;
                    let d = next - center;
                    if d.length() < reach {
                        next = center + d.normalize_or(Vec3::Y) * reach;
                        next = head + (next - head).normalize_or(rest_dir) * state.length;
                    }
                }
                state.prev_tail = state.tail;
                state.tail = next;
                // Turn the joint so its bone points at the new tail.
                let to = (next - head).normalize_or(rest_dir);
                let delta = Quat::from_rotation_arc(rest_dir, to);
                let world_rot = delta * parent_rot * state.rest_rotation;
                if let Some(l) = local.get_mut(joint.node) {
                    l.1 = (parent_rot.inverse() * world_rot).normalize();
                }
                // The next joint (this one's child) sees the new rotation:
                // refresh just this node's matrix (its children are
                // computed from it as the chain continues; the caller
                // rebuilds the full pose afterwards).
                if let Some(&(t, r, sc)) = local.get(joint.node) {
                    let parent_world = parents.get(joint.node).copied().flatten().and_then(|p| world.get(p).copied()).unwrap_or(instance);
                    world[joint.node] = parent_world * Mat4::from_scale_rotation_translation(sc, r, t);
                    if let Some(next) = chain.joints.get(k + 1) {
                        if let Some(&(t2, r2, s2)) = local.get(next.node) {
                            if parents.get(next.node).copied().flatten() == Some(joint.node) {
                                world[next.node] = world[joint.node] * Mat4::from_scale_rotation_translation(s2, r2, t2);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_vrm1_humanoid_expressions_springs_and_mtoon() {
        let json: Value = serde_json::from_str(
            r#"{"nodes":[{"name":"hips"},{"name":"head","mesh":0},{"name":"hair1"},{"name":"hair2"}],
            "materials":[{"name":"skin","extensions":{"VRMC_materials_mtoon":{"shadeColorFactor":[0.5,0.4,0.4],"shadingToonyFactor":0.95,"outlineWidthMode":"worldCoordinates","outlineWidthFactor":0.002}}}],
            "extensions":{"VRMC_vrm":{"meta":{"name":"Test"},"humanoid":{"humanBones":{"hips":{"node":0},"head":{"node":1}}},
              "expressions":{"preset":{"happy":{"morphTargetBinds":[{"node":1,"index":3,"weight":1}]}}}},
              "VRMC_springBone":{"colliders":[{"node":1,"shape":{"sphere":{"offset":[0,0.1,0],"radius":0.1}}}],"colliderGroups":[{"colliders":[0]}],
              "springs":[{"joints":[{"node":2,"stiffness":0.8,"gravityPower":0.2},{"node":3}],"colliderGroups":[0]}]}}}"#,
        )
        .unwrap();
        let vrm = parse(&json, &[vec![1], vec![2], vec![3], vec![]]).expect("is a VRM");
        assert_eq!(vrm.version, 1);
        assert_eq!(vrm.bone("HEAD"), Some(1));
        let happy = &vrm.expressions[vrm.expression("happy").unwrap()];
        assert_eq!(happy.binds, vec![(0, 3, 1.0)]);
        assert_eq!(vrm.springs.len(), 1);
        assert_eq!(vrm.springs[0].colliders, vec![0]);
        let toon = vrm.toon[0].unwrap();
        assert!((toon.toony - 0.95).abs() < 1e-6 && (toon.outline_width - 0.002).abs() < 1e-6);
    }

    #[test]
    fn vrm0_presets_map_to_vrm1_names() {
        assert_eq!(vrm0_preset("Joy"), "happy");
        assert_eq!(vrm0_preset("A"), "aa");
        assert_eq!(vrm0_preset("Blink_L"), "blinkLeft");
    }
}
