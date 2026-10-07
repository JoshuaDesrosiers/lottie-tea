//! Phase 6 smoke test: a two-state state machine toggled by a keypress
//! (AN1MBYT3_BUILD_PLAN.md §10, Phase 6 exit criterion: "A state machine
//! with 2+ states and a trigger transitions correctly at runtime").
//!
//! Press Space to fire the `"toggle"` trigger, switching the square between
//! "Idle" (parked left, red) and "Active" (slid right, blue) — sliding
//! *and* flashing color, eased, in both directions.
//!
//! Idle and Active deliberately reference the exact same `Animation`
//! (`shared_animation_id`) rather than each getting its own — going to
//! Active plays it forward (`PlaybackDirection::Forward`), going back to
//! Idle plays the *same* animation in reverse (`PlaybackDirection::Reverse`,
//! Phase 10). No hand-authored mirror animation needed:
//! `StateMachine::transition_to` seeks a reversed transition's target state
//! to the animation's own authored `duration` and flips `Playback::speed`
//! to `-1.0` instead of always starting fresh at time `0.0`. The color
//! change rides on the same mechanism as the position change — both are
//! just tracks on the one shared `Animation` (`PropertyTrack::Translation`/
//! `PropertyTrack::FillColor`) — a transition doesn't need a special-cased
//! "color modifier," it's just another property the shared animation
//! drives, in whichever direction this transition plays it.
//!
//! Phase 8 (AN1MBYT3_BUILD_PLAN.md §10) reworked how this state machine is
//! *built*: the animation and the state machine definition are now real,
//! saved `Artboard` content (`Artboard::add_animation`/`add_state_machine`)
//! — `StateMachine::from_definition` builds the live instance this example
//! actually drives from that saved `StateMachineDef`, rather than the
//! state machine being hand-assembled Rust structs with no relationship to
//! anything an artboard could save/load. See
//! `an1mbyt3_core::state_machine`'s module doc for the full reasoning.
//!
//! Builds on `render_shape.rs`'s winit/wgpu setup (in this crate's own
//! examples). Run with `cargo run -p an1mbyt3-renderer --example state_machine`.
//! See `render_shape.rs`'s doc comment for the `WGPU_BACKEND` troubleshooting
//! note if this crashes immediately on Windows.

use std::sync::Arc;
use std::time::Instant;

use glam::Vec2;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use an1mbyt3_core::{
    Animation, Artboard, ArtboardId, ConditionDef, Document, InputDef, InputKind, LoopMode, Node, NodeKind, Path, PlaybackDirection, PropertyTrack,
    ShapeNode, StateDef, StateMachineDef, TransitionDef,
};
use an1mbyt3_geometry::tessellate_path;
use an1mbyt3_renderer::{BlendMode, Camera, DrawBinding, DrawCall, GpuMesh, Renderer};
use an1mbyt3_runtime::{Easing, Keyframe, StateMachine, Track};

const ARTBOARD_WIDTH: f32 = 400.0;
const ARTBOARD_HEIGHT: f32 = 400.0;

const REST_X: f32 = -120.0;
const ACTIVE_X: f32 = 120.0;
const REST_COLOR: [f32; 4] = [0.85, 0.2, 0.35, 1.0];
const ACTIVE_COLOR: [f32; 4] = [0.3, 0.55, 0.95, 1.0];

