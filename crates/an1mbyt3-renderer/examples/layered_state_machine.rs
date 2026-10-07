//! Smoke test / demo for layered state machines (`StateMachineLayerDef`,
//! `an1mbyt3-core`) — the "eyebrows independent of mouth, within one
//! machine" feature request this answers. Two independent rig parts, two
//! independent little state machines, one shared `StateMachineDef`:
//!
//! - The base layer drives the **Head** square: press Space to fire
//!   `"toggleHead"`, sliding it from parked-left/red ("Idle") to
//!   slid-right/blue ("Nod").
//! - An extra layer, masked to the **Mouth** square only, drives it
//!   independently: press M to fire `"toggleMouth"`, sliding *it* from
//!   parked-left/red ("Closed") to slid-right/blue ("Open") — completely
//!   unaffected by whichever state the head happens to be in, and vice
//!   versa, even though both live inside the exact same `StateMachineDef`
//!   and share the same `StateMachine::inputs`.
//!
//! Every transition here crossfades over 0.6s (`TransitionDef::duration_seconds`)
//! and is fully interruptible (`interruptible: true`, Rive's "Allow Exit
//! During Transition") — mash Space or M rapidly and each square smoothly
//! reverses direction from wherever it actually is, rather than popping.
//! That correctness depends on `an1mbyt3_runtime::Layer::transition_to`
//! freezing the *live composited pose* (`Layer::composited_pose`) as the
//! new crossfade's starting point when a transition interrupts an
//! already-in-progress one, instead of resampling the target state's raw
//! animation — see that function's own doc comment for the exact failure
//! mode this avoids.
//!
//! Builds on `state_machine.rs`'s winit/wgpu setup and its
//! `AN1MBYT3_SMOKE_TEST=1` convention (fires every trigger once, renders
//! one frame, exits, and — new here — asserts both squares actually ended
//! up where each layer's own transition says they should, proving the
//! whole authored-definition -> live-instance -> masked-composite-> render
//! pipeline works end to end without a running interactive session).
//! Run with `cargo run -p an1mbyt3-renderer --example layered_state_machine`.

use std::sync::Arc;
use std::time::Instant;

