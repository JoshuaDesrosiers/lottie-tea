//! **Particle System 3D** — a component on a `Node3D` that emits glowing
//! (or smoky) camera-facing particles in 3D space.
//!
//! Built around *orbits* rather than plain velocities, because the effects
//! people reach for in 3D — magic swirling round an orb, a vortex, a
//! portal ring, embers rising round a fire — are motion *around* a centre.
//! Each particle is born at a point on the emitter `shape` and then lives
//! in the emitter's frame as:
//!
//! - an **orbit**: an angle around its own orbit axis (the node's local Y,
//!   tilted at random by up to `tilt` degrees per particle, so a swarm
//!   forms crossing bands instead of one flat disc), advancing at `swirl`
//!   degrees per second (± `swirl_jitter`);
//! - a **radius** from that axis, growing at `speed` (outward burst) and
//!   pulled back towards the emitter `radius` by `pull` (a spring — a
//!   thick, stable band instead of particles flying off);
//! - a **height** along the axis, climbing at `rise`;
//! - plus a free **drift** (`velocity`, `gravity`, `turbulence`) on top.
//!
//! So `swirl = 0, pull = 0` is a classic fountain / explosion, and
//! `swirl = 200, pull = 4, tilt = 40` is a thick magical vortex.
//!
//! Colour and size blend from `*_start` to `*_end` over each particle's
//! life (`color_mid` in between, for a hot-core-then-cool-off look). In
//! `Additive` mode particles add light (black is invisible); in `Alpha`
//! mode they hide what's behind them (smoke, dust, dark matter). `stretch`
//! smears each particle along its motion into a streak; `trail_segments`
//! turns it into a true ribbon that follows its curved path (tapering
//! tendrils), and `hardness` makes the edges crisp (ink) instead of soft.
//!
//! The simulation itself lives in the editor / player (`an1mbyt3-wasm`'s
//! `particles3d`); this is only the authored, saved description. Like
//! every component it's postcard-serialized positionally, so fields are
//! never inserted or reordered — new ones would mean a new component.

use serde::{Deserialize, Serialize};

/// Where new particles appear, in the node's local space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Emitter3DShape {
    /// All at the node's origin.
    Point,
    /// Anywhere inside a ball of `radius`.
    Sphere,
    /// On a ball's surface (`radius`, give or take `thickness`).
    Shell,
    /// Round a ring of `radius` in the local XZ plane (± `thickness`).
    #[default]
    Ring,
}

/// How particles combine with what's behind them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ParticleBlend3D {
    /// Adds light: glows, magic, fire, energy.
    #[default]
    Additive,
    /// Covers what's behind: smoke, dust, ink.
    Alpha,
}

/// A value over a particle's life: points `[t, value]` with `t` in `0..1`
/// (birth → death), joined by straight lines and held flat past either
/// end. Behaviours scale their strength by it, which is how "fly out
/// freely, then converge" (0 → 1) or "start gathered, then burst out"
/// (1 → 0) are both one behaviour with a different curve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LifeCurve {
    pub points: Vec<[f32; 2]>,
}

impl Default for LifeCurve {
    /// A ramp from nothing at birth to full at death.
    fn default() -> Self {
        Self { points: vec![[0.0, 0.0], [1.0, 1.0]] }
    }
}

impl LifeCurve {
    /// A flat curve.
    pub fn constant(value: f32) -> Self {
        Self { points: vec![[0.0, value], [1.0, value]] }
    }

    /// The curve's value at `t` (unsorted points are fine; none = 1).
    pub fn eval(&self, t: f32) -> f32 {
        let mut pts: Vec<[f32; 2]> = self.points.iter().copied().filter(|p| p[0].is_finite() && p[1].is_finite()).collect();
        if pts.is_empty() {
            return 1.0;
        }
        pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let t = t.clamp(0.0, 1.0);
        if t <= pts[0][0] {
            return pts[0][1];
        }
        for w in pts.windows(2) {
            let ([t0, v0], [t1, v1]) = (w[0], w[1]);
            if t <= t1 {
                let k = if t1 > t0 { (t - t0) / (t1 - t0) } else { 1.0 };
                return v0 + (v1 - v0) * k;
            }
        }
        pts[pts.len() - 1][1]
    }
}

