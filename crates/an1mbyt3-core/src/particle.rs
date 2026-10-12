//! [`NodeKind::ParticleSystem`] — a Unity-`ParticleSystem`-style game object:
//! one emitter authoring how many particles spawn, where, and how each one
//! evolves over its own lifetime (color/size/rotation), plus simple physics
//! (gravity/drag). Deliberately not Unity's full module set — this is the
//! "core" slice covering the overwhelming majority of real effects (fire,
//! smoke, sparks, confetti, magic, explosions): Main, Emission, Shape,
//! simple Gravity/Drag, Color over Lifetime, Size over Lifetime, and a
//! constant-rate Rotation over Lifetime. Explicitly **not** included (a
//! real, disclosed scope limit, not a silent drop — same convention as
//! `ComponentDefinition`'s single-`Shape` limit or `Artboard::spawn_clone_of`'s
//! single-node-not-a-subtree limit elsewhere in this codebase): Limit
//! Velocity, Inherit Velocity, Force over Lifetime, Color by Speed, Size by
//! Speed, Noise, Collision, Triggers, Sub Emitters, Texture Sheet Animation,
//! Lights, Trails. Each of those is its own real simulation feature with its
//! own field set — upgradeable later, not designed away.
//!
//! This struct is pure authored data (what an author sets up once, in the
//! Inspector) — the actual *live* particles (position/velocity/age of each
//! one right now) are runtime-only state, never saved to the document, the
//! same "ephemeral, not serialized" category as `Artboard::spawn_clone_of`'s
//! clones. See `an1mbyt3_runtime::particle_sim` for that half.

use serde::{Deserialize, Serialize};

/// A `min..=max` authored range a start value is drawn uniformly from every
/// time a particle spawns — `min == max` is a plain fixed value (Unity's own
/// "Constant" vs "Random Between Two Constants" modes collapsed into one
/// shape, since a fixed value is just a zero-width range). Never serializes
/// `min > max` (every authoring path clamps), but nothing here enforces that
/// — same "authoring UI's job, not this type's" convention `GradientStop::offset`
/// already follows.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ParticleRange {
    pub min: f32,
    pub max: f32,
}

impl ParticleRange {
    pub fn fixed(value: f32) -> Self {
        Self { min: value, max: value }
    }
}

/// Main module — the particle system's own overall lifecycle/budget and
/// each new particle's starting values. Mirrors Unity's "Main" module
/// closely (`duration`/`looping`/`startLifetime`/`startSpeed`/`startSize`/
/// `startRotation`/`startColor`/`gravityModifier`/`simulationSpace`/
/// `maxParticles`/`playOnAwake`), minus Unity's own `startDelay`/`prewarm`/
/// `scalingMode`/`playbackSpeed` (deferred, not core to the overwhelming
/// majority of effects).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParticleMain {
    /// One cycle's length, in seconds — only meaningful while `looping` is
    /// `true` (restarts emission from the top every `duration` seconds);
    /// while `false`, emission simply stops for good once a non-looping
    /// system has run for `duration` seconds (existing live particles still
    /// finish out their own `start_lifetime`).
    pub duration: f32,
    pub looping: bool,
    pub start_lifetime: ParticleRange,
    pub start_speed: ParticleRange,
    pub start_size: ParticleRange,
    /// Radians — this workspace's own rotation unit throughout (see
    /// `an1mbyt3_core::Transform2D::rotation`'s own doc comment), not
    /// Unity's degrees.
    pub start_rotation: ParticleRange,
    /// Straight (non-premultiplied) linear RGBA — every spawned particle's
    /// own base color, multiplied by `color_over_lifetime`'s sampled value
    /// (if any) the same way Unity's own Main-module start color composes
    /// with its Color-over-Lifetime module.
    pub start_color: [f32; 4],
    /// Downward (`-y`) acceleration applied to every live particle, in this
    /// workspace's own world units/second² — `0.0` (the default) means no
    /// gravity at all, matching a freshly-added system's "just emits
    /// outward, nothing pulls it down" starting behavior.
    pub gravity_modifier: f32,
    /// `0.0..=1.0` fraction of `start_speed` lost per second (simple linear
    /// drag, not Unity's own per-axis damping) — `0.0` (the default) means
    /// particles never slow down on their own.
    pub drag: f32,
    pub simulation_space: SimulationSpace,
    /// Hard cap on simultaneously-live particles — spawning that would
    /// exceed this simply doesn't happen (the oldest particles are *not*
    /// evicted to make room; a system at its cap just waits for natural
    /// deaths before spawning more), matching Unity's own behavior.
    pub max_particles: u32,
    pub play_on_awake: bool,
}

