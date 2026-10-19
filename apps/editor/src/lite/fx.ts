/**
 * **Toasty Lite image effects** — the Canvas2D side of the subset of
 * `an1mbyt3_core::ImageEffects` Lite exports (see `lite.rs`'s
 * `lite_image_effects`): an element with effects is drawn into its own
 * offscreen canvas ("layer"), these run on it, and the result is
 * composited back — the same model as the full engine's `layer_fx`, done
 * with Canvas2D's own tools instead of shaders:
 *
 * | Effect | How |
 * |---|---|
 * | background | rounded rect drawn *under* the content (`destination-over`) |
 * | drop shadow | tinted silhouette, `filter: blur()`, offset, under |
 * | stroke (outside) | tinted silhouette stamped in a ring, under |
 * | blur | `filter: blur()` |
 * | glow | blurred, brightened copy added (`lighter`) |
 * | grain | a noise tile, blended, trimmed back to the shape |
 * | texture | a procedural / image tile pattern in the node's space, blended, trimmed |
 * | gradient map | per-pixel lookup (getImageData) |
 * | clip / feather | the content's own alpha (feather: blurred + eased) as a mask |
 *
 * Procedural texture tiles are generated once per (source, detail,
 * contrast, tint) and cached; they tile seamlessly (periodic noise).
 */

export type Color = [number, number, number, number]

export interface LiteTexture {
  type: 'texture'
  source: string
  image: string | null
  blend: string
  opacity: number
  scale: number
  rotation: number
  offset: [number, number]
  contrast: number
  tint: Color
  speed: number
  detail: number
}

export type LiteEffect =
  | { type: 'shadow'; color: Color; offset: [number, number]; blur: number; spread: number }
  | { type: 'stroke'; color: Color; width: number }
  | { type: 'blur'; radius: number }
  | { type: 'glow'; intensity: number; radius: number; tint: Color }
  | { type: 'grain'; amount: number; size: number; animated: boolean; mono: boolean }
  | { type: 'gradientMap'; amount: number; smooth: number; offset: number; stops: { position: number; color: Color }[] }
  | LiteTexture

export interface LiteFx {
  effects: LiteEffect[]
  background: Color
  radius: number
  padding: number
  clip: boolean
  feather: number
}

const css = (c: ArrayLike<number>) => `rgba(${Math.round(c[0] * 255)},${Math.round(c[1] * 255)},${Math.round(c[2] * 255)},${c[3]})`

// ---- canvases ---------------------------------------------------------------

/** A reusable scratch canvas of at least `w`×`h`. */
class Pool {
  private free: HTMLCanvasElement[] = []
  take(w: number, h: number): HTMLCanvasElement {
    // Prefer a free canvas already this size: resizing reallocates its
    // pixels, and layers come in a few steady sizes frame to frame.
    const same = this.free.findIndex((f) => f.width === w && f.height === h)
    const c = same >= 0 ? this.free.splice(same, 1)[0] : (this.free.pop() ?? document.createElement('canvas'))
    if (c.width !== w || c.height !== h) {
      c.width = w
      c.height = h
    }
    // Reused canvases come back with a clean slate: identity transform,
    // default compositing, cleared.
    const g = c.getContext('2d')!
    g.setTransform(1, 0, 0, 1, 0, 0)
    g.globalCompositeOperation = 'source-over'
    g.globalAlpha = 1
    g.filter = 'none'
    g.clearRect(0, 0, w, h)
    return c
  }
  give(c: HTMLCanvasElement) {
    this.free.push(c)
  }
}
export const pool = new Pool()

function ctx2d(c: HTMLCanvasElement): CanvasRenderingContext2D {
  return c.getContext('2d', { willReadFrequently: false })!
}

/** A copy of `src`. */
function copy(src: HTMLCanvasElement): HTMLCanvasElement {
  const c = pool.take(src.width, src.height)
  ctx2d(c).drawImage(src, 0, 0)
  return c
}

