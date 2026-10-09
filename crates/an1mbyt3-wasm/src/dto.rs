//! JS-boundary data-transfer types for `editor.rs`'s `WasmEditor` — the
//! wasm-bindgen-boundary counterpart to `apps/editor/src-tauri/src/dto.rs`'s
//! Tauri-IPC-boundary DTOs (same shapes, on purpose — see this crate's
//! `editor.rs` module doc for why the frontend's existing TypeScript types
//! didn't need to change when the editor's document/render logic moved from
//! a native Tauri process into this wasm module).
//!
//! Same two reasons that DTO layer existed apply here, not just there:
//! - `Document`/`Artboard` hold nodes in a `slotmap::SlotMap`, whose own
//!   `Serialize` impl emits its *internal* slot array (including
//!   vacant/tombstoned slots), not a clean id-to-value map.
//! - `Transform2D`'s `translation`/`scale` are `glam::Vec2`, which
//!   serializes as a bare `[x, y]` tuple, not `{x, y}` — the exact bug that
//!   first broke the Tauri IPC bridge before `Vec2Dto` existed.
//!
//! `NodeId`/`ArtboardId` *by themselves* are still the one exception fine to
//! serialize directly (via `serde_wasm_bindgen`) — a plain `{idx, version}`
//! struct, only ever an opaque token the frontend stores and echoes back.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use an1mbyt3_core::{
    Artboard, AnimationId, ArtboardId, BlockProgramDef, BlockScript, ComponentDefinition, ComponentId, ConditionDef, Document, Easing, EventKind, Expr,
    GraphEdgeDef, GraphNodeDef, GraphNodeKind, GraphProperty, GradientDef, GradientKind, ImageId, InputDef, ListExpr, LoopMode, MathFn, NodeGraphDef, NodeId,
    NodeKind, NodeRef, PlaybackDirection, PostEffect, PostEffectLayer, ProgramDef, PropertyTrack, SocketOverride, StateDef, StateMachineDef, StateMachineId,
    Statement, Transform2D,
};
use an1mbyt3_runtime::StateMachine;

/// Fallback local-space half-extent for node kinds with no authored path to
/// measure (`Group`/`Instance`) — just enough for the on-canvas transform
/// tool's selection handles to have *something* to sit on. Nothing in the
/// editor's toolbar creates these kinds yet (see `ArtboardSummary::nodes`'s
/// doc comment), so getting this pixel-perfect isn't worth it until it does.
const DEFAULT_LOCAL_HALF_EXTENT: f32 = 50.0;

/// Half-size (world units) of a `Camera` node's on-canvas grab box — the
/// Unity-camera-gizmo-icon analog: a camera has no geometry of its own, so
/// this small square at its center is what selection handles sit on *and*
/// what `WasmEditor::pick_node_at` hit-tests, so clicking/dragging the camera
/// icon on the canvas works like dragging any shape. Shared by both so the
/// selection box and the clickable area can never disagree.
pub(crate) const CAMERA_GIZMO_HALF_EXTENT: f32 = 24.0;
/// Selection/gizmo half-size for things with no drawn footprint of their
/// own in the editor (a Global light, a Trail's emitter point).
pub(crate) const SMALL_GIZMO_HALF_EXTENT: f32 = 12.0;

/// Approximate visual width of a bone's slim-quad shape, for the same
/// selection-handle purpose as `DEFAULT_LOCAL_HALF_EXTENT` — matches the
/// demo bone shape's own width (`crates/an1mbyt3-wasm/src/bone_ik.rs`'s
/// `BONE_WIDTH`) since `BoneNode` itself only stores `length`, not a visual
/// width, to draw from.
const BONE_VISUAL_WIDTH: f32 = 22.0;

/// A node's local-space (pre-transform) bounding box, as `(min, max)` — what
/// the on-canvas transform tool's selection outline and corner/rotate
/// handles are built from (`TransformHandles.tsx` transforms these corners
/// through the node's own `Transform2D` to place them in world space). Real
/// geometry (`ShapeNode::path.bounds()`) for the one kind that actually has
/// authored geometry today; a documented best-effort approximation for
/// every other kind (see `DEFAULT_LOCAL_HALF_EXTENT`/`BONE_VISUAL_WIDTH`).
/// Every non-deleted node id in `artboard`, depth-first, parent immediately
/// followed by its own children (`Artboard::children_of`'s insertion-order
/// scan) — what lets `summarize_document`'s `nodes` list show a bone chain's
/// nested second/third bone at all, not just root nodes (see the doc comment
/// at that call site). A deleted node's whole subtree is skipped, not just
/// the node itself — an orphaned nested node with no visible parent in the
/// list would be worse than not listing it.
fn ordered_node_ids(artboard: &Artboard) -> Vec<NodeId> {
    fn walk(artboard: &Artboard, id: NodeId, out: &mut Vec<NodeId>) {
        if artboard.node(id).is_some_and(|n| n.deleted) {
            return;
        }
        out.push(id);
        for (child_id, _) in artboard.children_of(id) {
            walk(artboard, child_id, out);
        }
    }

    let mut out = Vec::new();
    for &root_id in artboard.root_nodes() {
        walk(artboard, root_id, &mut out);
    }
    out
}

/// A node's local-space (pre-transform) bounding box — see the doc comment
/// above this function's call site in `summarize_document` for what it's
/// used for. Takes `(artboard, node_id)` rather than just `&Node` because
/// `NodeKind::Group` needs to recurse into `Artboard::children_of` to
/// compute its *real content* bounds (`group_content_bounds`) — a group has
/// no geometry of its own, so its selection box has to come from whatever's
/// actually inside it, not a fixed placeholder (see that function's own doc
/// comment for why a placeholder box was actively wrong: ungrabbable,
/// disconnected from the group's visible content, and made rotate/scale spin
/// everything around a corner of empty space instead of the group's own
/// content).
pub(crate) fn local_bounds(artboard: &Artboard, node_id: NodeId) -> (Vec2, Vec2) {
    let node = artboard.node(node_id).expect("local_bounds: node not found");
    match &node.kind {
        NodeKind::Shape(shape) => shape.path.bounds(),
        NodeKind::Bone(bone) => (Vec2::new(0.0, -BONE_VISUAL_WIDTH / 2.0), Vec2::new(bone.length, BONE_VISUAL_WIDTH / 2.0)),
        NodeKind::Text(text) => {
            // No authored path to measure (glyph meshes are a derived,
            // unsaved artifact — see `TextNode`'s doc comment) — a rough
            // monospace-ish estimate from character count and font size,
            // symmetric around the origin like every other approximation
            // here, since the actual glyph-anchoring convention isn't worth
            // guessing at just for a selection box.
            let half_width = (text.content.chars().count().max(1) as f32) * text.size_px * 0.3;
            let half_height = text.size_px * 0.6;
            (Vec2::new(-half_width, -half_height), Vec2::new(half_width, half_height))
        }
        NodeKind::Instance(_) => (Vec2::splat(-DEFAULT_LOCAL_HALF_EXTENT), Vec2::splat(DEFAULT_LOCAL_HALF_EXTENT)),
        // A GameObject whose job is a component (Camera / Light 2D / ...):
        // its gizmo's extent, since it has no art of its own.
        NodeKind::Group(_) if node.camera().is_some() => (Vec2::splat(-CAMERA_GIZMO_HALF_EXTENT), Vec2::splat(CAMERA_GIZMO_HALF_EXTENT)),
        NodeKind::Group(_) if node.light2d().is_some() => node
            .light2d()
            .and_then(|l| l.local_extent())
            .unwrap_or((Vec2::splat(-SMALL_GIZMO_HALF_EXTENT), Vec2::splat(SMALL_GIZMO_HALF_EXTENT))),
        NodeKind::Group(_) if node.behaviors.iter().any(|e| matches!(e.behavior, an1mbyt3_core::NodeBehavior::UiHtml(_))) => {
            let half = node
                .behaviors
                .iter()
                .find_map(|e| match &e.behavior {
                    an1mbyt3_core::NodeBehavior::UiHtml(h) => Some(Vec2::new(h.width, h.height) * 0.5),
                    _ => None,
                })
                .unwrap_or(Vec2::splat(SMALL_GIZMO_HALF_EXTENT));
            (-half, half)
        }
        NodeKind::Group(_) if !node.behaviors.is_empty() && artboard.children_of(node_id).next().is_none() => {
            (Vec2::splat(-SMALL_GIZMO_HALF_EXTENT), Vec2::splat(SMALL_GIZMO_HALF_EXTENT))
        }
        NodeKind::Group(_) => group_content_bounds(artboard, node_id),
        NodeKind::Image(image) => (Vec2::new(-image.width / 2.0, -image.height / 2.0), Vec2::new(image.width / 2.0, image.height / 2.0)),
        // No authored path/visual footprint of its own (its live particles
        // are runtime-only, not part of this static local-bounds notion at
        // all — see `an1mbyt3_core::particle`'s own module doc) — same
        // generic placeholder extent `Instance` already uses, just enough
        // for the on-canvas selection handles to have something to sit on.
        NodeKind::ParticleSystem(_) => (Vec2::splat(-DEFAULT_LOCAL_HALF_EXTENT), Vec2::splat(DEFAULT_LOCAL_HALF_EXTENT)),
        NodeKind::Tilemap(tilemap) => {
            let half = Vec2::new(tilemap.map_width as f32 * tilemap.cell_size / 2.0, tilemap.map_height as f32 * tilemap.cell_size / 2.0);
            (-half, half)
        }
        // No visual footprint of its own (a camera draws nothing — see
        // `an1mbyt3_core::CameraNode`'s own doc comment) — same generic
        // placeholder extent `Instance`/`ParticleSystem` already use, just
        // enough for the on-canvas selection handles to have something to
        // sit on.
        NodeKind::Camera(_) => (Vec2::splat(-CAMERA_GIZMO_HALF_EXTENT), Vec2::splat(CAMERA_GIZMO_HALF_EXTENT)),
        // A light's box is what it can reach (so the selection handles hug
        // its falloff/shape); a Global light has no extent, just a gizmo.
        NodeKind::Light2D(light) => light.local_extent().unwrap_or((Vec2::splat(-SMALL_GIZMO_HALF_EXTENT), Vec2::splat(SMALL_GIZMO_HALF_EXTENT))),
        // The ribbon itself lives in world space (runtime-only points); the
        // node is just the emitter, so it gets a small grab box.
        NodeKind::Trail(_) => (Vec2::splat(-SMALL_GIZMO_HALF_EXTENT), Vec2::splat(SMALL_GIZMO_HALF_EXTENT)),
        NodeKind::Node3D(_) => (Vec2::splat(-SMALL_GIZMO_HALF_EXTENT), Vec2::splat(SMALL_GIZMO_HALF_EXTENT)),
    }
}

/// A `Group`'s real content bounding box, in the group's *own* local
/// coordinate frame — the union of every descendant's own local bounds,
/// each first transformed by `Artboard::transform_relative_to(descendant,
/// group_id)` (so a nested child's position/rotation/scale *within* the
/// group composes correctly, without pulling in the group's own transform —
/// `TransformHandles.tsx` composes that separately, the same way it already
/// does for any other node kind). Recurses through nested groups via
/// `local_bounds` itself (mutual recursion), so a group-of-groups still gets
/// one correctly unioned box. Skips deleted descendants (`Command::SetDeleted`'s
/// soft-delete convention — a deleted subtree shouldn't inflate its
/// still-visible parent's selection box). Falls back to
/// `DEFAULT_LOCAL_HALF_EXTENT` only for a genuinely empty group (no
/// non-deleted descendants at all) — there's nothing else it could mean.
fn group_content_bounds(artboard: &Artboard, group_id: NodeId) -> (Vec2, Vec2) {
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);

    for (child_id, child) in artboard.children_of(group_id) {
        if child.deleted {
            continue;
        }
        let relative = artboard.transform_relative_to(child_id, group_id).to_affine2();
        let (child_min, child_max) = local_bounds(artboard, child_id);
        for corner in [
            Vec2::new(child_min.x, child_min.y),
            Vec2::new(child_max.x, child_min.y),
            Vec2::new(child_max.x, child_max.y),
            Vec2::new(child_min.x, child_max.y),
        ] {
            let world = relative.transform_point2(corner);
            min = min.min(world);
            max = max.max(world);
        }
    }

    if !min.x.is_finite() {
        return (Vec2::splat(-DEFAULT_LOCAL_HALF_EXTENT), Vec2::splat(DEFAULT_LOCAL_HALF_EXTENT));
    }
    (min, max)
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct Vec2Dto {
    pub x: f32,
    pub y: f32,
}

impl From<glam::Vec2> for Vec2Dto {
    fn from(v: glam::Vec2) -> Self {
        Self { x: v.x, y: v.y }
    }
}

impl From<Vec2Dto> for glam::Vec2 {
    fn from(v: Vec2Dto) -> Self {
        Self::new(v.x, v.y)
    }
}

/// Mirrors `an1mbyt3_core::GradientStop`.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradientStopDto {
    pub offset: f32,
    pub color: [f32; 4],
    /// Mirrors `an1mbyt3_core::GradientStop::midpoint`.
    pub midpoint: f32,
}

/// Mirrors `an1mbyt3_core::GradientDef` — the Fill tool's gradient
/// authoring shape. `kind` reuses `an1mbyt3_core::GradientKind` directly —
/// a plain fieldless enum, whose default `Serialize` output (the bare
/// variant name, e.g. `"Linear"`) is already the TS string-literal union
/// shape wants, same convention `LoopMode`/`PlaybackDirection` reuse
/// directly elsewhere in this file.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradientDefDto {
    pub kind: GradientKind,
    pub start: Vec2Dto,
    pub end: Vec2Dto,
    pub stops: Vec<GradientStopDto>,
}

impl From<&GradientDef> for GradientDefDto {
    fn from(g: &GradientDef) -> Self {
        Self {
            kind: g.kind,
            start: g.start.into(),
            end: g.end.into(),
            stops: g.stops.iter().map(|s| GradientStopDto { offset: s.offset, color: s.color, midpoint: s.midpoint }).collect(),
        }
    }
}

impl From<GradientDefDto> for GradientDef {
    fn from(g: GradientDefDto) -> Self {
        Self {
            kind: g.kind,
            start: g.start.into(),
            end: g.end.into(),
            stops: g.stops.into_iter().map(|s| an1mbyt3_core::GradientStop { offset: s.offset, color: s.color, midpoint: s.midpoint }).collect(),
        }
    }
}

#[derive(Serialize)]
pub struct Transform2DDto {
    pub translation: Vec2Dto,
    /// Radians, same unit `an1mbyt3_core::Transform2D` uses internally —
    /// converting to degrees is a display concern, done on the frontend.
    pub rotation: f32,
    pub scale: Vec2Dto,
    /// Local-space pivot point — see `an1mbyt3_core::Transform2D::anchor`'s
    /// doc comment. `(0, 0)` for every node that hasn't had its anchor
    /// moved, which reproduces the pre-anchor rendering exactly.
    pub anchor: Vec2Dto,
}

impl From<Transform2D> for Transform2DDto {
    fn from(t: Transform2D) -> Self {
        Self {
            translation: t.translation.into(),
            rotation: t.rotation,
            scale: t.scale.into(),
            anchor: t.anchor.into(),
        }
    }
}

/// Mirrors `an1mbyt3_core::PathCommand` across the wasm-bindgen boundary —
/// internally tagged the same way `KeyframeValueDto` mirrors `KeyframeValue`.
/// `Deserialize` too: this crosses the boundary in both directions (the
/// Timeline reads a `Path` track's keyframe values out; the path-editing
/// overlay sends an edited `Path` back in).
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum PathCommandDto {
    MoveTo(Vec2Dto),
    LineTo(Vec2Dto),
    CubicTo { control1: Vec2Dto, control2: Vec2Dto, to: Vec2Dto },
    Close,
}

impl From<&an1mbyt3_core::PathCommand> for PathCommandDto {
    fn from(c: &an1mbyt3_core::PathCommand) -> Self {
        match *c {
            an1mbyt3_core::PathCommand::MoveTo(p) => PathCommandDto::MoveTo(p.into()),
            an1mbyt3_core::PathCommand::LineTo(p) => PathCommandDto::LineTo(p.into()),
            an1mbyt3_core::PathCommand::CubicTo { control1, control2, to } => {
                PathCommandDto::CubicTo { control1: control1.into(), control2: control2.into(), to: to.into() }
            }
            an1mbyt3_core::PathCommand::Close => PathCommandDto::Close,
        }
    }
}

impl From<PathCommandDto> for an1mbyt3_core::PathCommand {
    fn from(c: PathCommandDto) -> Self {
        match c {
            PathCommandDto::MoveTo(p) => an1mbyt3_core::PathCommand::MoveTo(p.into()),
            PathCommandDto::LineTo(p) => an1mbyt3_core::PathCommand::LineTo(p.into()),
            PathCommandDto::CubicTo { control1, control2, to } => {
                an1mbyt3_core::PathCommand::CubicTo { control1: control1.into(), control2: control2.into(), to: to.into() }
            }
            PathCommandDto::Close => an1mbyt3_core::PathCommand::Close,
        }
    }
}

