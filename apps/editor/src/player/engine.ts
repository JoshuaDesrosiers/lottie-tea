/**
 * The embeddable game player: one an1mbyt3 game running in a box on any web
 * page, steerable from the page — the core of both the standalone "Export
 * Game" page (`player/main.ts`) and the React / JS embed (`src/embed`).
 *
 * It runs the *same* engine the editor does (`WasmEditor` in Playground
 * mode, scripts through `ScriptHost`), so a game behaves exactly as in the
 * editor's Playground. On top of that it adds the page link:
 *
 * - **Data in** — `set(name, value)` / `setData({...})`: kept across
 *   scenes, readable by scripts as `ctx.data`, and written into same-named
 *   scene Properties (so Property Bindings / HTML `data-bind` update a
 *   label, colour or toggle with no script). Applied at the next frame —
 *   cheap enough to call on every fetch / keystroke.
 * - **Control** — `setInput` / `fire` (state machine inputs and triggers),
 *   `loadScene`, `call` (functions scripts registered with `ctx.on`),
 *   `pause` / `play`.
 * - **Events out** — `on('state' | 'scene' | 'prop' | 'event' | …)`: state
 *   machine state changes (per layer), scene switches, scene Property
 *   changes made by the game, and `ctx.emit(name, payload)` from scripts
 *   (or On Click "Call script function" with no script behind it). Every
 *   event also fires as a DOM `CustomEvent` (`an1:state`, …) on the
 *   container.
 *
 * One player per page at a time: the engine's HTML UI overlay, scene
 * properties and script state are page-wide. Creating a second player
 * destroys the first.
 */
import {
  createWasmEditor,
  fireTrigger as engineFireTrigger,
  getStateMachineSummary,
  loadTeab,
  playgroundView,
  setActiveArtboard,
  setKeyDown,
  setMode,
  setStateMachineBoolInput,
  setStateMachineEnumInput,
  setStateMachineNumberInput,
  setStateProgram,
  setWasmBytes,
} from '../ipc/wasmEditor'
import { setPropertyDefs, sceneProps } from '../game/props'
import { setProjectFiles } from '../game/projectFiles'
import type { ArtboardSummary, DocumentSummary, OpaqueId } from '../ipc/document'
import { forwardMouseKeys, normalizeKeyName, PointerGestures, ScriptHost, syncMouseButtons } from '../game/scriptHost'
import { adaptiveResolution, renderDpr } from '../game/adaptiveResolution'
import { PerfMeter } from '../game/perf'
import { responsiveView } from '../game/screen'
import { htmlOverlay } from '../game/htmlOverlay'
import { embedBridge } from '../game/embedBridge'
import { callFunction } from '../game/events'
import { gameConsole } from '../game/console'

/** Bytes, or a URL to fetch them from. */
export type Source = string | URL | ArrayBuffer | Uint8Array

export interface PlayerOptions {
  /** The game: a `.teab` file (URL or bytes). */
  game: Source
  /** The engine (`an1mbyt3_wasm_bg.wasm`). Default: next to `game` when
   * that's a URL. */
  wasm?: Source
  /** Scene to open on (by name). Default: the first scene. */
  scene?: string
  /** How the game's frame fits the box: `contain` (letterboxed, default)
   * or `cover` (fills, cropping the overflow). Responsive scenes always
   * fill the box. */
  fit?: 'contain' | 'cover'
  /** Which keys reach the game: `focus` (default for embeds — only while
   * the canvas has focus; click it to focus), `window` (every key on the
   * page, like the exported game page), `none`. */
  keyboard?: 'focus' | 'window' | 'none'
  /** Starting data (same as calling `set` for each). */
  data?: Record<string, unknown>
  /** Start paused. */
  paused?: boolean
  /** Box background (letterbox colour). Default transparent. */
  background?: string
  /** F3 / `?stats` frame profiler. Default off. */
  stats?: boolean
  /** Replaces every state's program, in every scene, with this JavaScript
   * (a Code program). For playgrounds and teaching: the same scene runs
   * whatever code the page hands it. */
  program?: string
}

export interface StateChange {
  /** `null` = the base layer, else the layer's name. */
  layer: string | null
  state: string
  previous: string | null
}

export interface PlayerEvents {
  ready: { scenes: string[]; scene: string }
  scene: { scene: string; previous: string | null }
  state: StateChange
  prop: { name: string; value: unknown }
  event: { name: string; payload: unknown }
  error: { error: unknown }
  /** A line in the game's console: `console.log` / `print` from scripts,
   * script errors (with their line), warnings from actions. */
  log: { level: 'log' | 'warn' | 'error'; message: string; line?: number; count: number }
}

