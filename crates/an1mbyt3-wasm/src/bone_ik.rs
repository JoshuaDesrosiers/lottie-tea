//! Browser port of `an1mbyt3-constraints`'s native `bone_ik` example: a
//! two-bone arm, posed every frame by `solve_two_bone_ik`, following a
//! target the caller sets (the native example reads `WindowEvent::CursorMoved`
//! directly; here JS reads `mousemove` and calls [`WasmBoneIk::set_target`]).
//!
//! Exists to stress a different part of the rendering pipeline than
//! [`crate::WasmPlayer`] does: two `NodeKind::Bone` nodes parented to each
//! other (`Artboard::world_transform`'s recursive parent-chain composition),
//! two simultaneous `DrawCall`s each needing their own [`DrawBinding`] (the
//! exact sharing bug `an1mbyt3-renderer`'s `GpuMesh`/`DrawBinding` split was
//! built to prevent — see `renderer.rs`'s doc comment), and continuous
//! per-frame IK solving rather than a canned keyframed clip. If the WASM
//! pipeline has a gap the static-square `WasmPlayer` demo doesn't exercise,
//! this is the first place it would show up.

use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use glam::Vec2;
use an1mbyt3_constraints::solve_two_bone_ik;
use an1mbyt3_core::{Artboard, ArtboardId, BoneNode, Document, Node, NodeId, NodeKind, Path, PathCommand, Transform2D};
use an1mbyt3_geometry::tessellate_path;
use an1mbyt3_renderer::{BlendMode, Camera, DrawBinding, DrawCall, GpuMesh, Renderer};

const ARTBOARD_WIDTH: f32 = 500.0;
const ARTBOARD_HEIGHT: f32 = 500.0;
const ROOT_ORIGIN: Vec2 = Vec2::new(-150.0, 0.0);
const ROOT_LENGTH: f32 = 110.0;
const MID_LENGTH: f32 = 90.0;
const BONE_WIDTH: f32 = 22.0;

/// Same slim-quad-from-one-end bone shape as the native example's
/// `bone_path` — see that file's doc comment for why a bone's origin is one
/// end, not centered.
fn bone_path(length: f32, width: f32) -> Path {
    let half_width = width / 2.0;
    Path {
        commands: vec![
            PathCommand::MoveTo(Vec2::new(0.0, -half_width)),
            PathCommand::LineTo(Vec2::new(length, -half_width)),
            PathCommand::LineTo(Vec2::new(length, half_width)),
            PathCommand::LineTo(Vec2::new(0.0, half_width)),
            PathCommand::Close,
        ],
    }
}

fn build_scene() -> (Document, ArtboardId, NodeId, NodeId) {
    let mut document = Document::new();
    let artboard_id = document.add_artboard(Artboard::new("Main", ARTBOARD_WIDTH, ARTBOARD_HEIGHT));

    let artboard = document.artboard_mut(artboard_id).unwrap();
    let mut root_node = Node::new("Root bone", None, NodeKind::Bone(BoneNode { length: ROOT_LENGTH }));
    root_node.transform.translation = ROOT_ORIGIN;
    let root_bone = artboard.add_node(root_node);

    let mut mid_node = Node::new("Mid bone", Some(root_bone), NodeKind::Bone(BoneNode { length: MID_LENGTH }));
    mid_node.transform.translation = Vec2::new(ROOT_LENGTH, 0.0);
    let mid_bone = artboard.add_node(mid_node);

    (document, artboard_id, root_bone, mid_bone)
}

/// Browser-side two-bone IK arm. One instance per `<canvas>`, same lifecycle
/// shape as [`crate::WasmPlayer`] (async `create`, `resize`, per-frame
/// `render`) — see that struct's doc comment for why there's no
/// `std::time::Instant` here either (IK solving is stateless per frame
/// anyway; `render` takes no `delta_seconds` because nothing here is
/// time-integrated, unlike `WasmPlayer`'s state-machine playback clock).
#[wasm_bindgen]
pub struct WasmBoneIk {
    renderer: Renderer,
    root_mesh: GpuMesh,
    mid_mesh: GpuMesh,
    root_binding: DrawBinding,
    mid_binding: DrawBinding,
    document: Document,
    artboard_id: ArtboardId,
    root_bone: NodeId,
    mid_bone: NodeId,
    /// Artboard-space target (origin centered, +y up) — see [`Self::set_target`]
    /// for the screen-to-artboard conversion JS coordinates go through.
    target: Vec2,
    /// See `WasmPlayer`'s field of the same name in `lib.rs` for why this is
    /// computed once at `create` time rather than every frame.
    clear_color: wgpu::Color,
}

