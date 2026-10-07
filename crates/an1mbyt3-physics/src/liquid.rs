//! Liquid 2D — a particle fluid (see `an1mbyt3_core::Liquid2D`).
//!
//! The method is *double-density relaxation* (Clavet, Beaudoin & Poulin,
//! "Particle-based Viscoelastic Fluid Simulation", 2005): instead of
//! solving pressure forces, each step moves particles directly so local
//! density relaxes toward a rest value. A second "near" density term keeps
//! neighbours from clumping and gives the fluid surface tension, so it
//! beads, splashes and settles flat without exploding — stable at game
//! timesteps, which is why it's a game-engine favourite.
//!
//! This module is the fluid on its own, in pixel space and with no Rapier
//! types: `LiquidSim::relax` does gravity, viscosity, and the density
//! relaxation; the physics world (`lib.rs`) then resolves collisions with
//! colliders, pushes dynamic bodies and applies buoyancy, and finally calls
//! `LiquidSim::finish` to derive velocities.

use std::collections::HashMap;

use an1mbyt3_core::Liquid2D;
use glam::Vec2;

/// One fluid particle.
#[derive(Debug, Clone, Copy)]
pub struct LiquidParticle {
    pub position: Vec2,
    pub velocity: Vec2,
    previous: Vec2,
    /// Local density from the last step, relative to rest (≈1 inside the
    /// body of the fluid, lower at the surface / in spray) — the renderer
    /// shades foam with it.
    pub density: f32,
}

/// A running liquid (one per Liquid 2D node).
#[derive(Debug, Clone, Default)]
pub struct LiquidSim {
    pub particles: Vec<LiquidParticle>,
    /// Rest density (sum of kernel weights at rest spacing).
    rest_density: f32,
    emit_accumulator: f32,
    grid: HashMap<(i32, i32), Vec<u32>>,
}

/// Interaction radius, in multiples of the particle spacing.
const RADIUS_FACTOR: f32 = 1.9;
/// Pressure gains (px / s² scale) — `Liquid2D::stiffness` multiplies them.
const STIFFNESS: f32 = 2600.0;
const NEAR_STIFFNESS: f32 = 9000.0;

/// The starting volume's particle positions (world px), on a slightly
/// staggered grid — also what the editor draws when not simulating.
pub fn initial_layout(config: &Liquid2D, center: Vec2) -> Vec<Vec2> {
    let s = config.particle_spacing.max(2.0);
    let cols = ((config.size.x / s).floor() as i32).max(1);
    let rows = ((config.size.y / s).floor() as i32).max(1);
    let origin = center - Vec2::new((cols - 1) as f32 * s, (rows - 1) as f32 * s) * 0.5;
    let mut out = Vec::with_capacity((cols * rows) as usize);
    'fill: for r in 0..rows {
        for c in 0..cols {
            if out.len() >= config.max_particles as usize {
                break 'fill;
            }
            let stagger = if r % 2 == 1 { s * 0.5 } else { 0.0 };
            out.push(origin + Vec2::new(c as f32 * s + stagger.min((cols - 1) as f32 * s - c as f32 * s), r as f32 * s));
        }
    }
    out
}

impl LiquidSim {
    pub fn new(config: &Liquid2D, center: Vec2) -> Self {
        let particles = initial_layout(config, center)
            .into_iter()
            .map(|p| LiquidParticle { position: p, velocity: Vec2::ZERO, previous: p, density: 1.0 })
            .collect();
        let mut sim = Self { particles, rest_density: 0.0, emit_accumulator: 0.0, grid: HashMap::new() };
        sim.rest_density = rest_density(config.particle_spacing.max(2.0));
        sim
    }

