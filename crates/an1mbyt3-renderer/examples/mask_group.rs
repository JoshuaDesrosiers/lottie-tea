//! Phase 10 smoke test: a mask authored *through the document model*, not
//! hand-built `DrawCall`s directly — the other half of `mask.rs`'s proof.
//! `mask.rs` shows `Renderer::render_with_mask` itself works; this example
//! shows an author-facing path actually gets there: a `NodeKind::Group`
//! node's `GroupNode::mask` field names a sibling child as the mask, and a
//! small resolver here (`resolve_masked_group`) walks `Artboard::children_of`
//! to turn that into the same `render_with_mask` call `mask.rs` makes by
//! hand. AN1MBYT3_BUILD_PLAN.md §6/§10: "a mask layer clips a group's
//! contents."
//!
//! The masked group also has its own (animated) transform, composed through
//! `Artboard::world_transform` for every child — proving the mask and its
//! content move together as a real parent-child group, not just two
//! independently-positioned shapes that happen to overlap.
//!
//! `resolve_masked_group` is a small, this-example-only stand-in for a real
//! scene-compiler pass — nothing in `an1mbyt3-renderer`/`an1mbyt3-core`
//! walks a whole `Artboard` into draw calls generically yet (every example
//! in this workspace hand-picks the handful of nodes it cares about); that
//! generic walker is real future work, not a Phase 10 gap.
//!
//! Run with `cargo run -p an1mbyt3-renderer --example mask_group`. See
//! `render_shape.rs`'s doc comment for the `WGPU_BACKEND` troubleshooting
//! note if this crashes immediately on Windows.

use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use glam::Vec2;
use an1mbyt3_core::{Artboard, ArtboardId, Document, GroupNode, Node, NodeId, NodeKind, Path, PathCommand, ShapeNode};
use an1mbyt3_geometry::tessellate_path;
use an1mbyt3_renderer::{BlendMode, Camera, DrawBinding, DrawCall, GpuMesh, Renderer};

const ARTBOARD_WIDTH: f32 = 400.0;
const ARTBOARD_HEIGHT: f32 = 400.0;
const MASK_RADIUS: f32 = 110.0;
const CIRCLE_SEGMENTS: usize = 48;

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

struct Scene {
    document: Document,
    artboard_id: ArtboardId,
    group_id: NodeId,
    stripe_ids: Vec<NodeId>,
}

