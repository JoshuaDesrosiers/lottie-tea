/**
 * The wasm-based editor bridge — replaces Tauri IPC (`invoke`) for
 * everything document/render-related. See `crates/an1mbyt3-wasm/src/editor.rs`'s
 * module doc for the full "why": the editor's canvas now renders via the
 * same wasm module the web runtime uses, directly into the canvas's own
 * `wgpu::Surface`, with no native process and no per-frame bridge at all —
 * the Rive/Figma-style architecture, not a native-Rust-plus-readback-bridge
 * one.
 *
 * `WasmEditor`'s wasm-bindgen-generated methods are typed `any` for
 * anything crossing the JS<->Rust boundary as JSON-like data (wasm-bindgen
 * can't know the JS shape of an arbitrary `serde`-derived Rust type at
 * compile time) — these thin wrappers restore real typing using the exact
 * same `DocumentSummary`/`OpaqueId` interfaces `ipc/document.ts` already
 * defined for the old Tauri bridge, since `editor.rs`'s DTOs are
 * byte-for-byte the same shape on purpose.
 */
import { fetchFontFile } from '../game/googleFonts'
import init, { WasmEditor } from '../wasm-pkg/an1mbyt3_wasm.js'
import type { ScreenSettings, ToonMaterialConfig, ToonStyle, FieldInfo,
  AddAnimationResult,
  PrefabResult,
  Transform2D,
  ProjectFileListing,
  AddArtboardResult,
  AddNodeResult,
  DrawingResult,
  Blend1DDef,
  BlendMode,
  CameraConfig,
  ComponentKind,
  NodeBehavior,
  Light2DConfig,
  Light2DType,
  ShadowCasterConfig,
  TrailConfig,
  ConditionDef,
  Easing,
  GradientDef,
  DocumentSummary,
  MaterialStatus,
  Node3D,
  Node3DItem,
  ShaderGraphDef,
  ShaderNodeSpec,
  DuplicateNodesResult,
  ImageProcessing,
  ImportImageResult,
  ImportSvgResult,
  InputKind,
  LoopMode,
  NodeBehaviorEntry,
  OpaqueId,
  ParticleSystemConfig,
  Path,
  PlaybackDirection,
  PostEffectLayer,
  ProgramDef,
  PropertyName,
  ScriptFieldDef,
  StateMachineSummary,
  Stroke,
} from './document'

type RgbaColor = [number, number, number, number]

let wasmInit: Promise<void> | null = null
// Where the engine's `.wasm` comes from: fetched next to the JS glue by
// default; the exported standalone player hands in the bytes it carries
// inline instead (`setWasmBytes`).
let wasmBytes: BufferSource | null = null

/** Use these `.wasm` bytes instead of fetching the file — call before the
 * first `createWasmEditor`. */
export function setWasmBytes(bytes: BufferSource): void {
  wasmBytes = bytes
}

function ensureWasmInit(): Promise<void> {
  if (!wasmInit) wasmInit = (wasmBytes ? init({ module_or_path: wasmBytes }) : init()).then(() => undefined)
  return wasmInit
}

/** Creates the one `WasmEditor` instance the whole app shares — call once,
 * after the canvas element exists (`App.tsx` does this in a mount effect). */
export async function createWasmEditor(canvas: HTMLCanvasElement): Promise<WasmEditor> {
  await ensureWasmInit()
  return WasmEditor.create(canvas)
}

export function getDocumentSummary(editor: WasmEditor): DocumentSummary {
  return editor.document_summary() as DocumentSummary
}

export function setNodeTranslation(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, x: number, y: number): DocumentSummary {
  return editor.set_node_translation(artboard, node, x, y) as DocumentSummary
}

export function setNodeRotationDegrees(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, degrees: number): DocumentSummary {
  return editor.set_node_rotation(artboard, node, degrees) as DocumentSummary
}

export function setNodeScale(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, x: number, y: number): DocumentSummary {
  return editor.set_node_scale(artboard, node, x, y) as DocumentSummary
}

/** The Inspector's plain Anchor X/Y fields — a raw, uncompensated edit (the
 * shape visibly shifts, matching how typing a new Anchor Point value
 * directly in After Effects' properties panel does). See `commitNodeAnchor`
 * for the compensated on-canvas drag version, and `editor.rs`'s
 * `set_node_anchor` doc comment for why the two intentionally differ. */
export function setNodeAnchor(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, x: number, y: number): DocumentSummary {
  return editor.set_node_anchor(artboard, node, x, y) as DocumentSummary
}

/** Adds a shape node with a preset default path (`kind`: `"Rectangle"`/
 * `"Ellipse"`/`"Triangle"`/`"Star"`) to `artboard` as one undoable edit
 * (`Command::AddNode` — see `editor.rs`'s doc comment on that variant) and
 * returns the id it was assigned, so the caller can select it right away —
 * the Shapes panel's quick-drop-in buttons. */
export function addShapeNode(editor: WasmEditor, artboard: OpaqueId, kind: ShapePreset, parent: OpaqueId | null = null): AddNodeResult {
  return editor.add_shape_node(artboard, kind, parent) as AddNodeResult
}

/** Creates a new `ParticleSystem` node with every field at its authored
 * default (a gentle upward cone spray) — the Shapes menu's "Add Particle
 * System" entry. One undoable edit (`Command::AddNode`), same shape as
 * `addShapeNode`. */
export function addParticleSystemNode(editor: WasmEditor, artboard: OpaqueId, parent: OpaqueId | null = null): AddNodeResult {
  return editor.add_particle_system_node(artboard, parent) as AddNodeResult
}

/** Paint Asset Mode's Pen tool — starts a brand-new `Shape` node from a
 * blank canvas (an empty `Path`, zero commands) as one undoable edit, the
 * one exception to every other `Shape` node always starting from a
 * non-empty path (a preset, or SVG/image import). The Pen tool overlay
 * calls this exactly once per new shape (not once per anchor placed), then
 * appends anchors locally and commits the whole finished path through the
 * existing `previewShapePath`/`setShapePath` (same as `PathEditOverlay`). */
export function addBlankShapeNode(editor: WasmEditor, artboard: OpaqueId, parent: OpaqueId | null = null): AddNodeResult {
  return editor.add_blank_shape_node(artboard, parent) as AddNodeResult
}

/** Imports `xml` (the contents of a dropped-in `.svg` file) onto `artboard`
 * as one undoable edit (`Command::Batch` of one `Command::AddNode` per
 * shape found — see `editor.rs`'s `import_svg` doc comment) — the Shapes
 * panel's "Import SVG" action. Throws (via wasm-bindgen's error mapping) if
 * the file isn't readable as SVG at all, or has zero importable shapes. */
export function importSvg(editor: WasmEditor, artboard: OpaqueId, xml: string): ImportSvgResult {
  return editor.import_svg(artboard, xml) as ImportSvgResult
}

/** The Shapes menu's "Load Geometry Dash Demo" action — builds a scrolling
 * auto-runner level (player, ground, spikes, a finish trigger, a wall) and a
 * Programmable-State `StateMachineDef` driving it with a real `Blocks`
 * program (gravity, jump-on-space, scroll, hazard-respawn, a launched-into-
 * the-wall ending), selects that machine, and returns the updated summary.
 * One undoable edit. See `editor.rs`'s `load_geometry_dash_demo`/
 * `build_demo_geometry_dash` doc comments for the full design. */
export function loadGeometryDashDemo(editor: WasmEditor, artboard: OpaqueId): DocumentSummary {
  return editor.load_geometry_dash_demo(artboard) as DocumentSummary
}

/** Adds the Ninja Adventure demo's two scenes (Village + House) and bakes
 * all its art. Returns the Village's id as `newArtboard` — the caller
 * switches to it (`setActiveArtboard`) before entering Playground. */
export async function loadNinjaDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_ninja_demo()) as AddArtboardResult
}

/** Adds the Raycaster FPS demo's scene and bakes its art. Returns the new
 * scene's id as `newArtboard`. */
/** Physics Playground demo scene (Rigidbody 2D + colliders). */
export function loadPhysicsDemo(editor: WasmEditor): AddArtboardResult {
  return editor.load_physics_demo() as AddArtboardResult
}

/** Dungeon Royale demo scene (cards + A* bots tower defense). */
/** Demos → "Sunset Tactics": a top-down 5v5 tactical shooter. */
export async function loadTacticsDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  // The agents wear the bundled VRM avatar (popi); without it they fall
  // back to the blocky characters.
  let avatar: Uint8Array | undefined
  try {
    const response = await fetch('/demo/popi.vrm')
    if (response.ok) avatar = new Uint8Array(await response.arrayBuffer())
  } catch {
    avatar = undefined
  }
  return (await editor.load_tactics_demo(avatar)) as AddArtboardResult
}

/** Demos → "Liquid Lab": the Liquid 2D component showcase. */
export async function loadLiquidDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_liquid_demo()) as AddArtboardResult
}

/** Demos → "UI & Sound Lab": UI anchors, a button stack, synthesized sounds. */
/** Demos → "Voxel Craft" (a Minetest-style block sandbox). */
export async function loadVoxelDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_voxel_demo()) as AddArtboardResult
}

/** Demos → "Shatterline" (Smash Hit-style glass-breaking rail shooter). */
/** Demos → "Fatal Clash": fetches the bundled avatar, builds the game. */
export async function loadFightDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  const response = await fetch('/demo/popi.vrm')
  if (!response.ok) throw new Error(`Fatal Clash: couldn't load the avatar (${response.status})`)
  const bytes = new Uint8Array(await response.arrayBuffer())
  return (await editor.load_fight_demo(bytes)) as AddArtboardResult
}

export async function loadShatterDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_shatter_demo()) as AddArtboardResult
}

/** Demos → "Toon Explosion" (layered particle explosion). */
export async function loadBoomDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_boom_demo()) as AddArtboardResult
}

/** Demos → "LAN Party" (host on a port / join by address). */
export async function loadLanDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_lan_demo()) as AddArtboardResult
}

/** Demos → "Cloth Lab" (Cloth 2D). */
export async function loadClothDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_cloth_demo()) as AddArtboardResult
}

/** Demos → "Goo Shapes" (Goo 2D on text / shapes / particles). */
export async function loadGooShapesDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_goo_shapes_demo()) as AddArtboardResult
}

/** Demos → "Goo Lab" (2D liquid particles + behaviours). */
export async function loadGooDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_goo_demo()) as AddArtboardResult
}

/** Demos → "Reyna's Leer" (3D particles + a shader-graph eye). */
export async function loadLeerDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_leer_demo()) as AddArtboardResult
}

/** Emits `count` particles at once from a node's Particle System 3D. */
export function burstParticles3D(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, count: number): boolean {
  return editor.burst_particles_3d(artboard, node, count)
}

/** Demos → "Tree Garden" (Tree 2D). */
export async function loadTreesDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_trees_demo()) as AddArtboardResult
}

/** Demos → "Physics 3D Lab". */
export async function loadPhysics3DDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_physics3d_demo()) as AddArtboardResult
}

/** Demos → "HTML UI Lab". */
export async function loadHtmlDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_html_demo()) as AddArtboardResult
}

/** Demos → "Responsive Layout". */
export async function loadResponsiveDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_responsive_demo()) as AddArtboardResult
}

export async function loadUiDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_ui_demo()) as AddArtboardResult
}

/** Demos → "3D Showroom": the first 3D scene (orbit camera, lit primitives). */
export async function load3DDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_3d_demo()) as AddArtboardResult
}

/** Demos → "Ragdoll Torture": a crash-test dummy and a lab of hazards. */
export async function loadTortureDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_torture_demo()) as AddArtboardResult
}

/** Demos → "Crown Match": a royal match-3 (Home, Levels, Game scenes). */
export async function loadMatch3Demo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_match3_demo()) as AddArtboardResult
}

export async function loadDungeonDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_dungeon_demo()) as AddArtboardResult
}

/** Ragdoll Arena demo scene (bones, IK, ragdolls, skinning). */
export async function loadRigDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_rig_demo()) as AddArtboardResult
}

/** Stress Test demo scene (profiling). */
export async function loadStressDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_stress_demo()) as AddArtboardResult
}

