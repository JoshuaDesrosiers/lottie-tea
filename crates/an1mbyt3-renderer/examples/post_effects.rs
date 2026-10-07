//! Phase 11 smoke test: post-processing (AN1MBYT3_BUILD_PLAN.md §6/§10 —
//! "offscreen scene render target + full-screen shader passes"), extended
//! for §11's layered post-processing panel. A bright shape sits over a dim
//! background square; press 1 to clear the effect stack, 2-5 to *append*
//! `Blur`/`Glow`/`Pixelate`/`ColorGrading` to it (press the same digit twice
//! to stack the same effect on itself, proving duplicates chain correctly,
//! not just coexist) — proof that `Renderer::render_with_post_effects`
//! actually chains each effect's output into the next one's input via the
//! ping-pong offscreen targets described in that method's doc comment, not
//! just that a `Vec<PostEffect>` parameter exists.
//!
//! Nothing about how the two shapes themselves draw changes between effects
//! (same `DrawCall`s, same flat-fill pipeline, same `BlendMode`) — only the
//! post-process step wrapped around the whole frame differs, matching §10's
//! exit criterion ("without changing how its own shapes are drawn").
//!
//! Run with `cargo run -p an1mbyt3-renderer --example post_effects`. See
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
use an1mbyt3_renderer::{BlendMode, Camera, DrawBinding, DrawCall, GpuMesh, PostEffect, Renderer};

const ARTBOARD_WIDTH: f32 = 400.0;
const ARTBOARD_HEIGHT: f32 = 400.0;

struct App {
    smoke_test: bool,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    background_mesh: Option<GpuMesh>,
    background_binding: Option<DrawBinding>,
    bright_mesh: Option<GpuMesh>,
    bright_binding: Option<DrawBinding>,
    /// The active effect *stack*, in render order — starts pre-loaded with
    /// a Blur-then-Glow chain (rather than one single effect) specifically
    /// so `AN1MBYT3_SMOKE_TEST=1`'s exit-after-first-frame path exercises
    /// real ping-pong chaining, not just the single-effect case the old
    /// version of this example only ever proved.
    effects: Vec<PostEffect>,
    rendered_one_frame: bool,
}

impl App {
    fn new(smoke_test: bool) -> Self {
        Self {
            smoke_test,
            window: None,
            renderer: None,
            background_mesh: None,
            background_binding: None,
            bright_mesh: None,
            bright_binding: None,
            effects: vec![PostEffect::Blur { radius: 1.5 }, PostEffect::Glow { radius: 1.5, threshold: 0.6, intensity: 1.8 }],
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
            .with_title("an1mbyt3 — Phase 11 post_effects smoke test (press 1-5)")
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

        let background_mesh_data = tessellate_path(&Path::rect(320.0, 320.0));
        let bright_mesh_data = tessellate_path(&Path::rect(80.0, 80.0));

        self.background_mesh = Some(renderer.upload_mesh(&background_mesh_data));
        self.background_binding = Some(renderer.create_draw_binding());
        self.bright_mesh = Some(renderer.upload_mesh(&bright_mesh_data));
        self.bright_binding = Some(renderer.create_draw_binding());

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
                match key_code {
                    KeyCode::Digit1 => self.effects.clear(),
                    KeyCode::Digit2 => self.effects.push(PostEffect::Blur { radius: 1.5 }),
                    KeyCode::Digit3 => self.effects.push(PostEffect::Glow { radius: 1.5, threshold: 0.6, intensity: 1.8 }),
                    KeyCode::Digit4 => self.effects.push(PostEffect::Pixelate { block_size: 10.0 }),
                    KeyCode::Digit5 => self.effects.push(PostEffect::ColorGrading { brightness: 0.05, contrast: 1.4, saturation: 0.3 }),
                    _ => {}
                };
                if let Some(window) = &self.window {
                    window.set_title(&format!("an1mbyt3 — Phase 11 post_effects smoke test — {:?} (1=clear, 2-5=append)", self.effects));
                }
            }
            WindowEvent::RedrawRequested => {
                let (
                    Some(renderer),
                    Some(background_mesh),
                    Some(background_binding),
                    Some(bright_mesh),
                    Some(bright_binding),
                ) = (
                    &self.renderer,
                    &self.background_mesh,
                    &self.background_binding,
                    &self.bright_mesh,
                    &self.bright_binding,
                )
                else {
                    return;
                };

                let draws = [
                    // Dim — a `Glow` bright-pass threshold should exclude
                    // this, proving the effect is selective, not just "blur
                    // everything."
                    DrawCall {
                        mesh: background_mesh,
                        binding: background_binding,
                        transform: Transform2D::default(),
                        fill_color: [0.15, 0.12, 0.2, 1.0],
                        blend_mode: BlendMode::Normal, gradient: None, texture: None,
                        material: None,
                    },
                    // Bright — well above any reasonable glow threshold.
                    DrawCall {
                        mesh: bright_mesh,
                        binding: bright_binding,
                        transform: Transform2D { translation: Vec2::ZERO, ..Transform2D::default() },
                        fill_color: [1.0, 0.9, 0.3, 1.0],
                        blend_mode: BlendMode::Normal, gradient: None, texture: None,
                        material: None,
                    },
                ];

                match renderer.render_with_post_effects(
                    (ARTBOARD_WIDTH, ARTBOARD_HEIGHT),
                    Camera::default(),
                    wgpu::Color { r: 0.05, g: 0.05, b: 0.07, a: 1.0 },
                    &[],
                    &draws,
                    &[],
                    &self.effects,
                    None,
                ) {
                    Ok(()) => {
                        self.rendered_one_frame = true;
                        if self.smoke_test {
                            event_loop.exit();
                        }
                    }
                    Err(err) => eprintln!("an1mbyt3 post_effects: render failed: {err:?}"),
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
