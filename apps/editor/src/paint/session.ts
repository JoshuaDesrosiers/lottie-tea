/**
 * Paint mode's frames × layers model — the editing-side twin of
 * `an1mbyt3_core::DrawingAnimation`.
 *
 * scratch-paint edits exactly one image at a time, so an animation is a grid
 * of *cels* (one per layer per frame) and the editor swaps the active cel in
 * and out of scratch-paint (`PaintAssetEditor`). Every cel keeps scratch-
 * paint's own export of it plus its **rotation center** — the registration
 * point that lines frames up. With it, any cel can be redrawn at exactly the
 * spot it occupies on scratch-paint's art board (`renderCel`), which is all
 * the frame strip needs for thumbnails, onion skins and the playback preview
 * (no reaching into paper.js).
 *
 * Coordinates (scratch-paint's own, see its `helper/view.js`): the art board
 * is 960×720 working units. A vector cel's SVG is exported at half scale, its
 * center in SVG units, and `center * 2` sits on the art-board center
 * (480, 360). A bitmap cel's pixels are working units 1:1, its center in
 * pixels.
 */
import type { OpaqueId } from '../ipc/document'

export const ART_WIDTH = 960
export const ART_HEIGHT = 720

export type Center = [number, number] | null

/** One layer's artwork on one frame. */
export type PaintCel =
  /** Vector mode: scratch-paint's SVG export. */
  | { kind: 'svg'; svg: string; center: Center }
  /** Bitmap mode, edited this session (not uploaded yet). */
  | { kind: 'bitmap'; bitmap: ImageData; center: Center }
  /** Bitmap mode, already saved as an image asset (unchanged since). */
  | { kind: 'image'; image: OpaqueId; dataUrl: string; center: Center }

export interface PaintLayer {
  name: string
  visible: boolean
  /** 0..1 */
  opacity: number
  /** One per frame; `null` = empty. */
  cels: (PaintCel | null)[]
}

export interface PaintSession {
  layers: PaintLayer[]
  frameCount: number
  currentFrame: number
  currentLayer: number
  fps: number
  /** Show the neighbouring frames faintly under the current one. */
  onion: boolean
}

export function blankSession(): PaintSession {
  return { layers: [{ name: 'Layer 1', visible: true, opacity: 1, cels: [null] }], frameCount: 1, currentFrame: 0, currentLayer: 0, fps: 12, onion: true }
}

/** `session` with `live` (scratch-paint's latest export) stored as the
 * current cel — call before switching cels or saving. */
export function withLive(session: PaintSession, live: PaintCel | null): PaintSession {
  if (!live) return session
  const layers = session.layers.map((layer, i) =>
    i === session.currentLayer ? { ...layer, cels: layer.cels.map((cel, f) => (f === session.currentFrame ? live : cel)) } : layer,
  )
  return { ...session, layers }
}

export function celAt(session: PaintSession, layer: number, frame: number): PaintCel | null {
  return session.layers[layer]?.cels[frame] ?? null
}

/** A bitmap export that's fully transparent counts as empty. */
export function isEmptyCel(cel: PaintCel | null): boolean {
  if (!cel) return true
  if (cel.kind === 'svg') return !/<(path|rect|ellipse|circle|polygon|polyline|line|text|image)\b/.test(cel.svg)
  if (cel.kind === 'bitmap') {
    const data = cel.bitmap.data
    for (let i = 3; i < data.length; i += 4) if (data[i] !== 0) return false
    return true
  }
  return false
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image()
    img.onload = () => resolve(img)
    img.onerror = () => reject(new Error('image failed to load'))
    img.src = src
  })
}

const celCanvases = new WeakMap<object, Promise<HTMLCanvasElement>>()

/** `cel` drawn where it sits on scratch-paint's art board (960×720,
 * transparent elsewhere). Cached per cel object. */
export function renderCel(cel: PaintCel): Promise<HTMLCanvasElement> {
  const key = cel.kind === 'bitmap' ? cel.bitmap : cel
  let entry = celCanvases.get(key)
  if (!entry) {
    entry = drawCel(cel)
    celCanvases.set(key, entry)
  }
  return entry
}

