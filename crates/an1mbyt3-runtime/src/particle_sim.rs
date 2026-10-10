//! Live simulation for one `NodeKind::ParticleSystem` — advances its own
//! pool of currently-alive particles one frame at a time and spawns new
//! ones per its authored `ParticleEmission`/`EmitterShape`. See
//! `an1mbyt3_core::particle`'s own module doc for the overall feature scope.
//!
//! **Wired into rendering** via `an1mbyt3_wasm::editor::WasmEditor::render`'s
//! own particle pre-pass: that's the one and only caller of `step` (once
//! per `NodeKind::ParticleSystem` node per frame), and `node_draws`' own
//! `ParticleSystem` arm turns the resulting `Particle`s into real
//! `DrawCall`s (a shared unit-circle `GpuMesh`, one grown-as-needed
//! `DrawBinding` per live particle). A `ParticleSystemRuntime` is still pure
//! runtime state, never saved to the document — same "ephemeral" category
//! as a Blocks program's own `ScriptRuntime` (`block_interpreter.rs`) or a
//! clone's live `NodeId` (`Artboard::spawn_clone_of`) — rebuilt fresh
//! (`ParticleSystemRuntime::default`) whenever `WasmEditor` itself does.
//!
//! **Simulation space** (`ParticleMain::simulation_space`, the Inspector's
//! own toggle): `Local` (the default) keeps every particle's own `position`/
//! `velocity`/`rotation` in the emitter's local space for its whole life —
//! `node_draws`'s own render-side composes that with the emitter's *current*
//! world transform fresh every frame, so already-alive particles visibly
//! drift along if the emitter itself moves or rotates afterward. `World`
//! instead bakes each particle's spawn position/direction through the
//! emitter's world transform *once*, at `spawn` (see that function's own
//! `emitter_world` parameter) — from that point on it's a plain world-space
//! point no longer tied to the emitter at all, so it stays exactly where it
//! was fired even if the emitter moves away afterward (a trail of footprints
//! behind a walking character is the canonical use case; a torch's sparks
//! drifting *with* the torch is `Local`'s). `node_draws` reads this same
//! field to know which of its own two composition paths to take for a given
//! system's particles.

use an1mbyt3_core::{SimulationSpace, Transform2D};
use glam::Vec2;

use an1mbyt3_core::{EmitterShape, ParticleRange, ParticleSystemNode};

use crate::block_interpreter::next_random_unit;

/// One currently-alive particle — everything the (future) renderer needs to
/// draw it, plus `age`/`lifetime` to know when it dies and to sample
/// `color_over_lifetime`/`size_over_lifetime` at its own current point in
/// its life.
#[derive(Debug, Clone, Copy)]
pub struct Particle {
    pub position: Vec2,
    pub velocity: Vec2,
    pub rotation: f32,
    pub age: f32,
    pub lifetime: f32,
    /// This particle's own randomly-drawn `start_size` (from
    /// `ParticleMain::start_size`'s range) — `size_over_lifetime`'s sampled
    /// multiplier (if any) applies on top of this per-particle base, not a
    /// shared system-wide constant, matching Unity's own per-particle draw.
    pub base_size: f32,
}

impl Particle {
    /// `age / lifetime`, clamped to `0.0..=1.0` — what `color_over_lifetime`/
    /// `size_over_lifetime` sample against.
    pub fn normalized_age(&self) -> f32 {
        if self.lifetime <= 0.0 { 1.0 } else { (self.age / self.lifetime).clamp(0.0, 1.0) }
    }
}

