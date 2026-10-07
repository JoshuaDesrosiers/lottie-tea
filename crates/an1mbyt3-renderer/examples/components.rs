//! Phase 7 smoke test, now covering the whole pass's scope (not just the
//! exit criterion): one [`an1mbyt3_core::ComponentDefinition`], four
//! [`an1mbyt3_core::NodeKind::Instance`] nodes referencing it, each
//! distinct in a different way:
//! - **Dot A**: no override — the component's own default red.
//! - **Dot B**: a direct [`an1mbyt3_core::ComponentInstance::fill_color_override`]
//!   (blue) — the original Phase 7 exit criterion ("A component instantiated
//!   twice with different parameter overrides renders both distinctly").
//! - **Dot C**: a named [`an1mbyt3_core::ComponentVariant`] ("Gold") instead
//!   of a direct override — the Figma-style distinction: a variant is a
//!   shared, swappable preset authored once on the *definition*, not a
//!   one-off tweak to one instance (see `component.rs`'s doc comment).
//! - **Dot D**: driven by its own `an1mbyt3_runtime::AnimationClip`/
//!   `Playback`, bobbing up and down independently of A/B/C — proves
//!   "nested animation": each instance's underlying node already has a real,
//!   independent `NodeId`, so an external animation can target just one
//!   instance without touching the others, all while every instance still
//!   shares the exact same `GpuMesh` (tessellated/uploaded exactly once).
//!
//! Run with `cargo run -p an1mbyt3-renderer --example components`. See
//! `render_shape.rs`'s doc comment for the `WGPU_BACKEND` troubleshooting
//! note if this crashes immediately on Windows.

use std::sync::Arc;
use std::time::Instant;

use glam::Vec2;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use an1mbyt3_core::{
    Artboard, ArtboardId, ComponentDefinition, ComponentId, ComponentInstance, ComponentVariant, Document, Node, NodeId,
    NodeKind, Path,
};
use an1mbyt3_geometry::tessellate_path;
use an1mbyt3_renderer::{BlendMode, Camera, DrawBinding, DrawCall, GpuMesh, Renderer};
use an1mbyt3_runtime::{AnimationClip, Easing, Keyframe, LoopMode, Playback, Track};

const ARTBOARD_WIDTH: f32 = 400.0;
const ARTBOARD_HEIGHT: f32 = 400.0;

struct Scene {
    document: Document,
    artboard_id: ArtboardId,
    component_id: ComponentId,
    instance_a: NodeId,
    instance_b: NodeId,
    instance_c: NodeId,
    instance_d: NodeId,
}

fn build_scene() -> Scene {
    let mut document = Document::new();

    // One definition: a small square, default color red, plus one named
    // variant ("Gold") instances can opt into instead of a one-off override.
    let component_id = document.add_component(ComponentDefinition {
        name: "Dot".into(),
        path: Path::rect(70.0, 70.0),
        default_fill_color: [0.85, 0.2, 0.35, 1.0],
        variants: vec![ComponentVariant { name: "Gold".into(), fill_color: [0.9, 0.75, 0.2, 1.0] }],
        deleted: false,
    });

    let artboard_id = document.add_artboard(Artboard::new("Main", ARTBOARD_WIDTH, ARTBOARD_HEIGHT));
    let artboard = document.artboard_mut(artboard_id).unwrap();

    // Instance A: no override — renders the component's default red.
    let mut node_a = Node::new("Dot A", None, NodeKind::Instance(ComponentInstance::new(component_id)));
    node_a.transform.translation = Vec2::new(-135.0, 0.0);
    let instance_a = artboard.add_node(node_a);

    // Instance B: a direct override (blue) — the original exit criterion's
    // "different parameter overrides renders both distinctly."
    let mut instance_b_data = ComponentInstance::new(component_id);
    instance_b_data.fill_color_override = Some([0.3, 0.6, 0.9, 1.0]);
    let mut node_b = Node::new("Dot B", None, NodeKind::Instance(instance_b_data));
    node_b.transform.translation = Vec2::new(-45.0, 0.0);
    let instance_b = artboard.add_node(node_b);

    // Instance C: a named variant instead of a direct override — proves
    // variant resolution (`ComponentInstance::resolved_fill_color`'s
    // precedence: override, then variant, then the definition's default).
    let mut instance_c_data = ComponentInstance::new(component_id);
    instance_c_data.variant = Some("Gold".into());
    let mut node_c = Node::new("Dot C", None, NodeKind::Instance(instance_c_data));
    node_c.transform.translation = Vec2::new(45.0, 0.0);
    let instance_c = artboard.add_node(node_c);

    // Instance D: same component, but this one also gets an independent
    // animation driving its translation — see `App`'s `playback_d` field.
    let mut node_d = Node::new("Dot D", None, NodeKind::Instance(ComponentInstance::new(component_id)));
    node_d.transform.translation = Vec2::new(135.0, 0.0);
    let instance_d = artboard.add_node(node_d);

    Scene { document, artboard_id, component_id, instance_a, instance_b, instance_c, instance_d }
}

