//! Phase 5 smoke test: a two-bone arm that follows the cursor, proving
//! `solve_two_bone_ik` drives a real bone chain — two `NodeKind::Bone` nodes
//! parented to each other, positioned via `Artboard::world_transform`
//! (AN1MBYT3_BUILD_PLAN.md §10, Phase 5 exit criterion: "A rigged shape
//! deforms correctly under IK-driven bone movement").
//!
//! Builds on `render_shape.rs`'s winit/wgpu setup (in `an1mbyt3-renderer`'s
//! examples). The cursor-following behavior is demo-level wiring, not a
//! runtime feature: AN1MBYT3_BUILD_PLAN.md §5's real parameter-binding
//! layer (pointer position driving arbitrary parameters) is Phase 6 work —
//! this example just reads `WindowEvent::CursorMoved` directly and calls
//! `solve_two_bone_ik` each frame, which is enough to prove the IK solver
//! and bone-chain world-transform composition work end to end without
//! needing that not-yet-built system.
//!
//! Run with `cargo run -p an1mbyt3-constraints --example bone_ik`. See
//! `render_shape.rs`'s doc comment for the `WGPU_BACKEND` troubleshooting
//! note if this crashes immediately on Windows.

use std::sync::Arc;

use glam::Vec2;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use an1mbyt3_constraints::solve_two_bone_ik;
use an1mbyt3_core::{Artboard, ArtboardId, BoneNode, Document, Node, NodeId, NodeKind};
use an1mbyt3_geometry::tessellate_path;
use an1mbyt3_renderer::{BlendMode, Camera, DrawBinding, DrawCall, GpuMesh, Renderer};

const ARTBOARD_WIDTH: f32 = 500.0;
const ARTBOARD_HEIGHT: f32 = 500.0;
const ROOT_ORIGIN: Vec2 = Vec2::new(-150.0, 0.0);
const ROOT_LENGTH: f32 = 110.0;
const MID_LENGTH: f32 = 90.0;
const BONE_WIDTH: f32 = 22.0;

/// Builds a bone-shaped [`an1mbyt3_core::Path`]: a slim quad from local
/// `(0, -width/2)` to `(length, width/2)` — the bone's own origin is one
/// end, not centered, unlike `Path::rect` — so that a bone's `Transform2D`
/// (a joint pose) naturally places its *start*, not its middle, at the
/// joint, matching every other 2D skeletal animation convention.
fn bone_path(length: f32, width: f32) -> an1mbyt3_core::Path {
    use an1mbyt3_core::PathCommand;
    let half_width = width / 2.0;
    an1mbyt3_core::Path {
        commands: vec![
            PathCommand::MoveTo(Vec2::new(0.0, -half_width)),
            PathCommand::LineTo(Vec2::new(length, -half_width)),
            PathCommand::LineTo(Vec2::new(length, half_width)),
            PathCommand::LineTo(Vec2::new(0.0, half_width)),
            PathCommand::Close,
        ],
    }
}

struct Scene {
    document: Document,
    artboard_id: ArtboardId,
    root_bone: NodeId,
    mid_bone: NodeId,
}

fn build_scene() -> Scene {
    let mut document = Document::new();
    let artboard_id = document.add_artboard(Artboard::new("Main", ARTBOARD_WIDTH, ARTBOARD_HEIGHT));

    let artboard = document.artboard_mut(artboard_id).unwrap();
    let mut root_node = Node::new("Root bone", None, NodeKind::Bone(BoneNode { length: ROOT_LENGTH }));
    root_node.transform.translation = ROOT_ORIGIN;
    let root_bone = artboard.add_node(root_node);

    let mut mid_node = Node::new("Mid bone", Some(root_bone), NodeKind::Bone(BoneNode { length: MID_LENGTH }));
    mid_node.transform.translation = Vec2::new(ROOT_LENGTH, 0.0);
    let mid_bone = artboard.add_node(mid_node);

    Scene { document, artboard_id, root_bone, mid_bone }
}

struct App {
    smoke_test: bool,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    root_mesh: Option<GpuMesh>,
    mid_mesh: Option<GpuMesh>,
    root_binding: Option<DrawBinding>,
    mid_binding: Option<DrawBinding>,
    scene: Scene,
    /// World-space cursor position (artboard coordinates: origin centered,
    /// +y up) — see `window_event`'s `CursorMoved` arm for the screen-to-
    /// artboard conversion. Starts pointing right, so the arm has a sane
    /// pose before the first mouse move.
    target: Vec2,
    rendered_one_frame: bool,
}

