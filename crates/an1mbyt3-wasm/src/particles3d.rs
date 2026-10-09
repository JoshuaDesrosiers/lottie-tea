//! The live simulation behind `an1mbyt3_core::ParticleSystem3D` (see that
//! module's docs for the orbit model) and the camera-facing quads the 3D
//! pass draws (`an1mbyt3_renderer::ParticleQuad3D`).
//!
//! `WasmEditor::collect_scene_3d` calls [`Particles3D::step`] then
//! [`Particles3D::quads`] for every enabled emitter each frame — in Design
//! too, so an author sees the effect while tuning it. Starting or stopping
//! Playground clears every emitter (and replays its `burst`), the same way
//! 2D particle systems restart on Play.
//!
//! Particles are stored in the emitter's *orbit coordinates* (axis, angle,
//! radius, height, drift) rather than as a plain position, which is what
//! makes swirls stable: a particle orbiting at 200°/s for two seconds is
//! still exactly on its band, with no integration drift.

use std::collections::HashMap;

use glam::{Mat4, Quat, Vec3};
use an1mbyt3_core::{Emitter3DShape, NodeId, ParticleBehaviorKind3D, ParticleBlend3D, ParticleSystem3D, PointSpace};
use an1mbyt3_renderer::ParticleQuad3D;

/// One live particle.
struct Particle {
    /// Orbit axis and the two directions spanning its orbit plane (local).
    axis: Vec3,
    u: Vec3,
    v: Vec3,
    angle: f32,
    /// Radians per second.
    angular_speed: f32,
    radius: f32,
    /// The radius `pull` springs back to (where it was born).
    rest_radius: f32,
    height: f32,
    /// Free offset (velocity, gravity, turbulence), local.
    drift: Vec3,
    /// Wobble phases (turbulence).
    phase: Vec3,
    age: f32,
    life: f32,
    /// World matrix it was born under (world-space emitters only).
    birth: Option<Mat4>,
    /// World-space push from Attract / Repel (and its velocity).
    push: Vec3,
    push_velocity: Vec3,
    /// World position last step, and world velocity (for `stretch`).
    position: Vec3,
    velocity: Vec3,
}

/// One emitter's particles.
#[derive(Default)]
struct Emitter {
    particles: Vec<Particle>,
    /// Fractional particles owed to `rate`.
    owed: f32,
    rng: u64,
    /// Whether the start-of-run `burst` was emitted.
    burst_done: bool,
}

/// Every emitter's live particles, keyed by node.
#[derive(Default)]
pub struct Particles3D {
    emitters: HashMap<NodeId, Emitter>,
    /// Playground state last frame — a change restarts everything.
    was_playing: bool,
}

