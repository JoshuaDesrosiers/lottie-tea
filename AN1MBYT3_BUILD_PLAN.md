# an1mbyt3 — Architecture & Build Plan (Expanded)

> Expanded from `rive_copycat_architecture_plan.docx`. This is the working build spec: concrete crate layout, data schemas, APIs, phase exit criteria, and repo structure — not just direction-setting bullets.

## 0. Product Framing

an1mbyt3 is a Rive-class interactive animation platform: a native editor for building vector animations with bones, constraints, state machines, and parameter-driven interactivity, plus a production runtime that plays those animations back in React, plain JavaScript, and raw HTML with a tiny API surface.

The rule that governs every decision below: **the runtime/engine is the product; the editor is one client of it, and the browser runtime is another.** Both must be built on the exact same Rust core so behavior never diverges between "what the designer saw" and "what ships."

---

## 1. Core Architecture

- **Rust is the animation brain.** Document model, animation evaluation, vector geometry, bones/constraints, state machines, serialization, and all runtime logic live in Rust. No business logic duplicated in TypeScript.
- **React/TypeScript is the editor's client shell.** It owns editor UI chrome: inspector panels, timeline widget, tool palette, selection/undo UI state, and window management. It does not own animation semantics — it calls into the Rust engine and renders what the engine reports.
- **wgpu is the rendering foundation** on both desktop and web (via WASM + WebGPU/WebGL2 fallback), so the same shader and batching code runs in both targets.
- **Tauri 2 hosts the desktop editor as a thin native shell only** (file dialogs, disk I/O, window chrome) — **REVISED**: the document model, undo/redo, and rendering do NOT run in the native Rust backend. They run as the same `an1mbyt3-wasm` module the browser runtime uses, loaded directly into the Tauri WebView, rendering to the canvas's own `wgpu::Surface` with zero IPC bridge for documents or frames. This deliberately mirrors how Rive's own web-based editor works (no native desktop engine process at all for their editor either). The bridge cost this avoids: a native-engine-plus-IPC design pays a per-frame or per-pixel serialization cost (either a full document sync or a full framebuffer readback, every frame); loading the engine directly in the webview means only genuine user edits cross a boundary at all, and that boundary is JS↔wasm function calls, not IPC. See `crates/an1mbyt3-wasm/src/editor.rs`'s module doc for the full reasoning; an earlier readback-bridge prototype (headless native `Renderer` + `render_to_texture` + Tauri command streaming RGBA bytes to a `<canvas>`) was built, verified, and then deliberately replaced — its underlying `render_to_texture`/`new_headless` primitives remain in `an1mbyt3-renderer` for other uses (e.g. thumbnail generation) but are no longer used by the editor.
- **The web runtime is the same Rust core compiled to WebAssembly.** No parallel "lite" engine — feature parity between desktop-authored and web-played animations is a hard constraint, enforced by using one crate graph for both. This now applies to the desktop *editor* too, not just the played-back runtime: the editor's document/render logic is the identical wasm module, not a separate native build of the same crates.

### Process/Compilation Topology

| Target | Rust engine runs as | UI layer | Transport |
|---|---|---|---|
| Desktop editor | `wasm32-unknown-unknown` module, loaded in the Tauri WebView | React in Tauri WebView | Direct WASM function calls (`WasmEditor`, via `serde-wasm-bindgen`) — Tauri IPC used only for native-shell concerns (file dialogs, disk I/O), not documents or frames |
| Web player | `wasm32-unknown-unknown` module | React/JS/HTML consumer | Direct WASM function calls (no serialization round-trip needed for hot path) |
| CLI | Native binary | none | stdin/stdout/file I/O |

The document model crate (`an1mbyt3-core`) must compile cleanly to all three targets with zero `cfg`-gated behavior differences in animation logic — only I/O and threading adapters differ per target.

---

## 2. Proposed Stack

