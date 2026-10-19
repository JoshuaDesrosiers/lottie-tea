/**
 * **Toasty Lite** — the small web runtime for Toasty UI animation.
 *
 * Draws one scene exported as Toasty Lite JSON (`an1mbyt3_format::lite`,
 * Export → Animation → Toasty Lite) with the browser's Canvas 2D: vector
 * shapes, gradients, strokes with trim paths and dashes, text, a transform
 * hierarchy, keyframed animations, the state machine (all layers,
 * crossfades), UI Button hover / press looks, On Click and Pointer
 * Listeners actions, scene Properties and Property Bindings — plus images,
 * blend modes, Drop Shadow, baked motion paths, a subset of Image Effects
 * (background, shadow, outside stroke, blur, glow, grain, gradient map,
 * texture, clip, feather — see `./fx.ts`) and the scene's global texture.
 *
 * Unlike the full engine it has no WebGPU, no wasm and no scripts, so it
 * starts instantly and any number of players can share a page — icons,
 * buttons, toggles, loaders, illustrations. All players share one
 * `requestAnimationFrame` loop and only repaint when something changed.
 *
 * ```js
 * import { createLite } from './toasty/an1mbyt3-lite.js'
 * const heart = await createLite(document.querySelector('#like'), { src: '/toasty/heart.json' })
 * heart.on('event', (e) => e.name === 'like' && save())
 * heart.setInput('liked', true)
 * ```
 */

import { applyFx, applyTexture, fxAnimated, pool, type LiteFx, type LiteTexture } from './fx'

// ---------------------------------------------------------------------
// File format (see crates/an1mbyt3-format/src/lite.rs)
// ---------------------------------------------------------------------

type Color = [number, number, number, number]
interface LiteGradient {
  kind: 'linear' | 'radial'
  start: [number, number]
  end: [number, number]
  stops: { offset: number; color: Color }[]
}
type LiteAction =
  | { type: 'setInput'; input: string; value: string }
  | { type: 'fireTrigger'; input: string }
  | { type: 'toggleInput'; input: string }
  | { type: 'setProperty'; property: string; value: string }
  | { type: 'event'; name: string; arg: string }
  | { type: 'setVisible'; target: number | null; visible: boolean }
  | { type: 'toggleVisible'; target: number | null }
  | { type: 'unsupported'; action: string }
interface LiteNode {
  id: string
  name: string
  parent: number | null
  kind: 'shape' | 'group' | 'text' | 'image'
  visible: boolean
  x: number
  y: number
  rotation: number
  scaleX: number
  scaleY: number
  anchorX: number
  anchorY: number
  clip?: boolean
  path?: string
  fill?: Color
  gradient?: LiteGradient
  stroke?: { width: number; color: Color; cap: CanvasLineCap; join: CanvasLineJoin; gradient?: LiteGradient }
  trim?: { start: number; end: number; offset: number }
  dash?: { dash: number; gap: number; offset: number }
  text?: { content: string; font: string; size: number; color: Color }
  button?: { interactable: boolean; transition: string; normalColor: Color; hoverColor: Color; pressedColor: Color; hoverScale: number; pressedScale: number; fade: number; tintChildren: boolean }
  listeners?: Partial<Record<'click' | 'enter' | 'exit' | 'down' | 'up', LiteAction[]>>
  bindings?: { property: string; target: string; format: string; scale: number; offset: number }[]
  /** An image node: a data URL, drawn centred at its size. */
  image?: { src: string; width: number; height: number }
  /** Canvas2D composite op for the node (`multiply`, `screen`, `lighter`). */
  blend?: string
  /** Drop Shadow 2D (world units, y up). */
  shadow?: { color: Color; offset: [number, number]; blur: number }
  /** Image Effects (the Lite subset) — the node + its children draw into
   * their own layer, these run on it, then it's composited back. */
  fx?: LiteFx
  /** The node subtree's local bounds `[x0, y0, x1, y1]` (fx backgrounds). */
  box?: [number, number, number, number]
}
interface LiteKey {
  t: number
  v: number[]
  e: [number, number, number, number] | null
  d?: string
}
interface LiteAnimation {
  name: string
  duration: number
  tracks: { node: number; prop: string; keys: LiteKey[] }[]
}
interface LiteCondition {
  type: 'bool' | 'gt' | 'lt' | 'enum' | 'trigger'
  input: string
  value?: boolean | number | string
}
interface LiteState {
  name: string
  animation: number | null
  loop: 'once' | 'loop' | 'pingpong'
  transitions: { to: string; condition: LiteCondition; duration: number; reverse: boolean }[]
}
export interface LiteFile {
  format: 'toasty-lite'
  version: number
  scene: string
  width: number
  height: number
  view: { x: number; y: number; zoom: number }
  background: Color | null
  nodes: LiteNode[]
  animations: LiteAnimation[]
  stateMachine: { name: string; inputs: { name: string; kind: 'bool' | 'number' | 'enum' | 'trigger' }[]; layers: { name: string | null; initial: string; states: LiteState[] }[] } | null
  properties: { name: string; kind: 'number' | 'bool' | 'text' | 'color'; default: unknown }[]
  fonts: { name: string; data: string }[]
  /** The scene's global texture (laid over the whole frame), or null. */
  texture?: LiteTexture | null
  skipped: string[]
}

// ---------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------

export interface LiteOptions {
  /** The Lite file: a URL, or the parsed JSON. */
  src: string | URL | LiteFile
  /** `contain` (default) letterboxes the scene in the box; `cover` fills it. */
  fit?: 'contain' | 'cover'
  /** Paint the scene's background colour (default: transparent). */
  background?: boolean
  /** Starting values for scene Properties. */
  data?: Record<string, unknown>
  /** Starting state machine inputs. */
  inputs?: Record<string, boolean | number | string>
  /** Start paused (default: playing). */
  paused?: boolean
}

export interface LiteEvents {
  /** A Call-function action ran (On Click / Pointer Listeners). */
  event: { name: string; payload: unknown }
  /** A state machine layer entered a state. */
  state: { layer: string | null; state: string; previous: string | null }
  /** A scene Property changed (by an action, or by `set`). */
  prop: { name: string; value: unknown }
  /** A state machine input changed (from the page or an action). */
  input: { name: string; value: boolean | number | string }
}
type LiteEventType = keyof LiteEvents

