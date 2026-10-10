//! The editor's 3D scene view (Design mode) — Unity's Scene view: an orbit
//! camera independent of the game's Camera 3D, click-to-select picking, and
//! a move gizmo (X / Y / Z arrows + a centre handle for the ground plane).
//!
//! Everything here is pure math over the document (no GPU), so it's unit
//! tested natively; `editor.rs` turns it into draws and wasm entry points.

use an1mbyt3_core::{Artboard, Node3DContent, NodeId, NodeKind, Primitive3D};
use glam::{Mat4, Vec2, Vec3, Vec4Swizzles};

/// The scene view's orbit camera.
#[derive(Debug, Clone, Copy)]
pub struct EditorCamera3D {
    pub target: Vec3,
    /// Degrees around +Y (0 = looking down −Z from +Z).
    pub yaw: f32,
    /// Degrees above the horizon.
    pub pitch: f32,
    pub distance: f32,
    pub fov_degrees: f32,
}

impl Default for EditorCamera3D {
    fn default() -> Self {
        Self { target: Vec3::new(0.0, 0.5, 0.0), yaw: 30.0, pitch: 28.0, distance: 12.0, fov_degrees: 55.0 }
    }
}

impl EditorCamera3D {
    pub fn eye(&self) -> Vec3 {
        let (y, p) = (self.yaw.to_radians(), self.pitch.to_radians());
        self.target + Vec3::new(y.sin() * p.cos(), p.sin(), y.cos() * p.cos()) * self.distance
    }

    pub fn view(&self) -> Mat4 {
        glam::camera::rh::view::look_at_mat4(self.eye(), self.target, Vec3::Y)
    }

    pub fn projection(&self, aspect: f32) -> Mat4 {
        glam::camera::rh::proj::directx::perspective(self.fov_degrees.to_radians(), aspect.max(0.01), 0.05, 2000.0)
    }

    /// Right-drag: orbit around the target.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.35;
        self.pitch = (self.pitch + dy * 0.3).clamp(-89.0, 89.0);
    }

    /// Middle-drag: slide the target in the view plane (pixels → meters
    /// scaled so the point under the cursor roughly follows it).
    pub fn pan(&mut self, dx: f32, dy: f32, viewport_height: f32) {
        let view = self.view();
        let right = view.row(0).xyz();
        let up = view.row(1).xyz();
        let meters_per_pixel = 2.0 * self.distance * (self.fov_degrees.to_radians() / 2.0).tan() / viewport_height.max(1.0);
        self.target += (-right * dx + up * dy) * meters_per_pixel;
    }

    /// Wheel: zoom toward the target.
    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance * (delta * 0.0015).exp()).clamp(0.3, 1500.0);
    }

    /// A world-space ray through a viewport point (pixels, origin top-left).
    pub fn ray(&self, point: Vec2, viewport: Vec2) -> (Vec3, Vec3) {
        let ndc = Vec2::new(point.x / viewport.x * 2.0 - 1.0, 1.0 - point.y / viewport.y * 2.0);
        let inv = (self.projection(viewport.x / viewport.y) * self.view()).inverse();
        let near = inv.project_point3(ndc.extend(0.0));
        let far = inv.project_point3(ndc.extend(1.0));
        (near, (far - near).normalize())
    }

    /// World point → viewport pixels (`None` behind the camera).
    pub fn project(&self, world: Vec3, viewport: Vec2) -> Option<Vec2> {
        let clip = self.projection(viewport.x / viewport.y) * self.view() * world.extend(1.0);
        if clip.w <= 0.0001 {
            return None;
        }
        let ndc = clip.xyz() / clip.w;
        Some(Vec2::new((ndc.x + 1.0) * 0.5 * viewport.x, (1.0 - ndc.y) * 0.5 * viewport.y))
    }

    /// The gizmo's world size at `at`, so it stays a constant size on screen.
    pub fn gizmo_scale(&self, at: Vec3) -> f32 {
        (self.eye() - at).length() * 0.16
    }
}

/// Whether a node and every ancestor are visible (hiding a group hides
/// the 3D content under it, like the 2D layer).
pub fn visible_chain(artboard: &Artboard, id: NodeId) -> bool {
    let mut current = Some(id);
    while let Some(c) = current {
        let Some(node) = artboard.node(c) else { return false };
        if !node.visible {
            return false;
        }
        current = node.parent;
    }
    true
}