- **Core engine language:** Rust, 2021 edition, workspace-based monorepo. MSRV pinned and checked in CI.
- **Renderer:** `wgpu` (targets Vulkan/Metal/DX12 natively, WebGPU/WebGL2 on web). Abstracted behind a `Renderer` trait in `an1mbyt3-renderer` so backend swaps don't leak into scene code.
- **Vector geometry/tessellation:** `lyon` for path flattening and tessellation initially; budget for a custom incremental tessellator once profiling shows lyon's full-rebuild model is the bottleneck for per-frame deforming geometry.
- **Desktop application shell:** Tauri 2, using its plugin system for filesystem, dialogs, and window management. **REVISED**: `src-tauri` registers no custom commands for documents or rendering at all — those live in the `an1mbyt3-wasm` module loaded by the WebView itself. `src-tauri` is reserved for OS-level integration a webview can't do itself (file-open/save dialogs, `.tea`/`.teab` disk I/O), none of which is built yet.
- **Editor UI:** React + TypeScript, Vite for dev/build tooling.
- **Editor state:** Zustand for UI-only state (panel layout, selection cursor, tool mode). Anything that is animation *data* (nodes, keyframes, constraints) is **not** duplicated into Zustand as a source of truth — Zustand holds a thin projection kept in sync via engine events; the engine's document model is the single source of truth, consistent with §4.
- **Web runtime:** Rust → `wasm-bindgen`/`wasm-pack` build targeting `wasm32-unknown-unknown`, published as an npm package wrapping the `.wasm` binary.
- **Developer API:** hand-written TypeScript wrapper (`@an1mbyt3/runtime`) around the raw WASM exports — consumers never call `wasm-bindgen` glue directly.
- **File format:**
  - Source/dev format: `.tea` — human-readable, pretty-printed JSON (or JSON5) matching the `serde`-derived document schema, diff-friendly for version control.
  - Production format: `.teab` — compact binary (bincode or a custom flatbuffers-style layout — decide in Phase 8) produced by the CLI compiler.
- **CLI:** Rust binary (`an1mbyt3-cli`, built with `clap`) exposing `compile`, `optimize`, `validate`, `export`, and `package` subcommands.

---

## 3. Engine Modules (Crate Layout)

Cargo workspace, one crate per concern, dependency arrows flow downward only (no cycles):

```
an1mbyt3-core         (document model, scene graph, nodes, transforms, assets)
an1mbyt3-geometry     (paths, Bézier curves, tessellation, meshes, deformation) — depends on core
an1mbyt3-runtime      (timelines, keyframes, interpolation, easing, events, playback, parameters) — depends on core, geometry
an1mbyt3-constraints  (bones, IK, aim/look-at, follow, distance, transform constraints) — depends on core, geometry
an1mbyt3-renderer     (wgpu, shaders, batching, masks, compositing, blend modes, GPU resources) — depends on core, geometry
an1mbyt3-text         (font shaping via rustybuzz, glyph atlas/SDF generation, glyph mesh output) — depends on core, geometry, renderer
an1mbyt3-format       (serialization, compression, versioning, migration) — depends on core, runtime, constraints
an1mbyt3-lottie       (Lottie JSON importer/exporter — compatibility layer) — depends on core, runtime
an1mbyt3-wasm         (JS/WASM bindings) — depends on core, runtime, constraints, renderer, text, format
an1mbyt3-cli          (compile, optimize, validate, package, export) — depends on core, format, lottie
```

Editor-only, not part of the runtime dependency chain:
```
apps/editor            (Tauri 2 + React/TypeScript desktop app) — depends on an1mbyt3-core et al. via Tauri commands
packages/runtime-react  (React <Animation/> wrapper) — depends on an1mbyt3-wasm's npm package
packages/runtime-js     (vanilla JS + custom element) — depends on an1mbyt3-wasm's npm package
```

Each engine crate exposes a narrow public API (documented with `///` doc comments and `cargo doc`); internal representations stay `pub(crate)`. This is what keeps the WASM bindings and Tauri commands thin — they're both just callers of the same public API.

---

## 4. Document Model

### Hierarchy
```
Document
 └─ Artboards[]
     └─ Nodes[]  (tree, parented by ID)
         ├─ Shape / Path / Group
         ├─ Image
         ├─ Text
         ├─ Bone
         ├─ Mesh
         └─ Component instance (with parameter overrides)
```

### Identity & storage
Use a generational-arena / `slotmap`-style ID scheme (`NodeId(u32 index, u32 generation)`) rather than raw indices or `Rc<RefCell<_>>` graphs — this gives O(1) lookup, safe deletion/reuse, and clean serialization (IDs serialize as plain integers, stable across save/load).

