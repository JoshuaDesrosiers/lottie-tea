//! **Humanoid animation retargeting**: play an animation made for one
//! skeleton on any VRM avatar.
//!
//! Sources:
//! - **VRMA** (`.vrma`, the VRM Animation format): a glTF whose
//!   `VRMC_vrm_animation` extension maps humanoid bones to its nodes.
//! - **VRM / glTF avatars with clips**: a VRM's own humanoid map, or —
//!   for plain glTF / GLB (e.g. a Mixamo character exported through
//!   Blender) — bones recognised by name (`mixamorig:LeftForeArm`,
//!   `LeftLowerArm`, `upper_arm.L` …).
//!
//! The trick is the *normalized* humanoid: a skeleton where every bone's
//! rest rotation is identity in a shared, world-aligned frame (the avatar
//! facing +Z, its left at +X — VRM 1.0's convention). For a source bone
//! with rest world rotation `W` (and parent `Wp`) animated to local `q`,
//! its normalized local rotation is `Wp · q · W⁻¹` (identity at rest).
//! Playing it on a target bone with rest world rotations `Tp`, `T` turns
//! it back into that bone's own local rotation, `Tp⁻¹ · n · T`. So any two
//! skeletons with the same humanoid bones share motion regardless of how
//! their bones happen to be oriented. Hips translation is carried as an
//! offset from rest, measured in hips heights (so a short avatar's jump is
//! shorter). Skeletons facing −Z (VRM 0.x) are turned 180° on the way.

use std::collections::HashMap;

use glam::{Mat4, Quat, Vec3};
use serde_json::Value;

use crate::model3d::{ChannelPath, ModelData};

/// One humanoid bone's motion: normalized local rotations over time.
#[derive(Debug, Clone)]
pub struct BoneTrack {
    pub bone: String,
    pub times: Vec<f32>,
    pub rotations: Vec<Quat>,
    pub step: bool,
}

/// A retargetable clip.
#[derive(Debug, Clone)]
pub struct HumanoidClip {
    pub name: String,
    pub duration: f32,
    pub bones: Vec<BoneTrack>,
    /// The hips' offset from rest over time, in hips heights (normalized
    /// frame) — root motion, bobs, jumps, crouches.
    pub hips: Option<(Vec<f32>, Vec<Vec3>)>,
}

/// A clip sampled at one time: bone → normalized local rotation, and the
/// hips offset (hips heights).
#[derive(Debug, Clone, Default)]
pub struct HumanoidPose {
    pub bones: HashMap<String, Quat>,
    pub hips: Option<Vec3>,
}

/// Every humanoid bone name (VRM 1.0), for name matching.
pub const HUMAN_BONES: &[&str] = &[
    "hips", "spine", "chest", "upperChest", "neck", "head", "jaw", "leftEye", "rightEye",
    "leftShoulder", "leftUpperArm", "leftLowerArm", "leftHand", "rightShoulder", "rightUpperArm", "rightLowerArm", "rightHand",
    "leftUpperLeg", "leftLowerLeg", "leftFoot", "leftToes", "rightUpperLeg", "rightLowerLeg", "rightFoot", "rightToes",
    "leftThumbMetacarpal", "leftThumbProximal", "leftThumbDistal", "leftIndexProximal", "leftIndexIntermediate", "leftIndexDistal",
    "leftMiddleProximal", "leftMiddleIntermediate", "leftMiddleDistal", "leftRingProximal", "leftRingIntermediate", "leftRingDistal",
    "leftLittleProximal", "leftLittleIntermediate", "leftLittleDistal",
    "rightThumbMetacarpal", "rightThumbProximal", "rightThumbDistal", "rightIndexProximal", "rightIndexIntermediate", "rightIndexDistal",
    "rightMiddleProximal", "rightMiddleIntermediate", "rightMiddleDistal", "rightRingProximal", "rightRingIntermediate", "rightRingDistal",
    "rightLittleProximal", "rightLittleIntermediate", "rightLittleDistal",
];

