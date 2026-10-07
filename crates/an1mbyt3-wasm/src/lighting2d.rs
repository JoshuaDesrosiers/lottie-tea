//! Editor-side runtime for the Unity-style 2D lighting and trail node kinds
//! (`an1mbyt3_core::Light2DNode`, `TrailNode`, `ShadowCaster2D`):
//!
//! - **Trails**: per-node point history (`TrailRuntime`), stepped once per
//!   frame from the node's world position, turned into a ribbon mesh with
//!   per-vertex UVs, and drawn with a baked texture (color gradient along
//!   the trail x the optional ribbon texture).
//! - **Lights**: gathering every visible light/caster in the artboard into
//!   the renderer's `Lighting` description each frame.
//!
//! `WasmEditor::render` owns the calls; everything here is plain data in,
//! data out, so it stays testable without a GPU.

use glam::Vec2;

use an1mbyt3_core::{Artboard, Light2DNode, Light2DType, LightBlendStyle, LightOverlapOperation, Node, NodeId, NodeKind, PathCommand, ShadowCastingOption, TrailNode};
use an1mbyt3_geometry::Mesh;

// ---------------------------------------------------------------------------
// Trails
// ---------------------------------------------------------------------------

/// One recorded trail point: where the node was, and how long ago.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TrailPoint {
    pub position: Vec2,
    pub age: f32,
}

/// A trail's live point history, oldest first. Runtime-only (never saved).
#[derive(Default, Debug)]
pub(crate) struct TrailRuntime {
    pub points: Vec<TrailPoint>,
}

/// Ages every point, drops the expired ones, and records `head` if the
/// node has moved at least `min_vertex_distance` since the newest point —
/// Unity's Time/Min Vertex Distance/Emitting behavior.
pub(crate) fn step_trail(config: &TrailNode, runtime: &mut TrailRuntime, head: Vec2, delta_seconds: f32) {
    let lifetime = config.time.max(0.001);
    for point in &mut runtime.points {
        point.age += delta_seconds;
    }
    runtime.points.retain(|p| p.age < lifetime);
    if config.emitting {
        let far_enough = runtime.points.last().map_or(true, |p| p.position.distance(head) >= config.min_vertex_distance.max(0.01));
        if far_enough {
            runtime.points.push(TrailPoint { position: head, age: 0.0 });
        }
    }
}

/// Piecewise-linear sample of a width curve (`t` 0 = head .. 1 = tail).
fn sample_width(config: &TrailNode, t: f32) -> f32 {
    let keys = &config.width_curve;
    let (Some(first), Some(last)) = (keys.first(), keys.last()) else { return config.width };
    let multiplier = if t <= first.t {
        first.size_multiplier
    } else if t >= last.t {
        last.size_multiplier
    } else {
        keys.windows(2)
            .find(|pair| t >= pair[0].t && t <= pair[1].t)
            .map(|pair| {
                let span = (pair[1].t - pair[0].t).max(1e-6);
                pair[0].size_multiplier + (pair[1].size_multiplier - pair[0].size_multiplier) * ((t - pair[0].t) / span)
            })
            .unwrap_or(last.size_multiplier)
    };
    config.width * multiplier
}

/// Chaikin corner-cutting: each pass replaces every interior corner with
/// two points a quarter of the way along its edges — how `corner_vertices`
/// rounds sharp turns (each extra vertex = one more smoothing pass, capped
/// so the point count stays sane).
fn round_corners(points: &[Vec2], passes: u32) -> Vec<Vec2> {
    let mut current = points.to_vec();
    for _ in 0..passes.min(4) {
        if current.len() < 3 {
            break;
        }
        let mut next = Vec::with_capacity(current.len() * 2);
        next.push(current[0]);
        for pair in current.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            next.push(a.lerp(b, 0.25));
            next.push(a.lerp(b, 0.75));
        }
        next.push(*current.last().unwrap());
        current = next;
    }
    current
}