### Cross-cutting objects
- **Constraints** are first-class document objects, not properties bolted onto a node — they reference source/target `NodeId`s and are evaluated in a dependency-ordered pass after base transforms, before rendering.
- **Animations** hold timelines → tracks → keyframes → curves(Bézier/easing) → events, keyed by `NodeId` + property path (e.g. `Node(42).rotation`).
- **State machines** hold states, transitions (with conditions over inputs), inputs (bool/number/enum/trigger), and blend logic, referencing animations by ID.
- **Components** wrap a subgraph of nodes as a reusable, parameterized unit; instances store parameter values and per-instance overrides, resolved at evaluation time against the component definition.

### Single source of truth
Editor, runtime, renderer, importer, and exporter all operate on the same `Document` struct (`an1mbyt3-core`). The editor's Zustand store is a read projection updated from engine-emitted change events — never an independent copy that gets reconciled later. This avoids the classic two-source-of-truth bug class (editor state and file state drifting).

### Serialization
`serde`-derived `Serialize`/`Deserialize` on every document type. Source format (`.tea`) is `serde_json` with pretty-printing; production format (`.teab`) is a compacted encoding chosen in Phase 9 after profiling load time and size (candidates: `bincode`, `postcard`, or a custom flatbuffer-like layout for true zero-copy loads in the WASM runtime). Version every top-level document with a schema version integer; `an1mbyt3-format` owns migration functions between versions (`migrate_v1_to_v2`, etc.) so old `.tea` files keep opening.

### Undo/Redo Architecture
Undo/redo lives in the engine (`an1mbyt3-core`), not bolted onto the editor's Zustand layer — this keeps it correct for any embedder, not just the React editor, and keeps the document model authoritative.

- All document mutation goes through a `Command` trait (`apply(&mut Document)`, `invert(&Document) -> Command`) rather than direct field writes — a node move, a keyframe edit, a node delete are each a `Command` value, not ad hoc mutation code scattered through editor handlers.
- `Document` owns an undo stack (`Vec<Command>`) and a redo stack, cleared/truncated on new mutation after an undo (standard branching-history-discarded behavior — no need for a full undo tree in v1).
- Exposed as `document.apply(cmd)`, `document.undo()`, `document.redo()` — the editor's UI never mutates node fields directly, it always constructs and applies a `Command`.
- Batch/composite edits (e.g. drag-adjusting 5 selected nodes at once) are a `CompositeCommand` wrapping child commands, so one drag = one undo step, not five.
- Coalescing: continuous edits (dragging a slider, live-typing a text field) coalesce into a single undo entry until the drag/typing session ends, rather than one entry per intermediate value.

---

## 5. Animation & Interaction Features

- **Timeline animation:** keyframes per property, Bézier/eased interpolation curves, playback controller (play/pause/seek/loop-modes: once, loop, ping-pong), markers, and timed events fired to listeners.
- **Bones & deformation:** bone hierarchies with weighted vertex skinning (linear blend skinning to start); constraint solvers for IK (start with CCD or two-bone analytic IK — cheaper than FABRIK for the common 2-bone-leg/arm case, add FABRIK later for longer chains), look-at/aim, follow, distance, and generic transform constraints. Constraints evaluate in a topologically-sorted pass so a bone's constraint can safely depend on another bone already resolved that frame.
- **State machines:** typed inputs — `bool`, `number`, `enum`/string, and `trigger` (fire-once, auto-resets) — driving transitions between states via boolean expressions over inputs; states reference blended animation playback.
- **Mouse/pointer-driven animation:** pointer position (and derived values — velocity, drag distance) is exposed as just another input source feeding the same parameter-binding system used by state machines — no special-cased "mouse mode."
- **Generic input-driven composition:** scroll position, device sensors, or arbitrary host-application data can all feed parameters through the same input-binding API, so "make it react to X" is always "bind X to a parameter," never a new engine feature.
- **Parameter binding:** an expression/binding layer that maps an input value to a target property (direct, remapped/clamped, or blended across a curve) — this is what lets a designer build a reactive *system* instead of authoring every possible animation state by hand.
- **Components & nested animations:** reusable animated subgraphs with declared parameters, instantiated multiple times with different overrides — the reuse mechanism for design-system-style motion assets.
- **Multi-property animation tracks:** a clip is not hard-coded to translation only — `an1mbyt3-runtime`'s `Keyframe<T>`/`Track<T>` are already generic over any type implementing a `Lerp` trait (Phase 2 only wired up `Vec2` translation as the first concrete case). Extending to rotation, scale, and fill color is additive: `impl Lerp for f32`/`impl Lerp for [f32; 4]`, then widening `AnimationClip` from one `Track<Vec2>` field to a small set of optional per-property tracks. This is what makes a "reverse" or "color" modifier on a state transition possible without inventing a scripting layer — see the state-machine transition-modifier note below.
- **Transition modifiers:** `Transition`/`fire_trigger` should be able to carry a small typed payload (e.g. a speed override, a reverse flag, a target color) that the state machine plugs into the freshly-entered state's `Playback`/property tracks — `Playback` already has a public `speed: f32` field and internal reverse-direction tracking (added for ping-pong), so "play this state back in reverse" is already one field-flip away at the primitive level; only the transition-carries-a-parameter plumbing is missing. Scope this as its own small design pass (payload shape: fixed enum of modifier kinds vs. open-ended named params) before implementing, not a one-line patch.

