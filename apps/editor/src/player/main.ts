/**
 * The standalone game player — what "Export Game" ships.
 *
 * An exported game is one self-contained `.html` file (see
 * `game/exportGame.ts`). It carries three payloads in inert `<script>`
 * tags, plus this file's bundled code:
 * - `#an1-config` (JSON): `{ title, startScene }`
 * - `#an1-wasm`   (base64): the engine (`an1mbyt3_wasm_bg.wasm`)
 * - `#an1-game`   (base64): the document (`.teab`)
 *
 * It runs the *same* engine the editor does, `WasmEditor`, in Playground
 * mode. Lighting, particles, tilemaps, physics, cameras, post effects,
 * state-machine layers, Graph/Blocks programs and multiple scenes all
 * behave exactly as in the editor's Playground. Code programs go through
 * the same `ScriptHost` the editor uses.
 */
import {
  createWasmEditor,
  getStateMachineSummary,
  playgroundView,
  loadTeab,
  setActiveArtboard,
  setKeyDown,
  setMode,
  setWasmBytes,
} from '../ipc/wasmEditor'
import type { ArtboardSummary, DocumentSummary, OpaqueId } from '../ipc/document'
import { normalizeKeyName, PointerGestures, ScriptHost } from '../game/scriptHost'
import { PerfMeter } from '../game/perf'
import { responsiveView } from '../game/screen'
import { htmlOverlay } from '../game/htmlOverlay'

interface PlayerConfig {
  title: string
  startScene: string | null
}

function payload(id: string): string {
  const el = document.getElementById(id)
  if (!el?.textContent) throw new Error(`missing ${id}`)
  return el.textContent.trim()
}

function base64Bytes(text: string): Uint8Array<ArrayBuffer> {
  const raw = atob(text)
  const bytes = new Uint8Array(new ArrayBuffer(raw.length))
  for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i)
  return bytes
}

function showMessage(text: string) {
  const el = document.getElementById('an1-status')
  if (!el) return
  el.textContent = text
  el.style.display = text ? 'flex' : 'none'
}