/// Mirrors `an1mbyt3_core::Path` across the wasm-bindgen boundary.
#[derive(Clone, Serialize, Deserialize)]
pub struct PathDto {
    pub commands: Vec<PathCommandDto>,
}

impl From<&an1mbyt3_core::Path> for PathDto {
    fn from(p: &an1mbyt3_core::Path) -> Self {
        Self { commands: p.commands.iter().map(PathCommandDto::from).collect() }
    }
}

impl From<PathDto> for an1mbyt3_core::Path {
    fn from(p: PathDto) -> Self {
        Self { commands: p.commands.into_iter().map(Into::into).collect() }
    }
}

/// Mirrors `an1mbyt3_core::Stroke` — the Inspector's Stroke section.
/// `cap`/`join` reuse `an1mbyt3_core::StrokeCap`/`StrokeJoin` directly (plain
/// fieldless enums, whose default `Serialize` output is already the TS
/// string-literal union shape wants), same convention `GradientKind` already
/// establishes elsewhere in this file.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrokeDto {
    pub width: f32,
    pub color: [f32; 4],
    pub gradient: Option<GradientDefDto>,
    pub image: Option<ImageId>,
    pub cap: an1mbyt3_core::StrokeCap,
    pub join: an1mbyt3_core::StrokeJoin,
}

/// Mirrors `an1mbyt3_core::ParticleMain` — needs a wrapper (unlike
/// `ParticleRange`/`ParticleBurst`/`EmitterShape`, every one of which is
/// reused directly: none of *their* own field names are multi-word, so
/// there's nothing for `rename_all` to actually change) because several of
/// this struct's own fields are (`start_lifetime`, `rate_over_time`-shaped
/// names throughout this module), and that attribute can only be applied at
/// a struct's own definition, not from a different crate at the call site.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticleMainDto {
    pub duration: f32,
    pub looping: bool,
    pub start_lifetime: an1mbyt3_core::ParticleRange,
    pub start_speed: an1mbyt3_core::ParticleRange,
    pub start_size: an1mbyt3_core::ParticleRange,
    pub start_rotation: an1mbyt3_core::ParticleRange,
    pub start_color: [f32; 4],
    pub gravity_modifier: f32,
    pub drag: f32,
    pub simulation_space: an1mbyt3_core::SimulationSpace,
    pub max_particles: u32,
    pub play_on_awake: bool,
}

impl From<&an1mbyt3_core::ParticleMain> for ParticleMainDto {
    fn from(m: &an1mbyt3_core::ParticleMain) -> Self {
        Self {
            duration: m.duration,
            looping: m.looping,
            start_lifetime: m.start_lifetime,
            start_speed: m.start_speed,
            start_size: m.start_size,
            start_rotation: m.start_rotation,
            start_color: m.start_color,
            gravity_modifier: m.gravity_modifier,
            drag: m.drag,
            simulation_space: m.simulation_space,
            max_particles: m.max_particles,
            play_on_awake: m.play_on_awake,
        }
    }
}

impl From<ParticleMainDto> for an1mbyt3_core::ParticleMain {
    fn from(d: ParticleMainDto) -> Self {
        Self {
            duration: d.duration,
            looping: d.looping,
            start_lifetime: d.start_lifetime,
            start_speed: d.start_speed,
            start_size: d.start_size,
            start_rotation: d.start_rotation,
            start_color: d.start_color,
            gravity_modifier: d.gravity_modifier,
            drag: d.drag,
            simulation_space: d.simulation_space,
            max_particles: d.max_particles,
            play_on_awake: d.play_on_awake,
        }
    }
}

/// Mirrors `an1mbyt3_core::ParticleEmission` — same "needs the wrapper
/// purely for `rate_over_time`'s own camelCase" reasoning as `ParticleMainDto`.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticleEmissionDto {
    pub rate_over_time: f32,
    pub bursts: Vec<an1mbyt3_core::ParticleBurst>,
}

impl From<&an1mbyt3_core::ParticleEmission> for ParticleEmissionDto {
    fn from(e: &an1mbyt3_core::ParticleEmission) -> Self {
        Self { rate_over_time: e.rate_over_time, bursts: e.bursts.clone() }
    }
}

impl From<ParticleEmissionDto> for an1mbyt3_core::ParticleEmission {
    fn from(d: ParticleEmissionDto) -> Self {
        Self { rate_over_time: d.rate_over_time, bursts: d.bursts }
    }
}

/// Mirrors `an1mbyt3_core::SizeKeyframe` — same reasoning as
/// `ParticleMainDto`, for `size_multiplier`'s own camelCase.
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct SizeKeyframeDto {
    pub t: f32,
    pub size_multiplier: f32,
}

impl From<an1mbyt3_core::SizeKeyframe> for SizeKeyframeDto {
    fn from(k: an1mbyt3_core::SizeKeyframe) -> Self {
        Self { t: k.t, size_multiplier: k.size_multiplier }
    }
}

impl From<SizeKeyframeDto> for an1mbyt3_core::SizeKeyframe {
    fn from(d: SizeKeyframeDto) -> Self {
        Self { t: d.t, size_multiplier: d.size_multiplier }
    }
}

/// Mirrors `an1mbyt3_core::ParticleSystemNode` — the Inspector's Particle
/// System section reads/writes this whole, in one piece, via
/// `WasmEditor::set_particle_system` — same "one nested object field, not a
/// dozen flattened scalars" convention `NodeSummary::path`/`stroke` already
/// use for their own all-at-once-edited sub-structures.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticleSystemDto {
    pub main: ParticleMainDto,
    pub emission: ParticleEmissionDto,
    pub shape: an1mbyt3_core::EmitterShape,
    pub color_over_lifetime: Vec<GradientStopDto>,
    pub size_over_lifetime: Vec<SizeKeyframeDto>,
    pub rotation_speed: f32,
}

impl From<&an1mbyt3_core::ParticleSystemNode> for ParticleSystemDto {
    fn from(p: &an1mbyt3_core::ParticleSystemNode) -> Self {
        Self {
            main: (&p.main).into(),
            emission: (&p.emission).into(),
            shape: p.shape,
            color_over_lifetime: p.color_over_lifetime.iter().map(|s| GradientStopDto { offset: s.offset, color: s.color, midpoint: s.midpoint }).collect(),
            size_over_lifetime: p.size_over_lifetime.iter().map(|k| (*k).into()).collect(),
            rotation_speed: p.rotation_speed,
        }
    }
}

impl From<ParticleSystemDto> for an1mbyt3_core::ParticleSystemNode {
    fn from(d: ParticleSystemDto) -> Self {
        Self {
            main: d.main.into(),
            emission: d.emission.into(),
            shape: d.shape,
            color_over_lifetime: d
                .color_over_lifetime
                .into_iter()
                .map(|s| an1mbyt3_core::GradientStop { offset: s.offset, color: s.color, midpoint: s.midpoint })
                .collect(),
            size_over_lifetime: d.size_over_lifetime.into_iter().map(|k| k.into()).collect(),
            rotation_speed: d.rotation_speed,
        }
    }
}

/// Mirrors `an1mbyt3_core::TilemapNode` — read whole by the Tilemap panel
/// (tileset/grid/cell-size metadata, plus the full `tiles` array to paint
/// the palette/preview from), but written one cell at a time via
/// `WasmEditor::set_tile`/`Command::SetTile`, not by sending this whole
/// struct back — the "one edit per cell click" granularity `Command::SetTile`'s
/// own doc comment explains, unlike `ParticleSystemDto`'s "replace the whole
/// thing" shape.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TilemapDto {
    pub tileset: an1mbyt3_core::ImageId,
    pub tileset_width_px: u32,
    pub tileset_height_px: u32,
    pub tile_width_px: u32,
    pub tile_height_px: u32,
    pub map_width: u32,
    pub map_height: u32,
    pub cell_size: f32,
    pub tiles: Vec<Option<u32>>,
}

/// Mirrors `an1mbyt3_core::CameraNode` — the Inspector's Camera section.
/// Read whole and written whole (`WasmEditor::set_camera`), same shape as
/// `ParticleSystemDto`.
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct CameraDto {
    pub zoom: f32,
    pub background_color: [f32; 4],
    pub follow_target: Option<NodeId>,
    pub follow_smoothing: f32,
    pub vignette: CameraVignetteDto,
    pub pixel_perfect: PixelPerfectDto,
}

/// Mirrors `an1mbyt3_core::PixelPerfect` (enums travel as variant names).
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct PixelPerfectDto {
    pub enabled: bool,
    pub grid_snapping: an1mbyt3_core::PixelGridSnapping,
    pub crop_frame: an1mbyt3_core::PixelCropFrame,
}

/// Mirrors `an1mbyt3_core::CameraVignette`.
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct CameraVignetteDto {
    pub enabled: bool,
    pub color: [f32; 4],
    pub intensity: f32,
    pub radius: f32,
    pub blur: f32,
}

impl From<&an1mbyt3_core::CameraNode> for CameraDto {
    fn from(c: &an1mbyt3_core::CameraNode) -> Self {
        let v = c.vignette;
        Self {
            zoom: c.zoom,
            background_color: c.background_color,
            follow_target: c.follow_target,
            follow_smoothing: c.follow_smoothing,
            vignette: CameraVignetteDto { enabled: v.enabled, color: v.color, intensity: v.intensity, radius: v.radius, blur: v.blur },
            pixel_perfect: PixelPerfectDto {
                enabled: c.pixel_perfect.enabled,
                grid_snapping: c.pixel_perfect.grid_snapping,
                crop_frame: c.pixel_perfect.crop_frame,
            },
        }
    }
}

impl From<CameraDto> for an1mbyt3_core::CameraNode {
    fn from(c: CameraDto) -> Self {
        let v = c.vignette;
        Self {
            zoom: c.zoom,
            background_color: c.background_color,
            follow_target: c.follow_target,
            follow_smoothing: c.follow_smoothing,
            vignette: an1mbyt3_core::CameraVignette { enabled: v.enabled, color: v.color, intensity: v.intensity, radius: v.radius, blur: v.blur },
            pixel_perfect: an1mbyt3_core::PixelPerfect {
                enabled: c.pixel_perfect.enabled,
                grid_snapping: c.pixel_perfect.grid_snapping,
                crop_frame: c.pixel_perfect.crop_frame,
            },
        }
    }
}

/// Mirrors `an1mbyt3_core::Light2DNode` — the Inspector's Light 2D section,
/// read and written whole (`WasmEditor::set_light2d`). Enums travel as
/// their variant names (`"Spot"`, `"Multiply"`, ...).
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Light2DDto {
    pub light_type: an1mbyt3_core::Light2DType,
    pub color: [f32; 4],
    pub intensity: f32,
    pub blend_style: an1mbyt3_core::LightBlendStyle,
    pub overlap_operation: an1mbyt3_core::LightOverlapOperation,
    pub light_order: i32,
    pub inner_radius: f32,
    pub outer_radius: f32,
    pub inner_spot_angle: f32,
    pub outer_spot_angle: f32,
    pub falloff_strength: f32,
    pub shape_points: Vec<Vec2Dto>,
    pub falloff_size: f32,
    pub sprite: Option<an1mbyt3_core::ImageId>,
    pub sprite_width: f32,
    pub sprite_height: f32,
    pub shadows_enabled: bool,
    pub shadow_strength: f32,
    pub shadow_softness: f32,
    pub volumetric_enabled: bool,
    pub volumetric_intensity: f32,
    pub volumetric_shadows_enabled: bool,
    pub volumetric_shadow_strength: f32,
    pub falloff_steps: u32,
    pub edge_blur: f32,
    pub pixel_size: f32,
}

impl From<&an1mbyt3_core::Light2DNode> for Light2DDto {
    fn from(l: &an1mbyt3_core::Light2DNode) -> Self {
        Self {
            light_type: l.light_type,
            color: l.color,
            intensity: l.intensity,
            blend_style: l.blend_style,
            overlap_operation: l.overlap_operation,
            light_order: l.light_order,
            inner_radius: l.inner_radius,
            outer_radius: l.outer_radius,
            inner_spot_angle: l.inner_spot_angle,
            outer_spot_angle: l.outer_spot_angle,
            falloff_strength: l.falloff_strength,
            shape_points: l.shape_points.iter().map(|p| (*p).into()).collect(),
            falloff_size: l.falloff_size,
            sprite: l.sprite,
            sprite_width: l.sprite_width,
            sprite_height: l.sprite_height,
            shadows_enabled: l.shadows_enabled,
            shadow_strength: l.shadow_strength,
            shadow_softness: l.shadow_softness,
            volumetric_enabled: l.volumetric_enabled,
            volumetric_intensity: l.volumetric_intensity,
            volumetric_shadows_enabled: l.volumetric_shadows_enabled,
            volumetric_shadow_strength: l.volumetric_shadow_strength,
            falloff_steps: l.falloff_steps,
            edge_blur: l.edge_blur,
            pixel_size: l.pixel_size,
        }
    }
}

impl From<Light2DDto> for an1mbyt3_core::Light2DNode {
    fn from(d: Light2DDto) -> Self {
        Self {
            light_type: d.light_type,
            color: d.color,
            intensity: d.intensity,
            blend_style: d.blend_style,
            overlap_operation: d.overlap_operation,
            light_order: d.light_order,
            inner_radius: d.inner_radius,
            outer_radius: d.outer_radius,
            inner_spot_angle: d.inner_spot_angle,
            outer_spot_angle: d.outer_spot_angle,
            falloff_strength: d.falloff_strength,
            shape_points: d.shape_points.into_iter().map(Into::into).collect(),
            falloff_size: d.falloff_size,
            sprite: d.sprite,
            sprite_width: d.sprite_width,
            sprite_height: d.sprite_height,
            shadows_enabled: d.shadows_enabled,
            shadow_strength: d.shadow_strength,
            shadow_softness: d.shadow_softness,
            volumetric_enabled: d.volumetric_enabled,
            volumetric_intensity: d.volumetric_intensity,
            volumetric_shadows_enabled: d.volumetric_shadows_enabled,
            volumetric_shadow_strength: d.volumetric_shadow_strength,
            falloff_steps: d.falloff_steps,
            edge_blur: d.edge_blur,
            pixel_size: d.pixel_size,
        }
    }
}

/// Mirrors `an1mbyt3_core::TrailNode` — the Inspector's Trail Renderer
/// section, read and written whole (`WasmEditor::set_trail`).
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrailDto {
    pub time: f32,
    pub min_vertex_distance: f32,
    pub width: f32,
    pub width_curve: Vec<SizeKeyframeDto>,
    pub color_gradient: Vec<GradientStopDto>,
    pub emitting: bool,
    pub end_cap_vertices: u32,
    pub corner_vertices: u32,
    pub texture: Option<an1mbyt3_core::ImageId>,
}

impl From<&an1mbyt3_core::TrailNode> for TrailDto {
    fn from(t: &an1mbyt3_core::TrailNode) -> Self {
        Self {
            time: t.time,
            min_vertex_distance: t.min_vertex_distance,
            width: t.width,
            width_curve: t.width_curve.iter().map(|k| (*k).into()).collect(),
            color_gradient: t.color_gradient.iter().map(|s| GradientStopDto { offset: s.offset, color: s.color, midpoint: s.midpoint }).collect(),
            emitting: t.emitting,
            end_cap_vertices: t.end_cap_vertices,
            corner_vertices: t.corner_vertices,
            texture: t.texture,
        }
    }
}

impl From<TrailDto> for an1mbyt3_core::TrailNode {
    fn from(d: TrailDto) -> Self {
        Self {
            time: d.time,
            min_vertex_distance: d.min_vertex_distance,
            width: d.width,
            width_curve: d.width_curve.into_iter().map(Into::into).collect(),
            color_gradient: d.color_gradient.into_iter().map(|s| an1mbyt3_core::GradientStop { offset: s.offset, color: s.color, midpoint: s.midpoint }).collect(),
            emitting: d.emitting,
            end_cap_vertices: d.end_cap_vertices,
            corner_vertices: d.corner_vertices,
            texture: d.texture,
        }
    }
}

/// `WasmEditor::physics_raycast`'s hit.
/// `physics3d_raycast`'s result.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RayHit3Dto {
    pub node: NodeId,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub normal_x: f32,
    pub normal_y: f32,
    pub normal_z: f32,
    pub distance: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RayHitDto {
    pub node: NodeId,
    pub x: f32,
    pub y: f32,
    pub normal_x: f32,
    pub normal_y: f32,
    pub distance: f32,
}

/// `WasmEditor::physics_events`' entries.
#[derive(Serialize)]
pub struct ContactEventDto {
    pub a: NodeId,
    pub b: NodeId,
    pub started: bool,
    pub trigger: bool,
}

