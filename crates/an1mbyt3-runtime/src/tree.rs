//! The live half of [`an1mbyt3_core::Tree2D`]: grows a tree skeleton from a
//! seed, poses it in the wind, and builds its meshes.
//!
//! **Skeleton** ([`grow_skeleton`]): a list of branches, each hung off a
//! parent at some fraction along it, with an angle relative to the parent,
//! a length, a base / tip width and a *birth* time (0 = the trunk, 1 = the
//! last twigs) so the tree can grow in branch by branch. Each species has
//! its own rules — an oak forks wide, a pine stacks short drooping tiers
//! up a straight trunk, a palm curves one trunk into a crown of fronds, a
//! willow lets long strands hang. Leaves (or blossoms, or needles) are
//! clumps attached to branches.
//!
//! **Pose** ([`build_tree_mesh`]): every frame each branch bends a little
//! with the wind — more the higher and thinner it is, in a slow sway plus a
//! faster flutter, plus any shake still ringing — and the growth fraction
//! scales branches in by birth order. Everything is built in the node's
//! local space (the trunk's foot at the origin, growing up +y).
//!
//! **Look**: flat toon colours. Branches are tapered quads with a darker
//! shadow strip down one side; leaf clumps are a dark disc with a lighter,
//! smaller disc offset up-left on top — a cheap, readable volume.

use an1mbyt3_core::{Tree2D, TreeSpecies};
use glam::Vec2;

/// One branch of the skeleton (see the module docs).
#[derive(Debug, Clone, Copy)]
pub struct Branch {
    pub parent: Option<usize>,
    /// Where along the parent it starts (0 = base, 1 = tip).
    pub at: f32,
    /// Angle relative to the parent's direction (radians, + = left).
    pub angle: f32,
    pub length: f32,
    pub width: f32,
    pub tip_width: f32,
    pub depth: u32,
    /// When it sprouts while growing in (0..1).
    pub birth: f32,
    /// How much it bends per unit length (radians; palms curve, willows droop).
    pub curve: f32,
}

/// A clump of foliage on a branch.
#[derive(Debug, Clone, Copy)]
pub struct Clump {
    pub branch: usize,
    pub at: f32,
    /// Offset from that point (in the branch's frame) and radius.
    pub offset: Vec2,
    pub radius: f32,
    /// 0..1: picks among the foliage colours.
    pub tone: f32,
    pub birth: f32,
    /// Hangs straight down from its point (willow strands).
    pub strand: f32,
    pub shape: ClumpShape,
}

/// How a clump is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClumpShape {
    /// A round leafy clump (dark disc + lighter disc up-left).
    Disc,
    /// A pine tier: a skirt of needles — a triangle `width` wide and
    /// `height` tall standing on its point, apex up.
    Tier { width: f32, height: f32 },
    /// A palm leaflet: a long narrow leaf angled `angle` off its frond.
    Leaflet { angle: f32 },
}

#[derive(Debug, Clone, Default)]
pub struct TreeSkeleton {
    pub branches: Vec<Branch>,
    pub clumps: Vec<Clump>,
}

/// A small deterministic random sequence (the tree's seed).
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / 16_777_216.0
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
}

