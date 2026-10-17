//! `an1mbyt3-wasm`: JavaScript/WebAssembly bindings for the browser runtime.
//!
//! See AN1MBYT3_BUILD_PLAN.md §1 (process/compilation topology) and §8
//! ("Developer Experience") — this crate is the thin `wasm-bindgen` surface over
//! the real engine crates. It must stay thin: no animation logic lives here, only
//! marshalling between JS and the same `an1mbyt3-core`/`an1mbyt3-runtime`/
//! `an1mbyt3-constraints`/`an1mbyt3-renderer`/`an1mbyt3-text` used natively by
//! the desktop editor, so desktop-authored and web-played animations never diverge.
//!
//! The hand-written TypeScript wrapper (`packages/runtime-js`, `packages/runtime-react`)
//! will eventually sit on top of this crate's raw exports — consumers never touch
//! `wasm-bindgen` glue directly (§8). Those packages don't exist yet; today the
//! only consumer is `apps/web-test`, a hand-written HTML/JS page used purely to
//! get a real browser window on screen for visual confirmation (mirroring how
//! every native phase got a winit example for the same reason).
//!
//! Phase 8 exit criterion (AN1MBYT3_BUILD_PLAN.md §10): "`<Animation>`
//! plays a real `.teab` file in a browser with working state-machine input."
//! `WasmPlayer::create` still starts every player on the same small
//! hand-built demo scene `an1mbyt3-renderer`'s `state_machine` example
//! proves natively (so there's always something to look at before a real
//! file has loaded), but [`WasmPlayer::load_teab`] is what actually closes
//! this phase: it replaces that demo with an arbitrary, designer-authored
//! document decoded from real `.teab` bytes — any number of shape/text
//! nodes, whichever state machine (if any) the artboard was actually saved
//! with, driven by the same [`Driver`] the demo scene itself uses. This proves:
//! - `an1mbyt3-renderer::Renderer` (windowing-agnostic by design, see its
//!   module doc) accepts a `wgpu::Surface` built from a `<canvas>` exactly as
//!   readily as one built from a winit window.
//! - The exact same `StateMachine`/`Animation`/`Document` types that
//!   drive the desktop editor and native examples drive a browser frame too —
//!   no separate "web animation representation" exists or is needed.
//! - A JS-driven input (a button click calling `fire_trigger`) reaches the
//!   same `Inputs::fire_trigger` the native `state_machine` example's Space
//!   key reaches.
//! - Page-wide pointer awareness (`WasmPlayer::create`'s `track_pointer`
//!   flag) is an opt-in capability, not a bespoke per-demo hack: it just
//!   keeps `Inputs::set_number`-backed `"PointerX"`/`"PointerY"` inputs
//!   current from a `window`-level listener, so any authored component binds
//!   to the cursor the same way it binds to any other named input — through
//!   ordinary `ConditionDef`s, no special-cased engine API.
//!
//! Deferred to later: selecting a specific *named* state machine when an
//! artboard has more than one (`load_teab` always picks the first —
//! AN1MBYT3_BUILD_PLAN.md §8's `animation.stateMachine("Character")`
//! implies a name-addressed choice, not built here yet since nothing in
//! this codebase authors more than one state machine per artboard yet
//! either); the `packages/runtime-js`/`packages/runtime-react` wrappers
//! `<Animation>` implies sit on top of this crate's raw exports.
//!
//! `WasmEditor` (`editor.rs`) is a second, later consumer of this same
//! crate: the Tauri desktop editor's canvas, not just the web runtime. It
//! reuses everything above unchanged — same `Renderer`, same `Document` —
//! just holds a real editable document (undo/redo included) instead of a
//! read-only demo, and renders via a real surface-backed `Renderer`
//! instead of the headless readback-bridge `apps/editor/src-tauri` used to
//! use. See `editor.rs`'s module doc for why (in short: it's the
//! Rive/Figma-style architecture — the editor's canvas is just another wasm
//! consumer, same as any web page's, with no native process to bridge to).

// This crate is wasm32-only: `wgpu::SurfaceTarget::Canvas`/`web_sys::HtmlCanvasElement`
// usage below only exists when wgpu's own `cfg(web)` (target_arch = "wasm32" +
// its "web" feature) is active, so a native `cargo check --workspace` build of
// this crate would fail outright without this. Gating the entire crate body
// behind this `cfg` instead makes native builds see a valid, empty crate
// (compiles, exports nothing) rather than a compile error — the same idiom
// several wasm-only crates in the ecosystem use, rather than excluding this
// crate from `--workspace` invocations everywhere.
#![cfg(target_arch = "wasm32")]