/// The ribbon mesh for one trail, in world space, or `None` if there's
/// nothing to draw yet. `head` is the node's current position (the trail
/// always reaches all the way to it, even between recorded points).
///
/// UVs: `u` runs 0 (head) .. 1 (tail) by length along the trail — the
/// color gradient and a stretched texture both read it; `v` runs 0..1
/// across the width.
pub(crate) fn build_trail_mesh(config: &TrailNode, runtime: &TrailRuntime, head: Vec2) -> Option<Mesh> {
    // Newest first, starting at the live head.
    let mut line: Vec<Vec2> = Vec::with_capacity(runtime.points.len() + 1);
    if config.emitting {
        line.push(head);
    }
    for point in runtime.points.iter().rev() {
        if line.last().map_or(true, |last| last.distance(point.position) > 0.001) {
            line.push(point.position);
        }
    }
    let line = round_corners(&line, config.corner_vertices);
    if line.len() < 2 {
        return None;
    }

    // Cumulative length -> the `t` everything is parameterized by.
    let mut lengths = vec![0.0f32; line.len()];
    for i in 1..line.len() {
        lengths[i] = lengths[i - 1] + line[i].distance(line[i - 1]);
    }
    let total = lengths.last().copied().unwrap_or(0.0).max(1e-4);

    let mut positions: Vec<[f32; 2]> = Vec::with_capacity(line.len() * 2);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(line.len() * 2);
    let mut indices: Vec<u32> = Vec::new();
    let mut half_widths = Vec::with_capacity(line.len());
    for i in 0..line.len() {
        let t = lengths[i] / total;
        let half = sample_width(config, t) * 0.5;
        half_widths.push(half);
        // Averaged neighbor tangent = a mitered join.
        let prev = if i == 0 { line[i] } else { line[i - 1] };
        let next = if i + 1 == line.len() { line[i] } else { line[i + 1] };
        let tangent = (next - prev).normalize_or(Vec2::X);
        let normal = Vec2::new(-tangent.y, tangent.x);
        let left = line[i] + normal * half;
        let right = line[i] - normal * half;
        positions.push(left.to_array());
        positions.push(right.to_array());
        uvs.push([t, 0.0]);
        uvs.push([t, 1.0]);
        if i > 0 {
            let base = (i as u32 - 1) * 2;
            indices.extend_from_slice(&[base, base + 1, base + 3, base, base + 3, base + 2]);
        }
    }

    // Unity's End Cap Vertices: a fan around each end, rounding it off.
    if config.end_cap_vertices > 0 {
        let segments = config.end_cap_vertices.min(16) + 1;
        let mut add_cap = |end: usize, inward: Vec2, u: f32| {
            let center = line[end];
            let half = half_widths[end];
            if half <= 0.001 {
                return;
            }
            let outward = -inward.normalize_or(Vec2::X);
            let normal = Vec2::new(-outward.y, outward.x);
            let center_index = positions.len() as u32;
            positions.push(center.to_array());
            uvs.push([u, 0.5]);
            for step in 0..=segments {
                let angle = std::f32::consts::PI * step as f32 / segments as f32;
                let dir = normal * angle.cos() + outward * angle.sin();
                positions.push((center + dir * half).to_array());
                uvs.push([u, 0.5 - 0.5 * angle.cos()]);
            }
            for step in 0..segments {
                indices.extend_from_slice(&[center_index, center_index + 1 + step, center_index + 2 + step]);
            }
        };
        add_cap(0, line[1] - line[0], 0.0);
        let last = line.len() - 1;
        add_cap(last, line[last - 1] - line[last], 1.0);
    }

    Some(Mesh { positions, indices, uvs })
}

