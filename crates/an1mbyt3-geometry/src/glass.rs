//! Breakable glass — the runtime behind `Primitive3D::Glass`.
//!
//! A pane is a `width × height` rectangle in its local XY plane (facing
//! +Z), `thickness` deep, pre-cut into **Voronoi cells** about `shard` units
//! across (a jittered grid of sites, so the cut lines look like real
//! fracture lines, not a checkerboard). Intact, the cells draw as one
//! seamless sheet: only the pane's outer rim and the rims of holes get
//! side faces, so you never see the cut lines until something breaks.
//!
//! **Breaking** ([`GlassPane::shatter`]) is local, the way real glass goes:
//!
//! 1. Cells near the impact break out. The ones closest to it are split
//!    again into sharp triangular slivers (fanned from a random point inside
//!    the cell); further out, whole cells break off with a falling-off
//!    probability, so the hole's edge is jagged rather than a neat circle.
//! 2. Cells next to the hole are marked *cracked* (the material can whiten
//!    them — see the UV contract below).
//! 3. **Support:** a pane hangs off its *anchored* edges (`anchors`, a
//!    bitmask of [`SIDE_LEFT`]/[`SIDE_RIGHT`]/[`SIDE_BOTTOM`]/[`SIDE_TOP`]).
//!    A flood fill from every intact cell touching an anchored edge, through
//!    shared cell edges, finds what's still held up; anything not reached
//!    has lost its support and falls as loose chunks. So a glass bar held
//!    top and bottom loses only a bite out of its middle (both stubs stay),
//!    a pane hanging from its top edge drops everything below a hole that
//!    cuts right across it, and a framed pane (all four anchors) only ever
//!    loses the pieces that were actually hit.
//!
//! Broken pieces become [`Shard`]s, simulated here on the CPU in world
//! space: thrown along the hit's direction (strongest at the centre),
//! spinning, falling under gravity and bouncing to rest on a floor height
//! the caller gives, then shrinking away at the end of their life.
//!
//! **UV contract** for materials on glass (both meshes): `u` is 0 on the
//! front/back faces and 1 on side faces (the thick edges of holes, the rim,
//! every face of a shard — glass edges read brighter/greener than faces);
//! `v` is how cracked a face is (0 clean … 1 right beside a hole; shards
//! use 0.5).
//!
//! Everything is deterministic for a given `seed` (cell layout) plus the
//! sequence of shatters (each mixes its own seed in).

use glam::{Mat3, Mat4, Quat, Vec2, Vec3};

use crate::Mesh3DData;

/// `anchors` bit: the pane is held along its left edge (−X).
pub const SIDE_LEFT: u32 = 1;
/// `anchors` bit: held along its right edge (+X).
pub const SIDE_RIGHT: u32 = 2;
/// `anchors` bit: held along its bottom edge (−Y).
pub const SIDE_BOTTOM: u32 = 4;
/// `anchors` bit: held along its top edge (+Y).
pub const SIDE_TOP: u32 = 8;
/// Held on every side — a pane in a frame.
pub const SIDE_ALL: u32 = 15;

/// Most live shards one pane keeps; the oldest go first beyond this.
const MAX_SHARDS: usize = 160;
/// Grid limit per axis, so a huge pane with tiny shards stays affordable.
const MAX_GRID: usize = 28;
/// Gravity on shards (m/s²).
const GRAVITY: f32 = 9.81;

// Edge labels: an edge of a cell polygon either borders another cell (its
// index, ≥ 0) or one of the pane's four sides (these negatives).
const EDGE_LEFT: i32 = -1;
const EDGE_RIGHT: i32 = -2;
const EDGE_BOTTOM: i32 = -3;
const EDGE_TOP: i32 = -4;

/// The `anchors` bit for a boundary edge label (0 for a cell neighbour).
fn side_bit(label: i32) -> u32 {
    match label {
        EDGE_LEFT => SIDE_LEFT,
        EDGE_RIGHT => SIDE_RIGHT,
        EDGE_BOTTOM => SIDE_BOTTOM,
        EDGE_TOP => SIDE_TOP,
        _ => 0,
    }
}