impl Emitter {
    /// xorshift64* → `0..1`.
    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        ((x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40) as f32) / ((1u64 << 24) as f32)
    }

    fn signed(&mut self) -> f32 {
        self.rand() * 2.0 - 1.0
    }

    /// A uniformly random unit vector.
    fn direction(&mut self) -> Vec3 {
        let z = self.signed();
        let a = self.rand() * std::f32::consts::TAU;
        let r = (1.0 - z * z).max(0.0).sqrt();
        Vec3::new(r * a.cos(), z, r * a.sin())
    }

    fn spawn(&mut self, cfg: &ParticleSystem3D, world: Mat4) {
        // The orbit axis: local Y, tilted a random amount up to `tilt`
        // towards a random horizontal direction.
        let tilt = cfg.tilt.clamp(0.0, 180.0).to_radians() * self.rand();
        let heading = self.rand() * std::f32::consts::TAU;
        let axis = Quat::from_axis_angle(Vec3::new(heading.cos(), 0.0, heading.sin()), tilt) * Vec3::Y;
        let u = axis.any_orthonormal_vector();
        let v = axis.cross(u);

        // Birth point (local), by shape.
        let r = cfg.radius.max(0.0);
        let point = match cfg.shape {
            Emitter3DShape::Point => Vec3::ZERO,
            Emitter3DShape::Sphere => self.direction() * r * self.rand().cbrt(),
            Emitter3DShape::Shell => self.direction() * (r + self.signed() * cfg.thickness),
            Emitter3DShape::Ring => {
                // On a ring round the (tilted) orbit axis, so tilted
                // particles start on their own band, not the flat one.
                let a = self.rand() * std::f32::consts::TAU;
                (u * a.cos() + v * a.sin()) * (r + self.signed() * cfg.thickness) + axis * self.signed() * cfg.thickness * 0.5
            }
        };
        let height = point.dot(axis);
        let radial = point - axis * height;
        let radius = radial.length();
        let angle = if radius > 1e-5 { radial.dot(v).atan2(radial.dot(u)) } else { self.rand() * std::f32::consts::TAU };
        let swirl = cfg.swirl.to_radians() * (1.0 + self.signed() * cfg.swirl_jitter.clamp(0.0, 1.0));
        let life = (cfg.lifetime * (1.0 + self.signed() * cfg.lifetime_jitter.clamp(0.0, 0.95))).max(0.02);
        let phase = Vec3::new(self.rand(), self.rand(), self.rand()) * std::f32::consts::TAU;
        let mut p = Particle {
            axis,
            u,
            v,
            angle,
            angular_speed: swirl,
            radius,
            rest_radius: radius,
            height,
            drift: Vec3::ZERO,
            phase,
            age: 0.0,
            life,
            birth: (!cfg.local_space).then_some(world),
            push: Vec3::ZERO,
            push_velocity: Vec3::ZERO,
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
        };
        p.position = world.transform_point3(p.local());
        self.particles.push(p);
    }
}

impl Particle {
    fn local(&self) -> Vec3 {
        self.axis * self.height + (self.u * self.angle.cos() + self.v * self.angle.sin()) * self.radius + self.drift
    }
}

impl Particles3D {
    /// Drops emitters whose node is gone (or no longer has the component).
    pub fn retain(&mut self, live: &std::collections::HashSet<NodeId>) {
        self.emitters.retain(|id, _| live.contains(id));
    }

    /// Restarts everything when Playground starts or stops.
    pub fn sync_playing(&mut self, playing: bool) {
        if playing != self.was_playing {
            self.emitters.clear();
            self.was_playing = playing;
        }
    }

    /// Emits `count` particles at once (On Click / scripts: "burst").
    pub fn burst(&mut self, id: NodeId, cfg: &ParticleSystem3D, world: Mat4, count: u32) {
        let e = self.emitter(id, cfg);
        for _ in 0..count.min(cfg.max_particles.max(1)) {
            e.spawn(cfg, world);
        }
        let cap = cfg.max_particles.max(1) as usize;
        if e.particles.len() > cap {
            let excess = e.particles.len() - cap;
            e.particles.drain(..excess);
        }
    }

