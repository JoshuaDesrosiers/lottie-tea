/**
 * "Export animation" — renders the artboard's active animation, frame by
 * frame, straight off the same `WasmEditor.render()` call the live canvas
 * uses, and encodes it as:
 *
 * - **GIF** (`gifenc`, a small client-side encoder), opaque or with 1-bit
 *   transparency;
 * - **WebM video** (VP9 via WebCodecs + `mediabunny`'s muxer), opaque or
 *   with a real alpha channel — the format for overlays / stickers that
 *   video editors, OBS and browsers can composite;
 * - **MP4 video** (H.264), opaque (MP4 / H.264 can't carry alpha).
 *
 * Transparent exports switch the engine into its export mode
 * (`set_export_transparent`): frames clear to alpha 0 and skip the editor
 * grid. The editor's canvas is alpha-capable (premultiplied), and reading
 * it back through a 2D canvas un-premultiplies the pixels, so each frame
 * arrives as straight RGBA with the artwork's real coverage.
 */
import { GIFEncoder, quantize, applyPalette } from 'gifenc'
import { BufferTarget, CanvasSource, Mp4OutputFormat, Output, QUALITY_HIGH, WebMOutputFormat, canEncodeVideo } from 'mediabunny'
import type { WasmEditor } from './wasm-pkg/an1mbyt3_wasm.js'
import { CLEAR_COLOR, GRID_COLOR } from './theme'
import type { ArtboardSummary, OpaqueId } from './ipc/document'
import { writeFileRaw } from './ipc/nativeFile'

function idsEqual(a: OpaqueId | null, b: OpaqueId | null): boolean {
  return a === b || (!!a && !!b && a.idx === b.idx && a.version === b.version)
}

/** GIFs sample at 24fps (plenty to judge motion, small files); videos at 30. */
const GIF_FPS = 24
const VIDEO_FPS = 30

export class GifExportError extends Error {}

export type AnimationExportFormat = 'gif' | 'webm' | 'mp4'

export interface AnimationExportOptions {
  format: AnimationExportFormat
  /** Clear to transparent instead of the dark background (GIF / WebM only). */
  transparent: boolean
}

/** What each format is called in menus / file dialogs. */
export const EXPORT_FORMAT_INFO: Record<AnimationExportFormat, { label: string; extension: string; filter: string }> = {
  gif: { label: 'GIF', extension: 'gif', filter: 'Animated GIF (.gif)' },
  webm: { label: 'WebM video', extension: 'webm', filter: 'WebM video (.webm)' },
  mp4: { label: 'MP4 video', extension: 'mp4', filter: 'MP4 video (.mp4)' },
}

/** Back-compat wrapper: an opaque GIF. */
export async function exportActiveAnimationAsGif(editor: WasmEditor, canvas: HTMLCanvasElement, artboard: ArtboardSummary, outPath: string): Promise<void> {
  return exportActiveAnimation(editor, canvas, artboard, outPath, { format: 'gif', transparent: false })
}

/**
 * Renders `artboard`'s active animation and writes it to `outPath`.
 * Samples at exactly `t = i / fps` via `render()`'s own
 * `playback_scrub_time` parameter — the same mechanism the Timeline's
 * scrub uses — so exporting never touches the document or undo history.
 *
 * Fits the whole artboard into the canvas's current backing size,
 * centred, ignoring the live camera's pan / zoom: an export shows the
 * whole piece the same way every time.
 */