/// Grows the branch / foliage structure for `config` (deterministic for a
/// seed).
pub fn grow_skeleton(config: &Tree2D) -> TreeSkeleton {
    let mut rng = Rng(config.seed.wrapping_mul(2_654_435_761).max(1));
    let mut t = TreeSkeleton::default();
    let h = config.height.max(10.0);
    let spread = config.spread.clamp(0.0, 2.5);
    let leafy = config.density.clamp(0.0, 3.0);
    match config.species {
        TreeSpecies::Oak | TreeSpecies::Sakura | TreeSpecies::Birch => {
            let (depth_max, fork, ratio, wiggle, width) = match config.species {
                TreeSpecies::Oak => (5u32, 0.55f32, 0.72f32, 0.18f32, h * 0.16),
                TreeSpecies::Sakura => (5, 0.65, 0.7, 0.35, h * 0.12),
                _ => (4, 0.35, 0.76, 0.1, h * 0.07),
            };
            t.branches.push(Branch { parent: None, at: 0.0, angle: rng.range(-0.06, 0.06), length: h * 0.55, width, tip_width: width * 0.62, depth: 0, birth: 0.0, curve: rng.range(-0.15, 0.15) * wiggle });
            let mut frontier = vec![0usize];
            while let Some(b) = frontier.pop() {
                let parent = t.branches[b];
                if parent.depth >= depth_max {
                    continue;
                }
                let children = if parent.depth == 0 { 3 } else { 2 + (rng.next() < 0.35) as u32 };
                for k in 0..children {
                    let side = if children == 1 { 0.0 } else { k as f32 / (children - 1) as f32 * 2.0 - 1.0 };
                    let angle = side * fork * spread + rng.range(-wiggle, wiggle);
                    let length = parent.length * ratio * rng.range(0.8, 1.1);
                    let width = parent.tip_width * rng.range(0.75, 0.9);
                    let at = if parent.depth == 0 { rng.range(0.75, 1.0) } else { rng.range(0.6, 1.0) };
                    let depth = parent.depth + 1;
                    t.branches.push(Branch { parent: Some(b), at, angle, length, width, tip_width: width * 0.6, depth, birth: depth as f32 / (depth_max + 1) as f32, curve: rng.range(-wiggle, wiggle) });
                    frontier.push(t.branches.len() - 1);
                }
            }
            // Foliage on the outer branches.
            let (size, per) = match config.species {
                TreeSpecies::Oak => (h * 0.1, 2.0),
                TreeSpecies::Sakura => (h * 0.09, 2.5),
                _ => (h * 0.05, 2.5),
            };
            for (i, br) in t.branches.clone().iter().enumerate() {
                if br.depth + 1 < depth_max {
                    continue;
                }
                let n = ((per * leafy) * if br.depth == depth_max { 1.0 } else { 0.5 }).round() as usize;
                for _ in 0..n {
                    t.clumps.push(Clump {
                        branch: i,
                        at: rng.range(0.4, 1.0),
                        offset: Vec2::new(rng.range(-0.4, 0.4), rng.range(-0.2, 0.4)) * size,
                        radius: size * rng.range(0.7, 1.1),
                        tone: rng.next(),
                        birth: br.birth + 0.15,
                        strand: 0.0,
                        shape: ClumpShape::Disc,
                    });
                }
            }
        }
        TreeSpecies::Pine => {
            let width = h * 0.07;
            t.branches.push(Branch { parent: None, at: 0.0, angle: rng.range(-0.03, 0.03), length: h, width, tip_width: width * 0.2, depth: 0, birth: 0.0, curve: 0.0 });
            // Overlapping skirts of needles, widest at the bottom: a cone.
            let tiers = (4.0 + 2.0 * leafy.min(1.5)).round() as usize;
            for k in 0..tiers {
                let f = k as f32 / tiers as f32;
                let at = 0.18 + 0.78 * f;
                let w = h * 0.62 * (1.0 - f * 0.82) * spread.clamp(0.4, 1.8);
                let tier_h = h * 0.3 * (1.0 - f * 0.45);
                t.clumps.push(Clump { branch: 0, at, offset: Vec2::new(rng.range(-0.02, 0.02) * h, -tier_h * 0.35), radius: w * 0.5, tone: rng.next(), birth: at * 0.9, strand: 0.0, shape: ClumpShape::Tier { width: w, height: tier_h } });
            }
        }
        TreeSpecies::Palm => {
            let width = h * 0.07;
            let lean = rng.range(-0.25, 0.25);
            t.branches.push(Branch { parent: None, at: 0.0, angle: lean, length: h, width, tip_width: width * 0.7, depth: 0, birth: 0.0, curve: -lean * 0.9 + rng.range(-0.15, 0.15) });
            // Fronds: arcs fanning out of the crown, drooping at the ends.
            let fronds = (7.0 * leafy.clamp(0.4, 1.6)).round() as usize;
            for k in 0..fronds {
                let side = k as f32 / (fronds - 1).max(1) as f32 * 2.0 - 1.0;
                let angle = side * 1.9 * spread.clamp(0.5, 1.3) + rng.range(-0.1, 0.1);
                t.branches.push(Branch { parent: Some(0), at: 1.0, angle, length: h * rng.range(0.42, 0.55), width: width * 0.3, tip_width: 1.0, depth: 1, birth: 0.8, curve: -side.signum() * 0.9 - side * 0.3 });
                let b = t.branches.len() - 1;
                // Leaflets in pairs along the frond, angled forward, shorter
                // toward the tip.
                for j in 0..9 {
                    let at = 0.12 + j as f32 * 0.1;
                    for s in [-1.0f32, 1.0] {
                        t.clumps.push(Clump { branch: b, at, offset: Vec2::ZERO, radius: h * 0.075 * (1.0 - at * 0.55), tone: rng.next(), birth: 0.85, strand: 0.0, shape: ClumpShape::Leaflet { angle: s * 0.85 } });
                    }
                }
            }
            // Coconuts.
            for k in 0..3 {
                t.clumps.push(Clump { branch: 0, at: 0.97, offset: Vec2::new((k as f32 - 1.0) * width * 0.6, -width * 0.4), radius: width * 0.45, tone: 2.0, birth: 0.95, strand: 0.0, shape: ClumpShape::Disc });
            }
        }
        TreeSpecies::Willow => {
            let width = h * 0.14;
            t.branches.push(Branch { parent: None, at: 0.0, angle: rng.range(-0.08, 0.08), length: h * 0.6, width, tip_width: width * 0.6, depth: 0, birth: 0.0, curve: rng.range(-0.2, 0.2) });
            // A few arching limbs, then long hanging strands of leaves.
            let limbs = 5;
            for k in 0..limbs {
                let side = k as f32 / (limbs - 1) as f32 * 2.0 - 1.0;
                t.branches.push(Branch { parent: Some(0), at: rng.range(0.85, 1.0), angle: side * 1.05 * spread + rng.range(-0.1, 0.1), length: h * rng.range(0.42, 0.55), width: width * 0.4, tip_width: width * 0.12, depth: 1, birth: 0.35, curve: side * 1.7 });
                let b = t.branches.len() - 1;
                let strands = (5.0 * leafy).round() as usize;
                for j in 0..strands {
                    t.clumps.push(Clump { branch: b, at: 0.3 + 0.7 * j as f32 / strands.max(1) as f32, offset: Vec2::ZERO, radius: h * 0.03, tone: rng.next(), birth: 0.5 + j as f32 * 0.04, strand: h * rng.range(0.45, 0.8), shape: ClumpShape::Disc });
                }
            }
        }
    }
    t
}