export async function loadFpsDemo(editor: WasmEditor): Promise<AddArtboardResult> {
  return (await editor.load_fps_demo()) as AddArtboardResult
}

/** Duplicates every node in `nodes` (each with its whole subtree) in place —
 * same parent, same transform, no offset — `Ctrl+V`'s "Paste in Place"
 * action, after `Ctrl+C` remembers the selection (see `App.tsx`'s
 * `handleCopySelection`/`handlePasteInPlace`; there's no real clipboard
 * involved, just an in-memory ref). Undoable as one batched edit. */
export function duplicateNodes(editor: WasmEditor, artboard: OpaqueId, nodes: OpaqueId[]): DuplicateNodesResult {
  return editor.duplicate_nodes(artboard, nodes) as DuplicateNodesResult
}

/** Replaces `artboard`'s whole post-processing effect stack as one undoable
 * edit (`Command::SetPostEffects`) — the Effects panel's add/remove/
 * reorder/toggle/duplicate-effect action. Always sends the *whole* list
 * back, even for a single-entry edit, same convention `editor.rs`'s
 * `set_post_effects` doc comment describes. */
export function setPostEffects(editor: WasmEditor, artboard: OpaqueId, layers: PostEffectLayer[]): DocumentSummary {
  return editor.set_post_effects(artboard, layers) as DocumentSummary
}

/** Creates a real Group node around `nodes` and returns the new group's id
 * so the UI can select it immediately. Undoable as one batched edit. */
export function groupNodes(editor: WasmEditor, artboard: OpaqueId, nodes: OpaqueId[], name: string): AddNodeResult {
  const bridge = editor as WasmEditor & { group_nodes?: (artboard: OpaqueId, nodes: OpaqueId[], name: string) => unknown }
  if (!bridge.group_nodes) {
    const idsEqual = (a: OpaqueId | null, b: OpaqueId | null) => a === b || (!!a && !!b && a.idx === b.idx && a.version === b.version)
    const before = editor.document_summary() as DocumentSummary
    const sourceArtboard = before.artboards.find((a) => idsEqual(a.id, artboard))
    const selectedNodes = nodes
      .map((id) => sourceArtboard?.nodes.find((node) => idsEqual(node.id, id)) ?? null)
      .filter((node): node is NonNullable<typeof node> => node !== null)
    if (!sourceArtboard || selectedNodes.length < 2) {
      throw new Error('Group Selection needs at least two selected layers.')
    }
    const commonParent = selectedNodes.every((node) => idsEqual(node.parent, selectedNodes[0].parent)) ? selectedNodes[0].parent : null
    const seed = editor.import_svg(
      artboard,
      `<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2" viewBox="-1 -1 2 2"><g id="${name}"><rect x="-0.5" y="-0.5" width="1" height="1" opacity="0"/></g></svg>`,
    ) as ImportSvgResult
    const freshArtboard = seed.document.artboards.find((a) => idsEqual(a.id, artboard))
    const group = seed.newNodes
      .map((id) => freshArtboard?.nodes.find((node) => idsEqual(node.id, id)) ?? null)
      .find((node) => node?.kind === 'Group')
    if (!group) {
      throw new Error('Group Selection could not create a group node.')
    }

    let document = seed.document
    if (commonParent) document = editor.reparent_node(artboard, group.id, commonParent) as DocumentSummary
    for (const node of selectedNodes) document = editor.reparent_node(artboard, node.id, group.id) as DocumentSummary
    for (const id of seed.newNodes.filter((id) => !idsEqual(id, group.id))) {
      document = editor.delete_node(artboard, id) as DocumentSummary
    }
    return { document, newNode: group.id }
  }
  return bridge.group_nodes(artboard, nodes, name) as AddNodeResult
}

/** Mirrors the `kind` strings `editor.rs`'s `preset_shape_path` matches on. */
export type ShapePreset = 'Rectangle' | 'Ellipse' | 'Triangle' | 'Star'

/** Adds a new bone node to `artboard`, optionally parented under an
 * existing bone (`parent: null` for a root bone) — the toolbar's "Add Bone"
 * action. See `editor.rs`'s `add_bone_node` doc comment for the resulting
 * position convention (parent's tip, or the origin for a root bone). */
export function addBoneNode(editor: WasmEditor, artboard: OpaqueId, parent: OpaqueId | null, length: number): AddNodeResult {
  return editor.add_bone_node(artboard, parent, length) as AddNodeResult
}

/** Resizes a bone's reach — the Inspector's bone-specific length field.
 * Undoable (`Command::SetBoneLength`). */
export function setBoneLength(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: number): DocumentSummary {
  return editor.set_bone_length(artboard, node, value) as DocumentSummary
}

/** Creates a new, empty scene (artboard) — the Scenes switcher's "+ New
 * Scene" action. Undoable (`Command::AddArtboard`). */
export function addArtboard(editor: WasmEditor, name: string, width: number, height: number): AddArtboardResult {
  return editor.add_artboard(name, width, height) as AddArtboardResult
}

/** Soft-deletes a scene — the Scenes switcher's "Delete Scene" action.
 * Undoable. Throws if it's the last remaining scene. */
export function deleteArtboard(editor: WasmEditor, artboard: OpaqueId): DocumentSummary {
  return editor.delete_artboard(artboard) as DocumentSummary
}

/** Renames a scene. Undoable (`Command::SetArtboardName`). */
export function renameArtboard(editor: WasmEditor, artboard: OpaqueId, name: string): DocumentSummary {
  return editor.rename_artboard(artboard, name) as DocumentSummary
}

/** Switches which scene the editor renders/edits — session state, not
 * undoable. Re-picks that scene's own active animation/state machine. */
export function setActiveArtboard(editor: WasmEditor, artboard: OpaqueId): DocumentSummary {
  return editor.set_active_artboard(artboard) as DocumentSummary
}

/** Adds a new Camera node to `artboard`, optionally parented — the Shapes
 * menu's "Add Camera" action. Undoable (`Command::AddNode`). */
export function addCameraNode(editor: WasmEditor, artboard: OpaqueId, parent: OpaqueId | null): AddNodeResult {
  return editor.add_camera_node(artboard, parent) as AddNodeResult
}

/** Replaces a Camera node's whole config (zoom/background/follow) — the
 * Inspector's Camera section. Undoable (`Command::SetCamera`). */
export function setCamera(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: CameraConfig): DocumentSummary {
  return editor.set_camera(artboard, node, value) as DocumentSummary
}

/** A fresh component of `kind` with Rust's (Unity-matching) defaults —
 * the Add Component menu appends it to the node's component list. */
export function defaultComponent(editor: WasmEditor, kind: ComponentKind): NodeBehavior {
  return editor.default_component(kind) as NodeBehavior
}

/** Scene gravity (Canvas section). Undoable. */
/** Scene 3D gravity (Rigidbody 3D), m/s². Undoable. */
export function setArtboardGravity3D(editor: WasmEditor, artboard: OpaqueId, x: number, y: number, z: number): DocumentSummary {
  return editor.set_artboard_gravity_3d(artboard, x, y, z) as DocumentSummary
}

/** `physics3d_raycast`'s hit. */
export interface RayHit3D {
  node: OpaqueId
  x: number
  y: number
  z: number
  normalX: number
  normalY: number
  normalZ: number
  distance: number
}
export function physics3dVelocity(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): [number, number, number] | null {
  return editor.physics3d_velocity(artboard, node) as [number, number, number] | null
}
export function physics3dAngularVelocity(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): [number, number, number] | null {
  return editor.physics3d_angular_velocity(artboard, node) as [number, number, number] | null
}
/** `kind` 0 = linear velocity (m/s), 1 = angular (degrees/s). */
export function physics3dSetVelocity(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, kind: 0 | 1, x: number, y: number, z: number): void {
  editor.physics3d_set_velocity(artboard, node, kind, x, y, z)
}
/** `mode` 0 = force, 1 = impulse, 2 = torque impulse. */
export function physics3dAddForce(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, mode: 0 | 1 | 2, x: number, y: number, z: number): void {
  editor.physics3d_add_force(artboard, node, mode, x, y, z)
}
export function physics3dRaycast(editor: WasmEditor, artboard: OpaqueId, o: [number, number, number], d: [number, number, number], maxDistance: number, exclude: OpaqueId | null): RayHit3D | null {
  return editor.physics3d_raycast(artboard, o[0], o[1], o[2], d[0], d[1], d[2], maxDistance, exclude) as RayHit3D | null
}

/** Glass script API (`ref.shatter3`) — untracked runtime state. */
export function glassShatter(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, p: [number, number, number], v: [number, number, number], radius: number, floor: number): { broken: number; loose: number } {
  return editor.glass_shatter(artboard, node, p[0], p[1], p[2], v[0], v[1], v[2], radius, floor) as { broken: number; loose: number }
}

/** Glass script API (`ctx.glassRaycast3`): nearest standing glass. */
export function glassRaycast(editor: WasmEditor, artboard: OpaqueId, o: [number, number, number], d: [number, number, number], maxDistance: number): RayHit3D | null {
  return editor.glass_raycast(artboard, o[0], o[1], o[2], d[0], d[1], d[2], maxDistance) as RayHit3D | null
}

export function setArtboardGravity(editor: WasmEditor, artboard: OpaqueId, x: number, y: number): DocumentSummary {
  return editor.set_artboard_gravity(artboard, x, y) as DocumentSummary
}

/** Physics script API — all untracked, Playground-only (no-ops otherwise). */
export function physicsVelocity(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): [number, number] | null {
  return editor.physics_velocity(artboard, node) as [number, number] | null
}
export function physicsSetVelocity(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, x: number, y: number): void {
  editor.physics_set_velocity(artboard, node, x, y)
}
export function physicsAngularVelocity(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): number | null {
  return editor.physics_angular_velocity(artboard, node) as number | null
}
export function physicsSetAngularVelocity(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, degreesPerSecond: number): void {
  editor.physics_set_angular_velocity(artboard, node, degreesPerSecond)
}
export function physicsAddForce(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, x: number, y: number, impulse: boolean): void {
  editor.physics_add_force(artboard, node, x, y, impulse)
}
export interface RayHit {
  node: OpaqueId
  x: number
  y: number
  normalX: number
  normalY: number
  distance: number
}
export function physicsRaycast(editor: WasmEditor, artboard: OpaqueId, ox: number, oy: number, dx: number, dy: number, maxDistance: number, exclude: OpaqueId | null): RayHit | null {
  return editor.physics_raycast(artboard, ox, oy, dx, dy, maxDistance, exclude) as RayHit | null
}
export interface ContactEvent {
  a: OpaqueId
  b: OpaqueId
  started: boolean
  trigger: boolean
}
export function physicsEvents(editor: WasmEditor, artboard: OpaqueId): ContactEvent[] {
  return editor.physics_events(artboard) as ContactEvent[]
}

/** Spawns `count` particles from a Particle System node next frame
 * (Unity's `Emit`) — untracked, script-driven. */
export function emitParticles(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, count: number): void {
  editor.emit_particles(artboard, node, count)
}

/** Untracked per-frame `z_index` write (script depth sorting). */
/** Playground's camera view `[panX, panY, zoomPhysicalPx]`, or null. */
export function playgroundView(editor: WasmEditor, artboard: OpaqueId): [number, number, number] | null {
  return editor.playground_view(artboard) as [number, number, number] | null
}

/** Untracked show/hide (script `ref.visible`). */
export function previewNodeVisible(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: boolean): void {
  editor.preview_node_visible(artboard, node, value)
}

/** Untracked whole-grid Tilemap rewrite (row-major, `-1` = empty). */
export function previewTilemapTiles(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, tiles: number[]): void {
  editor.preview_tilemap_tiles(artboard, node, Int32Array.from(tiles))
}

/** Untracked Light 2D intensity / outer radius (negative = unchanged). */
export function previewLight(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, intensity: number, outerRadius: number): void {
  editor.preview_light(artboard, node, intensity, outerRadius)
}

export function previewNodeZIndex(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: number): void {
  editor.preview_node_z_index(artboard, node, value)
}

/** The + menu's "Text" item. Undoable. */
export function addTextNode(editor: WasmEditor, artboard: OpaqueId, parent: OpaqueId | null): AddNodeResult {
  return editor.add_text_node(artboard, parent) as AddNodeResult
}