mod bone_ik;
pub use bone_ik::WasmBoneIk;

mod demo_dungeon;
mod demo_match3;
mod demo_liquid;
mod demo_torture;
mod demo_3d;
mod demo_games;
mod demo_lab;
mod demo_lite;
mod demo_lite2;
mod demo_ui;
mod demo_responsive;
mod demo_html;
mod demo_physics3d;
mod demo_voxel;
mod demo_shatter;
mod demo_fight;
mod demo_boom;
mod demo_lan;
mod demo_cloth;
mod demo_trees;
mod demo_leer;
mod demo_goo;
mod demo_goo_shapes;
mod view3d;
mod demo_tactics_anim;
mod demo_tactics_3d;
mod demo_tactics;
mod demo_tactics_ui;
mod demo_fps;
mod demo_physics;
mod demo_imagefx;
mod demo_rig;
mod demo_stress;
mod prefab_ops;
mod profiler;
mod script_scene;
mod text_bounds;
mod particles3d;
mod metaball2d;
mod goo_gpu;
mod demo_ninja;
mod lighting2d;
mod dto;
mod editor;
pub use editor::WasmEditor;

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;

use glam::Vec2;
use an1mbyt3_core::{
    Animation, AnimationId, Artboard, ArtboardId, ConditionDef, Document, FontId, InputDef, InputKind, Node, NodeId, NodeKind, Path,
    PlaybackDirection, PropertyTrack, ShapeNode, StateDef, StateMachineDef, TransitionDef,
};
use an1mbyt3_geometry::tessellate_path;
use an1mbyt3_renderer::{BlendMode, Camera, DrawBinding, DrawCall, GpuMesh, Renderer};
use an1mbyt3_runtime::{Easing, Keyframe, LoopMode, Playback, StateMachine, Track};
use an1mbyt3_text::{shape_and_tessellate, Font};

/// Named numeric inputs (`an1mbyt3_runtime::state_machine::Inputs::set_number`
/// already supports arbitrary names — nothing new needed at the engine level)
/// that [`WasmPlayer::create`]'s `track_pointer` flag keeps updated every
/// frame with the page's live cursor position, in artboard coordinates
/// (origin centered, +y up — same convention every other example uses).
/// Component authors bind to these exactly like any other named input (e.g.
/// `ConditionDef::NumberGreaterThan { input: "PointerX".into(), .. }`), so
/// pointer-reactive behavior is a document-authoring choice, not a
/// per-component Rust API. Only actually fed into anything when
/// [`Driver::StateMachine`] is active — a bare-`Animation` or static driver
/// has no `Inputs` to feed at all.
const POINTER_X_INPUT: &str = "PointerX";
const POINTER_Y_INPUT: &str = "PointerY";

const DEMO_ARTBOARD_WIDTH: f32 = 400.0;
const DEMO_ARTBOARD_HEIGHT: f32 = 400.0;

/// Converts a Rust panic into a `console.error` log instead of the opaque
/// "unreachable executed" trap the browser shows by default — called once
/// automatically on module load via `#[wasm_bindgen(start)]`, not something
/// JS callers need to remember to invoke themselves.
#[wasm_bindgen(start)]
fn set_panic_hook() {
    console_error_panic_hook::set_once();
}