/// Mirrors `an1mbyt3_core::ShadowCaster2D`.
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct ShadowCasterDto {
    pub enabled: bool,
    pub casting_option: an1mbyt3_core::ShadowCastingOption,
    pub trim_edge: f32,
}

impl From<&an1mbyt3_core::ShadowCaster2D> for ShadowCasterDto {
    fn from(c: &an1mbyt3_core::ShadowCaster2D) -> Self {
        Self { enabled: c.enabled, casting_option: c.casting_option, trim_edge: c.trim_edge }
    }
}

impl From<ShadowCasterDto> for an1mbyt3_core::ShadowCaster2D {
    fn from(d: ShadowCasterDto) -> Self {
        Self { enabled: d.enabled, casting_option: d.casting_option, trim_edge: d.trim_edge }
    }
}

impl From<&an1mbyt3_core::TilemapNode> for TilemapDto {
    fn from(t: &an1mbyt3_core::TilemapNode) -> Self {
        Self {
            tileset: t.tileset,
            tileset_width_px: t.tileset_width_px,
            tileset_height_px: t.tileset_height_px,
            tile_width_px: t.tile_width_px,
            tile_height_px: t.tile_height_px,
            map_width: t.map_width,
            map_height: t.map_height,
            cell_size: t.cell_size,
            tiles: t.tiles.clone(),
        }
    }
}

impl From<&an1mbyt3_core::Stroke> for StrokeDto {
    fn from(s: &an1mbyt3_core::Stroke) -> Self {
        Self { width: s.width, color: s.color, gradient: s.gradient.as_ref().map(GradientDefDto::from), image: s.image, cap: s.cap, join: s.join }
    }
}

impl From<StrokeDto> for an1mbyt3_core::Stroke {
    fn from(s: StrokeDto) -> Self {
        Self { width: s.width, color: s.color, gradient: s.gradient.map(Into::into), image: s.image, cap: s.cap, join: s.join }
    }
}

/// A keyframe's value, tagged with which property it belongs to — mirrors
/// `an1mbyt3_core::KeyframeValue` across the wasm-bindgen boundary.
/// Internally tagged (`{"kind": "Translation", "value": {...}}`) so the
/// frontend gets a real discriminated union, not a guess based on which
/// field happens to be present.
#[derive(Serialize)]
#[serde(tag = "kind", content = "value")]
pub enum KeyframeValueDto {
    Translation(Vec2Dto),
    Rotation(f32),
    Scale(Vec2Dto),
    FillColor([f32; 4]),
    Path(PathDto),
    MotionPath(f32),
}

/// Mirrors `an1mbyt3_core::Easing` across the wasm-bindgen boundary —
/// internally tagged on `kind` so a unit variant (`Linear`) and a struct
/// variant (`CubicBezier`) both serialize/deserialize as one discriminated
/// union (`{"kind":"Linear"}` / `{"kind":"CubicBezier","x1":...}`) the same
/// way `KeyframeValueDto` mirrors `KeyframeValue`. `Deserialize` too (unlike
/// `KeyframeValueDto`) since the Curve Editor panel sends one of these back
/// in via `WasmEditor::set_keyframe_easing`, not just reads one out.
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum EasingDto {
    Linear,
    CubicBezier { x1: f32, y1: f32, x2: f32, y2: f32 },
}

impl From<Easing> for EasingDto {
    fn from(e: Easing) -> Self {
        match e {
            Easing::Linear => EasingDto::Linear,
            Easing::CubicBezier { x1, y1, x2, y2 } => EasingDto::CubicBezier { x1, y1, x2, y2 },
        }
    }
}

impl From<EasingDto> for Easing {
    fn from(e: EasingDto) -> Self {
        match e {
            EasingDto::Linear => Easing::Linear,
            EasingDto::CubicBezier { x1, y1, x2, y2 } => Easing::CubicBezier { x1, y1, x2, y2 },
        }
    }
}

/// One authored keyframe, for the Timeline panel — mirrors
/// `an1mbyt3_core::Keyframe<T>`, including its `easing` (the curve used to
/// interpolate *out of* this keyframe toward the next — see
/// `an1mbyt3_core::Keyframe`'s own doc comment). The Timeline's "double-click
/// a keyframe to edit its curve" editor reads/writes this via
/// `WasmEditor::set_keyframe_easing`.
#[derive(Serialize)]
pub struct KeyframeDto {
    pub time: f32,
    pub value: KeyframeValueDto,
    pub easing: EasingDto,
}

/// One property's full keyframe track, for the Timeline panel's per-property
/// row. `property` is the same string tag `WasmEditor::add_keyframe`/
/// `remove_keyframe`/etc. accept back (`"Translation"`/`"Rotation"`/
/// `"Scale"`/`"FillColor"`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyTrackSummary {
    pub property: &'static str,
    /// Always in ascending `time` order (the same invariant `Track`/
    /// `Command::AddKeyframe`'s doc comments establish), so the Timeline
    /// panel can index into this directly — `preview_keyframe_time`/
    /// `commit_keyframe_time` address a keyframe by that index.
    pub keyframes: Vec<KeyframeDto>,
    /// Only present for `property: "MotionPath"` — which node's `Path` this
    /// track's progress keyframes are sampled against. `None` for every
    /// other property kind.
    pub guide: Option<NodeId>,
}

fn summarize_property_track(track: &PropertyTrack) -> PropertyTrackSummary {
    match track {
        PropertyTrack::Translation(t) => PropertyTrackSummary {
            property: "Translation",
            keyframes: t.keyframes.iter().map(|k| KeyframeDto { time: k.time, value: KeyframeValueDto::Translation(k.value.into()), easing: k.easing.into() }).collect(),
            guide: None,
        },
        PropertyTrack::Rotation(t) => PropertyTrackSummary {
            property: "Rotation",
            keyframes: t.keyframes.iter().map(|k| KeyframeDto { time: k.time, value: KeyframeValueDto::Rotation(k.value), easing: k.easing.into() }).collect(),
            guide: None,
        },
        PropertyTrack::Scale(t) => PropertyTrackSummary {
            property: "Scale",
            keyframes: t.keyframes.iter().map(|k| KeyframeDto { time: k.time, value: KeyframeValueDto::Scale(k.value.into()), easing: k.easing.into() }).collect(),
            guide: None,
        },
        PropertyTrack::FillColor(t) => PropertyTrackSummary {
            property: "FillColor",
            keyframes: t.keyframes.iter().map(|k| KeyframeDto { time: k.time, value: KeyframeValueDto::FillColor(k.value), easing: k.easing.into() }).collect(),
            guide: None,
        },
        PropertyTrack::Path(t) => PropertyTrackSummary {
            property: "Path",
            keyframes: t.keyframes.iter().map(|k| KeyframeDto { time: k.time, value: KeyframeValueDto::Path((&k.value).into()), easing: k.easing.into() }).collect(),
            guide: None,
        },
        PropertyTrack::MotionPath { guide, track } => PropertyTrackSummary {
            property: "MotionPath",
            keyframes: track.keyframes.iter().map(|k| KeyframeDto { time: k.time, value: KeyframeValueDto::MotionPath(k.value), easing: k.easing.into() }).collect(),
            guide: Some(*guide),
        },
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeSummary {
    pub id: NodeId,
    pub name: String,
    /// Mirrors `an1mbyt3_core::Node::id` — the author-facing "tag" a
    /// Code-mode script's `getCompByID` looks a node up by. Named
    /// `element_id`, not `id`, in this DTO specifically to avoid colliding
    /// with the field right above it (this struct's own opaque `NodeId`
    /// token) — the two are entirely different concepts that happen to
    /// share a name in the Rust model (`Node::id`) but can't both be
    /// called `id` here without one shadowing the other.
    pub element_id: String,
    /// Mirrors `an1mbyt3_core::Node::classes` — `getCompByClass`'s
    /// selector tags.
    pub classes: Vec<String>,
    pub kind: &'static str,
    pub transform: Transform2DDto,
    /// `None` for a root node; `Some(parent)` for anything nested — the
    /// Bones/Layers panel's tree indentation and the Bones panel's
    /// parent-bone picker both read this. Mirrors `an1mbyt3_core::Node::parent`.
    pub parent: Option<NodeId>,
    /// `Some(length)` only for `NodeKind::Bone` nodes (the Bones panel's
    /// length field); `None` for every other kind.
    pub bone_length: Option<f32>,
    /// `Group` nodes only — which child (if any) clips the rest of this
    /// group's contents (`GroupNode::mask`). The Layers panel's mask
    /// indicator badge and "use as mask" drag action both read/write this
    /// via `WasmEditor::set_group_mask`.
    pub group_mask: Option<NodeId>,
    /// `Some(enabled)` only for `NodeKind::Group` nodes — `GroupNode::y_sort_enabled`,
    /// the Inspector's "Y Sort" checkbox; `None` for every other kind.
    pub group_y_sort: Option<bool>,
    /// Mirrors `an1mbyt3_core::Node::clip_children` — see that field's own
    /// doc comment. Any node kind, not just `Group`; read/written via
    /// `WasmEditor::set_node_clip_children`.
    pub clip_children: bool,
    /// Mirrors `an1mbyt3_core::Node::visible` — a Code-mode script's own
    /// read side of `WasmEditor::set_node_visible`/`ScriptContext.setVisible`.
    pub visible: bool,
    /// Mirrors `an1mbyt3_core::Node::z_index` — the Inspector's "Z Index"
    /// field, any node kind. See that field's own doc comment for how this
    /// interacts with a y-sort-enabled parent `Group`.
    pub z_index: f32,
    /// `Some(mode)` only for `NodeKind::Shape` nodes (the Inspector's
    /// blend-mode dropdown); `None` for every other kind. Reuses
    /// `an1mbyt3_core::BlendMode` directly — a plain fieldless enum, whose
    /// default `Serialize` output (`"Normal"`/`"Multiply"`/etc) is already
    /// the TS string-literal union shape wants, same reasoning `LoopMode`/
    /// `PlaybackDirection` reuse directly elsewhere in this file.
    pub blend_mode: Option<an1mbyt3_core::BlendMode>,
    /// `Some(color)` for `NodeKind::Shape` and `NodeKind::Text` nodes (the
    /// Inspector's fill-color swatch, `WasmEditor::set_fill_color`); `None`
    /// for every other kind. Distinct from `fill_color_override`/
    /// `resolved_fill_color` below, which are `Instance`-only concepts —
    /// a plain `Shape`/`Text` node's color is authored directly, no
    /// override/variant resolution involved.
    pub fill_color: Option<[f32; 4]>,
    /// `Some(gradient)` for `NodeKind::Shape`/`NodeKind::Text` nodes whose
    /// fill is currently a gradient (`WasmEditor::set_gradient`); `None`
    /// for a plain flat-fill node (of any kind, including `Shape`/`Text`
    /// themselves — this mirrors `ShapeNode::gradient`/`TextNode::gradient`
    /// exactly, not just "is this the right kind").
    pub gradient: Option<GradientDefDto>,
    /// `Shape` nodes only — `Some(id)` when this shape is textured with an
    /// already-imported raster asset instead of a flat/gradient fill (see
    /// `an1mbyt3_core::ShapeNode::image`'s own doc comment). The Inspector's
    /// asset-reference swatch, `WasmEditor::set_shape_image`.
    pub shape_image: Option<ImageId>,
    /// `Shape` nodes only — the path-editing overlay's own read of
    /// `ShapeNode::path`, and the source for a fresh `Command::SetShapePath`
    /// commit; `None` for every other kind.
    pub path: Option<PathDto>,
    /// `Shape` nodes only — `Some(stroke)` when this shape has an outline
    /// (`ShapeNode::stroke`), the Inspector's Stroke section
    /// (`WasmEditor::set_shape_stroke`); `None` for every other kind or an
    /// un-stroked shape.
    pub stroke: Option<StrokeDto>,
    /// `Some(content)` only for `NodeKind::Text` nodes (the Inspector's
    /// content field, `WasmEditor::set_text_content`); `None` for every
    /// other kind.
    pub text_content: Option<String>,
    /// `Some(size)` only for `NodeKind::Text` nodes (the Inspector's size
    /// field, `WasmEditor::set_text_size`); `None` for every other kind.
    pub text_size_px: Option<f32>,
    /// `Some(id)` only for `NodeKind::Instance` nodes — which
    /// `ComponentDefinition` this instance is of; `None` for every other kind.
    pub component: Option<ComponentId>,
    /// `Instance` nodes only — the Inspector's per-instance color-override
    /// field. `None` means "no override" (falls through to `variant`/the
    /// definition's default), not "override to transparent."
    pub fill_color_override: Option<[f32; 4]>,
    /// `Instance` nodes only — the Inspector's per-instance variant dropdown.
    pub variant: Option<String>,
    /// `Instance` nodes only — this instance's effective fill color right
    /// now (`ComponentInstance::resolved_fill_color`'s precedence already
    /// applied), so the Inspector/canvas never has to re-derive override ->
    /// variant -> default resolution order itself.
    pub resolved_fill_color: Option<[f32; 4]>,
    /// `Some(id)` only for `NodeKind::Image` nodes — which `ImageAsset` this
    /// node draws; `None` for every other kind.
    pub image: Option<ImageId>,
    /// `Image` nodes only — the node's authored on-canvas display size
    /// (`ImageNode::width`/`height`), independent of the referenced asset's
    /// own pixel dimensions. The Inspector's width/height fields read this.
    pub image_width: Option<f32>,
    /// `ParticleSystem` nodes only — the Inspector's whole Particle System
    /// section, read/written in one piece via `WasmEditor::set_particle_system`;
    /// `None` for every other kind.
    pub particle_system: Option<ParticleSystemDto>,
    pub image_height: Option<f32>,
    /// `Tilemap` nodes only — the Tilemap panel's whole grid (`TilemapDto`'s
    /// own doc comment for why this is read-whole but written one cell at a
    /// time); `None` for every other kind.
    pub tilemap: Option<TilemapDto>,
    /// `Camera` nodes only — the Inspector's Camera section; `None` for
    /// every other kind.
    pub camera: Option<CameraDto>,
    /// `Light2D` nodes only — the Inspector's Light 2D section.
    pub light2d: Option<Light2DDto>,
    /// `Trail` nodes only — the Inspector's Trail Renderer section.
    pub trail: Option<TrailDto>,
    /// `Node3D` nodes only — the Inspector's 3D section (transform + mesh /
    /// camera / light settings), serialized as-is.
    pub node3d: Option<an1mbyt3_core::Node3D>,
    /// `Node::unlit` — skips 2D lighting (Unity's unlit sprite material).
    pub unlit: bool,
    /// `Node::shadow_caster` — the Inspector's Shadow Caster 2D section.
    pub shadow_caster: Option<ShadowCasterDto>,
    /// Set when this node came from a prefab — see `PrefabLinkDto`.
    pub prefab: Option<PrefabLinkDto>,
    /// This node's attached behaviors (Unity-"component"-style, see
    /// `an1mbyt3_core::behavior`'s own doc comment) — the Inspector's
    /// "Behaviors" section, read/written as one whole list via
    /// `WasmEditor::set_node_behaviors` (same "one nested list, not a dozen
    /// flattened sub-commands" convention `particleSystem`/`tilemap` above
    /// already establish).
    pub behaviors: Vec<NodeBehaviorEntryDto>,
    /// This node's tracks *within the currently active animation* — empty if
    /// no animation is active, or the active animation doesn't key this
    /// node at all. Switching the active animation (`set_active_animation`)
    /// changes what shows up here without touching the node itself.
    pub tracks: Vec<PropertyTrackSummary>,
    /// This node's local-space bounding box — see `local_bounds`'s doc
    /// comment. What the on-canvas transform tool's selection outline and
    /// handles are built from.
    pub local_min: Vec2Dto,
    pub local_max: Vec2Dto,
    /// This node's own transform composed through every ancestor's, up to
    /// the artboard root (`Artboard::world_transform`) — what the renderer
    /// actually draws it at. `transform` (above) is local-to-parent only;
    /// for a root node the two are identical, but for anything nested under
    /// a `Group` that itself has a non-identity transform, `transform`
    /// alone doesn't say where the node actually ends up on screen. The
    /// on-canvas transform tool (`TransformHandles.tsx`) uses this (not
    /// `transform`) to position its selection outline/handles — using the
    /// local-only value there was a real, visible bug: the handles drew
    /// offset from the shape's actual rendered position for any selected
    /// node nested under a moved/rotated/scaled group.
    pub world_transform: Transform2DDto,
    /// This node's immediate parent's own world transform, or `None` for a
    /// root node — what `TransformHandles.tsx` needs to convert an on-canvas
    /// drag's *world*-space result (rotation/scale/translation, computed
    /// against `world_transform` above) back into the *local* value
    /// `Command::SetRotation`/`SetScale`/`SetTranslation` actually store,
    /// via the same `Transform2D::local_for` composition every reparent
    /// already goes through. Without this, committing a rotate/scale drag
    /// on a nested node would write the *world* value as if it were local —
    /// silently double-counting the parent's own contribution (a rotate) or
    /// teleporting the node by the parent's offset (a translate/scale).
    pub parent_world_transform: Option<Transform2DDto>,
}

#[derive(Serialize)]
pub struct AnimationSummary {
    pub id: AnimationId,
    pub name: String,
    /// The animation's fixed, authored time range (seconds) — see
    /// `an1mbyt3_core::Animation::duration`'s doc comment for why this is
    /// authored, not derived from keyframes.
    pub duration: f32,
}

/// A state machine transition's guard condition — mirrors
/// `an1mbyt3_core::ConditionDef` across the wasm-bindgen boundary, using
/// the same internally-tagged discriminated-union shape `KeyframeValueDto`
/// already established (nicer for a TS consumer than `ConditionDef`'s own
/// default externally-tagged `{"BoolIs": {...}}` serde representation,
/// which has no `#[serde(tag = ...)]` since `an1mbyt3-core` has no reason
/// to care about JS-ergonomic JSON shapes).
/// `Deserialize` too (not just `Serialize`) — the State Machine panel sends
/// a condition back in exactly this same shape when authoring a new
/// transition (`WasmEditor::add_transition`), so this type round-trips in
/// both directions rather than needing a second, input-only mirror of itself.
/// No `rename_all` here (unlike `TransitionDefDto`/`StateDefDto` below) —
/// `rename_all` on a tagged enum renames the tag's own variant *values* too,
/// not just field names, which would emit `"trigger"`/`"boolIs"`/etc instead
/// of the `"Trigger"`/`"BoolIs"` this type's own variant names already are.
/// The TS-side discriminated union (`ipc/document.ts`'s `ConditionDef`) was
/// written against the latter — this bit the State Machine panel's very
/// first live test (`add_transition`/`set_transition` both round-trip a
/// condition through this type), surfaced as "invalid condition: unknown
/// variant `Trigger`, expected one of `boolIs`, ...".
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ConditionDefDto {
    BoolIs { input: String, value: bool },
    NumberGreaterThan { input: String, threshold: f32 },
    NumberLessThan { input: String, threshold: f32 },
    EnumIs { input: String, value: String },
    Trigger { input: String },
}

impl From<&ConditionDef> for ConditionDefDto {
    fn from(condition: &ConditionDef) -> Self {
        match condition.clone() {
            ConditionDef::BoolIs { input, value } => ConditionDefDto::BoolIs { input, value },
            ConditionDef::NumberGreaterThan { input, threshold } => ConditionDefDto::NumberGreaterThan { input, threshold },
            ConditionDef::NumberLessThan { input, threshold } => ConditionDefDto::NumberLessThan { input, threshold },
            ConditionDef::EnumIs { input, value } => ConditionDefDto::EnumIs { input, value },
            ConditionDef::Trigger { input } => ConditionDefDto::Trigger { input },
        }
    }
}

impl From<ConditionDefDto> for ConditionDef {
    fn from(dto: ConditionDefDto) -> Self {
        match dto {
            ConditionDefDto::BoolIs { input, value } => ConditionDef::BoolIs { input, value },
            ConditionDefDto::NumberGreaterThan { input, threshold } => ConditionDef::NumberGreaterThan { input, threshold },
            ConditionDefDto::NumberLessThan { input, threshold } => ConditionDef::NumberLessThan { input, threshold },
            ConditionDefDto::EnumIs { input, value } => ConditionDef::EnumIs { input, value },
            ConditionDefDto::Trigger { input } => ConditionDef::Trigger { input },
        }
    }
}

/// Mirrors `an1mbyt3_core::TransitionDef` — `direction`/the `StateDefDto`
/// below's `loop_mode` reuse `an1mbyt3_core::PlaybackDirection`/`LoopMode`
/// directly rather than a wrapper DTO: both are plain fieldless enums, whose
/// default `Serialize` output (the bare variant name as a JSON string,
/// e.g. `"Forward"`) is already exactly the shape a TS string-literal union
/// wants, with no snake_case/case-conversion concern a wrapper would need
/// to fix (unlike `ConditionDef`, which carries per-variant data).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionDefDto {
    pub to: String,
    pub condition: ConditionDefDto,
    pub direction: PlaybackDirection,
    /// Mirrors `an1mbyt3_core::TransitionDef::duration_seconds` — see that
    /// field's own doc comment for the crossfade this drives.
    pub duration_seconds: f32,
    /// Mirrors `an1mbyt3_core::TransitionDef::interruptible` — Rive's
    /// "Allow Exit During Transition" toggle.
    pub interruptible: bool,
}

/// Mirrors `an1mbyt3_core::Blend1DDef` — see its own doc comment
/// (`an1mbyt3-core`) for the "dynamic expression" blend-tree concept this
/// authors.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Blend1DDefDto {
    pub input: String,
    pub low: AnimationId,
    pub high: AnimationId,
    pub low_value: f32,
    pub high_value: f32,
}