/// A posed point along a branch: position and direction angle.
#[derive(Debug, Clone, Copy)]
struct Frame {
    base: Vec2,
    angle: f32,
    length: f32,
}

/// Where branch `b` starts and points this frame (`frames` holds every
/// earlier — i.e. parent — branch already).
fn point_on(frame: &Frame, curve: f32, at: f32) -> (Vec2, f32) {
    // A gently curved branch: the angle drifts by `curve` along its length.
    let steps = 4;
    let mut p = frame.base;
    let mut a = frame.angle;
    let seg = frame.length * at / steps as f32;
    for _ in 0..steps {
        p += Vec2::new(-a.sin(), a.cos()) * seg;
        a += curve * at / steps as f32;
    }
    (p, a)
}

/// Meshes for one frame, each a flat colour (see [`TreeMesh::parts`]).
#[derive(Debug, Clone, Default)]
pub struct TreeMesh {
    /// `(colour, positions, indices)` back to front: bark, bark shadow,
    /// foliage dark, foliage light, accents (blossom / fruit), falling leaves.
    pub parts: Vec<([f32; 4], Vec<[f32; 2]>, Vec<u32>)>,
}

fn quad(out: &mut (Vec<[f32; 2]>, Vec<u32>), a: Vec2, b: Vec2, wa: f32, wb: f32, shift: f32) {
    let dir = (b - a).normalize_or_zero();
    let n = Vec2::new(-dir.y, dir.x);
    let base = out.0.len() as u32;
    // `shift` slides the quad sideways (the shadow strip sits on one side).
    for p in [a - n * wa * 0.5 + n * shift * wa, a + n * wa * 0.5 + n * shift * wa, b + n * wb * 0.5 + n * shift * wb, b - n * wb * 0.5 + n * shift * wb] {
        out.0.push([p.x, p.y]);
    }
    out.1.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn triangle(out: &mut (Vec<[f32; 2]>, Vec<u32>), a: Vec2, b: Vec2, c: Vec2) {
    let base = out.0.len() as u32;
    out.0.extend([[a.x, a.y], [b.x, b.y], [c.x, c.y]]);
    out.1.extend([base, base + 1, base + 2]);
}

/// A quad from four corners in order (a leaf: base, one side, tip, other side).
fn quad_points(out: &mut (Vec<[f32; 2]>, Vec<u32>), a: Vec2, b: Vec2, c: Vec2, d: Vec2) {
    let base = out.0.len() as u32;
    out.0.extend([[a.x, a.y], [b.x, b.y], [c.x, c.y], [d.x, d.y]]);
    out.1.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn disc(out: &mut (Vec<[f32; 2]>, Vec<u32>), c: Vec2, r: f32, sides: u32) {
    let base = out.0.len() as u32;
    out.0.push([c.x, c.y]);
    // One sin/cos per disc, then step a unit vector around by rotation:
    // trees rebuild every frame in Playground and wasm's sin/cos are slow.
    let (step_sin, step_cos) = (std::f32::consts::TAU / sides.max(1) as f32).sin_cos();
    let mut dir = Vec2::X;
    for _ in 0..sides {
        out.0.push([c.x + dir.x * r, c.y + dir.y * r]);
        dir = Vec2::new(dir.x * step_cos - dir.y * step_sin, dir.x * step_sin + dir.y * step_cos);
    }
    for k in 0..sides {
        out.1.extend([base, base + 1 + k, base + 1 + (k + 1) % sides]);
    }
}

/// A leaf falling off the tree (local space).
#[derive(Debug, Clone, Copy)]
pub struct FallingLeaf {
    pub pos: Vec2,
    pub vel: Vec2,
    pub age: f32,
    pub spin: f32,
    pub tone: f32,
}

/// Running state for one tree: its skeleton (regrown when the authored
/// shape changes), clock, any shake still ringing, and falling leaves.
#[derive(Debug, Clone, Default)]
pub struct TreeRuntime {
    pub skeleton: TreeSkeleton,
    pub key: Option<(u32, TreeSpecies, [u32; 3])>,
    pub time: f32,
    pub shake: f32,
    pub leaves: Vec<FallingLeaf>,
    /// Where each foliage clump was drawn last frame (leaves drop from there).
    pub last_clumps: Vec<Vec2>,
    spawn_acc: f32,
    rng: u32,
}

impl TreeRuntime {
    /// Regrows the skeleton if the shape settings changed.
    pub fn sync(&mut self, config: &Tree2D) {
        let key = (config.seed, config.species, [config.height.to_bits(), config.spread.to_bits(), config.density.to_bits()]);
        if self.key != Some(key) {
            self.skeleton = grow_skeleton(config);
            self.key = Some(key);
            self.rng = config.seed | 1;
        }
    }

    /// A shake (a click, a hit): it sways hard for a moment and drops a
    /// handful of leaves.
    pub fn shake(&mut self, strength: f32) {
        self.shake = (self.shake + strength).min(3.0);
        let n = (strength * 12.0) as usize;
        self.drop_leaves(n);
    }

    fn rand(&mut self) -> f32 {
        let mut r = Rng(self.rng.max(1));
        let v = r.next();
        self.rng = r.0;
        v
    }

    fn drop_leaves(&mut self, n: usize) {
        if self.skeleton.clumps.is_empty() {
            return;
        }
        for _ in 0..n {
            let i = (self.rand() * self.skeleton.clumps.len() as f32) as usize % self.skeleton.clumps.len();
            let tone = self.skeleton.clumps[i].tone;
            if tone > 1.5 {
                continue; // fruit stays put
            }
            // From where the clump was last drawn.
            let at = self.last_clumps.get(i).copied().unwrap_or(Vec2::ZERO);
            let jitter = Vec2::new(self.rand() - 0.5, self.rand() - 0.5) * 20.0;
            let vel = Vec2::new((self.rand() - 0.5) * 40.0, self.rand() * 20.0);
            let spin = self.rand() * 6.0;
            self.leaves.push(FallingLeaf { pos: at + jitter, vel, age: 0.0, spin, tone });
        }
        if self.leaves.len() > 200 {
            let extra = self.leaves.len() - 200;
            self.leaves.drain(..extra);
        }
    }

    /// Advances the clock, the shake and the falling leaves.
    pub fn step(&mut self, config: &Tree2D, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        self.time += dt;
        self.shake = (self.shake - dt * 1.6).max(0.0);
        self.spawn_acc += config.falling_leaves.max(0.0) * dt;
        while self.spawn_acc >= 1.0 {
            self.spawn_acc -= 1.0;
            self.drop_leaves(1);
        }
        let wind = config.wind;
        for leaf in &mut self.leaves {
            leaf.age += dt;
            // Flutter down: slow fall, side-to-side drift, carried by wind.
            leaf.vel.y = (leaf.vel.y - 140.0 * dt).max(-55.0);
            leaf.vel.x += ((leaf.age * 3.0 + leaf.spin).sin() * 90.0 + wind * 60.0 - leaf.vel.x) * dt * 2.0;
            leaf.pos += leaf.vel * dt;
        }
        let floor = -config.height * 0.02;
        self.leaves.retain(|l| l.age < 6.0 && l.pos.y > floor - 4.0);
    }
}

/// Builds the posed tree. `grow` (0..1) scales branches in by birth order.
pub fn build_tree_mesh(config: &Tree2D, rt: &mut TreeRuntime, grow: f32) -> TreeMesh {
    let t = rt.time;
    let sway_amount = config.wind_sway.max(0.0) + rt.shake * 1.6;
    let wind_lean = config.wind * 0.12;
    let sk = &rt.skeleton;
    let mut frames: Vec<Frame> = Vec::with_capacity(sk.branches.len());
    let mut bark = (Vec::new(), Vec::new());
    let mut shadow = (Vec::new(), Vec::new());
    let grown = |birth: f32| ((grow - birth * 0.85) / 0.15).clamp(0.0, 1.0);
    for (i, br) in sk.branches.iter().enumerate() {
        let g = grown(br.birth);
        let depth = br.depth as f32;
        // Sway: slow and broad for the trunk, faster and livelier up top.
        let sway = sway_amount * (0.03 + depth * 0.035) * ((t * (1.1 + depth * 0.35) + i as f32 * 0.7).sin() * 0.7 + (t * (3.3 + depth) + i as f32 * 1.3).sin() * 0.3) - wind_lean * (0.2 + depth * 0.25);
        let (base, parent_angle) = match br.parent {
            None => (Vec2::ZERO, 0.0),
            Some(p) => point_on(&frames[p], sk.branches[p].curve, br.at),
        };
        let frame = Frame { base, angle: parent_angle + br.angle + sway, length: br.length * g };
        frames.push(frame);
        if g <= 0.0 {
            continue;
        }
        // Tapered, gently curved: four segments.
        let steps = 4;
        let mut a = base;
        for s in 0..steps {
            let f0 = s as f32 / steps as f32;
            let f1 = (s + 1) as f32 / steps as f32;
            let (p1, _) = point_on(&frame, br.curve, f1);
            let w0 = (br.width + (br.tip_width - br.width) * f0) * g.max(0.3);
            let w1 = (br.width + (br.tip_width - br.width) * f1) * g.max(0.3);
            quad(&mut bark, a, p1, w0, w1, 0.0);
            quad(&mut shadow, a, p1, w0 * 0.4, w1 * 0.4, -0.75);
            a = p1;
        }
    }
    // Foliage: each clump a dark disc with a lighter one up-left on top.
    let mut dark = (Vec::new(), Vec::new());
    let mut light = (Vec::new(), Vec::new());
    let mut accent = (Vec::new(), Vec::new());
    let mut positions = Vec::with_capacity(sk.clumps.len());
    let sides = 18;
    for (k, c) in sk.clumps.iter().enumerate() {
        let frame = frames[c.branch];
        let (p, a) = point_on(&frame, sk.branches[c.branch].curve, c.at);
        let rot = Vec2::new(a.cos(), a.sin());
        let mut center = p + Vec2::new(c.offset.x * rot.x - c.offset.y * rot.y, c.offset.x * rot.y + c.offset.y * rot.x);
        let g = grown(c.birth);
        positions.push(center);
        if g <= 0.0 {
            continue;
        }
        let r = c.radius * g;
        if c.strand > 0.0 {
            // A hanging strand: a column of small leaves, swaying at the end.
            let n = 6;
            for j in 0..n {
                let f = j as f32 / (n - 1) as f32;
                let swing = (t * 1.7 + k as f32 * 0.5).sin() * sway_amount * 6.0 * f + config.wind * 18.0 * f;
                let at = center + Vec2::new(swing, -c.strand * f * g);
                disc(if j % 2 == 0 { &mut dark } else { &mut light }, at, r * (1.0 - f * 0.4), 6);
            }
            continue;
        }
        if c.tone > 1.5 {
            disc(&mut accent, center, r, sides);
            continue;
        }
        match c.shape {
            ClumpShape::Tier { width, height } => {
                // A needle skirt: dark triangle, a lighter one inset up-left,
                // its hem nudged by the sway.
                let (w, th) = (width * g, height * g);
                let lean = (t * 1.3 + k as f32).sin() * sway_amount * 2.0;
                triangle(&mut dark, center + Vec2::new(lean * 0.5, th), center + Vec2::new(-w * 0.5 + lean, 0.0), center + Vec2::new(w * 0.5 + lean, 0.0));
                triangle(&mut light, center + Vec2::new(lean * 0.5 - w * 0.04, th * 0.92), center + Vec2::new(-w * 0.4 + lean, th * 0.2), center + Vec2::new(w * 0.05 + lean, th * 0.2));
                continue;
            }
            ClumpShape::Leaflet { angle } => {
                // A long narrow leaf off the frond, drooping a little at the
                // tip, fluttering.
                let flutter = (t * 4.0 + k as f32 * 0.9).sin() * 0.08 * sway_amount;
                let dir = a + angle + flutter;
                let along = Vec2::new(-dir.sin(), dir.cos());
                let side_n = Vec2::new(-along.y, along.x);
                let tip = center + along * r * 2.2 + Vec2::new(0.0, -r * 0.4);
                quad_points(if k % 2 == 0 { &mut dark } else { &mut light }, center + side_n * r * 0.08, center + along * r * 1.1 + side_n * r * 0.22, tip, center + along * r * 1.1 - side_n * r * 0.22);
                continue;
            }
            ClumpShape::Disc => {}
        }
        // A little rustle on the clump itself.
        center += Vec2::new((t * 2.6 + k as f32).sin(), (t * 3.1 + k as f32 * 1.7).cos()) * sway_amount * 1.2;
        let target = if c.tone > 0.78 && config.species == TreeSpecies::Sakura { &mut accent } else { &mut dark };
        disc(target, center, r, sides);
        if !(c.tone > 0.78 && config.species == TreeSpecies::Sakura) {
            disc(&mut light, center + Vec2::new(-r * 0.22, r * 0.25), r * 0.68, sides);
        }
    }
    rt.last_clumps = positions;
    // Falling leaves: little diamonds.
    let mut falling = (Vec::new(), Vec::new());
    for leaf in &rt.leaves {
        let s = 5.0 * (1.0 - (leaf.age / 6.0).powi(3)).max(0.2);
        let a = leaf.age * 4.0 + leaf.spin;
        let (sa, ca) = a.sin_cos();
        let pts = [Vec2::new(0.0, s), Vec2::new(s * 0.6, 0.0), Vec2::new(0.0, -s), Vec2::new(-s * 0.6, 0.0)].map(|p| leaf.pos + Vec2::new(p.x * ca - p.y * sa, p.x * sa + p.y * ca));
        let base = falling.0.len() as u32;
        falling.0.extend(pts.map(|p| [p.x, p.y]));
        falling.1.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let shade = |c: [f32; 4], k: f32| [c[0] * k, c[1] * k, c[2] * k, c[3]];
    let leaf_dark = shade(config.leaf_color, 0.72);
    let fruit = if config.species == TreeSpecies::Palm { [0.45, 0.3, 0.15, 1.0] } else { config.accent_color };
    TreeMesh {
        parts: vec![
            (config.bark_color, bark.0, bark.1),
            (shade(config.bark_color, 0.7), shadow.0, shadow.1),
            (leaf_dark, dark.0, dark.1),
            (config.leaf_color, light.0, light.1),
            (fruit, accent.0, accent.1),
            (if config.species == TreeSpecies::Sakura { config.accent_color } else { config.leaf_color }, falling.0, falling.1),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [TreeSpecies; 6] = [TreeSpecies::Oak, TreeSpecies::Pine, TreeSpecies::Birch, TreeSpecies::Sakura, TreeSpecies::Palm, TreeSpecies::Willow];

    fn triangles(mesh: &TreeMesh) -> usize {
        mesh.parts.iter().map(|(_, _, i)| i.len() / 3).sum()
    }

    #[test]
    fn every_species_grows_a_deterministic_tree_with_foliage() {
        for species in ALL {
            let config = Tree2D::of(species, 11, 160.0);
            let a = grow_skeleton(&config);
            let b = grow_skeleton(&config);
            assert!(!a.branches.is_empty() && !a.clumps.is_empty(), "{species:?}");
            assert_eq!(a.branches.len(), b.branches.len());
            assert_eq!(a.clumps[0].radius, b.clumps[0].radius);
            // A different seed: a different tree.
            let c = grow_skeleton(&Tree2D { seed: 12, ..config });
            assert!(c.branches.iter().zip(&a.branches).any(|(x, y)| x.length != y.length || x.angle != y.angle) || a.clumps.iter().zip(&c.clumps).any(|(x, y)| x.radius != y.radius || x.offset != y.offset), "{species:?}");
        }
    }

    #[test]
    fn growing_in_adds_geometry_and_the_tree_stands_on_its_origin() {
        let config = Tree2D::of(TreeSpecies::Oak, 3, 160.0);
        let mut rt = TreeRuntime::default();
        rt.sync(&config);
        let sapling = triangles(&build_tree_mesh(&config, &mut rt, 0.05));
        let full = build_tree_mesh(&config, &mut rt, 1.0);
        assert!(triangles(&full) > sapling);
        // The trunk's foot is at the origin and the crown goes up.
        let ys: Vec<f32> = full.parts.iter().flat_map(|(_, p, _)| p.iter().map(|q| q[1])).collect();
        let (lo, hi) = (ys.iter().cloned().fold(f32::MAX, f32::min), ys.iter().cloned().fold(f32::MIN, f32::max));
        assert!(lo > -40.0 && hi > 100.0, "{lo}..{hi}");
    }

    #[test]
    fn shaking_drops_leaves_that_fall() {
        let config = Tree2D { falling_leaves: 0.0, ..Tree2D::of(TreeSpecies::Oak, 3, 160.0) };
        let mut rt = TreeRuntime::default();
        rt.sync(&config);
        build_tree_mesh(&config, &mut rt, 1.0);
        rt.shake(1.0);
        assert!(!rt.leaves.is_empty());
        let y0 = rt.leaves[0].pos.y;
        for _ in 0..30 {
            rt.step(&config, 1.0 / 30.0);
        }
        assert!(rt.leaves.is_empty() || rt.leaves[0].pos.y < y0);
    }
}