/// Saves the shared animation + state machine definition onto `artboard`,
/// then builds the live `StateMachine` from what was just saved — proving
/// the whole round trip (authored definition -> live instance) actually
/// works, not just that a live instance can be hand-built directly.
fn build_state_machine(document: &mut Document, artboard_id: ArtboardId, node: an1mbyt3_core::NodeId) -> StateMachine {
    let ease_in_out = Easing::CubicBezier { x1: 0.42, y1: 0.0, x2: 0.58, y2: 1.0 };

    // One animation, shared by both states (see this file's module doc for
    // why that's the point) — Idle's own rest pose *is* this animation's
    // time-0 keyframe, so nothing extra is needed for the initial,
    // never-animated entrance into Idle at `StateMachine::from_definition` time.
    let mut shared_animation = Animation::new("Idle/Active");
    shared_animation.duration = 0.6;
    shared_animation.tracks.push((
        node,
        PropertyTrack::Translation(Track::new(vec![
            Keyframe::new(0.0, Vec2::new(REST_X, 0.0), ease_in_out),
            Keyframe::new(0.6, Vec2::new(ACTIVE_X, 0.0), ease_in_out),
        ])),
    ));
    shared_animation.tracks.push((
        node,
        PropertyTrack::FillColor(Track::new(vec![Keyframe::new(0.0, REST_COLOR, ease_in_out), Keyframe::new(0.6, ACTIVE_COLOR, ease_in_out)])),
    ));

    let artboard = document.artboard_mut(artboard_id).unwrap();
    let shared_animation_id = artboard.add_animation(shared_animation);

    let def = StateMachineDef {
        name: "Main".into(),
        inputs: vec![InputDef { name: "toggle".into(), kind: InputKind::Trigger }],
        states: vec![
            StateDef {
                name: "Idle".into(),
                animation: shared_animation_id,
                loop_mode: LoopMode::Once,
                transitions: vec![TransitionDef {
                    to: "Active".into(),
                    condition: ConditionDef::Trigger { input: "toggle".into() },
                    direction: PlaybackDirection::Forward,
                    duration_seconds: 0.0,
                    interruptible: true,
                }],
                position: glam::Vec2::ZERO,
                blend: None,
                program: None,
            },
            StateDef {
                name: "Active".into(),
                animation: shared_animation_id,
                loop_mode: LoopMode::Once,
                transitions: vec![TransitionDef {
                    to: "Idle".into(),
                    condition: ConditionDef::Trigger { input: "toggle".into() },
                    direction: PlaybackDirection::Reverse,
                    duration_seconds: 0.0,
                    interruptible: true,
                }],
                position: glam::Vec2::ZERO,
                blend: None,
                program: None,
            },
        ],
        initial_state: "Idle".into(),
        layers: Vec::new(),
        deleted: false,
    };

    let state_machine_id = artboard.add_state_machine(def);

    let artboard = document.artboard(artboard_id).unwrap();
    StateMachine::from_definition(artboard.state_machine(state_machine_id).unwrap(), artboard)
}

fn build_document() -> (Document, ArtboardId, an1mbyt3_core::NodeId) {
    let mut document = Document::new();
    let artboard_id = document.add_artboard(Artboard::new("Main", ARTBOARD_WIDTH, ARTBOARD_HEIGHT));

    let artboard = document.artboard_mut(artboard_id).unwrap();
    let node_id = artboard.add_node(Node::new(
        "Square",
        None,
        NodeKind::Shape(ShapeNode {
            path: Path::rect(120.0, 120.0),
            fill_color: [0.85, 0.2, 0.35, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
    ));

    (document, artboard_id, node_id)
}

struct App {
    smoke_test: bool,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    gpu_mesh: Option<GpuMesh>,
    draw_binding: Option<DrawBinding>,
    document: Document,
    artboard_id: ArtboardId,
    node_id: an1mbyt3_core::NodeId,
    state_machine: StateMachine,
    last_frame: Option<Instant>,
    rendered_one_frame: bool,
}

impl App {
    fn new(smoke_test: bool) -> Self {
        let (mut document, artboard_id, node_id) = build_document();
        let state_machine = build_state_machine(&mut document, artboard_id, node_id);

        Self {
            smoke_test,
            window: None,
            renderer: None,
            gpu_mesh: None,
            draw_binding: None,
            document,
            artboard_id,
            node_id,
            state_machine,
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
            .with_title("an1mbyt3 — Phase 6 state_machine smoke test (press Space)")
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

        let artboard = self.document.artboard(self.artboard_id).unwrap();
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
        self.last_frame = Some(Instant::now());

        if self.smoke_test {
            self.state_machine.inputs.fire_trigger("toggle");
        }
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
                event: KeyEvent { physical_key: PhysicalKey::Code(KeyCode::Space), state: ElementState::Pressed, repeat: false, .. },
                ..
            } => {
                self.state_machine.inputs.fire_trigger("toggle");
            }
            WindowEvent::RedrawRequested => {
                let (Some(renderer), Some(gpu_mesh), Some(draw_binding)) =
                    (&self.renderer, &self.gpu_mesh, &self.draw_binding)
                else {
                    return;
                };

                let now = Instant::now();
                let delta = now.duration_since(self.last_frame.unwrap_or(now)).as_secs_f32();
                self.last_frame = Some(now);

                let artboard_ref = self.document.artboard(self.artboard_id).unwrap();
                self.state_machine.advance(delta, artboard_ref);
                let artboard = self.document.artboard_mut(self.artboard_id).unwrap();
                self.state_machine.apply(artboard);
                let node = artboard.node(self.node_id).unwrap();
                let transform = node.transform;
                // No longer a hardcoded constant — `AnimationClip::apply`
                // (Phase 10's `with_fill_color`) now writes the animated
                // color onto the node itself each frame, same as it always
                // has for `transform.translation`.
                let NodeKind::Shape(shape) = &node.kind else {
                    unreachable!("build_document always creates this node as a Shape")
                };
                let fill_color = shape.fill_color;

                let draws = [DrawCall {
                    mesh: gpu_mesh,
                    binding: draw_binding,
                    transform,
                    fill_color,
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
                    Err(err) => eprintln!("an1mbyt3 state_machine: render failed: {err:?}"),
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
