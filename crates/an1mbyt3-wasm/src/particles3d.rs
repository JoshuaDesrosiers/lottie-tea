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

use std::collections::{HashMap, VecDeque};

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
    /// Past world positions, newest first (ribbon trails), and time since
    /// the last one was recorded.
    trail: VecDeque<Vec3>,
    trail_clock: f32,
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
            trail: VecDeque::new(),
            trail_clock: 0.0,
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
        };
        p.position = world.transform_point3(p.local());
        self.particles.push(p);
    }
}

/// One particle's ribbon: head at its position, then its trail points.
/// Its width swells from a point at the head to `size` mid-way and back to
/// a point at the tail (`sin`-shaped, measured over the full `segments`
/// so a young particle's short trail grows out of a sharp head) — a
/// swooshing brush stroke, not a strip with square ends.
fn ribbon(p: &Particle, segments: f32, size: f32, color: [f32; 4], hardness: f32, eye: Vec3, out: &mut Vec<ParticleQuad3D>) {
    let points: Vec<Vec3> = std::iter::once(p.position).chain(p.trail.iter().copied()).collect();
    if points.len() < 2 {
        return;
    }
    // Each point's sideways direction, from the path through it (smooth
    // joins at bends), facing the camera.
    let sides: Vec<Vec3> = (0..points.len())
        .map(|j| {
            let before = points[j.saturating_sub(1)];
            let after = points[(j + 1).min(points.len() - 1)];
            let dir = (before - after).normalize_or(Vec3::Y);
            (eye - points[j]).normalize_or(Vec3::Z).cross(dir).normalize_or(Vec3::X)
        })
        .collect();
    let width = |j: usize| size * 0.5 * (std::f32::consts::PI * (j as f32 / segments).min(1.0)).sin().max(0.0).powf(0.7);
    // A slight fade over the last third only, so the body stays solid.
    let fade = |j: usize| (3.0 * (1.0 - j as f32 / segments)).clamp(0.0, 1.0);
    let tint = |j: usize| color.map(|c| c * fade(j));
    for j in 0..points.len() - 1 {
        let (a, b) = (points[j], points[j + 1]);
        if a.distance_squared(b) < 1e-10 {
            continue;
        }
        let (wa, wb) = (sides[j] * width(j), sides[j + 1] * width(j + 1));
        out.push(ParticleQuad3D {
            corners: [(a - wa).to_array(), (b - wb).to_array(), (b + wb).to_array(), (a + wa).to_array()],
            colors: [tint(j), tint(j + 1), tint(j + 1), tint(j)],
            // u pinned to the middle: only "across" shapes the edge.
            uvs: [[0.5, 0.0], [0.5, 0.0], [0.5, 1.0], [0.5, 1.0]],
            hardness,
            depth: ((a + b) * 0.5).distance(eye),
        });
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
            if cfg.trail_segments > 0 {
                // A point every `trail_time / segments` seconds.
                let interval = (cfg.trail_time / cfg.trail_segments as f32).max(0.001);
                if p.trail.is_empty() {
                    p.trail.push_front(position);
                }
                p.trail_clock += dt;
                while p.trail_clock >= interval {
                    p.trail_clock -= interval;
                    p.trail.push_front(position);
                }
                p.trail.truncate(cfg.trail_segments as usize);
            }
        }
        e.particles.retain(|p| p.age < p.life);
    }

    /// `id`'s particles as camera-facing quads (appended to `out`).
    /// `right` / `up` are the camera's world axes. With `trail_segments`,
    /// each particle is a ribbon instead: one quad per trail segment,
    /// tapering and fading from head to tail.
    pub fn quads(&self, id: NodeId, cfg: &ParticleSystem3D, world: Mat4, eye: Vec3, right: Vec3, up: Vec3, out: &mut Vec<ParticleQuad3D>) {
        let Some(e) = self.emitters.get(&id) else { return };
        let (scale, _, _) = world.to_scale_rotation_translation();
        let scale = ((scale.x.abs() + scale.y.abs() + scale.z.abs()) / 3.0).max(0.0001);
        let hardness = cfg.hardness.clamp(0.0, 1.0);
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
            if cfg.trail_segments > 0 && !p.trail.is_empty() {
                ribbon(p, cfg.trail_segments as f32, size, color, hardness, eye, out);
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
            out.push(ParticleQuad3D { corners: corners.map(|c| c.to_array()), colors: [color; 4], uvs: ParticleQuad3D::SPRITE_UVS, hardness, depth: p.position.distance(eye) });
        }
    }

    /// The nearest of `id`'s live particles a ray hits (`dir` normalized):
    /// its distance along the ray. Each particle counts as a ball the size
    /// it's drawn (a fluid blob's visible part is about half its splat),
    /// trail points included — so clicking the effect you *see* in the 3D
    /// view selects its emitter, not whatever node sits at its centre.
    pub fn ray_hit(&self, id: NodeId, cfg: &ParticleSystem3D, world: Mat4, origin: Vec3, dir: Vec3) -> Option<f32> {
        let e = self.emitters.get(&id)?;
        let (scale, _, _) = world.to_scale_rotation_translation();
        let scale = ((scale.x.abs() + scale.y.abs() + scale.z.abs()) / 3.0).max(0.0001);
        // Visible radius as a share of the drawn size: a fluid blob shows
        // about a quarter of its splat, a soft glow less than a crisp dot.
        let share = if cfg.fluid { 0.25 } else { 0.3 + 0.2 * cfg.hardness.clamp(0.0, 1.0) };
        let mut best: Option<f32> = None;
        for p in &e.particles {
            let t = p.age / p.life;
            // Faint wisps (smoke) don't catch clicks meant for what's behind.
            if cfg.color_at(t)[3] < 0.35 {
                continue;
            }
            let radius = (cfg.size_at(t) * scale * share).max(0.03);
            for c in std::iter::once(p.position).chain(p.trail.iter().copied()) {
                let along = (c - origin).dot(dir);
                if along <= 0.0 {
                    continue;
                }
                let miss = (origin + dir * along).distance_squared(c);
                if miss <= radius * radius {
                    // A fluid blob is drawn merged into its neighbours (and
                    // hidden behind solid meshes it wraps): count it at its
                    // centre, so a solid thing it surrounds — an eye inside
                    // an ink body — still wins a click on itself.
                    let hit = if cfg.fluid { along } else { along - (radius * radius - miss).sqrt() };
                    if best.is_none_or(|b| hit < b) {
                        best = Some(hit);
                    }
                }
            }
        }
        best
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
    fn rays_hit_live_particles() {
        let cfg = ParticleSystem3D { burst: 1, rate: 0.0, shape: Emitter3DShape::Point, swirl: 0.0, turbulence: 0.0, lifetime: 10.0, size_start: 0.5, size_end: 0.5, color_start: [1.0; 4], color_mid: [1.0; 4], color_end: [1.0; 4], hardness: 1.0, ..Default::default() };
        let mut sim = Particles3D::default();
        sim.step(node(), &cfg, Mat4::IDENTITY, 0.01);
        let hit = sim.ray_hit(node(), &cfg, Mat4::IDENTITY, Vec3::new(0.0, 0.0, 10.0), Vec3::NEG_Z).expect("hits the particle at the origin");
        assert!((hit - 9.75).abs() < 0.01, "{hit}");
        assert!(sim.ray_hit(node(), &cfg, Mat4::IDENTITY, Vec3::new(3.0, 0.0, 10.0), Vec3::NEG_Z).is_none());
    }

    #[test]
    fn additive_quads_hide_nothing() {
        let cfg = ParticleSystem3D { burst: 5, rate: 0.0, ..Default::default() };
        let mut sim = Particles3D::default();
        sim.step(node(), &cfg, Mat4::IDENTITY, 0.01);
        let mut out = Vec::new();
        sim.quads(node(), &cfg, Mat4::IDENTITY, Vec3::new(0.0, 0.0, 10.0), Vec3::X, Vec3::Y, &mut out);
        assert_eq!(out.len(), 5);
        assert!(out.iter().all(|q| q.colors[0][3] == 0.0 && q.colors[0][0] > 0.0));
    }

    #[test]
    fn trails_become_tapering_ribbons() {
        let cfg = ParticleSystem3D { burst: 1, rate: 0.0, lifetime: 5.0, swirl: 180.0, trail_segments: 8, trail_time: 0.4, hardness: 1.0, ..Default::default() };
        let mut sim = Particles3D::default();
        for _ in 0..60 {
            sim.step(node(), &cfg, Mat4::IDENTITY, 1.0 / 60.0);
        }
        let mut out = Vec::new();
        sim.quads(node(), &cfg, Mat4::IDENTITY, Vec3::new(0.0, 10.0, 0.0), Vec3::X, Vec3::Z, &mut out);
        // A full trail: the head + 8 recorded points = 8 segments.
        assert!((7..=8).contains(&out.len()), "{}", out.len());
        let width = |q: &ParticleQuad3D| Vec3::from(q.corners[0]).distance(Vec3::from(q.corners[3]));
        let middle = &out[out.len() / 2];
        assert!(width(middle) > width(&out[0]) && width(middle) > width(out.last().unwrap()), "pointed at both ends, widest mid-way");
        assert!(out.iter().all(|q| q.hardness == 1.0 && q.uvs[0][0] == 0.5));
    }
}