export interface LitePlayer {
  readonly file: LiteFile
  readonly canvas: HTMLCanvasElement
  /** Sets a scene Property (bound text / colours / positions update). */
  set(name: string, value: unknown): void
  setData(data: Record<string, unknown>): void
  get(name: string): unknown
  /** Sets a Bool / Number / Enum input; a Trigger fires when `value` is truthy. */
  setInput(name: string, value: boolean | number | string): void
  fire(trigger: string): void
  /** A Bool / Number / Enum input's current value (`undefined` if none). */
  getInput(name: string): boolean | number | string | undefined
  /** The current state of a layer (`null` = the base layer). */
  state(layer?: string | null): string | null
  on<T extends LiteEventType>(type: T, fn: (event: LiteEvents[T]) => void): () => void
  play(): void
  pause(): void
  destroy(): void
}

const cache = new Map<string, Promise<LiteFile>>()

/** Loads a Lite file (cached by URL). */
export function loadLite(src: string | URL): Promise<LiteFile> {
  const url = new URL(String(src), location.href).href
  let p = cache.get(url)
  if (!p) {
    p = fetch(url).then((r) => {
      if (!r.ok) throw new Error(`Toasty Lite: ${url} → ${r.status}`)
      return r.json() as Promise<LiteFile>
    })
    cache.set(url, p)
  }
  return p
}

/** Creates a player inside `container` (its canvas fills the box). */
export async function createLite(container: HTMLElement, options: LiteOptions): Promise<LitePlayer> {
  const file = typeof options.src === 'string' || options.src instanceof URL ? await loadLite(options.src) : options.src
  if (file.format !== 'toasty-lite') throw new Error('Toasty Lite: not a Toasty Lite file')
  await loadFonts(file)
  return new Player(container, file, options)
}

// ---------------------------------------------------------------------
// Shared frame loop
// ---------------------------------------------------------------------

const players = new Set<Player>()
let rafId = 0
let lastTime = 0
function tick(now: number) {
  const dt = lastTime ? Math.min(0.1, (now - lastTime) / 1000) : 1 / 60
  lastTime = now
  for (const p of players) p.frame(dt)
  rafId = players.size ? requestAnimationFrame(tick) : 0
  if (!rafId) lastTime = 0
}
function schedule() {
  if (!rafId && players.size) rafId = requestAnimationFrame(tick)
}

const loadedFonts = new Set<string>()
async function loadFonts(file: LiteFile) {
  for (const f of file.fonts) {
    if (loadedFonts.has(f.name) || typeof FontFace === 'undefined') continue
    loadedFonts.add(f.name)
    try {
      const bin = Uint8Array.from(atob(f.data), (c) => c.charCodeAt(0))
      const face = new FontFace(f.name, bin)
      await face.load()
      ;(document.fonts as unknown as { add(f: FontFace): void }).add(face)
    } catch {
      // Falls back to the page's sans-serif.
    }
  }
}

// ---------------------------------------------------------------------
// Geometry: paths, flattening, trim
// ---------------------------------------------------------------------

type Cmd = { c: 'M' | 'L' | 'C' | 'Z'; p: number[] }

function parsePath(d: string): Cmd[] {
  const out: Cmd[] = []
  const re = /([MLCZ])([^MLCZ]*)/g
  let m: RegExpExecArray | null
  while ((m = re.exec(d))) {
    const nums = m[2].trim() ? m[2].trim().split(/[\s,]+/).map(Number) : []
    out.push({ c: m[1] as Cmd['c'], p: nums })
  }
  return out
}

function withNumbers(template: Cmd[], numbers: number[]): Cmd[] {
  let i = 0
  return template.map((cmd) => {
    const p = numbers.slice(i, i + cmd.p.length)
    i += cmd.p.length
    return { c: cmd.c, p }
  })
}

function toPath2D(cmds: Cmd[]): Path2D {
  const path = new Path2D()
  for (const { c, p } of cmds) {
    if (c === 'M') path.moveTo(p[0], p[1])
    else if (c === 'L') path.lineTo(p[0], p[1])
    else if (c === 'C') path.bezierCurveTo(p[0], p[1], p[2], p[3], p[4], p[5])
    else path.closePath()
  }
  return path
}

/** Polylines (one per subpath) with cumulative lengths — for trim paths. */
interface Flat {
  subpaths: { pts: number[]; lens: number[]; closed: boolean }[]
  total: number
}

function flatten(cmds: Cmd[]): Flat {
  const subpaths: Flat['subpaths'] = []
  let cur: number[] = []
  let sx = 0
  let sy = 0
  let x = 0
  let y = 0
  const finish = (closed: boolean) => {
    if (cur.length >= 4) {
      const lens = [0]
      for (let i = 2; i < cur.length; i += 2) lens.push(lens[lens.length - 1] + Math.hypot(cur[i] - cur[i - 2], cur[i + 1] - cur[i - 1]))
      subpaths.push({ pts: cur, lens, closed })
    }
    cur = []
  }
  for (const { c, p } of cmds) {
    if (c === 'M') {
      finish(false)
      x = sx = p[0]
      y = sy = p[1]
      cur = [x, y]
    } else if (c === 'L') {
      x = p[0]
      y = p[1]
      cur.push(x, y)
    } else if (c === 'C') {
      const steps = Math.max(8, Math.min(48, Math.ceil((Math.hypot(p[0] - x, p[1] - y) + Math.hypot(p[2] - p[0], p[3] - p[1]) + Math.hypot(p[4] - p[2], p[5] - p[3])) / 3)))
      for (let i = 1; i <= steps; i++) {
        const t = i / steps
        const u = 1 - t
        cur.push(u * u * u * x + 3 * u * u * t * p[0] + 3 * u * t * t * p[2] + t * t * t * p[4], u * u * u * y + 3 * u * u * t * p[1] + 3 * u * t * t * p[3] + t * t * t * p[5])
      }
      x = p[4]
      y = p[5]
    } else {
      if (cur.length && (x !== sx || y !== sy)) cur.push(sx, sy)
      finish(true)
      x = sx
      y = sy
    }
  }
  finish(false)
  return { subpaths, total: subpaths.reduce((s, sp) => s + sp.lens[sp.lens.length - 1], 0) }
}

