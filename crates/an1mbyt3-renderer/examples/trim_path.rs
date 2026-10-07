//! Phase 10 smoke test: animated trim paths on more than one kind of
//! geometry — a stroked rectangle (straight segments) and a stroked S-curve
//! (cubic Bézier segments), each with its visible arc-length range growing
//! and shrinking over time via `an1mbyt3_geometry::trim_path` +
//! `tessellate_stroke` (AN1MBYT3_BUILD_PLAN.md §10, Phase 10 exit criterion:
//! "a stroked path's visible extent animates via start/end/offset
//! percentages"). Two different shapes, not one, on purpose — proving
//! `trim_path`'s arc-length cropping handles curved segments (whose arc
//! length has no closed form and must be numerically approximated) exactly
//! as generically as straight ones, since it operates on any
//! `an1mbyt3_core::Path` without caring what commands built it.
//!
//! Unlike every earlier example, this one re-tessellates AND re-uploads a
//! brand new `GpuMesh` every frame per shape instead of reusing one via
//! `update_mesh_positions` — a trimmed stroke's vertex *count* changes every
//! frame (more of the outline visible = more geometry), unlike skinning
//! (Phase 5), which only ever moves the same fixed set of vertices. This is
//! the same "full re-tessellation, caching is the caller's job" tradeoff
//! `tessellate_path`'s own doc comment already describes — fine for this
//! demo's two simple shapes; a real authoring tool would cache and only
//! re-trim when the trim percentages actually change, but that caching isn't
//! this example's job to prove.
//!
//! Run with `cargo run -p an1mbyt3-renderer --example trim_path`. See
//! `render_shape.rs`'s doc comment for the `WGPU_BACKEND` troubleshooting
//! note if this crashes immediately on Windows.

use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use glam::Vec2;
use an1mbyt3_core::{Path, PathCommand, Transform2D};
use an1mbyt3_geometry::{tessellate_stroke, trim_path, StrokeCap, StrokeStyle};
use an1mbyt3_renderer::{BlendMode, Camera, DrawBinding, DrawCall, Renderer};
use an1mbyt3_runtime::{Easing, Keyframe, Track};

const ARTBOARD_WIDTH: f32 = 700.0;
const ARTBOARD_HEIGHT: f32 = 350.0;
const STROKE_WIDTH: f32 = 10.0;

/// The trim "end" percentage animates 0 -> 1 -> 0 over a 2.4s hand-authored
/// track (down-then-up, rather than reaching for `Playback`'s `PingPong` loop
/// mode for a single scalar track outside any `AnimationClip`/`Document`).
fn build_trim_end_track() -> Track<f32> {
    let ease = Easing::CubicBezier { x1: 0.42, y1: 0.0, x2: 0.58, y2: 1.0 };
    Track::new(vec![
        Keyframe::new(0.0, 0.0, ease),
        Keyframe::new(1.2, 1.0, ease),
        Keyframe::new(2.4, 0.0, ease),
    ])
}

/// A two-hump S-curve built entirely from `CubicTo` segments — the "other
/// than a rectangle" shape this example trims, specifically because it has
/// no straight segments at all, unlike `Path::rect`.
fn build_wave_path() -> Path {
    Path {
        commands: vec![
            PathCommand::MoveTo(Vec2::new(-90.0, 0.0)),
            PathCommand::CubicTo {
                control1: Vec2::new(-60.0, -70.0),
                control2: Vec2::new(-30.0, 70.0),
                to: Vec2::new(0.0, 0.0),
            },
            PathCommand::CubicTo {
                control1: Vec2::new(30.0, -70.0),
                control2: Vec2::new(60.0, 70.0),
                to: Vec2::new(90.0, 0.0),
            },
        ],
    }
}

struct App {
    smoke_test: bool,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    /// Each shape's untrimmed base path, where it sits on the artboard, and
    /// its own [`DrawBinding`] (`None` until `resumed()` creates it) — a
    /// second, independent binding per shape (not one binding reused across
    /// both) because two shapes drawing simultaneously each need their own
    /// uniform slot, exactly the sharing bug `an1mbyt3-renderer`'s
    /// `GpuMesh`/`DrawBinding` split (Phase 7) exists to prevent.
    shapes: Vec<(Path, Transform2D, Option<DrawBinding>)>,
    trim_end_track: Track<f32>,
    elapsed: f32,
    last_frame: Option<Instant>,
    rendered_one_frame: bool,
}