    /// Faucet emission at `origin` (world px).
    pub fn emit(&mut self, config: &Liquid2D, origin: Vec2, dt: f32) {
        if config.emit_rate <= 0.0 {
            return;
        }
        self.emit_accumulator += config.emit_rate * dt;
        let mut k = 0;
        while self.emit_accumulator >= 1.0 && self.particles.len() < config.max_particles as usize {
            self.emit_accumulator -= 1.0;
            // A little spread so the stream isn't a single line.
            let jitter = ((self.particles.len() as f32 * 12.9898).sin() * 43758.545).fract() - 0.5;
            let side = Vec2::new(-config.emit_velocity.y, config.emit_velocity.x).normalize_or_zero();
            let p = origin + side * jitter * config.particle_spacing;
            let v = config.emit_velocity + side * jitter * 10.0;
            self.particles.push(LiquidParticle { position: p, velocity: v, previous: p - v * dt, density: 0.5 });
            k += 1;
            if k > 64 {
                break;
            }
        }
        self.emit_accumulator = self.emit_accumulator.min(4.0);
    }

    /// Gravity, viscosity, prediction and double-density relaxation. Leaves
    /// particles at their new (predicted) positions; collisions come next,
    /// then [`LiquidSim::finish`].
    pub fn relax(&mut self, config: &Liquid2D, gravity: Vec2, dt: f32) {
        let spacing = config.particle_spacing.max(2.0);
        let h = spacing * RADIUS_FACTOR;
        if self.rest_density <= 0.0 {
            self.rest_density = rest_density(spacing);
        }
        // 1. Gravity.
        for p in &mut self.particles {
            p.velocity += gravity * dt;
        }
        // 2. Viscosity: neighbours approaching each other trade velocity
        //    (linear + quadratic terms, along the line between them).
        self.rebuild_grid(h);
        let sigma = config.viscosity.clamp(0.0, 1.0) * 6.0;
        let beta = config.viscosity.clamp(0.0, 1.0) * 0.08 + 0.004;
        let n = self.particles.len();
        let mut ids: Vec<u32> = Vec::with_capacity(64);
        for i in 0..n {
            let pi = self.particles[i].position;
            ids.clear();
            ids.extend(self.neighbours(pi, h));
            for &j in &ids {
                let j = j as usize;
                if j <= i {
                    continue;
                }
                let d = self.particles[j].position - pi;
                let len = d.length();
                if len <= 1e-4 || len >= h {
                    continue;
                }
                let q = len / h;
                let dir = d / len;
                let u = (self.particles[i].velocity - self.particles[j].velocity).dot(dir);
                if u > 0.0 {
                    let impulse = dir * (dt * (1.0 - q) * (sigma * u + beta * u * u)).min(u);
                    self.particles[i].velocity -= impulse * 0.5;
                    self.particles[j].velocity += impulse * 0.5;
                }
            }
        }
        // 3. Predict.
        for p in &mut self.particles {
            p.previous = p.position;
            p.position += p.velocity * dt;
        }
        // 4. Double-density relaxation.
        self.rebuild_grid(h);
        let k = STIFFNESS * config.stiffness.max(0.05);
        let k_near = NEAR_STIFFNESS * config.stiffness.max(0.05);
        let dt2 = dt * dt;
        let max_push = spacing * 0.5;
        let mut neighbours: Vec<(usize, Vec2, f32)> = Vec::with_capacity(32);
        for i in 0..n {
            let pi = self.particles[i].position;
            neighbours.clear();
            let mut density = 0.0;
            let mut near = 0.0;
            ids.clear();
            ids.extend(self.neighbours(pi, h));
            for &j in &ids {
                let j = j as usize;
                if j == i {
                    continue;
                }
                let d = self.particles[j].position - pi;
                let len = d.length();
                if len >= h {
                    continue;
                }
                let q = 1.0 - len / h;
                density += q * q;
                near += q * q * q;
                let dir = if len > 1e-4 { d / len } else { Vec2::new(((i * 7 + j) % 3) as f32 - 1.0, 1.0).normalize() };
                neighbours.push((j, dir, q));
            }
            let pressure = k * (density - self.rest_density);
            let pressure_near = k_near * near;
            let mut dx = Vec2::ZERO;
            for &(j, dir, q) in &neighbours {
                let push = (dt2 * (pressure * q + pressure_near * q * q)).clamp(-max_push, max_push);
                let d = dir * push;
                self.particles[j].position += d * 0.5;
                dx -= d * 0.5;
            }
            self.particles[i].position += dx;
            self.particles[i].density = density / self.rest_density.max(1e-4);
        }
    }