/** The part of the path between `a` and `b` (absolute lengths along the whole path). */
function trimSegment(flat: Flat, a: number, b: number, path: Path2D) {
  let base = 0
  for (const sp of flat.subpaths) {
    const len = sp.lens[sp.lens.length - 1]
    const s = Math.max(a - base, 0)
    const e = Math.min(b - base, len)
    if (e > s) {
      const at = (d: number): [number, number] => {
        let i = 1
        while (i < sp.lens.length - 1 && sp.lens[i] < d) i++
        const l0 = sp.lens[i - 1]
        const l1 = sp.lens[i]
        const t = l1 > l0 ? (d - l0) / (l1 - l0) : 0
        return [sp.pts[(i - 1) * 2] + (sp.pts[i * 2] - sp.pts[(i - 1) * 2]) * t, sp.pts[(i - 1) * 2 + 1] + (sp.pts[i * 2 + 1] - sp.pts[(i - 1) * 2 + 1]) * t]
      }
      const [x0, y0] = at(s)
      path.moveTo(x0, y0)
      for (let i = 1; i < sp.lens.length; i++) if (sp.lens[i] > s && sp.lens[i] < e) path.lineTo(sp.pts[i * 2], sp.pts[i * 2 + 1])
      const [x1, y1] = at(e)
      path.lineTo(x1, y1)
    }
    base += len
  }
}

function trimmedPath(flat: Flat, start: number, end: number, offset: number): Path2D {
  const path = new Path2D()
  const total = flat.total
  if (total <= 0) return path
  let s = Math.min(start, end)
  let e = Math.max(start, end)
  if (e - s >= 1 - 1e-6) {
    trimSegment(flat, 0, total, path)
    return path
  }
  s = (((s + offset) % 1) + 1) % 1
  e = s + (Math.max(start, end) - Math.min(start, end))
  if (e <= 1) trimSegment(flat, s * total, e * total, path)
  else {
    trimSegment(flat, s * total, total, path)
    trimSegment(flat, 0, (e - 1) * total, path)
  }
  return path
}

// ---------------------------------------------------------------------
// Animation sampling
// ---------------------------------------------------------------------

function bezierEase(e: [number, number, number, number], x: number): number {
  const [x1, y1, x2, y2] = e
  // Solve the curve's x(t) = x with Newton, then return y(t).
  let t = x
  for (let i = 0; i < 6; i++) {
    const u = 1 - t
    const fx = 3 * u * u * t * x1 + 3 * u * t * t * x2 + t * t * t - x
    const dx = 3 * u * u * x1 + 6 * u * t * (x2 - x1) + 3 * t * t * (1 - x2)
    if (Math.abs(dx) < 1e-6) break
    t = Math.min(1, Math.max(0, t - fx / dx))
  }
  const u = 1 - t
  return 3 * u * u * t * y1 + 3 * u * t * t * y2 + t * t * t
}

function sampleKeys(keys: LiteKey[], time: number): number[] {
  if (time <= keys[0].t) return keys[0].v
  const last = keys[keys.length - 1]
  if (time >= last.t) return last.v
  let i = 1
  while (keys[i].t < time) i++
  const a = keys[i - 1]
  const b = keys[i]
  let k = (time - a.t) / Math.max(1e-6, b.t - a.t)
  if (a.e) k = bezierEase(a.e, k)
  if (a.v.length !== b.v.length) return k < 1 ? a.v : b.v
  return a.v.map((v, j) => v + (b.v[j] - v) * k)
}

const mixValues = (a: number[], b: number[], w: number) => (a.length === b.length ? a.map((v, i) => v + (b[i] - v) * w) : w < 0.5 ? a : b)

/** One node property as numbers (what tracks and blends work with). */
type Pose = Map<string, number[]> // key: `${node}:${prop}`

// ---------------------------------------------------------------------
// The player
// ---------------------------------------------------------------------

interface NodeState {
  def: LiteNode
  template: Cmd[] | null
  /** The current commands (the template, or a path morph's sample). */
  cmds: Cmd[] | null
  path: Path2D | null
  flat: Flat | null
  pathKey: string
  // Button look, eased.
  hover: boolean
  pressed: boolean
  lookScale: number
  lookTint: Color
  visibleOverride: boolean | null
  text: string | null
  /** An image node's picture (loads async; drawn once complete). */
  img: HTMLImageElement | null
}

interface LayerRun {
  name: string | null
  states: LiteState[]
  current: LiteState | null
  time: number
  dir: 1 | -1
  from: { state: LiteState; time: number; dir: 1 | -1 } | null
  fade: number
  fadeDuration: number
}

class Player implements LitePlayer {
  readonly canvas: HTMLCanvasElement
  private ctx: CanvasRenderingContext2D
  private nodes: NodeState[]
  private props = new Map<string, unknown>()
  private inputs = new Map<string, boolean | number | string>()
  private triggers = new Set<string>()
  private layers: LayerRun[] = []
  private listeners = new Map<string, Set<(e: unknown) => void>>()
  private paused: boolean
  private dirty = true
  private visible = true
  private size = { w: 1, h: 1, dpr: 1 }
  /** `presses` / `releases` count events since the last frame, so a click
   * whose down and up land in the same frame is never missed. */
  private pointer: { x: number; y: number; inside: boolean; down: boolean; presses: number; releases: number; leaving: boolean } = { x: 0, y: 0, inside: false, down: false, presses: 0, releases: 0, leaving: false }
  private worlds: DOMMatrix[] = []
  private cleanups: (() => void)[] = []
  private lastPose: Pose = new Map()
  /** Seconds since start — animated grain / textures. */
  private clock = 0
  /** Time since animated effects last redrew (they step at 24 fps). */
  private fxStep = 0
  /** Grain / a texture animates: draw every frame, not only on change. */
  private animatedFx = false
  /** Redraw soon (an image or texture finished loading). */
  private wake = () => {
    this.dirty = true
    schedule()
  }

  private container: HTMLElement
  readonly file: LiteFile
  private options: LiteOptions