export type PlayerEventType = keyof PlayerEvents

export interface An1Player {
  readonly container: HTMLElement
  readonly canvas: HTMLCanvasElement
  /** Every scene's name. */
  readonly scenes: string[]
  /** The scene playing. */
  readonly scene: string | null
  /** The base layer's current state (null without a state machine). */
  readonly state: string | null
  /** Each extra state machine layer's current state. */
  readonly layerStates: Readonly<Record<string, string>>
  readonly paused: boolean
  /** Data for the game (`ctx.data.name`, and the scene Property `name`
   * if there is one). Kept across scenes. Applied next frame. */
  set(name: string, value: unknown): void
  /** Several at once. */
  setData(values: Record<string, unknown>): void
  /** A scene Property's current value (else the last `set` value). */
  get(name: string): unknown
  /** A state machine input (Bool / Number / Enum, or a Trigger when
   * `value` is truthy). */
  setInput(name: string, value: boolean | number | string): void
  /** Fires a state machine trigger. */
  fire(trigger: string): void
  /** Switches scene (by name), after the current frame. */
  loadScene(name: string): void
  /** Calls a script function registered with `ctx.on(name, fn)`; returns
   * its result (`undefined` and a warning if none). */
  call(name: string, arg?: unknown): unknown
  on<T extends PlayerEventType>(type: T, listener: (event: PlayerEvents[T]) => void): () => void
  off<T extends PlayerEventType>(type: T, listener: (event: PlayerEvents[T]) => void): void
  /** Resolves the next time the base layer (or `layer`) enters `state`
   * (right away if it's already there). */
  waitForState(state: string, layer?: string | null): Promise<void>
  pause(): void
  play(): void
  /** Stops the game and removes the canvas. */
  destroy(): void
}

let current: { destroy(): void } | null = null

async function bytesOf(source: Source): Promise<Uint8Array<ArrayBuffer>> {
  if (source instanceof Uint8Array) return new Uint8Array(source)
  if (source instanceof ArrayBuffer) return new Uint8Array(source)
  const response = await fetch(source)
  if (!response.ok) throw new Error(`couldn't load ${String(source)} (${response.status})`)
  return new Uint8Array(await response.arrayBuffer())
}

const sameId = (a: OpaqueId, b: OpaqueId) => a.idx === b.idx && a.version === b.version
const sameValue = (a: unknown, b: unknown) => a === b || (typeof a === 'object' && typeof b === 'object' && JSON.stringify(a) === JSON.stringify(b))

