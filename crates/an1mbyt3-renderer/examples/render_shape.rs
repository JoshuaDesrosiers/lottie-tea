//! Phase 1 smoke test: proves the `Document -> tessellate -> Renderer` path
//! actually draws a shape to a real window (AN1MBYT3_BUILD_PLAN.md §10,
//! Phase 1 exit criterion: "Can construct a document in-process and render a
//! static shape to a window/canvas").
//!
//! This is the windowing glue (winit) that `an1mbyt3-renderer` itself
//! deliberately does not depend on (see `src/renderer.rs`'s module doc) — it
//! lives here, in an example, precisely to keep that boundary real instead of
//! theoretical.
//!
//! Run with `cargo run -p an1mbyt3-renderer --example render_shape`. Set
//! `AN1MBYT3_SMOKE_TEST=1` to auto-close after the first rendered frame,
//! which is what makes this usable as an automated/CI check rather than only
//! a manual one.
//!
//! **Troubleshooting:** if this crashes immediately with
//! `STATUS_ACCESS_VIOLATION` (or a segfault) on Windows, it's very likely a
//! broken Vulkan driver/ICD on that machine, not an engine bug — wgpu's
//! default backend selection tries Vulkan first. Confirm by setting
//! `WGPU_BACKEND=dx12` (or `gl`) before running; if that succeeds, the
//! Vulkan backend on that machine is the problem, and DX12 is the practical
//! default for local Windows development.

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use an1mbyt3_core::{Artboard, Document, Node, NodeKind, Path, ShapeNode};
use an1mbyt3_geometry::tessellate_path;
use an1mbyt3_renderer::{BlendMode, Camera, DrawBinding, DrawCall, GpuMesh, Renderer};

const ARTBOARD_WIDTH: f32 = 400.0;
const ARTBOARD_HEIGHT: f32 = 400.0;

/// Builds the minimal document this smoke test renders: one artboard, one
/// filled rectangle — matching §11's "draw a vector shape" first step.
fn build_document() -> Document {
    let mut document = Document::new();
    let artboard_id = document.add_artboard(Artboard::new("Main", ARTBOARD_WIDTH, ARTBOARD_HEIGHT));

    let artboard = document.artboard_mut(artboard_id).unwrap();
    artboard.add_node(Node::new(
        "Square",
        None,
        NodeKind::Shape(ShapeNode {
            path: Path::rect(160.0, 160.0),
            fill_color: [0.85, 0.2, 0.35, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
    ));

    document
}

/// Owns the window and GPU state once the event loop has actually resumed —
/// `None` until `resumed()` fires, per winit 0.30's `ApplicationHandler`
/// lifecycle (a window can't be created before that point on some platforms).
struct App {
    smoke_test: bool,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    gpu_mesh: Option<GpuMesh>,
    draw_binding: Option<DrawBinding>,
    rendered_one_frame: bool,
}

impl App {
    fn new(smoke_test: bool) -> Self {
        Self {
            smoke_test,
            window: None,
            renderer: None,
            gpu_mesh: None,
            draw_binding: None,
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
            .with_title("an1mbyt3 — Phase 1 render_shape smoke test")
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

        let document = build_document();
        let artboard_id = document.artboards().next().unwrap().0;
        let artboard = document.artboard(artboard_id).unwrap();
        let (_, shape_node) = artboard.shapes().next().expect("document has no shape node");
        let path = match &shape_node.kind {
            NodeKind::Shape(shape) => &shape.path,
            NodeKind::Group(_) | NodeKind::Bone(_) | NodeKind::Instance(_) | NodeKind::Text(_) | NodeKind::Image(_) | NodeKind::ParticleSystem(_) | NodeKind::Tilemap(_) | NodeKind::Camera(_) | NodeKind::Light2D(_) | NodeKind::Trail(_) | NodeKind::Node3D(_) => unreachable!(),
        };
        let mesh = tessellate_path(path);
        let gpu_mesh = renderer.upload_mesh(&mesh);
        let draw_binding = renderer.create_draw_binding();

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.gpu_mesh = Some(gpu_mesh);
        self.draw_binding = Some(draw_binding);
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
                let (Some(renderer), Some(gpu_mesh), Some(draw_binding)) =
                    (&self.renderer, &self.gpu_mesh, &self.draw_binding)
                else {
                    return;
                };

                let draws = [DrawCall {
                    mesh: gpu_mesh,
                    binding: draw_binding,
                    transform: an1mbyt3_core::Transform2D::default(),
                    fill_color: [0.85, 0.2, 0.35, 1.0],
                    blend_mode: BlendMode::Normal, gradient: None, texture: None,
                    material: None,
                }];

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
                    Err(err) => eprintln!("an1mbyt3 render_shape: render failed: {err:?}"),
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        } else {
            let _ = event_loop; // window not created yet — nothing to redraw
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