/// Builds the same two-state ("Idle" parked left, "Active" slid right and
/// eased) demo document/state-machine pair as `an1mbyt3-renderer`'s
/// `state_machine` example — see that file's module doc for why "Idle"
/// and "Active" each reference a real, saved `Artboard` `Animation` (via
/// `Artboard::add_animation`/`add_state_machine`) rather than a hand-built
/// `AnimationClip`: this is what makes the state machine a genuine document
/// round trip (`StateMachine::from_definition` builds the live instance
/// this function returns from exactly what was just saved), not just
/// hardcoded Rust structs with no relationship to anything a `.teab` could
/// ever carry.
fn build_scene() -> (Document, ArtboardId, StateMachine) {
    let mut document = Document::new();
    let artboard_id = document.add_artboard(Artboard::new("Main", DEMO_ARTBOARD_WIDTH, DEMO_ARTBOARD_HEIGHT));

    let artboard = document.artboard_mut(artboard_id).unwrap();
    let node_id = artboard.add_node(Node::new(
        "Square",
        None,
        NodeKind::Shape(ShapeNode {
            path: Path::rect(120.0, 120.0),
            fill_color: [0.85, 0.2, 0.35, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None }),
    ));

    let ease_in_out = Easing::CubicBezier { x1: 0.42, y1: 0.0, x2: 0.58, y2: 1.0 };

    let mut idle_animation = Animation::new("Idle");
    idle_animation.duration = 0.0;
    idle_animation
        .tracks
        .push((node_id, PropertyTrack::Translation(Track::new(vec![Keyframe::new(0.0, Vec2::new(-120.0, 0.0), Easing::Linear)]))));

    let mut active_animation = Animation::new("Active");
    active_animation.duration = 0.6;
    active_animation.tracks.push((
        node_id,
        PropertyTrack::Translation(Track::new(vec![
            Keyframe::new(0.0, Vec2::new(-120.0, 0.0), ease_in_out),
            Keyframe::new(0.6, Vec2::new(120.0, 0.0), ease_in_out),
        ])),
    ));

    let idle_animation_id = artboard.add_animation(idle_animation);
    let active_animation_id = artboard.add_animation(active_animation);

    let def = StateMachineDef {
        name: "Main".into(),
        inputs: vec![InputDef { name: "toggle".into(), kind: InputKind::Trigger }],
        states: vec![
            StateDef {
                name: "Idle".into(),
                animation: idle_animation_id,
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
                animation: active_animation_id,
                loop_mode: LoopMode::Once,
                transitions: vec![TransitionDef {
                    to: "Idle".into(),
                    condition: ConditionDef::Trigger { input: "toggle".into() },
                    direction: PlaybackDirection::Forward,
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
    let state_machine = StateMachine::from_definition(artboard.state_machine(state_machine_id).unwrap(), artboard);

    (document, artboard_id, state_machine)
}

/// A shaped-and-tessellated `NodeKind::Text` run's uploaded glyph meshes,
/// plus the `(content, size_px)` key they were shaped from — mirrors
/// `editor.rs`'s own `TextMeshCache` (kept as a separate copy rather than a
/// shared type: `editor.rs`'s version is private to that module, and this
/// player's render loop is deliberately simpler — no selection/motion-path
/// chrome to also account for — so sharing it would mean either exposing an
/// editor-internal type or building an abstraction neither caller actually
/// needs yet).
struct TextMeshCache {
    /// Content, size and font — a change to any of them re-shapes.
    key: (String, f32, FontId),
    glyphs: Vec<(GpuMesh, DrawBinding, Vec2)>,
}

/// What's actually advancing/sampling the artboard's current pose each
/// frame. A loaded (or demo) artboard can have zero, one, or more saved
/// `StateMachineDef`s/`Animation`s — [`pick_driver`] decides which single
/// thing drives playback, since `WasmPlayer` only ever plays one thing at a
/// time today (AN1MBYT3_BUILD_PLAN.md §8's name-addressed
/// `stateMachine("Character")` selection is a further increment, see this
/// module's doc comment).
enum Driver {
    StateMachine(StateMachine),
    /// A bare `Animation`, played directly with no state machine at all —
    /// the common case for a document that's just one continuous animation.
    /// `Animation` has no authored loop mode of its own yet (only a
    /// `StateDef` does), so this always loops — the sensible default for
    /// "just play this," not a placeholder for a real per-animation setting
    /// that doesn't exist yet.
    Clip { animation: AnimationId, playback: Playback },
    /// Nothing to animate (no state machine, no animation) — just render
    /// whatever static pose the artboard's nodes already have.
    Static,
}

impl Driver {
    fn advance_and_apply(&mut self, delta_seconds: f32, artboard: &mut Artboard) {
        match self {
            Driver::StateMachine(state_machine) => {
                state_machine.advance(delta_seconds, artboard);
                state_machine.apply(artboard);
            }
            Driver::Clip { animation, playback } => {
                playback.advance(delta_seconds);
                // Cloned out first — same borrow-shape reasoning
                // `StateMachine::apply` already documents (immutable lookup,
                // then `&mut Artboard` for `Animation::apply`, can't overlap).
                if let Some(animation) = artboard.animation(*animation).cloned() {
                    animation.apply(artboard, playback.time);
                }
            }
            Driver::Static => {}
        }
    }

    fn play(&mut self) {
        match self {
            Driver::StateMachine(state_machine) => state_machine.play(),
            Driver::Clip { playback, .. } => playback.play(),
            Driver::Static => {}
        }
    }

    fn pause(&mut self) {
        match self {
            Driver::StateMachine(state_machine) => state_machine.pause(),
            Driver::Clip { playback, .. } => playback.pause(),
            Driver::Static => {}
        }
    }

    fn seek(&mut self, time: f32) {
        match self {
            Driver::StateMachine(state_machine) => state_machine.seek(time),
            Driver::Clip { playback, .. } => playback.seek(time),
            Driver::Static => {}
        }
    }
}

/// Decides what should drive playback for a freshly built/loaded artboard —
/// its first saved state machine if it has one, otherwise its first
/// non-deleted animation (played on a loop — see [`Driver::Clip`]'s doc
/// comment for why), otherwise a static pose. Shared by [`build_scene`]'s
/// demo content and [`WasmPlayer::load_teab`], so a loaded real document is
/// driven by exactly the same selection rule the built-in demo already is.
fn pick_driver(artboard: &Artboard) -> Driver {
    if let Some((_, def)) = artboard.state_machines().next() {
        return Driver::StateMachine(StateMachine::from_definition(def, artboard));
    }
    if let Some((animation_id, animation)) = artboard.animations().next() {
        let mut playback = Playback::new(animation.duration, LoopMode::Loop);
        playback.play();
        return Driver::Clip { animation: animation_id, playback };
    }
    Driver::Static
}

/// The whole browser-side player: owns the `wgpu` surface/renderer plus the
/// document/[`Driver`] driving what it draws. One instance per `<canvas>`.
///
/// Deliberately holds no `std::time::Instant` — that type panics on
/// `wasm32-unknown-unknown` (no OS clock to read). Frame timing instead comes
/// from the caller's own `requestAnimationFrame` loop, which already has a
/// high-resolution timestamp; see [`WasmPlayer::render`]'s `delta_seconds`
/// parameter.
#[wasm_bindgen]
pub struct WasmPlayer {
    renderer: Renderer,
    /// Uploaded meshes for every `NodeKind::Shape` the current document's
    /// artboard has, keyed by node — populated lazily in `render` and fully
    /// cleared by `load_teab` (a new document's `NodeId`s have no
    /// relationship to the previous one's, see that method's doc comment).
    /// Same caching discipline `WasmEditor`'s own `mesh_cache` uses.
    mesh_cache: HashMap<NodeId, (GpuMesh, DrawBinding)>,
    font_cache: HashMap<FontId, Font>,
    text_mesh_cache: HashMap<NodeId, TextMeshCache>,
    /// Shared unit quad every `NodeKind::Image` node draws through — see
    /// `WasmEditor::image_quad_mesh`'s doc comment (same reasoning, same
    /// pattern, kept as a separate copy since this player's cache set is
    /// otherwise already a smaller, independent copy of the editor's).
    image_quad_mesh: Option<GpuMesh>,
    image_bindings: HashMap<NodeId, DrawBinding>,
    image_cache: HashMap<an1mbyt3_core::ImageId, an1mbyt3_renderer::GpuImageTexture>,
    document: Document,
    artboard_id: ArtboardId,
    driver: Driver,
    /// `alpha: 0.0` if the surface actually came back transparent (see
    /// `Renderer::new_transparent`'s doc comment), otherwise an opaque
    /// fallback background — computed once at `create` time rather than every
    /// frame since `Renderer::supports_transparency()` never changes after
    /// the surface is configured.
    clear_color: wgpu::Color,
    /// Live cursor position, updated by `pointer_listener` below — `None`
    /// unless `create`'s `track_pointer` flag was `true`. Stored in raw
    /// canvas-pixel coordinates (not yet converted to artboard space) since
    /// the conversion depends on the *current* artboard's width/height,
    /// which can change out from under a long-lived listener closure after
    /// `load_teab` swaps in a differently-sized document — `render` does the
    /// conversion itself, against whatever artboard is current that frame.
    /// A plain `Rc<Cell<_>>` (not a channel/queue) because only the latest
    /// position ever matters; `render` reads it once per frame.
    pointer_position: Rc<Cell<(f32, f32)>>,
    /// The `window`-level `mousemove` listener itself, kept alive for as long
    /// as this player exists — dropping a `wasm_bindgen::closure::Closure`
    /// invalidates the JS function it wraps, so letting this field drop
    /// silently (without also removing the listener, see this struct's
    /// `Drop` impl) would leave `window` holding a dangling callback. `None`
    /// when `track_pointer` was `false` — no listener is attached at all in
    /// that case, not just an unused one.
    pointer_listener: Option<Closure<dyn FnMut(web_sys::MouseEvent)>>,
}

#[wasm_bindgen]
impl WasmPlayer {
    /// Creates a player rendering into `canvas`, starting on the same small
    /// built-in demo scene as ever — call [`WasmPlayer::load_teab`]
    /// afterward to play a real authored file instead. Async because
    /// acquiring a `wgpu` adapter/device is async on every target (see
    /// `an1mbyt3_renderer::Renderer::new`'s doc comment) — JS callers
    /// `await` this like any other promise-returning call.
    ///
    /// Not named `new`/tagged `#[wasm_bindgen(constructor)]`: wasm-bindgen's
    /// constructor sugar doesn't support async functions, so this is a plain
    /// static factory instead (`await WasmPlayer.create(canvas, true)` from JS).
    ///
    /// `track_pointer` is the opt-in toggle for page-wide cursor tracking:
    /// when `true`, a `mousemove` listener is attached to `window` (not the
    /// canvas — the whole reason for this feature is reacting to the cursor
    /// even when it's elsewhere on the page, which a canvas-scoped listener
    /// could never see) that keeps two named number inputs, `"PointerX"`/
    /// `"PointerY"`, updated in artboard coordinates every frame — only when
    /// [`Driver::StateMachine`] is active (a bare-`Animation`/static driver
    /// has no `Inputs` to feed). Component authors don't get a bespoke Rust
    /// API for this — see [`POINTER_X_INPUT`]'s doc comment — they bind to
    /// those inputs exactly like any other state-machine `ConditionDef`.
    /// `false` attaches no listener at all, not just an inert one, so a
    /// component that doesn't want this pays nothing for it.
    pub async fn create(canvas: HtmlCanvasElement, track_pointer: bool) -> Result<WasmPlayer, JsValue> {
        let width = canvas.width();
        let height = canvas.height();

        // Cloned before `canvas` is moved into `create_surface` below — a
        // `web_sys::HtmlCanvasElement` clone is just a JS reference clone
        // (cheap), not a copy of the actual DOM element.
        let canvas_for_listener = canvas.clone();

        // WebGPU when the browser has it, else WebGL2 (decided before the
        // canvas gets a context — a canvas can only ever have one kind).
        let instance = wgpu::util::new_instance_with_webgpu_detection(wgpu::InstanceDescriptor::new_without_display_handle_from_env()).await;
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|err| JsValue::from_str(&format!("failed to create wgpu surface: {err}")))?;

        let renderer = Renderer::new_transparent(&instance, surface, width, height).await;
        let clear_color = if renderer.supports_transparency() {
            wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }
        } else {
            wgpu::Color { r: 0.08, g: 0.08, b: 0.1, a: 1.0 }
        };

        let (document, artboard_id, state_machine) = build_scene();

        let pointer_position = Rc::new(Cell::new((0.0f32, 0.0f32)));
        let pointer_listener = if track_pointer {
            let pointer_position_for_listener = Rc::clone(&pointer_position);
            let closure = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |event: web_sys::MouseEvent| {
                let rect = canvas_for_listener.get_bounding_client_rect();
                let canvas_x = event.client_x() as f32 - rect.left() as f32;
                let canvas_y = event.client_y() as f32 - rect.top() as f32;
                pointer_position_for_listener.set((canvas_x, canvas_y));
            });

            let window = web_sys::window().expect("no global `window` exists");
            window
                .add_event_listener_with_callback("mousemove", closure.as_ref().unchecked_ref())
                .map_err(|err| JsValue::from_str(&format!("failed to attach pointer listener: {err:?}")))?;

            Some(closure)
        } else {
            None
        };

        Ok(WasmPlayer {
            renderer,
            mesh_cache: HashMap::new(),
            font_cache: HashMap::new(),
            text_mesh_cache: HashMap::new(),
            image_quad_mesh: None,
            image_bindings: HashMap::new(),
            image_cache: HashMap::new(),
            document,
            artboard_id,
            driver: state_machine.into(),
            clear_color,
            pointer_position,
            pointer_listener,
        })
    }

    /// Resizes the underlying swap chain — call this from JS on window/canvas
    /// resize, mirroring `winit::WindowEvent::Resized` in the native examples.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.renderer.resize(width, height);
    }

    /// Replaces the current document (the initial demo scene, or a
    /// previously loaded one) with one decoded from real `.teab` bytes —
    /// the actual Phase 8 exit-criterion capability, see this module's doc
    /// comment. Picks what drives playback via [`pick_driver`]: the
    /// artboard's first state machine if it has one, else its first
    /// animation on a loop, else a static pose.
    ///
    /// Clears every mesh/glyph cache — a loaded document's `NodeId`s have no
    /// relationship to the previous document's (each is only meaningful
    /// within its own `Artboard`'s slotmap), so keeping stale entries around
    /// would either leak GPU resources for nodes that no longer exist, or
    /// worse, silently reuse a mesh keyed by a `NodeId` the new document's
    /// slotmap happens to reissue for a *different* node.
    /// `async` (unlike a plain-sync `render`) because it bakes every
    /// `ImageAsset` the freshly loaded document's `Image` nodes reference
    /// before returning — see `WasmEditor::bake_and_cache_image`'s doc
    /// comment for why baking (a GPU readback) can only happen at a real
    /// `.await` point, never inside the per-frame `render` loop.
    pub async fn load_teab(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        let document = an1mbyt3_format::load_teab(bytes).map_err(|err| JsValue::from_str(&format!("failed to load .teab: {err}")))?;
        let artboard_id = document
            .artboards()
            .next()
            .map(|(id, _)| id)
            .ok_or_else(|| JsValue::from_str("load_teab: loaded document has no artboards"))?;
        let driver = pick_driver(document.artboard(artboard_id).unwrap());

        self.document = document;
        self.artboard_id = artboard_id;
        self.driver = driver;
        self.mesh_cache.clear();
        self.font_cache.clear();
        crate::text_bounds::clear();
        self.text_mesh_cache.clear();
        self.image_bindings.clear();
        self.image_cache.clear();

        let image_ids: Vec<an1mbyt3_core::ImageId> = self.document.images().map(|(id, _)| id).collect();
        for image_id in image_ids {
            let Some(asset) = self.document.image(image_id) else { continue };
            let Ok((rgba, width, height)) = an1mbyt3_image::decode_to_rgba(&asset.source) else { continue };
            let (baked, baked_width, baked_height) = self.renderer.bake_image(&rgba, width, height, &asset.processing).await;
            let texture = self.renderer.upload_texture(baked_width, baked_height, &baked, asset.processing.filter);
            self.image_cache.insert(image_id, texture);
        }

        Ok(())
    }

    /// Every state machine name declared on the current artboard — lets a
    /// host page discover what's playable (and, in particular, whether
    /// `fire_trigger`/`set_bool_input`/etc. below will actually reach
    /// anything) without needing to inspect the `.teab` itself.
    pub fn state_machine_names(&self) -> Vec<String> {
        self.document
            .artboard(self.artboard_id)
            .map(|artboard| artboard.state_machines().map(|(_, def)| def.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Resumes playback — see `an1mbyt3_runtime::Playback::play`/
    /// `StateMachine::play`'s doc comments. A no-op if the current `Driver`
    /// is `Static` (nothing to play).
    pub fn play(&mut self) {
        self.driver.play();
    }

    /// Halts playback in place — a state machine still checks transitions
    /// on every `render` call while paused (an input can still switch
    /// state), it just stops advancing time until `play` is called again.
    pub fn pause(&mut self) {
        self.driver.pause();
    }

    /// Jumps directly to `time` (seconds) within whatever's currently
    /// driving playback — does not change whether it's playing, matching
    /// `Playback::seek`'s own convention.
    pub fn seek(&mut self, time: f32) {
        self.driver.seek(time);
    }

    /// Fires a named trigger input on the active state machine — e.g. a
    /// button click calling `player.fire_trigger("toggle")` reaches exactly
    /// the same `Inputs::fire_trigger` the native `state_machine` example's
    /// Space key reaches. See `an1mbyt3_runtime::state_machine`'s doc
    /// comment for why triggers auto-reset after one `advance` (a JS click
    /// handler firing this twice in a row behaves the same as two Space
    /// presses natively). Silently does nothing if the current `Driver`
    /// isn't a state machine — there's no `Inputs` to fire a trigger on.
    pub fn fire_trigger(&mut self, name: &str) {
        if let Driver::StateMachine(state_machine) = &mut self.driver {
            state_machine.inputs.fire_trigger(name);
        }
    }

    /// Sets a named boolean input on the active state machine. Same
    /// silently-does-nothing-without-a-state-machine convention as
    /// `fire_trigger`.
    pub fn set_bool_input(&mut self, name: &str, value: bool) {
        if let Driver::StateMachine(state_machine) = &mut self.driver {
            state_machine.inputs.set_bool(name, value);
        }
    }

    /// Sets a named numeric input on the active state machine. Same
    /// silently-does-nothing-without-a-state-machine convention as
    /// `fire_trigger`.
    pub fn set_number_input(&mut self, name: &str, value: f32) {
        if let Driver::StateMachine(state_machine) = &mut self.driver {
            state_machine.inputs.set_number(name, value);
        }
    }

    /// Sets a named enum-valued input on the active state machine. Same
    /// silently-does-nothing-without-a-state-machine convention as
    /// `fire_trigger`.
    pub fn set_enum_input(&mut self, name: &str, value: &str) {
        if let Driver::StateMachine(state_machine) = &mut self.driver {
            state_machine.inputs.set_enum(name, value);
        }
    }

    /// Forwards a real keyboard event into the active state machine's
    /// `Blocks`-mode input surface — a host page's own `keydown`/`keyup`
    /// listeners are expected to call this so an embedded player's
    /// Programmable States can react to key presses the same way the
    /// desktop editor's own preview does (`WasmEditor::set_key_down`). Same
    /// silently-does-nothing-without-a-state-machine convention as
    /// `fire_trigger`.
    pub fn set_key_down(&mut self, key: &str, pressed: bool) {
        if let Driver::StateMachine(state_machine) = &mut self.driver {
            state_machine.keys.set_down(key, pressed);
        }
    }

    /// Advances whatever's driving playback by `delta_seconds` and draws one
    /// frame. `delta_seconds` is the caller's responsibility to compute
    /// (from two consecutive `requestAnimationFrame` timestamps) — see this
    /// struct's doc comment for why `Instant` isn't available here to do it
    /// internally.
    pub fn render(&mut self, delta_seconds: f32) -> Result<(), JsValue> {
        let artboard = self.document.artboard_mut(self.artboard_id).unwrap();
        self.driver.advance_and_apply(delta_seconds, artboard);

        // Pointer tracking (if enabled) feeds the same two named inputs
        // every authored `ConditionDef` can already reference
        // (`POINTER_X_INPUT`/`POINTER_Y_INPUT`'s doc comment) — only when a
        // state machine is actually driving playback; a bare-`Animation`/
        // static driver has no `Inputs` to feed at all. Deliberately does
        // *not* also force-move some specific node the way this used to
        // (back when there was always exactly one hardcoded demo node) —
        // an arbitrary loaded document has no single "the" node to hijack;
        // a real component reacts to the cursor via its own authored
        // `ConditionDef`s referencing these inputs, same as any other input.
        if self.pointer_listener.is_some() {
            if let Driver::StateMachine(state_machine) = &mut self.driver {
                let artboard = self.document.artboard(self.artboard_id).unwrap();
                let (canvas_x, canvas_y) = self.pointer_position.get();
                // Canvas-pixel space (origin top-left, +y down) -> artboard
                // space (origin centered, +y up) — same convention every
                // other pointer-facing call in this workspace uses.
                // Deliberately not clamped to the canvas rect: a cursor past
                // the canvas edge still produces a valid (if off-artboard)
                // coordinate, which is the entire point of listening on
                // `window` instead of the canvas.
                let artboard_x = canvas_x - artboard.width / 2.0;
                let artboard_y = artboard.height / 2.0 - canvas_y;
                state_machine.inputs.set_number(POINTER_X_INPUT, artboard_x);
                state_machine.inputs.set_number(POINTER_Y_INPUT, artboard_y);
            }
        }

        let artboard = self.document.artboard(self.artboard_id).unwrap();
        for (node_id, node) in artboard.shapes() {
            if !self.mesh_cache.contains_key(&node_id) {
                let NodeKind::Shape(shape) = &node.kind else {
                    unreachable!("Artboard::shapes() only yields Shape nodes")
                };
                let mesh = tessellate_path(&shape.path);
                let gpu_mesh = self.renderer.upload_mesh(&mesh);
                let binding = self.renderer.create_draw_binding();
                self.mesh_cache.insert(node_id, (gpu_mesh, binding));
            }
        }

        // Same lazy-populate-a-cache shape as the shapes loop above, for
        // `NodeKind::Text` — mirrors `editor.rs`'s `render()` exactly (see
        // that method's doc comment for the re-shape-only-when-the-key-
        // changes reasoning).
        let artboard = self.document.artboard(self.artboard_id).unwrap();
        for (node_id, node) in artboard.texts() {
            let NodeKind::Text(text) = &node.kind else {
                unreachable!("Artboard::texts() only yields Text nodes")
            };

            if !self.font_cache.contains_key(&text.font) {
                let font_asset = self
                    .document
                    .font(text.font)
                    .expect("TextNode::font must reference a FontAsset registered on the same Document");
                let font = Font::from_bytes(font_asset.data.clone(), 0).expect("TextNode font bytes must be a valid font file");
                crate::text_bounds::register_font(text.font, &font_asset.data);
                self.font_cache.insert(text.font, font);
            }

            let key = (text.content.clone(), text.size_px, text.font);
            let needs_reshape = self.text_mesh_cache.get(&node_id).map(|cached| cached.key != key).unwrap_or(true);
            if needs_reshape {
                let font = self.font_cache.get(&text.font).unwrap();
                let shaped = shape_and_tessellate(font, &text.content, text.size_px);
                let recenter = Vec2::new(-shaped.advance.x / 2.0, 0.0);
                // A whitespace glyph (a space, most commonly) has no outline
                // at all — `tessellate_path` hands back a zero-vertex mesh,
                // which `Renderer::upload_mesh` builds a GPU buffer for, but
                // actually drawing that buffer later panics wgpu-side with
                // "buffer slice can not be empty." See `editor.rs`'s
                // identical fix for the full reasoning.
                let glyphs = shaped
                    .glyphs
                    .into_iter()
                    .filter(|g| !g.mesh.indices.is_empty())
                    .map(|g| (self.renderer.upload_mesh(&g.mesh), self.renderer.create_draw_binding(), g.offset + recenter))
                    .collect();
                self.text_mesh_cache.insert(node_id, TextMeshCache { key, glyphs });
            }
        }

        // Same lazy-populate-a-cache shape as the shapes loop above, for
        // `NodeKind::Image` — only the shared unit quad and this node's own
        // `DrawBinding` are built here; the actual baked texture
        // (`image_cache`) is populated by `load_teab` instead (a real
        // `.await` point, unlike this plain-sync `render`) — see
        // `WasmEditor::bake_and_cache_image`'s doc comment for why baking
        // can't happen here.
        if self.image_quad_mesh.is_none() {
            self.image_quad_mesh = Some(self.renderer.upload_mesh(&an1mbyt3_geometry::unit_quad_mesh()));
        }
        let artboard = self.document.artboard(self.artboard_id).unwrap();
        for (node_id, _) in artboard.images() {
            self.image_bindings.entry(node_id).or_insert_with(|| self.renderer.create_draw_binding());
        }

        let artboard = self.document.artboard(self.artboard_id).unwrap();
        let mut draws: Vec<DrawCall> = artboard
            .shapes()
            .map(|(node_id, node)| {
                let NodeKind::Shape(shape) = &node.kind else {
                    unreachable!("Artboard::shapes() only yields Shape nodes")
                };
                let (mesh, binding) = self.mesh_cache.get(&node_id).expect("just populated above");
                DrawCall { mesh, binding, transform: artboard.world_transform(node_id), fill_color: shape.fill_color, blend_mode: shape.blend_mode, gradient: shape.gradient.as_ref(), texture: None, material: None }
            })
            .collect();

        for (node_id, node) in artboard.texts() {
            let NodeKind::Text(text) = &node.kind else {
                unreachable!("Artboard::texts() only yields Text nodes")
            };
            let node_transform = artboard.world_transform(node_id);
            let cached = self.text_mesh_cache.get(&node_id).expect("just populated above");
            draws.extend(cached.glyphs.iter().map(|(mesh, binding, offset)| DrawCall {
                mesh,
                binding,
                transform: node_transform.with_local_offset(*offset),
                fill_color: text.fill_color,
                blend_mode: BlendMode::Normal,
                gradient: text.gradient.as_ref(),
                texture: None,
                material: None,
            }));
        }

        for (node_id, node) in artboard.images() {
            let NodeKind::Image(image_node) = &node.kind else {
                unreachable!("Artboard::images() only yields Image nodes")
            };
            let Some(binding) = self.image_bindings.get(&node_id) else { continue };
            let Some(texture) = self.image_cache.get(&image_node.image) else { continue };
            let quad_mesh = self.image_quad_mesh.as_ref().expect("populated above alongside image_bindings");
            draws.push(DrawCall {
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
                material: None,
            });
        }

        self.renderer
            .render((artboard.width, artboard.height), Camera::default(), self.clear_color, &draws)
            .map_err(|err| JsValue::from_str(&format!("render failed: {err:?}")))
    }
}

impl From<StateMachine> for Driver {
    fn from(state_machine: StateMachine) -> Self {
        Driver::StateMachine(state_machine)
    }
}

impl Drop for WasmPlayer {
    /// Explicitly detaches the `window` `mousemove` listener instead of
    /// letting `pointer_listener`'s `Closure` merely drop — dropping a
    /// `Closure` invalidates the JS function it wraps but does *not* call
    /// `removeEventListener` for you, so skipping this would leave `window`
    /// holding a reference to a now-dangling callback (the next real mouse
    /// move would try to invoke deallocated Wasm memory).
    fn drop(&mut self) {
        if let Some(closure) = &self.pointer_listener {
            if let Some(window) = web_sys::window() {
                let _ = window.remove_event_listener_with_callback("mousemove", closure.as_ref().unchecked_ref());
            }
        }
    }
}