  constructor(container: HTMLElement, file: LiteFile, options: LiteOptions) {
    this.container = container
    this.file = file
    this.options = options
    this.canvas = document.createElement('canvas')
    this.canvas.style.cssText = 'display:block;width:100%;height:100%;touch-action:manipulation'
    container.appendChild(this.canvas)
    const ctx = this.canvas.getContext('2d')
    if (!ctx) throw new Error('Toasty Lite: no 2D canvas')
    this.ctx = ctx
    this.paused = !!options.paused
    this.nodes = file.nodes.map((def) => {
      const template = def.path ? parsePath(def.path) : null
      let img: HTMLImageElement | null = null
      if (def.kind === 'image' && def.image?.src) {
        img = new Image()
        img.onload = () => this.wake()
        img.src = def.image.src
      }
      return { def, template, cmds: template, path: template ? toPath2D(template) : null, flat: null, pathKey: def.path ?? '', hover: false, pressed: false, lookScale: 1, lookTint: [1, 1, 1, 1], visibleOverride: null, text: null, img }
    })
    this.animatedFx = file.nodes.some((n) => n.fx && fxAnimated(n.fx.effects)) || (!!file.texture && fxAnimated([file.texture]))
    for (const p of file.properties) this.props.set(p.name, coerce(p.kind, p.default))
    for (const [k, v] of Object.entries(options.data ?? {})) this.setProp(k, v, false)
    for (const i of file.stateMachine?.inputs ?? []) if (i.kind !== 'trigger') this.inputs.set(i.name, i.kind === 'bool' ? false : i.kind === 'number' ? 0 : '')
    for (const [k, v] of Object.entries(options.inputs ?? {})) this.setInput(k, v)
    for (const layer of file.stateMachine?.layers ?? []) {
      const run: LayerRun = { name: layer.name, states: layer.states, current: null, time: 0, dir: 1, from: null, fade: 0, fadeDuration: 0 }
      this.layers.push(run)
      this.enter(run, layer.states.find((s) => s.name === layer.initial) ?? layer.states[0] ?? null, 0, false)
    }

    const ro = new ResizeObserver(() => this.resize())
    ro.observe(container)
    this.cleanups.push(() => ro.disconnect())
    if (typeof IntersectionObserver !== 'undefined') {
      const io = new IntersectionObserver((entries) => {
        this.visible = entries.some((e) => e.isIntersecting)
        if (this.visible) this.dirty = true
      })
      io.observe(container)
      this.cleanups.push(() => io.disconnect())
    }
    const move = (e: PointerEvent) => {
      const r = this.canvas.getBoundingClientRect()
      this.pointer.x = e.clientX - r.left
      this.pointer.y = e.clientY - r.top
      this.pointer.inside = true
      this.dirty = true
      // Wake the shared loop: hover / press must react on the next frame
      // even when nothing was animating.
      schedule()
    }
    // Touch fires pointerleave right after pointerup, in the same frame:
    // leaving at once would hit-test the tap with the pointer already gone
    // (a tap showed hover but never clicked). So a leave waits until the
    // next frame has handled any press / release first.
    const leave = () => {
      this.pointer.leaving = true
      this.dirty = true
      schedule()
    }
    const down = (e: PointerEvent) => {
      move(e)
      this.pointer.leaving = false
      this.pointer.down = true
      this.pointer.presses++
    }
    const up = (e: PointerEvent) => {
      move(e)
      this.pointer.down = false
      this.pointer.releases++
    }
    this.canvas.addEventListener('pointermove', move)
    this.canvas.addEventListener('pointerleave', leave)
    this.canvas.addEventListener('pointerdown', down)
    this.canvas.addEventListener('pointerup', up)
    // A scroll that takes over the touch: treat as leaving, no click.
    const cancel = () => {
      this.pointer.down = false
      this.pointer.presses = 0
      this.pointer.releases = 0
      leave()
    }
    this.canvas.addEventListener('pointercancel', cancel)
    this.cleanups.push(() => {
      this.canvas.removeEventListener('pointercancel', cancel)
      this.canvas.removeEventListener('pointermove', move)
      this.canvas.removeEventListener('pointerleave', leave)
      this.canvas.removeEventListener('pointerdown', down)
      this.canvas.removeEventListener('pointerup', up)
    })
    this.resize()
    players.add(this)
    schedule()
  }

  // ---- API ----

  set(name: string, value: unknown) {
    this.setProp(name, value, true)
  }
  setData(data: Record<string, unknown>) {
    for (const [k, v] of Object.entries(data)) this.setProp(k, v, true)
  }
  get(name: string) {
    return this.props.get(name)
  }
  setInput(name: string, value: boolean | number | string) {
    const def = this.file.stateMachine?.inputs.find((i) => i.name === name)
    if (!def) return
    if (def.kind === 'trigger') {
      if (value) this.triggers.add(name)
    } else {
      const next = def.kind === 'bool' ? value === true || value === 'true' || value === 1 || value === '1' : def.kind === 'number' ? Number(value) || 0 : String(value)
      if (this.inputs.get(name) !== next) {
        this.inputs.set(name, next)
        this.emit('input', { name, value: next })
      }
    }
    this.dirty = true
    schedule()
  }
  fire(trigger: string) {
    this.setInput(trigger, true)
  }
  getInput(name: string) {
    return this.inputs.get(name)
  }
  state(layer: string | null = null) {
    return this.layers.find((l) => l.name === layer)?.current?.name ?? null
  }
  on<T extends LiteEventType>(type: T, fn: (event: LiteEvents[T]) => void) {
    let set = this.listeners.get(type)
    if (!set) this.listeners.set(type, (set = new Set()))
    set.add(fn as (e: unknown) => void)
    return () => set!.delete(fn as (e: unknown) => void)
  }
  play() {
    this.paused = false
    this.dirty = true
    schedule()
  }
  pause() {
    this.paused = true
  }
  destroy() {
    players.delete(this)
    for (const c of this.cleanups) c()
    this.canvas.remove()
  }

  // ---- internals ----

  private emit<T extends LiteEventType>(type: T, event: LiteEvents[T]) {
    for (const fn of this.listeners.get(type) ?? []) {
      try {
        fn(event)
      } catch (err) {
        console.error(err)
      }
    }
    this.container.dispatchEvent(new CustomEvent(`toasty:${type}`, { detail: event, bubbles: true }))
  }

  private setProp(name: string, value: unknown, notify: boolean) {
    const def = this.file.properties.find((p) => p.name === name)
    const v = def ? coerce(def.kind, value) : value
    if (sameValue(this.props.get(name), v)) return
    this.props.set(name, v)
    this.dirty = true
    if (notify) this.emit('prop', { name, value: v })
    schedule()
  }

  private resize() {
    const dpr = Math.min(3, window.devicePixelRatio || 1)
    const w = Math.max(1, this.container.clientWidth)
    const h = Math.max(1, this.container.clientHeight)
    this.size = { w, h, dpr }
    this.canvas.width = Math.round(w * dpr)
    this.canvas.height = Math.round(h * dpr)
    this.dirty = true
  }

  private enter(run: LayerRun, state: LiteState | null, fade: number, reverse: boolean) {
    const previous = run.current
    if (previous && fade > 0) run.from = { state: previous, time: run.time, dir: run.dir }
    else run.from = null
    run.fade = 0
    run.fadeDuration = fade
    run.current = state
    run.dir = reverse ? -1 : 1
    const anim = state?.animation != null ? this.file.animations[state.animation] : null
    run.time = reverse && anim ? anim.duration : 0
    if (state && previous !== state) this.emit('state', { layer: run.name, state: state.name, previous: previous?.name ?? null })
    this.dirty = true
  }

