/**
 * Runs a scene's Code programs each frame — shared by the editor's
 * Playground (`App.tsx`) and the exported standalone player
 * (`src/player/main.ts`), so a game behaves identically in both.
 *
 * What it owns:
 * - per-layer state clocks (`ctx.time` restarts whenever *that* layer
 *   changes state),
 * - collecting the active Code program of the base layer and every extra
 *   state-machine layer (base first),
 * - building each program's `ScriptContext` against the engine's untracked
 *   `preview*` / physics / state-machine calls.
 *
 * What the caller supplies each frame (`ScriptFrame`): the engine, the
 * fresh summaries, input state, and the camera hooks — everything that
 * differs between "inside the editor" and "standalone".
 */
import type { ArtboardSummary, OpaqueId, StateMachineDefSummary, StateMachineSummary } from '../ipc/document'
import { SceneMirror } from './sceneMirror'
import { gameStorage } from './storage'
import {
  deleteNode,
  duplicateNodes,
  destroyNode,
  spawnPrefab,
  emitParticles,
  fireTrigger,
  physicsAddForce,
  physicsAngularVelocity,
  physicsEvents,
  physicsRaycast,
  physicsSetAngularVelocity,
  physicsSetVelocity,
  physicsVelocity,
  playgroundView,
  previewLight,
  previewNodeAnchor,
  previewNodePath,
  previewNodeScale,
  previewTextContent,
  previewTilemapTiles,
  previewBodyType,
  previewBreakJoint,
  previewNode3DTransform,
  previewNode3DAnimation,
  setStateMachineBoolInput,
  setStateMachineEnumInput,
  setStateMachineNumberInput,
} from '../ipc/wasmEditor'
import type { WasmEditor } from '../wasm-pkg/an1mbyt3_wasm'
import { runScript } from '../scriptRuntime'

/** Keyboard names scripts and Blocks see: `" "` -> `"space"`, arrows ->
 * `"up"`/`"down"`/`"left"`/`"right"`, everything else lowercased. */
const NAMED_KEYS: Record<string, string> = {
  ' ': 'space',
  ArrowUp: 'up',
  ArrowDown: 'down',
  ArrowLeft: 'left',
  ArrowRight: 'right',
}

export function normalizeKeyName(key: string): string {
  return NAMED_KEYS[key] ?? key.toLowerCase()
}

/** A finished swipe: direction plus travel in CSS pixels (screen axes, +y down). */
export interface Swipe {
  dir: 'left' | 'right' | 'up' | 'down'
  dx: number
  dy: number
}

/**
 * Turns pointer input (mouse, touch, pen — Pointer Events treat them alike)
 * into the gestures scripts read: `ctx.tap` (pressed and released quickly
 * without moving much — a click or a finger tap) and `ctx.swipe` (a quick
 * flick). Each lasts exactly one frame: call `take()` once per frame.
 * The held "mouse" key and `mouseX/Y` already follow touches too.
 */
export class PointerGestures {
  private start: { x: number; y: number; t: number } | null = null
  private tapped = false
  private swiped: Swipe | null = null

  down(event: PointerEvent): void {
    this.start = { x: event.clientX, y: event.clientY, t: performance.now() }
  }

  up(event: PointerEvent): void {
    const s = this.start
    this.start = null
    if (!s) return
    const dx = event.clientX - s.x
    const dy = event.clientY - s.y
    const dist = Math.hypot(dx, dy)
    const ms = performance.now() - s.t
    if (dist < 12 && ms < 350) this.tapped = true
    else if (dist >= 30 && ms < 600) {
      const dir = Math.abs(dx) > Math.abs(dy) ? (dx > 0 ? 'right' : 'left') : dy > 0 ? 'down' : 'up'
      this.swiped = { dir, dx, dy }
    }
  }

  /** This frame's gestures; clears them. */
  take(): { tap: boolean; swipe: Swipe | null } {
    const out = { tap: this.tapped, swipe: this.swiped }
    this.tapped = false
    this.swiped = null
    return out
  }
}

/** Canvas-relative mouse position -> world coordinates through the
 * Playground camera (the scene's Main Camera), falling back to `camera`
 * when the scene has none. `size` is the canvas's CSS size. */
export function mouseToWorld(
  editor: WasmEditor,
  artboard: ArtboardSummary['id'],
  canvasX: number,
  canvasY: number,
  camera: { pan: { x: number; y: number }; zoom: number },
  size: { width: number; height: number },
): { x: number; y: number } {
  const view = playgroundView(editor, artboard)
  const dpr = window.devicePixelRatio || 1
  const pan = view ? { x: view[0], y: view[1] } : camera.pan
  const zoom = view ? view[2] / dpr : camera.zoom
  return { x: pan.x + (canvasX - size.width / 2) / zoom, y: pan.y - (canvasY - size.height / 2) / zoom }
}

