/**
 * "Export GIF" — renders the artboard's active animation, frame by frame,
 * straight off the same `WasmEditor.render()` call the live canvas uses,
 * and encodes the result into an animated `.gif` via `gifenc` (a small,
 * dependency-free client-side encoder — no server round-trip, no native
 * GIF-writing crate needed on the Rust side).
 *
 * The point isn't a polished "share this clip" feature — it's a debugging
 * aid: dropping a Lottie file's real motion next to an1mbyt3's own render
 * of the same file (side by side, or in an image diff tool) is a much
 * faster way to spot import bugs than eyeballing the live canvas against a
 * mental model of "what it should look like."
 */
import { GIFEncoder, quantize, applyPalette } from 'gifenc'
import type { WasmEditor } from './wasm-pkg/an1mbyt3_wasm.js'
import { CLEAR_COLOR, GRID_COLOR } from './theme'
import type { ArtboardSummary, OpaqueId } from './ipc/document'
import { writeBinaryFile } from './ipc/nativeFile'

function idsEqual(a: OpaqueId | null, b: OpaqueId | null): boolean {
  return a === b || (!!a && !!b && a.idx === b.idx && a.version === b.version)
}

/** Frames per second sampled for the export — independent of the source
 * file's own `fr` (already baked into each keyframe's time in seconds by
 * the importer) and of the editor's own display refresh rate. 24fps is
 * plenty to judge motion/timing correctness without a huge frame count. */
const EXPORT_FPS = 24

export class GifExportError extends Error {}

/**
 * Renders `artboard`'s active animation to a GIF and writes it to
 * `outPath`. Samples at exactly `t = i / EXPORT_FPS` for each frame via
 * `render()`'s own `playback_scrub_time` parameter — the same mechanism
 * `TimelinePanel`'s scrub/playback already uses — so this never touches the
 * document's own authoring scrub-time state or issues an undoable command;
 * exporting is a pure read.
 *
 * Fits the whole artboard into the canvas's *current* backing-buffer pixel
 * size (whatever it happens to be on screen right now — not a fixed export
 * resolution), centered, ignoring the live camera's own pan/zoom entirely:
 * a reference-quality export should show the whole piece the same way
 * every time, not whatever crop the user happened to be looking at.
 *
 * One color palette, computed from the first frame and reused for every
 * subsequent frame — right for this engine's flat, small color palettes
 * (a character rig is a handful of solid fills), and avoids the palette
 * visibly shifting frame to frame that re-quantizing per frame would risk.
 */
export async function exportActiveAnimationAsGif(editor: WasmEditor, canvas: HTMLCanvasElement, artboard: ArtboardSummary, outPath: string): Promise<void> {
  const animation = artboard.activeAnimation && artboard.animations.find((a) => idsEqual(a.id, artboard.activeAnimation))
  if (!animation) {
    throw new GifExportError('No active animation to export — switch to Animate mode and pick (or create) one first.')
  }
  if (canvas.width === 0 || canvas.height === 0) {
    throw new GifExportError('Canvas has no pixels yet — try again once the editor has finished laying out.')
  }

  const zoom = Math.min(canvas.width / artboard.width, canvas.height / artboard.height)
  const frameDelayMs = 1000 / EXPORT_FPS
  // At least 2 frames even for a near-zero-length animation, so the GIF
  // isn't a single static image with no encoded delay/loop behavior.
  const frameCount = Math.max(2, Math.round(animation.duration * EXPORT_FPS))

  const readback = document.createElement('canvas')
  readback.width = canvas.width
  readback.height = canvas.height
  const ctx2d = readback.getContext('2d', { willReadFrequently: true })
  if (!ctx2d) throw new GifExportError('Could not create a 2D context to read the canvas back.')

  const gif = GIFEncoder()
  let palette: number[][] | null = null

  for (let i = 0; i < frameCount; i++) {
    const t = (i / EXPORT_FPS) % Math.max(animation.duration, 1e-6)
    // Always the dark clear color, regardless of the live editor's own
    // theme toggle — an exported GIF is the artwork, not a screenshot of
    // this session's chrome preference, so it shouldn't silently change
    // appearance depending on whichever theme happened to be active when
    // "Export GIF" was clicked.
    const [clearR, clearG, clearB] = CLEAR_COLOR.dark
    const [gridR, gridG, gridB] = GRID_COLOR.dark
    editor.render(0, 0, 0, zoom, t, window.devicePixelRatio || 1, clearR, clearG, clearB, gridR, gridG, gridB)
    // One real paint before reading pixels back — `render()` submits the
    // draw, but the browser only actually presents the WebGPU canvas's new
    // content on the next compositor frame; reading synchronously right
    // after `render()` returns risks grabbing the *previous* frame's pixels.
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))

    ctx2d.clearRect(0, 0, readback.width, readback.height)
    ctx2d.drawImage(canvas, 0, 0)
    const { data } = ctx2d.getImageData(0, 0, readback.width, readback.height)

    if (!palette) palette = quantize(data, 256)
    const indexed = applyPalette(data, palette)
    gif.writeFrame(indexed, readback.width, readback.height, { palette, delay: frameDelayMs })
  }

  gif.finish()
  await writeBinaryFile(outPath, gif.bytes())
}
