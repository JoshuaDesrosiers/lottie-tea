//! glTF 2.0 / GLB model import — meshes, textures, the node hierarchy and
//! node (rigid-part) animations.
//!
//! [`parse_gltf`] turns a model file into a [`ModelData`]: every mesh
//! primitive as a [`Mesh3DData`] attached to the node that holds it, the
//! base colour (factor + texture), the node tree's rest pose, and every
//! animation clip. [`ModelData::pose`] evaluates a clip at a time and gives
//! each node's model-space matrix — enough to play rigid-part animated
//! characters (like Kenney's blocky characters: separate head / arm / leg
//! nodes animated by keyframes).
//!
//! **Skinned meshes** (characters with a skeleton — every VRM avatar) keep
//! their skin (joint nodes + inverse bind matrices), per-vertex joints and
//! weights, and morph targets (blend shapes); [`ModelData::skin_primitive`]
//! deforms one on the CPU for a pose + morph weights. [`ModelData::vrm`]
//! holds a VRM's humanoid bones, expressions, spring bones and MToon
//! parameters ([`crate::vrm`]).
//!
//! External files (`Textures/x.png`, a `.bin` buffer) come from `resolve`,
//! so this works on any platform: the editor looks them up in the
//! document's [`an1mbyt3_core::ModelAsset`]. Images are returned *encoded*
//! (PNG / JPEG bytes) for the caller to decode.

use glam::{Mat4, Quat, Vec3, Vec4};

use crate::Mesh3DData;

/// A model, ready to draw.
#[derive(Debug, Clone, Default)]
pub struct ModelData {
    pub nodes: Vec<ModelNode>,
    pub primitives: Vec<ModelPrimitive>,
    /// Encoded image files (PNG / JPEG), by glTF image index.
    pub images: Vec<Vec<u8>>,
    pub animations: Vec<ModelClip>,
    /// Model-space bounds of the rest pose.
    pub bounds: (Vec3, Vec3),
    pub skins: Vec<ModelSkin>,
    /// The VRM extensions, for VRM avatars.
    pub vrm: Option<crate::vrm::Vrm>,
}

/// A skeleton: its joint nodes and their inverse bind matrices.
#[derive(Debug, Clone)]
pub struct ModelSkin {
    pub joints: Vec<usize>,
    pub inverse_bind: Vec<Mat4>,
}

/// How a primitive's alpha is used (glTF `alphaMode`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlphaMode {
    Opaque,
    /// Cut out below this alpha (hair strands, lashes).
    Mask(f32),
    Blend,
}

#[derive(Debug, Clone)]
pub struct ModelNode {
    pub name: String,
    pub parent: Option<usize>,
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

#[derive(Debug, Clone)]
pub struct ModelPrimitive {
    /// The node whose transform places this mesh.
    pub node: usize,
    pub mesh: Mesh3DData,
    /// Linear RGBA multiplier (glTF `baseColorFactor`).
    pub base_color: [f32; 4],
    /// Index into [`ModelData::images`] of the base colour texture.
    pub image: Option<usize>,
    /// The glTF mesh and material it came from.
    pub mesh_index: usize,
    pub material: Option<usize>,
    pub alpha: AlphaMode,
    pub double_sided: bool,
    /// Skinned: its skin, and per vertex up to four joints (indices into
    /// the skin's joint list) with weights.
    pub skin: Option<usize>,
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    /// Morph targets: per target, a position offset per vertex (and normal
    /// offsets when the file has them).
    pub morph_positions: Vec<Vec<[f32; 3]>>,
    pub morph_normals: Vec<Vec<[f32; 3]>>,
    /// The mesh's default morph weights.
    pub morph_defaults: Vec<f32>,
}

/// Which node property a channel animates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelPath {
    Translation,
    Rotation,
    Scale,
}

