//! Phase 5 smoke test: the *real* smooth-bending arm — one continuous mesh
//! linear-blend-skinned across the same two-bone IK chain `bone_ik.rs`
//! drives, instead of two rigid pieces in matching colors. Proves
//! `an1mbyt3_geometry::skin_mesh` end to end: real per-vertex weights,
//! real inverse-bind matrices, real per-frame GPU vertex updates via
//! `Renderer::update_mesh_positions`.
//!
//! The document still has two `NodeKind::Bone` nodes (same as `bone_ik.rs`)
//! — they're not drawn directly here, they only exist so
//! `Artboard::world_transform` gives each bone's current world matrix for
//! the skinner to use. The one thing actually rendered is a single mesh
//! built once at startup in "bind pose" (a straight rectangle spanning both
//! bones' rest lengths) and re-skinned every frame.
//!
//! Run with `cargo run -p an1mbyt3-constraints --example bone_skin`. See
//! `render_shape.rs`'s (in `an1mbyt3-renderer`'s examples) doc comment for
//! the `WGPU_BACKEND` troubleshooting note if this crashes immediately on Windows.

use std::sync::Arc;

use glam::{Affine2, Vec2};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use an1mbyt3_constraints::solve_two_bone_ik;
use an1mbyt3_core::{Artboard, ArtboardId, BoneNode, Document, Node, NodeId, NodeKind, Transform2D};
use an1mbyt3_geometry::{skin_mesh, Mesh, SkinBinding, VertexWeight};
use an1mbyt3_renderer::{BlendMode, Camera, DrawBinding, DrawCall, GpuMesh, Renderer};

const ARTBOARD_WIDTH: f32 = 500.0;
const ARTBOARD_HEIGHT: f32 = 500.0;
const ROOT_ORIGIN: Vec2 = Vec2::new(-150.0, 0.0);
const ROOT_LENGTH: f32 = 110.0;
const MID_LENGTH: f32 = 90.0;
const BONE_WIDTH: f32 = 26.0;
/// How far on either side of the joint vertex weights blend between the two
/// bones, in the same local-length units as `ROOT_LENGTH`/`MID_LENGTH`.
/// Wider = a softer, more gradual bend; narrower converges toward the
/// hard-seam look `bone_ik.rs` gets from two separate rigid meshes.
const BLEND_RADIUS: f32 = 30.0;
/// How many quad segments the rest-pose mesh is divided into along its
/// length. Skinning can only blend *between vertices it has* — a plain
/// 4-corner rectangle has no vertices anywhere near the joint to blend, so
/// bending it produces a straight-edged taper between two rigid ends (a
/// blade/parallelogram shape), not a smooth bend. More segments here means
/// more vertices fall inside `BLEND_RADIUS` and the bend reads as curved
/// rather than kinked.
const LENGTH_SEGMENTS: usize = 24;

/// Builds the mesh's *rest pose* directly as a subdivided quad strip (not
/// via `Path`/`tessellate_path`) — a plain fill tessellator has no reason to
/// insert vertices along a straight edge, since fill tessellation only cares
/// about the boundary shape, not vertex density; getting `LENGTH_SEGMENTS`
/// interior vertices needs building the mesh directly.
///
/// Positions are in world space (offset by `ROOT_ORIGIN`) rather than some
/// separate local space — `skin_mesh`'s inverse-bind matrices are defined as
/// "undo the bone's rest-pose *world* transform," so the rest-pose vertex
/// positions have to already be in that same world space for the math to
/// line up (see `skin.rs`'s doc comment).
fn build_rest_mesh(total_length: f32, width: f32, segments: usize) -> Mesh {
    let half_width = width / 2.0;
    let mut positions = Vec::with_capacity((segments + 1) * 2);
    for i in 0..=segments {
        let x = ROOT_ORIGIN.x + total_length * (i as f32 / segments as f32);
        positions.push([x, ROOT_ORIGIN.y - half_width]); // bottom edge
        positions.push([x, ROOT_ORIGIN.y + half_width]); // top edge
    }

    let mut indices = Vec::with_capacity(segments * 6);
    for i in 0..segments {
        let bottom_left = (i * 2) as u32;
        let top_left = bottom_left + 1;
        let bottom_right = ((i + 1) * 2) as u32;
        let top_right = bottom_right + 1;
        indices.extend_from_slice(&[
            bottom_left, bottom_right, top_right, // first triangle of this segment's quad
            bottom_left, top_right, top_left, // second triangle
        ]);
    }

    Mesh { positions, indices, uvs: Vec::new() }
}