    /// Velocities from the (collision-corrected) positions; drops particles
    /// that have fallen hopelessly far.
    pub fn finish(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.velocity = (p.position - p.previous) / dt;
            // Clamp speeds so no particle can cross a thin wall in a single
            // substep (1200 px/s = 5 px per 1/240 s).
            let max = 1200.0;
            if p.velocity.length_squared() > max * max {
                p.velocity = p.velocity.normalize() * max;
            }
        }
        self.particles.retain(|p| p.position.y > -20_000.0 && p.position.is_finite());
    }

    /// Moves particle `i` to `position` and adjusts its implied velocity so
    /// it no longer moves into the surface along `normal` (with `friction`
    /// on the sliding part) — relative to a surface moving at `surface_v`.
    /// Returns the velocity change (for pushing the body back).
    pub fn collide(&mut self, i: usize, position: Vec2, normal: Vec2, surface_v: Vec2, friction: f32, dt: f32) -> Vec2 {
        let p = &mut self.particles[i];
        p.position = position;
        let v = (p.position - p.previous) / dt - surface_v;
        let vn = v.dot(normal);
        if vn >= 0.0 {
            return Vec2::ZERO;
        }
        let tangent = v - normal * vn;
        let new_v = tangent * (1.0 - friction);
        let change = new_v - v;
        p.previous = p.position - (new_v + surface_v) * dt;
        change
    }

    /// Where particle `i` was before this step's prediction.
    pub fn previous(&self, i: usize) -> Vec2 {
        self.particles[i].previous
    }

    fn rebuild_grid(&mut self, h: f32) {
        for cell in self.grid.values_mut() {
            cell.clear();
        }
        for (i, p) in self.particles.iter().enumerate() {
            let key = ((p.position.x / h).floor() as i32, (p.position.y / h).floor() as i32);
            self.grid.entry(key).or_default().push(i as u32);
        }
        // Don't let empty cells pile up forever as the fluid moves.
        if self.grid.len() > self.particles.len() * 4 + 64 {
            self.grid.retain(|_, v| !v.is_empty());
        }
    }

    /// Indices of particles in the 3x3 cells around `at`.
    fn neighbours(&self, at: Vec2, h: f32) -> impl Iterator<Item = u32> + '_ {
        let (cx, cy) = ((at.x / h).floor() as i32, (at.y / h).floor() as i32);
        (-1..=1).flat_map(move |dy| (-1..=1).flat_map(move |dx| self.grid.get(&(cx + dx, cy + dy)).into_iter().flatten().copied()))
    }
}