/// Live state for one `NodeKind::ParticleSystem` node — see this module's
/// own doc comment for why this is never saved to the document.
#[derive(Debug, Clone, Default)]
pub struct ParticleSystemRuntime {
    particles: Vec<Particle>,
    /// Fractional particles "owed" by continuous emission, carried across
    /// frames so a low `rate_over_time` (e.g. 0.5/sec) still spawns at the
    /// right long-run average instead of only ever emitting on whole-second
    /// boundaries.
    emission_accumulator: f32,
    /// Seconds into the current loop cycle — wraps back to `0.0` (minus
    /// `main.duration`, not a hard reset, so a frame that overshoots the
    /// boundary doesn't lose that overshoot) every `main.duration` seconds
    /// while `main.looping`; for a non-looping system, reaching
    /// `main.duration` here sets `stopped` instead of wrapping.
    cycle_time: f32,
    /// How many times each of `emission.bursts` has fired so far this
    /// cycle — index-aligned with `ParticleEmission::bursts`, resized (and
    /// zeroed) on mismatch the same "author edited the config while it was
    /// running" resync `block_interpreter::step` already does for its own
    /// script pools.
    burst_fired: Vec<u32>,
    /// `true` once a non-looping system's `main.duration` has elapsed —
    /// stops *emitting* new particles; already-alive ones keep simulating
    /// to the end of their own `lifetime` regardless.
    stopped: bool,
    rng_state: u32,
    /// Particles requested by [`ParticleSystemRuntime::emit`] that the next
    /// `step` will spawn — at the emitter's position *at that step*, so a
    /// caller can move the emitter and then emit in the same frame.
    pending_emit: u32,
}

impl ParticleSystemRuntime {
    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }

    /// The live particles, for effects layered on the simulation (2D
    /// particle behaviours: converge / attract / repel / drag).
    pub fn particles_mut(&mut self) -> &mut [Particle] {
        &mut self.particles
    }

    /// Unity's `ParticleSystem.Emit(count)`: spawn `count` particles on the
    /// next step, on top of (and without disturbing) whatever is already
    /// alive and whatever the emission module is doing. Works even on a
    /// stopped/non-looping system, which is how one-shot effects (hit
    /// sparks, pickup glitter) are usually driven: rate 0, no bursts, and a
    /// script calling `emit` when the moment happens.
    pub fn emit(&mut self, count: u32) {
        self.pending_emit = self.pending_emit.saturating_add(count);
    }
}

fn sample_range(range: ParticleRange, rng_state: &mut u32) -> f32 {
    if range.max <= range.min {
        return range.min;
    }
    let t = next_random_unit(rng_state, 0);
    range.min + (range.max - range.min) * t
}

/// Draws this spawn's own starting local position + initial unit-length
/// direction (multiplied by `start_speed` to get a velocity) from `shape` —
/// see `EmitterShape`'s own doc comment for each variant's exact geometry.
fn sample_shape(shape: &EmitterShape, rng_state: &mut u32) -> (Vec2, Vec2) {
    match *shape {
        EmitterShape::Point => {
            let angle = next_random_unit(rng_state, 0) * std::f32::consts::TAU;
            (Vec2::ZERO, Vec2::new(angle.cos(), angle.sin()))
        }
        EmitterShape::Circle { radius } => {
            let angle = next_random_unit(rng_state, 0) * std::f32::consts::TAU;
            let direction = Vec2::new(angle.cos(), angle.sin());
            (direction * radius, direction)
        }
        EmitterShape::Cone { angle, radius } => {
            let spread = (next_random_unit(rng_state, 0) * 2.0 - 1.0) * angle;
            let direction = Vec2::new(spread.sin(), spread.cos());
            let r = if radius > 0.0 { next_random_unit(rng_state, 0).sqrt() * radius } else { 0.0 };
            let spawn_angle = next_random_unit(rng_state, 0) * std::f32::consts::TAU;
            (Vec2::new(spawn_angle.cos(), spawn_angle.sin()) * r, direction)
        }
        EmitterShape::Box { width, height } => {
            let x = (next_random_unit(rng_state, 0) - 0.5) * width;
            let y = (next_random_unit(rng_state, 0) - 0.5) * height;
            (Vec2::new(x, y), Vec2::new(0.0, 1.0))
        }
    }
}