/// A bone's humanoid name from a common rig naming scheme (Mixamo,
/// Unity-style, Blender Rigify-ish), or `None`.
pub fn humanoid_name(raw: &str) -> Option<&'static str> {
    // Drop namespaces / armature prefixes ("mixamorig:", "Armature|").
    let base = raw.rsplit([':', '|']).next().unwrap_or(raw);
    let key: String = base.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
    if let Some(b) = HUMAN_BONES.iter().find(|b| b.to_ascii_lowercase() == key) {
        return Some(b);
    }
    // Blender-style suffixes: "upper_arm.L", "thigh_r".
    let (side, stem) = if let Some(s) = key.strip_suffix('l').filter(|_| base.ends_with(".L") || base.ends_with("_l") || base.ends_with("_L")) {
        ("left", s.to_string())
    } else if let Some(s) = key.strip_suffix('r').filter(|_| base.ends_with(".R") || base.ends_with("_r") || base.ends_with("_R")) {
        ("right", s.to_string())
    } else if let Some(s) = key.strip_prefix("left") {
        ("left", s.to_string())
    } else if let Some(s) = key.strip_prefix("right") {
        ("right", s.to_string())
    } else {
        ("", key.clone())
    };
    let part: &str = match stem.as_str() {
        // Mixamo
        "hips" | "pelvis" => "hips",
        "spine" => "spine",
        "spine1" | "spine01" | "chest" => "chest",
        "spine2" | "spine02" | "upperchest" => "upperChest",
        "neck" => "neck",
        "head" => "head",
        "shoulder" | "clavicle" => "Shoulder",
        "arm" | "upperarm" => "UpperArm",
        "forearm" | "lowerarm" => "LowerArm",
        "hand" => "Hand",
        "upleg" | "thigh" | "upperleg" => "UpperLeg",
        "leg" | "shin" | "calf" | "lowerleg" => "LowerLeg",
        "foot" => "Foot",
        "toebase" | "toe" | "toes" => "Toes",
        _ => {
            // Fingers: "HandIndex1" … "HandThumb3".
            let s = stem.strip_prefix("hand").unwrap_or(&stem);
            let (finger, rest) = ["thumb", "index", "middle", "ring", "pinky", "little"].iter().find_map(|f| s.strip_prefix(f).map(|r| (*f, r)))?;
            let finger = if finger == "pinky" { "Little" } else { &finger[..] };
            let cap = |f: &str| format!("{}{}", f[..1].to_ascii_uppercase(), &f[1..]);
            let joint = match (finger, rest) {
                ("thumb", "1") => "Metacarpal",
                ("thumb", "2") => "Proximal",
                ("thumb", "3") => "Distal",
                (_, "1") => "Proximal",
                (_, "2") => "Intermediate",
                (_, "3") => "Distal",
                _ => return None,
            };
            let name = format!("{side}{}{joint}", cap(finger));
            return HUMAN_BONES.iter().find(|b| **b == name).copied();
        }
    };
    let name = if part.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
        if side.is_empty() {
            return None;
        }
        format!("{side}{part}")
    } else {
        part.to_string()
    };
    HUMAN_BONES.iter().find(|b| **b == name).copied()
}

/// Rest world rotations / matrices of every node.
pub fn rest_world(data: &ModelData) -> Vec<Mat4> {
    data.world_pose(&data.rest_pose())
}

