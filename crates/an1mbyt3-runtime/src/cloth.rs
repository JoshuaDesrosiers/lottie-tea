//! The live half of [`an1mbyt3_core::Cloth2D`]: a Verlet cloth.
//!
//! **Model.** A `columns × rows` grid of points in *world* space. Each point
//! remembers where it was last step, so its velocity is implicit
//! (`pos - prev`) — Verlet integration, which is what makes cloth stable
//! without tiny time steps. Neighbours are joined by *links* that want to
//! stay their rest length:
//! - **structural** links (right and down neighbours) are the threads; the
//!   mesh's triangles exist only while their edges do, so cutting threads
//!   opens holes;
//! - **shear** links (both diagonals of every square) are deliberately soft:
//!   they keep squares from collapsing into slivers but let the weave skew,
//!   which is what lets a curtain lean and swing. Cutting a thread also cuts the shear links of
//!   the squares on either side of it, so the fabric really parts.
//!
//! Each step: gravity, gusty wind and damping move the free points, pinned
//! points snap to where the node now holds them (they follow the node),
//! then a few relaxation passes pull every link back toward its length
//! (more passes = stiffer) — firmly when stretched, only gently when
//! squashed, since real fabric buckles rather than pushing back (in 2D:
//! bunches into folds). Links stretched past `tear_stretch` × rest snap.
//!
//! **Shading.** [`build_mesh`] also sorts triangles into shade buckets by
//! how squashed they are relative to rest — soft folds a little darker,
//! creases more, a triangle flipped over (the back of a fold) darkest,
//! stretched cloth slightly lighter — drawn as translucent overlays on the
//! fabric. In 2D, bunched-up fabric is what a ripple *looks* like, so the
//! travelling gust waves read as light and shadow rolling along a flag.

use an1mbyt3_core::Cloth2D;
use glam::Vec2;

/// How hard links push back against being squashed (vs 1.0 for being
/// stretched) — a little, so folds don't collapse into a tangle.
const COMPRESSION: f32 = 0.12;

#[derive(Debug, Clone, Copy)]
struct Link {
    a: usize,
    b: usize,
    rest: f32,
    alive: bool,
    /// A diagonal (shear) link: always soft — woven fabric skews easily.
    shear: bool,
}

/// How firmly shear links hold their length (vs 1.0 for threads).
const SHEAR: f32 = 0.02;

/// A running cloth (see the module docs).
#[derive(Debug, Clone)]
pub struct ClothSim {
    cols: usize,
    rows: usize,
    pos: Vec<Vec2>,
    prev: Vec<Vec2>,
    pinned: Vec<bool>,
    links: Vec<Link>,
    /// Structural link indices: `h[r * (cols-1) + c]` joins (c, r)–(c+1, r);
    /// `v[r * cols + c]` joins (c, r)–(c, r+1).
    h: Vec<usize>,
    v: Vec<usize>,
    /// Both shear links of square (c, r): `shear[r * (cols-1) + c]`.
    shear: Vec<[usize; 2]>,
    time: f32,
    grab: Option<(usize, Vec2)>,
    /// A script's wind, replacing the authored one (`ref.setClothWind`).
    pub wind_override: Option<Vec2>,
}

/// What [`build_mesh`] produces: the fabric's triangles plus the shade
/// overlays (indices into the same vertices).
#[derive(Debug, Clone, Default)]
pub struct ClothMesh {
    pub positions: Vec<[f32; 2]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    /// Shade overlays: soft folds (a little squashed), creases (squashed
    /// hard), backs (flipped over), highlights (stretched).
    pub folds: Vec<u32>,
    pub creases: Vec<u32>,
    pub backs: Vec<u32>,
    pub highlights: Vec<u32>,
}

