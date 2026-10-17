//! `WasmEditor`: the real document model + real renderer, both living in the
//! browser/webview process — a deliberate reversal of AN1MBYT3_BUILD_PLAN.md
//! §1's original desktop-editor principle ("Tauri backend runs the real
//! engine, not WASM, for the desktop case").
//!
//! Why reverse it: the desktop editor's canvas used to be a *readback
//! bridge* — a native Rust process (`an1mbyt3_renderer::Renderer::new_headless`
//! + `render_to_texture`) rendering off-screen and shipping a full RGBA frame
//! over Tauri IPC every animation frame. That's exactly how Rive's web
//! runtime *and* Rive's own web-based editor work instead: the engine
//! compiles to WASM and renders directly into an HTML `<canvas>` via
//! WebGPU/WebGL2, with no native process and no bridge at all — because
//! there's nothing native to bridge to. Rive's editor being a pure web app
//! (not a native desktop shell) is *why* they never needed a readback
//! bridge or native window compositing in the first place.
//!
//! Applying that here: `WasmEditor` is the exact same wasm module the web
//! runtime (`WasmPlayer`/`WasmBoneIk`) already uses, just also holding a
//! real, editable `an1mbyt3_core::Document` (with undo/redo) instead of a
//! read-only demo scene, and rendering via a *real* (non-headless)
//! `wgpu::Surface` built from the canvas — `Renderer::new`, not
//! `new_headless`. Zero per-frame IPC: `render()` draws directly to the
//! surface the canvas already owns. The Tauri native backend
//! (`apps/editor/src-tauri`) is now a thin shell (file dialogs, disk I/O) —
//! it no longer owns the document or does any rendering.
//!
//! `dto.rs` (this crate) mirrors `apps/editor/src-tauri/src/dto.rs`'s DTOs
//! exactly, serialized via `serde_wasm_bindgen` instead of Tauri's JSON IPC —
//! same reasoning, different boundary. The frontend's existing TypeScript
//! `DocumentSummary`/`OpaqueId` types didn't need to change at all.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;

use glam::Vec2;
use an1mbyt3_core::{
    Animation, AnimationId, ArtboardId, BoneNode, Command, ComponentDefinition, ComponentId, ComponentInstance,
    ComponentVariant, Document, FontAsset, FontId, GroupNode,
    InputDef, InputKind, KeyframeValue, Lerp, Node, NodeId, NodeKind, Path, PathCommand, PlaybackDirection, PostEffectLayer, ProgramDef,
    PropertyKind, PropertyTrack, ScriptDefinition, ScriptId, ShapeNode, StateDef, StateMachineDef, TextNode, Track, TransitionDef,
};
use an1mbyt3_geometry::{tessellate_path, tessellate_stroke, Mesh, StrokeCap, StrokeJoin, StrokeStyle};
use an1mbyt3_renderer::{BlendMode, Camera, DrawBinding, DrawCall, GpuMesh, Renderer};
use an1mbyt3_runtime::{Easing, Keyframe, LoopMode, StateMachine};
use an1mbyt3_text::{shape_and_tessellate, Font};

use crate::dto::{
    summarize_document, summarize_state_machine, AddAnimationResult, AddNodeResult, DuplicateNodesResult, PrefabResult, ProjectFileDto, SpawnedPart, ImportImageResult, ImportSvgResult,
    NodeBehaviorEntryDto, ScriptFieldDefDto,
};

/// A shaped-and-tessellated `NodeKind::Text` run's uploaded glyph meshes, plus
/// the `(content, size_px)` key they were shaped from — mirrors
/// `an1mbyt3-text`'s `text_shape` example's `ShapedRunCache`, the same
/// re-shape-only-when-the-key-changes discipline `an1mbyt3_text::run`'s
/// module doc asks of every caller (AN1MBYT3_BUILD_PLAN.md §6: transform-only
/// movement never touches this — the key has no transform in it).
struct TextMeshCache {
    /// Content, size and font — a change to any of them re-shapes.
    key: (String, f32, FontId),
    glyphs: Vec<(GpuMesh, DrawBinding, Vec2)>,
    /// The run's real local bounds (glyph outlines, horizontally centred,
    /// baseline at 0) — what UI layouts measure text by, instead of
    /// `dto::local_bounds`' per-character estimate.
    bounds: (Vec2, Vec2),
}

/// The "nothing opened yet" starting document: one 640x480 "Main" scene
/// holding only a Main Camera (set as the scene's camera) — nothing to
/// delete before starting real work. It also gets one empty
/// "Animation 1" (not a scene node), so the Timeline's animation dropdown
/// never starts genuinely empty, the way a fresh project in most animation
/// tools starts with one timeline already.
fn build_start_document() -> (Document, AnimationId) {
    let mut document = Document::new();
    let artboard_id = document.add_artboard(an1mbyt3_core::Artboard::new("Main", 640.0, 480.0));
    let artboard = document.artboard_mut(artboard_id).unwrap();
    let (camera, _) = default_scene_nodes();
    let camera_id = artboard.add_node(camera);
    let animation_id = artboard.add_animation(Animation::new("Animation 1"));
    // `Command::apply` directly (not `Document::apply`): starting content,
    // not an undoable edit.
    Command::SetActiveCamera { artboard: artboard_id, value: Some(camera_id) }.apply(&mut document);
    (document, animation_id)
}

/// A live-preview `StateMachine` for a document that has none yet (the
/// start document): one state on `animation`, built from a definition that
/// is *not* added to the document — `WasmEditor::state_machine` can't be
/// empty (a runtime layer needs a state), and `active_state_machine: None`
/// keeps every authoring path from treating this as a real machine. The
/// first real state machine replaces it via `refresh_state_machine_preview`.
fn placeholder_state_machine(artboard: &an1mbyt3_core::Artboard, animation: AnimationId) -> StateMachine {
    let def = StateMachineDef {
        name: String::new(),
        inputs: Vec::new(),
        states: vec![StateDef {
            name: "Entry".into(),
            animation,
            loop_mode: LoopMode::Once,
            transitions: Vec::new(),
            position: Vec2::ZERO,
            blend: None,
            program: None,
        }],
        initial_state: "Entry".into(),
        layers: Vec::new(),
        deleted: false,
    };
    StateMachine::from_definition(&def, artboard)
}

/// Converts every node still using a legacy component *kind* (Camera /
/// Light 2D / Trail / Particle System, or the old `shadow_caster` field)
/// into an empty GameObject carrying that component — run on load and after
/// building demos, so the rest of the editor only ever sees components.
/// Direct (not a command): it's a representation change, not an edit.
pub(crate) fn migrate_document(document: &mut Document) {
    let artboard_ids: Vec<ArtboardId> = document.artboards().map(|(id, _)| id).collect();
    for artboard_id in artboard_ids {
        let Some(artboard) = document.artboard_mut(artboard_id) else { continue };
        let node_ids: Vec<NodeId> = artboard.node_ids().collect();
        for id in node_ids {
            if let Some(node) = artboard.node_mut(id) {
                node.migrate_legacy_kind_to_behaviors();
            }
        }
    }
}

/// The Main Camera + Global Light 2D every new scene starts with — see
/// `build_start_document` (camera only) and `WasmEditor::add_artboard`.
fn default_scene_nodes() -> (Node, Node) {
    let camera = Node::with_components("Main Camera", None, vec![an1mbyt3_core::NodeBehavior::Camera(an1mbyt3_core::CameraNode::default())]);
    let light = Node::with_components("Global Light 2D", None, vec![an1mbyt3_core::NodeBehavior::Light2D(an1mbyt3_core::Light2DNode::global())]);
    (camera, light)
}

/// Loads (once per document) the bundled UI font the "Text" item uses when
/// the document has no font of its own yet.
fn default_text_font(document: &mut Document) -> FontId {
    if let Some((id, _)) = document.fonts().next() {
        return id;
    }
    document.add_font(FontAsset { name: "Russo One".into(), data: GEOMETRY_DASH_FONT_BYTES.to_vec() })
}

/// A hand-authored upward-pointing triangle (base flush with the local
/// origin, tip at `+y`) — deliberately *not* `Path::polygon(3, radius)`:
/// `an1mbyt3_renderer::build_mvp`'s own doc comment states world space is
/// `+x right, +y up`, which makes `Path::polygon`'s first vertex (`(0,
/// -radius)`, per its own doc comment's "-90°") render pointing *down*, the
/// opposite of what that doc comment itself claims — a latent inconsistency
/// somewhere between that function and the renderer's actual convention.
/// Rather than risk silently flipping every existing Triangle/Star shape
/// preset (and any `.tea` already authored with one) by "fixing" a long-
/// lived, widely-used function based on that read, this spike geometry is
/// authored from scratch against the *renderer's* own documented convention,
/// which this whole level's layout also follows.
fn spike_triangle_path(radius: f32) -> Path {
    let half_width = radius * 0.8660254; // sin(60°) — same proportions `Path::polygon(3, _)` uses
    let base_y = -radius * 0.5;
    Path {
        commands: vec![
            PathCommand::MoveTo(Vec2::new(0.0, radius)),
            PathCommand::LineTo(Vec2::new(half_width, base_y)),
            PathCommand::LineTo(Vec2::new(-half_width, base_y)),
            PathCommand::Close,
        ],
    }
}

/// The one font this whole demo ships — Russo One (SIL Open Font License,
/// see `assets/fonts/RussoOne-OFL.txt` alongside it), a bold geometric
/// display face that reads well at both title-card and small-HUD-digit
/// sizes, so one embedded family covers every text need here instead of
/// pulling in a second one. `include_bytes!` resolves at compile time, so
/// this works in the `wasm32-unknown-unknown` build same as any other
/// target — no filesystem access needed at runtime (there isn't any, inside
/// a browser/webview).
const GEOMETRY_DASH_FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/RussoOne-Regular.ttf");

/// Builds a `NodeKind::Text` node at `(center_x, y)` — despite the name,
/// no width measurement/offset happens here: `WasmEditor::render`'s own
/// `text_mesh_cache` population (`editor.rs`, right where it calls
/// `shape_and_tessellate`) already recenters every shaped glyph run by half
/// its own total advance before caching it, so *every* `Text` node's
/// `transform.translation` already means "my own horizontal center," not a
/// baseline start — this helper doing its own width-based centering on top
/// of that would double it. Kept named/shaped like a "centering" call (not
/// just inlining the `Node::new` + translation) so every call site below
/// still reads the same regardless of which layer ends up owning that
/// behavior.
fn centered_text_node(parent: Option<NodeId>, content: &str, font_id: FontId, size_px: f32, color: [f32; 4], center_x: f32, y: f32) -> Node {
    let mut node = Node::new(content, parent, NodeKind::Text(TextNode { content: content.to_string(), font: font_id, size_px, fill_color: color, gradient: None }));
    node.transform.translation = Vec2::new(center_x, y);
    node
}

/// A demo, user-loadable ("Load Geometry Dash Demo," the Shapes menu — see
/// `WasmEditor::load_geometry_dash_demo`) full-flow auto-runner: a title
/// card, a scrolling level with a live HUD (progress bar, real text),
/// instant-retry-on-death, procedurally streamed (spawn-ahead, despawn-
/// behind) obstacles, a scripted "launched into the wall" finish with
/// camera shake, a win card, and back to the title — built on `ProgramDef::Code`
/// (real JavaScript, `scriptRuntime.ts`) rather than `ProgramDef::Blocks`.
///
/// An earlier version of this exact demo was built on Blocks — every single
/// extra capability it needed (live text, a camera hook, arrays of tracked
/// live obstacles) needed its own hand-added engine primitive first
/// (`Statement::SetTextNumber`, and still no camera/audio hook at all for
/// Blocks). Code mode already had a real scripting language sitting one
/// level up the stack the whole time: this rewrite uses that instead —
/// real `let`/arrays/functions for all of this demo's own state (no more
/// hidden "data register" nodes repurposing `FillColorR` as a generation
/// counter), a real camera (`ctx.camera`/`ctx.setCamera`, this whole
/// engine's *first* script-reachable camera hook — see this function's own
/// JS source for the shake), and real procedural audio via the page's own
/// `AudioContext` (Code mode runs fully unsandboxed — `scriptRuntime.ts`'s
/// own module doc explains why `new Function` was chosen specifically so a
/// script *can* reach the ordinary web platform, not just an engine-blessed
/// subset of it).
///
/// **Persistence across frames**: `runScript` recompiles nothing (cached by
/// source identity) but re-*executes* the whole script body fresh every
/// call — a plain `let` declared in the script resets every frame. This
/// demo's own state (`scene`, velocity, the live spike list, ...) lives on
/// `window.__gd` instead, initialized once (`window.__gd ?? { ... }`) and
/// mutated in place every frame after — the same "stash your own instance
/// state somewhere that outlives one call" idea a Lua coroutine's upvalues
/// or a `MonoBehaviour`'s own fields give you for free, just done by hand
/// here since a Code-mode script has no engine-provided equivalent yet.
///
/// **In-game "scenes" here are still sibling `Group`s** (`title`/`hud`/`win`)
/// shown/hidden via `ctx.setVisible`, driven by a real `if (scene ===
/// 'title')`. Editor-level scenes (separate artboards, `WasmEditor::
/// set_active_artboard`) exist now, but a running script can't yet switch
/// between them — that's a runtime-side "load scene" capability, separate
/// from the editor's own scene switcher.
fn build_demo_geometry_dash(document: &mut Document, artboard_id: ArtboardId) -> an1mbyt3_core::StateMachineId {
    // Only the constants this function's own node *placement* actually
    // needs — layout only, no physics (gravity/impulse/scroll speed never
    // position anything here, they only drive runtime motion, so they live
    // solely in `GEOMETRY_DASH_SCRIPT`'s own copy below; that constant's own
    // doc comment explains why these two lists are hand-kept-in-sync
    // duplicates rather than one `format!`-interpolated source — escaping
    // every brace in a few hundred lines of JS would be the worse tradeoff).
    // Signs follow the renderer's own `+x right, +y up` world convention
    // (see `spike_triangle_path`'s doc comment).
    const PLAYER_SIZE: f32 = 40.0;
    const PLAYER_X: f32 = -250.0;
    const GROUND_SURFACE_Y: f32 = -100.0;
    const PLAYER_GROUND_Y: f32 = GROUND_SURFACE_Y + PLAYER_SIZE / 2.0;
    const GROUND_THICKNESS: f32 = 40.0;
    const SPIKE_RADIUS: f32 = 20.0;
    const SPIKE_Y: f32 = GROUND_SURFACE_Y + SPIKE_RADIUS * 0.5;
    const WALL_X: f32 = 6500.0;
    const LEVEL_LENGTH: f32 = WALL_X + 200.0;
    const BAR_WIDTH: f32 = 300.0;
    const BAR_HEIGHT: f32 = 18.0;
    const BAR_Y: f32 = 210.0;
    const BAR_LEFT_X: f32 = -BAR_WIDTH / 2.0;
    const HUD_TEXT_SIZE: f32 = 26.0;
    const PERCENT_Y: f32 = 178.0;
    const ATTEMPT_Y: f32 = 148.0;
    const ATTEMPT_LABEL_SIZE: f32 = 18.0;

    let font_id = document.add_font(FontAsset { name: "Russo One".into(), data: GEOMETRY_DASH_FONT_BYTES.to_vec() });

    let mut inverses = Vec::new();
    let add_node = |document: &mut Document, inverses: &mut Vec<Command>, node: Node| -> NodeId {
        let inverse = document.apply(Command::AddNode { artboard: artboard_id, node });
        let Command::SetDeleted { node: id, .. } = inverse else {
            unreachable!("AddNode's inverse is always a SetDeleted")
        };
        inverses.push(inverse);
        id
    };
    // Every node the script addresses gets a real author-facing `Node::id`
    // (`getCompByID`'s own lookup key) instead of the auto-generated
    // `shape(<serial>)` `add_node` would otherwise mint — named to match
    // exactly what `GEOMETRY_DASH_SCRIPT` below calls `getCompByID` with.
    let with_id = |mut node: Node, id: &str| -> Node {
        node.id = id.to_string();
        node
    };

    // Drawn first (bottom of the stack) — a big, dark, always-visible
    // backdrop panel behind every scene (title card, level, win card all
    // sit on top of it), so there's never a flash of the raw canvas color
    // behind any screen. The script pulses its own blue channel slowly,
    // regardless of `scene` — the one thing that never stops animating.
    // Real Geometry Dash stage backdrops are a vertical gradient (deep
    // violet-blue up top fading to a richer blue near the ground), not a
    // flat near-black panel — a `GradientDef` on the same rect the script's
    // own slow blue-channel pulse already tints. `start`/`end` are in the
    // path's own *local* space (`GradientDef::start`'s doc comment), so
    // `800.0`/`-800.0` are this 1600-tall rect's own top/bottom edges.
    let mut background = with_id(
        Node::new(
            "Background",
            None,
            NodeKind::Shape(ShapeNode {
                path: Path::rect(3000.0, 1600.0),
                fill_color: [0.06, 0.07, 0.12, 1.0],
                blend_mode: Default::default(),
                gradient: Some(an1mbyt3_core::GradientDef {
                    kind: an1mbyt3_core::GradientKind::Linear,
                    start: Vec2::new(0.0, 800.0),
                    end: Vec2::new(0.0, -800.0),
                    stops: vec![
                        an1mbyt3_core::GradientStop { offset: 0.0, color: [0.1, 0.08, 0.28, 1.0], midpoint: 0.5 },
                        an1mbyt3_core::GradientStop { offset: 1.0, color: [0.04, 0.2, 0.42, 1.0], midpoint: 0.5 },
                    ],
                }),
                image: None,
                stroke: None,
            }),
        ),
        "background",
    );
    background.transform.translation = Vec2::new(PLAYER_X, 0.0);
    add_node(document, &mut inverses, background);

    // Hidden on the title card — the script's own `showPlayArea`/`hidePlayArea`
    // helpers toggle this back on/off on every scene transition.
    let mut level_group = with_id(Node::new("Level", None, NodeKind::Group(GroupNode::default())), "level");
    level_group.visible = false;
    let level_group_id = add_node(document, &mut inverses, level_group);

    let mut ground = Node::new(
        "Ground",
        Some(level_group_id),
        NodeKind::Shape(ShapeNode { path: Path::rect(LEVEL_LENGTH, GROUND_THICKNESS), fill_color: [0.13, 0.14, 0.2, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None }),
    );
    ground.transform.translation = Vec2::new(LEVEL_LENGTH / 2.0, GROUND_SURFACE_Y - GROUND_THICKNESS / 2.0);
    add_node(document, &mut inverses, ground);

    // The bright accent line along the ground's top edge — every real GD
    // ground tile has one; a flat dark ground body alone reads as a plain
    // placeholder bar. A separate thin rect rather than a `Stroke` on
    // `ground` itself so it only marks the *top* edge, not all four sides.
    const GROUND_STRIPE_THICKNESS: f32 = 8.0;
    let mut ground_stripe = Node::new(
        "Ground Stripe",
        Some(level_group_id),
        NodeKind::Shape(ShapeNode { path: Path::rect(LEVEL_LENGTH, GROUND_STRIPE_THICKNESS), fill_color: [0.3, 0.95, 0.85, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None }),
    );
    ground_stripe.transform.translation = Vec2::new(LEVEL_LENGTH / 2.0, GROUND_SURFACE_Y - GROUND_STRIPE_THICKNESS / 2.0);
    add_node(document, &mut inverses, ground_stripe);

    // The spike *prototype* — never shown itself (`visible: false`); every
    // actual spike the player sees is a clone of this one, spawned via
    // `spawnClone(ref)` and tracked in the script's own `spikes` array (a
    // real JS array of `{id, x, gen}` objects — the "better support for
    // arrays" ask, directly: no more encoding "how many spikes exist" into
    // a hidden node's transform field). `classes` carries over to every
    // clone automatically (`Artboard::spawn_clone_of`'s own doc comment).
    // Parented under `level_group` (so clones — which must be, to scroll
    // with the level — inherit that same parent), its own *local* x is
    // pushed absurdly far out (`1e5`) so `level_group`'s own scroll can
    // never drag its *world* position anywhere near the player's fixed one.
    // A dark outline on spike/wall/player — every real GD obstacle and icon
    // is outlined, not just a flat fill; it's what gives the "cartoon
    // sticker" look rather than plain vector shapes.
    let obstacle_stroke = || {
        Some(an1mbyt3_core::Stroke {
            width: 3.0,
            color: [0.05, 0.03, 0.08, 1.0],
            gradient: None,
            image: None,
            cap: an1mbyt3_core::StrokeCap::Round,
            join: an1mbyt3_core::StrokeJoin::Round,
        })
    };
    let mut spike_prototype = with_id(
        Node::new("Spike", Some(level_group_id), NodeKind::Shape(ShapeNode { path: spike_triangle_path(SPIKE_RADIUS), fill_color: [0.95, 0.2, 0.4, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: obstacle_stroke() })),
        "spikeProto",
    );
    spike_prototype.transform.translation = Vec2::new(100_000.0, SPIKE_Y);
    spike_prototype.classes = vec!["hazard".into()];
    spike_prototype.visible = false;
    add_node(document, &mut inverses, spike_prototype);

    let mut wall = with_id(
        Node::new("Wall", Some(level_group_id), NodeKind::Shape(ShapeNode { path: Path::rect(60.0, 300.0), fill_color: [0.55, 0.35, 0.95, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: obstacle_stroke() })),
        "wall",
    );
    wall.transform.translation = Vec2::new(WALL_X, GROUND_SURFACE_Y + 150.0);
    add_node(document, &mut inverses, wall);

    // The player — a layered "icon" instead of a flat square: a bright
    // body, an inset lighter panel (the two-tone look every real GD cube
    // icon has), and two eye dots for a simple face, all riding along as
    // children of the body `Shape` (not wrapped in a `Group` — collision
    // in this demo is now plain AABB math the script does itself against
    // each `ComponentRef`'s own known size, so there's no `Group`-local-
    // bounds concern the way Blocks' `TouchingClass` had, but keeping the
    // accents as plain children is still simplest: they just ride along).
    // Hidden on the title card along with `level_group`.
    let mut player = with_id(
        Node::new("Player", None, NodeKind::Shape(ShapeNode { path: Path::rect(PLAYER_SIZE, PLAYER_SIZE), fill_color: [0.15, 0.75, 0.95, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: obstacle_stroke() })),
        "player",
    );
    player.transform.translation = Vec2::new(PLAYER_X, PLAYER_GROUND_Y);
    player.visible = false;
    let player_id = add_node(document, &mut inverses, player);

    let panel = Node::new(
        "Player Panel",
        Some(player_id),
        NodeKind::Shape(ShapeNode { path: Path::rect(PLAYER_SIZE * 0.62, PLAYER_SIZE * 0.62), fill_color: [0.65, 0.95, 1.0, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None }),
    );
    add_node(document, &mut inverses, panel);

    for eye_x in [-PLAYER_SIZE * 0.16, PLAYER_SIZE * 0.16] {
        let mut eye = Node::new(
            "Player Eye",
            Some(player_id),
            NodeKind::Shape(ShapeNode { path: Path::ellipse(PLAYER_SIZE * 0.14, PLAYER_SIZE * 0.18), fill_color: [0.05, 0.08, 0.16, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None }),
        );
        eye.transform.translation = Vec2::new(eye_x, PLAYER_SIZE * 0.08);
        add_node(document, &mut inverses, eye);
    }

    // --- Title screen -------------------------------------------------------
    let title_screen = with_id(Node::new("Title Screen", None, NodeKind::Group(GroupNode::default())), "titleScreen");
    let title_screen_id = add_node(document, &mut inverses, title_screen);
    add_node(document, &mut inverses, centered_text_node(Some(title_screen_id), "GEOMETRY DASH", font_id, 56.0, [0.1, 0.85, 1.0, 1.0], 0.0, 160.0));
    add_node(document, &mut inverses, centered_text_node(Some(title_screen_id), "PRESS SPACE TO START", font_id, 22.0, [0.55, 0.62, 0.78, 1.0], 0.0, 100.0));

    // --- Win screen (hidden until the player reaches the wall) --------------
    let mut win_screen = with_id(Node::new("Win Screen", None, NodeKind::Group(GroupNode::default())), "winScreen");
    win_screen.visible = false;
    let win_screen_id = add_node(document, &mut inverses, win_screen);
    add_node(document, &mut inverses, centered_text_node(Some(win_screen_id), "LEVEL COMPLETE!", font_id, 52.0, [1.0, 0.85, 0.3, 1.0], 0.0, 160.0));
    add_node(document, &mut inverses, centered_text_node(Some(win_screen_id), "PRESS SPACE TO CONTINUE", font_id, 22.0, [0.55, 0.62, 0.78, 1.0], 0.0, 100.0));

    // --- HUD (hidden until play actually starts) -----------------------------
    let mut hud = with_id(Node::new("Hud", None, NodeKind::Group(GroupNode::default())), "hud");
    hud.visible = false;
    let hud_id = add_node(document, &mut inverses, hud);

    let mut bar_track = Node::new(
        "Bar Track",
        Some(hud_id),
        NodeKind::Shape(ShapeNode { path: Path::rect(BAR_WIDTH, BAR_HEIGHT), fill_color: [0.15, 0.16, 0.22, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None }),
    );
    bar_track.transform.translation = Vec2::new(0.0, BAR_Y);
    add_node(document, &mut inverses, bar_track);

    // `ScaleX`/`x` are both driven every tick by the *same* progress
    // fraction (see the script below) so the bar's left edge stays fixed at
    // `BAR_LEFT_X` while only its right edge grows — scaling a centered
    // rect in place alone would grow it from the *center* outward instead.
    let mut bar_fill = with_id(
        Node::new("Bar Fill", Some(hud_id), NodeKind::Shape(ShapeNode { path: Path::rect(BAR_WIDTH, BAR_HEIGHT - 6.0), fill_color: [0.3, 0.9, 1.0, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None })),
        "barFill",
    );
    bar_fill.transform.translation = Vec2::new(BAR_LEFT_X, BAR_Y);
    bar_fill.transform.scale = Vec2::new(0.0, 1.0);
    add_node(document, &mut inverses, bar_fill);

    // Real, live-updating text (`ctx.setTextContent`/`ComponentRef.textContent`)
    // — not a fixed slot's worth of pre-authored digit glyphs, so each
    // number's own width can change (e.g. "5" vs "100") without needing ten
    // nodes per digit. The static "%"/"ATTEMPT" siblings sit at a fixed
    // offset clear of the widest the adjacent live number ever gets, rather
    // than perfectly kerned against it — close enough for a HUD glance.
    let percent_text = with_id(centered_text_node(Some(hud_id), "0", font_id, HUD_TEXT_SIZE, [0.9, 0.9, 1.0, 1.0], -15.0, PERCENT_Y), "percentText");
    add_node(document, &mut inverses, percent_text);
    add_node(document, &mut inverses, centered_text_node(Some(hud_id), "%", font_id, HUD_TEXT_SIZE, [0.9, 0.9, 1.0, 1.0], 25.0, PERCENT_Y));

    add_node(document, &mut inverses, centered_text_node(Some(hud_id), "ATTEMPT", font_id, ATTEMPT_LABEL_SIZE, [0.55, 0.62, 0.78, 1.0], -35.0, ATTEMPT_Y));
    let attempt_text = with_id(centered_text_node(Some(hud_id), "1", font_id, ATTEMPT_LABEL_SIZE, [0.85, 0.9, 1.0, 1.0], 25.0, ATTEMPT_Y), "attemptText");
    add_node(document, &mut inverses, attempt_text);

    // Drawn last (top of the stack) — a full-screen flash, transparent at
    // rest, briefly opaque red on death, fading back out every tick after.
    let mut flash_overlay = with_id(
        Node::new("Death Flash", None, NodeKind::Shape(ShapeNode { path: Path::rect(3000.0, 1600.0), fill_color: [0.9, 0.15, 0.2, 0.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None })),
        "flash",
    );
    flash_overlay.transform.translation = Vec2::new(PLAYER_X, 0.0);
    add_node(document, &mut inverses, flash_overlay);

    // Same filler-animation convention every Code-program demo
    // uses — this state is pure Code, the animation/loop_mode fields are
    // unused filler (`StateDef::animation`'s own doc comment).
    let existing_animation = document.artboard(artboard_id).unwrap().animations().next().map(|(id, _)| id);
    let filler_animation = match existing_animation {
        Some(id) => id,
        None => {
            let animation_inverse = document.apply(Command::AddAnimation { artboard: artboard_id, animation: Animation::new("Geometry Dash filler") });
            let Command::SetAnimationDeleted { animation: new_animation, .. } = animation_inverse else {
                unreachable!("AddAnimation's inverse is always a SetAnimationDeleted")
            };
            inverses.push(animation_inverse);
            new_animation
        }
    };

    let def = StateMachineDef {
        name: "Geometry Dash".into(),
        inputs: Vec::new(),
        states: vec![StateDef {
            name: "Playing".into(),
            animation: filler_animation,
            loop_mode: LoopMode::Once,
            transitions: Vec::new(),
            position: Vec2::ZERO,
            blend: None,
            program: Some(ProgramDef::Code(GEOMETRY_DASH_SCRIPT.into())),
        }],
        initial_state: "Playing".into(),
        layers: Vec::new(),
        deleted: false,
    };
    let state_machine_inverse = document.apply(Command::AddStateMachine { artboard: artboard_id, state_machine: def });
    let Command::SetStateMachineDeleted { state_machine: state_machine_id, .. } = state_machine_inverse else {
        unreachable!("AddStateMachine's inverse is always a SetStateMachineDeleted")
    };
    inverses.push(state_machine_inverse);

    // A glow layer, appended (not replacing) whatever the artboard already
    // had — the "make this look awesome" ask. Artboard-wide, same as every
    // `PostEffect` (there's no per-node post-fx scoping in this engine), so
    // it affects any other content already on `artboard` too — an accepted
    // side effect of a demo whose whole point is a polished, glowy look.
    let mut effects = document.artboard(artboard_id).unwrap().post_effects().to_vec();
    effects.push(an1mbyt3_core::PostEffectLayer { effect: an1mbyt3_core::PostEffect::Glow { radius: 7.0, threshold: 0.55, intensity: 0.9 }, enabled: true });
    let post_effects_inverse = document.apply(Command::SetPostEffects { artboard: artboard_id, value: effects });
    inverses.push(post_effects_inverse);

    inverses.reverse();
    document.commit_batch(inverses);

    state_machine_id
}

/// The whole game, as real JavaScript — see `build_demo_geometry_dash`'s own
/// doc comment for why Code mode (not Blocks) and why `window.__gd` (not a
/// plain top-level `let`). Every numeric constant here is a hand-kept copy
/// of the identically-named Rust `const` right above (`build_demo_geometry_dash`'s
/// own layout math) — `format!`-interpolating them in would mean escaping
/// every one of this script's own `{`/`}` pairs (hundreds, for ordinary JS
/// blocks/object literals), a worse tradeoff than keeping two short,
/// clearly-labeled copies of a dozen numbers in sync by hand.
const GEOMETRY_DASH_SCRIPT: &str = r#"
const GRAVITY = -1.1
const JUMP_IMPULSE = 16
const JUMP_SPIN_SPEED = (2 * Math.PI) / (2 * JUMP_IMPULSE / -GRAVITY) // one clean flip per jump
const SCROLL_SPEED = 5
const PLAYER_X = -250
const PLAYER_GROUND_Y = -80
const PLAYER_HALF = 20
const SPIKE_RADIUS = 20
const SPIKE_HALF_W = SPIKE_RADIUS * 0.866
const SPIKE_Y = -90
const SPIKE_COUNT = 16
const SPAWN_LOOKAHEAD = 700
const DESPAWN_MARGIN = 700
const FINISH_X = 6200
const WALL_X = 6500
const WALL_HALF = 30
const FLY_VY = 22
const FLY_VX = 6
const SQUASH_X = 1.6
const SQUASH_Y = 0.45
const BAR_WIDTH = 300
const BAR_LEFT_X = -150

if (!window.__gd) {
  window.__gd = {
    scene: 'title',
    velocityY: 0,
    flyVx: 0,
    finished: false,
    landed: false,
    attempt: 1,
    generation: 0,
    spawnIndex: 0,
    nextSpikeX: 550,
    spikes: [], // array of {ref, x, gen} — real tracked live obstacles
    shakeTicks: 0,
    clock: 0,
    prevSpace: false,
  }
}
const s = window.__gd

const level = getCompByID('level')
const player = getCompByID('player')
const wall = getCompByID('wall')
const titleScreen = getCompByID('titleScreen')
const winScreen = getCompByID('winScreen')
const hud = getCompByID('hud')
const barFill = getCompByID('barFill')
const percentText = getCompByID('percentText')
const attemptText = getCompByID('attemptText')
const flash = getCompByID('flash')
const spikeProto = getCompByID('spikeProto')
const background = getCompByID('background')
if (!level || !player || !wall || !spikeProto) {
  // Still mid-load the very first frame or two after the demo document
  // swaps in — nothing to drive yet, just wait for the next frame.
} else {

const spaceDown = ctx.keysDown.has('space')
const spacePressed = spaceDown && !s.prevSpace
s.prevSpace = spaceDown

// --- procedural audio: a tiny persistent synth, not a downloaded asset ---
// (Code mode has full `window` access — `scriptRuntime.ts`'s own module
// doc — so the ordinary Web Audio API is all this needs; stashed on `s`
// same as everything else here so one `AudioContext` is reused, not
// recreated every frame.)
if (!s.audioCtx) {
  const AC = window.AudioContext || window.webkitAudioContext
  s.audioCtx = AC ? new AC() : null
}
function beep(freq, dur, type) {
  if (!s.audioCtx) return
  if (s.audioCtx.state === 'suspended') s.audioCtx.resume()
  const t0 = s.audioCtx.currentTime
  const osc = s.audioCtx.createOscillator()
  const gain = s.audioCtx.createGain()
  osc.type = type || 'square'
  osc.frequency.value = freq
  gain.gain.value = 0.09
  gain.gain.exponentialRampToValueAtTime(0.0001, t0 + dur)
  osc.connect(gain).connect(s.audioCtx.destination)
  osc.start(t0)
  osc.stop(t0 + dur)
}

// --- always-on ambience: background pulse, death-flash decay, camera shake
s.clock += 1
if (background) background.fillColor = [0.06, 0.07, 0.17 + Math.sin(s.clock * 0.015) * 0.05, 1]
if (flash && flash.fillColor && flash.fillColor[3] > 0) {
  flash.fillColor = [0.9, 0.15, 0.2, Math.max(0, flash.fillColor[3] - 0.04)]
}
if (s.shakeTicks > 0) {
  s.shakeTicks -= 1
  const mag = s.shakeTicks > 0 ? 6 : 0
  ctx.setCamera((Math.random() * 2 - 1) * mag, (Math.random() * 2 - 1) * mag, ctx.camera.zoom)
} else if (ctx.camera.panX !== 0 || ctx.camera.panY !== 0) {
  ctx.setCamera(0, 0, ctx.camera.zoom)
}

function resetRun(includeAttempts) {
  s.generation += 1
  level.x = 0
  player.y = PLAYER_GROUND_Y
  player.x = PLAYER_X
  player.rotation = 0
  player.scaleX = 1
  player.scaleY = 1
  s.velocityY = 0
  s.finished = false
  s.landed = false
  s.flyVx = 0
  s.spawnIndex = 0
  s.nextSpikeX = 550
  wall.fillColor = [0.55, 0.35, 0.95, 1]
  barFill.scaleX = 0
  barFill.x = BAR_LEFT_X
  percentText.textContent = '0'
  for (const spike of s.spikes) deleteNode(spike.ref)
  s.spikes = []
  if (includeAttempts) {
    s.attempt = 1
    attemptText.textContent = '1'
  }
}

if (s.scene === 'title') {
  if (spacePressed) {
    titleScreen.visible = false
    level.visible = true
    player.visible = true
    hud.visible = true
    s.scene = 'playing'
    resetRun(true)
    beep(660, 0.1)
  }
} else if (s.scene === 'playing') {
  if (!s.finished) {
    if (spacePressed && player.y === PLAYER_GROUND_Y) {
      s.velocityY = JUMP_IMPULSE
      beep(880, 0.07)
    }
    s.velocityY += GRAVITY
    player.y += s.velocityY
    if (player.y < PLAYER_GROUND_Y) {
      player.y = PLAYER_GROUND_Y
      s.velocityY = 0
      player.rotation = 0
    } else {
      player.rotation += JUMP_SPIN_SPEED
    }
    level.x -= SCROLL_SPEED

    // Spawn the next spike only once it's about to come within
    // `SPAWN_LOOKAHEAD` of the player's own fixed x ("off screen" ahead,
    // not already visible) — and only while fewer than `SPIKE_COUNT` have
    // spawned so far. Real tracked array, not a hidden node's own scalar.
    const nextWorldX = level.x + s.nextSpikeX
    if (s.spawnIndex < SPIKE_COUNT && nextWorldX < PLAYER_X + SPAWN_LOOKAHEAD) {
      const newSpike = spawnClone(spikeProto)
      if (newSpike) {
        newSpike.x = s.nextSpikeX
        newSpike.y = SPIKE_Y
        newSpike.visible = true
        s.spikes.push({ ref: newSpike, x: s.nextSpikeX, gen: s.generation })
        const gapCycle = (s.spawnIndex % 3) * 70
        s.nextSpikeX += 280 + gapCycle + (Math.random() * 40 - 20)
        s.spawnIndex += 1
      }
    }

    // Despawn every spike that's either genuinely scrolled off behind the
    // player, or is a leftover from a previous run (`gen` mismatch) — see
    // `resetRun`'s own generation bump. `Array.prototype.filter` directly;
    // no engine-side "delete an arbitrary node" restriction to work around
    // the way Blocks' `Myself`-only `DeleteThisClone` needed — Code mode's
    // `deleteNode` takes any live `ComponentRef`.
    s.spikes = s.spikes.filter((spike) => {
      const worldX = level.x + spike.x
      const stale = spike.gen !== s.generation
      if (worldX < PLAYER_X - DESPAWN_MARGIN || stale) {
        deleteNode(spike.ref)
        return false
      }
      return true
    })

    const progress = -level.x / FINISH_X
    barFill.scaleX = progress
    barFill.x = BAR_LEFT_X + BAR_WIDTH / 2 * progress
    percentText.textContent = String(Math.floor(progress * 100))

    for (const spike of s.spikes) {
      const worldX = level.x + spike.x
      if (Math.abs(player.x - worldX) < PLAYER_HALF + SPIKE_HALF_W && Math.abs(player.y - SPIKE_Y) < PLAYER_HALF + 10) {
        s.attempt += 1
        resetRun(false)
        attemptText.textContent = String(s.attempt)
        flash.fillColor = [0.9, 0.15, 0.2, 0.55]
        s.shakeTicks = 10
        beep(140, 0.3, 'sawtooth')
        break
      }
    }

    const finishWorldX = level.x + FINISH_X
    if (!s.finished && Math.abs(player.x - finishWorldX) < PLAYER_HALF + 5) {
      s.finished = true
      s.velocityY = FLY_VY
      s.flyVx = FLY_VX
      beep(523, 0.12)
    }
  } else if (!s.landed) {
    s.velocityY += GRAVITY
    player.y += s.velocityY
    player.x += s.flyVx
    player.rotation += JUMP_SPIN_SPEED * 2
    const wallWorldX = level.x + WALL_X
    if (Math.abs(player.x - wallWorldX) < WALL_HALF + PLAYER_HALF) {
      s.landed = true
      s.velocityY = 0
      s.flyVx = 0
      player.rotation = 0
      player.scaleX = SQUASH_X
      player.scaleY = SQUASH_Y
      wall.fillColor = [1, 0.9, 0.3, 1]
      s.scene = 'win'
      hud.visible = false
      winScreen.visible = true
      s.shakeTicks = 16
      ;[523, 659, 784, 1047].forEach((freq, i) => setTimeout(() => beep(freq, 0.15), i * 90))
    }
  }
} else if (s.scene === 'win') {
  if (spacePressed) {
    winScreen.visible = false
    level.visible = false
    player.visible = false
    titleScreen.visible = true
    s.scene = 'title'
    resetRun(false)
  }
}

}
"#;

/// A VRM avatar's ragdoll: one capsule per major humanoid bone (hips,
/// spine, chest, head, upper / lower arms and legs), from its joint to its
/// child's, posed where the avatar is now. Returns the parts and, per part,
/// (glTF node, humanoid name). `None` if the avatar lacks hips.
fn ragdoll_parts(model: &GpuModel, pose: &[glam::Mat4], world: glam::Mat4, cfg: &an1mbyt3_core::Ragdoll3D) -> Option<(Vec<an1mbyt3_physics::physics3d::RagdollPart>, Vec<(usize, String)>)> {
    let vrm = model.data.vrm.as_ref()?;
    let at = |name: &str| vrm.bone(name).filter(|i| *i < pose.len());
    let world_of = |i: usize| world * pose[i];
    let pos = |i: usize| world_of(i).transform_point3(glam::Vec3::ZERO);
    let (scale, _, _) = world.to_scale_rotation_translation();
    let s = scale.y.abs().max(0.01) * cfg.thickness.max(0.1);
    let flex = cfg.flexibility.clamp(0.05, 2.0);
    // (bone, the bone its capsule reaches to, parent limb, radius m,
    //  share of the mass, bend limit °)
    let chest = if at("upperChest").is_some() { "upperChest" } else { "chest" };
    let spec: [(&str, &[&str], Option<&str>, f32, f32, f32); 15] = [
        ("hips", &["spine"], None, 0.12, 0.16, 0.0),
        ("spine", &["chest", "upperChest", "neck"], Some("hips"), 0.11, 0.12, 25.0),
        ("chest", &["upperChest", "neck"], Some("spine"), 0.13, 0.14, 20.0),
        ("head", &[], Some(chest), 0.1, 0.08, 45.0),
        ("leftUpperArm", &["leftLowerArm"], Some(chest), 0.045, 0.03, 85.0),
        ("leftLowerArm", &["leftHand"], Some("leftUpperArm"), 0.04, 0.02, 80.0),
        ("rightUpperArm", &["rightLowerArm"], Some(chest), 0.045, 0.03, 85.0),
        ("rightLowerArm", &["rightHand"], Some("rightUpperArm"), 0.04, 0.02, 80.0),
        ("leftUpperLeg", &["leftLowerLeg"], Some("hips"), 0.07, 0.1, 70.0),
        ("leftLowerLeg", &["leftFoot"], Some("leftUpperLeg"), 0.055, 0.05, 75.0),
        ("rightUpperLeg", &["rightLowerLeg"], Some("hips"), 0.07, 0.1, 70.0),
        ("rightLowerLeg", &["rightFoot"], Some("rightUpperLeg"), 0.055, 0.05, 75.0),
        ("upperChest", &["neck"], Some("chest"), 0.13, 0.04, 15.0),
        ("leftFoot", &["leftToes"], Some("leftLowerLeg"), 0.04, 0.015, 30.0),
        ("rightFoot", &["rightToes"], Some("rightLowerLeg"), 0.04, 0.015, 30.0),
    ];
    at("hips")?;
    let mut parts = Vec::new();
    let mut bones: Vec<(usize, String)> = Vec::new();
    let mut index_of: HashMap<&str, usize> = HashMap::new();
    for (name, ends, parent, radius, share, limit) in spec {
        let Some(i) = at(name) else { continue };
        // upperChest only when present (it's a limb then, and chest's
        // capsule stops at it).
        let start = pos(i);
        let end = ends.iter().find_map(|e| at(e)).map(pos).unwrap_or_else(|| {
            // Head / feet: a short capsule along the bone's own direction.
            let parent_pos = model.parents.get(i).copied().flatten().map(pos).unwrap_or(start - glam::Vec3::Y * 0.1 * s);
            start + (start - parent_pos).normalize_or(glam::Vec3::Y) * if name == "head" { 0.2 * s } else { 0.12 * s }
        });
        let parent = parent.and_then(|p| index_of.get(p).copied().or_else(|| if p == "upperChest" { index_of.get("chest").copied() } else { None }));
        if name != "hips" && parent.is_none() {
            continue;
        }
        let (_, rotation, _) = world_of(i).to_scale_rotation_translation();
        parts.push(an1mbyt3_physics::physics3d::RagdollPart { parent, start, end, radius: radius * s, rotation, mass: cfg.mass.max(1.0) * share, limit_degrees: limit * flex });
        index_of.insert(name, parts.len() - 1);
        bones.push((i, name.to_string()));
    }
    Some((parts, bones))
}

/// A VRM's parsed MToon settings as an editable [`an1mbyt3_core::ToonMaterial`].
fn toon_from_vrm(t: &an1mbyt3_geometry::vrm::Toon, emission: [f32; 3], cutoff: Option<f32>) -> an1mbyt3_core::ToonMaterial {
    an1mbyt3_core::ToonMaterial {
        shade: t.shade,
        shift: t.shift,
        toony: t.toony,
        rim: t.rim,
        rim_power: t.rim_power,
        rim_lift: t.rim_lift,
        emission,
        outline_width: t.outline_width,
        outline: t.outline,
        alpha_cutoff: cutoff.unwrap_or(0.0),
        ..an1mbyt3_core::ToonMaterial::default()
    }
}

/// Field paths that duplicate a dedicated track type (2D position /
/// rotation / scale have their own Timeline rows).
fn is_dedicated_track(path: &str) -> bool {
    matches!(path, "transform/translation" | "transform/rotation" | "transform/scale")
}

/// Parses the Timeline panel's property-name strings (`"Translation"` /
/// `"Rotation"` / `"Scale"` / `"FillColor"` — the same strings
/// `dto::PropertyTrackSummary::property` sends out) back into a
/// `PropertyKind`.
fn parse_property_kind(property: &str) -> Result<PropertyKind, JsValue> {
    match property {
        "Translation" => Ok(PropertyKind::Translation),
        "Rotation" => Ok(PropertyKind::Rotation),
        "Scale" => Ok(PropertyKind::Scale),
        "FillColor" => Ok(PropertyKind::FillColor),
        "Path" => Ok(PropertyKind::Path),
        "MotionPath" => Ok(PropertyKind::MotionPath),
        other if other.starts_with("Field:") => Ok(an1mbyt3_core::field_path::field_kind(&other["Field:".len()..])),
        other => Err(JsValue::from_str(&format!("unknown property: {other}"))),
    }
}

/// Parses the State Machine panel's loop-mode strings back into a `LoopMode`
/// — mirrors `parse_property_kind` above.
fn parse_loop_mode(value: &str) -> Result<LoopMode, JsValue> {
    match value {
        "Once" => Ok(LoopMode::Once),
        "Loop" => Ok(LoopMode::Loop),
        "PingPong" => Ok(LoopMode::PingPong),
        other => Err(JsValue::from_str(&format!("unknown loop mode: {other}"))),
    }
}

/// Parses the State Machine panel's transition-direction strings back into a
/// `PlaybackDirection` — mirrors `parse_property_kind` above.
fn parse_playback_direction(value: &str) -> Result<PlaybackDirection, JsValue> {
    match value {
        "Forward" => Ok(PlaybackDirection::Forward),
        "Reverse" => Ok(PlaybackDirection::Reverse),
        other => Err(JsValue::from_str(&format!("unknown playback direction: {other}"))),
    }
}

/// Parses the State Machine panel's input-kind strings back into an
/// `InputKind` — mirrors `parse_property_kind` above.
fn parse_input_kind(value: &str) -> Result<InputKind, JsValue> {
    match value {
        "Bool" => Ok(InputKind::Bool),
        "Number" => Ok(InputKind::Number),
        "Enum" => Ok(InputKind::Enum),
        "Trigger" => Ok(InputKind::Trigger),
        other => Err(JsValue::from_str(&format!("unknown input kind: {other}"))),
    }
}

/// Builds the default path for one of the Shapes panel's quick drop-in
/// presets — the "Rectangle"/"Ellipse"/"Triangle"/"Star" buttons. Each
/// preset is sized to roughly the same visual footprint (~120-140px across)
/// so dropping in any one of them and immediately eyeballing it against
/// another feels consistent, the same reasoning `add_shape_node`'s single
/// hardcoded rectangle size always had.
fn preset_shape_path(kind: &str) -> Result<Path, JsValue> {
    match kind {
        "Rectangle" => Ok(Path::rect(120.0, 120.0)),
        "Ellipse" => Ok(Path::ellipse(120.0, 120.0)),
        "Triangle" => Ok(Path::polygon(3, 70.0)),
        "Star" => Ok(Path::star(5, 70.0, 28.0)),
        other => Err(JsValue::from_str(&format!("unknown shape preset: {other}"))),
    }
}

/// Inserts one `an1mbyt3_svg::ImportedNode` (and, for a `Group`, its whole
/// subtree) as real document nodes under `parent`, applying each `AddNode`
/// (and any `SetGroupMask`) one at a time via the raw `Command::apply` —
/// see `WasmEditor::import_svg`'s doc comment for why this can't be a single
/// upfront `Command::Batch`: a child's `Node::parent` needs its parent
/// `Group`'s freshly assigned `NodeId`, which only exists after that
/// `AddNode` has actually applied. Appends every inverse (in application
/// order — the caller reverses the whole accumulated list once, after every
/// top-level sibling is done, matching `Command::Batch`'s own "undo order is
/// reversed application order" convention) and every newly created id to
/// `inverses`/`new_nodes`. Returns the id assigned to `imported` itself, so
/// a `Group` arm can resolve its own `mask_index` against its children's
/// freshly assigned ids once they're all inserted.
/// Multiplies every imported shape's fill / stroke alpha by `opacity` — a
/// drawing layer's opacity, on placement (nodes have no opacity of their own).
fn fade_imported(nodes: &mut [an1mbyt3_svg::ImportedNode], opacity: f32) {
    if opacity >= 1.0 {
        return;
    }
    for node in nodes {
        match node {
            an1mbyt3_svg::ImportedNode::Shape { fill_color, stroke, .. } => {
                fill_color[3] *= opacity;
                if let Some(stroke) = stroke {
                    stroke.color[3] *= opacity;
                }
            }
            an1mbyt3_svg::ImportedNode::Group { children, .. } => fade_imported(children, opacity),
        }
    }
}

/// An SVG root's `viewBox` (min-x, min-y, width, height), comma- or
/// space-separated.
fn svg_view_box(svg: &str) -> Option<(f32, f32, f32, f32)> {
    let start = svg.find("viewBox=\"")? + "viewBox=\"".len();
    let end = start + svg[start..].find('"')?;
    let n: Vec<f32> = svg[start..end].split(|c: char| c == ',' || c.is_whitespace()).filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect();
    (n.len() == 4).then(|| (n[0], n[1], n[2], n[3]))
}

/// Refuses an SVG that wouldn't place as anything (the same importer
/// `place_drawing` uses) — see `add_drawing`.
fn check_drawing_svg(svg: &str) -> Result<(), JsValue> {
    let nodes = an1mbyt3_svg::import_svg(svg).map_err(|err| JsValue::from_str(&format!("drawing isn't a readable SVG: {err}")))?;
    if imported_bounds(&nodes).is_none() {
        return Err(JsValue::from_str("the drawing is empty — draw something first"));
    }
    Ok(())
}

/// The union of every imported shape's path bounds (`None` if there are no
/// shapes at all).
fn imported_bounds(nodes: &[an1mbyt3_svg::ImportedNode]) -> Option<(Vec2, Vec2)> {
    let mut out: Option<(Vec2, Vec2)> = None;
    for node in nodes {
        let bounds = match node {
            an1mbyt3_svg::ImportedNode::Shape { path, .. } if !path.commands.is_empty() => Some(path.bounds()),
            an1mbyt3_svg::ImportedNode::Shape { .. } => None,
            an1mbyt3_svg::ImportedNode::Group { children, .. } => imported_bounds(children),
        };
        if let Some((min, max)) = bounds {
            out = Some(match out {
                Some((a, b)) => (a.min(min), b.max(max)),
                None => (min, max),
            });
        }
    }
    out
}

fn insert_imported_node(
    document: &mut Document,
    artboard: ArtboardId,
    imported: an1mbyt3_svg::ImportedNode,
    parent: Option<NodeId>,
    inverses: &mut Vec<Command>,
    new_nodes: &mut Vec<NodeId>,
) -> NodeId {
    match imported {
        an1mbyt3_svg::ImportedNode::Shape { name, path, fill_color, gradient, stroke } => {
            // An SVG's own path coordinates are almost never centered on
            // local `(0, 0)` the way every preset shape (`Path::rect`/etc.)
            // already is — without this, the shape would still render in
            // exactly the right place (its `transform` starts at identity),
            // but rotating/scaling it would visibly pivot around whatever
            // corner the SVG happened to put at its own origin instead of
            // the shape's visual center. `with_anchor_preserving_world`
            // moves the pivot there while leaving the rendered geometry
            // exactly where it already was (see that method's own doc
            // comment for the math) — `an1mbyt3_core::Path::bounds()`'s
            // center is this crate's own standard "visual center" measure,
            // the same one `dto.rs::local_bounds` already uses for a
            // `Shape` node's selection box.
            let (min, max) = path.bounds();
            let transform = an1mbyt3_core::Transform2D::default().with_anchor_preserving_world((min + max) / 2.0);
            let mut node = Node::new(name, parent, NodeKind::Shape(ShapeNode { path, fill_color, blend_mode: Default::default(), gradient, image: None, stroke }));
            node.transform = transform;
            let inverse = Command::AddNode { artboard, node }.apply(document);
            let Command::SetDeleted { node: id, .. } = inverse else {
                unreachable!("Command::AddNode's inverse is always a SetDeleted")
            };
            inverses.push(inverse);
            new_nodes.push(id);
            id
        }
        an1mbyt3_svg::ImportedNode::Group { name, children, mask_index } => {
            let node = Node::new(name, parent, NodeKind::Group(Default::default()));
            let inverse = Command::AddNode { artboard, node }.apply(document);
            let Command::SetDeleted { node: group_id, .. } = inverse else {
                unreachable!("Command::AddNode's inverse is always a SetDeleted")
            };
            inverses.push(inverse);
            new_nodes.push(group_id);

            let child_ids: Vec<NodeId> = children
                .into_iter()
                .map(|child| insert_imported_node(document, artboard, child, Some(group_id), inverses, new_nodes))
                .collect();

            if let Some(mask_id) = mask_index.and_then(|index| child_ids.get(index).copied()) {
                let mask_inverse = Command::SetGroupMask { artboard, node: group_id, value: Some(mask_id) }.apply(document);
                inverses.push(mask_inverse);
            }

            group_id
        }
    }
}

/// The actual scene-tree walk `WasmEditor::render`'s masking needs —
/// recurses depth-first from `artboard.root_nodes()`, tracking `depth` (how
/// many ancestor masked `Group`s `node_id` sits under). A `Group` with a
/// `mask` set emits one [`an1mbyt3_renderer::SceneOp::WriteMask`] for that
/// mask child (at the *current* `depth`, before recursing into the rest of
/// its children at `depth + 1`) — every other node kind just contributes
/// its own [`node_draws`] at whatever `depth` it's already at. A node
/// currently serving as *some* group's mask (`Artboard::is_mask_target`)
/// never contributes its own regular content draw (it's only ever drawn
/// via the `WriteMask` op above, same "never becomes visible itself" rule
/// `Renderer::mask_write_pipeline` itself enforces) — it can still recurse
/// into its own children (an unusual shape, but not disallowed), which
/// inherit whatever `depth` this node itself was already at.
///
/// Also tracks `island` — a fresh id (from `next_island`, shared across
/// every root this is called for) minted the moment `depth` would go from
/// `0` to `1` (i.e. entering a brand-new, not-nested-under-anything-else
/// masked subtree), then inherited unchanged by everything nested inside it
/// (further nested masks only bump `depth`, never `island`). See
/// `an1mbyt3_renderer::SceneOp::Draw`'s own doc comment for why `depth`
/// alone can't tell two *sibling* masked subtrees apart — this is what lets
/// `Renderer::draw_masked_scene` give each one its own stencil-cleared pass.
fn build_scene_ops<'a>(
    editor: &'a WasmEditor,
    artboard: &'a an1mbyt3_core::Artboard,
    index: &an1mbyt3_core::SceneIndex,
    node_id: NodeId,
    depth: u32,
    island: u32,
    next_island: &mut u32,
    unmasked: &mut Vec<DrawCall<'a>>,
    masked: &mut Vec<an1mbyt3_renderer::SceneOp<'a>>,
    unlit_split: Option<&mut Vec<DrawCall<'a>>>,
) {
    let Some(node) = artboard.node(node_id) else { return };
    // 2D lighting on and this node is `unlit` (Unity's unlit sprite
    // material): its whole subtree draws *after* lighting instead, on top
    // of the lit image — the HUD-under-the-camera case. Masking doesn't
    // carry over into that overlay (unlit content draws unclipped).
    if let Some(unlit) = unlit_split {
        if node.unlit {
            if !node.deleted && node.visible && editor.scene_collect_unlit.get() {
                collect_unlit_subtree(editor, artboard, index, node_id, unlit);
            }
            return;
        }
        return build_scene_ops_lit(editor, artboard, index, node, node_id, depth, island, next_island, unmasked, masked, Some(unlit));
    }
    build_scene_ops_lit(editor, artboard, index, node, node_id, depth, island, next_island, unmasked, masked, None)
}

/// Every draw in `node_id`'s subtree (visible, not deleted, mask targets
/// skipped), in draw order — the unlit overlay's flat list.
fn collect_unlit_subtree<'a>(editor: &'a WasmEditor, artboard: &'a an1mbyt3_core::Artboard, index: &an1mbyt3_core::SceneIndex, node_id: NodeId, out: &mut Vec<DrawCall<'a>>) {
    let Some(node) = artboard.node(node_id) else { return };
    if node.deleted || !node.visible {
        return;
    }
    if !index.is_mask_target(node_id) {
        out.extend(apply_draw_effects(editor, artboard, node_id, node_draws(editor, artboard, node_id, node)));
    }
    for &child_id in index.children(Some(node_id)) {
        collect_unlit_subtree(editor, artboard, index, child_id, out);
    }
}

/// `build_scene_ops`' body for a node that isn't being diverted to the
/// unlit overlay.
#[allow(clippy::too_many_arguments)]
/// The world-space box an Image Effects layer is laid out against: the
/// node's local bounds (a group's covers its children) through its world
/// transform.
fn image_effects_bounds(artboard: &an1mbyt3_core::Artboard, node_id: NodeId) -> (Vec2, Vec2) {
    let (lo, hi) = crate::dto::local_bounds(artboard, node_id);
    let Some(world) = artboard.try_world_transform(node_id) else { return (lo, hi) };
    let m = world.to_affine2();
    let (mut min, mut max) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
    for c in [lo, Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y)] {
        let p = m.transform_point2(c);
        min = min.min(p);
        max = max.max(p);
    }
    if !min.is_finite() || !max.is_finite() || (max - min).min_element() <= 1e-4 {
        let p = m.transform_point2(Vec2::ZERO);
        return (p - Vec2::splat(50.0), p + Vec2::splat(50.0));
    }
    (min, max)
}

fn build_scene_ops_lit<'a>(
    editor: &'a WasmEditor,
    artboard: &'a an1mbyt3_core::Artboard,
    index: &an1mbyt3_core::SceneIndex,
    node: &'a Node,
    node_id: NodeId,
    depth: u32,
    island: u32,
    next_island: &mut u32,
    unmasked: &mut Vec<DrawCall<'a>>,
    masked: &mut Vec<an1mbyt3_renderer::SceneOp<'a>>,
    mut unlit_split: Option<&mut Vec<DrawCall<'a>>>,
) {
    if node.deleted {
        return;
    }
    // `Node::visible` (the "show"/"hide" Looks concept) — skips this node's
    // own draw call(s) *and* its whole child subtree, the same "hiding a
    // group hides everything visually inside it" convention Illustrator/
    // Figma/Photoshop's own visibility toggle already uses (for a leaf
    // node with no children this is equivalent to just skipping itself).
    // Editor-authoring readers (the Layers panel, `Artboard::shapes`/
    // `texts`/`images`, `pickable_nodes`) deliberately do *not* filter on
    // this — a hidden node must stay listed/selectable/editable, only its
    // rendered output disappears, so this check belongs here and nowhere
    // upstream of it.
    if !node.visible {
        return;
    }

    // Reference objects: design-time only. Hidden while playing (unless
    // asked) and in every export; faded as one layer (so overlapping parts
    // don't stack darker) at the reference's opacity.
    if let Some(reference) = node.reference() {
        if editor.exporting || (editor.effects_live() && !reference.show_when_playing) {
            return;
        }
        let k = reference.opacity.clamp(0.0, 1.0);
        if k <= 0.0 {
            return;
        }
        if k < 0.999 && editor.reference_building.get() != Some(node_id) {
            let (mut layer_unmasked, mut layer_masked) = (Vec::new(), Vec::new());
            let mut layer_islands = 0u32;
            let outer = editor.reference_building.replace(Some(node_id));
            build_scene_ops_lit(editor, artboard, index, node, node_id, 0, 0, &mut layer_islands, &mut layer_unmasked, &mut layer_masked, None);
            editor.reference_building.set(outer);
            if layer_unmasked.is_empty() && layer_masked.is_empty() {
                return;
            }
            let island = if depth == 0 {
                let fresh = *next_island;
                *next_island += 1;
                masked.push(an1mbyt3_renderer::SceneOp::Begin { island: fresh, after: unmasked.len() });
                fresh
            } else {
                island
            };
            masked.push(an1mbyt3_renderer::SceneOp::Layer { unmasked: layer_unmasked, masked: layer_masked, composites: vec![(depth, k)], island, fx: None });
            return;
        }
    }

    // Image Effects: the node and everything under it are drawn as one
    // flattened layer (a scene of its own, from depth 0), the effect stack
    // runs on that image, and the result is pasted here — at this depth, so
    // masks above still clip it.
    if let Some(effects) = node.image_effects() {
        if editor.fx_building.get() != Some(node_id) {
            let (mut layer_unmasked, mut layer_masked) = (Vec::new(), Vec::new());
            let mut layer_islands = 0u32;
            let outer = editor.fx_building.replace(Some(node_id));
            // (No unlit split: everything the effects should touch stays in
            // the layer.)
            build_scene_ops_lit(editor, artboard, index, node, node_id, 0, 0, &mut layer_islands, &mut layer_unmasked, &mut layer_masked, None);
            editor.fx_building.set(outer);
            if layer_unmasked.is_empty() && layer_masked.is_empty() {
                return;
            }
            let (min, max) = image_effects_bounds(artboard, node_id);
            let depth_image = effects.effects.iter().filter(|l| l.enabled).find_map(|l| l.effect.depth_image()).and_then(|id| editor.image_cache.get(&id));
            let background_image = effects.background_image.and_then(|id| editor.image_cache.get(&id));
            let enabled: Vec<&an1mbyt3_core::ImageEffect> = effects.effects.iter().filter(|l| l.enabled).map(|l| &l.effect).collect();
            let textures = enabled.iter().map(|e| match e {
                an1mbyt3_core::ImageEffect::Texture(t) => t.image.and_then(|id| editor.image_cache.get(&id)),
                _ => None,
            }).collect();
            let fx = an1mbyt3_renderer::LayerFx::new(enabled.into_iter().map(std::borrow::Cow::Borrowed).collect(), min, max, depth_image)
                .with_textures(textures)
                .with_background(effects.background, background_image, effects.background_image_opacity, effects.background_radius, effects.background_padding)
                .with_clip(effects.clip_to_shape);
            let island = if depth == 0 {
                let fresh = *next_island;
                *next_island += 1;
                masked.push(an1mbyt3_renderer::SceneOp::Begin { island: fresh, after: unmasked.len() });
                fresh
            } else {
                island
            };
            masked.push(an1mbyt3_renderer::SceneOp::Layer { unmasked: layer_unmasked, masked: layer_masked, composites: vec![(depth, 1.0)], island, fx: Some(Box::new(fx)) });
            return;
        }
    }

    let is_mask_target = index.is_mask_target(node_id);
    // Mask 2D: written before this node's own drawing, so
    // they clip it as well as everything under it.
    let (depth, island) = write_node_masks(editor, artboard, index, node_id, node, depth, island, next_island, unmasked, masked);
    // Sorting layers: a layer pass draws only its own layer's nodes (the
    // walk still visits everything, so masks / clips stay correct).
    let on_layer = editor.scene_layer_filter.get().is_none_or(|f| effective_layer(artboard, node_id) == f);
    let clip_opts = node.clip_options();
    let clip_fade = node.clip_fade();
    let hidden_clip = node.clip_children && clip_opts.hide_mask;
    // An "invisible" mask shape shown faintly (its own copy + binding: the
    // node's bindings also carry its stencil write this frame).
    if hidden_clip && clip_fade.mask_opacity > 0.0 && !is_mask_target && on_layer {
        let k = clip_fade.mask_opacity.clamp(0.0, 1.0);
        for draw in node_draws(editor, artboard, node_id, node) {
            let mut faint = DrawCall { binding: editor.effect_bindings.get(&editor.renderer), ..draw };
            faint.fill_color[3] *= k;
            if depth == 0 {
                unmasked.push(faint);
            } else {
                masked.push(an1mbyt3_renderer::SceneOp::Draw { draw: faint, depth, island });
            }
        }
    }
    if !is_mask_target && on_layer && !hidden_clip && !is_mask2d_shape(editor, artboard, node_id) && !hidden_by_goo_flipbook(editor, artboard, node_id) {
        for draw in apply_draw_effects(editor, artboard, node_id, node_draws(editor, artboard, node_id, node)) {
            if depth == 0 {
                unmasked.push(draw);
            } else {
                masked.push(an1mbyt3_renderer::SceneOp::Draw { draw, depth, island });
            }
        }
    }

    let group_mask = match &node.kind {
        NodeKind::Group(group) => group.mask,
        _ => None,
    };
    // A new island only ever gets minted right here/below (never inherited
    // as `0` forever) — `depth == 0` means nothing *else* already put
    // `node_id` under a mask, so this is the start of a brand-new,
    // independent masked subtree (see this function's own doc comment).
    let island = if group_mask.is_some() && depth == 0 {
        let fresh = *next_island;
        *next_island += 1;
        // The island draws here in the scene order, between the unmasked
        // draws before and after it.
        masked.push(an1mbyt3_renderer::SceneOp::Begin { island: fresh, after: unmasked.len() });
        fresh
    } else {
        island
    };
    // Inverted: fill everything first, then cut the mask back out — the
    // children draw at the "everything" depth, which the mask's area left.
    let invert = clip_opts.invert;
    let full_screen = |depth: u32, island: u32, masked: &mut Vec<an1mbyt3_renderer::SceneOp<'a>>| {
        if let Some(quad) = editor.mask_quad.as_ref() {
            let draw = DrawCall {
                mesh: quad,
                binding: editor.effect_bindings.get(&editor.renderer),
                transform: an1mbyt3_core::Transform2D { scale: Vec2::splat(1.0e6), ..Default::default() },
                fill_color: [1.0; 4],
                blend_mode: BlendMode::Normal,
                gradient: None,
                texture: None,
                material: None,
            };
            masked.push(an1mbyt3_renderer::SceneOp::WriteMask { mask: draw, depth, island });
        }
    };
    let mut depth_after_group_mask = depth;
    if let Some(mask_id) = group_mask {
        if invert {
            full_screen(depth, island, masked);
            depth_after_group_mask += 1;
        }
        if let Some(mask_node) = artboard.node(mask_id) {
            for draw in node_draws(editor, artboard, mask_id, mask_node) {
                masked.push(an1mbyt3_renderer::SceneOp::WriteMask { mask: draw, depth: depth_after_group_mask, island });
            }
        }
        if !invert {
            depth_after_group_mask += 1;
        }
    }

    // Same "mint a fresh island only at the true root of a masked subtree"
    // rule as `group_mask` above, checked against `depth_after_group_mask`
    // (not `depth`) so a node that *also* has a `group_mask` composes onto
    // the island that just started, instead of minting a second one.
    let island = if node.clip_children && !is_mask_target && depth_after_group_mask == 0 {
        let fresh = *next_island;
        *next_island += 1;
        masked.push(an1mbyt3_renderer::SceneOp::Begin { island: fresh, after: unmasked.len() });
        fresh
    } else {
        island
    };
    // `Node::clip_children` — this same node's own geometry *also* clips
    // whatever's nested inside it, no separate mask child needed (see that
    // field's own doc comment). Skipped for a node already serving as
    // *someone else's* `GroupNode::mask` (`is_mask_target`) — such a node
    // never contributes a regular draw of its own either, so it has no
    // "own geometry" here to clip with in the first place. Composes with
    // `group_mask` above: each mechanism a node actually uses contributes
    // its own stencil depth level.
    let child_depth = if node.clip_children && !is_mask_target {
        let mut d = depth_after_group_mask;
        // (A group mask already took the invert.)
        let invert_here = invert && group_mask.is_none();
        if invert_here {
            full_screen(d, island, masked);
            d += 1;
        }
        for draw in node_draws(editor, artboard, node_id, node) {
            masked.push(an1mbyt3_renderer::SceneOp::WriteMask { mask: draw, depth: d, island });
        }
        if invert_here { d } else { d + 1 }
    } else {
        depth_after_group_mask
    };

    // Clip fade: the clipped content is drawn *together* as one flattened
    // layer, then pasted at full strength inside the clip and at the fade's
    // opacity in the part the clip cuts away (the stencil there holds the
    // other level: one lower outside a plain clip, one higher inside an
    // inverted one). One fade for the whole image — not per particle.
    if node.clip_children && !is_mask_target && clip_fade.hidden_opacity > 0.0 && group_mask.is_none() {
        let fade_depth = if invert { child_depth + 1 } else { child_depth - 1 };
        let k = clip_fade.hidden_opacity.clamp(0.0, 1.0);
        let (mut layer_unmasked, mut layer_masked) = (Vec::new(), Vec::new());
        let mut layer_islands = 0u32;
        for &child_id in index.children(Some(node_id)) {
            build_scene_ops(editor, artboard, index, child_id, 0, 0, &mut layer_islands, &mut layer_unmasked, &mut layer_masked, unlit_split.as_deref_mut());
        }
        masked.push(an1mbyt3_renderer::SceneOp::Layer { unmasked: layer_unmasked, masked: layer_masked, composites: vec![(child_depth, 1.0), (fade_depth, k)], island, fx: None });
        return;
    }

    // `sorted_children`, not `children_of`: the authored order alone would
    // ignore Y-sort groups and `z_index` entirely — the actual on-screen
    // draw order has to be the same one `Artboard::ordered_node_ids` uses.
    for &child_id in index.children(Some(node_id)) {
        build_scene_ops(editor, artboard, index, child_id, child_depth, island, next_island, unmasked, masked, unlit_split.as_deref_mut());
    }
}

/// Linearly interpolates `stops` (sorted by `offset`, same convention
/// `GradientDef::stops` itself follows) at `t` — a particle's own Color
/// over Lifetime sample. `None` for an empty list (no curve authored —
/// `node_draws`'s own caller treats that as "no tint," not black). Ignores
/// each stop's own `midpoint` bias (a real gradient *fill*'s own concern,
/// not this per-particle point-sample) — a plain unbiased lerp between the
/// two stops straddling `t`, same "safe, simple approximation" precedent
/// this codebase already sets elsewhere rather than a second bias-aware
/// implementation just for this.
fn sample_color_over_lifetime(stops: &[an1mbyt3_core::GradientStop], t: f32) -> Option<[f32; 4]> {
    let (first, last) = (stops.first()?, stops.last()?);
    if t <= first.offset {
        return Some(first.color);
    }
    if t >= last.offset {
        return Some(last.color);
    }
    for pair in stops.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if t >= a.offset && t <= b.offset {
            let local_t = (t - a.offset) / (b.offset - a.offset).max(1e-6);
            let lerp = |x: f32, y: f32| x + (y - x) * local_t;
            return Some([lerp(a.color[0], b.color[0]), lerp(a.color[1], b.color[1]), lerp(a.color[2], b.color[2]), lerp(a.color[3], b.color[3])]);
        }
    }
    Some(last.color)
}

/// Same shape as `sample_color_over_lifetime`, for a particle's own Size
/// over Lifetime multiplier — `1.0` (unchanged) for an empty list.
fn sample_size_over_lifetime(keys: &[an1mbyt3_core::SizeKeyframe], t: f32) -> f32 {
    let (Some(first), Some(last)) = (keys.first(), keys.last()) else { return 1.0 };
    if t <= first.t {
        return first.size_multiplier;
    }
    if t >= last.t {
        return last.size_multiplier;
    }
    for pair in keys.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if t >= a.t && t <= b.t {
            let local_t = (t - a.t) / (b.t - a.t).max(1e-6);
            return a.size_multiplier + (b.size_multiplier - a.size_multiplier) * local_t;
        }
    }
    last.size_multiplier
}

/// Batches a whole `TilemapNode`'s non-empty cells into one mesh — one quad
/// per painted cell, each with its own per-vertex UV pointing at that tile's
/// sub-rect of the shared tileset texture (this engine's first real use of
/// per-vertex UV; see `an1mbyt3_renderer::shader.wgsl`'s `compute_textured_color`
/// for the other half). An empty grid (every cell `None`) produces an empty
/// `Mesh` — the caller (the per-frame cache-population loop below) already
/// treats that the same as a degenerate tessellated shape path: leave it
/// uncached, skip drawing, not a panic.
///
/// World space is centered on the tilemap node's own local origin (row 0 at
/// the *top*, matching `TilemapNode::tiles`' own row-major reading order,
/// and this engine's `+y`-up convention) — the same centering
/// `local_bounds`'s own `Tilemap` arm assumes for on-canvas selection.
fn build_tilemap_mesh(tilemap: &an1mbyt3_core::TilemapNode) -> Mesh {
    let columns = tilemap.tileset_columns().max(1);
    let total_width = tilemap.map_width as f32 * tilemap.cell_size;
    let total_height = tilemap.map_height as f32 * tilemap.cell_size;
    let tile_u = tilemap.tile_width_px as f32 / tilemap.tileset_width_px.max(1) as f32;
    let tile_v = tilemap.tile_height_px as f32 / tilemap.tileset_height_px.max(1) as f32;

    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    for y in 0..tilemap.map_height {
        for x in 0..tilemap.map_width {
            let Some(tile_index) = tilemap.get(x, y) else { continue };
            let left = x as f32 * tilemap.cell_size - total_width / 2.0;
            let right = left + tilemap.cell_size;
            let top = total_height / 2.0 - y as f32 * tilemap.cell_size;
            let bottom = top - tilemap.cell_size;

            let tile_col = (tile_index % columns) as f32;
            let tile_row = (tile_index / columns) as f32;
            let u0 = tile_col * tile_u;
            let u1 = u0 + tile_u;
            let v0 = tile_row * tile_v;
            let v1 = v0 + tile_v;

            let base = positions.len() as u32;
            positions.push([left, bottom]);
            positions.push([right, bottom]);
            positions.push([right, top]);
            positions.push([left, top]);
            uvs.push([u0, v1]);
            uvs.push([u1, v1]);
            uvs.push([u1, v0]);
            uvs.push([u0, v0]);
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    Mesh { positions, indices, uvs }
}

/// One node's own visual content, as zero or more [`DrawCall`]s — zero for
/// `Group`/`Bone` (no payload of their own) or a not-yet-cached/resolved
/// node (mesh still tessellating, image still baking, ...), exactly one for
/// `Shape`/`Instance`/`Image`, one per currently-alive particle for
/// `ParticleSystem`, or one *per glyph* for `Text` (a shaped run is however
/// many glyph meshes `an1mbyt3_text::shape_and_tessellate` produced, not one
/// draw for the whole string). `build_scene_ops` is the only caller — it
/// decides whether these end up in `unmasked` or `masked`, this function
/// only knows how to build them, not where they belong.
/// Everything a node draws: its kind's own geometry, then its drawing
/// components (Particle System, Trail Renderer) — Unity-style, any node can
/// carry those.
fn node_draws<'a>(editor: &'a WasmEditor, artboard: &'a an1mbyt3_core::Artboard, node_id: NodeId, node: &'a Node) -> Vec<DrawCall<'a>> {
    // Goo 2D: a group's liquid is drawn at its front-most member, under
    // that member's own drawing (which "Hide source" leaves out entirely).
    // (While designing there's no liquid: members draw as themselves.)
    // Anti Goo with Hide source: not drawn while its goo runs.
    if node.anti_goo().is_some_and(|a| a.hide_source && (editor.effects_live() || editor.goo_preview.contains(&a.group))) {
        return Vec::new();
    }
    if let Some(goo) = node.goo().filter(|g| editor.effects_live() || editor.goo_preview.contains(&g.group)) {
        let mut draws = editor.goo_draw_at.get(&node_id).map(|owner| goo_draws(editor, artboard, *owner)).unwrap_or_default();
        if !goo.hide_source {
            draws.extend(node_draws_plain(editor, artboard, node_id, node));
        }
        return draws;
    }
    node_draws_plain(editor, artboard, node_id, node)
}

/// [`node_draws`] without Goo 2D.
fn node_draws_plain<'a>(editor: &'a WasmEditor, artboard: &'a an1mbyt3_core::Artboard, node_id: NodeId, node: &'a Node) -> Vec<DrawCall<'a>> {
    let mut draws = kind_draws(editor, artboard, node_id, node);
    // A Particle Fluid 2D draws the node's particles (or liquid) as one
    // metaball body instead of individual discs.
    let fluid = node.particle_fluid().is_some();
    if fluid {
        draws.extend(fluid_draws(editor, node_id));
    }
    if let (Some(config), false) = (node.particle_system(), fluid) {
        draws.extend(particle_draws(editor, artboard, node_id, config));
    }
    if node.trail().is_some() {
        draws.extend(trail_draws(editor, node_id));
    }
    if node.cloth().is_some() {
        draws.extend(cloth_draws(editor, node_id));
    }
    if node.tree().is_some() {
        draws.extend(tree_draws(editor, artboard, node_id));
    }
    if let (Some(config), false) = (node.liquid(), fluid) {
        draws.extend(liquid_draws(editor, artboard, node_id, config));
    }
    draws
}

/// Where a Liquid 2D node's particles are: the running fluid in Playground,
/// otherwise its starting volume (so it's visible and placeable while
/// designing). Each with its relative density, for foam shading.
fn liquid_points(editor: &WasmEditor, artboard: &an1mbyt3_core::Artboard, node_id: NodeId, config: &an1mbyt3_core::Liquid2D) -> Vec<(Vec2, f32)> {
    if let Some((_, world)) = editor.physics.as_ref() {
        if let Some(sim) = world.liquid(node_id) {
            return sim.particles.iter().map(|p| (p.position, p.density)).collect();
        }
    }
    an1mbyt3_physics::liquid::initial_layout(config, artboard.world_transform(node_id).translation).into_iter().map(|p| (p, 1.0)).collect()
}

/// A Liquid 2D's particles as overlapping discs about twice their spacing
/// across, so neighbours merge into one body of liquid; thin, splashing or
/// surface spots (low density) blend toward white like foam.
fn liquid_draws<'a>(editor: &'a WasmEditor, artboard: &'a an1mbyt3_core::Artboard, node_id: NodeId, config: &'a an1mbyt3_core::Liquid2D) -> Vec<DrawCall<'a>> {
    let (Some(mesh), Some(bindings)) = (editor.particle_mesh.as_ref(), editor.liquid_bindings.get(&node_id)) else { return Vec::new() };
    let size = config.particle_spacing.max(2.0) * 2.1;
    liquid_points(editor, artboard, node_id, config)
        .into_iter()
        .zip(bindings.iter())
        .map(|((position, density), binding)| {
            let foam = (1.0 - density).clamp(0.0, 1.0) * 0.55;
            let c = config.color;
            DrawCall {
                mesh,
                binding,
                transform: an1mbyt3_core::Transform2D { translation: position, rotation: 0.0, scale: Vec2::splat(size), anchor: Vec2::ZERO },
                fill_color: [c[0] + (1.0 - c[0]) * foam, c[1] + (1.0 - c[1]) * foam, c[2] + (1.0 - c[2]) * foam, c[3]],
                blend_mode: BlendMode::Normal,
                gradient: None,
                texture: None,
                material: None,
            }
        })
        .collect()
}

/// `p` from an emitter's local space to world (the same component-wise
/// composition `particle_draws` uses).
fn emitter_to_world(emitter: an1mbyt3_core::Transform2D, p: Vec2) -> Vec2 {
    let (sin, cos) = emitter.rotation.sin_cos();
    let s = p * emitter.scale;
    emitter.translation + Vec2::new(s.x * cos - s.y * sin, s.x * sin + s.y * cos)
}

/// The inverse of [`emitter_to_world`].
fn world_to_emitter(emitter: an1mbyt3_core::Transform2D, p: Vec2) -> Vec2 {
    let (sin, cos) = (-emitter.rotation).sin_cos();
    let d = p - emitter.translation;
    let r = Vec2::new(d.x * cos - d.y * sin, d.x * sin + d.y * cos);
    r / emitter.scale.max(Vec2::splat(1e-6))
}

/// Particle Behaviours 2D, applied after the simulation's own step: each
/// behaviour's strength is its curve at the particle's age. Converge pulls
/// positions toward the point (at 1 they sit on it); Attract / Repel push
/// velocity (px/s²); Drag slows it. `local` = the particles live in the
/// emitter's space (targets are converted into it).
fn apply_particle_behaviors_2d(fx: &an1mbyt3_core::ParticleBehaviors2D, particles: &mut [an1mbyt3_runtime::Particle], local: bool, emitter: an1mbyt3_core::Transform2D, dt: f32) {
    use an1mbyt3_core::{ParticleBehaviorKind3D as K, PointSpace};
    if dt <= 0.0 {
        return;
    }
    for b in fx.behaviors.iter().filter(|b| b.enabled) {
        let point = Vec2::new(b.point[0], b.point[1]);
        let world = match b.space {
            PointSpace::Local => emitter_to_world(emitter, point),
            PointSpace::World => point,
        };
        let target = if local { world_to_emitter(emitter, world) } else { world };
        for p in particles.iter_mut() {
            let w = b.weight(p.normalized_age());
            match b.kind {
                K::Converge => {
                    let k = w.clamp(0.0, 1.0);
                    if k >= 0.999 {
                        p.position = target;
                        p.velocity = Vec2::ZERO;
                    } else if k > 0.0 {
                        // Frame-rate independent: k is "how gathered".
                        p.position = p.position.lerp(target, 1.0 - (1.0 - k).powf(dt * 8.0));
                    }
                }
                K::Attract | K::Repel => {
                    let sign = if b.kind == K::Attract { 1.0 } else { -1.0 };
                    let to = target - p.position;
                    p.velocity += to / (to.length() + 8.0) * w * sign * dt;
                }
                K::Drag => p.velocity *= (-w.max(0.0) * dt).exp(),
            }
        }
    }
}

/// The metaball blobs for a node with a Particle Fluid 2D: its Particle
/// System's live particles (world space, sized as drawn, weighted by their
/// colour's alpha) and/or its Liquid 2D's particles.
/// The grid cell for a node's Particle Fluid 2D, from its *settings* (the
/// typical drop's reach × detail) — never from the live particles, whose
/// sizes change every frame and would make the outline shimmer.
fn fluid_cell(node: &Node, fluid: &an1mbyt3_core::ParticleFluid2D) -> f32 {
    let reach = fluid.stickiness.max(0.5);
    let radius = if let Some(config) = node.particle_system() {
        (config.main.start_size.min + config.main.start_size.max) * 0.25 * reach
    } else if let Some(config) = node.liquid() {
        config.particle_spacing.max(2.0) * 0.75 * reach
    } else {
        10.0
    };
    (radius.max(1.0) * fluid.detail.clamp(0.08, 0.6)).max(0.5)
}

fn fluid_blobs(editor: &WasmEditor, artboard: &an1mbyt3_core::Artboard, node_id: NodeId, node: &Node, fluid: &an1mbyt3_core::ParticleFluid2D) -> Vec<crate::metaball2d::Blob> {
    let reach = fluid.stickiness.max(0.5);
    let mut blobs = Vec::new();
    if let (Some(config), Some(runtime)) = (node.particle_system(), editor.particle_runtimes.get(&node_id)) {
        let emitter = artboard.world_transform(node_id);
        let local = config.main.simulation_space == an1mbyt3_core::SimulationSpace::Local;
        let scale = if local { (emitter.scale.x.abs() + emitter.scale.y.abs()) * 0.5 } else { 1.0 };
        for p in runtime.particles() {
            let age = p.normalized_age();
            let size = p.base_size * sample_size_over_lifetime(&config.size_over_lifetime, age) * scale;
            let alpha = config.main.start_color[3] * sample_color_over_lifetime(&config.color_over_lifetime, age).map_or(1.0, |c| c[3]);
            let center = if local { emitter_to_world(emitter, p.position) } else { p.position };
            // (The particle disc is `size` across.)
            blobs.push(crate::metaball2d::Blob { center, radius: size * 0.5 * reach, weight: alpha });
        }
    }
    if let Some(config) = node.liquid() {
        let radius = config.particle_spacing.max(2.0) * 0.75 * reach;
        for (center, _) in liquid_points(editor, artboard, node_id, config) {
            blobs.push(crate::metaball2d::Blob { center, radius, weight: 1.0 });
        }
    }
    blobs
}

/// A liquid on the GPU (see `goo_gpu`): its field texture, the quad it's
/// drawn on, and which compiled shader draws it.
pub(crate) struct LiquidGpu {
    texture: Option<an1mbyt3_renderer::DataTexture>,
    mesh: Option<GpuMesh>,
    shader: String,
    /// Fingerprint of the look `shader` was generated from.
    look_key: u64,
    binding: DrawBinding,
    /// The glow's additive material key (`liquid_materials`) and its own
    /// binding (one binding per draw).
    glow_shader: Option<String>,
    glow_binding: DrawBinding,
}

impl LiquidGpu {
    fn new(renderer: &Renderer) -> Self {
        Self { texture: None, mesh: None, shader: String::new(), glow_shader: None, look_key: 0, binding: renderer.create_draw_binding(), glow_binding: renderer.create_draw_binding() }
    }

    /// Uploads `field` (+ `second` on its grid), rebuilds the quad in the
    /// draw's space (`to_draw`: world → it), and makes sure `look`'s shader
    /// is compiled (once per distinct look).
    fn update(&mut self, renderer: &Renderer, materials: &mut HashMap<String, an1mbyt3_renderer::GpuMaterial>, field: &crate::metaball2d::Field, second: Option<&[f32]>, look: &crate::goo_gpu::Look, to_draw: glam::Affine2) {
        let (w, h, bytes) = crate::goo_gpu::pack(field, second);
        renderer.write_data_texture(&mut self.texture, w, h, &bytes);
        let capacity = self.texture.as_ref().map_or((w, h), |t| t.size());
        let quad = crate::goo_gpu::quad(field, capacity, to_draw);
        match self.mesh.as_mut() {
            Some(mesh) => renderer.rewrite_mesh(mesh, &quad),
            None => self.mesh = Some(renderer.upload_mesh_with_headroom(&quad, 1.0)),
        }
        // The shader only changes with the look (settings), not per frame.
        let look_key = fingerprint_debug(look) | 1;
        if look_key == self.look_key {
            return;
        }
        self.look_key = look_key;
        // The glow: its own additive material (keyed apart from normal ones).
        self.glow_shader = crate::goo_gpu::glow_shader(look).map(|glow| {
            let key = format!("additive\n{glow}");
            if !materials.contains_key(&key) {
                if let Ok(m) = renderer.compile_material_blended(&glow, true) {
                    materials.insert(key.clone(), m);
                }
            }
            key
        });
        let wgsl = crate::goo_gpu::shader(look);
        if !materials.contains_key(&wgsl) {
            match renderer.compile_material(&wgsl) {
                Ok(m) => {
                    materials.insert(wgsl.clone(), m);
                }
                // Every look's WGSL is validated by tests (`goo_gpu`); a
                // failure here just leaves the liquid undrawn.
                Err(_) => {}
            }
        }
        self.shader = wgsl;
    }

    /// Its draws (`transform`: the draw's space → world): the liquid, then
    /// its glow (additive, on top) if it has one.
    fn draw<'a>(&'a self, materials: &'a HashMap<String, an1mbyt3_renderer::GpuMaterial>, transform: an1mbyt3_core::Transform2D) -> Vec<DrawCall<'a>> {
        let (Some(mesh), Some(texture)) = (self.mesh.as_ref(), self.texture.as_ref()) else { return Vec::new() };
        let call = |binding: &'a DrawBinding, material: &'a an1mbyt3_renderer::GpuMaterial| DrawCall {
            mesh,
            binding,
            transform,
            fill_color: [1.0; 4],
            blend_mode: BlendMode::Normal,
            gradient: None,
            texture: Some(&texture.image),
            material: Some(material),
        };
        let mut out = Vec::new();
        if let Some(m) = materials.get(&self.shader) {
            out.push(call(&self.binding, m));
        }
        if let Some(m) = self.glow_shader.as_ref().and_then(|k| materials.get(k)) {
            out.push(call(&self.glow_binding, m));
        }
        out
    }
}

/// A Particle Fluid 2D's look: the rim (whole outline, edge colour), the
/// body (a slightly higher level, so the rim shows as a band round it),
/// and a glossy highlight from the same drops at about half size nudged
/// up-left (packed as the texture's second field) — a shine that merges
/// and stretches along with the drops.
fn fluid_look(fluid: &an1mbyt3_core::ParticleFluid2D, antialias: Option<f32>) -> crate::goo_gpu::Look {
    use crate::goo_gpu::{Highlight, Look, Paint};
    let th = fluid.threshold.clamp(0.05, 0.95);
    let has_rim = fluid.edge_color[3] > 0.001 && fluid.edge_width > 0.0;
    let body_th = if has_rim { (th * (1.0 + fluid.edge_width.clamp(0.0, 1.0) * 1.5)).min(0.97) } else { th };
    let c = fluid.color;
    let k = 0.6;
    Look {
        rim: has_rim.then(|| (th, Paint::Solid(fluid.edge_color))),
        body: (body_th, Paint::Solid(fluid.color)),
        highlight: (fluid.gloss > 0.0).then(|| Highlight {
            level: th,
            color: [c[0] + (1.0 - c[0]) * k, c[1] + (1.0 - c[1]) * k, c[2] + (1.0 - c[2]) * k, c[3] * fluid.gloss.clamp(0.0, 1.0) * 0.8],
            offset_texels: Vec2::ZERO,
            second_field: true,
            soft_from: None,
        }),
        antialias,
        material: None,
        glow: None,
    }
}

/// A Goo 2D member's sources, in world space: a shape's path, a text's
/// glyph outlines, a sprite's rectangle, a particle system's particles (as
/// drawn) and a Liquid 2D's particles.
fn goo_sources(editor: &WasmEditor, artboard: &an1mbyt3_core::Artboard, node_id: NodeId, node: &Node) -> Vec<crate::metaball2d::Source> {
    use crate::metaball2d::{flatten_path, Source};
    let world = artboard.world_transform(node_id).to_affine2();
    let to = |p: Vec2| world.transform_point2(p);
    let mut out = Vec::new();
    match &node.kind {
        NodeKind::Shape(shape) => {
            let loops = flatten_path(&shape.path, to);
            if !loops.is_empty() {
                out.push(Source::Polygons { loops, weight: shape.fill_color[3].max(0.01) });
            }
        }
        NodeKind::Text(text) => {
            if let Some(font) = editor.font_cache.get(&text.font) {
                // Glyph outlines in the node's space, shaped once per
                // (content, size, font) and cached; laid out exactly as
                // the text is drawn (centred on its advance).
                let mut cache = editor.goo_text_cache.borrow_mut();
                let fresh = cache.get(&node_id).is_none_or(|(c, s, f, _)| *c != text.content || *s != text.size_px.to_bits() || *f != text.font);
                if fresh {
                    let glyphs = an1mbyt3_text::shape_text(font, &text.content, text.size_px);
                    let advance: f32 = glyphs.iter().map(|g| g.advance.x).sum();
                    let mut pen = Vec2::new(-advance / 2.0, 0.0);
                    let mut local = Vec::new();
                    for g in &glyphs {
                        let offset = pen + g.offset;
                        local.extend(flatten_path(&an1mbyt3_text::glyph_outline(font, g.glyph_id, text.size_px), |p| p + offset));
                        pen += g.advance;
                    }
                    cache.insert(node_id, (text.content.clone(), text.size_px.to_bits(), text.font, local));
                }
                let loops: Vec<Vec<Vec2>> = cache[&node_id].3.iter().map(|ring| ring.iter().map(|p| to(*p)).collect()).collect();
                if !loops.is_empty() {
                    out.push(Source::Polygons { loops, weight: 1.0 });
                }
            }
        }
        NodeKind::Image(image) => {
            let (hw, hh) = (image.width / 2.0, image.height / 2.0);
            out.push(Source::Polygons { loops: vec![vec![to(Vec2::new(-hw, -hh)), to(Vec2::new(hw, -hh)), to(Vec2::new(hw, hh)), to(Vec2::new(-hw, hh))]], weight: 1.0 });
        }
        _ => {}
    }
    // A Flipbook member: the frame it's showing (and everything in it)
    // shapes the liquid — the goo animates with the flipbook.
    if node.flipbook().is_some() && node.particle_system().is_none() {
        fn shown_sources(editor: &WasmEditor, artboard: &an1mbyt3_core::Artboard, id: NodeId, out: &mut Vec<crate::metaball2d::Source>) {
            let Some(n) = artboard.node(id) else { return };
            if n.deleted {
                return;
            }
            // (Without its own Goo 2D: a frame is part of its parent's goo.)
            let plain = Node { behaviors: Vec::new(), ..n.clone() };
            out.extend(goo_sources(editor, artboard, id, &plain));
            for (child, _) in artboard.children_of(id) {
                shown_sources(editor, artboard, child, out);
            }
        }
        if let Some((frame, _)) = artboard.children_of(node_id).find(|(_, c)| !c.deleted && c.visible) {
            shown_sources(editor, artboard, frame, &mut out);
        }
    }
    if let (Some(config), Some(runtime)) = (node.particle_system(), editor.particle_runtimes.get(&node_id)) {
        let emitter = artboard.world_transform(node_id);
        let local = config.main.simulation_space == an1mbyt3_core::SimulationSpace::Local;
        let scale = if local { (emitter.scale.x.abs() + emitter.scale.y.abs()) * 0.5 } else { 1.0 };
        for p in runtime.particles() {
            let age = p.normalized_age();
            let size = p.base_size * sample_size_over_lifetime(&config.size_over_lifetime, age) * scale;
            let alpha = config.main.start_color[3] * sample_color_over_lifetime(&config.color_over_lifetime, age).map_or(1.0, |c| c[3]);
            let center = if local { emitter_to_world(emitter, p.position) } else { p.position };
            out.push(Source::Circle { center, radius: size * 0.5, weight: alpha });
        }
    }
    if let Some(config) = node.liquid() {
        let radius = config.particle_spacing.max(2.0) * 0.6;
        for (center, _) in liquid_points(editor, artboard, node_id, config) {
            out.push(Source::Circle { center, radius, weight: 1.0 });
        }
    }
    out
}

/// A settings struct's fingerprint (its `Debug` text, hashed) — part of a
/// liquid's rebuild key, so editing a colour or reach rebuilds it.
fn fingerprint_debug<T: std::fmt::Debug>(value: &T) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{value:?}").hash(&mut h);
    h.finish()
}

/// A Goo 2D group's look: the rim (whole outline, edge colour/gradient),
/// the body (`edge_width` inside it, fill colour/gradient), and a soft
/// highlight (deeper inside, nudged up and left, the fill lightened).
fn goo_look(goo: &an1mbyt3_core::Goo2D, gloss: Option<&an1mbyt3_core::GooGloss2D>, field: &crate::metaball2d::Field, antialias: Option<f32>) -> crate::goo_gpu::Look {
    use crate::goo_gpu::{Highlight, Look, Paint};
    let paint = |color: [f32; 4], gradient: &Option<an1mbyt3_core::GradientDef>| gradient.clone().map_or(Paint::Solid(color), Paint::Gradient);
    let reach = goo.reach.max(1.0);
    let has_rim = goo.edge_width > 0.0 && (goo.edge_color[3] > 0.001 || goo.edge_gradient.is_some());
    let body_inset = if has_rim { goo.edge_width } else { 0.0 };
    let (_, _, _, cell) = field.layout();
    let base = goo.gradient.as_ref().and_then(|g| g.stops.first().map(|s| s.color)).unwrap_or(goo.color);
    let k = gloss.map_or(0.6, |g| g.blend.clamp(0.0, 1.0));
    let tint = gloss.map_or([1.0; 4], |g| g.color);
    let opacity = gloss.map_or(0.4, |g| g.opacity.clamp(0.0, 1.0));
    let color = [base[0] + (tint[0] - base[0]) * k, base[1] + (tint[1] - base[1]) * k, base[2] + (tint[2] - base[2]) * k, goo.gloss.clamp(0.0, 1.0) * opacity * tint[3]];
    let highlight = match gloss {
        // Built in: just inside the edge (the field saturates deeper in).
        None => (goo.gloss > 0.0).then(|| Highlight {
            level: field.level_for_inset(body_inset + reach * 0.62),
            color,
            offset_texels: Vec2::new(1.0, -1.0) * reach * 0.12 / cell,
            second_field: false,
            soft_from: None,
        }),
        // Goo Gloss: on the liquid eroded by `depth` px (the second field,
        // see `metaball2d::erode`, 1 deep inside), nudged toward `angle`
        // (the field is read the opposite way), soft over `softness`.
        Some(g) => (goo.gloss > 0.0 && g.opacity > 0.0).then(|| {
            let (sin, cos) = g.angle.to_radians().sin_cos();
            let (_, start, full) = gloss_levels(g);
            Highlight {
                level: full,
                color,
                offset_texels: -Vec2::new(cos, sin) * g.distance.max(0.0) / cell,
                second_field: true,
                soft_from: (full > start).then_some(start),
            }
        }),
    };
    Look {
        rim: has_rim.then(|| (field.level_for_inset(0.0), paint(goo.edge_color, &goo.edge_gradient))),
        body: (field.level_for_inset(body_inset), paint(goo.color, &goo.gradient)),
        // Deep inside, so only thick parts (blobs, merged bulges) catch a
        // glint; shown nudged up-left (so the field is read down-right).
        highlight,
        antialias,
        material: None,
        glow: None,
    }
}

/// A Goo Gloss's erosion depth (world px) and the eroded-liquid values its
/// shine starts at and is fully opaque at. The erosion (`metaball2d::erode`)
/// is a tent blur reaching 1 at depth `d_max`; at distance `x` inside the
/// edge it reads `1 - (1 - x/d_max)² / 2`. The shine starts at 0.8 × Depth
/// in and, with Softness, fades in further inward (up to 2 × Depth) —
/// never toward the edge.
fn gloss_levels(g: &an1mbyt3_core::GooGloss2D) -> (f32, f32, f32) {
    let depth = g.depth.max(1.0);
    let soft = g.softness.clamp(0.0, 1.0);
    let d_max = depth * (1.0 + 1.5 * soft);
    let at = |x: f32| 1.0 - (1.0 - (x / d_max).min(1.0)).powi(2) / 2.0;
    let start = at(0.8 * depth);
    let full = if soft > 0.0 { at(0.8 * d_max).min(0.995) } else { start };
    (d_max, start, full)
}

/// A Goo 2D group's liquid, drawn with its owner's world transform.
fn goo_draws<'a>(editor: &'a WasmEditor, artboard: &'a an1mbyt3_core::Artboard, node_id: NodeId) -> Vec<DrawCall<'a>> {
    editor.goo_gpu.get(&node_id).map(|g| g.draw(&editor.liquid_materials, artboard.world_transform(node_id))).unwrap_or_default()
}

/// A node's Particle Fluid 2D liquid (its quad is in world space).
fn fluid_draws(editor: &WasmEditor, node_id: NodeId) -> Vec<DrawCall<'_>> {
    editor.particle_fluid_gpu.get(&node_id).map(|g| g.draw(&editor.liquid_materials, an1mbyt3_core::Transform2D::default())).unwrap_or_default()
}

/// A Particle System component's live particles.
fn particle_draws<'a>(editor: &'a WasmEditor, artboard: &'a an1mbyt3_core::Artboard, node_id: NodeId, config: &'a an1mbyt3_core::ParticleSystemNode) -> Vec<DrawCall<'a>> {
    let (Some(disc), Some(runtime), Some(bindings)) =
        (editor.particle_mesh.as_ref(), editor.particle_runtimes.get(&node_id), editor.particle_bindings.get(&node_id))
    else {
        return Vec::new();
    };
    // Particle Look: shape, blend, alignment, image and glow.
    let look = artboard.node(node_id).and_then(|n| n.particle_look());
    let mesh = editor.particle_shape_keys.get(&node_id).and_then(|k| editor.particle_shape_meshes.get(k)).unwrap_or(disc);
    let image = look.filter(|l| l.shape == an1mbyt3_core::ParticleShape2D::Image).and_then(|l| l.image).and_then(|id| editor.image_cache.get(&id));
    let (mesh, aspect) = match (image, editor.image_quad_mesh.as_ref()) {
        (Some(_), Some(quad)) => {
            // The image's aspect, from its PNG header (square otherwise).
            let aspect = look.and_then(|l| l.image).and_then(|id| editor.document.image(id)).and_then(|a| png_aspect(&a.source)).unwrap_or(1.0);
            (quad, aspect)
        }
        _ => (mesh, 1.0),
    };
    let align = look.is_some_and(|l| l.align_to_velocity || l.shape == an1mbyt3_core::ParticleShape2D::Streak);
    let stretch = look.map_or(0.0, |l| l.stretch.max(0.0));
    let blend_mode = look.map_or(BlendMode::Normal, |l| l.blend_mode);
    // Flipbook on the emitter: each particle shows its frames (the node's
    // children) over its own life.
    if let Some(book) = artboard.node(node_id).and_then(|n| n.flipbook()) {
        let frames: Vec<NodeId> = artboard.children_of(node_id).filter(|(_, n)| !n.deleted).map(|(c, _)| c).collect();
        if !frames.is_empty() {
            return flipbook_particle_draws(editor, artboard, node_id, config, book, &frames, runtime.particles());
        }
    }
    // `Local` draws every particle in the emitter's own local space,
    // composed fresh with this node's *current* world transform
    // every frame (so it visibly drags along if the emitter moves);
    // `World` particles already carry a real world-space position
    // baked in at spawn time (`an1mbyt3_runtime::particle_sim::spawn`)
    // and are drawn exactly as stored, never touched by the
    // emitter's own transform again — see this module's own doc
    // comment for the full reasoning. `Local`'s own composition is a
    // plain component-wise one (rotate-then-translate the position,
    // add rotations, multiply scales), not a full affine
    // decomposition: correct for the common case (no shear from
    // non-uniform nested scale+rotation), a known approximation for
    // the uncommon one, same "safe over-approximation" precedent
    // `touching`'s own AABB check already establishes elsewhere.
    let emitter_world = artboard.world_transform(node_id);
    let (sin, cos) = emitter_world.rotation.sin_cos();
    let local_space = config.main.simulation_space == an1mbyt3_core::SimulationSpace::Local;
    let mut draws: Vec<DrawCall<'a>> = runtime
        .particles()
        .iter()
        .zip(bindings.iter())
        .map(|(particle, binding)| {
            let age = particle.normalized_age();
            let size_mult = sample_size_over_lifetime(&config.size_over_lifetime, age);
            let tint = sample_color_over_lifetime(&config.color_over_lifetime, age).unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let transform = match config.main.simulation_space {
                an1mbyt3_core::SimulationSpace::World => an1mbyt3_core::Transform2D {
                    translation: particle.position,
                    rotation: particle.rotation,
                    scale: Vec2::splat(particle.base_size * size_mult),
                    anchor: Vec2::ZERO,
                },
                an1mbyt3_core::SimulationSpace::Local => {
                    let scaled = particle.position * emitter_world.scale;
                    let rotated = Vec2::new(scaled.x * cos - scaled.y * sin, scaled.x * sin + scaled.y * cos);
                    an1mbyt3_core::Transform2D {
                        translation: emitter_world.translation + rotated,
                        rotation: emitter_world.rotation + particle.rotation,
                        scale: Vec2::splat(particle.base_size * size_mult) * emitter_world.scale,
                        anchor: Vec2::ZERO,
                    }
                }
            };
            let mut transform = transform;
            // Facing the motion (world direction), stretched by speed.
            let speed = particle.velocity.length();
            if align && speed > 1e-3 {
                let dir = if local_space { emitter_world.rotation + particle.velocity.y.atan2(particle.velocity.x) } else { particle.velocity.y.atan2(particle.velocity.x) };
                transform.rotation = dir;
            }
            if stretch > 0.0 {
                transform.scale.x *= 1.0 + stretch * speed / 100.0;
            }
            transform.scale.x *= aspect;
            DrawCall {
                mesh,
                binding,
                transform,
                fill_color: [
                    config.main.start_color[0] * tint[0],
                    config.main.start_color[1] * tint[1],
                    config.main.start_color[2] * tint[2],
                    config.main.start_color[3] * tint[3],
                ],
                blend_mode,
                gradient: None,
                texture: image,
                material: None,
            }
        })
        .collect();
    // Glow: a soft additive halo per particle, over all of them.
    if let (Some(look), Some((quad, material)), Some(glow_bindings)) = (look.filter(|l| l.glow > 0.0), editor.particle_glow.as_ref(), editor.particle_glow_bindings.get(&node_id)) {
        let halos: Vec<DrawCall<'a>> = draws
            .iter()
            .zip(glow_bindings.iter())
            .map(|(d, binding)| {
                let base = if look.glow_tint { d.fill_color } else { look.glow_color };
                let size = d.transform.scale.x.abs().max(d.transform.scale.y.abs()) * look.glow_size.max(0.1);
                DrawCall {
                    mesh: quad,
                    binding,
                    transform: an1mbyt3_core::Transform2D { translation: d.transform.translation, rotation: 0.0, scale: Vec2::splat(size), anchor: Vec2::ZERO },
                    fill_color: [base[0], base[1], base[2], (base[3] * look.glow.min(1.0)).clamp(0.0, 1.0)],
                    blend_mode: BlendMode::Normal,
                    gradient: None,
                    texture: None,
                    material: Some(material),
                }
            })
            .collect();
        draws.extend(halos);
    }
    draws
}

/// A particle's soft glow: a radial falloff on a unit quad, in the draw's
/// colour, added as light (compiled additive).
const PARTICLE_GLOW_WGSL: &str = r#"
fn material_fragment(in: VertexOutput) -> vec4<f32> {
    let d = length(in.local_position) * 2.0;
    let fall = clamp(1.0 - d, 0.0, 1.0);
    return vec4<f32>(uniforms.color.rgb * 1.4, uniforms.color.a * fall * fall);
}
"#;

/// A Particle Look's shape as a path (any size; `unit_mesh` fits it), with
/// a cache key — `None` for the default circle and for images.
fn particle_shape_path(artboard: &an1mbyt3_core::Artboard, look: &an1mbyt3_core::ParticleLook2D) -> Option<(u64, an1mbyt3_core::Path)> {
    use an1mbyt3_core::{Path, ParticleShape2D as S};
    let builtin = |k: u64, p: Path| Some((k, p));
    match look.shape {
        S::Circle | S::Image => None,
        S::Square => builtin(1, Path::rect(64.0, 64.0)),
        S::Triangle => builtin(2, Path::polygon(3, 32.0)),
        S::Diamond => builtin(3, Path::polygon(4, 32.0)),
        S::Star => builtin(4, Path::star(5, 32.0, 14.0)),
        S::Hexagon => builtin(5, Path::polygon(6, 32.0)),
        S::Ring => {
            // Outer circle one way, inner the other: a hole under any fill rule.
            use an1mbyt3_core::PathCommand as C;
            let ring = |r: f32, reverse: bool| {
                let mut out = Vec::new();
                for i in 0..48 {
                    let i = if reverse { 48 - i } else { i };
                    let a = i as f32 / 48.0 * std::f32::consts::TAU;
                    let p = Vec2::new(a.cos() * r, a.sin() * r);
                    out.push(if out.is_empty() { C::MoveTo(p) } else { C::LineTo(p) });
                }
                out.push(C::Close);
                out
            };
            let mut commands = ring(32.0, false);
            commands.extend(ring(22.0, true));
            builtin(6, Path { commands })
        }
        S::Streak => {
            // A long thin lens along +x (the motion direction).
            use an1mbyt3_core::PathCommand as C;
            let pts = [Vec2::new(32.0, 0.0), Vec2::new(0.0, 5.0), Vec2::new(-32.0, 0.0), Vec2::new(0.0, -5.0)];
            let mut commands: Vec<C> = pts.iter().enumerate().map(|(i, p)| if i == 0 { C::MoveTo(*p) } else { C::LineTo(*p) }).collect();
            commands.push(C::Close);
            builtin(7, Path { commands })
        }
        S::Shape => {
            let id = look.node?;
            let NodeKind::Shape(shape) = &artboard.node(id)?.kind else { return None };
            let key = fingerprint_debug(&(id, &shape.path)) | (1 << 63);
            Some((key, shape.path.clone()))
        }
    }
}

/// Width / height of a PNG, from its header.
fn png_aspect(bytes: &[u8]) -> Option<f32> {
    if bytes.len() < 24 || &bytes[1..4] != b"PNG" {
        return None;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    (w > 0 && h > 0).then(|| w as f32 / h as f32)
}

/// `path` tessellated and fitted into the unit box (−0.5 … 0.5), centred.
/// (Tessellated at its own size — lyon's tolerance is absolute.)
fn unit_mesh(path: &an1mbyt3_core::Path) -> an1mbyt3_geometry::Mesh {
    let mut mesh = tessellate_path(path);
    let (min, max) = path.bounds();
    let center = (min + max) * 0.5;
    let extent = (max - min).max_element().max(1e-3);
    for p in &mut mesh.positions {
        p[0] = (p[0] - center.x) / extent;
        p[1] = (p[1] - center.y) / extent;
    }
    mesh
}

/// A Tree 2D's parts, back to front, in the node's own space.
fn tree_draws<'a>(editor: &'a WasmEditor, artboard: &'a an1mbyt3_core::Artboard, node_id: NodeId) -> Vec<DrawCall<'a>> {
    let Some(gpu) = editor.tree_gpu.get(&node_id) else { return Vec::new() };
    let transform = artboard.world_transform(node_id);
    gpu.parts
        .iter()
        .zip(gpu.bindings.iter())
        .map(|((mesh, color), binding)| DrawCall { mesh, binding, transform, fill_color: *color, blend_mode: BlendMode::Normal, gradient: None, texture: None, material: None })
        .collect()
}

/// A Cloth 2D's fabric (world space, textured), then its shade overlays:
/// folds, creases and backs darkened, stretched spots lightened.
fn cloth_draws<'a>(editor: &'a WasmEditor, node_id: NodeId) -> Vec<DrawCall<'a>> {
    let Some(gpu) = editor.cloth_gpu.get(&node_id) else { return Vec::new() };
    let s = gpu.shading.clamp(0.0, 1.0);
    let tints = [[1.0, 1.0, 1.0, 1.0], [0.0, 0.0, 0.05, 0.16 * s], [0.0, 0.0, 0.05, 0.32 * s], [0.0, 0.0, 0.05, 0.48 * s], [1.0, 1.0, 1.0, 0.06 * s]];
    gpu.meshes
        .iter()
        .zip(gpu.bindings.iter())
        .zip(tints)
        .enumerate()
        .filter_map(|(i, ((mesh, binding), fill_color))| {
            Some(DrawCall {
                mesh: mesh.as_ref()?,
                binding,
                transform: an1mbyt3_core::Transform2D::default(),
                fill_color,
                blend_mode: BlendMode::Normal,
                gradient: None,
                texture: (i == 0).then_some(&gpu.texture),
                material: None,
            })
        })
        .collect()
}

/// A Trail Renderer component's ribbon (already in world space).
fn trail_draws<'a>(editor: &'a WasmEditor, node_id: NodeId) -> Vec<DrawCall<'a>> {
    let Some(gpu) = editor.trail_gpu.get(&node_id) else { return Vec::new() };
    let Some(mesh) = gpu.mesh.as_ref() else { return Vec::new() };
    vec![DrawCall {
        mesh,
        binding: &gpu.binding,
        transform: an1mbyt3_core::Transform2D::default(),
        fill_color: [1.0, 1.0, 1.0, 1.0],
        blend_mode: BlendMode::Normal,
        gradient: None,
        texture: Some(&gpu.texture),
        material: None,
    }]
}

/// The compiled shader-graph material a node wears — its (enabled)
/// SpriteRenderer component's `material`, if that compiled successfully.
/// A material that failed to compile draws the node normally.
fn node_material<'a>(editor: &'a WasmEditor, node: &Node) -> Option<&'a an1mbyt3_renderer::GpuMaterial> {
    let id = node.behaviors.iter().find_map(|entry| match &entry.behavior {
        an1mbyt3_core::NodeBehavior::SpriteRenderer(sprite) if entry.enabled => sprite.material,
        _ => None,
    })?;
    editor.material_gpu.get(&id).and_then(|(_, compiled)| compiled.as_ref().ok())
}

/// An imported model on the GPU (see `WasmEditor::gpu_model`).
struct GpuModel {
    data: an1mbyt3_geometry::ModelData,
    /// Per primitive: its mesh and base-colour texture.
    primitives: Vec<(std::rc::Rc<an1mbyt3_renderer::GpuMesh3D>, Option<an1mbyt3_renderer::GpuImageTexture>)>,
    /// The rest pose and each node's parent (spring bones need both).
    rest: Vec<(glam::Vec3, glam::Quat, glam::Vec3)>,
    parents: Vec<Option<usize>>,
    /// VRM avatars: the skeleton retargeted humanoid clips play on.
    humanoid: Option<an1mbyt3_geometry::retarget::HumanoidTarget>,
}

/// One model node's runtime state (see `WasmEditor::model_instances`).
#[derive(Default)]
struct ModelInstance {
    model: Option<an1mbyt3_core::ModelId>,
    /// Script bone poses: node → a rotation on top of the animated local
    /// rotation, and a translation added to it (hips root motion).
    bones: HashMap<usize, glam::Quat>,
    offsets: HashMap<usize, glam::Vec3>,
    /// Expression name → weight 0..1.
    expressions: HashMap<String, f32>,
    /// VRM Poses: the pose a script switched to (`ref.setPose3`; `None` =
    /// the component's active one) and its crossfade seconds, and the
    /// blended pose right now (bone → rotation, offset; expressions).
    pose_override: Option<String>,
    /// Ragdoll: a script's request (go limp / get up, with a starting
    /// velocity), and while limp, which glTF node each physics part drives
    /// (+ its humanoid name, for hits).
    ragdoll_request: Option<(bool, glam::Vec3)>,
    /// Humanoid Animator playback: the clip and its time, the clip fading
    /// out (and how much is left of the fade, 1 → 0), and a script's
    /// override (`ref.playAnim3`: name, loop, speed, blend).
    anim: Option<(String, f32)>,
    anim_prev: Option<(String, f32, f32)>,
    anim_override: Option<(String, Option<bool>, Option<f32>, Option<f32>)>,
    ragdoll_bones: Option<Vec<(usize, String)>>,
    /// `start_ragdolled` already acted on this Playground run.
    ragdoll_started: bool,
    pose_blend: Option<f32>,
    pose_bones: HashMap<usize, (glam::Quat, glam::Vec3)>,
    pose_expressions: HashMap<String, f32>,
    springs: an1mbyt3_geometry::vrm::SpringSim,
    /// Per primitive: this instance's own deformed mesh, when it needs one
    /// (morphed expressions; CPU-morphed unskinned meshes).
    meshes: Vec<Option<std::rc::Rc<an1mbyt3_renderer::GpuMesh3D>>>,
    /// Reused interleaved vertex buffer (morphed / deformed vertices).
    scratch: Vec<f32>,
    /// Per primitive: the morph weights its own mesh copy was last built
    /// with (skinned + morphed primitives re-upload only on change).
    morph_keys: Vec<Vec<f32>>,
    /// Last frame's model-space node matrices and the node's world matrix
    /// (bone positions for scripts: hitboxes, effects, camera targets).
    last_pose: Vec<glam::Mat4>,
    last_world: glam::Mat4,
}

/// One model node's animation playback (base + overlay clips).
struct AnimClock {
    clips: [Option<an1mbyt3_core::AnimationName>; 2],
    times: [f32; 2],
}

/// The node kind's own geometry.
fn kind_draws<'a>(editor: &'a WasmEditor, artboard: &'a an1mbyt3_core::Artboard, node_id: NodeId, node: &'a Node) -> Vec<DrawCall<'a>> {
    match &node.kind {
        NodeKind::Shape(shape) => {
            // A zero-area path (a straight line) has no fill mesh but can
            // still have a stroke: draw just that.
            let Some((mesh, binding)) = editor.mesh_cache.get(&node_id) else {
                let Some(stroke) = &shape.stroke else { return Vec::new() };
                let Some((stroke_mesh, stroke_binding)) = editor.stroke_mesh_cache.get(&node_id) else { return Vec::new() };
                return vec![DrawCall {
                    mesh: stroke_mesh,
                    binding: stroke_binding,
                    transform: artboard.world_transform(node_id),
                    fill_color: stroke.color,
                    blend_mode: shape.blend_mode,
                    gradient: stroke.gradient.as_ref(),
                    texture: stroke.image.and_then(|image_id| editor.image_cache.get(&image_id)),
                    material: None,
                }];
            };
            // `shape.image` textures this shape with an already-imported
            // raster asset instead of a flat/gradient fill — see that
            // field's own doc comment. Reuses the same `image_cache` an
            // `Image` node's own texture lookup already reads (keyed by
            // `ImageId`, not `NodeId` — nothing Shape-specific about the
            // cache itself), and this node's own already-tessellated path
            // mesh/binding above unchanged either way (`fs_textured` derives
            // UV from that mesh's own local bounds, not a special quad).
            let texture = shape.image.and_then(|image_id| editor.image_cache.get(&image_id));
            let material = node_material(editor, node);
            let mut draws = vec![DrawCall {
                mesh,
                binding,
                transform: artboard.world_transform(node_id),
                fill_color: shape.fill_color,
                blend_mode: shape.blend_mode,
                gradient: if material.is_some() { None } else { shape.gradient.as_ref() },
                texture,
                material,
            }];
            // A stroke is drawn as its own second `DrawCall` — its own
            // independently tessellated mesh (`stroke_mesh_cache`) and own
            // paint, same transform/blend mode as the fill. No new pipeline
            // needed: this reuses whichever flat/gradient/textured pipeline
            // the fill draw above already does, per the blend mode in play.
            if let Some(stroke) = &shape.stroke {
                if let Some((stroke_mesh, stroke_binding)) = editor.stroke_mesh_cache.get(&node_id) {
                    let stroke_texture = stroke.image.and_then(|image_id| editor.image_cache.get(&image_id));
                    draws.push(DrawCall {
                        mesh: stroke_mesh,
                        binding: stroke_binding,
                        transform: artboard.world_transform(node_id),
                        fill_color: stroke.color,
                        blend_mode: shape.blend_mode,
                        gradient: stroke.gradient.as_ref(),
                        texture: stroke_texture,
                        material: None,
                    });
                }
            }
            draws
        }
        NodeKind::Instance(instance) => {
            let Some(definition) = editor.document.component(instance.component) else { return Vec::new() };
            let Some((mesh, binding)) = editor.mesh_cache.get(&node_id) else { return Vec::new() };
            vec![DrawCall {
                mesh,
                binding,
                transform: artboard.world_transform(node_id),
                fill_color: instance.resolved_fill_color(definition),
                blend_mode: BlendMode::Normal,
                gradient: None,
                texture: None,
                material: None,
            }]
        }
        NodeKind::Text(text) => {
            let Some(cached) = editor.text_mesh_cache.get(&node_id) else { return Vec::new() };
            let node_transform = artboard.world_transform(node_id);
            cached
                .glyphs
                .iter()
                .map(|(mesh, binding, offset)| DrawCall {
                    mesh,
                    binding,
                    transform: node_transform.with_local_offset(*offset),
                    fill_color: text.fill_color,
                    blend_mode: BlendMode::Normal,
                    gradient: text.gradient.as_ref(),
                    texture: None,
                    material: None,
                })
                .collect()
        }
        NodeKind::Image(image_node) if editor.skin_meshes.contains_key(&node_id) => {
            // Sprite Skin 2D: the mesh is already in world space.
            let Some((mesh, binding)) = editor.skin_meshes.get(&node_id) else { return Vec::new() };
            let Some(texture) = editor.image_cache.get(&image_node.image) else { return Vec::new() };
            vec![DrawCall {
                mesh,
                binding,
                transform: an1mbyt3_core::Transform2D::default(),
                fill_color: image_node.fill_color,
                blend_mode: BlendMode::Normal,
                gradient: None,
                texture: Some(texture),
                material: None,
            }]
        }
        NodeKind::Image(image_node) => {
            let Some(binding) = editor.image_bindings.get(&node_id) else { return Vec::new() };
            let Some(texture) = editor.image_cache.get(&image_node.image) else { return Vec::new() };
            let Some(quad_mesh) = editor.image_quad_mesh.as_ref() else { return Vec::new() };
            vec![DrawCall {
                mesh: quad_mesh,
                binding,
                transform: artboard.world_transform(node_id).compose(&an1mbyt3_core::Transform2D {
                    scale: Vec2::new(image_node.width, image_node.height),
                    ..Default::default()
                }),
                fill_color: image_node.fill_color,
                blend_mode: BlendMode::Normal,
                gradient: None,
                texture: Some(texture),
                material: node_material(editor, node),
            }]
        }
        // Particle systems / trails are components now — drawn below by
        // `node_draws` for any node carrying one.
        NodeKind::ParticleSystem(_) => Vec::new(),
        NodeKind::Tilemap(tilemap) => {
            let Some((mesh, binding)) = editor.tilemap_mesh_cache.get(&node_id) else { return Vec::new() };
            let Some(texture) = editor.image_cache.get(&tilemap.tileset) else { return Vec::new() };
            vec![DrawCall {
                mesh,
                binding,
                transform: artboard.world_transform(node_id),
                fill_color: [1.0, 1.0, 1.0, 1.0],
                blend_mode: BlendMode::Normal,
                gradient: None,
                texture: Some(texture),
                material: None,
            }]
        }
        NodeKind::Trail(_) => Vec::new(),
        // 3D nodes draw in the renderer's 3D pass (`collect_3d`), not as 2D geometry.
        NodeKind::Node3D(_) => Vec::new(),
        // Lights don't draw as geometry — they're gathered separately into
        // the renderer's lighting pass (`lighting2d::collect_lights`).
        NodeKind::Group(_) | NodeKind::Bone(_) | NodeKind::Camera(_) | NodeKind::Light2D(_) => Vec::new(),
    }
}

/// Clones `source_id` (and, recursively, its children) as a new node in the
/// *same* `document`/`artboard` — `WasmEditor::duplicate_nodes`'s per-node
/// worker. Snapshots the source node's data (a plain `.clone()`) and its
/// child ids via a short-lived immutable borrow that ends before the
/// `Command::AddNode` mutation, since (unlike `insert_imported_node`, which
/// reads from a separate, already-fully-built source `Document`) source and
/// destination are the same `Document` here and can't be borrowed
/// immutably/mutably at once. Otherwise identical in shape: records
/// `source_id -> freshly-assigned id` in `id_map` (unused by
/// `duplicate_nodes` today, kept for parity and in case a future caller
/// needs to remap something — e.g. an animation track — the way
/// `import_svg` does) and every new id in `new_nodes`/`inverses`.
fn duplicate_node_recursive(
    document: &mut Document,
    artboard: ArtboardId,
    source_id: NodeId,
    parent: Option<NodeId>,
    id_map: &mut HashMap<NodeId, NodeId>,
    inverses: &mut Vec<Command>,
    new_nodes: &mut Vec<NodeId>,
) {
    let (mut node, child_ids) = {
        let artboard_ref = document.artboard(artboard).expect("artboard exists");
        let node = artboard_ref.node(source_id).expect("source_id came from this artboard's own tree").clone();
        let child_ids: Vec<NodeId> = artboard_ref.children_of(source_id).map(|(id, _)| id).collect();
        (node, child_ids)
    };
    node.parent = parent;
    // A duplicate must not keep the original's `id` — `Artboard::add_node`
    // only mints a fresh `shape(<serial>)` id when it's empty, and copying
    // the source's id verbatim would leave two live nodes answering to the
    // same `getCompByID` lookup (`scriptRuntime.ts`).
    node.id = String::new();
    let inverse = Command::AddNode { artboard, node }.apply(document);
    let Command::SetDeleted { node: new_id, .. } = inverse else {
        unreachable!("Command::AddNode's inverse is always a SetDeleted")
    };
    inverses.push(inverse);
    new_nodes.push(new_id);
    id_map.insert(source_id, new_id);

    for child_id in child_ids {
        duplicate_node_recursive(document, artboard, child_id, Some(new_id), id_map, inverses, new_nodes);
    }
}

/// Parses the Inspector's blend-mode strings back into a `BlendMode` —
/// mirrors `parse_property_kind` above.
fn parse_blend_mode(value: &str) -> Result<BlendMode, JsValue> {
    match value {
        "Normal" => Ok(BlendMode::Normal),
        "Multiply" => Ok(BlendMode::Multiply),
        "Screen" => Ok(BlendMode::Screen),
        "Add" => Ok(BlendMode::Add),
        other => Err(JsValue::from_str(&format!("unknown blend mode: {other}"))),
    }
}

/// `node`'s own `fill_color`, for whichever kinds actually have one —
/// `set_mode`'s design-pose/color snapshot's own read side. Mirrors
/// `an1mbyt3_runtime::graph::read_property`'s `FillColor*` match arms
/// (that crate's own version is `pub(crate)`, not reachable from here).
fn node_fill_color(node: &Node) -> Option<[f32; 4]> {
    node.fill_color()
}

/// The write side of `node_fill_color` — a no-op for a node kind with no
/// fill color at all (can't happen in practice here: `set_mode` only ever
/// calls this with a color `node_fill_color` itself already produced for
/// this same node, but harmless either way, same "degrades quietly"
/// convention `write_property` itself already follows).
fn set_node_fill_color(node: &mut Node, color: [f32; 4]) {
    if let Some(fill) = node.fill_color_mut() {
        *fill = color;
    }
}

/// Retimes the keyframe at `index` to `new_time` — shared by
/// `preview_keyframe_time`/`commit_keyframe_time`'s per-`PropertyTrack`-variant
/// match arms, since retiming never touches the keyframe's value, only its
/// time (mirrors `an1mbyt3_core::command`'s private `move_keyframe_time`,
/// which isn't public — this crate needs its own copy for index-addressed,
/// not time-addressed, retiming during a live drag preview).
fn set_keyframe_time_at<T: Lerp>(track: &mut Track<T>, index: usize, new_time: f32) -> Result<(), JsValue> {
    track
        .keyframes
        .get_mut(index)
        .ok_or_else(|| JsValue::from_str("keyframe index out of range"))?
        .time = new_time;
    Ok(())
}

/// A small filled circle, centered on the origin — the motion-path overlay's
/// per-keyframe marker (`render`'s doc comment). Built from 4 cubic Bézier
/// arcs via the standard "kappa" constant (0.5522847498), the same
/// circle-approximation technique `dash_and_offset.rs`'s demo already uses —
/// `an1mbyt3-core` has no dedicated circle primitive (`Path::rect` is the
/// only named shape constructor).
fn build_dot_path(radius: f32) -> Path {
    const KAPPA: f32 = 0.5522847498;
    let k = radius * KAPPA;
    let mut path = Path::new();
    path.commands.push(an1mbyt3_core::PathCommand::MoveTo(Vec2::new(radius, 0.0)));
    path.commands.push(an1mbyt3_core::PathCommand::CubicTo {
        control1: Vec2::new(radius, k),
        control2: Vec2::new(k, radius),
        to: Vec2::new(0.0, radius),
    });
    path.commands.push(an1mbyt3_core::PathCommand::CubicTo {
        control1: Vec2::new(-k, radius),
        control2: Vec2::new(-radius, k),
        to: Vec2::new(-radius, 0.0),
    });
    path.commands.push(an1mbyt3_core::PathCommand::CubicTo {
        control1: Vec2::new(-radius, -k),
        control2: Vec2::new(-k, -radius),
        to: Vec2::new(0.0, -radius),
    });
    path.commands.push(an1mbyt3_core::PathCommand::CubicTo {
        control1: Vec2::new(k, -radius),
        control2: Vec2::new(radius, -k),
        to: Vec2::new(radius, 0.0),
    });
    path.commands.push(an1mbyt3_core::PathCommand::Close);
    path
}

/// Grid-cell spacing (local units) for the artboard's background grid.
const ARTBOARD_GRID_SPACING: f32 = 40.0;
/// Only this constant's own alpha is still fixed here — `grid_r/g/b`
/// (`render`'s own parameters, same theme-driven-from-the-frontend pattern
/// `clear_r/g/b` already established) carry the actual color now, since a
/// fixed dark-gray-on-dark-canvas tuning stopped making sense once a light
/// theme existed: the frontend picks a grid color against its *own*
/// clear color each frame (`apps/editor/src/theme.ts`'s `GRID_COLOR`) the
/// same way it already does for the clear color itself.
const ARTBOARD_GRID_COLOR: [f32; 4] = [0.2, 0.2, 0.23, 0.85];

/// A graph-paper-style grid spanning the artboard's interior — one `MoveTo`/
/// `LineTo` pair per line, all in a single `Path` (`tessellate_stroke`
/// starts a fresh subpath at each `MoveTo`, so this tessellates into one
/// mesh/one draw call for the whole grid rather than one per line). Lines
/// sit at multiples of `spacing` outward from the center on both axes.
fn build_grid_path(width: f32, height: f32, spacing: f32) -> Path {
    let (hw, hh) = (width / 2.0, height / 2.0);
    let mut path = Path::new();

    let mut x = 0.0;
    while x <= hw {
        path.commands.push(an1mbyt3_core::PathCommand::MoveTo(Vec2::new(x, -hh)));
        path.commands.push(an1mbyt3_core::PathCommand::LineTo(Vec2::new(x, hh)));
        if x != 0.0 {
            path.commands.push(an1mbyt3_core::PathCommand::MoveTo(Vec2::new(-x, -hh)));
            path.commands.push(an1mbyt3_core::PathCommand::LineTo(Vec2::new(-x, hh)));
        }
        x += spacing;
    }

    let mut y = 0.0;
    while y <= hh {
        path.commands.push(an1mbyt3_core::PathCommand::MoveTo(Vec2::new(-hw, y)));
        path.commands.push(an1mbyt3_core::PathCommand::LineTo(Vec2::new(hw, y)));
        if y != 0.0 {
            path.commands.push(an1mbyt3_core::PathCommand::MoveTo(Vec2::new(-hw, -y)));
            path.commands.push(an1mbyt3_core::PathCommand::LineTo(Vec2::new(hw, -y)));
        }
        y += spacing;
    }

    path
}

/// The whole editor: a real (non-headless) `Renderer` bound to the canvas's
/// own surface, the real `Document` (undo/redo included), and a cache of
/// uploaded GPU meshes per shape node — same caching reasoning as every
/// other `GpuMesh` use in this workspace (§6: re-tessellate only when a
/// shape's actual path data changes, never per frame).
#[wasm_bindgen]
pub struct WasmEditor {
    renderer: Renderer,
    document: Document,
    mesh_cache: HashMap<NodeId, (GpuMesh, DrawBinding)>,
    /// The exact `Path` last tessellated into `mesh_cache`'s entry for a
    /// given node — lets the population loop below tell "never cached yet"
    /// apart from "cached, but stale" (a `Path` track animating this shape,
    /// or the path-editing overlay, changed `shape.path` since). Without
    /// this, `mesh_cache`'s own doc comment's "re-tessellate only when a
    /// shape's actual path data changes" was aspirational, not enforced —
    /// nothing ever compared the cached mesh against the *current* path,
    /// only checked whether an entry existed at all, so an edited path
    /// silently kept rendering its old geometry forever. Harmless before
    /// this session (nothing ever changed a shape's `Path` post-creation);
    /// load-bearing now that both exist.
    shape_mesh_source: HashMap<NodeId, Path>,
    /// A shape's stroke outline is a second, independently cached mesh —
    /// `tessellate_stroke`'s own geometry, not a variant of the fill mesh
    /// above. Keyed the same way, invalidated the same way (`stroke_mesh_source`
    /// tracks the exact `(path, width, cap, join)` last tessellated — a
    /// stroke's *color*/gradient/image never affect its mesh, only its
    /// paint, so those aren't part of the cache key).
    stroke_mesh_cache: HashMap<NodeId, (GpuMesh, DrawBinding)>,
    stroke_mesh_source: HashMap<NodeId, (Path, f32, an1mbyt3_core::StrokeCap, an1mbyt3_core::StrokeJoin)>,
    /// A `Tilemap` node's whole grid, batched into one mesh with real
    /// per-vertex UVs (`build_tilemap_mesh`) — same cache/staleness-key
    /// shape as `mesh_cache`/`shape_mesh_source` above, just keyed on the
    /// tile grid's own content (any `Command::SetTile` paint edit) instead
    /// of a `Path`.
    tilemap_mesh_cache: HashMap<NodeId, (GpuMesh, DrawBinding)>,
    /// Sprite Skin 2D: this frame's deformed (world-space) mesh per skinned
    /// Image node, re-uploaded every frame since the bones move.
    skin_meshes: HashMap<NodeId, (GpuMesh, DrawBinding)>,
    /// Per skinned node: the binding it was weighted for, plus its rest
    /// mesh and weights — recomputed only when the binding changes.
    skin_bindings: HashMap<NodeId, SkinCache>,
    tilemap_mesh_source: HashMap<NodeId, Vec<Option<u32>>>,
    /// Parsed `Font` per `FontId` — parsing (`rustybuzz`/`ttf-parser`) is
    /// worth memoizing the same way tessellation is, and a `FontAsset`'s raw
    /// bytes never change after `Document::add_font`, so this never needs
    /// invalidating, only populating lazily on first use.
    font_cache: HashMap<FontId, Font>,
    text_mesh_cache: HashMap<NodeId, TextMeshCache>,
    state_machine: StateMachine,
    /// Whether `render` should advance/apply `state_machine` this frame —
    /// `false` while the editor's Design tab is active, so Inspector/drag
    /// edits to the state machine's own target node ("Square") are never
    /// silently overwritten by preview playback; `true` only while the
    /// State Machine tab (`set_state_machine_active`) is selected.
    state_machine_active: bool,
    /// Whether the toolbar's mode is currently "Playground" — the one other
    /// place (besides the State Machine tab, gated by `state_machine_active`
    /// *and* `state_machine_programs_enabled`) a Programmable State's
    /// `program` is ever allowed to actually run. Unlike the State Machine
    /// tab, Playground has no separate toggle: programs always run there —
    /// it exists specifically to be "the safe place programs are always
    /// live," per the same reasoning that makes the State Machine tab's own
    /// toggle default `false` (see `state_machine_programs_enabled`'s doc
    /// comment). `render` treats `state_machine_active`/`playground_active`
    /// identically for the purposes of "should `self.state_machine`
    /// advance/apply at all this frame" — they only differ in whether
    /// programs specifically are allowed to run.
    playground_active: bool,
    /// The State Machine tab's own opt-in toggle for running a Programmable
    /// State's `program` while previewing there — `false` by default, so
    /// simply switching to the State Machine tab to test transitions/clips
    /// never silently starts running arbitrary authored code/graphs; an
    /// author has to deliberately flip this (or switch to Playground, which
    /// always runs them) to see a program actually drive anything outside
    /// Design mode. Has no effect at all while `state_machine_active` is
    /// `false` (nothing runs in Design/Animate mode either way).
    state_machine_programs_enabled: bool,
    /// Which `Animation` the Timeline panel is currently authoring/scrubbing
    /// — session state, not saved document content (the animations
    /// themselves are; which one happens to be selected in the dropdown
    /// right now isn't). `None` only if every animation on the artboard was
    /// deleted; `build_start_document`/`add_animation` otherwise keep this
    /// populated.
    active_animation: Option<AnimationId>,
    /// Which artboard ("Scene") the editor is currently showing/editing —
    /// session state, same as `active_animation` (which scene happens to be
    /// open right now isn't saved document content). `None`, or an id that
    /// no longer resolves/was deleted, falls back to the first non-deleted
    /// artboard — see `current_artboard_id`, which every "which artboard do
    /// I render/resample/preview" decision here goes through instead of the
    /// `artboards().next()` every one of them used to hard-code.
    active_artboard: Option<ArtboardId>,
    /// Which `StateMachineDef` the State Machine tab currently
    /// previews/tests — mirrors `active_animation`'s reasoning exactly,
    /// just for state machines instead of animations. `self.state_machine`
    /// (the *live* instance) is rebuilt from whichever definition this
    /// points at every time an authoring edit changes it (see
    /// `refresh_state_machine_preview`).
    active_state_machine: Option<an1mbyt3_core::StateMachineId>,
    /// The Timeline playhead's current position, seconds — session state,
    /// mirrored here (not just in the frontend) so `commit_node_translation`/
    /// `set_node_rotation`/`set_node_scale` can check "is there already a
    /// keyframe for this property at exactly this time" without the
    /// frontend having to pass it on every single edit call.
    scrub_time: f32,
    /// Whether the toolbar's mode is currently "Animate" — mirrors
    /// `state_machine_active`'s reasoning exactly, just for a different tab.
    /// Gates `apply_transform_edit`/`commit_node_translation`'s choice
    /// between "overwrite the keyframe at the playhead" (Animate mode: an
    /// edit made while actively authoring keyframes shouldn't silently
    /// rebase the whole path) and "rebase every keyframe by this delta"
    /// (Design mode: repositioning the node's rest pose should carry its
    /// whole animated path along, per "animate relative to its new
    /// position"). Without this, positioning a node for a brand-new
    /// keyframe while scrubbed to an empty moment in Animate mode was
    /// incorrectly triggering a path-wide rebase instead of an isolated edit.
    animate_mode_active: bool,
    /// Timeline "Record" (●): in Animate, *any* Inspector change keys the
    /// changed values at the playhead (otherwise only already-animated
    /// values are keyed).
    record_keys: bool,
    /// Every node's authored (Design-mode) transform, captured the moment
    /// the editor *first* enters Animate or State Machine mode from Design —
    /// `None` while in Design mode (nothing to restore), `Some` for as long
    /// as either animated mode is active. `Animation::apply`/`StateMachine::apply`
    /// both mutate `Node::transform` (and, via a Programmable State's own
    /// `SetProperty`/`ChangeProperty` on a `FillColor*` `GraphProperty`,
    /// `fill_color`) directly, bypassing `Command`/undo (by design — see
    /// their own doc comments), so there's nothing in the document model
    /// itself that remembers "what this looked like before playback
    /// started" — leaving Animate/State Machine mode used to leave every
    /// node sitting at whatever pose *and color* the last scrub/tick/state
    /// happened to mutate it to, rather than snapping back to its actually-
    /// authored rest look. Color was a real, hand-confirmed gap here even
    /// after transform itself was already handled: a state's own `SetProperty`
    /// on fill color stuck around permanently after returning to Design,
    /// the one piece this snapshot didn't cover. `set_mode` is the only
    /// reader/writer — see its own doc comment for why this snapshots/
    /// restores on the *combined* Design-vs-either-animated-mode transition,
    /// not per-flag, so switching directly between Animate and State Machine
    /// (no Design in between) neither re-snapshots an already-animated
    /// pose/color nor loses the original.
    ///
    /// The snapshot carries the scene it was taken of and is only ever
    /// restored onto *that* scene. (It used to be restored onto "the
    /// current scene": deleting the open scene mid-play made the current
    /// scene fall back to another one, and the deleted scene's poses —
    /// transforms, visibility, fills, z — were written onto that scene's
    /// nodes wherever the `NodeId`s collided.)
    design_pose_snapshot: Option<PoseSnapshot>,
    /// Prefab instances scripts spawned during this Playground session
    /// (`spawn_prefab`) — not document edits, so they're hard-removed when
    /// play ends (`despawn_runtime_spawned`) instead of being undone.
    runtime_spawned: Vec<(ArtboardId, NodeId)>,
    /// Binary files of the last `save_project`, until JS takes them.
    pending_project_bytes: Vec<Vec<u8>>,
    /// Which node the Layers panel/canvas currently has selected — session
    /// state, not saved document content, tracked here (not just in the
    /// frontend) so `render` can draw that node's authored motion path (see
    /// `render`'s doc comment) without needing the frontend to pass the
    /// selection on every single frame.
    selected_node: Option<NodeId>,
    /// The actual surface pixel dimensions (updated in `create`/`resize`),
    /// used as `render`'s projection's viewport size — no longer necessarily
    /// the artboard's own size, now that the canvas fills the available
    /// viewport instead of being sized 1:1 to the artboard. The camera
    /// itself (pan/zoom) isn't stored here at all — `App.tsx` owns that
    /// state and passes it into `render` fresh every frame, the same way
    /// `delta_seconds` already is, rather than needing to be kept in sync
    /// with a second copy.
    canvas_width: u32,
    canvas_height: u32,
    /// A small filled circle mesh, reused (different `DrawBinding`/transform
    /// per marker) for every keyframe dot in the selected node's motion-path
    /// overlay — created lazily on first use, never invalidated (its shape
    /// never changes).
    motion_path_dot_mesh: Option<GpuMesh>,
    /// The artboard's background chrome (fill + grid lines — see `render`'s
    /// doc comment on why this needs to be drawn explicitly now). Cached the
    /// same lazy-populate way `text_mesh_cache` is, rebuilt only if the
    /// artboard's own dimensions change (nothing in the editor's toolbar
    /// resizes an artboard yet, so in practice this is built once and never
    /// invalidated).
    artboard_chrome: Option<ArtboardChrome>,
    /// Unit square + 4 uniform slots for the Pixel Perfect camera's crop
    /// bars (`CameraFrame::crop`), built on first use.
    crop_bars: Option<(GpuMesh, Vec<DrawBinding>)>,
    /// Shared unit quad every `NodeKind::Image` node draws through — see
    /// `an1mbyt3_geometry::unit_quad_mesh`'s doc comment. Built once, lazily,
    /// never invalidated (its geometry never changes).
    image_quad_mesh: Option<GpuMesh>,
    /// One `DrawBinding` per `Image` node — separate from `image_cache`
    /// (below) since many nodes can share one baked texture but each still
    /// needs its own transform/tint uniform slot, same reasoning `mesh_cache`
    /// already documents for why a shared mesh still needs per-instance bindings.
    image_bindings: HashMap<NodeId, DrawBinding>,
    /// Baked GPU textures per `ImageId` — populated by `import_image`/
    /// `set_image_processing`/`load_tea`/`load_teab` (all real `.await`
    /// points; baking is a GPU readback, which the plain-sync `render`
    /// method below has no way to perform), never by `render` itself.
    image_cache: HashMap<an1mbyt3_core::ImageId, an1mbyt3_renderer::GpuImageTexture>,
    /// Compiled shader-graph materials: the WGSL each was built from (so an
    /// unchanged graph is never recompiled) and the pipeline, or the
    /// compile error the Shader tab shows. Synced every frame in `render`.
    material_gpu: HashMap<an1mbyt3_core::MaterialId, (String, Result<an1mbyt3_renderer::GpuMaterial, String>)>,
    /// `(document instance, revision)` of the last `sync_materials` /
    /// `material3d` check: materials only change through commands, so
    /// while this matches, re-generating every graph's WGSL (per frame,
    /// and per 3D mesh) to compare it with the cache is skipped.
    materials_checked_at: Option<(u64, u64)>,
    material3d_checked_at: HashMap<an1mbyt3_core::MaterialId, (u64, u64)>,
    /// Seconds since the editor started — shader graphs' `Time`.
    shader_clock: f32,
    /// Uploaded 3D primitive meshes, keyed by their exact parameters
    /// (`format!("{primitive:?}")`) — every cube of the same size shares one.
    mesh3d_cache: HashMap<Mesh3DKey, std::rc::Rc<an1mbyt3_renderer::GpuMesh3D>>,
    /// Shader-graph materials compiled for the 3D pass (keyed like
    /// `material_gpu`, by the generated WGSL).
    material_gpu_3d: HashMap<an1mbyt3_core::MaterialId, (String, Result<std::rc::Rc<an1mbyt3_renderer::GpuMaterial3D>, String>)>,
    /// Design mode's 3D scene view (Unity's Scene view) — on: the canvas
    /// shows the 3D world through `view3d_camera` with a grid, markers and
    /// a move gizmo; off (or Playground): through the scene's Camera 3D.
    view3d_enabled: bool,
    view3d_camera: crate::view3d::EditorCamera3D,
    /// The node the gizmo / selection box is drawn for.
    view3d_selected: Option<NodeId>,
    /// A multi-selection scale / rotate drag in progress: the artboard and
    /// each moved node's transform when it started (see
    /// `multi_transform_begin`).
    multi_transform: Option<(ArtboardId, Vec<(NodeId, an1mbyt3_core::Transform2D)>)>,
    /// The scene walk's current sorting-layer pass (`None` = every layer)
    /// and whether it also collects the unlit overlay (only one pass may).
    scene_layer_filter: std::cell::Cell<Option<u32>>,
    /// The node whose Image Effects layer is being built right now — its
    /// own walk inside the layer must draw it normally, not start another
    /// layer (see `build_scene_ops_lit`).
    fx_building: std::cell::Cell<Option<NodeId>>,
    /// Same, for a Reference object's faded layer.
    reference_building: std::cell::Cell<Option<NodeId>>,
    scene_collect_unlit: std::cell::Cell<bool>,
    /// Draw bindings for draw-effect copies (shadows, outlines, repeats,
    /// mirrors) — handed out during the scene walk, reset each frame.
    effect_bindings: BindingArena,
    /// The scene camera's position this frame (Parallax).
    effect_camera_pan: std::cell::Cell<Vec2>,
    /// Camera Shake "trauma" per camera (0..1), decaying each frame.
    camera_trauma: HashMap<NodeId, f32>,
    /// Move / rotate / scale (W / E / R).
    view3d_mode: crate::view3d::GizmoMode,
    /// A selected bone of the selected avatar (humanoid name): the gizmo
    /// rotates it, writing into the node's active VRM pose.
    view3d_bone: Option<String>,
    /// While dragging a bone: its world rotation without the pose's own
    /// rotation (`world = basis * pose`), from the drag's start.
    view3d_bone_basis: Option<glam::Quat>,
    /// The gizmo handle under the mouse / being dragged (highlighted).
    view3d_hot: Option<crate::view3d::GizmoHandle>,
    /// The last `render` call's device pixel ratio (CSS px -> canvas px).
    last_device_pixel_ratio: f32,
    /// Exporting a transparent GIF / video: frames clear to alpha 0 and
    /// skip editor chrome (the grid) — see `set_export_transparent`.
    export_transparent: bool,
    /// An Export (GIF / video) is rendering frames: post effects, particles
    /// and goo render as at play time — see `set_exporting`.
    exporting: bool,
    /// Design-mode "preview on" screen size (CSS px) — how UI Layouts,
    /// UI Anchors and Show When react to a phone / tablet / desktop
    /// without leaving the editor. `None` = the artboard's own size.
    preview_screen: Option<Vec2>,
    /// The screen facts the last frame's UI layout used (`ctx.screen`, and
    /// the state machine's `screenWidth` / `isCompact` / … inputs).
    last_screen: Option<an1mbyt3_core::ScreenInfo>,
    /// Imported models, parsed and uploaded on first use: the model data
    /// (node tree, clips) and each primitive's mesh + texture.
    model_gpu: HashMap<an1mbyt3_core::ModelId, std::rc::Rc<GpuModel>>,
    /// Per-node animation clocks for model meshes: the clips playing (base,
    /// overlay) and how far into each.
    anim_clocks: HashMap<NodeId, AnimClock>,
    /// Live 3D particles (`ParticleSystem3D` components), see `particles3d`.
    particles3d: crate::particles3d::Particles3D,
    /// Seconds each Flipbook node has been playing this Playground session
    /// (cleared on entering / leaving Playground) — see `apply_flipbooks`.
    flipbook_clocks: HashMap<NodeId, f32>,
    /// The built-in material textured model meshes draw with (Sample
    /// Texture × Tint), compiled once.
    textured_material_3d: Option<std::rc::Rc<an1mbyt3_renderer::GpuMaterial3D>>,
    /// Per model node: script poses (bone overrides, expressions), spring
    /// bones and the deformed meshes of skinned / morphed models (VRM
    /// avatars). Runtime only — reset on entering/leaving Playground.
    model_instances: HashMap<NodeId, ModelInstance>,
    /// Retargetable clips parsed from imported animation files (by asset).
    humanoid_clips: HashMap<an1mbyt3_core::ModelId, std::rc::Rc<Vec<an1mbyt3_geometry::retarget::HumanoidClip>>>,
    /// Ragdolls to create once the scene's physics world exists (made while
    /// drawing, where the avatar's bone poses are known).
    pending_ragdolls: Vec<(NodeId, Vec<an1mbyt3_physics::physics3d::RagdollPart>, glam::Vec3)>,
    /// The two Toon material shaders (MToon, Arcane), compiled once; every
    /// Toon material shares them (its values go per draw).
    toon_gpu: [Option<std::rc::Rc<an1mbyt3_renderer::GpuMaterial3D>>; 2],
    /// Toon materials being dragged in the Inspector: each one's value
    /// before the drag (so the commit's undo goes back to it).
    toon_preview_base: HashMap<an1mbyt3_core::MaterialId, an1mbyt3_core::ToonMaterial>,
    /// Block worlds' runtime blocks and their meshed regions, by the
    /// `Primitive3D::Voxels` node (scripts fill them; see `VoxelWorld`).
    voxel_worlds: HashMap<NodeId, VoxelWorld>,
    /// Breakable glass (`Primitive3D::Glass`): which pieces are still in
    /// place and the flying shards, per pane. Runtime only — cleared on
    /// entering/leaving Playground, so every run starts with whole glass.
    glass: HashMap<NodeId, GlassRuntime>,
    /// The block atlas texture and the material block worlds draw with
    /// (`an1mbyt3_geometry::voxel`), made on first use.
    voxel_atlas_gpu: Option<an1mbyt3_renderer::GpuImageTexture>,
    voxel_material_3d: Option<std::rc::Rc<an1mbyt3_renderer::GpuMaterial3D>>,
    /// A script's 3D environment override (`ctx.setEnvironment3`): fog
    /// colour, fog density and sky ambient. Cleared entering / leaving
    /// Playground, so it never outlives the game that set it.
    environment_3d: Option<([f32; 3], f32, [f32; 3])>,
    /// This frame's `delta_seconds` (advances the animation clocks).
    frame_delta: f32,
    /// Live cursor position in raw canvas-pixel coordinates, updated by
    /// `pointer_listener` — same `Rc<Cell<_>>`, same "only the latest
    /// position ever matters, converted to artboard space fresh every
    /// frame" reasoning as `WasmPlayer::pointer_position`'s own doc comment.
    /// Feeds a graph's `MousePosition` source node (`an1mbyt3_runtime::graph::GraphContext::mouse_artboard`)
    /// while the State Machine or Playground tab is active — see `render`.
    pointer_position: Rc<Cell<(f32, f32)>>,
    /// The `window`-level `mousemove` listener — kept alive for as long as
    /// this editor exists, same reasoning as `WasmPlayer::pointer_listener`'s
    /// own doc comment. Unlike `WasmPlayer`, always attached (not gated by
    /// an opt-in flag): the desktop editor always benefits from a live
    /// `MousePosition` graph node being authorable/previewable, so there's
    /// no "pays nothing for it" case worth preserving here.
    pointer_listener: Closure<dyn FnMut(web_sys::MouseEvent)>,
    /// Live simulation state for every `NodeKind::ParticleSystem` node —
    /// see `an1mbyt3_runtime::particle_sim`'s own module doc for why this
    /// is never saved to the document, same "ephemeral, rebuilt fresh"
    /// category as `state_machine`'s own runtime pools. `render` is the
    /// only place `step_particle_system` is ever called.
    particle_runtimes: HashMap<NodeId, an1mbyt3_runtime::ParticleSystemRuntime>,
    /// The last `render`'s timing breakdown and scene counts (`frame_stats`).
    frame_stats: crate::profiler::FrameStats,
    /// This frame's sub-timings (copied into `frame_stats`).
    sections: crate::profiler::Sections,
    /// A shared unit-circle mesh every live particle, across every
    /// `ParticleSystem` node, draws through — same "one shared mesh, many
    /// per-instance bindings" shape `image_quad_mesh`/`image_bindings`
    /// already establish for `Image` nodes. Built once, lazily, never
    /// invalidated (its geometry never changes).
    particle_mesh: Option<GpuMesh>,
    /// A *pool* of `DrawBinding`s per `ParticleSystem` node — unlike
    /// `image_bindings` (exactly one binding per node), a system can have
    /// many simultaneously-live particles, each needing its own transform/
    /// color uniform slot. Grown (never shrunk) to match
    /// `ParticleSystemRuntime::particles().len()` each frame — a binding
    /// left over from a frame with more live particles than the current one
    /// is simply unused this frame, cheap to keep around for next time the
    /// count grows back rather than freeing and re-allocating GPU resources
    /// every time the live count fluctuates.
    particle_bindings: HashMap<NodeId, Vec<DrawBinding>>,
    /// Particle Look shapes (unit-sized meshes), by shape key — built-in
    /// shapes once, Shape-node outlines per (node, path).
    particle_shape_meshes: HashMap<u64, GpuMesh>,
    /// Which shape mesh each Particle Look system draws with this frame.
    particle_shape_keys: HashMap<NodeId, u64>,
    /// Particle glow: a unit quad and its soft additive halo material.
    particle_glow: Option<(GpuMesh, an1mbyt3_renderer::GpuMaterial)>,
    /// One glow binding per live particle, per system (one binding per draw).
    particle_glow_bindings: HashMap<NodeId, Vec<DrawBinding>>,
    /// Particle Fluid 2D: this frame's liquid meshes per node (world space,
    /// back to front: rim, body, highlight) and the bindings they reuse.
    particle_fluid_gpu: HashMap<NodeId, LiquidGpu>,
    /// Goo 2D: each group's liquid meshes, keyed by the group's first
    /// member (drawn there, in that node's space): (mesh, colour, gradient).
    goo_gpu: HashMap<NodeId, LiquidGpu>,
    /// Where each Goo 2D group's liquid draws: its front-most member (in
    /// draw order, sorting layers included) → the group's owner (first
    /// member, whose settings / space it uses).
    goo_draw_at: HashMap<NodeId, NodeId>,
    /// Goo groups previewed in Design this frame (a member is selected).
    goo_preview: std::collections::HashSet<u32>,
    /// A unit square (−0.5 … 0.5) — the huge quad an inverted Mask 2D
    /// writes into the stencil.
    mask_quad: Option<GpuMesh>,
    /// Nodes some Mask 2D uses as its mask: they (and their subtrees)
    /// don't draw as content.
    mask2d_targets: std::collections::HashSet<NodeId>,
    /// Compiled liquid shaders (`goo_gpu::shader`), keyed by their WGSL —
    /// liquids with the same look share one pipeline.
    liquid_materials: HashMap<String, an1mbyt3_renderer::GpuMaterial>,
    /// Last built input fingerprint per goo owner / fluid node: unchanged
    /// liquids (still text, a paused emitter) skip the rebuild entirely.
    goo_keys: HashMap<NodeId, u64>,
    /// Still Goo 2D members' field patches (see `metaball2d::SourceCache`).
    goo_field_cache: crate::metaball2d::SourceCache,
    fluid_keys: HashMap<NodeId, u64>,
    /// Text members' glyph outlines in the node's own space, keyed by what
    /// they depend on — shaping text every frame was a large share of a
    /// goo group's cost.
    goo_text_cache: std::cell::RefCell<HashMap<NodeId, (String, u32, FontId, Vec<Vec<Vec2>>)>>,
    /// One draw binding per Liquid 2D particle (grown as the fluid grows).
    liquid_bindings: HashMap<NodeId, Vec<DrawBinding>>,
    /// Opaque-pixel bounding box per (image, tile index) — `u32::MAX` as the
    /// tile index means "the whole image". Shadow casters use it so a
    /// sprite casts from its visible pixels, not its padded rectangle
    /// (`WasmEditor::caster_local_bounds`). `RefCell`: filled lazily from
    /// inside `render`'s read-only draw-building phase. `None` = fully
    /// transparent.
    alpha_bounds_cache: std::cell::RefCell<HashMap<(an1mbyt3_core::ImageId, u32), Vec<[u32; 2]>>>,
    /// Each Trail node's recorded point history (`lighting2d::TrailRuntime`)
    /// — live runtime state like `particle_runtimes`, never saved.
    trail_runtimes: HashMap<NodeId, crate::lighting2d::TrailRuntime>,
    /// Each Trail node's GPU side: this frame's ribbon mesh (rebuilt every
    /// frame — the points move), its uniform slot, and its baked gradient
    /// texture (rebaked only when the gradient/texture changes).
    trail_gpu: HashMap<NodeId, TrailGpu>,
    /// Cloth 2D: each cloth's live simulation (Playground only; rebuilt at
    /// rest otherwise) and its GPU meshes / texture — see `ClothGpu`.
    cloth_sims: HashMap<NodeId, an1mbyt3_runtime::ClothSim>,
    cloth_gpu: HashMap<NodeId, ClothGpu>,
    /// Tree 2D: each tree's runtime (skeleton, clock, shake, falling
    /// leaves) and its per-colour meshes, rebuilt every frame (it sways).
    tree_states: HashMap<NodeId, an1mbyt3_runtime::TreeRuntime>,
    tree_gpu: HashMap<NodeId, TreeGpu>,
    /// Live Rigidbody 2D / Collider 2D simulation for the scene Playground
    /// is running — `(scene, world)`. Built on the first Playground frame,
    /// dropped on leaving Playground or switching scenes (stopping
    /// Playground restores every node's designed pose anyway).
    physics: Option<(ArtboardId, an1mbyt3_physics::PhysicsWorld)>,
    /// Physics 3D (Rigidbody 3D / Collider 3D), Playground only — same
    /// lifecycle as `physics`.
    physics3d: Option<(ArtboardId, an1mbyt3_physics::physics3d::Physics3DWorld)>,
}

/// One node's runtime-mutable state, as captured on entering a live mode
/// (see `WasmEditor::snapshot_design_pose`).
/// A whole scene's [`PoseEntry`]s, tagged with the scene they belong to.
struct PoseSnapshot {
    artboard: ArtboardId,
    entries: Vec<PoseEntry>,
}

struct PoseEntry {
    id: NodeId,
    transform: an1mbyt3_core::Transform2D,
    fill_color: Option<[f32; 4]>,
    z_index: f32,
    image: Option<an1mbyt3_core::ImageId>,
    text: Option<String>,
    visible: bool,
    /// A Tilemap's cells (scripts can regenerate a whole map).
    tiles: Option<Vec<Option<u32>>>,
    /// A Light 2D component's `(intensity, outer_radius)`.
    light: Option<(f32, f32)>,
    /// A Rigidbody 2D's body type (a script may switch it for a ragdoll).
    body_type: Option<an1mbyt3_core::RigidbodyType2D>,
    /// A Node3D's transform + content (3D physics, `ref.position3`,
    /// `playAnimation3` all change it).
    node3d: Option<an1mbyt3_core::Node3D>,
    /// Field-path tracks' values (any Inspector value some animation keys).
    fields: Vec<(String, [f32; 4])>,
}

/// A skinned image's rest mesh + weights, for one binding.
struct SkinCache {
    skin: an1mbyt3_core::SpriteSkin2D,
    size: (f32, f32),
    /// The image's grid, in world space at bind time, with UVs.
    rest: Mesh,
    binding: an1mbyt3_geometry::SkinBinding,
}

/// Grids the image (`columns` x `rows` quads), places it where it was when
/// bound, and weights every vertex to its two nearest bones (by distance to
/// each bone's segment at bind time), softened by `smoothness`.
fn build_skin_cache(artboard: &an1mbyt3_core::Artboard, skin: &an1mbyt3_core::SpriteSkin2D, width: f32, height: f32) -> SkinCache {
    let cols = skin.columns.clamp(1, 64) as usize;
    let rows = skin.rows.clamp(1, 64) as usize;
    let bind_node = an1mbyt3_core::array_to_affine(skin.bind_node);
    let mut positions = Vec::with_capacity((cols + 1) * (rows + 1));
    let mut uvs = Vec::with_capacity(positions.capacity());
    for r in 0..=rows {
        for c in 0..=cols {
            let u = c as f32 / cols as f32;
            let v = r as f32 / rows as f32;
            let local = Vec2::new(-width / 2.0 + u * width, height / 2.0 - v * height);
            let world = bind_node.transform_point2(local);
            positions.push([world.x, world.y]);
            uvs.push([u, v]);
        }
    }
    let mut indices = Vec::with_capacity(cols * rows * 6);
    for r in 0..rows {
        for c in 0..cols {
            let i = (r * (cols + 1) + c) as u32;
            let below = i + cols as u32 + 1;
            indices.extend_from_slice(&[i, below, i + 1, i + 1, below, below + 1]);
        }
    }
    // Bone segments at bind time.
    let segments: Vec<(Vec2, Vec2)> = skin
        .bones
        .iter()
        .zip(&skin.bind_poses)
        .map(|(bone, pose)| {
            let length = match artboard.node(*bone).map(|n| &n.kind) {
                Some(NodeKind::Bone(b)) => b.length,
                _ => 0.0,
            };
            let m = an1mbyt3_core::array_to_affine(*pose);
            (m.transform_point2(Vec2::ZERO), m.transform_point2(Vec2::new(length, 0.0)))
        })
        .collect();
    let segment_distance = |p: Vec2, (a, b): (Vec2, Vec2)| {
        let ab = b - a;
        let t = if ab.length_squared() < 1e-6 { 0.0 } else { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) };
        p.distance(a + ab * t)
    };
    let soft = skin.smoothness.max(0.5);
    let weights = positions
        .iter()
        .map(|&[x, y]| {
            let p = Vec2::new(x, y);
            let mut best = [(f32::MAX, 0usize), (f32::MAX, 0usize)];
            for (i, seg) in segments.iter().enumerate() {
                let d = segment_distance(p, *seg);
                if d < best[0].0 {
                    best[1] = best[0];
                    best[0] = (d, i);
                } else if d < best[1].0 {
                    best[1] = (d, i);
                }
            }
            if segments.len() < 2 {
                return an1mbyt3_geometry::VertexWeight { bone_a: 0, bone_b: 0, weight_a: 1.0 };
            }
            // Closer bone dominates; within `soft` px of each other they blend.
            let (da, db) = (best[0].0, best[1].0);
            let weight_a = 0.5 + 0.5 * ((db - da) / soft).clamp(0.0, 1.0);
            an1mbyt3_geometry::VertexWeight { bone_a: best[0].1, bone_b: best[1].1, weight_a }
        })
        .collect();
    let inverse_bind_matrices = skin.bind_poses.iter().map(|pose| an1mbyt3_core::array_to_affine(*pose).inverse()).collect();
    SkinCache {
        skin: skin.clone(),
        size: (width, height),
        rest: Mesh { positions, indices, uvs },
        binding: an1mbyt3_geometry::SkinBinding { weights, inverse_bind_matrices },
    }
}

/// Sets a node's Rigidbody 2D body type in place (no undo).
fn set_body_type(node: &mut Node, body_type: an1mbyt3_core::RigidbodyType2D) {
    for entry in node.behaviors.iter_mut() {
        if let an1mbyt3_core::NodeBehavior::Rigidbody2D(rb) = &mut entry.behavior {
            rb.body_type = body_type;
        }
    }
}

/// `node` and every descendant, depth first (parents before children).
fn subtree(artboard: &an1mbyt3_core::Artboard, node: NodeId) -> Vec<NodeId> {
    let mut out = vec![node];
    let mut i = 0;
    while i < out.len() {
        let children: Vec<NodeId> = artboard.children_of(out[i]).filter(|(_, n)| !n.deleted).map(|(id, _)| id).collect();
        out.extend(children);
        i += 1;
    }
    out
}

/// Andrew's monotone-chain convex hull, counter-clockwise, no duplicates.
fn convex_hull(mut points: Vec<Vec2>) -> Vec<Vec2> {
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let cross = |o: Vec2, a: Vec2, b: Vec2| (a - o).perp_dot(b - o);
    let mut lower: Vec<Vec2> = Vec::new();
    for &p in &points {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Vec2> = Vec::new();
    for &p in points.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// See `WasmEditor::active_camera_frame`.
struct CameraFrame {
    pan: Vec2,
    /// Physical canvas pixels per world unit.
    zoom: f32,
    background: [f32; 4],
    /// `Some(n)`: render the whole frame at 1/n resolution and nearest-
    /// upscale (Pixel Perfect "Upscale Render Texture").
    render_scale: Option<f32>,
    /// `Some(half extent)`: black bars outside `pan ± half` (an infinite
    /// component means no bars on that axis).
    crop: Option<Vec2>,
}

/// See `WasmEditor::cloth_gpu`: the fabric mesh and its four shade
/// overlays (folds, creases, backs, highlights — rebuilt every frame), one draw
/// binding each, and the fabric texture (rebaked when its look changes).
struct ClothGpu {
    meshes: [Option<GpuMesh>; 5],
    bindings: [DrawBinding; 5],
    texture: an1mbyt3_renderer::GpuImageTexture,
    texture_key: ClothTextureKey,
    shading: f32,
}

/// See `WasmEditor::tree_gpu`: one flat-colour mesh per part (bark, bark
/// shadow, dark foliage, light foliage, accents, falling leaves), each with
/// its own draw binding.
struct TreeGpu {
    parts: Vec<(GpuMesh, [f32; 4])>,
    bindings: Vec<DrawBinding>,
    /// What `parts` was built from: the settings, the tree's clock / shake /
    /// falling-leaf count, and the grow amount. While designing none of
    /// these move, so the tree is built once instead of every frame.
    key: Option<(an1mbyt3_core::Tree2D, u32, u32, usize, u32)>,
}

/// What a cloth's texture was baked from.
type ClothTextureKey = ([u32; 4], an1mbyt3_core::ClothPatternKind, [u32; 4], u32, Option<an1mbyt3_core::ImageId>);

fn cloth_texture_key(c: &an1mbyt3_core::Cloth2D) -> ClothTextureKey {
    (c.color.map(f32::to_bits), c.pattern, c.pattern_color.map(f32::to_bits), c.pattern_count, c.image)
}

/// The fabric texture: the image if there is one, else the colour with its
/// pattern and a faint woven texture (so flat cloth doesn't look plastic).
fn bake_cloth_texture(c: &an1mbyt3_core::Cloth2D, image: Option<(Vec<u8>, u32, u32)>) -> (Vec<u8>, u32, u32) {
    if let Some(image) = image {
        return image;
    }
    const SIZE: u32 = 128;
    let to_byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let count = c.pattern_count.max(1) as f32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (u, v) = ((x as f32 + 0.5) / SIZE as f32, (y as f32 + 0.5) / SIZE as f32);
            let second = match c.pattern {
                an1mbyt3_core::ClothPatternKind::Solid => false,
                an1mbyt3_core::ClothPatternKind::HorizontalStripes => (v * count) as u32 % 2 == 1,
                an1mbyt3_core::ClothPatternKind::VerticalStripes => (u * count) as u32 % 2 == 1,
                an1mbyt3_core::ClothPatternKind::Checker => ((u * count) as u32 + (v * count) as u32) % 2 == 1,
            };
            let base = if second { c.pattern_color } else { c.color };
            // Weave: alternating threads a few percent lighter / darker.
            let weave = if (x / 2 + y / 2) % 2 == 0 { 1.04 } else { 0.96 };
            rgba.extend([to_byte(base[0] * weave), to_byte(base[1] * weave), to_byte(base[2] * weave), to_byte(base[3])]);
        }
    }
    (rgba, SIZE, SIZE)
}

/// See `WasmEditor::trail_gpu`.
struct TrailGpu {
    mesh: Option<GpuMesh>,
    binding: DrawBinding,
    texture: an1mbyt3_renderer::GpuImageTexture,
    /// What `texture` was baked from — rebake when this differs.
    texture_key: (Vec<an1mbyt3_core::GradientStop>, Option<an1mbyt3_core::ImageId>),
}

/// See `WasmEditor::artboard_chrome`'s doc comment. Not `#[wasm_bindgen]` —
/// purely an internal rendering cache, nothing crosses the JS boundary.
struct ArtboardChrome {
    width: f32,
    height: f32,
    grid_mesh: GpuMesh,
    grid_binding: DrawBinding,
}

impl Drop for WasmEditor {
    /// See `WasmPlayer`'s own `Drop` impl doc comment for why this explicit
    /// `removeEventListener` is necessary — dropping `pointer_listener`'s
    /// `Closure` alone would leave `window` holding a dangling callback.
    fn drop(&mut self) {
        if let Some(window) = web_sys::window() {
            let _ = window.remove_event_listener_with_callback("mousemove", self.pointer_listener.as_ref().unchecked_ref());
        }
    }
}

#[wasm_bindgen]
impl WasmEditor {
    /// See `WasmPlayer::create`'s doc comment for why this is a plain static
    /// factory (`await WasmEditor.create(canvas)`), not `#[wasm_bindgen(constructor)]`.
    pub async fn create(canvas: HtmlCanvasElement) -> Result<WasmEditor, JsValue> {
        let width = canvas.width();
        let height = canvas.height();

        // Cloned before `canvas` is moved into `create_surface` below —
        // same "cheap JS reference clone, not a DOM element copy" reasoning
        // `WasmPlayer::create`'s own `canvas_for_listener` doc comment gives.
        let canvas_for_listener = canvas.clone();

        // WebGPU when the browser has it, else WebGL2 (decided before the
        // canvas gets a context — a canvas can only ever have one kind).
        let instance = wgpu::util::new_instance_with_webgpu_detection(wgpu::InstanceDescriptor::new_without_display_handle_from_env()).await;
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|err| JsValue::from_str(&format!("failed to create wgpu surface: {err}")))?;

        // A *real* surface-backed renderer, not `new_headless` — this is
        // the whole point: the canvas's own surface is presented to
        // directly, no offscreen texture, no readback, no bridge.
        // Alpha-capable surface (premultiplied), so a transparent export
        // can read frames back with their alpha; every normal frame clears
        // opaque, so the editor looks the same.
        let renderer = Renderer::new_transparent(&instance, surface, width, height).await;
        let (mut document, animation_id) = build_start_document();
        migrate_document(&mut document);
        let artboard_id = document.artboards().next().map(|(id, _)| id).expect("build_start_document always creates one artboard");
        let state_machine = placeholder_state_machine(document.artboard(artboard_id).unwrap(), animation_id);

        let pointer_position = Rc::new(Cell::new((0.0f32, 0.0f32)));
        let pointer_position_for_listener = Rc::clone(&pointer_position);
        let pointer_listener = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |event: web_sys::MouseEvent| {
            let rect = canvas_for_listener.get_bounding_client_rect();
            let canvas_x = event.client_x() as f32 - rect.left() as f32;
            let canvas_y = event.client_y() as f32 - rect.top() as f32;
            pointer_position_for_listener.set((canvas_x, canvas_y));
        });
        web_sys::window()
            .expect("no global `window` exists")
            .add_event_listener_with_callback("mousemove", pointer_listener.as_ref().unchecked_ref())
            .map_err(|err| JsValue::from_str(&format!("failed to attach pointer listener: {err:?}")))?;

        Ok(WasmEditor {
            renderer,
            document,
            mesh_cache: HashMap::new(),
            shape_mesh_source: HashMap::new(),
            stroke_mesh_cache: HashMap::new(),
            stroke_mesh_source: HashMap::new(),
            tilemap_mesh_cache: HashMap::new(),
            skin_meshes: HashMap::new(),
            skin_bindings: HashMap::new(),
            tilemap_mesh_source: HashMap::new(),
            font_cache: HashMap::new(),
            text_mesh_cache: HashMap::new(),
            state_machine,
            state_machine_active: false,
            playground_active: false,
            state_machine_programs_enabled: false,
            active_animation: Some(animation_id),
            active_artboard: None,
            active_state_machine: None,
            scrub_time: 0.0,
            animate_mode_active: false,
            record_keys: false,
            design_pose_snapshot: None,
            runtime_spawned: Vec::new(),
            pending_project_bytes: Vec::new(),
            selected_node: None,
            canvas_width: width,
            canvas_height: height,
            motion_path_dot_mesh: None,
            artboard_chrome: None,
            image_quad_mesh: None,
            image_bindings: HashMap::new(),
            image_cache: HashMap::new(),
            material_gpu: HashMap::new(),
            materials_checked_at: None,
            material3d_checked_at: HashMap::new(),
            shader_clock: 0.0,
            mesh3d_cache: HashMap::new(),
            material_gpu_3d: HashMap::new(),
            view3d_enabled: false,
            view3d_camera: crate::view3d::EditorCamera3D::default(),
            view3d_selected: None,
            multi_transform: None,
            scene_layer_filter: std::cell::Cell::new(None),
            fx_building: std::cell::Cell::new(None),
            reference_building: std::cell::Cell::new(None),
            scene_collect_unlit: std::cell::Cell::new(true),
            effect_bindings: BindingArena::default(),
            effect_camera_pan: std::cell::Cell::new(Vec2::ZERO),
            camera_trauma: HashMap::new(),
            view3d_hot: None,
            view3d_mode: crate::view3d::GizmoMode::Move,
            view3d_bone: None,
            view3d_bone_basis: None,
            last_device_pixel_ratio: 1.0,
            preview_screen: None,
            last_screen: None,
            export_transparent: false,
            exporting: false,
            model_gpu: HashMap::new(),
            anim_clocks: HashMap::new(),
            particles3d: Default::default(),
            flipbook_clocks: HashMap::new(),
            textured_material_3d: None,
            model_instances: HashMap::new(),
            toon_gpu: [None, None],
            pending_ragdolls: Vec::new(),
            humanoid_clips: HashMap::new(),
            toon_preview_base: HashMap::new(),
            voxel_worlds: HashMap::new(),
            glass: HashMap::new(),
            voxel_atlas_gpu: None,
            voxel_material_3d: None,
            environment_3d: None,
            frame_delta: 0.0,
            pointer_position,
            pointer_listener,
            particle_runtimes: HashMap::new(),
            frame_stats: Default::default(),
            sections: Default::default(),
            particle_mesh: None,
            particle_bindings: HashMap::new(),
            particle_shape_meshes: HashMap::new(),
            particle_shape_keys: HashMap::new(),
            particle_glow: None,
            particle_glow_bindings: HashMap::new(),
            particle_fluid_gpu: HashMap::new(),
            goo_gpu: HashMap::new(),
            goo_draw_at: HashMap::new(),
            goo_preview: Default::default(),
            mask_quad: None,
            mask2d_targets: std::collections::HashSet::new(),
            liquid_materials: HashMap::new(),
            goo_keys: HashMap::new(),
            goo_field_cache: Default::default(),
            fluid_keys: HashMap::new(),
            goo_text_cache: Default::default(),
            liquid_bindings: HashMap::new(),
            trail_runtimes: HashMap::new(),
            alpha_bounds_cache: std::cell::RefCell::new(HashMap::new()),
            crop_bars: None,
            trail_gpu: HashMap::new(),
            cloth_sims: HashMap::new(),
            cloth_gpu: HashMap::new(),
            tree_states: HashMap::new(),
            tree_gpu: HashMap::new(),
            physics: None,
            physics3d: None,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
        self.canvas_width = width;
        self.canvas_height = height;
    }

    /// Toggles whether the toolbar's mode is "Animate" — see
    /// `animate_mode_active`'s doc comment for what this gates.
    pub fn set_animate_mode_active(&mut self, active: bool) {
        self.animate_mode_active = active;
    }

    /// The toolbar's mode-switch entry point (`mode`: `"Design"`/`"Animate"`/
    /// `"StateMachine"`) — sets `state_machine_active`/`animate_mode_active`
    /// together (replacing two separate calls, see below for why that
    /// mattered) *and* snapshots/restores every node's transform across the
    /// Design-vs-either-animated-mode boundary — see `design_pose_snapshot`'s
    /// field doc comment for why leaving Animate/State Machine mode needs
    /// this at all.
    ///
    /// Snapshots only on the transition *into* an animated mode from Design
    /// (`design_pose_snapshot` was `None`), and restores only on the
    /// transition *out* to Design (back to `None`) — deliberately not
    /// per-flag, so switching directly from Animate to State Machine (no
    /// Design in between, both calls this replaces used to fire back to
    /// back) can't snapshot the *already-animated* pose in between a
    /// restore-then-fresh-snapshot pair. Both mutations write `Node::transform`
    /// directly, bypassing `Command`/undo — same "session/view state, not a
    /// user edit" convention `Animation::apply`/`StateMachine::apply`
    /// themselves already use, deliberately not adding a spurious undo step
    /// just for switching tabs.
    /// Every node's current runtime-mutable state — `design_pose_snapshot`'s
    /// capture. Covers everything a Playground script can change through its
    /// untracked `preview_*` writes (transform, fill/light color, z-index,
    /// an Image's frame, a Text's string), so stopping Playground puts the
    /// scene back exactly as it was designed.
    fn snapshot_design_pose(&self, artboard_id: ArtboardId) -> Option<PoseSnapshot> {
        let artboard_ref = self.document.artboard(artboard_id)?;
        // Every (node, field path) some animation here keys.
        let mut field_tracks: HashMap<NodeId, Vec<String>> = HashMap::new();
        for (_, anim) in artboard_ref.animations() {
            for (n, track) in &anim.tracks {
                if let PropertyTrack::Field { path, .. } = track {
                    let paths = field_tracks.entry(*n).or_default();
                    if !paths.contains(path) {
                        paths.push(path.clone());
                    }
                }
            }
        }
        Some(PoseSnapshot {
            artboard: artboard_id,
            entries: artboard_ref
                .node_ids()
                .filter_map(|id| {
                    artboard_ref.node(id).map(|n| PoseEntry {
                        id,
                        transform: n.transform,
                        fill_color: node_fill_color(n),
                        z_index: n.z_index,
                        image: match &n.kind {
                            NodeKind::Image(image) => Some(image.image),
                            _ => None,
                        },
                        text: match &n.kind {
                            NodeKind::Text(text) => Some(text.content.clone()),
                            _ => None,
                        },
                        visible: n.visible,
                        tiles: match &n.kind {
                            NodeKind::Tilemap(tilemap) => Some(tilemap.tiles.clone()),
                            _ => None,
                        },
                        light: n.light2d().map(|l| (l.intensity, l.outer_radius)),
                        body_type: n.rigidbody().map(|rb| rb.body_type),
                        node3d: match &n.kind {
                            NodeKind::Node3D(n3) => Some(*n3),
                            _ => None,
                        },
                        fields: field_tracks.get(&id).map(|paths| paths.iter().filter_map(|p| an1mbyt3_core::field_path::read(n, p).map(|v| (p.clone(), v))).collect()).unwrap_or_default(),
                    })
                })
                .collect(),
        })
    }

    /// Writes a `snapshot_design_pose` capture back. `only_keyed`: restore
    /// just the nodes some animation actually has a track for (the only
    /// nodes Animate/State Machine ever move) — leaving every other node's
    /// current pose alone, so plain Design edits made while those tabs were
    /// open survive. See `set_mode`'s own comment for when each applies.
    fn restore_design_pose(&mut self, snapshot: PoseSnapshot, only_keyed: bool) {
        let PoseSnapshot { artboard: artboard_id, entries: snapshot } = snapshot;
        // Which properties each node has tracks for (any animation).
        let mut keyed: HashMap<NodeId, std::collections::HashSet<PropertyKind>> = HashMap::new();
        if only_keyed {
            if let Some(a) = self.document.artboard(artboard_id) {
                for (_, anim) in a.animations() {
                    for (n, track) in &anim.tracks {
                        keyed.entry(*n).or_default().insert(track.kind());
                    }
                }
            }
        }
        let Some(artboard_mut) = self.document.artboard_mut(artboard_id) else { return };
        for entry in snapshot {
            if only_keyed {
                // Animate / State Machine only ever move *animated*
                // properties — put back just those, so every other edit
                // made while those tabs were open (a material, a component,
                // an untracked 3D transform …) is kept.
                let Some(kinds) = keyed.get(&entry.id) else { continue };
                let Some(node) = artboard_mut.node_mut(entry.id) else { continue };
                if kinds.contains(&PropertyKind::Translation) || kinds.contains(&PropertyKind::MotionPath) {
                    node.transform.translation = entry.transform.translation;
                }
                if kinds.contains(&PropertyKind::Rotation) {
                    node.transform.rotation = entry.transform.rotation;
                }
                if kinds.contains(&PropertyKind::Scale) {
                    node.transform.scale = entry.transform.scale;
                }
                if let (true, Some(color)) = (kinds.contains(&PropertyKind::FillColor), entry.fill_color) {
                    set_node_fill_color(node, color);
                }
                for (path, v) in entry.fields {
                    an1mbyt3_core::field_path::write(node, &path, v);
                }
                continue;
            }
            if let Some(node) = artboard_mut.node_mut(entry.id) {
                node.transform = entry.transform;
                node.z_index = entry.z_index;
                node.visible = entry.visible;
                if let (NodeKind::Tilemap(tilemap), Some(tiles)) = (&mut node.kind, entry.tiles) {
                    if tiles.len() == tilemap.tiles.len() {
                        tilemap.tiles = tiles;
                    }
                }
                if let (Some(light), Some((intensity, outer_radius))) = (node.light2d_mut(), entry.light) {
                    light.intensity = intensity;
                    light.outer_radius = outer_radius;
                }
                if let Some(body_type) = entry.body_type {
                    set_body_type(node, body_type);
                }
                if let Some(color) = entry.fill_color {
                    set_node_fill_color(node, color);
                }
                match (&mut node.kind, entry.image, entry.text) {
                    (NodeKind::Image(image), Some(id), _) => image.image = id,
                    (NodeKind::Text(text), _, Some(content)) => text.content = content,
                    _ => {}
                }
                if let (NodeKind::Node3D(n3), Some(saved)) = (&mut node.kind, entry.node3d) {
                    *n3 = saved;
                }
                for (path, v) in entry.fields {
                    an1mbyt3_core::field_path::write(node, &path, v);
                }
            }
        }
    }

    pub fn set_mode(&mut self, artboard: JsValue, mode: &str) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let is_state_machine = mode == "StateMachine";
        let is_animate = mode == "Animate";
        let is_playground = mode == "Playground";
        let will_be_active = is_state_machine || is_animate || is_playground;
        let was_active = self.design_pose_snapshot.is_some();
        let entering_playground = is_playground && !self.playground_active;
        let leaving_playground = !is_playground && self.playground_active;

        // Two different kinds of "live" pose need two different restores:
        //
        // - Animate / State Machine (now just bottom-dock *tabs*, routinely
        //   open while designing — not a separate mode the author leaves
        //   before editing) only ever drive *keyed* nodes (ones with a track
        //   in some animation). Restoring every node on leaving them silently
        //   reverted ordinary Design edits made while the Timeline happened to
        //   be open (a real bug: move a shape, switch to Paint and back, the
        //   move was gone). So these restore keyed nodes only.
        // - Playground runs scripts that can move *any* node, so leaving it
        //   restores everything — and it gets its own fresh snapshot on the
        //   way in, so edits made in Design/Animate since the Animate-tab
        //   snapshot was taken aren't rolled back by Playground's restore.
        if entering_playground || leaving_playground {
            self.flipbook_clocks.clear();
            self.environment_3d = None;
            // Cloth starts hanging at rest every time play starts / stops,
            // and trees start their clocks (and grow-in) over.
            self.cloth_sims.clear();
            self.tree_states.clear();
        }
        if entering_playground {
            self.runtime_spawned.clear();
            if let Some(snapshot) = self.design_pose_snapshot.take() {
                self.restore_design_pose(snapshot, true);
            }
            self.design_pose_snapshot = self.snapshot_design_pose(artboard_id);
        } else if leaving_playground {
            self.despawn_runtime_spawned();
            if let Some(snapshot) = self.design_pose_snapshot.take() {
                self.restore_design_pose(snapshot, false);
            }
            if will_be_active {
                self.design_pose_snapshot = self.snapshot_design_pose(artboard_id);
            }
        } else if will_be_active && !was_active {
            self.design_pose_snapshot = self.snapshot_design_pose(artboard_id);
        } else if !will_be_active && was_active {
            if let Some(snapshot) = self.design_pose_snapshot.take() {
                self.restore_design_pose(snapshot, true);
            }
        }

        self.state_machine_active = is_state_machine;
        self.animate_mode_active = is_animate;
        if is_playground != self.playground_active {
            // Whole glass again, every run (and back in Design); models
            // back in their authored pose.
            self.glass.clear();
            self.model_instances.clear();
        }
        self.playground_active = is_playground;
        self.document_summary()
    }

    /// The State Machine tab's own opt-in toggle for running a Programmable
    /// State's `program` — see `state_machine_programs_enabled`'s own field
    /// doc comment for why this defaults `false` and what it's for.
    pub fn set_state_machine_programs_enabled(&mut self, enabled: bool) {
        self.state_machine_programs_enabled = enabled;
    }

    /// Tracks the Layers panel/canvas's current selection, purely so
    /// `render` can draw the selected node's motion-path overlay — pass
    /// `JsValue::NULL`/`undefined` to clear it (deselection).
    pub fn set_selected_node(&mut self, node: JsValue) -> Result<(), JsValue> {
        self.selected_node = if node.is_null() || node.is_undefined() { None } else { Some(parse_id(node)?) };
        Ok(())
    }

    /// Renders the document's first artboard directly to the canvas's own
    /// surface. `delta_seconds` (wall-clock time since the previous frame,
    /// same convention `WasmPlayer::render` already established) only
    /// matters while `state_machine_active` is set — see
    /// `set_state_machine_active`'s doc comment — otherwise the state
    /// machine simply never advances, and this renders whatever the
    /// document's own (Design-mode-edited) transforms say, unchanged from
    /// before this method took a `delta_seconds` parameter at all.
    ///
    /// `camera_pan_x`/`camera_pan_y`/`camera_zoom` build this frame's
    /// `an1mbyt3_renderer::Camera` — the frontend's own pan/zoom state
    /// (`App.tsx`), passed in fresh every frame the same way `delta_seconds`
    /// already is, rather than tracked as separate mutable session state
    /// here (there's nothing to keep in sync since the frontend already
    /// needs its own copy for hit-testing math).
    ///
    /// Ensures a cached `GpuMesh` exists for every shape node it sees
    /// (tessellating lazily, same as every other mesh cache in this
    /// workspace), then draws — plus, if a node is selected
    /// (`set_selected_node`) and it has an authored `Translation` track in
    /// the active animation, a motion-path overlay: a small dot at every
    /// keyframe's world position, connected by a thin line, so dragging the
    /// node (which — see `apply_transform_edit`'s doc comment — rebases the
    /// whole path in Design mode) visibly shows the path shifting along with it.
    /// `playback_scrub_time` lets Timeline playback resample in this exact
    /// call, the same way `state_machine_active` resamples in this exact
    /// call instead of via a separate prior wasm call — pass a negative
    /// value (the frontend's `tick()` returns `null`/`undefined` when not
    /// playing) to mean "no playback update this frame, just draw the
    /// document as it stands." A real, non-negative time sets
    /// `self.scrub_time` and resamples before any draw call is built, so
    /// there is never a frame where the mutation has happened but the draw
    /// still reflects the previous pose (or vice versa) — the two literally
    /// cannot be observed apart since they happen in the same `&mut self`
    /// call. `set_scrub_time` (manual drag of the ruler/playhead) is
    /// unaffected and still resamples immediately on its own, since that
    /// call doesn't go through `render()` at all.
    /// `device_pixel_ratio` exists purely so the `MousePosition` graph
    /// source can convert `self.pointer_position` (CSS/logical pixels,
    /// `create()`'s `mousemove` listener's own unit — see that field's doc
    /// comment) back out of `camera_zoom`/`self.canvas_width`/`canvas_height`'s
    /// physical-pixel space (both *already* multiplied by this same ratio
    /// by the caller, for the renderer's own projection — see this
    /// parameter list's own `camera_zoom` doc history). Passing the ratio
    /// itself, rather than a second already-divided logical zoom/size, is
    /// the one value that lets this method undo that scaling itself
    /// without the caller needing to compute and pass yet another
    /// derived pair — `apps/editor/src/camera.ts`'s `screenToWorld` is the
    /// exact same formula this mirrors, just against the frontend's own
    /// already-logical `camera`/`canvasSize`.
    #[allow(clippy::too_many_arguments)]
    /// Whether particles, Particle Fluid 2D and Goo 2D run and draw: only in
    /// Playground (and the State Machine tab's live preview). While
    /// designing they're skipped entirely, so Design stays light; goo
    /// members show as their plain shapes there.
    fn effects_live(&self) -> bool {
        self.playground_active || self.state_machine_active || self.exporting
    }

    pub fn render(
        &mut self,
        delta_seconds: f32,
        camera_pan_x: f32,
        camera_pan_y: f32,
        camera_zoom: f32,
        playback_scrub_time: f32,
        device_pixel_ratio: f32,
        clear_r: f32,
        clear_g: f32,
        clear_b: f32,
        grid_r: f32,
        grid_g: f32,
        grid_b: f32,
    ) -> Result<(), JsValue> {
        let artboard_id = self.current_artboard_id().ok_or_else(|| JsValue::from_str("WasmEditor::render: document has no artboards"))?;
        let mut stopwatch = crate::profiler::Stopwatch::start();
        let mut stats = crate::profiler::FrameStats::default();

        // In Playground with an active camera, the camera — not the editor's
        // own pan/zoom — decides what's seen, every frame (`active_camera_view`'s
        // own doc comment). Shadowed here (previous frame's camera position)
        // so this frame's mouse-to-world conversion below already agrees with
        // what's on screen; re-resolved again after this frame's follow step
        // for the actual draw.
        let (camera_pan_x, camera_pan_y, camera_zoom) = match self.active_camera_view(artboard_id) {
            Some((pan, zoom, _)) => (pan.x, pan.y, zoom),
            None => (camera_pan_x, camera_pan_y, camera_zoom),
        };
        if device_pixel_ratio > 0.0 {
            self.last_device_pixel_ratio = device_pixel_ratio;
        }
        self.frame_delta = delta_seconds.clamp(0.0, 0.25);

        if self.state_machine_active || self.playground_active {
            // Same canvas-pixel -> artboard-space conversion as
            // `apps/editor/src/camera.ts`'s `screenToWorld` (and
            // `WasmPlayer::render`'s own pointer-tracking block for the
            // no-camera case) — see this struct's `pointer_position` doc
            // comment for why raw CSS-pixel coordinates are stored and
            // converted fresh here rather than at listener time. Uses the
            // canvas's own logical (CSS-pixel) size/zoom, not the physical,
            // already-`device_pixel_ratio`-scaled `self.canvas_width`/
            // `camera_zoom` the renderer's projection itself needs — mixing
            // the two would silently double-apply the device pixel ratio,
            // scaling the mouse offset wrong on any HiDPI display (this
            // method's own doc comment explains why `device_pixel_ratio` is
            // passed in at all rather than assumed to be `1`).
            let ratio = if device_pixel_ratio > 0.0 { device_pixel_ratio } else { 1.0 };
            let logical_width = self.canvas_width as f32 / ratio;
            let logical_height = self.canvas_height as f32 / ratio;
            let logical_zoom = camera_zoom / ratio;
            let (canvas_x, canvas_y) = self.pointer_position.get();
            let mouse_artboard = Vec2::new(
                camera_pan_x + (canvas_x - logical_width / 2.0) / logical_zoom,
                camera_pan_y - (canvas_y - logical_height / 2.0) / logical_zoom,
            );
            self.state_machine.set_mouse_position(mouse_artboard);

            // Playground always runs programs; the State Machine tab only
            // runs them if its own toggle is on — see `playground_active`/
            // `state_machine_programs_enabled`'s field doc comments.
            self.state_machine.set_programs_enabled(self.playground_active || self.state_machine_programs_enabled);

            // Layout-driven conditions: any state machine can read these
            // (declare inputs with these names to use them in transitions).
            if let Some(screen) = self.last_screen {
                let inputs = &mut self.state_machine.inputs;
                inputs.set_number("screenWidth", screen.screen.x);
                inputs.set_number("screenHeight", screen.screen.y);
                inputs.set_bool("isCompact", screen.compact);
                inputs.set_bool("isPortrait", screen.portrait);
            }

            // Only while a live (not deleted) state machine is active: the
            // runtime still holds the last one built, which would otherwise
            // keep posing nodes after that machine was deleted.
            let artboard_ref = self.document.artboard(artboard_id).unwrap();
            let live = self.active_state_machine.and_then(|id| artboard_ref.state_machine(id)).is_some_and(|def| !def.deleted);
            if live {
                self.state_machine.advance(delta_seconds, artboard_ref);
                let artboard_mut = self.document.artboard_mut(artboard_id).unwrap();
                self.state_machine.apply(artboard_mut);
            }
        }
        stats.logic_ms = stopwatch.lap();

        // Live IK (IK Constraint 2D): after animation posed the bones,
        // before physics — so kinematic ragdoll limbs follow the solved pose.
        if let Some(artboard_mut) = self.document.artboard_mut(artboard_id) {
            an1mbyt3_constraints::solve_ik_constraints(artboard_mut);
        }
        stats.ik_ms = stopwatch.lap();

        // After this frame's scripts moved the follow target (the player),
        // not before — otherwise the camera always trails one frame behind.
        // Physics 2D (Playground only): rebuild if the set of physics
        // objects changed, push script moves in, step, write bodies out.
        if self.playground_active {
            if self.physics.as_ref().is_none_or(|(scene, _)| *scene != artboard_id) {
                self.physics = Some((artboard_id, an1mbyt3_physics::PhysicsWorld::new()));
            }
            if let (Some((_, world)), Some(artboard_mut)) = (self.physics.as_mut(), self.document.artboard_mut(artboard_id)) {
                world.sync(artboard_mut);
                world.step(artboard_mut, delta_seconds);
            }
        } else if self.physics.is_some() {
            self.physics = None;
        self.physics3d = None;
        }
        // Physics 3D, the same way (only for scenes that use it).
        let ragdolls_live = self.model_instances.values().any(|i| i.ragdoll_bones.is_some());
        if self.playground_active && (ragdolls_live || self.document.artboard(artboard_id).is_some_and(an1mbyt3_physics::physics3d::Physics3DWorld::scene_has_physics)) {
            if self.physics3d.as_ref().is_none_or(|(scene, _)| *scene != artboard_id) {
                self.physics3d = Some((artboard_id, an1mbyt3_physics::physics3d::Physics3DWorld::new()));
            }
            let model_gpu = &self.model_gpu;
            let bounds = |m: an1mbyt3_core::ModelId| model_gpu.get(&m).map(|g| g.data.bounds);
            if let (Some((_, world)), Some(artboard_mut)) = (self.physics3d.as_mut(), self.document.artboard_mut(artboard_id)) {
                world.sync(artboard_mut, &bounds);
                world.step(artboard_mut, delta_seconds);
            }
        } else if self.physics3d.is_some() {
            self.physics3d = None;
        }
        stats.physics_ms = stopwatch.lap();
        stats.bodies = self.physics.as_ref().map_or(0, |(_, world)| world.body_count() as u32) + self.physics3d.as_ref().map_or(0, |(_, world)| world.body_count() as u32);
        self.step_camera_follow(artboard_id, delta_seconds);
        let (camera_pan_x, camera_pan_y, camera_zoom, clear_r, clear_g, clear_b, grid_r, grid_g, grid_b) =
            match self.active_camera_view(artboard_id) {
                // Grid drawn in the background color itself — Playground
                // through a camera is "the game view," no editor grid.
                Some((pan, zoom, bg)) => (pan.x, pan.y, zoom, bg[0], bg[1], bg[2], bg[0], bg[1], bg[2]),
                None => (camera_pan_x, camera_pan_y, camera_zoom, clear_r, clear_g, clear_b, grid_r, grid_g, grid_b),
            };

        if playback_scrub_time >= 0.0 {
            self.scrub_time = playback_scrub_time;
            self.resample_active_animation(artboard_id);
        }

        // Shader-graph materials: (re)compile any whose graph changed, drop
        // deleted ones, and advance their clock.
        self.sync_materials();
        self.shader_clock += delta_seconds.max(0.0);
        self.renderer.set_shader_time(self.shader_clock);
        if let Some(a) = self.document.artboard(artboard_id) {
            self.mask2d_targets = a.node_ids().filter_map(|id| a.node(id).and_then(|n| n.mask2d()).and_then(|m| m.mask)).collect();
        }
        if self.mask_quad.is_none() {
            self.mask_quad = Some(self.renderer.upload_mesh(&tessellate_path(&Path::rect(1.0, 1.0))));
        }
        // Draw-effect copies hand out fresh bindings each frame; Camera
        // Shake trauma settles.
        self.effect_bindings.reset();
        if !self.camera_trauma.is_empty() {
            let dt = delta_seconds.max(0.0);
            let recovery: HashMap<NodeId, f32> = self
                .camera_trauma
                .keys()
                .map(|id| (*id, self.document.artboard(artboard_id).and_then(|a| a.node(*id)).and_then(|n| n.camera_shake()).map_or(0.5, |s| s.recovery.max(0.01))))
                .collect();
            self.camera_trauma.retain(|id, t| {
                *t -= dt / recovery[id];
                *t > 0.0
            });
        }
        // Parallax follows the scene camera.
        if let Some(a) = self.document.artboard(artboard_id) {
            let pan = a.active_camera().and_then(|c| a.try_world_transform(c)).map_or(Vec2::ZERO, |t| t.translation);
            self.effect_camera_pan.set(pan);
        }
        // 3D: gather this frame's Node3Ds for the renderer's 3D pass.
        let t3d = crate::profiler::now_ms();
        self.collect_scene_3d(artboard_id);
        self.sections.scene3d_ms = crate::profiler::now_ms() - t3d;
        // UI: anchors and stacks position their nodes for this frame.
        self.apply_ui_layout(artboard_id);
        // Flipbooks: show this frame's child.
        self.apply_flipbooks(artboard_id);

        let artboard = self.document.artboard(artboard_id).unwrap();
        // Deliberately *not* `artboard.shapes()` — that iterator excludes
        // any node currently serving as some `Group`'s mask (correct for its
        // own callers, which draw a flat list with no masking support of
        // their own and must never paint a mask shape as if it were regular
        // content). But `build_scene_ops`/`node_draws` below still needs a
        // mask node's mesh *cached* to actually write it into the stencil
        // buffer (`SceneOp::WriteMask`) — skipping mask nodes here starved
        // every masked layer of its clip entirely: `node_draws` returned an
        // empty `Vec` for every mask (no cache entry), so `WriteMask` never
        // fired, the stencil buffer stayed all-zero, and every `Equal`
        // stencil test at depth ≥ 1 failed, hiding all masked content. Mask
        // nodes still never render as their own visible content —
        // `build_scene_ops`'s own `is_mask_target` check (separate from this
        // cache population) still excludes that.
        for node_id in artboard.node_ids() {
            if artboard.is_effectively_deleted(node_id) {
                continue;
            }
            let node = artboard.node(node_id).expect("Artboard::node_ids() returned an id not in its own arena");
            let NodeKind::Shape(shape) = &node.kind else { continue };
            if !self.mesh_cache.contains_key(&node_id) || self.shape_mesh_source.get(&node_id) != Some(&shape.path) {
                let mesh = tessellate_path(&shape.path);
                // A degenerate path (zero enclosed area — an open/unclosed
                // path, or one with fewer than 3 effective points) tessellates
                // to an empty `Mesh`, and `wgpu` panics ("buffer slice can not
                // be empty") on an empty vertex/index buffer upload. Rather
                // than crash the whole render loop over one bad shape (a real
                // case: a malformed/still-being-edited path, or an imported
                // file with a stroke-only path an importer can't represent),
                // leave it uncached; the draws-building loop below skips any
                // shape with no cache entry instead of drawing
                // nothing-shaped geometry.
                if mesh.positions.is_empty() || mesh.indices.is_empty() {
                    self.mesh_cache.remove(&node_id);
                    self.shape_mesh_source.remove(&node_id);
                } else {
                let gpu_mesh = self.renderer.upload_mesh(&mesh);
                // Reuse the existing `DrawBinding` (its own uniform buffer)
                // across a re-tessellation — only a brand-new node needs a
                // freshly allocated one. Recreating it every time a `Path`
                // track ticks this shape's geometry (every frame, while
                // playing) would leak one GPU uniform buffer per frame.
                let binding = match self.mesh_cache.remove(&node_id) {
                    Some((_, existing_binding)) => existing_binding,
                    None => self.renderer.create_draw_binding(),
                };
                self.mesh_cache.insert(node_id, (gpu_mesh, binding));
                self.shape_mesh_source.insert(node_id, shape.path.clone());
                }
            }
            // An empty fill (an open, zero-area path such as a straight
            // line) must not skip the stroke below: stroke-only lines are
            // exactly what trim-path draw-ons are made of.

            // A shape's stroke is a second, independently cached mesh (see
            // `stroke_mesh_cache`'s own doc comment) — only geometry
            // (path/width/cap/join) invalidates it; the stroke's paint
            // (color/gradient/image) is read fresh from `shape.stroke` every
            // frame by `node_draws`, same as the fill's own paint fields.
            match &shape.stroke {
                Some(stroke) => {
                    // Stroke Style: dashes / Trim Path reshape the stroked path.
                    let stroked = match node.stroke_style() {
                        Some(style) => style_stroke_path(&shape.path, style),
                        None => shape.path.clone(),
                    };
                    let source_key = (stroked.clone(), stroke.width, stroke.cap, stroke.join);
                    if self.stroke_mesh_source.get(&node_id) != Some(&source_key) {
                        let style = StrokeStyle {
                            width: stroke.width,
                            cap: match stroke.cap {
                                an1mbyt3_core::StrokeCap::Butt => StrokeCap::Butt,
                                an1mbyt3_core::StrokeCap::Square => StrokeCap::Square,
                                an1mbyt3_core::StrokeCap::Round => StrokeCap::Round,
                            },
                            join: match stroke.join {
                                an1mbyt3_core::StrokeJoin::Miter => StrokeJoin::Miter,
                                an1mbyt3_core::StrokeJoin::Round => StrokeJoin::Round,
                                an1mbyt3_core::StrokeJoin::Bevel => StrokeJoin::Bevel,
                            },
                        };
                        let mesh = tessellate_stroke(&stroked, &style);
                        if mesh.positions.is_empty() || mesh.indices.is_empty() {
                            self.stroke_mesh_cache.remove(&node_id);
                            self.stroke_mesh_source.remove(&node_id);
                        } else {
                            let gpu_mesh = self.renderer.upload_mesh(&mesh);
                            let binding = match self.stroke_mesh_cache.remove(&node_id) {
                                Some((_, existing_binding)) => existing_binding,
                                None => self.renderer.create_draw_binding(),
                            };
                            self.stroke_mesh_cache.insert(node_id, (gpu_mesh, binding));
                            self.stroke_mesh_source.insert(node_id, source_key);
                        }
                    }
                }
                None => {
                    self.stroke_mesh_cache.remove(&node_id);
                    self.stroke_mesh_source.remove(&node_id);
                }
            }
        }

        // Same lazy-populate-a-cache shape as the shapes loop above, for
        // `NodeKind::Instance` — tessellates the *referenced component
        // definition's* path (not anything stored on the instance node
        // itself, which has no path of its own), shared into the same
        // `mesh_cache` `shapes()` populates (keyed by `NodeId`, so an
        // instance's own node id is a distinct, non-colliding key even
        // though its geometry comes from elsewhere). Two instances of the
        // same component each get their own separate `GpuMesh` upload —
        // simple, if not memory-optimal; revisit only if profiling ever
        // shows many-instances-of-one-component scenes actually mattering.
        let artboard = self.document.artboard(artboard_id).unwrap();
        for node_id in artboard.node_ids() {
            let node = artboard.node(node_id).expect("Artboard::node_ids() returned an id not in its own arena");
            let NodeKind::Instance(instance) = &node.kind else { continue };
            if node.deleted || self.mesh_cache.contains_key(&node_id) {
                continue;
            }
            let Some(definition) = self.document.component(instance.component) else { continue };
            let mesh = tessellate_path(&definition.path);
            let gpu_mesh = self.renderer.upload_mesh(&mesh);
            let binding = self.renderer.create_draw_binding();
            self.mesh_cache.insert(node_id, (gpu_mesh, binding));
        }

        // Same lazy-populate-a-cache shape as the shapes loop above, for
        // `NodeKind::Tilemap` — batches the whole grid into one mesh
        // (`build_tilemap_mesh`), re-rebuilt only when `tiles` itself
        // changed (a `Command::SetTile` paint edit), never per frame.
        for node_id in artboard.node_ids() {
            if artboard.is_effectively_deleted(node_id) {
                continue;
            }
            let node = artboard.node(node_id).expect("Artboard::node_ids() returned an id not in its own arena");
            let NodeKind::Tilemap(tilemap) = &node.kind else { continue };
            if !self.tilemap_mesh_cache.contains_key(&node_id) || self.tilemap_mesh_source.get(&node_id) != Some(&tilemap.tiles) {
                let mesh = build_tilemap_mesh(tilemap);
                if mesh.positions.is_empty() || mesh.indices.is_empty() {
                    self.tilemap_mesh_cache.remove(&node_id);
                    self.tilemap_mesh_source.remove(&node_id);
                    continue;
                }
                let gpu_mesh = self.renderer.upload_mesh(&mesh);
                let binding = match self.tilemap_mesh_cache.remove(&node_id) {
                    Some((_, existing_binding)) => existing_binding,
                    None => self.renderer.create_draw_binding(),
                };
                self.tilemap_mesh_cache.insert(node_id, (gpu_mesh, binding));
                self.tilemap_mesh_source.insert(node_id, tilemap.tiles.clone());
            }
        }

        // Sprite Skin 2D: deform each skinned image by its bones' *current*
        // world matrices (bones move every frame, so this re-uploads).
        let skinned: Vec<(NodeId, an1mbyt3_core::SpriteSkin2D, f32, f32)> = artboard
            .node_ids()
            .filter(|id| !artboard.is_effectively_deleted(*id))
            .filter_map(|id| {
                let node = artboard.node(id)?;
                let NodeKind::Image(image) = &node.kind else { return None };
                let skin = node.sprite_skin()?;
                (!skin.bones.is_empty() && skin.bones.len() == skin.bind_poses.len()).then(|| (id, skin.clone(), image.width, image.height))
            })
            .collect();
        let live: std::collections::HashSet<NodeId> = skinned.iter().map(|(id, ..)| *id).collect();
        self.skin_meshes.retain(|id, _| live.contains(id));
        self.skin_bindings.retain(|id, _| live.contains(id));
        for (node_id, skin, width, height) in skinned {
            let fresh = self.skin_bindings.get(&node_id).is_none_or(|c| c.skin != skin || c.size != (width, height));
            if fresh {
                self.skin_bindings.insert(node_id, build_skin_cache(artboard, &skin, width, height));
            }
            let cache = &self.skin_bindings[&node_id];
            let bones: Vec<glam::Affine2> = skin.bones.iter().map(|b| artboard.try_world_transform(*b).map_or(glam::Affine2::IDENTITY, |t| t.to_affine2())).collect();
            let mesh = an1mbyt3_geometry::skin_mesh(&cache.rest, &cache.binding, &bones);
            let gpu_mesh = self.renderer.upload_mesh(&mesh);
            let binding = match self.skin_meshes.remove(&node_id) {
                Some((_, existing)) => existing,
                None => self.renderer.create_draw_binding(),
            };
            self.skin_meshes.insert(node_id, (gpu_mesh, binding));
        }

        // Same lazy-populate-a-cache shape as the shapes loop above, for
        // `NodeKind::Text` — a document opened from a `.tea`/`.teab` file
        // that contains a `Text` node (its `FontAsset` bytes travel with the
        // document, see `FontAsset`'s doc comment) renders it for real here,
        // not just in `an1mbyt3-text`'s standalone example.
        let artboard = self.document.artboard(artboard_id).unwrap();
        // Not `artboard.texts()` — same reasoning as the shapes loop above:
        // a `Text` node can be a `Group`'s mask too (`GroupNode::mask` is a
        // bare `NodeId`, not restricted to `Shape`), and excluding it here
        // would starve it of a cached mesh the same way.
        for node_id in artboard.node_ids() {
            if artboard.is_effectively_deleted(node_id) {
                continue;
            }
            let node = artboard.node(node_id).expect("Artboard::node_ids() returned an id not in its own arena");
            let NodeKind::Text(text) = &node.kind else { continue };

            if !self.font_cache.contains_key(&text.font) {
                let font_asset = self
                    .document
                    .font(text.font)
                    .expect("TextNode::font must reference a FontAsset registered on the same Document");
                let font = Font::from_bytes(font_asset.data.clone(), 0)
                    .expect("TextNode font bytes must be a valid font file");
                crate::text_bounds::register_font(text.font, &font_asset.data);
                self.font_cache.insert(text.font, font);
            }

            let key = (text.content.clone(), text.size_px, text.font);
            let needs_reshape = self.text_mesh_cache.get(&node_id).map(|cached| cached.key != key).unwrap_or(true);
            if needs_reshape {
                let font = self.font_cache.get(&text.font).unwrap();
                let shaped = shape_and_tessellate(font, &text.content, text.size_px);
                let recenter = Vec2::new(-shaped.advance.x / 2.0, 0.0);
                // A whitespace glyph (a space, most commonly — any run with
                // more than one word hits this) has no outline at all, so
                // `tessellate_path` hands back a mesh with zero vertices —
                // `Renderer::upload_mesh` still happily builds a (zero-byte)
                // GPU buffer for it, but actually *drawing* that buffer later
                // (`vertex_buffer.slice(..)`) panics wgpu-side with "buffer
                // slice can not be empty." Skipping an empty mesh here is
                // correct either way: it has nothing to draw regardless of
                // whether the GPU upload would have survived.
                let mut min = Vec2::splat(f32::INFINITY);
                let mut max = Vec2::splat(f32::NEG_INFINITY);
                for g in &shaped.glyphs {
                    for p in &g.mesh.positions {
                        let p = Vec2::from(*p) + g.offset + recenter;
                        min = min.min(p);
                        max = max.max(p);
                    }
                }
                // A blank run still occupies its advance.
                let bounds = if min.x <= max.x { (min, max) } else { (Vec2::new(-shaped.advance.x / 2.0, 0.0), Vec2::new(shaped.advance.x / 2.0, text.size_px)) };
                let glyphs = shaped
                    .glyphs
                    .into_iter()
                    .filter(|g| !g.mesh.indices.is_empty())
                    .map(|g| (self.renderer.upload_mesh(&g.mesh), self.renderer.create_draw_binding(), g.offset + recenter))
                    .collect();
                crate::text_bounds::record(&text.content, text.size_px, text.font, bounds);
                self.text_mesh_cache.insert(node_id, TextMeshCache { key, glyphs, bounds });
            }
        }

        // Same lazy-populate-a-cache shape as the shapes loop above, for
        // `NodeKind::Image` — only the shared unit quad and this node's own
        // `DrawBinding` are built here (both cheap, synchronous GPU calls);
        // the actual baked texture (`image_cache`, keyed by `ImageId`) is
        // populated by `import_image`/`set_image_processing`/`load_tea`/
        // `load_teab` instead, since baking needs a real `.await` (a GPU
        // readback) that this per-frame `render` — a plain sync method — has
        // no way to perform. A node whose asset hasn't finished baking yet
        // (or was skipped due to a decode error) is simply not drawn this
        // frame, same "no cache entry, draw nothing" convention the shapes
        // loop above already uses for a degenerate path.
        if self.image_quad_mesh.is_none() {
            self.image_quad_mesh = Some(self.renderer.upload_mesh(&an1mbyt3_geometry::unit_quad_mesh()));
        }
        let artboard = self.document.artboard(artboard_id).unwrap();
        // Not `artboard.images()` — same reasoning as the shapes/texts loops
        // above: an `Image` node can be a `Group`'s mask too.
        for node_id in artboard.node_ids() {
            if artboard.is_effectively_deleted(node_id) {
                continue;
            }
            let node = artboard.node(node_id).expect("Artboard::node_ids() returned an id not in its own arena");
            if !matches!(node.kind, NodeKind::Image(_)) {
                continue;
            }
            self.image_bindings.entry(node_id).or_insert_with(|| self.renderer.create_draw_binding());
        }

        // Particle System simulation + GPU resource pre-pass — mirrors the
        // Image loop just above (one shared mesh, grown per-node binding
        // pools instead of one binding each) but also does the actual
        // simulation step: `render` is the only place
        // `an1mbyt3_runtime::step_particle_system` is ever called, once per
        // system per frame, before `node_draws` (an immutable read-only
        // pass, below) builds this frame's own `DrawCall`s from whatever's
        // now alive. See `an1mbyt3_runtime::particle_sim`'s own module doc
        // for the simulation itself.
        if self.particle_mesh.is_none() {
            // Tessellated big, then shrunk to the unit disc: lyon's flattening
            // tolerance is absolute, so a 1-unit ellipse came out as an
            // octagon — visibly so on large particles (smoke puffs).
            let mut disc = tessellate_path(&an1mbyt3_core::Path::ellipse(64.0, 64.0));
            for p in &mut disc.positions {
                p[0] /= 64.0;
                p[1] /= 64.0;
            }
            self.particle_mesh = Some(self.renderer.upload_mesh(&disc));
        }
        let artboard = self.document.artboard(artboard_id).unwrap();
        // Particles, Particle Fluid 2D and Goo 2D only run in Playground.
        let fx_live = self.effects_live();
        let selected = self.selected_node;
        // A goo group previews whole when any member is selected (its
        // particle members run too).
        self.goo_preview = if fx_live {
            Default::default()
        } else {
            artboard
                .node_ids()
                .filter(|id| fx_preview(artboard, selected, *id))
                .filter_map(|id| {
                    let n = artboard.node(id)?;
                    n.goo().map(|g| g.group).or(n.anti_goo().map(|a| a.group))
                })
                .collect()
        };
        for node_id in artboard.node_ids() {
            if artboard.is_effectively_deleted(node_id) {
                continue;
            }
            let node = artboard.node(node_id).expect("Artboard::node_ids() returned an id not in its own arena");
            let Some(config) = node.particle_system() else { continue };
            // A hidden emitter (itself or any ancestor) is switched off, like
            // a disabled GameObject in Unity: no simulation, and it restarts
            // fresh when shown again. Same while designing: particles only
            // run in Playground (keeps Design light).
            let previewed = fx_preview(artboard, selected, node_id) || node.goo().map(|g| g.group).or(node.anti_goo().map(|a| a.group)).is_some_and(|g| self.goo_preview.contains(&g));
            if !(fx_live || previewed) || !crate::lighting2d::effectively_visible(artboard, node_id) {
                self.particle_runtimes.remove(&node_id);
                continue;
            }
            let emitter_world = artboard.world_transform(node_id);
            let runtime = self.particle_runtimes.entry(node_id).or_default();
            an1mbyt3_runtime::step_particle_system(config, runtime, delta_seconds, emitter_world);
            if let Some(fx) = node.particle_behaviors() {
                let local = config.main.simulation_space == an1mbyt3_core::SimulationSpace::Local;
                apply_particle_behaviors_2d(fx, runtime.particles_mut(), local, emitter_world, delta_seconds);
            }
            // Particle Path: steer onto a Shape node's outline.
            if let Some(cfg) = node.particle_path() {
                let local = config.main.simulation_space == an1mbyt3_core::SimulationSpace::Local;
                if let Some(line) = cfg.path.and_then(|id| path_polyline(artboard, id, local.then_some(emitter_world))) {
                    apply_particle_path(cfg, &line, runtime.particles_mut(), self.shader_clock, delta_seconds);
                }
            }
            let live = runtime.particles().len();
            let bindings = self.particle_bindings.entry(node_id).or_default();
            while bindings.len() < live {
                bindings.push(self.renderer.create_draw_binding());
            }
            // Particle Look: this system's shape mesh, and glow bindings.
            if let Some(look) = node.particle_look() {
                if let Some((key, path)) = particle_shape_path(artboard, look) {
                    if !self.particle_shape_meshes.contains_key(&key) {
                        let mesh = unit_mesh(&path);
                        self.particle_shape_meshes.insert(key, self.renderer.upload_mesh(&mesh));
                    }
                    self.particle_shape_keys.insert(node_id, key);
                } else {
                    self.particle_shape_keys.remove(&node_id);
                }
                if look.glow > 0.0 {
                    let glow = self.particle_glow_bindings.entry(node_id).or_default();
                    while glow.len() < live {
                        glow.push(self.renderer.create_draw_binding());
                    }
                }
            } else {
                self.particle_shape_keys.remove(&node_id);
            }
        }
        if self.particle_glow.is_none() {
            let quad = an1mbyt3_geometry::Mesh {
                positions: vec![[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]],
                indices: vec![0, 1, 2, 0, 2, 3],
                uvs: vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            };
            if let Ok(material) = self.renderer.compile_material_blended(PARTICLE_GLOW_WGSL, true) {
                self.particle_glow = Some((self.renderer.upload_mesh(&quad), material));
            }
        }

        let tf = crate::profiler::now_ms();
        // Particle Fluid 2D: contour every liquid node's particles into
        // metaball outlines (rim / body / highlight) and upload them.
        let artboard = self.document.artboard(artboard_id).unwrap();
        let fluids: Vec<(NodeId, an1mbyt3_core::ParticleFluid2D, Vec<crate::metaball2d::Blob>, f32)> = artboard
            .node_ids()
            .filter(|id| (fx_live || fx_preview(artboard, selected, *id)) && !artboard.is_effectively_deleted(*id) && crate::lighting2d::effectively_visible(artboard, *id))
            .filter_map(|id| {
                let node = artboard.node(id)?;
                let fluid = *node.particle_fluid()?;
                Some((id, fluid, fluid_blobs(self, artboard, id, node, &fluid), fluid_cell(node, &fluid)))
            })
            .collect();
        let live: std::collections::HashSet<NodeId> = fluids.iter().map(|(id, _, _, _)| *id).collect();
        // A lowered resolution (pixel blocks) is pixel art: liquid edges
        // stay hard there even with an Antialias 2D component, which only
        // smooths at native resolution.
        let pixelated = self.renderer.resolution_scale() > 1.0;
        let smoothing = |node: Option<&Node>| if pixelated { None } else { node.and_then(|n| n.antialias()).map(|a| a.softness.max(0.05)) };
        self.particle_fluid_gpu.retain(|id, _| live.contains(id));
        self.fluid_keys.retain(|id, _| live.contains(id));
        for (node_id, fluid, blobs, cell) in fluids {
            let key = crate::metaball2d::fingerprint_blobs(&blobs, fingerprint_debug(&(fluid, smoothing(artboard.node(node_id)))) ^ cell.to_bits() as u64);
            if self.fluid_keys.get(&node_id) == Some(&key) && self.particle_fluid_gpu.contains_key(&node_id) {
                continue;
            }
            self.fluid_keys.insert(node_id, key);
            // The liquid's field (and its highlight drops on the same grid)
            // becomes a small texture; a shader draws it per pixel.
            let field = crate::metaball2d::Field::from_blobs(&blobs, cell);
            if field.layout().0 == 0 {
                self.particle_fluid_gpu.remove(&node_id);
                continue;
            }
            let highlight: Vec<crate::metaball2d::Blob> = blobs.iter().map(|b| crate::metaball2d::Blob { center: b.center + Vec2::new(-1.0, 1.0) * b.radius * 0.14, radius: b.radius * 0.42, ..*b }).collect();
            let second = field.blobs_on_grid(&highlight);
            let look = fluid_look(&fluid, smoothing(artboard.node(node_id)));
            let slot = self.particle_fluid_gpu.entry(node_id).or_insert_with(|| LiquidGpu::new(&self.renderer));
            slot.update(&self.renderer, &mut self.liquid_materials, &field, Some(&second), &look, glam::Affine2::IDENTITY);
        }

        self.sections.fluid2d_ms = crate::profiler::now_ms() - tf;
        let tg = crate::profiler::now_ms();
        // Goo 2D: every group's members (shapes, text, sprites, particles)
        // as signed-distance sources, merged into one liquid outline and
        // tessellated in the group owner's (first member's) space.
        let artboard = self.document.artboard(artboard_id).unwrap();
        let mut groups: Vec<(u32, NodeId, an1mbyt3_core::Goo2D, Vec<crate::metaball2d::Source>)> = Vec::new();
        let t_sources = crate::profiler::now_ms();
        let (mut mesh_ms, mut upload_ms) = (0.0, 0.0);
        for id in artboard.node_ids() {
            if artboard.is_effectively_deleted(id) || !crate::lighting2d::effectively_visible(artboard, id) {
                continue;
            }
            let Some(node) = artboard.node(id) else { continue };
            let Some(goo) = node.goo() else { continue };
            if !fx_live && !self.goo_preview.contains(&goo.group) {
                continue;
            }
            let sources = goo_sources(self, artboard, id, node);
            match groups.iter_mut().find(|g| g.0 == goo.group) {
                Some(g) => g.3.extend(sources),
                None => groups.push((goo.group, id, goo.clone(), sources)),
            }
        }
        // Anti Goo: carving members, as negative sources of their group
        // (a group needs at least one real member to exist).
        for id in artboard.node_ids() {
            if artboard.is_effectively_deleted(id) || !crate::lighting2d::effectively_visible(artboard, id) {
                continue;
            }
            let Some(node) = artboard.node(id) else { continue };
            let Some(anti) = node.anti_goo() else { continue };
            let Some(g) = groups.iter_mut().find(|g| g.0 == anti.group) else { continue };
            // (Only its outline / particles count — not its own Goo or
            // Flipbook frames.)
            let plain = Node { behaviors: node.behaviors.iter().filter(|b| matches!(b.behavior, an1mbyt3_core::NodeBehavior::ParticleSystem(_))).cloned().collect(), ..node.clone() };
            let strength = anti.strength.max(0.0);
            for mut s in goo_sources(self, artboard, id, &plain) {
                match &mut s {
                    crate::metaball2d::Source::Circle { weight, .. } | crate::metaball2d::Source::Polygons { weight, .. } => *weight = -weight.abs() * strength,
                }
                g.3.push(s);
            }
        }
        self.sections.goo_sources_ms = crate::profiler::now_ms() - t_sources;
        // The liquid draws at its group's front-most member — so moving any
        // member up or down the layers moves the liquid with it (it used
        // to sit at the first-created member's place, whatever the order).
        self.goo_draw_at.clear();
        if !groups.is_empty() {
            let layer_order: Vec<u32> = self.document.sorting_layers().iter().map(|l| l.id).collect();
            let rank: HashMap<NodeId, (usize, usize)> = artboard
                .pickable_nodes()
                .enumerate()
                .map(|(i, (id, _))| {
                    let layer = effective_layer(artboard, id);
                    (id, (layer_order.iter().position(|l| *l == layer).unwrap_or(layer_order.len()), i))
                })
                .collect();
            let mut front: HashMap<u32, (NodeId, (usize, usize))> = HashMap::new();
            for id in artboard.node_ids() {
                let Some(goo) = artboard.node(id).and_then(|n| n.goo()) else { continue };
                if artboard.is_effectively_deleted(id) || !crate::lighting2d::effectively_visible(artboard, id) {
                    continue;
                }
                let r = rank.get(&id).copied().unwrap_or((0, 0));
                match front.get(&goo.group) {
                    Some((_, best)) if *best >= r => {}
                    _ => {
                        front.insert(goo.group, (id, r));
                    }
                }
            }
            for (group, _, _, _) in &groups {
                if let (Some((at, _)), Some(owner)) = (front.get(group), groups.iter().find(|g| g.0 == *group).map(|g| g.1)) {
                    self.goo_draw_at.insert(*at, owner);
                }
            }
        }
        let owners: std::collections::HashSet<NodeId> = groups.iter().map(|g| g.1).collect();
        self.goo_gpu.retain(|id, _| owners.contains(id));
        self.goo_keys.retain(|id, _| owners.contains(id));
        for (_, owner, goo, sources) in groups {
            let to_owner = artboard.world_transform(owner).to_affine2().inverse();
            // The owner's Goo Material (and its graph) are part of the look.
            let goo_material = artboard.node(owner).and_then(|n| n.goo_material()).cloned();
            // Goo Gloss on the owner, else on any member of the group.
            let gloss = artboard.node(owner).and_then(|n| n.goo_gloss()).copied().or_else(|| {
                artboard.node_ids().filter(|id| !artboard.is_effectively_deleted(*id)).find_map(|id| {
                    let n = artboard.node(id)?;
                    (n.goo().is_some_and(|g| g.group == goo.group)).then(|| n.goo_gloss().copied()).flatten()
                })
            });
            let graph = goo_material.as_ref().and_then(|m| m.material).and_then(|id| self.document.material(id)).and_then(|m| match &m.kind {
                an1mbyt3_core::MaterialKind::ShaderGraph(g) => Some(g.clone()),
                _ => None,
            });
            let mut extra = fingerprint_debug(&(&goo, smoothing(artboard.node(owner)), &goo_material, &graph, &gloss));
            for v in to_owner.to_cols_array() {
                extra = extra.rotate_left(7) ^ v.to_bits() as u64;
            }
            let key = crate::metaball2d::fingerprint(&sources, extra);
            if self.goo_keys.get(&owner) == Some(&key) && self.goo_gpu.contains_key(&owner) {
                continue;
            }
            self.goo_keys.insert(owner, key);
            let t_mesh = crate::profiler::now_ms();
            // One field for the whole group (still members' patches come
            // from the cache), drawn per pixel on the GPU.
            let field = crate::metaball2d::Field::from_sources_cached(&sources, goo.reach.max(1.0), goo.detail.clamp(0.08, 0.6), Some(&mut self.goo_field_cache));
            mesh_ms += crate::profiler::now_ms() - t_mesh;
            let t_upload = crate::profiler::now_ms();
            if field.layout().0 == 0 {
                self.goo_gpu.remove(&owner);
                continue;
            }
            let mut look = goo_look(&goo, gloss.as_ref(), &field, smoothing(artboard.node(owner)));
            if let Some(m) = &goo_material {
                look.material = graph.as_ref().and_then(|g| an1mbyt3_core::compile_wgsl(g).ok());
                if m.glow > 0.0 {
                    let edge = look.rim.as_ref().map_or(look.body.0, |r| r.0);
                    // Spread 0 → a thin halo; 1 → out to where the field ends.
                    let outer = edge * (1.0 - (0.15 + 0.85 * m.glow_spread.clamp(0.0, 1.0))).max(0.0);
                    look.glow = Some(crate::goo_gpu::Glow { color: m.glow_color, strength: m.glow, outer: outer.max(1e-4), edge });
                }
            }
            let slot = self.goo_gpu.entry(owner).or_insert_with(|| LiquidGpu::new(&self.renderer));
            // Goo Gloss: the liquid eroded by its depth, for the shine.
            let eroded = gloss.map(|g| crate::metaball2d::erode(&field, look.body.0, gloss_levels(&g).0));
            slot.update(&self.renderer, &mut self.liquid_materials, &field, eroded.as_deref(), &look, to_owner);
            upload_ms += crate::profiler::now_ms() - t_upload;
        }

        self.goo_field_cache.end_frame();
        self.sections.goo_mesh_ms = mesh_ms;
        self.sections.goo_upload_ms = upload_ms;
        self.sections.goo_ms = crate::profiler::now_ms() - tg;

        // Liquid 2D pre-pass: enough draw bindings for every particle.
        let artboard = self.document.artboard(artboard_id).unwrap();
        let liquid_counts: Vec<(NodeId, usize)> = artboard
            .node_ids()
            .filter(|id| !artboard.is_effectively_deleted(*id))
            .filter_map(|id| artboard.node(id)?.liquid().map(|config| (id, liquid_points(self, artboard, id, config).len())))
            .collect();
        for (node_id, count) in liquid_counts {
            let bindings = self.liquid_bindings.entry(node_id).or_default();
            while bindings.len() < count {
                bindings.push(self.renderer.create_draw_binding());
            }
        }

        // Trail Renderer pre-pass: step each trail from its node's current
        // world position, rebuild its ribbon mesh, and (re)bake its texture
        // if the gradient/texture changed. Same "mutate GPU caches before
        // `node_draws` borrows them" placement as the particle pass above.
        let artboard = self.document.artboard(artboard_id).unwrap();
        let trails: Vec<(NodeId, an1mbyt3_core::TrailNode, Vec2)> = artboard
            .node_ids()
            .filter(|id| !artboard.is_effectively_deleted(*id))
            .filter_map(|id| artboard.node(id)?.trail().map(|trail| (id, trail.clone(), artboard.world_transform(id).translation)))
            .collect();
        for (node_id, config, head) in trails {
            let runtime = self.trail_runtimes.entry(node_id).or_default();
            crate::lighting2d::step_trail(&config, runtime, head, delta_seconds);
            let mesh = crate::lighting2d::build_trail_mesh(&config, runtime, head).map(|mesh| self.renderer.upload_mesh(&mesh));
            let key = (config.color_gradient.clone(), config.texture);
            let needs_bake = self.trail_gpu.get(&node_id).is_none_or(|gpu| gpu.texture_key != key);
            if needs_bake {
                let decoded = config
                    .texture
                    .and_then(|image_id| self.document.image(image_id))
                    .and_then(|asset| an1mbyt3_image::decode_to_rgba(&asset.source).ok());
                let (pixels, width, height) =
                    crate::lighting2d::bake_trail_texture(&config, decoded.as_ref().map(|(rgba, w, h)| (rgba.as_slice(), *w, *h)));
                let texture = self.renderer.upload_texture(width, height, &pixels, an1mbyt3_core::ImageFilterMode::Linear);
                match self.trail_gpu.get_mut(&node_id) {
                    Some(gpu) => {
                        gpu.texture = texture;
                        gpu.texture_key = key;
                    }
                    None => {
                        let binding = self.renderer.create_draw_binding();
                        self.trail_gpu.insert(node_id, TrailGpu { mesh: None, binding, texture, texture_key: key });
                    }
                }
            }
            if let Some(gpu) = self.trail_gpu.get_mut(&node_id) {
                gpu.mesh = mesh;
            }
        }

        // Cloth 2D pre-pass: step each cloth (Playground; otherwise it hangs
        // at rest), rebuild its fabric + shade meshes and (re)bake its
        // texture if its look changed.
        let artboard = self.document.artboard(artboard_id).unwrap();
        let cloths: Vec<(NodeId, an1mbyt3_core::Cloth2D, an1mbyt3_core::Transform2D)> = artboard
            .node_ids()
            .filter(|id| !artboard.is_effectively_deleted(*id))
            .filter_map(|id| artboard.node(id)?.cloth().map(|cloth| (id, *cloth, artboard.world_transform(id))))
            .collect();
        let live_cloths: std::collections::HashSet<NodeId> = cloths.iter().map(|(id, _, _)| *id).collect();
        self.cloth_sims.retain(|id, _| live_cloths.contains(id));
        self.cloth_gpu.retain(|id, _| live_cloths.contains(id));
        for (node_id, config, world) in cloths {
            let (sin, cos) = world.rotation.sin_cos();
            let to_world = move |p: Vec2| {
                let s = p * world.scale;
                world.translation + Vec2::new(s.x * cos - s.y * sin, s.x * sin + s.y * cos)
            };
            let mesh = if self.playground_active {
                let sim = self.cloth_sims.entry(node_id).or_insert_with(|| an1mbyt3_runtime::ClothSim::new(&config, &to_world));
                if !sim.matches(&config) {
                    *sim = an1mbyt3_runtime::ClothSim::new(&config, &to_world);
                }
                sim.step(&config, &to_world, delta_seconds);
                an1mbyt3_runtime::build_cloth_mesh(sim)
            } else {
                an1mbyt3_runtime::build_cloth_mesh(&an1mbyt3_runtime::ClothSim::new(&config, &to_world))
            };
            let key = cloth_texture_key(&config);
            if self.cloth_gpu.get(&node_id).is_none_or(|gpu| gpu.texture_key != key) {
                let image = config.image.and_then(|id| self.document.image(id)).and_then(|asset| an1mbyt3_image::decode_to_rgba(&asset.source).ok());
                let (pixels, width, height) = bake_cloth_texture(&config, image);
                let texture = self.renderer.upload_texture(width, height, &pixels, an1mbyt3_core::ImageFilterMode::Linear);
                match self.cloth_gpu.get_mut(&node_id) {
                    Some(gpu) => {
                        gpu.texture = texture;
                        gpu.texture_key = key;
                    }
                    None => {
                        let bindings = std::array::from_fn(|_| self.renderer.create_draw_binding());
                        self.cloth_gpu.insert(node_id, ClothGpu { meshes: [None, None, None, None, None], bindings, texture, texture_key: key, shading: 0.0 });
                    }
                }
            }
            let upload = |renderer: &an1mbyt3_renderer::Renderer, indices: &[u32]| {
                (!indices.is_empty()).then(|| renderer.upload_mesh(&an1mbyt3_geometry::Mesh { positions: mesh.positions.clone(), indices: indices.to_vec(), uvs: mesh.uvs.clone() }))
            };
            let meshes = [upload(&self.renderer, &mesh.indices), upload(&self.renderer, &mesh.folds), upload(&self.renderer, &mesh.creases), upload(&self.renderer, &mesh.backs), upload(&self.renderer, &mesh.highlights)];
            if let Some(gpu) = self.cloth_gpu.get_mut(&node_id) {
                gpu.meshes = meshes;
                gpu.shading = config.shading;
            }
        }

        // Tree 2D pre-pass: advance each tree's clock / shake / falling
        // leaves (Playground; frozen and fully grown while designing) and
        // rebuild its meshes.
        let artboard = self.document.artboard(artboard_id).unwrap();
        let trees: Vec<(NodeId, an1mbyt3_core::Tree2D)> =
            artboard.node_ids().filter(|id| !artboard.is_effectively_deleted(*id)).filter_map(|id| artboard.node(id)?.tree().map(|tree| (id, *tree))).collect();
        let live_trees: std::collections::HashSet<NodeId> = trees.iter().map(|(id, _)| *id).collect();
        self.tree_states.retain(|id, _| live_trees.contains(id));
        self.tree_gpu.retain(|id, _| live_trees.contains(id));
        for (node_id, config) in trees {
            let state = self.tree_states.entry(node_id).or_default();
            state.sync(&config);
            let grow = if self.playground_active {
                state.step(&config, delta_seconds);
                if config.grow_on_play { (state.time / config.grow_time.max(0.1)).min(1.0) } else { 1.0 }
            } else {
                1.0
            };
            let key = Some((config, state.time.to_bits(), state.shake.to_bits(), state.leaves.len(), grow.to_bits()));
            let gpu = self.tree_gpu.entry(node_id).or_insert_with(|| TreeGpu { parts: Vec::new(), bindings: Vec::new(), key: None });
            if gpu.key == key {
                continue;
            }
            gpu.key = key;
            let mesh = an1mbyt3_runtime::build_tree_mesh(&config, state, grow);
            while gpu.bindings.len() < mesh.parts.len() {
                gpu.bindings.push(self.renderer.create_draw_binding());
            }
            // Refill last frame's GPU buffers in place (a swaying tree
            // rebuilds every frame in Playground) rather than allocating new.
            let mut pool = std::mem::take(&mut gpu.parts).into_iter();
            for (color, positions, indices) in mesh.parts {
                if positions.is_empty() || indices.is_empty() {
                    continue;
                }
                let part = an1mbyt3_geometry::Mesh { positions, indices, uvs: Vec::new() };
                let gpu_mesh = match pool.next() {
                    Some((mut old, _)) => {
                        self.renderer.rewrite_mesh(&mut old, &part);
                        old
                    }
                    None => self.renderer.upload_mesh_with_headroom(&part, 1.5),
                };
                gpu.parts.push((gpu_mesh, color));
            }
        }

        // Motion-path world positions for the selected node's Translation
        // track (empty if nothing's selected, or the selection has no such
        // track) — computed before the mesh-cache-population/draw-building
        // passes below need it, since building the actual dot/line
        // `GpuMesh`es and `DrawBinding`s has to happen before `draws`
        // borrows them.
        let artboard = self.document.artboard(artboard_id).unwrap();
        let motion_path_points: Vec<Vec2> = (|| {
            let selected_id = self.selected_node?;
            let animation_id = self.active_animation?;
            let animation = artboard.animation(animation_id)?;
            let (_, track) = animation.tracks.iter().find(|(n, t)| *n == selected_id && t.kind() == PropertyKind::Translation)?;
            let PropertyTrack::Translation(track) = track else { unreachable!("kind() == Translation") };
            if track.keyframes.is_empty() {
                return None;
            }
            let selected_node_ref = artboard.node(selected_id)?;
            let parent_world = match selected_node_ref.parent {
                Some(parent_id) => artboard.try_world_transform(parent_id)?,
                None => an1mbyt3_core::Transform2D::default(),
            };
            let parent_affine = parent_world.to_affine2();
            Some(track.keyframes.iter().map(|k| parent_affine.transform_point2(k.value)).collect())
        })()
        .unwrap_or_default();

        if !motion_path_points.is_empty() && self.motion_path_dot_mesh.is_none() {
            let dot_mesh = self.renderer.upload_mesh(&tessellate_path(&build_dot_path(4.0)));
            self.motion_path_dot_mesh = Some(dot_mesh);
        }
        // One `DrawBinding` per dot, plus one more for the connecting line
        // if there's more than one point — fresh every frame (the point
        // count/positions can change frame to frame as the node is dragged),
        // same "don't share a uniform slot across draws sharing one mesh"
        // reasoning `DrawBinding`'s own doc comment already established.
        let motion_path_bindings: Vec<DrawBinding> =
            (0..motion_path_points.len() + usize::from(motion_path_points.len() > 1))
                .map(|_| self.renderer.create_draw_binding())
                .collect();
        let motion_path_line_mesh = (motion_path_points.len() > 1).then(|| {
            let mut line_path = Path::new();
            line_path.commands.push(an1mbyt3_core::PathCommand::MoveTo(motion_path_points[0]));
            line_path.commands.extend(motion_path_points[1..].iter().map(|&p| an1mbyt3_core::PathCommand::LineTo(p)));
            let style = StrokeStyle { width: 1.5, cap: StrokeCap::Round, join: StrokeJoin::Round };
            self.renderer.upload_mesh(&tessellate_stroke(&line_path, &style))
        });

        // The artboard's own bounds, drawn as a dark-gray grid *before* any
        // node — the camera's now a real infinite-pan/zoom viewport (see
        // `Camera`'s doc comment), so unlike the old 1:1-sized canvas,
        // there's genuinely empty space around the artboard now that needs
        // *something* to visually mark where the artboard actually is; a
        // plain graph-paper grid directly over the same background the rest
        // of the infinite canvas uses, no separate fill color. Cached by
        // `(width, height)` the same lazy way `text_mesh_cache` is —
        // rebuilt only if those change.
        let artboard = self.document.artboard(artboard_id).unwrap();
        let needs_rebuild = !matches!(&self.artboard_chrome, Some(chrome) if chrome.width == artboard.width && chrome.height == artboard.height);
        if needs_rebuild {
            let grid_style = StrokeStyle { width: 1.0, cap: StrokeCap::Butt, join: StrokeJoin::Miter };
            let grid_mesh = self
                .renderer
                .upload_mesh(&tessellate_stroke(&build_grid_path(artboard.width, artboard.height, ARTBOARD_GRID_SPACING), &grid_style));
            let grid_binding = self.renderer.create_draw_binding();
            self.artboard_chrome = Some(ArtboardChrome { width: artboard.width, height: artboard.height, grid_mesh, grid_binding });
        }
        let chrome = self.artboard_chrome.as_ref().expect("just populated above");

        let artboard = self.document.artboard(artboard_id).unwrap();
        // Must happen before `build_scene_ops` below borrows `self` (via
        // `mesh_cache`/`image_cache` lookups) for the rest of this method —
        // a no-op when unchanged from last frame (see its own doc comment),
        // so cheap to call unconditionally every frame rather than tracking
        // "did this artboard's `resolution_scale` change" ourselves.
        // A Pixel Perfect camera's "Upscale Render Texture" overrides the
        // scene's own resolution setting while Playground looks through it.
        let camera_frame = self.active_camera_frame(artboard_id);
        let render_scale = camera_frame.as_ref().and_then(|f| f.render_scale).unwrap_or(artboard.resolution_scale);
        self.renderer.set_resolution_scale(render_scale);
        if camera_frame.as_ref().is_some_and(|f| f.crop.is_some()) && self.crop_bars.is_none() {
            let mesh = self.renderer.upload_mesh(&tessellate_path(&Path::rect(1.0, 1.0)));
            let bindings = (0..4).map(|_| self.renderer.create_draw_binding()).collect();
            self.crop_bars = Some((mesh, bindings));
        }
        let artboard = self.document.artboard(artboard_id).unwrap();
        // Drawn separately from `unmasked`/`masked` below (see
        // `Renderer::render_with_masks`'s own doc comment) — the grid is
        // editor chrome, never part of the actual artwork, and stays at the
        // real native resolution/unaffected by any post effect even when
        // the real content ends up rendered through a scaled/post-processed
        // offscreen buffer instead.
        let chrome_draws: Vec<DrawCall> = if self.export_transparent { Vec::new() } else { vec![DrawCall {
            mesh: &chrome.grid_mesh,
            binding: &chrome.grid_binding,
            transform: an1mbyt3_core::Transform2D::default(),
            fill_color: [grid_r, grid_g, grid_b, ARTBOARD_GRID_COLOR[3]],
            blend_mode: BlendMode::Normal,
            gradient: None,
            texture: None,
            material: None,
        }] };
        stats.prep_ms = stopwatch.lap();
        let mut unmasked: Vec<DrawCall> = Vec::new();
        // A real, depth-first walk of the scene tree — not four separate
        // flat per-kind lists the way this used to build `draws` — because
        // `GroupNode::mask` needs actual tree position (how many ancestor
        // masked `Group`s a node sits under) to clip correctly, which a
        // flat `artboard.shapes()`/`texts()`/etc. iteration order alone
        // can't express. See `build_scene_ops`'s own doc comment.
        let mut masked: Vec<an1mbyt3_renderer::SceneOp> = Vec::new();
        // Shared across every root so two independently-masked top-level
        // subtrees (including an original and its freshly-duplicated copy —
        // see `SceneOp::Draw`'s own doc comment) never end up with the same
        // `island` id.
        let mut next_island = 0u32;
        // 2D lighting (Unity URP-style): only switches on when the scene
        // actually contains a Light2D, so unlit scenes render exactly as
        // before. See `lighting2d` / `an1mbyt3_renderer::Lighting`.
        let collected_lights = crate::lighting2d::collect_lights(artboard, self.shader_clock);
        let lighting_on = collected_lights.any_light;
        let mut unlit_draws: Vec<DrawCall> = Vec::new();
        // One O(N) pass for draw order + mask targets (the walk below would
        // otherwise rescan every node at every node — O(N²)).
        let scene_index = artboard.scene_index();
        // Sorting layers: one walk per layer in use, back to front (just
        // one plain walk when everything is on one layer). With per-layer
        // light filters, consecutive layers lit alike become light groups.
        let layer_order: Vec<u32> = self.document.sorting_layers().iter().map(|l| l.id).collect();
        let used = used_layers(artboard, &layer_order);
        let grouping = lighting_on && collected_lights.filtered;
        // (layer, unmasked, masked) per walk.
        let mut layer_parts: Vec<(Option<u32>, Vec<DrawCall>, Vec<an1mbyt3_renderer::SceneOp>)> = Vec::new();
        let passes: Vec<Option<u32>> = if used.len() > 1 || grouping { used.iter().map(|l| Some(*l)).collect() } else { vec![None] };
        for (i, pass) in passes.iter().enumerate() {
            self.scene_layer_filter.set(*pass);
            self.scene_collect_unlit.set(i == 0);
            let (mut u, mut m) = (Vec::new(), Vec::new());
            for &root_id in scene_index.children(None) {
                let split = if lighting_on { Some(&mut unlit_draws) } else { None };
                build_scene_ops(self, artboard, &scene_index, root_id, 0, 0, &mut next_island, &mut u, &mut m, split);
            }
            layer_parts.push((*pass, u, m));
        }
        self.scene_layer_filter.set(None);
        self.scene_collect_unlit.set(true);
        // Light groups: runs of layers that the same lights reach.
        let lights_reaching = |layer: u32| -> Vec<usize> { collected_lights.lights.iter().enumerate().filter(|(_, l)| l.layers.as_ref().is_none_or(|ls| ls.contains(&layer))).map(|(i, _)| i).collect() };
        let mut group_parts: Vec<(Vec<DrawCall>, Vec<an1mbyt3_renderer::SceneOp>, Vec<usize>, ([f32; 3], [f32; 3]))> = Vec::new();
        if grouping {
            for (layer, u, m) in layer_parts.drain(..) {
                let layer = layer.unwrap_or(0);
                let reach = lights_reaching(layer);
                let ambient = collected_lights.ambient_for(layer);
                match group_parts.last_mut() {
                    Some(g) if g.2 == reach && g.3 == ambient => append_scene_parts(&mut g.0, &mut g.1, u, m),
                    _ => group_parts.push((u, m, reach, ambient)),
                }
            }
        } else {
            for (_, u, m) in layer_parts.drain(..) {
                append_scene_parts(&mut unmasked, &mut masked, u, m);
            }
            // Global texture (the scene's post stack): the whole scene is
            // drawn as one layer and the texture laid over it, clipped to
            // what's drawn — shown in Design too, so the look is visible
            // while working.
            let global: Vec<an1mbyt3_core::Texture> = artboard
                .post_effects()
                .iter()
                .filter(|l| l.enabled)
                .filter_map(|l| match l.effect {
                    an1mbyt3_core::PostEffect::Texture(t) => Some(t),
                    _ => None,
                })
                .collect();
            if !global.is_empty() && !(unmasked.is_empty() && masked.is_empty()) {
                let half = Vec2::new(artboard.width, artboard.height) * 0.5;
                let textures = global.iter().map(|t| t.image.and_then(|id| self.image_cache.get(&id))).collect();
                let fx = an1mbyt3_renderer::LayerFx::new(global.into_iter().map(|t| std::borrow::Cow::Owned(an1mbyt3_core::ImageEffect::Texture(t))).collect(), -half, half, None).with_textures(textures);
                let island = next_island;
                let scene_unmasked = std::mem::take(&mut unmasked);
                let scene_masked = std::mem::take(&mut masked);
                masked.push(an1mbyt3_renderer::SceneOp::Begin { island, after: 0 });
                masked.push(an1mbyt3_renderer::SceneOp::Layer { unmasked: scene_unmasked, masked: scene_masked, composites: vec![(0, 1.0)], island, fx: Some(Box::new(fx)) });
            }
        }
        let light_casters = if lighting_on {
            crate::lighting2d::collect_casters(artboard, |id| crate::dto::local_bounds(artboard, id), |id| self.caster_local_outline(artboard, id))
        } else {
            Vec::new()
        };
        // (Which collected light each draw came from — some lights make
        // no draw, so indices differ; light groups pick by source.)
        let (light_sources, light_draws): (Vec<usize>, Vec<an1mbyt3_renderer::LightDraw>) = collected_lights
            .lights
            .iter()
            .enumerate()
            .filter_map(|(source, collected)| {
                let l = &collected.light;
                let shape = match l.light_type {
                    an1mbyt3_core::Light2DType::Spot => an1mbyt3_renderer::LightShape::Spot {
                        inner_radius: l.inner_radius,
                        outer_radius: l.outer_radius,
                        inner_angle_degrees: l.inner_spot_angle,
                        outer_angle_degrees: l.outer_spot_angle,
                    },
                    an1mbyt3_core::Light2DType::Freeform => an1mbyt3_renderer::LightShape::Freeform { points: &l.shape_points, falloff_size: l.falloff_size },
                    an1mbyt3_core::Light2DType::Sprite => {
                        let texture = artboard.node(collected.node).and_then(crate::lighting2d::light_sprite).and_then(|id| self.image_cache.get(&id))?;
                        an1mbyt3_renderer::LightShape::Sprite { texture, size: Vec2::new(l.sprite_width, l.sprite_height) }
                    }
                    an1mbyt3_core::Light2DType::Global => return None,
                };
                Some((source, an1mbyt3_renderer::LightDraw {
                    shape,
                    transform: collected.transform,
                    // Light colors are authored in sRGB like every other color;
                    // the light maps work in linear light.
                    color: {
                        let c = an1mbyt3_renderer::linear_rgba(l.color);
                        [c[0] * l.intensity, c[1] * l.intensity, c[2] * l.intensity]
                    },
                    additive_blend_style: l.blend_style == an1mbyt3_core::LightBlendStyle::Additive,
                    alpha_blend_overlap: l.overlap_operation == an1mbyt3_core::LightOverlapOperation::AlphaBlend,
                    max_overlap: l.overlap_operation == an1mbyt3_core::LightOverlapOperation::Max,
                    falloff_strength: l.falloff_strength,
                    shadows: l.shadows_enabled.then_some(an1mbyt3_renderer::LightShadows { strength: l.shadow_strength, softness: l.shadow_softness }),
                    volumetric: l.volumetric_enabled.then_some(an1mbyt3_renderer::LightVolumetric {
                        intensity: l.volumetric_intensity,
                        shadow_strength: if l.volumetric_shadows_enabled { l.volumetric_shadow_strength } else { 0.0 },
                    }),
                    falloff_steps: l.falloff_steps,
                    edge_blur: l.edge_blur,
                    pixel_size: l.pixel_size,
                }))
            })
            .unzip();
        // Pixel Perfect crop bars: four huge black rects hugging the frame,
        // drawn over everything (into the unlit overlay when lighting is on,
        // so the night doesn't tint them).
        if let (Some(frame), Some((bar_mesh, bar_bindings))) = (camera_frame.as_ref(), self.crop_bars.as_ref()) {
            if let Some(half) = frame.crop {
                const FAR: f32 = 100_000.0;
                let mut bars: Vec<(Vec2, Vec2)> = Vec::new();
                if half.x.is_finite() {
                    bars.push((Vec2::new(frame.pan.x - half.x - FAR / 2.0, frame.pan.y), Vec2::new(FAR, FAR * 2.0)));
                    bars.push((Vec2::new(frame.pan.x + half.x + FAR / 2.0, frame.pan.y), Vec2::new(FAR, FAR * 2.0)));
                }
                if half.y.is_finite() {
                    bars.push((Vec2::new(frame.pan.x, frame.pan.y - half.y - FAR / 2.0), Vec2::new(FAR * 2.0, FAR)));
                    bars.push((Vec2::new(frame.pan.x, frame.pan.y + half.y + FAR / 2.0), Vec2::new(FAR * 2.0, FAR)));
                }
                for (i, (center, size)) in bars.into_iter().enumerate() {
                    let draw = DrawCall {
                        mesh: bar_mesh,
                        binding: &bar_bindings[i],
                        transform: an1mbyt3_core::Transform2D { translation: center, scale: size, ..Default::default() },
                        fill_color: [0.0, 0.0, 0.0, 1.0],
                        blend_mode: BlendMode::Normal,
                        gradient: None,
                        texture: None,
                        material: None,
                    };
                    if lighting_on {
                        unlit_draws.push(draw);
                    } else {
                        unmasked.push(draw);
                    }
                }
            }
        }
        // Each light group's own lights (the same draws, filtered).
        let group_lights: Vec<Vec<an1mbyt3_renderer::LightDraw>> = group_parts.iter().map(|g| g.2.iter().filter_map(|&i| light_sources.iter().position(|&src| src == i).map(|k| light_draws[k])).collect()).collect();
        let lit_groups: Vec<an1mbyt3_renderer::LitGroup> = group_parts
            .iter()
            .zip(group_lights.iter())
            .map(|(g, lights)| an1mbyt3_renderer::LitGroup { unmasked: &g.0, masked: &g.1, ambient_multiply: g.3 .0, ambient_additive: g.3 .1, lights })
            .collect();
        let lighting = lighting_on.then(|| an1mbyt3_renderer::Lighting {
            ambient_multiply: collected_lights.ambient_multiply,
            ambient_additive: collected_lights.ambient_additive,
            lights: &light_draws,
            casters: &light_casters,
            unlit: &unlit_draws,
            groups: &lit_groups,
        });

        if let Some(dot_mesh) = &self.motion_path_dot_mesh {
            const MARKER_COLOR: [f32; 4] = [0.427, 0.549, 1.0, 0.9];
            const LINE_COLOR: [f32; 4] = [0.427, 0.549, 1.0, 0.5];
            for (i, &point) in motion_path_points.iter().enumerate() {
                unmasked.push(DrawCall {
                    mesh: dot_mesh,
                    binding: &motion_path_bindings[i],
                    transform: an1mbyt3_core::Transform2D { translation: point, ..Default::default() },
                    fill_color: MARKER_COLOR,
                    blend_mode: BlendMode::Normal,
                    gradient: None,
                    texture: None,
                    material: None,
                });
            }
            if let Some(line_mesh) = &motion_path_line_mesh {
                unmasked.push(DrawCall {
                    mesh: line_mesh,
                    binding: &motion_path_bindings[motion_path_points.len()],
                    transform: an1mbyt3_core::Transform2D::default(),
                    fill_color: LINE_COLOR,
                    blend_mode: BlendMode::Normal,
                    gradient: None,
                    texture: None,
                    material: None,
                });
            }
        }

        let camera = Camera { pan: Vec2::new(camera_pan_x, camera_pan_y), zoom: camera_zoom };
        let viewport_size = (self.canvas_width as f32, self.canvas_height as f32);
        // Only `enabled` stack entries actually render — a disabled one
        // still shows up (and keeps its authored parameters/position) in
        // the Effects panel's list, per `Artboard::post_effects`'s own doc
        // comment, it just contributes nothing to this frame.
        // Post effects only run at play time (Playground / the State
        // Machine preview): Design shows the scene as authored, and stays
        // light.
        let mut effects: Vec<an1mbyt3_core::PostEffect> = if self.effects_live() {
            // (A global Texture isn't a chain op — see the scene build.)
            artboard.post_effects().iter().filter(|layer| layer.enabled && !matches!(layer.effect, an1mbyt3_core::PostEffect::Texture(_))).map(|layer| layer.effect).collect()
        } else {
            Vec::new()
        };
        // The active camera's vignette (Playground only, like everything
        // else a camera controls) — last in the chain, so it frames the
        // final image the way a lens would.
        if let Some(v) = self.active_camera_vignette(artboard_id) {
            effects.push(an1mbyt3_core::PostEffect::Vignette { color: v.color, intensity: v.intensity, radius: v.radius, blur: v.blur });
        }
        let resolution_scale = artboard.resolution_scale;
        stats.scene_ms = stopwatch.lap();
        stats.nodes = artboard.node_ids().filter(|id| !artboard.is_effectively_deleted(*id)).count() as u32;
        stats.particles = self.particle_runtimes.values().map(|r| r.particles().len() as u32).sum();
        stats.lights = light_draws.len() as u32;
        // The frontend's own theme toggle (`App.tsx`'s `theme` state, via
        // `src/theme.ts`) picks both `clear_r/g/b` and `grid_r/g/b` — this
        // crate has no concept of "app theme" itself, only whatever colors
        // it's told to clear/draw the grid with each frame. Both were fixed
        // constants before the light theme existed at all.
        let clear_color = if self.export_transparent { wgpu::Color::TRANSPARENT } else { wgpu::Color { r: clear_r as f64, g: clear_g as f64, b: clear_b as f64, a: 1.0 } };
        stats.draws = (unmasked.len() + masked.len() + chrome_draws.len() + lighting.as_ref().map_or(0, |l| l.unlit.len())) as u32;
        let result = if lighting.is_some() || resolution_scale != 1.0 || effects.iter().any(|effect| *effect != an1mbyt3_core::PostEffect::None) {
            // `render_with_post_effects` now draws `(unmasked, masked)`
            // through the same stencil-aware two-pass technique
            // `render_with_masks` uses (see `Renderer::draw_masked_scene`'s
            // own doc comment) before running the post-process chain, so a
            // clip/mask is no longer dropped just because a post effect is
            // active — the two features used to be mutually exclusive code
            // paths (this arm used to flatten `masked` into plain unclipped
            // draws); they're composed now.
            self.renderer
                .render_with_post_effects(viewport_size, camera, clear_color, &chrome_draws, &unmasked, &masked, &effects, lighting.as_ref())
                .map_err(|err| JsValue::from_str(&format!("render failed: {err:?}")))
        } else {
            self.renderer
                .render_with_masks(viewport_size, camera, clear_color, &chrome_draws, &unmasked, &masked)
                .map_err(|err| JsValue::from_str(&format!("render failed: {err:?}")))
        };
        stats.gpu_submit_ms = stopwatch.lap();
        stats.total_ms = stopwatch.total();
        stats.scene3d_ms = self.sections.scene3d_ms;
        stats.particles3d_ms = self.sections.particles3d_ms;
        stats.fluid2d_ms = self.sections.fluid2d_ms;
        stats.goo_ms = self.sections.goo_ms;
        stats.goo_sources_ms = self.sections.goo_sources_ms;
        stats.goo_mesh_ms = self.sections.goo_mesh_ms;
        stats.goo_upload_ms = self.sections.goo_upload_ms;
        self.frame_stats = stats;
        result
    }

    /// Script loop: applies a frame's worth of untracked node writes in one
    /// call (one wasm round trip instead of one per property per node).
    /// `records` is a flat array of 7-number records
    /// `[op, node idx, node version, a, b, c, d]`:
    /// 1 translation `(x, y)`, 2 rotation `(degrees)`, 3 z-index `(z)`,
    /// 4 visible `(0/1)`, 5 image `(image idx, image version)`,
    /// 6 fill color `(r, g, b, a)`. Writes to missing nodes are skipped.
    pub fn apply_node_writes(&mut self, artboard: JsValue, records: Vec<f64>) -> Result<(), JsValue> {
        use slotmap::KeyData;
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let Some(artboard_mut) = self.document.artboard_mut(artboard_id) else { return Ok(()) };
        let key = |idx: f64, version: f64| KeyData::from_ffi(((version as u64) << 32) | (idx as u64 & 0xffff_ffff));
        for r in records.chunks_exact(7) {
            let node_id: NodeId = key(r[1], r[2]).into();
            let Some(node) = artboard_mut.node_mut(node_id) else { continue };
            match r[0] as u32 {
                1 => node.transform.translation = Vec2::new(r[3] as f32, r[4] as f32),
                2 => node.transform.rotation = (r[3] as f32).to_radians(),
                3 => node.z_index = r[3] as f32,
                4 => node.visible = r[3] != 0.0,
                5 => {
                    if let NodeKind::Image(image) = &mut node.kind {
                        image.image = key(r[3], r[4]).into();
                    }
                }
                6 => set_node_fill_color(node, [r[3] as f32, r[4] as f32, r[5] as f32, r[6] as f32]),
                _ => {}
            }
        }
        Ok(())
    }

    /// Script loop: fingerprint of `artboard`'s structure — when it changes,
    /// refetch `script_scene`. See `script_scene.rs`.
    pub fn script_revision(&self, artboard: JsValue) -> Result<u32, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let artboard = self.document.artboard(artboard_id).ok_or_else(|| JsValue::from_str("script_revision: unknown artboard"))?;
        Ok(crate::script_scene::revision(&self.document, artboard))
    }

    /// Script loop: the static node table (+ images) for `artboard`.
    pub fn script_scene(&self, artboard: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let artboard = self.document.artboard(artboard_id).ok_or_else(|| JsValue::from_str("script_scene: unknown artboard"))?;
        to_js_value(&crate::script_scene::scene(&self.document, artboard))
    }

    /// Script loop: this frame's per-node transforms/visibility/fill/image
    /// as one `Float32Array` (`script_scene::STRIDE` floats per node).
    pub fn script_dynamic(&self, artboard: JsValue) -> Result<Vec<f32>, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let artboard = self.document.artboard(artboard_id).ok_or_else(|| JsValue::from_str("script_dynamic: unknown artboard"))?;
        Ok(crate::script_scene::dynamic(artboard))
    }

    /// The last frame's profile (`profiler::FrameStats`): per-stage
    /// milliseconds and scene counts, for the F3 performance overlay.
    pub fn frame_stats(&self) -> Result<JsValue, JsValue> {
        to_js_value(&self.frame_stats)
    }

    /// Replaces `artboard`'s whole post-processing effect stack — the
    /// Effects panel's add/remove/reorder/toggle/duplicate-effect action.
    /// `value` is a JS array of `{ effect: { kind, ...params }, enabled }`
    /// objects (`dto::PostEffectLayerDto`'s shape) — the panel always sends
    /// the *whole* list back, even for a single-entry edit (toggling one
    /// checkbox, dragging one slider), same "undo only needs the old whole
    /// list" convention `Command::SetPostEffects` itself documents. Takes
    /// effect on the *next* `render()` call, not immediately — this only
    /// updates the document; the canvas's own rAF loop is what actually
    /// re-renders.
    pub fn set_post_effects(&mut self, artboard: JsValue, value: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let dtos: Vec<crate::dto::PostEffectLayerDto> =
            serde_wasm_bindgen::from_value(value).map_err(|err| JsValue::from_str(&format!("invalid post effects: {err}")))?;
        let layers: Vec<PostEffectLayer> = dtos.into_iter().map(PostEffectLayer::from).collect();
        self.document.apply(Command::SetPostEffects { artboard: artboard_id, value: layers });
        self.document_summary()
    }

    /// Current document state, in the same shape the frontend's
    /// `DocumentSummary` TypeScript interface already expects.
    pub fn document_summary(&self) -> Result<JsValue, JsValue> {
        to_js_value(&summarize_document(&self.document, self.active_animation, self.active_state_machine))
    }

    /// The same summary as JSON text — for the live panel refresh, which
    /// compares it with the previous one and skips re-rendering the whole
    /// editor when nothing changed (most frames of a live preview).
    pub fn document_summary_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&summarize_document(&self.document, self.active_animation, self.active_state_machine)).map_err(|err| JsValue::from_str(&format!("document summary failed: {err}")))
    }

    /// Re-samples the active animation's tracks (if any) into their target
    /// nodes' transforms/fill colors at `self.scrub_time` — shared by
    /// `set_scrub_time` (an explicit scrub) and `undo`/`redo` (so undoing a
    /// keyframe edit doesn't leave the canvas showing a now-stale sampled
    /// pose until the next scrub). Cloning the `Animation` out first is what
    /// lets `Animation::apply` take `artboard_mut` mutably right after an
    /// immutable borrow of the very same artboard read it — acceptable
    /// (editor-only, not a hot per-frame path) since a design tool's
    /// authored animations are small.
    fn resample_active_animation(&mut self, artboard_id: ArtboardId) {
        let Some(animation_id) = self.active_animation else { return };
        let Some(artboard_mut) = self.document.artboard_mut(artboard_id) else { return };
        let Some(animation) = artboard_mut.animation(animation_id).cloned() else { return };
        animation.apply(artboard_mut, self.scrub_time);
    }

    /// Rebuilds `self.state_machine` (the *live* preview/test instance) from
    /// whatever `self.active_state_machine` currently points at — called
    /// after every state-machine-authoring edit (`add_state`/`add_transition`/
    /// etc.) so the State Machine tab's live preview always reflects the
    /// latest saved definition, and after `set_active_state_machine` itself.
    /// A no-op if `active_state_machine` is `None` or no longer resolves
    /// (e.g. it was just soft-deleted) — `self.state_machine` simply keeps
    /// whatever it was previously showing rather than this method needing
    /// `state_machine` to become an `Option` just to express "nothing to
    /// preview right now" (a document with zero state machines is rare in
    /// practice — every authoring path here always
    /// creates at least one — and the stale instance left behind is
    /// harmless: `StateMachine::apply` already silently no-ops against
    /// missing content, per its own doc comment).
    fn refresh_state_machine_preview(&mut self, artboard_id: ArtboardId) {
        let Some(state_machine_id) = self.active_state_machine else { return };
        let Some(artboard) = self.document.artboard(artboard_id) else { return };
        let Some(def) = artboard.state_machine(state_machine_id) else { return };
        self.state_machine = StateMachine::from_definition(def, artboard);
    }

    /// Parses a `layer` wasm-boundary argument: `null`/`undefined` means
    /// "the state machine's own top-level states" (the base layer, in
    /// `an1mbyt3_runtime::StateMachine`'s terms), any other value must be a
    /// string naming one of `StateMachineDef::layers`. Shared by
    /// `state_list_command`/`transitions_command` below so every per-state/
    /// per-transition editing method stays layer-aware through one
    /// indirection rather than each re-deriving this itself.
    fn parse_layer_arg(layer: &JsValue) -> Result<Option<String>, JsValue> {
        if layer.is_null() || layer.is_undefined() {
            return Ok(None);
        }
        layer.as_string().map(Some).ok_or_else(|| JsValue::from_str("layer must be a string or null"))
    }

    /// Reads the current whole `states` list a state-editing method should
    /// mutate — `state_machine`'s own top-level `states` (`layer: None`) or
    /// one of its extra `layers`' own `states` (`layer: Some(name)`) — and
    /// returns it alongside a closure that re-applies an edited copy via
    /// whichever `Command` actually owns that list: `Command::SetStateMachineStates`
    /// for the base layer (unchanged from before layers existed), or
    /// `Command::SetStateMachineLayers` (a whole-`layers`-list swap, with
    /// just the target layer's `states` replaced) for an extra one — see
    /// `StateMachineLayerDef`'s own doc comment (`an1mbyt3-core`) for what
    /// a layer is. `add_state`/`update_state`/`set_state_blend`/`delete_state`
    /// are layer-aware purely through this one indirection.
    fn state_list_command(
        &self,
        artboard_id: ArtboardId,
        state_machine_id: an1mbyt3_core::StateMachineId,
        layer: &JsValue,
    ) -> Result<(Vec<StateDef>, Box<dyn Fn(Vec<StateDef>) -> Command>), JsValue> {
        let layer_name = Self::parse_layer_arg(layer)?;
        let def = self
            .document
            .artboard(artboard_id)
            .and_then(|a| a.state_machine(state_machine_id))
            .ok_or_else(|| JsValue::from_str("unknown state machine"))?;

        let Some(layer_name) = layer_name else {
            let states = def.states.clone();
            return Ok((states, Box::new(move |value| Command::SetStateMachineStates { artboard: artboard_id, state_machine: state_machine_id, value })));
        };

        let layers = def.layers.clone();
        let states = layers.iter().find(|l| l.name == layer_name).ok_or_else(|| JsValue::from_str("unknown layer"))?.states.clone();
        Ok((
            states,
            Box::new(move |value| {
                let mut layers = layers.clone();
                if let Some(l) = layers.iter_mut().find(|l| l.name == layer_name) {
                    l.states = value;
                }
                Command::SetStateMachineLayers { artboard: artboard_id, state_machine: state_machine_id, value: layers }
            }),
        ))
    }

    /// Same indirection as `state_list_command`, one level narrower: reads
    /// `state_name`'s current `transitions` list (again scoped to the base
    /// layer or a named extra one) and returns a closure that re-applies an
    /// edited copy — the base layer keeps using the finer-grained
    /// `Command::SetStateTransitions` (unchanged), while an extra layer
    /// goes through the same whole-`layers`-list `Command::SetStateMachineLayers`
    /// swap `state_list_command` uses, since a layer's states have no
    /// standalone per-state command of their own. `add_transition`/
    /// `set_transition`/`remove_transition` are layer-aware purely through
    /// this one indirection.
    fn transitions_command(
        &self,
        artboard_id: ArtboardId,
        state_machine_id: an1mbyt3_core::StateMachineId,
        layer: &JsValue,
        state_name: String,
    ) -> Result<(Vec<TransitionDef>, Box<dyn Fn(Vec<TransitionDef>) -> Command>), JsValue> {
        let layer_name = Self::parse_layer_arg(layer)?;
        let def = self
            .document
            .artboard(artboard_id)
            .and_then(|a| a.state_machine(state_machine_id))
            .ok_or_else(|| JsValue::from_str("unknown state machine"))?;

        let Some(layer_name) = layer_name else {
            let transitions = def.states.iter().find(|s| s.name == state_name).ok_or_else(|| JsValue::from_str("unknown state"))?.transitions.clone();
            return Ok((
                transitions,
                Box::new(move |value| Command::SetStateTransitions { artboard: artboard_id, state_machine: state_machine_id, state_name: state_name.clone(), value }),
            ));
        };

        let layers = def.layers.clone();
        let transitions = layers
            .iter()
            .find(|l| l.name == layer_name)
            .ok_or_else(|| JsValue::from_str("unknown layer"))?
            .states
            .iter()
            .find(|s| s.name == state_name)
            .ok_or_else(|| JsValue::from_str("unknown state"))?
            .transitions
            .clone();
        Ok((
            transitions,
            Box::new(move |value| {
                let mut layers = layers.clone();
                if let Some(l) = layers.iter_mut().find(|l| l.name == layer_name) {
                    if let Some(s) = l.states.iter_mut().find(|s| s.name == state_name) {
                        s.transitions = value;
                    }
                }
                Command::SetStateMachineLayers { artboard: artboard_id, state_machine: state_machine_id, value: layers }
            }),
        ))
    }

    /// Same indirection as `transitions_command`, for `program` instead of
    /// `transitions`: the base layer keeps using the dedicated
    /// `Command::SetStateProgram`, while an extra layer falls back to the
    /// same whole-`layers`-list `Command::SetStateMachineLayers` swap
    /// every other layer-scoped per-state field in this file uses.
    fn program_command(
        &self,
        artboard_id: ArtboardId,
        state_machine_id: an1mbyt3_core::StateMachineId,
        layer: &JsValue,
        state_name: String,
    ) -> Result<Box<dyn Fn(Option<an1mbyt3_core::ProgramDef>) -> Command>, JsValue> {
        let layer_name = Self::parse_layer_arg(layer)?;
        let def = self
            .document
            .artboard(artboard_id)
            .and_then(|a| a.state_machine(state_machine_id))
            .ok_or_else(|| JsValue::from_str("unknown state machine"))?;

        let Some(layer_name) = layer_name else {
            def.states.iter().find(|s| s.name == state_name).ok_or_else(|| JsValue::from_str("unknown state"))?;
            return Ok(Box::new(move |value| Command::SetStateProgram { artboard: artboard_id, state_machine: state_machine_id, state_name: state_name.clone(), value }));
        };

        let layers = def.layers.clone();
        layers
            .iter()
            .find(|l| l.name == layer_name)
            .ok_or_else(|| JsValue::from_str("unknown layer"))?
            .states
            .iter()
            .find(|s| s.name == state_name)
            .ok_or_else(|| JsValue::from_str("unknown state"))?;
        Ok(Box::new(move |value| {
            let mut layers = layers.clone();
            if let Some(l) = layers.iter_mut().find(|l| l.name == layer_name) {
                if let Some(s) = l.states.iter_mut().find(|s| s.name == state_name) {
                    s.program = value;
                }
            }
            Command::SetStateMachineLayers { artboard: artboard_id, state_machine: state_machine_id, value: layers }
        }))
    }

    /// Points `active_state_machine` at the loaded artboard's first state
    /// machine (if any) and rebuilds the live preview from it — same
    /// "previous session state means nothing against a brand-new `Document`"
    /// reasoning `load_tea`/`load_teab` already apply to `active_animation`.
    fn reset_active_state_machine_after_load(&mut self) {
        let Some(artboard_id) = self.current_artboard_id() else {
            self.active_state_machine = None;
            return;
        };
        self.active_state_machine = self.document.artboard(artboard_id).and_then(|a| a.state_machines().next()).map(|(id, _)| id);
        self.refresh_state_machine_preview(artboard_id);
    }

    /// Playground's "look through the active camera" view: `(pan, physical
    /// zoom, background)` for `artboard_id`'s `active_camera`, or `None` if
    /// Playground isn't running or no camera is assigned/resolvable (the
    /// editor's own free-roam pan/zoom then applies, exactly as before
    /// cameras existed). The zoom *fits the camera's frame* — the artboard's
    /// own `width`x`height` divided by `CameraNode::zoom`, see that type's
    /// own doc comment — into the canvas (letterboxing the narrower axis),
    /// so what an author framed in the editor is what plays, at any window
    /// size. Physical pixels per world unit, same convention as `render`'s
    /// own `camera_zoom` parameter (already device-pixel-ratio-scaled).
    fn active_camera_view(&self, artboard_id: ArtboardId) -> Option<(Vec2, f32, [f32; 4])> {
        self.active_camera_frame(artboard_id).map(|frame| (frame.pan, frame.zoom, frame.background))
    }

    /// Everything Playground's camera view needs for one frame — pan/zoom/
    /// background as before, plus what Unity's Pixel Perfect Camera adds
    /// (`CameraNode::pixel_perfect`):
    ///
    /// - **integer scale**: the frame (scene size ÷ camera zoom) is shown at
    ///   the largest *whole* number of screen pixels per art pixel that
    ///   fits, so no art pixel is ever drawn 2 wide next to one drawn 3 wide;
    /// - **grid snapping**: `PixelSnapping` moves the camera in whole art
    ///   pixels; `UpscaleRenderTexture` additionally renders the frame at
    ///   that low resolution and nearest-upscales it (`render_scale`);
    /// - **crop frame**: which sides get black bars (`crop_half_extent`).
    fn active_camera_frame(&self, artboard_id: ArtboardId) -> Option<CameraFrame> {
        if !self.playground_active {
            return None;
        }
        let artboard = self.document.artboard(artboard_id)?;
        let camera_id = artboard.active_camera()?;
        let camera_node = artboard.node(camera_id).filter(|n| !n.deleted)?;
        let camera = *camera_node.camera()?;
        let mut pan = artboard.try_world_transform(camera_id)?.translation;
        // Camera Shake: trauma² × noise.
        if let Some(shake) = camera_node.camera_shake() {
            let trauma = self.camera_trauma.get(&camera_id).copied().unwrap_or(0.0).max(shake.constant.clamp(0.0, 1.0));
            if trauma > 0.0 {
                let t = self.shader_clock * shake.frequency.max(0.0);
                let k = trauma * trauma * shake.amplitude;
                pan += Vec2::new(an1mbyt3_core::noise1(t, 1, 7), an1mbyt3_core::noise1(t, 2, 7)) * k;
            }
        }
        let mut frame_w = artboard.width.max(1.0) / camera.zoom.max(0.01);
        let mut frame_h = artboard.height.max(1.0) / camera.zoom.max(0.01);
        if artboard.screen.mode == an1mbyt3_core::ScreenMode::Responsive {
            // Fill the canvas: the view's world size follows the screen.
            let css = Vec2::new(self.canvas_width as f32, self.canvas_height as f32) / self.last_device_pixel_ratio.max(0.01);
            let view = an1mbyt3_core::responsive_view(Vec2::new(frame_w, frame_h), css, &artboard.screen);
            frame_w = view.x;
            frame_h = view.y;
        }
        let fit = (self.canvas_width as f32 / frame_w).min(self.canvas_height as f32 / frame_h);
        let pp = camera.pixel_perfect;
        if !pp.enabled {
            // A Fixed screen shows exactly the camera's frame: the axis with
            // room to spare gets bars instead of revealing world outside the
            // frame (what an exported game shows too). Responsive screens
            // already fill the canvas with the frame.
            let crop = (artboard.screen.mode != an1mbyt3_core::ScreenMode::Responsive).then(|| Vec2::new(frame_w, frame_h) * 0.5);
            return Some(CameraFrame { pan, zoom: fit, background: camera.background_color, render_scale: None, crop });
        }
        use an1mbyt3_core::{PixelCropFrame, PixelGridSnapping};
        let zoom = if pp.crop_frame == PixelCropFrame::StretchFill { fit } else { fit.floor().max(1.0) };
        if pp.grid_snapping != PixelGridSnapping::None {
            pan = pan.round();
        }
        // A 3D view renders at full resolution: the low-res upscale is for
        // pixel art, and a 3D world through it is just blurry blocks.
        let render_scale = (pp.grid_snapping == PixelGridSnapping::UpscaleRenderTexture && !self.renderer.has_scene_3d()).then_some(zoom.floor().max(1.0));
        let half = Vec2::new(frame_w, frame_h) * 0.5;
        let crop = match pp.crop_frame {
            PixelCropFrame::None => None,
            PixelCropFrame::Pillarbox => Some(Vec2::new(half.x, f32::INFINITY)),
            PixelCropFrame::Letterbox => Some(Vec2::new(f32::INFINITY, half.y)),
            PixelCropFrame::Windowbox | PixelCropFrame::StretchFill => Some(half),
        };
        Some(CameraFrame { pan, zoom, background: camera.background_color, render_scale, crop })
    }

    /// The opaque-pixel silhouette of `image` (or of tile `tile` within it:
    /// `Some((index, tile_w, tile_h))`), as pixel-corner points, top-left
    /// origin: each row's leftmost and rightmost opaque pixel contributes its
    /// four corners — enough for the convex hull to follow rounded or
    /// stepped edges exactly. Empty = fully transparent. Cached per
    /// image/tile (`alpha_bounds_cache`).
    fn silhouette_points(&self, image: an1mbyt3_core::ImageId, tile: Option<(u32, u32, u32)>) -> Vec<[u32; 2]> {
        let key = (image, tile.map_or(u32::MAX, |(index, _, _)| index));
        if let Some(hit) = self.alpha_bounds_cache.borrow().get(&key) {
            return hit.clone();
        }
        let computed = (|| {
            let asset = self.document.image(image)?;
            let (rgba, width, height) = an1mbyt3_image::decode_to_rgba(&asset.source).ok()?;
            let (x_start, y_start, w, h) = match tile {
                Some((index, tw, th)) => {
                    let columns = (width / tw.max(1)).max(1);
                    ((index % columns) * tw, (index / columns) * th, tw, th)
                }
                None => (0, 0, width, height),
            };
            let mut points = Vec::new();
            for y in y_start..(y_start + h).min(height) {
                let opaque = |x: u32| rgba[((y * width + x) * 4 + 3) as usize] > 8;
                let row: Vec<u32> = (x_start..(x_start + w).min(width)).filter(|&x| opaque(x)).collect();
                if let (Some(&l), Some(&r)) = (row.first(), row.last()) {
                    let (ly, lx, rx) = (y - y_start, l - x_start, r - x_start + 1);
                    points.extend_from_slice(&[[lx, ly], [lx, ly + 1], [rx, ly], [rx, ly + 1]]);
                }
            }
            Some(points)
        })()
        .unwrap_or_default();
        self.alpha_bounds_cache.borrow_mut().insert(key, computed.clone());
        computed
    }

    /// A shadow caster's node-local outline — Unity's "use renderer
    /// silhouette": Images and Tilemaps cast from the convex hull of their
    /// visible pixels (so a rounded-corner sprite's shadow starts flush at
    /// its rounded corners, not at its rectangle), other kinds from their
    /// bounds. `None` = use the generic per-kind outline.
    fn caster_local_outline(&self, artboard: &an1mbyt3_core::Artboard, node_id: NodeId) -> Option<Vec<Vec2>> {
        let node = artboard.node(node_id)?;
        let points: Vec<Vec2> = match &node.kind {
            NodeKind::Image(image) => {
                let asset = self.document.image(image.image)?;
                let (iw, ih) = an1mbyt3_image::png_dimensions(&asset.source).ok()?;
                self.silhouette_points(image.image, None)
                    .into_iter()
                    .map(|[px, py]| Vec2::new(-image.width / 2.0 + px as f32 / iw as f32 * image.width, image.height / 2.0 - py as f32 / ih as f32 * image.height))
                    .collect()
            }
            NodeKind::Tilemap(map) => {
                let total = Vec2::new(map.map_width as f32, map.map_height as f32) * map.cell_size;
                let scale = Vec2::new(map.cell_size / map.tile_width_px.max(1) as f32, map.cell_size / map.tile_height_px.max(1) as f32);
                let mut points = Vec::new();
                for cy in 0..map.map_height {
                    for cx in 0..map.map_width {
                        let Some(tile) = map.get(cx, cy) else { continue };
                        let left = cx as f32 * map.cell_size - total.x / 2.0;
                        let top = total.y / 2.0 - cy as f32 * map.cell_size;
                        for [px, py] in self.silhouette_points(map.tileset, Some((tile, map.tile_width_px, map.tile_height_px))) {
                            points.push(Vec2::new(left + px as f32 * scale.x, top - py as f32 * scale.y));
                        }
                    }
                }
                points
            }
            _ => return None,
        };
        let hull = convex_hull(points);
        (hull.len() >= 3).then_some(hull)
    }

    /// The active camera's vignette, if Playground is viewing through a
    /// camera that has one enabled.
    fn active_camera_vignette(&self, artboard_id: ArtboardId) -> Option<an1mbyt3_core::CameraVignette> {
        if !self.playground_active {
            return None;
        }
        let artboard = self.document.artboard(artboard_id)?;
        let camera_id = artboard.active_camera()?;
        let camera = artboard.node(camera_id).filter(|n| !n.deleted)?.camera()?;
        camera.vignette.enabled.then_some(camera.vignette)
    }

    /// Moves `artboard_id`'s active camera toward its `follow_target` by one
    /// frame's worth of exponential smoothing (`CameraNode::follow_smoothing`'s
    /// own doc comment) — Playground only, a live runtime pose like any
    /// state-machine-driven one (`set_mode` restores the authored pose on
    /// leaving Playground). Moves by a world-space delta added to the
    /// camera's local translation, which is exact for a root-level camera
    /// (the normal case) and only approximate under a rotated/scaled parent.
    fn step_camera_follow(&mut self, artboard_id: ArtboardId, delta_seconds: f32) {
        if !self.playground_active {
            return;
        }
        let Some(artboard) = self.document.artboard(artboard_id) else { return };
        let Some(camera_id) = artboard.active_camera() else { return };
        let Some(camera) = artboard.node(camera_id).and_then(|n| n.camera().copied()) else { return };
        let Some(target_id) = camera.follow_target else { return };
        if artboard.node(target_id).is_none_or(|n| n.deleted) {
            return;
        }
        let (Some(target), Some(current)) = (artboard.try_world_transform(target_id), artboard.try_world_transform(camera_id)) else { return };
        let t = if camera.follow_smoothing <= 0.0 { 1.0 } else { 1.0 - (-camera.follow_smoothing * delta_seconds.max(0.0)).exp() };
        let delta = (target.translation - current.translation) * t;
        if let Some(node) = self.document.artboard_mut(artboard_id).and_then(|a| a.node_mut(camera_id)) {
            node.transform.translation += delta;
        }
    }

    /// The artboard the editor is currently showing/editing — `active_artboard`
    /// if it still resolves to a non-deleted artboard, else the first
    /// non-deleted one (a deleted active scene, or a freshly loaded document
    /// whose old `active_artboard` id means nothing, both fall back here).
    fn current_artboard_id(&self) -> Option<ArtboardId> {
        self.active_artboard
            .filter(|&id| self.document.artboard(id).is_some_and(|a| !a.deleted))
            .or_else(|| self.document.artboards().next().map(|(id, _)| id))
    }

    /// Switches which scene (`Artboard`) the editor renders/edits — the Scenes
    /// switcher's own selection. Not undoable (session state, same as
    /// `set_active_animation`). Clears every `NodeId`-keyed render cache:
    /// `NodeId`s are only unique *within* one artboard's own slotmap, so two
    /// scenes can genuinely have a node at the same `{idx, version}` — a
    /// stale cache entry from the previous scene must never be mistaken for
    /// the new scene's node. `ImageId`/`FontId`-keyed caches stay (those are
    /// document-level, shared across every scene). Also re-picks the active
    /// animation/state machine, since both ids are artboard-scoped and the
    /// previous scene's mean nothing here.
    pub fn set_active_artboard(&mut self, artboard: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        if !self.document.artboard(artboard_id).is_some_and(|a| !a.deleted) {
            return Err(JsValue::from_str("set_active_artboard: unknown or deleted artboard"));
        }
        // A live design-pose snapshot (Playground/Animate/State Machine
        // running) belongs to the scene being left — its `NodeId`s mean
        // nothing (or collide with unrelated nodes) in the new one. Restore
        // it onto the old scene now, then snapshot the new scene fresh, so
        // leaving the live mode later restores the right poses on the right
        // scene.
        // Leaving a scene mid-play restores its designed pose — and removes
        // whatever scripts spawned in it, so coming back (a "Play Again")
        // starts from the authored scene, not the last run's leftovers.
        if self.playground_active {
            if let Some(old_id) = self.current_artboard_id().filter(|old| *old != artboard_id) {
                self.despawn_runtime_spawned_in(old_id);
            }
        }
        // (Restored onto the scene it was taken of — even if that scene
        // was just deleted — never onto whatever is "current" now.)
        if let Some(snapshot) = self.design_pose_snapshot.take() {
            let only_keyed = !self.playground_active;
            self.restore_design_pose(snapshot, only_keyed);
            self.design_pose_snapshot = self.snapshot_design_pose(artboard_id);
        }
        self.active_artboard = Some(artboard_id);
        self.mesh_cache.clear();
        self.shape_mesh_source.clear();
        self.stroke_mesh_cache.clear();
        self.stroke_mesh_source.clear();
        self.tilemap_mesh_cache.clear();
        self.tilemap_mesh_source.clear();
        self.text_mesh_cache.clear();
        self.image_bindings.clear();
        self.particle_runtimes.clear();
        self.particle_bindings.clear();
        self.particle_fluid_gpu.clear();
        self.goo_gpu.clear();
        self.goo_keys.clear();
        self.fluid_keys.clear();
        self.goo_text_cache.borrow_mut().clear();
        self.liquid_bindings.clear();
        self.trail_runtimes.clear();
        self.physics = None;
        self.physics3d = None;
        self.trail_gpu.clear();
        self.cloth_sims.clear();
        self.cloth_gpu.clear();
        self.tree_states.clear();
        self.tree_gpu.clear();
        self.active_animation = self.document.artboard(artboard_id).and_then(|a| a.animations().next()).map(|(id, _)| id);
        self.reset_active_state_machine_after_load();
        self.scrub_time = 0.0;
        self.document_summary()
    }

    /// `true` if `node`'s `property` track, within the active animation, has
    /// a keyframe at exactly `self.scrub_time` — the check behind "editing a
    /// property while the playhead sits on one of its own keyframes updates
    /// that keyframe instead of just the live pose."
    fn keyframe_at_scrub_time(&self, artboard_id: ArtboardId, node_id: NodeId, property: PropertyKind) -> bool {
        let Some(animation_id) = self.active_animation else { return false };
        let Some(artboard) = self.document.artboard(artboard_id) else { return false };
        let Some(animation) = artboard.animation(animation_id) else { return false };
        animation.tracks.iter().any(|(n, track)| {
            *n == node_id
                && track.kind() == property
                && match track {
                    PropertyTrack::Translation(t) => t.keyframes.iter().any(|k| k.time == self.scrub_time),
                    PropertyTrack::Rotation(t) => t.keyframes.iter().any(|k| k.time == self.scrub_time),
                    PropertyTrack::Scale(t) => t.keyframes.iter().any(|k| k.time == self.scrub_time),
                    PropertyTrack::FillColor(t) => t.keyframes.iter().any(|k| k.time == self.scrub_time),
                    PropertyTrack::Path(t) => t.keyframes.iter().any(|k| k.time == self.scrub_time),
                    PropertyTrack::MotionPath { track: t, .. } => t.keyframes.iter().any(|k| k.time == self.scrub_time),
                    PropertyTrack::Field { track: t, .. } => t.keyframes.iter().any(|k| k.time == self.scrub_time),
                }
        })
    }

    /// Reads `node`'s current value for `property` — `node.transform` for
    /// the three transform properties, `NodeKind`'s fill color for
    /// `FillColor`, `ShapeNode::path` for `Path` (`None` if the kind has
    /// none/isn't a `Shape`).
    fn read_property_value(&self, artboard_id: ArtboardId, node_id: NodeId, property: PropertyKind) -> Option<KeyframeValue> {
        let node = self.document.artboard(artboard_id)?.node(node_id)?;
        Some(match property {
            PropertyKind::Translation => KeyframeValue::Translation(node.transform.translation),
            PropertyKind::Rotation => KeyframeValue::Rotation(node.transform.rotation),
            PropertyKind::Scale => KeyframeValue::Scale(node.transform.scale),
            PropertyKind::FillColor => KeyframeValue::FillColor(node.fill_color()?),
            PropertyKind::Path => match &node.kind {
                NodeKind::Shape(shape) => KeyframeValue::Path(shape.path.clone()),
                _ => return None,
            },
            // A `MotionPath` track's value is a scalar progress along its
            // guide, not anything derivable from `node`'s own current state
            // (unlike every other property, which mirrors a real field on
            // the node) — there's no "current pose" to snapshot, so this
            // property is never added via the generic "pose it, then key
            // it" `add_keyframe` flow the other variants share; see
            // `add_keyframe`'s own `MotionPath` arm.
            PropertyKind::MotionPath => return None,
            // Needs the path, not just its hash — see `add_keyframe`.
            PropertyKind::Field(_) => return None,
        })
    }

    /// `Some(delta)` if `node` already has at least one keyframe for
    /// `property` in *any* animation on `artboard` (not just the active
    /// one) — the check behind `Command::ShiftPropertyKeyframes` ("moving a
    /// keyframed node's Design-mode rest pose rebases its whole animated
    /// path, rather than the animation staying pinned to wherever it was
    /// first authored"). `delta` is `new_value` minus the node's *current*
    /// value for that property, read before the edit lands.
    fn keyframed_property_delta(&self, artboard_id: ArtboardId, node_id: NodeId, property: PropertyKind, new_value: KeyframeValue) -> Option<KeyframeValue> {
        let artboard = self.document.artboard(artboard_id)?;
        let has_keyframes = artboard
            .animations()
            .any(|(_, animation)| animation.tracks.iter().any(|(n, t)| *n == node_id && t.kind() == property));
        if !has_keyframes {
            return None;
        }
        let old_value = self.read_property_value(artboard_id, node_id, property)?;
        Some(new_value.subtract(old_value))
    }

    /// Applies `value` as one of three things, gated first on whether the
    /// toolbar's mode is currently "Animate" (`animate_mode_active`) — shared
    /// by `set_node_translation`/`set_node_rotation`/`set_node_scale`:
    ///
    /// - **In Animate mode** (actively authoring keyframes): if the playhead
    ///   sits exactly on one of `node`'s own `property` keyframes
    ///   (`keyframe_at_scrub_time`), overwrite *that keyframe* instead of
    ///   just the live pose — otherwise scrubbing back to that time would
    ///   re-sample the old value and the edit would look like it never
    ///   happened. Otherwise (scrubbed to an empty moment, positioning the
    ///   node for a *new* keyframe), `plain_command` alone — this is
    ///   ordinary keyframe authoring, not a reason to touch any *other*
    ///   keyframe's value.
    /// - **Outside Animate mode** (Design mode): if `node` has keyframes for
    ///   `property` *anywhere* on this artboard, `plain_command` is bundled
    ///   with a `Command::ShiftPropertyKeyframes` into one `Command::Batch`
    ///   — the edit rebases the entire keyframed path by the same delta, so
    ///   re-entering Animate mode animates relative to wherever Design mode
    ///   just put it ("animate relative to its new position"), rather than
    ///   the original authored position. Otherwise, `plain_command` alone —
    ///   no keyframes exist yet for this property.
    ///
    /// Without the Animate-mode branch being separate from the Design-mode
    /// one, positioning a node for a brand-new keyframe (scrubbed to an
    /// empty moment, mid-authoring) was incorrectly triggering a path-wide
    /// rebase instead of an isolated edit — the bug this priority split fixes.
    fn apply_transform_edit(&mut self, artboard_id: ArtboardId, node_id: NodeId, property: PropertyKind, value: KeyframeValue, plain_command: Command) {
        let command = self.build_transform_edit_command(artboard_id, node_id, property, value, plain_command);
        self.document.apply(command);
    }

    /// If the playhead sits exactly on one of `node`'s own `property`
    /// keyframes, an `AddKeyframe` overwriting it with `value` — otherwise
    /// `plain`. The Animate-mode half of `build_transform_edit_command`'s
    /// branch, factored out so `commit_node_scale` (which needs this
    /// per-property, not just once per call) doesn't duplicate it.
    fn keyframe_or_plain(&self, artboard_id: ArtboardId, node_id: NodeId, property: PropertyKind, value: KeyframeValue, plain: Command) -> Command {
        if self.keyframe_at_scrub_time(artboard_id, node_id, property) {
            let animation_id = self.active_animation.expect("keyframe_at_scrub_time only returns true when Some");
            Command::AddKeyframe { artboard: artboard_id, animation: animation_id, node: node_id, time: self.scrub_time, value, easing: Easing::Linear }
        } else {
            plain
        }
    }

    /// If `node` already has keyframes anywhere for `property`, `plain`
    /// batched with a `ShiftPropertyKeyframes` rebasing them by the delta
    /// `value` represents — otherwise `plain` alone. The Design-mode half of
    /// `build_transform_edit_command`, factored out for the same reason as
    /// `keyframe_or_plain` above.
    fn shift_or_plain(&self, artboard_id: ArtboardId, node_id: NodeId, property: PropertyKind, value: KeyframeValue, plain: Command) -> Command {
        match self.keyframed_property_delta(artboard_id, node_id, property, value) {
            Some(delta) => Command::Batch(vec![plain, Command::ShiftPropertyKeyframes { artboard: artboard_id, node: node_id, property, delta }]),
            None => plain,
        }
    }

    /// The Alt-held "edit this parent's rotate/scale independently of its
    /// children" gesture (`TransformHandles.tsx`) — computes one plain
    /// `Command::SetTranslation`/`SetRotation`/`SetScale` per *direct* child
    /// of `node_id`, each restoring that child's own *world* transform back
    /// to whatever it already was, so `node_id`'s own about-to-change
    /// transform (`new_local`, its post-drag local value) doesn't visibly
    /// carry those children along with it. Must be called with each child's
    /// world transform read *before* `node_id` itself actually changes (this
    /// method itself is read-only — it just returns the commands, batched
    /// alongside `node_id`'s own edit by the caller).
    ///
    /// Only direct children, deliberately — a grandchild's own world
    /// transform is already fully determined by its parent's (`node_id`'s
    /// child)'s world transform, which this fixes; if that child's world
    /// stays put, everything nested under it does too, with no need to walk
    /// deeper.
    ///
    /// Anchor is never touched (mirrors `reparent_node`'s own precedent of
    /// treating anchor as intrinsic to a node, not something a
    /// world-preserving recompute should change), and this always emits
    /// plain `Command`s regardless of Animate mode — `node_id`'s own edit
    /// still respects Animate mode/keyframing exactly as it already did;
    /// only these compensating child adjustments are a plain, direct edit,
    /// since keyframing every affected child's own compensation too would
    /// be a much bigger, separate feature.
    fn compensating_child_commands(&self, artboard_id: ArtboardId, node_id: NodeId, new_local: an1mbyt3_core::Transform2D) -> Vec<Command> {
        let Some(artboard) = self.document.artboard(artboard_id) else { return Vec::new() };
        let Some(node) = artboard.node(node_id) else { return Vec::new() };
        let parent_world = match node.parent {
            Some(parent_id) => artboard.world_transform(parent_id),
            None => an1mbyt3_core::Transform2D::default(),
        };
        let new_world = parent_world.compose(&new_local);

        artboard
            .children_of(node_id)
            .flat_map(|(child_id, _)| {
                let old_child_world = artboard.world_transform(child_id);
                let new_child_local = new_world.local_for(&old_child_world);
                [
                    Command::SetTranslation { artboard: artboard_id, node: child_id, value: new_child_local.translation },
                    Command::SetRotation { artboard: artboard_id, node: child_id, value: new_child_local.rotation },
                    Command::SetScale { artboard: artboard_id, node: child_id, value: new_child_local.scale },
                ]
            })
            .collect()
    }

    /// Builds (without applying) the command `apply_transform_edit` would
    /// apply — same Animate-vs-Design mode gate as its doc comment describes,
    /// just returning the result instead of mutating `self.document`
    /// directly. Exists so `commit_node_scale` can build one such command
    /// per property (Translation *and* Scale both move during an
    /// anchor-corner drag) and batch them together, rather than only ever
    /// producing exactly one command per call the way every single-property
    /// caller (`apply_transform_edit`, `commit_node_rotation`) does.
    fn build_transform_edit_command(&self, artboard_id: ArtboardId, node_id: NodeId, property: PropertyKind, value: KeyframeValue, plain_command: Command) -> Command {
        if self.animate_mode_active {
            self.keyframe_or_plain(artboard_id, node_id, property, value, plain_command)
        } else {
            self.shift_or_plain(artboard_id, node_id, property, value, plain_command)
        }
    }

    /// Adds a new shape node to `artboard` with a preset default path
    /// (`kind`: `"Rectangle"`/`"Ellipse"`/`"Triangle"`/`"Star"` — see
    /// `preset_shape_path`) — the Shapes panel's quick-drop-in buttons, and
    /// the first concrete instance of `Command::AddNode` (a real, undoable
    /// creation, per that variant's doc comment). Cascades each new shape's
    /// starting position diagonally by however many shapes already exist, so
    /// repeatedly dropping in shapes doesn't stack every one exactly on top
    /// of the last — the designer drags it wherever they actually want
    /// afterward, same as any node.
    ///
    /// Returns both the updated `DocumentSummary` and the new node's id (the
    /// frontend's `OpaqueId` shape) so the caller can select it immediately,
    /// matching how a design tool typically selects whatever it just
    /// created. The id comes from `Command::AddNode`'s own inverse — see
    /// that variant's doc comment for why it's the only place `slotmap`
    /// hands back the freshly assigned `NodeId`.
    /// `parent` (an `OpaqueId`, or `null`/`undefined`) is the Layers panel's
    /// per-group "Add child" button — the same shape-creation path the
    /// Shapes menu's always-root-level "Add Shape" already used, just
    /// parented under a specific group instead of always the artboard root.
    /// The new shape's transform is in *local* (parent-relative) space
    /// either way, so a cascading offset relative to *that parent's* own
    /// existing children (not the whole artboard's shape count) is what
    /// keeps repeatedly adding children to the same group from stacking them
    /// exactly on top of each other, same reasoning as the root-level case.
    pub fn add_shape_node(&mut self, artboard: JsValue, kind: &str, parent: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let parent_id: Option<NodeId> = parse_id(parent)?;
        let path = preset_shape_path(kind)?;
        let artboard_ref = self.document.artboard(artboard_id).ok_or_else(|| JsValue::from_str("add_shape_node: unknown artboard"))?;
        let cascade = match parent_id {
            Some(p) => artboard_ref.children_of(p).count() as f32,
            None => artboard_ref.shapes().count() as f32,
        };

        let mut node = Node::new(kind, parent_id, NodeKind::Shape(ShapeNode { path, fill_color: [0.4, 0.6, 0.9, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None }));
        node.transform.translation = Vec2::new(20.0 * cascade, -20.0 * cascade);

        let inverse = self.document.apply(Command::AddNode { artboard: artboard_id, node });
        let Command::SetDeleted { node: new_node, .. } = inverse else {
            unreachable!("Command::AddNode's inverse is always a SetDeleted")
        };

        to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node })
    }

    /// Creates a new `ParticleSystem` node with every field at its authored
    /// default (`ParticleSystemNode::default()` — a gentle upward cone
    /// spray, 10 particles/sec, no color/size/rotation-over-lifetime curves
    /// yet) — the Shapes menu's "Add Particle System" entry. Same cascade-
    /// position convention as `add_shape_node`.
    pub fn add_particle_system_node(&mut self, artboard: JsValue, parent: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let parent_id: Option<NodeId> = parse_id(parent)?;
        let artboard_ref = self.document.artboard(artboard_id).ok_or_else(|| JsValue::from_str("add_particle_system_node: unknown artboard"))?;
        let cascade = match parent_id {
            Some(p) => artboard_ref.children_of(p).count() as f32,
            None => artboard_ref.shapes().count() as f32,
        };

        let mut node = Node::with_components("Particle System", parent_id, vec![an1mbyt3_core::NodeBehavior::ParticleSystem(an1mbyt3_core::ParticleSystemNode::default())]);
        node.transform.translation = Vec2::new(20.0 * cascade, -20.0 * cascade);

        let inverse = self.document.apply(Command::AddNode { artboard: artboard_id, node });
        let Command::SetDeleted { node: new_node, .. } = inverse else {
            unreachable!("Command::AddNode's inverse is always a SetDeleted")
        };

        to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node })
    }

    /// The Pen tool's "start a brand-new shape from a blank canvas" entry
    /// point (Paint Asset Mode, Phase 1) — every other route to a `Shape`
    /// node (`add_shape_node`'s 4 presets, SVG/image import) always starts
    /// from a non-empty `Path`; this is the one exception. Identical shape
    /// to `add_shape_node` (same cascade-position convention, same
    /// `AddNodeResult`) except the path itself starts with zero commands —
    /// the Pen tool overlay appends `MoveTo`/`LineTo`/`CubicTo` as the
    /// author places anchors, then commits the whole finished path through
    /// the *existing* `set_shape_path`/`Command::SetShapePath` (the same
    /// preview/commit split `PathEditOverlay` already uses), so this method
    /// only ever needs to run once per new shape, not once per anchor.
    pub fn add_blank_shape_node(&mut self, artboard: JsValue, parent: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let parent_id: Option<NodeId> = parse_id(parent)?;
        let artboard_ref = self.document.artboard(artboard_id).ok_or_else(|| JsValue::from_str("add_blank_shape_node: unknown artboard"))?;
        let cascade = match parent_id {
            Some(p) => artboard_ref.children_of(p).count() as f32,
            None => artboard_ref.shapes().count() as f32,
        };

        let mut node = Node::new(
            "Path",
            parent_id,
            NodeKind::Shape(ShapeNode { path: Path { commands: Vec::new() }, fill_color: [0.4, 0.6, 0.9, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None }),
        );
        node.transform.translation = Vec2::new(20.0 * cascade, -20.0 * cascade);

        let inverse = self.document.apply(Command::AddNode { artboard: artboard_id, node });
        let Command::SetDeleted { node: new_node, .. } = inverse else {
            unreachable!("Command::AddNode's inverse is always a SetDeleted")
        };

        to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node })
    }

    /// Imports `xml` (the contents of a dropped-in `.svg` file) onto
    /// `artboard` — the toolbar/Shapes panel's "Import SVG" action. Each
    /// `an1mbyt3_svg::ImportedNode` becomes one real `NodeKind::Shape`/
    /// `NodeKind::Group` document node via `insert_imported_node`, preserving
    /// the source `<g>` hierarchy (real `Node::parent` links, not flattened
    /// siblings) and any clip-path/mask relationship as a real
    /// `Command::SetGroupMask` — see that crate's module doc comment for why.
    /// Every node's `AddNode` (and any `SetGroupMask`) is applied one at a
    /// time via the raw `Command::apply` (not `self.document.apply`, which
    /// would push one undo entry *per node*) precisely because a child's
    /// `Node::parent`/a mask's target both need a *previous* insertion's
    /// freshly assigned `NodeId` — a plain `Command::Batch` can't express
    /// that dependency, since its whole `Vec<Command>` has to be built
    /// before any of it applies (see `Document::commit_batch`'s doc
    /// comment). The collected inverses are folded into one combined undo
    /// step at the end via `commit_batch`, so importing a whole SVG is still
    /// exactly one undo, not one per node. Fails outright only if the file
    /// itself can't be read as SVG at all, or parses to zero importable
    /// nodes (an empty/unsupported-only file) — anything an individual
    /// element gets wrong is already handled by `import_svg` itself skipping
    /// it, per that crate's own doc comment.
    pub fn import_svg(&mut self, artboard: JsValue, xml: &str) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let nodes = an1mbyt3_svg::import_svg(xml).map_err(|err| JsValue::from_str(&format!("failed to import SVG: {err}")))?;
        if nodes.is_empty() {
            return Err(JsValue::from_str("import_svg: no importable shapes found in this file"));
        }

        let mut inverses = Vec::new();
        let mut new_nodes = Vec::new();
        for node in nodes {
            insert_imported_node(&mut self.document, artboard_id, node, None, &mut inverses, &mut new_nodes);
        }
        inverses.reverse();
        self.document.commit_batch(inverses);

        to_js_value(&ImportSvgResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_nodes })
    }

    /// Imports a raster image file — PNG/JPEG create one `NodeKind::Image`
    /// node (`add_shape_node`'s own "select what you just created"
    /// convention); a GIF creates N `ImageAsset`s + one `SpriteGroup` and NO
    /// canvas node at all (frames-only this pass: each decoded frame is just
    /// an ordinary image, organized for the Resources panel — see
    /// `an1mbyt3_image::ImportedImage`'s doc comment for why building an
    /// auto-authored playback animation from it is deliberately out of scope
    /// here).
    ///
    /// `async`, unlike `import_svg`: every freshly added image asset is
    /// baked (`bake_and_cache_image`) before this returns, so it's already
    /// visible on the very first frame after the JS caller's `await`
    /// resolves — same reasoning `load_tea`/`load_teab` document.
    pub async fn import_image(&mut self, artboard: JsValue, filename: &str, bytes: &[u8]) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        self.import_image_inner(Some(artboard_id), filename, bytes).await
    }

    /// Places an existing image asset as a new sprite (`Image` node) with its
    /// center at `(x, y)` in `parent`'s space (`null` = scene root), at the
    /// image's own pixel size — dragging an image from Resources onto the
    /// canvas. Undoable. Returns `{ document, newNode }`.
    pub fn add_image_node(&mut self, artboard: JsValue, parent: JsValue, image: JsValue, x: f32, y: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let parent_id: Option<NodeId> = parse_id(parent)?;
        let image_id: an1mbyt3_core::ImageId = parse_id(image)?;
        let asset = self.document.image(image_id).ok_or_else(|| JsValue::from_str("add_image_node: unknown image"))?;
        let (width, height) = an1mbyt3_image::png_dimensions(&asset.source).unwrap_or((32, 32));
        let name = asset.name.rsplit_once('.').map_or(asset.name.as_str(), |(stem, _)| stem).to_string();
        let mut node = Node::new(
            name,
            parent_id,
            NodeKind::Image(an1mbyt3_core::ImageNode { image: image_id, width: width as f32, height: height as f32, fill_color: [1.0; 4], blend_mode: Default::default() }),
        );
        node.transform.translation = glam::Vec2::new(x, y);
        let inverse = self.document.apply(Command::AddNode { artboard: artboard_id, node });
        let Command::SetDeleted { node: new_node, .. } = inverse else { unreachable!("Command::AddNode's inverse is always a SetDeleted") };
        to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node })
    }

    /// Image Effects' "Cut out subject": a copy of Image node `node` drawing
    /// `image` (the cut-out) instead — same parent, transform and size —
    /// added on top, so the subject can sit in front of things placed
    /// between the two (text behind a person). Undoable. Returns
    /// `{ document, newNode }`.
    pub fn add_cutout_layer(&mut self, artboard: JsValue, node: JsValue, image: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let image_id: an1mbyt3_core::ImageId = parse_id(image)?;
        let source = self.document.artboard(artboard_id).and_then(|a| a.node(node_id)).ok_or_else(|| JsValue::from_str("add_cutout_layer: unknown node"))?;
        let NodeKind::Image(img) = &source.kind else { return Err(JsValue::from_str("add_cutout_layer: not an image node")) };
        let mut copy = Node::new(format!("{} cutout", source.name), source.parent, NodeKind::Image(an1mbyt3_core::ImageNode { image: image_id, ..img.clone() }));
        copy.transform = source.transform;
        copy.z_index = source.z_index;
        let inverse = self.document.apply(Command::AddNode { artboard: artboard_id, node: copy });
        let Command::SetDeleted { node: new_node, .. } = inverse else { unreachable!("Command::AddNode's inverse is always a SetDeleted") };
        to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node })
    }

    /// `import_image` without placing anything on the canvas — the asset
    /// (or a GIF's frames + sprite group) only. Used when a file appears in
    /// the project's `Assets/` folder on disk (Unity-style drop-in import).
    pub async fn import_image_asset(&mut self, filename: &str, bytes: &[u8]) -> Result<JsValue, JsValue> {
        self.import_image_inner(None, filename, bytes).await
    }

    async fn import_image_inner(&mut self, artboard_id: Option<ArtboardId>, filename: &str, bytes: &[u8]) -> Result<JsValue, JsValue> {
        let imported = an1mbyt3_image::import_image(filename, bytes).map_err(|err| JsValue::from_str(&format!("failed to import image: {err}")))?;

        let (new_node, new_images, new_sprite_group) = match imported {
            an1mbyt3_image::ImportedImage::Single { name, bytes, width, height } => {
                self.document.touch();
                let image_id = self.document.add_image(an1mbyt3_core::ImageAsset { name, source: bytes, processing: Default::default(), file: None });
                self.bake_and_cache_image(image_id).await;
                let Some(artboard_id) = artboard_id else {
                    return to_js_value(&ImportImageResult {
                        document: summarize_document(&self.document, self.active_animation, self.active_state_machine),
                        new_node: None,
                        new_images: vec![image_id],
                        new_sprite_group: None,
                    });
                };

                let node = Node::new(
                    "Image",
                    None,
                    NodeKind::Image(an1mbyt3_core::ImageNode {
                        image: image_id,
                        width: width as f32,
                        height: height as f32,
                        fill_color: [1.0; 4],
                        blend_mode: Default::default(),
                    }),
                );
                let inverse = self.document.apply(Command::AddNode { artboard: artboard_id, node });
                let Command::SetDeleted { node: new_node_id, .. } = inverse else {
                    unreachable!("Command::AddNode's inverse is always a SetDeleted")
                };
                (Some(new_node_id), vec![image_id], None)
            }
            an1mbyt3_image::ImportedImage::AnimatedFrames { name, frames } => {
                let mut new_images = Vec::with_capacity(frames.len());
                let mut group_frames = Vec::with_capacity(frames.len());
                for (i, frame) in frames.into_iter().enumerate() {
                    self.document.touch();
                    let image_id = self.document.add_image(an1mbyt3_core::ImageAsset {
                        name: format!("{name} {i}"),
                        source: frame.bytes,
                        processing: Default::default(),
                        file: None,
                    });
                    self.bake_and_cache_image(image_id).await;
                    group_frames.push((image_id, frame.delay_ms));
                    new_images.push(image_id);
                }
                let group_id = self.document.add_sprite_group(an1mbyt3_core::SpriteGroup { name, frames: group_frames });
                (None, new_images, Some(group_id))
            }
        };

        to_js_value(&ImportImageResult {
            document: summarize_document(&self.document, self.active_animation, self.active_state_machine),
            new_node,
            new_images,
            new_sprite_group,
        })
    }

    /// Creates a new `Tilemap` node from a tileset image file — imports
    /// `bytes` as a fresh `ImageAsset` (same decode/re-encode/bake path
    /// `import_image`'s own `Single` branch uses) and builds an empty grid
    /// of `map_width x map_height` cells referencing it, ready for the
    /// Tilemap panel to paint into via `set_tile`. `async` for the same
    /// reason `import_image`/`load_ninja_demo` are: the tileset needs
    /// baking (GPU upload) before this resolves, so it's already visible
    /// the moment the caller's `await` does.
    pub async fn add_tilemap_node(
        &mut self,
        artboard: JsValue,
        parent: JsValue,
        filename: &str,
        bytes: &[u8],
        tile_width_px: u32,
        tile_height_px: u32,
        map_width: u32,
        map_height: u32,
        cell_size: f32,
    ) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let parent_id: Option<NodeId> = parse_id(parent)?;
        let imported = an1mbyt3_image::import_image(filename, bytes).map_err(|err| JsValue::from_str(&format!("failed to import tileset: {err}")))?;
        let an1mbyt3_image::ImportedImage::Single { name, bytes, width, height } = imported else {
            return Err(JsValue::from_str("add_tilemap_node: tileset must be a single still image, not a GIF"));
        };
        self.document.touch();
        let image_id = self.document.add_image(an1mbyt3_core::ImageAsset { name, source: bytes, processing: Default::default(), file: None });
        self.bake_and_cache_image(image_id).await;

        let tilemap = an1mbyt3_core::TilemapNode::empty(image_id, width, height, tile_width_px, tile_height_px, map_width, map_height, cell_size);
        let node = Node::new("Tilemap", parent_id, NodeKind::Tilemap(tilemap));
        let inverse = self.document.apply(Command::AddNode { artboard: artboard_id, node });
        let Command::SetDeleted { node: new_node, .. } = inverse else {
            unreachable!("Command::AddNode's inverse is always a SetDeleted")
        };

        to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node })
    }

    /// Paints (`Some`) or erases (`None`) one cell of a `Tilemap` node — the
    /// Tilemap panel's paint tool. One undoable edit per cell (`Command::SetTile`'s
    /// own doc comment).
    pub fn set_tile(&mut self, artboard: JsValue, node: JsValue, x: u32, y: u32, value: Option<u32>) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        self.document.apply(Command::SetTile { artboard: artboard_id, node: node_id, x, y, value });
        self.document_summary()
    }

    /// Resizes a `Tilemap` node's grid — the Inspector's "Map Width"/"Map
    /// Height" fields. Computes the new (overlap-preserving) tile data here
    /// via a scratch clone of `TilemapNode::resize`, then applies it as one
    /// `Command::SetTilemapGrid` — see that command's own doc comment for
    /// why the command itself has to carry the full new `tiles`, not just
    /// the new size.
    pub fn resize_tilemap(&mut self, artboard: JsValue, node: JsValue, map_width: u32, map_height: u32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let artboard_ref = self.document.artboard(artboard_id).ok_or_else(|| JsValue::from_str("resize_tilemap: unknown artboard"))?;
        let node_ref = artboard_ref.node(node_id).ok_or_else(|| JsValue::from_str("resize_tilemap: unknown node"))?;
        let NodeKind::Tilemap(tilemap) = &node_ref.kind else {
            return Err(JsValue::from_str("resize_tilemap: node is not a Tilemap"));
        };
        let mut scratch = tilemap.clone();
        scratch.resize(map_width, map_height);
        self.document.apply(Command::SetTilemapGrid { artboard: artboard_id, node: node_id, map_width, map_height, tiles: scratch.tiles });
        self.document_summary()
    }

    /// Changes a `Tilemap` node's on-canvas cell size — the Inspector's
    /// "Cell Size" field.
    pub fn set_tilemap_cell_size(&mut self, artboard: JsValue, node: JsValue, value: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        self.document.apply(Command::SetTilemapCellSize { artboard: artboard_id, node: node_id, value });
        self.document_summary()
    }

    /// Returns an image asset's raw (always-PNG, see `an1mbyt3_image`'s
    /// module doc) source bytes — the Resources panel's thumbnail rendering
    /// wraps this directly in a `Blob`/object URL for an `<img>` tag, no
    /// RGBA/canvas round-trip or Rust-side re-encoding needed since the
    /// bytes are already a real, displayable PNG.
    pub fn get_image_source(&self, image: JsValue) -> Result<Vec<u8>, JsValue> {
        let image_id: an1mbyt3_core::ImageId = parse_id(image)?;
        self.document
            .image(image_id)
            .map(|asset| asset.source.clone())
            .ok_or_else(|| JsValue::from_str("get_image_source: unknown image"))
    }

    /// Updates an image asset's pre-processing params (resize/filter/effect)
    /// — the Inspector's asset-mode fields, committed the same "whole struct,
    /// not per-field" way `Command::SetPostEffects` already is. A direct
    /// document mutation, not a `Command`: asset content (this, `add_image`,
    /// `add_font`) sits outside the undo/redo system entirely, same
    /// established precedent `FontAsset` already set — only *node placement*
    /// is undoable. `async` for the same reason `import_image` is: the
    /// edited asset is re-baked (replacing its stale `image_cache` entry)
    /// before this returns.
    pub async fn set_image_processing(&mut self, image: JsValue, processing: JsValue) -> Result<JsValue, JsValue> {
        let image_id: an1mbyt3_core::ImageId = parse_id(image)?;
        let processing: an1mbyt3_core::ImageProcessing =
            serde_wasm_bindgen::from_value(processing).map_err(|err| JsValue::from_str(&format!("invalid image processing params: {err}")))?;

        let asset = self
            .document
            .image_mut(image_id)
            .ok_or_else(|| JsValue::from_str("set_image_processing: unknown image"))?;
        asset.processing = processing;
        self.document.touch();

        self.image_cache.remove(&image_id);
        self.bake_and_cache_image(image_id).await;

        self.document_summary()
    }

    /// Creates a real `Group` node around the selected nodes, preserving
    /// every child's world-space pose as it moves under the new parent. This
    /// is the editor-facing counterpart to SVG import's `<g>` preservation:
    /// grouping is a first-class document edit, not just a frontend selection
    /// trick. Ancestor/descendant duplicates are collapsed to the topmost
    /// selected nodes so grouping a selected group plus one of its children
    /// does not pull the child out twice.
    pub fn group_nodes(&mut self, artboard: JsValue, nodes: JsValue, name: String) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let requested: Vec<NodeId> = parse_id(nodes)?;
        let artboard_ref = self
            .document
            .artboard(artboard_id)
            .ok_or_else(|| JsValue::from_str("group_nodes: unknown artboard"))?;

        let mut selected = Vec::new();
        for node_id in requested {
            if selected.contains(&node_id) || artboard_ref.node(node_id).is_none() {
                continue;
            }
            selected.push(node_id);
        }
        if selected.len() < 2 {
            return Err(JsValue::from_str("group_nodes: select at least two nodes"));
        }

        let topmost: Vec<NodeId> = selected
            .iter()
            .copied()
            .filter(|&node_id| {
                let mut cursor = artboard_ref.node(node_id).and_then(|node| node.parent);
                while let Some(parent) = cursor {
                    if selected.contains(&parent) {
                        return false;
                    }
                    cursor = artboard_ref.node(parent).and_then(|node| node.parent);
                }
                true
            })
            .collect();
        if topmost.len() < 2 {
            return Err(JsValue::from_str("group_nodes: selection is already contained by another selected node"));
        }

        let first_parent = artboard_ref.node(topmost[0]).and_then(|node| node.parent);
        let common_parent = topmost
            .iter()
            .all(|&node_id| artboard_ref.node(node_id).and_then(|node| node.parent) == first_parent)
            .then_some(first_parent)
            .flatten();

        let center = topmost
            .iter()
            .map(|&node_id| artboard_ref.world_transform(node_id).translation)
            .fold(Vec2::ZERO, |sum, point| sum + point)
            / topmost.len() as f32;
        let parent_world = match common_parent {
            Some(parent_id) => artboard_ref.world_transform(parent_id),
            None => an1mbyt3_core::Transform2D::default(),
        };
        let group_world = an1mbyt3_core::Transform2D { translation: center, ..Default::default() };
        let group_local = parent_world.local_for(&group_world);

        let mut group = Node::new(name, common_parent, NodeKind::Group(Default::default()));
        group.transform = group_local;

        let mut inverses = Vec::new();
        let inverse = Command::AddNode { artboard: artboard_id, node: group }.apply(&mut self.document);
        let Command::SetDeleted { node: group_id, .. } = inverse else {
            unreachable!("Command::AddNode's inverse is always a SetDeleted")
        };
        inverses.push(inverse);

        for node_id in topmost {
            let artboard_ref = self.document.artboard(artboard_id).unwrap();
            let old_world = artboard_ref.world_transform(node_id);
            let group_world = artboard_ref.world_transform(group_id);
            let new_local = group_world.local_for(&old_world);
            inverses.push(Command::ReparentNode { artboard: artboard_id, node: node_id, value: Some(group_id) }.apply(&mut self.document));
            inverses.push(Command::SetTranslation { artboard: artboard_id, node: node_id, value: new_local.translation }.apply(&mut self.document));
            inverses.push(Command::SetRotation { artboard: artboard_id, node: node_id, value: new_local.rotation }.apply(&mut self.document));
            inverses.push(Command::SetScale { artboard: artboard_id, node: node_id, value: new_local.scale }.apply(&mut self.document));
        }

        inverses.reverse();
        self.document.commit_batch(inverses);

        to_js_value(&AddNodeResult {
            document: summarize_document(&self.document, self.active_animation, self.active_state_machine),
            new_node: group_id,
        })
    }

    /// Duplicates every node in `nodes` (each with its whole subtree) in
    /// place — same parent, same transform, no positional offset — the
    /// canvas/Layers multi-select's "Paste in Place" action (`Ctrl+V`, after
    /// `Ctrl+C` remembers the current selection client-side; there's no real
    /// OS clipboard/cross-app paste involved, `App.tsx` just re-invokes this
    /// with whatever `Ctrl+C` last recorded). Ancestor/descendant duplicates
    /// in `nodes` collapse to the topmost selected ones first, same "don't
    /// duplicate a child twice — once directly, once as part of its own
    /// selected ancestor's subtree clone" reasoning `group_nodes` already
    /// established. Each topmost node's subtree is cloned via the same
    /// "snapshot node + child ids, then one `Command::AddNode` at a time,
    /// remapping ids through a map" shape `insert_imported_node` uses for a
    /// *different*-document import — this is that same shape applied within
    /// one document instead (`duplicate_node_recursive`). A duplicate's
    /// `Node::parent` is set to the *original* node's own parent (not
    /// `None`/some synthetic wrapper), so the copy lands as a plain sibling —
    /// stacked exactly on top, since later `root_nodes`/`child_order` entries
    /// draw on top and its `Transform2D` is an exact clone, not offset at
    /// all. Returns each topmost duplicate's own new id, in the same order
    /// `nodes` was given, so the caller can select exactly the freshly pasted
    /// copies.
    pub fn duplicate_nodes(&mut self, artboard: JsValue, nodes: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let requested: Vec<NodeId> = parse_id(nodes)?;
        let artboard_ref = self
            .document
            .artboard(artboard_id)
            .ok_or_else(|| JsValue::from_str("duplicate_nodes: unknown artboard"))?;

        let mut selected = Vec::new();
        for node_id in requested {
            if selected.contains(&node_id) || artboard_ref.node(node_id).is_none() {
                continue;
            }
            selected.push(node_id);
        }
        if selected.is_empty() {
            return Err(JsValue::from_str("duplicate_nodes: nothing to duplicate"));
        }

        let topmost: Vec<NodeId> = selected
            .iter()
            .copied()
            .filter(|&node_id| {
                let mut cursor = artboard_ref.node(node_id).and_then(|node| node.parent);
                while let Some(parent) = cursor {
                    if selected.contains(&parent) {
                        return false;
                    }
                    cursor = artboard_ref.node(parent).and_then(|node| node.parent);
                }
                true
            })
            .collect();

        let mut inverses = Vec::new();
        let mut new_nodes = Vec::new();
        let mut top_level_new_ids = Vec::new();
        let mut id_map: HashMap<NodeId, NodeId> = HashMap::new();

        for node_id in topmost {
            let parent = self.document.artboard(artboard_id).unwrap().node(node_id).unwrap().parent;
            let before = new_nodes.len();
            duplicate_node_recursive(&mut self.document, artboard_id, node_id, parent, &mut id_map, &mut inverses, &mut new_nodes);
            top_level_new_ids.push(new_nodes[before]);
        }

        // `duplicate_node_recursive`'s own `.clone()` copies a `Group`'s
        // `mask` field verbatim — still pointing at the *original* subtree's
        // mask child, not this duplicate's own freshly-cloned one. Left
        // unfixed, every duplicate of a masked group clips using the
        // original's own mask shape (both copies' content ends up sharing
        // one physical mask instead of each clipping against its own), the
        // real, hand-confirmed bug report this fixes — not specific to any
        // one node kind (`ParticleSystem` included), since masking is
        // driven by the clipping *group*'s own `mask` reference, not by
        // whatever's actually inside it. Only remaps a mask that was itself
        // part of this same duplicated subtree (`id_map` only ever contains
        // those) — a mask pointing outside it (already an unusual, perhaps
        // never-authored case) is left alone, same as before this fix.
        for &new_id in &new_nodes {
            let Some(an1mbyt3_core::NodeKind::Group(group)) = self.document.artboard(artboard_id).unwrap().node(new_id).map(|n| &n.kind) else {
                continue;
            };
            let Some(old_mask) = group.mask else { continue };
            let Some(&new_mask) = id_map.get(&old_mask) else { continue };
            let mask_inverse = Command::SetGroupMask { artboard: artboard_id, node: new_id, value: Some(new_mask) }.apply(&mut self.document);
            inverses.push(mask_inverse);
        }

        inverses.reverse();
        self.document.commit_batch(inverses);

        to_js_value(&DuplicateNodesResult {
            document: summarize_document(&self.document, self.active_animation, self.active_state_machine),
            new_nodes: top_level_new_ids,
        })
    }

    /// Moves `node` to a new parent (`parent`: an `OpaqueId`, or `null`/
    /// `undefined` for the artboard's root) — the Layers panel's
    /// drag-to-reparent action, going through `Command::ReparentNode`.
    /// Rejects a move that would parent `node` under itself or under one of
    /// its own descendants — `Artboard::reparent_node`/`Command::ReparentNode`
    /// don't check this themselves (see their own doc comments: it's a
    /// caller-bug-not-recoverable primitive), so it happens here, the one
    /// place a reparent can actually be requested interactively, before it
    /// ever reaches the document and turns `world_transform`'s parent-chain
    /// walk into infinite recursion.
    ///
    /// Also preserves `node`'s on-canvas position/rotation/scale across the
    /// move: `Command::ReparentNode` only changes `Node::parent`, it doesn't
    /// touch `Node::transform` — since a node's `Transform2D` is always
    /// relative to whichever parent it currently has, leaving it as-is would
    /// silently *reinterpret* the same numbers as relative to the *new*
    /// parent instead, which visibly jumps/spins the node the moment its new
    /// parent isn't sitting at the identity transform (e.g. dragging a shape
    /// authored at world (40, -40) onto a group sitting at (200, 100) would
    /// otherwise land it at world (240, 60)). Computes `node`'s current world
    /// transform, the new parent's world transform (identity for a move to
    /// root), and `Transform2D::local_for` the local transform that
    /// preserves it — batched with the `ReparentNode` itself as one undo step
    /// (`solve_two_bone_ik`/`commit_node_scale` already establish this
    /// "derive values, batch several primitives as one user action" shape).
    /// `reparent_node`'s positional counterpart — the Layers panel's actual
    /// drag-to-*reorder* gesture (dropping a dragged layer's top half onto
    /// another row — see `LayersPanel.tsx`'s own `dragOverZone` doc comment).
    /// `reparent_node` alone always lands `node` at the very *end* of its
    /// new parent's children (`Artboard::reparent_node`'s own doc comment),
    /// which made every reorder-within-the-same-parent drag either silently
    /// no-op (if `node` was already last) or jump to an unexpected spot —
    /// indistinguishable from "dragging doesn't do anything." This instead
    /// positions `node` immediately before `before` (or at the end, if
    /// `before` is `null`/not actually among the resulting siblings).
    ///
    /// Same cycle guard and world-position-preserving batch as
    /// `reparent_node` itself (see that method's own doc comment) — this
    /// isn't just "the same thing plus an index," it's meant to fully
    /// replace it for any caller that has a specific drop position in mind.
    pub fn reparent_node_before(&mut self, artboard: JsValue, node: JsValue, parent: JsValue, before: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let new_parent: Option<NodeId> = parse_id(parent)?;
        let before_id: Option<NodeId> = parse_id(before)?;

        if let Some(candidate) = new_parent {
            if candidate == node_id {
                return Err(JsValue::from_str("reparent_node_before: a node cannot be its own parent"));
            }
            let artboard_ref = self.document.artboard(artboard_id).ok_or_else(|| JsValue::from_str("reparent_node_before: unknown artboard"))?;
            let mut cursor = Some(candidate);
            while let Some(current) = cursor {
                if current == node_id {
                    return Err(JsValue::from_str("reparent_node_before: would create a cycle"));
                }
                cursor = artboard_ref.node(current).and_then(|n| n.parent);
            }
        }

        let artboard_ref = self.document.artboard(artboard_id).ok_or_else(|| JsValue::from_str("reparent_node_before: unknown artboard"))?;
        let old_world = artboard_ref.world_transform(node_id);
        let new_parent_world = match new_parent {
            Some(parent_id) => artboard_ref.world_transform(parent_id),
            None => an1mbyt3_core::Transform2D::default(),
        };
        let new_local = new_parent_world.local_for(&old_world);

        // `node` itself is filtered out before searching — it may already be
        // one of `new_parent`'s children (a same-parent reorder), and its
        // *current* position must not count when resolving `before`'s index
        // (matching `reparent_node_at`'s own "remove first, then insert"
        // order — see that method's doc comment for why this filter exists).
        let siblings = artboard_ref.sibling_order(new_parent);
        let index = match before_id {
            Some(before_node) => siblings.iter().filter(|&&id| id != node_id).position(|&id| id == before_node).unwrap_or(siblings.len()),
            None => siblings.len(),
        };

        self.document.apply(Command::Batch(vec![
            Command::ReparentNodeAt { artboard: artboard_id, node: node_id, parent: new_parent, index },
            Command::SetTranslation { artboard: artboard_id, node: node_id, value: new_local.translation },
            Command::SetRotation { artboard: artboard_id, node: node_id, value: new_local.rotation },
            Command::SetScale { artboard: artboard_id, node: node_id, value: new_local.scale },
        ]));
        self.document_summary()
    }

    pub fn reparent_node(&mut self, artboard: JsValue, node: JsValue, parent: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let new_parent: Option<NodeId> = parse_id(parent)?;

        if let Some(candidate) = new_parent {
            if candidate == node_id {
                return Err(JsValue::from_str("reparent_node: a node cannot be its own parent"));
            }
            let artboard_ref = self.document.artboard(artboard_id).ok_or_else(|| JsValue::from_str("reparent_node: unknown artboard"))?;
            let mut cursor = Some(candidate);
            while let Some(current) = cursor {
                if current == node_id {
                    return Err(JsValue::from_str("reparent_node: would create a cycle"));
                }
                cursor = artboard_ref.node(current).and_then(|n| n.parent);
            }
        }

        let artboard_ref = self.document.artboard(artboard_id).ok_or_else(|| JsValue::from_str("reparent_node: unknown artboard"))?;
        let old_world = artboard_ref.world_transform(node_id);
        let new_parent_world = match new_parent {
            Some(parent_id) => artboard_ref.world_transform(parent_id),
            None => an1mbyt3_core::Transform2D::default(),
        };
        let new_local = new_parent_world.local_for(&old_world);

        self.document.apply(Command::Batch(vec![
            Command::ReparentNode { artboard: artboard_id, node: node_id, value: new_parent },
            Command::SetTranslation { artboard: artboard_id, node: node_id, value: new_local.translation },
            Command::SetRotation { artboard: artboard_id, node: node_id, value: new_local.rotation },
            Command::SetScale { artboard: artboard_id, node: node_id, value: new_local.scale },
        ]));
        self.document_summary()
    }

    /// Sets (`mask`: an `OpaqueId`) or clears (`null`/`undefined`) which
    /// child clips the rest of a `Group` node's contents — the Layers
    /// panel's "use as mask" action, going through `Command::SetGroupMask`.
    /// Returns an error (rather than panicking via `Command`'s own "caller
    /// bug" convention) if `node` isn't a `Group` — this is reachable from
    /// an interactive drag, where the target row could in principle be
    /// anything, unlike every other `Set*` field editor here that's only
    /// ever offered on a selection already filtered to the right kind.
    pub fn set_group_mask(&mut self, artboard: JsValue, node: JsValue, mask: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let mask_id: Option<NodeId> = parse_id(mask)?;

        let is_group = self
            .document
            .artboard(artboard_id)
            .and_then(|a| a.node(node_id))
            .is_some_and(|n| matches!(n.kind, NodeKind::Group(_)));
        if !is_group {
            return Err(JsValue::from_str("set_group_mask: node is not a Group"));
        }

        self.document.apply(Command::SetGroupMask { artboard: artboard_id, node: node_id, value: mask_id });
        self.document_summary()
    }

    /// Moves `node` one step later in its draw-order list ("Bring Forward")
    /// — root-level if it has no parent, otherwise among its siblings under
    /// the same parent — going through `Command::MoveNodeForward`. A no-op
    /// (still returns the current summary, doesn't error) if `node` is
    /// already last, matching `Artboard::move_node_forward`'s own "harmless
    /// no-op at the boundary" behavior.
    pub fn move_node_forward(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        self.document.apply(Command::MoveNodeForward { artboard: artboard_id, node: node_id });
        self.document_summary()
    }

    /// `move_node_forward`'s mirror — one step earlier ("Send Backward").
    pub fn send_node_backward(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        self.document.apply(Command::SendNodeBackward { artboard: artboard_id, node: node_id });
        self.document_summary()
    }

    /// Moves `node` to the very end of its draw-order list ("Bring to
    /// Front") — going through `Command::BringNodeToFront`.
    pub fn bring_node_to_front(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        self.document.apply(Command::BringNodeToFront { artboard: artboard_id, node: node_id });
        self.document_summary()
    }

    /// `bring_node_to_front`'s mirror — moves to the very start of the list
    /// ("Send to Back").
    pub fn send_node_to_back(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        self.document.apply(Command::SendNodeToBack { artboard: artboard_id, node: node_id });
        self.document_summary()
    }

    /// Adds a new bone node to `artboard`, optionally parented under an
    /// existing bone (`parent`: an `OpaqueId`, or `null`/`undefined` for a
    /// root bone) — the Bones panel's "Add Bone" action, and the first real
    /// use of `Command::AddNode` with a non-`None` parent. When parented,
    /// positions the new bone's `translation` at its parent's own tip
    /// (`(parent_length, 0.0)` in the parent's local space — where a child
    /// bone attaches, per `BoneNode`'s doc comment), matching the canonical
    /// two-bone rig `an1mbyt3-constraints`'s `bone_ik` example hand-builds.
    /// A root bone starts at the artboard's origin, same "wherever you drag
    /// it after" convention `add_shape_node` already established.
    pub fn add_bone_node(&mut self, artboard: JsValue, parent: JsValue, length: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let parent_id: Option<NodeId> = parse_id(parent)?;

        let translation = match parent_id {
            Some(parent_id) => {
                let parent_node = self
                    .document
                    .artboard(artboard_id)
                    .and_then(|a| a.node(parent_id))
                    .ok_or_else(|| JsValue::from_str("add_bone_node: unknown parent"))?;
                let NodeKind::Bone(parent_bone) = &parent_node.kind else {
                    return Err(JsValue::from_str("add_bone_node: parent is not a Bone"));
                };
                Vec2::new(parent_bone.length, 0.0)
            }
            None => Vec2::ZERO,
        };

        let mut node = Node::new("Bone", parent_id, NodeKind::Bone(BoneNode { length }));
        node.transform.translation = translation;

        let inverse = self.document.apply(Command::AddNode { artboard: artboard_id, node });
        let Command::SetDeleted { node: new_node, .. } = inverse else {
            unreachable!("Command::AddNode's inverse is always a SetDeleted")
        };

        to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node })
    }

    /// Adds a new `Camera` node to `artboard`, optionally parented — the
    /// Shapes menu's "Add Camera" action. Undoable (`Command::AddNode`).
    /// Starts at `CameraNode::default()` (`zoom: 1.0`) and isn't
    /// automatically wired up as `active_camera` — see `set_active_camera`.
    pub fn add_camera_node(&mut self, artboard: JsValue, parent: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let parent_id: Option<NodeId> = parse_id(parent)?;

        let node = Node::with_components("Camera", parent_id, vec![an1mbyt3_core::NodeBehavior::Camera(an1mbyt3_core::CameraNode::default())]);
        let inverse = self.document.apply(Command::AddNode { artboard: artboard_id, node });
        let Command::SetDeleted { node: new_node, .. } = inverse else {
            unreachable!("Command::AddNode's inverse is always a SetDeleted")
        };

        to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node })
    }

    /// The + menu's "Text" item — a Text node using the document's first
    /// font (the bundled UI font is added the first time). Undoable.
    pub fn add_text_node(&mut self, artboard: JsValue, parent: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let parent_id: Option<NodeId> = parse_id(parent)?;
        let font = default_text_font(&mut self.document);
        let node = Node::new(
            "Text",
            parent_id,
            NodeKind::Text(TextNode { content: "Text".into(), font, size_px: 32.0, fill_color: [1.0, 1.0, 1.0, 1.0], gradient: None }),
        );
        let inverse = self.document.apply(Command::AddNode { artboard: artboard_id, node });
        let Command::SetDeleted { node: new_node, .. } = inverse else {
            unreachable!("Command::AddNode's inverse is always a SetDeleted")
        };
        to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node })
    }

    /// The + menu's "Light 2D" items — adds a Unity-style `Light2D` node of
    /// `light_type` (`"Global"`, `"Spot"`, `"Freeform"`, `"Sprite"`) with
    /// Unity's defaults. Undoable.
    pub fn add_light2d_node(&mut self, artboard: JsValue, parent: JsValue, light_type: &str) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let parent_id: Option<NodeId> = parse_id(parent)?;
        let kind = match light_type {
            "Global" => an1mbyt3_core::Light2DType::Global,
            "Spot" => an1mbyt3_core::Light2DType::Spot,
            "Freeform" => an1mbyt3_core::Light2DType::Freeform,
            "Sprite" => an1mbyt3_core::Light2DType::Sprite,
            other => return Err(JsValue::from_str(&format!("add_light2d_node: unknown light type {other}"))),
        };
        let name = format!("{light_type} Light 2D");
        let node = Node::with_components(name, parent_id, vec![an1mbyt3_core::NodeBehavior::Light2D(an1mbyt3_core::Light2DNode { light_type: kind, ..Default::default() })]);
        let inverse = self.document.apply(Command::AddNode { artboard: artboard_id, node });
        let Command::SetDeleted { node: new_node, .. } = inverse else {
            unreachable!("Command::AddNode's inverse is always a SetDeleted")
        };
        to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node })
    }

    /// The + menu's "Trail Renderer" item — adds a Unity-style `Trail` node
    /// (parent it under something that moves). Undoable.
    pub fn add_trail_node(&mut self, artboard: JsValue, parent: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let parent_id: Option<NodeId> = parse_id(parent)?;
        let node = Node::with_components("Trail", parent_id, vec![an1mbyt3_core::NodeBehavior::TrailRenderer(an1mbyt3_core::TrailNode::default())]);
        let inverse = self.document.apply(Command::AddNode { artboard: artboard_id, node });
        let Command::SetDeleted { node: new_node, .. } = inverse else {
            unreachable!("Command::AddNode's inverse is always a SetDeleted")
        };
        to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node })
    }

    /// Replaces a Light2D node's settings — the Inspector's Light 2D
    /// section (`value`: `dto::Light2DDto`). Undoable.
    pub fn set_light2d(&mut self, artboard: JsValue, node: JsValue, value: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let before = self.record_before(artboard_id, node_id);
        let value: crate::dto::Light2DDto = parse_id(value)?;
        self.document.apply(Command::SetLight2D { artboard: artboard_id, node: node_id, value: value.into() });
        self.record_field_edits(artboard_id, node_id, before);
        self.document_summary()
    }

    /// Replaces a Trail node's settings — the Inspector's Trail Renderer
    /// section (`value`: `dto::TrailDto`). Undoable.
    pub fn set_trail(&mut self, artboard: JsValue, node: JsValue, value: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let before = self.record_before(artboard_id, node_id);
        let value: crate::dto::TrailDto = parse_id(value)?;
        self.document.apply(Command::SetTrail { artboard: artboard_id, node: node_id, value: value.into() });
        self.record_field_edits(artboard_id, node_id, before);
        self.document_summary()
    }

    /// Adds/edits (`value`: `dto::ShadowCasterDto`) or removes (`null`) a
    /// node's Shadow Caster 2D. Undoable.
    pub fn set_shadow_caster(&mut self, artboard: JsValue, node: JsValue, value: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let before = self.record_before(artboard_id, node_id);
        let value: Option<crate::dto::ShadowCasterDto> = parse_id(value)?;
        self.document.apply(Command::SetShadowCaster { artboard: artboard_id, node: node_id, value: value.map(Into::into) });
        self.record_field_edits(artboard_id, node_id, before);
        self.document_summary()
    }

    /// Toggles `Node::unlit` (skip 2D lighting for this subtree). Undoable.
    pub fn set_node_unlit(&mut self, artboard: JsValue, node: JsValue, value: bool) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let before = self.record_before(artboard_id, node_id);
        self.document.apply(Command::SetNodeUnlit { artboard: artboard_id, node: node_id, value });
        self.record_field_edits(artboard_id, node_id, before);
        self.document_summary()
    }

    /// Creates a new, empty scene (`Artboard`) — the Scenes switcher's
    /// "+ New Scene" action. Undoable (`Command::AddArtboard`). Returns the
    /// new artboard's id alongside the document so the frontend can switch
    /// straight to it.
    pub fn add_artboard(&mut self, name: String, width: f32, height: f32) -> Result<JsValue, JsValue> {
        let mut inverses = Vec::new();
        let inverse = self.document.apply(Command::AddArtboard { name, width, height });
        let Command::SetArtboardDeleted { artboard: new_artboard, .. } = inverse else {
            unreachable!("Command::AddArtboard's inverse is always a SetArtboardDeleted")
        };
        inverses.push(inverse);
        // Same starting contents as the default document: Main Camera +
        // Global Light 2D, one undo step together with the scene itself.
        let (camera, light) = default_scene_nodes();
        let camera_inverse = self.document.apply(Command::AddNode { artboard: new_artboard, node: camera });
        let Command::SetDeleted { node: camera_id, .. } = camera_inverse else { unreachable!("AddNode's inverse is always a SetDeleted") };
        inverses.push(camera_inverse);
        inverses.push(self.document.apply(Command::AddNode { artboard: new_artboard, node: light }));
        inverses.push(self.document.apply(Command::SetActiveCamera { artboard: new_artboard, value: Some(camera_id) }));
        inverses.reverse();
        self.document.commit_batch(inverses);
        to_js_value(&crate::dto::AddArtboardResult {
            document: summarize_document(&self.document, self.active_animation, self.active_state_machine),
            new_artboard,
        })
    }

    /// Soft-deletes a scene — the Scenes switcher's "Delete Scene" action.
    /// Undoable (`Command::SetArtboardDeleted`). Refuses to delete the last
    /// remaining scene: every editor code path assumes at least one artboard
    /// exists, and an empty document has nothing to switch back to.
    pub fn delete_artboard(&mut self, artboard: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        if self.document.artboards().count() <= 1 {
            return Err(JsValue::from_str("delete_artboard: can't delete the last remaining scene"));
        }
        self.document.apply(Command::SetArtboardDeleted { artboard: artboard_id, value: true });
        self.document_summary()
    }

    /// Renames a scene — the Scenes switcher's rename action. Undoable
    /// (`Command::SetArtboardName`).
    pub fn rename_artboard(&mut self, artboard: JsValue, name: String) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        self.document.apply(Command::SetArtboardName { artboard: artboard_id, value: name });
        self.document_summary()
    }

    /// Replaces a `Camera` node's whole config (zoom/background/follow) —
    /// the Inspector's Camera section. Undoable (`Command::SetCamera`).
    pub fn set_camera(&mut self, artboard: JsValue, node: JsValue, value: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let before = self.record_before(artboard_id, node_id);
        let value: crate::dto::CameraDto = parse_id(value)?;
        self.document.apply(Command::SetCamera { artboard: artboard_id, node: node_id, value: value.into() });
        self.record_field_edits(artboard_id, node_id, before);
        self.document_summary()
    }

    /// Names which `Camera` node (`null` clears it) Playground actually
    /// views `artboard` through — the Inspector's Camera section's "Set as
    /// Active Camera" action. Undoable. See `Artboard::active_camera`'s own
    /// doc comment for why this is saved scene state, not an editor-session
    /// preference.
    pub fn set_active_camera(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: Option<NodeId> = parse_id(node)?;
        self.document.apply(Command::SetActiveCamera { artboard: artboard_id, value: node_id });
        self.document_summary()
    }

    /// Toggles a `Group`'s "Y Sort" — the Inspector's checkbox,
    /// `GroupNode::y_sort_enabled`'s own doc comment for what it does.
    /// Undoable.
    pub fn set_group_y_sort(&mut self, artboard: JsValue, node: JsValue, value: bool) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        self.document.apply(Command::SetGroupYSort { artboard: artboard_id, node: node_id, value });
        self.document_summary()
    }

    /// Changes a node's `z_index` — the Inspector's "Z Index" field, any
    /// node kind. Undoable.
    pub fn set_node_z_index(&mut self, artboard: JsValue, node: JsValue, value: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let before = self.record_before(artboard_id, node_id);
        self.document.apply(Command::SetNodeZIndex { artboard: artboard_id, node: node_id, value });
        self.record_field_edits(artboard_id, node_id, before);
        self.document_summary()
    }

    /// Creates a new [`ComponentDefinition`] by copying `node`'s path/fill
    /// color (`node` must be a `NodeKind::Shape`) — the Components panel's
    /// "Make Component" action. Leaves the source shape untouched (it stays
    /// an ordinary, independent `Shape` node); this only *bundles a copy* of
    /// its geometry into a new, separately reusable definition. Undoable
    /// (`Command::AddComponent`) — its inverse is `SetComponentDeleted`, same
    /// "doesn't know the freshly assigned id until `slotmap` inserts" shape
    /// as `AddNode`/`AddStateMachine`.
    pub fn add_component(&mut self, artboard: JsValue, node: JsValue, name: String) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;

        let shape_node = self
            .document
            .artboard(artboard_id)
            .and_then(|a| a.node(node_id))
            .ok_or_else(|| JsValue::from_str("add_component: unknown node"))?;
        let NodeKind::Shape(shape) = &shape_node.kind else {
            return Err(JsValue::from_str("add_component: node is not a Shape"));
        };

        let definition = ComponentDefinition { name, path: shape.path.clone(), default_fill_color: shape.fill_color, variants: Vec::new(), deleted: false };
        self.document.apply(Command::AddComponent { definition });
        self.document_summary()
    }

    // ---- Prefabs (see `prefab_ops` and `an1mbyt3_core::prefab`) ----

    /// Inspector "Create Prefab": turns `node`'s subtree into a new prefab
    /// asset and makes it the first instance. Undoable. Returns
    /// `{ document, prefab }`.
    pub fn create_prefab(&mut self, artboard: JsValue, node: JsValue, name: String) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let (prefab, inverses) =
            crate::prefab_ops::create_prefab(&mut self.document, artboard_id, node_id, name).map_err(|e| JsValue::from_str(&format!("create_prefab: {e}")))?;
        self.document.commit_batch(inverses);
        to_js_value(&PrefabResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), prefab: Some(prefab), node: None })
    }

    /// Places an instance of `prefab` (Resources panel "Place") with its
    /// root at `(x, y)` in `parent`'s space (`parent: null` = scene root).
    /// Undoable. Returns `{ document, prefab, node }` (the new instance root).
    pub fn instantiate_prefab(&mut self, artboard: JsValue, prefab: JsValue, parent: JsValue, x: f32, y: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let prefab_id: an1mbyt3_core::PrefabId = parse_id(prefab)?;
        let parent_id: Option<NodeId> = parse_id(parent)?;
        let mut transform = self.document.prefab(prefab_id).and_then(|d| d.nodes.first()).map(|n| n.node.transform).unwrap_or_default();
        transform.translation = glam::Vec2::new(x, y);
        let (root, inverses) = crate::prefab_ops::instantiate(&mut self.document, artboard_id, prefab_id, parent_id, Some(transform))
            .map_err(|e| JsValue::from_str(&format!("instantiate_prefab: {e}")))?;
        self.document.commit_batch(inverses);
        to_js_value(&PrefabResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), prefab: Some(prefab_id), node: Some(root) })
    }

    /// The instance root `node` belongs to (`null` if it isn't inside a
    /// prefab instance) — the Inspector shows the prefab bar for any node
    /// of an instance, acting on its root.
    pub fn prefab_instance_root(&self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let root = self.document.artboard(artboard_id).and_then(|a| crate::prefab_ops::instance_root(&self.document, a, node_id));
        to_js_value(&root)
    }

    /// Inspector "Apply": instance -> prefab, then every other instance in
    /// every scene is re-synced. One undo step.
    pub fn apply_prefab_instance(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let root = self.require_instance_root(artboard_id, parse_id(node)?)?;
        let inverses = crate::prefab_ops::apply_instance(&mut self.document, artboard_id, root).map_err(|e| JsValue::from_str(&format!("apply_prefab_instance: {e}")))?;
        self.document.commit_batch(inverses);
        self.document_summary()
    }

    /// Inspector "Revert": prefab -> this instance. One undo step.
    pub fn revert_prefab_instance(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let root = self.require_instance_root(artboard_id, parse_id(node)?)?;
        let mut inverses = Vec::new();
        crate::prefab_ops::sync_instance(&mut self.document, artboard_id, root, &mut inverses);
        inverses.reverse();
        self.document.commit_batch(inverses);
        self.document_summary()
    }

    /// Inspector "Unpack": the instance becomes plain nodes. Undoable.
    pub fn unpack_prefab_instance(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let root = self.require_instance_root(artboard_id, parse_id(node)?)?;
        let inverses = crate::prefab_ops::unpack_instance(&mut self.document, artboard_id, root);
        self.document.commit_batch(inverses);
        self.document_summary()
    }

    fn require_instance_root(&self, artboard: ArtboardId, node: NodeId) -> Result<NodeId, JsValue> {
        self.document
            .artboard(artboard)
            .and_then(|a| crate::prefab_ops::instance_root(&self.document, a, node))
            .ok_or_else(|| JsValue::from_str("not part of a prefab instance"))
    }

    /// Resources panel "Delete" on a prefab. Instances stay as plain nodes
    /// (their links just stop resolving). Undoable.
    pub fn delete_prefab(&mut self, prefab: JsValue) -> Result<JsValue, JsValue> {
        let prefab_id: an1mbyt3_core::PrefabId = parse_id(prefab)?;
        self.document.apply(Command::SetPrefabDeleted { prefab: prefab_id, value: true });
        self.document_summary()
    }

    // ---- Drawings (Paint mode's "Save as Resource", see `an1mbyt3_core::drawing`) ----

    /// Saves a vector drawing (scratch-paint's SVG export) as a new Drawing
    /// resource. Validates that the SVG actually imports first, so a
    /// drawing that could never be placed is refused up front instead of
    /// failing later. Undoable. Returns `{ document, drawing }`.
    pub fn add_drawing(&mut self, name: String, svg: String) -> Result<JsValue, JsValue> {
        check_drawing_svg(&svg)?;
        let inverse = self.document.apply(Command::AddDrawing {
            drawing: an1mbyt3_core::DrawingAsset { name, content: an1mbyt3_core::DrawingContent::Vector { svg }, deleted: false },
        });
        let Command::SetDrawingDeleted { drawing, .. } = inverse else { unreachable!("AddDrawing's inverse is SetDrawingDeleted") };
        to_js_value(&crate::dto::DrawingResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), drawing })
    }

    /// Saves a bitmap-mode drawing (PNG bytes) as a new Drawing resource,
    /// backed by a new image asset (see `DrawingContent::Bitmap`). The
    /// drawing itself is undoable; the image asset, like every imported
    /// image, is not (undo leaves an unused image behind, harmlessly).
    /// Returns `{ document, drawing }`.
    pub async fn add_bitmap_drawing(&mut self, name: String, png: &[u8]) -> Result<JsValue, JsValue> {
        let image = self.add_drawing_image(&name, png).await?;
        let inverse = self.document.apply(Command::AddDrawing {
            drawing: an1mbyt3_core::DrawingAsset { name, content: an1mbyt3_core::DrawingContent::Bitmap { image }, deleted: false },
        });
        let Command::SetDrawingDeleted { drawing, .. } = inverse else { unreachable!("AddDrawing's inverse is SetDrawingDeleted") };
        to_js_value(&crate::dto::DrawingResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), drawing })
    }

    /// Re-saves an existing drawing from Paint (vector). Undoable. Copies
    /// already placed in scenes are plain nodes and don't change; placing
    /// it again uses the new artwork.
    pub fn update_drawing(&mut self, drawing: JsValue, svg: String) -> Result<JsValue, JsValue> {
        let drawing_id: an1mbyt3_core::DrawingId = parse_id(drawing)?;
        check_drawing_svg(&svg)?;
        let mut value = self.document.drawing(drawing_id).ok_or_else(|| JsValue::from_str("update_drawing: unknown drawing"))?.clone();
        value.content = an1mbyt3_core::DrawingContent::Vector { svg };
        self.document.apply(Command::SetDrawing { drawing: drawing_id, value: Box::new(value) });
        self.document_summary()
    }

    /// Re-saves an existing drawing from Paint (bitmap): a new image asset
    /// (so undo can restore the previous one). Undoable.
    pub async fn update_bitmap_drawing(&mut self, drawing: JsValue, png: &[u8]) -> Result<JsValue, JsValue> {
        let drawing_id: an1mbyt3_core::DrawingId = parse_id(drawing)?;
        let mut value = self.document.drawing(drawing_id).ok_or_else(|| JsValue::from_str("update_bitmap_drawing: unknown drawing"))?.clone();
        let image = self.add_drawing_image(&value.name, png).await?;
        value.content = an1mbyt3_core::DrawingContent::Bitmap { image };
        self.document.apply(Command::SetDrawing { drawing: drawing_id, value: Box::new(value) });
        self.document_summary()
    }

    /// Scene → Properties → Add: a new property on `artboard` (a unique
    /// name made from `name`) of `kind` (`"Number"`, `"Bool"`, `"Text"`,
    /// `"Color"`), with that kind's zero value. Undoable.
    pub fn add_property(&mut self, artboard: JsValue, name: String, kind: String) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let kind: an1mbyt3_core::PropKind = serde_json::from_value(serde_json::Value::String(kind)).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let taken: Vec<String> = self.document.properties().filter(|(_, p)| p.artboard == artboard_id).map(|(_, p)| p.name.clone()).collect();
        let base = if name.trim().is_empty() { "value".to_string() } else { name.trim().to_string() };
        let mut unique = base.clone();
        let mut n = 2;
        while taken.contains(&unique) {
            unique = format!("{base}{n}");
            n += 1;
        }
        let default = match kind {
            an1mbyt3_core::PropKind::Number => "0",
            an1mbyt3_core::PropKind::Bool => "false",
            an1mbyt3_core::PropKind::Text => "\"\"",
            an1mbyt3_core::PropKind::Color => "[1,1,1,1]",
        }
        .to_string();
        self.document.apply(Command::AddProperty { property: an1mbyt3_core::PropertyDef { artboard: artboard_id, name: unique, kind, default, deleted: false } });
        self.document_summary()
    }

    /// Edits a property (name, kind, default JSON). Undoable. Fails on a
    /// name another property of that scene has.
    pub fn update_property(&mut self, property: JsValue, name: String, kind: String, default: String) -> Result<JsValue, JsValue> {
        let id: an1mbyt3_core::PropertyId = parse_id(property)?;
        let mut value = self.document.property(id).ok_or_else(|| JsValue::from_str("update_property: unknown property"))?.clone();
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(JsValue::from_str("A property needs a name"));
        }
        if self.document.properties().any(|(other, p)| other != id && p.artboard == value.artboard && p.name == name) {
            return Err(JsValue::from_str(&format!("This scene already has a property named {name}")));
        }
        value.name = name;
        value.kind = serde_json::from_value(serde_json::Value::String(kind)).map_err(|e| JsValue::from_str(&e.to_string()))?;
        value.default = default;
        self.document.apply(Command::SetProperty { property: id, value: Box::new(value) });
        self.document_summary()
    }

    pub fn delete_property(&mut self, property: JsValue) -> Result<JsValue, JsValue> {
        let id: an1mbyt3_core::PropertyId = parse_id(property)?;
        self.document.apply(Command::SetPropertyDeleted { property: id, value: true });
        self.document_summary()
    }

    /// Untracked runtime write of one component field (On Click "Set
    /// component field", property bindings): the node's first enabled
    /// `component` (kind name, e.g. `"Light2D"`), its camelCase `field` set
    /// to `value` (JSON). Goes through the same JSON shape the Inspector
    /// uses, so any field of any component works. `false` if the node has
    /// no such component or the value doesn't fit the field.
    pub fn preview_component_field(&mut self, artboard: JsValue, node: JsValue, component: &str, field: &str, value: &str) -> Result<bool, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let parsed: serde_json::Value = serde_json::from_str(value).unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
        let Some(target) = self.document.artboard_mut(artboard_id).and_then(|a| a.node_mut(node_id)) else { return Ok(false) };
        // A shape's stroke colour isn't a component, but Property Bindings
        // (and On Click "Set field") address it as `Shape.strokeColor` so a
        // theme colour property can drive line icons.
        if component == "Shape" && field == "strokeColor" {
            let NodeKind::Shape(shape) = &mut target.kind else { return Ok(false) };
            let Some(stroke) = shape.stroke.as_mut() else { return Ok(false) };
            let Ok(color) = serde_json::from_value::<[f32; 4]>(parsed) else { return Ok(false) };
            stroke.color = color;
            return Ok(true);
        }
        for entry in target.behaviors.iter_mut().filter(|e| e.enabled) {
            let dto = crate::dto::NodeBehaviorDto::from(&entry.behavior);
            let Ok(mut json) = serde_json::to_value(&dto) else { continue };
            if json.get("kind").and_then(|k| k.as_str()) != Some(component) {
                continue;
            }
            let Some(object) = json.as_object_mut() else { return Ok(false) };
            if !object.contains_key(field) {
                return Ok(false);
            }
            object.insert(field.to_string(), parsed);
            let Ok(next) = serde_json::from_value::<crate::dto::NodeBehaviorDto>(json) else { return Ok(false) };
            entry.behavior = next.into();
            return Ok(true);
        }
        Ok(false)
    }

    /// The Code tab's New File: adds a code / markup file at `path` (made
    /// unique with a number if taken) with `text`. Undoable. Returns the
    /// document summary.
    pub fn add_text_file(&mut self, path: String, text: String) -> Result<JsValue, JsValue> {
        let mut path = an1mbyt3_core::text_file::normalize_path(&path);
        if path.is_empty() {
            path = "untitled.txt".into();
        }
        if self.document.text_file_by_path(&path).is_some() {
            let (stem, ext) = path.rsplit_once('.').map_or((path.clone(), String::new()), |(s, e)| (s.to_string(), format!(".{e}")));
            let mut n = 2;
            while self.document.text_file_by_path(&format!("{stem} {n}{ext}")).is_some() {
                n += 1;
            }
            path = format!("{stem} {n}{ext}");
        }
        self.document.apply(Command::AddTextFile { file: an1mbyt3_core::TextFile::new(path, text) });
        self.document_summary()
    }

    /// Typing in the Code tab: replaces a file's text without an undo entry
    /// per keystroke (the code editor has its own undo) — still counts as an
    /// unsaved change. Cheap: no summary is returned.
    pub fn set_text_file_text(&mut self, file: JsValue, text: String) -> Result<bool, JsValue> {
        let id: an1mbyt3_core::TextFileId = parse_id(file)?;
        Ok(self.document.set_text_file_text(id, text))
    }

    /// Renames / moves a file (`path` is its new full path). Undoable.
    /// Fails if another file already has that path.
    pub fn rename_text_file(&mut self, file: JsValue, path: String) -> Result<JsValue, JsValue> {
        let id: an1mbyt3_core::TextFileId = parse_id(file)?;
        let path = an1mbyt3_core::text_file::normalize_path(&path);
        if path.is_empty() {
            return Err(JsValue::from_str("A file needs a name"));
        }
        if self.document.text_file_by_path(&path).is_some_and(|(other, _)| other != id) {
            return Err(JsValue::from_str(&format!("There's already a file at {path}")));
        }
        let mut value = self.document.text_file(id).ok_or_else(|| JsValue::from_str("rename_text_file: unknown file"))?.clone();
        value.path = path;
        self.document.apply(Command::SetTextFile { file: id, value: Box::new(value) });
        self.document_summary()
    }

    /// Deletes a file. Undoable.
    pub fn delete_text_file(&mut self, file: JsValue) -> Result<JsValue, JsValue> {
        let id: an1mbyt3_core::TextFileId = parse_id(file)?;
        self.document.apply(Command::SetTextFileDeleted { file: id, value: true });
        self.document_summary()
    }

    pub fn rename_drawing(&mut self, drawing: JsValue, name: String) -> Result<JsValue, JsValue> {
        let drawing_id: an1mbyt3_core::DrawingId = parse_id(drawing)?;
        let mut value = self.document.drawing(drawing_id).ok_or_else(|| JsValue::from_str("rename_drawing: unknown drawing"))?.clone();
        value.name = name;
        self.document.apply(Command::SetDrawing { drawing: drawing_id, value: Box::new(value) });
        self.document_summary()
    }

    pub fn delete_drawing(&mut self, drawing: JsValue) -> Result<JsValue, JsValue> {
        let drawing_id: an1mbyt3_core::DrawingId = parse_id(drawing)?;
        self.document.apply(Command::SetDrawingDeleted { drawing: drawing_id, value: true });
        self.document_summary()
    }

    /// A vector drawing's SVG (to reopen it in Paint); `null` for bitmap
    /// drawings (their PNG is the image asset `DrawingSummary::image`).
    pub fn drawing_svg(&self, drawing: JsValue) -> Result<Option<String>, JsValue> {
        let drawing_id: an1mbyt3_core::DrawingId = parse_id(drawing)?;
        let value = self.document.drawing(drawing_id).ok_or_else(|| JsValue::from_str("drawing_svg: unknown drawing"))?;
        Ok(match &value.content {
            an1mbyt3_core::DrawingContent::Vector { svg } => Some(svg.clone()),
            an1mbyt3_core::DrawingContent::Bitmap { .. } | an1mbyt3_core::DrawingContent::Animation(_) => None,
        })
    }

    /// Places a drawing in a scene, centred at `(x, y)` in `parent`'s space
    /// (`null` = scene root) — dragging it from Resources, or its "Place"
    /// action. A vector drawing becomes one Group named after it, holding
    /// the imported shapes (plain nodes: edit freely); a bitmap one becomes
    /// an Image node. One undo step. Returns `{ document, newNode }`.
    pub fn place_drawing(&mut self, artboard: JsValue, drawing: JsValue, parent: JsValue, x: f32, y: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let drawing_id: an1mbyt3_core::DrawingId = parse_id(drawing)?;
        let parent_id: Option<NodeId> = parse_id(parent)?;
        let value = self.document.drawing(drawing_id).filter(|d| !d.deleted).ok_or_else(|| JsValue::from_str("place_drawing: unknown drawing"))?.clone();
        let svg = match value.content {
            an1mbyt3_core::DrawingContent::Bitmap { image } => {
                // scratch-paint's bitmap mode paints at 2x: half the pixel
                // size is the drawing's real size.
                let (w, h) = self.document.image(image).and_then(|a| an1mbyt3_image::png_dimensions(&a.source).ok()).unwrap_or((32, 32));
                let mut node = Node::new(value.name.clone(), parent_id, NodeKind::Image(an1mbyt3_core::ImageNode { image, width: w as f32 / 2.0, height: h as f32 / 2.0, fill_color: [1.0; 4], blend_mode: Default::default() }));
                node.transform.translation = Vec2::new(x, y);
                let inverse = self.document.apply(Command::AddNode { artboard: artboard_id, node });
                let Command::SetDeleted { node: new_node, .. } = inverse else { unreachable!("Command::AddNode's inverse is always a SetDeleted") };
                return to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node });
            }
            an1mbyt3_core::DrawingContent::Animation(animation) => return self.place_animation(artboard_id, parent_id, &value.name, &animation, x, y),
            an1mbyt3_core::DrawingContent::Vector { svg } => svg,
        };
        let nodes = an1mbyt3_svg::import_svg(&svg).map_err(|err| JsValue::from_str(&format!("place_drawing: {err}")))?;
        // The imported paths sit wherever scratch-paint's canvas put them;
        // a root Group positioned so the drawing's centre lands on (x, y),
        // with its pivot moved to that centre (rotating/scaling the placed
        // drawing then turns it in place).
        let (min, max) = imported_bounds(&nodes).unwrap_or((Vec2::ZERO, Vec2::ZERO));
        let centre = (min + max) / 2.0;
        let mut root = Node::new(value.name.clone(), parent_id, NodeKind::Group(Default::default()));
        root.transform.translation = Vec2::new(x, y) - centre;
        root.transform = root.transform.with_anchor_preserving_world(centre);
        let mut inverses = Vec::new();
        let inverse = Command::AddNode { artboard: artboard_id, node: root }.apply(&mut self.document);
        let Command::SetDeleted { node: root_id, .. } = inverse else { unreachable!("Command::AddNode's inverse is always a SetDeleted") };
        inverses.push(inverse);
        let mut new_nodes = vec![root_id];
        for node in nodes {
            insert_imported_node(&mut self.document, artboard_id, node, Some(root_id), &mut inverses, &mut new_nodes);
        }
        inverses.reverse();
        self.document.commit_batch(inverses);
        to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node: root_id })
    }

    /// `place_drawing` for a frames × layers drawing: one root Group named
    /// after it at `(x, y)` (with a Flipbook when there's more than one
    /// frame), a Group per frame ("Frame 1"…, in order), and inside each a
    /// node per visible layer's cel — imported shapes for vector cels, an
    /// Image for bitmap cels. Each cel is offset so its registration point
    /// (`DrawingCel::center`) sits on the root's origin, which is what keeps
    /// the frames lined up exactly as they were in Paint. One undo step.
    fn place_animation(&mut self, artboard_id: ArtboardId, parent: Option<NodeId>, name: &str, animation: &an1mbyt3_core::DrawingAnimation, x: f32, y: f32) -> Result<JsValue, JsValue> {
        let mut inverses = Vec::new();
        let mut new_nodes = Vec::new();
        let add = |document: &mut Document, node: Node, inverses: &mut Vec<Command>| -> NodeId {
            let inverse = Command::AddNode { artboard: artboard_id, node }.apply(document);
            let Command::SetDeleted { node: id, .. } = inverse else { unreachable!("Command::AddNode's inverse is always a SetDeleted") };
            inverses.push(inverse);
            id
        };
        let frames = animation.frame_count.max(1) as usize;
        let mut root = Node::new(name.to_string(), parent, NodeKind::Group(Default::default()));
        root.transform.translation = Vec2::new(x, y);
        if frames > 1 {
            root.behaviors.push(an1mbyt3_core::NodeBehaviorEntry {
                behavior: an1mbyt3_core::NodeBehavior::Flipbook(an1mbyt3_core::Flipbook { fps: animation.fps, ..Default::default() }),
                enabled: true,
            });
        }
        let root_id = add(&mut self.document, root, &mut inverses);
        new_nodes.push(root_id);
        let visible_layers: Vec<&an1mbyt3_core::DrawingLayer> = animation.layers.iter().filter(|l| l.visible).collect();
        for frame in 0..frames {
            let frame_parent = if frames > 1 {
                let mut node = Node::new(format!("Frame {}", frame + 1), Some(root_id), NodeKind::Group(Default::default()));
                node.visible = frame == 0;
                add(&mut self.document, node, &mut inverses)
            } else {
                root_id
            };
            for layer in &visible_layers {
                let Some(Some(cel)) = layer.cels.get(frame) else { continue };
                let [cx, cy] = cel.center;
                let layer_name = if visible_layers.len() > 1 { layer.name.clone() } else { format!("{name} art") };
                match &cel.content {
                    an1mbyt3_core::DrawingCelContent::Vector { svg } => {
                        let Ok(mut nodes) = an1mbyt3_svg::import_svg(svg) else { continue };
                        fade_imported(&mut nodes, layer.opacity.clamp(0.0, 1.0));
                        // The importer centres the SVG's viewBox on (0, 0)
                        // and flips y: SVG point (px, py) lands at
                        // (px - w/2, h/2 - py). Shift so `center` hits 0.
                        let (_, _, w, h) = svg_view_box(svg).unwrap_or((0.0, 0.0, 0.0, 0.0));
                        let mut holder = Node::new(layer_name, Some(frame_parent), NodeKind::Group(Default::default()));
                        holder.transform.translation = Vec2::new(w / 2.0 - cx, cy - h / 2.0);
                        let holder_id = add(&mut self.document, holder, &mut inverses);
                        for node in nodes {
                            insert_imported_node(&mut self.document, artboard_id, node, Some(holder_id), &mut inverses, &mut new_nodes);
                        }
                    }
                    an1mbyt3_core::DrawingCelContent::Bitmap { image } => {
                        let Some((w, h)) = self.document.image(*image).and_then(|a| an1mbyt3_image::png_dimensions(&a.source).ok()) else { continue };
                        let (w, h) = (w as f32, h as f32);
                        // 2 bitmap px per unit; the Image is centred on its node.
                        let mut node = Node::new(layer_name, Some(frame_parent), NodeKind::Image(an1mbyt3_core::ImageNode { image: *image, width: w / 2.0, height: h / 2.0, fill_color: [1.0, 1.0, 1.0, layer.opacity.clamp(0.0, 1.0)], blend_mode: Default::default() }));
                        node.transform.translation = Vec2::new((w / 2.0 - cx) / 2.0, (cy - h / 2.0) / 2.0);
                        add(&mut self.document, node, &mut inverses);
                    }
                }
            }
        }
        inverses.reverse();
        self.document.commit_batch(inverses);
        to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node: root_id })
    }

    /// Saves a frames × layers drawing (Paint's frame strip) as a new
    /// Drawing resource. `value` is `DrawingAnimation`-shaped JSON; bitmap
    /// cels reference image assets made first with `add_drawing_cel_image`.
    /// Undoable. Returns `{ document, drawing }`.
    pub fn add_animated_drawing(&mut self, name: String, value: JsValue) -> Result<JsValue, JsValue> {
        let animation = self.parse_drawing_animation(value)?;
        let inverse = self.document.apply(Command::AddDrawing {
            drawing: an1mbyt3_core::DrawingAsset { name, content: an1mbyt3_core::DrawingContent::Animation(animation), deleted: false },
        });
        let Command::SetDrawingDeleted { drawing, .. } = inverse else { unreachable!("AddDrawing's inverse is SetDrawingDeleted") };
        to_js_value(&crate::dto::DrawingResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), drawing })
    }

    /// Re-saves a drawing as frames × layers (see `add_animated_drawing`). Undoable.
    pub fn update_animated_drawing(&mut self, drawing: JsValue, value: JsValue) -> Result<JsValue, JsValue> {
        let drawing_id: an1mbyt3_core::DrawingId = parse_id(drawing)?;
        let animation = self.parse_drawing_animation(value)?;
        let mut asset = self.document.drawing(drawing_id).ok_or_else(|| JsValue::from_str("update_animated_drawing: unknown drawing"))?.clone();
        asset.content = an1mbyt3_core::DrawingContent::Animation(animation);
        self.document.apply(Command::SetDrawing { drawing: drawing_id, value: Box::new(asset) });
        self.document_summary()
    }

    /// A bitmap cel's PNG as a new image asset (2x scale, see
    /// `DrawingCel::center`); returns its id for `add_animated_drawing`.
    pub async fn add_drawing_cel_image(&mut self, name: String, png: &[u8]) -> Result<JsValue, JsValue> {
        let image = self.add_drawing_image(&name, png).await?;
        to_js_value(&image)
    }

    /// The drawing's full content as JSON (`DrawingContent`), for reopening
    /// it in Paint and drawing its thumbnail.
    pub fn drawing_content(&self, drawing: JsValue) -> Result<JsValue, JsValue> {
        let drawing_id: an1mbyt3_core::DrawingId = parse_id(drawing)?;
        let value = self.document.drawing(drawing_id).ok_or_else(|| JsValue::from_str("drawing_content: unknown drawing"))?;
        to_js_value(&value.content)
    }

    fn parse_drawing_animation(&self, value: JsValue) -> Result<an1mbyt3_core::DrawingAnimation, JsValue> {
        let mut animation: an1mbyt3_core::DrawingAnimation = serde_wasm_bindgen::from_value(value).map_err(|err| JsValue::from_str(&format!("invalid drawing: {err}")))?;
        if animation.layers.is_empty() {
            return Err(JsValue::from_str("the drawing has no layers"));
        }
        let frames = animation.frame_count.max(1);
        animation.frame_count = frames;
        let mut any = false;
        for layer in &mut animation.layers {
            layer.cels.resize(frames as usize, None);
            for cel in layer.cels.iter_mut() {
                let Some(c) = cel else { continue };
                let ok = match &c.content {
                    an1mbyt3_core::DrawingCelContent::Vector { svg } => an1mbyt3_svg::import_svg(svg).ok().and_then(|n| imported_bounds(&n)).is_some(),
                    an1mbyt3_core::DrawingCelContent::Bitmap { image } => self.document.image(*image).is_some(),
                };
                // An empty or unreadable cel is just an empty cel.
                if ok { any = true } else { *cel = None }
            }
        }
        if !any {
            return Err(JsValue::from_str("the drawing is empty — draw something first"));
        }
        Ok(animation)
    }

    /// A drawing's PNG as a new (baked) image asset named after it.
    async fn add_drawing_image(&mut self, name: &str, png: &[u8]) -> Result<an1mbyt3_core::ImageId, JsValue> {
        if an1mbyt3_image::png_dimensions(png).is_err() {
            return Err(JsValue::from_str("bitmap drawing: not a PNG"));
        }
        self.document.touch();
        let image = self.document.add_image(an1mbyt3_core::ImageAsset { name: format!("{name}.png"), source: png.to_vec(), processing: Default::default(), file: None });
        self.bake_and_cache_image(image).await;
        Ok(image)
    }

    pub fn rename_prefab(&mut self, prefab: JsValue, name: String) -> Result<JsValue, JsValue> {
        let prefab_id: an1mbyt3_core::PrefabId = parse_id(prefab)?;
        let mut definition = self.document.prefab(prefab_id).ok_or_else(|| JsValue::from_str("rename_prefab: unknown prefab"))?.clone();
        definition.name = name;
        self.document.apply(Command::SetPrefab { prefab: prefab_id, value: Box::new(definition) });
        self.document_summary()
    }

    /// Scripts' `ctx.spawn(prefab, x, y)` — Playground only. Instantiates
    /// straight into the scene (no undo entry) and remembers it, so leaving
    /// Playground removes everything spawned. `prefab_name` matches a
    /// prefab by name (what scripts know). `parent`: `null` = scene root;
    /// `(x, y)` are in the parent's space. Returns `[{ id, name }]` for every
    /// spawned node (root first), or `null` if no prefab has that name.
    pub fn spawn_prefab(&mut self, artboard: JsValue, prefab_name: &str, parent: JsValue, x: f32, y: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let parent_id: Option<NodeId> = parse_id(parent)?;
        let Some((prefab_id, definition)) = self.document.prefabs().find(|(_, d)| d.name == prefab_name).map(|(id, d)| (id, d.clone())) else {
            return to_js_value(&Option::<NodeId>::None);
        };
        let Some(artboard_mut) = self.document.artboard_mut(artboard_id) else {
            return Err(JsValue::from_str("spawn_prefab: unknown scene"));
        };
        let mut transform = definition.nodes.first().map(|n| n.node.transform).unwrap_or_default();
        transform.translation = glam::Vec2::new(x, y);
        let ids = crate::prefab_ops::spawn_runtime(artboard_mut, &definition, prefab_id, parent_id, transform);
        let Some(&root) = ids.first() else { return to_js_value(&Option::<NodeId>::None) };
        self.runtime_spawned.push((artboard_id, root));
        // Every part, root first, by its prefab name — the new nodes aren't
        // in the script's scene snapshot until next frame, so this is how a
        // script reaches a spawned object's children right away
        // (`ref.part('HP')`).
        let artboard_ref = self.document.artboard(artboard_id).expect("spawned into it above");
        let parts: Vec<SpawnedPart> = ids
            .iter()
            .zip(&definition.nodes)
            .filter_map(|(&id, n)| {
                let node = artboard_ref.node(id)?;
                Some(SpawnedPart { id, name: n.node.name.clone(), transform: node.transform.into(), fill_color: node_fill_color(node) })
            })
            .collect();
        to_js_value(&Some(parts))
    }

    /// Scripts' `ref.destroy()`. A runtime-spawned object is removed for
    /// real (subtree included); a node that's part of the authored scene is
    /// only hidden — it must come back when play ends, and Playground's
    /// pose restore brings `visible` back.
    pub fn destroy_node(&mut self, artboard: JsValue, node: JsValue) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let spawned = self.runtime_spawned.iter().position(|&(a, n)| a == artboard_id && n == node_id);
        let Some(artboard_mut) = self.document.artboard_mut(artboard_id) else { return Ok(()) };
        match spawned {
            Some(index) => {
                self.runtime_spawned.swap_remove(index);
                crate::prefab_ops::despawn_subtree(artboard_mut, node_id);
            }
            None => {
                if let Some(n) = artboard_mut.node_mut(node_id) {
                    n.visible = false;
                }
            }
        }
        Ok(())
    }

    /// Removes what scripts spawned in one scene (leaving it mid-play).
    fn despawn_runtime_spawned_in(&mut self, artboard_id: ArtboardId) {
        let (here, elsewhere): (Vec<_>, Vec<_>) = std::mem::take(&mut self.runtime_spawned).into_iter().partition(|(a, _)| *a == artboard_id);
        self.runtime_spawned = elsewhere;
        if let Some(artboard_mut) = self.document.artboard_mut(artboard_id) {
            for (_, root) in here {
                crate::prefab_ops::despawn_subtree(artboard_mut, root);
            }
        }
    }

    /// Removes every object scripts spawned this play session.
    fn despawn_runtime_spawned(&mut self) {
        for (artboard_id, root) in std::mem::take(&mut self.runtime_spawned) {
            if let Some(artboard_mut) = self.document.artboard_mut(artboard_id) {
                crate::prefab_ops::despawn_subtree(artboard_mut, root);
            }
        }
    }

    /// Soft-deletes a component definition (`Command::SetComponentDeleted`)
    /// — the Components panel's "Delete" action. Undoable. Existing
    /// instances aren't touched or cleaned up — same "dangling reference is
    /// a harmless no-op, not this command's job to cascade" reasoning
    /// `SetStateMachineStates`'s doc comment already establishes for a
    /// deleted state; an instance whose `component` no longer resolves just
    /// won't have anything to render until either the component is restored
    /// (undo) or the instance itself is deleted.
    pub fn delete_component(&mut self, component: JsValue) -> Result<JsValue, JsValue> {
        let component_id: ComponentId = parse_id(component)?;
        self.document.apply(Command::SetComponentDeleted { component: component_id, value: true });
        self.document_summary()
    }

    /// Changes a component definition's default fill color — the Components
    /// panel's default-color field. Undoable (`Command::SetComponentDefaultFillColor`).
    pub fn set_component_default_fill_color(&mut self, component: JsValue, value: JsValue) -> Result<JsValue, JsValue> {
        let component_id: ComponentId = parse_id(component)?;
        let value: [f32; 4] = parse_id(value)?;
        self.document.apply(Command::SetComponentDefaultFillColor { component: component_id, value });
        self.document_summary()
    }

    /// Declares a new named variant on a component definition — the
    /// Components panel's "Add Variant" action. Undoable
    /// (`Command::SetComponentVariants`, whole-list swap — see that Rust
    /// variant's doc comment).
    pub fn add_component_variant(&mut self, component: JsValue, name: String, fill_color: JsValue) -> Result<JsValue, JsValue> {
        let component_id: ComponentId = parse_id(component)?;
        let fill_color: [f32; 4] = parse_id(fill_color)?;

        let mut variants = self
            .document
            .component(component_id)
            .ok_or_else(|| JsValue::from_str("add_component_variant: unknown component"))?
            .variants
            .clone();
        variants.push(ComponentVariant { name, fill_color });
        self.document.apply(Command::SetComponentVariants { component: component_id, value: variants });
        self.document_summary()
    }

    /// Creates a new, empty (no fields yet) `ScriptDefinition` — the
    /// Inspector's "+ New Script..." action. Undoable (`Command::AddScript`).
    /// Fields are added afterward via `set_script_fields`, same two-step
    /// shape `add_component`/`add_component_variant` already split create-vs-
    /// configure into.
    pub fn add_script(&mut self, name: String) -> Result<JsValue, JsValue> {
        self.document.apply(Command::AddScript { definition: ScriptDefinition { name, fields: Vec::new(), deleted: false } });
        self.document_summary()
    }

    /// Soft-deletes a script definition (`Command::SetScriptDeleted`) — same
    /// "dangling reference is a harmless no-op" convention `delete_component`
    /// already establishes for an instance whose definition no longer
    /// resolves.
    pub fn delete_script(&mut self, script: JsValue) -> Result<JsValue, JsValue> {
        let script_id: ScriptId = parse_id(script)?;
        self.document.apply(Command::SetScriptDeleted { script: script_id, value: true });
        self.document_summary()
    }

    /// Replaces a script definition's whole field list — the script
    /// authoring UI's add/rename/remove/retype-a-field actions, all funneling
    /// through one call (`Command::SetScriptFields`, whole-list swap — see
    /// that Rust variant's own doc comment).
    pub fn set_script_fields(&mut self, script: JsValue, fields: JsValue) -> Result<JsValue, JsValue> {
        let script_id: ScriptId = parse_id(script)?;
        let fields: Vec<ScriptFieldDefDto> = parse_id(fields)?;
        self.document.apply(Command::SetScriptFields { script: script_id, value: fields.into_iter().map(Into::into).collect() });
        self.document_summary()
    }

    /// Replaces a node's whole attached-behaviors list — the Inspector's
    /// "Behaviors" section: add/remove/toggle a behavior, or edit one
    /// `Script` instance's field values, all funnel through this one call
    /// (`Command::SetNodeBehaviors`, whole-list swap — see that Rust
    /// variant's own doc comment for why editing a script instance's fields
    /// goes through here too, not a narrower per-field command).
    pub fn set_node_behaviors(&mut self, artboard: JsValue, node: JsValue, behaviors: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let before = self.record_before(artboard_id, node_id);
        let behaviors: Vec<NodeBehaviorEntryDto> = parse_id(behaviors)?;
        self.document.apply(Command::SetNodeBehaviors { artboard: artboard_id, node: node_id, value: behaviors.into_iter().map(Into::into).collect() });
        self.record_field_edits(artboard_id, node_id, before);
        self.document_summary()
    }

    /// Adds a new instance node of `component` to `artboard`, optionally
    /// parented (`parent: null` for a root instance) — the Components
    /// panel's "drag component into artboard" action. Undoable
    /// (`Command::AddNode` — `NodeKind::Instance` is already a valid
    /// `AddNode` payload, same reuse `add_bone_node` established for `Bone`).
    pub fn add_instance_node(&mut self, artboard: JsValue, parent: JsValue, component: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let parent_id: Option<NodeId> = parse_id(parent)?;
        let component_id: ComponentId = parse_id(component)?;

        let node = Node::new("Instance", parent_id, NodeKind::Instance(ComponentInstance::new(component_id)));
        let inverse = self.document.apply(Command::AddNode { artboard: artboard_id, node });
        let Command::SetDeleted { node: new_node, .. } = inverse else {
            unreachable!("Command::AddNode's inverse is always a SetDeleted")
        };

        to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node })
    }

    /// Changes an instance's direct fill-color override — the Inspector's
    /// per-instance color field (`Instance` nodes only). `value: null` clears
    /// the override. Undoable (`Command::SetInstanceFillColorOverride`).
    pub fn set_instance_fill_color_override(&mut self, artboard: JsValue, node: JsValue, value: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let value: Option<[f32; 4]> = parse_id(value)?;
        self.document.apply(Command::SetInstanceFillColorOverride { artboard: artboard_id, node: node_id, value });
        self.document_summary()
    }

    /// Changes which named variant an instance references — the Inspector's
    /// per-instance variant dropdown (`Instance` nodes only). `value: null`
    /// clears it (falls back to the definition's default color). Undoable
    /// (`Command::SetInstanceVariant`).
    pub fn set_instance_variant(&mut self, artboard: JsValue, node: JsValue, value: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let value: Option<String> = parse_id(value)?;
        self.document.apply(Command::SetInstanceVariant { artboard: artboard_id, node: node_id, value });
        self.document_summary()
    }

    /// Resizes a bone's reach — the Bones panel's length field. Undoable
    /// (`Command::SetBoneLength`).
    pub fn set_bone_length(&mut self, artboard: JsValue, node: JsValue, value: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let before = self.record_before(artboard_id, node_id);
        self.document.apply(Command::SetBoneLength { artboard: artboard_id, node: node_id, value });
        self.record_field_edits(artboard_id, node_id, before);
        self.document_summary()
    }

    /// Changes a shape's blend mode (`"Normal"`/`"Multiply"`/`"Screen"`/
    /// `"Add"`) — the Inspector's blend-mode dropdown. Undoable
    /// (`Command::SetShapeBlendMode`).
    pub fn set_shape_blend_mode(&mut self, artboard: JsValue, node: JsValue, value: &str) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let value = parse_blend_mode(value)?;
        self.document.apply(Command::SetShapeBlendMode { artboard: artboard_id, node: node_id, value });
        self.document_summary()
    }

    /// Changes a `Shape` or `Text` node's own `fill_color` — the
    /// Inspector's fill-color swatch. Undoable (`Command::SetFillColor`).
    pub fn set_fill_color(&mut self, artboard: JsValue, node: JsValue, value: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let value: [f32; 4] = parse_id(value)?;
        if self.is_group(artboard_id, node_id) {
            self.apply_fill(artboard_id, &[node_id], Some(value), None);
            return self.document_summary();
        }
        self.document.apply(Command::SetFillColor { artboard: artboard_id, node: node_id, value });
        self.document_summary()
    }

    /// Sets a node's fill color directly, bypassing undo — the continuous-
    /// per-frame counterpart to `set_fill_color`, same relationship
    /// `preview_node_translation` has to `set_translation`/`commit_*`: a
    /// script (Code-mode `runScript`) driving a color every single frame
    /// (a flash fading out, a background pulse) through the *undoable*
    /// command would push one undo entry per frame — this mutates the node
    /// directly instead, same "no document summary needed, the frontend's
    /// own periodic `getDocumentSummary` picks it up" convention every
    /// other `preview_*` method already follows. A no-op (not an error) for
    /// a node kind with no fill color at all — see `set_node_fill_color`'s
    /// own doc comment.
    pub fn preview_fill_color(&mut self, artboard: JsValue, node: JsValue, value: JsValue) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let value: [f32; 4] = parse_id(value)?;
        let artboard_ref = self.document.artboard_mut(artboard_id).ok_or_else(|| JsValue::from_str("preview_fill_color: unknown artboard"))?;
        let node_ref = artboard_ref.node_mut(node_id).ok_or_else(|| JsValue::from_str("preview_fill_color: unknown node"))?;
        set_node_fill_color(node_ref, value);
        Ok(())
    }

    /// Overwrites a `Shape` node's whole path geometry directly (`value`: a
    /// `dto::PathDto`-shaped object) — untracked, same reasoning as
    /// `preview_fill_color`: a script driving a per-frame organic "breathing"
    /// shape (interpolating its own path commands in JS and writing the
    /// result back every tick) would flood the undo stack pointlessly if
    /// this went through a `Command`. A no-op if `node` isn't a `Shape` node.
    pub fn preview_node_path(&mut self, artboard: JsValue, node: JsValue, value: JsValue) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let dto: crate::dto::PathDto = serde_wasm_bindgen::from_value(value).map_err(|err| JsValue::from_str(&format!("invalid path: {err}")))?;
        let artboard_ref = self.document.artboard_mut(artboard_id).ok_or_else(|| JsValue::from_str("preview_node_path: unknown artboard"))?;
        let node_ref = artboard_ref.node_mut(node_id).ok_or_else(|| JsValue::from_str("preview_node_path: unknown node"))?;
        if let NodeKind::Shape(shape) = &mut node_ref.kind {
            shape.path = dto.into();
        }
        Ok(())
    }

    /// Sets `Node::visible` directly — the Code-mode/script equivalent of a
    /// Blocks `GraphProperty::Visible` write, undoable
    /// (`Command::SetNodeVisible`) since this is meant for occasional scene-
    /// transition toggles (title/HUD/win-screen groups), not a per-frame
    /// continuous change — see that command's own doc comment.
    pub fn set_node_visible(&mut self, artboard: JsValue, node: JsValue, value: bool) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let before = self.record_before(artboard_id, node_id);
        self.document.apply(Command::SetNodeVisible { artboard: artboard_id, node: node_id, value });
        self.record_field_edits(artboard_id, node_id, before);
        Ok(())
    }

    /// Sets (`value`: a `dto::GradientDefDto`-shaped object) or clears
    /// (`value: null`/`undefined`) a `Shape`/`Text` node's gradient — the
    /// Fill tool's solid/gradient toggle and its on-canvas handle both go
    /// through this. Undoable (`Command::SetGradient`).
    pub fn set_gradient(&mut self, artboard: JsValue, node: JsValue, value: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let before = self.record_before(artboard_id, node_id);
        let value: Option<an1mbyt3_core::GradientDef> = if value.is_null() || value.is_undefined() {
            None
        } else {
            let dto: crate::dto::GradientDefDto = serde_wasm_bindgen::from_value(value).map_err(|err| JsValue::from_str(&format!("invalid gradient: {err}")))?;
            Some(dto.into())
        };
        if self.is_group(artboard_id, node_id) {
            // A group's gradient is in its own space: to world for the bake.
            let world = self.document.artboard(artboard_id).and_then(|a| a.try_world_transform(node_id)).map_or(glam::Affine2::IDENTITY, |t| t.to_affine2());
            let color = self.document.artboard(artboard_id).and_then(|a| a.node(node_id)).and_then(|n| n.group_fill()).map_or([1.0; 4], |f| f.color);
            match value {
                Some(g) => self.apply_fill(artboard_id, &[node_id], None, Some(map_gradient(&g, world))),
                None => self.apply_fill(artboard_id, &[node_id], Some(color), None),
            }
            return self.document_summary();
        }
        self.document.apply(Command::SetGradient { artboard: artboard_id, node: node_id, value });
        self.record_field_edits(artboard_id, node_id, before);
        self.document_summary()
    }

    /// **Fill several things at once** — a multi-selection's Fill field:
    /// every shape and text in `nodes` (groups: everything inside them) gets
    /// `color`, or the gradient `gradient` given in *world* space (one
    /// gradient across the whole selection, each node getting it in its own
    /// space). Groups among `nodes` also remember it as their Group Fill.
    /// One undoable edit. Returns the document summary.
    pub fn fill_nodes(&mut self, artboard: JsValue, nodes: JsValue, color: JsValue, gradient: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let nodes: Vec<NodeId> = parse_id(nodes)?;
        let color: Option<[f32; 4]> = if color.is_null() || color.is_undefined() { None } else { Some(parse_id(color)?) };
        let gradient: Option<an1mbyt3_core::GradientDef> = if gradient.is_null() || gradient.is_undefined() {
            None
        } else {
            let dto: crate::dto::GradientDefDto = serde_wasm_bindgen::from_value(gradient).map_err(|err| JsValue::from_str(&format!("invalid gradient: {err}")))?;
            Some(dto.into())
        };
        self.apply_fill(artboard_id, &nodes, color, gradient);
        self.document_summary()
    }

    fn is_group(&self, artboard: ArtboardId, node: NodeId) -> bool {
        self.document.artboard(artboard).and_then(|a| a.node(node)).is_some_and(|n| matches!(n.kind, NodeKind::Group(_)))
    }

    /// Paints `targets` (see `fill_nodes`): `color` (and no gradient), or
    /// `gradient_world` mapped into each painted node's own space. Groups
    /// among the targets store it as their Group Fill (in their space). All
    /// one undo step.
    fn apply_fill(&mut self, artboard_id: ArtboardId, targets: &[NodeId], color: Option<[f32; 4]>, gradient_world: Option<an1mbyt3_core::GradientDef>) {
        let Some(art) = self.document.artboard(artboard_id) else { return };
        let world = |id: NodeId| art.try_world_transform(id).map_or(glam::Affine2::IDENTITY, |t| t.to_affine2());
        let mut painted: Vec<NodeId> = Vec::new();
        let mut groups: Vec<(NodeId, an1mbyt3_core::GroupFill, Vec<an1mbyt3_core::NodeBehaviorEntry>)> = Vec::new();
        for &t in targets {
            for id in crate::dto::paintable_descendants(art, t) {
                if !painted.contains(&id) {
                    painted.push(id);
                }
            }
            if let Some(n) = art.node(t).filter(|n| matches!(n.kind, NodeKind::Group(_))) {
                let previous = n.group_fill().cloned().unwrap_or_default();
                let fill = an1mbyt3_core::GroupFill {
                    color: color.unwrap_or(previous.color),
                    gradient: gradient_world.as_ref().map(|g| map_gradient(g, world(t).inverse())),
                };
                groups.push((t, fill, n.behaviors.clone()));
            }
        }
        let locals: Vec<(NodeId, Option<an1mbyt3_core::GradientDef>)> = painted.iter().map(|&id| (id, gradient_world.as_ref().map(|g| map_gradient(g, world(id).inverse())))).collect();
        let mut count = 0;
        for (id, local) in locals {
            if let Some(c) = color {
                self.document.apply(Command::SetFillColor { artboard: artboard_id, node: id, value: c });
                count += 1;
            }
            self.document.apply(Command::SetGradient { artboard: artboard_id, node: id, value: local });
            count += 1;
        }
        for (id, fill, mut behaviors) in groups {
            match behaviors.iter_mut().find(|e| matches!(e.behavior, an1mbyt3_core::NodeBehavior::GroupFill(_))) {
                Some(entry) => {
                    entry.behavior = an1mbyt3_core::NodeBehavior::GroupFill(fill);
                    entry.enabled = true;
                }
                None => behaviors.push(an1mbyt3_core::NodeBehaviorEntry { behavior: an1mbyt3_core::NodeBehavior::GroupFill(fill), enabled: true }),
            }
            self.document.apply(Command::SetNodeBehaviors { artboard: artboard_id, node: id, value: behaviors });
            count += 1;
        }
        if count > 1 {
            self.document.merge_last(count);
        }
    }

    /// Sets (`Some`) or clears (`None`/`null`/`undefined`) a `Shape` node's
    /// `image` — the Inspector's asset-reference swatch (see
    /// `ShapeNode::image`'s own doc comment). Undoable (`Command::SetShapeImage`).
    pub fn set_shape_image(&mut self, artboard: JsValue, node: JsValue, image: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let image_id: Option<an1mbyt3_core::ImageId> = parse_id(image)?;
        self.document.apply(Command::SetShapeImage { artboard: artboard_id, node: node_id, value: image_id });
        self.document_summary()
    }

    /// Replaces a `Shape` node's whole path — the path-editing overlay's
    /// commit action (drag a point/handle, insert a point, delete a point).
    /// `value` is a JS object shaped like `dto::PathDto`. Undoable
    /// (`Command::SetShapePath`).
    pub fn set_shape_path(&mut self, artboard: JsValue, node: JsValue, value: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let dto: crate::dto::PathDto = serde_wasm_bindgen::from_value(value).map_err(|err| JsValue::from_str(&format!("invalid path: {err}")))?;
        self.document.apply(Command::SetShapePath { artboard: artboard_id, node: node_id, value: dto.into() });
        self.document_summary()
    }

    /// Live path-edit preview: directly mutates the shape's path, bypassing
    /// `Document::apply`/`Command` entirely — same untracked-preview
    /// reasoning as `preview_node_translation` (dragging a point across many
    /// `pointermove` events shouldn't push a separate undo entry per event,
    /// and the mesh cache's own per-frame `shape_mesh_source` staleness
    /// check — see `editor.rs`'s `render` — already re-tessellates whenever
    /// `shape.path` differs from what it last cached, so this alone is
    /// enough to make the dragged edit visible on the very next frame with
    /// no extra plumbing). `set_shape_path` is still what actually commits
    /// one undoable edit when the drag ends.
    pub fn preview_shape_path(&mut self, artboard: JsValue, node: JsValue, value: JsValue) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let dto: crate::dto::PathDto = serde_wasm_bindgen::from_value(value).map_err(|err| JsValue::from_str(&format!("invalid path: {err}")))?;
        let artboard_ref = self
            .document
            .artboard_mut(artboard_id)
            .ok_or_else(|| JsValue::from_str("preview_shape_path: unknown artboard"))?;
        let node_ref = artboard_ref
            .node_mut(node_id)
            .ok_or_else(|| JsValue::from_str("preview_shape_path: unknown node"))?;
        let NodeKind::Shape(shape) = &mut node_ref.kind else {
            return Err(JsValue::from_str("preview_shape_path: node is not a Shape"));
        };
        shape.path = dto.into();
        Ok(())
    }

    /// Sets (`Some`) or clears (`None`/`null`/`undefined`) a `Shape` node's
    /// `stroke` — the Inspector's Stroke section. `value` (when not
    /// null/undefined) is a JS object shaped like `dto::StrokeDto`.
    /// Undoable (`Command::SetShapeStroke`).
    pub fn set_shape_stroke(&mut self, artboard: JsValue, node: JsValue, value: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let before = self.record_before(artboard_id, node_id);
        let value: Option<an1mbyt3_core::Stroke> = if value.is_null() || value.is_undefined() {
            None
        } else {
            let dto: crate::dto::StrokeDto = serde_wasm_bindgen::from_value(value).map_err(|err| JsValue::from_str(&format!("invalid stroke: {err}")))?;
            Some(dto.into())
        };
        self.document.apply(Command::SetShapeStroke { artboard: artboard_id, node: node_id, value });
        self.record_field_edits(artboard_id, node_id, before);
        self.document_summary()
    }

    /// Replaces a `ParticleSystem` node's whole authored config — the
    /// Inspector's Particle System section, written all at once. Undoable
    /// (`Command::SetParticleSystem`).
    pub fn set_particle_system(&mut self, artboard: JsValue, node: JsValue, value: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let before = self.record_before(artboard_id, node_id);
        let dto: crate::dto::ParticleSystemDto = serde_wasm_bindgen::from_value(value).map_err(|err| JsValue::from_str(&format!("invalid particle system: {err}")))?;
        self.document.apply(Command::SetParticleSystem { artboard: artboard_id, node: node_id, value: dto.into() });
        self.record_field_edits(artboard_id, node_id, before);
        self.document_summary()
    }

    /// Restarts `node`'s live particle simulation from scratch — the
    /// Inspector's "Replay" button (`ParticleSystemFields`), so a designer
    /// can re-trigger a burst/preview the effect repeatedly without
    /// switching modes or scrubbing the Timeline. Not a `Command` (nothing
    /// here is document/undo-worthy — `particle_runtimes` is pure live
    /// render state, the same "not part of the saved document" category
    /// `mesh_cache`/`image_cache` are in); removing the entry is enough —
    /// `render`'s own pre-pass (`particle_runtimes.entry(node_id).or_default()`)
    /// lazily recreates a fresh, empty `ParticleSystemRuntime` the very next
    /// frame, which reads as the simulation restarting at `t = 0`. Harmless,
    /// not an error, if `node` has no live runtime yet (nothing to replay).
    /// The Inspector's "Add Component" menu: a fresh component of `kind`
    /// (a `NodeBehaviorDto` kind name — "Camera", "Light2D",
    /// "TrailRenderer", "ParticleSystem", "ShadowCaster2D", "Rigidbody2D",
    /// "BoxCollider2D", "CircleCollider2D", "CapsuleCollider2D",
    /// "PolygonCollider2D") with Unity's defaults, as the frontend appends
    /// it to the node's component list. Rust owns the defaults so the UI
    /// never drifts from them.
    /// A new image effect of `kind` (`"Blur"`, `"RibbedGlass"` …) with its
    /// defaults — the Image Effects component's Add menu.
    pub fn image_effect_default(&self, kind: &str) -> Result<JsValue, JsValue> {
        let effect = an1mbyt3_core::default_image_effect(kind).ok_or_else(|| JsValue::from_str(&format!("image_effect_default: unknown effect {kind}")))?;
        to_js_value(&effect)
    }

    /// A Code effect's `@param` / `@color` lines: `{ params, colors }`.
    pub fn code_effect_params(&self, code: &str) -> Result<JsValue, JsValue> {
        let (params, colors) = an1mbyt3_core::parse_code_params(code);
        to_js_value(&serde_json::json!({ "params": params, "colors": colors }))
    }

    /// Why a Code effect doesn't compile (`undefined` = it does). `params` /
    /// `colors` are its current names, in order.
    pub fn code_effect_error(&self, code: &str, params: Vec<String>, colors: Vec<String>) -> Option<String> {
        let mut src = String::new();
        for (i, n) in params.iter().enumerate() {
            src.push_str(&format!("//#param {i} {n}
"));
        }
        for (i, n) in colors.iter().enumerate() {
            src.push_str(&format!("//#color {i} {n}
"));
        }
        src.push_str(code);
        self.renderer.code_effect_error(&src)
    }

    pub fn default_component(&self, kind: &str) -> Result<JsValue, JsValue> {
        use an1mbyt3_core::NodeBehavior as B;
        let behavior = match kind {
            "Camera" => B::Camera(an1mbyt3_core::CameraNode::default()),
            "Light2D" => B::Light2D(an1mbyt3_core::Light2DNode::default()),
            "TrailRenderer" => B::TrailRenderer(an1mbyt3_core::TrailNode::default()),
            "ParticleSystem" => B::ParticleSystem(an1mbyt3_core::ParticleSystemNode::default()),
            "ShadowCaster2D" => B::ShadowCaster2D(an1mbyt3_core::ShadowCaster2D::default()),
            "Rigidbody2D" => B::Rigidbody2D(an1mbyt3_core::Rigidbody2D::default()),
            "BoxCollider2D" => B::BoxCollider2D(an1mbyt3_core::BoxCollider2D::default()),
            "CircleCollider2D" => B::CircleCollider2D(an1mbyt3_core::CircleCollider2D::default()),
            "CapsuleCollider2D" => B::CapsuleCollider2D(an1mbyt3_core::CapsuleCollider2D::default()),
            "PolygonCollider2D" => B::PolygonCollider2D(an1mbyt3_core::PolygonCollider2D::default()),
            "HingeJoint2D" => B::HingeJoint2D(an1mbyt3_core::HingeJoint2D::default()),
            "IkConstraint2D" => B::IkConstraint2D(an1mbyt3_core::IkConstraint2D::default()),
            "SpriteSkin2D" => B::SpriteSkin2D(an1mbyt3_core::SpriteSkin2D::default()),
            "Liquid2D" => B::Liquid2D(an1mbyt3_core::Liquid2D::default()),
            // The first imported sound, if there is one.
            "UiAnchor" => B::UiAnchor(an1mbyt3_core::UiAnchor::default()),
            "UiStack" => B::UiStack(an1mbyt3_core::UiStack::default()),
            "UiButton" => B::UiButton(an1mbyt3_core::UiButton::default()),
            "UiSlider" => B::UiSlider(an1mbyt3_core::UiSlider::default()),
            "UiScrollbar" => B::UiScrollbar(an1mbyt3_core::UiScrollbar::default()),
            "UiScrollView" => B::UiScrollView(an1mbyt3_core::UiScrollView::default()),
            "UiToggle" => B::UiToggle(an1mbyt3_core::UiToggle::default()),
            "UiLayout" => B::UiLayout(an1mbyt3_core::UiLayout::default()),
            "UiLayoutItem" => B::UiLayoutItem(an1mbyt3_core::UiLayoutItem::default()),
            "UiShowWhen" => B::UiShowWhen(an1mbyt3_core::UiShowWhen::default()),
            "UiHtml" => B::UiHtml(an1mbyt3_core::UiHtml::default()),
            "Rigidbody3D" => B::Rigidbody3D(an1mbyt3_core::Rigidbody3D::default()),
            "Collider3D" => B::Collider3D(an1mbyt3_core::Collider3D::default()),
            "Flipbook" => B::Flipbook(an1mbyt3_core::Flipbook::default()),
            "Cloth2D" => B::Cloth2D(an1mbyt3_core::Cloth2D::default()),
            "Tree2D" => B::Tree2D(an1mbyt3_core::Tree2D::default()),
            "UiEvents" => B::UiEvents(an1mbyt3_core::UiEvents::default()),
            "UiListeners" => B::UiListeners(an1mbyt3_core::UiListeners::default()),
            "PropertyBindings" => B::PropertyBindings(an1mbyt3_core::PropertyBindings::default()),
            "ParticleSystem3D" => B::ParticleSystem3D(an1mbyt3_core::ParticleSystem3D::default()),
            "ParticleFluid2D" => B::ParticleFluid2D(an1mbyt3_core::ParticleFluid2D::default()),
            "ParticleBehaviors2D" => B::ParticleBehaviors2D(an1mbyt3_core::ParticleBehaviors2D::default()),
            "Goo2D" => B::Goo2D(an1mbyt3_core::Goo2D::default()),
            "Antialias2D" => B::Antialias2D(an1mbyt3_core::Antialias2D::default()),
            "VrmPoses" => B::VrmPoses(an1mbyt3_core::VrmPoses::default()),
            "Ragdoll3D" => B::Ragdoll3D(an1mbyt3_core::Ragdoll3D::default()),
            "MaterialSlots" => B::MaterialSlots(an1mbyt3_core::MaterialSlots::default()),
            "HumanoidAnimator" => B::HumanoidAnimator(an1mbyt3_core::HumanoidAnimator::default()),
            "FootIK" => B::FootIK(an1mbyt3_core::FootIK::default()),
            "BoneAttachment" => B::BoneAttachment(an1mbyt3_core::BoneAttachment::default()),
            "GooMaterial2D" => B::GooMaterial2D(an1mbyt3_core::GooMaterial2D::default()),
            "ParticleLook2D" => B::ParticleLook2D(an1mbyt3_core::ParticleLook2D::default()),
            "SortingLayer2D" => B::SortingLayer2D(an1mbyt3_core::SortingLayer2D::default()),
            "LightLayers2D" => B::LightLayers2D(an1mbyt3_core::LightLayers2D::default()),
            "LightFlicker2D" => B::LightFlicker2D(an1mbyt3_core::LightFlicker2D::default()),
            "DropShadow2D" => B::DropShadow2D(an1mbyt3_core::DropShadow2D::default()),
            "Outline2D" => B::Outline2D(an1mbyt3_core::Outline2D::default()),
            "Repeater2D" => B::Repeater2D(an1mbyt3_core::Repeater2D::default()),
            "Mirror2D" => B::Mirror2D(an1mbyt3_core::Mirror2D::default()),
            "Wiggle2D" => B::Wiggle2D(an1mbyt3_core::Wiggle2D::default()),
            "AntiGoo2D" => B::AntiGoo2D(an1mbyt3_core::AntiGoo2D::default()),
            "GooGloss2D" => B::GooGloss2D(an1mbyt3_core::GooGloss2D::default()),
            "Parallax2D" => B::Parallax2D(an1mbyt3_core::Parallax2D::default()),
            "PixelSnap2D" => B::PixelSnap2D(an1mbyt3_core::PixelSnap2D::default()),
            "StrokeStyle2D" => B::StrokeStyle2D(an1mbyt3_core::StrokeStyle2D::default()),
            "CameraShake2D" => B::CameraShake2D(an1mbyt3_core::CameraShake2D::default()),
            "ParticlePath2D" => B::ParticlePath2D(an1mbyt3_core::ParticlePath2D::default()),
            "Mask2D" => B::Mask2D(an1mbyt3_core::Mask2D::default()),
            "ImageEffects" => B::ImageEffects(an1mbyt3_core::ImageEffects::default()),
            "Reference2D" => B::Reference2D(an1mbyt3_core::Reference2D::default()),
            "AudioSource" => B::AudioSource(an1mbyt3_core::AudioSource { clip: self.document.audio_assets().next().map(|(id, _)| id), ..Default::default() }),
            other => return Err(JsValue::from_str(&format!("default_component: unknown component {other}"))),
        };
        to_js_value(&crate::dto::NodeBehaviorDto::from(&behavior))
    }

    /// The physics world for `artboard`, if Playground is simulating it.
    fn physics_for(&mut self, artboard: ArtboardId) -> Option<&mut an1mbyt3_physics::PhysicsWorld> {
        self.physics.as_mut().filter(|(scene, _)| *scene == artboard).map(|(_, world)| world)
    }

    /// Script API (`ref.velocity`): a Rigidbody 2D's linear velocity
    /// `[x, y]` in world units/second, or `null` (no body / not playing).
    pub fn physics_velocity(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let velocity = self.physics_for(artboard_id).and_then(|w| w.velocity(node_id)).map(|v| [v.x, v.y]);
        to_js_value(&velocity)
    }

    /// Script API (`ref.velocity = ...`).
    pub fn physics_set_velocity(&mut self, artboard: JsValue, node: JsValue, x: f32, y: f32) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        if let Some(world) = self.physics_for(artboard_id) {
            world.set_velocity(node_id, Vec2::new(x, y));
        }
        Ok(())
    }

    /// Script API (`ref.angularVelocity`, degrees/second); `null` if none.
    pub fn physics_angular_velocity(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let value = self.physics_for(artboard_id).and_then(|w| w.angular_velocity(node_id));
        to_js_value(&value)
    }

    pub fn physics_set_angular_velocity(&mut self, artboard: JsValue, node: JsValue, degrees_per_second: f32) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        if let Some(world) = self.physics_for(artboard_id) {
            world.set_angular_velocity(node_id, degrees_per_second);
        }
        Ok(())
    }

    /// Script API (`ref.addForce(x, y)` / `ref.addImpulse(x, y)`):
    /// `impulse == false` is Unity's ForceMode2D.Force (spread over the next
    /// frame), `true` is ForceMode2D.Impulse (instant).
    pub fn physics_add_force(&mut self, artboard: JsValue, node: JsValue, x: f32, y: f32, impulse: bool) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        if let Some(world) = self.physics_for(artboard_id) {
            if impulse {
                world.add_impulse(node_id, Vec2::new(x, y));
            } else {
                world.add_force(node_id, Vec2::new(x, y));
            }
        }
        Ok(())
    }

    /// Script API (`ctx.raycast`): Unity's `Physics2D.Raycast` — the first
    /// non-trigger collider hit, as `{ node, x, y, normalX, normalY,
    /// distance }`, or `null`. `exclude`: a node whose body to ignore
    /// (usually the caster itself), or `null`.
    #[allow(clippy::too_many_arguments)]
    pub fn physics_raycast(&mut self, artboard: JsValue, ox: f32, oy: f32, dx: f32, dy: f32, max_distance: f32, exclude: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let exclude: Option<NodeId> = parse_id(exclude)?;
        let hit = self.physics_for(artboard_id).and_then(|w| w.raycast(Vec2::new(ox, oy), Vec2::new(dx, dy), max_distance, exclude));
        to_js_value(&hit.map(|h| crate::dto::RayHitDto { node: h.node, x: h.point.x, y: h.point.y, normal_x: h.normal.x, normal_y: h.normal.y, distance: h.distance }))
    }

    /// Script API (`ctx.collisions`): contact/trigger enter/exit events
    /// since the last call, as `{ a, b, started, trigger }` (node ids).
    pub fn physics_events(&mut self, artboard: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let mut events = self.physics_for(artboard_id).map(|w| w.take_events()).unwrap_or_default();
        // 3D contacts come through the same `ctx.collisions` list.
        if let Some(world) = self.physics3d_for(artboard_id) {
            events.extend(world.take_events());
        }
        let events: Vec<crate::dto::ContactEventDto> = events
            .into_iter()
            .map(|e| crate::dto::ContactEventDto { a: e.a, b: e.b, started: e.started, trigger: e.trigger })
            .collect();
        to_js_value(&events)
    }

    /// The 3D physics world for `artboard`, if Playground is simulating it.
    fn physics3d_for(&mut self, artboard: ArtboardId) -> Option<&mut an1mbyt3_physics::physics3d::Physics3DWorld> {
        self.physics3d.as_mut().filter(|(scene, _)| *scene == artboard).map(|(_, world)| world)
    }

    /// Script API (`ref.velocity3`): a Rigidbody 3D's linear velocity
    /// `[x, y, z]` in m/s, or `null`.
    pub fn physics3d_velocity(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let v = self.physics3d_for(artboard_id).and_then(|w| w.velocity(node_id)).map(|v| [v.x, v.y, v.z]);
        to_js_value(&v)
    }

    /// Script API (`ref.angularVelocity3`): degrees/second about each axis.
    pub fn physics3d_angular_velocity(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let v = self.physics3d_for(artboard_id).and_then(|w| w.angular_velocity(node_id)).map(|v| [v.x, v.y, v.z]);
        to_js_value(&v)
    }

    /// Script API: set velocity (`kind` 0) or angular velocity in degrees/s
    /// (`kind` 1).
    #[allow(clippy::too_many_arguments)]
    pub fn physics3d_set_velocity(&mut self, artboard: JsValue, node: JsValue, kind: u32, x: f32, y: f32, z: f32) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        if let Some(world) = self.physics3d_for(artboard_id) {
            let v = glam::Vec3::new(x, y, z);
            if kind == 0 {
                world.set_velocity(node_id, v);
            } else {
                world.set_angular_velocity(node_id, v);
            }
        }
        Ok(())
    }

    /// Script API (`ref.addForce3` / `addImpulse3` / `addTorque3`):
    /// `mode` 0 = force (over the next frame), 1 = impulse, 2 = torque
    /// impulse.
    #[allow(clippy::too_many_arguments)]
    pub fn physics3d_add_force(&mut self, artboard: JsValue, node: JsValue, mode: u32, x: f32, y: f32, z: f32) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        if let Some(world) = self.physics3d_for(artboard_id) {
            let v = glam::Vec3::new(x, y, z);
            match mode {
                0 => world.add_force(node_id, v),
                1 => world.add_impulse(node_id, v),
                _ => world.add_torque_impulse(node_id, v),
            }
        }
        Ok(())
    }

    /// Script API (`ctx.raycast3`): Unity's `Physics.Raycast` — the first
    /// non-trigger 3D collider hit as `{ node, x, y, z, normalX, normalY,
    /// normalZ, distance }`, or `null`.
    #[allow(clippy::too_many_arguments)]
    pub fn physics3d_raycast(&mut self, artboard: JsValue, ox: f32, oy: f32, oz: f32, dx: f32, dy: f32, dz: f32, max_distance: f32, exclude: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let exclude: Option<NodeId> = parse_id(exclude)?;
        let hit = self.physics3d_for(artboard_id).and_then(|w| w.raycast(glam::Vec3::new(ox, oy, oz), glam::Vec3::new(dx, dy, dz), max_distance, exclude));
        to_js_value(&hit.map(|h| crate::dto::RayHit3Dto { node: h.node, x: h.point.x, y: h.point.y, z: h.point.z, normal_x: h.normal.x, normal_y: h.normal.y, normal_z: h.normal.z, distance: h.distance }))
    }

    /// Scene 3D gravity (Inspector → Canvas → Gravity 3D). Undoable.
    pub fn set_artboard_gravity_3d(&mut self, artboard: JsValue, x: f32, y: f32, z: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        self.document.apply(Command::SetArtboardGravity3D { artboard: artboard_id, value: glam::Vec3::new(x, y, z) });
        self.document_summary()
    }

    /// Scene gravity (Inspector → Canvas → Gravity). Undoable.
    pub fn set_artboard_gravity(&mut self, artboard: JsValue, x: f32, y: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        self.document.apply(Command::SetArtboardGravity { artboard: artboard_id, value: Vec2::new(x, y) });
        self.document_summary()
    }

    /// Unity's `ParticleSystem.Emit(count)` for scripts (`ref.emit(n)`):
    /// spawns `count` particles on the next frame at wherever the emitter
    /// is by then, without restarting the system or killing live particles
    /// (unlike `replay_particle_system`). Untracked runtime state — no undo
    /// entry, no document summary returned (called per hit/pickup).
    pub fn emit_particles(&mut self, artboard: JsValue, node: JsValue, count: u32) -> Result<(), JsValue> {
        let _artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        self.particle_runtimes.entry(node_id).or_default().emit(count);
        Ok(())
    }

    /// Sets a node's `z_index` directly, bypassing undo — the per-frame
    /// counterpart to `set_node_z_index`, for scripts that depth-sort every
    /// frame (`ref.zIndex = -distance` in a raycaster: painter's-algorithm
    /// ordering of wall slices and billboards). Same "no history spam"
    /// reasoning as every `preview_*`.
    /// Untracked show/hide for scripts (`ref.visible = ...`) — a game
    /// toggles pooled sprites/lights constantly, which must not flood the
    /// undo stack. Leaving Playground restores the designed value.
    /// The view Playground is rendering through (the Main Camera's frame:
    /// pan + zoom in *physical* canvas pixels per world unit), or `null`
    /// when there's no camera / not playing. The frontend maps the mouse
    /// with it so scripts get world coordinates matching what's on screen.
    pub fn playground_view(&self, artboard: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let view = self.active_camera_frame(artboard_id).map(|f| [f.pan.x, f.pan.y, f.zoom]);
        to_js_value(&view)
    }

    pub fn preview_node_visible(&mut self, artboard: JsValue, node: JsValue, value: bool) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        if let Some(node_ref) = self.document.artboard_mut(artboard_id).and_then(|a| a.node_mut(node_id)) {
            node_ref.visible = value;
        }
        Ok(())
    }

    /// Untracked whole-grid rewrite of a Tilemap (`ref.setTiles([...])`) —
    /// how a script builds a procedurally generated level. `tiles` is
    /// row-major, `map_width * map_height` long, `-1` = empty cell; a
    /// wrong-length array is ignored. The tile mesh rebuilds next frame.
    pub fn preview_tilemap_tiles(&mut self, artboard: JsValue, node: JsValue, tiles: Vec<i32>) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        if let Some(node_ref) = self.document.artboard_mut(artboard_id).and_then(|a| a.node_mut(node_id)) {
            if let NodeKind::Tilemap(tilemap) = &mut node_ref.kind {
                if tiles.len() == tilemap.tiles.len() {
                    tilemap.tiles = tiles.into_iter().map(|t| u32::try_from(t).ok()).collect();
                }
            }
        }
        Ok(())
    }

    /// Untracked Light 2D tweak for scripts (`ref.lightIntensity`,
    /// `ref.lightRadius`) — torch flicker, hit flashes, pulsing auras.
    /// Negative values leave that field unchanged.
    pub fn preview_light(&mut self, artboard: JsValue, node: JsValue, intensity: f32, outer_radius: f32) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        if let Some(light) = self.document.artboard_mut(artboard_id).and_then(|a| a.node_mut(node_id)).and_then(|n| n.light2d_mut()) {
            if intensity >= 0.0 {
                light.intensity = intensity;
            }
            if outer_radius >= 0.0 {
                light.outer_radius = outer_radius;
                light.inner_radius = light.inner_radius.min(outer_radius);
            }
        }
        Ok(())
    }

    pub fn preview_node_z_index(&mut self, artboard: JsValue, node: JsValue, value: f32) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let artboard_ref = self.document.artboard_mut(artboard_id).ok_or_else(|| JsValue::from_str("preview_node_z_index: unknown artboard"))?;
        let node_ref = artboard_ref.node_mut(node_id).ok_or_else(|| JsValue::from_str("preview_node_z_index: unknown node"))?;
        node_ref.z_index = value;
        Ok(())
    }

    pub fn replay_particle_system(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let _artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        self.particle_runtimes.remove(&node_id);
        self.document_summary()
    }

    /// Changes a `Text` node's authored string — the Inspector's content
    /// field. Undoable (`Command::SetTextContent`).
    pub fn set_text_content(&mut self, artboard: JsValue, node: JsValue, value: String) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        self.document.apply(Command::SetTextContent { artboard: artboard_id, node: node_id, value });
        self.document_summary()
    }

    /// Sets a `Text` node's content directly, bypassing undo — the
    /// continuous-per-frame counterpart to `set_text_content`, same
    /// reasoning as `preview_fill_color`'s own doc comment (a live HUD
    /// number rewritten every frame through the undoable command would
    /// push one undo entry per frame). A no-op (not an error) if `node`
    /// isn't a `Text` node.
    pub fn preview_text_content(&mut self, artboard: JsValue, node: JsValue, value: String) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let artboard_ref = self.document.artboard_mut(artboard_id).ok_or_else(|| JsValue::from_str("preview_text_content: unknown artboard"))?;
        let node_ref = artboard_ref.node_mut(node_id).ok_or_else(|| JsValue::from_str("preview_text_content: unknown node"))?;
        if let NodeKind::Text(text) = &mut node_ref.kind {
            text.content = value;
        }
        Ok(())
    }

    /// Swaps which baked `ImageAsset` an `Image` node draws — untracked
    /// (bypasses undo), same reasoning as `preview_fill_color`/
    /// `preview_text_content`: a script-driven sprite-sheet frame swap fires
    /// every single tick (e.g. a 12fps walk cycle), and every candidate
    /// frame is already baked into `image_cache` up front by whatever demo
    /// builder registered them (e.g. the demo builders' `load_*_demo`
    /// bakes), so this is a plain `ImageId` swap, no new baking here.
    /// A no-op if `node` isn't an `Image` node — same "degrades harmlessly"
    /// convention every other `preview_*` setter follows for the wrong kind.
    pub fn preview_node_image(&mut self, artboard: JsValue, node: JsValue, image: JsValue) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let image_id: an1mbyt3_core::ImageId = parse_id(image)?;
        let artboard_ref = self.document.artboard_mut(artboard_id).ok_or_else(|| JsValue::from_str("preview_node_image: unknown artboard"))?;
        let node_ref = artboard_ref.node_mut(node_id).ok_or_else(|| JsValue::from_str("preview_node_image: unknown node"))?;
        if let NodeKind::Image(image_node) = &mut node_ref.kind {
            image_node.image = image_id;
        }
        Ok(())
    }

    /// Changes a `Text` node's `size_px` — the Inspector's size field.
    /// Undoable (`Command::SetTextSize`).
    /// Registers a font file's bytes (TTF / OTF — e.g. a Google Font the
    /// editor downloaded) as a `FontAsset` named `name`, and returns its
    /// `FontId`. A font with the same name already on the document is
    /// reused rather than duplicated. Errors if the bytes aren't a font the
    /// shaper can read (WOFF2 isn't — the editor fetches TTF).
    pub fn add_font(&mut self, name: String, bytes: Vec<u8>) -> Result<JsValue, JsValue> {
        if let Some((id, _)) = self.document.fonts().find(|(_, f)| f.name == name) {
            return Ok(serde_wasm_bindgen::to_value(&id)?);
        }
        Font::from_bytes(bytes.clone(), 0).map_err(|e| JsValue::from_str(&format!("add_font: {name} isn't a readable TTF/OTF font ({e:?})")))?;
        let id = self.document.add_font(FontAsset { name, data: bytes });
        Ok(serde_wasm_bindgen::to_value(&id)?)
    }

    /// Emits `count` particles at once from `node`'s Particle System 3D
    /// (On Click "Burst", scripts' `ref.burst3(n)`). False if it has none.
    pub fn burst_particles_3d(&mut self, artboard: JsValue, node: JsValue, count: u32) -> Result<bool, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let Some(artboard) = self.document.artboard(artboard_id) else { return Ok(false) };
        let Some(world) = artboard.world_matrix_3d(node_id) else { return Ok(false) };
        let Some(cfg) = artboard.node(node_id).and_then(|n| {
            n.behaviors.iter().find_map(|e| match &e.behavior {
                an1mbyt3_core::NodeBehavior::ParticleSystem3D(p) if e.enabled => Some(p.clone()),
                _ => None,
            })
        }) else {
            return Ok(false);
        };
        self.particles3d.burst(node_id, &cfg, world, count);
        Ok(true)
    }

    /// Switches a `Text` node to another of the document's fonts (one
    /// undoable step).
    pub fn set_text_font(&mut self, artboard: JsValue, node: JsValue, font: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let font_id: FontId = parse_id(font)?;
        if self.document.font(font_id).is_none() {
            return Err(JsValue::from_str("set_text_font: unknown font"));
        }
        self.document.apply(Command::SetTextFont { artboard: artboard_id, node: node_id, value: font_id });
        self.document_summary()
    }

    pub fn set_text_size(&mut self, artboard: JsValue, node: JsValue, value: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let before = self.record_before(artboard_id, node_id);
        self.document.apply(Command::SetTextSize { artboard: artboard_id, node: node_id, value });
        self.record_field_edits(artboard_id, node_id, before);
        self.document_summary()
    }

    /// Solves the classic two-bone IK problem for `root`/`mid` (must both be
    /// `NodeKind::Bone`, `mid` parented under `root`) against `target_x`/
    /// `target_y` (artboard-space), and commits the result as one undoable
    /// edit — a `Command::Batch` of the two bones' `SetRotation`s, same
    /// "batch several primitive commands as one user-facing undo step"
    /// convention `editor.rs`'s `apply_transform_edit` already established.
    /// This is deliberately the only IK surface: there's no saved, authored
    /// "IK constraint" document object yet (AN1MBYT3_BUILD_PLAN.md §5's
    /// constraint-graph vision is bigger than this pass), so posing a rig via
    /// IK is a one-shot "solve once, bake the resulting rotations" tool — the
    /// Bones panel's drag-the-target handle calls this on every drag-move
    /// frame, and each call replaces the previous undo entry's *intent* but
    /// still pushes a real one (matching `commit_node_translation`'s
    /// preview/commit split would need a separate un-committed preview path;
    /// left for later since a designer dragging an IK handle a few times
    /// producing a few undo steps is a minor inconvenience, not a bug).
    pub fn solve_two_bone_ik(&mut self, artboard: JsValue, root: JsValue, mid: JsValue, target_x: f32, target_y: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let root_id: NodeId = parse_id(root)?;
        let mid_id: NodeId = parse_id(mid)?;

        let artboard_ref = self
            .document
            .artboard(artboard_id)
            .ok_or_else(|| JsValue::from_str("solve_two_bone_ik: unknown artboard"))?;
        let root_node = artboard_ref.node(root_id).ok_or_else(|| JsValue::from_str("solve_two_bone_ik: unknown root bone"))?;
        let mid_node = artboard_ref.node(mid_id).ok_or_else(|| JsValue::from_str("solve_two_bone_ik: unknown mid bone"))?;
        let (NodeKind::Bone(root_bone), NodeKind::Bone(mid_bone)) = (&root_node.kind, &mid_node.kind) else {
            return Err(JsValue::from_str("solve_two_bone_ik: root/mid must both be Bone nodes"));
        };
        // The root bone's own world-space origin — `solve_two_bone_ik` needs
        // this (not just the root's local translation) since a root bone can
        // itself be nested under some other node (a group, say).
        let root_origin = artboard_ref.world_transform(root_id).translation;
        let solution = an1mbyt3_constraints::solve_two_bone_ik(root_origin, root_bone.length, mid_bone.length, Vec2::new(target_x, target_y));

        self.document.apply(Command::Batch(vec![
            Command::SetRotation { artboard: artboard_id, node: root_id, value: solution.root_rotation },
            Command::SetRotation { artboard: artboard_id, node: mid_id, value: solution.mid_local_rotation },
        ]));
        self.document_summary()
    }

    /// "Add IK Target": creates an empty target node at `bone`'s tip (scene
    /// root, so it doesn't move with the rig) and gives the bone an IK
    /// Constraint 2D reaching for it over `chain_length` bones — drag the
    /// target and the limb follows. One undoable step. Returns the summary;
    /// the new target is the last node added.
    pub fn add_ik_target(&mut self, artboard: JsValue, bone: JsValue, chain_length: u32) -> Result<JsValue, JsValue> {
        use an1mbyt3_core::{IkConstraint2D, NodeBehavior, NodeBehaviorEntry};
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let bone_id: NodeId = parse_id(bone)?;
        let artboard_ref = self.document.artboard(artboard_id).ok_or_else(|| JsValue::from_str("add_ik_target: unknown artboard"))?;
        let bone_node = artboard_ref.node(bone_id).ok_or_else(|| JsValue::from_str("add_ik_target: unknown bone"))?;
        let NodeKind::Bone(bone_data) = &bone_node.kind else { return Err(JsValue::from_str("add_ik_target: not a bone")) };
        let tip = artboard_ref.world_transform(bone_id).to_affine2().transform_point2(Vec2::new(bone_data.length, 0.0));
        let mut target = Node::new(format!("{} IK Target", bone_node.name), None, NodeKind::Group(GroupNode::default()));
        target.transform.translation = tip;
        let mut behaviors = bone_node.behaviors.clone();
        behaviors.retain(|e| !matches!(e.behavior, NodeBehavior::IkConstraint2D(_)));
        let add_inverse = Command::AddNode { artboard: artboard_id, node: target }.apply(&mut self.document);
        let Command::SetDeleted { node: target_id, .. } = add_inverse else { unreachable!("AddNode's inverse is always a SetDeleted") };
        behaviors.push(NodeBehaviorEntry {
            behavior: NodeBehavior::IkConstraint2D(IkConstraint2D { target: Some(target_id), chain_length: chain_length.max(1), ..Default::default() }),
            enabled: true,
        });
        let set_inverse = Command::SetNodeBehaviors { artboard: artboard_id, node: bone_id, value: behaviors }.apply(&mut self.document);
        self.document.commit_batch(vec![set_inverse, add_inverse]);
        self.document_summary()
    }

    /// "Bind to Bones": skins the Image `node` to every bone in the rig
    /// rooted at `root_bone` (Sprite Skin 2D), recording the current pose as
    /// the bind pose. One undoable step; re-binding replaces the old skin.
    pub fn bind_sprite_skin(&mut self, artboard: JsValue, node: JsValue, root_bone: JsValue, columns: u32, rows: u32) -> Result<JsValue, JsValue> {
        use an1mbyt3_core::{affine_to_array, NodeBehavior, NodeBehaviorEntry, SpriteSkin2D};
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let root_id: NodeId = parse_id(root_bone)?;
        let artboard_ref = self.document.artboard(artboard_id).ok_or_else(|| JsValue::from_str("bind_sprite_skin: unknown artboard"))?;
        let node_ref = artboard_ref.node(node_id).ok_or_else(|| JsValue::from_str("bind_sprite_skin: unknown node"))?;
        if !matches!(node_ref.kind, NodeKind::Image(_)) {
            return Err(JsValue::from_str("bind_sprite_skin: only Image nodes can be skinned"));
        }
        let bones: Vec<NodeId> = subtree(artboard_ref, root_id)
            .into_iter()
            .filter(|id| artboard_ref.node(*id).is_some_and(|n| matches!(n.kind, NodeKind::Bone(_))))
            .collect();
        if bones.is_empty() {
            return Err(JsValue::from_str("bind_sprite_skin: no bones under that root"));
        }
        let skin = SpriteSkin2D {
            bind_poses: bones.iter().map(|b| affine_to_array(artboard_ref.world_transform(*b).to_affine2())).collect(),
            bones,
            bind_node: affine_to_array(artboard_ref.world_transform(node_id).to_affine2()),
            columns: columns.clamp(1, 64),
            rows: rows.clamp(1, 64),
            ..SpriteSkin2D::default()
        };
        let mut behaviors: Vec<NodeBehaviorEntry> = node_ref.behaviors.iter().filter(|e| !matches!(e.behavior, NodeBehavior::SpriteSkin2D(_))).cloned().collect();
        behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::SpriteSkin2D(skin), enabled: true });
        self.document.apply(Command::SetNodeBehaviors { artboard: artboard_id, node: node_id, value: behaviors });
        self.document_summary()
    }

    /// "Make Ragdoll": turns the bone rig rooted at `root` into jointed
    /// physics, one undoable step. Every bone in the subtree gets a
    /// Rigidbody 2D, a capsule collider along its length (`thickness` px
    /// wide) and — except the root — a Hinge Joint 2D to its parent bone
    /// at its joint, limited to ±`limit_degrees`. `animated`: start the
    /// bodies Kinematic (they follow animation/IK until a script switches
    /// them to Dynamic, e.g. `ref.ragdoll(true)` on death) instead of
    /// collapsing as soon as Playground starts.
    pub fn make_ragdoll(&mut self, artboard: JsValue, root: JsValue, thickness: f32, limit_degrees: f32, animated: bool) -> Result<JsValue, JsValue> {
        use an1mbyt3_core::{CapsuleCollider2D, CapsuleDirection2D, HingeJoint2D, NodeBehavior, NodeBehaviorEntry, Rigidbody2D, RigidbodyType2D};
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let root_id: NodeId = parse_id(root)?;
        let artboard_ref = self.document.artboard(artboard_id).ok_or_else(|| JsValue::from_str("make_ragdoll: unknown artboard"))?;
        let mut commands = Vec::new();
        for id in subtree(artboard_ref, root_id) {
            let Some(node) = artboard_ref.node(id) else { continue };
            let NodeKind::Bone(bone) = &node.kind else { continue };
            let is_root = id == root_id || !node.parent.and_then(|p| artboard_ref.node(p)).is_some_and(|p| matches!(p.kind, NodeKind::Bone(_)));
            // Keep unrelated components; replace any physics ones.
            let mut behaviors: Vec<NodeBehaviorEntry> = node
                .behaviors
                .iter()
                .filter(|e| {
                    !matches!(
                        e.behavior,
                        NodeBehavior::Rigidbody2D(_)
                            | NodeBehavior::HingeJoint2D(_)
                            | NodeBehavior::BoxCollider2D(_)
                            | NodeBehavior::CircleCollider2D(_)
                            | NodeBehavior::CapsuleCollider2D(_)
                            | NodeBehavior::PolygonCollider2D(_)
                    )
                })
                .cloned()
                .collect();
            let length = bone.length.max(1.0);
            let width = thickness.clamp(1.0, length);
            let body_type = if animated { RigidbodyType2D::Kinematic } else { RigidbodyType2D::Dynamic };
            behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::Rigidbody2D(Rigidbody2D { body_type, mass: (length / 32.0).max(0.2), angular_drag: 0.5, ..Default::default() }), enabled: true });
            behaviors.push(NodeBehaviorEntry {
                behavior: NodeBehavior::CapsuleCollider2D(CapsuleCollider2D {
                    offset: Vec2::new(length / 2.0, 0.0),
                    size: Vec2::new(length.max(width), width),
                    direction: CapsuleDirection2D::Horizontal,
                    ..Default::default()
                }),
                enabled: true,
            });
            if !is_root {
                behaviors.push(NodeBehaviorEntry {
                    behavior: NodeBehavior::HingeJoint2D(HingeJoint2D { lower_angle: -limit_degrees.abs(), upper_angle: limit_degrees.abs(), ..Default::default() }),
                    enabled: true,
                });
            }
            commands.push(Command::SetNodeBehaviors { artboard: artboard_id, node: id, value: behaviors });
        }
        if !commands.is_empty() {
            self.document.apply(Command::Batch(commands));
        }
        self.document_summary()
    }

    /// Script API (`ref.bodyType = ...`, `ref.ragdoll(on)`): switches a
    /// Rigidbody 2D between Dynamic / Kinematic / Static — untracked,
    /// restored on leaving Playground. `deep`: also every descendant's
    /// body (a whole ragdoll at once). The physics world rebuilds next
    /// frame, keeping each body's velocity, so an animated (Kinematic)
    /// rig goes limp *with* its momentum.
    /// Untracked joint break for scripts (`ref.breakJoint()`): disables the
    /// node's Hinge Joint 2D, so the physics world (whose build signature
    /// includes "has an enabled hinge") rebuilds next step without it — a
    /// ragdoll limb coming off, keeping its velocity. Its own children stay
    /// hinged to it. Same "no history spam" reasoning as every `preview_*`;
    /// leaving Playground restores the designed document.
    /// A parsed + uploaded imported model (cached; `None` if it's missing
    /// or fails to parse).
    fn gpu_model(&mut self, id: an1mbyt3_core::ModelId) -> Option<std::rc::Rc<GpuModel>> {
        if let Some(m) = self.model_gpu.get(&id) {
            return Some(std::rc::Rc::clone(m));
        }
        let asset = self.document.model(id)?;
        let data = an1mbyt3_geometry::parse_gltf(&asset.source, &|uri| asset.file(uri).map(|b| b.to_vec())).ok()?;
        // Smooth for painted avatars (VRM); hard texels for low-poly palette models.
        let filter = if data.vrm.is_some() { an1mbyt3_renderer::ImageFilterMode::Linear } else { an1mbyt3_renderer::ImageFilterMode::Nearest };
        let textures: Vec<Option<an1mbyt3_renderer::GpuImageTexture>> = data
            .images
            .iter()
            .map(|bytes| {
                // Any PNG / JPEG flavour (Kenney's textures are palette PNGs).
                let (rgba, w, h) = an1mbyt3_image::decode_any_to_rgba(bytes).ok()?;
                // Nearest: low-poly models ship small palette textures.
                // Repeat: glTF's default sampler wraps (atlas models rely on it).
                Some(self.renderer.upload_texture_wrapped(w, h, &rgba, filter, true))
            })
            .collect();
        let primitives = data
            .primitives
            .iter()
            .map(|p| {
                // Skinned meshes carry their joints / weights to the GPU.
                let gpu = if p.skin.is_some() { self.renderer.upload_mesh_3d_skinned(&p.mesh, &p.joints, &p.weights) } else { self.renderer.upload_mesh_3d(&p.mesh) };
                (std::rc::Rc::new(gpu), p.image.and_then(|i| textures.get(i).cloned().flatten()))
            })
            .collect();
        let rest = data.rest_pose();
        let parents = data.parents();
        let humanoid = an1mbyt3_geometry::retarget::HumanoidTarget::of(&data);
        let gpu = std::rc::Rc::new(GpuModel { data, primitives, rest, parents, humanoid });
        self.model_gpu.insert(id, std::rc::Rc::clone(&gpu));
        Some(gpu)
    }

    /// Rest-pose bounds of every model parsed so far (picking / selection).
    fn model_bounds_map(&self) -> HashMap<an1mbyt3_core::ModelId, (glam::Vec3, glam::Vec3)> {
        self.model_gpu.iter().map(|(id, m)| (*id, m.data.bounds)).collect()
    }

    /// The built-in "texture × tint" material textured model meshes use.
    fn textured_material(&mut self) -> Option<std::rc::Rc<an1mbyt3_renderer::GpuMaterial3D>> {
        if self.textured_material_3d.is_none() {
            let wgsl = an1mbyt3_core::compile_wgsl(&an1mbyt3_core::ShaderGraphDef::starter()).ok()?;
            self.textured_material_3d = self.renderer.compile_material_3d(&wgsl).ok().map(std::rc::Rc::new);
        }
        self.textured_material_3d.clone()
    }

    /// Draws one model mesh node: every primitive, posed by its animation
    /// clips (advancing the node's clock by `delta`).
    #[allow(clippy::too_many_arguments)]
    fn model_draws(
        &mut self,
        node_id: NodeId,
        model_id: an1mbyt3_core::ModelId,
        world: glam::Mat4,
        mesh: &an1mbyt3_core::Mesh3D,
        material: Option<std::rc::Rc<an1mbyt3_renderer::GpuMaterial3D>>,
        delta: f32,
        poses: Option<&an1mbyt3_core::VrmPoses>,
        node_toon: Option<&an1mbyt3_core::ToonMaterial>,
        slot_overrides: Option<&an1mbyt3_core::MaterialSlots>,
        animator: Option<&an1mbyt3_core::HumanoidAnimator>,
        foot_ik: Option<&an1mbyt3_core::FootIK>,
        draws: &mut Vec<an1mbyt3_renderer::MeshDraw3D>,
    ) {
        let Some(model) = self.gpu_model(model_id) else { return };
        let textured = self.textured_material();
        // Clocks restart when a clip changes.
        let wanted = [mesh.animation, mesh.animation_overlay];
        let clock = self.anim_clocks.entry(node_id).or_insert(AnimClock { clips: wanted, times: [0.0; 2] });
        for i in 0..2 {
            if clock.clips[i] != wanted[i] {
                clock.clips[i] = wanted[i];
                clock.times[i] = 0.0;
            }
            clock.times[i] += delta;
        }
        let mut layers = [(None, 0.0f32); 2];
        for i in 0..2 {
            let Some(name) = wanted[i] else { continue };
            let Some(clip) = model.data.clip(name.as_str()) else { continue };
            let duration = model.data.animations[clip].duration.max(1e-3);
            let t = clock.times[i];
            // The overlay always loops; the base loops unless told to hold.
            let t = if i == 1 || mesh.animation_loop { t % duration } else { t.min(duration) };
            layers[i] = (Some(clip), t);
        }
        let data = &model.data;
        let mut local = data.local_pose(&layers);
        // Runtime state (taken out of the map so `self` stays free below).
        let mut inst = self.model_instances.remove(&node_id).unwrap_or_default();
        if inst.model != Some(model_id) {
            inst = ModelInstance { model: Some(model_id), ..ModelInstance::default() };
        }
        // Humanoid Animator: the playing clip (crossfaded from the last one),
        // retargeted onto this avatar's bones. Poses and scripts go on top.
        if let (Some(a), Some(target)) = (animator, model.humanoid.as_ref()) {
            if self.playground_active || a.preview {
                let (want, looping, speed, blend) = match &inst.anim_override {
                    Some((n, l, s, b)) => (n.clone(), l.unwrap_or(a.looping), s.unwrap_or(a.speed), b.unwrap_or(a.blend)),
                    None => (a.playing.clone(), a.looping, a.speed, a.blend),
                };
                if inst.anim.as_ref().map(|(n, _)| n.as_str()) != Some(want.as_str()) {
                    let old = inst.anim.take();
                    inst.anim_prev = old.map(|(n, t)| (n, t, 1.0));
                    inst.anim = (!want.is_empty()).then(|| (want.clone(), 0.0));
                }
                let mut pose = None;
                if let Some((name, t)) = inst.anim.as_mut() {
                    if let Some((clips, i)) = self.humanoid_clip(a, name) {
                        let clip = &clips[i];
                        *t += delta * speed;
                        let d = clip.duration;
                        *t = if looping { t.rem_euclid(d) } else { t.clamp(0.0, d) };
                        pose = Some(clip.sample(*t));
                    }
                }
                if let Some((name, t, fade)) = inst.anim_prev.as_mut() {
                    *fade -= if blend > 0.0 { delta / blend } else { 1.0 };
                    *t += delta * speed;
                    let prev = self.humanoid_clip(a, name).map(|(clips, i)| {
                        let c = &clips[i];
                        c.sample(if looping { t.rem_euclid(c.duration) } else { t.min(c.duration) })
                    });
                    if *fade <= 0.0 {
                        inst.anim_prev = None;
                    } else if let Some(prev) = prev {
                        let w = 1.0 - *fade;
                        pose = Some(match pose {
                            Some(p) => prev.blend(&p, w),
                            None => prev.blend(&an1mbyt3_geometry::retarget::HumanoidPose::default(), w),
                        });
                    }
                }
                if let Some(pose) = pose {
                    target.apply(&pose, 1.0, &model.rest, &mut local);
                }
            }
        }
        // VRM Poses: blend toward the current pose (instantly in Design, so
        // editing shows straight away; crossfaded in Playground).
        if let Some(poses) = poses {
            let name = inst.pose_override.clone().unwrap_or_else(|| poses.active.clone());
            let mut target_bones: HashMap<usize, (glam::Quat, glam::Vec3)> = HashMap::new();
            let mut target_expressions: HashMap<String, f32> = HashMap::new();
            if let Some(pose) = poses.pose(&name) {
                for b in &pose.bones {
                    let Some(index) = Self::model_bone(&model, &b.bone) else { continue };
                    let r = b.rotation;
                    target_bones.insert(index, (glam::Quat::from_euler(glam::EulerRot::YXZ, r[1].to_radians(), r[0].to_radians(), r[2].to_radians()), glam::Vec3::from(b.offset)));
                }
                for e in &pose.expressions {
                    target_expressions.insert(e.name.clone(), e.weight);
                }
            }
            let blend = inst.pose_blend.unwrap_or(poses.blend);
            let k = if self.playground_active && blend > 0.0 { 1.0 - (-delta * 4.0 / blend).exp() } else { 1.0 };
            let keys: std::collections::HashSet<usize> = inst.pose_bones.keys().chain(target_bones.keys()).copied().collect();
            for key in keys {
                let (cq, co) = inst.pose_bones.get(&key).copied().unwrap_or((glam::Quat::IDENTITY, glam::Vec3::ZERO));
                let (tq, to) = target_bones.get(&key).copied().unwrap_or((glam::Quat::IDENTITY, glam::Vec3::ZERO));
                let next = (cq.slerp(tq, k).normalize(), co.lerp(to, k));
                if !target_bones.contains_key(&key) && next.0.angle_between(glam::Quat::IDENTITY) < 1e-3 && next.1.length() < 1e-4 {
                    inst.pose_bones.remove(&key);
                } else {
                    inst.pose_bones.insert(key, next);
                }
            }
            let names: std::collections::HashSet<String> = inst.pose_expressions.keys().chain(target_expressions.keys()).cloned().collect();
            for name in names {
                let c = inst.pose_expressions.get(&name).copied().unwrap_or(0.0);
                let t = target_expressions.get(&name).copied().unwrap_or(0.0);
                let next = c + (t - c) * k;
                if next.abs() < 1e-3 && t == 0.0 {
                    inst.pose_expressions.remove(&name);
                } else {
                    inst.pose_expressions.insert(name, next);
                }
            }
        } else {
            inst.pose_bones.clear();
            inst.pose_expressions.clear();
        }
        for (&n, (q, offset)) in &inst.pose_bones {
            if let Some(l) = local.get_mut(n) {
                l.1 = (l.1 * *q).normalize();
                l.0 += *offset;
            }
        }
        // Script poses go on top of the animation.
        for (&n, q) in &inst.bones {
            if let Some(l) = local.get_mut(n) {
                l.1 = (l.1 * *q).normalize();
            }
        }
        for (&n, offset) in &inst.offsets {
            if let Some(l) = local.get_mut(n) {
                l.0 += *offset;
            }
        }
        // Ragdoll: go limp / get up on request (or at Playground start),
        // and while limp the physics capsules pose the humanoid bones.
        if self.playground_active {
            if let (Some(cfg), false) = (self.ragdoll_config(node_id), inst.ragdoll_started) {
                inst.ragdoll_started = true;
                if cfg.start_ragdolled && inst.ragdoll_request.is_none() {
                    inst.ragdoll_request = Some((true, glam::Vec3::ZERO));
                }
            }
            match inst.ragdoll_request.take() {
                Some((true, velocity)) if !inst.last_pose.is_empty() => {
                    let cfg = self.ragdoll_config(node_id).unwrap_or_default();
                    if let Some((parts, bones)) = ragdoll_parts(&model, &inst.last_pose, inst.last_world, &cfg) {
                        self.pending_ragdolls.push((node_id, parts, velocity));
                        inst.ragdoll_bones = Some(bones);
                    }
                }
                Some((true, v)) => inst.ragdoll_request = Some((true, v)),
                Some((false, _)) => {
                    inst.ragdoll_bones = None;
                    if let Some((_, w)) = self.physics3d.as_mut() {
                        w.remove_ragdoll(node_id);
                    }
                }
                None => {}
            }
        }
        if let (Some(bones), Some(poses)) = (&inst.ragdoll_bones, self.physics3d.as_ref().and_then(|(_, w)| w.ragdoll_poses(node_id))) {
            let (_, node_rot, _) = world.to_scale_rotation_translation();
            let inv_world = world.inverse();
            for (k, (bone, _)) in bones.iter().enumerate() {
                let Some((pos, rot)) = poses.get(k) else { continue };
                // The bone's pose in model space, then local to its parent.
                let model_rot = node_rot.inverse() * *rot;
                let parent = model.parents.get(*bone).copied().flatten().map(|p| data.world_pose(&local)[p]).unwrap_or(glam::Mat4::IDENTITY);
                let (_, parent_rot, _) = parent.to_scale_rotation_translation();
                if let Some(l) = local.get_mut(*bone) {
                    l.1 = (parent_rot.inverse() * model_rot).normalize();
                    // The root part (hips) also moves.
                    if k == 0 {
                        l.0 = parent.inverse().transform_point3(inv_world.transform_point3(*pos));
                    }
                }
            }
        }
        // Foot IK: plant the feet (after every pose layer, before springs).
        if let (Some(cfg), Some(vrm), true) = (foot_ik, data.vrm.as_ref(), inst.ragdoll_bones.is_none()) {
            self.plant_feet(&model, vrm, cfg, world, &mut local);
        }
        // Hair / skirts / ribbons swing (in world space, so moving the
        // avatar moves them).
        if let Some(vrm) = data.vrm.as_ref().filter(|v| !v.springs.is_empty()) {
            inst.springs.step(vrm, &model.rest, &mut local, world, delta, &|l| data.world_pose(l), &model.parents);
        }
        let pose = data.world_pose(&local);
        // Expressions → morph weights per glTF mesh.
        let mut morphs: HashMap<usize, Vec<f32>> = HashMap::new();
        if let Some(vrm) = &data.vrm {
            // The pose's expressions, with the script's on top.
            let mut all: HashMap<&String, f32> = inst.pose_expressions.iter().map(|(k, v)| (k, *v)).collect();
            for (k, v) in &inst.expressions {
                all.insert(k, *v);
            }
            for (name, &weight) in &all {
                let Some(e) = vrm.expression(name).map(|i| &vrm.expressions[i]) else { continue };
                let weight = if e.is_binary { if weight > 0.5 { 1.0 } else { 0.0 } } else { weight.clamp(0.0, 1.0) };
                for &(mesh_index, target, w) in &e.binds {
                    let v = morphs.entry(mesh_index).or_default();
                    if v.len() <= target {
                        v.resize(target + 1, 0.0);
                    }
                    v[target] = (v[target] + w * weight).min(1.0);
                }
            }
        }
        inst.meshes.resize(data.primitives.len(), None);
        inst.morph_keys.resize(data.primitives.len(), Vec::new());
        // Skeletons: one joint palette each, skinned on the GPU.
        let palettes: Vec<std::rc::Rc<Vec<glam::Mat4>>> = data.joint_palettes(&pose).into_iter().map(std::rc::Rc::new).collect();
        let no_morphs = Vec::new();
        // The model's material-slot assets (Toon materials made on import).
        let mut slot_toons: HashMap<u32, an1mbyt3_core::ToonMaterial> = self
            .document
            .materials()
            .filter_map(|(_, m)| match &m.kind {
                an1mbyt3_core::MaterialKind::Toon(t) => t.source.filter(|s| s.model == model_id).map(|s| (s.slot, t.clone())),
                _ => None,
            })
            .collect();
        // This object's own per-slot overrides win.
        for o in slot_overrides.map(|s| s.slots.as_slice()).unwrap_or_default() {
            if let Some(t) = o.material.and_then(|id| self.toon_material(id)) {
                slot_toons.insert(o.slot, t);
            }
        }
        for (i, (prim, (gpu_mesh, texture))) in data.primitives.iter().zip(&model.primitives).enumerate() {
            let c = prim.base_color;
            let color = [mesh.color[0] * c[0], mesh.color[1] * c[1], mesh.color[2] * c[2], mesh.color[3] * c[3]];
            let cutoff = match prim.alpha {
                an1mbyt3_geometry::AlphaMode::Mask(c) => Some(c),
                _ => None,
            };
            // Which toon look: the node's own Toon material (all slots), the
            // slot's material asset, else the VRM's built-in MToon values.
            let toon: Option<an1mbyt3_core::ToonMaterial> = node_toon.cloned().or_else(|| prim.material.and_then(|m| slot_toons.get(&(m as u32)).cloned())).or_else(|| {
                let v = data.vrm.as_ref()?;
                let m = prim.material?;
                let t = v.toon.get(m).copied().flatten()?;
                Some(toon_from_vrm(&t, v.emissive.get(m).copied().unwrap_or([0.0; 3]), cutoff))
            });
            // Skinned / morphed: deform on the CPU, rewrite the GPU mesh in
            // place (the result is already in model space).
            let weights = morphs.get(&prim.mesh_index).unwrap_or(&no_morphs);
            let palette = prim.skin.and_then(|s| palettes.get(s)).cloned();
            let (draw_mesh, model_matrix) = if palette.is_some() {
                // Skinned on the GPU. Morphs (expressions) are applied to
                // this instance's own copy of the mesh, only when they change.
                let morphing = !prim.morph_positions.is_empty() && (weights.iter().any(|w| *w != 0.0) || inst.meshes[i].is_some());
                if morphing && (inst.meshes[i].is_none() || inst.morph_keys[i] != *weights) {
                    data.morph_primitive_interleaved(i, weights, &mut inst.scratch);
                    let slot = &mut inst.meshes[i];
                    if !slot.as_ref().is_some_and(|g| self.renderer.rewrite_mesh_3d_interleaved(g, &inst.scratch)) {
                        let mut base = prim.mesh.clone();
                        for (v, chunk) in inst.scratch.chunks_exact(8).enumerate() {
                            base.positions[v] = [chunk[0], chunk[1], chunk[2]];
                            base.normals[v] = [chunk[3], chunk[4], chunk[5]];
                        }
                        *slot = Some(std::rc::Rc::new(self.renderer.upload_mesh_3d_skinned(&base, &prim.joints, &prim.weights)));
                    }
                    inst.morph_keys[i] = weights.clone();
                }
                (inst.meshes[i].clone().unwrap_or_else(|| std::rc::Rc::clone(gpu_mesh)), world)
            } else if data.deforms(i) {
                // Morph-only (no skeleton): on the CPU.
                data.skin_primitive_interleaved(i, &pose, weights, &mut inst.scratch);
                let slot = &mut inst.meshes[i];
                if !slot.as_ref().is_some_and(|g| self.renderer.rewrite_mesh_3d_interleaved(g, &inst.scratch)) {
                    *slot = Some(std::rc::Rc::new(self.renderer.upload_mesh_3d_interleaved(&inst.scratch, &prim.mesh.indices)));
                }
                (std::rc::Rc::clone(slot.as_ref().expect("just set")), world)
            } else {
                (std::rc::Rc::clone(gpu_mesh), world * pose.get(prim.node).copied().unwrap_or(glam::Mat4::IDENTITY))
            };
            let blend = if prim.alpha == an1mbyt3_geometry::AlphaMode::Blend || color[3] < 0.999 { an1mbyt3_renderer::Blend3D::Transparent } else { an1mbyt3_renderer::Blend3D::Opaque };
            if let Some(mut toon) = toon {
                // A cut-out primitive keeps cutting out under any toon material.
                if toon.alpha_cutoff <= 0.0 {
                    toon.alpha_cutoff = cutoff.unwrap_or(0.0);
                }
                let transparent = prim.alpha == an1mbyt3_geometry::AlphaMode::Blend || color[3] < 0.999;
                self.push_toon_draws(&toon, draw_mesh, model_matrix, color, texture.clone(), palette, transparent, prim.double_sided, draws);
                continue;
            }
            let draw_material = material.clone().or_else(|| texture.as_ref().and(textured.clone()));
            draws.push(an1mbyt3_renderer::MeshDraw3D {
                shininess: mesh.shininess,
                unlit: mesh.unlit,
                material: draw_material,
                texture: texture.clone(),
                blend,
                double_sided: prim.double_sided,
                joints: palette,
                ..an1mbyt3_renderer::MeshDraw3D::new(draw_mesh, model_matrix, color)
            });
        }
        inst.last_pose = pose;
        inst.last_world = world;
        self.model_instances.insert(node_id, inst);
    }

    /// The compiled shader for a Toon material style.
    fn toon_gpu(&mut self, style: an1mbyt3_core::ToonStyle) -> Option<std::rc::Rc<an1mbyt3_renderer::GpuMaterial3D>> {
        let (slot, wgsl) = match style {
            an1mbyt3_core::ToonStyle::MToon => (0, an1mbyt3_renderer::TOON_MTOON_WGSL),
            an1mbyt3_core::ToonStyle::Arcane => (1, an1mbyt3_renderer::TOON_ARCANE_WGSL),
        };
        if self.toon_gpu[slot].is_none() {
            // (Validated by the renderer's `toon_materials_validate_in_3d` test.)
            self.toon_gpu[slot] = self.renderer.compile_material_3d(wgsl).ok().map(std::rc::Rc::new);
        }
        self.toon_gpu[slot].clone()
    }

    /// A Toon material asset's settings (`None` for other kinds / deleted).
    fn toon_material(&self, id: an1mbyt3_core::MaterialId) -> Option<an1mbyt3_core::ToonMaterial> {
        match self.document.material(id).filter(|m| !m.deleted).map(|m| &m.kind) {
            Some(an1mbyt3_core::MaterialKind::Toon(t)) => Some(t.clone()),
            _ => None,
        }
    }

    /// Draws a mesh with a Toon material (+ its ink outline): the shared
    /// toon shader, the material's values per draw, drawn "unlit" since
    /// the shader lights itself.
    #[allow(clippy::too_many_arguments)]
    fn push_toon_draws(
        &mut self,
        toon: &an1mbyt3_core::ToonMaterial,
        mesh: std::rc::Rc<an1mbyt3_renderer::GpuMesh3D>,
        model: glam::Mat4,
        color: [f32; 4],
        texture: Option<an1mbyt3_renderer::GpuImageTexture>,
        joints: Option<std::rc::Rc<Vec<glam::Mat4>>>,
        transparent: bool,
        double_sided: bool,
        draws: &mut Vec<an1mbyt3_renderer::MeshDraw3D>,
    ) {
        let Some(material) = self.toon_gpu(toon.style) else { return };
        let b = toon.base;
        let color = [color[0] * b[0], color[1] * b[1], color[2] * b[2], color[3] * b[3]];
        let texture = toon.texture.and_then(|id| self.image_cache.get(&id).cloned()).or(texture);
        let blend = if (transparent || color[3] < 0.999) && toon.alpha_cutoff <= 0.0 { an1mbyt3_renderer::Blend3D::Transparent } else { an1mbyt3_renderer::Blend3D::Opaque };
        draws.push(an1mbyt3_renderer::MeshDraw3D {
            unlit: true,
            material: Some(material),
            texture,
            blend,
            double_sided: double_sided || toon.double_sided,
            joints: joints.clone(),
            params: toon.gpu_params(),
            ..an1mbyt3_renderer::MeshDraw3D::new(std::rc::Rc::clone(&mesh), model, color)
        });
        // The ink line: the mesh pushed out along its normals in the vertex
        // shader, front faces culled.
        if toon.outline_width > 0.0 && blend == an1mbyt3_renderer::Blend3D::Opaque {
            let o = toon.outline;
            draws.push(an1mbyt3_renderer::MeshDraw3D { unlit: true, joints, outline: toon.outline_width, ..an1mbyt3_renderer::MeshDraw3D::new(mesh, model, [o[0], o[1], o[2], 1.0]) });
        }
    }

    /// Makes an editable Toon material asset for every MToon material slot
    /// of an imported model that doesn't have one yet (VRM import does
    /// this; "Extract materials" in the Inspector for older imports).
    /// Returns how many were made.
    fn extract_model_materials(&mut self, model_id: an1mbyt3_core::ModelId, data: &an1mbyt3_geometry::ModelData, model_name: &str) -> usize {
        let Some(vrm) = &data.vrm else { return 0 };
        let have: std::collections::HashSet<u32> = self
            .document
            .materials()
            .filter_map(|(_, m)| match &m.kind {
                an1mbyt3_core::MaterialKind::Toon(t) => t.source.filter(|s| s.model == model_id).map(|s| s.slot),
                _ => None,
            })
            .collect();
        let mut made = 0;
        for (slot, toon) in vrm.toon.iter().enumerate() {
            let Some(toon) = toon else { continue };
            if have.contains(&(slot as u32)) {
                continue;
            }
            // Its alpha cutoff, if a primitive uses it as a cut-out.
            let cutoff = data.primitives.iter().find(|p| p.material == Some(slot)).and_then(|p| match p.alpha {
                an1mbyt3_geometry::AlphaMode::Mask(c) => Some(c),
                _ => None,
            });
            let mut value = toon_from_vrm(toon, vrm.emissive.get(slot).copied().unwrap_or([0.0; 3]), cutoff);
            value.source = Some(an1mbyt3_core::ModelMaterialSlot { model: model_id, slot: slot as u32 });
            let label = vrm.material_names.get(slot).filter(|n| !n.is_empty()).cloned().unwrap_or_else(|| format!("Material {slot}"));
            let material = an1mbyt3_core::Material { name: format!("{model_name} · {label}"), kind: an1mbyt3_core::MaterialKind::Toon(value), deleted: false };
            self.document.apply(Command::AddMaterial { material });
            made += 1;
        }
        made
    }

    /// The imported model a 3D node shows (and the node), if it is one.
    fn model_of(&mut self, artboard: JsValue, node: JsValue) -> Result<Option<(NodeId, std::rc::Rc<GpuModel>)>, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let model_id = match self.document.artboard(artboard_id).and_then(|a| a.node(node_id)).map(|n| &n.kind) {
            Some(NodeKind::Node3D(an1mbyt3_core::Node3D { content: an1mbyt3_core::Node3DContent::Mesh(an1mbyt3_core::Mesh3D { primitive: an1mbyt3_core::Primitive3D::Model { model }, .. }), .. })) => *model,
            _ => return Ok(None),
        };
        Ok(self.gpu_model(model_id).map(|m| (node_id, m)))
    }

    /// A bone by humanoid name (VRM: `hips`, `leftUpperArm` …) or by the
    /// model's own node name.
    fn model_bone(model: &GpuModel, name: &str) -> Option<usize> {
        model.data.vrm.as_ref().and_then(|v| v.bone(name)).or_else(|| model.data.nodes.iter().position(|n| n.name == name)).or_else(|| model.data.nodes.iter().position(|n| n.name.eq_ignore_ascii_case(name)))
    }

    /// Script API (`ref.setBone3`): turns a bone (humanoid name or node
    /// name) by Euler degrees (Y-X-Z, in the bone's own space) on top of
    /// its animated pose — `0, 0, 0` puts it back. Returns `false` for an
    /// unknown bone.
    #[allow(clippy::too_many_arguments)]
    pub fn model_set_bone(&mut self, artboard: JsValue, node: JsValue, bone: String, x: f32, y: f32, z: f32) -> Result<bool, JsValue> {
        let Some((node_id, model)) = self.model_of(artboard, node)? else { return Ok(false) };
        let Some(index) = Self::model_bone(&model, &bone) else { return Ok(false) };
        let inst = self.model_instances.entry(node_id).or_default();
        if x == 0.0 && y == 0.0 && z == 0.0 {
            inst.bones.remove(&index);
        } else {
            inst.bones.insert(index, glam::Quat::from_euler(glam::EulerRot::YXZ, y.to_radians(), x.to_radians(), z.to_radians()));
        }
        Ok(true)
    }

    /// Script API (`ref.setBoneOffset3`): moves a bone (usually `hips`, for
    /// crouches, hops and lunges) by `x, y, z` in its parent's space.
    #[allow(clippy::too_many_arguments)]
    pub fn model_set_bone_offset(&mut self, artboard: JsValue, node: JsValue, bone: String, x: f32, y: f32, z: f32) -> Result<bool, JsValue> {
        let Some((node_id, model)) = self.model_of(artboard, node)? else { return Ok(false) };
        let Some(index) = Self::model_bone(&model, &bone) else { return Ok(false) };
        let inst = self.model_instances.entry(node_id).or_default();
        if x == 0.0 && y == 0.0 && z == 0.0 {
            inst.offsets.remove(&index);
        } else {
            inst.offsets.insert(index, glam::Vec3::new(x, y, z));
        }
        Ok(true)
    }

    /// Script API (`ref.clearBones3()`): every scripted bone pose off.
    pub fn model_clear_bones(&mut self, artboard: JsValue, node: JsValue) -> Result<(), JsValue> {
        if let Some((node_id, _)) = self.model_of(artboard, node)? {
            let inst = self.model_instances.entry(node_id).or_default();
            inst.bones.clear();
            inst.offsets.clear();
        }
        Ok(())
    }

    /// Script API (`ref.setExpression3`): a VRM expression's weight (0..1)
    /// — `happy`, `angry`, `sad`, `surprised`, `blink`, mouth shapes
    /// `aa`/`ih`/`ou`/`ee`/`oh` …
    pub fn model_set_expression(&mut self, artboard: JsValue, node: JsValue, name: String, weight: f32) -> Result<bool, JsValue> {
        let Some((node_id, model)) = self.model_of(artboard, node)? else { return Ok(false) };
        let known = model.data.vrm.as_ref().is_some_and(|v| v.expression(&name).is_some());
        let inst = self.model_instances.entry(node_id).or_default();
        if weight <= 0.0 {
            inst.expressions.remove(&name);
        } else {
            inst.expressions.insert(name, weight);
        }
        Ok(known)
    }

    /// Script API (`ref.bone3(name)`): a bone's world position last frame,
    /// `[x, y, z]`, or null.
    pub fn model_bone_position(&mut self, artboard: JsValue, node: JsValue, bone: String) -> Result<JsValue, JsValue> {
        let Some((node_id, model)) = self.model_of(artboard, node)? else { return Ok(JsValue::NULL) };
        let Some(index) = Self::model_bone(&model, &bone) else { return Ok(JsValue::NULL) };
        let Some(inst) = self.model_instances.get(&node_id) else { return Ok(JsValue::NULL) };
        let Some(m) = inst.last_pose.get(index) else { return Ok(JsValue::NULL) };
        let p = (inst.last_world * *m).transform_point3(glam::Vec3::ZERO);
        to_js_value(&[p.x, p.y, p.z])
    }

    /// Script API (`ref.modelInfo3`): `{ vrm, version, name, forwardZ,
    /// bones, expressions, clips }` — what a model offers to script.
    pub fn model_info(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let Some((_, model)) = self.model_of(artboard, node)? else { return Ok(JsValue::NULL) };
        let data = &model.data;
        let mut bones: Vec<String> = match &data.vrm {
            Some(v) => v.humanoid.keys().cloned().collect(),
            None => data.nodes.iter().map(|n| n.name.clone()).filter(|n| !n.is_empty()).collect(),
        };
        bones.sort();
        to_js_value(&crate::dto::ModelInfoDto {
            vrm: data.vrm.is_some(),
            version: data.vrm.as_ref().map_or(0, |v| v.version),
            name: data.vrm.as_ref().map_or_else(String::new, |v| v.name.clone()),
            forward_z: data.vrm.as_ref().map_or(1.0, |v| v.forward_z),
            bones,
            expressions: data.vrm.as_ref().map(|v| v.expressions.iter().map(|e| e.name.clone()).collect()).unwrap_or_default(),
            clips: data.animations.iter().map(|c| c.name.clone()).collect(),
        })
    }

    /// Script API (`ref.setPose3(name, blend)`): crossfade to one of the
    /// node's VRM Poses over `blend` seconds (negative = the component's
    /// blend time). An empty name goes back to the component's active pose.
    pub fn model_set_pose(&mut self, artboard: JsValue, node: JsValue, name: String, blend: f32) -> Result<bool, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard.clone())?;
        let node_id: NodeId = parse_id(node)?;
        let known = self.document.artboard(artboard_id).and_then(|a| a.node(node_id)).and_then(|n| n.vrm_poses()).is_some_and(|p| name.is_empty() || p.pose(&name).is_some());
        let inst = self.model_instances.entry(node_id).or_default();
        inst.pose_override = if name.is_empty() { None } else { Some(name) };
        inst.pose_blend = (blend >= 0.0).then_some(blend);
        Ok(known)
    }

    /// Script API (`ref.pose3`): the VRM pose the node is showing (or
    /// blending to).
    pub fn model_pose(&mut self, artboard: JsValue, node: JsValue) -> Result<String, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let active = self.document.artboard(artboard_id).and_then(|a| a.node(node_id)).and_then(|n| n.vrm_poses()).map(|p| p.active.clone()).unwrap_or_default();
        Ok(self.model_instances.get(&node_id).and_then(|i| i.pose_override.clone()).unwrap_or(active))
    }

    /// Script API (`ref.resetSprings3()`): hair and cloth settle from the
    /// current pose (after a teleport, a scene cut).
    pub fn model_reset_springs(&mut self, artboard: JsValue, node: JsValue) -> Result<(), JsValue> {
        if let Some((node_id, _)) = self.model_of(artboard, node)? {
            self.model_instances.entry(node_id).or_default().springs.reset();
        }
        Ok(())
    }

    /// The retargetable clips of an imported animation file (cached).
    fn humanoid_clips_of(&mut self, source: an1mbyt3_core::ModelId) -> std::rc::Rc<Vec<an1mbyt3_geometry::retarget::HumanoidClip>> {
        if let Some(c) = self.humanoid_clips.get(&source) {
            return std::rc::Rc::clone(c);
        }
        let clips = self
            .document
            .model(source)
            .and_then(|asset| {
                let data = an1mbyt3_geometry::parse_gltf(&asset.source, &|uri| asset.file(uri).map(|b| b.to_vec())).ok()?;
                let json = an1mbyt3_geometry::vrm::gltf_json(&asset.source);
                an1mbyt3_geometry::retarget::humanoid_clips(&data, json.as_ref()).ok()
            })
            .unwrap_or_default();
        let rc = std::rc::Rc::new(clips);
        self.humanoid_clips.insert(source, std::rc::Rc::clone(&rc));
        rc
    }

    /// The clip an animator calls `name`.
    /// (Shared, not copied: the cached clips and the index of this one.)
    fn humanoid_clip(&mut self, animator: &an1mbyt3_core::HumanoidAnimator, name: &str) -> Option<(std::rc::Rc<Vec<an1mbyt3_geometry::retarget::HumanoidClip>>, usize)> {
        let r = animator.clips.iter().find(|c| c.name == name)?;
        let clips = self.humanoid_clips_of(r.source?);
        let i = clips.iter().position(|c| c.name == r.clip).or(if clips.is_empty() { None } else { Some(0) })?;
        Some((clips, i))
    }

    /// Import → "Avatar animation": an animation file (VRMA, or a glTF /
    /// GLB / VRM with a humanoid skeleton) as an asset; its clips are added
    /// to `node`'s Humanoid Animator (made if missing) when `node` is a VRM
    /// avatar. Returns `{ document, clips: [names] }`; errors if the file
    /// has no humanoid animation.
    pub fn import_humanoid_animation(&mut self, name: String, source: Vec<u8>, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let data = an1mbyt3_geometry::parse_gltf(&source, &|_| None).map_err(|e| JsValue::from_str(&format!("not a readable animation file: {e}")))?;
        let json = an1mbyt3_geometry::vrm::gltf_json(&source);
        let clips = an1mbyt3_geometry::retarget::humanoid_clips(&data, json.as_ref()).map_err(|e| JsValue::from_str(&e))?;
        let model = self.document.add_model(an1mbyt3_core::ModelAsset { name: name.clone(), source, files: Vec::new(), deleted: false });
        let stem = name.rsplit(['/', '\\']).next().unwrap_or(&name).rsplit_once('.').map_or(name.as_str(), |(s, _)| s).to_string();
        let mut names = Vec::new();
        let artboard_id: Option<ArtboardId> = parse_id(artboard).ok().flatten();
        let node_id: Option<NodeId> = parse_id(node).ok().flatten();
        if let (Some(artboard_id), Some(node_id)) = (artboard_id, node_id) {
            if let Some(n) = self.document.artboard(artboard_id).and_then(|a| a.node(node_id)) {
                let mut behaviors = n.behaviors.clone();
                let at = behaviors.iter().position(|e| matches!(e.behavior, an1mbyt3_core::NodeBehavior::HumanoidAnimator(_)));
                let at = at.unwrap_or_else(|| {
                    behaviors.push(an1mbyt3_core::NodeBehaviorEntry { behavior: an1mbyt3_core::NodeBehavior::HumanoidAnimator(an1mbyt3_core::HumanoidAnimator::default()), enabled: true });
                    behaviors.len() - 1
                });
                if let an1mbyt3_core::NodeBehavior::HumanoidAnimator(a) = &mut behaviors[at].behavior {
                    for c in &clips {
                        let base = if clips.len() == 1 || c.name.starts_with("Clip ") { stem.clone() } else { c.name.clone() };
                        let mut label = base.clone();
                        let mut k = 2;
                        while a.clips.iter().any(|x| x.name == label) {
                            label = format!("{base} {k}");
                            k += 1;
                        }
                        a.clips.push(an1mbyt3_core::HumanoidClipRef { name: label.clone(), source: Some(model), clip: c.name.clone() });
                        names.push(label);
                    }
                    if a.playing.is_empty() {
                        a.playing = names.first().cloned().unwrap_or_default();
                    }
                }
                self.document.apply(Command::SetNodeBehaviors { artboard: artboard_id, node: node_id, value: behaviors });
            }
        }
        if names.is_empty() {
            names = clips.iter().map(|c| c.name.clone()).collect();
        }
        #[derive(serde::Serialize)]
        struct Imported {
            document: crate::dto::DocumentSummary,
            clips: Vec<String>,
        }
        to_js_value(&Imported { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), clips: names })
    }

    /// Script API (`ref.playAnim3(name, { loop, speed, blend })`): switch
    /// the avatar's Humanoid Animator clip (crossfading). `loop` −1 / speed
    /// / blend < 0 = the component's settings. `""` = back to its own.
    #[allow(clippy::too_many_arguments)]
    pub fn model_play_anim(&mut self, artboard: JsValue, node: JsValue, name: String, looping: i32, speed: f32, blend: f32) -> Result<bool, JsValue> {
        let Some((node_id, _)) = self.model_of(artboard, node)? else { return Ok(false) };
        let inst = self.model_instances.entry(node_id).or_default();
        inst.anim_override = (!name.is_empty()).then(|| (name, (looping >= 0).then_some(looping > 0), (speed >= 0.0).then_some(speed), (blend >= 0.0).then_some(blend)));
        Ok(true)
    }

    /// Script API (`ref.anim3`): `{ clip, time }` of what's playing.
    pub fn model_anim_state(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let Some((node_id, _)) = self.model_of(artboard, node)? else { return Ok(JsValue::NULL) };
        #[derive(serde::Serialize)]
        struct State {
            clip: String,
            time: f32,
        }
        match self.model_instances.get(&node_id).and_then(|i| i.anim.clone()) {
            Some((clip, time)) => to_js_value(&State { clip, time }),
            None => Ok(JsValue::NULL),
        }
    }

    /// Bone Attachments (see `an1mbyt3_core::BoneAttachment`) in `artboard`:
    /// attached node → the correction that moves it (and its subtree) from
    /// where its transform alone puts it onto its bone. Uses the model's
    /// last drawn pose (the frame before), like `ref.bone3`.
    fn bone_attachments(&mut self, artboard_id: ArtboardId) -> HashMap<NodeId, glam::Mat4> {
        use glam::{Mat4, Quat};
        let mut out = HashMap::new();
        let Some(artboard) = self.document.artboard(artboard_id) else { return out };
        // (attached node, model node, model, bone, own world matrix)
        let mut wanted = Vec::new();
        for id in artboard.node_ids() {
            let Some(node) = artboard.node(id) else { continue };
            let Some(a) = node.bone_attachment() else { continue };
            let Some(parent) = node.parent else { continue };
            let Some(NodeKind::Node3D(an1mbyt3_core::Node3D { content: an1mbyt3_core::Node3DContent::Mesh(m), .. })) = artboard.node(parent).map(|n| &n.kind) else { continue };
            let an1mbyt3_core::Primitive3D::Model { model } = m.primitive else { continue };
            let (Some(own), Some(parent_world)) = (artboard.world_matrix_3d(id), artboard.world_matrix_3d(parent)) else { continue };
            let local = parent_world.inverse() * own;
            wanted.push((id, parent, model, a.bone.clone(), parent_world, local, own));
        }
        for (id, parent, model_id, bone, parent_world, local, own) in wanted {
            let Some(model) = self.gpu_model(model_id) else { continue };
            let Some(index) = Self::model_bone(&model, &bone) else { continue };
            let Some(inst) = self.model_instances.get(&parent) else { continue };
            let Some(pose) = inst.last_pose.get(index) else { continue };
            let (_, rotation, translation) = pose.to_scale_rotation_translation();
            // Humanoid bones: the bone's turn from its rest, in the
            // normalized frame (facing +Z) — so the offset is the same on
            // any avatar.
            let frame = match model.humanoid.as_ref().and_then(|h| h.bones.get(&bone).map(|b| (b.2, h.faces_neg_z))) {
                Some((rest, neg_z)) => {
                    let turn = if neg_z { Quat::from_rotation_y(std::f32::consts::PI) } else { Quat::IDENTITY };
                    rotation * rest.inverse() * turn
                }
                None => rotation,
            };
            let on_bone = parent_world * Mat4::from_rotation_translation(frame, translation) * local;
            out.insert(id, on_bone * own.inverse());
        }
        out
    }

    /// Foot IK (see `an1mbyt3_core::FootIK`): where each foot should stand —
    /// the ground under it (a physics raycast in Playground, else the
    /// avatar's own floor, its origin's height) — then drop the hips for a
    /// lower step, solve each leg and tilt each foot to the slope.
    fn plant_feet(&self, model: &GpuModel, vrm: &an1mbyt3_geometry::vrm::Vrm, cfg: &an1mbyt3_core::FootIK, world: glam::Mat4, local: &mut [(glam::Vec3, glam::Quat, glam::Vec3)]) {
        use an1mbyt3_geometry::ik3d::{rotate_bone, solve_two_bone, TwoBoneChain};
        use glam::Vec3;
        let data = &model.data;
        let legs: Vec<TwoBoneChain> = [("leftUpperLeg", "leftLowerLeg", "leftFoot"), ("rightUpperLeg", "rightLowerLeg", "rightFoot")]
            .iter()
            .filter_map(|(u, l, e)| Some(TwoBoneChain { upper: vrm.bone(u)?, lower: vrm.bone(l)?, end: vrm.bone(e)? }))
            .collect();
        if legs.is_empty() || cfg.weight <= 0.0 {
            return;
        }
        let pos = |m: &glam::Mat4| m.transform_point3(Vec3::ZERO);
        let rest = data.world_pose(&model.rest);
        let (scale, _, base) = world.to_scale_rotation_translation();
        let inv = world.inverse();
        let physics = self.physics3d.as_ref().filter(|_| self.playground_active).map(|(_, w)| w);
        let pose = data.world_pose(local);
        // Per leg: (foot world position now, where it should be (world y),
        // ground normal).
        let mut plans = Vec::new();
        let mut drop = 0.0f32;
        for c in &legs {
            let foot = world.transform_point3(pos(&pose[c.end]));
            let ankle = pos(&rest[c.end]).y * scale.y.abs() + cfg.sole;
            let hit = physics.and_then(|p| p.raycast(foot + Vec3::Y * 0.6, Vec3::NEG_Y, 0.6 + cfg.max_step + 1.0, None));
            let (ground, normal) = hit.map_or((base.y, Vec3::Y), |h| (h.point.y, h.normal));
            // Stood on (not jumping / falling): follow the ground's height.
            let adapt = if (ground - base.y).abs() <= cfg.max_step { ground - base.y } else { 0.0 };
            let want = (foot.y + adapt).max(ground + ankle);
            drop = drop.min(want - foot.y);
            plans.push((foot, want, normal));
        }
        // A lower foothold: the hips come down to reach it.
        let drop = drop.max(-cfg.max_step) * cfg.weight;
        if drop < -1e-4 {
            if let Some(h) = vrm.bone("hips") {
                let parent = data.nodes[h].parent.map_or(glam::Quat::IDENTITY, |p| pose[p].to_scale_rotation_translation().1);
                local[h].0 += parent.inverse() * inv.transform_vector3(Vec3::Y * drop);
            }
        }
        let forward = Vec3::new(0.0, 0.0, if vrm.forward_z < 0.0 { -1.0 } else { 1.0 });
        for (c, (foot, want, normal)) in legs.iter().zip(plans) {
            let now = data.world_pose(local);
            let current = pos(&now[c.end]);
            let target_world = Vec3::new(foot.x, want, foot.z);
            let target = current.lerp(inv.transform_point3(target_world), cfg.weight);
            if current.distance(target) > 1e-4 {
                let knee = pos(&now[c.lower]);
                solve_two_bone(data, local, *c, target, knee + forward * 0.4);
            }
            if cfg.align && normal.y < 0.999 {
                let now = data.world_pose(local);
                let up = inv.transform_vector3(Vec3::Y).normalize_or(Vec3::Y);
                let n = inv.transform_vector3(normal).normalize_or(up);
                let tilt = glam::Quat::IDENTITY.slerp(glam::Quat::from_rotation_arc(up, n), cfg.weight);
                rotate_bone(data, &now, local, c.end, tilt);
            }
        }
    }

    /// The node's enabled Ragdoll 3D settings, if any.
    fn ragdoll_config(&self, node: NodeId) -> Option<an1mbyt3_core::Ragdoll3D> {
        self.document.artboards().find_map(|(_, a)| a.node(node).and_then(|n| n.ragdoll3d().copied()))
    }

    /// Script API (`ref.ragdoll3(on, { velocity })`): the avatar goes limp
    /// (its bones become physics, starting with `velocity` m/s), or gets
    /// back up (animation again). Playground only.
    #[allow(clippy::too_many_arguments)]
    pub fn model_ragdoll(&mut self, artboard: JsValue, node: JsValue, on: bool, vx: f32, vy: f32, vz: f32) -> Result<bool, JsValue> {
        let Some((node_id, model)) = self.model_of(artboard, node)? else { return Ok(false) };
        if model.data.vrm.is_none() {
            return Ok(false);
        }
        self.model_instances.entry(node_id).or_default().ragdoll_request = Some((on, glam::Vec3::new(vx, vy, vz)));
        Ok(true)
    }

    /// Script API (`ref.ragdoll3` getter): whether the avatar is limp.
    pub fn model_is_ragdoll(&mut self, artboard: JsValue, node: JsValue) -> Result<bool, JsValue> {
        let Some((node_id, _)) = self.model_of(artboard, node)? else { return Ok(false) };
        Ok(self.model_instances.get(&node_id).is_some_and(|i| i.ragdoll_bones.is_some()))
    }

    /// Script API (`ref.ragdollHit3(bone, impulse)`): knocks one limb of a
    /// limp avatar (`"head"`, `"chest"`, `"leftLowerLeg"` …; a bone without
    /// its own limb hits the nearest one above it). `impulse` in N·s.
    #[allow(clippy::too_many_arguments)]
    pub fn model_ragdoll_hit(&mut self, artboard: JsValue, node: JsValue, bone: String, ix: f32, iy: f32, iz: f32) -> Result<bool, JsValue> {
        let Some((node_id, model)) = self.model_of(artboard, node)? else { return Ok(false) };
        let Some(bones) = self.model_instances.get(&node_id).and_then(|i| i.ragdoll_bones.clone()) else { return Ok(false) };
        // The limb for this bone, or its nearest ancestor's.
        let mut index = Self::model_bone(&model, &bone);
        let mut part = None;
        while let Some(i) = index {
            if let Some(k) = bones.iter().position(|(b, _)| *b == i) {
                part = Some(k);
                break;
            }
            index = model.parents.get(i).copied().flatten();
        }
        let part = part.unwrap_or(0);
        if let Some((_, w)) = self.physics3d.as_mut() {
            w.ragdoll_impulse(node_id, part, glam::Vec3::new(ix, iy, iz), None);
        }
        Ok(true)
    }

    /// File → Import → 3D Model: adds a glTF / GLB model (plus the files it
    /// references: `names[i]` / `files[i]`, e.g. its textures) and places
    /// it as a new Node3D at the origin. Undoable (the node). Returns
    /// `{ document, newNode }`, or an error if the model can't be read.
    /// (`names[i]`'s bytes are `blob[offset..offset + lengths[i]]`, packed
    /// back to back.)
    pub fn import_model(&mut self, artboard: JsValue, name: String, source: Vec<u8>, names: Vec<String>, blob: Vec<u8>, lengths: Vec<u32>) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let mut offset = 0usize;
        let mut files = Vec::with_capacity(names.len());
        for (path, len) in names.into_iter().zip(lengths) {
            let end = (offset + len as usize).min(blob.len());
            files.push(an1mbyt3_core::ModelFile { path, data: blob[offset..end].to_vec() });
            offset = end;
        }
        let asset = an1mbyt3_core::ModelAsset { name: name.clone(), source, files, deleted: false };
        let parsed = an1mbyt3_geometry::parse_gltf(&asset.source, &|uri| asset.file(uri).map(|b| b.to_vec())).map_err(|e| JsValue::from_str(&format!("import_model: {e}")))?;
        let model = self.document.add_model(asset);
        // VRM: every MToon material becomes an editable material asset.
        let model_name = name.rsplit(['/', '\\']).next().unwrap_or(&name).trim_end_matches(".vrm").trim_end_matches(".glb").to_string();
        self.extract_model_materials(model, &parsed, &model_name);
        // Stand it on the ground, about human-sized if it's huge or tiny.
        let (min, max) = parsed.bounds;
        let height = (max.y - min.y).max(1e-3);
        let scale = if !(0.2..=20.0).contains(&height) { 1.8 / height } else { 1.0 };
        let mesh = an1mbyt3_core::Mesh3D { primitive: an1mbyt3_core::Primitive3D::Model { model }, color: [1.0; 4], shininess: 0.15, ..an1mbyt3_core::Mesh3D::new(an1mbyt3_core::Primitive3D::cube(), [1.0; 4]) };
        let transform = an1mbyt3_core::Transform3D { position: glam::Vec3::new(0.0, -min.y * scale, 0.0), rotation_degrees: glam::Vec3::ZERO, scale: glam::Vec3::splat(scale) };
        let display = name.rsplit(['/', '\\']).next().unwrap_or(&name).trim_end_matches(".glb").trim_end_matches(".gltf").to_string();
        let mut node = Node::new(display, None, NodeKind::Node3D(an1mbyt3_core::Node3D::new(transform, an1mbyt3_core::Node3DContent::Mesh(mesh))));
        // VRM avatars start with Foot IK: feet planted, knees bend.
        if parsed.vrm.is_some() {
            node.behaviors.push(an1mbyt3_core::NodeBehaviorEntry { behavior: an1mbyt3_core::NodeBehavior::FootIK(an1mbyt3_core::FootIK::default()), enabled: true });
        }
        let inverse = self.document.apply(Command::AddNode { artboard: artboard_id, node });
        let Command::SetDeleted { node: new_node, .. } = inverse else { unreachable!("Command::AddNode's inverse is always a SetDeleted") };
        to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node })
    }

    /// An imported model's animation clip names (the Inspector's dropdown).
    pub fn model_clips(&self, model: JsValue) -> Result<JsValue, JsValue> {
        let id: an1mbyt3_core::ModelId = parse_id(model)?;
        let names: Vec<String> = self
            .model_gpu
            .get(&id)
            .map(|m| m.data.animations.iter().map(|c| c.name.clone()).collect())
            .or_else(|| {
                let asset = self.document.model(id)?;
                let data = an1mbyt3_geometry::parse_gltf(&asset.source, &|uri| asset.file(uri).map(|b| b.to_vec())).ok()?;
                Some(data.animations.iter().map(|c| c.name.clone()).collect())
            })
            .unwrap_or_default();
        to_js_value(&names)
    }

    /// Untracked script write (`ref.playAnimation3(...)`): a model mesh's
    /// base clip (`""` = rest pose), whether it loops, and the overlay clip.
    pub fn preview_node3d_animation(&mut self, artboard: JsValue, node: JsValue, clip: &str, looping: bool, overlay: &str) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        if let Some(NodeKind::Node3D(an1mbyt3_core::Node3D { content: an1mbyt3_core::Node3DContent::Mesh(mesh), .. })) = self.document.artboard_mut(artboard_id).and_then(|a| a.node_mut(node_id)).map(|n| &mut n.kind) {
            mesh.animation = (!clip.is_empty()).then(|| an1mbyt3_core::AnimationName::new(clip));
            mesh.animation_loop = looping;
            mesh.animation_overlay = (!overlay.is_empty()).then(|| an1mbyt3_core::AnimationName::new(overlay));
        }
        Ok(())
    }

    /// An uploaded 3D mesh for `primitive` (shared by every node using the
    /// exact same parameters).
    fn mesh3d(&mut self, primitive: &an1mbyt3_core::Primitive3D) -> std::rc::Rc<an1mbyt3_renderer::GpuMesh3D> {
        let key = mesh3d_key(primitive);
        if let Some(gpu) = self.mesh3d_cache.get(&key) {
            return std::rc::Rc::clone(gpu);
        }
        let gpu = std::rc::Rc::new(self.renderer.upload_mesh_3d(&an1mbyt3_geometry::primitive_mesh(primitive)));
        self.mesh3d_cache.insert(key, std::rc::Rc::clone(&gpu));
        gpu
    }

    /// The scene view's floor grid: 1 m lines over 60 m, every 5th brighter.
    fn grid_mesh(&mut self) -> std::rc::Rc<an1mbyt3_renderer::GpuMesh3D> {
        if let Some(gpu) = self.mesh3d_cache.get(&GRID_MESH_KEY) {
            return std::rc::Rc::clone(gpu);
        }
        let mut m = an1mbyt3_geometry::Mesh3DData::default();
        let half = 30i32;
        for i in -half..=half {
            let w = if i % 5 == 0 { 0.035 } else { 0.015 };
            let f = i as f32;
            let e = half as f32;
            for quad in [
                [[-e, 0.0, f - w], [-e, 0.0, f + w], [e, 0.0, f + w], [e, 0.0, f - w]],
                [[f - w, 0.0, -e], [f - w, 0.0, e], [f + w, 0.0, e], [f + w, 0.0, -e]],
            ] {
                let base = m.positions.len() as u32;
                for v in quad {
                    m.positions.push(v);
                    m.normals.push([0.0, 1.0, 0.0]);
                    m.uvs.push([0.0, 0.0]);
                }
                m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
        let gpu = std::rc::Rc::new(self.renderer.upload_mesh_3d(&m));
        self.mesh3d_cache.insert(GRID_MESH_KEY, std::rc::Rc::clone(&gpu));
        gpu
    }

    /// A shader-graph material compiled for the 3D pass (cached by WGSL);
    /// `None` if it isn't a graph or failed to compile.
    fn material3d(&mut self, id: an1mbyt3_core::MaterialId) -> Option<std::rc::Rc<an1mbyt3_renderer::GpuMaterial3D>> {
        let stamp = (self.document.instance(), self.document.revision());
        if self.material3d_checked_at.get(&id) == Some(&stamp) {
            if let Some((_, compiled)) = self.material_gpu_3d.get(&id) {
                return compiled.as_ref().ok().cloned();
            }
        }
        self.material3d_checked_at.insert(id, stamp);
        // A missing / non-graph / uncompilable material drops its cache entry,
        // so the revision-stamp fast path above can't serve a stale shader.
        let wgsl = match self.document.material(id).filter(|m| !m.deleted).map(|m| &m.kind) {
            Some(an1mbyt3_core::MaterialKind::ShaderGraph(graph)) => an1mbyt3_core::compile_wgsl(graph).ok(),
            _ => None,
        };
        let Some(wgsl) = wgsl else {
            self.material_gpu_3d.remove(&id);
            return None;
        };
        if let Some((cached, compiled)) = self.material_gpu_3d.get(&id) {
            if *cached == wgsl {
                return compiled.as_ref().ok().cloned();
            }
        }
        let compiled = self.renderer.compile_material_3d(&wgsl).map(std::rc::Rc::new);
        let out = compiled.as_ref().ok().cloned();
        self.material_gpu_3d.insert(id, (wgsl, compiled));
        out
    }

    /// Positions UI nodes for this frame (untracked, every mode — layout is
    /// derived, so Design shows exactly what Playground will):
    /// - **UI Anchor**: the node goes to its point of the view rectangle (the
    ///   game camera's visible area in Playground, the canvas size over the
    ///   camera's zoom otherwise), centred on the parent's origin — i.e. the
    ///   camera, for HUD nodes.
    /// - **UI Stack**: its visible children are laid out in a row / column
    ///   by their bounds (`an1mbyt3_core::stack_positions`).
    /// This frame's screen facts for `artboard_id`: the visible world area
    /// (what anchors / root layouts fill), the screen in CSS px, and the
    /// compact / portrait classification. Playground uses the real canvas;
    /// Design uses the "preview on" size (or the artboard's own).
    fn screen_info(&self, artboard_id: ArtboardId) -> Option<an1mbyt3_core::ScreenInfo> {
        let artboard = self.document.artboard(artboard_id)?;
        let camera_zoom = artboard.active_camera().and_then(|c| artboard.node(c)).and_then(|n| n.camera()).map_or(1.0, |c| c.zoom.max(0.01));
        let reference = Vec2::new(artboard.width.max(1.0), artboard.height.max(1.0)) / camera_zoom;
        let settings = artboard.screen;
        let live = self.active_camera_frame(artboard_id);
        let screen = match live {
            Some(_) => Vec2::new(self.canvas_width as f32, self.canvas_height as f32) / self.last_device_pixel_ratio.max(0.01),
            None => self.preview_screen.unwrap_or(Vec2::new(artboard.width, artboard.height)),
        };
        let view = match (live, settings.mode) {
            // Playground: exactly what the camera shows.
            (Some(f), _) => Vec2::new(self.canvas_width as f32, self.canvas_height as f32) / f.zoom.max(1e-3),
            (None, an1mbyt3_core::ScreenMode::Responsive) => an1mbyt3_core::responsive_view(reference, screen, &settings),
            (None, an1mbyt3_core::ScreenMode::Fixed) => reference,
        };
        Some(an1mbyt3_core::ScreenInfo::new(view, screen, &settings))
    }

    /// Every enabled Flipbook component shows exactly one of its node's
    /// children (in Layers order): in Playground the frame its clock has
    /// reached, otherwise its `frame` setting — so Design shows a stable
    /// frame you can pick in the Inspector. Untracked visibility writes,
    /// like UI Show When's.
    fn apply_flipbooks(&mut self, artboard_id: ArtboardId) {
        let playing = self.playground_active;
        let delta = self.frame_delta;
        let Some(artboard) = self.document.artboard_mut(artboard_id) else { return };
        let books: Vec<(NodeId, an1mbyt3_core::Flipbook)> = artboard
            .node_ids()
            .filter_map(|id| {
                let node = artboard.node(id).filter(|n| !n.deleted)?;
                node.behaviors.iter().find_map(|e| match &e.behavior {
                    an1mbyt3_core::NodeBehavior::Flipbook(f) if e.enabled => Some((id, *f)),
                    _ => None,
                })
            })
            .collect();
        for (id, book) in books {
            let frames: Vec<NodeId> = artboard.children_of(id).filter(|(_, n)| !n.deleted).map(|(c, _)| c).collect();
            // On a particle system the frames are each particle's art
            // (`particle_draws`), not something drawn at the emitter.
            if artboard.node(id).is_some_and(|n| n.particle_system().is_some()) {
                for child in frames {
                    if let Some(node) = artboard.node_mut(child) {
                        node.visible = false;
                    }
                }
                continue;
            }
            let shown = if playing {
                let clock = self.flipbook_clocks.entry(id).or_insert(0.0);
                let shown = book.frame_at(*clock, frames.len());
                *clock += delta;
                shown
            } else {
                an1mbyt3_core::Flipbook { playing: false, ..book }.frame_at(0.0, frames.len())
            };
            for (index, child) in frames.into_iter().enumerate() {
                if let Some(node) = artboard.node_mut(child) {
                    node.visible = index == shown;
                }
            }
        }
    }

    fn apply_ui_layout(&mut self, artboard_id: ArtboardId) {
        use an1mbyt3_core::NodeBehavior as B;
        let Some(screen) = self.screen_info(artboard_id) else { return };
        self.last_screen = Some(screen);
        let view = screen.view;

        // Show When: decided first, so hidden nodes take no layout space.
        if let Some(artboard) = self.document.artboard_mut(artboard_id) {
            let rules: Vec<(NodeId, bool)> = artboard
                .node_ids()
                .filter_map(|id| {
                    let node = artboard.node(id)?;
                    node.behaviors.iter().find_map(|e| match &e.behavior {
                        B::UiShowWhen(rule) if e.enabled => Some((id, rule.passes(&screen))),
                        _ => None,
                    })
                })
                .collect();
            for (id, show) in rules {
                if let Some(node) = artboard.node_mut(id) {
                    node.visible = show;
                }
            }
        }

        let Some(artboard) = self.document.artboard(artboard_id) else { return };
        let mut moves: Vec<(NodeId, Vec2)> = Vec::new();
        for id in artboard.node_ids() {
            if artboard.is_effectively_deleted(id) {
                continue;
            }
            let Some(node) = artboard.node(id) else { continue };
            for entry in node.behaviors.iter().filter(|e| e.enabled) {
                match &entry.behavior {
                    B::UiAnchor(anchor) => moves.push((id, (anchor.anchor - Vec2::splat(0.5)) * view + anchor.offset)),
                    B::UiStack(stack) if !node.behaviors.iter().any(|e| e.enabled && matches!(e.behavior, B::UiLayout(_))) => {
                        let children: Vec<NodeId> = artboard.children_of(id).filter(|(_, c)| c.visible && !c.deleted).map(|(cid, _)| cid).collect();
                        let boxes: Vec<(Vec2, Vec2)> = children
                            .iter()
                            .map(|cid| {
                                let (min, max) = crate::dto::local_bounds(artboard, *cid);
                                let scale = artboard.node(*cid).map_or(Vec2::ONE, |c| c.transform.scale.abs());
                                (min * scale, max * scale)
                            })
                            .collect();
                        let sizes: Vec<Vec2> = boxes.iter().map(|(min, max)| *max - *min).collect();
                        for ((cid, pos), (min, max)) in children.iter().zip(an1mbyt3_core::stack_positions(stack, &sizes)).zip(boxes) {
                            // The child's bounds needn't be centred on its origin.
                            moves.push((*cid, pos - (min + max) * 0.5));
                        }
                    }
                    _ => {}
                }
            }
        }
        // Flex layouts (after anchors, so an anchored root is placed first).
        // Text measured by its shaped glyphs (once it's been drawn), not the
        // rough per-character estimate.
        let text_cache = &self.text_mesh_cache;
        let layout_writes = an1mbyt3_core::solve_layouts(artboard, &screen, &|id| match (artboard.node(id).map(|n| &n.kind), text_cache.get(&id)) {
            (Some(NodeKind::Text(text)), Some(cached)) if cached.key.0 == text.content && cached.key.1 == text.size_px => cached.bounds,
            _ => crate::dto::local_bounds(artboard, id),
        });
        let Some(artboard) = self.document.artboard_mut(artboard_id) else { return };
        for (id, at) in moves {
            if let Some(node) = artboard.node_mut(id) {
                if node.transform.translation.distance_squared(at) > 1e-8 {
                    node.transform.translation = at;
                }
            }
        }
        for write in layout_writes {
            let Some(node) = artboard.node_mut(write.node) else { continue };
            if let Some(at) = write.translation {
                if node.transform.translation.distance_squared(at) > 1e-8 {
                    node.transform.translation = at;
                }
            }
            if let (Some(size), NodeKind::Shape(shape)) = (write.rect_size, &mut node.kind) {
                // A rectangle takes its slot's size (re-centred on its origin).
                shape.path = an1mbyt3_core::Path::rect(size.x.max(0.0), size.y.max(0.0));
            }
        }
    }

    /// Builds this frame's [`an1mbyt3_renderer::Scene3D`] from the scene's
    /// Node3Ds — meshes, lights and the camera — or clears it when there's
    /// nothing 3D to show. The camera is the scene view's orbit camera in
    /// Design mode with the 3D view on (plus grid, camera / light markers,
    /// the selection box and the move gizmo), else the current Camera 3D.
    fn collect_scene_3d(&mut self, artboard_id: ArtboardId) {
        use an1mbyt3_core::{Light3DKind, Node3DContent, Primitive3D};
        use an1mbyt3_renderer::MeshDraw3D;
        use glam::{Mat4, Vec3};
        let Some(artboard) = self.document.artboard(artboard_id) else {
            self.renderer.set_scene_3d(None);
            return;
        };
        // The scene's screen shape (camera frustum gizmos draw at it).
        let screen_aspect = artboard.width.max(1.0) / artboard.height.max(1.0);
        let mut camera: Option<(Mat4, an1mbyt3_core::Camera3D)> = None;
        let mut meshes: Vec<(NodeId, Mat4, an1mbyt3_core::Mesh3D)> = Vec::new();
        let mut markers: Vec<(Mat4, Node3DContent)> = Vec::new();
        let mut sun: Option<(Vec3, Vec3)> = None;
        let mut points = Vec::new();
        let mut emitters: Vec<(NodeId, Mat4, an1mbyt3_core::ParticleSystem3D)> = Vec::new();
        let mut vrm_poses: HashMap<NodeId, an1mbyt3_core::VrmPoses> = HashMap::new();
        let mut slot_overrides: HashMap<NodeId, an1mbyt3_core::MaterialSlots> = HashMap::new();
        let mut animators: HashMap<NodeId, an1mbyt3_core::HumanoidAnimator> = HashMap::new();
        let mut foot_iks: HashMap<NodeId, an1mbyt3_core::FootIK> = HashMap::new();
        let mut any_3d = false;
        // Bone Attachments: each attached node's correction (its world
        // matrix as it rides the bone, times its plain world matrix's
        // inverse) — applied to it and everything under it.
        let attachments = self.bone_attachments(artboard_id);
        let artboard = self.document.artboard(artboard_id).expect("checked above");
        let attached = |id: NodeId, world: Mat4| -> Mat4 {
            if attachments.is_empty() {
                return world;
            }
            let mut at = Some(id);
            while let Some(a) = at {
                if let Some(fix) = attachments.get(&a) {
                    return *fix * world;
                }
                at = artboard.node(a).and_then(|n| n.parent);
            }
            world
        };
        for id in artboard.node_ids() {
            if artboard.is_effectively_deleted(id) {
                continue;
            }
            let Some(node) = artboard.node(id) else { continue };
            let NodeKind::Node3D(n) = &node.kind else { continue };
            any_3d = true;
            if !crate::view3d::visible_chain(artboard, id) {
                continue;
            }
            let Some(world) = artboard.world_matrix_3d(id) else { continue };
            let world = attached(id, world);
            for entry in &node.behaviors {
                if let (true, an1mbyt3_core::NodeBehavior::ParticleSystem3D(p)) = (entry.enabled, &entry.behavior) {
                    emitters.push((id, world, p.clone()));
                }
            }
            if let Some(poses) = node.vrm_poses() {
                vrm_poses.insert(id, poses.clone());
            }
            if let Some(slots) = node.material_slots() {
                slot_overrides.insert(id, slots.clone());
            }
            if let Some(a) = node.humanoid_animator() {
                animators.insert(id, a.clone());
            }
            if let Some(f) = node.foot_ik() {
                foot_iks.insert(id, *f);
            }
            match n.content {
                Node3DContent::Mesh(mesh) => meshes.push((id, world, mesh)),
                Node3DContent::Camera(cam) => {
                    // The first `current` camera wins; else the first camera.
                    if camera.as_ref().is_none_or(|(_, c)| cam.current && !c.current) {
                        camera = Some((world, cam));
                    }
                    markers.push((world, n.content));
                }
                Node3DContent::Light(light) => {
                    let color = Vec3::from(light.color) * light.intensity;
                    match light.kind {
                        Light3DKind::Directional => {
                            if sun.is_none() {
                                sun = Some((world.transform_vector3(Vec3::NEG_Z), color));
                            }
                        }
                        Light3DKind::Point => points.push(an1mbyt3_renderer::PointLight3D { position: world.transform_point3(Vec3::ZERO), color, range: light.range }),
                    }
                    markers.push((world, n.content));
                }
                Node3DContent::Empty => markers.push((world, n.content)),
            }
        }
        let editor_view = self.view3d_enabled && !self.playground_active && any_3d;
        let selected = self.view3d_selected.and_then(|id| Some((artboard.world_matrix_3d(id)?, match &artboard.node(id)?.kind {
            NodeKind::Node3D(n) => n.content,
            _ => return None,
        })));
        // A selected particle emitter's box shows its effect's extent (its
        // node is an empty pivot; the particles are what you see).
        let selected_extent = self.view3d_selected.and_then(|id| {
            artboard.node(id)?.behaviors.iter().find_map(|e| match &e.behavior {
                an1mbyt3_core::NodeBehavior::ParticleSystem3D(p) => {
                    let spread = if p.shape == an1mbyt3_core::Emitter3DShape::Point { 0.0 } else { p.radius + p.thickness };
                    // A fluid blob shows about a quarter of its splat size.
                    let blob = p.size_start.max(p.size_end) * if p.fluid { 0.25 } else { 0.5 };
                    Some(Vec3::splat((spread + blob).max(0.15)))
                }
                _ => None,
            })
        });
        let model_bounds = self.model_bounds_map();
        if camera.is_none() && !editor_view {
            self.renderer.set_scene_3d(None);
            return;
        }

        let mut draws = Vec::with_capacity(meshes.len() + 32);
        let delta = self.frame_delta;
        let live: std::collections::HashSet<NodeId> = meshes.iter().map(|(id, _, _)| *id).collect();
        self.anim_clocks.retain(|id, _| live.contains(id));
        // (Hidden panes keep their state: a pooled pane can be hidden and
        // reshown; only deleted ones are forgotten.)
        self.glass.retain(|id, _| artboard.node(*id).is_some());
        let aspect = (self.canvas_width.max(1) as f32) / (self.canvas_height.max(1) as f32);
        let (view, projection, eye, clear) = if editor_view {
            let cam = self.view3d_camera;
            let clear = camera.and_then(|(_, c)| c.clear_color).unwrap_or([0.36, 0.42, 0.5, 1.0]);
            (cam.view(), cam.projection(aspect), cam.eye(), Some(clear))
        } else {
            let (world, cam) = camera.expect("checked above");
            let near = cam.near.max(0.001);
            let projection = glam::camera::rh::proj::directx::perspective(cam.fov_degrees.clamp(1.0, 170.0).to_radians(), aspect, near, cam.far.max(near + 0.01));
            (world.inverse(), projection, world.transform_point3(Vec3::ZERO), cam.clear_color)
        };
        let frustum = frustum_planes(projection * view);

        // 3D particles: simulate, then camera-facing quads (fluid emitters
        // become their own liquid layer).
        let tp = crate::profiler::now_ms();
        let mut particles = Vec::new();
        let mut fluids: Vec<an1mbyt3_renderer::FluidLayer3D> = Vec::new();
        let mut fluid_groups: Vec<u32> = Vec::new();
        {
            self.particles3d.sync_playing(self.playground_active);
            let live: std::collections::HashSet<NodeId> = emitters.iter().map(|(id, _, _)| *id).collect();
            self.particles3d.retain(&live);
            let camera_world = view.inverse();
            let (right, up) = (camera_world.x_axis.truncate().normalize_or(Vec3::X), camera_world.y_axis.truncate().normalize_or(Vec3::Y));
            for (id, world, cfg) in &emitters {
                self.particles3d.step(*id, cfg, *world, delta);
                if cfg.fluid {
                    // Density = each particle's alpha over life. Emitters in
                    // the same fluid group splat into one layer, so their
                    // blobs merge (the first one's look wins).
                    let splat = an1mbyt3_core::ParticleSystem3D { blend: an1mbyt3_core::ParticleBlend3D::Alpha, intensity: 1.0, ..cfg.clone() };
                    let layer = match fluid_groups.iter().position(|g| *g == cfg.fluid_group) {
                        Some(i) => i,
                        None => {
                            let c = cfg.color_start;
                            fluid_groups.push(cfg.fluid_group);
                            fluids.push(an1mbyt3_renderer::FluidLayer3D { quads: Vec::new(), body: [c[0], c[1], c[2], 1.0], edge: cfg.fluid_edge, threshold: cfg.fluid_threshold.max(0.01), gloss: cfg.fluid_gloss.max(0.0) });
                            fluids.len() - 1
                        }
                    };
                    self.particles3d.quads(*id, &splat, *world, eye, right, up, &mut fluids[layer].quads);
                } else {
                    self.particles3d.quads(*id, cfg, *world, eye, right, up, &mut particles);
                }
            }
        }
        self.sections.particles3d_ms = crate::profiler::now_ms() - tp;
        for (node_id, model, mesh) in meshes {
            let node_toon = mesh.material.and_then(|id| self.toon_material(id));
            let material = if node_toon.is_some() { None } else { mesh.material.and_then(|id| self.material3d(id)) };
            if let Primitive3D::Model { model: model_id } = mesh.primitive {
                self.model_draws(node_id, model_id, model, &mesh, material, delta, vrm_poses.get(&node_id), node_toon.as_ref(), slot_overrides.get(&node_id), animators.get(&node_id), foot_iks.get(&node_id), &mut draws);
                continue;
            }
            if let (Some(toon), Primitive3D::Cube { .. } | Primitive3D::Sphere { .. } | Primitive3D::Plane { .. } | Primitive3D::Cylinder { .. } | Primitive3D::Cone { .. } | Primitive3D::Torus { .. }) = (&node_toon, &mesh.primitive) {
                let gpu = self.mesh3d(&mesh.primitive);
                self.push_toon_draws(toon, gpu, model, mesh.color, None, None, false, false, &mut draws);
                continue;
            }
            if let Primitive3D::Voxels { width, height, depth } = mesh.primitive {
                self.voxel_draws(node_id, [width, height, depth], model, &mesh, eye, &frustum, &mut draws);
                continue;
            }
            if let Primitive3D::Glass { .. } = mesh.primitive {
                self.glass_draws(node_id, model, &mesh, material, &mut draws);
                continue;
            }
            // A colour with alpha below 100 % draws see-through (sorted,
            // not hiding what's behind it) — glass, ghosts, energy orbs.
            let blend = if mesh.color[3] < 0.999 { an1mbyt3_renderer::Blend3D::Transparent } else { an1mbyt3_renderer::Blend3D::Opaque };
            draws.push(MeshDraw3D { shininess: mesh.shininess, unlit: mesh.unlit, material, blend, ..MeshDraw3D::new(self.mesh3d(&mesh.primitive), model, mesh.color) });
        }

        // Ragdolls asked for while drawing: into the physics world.
        if !self.pending_ragdolls.is_empty() {
            if self.physics3d.as_ref().is_none_or(|(scene, _)| *scene != artboard_id) {
                self.physics3d = Some((artboard_id, an1mbyt3_physics::physics3d::Physics3DWorld::new()));
            }
            if let Some((_, w)) = self.physics3d.as_mut() {
                for (node, parts, velocity) in self.pending_ragdolls.drain(..) {
                    w.add_ragdoll(node, &parts, velocity);
                }
            }
        }
        if editor_view {
            let cam = self.view3d_camera;
            let unlit = |mesh, model, color: [f32; 4]| MeshDraw3D { unlit: true, ..MeshDraw3D::new(mesh, model, color) };
            // (Lifted 1 cm so it never z-fights a floor at y = 0.)
            draws.push(unlit(self.grid_mesh(), Mat4::from_translation(Vec3::new(0.0, 0.01, 0.0)), [0.8, 0.85, 0.95, 0.28]));
            // Markers for things with no geometry: cameras (a box + a cone
            // pointing where it looks), lights (a glowing ball), pivots.
            let cube = self.mesh3d(&Primitive3D::cube());
            let cone = self.mesh3d(&Primitive3D::cone());
            let ball = self.mesh3d(&Primitive3D::sphere());
            for (world, content) in &markers {
                let at = world.transform_point3(Vec3::ZERO);
                let (_, rotation, _) = world.to_scale_rotation_translation();
                let rotation = if rotation.is_finite() { rotation.normalize() } else { glam::Quat::IDENTITY };
                match content {
                    Node3DContent::Camera(cam) => {
                        // The camera Playground looks through is amber (the
                        // 2D camera frame's colour); others grey.
                        let active = camera.as_ref().is_some_and(|(w, c)| *w == *world && c.current == cam.current && c.fov_degrees == cam.fov_degrees);
                        let tone = if active { [1.0, 0.62, 0.15, 1.0] } else { [0.6, 0.62, 0.68, 1.0] };
                        let body = if active { [0.35, 0.22, 0.08, 1.0] } else { [0.15, 0.15, 0.18, 1.0] };
                        draws.push(unlit(std::rc::Rc::clone(&cube), Mat4::from_scale_rotation_translation(Vec3::new(0.35, 0.28, 0.45), rotation, at), body));
                        // Cone's tip is +Y; turn it to point down local -Z.
                        let lens = rotation * glam::Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
                        draws.push(unlit(std::rc::Rc::clone(&cone), Mat4::from_scale_rotation_translation(Vec3::new(0.3, 0.35, 0.3), lens, at + rotation * Vec3::new(0.0, 0.0, -0.38)), tone));
                        // Its view: a frustum out to a screen rectangle at
                        // the scene's aspect — the frame the 2D layer (HUD,
                        // the 2D camera's view) is drawn on top of in
                        // Playground — with an "up" marker above it.
                        let distance = 2.2;
                        let half_h = distance * (cam.fov_degrees.clamp(1.0, 170.0).to_radians() * 0.5).tan();
                        let half_w = half_h * screen_aspect;
                        let corner = |sx: f32, sy: f32| at + rotation * Vec3::new(sx * half_w, sy * half_h, -distance);
                        let corners = [corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)];
                        let thin = if active { 0.018 } else { 0.01 };
                        for k in 0..4 {
                            draws.push(unlit(std::rc::Rc::clone(&cube), crate::view3d::bar_between(at, corners[k], thin * 0.6), [tone[0], tone[1], tone[2], 0.7]));
                            draws.push(unlit(std::rc::Rc::clone(&cube), crate::view3d::bar_between(corners[k], corners[(k + 1) % 4], thin * 1.6), tone));
                        }
                        // The screen itself, faintly filled.
                        let centre = at + rotation * Vec3::new(0.0, 0.0, -distance);
                        draws.push(MeshDraw3D {
                            unlit: true,
                            blend: an1mbyt3_renderer::Blend3D::Transparent,
                            ..MeshDraw3D::new(std::rc::Rc::clone(&cube), Mat4::from_scale_rotation_translation(Vec3::new(half_w * 2.0, half_h * 2.0, 0.002), rotation, centre), [tone[0], tone[1], tone[2], if active { 0.12 } else { 0.05 }])
                        });
                        // "Up": a small triangle above the screen's top edge.
                        draws.push(unlit(std::rc::Rc::clone(&cone), Mat4::from_scale_rotation_translation(Vec3::new(0.18, 0.16, 0.02), rotation, at + rotation * Vec3::new(0.0, half_h + 0.14, -distance)), tone));
                    }
                    Node3DContent::Light(light) => {
                        let c = light.color;
                        draws.push(unlit(std::rc::Rc::clone(&ball), Mat4::from_scale_rotation_translation(Vec3::splat(0.3), glam::Quat::IDENTITY, at), [c[0], c[1], c[2], 1.0]));
                        if light.kind == Light3DKind::Directional {
                            let dir = world.transform_vector3(Vec3::NEG_Z).normalize_or_zero();
                            draws.push(unlit(std::rc::Rc::clone(&cube), crate::view3d::bar_between(at, at + dir * 1.2, 0.05), [c[0], c[1], c[2], 1.0]));
                        }
                    }
                    _ => draws.push(unlit(std::rc::Rc::clone(&cube), Mat4::from_scale_rotation_translation(Vec3::splat(0.15), rotation, at), [0.9, 0.9, 0.9, 1.0])),
                }
            }
            if let Some((world, content)) = selected {
                // Selection box (orange, over everything).
                let (min, max) = match selected_extent {
                    Some(half) => (-half, half),
                    None => crate::view3d::local_bounds_with(&content, &|m| model_bounds.get(&m).copied()),
                };
                let thickness = cam.gizmo_scale(world.transform_point3(Vec3::ZERO)) * 0.012;
                for (a, b) in crate::view3d::box_edges(world, min, max) {
                    draws.push(MeshDraw3D { overlay: true, ..MeshDraw3D::new(std::rc::Rc::clone(&cube), crate::view3d::bar_between(a, b, thickness), [1.0, 0.6, 0.15, 1.0]) });
                }
                // An avatar's bones: sticks between humanoid joints, the
                // selected bone orange — click one to pose it.
                if let Some(selected_id) = self.view3d_selected {
                    let bones = self.view3d_bone_list(selected_id);
                    if !bones.is_empty() {
                        let ball = self.mesh3d(&Primitive3D::sphere());
                        for (name, pos, parent) in &bones {
                            let s3 = cam.gizmo_scale(*pos);
                            let hot = self.view3d_bone.as_deref() == Some(name.as_str());
                            let color = if hot { [1.0, 0.6, 0.15, 1.0] } else { [0.45, 0.85, 1.0, 0.9] };
                            draws.push(MeshDraw3D { overlay: true, ..MeshDraw3D::new(std::rc::Rc::clone(&ball), Mat4::from_scale_rotation_translation(Vec3::splat(s3 * if hot { 0.075 } else { 0.05 }), glam::Quat::IDENTITY, *pos), color) });
                            if let Some(p) = parent.and_then(|p| bones.get(p)) {
                                draws.push(MeshDraw3D { overlay: true, ..MeshDraw3D::new(std::rc::Rc::clone(&cube), crate::view3d::bar_between(p.1, *pos, s3 * 0.012), [0.45, 0.85, 1.0, 0.6]) });
                            }
                        }
                    }
                }
                // The gizmo, in its mode: X red, Y green, Z blue (yellow when
                // hot). Move: arrows + a centre cube for the ground plane;
                // rotate: a ring per axis; scale: cube tips (local axes) + a
                // centre cube for uniform scale.
                if let Some((origin, axes, mode)) = self.view3d_gizmo_frame(world) {
                    let scale = cam.gizmo_scale(origin);
                    let cylinder = self.mesh3d(&Primitive3D::cylinder());
                    let ring = self.mesh3d(&Primitive3D::Torus { radius: 1.0, tube: 0.014, segments: 72, sides: 6 });
                    let colors = [[0.95, 0.25, 0.25, 1.0], [0.35, 0.9, 0.3, 1.0], [0.3, 0.5, 1.0, 1.0]];
                    let handles = [crate::view3d::GizmoHandle::X, crate::view3d::GizmoHandle::Y, crate::view3d::GizmoHandle::Z];
                    for (i, handle) in handles.into_iter().enumerate() {
                        let color = if self.view3d_hot == Some(handle) { [1.0, 0.9, 0.2, 1.0] } else { colors[i] };
                        let axis = axes[i];
                        // Turns +Y (the primitives' long axis) onto this axis.
                        let rotation = glam::Quat::from_rotation_arc(Vec3::Y, axis);
                        match mode {
                            crate::view3d::GizmoMode::Rotate => {
                                draws.push(MeshDraw3D { overlay: true, ..MeshDraw3D::new(std::rc::Rc::clone(&ring), Mat4::from_scale_rotation_translation(Vec3::splat(scale), rotation, origin), color) });
                            }
                            _ => {
                                let shaft = Mat4::from_scale_rotation_translation(Vec3::new(0.04 * scale, scale, 0.04 * scale), rotation, origin + axis * scale * 0.5);
                                draws.push(MeshDraw3D { overlay: true, ..MeshDraw3D::new(std::rc::Rc::clone(&cylinder), shaft, color) });
                                let tip = if mode == crate::view3d::GizmoMode::Scale {
                                    (std::rc::Rc::clone(&cube), Mat4::from_scale_rotation_translation(Vec3::splat(0.16 * scale), rotation, origin + axis * scale * 1.08))
                                } else {
                                    (std::rc::Rc::clone(&cone), Mat4::from_scale_rotation_translation(Vec3::new(0.14 * scale, 0.28 * scale, 0.14 * scale), rotation, origin + axis * scale * 1.1))
                                };
                                draws.push(MeshDraw3D { overlay: true, ..MeshDraw3D::new(tip.0, tip.1, color) });
                            }
                        }
                    }
                    if mode != crate::view3d::GizmoMode::Rotate {
                        let centre_color = if self.view3d_hot == Some(crate::view3d::GizmoHandle::Plane) { [1.0, 0.9, 0.2, 1.0] } else { [0.95, 0.95, 0.95, 1.0] };
                        draws.push(MeshDraw3D { overlay: true, ..MeshDraw3D::new(std::rc::Rc::clone(&cube), Mat4::from_scale_rotation_translation(Vec3::splat(0.16 * scale), glam::Quat::IDENTITY, origin), centre_color) });
                    }
                }
            }
        }

        // No sun in the scene: a default one, so meshes are never black.
        let (sun_direction, sun_color) = sun.unwrap_or((Vec3::new(-0.4, -1.0, -0.3), Vec3::splat(0.85)));
        self.renderer.set_scene_3d(Some(an1mbyt3_renderer::Scene3D {
            view,
            projection,
            camera_position: eye,
            clear_color: clear,
            sun_direction,
            sun_color,
            ambient: self.environment_3d.map_or(Vec3::new(0.3, 0.33, 0.4), |(_, _, a)| Vec3::from(a)),
            fog_color: self.environment_3d.map_or_else(|| clear.map_or(Vec3::splat(0.5), |c| Vec3::new(c[0], c[1], c[2])), |(c, _, _)| Vec3::from(c)),
            fog_density: self.environment_3d.map_or(0.0, |(_, d, _)| d),
            point_lights: points,
            draws,
            particles,
            fluids,
            // Sun shadows round what the camera is looking at: ~16 m in
            // front (the view's orbit point in the editor).
            shadows: sun.map(|_| {
                let forward = (view.inverse().transform_vector3(Vec3::NEG_Z)).normalize_or(Vec3::NEG_Z);
                let center = if editor_view { self.view3d_camera.target } else { eye + forward * 9.0 };
                an1mbyt3_renderer::Shadows3D { center, extent: 14.0, strength: 0.72 }
            }),
        }));
    }

    // ---------------------------------------------------- 3D scene view --

    /// The 3D camera Playground looks through in a scene (the first
    /// `current` one, else the first), as (node, world matrix).
    fn playground_camera_3d(&self, artboard_id: ArtboardId) -> Option<(NodeId, glam::Mat4, an1mbyt3_core::Camera3D)> {
        let artboard = self.document.artboard(artboard_id)?;
        let mut best: Option<(NodeId, glam::Mat4, an1mbyt3_core::Camera3D)> = None;
        for id in artboard.node_ids() {
            if artboard.is_effectively_deleted(id) {
                continue;
            }
            let Some(NodeKind::Node3D(an1mbyt3_core::Node3D { content: an1mbyt3_core::Node3DContent::Camera(cam), .. })) = artboard.node(id).map(|n| &n.kind) else { continue };
            if best.as_ref().is_none_or(|(_, _, c)| cam.current && !c.current) {
                best = Some((id, artboard.world_matrix_3d(id)?, *cam));
            }
        }
        best
    }

    /// 3D view → "Align camera to view": moves the scene's Playground
    /// camera (or the selected Camera 3D, `node`) to exactly where the 3D
    /// view is looking from. Undoable. Returns the document summary (or
    /// null when the scene has no 3D camera).
    pub fn view3d_align_camera(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let selected: Option<NodeId> = parse_id(node).ok().flatten();
        let target = selected
            .filter(|id| matches!(self.document.artboard(artboard_id).and_then(|a| a.node(*id)).map(|n| &n.kind), Some(NodeKind::Node3D(an1mbyt3_core::Node3D { content: an1mbyt3_core::Node3DContent::Camera(_), .. }))))
            .or_else(|| self.playground_camera_3d(artboard_id).map(|(id, _, _)| id));
        let Some(node_id) = target else { return Ok(JsValue::NULL) };
        let Some(artboard_ref) = self.document.artboard(artboard_id) else { return Ok(JsValue::NULL) };
        let Some(NodeKind::Node3D(node3d)) = artboard_ref.node(node_id).map(|n| n.kind.clone()) else { return Ok(JsValue::NULL) };
        // The view camera's world pose → the node's local pose.
        let world = self.view3d_camera.view().inverse();
        let parent_world = artboard_ref.node(node_id).and_then(|n| n.parent).and_then(|p| artboard_ref.world_matrix_3d(p)).unwrap_or(glam::Mat4::IDENTITY);
        let local = parent_world.inverse() * world;
        let (_, rotation, translation) = local.to_scale_rotation_translation();
        let mut value = node3d;
        value.transform.position = translation;
        value.transform.set_rotation(rotation);
        self.document.apply(Command::SetNode3D { artboard: artboard_id, node: node_id, value });
        self.document_summary()
    }

    /// 3D view → "View through camera": puts the 3D view exactly where the
    /// scene's Playground camera (or `node`, a Camera 3D) is, looking where
    /// it looks, with its field of view.
    pub fn view3d_look_through(&mut self, artboard: JsValue, node: JsValue) -> Result<bool, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let selected: Option<NodeId> = parse_id(node).ok().flatten();
        let picked = selected.and_then(|id| {
            let artboard = self.document.artboard(artboard_id)?;
            match &artboard.node(id)?.kind {
                NodeKind::Node3D(an1mbyt3_core::Node3D { content: an1mbyt3_core::Node3DContent::Camera(cam), .. }) => Some((id, artboard.world_matrix_3d(id)?, *cam)),
                _ => None,
            }
        });
        let Some((_, world, cam)) = picked.or_else(|| self.playground_camera_3d(artboard_id)) else { return Ok(false) };
        let eye = world.transform_point3(glam::Vec3::ZERO);
        let forward = world.transform_vector3(glam::Vec3::NEG_Z).normalize_or(glam::Vec3::NEG_Z);
        let back = -forward;
        let view = &mut self.view3d_camera;
        view.pitch = back.y.clamp(-1.0, 1.0).asin().to_degrees().clamp(-89.0, 89.0);
        view.yaw = back.x.atan2(back.z).to_degrees();
        view.distance = view.distance.clamp(1.0, 50.0);
        view.target = eye + forward * view.distance;
        view.fov_degrees = cam.fov_degrees;
        Ok(true)
    }

    /// Turns Design mode's 3D scene view on / off.
    pub fn view3d_set_enabled(&mut self, enabled: bool) {
        self.view3d_enabled = enabled;
    }

    /// The node the gizmo and selection box follow (the editor selection).
    pub fn view3d_select(&mut self, node: JsValue) -> Result<(), JsValue> {
        self.view3d_selected = parse_id(node)?;
        Ok(())
    }

    /// Right-drag orbit / middle-drag pan / wheel zoom (CSS pixels).
    pub fn view3d_orbit(&mut self, dx: f32, dy: f32) {
        self.view3d_camera.orbit(dx, dy);
    }
    pub fn view3d_pan(&mut self, dx: f32, dy: f32) {
        let ratio = self.last_device_pixel_ratio.max(0.01);
        self.view3d_camera.pan(dx * ratio, dy * ratio, self.canvas_height as f32);
    }
    pub fn view3d_zoom(&mut self, delta: f32) {
        self.view3d_camera.zoom(delta);
    }

    /// F: centre the view on a node.
    pub fn view3d_frame(&mut self, artboard: JsValue, node: JsValue) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let Some(a) = self.document.artboard(artboard_id) else { return Ok(()) };
        let (Some(world), Some(NodeKind::Node3D(n))) = (a.world_matrix_3d(node_id), a.node(node_id).map(|n| &n.kind)) else { return Ok(()) };
        let bounds = self.model_bounds_map();
        let (min, max) = crate::view3d::local_bounds_with(&n.content, &|m| bounds.get(&m).copied());
        let size = world.transform_vector3(max - min).length().max(0.5);
        self.view3d_camera.target = world.transform_point3((min + max) * 0.5);
        self.view3d_camera.distance = (size * 2.2).clamp(1.5, 400.0);
        Ok(())
    }

    fn view3d_point(&self, x: f32, y: f32) -> (glam::Vec2, glam::Vec2) {
        let ratio = self.last_device_pixel_ratio.max(0.01);
        (glam::Vec2::new(x * ratio, y * ratio), glam::Vec2::new(self.canvas_width.max(1) as f32, self.canvas_height.max(1) as f32))
    }

    /// The selected node's world matrix, if any.
    fn view3d_selected_world(&self, artboard: ArtboardId) -> Option<glam::Mat4> {
        self.document.artboard(artboard)?.world_matrix_3d(self.view3d_selected?)
    }

    /// The selected avatar's bones for the overlay / picking: (humanoid
    /// name, world position, index of its parent bone in this list). Empty
    /// for non-VRM nodes or before the model has drawn once.
    fn view3d_bone_list(&self, node: NodeId) -> Vec<(String, glam::Vec3, Option<usize>)> {
        let Some(inst) = self.model_instances.get(&node) else { return Vec::new() };
        let Some(model) = inst.model.and_then(|m| self.model_gpu.get(&m)) else { return Vec::new() };
        let Some(vrm) = &model.data.vrm else { return Vec::new() };
        let mut names: Vec<(&String, usize)> = vrm.humanoid.iter().map(|(n, i)| (n, *i)).filter(|(_, i)| *i < inst.last_pose.len()).collect();
        names.sort_by_key(|(_, i)| *i);
        let index_of: HashMap<usize, usize> = names.iter().enumerate().map(|(k, (_, i))| (*i, k)).collect();
        names
            .iter()
            .map(|(name, i)| {
                let pos = (inst.last_world * inst.last_pose[*i]).transform_point3(glam::Vec3::ZERO);
                // The nearest ancestor that is itself a humanoid bone.
                let mut p = model.parents.get(*i).copied().flatten();
                let mut parent = None;
                while let Some(j) = p {
                    if let Some(k) = index_of.get(&j) {
                        parent = Some(*k);
                        break;
                    }
                    p = model.parents.get(j).copied().flatten();
                }
                ((*name).clone(), pos, parent)
            })
            .collect()
    }

    /// Where the gizmo is and how it's oriented: (origin, handle axes,
    /// mode). A selected bone gets a rotate gizmo at its joint; a node gets
    /// the current mode, world axes (scale: the node's own axes).
    fn view3d_gizmo_frame(&self, node_world: glam::Mat4) -> Option<(glam::Vec3, [glam::Vec3; 3], crate::view3d::GizmoMode)> {
        let world_axes = [glam::Vec3::X, glam::Vec3::Y, glam::Vec3::Z];
        if let (Some(bone), Some(node)) = (&self.view3d_bone, self.view3d_selected) {
            if let Some((_, pos, _)) = self.view3d_bone_list(node).into_iter().find(|(n, _, _)| n == bone) {
                return Some((pos, world_axes, crate::view3d::GizmoMode::Rotate));
            }
        }
        let origin = node_world.transform_point3(glam::Vec3::ZERO);
        let axes = if self.view3d_mode == crate::view3d::GizmoMode::Scale {
            [0, 1, 2].map(|i| node_world.transform_vector3(world_axes[i]).normalize_or(world_axes[i]))
        } else {
            world_axes
        };
        Some((origin, axes, self.view3d_mode))
    }

    fn view3d_gizmo_hit(&self, artboard: ArtboardId, point: glam::Vec2, viewport: glam::Vec2) -> Option<crate::view3d::GizmoHandle> {
        let world = self.view3d_selected_world(artboard)?;
        let (origin, axes, mode) = self.view3d_gizmo_frame(world)?;
        crate::view3d::gizmo_hit_mode(&self.view3d_camera, mode, origin, axes, point, viewport)
    }

    /// A click at canvas point `(x, y)` (CSS px): `{ handle }` (0 X, 1 Y,
    /// 2 Z, 3 centre) if it's on the gizmo, `{ bone }` for a joint of the
    /// selected avatar, else `{ node }` — the Node3D under it, or `null`.
    pub fn view3d_pick(&mut self, artboard: JsValue, x: f32, y: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let (point, viewport) = self.view3d_point(x, y);
        #[derive(serde::Serialize)]
        struct Pick {
            handle: Option<u8>,
            node: Option<NodeId>,
            bone: Option<String>,
        }
        if let Some(h) = self.view3d_gizmo_hit(artboard_id, point, viewport) {
            return to_js_value(&Pick { handle: Some(h.index()), node: None, bone: None });
        }
        // The selected avatar's joints (nearest on screen, within 10 px).
        if let Some(selected) = self.view3d_selected {
            let mut best: Option<(f32, String)> = None;
            for (name, pos, _) in self.view3d_bone_list(selected) {
                let Some(p) = self.view3d_camera.project(pos, viewport) else { continue };
                let d = p.distance(point);
                if d < 10.0 && best.as_ref().is_none_or(|(b, _)| d < *b) {
                    best = Some((d, name));
                }
            }
            if let Some((_, name)) = best {
                return to_js_value(&Pick { handle: None, node: Some(selected), bone: Some(name) });
            }
        }
        let Some(a) = self.document.artboard(artboard_id) else { return to_js_value(&Pick { handle: None, node: None, bone: None }) };
        let (o, d) = self.view3d_camera.ray(point, viewport);
        let bounds = self.model_bounds_map();
        let mut best = crate::view3d::pick_with_distance(a, o, d, &|m| bounds.get(&m).copied());
        // Particle effects are picked by their live particles (what you
        // see), not by their empty node's little marker box.
        for id in a.node_ids() {
            if a.is_effectively_deleted(id) || !crate::view3d::visible_chain(a, id) {
                continue;
            }
            let Some(node) = a.node(id) else { continue };
            let Some(cfg) = node.behaviors.iter().find_map(|e| match &e.behavior {
                an1mbyt3_core::NodeBehavior::ParticleSystem3D(p) if e.enabled => Some(p),
                _ => None,
            }) else {
                continue;
            };
            let Some(world) = a.world_matrix_3d(id) else { continue };
            if let Some(t) = self.particles3d.ray_hit(id, cfg, world, o, d) {
                if best.is_none_or(|(b, _)| t < b) {
                    best = Some((t, id));
                }
            }
        }
        to_js_value(&Pick { handle: None, node: best.map(|(_, id)| id), bone: None })
    }

    /// Hover feedback: highlights the gizmo handle under `(x, y)`, or
    /// `handle` while dragging (`-1` = none / just hover).
    pub fn view3d_hover(&mut self, artboard: JsValue, x: f32, y: f32, dragging: i32) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        if dragging >= 0 {
            self.view3d_hot = crate::view3d::GizmoHandle::from_index(dragging as u8);
            return Ok(());
        }
        let (point, viewport) = self.view3d_point(x, y);
        self.view3d_hot = self.view3d_gizmo_hit(artboard_id, point, viewport);
        Ok(())
    }

    /// W / E / R: the gizmo's mode (0 move, 1 rotate, 2 scale).
    pub fn view3d_set_gizmo_mode(&mut self, mode: u8) {
        self.view3d_mode = crate::view3d::GizmoMode::from_index(mode);
    }

    /// Selects one of the selected avatar's bones (humanoid name; `""` =
    /// none) — its joint gets a rotate gizmo.
    pub fn view3d_select_bone(&mut self, bone: String) {
        self.view3d_bone = (!bone.is_empty()).then_some(bone);
        self.view3d_bone_basis = None;
    }

    /// Dragging gizmo `handle` of the selected node from `(x0, y0)` to
    /// `(x1, y1)` (CSS px), the node's local transform at the drag's start
    /// being `start` = `[px, py, pz, rx, ry, rz, sx, sy, sz]` (rotation in
    /// Euler degrees): the new local transform, same layout. Moves and
    /// rotations happen in world space and are converted through the
    /// parent, so nested nodes follow the gizmo exactly. `snap`: 0.25 m,
    /// 15°, 0.1 × steps.
    #[allow(clippy::too_many_arguments)]
    pub fn view3d_drag_transform(&self, artboard: JsValue, handle: u8, start: Vec<f32>, x0: f32, y0: f32, x1: f32, y1: f32, snap: bool) -> Result<Vec<f32>, JsValue> {
        use crate::view3d::{GizmoHandle, GizmoMode};
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let v = |i: usize| start.get(i).copied().unwrap_or(0.0);
        let mut t = an1mbyt3_core::Transform3D { position: glam::Vec3::new(v(0), v(1), v(2)), rotation_degrees: glam::Vec3::new(v(3), v(4), v(5)), scale: glam::Vec3::new(v(6), v(7), v(8)) };
        let Some(world) = self.view3d_selected_world(artboard_id) else { return Ok(start) };
        let Some((origin, axes, mode)) = self.view3d_gizmo_frame(world) else { return Ok(start) };
        // The parent's world matrix (world = parent × local).
        let parent = world * t.matrix().inverse();
        let (from, viewport) = self.view3d_point(x0, y0);
        let (to, _) = self.view3d_point(x1, y1);
        let cam = &self.view3d_camera;
        let Some(h) = GizmoHandle::from_index(handle) else { return Ok(start) };
        match mode {
            GizmoMode::Move => {
                let start_world = parent.transform_point3(t.position);
                let moved = match h {
                    GizmoHandle::Plane => match (crate::view3d::ground_point(cam, from, viewport, start_world.y), crate::view3d::ground_point(cam, to, viewport, start_world.y)) {
                        (Some(a), Some(b)) => start_world + (b - a),
                        _ => start_world,
                    },
                    _ => {
                        let axis = axes[h.index() as usize];
                        start_world + axis * crate::view3d::axis_drag(cam, origin, axis, from, to, viewport)
                    }
                };
                let mut local = parent.inverse().transform_point3(moved);
                if snap {
                    local = (local / 0.25).round() * 0.25;
                }
                t.position = local;
            }
            GizmoMode::Rotate => {
                if h == GizmoHandle::Plane {
                    return Ok(start);
                }
                let axis = axes[h.index() as usize];
                let mut angle = crate::view3d::ring_drag(cam, origin, axis, from, to, viewport);
                if snap {
                    let step = 15f32.to_radians();
                    angle = (angle / step).round() * step;
                }
                let (_, parent_rot, _) = parent.to_scale_rotation_translation();
                let local = parent_rot.inverse() * glam::Quat::from_axis_angle(axis, angle) * parent_rot * t.rotation();
                t.set_rotation(local.normalize());
            }
            GizmoMode::Scale => {
                let gizmo = cam.gizmo_scale(origin).max(1e-4);
                let factor = |amount: f32| {
                    let f = (1.0 + amount).max(0.01);
                    if snap { ((f * 10.0).round() / 10.0).max(0.1) } else { f }
                };
                match h {
                    GizmoHandle::Plane => {
                        let f = factor(((to.x - from.x) - (to.y - from.y)) / 200.0);
                        t.scale *= f;
                    }
                    _ => {
                        let i = h.index() as usize;
                        let f = factor(crate::view3d::axis_drag(cam, origin, axes[i], from, to, viewport) / gizmo);
                        t.scale[i] *= f;
                    }
                }
            }
        }
        let (p, r, s) = (t.position, t.rotation_degrees, t.scale);
        Ok(vec![p.x, p.y, p.z, r.x, r.y, r.z, s.x, s.y, s.z])
    }

    /// Start dragging the selected bone of `node`, whose active pose has it
    /// at `start` (Euler degrees `[x, y, z]`). `false` if the bone isn't
    /// drawn yet.
    pub fn view3d_bone_drag_begin(&mut self, node: JsValue, start: Vec<f32>) -> Result<bool, JsValue> {
        let node_id: NodeId = parse_id(node)?;
        let Some(bone) = self.view3d_bone.clone() else { return Ok(false) };
        let Some(inst) = self.model_instances.get(&node_id) else { return Ok(false) };
        let Some(model) = inst.model.and_then(|m| self.model_gpu.get(&m)) else { return Ok(false) };
        let Some(index) = Self::model_bone(model, &bone) else { return Ok(false) };
        let Some(m) = inst.last_pose.get(index) else { return Ok(false) };
        let (_, world_rot, _) = (inst.last_world * *m).to_scale_rotation_translation();
        let v = |i: usize| start.get(i).copied().unwrap_or(0.0).to_radians();
        let q = glam::Quat::from_euler(glam::EulerRot::YXZ, v(1), v(0), v(2));
        self.view3d_bone_basis = Some(world_rot * q.inverse());
        Ok(true)
    }

    /// Dragging the bone's rotate gizmo: the pose's new Euler degrees
    /// `[x, y, z]` for the bone (the ring turns the bone about that world
    /// axis), from `start` (its degrees when the drag began).
    #[allow(clippy::too_many_arguments)]
    pub fn view3d_bone_drag(&self, artboard: JsValue, handle: u8, start: Vec<f32>, x0: f32, y0: f32, x1: f32, y1: f32, snap: bool) -> Result<Vec<f32>, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let (Some(basis), Some(h)) = (self.view3d_bone_basis, crate::view3d::GizmoHandle::from_index(handle)) else { return Ok(start) };
        if h == crate::view3d::GizmoHandle::Plane {
            return Ok(start);
        }
        let Some(world) = self.view3d_selected_world(artboard_id) else { return Ok(start) };
        let Some((origin, axes, _)) = self.view3d_gizmo_frame(world) else { return Ok(start) };
        let (from, viewport) = self.view3d_point(x0, y0);
        let (to, _) = self.view3d_point(x1, y1);
        let axis = axes[h.index() as usize];
        let mut angle = crate::view3d::ring_drag(&self.view3d_camera, origin, axis, from, to, viewport);
        if snap {
            let step = 15f32.to_radians();
            angle = (angle / step).round() * step;
        }
        let v = |i: usize| start.get(i).copied().unwrap_or(0.0).to_radians();
        let q = glam::Quat::from_euler(glam::EulerRot::YXZ, v(1), v(0), v(2));
        let next = (basis.inverse() * glam::Quat::from_axis_angle(axis, angle) * basis * q).normalize();
        let (y, x, z) = next.to_euler(glam::EulerRot::YXZ);
        let r = |a: f32| (a.to_degrees() * 100.0).round() / 100.0;
        Ok(vec![r(x), r(y), r(z)])
    }

    /// Untracked live preview of one bone in one VRM pose (bone gizmo
    /// drags): adds the bone to the pose if it isn't there.
    #[allow(clippy::too_many_arguments)]
    pub fn preview_vrm_pose_bone(&mut self, artboard: JsValue, node: JsValue, pose: &str, bone: &str, x: f32, y: f32, z: f32) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let Some(n) = self.document.artboard_mut(artboard_id).and_then(|a| a.node_mut(node_id)) else { return Ok(()) };
        for entry in &mut n.behaviors {
            if let an1mbyt3_core::NodeBehavior::VrmPoses(p) = &mut entry.behavior {
                if let Some(target) = p.poses.iter_mut().find(|q| q.name == pose) {
                    match target.bones.iter_mut().find(|b| b.bone == bone) {
                        Some(b) => b.rotation = [x, y, z],
                        None => target.bones.push(an1mbyt3_core::BonePose { bone: bone.to_string(), rotation: [x, y, z], offset: [0.0; 3] }),
                    }
                }
                break;
            }
        }
        Ok(())
    }

    /// Hold right mouse + WASD / QE: fly the view `forward`, `right`, `up`
    /// metres (the orbit point travels with it).
    pub fn view3d_fly(&mut self, forward: f32, right: f32, up: f32) {
        let cam = &mut self.view3d_camera;
        let eye = cam.eye();
        let f = (cam.target - eye).normalize_or(glam::Vec3::NEG_Z);
        let r = f.cross(glam::Vec3::Y).normalize_or(glam::Vec3::X);
        cam.target += f * forward + r * right + glam::Vec3::Y * up;
    }

    /// Wheel zoom toward the point under the cursor (CSS px).
    pub fn view3d_zoom_at(&mut self, delta: f32, x: f32, y: f32) {
        let (point, viewport) = self.view3d_point(x, y);
        let cam = &mut self.view3d_camera;
        let (o, d) = cam.ray(point, viewport);
        let before = cam.distance;
        cam.zoom(delta);
        let f = cam.distance / before.max(1e-4);
        // The point at the orbit depth under the cursor stays put.
        let p = o + d * before;
        cam.target = p + (cam.target - p) * f;
    }

    /// View presets: `"front"`, `"back"`, `"left"`, `"right"`, `"top"`,
    /// `"bottom"`, `"persp"`.
    pub fn view3d_view(&mut self, preset: &str) {
        let cam = &mut self.view3d_camera;
        let (yaw, pitch) = match preset {
            "front" => (0.0, 0.0),
            "back" => (180.0, 0.0),
            "right" => (90.0, 0.0),
            "left" => (-90.0, 0.0),
            "top" => (0.0, 89.0),
            "bottom" => (0.0, -89.0),
            _ => (30.0, 28.0),
        };
        cam.yaw = yaw;
        cam.pitch = pitch;
    }

    /// The + menu's 3D items: a Node3D with a primitive mesh (`"Cube"`,
    /// `"Sphere"`, `"Plane"`, `"Cylinder"`, `"Cone"`, `"Torus"`), a
    /// `"Camera"`, a `"Sun"` (directional light), a `"PointLight"` or an
    /// `"Empty"` pivot. Undoable. Returns `{ document, newNode }`.
    pub fn add_node3d(&mut self, artboard: JsValue, parent: JsValue, what: &str) -> Result<JsValue, JsValue> {
        use an1mbyt3_core::{Camera3D, Light3D, Light3DKind, Mesh3D, Node3D, Node3DContent, Primitive3D, Transform3D};
        use glam::Vec3;
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let parent_id: Option<NodeId> = parse_id(parent)?;
        let mesh = |p: Primitive3D, y: f32| (Transform3D::at(Vec3::new(0.0, y, 0.0)), Node3DContent::Mesh(Mesh3D::new(p, [0.8, 0.8, 0.82, 1.0])));
        let (transform, content) = match what {
            "Cube" => mesh(Primitive3D::cube(), 0.5),
            "Sphere" => mesh(Primitive3D::sphere(), 0.5),
            "Plane" => mesh(Primitive3D::plane(), 0.0),
            "Cylinder" => mesh(Primitive3D::cylinder(), 0.5),
            "Cone" => mesh(Primitive3D::cone(), 0.5),
            "Torus" => mesh(Primitive3D::torus(), 0.5),
            // Back and up a little, looking slightly down at the origin.
            "Camera" => (
                Transform3D { position: Vec3::new(0.0, 2.0, 5.0), rotation_degrees: Vec3::new(-18.0, 0.0, 0.0), scale: Vec3::ONE },
                Node3DContent::Camera(Camera3D::default()),
            ),
            "Sun" => (
                Transform3D { position: Vec3::new(0.0, 5.0, 0.0), rotation_degrees: Vec3::new(-55.0, -35.0, 0.0), scale: Vec3::ONE },
                Node3DContent::Light(Light3D::default()),
            ),
            "PointLight" => (Transform3D::at(Vec3::new(0.0, 2.0, 0.0)), Node3DContent::Light(Light3D { kind: Light3DKind::Point, color: [1.0, 0.85, 0.6], intensity: 1.5, range: 8.0 })),
            "Empty" => (Transform3D::default(), Node3DContent::Empty),
            other => return Err(JsValue::from_str(&format!("add_node3d: unknown 3D item {other:?}"))),
        };
        let name = match what {
            "PointLight" => "Point Light".to_string(),
            "Sun" => "Sun".to_string(),
            other => other.to_string(),
        };
        let node = Node::new(name, parent_id, NodeKind::Node3D(Node3D::new(transform, content)));
        let inverse = self.document.apply(Command::AddNode { artboard: artboard_id, node });
        let Command::SetDeleted { node: new_node, .. } = inverse else { unreachable!("Command::AddNode's inverse is always a SetDeleted") };
        to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node })
    }

    /// The + menu's UI items, added under the scene's active Camera (the HUD
    /// layer) when it has one: `"Button"` (a rounded panel + label with a
    /// UI Button), `"Toggle"` (box + Checkmark + label), `"Slider"` (track +
    /// Fill + Handle), `"Scrollbar"` (track + Handle), `"ScrollView"` (a
    /// clipped viewport with a Content stack and a linked Scrollbar),
    /// `"Panel"`, `"Label"` (text), `"Stack"` (a group that lays out its
    /// children). Undoable as one step. Returns
    /// `{ document, newNode }`.
    pub fn add_ui_node(&mut self, artboard: JsValue, what: &str) -> Result<JsValue, JsValue> {
        use an1mbyt3_core::NodeBehavior as B;
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let parent = self.document.artboard(artboard_id).and_then(|a| a.active_camera());
        let font = default_text_font(&mut self.document);
        let shape = |name: &str, w: f32, h: f32, color: [f32; 4]| Node::new(name, parent, NodeKind::Shape(ShapeNode { path: an1mbyt3_core::Path::rect(w, h), fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None }));
        let label = |text: &str, size: f32, parent: Option<NodeId>| Node::new(text, parent, NodeKind::Text(TextNode { content: text.into(), font, size_px: size, fill_color: [1.0; 4], gradient: None }));
        let mut inverses = Vec::new();
        let root = match what {
            "Button" => {
                let mut button = shape("Button", 180.0, 48.0, [0.28, 0.39, 0.84, 1.0]);
                button.behaviors.push(an1mbyt3_core::NodeBehaviorEntry { behavior: B::UiButton(an1mbyt3_core::UiButton::default()), enabled: true });
                let inverse = Command::AddNode { artboard: artboard_id, node: button }.apply(&mut self.document);
                let Command::SetDeleted { node: id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
                inverses.push(inverse);
                inverses.push(Command::AddNode { artboard: artboard_id, node: label("Button", 20.0, Some(id)) }.apply(&mut self.document));
                id
            }
            // A track with Fill + Handle children (positioned by the host).
            "Slider" => {
                let mut track = shape("Slider", 220.0, 8.0, [0.16, 0.17, 0.22, 1.0]);
                track.behaviors.push(an1mbyt3_core::NodeBehaviorEntry { behavior: B::UiSlider(an1mbyt3_core::UiSlider::default()), enabled: true });
                let inverse = Command::AddNode { artboard: artboard_id, node: track }.apply(&mut self.document);
                let Command::SetDeleted { node: id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
                inverses.push(inverse);
                let mut fill = shape("Fill", 220.0, 8.0, [0.28, 0.39, 0.84, 1.0]);
                fill.parent = Some(id);
                inverses.push(Command::AddNode { artboard: artboard_id, node: fill }.apply(&mut self.document));
                let mut handle = Node::new("Handle", Some(id), NodeKind::Shape(ShapeNode { path: an1mbyt3_core::Path::ellipse(20.0, 20.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None }));
                handle.parent = Some(id);
                inverses.push(Command::AddNode { artboard: artboard_id, node: handle }.apply(&mut self.document));
                id
            }
            // A box with a Checkmark child (shown while on) and a label.
            "Toggle" => {
                let mut toggle = shape("Toggle", 26.0, 26.0, [0.16, 0.17, 0.22, 1.0]);
                toggle.behaviors.push(an1mbyt3_core::NodeBehaviorEntry { behavior: B::UiToggle(an1mbyt3_core::UiToggle { is_on: true, ..Default::default() }), enabled: true });
                let inverse = Command::AddNode { artboard: artboard_id, node: toggle }.apply(&mut self.document);
                let Command::SetDeleted { node: id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
                inverses.push(inverse);
                let mut check = shape("Checkmark", 14.0, 14.0, [0.4, 0.55, 1.0, 1.0]);
                check.parent = Some(id);
                inverses.push(Command::AddNode { artboard: artboard_id, node: check }.apply(&mut self.document));
                let mut text = label("Toggle", 16.0, Some(id));
                text.transform.translation = Vec2::new(60.0, 0.0);
                inverses.push(Command::AddNode { artboard: artboard_id, node: text }.apply(&mut self.document));
                id
            }
            // Flex containers: a row / column that hugs its children, and a
            // full-screen responsive page (header, then a row of cards
            // that stacks into a column on phones).
            // An HTML + CSS box (starts as a sample card).
            "Html" => {
                let mut node = Node::new("HTML", parent, NodeKind::Group(an1mbyt3_core::GroupNode::default()));
                node.behaviors.push(an1mbyt3_core::NodeBehaviorEntry { behavior: B::UiHtml(an1mbyt3_core::UiHtml::default()), enabled: true });
                let inverse = Command::AddNode { artboard: artboard_id, node }.apply(&mut self.document);
                let Command::SetDeleted { node: id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
                inverses.push(inverse);
                id
            }
            "Row" | "Column" => {
                let mut node = Node::new(what, parent, NodeKind::Group(an1mbyt3_core::GroupNode::default()));
                let direction = if what == "Row" { an1mbyt3_core::UiFlexDirection::Row } else { an1mbyt3_core::UiFlexDirection::Column };
                node.behaviors.push(an1mbyt3_core::NodeBehaviorEntry {
                    behavior: B::UiLayout(an1mbyt3_core::UiLayout { style: an1mbyt3_core::UiFlexStyle { direction, gap: 12.0, align: an1mbyt3_core::UiAlign::Center, ..Default::default() }, ..Default::default() }),
                    enabled: true,
                });
                let inverse = Command::AddNode { artboard: artboard_id, node }.apply(&mut self.document);
                let Command::SetDeleted { node: id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
                inverses.push(inverse);
                for i in 1..=3 {
                    let mut item = shape(&format!("Item {i}"), 120.0, 60.0, [0.18, 0.2, 0.27, 1.0]);
                    item.parent = Some(id);
                    let inverse = Command::AddNode { artboard: artboard_id, node: item }.apply(&mut self.document);
                    let Command::SetDeleted { node: item_id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
                    inverses.push(inverse);
                    inverses.push(Command::AddNode { artboard: artboard_id, node: label(&format!("Item {i}"), 14.0, Some(item_id)) }.apply(&mut self.document));
                }
                id
            }
            "Page" => {
                use an1mbyt3_core::{UiAlign, UiEdges, UiFlexDirection, UiFlexStyle, UiLayout, UiLayoutItem, UiSize};
                let item = |w: UiSize, h: UiSize| an1mbyt3_core::NodeBehaviorEntry { behavior: B::UiLayoutItem(UiLayoutItem { width: w, height: h, ..Default::default() }), enabled: true };
                let mut page = Node::new("Page", parent, NodeKind::Group(an1mbyt3_core::GroupNode::default()));
                page.behaviors.push(an1mbyt3_core::NodeBehaviorEntry {
                    behavior: B::UiLayout(UiLayout {
                        style: UiFlexStyle { direction: UiFlexDirection::Column, gap: 16.0, align: UiAlign::Stretch, padding: UiEdges::all(24.0), ..Default::default() },
                        compact: Some(UiFlexStyle { direction: UiFlexDirection::Column, gap: 10.0, align: UiAlign::Stretch, padding: UiEdges::all(12.0), ..Default::default() }),
                        width: UiSize::Fill,
                        height: UiSize::Fill,
                    }),
                    enabled: true,
                });
                let inverse = Command::AddNode { artboard: artboard_id, node: page }.apply(&mut self.document);
                let Command::SetDeleted { node: page_id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
                inverses.push(inverse);
                let mut header = shape("Header", 100.0, 56.0, [0.16, 0.18, 0.26, 1.0]);
                header.parent = Some(page_id);
                header.behaviors.push(item(UiSize::Fill, UiSize::Fixed(56.0)));
                let inverse = Command::AddNode { artboard: artboard_id, node: header }.apply(&mut self.document);
                let Command::SetDeleted { node: header_id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
                inverses.push(inverse);
                inverses.push(Command::AddNode { artboard: artboard_id, node: label("Header", 20.0, Some(header_id)) }.apply(&mut self.document));
                let mut cards = Node::new("Cards", Some(page_id), NodeKind::Group(an1mbyt3_core::GroupNode::default()));
                cards.behaviors.push(an1mbyt3_core::NodeBehaviorEntry {
                    behavior: B::UiLayout(UiLayout {
                        style: UiFlexStyle { direction: UiFlexDirection::Row, gap: 16.0, align: UiAlign::Stretch, ..Default::default() },
                        compact: Some(UiFlexStyle { direction: UiFlexDirection::Column, gap: 10.0, align: UiAlign::Stretch, ..Default::default() }),
                        width: UiSize::Fill,
                        height: UiSize::Fill,
                    }),
                    enabled: true,
                });
                cards.behaviors.push(item(UiSize::Fill, UiSize::Fill));
                let inverse = Command::AddNode { artboard: artboard_id, node: cards }.apply(&mut self.document);
                let Command::SetDeleted { node: cards_id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
                inverses.push(inverse);
                for i in 1..=3 {
                    let mut card = shape(&format!("Card {i}"), 100.0, 100.0, [0.12, 0.13, 0.19, 1.0]);
                    card.parent = Some(cards_id);
                    card.behaviors.push(item(UiSize::Fill, UiSize::Fill));
                    let inverse = Command::AddNode { artboard: artboard_id, node: card }.apply(&mut self.document);
                    let Command::SetDeleted { node: card_id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
                    inverses.push(inverse);
                    inverses.push(Command::AddNode { artboard: artboard_id, node: label(&format!("Card {i}"), 16.0, Some(card_id)) }.apply(&mut self.document));
                }
                page_id
            }
            // A track with a Handle child.
            "Scrollbar" => {
                let mut track = shape("Scrollbar", 12.0, 180.0, [0.16, 0.17, 0.22, 1.0]);
                track.behaviors.push(an1mbyt3_core::NodeBehaviorEntry { behavior: B::UiScrollbar(an1mbyt3_core::UiScrollbar::default()), enabled: true });
                let inverse = Command::AddNode { artboard: artboard_id, node: track }.apply(&mut self.document);
                let Command::SetDeleted { node: id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
                inverses.push(inverse);
                let mut handle = shape("Handle", 12.0, 180.0, [0.55, 0.57, 0.66, 1.0]);
                handle.parent = Some(id);
                inverses.push(Command::AddNode { artboard: artboard_id, node: handle }.apply(&mut self.document));
                id
            }
            // A clipped viewport with a stacked Content list and a Scrollbar.
            "ScrollView" => {
                let mut view = shape("Scroll View", 260.0, 200.0, [0.08, 0.08, 0.11, 0.9]);
                view.clip_children = true;
                view.behaviors.push(an1mbyt3_core::NodeBehaviorEntry { behavior: B::UiScrollView(an1mbyt3_core::UiScrollView::default()), enabled: true });
                let inverse = Command::AddNode { artboard: artboard_id, node: view }.apply(&mut self.document);
                let Command::SetDeleted { node: id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
                inverses.push(inverse);
                let mut content = Node::new("Content", Some(id), NodeKind::Group(an1mbyt3_core::GroupNode::default()));
                content.behaviors.push(an1mbyt3_core::NodeBehaviorEntry { behavior: B::UiStack(an1mbyt3_core::UiStack { spacing: 6.0, padding: 8.0, ..Default::default() }), enabled: true });
                let inverse = Command::AddNode { artboard: artboard_id, node: content }.apply(&mut self.document);
                let Command::SetDeleted { node: content_id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
                inverses.push(inverse);
                for i in 1..=10 {
                    let mut row = shape(&format!("Item {i}"), 220.0, 30.0, [0.18, 0.2, 0.27, 1.0]);
                    row.parent = Some(content_id);
                    let inverse = Command::AddNode { artboard: artboard_id, node: row }.apply(&mut self.document);
                    let Command::SetDeleted { node: row_id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
                    inverses.push(inverse);
                    inverses.push(Command::AddNode { artboard: artboard_id, node: label(&format!("Item {i}"), 14.0, Some(row_id)) }.apply(&mut self.document));
                }
                let mut bar = shape("Scrollbar", 10.0, 188.0, [0.12, 0.13, 0.17, 1.0]);
                bar.parent = Some(id);
                bar.transform.translation = Vec2::new(121.0, 0.0);
                bar.behaviors.push(an1mbyt3_core::NodeBehaviorEntry { behavior: B::UiScrollbar(an1mbyt3_core::UiScrollbar::default()), enabled: true });
                let inverse = Command::AddNode { artboard: artboard_id, node: bar }.apply(&mut self.document);
                let Command::SetDeleted { node: bar_id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
                inverses.push(inverse);
                let mut handle = shape("Handle", 10.0, 188.0, [0.5, 0.52, 0.62, 1.0]);
                handle.parent = Some(bar_id);
                inverses.push(Command::AddNode { artboard: artboard_id, node: handle }.apply(&mut self.document));
                id
            }
            "Panel" | "Label" | "Stack" => {
                let mut node = match what {
                    "Panel" => shape("Panel", 320.0, 220.0, [0.08, 0.08, 0.11, 0.85]),
                    "Label" => label("Label", 24.0, parent),
                    _ => Node::new("Stack", parent, NodeKind::Group(an1mbyt3_core::GroupNode::default())),
                };
                if what == "Stack" {
                    node.behaviors.push(an1mbyt3_core::NodeBehaviorEntry { behavior: B::UiStack(an1mbyt3_core::UiStack::default()), enabled: true });
                }
                let inverse = Command::AddNode { artboard: artboard_id, node }.apply(&mut self.document);
                let Command::SetDeleted { node: id, .. } = inverse else { unreachable!("AddNode's inverse is SetDeleted") };
                inverses.push(inverse);
                id
            }
            other => return Err(JsValue::from_str(&format!("add_ui_node: unknown UI item {other:?}"))),
        };
        // One undo step for the whole item (button + its label).
        inverses.reverse();
        self.document.commit_batch(inverses);
        to_js_value(&AddNodeResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_node: root })
    }

    /// Transparent export mode (Export → transparent GIF / WebM): while on,
    /// frames clear to fully transparent and draw no editor grid, so the
    /// canvas read back holds only the artwork, with its alpha.
    pub fn set_export_transparent(&mut self, on: bool) {
        self.export_transparent = on;
    }

    /// Export mode (any GIF / video export): while on, the frames render
    /// the way the game looks — post effects, particles and goo included —
    /// even when exporting from Design.
    pub fn set_exporting(&mut self, on: bool) {
        self.exporting = on;
    }

    /// Sets the artboard's screen behaviour (Fixed / Responsive, scale
    /// match, compact breakpoint). Undoable.
    pub fn set_artboard_screen(&mut self, artboard: JsValue, value: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let value: an1mbyt3_core::ScreenSettings = serde_wasm_bindgen::from_value(value).map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.document.apply(Command::SetArtboardScreen { artboard: artboard_id, value });
        self.document_summary()
    }

    /// Design mode's "preview on" screen size in CSS px (`0, 0` = the
    /// artboard's own size): UI Layouts / Anchors / Show When react to it.
    pub fn set_preview_screen(&mut self, width: f32, height: f32) {
        self.preview_screen = (width > 0.0 && height > 0.0).then_some(Vec2::new(width, height));
    }

    /// The last frame's screen: `[view width, view height (world units),
    /// screen width, screen height (CSS px), compact (0/1), portrait (0/1)]`.
    pub fn screen_info_now(&self) -> Vec<f32> {
        match self.last_screen {
            Some(s) => vec![s.view.x, s.view.y, s.screen.x, s.screen.y, s.compact as u8 as f32, s.portrait as u8 as f32],
            None => vec![0.0; 6],
        }
    }

    /// A node's *current* local bounds `[min x, min y, max x, max y]` — for
    /// the host's scroll views, whose content (a stack laid out this frame)
    /// changes size.
    pub fn node_local_bounds(&self, artboard: JsValue, node: JsValue) -> Result<Vec<f32>, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let Some(a) = self.document.artboard(artboard_id).filter(|a| a.node(node_id).is_some()) else { return Ok(vec![0.0; 4]) };
        let (min, max) = crate::dto::local_bounds(a, node_id);
        Ok(vec![min.x, min.y, max.x, max.y])
    }

    /// The Inspector's 3D section: replaces a Node3D's transform + content.
    /// Undoable.
    pub fn set_node3d(&mut self, artboard: JsValue, node: JsValue, value: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let before = self.record_before(artboard_id, node_id);
        let value: an1mbyt3_core::Node3D = serde_wasm_bindgen::from_value(value).map_err(|e| JsValue::from_str(&format!("set_node3d: {e}")))?;
        let is_3d = self.document.artboard(artboard_id).and_then(|a| a.node(node_id)).is_some_and(|n| matches!(n.kind, NodeKind::Node3D(_)));
        if !is_3d {
            return Err(JsValue::from_str("set_node3d: not a 3D node"));
        }
        self.document.apply(Command::SetNode3D { artboard: artboard_id, node: node_id, value });
        self.record_field_edits(artboard_id, node_id, before);
        self.document_summary()
    }

    /// Untracked per-frame 3D transform write for scripts
    /// (`ref.position3 = ...`): `[px, py, pz, rx, ry, rz, sx, sy, sz]`,
    /// rotation in degrees. Same "no history spam" reasoning as every
    /// `preview_*`; leaving Playground restores the designed values.
    pub fn preview_node3d_transform(&mut self, artboard: JsValue, node: JsValue, values: Vec<f32>) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        if values.len() != 9 {
            return Err(JsValue::from_str("preview_node3d_transform: expected 9 values"));
        }
        if let Some(NodeKind::Node3D(n)) = self.document.artboard_mut(artboard_id).and_then(|a| a.node_mut(node_id)).map(|n| &mut n.kind) {
            n.transform.position = glam::Vec3::new(values[0], values[1], values[2]);
            n.transform.rotation_degrees = glam::Vec3::new(values[3], values[4], values[5]);
            n.transform.scale = glam::Vec3::new(values[6], values[7], values[8]);
        }
        Ok(())
    }

    /// Script API (`ref.shakeTree(strength)`): the tree sways hard for a
    /// moment and drops leaves (strength ~0.5 = a nudge, 2 = a big shake).
    pub fn tree_shake(&mut self, node: JsValue, strength: f32) -> Result<(), JsValue> {
        let node_id: NodeId = parse_id(node)?;
        if let Some(state) = self.tree_states.get_mut(&node_id) {
            state.shake(strength.max(0.0));
        }
        Ok(())
    }

    /// Script API (`ref.regrowTree()`): restarts its clock — with Grow on
    /// Play on, it grows in again from a sapling.
    pub fn tree_regrow(&mut self, node: JsValue) -> Result<(), JsValue> {
        let node_id: NodeId = parse_id(node)?;
        if let Some(state) = self.tree_states.get_mut(&node_id) {
            state.time = 0.0;
            state.leaves.clear();
        }
        Ok(())
    }

    /// Script API (`ref.grabCloth(x, y, radius)`): takes hold of the cloth's
    /// free point nearest the world point (within `radius`). `false` if
    /// there's none there (or the cloth isn't simulating yet).
    pub fn cloth_grab(&mut self, node: JsValue, x: f32, y: f32, radius: f32) -> Result<bool, JsValue> {
        let node_id: NodeId = parse_id(node)?;
        Ok(self.cloth_sims.get_mut(&node_id).is_some_and(|sim| sim.grab(Vec2::new(x, y), radius)))
    }

    /// Script API (`ref.dragCloth(x, y)`): moves the held point.
    pub fn cloth_drag(&mut self, node: JsValue, x: f32, y: f32) -> Result<(), JsValue> {
        let node_id: NodeId = parse_id(node)?;
        if let Some(sim) = self.cloth_sims.get_mut(&node_id) {
            sim.drag(Vec2::new(x, y));
        }
        Ok(())
    }

    /// Script API (`ref.releaseCloth()`): lets go.
    pub fn cloth_release(&mut self, node: JsValue) -> Result<(), JsValue> {
        let node_id: NodeId = parse_id(node)?;
        if let Some(sim) = self.cloth_sims.get_mut(&node_id) {
            sim.release();
        }
        Ok(())
    }

    /// Script API (`ref.tearCloth(x, y, radius)`): cuts the threads around
    /// a world point; returns how many were cut.
    pub fn cloth_tear(&mut self, node: JsValue, x: f32, y: f32, radius: f32) -> Result<u32, JsValue> {
        let node_id: NodeId = parse_id(node)?;
        Ok(self.cloth_sims.get_mut(&node_id).map_or(0, |sim| sim.tear(Vec2::new(x, y), radius) as u32))
    }

    /// Script API (`ref.setClothWind(x, y)` / `ref.setClothWind(null)`):
    /// overrides the authored wind until play stops.
    pub fn cloth_wind(&mut self, node: JsValue, wind: Option<Vec<f32>>) -> Result<(), JsValue> {
        let node_id: NodeId = parse_id(node)?;
        if let Some(sim) = self.cloth_sims.get_mut(&node_id) {
            sim.wind_override = wind.filter(|w| w.len() == 2).map(|w| Vec2::new(w[0], w[1]));
        }
        Ok(())
    }

    /// Script API (`ref.resetCloth()`): mends it — back to hanging at rest.
    pub fn cloth_reset(&mut self, node: JsValue) -> Result<(), JsValue> {
        let node_id: NodeId = parse_id(node)?;
        self.cloth_sims.remove(&node_id);
        Ok(())
    }

    /// Script API (`ctx.setEnvironment3`): the 3D pass's fog (colour +
    /// exponential-squared density, 0 = none) and sky ambient light, until
    /// Playground stops; a negative density clears the override.
    pub fn set_environment_3d(&mut self, fog_r: f32, fog_g: f32, fog_b: f32, fog_density: f32, ambient_r: f32, ambient_g: f32, ambient_b: f32) {
        self.environment_3d = (fog_density >= 0.0).then_some(([fog_r, fog_g, fog_b], fog_density, [ambient_r, ambient_g, ambient_b]));
    }

    /// The `Primitive3D::Voxels` size of `node` (a block world), if it is one.
    fn voxel_size(&self, artboard_id: ArtboardId, node_id: NodeId) -> Option<[u32; 3]> {
        match &self.document.artboard(artboard_id)?.node(node_id)?.kind {
            NodeKind::Node3D(an1mbyt3_core::Node3D { content: an1mbyt3_core::Node3DContent::Mesh(an1mbyt3_core::Mesh3D { primitive: an1mbyt3_core::Primitive3D::Voxels { width, height, depth }, .. }), .. }) => Some([*width, *height, *depth]),
            _ => None,
        }
    }

    /// Script API (`ref.setVoxels(bytes)`): replaces a block world's blocks
    /// wholesale — one byte per block, x fastest, then z, then y (index =
    /// `x + width * (z + depth * y)`), block ids from
    /// `an1mbyt3_geometry::voxel::BLOCKS`. Short input leaves the rest air.
    /// Runtime state, not a document edit (no undo, not saved).
    pub fn voxels_set(&mut self, artboard: JsValue, node: JsValue, blocks: Vec<u8>) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let size = self.voxel_size(artboard_id, node_id).ok_or_else(|| JsValue::from_str("setVoxels: not a block world (Voxels mesh)"))?;
        let mut world = VoxelWorld::dense(size);
        if let VoxelStore::Dense(grid) = &mut world.store {
            let n = blocks.len().min(grid.blocks.len());
            grid.blocks[..n].copy_from_slice(&blocks[..n]);
        }
        self.voxel_worlds.insert(node_id, world);
        Ok(())
    }

    /// Script API (`ref.setVoxelChunk(cx, cz, bytes)`): loads (or replaces)
    /// one chunk of a **streamed** block world — the column of blocks
    /// `[cx*16, cx*16+16) × [0, height) × [cz*16, cz*16+16)`, any `cx` / `cz`
    /// (negative too: the world has no edge). Bytes are x fastest, then z,
    /// then y (`lx + 16 * (lz + 16 * y)`). The first chunk switches the node
    /// from a fixed-size world to a streamed one. Neighbouring chunks are
    /// re-meshed too, so faces and shading across the seam are right.
    pub fn voxel_chunk_set(&mut self, artboard: JsValue, node: JsValue, cx: i32, cz: i32, blocks: Vec<u8>) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let size = self.voxel_size(artboard_id, node_id).ok_or_else(|| JsValue::from_str("setVoxelChunk: not a block world (Voxels mesh)"))?;
        let world = self.voxel_worlds.entry(node_id).or_insert_with(|| VoxelWorld::chunked(size));
        if world.size != size || matches!(world.store, VoxelStore::Dense(_)) {
            *world = VoxelWorld::chunked(size);
        }
        world.set_chunk(cx, cz, blocks);
        Ok(())
    }

    /// Script API (`ref.unloadVoxelChunk(cx, cz)`): drops a streamed chunk
    /// (and its meshes) — the far-away part of the world you walked away
    /// from.
    pub fn voxel_chunk_unload(&mut self, artboard: JsValue, node: JsValue, cx: i32, cz: i32) -> Result<(), JsValue> {
        let _: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        if let Some(world) = self.voxel_worlds.get_mut(&node_id) {
            world.unload_chunk(cx, cz);
        }
        Ok(())
    }

    /// Script API (`ref.setVoxel(x, y, z, block)`): sets one block (0 =
    /// air) and re-meshes just the regions it touches. `false` outside the
    /// world.
    pub fn voxel_set(&mut self, artboard: JsValue, node: JsValue, x: i32, y: i32, z: i32, block: u8) -> Result<bool, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let size = self.voxel_size(artboard_id, node_id).ok_or_else(|| JsValue::from_str("setVoxel: not a block world (Voxels mesh)"))?;
        let world = self.voxel_worlds.entry(node_id).or_insert_with(|| VoxelWorld::dense(size));
        Ok(world.set(x, y, z, block))
    }

    /// One draw per non-empty meshed region of a block world that the
    /// camera can see, re-meshing edited / newly loaded regions first —
    /// nearest the camera first, at most `VOXEL_MESH_BUDGET` a frame, so
    /// streaming in a big area never stalls a frame.
    #[allow(clippy::too_many_arguments)]
    fn voxel_draws(&mut self, node_id: NodeId, size: [u32; 3], world_matrix: glam::Mat4, mesh: &an1mbyt3_core::Mesh3D, eye: glam::Vec3, frustum: &[glam::Vec4; 6], draws: &mut Vec<an1mbyt3_renderer::MeshDraw3D>) {
        if self.voxel_material_3d.is_none() {
            // (Validated by a renderer test, so this can't fail at runtime.)
            let Ok(material) = self.renderer.compile_material_3d(an1mbyt3_geometry::VOXEL_MATERIAL_WGSL) else { return };
            self.voxel_material_3d = Some(std::rc::Rc::new(material));
        }
        if self.voxel_atlas_gpu.is_none() {
            let (w, h, rgba) = an1mbyt3_geometry::voxel_atlas();
            self.voxel_atlas_gpu = Some(self.renderer.upload_texture_wrapped(w, h, &rgba, an1mbyt3_renderer::ImageFilterMode::Nearest, false));
        }
        let world = self.voxel_worlds.entry(node_id).or_insert_with(|| VoxelWorld::dense(size));
        // The node was resized: start over with an empty world that size.
        if world.size != size {
            *world = VoxelWorld::dense(size);
        }
        let local_eye = world_matrix.inverse().transform_point3(eye);
        let r = VOXEL_REGION as f32;
        let centre = |region: &[i32; 3]| glam::Vec3::new((region[0] as f32 + 0.5) * r, (region[1] as f32 + 0.5) * r, (region[2] as f32 + 0.5) * r);
        if !world.dirty.is_empty() {
            // (Streamed columns wait for their 4 neighbours, so seam faces and
            // shading are right the first time and each is meshed once.)
            let mut queue: Vec<[i32; 3]> = world.dirty.iter().copied().filter(|r| world.meshable(r)).collect();
            if queue.len() > VOXEL_MESH_BUDGET {
                queue.sort_by(|a, b| centre(a).distance_squared(local_eye).total_cmp(&centre(b).distance_squared(local_eye)));
                queue.truncate(VOXEL_MESH_BUDGET);
            }
            for region in queue {
                world.dirty.remove(&region);
                let min = [region[0] * VOXEL_REGION as i32, region[1] * VOXEL_REGION as i32, region[2] * VOXEL_REGION as i32];
                let max = [min[0] + VOXEL_REGION as i32, min[1] + VOXEL_REGION as i32, min[2] + VOXEL_REGION as i32];
                let data = an1mbyt3_geometry::mesh_voxel_region(&world.store, min, max);
                if data.indices.is_empty() {
                    world.regions.remove(&region);
                } else {
                    world.regions.insert(region, std::rc::Rc::new(self.renderer.upload_mesh_3d(&data)));
                }
            }
        }
        let material = self.voxel_material_3d.clone();
        let texture = self.voxel_atlas_gpu.clone();
        // A region's bounding sphere, in world space.
        let scale = world_matrix.to_scale_rotation_translation().0.max_element();
        let radius = r * 0.87 * scale;
        for (region, gpu) in &world.regions {
            let at = world_matrix.transform_point3(centre(region));
            if frustum.iter().any(|p| p.truncate().dot(at) + p.w < -radius * p.truncate().length()) {
                continue;
            }
            draws.push(an1mbyt3_renderer::MeshDraw3D {
                shininess: mesh.shininess,
                unlit: mesh.unlit,
                material: material.clone(),
                texture: texture.clone(),
                ..an1mbyt3_renderer::MeshDraw3D::new(std::rc::Rc::clone(gpu), world_matrix, mesh.color)
            });
        }
    }

    /// A glass pane's runtime state, (re)cut when its shape changed.
    fn glass_runtime(&mut self, node_id: NodeId, primitive: an1mbyt3_core::Primitive3D) -> Option<&mut GlassRuntime> {
        glass_entry(&mut self.glass, node_id, primitive)
    }

    /// Glass draws: the intact pieces (re-meshed only when something
    /// breaks) in the pane's space, plus its flying shards (re-meshed every
    /// frame while there are any) in world space. Both wear the pane's
    /// colour/material; below 100 % alpha they draw see-through.
    fn glass_draws(&mut self, node_id: NodeId, world: glam::Mat4, mesh: &an1mbyt3_core::Mesh3D, material: Option<std::rc::Rc<an1mbyt3_renderer::GpuMaterial3D>>, draws: &mut Vec<an1mbyt3_renderer::MeshDraw3D>) {
        let delta = self.frame_delta;
        let renderer = &self.renderer;
        let Some(runtime) = glass_entry(&mut self.glass, node_id, mesh.primitive) else { return };
        let flying = runtime.pane.step(delta);
        if runtime.intact.as_ref().is_none_or(|(v, _)| *v != runtime.pane.version) {
            let data = runtime.pane.intact_mesh();
            let gpu = (!data.indices.is_empty()).then(|| std::rc::Rc::new(renderer.upload_mesh_3d(&data)));
            runtime.intact = Some((runtime.pane.version, gpu));
        }
        runtime.shards = if flying {
            let data = runtime.pane.shards_mesh();
            (!data.indices.is_empty()).then(|| std::rc::Rc::new(renderer.upload_mesh_3d(&data)))
        } else {
            None
        };
        let blend = if mesh.color[3] < 0.999 { an1mbyt3_renderer::Blend3D::Transparent } else { an1mbyt3_renderer::Blend3D::Opaque };
        let draw = |gpu: &std::rc::Rc<an1mbyt3_renderer::GpuMesh3D>, model: glam::Mat4| an1mbyt3_renderer::MeshDraw3D { shininess: mesh.shininess, unlit: mesh.unlit, material: material.clone(), blend, ..an1mbyt3_renderer::MeshDraw3D::new(std::rc::Rc::clone(gpu), model, mesh.color) };
        if let Some((_, Some(gpu))) = &runtime.intact {
            draws.push(draw(gpu, world));
        }
        if let Some(gpu) = &runtime.shards {
            draws.push(draw(gpu, glam::Mat4::IDENTITY));
        }
    }
    /// The `Primitive3D` of a 3D mesh node, with its world matrix.
    fn mesh3d_node(&self, artboard_id: ArtboardId, node_id: NodeId) -> Option<(an1mbyt3_core::Primitive3D, glam::Mat4)> {
        let artboard = self.document.artboard(artboard_id)?;
        match &artboard.node(node_id)?.kind {
            NodeKind::Node3D(an1mbyt3_core::Node3D { content: an1mbyt3_core::Node3DContent::Mesh(mesh), .. }) => Some((mesh.primitive, artboard.world_matrix_3d(node_id)?)),
            _ => None,
        }
    }

    /// Script API (`ref.shatter3`): breaks a glass pane where something hit
    /// it — world point (`px, py, pz`; projected onto the pane), the hitting
    /// object's world velocity (`vx, vy, vz`, m/s; shards fly along it),
    /// how far the breakage reaches (`radius`, metres) and the world height
    /// shards land on (`floor`; pass `-Infinity` for none). A hit on a hole
    /// breaks nothing. Returns `{ broken, loose }`: pieces knocked out, and
    /// pieces that lost their support and fell. Untracked runtime state.
    #[allow(clippy::too_many_arguments)]
    pub fn glass_shatter(&mut self, artboard: JsValue, node: JsValue, px: f32, py: f32, pz: f32, vx: f32, vy: f32, vz: f32, radius: f32, floor: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let Some((primitive, world)) = self.mesh3d_node(artboard_id, node_id) else { return Err(JsValue::from_str("shatter3: not a glass pane")) };
        let local = world.inverse().transform_point3(glam::Vec3::new(px, py, pz));
        // Metres → the pane's local units (its average in-plane scale).
        let scale = ((world.x_axis.truncate().length() + world.y_axis.truncate().length()) * 0.5).max(1e-6);
        let seed = (px.to_bits() ^ py.to_bits().rotate_left(11) ^ pz.to_bits().rotate_left(22)) ^ node_id_hash(node_id);
        let Some(runtime) = self.glass_runtime(node_id, primitive) else { return Err(JsValue::from_str("shatter3: not a glass pane")) };
        let result = runtime.pane.shatter(&an1mbyt3_geometry::ShatterParams { point: local.truncate(), radius: radius / scale, velocity: glam::Vec3::new(vx, vy, vz), world, floor, seed });
        to_js_value(&crate::dto::ShatterDto { broken: result.broken as u32, loose: result.loose as u32 })
    }

    /// Script API (`ref.crack3`): cracks a glass pane where something hit it
    /// without breaking it (thick glass that survives a hit) — world point,
    /// reach in metres, how much to whiten (0..1, accumulates). Returns
    /// the pieces touched (0 on a hole).
    #[allow(clippy::too_many_arguments)]
    pub fn glass_crack(&mut self, artboard: JsValue, node: JsValue, px: f32, py: f32, pz: f32, radius: f32, amount: f32) -> Result<u32, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let Some((primitive, world)) = self.mesh3d_node(artboard_id, node_id) else { return Err(JsValue::from_str("crack3: not a glass pane")) };
        let local = world.inverse().transform_point3(glam::Vec3::new(px, py, pz));
        let scale = ((world.x_axis.truncate().length() + world.y_axis.truncate().length()) * 0.5).max(1e-6);
        let Some(runtime) = self.glass_runtime(node_id, primitive) else { return Err(JsValue::from_str("crack3: not a glass pane")) };
        Ok(runtime.pane.crack(local.truncate(), radius / scale, amount) as u32)
    }

    /// Script API (`ref.resetGlass()`): every piece back in place.
    pub fn glass_reset(&mut self, artboard: JsValue, node: JsValue) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        if let Some((primitive, _)) = self.mesh3d_node(artboard_id, node_id) {
            if let Some(runtime) = self.glass_runtime(node_id, primitive) {
                runtime.pane.reset();
            }
        }
        Ok(())
    }

    /// Script API (`ref.glassIntact`): how much of a pane is left, 0..1.
    pub fn glass_intact(&mut self, artboard: JsValue, node: JsValue) -> Result<f32, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let Some((primitive, _)) = self.mesh3d_node(artboard_id, node_id) else { return Ok(0.0) };
        Ok(self.glass_runtime(node_id, primitive).map_or(0.0, |r| r.pane.intact_fraction()))
    }

    /// Script API (`ctx.glassRaycast3(origin, dir, max)`): the nearest
    /// *still-standing* glass along a ray, over every visible pane in the
    /// scene — holes let it through. Same hit shape as `ctx.raycast3`
    /// (`normal` faces back towards the ray's origin), or null.
    #[allow(clippy::too_many_arguments)]
    pub fn glass_raycast(&mut self, artboard: JsValue, ox: f32, oy: f32, oz: f32, dx: f32, dy: f32, dz: f32, max_distance: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let origin = glam::Vec3::new(ox, oy, oz);
        let dir = glam::Vec3::new(dx, dy, dz).normalize_or_zero();
        if dir == glam::Vec3::ZERO {
            return to_js_value(&None::<crate::dto::RayHit3Dto>);
        }
        let panes: Vec<(NodeId, an1mbyt3_core::Primitive3D, glam::Mat4)> = {
            let Some(artboard) = self.document.artboard(artboard_id) else { return to_js_value(&None::<crate::dto::RayHit3Dto>) };
            // Only panes the renderer has met (every visible pane gets a
            // runtime when first drawn) — not every node in the scene.
            self.glass
                .keys()
                .copied()
                .filter(|id| !artboard.is_effectively_deleted(*id) && crate::view3d::visible_chain(artboard, *id))
                .filter_map(|id| match &artboard.node(id)?.kind {
                    NodeKind::Node3D(an1mbyt3_core::Node3D { content: an1mbyt3_core::Node3DContent::Mesh(mesh), .. }) if matches!(mesh.primitive, an1mbyt3_core::Primitive3D::Glass { .. }) => Some((id, mesh.primitive, artboard.world_matrix_3d(id)?)),
                    _ => None,
                })
                .collect()
        };
        let mut best: Option<crate::dto::RayHit3Dto> = None;
        for (id, primitive, world) in panes {
            // Cheap reject: the ray's closest approach to the pane's centre
            // vs its bounding radius.
            let centre = world.transform_point3(glam::Vec3::ZERO);
            let along = (centre - origin).dot(dir);
            if let an1mbyt3_core::Primitive3D::Glass { width, height, .. } = primitive {
                let reach = world.x_axis.truncate().length() * width * 0.5 + world.y_axis.truncate().length() * height * 0.5;
                if along < -reach || along > max_distance + reach || (origin + dir * along).distance(centre) > reach {
                    continue;
                }
            }
            let inverse = world.inverse();
            let local_origin = inverse.transform_point3(origin);
            let local_dir = inverse.transform_vector3(dir);
            let Some(runtime) = self.glass_runtime(id, primitive) else { continue };
            let Some((t, _)) = runtime.pane.raycast(local_origin, local_dir, max_distance) else { continue };
            if best.as_ref().is_some_and(|b| b.distance <= t) {
                continue;
            }
            let point = origin + dir * t;
            let mut normal = inverse.transpose().transform_vector3(glam::Vec3::Z).normalize_or_zero();
            if normal.dot(dir) > 0.0 {
                normal = -normal;
            }
            best = Some(crate::dto::RayHit3Dto { node: id, x: point.x, y: point.y, z: point.z, normal_x: normal.x, normal_y: normal.y, normal_z: normal.z, distance: t });
        }
        to_js_value(&best)
    }

    /// Recompiles every shader-graph material whose generated WGSL changed
    /// since last time, and forgets deleted ones.
    fn sync_materials(&mut self) {
        let stamp = (self.document.instance(), self.document.revision());
        if self.materials_checked_at == Some(stamp) {
            return;
        }
        self.materials_checked_at = Some(stamp);
        let live: Vec<(an1mbyt3_core::MaterialId, Result<String, String>)> = self
            .document
            .materials()
            .filter_map(|(id, m)| match &m.kind {
                an1mbyt3_core::MaterialKind::ShaderGraph(graph) => Some((id, an1mbyt3_core::compile_wgsl(graph).map_err(|e| e.to_string()))),
                _ => None,
            })
            .collect();
        self.material_gpu.retain(|id, _| live.iter().any(|(live_id, _)| live_id == id));
        for (id, wgsl) in live {
            let key = match &wgsl {
                Ok(src) => src.clone(),
                Err(e) => format!("// error: {e}"),
            };
            if self.material_gpu.get(&id).is_some_and(|(cached, _)| *cached == key) {
                continue;
            }
            let compiled = wgsl.and_then(|src| self.renderer.compile_material(&src));
            self.material_gpu.insert(id, (key, compiled));
        }
    }

    /// The Shader tab's "+ New Material": a shader-graph material with the
    /// starter graph. Undoable. Returns `{ document, material }`.
    pub fn add_material(&mut self, name: String) -> Result<JsValue, JsValue> {
        let inverse = self.document.apply(Command::AddMaterial { material: an1mbyt3_core::Material::shader_graph(name) });
        let Command::SetMaterialDeleted { material, .. } = inverse else { unreachable!("AddMaterial's inverse is always SetMaterialDeleted") };
        #[derive(serde::Serialize)]
        struct Out {
            document: crate::dto::DocumentSummary,
            material: an1mbyt3_core::MaterialId,
        }
        to_js_value(&Out { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), material })
    }

    /// A material's shader graph (`null` for a non-graph material).
    pub fn material_graph(&self, material: JsValue) -> Result<JsValue, JsValue> {
        let id: an1mbyt3_core::MaterialId = parse_id(material)?;
        let graph = self.document.material(id).and_then(|m| match &m.kind {
            an1mbyt3_core::MaterialKind::ShaderGraph(graph) => Some(graph),
            _ => None,
        });
        to_js_value(&graph)
    }

    /// Replaces a material's shader graph (one undo step per call — the
    /// graph editor sends one per finished edit, not per drag frame).
    pub fn set_material_graph(&mut self, material: JsValue, graph: JsValue) -> Result<JsValue, JsValue> {
        let id: an1mbyt3_core::MaterialId = parse_id(material)?;
        let graph: an1mbyt3_core::ShaderGraphDef = serde_wasm_bindgen::from_value(graph).map_err(|e| JsValue::from_str(&format!("set_material_graph: {e}")))?;
        let Some(current) = self.document.material(id) else { return Err(JsValue::from_str("set_material_graph: no such material")) };
        let value = an1mbyt3_core::Material { kind: an1mbyt3_core::MaterialKind::ShaderGraph(graph), ..current.clone() };
        self.document.apply(Command::SetMaterial { material: id, value: Box::new(value) });
        self.document_summary()
    }

    /// Renames a material. Undoable.
    pub fn rename_material(&mut self, material: JsValue, name: String) -> Result<JsValue, JsValue> {
        let id: an1mbyt3_core::MaterialId = parse_id(material)?;
        let Some(current) = self.document.material(id) else { return Err(JsValue::from_str("rename_material: no such material")) };
        let value = an1mbyt3_core::Material { name, ..current.clone() };
        self.document.apply(Command::SetMaterial { material: id, value: Box::new(value) });
        self.document_summary()
    }

    /// Deletes a material (soft — undo brings it back). Nodes wearing it
    /// draw normally meanwhile.
    pub fn delete_material(&mut self, material: JsValue) -> Result<JsValue, JsValue> {
        let id: an1mbyt3_core::MaterialId = parse_id(material)?;
        self.document.apply(Command::SetMaterialDeleted { material: id, value: true });
        self.document_summary()
    }

    /// `{ wgsl, error }` for a material — the Shader tab's code view and
    /// error banner. `error` is a graph error (no Output, a loop) or a
    /// shader compile error; `null` when it compiled.
    /// "+ Toon material": a new Toon material asset, `style` `"MToon"` or
    /// `"Arcane"`. Returns `{ document, material }`.
    pub fn add_toon_material(&mut self, name: String, style: &str) -> Result<JsValue, JsValue> {
        let value = if style == "Arcane" { an1mbyt3_core::ToonMaterial::arcane() } else { an1mbyt3_core::ToonMaterial::default() };
        let inverse = self.document.apply(Command::AddMaterial { material: an1mbyt3_core::Material { name, kind: an1mbyt3_core::MaterialKind::Toon(value), deleted: false } });
        let Command::SetMaterialDeleted { material, .. } = inverse else { return Err(JsValue::from_str("add_toon_material: unexpected inverse")) };
        #[derive(serde::Serialize)]
        struct Added {
            document: crate::dto::DocumentSummary,
            material: an1mbyt3_core::MaterialId,
        }
        to_js_value(&Added { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), material })
    }

    /// A Toon material's settings (null for other kinds).
    pub fn material_toon(&self, material: JsValue) -> Result<JsValue, JsValue> {
        let id: an1mbyt3_core::MaterialId = parse_id(material)?;
        match self.toon_material(id) {
            Some(t) => to_js_value(&t),
            None => Ok(JsValue::NULL),
        }
    }

    /// Edits a Toon material's settings. `commit = false` previews (live,
    /// untracked — slider drags); `commit = true` records one undo step
    /// from the value before the first preview.
    pub fn set_material_toon(&mut self, material: JsValue, value: JsValue, commit: bool) -> Result<JsValue, JsValue> {
        let id: an1mbyt3_core::MaterialId = parse_id(material)?;
        let value: an1mbyt3_core::ToonMaterial = serde_wasm_bindgen::from_value(value).map_err(|e| JsValue::from_str(&format!("set_material_toon: {e}")))?;
        let Some(current) = self.document.material(id).cloned() else { return Ok(JsValue::NULL) };
        let an1mbyt3_core::MaterialKind::Toon(before) = &current.kind else { return Ok(JsValue::NULL) };
        if !commit {
            self.toon_preview_base.entry(id).or_insert_with(|| before.clone());
            if let Some(m) = self.document.material_mut(id) {
                m.kind = an1mbyt3_core::MaterialKind::Toon(value);
            }
            return Ok(JsValue::NULL);
        }
        // Back to the pre-drag value, then one tracked edit to the new one.
        if let Some(base) = self.toon_preview_base.remove(&id) {
            if let Some(m) = self.document.material_mut(id) {
                m.kind = an1mbyt3_core::MaterialKind::Toon(base);
            }
        }
        let mut next = current.clone();
        next.kind = an1mbyt3_core::MaterialKind::Toon(value);
        self.document.apply(Command::SetMaterial { material: id, value: Box::new(next) });
        self.document_summary()
    }

    /// Inspector → "Extract materials" on an imported VRM: makes an
    /// editable Toon material asset for each of its MToon materials that
    /// doesn't have one. Returns the document summary (null if none made).
    pub fn model_extract_materials(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard.clone())?;
        let node_id: NodeId = parse_id(node.clone())?;
        let model_id = match self.document.artboard(artboard_id).and_then(|a| a.node(node_id)).map(|n| &n.kind) {
            Some(NodeKind::Node3D(an1mbyt3_core::Node3D { content: an1mbyt3_core::Node3DContent::Mesh(an1mbyt3_core::Mesh3D { primitive: an1mbyt3_core::Primitive3D::Model { model }, .. }), .. })) => *model,
            _ => return Ok(JsValue::NULL),
        };
        let Some(gpu) = self.gpu_model(model_id) else { return Ok(JsValue::NULL) };
        let name = self.document.model(model_id).map(|m| m.name.rsplit(['/', '\\']).next().unwrap_or(&m.name).trim_end_matches(".vrm").trim_end_matches(".glb").to_string()).unwrap_or_default();
        if self.extract_model_materials(model_id, &gpu.data, &name) == 0 {
            return Ok(JsValue::NULL);
        }
        self.document_summary()
    }

    pub fn material_status(&self, material: JsValue) -> Result<JsValue, JsValue> {
        let id: an1mbyt3_core::MaterialId = parse_id(material)?;
        #[derive(serde::Serialize)]
        struct Status {
            wgsl: Option<String>,
            error: Option<String>,
            #[serde(rename = "errorNode")]
            error_node: Option<u32>,
        }
        let graph = self.document.material(id).and_then(|m| match &m.kind {
            an1mbyt3_core::MaterialKind::ShaderGraph(graph) => Some(graph),
            _ => None,
        });
        let status = match graph.map(an1mbyt3_core::compile_wgsl) {
            None => Status { wgsl: None, error: Some("not a shader-graph material".into()), error_node: None },
            Some(Err(e)) => Status { wgsl: None, error: Some(e.message.clone()), error_node: e.node },
            Some(Ok(src)) => {
                let gpu_error = self.material_gpu.get(&id).and_then(|(_, compiled)| compiled.as_ref().err().cloned());
                Status { wgsl: Some(src), error: gpu_error, error_node: None }
            }
        };
        to_js_value(&status)
    }

    /// Every shader-graph node type with its ports — the graph editor's
    /// palette and socket layout come from here (one source of truth).
    pub fn shader_node_catalog(&self) -> Result<JsValue, JsValue> {
        use an1mbyt3_core::shader_graph::{PortDefault, PortType};
        #[derive(serde::Serialize)]
        struct Port {
            name: &'static str,
            /// "Float" | "Vec2" | "Vec3" | "Vec4" | "Dynamic"
            ty: String,
            /// The unwired value (4 floats), or null for "the sprite's UV".
            default: Option<[f32; 4]>,
        }
        #[derive(serde::Serialize)]
        struct Kind {
            kind: an1mbyt3_core::ShaderNodeKind,
            category: &'static str,
            #[serde(rename = "hasValue")]
            has_value: bool,
            inputs: Vec<Port>,
            outputs: Vec<Port>,
        }
        let port = |p: &an1mbyt3_core::shader_graph::PortSpec| Port {
            name: p.name,
            ty: match p.ty {
                PortType::Fixed(t) => format!("{t:?}"),
                PortType::Dynamic => "Dynamic".into(),
            },
            default: match p.default {
                PortDefault::Value(v) => Some(v),
                PortDefault::Uv => None,
            },
        };
        let kinds: Vec<Kind> = an1mbyt3_core::ShaderNodeKind::ALL
            .iter()
            .map(|&kind| Kind { kind, category: kind.category(), has_value: kind.has_value(), inputs: kind.inputs().iter().map(port).collect(), outputs: kind.outputs().iter().map(port).collect() })
            .collect();
        to_js_value(&kinds)
    }

    pub fn preview_break_joint(&mut self, artboard: JsValue, node: JsValue) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        if let Some(node) = self.document.artboard_mut(artboard_id).and_then(|a| a.node_mut(node_id)) {
            for entry in &mut node.behaviors {
                if matches!(entry.behavior, an1mbyt3_core::NodeBehavior::HingeJoint2D(_)) {
                    entry.enabled = false;
                }
            }
        }
        Ok(())
    }

    pub fn preview_body_type(&mut self, artboard: JsValue, node: JsValue, body_type: &str, deep: bool) -> Result<(), JsValue> {
        use an1mbyt3_core::RigidbodyType2D;
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let body_type = match body_type {
            "Dynamic" => RigidbodyType2D::Dynamic,
            "Kinematic" => RigidbodyType2D::Kinematic,
            "Static" => RigidbodyType2D::Static,
            other => return Err(JsValue::from_str(&format!("unknown body type {other:?}"))),
        };
        let Some(artboard_mut) = self.document.artboard_mut(artboard_id) else { return Ok(()) };
        let targets = if deep { subtree(artboard_mut, node_id) } else { vec![node_id] };
        for id in targets {
            if let Some(node) = artboard_mut.node_mut(id) {
                set_body_type(node, body_type);
            }
        }
        Ok(())
    }

    /// Bone drag, FK: rotates `bone` (untracked) so it points at the world
    /// point `(x, y)`; its children follow. Commit with `commit_bone_pose`.
    pub fn preview_bone_aim(&mut self, artboard: JsValue, bone: JsValue, x: f32, y: f32) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let bone_id: NodeId = parse_id(bone)?;
        let Some(artboard_mut) = self.document.artboard_mut(artboard_id) else { return Ok(()) };
        let rotation = an1mbyt3_constraints::aim_bone(artboard_mut, bone_id, Vec2::new(x, y));
        an1mbyt3_constraints::apply_pose(artboard_mut, &[(bone_id, rotation)]);
        Ok(())
    }

    /// Bone drag, IK: poses the chain of up to `chain_length` bones ending
    /// at `tip` (untracked) so the tip reaches toward `(x, y)` — analytic for
    /// two bones (keeping the current bend side), FABRIK for longer chains.
    /// Returns the posed bone ids, root first, for `commit_bone_pose`.
    pub fn preview_bone_ik(&mut self, artboard: JsValue, tip: JsValue, chain_length: u32, x: f32, y: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let tip_id: NodeId = parse_id(tip)?;
        let Some(artboard_mut) = self.document.artboard_mut(artboard_id) else { return to_js_value(&Vec::<NodeId>::new()) };
        let chain = an1mbyt3_constraints::bone_chain(artboard_mut, tip_id, chain_length as usize);
        let pose = an1mbyt3_constraints::solve_chain(artboard_mut, &chain, Vec2::new(x, y), None, 12);
        an1mbyt3_constraints::apply_pose(artboard_mut, &pose);
        to_js_value(&chain)
    }

    /// Ends a bone drag as ONE undoable edit: `bones` are the posed nodes
    /// and `original` their rotations (radians) from before the drag. The
    /// originals are restored first so the undo entry records them.
    pub fn commit_bone_pose(&mut self, artboard: JsValue, bones: JsValue, original: Vec<f32>) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let bones: Vec<NodeId> = serde_wasm_bindgen::from_value(bones).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let Some(artboard_mut) = self.document.artboard_mut(artboard_id) else { return self.document_summary() };
        let mut commands = Vec::new();
        for (i, &id) in bones.iter().enumerate() {
            let Some(node) = artboard_mut.node_mut(id) else { continue };
            let posed = node.transform.rotation;
            if let Some(&before) = original.get(i) {
                node.transform.rotation = before;
            }
            commands.push(Command::SetRotation { artboard: artboard_id, node: id, value: posed });
        }
        if !commands.is_empty() {
            self.document.apply(Command::Batch(commands));
        }
        self.document_summary()
    }

    /// Soft-deletes `node` (`Command::SetDeleted` — see its doc comment for
    /// why deletion is a flag flip, not real `Artboard`-arena removal) as one
    /// undoable edit. `undo()` restores it exactly, `NodeId` included, no
    /// different than any other command.
    pub fn delete_node(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        self.document.apply(Command::SetDeleted { artboard: artboard_id, node: node_id, value: true });
        self.document_summary()
    }

    /// Soft-deletes every id in `nodes` as one undoable edit — the canvas/
    /// Layers panel multi-select's "Delete" action. A `Command::Batch` of one
    /// `Command::SetDeleted` per id, same "one user action, one undo step"
    /// reasoning `import_svg`'s batched insert already established, rather
    /// than issuing `delete_node` once per id (which would cost as many undo
    /// presses as selected nodes to fully revert one multi-delete).
    pub fn delete_nodes(&mut self, artboard: JsValue, nodes: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_ids: Vec<NodeId> = parse_id(nodes)?;
        let commands = node_ids.into_iter().map(|node| Command::SetDeleted { artboard: artboard_id, node, value: true }).collect();
        self.document.apply(Command::Batch(commands));
        self.document_summary()
    }

    /// Renames `node` — the Layers panel's double-click-to-rename action.
    /// Undoable (`Command::SetNodeName`).
    pub fn rename_node(&mut self, artboard: JsValue, node: JsValue, name: &str) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        self.document.apply(Command::SetNodeName { artboard: artboard_id, node: node_id, value: name.to_string() });
        self.document_summary()
    }

    /// Changes a node's `id` (`getCompByID`'s lookup key) — the Inspector's
    /// id field. Same "swap in a new value" shape as `rename_node`; doesn't
    /// check uniqueness itself, see `Command::SetNodeId`'s own doc comment.
    pub fn set_node_id(&mut self, artboard: JsValue, node: JsValue, value: String) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        self.document.apply(Command::SetNodeId { artboard: artboard_id, node: node_id, value });
        self.document_summary()
    }

    /// Replaces a node's whole `classes` list (`getCompByClass`'s selector
    /// tags) — the Inspector's classes field.
    pub fn set_node_classes(&mut self, artboard: JsValue, node: JsValue, value: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let classes: Vec<String> = serde_wasm_bindgen::from_value(value).map_err(|err| JsValue::from_str(&format!("invalid classes: {err}")))?;
        self.document.apply(Command::SetNodeClasses { artboard: artboard_id, node: node_id, value: classes });
        self.document_summary()
    }

    /// Toggles a node's `clip_children` — see that field's own doc comment
    /// (`an1mbyt3_core::Node`) for what it does. Works on any node kind.
    pub fn set_node_clip_children(&mut self, artboard: JsValue, node: JsValue, value: bool) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        self.document.apply(Command::SetNodeClipChildren { artboard: artboard_id, node: node_id, value });
        self.document_summary()
    }

    /// Creates a new, empty, named `Animation` on `artboard` and selects it
    /// as the active one — the Timeline's animation dropdown "New" action.
    /// Returns the new `AnimationId` alongside the updated summary, same
    /// "the id only comes from the inverse `Command::AddAnimation` hands
    /// back" reasoning as `add_shape_node`.
    pub fn add_animation(&mut self, artboard: JsValue, name: &str) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let inverse = self.document.apply(Command::AddAnimation { artboard: artboard_id, animation: Animation::new(name) });
        let Command::SetAnimationDeleted { animation: new_animation, .. } = inverse else {
            unreachable!("Command::AddAnimation's inverse is always a SetAnimationDeleted")
        };
        self.active_animation = Some(new_animation);
        self.scrub_time = 0.0;

        to_js_value(&AddAnimationResult {
            document: summarize_document(&self.document, self.active_animation, self.active_state_machine),
            new_animation,
        })
    }

    /// Builds a small "hover and sway" animation on `node` (a 2-second loop:
    /// rise 24 units then settle back, with a gentle ±3° rotation) and makes
    /// it the active animation — only ever used by the Shapes menu's "Load
    /// Demo Scene" action (`App.tsx`'s `handleLoadDemo`) so the demo scene
    /// has something actually *animated* to point at. Unlike `add_keyframe`
    /// — built for the Timeline's interactive "scrub, edit, capture whatever
    /// the live value now is" workflow, one keyframe per call — this
    /// constructs the whole `Animation` (every keyframe, both properties) as
    /// one fully-formed value and applies it as a single
    /// `Command::AddAnimation`. That's deliberate: the demo needs exact,
    /// deterministic keyframe values with no dependency on `self`'s current
    /// mode flags or live document state across several chained calls — the
    /// earlier version of this (built via `preview_node_translation`/
    /// `preview_node_rotation` immediately followed by `add_keyframe`, driven
    /// from the frontend) depended on several sequential wasm calls each
    /// leaving `self` in the right state for the next, with no way to verify
    /// that short of manually clicking through the running app (this crate
    /// isn't unit-testable in a plain `cargo test` run — `WasmEditor::create`
    /// needs a real `HtmlCanvasElement`/wgpu surface). Constructing the whole
    /// `Animation` as one value removes that whole class of ordering risk.
    /// `flame`, if given (an `OpaqueId`, or `null`/`undefined` to skip it),
    /// gets its own fast scale-pulse + warm-color-cycle tracks added to the
    /// same animation — `DEMO_SVG`'s `Flame` node, so the demo has more than
    /// one piece visibly moving, not just the rocket's own hover.
    pub fn add_demo_hover_animation(&mut self, artboard: JsValue, node: JsValue, flame: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let flame_id: Option<NodeId> = parse_id(flame)?;

        let mut animation = Animation::new("Liftoff");
        animation.duration = 2.0;
        animation.tracks.push((
            node_id,
            PropertyTrack::Translation(Track::new(vec![
                Keyframe::new(0.0, Vec2::new(0.0, 0.0), Easing::Linear),
                Keyframe::new(1.0, Vec2::new(0.0, 24.0), Easing::Linear),
                Keyframe::new(2.0, Vec2::new(0.0, 0.0), Easing::Linear),
            ])),
        ));
        animation.tracks.push((
            node_id,
            PropertyTrack::Rotation(Track::new(vec![
                Keyframe::new(0.0, -3.0_f32.to_radians(), Easing::Linear),
                Keyframe::new(1.0, 3.0_f32.to_radians(), Easing::Linear),
                Keyframe::new(2.0, -3.0_f32.to_radians(), Easing::Linear),
            ])),
        ));
        if let Some(flame_id) = flame_id {
            animation.tracks.push((
                flame_id,
                PropertyTrack::Scale(Track::new(vec![
                    Keyframe::new(0.0, Vec2::new(1.0, 1.0), Easing::Linear),
                    Keyframe::new(0.33, Vec2::new(1.15, 0.85), Easing::Linear),
                    Keyframe::new(0.66, Vec2::new(0.9, 1.2), Easing::Linear),
                    Keyframe::new(1.0, Vec2::new(1.1, 0.9), Easing::Linear),
                    Keyframe::new(1.33, Vec2::new(0.95, 1.1), Easing::Linear),
                    Keyframe::new(1.66, Vec2::new(1.15, 0.85), Easing::Linear),
                    Keyframe::new(2.0, Vec2::new(1.0, 1.0), Easing::Linear),
                ])),
            ));
            animation.tracks.push((
                flame_id,
                PropertyTrack::FillColor(Track::new(vec![
                    Keyframe::new(0.0, [1.0, 0.8, 0.4, 1.0], Easing::Linear),
                    Keyframe::new(0.5, [1.0, 0.55, 0.2, 1.0], Easing::Linear),
                    Keyframe::new(1.0, [1.0, 0.35, 0.15, 1.0], Easing::Linear),
                    Keyframe::new(1.5, [1.0, 0.55, 0.2, 1.0], Easing::Linear),
                    Keyframe::new(2.0, [1.0, 0.8, 0.4, 1.0], Easing::Linear),
                ])),
            ));
        }

        let inverse = self.document.apply(Command::AddAnimation { artboard: artboard_id, animation });
        let Command::SetAnimationDeleted { animation: new_animation, .. } = inverse else {
            unreachable!("Command::AddAnimation's inverse is always a SetAnimationDeleted")
        };
        self.active_animation = Some(new_animation);
        self.scrub_time = 0.0;
        self.resample_active_animation(artboard_id);

        to_js_value(&AddAnimationResult {
            document: summarize_document(&self.document, self.active_animation, self.active_state_machine),
            new_animation,
        })
    }

    /// Switches which animation the Timeline authors/scrubs — the dropdown's
    /// selection action. Resets the scrub position to `0.0` and immediately
    /// re-samples so the canvas reflects the newly selected animation's pose
    /// at that time, not whatever the previously active one last sampled.
    pub fn set_active_animation(&mut self, artboard: JsValue, animation: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let animation_id: AnimationId = parse_id(animation)?;
        self.active_animation = Some(animation_id);
        self.scrub_time = 0.0;
        self.resample_active_animation(artboard_id);
        self.document_summary()
    }

    /// Renames an animation — the dropdown's rename action. Undoable.
    pub fn rename_animation(&mut self, artboard: JsValue, animation: JsValue, name: &str) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let animation_id: AnimationId = parse_id(animation)?;
        self.document.apply(Command::SetAnimationName { artboard: artboard_id, animation: animation_id, value: name.to_string() });
        self.document_summary()
    }

    /// Resizes an animation's fixed, authored time range — the Timeline's
    /// duration input. Clamps `self.scrub_time` (and re-samples) if the new,
    /// shorter duration would leave the playhead past the end.
    pub fn set_animation_duration(&mut self, artboard: JsValue, animation: JsValue, seconds: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let animation_id: AnimationId = parse_id(animation)?;
        self.document.apply(Command::SetAnimationDuration { artboard: artboard_id, animation: animation_id, value: seconds });
        if self.scrub_time > seconds {
            self.scrub_time = seconds.max(0.0);
            self.resample_active_animation(artboard_id);
        }
        self.document_summary()
    }

    /// Resizes the artboard's canvas — the Inspector's "Canvas" section
    /// width/height fields.
    pub fn set_artboard_size(&mut self, artboard: JsValue, width: f32, height: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        self.document.apply(Command::SetArtboardSize { artboard: artboard_id, width, height });
        self.document_summary()
    }

    /// Sets the artboard's pixelation block size, in pixels — the
    /// Inspector's "Canvas" section's resolution field. Takes effect the
    /// next frame (`render`'s own `Renderer::set_resolution_scale` call),
    /// not immediately — this method only edits the document.
    pub fn set_artboard_resolution_scale(&mut self, artboard: JsValue, value: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        self.document.apply(Command::SetArtboardResolutionScale { artboard: artboard_id, value });
        self.document_summary()
    }

    /// Soft-deletes an animation (`Command::SetAnimationDeleted` — see its
    /// doc comment). Clears `active_animation` if it was the one just
    /// deleted, so the Timeline falls back to its "nothing active" state
    /// instead of pointing at a hidden animation.
    pub fn delete_animation(&mut self, artboard: JsValue, animation: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let animation_id: AnimationId = parse_id(animation)?;
        self.document.apply(Command::SetAnimationDeleted { artboard: artboard_id, animation: animation_id, value: true });
        if self.active_animation == Some(animation_id) {
            self.active_animation = None;
        }
        self.document_summary()
    }

    /// Inserts (or, at an already-keyframed `time`, overwrites) a keyframe on
    /// `node`'s `property` track within the active animation, using the
    /// node's *current* live value for that property (its `transform`, or
    /// `NodeKind`'s fill color) — the Timeline's per-property "Add Keyframe"
    /// action. The caller never passes a value directly: position/rotate/
    /// scale/recolor the node where you want it first, then call this —
    /// same two-step "pose it, then key it" workflow every keyframe
    /// animation tool uses. Always `Easing::Linear` — a keyframe's easing
    /// is edited afterward, via `set_keyframe_easing` below (the Curve
    /// Editor panel), not chosen at creation time.
    /// Timeline "Record": while on (in Animate), every Inspector change
    /// becomes keyframes at the playhead.
    pub fn set_record_keys(&mut self, on: bool) {
        self.record_keys = on;
    }

    pub fn record_keys(&self) -> bool {
        self.record_keys
    }

    /// Every animatable value of a node (the Timeline's "+ Property" list):
    /// `[{ path, shape, value }]` — see `an1mbyt3_core::field_path`. The 2D
    /// translation / rotation / scale have their own tracks and aren't
    /// repeated here.
    pub fn node_fields(&self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let Some(n) = self.document.artboard(artboard_id).and_then(|a| a.node(node_id)) else { return Ok(JsValue::NULL) };
        let fields: Vec<an1mbyt3_core::field_path::FieldInfo> = an1mbyt3_core::field_path::list(n).into_iter().filter(|f| !is_dedicated_track(&f.path)).collect();
        to_js_value(&fields)
    }

    /// Before an Inspector edit: the node as it was, when the edit might
    /// turn into keyframes (Animate, with an animation open).
    fn record_before(&self, artboard_id: ArtboardId, node_id: NodeId) -> Option<Node> {
        if !self.animate_mode_active || self.active_animation.is_none() {
            return None;
        }
        self.document.artboard(artboard_id)?.node(node_id).cloned()
    }

    /// After an Inspector edit (already applied as one undo step): keys
    /// every value it changed that's already animated — or every changed
    /// value while Record is on — at the playhead. A value keyed for the
    /// first time away from 0 s also gets its old value keyed at 0 s, so
    /// it animates *from* where it was. Folded into the edit's undo step.
    fn record_field_edits(&mut self, artboard_id: ArtboardId, node_id: NodeId, before: Option<Node>) {
        let Some(before) = before else { return };
        let Some(animation_id) = self.active_animation else { return };
        let Some(artboard) = self.document.artboard(artboard_id) else { return };
        let Some(after) = artboard.node(node_id) else { return };
        let Some(animation) = artboard.animation(animation_id) else { return };
        let time = self.scrub_time.clamp(0.0, animation.duration);
        let tracked: std::collections::HashSet<PropertyKind> = animation.tracks.iter().filter(|(n, _)| *n == node_id).map(|(_, t)| t.kind()).collect();
        let old: HashMap<String, [f32; 4]> = an1mbyt3_core::field_path::list(&before).into_iter().map(|f| (f.path, f.value)).collect();
        let mut keys: Vec<(f32, String, [f32; 4])> = Vec::new();
        for f in an1mbyt3_core::field_path::list(after) {
            if is_dedicated_track(&f.path) {
                continue;
            }
            let Some(o) = old.get(&f.path) else { continue };
            if o.iter().zip(&f.value).all(|(a, b)| (a - b).abs() <= 1e-6) {
                continue;
            }
            let is_tracked = tracked.contains(&an1mbyt3_core::field_path::field_kind(&f.path));
            if !is_tracked && !self.record_keys {
                continue;
            }
            if !is_tracked && time > 1e-4 {
                keys.push((0.0, f.path.clone(), *o));
            }
            // Leaving Animate puts the value back to its design value.
            if let Some(entry) = self.design_pose_snapshot.as_mut().filter(|snap| snap.artboard == artboard_id).and_then(|snap| snap.entries.iter_mut().find(|e| e.id == node_id)) {
                if !entry.fields.iter().any(|(p, _)| *p == f.path) {
                    entry.fields.push((f.path.clone(), *o));
                }
            }
            keys.push((time, f.path, f.value));
        }
        let count = keys.len();
        for (t, path, value) in keys {
            self.document.apply(Command::AddKeyframe { artboard: artboard_id, animation: animation_id, node: node_id, time: t, value: KeyframeValue::Field(path, value), easing: Easing::Linear });
        }
        if count > 0 {
            self.document.merge_last(count + 1);
        }
    }

    pub fn add_keyframe(&mut self, artboard: JsValue, node: JsValue, property: &str, time: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let property_kind = parse_property_kind(property)?;
        let animation_id = self.active_animation.ok_or_else(|| JsValue::from_str("add_keyframe: no active animation"))?;
        let field_path = property.strip_prefix("Field:").map(str::to_string);

        let artboard_ref = self
            .document
            .artboard(artboard_id)
            .ok_or_else(|| JsValue::from_str("add_keyframe: unknown artboard"))?;
        // Clamped to the animation's fixed, authored range — a keyframe can
        // never land outside `duration`, the same "bound by the composition's
        // own length" invariant every mainstream keyframe tool enforces (see
        // `Animation::duration`'s doc comment).
        let duration = artboard_ref.animation(animation_id).map(|a| a.duration).unwrap_or(0.0);
        let time = time.clamp(0.0, duration);
        let node_ref = artboard_ref
            .node(node_id)
            .ok_or_else(|| JsValue::from_str("add_keyframe: unknown node"))?;

        let value = match property_kind {
            PropertyKind::Translation => KeyframeValue::Translation(node_ref.transform.translation),
            PropertyKind::Rotation => KeyframeValue::Rotation(node_ref.transform.rotation),
            PropertyKind::Scale => KeyframeValue::Scale(node_ref.transform.scale),
            PropertyKind::FillColor => {
                let Some(color) = node_ref.fill_color() else {
                    return Err(JsValue::from_str("add_keyframe: node has no fill color"));
                };
                KeyframeValue::FillColor(color)
            }
            PropertyKind::Path => match &node_ref.kind {
                NodeKind::Shape(shape) => KeyframeValue::Path(shape.path.clone()),
                _ => return Err(JsValue::from_str("add_keyframe: node has no path")),
            },
            PropertyKind::MotionPath => {
                return Err(JsValue::from_str("add_keyframe: use add_motion_path_keyframe for MotionPath tracks"));
            }
            PropertyKind::Field(_) => {
                let path = field_path.unwrap_or_default();
                let Some(v) = an1mbyt3_core::field_path::read(node_ref, &path) else {
                    return Err(JsValue::from_str(&format!("add_keyframe: node has no field {path}")));
                };
                KeyframeValue::Field(path, v)
            }
        };

        self.document
            .apply(Command::AddKeyframe { artboard: artboard_id, animation: animation_id, node: node_id, time, value, easing: Easing::Linear });
        self.document_summary()
    }

    /// Binds `node`'s `MotionPath` track (within the active animation) to
    /// `guide` — creating the track (with no progress keyframes yet) if this
    /// is the first time `node` has been given a motion-path guide, or
    /// rebinding an existing track's guide in place otherwise. See
    /// `Command::SetMotionPathGuide`'s doc comment. Call this once (when the
    /// user picks a guide shape for a node in the Timeline) before
    /// `add_motion_path_keyframe`.
    pub fn set_motion_path_guide(&mut self, artboard: JsValue, node: JsValue, guide: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let guide_id: NodeId = parse_id(guide)?;
        let animation_id = self.active_animation.ok_or_else(|| JsValue::from_str("set_motion_path_guide: no active animation"))?;
        self.document.apply(Command::SetMotionPathGuide { artboard: artboard_id, animation: animation_id, node: node_id, guide: guide_id });
        self.document_summary()
    }

    /// Removes `node`'s entire `MotionPath` track (guide binding and any
    /// progress keyframes) from the active animation — the Timeline's
    /// "remove motion path" action. See `Command::RemoveMotionPathTrack`'s
    /// doc comment.
    pub fn remove_motion_path_track(&mut self, artboard: JsValue, node: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let animation_id = self.active_animation.ok_or_else(|| JsValue::from_str("remove_motion_path_track: no active animation"))?;
        self.document.apply(Command::RemoveMotionPathTrack { artboard: artboard_id, animation: animation_id, node: node_id });
        self.document_summary()
    }

    /// Adds (or, at an already-keyframed `time`, replaces) a progress
    /// keyframe on `node`'s `MotionPath` track — the `MotionPath` counterpart
    /// to `add_keyframe`, taking an explicit `progress` (`0.0..=1.0` along
    /// the guide's own path) instead of capturing the node's current pose,
    /// since a motion-path track has no "current value" on the node itself
    /// to snapshot (see `read_property_value`'s `MotionPath` arm). `node`
    /// must already have a `MotionPath` track (via `set_motion_path_guide`)
    /// before this is called.
    pub fn add_motion_path_keyframe(&mut self, artboard: JsValue, node: JsValue, time: f32, progress: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let animation_id = self.active_animation.ok_or_else(|| JsValue::from_str("add_motion_path_keyframe: no active animation"))?;
        let artboard_ref = self
            .document
            .artboard(artboard_id)
            .ok_or_else(|| JsValue::from_str("add_motion_path_keyframe: unknown artboard"))?;
        let duration = artboard_ref.animation(animation_id).map(|a| a.duration).unwrap_or(0.0);
        let time = time.clamp(0.0, duration);
        let progress = progress.clamp(0.0, 1.0);

        self.document.apply(Command::AddKeyframe {
            artboard: artboard_id, animation: animation_id, node: node_id, time,
            value: KeyframeValue::MotionPath(progress), easing: Easing::Linear,
        });
        self.document_summary()
    }

    /// Sets the easing curve a keyframe uses to interpolate *out of* itself
    /// toward the next one — the Curve Editor panel's save action, opened by
    /// double-clicking a keyframe diamond in the Timeline. `index` addresses
    /// the keyframe the same way `preview_keyframe_time`/`commit_keyframe_time`
    /// already do (its position in that property's `PropertyTrackSummary::keyframes`,
    /// always in ascending-time order). Re-issues `Command::AddKeyframe` at
    /// that keyframe's own unchanged `time`/`value` with the new `easing` —
    /// `Command::AddKeyframe`'s own doc comment already establishes that
    /// re-adding at an existing time *replaces* that keyframe in place, so no
    /// dedicated `Command` variant is needed just for this.
    pub fn set_keyframe_easing(&mut self, artboard: JsValue, node: JsValue, property: &str, index: usize, easing: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let property_kind = parse_property_kind(property)?;
        let animation_id = self.active_animation.ok_or_else(|| JsValue::from_str("set_keyframe_easing: no active animation"))?;
        let easing_dto: crate::dto::EasingDto =
            serde_wasm_bindgen::from_value(easing).map_err(|err| JsValue::from_str(&format!("invalid easing: {err}")))?;
        let easing: Easing = easing_dto.into();

        let artboard_ref = self
            .document
            .artboard(artboard_id)
            .ok_or_else(|| JsValue::from_str("set_keyframe_easing: unknown artboard"))?;
        let animation = artboard_ref
            .animation(animation_id)
            .ok_or_else(|| JsValue::from_str("set_keyframe_easing: unknown animation"))?;
        let (_, track) = animation
            .tracks
            .iter()
            .find(|(n, t)| *n == node_id && t.kind() == property_kind)
            .ok_or_else(|| JsValue::from_str("set_keyframe_easing: no track for this node/property"))?;

        let (time, value) = match track {
            PropertyTrack::Translation(t) => t.keyframes.get(index).map(|k| (k.time, KeyframeValue::Translation(k.value))),
            PropertyTrack::Rotation(t) => t.keyframes.get(index).map(|k| (k.time, KeyframeValue::Rotation(k.value))),
            PropertyTrack::Scale(t) => t.keyframes.get(index).map(|k| (k.time, KeyframeValue::Scale(k.value))),
            PropertyTrack::FillColor(t) => t.keyframes.get(index).map(|k| (k.time, KeyframeValue::FillColor(k.value))),
            PropertyTrack::Path(t) => t.keyframes.get(index).map(|k| (k.time, KeyframeValue::Path(k.value.clone()))),
            PropertyTrack::MotionPath { track: t, .. } => t.keyframes.get(index).map(|k| (k.time, KeyframeValue::MotionPath(k.value))),
            PropertyTrack::Field { path, track: t } => t.keyframes.get(index).map(|k| (k.time, KeyframeValue::Field(path.clone(), k.value))),
        }
        .ok_or_else(|| JsValue::from_str("set_keyframe_easing: no keyframe at that index"))?;

        self.document.apply(Command::AddKeyframe { artboard: artboard_id, animation: animation_id, node: node_id, time, value, easing });
        self.document_summary()
    }

    /// Removes the keyframe at exactly `time` from `node`'s `property` track
    /// within the active animation — the Timeline's "Delete Keyframe" action.
    pub fn remove_keyframe(&mut self, artboard: JsValue, node: JsValue, property: &str, time: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let property_kind = parse_property_kind(property)?;
        let animation_id = self.active_animation.ok_or_else(|| JsValue::from_str("remove_keyframe: no active animation"))?;
        self.document
            .apply(Command::RemoveKeyframe { artboard: artboard_id, animation: animation_id, node: node_id, property: property_kind, time });
        self.document_summary()
    }

    /// Live keyframe-drag preview: directly mutates the keyframe at array
    /// position `index` (within that property's `PropertyTrackSummary::keyframes`,
    /// always in ascending-time order) to `new_time`, bypassing `Command`/undo
    /// entirely — same untracked-preview reasoning as `preview_node_translation`.
    /// Deliberately does *not* re-sort the track's keyframes after this
    /// (unlike `Command::MoveKeyframe`'s own apply, which does) — re-sorting
    /// mid-drag would let `index` stop pointing at the keyframe actually
    /// being dragged the moment it crosses a neighbor's time, exactly the
    /// bug a stable index exists to avoid. This means the track can be
    /// transiently out of ascending-time order for the duration of a drag;
    /// nothing reads it (no `set_scrub_time` call, no render sampling) until
    /// `commit_keyframe_time` restores order below — a live-drag-only
    /// invariant relaxation, not a general one.
    pub fn preview_keyframe_time(&mut self, artboard: JsValue, node: JsValue, property: &str, index: usize, new_time: f32) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let property_kind = parse_property_kind(property)?;
        let animation_id = self.active_animation.ok_or_else(|| JsValue::from_str("preview_keyframe_time: no active animation"))?;

        let animation = self
            .document
            .artboard_mut(artboard_id)
            .and_then(|a| a.animation_mut(animation_id))
            .ok_or_else(|| JsValue::from_str("preview_keyframe_time: unknown artboard or animation"))?;
        // Clamped to the animation's fixed range — see `add_keyframe`'s doc
        // comment on the same invariant.
        let new_time = new_time.clamp(0.0, animation.duration);
        let (_, track) = animation
            .tracks
            .iter_mut()
            .find(|(n, t)| *n == node_id && t.kind() == property_kind)
            .ok_or_else(|| JsValue::from_str("preview_keyframe_time: no track for this node/property"))?;

        match track {
            PropertyTrack::Translation(t) => set_keyframe_time_at(t, index, new_time),
            PropertyTrack::Rotation(t) => set_keyframe_time_at(t, index, new_time),
            PropertyTrack::Scale(t) => set_keyframe_time_at(t, index, new_time),
            PropertyTrack::FillColor(t) => set_keyframe_time_at(t, index, new_time),
            PropertyTrack::Path(t) => set_keyframe_time_at(t, index, new_time),
            PropertyTrack::MotionPath { track: t, .. } => set_keyframe_time_at(t, index, new_time),
            PropertyTrack::Field { track: t, .. } => set_keyframe_time_at(t, index, new_time),
        }
    }

    /// Ends a keyframe-drag gesture as exactly one undoable edit.
    /// `origin_time` is the keyframe's time from *before* the drag started —
    /// restoring it here first, then applying the real `Command::MoveKeyframe`,
    /// is what makes `Command::apply`'s "capture the current value as the
    /// inverse" convention correctly capture the pre-drag time rather than
    /// wherever `preview_keyframe_time` last left it — the exact same
    /// reasoning `commit_node_translation`'s doc comment gives for restoring
    /// origin before applying the real command.
    pub fn commit_keyframe_time(
        &mut self,
        artboard: JsValue,
        node: JsValue,
        property: &str,
        index: usize,
        origin_time: f32,
        new_time: f32,
    ) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let property_kind = parse_property_kind(property)?;
        let animation_id = self.active_animation.ok_or_else(|| JsValue::from_str("commit_keyframe_time: no active animation"))?;

        let new_time = {
            let animation = self
                .document
                .artboard_mut(artboard_id)
                .and_then(|a| a.animation_mut(animation_id))
                .ok_or_else(|| JsValue::from_str("commit_keyframe_time: unknown artboard or animation"))?;
            // Clamped to the animation's fixed range — see `add_keyframe`'s
            // doc comment on the same invariant.
            let new_time = new_time.clamp(0.0, animation.duration);
            let (_, track) = animation
                .tracks
                .iter_mut()
                .find(|(n, t)| *n == node_id && t.kind() == property_kind)
                .ok_or_else(|| JsValue::from_str("commit_keyframe_time: no track for this node/property"))?;
            match track {
                PropertyTrack::Translation(t) => set_keyframe_time_at(t, index, origin_time)?,
                PropertyTrack::Rotation(t) => set_keyframe_time_at(t, index, origin_time)?,
                PropertyTrack::Scale(t) => set_keyframe_time_at(t, index, origin_time)?,
                PropertyTrack::FillColor(t) => set_keyframe_time_at(t, index, origin_time)?,
                PropertyTrack::Path(t) => set_keyframe_time_at(t, index, origin_time)?,
                PropertyTrack::MotionPath { track: t, .. } => set_keyframe_time_at(t, index, origin_time)?,
                PropertyTrack::Field { track: t, .. } => set_keyframe_time_at(t, index, origin_time)?,
            }
            new_time
        };
        self.document.apply(Command::MoveKeyframe {
            artboard: artboard_id,
            animation: animation_id,
            node: node_id,
            property: property_kind,
            old_time: origin_time,
            new_time,
        });
        self.document_summary()
    }

    /// Scrubs the active animation to `time` — the Timeline's playhead-drag
    /// preview. Directly overwrites every keyframed node's transform/fill
    /// color (untracked, like `preview_node_translation`), *not* re-applied
    /// every rendered frame: scrubbing is a discrete "seek to this static
    /// time" action, not continuous playback, so this only needs to run
    /// once per scrub position change — unlike `state_machine_active`'s
    /// continuous per-`render()`-call advancement, there's no wall-clock
    /// time to integrate here. This deliberately lets an on-canvas drag
    /// afterward (still going through `preview_node_translation`/
    /// `commit_node_translation` unchanged) stick without a subsequent frame
    /// snapping it back — it would, if this ran every frame instead of only
    /// on an explicit scrub.
    pub fn set_scrub_time(&mut self, artboard: JsValue, time: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        self.scrub_time = time;
        self.resample_active_animation(artboard_id);
        self.document_summary()
    }

    /// `artboard`/`node` are `{idx, version}` objects — the frontend's
    /// `OpaqueId` shape, echoed back verbatim from a prior `document_summary()`
    /// response, exactly as it always was over Tauri IPC. Goes through
    /// `Document::apply`/`Command`, same as the old Tauri command did — a
    /// real, undoable edit, never a direct field write. See
    /// `apply_transform_edit`'s doc comment for why this can become a
    /// keyframe overwrite instead of a plain `SetTranslation` when the
    /// playhead sits exactly on one of this node's own keyframes.
    pub fn set_node_translation(&mut self, artboard: JsValue, node: JsValue, x: f32, y: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let value = Vec2::new(x, y);
        self.apply_transform_edit(
            artboard_id,
            node_id,
            PropertyKind::Translation,
            KeyframeValue::Translation(value),
            Command::SetTranslation { artboard: artboard_id, node: node_id, value },
        );
        self.document_summary()
    }

    pub fn set_node_rotation(&mut self, artboard: JsValue, node: JsValue, degrees: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let value = degrees.to_radians();
        self.apply_transform_edit(
            artboard_id,
            node_id,
            PropertyKind::Rotation,
            KeyframeValue::Rotation(value),
            Command::SetRotation { artboard: artboard_id, node: node_id, value },
        );
        self.document_summary()
    }

    pub fn set_node_scale(&mut self, artboard: JsValue, node: JsValue, x: f32, y: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let value = Vec2::new(x, y);
        self.apply_transform_edit(
            artboard_id,
            node_id,
            PropertyKind::Scale,
            KeyframeValue::Scale(value),
            Command::SetScale { artboard: artboard_id, node: node_id, value },
        );
        self.document_summary()
    }

    /// The Inspector's plain Anchor X/Y fields — a raw field edit, **not**
    /// compensated (unlike `commit_node_anchor`, the on-canvas drag handle's
    /// commit). This deliberately matches After Effects' own split: typing a
    /// new Anchor Point value directly in a properties panel visibly shifts
    /// the layer (you changed which local point `translation` refers to),
    /// while dragging the dedicated Pan-Behind/anchor tool *in the canvas*
    /// keeps the layer visually fixed. No `apply_transform_edit`/keyframe
    /// involvement — the anchor isn't an animatable `PropertyKind` this pass
    /// (see `Command::SetAnchor`'s doc comment), so this is a plain
    /// `Command::apply`, same shape as `set_node_translation` minus the
    /// keyframe-overwrite branch.
    pub fn set_node_anchor(&mut self, artboard: JsValue, node: JsValue, x: f32, y: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        self.document
            .apply(Command::SetAnchor { artboard: artboard_id, node: node_id, value: Vec2::new(x, y) });
        self.document_summary()
    }

    /// Live preview while dragging the on-canvas anchor handle — sets
    /// anchor *and* translation directly (no undo), same "both fields move
    /// together" shape `preview_node_scale` already has for its own
    /// corner-pin drag. The frontend computes both values per mousemove via
    /// `Transform2D::with_anchor_preserving_world`'s formula (ported to TS
    /// in `TransformHandles.tsx`) so the shape's rendered geometry stays
    /// visually fixed while the anchor marker tracks the cursor.
    pub fn preview_node_anchor(&mut self, artboard: JsValue, node: JsValue, anchor_x: f32, anchor_y: f32, x: f32, y: f32) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let artboard_ref = self
            .document
            .artboard_mut(artboard_id)
            .ok_or_else(|| JsValue::from_str("preview_node_anchor: unknown artboard"))?;
        let node_ref = artboard_ref
            .node_mut(node_id)
            .ok_or_else(|| JsValue::from_str("preview_node_anchor: unknown node"))?;
        node_ref.transform.anchor = Vec2::new(anchor_x, anchor_y);
        node_ref.transform.translation = Vec2::new(x, y);
        Ok(())
    }

    /// Ends an anchor-handle drag as one undoable edit (a `Batch` of
    /// `SetAnchor`+`SetTranslation`, same "restore origin first, then apply
    /// the real command" shape `commit_node_scale` already uses) — unlike
    /// the Inspector's plain `set_node_anchor`, this one *does* keep the
    /// shape's rendered geometry fixed (the whole point of dragging the
    /// pivot directly in the canvas, per `set_node_anchor`'s own doc
    /// comment on the After Effects convention this mirrors).
    #[allow(clippy::too_many_arguments)]
    pub fn commit_node_anchor(
        &mut self,
        artboard: JsValue,
        node: JsValue,
        origin_anchor_x: f32,
        origin_anchor_y: f32,
        origin_x: f32,
        origin_y: f32,
        anchor_x: f32,
        anchor_y: f32,
        x: f32,
        y: f32,
    ) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        {
            let artboard_ref = self
                .document
                .artboard_mut(artboard_id)
                .ok_or_else(|| JsValue::from_str("commit_node_anchor: unknown artboard"))?;
            let node_ref = artboard_ref
                .node_mut(node_id)
                .ok_or_else(|| JsValue::from_str("commit_node_anchor: unknown node"))?;
            node_ref.transform.anchor = Vec2::new(origin_anchor_x, origin_anchor_y);
            node_ref.transform.translation = Vec2::new(origin_x, origin_y);
        }

        let translation_changed = (x - origin_x).abs() > f32::EPSILON || (y - origin_y).abs() > f32::EPSILON;
        let mut commands = vec![Command::SetAnchor { artboard: artboard_id, node: node_id, value: Vec2::new(anchor_x, anchor_y) }];
        if translation_changed {
            commands.push(Command::SetTranslation { artboard: artboard_id, node: node_id, value: Vec2::new(x, y) });
        }
        let final_command = if commands.len() == 1 { commands.pop().unwrap() } else { Command::Batch(commands) };
        self.document.apply(final_command);
        self.document_summary()
    }

    /// Hit-tests `x`/`y` (artboard-space coordinates, same convention as
    /// every other pointer-facing method in this workspace: origin centered,
    /// +y up) against every shape's local-space path bounds
    /// (`an1mbyt3_core::Path::bounds`), walking shapes in reverse draw
    /// order so a node drawn on top of another wins the hit — `Artboard::shapes()`
    /// yields root-plus-nested shapes in insertion/draw order, later entries
    /// draw on top, same convention `Artboard::root_nodes`'s doc comment
    /// establishes. Returns the hit node's id (the frontend's `OpaqueId`
    /// shape) or `null` if the point hits nothing, so the frontend can
    /// deselect on an empty-canvas click.
    pub fn pick_node_at(&self, artboard: JsValue, x: f32, y: f32) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let artboard_ref = self
            .document
            .artboard(artboard_id)
            .ok_or_else(|| JsValue::from_str("pick_node_at: unknown artboard"))?;

        let point = Vec2::new(x, y);
        // A single, unified walk in real draw order (`Artboard::pickable_nodes`),
        // checked topmost-first (`.rev()`) — a `Shape` drawn on top of an
        // `Image` (or vice versa) now wins picking the same way it wins
        // rendering. Previously this checked every `Image` first, then every
        // `Shape`, as two separate passes — an `Image` always beat a `Shape`
        // regardless of which one actually drew on top, since that split
        // predates `shapes`/`texts`/`images` being unified onto one shared
        // draw-order walk. `Shape`/`Image`/`Text` (its glyph box) and gizmo-only
        // objects have a bounds check here; `Bone`/`Instance` and plain
        // groups are picked through their content or the Layers panel.
        // Locked reference objects (and what's inside them) let clicks pass
        // through — they're picked in the Layers panel.
        let locked_reference = |mut id: NodeId| loop {
            let Some(n) = artboard_ref.node(id) else { return false };
            if n.reference().is_some_and(|r| r.locked) {
                return true;
            }
            match n.parent {
                Some(p) => id = p,
                None => return false,
            }
        };
        let hit = artboard_ref.pickable_nodes().filter(|(id, _)| !locked_reference(*id)).collect::<Vec<_>>().into_iter().rev().find_map(|(node_id, node)| {
            let (min, max) = match &node.kind {
                NodeKind::Shape(shape) => shape.path.bounds(),
                NodeKind::Image(image) => (Vec2::new(-image.width / 2.0, -image.height / 2.0), Vec2::new(image.width / 2.0, image.height / 2.0)),
                // Text: its measured glyph box (the selection box the
                // canvas shows), padded a little so thin or short text is
                // still easy to grab.
                NodeKind::Text(text) => {
                    let (min, max) = crate::dto::local_bounds(artboard_ref, node_id);
                    let pad = Vec2::splat((text.size_px * 0.15).clamp(2.0, 8.0));
                    (min - pad, max + pad)
                }
                // A camera's gizmo icon — see `CAMERA_GIZMO_HALF_EXTENT`.
                // Component-only objects (an empty GameObject carrying a
                // Camera / Light 2D / Trail / Particle System / collider) are
                // picked by a small gizmo at their center — a camera by its
                // camera icon, a light by its bulb (its whole radius would
                // otherwise swallow clicks meant for what it shines on).
                NodeKind::Group(_) if node.camera().is_some() => (Vec2::splat(-crate::dto::CAMERA_GIZMO_HALF_EXTENT), Vec2::splat(crate::dto::CAMERA_GIZMO_HALF_EXTENT)),
                NodeKind::Group(_) if !node.behaviors.is_empty() => (Vec2::splat(-crate::dto::SMALL_GIZMO_HALF_EXTENT), Vec2::splat(crate::dto::SMALL_GIZMO_HALF_EXTENT)),
                _ => return None,
            };
            let local = artboard_ref.world_transform(node_id).to_affine2().inverse().transform_point2(point);
            (local.x >= min.x && local.x <= max.x && local.y >= min.y && local.y <= max.y).then_some(node_id)
        });

        match hit {
            Some(node_id) => to_js_value(&node_id),
            None => Ok(JsValue::NULL),
        }
    }

    /// Live drag preview: directly mutates the node's translation, bypassing
    /// `Document::apply`/`Command` entirely — deliberately untracked, so
    /// dragging a node across many `mousemove` events doesn't push a
    /// separate undo entry per event. `render()` picks up the new position
    /// on its next frame; no `document_summary()` call here, since a preview
    /// frame isn't a committed edit the rest of the UI needs to react to yet
    /// (see `commit_node_translation` for the single real, undoable edit a
    /// whole drag gesture produces).
    pub fn preview_node_translation(&mut self, artboard: JsValue, node: JsValue, x: f32, y: f32) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let artboard_ref = self
            .document
            .artboard_mut(artboard_id)
            .ok_or_else(|| JsValue::from_str("preview_node_translation: unknown artboard"))?;
        let node_ref = artboard_ref
            .node_mut(node_id)
            .ok_or_else(|| JsValue::from_str("preview_node_translation: unknown node"))?;
        node_ref.transform.translation = Vec2::new(x, y);
        Ok(())
    }

    /// Ends a drag gesture as exactly one undoable edit. `origin_x`/`origin_y`
    /// is the node's translation from *before* the drag started (captured by
    /// the frontend at the first `pick_node_at` hit).
    ///
    /// Same mode-gated priority `apply_transform_edit` uses (see its doc
    /// comment for why the Animate/Design split matters):
    /// - **In Animate mode**, if the playhead sits exactly on one of this
    ///   node's own `Translation` keyframes (`keyframe_at_scrub_time`), the
    ///   drag overwrites *that keyframe's value* instead of issuing a plain
    ///   `Command::SetTranslation` — the live preview already left
    ///   `transform.translation` at the dragged-to position, so nothing else
    ///   needs updating, only the authored keyframe data. Otherwise (still
    ///   Animate mode, not parked on a keyframe — positioning for a new one),
    ///   a plain `SetTranslation` alone.
    /// - **Outside Animate mode**, restores the node to `origin_x`/`origin_y`
    ///   first (so `Command::apply`'s "capture the current value as the
    ///   inverse" convention correctly captures the *pre-drag* position, not
    ///   whatever `preview_node_translation` last wrote), then bundles the
    ///   real `Command::SetTranslation` with a `Command::ShiftPropertyKeyframes`
    ///   (if this node already has a `Translation` track anywhere: rebase its
    ///   whole animated path by the drag's delta, per "animate relative to
    ///   its new position") or applies `SetTranslation` alone (no existing
    ///   keyframes to rebase).
    pub fn commit_node_translation(
        &mut self,
        artboard: JsValue,
        node: JsValue,
        origin_x: f32,
        origin_y: f32,
        x: f32,
        y: f32,
    ) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;

        if self.animate_mode_active && self.keyframe_at_scrub_time(artboard_id, node_id, PropertyKind::Translation) {
            let animation_id = self.active_animation.expect("keyframe_at_scrub_time only returns true when Some");
            self.document.apply(Command::AddKeyframe {
                artboard: artboard_id,
                animation: animation_id,
                node: node_id,
                time: self.scrub_time,
                value: KeyframeValue::Translation(Vec2::new(x, y)),
                easing: Easing::Linear,
            });
        } else if self.animate_mode_active {
            self.document
                .apply(Command::SetTranslation { artboard: artboard_id, node: node_id, value: Vec2::new(x, y) });
        } else {
            {
                let artboard_ref = self
                    .document
                    .artboard_mut(artboard_id)
                    .ok_or_else(|| JsValue::from_str("commit_node_translation: unknown artboard"))?;
                let node_ref = artboard_ref
                    .node_mut(node_id)
                    .ok_or_else(|| JsValue::from_str("commit_node_translation: unknown node"))?;
                node_ref.transform.translation = Vec2::new(origin_x, origin_y);
            }
            let new_value = KeyframeValue::Translation(Vec2::new(x, y));
            let plain_command = Command::SetTranslation { artboard: artboard_id, node: node_id, value: Vec2::new(x, y) };
            if let Some(delta) = self.keyframed_property_delta(artboard_id, node_id, PropertyKind::Translation, new_value) {
                self.document.apply(Command::Batch(vec![
                    plain_command,
                    Command::ShiftPropertyKeyframes { artboard: artboard_id, node: node_id, property: PropertyKind::Translation, delta },
                ]));
            } else {
                self.document.apply(plain_command);
            }
        }
        self.document_summary()
    }

    /// Live preview while dragging the on-canvas transform tool's rotate
    /// handle — sets rotation directly, no undo entry, same reasoning as
    /// `preview_node_translation`. Degrees, converted to radians to match
    /// `set_node_rotation`'s convention.
    pub fn preview_node_rotation(&mut self, artboard: JsValue, node: JsValue, degrees: f32) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let artboard_ref = self
            .document
            .artboard_mut(artboard_id)
            .ok_or_else(|| JsValue::from_str("preview_node_rotation: unknown artboard"))?;
        let node_ref = artboard_ref
            .node_mut(node_id)
            .ok_or_else(|| JsValue::from_str("preview_node_rotation: unknown node"))?;
        node_ref.transform.rotation = degrees.to_radians();
        Ok(())
    }

    /// Ends a rotate-handle drag as exactly one undoable edit. Unlike
    /// `commit_node_translation`, `origin_degrees` is restored
    /// unconditionally before branching (not just in the Design-mode
    /// branch) — a rotate-only drag never needs the Animate-mode branch's
    /// "value's already live, don't bother restoring" shortcut to hold, so
    /// there's no reason to special-case it out.
    ///
    /// `compensate_children` is the Alt-held "rotate this node only, not
    /// its children" gesture — see `compensating_child_commands`'s own doc
    /// comment. Computed *after* restoring `origin_degrees` above (so it
    /// reads each child's pre-drag world transform) but *before* actually
    /// applying `degrees`, and batched into the same undo step either way.
    pub fn commit_node_rotation(&mut self, artboard: JsValue, node: JsValue, origin_degrees: f32, degrees: f32, compensate_children: bool) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let (origin_translation, origin_scale, origin_anchor);
        {
            let artboard_ref = self
                .document
                .artboard_mut(artboard_id)
                .ok_or_else(|| JsValue::from_str("commit_node_rotation: unknown artboard"))?;
            let node_ref = artboard_ref
                .node_mut(node_id)
                .ok_or_else(|| JsValue::from_str("commit_node_rotation: unknown node"))?;
            node_ref.transform.rotation = origin_degrees.to_radians();
            origin_translation = node_ref.transform.translation;
            origin_scale = node_ref.transform.scale;
            origin_anchor = node_ref.transform.anchor;
        }
        let value = degrees.to_radians();
        let mut commands = vec![self.build_transform_edit_command(
            artboard_id,
            node_id,
            PropertyKind::Rotation,
            KeyframeValue::Rotation(value),
            Command::SetRotation { artboard: artboard_id, node: node_id, value },
        )];
        if compensate_children {
            let new_local = an1mbyt3_core::Transform2D { translation: origin_translation, rotation: value, scale: origin_scale, anchor: origin_anchor };
            commands.extend(self.compensating_child_commands(artboard_id, node_id, new_local));
        }
        let final_command = if commands.len() == 1 { commands.pop().unwrap() } else { Command::Batch(commands) };
        self.document.apply(final_command);
        self.document_summary()
    }

    /// The project's sorting layers, back to front: `[{ id, name }]`.
    pub fn sorting_layers(&self) -> Result<JsValue, JsValue> {
        #[derive(serde::Serialize)]
        struct L {
            id: u32,
            name: String,
        }
        to_js_value(&self.document.sorting_layers().into_iter().map(|l| L { id: l.id, name: l.name }).collect::<Vec<_>>())
    }

    /// Replaces the sorting layers (`[{ id, name }]`, back to front).
    pub fn set_sorting_layers(&mut self, layers: JsValue) -> Result<JsValue, JsValue> {
        #[derive(serde::Deserialize)]
        struct L {
            id: u32,
            name: String,
        }
        let layers: Vec<L> = parse_id(layers)?;
        self.document.set_sorting_layers(layers.into_iter().map(|l| an1mbyt3_core::SortingLayerDef { id: l.id, name: l.name }).collect());
        self.document_summary()
    }

    /// Script API (`ref.shake2(amount)`): kicks a camera's Camera Shake
    /// (adds `amount` 0..1 of trauma).
    pub fn camera_shake(&mut self, node: JsValue, amount: f32) -> Result<(), JsValue> {
        let node_id: NodeId = parse_id(node)?;
        let t = self.camera_trauma.entry(node_id).or_insert(0.0);
        *t = (*t + amount.max(0.0)).min(1.0);
        Ok(())
    }

    /// Align & distribute: moves each of `nodes` by its own world-space
    /// `deltas` (`[dx, dy, dx, dy, …]`), as one undo step.
    pub fn translate_nodes(&mut self, artboard: JsValue, nodes: JsValue, deltas: Vec<f32>) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let nodes: Vec<NodeId> = parse_id(nodes)?;
        let moves: Vec<(NodeId, Vec2)> = {
            let Some(art) = self.document.artboard(artboard_id) else { return self.document_summary() };
            nodes
                .iter()
                .enumerate()
                .filter_map(|(i, id)| {
                    let d = Vec2::new(*deltas.get(i * 2)?, *deltas.get(i * 2 + 1)?);
                    if d.length_squared() < 1e-10 {
                        return None;
                    }
                    let node = art.node(*id)?;
                    let parent = node.parent.and_then(|p| art.try_world_transform(p)).map_or(glam::Affine2::IDENTITY, |t| t.to_affine2());
                    Some((*id, node.transform.translation + parent.matrix2.inverse() * d))
                })
                .collect()
        };
        let count = moves.len();
        for (id, value) in moves {
            self.document.apply(Command::SetTranslation { artboard: artboard_id, node: id, value });
        }
        if count > 1 {
            self.document.merge_last(count);
        }
        self.document_summary()
    }

    /// Multi-selection transform (the multi-select box's scale / rotate
    /// handles), step 1: remembers where every node in `nodes` starts.
    /// Nodes whose ancestor is also selected are left out (they ride along
    /// with it). Then `multi_transform_preview` per pointer move and
    /// `multi_transform_end` on release.
    pub fn multi_transform_begin(&mut self, artboard: JsValue, nodes: JsValue) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let nodes: Vec<NodeId> = parse_id(nodes)?;
        let a = self.document.artboard(artboard_id).ok_or_else(|| JsValue::from_str("multi_transform_begin: unknown artboard"))?;
        let selected: std::collections::HashSet<NodeId> = nodes.iter().copied().collect();
        let covered = |id: NodeId| {
            let mut at = a.node(id).and_then(|n| n.parent);
            while let Some(p) = at {
                if selected.contains(&p) {
                    return true;
                }
                at = a.node(p).and_then(|n| n.parent);
            }
            false
        };
        let base = nodes.iter().filter(|id| !covered(**id)).filter_map(|id| Some((*id, a.node(*id)?.transform))).collect();
        self.multi_transform = Some((artboard_id, base));
        Ok(())
    }

    /// Step 2: every node gets the same world-space change `delta` (a 2D
    /// affine as `[a, b, c, d, tx, ty]`: x axis `(a, b)`, y axis `(c, d)`,
    /// translation) on top of where it started — a live preview, no undo.
    /// Each node keeps its own anchor; skew (from scaling a rotated node
    /// unevenly) is dropped.
    pub fn multi_transform_preview(&mut self, a: f32, b: f32, c: f32, d: f32, tx: f32, ty: f32) -> Result<(), JsValue> {
        let Some((artboard_id, base)) = self.multi_transform.clone() else { return Ok(()) };
        let delta = glam::Affine2::from_cols(Vec2::new(a, b), Vec2::new(c, d), Vec2::new(tx, ty));
        let updates: Vec<(NodeId, an1mbyt3_core::Transform2D)> = {
            let Some(art) = self.document.artboard(artboard_id) else { return Ok(()) };
            base.iter()
                .map(|(id, start)| {
                    let parent = art.node(*id).and_then(|n| n.parent).and_then(|p| art.try_world_transform(p)).map_or(glam::Affine2::IDENTITY, |t| t.to_affine2());
                    let world = delta * parent * start.to_affine2();
                    // Back to local, with the anchor folded out: M = T·R·S.
                    let m = parent.inverse() * world * glam::Affine2::from_translation(start.anchor);
                    let x = m.matrix2.x_axis;
                    let y = m.matrix2.y_axis;
                    let sx = x.length().max(1e-6);
                    let sy = (x.x * y.y - x.y * y.x) / sx;
                    (*id, an1mbyt3_core::Transform2D { translation: m.translation, rotation: x.y.atan2(x.x), scale: Vec2::new(sx, sy), anchor: start.anchor })
                })
                .collect()
        };
        if let Some(art) = self.document.artboard_mut(artboard_id) {
            for (id, t) in updates {
                if let Some(n) = art.node_mut(id) {
                    n.transform = t;
                }
            }
        }
        Ok(())
    }

    /// Step 3: on release, `commit` keeps the result as one undoable edit
    /// (translation, rotation and scale of every node); otherwise every
    /// node goes back to where it started. Returns the document summary.
    pub fn multi_transform_end(&mut self, commit: bool) -> Result<JsValue, JsValue> {
        let Some((artboard_id, base)) = self.multi_transform.take() else { return self.document_summary() };
        let mut finals = Vec::new();
        if let Some(art) = self.document.artboard_mut(artboard_id) {
            for (id, start) in &base {
                if let Some(n) = art.node_mut(*id) {
                    finals.push((*id, n.transform));
                    n.transform = *start;
                }
            }
        }
        if commit {
            let mut inverses = Vec::new();
            for (id, t) in finals {
                let Some(start) = base.iter().find(|(b, _)| *b == id).map(|(_, s)| *s) else { continue };
                if t.translation != start.translation {
                    inverses.push(self.document.apply(Command::SetTranslation { artboard: artboard_id, node: id, value: t.translation }));
                }
                if t.rotation != start.rotation {
                    inverses.push(self.document.apply(Command::SetRotation { artboard: artboard_id, node: id, value: t.rotation }));
                }
                if t.scale != start.scale {
                    inverses.push(self.document.apply(Command::SetScale { artboard: artboard_id, node: id, value: t.scale }));
                }
            }
            if !inverses.is_empty() {
                // `apply` pushed one undo entry per command: fold them into one.
                self.document.merge_last(inverses.len());
            }
        }
        self.document_summary()
    }

    /// Live preview while dragging an on-canvas scale handle — sets
    /// translation *and* scale directly (no undo). Translation moves too,
    /// not just scale: the transform tool's corner handles scale around the
    /// *opposite* corner (matching Figma/Illustrator's default, unmodified
    /// corner-drag), and keeping that corner fixed in world space while the
    /// node grows/shrinks means the origin itself has to move — see
    /// `TransformHandles.tsx`'s corner-drag math for the derivation.
    pub fn preview_node_scale(&mut self, artboard: JsValue, node: JsValue, x: f32, y: f32, scale_x: f32, scale_y: f32) -> Result<(), JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let artboard_ref = self
            .document
            .artboard_mut(artboard_id)
            .ok_or_else(|| JsValue::from_str("preview_node_scale: unknown artboard"))?;
        let node_ref = artboard_ref
            .node_mut(node_id)
            .ok_or_else(|| JsValue::from_str("preview_node_scale: unknown node"))?;
        node_ref.transform.translation = Vec2::new(x, y);
        node_ref.transform.scale = Vec2::new(scale_x, scale_y);
        Ok(())
    }

    /// Ends a scale-handle drag as one undoable edit — two, batched
    /// together, if the anchor-corner math also moved translation (see
    /// `preview_node_scale`'s doc comment). Both `origin_*` values are
    /// restored unconditionally first, same reasoning as
    /// `commit_node_rotation`. Translation is only included in the result if
    /// it actually changed — a corner drag always moves it, but skipping an
    /// unconditional `SetTranslation` when it *didn't* move avoids a no-op
    /// undo entry.
    ///
    /// `compensate_children` is the Alt-held "resize this node only, not its
    /// children" gesture — see `compensating_child_commands`'s own doc
    /// comment; same "computed after restoring origin state, before
    /// applying the real edit, batched into the same undo step" shape
    /// `commit_node_rotation` uses.
    #[allow(clippy::too_many_arguments)]
    pub fn commit_node_scale(
        &mut self,
        artboard: JsValue,
        node: JsValue,
        origin_x: f32,
        origin_y: f32,
        origin_scale_x: f32,
        origin_scale_y: f32,
        x: f32,
        y: f32,
        scale_x: f32,
        scale_y: f32,
        compensate_children: bool,
    ) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let node_id: NodeId = parse_id(node)?;
        let (origin_rotation, origin_anchor);
        {
            let artboard_ref = self
                .document
                .artboard_mut(artboard_id)
                .ok_or_else(|| JsValue::from_str("commit_node_scale: unknown artboard"))?;
            let node_ref = artboard_ref
                .node_mut(node_id)
                .ok_or_else(|| JsValue::from_str("commit_node_scale: unknown node"))?;
            node_ref.transform.translation = Vec2::new(origin_x, origin_y);
            node_ref.transform.scale = Vec2::new(origin_scale_x, origin_scale_y);
            origin_rotation = node_ref.transform.rotation;
            origin_anchor = node_ref.transform.anchor;
        }

        let translation_changed = (x - origin_x).abs() > f32::EPSILON || (y - origin_y).abs() > f32::EPSILON;
        let mut commands = Vec::new();
        if translation_changed {
            commands.push(self.build_transform_edit_command(
                artboard_id,
                node_id,
                PropertyKind::Translation,
                KeyframeValue::Translation(Vec2::new(x, y)),
                Command::SetTranslation { artboard: artboard_id, node: node_id, value: Vec2::new(x, y) },
            ));
        }
        commands.push(self.build_transform_edit_command(
            artboard_id,
            node_id,
            PropertyKind::Scale,
            KeyframeValue::Scale(Vec2::new(scale_x, scale_y)),
            Command::SetScale { artboard: artboard_id, node: node_id, value: Vec2::new(scale_x, scale_y) },
        ));
        if compensate_children {
            let new_local = an1mbyt3_core::Transform2D {
                translation: Vec2::new(x, y),
                rotation: origin_rotation,
                scale: Vec2::new(scale_x, scale_y),
                anchor: origin_anchor,
            };
            commands.extend(self.compensating_child_commands(artboard_id, node_id, new_local));
        }

        let final_command = if commands.len() == 1 { commands.pop().unwrap() } else { Command::Batch(commands) };
        self.document.apply(final_command);
        self.document_summary()
    }

    pub fn undo(&mut self) -> Result<JsValue, JsValue> {
        self.document.undo();
        let artboard_id = self.current_artboard_id();
        if let Some(artboard_id) = artboard_id {
            self.resample_active_animation(artboard_id);
            self.refresh_state_machine_preview(artboard_id);
        }
        self.document_summary()
    }

    pub fn redo(&mut self) -> Result<JsValue, JsValue> {
        self.document.redo();
        let artboard_id = self.current_artboard_id();
        if let Some(artboard_id) = artboard_id {
            self.resample_active_animation(artboard_id);
            self.refresh_state_machine_preview(artboard_id);
        }
        self.document_summary()
    }

    /// Toggles whether `render` drives the demo state machine — called by
    /// the frontend whenever the toolbar's mode switches to/from "State
    /// Machine" (`App.tsx`). Entering the State Machine tab intentionally
    /// takes over the target node's pose for preview purposes (the same way
    /// any state-machine authoring tool's preview mode differs from its
    /// static edit mode) — that's a real mode switch, not a bug, even though
    /// it can visibly "jump" the node from wherever Design-mode editing left
    /// it to wherever the state machine's current state/time says it should be.
    pub fn set_state_machine_active(&mut self, active: bool) {
        self.state_machine_active = active;
    }

    /// Fires a named trigger input — e.g. the State Machine panel's toggle
    /// button reaching exactly the same `Inputs::fire_trigger` the native
    /// `state_machine` example's Space key reaches. Only takes effect on the
    /// *next* `render` call (that's where `StateMachine::advance` actually
    /// checks fired triggers), same one-`advance`-later timing every other
    /// trigger-driven demo in this workspace already has.
    pub fn fire_trigger(&mut self, name: &str) {
        self.state_machine.inputs.fire_trigger(name);
    }

    /// Sets a named boolean input on the live preview instance — the State
    /// Machine panel's live-test controls. Same "only takes effect on the
    /// next `render`" timing as `fire_trigger`.
    pub fn set_state_machine_bool_input(&mut self, name: &str, value: bool) {
        self.state_machine.inputs.set_bool(name, value);
    }

    /// Sets a named numeric input — same timing as `set_state_machine_bool_input`.
    /// A Bool input's current value (`false` when it was never set).
    pub fn state_machine_bool_input(&self, name: &str) -> bool {
        self.state_machine.inputs.bool(name)
    }

    pub fn set_state_machine_number_input(&mut self, name: &str, value: f32) {
        self.state_machine.inputs.set_number(name, value);
    }

    /// Sets a named enum-valued input — same timing as `set_state_machine_bool_input`.
    pub fn set_state_machine_enum_input(&mut self, name: &str, value: &str) {
        self.state_machine.inputs.set_enum(name, value);
    }

    /// Forwards a real keyboard event into the live preview instance's
    /// `Blocks`-mode input surface (`KeyboardState`) — a Programmable
    /// State's "key [ ] pressed?" sensing reporter and "when key [ ]
    /// pressed" hat both read this. `key` is a `KeyboardEvent.key` value
    /// (lowercased by the caller), forwarded unconditionally regardless of
    /// whether anything currently reads it — same "always forward, gate at
    /// the read side" convention `set_state_machine_bool_input`/
    /// `programs_enabled` already establish; a key no active `Blocks`
    /// script checks is simply inert. Same one-`advance`-later timing as
    /// every other input setter here (this only affects the *next*
    /// `render` call's `StateMachine::apply`).
    pub fn set_key_down(&mut self, key: &str, pressed: bool) {
        self.state_machine.keys.set_down(key, pressed);
    }

    /// Creates a new state machine on `artboard` with one default state
    /// (`"State 1"`, looping, referencing `initial_animation`) and makes it
    /// the one this editor's State Machine tab previews/tests — the panel's
    /// "New State Machine" action. `initial_animation` has to already exist
    /// (created via the Timeline's own "New Animation" action first) since a
    /// state always references a real `Animation`, never an embedded clip
    /// (see `an1mbyt3_core::state_machine`'s module doc).
    pub fn add_state_machine(&mut self, artboard: JsValue, name: String, initial_animation: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let animation_id: AnimationId = parse_id(initial_animation)?;

        let def = StateMachineDef {
            name,
            inputs: Vec::new(),
            states: vec![StateDef { name: "State 1".into(), animation: animation_id, loop_mode: LoopMode::Loop, transitions: Vec::new(), position: Vec2::ZERO, blend: None, program: None }],
            initial_state: "State 1".into(),
            layers: Vec::new(),
            deleted: false,
        };

        let inverse = Command::AddStateMachine { artboard: artboard_id, state_machine: def };
        let Command::SetStateMachineDeleted { state_machine: new_id, .. } = self.document.apply(inverse) else {
            unreachable!("AddStateMachine's inverse is always a SetStateMachineDeleted")
        };
        self.active_state_machine = Some(new_id);
        self.refresh_state_machine_preview(artboard_id);
        self.document_summary()
    }

    /// The Shapes menu's "Load Geometry Dash Demo" action — builds
    /// `build_demo_geometry_dash`'s whole level/player/physics-register/
    /// Blocks-program state machine into `artboard` as one undoable edit,
    /// same "select it and switch to Playground" convenience
    /// the other demo loaders give. Space to jump, run into a spike
    /// to instantly respawn at the start, reach the end to get launched into
    /// the wall.
    pub fn load_geometry_dash_demo(&mut self, artboard: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id = build_demo_geometry_dash(&mut self.document, artboard_id);
        migrate_document(&mut self.document);
        self.active_state_machine = Some(state_machine_id);
        self.refresh_state_machine_preview(artboard_id);
        self.document_summary()
    }

    /// The Shapes menu's "Load Ninja Adventure Demo" action — builds
    /// `demo_ninja::build_demo_ninja_adventure`'s two new scenes (Village +
    /// House) as one undoable edit, bakes every sprite frame/tileset up
    /// front (same "never hitch on first draw" reasoning as
    /// the other demo loaders), and returns the Village's id so the frontend
    /// can switch to it (`setActiveArtboard`) before entering Playground.
    /// Unlike every earlier demo it doesn't build into the current scene —
    /// it *adds* scenes, so the author's own work is left untouched.
    /// Sprite Editor → "Automatic": every separate sprite on image asset
    /// `image`, as flat `[x, y, width, height, ...]` pixel boxes in reading
    /// order (see `an1mbyt3_image::alpha_islands`).
    pub fn auto_slice_image(&self, image: JsValue, alpha_min: u8, min_size: u32, merge_gap: u32) -> Result<Vec<u32>, JsValue> {
        let image_id: an1mbyt3_core::ImageId = parse_id(image)?;
        let asset = self.document.image(image_id).ok_or_else(|| JsValue::from_str("auto_slice_image: unknown image"))?;
        let boxes = an1mbyt3_image::alpha_islands(&asset.source, alpha_min, min_size, merge_gap).map_err(|e| JsValue::from_str(&format!("auto_slice_image: {e:?}")))?;
        Ok(boxes.into_iter().flatten().collect())
    }

    /// Sprite Editor → "Slice": crops `rects` (flat `[x, y, w, h, ...]`
    /// pixels) out of image asset `image` into new image assets named
    /// `names[i]` (same filter / processing as the sheet), registers them
    /// as one `SpriteGroup` called `group` (`frame_ms` per frame, so the
    /// frames also play as a flipbook), bakes them, and returns the
    /// document summary.
    pub async fn slice_image(&mut self, image: JsValue, rects: Vec<u32>, names: JsValue, group: String, frame_ms: u32) -> Result<JsValue, JsValue> {
        let image_id: an1mbyt3_core::ImageId = parse_id(image)?;
        let names: Vec<String> = serde_wasm_bindgen::from_value(names).unwrap_or_default();
        let (source, processing) = {
            let asset = self.document.image(image_id).ok_or_else(|| JsValue::from_str("slice_image: unknown image"))?;
            (asset.source.clone(), asset.processing.clone())
        };
        let boxes: Vec<[u32; 4]> = rects.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
        let crops = an1mbyt3_image::crop_rects(&source, &boxes).map_err(|e| JsValue::from_str(&format!("slice_image: {e:?}")))?;
        let mut frames = Vec::with_capacity(crops.len());
        for (i, (png, _, _)) in crops.into_iter().enumerate() {
            let name = names.get(i).cloned().unwrap_or_else(|| format!("{group} {i}"));
            let id = self.document.add_image(an1mbyt3_core::ImageAsset { name, source: png, processing: processing.clone(), file: None });
            frames.push(id);
        }
        if !frames.is_empty() {
            self.document.add_sprite_group(an1mbyt3_core::SpriteGroup { name: group, frames: frames.iter().map(|&id| (id, frame_ms.max(1))).collect() });
        }
        for id in frames {
            self.bake_and_cache_image(id).await;
        }
        self.document_summary()
    }

    /// The Toasty Lite components (`demo_lite`): `kind` is one of
    /// `heart`, `menu`, `theme`, `status`, `bell`, `download`, `copy`,
    /// `arrow`, `loader`, `toasty`. Adds that scene; returns its id.
    pub fn load_lite_component(&mut self, kind: &str) -> Result<JsValue, JsValue> {
        let demo = crate::demo_lite::build(&mut self.document, kind).ok_or_else(|| JsValue::from_str(&format!("unknown Lite component \"{kind}\"")))?;
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The docs' small original games (`demo_games`): `toast-catch`, `hop`;
    /// and the Script Lab scenes (`demo_lab`): `lab`, `lab3d`.
    pub fn load_docs_game(&mut self, kind: &str) -> Result<JsValue, JsValue> {
        // The docs' Script Lab scenes (`lab`, `lab3d`) come through here too.
        let scene = match crate::demo_games::build(&mut self.document, kind) {
            Some(demo) => demo.scene,
            None => crate::demo_lab::build(&mut self.document, kind).ok_or_else(|| JsValue::from_str(&format!("unknown game \"{kind}\"")))?.scene,
        };
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: scene })
    }

    pub async fn load_ninja_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_ninja::build_demo_ninja_adventure(&mut self.document);
        migrate_document(&mut self.document);
        for image_id in demo.images {
            self.bake_and_cache_image(image_id).await;
        }
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.village })
    }

    /// The + menu's Demos → "Raycaster FPS" — adds the Scratch-style fake-3D
    /// shooter's scene (`demo_fps`), bakes its art, and returns the scene
    /// id for the frontend to switch to before entering Playground.
    /// Physics Playground demo — Rigidbody 2D / Collider 2D components plus
    /// the script physics API. Adds its own scene (no images to bake).
    pub fn load_physics_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_physics::build_demo_physics(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "Dungeon Royale" — adds the card-battle tower
    /// defense scene (`demo_dungeon`), bakes its art, and returns the scene
    /// id for the frontend to switch to before entering Playground.
    pub async fn load_dungeon_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_dungeon::build_demo_dungeon(&mut self.document);
        migrate_document(&mut self.document);
        for image_id in demo.images {
            self.bake_and_cache_image(image_id).await;
        }
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "Sunset Tactics" — a top-down 5v5 tactical
    /// shooter on a Sunset recreation: shadow-cast vision, 5 agents with
    /// abilities and ults, bots that play their role. `vrm` (optional: the
    /// avatar file's bytes; the host fetches the bundled popi) dresses
    /// every agent in it, animated by the generated rifle clips; enemies get
    /// a red outline (Material Overrides on the avatar's own materials).
    pub async fn load_tactics_demo(&mut self, vrm: Option<Vec<u8>>) -> Result<JsValue, JsValue> {
        let parsed = match &vrm {
            Some(bytes) => Some(an1mbyt3_geometry::parse_gltf(bytes, &|_| None).map_err(|e| JsValue::from_str(&format!("load_tactics_demo: {e}")))?),
            None => None,
        };
        let avatar = match (vrm, &parsed) {
            (Some(source), Some(data)) if data.vrm.is_some() => {
                let target = an1mbyt3_geometry::retarget::HumanoidTarget::of(data);
                Some(crate::demo_tactics::Avatar3D {
                    source,
                    faces_neg_z: data.vrm.as_ref().is_some_and(|v| v.forward_z < 0.0),
                    hips_height: target.map_or(0.85, |t| t.hips_height),
                })
            }
            _ => None,
        };
        let demo = crate::demo_tactics::build_demo_tactics(&mut self.document, avatar);
        if let (Some((scene, model, enemies)), Some(parsed)) = (&demo.avatar, &parsed) {
            // The avatar's own materials, editable; enemies wear the same
            // ones with a red outline (Valorant's enemy highlight).
            self.extract_model_materials(*model, parsed, "Agent");
            let own: Vec<(u32, an1mbyt3_core::Material)> = self
                .document
                .materials()
                .filter_map(|(_, m)| match &m.kind {
                    an1mbyt3_core::MaterialKind::Toon(t) => t.source.filter(|s| s.model == *model).map(|s| (s.slot, m.clone())),
                    _ => None,
                })
                .collect();
            let mut slots = Vec::new();
            for (slot, mut material) in own {
                let lower = material.name.to_ascii_lowercase();
                // Eyes, brows, mouth and the face's own lines stay as they are.
                if ["eye", "face", "mouth", "brow", "lash", "teeth", "tongue"].iter().any(|k| lower.contains(k)) {
                    continue;
                }
                if let an1mbyt3_core::MaterialKind::Toon(t) = &mut material.kind {
                    t.outline = [1.0, 0.16, 0.2];
                    t.outline_width = (t.outline_width * 1.4).max(0.006);
                    t.source = None;
                }
                material.name = format!("Enemy · {}", material.name.split(" · ").last().unwrap_or("outfit"));
                if let Command::SetMaterialDeleted { material: id, .. } = self.document.apply(Command::AddMaterial { material }) {
                    slots.push(an1mbyt3_core::SlotMaterial { slot, material: Some(id) });
                }
            }
            for node in enemies {
                if let Some(n) = self.document.artboard(*scene).and_then(|a| a.node(*node)) {
                    let mut value = n.behaviors.clone();
                    value.push(an1mbyt3_core::NodeBehaviorEntry { behavior: an1mbyt3_core::NodeBehavior::MaterialSlots(an1mbyt3_core::MaterialSlots { slots: slots.clone() }), enabled: true });
                    self.document.apply(Command::SetNodeBehaviors { artboard: *scene, node: *node, value });
                }
            }
        }
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "Liquid Lab" — Liquid 2D: water with floating
    /// crates, a honey faucet, click to drop crates and stones.
    pub async fn load_liquid_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_liquid::build_demo_liquid(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "UI & Sound Lab" — anchored HUD text, a stack
    /// of UI Buttons, and synthesized sounds on click.
    pub async fn load_ui_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_ui::build_demo_ui(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "Responsive Layout" — a page that reflows for
    /// phones and desktops (UI Layout / Show When / Responsive screen).
    pub async fn load_responsive_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_responsive::build_demo_responsive(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "HTML UI Lab" — HTML + CSS UI in a scene,
    /// wired by data-trigger, by script (`ref.dom`) and anchored in the world.
    pub async fn load_html_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_html::build_demo_html(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "Voxel Craft" — a Minetest-style block sandbox:
    /// a generated block world (`Primitive3D::Voxels`), first-person
    /// dig / build / fly, hotbar + creative inventory in HTML.
    pub async fn load_voxel_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_voxel::build_demo_voxel(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// Adaptive resolution (the game's playback setting): `{ enabled,
    /// minScale }` — see `an1mbyt3_core::AdaptiveResolution`.
    pub fn adaptive_resolution(&self) -> Result<JsValue, JsValue> {
        let a = self.document.adaptive_resolution();
        to_js_value(&crate::dto::AdaptiveResolutionDto { enabled: a.enabled(), min_scale: a.min_scale() })
    }

    /// Sets the adaptive-resolution setting (saved with the project).
    pub fn set_adaptive_resolution(&mut self, enabled: bool, min_scale: f32) {
        let min_percent = (min_scale.clamp(0.4, 1.0) * 100.0).round() as u8;
        self.document.set_adaptive_resolution(an1mbyt3_core::AdaptiveResolution { disabled: !enabled, min_percent: if min_percent == 50 { 0 } else { min_percent } });
    }

    /// The renderer's internal resolution scale right now (0.4 … 1) — the
    /// host's adaptive-resolution controller drives it (`set_dynamic_scale`).
    pub fn dynamic_scale(&self) -> f32 {
        self.renderer.dynamic_scale()
    }

    /// Renders the scene and post effects at `scale` of native size
    /// (snapped to 5 % steps), upscaled smoothly. The host lowers it when
    /// frames run long (see `game/adaptiveResolution.ts`).
    pub fn set_dynamic_scale(&mut self, scale: f32) {
        self.renderer.set_dynamic_scale(scale);
    }

    /// The + menu's Demos → "Shatterline" — a Smash Hit-style rail shooter:
    /// throw chrome balls to break real fracturing glass (`Primitive3D::Glass`)
    /// down an endless corridor, ten zones, crystals and multiball.
    pub async fn load_shatter_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_shatter::build_demo_shatter(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "Fatal Clash" — a Mortal Kombat-style 3D
    /// fighting game: two VRM avatars (`vrm`: the avatar file's bytes; the
    /// host fetches the bundled one), ragdoll knockdowns, blood, rounds,
    /// FINISH HER / FATALITY, a CPU opponent or 2 players.
    pub async fn load_fight_demo(&mut self, vrm: Vec<u8>) -> Result<JsValue, JsValue> {
        let parsed = an1mbyt3_geometry::parse_gltf(&vrm, &|_| None).map_err(|e| JsValue::from_str(&format!("load_fight_demo: {e}")))?;
        let demo = crate::demo_fight::build_demo_fight(&mut self.document, vrm, "popi.vrm");
        // Popi's own MToon materials, editable.
        self.extract_model_materials(demo.model, &parsed, "popi");
        // Shadow Popi: the same outfit, her own skin (bronze) —
        // through Material Overrides on her skin slots only.
        let skins: Vec<(u32, an1mbyt3_core::Material)> = self
            .document
            .materials()
            .filter_map(|(_, m)| match &m.kind {
                an1mbyt3_core::MaterialKind::Toon(t) if m.name.contains("_SKIN") => t.source.filter(|s| s.model == demo.model).map(|s| (s.slot, m.clone())),
                _ => None,
            })
            .collect();
        let mut slots = Vec::new();
        for (slot, mut material) in skins {
            if let an1mbyt3_core::MaterialKind::Toon(t) = &mut material.kind {
                // A warm bronze skin (popi's is pale).
                t.base = [0.74, 0.52, 0.4, 1.0];
                t.shade = [0.5, 0.3, 0.26];
                t.source = None;
            }
            material.name = format!("Shadow Popi · {}", material.name.split(" · ").last().unwrap_or("skin"));
            if let Command::SetMaterialDeleted { material: id, .. } = self.document.apply(Command::AddMaterial { material }) {
                slots.push(an1mbyt3_core::SlotMaterial { slot, material: Some(id) });
            }
        }
        if let Some(node) = self.document.artboard(demo.scene).and_then(|a| a.node(demo.shadow)) {
            let mut value = node.behaviors.clone();
            value.push(an1mbyt3_core::NodeBehaviorEntry { behavior: an1mbyt3_core::NodeBehavior::MaterialSlots(an1mbyt3_core::MaterialSlots { slots }), enabled: true });
            self.document.apply(Command::SetNodeBehaviors { artboard: demo.scene, node: demo.shadow, value });
        }
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "Toon Explosion" — a cartoon explosion from
    /// layered particle systems (smoke, fire, outlines, sparks, debris,
    /// shockwave) plus a starburst flash, comic words and screen shake.
    pub async fn load_boom_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_boom::build_demo_boom(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "LAN Party" — port-based multiplayer: host
    /// on a port or join by address (`ctx.net`), toon blobs, host-owned coins.
    pub async fn load_lan_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_lan::build_demo_lan(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "Cloth Lab" — the Cloth 2D component: a flag,
    /// a curtain, a banner, a tearable sheet and a caped hero.
    pub async fn load_cloth_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_cloth::build_demo_cloth(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "Tree Garden" — the Tree 2D component: every
    /// species growing in, swaying, shaken by clicks.
    pub async fn load_trees_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_trees::build_demo_trees(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "Goo Shapes" — Goo 2D on text, shapes and
    /// particles: members of one group melt together (gradient fills).
    pub async fn load_goo_shapes_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_goo_shapes::build_demo_goo_shapes(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "Goo Lab" — 2D particles as liquid (Particle
    /// Fluid 2D) steered by behaviours (Particle Behaviours 2D).
    pub async fn load_goo_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_goo::build_demo_goo(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "Reyna's Leer" — 3D particles (swirling bands,
    /// smoke, converging sparks) round a see-through shader-graph eye.
    pub async fn load_leer_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_leer::build_demo_leer(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "Physics 3D Lab" — Rigidbody 3D / Collider 3D:
    /// a crate pyramid, a ramp, a trigger zone and a click-to-fire cannon.
    pub async fn load_physics3d_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_physics3d::build_demo_physics3d(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "3D Showroom" — the first 3D scene: every
    /// primitive on a lit stage, an orbit camera, moving point lights.
    pub async fn load_3d_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_3d::build_demo_3d(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "Ragdoll Torture" — a crash-test dummy ragdoll
    /// (a prefab) in a lab of hazards, with grab / punch / bomb / anvil /
    /// cannon tools and a PAIN score.
    pub async fn load_torture_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_torture::build_demo_torture(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "Crown Match" — a royal match-3: Home, a
    /// 20-level map, and the board (all vector shapes, no images).
    pub async fn load_match3_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_match3::build_demo_match3(&mut self.document);
        migrate_document(&mut self.document);
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "Ragdoll Arena" — bones, live IK, ragdoll
    /// joints, sprite skinning and layered state machines in one scene.
    pub async fn load_rig_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_rig::build_demo_rig(&mut self.document);
        migrate_document(&mut self.document);
        for image_id in demo.images {
            self.bake_and_cache_image(image_id).await;
        }
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// The + menu's Demos → "Stress Test" — a scalable scene for profiling.
    pub async fn load_stress_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_stress::build_demo_stress(&mut self.document);
        migrate_document(&mut self.document);
        for image_id in demo.images {
            self.bake_and_cache_image(image_id).await;
        }
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    pub async fn load_fps_demo(&mut self) -> Result<JsValue, JsValue> {
        let demo = crate::demo_fps::build_demo_fps(&mut self.document);
        migrate_document(&mut self.document);
        for image_id in demo.images {
            self.bake_and_cache_image(image_id).await;
        }
        to_js_value(&crate::dto::AddArtboardResult { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), new_artboard: demo.scene })
    }

    /// Switches which state machine this editor's State Machine tab
    /// previews/tests — the panel's state-machine selector, mirroring
    /// `set_active_animation`'s reasoning for the Timeline.
    pub fn set_active_state_machine(&mut self, artboard: JsValue, state_machine: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;
        self.active_state_machine = Some(state_machine_id);
        self.refresh_state_machine_preview(artboard_id);
        self.document_summary()
    }

    /// Soft-deletes a state machine — the panel's "Delete State Machine" action.
    pub fn delete_state_machine(&mut self, artboard: JsValue, state_machine: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;
        self.document.apply(Command::SetStateMachineDeleted { artboard: artboard_id, state_machine: state_machine_id, value: true });
        if self.active_state_machine == Some(state_machine_id) {
            self.active_state_machine = self.document.artboard(artboard_id).and_then(|a| a.state_machines().next()).map(|(id, _)| id);
            self.refresh_state_machine_preview(artboard_id);
        }
        self.document_summary()
    }

    /// Changes which state `state_machine` starts in — the panel's
    /// initial-state dropdown.
    pub fn set_state_machine_initial_state(&mut self, artboard: JsValue, state_machine: JsValue, value: String) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;
        self.document.apply(Command::SetStateMachineInitialState { artboard: artboard_id, state_machine: state_machine_id, value });
        self.refresh_state_machine_preview(artboard_id);
        self.document_summary()
    }

    /// Renames a state machine — the State Machine panel's "Edit" button.
    /// Undoable (`Command::SetStateMachineName`).
    pub fn rename_state_machine(&mut self, artboard: JsValue, state_machine: JsValue, value: String) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;
        self.document.apply(Command::SetStateMachineName { artboard: artboard_id, state_machine: state_machine_id, value });
        self.document_summary()
    }

    /// Declares a new named input (`kind`: `"Bool"`/`"Number"`/`"Enum"`/
    /// `"Trigger"`) on `state_machine` — the panel's "Declare Input" action.
    pub fn add_state_machine_input(&mut self, artboard: JsValue, state_machine: JsValue, name: String, kind: &str) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;
        let kind = parse_input_kind(kind)?;

        let mut inputs = self
            .document
            .artboard(artboard_id)
            .and_then(|a| a.state_machine(state_machine_id))
            .ok_or_else(|| JsValue::from_str("add_state_machine_input: unknown state machine"))?
            .inputs
            .clone();
        inputs.push(InputDef { name, kind });
        self.document.apply(Command::SetStateMachineInputs { artboard: artboard_id, state_machine: state_machine_id, value: inputs });
        self.refresh_state_machine_preview(artboard_id);
        self.document_summary()
    }

    /// Adds a new, independent extra layer to `state_machine` — one
    /// default state (`"State 1"`, looping, referencing
    /// `initial_animation`, same shape a brand-new machine's base layer
    /// gets from `add_state_machine`) and an empty node `mask` — the
    /// Layers panel's "Add Layer" action. Empty, not "every node," is the
    /// deliberate default: see `an1mbyt3_core::StateMachineLayerDef::mask`'s
    /// own doc comment for why a brand-new layer shouldn't silently start
    /// fighting the base layer for every node in the artboard.
    pub fn add_state_machine_layer(&mut self, artboard: JsValue, state_machine: JsValue, name: String, initial_animation: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;
        let animation_id: AnimationId = parse_id(initial_animation)?;

        let mut layers = self
            .document
            .artboard(artboard_id)
            .and_then(|a| a.state_machine(state_machine_id))
            .ok_or_else(|| JsValue::from_str("add_state_machine_layer: unknown state machine"))?
            .layers
            .clone();
        layers.push(an1mbyt3_core::StateMachineLayerDef {
            name,
            states: vec![StateDef { name: "State 1".into(), animation: animation_id, loop_mode: LoopMode::Loop, transitions: Vec::new(), position: Vec2::ZERO, blend: None, program: None }],
            initial_state: "State 1".into(),
            mask: Vec::new(),
        });
        self.document.apply(Command::SetStateMachineLayers { artboard: artboard_id, state_machine: state_machine_id, value: layers });
        self.refresh_state_machine_preview(artboard_id);
        self.document_summary()
    }

    /// Removes `layer_name` from `state_machine`'s extra `layers` — the
    /// Layers panel's per-layer "delete" action. Unlike `delete_state`
    /// (which refuses to remove a state machine's last remaining *state*),
    /// removing the last extra *layer* is always fine — a machine with
    /// zero extra layers is exactly what every machine already was before
    /// layers existed.
    pub fn delete_state_machine_layer(&mut self, artboard: JsValue, state_machine: JsValue, layer_name: String) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;

        let mut layers = self
            .document
            .artboard(artboard_id)
            .and_then(|a| a.state_machine(state_machine_id))
            .ok_or_else(|| JsValue::from_str("delete_state_machine_layer: unknown state machine"))?
            .layers
            .clone();
        layers.retain(|l| l.name != layer_name);
        self.document.apply(Command::SetStateMachineLayers { artboard: artboard_id, state_machine: state_machine_id, value: layers });
        self.refresh_state_machine_preview(artboard_id);
        self.document_summary()
    }

    /// Replaces `layer_name`'s whole node `mask` — the Layers panel's
    /// per-node checkbox for whichever layer is currently selected there.
    /// The frontend sends the full updated node-id set on every toggle
    /// (checked/unchecked), same "whole-list swap" convention every other
    /// `Vec`-valued field in this workspace uses, rather than separate
    /// add-one/remove-one variants.
    pub fn set_state_machine_layer_mask(&mut self, artboard: JsValue, state_machine: JsValue, layer_name: String, mask: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;
        let mask: Vec<NodeId> = parse_id(mask)?;

        let mut layers = self
            .document
            .artboard(artboard_id)
            .and_then(|a| a.state_machine(state_machine_id))
            .ok_or_else(|| JsValue::from_str("set_state_machine_layer_mask: unknown state machine"))?
            .layers
            .clone();
        let layer = layers.iter_mut().find(|l| l.name == layer_name).ok_or_else(|| JsValue::from_str("set_state_machine_layer_mask: unknown layer"))?;
        layer.mask = mask;
        self.document.apply(Command::SetStateMachineLayers { artboard: artboard_id, state_machine: state_machine_id, value: layers });
        self.refresh_state_machine_preview(artboard_id);
        self.document_summary()
    }

    /// Changes which state `layer_name` starts in — that layer's own
    /// initial-state dropdown, mirroring `set_state_machine_initial_state`'s
    /// reasoning for the base layer. Folds directly into the one
    /// `Command::SetStateMachineLayers` call, since a layer's own
    /// `initial_state` lives on the same `StateMachineLayerDef` as its
    /// `states` (unlike the base layer, whose `initial_state` is a
    /// separate `StateMachineDef` field, needing its own command).
    pub fn set_state_machine_layer_initial_state(&mut self, artboard: JsValue, state_machine: JsValue, layer_name: String, value: String) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;

        let mut layers = self
            .document
            .artboard(artboard_id)
            .and_then(|a| a.state_machine(state_machine_id))
            .ok_or_else(|| JsValue::from_str("set_state_machine_layer_initial_state: unknown state machine"))?
            .layers
            .clone();
        let layer = layers.iter_mut().find(|l| l.name == layer_name).ok_or_else(|| JsValue::from_str("set_state_machine_layer_initial_state: unknown layer"))?;
        layer.initial_state = value;
        self.document.apply(Command::SetStateMachineLayers { artboard: artboard_id, state_machine: state_machine_id, value: layers });
        self.refresh_state_machine_preview(artboard_id);
        self.document_summary()
    }

    /// Adds a new state to `state_machine`, referencing `animation` and
    /// looping per `loop_mode` (`"Once"`/`"Loop"`/`"PingPong"`) — the
    /// panel's "Add State" action.
    pub fn add_state(&mut self, artboard: JsValue, state_machine: JsValue, name: String, animation: JsValue, loop_mode: &str, layer: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;
        let animation_id: AnimationId = parse_id(animation)?;
        let loop_mode = parse_loop_mode(loop_mode)?;

        let (mut states, rebuild) = self.state_list_command(artboard_id, state_machine_id, &layer)?;
        // Cascades new states left-to-right so repeatedly clicking "Add
        // State" doesn't stack every box exactly on top of the last one —
        // same reasoning as `add_shape_node`'s own diagonal cascade, just a
        // wider horizontal step since a state box is wider than a shape's
        // selection handle. The designer drags it wherever they actually
        // want on the graph afterward, same as any node.
        let position = Vec2::new(220.0 * states.len() as f32, 0.0);
        states.push(StateDef { name, animation: animation_id, loop_mode, transitions: Vec::new(), position, blend: None, program: None });
        self.document.apply(rebuild(states));
        self.refresh_state_machine_preview(artboard_id);
        self.document_summary()
    }

    /// Changes an existing state's `animation`/`loop_mode` in place — the
    /// graph's selected-state inspector. Keeps that state's `transitions`
    /// and `position` untouched (unlike `add_state`, this isn't creating a
    /// new entry).
    pub fn update_state(&mut self, artboard: JsValue, state_machine: JsValue, name: String, animation: JsValue, loop_mode: &str, layer: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;
        let animation_id: AnimationId = parse_id(animation)?;
        let loop_mode = parse_loop_mode(loop_mode)?;

        let (mut states, rebuild) = self.state_list_command(artboard_id, state_machine_id, &layer)?;
        let state = states.iter_mut().find(|s| s.name == name).ok_or_else(|| JsValue::from_str("update_state: unknown state"))?;
        state.animation = animation_id;
        state.loop_mode = loop_mode;
        self.document.apply(rebuild(states));
        self.refresh_state_machine_preview(artboard_id);
        self.document_summary()
    }

    /// Turns `name` into a continuous blend-tree state (`blend: Some`) or
    /// back into a plain played-back one (`blend: null`/`undefined`) — the
    /// graph's selected-state inspector's "dynamic expression" toggle. See
    /// `an1mbyt3_core::Blend1DDef`'s own doc comment for what this changes
    /// at runtime; this method only ever edits `StateDef::blend` itself,
    /// same "whole-state-list swap" mechanism `update_state` already uses
    /// for its own single-field edits.
    pub fn set_state_blend(&mut self, artboard: JsValue, state_machine: JsValue, name: String, blend: JsValue, layer: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;
        let blend: Option<crate::dto::Blend1DDefDto> = if blend.is_null() || blend.is_undefined() {
            None
        } else {
            Some(serde_wasm_bindgen::from_value(blend).map_err(|err| JsValue::from_str(&format!("invalid blend: {err}")))?)
        };

        let (mut states, rebuild) = self.state_list_command(artboard_id, state_machine_id, &layer)?;
        let state = states.iter_mut().find(|s| s.name == name).ok_or_else(|| JsValue::from_str("set_state_blend: unknown state"))?;
        state.blend = blend.map(|b| an1mbyt3_core::Blend1DDef { input: b.input, low: b.low, high: b.high, low_value: b.low_value, high_value: b.high_value });
        self.document.apply(rebuild(states));
        self.refresh_state_machine_preview(artboard_id);
        self.document_summary()
    }

    /// Replaces `name`'s whole `program` — the node-graph/code editor's save
    /// action. `program` is `null`/`undefined` to turn a Programmable State
    /// back into a plain played-back one, or the JSON-serialized
    /// `crate::dto::ProgramDefDto` otherwise — see that type's own doc
    /// comment. The frontend computes the entire new value after every
    /// node/wire/override edit and calls this once (same convention
    /// `an1mbyt3_core::command::Command::SetStateProgram`'s own doc comment
    /// describes), never a finer-grained per-edit command.
    pub fn set_state_program(&mut self, artboard: JsValue, state_machine: JsValue, name: String, program: JsValue, layer: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;
        let program: Option<an1mbyt3_core::ProgramDef> = if program.is_null() || program.is_undefined() {
            None
        } else {
            let dto: crate::dto::ProgramDefDto = serde_wasm_bindgen::from_value(program).map_err(|err| JsValue::from_str(&format!("invalid program: {err}")))?;
            Some(dto.into())
        };

        let rebuild = self.program_command(artboard_id, state_machine_id, &layer, name)?;
        self.document.apply(rebuild(program));
        self.refresh_state_machine_preview(artboard_id);
        self.document_summary()
    }

    /// Removes `name` from `state_machine`'s `states` list — the graph's
    /// per-box "delete" action. Refuses (returns `Err`) to delete a state
    /// machine's last remaining state — an empty machine has nothing for
    /// `initial_state` to name and nothing for the runtime to start in. If
    /// the deleted state was `initial_state`, reassigns it to whichever
    /// state happens to be first afterward, batched into the same undo step
    /// (`Command::Batch`) so undo restores both the state and the original
    /// `initial_state` together. Doesn't cascade into removing dangling
    /// transitions elsewhere that targeted this state's name — same
    /// "harmless no-op, not this command's job to understand the whole
    /// graph" reasoning `SetStateMachineStates`'s own doc comment already
    /// establishes.
    ///
    /// `layer` (`null`/`undefined` for the base layer, a layer name
    /// otherwise) picks which one's own "needs at least one state"
    /// guard/fallback-initial-state logic applies — an extra layer's
    /// `initial_state` fallback folds directly into the same
    /// `Command::SetStateMachineLayers` call (both fields live on the same
    /// `StateMachineLayerDef`), unlike the base layer's two-command
    /// `Command::Batch` (`states`/`initial_state` are separate
    /// `StateMachineDef` fields there).
    pub fn delete_state(&mut self, artboard: JsValue, state_machine: JsValue, name: String, layer: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;
        let layer_name = Self::parse_layer_arg(&layer)?;

        let def = self
            .document
            .artboard(artboard_id)
            .and_then(|a| a.state_machine(state_machine_id))
            .ok_or_else(|| JsValue::from_str("delete_state: unknown state machine"))?;

        match layer_name {
            None => {
                if def.states.len() <= 1 {
                    return Err(JsValue::from_str("delete_state: a state machine needs at least one state"));
                }
                let mut states = def.states.clone();
                states.retain(|s| s.name != name);
                let needs_new_initial = def.initial_state == name;
                let fallback_initial = states[0].name.clone();

                let mut commands = vec![Command::SetStateMachineStates { artboard: artboard_id, state_machine: state_machine_id, value: states }];
                if needs_new_initial {
                    commands.push(Command::SetStateMachineInitialState { artboard: artboard_id, state_machine: state_machine_id, value: fallback_initial });
                }
                self.document.apply(Command::Batch(commands));
            }
            Some(layer_name) => {
                let target = def.layers.iter().find(|l| l.name == layer_name).ok_or_else(|| JsValue::from_str("delete_state: unknown layer"))?;
                if target.states.len() <= 1 {
                    return Err(JsValue::from_str("delete_state: a layer needs at least one state"));
                }
                let mut layers = def.layers.clone();
                let l = layers.iter_mut().find(|l| l.name == layer_name).unwrap();
                l.states.retain(|s| s.name != name);
                if l.initial_state == name {
                    l.initial_state = l.states[0].name.clone();
                }
                self.document.apply(Command::SetStateMachineLayers { artboard: artboard_id, state_machine: state_machine_id, value: layers });
            }
        }
        self.refresh_state_machine_preview(artboard_id);
        self.document_summary()
    }

    /// Adds a transition out of `state_name`, to state `to`, guarded by
    /// `condition` (a JSON object matching `dto::ConditionDefDto`'s shape —
    /// e.g. `{kind: "Trigger", input: "toggle"}`) and playing `direction`
    /// (`"Forward"`/`"Reverse"`) — the panel's "Add Transition" action.
    ///
    /// `duration_seconds` (crossfade length — see `TransitionDef::duration_seconds`'s
    /// doc comment) accepts a negative sentinel meaning "use this authoring-
    /// time default": the *target* (`to`) state's own animation duration.
    /// That default is a UI convenience for a brand-new transition (a
    /// crossfade that fills the whole time it takes the destination
    /// animation to play once reads as an intentional choice, not an
    /// arbitrary number) — it is *not* the same as `TransitionDef`'s own
    /// `#[serde(default)]` (`0.0`, instant), which only ever matters for a
    /// `.tea`/`.teab` saved before this field existed.
    pub fn add_transition(
        &mut self,
        artboard: JsValue,
        state_machine: JsValue,
        state_name: String,
        to: String,
        condition: JsValue,
        direction: &str,
        duration_seconds: f32,
        interruptible: bool,
        layer: JsValue,
    ) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;
        let condition: crate::dto::ConditionDefDto =
            serde_wasm_bindgen::from_value(condition).map_err(|err| JsValue::from_str(&format!("invalid condition: {err}")))?;
        let direction = parse_playback_direction(direction)?;

        // The negative-duration-sentinel default (target state's own
        // animation duration) needs its own lookup against whichever
        // states list (base or layer) `to` actually lives in — resolved
        // here, before the shared `transitions_command` helper, since that
        // helper only knows about `state_name`'s own transitions, not `to`'s
        // animation.
        let layer_name = Self::parse_layer_arg(&layer)?;
        let artboard_ref = self.document.artboard(artboard_id).ok_or_else(|| JsValue::from_str("add_transition: unknown artboard"))?;
        let state_machine_def = artboard_ref
            .state_machine(state_machine_id)
            .ok_or_else(|| JsValue::from_str("add_transition: unknown state machine"))?;
        let target_states = match &layer_name {
            None => &state_machine_def.states,
            Some(name) => &state_machine_def.layers.iter().find(|l| &l.name == name).ok_or_else(|| JsValue::from_str("add_transition: unknown layer"))?.states,
        };
        let resolved_duration = if duration_seconds >= 0.0 {
            duration_seconds
        } else {
            target_states.iter().find(|s| s.name == to).and_then(|target_state| artboard_ref.animation(target_state.animation)).map(|anim| anim.duration).unwrap_or(0.0)
        };

        let (mut transitions, rebuild) = self.transitions_command(artboard_id, state_machine_id, &layer, state_name)?;
        transitions.push(TransitionDef { to, condition: condition.into(), direction, duration_seconds: resolved_duration, interruptible });
        self.document.apply(rebuild(transitions));
        self.refresh_state_machine_preview(artboard_id);
        self.document_summary()
    }

    /// Replaces the transition at `index` in `state_name`'s transition list
    /// in place — the graph's per-transition inline editor (changing its
    /// condition/direction after creation without re-adding it at the end
    /// of the list). Panics (via the `expect` below) if `index` is out of
    /// range — same "caller bug" convention as every other variant; the
    /// panel only ever passes back an index it itself just listed.
    /// `duration_seconds` accepts a negative sentinel meaning "leave this
    /// transition's crossfade duration exactly as it already is" — unlike
    /// `add_transition`'s own sentinel (which derives a fresh default from
    /// the target state's animation), re-deriving here would silently reset
    /// a duration the designer already deliberately tuned every time they
    /// only meant to change the condition/direction. `interruptible` takes
    /// the same "leave unchanged" approach via `null`/`undefined` (a plain
    /// `bool` has no spare sentinel value the way a duration has "negative"),
    /// for the identical reason.
    pub fn set_transition(
        &mut self,
        artboard: JsValue,
        state_machine: JsValue,
        state_name: String,
        index: usize,
        condition: JsValue,
        direction: &str,
        duration_seconds: f32,
        interruptible: JsValue,
        layer: JsValue,
    ) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;
        let condition: crate::dto::ConditionDefDto =
            serde_wasm_bindgen::from_value(condition).map_err(|err| JsValue::from_str(&format!("invalid condition: {err}")))?;
        let direction = parse_playback_direction(direction)?;
        let interruptible: Option<bool> = if interruptible.is_null() || interruptible.is_undefined() {
            None
        } else {
            Some(interruptible.as_bool().ok_or_else(|| JsValue::from_str("interruptible must be a boolean or null"))?)
        };

        let (mut transitions, rebuild) = self.transitions_command(artboard_id, state_machine_id, &layer, state_name)?;
        let existing = transitions.get(index).ok_or_else(|| JsValue::from_str("set_transition: index out of range"))?;
        let to = existing.to.clone();
        let resolved_duration = if duration_seconds >= 0.0 { duration_seconds } else { existing.duration_seconds };
        let resolved_interruptible = interruptible.unwrap_or(existing.interruptible);
        transitions[index] = TransitionDef { to, condition: condition.into(), direction, duration_seconds: resolved_duration, interruptible: resolved_interruptible };
        self.document.apply(rebuild(transitions));
        self.refresh_state_machine_preview(artboard_id);
        self.document_summary()
    }

    /// Removes the transition at `index` from `state_name`'s transition list
    /// — the graph's per-transition "delete" button.
    pub fn remove_transition(&mut self, artboard: JsValue, state_machine: JsValue, state_name: String, index: usize, layer: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;

        let (mut transitions, rebuild) = self.transitions_command(artboard_id, state_machine_id, &layer, state_name)?;
        if index >= transitions.len() {
            return Err(JsValue::from_str("remove_transition: index out of range"));
        }
        transitions.remove(index);
        self.document.apply(rebuild(transitions));
        self.refresh_state_machine_preview(artboard_id);
        self.document_summary()
    }

    /// Moves a state's box on the graph canvas — the panel's drag-to-reposition
    /// gesture. Undoable (`Command::SetStatePosition`); meant to be called
    /// once per whole drag (on pointer-up), not per pointer-move frame, same
    /// "one undo step per gesture" convention `commit_node_translation`
    /// already established (there's no live-preview variant here since a
    /// state box's position isn't rendered on the WGPU canvas at all — it's
    /// pure React/DOM state in `StateMachineGraph.tsx` while dragging).
    pub fn set_state_position(&mut self, artboard: JsValue, state_machine: JsValue, state_name: String, x: f32, y: f32, layer: JsValue) -> Result<JsValue, JsValue> {
        let artboard_id: ArtboardId = parse_id(artboard)?;
        let state_machine_id: an1mbyt3_core::StateMachineId = parse_id(state_machine)?;

        if Self::parse_layer_arg(&layer)?.is_none() {
            self.document.apply(Command::SetStatePosition { artboard: artboard_id, state_machine: state_machine_id, state_name, value: Vec2::new(x, y) });
        } else {
            let (mut states, rebuild) = self.state_list_command(artboard_id, state_machine_id, &layer)?;
            let state = states.iter_mut().find(|s| s.name == state_name).ok_or_else(|| JsValue::from_str("set_state_position: unknown state"))?;
            state.position = Vec2::new(x, y);
            self.document.apply(rebuild(states));
        }
        self.document_summary()
    }

    /// Current state machine state (for the State Machine panel to render
    /// its states list/active-state highlight/trigger buttons) — see
    /// `dto::StateMachineSummary`.
    pub fn state_machine_summary(&self) -> Result<JsValue, JsValue> {
        to_js_value(&summarize_state_machine(&self.state_machine))
    }

    /// Serializes the current document to `.tea` (pretty-printed JSON) — see
    /// `an1mbyt3_format::save_tea`. The native shell (`apps/editor/src-tauri`)
    /// never sees a `Document`, only this string, which it writes verbatim
    /// to whatever path the user picked.
    pub fn save_tea(&self) -> Result<String, JsValue> {
        an1mbyt3_format::save_tea(&self.document)
            .map_err(|err| JsValue::from_str(&format!("failed to save .tea: {err}")))
    }

    /// Encodes the current document to `.teab` (postcard + DEFLATE) — see
    /// `an1mbyt3_format::save_teab`. `Vec<u8>` bridges to JS as a `Uint8Array`.
    /// Exports one scene (`artboard`, or the first) as **Toasty Lite** JSON
    /// — the small Canvas 2D runtime's format for UI animation (see
    /// `an1mbyt3_format::lite`). Export → Animation → Toasty Lite.
    pub fn export_lite(&self, artboard: JsValue) -> Result<String, JsValue> {
        let id: Option<ArtboardId> = if artboard.is_null() || artboard.is_undefined() { None } else { Some(parse_id(artboard)?) };
        let value = an1mbyt3_format::lite::export_lite(&self.document, id).ok_or_else(|| JsValue::from_str("no such scene"))?;
        serde_json::to_string(&value).map_err(|err| JsValue::from_str(&err.to_string()))
    }

    pub fn save_teab(&self) -> Result<Vec<u8>, JsValue> {
        an1mbyt3_format::save_teab(&self.document)
            .map_err(|err| JsValue::from_str(&format!("failed to save .teab: {err}")))
    }

    /// Replaces the current document with one parsed from `.tea` JSON.
    /// Clears `mesh_cache` since the loaded document's `NodeId`s are unrelated
    /// to any cached from the previous document; a fresh `Document` also
    /// starts with an empty undo/redo history, correctly discarding the
    /// replaced document's history rather than letting undo reach back into it.
    /// Selects the loaded artboard's first non-deleted animation (if any) as
    /// active, since the previous `active_animation`'s id is meaningless
    /// against a brand-new `Document`.
    ///
    /// `async` (unlike every other document-mutating method here) because it
    /// bakes every `ImageAsset` the freshly loaded document's `Image` nodes
    /// reference before returning — see `bake_and_cache_image`'s doc comment
    /// for why baking can only happen at a real `.await` point, never inside
    /// the plain-sync `render`. Doing this once here (not lazily in `render`)
    /// means every loaded image is already visible the very first frame
    /// after the JS caller's `await` resolves, rather than popping in a few
    /// frames later.
    pub async fn load_tea(&mut self, json: &str) -> Result<JsValue, JsValue> {
        self.document =
            an1mbyt3_format::load_tea(json).map_err(|err| JsValue::from_str(&format!("failed to load .tea: {err}")))?;
        migrate_document(&mut self.document);
        self.mesh_cache.clear();
        self.shape_mesh_source.clear();
        self.stroke_mesh_cache.clear();
        self.stroke_mesh_source.clear();
        self.tilemap_mesh_cache.clear();
        self.tilemap_mesh_source.clear();
        self.font_cache.clear();
        crate::text_bounds::clear();
        self.text_mesh_cache.clear();
        self.image_bindings.clear();
        self.image_cache.clear();
        self.active_artboard = None;
        self.particle_runtimes.clear();
        self.particle_bindings.clear();
        self.particle_fluid_gpu.clear();
        self.goo_gpu.clear();
        self.goo_keys.clear();
        self.fluid_keys.clear();
        self.goo_text_cache.borrow_mut().clear();
        self.liquid_bindings.clear();
        self.trail_runtimes.clear();
        self.physics = None;
        self.physics3d = None;
        self.trail_gpu.clear();
        self.cloth_sims.clear();
        self.cloth_gpu.clear();
        self.tree_states.clear();
        self.tree_gpu.clear();
        // A new document: no live pose snapshot belongs to it.
        self.design_pose_snapshot = None;
        self.runtime_spawned.clear();
        self.active_animation = self.current_artboard_id().and_then(|id| self.document.artboard(id)).and_then(|a| a.animations().next()).map(|(id, _)| id);
        self.reset_active_state_machine_after_load();
        self.scrub_time = 0.0;
        self.bake_all_images().await;
        self.document_summary()
    }

    // ---- Project folders (see `an1mbyt3_format::project`) ----

    /// Splits the document into project files for `project_name`'s folder.
    /// Returns `[{ path, text }]` for text files and `[{ path, bytes: i }]`
    /// for binary ones — fetch those with `take_project_file_bytes(i)` (one
    /// `Uint8Array` each, instead of a JSON number array per image).
    pub fn save_project(&mut self, project_name: &str) -> Result<JsValue, JsValue> {
        let files = an1mbyt3_format::save_project(&self.document, project_name).map_err(|err| JsValue::from_str(&format!("failed to save project: {err}")))?;
        let mut listing = Vec::new();
        self.pending_project_bytes.clear();
        for file in files {
            let asset = file.asset.map(Into::into);
            match file.contents {
                an1mbyt3_format::ProjectContents::Text(text) => listing.push(ProjectFileDto { path: file.path, text: Some(text), bytes: None, linked: file.linked, asset }),
                an1mbyt3_format::ProjectContents::Bytes(bytes) => {
                    listing.push(ProjectFileDto { path: file.path, text: None, bytes: Some(self.pending_project_bytes.len() as u32), linked: file.linked, asset });
                    self.pending_project_bytes.push(bytes);
                }
            }
        }
        to_js_value(&listing)
    }

    /// Which project file holds which scene / prefab / image, without any
    /// file contents — the Resources panel's folder view maps disk files to
    /// document items with it. Same paths a `save_project` would write.
    pub fn project_file_index(&self, project_name: &str) -> Result<JsValue, JsValue> {
        let files = an1mbyt3_format::save_project(&self.document, project_name).map_err(|err| JsValue::from_str(&format!("failed to index project: {err}")))?;
        let listing: Vec<ProjectFileDto> =
            files.into_iter().map(|file| ProjectFileDto { path: file.path, text: None, bytes: None, linked: file.linked, asset: file.asset.map(Into::into) }).collect();
        to_js_value(&listing)
    }

    /// Links an image asset to its file inside the project folder
    /// (`ImageAsset::file`) — set right after the file was copied into
    /// `Assets/`, so saves treat that file as the source and never write a
    /// duplicate. `null` unlinks.
    // ------------------------------------------------------------ audio --

    /// Import → Audio: adds a sound (MP3 / WAV / OGG / … bytes) to the
    /// document. Returns `{ document, audio }`. (Decoding / playback happen
    /// in the host's Web Audio — see `apps/editor/src/game/audio.ts`.)
    pub fn import_audio(&mut self, filename: String, bytes: Vec<u8>) -> Result<JsValue, JsValue> {
        if bytes.is_empty() {
            return Err(JsValue::from_str("import_audio: the file is empty"));
        }
        let name = filename.rsplit(['/', '\\']).next().unwrap_or(&filename).to_string();
        let audio = self.document.add_audio(an1mbyt3_core::AudioAsset { name, source: bytes, file: None, deleted: false });
        self.document.touch();
        #[derive(serde::Serialize)]
        struct Out {
            document: crate::dto::DocumentSummary,
            audio: an1mbyt3_core::AudioId,
        }
        to_js_value(&Out { document: summarize_document(&self.document, self.active_animation, self.active_state_machine), audio })
    }

    /// A sound's encoded bytes (for the host to decode and play).
    pub fn audio_bytes(&self, audio: JsValue) -> Result<Vec<u8>, JsValue> {
        let id: an1mbyt3_core::AudioId = parse_id(audio)?;
        Ok(self.document.audio(id).map(|a| a.source.clone()).unwrap_or_default())
    }

    /// Every sound's id and name (scripts' `ctx.playSound(name)` lookup).
    pub fn audio_list(&self) -> Result<JsValue, JsValue> {
        #[derive(serde::Serialize)]
        struct Entry<'a> {
            id: an1mbyt3_core::AudioId,
            name: &'a str,
        }
        let list: Vec<Entry> = self.document.audio_assets().map(|(id, a)| Entry { id, name: &a.name }).collect();
        to_js_value(&list)
    }

    /// Links a sound to its file in the project folder (like `set_image_file`).
    pub fn set_audio_file(&mut self, audio: JsValue, path: Option<String>) -> Result<(), JsValue> {
        let id: an1mbyt3_core::AudioId = parse_id(audio)?;
        let asset = self.document.audio_mut(id).ok_or_else(|| JsValue::from_str("set_audio_file: unknown sound"))?;
        asset.file = path;
        self.document.touch();
        Ok(())
    }

    /// Renames a sound (scripts play it by name).
    pub fn rename_audio(&mut self, audio: JsValue, name: String) -> Result<JsValue, JsValue> {
        let id: an1mbyt3_core::AudioId = parse_id(audio)?;
        if let Some(asset) = self.document.audio_mut(id) {
            asset.name = name;
        }
        self.document.touch();
        self.document_summary()
    }

    /// Removes a sound from the document (Audio Sources using it go quiet).
    pub fn delete_audio(&mut self, audio: JsValue) -> Result<JsValue, JsValue> {
        let id: an1mbyt3_core::AudioId = parse_id(audio)?;
        if let Some(asset) = self.document.audio_mut(id) {
            asset.deleted = true;
            asset.source.clear();
        }
        self.document.touch();
        self.document_summary()
    }

    pub fn set_image_file(&mut self, image: JsValue, path: Option<String>) -> Result<(), JsValue> {
        let image_id: an1mbyt3_core::ImageId = parse_id(image)?;
        let asset = self.document.image_mut(image_id).ok_or_else(|| JsValue::from_str("set_image_file: unknown image"))?;
        asset.file = path;
        self.document.touch();
        Ok(())
    }

    /// The bytes of `save_project`'s binary file `index` (moved out — each
    /// can be taken once).
    pub fn take_project_file_bytes(&mut self, index: u32) -> Vec<u8> {
        self.pending_project_bytes.get_mut(index as usize).map(std::mem::take).unwrap_or_default()
    }

    /// The asset files a project manifest references — read these from the
    /// folder, then pass them to `load_project`.
    pub fn project_asset_paths(manifest: &str) -> Result<Vec<String>, JsValue> {
        an1mbyt3_format::project_asset_paths(manifest).map_err(|err| JsValue::from_str(&format!("{err}")))
    }

    /// Opens a project: `manifest` text plus every file it references, as
    /// parallel `paths` / `ends` (exclusive end offsets) into one
    /// concatenated `blob` — one boundary crossing for the whole project.
    /// Same resets as `load_tea`.
    pub async fn load_project(&mut self, manifest: &str, paths: Vec<String>, ends: Vec<u32>, blob: Vec<u8>) -> Result<JsValue, JsValue> {
        let mut start = 0usize;
        let mut ranges = std::collections::HashMap::new();
        for (path, end) in paths.into_iter().zip(ends) {
            let end = (end as usize).min(blob.len());
            ranges.insert(path, (start, end));
            start = end;
        }
        self.document = an1mbyt3_format::load_project(manifest, |path| ranges.get(path).map(|&(a, b)| blob[a..b].to_vec()))
            .map_err(|err| JsValue::from_str(&format!("failed to open project: {err}")))?;
        migrate_document(&mut self.document);
        self.mesh_cache.clear();
        self.shape_mesh_source.clear();
        self.stroke_mesh_cache.clear();
        self.stroke_mesh_source.clear();
        self.tilemap_mesh_cache.clear();
        self.tilemap_mesh_source.clear();
        self.font_cache.clear();
        crate::text_bounds::clear();
        self.text_mesh_cache.clear();
        self.image_bindings.clear();
        self.image_cache.clear();
        self.active_artboard = None;
        self.particle_runtimes.clear();
        self.particle_bindings.clear();
        self.particle_fluid_gpu.clear();
        self.goo_gpu.clear();
        self.goo_keys.clear();
        self.fluid_keys.clear();
        self.goo_text_cache.borrow_mut().clear();
        self.liquid_bindings.clear();
        self.trail_runtimes.clear();
        self.physics = None;
        self.physics3d = None;
        self.trail_gpu.clear();
        self.cloth_sims.clear();
        self.cloth_gpu.clear();
        self.tree_states.clear();
        self.tree_gpu.clear();
        // A new document: no live pose snapshot belongs to it.
        self.design_pose_snapshot = None;
        self.runtime_spawned.clear();
        self.active_animation = self.current_artboard_id().and_then(|id| self.document.artboard(id)).and_then(|a| a.animations().next()).map(|(id, _)| id);
        self.reset_active_state_machine_after_load();
        self.scrub_time = 0.0;
        self.bake_all_images().await;
        self.document_summary()
    }

    /// See `Document::instance` — changes whenever the document is replaced.
    pub fn document_instance(&self) -> f64 {
        self.document.instance() as f64
    }

    /// A cheap fingerprint of everything the document summary could show
    /// changing without an edit: the document revision (any edit), the
    /// active animation / state machine, and the shown scene's live pose
    /// (every node's transform, visibility, z and fill — what a live
    /// preview animates). The live panel refresh only rebuilds the summary
    /// (tens of ms on a big project) when this changes. Folded into 52
    /// bits so it survives the trip through a JS number exactly.
    pub fn summary_fingerprint(&self) -> f64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |v: u64| {
            h ^= v;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        };
        mix(self.document.revision() as u64);
        mix(self.document.instance() as u64);
        mix(self.active_animation.map_or(u64::MAX, |id| slotmap::Key::data(&id).as_ffi()));
        mix(self.active_state_machine.map_or(u64::MAX, |id| slotmap::Key::data(&id).as_ffi()));
        if let Some(artboard) = self.current_artboard_id().and_then(|id| self.document.artboard(id)) {
            for id in artboard.node_ids() {
                let Some(node) = artboard.node(id) else { continue };
                let t = &node.transform;
                for v in [t.translation.x, t.translation.y, t.rotation, t.scale.x, t.scale.y, t.anchor.x, t.anchor.y, node.z_index] {
                    mix(v.to_bits() as u64);
                }
                mix(node.visible as u64);
                if let Some(c) = node_fill_color(node) {
                    for v in c {
                        mix(v.to_bits() as u64);
                    }
                }
                if let NodeKind::Node3D(n3) = &node.kind {
                    for v in n3.transform.position.to_array().into_iter().chain(n3.transform.rotation_degrees.to_array()).chain(n3.transform.scale.to_array()) {
                        mix(v.to_bits() as u64);
                    }
                }
            }
        }
        (h & ((1u64 << 52) - 1)) as f64
    }

    /// See `Document::revision` — the frontend's unsaved-changes tracking.
    pub fn document_revision(&self) -> f64 {
        self.document.revision() as f64
    }

    /// Replaces the current document with one decoded from `.teab` bytes.
    /// Same cache-clearing/history-reset/active-animation-reset/image-baking
    /// reasoning as `load_tea`.
    pub async fn load_teab(&mut self, bytes: &[u8]) -> Result<JsValue, JsValue> {
        self.document =
            an1mbyt3_format::load_teab(bytes).map_err(|err| JsValue::from_str(&format!("failed to load .teab: {err}")))?;
        migrate_document(&mut self.document);
        self.mesh_cache.clear();
        self.shape_mesh_source.clear();
        self.stroke_mesh_cache.clear();
        self.stroke_mesh_source.clear();
        self.tilemap_mesh_cache.clear();
        self.tilemap_mesh_source.clear();
        self.font_cache.clear();
        crate::text_bounds::clear();
        self.text_mesh_cache.clear();
        self.image_bindings.clear();
        self.image_cache.clear();
        self.active_artboard = None;
        self.particle_runtimes.clear();
        self.particle_bindings.clear();
        self.particle_fluid_gpu.clear();
        self.goo_gpu.clear();
        self.goo_keys.clear();
        self.fluid_keys.clear();
        self.goo_text_cache.borrow_mut().clear();
        self.liquid_bindings.clear();
        self.trail_runtimes.clear();
        self.physics = None;
        self.physics3d = None;
        self.trail_gpu.clear();
        self.cloth_sims.clear();
        self.cloth_gpu.clear();
        self.tree_states.clear();
        self.tree_gpu.clear();
        // A new document: no live pose snapshot belongs to it.
        self.design_pose_snapshot = None;
        self.runtime_spawned.clear();
        self.active_animation = self.current_artboard_id().and_then(|id| self.document.artboard(id)).and_then(|a| a.animations().next()).map(|(id, _)| id);
        self.reset_active_state_machine_after_load();
        self.scrub_time = 0.0;
        self.bake_all_images().await;
        self.document_summary()
    }

    /// Decodes `document.image(image)`'s `source` bytes and bakes them
    /// through `Renderer::bake_image` per that asset's own `processing`
    /// params, then uploads the result into `image_cache` — the one place
    /// this crate ever does a GPU readback (`Renderer::bake_image`'s own doc
    /// comment). Called from `import_image`/`set_image_processing`/`load_tea`/
    /// `load_teab`, every one of which is `async` specifically so this can be
    /// awaited to completion — `render` itself is a plain sync per-frame
    /// method with no way to await a GPU readback, which is why baking never
    /// happens there (see `render`'s Image-loop comment). Silently leaves
    /// `image_cache` unpopulated for `image` on any decode error — same
    /// "degrade to not-drawn rather than crash the whole render loop"
    /// convention the shapes loop's degenerate-path handling already uses.
    async fn bake_and_cache_image(&mut self, image: an1mbyt3_core::ImageId) {
        let Some(asset) = self.document.image(image) else { return };
        let Ok((rgba, width, height)) = an1mbyt3_image::decode_to_rgba(&asset.source) else { return };
        let (baked, baked_width, baked_height) = self.renderer.bake_image(&rgba, width, height, &asset.processing).await;
        let texture = self.renderer.upload_texture(baked_width, baked_height, &baked, asset.processing.filter);
        self.image_cache.insert(image, texture);
    }

    /// Bakes every image asset in the current document — `load_tea`/
    /// `load_teab`'s shared post-load step, since a freshly loaded document's
    /// `images()` table is entirely new to this `WasmEditor` (its
    /// `image_cache` was just cleared alongside the document swap).
    async fn bake_all_images(&mut self) {
        let ids: Vec<an1mbyt3_core::ImageId> = self.document.images().map(|(id, _)| id).collect();
        for id in ids {
            self.bake_and_cache_image(id).await;
        }
    }
}

fn parse_id<T: serde::de::DeserializeOwned>(value: JsValue) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(value).map_err(|err| JsValue::from_str(&format!("invalid id: {err}")))
}

/// Serializes `value` to a `JsValue` for the wasm boundary — every method
/// here that hands JS a `DocumentSummary`-shaped result goes through this
/// instead of the bare `serde_wasm_bindgen::to_value` free function, because
/// that free function's *default* `Serializer` turns `Option::None` into JS
/// `undefined`, not `null`. The frontend's own TS types (`ipc/document.ts`)
/// declare every one of those fields (`NodeSummary::parent`, `groupMask`,
/// `fillColorOverride`, `variant`, ...) as `T | null`, and code throughout
/// the frontend checks for it that way (`node.parent === null`,
/// `fillColorOverride === null`, an `App.tsx` lookup filtering root-level
/// nodes by `n.parent === null`, ...) — against the default serializer,
/// every one of those checks silently never matches (`undefined === null`
/// is `false` in JS), which is exactly what made "Load Demo Scene"'s
/// freshly-imported `Rocket` node impossible to find and its hover
/// animation never get built, despite the SVG import itself always
/// succeeding. `serialize_missing_as_null(true)` makes the wire format
/// actually match what the TS side already assumed it was.
#[wasm_bindgen]
extern "C" {
    /// The browser's native `JSON.parse` — see `to_js_value`.
    #[wasm_bindgen(js_namespace = JSON, js_name = parse, catch)]
    fn json_parse(text: &str) -> Result<JsValue, JsValue>;
}

fn to_js_value<T: serde::Serialize>(value: &T) -> Result<JsValue, JsValue> {
    // Fast path: serialize to one JSON string and let the browser's native
    // `JSON.parse` build the objects. `serde_wasm_bindgen` instead crosses
    // the wasm/JS boundary once per field (thousands of calls per node), so
    // for big payloads — a `DocumentSummary` of a 4k-node scene, returned by
    // nearly every editing call — this is several times faster and is what
    // keeps the editor's panels responsive on large projects.
    //
    // The shapes match what `serde_wasm_bindgen` produced below: `None` is
    // `null` (`serialize_missing_as_null`), maps are plain objects. The one
    // difference is non-finite floats (JSON has no NaN — they become
    // `null`); nothing returned through here carries those. Anything JSON
    // can't express (e.g. a map with non-string keys) errors, and falls
    // back to the old path.
    if let Ok(text) = serde_json::to_string(value) {
        if let Ok(parsed) = json_parse(&text) {
            return Ok(parsed);
        }
    }
    // Maps go out as plain objects (`Record<string, …>` in TS), not JS
    // `Map`s — `summary.layerActiveStates[name]` must index like an object.
    value
        .serialize(&serde_wasm_bindgen::Serializer::new().serialize_missing_as_null(true).serialize_maps_as_objects(true))
        .map_err(|err| JsValue::from_str(&format!("failed to serialize value for the frontend: {err}")))
}

/// `mesh3d_cache` key: a primitive's variant plus its parameters' exact
/// bits. (It used to be `format!("{primitive:?}")` — formatting floats as
/// text for every mesh, every frame, showed up in big-scene profiles.)
type Mesh3DKey = (u8, [u32; 4]);

/// Block worlds are meshed in cubes of this many blocks a side, so one
/// edit re-meshes a few thousand blocks, not the whole world; streamed
/// worlds load in columns this wide.
const VOXEL_REGION: u32 = 16;
/// Regions meshed per block world per frame at most (nearest first).
const VOXEL_MESH_BUDGET: usize = 40;

/// Where a block world's blocks live: one fixed box (`ref.setVoxels`), or
/// streamed 16-wide columns loaded / unloaded by the script around the
/// player (`ref.setVoxelChunk`), with no edge.
enum VoxelStore {
    Dense(an1mbyt3_geometry::VoxelGrid),
    Chunked { height: u32, columns: HashMap<(i32, i32), Vec<u8>> },
}

impl an1mbyt3_geometry::BlockSource for VoxelStore {
    fn block(&self, x: i32, y: i32, z: i32) -> u8 {
        match self {
            VoxelStore::Dense(grid) => grid.get(x, y, z),
            VoxelStore::Chunked { height, columns } => {
                if y < 0 || y >= *height as i32 {
                    return 0;
                }
                let r = VOXEL_REGION as i32;
                columns.get(&(x.div_euclid(r), z.div_euclid(r))).map_or(0, |c| c[(x.rem_euclid(r) + r * (z.rem_euclid(r) + r * y)) as usize])
            }
        }
    }
}

/// A breakable glass pane's runtime (`WasmEditor::glass`): the fracture
/// state plus its GPU meshes — the intact pieces (tagged with the pane
/// version they were built from; `None` inside once nothing is left) and
/// this frame's shards.
pub(crate) struct GlassRuntime {
    /// The `Primitive3D::Glass` fields it was cut from (re-cut on change).
    key: [u32; 6],
    pane: an1mbyt3_geometry::GlassPane,
    intact: Option<(u64, Option<std::rc::Rc<an1mbyt3_renderer::GpuMesh3D>>)>,
    shards: Option<std::rc::Rc<an1mbyt3_renderer::GpuMesh3D>>,
}

impl GlassRuntime {
    fn new(key: [u32; 6], pane: an1mbyt3_geometry::GlassPane) -> Self {
        Self { key, pane, intact: None, shards: None }
    }
}

/// A glass pane's runtime state in `glass`, (re)cut from `primitive` when
/// it's new or its shape changed. `None` if it isn't glass.
fn glass_entry(glass: &mut HashMap<NodeId, GlassRuntime>, node_id: NodeId, primitive: an1mbyt3_core::Primitive3D) -> Option<&mut GlassRuntime> {
    let an1mbyt3_core::Primitive3D::Glass { width, height, thickness, shard, seed, anchors } = primitive else { return None };
    let key = [width.to_bits(), height.to_bits(), thickness.to_bits(), shard.to_bits(), seed, anchors];
    let runtime = glass.entry(node_id).or_insert_with(|| GlassRuntime::new(key, an1mbyt3_geometry::GlassPane::new(width, height, thickness, shard, seed, anchors)));
    if runtime.key != key {
        *runtime = GlassRuntime::new(key, an1mbyt3_geometry::GlassPane::new(width, height, thickness, shard, seed, anchors));
    }
    Some(runtime)
}

/// A stable per-node number (mixed into glass break seeds).
fn node_id_hash(id: NodeId) -> u32 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut h);
    h.finish() as u32
}

/// A block world's runtime state (`WasmEditor::voxel_worlds`): its blocks,
/// the uploaded mesh of every non-empty region, and which regions need
/// re-meshing.
struct VoxelWorld {
    /// The node's `Voxels` size when this was made (a resize starts over).
    size: [u32; 3],
    store: VoxelStore,
    regions: HashMap<[i32; 3], std::rc::Rc<an1mbyt3_renderer::GpuMesh3D>>,
    dirty: std::collections::HashSet<[i32; 3]>,
}

impl VoxelWorld {
    /// An empty (all air) fixed-size world; every region starts dirty so it
    /// is fully meshed once filled.
    fn dense(size: [u32; 3]) -> Self {
        let mut world = Self { size, store: VoxelStore::Dense(an1mbyt3_geometry::VoxelGrid::new(size[0], size[1], size[2])), regions: HashMap::new(), dirty: Default::default() };
        let count = |n: u32| n.div_ceil(VOXEL_REGION) as i32;
        for y in 0..count(size[1]) {
            for z in 0..count(size[2]) {
                for x in 0..count(size[0]) {
                    world.dirty.insert([x, y, z]);
                }
            }
        }
        world
    }

    /// An empty streamed world (`size[1]` blocks tall; width / depth unused).
    fn chunked(size: [u32; 3]) -> Self {
        Self { size, store: VoxelStore::Chunked { height: size[1], columns: HashMap::new() }, regions: HashMap::new(), dirty: Default::default() }
    }

    fn region_rows(&self) -> i32 {
        self.size[1].div_ceil(VOXEL_REGION) as i32
    }

    /// Marks every region of column `(cx, cz)` for re-meshing (if loaded).
    fn dirty_column(&mut self, cx: i32, cz: i32) {
        if let VoxelStore::Chunked { columns, .. } = &self.store {
            if !columns.contains_key(&(cx, cz)) {
                return;
            }
        }
        for ry in 0..self.region_rows() {
            self.dirty.insert([cx, ry, cz]);
        }
    }

    /// Whether a dirty region can be meshed now: always for fixed worlds;
    /// for streamed ones, once the 4 neighbouring columns are loaded (a
    /// column at the edge of the loaded area waits — like Minecraft, the
    /// outermost ring of loaded chunks isn't drawn).
    fn meshable(&self, region: &[i32; 3]) -> bool {
        match &self.store {
            VoxelStore::Dense(_) => true,
            VoxelStore::Chunked { columns, .. } => [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)].iter().all(|(dx, dz)| columns.contains_key(&(region[0] + dx, region[2] + dz))),
        }
    }

    fn set_chunk(&mut self, cx: i32, cz: i32, mut blocks: Vec<u8>) {
        let VoxelStore::Chunked { height, columns } = &mut self.store else { return };
        blocks.resize((VOXEL_REGION * VOXEL_REGION * *height) as usize, 0);
        let replaced = columns.insert((cx, cz), blocks).is_some();
        // A new column only needs meshing itself (neighbours that were
        // waiting for it are still dirty); replacing one changes what its
        // neighbours see across the seams.
        if replaced {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    self.dirty_column(cx + dx, cz + dz);
                }
            }
        } else {
            self.dirty_column(cx, cz);
        }
    }

    fn unload_chunk(&mut self, cx: i32, cz: i32) {
        let VoxelStore::Chunked { columns, .. } = &mut self.store else { return };
        if columns.remove(&(cx, cz)).is_none() {
            return;
        }
        self.regions.retain(|r, _| (r[0], r[2]) != (cx, cz));
        self.dirty.retain(|r| (r[0], r[2]) != (cx, cz));
        // Neighbours keep their meshes (unloading is far from the player,
        // behind the fog).
    }

    /// Sets a block and marks every region whose mesh it can change: its
    /// own, plus the neighbours it borders (their faces and AO look one
    /// block across the seam). `false` outside the world (or in a chunk
    /// that isn't loaded).
    fn set(&mut self, x: i32, y: i32, z: i32, block: u8) -> bool {
        use an1mbyt3_geometry::BlockSource;
        let before = self.store.block(x, y, z);
        let r = VOXEL_REGION as i32;
        let stored = match &mut self.store {
            VoxelStore::Dense(grid) => grid.set(x, y, z, block),
            VoxelStore::Chunked { height, columns } => {
                if y < 0 || y >= *height as i32 {
                    false
                } else if let Some(c) = columns.get_mut(&(x.div_euclid(r), z.div_euclid(r))) {
                    c[(x.rem_euclid(r) + r * (z.rem_euclid(r) + r * y)) as usize] = block;
                    true
                } else {
                    false
                }
            }
        };
        if !stored {
            return false;
        }
        if before == block {
            return true;
        }
        for dy in -1..=1 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let (px, py, pz) = (x + dx, y + dy, z + dz);
                    if py >= 0 {
                        self.dirty.insert([px.div_euclid(r), py / r, pz.div_euclid(r)]);
                    }
                }
            }
        }
        true
    }
}

/// The six clip planes (`ax + by + cz + d >= 0` inside) of a view-projection
/// matrix with a 0..1 depth range (Gribb / Hartmann).
fn frustum_planes(m: glam::Mat4) -> [glam::Vec4; 6] {
    let (r0, r1, r2, r3) = (m.row(0), m.row(1), m.row(2), m.row(3));
    [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2]
}

/// The scene view's floor grid (not a primitive).
const GRID_MESH_KEY: Mesh3DKey = (u8::MAX, [0; 4]);

fn mesh3d_key(primitive: &an1mbyt3_core::Primitive3D) -> Mesh3DKey {
    use an1mbyt3_core::Primitive3D as P;
    match *primitive {
        P::Cube { size } => (0, [size.x.to_bits(), size.y.to_bits(), size.z.to_bits(), 0]),
        P::Sphere { radius, segments, rings } => (1, [radius.to_bits(), segments, rings, 0]),
        P::Plane { size, subdivisions } => (2, [size.to_bits(), subdivisions, 0, 0]),
        P::Cylinder { radius, height, segments } => (3, [radius.to_bits(), height.to_bits(), segments, 0]),
        P::Cone { radius, height, segments } => (4, [radius.to_bits(), height.to_bits(), segments, 0]),
        P::Torus { radius, tube, segments, sides } => (5, [radius.to_bits(), tube.to_bits(), segments, sides]),
        // Models draw through `model_draws`; their primitive mesh doesn't
        // depend on which model it is.
        P::Model { .. } => (6, [0; 4]),
        // Block worlds draw through `voxel_draws` (meshed per region).
        P::Voxels { width, height, depth } => (7, [width, height, depth, 0]),
        // Glass draws through `glass_draws` (its live, breakable state);
        // only the shape matters here.
        P::Glass { width, height, thickness, shard, seed, anchors } => (8, [width.to_bits() ^ thickness.to_bits().rotate_left(7), height.to_bits() ^ shard.to_bits().rotate_left(13), seed, anchors]),
    }
}

/// `gradient` with its points carried through `m` (e.g. one node's space
/// to another's). A radial's radius is the start→end distance, so it scales
/// with `m` too.
fn map_gradient(gradient: &an1mbyt3_core::GradientDef, m: glam::Affine2) -> an1mbyt3_core::GradientDef {
    let mut g = gradient.clone();
    g.start = m.transform_point2(g.start);
    g.end = m.transform_point2(g.end);
    g
}

/// Hands out draw bindings during the (immutable) scene walk — draw-effect
/// copies each need their own (one binding per draw). Bindings are boxed
/// and never removed, so references stay valid; `reset` (each frame, with
/// `&mut`) just starts handing them out again from the first.
#[derive(Default)]
pub(crate) struct BindingArena {
    items: std::cell::RefCell<Vec<Box<DrawBinding>>>,
    next: std::cell::Cell<usize>,
}

impl BindingArena {
    fn reset(&mut self) {
        self.next.set(0);
    }
    fn get<'a>(&'a self, renderer: &Renderer) -> &'a DrawBinding {
        let i = self.next.get();
        self.next.set(i + 1);
        let mut items = self.items.borrow_mut();
        while items.len() <= i {
            items.push(Box::new(renderer.create_draw_binding()));
        }
        let ptr: *const DrawBinding = &*items[i];
        // SAFETY: each binding is boxed (its address never moves when the
        // vec grows) and items are only ever appended — never removed or
        // replaced — so the reference lives as long as `self` does.
        unsafe { &*ptr }
    }
}

/// The sorting layer `node` draws on: its own, else its nearest
/// ancestor's, else Normal (0).
fn effective_layer(artboard: &an1mbyt3_core::Artboard, node: NodeId) -> u32 {
    let mut at = Some(node);
    while let Some(id) = at {
        let Some(n) = artboard.node(id) else { break };
        if let Some(layer) = n.sorting_layer() {
            return layer;
        }
        at = n.parent;
    }
    0
}

/// The sorting layers anything in `artboard` is on, in `order` (unknown
/// ids after, Normal always included).
fn used_layers(artboard: &an1mbyt3_core::Artboard, order: &[u32]) -> Vec<u32> {
    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    seen.insert(0);
    for id in artboard.node_ids() {
        if let Some(l) = artboard.node(id).and_then(|n| n.sorting_layer()) {
            seen.insert(l);
        }
    }
    let mut out: Vec<u32> = order.iter().copied().filter(|l| seen.contains(l)).collect();
    let mut rest: Vec<u32> = seen.into_iter().filter(|l| !order.contains(l)).collect();
    rest.sort();
    out.extend(rest);
    out
}

/// Appends one walk's draws onto another's, keeping each island's place
/// among the unmasked draws (`SceneOp::Begin` indices shift by the
/// unmasked draws already there).
fn append_scene_parts<'a>(unmasked: &mut Vec<DrawCall<'a>>, masked: &mut Vec<an1mbyt3_renderer::SceneOp<'a>>, u: Vec<DrawCall<'a>>, m: Vec<an1mbyt3_renderer::SceneOp<'a>>) {
    let offset = unmasked.len();
    unmasked.extend(u);
    masked.extend(m.into_iter().map(|op| match op {
        an1mbyt3_renderer::SceneOp::Begin { island, after } => an1mbyt3_renderer::SceneOp::Begin { island, after: after + offset },
        other => other,
    }));
}

/// A Transform2D from an affine (translation, rotation, scale; a mirror
/// shows as a negative y scale; skew is dropped).
fn transform_from_affine(m: glam::Affine2) -> an1mbyt3_core::Transform2D {
    let x = m.matrix2.x_axis;
    let y = m.matrix2.y_axis;
    let sx = x.length().max(1e-9);
    let sy = (x.x * y.y - x.y * y.x) / sx;
    an1mbyt3_core::Transform2D { translation: m.translation, rotation: x.y.atan2(x.x), scale: Vec2::new(sx, sy), anchor: Vec2::ZERO }
}

/// Does `node` carry any draw effect?
fn has_draw_effect(node: &Node) -> bool {
    node.drop_shadow().is_some() || node.outline2d().is_some() || node.repeater().is_some() || node.mirror2d().is_some() || node.wiggle().is_some() || node.parallax().is_some() || node.pixel_snap().is_some()
}

/// Applies the draw effects of `node_id` and its ancestors (nearest first)
/// to its draws — see `an1mbyt3_core::effects2d`. Effects on a group act
/// on everything inside it, about the group's own origin.
fn apply_draw_effects<'a>(editor: &'a WasmEditor, artboard: &'a an1mbyt3_core::Artboard, node_id: NodeId, draws: Vec<DrawCall<'a>>) -> Vec<DrawCall<'a>> {
    use glam::Affine2;
    if draws.is_empty() {
        return draws;
    }
    let mut chain: Vec<(NodeId, &Node)> = Vec::new();
    let mut at = Some(node_id);
    while let Some(id) = at {
        let Some(n) = artboard.node(id) else { break };
        if has_draw_effect(n) {
            chain.push((id, n));
        }
        at = n.parent;
    }
    if chain.is_empty() {
        return draws;
    }
    let affine = |t: &an1mbyt3_core::Transform2D| t.to_affine2();
    // A copy of `d` with world affine `m` (and its own binding).
    let copy = |d: &DrawCall<'a>, m: Affine2| DrawCall { transform: transform_from_affine(m), binding: editor.effect_bindings.get(&editor.renderer), ..*d };
    let mut draws = draws;
    let t = editor.shader_clock;
    for (owner, n) in chain {
        let world = artboard.world_transform(owner).to_affine2();
        let inv = world.inverse();
        // About the owner's own origin: M = world · D · world⁻¹.
        let about = |d: Affine2| world * d * inv;
        if let Some(w) = n.wiggle() {
            let f = t * w.frequency.max(0.0);
            let s = w.seed;
            let d = Affine2::from_scale_angle_translation(
                Vec2::splat(1.0 + w.scale * an1mbyt3_core::noise1(f, 3, s)),
                (w.rotation * an1mbyt3_core::noise1(f, 2, s)).to_radians(),
                Vec2::new(an1mbyt3_core::noise1(f, 0, s), an1mbyt3_core::noise1(f, 1, s)) * w.position,
            );
            let m = about(d);
            for dr in &mut draws {
                dr.transform = transform_from_affine(m * affine(&dr.transform));
            }
        }
        if let Some(p) = n.parallax() {
            let shift = editor.effect_camera_pan.get() * Vec2::from(p.factor);
            for dr in &mut draws {
                dr.transform.translation += shift;
            }
        }
        if let Some(mirror) = n.mirror2d() {
            let mut extra = Vec::new();
            let flips: Vec<Vec2> = [(mirror.x, Vec2::new(-1.0, 1.0)), (mirror.y, Vec2::new(1.0, -1.0)), (mirror.x && mirror.y, Vec2::new(-1.0, -1.0))].into_iter().filter(|(on, _)| *on).map(|(_, s)| s).collect();
            for s in flips {
                let m = about(Affine2::from_scale(s));
                extra.extend(draws.iter().map(|d| copy(d, m * affine(&d.transform))));
            }
            draws.extend(extra);
        }
        if let Some(r) = n.repeater() {
            let count = r.count.clamp(1, 64);
            let step = Affine2::from_scale_angle_translation(Vec2::splat(r.scale.max(0.01)), r.rotation.to_radians(), Vec2::from(r.offset));
            let mut copies = Vec::new();
            // Back to front: the last copy first, the original on top.
            for k in (1..count).rev() {
                let mut d = Affine2::IDENTITY;
                for _ in 0..k {
                    d = d * step;
                }
                let m = about(d);
                let fade = 1.0 + (r.end_opacity.clamp(0.0, 1.0) - 1.0) * k as f32 / (count - 1).max(1) as f32;
                for dr in &draws {
                    let mut c = copy(dr, m * affine(&dr.transform));
                    c.fill_color[3] *= fade;
                    copies.push(c);
                }
            }
            copies.extend(draws);
            draws = copies;
        }
        // Silhouette copies behind: a flat colour (no gradient / material).
        let silhouette = |d: &DrawCall<'a>, offset: Vec2, color: [f32; 4]| {
            let mut c = copy(d, Affine2::from_translation(offset) * affine(&d.transform));
            c.gradient = None;
            c.material = None;
            c.blend_mode = BlendMode::Normal;
            c.fill_color = if c.texture.is_some() { [color[0], color[1], color[2], color[3]] } else { [color[0], color[1], color[2], color[3] * d.fill_color[3].clamp(0.0, 1.0)] };
            c
        };
        let paintable = |d: &DrawCall<'a>| d.material.is_none();
        let mut behind: Vec<DrawCall<'a>> = Vec::new();
        if let Some(s) = n.drop_shadow() {
            let base = Vec2::from(s.offset);
            let ring: Vec<Vec2> = if s.softness > 0.5 { (0..8).map(|i| { let a = i as f32 / 8.0 * std::f32::consts::TAU; Vec2::new(a.cos(), a.sin()) * s.softness * 0.5 }).chain(std::iter::once(Vec2::ZERO)).collect() } else { vec![Vec2::ZERO] };
            let mut c = s.color;
            if ring.len() > 1 {
                c[3] = 1.0 - (1.0 - c[3].clamp(0.0, 0.999)).powf(1.0 / ring.len() as f32);
            }
            for o in &ring {
                behind.extend(draws.iter().filter(|d| paintable(d)).map(|d| silhouette(d, base + *o, c)));
            }
        }
        if let Some(o) = n.outline2d() {
            let dirs = if o.smooth { 16 } else { 8 };
            for i in 0..dirs {
                let a = i as f32 / dirs as f32 * std::f32::consts::TAU;
                // 8-way pixel outlines use whole-pixel king moves.
                let v = if o.smooth { Vec2::new(a.cos(), a.sin()) } else { Vec2::new(a.cos().round(), a.sin().round()) };
                behind.extend(draws.iter().filter(|d| paintable(d)).map(|d| silhouette(d, v * o.width, o.color)));
            }
        }
        if !behind.is_empty() {
            behind.extend(draws);
            draws = behind;
        }
        if let Some(p) = n.pixel_snap() {
            let g = p.grid.max(0.01);
            let step = p.angle_step.to_radians();
            for dr in &mut draws {
                dr.transform.translation = (dr.transform.translation / g).round() * g;
                if step > 0.0 {
                    dr.transform.rotation = (dr.transform.rotation / step).round() * step;
                }
            }
        }
    }
    draws
}

/// A path reshaped by a Stroke Style: trimmed to its `trim_start..trim_end`
/// window (by length, slid by `trim_offset`), then cut into dashes. The
/// result is open polylines, ready to stroke.
fn style_stroke_path(path: &an1mbyt3_core::Path, style: &an1mbyt3_core::StrokeStyle2D) -> an1mbyt3_core::Path {
    use an1mbyt3_core::PathCommand as C;
    // Flatten into polylines (cubics sampled).
    let mut lines: Vec<Vec<Vec2>> = Vec::new();
    let mut cur = Vec2::ZERO;
    let mut start = Vec2::ZERO;
    for c in &path.commands {
        match *c {
            C::MoveTo(p) => {
                lines.push(vec![p]);
                cur = p;
                start = p;
            }
            C::LineTo(p) => {
                if lines.is_empty() {
                    lines.push(vec![cur]);
                }
                lines.last_mut().unwrap().push(p);
                cur = p;
            }
            C::CubicTo { control1: ctrl1, control2: ctrl2, to } => {
                if lines.is_empty() {
                    lines.push(vec![cur]);
                }
                let l = lines.last_mut().unwrap();
                for i in 1..=16 {
                    let t = i as f32 / 16.0;
                    let u = 1.0 - t;
                    l.push(cur * u * u * u + ctrl1 * 3.0 * u * u * t + ctrl2 * 3.0 * u * t * t + to * t * t * t);
                }
                cur = to;
            }
            C::Close => {
                if let Some(l) = lines.last_mut() {
                    l.push(start);
                }
                cur = start;
            }
        }
    }
    // Every segment, with its distance along the whole path.
    let mut segments: Vec<(Vec2, Vec2, f32, usize)> = Vec::new();
    let mut total = 0.0;
    for (li, l) in lines.iter().enumerate() {
        for w in l.windows(2) {
            let len = w[0].distance(w[1]);
            if len > 1e-6 {
                segments.push((w[0], w[1], total, li));
                total += len;
            }
        }
    }
    if total <= 0.0 {
        return path.clone();
    }
    // The visible window(s), in length units (may wrap).
    let mut a = style.trim_start.min(style.trim_end).clamp(0.0, 1.0);
    let mut b = style.trim_start.max(style.trim_end).clamp(0.0, 1.0);
    let off = style.trim_offset.rem_euclid(1.0);
    a += off;
    b += off;
    let mut windows: Vec<(f32, f32)> = Vec::new();
    if b - a >= 0.9999 {
        windows.push((0.0, total));
    } else if b <= 1.0 {
        windows.push((a * total, b * total));
    } else if a >= 1.0 {
        windows.push(((a - 1.0) * total, (b - 1.0) * total));
    } else {
        windows.push((a * total, total));
        windows.push((0.0, (b - 1.0) * total));
    }
    windows.sort_by(|x, y| x.0.total_cmp(&y.0));
    // Dash pattern (0 dash = solid).
    let period = style.dash.max(0.0) + style.gap.max(0.0);
    let dashed = style.dash > 0.0 && style.gap > 0.0;
    let on = |s: f32| -> bool { !dashed || (s + style.dash_offset).rem_euclid(period) < style.dash };
    let mut out: Vec<C> = Vec::new();
    let mut pen_down = false;
    let mut last_line = usize::MAX;
    for (p0, p1, d0, li) in segments {
        let len = p0.distance(p1);
        // Split each segment at window and dash boundaries.
        let mut cuts: Vec<f32> = vec![0.0, len];
        for (wa, wb) in &windows {
            for edge in [*wa, *wb] {
                if edge > d0 && edge < d0 + len {
                    cuts.push(edge - d0);
                }
            }
        }
        if dashed {
            let mut k = ((d0 + style.dash_offset) / period).floor() * period - style.dash_offset;
            while k < d0 + len {
                for edge in [k, k + style.dash] {
                    if edge > d0 && edge < d0 + len {
                        cuts.push(edge - d0);
                    }
                }
                k += period;
            }
        }
        cuts.sort_by(|x, y| x.total_cmp(y));
        if li != last_line {
            pen_down = false;
            last_line = li;
        }
        for w in cuts.windows(2) {
            let (s0, s1) = (w[0], w[1]);
            if s1 - s0 < 1e-5 {
                continue;
            }
            let mid = d0 + (s0 + s1) * 0.5;
            let visible = windows.iter().any(|(wa, wb)| mid >= *wa && mid <= *wb) && on(mid);
            let q0 = p0.lerp(p1, s0 / len);
            let q1 = p0.lerp(p1, s1 / len);
            if visible {
                if !pen_down {
                    out.push(C::MoveTo(q0));
                    pen_down = true;
                }
                out.push(C::LineTo(q1));
            } else {
                pen_down = false;
            }
        }
    }
    an1mbyt3_core::Path { commands: out }
}

/// A path as a polyline with cumulative lengths (for arc-length lookups).
struct Polyline {
    points: Vec<Vec2>,
    /// `lengths[i]` = distance from the start to `points[i]`.
    lengths: Vec<f32>,
}

impl Polyline {
    fn total(&self) -> f32 {
        self.lengths.last().copied().unwrap_or(0.0)
    }
    /// Point and unit tangent at distance `d` along the line.
    fn at(&self, d: f32) -> (Vec2, Vec2) {
        let n = self.points.len();
        if n < 2 {
            return (self.points.first().copied().unwrap_or(Vec2::ZERO), Vec2::X);
        }
        let d = d.clamp(0.0, self.total());
        let i = self.lengths.partition_point(|&l| l <= d).clamp(1, n - 1);
        let (a, b) = (self.points[i - 1], self.points[i]);
        let span = (self.lengths[i] - self.lengths[i - 1]).max(1e-6);
        let t = (d - self.lengths[i - 1]) / span;
        (a.lerp(b, t), (b - a).normalize_or(Vec2::X))
    }
}

/// The first subpath of Shape node `id`'s outline (cubics sampled), in
/// world space — or in an emitter's local space when `emitter` is given.
fn path_polyline(artboard: &an1mbyt3_core::Artboard, id: NodeId, emitter: Option<an1mbyt3_core::Transform2D>) -> Option<Polyline> {
    use an1mbyt3_core::PathCommand as C;
    let node = artboard.node(id)?;
    let NodeKind::Shape(shape) = &node.kind else { return None };
    let world = artboard.try_world_transform(id)?.to_affine2();
    let mut pts: Vec<Vec2> = Vec::new();
    let (mut cur, mut start) = (Vec2::ZERO, Vec2::ZERO);
    for c in &shape.path.commands {
        match *c {
            C::MoveTo(p) => {
                if !pts.is_empty() {
                    break;
                }
                pts.push(p);
                cur = p;
                start = p;
            }
            C::LineTo(p) => {
                pts.push(p);
                cur = p;
            }
            C::CubicTo { control1, control2, to } => {
                for i in 1..=24 {
                    let t = i as f32 / 24.0;
                    let u = 1.0 - t;
                    pts.push(cur * u * u * u + control1 * 3.0 * u * u * t + control2 * 3.0 * u * t * t + to * t * t * t);
                }
                cur = to;
            }
            C::Close => {
                pts.push(start);
                break;
            }
        }
    }
    let points: Vec<Vec2> = pts
        .into_iter()
        .map(|p| {
            let w = world.transform_point2(p);
            match emitter {
                Some(e) => world_to_emitter(e, w),
                None => w,
            }
        })
        .collect();
    if points.len() < 2 {
        return None;
    }
    let mut lengths = Vec::with_capacity(points.len());
    let mut total = 0.0;
    lengths.push(0.0);
    for w in points.windows(2) {
        total += w[0].distance(w[1]);
        lengths.push(total);
    }
    (total > 1e-3).then_some(Polyline { points, lengths })
}

/// Particle Path (see `an1mbyt3_core::ParticlePath2D`): each particle has
/// a place on the path for its age, an offset across the stream (its own
/// random lane) plus turbulence; once it starts joining it eases onto that
/// spot and takes the stream's velocity. Strays stay free.
fn apply_particle_path(cfg: &an1mbyt3_core::ParticlePath2D, line: &Polyline, particles: &mut [an1mbyt3_runtime::Particle], time: f32, dt: f32) {
    if dt <= 0.0 {
        return;
    }
    let total = line.total();
    let start = cfg.start.clamp(0.0, 1.0) * total;
    for p in particles.iter_mut() {
        // A stable per-particle random (lifetime and size are drawn at spawn).
        let seed = {
            let mut x = (p.lifetime.to_bits() as u64) << 32 | p.base_size.to_bits() as u64;
            x ^= x >> 33;
            x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
            x ^= x >> 33;
            x
        };
        let rand = |k: u32| (((seed >> (k * 13)) & 0xffff) as f32) / 65535.0;
        if rand(0) < cfg.stray.clamp(0.0, 1.0) {
            continue;
        }
        let life = p.normalized_age();
        let join = ((life - cfg.join_delay.clamp(0.0, 1.0)) / cfg.join_time.max(1e-3)).clamp(0.0, 1.0);
        let join = join * join * (3.0 - 2.0 * join);
        let pull = cfg.weight.clamp(0.0, 1.0) * join;
        if pull <= 0.0 {
            continue;
        }
        // Where on the path this particle should be now, and how fast the
        // stream moves there.
        let (along, stream_speed) = if cfg.speed > 0.0 {
            (start + p.age * cfg.speed, cfg.speed)
        } else {
            let span = (total - start).max(1e-3);
            (start + life * span, span / p.lifetime.max(1e-3))
        };
        let along = if cfg.loop_path { along.rem_euclid(total.max(1e-3)) } else { along.min(total) };
        let (point, tangent) = line.at(along);
        let normal = Vec2::new(-tangent.y, tangent.x);
        let progress = along / total.max(1e-3);
        let width = cfg.spread * (1.0 + (cfg.widen - 1.0) * progress);
        let lane = (rand(1) * 2.0 - 1.0) * width;
        let tt = time * cfg.turbulence_speed.max(0.0) + rand(2) * 100.0;
        let wobble = Vec2::new(an1mbyt3_core::noise1(tt, 0, seed as u32), an1mbyt3_core::noise1(tt, 1, seed as u32)) * cfg.turbulence;
        let target = point + normal * lane + wobble;
        // Frame-rate-independent ease onto the spot (1 = locked on).
        let k = if pull >= 0.999 { 1.0 } else { 1.0 - (1.0 - pull).powf(dt * 10.0) };
        p.position = p.position.lerp(target, k);
        let flow = if along >= total && !cfg.loop_path { Vec2::ZERO } else { tangent * stream_speed };
        p.velocity = p.velocity.lerp(flow, k);
    }
}

/// Mask 2D on `node`: writes its stencil mask and returns the
/// `(depth, island)` its own drawing and children use (a new island at the
/// top level). An inverted mask ("no-go area") first fills the whole
/// screen, then cuts the shape back out, so it nests like any other clip.
#[allow(clippy::too_many_arguments)]
fn write_node_masks<'a>(
    editor: &'a WasmEditor,
    artboard: &'a an1mbyt3_core::Artboard,
    index: &an1mbyt3_core::SceneIndex,
    node_id: NodeId,
    node: &'a Node,
    depth: u32,
    island: u32,
    next_island: &mut u32,
    unmasked: &mut Vec<DrawCall<'a>>,
    masked: &mut Vec<an1mbyt3_renderer::SceneOp<'a>>,
) -> (u32, u32) {
    let mask = node.mask2d().filter(|m| m.mask.is_some());
    let Some(quad) = editor.mask_quad.as_ref() else { return (depth, island) };
    if mask.is_none() || index.is_mask_target(node_id) {
        return (depth, island);
    }
    let mut depth = depth;
    let island = if depth == 0 {
        let fresh = *next_island;
        *next_island += 1;
        masked.push(an1mbyt3_renderer::SceneOp::Begin { island: fresh, after: unmasked.len() });
        fresh
    } else {
        island
    };
    const BIG: f32 = 1.0e6;
    let quad_draw = |transform: an1mbyt3_core::Transform2D| DrawCall {
        mesh: quad,
        binding: editor.effect_bindings.get(&editor.renderer),
        transform,
        fill_color: [1.0; 4],
        blend_mode: BlendMode::Normal,
        gradient: None,
        texture: None,
        material: None,
    };
    if let (Some(m), Some(mask_id)) = (mask, mask.and_then(|m| m.mask)) {
        if m.invert {
            // Everything first …
            masked.push(an1mbyt3_renderer::SceneOp::WriteMask { mask: quad_draw(an1mbyt3_core::Transform2D { scale: Vec2::splat(BIG), ..Default::default() }), depth, island });
            depth += 1;
        }
        // … the mask shape(s): a group counts every shape inside.
        let mut shapes = Vec::new();
        mask_subtree_draws(editor, artboard, index, mask_id, &mut shapes);
        for draw in shapes {
            masked.push(an1mbyt3_renderer::SceneOp::WriteMask { mask: draw, depth, island });
        }
        if !m.invert {
            depth += 1;
        }
        // Inverted: content stays at the "everything" depth, which the
        // shape's area has now left (it was bumped one higher).
    }
    (depth, island)
}

/// A mask node's draws and its descendants' (visible or not — a mask is
/// usually a hidden shape).
fn mask_subtree_draws<'a>(editor: &'a WasmEditor, artboard: &'a an1mbyt3_core::Artboard, index: &an1mbyt3_core::SceneIndex, id: NodeId, out: &mut Vec<DrawCall<'a>>) {
    let Some(node) = artboard.node(id) else { return };
    if node.deleted {
        return;
    }
    out.extend(node_draws_plain(editor, artboard, id, node));
    for &child in index.children(Some(id)) {
        mask_subtree_draws(editor, artboard, index, child, out);
    }
}

/// Is `node` (or an ancestor) the shape some Mask 2D masks with? Such
/// shapes only shape the mask — they don't draw themselves.
fn is_mask2d_shape(editor: &WasmEditor, artboard: &an1mbyt3_core::Artboard, node: NodeId) -> bool {
    if editor.mask2d_targets.is_empty() {
        return false;
    }
    let mut at = Some(node);
    while let Some(id) = at {
        if editor.mask2d_targets.contains(&id) {
            return true;
        }
        at = artboard.node(id).and_then(|n| n.parent);
    }
    false
}

/// A frame node's draws and its descendants', in world space, visible or
/// not (flipbook frames are hidden as nodes; they're drawn as particle art
/// or used as goo outlines).
fn frame_subtree_draws<'a>(editor: &'a WasmEditor, artboard: &'a an1mbyt3_core::Artboard, id: NodeId, out: &mut Vec<DrawCall<'a>>) {
    let Some(node) = artboard.node(id) else { return };
    if node.deleted {
        return;
    }
    out.extend(node_draws_plain(editor, artboard, id, node));
    for (child, _) in artboard.children_of(id) {
        frame_subtree_draws(editor, artboard, child, out);
    }
}

/// Particles drawn as a Flipbook's frames: each particle shows frame
/// `book.frame_at(its age)`, the frame art (in the emitter's space) fitted
/// so the first frame's size matches the particle's size, turned with the
/// particle and tinted by its colour over life.
fn flipbook_particle_draws<'a>(
    editor: &'a WasmEditor,
    artboard: &'a an1mbyt3_core::Artboard,
    node_id: NodeId,
    config: &'a an1mbyt3_core::ParticleSystemNode,
    book: &an1mbyt3_core::Flipbook,
    frames: &[NodeId],
    particles: &[an1mbyt3_runtime::Particle],
) -> Vec<DrawCall<'a>> {
    let emitter = artboard.world_transform(node_id);
    let to_emitter = emitter.to_affine2().inverse();
    // The art's size: the first frame's box (local to the emitter).
    let extent = {
        let (min, max) = crate::dto::local_bounds(artboard, frames[0]);
        let t = artboard.node(frames[0]).map_or(Vec2::ONE, |n| n.transform.scale.abs());
        ((max - min) * t).max_element().max(1.0)
    };
    // Each frame's draws once, in the emitter's space.
    let frame_draws: Vec<Vec<(DrawCall<'a>, glam::Affine2)>> = frames
        .iter()
        .map(|f| {
            let mut draws = Vec::new();
            frame_subtree_draws(editor, artboard, *f, &mut draws);
            draws.into_iter().map(|d| {
                let m = to_emitter * d.transform.to_affine2();
                (d, m)
            }).collect()
        })
        .collect();
    let (sin, cos) = emitter.rotation.sin_cos();
    let mut out = Vec::new();
    for p in particles {
        let age = p.normalized_age();
        let size = p.base_size * sample_size_over_lifetime(&config.size_over_lifetime, age);
        let tint = sample_color_over_lifetime(&config.color_over_lifetime, age).unwrap_or([1.0; 4]);
        let color = [0, 1, 2, 3].map(|i| config.main.start_color[i] * tint[i]);
        let (position, rotation, scale) = match config.main.simulation_space {
            an1mbyt3_core::SimulationSpace::World => (p.position, p.rotation, size),
            an1mbyt3_core::SimulationSpace::Local => {
                let scaled = p.position * emitter.scale;
                (emitter.translation + Vec2::new(scaled.x * cos - scaled.y * sin, scaled.x * sin + scaled.y * cos), emitter.rotation + p.rotation, size * (emitter.scale.x.abs() + emitter.scale.y.abs()) * 0.5)
            }
        };
        let place = glam::Affine2::from_scale_angle_translation(Vec2::splat(scale / extent), rotation, position);
        let frame = book.frame_at(p.age, frames.len());
        for (d, m) in &frame_draws[frame] {
            let mut c = DrawCall { binding: editor.effect_bindings.get(&editor.renderer), transform: transform_from_affine(place * *m), ..*d };
            for i in 0..4 {
                c.fill_color[i] *= color[i];
            }
            out.push(c);
        }
    }
    out
}

/// Is `node` inside a goo member that's a Flipbook with Hide source (its
/// frames are only the liquid's outline)?
fn hidden_by_goo_flipbook(editor: &WasmEditor, artboard: &an1mbyt3_core::Artboard, node: NodeId) -> bool {
    let mut at = artboard.node(node).and_then(|n| n.parent);
    while let Some(id) = at {
        let Some(n) = artboard.node(id) else { break };
        // (Only while the goo is actually running.)
        if n.flipbook().is_some() && n.goo().is_some_and(|g| g.hide_source && (editor.effects_live() || editor.goo_preview.contains(&g.group))) {
            return true;
        }
        at = n.parent;
    }
    false
}

/// Design-time preview: particles / fluid / goo on `node` run while it's
/// selected, or the selection is inside it (a flipbook frame) or above it
/// (its group).
fn fx_preview(artboard: &an1mbyt3_core::Artboard, selected: Option<NodeId>, node: NodeId) -> bool {
    let Some(selected) = selected else { return false };
    let is_ancestor = |above: NodeId, below: NodeId| {
        let mut at = Some(below);
        while let Some(id) = at {
            if id == above {
                return true;
            }
            at = artboard.node(id).and_then(|n| n.parent);
        }
        false
    };
    is_ancestor(selected, node) || is_ancestor(node, selected)
}
