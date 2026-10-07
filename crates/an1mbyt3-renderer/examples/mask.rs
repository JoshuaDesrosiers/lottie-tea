//! Phase 10 smoke test: a mask clipping a group's contents
//! (AN1MBYT3_BUILD_PLAN.md §6's "masks... dedicated shader passes /
//! stencil-buffer techniques", Phase 10 exit criterion: "a mask layer clips
//! a group's contents"). A circular mask sits in the center of the artboard;
//! three overlapping colored rectangles slide back and forth behind it, only
//! ever visible through the circle — proof that `Renderer::render_with_mask`
//! (the stencil-buffer two-pass technique described in that method's doc
//! comment) actually clips, and keeps clipping correctly frame to frame as
//! the masked content moves, not just in one static screenshot.
//!
//! Run with `cargo run -p an1mbyt3-renderer --example mask`. See
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
use an1mbyt3_geometry::tessellate_path;
use an1mbyt3_renderer::{BlendMode, Camera, DrawBinding, DrawCall, GpuMesh, Renderer};

const ARTBOARD_WIDTH: f32 = 400.0;
const ARTBOARD_HEIGHT: f32 = 400.0;
const MASK_RADIUS: f32 = 120.0;
const CIRCLE_SEGMENTS: usize = 48;

/// A regular `segments`-gon approximating a circle — `an1mbyt3_core::Path`
/// has no dedicated ellipse/circle constructor yet, and a mask shape's exact
/// curve smoothness doesn't matter for proving the clipping technique works.
fn build_circle_path(radius: f32, segments: usize) -> Path {
    let mut commands = Vec::with_capacity(segments + 1);
    for i in 0..segments {
        let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
        let point = Vec2::new(angle.cos() * radius, angle.sin() * radius);
        commands.push(if i == 0 { PathCommand::MoveTo(point) } else { PathCommand::LineTo(point) });
    }
    commands.push(PathCommand::Close);
    Path { commands }
}

struct App {
    smoke_test: bool,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    mask_mesh: Option<GpuMesh>,
    mask_binding: Option<DrawBinding>,
    content_meshes: Vec<GpuMesh>,
    content_bindings: Vec<DrawBinding>,
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
            mask_mesh: None,
            mask_binding: None,
            content_meshes: Vec::new(),
            content_bindings: Vec::new(),
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
            .with_title("an1mbyt3 — Phase 10 mask smoke test")
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

        let mask_mesh_data = tessellate_path(&build_circle_path(MASK_RADIUS, CIRCLE_SEGMENTS));
        self.mask_mesh = Some(renderer.upload_mesh(&mask_mesh_data));
        self.mask_binding = Some(renderer.create_draw_binding());

        // Three overlapping stripes, wider than the mask circle, so every
        // edge visible on screen is the mask's circular edge, never a
        // stripe's own rectangular edge — the clearest possible proof the
        // circle (not the rectangles) is what's actually clipping.
        let stripe_mesh_data = tessellate_path(&Path::rect(320.0, 90.0));
        for _ in 0..3 {
            self.content_meshes.push(renderer.upload_mesh(&stripe_mesh_data));
            self.content_bindings.push(renderer.create_draw_binding());
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
                let (Some(mask_mesh), Some(mask_binding)) = (&self.mask_mesh, &self.mask_binding) else {
                    return;
                };

                let now = Instant::now();
                let delta = now.duration_since(self.last_frame.unwrap_or(now)).as_secs_f32();
                self.last_frame = Some(now);
                self.elapsed += delta;

                let mask = DrawCall {
                    mesh: mask_mesh,
                    binding: mask_binding,
                    transform: Transform2D::default(),
                    fill_color: [1.0, 1.0, 1.0, 1.0],
                    blend_mode: BlendMode::Normal, gradient: None, texture: None,
                    material: None,
                };

                let stripe_colors = [[0.9, 0.25, 0.3, 1.0], [0.25, 0.8, 0.4, 1.0], [0.3, 0.45, 0.95, 1.0]];
                let content: Vec<DrawCall> = self
                    .content_meshes
                    .iter()
                    .zip(&self.content_bindings)
                    .enumerate()
                    .map(|(i, (mesh, binding))| {
                        // Each stripe slides horizontally at its own speed
                        // and phase, so they visibly cross each other (and
                        // the mask's edge) rather than moving in lockstep.
                        let phase = i as f32 * 2.0;
                        let x = (self.elapsed * (0.6 + i as f32 * 0.2) + phase).sin() * 140.0;
                        let y = -70.0 + i as f32 * 70.0;
                        DrawCall {
                            mesh,
                            binding,
                            transform: Transform2D { translation: Vec2::new(x, y), ..Transform2D::default() },
                            fill_color: stripe_colors[i],
                            blend_mode: BlendMode::Normal, gradient: None, texture: None,
                            material: None,
                        }
                    })
                    .collect();

                match renderer.render_with_mask(
                    (ARTBOARD_WIDTH, ARTBOARD_HEIGHT),
                    Camera::default(),
                    wgpu::Color { r: 0.08, g: 0.08, b: 0.1, a: 1.0 },
                    &mask,
                    &content,
                ) {
                    Ok(()) => {
                        self.rendered_one_frame = true;
                        if self.smoke_test {
                            event_loop.exit();
                        }
                    }
                    Err(err) => eprintln!("an1mbyt3 mask: render failed: {err:?}"),
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