impl ClothSim {
    /// A cloth hanging at rest where `to_world` (the node's local → world
    /// mapping) puts it.
    pub fn new(config: &Cloth2D, to_world: &dyn Fn(Vec2) -> Vec2) -> Self {
        let (cols, rows) = config.grid();
        let mut pos = Vec::with_capacity(cols * rows);
        let mut pinned = Vec::with_capacity(cols * rows);
        for r in 0..rows {
            for c in 0..cols {
                pos.push(to_world(config.rest_point(c, r)));
                pinned.push(config.is_pinned(c, r));
            }
        }
        let mut sim = Self { cols, rows, prev: pos.clone(), pos, pinned, links: Vec::new(), h: Vec::new(), v: Vec::new(), shear: Vec::new(), time: 0.0, grab: None, wind_override: None };
        let idx = |c: usize, r: usize| r * cols + c;
        for r in 0..rows {
            for c in 0..cols - 1 {
                let i = sim.link(idx(c, r), idx(c + 1, r), false);
                sim.h.push(i);
            }
        }
        for r in 0..rows - 1 {
            for c in 0..cols {
                let i = sim.link(idx(c, r), idx(c, r + 1), false);
                sim.v.push(i);
            }
        }
        for r in 0..rows - 1 {
            for c in 0..cols - 1 {
                let a = sim.link(idx(c, r), idx(c + 1, r + 1), true);
                let b = sim.link(idx(c + 1, r), idx(c, r + 1), true);
                sim.shear.push([a, b]);
            }
        }
        sim
    }

    fn link(&mut self, a: usize, b: usize, shear: bool) -> usize {
        let rest = self.pos[a].distance(self.pos[b]).max(1e-3);
        self.links.push(Link { a, b, rest, alive: true, shear });
        self.links.len() - 1
    }

    /// Whether this simulation was built for `config`'s grid (a resized
    /// grid needs a fresh one).
    pub fn matches(&self, config: &Cloth2D) -> bool {
        config.grid() == (self.cols, self.rows)
    }

    pub fn points(&self) -> &[Vec2] {
        &self.pos
    }

    /// Advances `dt` seconds. `to_world` is the node's current local →
    /// world mapping (pinned points follow it).
    pub fn step(&mut self, config: &Cloth2D, to_world: &dyn Fn(Vec2) -> Vec2, dt: f32) {
        let dt = dt.clamp(0.0, 1.0 / 20.0);
        if dt <= 0.0 {
            return;
        }
        let substeps = (dt * 120.0).ceil().clamp(1.0, 4.0) as usize;
        let h = dt / substeps as f32;
        let wind = self.wind_override.unwrap_or(config.wind);
        let gust_amount = config.turbulence.clamp(0.0, 2.0);
        let keep = (1.0 - config.damping.max(0.0) * h).clamp(0.0, 1.0);
        let iterations = config.stiffness.clamp(1, 30);
        for _ in 0..substeps {
            self.time += h;
            let t = self.time;
            for i in 0..self.pos.len() {
                let (c, r) = (i % self.cols, i / self.cols);
                if self.pinned[i] {
                    let target = to_world(config.rest_point(c, r));
                    self.prev[i] = self.pos[i];
                    self.pos[i] = target;
                    continue;
                }
                if let Some((g, target)) = self.grab {
                    if g == i {
                        self.prev[i] = self.pos[i];
                        self.pos[i] = target;
                        continue;
                    }
                }
                let p = self.pos[i];
                // Gusts: a slow swell over the whole cloth, plus waves that
                // travel along it (down-wind through the columns) — they
                // bunch the fabric up and spread it out in moving bands, and
                // a flutter across the wind ripples its edges.
                let (cf, rf) = (c as f32, r as f32);
                let wave = (0.85 * cf - t * 7.0 + rf * 0.12).sin();
                let gust = 1.0 + gust_amount * (0.45 * (t * 2.3 + p.y * 0.013 + p.x * 0.004).sin() + 0.55 * wave);
                let across = Vec2::new(-wind.y, wind.x) * (gust_amount * 0.45 * (0.7 * cf - t * 6.0 + rf * 0.1).sin());
                let accel = Vec2::new(0.0, -config.gravity) + wind * gust + across;
                let velocity = (p - self.prev[i]) * keep;
                self.prev[i] = p;
                self.pos[i] = p + velocity + accel * h * h;
            }
            for pass in 0..iterations {
                let last = pass + 1 == iterations;
                for k in 0..self.links.len() {
                    let link = self.links[k];
                    if !link.alive {
                        continue;
                    }
                    let wa = self.inverse_mass(link.a);
                    let wb = self.inverse_mass(link.b);
                    if wa + wb == 0.0 {
                        continue;
                    }
                    let delta = self.pos[link.b] - self.pos[link.a];
                    let len = delta.length().max(1e-5);
                    if last && config.tear_stretch > 1.0 && len > link.rest * config.tear_stretch {
                        self.cut(k);
                        continue;
                    }
                    // Fabric resists stretching fully but barely resists
                    // being squashed: real cloth buckles out of plane when
                    // pushed together, which in 2D means it bunches into
                    // folds. Resisting both ways would make a sheet pinned
                    // along an edge a rigid lattice that can't sway at all.
                    let give = if link.shear { SHEAR } else if len > link.rest { 1.0 } else { COMPRESSION };
                    let correction = delta * ((len - link.rest) / len / (wa + wb) * give);
                    self.pos[link.a] += correction * wa;
                    self.pos[link.b] -= correction * wb;
                }
            }
        }
    }