/// Bone 0 = root, bone 1 = mid (matching `bone_world_matrices`'s order in
/// the per-frame skin call). Weight blends linearly across `BLEND_RADIUS`
/// centered on the joint (`ROOT_LENGTH` along the rest-pose local length).
fn build_skin_binding(positions: &[[f32; 2]]) -> SkinBinding {
    let weights = positions
        .iter()
        .map(|&[x, _y]| {
            let local_x = x - ROOT_ORIGIN.x;
            let weight_a = if local_x <= ROOT_LENGTH - BLEND_RADIUS {
                1.0
            } else if local_x >= ROOT_LENGTH + BLEND_RADIUS {
                0.0
            } else {
                1.0 - (local_x - (ROOT_LENGTH - BLEND_RADIUS)) / (2.0 * BLEND_RADIUS)
            };
            VertexWeight { bone_a: 0, bone_b: 1, weight_a }
        })
        .collect();

    let root_bind = Transform2D::from_translation(ROOT_ORIGIN).to_affine2();
    let mid_bind = Transform2D::from_translation(ROOT_ORIGIN + Vec2::new(ROOT_LENGTH, 0.0)).to_affine2();

    SkinBinding { weights, inverse_bind_matrices: vec![root_bind.inverse(), mid_bind.inverse()] }
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
    gpu_mesh: Option<GpuMesh>,
    draw_binding: Option<DrawBinding>,
    rest_mesh: Option<Mesh>,
    skin_binding: Option<SkinBinding>,
    scene: Scene,
    target: Vec2,
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
            rest_mesh: None,
            skin_binding: None,
            scene: build_scene(),
            target: Vec2::new(150.0, 0.0),
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
            .with_title("an1mbyt3 — Phase 5 bone_skin smoke test (move your cursor)")
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

        let rest_mesh = build_rest_mesh(ROOT_LENGTH + MID_LENGTH, BONE_WIDTH, LENGTH_SEGMENTS);
        let skin_binding = build_skin_binding(&rest_mesh.positions);
        let gpu_mesh = renderer.upload_mesh(&rest_mesh);
        let draw_binding = renderer.create_draw_binding();

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.gpu_mesh = Some(gpu_mesh);
        self.draw_binding = Some(draw_binding);
        self.rest_mesh = Some(rest_mesh);
        self.skin_binding = Some(skin_binding);
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
                self.target = Vec2::new(
                    position.x as f32 - window_size.width as f32 / 2.0,
                    window_size.height as f32 / 2.0 - position.y as f32,
                );
            }
            WindowEvent::RedrawRequested => {
                let (
                    Some(renderer),
                    Some(gpu_mesh),
                    Some(draw_binding),
                    Some(rest_mesh),
                    Some(skin_binding),
                ) = (
                    &self.renderer,
                    &self.gpu_mesh,
                    &self.draw_binding,
                    &self.rest_mesh,
                    &self.skin_binding,
                )
                else {
                    return;
                };

                let solution = solve_two_bone_ik(ROOT_ORIGIN, ROOT_LENGTH, MID_LENGTH, self.target);
                let artboard = self.scene.document.artboard_mut(self.scene.artboard_id).unwrap();
                artboard.node_mut(self.scene.root_bone).unwrap().transform.rotation = solution.root_rotation;
                artboard.node_mut(self.scene.mid_bone).unwrap().transform.rotation = solution.mid_local_rotation;

                let root_world: Affine2 = artboard.world_transform(self.scene.root_bone).to_affine2();
                let mid_world: Affine2 = artboard.world_transform(self.scene.mid_bone).to_affine2();

                let skinned = skin_mesh(rest_mesh, skin_binding, &[root_world, mid_world]);
                renderer.update_mesh_positions(gpu_mesh, &skinned.positions);

                // Positions are already in world/artboard space after
                // skinning, so the draw's own transform is identity — the
                // per-vertex deformation already did what a rigid mesh's
                // uniform transform normally does.
                let draws = [DrawCall {
                    mesh: gpu_mesh,
                    binding: draw_binding,
                    transform: Transform2D::default(),
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
                    Err(err) => eprintln!("an1mbyt3 bone_skin: render failed: {err:?}"),
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