/** Adds a Unity-style Light 2D node of `lightType` with Unity's defaults —
 * the + menu's Light 2D items. Undoable. */
export function addLight2DNode(editor: WasmEditor, artboard: OpaqueId, parent: OpaqueId | null, lightType: Light2DType): AddNodeResult {
  return editor.add_light2d_node(artboard, parent, lightType) as AddNodeResult
}

/** Adds a Unity-style Trail Renderer node — the + menu's "Trail Renderer"
 * item. Parent it under something that moves. Undoable. */
export function addTrailNode(editor: WasmEditor, artboard: OpaqueId, parent: OpaqueId | null): AddNodeResult {
  return editor.add_trail_node(artboard, parent) as AddNodeResult
}

/** Replaces a Light 2D node's whole config — the Inspector's Light 2D section. Undoable. */
export function setLight2D(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: Light2DConfig): DocumentSummary {
  return editor.set_light2d(artboard, node, value) as DocumentSummary
}

/** Replaces a Trail node's whole config — the Inspector's Trail Renderer section. Undoable. */
export function setTrail(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: TrailConfig): DocumentSummary {
  return editor.set_trail(artboard, node, value) as DocumentSummary
}

/** Attaches/edits (`value`) or removes (`null`) a node's Shadow Caster 2D. Undoable. */
export function setShadowCaster(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: ShadowCasterConfig | null): DocumentSummary {
  return editor.set_shadow_caster(artboard, node, value) as DocumentSummary
}

/** Toggles whether a node (and its subtree) ignores 2D lighting. Undoable. */
export function setNodeUnlit(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: boolean): DocumentSummary {
  return editor.set_node_unlit(artboard, node, value) as DocumentSummary
}

/** Names which Camera node (`null` clears it) Playground actually views
 * `artboard` through — the Inspector's "Set as Active Camera" action.
 * Undoable (`Command::SetActiveCamera`). */
export function setActiveCamera(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId | null): DocumentSummary {
  return editor.set_active_camera(artboard, node) as DocumentSummary
}

/** Toggles a Group's "Y Sort" — the Inspector's checkbox. Undoable
 * (`Command::SetGroupYSort`). */
export function setGroupYSort(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: boolean): DocumentSummary {
  return editor.set_group_y_sort(artboard, node, value) as DocumentSummary
}

/** Changes a node's manual draw-order override — the Inspector's "Z Index"
 * field, any node kind. Undoable (`Command::SetNodeZIndex`). */
export function setNodeZIndex(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: number): DocumentSummary {
  return editor.set_node_z_index(artboard, node, value) as DocumentSummary
}

/** Changes a shape's blend mode — the Inspector's blend-mode dropdown.
 * Undoable (`Command::SetShapeBlendMode`). */
export function setShapeBlendMode(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: BlendMode): DocumentSummary {
  return editor.set_shape_blend_mode(artboard, node, value) as DocumentSummary
}

/** Changes a `Shape` or `Text` node's own fill color — the Inspector's
 * fill-color swatch. Undoable (`Command::SetFillColor`). */
export function setFillColor(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: [number, number, number, number]): DocumentSummary {
  return editor.set_fill_color(artboard, node, value) as DocumentSummary
}

/** Live fill-color preview — untracked, no undo entry, same reasoning as
 * `previewNodeTranslation`/`previewShapePath`. For a script changing a
 * color every frame (a flash fading out, a background pulse) — see
 * `editor.rs`'s `preview_fill_color` doc comment for why that must not go
 * through the undoable `setFillColor` instead. */
export function previewFillColor(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: [number, number, number, number]): void {
  editor.preview_fill_color(artboard, node, value)
}

/** Sets (pass a `GradientDef`) or clears (pass `null`) a `Shape`/`Text`
 * node's gradient — the Fill tool's solid/gradient toggle and its
 * on-canvas handle both go through this. Undoable (`Command::SetGradient`). */
export function setGradient(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: GradientDef | null): DocumentSummary {
  return editor.set_gradient(artboard, node, value) as DocumentSummary
}

/** Fills several things at once (a multi-selection's Fill): every shape /
 * text in `nodes` (groups: everything inside) gets `color`, or the
 * world-space `gradient` laid across them all. One undo step. */
export function fillNodes(editor: WasmEditor, artboard: OpaqueId, nodes: OpaqueId[], color: [number, number, number, number] | null, gradient: GradientDef | null): DocumentSummary {
  return editor.fill_nodes(artboard, nodes, color, gradient) as DocumentSummary
}

/** Multi-selection scale / rotate: remember where `nodes` start. */
export function multiTransformBegin(editor: WasmEditor, artboard: OpaqueId, nodes: OpaqueId[]): void {
  editor.multi_transform_begin(artboard, nodes)
}

/** Live preview: the world-space affine `[a, b, c, d, tx, ty]` (x axis,
 * y axis, translation) applied to every node on top of where it started. */
export function multiTransformPreview(editor: WasmEditor, m: [number, number, number, number, number, number]): void {
  editor.multi_transform_preview(m[0], m[1], m[2], m[3], m[4], m[5])
}

/** Ends the drag: `commit` keeps it (one undo step), else reverts. */
export function multiTransformEnd(editor: WasmEditor, commit: boolean): DocumentSummary {
  return editor.multi_transform_end(commit) as DocumentSummary
}

/** Sets (`value`) or clears (`null`) a `Shape` node's `image` — the
 * Inspector's asset-reference swatch. Undoable (`Command::SetShapeImage`). */
export function setShapeImage(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: OpaqueId | null): DocumentSummary {
  return editor.set_shape_image(artboard, node, value) as DocumentSummary
}

/** Live path-edit preview — untracked, no undo entry, same reasoning as
 * `previewNodeTranslation`. The mesh cache's own per-frame staleness check
 * (`editor.rs`'s `render`) picks up the change on the very next frame with
 * no extra plumbing needed here. */
export function previewShapePath(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: Path): void {
  editor.preview_shape_path(artboard, node, value)
}

/** Replaces a `Shape` node's whole path — the path-editing overlay's commit
 * action. Undoable (`Command::SetShapePath`). */
export function setShapePath(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: Path): DocumentSummary {
  return editor.set_shape_path(artboard, node, value) as DocumentSummary
}

/** Sets (`Some`) or clears (`null`) a `Shape` node's stroke — the
 * Inspector's Stroke section. Undoable (`Command::SetShapeStroke`). */
export function setShapeStroke(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: Stroke | null): DocumentSummary {
  return editor.set_shape_stroke(artboard, node, value) as DocumentSummary
}

/** Replaces a `ParticleSystem` node's whole authored config — the
 * Inspector's Particle System section, written all at once. Undoable
 * (`Command::SetParticleSystem`). */
export function setParticleSystem(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: ParticleSystemConfig): DocumentSummary {
  return editor.set_particle_system(artboard, node, value) as DocumentSummary
}

/** Restarts a `ParticleSystem` node's live simulation from scratch — the
 * Particle System section's "Replay" button. Not undoable (no document
 * change at all — see `editor.rs`'s `replay_particle_system` doc comment). */
export function replayParticleSystem(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): DocumentSummary {
  return editor.replay_particle_system(artboard, node) as DocumentSummary
}

/** Changes a `Text` node's authored string — the Inspector's content
 * field. Undoable (`Command::SetTextContent`). */
export function setTextContent(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: string): DocumentSummary {
  return editor.set_text_content(artboard, node, value) as DocumentSummary
}

/** Live text-content preview — untracked, no undo entry, same reasoning as
 * `previewFillColor`. For a script rewriting a live HUD number every frame
 * — see `editor.rs`'s `preview_text_content` doc comment. */
export function previewTextContent(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: string): void {
  editor.preview_text_content(artboard, node, value)
}

/** Shows/hides a node directly (`Node::visible`) — meant for occasional
 * scene-transition toggles (a Code-mode script's own "scene" groups), not a
 * per-frame change, so this *is* undoable (`Command::SetNodeVisible`) unlike
 * the `preview*` family above. No Inspector UI drives this yet. */
export function setNodeVisible(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: boolean): void {
  editor.set_node_visible(artboard, node, value)
}

/** Swaps which baked image asset an `Image` node draws — untracked, meant
 * for a per-frame sprite-sheet frame swap (`preview_node_image`'s own doc
 * comment). Every candidate frame must already be baked (loaded as part of
 * the demo/document, not freshly imported) — this is a plain id swap. */
export function previewNodeImage(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, image: OpaqueId): void {
  editor.preview_node_image(artboard, node, image)
}

/** Overwrites a `Shape` node's whole path geometry — untracked, meant for a
 * script computing an organic per-frame shape (a "breathing"/growing blob)
 * and writing the result back every tick (`preview_node_path`'s own doc
 * comment). A no-op if `node` isn't a `Shape` node. */
export function previewNodePath(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: Path): void {
  editor.preview_node_path(artboard, node, value)
}

/** Adds a font file (TTF / OTF bytes) to the document as `name`, reusing a
 * font of the same name. Throws if the bytes aren't a readable font. */
export function addFont(editor: WasmEditor, name: string, bytes: Uint8Array): OpaqueId {
  return editor.add_font(name, bytes) as OpaqueId
}

/** Switches a `Text` node's font. Undoable (`Command::SetTextFont`). */
export function setTextFont(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, font: OpaqueId): DocumentSummary {
  return editor.set_text_font(artboard, node, font) as DocumentSummary
}

/** Downloads Google Font `family` (unless the document has it already),
 * adds it, and returns its id. */
export async function addGoogleFont(editor: WasmEditor, family: string): Promise<OpaqueId> {
  const existing = (editor.document_summary() as DocumentSummary).fonts.find((f) => f.name === family)
  if (existing) return existing.id
  const bytes = await fetchFontFile(family)
  return addFont(editor, family, bytes)
}

/** Changes a `Text` node's size (pixels) — the Inspector's size field.
 * Undoable (`Command::SetTextSize`). */
export function setTextSize(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: number): DocumentSummary {
  return editor.set_text_size(artboard, node, value) as DocumentSummary
}

/** Solves two-bone IK for `root`/`mid` against an artboard-space target and
 * commits the result as one undoable edit (a `Batch` of both bones'
 * rotations) — see `editor.rs`'s `solve_two_bone_ik` doc comment for why
 * there's no separate saved "IK constraint" object yet; this is a one-shot
 * "solve and bake" tool. */
export function solveTwoBoneIk(editor: WasmEditor, artboard: OpaqueId, root: OpaqueId, mid: OpaqueId, targetX: number, targetY: number): DocumentSummary {
  return editor.solve_two_bone_ik(artboard, root, mid, targetX, targetY) as DocumentSummary
}

/** Creates a new component definition by copying a Shape node's path/fill
 * color — the Components panel's "Make Component" action. Undoable
 * (`Command::AddComponent`). See `editor.rs`'s `add_component` doc comment
 * for why the source shape itself is left untouched. */
export function addComponent(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, name: string): DocumentSummary {
  return editor.add_component(artboard, node, name) as DocumentSummary
}

/** Soft-deletes a component definition — the Components panel's "Delete"
 * action. Undoable. */
export function deleteComponent(editor: WasmEditor, component: OpaqueId): DocumentSummary {
  return editor.delete_component(component) as DocumentSummary
}

/** Changes a component definition's default fill color. Undoable. */
export function setComponentDefaultFillColor(editor: WasmEditor, component: OpaqueId, value: RgbaColor): DocumentSummary {
  return editor.set_component_default_fill_color(component, value) as DocumentSummary
}

/** Declares a new named variant on a component definition — the Components
 * panel's "Add Variant" action. Undoable. */
export function addComponentVariant(editor: WasmEditor, component: OpaqueId, name: string, fillColor: RgbaColor): DocumentSummary {
  return editor.add_component_variant(component, name, fillColor) as DocumentSummary
}

/** Creates a new, empty (no fields yet) script definition — the Inspector's
 * "+ New Script..." action. Undoable. Fields are added afterward via
 * `setScriptFields`. */
export function addScript(editor: WasmEditor, name: string): DocumentSummary {
  return editor.add_script(name) as DocumentSummary
}

/** Soft-deletes a script definition. Undoable. */
export function deleteScript(editor: WasmEditor, script: OpaqueId): DocumentSummary {
  return editor.delete_script(script) as DocumentSummary
}