export async function exportActiveAnimation(editor: WasmEditor, canvas: HTMLCanvasElement, artboard: ArtboardSummary, outPath: string, options: AnimationExportOptions): Promise<void> {
  const animation = artboard.activeAnimation && artboard.animations.find((a) => idsEqual(a.id, artboard.activeAnimation))
  if (!animation) {
    throw new GifExportError('No active animation to export — switch to Animate mode and pick (or create) one first.')
  }
  if (canvas.width === 0 || canvas.height === 0) {
    throw new GifExportError('Canvas has no pixels yet — try again once the editor has finished laying out.')
  }
  const transparent = options.transparent && options.format !== 'mp4'
  const fps = options.format === 'gif' ? GIF_FPS : VIDEO_FPS
  // At least 2 frames even for a near-zero-length animation.
  const frameCount = Math.max(2, Math.round(animation.duration * fps))
  const zoom = Math.min(canvas.width / artboard.width, canvas.height / artboard.height)

  // Read back through a 2D canvas (un-premultiplies the alpha). Videos
  // need even dimensions (H.264 / VP9 chroma subsampling).
  const readback = document.createElement('canvas')
  readback.width = options.format === 'gif' ? canvas.width : canvas.width & ~1
  readback.height = options.format === 'gif' ? canvas.height : canvas.height & ~1
  const ctx2d = readback.getContext('2d', { willReadFrequently: options.format === 'gif', alpha: true })
  if (!ctx2d) throw new GifExportError('Could not create a 2D context to read the canvas back.')

  /** Renders frame `i` into `readback`. */
  const renderFrame = async (i: number) => {
    const t = (i / fps) % Math.max(animation.duration, 1e-6)
    // Always the dark clear colour, whatever the editor theme — the export
    // is the artwork, not this session's chrome preference.
    const [clearR, clearG, clearB] = CLEAR_COLOR.dark
    const [gridR, gridG, gridB] = GRID_COLOR.dark
    editor.render(0, 0, 0, zoom, t, window.devicePixelRatio || 1, clearR, clearG, clearB, gridR, gridG, gridB)
    // Read back in the same task as the render: the canvas then holds this
    // frame's drawing buffer. After it's presented (next compositor frame)
    // a transparent WebGPU / WebGL canvas may read back empty.
    ctx2d.clearRect(0, 0, readback.width, readback.height)
    ctx2d.drawImage(canvas, 0, 0)
    // Then yield a frame so the editor stays responsive.
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))
  }

  editor.set_export_transparent(transparent)
  try {
    if (options.format === 'gif') await encodeGif(readback, ctx2d, frameCount, transparent, renderFrame, outPath)
    else await encodeVideo(readback, options.format, frameCount, fps, transparent, renderFrame, outPath)
  } finally {
    editor.set_export_transparent(false)
  }
}

async function encodeGif(readback: HTMLCanvasElement, ctx2d: CanvasRenderingContext2D, frameCount: number, transparent: boolean, renderFrame: (i: number) => Promise<void>, outPath: string) {
  const gif = GIFEncoder()
  const format = transparent ? 'rgba4444' : 'rgb565'
  let palette: number[][] | null = null
  let transparentIndex = 0
  for (let i = 0; i < frameCount; i++) {
    await renderFrame(i)
    const { data } = ctx2d.getImageData(0, 0, readback.width, readback.height)
    // One palette from the first frame (this engine's flat palettes don't
    // need per-frame re-quantizing, which would make colours shimmer).
    if (!palette) {
      palette = quantize(data, 256, { format, oneBitAlpha: transparent })
      transparentIndex = Math.max(0, palette.findIndex((c) => c.length > 3 && c[3] === 0))
    }
    const indexed = applyPalette(data, palette, format)
    // `dispose: 2` clears each frame before the next, so transparent pixels
    // don't keep showing the previous frame.
    gif.writeFrame(indexed, readback.width, readback.height, { palette, delay: 1000 / GIF_FPS, transparent, transparentIndex, dispose: transparent ? 2 : -1 })
  }
  gif.finish()
  await writeFileRaw(outPath, gif.bytes())
}

async function encodeVideo(readback: HTMLCanvasElement, format: 'webm' | 'mp4', frameCount: number, fps: number, transparent: boolean, renderFrame: (i: number) => Promise<void>, outPath: string) {
  const codec = format === 'webm' ? 'vp9' : 'avc'
  if (!(await canEncodeVideo(codec, { width: readback.width, height: readback.height }))) {
    throw new GifExportError(`This system can't encode ${format === 'webm' ? 'VP9 (WebM)' : 'H.264 (MP4)'} video.`)
  }
  const target = new BufferTarget()
  const output = new Output({ format: format === 'webm' ? new WebMOutputFormat() : new Mp4OutputFormat(), target })
  const source = new CanvasSource(readback, { codec, bitrate: QUALITY_HIGH, alpha: transparent ? 'keep' : 'discard' })
  output.addVideoTrack(source, { frameRate: fps })
  await output.start()
  for (let i = 0; i < frameCount; i++) {
    await renderFrame(i)
    await source.add(i / fps, 1 / fps)
  }
  await output.finalize()
  if (!target.buffer) throw new GifExportError('The video encoder produced no output.')
  await writeFileRaw(outPath, new Uint8Array(target.buffer))
}