/// The kernel-weight sum a particle sees in a perfectly packed fluid at
/// this spacing — the density the relaxation aims for.
fn rest_density(spacing: f32) -> f32 {
    let h = spacing * RADIUS_FACTOR;
    let mut sum = 0.0;
    let reach = (RADIUS_FACTOR.ceil() as i32) + 1;
    for r in -reach..=reach {
        for c in -reach..=reach {
            if r == 0 && c == 0 {
                continue;
            }
            let stagger = if r.rem_euclid(2) == 1 { 0.5 } else { 0.0 };
            let d = Vec2::new((c as f32 + stagger) * spacing, r as f32 * spacing).length();
            if d < h {
                let q = 1.0 - d / h;
                sum += q * q;
            }
        }
    }
    // Slightly under the packed value: a resting fluid sits a touch
    // compressed under gravity, which keeps it from fizzing.
    sum * 0.92
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs a liquid in a box made of four walls (plain plane clamps).
    fn settle(config: &Liquid2D, steps: usize) -> LiquidSim {
        let mut sim = LiquidSim::new(config, Vec2::new(0.0, 100.0));
        let dt = 1.0 / 120.0;
        let (half_w, floor) = (100.0, 0.0);
        for _ in 0..steps {
            sim.relax(config, Vec2::new(0.0, -900.0), dt);
            let r = config.particle_spacing * 0.5;
            for i in 0..sim.particles.len() {
                let p = sim.particles[i].position;
                if p.y < floor + r {
                    sim.collide(i, Vec2::new(p.x, floor + r), Vec2::Y, Vec2::ZERO, 0.1, dt);
                }
                let p = sim.particles[i].position;
                if p.x < -half_w + r {
                    sim.collide(i, Vec2::new(-half_w + r, p.y), Vec2::X, Vec2::ZERO, 0.1, dt);
                } else if p.x > half_w - r {
                    sim.collide(i, Vec2::new(half_w - r, p.y), -Vec2::X, Vec2::ZERO, 0.1, dt);
                }
            }
            sim.finish(dt);
        }
        sim
    }

    #[test]
    fn a_block_of_water_falls_and_settles_into_a_calm_pool() {
        let config = Liquid2D { size: Vec2::new(80.0, 80.0), ..Liquid2D::default() };
        let sim = settle(&config, 600);
        let n = sim.particles.len() as f32;
        assert!(n > 50.0);
        // Calm: nearly still after 5 s.
        let speed: f32 = sim.particles.iter().map(|p| p.velocity.length()).sum::<f32>() / n;
        assert!(speed < 25.0, "mean speed {speed}");
        let dens: f32 = sim.particles.iter().map(|p| p.density).sum::<f32>() / n;
        let minx = sim.particles.iter().map(|p| p.position.x).fold(f32::MAX, f32::min);
        let maxx = sim.particles.iter().map(|p| p.position.x).fold(f32::MIN, f32::max);
        eprintln!("DEBUG n={n} speed={speed:.2} density={dens:.2} x=[{minx:.0},{maxx:.0}]");
        // Spread across the 200 px floor and not compressed to a sliver or
        // blown apart: the pool's height ~ its volume / width.
        let top = sim.particles.iter().map(|p| p.position.y).fold(f32::MIN, f32::max);
        let volume = 80.0 * 80.0;
        let expected = volume / 200.0;
        eprintln!("DEBUG top={top:.1} expected={expected:.1}");
        assert!(top > expected * 0.5 && top < expected * 2.2, "pool height {top}, expected about {expected}");
        assert!(sim.particles.iter().all(|p| p.position.y > -1.0 && p.position.x.abs() < 101.0));
    }

    #[test]
    fn viscosity_slows_the_spread() {
        let thin = settle(&Liquid2D { size: Vec2::new(60.0, 100.0), viscosity: 0.0, ..Liquid2D::default() }, 90);
        let thick = settle(&Liquid2D { size: Vec2::new(60.0, 100.0), viscosity: 1.0, ..Liquid2D::default() }, 90);
        let spread = |s: &LiquidSim| s.particles.iter().map(|p| p.position.x.abs()).fold(0.0, f32::max);
        assert!(spread(&thick) < spread(&thin) + 1.0, "thick {} thin {}", spread(&thick), spread(&thin));
    }

    #[test]
    fn faucet_emits_up_to_the_cap() {
        let config = Liquid2D { size: Vec2::new(0.0, 0.0), emit_rate: 600.0, max_particles: 50, ..Liquid2D::default() };
        let mut sim = LiquidSim { particles: Vec::new(), ..LiquidSim::default() };
        for _ in 0..60 {
            sim.emit(&config, Vec2::new(0.0, 100.0), 1.0 / 60.0);
        }
        assert_eq!(sim.particles.len(), 50);
    }
}