export interface ScriptFrame {
  editor: WasmEditor
  deltaSeconds: number
  /** The scene being played (its nodes come from the host's mirror). */
  scene: { id: OpaqueId; name: string }
  machine: StateMachineDefSummary | undefined
  mouseWorld: { x: number; y: number }
  mouseDelta: { x: number; y: number }
  /** This frame's tap / swipe (`PointerGestures.take()`). */
  gestures: { tap: boolean; swipe: Swipe | null }
  keysDown: ReadonlySet<string>
  pointerLocked: boolean
  requestPointerLock(): void
  exitPointerLock(): void
  camera: { panX: number; panY: number; zoom: number }
  setCamera(panX: number, panY: number, zoom: number): void
  /** `ctx.globals` — survives scene switches, reset per play session. */
  globals: Record<string, any>
}

export class ScriptHost {
  /** Cheap per-frame view of the scene's nodes for scripts. */
  private readonly mirror = new SceneMirror()
  /** Queued per-frame node writes, flushed at the end of every `run`. */
  private readonly writes = new WriteBatch()
  private baseState: string | null = null
  private baseElapsed = 0
  private layerClocks: Record<string, { state: string; elapsed: number }> = {}

  /** Advances the per-layer clocks from this frame's state machine summary.
   * Returns `true` when the base layer changed state (callers that mirror
   * the summary into UI use it to avoid per-frame updates). */
  tick(summary: StateMachineSummary, deltaSeconds: number): boolean {
    let changed = false
    if (summary.activeState !== this.baseState) {
      this.baseState = summary.activeState
      this.baseElapsed = 0
      changed = true
    } else {
      this.baseElapsed += deltaSeconds
    }
    for (const [layer, state] of Object.entries(summary.layerActiveStates ?? {})) {
      const clock = this.layerClocks[layer]
      if (!clock || clock.state !== state) this.layerClocks[layer] = { state, elapsed: 0 }
      else clock.elapsed += deltaSeconds
    }
    return changed
  }

