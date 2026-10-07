//! Phase 10 smoke test: blend modes (AN1MBYT3_BUILD_PLAN.md §6's
//! "compositing, masks, blend modes... dedicated shader passes" bullet, and
//! Phase 10's exit criterion note on the same). A static orange square sits
//! under a moving cyan circle-ish shape; press 1/2/3/4 to switch the moving
//! shape's `BlendMode` between Normal/Multiply/Screen/Add and watch the
//! overlap region's color change live — proof that `Renderer` actually picks
//! a different pipeline per `DrawCall` (`renderer.rs`'s per-draw
//! `pipelines.get(&draw.blend_mode)` lookup), not just that the enum exists.
//!
//! Run with `cargo run -p an1mbyt3-renderer --example blend_modes`. See
//! `render_shape.rs`'s doc comment for the `WGPU_BACKEND` troubleshooting
//! note if this crashes immediately on Windows.

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use glam::Vec2;
use an1mbyt3_core::{Path, Transform2D};
use an1mbyt3_geometry::tessellate_path;
use an1mbyt3_renderer::{BlendMode, Camera, DrawBinding, DrawCall, GpuMesh, Renderer};

const ARTBOARD_WIDTH: f32 = 400.0;
const ARTBOARD_HEIGHT: f32 = 400.0;

struct App {
    smoke_test: bool,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    base_mesh: Option<GpuMesh>,
    base_binding: Option<DrawBinding>,
    overlay_mesh: Option<GpuMesh>,
    overlay_binding: Option<DrawBinding>,
    overlay_blend_mode: BlendMode,
    rendered_one_frame: bool,
}

impl App {
    fn new(smoke_test: bool) -> Self {
        Self {
            smoke_test,
            window: None,
            renderer: None,
            base_mesh: None,
            base_binding: None,
            overlay_mesh: None,
            overlay_binding: None,
            overlay_blend_mode: BlendMode::Normal,
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
            .with_title("an1mbyt3 — Phase 10 blend_modes smoke test (press 1/2/3/4)")
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

        let base_mesh_data = tessellate_path(&Path::rect(180.0, 180.0));
        let overlay_mesh_data = tessellate_path(&Path::rect(180.0, 180.0));

        self.base_mesh = Some(renderer.upload_mesh(&base_mesh_data));
        self.base_binding = Some(renderer.create_draw_binding());
        self.overlay_mesh = Some(renderer.upload_mesh(&overlay_mesh_data));
        self.overlay_binding = Some(renderer.create_draw_binding());

        self.window = Some(window);
        self.renderer = Some(renderer);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(key_code), state: ElementState::Pressed, repeat: false, .. },
                ..
            } => {
                self.overlay_blend_mode = match key_code {
                    KeyCode::Digit1 => BlendMode::Normal,
                    KeyCode::Digit2 => BlendMode::Multiply,
                    KeyCode::Digit3 => BlendMode::Screen,
                    KeyCode::Digit4 => BlendMode::Add,
                    _ => self.overlay_blend_mode,
                };
                if let Some(window) = &self.window {
                    window.set_title(&format!(
                        "an1mbyt3 — Phase 10 blend_modes smoke test — {:?} (press 1/2/3/4)",
                        self.overlay_blend_mode
                    ));
                }
            }
            WindowEvent::RedrawRequested => {
                let (
                    Some(renderer),
                    Some(base_mesh),
                    Some(base_binding),
                    Some(overlay_mesh),
                    Some(overlay_binding),
                ) = (
                    &self.renderer,
                    &self.base_mesh,
                    &self.base_binding,
                    &self.overlay_mesh,
                    &self.overlay_binding,
                )
                else {
                    return;
                };

                let draws = [
                    DrawCall {
                        mesh: base_mesh,
                        binding: base_binding,
                        transform: Transform2D { translation: Vec2::new(-60.0, 0.0), ..Transform2D::default() },
                        fill_color: [0.95, 0.55, 0.1, 1.0],
                        blend_mode: BlendMode::Normal, gradient: None, texture: None,
                        material: None,
                    },
                    DrawCall {
                        mesh: overlay_mesh,
                        binding: overlay_binding,
                        transform: Transform2D { translation: Vec2::new(60.0, 0.0), ..Transform2D::default() },
                        fill_color: [0.1, 0.7, 0.95, 1.0],
                        blend_mode: self.overlay_blend_mode, gradient: None, texture: None,
                        material: None,
                    },
                ];

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
                    Err(err) => eprintln!("an1mbyt3 blend_modes: render failed: {err:?}"),
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