/** Replaces a script definition's whole field list — the script authoring
 * UI's add/rename/remove/retype-a-field actions. Undoable. */
export function setScriptFields(editor: WasmEditor, script: OpaqueId, fields: ScriptFieldDef[]): DocumentSummary {
  return editor.set_script_fields(script, fields) as DocumentSummary
}

/** Replaces a node's whole attached-behaviors list — the Inspector's
 * "Behaviors" section: add/remove/toggle a behavior, or edit one `Script`
 * instance's field values, all funnel through this one call. Undoable. */
export function setNodeBehaviors(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, behaviors: NodeBehaviorEntry[]): DocumentSummary {
  return editor.set_node_behaviors(artboard, node, behaviors) as DocumentSummary
}

/** Adds a new instance node of `component` to `artboard`, optionally
 * parented — the Components panel's "drag component into artboard" action.
 * Undoable (`Command::AddNode`). */
export function addInstanceNode(editor: WasmEditor, artboard: OpaqueId, parent: OpaqueId | null, component: OpaqueId): AddNodeResult {
  return editor.add_instance_node(artboard, parent, component) as AddNodeResult
}

/** Changes an instance's direct fill-color override (`null` clears it) —
 * the Inspector's per-instance color field. Undoable. */
export function setInstanceFillColorOverride(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: RgbaColor | null): DocumentSummary {
  return editor.set_instance_fill_color_override(artboard, node, value) as DocumentSummary
}

/** Changes which named variant an instance references (`null` clears it) —
 * the Inspector's per-instance variant dropdown. Undoable. */
export function setInstanceVariant(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: string | null): DocumentSummary {
  return editor.set_instance_variant(artboard, node, value) as DocumentSummary
}

/** Soft-deletes `node` as one undoable edit (`Command::SetDeleted` — see
 * `editor.rs`'s doc comment for why deletion is a flag flip, not real
 * removal). `undo()` restores it exactly, `NodeId` included. */
export function deleteNode(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): DocumentSummary {
  return editor.delete_node(artboard, node) as DocumentSummary
}

/** Soft-deletes every id in `nodes` as one undoable edit — the canvas/Layers
 * panel multi-select's "Delete" action. Undoable (`Command::Batch` of
 * `SetDeleted`s, one undo step for the whole selection). */
export function deleteNodes(editor: WasmEditor, artboard: OpaqueId, nodes: OpaqueId[]): DocumentSummary {
  return editor.delete_nodes(artboard, nodes) as DocumentSummary
}

/** Renames `node` — the Layers panel's double-click-to-rename action.
 * Undoable (`Command::SetNodeName`). */
export function renameNode(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, name: string): DocumentSummary {
  return editor.rename_node(artboard, node, name) as DocumentSummary
}

/** Changes a node's `elementId` — the Inspector's id field, `getCompByID`'s
 * lookup key (`scriptRuntime.ts`). Undoable. */
export function setNodeId(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: string): DocumentSummary {
  return editor.set_node_id(artboard, node, value) as DocumentSummary
}

/** Replaces a node's whole `classes` list — the Inspector's classes field,
 * `getCompByClass`'s selector tags. Undoable. */
export function setNodeClasses(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: string[]): DocumentSummary {
  return editor.set_node_classes(artboard, node, value) as DocumentSummary
}

/** Toggles `NodeSummary.clipChildren` — see that field's own doc comment.
 * Undoable (`Command::SetNodeClipChildren`). */
export function setNodeClipChildren(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: boolean): DocumentSummary {
  return editor.set_node_clip_children(artboard, node, value) as DocumentSummary
}

/** Moves `node` to a new parent (`null` for the artboard's root) — the
 * Layers panel's drag-to-reparent action. Undoable (`Command::ReparentNode`).
 * Throws (via wasm-bindgen's error mapping) if this would parent a node
 * under itself or one of its own descendants. */
export function reparentNode(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, parent: OpaqueId | null): DocumentSummary {
  return editor.reparent_node(artboard, node, parent) as DocumentSummary
}

/** `reparentNode`'s positional counterpart — the Layers panel's actual
 * drag-to-*reorder* gesture. Moves `node` into `parent`'s children (`null`
 * for the artboard root), positioned immediately before `before` (or at the
 * end, if `before` is `null`/not among the resulting siblings) — unlike
 * `reparentNode` alone, which always lands `node` at the very end regardless
 * of where it was dropped. Undoable (`Command::ReparentNodeAt`); same
 * cycle/self-parent guard as `reparentNode`. */
export function reparentNodeBefore(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, parent: OpaqueId | null, before: OpaqueId | null): DocumentSummary {
  return editor.reparent_node_before(artboard, node, parent, before) as DocumentSummary
}

/** Sets (or, via `null`, clears) which child clips the rest of a `Group`
 * node's contents — the Layers panel's "use as mask" action. Undoable
 * (`Command::SetGroupMask`). Throws if `node` isn't a `Group`. */
export function setGroupMask(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, mask: OpaqueId | null): DocumentSummary {
  return editor.set_group_mask(artboard, node, mask) as DocumentSummary
}

/** "Bring Forward" — one step later in `node`'s draw-order list (its
 * siblings under the same parent, or the artboard root if it has none). */
export function moveNodeForward(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): DocumentSummary {
  return editor.move_node_forward(artboard, node) as DocumentSummary
}

/** "Send Backward" — `moveNodeForward`'s mirror. */
export function sendNodeBackward(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): DocumentSummary {
  return editor.send_node_backward(artboard, node) as DocumentSummary
}

/** "Bring to Front" — moves `node` to the very end of its draw-order list. */
export function bringNodeToFront(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): DocumentSummary {
  return editor.bring_node_to_front(artboard, node) as DocumentSummary
}

/** "Send to Back" — `bringNodeToFront`'s mirror. */
export function sendNodeToBack(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): DocumentSummary {
  return editor.send_node_to_back(artboard, node) as DocumentSummary
}

export function undo(editor: WasmEditor): DocumentSummary {
  return editor.undo() as DocumentSummary
}

export function redo(editor: WasmEditor): DocumentSummary {
  return editor.redo() as DocumentSummary
}

/** `.tea` is plain JSON text — `editor.rs`'s `save_tea` already returns it
 * pretty-printed, nothing left for this wrapper to do but type it. */
export function saveTea(editor: WasmEditor): string {
  return editor.save_tea()
}

/** `.teab` is compressed binary — `editor.rs`'s `save_teab` returns a
 * `Uint8Array` directly (wasm-bindgen's native `Vec<u8>` mapping). */
export function saveTeab(editor: WasmEditor): Uint8Array {
  return editor.save_teab()
}

/** `async`: `editor.rs`'s `load_tea` bakes every image asset the freshly
 * loaded document's `Image` nodes reference before resolving — see that
 * method's own doc comment for why loading a document is the one place
 * besides `importImage`/`setImageProcessing` that needs a real `await`
 * (baking is a GPU readback, which the per-frame render loop can't perform). */
export async function loadTea(editor: WasmEditor, json: string): Promise<DocumentSummary> {
  return (await editor.load_tea(json)) as DocumentSummary
}

/** `async` — see `loadTea`'s doc comment. */
export async function loadTeab(editor: WasmEditor, bytes: Uint8Array): Promise<DocumentSummary> {
  return (await editor.load_teab(bytes)) as DocumentSummary
}

/** Imports a PNG/JPEG/GIF file's raw bytes onto `artboard` — the Shapes
 * panel's "Import Image/GIF" action. A still image creates one selectable
 * `Image` node (`result.newNode`); a GIF creates its decoded frames as a
 * grouped set of ordinary image assets (`result.newImages`/`newSpriteGroup`)
 * with no canvas node at all — frames-only this pass, see `editor.rs`'s
 * `import_image` doc comment. `async`: every freshly added asset is already
 * baked (visible) by the time this resolves. */
export async function importImage(editor: WasmEditor, artboard: OpaqueId, filename: string, bytes: Uint8Array): Promise<ImportImageResult> {
  return (await editor.import_image(artboard, filename, bytes)) as ImportImageResult
}

/** Creates a new `Tilemap` node from a tileset image file's raw bytes — the
 * Tilemap panel's "New Tilemap" action. `async`: the tileset is already
 * baked (visible) by the time this resolves, same reasoning as `importImage`. */
export async function addTilemapNode(
  editor: WasmEditor,
  artboard: OpaqueId,
  parent: OpaqueId | null,
  filename: string,
  bytes: Uint8Array,
  tileWidthPx: number,
  tileHeightPx: number,
  mapWidth: number,
  mapHeight: number,
  cellSize: number,
): Promise<AddNodeResult> {
  return (await editor.add_tilemap_node(artboard, parent, filename, bytes, tileWidthPx, tileHeightPx, mapWidth, mapHeight, cellSize)) as AddNodeResult
}

/** Paints (`value` a tile index) or erases (`value: null`) one cell of a
 * `Tilemap` node — the Tilemap panel's paint tool. One undoable edit per
 * cell click. */
export function setTile(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, x: number, y: number, value: number | null): DocumentSummary {
  return editor.set_tile(artboard, node, x, y, value) as DocumentSummary
}

/** Resizes a `Tilemap` node's grid — the Inspector's "Map Width"/"Map
 * Height" fields. Overlapping tiles are preserved (`Command::SetTilemapGrid`'s
 * own doc comment). One undoable edit. */
export function resizeTilemap(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, mapWidth: number, mapHeight: number): DocumentSummary {
  return editor.resize_tilemap(artboard, node, mapWidth, mapHeight) as DocumentSummary
}

/** Changes a `Tilemap` node's on-canvas cell size — the Inspector's "Cell
 * Size" field. One undoable edit. */
export function setTilemapCellSize(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: number): DocumentSummary {
  return editor.set_tilemap_cell_size(artboard, node, value) as DocumentSummary
}

/** Returns an image asset's raw PNG bytes — the Resources panel wraps this
 * in a `Blob`/object URL to render a real `<img>` thumbnail, no canvas/
 * `ImageData` round-trip needed since the bytes are already displayable PNG. */
export function getImageSource(editor: WasmEditor, image: OpaqueId): Uint8Array {
  return editor.get_image_source(image)
}

/** Updates an image asset's pre-processing params (resize/filter/effect) —
 * the Inspector's asset-mode fields, committed as one whole-struct edit (not
 * undoable — asset content sits outside the undo/redo system, same as a
 * font's bytes). `async`: the asset is re-baked before this resolves, so the
 * canvas reflects the new params on its very next render. */
export async function setImageProcessing(editor: WasmEditor, image: OpaqueId, processing: ImageProcessing): Promise<DocumentSummary> {
  return (await editor.set_image_processing(image, processing)) as DocumentSummary
}

/** Hit-tests an artboard-space point against every shape, topmost-drawn
 * first — `null` if the point hits nothing. `x`/`y` use the same
 * origin-centered, +y-up convention every other pointer-facing call in this
 * workspace uses (canvas-pixel -> artboard-space conversion happens in
 * `App.tsx`'s pointer handlers). */
export function pickNodeAt(editor: WasmEditor, artboard: OpaqueId, x: number, y: number): OpaqueId | null {
  return (editor.pick_node_at(artboard, x, y) as OpaqueId | null) ?? null
}

/** Live drag preview — mutates the node's translation directly, no undo
 * entry, no `DocumentSummary` returned (nothing the rest of the UI needs to
 * react to mid-drag; see `commit_node_translation` for the single real edit
 * a whole drag produces). Call this on every `mousemove` while dragging. */
export function previewNodeTranslation(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, x: number, y: number): void {
  editor.preview_node_translation(artboard, node, x, y)
}

/** Ends a drag as exactly one undoable edit. `origin` is the node's
 * translation from *before* the drag started — restoring it first is what
 * makes the resulting undo entry correctly revert to the pre-drag position
 * rather than the drag's second-to-last preview frame (see `editor.rs`'s
 * `commit_node_translation` doc comment). */
export function commitNodeTranslation(
  editor: WasmEditor,
  artboard: OpaqueId,
  node: OpaqueId,
  origin: { x: number; y: number },
  x: number,
  y: number,
): DocumentSummary {
  return editor.commit_node_translation(artboard, node, origin.x, origin.y, x, y) as DocumentSummary
}