impl App {
    fn new(smoke_test: bool) -> Self {
        let rect_transform = Transform2D { translation: Vec2::new(-180.0, 0.0), ..Transform2D::default() };
        let wave_transform = Transform2D { translation: Vec2::new(180.0, 0.0), ..Transform2D::default() };

        Self {
            smoke_test,
            window: None,
            renderer: None,
            shapes: vec![
                (Path::rect(160.0, 160.0), rect_transform, None),
                (build_wave_path(), wave_transform, None),
            ],
            trim_end_track: build_trim_end_track(),
            elapsed: 0.0,
            last_frame: None,
            rendered_one_frame: false,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attributes = Window::default_attributes()
            .with_title("an1mbyt3 — Phase 10 trim_path smoke test")
            .with_inner_size(winit::dpi::LogicalSize::new(ARTBOARD_WIDTH, ARTBOARD_HEIGHT));
        let window = Arc::new(
            event_loop
                .create_window(window_attributes)
                .expect("failed to create window"),
        );

        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance
            .create_surface(window.clone())
            .expect("failed to create wgpu surface for window");

        let renderer = pollster::block_on(Renderer::new(&instance, surface, size.width, size.height));

        for (_, _, draw_binding) in &mut self.shapes {
            *draw_binding = Some(renderer.create_draw_binding());
        }

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.last_frame = Some(Instant::now());
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                let Some(renderer) = &self.renderer else { return };

                let now = Instant::now();
                let delta = now.duration_since(self.last_frame.unwrap_or(now)).as_secs_f32();
                self.last_frame = Some(now);
                self.elapsed += delta;
                // Track itself doesn't loop (that's `Playback`'s job) — loop
                // the 2.4s authored track manually via modulo instead of
                // pulling in a full `Playback` for one scalar.
                let track_time = self.elapsed % self.trim_end_track.duration().max(0.001);
                let trim_end = self.trim_end_track.sample(track_time);

                // Each shape's trimmed mesh is built fresh this frame, then
                // paired with its own `DrawBinding` — collected into
                // `gpu_meshes` first so their borrows outlive the `draws`
                // Vec built from them below.
                let gpu_meshes: Vec<_> = self
                    .shapes
                    .iter()
                    .filter_map(|(base_path, transform, draw_binding)| {
                        let draw_binding = draw_binding.as_ref()?;
                        let trimmed = trim_path(base_path, 0.0, trim_end);
                        // Round caps on the trimmed ends -- the common
                        // "loading spinner" trim-path look, and a live check
                        // that `StrokeStyle::cap` actually reaches the
                        // tessellator (a `Butt`-capped trim would show flat,
                        // not rounded, ends).
                        let style = StrokeStyle { width: STROKE_WIDTH, cap: StrokeCap::Round, ..Default::default() };
                        let mesh = tessellate_stroke(&trimmed, &style);
                        // A trim range of zero length (both ends of the
                        // authored track sit at 0.0) produces a genuinely
                        // empty stroke mesh — correct behavior (nothing to
                        // draw), but `wgpu` panics slicing a zero-size
                        // vertex buffer, so an empty mesh is filtered out
                        // here rather than uploaded.
                        if mesh.positions.is_empty() {
                            return None;
                        }
                        Some((renderer.upload_mesh(&mesh), *transform, draw_binding))
                    })
                    .collect();

                let draws: Vec<DrawCall> = gpu_meshes
                    .iter()
                    .map(|(gpu_mesh, transform, draw_binding)| DrawCall {
                        mesh: gpu_mesh,
                        binding: draw_binding,
                        transform: *transform,
                        fill_color: [0.85, 0.2, 0.35, 1.0],
                        blend_mode: BlendMode::Normal, gradient: None, texture: None,
                        material: None,
                    })
                    .collect();

                match renderer.render(
                    (ARTBOARD_WIDTH, ARTBOARD_HEIGHT),
                    Camera::default(),
                    wgpu::Color { r: 0.08, g: 0.08, b: 0.1, a: 1.0 },
                    &draws,
                ) {
                    Ok(()) => {
                        self.rendered_one_frame = true;
                        if self.smoke_test {
                            event_loop.exit();
                        }
                    }
                    Err(err) => eprintln!("an1mbyt3 trim_path: render failed: {err:?}"),
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        } else {
            let _ = event_loop;
        }
    }
}

fn main() {
    let smoke_test = std::env::var("AN1MBYT3_SMOKE_TEST").as_deref() == Ok("1");

    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(smoke_test);
    event_loop.run_app(&mut app).expect("event loop exited with an error");

    if smoke_test && !app.rendered_one_frame {
        panic!("smoke test finished without ever rendering a frame");
    }
}