/// Piecewise-linear color along the gradient (offset 0 = head .. 1 = tail);
/// white if the gradient is empty.
fn sample_gradient(config: &TrailNode, t: f32) -> [f32; 4] {
    let stops = &config.color_gradient;
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else { return [1.0; 4] };
    if t <= first.offset {
        return first.color;
    }
    if t >= last.offset {
        return last.color;
    }
    for pair in stops.windows(2) {
        if t >= pair[0].offset && t <= pair[1].offset {
            let span = (pair[1].offset - pair[0].offset).max(1e-6);
            let k = (t - pair[0].offset) / span;
            let (a, b) = (pair[0].color, pair[1].color);
            return [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k, a[3] + (b[3] - a[3]) * k];
        }
    }
    last.color
}

/// The trail's texture as RGBA8 pixels: the color gradient laid out along
/// `x` (head at the left), multiplied into the optional ribbon texture
/// (`texture_rgba`, already decoded: pixels, width, height), stretched.
pub(crate) fn bake_trail_texture(config: &TrailNode, texture_rgba: Option<(&[u8], u32, u32)>) -> (Vec<u8>, u32, u32) {
    let (width, height) = match texture_rgba {
        Some((_, w, h)) => (w.clamp(1, 256), h.clamp(1, 256)),
        None => (64, 1),
    };
    let to_byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let t = if width > 1 { x as f32 / (width - 1) as f32 } else { 0.0 };
            let mut color = sample_gradient(config, t);
            if let Some((src, src_w, src_h)) = texture_rgba {
                let sx = (x * src_w / width).min(src_w - 1);
                let sy = (y * src_h / height).min(src_h - 1);
                let i = ((sy * src_w + sx) * 4) as usize;
                for c in 0..4 {
                    color[c] *= src[i + c] as f32 / 255.0;
                }
            }
            pixels.extend_from_slice(&[to_byte(color[0]), to_byte(color[1]), to_byte(color[2]), to_byte(color[3])]);
        }
    }
    (pixels, width, height)
}

// ---------------------------------------------------------------------------
// Lights
// ---------------------------------------------------------------------------

/// `node` and every ancestor are visible and not deleted — what actually
/// decides whether a light/caster takes part in this frame (mirrors
/// `build_scene_ops`' own "hidden parent hides the subtree" rule).
pub(crate) fn effectively_visible(artboard: &Artboard, node_id: NodeId) -> bool {
    if artboard.is_effectively_deleted(node_id) {
        return false;
    }
    let mut cursor = Some(node_id);
    for _ in 0..64 {
        let Some(id) = cursor else { return true };
        let Some(node) = artboard.node(id) else { return false };
        if !node.visible {
            return false;
        }
        cursor = node.parent;
    }
    true
}

/// One light, everything resolved except borrowed GPU resources (the
/// sprite texture is looked up by id at draw-assembly time).
pub(crate) struct CollectedLight {
    pub node: NodeId,
    pub light: Light2DNode,
    pub transform: an1mbyt3_core::Transform2D,
}

/// Every Light2D in the artboard this frame: Global lights folded into the
/// two ambient colors, the rest returned in Light Order.
pub(crate) struct CollectedLighting {
    pub any_light: bool,
    pub ambient_multiply: [f32; 3],
    pub ambient_additive: [f32; 3],
    pub lights: Vec<CollectedLight>,
}

pub(crate) fn collect_lights(artboard: &Artboard) -> CollectedLighting {
    let mut out = CollectedLighting { any_light: false, ambient_multiply: [0.0; 3], ambient_additive: [0.0; 3], lights: Vec::new() };
    for node_id in artboard.node_ids() {
        let Some(node) = artboard.node(node_id) else { continue };
        let Some(light) = node.light2d() else { continue };
        if !effectively_visible(artboard, node_id) {
            continue;
        }
        out.any_light = true;
        if light.light_type == Light2DType::Global {
            let target = match light.blend_style {
                LightBlendStyle::Multiply => &mut out.ambient_multiply,
                LightBlendStyle::Additive => &mut out.ambient_additive,
            };
            for c in 0..3 {
                // AlphaBlend between global lights: the later one wins,
                // as stacking two "whole scene" lights would in Unity.
                match light.overlap_operation {
                    LightOverlapOperation::Additive => target[c] += an1mbyt3_renderer::srgb_to_linear(light.color[c]) * light.intensity,
                    LightOverlapOperation::AlphaBlend => target[c] = an1mbyt3_renderer::srgb_to_linear(light.color[c]) * light.intensity,
                    LightOverlapOperation::Max => target[c] = target[c].max(an1mbyt3_renderer::srgb_to_linear(light.color[c]) * light.intensity),
                }
            }
            continue;
        }
        out.lights.push(CollectedLight { node: node_id, light: light.clone(), transform: artboard.world_transform(node_id) });
    }
    out.lights.sort_by_key(|l| l.light.light_order);
    out
}