/** Toggles whether `editor.render()` drives the demo state machine — call
 * with `true` when the toolbar's mode switches to "State Machine", `false`
 * otherwise. See `editor.rs`'s `set_state_machine_active` doc comment for
 * why entering the tab can visibly "jump" the previewed node (expected,
 * not a bug). */
export function setStateMachineActive(editor: WasmEditor, active: boolean): void {
  editor.set_state_machine_active(active)
}

/** The toolbar's mode-switch entry point — sets both `state_machine_active`/
 * `animate_mode_active` together *and* snapshots/restores every node's
 * transform across the Design-vs-either-animated-mode boundary, so leaving
 * Animate/State Machine mode snaps back to the actually-authored pose
 * instead of leaving whatever the last scrub/tick/state happened to mutate
 * it to. See `editor.rs`'s `set_mode` doc comment for the full reasoning,
 * including why this replaced two separate calls rather than living
 * alongside them. `mode` is `'Design' | 'Animate' | 'StateMachine'` (not
 * `'State Machine'` with a space — a plain wasm-bindgen `&str` match, not
 * the frontend's own `Mode` union). */
export function setMode(editor: WasmEditor, artboard: OpaqueId, mode: 'Design' | 'Animate' | 'StateMachine' | 'Playground'): DocumentSummary {
  return editor.set_mode(artboard, mode) as DocumentSummary
}

/** The State Machine tab's own opt-in toggle for actually running a
 * Programmable State's `program` while previewing there — `false` by
 * default (see `editor.rs`'s `state_machine_programs_enabled` doc comment
 * for why). Playground always runs programs regardless of this toggle. */
export function setStateMachineProgramsEnabled(editor: WasmEditor, enabled: boolean): void {
  editor.set_state_machine_programs_enabled(enabled)
}

/** Live preview while dragging the on-canvas transform tool's rotate
 * handle — mutates the node's rotation directly, no undo entry, same
 * "live preview, commit separately" split as `previewNodeTranslation`.
 * `degrees` matches `setNodeRotationDegrees`'s convention. */
/** Bone drag (FK): untracked — rotates `bone` to point at world `(x, y)`. */
export function previewBoneAim(editor: WasmEditor, artboard: OpaqueId, bone: OpaqueId, x: number, y: number): void {
  editor.preview_bone_aim(artboard, bone, x, y)
}

/** Bone drag (IK): untracked — bends the chain of up to `chainLength`
 * bones ending at `tip` toward world `(x, y)`. Returns the chain. */
export function previewBoneIk(editor: WasmEditor, artboard: OpaqueId, tip: OpaqueId, chainLength: number, x: number, y: number): OpaqueId[] {
  return editor.preview_bone_ik(artboard, tip, chainLength, x, y) as OpaqueId[]
}

/** "Make Ragdoll": body + capsule + hinge on every bone under `root`.
 * `animated`: start Kinematic (follow animation until switched). Undoable. */
export function makeRagdoll(editor: WasmEditor, artboard: OpaqueId, root: OpaqueId, thickness: number, limitDegrees: number, animated: boolean): DocumentSummary {
  return editor.make_ragdoll(artboard, root, thickness, limitDegrees, animated) as DocumentSummary
}

/** "Add IK Target": target node at the bone's tip + IK constraint. Undoable. */
export function addIkTarget(editor: WasmEditor, artboard: OpaqueId, bone: OpaqueId, chainLength: number): DocumentSummary {
  return editor.add_ik_target(artboard, bone, chainLength) as DocumentSummary
}

/** "Bind to Bones": skin an Image to the rig under `rootBone`. Undoable. */
export function bindSpriteSkin(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, rootBone: OpaqueId, columns: number, rows: number): DocumentSummary {
  return editor.bind_sprite_skin(artboard, node, rootBone, columns, rows) as DocumentSummary
}

/** Script API: untracked Rigidbody 2D body type switch (`deep` = whole subtree). */
// ---- UI ----

/** Everything the + → UI menu can add (`WasmEditor::add_ui_node`). */
export type UiMenuItem = 'Button' | 'Toggle' | 'Slider' | 'Scrollbar' | 'ScrollView' | 'Panel' | 'Label' | 'Stack' | 'Row' | 'Column' | 'Page' | 'Html'
/** Menu order. */
export const UI_MENU_ITEMS: UiMenuItem[] = ['Page', 'Row', 'Column', 'Html', 'Button', 'Toggle', 'Slider', 'Scrollbar', 'ScrollView', 'Panel', 'Label', 'Stack']
/** Menu labels where the id isn't presentable. */
export const UI_MENU_LABEL: Partial<Record<UiMenuItem, string>> = { ScrollView: 'Scroll View', Page: 'Responsive Page', Row: 'Row Layout', Column: 'Column Layout', Html: 'HTML + CSS' }

/** Sets the scene's screen behaviour (Fixed / Responsive). Undoable. */
export function setArtboardScreen(editor: WasmEditor, artboard: OpaqueId, value: ScreenSettings): DocumentSummary {
  return editor.set_artboard_screen(artboard, value) as DocumentSummary
}

/** Design mode's "preview on" screen size (CSS px; `null` = the design size). */
export function setPreviewScreen(editor: WasmEditor, size: { width: number; height: number } | null): void {
  editor.set_preview_screen(size?.width ?? 0, size?.height ?? 0)
}

/** The screen the last frame laid out for (`ctx.screen`). */
export function screenInfoNow(editor: WasmEditor): { width: number; height: number; screenWidth: number; screenHeight: number; compact: boolean; portrait: boolean } {
  const s = editor.screen_info_now()
  return { width: s[0] ?? 0, height: s[1] ?? 0, screenWidth: s[2] ?? 0, screenHeight: s[3] ?? 0, compact: s[4] === 1, portrait: s[5] === 1 }
}

/** The + menu's UI items, under the active camera (HUD). One undo step. */
export function addUiNode(editor: WasmEditor, artboard: OpaqueId, item: UiMenuItem): AddNodeResult {
  return editor.add_ui_node(artboard, item) as AddNodeResult
}

// ---- Audio ----

/** Import → Audio: adds a sound to the document. */
export function importAudio(editor: WasmEditor, fileName: string, bytes: Uint8Array): { document: DocumentSummary; audio: OpaqueId } {
  return editor.import_audio(fileName, bytes) as { document: DocumentSummary; audio: OpaqueId }
}

/** Every sound's id and name. */
export function audioList(editor: WasmEditor): { id: OpaqueId; name: string }[] {
  return editor.audio_list() as { id: OpaqueId; name: string }[]
}

/** Links a sound to its file in the project folder. */
export function setAudioFile(editor: WasmEditor, audio: OpaqueId, path: string | null): void {
  editor.set_audio_file(audio, path ?? undefined)
}

export function renameAudio(editor: WasmEditor, audio: OpaqueId, name: string): DocumentSummary {
  return editor.rename_audio(audio, name) as DocumentSummary
}

export function deleteAudio(editor: WasmEditor, audio: OpaqueId): DocumentSummary {
  return editor.delete_audio(audio) as DocumentSummary
}

// ---- 3D ----

/** The + menu's 3D items: a primitive mesh, a camera, a light or a pivot. Undoable. */
export function addNode3D(editor: WasmEditor, artboard: OpaqueId, parent: OpaqueId | null, item: Node3DItem): AddNodeResult {
  return editor.add_node3d(artboard, parent, item) as AddNodeResult
}

/** The Inspector's 3D section: replaces a Node3D's transform + content (one undo step). */
export function setNode3D(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, value: Node3D): DocumentSummary {
  return editor.set_node3d(artboard, node, value) as DocumentSummary
}

/** Every external file a glTF / GLB references (textures, buffers), by URI. */
export function gltfExternalUris(bytes: Uint8Array): string[] {
  let json: { images?: { uri?: string }[]; buffers?: { uri?: string }[] }
  try {
    const isGlb = bytes[0] === 0x67 && bytes[1] === 0x6c && bytes[2] === 0x54 && bytes[3] === 0x46 // "glTF"
    if (isGlb) {
      const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
      const length = view.getUint32(12, true)
      json = JSON.parse(new TextDecoder().decode(bytes.subarray(20, 20 + length)))
    } else {
      json = JSON.parse(new TextDecoder().decode(bytes))
    }
  } catch {
    return []
  }
  const uris = [...(json.images ?? []), ...(json.buffers ?? [])].map((x) => x.uri).filter((u): u is string => !!u && !u.startsWith('data:'))
  return [...new Set(uris.map((u) => decodeURIComponent(u)))]
}

/** Imports a glTF / GLB model (+ the files it references) as a new Node3D. */
export function importModel(editor: WasmEditor, artboard: OpaqueId, name: string, source: Uint8Array, files: { path: string; data: Uint8Array }[]): AddNodeResult {
  const total = files.reduce((n, f) => n + f.data.length, 0)
  const blob = new Uint8Array(total)
  let offset = 0
  for (const f of files) {
    blob.set(f.data, offset)
    offset += f.data.length
  }
  return editor.import_model(artboard, name, source, files.map((f) => f.path), blob, Uint32Array.from(files.map((f) => f.data.length))) as AddNodeResult
}

/** An imported model's animation clip names. */
export function modelClips(editor: WasmEditor, model: OpaqueId): string[] {
  return editor.model_clips(model) as string[]
}

/** Untracked script write of a model mesh's animation (`ref.playAnimation3`). */
export function previewNode3DAnimation(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, clip: string, loop: boolean, overlay: string): void {
  editor.preview_node3d_animation(artboard, node, clip, loop, overlay)
}

// ---- Scene properties (Rive view models) ----

export function addProperty(editor: WasmEditor, artboard: OpaqueId, name: string, kind: string): DocumentSummary {
  return editor.add_property(artboard, name, kind) as DocumentSummary
}

export function updateProperty(editor: WasmEditor, property: OpaqueId, name: string, kind: string, value: string): DocumentSummary {
  return editor.update_property(property, name, kind, value) as DocumentSummary
}

export function deleteProperty(editor: WasmEditor, property: OpaqueId): DocumentSummary {
  return editor.delete_property(property) as DocumentSummary
}

// ---- Code files (the Code tab) ----

/** New file at `path` (made unique if taken). Undoable. */
export function addTextFile(editor: WasmEditor, path: string, text: string): DocumentSummary {
  return editor.add_text_file(path, text) as DocumentSummary
}

/** Typing: the file's new text (no undo entry; the code editor has its own). */
export function setTextFileText(editor: WasmEditor, file: OpaqueId, text: string): void {
  editor.set_text_file_text(file, text)
}

/** Rename / move. Undoable. Throws if the path is taken. */
export function renameTextFile(editor: WasmEditor, file: OpaqueId, path: string): DocumentSummary {
  return editor.rename_text_file(file, path) as DocumentSummary
}

/** Delete. Undoable. */
export function deleteTextFile(editor: WasmEditor, file: OpaqueId): DocumentSummary {
  return editor.delete_text_file(file) as DocumentSummary
}

/** Script API: replace a block world's blocks (`ref.setVoxels`). */
export function setVoxels(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, blocks: Uint8Array): void {
  editor.voxels_set(artboard, node, blocks)
}

/** Script API: load one chunk of a streamed block world (`ref.setVoxelChunk`). */
export function setVoxelChunk(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, cx: number, cz: number, blocks: Uint8Array): void {
  editor.voxel_chunk_set(artboard, node, cx, cz, blocks)
}

/** Script API: drop one chunk of a streamed block world (`ref.unloadVoxelChunk`). */
export function unloadVoxelChunk(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, cx: number, cz: number): void {
  editor.voxel_chunk_unload(artboard, node, cx, cz)
}

/** Script API: set one block of a block world (`ref.setVoxel`). */
export function setVoxel(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, x: number, y: number, z: number, block: number): boolean {
  return editor.voxel_set(artboard, node, x, y, z, block)
}

/** Untracked script write of a Node3D's transform (`ref.position3 = ...`). */
export function previewNode3DTransform(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, values: number[]): void {
  editor.preview_node3d_transform(artboard, node, Float32Array.from(values))
}

// ---- Shader-graph materials ----