/// Mirrors `an1mbyt3_core::SocketOverride`.
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct SocketOverrideDto {
    pub socket_index: u32,
    pub value: f32,
    pub use_override: bool,
}

impl From<SocketOverride> for SocketOverrideDto {
    fn from(value: SocketOverride) -> Self {
        SocketOverrideDto { socket_index: value.socket_index, value: value.value, use_override: value.use_override }
    }
}

impl From<SocketOverrideDto> for SocketOverride {
    fn from(dto: SocketOverrideDto) -> Self {
        SocketOverride { socket_index: dto.socket_index, value: dto.value, use_override: dto.use_override }
    }
}

/// Mirrors `an1mbyt3_core::GraphEdgeDef`.
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdgeDefDto {
    pub from_node: u32,
    pub from_socket: u32,
    pub to_node: u32,
    pub to_socket: u32,
}

impl From<GraphEdgeDef> for GraphEdgeDefDto {
    fn from(edge: GraphEdgeDef) -> Self {
        GraphEdgeDefDto { from_node: edge.from_node, from_socket: edge.from_socket, to_node: edge.to_node, to_socket: edge.to_socket }
    }
}

impl From<GraphEdgeDefDto> for GraphEdgeDef {
    fn from(dto: GraphEdgeDefDto) -> Self {
        GraphEdgeDef { from_node: dto.from_node, from_socket: dto.from_socket, to_node: dto.to_node, to_socket: dto.to_socket }
    }
}

/// Mirrors `an1mbyt3_core::GraphNodeKind` — adjacently tagged (`kind`/`value`),
/// not internally tagged like `ConditionDefDto` — internally-tagged enums
/// can't hold a bare-scalar newtype variant (`Constant(f32)`), which
/// `serde` rejects outright; adjacent tagging wraps whatever the variant
/// carries (scalar, struct, or nothing) under one `value` key uniformly.
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind", content = "value")]
pub enum GraphNodeKindDto {
    Constant(f32),
    Time,
    MousePosition,
    Random { seed: u32 },
    Add,
    Subtract,
    Multiply,
    Divide,
    Sin,
    Cos,
    Lerp,
    Clamp,
    GetProperty { node: NodeId, property: GraphProperty },
    SetProperty { node: NodeId, property: GraphProperty, #[serde(default)] global: bool },
    Min,
    Max,
    GreaterThan,
    LessThan,
    Equals,
    And,
    Or,
    Not,
    Select,
    GetNumberInput { name: String },
    GetBoolInput { name: String },
}

impl From<&GraphNodeKind> for GraphNodeKindDto {
    fn from(kind: &GraphNodeKind) -> Self {
        match kind {
            &GraphNodeKind::Constant(v) => GraphNodeKindDto::Constant(v),
            GraphNodeKind::Time => GraphNodeKindDto::Time,
            GraphNodeKind::MousePosition => GraphNodeKindDto::MousePosition,
            &GraphNodeKind::Random { seed } => GraphNodeKindDto::Random { seed },
            GraphNodeKind::Add => GraphNodeKindDto::Add,
            GraphNodeKind::Subtract => GraphNodeKindDto::Subtract,
            GraphNodeKind::Multiply => GraphNodeKindDto::Multiply,
            GraphNodeKind::Divide => GraphNodeKindDto::Divide,
            GraphNodeKind::Sin => GraphNodeKindDto::Sin,
            GraphNodeKind::Cos => GraphNodeKindDto::Cos,
            GraphNodeKind::Lerp => GraphNodeKindDto::Lerp,
            GraphNodeKind::Clamp => GraphNodeKindDto::Clamp,
            &GraphNodeKind::GetProperty { node, property } => GraphNodeKindDto::GetProperty { node, property },
            &GraphNodeKind::SetProperty { node, property, global } => GraphNodeKindDto::SetProperty { node, property, global },
            GraphNodeKind::Min => GraphNodeKindDto::Min,
            GraphNodeKind::Max => GraphNodeKindDto::Max,
            GraphNodeKind::GreaterThan => GraphNodeKindDto::GreaterThan,
            GraphNodeKind::LessThan => GraphNodeKindDto::LessThan,
            GraphNodeKind::Equals => GraphNodeKindDto::Equals,
            GraphNodeKind::And => GraphNodeKindDto::And,
            GraphNodeKind::Or => GraphNodeKindDto::Or,
            GraphNodeKind::Not => GraphNodeKindDto::Not,
            GraphNodeKind::Select => GraphNodeKindDto::Select,
            GraphNodeKind::GetNumberInput { name } => GraphNodeKindDto::GetNumberInput { name: name.clone() },
            GraphNodeKind::GetBoolInput { name } => GraphNodeKindDto::GetBoolInput { name: name.clone() },
        }
    }
}

impl From<GraphNodeKindDto> for GraphNodeKind {
    fn from(dto: GraphNodeKindDto) -> Self {
        match dto {
            GraphNodeKindDto::Constant(v) => GraphNodeKind::Constant(v),
            GraphNodeKindDto::Time => GraphNodeKind::Time,
            GraphNodeKindDto::MousePosition => GraphNodeKind::MousePosition,
            GraphNodeKindDto::Random { seed } => GraphNodeKind::Random { seed },
            GraphNodeKindDto::Add => GraphNodeKind::Add,
            GraphNodeKindDto::Subtract => GraphNodeKind::Subtract,
            GraphNodeKindDto::Multiply => GraphNodeKind::Multiply,
            GraphNodeKindDto::Divide => GraphNodeKind::Divide,
            GraphNodeKindDto::Sin => GraphNodeKind::Sin,
            GraphNodeKindDto::Cos => GraphNodeKind::Cos,
            GraphNodeKindDto::Lerp => GraphNodeKind::Lerp,
            GraphNodeKindDto::Clamp => GraphNodeKind::Clamp,
            GraphNodeKindDto::GetProperty { node, property } => GraphNodeKind::GetProperty { node, property },
            GraphNodeKindDto::SetProperty { node, property, global } => GraphNodeKind::SetProperty { node, property, global },
            GraphNodeKindDto::Min => GraphNodeKind::Min,
            GraphNodeKindDto::Max => GraphNodeKind::Max,
            GraphNodeKindDto::GreaterThan => GraphNodeKind::GreaterThan,
            GraphNodeKindDto::LessThan => GraphNodeKind::LessThan,
            GraphNodeKindDto::Equals => GraphNodeKind::Equals,
            GraphNodeKindDto::And => GraphNodeKind::And,
            GraphNodeKindDto::Or => GraphNodeKind::Or,
            GraphNodeKindDto::Not => GraphNodeKind::Not,
            GraphNodeKindDto::Select => GraphNodeKind::Select,
            GraphNodeKindDto::GetNumberInput { name } => GraphNodeKind::GetNumberInput { name },
            GraphNodeKindDto::GetBoolInput { name } => GraphNodeKind::GetBoolInput { name },
        }
    }
}

/// Mirrors `an1mbyt3_core::GraphNodeDef`.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GraphNodeDefDto {
    pub id: u32,
    pub kind: GraphNodeKindDto,
    pub position: Vec2Dto,
    pub overrides: Vec<SocketOverrideDto>,
}

impl From<&GraphNodeDef> for GraphNodeDefDto {
    fn from(node: &GraphNodeDef) -> Self {
        GraphNodeDefDto {
            id: node.id,
            kind: (&node.kind).into(),
            position: node.position.into(),
            overrides: node.overrides.iter().map(|o| (*o).into()).collect(),
        }
    }
}

impl From<GraphNodeDefDto> for GraphNodeDef {
    fn from(dto: GraphNodeDefDto) -> Self {
        GraphNodeDef {
            id: dto.id,
            kind: dto.kind.into(),
            position: dto.position.into(),
            overrides: dto.overrides.into_iter().map(Into::into).collect(),
        }
    }
}

/// Mirrors `an1mbyt3_core::NodeGraphDef` — the whole authored dataflow graph
/// for a Programmable State, round-tripped whole through `set_state_program`
/// (the node graph editor computes the entire new value after every edit
/// and calls that once — see `an1mbyt3_core::command::Command::SetStateProgram`'s
/// doc comment for why).
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NodeGraphDefDto {
    pub nodes: Vec<GraphNodeDefDto>,
    pub edges: Vec<GraphEdgeDefDto>,
}

impl From<&NodeGraphDef> for NodeGraphDefDto {
    fn from(graph: &NodeGraphDef) -> Self {
        NodeGraphDefDto { nodes: graph.nodes.iter().map(Into::into).collect(), edges: graph.edges.iter().map(|e| (*e).into()).collect() }
    }
}

impl From<NodeGraphDefDto> for NodeGraphDef {
    fn from(dto: NodeGraphDefDto) -> Self {
        NodeGraphDef { nodes: dto.nodes.into_iter().map(Into::into).collect(), edges: dto.edges.into_iter().map(Into::into).collect() }
    }
}

/// Mirrors `an1mbyt3_core::ListExpr`.
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind", content = "value")]
pub enum ListExprDto {
    NodesWithClass { class: String },
}

impl From<&ListExpr> for ListExprDto {
    fn from(list: &ListExpr) -> Self {
        match list {
            ListExpr::NodesWithClass { class } => ListExprDto::NodesWithClass { class: class.clone() },
        }
    }
}

impl From<ListExprDto> for ListExpr {
    fn from(dto: ListExprDto) -> Self {
        match dto {
            ListExprDto::NodesWithClass { class } => ListExpr::NodesWithClass { class },
        }
    }
}

/// Mirrors `an1mbyt3_core::NodeRef`.
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind", content = "value")]
pub enum NodeRefDto {
    Literal(NodeId),
    CurrentItem,
    RandomFromList(ListExprDto),
    Myself,
}

impl From<&NodeRef> for NodeRefDto {
    fn from(node_ref: &NodeRef) -> Self {
        match node_ref {
            &NodeRef::Literal(id) => NodeRefDto::Literal(id),
            NodeRef::CurrentItem => NodeRefDto::CurrentItem,
            NodeRef::RandomFromList(list) => NodeRefDto::RandomFromList(list.into()),
            NodeRef::Myself => NodeRefDto::Myself,
        }
    }
}

impl From<NodeRefDto> for NodeRef {
    fn from(dto: NodeRefDto) -> Self {
        match dto {
            NodeRefDto::Literal(id) => NodeRef::Literal(id),
            NodeRefDto::CurrentItem => NodeRef::CurrentItem,
            NodeRefDto::RandomFromList(list) => NodeRef::RandomFromList(list.into()),
            NodeRefDto::Myself => NodeRef::Myself,
        }
    }
}

/// Mirrors `an1mbyt3_core::MathFn`.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum MathFnDto {
    Abs,
    Floor,
    Ceiling,
    Round,
    Sqrt,
    Sin,
    Cos,
    Tan,
    Ln,
    Log10,
}

impl From<MathFn> for MathFnDto {
    fn from(op: MathFn) -> Self {
        match op {
            MathFn::Abs => MathFnDto::Abs,
            MathFn::Floor => MathFnDto::Floor,
            MathFn::Ceiling => MathFnDto::Ceiling,
            MathFn::Round => MathFnDto::Round,
            MathFn::Sqrt => MathFnDto::Sqrt,
            MathFn::Sin => MathFnDto::Sin,
            MathFn::Cos => MathFnDto::Cos,
            MathFn::Tan => MathFnDto::Tan,
            MathFn::Ln => MathFnDto::Ln,
            MathFn::Log10 => MathFnDto::Log10,
        }
    }
}

impl From<MathFnDto> for MathFn {
    fn from(dto: MathFnDto) -> Self {
        match dto {
            MathFnDto::Abs => MathFn::Abs,
            MathFnDto::Floor => MathFn::Floor,
            MathFnDto::Ceiling => MathFn::Ceiling,
            MathFnDto::Round => MathFn::Round,
            MathFnDto::Sqrt => MathFn::Sqrt,
            MathFnDto::Sin => MathFn::Sin,
            MathFnDto::Cos => MathFn::Cos,
            MathFnDto::Tan => MathFn::Tan,
            MathFnDto::Ln => MathFn::Ln,
            MathFnDto::Log10 => MathFn::Log10,
        }
    }
}