    fn emitter(&mut self, id: NodeId, cfg: &ParticleSystem3D) -> &mut Emitter {
        self.emitters.entry(id).or_insert_with(|| Emitter { rng: (cfg.seed as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1, ..Default::default() })
    }

    /// Advances `id`'s particles by `dt` seconds under `world`.
    pub fn step(&mut self, id: NodeId, cfg: &ParticleSystem3D, world: Mat4, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        let cap = cfg.max_particles.max(1) as usize;
        let e = self.emitter(id, cfg);
        if !e.burst_done {
            e.burst_done = true;
            for _ in 0..cfg.burst.min(cfg.max_particles) {
                e.spawn(cfg, world);
            }
        }
        if cfg.emitting && cfg.rate > 0.0 {
            e.owed += cfg.rate * dt;
            // Never more than a frame's worth of catch-up.
            let n = (e.owed.floor() as usize).min(cap);
            e.owed -= n as f32;
            for _ in 0..n {
                e.spawn(cfg, world);
            }
        }
        if e.particles.len() > cap {
            let excess = e.particles.len() - cap;
            e.particles.drain(..excess);
        }

        let pull = 1.0 - (-cfg.pull.max(0.0) * dt).exp();
        let drift_velocity = Vec3::from(cfg.velocity);
        for p in &mut e.particles {
            p.age += dt;
            p.angle += p.angular_speed * dt;
            p.radius = (p.radius + cfg.speed * dt).max(0.0);
            p.radius += (p.rest_radius - p.radius) * pull;
            p.height += cfg.rise * dt;
            let wobble = Vec3::new((p.age * 2.3 + p.phase.x).sin(), (p.age * 1.9 + p.phase.y).sin(), (p.age * 2.7 + p.phase.z).sin());
            p.drift += (drift_velocity + Vec3::new(0.0, -cfg.gravity * p.age, 0.0) + wobble * cfg.turbulence) * dt;
            let matrix = p.birth.unwrap_or(world);
            let mut position = matrix.transform_point3(p.local()) + p.push;
            // Behaviours, in order, each scaled by its curve at this age.
            let t = (p.age / p.life).clamp(0.0, 1.0);
            // Converges apply last (in order), so a weight of 1 pins exactly.
            let mut converges: Vec<(Vec3, f32)> = Vec::new();
            for b in cfg.behaviors.iter().filter(|b| b.enabled) {
                let w = b.weight(t);
                let target = match b.space {
                    PointSpace::Local => world.transform_point3(Vec3::from(b.point)),
                    PointSpace::World => Vec3::from(b.point),
                };
                match b.kind {
                    ParticleBehaviorKind3D::Converge => converges.push((target, w.clamp(0.0, 1.0))),
                    ParticleBehaviorKind3D::Attract | ParticleBehaviorKind3D::Repel => {
                        let sign = if b.kind == ParticleBehaviorKind3D::Attract { 1.0 } else { -1.0 };
                        let to = target - position;
                        // Softened near the point so nothing explodes there.
                        let dir = to / (to.length() + 0.15);
                        p.push_velocity += dir * w * sign * dt;
                    }
                    ParticleBehaviorKind3D::Drag => {
                        let keep = (-w.max(0.0) * dt).exp();
                        p.push_velocity *= keep;
                        p.angular_speed *= keep;
                    }
                }
            }
            p.push += p.push_velocity * dt;
            position += p.push_velocity * dt;
            for (target, k) in converges {
                position = position.lerp(target, k);
            }
            if dt > 0.0 {
                p.velocity = (position - p.position) / dt;
            }
            p.position = position;
        }
        e.particles.retain(|p| p.age < p.life);
    }

    /// `id`'s particles as camera-facing quads (appended to `out`).
    /// `right` / `up` are the camera's world axes.
    pub fn quads(&self, id: NodeId, cfg: &ParticleSystem3D, world: Mat4, eye: Vec3, right: Vec3, up: Vec3, out: &mut Vec<ParticleQuad3D>) {
        let Some(e) = self.emitters.get(&id) else { return };
        let (scale, _, _) = world.to_scale_rotation_translation();
        let scale = ((scale.x.abs() + scale.y.abs() + scale.z.abs()) / 3.0).max(0.0001);
        for p in &e.particles {
            let t = p.age / p.life;
            let size = cfg.size_at(t) * scale;
            if size <= 0.0 {
                continue;
            }
            let c = cfg.color_at(t);
            let a = c[3].clamp(0.0, 1.0);
            let k = cfg.intensity.max(0.0) * a;
            // Premultiplied: additive particles hide nothing (alpha 0).
            let color = [c[0] * k, c[1] * k, c[2] * k, if cfg.blend == ParticleBlend3D::Additive { 0.0 } else { a }];
            if color[0] + color[1] + color[2] + color[3] <= 0.0005 {
                continue;
            }
            let half = size * 0.5;
            let to_eye = (eye - p.position).normalize_or(Vec3::Z);
            let along = p.velocity - to_eye * p.velocity.dot(to_eye);
            let corners = if cfg.stretch > 0.0 && along.length_squared() > 1e-6 {
                // A streak: long along the on-screen motion, `size` wide.
                let axis = along.normalize();
                let side = to_eye.cross(axis).normalize_or(right) * half;
                let length = (size + p.velocity.length() * cfg.stretch * scale) * 0.5;
                let a = axis * length;
                [p.position - a - side, p.position + a - side, p.position + a + side, p.position - a + side]
            } else {
                let (r, u) = (right * half, up * half);
                [p.position - r - u, p.position + r - u, p.position + r + u, p.position - r + u]
            };
            out.push(ParticleQuad3D { corners: corners.map(|c| c.to_array()), color, depth: p.position.distance(eye) });
        }
    }

    /// Live particle count (the profiler / tests).
    #[allow(dead_code)]
    pub fn count(&self, id: NodeId) -> usize {
        self.emitters.get(&id).map_or(0, |e| e.particles.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::KeyData;

    fn node() -> NodeId {
        NodeId::from(KeyData::from_ffi(1))
    }

    #[test]
    fn emits_at_rate_and_dies_at_lifetime() {
        let cfg = ParticleSystem3D { rate: 100.0, lifetime: 0.5, lifetime_jitter: 0.0, ..Default::default() };
        let mut sim = Particles3D::default();
        for _ in 0..30 {
            sim.step(node(), &cfg, Mat4::IDENTITY, 1.0 / 60.0);
        }
        // Half a second of 100/s, all still alive (and none older).
        let n = sim.count(node());
        assert!((45..=52).contains(&n), "{n}");
        for _ in 0..60 {
            sim.step(node(), &ParticleSystem3D { emitting: false, ..cfg.clone() }, Mat4::IDENTITY, 1.0 / 60.0);
        }
        assert_eq!(sim.count(node()), 0);
    }

    #[test]
    fn a_pulled_ring_stays_a_band() {
        let cfg = ParticleSystem3D { rate: 0.0, burst: 200, radius: 2.0, thickness: 0.0, tilt: 0.0, speed: 0.0, turbulence: 0.0, lifetime: 10.0, swirl: 300.0, ..Default::default() };
        let mut sim = Particles3D::default();
        for _ in 0..120 {
            sim.step(node(), &cfg, Mat4::IDENTITY, 1.0 / 60.0);
        }
        let e = &sim.emitters[&node()];
        assert_eq!(e.particles.len(), 200);
        for p in &e.particles {
            let horizontal = Vec3::new(p.position.x, 0.0, p.position.z).length();
            assert!((horizontal - 2.0).abs() < 0.01 && p.position.y.abs() < 0.01, "{:?}", p.position);
        }
    }

    #[test]
    fn converge_follows_its_curve() {
        use an1mbyt3_core::{LifeCurve, ParticleBehavior3D};
        // Free for the first half of life, fully on the point by the end.
        let cfg = ParticleSystem3D {
            rate: 0.0,
            burst: 20,
            lifetime: 1.0,
            lifetime_jitter: 0.0,
            radius: 3.0,
            behaviors: vec![ParticleBehavior3D { curve: LifeCurve { points: vec![[0.0, 0.0], [0.5, 0.0], [0.95, 1.0]] }, point: [0.0, 5.0, 0.0], space: PointSpace::World, ..Default::default() }],
            ..Default::default()
        };
        let mut sim = Particles3D::default();
        let world = Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
        for _ in 0..12 {
            sim.step(node(), &cfg, world, 1.0 / 30.0);
        }
        // Early: still round the emitter (x ≈ 10), nowhere near the point.
        assert!(sim.emitters[&node()].particles.iter().all(|p| p.position.distance(Vec3::new(0.0, 5.0, 0.0)) > 5.0));
        for _ in 0..17 {
            sim.step(node(), &cfg, world, 1.0 / 30.0);
        }
        // Late (t > 0.95): exactly on the world point.
        assert!(sim.emitters[&node()].particles.iter().all(|p| p.position.distance(Vec3::new(0.0, 5.0, 0.0)) < 1e-3));
    }

    #[test]
    fn additive_quads_hide_nothing() {
        let cfg = ParticleSystem3D { burst: 5, rate: 0.0, ..Default::default() };
        let mut sim = Particles3D::default();
        sim.step(node(), &cfg, Mat4::IDENTITY, 0.01);
        let mut out = Vec::new();
        sim.quads(node(), &cfg, Mat4::IDENTITY, Vec3::new(0.0, 0.0, 10.0), Vec3::X, Vec3::Y, &mut out);
        assert_eq!(out.len(), 5);
        assert!(out.iter().all(|q| q.color[3] == 0.0 && q.color[0] > 0.0));
    }
}