  private holds(c: LiteCondition): boolean {
    const v = this.inputs.get(c.input)
    switch (c.type) {
      case 'bool':
        return v === c.value
      case 'gt':
        return Number(v) > Number(c.value)
      case 'lt':
        return Number(v) < Number(c.value)
      case 'enum':
        return v === c.value
      case 'trigger':
        return this.triggers.has(c.input)
    }
  }

  private advance(run: LayerRun, dt: number): boolean {
    let moving = false
    const step = (state: LiteState, time: number, dir: 1 | -1): [number, 1 | -1, boolean] => {
      const anim = state.animation != null ? this.file.animations[state.animation] : null
      if (!anim || anim.duration <= 0) return [0, dir, false]
      let t = time + dt * dir
      const d = anim.duration
      if (state.loop === 'loop') return [((t % d) + d) % d, dir, true]
      if (state.loop === 'pingpong') {
        if (t > d) return [d - (t - d), -1, true]
        if (t < 0) return [-t, 1, true]
        return [t, dir, true]
      }
      t = Math.min(d, Math.max(0, t))
      return [t, dir, t > 0 && t < d]
    }
    if (run.current) {
      const [t, dir, m] = step(run.current, run.time, run.dir)
      run.time = t
      run.dir = dir
      moving = m
    }
    if (run.from) {
      const [t, dir] = step(run.from.state, run.from.time, run.from.dir)
      run.from.time = t
      run.from.dir = dir
      run.fade += dt
      if (run.fade >= run.fadeDuration) run.from = null
      moving = true
    }
    return moving
  }

  private sample(state: LiteState | null, time: number, into: Pose) {
    if (!state || state.animation == null) return
    const anim = this.file.animations[state.animation]
    if (!anim) return
    for (const track of anim.tracks) if (track.keys.length) into.set(`${track.node}:${track.prop}`, sampleKeys(track.keys, time))
  }

  frame(dt: number) {
    if (!this.visible) return
    this.clock += dt
    // Animated grain / textures step at 24 fps — film-like, and a fraction
    // of the cost of redrawing effect layers every display frame.
    this.fxStep += dt
    let moving = false
    if (this.animatedFx && this.fxStep >= 1 / 24) {
      this.fxStep = 0
      moving = true
    }
    if (!this.paused) {
      // Transitions (one hop per layer per frame), then time.
      for (const run of this.layers) {
        const t = run.current?.transitions.find((tr) => this.holds(tr.condition))
        if (t) {
          const next = run.states.find((s) => s.name === t.to)
          if (next) this.enter(run, next, t.duration, t.reverse)
        }
      }
      this.triggers.clear()
      for (const run of this.layers) moving = this.advance(run, dt) || moving
    }
    const looks = this.updatePointer(dt)
    if (!moving && !looks && !this.dirty) return
    this.dirty = false
    this.draw()
    schedule()
  }

  /** The pose: base layer, then each layer over it; crossfades blend. */
  private pose(): Pose {
    const pose: Pose = new Map()
    for (const run of this.layers) {
      const now: Pose = new Map()
      this.sample(run.current, run.time, now)
      if (run.from && run.fadeDuration > 0) {
        const before: Pose = new Map()
        this.sample(run.from.state, run.from.time, before)
        const w = Math.min(1, run.fade / run.fadeDuration)
        const keys = new Set([...before.keys(), ...now.keys()])
        for (const k of keys) {
          const a = before.get(k) ?? pose.get(k) ?? this.lastPose.get(k)
          const b = now.get(k) ?? pose.get(k) ?? this.base(k)
          if (a && b) pose.set(k, mixValues(a, b, w))
          else if (b) pose.set(k, b)
        }
      } else for (const [k, v] of now) pose.set(k, v)
    }
    this.lastPose = pose
    return pose
  }

  /** The authored value of a pose key (for blends from "nothing"). */
  private base(key: string): number[] | undefined {
    const [i, prop] = key.split(':')
    const n = this.file.nodes[Number(i)]
    if (!n) return undefined
    switch (prop) {
      case 'translation':
        return [n.x, n.y]
      case 'rotation':
        return [n.rotation]
      case 'scale':
        return [n.scaleX, n.scaleY]
      case 'fill':
        return n.fill ?? n.text?.color
      case 'trimStart':
        return [n.trim?.start ?? 0]
      case 'trimEnd':
        return [n.trim?.end ?? 1]
      case 'trimOffset':
        return [n.trim?.offset ?? 0]
      case 'strokeWidth':
        return [n.stroke?.width ?? 0]
      case 'strokeColor':
        return n.stroke?.color
      case 'visible':
        return [n.visible ? 1 : 0]
    }
    return undefined
  }

  /** Hover / press for UI Button nodes: hit test, looks, listeners. */
  private updatePointer(dt: number): boolean {
    let changed = false
    let hit = -1
    const { presses, releases } = this.pointer
    this.pointer.presses = 0
    this.pointer.releases = 0
    if (this.pointer.inside) {
      for (let i = this.nodes.length - 1; i >= 0; i--) {
        const n = this.nodes[i]
        if (!n.def.button?.interactable || !n.path || !this.worlds[i] || !this.isShown(i)) continue
        this.ctx.setTransform(this.worlds[i])
        if (this.ctx.isPointInPath(n.path, this.pointer.x * this.size.dpr, this.pointer.y * this.size.dpr)) {
          hit = i
          break
        }
      }
    }
    this.canvas.style.cursor = hit >= 0 ? 'pointer' : ''
    this.nodes.forEach((n, i) => {
      const b = n.def.button
      if (!b) return
      const hover = i === hit
      const pressed = hover && this.pointer.down
      const run = (list: LiteAction[] | undefined) => list && this.runActions(list, i)
      if (hover && !n.hover) run(n.def.listeners?.enter)
      const gotDown = hover && (presses > 0 || (pressed && !n.pressed))
      if (gotDown && !n.pressed) run(n.def.listeners?.down)
      if ((n.pressed || gotDown) && (releases > 0 || !this.pointer.down)) {
        run(n.def.listeners?.up)
        if (hover) run(n.def.listeners?.click)
      }
      if (!hover && n.hover) run(n.def.listeners?.exit)
      n.hover = hover
      n.pressed = pressed && !this.pointer.leaving
      const targetScale = pressed ? b.pressedScale : hover ? b.hoverScale : 1
      const targetTint = !b.interactable ? b.normalColor : pressed ? b.pressedColor : hover ? b.hoverColor : b.normalColor
      const k = b.fade > 0 ? Math.min(1, dt / b.fade) : 1
      const ns = n.lookScale + (targetScale - n.lookScale) * k
      const nt = n.lookTint.map((v, j) => v + (targetTint[j] - v) * k) as Color
      if (Math.abs(ns - n.lookScale) > 1e-4 || nt.some((v, j) => Math.abs(v - n.lookTint[j]) > 1e-4)) changed = true
      n.lookScale = ns
      n.lookTint = nt
    })
    // A deferred leave (see `leave`): now that this frame handled the press,
    // the pointer is gone — next frame runs the exits.
    if (this.pointer.leaving) {
      this.pointer.leaving = false
      this.pointer.inside = false
      this.pointer.down = false
      this.dirty = true
      schedule()
    }
    return changed
  }

