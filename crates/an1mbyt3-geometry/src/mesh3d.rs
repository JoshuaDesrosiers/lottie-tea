//! Triangle meshes for 3D — the built-in [`Primitive3D`] shapes.
//!
//! Every mesh is indexed triangles with a per-vertex position, normal and
//! UV, wound counter-clockwise when seen from outside (the renderer culls
//! back faces). Hard-edged shapes (cube, cylinder caps) duplicate vertices
//! along their creases so each face gets its own flat normal.

use std::f32::consts::{PI, TAU};

use an1mbyt3_core::Primitive3D;
use glam::{Vec2, Vec3};

/// A 3D triangle mesh, ready to upload.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh3DData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl Mesh3DData {
    fn vertex(&mut self, position: Vec3, normal: Vec3, uv: Vec2) -> u32 {
        self.positions.push(position.into());
        self.normals.push(normal.normalize_or_zero().into());
        self.uvs.push(uv.into());
        (self.positions.len() - 1) as u32
    }

    fn triangle(&mut self, a: u32, b: u32, c: u32) {
        self.indices.extend([a, b, c]);
    }

    /// A quad `a b c d` (counter-clockwise from outside).
    fn quad(&mut self, a: u32, b: u32, c: u32, d: u32) {
        self.indices.extend([a, b, c, a, c, d]);
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// The mesh for a built-in primitive.
pub fn primitive_mesh(primitive: &Primitive3D) -> Mesh3DData {
    match *primitive {
        Primitive3D::Cube { size } => cube(size),
        Primitive3D::Sphere { radius, segments, rings } => sphere(radius, segments.clamp(3, 256), rings.clamp(2, 256)),
        Primitive3D::Plane { size, subdivisions } => plane(size, subdivisions.clamp(1, 256)),
        Primitive3D::Cylinder { radius, height, segments } => cylinder(radius, radius, height, segments.clamp(3, 256)),
        Primitive3D::Cone { radius, height, segments } => cylinder(radius, 0.0, height, segments.clamp(3, 256)),
        Primitive3D::Torus { radius, tube, segments, sides } => torus(radius, tube, segments.clamp(3, 256), sides.clamp(3, 256)),
        // Imported models carry their own meshes (`parse_gltf`).
        Primitive3D::Model { .. } => Mesh3DData::default(),
        // Block worlds are meshed from their runtime blocks
        // (`voxel::mesh_voxel_region`), not from the primitive alone.
        Primitive3D::Voxels { .. } => Mesh3DData::default(),
    }
}

fn cube(size: Vec3) -> Mesh3DData {
    let h = size * 0.5;
    let mut m = Mesh3DData::default();
    // (normal, u axis, v axis): u × v = normal, so the quad below is CCW.
    for (n, u, v) in [
        (Vec3::X, Vec3::NEG_Z, Vec3::Y),
        (Vec3::NEG_X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::X, Vec3::NEG_Z),
        (Vec3::NEG_Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
    ] {
        let center = n * h;
        let (du, dv) = (u * h, v * h);
        let a = m.vertex(center - du - dv, n, Vec2::new(0.0, 1.0));
        let b = m.vertex(center + du - dv, n, Vec2::new(1.0, 1.0));
        let c = m.vertex(center + du + dv, n, Vec2::new(1.0, 0.0));
        let d = m.vertex(center - du + dv, n, Vec2::new(0.0, 0.0));
        m.quad(a, b, c, d);
    }
    m
}

fn sphere(radius: f32, segments: u32, rings: u32) -> Mesh3DData {
    let mut m = Mesh3DData::default();
    for ring in 0..=rings {
        let v = ring as f32 / rings as f32;
        let phi = v * PI; // 0 at the top
        for seg in 0..=segments {
            let u = seg as f32 / segments as f32;
            let theta = u * TAU;
            let n = Vec3::new(phi.sin() * theta.cos(), phi.cos(), -phi.sin() * theta.sin());
            m.vertex(n * radius, n, Vec2::new(u, v));
        }
    }
    let row = segments + 1;
    for ring in 0..rings {
        for seg in 0..segments {
            let a = ring * row + seg;
            let (b, c, d) = (a + row, a + row + 1, a + 1);
            m.quad(a, b, c, d);
        }
    }
    m
}

fn plane(size: f32, subdivisions: u32) -> Mesh3DData {
    let mut m = Mesh3DData::default();
    let n = subdivisions;
    for z in 0..=n {
        for x in 0..=n {
            let (u, v) = (x as f32 / n as f32, z as f32 / n as f32);
            m.vertex(Vec3::new((u - 0.5) * size, 0.0, (v - 0.5) * size), Vec3::Y, Vec2::new(u, v));
        }
    }
    let row = n + 1;
    for z in 0..n {
        for x in 0..n {
            let a = z * row + x;
            // Seen from +Y: a (−x,−z) -> d (−x,+z) -> c -> b is CCW.
            m.quad(a, a + row, a + row + 1, a + 1);
        }
    }
    m
}

/// A cylinder (or, with `top == 0`, a cone) along Y, with caps.
fn cylinder(bottom: f32, top: f32, height: f32, segments: u32) -> Mesh3DData {
    let mut m = Mesh3DData::default();
    let h = height * 0.5;
    // Side: the normal leans by the slope so cones shade correctly.
    let slope = (bottom - top) / height.max(1e-6);
    let base = m.positions.len() as u32;
    for seg in 0..=segments {
        let u = seg as f32 / segments as f32;
        let a = u * TAU;
        let dir = Vec3::new(a.cos(), 0.0, -a.sin());
        let n = (dir + Vec3::Y * slope).normalize();
        m.vertex(dir * bottom - Vec3::Y * h, n, Vec2::new(u, 1.0));
        m.vertex(dir * top + Vec3::Y * h, n, Vec2::new(u, 0.0));
    }
    for seg in 0..segments {
        let i = base + seg * 2;
        m.quad(i, i + 2, i + 3, i + 1);
    }
    // Caps (a fan each).
    for (y, n, radius) in [(-h, Vec3::NEG_Y, bottom), (h, Vec3::Y, top)] {
        if radius <= 0.0 {
            continue;
        }
        let center = m.vertex(Vec3::Y * y, n, Vec2::splat(0.5));
        let first = m.positions.len() as u32;
        for seg in 0..=segments {
            let a = seg as f32 / segments as f32 * TAU;
            let dir = Vec3::new(a.cos(), 0.0, -a.sin());
            m.vertex(dir * radius + Vec3::Y * y, n, Vec2::new(0.5 + a.cos() * 0.5, 0.5 + a.sin() * 0.5));
        }
        for seg in 0..segments {
            let (p, q) = (first + seg, first + seg + 1);
            if n.y > 0.0 { m.triangle(center, p, q) } else { m.triangle(center, q, p) }
        }
    }
    m
}

fn torus(radius: f32, tube: f32, segments: u32, sides: u32) -> Mesh3DData {
    let mut m = Mesh3DData::default();
    for seg in 0..=segments {
        let u = seg as f32 / segments as f32;
        let a = u * TAU;
        let ring_center = Vec3::new(a.cos(), 0.0, -a.sin()) * radius;
        let outward = Vec3::new(a.cos(), 0.0, -a.sin());
        for side in 0..=sides {
            let v = side as f32 / sides as f32;
            let b = v * TAU;
            let n = outward * b.cos() + Vec3::Y * b.sin();
            m.vertex(ring_center + n * tube, n, Vec2::new(u, v));
        }
    }
    let row = sides + 1;
    for seg in 0..segments {
        for side in 0..sides {
            let a = seg * row + side;
            m.quad(a, a + row, a + row + 1, a + 1);
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every triangle faces outward: its geometric normal agrees with the
    /// vertex normals (counter-clockwise from outside).
    fn assert_outward(m: &Mesh3DData, name: &str) {
        assert!(!m.indices.is_empty(), "{name} has triangles");
        for tri in m.indices.chunks(3) {
            let p = |i: u32| Vec3::from(m.positions[i as usize]);
            let face = (p(tri[1]) - p(tri[0])).cross(p(tri[2]) - p(tri[0]));
            if face.length() < 1e-5 {
                continue; // degenerate (sphere poles)
            }
            let n = Vec3::from(m.normals[tri[0] as usize]) + Vec3::from(m.normals[tri[1] as usize]) + Vec3::from(m.normals[tri[2] as usize]);
            assert!(face.dot(n) > 0.0, "{name}: a triangle faces inward");
        }
    }

    #[test]
    fn every_primitive_is_wound_outward() {
        for (name, p) in [
            ("cube", Primitive3D::cube()),
            ("sphere", Primitive3D::sphere()),
            ("plane", Primitive3D::plane()),
            ("cylinder", Primitive3D::cylinder()),
            ("cone", Primitive3D::cone()),
            ("torus", Primitive3D::torus()),
        ] {
            assert_outward(&primitive_mesh(&p), name);
        }
    }

    #[test]
    fn cube_has_flat_faces() {
        let m = primitive_mesh(&Primitive3D::cube());
        assert_eq!(m.positions.len(), 24);
        assert_eq!(m.triangle_count(), 12);
    }
}