/// What a particle behaviour does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ParticleBehaviorKind3D {
    /// Moves each particle *towards* the point: at strength × curve = 1 it
    /// is exactly on it, at 0.5 halfway between where it would be and the
    /// point. Gathers to a spot, sucks into a portal, forms on a target.
    #[default]
    Converge,
    /// Accelerates particles towards the point (m/s², gravity-well style —
    /// they overshoot and fall back).
    Attract,
    /// Accelerates particles away from the point.
    Repel,
    /// Slows particles down (their swirl and any push they picked up).
    Drag,
}

/// Which space a behaviour's point is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PointSpace {
    /// Relative to the emitter node — moves, turns and scales with it.
    #[default]
    Local,
    /// A fixed spot in the world.
    World,
}

/// One behaviour in a `ParticleSystem3D`'s stack (applied in order, after
/// the emitter's own motion).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ParticleBehavior3D {
    pub kind: ParticleBehaviorKind3D,
    pub enabled: bool,
    /// Overall strength (Converge: 0..1 blend; Attract / Repel: m/s²;
    /// Drag: per second).
    pub strength: f32,
    /// Strength over each particle's life (× `strength`).
    pub curve: LifeCurve,
    /// The point Converge / Attract / Repel use.
    pub point: [f32; 3],
    pub space: PointSpace,
}

impl Default for ParticleBehavior3D {
    fn default() -> Self {
        Self { kind: ParticleBehaviorKind3D::Converge, enabled: true, strength: 1.0, curve: LifeCurve::default(), point: [0.0; 3], space: PointSpace::Local }
    }
}

impl ParticleBehavior3D {
    /// Strength × curve at life fraction `t`.
    pub fn weight(&self, t: f32) -> f32 {
        self.strength * self.curve.eval(t)
    }
}

/// A 3D particle emitter — see the module docs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ParticleSystem3D {
    /// Emits continuously at `rate` while on (bursts work either way).
    pub emitting: bool,
    /// New particles per second.
    pub rate: f32,
    /// Particles emitted at once when play starts.
    pub burst: u32,
    /// Hard cap on live particles (oldest make way).
    pub max_particles: u32,
    /// Seconds each particle lives.
    pub lifetime: f32,
    /// Random lifetime spread, `0..1` (0.3 = ±30 %).
    pub lifetime_jitter: f32,

    pub shape: Emitter3DShape,
    /// Emitter size, world units (meters).
    pub radius: f32,
    /// Random spread around `radius` for Ring / Shell.
    pub thickness: f32,

    /// Outward speed from the orbit axis, m/s (negative = inward).
    pub speed: f32,
    /// Climb along the orbit axis, m/s.
    pub rise: f32,
    /// Orbit speed, degrees per second (0 = no swirl; negative = the other way).
    pub swirl: f32,
    /// Random orbit-speed spread, `0..1`.
    pub swirl_jitter: f32,
    /// Random tilt of each particle's orbit axis, degrees (0 = all orbit
    /// the node's Y; 90 = every direction).
    pub tilt: f32,
    /// Spring back towards `radius` (0 = none; 3–6 = a tight band).
    pub pull: f32,
    /// Constant drift, world m/s.
    pub velocity: [f32; 3],
    /// Downward acceleration, m/s² (negative floats up).
    pub gravity: f32,
    /// Random wobble strength, m/s.
    pub turbulence: f32,

    /// Size (diameter, meters) at birth and at death.
    pub size_start: f32,
    pub size_end: f32,
    /// Linear RGBA at birth, mid-life and death.
    pub color_start: [f32; 4],
    pub color_mid: [f32; 4],
    pub color_end: [f32; 4],
    /// Brightness multiplier (Additive glow strength; > 1 = hotter).
    pub intensity: f32,
    pub blend: ParticleBlend3D,
    /// Smear along motion: each particle becomes a streak as long as
    /// `stretch` seconds of its travel (0 = round; 0.05–0.2 = ribbons).
    pub stretch: f32,
    /// Particles move with the node (true) or stay where they were born
    /// in the world when it moves (false: trails behind a moving emitter).
    pub local_space: bool,
    /// Random seed (same seed = same pattern).
    pub seed: u32,
    /// Extra behaviours (Converge, Attract, Repel, Drag), each with its own
    /// strength curve over particle life.
    pub behaviors: Vec<ParticleBehavior3D>,
    /// 0 = soft, glowing blobs; 1 = crisp, solid shapes (ink, droplets).
    pub hardness: f32,
    /// Ribbon trails: each particle drags a tapering band through the last
    /// `trail_time` seconds of its path, in this many segments (0 = no
    /// trail, a round sprite). `size` is then the ribbon's width at its head.
    pub trail_segments: u32,
    pub trail_time: f32,
    /// **Fluid**: draw all of this emitter's particles (and ribbons) as one
    /// liquid surface — they merge where they touch (screen-space
    /// metaballs). `color_start` is the liquid's colour; each particle's
    /// colour alpha over life is how much liquid it adds.
    pub fluid: bool,
    /// The liquid's rim colour (alpha = strength) where its surface turns away.
    pub fluid_edge: [f32; 4],
    /// How much overlapping it takes to count as liquid: lower = fatter,
    /// more merged; higher = thinner, more separate drops.
    pub fluid_threshold: f32,
    /// Wet highlight strength (0 = matte).
    pub fluid_gloss: f32,
    /// Fluid emitters with the same group number are one liquid: their
    /// blobs merge with each other (the group takes the colours of its
    /// first emitter). Different numbers stay separate liquids.
    pub fluid_group: u32,
}