/// A Node3D's local-space bounds (what picking and the selection box use),
/// asking `model_bounds` for imported models' bounds.
pub fn local_bounds_with(content: &Node3DContent, model_bounds: &dyn Fn(an1mbyt3_core::ModelId) -> Option<(Vec3, Vec3)>) -> (Vec3, Vec3) {
    if let Node3DContent::Mesh(mesh) = content {
        if let Primitive3D::Model { model } = mesh.primitive {
            return model_bounds(model).unwrap_or((Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 1.8, 0.5)));
        }
        // Block worlds start at their origin corner.
        if let Primitive3D::Voxels { width, height, depth } = mesh.primitive {
            return (Vec3::ZERO, Vec3::new(width as f32, height as f32, depth as f32));
        }
    }
    let half = match content {
        Node3DContent::Mesh(mesh) => match mesh.primitive {
            Primitive3D::Cube { size } => size * 0.5,
            Primitive3D::Sphere { radius, .. } => Vec3::splat(radius),
            Primitive3D::Plane { size, .. } => Vec3::new(size * 0.5, 0.01, size * 0.5),
            Primitive3D::Cylinder { radius, height, .. } | Primitive3D::Cone { radius, height, .. } => Vec3::new(radius, height * 0.5, radius),
            Primitive3D::Torus { radius, tube, .. } => Vec3::new(radius + tube, tube, radius + tube),
            Primitive3D::Model { .. } | Primitive3D::Voxels { .. } => unreachable!("handled above"),
        },
        // Cameras / lights / pivots: a small clickable box (their marker).
        _ => Vec3::splat(0.25),
    };
    (-half, half)
}

/// Ray vs axis-aligned box (slab test); the entry distance along `dir`.
pub fn ray_aabb(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let inv = Vec3::ONE / dir;
    let t1 = (min - origin) * inv;
    let t2 = (max - origin) * inv;
    let t_near = t1.min(t2).max_element();
    let t_far = t1.max(t2).min_element();
    (t_far >= t_near.max(0.0)).then_some(t_near.max(0.0))
}

/// The nearest Node3D under a world ray (tested in each node's local space,
/// so rotated / scaled nodes pick by their real box).
#[allow(dead_code)]
pub fn pick(artboard: &Artboard, origin: Vec3, dir: Vec3, model_bounds: &dyn Fn(an1mbyt3_core::ModelId) -> Option<(Vec3, Vec3)>) -> Option<NodeId> {
    pick_with_distance(artboard, origin, dir, model_bounds).map(|(_, id)| id)
}

/// [`pick`], plus how far along the ray the hit is (so other hits — live
/// particles — can be compared against it). Planes score `t + 1e6`.
pub fn pick_with_distance(artboard: &Artboard, origin: Vec3, dir: Vec3, model_bounds: &dyn Fn(an1mbyt3_core::ModelId) -> Option<(Vec3, Vec3)>) -> Option<(f32, NodeId)> {
    let mut best: Option<(f32, NodeId)> = None;
    for id in artboard.node_ids() {
        if artboard.is_effectively_deleted(id) {
            continue;
        }
        let Some(node) = artboard.node(id) else { continue };
        let NodeKind::Node3D(n) = &node.kind else { continue };
        if !visible_chain(artboard, id) {
            continue;
        }
        let Some(world) = artboard.world_matrix_3d(id) else { continue };
        let inv = world.inverse();
        let local_origin = inv.transform_point3(origin);
        let local_dir = inv.transform_vector3(dir);
        let (min, max) = local_bounds_with(&n.content, model_bounds);
        // Big flat planes (floors) only win when nothing else is hit, so a
        // click on a prop standing on the floor selects the prop.
        let is_plane = matches!(n.content, Node3DContent::Mesh(m) if matches!(m.primitive, Primitive3D::Plane { .. }));
        if let Some(t) = ray_aabb(local_origin, local_dir, min, max) {
            // `t` is in units of `dir` (world), since the transform is linear.
            let score = if is_plane { t + 1.0e6 } else { t };
            if best.is_none_or(|(b, _)| score < b) {
                best = Some((score, id));
            }
        }
    }
    best
}

/// Gizmo handles: the three axes and the centre (move on the ground plane).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoHandle {
    X,
    Y,
    Z,
    Plane,
}

impl GizmoHandle {
    pub fn axis(self) -> Vec3 {
        match self {
            GizmoHandle::X => Vec3::X,
            GizmoHandle::Y => Vec3::Y,
            GizmoHandle::Z => Vec3::Z,
            GizmoHandle::Plane => Vec3::ZERO,
        }
    }
    pub fn index(self) -> u8 {
        self as u8
    }
    pub fn from_index(i: u8) -> Option<Self> {
        [GizmoHandle::X, GizmoHandle::Y, GizmoHandle::Z, GizmoHandle::Plane].get(i as usize).copied()
    }
}

/// Distance from `p` to the segment `a..b` (2D).
fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t).distance(p)
}