/// The retargetable clips in an animation file (VRMA, or a VRM / glTF
/// with a humanoid skeleton and animations). `json` is the file's glTF
/// JSON (for the VRMA / VRM humanoid map).
pub fn humanoid_clips(data: &ModelData, json: Option<&Value>) -> Result<Vec<HumanoidClip>, String> {
    // Humanoid bone → node.
    let mut map: HashMap<&'static str, usize> = HashMap::new();
    let vrma = json.and_then(|j| j.pointer("/extensions/VRMC_vrm_animation/humanoid/humanBones")).and_then(Value::as_object);
    let mut faces_neg_z = false;
    if let Some(bones) = vrma {
        for (name, b) in bones {
            if let (Some(bone), Some(node)) = (HUMAN_BONES.iter().find(|h| *h == name), b.get("node").and_then(Value::as_u64)) {
                map.insert(bone, node as usize);
            }
        }
    } else if let Some(vrm) = &data.vrm {
        faces_neg_z = vrm.forward_z < 0.0;
        for (name, &node) in &vrm.humanoid {
            if let Some(bone) = HUMAN_BONES.iter().find(|h| *h == name) {
                map.insert(bone, node);
            }
        }
    } else {
        for (i, n) in data.nodes.iter().enumerate() {
            if let Some(bone) = humanoid_name(&n.name) {
                map.entry(bone).or_insert(i);
            }
        }
    }
    let Some(&hips) = map.get("hips") else { return Err("no humanoid skeleton found (no hips bone)".into()) };
    if data.animations.is_empty() {
        return Err("the file has no animations".into());
    }
    let world = rest_world(data);
    let rot = |m: &Mat4| m.to_scale_rotation_translation().1;
    let parent_world = |i: usize| data.nodes.get(i).and_then(|n| n.parent).map_or(Mat4::IDENTITY, |p| world[p]);
    let hips_rest = world[hips].transform_point3(Vec3::ZERO);
    // Hips height above the skeleton's feet (or its origin).
    let feet = ["leftFoot", "rightFoot"].iter().filter_map(|b| map.get(b)).map(|&i| world[i].transform_point3(Vec3::ZERO).y).fold(f32::INFINITY, f32::min);
    let ground = if feet.is_finite() { feet.min(0.0).min(hips_rest.y) } else { 0.0 };
    let hips_height = (hips_rest.y - ground).max(1e-3);
    let turn = Quat::from_rotation_y(std::f32::consts::PI);
    let node_bone: HashMap<usize, &'static str> = map.iter().map(|(b, n)| (*n, *b)).collect();

    let mut clips = Vec::new();
    for (ci, clip) in data.animations.iter().enumerate() {
        let mut bones = Vec::new();
        let mut hips_track = None;
        for ch in &clip.channels {
            let Some(&bone) = node_bone.get(&ch.node) else { continue };
            match ch.path {
                ChannelPath::Rotation => {
                    let wp = rot(&parent_world(ch.node));
                    let w = rot(&world[ch.node]);
                    let rotations = ch
                        .values
                        .iter()
                        .map(|v| {
                            let q = Quat::from_xyzw(v.x, v.y, v.z, v.w).normalize();
                            let n = (wp * q * w.inverse()).normalize();
                            if faces_neg_z { (turn * n * turn.inverse()).normalize() } else { n }
                        })
                        .collect();
                    bones.push(BoneTrack { bone: bone.to_string(), times: ch.times.clone(), rotations, step: ch.step });
                }
                ChannelPath::Translation if bone == "hips" => {
                    let pw = parent_world(ch.node);
                    let values = ch
                        .values
                        .iter()
                        .map(|v| {
                            let p = pw.transform_point3(Vec3::new(v.x, v.y, v.z));
                            let d = (p - hips_rest) / hips_height;
                            if faces_neg_z { Vec3::new(-d.x, d.y, -d.z) } else { d }
                        })
                        .collect();
                    hips_track = Some((ch.times.clone(), values));
                }
                _ => {}
            }
        }
        if bones.is_empty() && hips_track.is_none() {
            continue;
        }
        let name = if clip.name.is_empty() { format!("Clip {}", ci + 1) } else { clip.name.clone() };
        clips.push(HumanoidClip { name, duration: clip.duration.max(1e-3), bones, hips: hips_track });
    }
    if clips.is_empty() {
        return Err("no animation moves a humanoid bone".into());
    }
    Ok(clips)
}