use glam::Vec2;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use an1mbyt3_core::{
    Animation, Artboard, ArtboardId, ConditionDef, Document, InputDef, InputKind, LoopMode, Node, NodeId, NodeKind, Path, PlaybackDirection,
    PropertyTrack, ShapeNode, StateDef, StateMachineDef, StateMachineLayerDef, TransitionDef,
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
const HEAD_Y: f32 = -60.0;
const MOUTH_Y: f32 = 60.0;

/// One square's rest-to-active slide+color animation, offset to `y` so the
/// head and mouth squares don't overlap — otherwise identical in shape to
/// `state_machine.rs`'s single shared animation.
fn slide_animation(name: &str, node: NodeId, y: f32) -> Animation {
    let ease_in_out = Easing::CubicBezier { x1: 0.42, y1: 0.0, x2: 0.58, y2: 1.0 };
    let mut animation = Animation::new(name);
    animation.duration = 0.6;
    animation.tracks.push((node, PropertyTrack::Translation(Track::new(vec![Keyframe::new(0.0, Vec2::new(REST_X, y), ease_in_out), Keyframe::new(0.6, Vec2::new(ACTIVE_X, y), ease_in_out)]))));
    animation.tracks.push((node, PropertyTrack::FillColor(Track::new(vec![Keyframe::new(0.0, REST_COLOR, ease_in_out), Keyframe::new(0.6, ACTIVE_COLOR, ease_in_out)]))));
    animation
}

/// Saves a base-layer "Head" machine plus an extra, mouth-masked layer onto
/// `artboard`, then builds the live `StateMachine` from what was just saved
/// — same "prove the round trip, not just a hand-built live instance"
/// reasoning `state_machine.rs`'s own `build_state_machine` already
/// establishes.
fn build_state_machine(document: &mut Document, artboard_id: ArtboardId, head: NodeId, mouth: NodeId) -> StateMachine {
    let artboard = document.artboard_mut(artboard_id).unwrap();
    let head_animation = artboard.add_animation(slide_animation("Head", head, HEAD_Y));
    let mouth_animation = artboard.add_animation(slide_animation("Mouth", mouth, MOUTH_Y));

    let def = StateMachineDef {
        name: "Main".into(),
        inputs: vec![InputDef { name: "toggleHead".into(), kind: InputKind::Trigger }, InputDef { name: "toggleMouth".into(), kind: InputKind::Trigger }],
        states: vec![
            StateDef {
                name: "Idle".into(),
                animation: head_animation,
                loop_mode: LoopMode::Once,
                transitions: vec![TransitionDef {
                    to: "Nod".into(),
                    condition: ConditionDef::Trigger { input: "toggleHead".into() },
                    direction: PlaybackDirection::Forward,
                    duration_seconds: 0.6,
                    interruptible: true,
                }],
                position: Vec2::ZERO,
                blend: None,
                program: None,
            },
            StateDef {
                name: "Nod".into(),
                animation: head_animation,
                loop_mode: LoopMode::Once,
                transitions: vec![TransitionDef {
                    to: "Idle".into(),
                    condition: ConditionDef::Trigger { input: "toggleHead".into() },
                    direction: PlaybackDirection::Reverse,
                    duration_seconds: 0.6,
                    interruptible: true,
                }],
                position: Vec2::ZERO,
                blend: None,
                program: None,
            },
        ],
        initial_state: "Idle".into(),
        layers: vec![StateMachineLayerDef {
            name: "Mouth".into(),
            states: vec![
                StateDef {
                    name: "Closed".into(),
                    animation: mouth_animation,
                    loop_mode: LoopMode::Once,
                    transitions: vec![TransitionDef {
                        to: "Open".into(),
                        condition: ConditionDef::Trigger { input: "toggleMouth".into() },
                        direction: PlaybackDirection::Forward,
                        duration_seconds: 0.6,
                        interruptible: true,
                    }],
                    position: Vec2::ZERO,
                    blend: None,
                    program: None,
                },
                StateDef {
                    name: "Open".into(),
                    animation: mouth_animation,
                    loop_mode: LoopMode::Once,
                    transitions: vec![TransitionDef {
                        to: "Closed".into(),
                        condition: ConditionDef::Trigger { input: "toggleMouth".into() },
                        direction: PlaybackDirection::Reverse,
                        duration_seconds: 0.6,
                        interruptible: true,
                    }],
                    position: Vec2::ZERO,
                    blend: None,
                    program: None,
                },
            ],
            initial_state: "Closed".into(),
            mask: vec![mouth],
        }],
        deleted: false,
    };

    let state_machine_id = artboard.add_state_machine(def);

    let artboard = document.artboard(artboard_id).unwrap();
    StateMachine::from_definition(artboard.state_machine(state_machine_id).unwrap(), artboard)
}

fn build_document() -> (Document, ArtboardId, NodeId, NodeId) {
    let mut document = Document::new();
    let artboard_id = document.add_artboard(Artboard::new("Main", ARTBOARD_WIDTH, ARTBOARD_HEIGHT));

    let artboard = document.artboard_mut(artboard_id).unwrap();
    let head = artboard.add_node(Node::new("Head", None, NodeKind::Shape(ShapeNode { path: Path::rect(80.0, 80.0), fill_color: REST_COLOR, blend_mode: Default::default(), gradient: None, image: None, stroke: None})));
    let mouth = artboard.add_node(Node::new("Mouth", None, NodeKind::Shape(ShapeNode { path: Path::rect(80.0, 40.0), fill_color: REST_COLOR, blend_mode: Default::default(), gradient: None, image: None, stroke: None})));

    (document, artboard_id, head, mouth)
}

struct App {
    smoke_test: bool,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    // One `DrawBinding` (GPU uniform buffer) *per shape*, not shared —
    // `Renderer::render` queues every draw's uniform write up front, before
    // recording the render pass, so two draws sharing one `DrawBinding`
    // would have the second write clobber the first before either actually
    // renders (both shapes would draw with whichever transform/color was
    // written last) — exactly the "only one square visible" bug a shared
    // binding produced here.
    shapes: Vec<(NodeId, GpuMesh, DrawBinding)>,
    document: Document,
    artboard_id: ArtboardId,
    head: NodeId,
    mouth: NodeId,
    state_machine: StateMachine,
    last_frame: Option<Instant>,
    rendered_one_frame: bool,
}

impl App {
    fn new(smoke_test: bool) -> Self {
        let (mut document, artboard_id, head, mouth) = build_document();
        let state_machine = build_state_machine(&mut document, artboard_id, head, mouth);

        Self {
            smoke_test,
            window: None,
            renderer: None,
            shapes: Vec::new(),
            document,
            artboard_id,
            head,
            mouth,
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
            .with_title("an1mbyt3 — layered_state_machine smoke test (Space: head, M: mouth)")
            .with_inner_size(winit::dpi::LogicalSize::new(ARTBOARD_WIDTH, ARTBOARD_HEIGHT));
        let window = Arc::new(event_loop.create_window(window_attributes).expect("failed to create window"));

        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance.create_surface(window.clone()).expect("failed to create wgpu surface for window");

        let renderer = pollster::block_on(Renderer::new(&instance, surface, size.width, size.height));

        let artboard = self.document.artboard(self.artboard_id).unwrap();
        let shapes = artboard
            .shapes()
            .map(|(id, shape_node)| {
                let NodeKind::Shape(shape) = &shape_node.kind else { unreachable!() };
                let mesh = tessellate_path(&shape.path);
                (id, renderer.upload_mesh(&mesh), renderer.create_draw_binding())
            })
            .collect();

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.shapes = shapes;
        self.last_frame = Some(Instant::now());

        if self.smoke_test {
            self.state_machine.inputs.fire_trigger("toggleHead");
            self.state_machine.inputs.fire_trigger("toggleMouth");
            // Forces both layers' `Once` animations to their fully-settled
            // end position deterministically — the render loop's first real
            // frame delta is whatever wall-clock time happened to elapse
            // between window creation and this callback (a handful of
            // milliseconds, not the full 0.6s slide), so asserting a final
            // position off that delta alone would be a flaky, timing-
            // dependent smoke test rather than a real proof of the feature.
            let artboard_ref = self.document.artboard(self.artboard_id).unwrap();
            self.state_machine.advance(0.6, artboard_ref);
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
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(KeyCode::Space), state: ElementState::Pressed, repeat: false, .. }, .. } => {
                self.state_machine.inputs.fire_trigger("toggleHead");
            }
            WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(KeyCode::KeyM), state: ElementState::Pressed, repeat: false, .. }, .. } => {
                self.state_machine.inputs.fire_trigger("toggleMouth");
            }
            WindowEvent::RedrawRequested => {
                let Some(renderer) = &self.renderer else {
                    return;
                };

                let now = Instant::now();
                let delta = now.duration_since(self.last_frame.unwrap_or(now)).as_secs_f32();
                self.last_frame = Some(now);

                let artboard_ref = self.document.artboard(self.artboard_id).unwrap();
                self.state_machine.advance(delta, artboard_ref);
                let artboard = self.document.artboard_mut(self.artboard_id).unwrap();
                self.state_machine.apply(artboard);

                if self.smoke_test {
                    let head_x = artboard.node(self.head).unwrap().transform.translation.x;
                    let mouth_x = artboard.node(self.mouth).unwrap().transform.translation.x;
                    assert!((head_x - ACTIVE_X).abs() < 1e-3, "base layer (head) should have transitioned to Nod, got x={head_x}");
                    assert!((mouth_x - ACTIVE_X).abs() < 1e-3, "extra layer (mouth) should have transitioned to Open independently, got x={mouth_x}");
                }

                let draws: Vec<DrawCall> = self
                    .shapes
                    .iter()
                    .map(|(node_id, gpu_mesh, draw_binding)| {
                        let node = artboard.node(*node_id).unwrap();
                        let NodeKind::Shape(shape) = &node.kind else { unreachable!() };
                        DrawCall { mesh: gpu_mesh, binding: draw_binding, transform: node.transform, fill_color: shape.fill_color, blend_mode: BlendMode::Normal, gradient: None, texture: None, material: None }
                    })
                    .collect();

                match renderer.render((ARTBOARD_WIDTH, ARTBOARD_HEIGHT), Camera::default(), wgpu::Color { r: 0.08, g: 0.08, b: 0.1, a: 1.0 }, &draws) {
                    Ok(()) => {
                        self.rendered_one_frame = true;
                        if self.smoke_test {
                            event_loop.exit();
                        }
                    }
                    Err(err) => eprintln!("an1mbyt3 layered_state_machine: render failed: {err:?}"),
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