async function boot() {
  const config = JSON.parse(payload('an1-config')) as PlayerConfig
  document.title = config.title
  // No WebGPU check: the engine falls back to WebGL2 by itself (see
  // `new_instance_with_webgpu_detection` in the wasm crate), so older
  // Safari / Firefox / Android browsers run the game too.
  setWasmBytes(base64Bytes(payload('an1-wasm')))
  const canvas = document.getElementById('an1-canvas') as HTMLCanvasElement
  // HTML UI nodes sit in a layer over the canvas: wrap it in a box that is
  // exactly the canvas's size.
  const stage = document.createElement('div')
  stage.style.cssText = 'position:relative;flex:none;line-height:0;'
  canvas.parentElement?.insertBefore(stage, canvas)
  stage.appendChild(canvas)
  htmlOverlay.attach(stage, canvas)

  // The game renders at its own resolution and the canvas is then *scaled*
  // (CSS) to fill as much of the screen as its aspect ratio allows,
  // centered. Pixel-perfect games render at a whole-number multiple of
  // their pixel size and upscale with hard edges, so pixels stay crisp at
  // any screen size.
  let cssSize = { width: 1, height: 1 }
  let frameSize = { width: 1, height: 1 }
  const editor = await createWasmEditor(canvas)
  let doc: DocumentSummary | null = null
  let sceneId: OpaqueId | null = null
  const sameId = (a: OpaqueId, b: OpaqueId) => a.idx === b.idx && a.version === b.version
  const currentScene = () => doc?.artboards.find((a) => sceneId && sameId(a.id, sceneId)) ?? doc?.artboards[0] ?? null
  const applySize = () => {
    const dpr = window.devicePixelRatio || 1
    // The *visual* viewport: on phones `innerHeight`/100vh can include the
    // area under the browser's address bar.
    const vv = window.visualViewport
    const availW = Math.max(1, vv?.width ?? window.innerWidth)
    const availH = Math.max(1, vv?.height ?? window.innerHeight)
    const scene = currentScene()
    const camera = scene?.nodes.find((n) => scene.activeCamera && sameId(n.id, scene.activeCamera))?.camera ?? null
    const zoom = Math.max(0.01, camera?.zoom ?? 1)
    let frameW = (scene?.width ?? availW) / zoom
    let frameH = (scene?.height ?? availH) / zoom
    // Responsive scenes fill the whole page: the view grows / shrinks with
    // it (the engine does the same maths for the camera).
    if (scene?.screen.mode === 'Responsive') {
      const view = responsiveView({ width: frameW, height: frameH }, { width: availW, height: availH }, scene.screen)
      frameW = view.width
      frameH = view.height
    }
    frameSize = { width: frameW, height: frameH }
    // CSS px per game unit: fill the screen (contain).
    const fit = Math.min(availW / frameW, availH / frameH)
    cssSize = { width: frameW * fit, height: frameH * fit }
    const pixelPerfect = !!camera?.pixelPerfect.enabled
    // Render resolution: whole-number pixel scale for pixel art (the browser
    // does the final stretch), otherwise the screen's full resolution.
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
  window.addEventListener('resize', applySize)
  window.visualViewport?.addEventListener('resize', applySize)
  window.addEventListener('orientationchange', () => setTimeout(applySize, 200))

  doc = await loadTeab(editor, base64Bytes(payload('an1-game')))
  const enterScene = (id: OpaqueId) => {
    doc = setActiveArtboard(editor, id)
    doc = setMode(editor, id, 'Playground')
    sceneId = id
    applySize()
  }
  const loaded = doc
  const start = loaded.artboards.find((a) => a.name === config.startScene) ?? loaded.artboards[0]
  if (!start) {
    showMessage('This game has no scenes.')
    return
  }
  enterScene(start.id)
  showMessage('')

  // ---- input (same conventions as the editor's Playground) ----
  const keys = new Set<string>()
  let mouseClient = { x: 0, y: 0 }
  let mouseDelta = { x: 0, y: 0 }
  let pointerLockArmed = false
  const forwardKey = (event: KeyboardEvent, pressed: boolean) => {
    const key = normalizeKeyName(event.key)
    // Keep arrows/space from scrolling the page.
    if (['space', 'up', 'down', 'left', 'right'].includes(key)) event.preventDefault()
    if (pressed) keys.add(key)
    else keys.delete(key)
    setKeyDown(editor, key, pressed)
  }
  window.addEventListener('keydown', (e) => forwardKey(e, true))
  window.addEventListener('keyup', (e) => forwardKey(e, false))
  window.addEventListener('blur', () => keys.clear())
  // Pointer events cover mouse, touch and pen alike: a finger moves the
  // game cursor, presses the "mouse" key, and taps/swipes become
  // `ctx.tap` / `ctx.swipe`.
  const gestures = new PointerGestures()
  window.addEventListener('pointermove', (e) => {
    mouseDelta = { x: mouseDelta.x + e.movementX, y: mouseDelta.y + e.movementY }
    mouseClient = { x: e.clientX, y: e.clientY }
  })
  canvas.addEventListener('pointerdown', (e) => {
    mouseClient = { x: e.clientX, y: e.clientY }
    gestures.down(e)
    keys.add('mouse')
    if (pointerLockArmed && document.pointerLockElement !== canvas) void canvas.requestPointerLock()
  })
  window.addEventListener('pointerup', (e) => {
    if (keys.has('mouse')) gestures.up(e)
    keys.delete('mouse')
  })
  window.addEventListener('pointercancel', () => keys.delete('mouse'))
  canvas.addEventListener('contextmenu', (e) => e.preventDefault())
  // Wheel since the last frame (UI Scroll Views).
  let wheel = { x: 0, y: 0 }
  canvas.addEventListener(
    'wheel',
    (e) => {
      e.preventDefault()
      wheel = { x: wheel.x + e.deltaX, y: wheel.y + e.deltaY }
    },
    { passive: false },
  )

  /** Canvas-box point -> world: the visible frame spans the whole box. */
  const screenToGame = (scene: OpaqueId, x: number, y: number, w: number, h: number) => {
    const view = playgroundView(editor, scene)
    const pan = view ? { x: view[0], y: view[1] } : camera.pan
    const perPx = view ? frameSize.width / Math.max(1, w) : 1 / Math.max(1e-6, camera.zoom * (w / Math.max(1, cssSize.width)))
    return { x: pan.x + (x - w / 2) * perPx, y: pan.y - (y - h / 2) * perPx }
  }

  // ---- frame loop ----
  const host = new ScriptHost()
  // F3 (or `?stats` in the URL) shows the frame profiler.
  const perf = new PerfMeter()
  if (new URLSearchParams(location.search).has('stats')) perf.toggle()
  window.addEventListener('keydown', (e) => {
    if (e.key === 'F3' || e.key === '`') {
      e.preventDefault()
      perf.toggle()
    }
  })
  const globals: Record<string, any> = {}
  let lastTime: number | null = null
  let frameErrors = 0
  // A scene without a Camera is shown whole, centered.
  const fitCamera = (artboard: ArtboardSummary) => ({
    pan: { x: 0, y: 0 },
    zoom: Math.min(cssSize.width / Math.max(1, artboard.width), cssSize.height / Math.max(1, artboard.height)),
  })
  let camera = { pan: { x: 0, y: 0 }, zoom: 1 }
  let cameraOverridden = false

  const frame = (now: number) => {
    try {
      const deltaSeconds = lastTime === null ? 0 : Math.min((now - lastTime) / 1000, 0.1)
      lastTime = now
      const dpr = window.devicePixelRatio || 1
      const artboard = currentScene()
      if (artboard && !cameraOverridden) camera = fitCamera(artboard)
      editor.render(deltaSeconds, camera.pan.x, camera.pan.y, camera.zoom * dpr, -1, dpr, 0, 0, 0, 0, 0, 0)

      const syncStart = performance.now()
      const summary = getStateMachineSummary(editor)
      host.tick(summary, deltaSeconds)
      // The scene list / state machine definitions don't change while
      // playing: reuse the document from load / the last scene switch
      // (re-reading the whole document every frame was the main cost on
      // big scenes). Scripts see live node data through `ScriptHost`.
      const fresh = doc
      perf.add('sync', performance.now() - syncStart)
      const live = fresh?.artboards.find((a) => sceneId && sameId(a.id, sceneId))
      if (live) {
        const machine = live.stateMachines.find((m) => live.activeStateMachine && sameId(m.id, live.activeStateMachine))
        const rect = canvas.getBoundingClientRect()
        const scriptsStart = performance.now()
        const sceneName = host.run(summary, {
          editor,
          deltaSeconds,
          scene: { id: live.id, name: live.name },
          machine,
          // Measured against the canvas's real (scaled) on-screen box, so
          // clicks and taps line up at any size, phones included.
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
        if (sceneName !== null) {
          const target = fresh?.artboards.find((a) => a.name === sceneName)
          if (target) enterScene(target.id)
          else console.warn(`ctx.loadScene: no scene named "${sceneName}"`)
        }
      }
      mouseDelta = { x: 0, y: 0 }
      wheel = { x: 0, y: 0 }
      perf.endFrame(editor, deltaSeconds * 1000, now)
    } catch (err) {
      // Logged sparingly: a per-frame error would otherwise flood the console.
      frameErrors++
      if (frameErrors <= 3 || frameErrors % 600 === 0) console.error(`frame threw (#${frameErrors}) — skipping it`, err)
    } finally {
      requestAnimationFrame(frame)
    }
  }
  requestAnimationFrame(frame)
}

boot().catch((err) => {
  console.error(err)
  showMessage(`Couldn't start the game: ${err instanceof Error ? err.message : String(err)}`)
})