/// A tiny deterministic RNG (SplitMix64) — glass must break the same way
/// for the same seed, on every platform.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in `0..1`.
    fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    /// Uniform in `-1..1`.
    fn signed(&mut self) -> f32 {
        self.f32() * 2.0 - 1.0
    }
    /// A random direction-ish vector in the unit cube.
    fn vec3(&mut self) -> Vec3 {
        Vec3::new(self.signed(), self.signed(), self.signed())
    }
}

/// One pre-cut piece of a pane: a convex polygon (counter-clockwise, seen
/// from +Z) whose edge `i` (from `points[i]` to `points[i + 1]`) borders
/// whatever `labels[i]` says — another cell's index, or a pane side.
#[derive(Debug, Clone)]
pub struct GlassCell {
    pub points: Vec<Vec2>,
    pub labels: Vec<i32>,
    pub centroid: Vec2,
    pub area: f32,
}

/// A broken-off piece in flight (or resting on the floor).
#[derive(Debug, Clone)]
pub struct Shard {
    /// Its outline, relative to its own centre, in the pane's local units.
    points: Vec<Vec2>,
    /// The pane's world rotation × scale when it broke (shards keep the
    /// pane's size and facing as their starting pose).
    basis: Mat3,
    /// World position of its centre, and its tumbling on top of `basis`.
    pos: Vec3,
    rot: Quat,
    vel: Vec3,
    /// Angular velocity (rad/s, world axes).
    spin: Vec3,
    age: f32,
    life: f32,
    /// Rough world-space size, for resting on the floor.
    reach: f32,
}

/// Parameters of one hit — see [`GlassPane::shatter`].
#[derive(Debug, Clone, Copy)]
pub struct ShatterParams {
    /// Where it hit, in the pane's local XY.
    pub point: Vec2,
    /// How far the breakage reaches, in local units.
    pub radius: f32,
    /// The hitting object's velocity (world, m/s): shards are thrown along
    /// it, hardest at the centre of the hit.
    pub velocity: Vec3,
    /// The pane's world matrix now (shards start where their cell was).
    pub world: Mat4,
    /// World height shards land on (`f32::NEG_INFINITY`: they fall forever).
    pub floor: f32,
    /// Mixed into the break pattern, so two hits on the same spot differ.
    pub seed: u32,
}

/// What a hit broke.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShatterResult {
    /// Cells broken out by the hit itself.
    pub broken: usize,
    /// Cells that lost their support and fell.
    pub loose: usize,
}

impl ShatterResult {
    pub fn total(&self) -> usize {
        self.broken + self.loose
    }
}

/// A pane's whole runtime state: its cells, which are still in place, and
/// its flying shards.
#[derive(Debug, Clone)]
pub struct GlassPane {
    pub width: f32,
    pub height: f32,
    pub thickness: f32,
    pub anchors: u32,
    pub cells: Vec<GlassCell>,
    intact: Vec<bool>,
    cracked: Vec<f32>,
    shards: Vec<Shard>,
    /// Bumped whenever the intact cells change (a mesh cache's key).
    pub version: u64,
    hits: u64,
    /// World height shards land on (from the latest hit).
    floor: f32,
}