#[wasm_bindgen]
impl WasmBoneIk {
    /// See [`crate::WasmPlayer::create`] for why this is a static factory
    /// rather than `#[wasm_bindgen(constructor)]`.
    pub async fn create(canvas: HtmlCanvasElement) -> Result<WasmBoneIk, JsValue> {
        let width = canvas.width();
        let height = canvas.height();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|err| JsValue::from_str(&format!("failed to create wgpu surface: {err}")))?;

        let renderer = Renderer::new_transparent(&instance, surface, width, height).await;
        let clear_color = if renderer.supports_transparency() {
            wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }
        } else {
            wgpu::Color { r: 0.08, g: 0.08, b: 0.1, a: 1.0 }
        };

        let (document, artboard_id, root_bone, mid_bone) = build_scene();

        // Same width/color for both segments as the native example — two
        // rigid bones still can't bend as one continuous shape (that needs
        // real skinning, not built for the web target yet), but matching
        // width/color hides the seam at the joint well enough to read as one
        // continuous arm.
        let root_mesh_data = tessellate_path(&bone_path(ROOT_LENGTH, BONE_WIDTH));
        let mid_mesh_data = tessellate_path(&bone_path(MID_LENGTH, BONE_WIDTH));
        let root_mesh = renderer.upload_mesh(&root_mesh_data);
        let mid_mesh = renderer.upload_mesh(&mid_mesh_data);
        let root_binding = renderer.create_draw_binding();
        let mid_binding = renderer.create_draw_binding();

        Ok(WasmBoneIk {
            renderer,
            root_mesh,
            mid_mesh,
            root_binding,
            mid_binding,
            document,
            artboard_id,
            root_bone,
            mid_bone,
            target: Vec2::new(150.0, 0.0),
            clear_color,
        })
    }

    /// Resizes the underlying swap chain — see [`crate::WasmPlayer::resize`].
    pub fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
    }

    /// Sets the IK target from canvas-space pixel coordinates (origin
    /// top-left, +y down — what `mousemove`'s `offsetX`/`offsetY` give JS),
    /// converting to artboard space (origin centered, +y up) the same way
    /// the native example's `CursorMoved` handler converts window pixels —
    /// this canvas is sized 1:1 with the artboard (`ARTBOARD_WIDTH`/`HEIGHT`),
    /// so no additional scale factor is needed.
    pub fn set_target(&mut self, canvas_x: f32, canvas_y: f32) {
        self.target = Vec2::new(canvas_x - ARTBOARD_WIDTH / 2.0, ARTBOARD_HEIGHT / 2.0 - canvas_y);
    }

    /// Solves IK for the current target, poses both bones, and draws one
    /// frame — the per-frame body of the native example's `RedrawRequested`
    /// handler, minus the winit plumbing.
    pub fn render(&mut self) -> Result<(), JsValue> {
        let solution = solve_two_bone_ik(ROOT_ORIGIN, ROOT_LENGTH, MID_LENGTH, self.target);

        let artboard = self.document.artboard_mut(self.artboard_id).unwrap();
        artboard.node_mut(self.root_bone).unwrap().transform.rotation = solution.root_rotation;
        artboard.node_mut(self.mid_bone).unwrap().transform.rotation = solution.mid_local_rotation;

        let root_world: Transform2D = artboard.world_transform(self.root_bone);
        let mid_world: Transform2D = artboard.world_transform(self.mid_bone);

        let draws = [
            DrawCall {
                mesh: &self.root_mesh,
                binding: &self.root_binding,
                transform: root_world,
                fill_color: [0.85, 0.2, 0.35, 1.0],
                blend_mode: BlendMode::Normal, gradient: None, texture: None,
                material: None,
            },
            DrawCall {
                mesh: &self.mid_mesh,
                binding: &self.mid_binding,
                transform: mid_world,
                fill_color: [0.85, 0.2, 0.35, 1.0],
                blend_mode: BlendMode::Normal, gradient: None, texture: None,
                material: None,
            },
        ];

        self.renderer
            .render((ARTBOARD_WIDTH, ARTBOARD_HEIGHT), Camera::default(), self.clear_color, &draws)
            .map_err(|err| JsValue::from_str(&format!("render failed: {err:?}")))
    }
}