export async function createPlayer(container: HTMLElement, options: PlayerOptions): Promise<An1Player> {
  current?.destroy()
  const cleanups: (() => void)[] = []
  const listen = <K extends string>(target: EventTarget, type: K, fn: EventListenerOrEventListenerObject, opts?: AddEventListenerOptions | boolean) => {
    target.addEventListener(type, fn, opts)
    cleanups.push(() => target.removeEventListener(type, fn, opts))
  }

  // ---- the box ----
  // container > stage (canvas-sized, holds the HTML UI layer) > canvas
  const containerStyle = getComputedStyle(container)
  if (containerStyle.position === 'static') container.style.position = 'relative'
  container.style.overflow = 'hidden'
  if (options.background) container.style.background = options.background
  const stage = document.createElement('div')
  stage.style.cssText = 'position:absolute;left:50%;top:50%;transform:translate(-50%,-50%);line-height:0;'
  const canvas = document.createElement('canvas')
  canvas.tabIndex = 0
  canvas.style.cssText = 'display:block;outline:none;touch-action:none;'
  stage.appendChild(canvas)
  container.appendChild(stage)
  cleanups.push(() => stage.remove())
  htmlOverlay.attach(stage, canvas)
  cleanups.push(() => htmlOverlay.detach())

  // ---- listeners ----
  const listeners = new Map<string, Set<(e: unknown) => void>>()
  const dispatch = <T extends PlayerEventType>(type: T, event: PlayerEvents[T]) => {
    for (const fn of listeners.get(type) ?? []) {
      try {
        fn(event)
      } catch (err) {
        console.error(`an1mbyt3: an on('${type}') listener threw`, err)
      }
    }
    // `ready` doesn't bubble: `<an1-player>` fires its own once `.player` is set.
    container.dispatchEvent(new CustomEvent(`an1:${type}`, { detail: event, bubbles: type !== 'ready' }))
  }

  // ---- engine + game ----
  const gameUrl = typeof options.game === 'string' || options.game instanceof URL ? new URL(String(options.game), location.href) : null
  const wasmSource = options.wasm ?? (gameUrl ? new URL('an1mbyt3_wasm_bg.wasm', gameUrl) : null)
  if (!wasmSource) throw new Error('an1mbyt3: pass `wasm` (the engine) when `game` is bytes')
  const [wasmBytes, gameBytes] = await Promise.all([bytesOf(wasmSource), bytesOf(options.game)])
  setWasmBytes(wasmBytes)
  const editor = await createWasmEditor(canvas)
  let doc: DocumentSummary = await loadTeab(editor, gameBytes)
  if (options.program !== undefined) {
    const program = { kind: 'Code' as const, value: options.program }
    for (const artboard of doc.artboards) {
      for (const machine of artboard.stateMachines) {
        for (const state of machine.states) doc = setStateProgram(editor, artboard.id, machine.id, state.name, program, null)
        for (const layer of machine.layers ?? []) for (const state of layer.states) doc = setStateProgram(editor, artboard.id, machine.id, state.name, program, layer.name)
      }
    }
  }
  setProjectFiles(doc.textFiles)
  setPropertyDefs(doc.properties)
  embedBridge.reset()

  // ---- sizing ----
  let cssSize = { width: 1, height: 1 }
  let frameSize = { width: 1, height: 1 }
  let sceneId: OpaqueId | null = null
  const currentScene = (): ArtboardSummary | null => doc.artboards.find((a) => sceneId && sameId(a.id, sceneId)) ?? doc.artboards[0] ?? null
  const applySize = () => {
    const dpr = renderDpr()
    const availW = Math.max(1, container.clientWidth)
    const availH = Math.max(1, container.clientHeight)
    const scene = currentScene()
    const camera = scene?.nodes.find((n) => scene.activeCamera && sameId(n.id, scene.activeCamera))?.camera ?? null
    const zoom = Math.max(0.01, camera?.zoom ?? 1)
    let frameW = (scene?.width ?? availW) / zoom
    let frameH = (scene?.height ?? availH) / zoom
    if (scene?.screen.mode === 'Responsive') {
      const view = responsiveView({ width: frameW, height: frameH }, { width: availW, height: availH }, scene.screen)
      frameW = view.width
      frameH = view.height
    }
    frameSize = { width: frameW, height: frameH }
    const fit = (options.fit === 'cover' ? Math.max : Math.min)(availW / frameW, availH / frameH)
    cssSize = { width: frameW * fit, height: frameH * fit }
    const pixelPerfect = !!camera?.pixelPerfect.enabled
    const renderScale = pixelPerfect ? Math.max(1, Math.floor(fit * dpr)) : fit * dpr
    const deviceW = Math.max(1, Math.round(frameW * renderScale))
    const deviceH = Math.max(1, Math.round(frameH * renderScale))
    canvas.width = deviceW
    canvas.height = deviceH
    canvas.style.width = `${cssSize.width}px`
    canvas.style.height = `${cssSize.height}px`
    canvas.style.imageRendering = pixelPerfect ? 'pixelated' : 'auto'
    editor.resize(deviceW, deviceH)
  }
  const resizeObserver = new ResizeObserver(() => applySize())
  resizeObserver.observe(container)
  cleanups.push(() => resizeObserver.disconnect())
  adaptiveResolution.onChange = applySize
  cleanups.push(() => {
    if (adaptiveResolution.onChange === applySize) adaptiveResolution.onChange = null
  })

  // ---- data ----
  const hostData = new Map<string, unknown>()
  /** Props last seen (to report the game's own changes). */
  const seenProps = new Map<string, unknown>()
  let dataDirty = false
  /** Keys the page sent since the last push. Only these are written on a
   * normal frame, so a value the game changed itself (a score, lives) isn't
   * overwritten when the page sends some *other* key; after a scene switch
   * (`all`) every page value is applied again. */
  const pendingKeys = new Set<string>()
  const pushData = (all = false) => {
    if (!sceneId) return
    sceneProps.ensureScene(sceneId)
    for (const [name, value] of hostData) {
      if (!all && !pendingKeys.has(name)) continue
      if (sceneProps.has(name) && !sameValue(sceneProps.get(name), value)) {
        sceneProps.set(name, value)
      }
      if (sceneProps.has(name)) seenProps.set(name, sceneProps.get(name))
    }
    pendingKeys.clear()
    dataDirty = false
  }
  for (const [name, value] of Object.entries(options.data ?? {})) {
    hostData.set(name, value)
    embedBridge.setData(name, value)
  }

  // ---- scenes ----
  let sceneName: string | null = null
  let pendingScene: string | null = null
  const enterScene = (id: OpaqueId) => {
    doc = setActiveArtboard(editor, id)
    doc = setMode(editor, id, 'Playground')
    sceneId = id
    const previous = sceneName
    sceneName = doc.artboards.find((a) => sameId(a.id, id))?.name ?? null
    applySize()
    sceneProps.ensureScene(id)
    seenProps.clear()
    for (const name of sceneProps.names()) seenProps.set(name, sceneProps.get(name))
    pushData(true)
    if (sceneName !== null) dispatch('scene', { scene: sceneName, previous })
  }
  const start = doc.artboards.find((a) => a.name === options.scene) ?? doc.artboards[0]
  if (!start) throw new Error('an1mbyt3: this game has no scenes')

  // ---- input ----
  const keys = new Set<string>()
  let mouseClient = { x: 0, y: 0 }
  let mouseDelta = { x: 0, y: 0 }
  let pointerLockArmed = false
  const keyboard = options.keyboard ?? 'focus'
  const forwardKey = (event: KeyboardEvent, pressed: boolean) => {
    const key = normalizeKeyName(event.key)
    if (['space', 'up', 'down', 'left', 'right'].includes(key)) event.preventDefault()
    if (pressed) keys.add(key)
    else keys.delete(key)
    setKeyDown(editor, key, pressed)
  }
  if (keyboard !== 'none') {
    const keyTarget: EventTarget = keyboard === 'window' ? window : canvas
    listen(keyTarget, 'keydown', (e) => forwardKey(e as KeyboardEvent, true))
    listen(keyTarget, 'keyup', (e) => forwardKey(e as KeyboardEvent, false))
    listen(keyboard === 'window' ? window : canvas, 'blur', () => {
      for (const key of keys) setKeyDown(editor, key, false)
      keys.clear()
    })
  }
  const gestures = new PointerGestures()
  listen(window, 'pointermove', (ev) => {
    const e = ev as PointerEvent
    mouseDelta = { x: mouseDelta.x + e.movementX, y: mouseDelta.y + e.movementY }
    mouseClient = { x: e.clientX, y: e.clientY }
    if (keys.has('mouse')) syncMouseButtons(keys, e)
  })
  listen(canvas, 'pointerdown', (ev) => {
    const e = ev as PointerEvent
    mouseClient = { x: e.clientX, y: e.clientY }
    gestures.down(e)
    keys.add('mouse')
    syncMouseButtons(keys, e)
    forwardMouseKeys(keys, (k, p) => setKeyDown(editor, k, p))
    if (keyboard === 'focus') canvas.focus({ preventScroll: true })
    if (pointerLockArmed && document.pointerLockElement !== canvas) void canvas.requestPointerLock()
  })
  listen(window, 'pointerup', (ev) => {
    const e = ev as PointerEvent
    if (keys.has('mouse')) gestures.up(e)
    syncMouseButtons(keys, e)
    if (!e.buttons) keys.delete('mouse')
    forwardMouseKeys(keys, (k, p) => setKeyDown(editor, k, p))
  })
  listen(window, 'pointercancel', () => {
    for (const k of ['mouse', 'mouseleft', 'mouseright', 'mousemiddle']) keys.delete(k)
    forwardMouseKeys(keys, (k, p) => setKeyDown(editor, k, p))
  })
  listen(canvas, 'contextmenu', (e) => e.preventDefault())
  let wheel = { x: 0, y: 0 }
  listen(
    canvas,
    'wheel',
    (ev) => {
      const e = ev as WheelEvent
      e.preventDefault()
      wheel = { x: wheel.x + e.deltaX, y: wheel.y + e.deltaY }
    },
    { passive: false },
  )

  const screenToGame = (scene: OpaqueId, x: number, y: number, w: number, h: number) => {
    const view = playgroundView(editor, scene)
    const pan = view ? { x: view[0], y: view[1] } : camera.pan
    const perPx = view ? frameSize.width / Math.max(1, w) : 1 / Math.max(1e-6, camera.zoom * (w / Math.max(1, cssSize.width)))
    return { x: pan.x + (x - w / 2) * perPx, y: pan.y - (y - h / 2) * perPx }
  }

  // ---- game → page ----
  embedBridge.listen((name, payload) => dispatch('event', { name, payload }))
  cleanups.push(() => embedBridge.listen(null))
  // Console lines (new ones and repeats) as `log` events.
  const logCounts = new Map<number, number>()
  for (const e of gameConsole.list()) logCounts.set(e.id, e.count)
  cleanups.push(
    gameConsole.subscribe(() => {
      for (const e of gameConsole.list()) {
        if (logCounts.get(e.id) === e.count) continue
        logCounts.set(e.id, e.count)
        const level = e.level === 'error' ? 'error' : e.level === 'warn' ? 'warn' : 'log'
        dispatch('log', { level, message: e.message, line: e.origin.line, count: e.count })
      }
    }),
  )

  // ---- frame loop ----
  const host = new ScriptHost()
  const perf = new PerfMeter()
  if (options.stats || new URLSearchParams(location.search).has('stats')) perf.toggle(container)
  // F3 / backtick toggles the profiler when the game owns the keyboard.
  if (keyboard !== 'none') {
    listen(keyboard === 'window' ? window : canvas, 'keydown', (ev) => {
      const e = ev as KeyboardEvent
      if (e.key === 'F3' || e.key === '`') {
        e.preventDefault()
        perf.toggle(container)
      }
    })
  }
  const globals: Record<string, any> = {}
  let lastTime: number | null = null
  let frameErrors = 0
  let paused = !!options.paused
  let rafId = 0
  let destroyed = false
  const fitCamera = (artboard: ArtboardSummary) => ({
    pan: { x: 0, y: 0 },
    zoom: Math.min(cssSize.width / Math.max(1, artboard.width), cssSize.height / Math.max(1, artboard.height)),
  })
  let camera = { pan: { x: 0, y: 0 }, zoom: 1 }
  let cameraOverridden = false
  let baseState: string | null = null
  const layerStates: Record<string, string> = {}

  const reportChanges = () => {
    const summary = getStateMachineSummary(editor)
    if (summary.activeState && summary.activeState !== baseState) {
      const previous = baseState
      baseState = summary.activeState
      dispatch('state', { layer: null, state: baseState, previous })
    }
    for (const [layer, state] of Object.entries(summary.layerActiveStates ?? {})) {
      if (layerStates[layer] !== state) {
        const previous = layerStates[layer] ?? null
        layerStates[layer] = state
        dispatch('state', { layer, state, previous })
      }
    }
    for (const name of sceneProps.names()) {
      const value = sceneProps.get(name)
      if (!seenProps.has(name) || !sameValue(seenProps.get(name), value)) {
        seenProps.set(name, Array.isArray(value) ? [...value] : value)
        dispatch('prop', { name, value })
      }
    }
    return summary
  }

  const frame = (now: number) => {
    if (destroyed) return
    rafId = requestAnimationFrame(frame)
    if (paused) {
      lastTime = null
      return
    }
    try {
      const deltaSeconds = lastTime === null ? 0 : Math.min((now - lastTime) / 1000, 0.1)
      lastTime = now
      if (dataDirty) pushData()
      const dpr = renderDpr()
      const artboard = currentScene()
      if (artboard && !cameraOverridden) camera = fitCamera(artboard)
      editor.render(deltaSeconds, camera.pan.x, camera.pan.y, camera.zoom * dpr, -1, dpr, 0, 0, 0, 0, 0, 0)

      const syncStart = performance.now()
      const summary = getStateMachineSummary(editor)
      host.tick(summary, deltaSeconds)
      perf.add('sync', performance.now() - syncStart)
      const live = doc.artboards.find((a) => sceneId && sameId(a.id, sceneId))
      let requested: string | null = null
      if (live) {
        const machine = live.stateMachines.find((m) => live.activeStateMachine && sameId(m.id, live.activeStateMachine))
        const rect = canvas.getBoundingClientRect()
        const scriptsStart = performance.now()
        requested = host.run(summary, {
          editor,
          deltaSeconds,
          scene: { id: live.id, name: live.name },
          machine,
          mouseWorld: screenToGame(live.id, mouseClient.x - rect.left, mouseClient.y - rect.top, rect.width, rect.height),
          mouseDelta,
          wheel,
          gestures: gestures.take(),
          keysDown: keys,
          pointerLocked: document.pointerLockElement === canvas,
          requestPointerLock: () => {
            pointerLockArmed = true
          },
          exitPointerLock: () => {
            pointerLockArmed = false
            if (document.pointerLockElement) document.exitPointerLock()
          },
          camera: { panX: camera.pan.x, panY: camera.pan.y, zoom: camera.zoom },
          setCamera: (panX, panY, zoom) => {
            camera = { pan: { x: panX, y: panY }, zoom }
            cameraOverridden = true
          },
          globals,
        })
        perf.add('scripts', performance.now() - scriptsStart)
      }
      reportChanges()
      const target = pendingScene ?? requested
      pendingScene = null
      if (target !== null) {
        const scene = doc.artboards.find((a) => a.name === target)
        if (scene) {
          cameraOverridden = false
          enterScene(scene.id)
        } else console.warn(`an1mbyt3: no scene named "${target}"`)
      }
      mouseDelta = { x: 0, y: 0 }
      wheel = { x: 0, y: 0 }
      perf.endFrame(editor, deltaSeconds * 1000, now)
      adaptiveResolution.update(editor, deltaSeconds * 1000, now, true)
    } catch (err) {
      frameErrors++
      if (frameErrors <= 3 || frameErrors % 600 === 0) {
        console.error(`an1mbyt3: frame threw (#${frameErrors}) — skipping it`, err)
        dispatch('error', { error: err })
      }
    }
  }

  // ---- the API ----
  const inputKind = (name: string) => {
    const live = currentScene()
    const machine = live?.stateMachines.find((m) => live.activeStateMachine && sameId(m.id, live.activeStateMachine))
    return machine?.inputs.find((i) => i.name === name)?.kind
  }
  const player: An1Player = {
    container,
    canvas,
    get scenes() {
      return doc.artboards.map((a) => a.name)
    },
    get scene() {
      return sceneName
    },
    get state() {
      return baseState
    },
    get layerStates() {
      return { ...layerStates }
    },
    get paused() {
      return paused
    },
    set(name, value) {
      hostData.set(name, value)
      embedBridge.setData(name, value)
      pendingKeys.add(name)
      dataDirty = true
    },
    setData(values) {
      for (const [name, value] of Object.entries(values)) player.set(name, value)
    },
    get(name) {
      return sceneProps.has(name) ? sceneProps.get(name) : hostData.get(name)
    },
    setInput(name, value) {
      const kind = inputKind(name)
      if (kind === 'Bool') setStateMachineBoolInput(editor, name, Boolean(value))
      else if (kind === 'Number') setStateMachineNumberInput(editor, name, Number(value))
      else if (kind === 'Enum') setStateMachineEnumInput(editor, name, String(value))
      else if (kind === 'Trigger') {
        if (value) engineFireTrigger(editor, name)
      } else console.warn(`an1mbyt3: the scene's state machine has no input "${name}"`)
    },
    fire(trigger) {
      engineFireTrigger(editor, trigger)
    },
    loadScene(name) {
      pendingScene = name
    },
    call(name, arg) {
      const { found, result } = callFunction(name, arg)
      if (!found) console.warn(`an1mbyt3: no script function "${name}" (register it with ctx.on('${name}', fn))`)
      return result
    },
    on(type, fn) {
      let set = listeners.get(type)
      if (!set) listeners.set(type, (set = new Set()))
      set.add(fn as (e: unknown) => void)
      return () => player.off(type, fn)
    },
    off(type, fn) {
      listeners.get(type)?.delete(fn as (e: unknown) => void)
    },
    waitForState(state, layer = null) {
      const now = layer === null ? baseState : (layerStates[layer] ?? null)
      if (now === state) return Promise.resolve()
      return new Promise((resolve) => {
        const off = player.on('state', (e) => {
          if (e.state === state && e.layer === layer) {
            off()
            resolve()
          }
        })
      })
    },
    pause() {
      paused = true
    },
    play() {
      paused = false
    },
    destroy() {
      if (destroyed) return
      destroyed = true
      cancelAnimationFrame(rafId)
      for (const key of keys) setKeyDown(editor, key, false)
      for (const cleanup of cleanups.reverse()) cleanup()
      listeners.clear()
      try {
        editor.free()
      } catch {
        // Already freed.
      }
      if (current === handle) current = null
    },
  }
  const handle = { destroy: () => player.destroy() }
  current = handle

  enterScene(start.id)
  reportChanges()
  dispatch('ready', { scenes: player.scenes, scene: sceneName ?? start.name })
  rafId = requestAnimationFrame(frame)
  return player
}