impl GlassPane {
    /// Cuts a fresh, unbroken pane. `shard` is the typical cell size (local
    /// units); `seed` picks the crack pattern.
    pub fn new(width: f32, height: f32, thickness: f32, shard: f32, seed: u32, anchors: u32) -> Self {
        let (w, h) = (width.max(0.01), height.max(0.01));
        let s = shard.clamp(0.01, w.max(h));
        let cols = ((w / s).round() as usize).clamp(1, MAX_GRID);
        let rows = ((h / s).round() as usize).clamp(1, MAX_GRID);
        let (cw, ch) = (w / cols as f32, h / rows as f32);
        let mut rng = Rng::new(seed as u64 * 7919 + 17);
        // One site per grid square, jittered well inside it: neighbouring
        // Voronoi cells are then always within two squares of each other.
        let mut sites = Vec::with_capacity(cols * rows);
        for row in 0..rows {
            for col in 0..cols {
                let jx = if cols > 1 { rng.signed() * 0.42 } else { rng.signed() * 0.3 };
                let jy = if rows > 1 { rng.signed() * 0.42 } else { rng.signed() * 0.3 };
                sites.push(Vec2::new(-w * 0.5 + (col as f32 + 0.5 + jx) * cw, -h * 0.5 + (row as f32 + 0.5 + jy) * ch));
            }
        }
        let rect = [Vec2::new(-w * 0.5, -h * 0.5), Vec2::new(w * 0.5, -h * 0.5), Vec2::new(w * 0.5, h * 0.5), Vec2::new(-w * 0.5, h * 0.5)];
        // (Edge from corner i to i + 1: bottom, right, top, left.)
        let rect_labels = [EDGE_BOTTOM, EDGE_RIGHT, EDGE_TOP, EDGE_LEFT];
        let mut cells = Vec::with_capacity(sites.len());
        for row in 0..rows {
            for col in 0..cols {
                let i = row * cols + col;
                let mut points = rect.to_vec();
                let mut labels = rect_labels.to_vec();
                for nr in row.saturating_sub(2)..(row + 3).min(rows) {
                    for nc in col.saturating_sub(2)..(col + 3).min(cols) {
                        let j = nr * cols + nc;
                        if j != i {
                            clip_half_plane(&mut points, &mut labels, sites[i], sites[j], j as i32);
                        }
                    }
                }
                let (centroid, area) = polygon_centroid(&points);
                cells.push(GlassCell { points, labels, centroid, area });
            }
        }
        // Cells are indexed by site; an empty (degenerate) cell just never
        // draws or gets hit, so indices stay valid.
        let n = cells.len();
        Self { width: w, height: h, thickness: thickness.max(0.0005), anchors, cells, intact: vec![true; n], cracked: vec![0.0; n], shards: Vec::new(), version: 1, hits: 0, floor: f32::NEG_INFINITY }
    }

    /// Puts every piece back (and drops the flying shards).
    pub fn reset(&mut self) {
        self.intact.iter_mut().for_each(|v| *v = true);
        self.cracked.iter_mut().for_each(|v| *v = 0.0);
        self.shards.clear();
        self.version += 1;
    }

    /// How much of the pane (by area) is still in place, `0..1`.
    pub fn intact_fraction(&self) -> f32 {
        let total: f32 = self.cells.iter().map(|c| c.area).sum();
        let left: f32 = self.cells.iter().zip(&self.intact).filter(|(_, i)| **i).map(|(c, _)| c.area).sum();
        if total > 0.0 { left / total } else { 0.0 }
    }

    pub fn is_intact(&self, cell: usize) -> bool {
        self.intact.get(cell).copied().unwrap_or(false)
    }