    fn inverse_mass(&self, i: usize) -> f32 {
        if self.pinned[i] || self.grab.is_some_and(|(g, _)| g == i) {
            0.0
        } else {
            1.0
        }
    }

    /// Cuts link `k`; a thread also takes the shear links of the squares on
    /// both sides with it, so the cloth parts there.
    fn cut(&mut self, k: usize) {
        self.links[k].alive = false;
        let (cols, rows) = (self.cols, self.rows);
        let quads: Vec<(usize, usize)> = if let Some(pos) = self.h.iter().position(|&i| i == k) {
            let (c, r) = (pos % (cols - 1), pos / (cols - 1));
            [r.checked_sub(1).map(|r0| (c, r0)), (r < rows - 1).then_some((c, r))].into_iter().flatten().collect()
        } else if let Some(pos) = self.v.iter().position(|&i| i == k) {
            let (c, r) = (pos % cols, pos / cols);
            [c.checked_sub(1).map(|c0| (c0, r)), (c < cols - 1).then_some((c, r))].into_iter().flatten().collect()
        } else {
            Vec::new()
        };
        for (c, r) in quads {
            for s in self.shear[r * (cols - 1) + c] {
                self.links[s].alive = false;
            }
        }
    }

    /// Cuts every thread whose middle is within `radius` of `at` (world).
    /// Returns how many were cut.
    pub fn tear(&mut self, at: Vec2, radius: f32) -> usize {
        let mut cut = 0;
        for k in self.h.iter().chain(self.v.iter()).copied().collect::<Vec<_>>() {
            let link = self.links[k];
            if link.alive && ((self.pos[link.a] + self.pos[link.b]) * 0.5).distance(at) <= radius {
                self.cut(k);
                cut += 1;
            }
        }
        cut
    }

    /// Takes hold of the free point nearest `at` within `radius`; `true` if
    /// one was grabbed.
    pub fn grab(&mut self, at: Vec2, radius: f32) -> bool {
        let nearest = (0..self.pos.len()).filter(|&i| !self.pinned[i]).map(|i| (i, self.pos[i].distance(at))).filter(|&(_, d)| d <= radius).min_by(|a, b| a.1.total_cmp(&b.1));
        self.grab = nearest.map(|(i, _)| (i, at));
        self.grab.is_some()
    }

    /// Moves the held point toward `to` (world).
    pub fn drag(&mut self, to: Vec2) {
        if let Some((_, target)) = &mut self.grab {
            *target = to;
        }
    }

    pub fn release(&mut self) {
        self.grab = None;
    }

    pub fn holding(&self) -> bool {
        self.grab.is_some()
    }
}