/** The Shader tab's "+ New Material" (starter graph). Undoable. */
export function addMaterial(editor: WasmEditor, name: string): { document: DocumentSummary; material: OpaqueId } {
  return editor.add_material(name) as { document: DocumentSummary; material: OpaqueId }
}

/** Import → avatar animation (VRMA / GLB / VRM): its clips go onto
 * `node`'s Humanoid Animator. Throws if the file has no humanoid motion. */
export function importHumanoidAnimation(editor: WasmEditor, name: string, bytes: Uint8Array, artboard: OpaqueId | null, node: OpaqueId | null): { document: DocumentSummary; clips: string[] } {
  return editor.import_humanoid_animation(name, bytes, artboard, node) as { document: DocumentSummary; clips: string[] }
}

/** Opens a file picker for avatar animations and imports the chosen file
 * onto `node` (a VRM avatar). Resolves with the new document, or null if
 * cancelled. */
export function pickAndImportHumanoidAnimation(editor: WasmEditor, artboard: OpaqueId | null, node: OpaqueId | null): Promise<DocumentSummary | null> {
  return new Promise((resolve, reject) => {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = '.vrma,.glb,.gltf,.vrm'
    input.onchange = async () => {
      const file = input.files?.[0]
      if (!file) return resolve(null)
      try {
        const bytes = new Uint8Array(await file.arrayBuffer())
        resolve(importHumanoidAnimation(editor, file.name, bytes, artboard, node).document)
      } catch (err) {
        reject(err)
      }
    }
    input.click()
  })
}

/** Every animatable value of a node (the Timeline's "+ Property"). */
export function nodeFields(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): FieldInfo[] {
  return (editor.node_fields(artboard, node) as FieldInfo[] | null) ?? []
}

/** Timeline "Record": every Inspector change keys at the playhead. */
export function setRecordKeys(editor: WasmEditor, on: boolean): void {
  editor.set_record_keys(on)
}

/** "+ Toon / Arcane material". Undoable. */
export function addToonMaterial(editor: WasmEditor, name: string, style: ToonStyle): { document: DocumentSummary; material: OpaqueId } {
  return editor.add_toon_material(name, style) as { document: DocumentSummary; material: OpaqueId }
}

/** A Toon material's settings (`null` for other kinds). */
export function materialToon(editor: WasmEditor, material: OpaqueId): ToonMaterialConfig | null {
  return editor.material_toon(material) as ToonMaterialConfig | null
}

/** Edits a Toon material: `commit = false` previews live (untracked),
 * `commit = true` records one undo step (returns the document then). */
export function setMaterialToon(editor: WasmEditor, material: OpaqueId, value: ToonMaterialConfig, commit: boolean): DocumentSummary | null {
  return editor.set_material_toon(material, value, commit) as DocumentSummary | null
}

/** "Extract materials": editable Toon materials for an imported VRM's
 * MToon materials (null when there was nothing to make). */
export function modelExtractMaterials(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): DocumentSummary | null {
  return editor.model_extract_materials(artboard, node) as DocumentSummary | null
}

/** A material's shader graph (`null` if it isn't a graph material). */
export function materialGraph(editor: WasmEditor, material: OpaqueId): ShaderGraphDef | null {
  return editor.material_graph(material) as ShaderGraphDef | null
}

/** Replaces a material's graph — one undo step. */
export function setMaterialGraph(editor: WasmEditor, material: OpaqueId, graph: ShaderGraphDef): DocumentSummary {
  return editor.set_material_graph(material, graph) as DocumentSummary
}

export function renameMaterial(editor: WasmEditor, material: OpaqueId, name: string): DocumentSummary {
  return editor.rename_material(material, name) as DocumentSummary
}

export function deleteMaterial(editor: WasmEditor, material: OpaqueId): DocumentSummary {
  return editor.delete_material(material) as DocumentSummary
}

/** The generated WGSL and any compile error, for the Shader tab. */
export function materialStatus(editor: WasmEditor, material: OpaqueId): MaterialStatus {
  return editor.material_status(material) as MaterialStatus
}

/** Every shader-graph node type with its sockets (fetched once). */
export function shaderNodeCatalog(editor: WasmEditor): ShaderNodeSpec[] {
  return editor.shader_node_catalog() as ShaderNodeSpec[]
}

/** Untracked: disables a node's Hinge Joint 2D (`ref.breakJoint()` — a
 * ragdoll limb breaking off). Restored when Playground stops. */
export function previewBreakJoint(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): void {
  editor.preview_break_joint(artboard, node)
}

export function previewBodyType(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, bodyType: 'Dynamic' | 'Kinematic' | 'Static', deep: boolean): void {
  editor.preview_body_type(artboard, node, bodyType, deep)
}

/** Ends a bone drag as one undo step (`original` = rotations before it). */
export function commitBonePose(editor: WasmEditor, artboard: OpaqueId, bones: OpaqueId[], original: number[]): DocumentSummary {
  return editor.commit_bone_pose(artboard, bones, Float32Array.from(original)) as DocumentSummary
}

export function previewNodeRotation(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, degrees: number): void {
  editor.preview_node_rotation(artboard, node, degrees)
}

/** Ends a rotate-handle drag as exactly one undoable edit. `originDegrees`
 * is the node's rotation from *before* the drag started — same
 * restore-then-apply reasoning as `commitNodeTranslation`. */
export function commitNodeRotation(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, originDegrees: number, degrees: number, compensateChildren: boolean): DocumentSummary {
  return editor.commit_node_rotation(artboard, node, originDegrees, degrees, compensateChildren) as DocumentSummary
}

/** Live preview while dragging an on-canvas scale handle — mutates
 * translation *and* scale directly (no undo). Translation moves too because
 * the transform tool's corner handles scale around the opposite corner
 * (`TransformHandles.tsx`'s corner-drag math), which keeps that corner fixed
 * in world space by moving the node's origin as it grows/shrinks. */
/** A node's current local bounds `[minX, minY, maxX, maxY]` (UI scroll views). */
export function nodeLocalBounds(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): [number, number, number, number] {
  const b = editor.node_local_bounds(artboard, node)
  return [b[0] ?? 0, b[1] ?? 0, b[2] ?? 0, b[3] ?? 0]
}

export function previewNodeScale(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, x: number, y: number, scaleX: number, scaleY: number): void {
  editor.preview_node_scale(artboard, node, x, y, scaleX, scaleY)
}

/** Ends a scale-handle drag as one undoable edit (two, batched together, if
 * the anchor-corner math also moved translation). All four `origin*` values
 * are the node's translation/scale from *before* the drag started. */
export function commitNodeScale(
  editor: WasmEditor,
  artboard: OpaqueId,
  node: OpaqueId,
  originX: number,
  originY: number,
  originScaleX: number,
  originScaleY: number,
  x: number,
  y: number,
  scaleX: number,
  scaleY: number,
  compensateChildren: boolean,
): DocumentSummary {
  return editor.commit_node_scale(artboard, node, originX, originY, originScaleX, originScaleY, x, y, scaleX, scaleY, compensateChildren) as DocumentSummary
}

/** Live preview while dragging the on-canvas anchor handle — mutates anchor
 * *and* translation directly (no undo), same "both fields move together"
 * shape `previewNodeScale` has for its own corner-pin drag. Compute both
 * values via the anchor-preserving-world-position formula (`Transform2D::
 * with_anchor_preserving_world`'s Rust doc comment has the derivation;
 * `TransformHandles.tsx` ports it to TS) so the shape's rendered geometry
 * stays visually fixed while the anchor marker tracks the cursor. */
export function previewNodeAnchor(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, anchorX: number, anchorY: number, x: number, y: number): void {
  editor.preview_node_anchor(artboard, node, anchorX, anchorY, x, y)
}

/** Ends an anchor-handle drag as one undoable edit (a batch of anchor+
 * translation, same shape `commitNodeScale` uses for its own corner-pin
 * commit). All four `origin*` values are the node's anchor/translation from
 * *before* the drag started. Unlike `setNodeAnchor`, this keeps the shape's
 * rendered geometry fixed — the whole point of dragging the pivot directly
 * in the canvas. */
export function commitNodeAnchor(
  editor: WasmEditor,
  artboard: OpaqueId,
  node: OpaqueId,
  originAnchorX: number,
  originAnchorY: number,
  originX: number,
  originY: number,
  anchorX: number,
  anchorY: number,
  x: number,
  y: number,
): DocumentSummary {
  return editor.commit_node_anchor(artboard, node, originAnchorX, originAnchorY, originX, originY, anchorX, anchorY, x, y) as DocumentSummary
}

/** Toggles whether the toolbar's mode is "Animate" — see `editor.rs`'s
 * `animate_mode_active` doc comment for what this gates (whether a plain
 * transform edit rebases a node's whole keyframed path, or overwrites/
 * creates just one keyframe). */
export function setAnimateModeActive(editor: WasmEditor, active: boolean): void {
  editor.set_animate_mode_active(active)
}

/** Tracks the current selection so `render()` can draw the selected node's
 * motion-path overlay — call with `null` on deselection. */
export function setSelectedNode(editor: WasmEditor, node: OpaqueId | null): void {
  editor.set_selected_node(node)
}

/** Fires a named trigger input — takes effect on the editor's *next*
 * rendered frame (`StateMachine::advance` is where a fired trigger is
 * actually checked). */
export function fireTrigger(editor: WasmEditor, name: string): void {
  editor.fire_trigger(name)
}

export function getStateMachineSummary(editor: WasmEditor): StateMachineSummary {
  return editor.state_machine_summary() as StateMachineSummary
}

/** Adds (or, at an already-keyframed time, overwrites) a keyframe on
 * `node`'s `property` track within the active animation, using the node's
 * current live value for that property as the value — the Timeline's
 * per-property "Add Keyframe" action. Undoable. */
export function addKeyframe(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, property: PropertyName, time: number): DocumentSummary {
  return editor.add_keyframe(artboard, node, property, time) as DocumentSummary
}

/** Deletes the keyframe at exactly `time` on `node`'s `property` track
 * within the active animation. Undoable. */
export function removeKeyframe(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, property: PropertyName, time: number): DocumentSummary {
  return editor.remove_keyframe(artboard, node, property, time) as DocumentSummary
}

/** Binds `node`'s `MotionPath` track (within the active animation) to
 * `guide` — creates the track if this is `node`'s first motion-path guide,
 * or rebinds an existing track's guide in place, keeping its keyframes.
 * Call once (guide picked in the Timeline) before `addMotionPathKeyframe`.
 * Undoable. */
export function setMotionPathGuide(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, guide: OpaqueId): DocumentSummary {
  return editor.set_motion_path_guide(artboard, node, guide) as DocumentSummary
}

/** Removes `node`'s entire `MotionPath` track (guide binding and any
 * progress keyframes) from the active animation. Undoable. */
export function removeMotionPathTrack(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): DocumentSummary {
  return editor.remove_motion_path_track(artboard, node) as DocumentSummary
}

/** Adds (or, at an already-keyframed time, overwrites) a progress keyframe
 * (`0..=1` along the guide's own path) on `node`'s `MotionPath` track — the
 * `MotionPath` counterpart to `addKeyframe`, taking an explicit `progress`
 * instead of the node's current live value (a motion-path track has no
 * "current value" on the node itself to capture). `node` must already have
 * a `MotionPath` track (`setMotionPathGuide`). Undoable. */
export function addMotionPathKeyframe(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, time: number, progress: number): DocumentSummary {
  return editor.add_motion_path_keyframe(artboard, node, time, progress) as DocumentSummary
}

/** Sets the easing curve a keyframe uses to interpolate *out of* itself
 * toward the next one — the Curve Editor panel's save action. `index` is
 * the keyframe's position in that property's `PropertyTrackSummary.keyframes`
 * (same addressing as `previewKeyframeTime`/`commitKeyframeTime`). Undoable
 * (re-issues `AddKeyframe` at the same time/value under the hood). */
export function setKeyframeEasing(
  editor: WasmEditor,
  artboard: OpaqueId,
  node: OpaqueId,
  property: PropertyName,
  index: number,
  easing: Easing,
): DocumentSummary {
  return editor.set_keyframe_easing(artboard, node, property, index, easing) as DocumentSummary
}