    /// Live shards right now.
    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }

    /// The intact cell under local point `p`, if any (holes return `None`).
    pub fn cell_at(&self, p: Vec2) -> Option<usize> {
        if p.x.abs() > self.width * 0.5 || p.y.abs() > self.height * 0.5 {
            return None;
        }
        (0..self.cells.len()).find(|&i| self.intact[i] && self.cells[i].points.len() >= 3 && point_in_convex(&self.cells[i].points, p))
    }

    /// A ray in the pane's local space (`dir` need not be unit length) vs
    /// the glass that's still there: the ray parameter `t` (in units of
    /// `dir`, so the same `t` works in world space when the ray was
    /// transformed without normalising) and the local point hit. Rays
    /// through holes, or parallel to the pane, miss. The pane is treated as
    /// infinitely thin (its mid-plane).
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max_t: f32) -> Option<(f32, Vec2)> {
        if dir.z.abs() < 1e-8 {
            return None;
        }
        let t = -origin.z / dir.z;
        if !(0.0..=max_t).contains(&t) {
            return None;
        }
        let p = (origin + dir * t).truncate();
        self.cell_at(p).map(|_| (t, p))
    }

    /// Cracks the pane around local `point` without breaking anything —
    /// thick glass taking a hit it survives. Pieces within `radius` (and the
    /// one under the point) whiten by `amount` (0..1, accumulating), so the
    /// impact shows as a frosted star that grows with each hit. Returns how
    /// many pieces it touched (0 for a hit on a hole).
    pub fn crack(&mut self, point: Vec2, radius: f32, amount: f32) -> usize {
        let Some(hit_cell) = self.cell_at(point) else { return 0 };
        let radius = radius.max(0.001);
        let mut touched = 0;
        for i in 0..self.cells.len() {
            if !self.intact[i] || self.cells[i].points.len() < 3 {
                continue;
            }
            let d = distance_to_polygon(&self.cells[i].points, point);
            if i == hit_cell || d < radius {
                let falloff = if i == hit_cell { 1.0 } else { 1.0 - d / radius };
                self.cracked[i] = (self.cracked[i] + amount * falloff).min(1.0);
                touched += 1;
            }
        }
        self.version += 1;
        touched
    }

    /// Breaks the pane around `params.point` (see the module doc for how).
    /// A hit on a hole breaks nothing.
    pub fn shatter(&mut self, params: &ShatterParams) -> ShatterResult {
        let Some(hit_cell) = self.cell_at(params.point) else { return ShatterResult::default() };
        self.hits += 1;
        let mut rng = Rng::new((params.seed as u64) << 20 ^ self.hits.wrapping_mul(0x2545_F491_4F6C_DD1D));
        let radius = params.radius.max(0.001);
        let mut newly_broken: Vec<(usize, f32)> = Vec::new();
        for i in 0..self.cells.len() {
            if !self.intact[i] || self.cells[i].points.len() < 3 {
                continue;
            }
            let d = distance_to_polygon(&self.cells[i].points, params.point);
            // Certain inside 55 % of the radius, then a falling chance out
            // to 140 % — jagged hole edges, never a clean circle.
            let breaks = i == hit_cell || d < radius * 0.55 || (d < radius * 1.4 && rng.f32() < (1.0 - (d - radius * 0.55) / (radius * 0.85)) * 0.75);
            if breaks {
                newly_broken.push((i, d / radius));
            }
        }
        for &(i, _) in &newly_broken {
            self.intact[i] = false;
        }
        // Cracks round the hole.
        for &(i, _) in &newly_broken {
            for &label in &self.cells[i].labels {
                if label >= 0 && self.intact[label as usize] {
                    let c = &mut self.cracked[label as usize];
                    *c = (*c + 0.7).min(1.0);
                }
            }
        }
        let loose = self.unsupported();
        for &i in &loose {
            self.intact[i] = false;
        }

        let world = params.world;
        let basis = Mat3::from_mat4(world);
        let speed = params.velocity.length();
        let along = params.velocity.normalize_or_zero();
        let hit_world = world.transform_point3(params.point.extend(0.0));
        for &(i, near) in &newly_broken {
            let cell = &self.cells[i];
            // The heart of the hit splinters into slivers.
            let pieces: Vec<Vec<Vec2>> = if near < 0.45 && cell.points.len() >= 3 { splinter(&cell.points, &mut rng) } else { vec![cell.points.clone()] };
            for piece in pieces {
                let (centre, _) = polygon_centroid(&piece);
                let pos = world.transform_point3(centre.extend(0.0));
                let falloff = (1.0 - near * 0.6).clamp(0.25, 1.0);
                let outward = (pos - hit_world).normalize_or_zero();
                let vel = along * speed * (0.3 + rng.f32() * 0.45) * falloff + outward * speed * (0.05 + rng.f32() * 0.15) + rng.vec3() * (0.4 + speed * 0.04) + Vec3::Y * rng.f32() * 1.2;
                let spin = rng.vec3() * (4.0 + 10.0 * falloff);
                self.push_shard(piece, centre, basis, pos, vel, spin, 2.4 + rng.f32() * 1.4);
            }
        }
        for &i in &loose {
            let cell = &self.cells[i];
            let pos = world.transform_point3(cell.centroid.extend(0.0));
            // Loose chunks mostly just drop, nudged a little by the blow.
            let vel = along * speed * (0.04 + rng.f32() * 0.06) + rng.vec3() * 0.35;
            let spin = rng.vec3() * 1.8;
            self.push_shard(cell.points.clone(), cell.centroid, basis, pos, vel, spin, 2.8 + rng.f32() * 1.4);
        }
        if self.shards.len() > MAX_SHARDS {
            let excess = self.shards.len() - MAX_SHARDS;
            self.shards.drain(..excess);
        }
        self.floor = params.floor;
        self.version += 1;
        ShatterResult { broken: newly_broken.len(), loose: loose.len() }
    }

    fn push_shard(&mut self, outline: Vec<Vec2>, centre: Vec2, basis: Mat3, pos: Vec3, vel: Vec3, spin: Vec3, life: f32) {
        let points: Vec<Vec2> = outline.iter().map(|p| *p - centre).collect();
        let local_reach = points.iter().map(|p| p.length()).fold(0.0f32, f32::max);
        let scale = (basis.x_axis.length() + basis.y_axis.length()) * 0.5;
        self.shards.push(Shard { points, basis, pos, rot: Quat::IDENTITY, vel, spin, age: 0.0, life, reach: local_reach * scale * 0.5 });
    }

    /// Intact cells no longer connected (through intact neighbours) to an
    /// anchored edge. With no anchors at all, nothing is held: every
    /// remaining piece is loose once any of it breaks.
    fn unsupported(&self) -> Vec<usize> {
        let n = self.cells.len();
        let mut held = vec![false; n];
        let mut stack: Vec<usize> = (0..n).filter(|&i| self.intact[i] && self.cells[i].labels.iter().any(|&l| side_bit(l) & self.anchors != 0)).collect();
        for &i in &stack {
            held[i] = true;
        }
        while let Some(i) = stack.pop() {
            for &label in &self.cells[i].labels {
                if label >= 0 {
                    let j = label as usize;
                    if self.intact[j] && !held[j] {
                        held[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        (0..n).filter(|&i| self.intact[i] && !held[i] && self.cells[i].points.len() >= 3).collect()
    }

    /// Advances the shards `dt` seconds. True while any are left (their
    /// mesh then needs rebuilding every frame).
    pub fn step(&mut self, dt: f32) -> bool {
        let dt = dt.clamp(0.0, 0.05);
        let floor = self.floor;
        for s in &mut self.shards {
            s.age += dt;
            s.vel.y -= GRAVITY * dt;
            // A little air drag, so slivers flutter rather than fly forever.
            s.vel *= 1.0 - (0.35 * dt).min(0.5);
            s.pos += s.vel * dt;
            let angle = s.spin.length() * dt;
            if angle > 1e-6 {
                s.rot = (Quat::from_axis_angle(s.spin.normalize(), angle) * s.rot).normalize();
            }
            if s.pos.y - s.reach < floor {
                s.pos.y = floor + s.reach;
                if s.vel.y < 0.0 {
                    s.vel.y = -s.vel.y * 0.25;
                }
                s.vel.x *= 0.6;
                s.vel.z *= 0.6;
                s.spin *= 0.5;
                // Settle flat-ish: resting shards stop tumbling.
                if s.vel.y < 0.4 {
                    s.vel.y = 0.0;
                    s.spin *= 0.2;
                }
            }
        }
        self.shards.retain(|s| s.age < s.life);
        !self.shards.is_empty()
    }

    /// The still-intact glass, in the pane's local space (draw it with the
    /// pane's world matrix). See the module doc for the UV contract.
    pub fn intact_mesh(&self) -> Mesh3DData {
        let mut m = Mesh3DData::default();
        let half = self.thickness * 0.5;
        for (i, cell) in self.cells.iter().enumerate() {
            if !self.intact[i] || cell.points.len() < 3 {
                continue;
            }
            let crack = self.cracked[i];
            // Side faces only where this cell borders a hole or the rim —
            // internal cuts stay invisible while both sides are intact.
            let sides: Vec<bool> = cell.labels.iter().map(|&l| l < 0 || !self.intact[l as usize]).collect();
            prism(&mut m, &cell.points, -half, half, crack, &sides, |p| p, |n| n);
        }
        m
    }

    /// Every live shard, in **world** space (draw with an identity matrix).
    pub fn shards_mesh(&self) -> Mesh3DData {
        let mut m = Mesh3DData::default();
        let half = self.thickness * 0.5;
        for s in &self.shards {
            // Shrink away over the last half second.
            let fade = ((s.life - s.age) / 0.5).clamp(0.0, 1.0);
            let basis = s.basis * fade.max(0.001);
            let normal_basis = basis.inverse().transpose();
            let all = vec![true; s.points.len()];
            prism(&mut m, &s.points, -half, half, 0.5, &all, |p| s.pos + s.rot * (basis * p), |n| s.rot * (normal_basis * n));
        }
        m
    }
}

/// Clips `points` (convex, CCW) to the side of the bisector between `site`
/// and `other` nearer `site`, keeping edge labels: kept parts of old edges
/// keep theirs, the new cut edge gets `label` (the other cell).
fn clip_half_plane(points: &mut Vec<Vec2>, labels: &mut Vec<i32>, site: Vec2, other: Vec2, label: i32) {
    let normal = other - site;
    let mid = (site + other) * 0.5;
    let side = |p: Vec2| (p - mid).dot(normal);
    let n = points.len();
    if n == 0 {
        return;
    }
    if points.iter().all(|p| side(*p) <= 0.0) {
        return;
    }
    let mut out_points = Vec::with_capacity(n + 1);
    let mut out_labels = Vec::with_capacity(n + 1);
    for k in 0..n {
        let a = points[k];
        let b = points[(k + 1) % n];
        let (sa, sb) = (side(a), side(b));
        let a_in = sa <= 0.0;
        let b_in = sb <= 0.0;
        let cross = || a + (b - a) * (sa / (sa - sb));
        match (a_in, b_in) {
            (true, true) => {
                out_points.push(a);
                out_labels.push(labels[k]);
            }
            (true, false) => {
                out_points.push(a);
                out_labels.push(labels[k]);
                // Leaving: the stretch from here to where we re-enter is
                // the new cut, bordering `other`.
                out_points.push(cross());
                out_labels.push(label);
            }
            (false, true) => {
                // Entering: the rest of this edge is still the old edge.
                out_points.push(cross());
                out_labels.push(labels[k]);
            }
            (false, false) => {}
        }
    }
    // Drop zero-length edges (a cut passing exactly through a corner).
    // Edge i (point i → i+1) collapsing means point i+1 goes and edge i
    // takes over the label of the edge that started there.
    let mut i = 0;
    while out_points.len() > 2 && i < out_points.len() {
        let j = (i + 1) % out_points.len();
        if out_points[i].distance_squared(out_points[j]) < 1e-12 {
            if j == 0 {
                // The closing edge collapsed: drop the last point; the
                // edge before it now runs to point 0, keeping its label.
                out_points.pop();
                out_labels.pop();
            } else {
                out_labels[i] = out_labels[j];
                out_points.remove(j);
                out_labels.remove(j);
            }
        } else {
            i += 1;
        }
    }
    *points = out_points;
    *labels = out_labels;
}

/// Area-weighted centroid and (positive, for CCW) area of a polygon.
fn polygon_centroid(points: &[Vec2]) -> (Vec2, f32) {
    if points.len() < 3 {
        return (points.first().copied().unwrap_or(Vec2::ZERO), 0.0);
    }
    let mut area = 0.0;
    let mut c = Vec2::ZERO;
    for k in 0..points.len() {
        let (a, b) = (points[k], points[(k + 1) % points.len()]);
        let cross = a.perp_dot(b);
        area += cross;
        c += (a + b) * cross;
    }
    if area.abs() < 1e-12 {
        let mean = points.iter().copied().sum::<Vec2>() / points.len() as f32;
        return (mean, 0.0);
    }
    (c / (3.0 * area), area * 0.5)
}

fn point_in_convex(points: &[Vec2], p: Vec2) -> bool {
    (0..points.len()).all(|k| (points[(k + 1) % points.len()] - points[k]).perp_dot(p - points[k]) >= -1e-7)
}

/// 0 inside, else the distance to the nearest edge.
fn distance_to_polygon(points: &[Vec2], p: Vec2) -> f32 {
    if point_in_convex(points, p) {
        return 0.0;
    }
    (0..points.len())
        .map(|k| {
            let (a, b) = (points[k], points[(k + 1) % points.len()]);
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
            p.distance(a + ab * t)
        })
        .fold(f32::INFINITY, f32::min)
}

/// Splits a convex cell into sharp triangular slivers fanned from a random
/// point inside it (long edges are split once more, so slivers stay thin).
fn splinter(points: &[Vec2], rng: &mut Rng) -> Vec<Vec<Vec2>> {
    let (centre, _) = polygon_centroid(points);
    // A random interior point: a random blend towards a random vertex.
    let toward = points[(rng.next_u64() as usize) % points.len()];
    let core = centre + (toward - centre) * rng.f32() * 0.6;
    let mut ring = Vec::with_capacity(points.len() * 2);
    let perimeter: f32 = (0..points.len()).map(|k| points[k].distance(points[(k + 1) % points.len()])).sum();
    let mean = perimeter / points.len() as f32;
    for k in 0..points.len() {
        let (a, b) = (points[k], points[(k + 1) % points.len()]);
        ring.push(a);
        if a.distance(b) > mean * 0.9 {
            ring.push(a + (b - a) * (0.35 + rng.f32() * 0.3));
        }
    }
    (0..ring.len()).map(|k| vec![core, ring[k], ring[(k + 1) % ring.len()]]).collect()
}

/// Extrudes a CCW polygon between `z0` (back) and `z1` (front), mapping
/// positions/normals through `place`/`turn`. Side face `k` (edge k → k+1)
/// is emitted only when `sides[k]`. UV: faces `(0, v)`, sides `(1, v)`.
fn prism(m: &mut Mesh3DData, points: &[Vec2], z0: f32, z1: f32, v: f32, sides: &[bool], place: impl Fn(Vec3) -> Vec3, turn: impl Fn(Vec3) -> Vec3) {
    let n = points.len();
    let mut push = |p: Vec3, normal: Vec3, uv: Vec2| -> u32 {
        m.positions.push(place(p).into());
        m.normals.push(turn(normal).normalize_or_zero().into());
        m.uvs.push(uv.into());
        (m.positions.len() - 1) as u32
    };
    let face_uv = Vec2::new(0.0, v);
    let side_uv = Vec2::new(1.0, v);
    let front: Vec<u32> = points.iter().map(|p| push(p.extend(z1), Vec3::Z, face_uv)).collect();
    let back: Vec<u32> = points.iter().map(|p| push(p.extend(z0), Vec3::NEG_Z, face_uv)).collect();
    let mut side_quads = Vec::new();
    for k in 0..n {
        if !sides.get(k).copied().unwrap_or(true) {
            continue;
        }
        let (a, b) = (points[k], points[(k + 1) % n]);
        let d = b - a;
        let outward = Vec3::new(d.y, -d.x, 0.0);
        let a0 = push(a.extend(z0), outward, side_uv);
        let b0 = push(b.extend(z0), outward, side_uv);
        let b1 = push(b.extend(z1), outward, side_uv);
        let a1 = push(a.extend(z1), outward, side_uv);
        side_quads.push([a0, b0, b1, a1]);
    }
    for k in 1..n.saturating_sub(1) {
        m.indices.extend([front[0], front[k], front[k + 1]]);
        m.indices.extend([back[0], back[k + 1], back[k]]);
    }
    for [a0, b0, b1, a1] in side_quads {
        m.indices.extend([a0, b0, b1, a0, b1, a1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(pane: &mut GlassPane, x: f32, y: f32, radius: f32) -> ShatterResult {
        pane.shatter(&ShatterParams { point: Vec2::new(x, y), radius, velocity: Vec3::new(0.0, 0.0, -20.0), world: Mat4::IDENTITY, floor: 0.0, seed: 3 })
    }

    #[test]
    fn cells_tile_the_pane() {
        let pane = GlassPane::new(2.0, 1.5, 0.02, 0.25, 7, SIDE_ALL);
        let total: f32 = pane.cells.iter().map(|c| c.area).sum();
        assert!((total - 3.0).abs() < 1e-3, "cells cover {total}, not 3.0");
        assert!(pane.cells.len() > 30);
    }

    #[test]
    fn adjacency_is_symmetric() {
        let pane = GlassPane::new(1.6, 1.0, 0.02, 0.2, 11, SIDE_ALL);
        for (i, cell) in pane.cells.iter().enumerate() {
            assert_eq!(cell.points.len(), cell.labels.len());
            for &l in &cell.labels {
                if l >= 0 {
                    assert!(pane.cells[l as usize].labels.contains(&(i as i32)), "cell {i} borders {l} but not back");
                }
            }
        }
    }

    #[test]
    fn intact_pane_has_no_inner_sides() {
        let pane = GlassPane::new(1.0, 1.0, 0.02, 0.25, 1, SIDE_ALL);
        let m = pane.intact_mesh();
        // Only rim side faces: every side vertex lies on the outer boundary.
        for (p, uv) in m.positions.iter().zip(&m.uvs) {
            if uv[0] > 0.5 {
                assert!((p[0].abs() - 0.5).abs() < 1e-4 || (p[1].abs() - 0.5).abs() < 1e-4, "inner side face at {p:?}");
            }
        }
    }

    #[test]
    fn hits_make_holes_rays_pass_through() {
        let mut pane = GlassPane::new(2.0, 2.0, 0.02, 0.25, 5, SIDE_ALL);
        let ray = |pane: &GlassPane| pane.raycast(Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, -1.0), 100.0);
        assert!(ray(&pane).is_some());
        let r = hit(&mut pane, 0.0, 0.0, 0.3);
        assert!(r.broken >= 1);
        assert_eq!(r.loose, 0, "a framed pane only loses what was hit");
        assert!(ray(&pane).is_none(), "the ray goes through the hole");
        assert!(pane.raycast(Vec3::new(0.9, 0.9, 5.0), Vec3::new(0.0, 0.0, -1.0), 100.0).is_some(), "the corner is still there");
        assert!(pane.shard_count() > r.broken, "the centre splinters into slivers");
        // Hitting the hole again breaks nothing.
        assert_eq!(hit(&mut pane, 0.0, 0.0, 0.01).total(), 0);
    }

    #[test]
    fn bar_held_at_both_ends_keeps_its_stubs() {
        let mut bar = GlassPane::new(0.15, 3.0, 0.08, 0.3, 9, SIDE_TOP | SIDE_BOTTOM);
        let r = hit(&mut bar, 0.0, 0.0, 0.2);
        assert!(r.broken >= 1);
        assert_eq!(r.loose, 0);
        assert!(bar.cell_at(Vec2::new(0.0, 1.4)).is_some() && bar.cell_at(Vec2::new(0.0, -1.4)).is_some());
        assert!(bar.intact_fraction() > 0.5);
    }

    #[test]
    fn hanging_pane_drops_what_is_cut_off() {
        let mut pane = GlassPane::new(0.3, 3.0, 0.05, 0.3, 4, SIDE_TOP);
        let r = hit(&mut pane, 0.0, 0.6, 0.25);
        assert!(r.loose >= 1, "everything below the cut should fall");
        assert!(pane.cell_at(Vec2::new(0.0, -1.4)).is_none());
        assert!(pane.cell_at(Vec2::new(0.0, 1.4)).is_some());
    }

    #[test]
    fn shards_fall_land_and_expire() {
        let mut pane = GlassPane::new(1.0, 1.0, 0.02, 0.25, 2, SIDE_ALL);
        hit(&mut pane, 0.0, 0.0, 0.3);
        let mesh = pane.shards_mesh();
        assert!(!mesh.indices.is_empty());
        for _ in 0..60 {
            pane.step(1.0 / 60.0);
        }
        assert!(pane.shards.iter().all(|s| s.pos.y >= -1e-3), "shards rest on the floor");
        for _ in 0..400 {
            pane.step(1.0 / 60.0);
        }
        assert_eq!(pane.shard_count(), 0);
    }

    #[test]
    fn crack_whitens_without_breaking() {
        let mut pane = GlassPane::new(1.0, 1.0, 0.1, 0.2, 3, SIDE_ALL);
        assert!(pane.crack(Vec2::ZERO, 0.3, 0.6) >= 1);
        assert!((pane.intact_fraction() - 1.0).abs() < 1e-6, "nothing breaks");
        let cell = pane.cell_at(Vec2::ZERO).unwrap();
        assert!(pane.cracked[cell] > 0.5);
        assert_eq!(pane.shard_count(), 0);
    }

    #[test]
    fn reset_restores_everything() {
        let mut pane = GlassPane::new(1.0, 1.0, 0.02, 0.25, 2, SIDE_ALL);
        hit(&mut pane, 0.0, 0.0, 0.4);
        pane.reset();
        assert!((pane.intact_fraction() - 1.0).abs() < 1e-6);
        assert_eq!(pane.shard_count(), 0);
    }
}