fn build_scene() -> Scene {
    let mut document = Document::new();
    let artboard_id = document.add_artboard(Artboard::new("Main", ARTBOARD_WIDTH, ARTBOARD_HEIGHT));
    let artboard = document.artboard_mut(artboard_id).unwrap();

    // The mask child is added first so its `NodeId` exists to reference from
    // `GroupNode::mask` below, but a group's `mask` can point at any child
    // regardless of insertion order — `children_of` doesn't care.
    let mask_id = artboard.add_node(Node::new(
        "Circle mask",
        None,
        NodeKind::Shape(ShapeNode { path: build_circle_path(MASK_RADIUS, CIRCLE_SEGMENTS), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
    ));

    let group_id = artboard.add_node(Node::new("Masked group", None, NodeKind::Group(GroupNode { mask: None, y_sort_enabled: false })));
    // `add_node` decided `mask_id`/`group_id` were roots when each was
    // inserted (both have `parent: None`) — reparenting the mask under the
    // group after the fact, then wiring `GroupNode::mask` to it, matches how
    // an editor would let an author drag an existing shape into a group and
    // mark it as that group's mask afterward, rather than requiring the
    // mask to be authored as a child from the start.
    artboard.node_mut(mask_id).unwrap().parent = Some(group_id);
    let NodeKind::Group(group_node) = &mut artboard.node_mut(group_id).unwrap().kind else {
        unreachable!("just constructed this node as a Group above")
    };
    group_node.mask = Some(mask_id);

    let stripe_colors = [[0.9, 0.25, 0.3, 1.0], [0.25, 0.8, 0.4, 1.0], [0.3, 0.45, 0.95, 1.0]];
    let stripe_ids: Vec<NodeId> = stripe_colors
        .into_iter()
        .enumerate()
        .map(|(i, color)| {
            let mut node = Node::new(
                format!("Stripe {i}"),
                Some(group_id),
                NodeKind::Shape(ShapeNode { path: Path::rect(320.0, 90.0), fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
            );
            node.transform.translation = Vec2::new(0.0, -70.0 + i as f32 * 70.0);
            artboard.add_node(node)
        })
        .collect();

    Scene { document, artboard_id, group_id, stripe_ids }
}

/// The this-example-only "scene compiler" described in the module doc:
/// resolves one masked `NodeKind::Group` into the `(mask, content)` pair
/// `Renderer::render_with_mask` needs, tessellating fresh geometry for
/// whichever children haven't been uploaded yet.
struct MaskedGroupMeshes {
    mask_mesh: GpuMesh,
    mask_binding: DrawBinding,
    stripe_meshes: Vec<GpuMesh>,
    stripe_bindings: Vec<DrawBinding>,
}

fn upload_masked_group(renderer: &Renderer, artboard: &Artboard, group_id: NodeId) -> MaskedGroupMeshes {
    let group_node = artboard.node(group_id).unwrap();
    let NodeKind::Group(GroupNode { mask: Some(mask_id), .. }) = group_node.kind else {
        panic!("upload_masked_group: `group_id` must be a Group with a mask set");
    };

    let mask_shape_node = artboard.node(mask_id).unwrap();
    let NodeKind::Shape(mask_shape) = &mask_shape_node.kind else {
        panic!("upload_masked_group: the mask node must be a Shape");
    };
    let mask_mesh = renderer.upload_mesh(&tessellate_path(&mask_shape.path));
    let mask_binding = renderer.create_draw_binding();

    let (stripe_meshes, stripe_bindings) = artboard
        .children_of(group_id)
        .filter(|(id, _)| *id != mask_id)
        .map(|(_, node)| {
            let NodeKind::Shape(shape) = &node.kind else {
                panic!("upload_masked_group: every non-mask child here is a Shape");
            };
            (renderer.upload_mesh(&tessellate_path(&shape.path)), renderer.create_draw_binding())
        })
        .unzip();

    MaskedGroupMeshes { mask_mesh, mask_binding, stripe_meshes, stripe_bindings }
}

struct App {
    smoke_test: bool,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    scene: Scene,
    meshes: Option<MaskedGroupMeshes>,
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
            scene: build_scene(),
            meshes: None,
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
            .with_title("an1mbyt3 — Phase 10 mask_group smoke test")
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

        let artboard = self.scene.document.artboard(self.scene.artboard_id).unwrap();
        self.meshes = Some(upload_masked_group(&renderer, artboard, self.scene.group_id));

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
                let Some(meshes) = &self.meshes else { return };

                let now = Instant::now();
                let delta = now.duration_since(self.last_frame.unwrap_or(now)).as_secs_f32();
                self.last_frame = Some(now);
                self.elapsed += delta;

                // The group itself orbits gently — proof the mask and its
                // content are positioned through `Artboard::world_transform`
                // (composed through the group's own transform), not drawn at
                // fixed screen coordinates.
                let artboard = self.scene.document.artboard_mut(self.scene.artboard_id).unwrap();
                artboard.node_mut(self.scene.group_id).unwrap().transform.translation =
                    Vec2::new(self.elapsed.cos() * 40.0, self.elapsed.sin() * 25.0);

                // Stripes still slide within the group's local space, same
                // as `mask.rs` — `world_transform` composes that local
                // offset with the group's own orbiting translation above.
                for (i, &stripe_id) in self.scene.stripe_ids.iter().enumerate() {
                    let phase = i as f32 * 2.0;
                    let x = (self.elapsed * (0.6 + i as f32 * 0.2) + phase).sin() * 120.0;
                    artboard.node_mut(stripe_id).unwrap().transform.translation.x = x;
                }

                let artboard = self.scene.document.artboard(self.scene.artboard_id).unwrap();
                let mask_transform = artboard.world_transform(
                    match artboard.node(self.scene.group_id).unwrap().kind {
                        NodeKind::Group(GroupNode { mask: Some(mask_id), .. }) => mask_id,
                        _ => unreachable!("built with a mask in build_scene"),
                    },
                );
                let mask = DrawCall {
                    mesh: &meshes.mask_mesh,
                    binding: &meshes.mask_binding,
                    transform: mask_transform,
                    fill_color: [1.0, 1.0, 1.0, 1.0],
                    blend_mode: BlendMode::Normal, gradient: None, texture: None,
                    material: None,
                };

                let content: Vec<DrawCall> = self
                    .scene
                    .stripe_ids
                    .iter()
                    .zip(&meshes.stripe_meshes)
                    .zip(&meshes.stripe_bindings)
                    .map(|((&stripe_id, mesh), binding)| {
                        let node = artboard.node(stripe_id).unwrap();
                        let NodeKind::Shape(shape) = &node.kind else {
                            unreachable!("stripes are always Shape nodes")
                        };
                        DrawCall {
                            mesh,
                            binding,
                            transform: artboard.world_transform(stripe_id),
                            fill_color: shape.fill_color,
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
                    Err(err) => eprintln!("an1mbyt3 mask_group: render failed: {err:?}"),
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