/// The fabric mesh for the cloth's current shape (world space; UVs span the
/// cloth 0..1, v = 0 at the top), and its shade overlays.
pub fn build_mesh(sim: &ClothSim) -> ClothMesh {
    let (cols, rows) = (sim.cols, sim.rows);
    let mut mesh = ClothMesh { positions: sim.pos.iter().map(|p| [p.x, p.y]).collect(), ..Default::default() };
    mesh.uvs = (0..rows).flat_map(|r| (0..cols).map(move |c| [c as f32 / (cols - 1) as f32, r as f32 / (rows - 1) as f32])).collect();
    let idx = |c: usize, r: usize| (r * cols + c) as u32;
    let alive = |k: usize| sim.links[k].alive;
    // Signed area of a triangle relative to the same triangle at rest:
    // 1 = flat, small = squashed into a fold, negative = flipped over.
    let squash = |a: u32, b: u32, c: u32, ra: f32| {
        let (pa, pb, pc) = (sim.pos[a as usize], sim.pos[b as usize], sim.pos[c as usize]);
        (pb - pa).perp_dot(pc - pa) / ra
    };
    for r in 0..rows - 1 {
        for c in 0..cols - 1 {
            let (tl, tr, bl, br) = (idx(c, r), idx(c + 1, r), idx(c, r + 1), idx(c + 1, r + 1));
            let top = alive(sim.h[r * (cols - 1) + c]);
            let bottom = alive(sim.h[(r + 1) * (cols - 1) + c]);
            let left = alive(sim.v[r * cols + c]);
            let right = alive(sim.v[r * cols + c + 1]);
            // Rest areas (signed, from the rest lengths).
            let ra = -(sim.links[sim.h[r * (cols - 1) + c]].rest * sim.links[sim.v[r * cols + c]].rest);
            for (on, tri) in [(top && left, [tl, tr, bl]), (right && bottom, [tr, br, bl])] {
                if !on {
                    continue;
                }
                mesh.indices.extend(tri);
                let s = squash(tri[0], tri[1], tri[2], ra);
                if s < 0.0 {
                    mesh.backs.extend(tri);
                } else if s < 0.62 {
                    mesh.creases.extend(tri);
                } else if s < 0.88 {
                    mesh.folds.extend(tri);
                } else if s > 1.1 {
                    mesh.highlights.extend(tri);
                }
            }
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use an1mbyt3_core::ClothPin;

    fn settle(sim: &mut ClothSim, config: &Cloth2D, seconds: f32) {
        for _ in 0..(seconds * 60.0) as usize {
            sim.step(config, &|p| p, 1.0 / 60.0);
        }
    }

    #[test]
    fn a_curtain_hangs_from_its_pins_without_stretching_much() {
        let config = Cloth2D { wind: Vec2::ZERO, turbulence: 0.0, columns: 10, rows: 8, width: 90.0, height: 70.0, ..Default::default() };
        let mut sim = ClothSim::new(&config, &|p| p);
        settle(&mut sim, &config, 3.0);
        // Pinned top row stays put; the bottom hangs about a cloth-length below.
        assert!((sim.points()[3] - config.rest_point(3, 0)).length() < 1e-3);
        let bottom = sim.points()[7 * 10 + 5].y;
        assert!(bottom < config.rest_point(5, 7).y + 5.0 && bottom > -70.0, "{bottom}");
        assert_eq!(build_mesh(&sim).indices.len(), 9 * 7 * 6);
    }

    #[test]
    fn wind_blows_a_flag_out_and_cutting_opens_a_hole() {
        let config = Cloth2D { pin: ClothPin::LeftEdge, wind: Vec2::new(900.0, 0.0), gravity: 100.0, turbulence: 0.0, columns: 8, rows: 6, ..Default::default() };
        let mut sim = ClothSim::new(&config, &|p| p);
        settle(&mut sim, &config, 2.0);
        assert!(sim.points()[7].x > config.width * 0.6, "the free end should stream out");
        let full = build_mesh(&sim).indices.len();
        let middle = (sim.points()[3 * 8 + 3] + sim.points()[3 * 8 + 4]) * 0.5;
        assert!(sim.tear(middle, 12.0) > 0);
        assert!(build_mesh(&sim).indices.len() < full);
    }

    #[test]
    fn grabbing_drags_a_point_and_over_stretching_tears() {
        let config = Cloth2D { wind: Vec2::ZERO, turbulence: 0.0, gravity: 0.0, columns: 6, rows: 6, tear_stretch: 1.6, ..Default::default() };
        let mut sim = ClothSim::new(&config, &|p| p);
        let corner = config.rest_point(5, 5);
        assert!(sim.grab(corner + Vec2::splat(2.0), 10.0));
        sim.drag(corner + Vec2::new(0.0, -600.0));
        settle(&mut sim, &config, 0.5);
        assert!(sim.links.iter().any(|l| !l.alive), "yanking far should tear something");
        sim.release();
        assert!(!sim.holding());
    }
}

#[cfg(test)]
mod sway_tests {
    use super::*;
    /// Regression: with stiff compression / shear links a sheet pinned
    /// along its top was a rigid lattice and wind couldn't move it.
    #[test]
    fn a_curtain_leans_in_a_breeze() {
        let config = Cloth2D { width: 250.0, height: 230.0, columns: 24, rows: 20, gravity: 800.0, wind: Vec2::new(180.0, 0.0), turbulence: 0.8, stiffness: 8, damping: 1.2, ..Default::default() };
        let mut sim = ClothSim::new(&config, &|p| p);
        let before = sim.points()[19 * 24 + 12];
        for _ in 0..240 {
            sim.step(&config, &|p| p, 1.0 / 60.0);
        }
        let after = sim.points()[19 * 24 + 12];
        assert!(after.x - before.x > 20.0, "{before:?} -> {after:?}");
    }
}