/** Live keyframe-drag preview — `index` is the keyframe's position in that
 * property's `PropertyTrackSummary.keyframes` (stable for the duration of
 * one drag; see `editor.rs`'s doc comment on why it deliberately isn't
 * re-sorted mid-drag). No undo entry, no `DocumentSummary` returned, same
 * reasoning as `previewNodeTranslation`. */
export function previewKeyframeTime(
  editor: WasmEditor,
  artboard: OpaqueId,
  node: OpaqueId,
  property: PropertyName,
  index: number,
  newTime: number,
): void {
  editor.preview_keyframe_time(artboard, node, property, index, newTime)
}

/** Ends a keyframe-drag gesture as exactly one undoable edit. `originTime`
 * is the keyframe's time from *before* the drag started — see
 * `commitNodeTranslation`'s doc comment for why restoring that first is what
 * makes the resulting undo entry correct. */
export function commitKeyframeTime(
  editor: WasmEditor,
  artboard: OpaqueId,
  node: OpaqueId,
  property: PropertyName,
  index: number,
  originTime: number,
  newTime: number,
): DocumentSummary {
  return editor.commit_keyframe_time(artboard, node, property, index, originTime, newTime) as DocumentSummary
}

/** Scrubs the active animation to its sampled pose at `time` — the
 * Timeline's playhead-drag preview. Not undoable (a scrub isn't an edit),
 * and safe to call continuously while dragging the playhead — see
 * `editor.rs`'s `set_scrub_time` doc comment for why this doesn't fight
 * with a subsequent on-canvas node drag. */
export function setScrubTime(editor: WasmEditor, artboard: OpaqueId, time: number): DocumentSummary {
  return editor.set_scrub_time(artboard, time) as DocumentSummary
}

/** Creates a new, empty, named animation on `artboard` and selects it as
 * active — the Timeline's animation-dropdown "New" action. Undoable
 * (`Command::AddAnimation`). */
export function addAnimation(editor: WasmEditor, artboard: OpaqueId, name: string): AddAnimationResult {
  return editor.add_animation(artboard, name) as AddAnimationResult
}

/** Builds the "Load Demo Scene" action's small hover/sway animation on
 * `node` (a real, fully-formed `Animation` — Translation and Rotation
 * tracks, both already keyframed — applied as one `Command::AddAnimation`,
 * not built up one keyframe at a time) and makes it active. Undoable, same
 * as `addAnimation`. `flame` (or `null`) additionally gets a scale-pulse +
 * fill-color-cycle track on the same animation, so more than one piece of
 * the demo scene visibly moves. See `editor.rs`'s `add_demo_hover_animation`
 * doc comment for why the demo doesn't build this via `addKeyframe` instead. */
export function addDemoHoverAnimation(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, flame: OpaqueId | null): AddAnimationResult {
  return editor.add_demo_hover_animation(artboard, node, flame) as AddAnimationResult
}

/** Switches which animation the Timeline authors/scrubs — the dropdown's
 * selection action. Resets the scrub position and re-samples the canvas to
 * the newly selected animation's pose at time `0`. */
export function setActiveAnimation(editor: WasmEditor, artboard: OpaqueId, animation: OpaqueId): DocumentSummary {
  return editor.set_active_animation(artboard, animation) as DocumentSummary
}

/** Renames an animation. Undoable. */
export function renameAnimation(editor: WasmEditor, artboard: OpaqueId, animation: OpaqueId, name: string): DocumentSummary {
  return editor.rename_animation(artboard, animation, name) as DocumentSummary
}

/** Resizes an animation's fixed, authored duration — the Timeline's
 * duration input. Undoable (`Command::SetAnimationDuration`). */
export function setAnimationDuration(editor: WasmEditor, artboard: OpaqueId, animation: OpaqueId, seconds: number): DocumentSummary {
  return editor.set_animation_duration(artboard, animation, seconds) as DocumentSummary
}

/** Resizes the artboard's canvas — the Inspector's "Canvas" section. Undoable. */
export function setArtboardSize(editor: WasmEditor, artboard: OpaqueId, width: number, height: number): DocumentSummary {
  return editor.set_artboard_size(artboard, width, height) as DocumentSummary
}

/** Sets the fraction of native resolution the artboard renders at — the
 * Inspector's "Canvas" section's resolution field. Undoable. */
export function setArtboardResolutionScale(editor: WasmEditor, artboard: OpaqueId, value: number): DocumentSummary {
  return editor.set_artboard_resolution_scale(artboard, value) as DocumentSummary
}

/** Soft-deletes an animation (`Command::SetAnimationDeleted`). Undoable. */
export function deleteAnimation(editor: WasmEditor, artboard: OpaqueId, animation: OpaqueId): DocumentSummary {
  return editor.delete_animation(artboard, animation) as DocumentSummary
}

/** Sets a named boolean input on the live preview instance — the State
 * Machine panel's live-test controls. Takes effect on the next frame, same
 * timing as `fireTrigger`. */
export function setStateMachineBoolInput(editor: WasmEditor, name: string, value: boolean): void {
  editor.set_state_machine_bool_input(name, value)
}

/** Sets a named numeric input — same timing as `setStateMachineBoolInput`. */
export function setStateMachineNumberInput(editor: WasmEditor, name: string, value: number): void {
  editor.set_state_machine_number_input(name, value)
}

/** Sets a named enum-valued input — same timing as `setStateMachineBoolInput`. */
export function setStateMachineEnumInput(editor: WasmEditor, name: string, value: string): void {
  editor.set_state_machine_enum_input(name, value)
}

/** Forwards a real keyboard event into the live preview instance's
 * `Blocks`-mode input surface — a Programmable State's "key [ ] pressed?"
 * sensing reporter and "when key [ ] pressed" hat both read this. Call
 * unconditionally on every `keydown`/`keyup` (inert unless a Blocks script
 * reads that key), same "always forward, gate at the read side" convention
 * every other input setter here already uses. */
export function setKeyDown(editor: WasmEditor, key: string, pressed: boolean): void {
  editor.set_key_down(key, pressed)
}

/** Creates a new state machine on `artboard` with one default state
 * ("State 1", looping, driven by `initialAnimation`) and makes it the
 * State Machine panel's active one — the panel's "New State Machine"
 * action. Undoable (`Command::AddStateMachine`). */
export function addStateMachine(editor: WasmEditor, artboard: OpaqueId, name: string, initialAnimation: OpaqueId): DocumentSummary {
  return editor.add_state_machine(artboard, name, initialAnimation) as DocumentSummary
}

/** Switches which state machine the State Machine panel previews/tests —
 * the panel's state-machine selector. Not undoable (session state, same as
 * `setActiveAnimation`). */
export function setActiveStateMachine(editor: WasmEditor, artboard: OpaqueId, stateMachine: OpaqueId): DocumentSummary {
  return editor.set_active_state_machine(artboard, stateMachine) as DocumentSummary
}

/** Soft-deletes a state machine — the panel's "Delete State Machine"
 * action. Undoable (`Command::SetStateMachineDeleted`). */
export function deleteStateMachine(editor: WasmEditor, artboard: OpaqueId, stateMachine: OpaqueId): DocumentSummary {
  return editor.delete_state_machine(artboard, stateMachine) as DocumentSummary
}

/** Changes which state a state machine starts in — the panel's
 * initial-state dropdown. Undoable (`Command::SetStateMachineInitialState`). */
export function setStateMachineInitialState(editor: WasmEditor, artboard: OpaqueId, stateMachine: OpaqueId, value: string): DocumentSummary {
  return editor.set_state_machine_initial_state(artboard, stateMachine, value) as DocumentSummary
}

/** Renames a state machine — the State Machine panel's "Edit" button.
 * Undoable (`Command::SetStateMachineName`). */
export function renameStateMachine(editor: WasmEditor, artboard: OpaqueId, stateMachine: OpaqueId, value: string): DocumentSummary {
  return editor.rename_state_machine(artboard, stateMachine, value) as DocumentSummary
}

/** Declares a new named input on a state machine — the panel's "Declare
 * Input" action. Undoable (`Command::SetStateMachineInputs`, whole-list
 * swap — see that Rust variant's doc comment). */
export function addStateMachineInput(editor: WasmEditor, artboard: OpaqueId, stateMachine: OpaqueId, name: string, kind: InputKind): DocumentSummary {
  return editor.add_state_machine_input(artboard, stateMachine, name, kind) as DocumentSummary
}

/** Adds a new state to a state machine, referencing `animation` and
 * looping per `loopMode` — the panel's "Add State" action. `layer` is
 * `null` for the base (top-level) layer, or a layer name to add the state
 * to one of `StateMachineDefSummary.layers` instead. Undoable
 * (`Command::SetStateMachineStates` for the base layer,
 * `Command::SetStateMachineLayers` for an extra one). */
export function addState(
  editor: WasmEditor,
  artboard: OpaqueId,
  stateMachine: OpaqueId,
  name: string,
  animation: OpaqueId,
  loopMode: LoopMode,
  layer: string | null = null,
): DocumentSummary {
  return editor.add_state(artboard, stateMachine, name, animation, loopMode, layer) as DocumentSummary
}

/** Adds a transition out of `stateName`, to state `to`, guarded by
 * `condition` and playing `direction` — the panel's "Add Transition"
 * action. `layer` is `null` for the base layer, or a layer name. Undoable.
 *
 * `durationSeconds` (the transition's crossfade length) defaults to `-1`,
 * a sentinel meaning "use the target state's own animation duration" — see
 * `WasmEditor::add_transition`'s doc comment for why that's the sensible
 * authoring-time default for a brand-new transition, distinct from
 * `TransitionDef::duration_seconds`'s own `0` (instant) serde default.
 * `interruptible` (Rive's "Allow Exit During Transition") defaults to
 * `true` — the most-responsive, always-interruptible default this engine
 * has always effectively had. */
export function addTransition(
  editor: WasmEditor,
  artboard: OpaqueId,
  stateMachine: OpaqueId,
  stateName: string,
  to: string,
  condition: ConditionDef,
  direction: PlaybackDirection,
  durationSeconds = -1,
  interruptible = true,
  layer: string | null = null,
): DocumentSummary {
  return editor.add_transition(artboard, stateMachine, stateName, to, condition, direction, durationSeconds, interruptible, layer) as DocumentSummary
}

/** Changes an existing state's animation/loop mode — the graph's
 * selected-state inspector. `layer` is `null` for the base layer, or a
 * layer name. Undoable. */
export function updateState(
  editor: WasmEditor,
  artboard: OpaqueId,
  stateMachine: OpaqueId,
  name: string,
  animation: OpaqueId,
  loopMode: LoopMode,
  layer: string | null = null,
): DocumentSummary {
  return editor.update_state(artboard, stateMachine, name, animation, loopMode, layer) as DocumentSummary
}

/** Turns `name` into a continuous blend-tree state (pass a `Blend1DDef`) or
 * back into a plain played-back one (pass `null`) — the graph's
 * selected-state inspector's "dynamic expression" toggle. `layer` is
 * `null` for the base layer, or a layer name. Undoable. */
export function setStateBlend(
  editor: WasmEditor,
  artboard: OpaqueId,
  stateMachine: OpaqueId,
  name: string,
  blend: Blend1DDef | null,
  layer: string | null = null,
): DocumentSummary {
  return editor.set_state_blend(artboard, stateMachine, name, blend, layer) as DocumentSummary
}

/** Replaces a state's whole `program` (pass `null` to turn a Programmable
 * State back into a plain played-back one) — the node graph/code editor's
 * save action. The frontend computes the entire new `ProgramDef` after
 * every node/wire/override edit and calls this once (see
 * `an1mbyt3_core::command::Command::SetStateProgram`'s doc comment).
 * `layer` is `null` for the base layer, or a layer name. Undoable. */
export function setStateProgram(
  editor: WasmEditor,
  artboard: OpaqueId,
  stateMachine: OpaqueId,
  name: string,
  program: ProgramDef | null,
  layer: string | null = null,
): DocumentSummary {
  return editor.set_state_program(artboard, stateMachine, name, program, layer) as DocumentSummary
}

/** Deletes a state (refuses on a state machine's/layer's last remaining
 * one) — the graph's per-box "delete" action. `layer` is `null` for the
 * base layer, or a layer name. Undoable. */