/// Mirrors `an1mbyt3_core::Expr` — adjacently tagged, same reasoning as
/// `GraphNodeKindDto` (`Number(f32)` is a bare-scalar newtype variant, and
/// the binary math/logic variants are bare tuple variants, neither of which
/// an internally-tagged enum can carry). Recursive (`Box<ExprDto>`) exactly
/// like the core type — serde boxes transparently, no special handling
/// needed.
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind", content = "value")]
pub enum ExprDto {
    Number(f32),
    Pi,
    GetProperty { node: NodeRefDto, property: GraphProperty },
    MouseX,
    MouseY,
    KeyPressed { key: String },
    Touching { a: NodeRefDto, b: NodeRefDto },
    TouchingClass { node: NodeRefDto, class: String },
    Add(Box<ExprDto>, Box<ExprDto>),
    Subtract(Box<ExprDto>, Box<ExprDto>),
    Multiply(Box<ExprDto>, Box<ExprDto>),
    Divide(Box<ExprDto>, Box<ExprDto>),
    Mod(Box<ExprDto>, Box<ExprDto>),
    GreaterThan(Box<ExprDto>, Box<ExprDto>),
    LessThan(Box<ExprDto>, Box<ExprDto>),
    Equals(Box<ExprDto>, Box<ExprDto>),
    And(Box<ExprDto>, Box<ExprDto>),
    Or(Box<ExprDto>, Box<ExprDto>),
    Not(Box<ExprDto>),
    MathFn { op: MathFnDto, value: Box<ExprDto> },
    RandomInt { min: Box<ExprDto>, max: Box<ExprDto> },
    RandomFloat { min: Box<ExprDto>, max: Box<ExprDto> },
    GetNumberInput { name: String },
    GetBoolInput { name: String },
}

impl From<&Expr> for ExprDto {
    fn from(expr: &Expr) -> Self {
        match expr {
            &Expr::Number(v) => ExprDto::Number(v),
            Expr::Pi => ExprDto::Pi,
            Expr::GetProperty { node, property } => ExprDto::GetProperty { node: node.into(), property: *property },
            Expr::MouseX => ExprDto::MouseX,
            Expr::MouseY => ExprDto::MouseY,
            Expr::KeyPressed { key } => ExprDto::KeyPressed { key: key.clone() },
            Expr::Touching { a, b } => ExprDto::Touching { a: a.into(), b: b.into() },
            Expr::TouchingClass { node, class } => ExprDto::TouchingClass { node: node.into(), class: class.clone() },
            Expr::Add(a, b) => ExprDto::Add(Box::new(a.as_ref().into()), Box::new(b.as_ref().into())),
            Expr::Subtract(a, b) => ExprDto::Subtract(Box::new(a.as_ref().into()), Box::new(b.as_ref().into())),
            Expr::Multiply(a, b) => ExprDto::Multiply(Box::new(a.as_ref().into()), Box::new(b.as_ref().into())),
            Expr::Divide(a, b) => ExprDto::Divide(Box::new(a.as_ref().into()), Box::new(b.as_ref().into())),
            Expr::Mod(a, b) => ExprDto::Mod(Box::new(a.as_ref().into()), Box::new(b.as_ref().into())),
            Expr::GreaterThan(a, b) => ExprDto::GreaterThan(Box::new(a.as_ref().into()), Box::new(b.as_ref().into())),
            Expr::LessThan(a, b) => ExprDto::LessThan(Box::new(a.as_ref().into()), Box::new(b.as_ref().into())),
            Expr::Equals(a, b) => ExprDto::Equals(Box::new(a.as_ref().into()), Box::new(b.as_ref().into())),
            Expr::And(a, b) => ExprDto::And(Box::new(a.as_ref().into()), Box::new(b.as_ref().into())),
            Expr::Or(a, b) => ExprDto::Or(Box::new(a.as_ref().into()), Box::new(b.as_ref().into())),
            Expr::Not(a) => ExprDto::Not(Box::new(a.as_ref().into())),
            &Expr::MathFn { op, ref value } => ExprDto::MathFn { op: op.into(), value: Box::new(value.as_ref().into()) },
            Expr::RandomInt { min, max } => ExprDto::RandomInt { min: Box::new(min.as_ref().into()), max: Box::new(max.as_ref().into()) },
            Expr::RandomFloat { min, max } => ExprDto::RandomFloat { min: Box::new(min.as_ref().into()), max: Box::new(max.as_ref().into()) },
            Expr::GetNumberInput { name } => ExprDto::GetNumberInput { name: name.clone() },
            Expr::GetBoolInput { name } => ExprDto::GetBoolInput { name: name.clone() },
        }
    }
}

impl From<ExprDto> for Expr {
    fn from(dto: ExprDto) -> Self {
        match dto {
            ExprDto::Number(v) => Expr::Number(v),
            ExprDto::Pi => Expr::Pi,
            ExprDto::GetProperty { node, property } => Expr::GetProperty { node: node.into(), property },
            ExprDto::MouseX => Expr::MouseX,
            ExprDto::MouseY => Expr::MouseY,
            ExprDto::KeyPressed { key } => Expr::KeyPressed { key },
            ExprDto::Touching { a, b } => Expr::Touching { a: a.into(), b: b.into() },
            ExprDto::TouchingClass { node, class } => Expr::TouchingClass { node: node.into(), class },
            ExprDto::Add(a, b) => Expr::Add(Box::new((*a).into()), Box::new((*b).into())),
            ExprDto::Subtract(a, b) => Expr::Subtract(Box::new((*a).into()), Box::new((*b).into())),
            ExprDto::Multiply(a, b) => Expr::Multiply(Box::new((*a).into()), Box::new((*b).into())),
            ExprDto::Divide(a, b) => Expr::Divide(Box::new((*a).into()), Box::new((*b).into())),
            ExprDto::Mod(a, b) => Expr::Mod(Box::new((*a).into()), Box::new((*b).into())),
            ExprDto::GreaterThan(a, b) => Expr::GreaterThan(Box::new((*a).into()), Box::new((*b).into())),
            ExprDto::LessThan(a, b) => Expr::LessThan(Box::new((*a).into()), Box::new((*b).into())),
            ExprDto::Equals(a, b) => Expr::Equals(Box::new((*a).into()), Box::new((*b).into())),
            ExprDto::And(a, b) => Expr::And(Box::new((*a).into()), Box::new((*b).into())),
            ExprDto::Or(a, b) => Expr::Or(Box::new((*a).into()), Box::new((*b).into())),
            ExprDto::Not(a) => Expr::Not(Box::new((*a).into())),
            ExprDto::MathFn { op, value } => Expr::MathFn { op: op.into(), value: Box::new((*value).into()) },
            ExprDto::RandomInt { min, max } => Expr::RandomInt { min: Box::new((*min).into()), max: Box::new((*max).into()) },
            ExprDto::RandomFloat { min, max } => Expr::RandomFloat { min: Box::new((*min).into()), max: Box::new((*max).into()) },
            ExprDto::GetNumberInput { name } => Expr::GetNumberInput { name },
            ExprDto::GetBoolInput { name } => Expr::GetBoolInput { name },
        }
    }
}

/// Mirrors `an1mbyt3_core::Statement`.
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind", content = "value")]
pub enum StatementDto {
    GoToXY { node: NodeRefDto, x: ExprDto, y: ExprDto },
    SetProperty { node: NodeRefDto, property: GraphProperty, value: ExprDto },
    SetTextNumber { node: NodeRefDto, value: ExprDto, decimals: u32 },
    ChangeProperty { node: NodeRefDto, property: GraphProperty, delta: ExprDto },
    Wait { seconds: ExprDto },
    // This enum has no `rename_all` (every other field here is naturally
    // case-agnostic), so `then_branch`/`else_branch` are the one spot that
    // actually needs a field-level rename: `gameBlocks.ts`'s codegen (and
    // `document.ts`'s own `Statement` IR type) emit camelCase
    // `thenBranch`/`elseBranch` for every `If` — without this, every `if`
    // block fails deserialization with "missing field `then_branch`" the
    // instant its script runs, not a corrupted-save-file issue.
    If {
        condition: ExprDto,
        #[serde(rename = "thenBranch")]
        then_branch: Vec<StatementDto>,
        #[serde(rename = "elseBranch")]
        else_branch: Vec<StatementDto>,
    },
    Repeat { count: ExprDto, body: Vec<StatementDto> },
    Forever { body: Vec<StatementDto> },
    ForEachNode { list: ListExprDto, body: Vec<StatementDto> },
    CreateCloneOf { node: NodeRefDto },
    DeleteThisClone,
}

impl From<&Statement> for StatementDto {
    fn from(statement: &Statement) -> Self {
        match statement {
            Statement::GoToXY { node, x, y } => StatementDto::GoToXY { node: node.into(), x: x.into(), y: y.into() },
            &Statement::SetProperty { ref node, property, ref value } => StatementDto::SetProperty { node: node.into(), property, value: value.into() },
            Statement::SetTextNumber { node, value, decimals } => StatementDto::SetTextNumber { node: node.into(), value: value.into(), decimals: *decimals },
            &Statement::ChangeProperty { ref node, property, ref delta } => StatementDto::ChangeProperty { node: node.into(), property, delta: delta.into() },
            Statement::Wait { seconds } => StatementDto::Wait { seconds: seconds.into() },
            Statement::If { condition, then_branch, else_branch } => StatementDto::If {
                condition: condition.into(),
                then_branch: then_branch.iter().map(Into::into).collect(),
                else_branch: else_branch.iter().map(Into::into).collect(),
            },
            Statement::Repeat { count, body } => StatementDto::Repeat { count: count.into(), body: body.iter().map(Into::into).collect() },
            Statement::Forever { body } => StatementDto::Forever { body: body.iter().map(Into::into).collect() },
            Statement::ForEachNode { list, body } => StatementDto::ForEachNode { list: list.into(), body: body.iter().map(Into::into).collect() },
            Statement::CreateCloneOf { node } => StatementDto::CreateCloneOf { node: node.into() },
            Statement::DeleteThisClone => StatementDto::DeleteThisClone,
        }
    }
}

impl From<StatementDto> for Statement {
    fn from(dto: StatementDto) -> Self {
        match dto {
            StatementDto::GoToXY { node, x, y } => Statement::GoToXY { node: node.into(), x: x.into(), y: y.into() },
            StatementDto::SetProperty { node, property, value } => Statement::SetProperty { node: node.into(), property, value: value.into() },
            StatementDto::SetTextNumber { node, value, decimals } => Statement::SetTextNumber { node: node.into(), value: value.into(), decimals },
            StatementDto::ChangeProperty { node, property, delta } => Statement::ChangeProperty { node: node.into(), property, delta: delta.into() },
            StatementDto::Wait { seconds } => Statement::Wait { seconds: seconds.into() },
            StatementDto::If { condition, then_branch, else_branch } => Statement::If {
                condition: condition.into(),
                then_branch: then_branch.into_iter().map(Into::into).collect(),
                else_branch: else_branch.into_iter().map(Into::into).collect(),
            },
            StatementDto::Repeat { count, body } => Statement::Repeat { count: count.into(), body: body.into_iter().map(Into::into).collect() },
            StatementDto::Forever { body } => Statement::Forever { body: body.into_iter().map(Into::into).collect() },
            StatementDto::ForEachNode { list, body } => Statement::ForEachNode { list: list.into(), body: body.into_iter().map(Into::into).collect() },
            StatementDto::CreateCloneOf { node } => Statement::CreateCloneOf { node: node.into() },
            StatementDto::DeleteThisClone => Statement::DeleteThisClone,
        }
    }
}

/// Mirrors `an1mbyt3_core::EventKind`.
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind", content = "value")]
pub enum EventKindDto {
    StateTriggered,
    KeyPressed { key: String, overlapping: bool },
    KeyHeld { key: String },
    CloneStarted,
}

impl From<&EventKind> for EventKindDto {
    fn from(event: &EventKind) -> Self {
        match event {
            EventKind::StateTriggered => EventKindDto::StateTriggered,
            EventKind::KeyPressed { key, overlapping } => EventKindDto::KeyPressed { key: key.clone(), overlapping: *overlapping },
            EventKind::KeyHeld { key } => EventKindDto::KeyHeld { key: key.clone() },
            EventKind::CloneStarted => EventKindDto::CloneStarted,
        }
    }
}

impl From<EventKindDto> for EventKind {
    fn from(dto: EventKindDto) -> Self {
        match dto {
            EventKindDto::StateTriggered => EventKind::StateTriggered,
            EventKindDto::KeyPressed { key, overlapping } => EventKind::KeyPressed { key, overlapping },
            EventKindDto::KeyHeld { key } => EventKind::KeyHeld { key },
            EventKindDto::CloneStarted => EventKind::CloneStarted,
        }
    }
}

/// Mirrors `an1mbyt3_core::BlockScript`.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BlockScriptDto {
    pub event: EventKindDto,
    pub statements: Vec<StatementDto>,
    pub position: Option<Vec2Dto>,
}

impl From<&BlockScript> for BlockScriptDto {
    fn from(script: &BlockScript) -> Self {
        BlockScriptDto { event: (&script.event).into(), statements: script.statements.iter().map(Into::into).collect(), position: script.position.map(Into::into) }
    }
}

impl From<BlockScriptDto> for BlockScript {
    fn from(dto: BlockScriptDto) -> Self {
        BlockScript { event: dto.event.into(), statements: dto.statements.into_iter().map(Into::into).collect(), position: dto.position.map(Into::into) }
    }
}

/// Mirrors `an1mbyt3_core::BlockProgramDef` — the whole authored Scratch-
/// style block program for a Programmable State, round-tripped whole
/// through `set_state_program` exactly like `NodeGraphDefDto`.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct BlockProgramDefDto {
    pub scripts: Vec<BlockScriptDto>,
    pub ticks_per_second: Option<f32>,
}

impl From<&BlockProgramDef> for BlockProgramDefDto {
    fn from(program: &BlockProgramDef) -> Self {
        BlockProgramDefDto { scripts: program.scripts.iter().map(Into::into).collect(), ticks_per_second: program.ticks_per_second }
    }
}

impl From<BlockProgramDefDto> for BlockProgramDef {
    fn from(dto: BlockProgramDefDto) -> Self {
        BlockProgramDef { scripts: dto.scripts.into_iter().map(Into::into).collect(), ticks_per_second: dto.ticks_per_second }
    }
}

/// Mirrors `an1mbyt3_core::ProgramDef` — adjacently tagged, same reasoning
/// as `GraphNodeKindDto` (`Code(String)` is a bare-scalar newtype variant,
/// which an internally-tagged enum can't carry).
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind", content = "value")]
pub enum ProgramDefDto {
    Code(String),
    Graph(NodeGraphDefDto),
    Blocks(BlockProgramDefDto),
}

impl From<&ProgramDef> for ProgramDefDto {
    fn from(program: &ProgramDef) -> Self {
        match program {
            ProgramDef::Code(source) => ProgramDefDto::Code(source.clone()),
            ProgramDef::Graph(graph) => ProgramDefDto::Graph(graph.into()),
            ProgramDef::Blocks(blocks) => ProgramDefDto::Blocks(blocks.into()),
        }
    }
}

impl From<ProgramDefDto> for ProgramDef {
    fn from(dto: ProgramDefDto) -> Self {
        match dto {
            ProgramDefDto::Code(source) => ProgramDef::Code(source),
            ProgramDefDto::Graph(graph) => ProgramDef::Graph(graph.into()),
            ProgramDefDto::Blocks(blocks) => ProgramDef::Blocks(blocks.into()),
        }
    }
}

/// Mirrors `an1mbyt3_core::PostEffect` — a discriminated union tagged on
/// `kind`, same "internally-tagged, no `rename_all`" reasoning
/// `ConditionDefDto`'s own doc comment gives (a `rename_all` on a tagged
/// enum would rename the tag's own variant *values*, not just field names —
/// exactly the bug that one hit first).
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum PostEffectDto {
    None,
    Blur { radius: f32 },
    Glow { radius: f32, threshold: f32, intensity: f32 },
    // `rename` (not a blanket `rename_all` on the whole enum) on just this
    // one field — `rename_all` on a `tag = "kind"` enum renames the tag's
    // own variant values too (`ConditionDefDto`'s own doc comment above),
    // which would turn `"Pixelate"` into `"pixelate"` for no reason; this
    // is the one field in this whole enum whose Rust name isn't already a
    // single camelCase-safe word.
    Pixelate {
        #[serde(rename = "blockSize")]
        block_size: f32,
    },
    ColorGrading { brightness: f32, contrast: f32, saturation: f32 },
    Posterize { levels: f32 },
    GradientMap { shadow: [f32; 4], highlight: [f32; 4] },
    Vignette { color: [f32; 4], intensity: f32, radius: f32, blur: f32 },
}