/// A Shape's outline, flattened (cubics sampled) in its own local space —
/// the first subpath only (the outer silhouette, for every preset shape).
fn shape_outline(path: &an1mbyt3_core::Path) -> Vec<Vec2> {
    let mut points = Vec::new();
    let mut current = Vec2::ZERO;
    for command in &path.commands {
        match *command {
            PathCommand::MoveTo(p) => {
                if !points.is_empty() {
                    break;
                }
                points.push(p);
                current = p;
            }
            PathCommand::LineTo(p) => {
                points.push(p);
                current = p;
            }
            PathCommand::CubicTo { control1, control2, to } => {
                const STEPS: usize = 8;
                for step in 1..=STEPS {
                    let t = step as f32 / STEPS as f32;
                    let u = 1.0 - t;
                    points.push(current * (u * u * u) + control1 * (3.0 * u * u * t) + control2 * (3.0 * u * t * t) + to * (t * t * t));
                }
                current = to;
            }
            PathCommand::Close => break,
        }
    }
    points
}

/// Every enabled Shadow Caster 2D, as world-space outlines.
/// `local_bounds` is the same per-kind bounds function the selection box
/// uses (for every kind but `Shape`, whose real outline is used).
pub(crate) fn collect_casters(
    artboard: &Artboard,
    local_bounds: impl Fn(NodeId) -> (Vec2, Vec2),
    silhouette: impl Fn(NodeId) -> Option<Vec<Vec2>>,
) -> Vec<an1mbyt3_renderer::ShadowCasterShape> {
    let mut casters = Vec::new();
    for node_id in artboard.node_ids() {
        let Some(node) = artboard.node(node_id) else { continue };
        let Some(caster) = node.shadow_caster().copied() else { continue };
        if !caster.enabled || !effectively_visible(artboard, node_id) {
            continue;
        }
        let mut local = match &node.kind {
            NodeKind::Shape(shape) => shape_outline(&shape.path),
            _ if let Some(outline) = silhouette(node_id) => outline,
            _ => {
                let (min, max) = local_bounds(node_id);
                vec![min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)]
            }
        };
        if local.len() < 3 {
            continue;
        }
        if caster.trim_edge > 0.0 {
            let centroid = local.iter().copied().sum::<Vec2>() / local.len() as f32;
            for p in &mut local {
                let to_center = centroid - *p;
                let length = to_center.length();
                *p += to_center.normalize_or_zero() * caster.trim_edge.min(length * 0.95);
            }
        }
        let affine = artboard.world_transform(node_id).to_affine2();
        let (casts, self_shadows) = match caster.casting_option {
            ShadowCastingOption::CastShadow => (true, false),
            ShadowCastingOption::SelfShadow => (false, true),
            ShadowCastingOption::CastAndSelfShadow => (true, true),
        };
        casters.push(an1mbyt3_renderer::ShadowCasterShape { outline: local.iter().map(|p| affine.transform_point2(*p)).collect(), casts, self_shadows });
    }
    casters
}