export function deleteState(editor: WasmEditor, artboard: OpaqueId, stateMachine: OpaqueId, name: string, layer: string | null = null): DocumentSummary {
  return editor.delete_state(artboard, stateMachine, name, layer) as DocumentSummary
}

/** Replaces one existing transition's condition/direction (and, if given, its
 * crossfade `durationSeconds`/`interruptible`) in place — the graph's
 * per-transition inline editor. `layer` is `null` for the base layer, or a
 * layer name. Undoable.
 *
 * `durationSeconds` defaults to `-1`, a sentinel meaning "leave the
 * transition's existing duration untouched" — see `WasmEditor::set_transition`'s
 * doc comment for why editing condition/direction shouldn't silently reset a
 * duration the designer already tuned. `interruptible` defaults to `null`
 * for the identical reason (a plain boolean has no spare sentinel value the
 * way a duration has "negative"). */
export function setTransition(
  editor: WasmEditor,
  artboard: OpaqueId,
  stateMachine: OpaqueId,
  stateName: string,
  index: number,
  condition: ConditionDef,
  direction: PlaybackDirection,
  durationSeconds = -1,
  interruptible: boolean | null = null,
  layer: string | null = null,
): DocumentSummary {
  return editor.set_transition(artboard, stateMachine, stateName, index, condition, direction, durationSeconds, interruptible, layer) as DocumentSummary
}

/** Deletes one transition — the graph's per-transition "delete" button.
 * `layer` is `null` for the base layer, or a layer name. Undoable. */
export function removeTransition(editor: WasmEditor, artboard: OpaqueId, stateMachine: OpaqueId, stateName: string, index: number, layer: string | null = null): DocumentSummary {
  return editor.remove_transition(artboard, stateMachine, stateName, index, layer) as DocumentSummary
}

/** Adds a new, independent extra layer to `stateMachine` — one default
 * state referencing `initialAnimation`, empty mask (touches nothing until
 * nodes are checked for it) — the Layers panel's "Add Layer" action.
 * Undoable (`Command::SetStateMachineLayers`). */
export function addStateMachineLayer(editor: WasmEditor, artboard: OpaqueId, stateMachine: OpaqueId, name: string, initialAnimation: OpaqueId): DocumentSummary {
  return editor.add_state_machine_layer(artboard, stateMachine, name, initialAnimation) as DocumentSummary
}

/** Removes `layerName` from `stateMachine`'s extra layers — the Layers
 * panel's per-layer "delete" action. Undoable. */
export function deleteStateMachineLayer(editor: WasmEditor, artboard: OpaqueId, stateMachine: OpaqueId, layerName: string): DocumentSummary {
  return editor.delete_state_machine_layer(artboard, stateMachine, layerName) as DocumentSummary
}

/** Replaces `layerName`'s whole node mask — the Layers panel's per-node
 * checkbox for whichever layer is currently selected there. Pass the full
 * updated node-id set on every toggle. Undoable. */
export function setStateMachineLayerMask(editor: WasmEditor, artboard: OpaqueId, stateMachine: OpaqueId, layerName: string, mask: OpaqueId[]): DocumentSummary {
  return editor.set_state_machine_layer_mask(artboard, stateMachine, layerName, mask) as DocumentSummary
}

/** Changes which state `layerName` starts in — that layer's own
 * initial-state dropdown. Undoable. */
export function setStateMachineLayerInitialState(editor: WasmEditor, artboard: OpaqueId, stateMachine: OpaqueId, layerName: string, value: string): DocumentSummary {
  return editor.set_state_machine_layer_initial_state(artboard, stateMachine, layerName, value) as DocumentSummary
}

/** Moves a state's box on the graph canvas — call once on pointer-up at the
 * end of a drag gesture, not per pointer-move frame (see `editor.rs`'s
 * `set_state_position` doc comment for why there's no live-preview variant).
 * Undoable (`Command::SetStatePosition`). */
export function setStatePosition(
  editor: WasmEditor,
  artboard: OpaqueId,
  stateMachine: OpaqueId,
  stateName: string,
  x: number,
  y: number,
  layer: string | null = null,
): DocumentSummary {
  return editor.set_state_position(artboard, stateMachine, stateName, x, y, layer) as DocumentSummary
}

export type { WasmEditor }

// ---- Prefabs (`editor.rs` "Prefabs" section, `an1mbyt3_core::prefab`) ----

/** Turns `node`'s subtree into a new prefab; it becomes the first instance. */
export function createPrefab(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId, name: string): PrefabResult {
  return editor.create_prefab(artboard, node, name) as PrefabResult
}

/** Places a new instance of `prefab` with its root at `(x, y)` in `parent`'s space. */
export function instantiatePrefab(editor: WasmEditor, artboard: OpaqueId, prefab: OpaqueId, parent: OpaqueId | null, x: number, y: number): PrefabResult {
  return editor.instantiate_prefab(artboard, prefab, parent, x, y) as PrefabResult
}

/** The prefab instance root `node` belongs to, or `null`. */
export function prefabInstanceRoot(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): OpaqueId | null {
  return editor.prefab_instance_root(artboard, node) as OpaqueId | null
}

export function applyPrefabInstance(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): DocumentSummary {
  return editor.apply_prefab_instance(artboard, node) as DocumentSummary
}

export function revertPrefabInstance(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): DocumentSummary {
  return editor.revert_prefab_instance(artboard, node) as DocumentSummary
}

export function unpackPrefabInstance(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): DocumentSummary {
  return editor.unpack_prefab_instance(artboard, node) as DocumentSummary
}

export function deletePrefab(editor: WasmEditor, prefab: OpaqueId): DocumentSummary {
  return editor.delete_prefab(prefab) as DocumentSummary
}

export function renamePrefab(editor: WasmEditor, prefab: OpaqueId, name: string): DocumentSummary {
  return editor.rename_prefab(prefab, name) as DocumentSummary
}

/** Scripts' `ctx.spawn` (Playground only, no undo; removed when play ends).
 * Every spawned node, root first, with its prefab name — `null` if no
 * prefab has that name. */
export function spawnPrefab(editor: WasmEditor, artboard: OpaqueId, prefabName: string, parent: OpaqueId | null, x: number, y: number): SpawnedPart[] | null {
  return editor.spawn_prefab(artboard, prefabName, parent, x, y) as SpawnedPart[] | null
}

/** One node `spawnPrefab` created, with its local transform and fill as spawned. */
export type SpawnedPart = { id: OpaqueId; name: string; transform: Transform2D; fillColor: [number, number, number, number] | null }

/** Scripts' `ref.destroy()`: removes a spawned object, hides an authored one. */
export function destroyNode(editor: WasmEditor, artboard: OpaqueId, node: OpaqueId): void {
  editor.destroy_node(artboard, node)
}

// ---- Project folders (`an1mbyt3_format::project`) ----

export function saveProjectFiles(editor: WasmEditor, projectName: string): ProjectFileListing[] {
  return editor.save_project(projectName) as ProjectFileListing[]
}

export function takeProjectFileBytes(editor: WasmEditor, index: number): Uint8Array {
  return editor.take_project_file_bytes(index)
}

export function projectAssetPaths(manifest: string): string[] {
  return WasmEditor.project_asset_paths(manifest)
}

export async function loadProjectFiles(editor: WasmEditor, manifest: string, files: { path: string; bytes: Uint8Array }[]): Promise<DocumentSummary> {
  const total = files.reduce((n, f) => n + f.bytes.length, 0)
  const blob = new Uint8Array(total)
  const ends = new Uint32Array(files.length)
  let offset = 0
  files.forEach((f, i) => {
    blob.set(f.bytes, offset)
    offset += f.bytes.length
    ends[i] = offset
  })
  return (await editor.load_project(manifest, files.map((f) => f.path), ends, blob)) as DocumentSummary
}

/** Unsaved-changes tracking — bumped by every edit/undo/redo. */
export function documentRevision(editor: WasmEditor): number {
  return editor.document_revision()
}

/** Imports an image file as an asset only (no canvas node). */
export async function importImageAsset(editor: WasmEditor, filename: string, bytes: Uint8Array): Promise<ImportImageResult> {
  return (await editor.import_image_asset(filename, bytes)) as ImportImageResult
}

/** Changes whenever the whole document is replaced (load, demo, import). */
export function documentInstance(editor: WasmEditor): number {
  return editor.document_instance()
}

/** Project file -> document item mapping (no contents). */
export function projectFileIndex(editor: WasmEditor, projectName: string): ProjectFileListing[] {
  return editor.project_file_index(projectName) as ProjectFileListing[]
}

/** Links an image asset to its file in the project folder (`null` unlinks). */
export function setImageFile(editor: WasmEditor, image: OpaqueId, path: string | null): void {
  editor.set_image_file(image, path ?? undefined)
}

/** Places an existing image asset as a sprite centered at `(x, y)` (drag from Resources). */
export function addImageNode(editor: WasmEditor, artboard: OpaqueId, parent: OpaqueId | null, image: OpaqueId, x: number, y: number): AddNodeResult {
  return editor.add_image_node(artboard, parent, image, x, y) as AddNodeResult
}

// ---- Drawings (Paint mode's "Save as Resource", `an1mbyt3_core::drawing`) ----

/** Saves a vector drawing (scratch-paint's SVG) as a new Drawing resource. Throws if the SVG is empty/unreadable. */
export function addDrawing(editor: WasmEditor, name: string, svg: string): DrawingResult {
  return editor.add_drawing(name, svg) as DrawingResult
}

/** Saves a bitmap-mode drawing (PNG bytes) as a new Drawing resource. */
export async function addBitmapDrawing(editor: WasmEditor, name: string, png: Uint8Array): Promise<DrawingResult> {
  return (await editor.add_bitmap_drawing(name, png)) as DrawingResult
}

/** Re-saves an existing drawing's vector artwork. */
export function updateDrawing(editor: WasmEditor, drawing: OpaqueId, svg: string): DocumentSummary {
  return editor.update_drawing(drawing, svg) as DocumentSummary
}

/** Re-saves an existing drawing as a bitmap. */
export async function updateBitmapDrawing(editor: WasmEditor, drawing: OpaqueId, png: Uint8Array): Promise<DocumentSummary> {
  return (await editor.update_bitmap_drawing(drawing, png)) as DocumentSummary
}

export function renameDrawing(editor: WasmEditor, drawing: OpaqueId, name: string): DocumentSummary {
  return editor.rename_drawing(drawing, name) as DocumentSummary
}

export function deleteDrawing(editor: WasmEditor, drawing: OpaqueId): DocumentSummary {
  return editor.delete_drawing(drawing) as DocumentSummary
}

/** A vector drawing's SVG (to reopen it in Paint / draw a thumbnail); `null` for bitmap drawings. */
export function drawingSvg(editor: WasmEditor, drawing: OpaqueId): string | null {
  return (editor.drawing_svg(drawing) as string | undefined) ?? null
}

/** Places a drawing centred at `(x, y)` in `parent`'s space: a Group of its shapes (vector) or an Image node (bitmap). */
export function placeDrawing(editor: WasmEditor, artboard: OpaqueId, drawing: OpaqueId, parent: OpaqueId | null, x: number, y: number): AddNodeResult {
  return editor.place_drawing(artboard, drawing, parent, x, y) as AddNodeResult
}

/** Saves a frames × layers drawing (Paint's frame strip) as a new Drawing resource. */
export function addAnimatedDrawing(editor: WasmEditor, name: string, value: unknown): DrawingResult {
  return editor.add_animated_drawing(name, value) as DrawingResult
}

/** Re-saves a drawing as frames × layers. */
export function updateAnimatedDrawing(editor: WasmEditor, drawing: OpaqueId, value: unknown): DocumentSummary {
  return editor.update_animated_drawing(drawing, value) as DocumentSummary
}

/** A bitmap cel's PNG as a new image asset; returns its id. */
export async function addDrawingCelImage(editor: WasmEditor, name: string, png: Uint8Array): Promise<OpaqueId> {
  return (await editor.add_drawing_cel_image(name, png)) as OpaqueId
}

/** A drawing's full content (`an1mbyt3_core::DrawingContent` as JSON). */
export function drawingContent(editor: WasmEditor, drawing: OpaqueId): unknown {
  return editor.drawing_content(drawing)
}