  private runActions(list: LiteAction[], self: number) {
    for (const a of list) {
      switch (a.type) {
        case 'setInput':
          this.setInput(a.input, parseLoose(a.value) as boolean | number | string)
          break
        case 'fireTrigger':
          this.fire(a.input)
          break
        case 'toggleInput':
          this.setInput(a.input, !this.inputs.get(a.input))
          break
        case 'setProperty': {
          const cur = this.props.get(a.property)
          const rel = /^[+-]\d/.test(a.value) && typeof cur === 'number'
          this.set(a.property, rel ? (cur as number) + Number(a.value) : parseLoose(a.value))
          break
        }
        case 'event':
          this.emit('event', { name: a.name, payload: a.arg === '' ? undefined : parseLoose(a.arg) })
          break
        case 'setVisible':
          this.nodes[a.target ?? self].visibleOverride = a.visible
          break
        case 'toggleVisible': {
          const i = a.target ?? self
          this.nodes[i].visibleOverride = !this.isShown(i)
          break
        }
      }
    }
    this.dirty = true
  }

  private isShown(i: number): boolean {
    for (let k: number | null = i; k != null; k = this.file.nodes[k].parent) {
      const n = this.nodes[k]
      const v = n.visibleOverride ?? this.cur.get(`${k}:visible`)?.[0] ?? (n.def.visible ? 1 : 0)
      if (typeof v === 'boolean' ? !v : v < 0.5) return false
    }
    return true
  }

  private cur: Pose = new Map()

