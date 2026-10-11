import { useCallback, useEffect, useRef, useState, type MouseEvent as ReactMouseEvent, type PointerEvent as ReactPointerEvent } from 'react'
import { screenToWorld, worldToScreen } from './camera'
import { DEMO_SVG } from './demoScene'
import { CanvasArea } from './components/CanvasArea'
import { SnapGuides } from './components/SnapGuides'
import { DockablePanel } from './components/DockablePanel'
import { DockGroup } from './components/DockGroup'
import { IconRail } from './components/IconRail'
import { InspectorPanel } from './components/InspectorPanel'
import { LayersPanel } from './components/LayersPanel'
import { ResourcesPanel } from './components/ResourcesPanel'
import { ComponentsPanel } from './components/ComponentsPanel'
import { EffectsPanel } from './components/EffectsPanel'
import { CameraKindIcon, CloseIcon, LightKindIcon, TrailKindIcon, ZoomInIcon, ZoomOutIcon } from './components/icons'
import { FloatingPanel } from './components/FloatingPanel'
import { GradientHandle } from './components/GradientHandle'
import { PathEditOverlay } from './components/PathEditOverlay'
import { TilemapPaintOverlay } from './components/TilemapPaintOverlay'
import { TilePalettePanel } from './components/TilePalettePanel'
import { PaintAssetEditor, type PaintImage } from './components/PaintAssetEditor'
import { blankSession, withLive, type PaintSession } from './paint/session'
import { sessionFromDrawing, sessionToAnimation } from './paint/drawingIo'
import { StateMachineDock } from './components/StateMachineDock'
import { LocalErrorBoundary } from './components/LocalErrorBoundary'
import { TimelinePanel, type TimelinePanelHandle } from './components/TimelinePanel'
import { TitleBar } from './components/TitleBar'
import { applyStyle, applyTheme, CLEAR_COLOR, GRID_COLOR, loadInitialStyle, loadInitialTheme, type Theme, type UiStyle } from './theme'
import { LayoutMetricsContext, layoutMetricsFor } from './layoutMetrics'
import { TransformHandles } from './components/TransformHandles'
import { TopToolbar, type Mode } from './components/TopToolbar'
import { useProject } from './project/useProject'
import { PrefabBar } from './components/PrefabBar'
import { ASSET_DRAG_MIME, ProjectBrowser, type DraggedAsset } from './components/ProjectBrowser'
import { ShaderDock } from './components/ShaderDock'
import { useView3D } from './game/useView3D'
import { audio } from './game/audio'
import {
  applyPrefabInstance,
  createPrefab,
  deletePrefab,
  addImageNode,
  instantiatePrefab,
  prefabInstanceRoot,
  renamePrefab,
  revertPrefabInstance,
  unpackPrefabInstance,
  addAnimatedDrawing,
  updateAnimatedDrawing,
  renameDrawing,
  deleteDrawing,
  placeDrawing,
} from './ipc/wasmEditor'
import { SceneSwitcher } from './components/SceneSwitcher'
import type {
  Node3D,
  Node3DItem,
  ArtboardSummary,
  Blend1DDef,
  BlendMode,
  CameraConfig,
  Light2DConfig,
  Light2DType,
  ShadowCasterConfig,
  TrailConfig,
  ConditionDef,
  Easing,
  GradientDef,
  DocumentSummary,
  ImageProcessing,
  InputKind,
  LoopMode,
  NodeBehaviorEntry,
  OpaqueId,
  ParticleSystemConfig,
  PlaybackDirection,
  PostEffectLayer,
  Path,
  ProgramDef,
  PropertyName,
  ScriptFieldDef,
  StateMachineSummary,
  Stroke,
  Transform2D,
} from './ipc/document'
import { pickOpenAudioPath, pickOpenImagePath, pickOpenModelPath, pickOpenSvgPath, pickSaveHtmlPath, pickSaveExePath, pickSaveFilePath, exportDesktopGame, readBinaryFile, readTextFile, writeTextFile } from './ipc/nativeFile'
import { exportActiveAnimation, EXPORT_FORMAT_INFO, GifExportError, type AnimationExportOptions } from './exportGif'
import {
  addAnimation,
  addBoneNode,
  addComponent,
  addComponentVariant,
  addDemoHoverAnimation,
  addInstanceNode,
  addKeyframe,
  addParticleSystemNode,
  addShapeNode,
  addState,
  addStateMachine,
  addStateMachineInput,
  addTransition,
  groupNodes,
  moveNodeForward,
  sendNodeBackward,
  bringNodeToFront,
  sendNodeToBack,
  commitKeyframeTime,
  setKeyframeEasing,
  commitNodeAnchor,
  commitNodeRotation,
  commitNodeScale,
  commitNodeTranslation,
  createWasmEditor,
  deleteAnimation,
  deleteComponent,
  deleteNode,
  deleteNodes,
  addTextNode,
  addGoogleFont,
  setTextFont,
  setTile,
  addTilemapNode,
  resizeTilemap,
  setTilemapCellSize,
  setNodeBehaviors,
  addScript,
  setScriptFields,
  addArtboard,
  deleteArtboard,
  renameArtboard,
  setActiveArtboard,
  addCameraNode,
  addLight2DNode,
  addTrailNode,
  setLight2D,
  setTrail,
  setShadowCaster,
  setNodeUnlit,
  setCamera as setCameraNode,
  setActiveCamera,
  setGroupYSort,
  setNodeZIndex,
  importSvg,
  duplicateNodes,
  setPostEffects,
  deleteState,
  deleteStateMachine,
  fireTrigger,
  getDocumentSummary,
  getStateMachineSummary,
  loadGeometryDashDemo,
  loadNinjaDemo,
  loadFpsDemo,
  loadStressDemo,
  loadRigDemo,
  loadDungeonDemo,
  loadMatch3Demo,
  loadLiquidDemo,
  loadTortureDemo,
  load3DDemo,
  loadUiDemo,
  loadResponsiveDemo,
  loadHtmlDemo,
  loadPhysics3DDemo,
  loadTreesDemo,
  loadLeerDemo,
  loadGooDemo,
  loadGooShapesDemo,
  loadClothDemo,
  loadLanDemo,
  loadBoomDemo,
  loadVoxelDemo,
  loadShatterDemo,
  addNode3D,
  setNode3D,
  importModel,
  gltfExternalUris,
  deleteAudio,
  addUiNode,
  loadTacticsDemo,
  loadPhysicsDemo,
  pickNodeAt,
  previewKeyframeTime,
  previewNodeAnchor,
  previewNodeRotation,
  previewBoneAim,
  makeRagdoll,
  addIkTarget,
  bindSpriteSkin,
  previewBoneIk,
  commitBonePose,
  previewNodeScale,
  previewNodeTranslation,
  redo as wasmRedo,
  removeKeyframe,
  removeTransition,
  reparentNode,
  reparentNodeBefore,
  setActiveAnimation,
  setActiveStateMachine,
  setAnimationDuration,
  setArtboardSize,
  setArtboardGravity,
  setArtboardResolutionScale,
  setArtboardScreen,
  setArtboardGravity3D,
  setPreviewScreen,
  setBoneLength,
  setImageProcessing,
  setComponentDefaultFillColor,
  setGroupMask,
  setInstanceFillColorOverride,
  setInstanceVariant,
  setMode as setWasmMode,
  setNodeAnchor,
  setNodeClasses,
  setNodeClipChildren,
  previewShapePath,
  setShapeImage,
  setShapePath,
  replayParticleSystem,
  setParticleSystem,
  setShapeStroke,
  addMotionPathKeyframe,
  setMotionPathGuide,
  removeMotionPathTrack,
  setKeyDown,
  setNodeId,
  setNodeRotationDegrees,
  setNodeScale,
  setNodeTranslation,
  setScrubTime,
  setSelectedNode,
  setShapeBlendMode,
  setFillColor,
  setGradient,
  setTextContent,
  setTextSize,
  setStateMachineBoolInput,
  setStateMachineEnumInput,
  setStateMachineInitialState,
  addStateMachineLayer,
  deleteStateMachineLayer,
  setStateMachineLayerMask,
  setStateMachineLayerInitialState,
  setStateBlend,
  setStateProgram,
  setStateMachineProgramsEnabled,
  setStateMachineNumberInput,
  setStatePosition,
  setTransition,
  solveTwoBoneIk,
  renameNode,
  renameStateMachine,
  undo as wasmUndo,
  updateState,
  type WasmEditor,
} from './ipc/wasmEditor'
import type { ShapePreset, UiMenuItem } from './ipc/wasmEditor'
import type { ScreenSettings } from './ipc/document'
import { responsiveView, SCREEN_PRESETS } from './game/screen'
import { htmlOverlay, type HtmlItem } from './game/htmlOverlay'
import { useDockLayout, groupMemberKeys, type PanelKey } from './hooks/useDockLayout'
import { isBoolean, isNumber, usePersistedState } from './hooks/persisted'
import { LEFT_DOCK_OFFSET, MAX_BOTTOM_DOCK_HEIGHT, RAIL_WIDTH, MIN_BOTTOM_DOCK_HEIGHT, TIMELINE_HEIGHT, TOP_CLEARANCE } from './layoutConstants'
import { collectSnapTargets, snapTranslation } from './snapping'
import { gameConsole, installAsyncErrorCapture, type ConsoleOrigin } from './game/console'
import { ConsoleBadge, ConsolePanel, consoleClearOnPlay } from './components/ConsolePanel'
import { sceneProps, setPropertyDefs } from './game/props'
import { resetFunctions } from './game/events'
import { iconFamily } from './game/googleFonts'
import { setProjectFiles } from './game/projectFiles'
import { CodeWorkspace, type CodeLocation } from './components/CodeWorkspace'
import { gameNet } from './game/net'
import { mouseToWorld, normalizeKeyName, PointerGestures, ScriptHost, syncMouseButtons } from './game/scriptHost'
import { BoneGizmos } from './components/BoneGizmos'
import { adaptiveResolution, renderDpr } from './game/adaptiveResolution'
import { PerfMeter } from './game/perf'
import { buildGameHtml } from './game/exportGame'

const MIN_CAMERA_ZOOM = 0.1
const MAX_CAMERA_ZOOM = 8
const CAMERA_ZOOM_STEP = 1.1
/** Screen-pixel snap "pull" radius for the move-drag alignment snapping —
 * divided by `camera.zoom` before use so it feels the same size regardless
 * of zoom level (see `snapDrag` below). */
const SNAP_THRESHOLD_PX = 8

function idsEqual(a: OpaqueId, b: OpaqueId): boolean {
  return a.idx === b.idx && a.version === b.version
}

/** Converts a *world*-space delta (`dx`/`dy`, in artboard units — what a
 * mouse-move actually measures, via `toArtboardPoint`) into the equivalent
 * delta in a node's own *local* space, given its parent's world transform.
 * A node's `Transform2D::translation` is always relative to its immediate
 * parent, not the artboard directly — under an ancestor with non-identity
 * scale/rotation (e.g. an imported Lottie file's own outer 40%-scale
 * precomp layer, or any hand-authored scaled/rotated group), one world unit
 * of mouse movement is *not* one local unit of translation. Adding a raw
 * world delta straight onto a node's local origin (as this file's drag
 * handler used to) silently under- or over-shoots depending on the
 * ancestor chain's accumulated scale, which reads as "dragging doesn't
 * move the shape properly" — exactly the bug this fixes. Only the
 * rotation/scale *ratio* matters for a delta (unlike converting an
 * absolute position, `parent.translation` cancels out entirely when
 * differencing two world points), so this is just `TransformHandles.tsx`'s
 * own `localFor`'s translation math, applied to a delta instead of a point.
 */
function localDelta(parent: Transform2D | null, dx: number, dy: number): { x: number; y: number } {
  if (!parent) return { x: dx, y: dy }
  const cos = Math.cos(parent.rotation)
  const sin = Math.sin(parent.rotation)
  return {
    x: (dx * cos + dy * sin) / parent.scale.x,
    y: (-dx * sin + dy * cos) / parent.scale.y,
  }
}



/**
 * The editor shell: custom title bar + icon rail + toolbar (all permanent,
 * non-draggable chrome) plus canvas-dominant layout with floating panels
 * overlaid on top, per AN1MBYT3_BUILD_PLAN.md §15 ("Rive's structure,
 * Procreate's polish"). Three different panel behaviors, deliberately not
 * unified into one component (see each component's own doc comment for why):
 * - `TitleBar`/`IconRail`/`TopToolbar` are fixed chrome — never move, exactly
 *   one of each.
 * - `LayersPanel`/`InspectorPanel` dock to the left/right screen edge when
 *   dragged near it (`DockablePanel`) — Layers starts docked left (after the
 *   icon rail), Inspector docked right.
 * - `TimelinePanel` is permanently bottom-docked, no dragging at all — same
 *   "fixed chrome" reasoning as the toolbar.
 *
 * `App` owns the app's real state: the one `WasmEditor` instance
 * (`ipc/wasmEditor.ts`) for the whole session, the document summary it
 * reports, and which node is selected. This replaced a native-Rust
 * Tauri-IPC document + a separate readback-bridge canvas — now both the
 * document *and* the rendering live in the same wasm module the web
 * runtime uses (`crates/an1mbyt3-wasm/src/editor.rs`'s module doc explains
 * why: it's the same architecture Rive's own web-based editor uses). The
 * `WasmEditor` is created once, on mount, against `canvasRef`'s element;
 * `CanvasArea` only renders the `<canvas>` tag itself, `App` drives its
 * `requestAnimationFrame` render loop.
 *
 * Every edit handler (`handleSetTranslation`/`handleSetRotation`/`handleSetScale`)
 * calls straight into the `WasmEditor` instance and replaces `doc` with the
 * response — a real, undoable `an1mbyt3_core::Command`, exactly as before,
 * just synchronous now (no IPC round-trip to await). `handleUndo`/`handleRedo`
 * do the same for the toolbar's undo/redo buttons.
 */
/** Must match `crates/an1mbyt3-wasm/src/dto.rs`'s `CAMERA_GIZMO_HALF_EXTENT`
 * — the camera icon drawn here is exactly the area `pick_node_at` hit-tests. */
const CAMERA_GIZMO_HALF_EXTENT = 24
/** Mirrors `dto.rs`'s `SMALL_GIZMO_HALF_EXTENT` — a light/trail's clickable center. */
const LIGHT_GIZMO_HALF_EXTENT = 12

/** The bottom dock's Animate/State Machine tab strip height — part of the
 * dock's own clearance (see `bottomDockClearance`). */
const BOTTOM_TAB_STRIP_HEIGHT = 30

/** The Console's height in Playground (kept small: the game is the point). */
const PLAY_CONSOLE_HEIGHT = 200