impl App {
    fn new(smoke_test: bool) -> Self {
        Self {
            smoke_test,
            window: None,
            renderer: None,
            root_mesh: None,
            mid_mesh: None,
            root_binding: None,
            mid_binding: None,
            scene: build_scene(),
            target: Vec2::new(150.0, 0.0),
            rendered_one_frame: false,
        }
    }

    /// Runs IK, writes the resulting rotations onto the two bone nodes, and
    /// returns each bone's *world* transform (composed via
    /// `Artboard::world_transform` — the mid bone's world position depends
    /// on the root's, since it's parented to it) ready to hand to the renderer.
    fn solve_and_pose(&mut self) -> (an1mbyt3_core::Transform2D, an1mbyt3_core::Transform2D) {
        let solution = solve_two_bone_ik(ROOT_ORIGIN, ROOT_LENGTH, MID_LENGTH, self.target);

        let artboard = self.scene.document.artboard_mut(self.scene.artboard_id).unwrap();
        artboard.node_mut(self.scene.root_bone).unwrap().transform.rotation = solution.root_rotation;
        artboard.node_mut(self.scene.mid_bone).unwrap().transform.rotation = solution.mid_local_rotation;

        let root_world = artboard.world_transform(self.scene.root_bone);
        let mid_world = artboard.world_transform(self.scene.mid_bone);
        (root_world, mid_world)
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attributes = Window::default_attributes()
            .with_title("an1mbyt3 — Phase 5 bone_ik smoke test (move your cursor)")
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

        // Same width for both segments (not tapered root->mid) and, below,
        // the same fill color for both draw calls — two rigid bones still
        // can't bend as one continuous shape (that needs real mesh skinning,
        // AN1MBYT3_BUILD_PLAN.md §10 Phase 5's still-open "skinning/
        // deformation" item), but matching width/color hides the seam at the
        // joint well enough to read as one continuous arm rather than two
        // visibly separate rectangles.
        let root_mesh_data = tessellate_path(&bone_path(ROOT_LENGTH, BONE_WIDTH));
        let mid_mesh_data = tessellate_path(&bone_path(MID_LENGTH, BONE_WIDTH));
        self.root_mesh = Some(renderer.upload_mesh(&root_mesh_data));
        self.mid_mesh = Some(renderer.upload_mesh(&mid_mesh_data));
        self.root_binding = Some(renderer.create_draw_binding());
        self.mid_binding = Some(renderer.create_draw_binding());

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
            WindowEvent::CursorMoved { position, .. } => {
                let Some(window) = &self.window else { return };
                let window_size = window.inner_size();
                // Screen space (origin top-left, +y down) -> artboard space
                // (origin centered, +y up) — the same convention
                // `an1mbyt3_renderer::Renderer`'s `build_mvp` uses.
                self.target = Vec2::new(
                    position.x as f32 - window_size.width as f32 / 2.0,
                    window_size.height as f32 / 2.0 - position.y as f32,
                );
            }
            WindowEvent::RedrawRequested => {
                if self.renderer.is_none()
                    || self.root_mesh.is_none()
                    || self.mid_mesh.is_none()
                    || self.root_binding.is_none()
                    || self.mid_binding.is_none()
                {
                    return;
                }

                // Solved before borrowing `renderer`/the meshes below —
                // `solve_and_pose` needs `&mut self` (it writes the solved
                // rotations onto the bone nodes), which can't coexist with
                // the immutable borrows the render call below needs.
                let (root_world, mid_world) = self.solve_and_pose();

                let (
                    Some(renderer),
                    Some(root_mesh),
                    Some(mid_mesh),
                    Some(root_binding),
                    Some(mid_binding),
                ) = (
                    &self.renderer,
                    &self.root_mesh,
                    &self.mid_mesh,
                    &self.root_binding,
                    &self.mid_binding,
                )
                else {
                    return;
                };

                let draws = [
                    DrawCall {
                        mesh: root_mesh,
                        binding: root_binding,
                        transform: root_world,
                        fill_color: [0.85, 0.2, 0.35, 1.0],
                        blend_mode: BlendMode::Normal, gradient: None, texture: None,
                        material: None,
                    },
                    DrawCall {
                        mesh: mid_mesh,
                        binding: mid_binding,
                        transform: mid_world,
                        fill_color: [0.85, 0.2, 0.35, 1.0],
                        blend_mode: BlendMode::Normal, gradient: None, texture: None,
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
                    Err(err) => eprintln!("an1mbyt3 bone_ik: render failed: {err:?}"),
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