---

## 6. Rendering Strategy

- **SVG is not the runtime renderer** — it's at most an interchange/debug format. All runtime rendering goes through wgpu.
- **Vector paths are tessellated to GPU-friendly triangle meshes**, with tessellation output **cached** per shape. Cache invalidation is keyed off a per-node dirty flag set only when a shape's actual path data changes.
- **Transform-only animation updates matrices, not geometry.** A moving/rotating/scaling shape with unchanged path data re-tessellates zero times per frame — only its transform uniform updates.
- **Deformation (skinning/bones) updates only the affected vertex buffers**, not the full scene's geometry.
- **Batching & culling:** compatible draws (same material/blend state) are batched into fewer draw calls; offscreen/invisible nodes are culled before hitting the renderer.
- **Compositing, masks, blend modes** are implemented as dedicated shader passes / stencil-buffer techniques, not CPU-side pixel manipulation.
- **Vector path effects (trim paths, dashed strokes):** these operate on a path's *stroke*, not its fill — `an1mbyt3-geometry` needs a stroke tessellator (lyon's `StrokeTessellator`, alongside the existing `FillTessellator`-based `tessellate_path`) plus arc-length parameterization (`lyon_algorithms`'s path-measurement/walk utilities) so a path can be cropped to a `[start, end]` percentage range before stroking — this is exactly how Rive and Lottie-web implement trim paths, and it's also Lottie import priority #4 (§9) for the same underlying reason: both need the same arc-length-cropping primitive, one authored natively, one imported from Lottie JSON.
- **Post-processing (glow, blur, pixelation, etc.):** requires restructuring the per-frame pipeline from "one pass straight to the swapchain" into "render the scene to an offscreen texture, then run a chain of full-screen-quad shader passes before compositing to the surface" — standard wgpu architecture (this is how most real-time engines implement post effects), and additive: it doesn't change how any existing shape/mesh is drawn, it just adds pass(es) after. Scope as its own phase (below) once trim paths/masks are in, since a glow pass over a scene that already has proper masking looks meaningfully better than one that doesn't.
- **Text glyphs are cached the same way as tessellated paths**: `an1mbyt3-text` shapes a run (via `rustybuzz`) into positioned glyph IDs, rasterizes/SDF-generates each distinct glyph once into a shared atlas, and caches the shaped-run-to-glyph-quad mapping keyed on (font, size, string content). Re-tessellation/re-shaping only happens when the string, font, or size actually changes — a moving text node is a transform-only update like any other node.

Concretely, the per-frame pipeline is:
1. Evaluate animation/state-machine/constraint pass → updated transforms + deformed vertex buffers (CPU, but cheap — no re-tessellation).
2. Diff against last frame's dirty set → decide what needs re-tessellation vs. just a transform update.
3. Build/update batches → submit to wgpu.

---

## 7. File & Runtime Pipeline

1. Designer authors an animation in the editor.
2. Editor saves the human-readable `.tea` source format (diffable, safe for git).
3. `an1mbyt3-cli compile` converts `.tea` → `.teab`: strips unused nodes/keyframes, deduplicates shared geometry, optimizes curves (reduces redundant keyframes within tolerance), builds string tables (dedupe repeated strings — layer names, etc.), compresses, and prepares runtime-ready resource layout.
4. Browser (or native embedder) loads the `.teab` binary through the WASM (or native) runtime.
5. Host application (React/JS) drives the runtime through the small public API (§8) — it never touches individual vector objects or the document model directly.

---

## 8. Developer Experience

**React:**
```tsx
<Animation src="/character.teab" stateMachine="Character" />
```

**Runtime input API:**
```ts
animation.stateMachine("Character").setNumber("speed", value);
animation.stateMachine("Character").setBoolean("isRunning", true);
animation.stateMachine("Character").fireTrigger("jump");
```

**Vanilla JS** is a first-class target, not an afterthought bolted onto the React wrapper — `packages/runtime-js` wraps `an1mbyt3-wasm` directly with no React dependency.

**Custom element**, for markup-only embedding:
```html
<animation-player src="/character.teab" state-machine="Character"></animation-player>
```

**Runtime surface** (same underlying API, exposed three ways — React props, JS methods, HTML attributes): play/pause/seek, state machine access, parameter get/set, event subscription (`on("complete", ...)`, custom animation events), input binding (pointer/scroll/custom), and component instantiation/override APIs.

---

## 9. Lottie Compatibility

Lottie import/export is a **compatibility layer bolted onto the side**, not the native representation:

```
Lottie JSON → an1mbyt3-lottie importer → native Document model → native runtime/renderer
```

This gets broad Lottie-file compatibility without importing Lottie's architectural constraints (no bones/constraints/state-machines in Lottie's model, layer-based rather than scene-graph-based, etc.) into an1mbyt3's core.