  private draw() {
    const ctx = this.ctx
    /** Where drawing goes: the canvas, or the innermost open fx layer. */
    let g: CanvasRenderingContext2D = ctx
    const { w, h, dpr } = this.size
    const file = this.file
    const pose = this.pose()
    this.cur = pose
    // Bindings write into the pose where no animation drives that value
    // this frame — the same precedence as the engine, where a property
    // binding is applied when the property changes and keyframes run every
    // frame on top.
    const animated = new Set(pose.keys())
    const free = (key: string) => !animated.has(key)
    file.nodes.forEach((n, i) => {
      for (const b of n.bindings ?? []) {
        const v = this.props.get(b.property)
        if (v === undefined) continue
        if (b.target === 'text') this.nodes[i].text = b.format ? b.format.replace('{}', fmt(v)) : fmt(v)
        else if (b.target === 'visible') free(`${i}:visible`) && pose.set(`${i}:visible`, [v ? 1 : 0])
        else if ((b.target === 'fill' || b.target === 'strokeColor') && Array.isArray(v)) free(`${i}:${b.target}`) && pose.set(`${i}:${b.target}`, v as number[])
        else {
          const num = Number(v) * b.scale + b.offset
          const base = pose.get(`${i}:translation`) ?? [n.x, n.y]
          const sc = pose.get(`${i}:scale`) ?? [n.scaleX, n.scaleY]
          if (b.target === 'x') pose.set(`${i}:translation`, [num, base[1]])
          else if (b.target === 'y') pose.set(`${i}:translation`, [base[0], num])
          else if (b.target === 'rotation') pose.set(`${i}:rotation`, [(num * Math.PI) / 180])
          else if (b.target === 'scale') pose.set(`${i}:scale`, [num, num])
          else if (b.target === 'scaleX') pose.set(`${i}:scale`, [num, sc[1]])
          else if (b.target === 'scaleY') pose.set(`${i}:scale`, [sc[0], num])
          else if (free(`${i}:${b.target}`)) pose.set(`${i}:${b.target}`, [num])
        }
      }
    })

    ctx.setTransform(1, 0, 0, 1, 0, 0)
    ctx.clearRect(0, 0, this.canvas.width, this.canvas.height)
    const fitFn = this.options.fit === 'cover' ? Math.max : Math.min
    const zoom = file.view.zoom || 1
    const scale = fitFn(w / (file.width / zoom), h / (file.height / zoom)) * zoom * dpr
    const root = new DOMMatrix().translate((w * dpr) / 2, (h * dpr) / 2).scale(scale, -scale).translate(-file.view.x, -file.view.y)
    if (this.options.background && file.background) {
      ctx.fillStyle = css(file.background)
      ctx.fillRect(0, 0, this.canvas.width, this.canvas.height)
    }

    // World matrices (parents come first in draw order).
    const worlds: DOMMatrix[] = []
    const tints: Color[] = []
    file.nodes.forEach((n, i) => {
      const t = pose.get(`${i}:translation`) ?? [n.x, n.y]
      const r = pose.get(`${i}:rotation`)?.[0] ?? n.rotation
      const s = pose.get(`${i}:scale`) ?? [n.scaleX, n.scaleY]
      const anchor = pose.get(`${i}:anchor`) ?? [n.anchorX, n.anchorY]
      const look = this.nodes[i].lookScale
      const local = new DOMMatrix()
        .translate(t[0], t[1])
        .rotate((r * 180) / Math.PI)
        .scale(s[0] * look, s[1] * look)
        .translate(-anchor[0], -anchor[1])
      worlds[i] = (n.parent != null ? worlds[n.parent] : root).multiply(local)
      const parentTint = n.parent != null && file.nodes[n.parent].button?.tintChildren ? tints[n.parent] : [1, 1, 1, 1]
      const own = n.button ? this.nodes[i].lookTint : [1, 1, 1, 1]
      tints[i] = own.map((v, j) => v * parentTint[j]) as Color
    })
    this.worlds = worlds

    // Animated Image Effects values (`<node>:fx/<i>/<field>`, `fx/feather` …)
    // override a per-frame copy of the node's effects.
    const fxNow: (LiteFx | undefined)[] = file.nodes.map((n) => n.fx)
    for (const [k, v] of pose) {
      const at = k.indexOf(':fx/')
      if (at < 0) continue
      const i = Number(k.slice(0, at))
      const base = file.nodes[i]?.fx
      if (!base) continue
      let cur = fxNow[i]!
      if (cur === base) cur = fxNow[i] = { ...base, effects: base.effects.map((e) => ({ ...e })) }
      const parts = k.slice(at + 4).split('/')
      const obj = (parts.length === 1 ? cur : cur.effects[Number(parts[0])]) as unknown as Record<string, unknown> | undefined
      const field = parts[parts.length - 1]
      if (!obj || !(field in obj)) continue
      // Track values come padded to 4 numbers: shape them like the field
      // they replace (a number, a 2D offset, an RGBA colour).
      const was = obj[field]
      obj[field] = Array.isArray(was) ? v.slice(0, was.length) : v[0]
    }
    /** Where the current target sits on the canvas (an fx layer only
     * covers its node's bounds, so drawing into it is shifted). */
    let off = { x: 0, y: 0 }
    const at = (m: DOMMatrix) => (off.x || off.y ? new DOMMatrix().translate(-off.x, -off.y).multiply(m) : m)
    // Screen px per node unit (shadow / effect sizes are in node units).
    const devScale = (i: number) => Math.hypot(worlds[i].a, worlds[i].b) || 1
    /** Sets node `i`'s blend + Drop Shadow on `g`; returns the reset. A node
     * with Image Effects blends when its layer composites, not here. */
    const look = (i: number) => {
      const n = file.nodes[i]
      if (n.blend && !n.fx) g.globalCompositeOperation = n.blend as GlobalCompositeOperation
      if (n.shadow) {
        const k = devScale(i)
        g.shadowColor = css(n.shadow.color)
        g.shadowBlur = n.shadow.blur * k
        g.shadowOffsetX = n.shadow.offset[0] * k
        g.shadowOffsetY = -n.shadow.offset[1] * k
      }
      return () => {
        g.globalCompositeOperation = 'source-over'
        if (n.shadow) {
          g.shadowColor = 'transparent'
          g.shadowBlur = 0
          g.shadowOffsetX = 0
          g.shadowOffsetY = 0
        }
      }
    }
    // Clipping groups: children of a `clip` node draw inside its path.
    const clipDepth: number[] = []
    const drawNode = (i: number) => {
      const n = file.nodes[i]
      const st = this.nodes[i]
      if (!this.isShown(i)) return
      g.setTransform(at(worlds[i]))
      const tint = tints[i]
      if (n.kind === 'shape' && st.template) {
        const morph = pose.get(`${i}:path`)
        if (morph) {
          const key = morph.join(',')
          if (key !== st.pathKey) {
            const cmds = withNumbers(st.template, morph)
            st.path = toPath2D(cmds)
            st.flat = null
            st.pathKey = key
            st.cmds = cmds
          }
        }
        const path = st.path!
        const restoreLook = look(i)
        const fill = pose.get(`${i}:fill`) ?? n.fill
        if (n.gradient) g.fillStyle = gradientStyle(g, n.gradient, tint)
        else if (fill && fill[3] > 0) g.fillStyle = css(mulColor(fill as Color, tint))
        if (n.gradient || (fill && fill[3] > 0)) g.fill(path)
        if (n.stroke) {
          const width = pose.get(`${i}:strokeWidth`)?.[0] ?? n.stroke.width
          const color = (pose.get(`${i}:strokeColor`) as Color | undefined) ?? n.stroke.color
          if (width > 0 && (color[3] > 0 || n.stroke.gradient)) {
            g.lineWidth = width
            g.lineCap = n.stroke.cap
            g.lineJoin = n.stroke.join
            g.strokeStyle = n.stroke.gradient ? gradientStyle(g, n.stroke.gradient, tint) : css(mulColor(color, tint))
            let strokePath = path
            if (n.trim) {
              const start = pose.get(`${i}:trimStart`)?.[0] ?? n.trim.start
              const end = pose.get(`${i}:trimEnd`)?.[0] ?? n.trim.end
              const offset = pose.get(`${i}:trimOffset`)?.[0] ?? n.trim.offset
              if (Math.abs(end - start) < 1e-4) strokePath = new Path2D()
              else if (!(start <= 0 && end >= 1 && offset === 0)) {
                if (!st.flat) st.flat = flatten(st.cmds ?? st.template)
                strokePath = trimmedPath(st.flat, start, end, offset)
              }
            }
            if (n.dash && n.dash.dash > 0) {
              g.setLineDash([n.dash.dash, n.dash.gap])
              g.lineDashOffset = -(pose.get(`${i}:dashOffset`)?.[0] ?? n.dash.offset)
            }
            g.stroke(strokePath)
            g.setLineDash([])
          }
        }
        restoreLook()
        if (n.clip) {
          g.save()
          g.clip(path)
          clipDepth.push(i)
        }
      } else if (n.kind === 'image' && n.image && st.img?.complete && st.img.naturalWidth > 0) {
        const restoreLook = look(i)
        g.scale(1, -1)
        g.globalAlpha = tint[3]
        g.drawImage(st.img, -n.image.width / 2, -n.image.height / 2, n.image.width, n.image.height)
        g.globalAlpha = 1
        restoreLook()
      } else if (n.kind === 'text' && n.text) {
        const restoreLook = look(i)
        const size = pose.get(`${i}:textSize`)?.[0] ?? n.text.size
        const color = (pose.get(`${i}:fill`) as Color | undefined) ?? n.text.color
        g.scale(1, -1)
        g.font = `${size}px "${n.text.font}", system-ui, sans-serif`
        g.textAlign = 'center'
        g.textBaseline = 'middle'
        g.fillStyle = css(mulColor(color, tint))
        g.fillText(st.text ?? n.text.content, 0, 0)
        restoreLook()
      } else if (n.clip) {
        clipDepth.push(-1)
      }
    }
    // Depth-first order is the file's order; close clips when leaving a subtree.
    const subtreeEnd: number[] = file.nodes.map(() => 0)
    for (let i = file.nodes.length - 1; i >= 0; i--) {
      let end = i
      for (let j = i + 1; j < file.nodes.length && this.isDescendant(j, i); j++) end = j
      subtreeEnd[i] = end
    }
    // Open clips and fx layers, innermost last. Both span a node's whole
    // subtree, so they nest; leaving a subtree closes them in order.
    type Rect = { x: number; y: number }
    type Open = { kind: 'clip'; end: number } | { kind: 'fx'; owner: number; end: number; layer: HTMLCanvasElement; prev: CanvasRenderingContext2D; prevOff: Rect; rect: Rect }
    /** Canvas pixels an fx node can touch: its box grown by what the
     * effects spread (shadow, stroke, blur, glow), clamped to the canvas. */
    const fxRect = (i: number) => {
      const n = file.nodes[i]
      const W = this.canvas.width
      const H = this.canvas.height
      const fx = fxNow[i]
      if (!n.box || !fx) return { x: 0, y: 0, w: W, h: H }
      let grow = Math.max(0, fx.padding)
      for (const e of fx.effects) {
        if (e.type === 'shadow') grow = Math.max(grow, Math.hypot(e.offset[0], e.offset[1]) + e.blur * 1.5 + e.spread)
        else if (e.type === 'stroke') grow = Math.max(grow, e.width * 1.5)
        else if (e.type === 'blur') grow = Math.max(grow, e.radius * 1.5)
        else if (e.type === 'glow') grow = Math.max(grow, e.radius * 1.5)
      }
      const [x0, y0, x1, y1] = n.box
      const m = worlds[i]
      let lx = Infinity
      let ly = Infinity
      let hx = -Infinity
      let hy = -Infinity
      for (const [px, py] of [[x0 - grow, y0 - grow], [x1 + grow, y0 - grow], [x1 + grow, y1 + grow], [x0 - grow, y1 + grow]]) {
        const q = m.transformPoint(new DOMPoint(px, py))
        lx = Math.min(lx, q.x)
        ly = Math.min(ly, q.y)
        hx = Math.max(hx, q.x)
        hy = Math.max(hy, q.y)
      }
      const x = Math.max(0, Math.floor(lx) - 2)
      const y = Math.max(0, Math.floor(ly) - 2)
      return { x, y, w: Math.max(1, Math.min(W, Math.ceil(hx) + 2) - x), h: Math.max(1, Math.min(H, Math.ceil(hy) + 2) - y) }
    }
    const open: Open[] = []
    const close = (o: Open) => {
      if (o.kind === 'clip') {
        g.restore()
        return
      }
      // The layer is done: run its effects, composite with the node's blend.
      const n = file.nodes[o.owner]
      applyFx(o.layer, fxNow[o.owner]!, at(worlds[o.owner]), n.box, devScale(o.owner), this.clock, this.wake)
      g = o.prev
      off = o.prevOff
      g.save()
      g.setTransform(1, 0, 0, 1, 0, 0)
      g.globalCompositeOperation = (n.blend ?? 'source-over') as GlobalCompositeOperation
      g.drawImage(o.layer, o.rect.x - off.x, o.rect.y - off.y)
      g.restore()
      pool.give(o.layer)
    }
    for (let i = 0; i < file.nodes.length; i++) {
      while (open.length && i > open[open.length - 1].end) close(open.pop()!)
      if (file.nodes[i].fx && this.isShown(i)) {
        const r = fxRect(i)
        const layer = pool.take(r.w, r.h)
        open.push({ kind: 'fx', owner: i, end: subtreeEnd[i], layer, prev: g, prevOff: off, rect: { x: r.x, y: r.y } })
        g = layer.getContext('2d')!
        off = { x: r.x, y: r.y }
      }
      const before = clipDepth.length
      drawNode(i)
      if (clipDepth.length > before) {
        if (clipDepth[clipDepth.length - 1] === -1) clipDepth.pop()
        else open.push({ kind: 'clip', end: subtreeEnd[i] })
      }
    }
    while (open.length) close(open.pop()!)
    // The global texture, last, over everything drawn.
    if (file.texture) applyTexture(this.canvas, file.texture, root, this.clock, this.wake)
  }