  /** Runs every active Code program (base layer, then each extra layer).
   * Returns the scene name a script asked to `loadScene`, if any — the
   * caller switches scenes *after* the frame, so no script ever sees half
   * of one scene and half of another. */
  run(summary: StateMachineSummary, frame: ScriptFrame): string | null {
    const { editor, machine, scene } = frame
    const programs: { layer: string | null; state: string; source: string; time: number }[] = []
    const base = machine?.states.find((s) => s.name === summary.activeState)
    if (base?.program?.kind === 'Code') programs.push({ layer: null, state: base.name, source: base.program.value, time: this.baseElapsed })
    for (const layer of machine?.layers ?? []) {
      const stateName = summary.layerActiveStates?.[layer.name]
      const state = layer.states.find((s) => s.name === stateName)
      if (state?.program?.kind === 'Code') programs.push({ layer: layer.name, state: state.name, source: state.program.value, time: this.layerClocks[layer.name]?.elapsed ?? 0 })
    }
    if (programs.length === 0) return null

    const artboardId = scene.id
    const nodes = this.mirror.update(editor, artboardId)
    let pendingScene: string | null = null
    // One batch for the host's lifetime, not per frame: scripts keep refs
    // across frames (in `ctx.globals`), and a ref's setters belong to the
    // frame it was made in — they must still land in a batch that flushes.
    const writes = this.writes
    writes.target(editor, artboardId)
    const flushed = <T,>(f: () => T): T => {
      writes.flush()
      return f()
    }
    // Drained once per frame, shared by every layer's script.
    const contactEvents = physicsEvents(editor, artboardId)
    const inputKinds = new Map((machine?.inputs ?? []).map((i) => [i.name, i.kind]))
    const setInput = (name: string, value: boolean | number | string) => {
      const kind = inputKinds.get(name)
      if (kind === 'Bool') setStateMachineBoolInput(editor, name, Boolean(value))
      else if (kind === 'Number') setStateMachineNumberInput(editor, name, Number(value))
      else if (kind === 'Enum') setStateMachineEnumInput(editor, name, String(value))
      else if (kind === 'Trigger') {
        if (value) fireTrigger(editor, name)
      } else console.warn(`ctx.setInput: state machine has no input "${name}"`)
    }
    for (const program of programs) {
      runScript(program.source, {
        time: program.time,
        layer: program.layer,
        state: program.state,
        setInput,
        fireTrigger: (name) => setInput(name, true),
        deltaSeconds: frame.deltaSeconds,
        mouseX: frame.mouseWorld.x,
        mouseY: frame.mouseWorld.y,
        nodes,
        images: this.mirror.images,
        keysDown: frame.keysDown,
        camera: frame.camera,
        setCamera: frame.setCamera,
        scene: scene.name,
        loadScene: (name) => {
          pendingScene = name
        },
        globals: frame.globals,
        storage: gameStorage,
        // Hot per-frame writes are queued and sent in one call (`writes`);
        // everything else flushes the queue first so engine state is
        // up to date before it's read or changed some other way.
        setTranslation: (node, x, y) => writes.push(1, node, x, y),
        setRotationDegrees: (node, degrees) => writes.push(2, node, degrees),
        // The scale/anchor engine calls also set the position; the ref
        // passes its own live x/y so an earlier `ref.x = …` this frame sticks.
        setScale: (node, scaleX, scaleY, tx, ty) => flushed(() => previewNodeScale(editor, artboardId, node, tx, ty, scaleX, scaleY)),
        setAnchor: (node, anchorX, anchorY, tx, ty) => flushed(() => previewNodeAnchor(editor, artboardId, node, anchorX, anchorY, tx, ty)),
        setFillColor: (node, r, g, b, a) => writes.push(6, node, r, g, b, a),
        setTextContent: (node, value) => flushed(() => previewTextContent(editor, artboardId, node, value)),
        setImage: (node, image) => writes.push(5, node, image.idx, image.version),
        setPath: (node, value) => flushed(() => previewNodePath(editor, artboardId, node, value)),
        tap: frame.gestures.tap,
        swipe: frame.gestures.swipe,
        mouseDX: frame.mouseDelta.x,
        mouseDY: frame.mouseDelta.y,
        pointerLocked: frame.pointerLocked,
        requestPointerLock: frame.requestPointerLock,
        exitPointerLock: frame.exitPointerLock,
        setZIndex: (node, value) => writes.push(3, node, value),
        setTiles: (node, tiles) => flushed(() => previewTilemapTiles(editor, artboardId, node, tiles)),
        setBodyType: (node, bodyType, deep) => flushed(() => previewBodyType(editor, artboardId, node, bodyType, deep)),
        breakJoint: (node) => flushed(() => previewBreakJoint(editor, artboardId, node)),
        setTransform3D: (node, values) => flushed(() => previewNode3DTransform(editor, artboardId, node, values)),
        setAnimation3D: (node, clip, loop, overlay) => flushed(() => previewNode3DAnimation(editor, artboardId, node, clip, loop, overlay)),
        setLight: (node, intensity, radius) => flushed(() => previewLight(editor, artboardId, node, intensity, radius)),
        emitParticles: (node, count) => flushed(() => emitParticles(editor, artboardId, node, count)),
        physics: {
          velocity: (node) => flushed(() => physicsVelocity(editor, artboardId, node)),
          setVelocity: (node, x, y) => flushed(() => physicsSetVelocity(editor, artboardId, node, x, y)),
          angularVelocity: (node) => flushed(() => physicsAngularVelocity(editor, artboardId, node)),
          setAngularVelocity: (node, dps) => flushed(() => physicsSetAngularVelocity(editor, artboardId, node, dps)),
          addForce: (node, x, y, impulse) => flushed(() => physicsAddForce(editor, artboardId, node, x, y, impulse)),
          raycast: (ox, oy, dx, dy, maxDistance, exclude) => flushed(() => physicsRaycast(editor, artboardId, ox, oy, dx, dy, maxDistance, exclude)),
          events: contactEvents,
        },
        // Replaced by `runScript` with ref-returning versions.
        raycast: () => null,
        collisions: [],
        setVisible: (node, value) => writes.push(4, node, value ? 1 : 0),
        spawnClone: (node) => flushed(() => duplicateNodes(editor, artboardId, [node]).newNodes[0] ?? null),
        // Replaced by `runScript` with the ref-returning version.
        spawn: () => null,
        spawnPrefab: (prefabName, parent, x, y) => flushed(() => spawnPrefab(editor, artboardId, prefabName, parent, x, y)),
        destroyNode: (node) => flushed(() => destroyNode(editor, artboardId, node)),
        deleteNode: (node) => {
          flushed(() => deleteNode(editor, artboardId, node))
        },
      })
    }
    writes.flush()
    return pendingScene
  }
}

/**
 * A frame's queued node writes (`WasmEditor::apply_node_writes` records:
 * `[op, node idx, node version, a, b, c, d]`), sent in one wasm call.
 */
class WriteBatch {
  private buf = new Float64Array(7 * 256)
  private len = 0
  private editor: WasmEditor | null = null
  private artboard: OpaqueId | null = null

  /** Where the next flush goes (the scene being played). */
  target(editor: WasmEditor, artboard: OpaqueId): void {
    if (this.artboard && (this.artboard.idx !== artboard.idx || this.artboard.version !== artboard.version)) this.len = 0
    this.editor = editor
    this.artboard = artboard
  }

  push(op: number, node: OpaqueId, a = 0, b = 0, c = 0, d = 0): void {
    if (this.len + 7 > this.buf.length) {
      const bigger = new Float64Array(this.buf.length * 2)
      bigger.set(this.buf)
      this.buf = bigger
    }
    const o = this.len
    this.buf[o] = op
    this.buf[o + 1] = node.idx
    this.buf[o + 2] = node.version
    this.buf[o + 3] = a
    this.buf[o + 4] = b
    this.buf[o + 5] = c
    this.buf[o + 6] = d
    this.len += 7
  }

  flush(): void {
    if (this.len === 0 || !this.editor || !this.artboard) return
    const records = this.buf.subarray(0, this.len)
    this.len = 0
    this.editor.apply_node_writes(this.artboard, records)
  }
}