fn sample_index(times: &[f32], t: f32) -> (usize, usize, f32) {
    if times.len() <= 1 || t <= times[0] {
        return (0, 0, 0.0);
    }
    let last = times.len() - 1;
    if t >= times[last] {
        return (last, last, 0.0);
    }
    let i = times.partition_point(|&x| x <= t).saturating_sub(1);
    let span = (times[i + 1] - times[i]).max(1e-6);
    (i, i + 1, (t - times[i]) / span)
}

impl HumanoidClip {
    /// The pose at time `t` (seconds, clamped to the clip).
    pub fn sample(&self, t: f32) -> HumanoidPose {
        let mut pose = HumanoidPose::default();
        for b in &self.bones {
            if b.rotations.is_empty() {
                continue;
            }
            let (i, j, u) = sample_index(&b.times, t);
            let q = if b.step || i == j { b.rotations[i] } else { b.rotations[i].slerp(b.rotations[j], u) };
            pose.bones.insert(b.bone.clone(), q);
        }
        if let Some((times, values)) = &self.hips {
            if !values.is_empty() {
                let (i, j, u) = sample_index(times, t);
                pose.hips = Some(values[i].lerp(values[j], u));
            }
        }
        pose
    }
}

impl HumanoidPose {
    /// `self` blended toward `other` by `t` (bones only in one keep it).
    pub fn blend(&self, other: &HumanoidPose, t: f32) -> HumanoidPose {
        let mut bones = self.bones.clone();
        for (b, q) in &other.bones {
            let from = bones.get(b).copied().unwrap_or(Quat::IDENTITY);
            bones.insert(b.clone(), from.slerp(*q, t));
        }
        for (b, q) in &self.bones {
            if !other.bones.contains_key(b) {
                bones.insert(b.clone(), q.slerp(Quat::IDENTITY, t));
            }
        }
        let hips = match (self.hips, other.hips) {
            (Some(a), Some(b)) => Some(a.lerp(b, t)),
            (Some(a), None) => Some(a * (1.0 - t)),
            (None, Some(b)) => Some(b * t),
            (None, None) => None,
        };
        HumanoidPose { bones, hips }
    }
}

/// A target skeleton, ready to receive normalized poses.
#[derive(Debug, Clone)]
pub struct HumanoidTarget {
    /// Humanoid bone → (node, parent rest world rotation, rest world rotation).
    pub bones: HashMap<String, (usize, Quat, Quat)>,
    pub hips: Option<usize>,
    /// The hips' parent rest world matrix, and the hips height (m).
    pub hips_parent: Mat4,
    pub hips_height: f32,
    pub faces_neg_z: bool,
}

impl HumanoidTarget {
    /// From a VRM avatar (`None` if it isn't one).
    pub fn of(data: &ModelData) -> Option<Self> {
        let vrm = data.vrm.as_ref()?;
        let world = rest_world(data);
        let rot = |m: &Mat4| m.to_scale_rotation_translation().1;
        let mut bones = HashMap::new();
        for (name, &node) in &vrm.humanoid {
            if node >= world.len() {
                continue;
            }
            let parent = data.nodes[node].parent.map_or(Quat::IDENTITY, |p| rot(&world[p]));
            bones.insert(name.clone(), (node, parent, rot(&world[node])));
        }
        let hips = vrm.bone("hips");
        let (hips_parent, hips_height) = match hips {
            Some(h) => {
                let parent = data.nodes[h].parent.map_or(Mat4::IDENTITY, |p| world[p]);
                let y = world[h].transform_point3(Vec3::ZERO).y;
                let feet = ["leftFoot", "rightFoot"].iter().filter_map(|b| vrm.bone(b)).map(|i| world[i].transform_point3(Vec3::ZERO).y).fold(f32::INFINITY, f32::min);
                let ground = if feet.is_finite() { feet.min(0.0).min(y) } else { 0.0 };
                (parent, (y - ground).max(1e-3))
            }
            None => (Mat4::IDENTITY, 1.0),
        };
        Some(Self { bones, hips, hips_parent, hips_height, faces_neg_z: vrm.forward_z < 0.0 })
    }