impl Default for ParticleMain {
    fn default() -> Self {
        // `start_speed`/`start_size` are in this workspace's own world
        // units — the same scale a preset shape is authored at (a preset
        // `Rectangle`/`Ellipse` is ~120 units across, `editor.rs`'s own
        // `preset_shape_path`). A first pass at these defaults used a
        // 0..1-ish scale instead (`start_size: 0.5`, `start_speed: 2.0`) —
        // at this app's actual scale that rendered as a sub-pixel, nearly
        // motionless speck, not obviously-broken so much as invisible,
        // the real "is it broken or am I just not configuring it right"
        // bug report this fixes. 30 units is clearly visible without
        // dwarfing a 120-unit shape; 100 units/second covers real screen
        // distance within a couple of seconds.
        Self {
            duration: 5.0,
            looping: true,
            start_lifetime: ParticleRange::fixed(2.0),
            start_speed: ParticleRange::fixed(100.0),
            start_size: ParticleRange::fixed(30.0),
            start_rotation: ParticleRange::fixed(0.0),
            start_color: [1.0, 1.0, 1.0, 1.0],
            gravity_modifier: 0.0,
            drag: 0.0,
            simulation_space: SimulationSpace::Local,
            max_particles: 100,
            play_on_awake: true,
        }
    }
}

/// Whether a live particle's position tracks its emitter node's own moving
/// transform (`Local`, the default — a torch's sparks drift with the
/// torch) or, once spawned, stays put in the artboard regardless of the
/// emitter moving afterward (`World` — footprints left behind a walking
/// character). Mirrors Unity's own "Simulation Space" exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SimulationSpace {
    #[default]
    Local,
    World,
}

/// Emission module — how often new particles spawn. Mirrors Unity's own
/// "Rate over Time" + "Bursts" (minus "Rate over Distance", deferred — it
/// only matters for a moving emitter, a smaller slice of real use than the
/// time-based rate every effect uses).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParticleEmission {
    /// Particles spawned per second while the system is emitting — `0.0`
    /// disables continuous emission entirely (a system that only ever
    /// fires via `bursts`, e.g. a one-shot explosion, is a real, intended
    /// configuration, not a mistake).
    pub rate_over_time: f32,
    pub bursts: Vec<ParticleBurst>,
}

impl Default for ParticleEmission {
    fn default() -> Self {
        Self { rate_over_time: 10.0, bursts: Vec::new() }
    }
}

/// One scheduled burst — spawns `count` particles all at once, `time`
/// seconds into the system's own cycle, repeating every `interval` seconds
/// for `cycles` total firings (`cycles: 1` is a true one-shot burst, the
/// common case). Mirrors Unity's own Bursts list exactly (minus its
/// per-burst random-count range and probability — a fixed `count`, always
/// firing, is the overwhelming common case).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ParticleBurst {
    pub time: f32,
    pub count: u32,
    pub cycles: u32,
    pub interval: f32,
}

/// Shape module — where a new particle starts (position) and which
/// direction it starts moving (combined with Main's own `start_speed` to
/// get an initial velocity). Mirrors Unity's own "Point"/"Circle"/"Cone"/
/// "Box" shapes exactly (minus Unity's own Sphere/Hemisphere/Mesh/
/// SpriteRenderer shapes, deferred — Point/Circle/Cone/Box alone already
/// cover radial bursts, directional cones, and rectangular area emission,
/// the three broad patterns almost every 2D effect needs).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum EmitterShape {
    /// Every particle starts at the emitter's own origin, moving in a
    /// uniformly random direction — a plain omnidirectional point burst
    /// (sparks, magic bursts).
    Point,
    /// Starts somewhere on a circle of this `radius` centered on the
    /// emitter, moving radially outward from the center — a flat ring/disc
    /// burst (ripples, shockwaves).
    Circle { radius: f32 },
    /// Starts at the emitter's own origin, moving within a cone of this
    /// half-`angle` (radians) around local `+y` ("up"), out to `radius`
    /// (`0.0` means every particle starts exactly at the origin, spreading
    /// only in direction) — directional sprays (fire, smoke, fountains).
    Cone { angle: f32, radius: f32 },
    /// Starts at a uniformly random point inside a `width` x `height`
    /// rectangle centered on the emitter, moving straight along local `+y`
    /// — rain/snow falling through a rectangular area, or a flat line of
    /// rising embers.
    Box { width: f32, height: f32 },
}