Import priority order (highest-value/most-common first):
1. Shape layers + transforms
2. Precomps
3. Masks
4. Trim paths
5. Track mattes
6. Text layers
7. Remaining/long-tail animatable properties, incrementally

Export (native → Lottie JSON) is lower priority than import and only covers the subset of native features that have a Lottie-compatible representation (no bones/state-machines — those simply don't round-trip to Lottie).

---

## 10. Development Phases

Each phase has an explicit exit criterion — don't advance until it's met.

| Phase | Scope | Exit criterion |
|---|---|---|
| **1** | Rust document model + vector paths + transforms + wgpu renderer | Can construct a document in-process and render a static shape to a window/canvas |
| **2** | Timeline, keyframes, interpolation, easing, playback | A keyframed property animates correctly with play/pause/seek/loop |
| **3** | Text rendering: font shaping (`rustybuzz`), glyph atlas/SDF generation, glyph-quad rendering via wgpu | A Text node renders correctly shaped/positioned/rasterized glyphs; editing string content, font, or size re-shapes and re-renders correctly; unrelated transform-only movement of the text node causes zero re-shaping |
| **4** | React/Tauri editor: canvas, layers, inspector, timeline, selection, transforms, path editing, undo/redo (see §15 for design direction) | Designer can create/edit/save a `.tea` file entirely through the UI, no hand-editing JSON — and undo/redo correctly reverses/reapplies node creation, deletion, transform edits, and keyframe edits, including coalesced drag edits as single steps |
| **5** | Bones, skinning/deformation, IK, constraints | A rigged shape deforms correctly under IK-driven bone movement |
| **6** | State machines, parameters, triggers, transitions, events | A state machine with 2+ states and a trigger transitions correctly at runtime |
| **7** | Components, nested animations, overrides, variants | A component instantiated twice with different parameter overrides renders both distinctly |
| **8** | WASM runtime + React/JS/HTML runtime packages | `<Animation>` plays a real `.teab` file in a browser with working state-machine input |
| **9** | Binary format, compiler, compression, caching, batching, lazy loading, memory/GPU optimization | `.teab` load-and-first-frame time and file size hit defined budgets (set concrete numbers once Phase 8 has a baseline) |
| **10** | Vector/motion effects: multi-property animation tracks (rotation/scale/color), transition modifiers, trim paths + stroke dashing, masks/clipping, blend modes | A shape's fill color animates via a keyframe track (not just translation); a state transition can specify "reverse" or a target color without a hand-authored mirror clip; a stroked path's visible extent animates via start/end/offset percentages; a mask layer clips a group's contents |
| **11** | Post-processing pipeline: offscreen scene render target + full-screen shader passes (glow/blur, pixelation, color grading) | A component can have a glow or blur effect applied as a post pass, toggleable/animatable, without changing how its own shapes are drawn |
| **parallel** | Lottie importer (compatibility infra) | A representative set of real-world Lottie files (shape/precomp/mask/trim-path cases) import and render correctly |

Phases 1–2 must be solid before Phase 3 starts, since the editor is a client of the engine, not the other way around. Phases 5–7 can reorder relative to each other if product priority shifts, but each depends on Phases 1–2 being done. Text (Phase 3) is placed before the editor (Phase 4) because the editor's canvas needs to actually render text nodes, not just vector shapes, to be useful for real authoring. Phases 10–11 depend on Phase 1 (renderer) but are otherwise independent of 5–9 — they can be pulled forward if a specific effect becomes a launch blocker.

---

## 11. First Vertical Slice

Do **not** attempt to recreate all of Rive's feature set before proving the pipeline end-to-end. The first milestone is the full round-trip, minimal-feature:

1. Draw a single vector shape (in-engine, not yet in a UI).
2. Animate one property on it via a keyframe.
3. Save it to `.tea`.
4. Reopen `.tea` and get back an identical document.
5. Render it through `an1mbyt3-renderer`, via `an1mbyt3-wasm`'s `WasmEditor` running in the Tauri WebView.
6. Compile it to `.teab` via `an1mbyt3-cli`.
7. Load the `.teab` in a browser via `an1mbyt3-wasm`.
8. Control playback from JavaScript (play/pause/seek at minimum).

Once this slice works end-to-end, every later feature (bones, state machines, components, Lottie import) plugs into an already-proven pipeline instead of becoming a one-off prototype that gets thrown away. Treat any feature work attempted before this slice is green as a prototype, not product code.

---

## 12. Repository & Project Structure

```
an1mbyt3/
├─ Cargo.toml                  (workspace root)
├─ crates/
│  ├─ an1mbyt3-core/
│  ├─ an1mbyt3-geometry/
│  ├─ an1mbyt3-runtime/
│  ├─ an1mbyt3-constraints/
│  ├─ an1mbyt3-renderer/
│  ├─ an1mbyt3-text/
│  ├─ an1mbyt3-format/
│  ├─ an1mbyt3-lottie/
│  ├─ an1mbyt3-wasm/
│  └─ an1mbyt3-cli/
├─ apps/
│  └─ editor/                  (Tauri 2 + React/TypeScript)
├─ packages/
│  ├─ runtime-react/           (@an1mbyt3/react)
│  └─ runtime-js/              (@an1mbyt3/runtime, custom element)
└─ docs/
```

### Testing & tooling
- Rust: unit tests per crate + golden-file/snapshot tests for the renderer (rendered frame hash or pixel-diff against a reference image) and for the format (round-trip serialize/deserialize fixtures).
- Migration tests: every schema version bump ships with a fixture file from the prior version and a test asserting it still loads.
- CI: `cargo test` + `cargo clippy` across the workspace; separate WASM build check (`wasm-pack build --release`) since WASM target failures don't always show up in native test runs.
- Lottie importer: fixture corpus of real-world `.json` Lottie files, one test per fixture, comparing key frames of rendered output against a reference render.

---

## 13. Guiding Principle

- React is the editor and developer-facing control surface — never the animation authority.
- Rust is the animation brain — the only place animation semantics are implemented.
- wgpu is the rendering foundation — the same shader/batching code, desktop and web.
- WASM is the bridge to the web — not a reimplementation, a compile target.
- **The animation asset (`.teab`) is the product.** Designers build interactive animation logic once; developers consume it through a tiny runtime API, never touching the internal model.

---

## 14. Code Style & Documentation Conventions

This project deliberately favors **heavy, explicit documentation over terse self-documenting code**. Any contributor (or future you) should be able to open a single file cold and understand what it does and why without hunting through the rest of the codebase. This is a conscious tradeoff for a long-lived, architecturally deep engine project — terseness optimizes for the writer, this project optimizes for the reader.

### Rust
- Every file starts with a `//!` module-level doc comment: what this module is responsible for, what it explicitly is *not* responsible for, and which other modules/crates it talks to.
- Every `pub` item (struct, enum, fn, trait) gets a `///` doc comment explaining purpose, not just restating the signature — include invariants, units, ownership/lifetime notes, and *why* it exists if that's non-obvious.
- Non-trivial logic (solvers, tessellation, constraint evaluation order, cache invalidation) gets inline `//` comments explaining the reasoning, edge cases handled, and any workaround-for-a-specific-bug context.
- Doctests (`/// # Examples`) on public API functions where a usage example clarifies behavior faster than prose.
- Every crate has its own `README.md`: what this crate owns, where it sits in the dependency graph (§3), and a short example of its main API.

### TypeScript/React
- Every exported function/component/hook gets a JSDoc block: purpose, params, return, and any gotchas (e.g. "must be called after engine init").
- Every file gets a short header comment summarizing what the file contains and how it fits into the editor or runtime package.
- Non-obvious state synchronization (e.g. Zustand projection reacting to engine change events, §2) gets an explicit comment explaining the data flow, since this is exactly the kind of cross-boundary logic that's easy to misread later.

### General rule
When in doubt, over-explain rather than assume the reader already has the context you have right now. Comments should answer "why does this exist / why is it built this way," not just "what does this line do."

---

## 15. Editor UI Design Direction

Target: **Rive's panel structure, Procreate's visual and interaction polish.** These are two separable design axes — layout skeleton vs. surface/interaction feel — and this project takes a clear position on both instead of defaulting to a generic clone.

### Structure (from Rive)
- Canvas-dominant center stage; the artboard is the largest thing on screen at all times, never squeezed by permanent chrome.
- Left panel: layer/node hierarchy tree.
- Right panel: contextual inspector (properties of current selection — changes based on what's selected, same pattern as Rive/Figma).
- Bottom panel: timeline (keyframe mode) / state-machine graph (logic mode), toggled by editing context rather than both permanently visible.
- Top bar: tool palette + mode switch (Design / Animate / State Machine), matching Rive's mode-based workflow rather than one flat toolbox.

### Polish & feel (from Procreate)
- **Floating, translucent panels instead of rigid fixed docks.** Side/bottom panels are overlays with `backdrop-filter` blur over the canvas, not panels that permanently shrink the canvas's actual pixel space — closer to how Procreate's tool panels float over the canvas.
- **Focus mode**: chrome auto-recedes (fades/slides out) during active canvas interaction (dragging, drawing, scrubbing) and animates back in on idle — canvas-first, chrome-on-demand, not chrome-always-visible.
- **Contextual, on-canvas controls near the selection** (transform handles, quick-action bubbles) rather than forcing every action through a sidebar — reduces eye travel, a big part of why Procreate feels fast and tactile.
- **Spring-based micro-transitions** on panel open/close, tool switching, and selection change — not instant snaps, not slow/decorative either. Implement via a motion library (e.g. Framer Motion) rather than hand-rolled CSS transitions, so easing stays consistent project-wide.
- **Restrained color system**: dark, neutral canvas-first palette with a single accent color for active/selected state — not a rainbow of semantic colors competing for attention.
- **One consistent, custom-drawn icon set** for tools and panel actions — mixing icon-font styles (rounded vs. sharp, filled vs. outline) is one of the fastest ways an editor UI reads as "unpolished" even when the layout is right.
- **Generous whitespace and a deliberate type scale** in panels — dense-but-breathable, the same quality that makes Procreate's sparse control surfaces feel premium rather than empty.

### Implementation notes
- Tailwind for layout/spacing utility classes; a small set of CSS custom properties for the accent color and dark-theme tokens so re-theming later doesn't require a rewrite.
- `backdrop-filter` blur and spring-animated panels cost webview compositing time, not engine/render time (per §2's process topology, the editor chrome and the wgpu canvas are separate render paths) — negligible next to actual animation rendering, so this polish target has no meaningful performance tradeoff.
- Build the floating-panel/focus-mode shell early in Phase 4 rather than bolting it on after a fixed-dock layout ships — retrofitting "fixed panels" into "floating overlay panels" later touches most of the layout code twice.

---

## 16. Industry-Standard Feature Gaps (Backlog)

A survey of capabilities real competing tools (Rive, Figma, Framer, ProtoPie, After Effects/Lottie) treat as standard, that this plan doesn't already cover. Filtered toward the product's actual focus — **polished, reactive UI motion, not long-form video production** — a few AE/Lottie-native concepts (masks, trim paths, blend modes) are called out anyway because they show up directly in UI micro-interactions (a button's press glow, a toast's wipe-reveal), not just in video compositing.

- **Per-shape pointer hit-testing ("Listeners"):** Rive's state machines have `Listener` objects — bind a specific shape's hover/press/enter/exit to inputs, not just raw pointer coordinates. Phase 8's `track_pointer` toggle (built) exposes page-wide cursor position as named inputs; it does **not** yet do shape-local hit-testing ("is the pointer over *this* button"). This is the highest-value gap for a UI-focused product — most real "hover this button, it lights up" interactions need it specifically, not just raw coordinates.
- **Data binding / view-model layer:** Rive's "Data Binding" binds a text/number/color property directly to live host-application data (a progress ring bound to a real percentage, a label bound to a username) instead of only to state-machine inputs. Complements §5's parameter-binding system rather than replacing it.
- **Auto-layout / responsive constraints + 9-slice scaling:** Figma's auto-layout and Rive's "Layout" (nine-slicing) let a component resize gracefully across container sizes without manual re-authoring per breakpoint — important if components are meant to drop into arbitrary host UIs at arbitrary sizes (ties directly to the transparent-canvas "runs anywhere on a page" work already done in Phase 8).
- **Vector boolean operations** (union/subtract/intersect/exclude): standard shape-building tool in Figma/Illustrator; currently authoring is limited to whatever `Path`/`PathCommand` can express directly, no boolean combination step.
- **Spring-based interpolators**, not just Bézier-eased keyframes: Framer/Rive/Principle's signature "native app feel" comes from spring physics (mass/stiffness/damping) as a first-class interpolation mode alongside `Easing`, not a designer hand-tuning a Bézier curve to fake it.
- **Component variants** (structural swap, not just parameter override): Figma-style variants swap an entire subgraph (e.g. "button/primary" vs. "button/ghost") rather than overriding one parameter on a fixed structure — a real extension of Phase 7's narrow MVP (§5), not a different mechanism.
- **Rich text runs:** multiple styles (font/size/color/weight) within a single text node — Phase 3's text rendering is scoped to one style per node; long-form or emphasis-heavy UI copy needs runs.
- **Design tokens / theming pass-through:** letting a host page's own CSS custom properties (or a passed-in theme object) drive a component's colors at runtime, rather than baking colors into the `.teab` — directly relevant to the "component can live anywhere on any page" positioning from Phase 8's transparent-canvas work.
- **Gesture recognition beyond raw position:** drag distance/velocity thresholds, swipe direction, multi-touch — ProtoPie's core authoring primitive; builds on §5's "pointer position is just another input" model rather than replacing it.
- **Accessibility semantics:** ARIA roles/labels/live-region announcements for interactive states — easy to skip, expected of a professional-grade UI tool, and currently not mentioned anywhere in this plan.

None of these block Phases 1–11 — they're additions to the same document model/parameter-binding/component systems already planned, not architectural pivots. Worth revisiting this list once Phase 7 (components) and Phase 10 (effects) are further along, since several of these (variants, data binding, listeners) are natural extensions of those two rather than standalone phases.

---

## Reference

Tauri 2 supports a web frontend with Rust backend logic and JavaScript↔Rust IPC. **REVISED** (see §1): the editor doesn't use that IPC path for the engine itself — the engine is the `an1mbyt3-wasm` module running directly in the WebView, same as Rive's own web editor. Tauri's role here is native shell only (window chrome, future file dialogs/disk I/O).
Official docs: https://v2.tauri.app/