/** `src`'s silhouette filled with `color`. */
function silhouette(src: HTMLCanvasElement, color: Color): HTMLCanvasElement {
  const c = copy(src)
  const g = ctx2d(c)
  g.globalCompositeOperation = 'source-in'
  g.fillStyle = css(color)
  g.fillRect(0, 0, c.width, c.height)
  g.globalCompositeOperation = 'source-over'
  return c
}

// ---- periodic noise (seamless tiles) ----------------------------------------

function hash(x: number, y: number, seed: number): number {
  let h = (x * 374761393 + y * 668265263 + seed * 2147483647) | 0
  h = Math.imul(h ^ (h >>> 13), 1274126177)
  return ((h ^ (h >>> 16)) >>> 0) / 4294967295
}

/** Value noise with period `p` (in cells) — tiles seamlessly. */
function vnoise(x: number, y: number, p: number, seed = 0): number {
  const ix = Math.floor(x)
  const iy = Math.floor(y)
  const fx = x - ix
  const fy = y - iy
  const w = (t: number) => t * t * (3 - 2 * t)
  const m = (v: number) => ((v % p) + p) % p
  const a = hash(m(ix), m(iy), seed)
  const b = hash(m(ix + 1), m(iy), seed)
  const c = hash(m(ix), m(iy + 1), seed)
  const d = hash(m(ix + 1), m(iy + 1), seed)
  const u = w(fx)
  const v = w(fy)
  return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

/** Periodic fBm: `u, v` in tile units (0..1), `base` cells at octave 0. */
function fbm(u: number, v: number, base: number, octaves: number, seed = 0): number {
  let sum = 0
  let amp = 0.5
  let f = base
  for (let o = 0; o < octaves; o++) {
    sum += vnoise(u * f, v * f, f, seed + o * 17) * amp
    amp *= 0.5
    f *= 2
  }
  return sum
}

/** One procedural surface, grey around 0.5, at tile coords `u, v` (0..1). */
function surface(source: string, u: number, v: number, detail: number): number {
  const oct = 3 + Math.round(detail * 3)
  const TAU = Math.PI * 2
  switch (source) {
    case 'Canvas': {
      const n = 16
      const tx = u * n
      const ty = v * n
      const cx = Math.floor(tx)
      const cy = Math.floor(ty)
      const fx = tx - cx - 0.5
      const fy = ty - cy - 0.5
      const over = (cx + cy) & 1
      const thx = 0.42 + (vnoise(cy * 2.3, 1, n) - 0.5) * 0.12
      const thy = 0.42 + (vnoise(3, cx * 2.9, n) - 0.5) * 0.12
      const warp = Math.sqrt(Math.max(0, 1 - (fy / thx) ** 2))
      const weft = Math.sqrt(Math.max(0, 1 - (fx / thy) ** 2))
      const top = over ? warp : weft
      const under = (over ? weft : warp) * 0.55
      const t = Math.max(top, under) * (0.82 + 0.3 * vnoise(tx * 0.35, ty * 0.35, n * 0.35 > 1 ? Math.round(n * 0.35) : 1, 5))
      return 0.24 + t * 0.5 + (fbm(u, v, 22, oct, 9) - 0.5) * 0.12
    }
    case 'Paper':
      return 0.4 + fbm(u, v, 3, oct) * 0.18 + fbm(u, v * 7, 2, 3, 3) * 0.1 + vnoise(u * 40, v * 40, 40, 7) * 0.08 * (0.5 + detail)
    case 'Linen': {
      const a = 0.5 + 0.5 * Math.sin(u * 14 * TAU + vnoise(u * 4, v * 28, 4, 1) * 2)
      const b = 0.5 + 0.5 * Math.sin(v * 14 * TAU + vnoise(u * 28, v * 4, 28, 2) * 2)
      return 0.35 + a * b * 0.25 + (a + b) * 0.08 + (fbm(u, v, 5, 3, 4) - 0.5) * 0.15
    }
    case 'Noise':
      return fbm(u, v, 4, oct)
    case 'Waves': {
      const bend = fbm(u, v, 2, 3, 6) * 3
      return 0.5 + Math.sin((v + Math.sin(u * TAU) * 0.08) * 4 * TAU + bend) * 0.32
    }
    case 'Fluid': {
      const ax = fbm(u, v, 2, 4, 11)
      const ay = fbm(u, v, 2, 4, 23)
      return 0.25 + fbm(u + ax * 0.3, v + ay * 0.3, 2, oct, 31) * 0.7
    }
    case 'Marble': {
      const turb = fbm(u, v, 2, oct, 12) * 6
      const vein = Math.abs(Math.sin((u + v * 0.4) * 2 * TAU + turb))
      return 0.32 + Math.pow(vein, 0.35) * 0.42
    }
    case 'Concrete': {
      const base = fbm(u, v, 4, oct, 2)
      const pits = vnoise(u * 32, v * 32, 32, 8) > 0.8 ? 1 : 0
      const specks = hash(Math.floor(u * 96), Math.floor(v * 96), 13) > 0.985 ? 1 : 0
      return 0.36 + base * 0.28 - pits * 0.18 + specks * 0.15
    }
    case 'Dust': {
      const speck = hash(Math.floor(u * 48), Math.floor(v * 48), 3) > 0.97 ? 1 : 0
      const lane = Math.floor(u * 9)
      const lr = hash(lane, 3, 5)
      const scratch = lr > 0.8 && Math.abs(u * 9 - lane - lr) < 0.02 && vnoise(lane, v * 3, 3, 4) > 0.3 ? 1 : 0
      return 0.5 - speck * 0.35 + scratch * 0.3
    }
    case 'Brushed':
      return 0.3 + fbm(u, v * 16, 1, oct, 21) * 0.45
    default:
      return 0.5
  }
}

const tiles = new Map<string, HTMLCanvasElement>()

/** The (cached) tile canvas for a texture: 256² procedural, or the image. */
function textureTile(t: LiteTexture, onLoad: () => void): CanvasImageSource | null {
  if (t.source === 'Image') {
    if (!t.image) return null
    const key = `img:${t.image.slice(0, 64)}:${t.image.length}`
    const cached = images.get(key)
    if (cached) return cached.complete ? cached : null
    const img = new Image()
    img.onload = onLoad
    img.src = t.image
    images.set(key, img)
    return null
  }
  const key = `${t.source}:${t.detail.toFixed(2)}:${t.contrast.toFixed(2)}:${t.tint.join(',')}`
  let tile = tiles.get(key)
  if (!tile) {
    const N = 256
    tile = document.createElement('canvas')
    tile.width = tile.height = N
    const g = tile.getContext('2d')!
    const data = g.createImageData(N, N)
    const k = 1 + t.contrast * 1.5
    for (let y = 0; y < N; y++)
      for (let x = 0; x < N; x++) {
        const s = Math.min(1, Math.max(0, (surface(t.source, x / N, y / N, t.detail) - 0.5) * k + 0.5))
        const o = (y * N + x) * 4
        data.data[o] = s * t.tint[0] * 255
        data.data[o + 1] = s * t.tint[1] * 255
        data.data[o + 2] = s * t.tint[2] * 255
        data.data[o + 3] = 255
      }
    g.putImageData(data, 0, 0)
    tiles.set(key, tile)
  }
  return tile
}
const images = new Map<string, HTMLImageElement>()

/** Texture blend → Canvas2D composite op. */
function blendOp(blend: string): GlobalCompositeOperation {
  const map: Record<string, GlobalCompositeOperation> = {
    Normal: 'source-over',
    Multiply: 'multiply',
    Screen: 'screen',
    Overlay: 'overlay',
    SoftLight: 'soft-light',
    HardLight: 'hard-light',
    ColorDodge: 'color-dodge',
    ColorBurn: 'color-burn',
    Darken: 'darken',
    Lighten: 'lighten',
    Difference: 'difference',
    LinearLight: 'hard-light',
  }
  return map[blend] ?? 'soft-light'
}

/**
 * Lays texture `t` over everything already on `layer`, trimmed back to
 * what was there (the texture never adds coverage). `world` maps the
 * node's units to layer pixels, so the texture moves / scales with it.
 */
export function applyTexture(layer: HTMLCanvasElement, t: LiteTexture, world: DOMMatrix, time: number, onLoad: () => void) {
  const tile = textureTile(t, onLoad)
  if (!tile) return
  const g = ctx2d(layer)
  const shape = copy(layer)
  const pattern = g.createPattern(tile, 'repeat')
  if (pattern) {
    const tileSize = t.source === 'Image' ? Math.max((tile as HTMLImageElement).naturalWidth || 256, 1) : 256
    const drift = ['Fluid', 'Waves', 'Noise'].includes(t.source) ? time * t.speed * 0.15 : 0
    const m = new DOMMatrix(world.toString())
      .rotate(t.rotation)
      .scale(t.scale / tileSize, -t.scale / tileSize)
      .translate((t.offset[0] + drift) * tileSize, (t.offset[1] + drift * 0.6) * tileSize)
    pattern.setTransform(m)
    g.save()
    g.setTransform(1, 0, 0, 1, 0, 0)
    g.globalCompositeOperation = blendOp(t.blend)
    g.globalAlpha = Math.min(1, Math.max(0, t.opacity * t.tint[3]))
    g.fillStyle = pattern
    g.fillRect(0, 0, layer.width, layer.height)
    g.globalAlpha = 1
    g.globalCompositeOperation = 'destination-in'
    g.drawImage(shape, 0, 0)
    g.restore()
  }
  pool.give(shape)
}

let grainTile: HTMLCanvasElement | null = null
let grainTileColor: HTMLCanvasElement | null = null
function grainCanvas(mono: boolean): HTMLCanvasElement {
  const make = () => {
    const c = document.createElement('canvas')
    c.width = c.height = 128
    const g = c.getContext('2d')!
    const d = g.createImageData(128, 128)
    for (let i = 0; i < 128 * 128; i++) {
      const r = Math.random() * 255
      d.data[i * 4] = r
      d.data[i * 4 + 1] = mono ? r : Math.random() * 255
      d.data[i * 4 + 2] = mono ? r : Math.random() * 255
      d.data[i * 4 + 3] = 255
    }
    g.putImageData(d, 0, 0)
    return c
  }
  if (mono) return (grainTile ??= make())
  return (grainTileColor ??= make())
}

/** Runs the element's effects on `layer` (its content just drawn there). */
export function applyFx(layer: HTMLCanvasElement, fx: LiteFx, world: DOMMatrix, box: number[] | undefined, devScale: number, time: number, onLoad: () => void) {
  const g = ctx2d(layer)
  // The content's own alpha — clip / feather / trims use it.
  const shape = fx.clip || fx.feather > 0 ? copy(layer) : null

  // Background, under the content.
  if (fx.background[3] > 0 && box) {
    const [x0, y0, x1, y1] = box
    const pad = fx.padding
    g.save()
    g.setTransform(world)
    g.globalCompositeOperation = 'destination-over'
    g.fillStyle = css(fx.background)
    g.beginPath()
    const w = x1 - x0 + pad * 2
    const h = y1 - y0 + pad * 2
    g.roundRect(x0 - pad, y0 - pad, w, h, Math.min(fx.radius, Math.min(w, h) / 2))
    g.fill()
    g.restore()
  }

  for (const e of fx.effects) {
    if (e.type === 'shadow') {
      const sil = silhouette(layer, e.color)
      g.save()
      g.setTransform(1, 0, 0, 1, 0, 0)
      g.globalCompositeOperation = 'destination-over'
      g.filter = `blur(${Math.max(0, e.blur * devScale * 0.5)}px)`
      g.drawImage(sil, e.offset[0] * devScale, -e.offset[1] * devScale)
      g.restore()
      pool.give(sil)
    } else if (e.type === 'stroke' && e.width > 0) {
      const sil = silhouette(layer, e.color)
      const r = e.width * devScale
      g.save()
      g.setTransform(1, 0, 0, 1, 0, 0)
      g.globalCompositeOperation = 'destination-over'
      for (let ring = 1; ring <= 2; ring++)
        for (let k = 0; k < 16; k++) {
          const a = (k / 16) * Math.PI * 2 + ring * 0.3
          g.drawImage(sil, Math.cos(a) * r * (ring / 2), Math.sin(a) * r * (ring / 2))
        }
      g.restore()
      pool.give(sil)
    } else if (e.type === 'blur' && e.radius > 0) {
      const c = copy(layer)
      g.save()
      g.setTransform(1, 0, 0, 1, 0, 0)
      g.clearRect(0, 0, layer.width, layer.height)
      g.filter = `blur(${e.radius * devScale * 0.5}px)`
      g.drawImage(c, 0, 0)
      g.restore()
      pool.give(c)
    } else if (e.type === 'glow') {
      const c = e.tint.slice(0, 3).every((v) => v >= 0.999) ? copy(layer) : silhouette(layer, e.tint)
      g.save()
      g.setTransform(1, 0, 0, 1, 0, 0)
      g.globalCompositeOperation = 'lighter'
      g.filter = `blur(${e.radius * devScale * 0.5}px) brightness(${Math.max(0, e.intensity)})`
      g.drawImage(c, 0, 0)
      g.restore()
      pool.give(c)
    } else if (e.type === 'grain' && e.amount > 0) {
      const tile = grainCanvas(e.mono)
      const pattern = g.createPattern(tile, 'repeat')
      if (pattern) {
        const s = Math.max(1, e.size * devScale)
        const jitter = e.animated ? Math.floor(time * 24) : 0
        pattern.setTransform(new DOMMatrix().translate((jitter * 37) % 128, (jitter * 53) % 128).scale(s, s))
        const before = copy(layer)
        g.save()
        g.setTransform(1, 0, 0, 1, 0, 0)
        g.globalCompositeOperation = 'overlay'
        g.globalAlpha = Math.min(1, e.amount * 1.6)
        g.fillStyle = pattern
        g.fillRect(0, 0, layer.width, layer.height)
        g.globalAlpha = 1
        g.globalCompositeOperation = 'destination-in'
        g.drawImage(before, 0, 0)
        g.restore()
        pool.give(before)
      }
    } else if (e.type === 'gradientMap' && e.stops.length > 0) {
      gradientMap(layer, e)
    } else if (e.type === 'texture') {
      applyTexture(layer, e, world, time, onLoad)
    }
  }

  if (shape && fx.clip) {
    g.save()
    g.setTransform(1, 0, 0, 1, 0, 0)
    g.globalCompositeOperation = 'destination-in'
    g.drawImage(shape, 0, 0)
    g.restore()
  }
  if (shape && fx.feather > 0) {
    const mask = featherMask(shape, fx.feather * devScale)
    g.save()
    g.setTransform(1, 0, 0, 1, 0, 0)
    g.globalCompositeOperation = 'destination-in'
    g.drawImage(mask, 0, 0)
    g.restore()
    pool.give(mask)
  }
  if (shape) pool.give(shape)
}

// ---- SVG filters (stay on the GPU: no pixel readback) -------------------

/** Canvas `filter` (incl. `url(#svg-filter)`) works here. Without it (old
 * Safari) feather / gradient map fall back to per-pixel loops. */
const canFilter = typeof CanvasRenderingContext2D !== 'undefined' && 'filter' in CanvasRenderingContext2D.prototype

let svgRoot: SVGSVGElement | null = null
/** Live filter elements by id, oldest first (an animated gradient map
 * makes a new one per step, so only the most recent are kept). */
const svgFilters = new Map<string, SVGFilterElement>()
const MAX_SVG_FILTERS = 48
/** Ensures an SVG `<filter id>` with this body exists in the page (sRGB
 * maths, like the canvas). Returns its `url(#id)`. */
function svgFilter(id: string, body: string): string {
  if (!svgFilters.has(id)) {
    if (!svgRoot) {
      svgRoot = document.createElementNS('http://www.w3.org/2000/svg', 'svg')
      svgRoot.setAttribute('width', '0')
      svgRoot.setAttribute('height', '0')
      svgRoot.setAttribute('aria-hidden', 'true')
      svgRoot.style.cssText = 'position:absolute;width:0;height:0;overflow:hidden;pointer-events:none'
      document.body.appendChild(svgRoot)
    }
    const f = document.createElementNS('http://www.w3.org/2000/svg', 'filter')
    f.setAttribute('id', id)
    f.setAttribute('color-interpolation-filters', 'sRGB')
    f.innerHTML = body
    svgRoot.appendChild(f)
    svgFilters.set(id, f)
    if (svgFilters.size > MAX_SVG_FILTERS) {
      const [oldest, el] = svgFilters.entries().next().value!
      el.remove()
      svgFilters.delete(oldest)
    }
  }
  return `url(#${id})`
}

/** The shape's coverage blurred over `radius` px, then its alpha stretched
 * (×2 − 1) so it's 0 at the edge and 1 about a radius inside — the
 * engine's inward feather. */
function featherMask(shape: HTMLCanvasElement, radius: number): HTMLCanvasElement {
  if (!canFilter) return featherMaskPixels(shape, radius)
  const mask = pool.take(shape.width, shape.height)
  const m = ctx2d(mask)
  m.filter = `blur(${radius}px) ${svgFilter('an1-lite-feather', '<feComponentTransfer><feFuncA type="linear" slope="2" intercept="-1"/></feComponentTransfer>')}`
  m.drawImage(shape, 0, 0)
  m.filter = 'none'
  return mask
}

/** 256-entry RGB lookup for a gradient map (stops, offset, smooth). */
function gradientLut(e: Extract<LiteEffect, { type: 'gradientMap' }>): Float32Array {
  const stops = [...e.stops].sort((a, b) => a.position - b.position)
  const lut = new Float32Array(256 * 4)
  for (let i = 0; i < 256; i++) {
    const l = Math.min(1, Math.max(0, i / 255 + e.offset))
    let c = stops[0].color
    if (l >= stops[stops.length - 1].position) c = stops[stops.length - 1].color
    for (let k = 0; k < stops.length - 1; k++) {
      const a = stops[k]
      const b = stops[k + 1]
      if (l >= a.position && l < b.position) {
        let t = (l - a.position) / Math.max(b.position - a.position, 1e-4)
        const hard = t < 0.5 ? 0 : 1
        const sm = t * t * (3 - 2 * t)
        t = hard + (sm - hard) * Math.min(1, Math.max(0, e.smooth))
        c = a.color.map((v, j) => v + (b.color[j] - v) * t) as Color
      }
    }
    lut.set(c, i * 4)
  }
  return lut
}

/** Gradient map as an SVG filter: luminance (feColorMatrix) looked up in
 * per-channel tables (feComponentTransfer) — one filter per distinct
 * gradient, cached; then mixed over the original by `amount`. */
function gradientMap(layer: HTMLCanvasElement, e: Extract<LiteEffect, { type: 'gradientMap' }>) {
  if (!canFilter) return gradientMapPixels(layer, e)
  const lut = gradientLut(e)
  const N = 48
  const table = (ch: number) =>
    Array.from({ length: N }, (_, k) => lut[Math.round((k / (N - 1)) * 255) * 4 + ch].toFixed(3)).join(' ')
  const r = table(0)
  const gg = table(1)
  const b = table(2)
  let h = 0
  for (const ch of r + gg + b) h = (Math.imul(h, 31) + ch.charCodeAt(0)) | 0
  const L = '0.2126 0.7152 0.0722 0 0'
  const url = svgFilter(
    `an1-lite-gm-${(h >>> 0).toString(36)}`,
    `<feColorMatrix type="matrix" values="${L} ${L} ${L} 0 0 0 1 0"/><feComponentTransfer><feFuncR type="table" tableValues="${r}"/><feFuncG type="table" tableValues="${gg}"/><feFuncB type="table" tableValues="${b}"/></feComponentTransfer>`,
  )
  const orig = copy(layer)
  const g = ctx2d(layer)
  g.save()
  g.setTransform(1, 0, 0, 1, 0, 0)
  g.globalCompositeOperation = 'source-atop'
  g.globalAlpha = Math.min(1, Math.max(0, e.amount))
  g.filter = url
  g.drawImage(orig, 0, 0)
  g.restore()
  pool.give(orig)
}

// ---- per-pixel fallbacks -------------------------------------------------

/** The shape's coverage blurred over `radius` px, eased so it's 0 at the
 * edge and 1 a radius inside (the engine's inward feather). */
function featherMaskPixels(shape: HTMLCanvasElement, radius: number): HTMLCanvasElement {
  const cov = pool.take(shape.width, shape.height)
  const g = ctx2d(cov)
  // Coverage: anything drawn = solid.
  g.drawImage(shape, 0, 0)
  g.globalCompositeOperation = 'source-in'
  g.fillStyle = '#fff'
  g.fillRect(0, 0, cov.width, cov.height)
  g.globalCompositeOperation = 'source-over'
  const mask = pool.take(shape.width, shape.height)
  const m = ctx2d(mask)
  m.filter = `blur(${radius}px)`
  m.drawImage(cov, 0, 0)
  m.filter = 'none'
  pool.give(cov)
  const data = m.getImageData(0, 0, mask.width, mask.height)
  const d = data.data
  for (let i = 3; i < d.length; i += 4) {
    const t = Math.min(1, Math.max(0, (d[i] / 255 - 0.5) * 2))
    d[i] = t * t * (3 - 2 * t) * 255
  }
  m.putImageData(data, 0, 0)
  return mask
}

function gradientMapPixels(layer: HTMLCanvasElement, e: Extract<LiteEffect, { type: 'gradientMap' }>) {
  const g = ctx2d(layer)
  const lut = gradientLut(e)
  const data = g.getImageData(0, 0, layer.width, layer.height)
  const d = data.data
  const amt = Math.min(1, Math.max(0, e.amount))
  for (let i = 0; i < d.length; i += 4) {
    const a = d[i + 3]
    if (a === 0) continue
    const lum = Math.round(0.2126 * d[i] + 0.7152 * d[i + 1] + 0.0722 * d[i + 2])
    const o = lum * 4
    d[i] += (lut[o] * 255 - d[i]) * amt
    d[i + 1] += (lut[o + 1] * 255 - d[i + 1]) * amt
    d[i + 2] += (lut[o + 2] * 255 - d[i + 2]) * amt
  }
  g.putImageData(data, 0, 0)
}

/** Whether these effects change over time (needs continuous frames). */
export function fxAnimated(effects: LiteEffect[]): boolean {
  return effects.some((e) => (e.type === 'grain' && e.animated) || (e.type === 'texture' && e.speed > 0 && ['Fluid', 'Waves', 'Noise'].includes(e.source)))
}