impl Default for EmitterShape {
    fn default() -> Self {
        Self::Cone { angle: 0.4, radius: 0.0 }
    }
}

/// Size over Lifetime module — a multiplier curve (Unity's own semantics
/// exactly: the sampled value multiplies `start_size`, it doesn't replace
/// it) sampled by linear interpolation between the two keyframes
/// straddling a particle's own normalized age (`age / start_lifetime`,
/// `0.0..=1.0`). An empty `Vec` (the default) means no curve at all —
/// every particle stays at its own fixed `start_size` for its whole life,
/// a freshly-added system's starting behavior (same "empty means off,
/// not an error" convention `color_over_lifetime` also follows).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SizeKeyframe {
    /// Normalized lifetime position, `0.0..=1.0`.
    pub t: f32,
    /// Multiplies `ParticleMain::start_size` — `1.0` is "unchanged."
    pub size_multiplier: f32,
}

/// Payload for `NodeKind::ParticleSystem` — see this module's own doc
/// comment for the overall scope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParticleSystemNode {
    pub main: ParticleMain,
    pub emission: ParticleEmission,
    pub shape: EmitterShape,
    /// Color over Lifetime module — multiplies `ParticleMain::start_color`
    /// by the gradient sampled at a particle's own normalized age, the
    /// same "stops at `offset: 0.0..=1.0`, linearly interpolated between
    /// the two straddling a sample point" shape `GradientDef::stops`
    /// already uses for fills — reused directly rather than inventing a
    /// second near-identical gradient type. Empty (the default) means no
    /// tint-over-lifetime at all: every particle keeps its own fixed
    /// `start_color` for its whole life (most effects still want at least
    /// a fade-to-transparent-alpha stop here, but an empty list is a real,
    /// intended "solid color the whole time" configuration too, e.g. a
    /// debris chunk that just falls and disappears abruptly).
    #[serde(default)]
    pub color_over_lifetime: Vec<crate::gradient::GradientStop>,
    /// Size over Lifetime module — see `SizeKeyframe`'s own doc comment.
    #[serde(default)]
    pub size_over_lifetime: Vec<SizeKeyframe>,
    /// Rotation over Lifetime module — a constant angular velocity
    /// (radians/second), not a full curve (Unity's own module supports a
    /// curve here too, but a constant spin rate already covers the
    /// overwhelming majority of real effects — tumbling debris, spinning
    /// sparks — without needing a third keyframe-list type for this pass).
    /// `0.0` (the default) means particles never rotate on their own after
    /// spawning (still subject to `start_rotation`'s own one-time authored
    /// spread).
    #[serde(default)]
    pub rotation_speed: f32,
}

impl Default for ParticleSystemNode {
    fn default() -> Self {
        Self {
            main: ParticleMain::default(),
            emission: ParticleEmission::default(),
            shape: EmitterShape::default(),
            color_over_lifetime: Vec::new(),
            size_over_lifetime: Vec::new(),
            rotation_speed: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_particle_system_node_round_trips_through_json() {
        let node = ParticleSystemNode {
            main: ParticleMain { gravity_modifier: 1.5, ..ParticleMain::default() },
            emission: ParticleEmission { rate_over_time: 20.0, bursts: vec![ParticleBurst { time: 0.0, count: 30, cycles: 1, interval: 0.0 }] },
            shape: EmitterShape::Circle { radius: 3.0 },
            color_over_lifetime: vec![
                crate::gradient::GradientStop { offset: 0.0, color: [1.0, 1.0, 1.0, 1.0], midpoint: 0.5 },
                crate::gradient::GradientStop { offset: 1.0, color: [1.0, 1.0, 1.0, 0.0], midpoint: 0.5 },
            ],
            size_over_lifetime: vec![SizeKeyframe { t: 0.0, size_multiplier: 0.2 }, SizeKeyframe { t: 1.0, size_multiplier: 1.0 }],
            rotation_speed: 1.0,
        };
        let json = serde_json::to_string(&node).expect("serialize");
        let round_tripped: ParticleSystemNode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(round_tripped, node);
    }

    #[test]
    fn a_particle_system_saved_before_the_curve_fields_existed_deserializes_with_them_empty() {
        let json = r#"{
            "main": {"duration":5.0,"looping":true,"start_lifetime":{"min":2.0,"max":2.0},"start_speed":{"min":2.0,"max":2.0},"start_size":{"min":0.5,"max":0.5},"start_rotation":{"min":0.0,"max":0.0},"start_color":[1.0,1.0,1.0,1.0],"gravity_modifier":0.0,"drag":0.0,"simulation_space":"Local","max_particles":100,"play_on_awake":true},
            "emission": {"rate_over_time":10.0,"bursts":[]},
            "shape": {"Cone":{"angle":0.4,"radius":0.0}}
        }"#;
        let node: ParticleSystemNode = serde_json::from_str(json).expect("deserialize");
        assert!(node.color_over_lifetime.is_empty());
        assert!(node.size_over_lifetime.is_empty());
        assert_eq!(node.rotation_speed, 0.0);
    }
}

/// What each particle of a 2D Particle System looks like (see
/// [`ParticleLook2D`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ParticleShape2D {
    #[default]
    Circle,
    Square,
    Triangle,
    Diamond,
    Star,
    Hexagon,
    Ring,
    /// A thin streak, always pointing along the particle's motion (sparks,
    /// rain, speed lines).
    Streak,
    /// An image asset (`ParticleLook2D::image`), as a sprite.
    Image,
    /// The outline of a Shape node in the scene (`ParticleLook2D::node`) —
    /// any custom shape (leaves, hearts, logos).
    Shape,
}

