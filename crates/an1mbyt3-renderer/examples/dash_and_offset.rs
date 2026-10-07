//! Phase 10 smoke test for the two `trim.rs` additions `trim_path.rs`'s
//! example doesn't cover: `dash_path` ("stroke dashing") and
//! `trim_path_with_offset` (Trim Paths' third parameter, "Offset").
//!
//! Two shapes, side by side, both stroked circles (approximated with a
//! `Path::rect`-style four-`CubicTo` circle so the effect is easy to read
//! visually — a dash pattern or a fixed-length rotating arc reads much more
//! clearly on a closed curve than on a rectangle's straight edges/corners):
//! - **Left**: a dashed ring whose `phase` animates continuously — the
//!   dashes visibly crawl around the circle, proving `dash_path`'s `phase`
//!   parameter actually shifts the pattern frame to frame, not just once.
//! - **Right**: a fixed-length arc (`trim_path_with_offset` with a constant
//!   `end - start` window) whose `offset` animates continuously — the
//!   visible arc spins around the circle, including wrapping from the end
//!   of the path back to the start (`trim_path_with_offset`'s doc comment's
//!   "two subpaths" case), which a plain `trim_path` animating `start`/`end`
//!   directly can't do without the visible window changing length as it
//!   approaches the wrap.
//!
//! Run with `cargo run -p an1mbyt3-renderer --example dash_and_offset`. See
//! `render_shape.rs`'s doc comment for the `WGPU_BACKEND` troubleshooting
//! note if this crashes immediately on Windows.

use std::sync::Arc;
use std::time::Instant;

use glam::Vec2;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use an1mbyt3_core::{Path, PathCommand, Transform2D};
use an1mbyt3_geometry::{dash_path, tessellate_stroke, trim_path_with_offset, StrokeCap, StrokeStyle};
use an1mbyt3_renderer::{BlendMode, Camera, DrawBinding, DrawCall, Renderer};

const ARTBOARD_WIDTH: f32 = 500.0;
const ARTBOARD_HEIGHT: f32 = 300.0;
const STROKE_WIDTH: f32 = 8.0;
const RADIUS: f32 = 90.0;
// A circle approximated by four cubic Béziers meeting the standard
// "0.5522847498 * radius" control-point-distance constant for a
// close-enough-for-rendering circular arc — same reasoning `build_wave_path`
// in `trim_path.rs` uses a hand-built `CubicTo` shape rather than reaching
// for a dedicated ellipse primitive this crate doesn't have yet.
const KAPPA: f32 = 0.5522847498;

fn build_circle_path(radius: f32) -> Path {
    let k = radius * KAPPA;
    Path {
        commands: vec![
            PathCommand::MoveTo(Vec2::new(radius, 0.0)),
            PathCommand::CubicTo { control1: Vec2::new(radius, k), control2: Vec2::new(k, radius), to: Vec2::new(0.0, radius) },
            PathCommand::CubicTo { control1: Vec2::new(-k, radius), control2: Vec2::new(-radius, k), to: Vec2::new(-radius, 0.0) },
            PathCommand::CubicTo { control1: Vec2::new(-radius, -k), control2: Vec2::new(-k, -radius), to: Vec2::new(0.0, -radius) },
            PathCommand::CubicTo { control1: Vec2::new(k, -radius), control2: Vec2::new(radius, -k), to: Vec2::new(radius, 0.0) },
            PathCommand::Close,
        ],
    }
}

struct App {
    smoke_test: bool,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    circle: Path,
    dash_binding: Option<DrawBinding>,
    offset_binding: Option<DrawBinding>,
    elapsed: f32,
    last_frame: Option<Instant>,
    rendered_one_frame: bool,
}

impl App {
    fn new(smoke_test: bool) -> Self {
        Self {
            smoke_test,
            window: None,
            renderer: None,
            circle: build_circle_path(RADIUS),
            dash_binding: None,
            offset_binding: None,
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
            .with_title("an1mbyt3 — Phase 10 dash_and_offset smoke test")
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

        self.dash_binding = Some(renderer.create_draw_binding());
        self.offset_binding = Some(renderer.create_draw_binding());
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
                let (Some(renderer), Some(dash_binding), Some(offset_binding)) =
                    (&self.renderer, &self.dash_binding, &self.offset_binding)
                else {
                    return;
                };

                let now = Instant::now();
                let delta = now.duration_since(self.last_frame.unwrap_or(now)).as_secs_f32();
                self.last_frame = Some(now);
                self.elapsed += delta;

                let style = StrokeStyle { width: STROKE_WIDTH, cap: StrokeCap::Round, ..Default::default() };

                // Left circle: dashes crawling around via an ever-increasing
                // `phase`, in the path's own local (pixel) units.
                let dash_phase = self.elapsed * 40.0; // px/sec crawl speed
                let dashed = dash_path(&self.circle, 24.0, 14.0, dash_phase);
                let dash_mesh = tessellate_stroke(&dashed, &style);

                // Right circle: a fixed-length (25% of the circumference)
                // visible arc spinning via an ever-increasing `offset` —
                // exercises the wraparound case once per revolution.
                let offset = (self.elapsed * 0.2) % 1.0;
                let arc = trim_path_with_offset(&self.circle, 0.0, 0.25, offset);
                let arc_mesh = tessellate_stroke(&arc, &style);

                let mut draws = Vec::new();
                let mut gpu_meshes = Vec::new();
                if !dash_mesh.positions.is_empty() {
                    gpu_meshes.push((renderer.upload_mesh(&dash_mesh), Transform2D::from_translation(Vec2::new(-120.0, 0.0)), dash_binding));
                }
                if !arc_mesh.positions.is_empty() {
                    gpu_meshes.push((renderer.upload_mesh(&arc_mesh), Transform2D::from_translation(Vec2::new(120.0, 0.0)), offset_binding));
                }
                for (mesh, transform, binding) in &gpu_meshes {
                    draws.push(DrawCall {
                        mesh,
                        binding,
                        transform: *transform,
                        fill_color: [0.85, 0.2, 0.35, 1.0],
                        blend_mode: BlendMode::Normal, gradient: None, texture: None,
                        material: None,
                    });
                }
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
                    Err(err) => eprintln!("an1mbyt3 dash_and_offset: render failed: {err:?}"),
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