/// `emitter_world` is only actually used for `SimulationSpace::World` (see
/// this module's own doc comment) — baking the freshly-sampled local spawn
/// position/direction through it *once*, here, turning both into plain
/// world-space values the particle keeps for the rest of its life,
/// independent of the emitter from this point on. `Local` ignores it
/// entirely, same "stay in local space, let the renderer compose fresh
/// every frame" behavior this already had.
fn spawn(config: &ParticleSystemNode, runtime: &mut ParticleSystemRuntime, emitter_world: Transform2D) {
    let lifetime = sample_range(config.main.start_lifetime, &mut runtime.rng_state).max(0.0001);
    let speed = sample_range(config.main.start_speed, &mut runtime.rng_state);
    let base_size = sample_range(config.main.start_size, &mut runtime.rng_state);
    let rotation = sample_range(config.main.start_rotation, &mut runtime.rng_state);
    let (local_position, local_direction) = sample_shape(&config.shape, &mut runtime.rng_state);

    let (position, direction, rotation) = match config.main.simulation_space {
        SimulationSpace::Local => (local_position, local_direction, rotation),
        SimulationSpace::World => {
            let (sin, cos) = emitter_world.rotation.sin_cos();
            let rotate = |v: Vec2| Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos);
            let world_position = emitter_world.translation + rotate(local_position * emitter_world.scale);
            // A direction is a unit vector, not a point — only rotates with
            // the emitter, never translates or scales with it.
            let world_direction = rotate(local_direction);
            (world_position, world_direction, rotation + emitter_world.rotation)
        }
    };
    runtime.particles.push(Particle { position, velocity: direction * speed, rotation, age: 0.0, lifetime, base_size });
}