/// **Particle Look** — a 2D Particle System's per-particle look, on the
/// same node as the Particle System:
/// - `shape`: the particle's shape (circle by default), an image, or any
///   Shape node's outline; fitted to the particle's size.
/// - `align_to_velocity`: turn each particle to face where it's going
///   (Streak always does); `stretch` lengthens it with speed (motion
///   streaks; 0 = none).
/// - `blend_mode`: how particles composite (Normal, Multiply, Screen, Add —
///   Add for fire, magic and light). Image particles draw Normal.
/// - Glow: a soft halo round every particle, added on top as light —
///   `glow` strength (0 = off), `glow_size` (× the particle's size), in the
///   particle's own colour (`glow_tint`) or `glow_color`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ParticleLook2D {
    pub shape: ParticleShape2D,
    pub image: Option<crate::ImageId>,
    pub node: Option<crate::NodeId>,
    pub align_to_velocity: bool,
    pub stretch: f32,
    pub blend_mode: crate::BlendMode,
    pub glow: f32,
    pub glow_size: f32,
    pub glow_tint: bool,
    pub glow_color: [f32; 4],
}

impl Default for ParticleLook2D {
    fn default() -> Self {
        Self {
            shape: ParticleShape2D::Circle,
            image: None,
            node: None,
            align_to_velocity: false,
            stretch: 0.0,
            blend_mode: crate::BlendMode::Normal,
            glow: 0.0,
            glow_size: 2.5,
            glow_tint: true,
            glow_color: [1.0, 0.8, 0.4, 1.0],
        }
    }
}

/// **Particle Path** — particles flow along a path (any Shape node's
/// outline, e.g. a hidden guide curve): smoke trails, magic streams,
/// rivers of sparks, swarms. On the same node as the Particle System.
/// - `path`: the Shape node to follow.
/// - `weight` (0..1): how strongly particles are pulled onto the path
///   (1 = locked to it, low = a loose, drifting stream).
/// - `join_delay` / `join_time` (fractions of each particle's life): it
///   flies free for `join_delay`, then eases onto the path over
///   `join_time` — e.g. smoke puffs out, then drifts into the stream.
/// - `stray` (0..1): the share of particles that never join (wisps that
///   wander off).
/// - `spread` (px): how wide the stream is; `widen` scales it by the end
///   of the path (billowing smoke).
/// - `speed` (px/s along the path): 0 = each particle travels the path
///   (from `start`, 0..1) to its end over its lifetime.
/// - `turbulence` (px) / `turbulence_speed` (Hz): wobble on the stream.
/// - `loop_path`: wrap round to the start (closed paths, orbits).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ParticlePath2D {
    pub path: Option<crate::NodeId>,
    pub weight: f32,
    pub join_delay: f32,
    pub join_time: f32,
    pub stray: f32,
    pub spread: f32,
    pub widen: f32,
    pub speed: f32,
    pub start: f32,
    pub turbulence: f32,
    pub turbulence_speed: f32,
    pub loop_path: bool,
}

impl Default for ParticlePath2D {
    fn default() -> Self {
        Self { path: None, weight: 0.85, join_delay: 0.0, join_time: 0.25, stray: 0.1, spread: 12.0, widen: 2.5, speed: 0.0, start: 0.0, turbulence: 8.0, turbulence_speed: 1.5, loop_path: false }
    }
}