#[derive(Debug, Clone)]
pub struct ModelChannel {
    pub node: usize,
    pub path: ChannelPath,
    /// Keyframe times (seconds, ascending).
    pub times: Vec<f32>,
    /// One value per key (`xyz` for T/S, a quaternion `xyzw` for R).
    pub values: Vec<Vec4>,
    /// glTF `STEP` interpolation (else linear / slerp).
    pub step: bool,
}

#[derive(Debug, Clone)]
pub struct ModelClip {
    pub name: String,
    pub duration: f32,
    pub channels: Vec<ModelChannel>,
}

/// Decodes a `data:...;base64,` URI (glTF allows embedded buffers / images).
fn data_uri(uri: &str) -> Option<Vec<u8>> {
    let payload = uri.strip_prefix("data:")?.split_once(";base64,")?.1;
    let mut out = Vec::with_capacity(payload.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in payload.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Parses a `.glb` or `.gltf` file. `resolve(uri)` supplies any external
/// file it references.
pub fn parse_gltf(bytes: &[u8], resolve: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<ModelData, String> {
    let gltf = gltf::Gltf::from_slice(bytes).map_err(|e| format!("not a readable glTF/GLB: {e}"))?;
    // Buffers: the GLB's binary chunk, embedded data URIs, or external files.
    let mut buffers: Vec<Vec<u8>> = Vec::new();
    for buffer in gltf.buffers() {
        let data = match buffer.source() {
            gltf::buffer::Source::Bin => gltf.blob.clone().ok_or("GLB has no binary chunk")?,
            gltf::buffer::Source::Uri(uri) => data_uri(uri).or_else(|| resolve(uri)).ok_or_else(|| format!("missing buffer file {uri:?}"))?,
        };
        buffers.push(data);
    }
    let get_buffer = |b: gltf::Buffer| buffers.get(b.index()).map(|v| v.as_slice());

    // Images (encoded), by glTF index. A missing texture file isn't fatal —
    // that mesh just draws untextured.
    let images: Vec<Vec<u8>> = gltf
        .images()
        .map(|image| match image.source() {
            gltf::image::Source::View { view, .. } => buffers.get(view.buffer().index()).map(|b| b[view.offset()..view.offset() + view.length()].to_vec()).unwrap_or_default(),
            gltf::image::Source::Uri { uri, .. } => data_uri(uri).or_else(|| resolve(uri)).unwrap_or_default(),
        })
        .collect();

    // Nodes and their parents.
    let mut nodes: Vec<ModelNode> = gltf
        .nodes()
        .map(|n| {
            let (t, r, s) = n.transform().decomposed();
            ModelNode { name: n.name().unwrap_or("").to_string(), parent: None, translation: Vec3::from(t), rotation: Quat::from_array(r), scale: Vec3::from(s) }
        })
        .collect();
    for n in gltf.nodes() {
        for child in n.children() {
            if let Some(c) = nodes.get_mut(child.index()) {
                c.parent = Some(n.index());
            }
        }
    }

    // Mesh primitives, attached to their nodes.
    let mut primitives = Vec::new();
    for node in gltf.nodes() {
        let Some(mesh) = node.mesh() else { continue };
        for prim in mesh.primitives() {
            if prim.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let reader = prim.reader(get_buffer);
            let Some(positions) = reader.read_positions() else { continue };
            let positions: Vec<[f32; 3]> = positions.collect();
            let count = positions.len();
            let normals: Vec<[f32; 3]> = reader.read_normals().map(|n| n.collect()).unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; count]);
            let uvs: Vec<[f32; 2]> = reader.read_tex_coords(0).map(|t| t.into_f32().collect()).unwrap_or_else(|| vec![[0.0, 0.0]; count]);
            let indices: Vec<u32> = reader.read_indices().map(|i| i.into_u32().collect()).unwrap_or_else(|| (0..count as u32).collect());
            let material = prim.material();
            let pbr = material.pbr_metallic_roughness();
            let skin = node.skin().map(|s| s.index());
            let (joints, weights) = if skin.is_some() {
                let joints: Vec<[u16; 4]> = reader.read_joints(0).map(|j| j.into_u16().collect()).unwrap_or_default();
                let weights: Vec<[f32; 4]> = reader.read_weights(0).map(|w| w.into_f32().collect()).unwrap_or_default();
                if joints.len() == count && weights.len() == count { (joints, weights) } else { (Vec::new(), Vec::new()) }
            } else {
                (Vec::new(), Vec::new())
            };
            let mut morph_positions = Vec::new();
            let mut morph_normals = Vec::new();
            for (p, n, _) in reader.read_morph_targets() {
                morph_positions.push(p.map(|p| p.collect()).unwrap_or_else(|| vec![[0.0; 3]; count]));
                morph_normals.push(n.map(|n| n.collect()).unwrap_or_default());
            }
            let morph_defaults = mesh.weights().map(|w| w.to_vec()).unwrap_or_else(|| vec![0.0; morph_positions.len()]);
            primitives.push(ModelPrimitive {
                node: node.index(),
                mesh: Mesh3DData { positions, normals, uvs, indices },
                base_color: pbr.base_color_factor(),
                image: pbr.base_color_texture().map(|t| t.texture().source().index()),
                mesh_index: mesh.index(),
                material: material.index(),
                alpha: match material.alpha_mode() {
                    gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
                    gltf::material::AlphaMode::Mask => AlphaMode::Mask(material.alpha_cutoff().unwrap_or(0.5)),
                    gltf::material::AlphaMode::Blend => AlphaMode::Blend,
                },
                double_sided: material.double_sided(),
                skin: if joints.is_empty() { None } else { skin },
                joints,
                weights,
                morph_positions,
                morph_normals,
                morph_defaults,
            });
        }
    }

    // Animations.
    let mut animations = Vec::new();
    for anim in gltf.animations() {
        let mut channels = Vec::new();
        let mut duration = 0.0f32;
        for channel in anim.channels() {
            let reader = channel.reader(get_buffer);
            let Some(times) = reader.read_inputs() else { continue };
            let times: Vec<f32> = times.collect();
            let (path, values): (ChannelPath, Vec<Vec4>) = match reader.read_outputs() {
                Some(gltf::animation::util::ReadOutputs::Translations(t)) => (ChannelPath::Translation, t.map(|v| Vec3::from(v).extend(0.0)).collect()),
                Some(gltf::animation::util::ReadOutputs::Scales(s)) => (ChannelPath::Scale, s.map(|v| Vec3::from(v).extend(0.0)).collect()),
                Some(gltf::animation::util::ReadOutputs::Rotations(r)) => (ChannelPath::Rotation, r.into_f32().map(Vec4::from).collect()),
                _ => continue,
            };
            let cubic = channel.sampler().interpolation() == gltf::animation::Interpolation::CubicSpline;
            // Cubic-spline outputs are (in-tangent, value, out-tangent) per
            // key: keep just the values (played back linearly).
            let values = if cubic { values.chunks(3).filter_map(|c| c.get(1).copied()).collect() } else { values };
            if let Some(&last) = times.last() {
                duration = duration.max(last);
            }
            channels.push(ModelChannel { node: channel.target().node().index(), path, times, values, step: channel.sampler().interpolation() == gltf::animation::Interpolation::Step });
        }
        animations.push(ModelClip { name: anim.name().unwrap_or("").to_string(), duration, channels });
    }

    let skins: Vec<ModelSkin> = gltf
        .skins()
        .map(|skin| {
            let joints: Vec<usize> = skin.joints().map(|j| j.index()).collect();
            let inverse_bind: Vec<Mat4> = skin
                .reader(get_buffer)
                .read_inverse_bind_matrices()
                .map(|m| m.map(|c| Mat4::from_cols_array_2d(&c)).collect())
                .unwrap_or_else(|| vec![Mat4::IDENTITY; joints.len()]);
            ModelSkin { joints, inverse_bind }
        })
        .collect();
    let children: Vec<Vec<usize>> = gltf.nodes().map(|n| n.children().map(|c| c.index()).collect()).collect();
    let vrm = crate::vrm::gltf_json(bytes).and_then(|json| crate::vrm::parse(&json, &children));
    let mut model = ModelData { nodes, primitives, images, animations, bounds: (Vec3::ZERO, Vec3::ZERO), skins, vrm };
    let pose = model.pose(None, 0.0);
    let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for p in &model.primitives {
        let m = pose[p.node];
        for v in &p.mesh.positions {
            let w = m.transform_point3(Vec3::from(*v));
            min = min.min(w);
            max = max.max(w);
        }
    }
    model.bounds = if min.x.is_finite() { (min, max) } else { (Vec3::ZERO, Vec3::ZERO) };
    Ok(model)
}

fn sample(channel: &ModelChannel, t: f32) -> Vec4 {
    let times = &channel.times;
    let values = &channel.values;
    if times.is_empty() || values.is_empty() {
        return Vec4::ZERO;
    }
    if t <= times[0] {
        return values[0];
    }
    let last = times.len().min(values.len()) - 1;
    if t >= times[last] {
        return values[last];
    }
    let i = times.partition_point(|&k| k <= t).saturating_sub(1).min(last - 1);
    if channel.step {
        return values[i];
    }
    let f = ((t - times[i]) / (times[i + 1] - times[i]).max(1e-6)).clamp(0.0, 1.0);
    match channel.path {
        ChannelPath::Rotation => Quat::from_vec4(values[i]).slerp(Quat::from_vec4(values[i + 1]), f).into(),
        _ => values[i].lerp(values[i + 1], f),
    }
}

impl ModelData {
    /// A clip's index by name (case-insensitive).
    pub fn clip(&self, name: &str) -> Option<usize> {
        self.animations.iter().position(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// Every node's model-space matrix with `clip` applied at `time`
    /// seconds (`None` = the rest pose).
    pub fn pose(&self, clip: Option<usize>, time: f32) -> Vec<Mat4> {
        self.pose_layers(&[(clip, time)])
    }

    /// Like [`ModelData::pose`] with several clips layered in order: each
    /// later clip overrides the node properties it animates.
    pub fn pose_layers(&self, layers: &[(Option<usize>, f32)]) -> Vec<Mat4> {
        self.world_pose(&self.local_pose(layers))
    }

    /// The rest pose: every node's local (translation, rotation, scale).
    pub fn rest_pose(&self) -> Vec<(Vec3, Quat, Vec3)> {
        self.nodes.iter().map(|n| (n.translation, n.rotation, n.scale)).collect()
    }

    /// Every node's *local* (translation, rotation, scale) with `layers`
    /// applied — edit it (pose bones, run springs), then
    /// [`ModelData::world_pose`].
    pub fn local_pose(&self, layers: &[(Option<usize>, f32)]) -> Vec<(Vec3, Quat, Vec3)> {
        let mut local = self.rest_pose();
        for (clip, time) in layers {
            let (Some(clip), time) = (clip.and_then(|c| self.animations.get(c)), *time) else { continue };
            for ch in &clip.channels {
                let Some(l) = local.get_mut(ch.node) else { continue };
                let v = sample(ch, time);
                match ch.path {
                    ChannelPath::Translation => l.0 = v.truncate(),
                    ChannelPath::Rotation => l.1 = Quat::from_vec4(v).normalize(),
                    ChannelPath::Scale => l.2 = v.truncate(),
                }
            }
        }
        local
    }

    /// Model-space matrices from a local pose.
    pub fn world_pose(&self, local: &[(Vec3, Quat, Vec3)]) -> Vec<Mat4> {
        let mut world: Vec<Option<Mat4>> = vec![None; self.nodes.len()];
        fn resolve(i: usize, nodes: &[ModelNode], local: &[(Vec3, Quat, Vec3)], world: &mut [Option<Mat4>], depth: usize) -> Mat4 {
            if let Some(m) = world[i] {
                return m;
            }
            let (t, r, s) = local[i];
            let own = Mat4::from_scale_rotation_translation(s, r, t);
            let m = match nodes[i].parent {
                Some(p) if p < nodes.len() && depth < 64 => resolve(p, nodes, local, world, depth + 1) * own,
                _ => own,
            };
            world[i] = Some(m);
            m
        }
        (0..self.nodes.len()).map(|i| resolve(i, &self.nodes, local, &mut world, 0)).collect()
    }

    /// Each node's parent (for [`crate::vrm::SpringSim`]).
    pub fn parents(&self) -> Vec<Option<usize>> {
        self.nodes.iter().map(|n| n.parent).collect()
    }

    /// Deforms primitive `prim` for a pose (`world`: model-space node
    /// matrices) and morph weights (`morphs[t]` for target `t`; missing =
    /// the mesh default), writing model-space positions and normals into
    /// `out` (UVs and indices copied once). Linear blend skinning: each
    /// vertex is moved by up to four joints' `world × inverse bind`,
    /// weighted. A skinned mesh ignores its own node's transform (glTF);
    /// an unskinned one is moved by it.
    pub fn skin_primitive(&self, prim: usize, world: &[Mat4], morphs: &[f32], out: &mut Mesh3DData) {
        let Some(p) = self.primitives.get(prim) else { return };
        let count = p.mesh.positions.len();
        if out.indices.len() != p.mesh.indices.len() || out.uvs.len() != count {
            out.indices = p.mesh.indices.clone();
            out.uvs = p.mesh.uvs.clone();
        }
        out.positions.resize(count, [0.0; 3]);
        out.normals.resize(count, [0.0; 3]);
        let weight = |t: usize| morphs.get(t).copied().unwrap_or_else(|| p.morph_defaults.get(t).copied().unwrap_or(0.0));
        let active: Vec<(usize, f32)> = (0..p.morph_positions.len()).map(|t| (t, weight(t))).filter(|(_, w)| w.abs() > 1e-4).collect();
        let joints: Vec<Mat4> = match p.skin.and_then(|s| self.skins.get(s)) {
            Some(skin) => skin.joints.iter().zip(&skin.inverse_bind).map(|(&j, ibm)| world.get(j).copied().unwrap_or(Mat4::IDENTITY) * *ibm).collect(),
            None => Vec::new(),
        };
        let node = world.get(p.node).copied().unwrap_or(Mat4::IDENTITY);
        for v in 0..count {
            let mut pos = Vec3::from(p.mesh.positions[v]);
            let mut nrm = Vec3::from(p.mesh.normals[v]);
            for &(t, w) in &active {
                pos += Vec3::from(p.morph_positions[t][v]) * w;
                if let Some(n) = p.morph_normals.get(t).and_then(|n| n.get(v)) {
                    nrm += Vec3::from(*n) * w;
                }
            }
            let (pos, nrm) = if joints.is_empty() {
                (node.transform_point3(pos), node.transform_vector3(nrm))
            } else {
                let (js, ws) = (p.joints[v], p.weights[v]);
                let mut sp = Vec3::ZERO;
                let mut sn = Vec3::ZERO;
                let mut total = 0.0;
                for k in 0..4 {
                    let w = ws[k];
                    if w <= 0.0 {
                        continue;
                    }
                    let Some(m) = joints.get(js[k] as usize) else { continue };
                    sp += m.transform_point3(pos) * w;
                    sn += m.transform_vector3(nrm) * w;
                    total += w;
                }
                if total > 0.0 { (sp / total, sn) } else { (pos, nrm) }
            };
            out.positions[v] = pos.into();
            out.normals[v] = nrm.normalize_or_zero().into();
        }
    }

    /// [`ModelData::skin_primitive`], but straight into the renderer's
    /// interleaved vertex layout (position, normal, UV: 8 floats per
    /// vertex) in a reused buffer, blending each vertex's joint matrices
    /// first so it's transformed once — the per-frame path for characters.
    pub fn skin_primitive_interleaved(&self, prim: usize, world: &[Mat4], morphs: &[f32], out: &mut Vec<f32>) {
        use glam::Affine3A;
        let Some(p) = self.primitives.get(prim) else { return };
        let count = p.mesh.positions.len();
        out.resize(count * 8, 0.0);
        let weight = |t: usize| morphs.get(t).copied().unwrap_or_else(|| p.morph_defaults.get(t).copied().unwrap_or(0.0));
        let active: Vec<(usize, f32)> = (0..p.morph_positions.len()).map(|t| (t, weight(t))).filter(|(_, w)| w.abs() > 1e-4).collect();
        let joints: Vec<Affine3A> = match p.skin.and_then(|s| self.skins.get(s)) {
            Some(skin) => skin.joints.iter().zip(&skin.inverse_bind).map(|(&j, ibm)| Affine3A::from_mat4(world.get(j).copied().unwrap_or(Mat4::IDENTITY) * *ibm)).collect(),
            None => Vec::new(),
        };
        let node = Affine3A::from_mat4(world.get(p.node).copied().unwrap_or(Mat4::IDENTITY));
        let zero = Affine3A { matrix3: glam::Mat3A::ZERO, translation: glam::Vec3A::ZERO };
        for v in 0..count {
            let mut pos = Vec3::from(p.mesh.positions[v]);
            let mut nrm = Vec3::from(p.mesh.normals[v]);
            for &(t, w) in &active {
                pos += Vec3::from(p.morph_positions[t][v]) * w;
                if let Some(n) = p.morph_normals.get(t).and_then(|n| n.get(v)) {
                    nrm += Vec3::from(*n) * w;
                }
            }
            let m = if joints.is_empty() {
                node
            } else {
                let (js, ws) = (p.joints[v], p.weights[v]);
                let mut m = zero;
                let mut total = 0.0;
                for k in 0..4 {
                    let w = ws[k];
                    if w <= 0.0 {
                        continue;
                    }
                    let Some(j) = joints.get(js[k] as usize) else { continue };
                    m.matrix3 += j.matrix3 * w;
                    m.translation += j.translation * w;
                    total += w;
                }
                if total > 0.0 && (total - 1.0).abs() > 1e-3 {
                    m.matrix3 *= 1.0 / total;
                    m.translation *= 1.0 / total;
                }
                if total > 0.0 { m } else { Affine3A::IDENTITY }
            };
            let sp = m.transform_point3(pos);
            let sn = (m.matrix3 * nrm).normalize_or_zero();
            let uv = p.mesh.uvs.get(v).copied().unwrap_or([0.0, 0.0]);
            let o = &mut out[v * 8..v * 8 + 8];
            o[0] = sp.x;
            o[1] = sp.y;
            o[2] = sp.z;
            o[3] = sn.x;
            o[4] = sn.y;
            o[5] = sn.z;
            o[6] = uv[0];
            o[7] = uv[1];
        }
    }

    /// Primitive `prim` in its bind pose with morph targets applied, in the
    /// interleaved 3D layout — for a skinned mesh the GPU then skins it
    /// (so this only runs when expression weights change).
    pub fn morph_primitive_interleaved(&self, prim: usize, morphs: &[f32], out: &mut Vec<f32>) {
        let Some(p) = self.primitives.get(prim) else { return };
        let count = p.mesh.positions.len();
        out.resize(count * 8, 0.0);
        let weight = |t: usize| morphs.get(t).copied().unwrap_or_else(|| p.morph_defaults.get(t).copied().unwrap_or(0.0));
        let active: Vec<(usize, f32)> = (0..p.morph_positions.len()).map(|t| (t, weight(t))).filter(|(_, w)| w.abs() > 1e-4).collect();
        for v in 0..count {
            let mut pos = Vec3::from(p.mesh.positions[v]);
            let mut nrm = Vec3::from(p.mesh.normals[v]);
            for &(t, w) in &active {
                pos += Vec3::from(p.morph_positions[t][v]) * w;
                if let Some(n) = p.morph_normals.get(t).and_then(|n| n.get(v)) {
                    nrm += Vec3::from(*n) * w;
                }
            }
            let nrm = nrm.normalize_or_zero();
            let uv = p.mesh.uvs.get(v).copied().unwrap_or([0.0, 0.0]);
            out[v * 8..v * 8 + 8].copy_from_slice(&[pos.x, pos.y, pos.z, nrm.x, nrm.y, nrm.z, uv[0], uv[1]]);
        }
    }

    /// Each skin's joint palette for a pose: `world[joint] × inverse bind`.
    pub fn joint_palettes(&self, world: &[Mat4]) -> Vec<Vec<Mat4>> {
        self.skins.iter().map(|skin| skin.joints.iter().zip(&skin.inverse_bind).map(|(&j, ibm)| world.get(j).copied().unwrap_or(Mat4::IDENTITY) * *ibm).collect()).collect()
    }

    /// Whether the model deforms (skinned or morphed) — such primitives are
    /// re-meshed per frame; rigid ones are drawn with node matrices.
    pub fn deforms(&self, prim: usize) -> bool {
        self.primitives.get(prim).is_some_and(|p| p.skin.is_some() || !p.morph_positions.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal hand-built glTF (one triangle on a child node, an
    /// embedded base64 buffer, one rotation animation) round-trips.
    #[test]
    fn parses_an_embedded_gltf_with_a_node_animation() {
        // Buffer: 3 positions (36 bytes) + 2 times (8) + 2 quaternions (32).
        let mut buf: Vec<u8> = Vec::new();
        for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0] {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        let half = std::f32::consts::FRAC_1_SQRT_2;
        for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, half, 0.0, half] {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut b64 = String::new();
        for chunk in buf.chunks(3) {
            let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
            for k in 0..4 {
                if k <= chunk.len() {
                    b64.push(T[(n >> (18 - 6 * k) & 63) as usize] as char);
                } else {
                    b64.push('=');
                }
            }
        }
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
            "nodes":[{{"name":"root","children":[1],"translation":[0,1,0]}},{{"name":"part","mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],
            "buffers":[{{"byteLength":{len},"uri":"data:application/octet-stream;base64,{b64}"}}],
            "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":8}},{{"buffer":0,"byteOffset":44,"byteLength":32}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}},
              {{"bufferView":1,"componentType":5126,"count":2,"type":"SCALAR","min":[0],"max":[1]}},
              {{"bufferView":2,"componentType":5126,"count":2,"type":"VEC4"}}],
            "animations":[{{"name":"spin","samplers":[{{"input":1,"output":2}}],"channels":[{{"sampler":0,"target":{{"node":1,"path":"rotation"}}}}]}}]}}"#,
            len = buf.len()
        );
        let model = parse_gltf(json.as_bytes(), &|_| None).expect("parses");
        assert_eq!(model.primitives.len(), 1);
        assert_eq!(model.nodes[1].parent, Some(0));
        assert!((model.bounds.1.y - 2.0).abs() < 1e-4, "child sits under the root's +1 y: {:?}", model.bounds);
        let clip = model.clip("SPIN").expect("clip by name");
        assert!((model.animations[clip].duration - 1.0).abs() < 1e-5);
        // Halfway through: a 45° turn about Y moves +X toward -Z.
        let pose = model.pose(Some(clip), 0.5);
        let x = pose[1].transform_vector3(Vec3::X);
        assert!(x.z < -0.6 && x.x > 0.6, "{x}");
    }
}