async function drawCel(cel: PaintCel): Promise<HTMLCanvasElement> {
  const canvas = document.createElement('canvas')
  canvas.width = ART_WIDTH
  canvas.height = ART_HEIGHT
  const context = canvas.getContext('2d')!
  if (cel.kind === 'svg') {
    const img = await loadImage(`data:image/svg+xml;charset=utf-8,${encodeURIComponent(cel.svg)}`).catch(() => null)
    if (!img) return canvas
    const w = img.naturalWidth * 2
    const h = img.naturalHeight * 2
    const [cx, cy] = cel.center ?? [img.naturalWidth / 2, img.naturalHeight / 2]
    context.drawImage(img, ART_WIDTH / 2 - cx * 2, ART_HEIGHT / 2 - cy * 2, w, h)
  } else {
    let source: CanvasImageSource
    let w: number
    let h: number
    if (cel.kind === 'bitmap') {
      const scratch = document.createElement('canvas')
      scratch.width = cel.bitmap.width
      scratch.height = cel.bitmap.height
      scratch.getContext('2d')!.putImageData(cel.bitmap, 0, 0)
      source = scratch
      w = scratch.width
      h = scratch.height
    } else {
      const img = await loadImage(cel.dataUrl).catch(() => null)
      if (!img) return canvas
      source = img
      w = img.naturalWidth
      h = img.naturalHeight
    }
    const [cx, cy] = cel.center ?? [w / 2, h / 2]
    context.drawImage(source, ART_WIDTH / 2 - cx, ART_HEIGHT / 2 - cy)
  }
  return canvas
}

/** Every visible layer of `frame` composited bottom to top (with layer
 * opacity). `override` replaces one layer's cel (the live, unsaved one). */
export async function renderFrame(session: PaintSession, frame: number, override?: { layer: number; cel: PaintCel | null }): Promise<HTMLCanvasElement> {
  const canvas = document.createElement('canvas')
  canvas.width = ART_WIDTH
  canvas.height = ART_HEIGHT
  const context = canvas.getContext('2d')!
  for (let l = 0; l < session.layers.length; l++) {
    const layer = session.layers[l]
    if (!layer.visible) continue
    const cel = override && override.layer === l ? override.cel : layer.cels[frame]
    if (!cel) continue
    context.globalAlpha = layer.opacity
    context.drawImage(await renderCel(cel), 0, 0)
  }
  return canvas
}

/** Visible layers `from..to` (inclusive, bottom to top) of `frame`, each
 * at its opacity — the layers below / above the one being edited. */
export async function renderLayers(session: PaintSession, frame: number, from: number, to: number): Promise<HTMLCanvasElement | null> {
  let canvas: HTMLCanvasElement | null = null
  for (let l = Math.max(0, from); l <= Math.min(to, session.layers.length - 1); l++) {
    const layer = session.layers[l]
    const cel = layer.cels[frame]
    if (!layer.visible || !cel) continue
    if (!canvas) {
      canvas = document.createElement('canvas')
      canvas.width = ART_WIDTH
      canvas.height = ART_HEIGHT
    }
    const context = canvas.getContext('2d')!
    context.globalAlpha = layer.opacity
    context.drawImage(await renderCel(cel), 0, 0)
  }
  return canvas
}

/** `top` drawn over `bottom` (either may be missing). */
export function stack(bottom: HTMLCanvasElement | null, top: HTMLCanvasElement | null): HTMLCanvasElement | null {
  if (!bottom || !top) return bottom ?? top
  const canvas = document.createElement('canvas')
  canvas.width = ART_WIDTH
  canvas.height = ART_HEIGHT
  const context = canvas.getContext('2d')!
  context.drawImage(bottom, 0, 0)
  context.drawImage(top, 0, 0)
  return canvas
}

/** The onion skin: the previous frame tinted red, the next tinted blue,
 * both faint — the Flash / Aseprite convention. */