  private isDescendant(node: number, ancestor: number): boolean {
    for (let k = this.file.nodes[node].parent; k != null; k = this.file.nodes[k].parent) if (k === ancestor) return true
    return false
  }
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

function css(c: ArrayLike<number>): string {
  const ch = (v: number) => Math.round(Math.min(1, Math.max(0, v)) * 255)
  return `rgba(${ch(c[0])},${ch(c[1])},${ch(c[2])},${Math.min(1, Math.max(0, c[3] ?? 1))})`
}

function mulColor(a: ArrayLike<number>, b: Color): Color {
  return [a[0] * b[0], a[1] * b[1], a[2] * b[2], (a[3] ?? 1) * b[3]]
}

function gradientStyle(ctx: CanvasRenderingContext2D, g: LiteGradient, tint: Color): CanvasGradient {
  const grad =
    g.kind === 'linear'
      ? ctx.createLinearGradient(g.start[0], g.start[1], g.end[0], g.end[1])
      : ctx.createRadialGradient(g.start[0], g.start[1], 0, g.start[0], g.start[1], Math.hypot(g.end[0] - g.start[0], g.end[1] - g.start[1]) || 1)
  for (const s of g.stops) grad.addColorStop(Math.min(1, Math.max(0, s.offset)), css(mulColor(s.color, tint)))
  return grad
}

function parseLoose(text: string): unknown {
  try {
    return JSON.parse(text)
  } catch {
    return text
  }
}

function coerce(kind: 'number' | 'bool' | 'text' | 'color', value: unknown): unknown {
  switch (kind) {
    case 'number': {
      const n = Number(value)
      return Number.isFinite(n) ? n : 0
    }
    case 'bool':
      return value === true || value === 1 || value === 'true' || value === '1'
    case 'text':
      return value == null ? '' : String(value)
    case 'color': {
      if (Array.isArray(value)) return [Number(value[0]), Number(value[1]), Number(value[2]), value.length > 3 ? Number(value[3]) : 1]
      if (typeof value === 'string' && /^#?[0-9a-f]{6}$/i.test(value)) {
        const h = value.replace('#', '')
        return [parseInt(h.slice(0, 2), 16) / 255, parseInt(h.slice(2, 4), 16) / 255, parseInt(h.slice(4, 6), 16) / 255, 1]
      }
      return [1, 1, 1, 1]
    }
  }
}

function sameValue(a: unknown, b: unknown): boolean {
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((v, i) => v === b[i])
  return Object.is(a, b)
}

function fmt(v: unknown): string {
  if (typeof v === 'number') return Number.isInteger(v) ? String(v) : String(Math.round(v * 100) / 100)
  return String(v)
}