/// Which gizmo handle (at world `origin`) is under the viewport point.
pub fn gizmo_hit(camera: &EditorCamera3D, origin: Vec3, point: Vec2, viewport: Vec2) -> Option<GizmoHandle> {
    let scale = camera.gizmo_scale(origin);
    let o = camera.project(origin, viewport)?;
    if o.distance(point) < 14.0 {
        return Some(GizmoHandle::Plane);
    }
    let mut best: Option<(f32, GizmoHandle)> = None;
    for handle in [GizmoHandle::X, GizmoHandle::Y, GizmoHandle::Z] {
        let Some(tip) = camera.project(origin + handle.axis() * scale * 1.15, viewport) else { continue };
        let d = segment_distance(point, o, tip);
        if d < 10.0 && best.is_none_or(|(b, _)| d < b) {
            best = Some((d, handle));
        }
    }
    best.map(|(_, h)| h)
}

/// How far (meters) a drag from `from` to `to` (pixels) moves along `axis`
/// at world `origin` — the drag projected onto the axis's screen direction.
pub fn axis_drag(camera: &EditorCamera3D, origin: Vec3, axis: Vec3, from: Vec2, to: Vec2, viewport: Vec2) -> f32 {
    let (Some(a), Some(b)) = (camera.project(origin, viewport), camera.project(origin + axis, viewport)) else { return 0.0 };
    let screen = b - a;
    let pixels_per_meter = screen.length();
    if pixels_per_meter < 1e-3 {
        return 0.0;
    }
    (to - from).dot(screen / pixels_per_meter) / pixels_per_meter
}

/// Where a viewport point's ray meets the horizontal plane `y = height`.
pub fn ground_point(camera: &EditorCamera3D, point: Vec2, viewport: Vec2, height: f32) -> Option<Vec3> {
    let (o, d) = camera.ray(point, viewport);
    if d.y.abs() < 1e-5 {
        return None;
    }
    let t = (height - o.y) / d.y;
    (t > 0.0).then(|| o + d * t)
}

/// The 12 edges of a box (`min..max` in the space `matrix` maps to world).
pub fn box_edges(matrix: Mat4, min: Vec3, max: Vec3) -> Vec<(Vec3, Vec3)> {
    let c = |x: bool, y: bool, z: bool| matrix.transform_point3(Vec3::new(if x { max.x } else { min.x }, if y { max.y } else { min.y }, if z { max.z } else { min.z }));
    let mut edges = Vec::with_capacity(12);
    for a in [false, true] {
        for b in [false, true] {
            edges.push((c(false, a, b), c(true, a, b)));
            edges.push((c(a, false, b), c(a, true, b)));
            edges.push((c(a, b, false), c(a, b, true)));
        }
    }
    edges
}

/// A model matrix that stretches a unit cube into a thin bar from `a` to `b`.
pub fn bar_between(a: Vec3, b: Vec3, thickness: f32) -> Mat4 {
    let dir = b - a;
    let len = dir.length().max(1e-5);
    let rotation = glam::Quat::from_rotation_arc(Vec3::X, dir / len);
    Mat4::from_scale_rotation_translation(Vec3::new(len, thickness, thickness), rotation, (a + b) * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_centre_of_the_view_rays_toward_the_target() {
        let cam = EditorCamera3D::default();
        let viewport = Vec2::new(800.0, 600.0);
        let (o, d) = cam.ray(viewport / 2.0, viewport);
        let to_target = (cam.target - o).normalize();
        assert!(d.dot(to_target) > 0.999, "{d} vs {to_target}");
        let p = cam.project(cam.target, viewport).unwrap();
        assert!(p.distance(viewport / 2.0) < 0.5);
    }

    #[test]
    fn dragging_along_a_projected_axis_moves_that_far() {
        let cam = EditorCamera3D::default();
        let viewport = Vec2::new(800.0, 600.0);
        let a = cam.project(Vec3::ZERO, viewport).unwrap();
        let b = cam.project(Vec3::X * 2.0, viewport).unwrap();
        let moved = axis_drag(&cam, Vec3::ZERO, Vec3::X, a, b, viewport);
        assert!((moved - 2.0).abs() < 0.15, "{moved}");
    }

    #[test]
    fn ray_box_hits_and_misses() {
        assert!(ray_aabb(Vec3::new(0.0, 0.0, 5.0), Vec3::NEG_Z, Vec3::splat(-0.5), Vec3::splat(0.5)).is_some_and(|t| (t - 4.5).abs() < 1e-4));
        assert!(ray_aabb(Vec3::new(2.0, 0.0, 5.0), Vec3::NEG_Z, Vec3::splat(-0.5), Vec3::splat(0.5)).is_none());
    }
}
