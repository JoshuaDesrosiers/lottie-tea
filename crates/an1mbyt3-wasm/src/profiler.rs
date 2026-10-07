//! Per-frame profiling for `WasmEditor::render` — where a frame's CPU time
//! goes, plus how big the scene is. Read by the editor's / player's F3
//! overlay through `WasmEditor::frame_stats`, and by stress tests.
//!
//! Timings are wall-clock milliseconds from `performance.now()` (sub-ms
//! resolution in browsers). `gpu_submit_ms` is the CPU side of drawing
//! (encoding + submitting command buffers); the GPU's own execution time
//! is asynchronous and not measured here.

use serde::Serialize;

/// Milliseconds since page load (0 when no `window`, e.g. tests).
pub(crate) fn now_ms() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map_or(0.0, |p| p.now())
}

/// One frame's breakdown. Stages run in this order inside `render`.
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameStats {
    /// State machine advance + apply (animations, Graph/Blocks programs).
    pub logic_ms: f64,
    /// IK constraints.
    pub ik_ms: f64,
    /// Physics sync + step + write-back.
    pub physics_ms: f64,
    /// Per-node GPU cache upkeep: meshes, text, images, particles (incl.
    /// their simulation), trails, skinning.
    pub prep_ms: f64,
    /// Building the frame's draw list + lights + shadow casters.
    pub scene_ms: f64,
    /// Encoding + submitting the frame to the GPU.
    pub gpu_submit_ms: f64,
    /// Everything inside `render`.
    pub total_ms: f64,
    /// Live (non-deleted) nodes in the scene.
    pub nodes: u32,
    /// Draw calls this frame (masked + unmasked + unlit overlay).
    pub draws: u32,
    /// Local (non-global) lights drawn.
    pub lights: u32,
    /// Bodies in the physics world (0 outside Playground).
    pub bodies: u32,
    /// Particles alive across every Particle System.
    pub particles: u32,
}

/// Running stopwatch for one frame: `lap()` returns ms since the last lap.
pub(crate) struct Stopwatch {
    start: f64,
    last: f64,
}

impl Stopwatch {
    pub(crate) fn start() -> Self {
        let t = now_ms();
        Self { start: t, last: t }
    }

    pub(crate) fn lap(&mut self) -> f64 {
        let t = now_ms();
        let dt = t - self.last;
        self.last = t;
        dt
    }

    pub(crate) fn total(&self) -> f64 {
        now_ms() - self.start
    }
}