    /// Writes `pose` into a local pose (`(translation, rotation, scale)`
    /// per node), with `weight` 0..1 blending from what's there. `rest`
    /// is the skeleton's rest pose (hips translation is rest + offset).
    pub fn apply(&self, pose: &HumanoidPose, weight: f32, rest: &[(Vec3, Quat, Vec3)], local: &mut [(Vec3, Quat, Vec3)]) {
        let turn = Quat::from_rotation_y(std::f32::consts::PI);
        for (name, n) in &pose.bones {
            let Some(&(node, parent, own)) = self.bones.get(name) else { continue };
            let n = if self.faces_neg_z { turn * *n * turn.inverse() } else { *n };
            let q = (parent.inverse() * n * own).normalize();
            if let Some(l) = local.get_mut(node) {
                l.1 = if weight >= 1.0 { q } else { l.1.slerp(q, weight) };
            }
        }
        if let (Some(h), Some(d)) = (self.hips, pose.hips) {
            let d = if self.faces_neg_z { Vec3::new(-d.x, d.y, -d.z) } else { d } * self.hips_height;
            let offset = self.hips_parent.inverse().transform_vector3(d);
            if let (Some(l), Some(r)) = (local.get_mut(h), rest.get(h)) {
                let target = r.0 + offset;
                l.0 = if weight >= 1.0 { target } else { l.0.lerp(target, weight) };
            }
        }
    }
}

/// One bone of a skeleton written by [`write_vrma`]: its humanoid name,
/// parent (index into the same list) and offset from that parent.
#[derive(Debug, Clone, Copy)]
pub struct VrmaBone {
    pub bone: &'static str,
    pub parent: Option<usize>,
    pub offset: Vec3,
}