impl From<&PostEffect> for PostEffectDto {
    fn from(effect: &PostEffect) -> Self {
        match *effect {
            PostEffect::None => PostEffectDto::None,
            PostEffect::Blur { radius } => PostEffectDto::Blur { radius },
            PostEffect::Glow { radius, threshold, intensity } => PostEffectDto::Glow { radius, threshold, intensity },
            PostEffect::Pixelate { block_size } => PostEffectDto::Pixelate { block_size },
            PostEffect::ColorGrading { brightness, contrast, saturation } => PostEffectDto::ColorGrading { brightness, contrast, saturation },
            PostEffect::Posterize { levels } => PostEffectDto::Posterize { levels },
            PostEffect::GradientMap { shadow, highlight } => PostEffectDto::GradientMap { shadow, highlight },
            PostEffect::Vignette { color, intensity, radius, blur } => PostEffectDto::Vignette { color, intensity, radius, blur },
        }
    }
}

impl From<PostEffectDto> for PostEffect {
    fn from(dto: PostEffectDto) -> Self {
        match dto {
            PostEffectDto::None => PostEffect::None,
            PostEffectDto::Blur { radius } => PostEffect::Blur { radius },
            PostEffectDto::Glow { radius, threshold, intensity } => PostEffect::Glow { radius, threshold, intensity },
            PostEffectDto::Pixelate { block_size } => PostEffect::Pixelate { block_size },
            PostEffectDto::ColorGrading { brightness, contrast, saturation } => PostEffect::ColorGrading { brightness, contrast, saturation },
            PostEffectDto::Posterize { levels } => PostEffect::Posterize { levels },
            PostEffectDto::GradientMap { shadow, highlight } => PostEffect::GradientMap { shadow, highlight },
            PostEffectDto::Vignette { color, intensity, radius, blur } => PostEffect::Vignette { color, intensity, radius, blur },
        }
    }
}

/// Mirrors `an1mbyt3_core::PostEffectLayer` — one entry in an artboard's
/// post-processing stack (`ArtboardSummary::post_effects`).
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostEffectLayerDto {
    pub effect: PostEffectDto,
    pub enabled: bool,
}

impl From<&PostEffectLayer> for PostEffectLayerDto {
    fn from(layer: &PostEffectLayer) -> Self {
        PostEffectLayerDto { effect: PostEffectDto::from(&layer.effect), enabled: layer.enabled }
    }
}

impl From<PostEffectLayerDto> for PostEffectLayer {
    fn from(dto: PostEffectLayerDto) -> Self {
        PostEffectLayer { effect: dto.effect.into(), enabled: dto.enabled }
    }
}

/// Mirrors `an1mbyt3_core::StateDef`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateDefDto {
    pub name: String,
    pub animation: AnimationId,
    pub loop_mode: LoopMode,
    pub transitions: Vec<TransitionDefDto>,
    /// This state's box position on the State Machine panel's node-graph
    /// canvas — mirrors `an1mbyt3_core::StateDef::position`.
    pub position: Vec2Dto,
    /// Mirrors `an1mbyt3_core::StateDef::blend` — `Some` when this state is
    /// a continuous blend-tree state instead of a plain played-back one.
    pub blend: Option<Blend1DDefDto>,
    /// Mirrors `an1mbyt3_core::StateDef::program` — `Some` when this is a
    /// Programmable State (see `ProgramDefDto`'s own doc comment).
    pub program: Option<ProgramDefDto>,
}

/// Mirrors `an1mbyt3_core::ComponentVariant` — needs a wrapper (unlike
/// `InputDef`) because `fill_color` needs the camelCase conversion this
/// file's `rename_all` convention applies.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentVariantDto {
    pub name: String,
    pub fill_color: [f32; 4],
}

/// Mirrors `an1mbyt3_core::ComponentDefinition` — what the Components
/// panel's library listing and variant editor read/edit, *and* what the
/// Resources panel's own "Drawings" section (vector resources saved from
/// Paint mode, alongside the raster `images`/`spriteGroups` it already
/// lists) renders a thumbnail from — `path_d`/`view_box` exist purely for
/// that preview, not for any editing affordance (which stays in the
/// Components panel, the one place a definition's shape can actually be
/// changed).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentDefinitionSummary {
    pub id: ComponentId,
    pub name: String,
    pub default_fill_color: [f32; 4],
    pub variants: Vec<ComponentVariantDto>,
    /// An SVG `<path d="...">` value tracing `definition.path` — a cheap,
    /// real thumbnail (no rasterization, no wasm round-trip for pixels) for
    /// a plain inline `<svg>` to render directly in the frontend.
    pub path_d: String,
    /// `[min_x, min_y, width, height]` in the same flipped coordinate space
    /// `path_d` is written in — a ready-to-use SVG `viewBox` for that same
    /// `<svg>`, not this document's own `Path::bounds()` convention
    /// (+y-up) — see `path_to_svg_d`'s own doc comment for why the flip.
    pub view_box: [f32; 4],
}

/// Traces `path`'s commands into an SVG `d` attribute value, flipping `y`
/// (this workspace's own `+y`-up convention -> SVG's `+y`-down) so the
/// result renders upright in a plain `<svg>` thumbnail — the exact inverse
/// of `an1mbyt3-svg`'s own importer flip (`recenter_vec2`), applied here for
/// the same reason in reverse: a `ComponentDefinition`'s `path` was very
/// often itself born from that importer (a Paint-mode drawing saved as a
/// component), so round-tripping through the same flip is what makes it
/// look right again on the way back out to a browser-native SVG consumer.
fn path_to_svg_d(path: &an1mbyt3_core::Path) -> String {
    let mut d = String::new();
    for command in &path.commands {
        match *command {
            an1mbyt3_core::PathCommand::MoveTo(p) => d.push_str(&format!("M{} {} ", p.x, -p.y)),
            an1mbyt3_core::PathCommand::LineTo(p) => d.push_str(&format!("L{} {} ", p.x, -p.y)),
            an1mbyt3_core::PathCommand::CubicTo { control1, control2, to } => {
                d.push_str(&format!("C{} {} {} {} {} {} ", control1.x, -control1.y, control2.x, -control2.y, to.x, -to.y));
            }
            an1mbyt3_core::PathCommand::Close => d.push_str("Z "),
        }
    }
    d.trim_end().to_string()
}

/// Mirrors `an1mbyt3_core::ScriptFieldKind` directly (a plain fieldless enum
/// — its default `Serialize` output, the bare variant name, is already the
/// TS string-literal union shape wants), same convention `GradientKind`/
/// `LoopMode` already establish elsewhere in this file.
///
/// Mirrors `an1mbyt3_core::ScriptFieldValue` — tagged so the frontend's
/// field editors can branch on `kind` to pick the right widget (number
/// input/checkbox/color swatch/text box) the same way they already do for
/// `KeyframeValueDto`. `#[serde(tag = "kind")]` (not `rename_all`, same
/// `ConditionDefDto` reasoning this file's own doc comment gives) so the
/// variant name round-trips as `"Float"`/`"Int"`/etc., matching
/// `ScriptFieldKind`'s own bare-variant-name serialization.
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind")]
pub enum ScriptFieldValueDto {
    Float { value: f32 },
    Int { value: i32 },
    Bool { value: bool },
    Color { value: [f32; 4] },
    Vec2 { value: [f32; 2] },
    String { value: String },
}

impl From<&an1mbyt3_core::ScriptFieldValue> for ScriptFieldValueDto {
    fn from(v: &an1mbyt3_core::ScriptFieldValue) -> Self {
        match v {
            an1mbyt3_core::ScriptFieldValue::Float(value) => ScriptFieldValueDto::Float { value: *value },
            an1mbyt3_core::ScriptFieldValue::Int(value) => ScriptFieldValueDto::Int { value: *value },
            an1mbyt3_core::ScriptFieldValue::Bool(value) => ScriptFieldValueDto::Bool { value: *value },
            an1mbyt3_core::ScriptFieldValue::Color(value) => ScriptFieldValueDto::Color { value: *value },
            an1mbyt3_core::ScriptFieldValue::Vec2(value) => ScriptFieldValueDto::Vec2 { value: *value },
            an1mbyt3_core::ScriptFieldValue::String(value) => ScriptFieldValueDto::String { value: value.clone() },
        }
    }
}

impl From<ScriptFieldValueDto> for an1mbyt3_core::ScriptFieldValue {
    fn from(v: ScriptFieldValueDto) -> Self {
        match v {
            ScriptFieldValueDto::Float { value } => an1mbyt3_core::ScriptFieldValue::Float(value),
            ScriptFieldValueDto::Int { value } => an1mbyt3_core::ScriptFieldValue::Int(value),
            ScriptFieldValueDto::Bool { value } => an1mbyt3_core::ScriptFieldValue::Bool(value),
            ScriptFieldValueDto::Color { value } => an1mbyt3_core::ScriptFieldValue::Color(value),
            ScriptFieldValueDto::Vec2 { value } => an1mbyt3_core::ScriptFieldValue::Vec2(value),
            ScriptFieldValueDto::String { value } => an1mbyt3_core::ScriptFieldValue::String(value),
        }
    }
}

/// Mirrors `an1mbyt3_core::ScriptFieldDef`.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScriptFieldDefDto {
    pub name: String,
    pub default: ScriptFieldValueDto,
    pub animatable: bool,
}

impl From<&an1mbyt3_core::ScriptFieldDef> for ScriptFieldDefDto {
    fn from(f: &an1mbyt3_core::ScriptFieldDef) -> Self {
        ScriptFieldDefDto { name: f.name.clone(), default: (&f.default).into(), animatable: f.animatable }
    }
}

impl From<ScriptFieldDefDto> for an1mbyt3_core::ScriptFieldDef {
    fn from(f: ScriptFieldDefDto) -> Self {
        let default: an1mbyt3_core::ScriptFieldValue = f.default.into();
        an1mbyt3_core::ScriptFieldDef { name: f.name, kind: default.kind(), default, animatable: f.animatable }
    }
}

/// Mirrors `an1mbyt3_core::ScriptDefinition` — the Inspector's "Add
/// Behavior" picker (choosing which script type to attach) and the script
/// authoring UI (add/remove/retype a field) both read/write this.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptDefinitionSummary {
    pub id: an1mbyt3_core::ScriptId,
    pub name: String,
    pub fields: Vec<ScriptFieldDefDto>,
}

fn summarize_script(id: an1mbyt3_core::ScriptId, definition: &an1mbyt3_core::ScriptDefinition) -> ScriptDefinitionSummary {
    ScriptDefinitionSummary { id, name: definition.name.clone(), fields: definition.fields.iter().map(Into::into).collect() }
}

/// Mirrors `an1mbyt3_core::NodeBehaviorEntry` — one entry in the Inspector's
/// "Behaviors" section for the selected node. `SpriteRenderer` has no editor
/// UI yet (see `an1mbyt3_core::behavior`'s own doc comment — it predates any
/// frontend wiring at all), so it round-trips as inert data for now; `Script`
/// is the variant the new script-component UI actually authors.
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum NodeBehaviorDto {
    SpriteRenderer { material: Option<an1mbyt3_core::MaterialId>, color: [f32; 4] },
    Script { definition: an1mbyt3_core::ScriptId, values: Vec<(String, ScriptFieldValueDto)> },
    // Unity components (payload fields flattened next to `kind`).
    Camera(CameraDto),
    Light2D(Light2DDto),
    TrailRenderer(TrailDto),
    ParticleSystem(ParticleSystemDto),
    ShadowCaster2D(ShadowCasterDto),
    Rigidbody2D(an1mbyt3_core::Rigidbody2D),
    BoxCollider2D(an1mbyt3_core::BoxCollider2D),
    CircleCollider2D(an1mbyt3_core::CircleCollider2D),
    CapsuleCollider2D(an1mbyt3_core::CapsuleCollider2D),
    PolygonCollider2D(an1mbyt3_core::PolygonCollider2D),
    HingeJoint2D(an1mbyt3_core::HingeJoint2D),
    IkConstraint2D(an1mbyt3_core::IkConstraint2D),
    SpriteSkin2D(an1mbyt3_core::SpriteSkin2D),
    Liquid2D(an1mbyt3_core::Liquid2D),
    AudioSource(an1mbyt3_core::AudioSource),
    UiAnchor(an1mbyt3_core::UiAnchor),
    UiStack(an1mbyt3_core::UiStack),
    UiButton(an1mbyt3_core::UiButton),
    UiSlider(an1mbyt3_core::UiSlider),
    UiScrollbar(an1mbyt3_core::UiScrollbar),
    UiScrollView(an1mbyt3_core::UiScrollView),
    UiToggle(an1mbyt3_core::UiToggle),
    UiLayout(an1mbyt3_core::UiLayout),
    UiLayoutItem(an1mbyt3_core::UiLayoutItem),
    UiShowWhen(an1mbyt3_core::UiShowWhen),
    UiHtml(an1mbyt3_core::UiHtml),
    Rigidbody3D(an1mbyt3_core::Rigidbody3D),
    Collider3D(an1mbyt3_core::Collider3D),
    Flipbook(an1mbyt3_core::Flipbook),
    Cloth2D(an1mbyt3_core::Cloth2D),
    Tree2D(an1mbyt3_core::Tree2D),
    UiEvents(an1mbyt3_core::UiEvents),
    PropertyBindings(an1mbyt3_core::PropertyBindings),
}

impl From<&an1mbyt3_core::NodeBehavior> for NodeBehaviorDto {
    fn from(b: &an1mbyt3_core::NodeBehavior) -> Self {
        match b {
            an1mbyt3_core::NodeBehavior::SpriteRenderer(sprite) => NodeBehaviorDto::SpriteRenderer { material: sprite.material, color: sprite.color },
            an1mbyt3_core::NodeBehavior::Script(instance) => NodeBehaviorDto::Script {
                definition: instance.definition,
                values: instance.values.iter().map(|(name, value)| (name.clone(), value.into())).collect(),
            },
            an1mbyt3_core::NodeBehavior::Camera(c) => NodeBehaviorDto::Camera(c.into()),
            an1mbyt3_core::NodeBehavior::Light2D(l) => NodeBehaviorDto::Light2D(l.into()),
            an1mbyt3_core::NodeBehavior::TrailRenderer(t) => NodeBehaviorDto::TrailRenderer(t.into()),
            an1mbyt3_core::NodeBehavior::ParticleSystem(p) => NodeBehaviorDto::ParticleSystem(p.into()),
            an1mbyt3_core::NodeBehavior::ShadowCaster2D(c) => NodeBehaviorDto::ShadowCaster2D(c.into()),
            an1mbyt3_core::NodeBehavior::Rigidbody2D(r) => NodeBehaviorDto::Rigidbody2D(*r),
            an1mbyt3_core::NodeBehavior::BoxCollider2D(c) => NodeBehaviorDto::BoxCollider2D(*c),
            an1mbyt3_core::NodeBehavior::CircleCollider2D(c) => NodeBehaviorDto::CircleCollider2D(*c),
            an1mbyt3_core::NodeBehavior::CapsuleCollider2D(c) => NodeBehaviorDto::CapsuleCollider2D(*c),
            an1mbyt3_core::NodeBehavior::PolygonCollider2D(c) => NodeBehaviorDto::PolygonCollider2D(c.clone()),
            an1mbyt3_core::NodeBehavior::HingeJoint2D(c) => NodeBehaviorDto::HingeJoint2D(*c),
            an1mbyt3_core::NodeBehavior::IkConstraint2D(c) => NodeBehaviorDto::IkConstraint2D(*c),
            an1mbyt3_core::NodeBehavior::SpriteSkin2D(c) => NodeBehaviorDto::SpriteSkin2D(c.clone()),
            an1mbyt3_core::NodeBehavior::Liquid2D(c) => NodeBehaviorDto::Liquid2D(*c),
            an1mbyt3_core::NodeBehavior::AudioSource(c) => NodeBehaviorDto::AudioSource(*c),
            an1mbyt3_core::NodeBehavior::UiAnchor(c) => NodeBehaviorDto::UiAnchor(*c),
            an1mbyt3_core::NodeBehavior::UiStack(c) => NodeBehaviorDto::UiStack(*c),
            an1mbyt3_core::NodeBehavior::UiButton(c) => NodeBehaviorDto::UiButton(*c),
            an1mbyt3_core::NodeBehavior::UiSlider(c) => NodeBehaviorDto::UiSlider(*c),
            an1mbyt3_core::NodeBehavior::UiScrollbar(c) => NodeBehaviorDto::UiScrollbar(*c),
            an1mbyt3_core::NodeBehavior::UiScrollView(c) => NodeBehaviorDto::UiScrollView(*c),
            an1mbyt3_core::NodeBehavior::UiToggle(c) => NodeBehaviorDto::UiToggle(*c),
            an1mbyt3_core::NodeBehavior::UiLayout(c) => NodeBehaviorDto::UiLayout(*c),
            an1mbyt3_core::NodeBehavior::UiLayoutItem(c) => NodeBehaviorDto::UiLayoutItem(*c),
            an1mbyt3_core::NodeBehavior::UiShowWhen(c) => NodeBehaviorDto::UiShowWhen(*c),
            an1mbyt3_core::NodeBehavior::UiHtml(c) => NodeBehaviorDto::UiHtml(c.clone()),
            an1mbyt3_core::NodeBehavior::Rigidbody3D(c) => NodeBehaviorDto::Rigidbody3D(*c),
            an1mbyt3_core::NodeBehavior::Collider3D(c) => NodeBehaviorDto::Collider3D(*c),
            an1mbyt3_core::NodeBehavior::Flipbook(f) => NodeBehaviorDto::Flipbook(*f),
            an1mbyt3_core::NodeBehavior::Cloth2D(c) => NodeBehaviorDto::Cloth2D(*c),
            an1mbyt3_core::NodeBehavior::Tree2D(t) => NodeBehaviorDto::Tree2D(*t),
            an1mbyt3_core::NodeBehavior::UiEvents(e) => NodeBehaviorDto::UiEvents(e.clone()),
            an1mbyt3_core::NodeBehavior::PropertyBindings(b) => NodeBehaviorDto::PropertyBindings(b.clone()),
        }
    }
}