impl Default for ParticleSystem3D {
    fn default() -> Self {
        Self {
            emitting: true,
            rate: 60.0,
            burst: 0,
            max_particles: 600,
            lifetime: 1.6,
            lifetime_jitter: 0.3,
            shape: Emitter3DShape::Ring,
            radius: 1.0,
            thickness: 0.15,
            speed: 0.0,
            rise: 0.0,
            swirl: 120.0,
            swirl_jitter: 0.2,
            tilt: 25.0,
            pull: 2.0,
            velocity: [0.0, 0.0, 0.0],
            gravity: 0.0,
            turbulence: 0.15,
            size_start: 0.25,
            size_end: 0.05,
            color_start: [0.9, 0.4, 1.0, 1.0],
            color_mid: [0.6, 0.2, 1.0, 0.8],
            color_end: [0.2, 0.05, 0.5, 0.0],
            intensity: 1.0,
            blend: ParticleBlend3D::Additive,
            stretch: 0.0,
            local_space: true,
            seed: 7,
            behaviors: Vec::new(),
            hardness: 0.0,
            trail_segments: 0,
            trail_time: 0.3,
            fluid: false,
            fluid_edge: [0.9, 0.2, 0.8, 0.8],
            fluid_threshold: 0.5,
            fluid_gloss: 0.6,
            fluid_group: 0,
        }
    }
}

impl ParticleSystem3D {
    /// The colour `t` (0 = birth, 1 = death) through its life: start → mid
    /// over the first half, mid → end over the second.
    pub fn color_at(&self, t: f32) -> [f32; 4] {
        let t = t.clamp(0.0, 1.0);
        let (a, b, k) = if t < 0.5 { (self.color_start, self.color_mid, t * 2.0) } else { (self.color_mid, self.color_end, t * 2.0 - 1.0) };
        [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k, a[3] + (b[3] - a[3]) * k]
    }

    /// The size `t` through its life.
    pub fn size_at(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        self.size_start + (self.size_end - self.size_start) * t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_runs_start_mid_end() {
        let p = ParticleSystem3D { color_start: [1.0, 0.0, 0.0, 1.0], color_mid: [0.0, 1.0, 0.0, 1.0], color_end: [0.0, 0.0, 1.0, 0.0], ..Default::default() };
        assert_eq!(p.color_at(0.0), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(p.color_at(0.5), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(p.color_at(1.0), [0.0, 0.0, 1.0, 0.0]);
    }

    #[test]
    fn life_curves_interpolate_and_hold() {
        let c = LifeCurve { points: vec![[0.5, 1.0], [0.0, 0.0]] };
        assert_eq!(c.eval(0.25), 0.5);
        assert_eq!(c.eval(0.9), 1.0);
        assert_eq!(LifeCurve { points: vec![] }.eval(0.3), 1.0);
    }

    #[test]
    fn round_trips_through_json() {
        let p = ParticleSystem3D { shape: Emitter3DShape::Shell, blend: ParticleBlend3D::Alpha, swirl: -90.0, behaviors: vec![ParticleBehavior3D { space: PointSpace::World, ..Default::default() }], ..Default::default() };
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<ParticleSystem3D>(&json).unwrap(), p);
    }
}