/// Writes `clips` as a **VRMA** file (VRM Animation, binary glTF) — the
/// inverse of [`humanoid_clips`], so generated or edited motion becomes an
/// ordinary animation asset any avatar can play through the Humanoid
/// Animator.
///
/// The skeleton is written in the *normalized* frame (every rest rotation
/// identity, facing +Z), so a clip's rotations are stored as they are and
/// read back unchanged. `skeleton[0]` must be the hips; its hips offsets
/// (in hips heights) become translation keys. Bones a clip doesn't move
/// stay at rest.
pub fn write_vrma(skeleton: &[VrmaBone], clips: &[HumanoidClip]) -> Vec<u8> {
    use serde_json::json;
    let mut bin: Vec<u8> = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    // One tightly packed float accessor per array.
    let mut push = |values: &[f32], kind: &str, width: usize, bounds: bool| -> usize {
        let offset = bin.len();
        for v in values {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        views.push(json!({ "buffer": 0, "byteOffset": offset, "byteLength": values.len() * 4 }));
        let mut accessor = json!({ "bufferView": views.len() - 1, "componentType": 5126, "count": values.len() / width, "type": kind });
        if bounds && !values.is_empty() {
            let (lo, hi) = values.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
            accessor["min"] = json!([lo]);
            accessor["max"] = json!([hi]);
        }
        accessors.push(accessor);
        accessors.len() - 1
    };

    // World rest positions (for the hips height, as the reader measures it).
    let mut world: Vec<Vec3> = Vec::with_capacity(skeleton.len());
    for b in skeleton {
        world.push(b.parent.map_or(Vec3::ZERO, |p| world[p]) + b.offset);
    }
    let hips_rest = world.first().copied().unwrap_or(Vec3::Y);
    let feet = skeleton.iter().zip(&world).filter(|(b, _)| b.bone == "leftFoot" || b.bone == "rightFoot").map(|(_, w)| w.y).fold(f32::INFINITY, f32::min);
    let ground = if feet.is_finite() { feet.min(0.0).min(hips_rest.y) } else { 0.0 };
    let hips_height = (hips_rest.y - ground).max(1e-3);

    let mut animations = Vec::new();
    for clip in clips {
        let mut samplers = Vec::new();
        let mut channels = Vec::new();
        for track in &clip.bones {
            let Some(node) = skeleton.iter().position(|b| b.bone == track.bone) else { continue };
            if track.times.is_empty() || track.times.len() != track.rotations.len() {
                continue;
            }
            let input = push(&track.times, "SCALAR", 1, true);
            let values: Vec<f32> = track.rotations.iter().flat_map(|q| q.normalize().to_array()).collect();
            let output = push(&values, "VEC4", 4, false);
            samplers.push(json!({ "input": input, "output": output, "interpolation": if track.step { "STEP" } else { "LINEAR" } }));
            channels.push(json!({ "sampler": samplers.len() - 1, "target": { "node": node, "path": "rotation" } }));
        }
        if let Some((times, offsets)) = &clip.hips {
            if !times.is_empty() && times.len() == offsets.len() {
                let input = push(times, "SCALAR", 1, true);
                let values: Vec<f32> = offsets.iter().flat_map(|d| (skeleton[0].offset + *d * hips_height).to_array()).collect();
                let output = push(&values, "VEC3", 3, false);
                samplers.push(json!({ "input": input, "output": output, "interpolation": "LINEAR" }));
                channels.push(json!({ "sampler": samplers.len() - 1, "target": { "node": 0, "path": "translation" } }));
            }
        }
        if !channels.is_empty() {
            animations.push(json!({ "name": clip.name, "samplers": samplers, "channels": channels }));
        }
    }

    let nodes: Vec<Value> = skeleton
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let children: Vec<usize> = skeleton.iter().enumerate().filter(|(_, c)| c.parent == Some(i)).map(|(j, _)| j).collect();
            let mut node = json!({ "name": b.bone, "translation": b.offset.to_array() });
            if !children.is_empty() {
                node["children"] = json!(children);
            }
            node
        })
        .collect();
    let human_bones: serde_json::Map<String, Value> = skeleton.iter().enumerate().map(|(i, b)| (b.bone.to_string(), json!({ "node": i }))).collect();
    let roots: Vec<usize> = skeleton.iter().enumerate().filter(|(_, b)| b.parent.is_none()).map(|(i, _)| i).collect();
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let doc = json!({
        "asset": { "version": "2.0", "generator": "an1mbyt3" },
        "extensionsUsed": ["VRMC_vrm_animation"],
        "extensions": { "VRMC_vrm_animation": { "specVersion": "1.0", "humanoid": { "humanBones": human_bones } } },
        "scene": 0,
        "scenes": [{ "nodes": roots }],
        "nodes": nodes,
        "buffers": [{ "byteLength": bin.len() }],
        "bufferViews": views,
        "accessors": accessors,
        "animations": animations,
    });
    let mut text = serde_json::to_vec(&doc).expect("JSON values always serialize");
    while text.len() % 4 != 0 {
        text.push(b' ');
    }
    // GLB: header, JSON chunk, BIN chunk.
    let total = 12 + 8 + text.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&text);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model3d::{ModelChannel, ModelClip, ModelNode};
    use glam::Vec4;

    fn node(name: &str, parent: Option<usize>, t: Vec3, r: Quat) -> ModelNode {
        ModelNode { name: name.into(), parent, translation: t, rotation: r, scale: Vec3::ONE }
    }

    #[test]
    fn names_from_common_rigs() {
        assert_eq!(humanoid_name("mixamorig:LeftForeArm"), Some("leftLowerArm"));
        assert_eq!(humanoid_name("mixamorig:Spine2"), Some("upperChest"));
        assert_eq!(humanoid_name("mixamorig:RightHandIndex2"), Some("rightIndexIntermediate"));
        assert_eq!(humanoid_name("mixamorig:LeftHandThumb1"), Some("leftThumbMetacarpal"));
        assert_eq!(humanoid_name("LeftUpLeg"), Some("leftUpperLeg"));
        assert_eq!(humanoid_name("hips"), Some("hips"));
        assert_eq!(humanoid_name("upper_arm.L"), Some("leftUpperArm"));
        assert_eq!(humanoid_name("Armature"), None);
    }

    /// A source arm with a rotated rest pose and a target arm with a
    /// different one: the same world-space motion comes out.
    #[test]
    fn retargets_across_rest_orientations() {
        // Source: hips turned 90° about X (like the VRMA sample), an upper
        // arm turned another way; the clip raises the arm 60° about Z in
        // world space.
        let hips_rest = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        let arm_rest = Quat::from_rotation_y(0.7);
        let world_arm_rest = hips_rest * arm_rest;
        let raise = Quat::from_rotation_z(1.0472);
        // Animated local = parent⁻¹ · raise · world rest.
        let anim_local = hips_rest.inverse() * raise * world_arm_rest;
        let src = ModelData {
            nodes: vec![node("Hips", None, Vec3::new(0.0, 1.0, 0.0), hips_rest), node("LeftArm", Some(0), Vec3::new(0.1, 0.0, 0.3), arm_rest)],
            animations: vec![ModelClip { name: "raise".into(), duration: 1.0, channels: vec![ModelChannel { node: 1, path: ChannelPath::Rotation, times: vec![0.0, 1.0], values: vec![Vec4::new(arm_rest.x, arm_rest.y, arm_rest.z, arm_rest.w), Vec4::new(anim_local.x, anim_local.y, anim_local.z, anim_local.w)], step: false }] }],
            ..ModelData::default()
        };
        let clips = humanoid_clips(&src, None).expect("clip");
        let end = clips[0].sample(1.0);
        let n = end.bones["leftUpperArm"];
        assert!(n.angle_between(raise) < 1e-3, "normalized = world-space raise");
        // Target: identity hips, arm with its own rest twist.
        let t_arm = Quat::from_rotation_x(-0.4);
        let target = HumanoidTarget {
            bones: HashMap::from([("leftUpperArm".to_string(), (1, Quat::IDENTITY, t_arm))]),
            hips: None,
            hips_parent: Mat4::IDENTITY,
            hips_height: 1.0,
            faces_neg_z: false,
        };
        let rest = vec![(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE), (Vec3::ZERO, t_arm, Vec3::ONE)];
        let mut local = rest.clone();
        target.apply(&end, 1.0, &rest, &mut local);
        // Target world = raise · target rest world.
        assert!((local[1].1).angle_between(raise * t_arm) < 1e-3);
    }

    #[test]
    fn vrma_round_trip() {
        let skeleton = [
            VrmaBone { bone: "hips", parent: None, offset: Vec3::new(0.0, 1.0, 0.0) },
            VrmaBone { bone: "leftUpperLeg", parent: Some(0), offset: Vec3::new(0.1, -0.05, 0.0) },
            VrmaBone { bone: "leftLowerLeg", parent: Some(1), offset: Vec3::new(0.0, -0.45, 0.0) },
            VrmaBone { bone: "leftFoot", parent: Some(2), offset: Vec3::new(0.0, -0.45, 0.0) },
        ];
        let bend = Quat::from_rotation_x(0.6);
        let clip = HumanoidClip {
            name: "Kneel".into(),
            duration: 1.0,
            bones: vec![BoneTrack { bone: "leftLowerLeg".into(), times: vec![0.0, 1.0], rotations: vec![Quat::IDENTITY, bend], step: false }],
            hips: Some((vec![0.0, 1.0], vec![Vec3::ZERO, Vec3::new(0.0, -0.2, 0.1)])),
        };
        let bytes = write_vrma(&skeleton, &[clip]);
        let data = crate::parse_gltf(&bytes, &|_| None).expect("parses");
        let json = crate::vrm::gltf_json(&bytes);
        let clips = humanoid_clips(&data, json.as_ref()).expect("has clips");
        assert_eq!(clips[0].name, "Kneel");
        let end = clips[0].sample(1.0);
        assert!(end.bones["leftLowerLeg"].angle_between(bend) < 1e-4);
        assert!((end.hips.unwrap() - Vec3::new(0.0, -0.2, 0.1)).length() < 1e-4);
    }
}