function App() {
  const width = typeof window !== 'undefined' ? window.innerWidth : 1200

  const canvasRef = useRef<HTMLCanvasElement>(null)
  const editorRef = useRef<WasmEditor | null>(null)
  // `TimelinePanel`'s playback tick — see that component's own doc comment
  // for why `frame()`'s already-reliable rAF loop drives it directly
  // (`timelinePanelRef.current?.tick(deltaSeconds)`) instead of `TimelinePanel`
  // running a second, independent `requestAnimationFrame` loop of its own.
  const timelinePanelRef = useRef<TimelinePanelHandle>(null)
  // `TransformHandles`' own overlay container — kept in sync with the
  // canvas's own position/size (see `CanvasArea`'s doc comment) so its
  // handle-drag math's `getBoundingClientRect()` agrees with `toArtboardPoint`'s.
  const transformHandlesContainerRef = useRef<HTMLDivElement>(null)
  // Canvas freeze for dock-edge resize drags — see `setCanvasFrozen`.
  const canvasFrozenRef = useRef(false)
  // Re-measures the canvas's container and applies its size (buffer +
  // surface); assigned by the `ResizeObserver` effect below once mounted.
  const applyCanvasSizeRef = useRef<() => void>(() => {})
  /** While a dock edge (side panel, side group, or bottom dock) is being
   * drag-resized, the canvas stays frozen at its pre-drag pixel size —
   * pinned to that CSS size so the browser neither stretches it nor
   * reallocates it — instead of being resized every pointer-move. In Solid
   * style the canvas viewport is carved out between the docks, so without
   * this every resize step reset the canvas's backing buffer (which blanks
   * it) and reconfigured the GPU surface: constant flashing. The real size
   * is applied once, on release. Glass style never resizes the viewport
   * during these drags, so this is a harmless no-op there. */
  const setCanvasFrozen = (active: boolean) => {
    const canvas = canvasRef.current
    canvasFrozenRef.current = active
    if (!canvas) return
    if (active) {
      canvas.style.width = `${canvas.clientWidth}px`
      canvas.style.height = `${canvas.clientHeight}px`
      // Safety net: a drag that ends without its own handle ever seeing a
      // `pointerup` (pointer capture lost, `pointercancel`) must not leave
      // the canvas frozen forever.
      const release = () => {
        window.removeEventListener('pointerup', release)
        window.removeEventListener('pointercancel', release)
        if (canvasFrozenRef.current) setCanvasFrozen(false)
      }
      window.addEventListener('pointerup', release)
      window.addEventListener('pointercancel', release)
    } else {
      canvas.style.width = ''
      canvas.style.height = ''
      applyCanvasSizeRef.current()
    }
  }
  const dock = useDockLayout({
    layers: { initialPosition: { x: LEFT_DOCK_OFFSET, y: TOP_CLEARANCE }, initialRegion: 'left', defaultWidth: 224 },
    inspector: { initialPosition: { x: width - 288, y: TOP_CLEARANCE }, initialRegion: 'right', defaultWidth: 256 },
    // Starts floating (not pre-docked to either edge — both are already
    // spoken for) at a position that doesn't initially overlap either one;
    // the user docks it themselves by dragging onto Layers'/Inspector's
    // title bar (tabs) or body (split), same as any other panel here.
    resources: { initialPosition: { x: width - 288, y: TOP_CLEARANCE + 340 }, initialRegion: null, defaultWidth: 256 },
    // Also starts floating, clear of the other 3's own starting spots —
    // an author docks/groups it themselves the same way.
    tilePalette: { initialPosition: { x: width / 2 - 140, y: TOP_CLEARANCE }, initialRegion: null, defaultWidth: 280 },
    // Also starts floating — these 2 used to be plain, non-dockable
    // `FloatingPanel`s toggled by their own `componentsOpen`/`effectsOpen`
    // booleans; folded into the same dock system as every other panel here
    // so they can tab/split with the rest too.
    components: { initialPosition: { x: LEFT_DOCK_OFFSET + 240, y: TOP_CLEARANCE }, initialRegion: null, defaultWidth: 280 },
    effects: { initialPosition: { x: LEFT_DOCK_OFFSET + 260, y: TOP_CLEARANCE + 40 }, initialRegion: null, defaultWidth: 240 },
  }, setCanvasFrozen)

  const [doc, setDoc] = useState<DocumentSummary | null>(null)
  /** What the Code tab should open next (a Console link, an Inspector button). */
  const [codeRequest, setCodeRequest] = useState<(CodeLocation & { nonce: number }) | null>(null)
  // Code files reach the runtime (HTML UI, scripts) through `projectFiles`.
  useEffect(() => {
    setProjectFiles(doc?.textFiles)
    setPropertyDefs(doc?.properties)
  }, [doc])
  // The latest document for the mount-once render loop (scene list/names).
  const docRef = useRef(doc)
  docRef.current = doc
  // `selection` is the "active" node — everything that edits exactly one
  // node's properties (Inspector fields, keyframes, IK, bone length, ...)
  // still reads only this, unchanged from before multi-select existed.
  // `multiSelectedIds` is the full highlighted set (Ctrl/Cmd/Shift-click adds
  // to it; a plain click on a node not already in it replaces it with just
  // that node) — what the Layers panel highlights, the canvas draws a
  // bounding box around, and Delete/move-drag act on together. `selection`
  // always tracks whichever id was most recently added to (or, after a
  // removal, the last one remaining in) `multiSelectedIds` — see
  // `selectSingle`/`toggleMultiSelect` below. Kept as two separate pieces of
  // state rather than one `nodeIds: OpaqueId[]` array so every existing
  // single-target handler (there are ~25 of them) keeps working unchanged —
  // only the handful of genuinely multi-node operations (delete, move-drag,
  // the Layers panel's highlight) need to know about the whole set.
  const [selection, setSelection] = useState<{ artboardId: OpaqueId; nodeId: OpaqueId } | null>(null)
  const [multiSelectedIds, setMultiSelectedIds] = useState<OpaqueId[]>([])
  /** Which Resources-panel asset is selected — mutually exclusive with node
   * `selection`/`multiSelectedIds` above (selecting an asset clears node
   * selection, and vice versa via `selectSingle`/`toggleMultiSelect`), same
   * "one Inspector, one active thing" convention Unity's Project/Hierarchy
   * windows both feeding one Inspector already established as the model
   * this panel follows. `InspectorPanel` renders asset-mode fields whenever
   * this is non-null instead of its usual node-shaped form. */
  const [selectedAssetId, setSelectedAssetId] = useState<OpaqueId | null>(null)
  // `Ctrl+C`'s "copy" — just remembers which nodes were selected at the
  // time, in-memory only (no real OS/cross-app clipboard integration). A
  // ref, not state: copying doesn't need to trigger a re-render, and reading
  // a stale value a `useState` closure might've captured across renders
  // isn't a risk to guard against here.
  const clipboardRef = useRef<{ artboardId: OpaqueId; nodeIds: OpaqueId[] } | null>(null)
  // The group currently "entered" for canvas click-to-select purposes —
  // `null` means top-level (clicking anywhere inside a group selects that
  // whole group as one unit, matching Figma/Illustrator's default: a group
  // acts like a single object until you deliberately drill into it).
  // Double-clicking a selected group enters it (`handleCanvasDoubleClick`);
  // clicking somewhere entirely outside the current scope, or an empty-canvas
  // click, backs back out to `null`. See `resolveCanvasSelection`'s doc
  // comment for the actual click-resolution rule this drives.
  const [selectionScope, setSelectionScope] = useState<OpaqueId | null>(null)
  // Double-clicking a leaf `Shape` (no children — see `handleCanvasDoubleClick`)
  // enters path-editing mode for it instead of drilling in; `null` means no
  // shape is currently being path-edited. Cleared whenever the selection
  // changes away from this exact node (the effect below) so switching
  // selection always leaves path-edit mode rather than silently editing
  // whatever's newly selected.
  const [pathEditNodeId, setPathEditNodeId] = useState<OpaqueId | null>(null)
  // Paint Asset Mode — see `PaintAssetEditor.tsx`'s own doc comment for why
  // its action bar (`paintActionBarTarget`) is a portal `TopToolbar` itself
  // renders (merged into its own row), rather than a prop either component
  // could just own independently: both are true siblings that need the
  // same live DOM node. `paintDrawingName`/`paintImageRef` are the drawing's
  // name and latest artwork "Save as Resource" (triggered from `TopToolbar`,
  // since its own Save button lives there now) reads; `editingDrawing` is
  // the Drawing resource being edited (re-saving updates it instead of
  // adding a copy); `paintSession` its frames × layers (`paint/session.ts`).
  const [paintActionBarTarget, setPaintActionBarTarget] = useState<HTMLDivElement | null>(null)
  // The icon rail's slot Paint's tools render into (Paint keeps Design's rail).
  const [paintToolsTarget, setPaintToolsTarget] = useState<HTMLDivElement | null>(null)
  const [paintDrawingName, setPaintDrawingName] = useState('New Drawing')
  const paintImageRef = useRef<PaintImage | null>(null)
  const [editingDrawing, setEditingDrawing] = useState<OpaqueId | null>(null)
  const [paintSession, setPaintSession] = useState<PaintSession>(blankSession)
  const [paintSessionKey, setPaintSessionKey] = useState('blank-0')
  // A short message over the canvas (e.g. why a save didn't happen).
  const [notice, setNotice] = useState<string | null>(null)
  useEffect(() => {
    if (!notice) return
    const timer = window.setTimeout(() => setNotice(null), 3500)
    return () => window.clearTimeout(timer)
  }, [notice])
  // One-shot flag: "Load Demo Scene" sets this so the freshly-mounted
  // `TimelinePanel` (the bottom dock opens to its Animate tab as part of
  // loading the demo) starts playing the "Liftoff" animation immediately,
  // instead of the
  // keyframes just sitting there unplayed with no obvious way to tell the
  // animation was actually created. See `TimelinePanel`'s `autoPlayOnMount`
  // doc comment for why it's consumed (reset to `false`) right after firing.
  const [autoPlayAnimation, setAutoPlayAnimation] = useState(false)
  // Live position while a canvas translate-drag is in progress — see the
  // window `mousemove` handler below for why `TransformHandles` needs this
  // instead of just reading `selectedNode.transform.translation`.
  const [dragTranslation, setDragTranslation] = useState<{ x: number; y: number } | null>(null)
  // World-space alignment-guide positions currently active during a
  // move-drag (Canva/Figma-style snapping) — `null` on an axis with no
  // active snap. See `snapping.ts`'s `snapTranslation`.
  const [snapGuides, setSnapGuides] = useState<{ x: number | null; y: number | null }>({ x: null, y: null })
  // Controlled (not local to `TopToolbar`) so switching modes can drive
  // `WasmEditor.set_state_machine_active` and pick which bottom-dock content
  // shows (`TimelinePanel` for Animate, `StateMachineDock` for State
  // Machine) — see `TopToolbar`'s doc comment for why this no longer also
  // swaps the right-dock Inspector away.
  const [mode, setMode] = useState<Mode>('Design')
  // The State Machine tab's own "Run Programs" toggle — see
  // `handleToggleProgramsEnabled`'s doc comment. Playground ignores this
  // entirely (always runs programs, enforced Rust-side by `playground_active`).
  const [programsEnabled, setProgramsEnabled] = useState(false)
  // Which state-machine layer the State Machine tab's graph shows/edits and
  // the Layers panel's mask checkboxes target — `null` is the base
  // (top-level) layer. Lives here, not inside `StateMachineGraph`, since
  // `LayersPanel` (a sibling panel) needs the same selection for its own
  // mask checkboxes. Reset whenever the active state machine changes (a
  // different machine's layers aren't the same list) via the effect below.
  const [selectedStateMachineLayer, setSelectedStateMachineLayer] = useState<string | null>(null)
  // Shared with `IconRail`'s Settings button — both drive the same state
  // (see `IconRail.tsx`'s own doc comment for why it needs to live here,
  // not inside `TitleBar`, which used to own it alone).
  const [theme, setTheme] = useState<Theme>(loadInitialTheme)
  useEffect(() => applyTheme(theme), [theme])
  // Glass (floating, translucent panes over a full-window canvas) vs Solid
  // (hard panes that enclose the canvas) — see `theme.ts`'s `UiStyle`.
  const [uiStyle, setUiStyle] = useState<UiStyle>(loadInitialStyle)
  useEffect(() => applyStyle(uiStyle), [uiStyle])
  const metrics = layoutMetricsFor(uiStyle)
  // A failed `setStateProgram` call (see its own doc comment) — shown as a
  // dismissible banner next to the State Machine dock instead of being
  // allowed to throw all the way up to `RootErrorBoundary`.
  const [stateMachineError, setStateMachineError] = useState<string | null>(null)
  // Per-panel visibility for every `useDockLayout` panel — "customize the
  // layout" ask: closing one (`DockablePanel`'s own `onClose`, or a tab's own
  // close button inside a `DockGroup`) just flips its own entry here;
  // `IconRail`'s matching toggle flips it back. Components/Effects used to
  // be a separate, simpler `componentsOpen`/`effectsOpen` pair of booleans
  // driving a plain non-dockable `FloatingPanel` each — folded into this
  // same record once they joined the real dock system, so every panel here
  // (not just these 2) now shares one open/closed convention.
  const [panelVisible, setPanelVisible] = usePersistedState<Record<PanelKey, boolean>>(
    'panelVisible',
    {
      layers: true,
      inspector: true,
      resources: false,
      tilePalette: true,
      components: false,
      effects: false,
    },
    (v): v is Record<PanelKey, boolean> => typeof v === 'object' && v !== null && Object.values(v).every((x) => typeof x === 'boolean'),
  )
  // Resources moved from a side panel to the bottom dock: drop it from any
  // layout saved before that (docked tab groups included).
  useEffect(() => {
    dock.closePanel('resources')
    setPanelVisible((v) => (v.resources ? { ...v, resources: false } : v))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  // The Tile Palette panel's own current brush — lifted up here (not owned
  // by the panel itself) since painting happens on the *canvas*
  // (`TilemapPaintOverlay`), a different part of the tree than the palette
  // panel that picks it.
  const [selectedTile, setSelectedTile] = useState(0)
  const [stateMachine, setStateMachine] = useState<StateMachineSummary | null>(null)
  // One shared height for the whole bottom dock — not one per tab, so
  // switching tabs never shifts the canvas/dock boundary around the user.
  const [bottomDockHeight, setBottomDockHeight] = usePersistedState('bottomDockHeight', TIMELINE_HEIGHT, isNumber)
  // The bottom dock's own tab (Timeline/State Machine graph used to be 2
  // separate top-level `Mode`s — now they're one dockable, tabbed area,
  // same "+ add panel" open/close convention the other dockable panels use
  // (`IconRail`'s matching toggle buttons), so Design-mode canvas editing
  // and the Timeline/state graph can be open and visible at the same time
  // instead of being mutually exclusive. Forced to `StateMachine` (ignoring
  // both of these) while `mode === 'Playground'` — see `effectiveBottomTab`
  // below.
  // Which scene (artboard) is open — `null` means "the first one," see
  // `activeArtboard` below. Session state mirrored into `WasmEditor` via
  // `setActiveArtboard` on every switch.
  const [activeArtboardId, setActiveArtboardId] = useState<OpaqueId | null>(null)
  // Mirror for the mount-only render loop, written *synchronously* by
  // `switchToScene` (not just on re-render) so a script-triggered scene
  // switch is seen by the very next frame, when the Rust side has already
  // moved on but React hasn't re-rendered yet.
  const activeArtboardIdRef = useRef<OpaqueId | null>(null)
  activeArtboardIdRef.current = activeArtboardId
  // `Resources` is the project folder browser (Unity's Project window) —
  // a bottom tab like Unity's, next to the animation tools.
  // `Shader` is the material node-graph editor (Godot's VisualShader).
  // `Console` is the game console (logs + errors, `components/ConsolePanel`);
  // unlike the others it stays up while playing.
  // Playground keeps its own Console open / closed state: it starts as
  // just the tab (the game gets the whole view), whatever Design had open.
  const [playConsoleOpen, setPlayConsoleOpen] = useState(false)
  const [bottomDockTab, setBottomDockTab] = usePersistedState<'Animate' | 'StateMachine' | 'Resources' | 'Shader' | 'Console'>(
    'bottomDockTab',
    'Animate',
    (v): v is 'Animate' | 'StateMachine' | 'Resources' | 'Shader' | 'Console' => v === 'Animate' || v === 'StateMachine' || v === 'Resources' || v === 'Shader' || v === 'Console',
  )
  /** The canvas's 2D / 3D view toggle (only shown when the scene has 3D nodes). */
  const [sceneView, setSceneView] = usePersistedState<'2D' | '3D'>('sceneView', '3D', (v): v is '2D' | '3D' => v === '2D' || v === '3D')
  /** The material the Shader tab is editing. */
  const [shaderMaterialId, setShaderMaterialId] = useState<OpaqueId | null>(null)
  const [bottomDockOpen, setBottomDockOpen] = usePersistedState('bottomDockOpen', true, isBoolean)
  // The Timeline's current marquee/click keyframe selection, lifted up here
  // (not kept purely local to `TimelinePanel`) so the single global Delete/
  // Backspace handler below can tell "delete these keyframes" from "delete
  // the selected node" — see that handler's comment for why a second,
  // timeline-local keydown listener would race it instead of composing with it.
  const [keyframeSelection, setKeyframeSelection] = useState<{ property: PropertyName; time: number }[]>([])
  // The canvas's actual pixel-buffer size — no longer the artboard's own
  // size (see `CanvasArea`'s doc comment): a `ResizeObserver` effect below
  // keeps this (and the canvas element's real `width`/`height` attributes,
  // and `WasmEditor`'s own surface config via `editor.resize`) in sync with
  // however much screen space the canvas area actually has. Starts at a
  // plausible default so `WasmEditor::create` has *something* to build a
  // surface from before the observer's first measurement lands.
  const [canvasSize, setCanvasSize] = useState({ width: 640, height: 480 })
  // The canvas view camera (AN1MBYT3_BUILD_PLAN.md §15's "infinite canvas"
  // pan/zoom) — world-space pan point (what's centered in the viewport) plus
  // zoom (viewport pixels per world unit). Lives here, not in `WasmEditor`,
  // since the frontend already needs its own copy for `toArtboardPoint`'s
  // hit-testing math; passed into `editor.render()` fresh every frame
  // instead of a second source of truth (see `editor.rs`'s `render` doc
  // comment).
  const [camera, setCamera] = useState({ pan: { x: 0, y: 0 }, zoom: 1 })
  // On-canvas drag state — a plain ref, not React state: it's read/written
  // every `mousemove` while dragging, and none of that needs to trigger a
  // re-render (the canvas's own visual feedback comes from `render()`
  // reading the wasm document directly, not from React). `origin` is the
  // node's translation from *before* the drag started, captured once at
  // `mousedown` — see `commitNodeTranslation`'s doc comment for why that's
  // needed to make the drag's undo entry correct.
  const dragRef = useRef<{
    artboardId: OpaqueId
    nodeId: OpaqueId
    origin: { x: number; y: number }
    startPointer: { x: number; y: number }
    /** Every *other* currently multi-selected node's own pre-drag
     * translation — populated only when the dragged node was already part
     * of a multi-selection (see `handleCanvasMouseDown`), so a group-drag
     * moves everything together by the same snapped delta the primary
     * (`nodeId`) node resolves to. Each one gets its own separate
     * `commitNodeTranslation` call on mouseup — a real, if not fully
     * batched-into-one-undo-step, group move (see the mouseup handler's own
     * comment for why one combined undo step isn't done here yet). */
    others: { nodeId: OpaqueId; origin: { x: number; y: number } }[]
  } | null>(null)
  // An empty-canvas mousedown starts panning the camera instead of a node
  // drag (`handleCanvasMouseDown`) — same "plain ref, no re-render needed"
  // reasoning as `dragRef`.
  const panRef = useRef<{ startClientX: number; startClientY: number; startPan: { x: number; y: number } } | null>(null)
  const bottomDockResizeRef = useRef<{ pointerY: number; startHeight: number } | null>(null)
  // The render loop (below) is set up once, in a mount-only effect, so it
  // closes over whatever `mode` was at that moment — a ref kept in sync
  // every render is how it sees the *current* mode without re-running the
  // whole effect (and therefore re-creating the `WasmEditor`) every time the
  // toolbar's mode switches. `cameraRef`/`canvasSizeRef` are the same
  // pattern, for the same reason: `toArtboardPoint` and the render loop are
  // called from effects/handlers that would otherwise need to re-subscribe
  // on every camera/canvas-size change (camera changes on every pan/zoom
  // event, which would be constant churn).
  const modeRef = useRef<Mode>(mode)
  // Design mode's "preview on" screen size (`null` = the design size) —
  // pushed into the engine, which lays UI out for it.
  const [previewScreen, setPreviewScreenState] = useState<{ width: number; height: number } | null>(null)
  useEffect(() => {
    if (editorRef.current) setPreviewScreen(editorRef.current, previewScreen)
  }, [previewScreen])
  modeRef.current = mode
  // `ctx.globals` for Code-mode scripts (`scriptRuntime.ts`) — one plain
  // object per Playground run, shared by every scene's script so game state
  // survives `ctx.loadScene`. Swapped for a fresh object on each entry into
  // Playground (the effect below) so pressing Play always starts a new game.
  // `ctx.requestPointerLock()` arms this; the next canvas click (a real user
  // gesture, which browsers require) actually locks. Leaving Playground
  // releases the lock.
  const pointerLockArmedRef = useRef(false)
  // Errors from callbacks scripts hand to the DOM / timers land in the
  // Console instead of only in DevTools.
  useEffect(() => installAsyncErrorCapture(), [])
  useEffect(() => {
    if (mode === 'Playground') {
      // A new run: programs that didn't compile last time get another try
      // (their code may be fixed), and the Console starts fresh if asked.
      gameConsole.resetBroken()
      if (consoleClearOnPlay()) gameConsole.clear()
      // Properties back to their defaults, script functions re-registered.
      sceneProps.reset()
      resetFunctions()
    }
    if (mode !== 'Playground') {
      pointerLockArmedRef.current = false
      if (document.pointerLockElement) document.exitPointerLock()
      // Leaving Playground silences the game, and drops HTML UI a script
      // may have changed (it's rebuilt from the design next frame).
      audio.stopAll()
      htmlOverlay.clear()
      // ...and ends any multiplayer session (stops hosting too).
      gameNet.reset()
    }
  }, [mode])
  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    // Mouse buttons reach scripts as the held key "mouse" (Playground only),
    // alongside the keyboard keys in `ctx.keysDown`.
    const onDown = (event: PointerEvent) => {
      if (modeRef.current !== 'Playground') return
      gesturesRef.current.down(event)
      heldKeysRef.current.add('mouse')
      syncMouseButtons(heldKeysRef.current, event)
      if (pointerLockArmedRef.current && document.pointerLockElement !== canvas) {
        void canvas.requestPointerLock()
      }
    }
    const onUp = (event: PointerEvent) => {
      if (heldKeysRef.current.has('mouse')) gesturesRef.current.up(event)
      syncMouseButtons(heldKeysRef.current, event)
      // "mouse" = any button: only let go once they're all up.
      if (!event.buttons) heldKeysRef.current.delete('mouse')
    }
    canvas.addEventListener('pointerdown', onDown)
    window.addEventListener('pointerup', onUp)
    return () => {
      canvas.removeEventListener('pointerdown', onDown)
      window.removeEventListener('pointerup', onUp)
    }
  }, [])
  // Taps / swipes for scripts (mouse and touch alike).
  const gesturesRef = useRef(new PointerGestures())
  // F3 toggles the frame profiler overlay (any mode).
  const perfRef = useRef(new PerfMeter())
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'F3' || event.key === '`') {
        event.preventDefault()
        perfRef.current.toggle()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])
  const scriptGlobalsRef = useRef<Record<string, any>>({})
  useEffect(() => {
    if (mode === 'Playground') scriptGlobalsRef.current = {}
  }, [mode])
  // Same mount-only-render-loop-closure reasoning as `modeRef` — the
  // render loop's own `clear_r/g/b` arguments (`CLEAR_COLOR[themeRef.current]`)
  // need the *current* theme every frame without re-running the effect that
  // sets the loop up.
  const themeRef = useRef<Theme>(theme)
  themeRef.current = theme
  // Mirrors `programsEnabled`/`activeStateMachine` for the mount-only render
  // loop below, same "closured local would go stale" reasoning `modeRef`
  // itself already has — the loop needs both to decide whether a Code-mode
  // Programmable State's script should actually run this frame (Graph-mode
  // programs are gated Rust-side instead, via `setStateMachineProgramsEnabled`).
  const programsEnabledRef = useRef(programsEnabled)
  programsEnabledRef.current = programsEnabled
  const cameraRef = useRef(camera)
  cameraRef.current = camera
  // Currently-held keyboard keys, normalized the same way the Blocks-mode
  // forwarder below already does (`normalizeKeyName`) — a Code-mode
  // script's own `ctx.keysDown` reads this directly (see
  // `scriptRuntime.ts`'s `ScriptContext.keysDown` doc comment for why
  // Code mode needed its own copy: it has no Rust-side `KeyboardState` to
  // read the way Blocks/Graph do). A plain mutable `Set` ref, not React
  // state — updated every keydown/keyup, read every render-loop frame,
  // never itself a reason to re-render.
  const heldKeysRef = useRef<Set<string>>(new Set())
  const canvasSizeRef = useRef(canvasSize)
  canvasSizeRef.current = canvasSize
  // Set for the duration of `handleExportAnimation` — the live loop below checks
  // this and skips its own `render()` call while true, so its "whatever the
  // live camera/scrub state is" draws don't race with the export's own
  // direct `render()` calls (each sampling a specific export frame's time)
  // for control of the same canvas. A plain ref, not state: toggling it
  // shouldn't itself trigger a re-render.
  const exportingRef = useRef(false)
  // Set for the duration of an in-flight `async` demo load (`load_ninja_demo`'s
  // own doc comment: it holds `&mut self` across many sequential `.await`
  // `bake_and_cache_image` calls). This loop's own `editor.render(...)` call
  // below is a SEPARATE synchronous call into that same wasm object — if it
  // fires while that async method is still suspended between awaits, wasm-
  // bindgen's reentrancy guard throws ("recursive use of an object"), the
  // same `exportingRef` reasoning applies: skip (but still reschedule) this
  // loop's own render call for as long as a load is in flight, rather than
  // race it.
  const demoLoadingRef = useRef(false)

  // Every non-deleted scene, for the Scenes switcher's own listing.
  const allArtboards = doc?.artboards ?? []
  // `artboards` is deliberately *just the active scene*, as a one-element
  // list — everything else in this component (dozens of `artboards[0]`
  // reads, `LayersPanel`'s own tree) predates multiple scenes and always
  // meant "the artboard being edited," so narrowing the list here retargets
  // all of it at once instead of rewriting each call site. Falls back to the
  // first scene when `activeArtboardId` is unset or no longer resolves (a
  // deleted scene, a freshly opened file) — the same fallback
  // `WasmEditor::current_artboard_id` uses, so the two always agree on which
  // scene is showing.
  const activeArtboard = allArtboards.find((a) => activeArtboardId !== null && idsEqual(a.id, activeArtboardId)) ?? allArtboards[0]
  const artboards = activeArtboard ? [activeArtboard] : []
  // `null` while Paint's full canvas takeover leaves no room for it and in
  // Playground (a live run shows only the game plus the top bar),
  // otherwise whichever tab is open — see `bottomDockTab`'s own
  // doc comment for why this replaced 2 separate top-level `Mode`s.
  const effectiveBottomTab: 'Animate' | 'StateMachine' | 'Resources' | 'Shader' | 'Console' | null =
    mode === 'Playground' ? (playConsoleOpen ? 'Console' : null) : mode === 'Paint' || mode === 'Code' ? null : bottomDockOpen ? bottomDockTab : null
  const bottomDockVisible = effectiveBottomTab !== null
  // Same mount-only-render-loop-closure reasoning as `modeRef` — the loop's
  // own per-frame state-machine-summary refresh below needs to know whether
  // the State Machine tab is the one currently driving the preview without
  // re-running the effect that sets the loop up.
  const effectiveBottomTabRef = useRef(effectiveBottomTab)
  effectiveBottomTabRef.current = effectiveBottomTab
  // The Animate/State Machine tab strip sits directly on top of the bottom
  // dock and counts as part of it — everything that clears the bottom dock
  // (side docks, zoom widget, Solid's canvas viewport) clears the strip too,
  // rather than the strip floating over whatever's above it.
  const bottomTabStripVisible = mode === 'Design' && bottomDockOpen
  const bottomDockClearance = bottomDockVisible
    ? bottomDockHeight + metrics.edgeMargin * 2 + (bottomTabStripVisible ? BOTTOM_TAB_STRIP_HEIGHT : 0)
    : metrics.edgeMargin
  const activeStateMachine = artboards[0]?.stateMachines.find(
    (sm) => sm.id.idx === artboards[0]?.activeStateMachine?.idx && sm.id.version === artboards[0]?.activeStateMachine?.version,
  )
  const activeStateMachineRef = useRef(activeStateMachine)
  activeStateMachineRef.current = activeStateMachine

  // A different state machine's `layers` aren't the same list — switching
  // machines with a non-base layer still selected would otherwise silently
  // point the graph/mask checkboxes at a layer name that means nothing (or
  // nothing at all) in the new machine.
  useEffect(() => {
    setSelectedStateMachineLayer(null)
  }, [artboards[0]?.activeStateMachine?.idx, artboards[0]?.activeStateMachine?.version])

  // Drives `LayersPanel`'s per-node mask checkboxes — only present while
  // the State Machine tab has a non-base layer selected (the base layer
  // has no mask; it's unmasked by definition, see `StateMachineLayerDef`'s
  // own doc comment). Toggling flips this node's membership in the
  // selected layer's current `mask` and sends the whole updated set,
  // matching `setStateMachineLayerMask`'s own "whole-list swap" contract.
  const selectedLayerDef = selectedStateMachineLayer ? activeStateMachine?.layers.find((l) => l.name === selectedStateMachineLayer) : undefined
  const layerMaskEditing =
    effectiveBottomTab === 'StateMachine' && selectedStateMachineLayer && selectedLayerDef
      ? {
          mask: selectedLayerDef.mask,
          onToggleNode: (nodeId: OpaqueId) => {
            const current = selectedLayerDef.mask
            const next = current.some((id) => idsEqual(id, nodeId)) ? current.filter((id) => !idsEqual(id, nodeId)) : [...current, nodeId]
            handleSetStateMachineLayerMask(selectedStateMachineLayer, next)
          },
        }
      : undefined

  useEffect(() => {
    let cancelled = false
    let rafHandle = 0

    // Raw client-space cursor position for a Code-mode Programmable
    // State's `ctx.mouseX`/`mouseY` — converted to artboard space fresh
    // every frame (inside `frame`, below) via the same `screenToWorld` the
    // canvas's own hit-testing uses. Not tracked in Rust the way
    // `WasmEditor`'s own `pointer_position` feeds a Graph program's
    // `MousePosition` node instead — that's a separate, parallel path; Code
    // never touches the wasm boundary for this at all.
    let mouseClient = { x: 0, y: 0 }
    // Raw movement since the last frame (`ctx.mouseDX/DY`) — keeps working
    // under pointer lock, where clientX/Y freeze.
    let mouseDelta = { x: 0, y: 0 }
    // Last time the per-frame document summary was pushed into React state,
    // and the JSON it was built from (an identical one isn't pushed again).
    let lastUiSync = 0
    let lastSummaryJson = ''
    let lastFingerprint = -1
    // How long the last live document refresh took (fetch + React commit
    // kick-off) — the refresh interval scales with it, see below.
    let uiSyncCost = 0
    // Pointer (not mouse) events, so a finger on a touch screen moves the
    // game cursor too; `pointerdown` also places it, since a tap has no
    // hover-move before it.
    // The canvas's page rect, cached: reading it every frame forces a full
    // layout whenever the frame's HTML UI changed the DOM (a game HUD does,
    // every frame). Refreshed on resize / scroll / pointer down.
    let canvasRect: DOMRect | null = null
    const invalidateCanvasRect = () => {
      canvasRect = null
    }
    window.addEventListener('resize', invalidateCanvasRect)
    window.addEventListener('scroll', invalidateCanvasRect, true)
    window.addEventListener('pointerdown', invalidateCanvasRect, true)
    const rectObserver = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(invalidateCanvasRect) : null
    if (canvasRef.current) rectObserver?.observe(canvasRef.current)
    const handleMouseMove = (event: PointerEvent) => {
      if (event.type === 'pointermove') mouseDelta = { x: mouseDelta.x + event.movementX, y: mouseDelta.y + event.movementY }
      mouseClient = { x: event.clientX, y: event.clientY }
      // Chorded presses (a second button while one is held) only show up
      // as a change in `buttons` on a move.
      if (event.type === 'pointermove' && heldKeysRef.current.has('mouse')) syncMouseButtons(heldKeysRef.current, event)
    }
    window.addEventListener('pointermove', handleMouseMove)
    window.addEventListener('pointerdown', handleMouseMove)
    // Wheel since the last frame (UI Scroll Views in Playground).
    let wheel = { x: 0, y: 0 }
    const handleGameWheel = (event: WheelEvent) => {
      if (event.ctrlKey || event.target !== canvasRef.current) return
      wheel = { x: wheel.x + event.deltaX, y: wheel.y + event.deltaY }
    }
    window.addEventListener('wheel', handleGameWheel, { passive: true })

    async function init() {
      if (!canvasRef.current) return
      const editor = await createWasmEditor(canvasRef.current)
      if (cancelled) return

      editorRef.current = editor
      // Development only: the live engine on `window`, for DevTools.
      if (import.meta.env.DEV) (window as unknown as { __an1Editor?: unknown }).__an1Editor = editor
      if (canvasRef.current.parentElement) htmlOverlay.attach(canvasRef.current.parentElement, canvasRef.current)
      const initialSummary = getDocumentSummary(editor)
      setDoc(initialSummary)

      // `lastTime`/`lastActiveState` are plain closured locals (like
      // `cancelled`/`rafHandle` above), not React state or refs — they only
      // matter within this one running loop, recreated fresh if this effect
      // ever re-runs (it never does; empty deps, mount-only).
      let lastTime: number | null = null

      // Runs the scene's Code programs (every state-machine layer) — shared
      // with the exported standalone player, see `game/scriptHost.ts`.
      const scriptHost = new ScriptHost()
      // F3 performance overlay (see `game/perf.ts`).
      const perf = perfRef.current

      function frame(now: number) {
        if (cancelled || !editorRef.current) return
        // While an animation export is in progress, `handleExportAnimation` is driving
        // `render()` directly with its own per-export-frame time — skip this
        // loop's own call entirely (but still reschedule) so the two don't
        // fight over the same canvas.
        // Paint mode covers the scene entirely: don't spend every frame
        // rendering a canvas nobody can see (it competed with painting).
        if (exportingRef.current || demoLoadingRef.current || modeRef.current === 'Paint' || modeRef.current === 'Code') {
          lastTime = null
          rafHandle = requestAnimationFrame(frame)
          return
        }
        // The whole body is wrapped in try/catch, with `requestAnimationFrame`
        // rescheduled in `finally` — an exception anywhere in here (a Rust
        // panic surfaces as a thrown JS error at the wasm call site that
        // triggered it) used to permanently kill this loop: nothing
        // reschedules the next frame once one throws, so the canvas freezes
        // as a literal still bitmap and stays that way even across mode/tab
        // switches, since there's no second independent loop to fall back on
        // (see this file's other comments on why there's deliberately only
        // one). Logging and continuing turns a single bad frame into a
        // recoverable glitch instead of a dead app.
        try {
          const deltaSeconds = lastTime === null ? 0 : (now - lastTime) / 1000
          lastTime = now
          const cam = cameraRef.current
          // The canvas's backing buffer is `devicePixelRatio` times bigger than
          // its CSS size (see the `ResizeObserver` effect's `applySize`) so the
          // renderer actually has enough pixels to draw sharp edges into on a
          // HiDPI display — without this, the buffer rendered at CSS-pixel
          // resolution and stretched by the browser to fit is exactly the
          // "chunky, pixely" look this compensates for. Multiplying `zoom` by
          // the same factor here is what keeps the *visible* world-space area
          // identical despite the bigger buffer: `build_mvp`'s `visible_hw =
          // viewport_size / 2 / zoom` (`an1mbyt3-renderer`) would otherwise
          // show *more* content (effectively zoomed out) at the larger
          // viewport size, not the same content at higher density. `camera`
          // itself (state, screen<->world math, `TransformHandles`/`SnapGuides`
          // overlay positioning) stays entirely in logical/CSS-pixel terms —
          // this multiplication only happens at this one render-call boundary.
          // Drives the Timeline's playback clock from this same, already-running
          // loop — see `TimelinePanel`'s `tick` doc comment for why this isn't a
          // second, independent `requestAnimationFrame` loop living inside that
          // component instead. A no-op (`tick` itself checks `isPlaying`, returns
          // `null`) unless something is actually playing. Runs *before* `render`
          // (rather than after, as this used to) and its return value is passed
          // straight into that same `render` call's `playback_scrub_time`
          // argument so the resample and the draw happen inside one wasm call —
          // no frame where they're a call apart in either direction.
          const playbackScrubTime = timelinePanelRef.current?.tick(deltaSeconds) ?? null
          const [clearR, clearG, clearB] = CLEAR_COLOR[themeRef.current]
          const [gridR, gridG, gridB] = GRID_COLOR[themeRef.current]
          editorRef.current.render(
            deltaSeconds,
            cam.pan.x,
            cam.pan.y,
            cam.zoom * renderDpr(),
            playbackScrubTime ?? -1,
            renderDpr(),
            clearR,
            clearG,
            clearB,
            gridR,
            gridG,
            gridB,
          )

          // HTML UI: a live preview over the canvas wherever scripts aren't
          // driving it (then `ScriptHost` lays it out, interactive).
          const scriptsDriveHtml = modeRef.current === 'Playground' || (effectiveBottomTabRef.current === 'StateMachine' && programsEnabledRef.current)
          if (!scriptsDriveHtml) {
            const doc = docRef.current
            const shown = doc?.artboards.find((a) => activeArtboardIdRef.current !== null && idsEqual(a.id, activeArtboardIdRef.current)) ?? doc?.artboards[0]
            const items: HtmlItem[] = []
            for (const n of shown?.nodes ?? []) {
              for (const entry of n.behaviors) {
                if (!entry.enabled || entry.behavior.kind !== 'UiHtml') continue
                const { kind: _kind, ...config } = entry.behavior
                const t = n.worldTransform
                items.push({ id: n.id, config, translation: t.translation, rotation: t.rotation, scale: t.scale, visible: n.visible, zIndex: n.zIndex })
              }
            }
            htmlOverlay.update(items, { panX: cam.pan.x, panY: cam.pan.y, pxPerWorld: cam.zoom }, false, null)
          }

          if (effectiveBottomTabRef.current === 'StateMachine' || modeRef.current === 'Playground') {
            const syncStart = performance.now()
            const summary = getStateMachineSummary(editorRef.current)
            if (scriptHost.tick(summary, deltaSeconds)) setStateMachine(summary)
            // Outside Playground the side panels show the live pose, so the
            // React document is refreshed (throttled: re-rendering the
            // Layers/Inspector trees is expensive). Playground hides the
            // panels, so it skips the full document read entirely — it was
            // the single biggest per-frame cost on large scenes (~100 ms at
            // 4k nodes). Scripts read the scene through `ScriptHost`'s own
            // lean mirror instead.
            //
            // Adaptive: the refresh waits at least ~8x its own last cost, so
            // a small scene stays live at ~8 Hz while a huge one (where one
            // refresh can take tens of ms) never spends more than ~1/8 of
            // the main thread on it — the canvas and input stay smooth and
            // the panels just update less often.
            if (modeRef.current !== 'Playground' && now - lastUiSync > Math.max(120, uiSyncCost * 8)) {
              const refreshStart = performance.now()
              // Only when something actually changed: re-rendering every
              // panel 8x a second for an unchanged scene is wasted work
              // (and in React's dev build it piled up memory until the
              // WebView crashed).
              // A fingerprint first (microseconds): building the summary
              // itself costs tens of ms on a big project.
              const fingerprint = editorRef.current.summary_fingerprint()
              if (fingerprint !== lastFingerprint) {
                lastFingerprint = fingerprint
                const json = editorRef.current.document_summary_json()
                if (json !== lastSummaryJson) {
                  lastSummaryJson = json
                  setDoc(JSON.parse(json) as DocumentSummary)
                }
              }
              lastUiSync = now
              uiSyncCost = performance.now() - refreshStart
            }
            perf.add('sync', performance.now() - syncStart)
            // The *showing* scene (not `artboards[0]`), from the last known
            // document — scene identity doesn't change frame to frame.
            const knownDoc = docRef.current
            const currentArtboard =
              knownDoc?.artboards.find((a) => activeArtboardIdRef.current !== null && idsEqual(a.id, activeArtboardIdRef.current)) ?? knownDoc?.artboards[0]

            // Code programs run only in Playground, or on the State Machine
            // tab with its "Run Programs" toggle — the same gate Rust applies
            // to Graph programs (`WasmEditor::render`).
            const programsRunning = modeRef.current === 'Playground' || programsEnabledRef.current
            if (programsRunning && canvasRef.current && currentArtboard) {
              canvasRect ??= canvasRef.current.getBoundingClientRect()
              const rect = canvasRect
              const editor = editorRef.current
              const scriptsStart = performance.now()
              const sceneName = scriptHost.run(summary, {
                editor,
                deltaSeconds,
                scene: { id: currentArtboard.id, name: currentArtboard.name },
                machine: activeStateMachineRef.current,
                mouseWorld: mouseToWorld(editor, currentArtboard.id, mouseClient.x - rect.left, mouseClient.y - rect.top, cameraRef.current, canvasSizeRef.current),
                mouseDelta,
                wheel,
                gestures: gesturesRef.current.take(),
                keysDown: heldKeysRef.current,
                pointerLocked: document.pointerLockElement === canvasRef.current,
                requestPointerLock: () => {
                  pointerLockArmedRef.current = true
                },
                exitPointerLock: () => {
                  pointerLockArmedRef.current = false
                  if (document.pointerLockElement) document.exitPointerLock()
                },
                camera: { panX: cam.pan.x, panY: cam.pan.y, zoom: cam.zoom },
                setCamera: (panX, panY, zoom) => setCamera({ pan: { x: panX, y: panY }, zoom }),
                globals: scriptGlobalsRef.current,
              })
              perf.add('scripts', performance.now() - scriptsStart)
              // `ctx.loadScene` only queues — switch now that every script
              // has finished running against the old scene's nodes.
              if (sceneName !== null) {
                const target = knownDoc?.artboards.find((a) => a.name === sceneName)
                if (target) switchToSceneRef.current(target.id)
                else console.warn(`ctx.loadScene: no scene named "${sceneName}"`)
              }
            }
          }
          mouseDelta = { x: 0, y: 0 }
          wheel = { x: 0, y: 0 }
          perf.endFrame(editorRef.current, deltaSeconds * 1000, now)
          // Hold the frame rate at any window size while a game plays.
          adaptiveResolution.update(editorRef.current, deltaSeconds * 1000, now, modeRef.current === 'Playground')
        } catch (err) {
          // The frame is skipped, the loop carries on. Reported in the game
          // Console (repeats collapse into a count there).
          gameConsole.add('error', `${err instanceof Error ? `${err.name}: ${err.message}` : String(err)} — this frame was skipped`, { kind: 'Engine' }, err instanceof Error ? err.stack : undefined)
        } finally {
          rafHandle = requestAnimationFrame(frame)
        }
      }
      rafHandle = requestAnimationFrame(frame)
    }

    init().catch((err) => console.error('WasmEditor initialization failed', err))

    return () => {
      cancelled = true
      cancelAnimationFrame(rafHandle)
      window.removeEventListener('pointermove', handleMouseMove)
      window.removeEventListener('pointerdown', handleMouseMove)
      window.removeEventListener('resize', invalidateCanvasRect)
      window.removeEventListener('scroll', invalidateCanvasRect, true)
      window.removeEventListener('pointerdown', invalidateCanvasRect, true)
      rectObserver?.disconnect()
      window.removeEventListener('wheel', handleGameWheel)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Keeps the canvas's actual pixel-buffer size (and `WasmEditor`'s surface
  // config) matched to however much screen space it actually has — a
  // `<canvas>`'s CSS size and its backing pixel-buffer `width`/`height` are
  // two different things, and only the latter affects both what
  // `WasmEditor::render`'s projection treats as the viewport and what
  // `toArtboardPoint` needs for its own hit-testing math. Mount-only (empty
  // deps): observes `canvasRef`'s own parent element, which never changes
  // identity for the life of this component.
  useEffect(() => {
    const canvas = canvasRef.current
    const container = canvas?.parentElement
    if (!canvas || !container) return

    function applySize(width: number, height: number) {
      // Frozen mid dock-resize — `setCanvasFrozen(false)` re-measures and
      // applies the final size once the drag ends.
      if (canvasFrozenRef.current) return
      const w = Math.max(1, Math.round(width))
      const h = Math.max(1, Math.round(height))
      // `canvas.width`/`canvas.height` (the backing pixel buffer) are set to
      // the *physical* size — CSS size times `devicePixelRatio` — while the
      // canvas's own CSS/layout size stays whatever its container gives it
      // (`className="h-full w-full"` in `CanvasArea.tsx`, unaffected by the
      // attribute values here). A backing buffer equal to the CSS size on a
      // HiDPI display gets stretched by the browser to fill the same box,
      // which is the blurry/blocky look this fixes — the same standard
      // technique any canvas-based app uses for a sharp HiDPI canvas.
      // `canvasSize` (React state) stays the *logical* CSS size: it feeds
      // `toArtboardPoint`'s `screenToWorld` math, which has to match the
      // CSS-pixel coordinates `MouseEvent.clientX/clientY` already report,
      // and the `TransformHandles`/`SnapGuides` overlays, which are
      // CSS-positioned `<div>`s over the same CSS-sized box — none of that
      // should know or care about device pixels. Only the actual render
      // call (`frame()`'s `editor.render(...)`) needs the physical size, via
      // `editor.resize(...)` here plus the compensating `zoom` multiply there.
      // (`renderDpr`: the display's ratio × the adaptive-resolution scale.)
      const dpr = renderDpr()
      const physicalW = Math.max(1, Math.round(w * dpr))
      const physicalH = Math.max(1, Math.round(h * dpr))
      canvas!.width = physicalW
      canvas!.height = physicalH
      setCanvasSize({ width: w, height: h })
      editorRef.current?.resize(physicalW, physicalH)
    }

    const rect = container.getBoundingClientRect()
    applySize(rect.width, rect.height)
    applyCanvasSizeRef.current = () => {
      const now = container.getBoundingClientRect()
      applySize(now.width, now.height)
    }

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) applySize(entry.contentRect.width, entry.contentRect.height)
    })
    // Adaptive resolution changed the buffer scale: re-apply the size.
    adaptiveResolution.onChange = () => applyCanvasSizeRef.current?.()
    observer.observe(container)
    return () => observer.disconnect()
  }, [])

  // Scroll/trackpad-swipe pans the camera; trackpad pinch or Ctrl+scroll
  // zooms toward the cursor (same "Chromium reports a pinch as `ctrlKey:
  // true`" convention `TimelinePanel`'s zoom handling uses). A real
  // (non-passive) native listener, not React's `onWheel` — same
  // can't-`preventDefault()`-a-passive-listener reasoning as every other
  // wheel handler in this app. Mount-only: reads `cameraRef`/`canvasSizeRef`
  // and uses functional `setCamera` updates, so it never goes stale despite
  // never re-subscribing.
  // Shared by the canvas's own wheel listener below, the zoom widget's
  // +/-/reset buttons, and `StateMachineDock`'s forwarded ctrl+wheel (see
  // `handleDockWheel`) — one place computing "zoom by this factor, keeping
  // `(canvasX, canvasY)` fixed on screen" instead of three copies of the
  // same pan-compensation math drifting apart.
  const zoomAtCanvasPoint = useCallback((canvasX: number, canvasY: number, zoomFactor: number) => {
    setCamera((cam) => {
      const newZoom = Math.min(MAX_CAMERA_ZOOM, Math.max(MIN_CAMERA_ZOOM, cam.zoom * zoomFactor))
      const dx = canvasX - canvasSizeRef.current.width / 2
      const dy = canvasY - canvasSizeRef.current.height / 2
      // Keeps the world point under `(canvasX, canvasY)` fixed across the
      // zoom change — the standard "zoom toward cursor" feel.
      return {
        zoom: newZoom,
        pan: {
          x: cam.pan.x + dx / cam.zoom - dx / newZoom,
          y: cam.pan.y - dy / cam.zoom + dy / newZoom,
        },
      }
    })
  }, [])

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return

    function handleWheel(event: WheelEvent) {
      event.preventDefault()
      // In Playground the wheel is the game's (UI Scroll Views), not a pan.
      if (modeRef.current === 'Playground') return
      const rect = canvas!.getBoundingClientRect()
      const canvasX = event.clientX - rect.left
      const canvasY = event.clientY - rect.top

      if (event.ctrlKey) {
        zoomAtCanvasPoint(canvasX, canvasY, event.deltaY > 0 ? 1 / CAMERA_ZOOM_STEP : CAMERA_ZOOM_STEP)
      } else {
        setCamera((cam) => ({
          ...cam,
          pan: { x: cam.pan.x + event.deltaX / cam.zoom, y: cam.pan.y - event.deltaY / cam.zoom },
        }))
      }
    }

    canvas.addEventListener('wheel', handleWheel, { passive: false })
    return () => canvas.removeEventListener('wheel', handleWheel)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Suppresses WebView2's native right-click context menu (Print/Inspect/
  // "Send tab to your devices"/etc) app-wide — this is a design tool, not a
  // browser page, so that menu leaking through on any right-click anywhere
  // (canvas, panels, chrome) is always wrong, never useful. A capture-phase
  // window listener, same "always wins regardless of what's under the
  // cursor" shape `preventPageZoom` above already uses.
  useEffect(() => {
    function preventNativeContextMenu(event: MouseEvent) {
      event.preventDefault()
    }
    window.addEventListener('contextmenu', preventNativeContextMenu, { capture: true })
    return () => window.removeEventListener('contextmenu', preventNativeContextMenu, { capture: true })
  }, [])

  // Manual zoom widget (corner of canvas) — centers on the viewport's own
  // center (`dx`/`dy` both 0 in `zoomAtCanvasPoint`'s pan-compensation math),
  // not the cursor, since a button click has no cursor-over-world-point
  // gesture to preserve the way wheel/pinch does.
  const handleZoomIn = useCallback(() => {
    zoomAtCanvasPoint(canvasSizeRef.current.width / 2, canvasSizeRef.current.height / 2, CAMERA_ZOOM_STEP)
  }, [zoomAtCanvasPoint])
  const handleZoomOut = useCallback(() => {
    zoomAtCanvasPoint(canvasSizeRef.current.width / 2, canvasSizeRef.current.height / 2, 1 / CAMERA_ZOOM_STEP)
  }, [zoomAtCanvasPoint])
  const handleResetZoom = useCallback(() => {
    setCamera((cam) => ({ ...cam, zoom: 1 }))
  }, [])

  const bottomDockResizeHandleProps = {
    onPointerDown: (event: ReactPointerEvent) => {
      if (!bottomDockVisible) return
      bottomDockResizeRef.current = { pointerY: event.clientY, startHeight: bottomDockHeight }
      setCanvasFrozen(true)
      event.currentTarget.setPointerCapture(event.pointerId)
      event.preventDefault()
    },
    onPointerMove: (event: ReactPointerEvent) => {
      const origin = bottomDockResizeRef.current
      if (!origin) return
      const nextHeight = Math.min(MAX_BOTTOM_DOCK_HEIGHT, Math.max(MIN_BOTTOM_DOCK_HEIGHT, origin.startHeight + (origin.pointerY - event.clientY)))
      setBottomDockHeight(nextHeight)
    },
    onPointerUp: () => {
      if (bottomDockResizeRef.current) setCanvasFrozen(false)
      bottomDockResizeRef.current = null
    },
  }

  // Ctrl+scroll/pinch anywhere the cursor happens to sit OUTSIDE the canvas
  // (toolbar, panels, title bar, bare chrome) has no app-level wheel handler
  // of its own to `preventDefault()` it — and since `tauri.conf.json` now
  // sets `zoomHotkeysEnabled: true` (needed so the canvas/Timeline's own
  // ctrlKey-wheel zoom gestures even reach JS at all on WebView2 — see
  // that config's memory note), WebView2 is free to fall back to its own
  // native "zoom the whole page" behavior whenever nothing else stops it.
  // This window-level, capture-phase guard closes that gap: it never
  // touches `camera` itself, it just makes sure ctrl+wheel is *always*
  // prevented somewhere before WebView2 gets a chance to treat it as a
  // page-zoom request, regardless of what's under the cursor.
  useEffect(() => {
    function preventPageZoom(event: WheelEvent) {
      if (event.ctrlKey) event.preventDefault()
    }
    window.addEventListener('wheel', preventPageZoom, { passive: false, capture: true })
    return () => window.removeEventListener('wheel', preventPageZoom, { capture: true })
  }, [])

  // Keeps `WasmEditor`'s state machine/Animate-mode flags in sync with the
  // toolbar's mode via the combined `setMode` call — see `editor.rs`'s
  // `set_mode` doc comment for why this is one call (not the two separate
  // `setStateMachineActive`/`setAnimateModeActive` calls it replaced) and
  // for why entering "State Machine" intentionally takes over the demo's
  // "Square" node's pose for preview, while leaving *either* animated mode
  // now snaps every node back to its actually-authored Design-mode pose
  // instead of leaving it at whatever the last scrub/tick/state left behind.
  // `animate_mode_active` (folded into the same call) is
  // `apply_transform_edit`/`commit_node_translation`'s gate between
  // "overwrite the keyframe at the playhead" (Animate mode) and "rebase the
  // whole path" (Design mode) — see `editor.rs`'s doc comment on that.
  // Clears any live Timeline keyframe selection when leaving Animate mode
  // too, since `TimelinePanel` itself unmounts then (see the JSX below) and
  // couldn't clear it on its own. Guarded on `editorRef.current` existing
  // since `mode` can only actually change after the user interacts with the
  // toolbar, by which point `WasmEditor.create` has virtually always resolved.
  useEffect(() => {
    if (!editorRef.current || !artboards[0]) return
    const wasmMode = mode === 'Playground' ? 'Playground' : effectiveBottomTab === 'StateMachine' ? 'StateMachine' : effectiveBottomTab === 'Animate' ? 'Animate' : 'Design'
    setDoc(setWasmMode(editorRef.current, artboards[0].id, wasmMode))
    if (effectiveBottomTab !== 'Animate') setKeyframeSelection([])
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, effectiveBottomTab])


  // Keeps `WasmEditor`'s selection-for-motion-path-overlay in sync — see
  // `editor.rs`'s `set_selected_node` doc comment.
  useEffect(() => {
    if (!editorRef.current) return
    setSelectedNode(editorRef.current, selection?.nodeId ?? null)
  }, [selection])

  const handleFireTrigger = (name: string) => {
    if (!editorRef.current) return
    fireTrigger(editorRef.current, name)
  }

  const handleSetBoolInput = (name: string, value: boolean) => {
    if (!editorRef.current) return
    setStateMachineBoolInput(editorRef.current, name, value)
  }

  const handleSetNumberInput = (name: string, value: number) => {
    if (!editorRef.current) return
    setStateMachineNumberInput(editorRef.current, name, value)
  }

  const handleSetEnumInput = (name: string, value: string) => {
    if (!editorRef.current) return
    setStateMachineEnumInput(editorRef.current, name, value)
  }

  /** State Machine panel's authoring actions — all scoped to the first
   * artboard, same convention every Timeline handler above already uses. */
  const handleAddStateMachine = (name: string, initialAnimation: OpaqueId) => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    setDoc(addStateMachine(editorRef.current, artboard.id, name, initialAnimation))
  }

  const handleSelectStateMachine = (stateMachineId: OpaqueId) => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    setDoc(setActiveStateMachine(editorRef.current, artboard.id, stateMachineId))
  }

  const handleDeleteStateMachine = (stateMachineId: OpaqueId) => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    setDoc(deleteStateMachine(editorRef.current, artboard.id, stateMachineId))
  }

  const handleSetStateMachineInitialState = (value: string) => {
    const artboard = artboards[0]
    const stateMachineId = artboard?.activeStateMachine
    if (!editorRef.current || !artboard || !stateMachineId) return
    setDoc(setStateMachineInitialState(editorRef.current, artboard.id, stateMachineId, value))
  }

  const handleRenameStateMachine = (stateMachineId: OpaqueId, name: string) => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    setDoc(renameStateMachine(editorRef.current, artboard.id, stateMachineId, name))
  }

  const handleAddStateMachineInput = (name: string, kind: InputKind) => {
    const artboard = artboards[0]
    const stateMachineId = artboard?.activeStateMachine
    if (!editorRef.current || !artboard || !stateMachineId) return
    setDoc(addStateMachineInput(editorRef.current, artboard.id, stateMachineId, name, kind))
  }

  const handleAddState = (name: string, animation: OpaqueId, loopMode: LoopMode, layer: string | null) => {
    const artboard = artboards[0]
    const stateMachineId = artboard?.activeStateMachine
    if (!editorRef.current || !artboard || !stateMachineId) return
    setDoc(addState(editorRef.current, artboard.id, stateMachineId, name, animation, loopMode, layer))
  }

  // Playground's "Reset Layout" button (`StateMachineDock`'s own doc
  // comment on why it only exists there) — a plain grid, not any attempt at
  // a smart auto-layout (e.g. minimizing transition-line crossings): the
  // point is just "get back to *a* readable, non-overlapping layout",
  // nothing more.
  const handleResetStateMachineLayout = () => {
    const states = selectedStateMachineLayer ? selectedLayerDef?.states : activeStateMachine?.states
    if (!states) return
    const COLUMNS = 4
    const SPACING_X = 220
    const SPACING_Y = 140
    states.forEach((state, i) => {
      handleSetStatePosition(state.name, (i % COLUMNS) * SPACING_X, Math.floor(i / COLUMNS) * SPACING_Y, selectedStateMachineLayer)
    })
  }

  const handleAddTransition = (stateName: string, to: string, condition: ConditionDef, direction: PlaybackDirection, layer: string | null) => {
    const artboard = artboards[0]
    const stateMachineId = artboard?.activeStateMachine
    if (!editorRef.current || !artboard || !stateMachineId) return
    setDoc(addTransition(editorRef.current, artboard.id, stateMachineId, stateName, to, condition, direction, -1, true, layer))
  }

  const handleUpdateState = (stateName: string, animation: OpaqueId, loopMode: LoopMode, layer: string | null) => {
    const artboard = artboards[0]
    const stateMachineId = artboard?.activeStateMachine
    if (!editorRef.current || !artboard || !stateMachineId) return
    setDoc(updateState(editorRef.current, artboard.id, stateMachineId, stateName, animation, loopMode, layer))
  }

  const handleDeleteState = (stateName: string, layer: string | null) => {
    const artboard = artboards[0]
    const stateMachineId = artboard?.activeStateMachine
    if (!editorRef.current || !artboard || !stateMachineId) return
    setDoc(deleteState(editorRef.current, artboard.id, stateMachineId, stateName, layer))
  }

  const handleSetStatePosition = (stateName: string, x: number, y: number, layer: string | null) => {
    const artboard = artboards[0]
    const stateMachineId = artboard?.activeStateMachine
    if (!editorRef.current || !artboard || !stateMachineId) return
    setDoc(setStatePosition(editorRef.current, artboard.id, stateMachineId, stateName, x, y, layer))
  }

  const handleSetTransition = (
    stateName: string,
    index: number,
    condition: ConditionDef,
    direction: PlaybackDirection,
    durationSeconds: number | undefined,
    interruptible: boolean | undefined,
    layer: string | null,
  ) => {
    const artboard = artboards[0]
    const stateMachineId = artboard?.activeStateMachine
    if (!editorRef.current || !artboard || !stateMachineId) return
    setDoc(setTransition(editorRef.current, artboard.id, stateMachineId, stateName, index, condition, direction, durationSeconds, interruptible ?? null, layer))
  }

  const handleRemoveTransition = (stateName: string, index: number, layer: string | null) => {
    const artboard = artboards[0]
    const stateMachineId = artboard?.activeStateMachine
    if (!editorRef.current || !artboard || !stateMachineId) return
    setDoc(removeTransition(editorRef.current, artboard.id, stateMachineId, stateName, index, layer))
  }

  const handleSetStateBlend = (stateName: string, blend: Blend1DDef | null, layer: string | null) => {
    const artboard = artboards[0]
    const stateMachineId = artboard?.activeStateMachine
    if (!editorRef.current || !artboard || !stateMachineId) return
    setDoc(setStateBlend(editorRef.current, artboard.id, stateMachineId, stateName, blend, layer))
  }

  const handleSetStateProgram = (stateName: string, program: ProgramDef | null, layer: string | null) => {
    const artboard = artboards[0]
    const stateMachineId = artboard?.activeStateMachine
    if (!editorRef.current || !artboard || !stateMachineId) return
    try {
      setDoc(setStateProgram(editorRef.current, artboard.id, stateMachineId, stateName, program, layer))
      setStateMachineError(null)
    } catch (err) {
      // A malformed compiled Program (bad IR shape, an incomplete block)
      // used to throw straight out of this handler — uncaught here, React
      // routes it to the nearest error boundary, which until now was only
      // `RootErrorBoundary`, blanking the entire window over one bad
      // script. Catching it here keeps the previously-saved program intact
      // (this failed write never reaches `setDoc`) and surfaces the failure
      // next to the State Machine dock instead.
      console.error('setStateProgram failed — program not saved, previous state kept', err)
      setStateMachineError(err instanceof Error ? err.message : String(err))
    }
  }

  /** The State Machine tab's own opt-in toggle for running Programmable
   * States there — `false` by default (see `editor.rs`'s
   * `state_machine_programs_enabled` doc comment). Session state, not saved
   * document content, same as `mode`/`animateModeActive`. */
  const handleToggleProgramsEnabled = (enabled: boolean) => {
    setProgramsEnabled(enabled)
    if (editorRef.current) setStateMachineProgramsEnabled(editorRef.current, enabled)
  }

  const handleAddStateMachineLayer = (name: string, animation: OpaqueId) => {
    const artboard = artboards[0]
    const stateMachineId = artboard?.activeStateMachine
    if (!editorRef.current || !artboard || !stateMachineId) return
    setDoc(addStateMachineLayer(editorRef.current, artboard.id, stateMachineId, name, animation))
  }

  const handleDeleteStateMachineLayer = (layerName: string) => {
    const artboard = artboards[0]
    const stateMachineId = artboard?.activeStateMachine
    if (!editorRef.current || !artboard || !stateMachineId) return
    setDoc(deleteStateMachineLayer(editorRef.current, artboard.id, stateMachineId, layerName))
  }

  const handleSetStateMachineLayerMask = (layerName: string, mask: OpaqueId[]) => {
    const artboard = artboards[0]
    const stateMachineId = artboard?.activeStateMachine
    if (!editorRef.current || !artboard || !stateMachineId) return
    setDoc(setStateMachineLayerMask(editorRef.current, artboard.id, stateMachineId, layerName, mask))
  }

  const handleSetStateMachineLayerInitialState = (layerName: string, value: string) => {
    const artboard = artboards[0]
    const stateMachineId = artboard?.activeStateMachine
    if (!editorRef.current || !artboard || !stateMachineId) return
    setDoc(setStateMachineLayerInitialState(editorRef.current, artboard.id, stateMachineId, layerName, value))
  }

  const selectedNode = selection
    ? (artboards
        .find((a) => a.id.idx === selection.artboardId.idx && a.id.version === selection.artboardId.version)
        ?.nodes.find((n) => n.id.idx === selection.nodeId.idx && n.id.version === selection.nodeId.version) ?? null)
    : null

  /** Every other `Shape` node in the selected node's own artboard — the
   * Timeline's `MotionPath` guide picker only offers these, since a guide
   * needs a `path` to sample (`Path::point_at_progress`) and a node can't
   * usefully guide its own motion. */
  const motionPathGuideOptions = selection
    ? (artboards
        .find((a) => a.id.idx === selection.artboardId.idx && a.id.version === selection.artboardId.version)
        ?.nodes.filter((n) => n.kind === 'Shape' && !idsEqual(n.id, selection.nodeId)) ?? [])
    : []

  // Overrides `selectedNode`'s translation with the live drag position while
  // a canvas translate-drag is in progress, so `TransformHandles`' outline
  // tracks the node instead of sitting frozen at its pre-drag spot until the
  // drag's `commitNodeTranslation` call lands (see `dragTranslation`'s doc
  // comment above).
  // Design mode's 3D scene view: on whenever the scene has 3D nodes and the
  // canvas's 2D/3D toggle says 3D (see `useView3D`).
  const sceneHas3D = !!activeArtboard?.nodes.some((n) => n.node3d)
  const view3dActive = mode === 'Design' && sceneHas3D && sceneView === '3D'
  useView3D({
    canvasRef,
    editor: editorRef.current,
    active: view3dActive,
    artboardId: activeArtboard?.id ?? null,
    nodes: activeArtboard?.nodes ?? [],
    selectedId: selection?.nodeId ?? null,
    // Same as clicking the row in Layers: the Inspector *and* the Layers
    // highlight move together (setting only `selection` left Layers
    // highlighting the previous node).
    onSelect: (node) => {
      if (node && activeArtboard) selectSingle(activeArtboard.id, node)
      else {
        setSelection(null)
        setMultiSelectedIds([])
      }
    },
    onCommit: (node, value) => {
      if (editorRef.current && activeArtboard) setDoc(setNode3D(editorRef.current, activeArtboard.id, node, value))
    },
  })

  // Playground shows only the game: no selection widgets of any kind
  // (handles, path edit, tile painting, gradient / bone gizmos).
  const transformHandlesNode = view3dActive || mode === 'Playground'
    ? null
    : selectedNode && dragTranslation
      ? { ...selectedNode, transform: { ...selectedNode.transform, translation: dragTranslation } }
      : selectedNode

  // `pathEditNodeId` only actually applies while that exact node is still
  // the current selection — switching selection to something else (or
  // deselecting) silently falls back out of path-edit mode instead of
  // needing its own explicit "exit" action synced against every place
  // selection can change.
  const isPathEditing = mode !== 'Playground' && pathEditNodeId !== null && selection !== null && idsEqual(pathEditNodeId, selection.nodeId)

  /** A plain axis-aligned screen-space bounding box around every
   * multi-selected node — rendered instead of `TransformHandles`' full
   * rotate/scale handle set (see `handleCanvasMouseDown`'s doc comment: a
   * multi-selected drag already moves every member together, so the only
   * thing missing is visual feedback that more than one shape is selected).
   * Reuses the exact same local-corners-through-`Transform2D` math
   * `TransformHandles` uses per node, against `worldTransform` (not
   * `transform`) for the same reason that component now does — a node
   * nested under a moved/rotated/scaled group needs its transform composed
   * through that ancestor to land in the right place; unioning each
   * member's own (now correctly world-positioned) box is still a reasonable
   * result even when members span different groups. `null` outside
   * multi-selection (falls through to the single-node `TransformHandles`
   * instead). */
  const multiSelectionScreenBox = (() => {
    if (mode === 'Playground' || multiSelectedIds.length < 2 || !selection) return null
    const artboard = artboards.find((a) => a.id.idx === selection.artboardId.idx && a.id.version === selection.artboardId.version)
    if (!artboard) return null

    let minX = Infinity
    let minY = Infinity
    let maxX = -Infinity
    let maxY = -Infinity
    for (const id of multiSelectedIds) {
      const node = artboard.nodes.find((n) => n.id.idx === id.idx && n.id.version === id.version)
      if (!node) continue
      const { translation, rotation, scale } = node.worldTransform
      const cos = Math.cos(rotation)
      const sin = Math.sin(rotation)
      const corners = [
        { x: node.localMin.x, y: node.localMin.y },
        { x: node.localMax.x, y: node.localMin.y },
        { x: node.localMax.x, y: node.localMax.y },
        { x: node.localMin.x, y: node.localMax.y },
      ]
      for (const c of corners) {
        const sx = c.x * scale.x
        const sy = c.y * scale.y
        const worldX = translation.x + (sx * cos - sy * sin)
        const worldY = translation.y + (sx * sin + sy * cos)
        minX = Math.min(minX, worldX)
        maxX = Math.max(maxX, worldX)
        minY = Math.min(minY, worldY)
        maxY = Math.max(maxY, worldY)
      }
    }
    if (!isFinite(minX)) return null

    const topLeft = worldToScreen(minX, maxY, camera, canvasSize)
    const bottomRight = worldToScreen(maxX, minY, camera, canvasSize)
    return { left: topLeft.x, top: topLeft.y, width: bottomRight.x - topLeft.x, height: bottomRight.y - topLeft.y }
  })()

  // Every Camera node's frame — the scene's own width x height divided by
  // the camera's zoom, centered on its world position, axis-aligned (the
  // Playground view only follows a camera's position, not its rotation) —
  // so an author sees exactly what Playground will show before pressing
  // play. Hidden in Playground itself, where you're looking *through* it.
  const cameraFrames =
    mode === 'Playground' || !activeArtboard || view3dActive
      ? []
      : activeArtboard.nodes
          .filter((n) => n.camera !== null)
          .map((n) => {
            const reference = { width: activeArtboard.width / Math.max(0.01, n.camera!.zoom), height: activeArtboard.height / Math.max(0.01, n.camera!.zoom) }
            // A responsive scene's frame is what the "preview on" device shows.
            const view =
              activeArtboard.screen.mode === 'Responsive'
                ? responsiveView(reference, previewScreen ?? { width: activeArtboard.width, height: activeArtboard.height }, activeArtboard.screen)
                : reference
            const halfW = view.width / 2
            const halfH = view.height / 2
            const { x, y } = n.worldTransform.translation
            const topLeft = worldToScreen(x - halfW, y + halfH, camera, canvasSize)
            const bottomRight = worldToScreen(x + halfW, y - halfH, camera, canvasSize)
            // The clickable camera icon at its center — same world-space size
            // as `dto.rs`'s `CAMERA_GIZMO_HALF_EXTENT`, which is exactly what
            // `pick_node_at` hit-tests, so clicking/dragging this icon selects/
            // moves the camera through the normal canvas drag path.
            const iconTopLeft = worldToScreen(x - CAMERA_GIZMO_HALF_EXTENT, y + CAMERA_GIZMO_HALF_EXTENT, camera, canvasSize)
            const iconSize = CAMERA_GIZMO_HALF_EXTENT * 2 * camera.zoom
            return {
              key: `${n.id.idx}:${n.id.version}`,
              name: n.name,
              iconLeft: iconTopLeft.x,
              iconTop: iconTopLeft.y,
              iconSize,
              isMain: activeArtboard.activeCamera !== null && idsEqual(activeArtboard.activeCamera, n.id),
              left: topLeft.x,
              top: topLeft.y,
              width: bottomRight.x - topLeft.x,
              height: bottomRight.y - topLeft.y,
            }
          })

  // Unity-style light gizmos: drawn for every Light 2D outside Playground
  // (brighter when selected) — Spot: outer/inner radius rings (or the cone's
  // arc + edges when narrower than 360°), Freeform: its polygon, Sprite: its
  // rectangle; plus a small clickable bulb at the light's center (the same
  // `SMALL_GIZMO_HALF_EXTENT` area `pick_node_at` hit-tests). Trails get the
  // bulb-sized grab marker only.
  // Light / trail / collider gizmos only for the selection: the selected
  // node itself, or anything under it (select a character to see its whole
  // rig's colliders). A scene full of torches would otherwise be buried in
  // overlapping radius circles.
  const gizmoNodeIds = (() => {
    const shown = new Set<string>()
    if (!selection || !activeArtboard) return shown
    const key = (id: OpaqueId) => `${id.idx}:${id.version}`
    const parentOf = new Map(activeArtboard.nodes.map((n) => [key(n.id), n.parent ? key(n.parent) : null]))
    const selected = key(selection.nodeId)
    for (const n of activeArtboard.nodes) {
      let cursor: string | null = key(n.id)
      for (let depth = 0; cursor && depth < 64; depth++) {
        if (cursor === selected) {
          shown.add(key(n.id))
          break
        }
        cursor = parentOf.get(cursor) ?? null
      }
    }
    return shown
  })()
  const gizmoShown = (id: OpaqueId) => gizmoNodeIds.has(`${id.idx}:${id.version}`)

  const lightGizmos =
    mode === 'Playground' || !activeArtboard || view3dActive
      ? []
      : activeArtboard.nodes
          .filter((n) => (n.light2d !== null || n.trail !== null) && gizmoShown(n.id))
          .map((n) => {
            const t = n.worldTransform
            const cos = Math.cos(t.rotation)
            const sin = Math.sin(t.rotation)
            // Node-local -> screen, same composition as `Transform2D::to_affine2`.
            const toScreen = (lx: number, ly: number) => {
              const px = (lx - t.anchor.x) * t.scale.x
              const py = (ly - t.anchor.y) * t.scale.y
              return worldToScreen(t.translation.x + px * cos - py * sin, t.translation.y + px * sin + py * cos, camera, canvasSize)
            }
            const polyline = (pts: { x: number; y: number }[], close: boolean) =>
              pts.map((p, i) => `${i === 0 ? 'M' : 'L'}${toScreen(p.x, p.y).x.toFixed(1)},${toScreen(p.x, p.y).y.toFixed(1)}`).join(' ') + (close ? ' Z' : '')
            const circle = (r: number, fromDeg: number, toDeg: number) => {
              const pts = []
              const steps = 48
              for (let i = 0; i <= steps; i++) {
                // Angle measured from local +Y (the spot's aim), clockwise-agnostic.
                const a = ((fromDeg + ((toDeg - fromDeg) * i) / steps) * Math.PI) / 180
                pts.push({ x: -Math.sin(a) * r, y: Math.cos(a) * r })
              }
              return pts
            }
            const paths: { d: string; dashed: boolean }[] = []
            const light = n.light2d
            if (light && light.lightType === 'Spot') {
              const outer = light.outerSpotAngle
              if (outer >= 359.9) {
                paths.push({ d: polyline(circle(light.outerRadius, 0, 360), true), dashed: false })
                if (light.innerRadius > 0) paths.push({ d: polyline(circle(light.innerRadius, 0, 360), true), dashed: true })
              } else {
                const half = outer / 2
                const arc = circle(light.outerRadius, -half, half)
                paths.push({ d: polyline([{ x: 0, y: 0 }, ...arc, { x: 0, y: 0 }], false), dashed: false })
                if (light.innerSpotAngle > 0) {
                  const innerHalf = light.innerSpotAngle / 2
                  paths.push({ d: polyline([{ x: 0, y: 0 }, ...circle(light.outerRadius, -innerHalf, innerHalf), { x: 0, y: 0 }], false), dashed: true })
                }
              }
            } else if (light && light.lightType === 'Freeform' && light.shapePoints.length >= 3) {
              paths.push({ d: polyline(light.shapePoints, true), dashed: false })
            } else if (light && light.lightType === 'Sprite') {
              const w = light.spriteWidth / 2
              const h = light.spriteHeight / 2
              paths.push({ d: polyline([{ x: -w, y: -h }, { x: w, y: -h }, { x: w, y: h }, { x: -w, y: h }], true), dashed: false })
            }
            const center = toScreen(0, 0)
            const iconSize = Math.max(14, LIGHT_GIZMO_HALF_EXTENT * 2 * camera.zoom)
            const selected = selection !== null && idsEqual(selection.nodeId, n.id)
            const c = light ? light.color : [1, 1, 1, 1]
            const tint = `rgb(${Math.round(Math.min(1, c[0]) * 255)}, ${Math.round(Math.min(1, c[1]) * 255)}, ${Math.round(Math.min(1, c[2]) * 255)})`
            return { key: `${n.id.idx}:${n.id.version}`, kind: light ? 'Light2D' : 'Trail', paths, center, iconSize, selected, tint }
          })

  // Unity's green Collider 2D outlines (Design only): every enabled Box /
  // Circle / Capsule / Polygon Collider 2D, drawn exactly where
  // `an1mbyt3-physics` places it — node-local offset/size through the
  // node's world transform (`Transform2D::to_affine2`). Triggers dashed.
  const colliderGizmos =
    mode === 'Playground' || !activeArtboard || view3dActive
      ? []
      : activeArtboard.nodes.flatMap((n) => {
          if (!gizmoShown(n.id)) return []
          const colliders = n.behaviors.filter((b) => b.enabled).map((b) => b.behavior)
          if (!colliders.some((b) => b.kind.endsWith('Collider2D'))) return []
          const t = n.worldTransform
          const cos = Math.cos(t.rotation)
          const sin = Math.sin(t.rotation)
          const sx = Math.abs(t.scale.x) || 1
          const sy = Math.abs(t.scale.y) || 1
          const toScreen = (lx: number, ly: number) => {
            const px = (lx - t.anchor.x) * t.scale.x
            const py = (ly - t.anchor.y) * t.scale.y
            return worldToScreen(t.translation.x + px * cos - py * sin, t.translation.y + px * sin + py * cos, camera, canvasSize)
          }
          const d = (pts: [number, number][]) =>
            pts.map(([x, y], i) => {
              const p = toScreen(x, y)
              return `${i === 0 ? 'M' : 'L'}${p.x.toFixed(1)},${p.y.toFixed(1)}`
            }).join(' ') + ' Z'
          // Arc of `radius` *world* units around local (cx, cy) — divided
          // back by scale so a circle stays round under non-uniform scale
          // (Rapier's ball uses the largest scale axis).
          const arc = (cx: number, cy: number, radius: number, from: number, to: number): [number, number][] => {
            const out: [number, number][] = []
            for (let i = 0; i <= 24; i++) {
              const a = from + ((to - from) * i) / 24
              out.push([cx + (Math.cos(a) * radius) / sx, cy + (Math.sin(a) * radius) / sy])
            }
            return out
          }
          const selected = selection !== null && idsEqual(selection.nodeId, n.id)
          const result: { key: string; d: string; trigger: boolean; selected: boolean }[] = []
          colliders.forEach((c, i) => {
            const key = `${n.id.idx}:${n.id.version}:${i}`
            if (c.kind === 'BoxCollider2D') {
              const [ox, oy] = c.offset
              const [w, h] = [c.size[0] / 2, c.size[1] / 2]
              result.push({ key, d: d([[ox - w, oy - h], [ox + w, oy - h], [ox + w, oy + h], [ox - w, oy + h]]), trigger: c.isTrigger, selected })
            } else if (c.kind === 'CircleCollider2D') {
              const r = c.radius * Math.max(sx, sy)
              result.push({ key, d: d(arc(c.offset[0], c.offset[1], r, 0, Math.PI * 2)), trigger: c.isTrigger, selected })
            } else if (c.kind === 'CapsuleCollider2D') {
              const [ox, oy] = c.offset
              const [w, h] = [c.size[0] * sx, c.size[1] * sy]
              const pts: [number, number][] =
                c.direction === 'Vertical'
                  ? (() => {
                      const r = w / 2
                      const half = Math.max(0, h / 2 - r) / sy
                      return [...arc(ox, oy + half, r, 0, Math.PI), ...arc(ox, oy - half, r, Math.PI, Math.PI * 2)]
                    })()
                  : (() => {
                      const r = h / 2
                      const half = Math.max(0, w / 2 - r) / sx
                      return [...arc(ox + half, oy, r, -Math.PI / 2, Math.PI / 2), ...arc(ox - half, oy, r, Math.PI / 2, Math.PI * 1.5)]
                    })()
              result.push({ key, d: d(pts), trigger: c.isTrigger, selected })
            } else if (c.kind === 'PolygonCollider2D' && c.points.length >= 3) {
              result.push({ key, d: d(c.points.map(([x, y]) => [x + c.offset[0], y + c.offset[1]] as [number, number])), trigger: c.isTrigger, selected })
            }
          })
          return result
        })

  const handleSetTranslation = (x: number, y: number) => {
    if (!selection || !editorRef.current) return
    setDoc(setNodeTranslation(editorRef.current, selection.artboardId, selection.nodeId, x, y))
  }

  const handleSetRotation = (degrees: number) => {
    if (!selection || !editorRef.current) return
    setDoc(setNodeRotationDegrees(editorRef.current, selection.artboardId, selection.nodeId, degrees))
  }

  const handleSetScale = (x: number, y: number) => {
    if (!selection || !editorRef.current) return
    setDoc(setNodeScale(editorRef.current, selection.artboardId, selection.nodeId, x, y))
  }

  const handleSetAnchor = (x: number, y: number) => {
    if (!selection || !editorRef.current) return
    setDoc(setNodeAnchor(editorRef.current, selection.artboardId, selection.nodeId, x, y))
  }

  // The on-canvas transform tool's rotate/scale handles — `TransformHandles`
  // owns the drag math (angle tracking, anchor-corner scale derivation) and
  // just calls these, same "component computes, App talks to WasmEditor"
  // split every other canvas interaction in this file already follows.
  const handlePreviewRotation = (degrees: number) => {
    if (!selection || !editorRef.current) return
    previewNodeRotation(editorRef.current, selection.artboardId, selection.nodeId, degrees)
  }

  // ---- Bone gizmo drags (FK aim / IK tip) — see `BoneGizmos` ----
  // The canvas updates live from the engine; the React document is
  // refreshed at most once per animation frame so the gizmos track too.
  const bonePreviewFrame = useRef<number | null>(null)
  const refreshDocSoon = () => {
    if (bonePreviewFrame.current !== null) return
    bonePreviewFrame.current = requestAnimationFrame(() => {
      bonePreviewFrame.current = null
      if (editorRef.current) setDoc(getDocumentSummary(editorRef.current))
    })
  }
  const handleBoneAim = (bone: OpaqueId, x: number, y: number) => {
    if (!editorRef.current || !activeArtboard) return
    previewBoneAim(editorRef.current, activeArtboard.id, bone, x, y)
    refreshDocSoon()
  }
  const handleBoneIk = (tip: OpaqueId, chainLength: number, x: number, y: number): OpaqueId[] => {
    if (!editorRef.current || !activeArtboard) return []
    const chain = previewBoneIk(editorRef.current, activeArtboard.id, tip, chainLength, x, y)
    refreshDocSoon()
    return chain
  }
  const handleBoneCommit = (bones: OpaqueId[], original: number[]) => {
    if (!editorRef.current || !activeArtboard) return
    setDoc(commitBonePose(editorRef.current, activeArtboard.id, bones, original))
  }

  // ---- Rig tools (bone Inspector, Sprite Skin) ----
  const handleMakeRagdoll = (animated: boolean) => {
    if (!editorRef.current || !selection) return
    setDoc(makeRagdoll(editorRef.current, selection.artboardId, selection.nodeId, 10, 70, animated))
  }
  const handleAddIkTarget = (chainLength: number) => {
    if (!editorRef.current || !selection) return
    const summary = addIkTarget(editorRef.current, selection.artboardId, selection.nodeId, chainLength)
    setDoc(summary)
    // Select the new target so it can be dragged right away.
    const scene = summary.artboards.find((a) => idsEqual(a.id, selection.artboardId))
    const target = scene?.nodes[scene.nodes.length - 1]
    if (target) setSelection({ artboardId: selection.artboardId, nodeId: target.id })
  }
  const handleBindSkin = (rootBone: OpaqueId) => {
    if (!editorRef.current || !selection) return
    try {
      setDoc(bindSpriteSkin(editorRef.current, selection.artboardId, selection.nodeId, rootBone, 8, 8))
    } catch (err) {
      window.alert(String(err))
    }
  }

  const handleCommitRotation = (originDegrees: number, degrees: number, compensateChildren: boolean) => {
    if (!selection || !editorRef.current) return
    setDoc(commitNodeRotation(editorRef.current, selection.artboardId, selection.nodeId, originDegrees, degrees, compensateChildren))
  }

  const handlePreviewScale = (x: number, y: number, scaleX: number, scaleY: number) => {
    if (!selection || !editorRef.current) return
    previewNodeScale(editorRef.current, selection.artboardId, selection.nodeId, x, y, scaleX, scaleY)
  }

  const handleCommitScale = (
    originX: number,
    originY: number,
    originScaleX: number,
    originScaleY: number,
    x: number,
    y: number,
    scaleX: number,
    scaleY: number,
    compensateChildren: boolean,
  ) => {
    if (!selection || !editorRef.current) return
    setDoc(commitNodeScale(editorRef.current, selection.artboardId, selection.nodeId, originX, originY, originScaleX, originScaleY, x, y, scaleX, scaleY, compensateChildren))
  }

  const handlePreviewAnchor = (anchorX: number, anchorY: number, x: number, y: number) => {
    if (!selection || !editorRef.current) return
    previewNodeAnchor(editorRef.current, selection.artboardId, selection.nodeId, anchorX, anchorY, x, y)
  }

  const handleCommitAnchor = (
    originAnchorX: number,
    originAnchorY: number,
    originX: number,
    originY: number,
    anchorX: number,
    anchorY: number,
    x: number,
    y: number,
  ) => {
    if (!selection || !editorRef.current) return
    setDoc(
      commitNodeAnchor(editorRef.current, selection.artboardId, selection.nodeId, originAnchorX, originAnchorY, originX, originY, anchorX, anchorY, x, y),
    )
  }

  /** Toolbar "Add Shape" — creates a new default rectangle on the first
   * artboard and selects it immediately, matching how a design tool
   * typically selects whatever it just created. No-op if there's no
   * artboard yet (shouldn't happen once `WasmEditor::create` has resolved —
   * every document has at least one artboard — but `editorRef`/`artboards`
   * can both still be momentarily unready right after mount). */
  const handleAddShape = (kind: ShapePreset) => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    const result = addShapeNode(editorRef.current, artboard.id, kind)
    setDoc(result.document)
    setSelection({ artboardId: artboard.id, nodeId: result.newNode })
  }

  const handleAddParticleSystem = () => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    const result = addParticleSystemNode(editorRef.current, artboard.id)
    setDoc(result.document)
    setSelection({ artboardId: artboard.id, nodeId: result.newNode })
  }

  /** `PaintAssetEditor`'s "Save as Resource": saves the painting as a
   * Drawing resource (`an1mbyt3_core::drawing`) — the whole drawing, every
   * stroke and color, kept as scratch-paint's own SVG (vector mode) or a
   * PNG (bitmap mode). It shows up in Resources ▸ Drawings, from where it
   * can be placed (drag / double-click "Place in Scene") or reopened here.
   * Saving a drawing that was opened from Resources updates it in place.
   *
   * (It used to turn the drawing into a Component, which holds exactly one
   * path and one color — so anything with a second stroke, i.e. nearly
   * every drawing, silently saved nothing.)
   *
   * The SVG's size is still guarded before it reaches wasm: a pathologically
   * large string once crashed wasm-bindgen's string marshaling with a hard
   * `memory access out of bounds` trap, not a catchable error. */
  const handleSaveDrawingAsResource = () => {
    const editor = editorRef.current
    if (!editor) return
    const name = paintDrawingName.trim() || 'Drawing'
    const session = withLive(paintSession, paintImageRef.current)
    ;(async () => {
      const MAX_SVG_BYTES = 4 * 1024 * 1024
      for (const layer of session.layers) {
        for (const cel of layer.cels) {
          if (cel?.kind === 'svg' && (!cel.svg.trimStart().startsWith('<') || cel.svg.length > MAX_SVG_BYTES)) throw new Error("A frame can't be saved (not a plain SVG, or too large).")
        }
      }
      const { json, saved: savedSession } = await sessionToAnimation(editor, session, name)
      let next: DocumentSummary
      let saved = editingDrawing
      if (saved) next = updateAnimatedDrawing(editor, saved, json)
      else {
        const result = addAnimatedDrawing(editor, name, json)
        next = result.document
        saved = result.drawing
      }
      if (saved && next.drawings.find((d) => idsEqual(d.id, saved))?.name !== name) next = renameDrawing(editor, saved, name)
      setEditingDrawing(saved)
      setPaintSession(savedSession)
      paintImageRef.current = savedSession.layers[savedSession.currentLayer]?.cels[savedSession.currentFrame] ?? null
      setDoc(next)
      setNotice(`Saved "${name}" to Resources ▸ Drawings${session.frameCount > 1 ? ` (${session.frameCount} frames)` : ''}`)
      setBottomDockTab('Resources')
      setBottomDockOpen(true)
      setMode('Design')
    })().catch((err) => {
      console.error('Save as Resource failed', err)
      setNotice(`Couldn't save the drawing: ${err instanceof Error ? err.message : String(err)}`)
    })
  }

  /** Paint toolbar "New": a blank canvas that saves as a new drawing. */
  const handleNewDrawing = () => {
    setEditingDrawing(null)
    paintImageRef.current = null
    setPaintDrawingName('New Drawing')
    setPaintSession(blankSession())
    setPaintSessionKey(`blank-${Date.now()}`)
    setMode('Paint')
  }

  /** Resources ▸ Drawings "Edit in Paint" (double-click): reopens a drawing
   * — every frame and layer — in Paint; saving then updates it. */
  const handleEditDrawing = (drawingId: OpaqueId) => {
    const editor = editorRef.current
    const drawing = doc?.drawings.find((d) => idsEqual(d.id, drawingId))
    if (!editor || !drawing) return
    try {
      setPaintSession(sessionFromDrawing(editor, drawingId))
    } catch (err) {
      setNotice(`Couldn't open the drawing: ${err instanceof Error ? err.message : String(err)}`)
      return
    }
    paintImageRef.current = null
    setEditingDrawing(drawingId)
    setPaintDrawingName(drawing.name)
    setPaintSessionKey(`drawing-${drawingId.idx}-${Date.now()}`)
    setMode('Paint')
  }

  /** Shapes menu's "Import SVG" action — opens a native file picker, reads
   * the chosen file as text, and hands it to `WasmEditor::import_svg`.
   * Selects the first newly created shape, same "select whatever you just
   * created" convention as `handleAddShape`/`handleAddBone`. Errors (an
   * unreadable file, or an SVG with nothing importable in it) are only
   * logged, not surfaced in the UI yet — same convention `handleOpen`
   * already established for `.tea`/`.teab` load failures. */
  const handleImportSvg = () => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    const editor = editorRef.current
    ;(async () => {
      const path = await pickOpenSvgPath()
      if (!path) return
      const xml = await readTextFile(path)
      const result = importSvg(editor, artboard.id, xml)
      setDoc(result.document)
      if (result.newNodes[0]) setSelection({ artboardId: artboard.id, nodeId: result.newNodes[0] })
    })().catch((err) => console.error('SVG import failed', err))
  }

  /** Toolbar "Import Image…" — same shape as `handleImportSvg`, but for
   * PNG/JPEG/GIF (binary, hence `readBinaryFile` instead of `readTextFile`).
   * A still image selects its new node immediately, same convention every
   * other import/add action here uses; a GIF creates no node at all
   * (frames-only this pass — see `editor.rs`'s `import_image` doc comment),
   * so there's nothing to select yet until the Resources panel exists to
   * reveal the new group. */
  const handleImportImage = () => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    ;(async () => {
      const path = await pickOpenImagePath()
      if (!path) return
      const bytes = await readBinaryFile(path)
      const filename = path.split(/[\\/]/).pop() ?? path
      // With a project open the file is copied into Assets/Images first and
      // the asset linked to that copy (see `useProject`'s `importImage`).
      const result = await project.importImage(filename, bytes, artboard.id)
      setDoc(result.document)
      if (result.newNode) setSelection({ artboardId: artboard.id, nodeId: result.newNode })
    })().catch((err) => console.error('Image import failed', err))
  }

  /** Import → Audio: a sound file into the document (and the project's
   * `Assets/Sounds/` when one is open). */
  const handleImportAudio = () => {
    ;(async () => {
      const path = await pickOpenAudioPath()
      if (!path) return
      const bytes = await readBinaryFile(path)
      const result = await project.importAudio(path.split(/[\\/]/).pop() ?? path, bytes)
      setDoc(result.document)
    })().catch((err) => {
      console.error('Audio import failed', err)
      window.alert(`Couldn't import that sound: ${err}`)
    })
  }

  const handleDeleteAudio = (id: OpaqueId) => {
    if (!editorRef.current) return
    audio.stopAll()
    audio.invalidate()
    setDoc(deleteAudio(editorRef.current, id))
  }

  /** Import → 3D Model: a glTF / GLB file, plus every texture / buffer it
   * references (read from beside it), as a new Node3D at the origin. */
  const handleImportModel = () => {
    // Into the scene you're looking at (not just the first one).
    const artboard = activeArtboard
    const editor = editorRef.current
    if (!editor || !artboard) return
    ;(async () => {
      const path = await pickOpenModelPath()
      if (!path) return
      const source = await readBinaryFile(path)
      const dir = path.replace(/[\\/][^\\/]*$/, '')
      const files: { path: string; data: Uint8Array }[] = []
      for (const uri of gltfExternalUris(source)) {
        try {
          files.push({ path: uri, data: await readBinaryFile(`${dir}/${uri}`) })
        } catch (err) {
          console.warn(`3D model: couldn't read ${uri} next to the model`, err)
        }
      }
      const name = path.split(/[\\/]/).pop() ?? path
      const result = importModel(editor, artboard.id, name, source, files)
      setDoc(result.document)
      setSelection({ artboardId: artboard.id, nodeId: result.newNode })
    })().catch((err) => {
      console.error('3D model import failed', err)
      window.alert(`Couldn't import that model: ${err}`)
    })
  }

  /** Shapes menu "Add Tilemap…" — picks a tileset image file and builds a
   * new, empty `Tilemap` node from it (same import/select-what-you-just-
   * created shape as `handleImportImage`). Asks for the tileset's own tile
   * size up front (not every tileset uses the same tile size — forcing a
   * blanket 16x16 assumption on all of them was wrong); map size/cell size
   * default to a reasonable starting grid and are freely editable afterward
   * from the Inspector's own Tilemap section (`resizeTilemap`/`setTilemapCellSize`),
   * so they don't need their own dialog fields here. */
  const handleAddTilemap = () => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    const editor = editorRef.current
    ;(async () => {
      const path = await pickOpenImagePath()
      if (!path) return
      const tileWidthInput = window.prompt('Tile width (pixels)?', '16')
      if (tileWidthInput === null) return
      const tileHeightInput = window.prompt('Tile height (pixels)?', tileWidthInput)
      if (tileHeightInput === null) return
      const tileWidthPx = Math.max(1, Math.round(Number(tileWidthInput)) || 16)
      const tileHeightPx = Math.max(1, Math.round(Number(tileHeightInput)) || 16)
      const bytes = await readBinaryFile(path)
      const filename = path.split(/[\\/]/).pop() ?? path
      // `demoLoadingRef`'s own doc comment — `addTilemapNode` holds `&mut
      // self` across its own `bake_and_cache_image` await the same way
      // `load_ninja_demo` does, so the render loop's own `editor.render(...)`
      // call needs to stand down for the same reentrancy reason while this
      // is in flight.
      demoLoadingRef.current = true
      try {
        const result = await addTilemapNode(editor, artboard.id, null, filename, bytes, tileWidthPx, tileHeightPx, 20, 15, 32)
        setDoc(result.document)
        if (result.newNode) setSelection({ artboardId: artboard.id, nodeId: result.newNode })
      } finally {
        demoLoadingRef.current = false
      }
    })().catch((err) => console.error('Tilemap creation failed', err))
  }

  /** Toolbar "Load Demo Scene" — imports `DEMO_SVG` (a rocket with nested
   * groups, a real clip-path-derived mask, and a few loose stars) through
   * the exact same `import_svg` call `handleImportSvg` uses, no file picker
   * needed, then builds a small real "Liftoff" hover/sway animation on the
   * `Rocket` group via `addDemoHoverAnimation` (one atomic wasm call — see
   * that function's own doc comment for why this isn't built one keyframe
   * at a time from here) so there's something actually *animated* to point
   * at, not just a static imported scene. Switches to Animate mode and
   * queues one-shot auto-play (`autoPlayAnimation`) so the hover is visible
   * immediately, no manual navigation needed.
   *
   * Failure to find the freshly-imported `Rocket` node (or, in principle,
   * to build the animation on it) is logged and otherwise swallowed rather
   * than left to throw mid-function — `setDoc`/the selection below must
   * still run either way, so the imported scene itself always shows up
   * correctly even if the animation step doesn't.
   *
   * Selects every *root-level* thing the import just created (the `Rocket`
   * group plus the four stars — `result.document`'s fresh `NodeSummary`s,
   * filtered to `parent === null`) as the initial multi-selection, so the
   * group/mask tree in the Layers panel and the multi-select bounding box on
   * canvas are both immediately visible without the user needing to
   * Ctrl-click anything first. */
  const handleLoadDemo = () => {
    const artboard = artboards[0]
    const editor = editorRef.current
    if (!editor || !artboard) return

    const result = importSvg(editor, artboard.id, DEMO_SVG)
    const freshArtboard = result.document.artboards.find((a) => idsEqual(a.id, artboard.id))
    const rocket = freshArtboard?.nodes.find((n) => n.name === 'Rocket' && n.parent === null)
    const flame = freshArtboard?.nodes.find((n) => n.name === 'Flame')
    if (!rocket) console.error('Load Demo Scene: could not find the imported "Rocket" node — no animation was built.')

    let finalDoc = result.document
    if (rocket) {
      try {
        finalDoc = addDemoHoverAnimation(editor, artboard.id, rocket.id, flame?.id ?? null).document
        setBottomDockTab('Animate')
        setBottomDockOpen(true)
        setAutoPlayAnimation(true)
      } catch (err) {
        // Never let a demo-animation failure block the import itself from
        // showing up — `finalDoc`/selection below still apply even if this
        // throws, same "don't let a nice-to-have break the core action"
        // reasoning as `handleImportSvg`'s own catch.
        console.error('Load Demo Scene: failed to build the hover animation', err)
      }
    }
    setDoc(finalDoc)

    const rootLevelNewIds = result.newNodes.filter((id) => freshArtboard?.nodes.some((n) => idsEqual(n.id, id) && n.parent === null))
    if (rootLevelNewIds.length > 0) {
      setMultiSelectedIds(rootLevelNewIds)
      // `rocket` specifically, not just whichever id happens to be last in
      // `rootLevelNewIds` (the stars sort after it) — the Timeline's
      // per-node keyframe view is scoped to whatever `selection` currently
      // is, so picking a star here would leave the Liftoff animation's own
      // tracks (all on `rocket`) invisible even in Animate mode.
      setSelection({ artboardId: artboard.id, nodeId: rocket?.id ?? rootLevelNewIds[rootLevelNewIds.length - 1] })
    }
  }

  /** Shapes menu's "Load Geometry Dash Demo" — builds a scrolling auto-
   * runner level as a real Programmable State (see `editor.rs`'s
   * `build_demo_geometry_dash` doc comment) and switches to Playground so
   * it's immediately playable (space to jump). */
  const handleLoadGeometryDashDemo = () => {
    const artboard = artboards[0]
    const editor = editorRef.current
    if (!editor || !artboard) return
    setDoc(loadGeometryDashDemo(editor, artboard.id))
    setMode('Playground')
  }

  /** Ninja Adventure demo — adds its own two scenes (Village + House),
   * switches to the Village and starts Playground. */
  const handleLoadNinjaDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadNinjaDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  /** Physics Playground demo — its own scene, straight into Playground. */
  const handleLoadPhysicsDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    const result = loadPhysicsDemo(editor)
    switchToScene(result.newArtboard, result.document)
    setMode('Playground')
  }

  /** Dungeon Royale demo — its own scene, baked, straight into Playground. */
  const handleLoadTacticsDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadTacticsDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  const handleLoadLiquidDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadLiquidDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  const handleLoadTortureDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadTortureDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  const handleLoad3DDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void load3DDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  const handleLoadShatterDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadShatterDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  const handleLoadVoxelDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadVoxelDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  /** The Console's "where" link: go to the state whose code it is. */
  const handleOpenCodeAt = (origin: ConsoleOrigin) => {
    if (!origin.state) return
    setCodeRequest({ kind: 'state', scene: origin.scene ?? '', layer: origin.layer ?? null, state: origin.state, line: origin.line, column: origin.column, nonce: Date.now() })
    setMode('Code')
  }
  /** Opens the Code tab at a file (Inspector "Edit file" buttons). */
  const handleOpenFile = (path: string) => {
    setCodeRequest({ kind: 'file', path, nonce: Date.now() })
    setMode('Code')
  }
  const handleOpenFileRef = useRef(handleOpenFile)
  handleOpenFileRef.current = handleOpenFile
  // `requestOpenFile` (game/projectFiles) from panels that aren't wired to
  // App directly (the state's code editor).
  useEffect(() => {
    const onOpen = (e: Event) => handleOpenFileRef.current(String((e as CustomEvent).detail))
    window.addEventListener('an1:open-file', onOpen)
    return () => window.removeEventListener('an1:open-file', onOpen)
  }, [])

  const handleLoadBoomDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadBoomDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  const handleLoadLanDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadLanDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  const handleLoadClothDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadClothDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  const handleLoadGooShapesDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadGooShapesDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  const handleLoadGooDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadGooDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  const handleLoadLeerDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadLeerDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  const handleLoadTreesDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadTreesDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  const handleLoadPhysics3DDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadPhysics3DDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  const handleLoadHtmlDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadHtmlDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  const handleLoadResponsiveDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadResponsiveDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  const handleLoadUiDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadUiDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  const handleLoadMatch3Demo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadMatch3Demo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  const handleLoadDungeonDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadDungeonDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  /** Ragdoll Arena demo — its own scene, baked, straight into Playground. */
  const handleLoadRigDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadRigDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  /** Stress Test demo — scalable profiling scene, straight into Playground. */
  const handleLoadStressDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadStressDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  /** Raycaster FPS demo — like the Ninja demo it adds its own scene, then
   * switches to it and starts Playground. */
  const handleLoadFpsDemo = () => {
    const editor = editorRef.current
    if (!editor) return
    demoLoadingRef.current = true
    void loadFpsDemo(editor)
      .then((result) => {
        switchToScene(result.newArtboard, result.document)
        setMode('Playground')
      })
      .finally(() => {
        demoLoadingRef.current = false
      })
  }

  /** + menu → 2D Object → Text: added at the scene root and selected, so the
   * Inspector's text field is right there to type into. `icon`: + menu →
   * Icon — the same Text node drawn with Material Symbols (its text is the
   * icon's name, e.g. `star`), so the Inspector shows the icon grid. */
  const handleAddText = (icon = false) => {
    const artboard = artboards[0]
    const editor = editorRef.current
    if (!editor || !artboard) return
    const result = addTextNode(editor, artboard.id, null)
    setDoc(result.document)
    setSelection({ artboardId: artboard.id, nodeId: result.newNode })
    if (!icon) return
    setDoc(setTextContent(editor, artboard.id, result.newNode, 'star'))
    setDoc(setTextSize(editor, artboard.id, result.newNode, 96))
    addGoogleFont(editor, iconFamily('Rounded'))
      .then((font) => setDoc(setTextFont(editor, artboard.id, result.newNode, font)))
      .catch((err) => gameConsole.add('error', `Couldn't download the icon font: ${err instanceof Error ? err.message : String(err)}`, { kind: 'Engine' }))
  }

  /** Inspector → Text → Font: downloads the Google Font into the document
   * (once) and switches the selected Text node to it. */
  const handleSetTextFontFamily = async (family: string) => {
    const editor = editorRef.current
    const target = selection
    if (!editor || !target) return
    try {
      const font = await addGoogleFont(editor, family)
      setDoc(setTextFont(editor, target.artboardId, target.nodeId, font))
    } catch (err) {
      gameConsole.add('error', `Couldn't use the font ${family}: ${err instanceof Error ? err.message : String(err)}`, { kind: 'Engine' })
    }
  }

  /** Toolbar "Add Bone" — chains onto the currently selected bone (becomes
   * its child, at its tip) if one's selected, otherwise starts a new root
   * bone, matching how a designer actually builds a chain: select the last
   * bone you placed, hit "Add Bone" again. */
  const handleAddBone = () => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    const selectedIsBone = selection && selectedNode?.kind === 'Bone' ? selection.nodeId : null
    const result = addBoneNode(editorRef.current, artboard.id, selectedIsBone, 100)
    setDoc(result.document)
    setSelection({ artboardId: artboard.id, nodeId: result.newNode })
  }

  /** Shapes menu's "Add Camera" — a root-level Camera node (no chaining
   * convention like Bone's, since cameras aren't nested into a kinematic
   * chain the way bones are). */
  const handleAddCamera = () => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    const result = addCameraNode(editorRef.current, artboard.id, null)
    // A scene's first camera becomes its Main Camera automatically — what
    // Unity's own default scene does, and what an author adding "a camera"
    // to a scene that has none almost always means.
    const hasMain = artboard.activeCamera !== null && artboard.nodes.some((n) => n.camera !== null && idsEqual(n.id, artboard.activeCamera!))
    setDoc(hasMain ? result.document : setActiveCamera(editorRef.current, artboard.id, result.newNode))
    setSelection({ artboardId: artboard.id, nodeId: result.newNode })
  }

  /** + menu's 3D items — a Node3D at the scene root, selected. A scene's
   * first mesh also gets a Camera 3D + Sun if it has none, so it's visible
   * straight away. */
  const handleAddNode3D = (item: Node3DItem) => {
    const artboard = artboards[0]
    const editor = editorRef.current
    if (!editor || !artboard) return
    const has = (type: string) => artboard.nodes.some((n) => n.node3d?.content.type === type)
    let result = addNode3D(editor, artboard.id, null, item)
    const selected = result.newNode
    if (item !== 'Camera' && !has('Camera')) result = addNode3D(editor, artboard.id, null, 'Camera')
    if (item !== 'Sun' && item !== 'PointLight' && !has('Light')) result = addNode3D(editor, artboard.id, null, 'Sun')
    setDoc(result.document)
    setSelection({ artboardId: artboard.id, nodeId: selected })
  }

  /** + menu's UI items: added under the scene's camera (the HUD) and selected. */
  const handleAddUi = (item: UiMenuItem) => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    const result = addUiNode(editorRef.current, artboard.id, item)
    setDoc(result.document)
    setSelection({ artboardId: artboard.id, nodeId: result.newNode })
  }

  const handleSetNode3D = (value: Node3D) => {
    if (!selection || !editorRef.current) return
    setDoc(setNode3D(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  /** + menu's Light 2D items — Unity-style lights, added at the scene root. */
  const handleAddLight2D = (lightType: Light2DType) => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    const result = addLight2DNode(editorRef.current, artboard.id, null, lightType)
    setDoc(result.document)
    setSelection({ artboardId: artboard.id, nodeId: result.newNode })
  }

  /** + menu's Trail Renderer — parented under the current selection (if
   * any) so it immediately trails whatever it's attached to, the way you'd
   * add a Trail Renderer component to a moving object in Unity. */
  const handleAddTrail = () => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    const parent = selection && idsEqual(selection.artboardId, artboard.id) ? selection.nodeId : null
    const result = addTrailNode(editorRef.current, artboard.id, parent)
    setDoc(result.document)
    setSelection({ artboardId: artboard.id, nodeId: result.newNode })
  }

  const handleSetLight2D = (value: Light2DConfig) => {
    if (!selection || !editorRef.current) return
    setDoc(setLight2D(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  const handleSetTrail = (value: TrailConfig) => {
    if (!selection || !editorRef.current) return
    setDoc(setTrail(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  const handleSetShadowCaster = (value: ShadowCasterConfig | null) => {
    if (!selection || !editorRef.current) return
    setDoc(setShadowCaster(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  const handleSetNodeUnlit = (value: boolean) => {
    if (!selection || !editorRef.current) return
    setDoc(setNodeUnlit(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  /** Inspector Canvas section's "Main Camera" picker. */
  const handleSetMainCamera = (camera: OpaqueId | null) => {
    if (!editorRef.current || !activeArtboard) return
    setDoc(setActiveCamera(editorRef.current, activeArtboard.id, camera))
  }

  const handleSetBoneLength = (value: number) => {
    if (!selection || !editorRef.current) return
    setDoc(setBoneLength(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  const handleSetBlendMode = (value: BlendMode) => {
    if (!selection || !editorRef.current) return
    setDoc(setShapeBlendMode(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  const handleSetFillColor = (value: [number, number, number, number]) => {
    if (!selection || !editorRef.current) return
    setDoc(setFillColor(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  const handleSetGradient = (value: GradientDef | null) => {
    if (!selection || !editorRef.current) return
    setDoc(setGradient(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  /** Inspector's asset-mode fields (Resize/Filter Mode/Effect) — commits the
   * whole `ImageProcessing` struct at once, same "whole struct, not
   * per-field" convention `handleSetPostEffects` below already uses.
   * `async` because re-baking the asset (`WasmEditor::set_image_processing`)
   * is a real GPU readback — awaited here so the canvas reflects the new
   * params by the time this resolves, same as `handleImportImage`. */
  const handleSetImageProcessing = (imageId: OpaqueId, processing: ImageProcessing) => {
    if (!editorRef.current) return
    const editor = editorRef.current
    setImageProcessing(editor, imageId, processing)
      .then(setDoc)
      .catch((err) => console.error('failed to update image processing', err))
  }

  /** Effects panel's own single edit point — every add/remove/reorder/
   * toggle/param-edit action builds the whole next `PostEffectLayer[]` and
   * calls this, matching `EffectsPanel`'s own doc comment on why it's one
   * `onChange`, not one callback per action. */
  const handleSetPostEffects = (layers: PostEffectLayer[]) => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    setDoc(setPostEffects(editorRef.current, artboard.id, layers))
  }

  const handleSetTextContent = (value: string) => {
    if (!selection || !editorRef.current) return
    setDoc(setTextContent(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  const handleSetTextSize = (value: number) => {
    if (!selection || !editorRef.current) return
    setDoc(setTextSize(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  /** Components panel's "Make Component from Selection" — bundles the
   * currently selected Shape node's path/fill color into a new definition.
   * No-op if nothing's selected (the panel itself only shows this action
   * when `selectedNode?.kind === 'Shape'`, so this guard is a belt-and-braces
   * check, not the primary gate). */
  const handleAddComponent = (name: string) => {
    if (!selection || !editorRef.current) return
    setDoc(addComponent(editorRef.current, selection.artboardId, selection.nodeId, name))
  }

  const handleDeleteComponent = (componentId: OpaqueId) => {
    if (!editorRef.current) return
    setDoc(deleteComponent(editorRef.current, componentId))
  }

  const handleSetComponentDefaultFillColor = (componentId: OpaqueId, color: [number, number, number, number]) => {
    if (!editorRef.current) return
    setDoc(setComponentDefaultFillColor(editorRef.current, componentId, color))
  }

  const handleAddComponentVariant = (componentId: OpaqueId, name: string, color: [number, number, number, number]) => {
    if (!editorRef.current) return
    setDoc(addComponentVariant(editorRef.current, componentId, name, color))
  }

  /** Components panel's "Add Instance to Artboard" — drops a new root
   * instance of `componentId` onto the first artboard and selects it,
   * matching how `handleAddShape`/`handleAddBone` already select whatever
   * they just created. */
  // ---- Prefabs (Inspector prefab bar + Resources ▸ Prefabs) ----
  const handleCreatePrefab = () => {
    const editor = editorRef.current
    if (!editor || !selection || !selectedNode) return
    const result = createPrefab(editor, selection.artboardId, selection.nodeId, selectedNode.name || 'Prefab')
    setDoc(result.document)
  }
  /** Runs a prefab-instance action (Apply/Revert/Unpack) on the selected
   * node's instance. Errors (not an instance) are ignored — the bar only
   * offers these on instances. */
  const runPrefabAction = (action: typeof applyPrefabInstance) => {
    const editor = editorRef.current
    if (!editor || !selection) return
    try {
      setDoc(action(editor, selection.artboardId, selection.nodeId))
    } catch (err) {
      console.warn('prefab action failed', err)
    }
  }
  const handleSelectPrefabRoot = () => {
    const editor = editorRef.current
    if (!editor || !selection) return
    const root = prefabInstanceRoot(editor, selection.artboardId, selection.nodeId)
    if (root) selectSingle(selection.artboardId, root)
  }
  /** Places a prefab / image / drawing from Resources: at `canvasPoint`
   * (a drop, canvas-relative CSS px) or the center of the view. */
  const handlePlaceAssetAt = (asset: DraggedAsset, canvasPoint: { x: number; y: number } | null) => {
    const editor = editorRef.current
    const artboard = activeArtboard
    if (!editor || !artboard) return
    const size = canvasSizeRef.current
    const at = canvasPoint ?? { x: size.width / 2, y: size.height / 2 }
    const world = mouseToWorld(editor, artboard.id, at.x, at.y, cameraRef.current, size)
    try {
      if (asset.kind === 'prefab') {
        const result = instantiatePrefab(editor, artboard.id, asset.id, null, world.x, world.y)
        setDoc(result.document)
        if (result.node) selectSingle(artboard.id, result.node)
      } else if (asset.kind === 'image') {
        const result = addImageNode(editor, artboard.id, null, asset.id, world.x, world.y)
        setDoc(result.document)
        selectSingle(artboard.id, result.newNode)
      } else if (asset.kind === 'drawing') {
        const result = placeDrawing(editor, artboard.id, asset.id, null, world.x, world.y)
        setDoc(result.document)
        selectSingle(artboard.id, result.newNode)
      } else {
        handleAddInstance(asset.id)
      }
    } catch (err) {
      console.warn('placing asset failed', err)
    }
  }
  const handlePlacePrefab = (prefabId: OpaqueId) => handlePlaceAssetAt({ kind: 'prefab', id: prefabId }, null)

  const handleAddInstance = (componentId: OpaqueId) => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    const result = addInstanceNode(editorRef.current, artboard.id, null, componentId)
    setDoc(result.document)
    setSelection({ artboardId: artboard.id, nodeId: result.newNode })
  }

  const handleSetInstanceVariant = (value: string | null) => {
    if (!selection || !editorRef.current) return
    setDoc(setInstanceVariant(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  const handleSetInstanceFillColorOverride = (value: [number, number, number, number] | null) => {
    if (!selection || !editorRef.current) return
    setDoc(setInstanceFillColorOverride(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  const handleSetElementId = (value: string) => {
    if (!selection || !editorRef.current) return
    setDoc(setNodeId(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  const handleSetClasses = (value: string[]) => {
    if (!selection || !editorRef.current) return
    setDoc(setNodeClasses(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  const handleSetClipChildren = (value: boolean) => {
    if (!selection || !editorRef.current) return
    setDoc(setNodeClipChildren(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  const handleSetShapeImage = (value: OpaqueId | null) => {
    if (!selection || !editorRef.current) return
    setDoc(setShapeImage(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  const handleSetStroke = (value: Stroke | null) => {
    if (!selection || !editorRef.current) return
    setDoc(setShapeStroke(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  const handleSetParticleSystem = (value: ParticleSystemConfig) => {
    if (!selection || !editorRef.current) return
    setDoc(setParticleSystem(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  /** Particle System section's "Replay" button — restarts the live
   * simulation (`WasmEditor::replay_particle_system`) so a burst/preview can
   * be re-triggered without switching modes or scrubbing the Timeline. */
  const handleReplayParticleSystem = () => {
    if (!selection || !editorRef.current) return
    replayParticleSystem(editorRef.current, selection.artboardId, selection.nodeId)
  }

  /** `PathEditOverlay`'s live-drag preview — untracked, no undo entry (the
   * actual canvas already redraws from the live wasm document state every
   * frame regardless of React's own `doc` mirror, so no `setDoc` here —
   * same reasoning as `handlePreviewNodeTranslation` and friends). */
  const handlePreviewPath = (value: Path) => {
    if (!selection || !editorRef.current) return
    previewShapePath(editorRef.current, selection.artboardId, selection.nodeId, value)
  }

  /** `PathEditOverlay`'s commit action — one undoable edit per drag gesture. */
  const handleCommitPath = (value: Path) => {
    if (!selection || !editorRef.current) return
    setDoc(setShapePath(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  /** `TilemapPaintOverlay`'s paint action — one undoable edit per cell click
   * (`Command::SetTile`'s own doc comment on why this granularity, not one
   * edit per whole brush stroke). */
  const handleSetTile = (node: OpaqueId, x: number, y: number, value: number | null) => {
    if (!selection || !editorRef.current) return
    setDoc(setTile(editorRef.current, selection.artboardId, node, x, y, value))
  }

  /** Inspector Tilemap section's "Map Size" fields. */
  const handleResizeTilemap = (mapWidth: number, mapHeight: number) => {
    if (!selection || !editorRef.current) return
    setDoc(resizeTilemap(editorRef.current, selection.artboardId, selection.nodeId, mapWidth, mapHeight))
  }

  /** Inspector Tilemap section's "Cell Size" field. */
  const handleSetTilemapCellSize = (value: number) => {
    if (!selection || !editorRef.current) return
    setDoc(setTilemapCellSize(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  /** Inspector's Camera section — the whole config at once. */
  const handleSetCamera = (value: CameraConfig) => {
    if (!selection || !editorRef.current) return
    setDoc(setCameraNode(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  /** Inspector's Camera section — "Active Camera" checkbox. `false` clears
   * this artboard's `activeCamera`; `true` sets it to the currently
   * selected Camera node. */
  const handleSetActiveCamera = (value: boolean) => {
    if (!selection || !editorRef.current) return
    setDoc(setActiveCamera(editorRef.current, selection.artboardId, value ? selection.nodeId : null))
  }

  /** Inspector's "Y Sort" checkbox (Group nodes only). */
  const handleSetGroupYSort = (value: boolean) => {
    if (!selection || !editorRef.current) return
    setDoc(setGroupYSort(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  /** Inspector's "Z Index" field (any node kind). */
  const handleSetNodeZIndex = (value: number) => {
    if (!selection || !editorRef.current) return
    setDoc(setNodeZIndex(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  /** Inspector's Behaviors section — whole attached-behaviors list. */
  const handleSetNodeBehaviors = (value: NodeBehaviorEntry[]) => {
    if (!selection || !editorRef.current) return
    setDoc(setNodeBehaviors(editorRef.current, selection.artboardId, selection.nodeId, value))
  }

  /** Behaviors section's "+ New Script..." action. */
  const handleAddScript = (name: string) => {
    if (!editorRef.current) return
    setDoc(addScript(editorRef.current, name))
  }

  /** Behaviors section's "Edit Fields" authoring UI. */
  const handleSetScriptFields = (script: OpaqueId, fields: ScriptFieldDef[]) => {
    if (!editorRef.current) return
    setDoc(setScriptFields(editorRef.current, script, fields))
  }

  /** Inspector's IK-target fields — only shown for a bone whose own parent
   * is also a bone (the "mid" bone of a two-bone chain). `root` is that
   * parent. See `editor.rs`'s `solve_two_bone_ik` doc comment for why this
   * bakes rotations immediately rather than previewing live while dragging
   * (there's no drag gesture here yet, just a target X/Y field + button). */
  const handleSolveIk = (root: OpaqueId, targetX: number, targetY: number) => {
    if (!selection || !editorRef.current) return
    setDoc(solveTwoBoneIk(editorRef.current, selection.artboardId, root, selection.nodeId, targetX, targetY))
  }

  /** Timeline handlers — all scoped to the current selection/first artboard,
   * same "no selection, no-op" guard every other edit handler here uses.
   * `handleScrub` calls `setDoc` on every scrub position, unlike the
   * on-canvas drag preview handlers below (`previewNodeTranslation` etc,
   * which deliberately skip it) — a scrub is comparatively low-frequency
   * (one timeline, not a whole canvas of pointer events) and keeping the
   * Inspector's position readout live while scrubbing is worth the extra
   * `setDoc` calls here. */
  const handleScrub = (time: number) => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    setDoc(setScrubTime(editorRef.current, artboard.id, time))
  }

  const handleAddKeyframe = (property: PropertyName, time: number) => {
    if (!selection || !editorRef.current) return
    setDoc(addKeyframe(editorRef.current, selection.artboardId, selection.nodeId, property, time))
  }

  const handleRemoveKeyframe = (property: PropertyName, time: number) => {
    if (!selection || !editorRef.current) return
    setDoc(removeKeyframe(editorRef.current, selection.artboardId, selection.nodeId, property, time))
  }

  const handlePreviewKeyframeTime = (property: PropertyName, index: number, newTime: number) => {
    if (!selection || !editorRef.current) return
    previewKeyframeTime(editorRef.current, selection.artboardId, selection.nodeId, property, index, newTime)
  }

  const handleCommitKeyframeTime = (property: PropertyName, index: number, originTime: number, newTime: number) => {
    if (!selection || !editorRef.current) return
    setDoc(commitKeyframeTime(editorRef.current, selection.artboardId, selection.nodeId, property, index, originTime, newTime))
  }

  const handleSetKeyframeEasing = (property: PropertyName, index: number, easing: Easing) => {
    if (!selection || !editorRef.current) return
    setDoc(setKeyframeEasing(editorRef.current, selection.artboardId, selection.nodeId, property, index, easing))
  }

  /** Binds (or rebinds) the selected node's `MotionPath` track to `guide` —
   * the Timeline's "pick a guide shape" action. See
   * `Command::SetMotionPathGuide`'s doc comment; creates an empty track on
   * first use, keeps existing progress keyframes on a rebind. */
  const handleSetMotionPathGuide = (guide: OpaqueId) => {
    if (!selection || !editorRef.current) return
    setDoc(setMotionPathGuide(editorRef.current, selection.artboardId, selection.nodeId, guide))
  }

  const handleRemoveMotionPathTrack = () => {
    if (!selection || !editorRef.current) return
    setDoc(removeMotionPathTrack(editorRef.current, selection.artboardId, selection.nodeId))
  }

  /** Adds (or, at an already-keyframed time, overwrites) a `MotionPath`
   * progress keyframe — the `MotionPath` counterpart to `handleAddKeyframe`,
   * taking an explicit `progress` from the Timeline's own progress field
   * instead of the node's current live value (see
   * `WasmEditor::add_motion_path_keyframe`'s doc comment for why). */
  const handleAddMotionPathKeyframe = (time: number, progress: number) => {
    if (!selection || !editorRef.current) return
    setDoc(addMotionPathKeyframe(editorRef.current, selection.artboardId, selection.nodeId, time, progress))
  }

  /** Timeline animation-dropdown handlers — "+ New Animation" auto-names
   * sequentially ("Animation 1", "Animation 2", ...) rather than prompting,
   * keeping the interaction a single click; renaming isn't exposed in the
   * UI yet even though `WasmEditor::rename_animation` exists (see
   * `ipc/wasmEditor.ts`), left for a later pass. */
  const handleAddAnimation = () => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    const name = `Animation ${artboard.animations.length + 1}`
    const result = addAnimation(editorRef.current, artboard.id, name)
    setDoc(result.document)
  }

  const handleSelectAnimation = (animationId: OpaqueId) => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    setDoc(setActiveAnimation(editorRef.current, artboard.id, animationId))
  }

  const handleDeleteAnimation = (animationId: OpaqueId) => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    setDoc(deleteAnimation(editorRef.current, artboard.id, animationId))
  }

  const handleSetAnimationDuration = (seconds: number) => {
    const artboard = artboards[0]
    const animationId = artboard?.activeAnimation
    if (!editorRef.current || !artboard || !animationId) return
    setDoc(setAnimationDuration(editorRef.current, artboard.id, animationId, seconds))
  }

  /** Inspector "Canvas" section — artboard-level settings, not gated on a
   * node selection (unlike everything else the Inspector edits). */
  const handleSetArtboardSize = (width: number, height: number) => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    setDoc(setArtboardSize(editorRef.current, artboard.id, width, height))
  }

  // Physics 2D gravity for the scene being edited (world units / s², +y up).
  const handleSetArtboardGravity3D = (x: number, y: number, z: number) => {
    if (!editorRef.current || !activeArtboard) return
    setDoc(setArtboardGravity3D(editorRef.current, activeArtboard.id, x, y, z))
  }

  const handleSetArtboardGravity = (x: number, y: number) => {
    if (!editorRef.current || !activeArtboard) return
    setDoc(setArtboardGravity(editorRef.current, activeArtboard.id, x, y))
  }

  const handleSetArtboardScreen = (value: ScreenSettings) => {
    if (!editorRef.current || !activeArtboard) return
    setDoc(setArtboardScreen(editorRef.current, activeArtboard.id, value))
  }

  const handleSetArtboardResolutionScale = (value: number) => {
    const artboard = artboards[0]
    if (!editorRef.current || !artboard) return
    setDoc(setArtboardResolutionScale(editorRef.current, artboard.id, value))
  }

  /** Inspector "Delete" button / Delete-Backspace key — soft-deletes the
   * current selection as one undoable edit (`Command::SetDeleted`) and
   * clears the selection, since the deleted node no longer appears in
   * `LayersPanel`/`InspectorPanel` (`dto::summarize_document` skips it). */
  const handleDeleteSelected = () => {
    if (!selection || !editorRef.current) return
    setDoc(
      multiSelectedIds.length > 1
        ? deleteNodes(editorRef.current, selection.artboardId, multiSelectedIds)
        : deleteNode(editorRef.current, selection.artboardId, selection.nodeId),
    )
    setSelection(null)
    setMultiSelectedIds([])
    setSelectionScope(null)
  }

  /** `Ctrl+C` — records the current selection into `clipboardRef`. No
   * document edit at all (nothing to undo), just remembers what `Ctrl+V`
   * should duplicate. */
  const handleCopySelection = () => {
    if (!selection || multiSelectedIds.length === 0) return
    clipboardRef.current = { artboardId: selection.artboardId, nodeIds: multiSelectedIds }
  }

  /** `Ctrl+V` — "Paste in Place": duplicates whatever `Ctrl+C` last recorded,
   * at the exact same position (no cursor-tracking/offset paste), and
   * selects the fresh copies so an immediate drag moves the *duplicates*,
   * not the originals. A no-op if nothing's been copied yet, or the copied
   * artboard no longer exists (e.g. after an undo past the copy). */
  const handlePasteInPlace = () => {
    const clipboard = clipboardRef.current
    if (!clipboard || !editorRef.current) return
    try {
      const result = duplicateNodes(editorRef.current, clipboard.artboardId, clipboard.nodeIds)
      setDoc(result.document)
      setMultiSelectedIds(result.newNodes)
      setSelection(result.newNodes[0] ? { artboardId: clipboard.artboardId, nodeId: result.newNodes[result.newNodes.length - 1] } : null)
      setSelectionScope(null)
    } catch (err) {
      console.error('Paste in place failed', err)
    }
  }

  const handleGroupSelection = () => {
    if (!selection || !editorRef.current || multiSelectedIds.length < 2) return
    try {
      const result = groupNodes(editorRef.current, selection.artboardId, multiSelectedIds, 'Group')
      setDoc(result.document)
      setSelection({ artboardId: selection.artboardId, nodeId: result.newNode })
      setMultiSelectedIds([result.newNode])
      setSelectionScope(null)
    } catch (err) {
      alert(err instanceof Error ? err.message : String(err))
    }
  }

  /** Layer z-order (Bring Forward/Send Backward/Bring to Front/Send to
   * Back) — the standard Figma/Illustrator convention, both as Layers panel
   * buttons and `Ctrl+]`/`Ctrl+[`/`Ctrl+Shift+]`/`Ctrl+Shift+[` (see the
   * `keydown` handler below). Acts on the primary selection only, same
   * "single-target edit" scope every other single-node Layers action here
   * uses (`handleReparentNode`, `handleSetGroupMask`) — reordering a whole
   * multi-selection as one relative-order-preserving batch is a real, richer
   * feature that isn't part of this pass. */
  const handleMoveNodeForward = () => {
    if (!selection || !editorRef.current) return
    setDoc(moveNodeForward(editorRef.current, selection.artboardId, selection.nodeId))
  }
  const handleSendNodeBackward = () => {
    if (!selection || !editorRef.current) return
    setDoc(sendNodeBackward(editorRef.current, selection.artboardId, selection.nodeId))
  }
  const handleBringNodeToFront = () => {
    if (!selection || !editorRef.current) return
    setDoc(bringNodeToFront(editorRef.current, selection.artboardId, selection.nodeId))
  }
  const handleSendNodeToBack = () => {
    if (!selection || !editorRef.current) return
    setDoc(sendNodeToBack(editorRef.current, selection.artboardId, selection.nodeId))
  }

  /** Layers panel double-click-to-rename. Undoable (`Command::SetNodeName`). */
  const handleRenameNode = (artboardId: OpaqueId, nodeId: OpaqueId, name: string) => {
    if (!editorRef.current) return
    setDoc(renameNode(editorRef.current, artboardId, nodeId, name))
  }

  /** Layers panel drag-to-reparent — dropping a row onto another row (or the
   * artboard's own pill, `parent: null`) moves it there. Undoable
   * (`Command::ReparentNode`). Errors (a would-be cycle) are surfaced as a
   * plain alert rather than silently swallowed — there's no toast/notification
   * system yet, and a silent no-op after a drag would read as a UI bug. */
  const handleReparentNode = (artboardId: OpaqueId, nodeId: OpaqueId, parent: OpaqueId | null) => {
    if (!editorRef.current) return
    try {
      setDoc(reparentNode(editorRef.current, artboardId, nodeId, parent))
    } catch (err) {
      alert(err instanceof Error ? err.message : String(err))
    }
  }

  /** The Layers panel's "sibling" drop zone — a real drag-to-*reorder*,
   * unlike `handleReparentNode` alone (which always lands the dragged node
   * at the very end of its new parent's children — see
   * `WasmEditor.reparent_node_before`'s own doc comment for why that made
   * reordering look broken). */
  const handleReparentNodeBefore = (artboardId: OpaqueId, nodeId: OpaqueId, parent: OpaqueId | null, before: OpaqueId | null) => {
    if (!editorRef.current) return
    try {
      setDoc(reparentNodeBefore(editorRef.current, artboardId, nodeId, parent, before))
    } catch (err) {
      alert(err instanceof Error ? err.message : String(err))
    }
  }

  /** Layers panel's per-group "Add child" button — a default rectangle,
   * parented directly under the clicked group. Selects the new shape
   * immediately, same "select whatever was just created" convention every
   * other add-node action here uses. */
  const handleAddChildShape = (artboardId: OpaqueId, parent: OpaqueId) => {
    if (!editorRef.current) return
    const result = addShapeNode(editorRef.current, artboardId, 'Rectangle', parent)
    setDoc(result.document)
    setSelection({ artboardId, nodeId: result.newNode })
    setMultiSelectedIds([result.newNode])
  }

  /** Layers panel's Alt-drop "use as mask" action — sets/clears which child
   * clips a `Group`'s contents. Undoable (`Command::SetGroupMask`). */
  const handleSetGroupMask = (artboardId: OpaqueId, groupId: OpaqueId, mask: OpaqueId | null) => {
    if (!editorRef.current) return
    try {
      setDoc(setGroupMask(editorRef.current, artboardId, groupId, mask))
    } catch (err) {
      alert(err instanceof Error ? err.message : String(err))
    }
  }

  /** The Layers panel drag gesture's own "clip children" toggle — a general
   * `(artboardId, nodeId, value)` version of `handleSetClipChildren` above,
   * which only ever operates on the current Inspector `selection`. The drag
   * target here is whatever row the dragged layer landed on, independent of
   * (and often different from) whatever's currently selected. */
  const handleSetClipChildrenFor = (artboardId: OpaqueId, nodeId: OpaqueId, value: boolean) => {
    if (!editorRef.current) return
    setDoc(setNodeClipChildren(editorRef.current, artboardId, nodeId, value))
  }

  // Undo/redo can delete the open scene out from under the editor (undoing
  // "+ New Scene" while viewing it) — when that happens, switch explicitly to
  // whatever scene remains first, same reasoning `handleDeleteScene` gives.
  const applyHistoryStep = (summary: DocumentSummary) => {
    const stillOpen = activeArtboard && summary.artboards.some((a) => idsEqual(a.id, activeArtboard.id))
    const fallback = summary.artboards[0]
    if (!stillOpen && fallback) switchToScene(fallback.id, summary)
    else setDoc(summary)
  }

  const handleUndo = () => {
    if (!editorRef.current) return
    applyHistoryStep(wasmUndo(editorRef.current))
  }

  const handleRedo = () => {
    if (!editorRef.current) return
    applyHistoryStep(wasmRedo(editorRef.current))
  }

  // Project folders (Unity-style: the project is a folder of files on disk,
  // autosaved) — see `project/useProject.tsx`. Single-file `.tea`/`.teab`
  // documents are still importable/exportable from the File menu.
  const project = useProject({
    editorRef,
    ready: doc !== null,
    playing: mode === 'Playground',
    onReplaced: (summary) => {
      setDoc(summary)
      setActiveArtboardId(null)
      setSelection(null)
      setMultiSelectedIds([])
      setSelectionScope(null)
    },
    onChanged: (summary) => setDoc(summary),
  })
  const handleSave = project.save
  const handleOpen = project.fileMenu.onOpenProject

  // ---- Scenes (artboards) ----
  // Every switch clears selection: a node id only means something within
  // its own scene, so a selection carried across would point at nothing
  // (or worse, at an unrelated node that happens to share the same id in
  // the new scene's own slotmap).
  const switchToScene = (artboardId: OpaqueId, summary?: DocumentSummary) => {
    if (!editorRef.current) return
    if (summary) setDoc(summary)
    setDoc(setActiveArtboard(editorRef.current, artboardId))
    activeArtboardIdRef.current = artboardId
    setActiveArtboardId(artboardId)
    setSelection(null)
    setMultiSelectedIds([])
    setSelectionScope(null)
    setPathEditNodeId(null)
  }

  // The render loop is set up once on mount, so it reaches the *current*
  // `switchToScene` (for `ctx.loadScene`) through this ref.
  const switchToSceneRef = useRef(switchToScene)
  switchToSceneRef.current = switchToScene

  const handleSelectScene = (artboardId: OpaqueId) => {
    if (activeArtboard && idsEqual(activeArtboard.id, artboardId)) return
    switchToScene(artboardId)
  }

  const handleAddScene = (name: string) => {
    if (!editorRef.current) return
    const width = activeArtboard?.width ?? 640
    const height = activeArtboard?.height ?? 480
    const result = addArtboard(editorRef.current, name, width, height)
    switchToScene(result.newArtboard, result.document)
  }

  const handleRenameScene = (artboardId: OpaqueId, name: string) => {
    if (!editorRef.current) return
    setDoc(renameArtboard(editorRef.current, artboardId, name))
  }

  const handleDeleteScene = (artboardId: OpaqueId) => {
    if (!editorRef.current || allArtboards.length <= 1) return
    const summary = deleteArtboard(editorRef.current, artboardId)
    const wasActive = activeArtboard && idsEqual(activeArtboard.id, artboardId)
    const fallback = summary.artboards[0]
    // Deleting the open scene switches explicitly (not just letting both
    // sides fall back on their own) so `WasmEditor::set_active_artboard`
    // actually runs and clears its `NodeId`-keyed caches for the new scene.
    if (wasActive && fallback) switchToScene(fallback.id, summary)
    else setDoc(summary)
  }

  /** Toolbar "Export GIF" — renders the current artboard's active animation
   * to an animated `.gif` for comparing against the source file's real
   * playback (e.g. in a browser tab or the original Lottie tool) side by
   * side. See `exportGif.ts`'s own doc comment for how the capture itself
   * works; `exportingRef` (set here, cleared in `finally`) is what stops
   * the main render loop from fighting it over the canvas mid-export. */
  /** Toolbar "Export Game": the whole document as one playable `.html`
   * (`game/exportGame.ts`), opening on the scene currently shown. Saved
   * from the *designed* state — leaving Playground first restores every
   * pose a running game changed. */
  /** `target`: a web page (`.html`), or a desktop app — the same page
   * appended to the prebuilt runner (`apps/runner`), one double-clickable
   * executable. */
  const handleExportGame = async (target: 'web' | 'desktop' = 'web') => {
    const editor = editorRef.current
    if (!editor || !activeArtboard) return
    const title = activeArtboard.name || 'Game'
    const stem = title.replace(/[^\w\- ]+/g, '').trim().replace(/\s+/g, '-') || 'game'
    const path = target === 'web' ? await pickSaveHtmlPath(`${stem}.html`) : await pickSaveExePath(`${stem}.exe`)
    if (!path) return
    if (mode === 'Playground') {
      setMode('Design')
      // Let the mode effect restore the designed pose before saving.
      await new Promise((resolve) => setTimeout(resolve, 100))
    }
    try {
      const html = await buildGameHtml(editor, { title, startScene: activeArtboard.name })
      if (target === 'web') await writeTextFile(path, html)
      else await exportDesktopGame(path, html)
    } catch (err) {
      console.error('Export Game failed', err)
      window.alert(`Export failed: ${err instanceof Error ? err.message : String(err)}`)
    }
  }

  /** Export menu → GIF / transparent GIF / WebM (transparent) / MP4. */
  const handleExportAnimation = (options: AnimationExportOptions) => {
    const artboard = artboards[0]
    if (!editorRef.current || !canvasRef.current || !artboard) return
    const editor = editorRef.current
    const canvas = canvasRef.current
    const info = EXPORT_FORMAT_INFO[options.format]
    ;(async () => {
      const stem = (artboard.name || 'untitled').replace(/[^\w\- ]+/g, '').trim().replace(/\s+/g, '-') || 'untitled'
      const path = await pickSaveFilePath(`${stem}${options.transparent ? '-transparent' : ''}.${info.extension}`, info.filter, [info.extension])
      if (!path) return
      exportingRef.current = true
      try {
        await exportActiveAnimation(editor, canvas, artboard, path, options)
      } finally {
        exportingRef.current = false
      }
    })().catch((err) => {
      console.error('Animation export failed', err)
      alert(err instanceof GifExportError ? err.message : `${info.label} export failed — see the console for details.`)
    })
  }

  /** Canvas-pixel (`clientX`/`clientY`, origin top-left, +y down) -> world/
   * artboard-space (origin centered, +y up) — same convention every other
   * pointer-facing code in this workspace uses (`bones.js`'s cursor-follow
   * demo, `WasmPlayer`'s `track_pointer` listener). Goes through the current
   * camera's pan/zoom (`cameraRef`) now that the canvas is a real viewport
   * rather than sized 1:1 with the artboard — reads refs, not state
   * directly, so this stays correct without needing to live inside a
   * `camera`-dependent effect (see the camera/canvasSize ref comment above). */
  const toArtboardPoint = (clientX: number, clientY: number) => {
    const rect = canvasRef.current!.getBoundingClientRect()
    return screenToWorld(clientX - rect.left, clientY - rect.top, cameraRef.current, canvasSizeRef.current)
  }

  const findArtboard = (id: OpaqueId) => artboards.find((a) => a.id.idx === id.idx && a.id.version === id.version)

  const findNode = (artboard: ArtboardSummary, id: OpaqueId) => artboard.nodes.find((n) => idsEqual(n.id, id))

  /** `true` if `id` sits anywhere underneath `ancestorId` in the parent
   * chain (not `id === ancestorId` itself) — used to tell whether a click
   * landed inside the group currently "entered" (`selectionScope`), or
   * outside it (in which case the scope has to reset before resolving what
   * was actually clicked). */
  const isDescendantOf = (id: OpaqueId, ancestorId: OpaqueId, artboard: ArtboardSummary): boolean => {
    let current = findNode(artboard, id)
    while (current?.parent) {
      if (idsEqual(current.parent, ancestorId)) return true
      current = findNode(artboard, current.parent)
    }
    return false
  }

  /**
   * The Figma-style scoped click-to-select rule: within `scope` (`null` =
   * top level), clicking anywhere on a shape selects the *immediate child of
   * scope* that contains it — a whole group is one clickable, draggable unit
   * until you double-click into it (`handleCanvasDoubleClick`), rather than
   * every click reaching straight down to the leaf shape under the cursor.
   * If `hitId` isn't actually inside `scope` at all (clicked something
   * outside the currently-entered group), the scope resets to top-level
   * first (`nextScope: null`) and resolution proceeds from there — the
   * "click outside the current group backs you out of it" half of the
   * convention.
   */
  const resolveCanvasSelection = (
    hitId: OpaqueId,
    artboard: ArtboardSummary,
    scope: OpaqueId | null,
  ): { target: OpaqueId; nextScope: OpaqueId | null } => {
    const effectiveScope = scope && isDescendantOf(hitId, scope, artboard) ? scope : null
    let current = findNode(artboard, hitId)
    if (!current) return { target: hitId, nextScope: effectiveScope }
    while (current.parent && !(effectiveScope && idsEqual(current.parent, effectiveScope))) {
      const parent = findNode(artboard, current.parent)
      if (!parent) break
      current = parent
    }
    return { target: current.id, nextScope: effectiveScope }
  }

  /** Replaces the whole selection with just `nodeId` — a plain (no
   * modifier) click on a node that wasn't already part of the current
   * multi-selection, on the canvas or in the Layers panel. */
  const selectSingle = (artboardId: OpaqueId, nodeId: OpaqueId) => {
    setSelection({ artboardId, nodeId })
    setMultiSelectedIds([nodeId])
    setSelectedAssetId(null)
  }

  /** Adds/removes `nodeId` from the multi-selection (Ctrl/Cmd/Shift-click) —
   * both modifiers are treated identically (a toggle), rather than Shift
   * meaning "range-select": there's no stable, meaningful "range" between
   * two arbitrary layers/canvas shapes the way there is between two rows in
   * a flat list. `selection` (the "active" node every single-target editor
   * reads) follows whichever id ends up last in the resulting set — the
   * most-recently-added one, or, after removing the previously-active node,
   * whatever's left. */
  const toggleMultiSelect = (artboardId: OpaqueId, nodeId: OpaqueId) => {
    setMultiSelectedIds((ids) => {
      const next = ids.some((id) => idsEqual(id, nodeId)) ? ids.filter((id) => !idsEqual(id, nodeId)) : [...ids, nodeId]
      setSelection(next.length > 0 ? { artboardId, nodeId: next[next.length - 1] } : null)
      return next
    })
    setSelectedAssetId(null)
  }

  /** Resources panel's row click — mutually exclusive with node selection,
   * see `selectedAssetId`'s own doc comment. */
  const handleSelectAsset = (assetId: OpaqueId) => {
    setSelectedAssetId(assetId)
    setSelection(null)
    setMultiSelectedIds([])
  }

  /** Starts a drag when the cursor lands on a real shape (`pick_node_at`);
   * an empty-canvas mousedown clears the selection (and exits whatever
   * group was entered) and starts panning the camera instead — the standard
   * "drag empty canvas to pan" convention (Figma/Rive), now that the canvas
   * can show space beyond the artboard.
   *
   * `pick_node_at` always returns the deepest *leaf shape* under the cursor
   * (it hit-tests real geometry, and only shapes have any) — `resolveCanvasSelection`
   * is what turns that into "the group as a whole," per its own doc comment.
   * Everything below (multi-select toggling, drag setup) operates on the
   * *resolved* target, not the raw hit, so selecting/dragging a group moves
   * the whole thing as one rigid body instead of just the one leaf shape the
   * cursor happened to land on. */
  const handleCanvasMouseDown = (event: ReactMouseEvent<HTMLCanvasElement>) => {
    // Without this, a fast drag can anchor the browser's native text
    // selection on whatever's behind/around the canvas, which then visibly
    // "fights" the drag (the classic accidental-select-while-dragging bug).
    event.preventDefault()
    const editor = editorRef.current
    const artboard = artboards[0]
    if (!editor || !artboard) return

    // Playground is meant to be played, not edited: the mouse belongs to
    // the game (scripts' "mouse" key, UI controls), so a click neither
    // selects nor drags nodes, nor pans the editor view underneath.
    if (mode === 'Playground') return

    const point = toArtboardPoint(event.clientX, event.clientY)
    const hit = pickNodeAt(editor, artboard.id, point.x, point.y)
    if (!hit) {
      setSelection(null)
      setMultiSelectedIds([])
      setSelectionScope(null)
      panRef.current = { startClientX: event.clientX, startClientY: event.clientY, startPan: cameraRef.current.pan }
      return
    }

    const { target, nextScope } = resolveCanvasSelection(hit, artboard, selectionScope)
    if (nextScope !== selectionScope) setSelectionScope(nextScope)

    const node = findNode(artboard, target)
    if (!node) return

    // A modifier-click toggles the clicked shape into/out of the
    // multi-selection instead of starting a drag or replacing the selection
    // — matches the Layers panel's own `LayerRow` modifier-click handling.
    if (event.ctrlKey || event.metaKey || event.shiftKey) {
      toggleMultiSelect(artboard.id, target)
      return
    }

    // Clicking an already multi-selected shape (no modifier) keeps the
    // whole selection intact and drags every member of it together — only a
    // click on something *not* already selected collapses back to a single
    // selection, the same convention Figma/Illustrator use.
    const isPartOfMultiSelection = multiSelectedIds.some((id) => idsEqual(id, target))
    if (!isPartOfMultiSelection) selectSingle(artboard.id, target)

    const others = isPartOfMultiSelection
      ? multiSelectedIds
          .filter((id) => !idsEqual(id, target))
          .map((id) => {
            const other = findNode(artboard, id)
            return other ? { nodeId: id, origin: { x: other.transform.translation.x, y: other.transform.translation.y } } : null
          })
          .filter((entry): entry is { nodeId: OpaqueId; origin: { x: number; y: number } } => entry !== null)
      : []

    dragRef.current = {
      artboardId: artboard.id,
      nodeId: target,
      origin: { x: node.transform.translation.x, y: node.transform.translation.y },
      startPointer: point,
      others,
    }
  }

  /** Double-click drills one level into whatever group is under the cursor
   * — the other half of `resolveCanvasSelection`'s Figma-style convention.
   * The *preceding* single click (part of the same double-click gesture)
   * already resolved and selected the top-level group at the click point;
   * this handler enters it (`selectionScope` becomes that group's id) and
   * re-resolves the same click point *at* the new scope, selecting whichever
   * immediate child — a shape, or a further-nested group — actually sits
   * under the cursor. Double-clicking that same spot again repeats the
   * process, digging one level deeper each time, until it bottoms out on a
   * leaf shape (which has nothing further to enter). */
  const handleCanvasDoubleClick = (event: ReactMouseEvent<HTMLCanvasElement>) => {
    event.preventDefault()
    if (mode === 'Playground') return
    const editor = editorRef.current
    const artboard = artboards[0]
    if (!editor || !artboard) return

    const point = toArtboardPoint(event.clientX, event.clientY)
    const hit = pickNodeAt(editor, artboard.id, point.x, point.y)
    if (!hit || !selection) return

    // Only makes sense to enter the node the preceding single click just
    // selected — if the double-click somehow resolves to something else
    // (stale selection, a fast double-click across two different shapes),
    // do nothing rather than guess. Entering isn't limited to `kind ===
    // 'Group'` — a `clipChildren` shape/image (the direct-child-clips-its-
    // own-children mechanism that replaced "wrap in a Group + mask" for the
    // Layers panel's drag-to-child gesture) or a masked `Group` are just as
    // enterable as a plain `Group`; what actually matters is whether there's
    // anything nested inside to drill into at all.
    const currentTarget = findNode(artboard, selection.nodeId)
    const hasChildren = currentTarget && artboard.nodes.some((n) => n.parent && idsEqual(n.parent, currentTarget.id))
    if (!currentTarget) return
    if (!hasChildren) {
      // No children to drill into — a leaf `Shape` enters path-editing mode
      // instead (see `PathEditOverlay`'s own doc comment); any other leaf
      // kind (`Text`/`Image`/`Bone`) has nothing this gesture does for it.
      if (currentTarget.kind === 'Shape') setPathEditNodeId(currentTarget.id)
      return
    }

    const { target } = resolveCanvasSelection(hit, artboard, selection.nodeId)
    setSelectionScope(selection.nodeId)
    selectSingle(artboard.id, target)
  }

  // Window-level, not canvas-level, so a drag/pan keeps tracking the cursor
  // even past the canvas edge — same reasoning `bones.js`'s cursor-follow
  // demo and `WasmPlayer`'s `track_pointer` listener already established for
  // pointer tracking elsewhere in this workspace. Re-subscribes whenever
  // `artboards` changes identity (i.e. on every committed edit) so the
  // closures below always see current node positions — cheap (two listeners),
  // and drags never span a moment where the array is mid-swap.
  useEffect(() => {
    // Snaps `candidate` to the artboard's own bounds/center and every other
    // node's edges/center (Canva/Figma-style alignment snapping) — shared by
    // the mousemove preview and the mouseup commit below so the drag always
    // *commits* whatever position it last *previewed*, snap included.
    function snapDrag(artboard: ArtboardSummary, nodeId: OpaqueId, candidate: { x: number; y: number }) {
      const node = artboard.nodes.find((n) => n.id.idx === nodeId.idx && n.id.version === nodeId.version)
      if (!node) return { translation: candidate, guideX: null, guideY: null }
      const targets = collectSnapTargets(artboard, nodeId)
      const thresholdWorld = SNAP_THRESHOLD_PX / cameraRef.current.zoom
      return snapTranslation(node, candidate, targets, thresholdWorld)
    }

    function handleWindowMouseMove(event: MouseEvent) {
      if (panRef.current) {
        const pan = panRef.current
        const dxScreen = event.clientX - pan.startClientX
        const dyScreen = event.clientY - pan.startClientY
        const zoom = cameraRef.current.zoom
        // Dragging the canvas right should reveal content that was to the
        // left (pan.x decreases); dragging down reveals content that was
        // above (pan.y increases — world +y is up, screen +y is down).
        setCamera((c) => ({ ...c, pan: { x: pan.startPan.x - dxScreen / zoom, y: pan.startPan.y + dyScreen / zoom } }))
        return
      }

      const drag = dragRef.current
      const editor = editorRef.current
      const artboard = drag && findArtboard(drag.artboardId)
      if (!drag || !editor || !artboard) return

      const point = toArtboardPoint(event.clientX, event.clientY)
      // `dx`/`dy` are a *world*-space delta (`toArtboardPoint` is artboard-
      // global) — `drag.origin` is the node's own *local* translation, so
      // this has to go through `localDelta` (using the node's own parent's
      // world transform) before it means anything as a local offset; adding
      // it directly, as this used to, silently under/over-shoots under any
      // scaled/rotated ancestor (a Lottie import's own outer precomp scale
      // included) — see `localDelta`'s own doc comment.
      const node = findNode(artboard, drag.nodeId)
      const local = localDelta(node?.parentWorldTransform ?? null, point.x - drag.startPointer.x, point.y - drag.startPointer.y)
      const snap = snapDrag(artboard, drag.nodeId, { x: drag.origin.x + local.x, y: drag.origin.y + local.y })
      previewNodeTranslation(editor, drag.artboardId, drag.nodeId, snap.translation.x, snap.translation.y)
      // Keeps the transform tool's selection outline/handles tracking the
      // node while it's being dragged — `previewNodeTranslation` only
      // mutates `WasmEditor`'s document directly, which `TransformHandles`'
      // `selectedNode` prop won't see until `commitNodeTranslation` lands a
      // new `DocumentSummary` on mouseup.
      setDragTranslation(snap.translation)
      setSnapGuides({ x: snap.guideX, y: snap.guideY })

      // A multi-selected group-drag: every other selected node rides along
      // by the *same snapped delta* the primary node just resolved to (not
      // its own independent snap — the whole group moves as one rigid body,
      // matching Figma's own group-drag behavior), rather than each member
      // snapping to targets independently and drifting apart.
      if (drag.others.length > 0) {
        const appliedDx = snap.translation.x - drag.origin.x
        const appliedDy = snap.translation.y - drag.origin.y
        for (const other of drag.others) {
          previewNodeTranslation(editor, drag.artboardId, other.nodeId, other.origin.x + appliedDx, other.origin.y + appliedDy)
        }
      }
    }

    function handleWindowMouseUp(event: MouseEvent) {
      if (panRef.current) {
        panRef.current = null
        return
      }

      const drag = dragRef.current
      dragRef.current = null
      setDragTranslation(null)
      // Unchanged state must stay the same object: this runs on *every*
      // mouseup anywhere (Paint strokes too), and a fresh `{ x: null, y: null }`
      // re-rendered the whole App each time — the lag after every brush stroke.
      setSnapGuides((guides) => (guides.x === null && guides.y === null ? guides : { x: null, y: null }))
      const editor = editorRef.current
      const artboard = drag && findArtboard(drag.artboardId)
      if (!drag || !editor || !artboard) return

      const point = toArtboardPoint(event.clientX, event.clientY)
      // Same world-to-local delta conversion as the mousemove preview above
      // — this commit has to resolve to the exact same local translation
      // the preview last showed, or the shape would visibly jump the
      // instant the drag ends.
      const node = findNode(artboard, drag.nodeId)
      const local = localDelta(node?.parentWorldTransform ?? null, point.x - drag.startPointer.x, point.y - drag.startPointer.y)
      const snap = snapDrag(artboard, drag.nodeId, { x: drag.origin.x + local.x, y: drag.origin.y + local.y })
      let summary = commitNodeTranslation(editor, drag.artboardId, drag.nodeId, drag.origin, snap.translation.x, snap.translation.y)

      // Each other multi-selected node commits its own share of the same
      // rigid-body move as a *separate* undo entry — not yet folded into one
      // combined undo step the way `import_svg`'s multi-node insert is (that
      // would need a batched multi-node commit primitive on the Rust side
      // this pass didn't add); a real, working group move today, at the cost
      // of needing a few extra undo presses to fully revert one, rather than
      // none at all.
      if (drag.others.length > 0) {
        const appliedDx = snap.translation.x - drag.origin.x
        const appliedDy = snap.translation.y - drag.origin.y
        for (const other of drag.others) {
          summary = commitNodeTranslation(
            editor,
            drag.artboardId,
            other.nodeId,
            other.origin,
            other.origin.x + appliedDx,
            other.origin.y + appliedDy,
          )
        }
      }
      setDoc(summary)
    }

    window.addEventListener('mousemove', handleWindowMouseMove)
    window.addEventListener('mouseup', handleWindowMouseUp)
    return () => {
      window.removeEventListener('mousemove', handleWindowMouseMove)
      window.removeEventListener('mouseup', handleWindowMouseUp)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [artboards])

  // Delete/Backspace deletes keyframes selected in the Timeline if there are
  // any, otherwise the currently selected node — one global listener rather
  // than a second one living inside `TimelinePanel`, since two independent
  // window-level `keydown` listeners can't `stopPropagation` each other off
  // (both are attached directly to `window`, not bubbling through a chain),
  // so whichever should "win" has to be a single shared decision. Skipped
  // while focus is in a text input (e.g. an Inspector `NumberField` or the
  // Timeline's duration field), so pressing Backspace to edit a number never
  // deletes anything instead. Re-subscribes whenever `selection`/
  // `keyframeSelection` change identity so the closure always sees current
  // state, same reasoning as the mouse listeners above.
  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      const target = event.target as HTMLElement | null
      if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) return
      // Playground is for actually playing a running game/Code-mode
      // program, not authoring — every shortcut below (undo/redo, copy/
      // paste, z-order, Delete, the bare '1'/'2'/'3' mode switch) would
      // otherwise fire on whatever key a game itself binds (a game's own
      // menu reading '1'/'2' would otherwise lose to this handler's bare '1',
      // silently switching the *editor* back to Design mode instead of
      // ever reaching the game's own `ctx.keysDown`). A running program's
      // keys reach it through a separate, always-on forwarder
      // (`heldKeysRef`/`setKeyDown`, below) regardless of this early
      // return — this only turns off the *editor's own* bindings.
      if (mode === 'Playground') return
      const key = event.key.toLowerCase()
      const command = event.ctrlKey || event.metaKey

      if (command && key === 'z') {
        event.preventDefault()
        if (event.shiftKey) handleRedo()
        else handleUndo()
        return
      }
      if (command && key === 'y') {
        event.preventDefault()
        handleRedo()
        return
      }
      if (command && key === 's') {
        event.preventDefault()
        handleSave()
        return
      }
      if (command && key === 'o') {
        event.preventDefault()
        handleOpen()
        return
      }
      if (command && key === 'g') {
        event.preventDefault()
        handleGroupSelection()
        return
      }
      if (command && key === 'c') {
        event.preventDefault()
        handleCopySelection()
        return
      }
      if (command && key === 'v') {
        event.preventDefault()
        handlePasteInPlace()
        return
      }
      // Layer z-order — the standard Figma/Illustrator convention:
      // `Ctrl+]`/`Ctrl+[` nudge one step, `Ctrl+Shift+]`/`Ctrl+Shift+[` jump
      // to the front/back. `event.key` for `]`/`[` doesn't shift with the
      // Shift key on a US layout (`{`/`}` live on the same physical keys but
      // report as `{`/`}`, not a shifted `]`/`[`) — checking `event.shiftKey`
      // explicitly rather than matching `{`/`}` directly is what makes this
      // work regardless of keyboard layout.
      if (command && (key === ']' || key === '}')) {
        event.preventDefault()
        if (event.shiftKey) handleBringNodeToFront()
        else handleMoveNodeForward()
        return
      }
      if (command && (key === '[' || key === '{')) {
        event.preventDefault()
        if (event.shiftKey) handleSendNodeToBack()
        else handleSendNodeBackward()
        return
      }
      if (key === '1') {
        setMode('Design')
        return
      }
      if (key === '2') {
        setMode('Design')
        setBottomDockTab('Animate')
        setBottomDockOpen(true)
        return
      }
      if (key === '3') {
        setMode('Design')
        setBottomDockTab('StateMachine')
        setBottomDockOpen(true)
        return
      }

      if (event.key === 'Escape' && isPathEditing) {
        event.preventDefault()
        setPathEditNodeId(null)
        return
      }

      if (event.key !== 'Delete' && event.key !== 'Backspace') return

      // While path-editing, Delete/Backspace is `PathEditOverlay`'s own
      // "delete the selected anchor" action (a separate `keydown` listener
      // it registers itself) — falling through to `handleDeleteSelected`
      // here too would delete the *entire node* being reshaped out from
      // under it on the same keypress. No anchor selected simply means
      // Delete does nothing while path-editing, rather than unexpectedly
      // nuking the whole shape.
      if (isPathEditing) return

      if (keyframeSelection.length > 0) {
        event.preventDefault()
        keyframeSelection.forEach(({ property, time }) => handleRemoveKeyframe(property, time))
        setKeyframeSelection([])
        return
      }
      event.preventDefault()
      handleDeleteSelected()
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selection, multiSelectedIds, keyframeSelection, pathEditNodeId, mode])

  // Forwards every real key press/release into the live preview's `Blocks`-
  // mode input surface (a Programmable State's "key [ ] pressed?" sensing
  // reporter / "when key [ ] pressed" hat) — deliberately a *separate*
  // listener from `handleKeyDown` above, not folded into it: that handler
  // returns early after handling any editor shortcut it recognizes (undo,
  // save, layer z-order, ...), and forwarding needs to happen regardless of
  // which (if any) shortcut branch a given key also happens to hit. Skips
  // while focus is in a text field, same "don't leak keystrokes from typing"
  // reasoning `handleKeyDown` itself applies. Inert unless a Blocks script
  // actually reads the key — see `setKeyDown`'s own doc comment.
  //
  // `KeyboardEvent.key` doesn't spell keys the way an author naturally
  // types them into a "when key [ ] pressed" block's plain text field —
  // notably `" "` (a literal space character) for the spacebar, and
  // `"ArrowUp"`/`"ArrowDown"`/`"ArrowLeft"`/`"ArrowRight"` for the arrow
  // keys. `normalizeKeyName` maps those to the word an author would
  // actually write ("space", "up"/"down"/"left"/"right") before forwarding;
  // everything else just lowercases, so a plain letter/digit key still
  // matches a block field typed in either case.
  useEffect(() => {
    function forward(event: KeyboardEvent, pressed: boolean) {
      const target = event.target as HTMLElement | null
      if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) return
      const key = normalizeKeyName(event.key)
      if (pressed) heldKeysRef.current.add(key)
      else heldKeysRef.current.delete(key)
      if (!editorRef.current) return
      setKeyDown(editorRef.current, key, pressed)
    }
    const onKeyDown = (event: KeyboardEvent) => forward(event, true)
    const onKeyUp = (event: KeyboardEvent) => forward(event, false)
    window.addEventListener('keydown', onKeyDown)
    window.addEventListener('keyup', onKeyUp)
    return () => {
      window.removeEventListener('keydown', onKeyDown)
      window.removeEventListener('keyup', onKeyUp)
    }
  }, [])

  // Solid style: the canvas viewport is the rectangle *between* the docked
  // panes (rail, left dock, right dock, toolbar, bottom dock) instead of the
  // whole window under them. Every canvas-relative overlay lives inside the
  // same wrapper (`viewportStyle` below), so its absolute positioning stays
  // canvas-relative automatically, and the canvas's own `ResizeObserver`
  // picks up the new size — nothing downstream had to learn about insets.
  // Glass keeps the original full-window viewport (all insets zero).
  const dockedWidth = (region: 'left' | 'right') => {
    if (mode === 'Playground') return 0
    const group = dock.groups[region]
    const grouped = group ? groupMemberKeys(group) : []
    const solo = (Object.keys(dock.panels) as PanelKey[])
      .filter((key) => panelVisible[key] && !grouped.includes(key) && dock.panels[key].region === region)
      .map((key) => dock.panels[key].width)
    return Math.max(group ? group.width : 0, ...solo, 0)
  }
  const viewportInsets =
    uiStyle === 'solid'
      ? {
          left: (mode === 'Playground' ? 0 : RAIL_WIDTH) + dockedWidth('left'),
          right: dockedWidth('right'),
          top: metrics.topClearance,
          bottom: bottomDockClearance,
        }
      : mode === 'Playground'
        ? // The game starts below the toolbar, so nothing it frames hides
          // under it (the camera fits the space that's actually visible).
          // An open Console takes its strip of the window too: the game
          // shrinks to fit above it instead of hiding behind it.
          { left: 0, right: 0, top: metrics.topClearance, bottom: playConsoleOpen ? PLAY_CONSOLE_HEIGHT + BOTTOM_TAB_STRIP_HEIGHT : 0 }
        : { left: 0, right: 0, top: 0, bottom: 0 }
  const viewportFillsWindow = uiStyle !== 'solid'

  return (
    <LayoutMetricsContext.Provider value={metrics}>
    <div className="relative h-full w-full overflow-hidden">
      {/* `CanvasArea` and its overlays stay permanently mounted regardless
          of `mode` — including Paint mode, which used to conditionally
          unmount this whole branch entirely. That broke the live artboard:
          `CanvasArea`'s `<canvas>` is where `WasmEditor`'s wgpu surface was
          originally created and bound; unmounting it destroys that DOM
          node, and remounting later creates a *new*, different canvas
          element the wgpu surface was never rebound to — a real,
          hand-verified bug (returning from Paint mode showed a blank dark
          canvas, no grid, no objects, nothing wrong with the document
          itself, just nothing drawing into the new orphaned canvas).
          `PaintAssetEditor` now renders as a *later sibling* instead
          (below), simply covering this with its own opaque `bg-canvas` —
          same "swap by painting over, not by unmounting" fix as the
          paper.js unmount-order bug just above it. */}
      <div className="absolute overflow-hidden" style={viewportInsets}>
      <div
        className="absolute inset-0"
        onDragOver={(e) => {
          if (e.dataTransfer.types.includes(ASSET_DRAG_MIME)) {
            e.preventDefault()
            e.dataTransfer.dropEffect = 'copy'
          }
        }}
        onDrop={(e) => {
          const raw = e.dataTransfer.getData(ASSET_DRAG_MIME)
          if (!raw || !canvasRef.current) return
          e.preventDefault()
          const rect = canvasRef.current.getBoundingClientRect()
          handlePlaceAssetAt(JSON.parse(raw) as DraggedAsset, { x: e.clientX - rect.left, y: e.clientY - rect.top })
        }}
      >
        <CanvasArea canvasRef={canvasRef} onMouseDown={handleCanvasMouseDown} onDoubleClick={handleCanvasDoubleClick} />
        {mode === 'Design' && sceneHas3D && (
          // Unity's 2D / 3D scene-view switch.
          <div className="pointer-events-auto absolute left-1/2 top-14 z-20 flex -translate-x-1/2 items-center gap-0.5 rounded-md border border-panel-border bg-panel p-0.5 text-[11px] backdrop-blur-panel">
            {(['2D', '3D'] as const).map((v) => (
              <button
                key={v}
                onClick={() => setSceneView(v)}
                title={v === '3D' ? 'Scene view: right-drag orbit, middle-drag pan, wheel zoom, F frame, click to select, drag the arrows to move' : 'Edit the 2D layer (HUD, sprites)'}
                className={`rounded px-2.5 py-1 font-medium ${sceneView === v ? 'bg-accent text-white' : 'text-ink-dim hover:text-ink'}`}
              >
                {v}
              </button>
            ))}
            {view3dActive && activeArtboard && (
              <>
                <span className="mx-1 h-4 w-px bg-panel-border" />
                {/* The amber camera with the frame in front of it is what
                    Playground looks through; the 2D layer (HUD) is drawn on
                    that frame. These place it without a rotate gizmo. */}
                <button
                  onClick={() => {
                    const editor = editorRef.current
                    if (!editor) return
                    const result = editor.view3d_align_camera(activeArtboard.id, selection?.nodeId ?? null)
                    if (result) setDoc(result as DocumentSummary)
                    else window.alert('This scene has no Camera 3D yet: add one from + → 3D → Camera 3D.')
                  }}
                  title="Move the Playground camera (the amber one, or the selected Camera 3D) to exactly where you're looking from now. Undoable."
                  className="rounded px-2 py-1 text-ink-dim hover:bg-overlay/10 hover:text-ink"
                >
                  Align camera to view
                </button>
                <button
                  onClick={() => {
                    const editor = editorRef.current
                    if (editor && !editor.view3d_look_through(activeArtboard.id, selection?.nodeId ?? null)) window.alert('This scene has no Camera 3D yet: add one from + → 3D → Camera 3D.')
                  }}
                  title="Look through the Playground camera (the amber one, or the selected Camera 3D): see exactly what Playground will show."
                  className="rounded px-2 py-1 text-ink-dim hover:bg-overlay/10 hover:text-ink"
                >
                  View through camera
                </button>
              </>
            )}
          </div>
        )}
      </div>
      {/* Manual zoom widget — bottom-*center* of the canvas, not a literal
          corner: `Layers`/`Inspector` are dockable to *either* screen edge
          (`useDockLayout`), so a fixed left/right corner placement silently
          sat directly underneath whichever side panel the user currently
          has docked there (invisible, not just visually crowded — the panel
          is a plain later sibling in the DOM and painted over it). Bottom-
          center is clear of both side docks regardless of which edge either
          one is currently snapped to, and only needs to clear whatever
          bottom dock (Timeline/State Machine) is showing, via the same
          `bottomDockClearance` those docks size against. Gesture zoom
          (ctrl+scroll/pinch) already worked via `handleWheel`; this is the
          on-screen control for it, reusing the exact same
          `zoomAtCanvasPoint`/`camera.zoom` state. */}
      {mode !== 'Playground' && (
      <FloatingPanel
        className="pointer-events-auto absolute flex items-center gap-1 px-1 py-1"
        style={{ left: '50%', transform: 'translateX(-50%)', bottom: viewportFillsWindow ? bottomDockClearance : 12 }}
      >
        <button onClick={handleZoomOut} title="Zoom out" className="rounded-md p-1 hover:bg-overlay/5 hover:text-ink">
          <ZoomOutIcon />
        </button>
        <button
          onClick={handleResetZoom}
          title="Reset zoom to 100%"
          className="w-12 rounded-md py-1 text-center text-[11px] tabular-nums hover:bg-overlay/5 hover:text-ink"
        >
          {Math.round(camera.zoom * 100)}%
        </button>
        <button onClick={handleZoomIn} title="Zoom in" className="rounded-md p-1 hover:bg-overlay/5 hover:text-ink">
          <ZoomInIcon />
        </button>
        <span className="mx-1 h-4 w-px bg-overlay/15" />
        <select
          value={previewScreen ? `${previewScreen.width}x${previewScreen.height}` : ''}
          onChange={(e) => {
            const preset = SCREEN_PRESETS.find((p) => `${p.width}x${p.height}` === e.target.value) ?? null
            setPreviewScreenState(preset ? { width: preset.width, height: preset.height } : null)
          }}
          title="Preview the layout on another screen size: UI Layouts, Anchors and Show When react (and a Responsive scene's camera frame resizes). Playground uses the real window."
          className="rounded-md bg-transparent px-1 py-1 text-[11px] text-ink-dim hover:bg-overlay/5 hover:text-ink"
        >
          {/* Native option lists don't inherit the dark theme: colour them. */}
          <option value="" className="bg-solid-panel text-ink">
            Design size
          </option>
          {SCREEN_PRESETS.map((p) => (
            <option key={p.label} value={`${p.width}x${p.height}`} className="bg-solid-panel text-ink">
              {p.label} · {p.width}×{p.height}
            </option>
          ))}
        </select>
      </FloatingPanel>
      )}
      {cameraFrames.map((frame) => (
        <div
          key={frame.key}
          // Light mode: a brighter, more orange frame (amber reads muddy on
          // the pale canvas).
          className={`pointer-events-none absolute rounded-[1px] ${theme === 'light' ? (frame.isMain ? 'border-2 border-orange-500' : 'border border-dashed border-orange-500/60') : frame.isMain ? 'border border-amber-400/80' : 'border border-dashed border-amber-400/35'}`}
          style={{ left: frame.left, top: frame.top, width: frame.width, height: frame.height }}
        >
          <span className={`absolute -top-4 left-0 whitespace-nowrap text-[10px] ${theme === 'light' ? 'font-semibold text-orange-600' : 'text-amber-400/80'}`}>
            {frame.name}
            {frame.isMain ? ' · main' : ''}
          </span>
        </div>
      ))}
      {colliderGizmos.length > 0 && (
        <svg className="pointer-events-none absolute inset-0 h-full w-full overflow-visible">
          {colliderGizmos.map((g) => (
            <path
              key={g.key}
              d={g.d}
              fill="none"
              stroke="rgb(74 222 128)"
              strokeOpacity={g.selected ? 0.95 : 0.55}
              strokeWidth={g.selected ? 1.5 : 1}
              strokeDasharray={g.trigger ? '4 3' : undefined}
            />
          ))}
        </svg>
      )}
      {lightGizmos.length > 0 && (
        <svg className="pointer-events-none absolute inset-0 h-full w-full overflow-visible">
          {lightGizmos.map((g) =>
            g.paths.map((p, i) => (
              <path
                key={`${g.key}-${i}`}
                d={p.d}
                fill="none"
                stroke="rgb(250 204 21)"
                strokeOpacity={g.selected ? 0.9 : 0.35}
                strokeWidth={g.selected ? 1.5 : 1}
                strokeDasharray={p.dashed ? '4 3' : undefined}
              />
            )),
          )}
        </svg>
      )}
      {lightGizmos.map((g) => (
        <div
          key={`${g.key}-icon`}
          className={`pointer-events-none absolute flex items-center justify-center rounded-full border ${g.selected ? 'border-yellow-300' : 'border-yellow-300/50'} bg-black/30`}
          style={{ left: g.center.x - g.iconSize / 2, top: g.center.y - g.iconSize / 2, width: g.iconSize, height: g.iconSize, color: g.kind === 'Trail' ? 'rgb(147 197 253)' : g.tint }}
        >
          {g.kind === 'Trail' ? <TrailKindIcon /> : <LightKindIcon />}
        </div>
      ))}
      {cameraFrames.map((frame) => (
        <div
          key={`${frame.key}-icon`}
          className={`pointer-events-none absolute flex items-center justify-center rounded-md border ${theme === 'light' ? 'border-orange-500 bg-orange-100 text-orange-600 shadow-sm' : 'border-amber-400/80 bg-amber-400/15 text-amber-500'}`}
          style={{ left: frame.iconLeft, top: frame.iconTop, width: frame.iconSize, height: frame.iconSize }}
        >
          <CameraKindIcon />
        </div>
      ))}
      {multiSelectionScreenBox ? (
        <div
          className="pointer-events-none absolute rounded-sm border border-dashed border-accent/80"
          style={{
            left: multiSelectionScreenBox.left,
            top: multiSelectionScreenBox.top,
            width: multiSelectionScreenBox.width,
            height: multiSelectionScreenBox.height,
          }}
        />
      ) : isPathEditing ? (
        <PathEditOverlay
          containerRef={transformHandlesContainerRef}
          selectedNode={transformHandlesNode}
          camera={camera}
          canvasSize={canvasSize}
          onPreviewPath={handlePreviewPath}
          onCommitPath={handleCommitPath}
        />
      ) : transformHandlesNode?.kind === 'Tilemap' && editorRef.current && selection ? (
        <TilemapPaintOverlay
          containerRef={transformHandlesContainerRef}
          selectedNode={transformHandlesNode}
          selectedTile={selectedTile}
          camera={camera}
          canvasSize={canvasSize}
          onSetTile={handleSetTile}
        />
      ) : (
        <>
          {mode === 'Design' && activeArtboard && !view3dActive && (
            <BoneGizmos
              nodes={activeArtboard.nodes}
              selectedId={selection?.nodeId ?? null}
              camera={camera}
              canvasSize={canvasSize}
              toWorld={toArtboardPoint}
              onSelect={(bone) => setSelection({ artboardId: activeArtboard.id, nodeId: bone })}
              onPreviewAim={handleBoneAim}
              onPreviewIk={handleBoneIk}
              onCommit={handleBoneCommit}
            />
          )}
          <TransformHandles
            containerRef={transformHandlesContainerRef}
            selectedNode={transformHandlesNode}
            camera={camera}
            canvasSize={canvasSize}
            onPreviewRotation={handlePreviewRotation}
            onCommitRotation={handleCommitRotation}
            onPreviewScale={handlePreviewScale}
            onCommitScale={handleCommitScale}
            onPreviewAnchor={handlePreviewAnchor}
            onCommitAnchor={handleCommitAnchor}
          />
          <GradientHandle
            containerRef={transformHandlesContainerRef}
            selectedNode={transformHandlesNode}
            camera={camera}
            canvasSize={canvasSize}
            onSetGradient={handleSetGradient}
          />
        </>
      )}
      <SnapGuides guideX={snapGuides.x} guideY={snapGuides.y} camera={camera} canvasSize={canvasSize} />
      </div>
      {/* Permanently mounted, like `CanvasArea` above — not conditionally
          rendered on `mode === 'Paint'` the way an earlier version of this
          was. Two real reasons, not just one: (1) the same unmount/remount
          issue `CanvasArea` itself hit (its own live paper.js canvas/
          project and Redux store would be torn down and rebuilt from
          scratch on every mode switch); (2) the user's own explicit ask —
          switching away and back should keep whatever was being painted,
          not reset to blank. `visible` just toggles its own CSS
          visibility; the whole scratch-paint subtree keeps running
          underneath regardless (idle — paper.js doesn't have a continuous
          render loop, it only redraws on an actual change). */}
      {mode === 'Code' && (
        // Opaque and above the canvas overlays (camera frames, zoom widget) —
        // the scene isn't drawn in Code mode.
        <div
          className="pointer-events-auto absolute z-40"
          style={{ left: metrics.leftDockOffset, right: 0, top: metrics.topClearance - metrics.edgeMargin, bottom: 0, padding: metrics.edgeMargin, background: theme === 'light' ? '#eceef3' : '#131318' }}
        >
          <CodeWorkspace
            editor={editorRef.current}
            doc={doc}
            nodes={artboards[0]?.nodes ?? []}
            theme={theme}
            request={codeRequest}
            onDocumentChange={setDoc}
            onRun={() => setMode('Playground')}
          />
        </div>
      )}
      <PaintAssetEditor
        visible={mode === 'Paint'}
        actionBarTarget={paintActionBarTarget}
        imageRef={paintImageRef}
        session={paintSession}
        sessionKey={paintSessionKey}
        onSessionChange={setPaintSession}
        toolsTarget={mode === 'Paint' ? paintToolsTarget : null}
        name={paintDrawingName}
        onNameChange={setPaintDrawingName}
        onClose={() => setMode('Design')}
      />
      {mode !== 'Playground' && (
        <IconRail
          paintToolsTarget={mode === 'Paint' ? setPaintToolsTarget : undefined}
          componentsOpen={panelVisible.components}
          onToggleComponents={() => setPanelVisible((v) => ({ ...v, components: !v.components }))}
          effectsOpen={panelVisible.effects}
          onToggleEffects={() => setPanelVisible((v) => ({ ...v, effects: !v.effects }))}
          layersOpen={panelVisible.layers}
          onToggleLayers={() => setPanelVisible((v) => ({ ...v, layers: !v.layers }))}
          inspectorOpen={panelVisible.inspector}
          onToggleInspector={() => setPanelVisible((v) => ({ ...v, inspector: !v.inspector }))}
          resourcesOpen={mode === 'Design' && bottomDockOpen && bottomDockTab === 'Resources'}
          onToggleResources={() => {
            setMode('Design')
            if (bottomDockOpen && bottomDockTab === 'Resources') setBottomDockOpen(false)
            else {
              setBottomDockTab('Resources')
              setBottomDockOpen(true)
            }
          }}
          tilePaletteOpen={panelVisible.tilePalette}
          onToggleTilePalette={() => setPanelVisible((v) => ({ ...v, tilePalette: !v.tilePalette }))}
          timelineOpen={mode === 'Design' && bottomDockOpen && bottomDockTab === 'Animate'}
          onToggleTimeline={() => {
            setMode('Design')
            if (bottomDockOpen && bottomDockTab === 'Animate') setBottomDockOpen(false)
            else {
              setBottomDockTab('Animate')
              setBottomDockOpen(true)
            }
          }}
          stateMachineOpen={mode === 'Design' && bottomDockOpen && bottomDockTab === 'StateMachine'}
          onToggleStateMachine={() => {
            setMode('Design')
            if (bottomDockOpen && bottomDockTab === 'StateMachine') setBottomDockOpen(false)
            else {
              setBottomDockTab('StateMachine')
              setBottomDockOpen(true)
            }
          }}
          theme={theme}
          onToggleTheme={() => setTheme((t) => (t === 'dark' ? 'light' : 'dark'))}
          uiStyle={uiStyle}
          onToggleUiStyle={() => setUiStyle((v) => (v === 'solid' ? 'glass' : 'solid'))}
        />
      )}
      <TitleBar theme={theme} onToggleTheme={() => setTheme((t) => (t === 'dark' ? 'light' : 'dark'))} />
      {project.overlay}
      <TopToolbar
        mode={mode}
        onModeChange={setMode}
        onUndo={handleUndo}
        onRedo={handleRedo}
        fileMenu={project.fileMenu}
        onSave={handleSave}
        onExportAnimation={handleExportAnimation}
        onExportGame={() => void handleExportGame()}
        onExportDesktopGame={() => void handleExportGame('desktop')}
        onAddShape={handleAddShape}
        onAddParticleSystem={handleAddParticleSystem}
        onImportSvg={handleImportSvg}
        onImportImage={handleImportImage}
        onLoadDemo={handleLoadDemo}
        onLoadGeometryDashDemo={handleLoadGeometryDashDemo}
        onLoadNinjaDemo={handleLoadNinjaDemo}
        onLoadFpsDemo={handleLoadFpsDemo}
            onLoadStressDemo={handleLoadStressDemo}
            onLoadRigDemo={handleLoadRigDemo}
            onLoadDungeonDemo={handleLoadDungeonDemo}
            onLoadMatch3Demo={handleLoadMatch3Demo}
            onLoadLiquidDemo={handleLoadLiquidDemo}
            onLoadTortureDemo={handleLoadTortureDemo}
            onLoad3DDemo={handleLoad3DDemo}
            onLoadUiDemo={handleLoadUiDemo}
            onLoadResponsiveDemo={handleLoadResponsiveDemo}
            onLoadHtmlDemo={handleLoadHtmlDemo}
            onLoadPhysics3DDemo={handleLoadPhysics3DDemo}
            onLoadTreesDemo={handleLoadTreesDemo}
            onLoadLeerDemo={handleLoadLeerDemo}
            onLoadGooDemo={handleLoadGooDemo}
            onLoadGooShapesDemo={handleLoadGooShapesDemo}
            onLoadClothDemo={handleLoadClothDemo}
            onLoadLanDemo={handleLoadLanDemo}
            onLoadBoomDemo={handleLoadBoomDemo}
            onLoadVoxelDemo={handleLoadVoxelDemo}
            onLoadShatterDemo={handleLoadShatterDemo}
            onLoadTacticsDemo={handleLoadTacticsDemo}
            onLoadPhysicsDemo={handleLoadPhysicsDemo}
        onAddText={handleAddText}
        onAddTilemap={handleAddTilemap}
        onAddCamera={handleAddCamera}
        onAddLight2D={handleAddLight2D}
        onAddNode3D={handleAddNode3D}
        onAddUi={handleAddUi}
        onImportModel={handleImportModel}
        onImportAudio={handleImportAudio}
        onAddTrail={handleAddTrail}
        sceneSwitcher={
          <SceneSwitcher
            scenes={allArtboards}
            activeScene={activeArtboard}
            onSelect={handleSelectScene}
            onAdd={handleAddScene}
            onRename={handleRenameScene}
            onDelete={handleDeleteScene}
          />
        }
        onAddBone={handleAddBone}
        onPaintActionBarTargetChange={setPaintActionBarTarget}
        onSaveDrawing={handleSaveDrawingAsResource}
        onNewDrawing={handleNewDrawing}
      />
      {notice && (
        <div className="pointer-events-none fixed left-1/2 top-16 z-50 -translate-x-1/2 rounded-lg border border-panel-border bg-panel px-3 py-1.5 text-xs text-ink shadow-2xl backdrop-blur-panel">
          {notice}
        </div>
      )}

      {mode !== 'Playground' && mode !== 'Paint' && mode !== 'Code' && (() => {
        const panelContent: Record<PanelKey, { title: string; content: React.ReactNode }> = {
          layers: {
            title: 'Layers',
            content: (
              <LayersPanel
                artboards={artboards}
                selectedNodeIds={multiSelectedIds}
                onSelectNode={(artboardId, nodeId, event) =>
                  event.ctrlKey || event.metaKey || event.shiftKey ? toggleMultiSelect(artboardId, nodeId) : selectSingle(artboardId, nodeId)
                }
                onRenameNode={handleRenameNode}
                onReparentNode={handleReparentNode}
                onReparentNodeBefore={handleReparentNodeBefore}
                onSetGroupMask={handleSetGroupMask}
                onSetClipChildren={handleSetClipChildrenFor}
                onGroupSelection={handleGroupSelection}
                onAddChildShape={handleAddChildShape}
                layerMaskEditing={layerMaskEditing}
              />
            ),
          },
          inspector: {
            title: 'Inspector',
            content: (
              <>
              {selectedNode && multiSelectedIds.length <= 1 && (
                <PrefabBar
                  node={selectedNode}
                  prefabs={doc?.prefabs ?? []}
                  onCreate={handleCreatePrefab}
                  onApply={() => runPrefabAction(applyPrefabInstance)}
                  onRevert={() => runPrefabAction(revertPrefabInstance)}
                  onUnpack={() => runPrefabAction(unpackPrefabInstance)}
                  onSelectRoot={handleSelectPrefabRoot}
                />
              )}
              <InspectorPanel
                node={selectedNode}
                components={doc?.components ?? []}
                images={doc?.images ?? []}
                editor={editorRef.current}
                selectedAsset={(selectedAssetId && doc?.images.find((image) => idsEqual(image.id, selectedAssetId))) || null}
                onSetElementId={handleSetElementId}
                onSetClasses={handleSetClasses}
                onSetClipChildren={handleSetClipChildren}
                onSetShapeImage={handleSetShapeImage}
                onSetStroke={handleSetStroke}
                editingPath={!!selection && !!pathEditNodeId && idsEqual(pathEditNodeId, selection.nodeId)}
                onEditPath={(editing) => setPathEditNodeId(editing && selection ? selection.nodeId : null)}
                onSetParticleSystem={handleSetParticleSystem}
                onReplayParticleSystem={handleReplayParticleSystem}
                onSetTranslation={handleSetTranslation}
                onSetRotation={handleSetRotation}
                onSetScale={handleSetScale}
                onSetAnchor={handleSetAnchor}
                onSetBoneLength={handleSetBoneLength}
                onSetBlendMode={handleSetBlendMode}
                onSetFillColor={handleSetFillColor}
                onSetGradient={handleSetGradient}
                onSetTextContent={handleSetTextContent}
                onSetTextSize={handleSetTextSize}
                onSetTextFontFamily={handleSetTextFontFamily}
                fonts={doc?.fonts ?? []}
                onSetInstanceVariant={handleSetInstanceVariant}
                onSetInstanceFillColorOverride={handleSetInstanceFillColorOverride}
                onSetImageProcessing={handleSetImageProcessing}
                onSolveIk={handleSolveIk}
                onMakeRagdoll={handleMakeRagdoll}
                onAddIkTarget={handleAddIkTarget}
                onBindSkin={handleBindSkin}
                materials={doc?.materials ?? []}
                onSetNode3D={handleSetNode3D}
                models={doc?.models ?? []}
                sounds={doc?.audio ?? []}
                textFiles={doc?.textFiles ?? []}
                onOpenFile={handleOpenFile}
                onDocumentChange={setDoc}
                properties={doc?.properties ?? []}
                sceneNames={allArtboards.map((a) => a.name)}
                onEditMaterial={(id) => {
                  setShaderMaterialId(id)
                  setBottomDockTab('Shader')
                  setBottomDockOpen(true)
                }}
                onDelete={handleDeleteSelected}
                artboard={artboards[0] ?? null}
                onSetArtboardSize={handleSetArtboardSize}
                onSetArtboardGravity={handleSetArtboardGravity}
                onSetArtboardGravity3D={handleSetArtboardGravity3D}
                onSetArtboardResolutionScale={handleSetArtboardResolutionScale}
                onSetArtboardScreen={handleSetArtboardScreen}
                onSetMainCamera={handleSetMainCamera}
                motionPathGuideOptions={motionPathGuideOptions}
                onSetMotionPathGuide={handleSetMotionPathGuide}
                onRemoveMotionPathTrack={handleRemoveMotionPathTrack}
                onResizeTilemap={handleResizeTilemap}
                onSetTilemapCellSize={handleSetTilemapCellSize}
                scripts={doc?.scripts ?? []}
                onSetCamera={handleSetCamera}
                onSetLight2D={handleSetLight2D}
                onSetTrail={handleSetTrail}
                onSetShadowCaster={handleSetShadowCaster}
                onSetNodeUnlit={handleSetNodeUnlit}
                onSetActiveCamera={handleSetActiveCamera}
                onSetGroupYSort={handleSetGroupYSort}
                onSetNodeZIndex={handleSetNodeZIndex}
                onSetNodeBehaviors={handleSetNodeBehaviors}
                onAddScript={handleAddScript}
                onSetScriptFields={handleSetScriptFields}
              />
              </>
            ),
          },
          resources: {
            title: 'Resources',
            content: (
              <ResourcesPanel
                editor={editorRef.current}
                sounds={doc?.audio ?? []}
                onImportAudio={handleImportAudio}
                onDeleteAudio={handleDeleteAudio}
                images={doc?.images ?? []}
                spriteGroups={doc?.spriteGroups ?? []}
                components={doc?.components ?? []}
                selectedAssetId={selectedAssetId}
                onSelectAsset={handleSelectAsset}
                onImportImage={handleImportImage}
                onPlaceComponent={handleAddInstance}
                prefabs={doc?.prefabs ?? []}
                scenes={allArtboards.map((a) => ({ id: a.id, name: a.name }))}
                projectName={project.fileMenu.projectName}
                projectFolder={project.projectFolder}
                onPlacePrefab={handlePlacePrefab}
                onRenamePrefab={(id, name) => editorRef.current && setDoc(renamePrefab(editorRef.current, id, name))}
                onDeletePrefab={(id) => editorRef.current && setDoc(deletePrefab(editorRef.current, id))}
                onOpenScene={handleSelectScene}
                onShowInExplorer={project.fileMenu.onShowInExplorer}
              />
            ),
          },
          tilePalette: {
            title: 'Tile Palette',
            content: (
              <TilePalettePanel editor={editorRef.current} tilemap={selectedNode?.tilemap ?? null} selectedTile={selectedTile} onSelectTile={setSelectedTile} />
            ),
          },
          components: {
            title: 'Components',
            content: (
              <ComponentsPanel
                components={doc?.components ?? []}
                selectedNode={selectedNode}
                onAddComponent={handleAddComponent}
                onDeleteComponent={handleDeleteComponent}
                onSetDefaultFillColor={handleSetComponentDefaultFillColor}
                onAddVariant={handleAddComponentVariant}
                onAddInstance={handleAddInstance}
              />
            ),
          },
          effects: {
            title: 'Effects',
            content: <EffectsPanel layers={artboards[0]?.postEffects ?? []} onChange={handleSetPostEffects} />,
          },
        }

        const groupedKeys: PanelKey[] = (['left', 'right'] as const).flatMap((region) => (dock.groups[region] ? groupMemberKeys(dock.groups[region]!) : []))
        // Resources lives in the bottom dock now (see `bottomDockTab`).
        const soloKeys = (['layers', 'inspector', 'tilePalette', 'components', 'effects'] as const).filter((key) => !groupedKeys.includes(key) && panelVisible[key])

        return (
          <>
            {(['left', 'right'] as const).map((region) =>
              dock.groups[region] ? (
                <DockGroup
                  key={region}
                  group={dock.groups[region]!}
                  containerRef={dock.groupContainerRefs[region]}
                  paneRefs={dock.groupPaneRefs[region]}
                  onActiveTabChange={(key) => dock.setActiveTab(region, key)}
                  onClosePanel={(key) => {
                    dock.closePanel(key)
                    setPanelVisible((v) => ({ ...v, [key]: false }))
                  }}
                  headerDragHandlers={dock.groupedHeaderDragHandlers}
                  makeSplitDividerHandlers={(dividerIndex) => dock.makeSplitDividerHandlers(region, dividerIndex)}
                  resizeHandlerProps={dock.makeGroupResizeHandlers(region)}
                  bottomClearance={bottomDockClearance}
                  panelContent={panelContent}
                />
              ) : null,
            )}
            {soloKeys.map((key) => (
              <DockablePanel
                key={key}
                ref={dock.panelRefs[key]}
                title={panelContent[key].title}
                region={dock.panels[key].region}
                position={dock.panels[key].position}
                width={dock.panels[key].width}
                dragHandleProps={dock.dragHandlers[key]}
                resizeHandleProps={dock.resizeHandlers[key]}
                bottomClearance={bottomDockClearance}
                onClose={() => setPanelVisible((v) => ({ ...v, [key]: false }))}
                className="p-2"
              >
                {panelContent[key].content}
              </DockablePanel>
            ))}
            {dock.dropPreview && (
              <div
                className={`pointer-events-none absolute z-50 rounded-sm border-2 ${dock.dropPreview.mode === 'tabs' ? 'border-accent bg-accent/25' : 'border-accent/70 bg-accent/10'}`}
                style={{ left: dock.dropPreview.rect.left, top: dock.dropPreview.rect.top, width: dock.dropPreview.rect.width, height: dock.dropPreview.rect.height }}
              />
            )}
          </>
        )
      })()}

      {mode === 'Design' && bottomDockOpen && (
        <div
          className="pointer-events-auto absolute z-20 flex items-center gap-0.5 rounded-md border border-panel-border bg-panel px-1 py-0.5 backdrop-blur-panel"
          style={{ left: metrics.leftDockOffset, bottom: bottomDockHeight + metrics.edgeMargin, height: BOTTOM_TAB_STRIP_HEIGHT }}
        >
          <button
            onClick={() => setBottomDockTab('Animate')}
            className={`rounded px-2 py-1 text-[11px] font-medium uppercase tracking-wide ${
              bottomDockTab === 'Animate' ? 'bg-overlay/10 text-ink' : 'text-ink-dim hover:text-ink'
            }`}
          >
            Animate
          </button>
          <button
            onClick={() => setBottomDockTab('StateMachine')}
            className={`rounded px-2 py-1 text-[11px] font-medium uppercase tracking-wide ${
              bottomDockTab === 'StateMachine' ? 'bg-overlay/10 text-ink' : 'text-ink-dim hover:text-ink'
            }`}
          >
            State Machine
          </button>
          <button
            onClick={() => setBottomDockTab('Resources')}
            className={`rounded px-2 py-1 text-[11px] font-medium uppercase tracking-wide ${
              bottomDockTab === 'Resources' ? 'bg-overlay/10 text-ink' : 'text-ink-dim hover:text-ink'
            }`}
          >
            Resources
          </button>
          <button
            onClick={() => setBottomDockTab('Shader')}
            className={`rounded px-2 py-1 text-[11px] font-medium uppercase tracking-wide ${
              bottomDockTab === 'Shader' ? 'bg-overlay/10 text-ink' : 'text-ink-dim hover:text-ink'
            }`}
          >
            Shader
          </button>
          <button
            onClick={() => setBottomDockTab('Console')}
            className={`relative rounded px-2 py-1 text-[11px] font-medium uppercase tracking-wide ${
              bottomDockTab === 'Console' ? 'bg-overlay/10 text-ink' : 'text-ink-dim hover:text-ink'
            }`}
          >
            Console
            <ConsoleBadge />
          </button>
          <button onClick={() => setBottomDockOpen(false)} title="Close" className="ml-0.5 rounded p-1 text-ink-dim hover:bg-overlay/10 hover:text-ink">
            <CloseIcon />
          </button>
        </div>
      )}

      {effectiveBottomTab === 'Resources' && (
        <div
          className="pointer-events-auto absolute select-none"
          style={{ left: metrics.leftDockOffset, right: metrics.edgeMargin, bottom: metrics.edgeMargin, height: bottomDockHeight }}
        >
          <FloatingPanel className="relative flex h-full flex-col p-2">
            <div {...bottomDockResizeHandleProps} className="group absolute -top-1.5 left-0 z-10 h-3 w-full touch-none" style={{ cursor: 'ns-resize' }}>
              <div className="mx-auto mt-1.5 h-px w-full bg-transparent group-hover:bg-accent/70 group-active:bg-accent" />
            </div>
            <ProjectBrowser
              editor={editorRef.current}
              doc={doc}
              projectName={project.fileMenu.projectName}
              projectFolder={project.projectFolder}
              projectFiles={project.projectFiles}
              selectedAssetId={selectedAssetId}
              onSelectAsset={handleSelectAsset}
              onImportImage={handleImportImage}
              onPlacePrefab={handlePlacePrefab}
              onPlaceImage={(id) => handlePlaceAssetAt({ kind: 'image', id }, null)}
              onPlaceDrawing={(id) => handlePlaceAssetAt({ kind: 'drawing', id }, null)}
              onPlaceComponent={handleAddInstance}
              onEditDrawing={handleEditDrawing}
              onRenameDrawing={(id, name) => editorRef.current && setDoc(renameDrawing(editorRef.current, id, name))}
              onDeleteDrawing={(id) => editorRef.current && setDoc(deleteDrawing(editorRef.current, id))}
              onRenamePrefab={(id, name) => editorRef.current && setDoc(renamePrefab(editorRef.current, id, name))}
              onDeletePrefab={(id) => editorRef.current && setDoc(deletePrefab(editorRef.current, id))}
              onOpenScene={handleSelectScene}
              onShowInExplorer={project.fileMenu.onShowInExplorer}
              onNewProject={project.fileMenu.onNewProject}
              onOpenProject={project.fileMenu.onOpenProject}
              onRefresh={project.refreshFiles}
            />
          </FloatingPanel>
        </div>
      )}

      {effectiveBottomTab === 'Console' && (
        <div
          className="pointer-events-auto absolute select-none"
          style={
            mode === 'Playground'
              ? // Docked edge to edge under the game (no rail in Playground).
                { left: 0, right: 0, bottom: 0, height: PLAY_CONSOLE_HEIGHT }
              : { left: metrics.leftDockOffset, right: metrics.edgeMargin, bottom: metrics.edgeMargin, height: bottomDockHeight }
          }
        >
          <FloatingPanel className={`relative flex h-full flex-col p-2 ${mode === 'Playground' ? '!rounded-none border-x-0 border-b-0 !shadow-none' : ''}`}>
            <div {...bottomDockResizeHandleProps} className="group absolute -top-1.5 left-0 z-10 h-3 w-full touch-none" style={{ cursor: 'ns-resize' }}>
              <div className="mx-auto mt-1.5 h-px w-full bg-transparent group-hover:bg-accent/70 group-active:bg-accent" />
            </div>
            <ConsolePanel onOpenCode={handleOpenCodeAt} />
          </FloatingPanel>
        </div>
      )}
      {/* Playground: just the Console tab (with its error count) until it's
          opened — same strip as Design's bottom dock, sitting on the panel. */}
      {mode === 'Playground' && (
        <div
          className={`pointer-events-auto absolute z-20 flex items-center gap-0.5 border border-panel-border bg-panel px-1 py-0.5 backdrop-blur-panel ${playConsoleOpen ? 'rounded-t-md border-b-0' : 'rounded-md'}`}
          style={{ left: playConsoleOpen ? 8 : metrics.edgeMargin, bottom: playConsoleOpen ? PLAY_CONSOLE_HEIGHT - 1 : metrics.edgeMargin, height: BOTTOM_TAB_STRIP_HEIGHT }}
        >
          <button
            onClick={() => setPlayConsoleOpen((open) => !open)}
            className={`relative rounded px-2 py-1 text-[11px] font-medium uppercase tracking-wide ${playConsoleOpen ? 'bg-overlay/10 text-ink' : 'text-ink-dim hover:text-ink'}`}
          >
            Console
            <ConsoleBadge />
          </button>
          {playConsoleOpen && (
            <button onClick={() => setPlayConsoleOpen(false)} title="Close" className="ml-0.5 rounded p-1 text-ink-dim hover:bg-overlay/10 hover:text-ink">
              <CloseIcon />
            </button>
          )}
        </div>
      )}

      {effectiveBottomTab === 'Shader' && (
        <div
          className="pointer-events-auto absolute select-none"
          style={{ left: metrics.leftDockOffset, right: metrics.edgeMargin, bottom: metrics.edgeMargin, height: bottomDockHeight }}
        >
          <FloatingPanel className="relative flex h-full flex-col p-2">
            <div {...bottomDockResizeHandleProps} className="group absolute -top-1.5 left-0 z-10 h-3 w-full touch-none" style={{ cursor: 'ns-resize' }}>
              <div className="mx-auto mt-1.5 h-px w-full bg-transparent group-hover:bg-accent/70 group-active:bg-accent" />
            </div>
            <ShaderDock
              editor={editorRef.current}
              doc={doc}
              materialId={shaderMaterialId}
              onSelectMaterial={setShaderMaterialId}
              onDocumentChange={setDoc}
              selectedNode={selectedNode}
              onSetSelectedBehaviors={handleSetNodeBehaviors}
            />
          </FloatingPanel>
        </div>
      )}

      {effectiveBottomTab === 'Animate' && (
        <TimelinePanel
          ref={timelinePanelRef}
          animations={artboards[0]?.animations ?? []}
          activeAnimationId={artboards[0]?.activeAnimation ?? null}
          selectedNode={selectedNode}
          onScrub={handleScrub}
          onAddKeyframe={handleAddKeyframe}
          onRemoveKeyframe={handleRemoveKeyframe}
          onPreviewKeyframeTime={handlePreviewKeyframeTime}
          onCommitKeyframeTime={handleCommitKeyframeTime}
          onSetKeyframeEasing={handleSetKeyframeEasing}
          onAddMotionPathKeyframe={handleAddMotionPathKeyframe}
          onAddAnimation={handleAddAnimation}
          onSelectAnimation={handleSelectAnimation}
          onDeleteAnimation={handleDeleteAnimation}
          onSetAnimationDuration={handleSetAnimationDuration}
          onSelectionChange={setKeyframeSelection}
          autoPlayOnMount={autoPlayAnimation}
          onAutoPlayConsumed={() => setAutoPlayAnimation(false)}
          height={bottomDockHeight}
          resizeHandleProps={bottomDockResizeHandleProps}
        />
      )}

      {effectiveBottomTab === 'StateMachine' && stateMachineError && (
        <div
          className="pointer-events-auto absolute flex items-center gap-2 rounded-md border border-red-500/30 bg-red-950/90 px-3 py-1.5 text-xs text-red-200 shadow-lg"
          style={{ left: metrics.leftDockOffset, bottom: bottomDockClearance + 4 }}
        >
          <span className="max-w-md truncate" title={stateMachineError}>
            Script error: {stateMachineError}
          </span>
          <button onClick={() => setStateMachineError(null)} className="shrink-0 rounded px-1.5 py-0.5 text-red-200/70 hover:bg-overlay/10 hover:text-red-100">
            Dismiss
          </button>
        </div>
      )}

      {effectiveBottomTab === 'StateMachine' && (
        <LocalErrorBoundary label="State Machine" resetKey={mode}>
        <StateMachineDock
          artboard={artboards[0] ?? null}
          activeMachine={activeStateMachine}
          summary={stateMachine}
          onFireTrigger={handleFireTrigger}
          onAddStateMachine={handleAddStateMachine}
          onSelectStateMachine={handleSelectStateMachine}
          onDeleteStateMachine={handleDeleteStateMachine}
          onRenameStateMachine={handleRenameStateMachine}
          onAddInput={handleAddStateMachineInput}
          onSetBoolInput={handleSetBoolInput}
          onSetNumberInput={handleSetNumberInput}
          onSetEnumInput={handleSetEnumInput}
          selectedLayer={selectedStateMachineLayer}
          onSelectLayer={setSelectedStateMachineLayer}
          onAddLayer={handleAddStateMachineLayer}
          onDeleteLayer={handleDeleteStateMachineLayer}
          onSetLayerInitialState={handleSetStateMachineLayerInitialState}
          onAddState={handleAddState}
          onUpdateState={handleUpdateState}
          onDeleteState={handleDeleteState}
          onSetStatePosition={handleSetStatePosition}
          onSetInitialState={handleSetStateMachineInitialState}
          onAddTransition={handleAddTransition}
          onSetTransition={handleSetTransition}
          onRemoveTransition={handleRemoveTransition}
          onSetStateBlend={handleSetStateBlend}
          onSetStateProgram={handleSetStateProgram}
          programsEnabled={programsEnabled}
          onToggleProgramsEnabled={handleToggleProgramsEnabled}
          onResetLayout={handleResetStateMachineLayout}
          isPlayground={mode === 'Playground'}
          height={bottomDockHeight}
          resizeHandleProps={bottomDockResizeHandleProps}
        />
        </LocalErrorBoundary>
      )}
    </div>
    </LayoutMetricsContext.Provider>
  )
}

export default App