impl From<NodeBehaviorDto> for an1mbyt3_core::NodeBehavior {
    fn from(b: NodeBehaviorDto) -> Self {
        match b {
            NodeBehaviorDto::SpriteRenderer { material, color } => an1mbyt3_core::NodeBehavior::SpriteRenderer(an1mbyt3_core::SpriteRendererBehavior { material, color }),
            NodeBehaviorDto::Script { definition, values } => an1mbyt3_core::NodeBehavior::Script(an1mbyt3_core::ScriptInstance {
                definition,
                values: values.into_iter().map(|(name, value)| (name, value.into())).collect(),
            }),
            NodeBehaviorDto::Camera(c) => an1mbyt3_core::NodeBehavior::Camera(c.into()),
            NodeBehaviorDto::Light2D(l) => an1mbyt3_core::NodeBehavior::Light2D(l.into()),
            NodeBehaviorDto::TrailRenderer(t) => an1mbyt3_core::NodeBehavior::TrailRenderer(t.into()),
            NodeBehaviorDto::ParticleSystem(p) => an1mbyt3_core::NodeBehavior::ParticleSystem(p.into()),
            NodeBehaviorDto::ShadowCaster2D(c) => an1mbyt3_core::NodeBehavior::ShadowCaster2D(c.into()),
            NodeBehaviorDto::Rigidbody2D(r) => an1mbyt3_core::NodeBehavior::Rigidbody2D(r),
            NodeBehaviorDto::BoxCollider2D(c) => an1mbyt3_core::NodeBehavior::BoxCollider2D(c),
            NodeBehaviorDto::CircleCollider2D(c) => an1mbyt3_core::NodeBehavior::CircleCollider2D(c),
            NodeBehaviorDto::CapsuleCollider2D(c) => an1mbyt3_core::NodeBehavior::CapsuleCollider2D(c),
            NodeBehaviorDto::PolygonCollider2D(c) => an1mbyt3_core::NodeBehavior::PolygonCollider2D(c),
            NodeBehaviorDto::HingeJoint2D(c) => an1mbyt3_core::NodeBehavior::HingeJoint2D(c),
            NodeBehaviorDto::IkConstraint2D(c) => an1mbyt3_core::NodeBehavior::IkConstraint2D(c),
            NodeBehaviorDto::SpriteSkin2D(c) => an1mbyt3_core::NodeBehavior::SpriteSkin2D(c),
            NodeBehaviorDto::Liquid2D(c) => an1mbyt3_core::NodeBehavior::Liquid2D(c),
            NodeBehaviorDto::AudioSource(c) => an1mbyt3_core::NodeBehavior::AudioSource(c),
            NodeBehaviorDto::UiAnchor(c) => an1mbyt3_core::NodeBehavior::UiAnchor(c),
            NodeBehaviorDto::UiStack(c) => an1mbyt3_core::NodeBehavior::UiStack(c),
            NodeBehaviorDto::UiButton(c) => an1mbyt3_core::NodeBehavior::UiButton(c),
            NodeBehaviorDto::UiSlider(c) => an1mbyt3_core::NodeBehavior::UiSlider(c),
            NodeBehaviorDto::UiScrollbar(c) => an1mbyt3_core::NodeBehavior::UiScrollbar(c),
            NodeBehaviorDto::UiScrollView(c) => an1mbyt3_core::NodeBehavior::UiScrollView(c),
            NodeBehaviorDto::UiToggle(c) => an1mbyt3_core::NodeBehavior::UiToggle(c),
            NodeBehaviorDto::UiLayout(c) => an1mbyt3_core::NodeBehavior::UiLayout(c),
            NodeBehaviorDto::UiLayoutItem(c) => an1mbyt3_core::NodeBehavior::UiLayoutItem(c),
            NodeBehaviorDto::UiShowWhen(c) => an1mbyt3_core::NodeBehavior::UiShowWhen(c),
            NodeBehaviorDto::UiHtml(c) => an1mbyt3_core::NodeBehavior::UiHtml(c),
            NodeBehaviorDto::Rigidbody3D(c) => an1mbyt3_core::NodeBehavior::Rigidbody3D(c),
            NodeBehaviorDto::Collider3D(c) => an1mbyt3_core::NodeBehavior::Collider3D(c),
            NodeBehaviorDto::Flipbook(f) => an1mbyt3_core::NodeBehavior::Flipbook(f),
            NodeBehaviorDto::Cloth2D(c) => an1mbyt3_core::NodeBehavior::Cloth2D(c),
            NodeBehaviorDto::Tree2D(t) => an1mbyt3_core::NodeBehavior::Tree2D(t),
            NodeBehaviorDto::UiEvents(e) => an1mbyt3_core::NodeBehavior::UiEvents(e),
            NodeBehaviorDto::PropertyBindings(b) => an1mbyt3_core::NodeBehavior::PropertyBindings(b),
        }
    }
}

/// Mirrors `an1mbyt3_core::NodeBehaviorEntry`.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeBehaviorEntryDto {
    pub behavior: NodeBehaviorDto,
    pub enabled: bool,
}

impl From<&an1mbyt3_core::NodeBehaviorEntry> for NodeBehaviorEntryDto {
    fn from(e: &an1mbyt3_core::NodeBehaviorEntry) -> Self {
        NodeBehaviorEntryDto { behavior: (&e.behavior).into(), enabled: e.enabled }
    }
}

impl From<NodeBehaviorEntryDto> for an1mbyt3_core::NodeBehaviorEntry {
    fn from(e: NodeBehaviorEntryDto) -> Self {
        an1mbyt3_core::NodeBehaviorEntry { behavior: e.behavior.into(), enabled: e.enabled }
    }
}

fn summarize_component(id: ComponentId, definition: &ComponentDefinition) -> ComponentDefinitionSummary {
    let (min, max) = definition.path.bounds();
    let (width, height) = (max.x - min.x, max.y - min.y);
    ComponentDefinitionSummary {
        id,
        name: definition.name.clone(),
        default_fill_color: definition.default_fill_color,
        variants: definition.variants.iter().map(|v| ComponentVariantDto { name: v.name.clone(), fill_color: v.fill_color }).collect(),
        path_d: path_to_svg_d(&definition.path),
        view_box: [min.x, -max.y, width.max(1.0), height.max(1.0)],
    }
}

/// Mirrors `an1mbyt3_core::StateMachineLayerDef` — one extra, node-masked
/// layer's own little machine. `mask` reuses `an1mbyt3_core::NodeId`
/// directly, same as `StateDefDto::animation` reuses `AnimationId` — a
/// bare id, opaque to the frontend, echoed straight into `set_state_machine_layer_mask`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateMachineLayerDefDto {
    pub name: String,
    pub states: Vec<StateDefDto>,
    pub initial_state: String,
    pub mask: Vec<NodeId>,
}

/// Mirrors `an1mbyt3_core::StateMachineDef` — what the State Machine
/// panel's authoring UI reads/edits. `inputs` reuses `an1mbyt3_core::InputDef`
/// directly (its own two fields, `name`/`kind`, are already single words —
/// nothing for `rename_all = "camelCase"` to change — so a wrapper DTO would
/// just be a copy with no actual shape difference).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateMachineDefSummary {
    pub id: StateMachineId,
    pub name: String,
    pub inputs: Vec<InputDef>,
    pub states: Vec<StateDefDto>,
    pub initial_state: String,
    /// Mirrors `an1mbyt3_core::StateMachineDef::layers` — see
    /// `StateMachineLayerDefDto`'s own doc comment.
    pub layers: Vec<StateMachineLayerDefDto>,
}

/// Maps one `StateDef` slice (the base `states` or one layer's own) into
/// `StateDefDto`s — shared by `summarize_state_machine_def`'s base-layer
/// mapping and its per-layer mapping below, since both produce the exact
/// same shape.
fn states_to_dto(states: &[StateDef]) -> Vec<StateDefDto> {
    states
        .iter()
        .map(|state| StateDefDto {
            name: state.name.clone(),
            animation: state.animation,
            loop_mode: state.loop_mode,
            position: state.position.into(),
            transitions: state
                .transitions
                .iter()
                .map(|transition| TransitionDefDto {
                    to: transition.to.clone(),
                    condition: (&transition.condition).into(),
                    direction: transition.direction,
                    duration_seconds: transition.duration_seconds,
                    interruptible: transition.interruptible,
                })
                .collect(),
            blend: state.blend.as_ref().map(|b| Blend1DDefDto {
                input: b.input.clone(),
                low: b.low,
                high: b.high,
                low_value: b.low_value,
                high_value: b.high_value,
            }),
            program: state.program.as_ref().map(Into::into),
        })
        .collect()
}

fn summarize_state_machine_def(id: StateMachineId, def: &StateMachineDef) -> StateMachineDefSummary {
    StateMachineDefSummary {
        id,
        name: def.name.clone(),
        inputs: def.inputs.clone(),
        states: states_to_dto(&def.states),
        layers: def
            .layers
            .iter()
            .map(|layer| StateMachineLayerDefDto { name: layer.name.clone(), states: states_to_dto(&layer.states), initial_state: layer.initial_state.clone(), mask: layer.mask.clone() })
            .collect(),
        initial_state: def.initial_state.clone(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtboardSummary {
    pub id: ArtboardId,
    pub name: String,
    pub width: f32,
    pub height: f32,
    /// This artboard's pixelation block size, in pixels — see
    /// `Artboard::resolution_scale`'s own doc comment. The Inspector's
    /// "Canvas" section's resolution field.
    pub resolution_scale: f32,
    /// Fixed / Responsive screen settings (`an1mbyt3_core::ScreenSettings`).
    pub screen: an1mbyt3_core::ScreenSettings,
    /// Every non-deleted node, depth-first parent-then-children order (see
    /// `ordered_node_ids`'s doc comment for why this isn't root-only).
    pub nodes: Vec<NodeSummary>,
    /// Every non-deleted animation on this artboard, for the Timeline's
    /// animation dropdown.
    pub animations: Vec<AnimationSummary>,
    /// Echoes `WasmEditor`'s own `active_animation` session state back, so
    /// the dropdown knows which entry is selected without the frontend
    /// needing to track it separately.
    pub active_animation: Option<AnimationId>,
    /// Every non-deleted state machine defined on this artboard, for the
    /// State Machine panel's authoring UI.
    pub state_machines: Vec<StateMachineDefSummary>,
    /// Echoes `WasmEditor`'s own `active_state_machine` session state back —
    /// same reasoning as `active_animation`.
    pub active_state_machine: Option<StateMachineId>,
    /// This artboard's post-processing effect stack, in render order — the
    /// Effects panel's whole list, including disabled entries (see
    /// `Artboard::post_effects`'s own doc comment).
    pub post_effects: Vec<PostEffectLayerDto>,
    /// Which `Camera` node (if any) Playground actually views this artboard
    /// through — `Artboard::active_camera`'s own doc comment. A saved scene
    /// property (unlike `active_animation`/`active_state_machine`, which are
    /// `WasmEditor`'s own session state), so this is read straight off the
    /// artboard rather than threaded through `summarize_document`'s params.
    pub active_camera: Option<NodeId>,
    /// Physics 2D gravity (world units / s²).
    pub gravity: Vec2Dto,
    /// Physics 3D gravity `[x, y, z]`, m/s².
    pub gravity_3d: [f32; 3],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSummary {
    pub artboards: Vec<ArtboardSummary>,
    /// Every non-deleted component definition in the whole document (not
    /// artboard-scoped — see `ComponentDefinition`'s own doc comment for
    /// why), for the Components panel's library listing.
    pub components: Vec<ComponentDefinitionSummary>,
    /// Every image asset in the document — the Resources panel's flat
    /// listing (grouped/nested per `sprite_groups` below).
    pub images: Vec<ImageAssetSummary>,
    /// Every sprite group (e.g. one imported GIF's decoded frames) — purely
    /// organizational, see `an1mbyt3_core::SpriteGroup`'s own doc comment.
    pub sprite_groups: Vec<SpriteGroupSummary>,
    /// Every non-deleted script definition in the whole document — the
    /// Inspector's "Add Behavior" picker, same document-level-asset shape as
    /// `components` above.
    pub scripts: Vec<ScriptDefinitionSummary>,
    /// Every non-deleted prefab — the Resources panel's Prefabs folder.
    pub prefabs: Vec<PrefabSummary>,
    /// Every saved Paint-mode drawing — the Resources panel's Drawings
    /// folder. (The SVG itself isn't included — it can be large and this
    /// summary is rebuilt often; `WasmEditor::drawing_svg` reads it.)
    pub drawings: Vec<DrawingSummary>,
    /// Every non-deleted material — the Shader tab's picker and the
    /// Material component's dropdown.
    pub materials: Vec<MaterialSummary>,
    /// Every imported 3D model (the Mesh section's Shape → Model picker).
    pub models: Vec<ModelSummary>,
    /// Every imported sound (Resources → Sounds, the Audio Source picker).
    pub audio: Vec<AudioSummary>,
    /// Every code / markup file, with its text (the Code tab, `@file:`
    /// references in HTML UI and scripts), in path order.
    pub text_files: Vec<TextFileSummary>,
    /// Every scene's properties (Rive view models).
    pub properties: Vec<PropertySummary>,
}

/// One scene property.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertySummary {
    pub id: an1mbyt3_core::PropertyId,
    pub artboard: ArtboardId,
    pub name: String,
    pub kind: an1mbyt3_core::PropKind,
    /// Its starting value as JSON.
    pub default: String,
}

/// One code / markup file.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextFileSummary {
    pub id: an1mbyt3_core::TextFileId,
    pub path: String,
    pub text: String,
    /// Lowercased extension (`"html"`, `"css"`, `"js"`…), or `""`.
    pub extension: String,
}

/// One imported sound.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSummary {
    pub id: an1mbyt3_core::AudioId,
    pub name: String,
    pub byte_length: u32,
    /// Its file in the project folder, if it's linked to one.
    pub file: Option<String>,
}

/// One imported 3D model.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSummary {
    pub id: an1mbyt3_core::ModelId,
    pub name: String,
}

/// One material asset.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialSummary {
    pub id: an1mbyt3_core::MaterialId,
    pub name: String,
    /// `"ShaderGraph"`, `"Color"` or `"Textured"`.
    pub kind: &'static str,
}

/// One saved drawing in the Resources panel.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingSummary {
    pub id: an1mbyt3_core::DrawingId,
    pub name: String,
    /// `"Vector"`, `"Bitmap"` (stills from before frames existed) or
    /// `"Animation"` (frames × layers — what Paint saves now).
    pub kind: &'static str,
    /// Frames and layers (1 / 1 for the older stills).
    pub frame_count: u32,
    pub layer_count: u32,
    /// A bitmap drawing's image asset (its thumbnail, and what placing it
    /// adds); `None` for vector drawings.
    pub image: Option<an1mbyt3_core::ImageId>,
}

/// Result of `add_drawing` / `add_bitmap_drawing`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawingResult {
    pub document: DocumentSummary,
    pub drawing: an1mbyt3_core::DrawingId,
}

/// One prefab in the Resources panel.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefabSummary {
    pub id: an1mbyt3_core::PrefabId,
    pub name: String,
    pub node_count: u32,
}

/// `NodeSummary::prefab` — which prefab a node belongs to, and whether it
/// is that instance's root (the Layers panel badges roots; the Inspector's
/// prefab bar acts on the root).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefabLinkDto {
    pub prefab: an1mbyt3_core::PrefabId,
    pub is_root: bool,
}