/// Advances `runtime` by `delta_seconds` against `config` — ages and moves
/// every currently-alive particle (culling any whose `age` has passed its
/// own `lifetime`), then emits new ones per `config.emission`'s continuous
/// rate and any bursts due this frame. A no-op for `delta_seconds <= 0.0`
/// (a paused/stalled frame), same "zero or negative delta changes nothing"
/// convention `Playback::advance` already follows. `emitter_world` is the
/// emitter node's own *current* world transform — passed straight through
/// to `spawn` for `SimulationSpace::World` (see this module's own doc
/// comment); already-alive particles never need it here regardless of
/// space, since this function only ever moves them by their own stored
/// `velocity`, never re-touches `position` against the emitter directly.
pub fn step(config: &ParticleSystemNode, runtime: &mut ParticleSystemRuntime, delta_seconds: f32, emitter_world: Transform2D) {
    if delta_seconds <= 0.0 {
        return;
    }

    for particle in &mut runtime.particles {
        particle.age += delta_seconds;
        particle.velocity.y -= config.main.gravity_modifier * delta_seconds;
        if config.main.drag > 0.0 {
            let retained = (1.0 - config.main.drag * delta_seconds).max(0.0);
            particle.velocity *= retained;
        }
        particle.position += particle.velocity * delta_seconds;
        particle.rotation += config.rotation_speed * delta_seconds;
    }
    runtime.particles.retain(|p| p.age < p.lifetime);

    // Script-requested one-shot emission (`ParticleSystemRuntime::emit`).
    let requested = std::mem::take(&mut runtime.pending_emit);
    for _ in 0..requested {
        if runtime.particles.len() >= config.main.max_particles as usize {
            break;
        }
        spawn(config, runtime, emitter_world);
    }

    if runtime.stopped {
        return;
    }

    runtime.cycle_time += delta_seconds;
    if config.main.duration > 0.0 && runtime.cycle_time >= config.main.duration {
        if config.main.looping {
            runtime.cycle_time -= config.main.duration;
            runtime.burst_fired.iter_mut().for_each(|count| *count = 0);
        } else {
            runtime.stopped = true;
            return;
        }
    }

    let max_particles = config.main.max_particles as usize;

    if config.emission.rate_over_time > 0.0 {
        runtime.emission_accumulator += config.emission.rate_over_time * delta_seconds;
        while runtime.emission_accumulator >= 1.0 && runtime.particles.len() < max_particles {
            runtime.emission_accumulator -= 1.0;
            spawn(config, runtime, emitter_world);
        }
    }

    if runtime.burst_fired.len() != config.emission.bursts.len() {
        runtime.burst_fired = vec![0; config.emission.bursts.len()];
    }
    for (index, burst) in config.emission.bursts.iter().enumerate() {
        if burst.cycles == 0 {
            continue;
        }
        let fired = runtime.burst_fired[index];
        if fired >= burst.cycles {
            continue;
        }
        let interval = burst.interval.max(0.0001);
        let next_fire_time = burst.time + fired as f32 * interval;
        if runtime.cycle_time >= next_fire_time {
            for _ in 0..burst.count {
                if runtime.particles.len() >= max_particles {
                    break;
                }
                spawn(config, runtime, emitter_world);
            }
            runtime.burst_fired[index] += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use an1mbyt3_core::{ParticleBurst, ParticleEmission, ParticleMain};

    #[test]
    fn emit_spawns_exactly_the_requested_count_even_when_stopped() {
        let mut config = ParticleSystemNode::default();
        config.main.looping = false;
        config.main.duration = 0.1;
        config.main.start_lifetime = ParticleRange::fixed(10.0);
        config.emission.rate_over_time = 0.0;
        config.emission.bursts.clear();
        let mut runtime = ParticleSystemRuntime::default();
        step(&config, &mut runtime, 0.2, Transform2D::default());
        assert_eq!(runtime.particles().len(), 0);
        runtime.emit(7);
        let at = Transform2D { translation: Vec2::new(40.0, 0.0), ..Default::default() };
        step(&config, &mut runtime, 0.016, at);
        assert_eq!(runtime.particles().len(), 7);
        if config.main.simulation_space == SimulationSpace::World {
            assert!(runtime.particles().iter().all(|p| (p.position.x - 40.0).abs() < 50.0));
        }
    }

    fn config_with(main: ParticleMain, emission: ParticleEmission, shape: EmitterShape) -> ParticleSystemNode {
        ParticleSystemNode { main, emission, shape, color_over_lifetime: Vec::new(), size_over_lifetime: Vec::new(), rotation_speed: 0.0 }
    }

    #[test]
    fn continuous_emission_spawns_at_the_authored_rate_over_several_seconds() {
        // `start_lifetime` overridden to comfortably outlast the whole test
        // window — otherwise this is measuring the steady-state live
        // population (`rate * lifetime`, a real and correct queueing
        // identity once particles start dying off mid-test), not the total
        // number ever spawned, which is what this test actually wants to
        // check.
        let config = config_with(
            ParticleMain { start_lifetime: an1mbyt3_core::ParticleRange::fixed(100.0), max_particles: 1000, ..ParticleMain::default() },
            ParticleEmission { rate_over_time: 10.0, bursts: Vec::new() },
            EmitterShape::Point,
        );
        let mut runtime = ParticleSystemRuntime::default();
        for _ in 0..300 {
            step(&config, &mut runtime, 0.01, Transform2D::default()); // 3 seconds total, 0.01s steps
        }
        // 10/sec * 3s = 30, within rounding of the fractional accumulator.
        assert!((29..=31).contains(&runtime.particles().len()), "expected ~30 particles, got {}", runtime.particles().len());
    }

    #[test]
    fn a_particle_dies_exactly_when_its_own_randomly_drawn_lifetime_elapses() {
        // A particle spawned mid-step (during this step's own emission
        // pass, which runs *after* the aging pass) starts the *next* call
        // with `age: 0.0` — its age only starts accumulating from there, not
        // from the instant it was spawned within its birth frame. This test
        // uses a tiny first step (so the burst fires with negligible aging
        // "lost" inside that frame) and then tracks age precisely from the
        // second step onward.
        let config = config_with(
            ParticleMain { start_lifetime: an1mbyt3_core::ParticleRange::fixed(1.0), max_particles: 10, ..ParticleMain::default() },
            ParticleEmission { rate_over_time: 0.0, bursts: vec![ParticleBurst { time: 0.0, count: 1, cycles: 1, interval: 0.0 }] },
            EmitterShape::Point,
        );
        let mut runtime = ParticleSystemRuntime::default();
        step(&config, &mut runtime, 0.01, Transform2D::default());
        assert_eq!(runtime.particles().len(), 1, "the burst should have fired");
        step(&config, &mut runtime, 0.98, Transform2D::default());
        assert_eq!(runtime.particles().len(), 1, "still alive at age 0.98 of a 1.0s lifetime");
        step(&config, &mut runtime, 0.03, Transform2D::default());
        assert_eq!(runtime.particles().len(), 0, "dead by age 1.01 of a 1.0s lifetime");
    }

    #[test]
    fn gravity_pulls_a_particle_downward_over_time() {
        let config = config_with(
            ParticleMain { gravity_modifier: 5.0, start_speed: an1mbyt3_core::ParticleRange::fixed(0.0), max_particles: 10, ..ParticleMain::default() },
            ParticleEmission { rate_over_time: 0.0, bursts: vec![ParticleBurst { time: 0.0, count: 1, cycles: 1, interval: 0.0 }] },
            EmitterShape::Point,
        );
        let mut runtime = ParticleSystemRuntime::default();
        step(&config, &mut runtime, 0.1, Transform2D::default());
        let before_y = runtime.particles()[0].position.y;
        for _ in 0..10 {
            step(&config, &mut runtime, 0.1, Transform2D::default());
        }
        assert!(runtime.particles()[0].position.y < before_y, "gravity should have pulled the particle downward");
    }

    #[test]
    fn a_burst_fires_exactly_once_per_cycle_even_across_many_small_steps() {
        let config = config_with(
            ParticleMain { max_particles: 100, ..ParticleMain::default() },
            ParticleEmission { rate_over_time: 0.0, bursts: vec![ParticleBurst { time: 0.5, count: 20, cycles: 1, interval: 0.0 }] },
            EmitterShape::Point,
        );
        let mut runtime = ParticleSystemRuntime::default();
        for _ in 0..100 {
            step(&config, &mut runtime, 0.01, Transform2D::default()); // 1 second total
        }
        assert_eq!(runtime.particles().len(), 20);
    }

    #[test]
    fn a_non_looping_system_stops_emitting_after_duration_but_existing_particles_keep_simulating() {
        let config = config_with(
            ParticleMain { looping: false, duration: 1.0, start_lifetime: an1mbyt3_core::ParticleRange::fixed(5.0), max_particles: 1000, ..ParticleMain::default() },
            ParticleEmission { rate_over_time: 10.0, bursts: Vec::new() },
            EmitterShape::Point,
        );
        let mut runtime = ParticleSystemRuntime::default();
        for _ in 0..100 {
            step(&config, &mut runtime, 0.01, Transform2D::default()); // 1 second — right at the duration boundary
        }
        let count_at_duration = runtime.particles().len();
        assert!(count_at_duration > 0);
        for _ in 0..100 {
            step(&config, &mut runtime, 0.01, Transform2D::default()); // another second — should emit nothing new
        }
        assert_eq!(runtime.particles().len(), count_at_duration, "no new particles after a non-looping system's duration elapses");
    }

    #[test]
    fn max_particles_caps_the_live_pool_even_with_a_high_emission_rate() {
        let config = config_with(
            ParticleMain { max_particles: 5, ..ParticleMain::default() },
            ParticleEmission { rate_over_time: 1000.0, bursts: Vec::new() },
            EmitterShape::Point,
        );
        let mut runtime = ParticleSystemRuntime::default();
        step(&config, &mut runtime, 1.0, Transform2D::default());
        assert_eq!(runtime.particles().len(), 5);
    }

    #[test]
    fn world_space_bakes_the_emitters_transform_in_at_spawn_and_ignores_it_afterward() {
        let config = config_with(
            ParticleMain {
                simulation_space: an1mbyt3_core::SimulationSpace::World,
                start_speed: an1mbyt3_core::ParticleRange::fixed(0.0),
                max_particles: 10,
                ..ParticleMain::default()
            },
            ParticleEmission { rate_over_time: 0.0, bursts: vec![ParticleBurst { time: 0.0, count: 1, cycles: 1, interval: 0.0 }] },
            EmitterShape::Point,
        );
        let mut runtime = ParticleSystemRuntime::default();
        let emitter_at_spawn = Transform2D { translation: Vec2::new(100.0, 50.0), ..Transform2D::default() };
        step(&config, &mut runtime, 0.01, emitter_at_spawn);
        assert_eq!(runtime.particles().len(), 1, "the burst should have fired");
        let spawned_position = runtime.particles()[0].position;
        assert!((spawned_position - emitter_at_spawn.translation).length() < 1e-4, "should spawn baked at the emitter's position, got {spawned_position:?}");

        // The emitter then moves far away — an already-alive World-space
        // particle must *not* follow it (that's exactly `Local`'s own
        // behavior instead).
        let emitter_moved_away = Transform2D { translation: Vec2::new(-500.0, -500.0), ..Transform2D::default() };
        step(&config, &mut runtime, 0.01, emitter_moved_away);
        assert!((runtime.particles()[0].position - spawned_position).length() < 1e-4, "an already-spawned World-space particle must stay where it was, not follow the emitter");
    }
}