struct App {
    smoke_test: bool,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    gpu_mesh: Option<GpuMesh>,
    // One `DrawBinding` per instance, not one shared with `gpu_mesh` — see
    // `GpuMesh`'s doc comment in `renderer.rs`: sharing a uniform buffer
    // across instances means the second instance's write clobbers the
    // first's before either draw executes, so both would render as whichever
    // instance wrote last (this was the exact bug the very first version of
    // this example shipped with — one blue square instead of two colors).
    binding_a: Option<DrawBinding>,
    binding_b: Option<DrawBinding>,
    binding_c: Option<DrawBinding>,
    binding_d: Option<DrawBinding>,
    scene: Scene,
    // Instance D's own animation clip + playback cursor — deliberately not
    // shared with anything else, proving "nested" animation actually means
    // *independent* per-instance state, not just "the whole scene animates."
    clip_d: AnimationClip,
    playback_d: Playback,
    last_frame: Option<Instant>,
    rendered_one_frame: bool,
}

impl App {
    fn new(smoke_test: bool) -> Self {
        let scene = build_scene();

        let ease = Easing::CubicBezier { x1: 0.42, y1: 0.0, x2: 0.58, y2: 1.0 };
        let base_x = 135.0;
        let clip_d = AnimationClip::new(
            scene.instance_d,
            Track::new(vec![
                Keyframe::new(0.0, Vec2::new(base_x, -60.0), ease),
                Keyframe::new(0.6, Vec2::new(base_x, 60.0), ease),
                Keyframe::new(1.2, Vec2::new(base_x, -60.0), ease),
            ]),
        );
        let mut playback_d = Playback::new(clip_d.duration(), LoopMode::Loop);
        playback_d.play();

        Self {
            smoke_test,
            window: None,
            renderer: None,
            gpu_mesh: None,
            binding_a: None,
            binding_b: None,
            binding_c: None,
            binding_d: None,
            scene,
            clip_d,
            playback_d,
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
            .with_title("an1mbyt3 — Phase 7 components smoke test")
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

        // Uploaded once — all four instances draw with this same `GpuMesh`,
        // never re-tessellated per instance.
        let definition = self.scene.document.component(self.scene.component_id).unwrap();
        let mesh = tessellate_path(&definition.path);
        let gpu_mesh = renderer.upload_mesh(&mesh);

        self.window = Some(window);
        self.binding_a = Some(renderer.create_draw_binding());
        self.binding_b = Some(renderer.create_draw_binding());
        self.binding_c = Some(renderer.create_draw_binding());
        self.binding_d = Some(renderer.create_draw_binding());
        self.renderer = Some(renderer);
        self.gpu_mesh = Some(gpu_mesh);
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
                let (Some(renderer), Some(gpu_mesh), Some(binding_a), Some(binding_b), Some(binding_c), Some(binding_d)) = (
                    &self.renderer,
                    &self.gpu_mesh,
                    &self.binding_a,
                    &self.binding_b,
                    &self.binding_c,
                    &self.binding_d,
                ) else {
                    return;
                };

                let now = Instant::now();
                let delta_seconds = self.last_frame.map(|prev| (now - prev).as_secs_f32()).unwrap_or(0.0);
                self.last_frame = Some(now);
                self.playback_d.advance(delta_seconds);

                let artboard = self.scene.document.artboard_mut(self.scene.artboard_id).unwrap();
                self.clip_d.apply(artboard, self.playback_d.time);

                let document = &self.scene.document;
                let definition = document.component(self.scene.component_id).unwrap();
                let artboard = document.artboard(self.scene.artboard_id).unwrap();

                let draws: Vec<DrawCall> = [
                    (self.scene.instance_a, binding_a),
                    (self.scene.instance_b, binding_b),
                    (self.scene.instance_c, binding_c),
                    (self.scene.instance_d, binding_d),
                ]
                .into_iter()
                .map(|(node_id, binding)| {
                    let node = artboard.node(node_id).unwrap();
                    let NodeKind::Instance(component_instance) = &node.kind else {
                        unreachable!("all four demo nodes are component instances")
                    };
                    DrawCall {
                        mesh: gpu_mesh,
                        binding,
                        transform: node.transform,
                        fill_color: component_instance.resolved_fill_color(definition),
                        blend_mode: BlendMode::Normal, gradient: None, texture: None,
                        material: None,
                    }
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
                    Err(err) => eprintln!("an1mbyt3 components: render failed: {err:?}"),
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