/// Mirrors `an1mbyt3_core::ImageAsset` — one entry in the Resources panel's
/// listing, plus the Inspector's asset-mode fields when selected.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageAssetSummary {
    pub id: an1mbyt3_core::ImageId,
    pub name: String,
    /// The source's own decoded pixel dimensions (read from its PNG header
    /// only, not a full decode — see `an1mbyt3_image::png_dimensions`) —
    /// `None` if `source` isn't valid PNG bytes (shouldn't happen for
    /// anything `import_image` produced, but a summary shouldn't panic over
    /// a corrupt asset). Distinct from `ImageNode::width`/`height` (a
    /// placed node's own authored on-canvas size) and from
    /// `processing.resize` (the baked/re-sampled size) — this is just "how
    /// big is the source picture."
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub processing: an1mbyt3_core::ImageProcessing,
}

/// Mirrors `an1mbyt3_core::SpriteGroup`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpriteGroupSummary {
    pub id: an1mbyt3_core::SpriteGroupId,
    pub name: String,
    /// `(frame image id, delay in milliseconds)`, in playback order.
    pub frames: Vec<(an1mbyt3_core::ImageId, u32)>,
}

fn summarize_image(id: an1mbyt3_core::ImageId, asset: &an1mbyt3_core::ImageAsset) -> ImageAssetSummary {
    let dimensions = an1mbyt3_image::png_dimensions(&asset.source).ok();
    ImageAssetSummary {
        id,
        name: asset.name.clone(),
        width: dimensions.map(|(w, _)| w),
        height: dimensions.map(|(_, h)| h),
        processing: asset.processing,
    }
}

pub fn summarize_document(document: &Document, active_animation: Option<AnimationId>, active_state_machine: Option<StateMachineId>) -> DocumentSummary {
    DocumentSummary {
        components: document.components().map(|(id, def)| summarize_component(id, def)).collect(),
        images: document.images().map(|(id, asset)| summarize_image(id, asset)).collect(),
        sprite_groups: document
            .sprite_groups()
            .map(|(id, group)| SpriteGroupSummary { id, name: group.name.clone(), frames: group.frames.clone() })
            .collect(),
        scripts: document.scripts().map(|(id, def)| summarize_script(id, def)).collect(),
        prefabs: document.prefabs().map(|(id, def)| PrefabSummary { id, name: def.name.clone(), node_count: def.nodes.len() as u32 }).collect(),
        drawings: document
            .drawings()
            .map(|(id, d)| {
                let (kind, image, frame_count, layer_count) = match &d.content {
                    an1mbyt3_core::DrawingContent::Vector { .. } => ("Vector", None, 1, 1),
                    an1mbyt3_core::DrawingContent::Bitmap { image } => ("Bitmap", Some(*image), 1, 1),
                    an1mbyt3_core::DrawingContent::Animation(a) => ("Animation", None, a.frame_count, a.layers.len() as u32),
                };
                DrawingSummary { id, name: d.name.clone(), kind, image, frame_count, layer_count }
            })
            .collect(),
        models: document.models().map(|(id, m)| ModelSummary { id, name: m.name.clone() }).collect(),
        text_files: document.text_files().map(|(id, f)| TextFileSummary { id, path: f.path.clone(), text: f.text.clone(), extension: f.extension() }).collect(),
        properties: document.properties().map(|(id, p)| PropertySummary { id, artboard: p.artboard, name: p.name.clone(), kind: p.kind, default: p.default.clone() }).collect(),
        audio: document.audio_assets().map(|(id, a)| AudioSummary { id, name: a.name.clone(), byte_length: a.source.len() as u32, file: a.file.clone() }).collect(),
        materials: document
            .materials()
            .map(|(id, m)| MaterialSummary {
                id,
                name: m.name.clone(),
                kind: match m.kind {
                    an1mbyt3_core::MaterialKind::ShaderGraph(_) => "ShaderGraph",
                    an1mbyt3_core::MaterialKind::Color(_) => "Color",
                    an1mbyt3_core::MaterialKind::Textured { .. } => "Textured",
                },
            })
            .collect(),
        artboards: document
            .artboards()
            .map(|(artboard_id, artboard)| ArtboardSummary {
                id: artboard_id,
                name: artboard.name.clone(),
                width: artboard.width,
                height: artboard.height,
                resolution_scale: artboard.resolution_scale,
                screen: artboard.screen,
                // Depth-first, parent-then-children order (`ordered_node_ids`)
                // rather than `root_nodes()` alone — a bone chain's second
                // bone is `Node::parent`-nested, not a second root, so a
                // root-only listing would silently drop it from the Layers/
                // Bones panel and the canvas selection outline. Skips
                // soft-deleted nodes (and their whole subtree) — see
                // `Command::SetDeleted`'s doc comment (`an1mbyt3_core::command`)
                // for why deletion is a flag flip rather than real
                // `Artboard`-arena removal, and why every render/UI-facing
                // listing has to filter it out itself, same as
                // `Artboard::shapes`/`texts` already do.
                nodes: ordered_node_ids(artboard)
                    .into_iter()
                    .filter_map(|node_id| {
                        let node = artboard
                            .node(node_id)
                            .expect("ordered_node_ids returned an id not in its own arena");
                        if node.deleted {
                            return None;
                        }

                        let tracks = active_animation
                            .and_then(|animation_id| artboard.animation(animation_id))
                            .map(|animation| {
                                animation
                                    .tracks
                                    .iter()
                                    .filter(|(n, _)| *n == node_id)
                                    .map(|(_, track)| summarize_property_track(track))
                                    .collect()
                            })
                            .unwrap_or_default();

                        let (local_min, local_max) = local_bounds(artboard, node_id);
                        Some(NodeSummary {
                            id: node_id,
                            name: node.name.clone(),
                            element_id: node.id.clone(),
                            classes: node.classes.clone(),
                            kind: match node.kind {
                                NodeKind::Shape(_) => "Shape",
                                NodeKind::Group(_) => "Group",
                                NodeKind::Bone(_) => "Bone",
                                NodeKind::Instance(_) => "Instance",
                                NodeKind::Text(_) => "Text",
                                NodeKind::Image(_) => "Image",
                                NodeKind::ParticleSystem(_) => "ParticleSystem",
                                NodeKind::Tilemap(_) => "Tilemap",
                                NodeKind::Camera(_) => "Camera",
                                NodeKind::Light2D(_) => "Light2D",
                                NodeKind::Trail(_) => "Trail",
                                NodeKind::Node3D(_) => "Node3D",
                            },
                            transform: node.transform.into(),
                            parent: node.parent,
                            bone_length: match &node.kind {
                                NodeKind::Bone(bone) => Some(bone.length),
                                _ => None,
                            },
                            group_mask: match &node.kind {
                                NodeKind::Group(group) => group.mask,
                                _ => None,
                            },
                            group_y_sort: match &node.kind {
                                NodeKind::Group(group) => Some(group.y_sort_enabled),
                                _ => None,
                            },
                            clip_children: node.clip_children,
                            visible: node.visible,
                            z_index: node.z_index,
                            blend_mode: match &node.kind {
                                NodeKind::Shape(shape) => Some(shape.blend_mode),
                                _ => None,
                            },
                            fill_color: match &node.kind {
                                NodeKind::Shape(shape) => Some(shape.fill_color),
                                NodeKind::Text(text) => Some(text.fill_color),
                                NodeKind::Image(image) => Some(image.fill_color),
                                _ => None,
                            },
                            gradient: match &node.kind {
                                NodeKind::Shape(shape) => shape.gradient.as_ref().map(GradientDefDto::from),
                                NodeKind::Text(text) => text.gradient.as_ref().map(GradientDefDto::from),
                                _ => None,
                            },
                            shape_image: match &node.kind {
                                NodeKind::Shape(shape) => shape.image,
                                _ => None,
                            },
                            path: match &node.kind {
                                NodeKind::Shape(shape) => Some(PathDto::from(&shape.path)),
                                _ => None,
                            },
                            stroke: match &node.kind {
                                NodeKind::Shape(shape) => shape.stroke.as_ref().map(StrokeDto::from),
                                _ => None,
                            },
                            text_content: match &node.kind {
                                NodeKind::Text(text) => Some(text.content.clone()),
                                _ => None,
                            },
                            text_size_px: match &node.kind {
                                NodeKind::Text(text) => Some(text.size_px),
                                _ => None,
                            },
                            component: match &node.kind {
                                NodeKind::Instance(instance) => Some(instance.component),
                                _ => None,
                            },
                            fill_color_override: match &node.kind {
                                NodeKind::Instance(instance) => instance.fill_color_override,
                                _ => None,
                            },
                            variant: match &node.kind {
                                NodeKind::Instance(instance) => instance.variant.clone(),
                                _ => None,
                            },
                            resolved_fill_color: match &node.kind {
                                NodeKind::Instance(instance) => document.component(instance.component).map(|def| instance.resolved_fill_color(def)),
                                _ => None,
                            },
                            image: match &node.kind {
                                NodeKind::Image(image) => Some(image.image),
                                _ => None,
                            },
                            image_width: match &node.kind {
                                NodeKind::Image(image) => Some(image.width),
                                _ => None,
                            },
                            image_height: match &node.kind {
                                NodeKind::Image(image) => Some(image.height),
                                _ => None,
                            },
                            particle_system: match &node.kind {
                                _ => node.particle_system().map(Into::into),
                            },
                            tilemap: match &node.kind {
                                NodeKind::Tilemap(tilemap) => Some(tilemap.into()),
                                _ => None,
                            },
                            camera: node.camera().map(Into::into),
                            light2d: node.light2d().map(Into::into),
                            trail: node.trail().map(Into::into),
                            node3d: match &node.kind {
                                NodeKind::Node3D(n) => Some(*n),
                                _ => None,
                            },
                            unlit: node.unlit,
                            shadow_caster: node.shadow_caster().map(Into::into),
                            prefab: node.prefab.map(|link| PrefabLinkDto {
                                prefab: link.prefab,
                                is_root: document.prefab(link.prefab).is_some_and(|def| !def.deleted && def.root_uid() == link.uid),
                            }),
                            behaviors: node.behaviors.iter().map(Into::into).collect(),
                            tracks,
                            local_min: local_min.into(),
                            local_max: local_max.into(),
                            world_transform: artboard.world_transform(node_id).into(),
                            parent_world_transform: node.parent.map(|parent_id| artboard.world_transform(parent_id).into()),
                        })
                    })
                    .collect(),
                animations: artboard
                    .animations()
                    .map(|(id, animation)| AnimationSummary { id, name: animation.name.clone(), duration: animation.duration })
                    .collect(),
                active_animation,
                state_machines: artboard.state_machines().map(|(id, def)| summarize_state_machine_def(id, def)).collect(),
                active_state_machine,
                post_effects: artboard.post_effects().iter().map(PostEffectLayerDto::from).collect(),
                active_camera: artboard.active_camera(),
                gravity: artboard.gravity.into(),
                gravity_3d: artboard.gravity_3d.to_array(),
            })
            .collect(),
    }
}

/// Result of `WasmEditor::add_shape_node`: the updated document plus the
/// newly created node's id, so the frontend can select it immediately (see
/// that method's doc comment). `rename_all = "camelCase"` for the same
/// reason `StateMachineSummary` below needs it — `new_node` is multi-word.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddNodeResult {
    pub document: DocumentSummary,
    pub new_node: NodeId,
}

/// Result of `WasmEditor::add_artboard`: the updated document plus the new
/// scene's id, so the Scenes switcher can jump straight to it — same shape
/// as `AddNodeResult`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddArtboardResult {
    pub document: DocumentSummary,
    pub new_artboard: ArtboardId,
}

/// Result of `WasmEditor::import_svg`: the updated document plus every
/// newly created shape's id, in the same order `an1mbyt3_svg::import_svg`
/// returned them — an SVG can drop in many shapes at once (unlike every
/// other `Add*Result` here, which only ever creates one thing), so this is
/// a `Vec`, not a single `NodeId`. The frontend selects the first one (see
/// `editor.rs`'s `import_svg` doc comment) the same "select whatever you
/// just created" convention `add_shape_node` established.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSvgResult {
    pub document: DocumentSummary,
    pub new_nodes: Vec<NodeId>,
}

/// Result of `WasmEditor::import_image` — a single still image creates one
/// canvas node (`new_node: Some(id)`, mirroring `add_shape_node`'s "select
/// what you just created" convention), while a GIF creates none at all
/// (`new_node: None`) since its decoded frames are frames-only this pass —
/// only the Resources panel needs to know about `new_sprite_group`, see that
/// method's own doc comment.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportImageResult {
    pub document: DocumentSummary,
    pub new_node: Option<NodeId>,
    pub new_images: Vec<an1mbyt3_core::ImageId>,
    pub new_sprite_group: Option<an1mbyt3_core::SpriteGroupId>,
}

/// Result of `WasmEditor::duplicate_nodes` — same "document plus every
/// freshly created id" shape as `ImportSvgResult`, kept as its own type for
/// the same "let each action's wasm-bindgen surface evolve independently"
/// reasoning `ImportSvgResult`'s own doc comment gives. `new_nodes` here is
/// only the *topmost* duplicate per requested
/// node (not every recursed descendant) — see `WasmEditor::duplicate_nodes`'s
/// doc comment for why: the caller selects exactly these.
/// One file of `WasmEditor::save_project`'s listing: `text` for text
/// files, `bytes` (an index for `take_project_file_bytes`) for binary ones.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFileDto {
    pub path: String,
    pub text: Option<String>,
    pub bytes: Option<u32>,
    /// An image's own source file — write it only if it's missing on disk.
    pub linked: bool,
    /// The document item the file holds (`None` for the manifest).
    pub asset: Option<ProjectAssetDto>,
}

/// `ProjectFileDto::asset`: `kind` is `"scene" | "prefab" | "image" | "audio" | "model"`, `id`
/// the item's id in the usual `{ idx, version }` shape.
#[derive(Serialize)]
pub struct ProjectAssetDto {
    pub kind: &'static str,
    pub id: OpaqueIdDto,
}

#[derive(Serialize)]
pub struct OpaqueIdDto {
    pub idx: u32,
    pub version: u32,
}

impl From<an1mbyt3_format::ProjectAsset> for ProjectAssetDto {
    fn from(asset: an1mbyt3_format::ProjectAsset) -> Self {
        Self { kind: asset.kind.as_str(), id: OpaqueIdDto { idx: asset.idx, version: asset.version } }
    }
}

/// One node of a runtime `spawn_prefab` result.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnedPart {
    pub id: NodeId,
    pub name: String,
    /// The part's own local transform and fill as spawned — scripts seed
    /// their refs with these, so writing one property (say `x`) doesn't
    /// also send a guessed `y` and knock the part out of place.
    pub transform: Transform2DDto,
    pub fill_color: Option<[f32; 4]>,
}

/// Result of the prefab editing calls (`create_prefab`,
/// `instantiate_prefab`): the updated document plus what was created.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefabResult {
    pub document: DocumentSummary,
    pub prefab: Option<an1mbyt3_core::PrefabId>,
    pub node: Option<NodeId>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateNodesResult {
    pub document: DocumentSummary,
    pub new_nodes: Vec<NodeId>,
}

/// Result of `WasmEditor::add_animation`: the updated document plus the
/// newly created animation's id, so the frontend can select it in the
/// dropdown immediately.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddAnimationResult {
    pub document: DocumentSummary,
    pub new_animation: AnimationId,
}

/// State-machine-preview counterpart to `DocumentSummary` — what the
/// editor's State Machine tab needs to render its states list and trigger
/// buttons. Same DTO reasoning as everything else in this file: don't expose
/// `StateMachine`'s own internals (it isn't even `Serialize`) directly across
/// the wasm-bindgen boundary.
// `rename_all = "camelCase"` — unlike every other DTO in this file (whose
// field names happen to already be single words), this one has multi-word
// fields, and `serde_wasm_bindgen` serializes field names verbatim
// (snake_case) with no automatic JS-convention renaming the way Tauri's IPC
// layer sometimes does — without this, the frontend would see
// `active_state`/`available_triggers`, not the idiomatic `activeState`/
// `availableTriggers` its TypeScript `StateMachineSummary` interface expects.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateMachineSummary {
    pub states: Vec<String>,
    pub active_state: String,
    /// Trigger-input names the *current* state's transitions check — see
    /// `StateMachine::available_triggers`'s doc comment for why this is
    /// scoped to the active state, not every trigger anywhere in the machine.
    pub available_triggers: Vec<String>,
    /// Every extra layer's own live current state, keyed by layer name —
    /// lets the graph highlight whichever layer's box is actually active,
    /// not just the base layer's (`active_state` above). See
    /// `StateMachine::layer_active_states`'s own doc comment.
    pub layer_active_states: std::collections::HashMap<String, String>,
}

pub fn summarize_state_machine(state_machine: &StateMachine) -> StateMachineSummary {
    StateMachineSummary {
        states: state_machine.state_names().map(str::to_string).collect(),
        active_state: state_machine.current_state_name().to_string(),
        available_triggers: state_machine.available_triggers().map(str::to_string).collect(),
        layer_active_states: state_machine.layer_active_states().map(|(name, state)| (name.to_string(), state.to_string())).collect(),
    }
}