export function onionSkin(previous: HTMLCanvasElement | null, next: HTMLCanvasElement | null): HTMLCanvasElement | null {
  if (!previous && !next) return null
  const canvas = document.createElement('canvas')
  canvas.width = ART_WIDTH
  canvas.height = ART_HEIGHT
  const context = canvas.getContext('2d')!
  const tinted = (source: HTMLCanvasElement, color: string, alpha: number) => {
    const layer = document.createElement('canvas')
    layer.width = ART_WIDTH
    layer.height = ART_HEIGHT
    const c = layer.getContext('2d')!
    c.drawImage(source, 0, 0)
    c.globalCompositeOperation = 'source-atop'
    c.fillStyle = color
    c.globalAlpha = 0.55
    c.fillRect(0, 0, ART_WIDTH, ART_HEIGHT)
    context.globalAlpha = alpha
    context.drawImage(layer, 0, 0)
  }
  if (next) tinted(next, '#3b82f6', 0.22)
  if (previous) tinted(previous, '#ef4444', 0.32)
  return canvas
}

/** A grid over the art board every `size` working units (2 per SVG unit),
 * the center lines a little stronger — drawn over the artwork as a guide. */
export function gridCanvas(size: number): HTMLCanvasElement {
  const canvas = document.createElement('canvas')
  canvas.width = ART_WIDTH
  canvas.height = ART_HEIGHT
  const context = canvas.getContext('2d')!
  // The art board is shown at about half scale, so lines are 2 units wide
  // to land on ~1 screen pixel.
  const line = (x0: number, y0: number, x1: number, y1: number, strong: boolean) => {
    context.strokeStyle = strong ? 'rgba(70, 80, 170, 0.55)' : 'rgba(70, 80, 170, 0.28)'
    context.lineWidth = strong ? 3 : 2
    context.beginPath()
    context.moveTo(x0, y0)
    context.lineTo(x1, y1)
    context.stroke()
  }
  for (let x = ART_WIDTH / 2 % size; x <= ART_WIDTH; x += size) line(x, 0, x, ART_HEIGHT, Math.abs(x - ART_WIDTH / 2) < 0.5)
  for (let y = ART_HEIGHT / 2 % size; y <= ART_HEIGHT; y += size) line(0, y, ART_WIDTH, y, Math.abs(y - ART_HEIGHT / 2) < 0.5)
  return canvas
}

/** A reference image to trace over: fitted inside the art board (aspect
 * kept), at `opacity`. */
export function referenceCanvas(image: HTMLImageElement, opacity: number): HTMLCanvasElement {
  const canvas = document.createElement('canvas')
  canvas.width = ART_WIDTH
  canvas.height = ART_HEIGHT
  const context = canvas.getContext('2d')!
  const scale = Math.min(ART_WIDTH / image.naturalWidth, ART_HEIGHT / image.naturalHeight)
  const w = image.naturalWidth * scale
  const h = image.naturalHeight * scale
  context.globalAlpha = opacity
  context.drawImage(image, (ART_WIDTH - w) / 2, (ART_HEIGHT - h) / 2, w, h)
  return canvas
}

/** A small data-URL thumbnail of an art-board canvas, cropped to the art
 * board's middle (scratch-paint's visible area). */
export function thumbnail(source: HTMLCanvasElement, width = 96): string {
  const canvas = document.createElement('canvas')
  canvas.width = width
  canvas.height = Math.round((width * ART_HEIGHT) / ART_WIDTH)
  const context = canvas.getContext('2d')!
  context.drawImage(source, 0, 0, canvas.width, canvas.height)
  return canvas.toDataURL()
}

/** A 2×2 fully transparent PNG — what a blank bitmap-mode cel loads as, so
 * scratch-paint stays in bitmap mode (a `null` image would switch it back
 * to vector). */
export const BLANK_PNG = (() => {
  if (typeof document === 'undefined') return ''
  const canvas = document.createElement('canvas')
  canvas.width = 2
  canvas.height = 2
  return canvas.toDataURL('image/png')
})()