/// A light node's sprite, if it's a Sprite light with one set.
pub(crate) fn light_sprite(node: &Node) -> Option<an1mbyt3_core::ImageId> {
    node.light2d().filter(|l| l.light_type == Light2DType::Sprite).and_then(|l| l.sprite)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trail_records_points_only_after_min_distance_and_expires_them() {
        let config = TrailNode { time: 1.0, min_vertex_distance: 5.0, ..TrailNode::default() };
        let mut runtime = TrailRuntime::default();
        step_trail(&config, &mut runtime, Vec2::ZERO, 0.1);
        step_trail(&config, &mut runtime, Vec2::new(2.0, 0.0), 0.1);
        assert_eq!(runtime.points.len(), 1, "moved less than min vertex distance");
        step_trail(&config, &mut runtime, Vec2::new(6.0, 0.0), 0.1);
        assert_eq!(runtime.points.len(), 2);
        step_trail(&config, &mut runtime, Vec2::new(6.0, 0.0), 2.0);
        // Everything older than `time` drops; the trail re-seeds from the
        // current head alone (one point draws nothing until it moves).
        assert_eq!(runtime.points.len(), 1);
        assert_eq!(runtime.points[0].age, 0.0);
    }

    #[test]
    fn ribbon_has_two_vertices_per_point_and_tapers_with_the_width_curve() {
        let config = TrailNode::default();
        let runtime = TrailRuntime { points: vec![TrailPoint { position: Vec2::new(-20.0, 0.0), age: 0.3 }, TrailPoint { position: Vec2::new(-10.0, 0.0), age: 0.1 }] };
        let mesh = build_trail_mesh(&config, &runtime, Vec2::ZERO).unwrap();
        assert_eq!(mesh.positions.len(), 6);
        assert_eq!(mesh.uvs.len(), 6);
        let head_width = (mesh.positions[0][1] - mesh.positions[1][1]).abs();
        let tail_width = (mesh.positions[4][1] - mesh.positions[5][1]).abs();
        assert!((head_width - config.width).abs() < 1e-3);
        assert!(tail_width < 1e-3, "default width curve tapers to 0 at the tail");
        assert_eq!(mesh.uvs[0][0], 0.0);
        assert_eq!(mesh.uvs[5][0], 1.0);
    }

    #[test]
    fn end_caps_add_fans() {
        let config = TrailNode { end_cap_vertices: 4, width_curve: Vec::new(), ..TrailNode::default() };
        let runtime = TrailRuntime { points: vec![TrailPoint { position: Vec2::new(-10.0, 0.0), age: 0.1 }] };
        let plain = build_trail_mesh(&TrailNode { end_cap_vertices: 0, ..config.clone() }, &runtime, Vec2::ZERO).unwrap();
        let capped = build_trail_mesh(&config, &runtime, Vec2::ZERO).unwrap();
        assert!(capped.indices.len() > plain.indices.len());
    }

    #[test]
    fn baked_texture_fades_along_the_gradient() {
        let (pixels, w, h) = bake_trail_texture(&TrailNode::default(), None);
        assert_eq!((w, h), (64, 1));
        assert_eq!(pixels[3], 255, "head opaque");
        assert_eq!(pixels[(63 * 4 + 3) as usize], 0, "tail transparent");
    }

    #[test]
    fn global_lights_fold_into_ambient_and_spots_sort_by_order() {
        let mut artboard = Artboard::new("Lit", 100.0, 100.0);
        artboard.add_node(Node::new("Global", None, NodeKind::Light2D(Light2DNode { intensity: 0.5, ..Light2DNode::global() })));
        artboard.add_node(Node::new("B", None, NodeKind::Light2D(Light2DNode { light_order: 2, ..Light2DNode::default() })));
        artboard.add_node(Node::new("A", None, NodeKind::Light2D(Light2DNode { light_order: 1, ..Light2DNode::default() })));
        let collected = collect_lights(&artboard);
        assert!(collected.any_light);
        assert_eq!(collected.ambient_multiply, [0.5, 0.5, 0.5]);
        assert_eq!(collected.lights.len(), 2);
        assert_eq!(collected.lights[0].light.light_order, 1);
    }
}
