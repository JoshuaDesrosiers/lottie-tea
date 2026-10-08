/**
 * Paint session ↔ Drawing resource (`an1mbyt3_core::DrawingContent`).
 *
 * Saving turns each cel into the engine's form: vector cels keep their SVG,
 * bitmap cels edited this session are uploaded as image assets first
 * (`addDrawingCelImage`), and already-saved ones keep their image id.
 * Opening goes the other way, reading bitmap cels' PNGs back as data URLs
 * for scratch-paint.
 */
import type { OpaqueId } from '../ipc/document'
import { addDrawingCelImage, drawingContent, getImageSource, type WasmEditor } from '../ipc/wasmEditor'
import { imageDataToPng } from '../components/PaintAssetEditor'
import { isEmptyCel, type Center, type PaintCel, type PaintSession } from './session'

type CelContentJson = { Vector: { svg: string } } | { Bitmap: { image: OpaqueId } }
type CelJson = { content: CelContentJson; center: [number, number] }
/** `an1mbyt3_core::DrawingAnimation`, as JSON. */
export interface DrawingAnimationJson {
  fps: number
  frame_count: number
  layers: { name: string; visible: boolean; opacity: number; cels: (CelJson | null)[] }[]
}
type DrawingContentJson = { Vector: { svg: string } } | { Bitmap: { image: OpaqueId } } | { Animation: DrawingAnimationJson }

function pngDataUrl(bytes: Uint8Array): string {
  let binary = ''
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000))
  return `data:image/png;base64,${btoa(binary)}`
}

/** The SVG's own middle, in its user units — the center for a cel saved
 * without one (scratch-paint then just centers it). */
function svgMiddle(svg: string): [number, number] {
  const box = svg.match(/viewBox="([^"]+)"/)?.[1].split(/[\s,]+/).map(Number)
  return box && box.length === 4 ? [box[2] / 2, box[3] / 2] : [0, 0]
}

/** `session` as the engine's frames × layers JSON, uploading new bitmap
 * cels on the way. Also returns the session with those cels swapped for
 * their saved image (so the next save doesn't upload them again). */
export async function sessionToAnimation(editor: WasmEditor, session: PaintSession, name: string): Promise<{ json: DrawingAnimationJson; saved: PaintSession }> {
  const layers: DrawingAnimationJson['layers'] = []
  const savedLayers: PaintSession['layers'] = []
  for (const layer of session.layers) {
    const cels: (CelJson | null)[] = []
    const savedCels: (PaintCel | null)[] = []
    for (const cel of layer.cels) {
      if (!cel || isEmptyCel(cel)) {
        cels.push(null)
        savedCels.push(null)
        continue
      }
      if (cel.kind === 'svg') {
        cels.push({ content: { Vector: { svg: cel.svg } }, center: cel.center ?? svgMiddle(cel.svg) })
        savedCels.push(cel)
      } else if (cel.kind === 'image') {
        cels.push({ content: { Bitmap: { image: cel.image } }, center: cel.center ?? [0, 0] })
        savedCels.push(cel)
      } else {
        const png = await imageDataToPng(cel.bitmap)
        const image = await addDrawingCelImage(editor, name, png)
        const center: [number, number] = cel.center ?? [cel.bitmap.width / 2, cel.bitmap.height / 2]
        cels.push({ content: { Bitmap: { image } }, center })
        savedCels.push({ kind: 'image', image, dataUrl: pngDataUrl(png), center })
      }
    }
    layers.push({ name: layer.name, visible: layer.visible, opacity: layer.opacity, cels })
    savedLayers.push({ ...layer, cels: savedCels })
  }
  return { json: { fps: session.fps, frame_count: session.frameCount, layers }, saved: { ...session, layers: savedLayers } }
}

/** A saved drawing as a Paint session (any of its three shapes). */
export function sessionFromDrawing(editor: WasmEditor, drawing: OpaqueId): PaintSession {
  const content = drawingContent(editor, drawing) as DrawingContentJson
  const imageCel = (image: OpaqueId, center: Center): PaintCel => ({ kind: 'image', image, dataUrl: pngDataUrl(getImageSource(editor, image)), center })
  const base = { currentFrame: 0, currentLayer: 0, onion: true }
  if ('Vector' in content) return { ...base, fps: 12, frameCount: 1, layers: [{ name: 'Layer 1', visible: true, opacity: 1, cels: [{ kind: 'svg', svg: content.Vector.svg, center: null }] }] }
  if ('Bitmap' in content) return { ...base, fps: 12, frameCount: 1, layers: [{ name: 'Layer 1', visible: true, opacity: 1, cels: [imageCel(content.Bitmap.image, null)] }] }
  const animation = content.Animation
  return {
    ...base,
    fps: animation.fps,
    frameCount: animation.frame_count,
    layers: animation.layers.map((layer) => ({
      name: layer.name,
      visible: layer.visible,
      opacity: layer.opacity,
      cels: layer.cels.map((cel) => (!cel ? null : 'Vector' in cel.content ? { kind: 'svg', svg: cel.content.Vector.svg, center: cel.center } : imageCel(cel.content.Bitmap.image, cel.center))),
    })),
  }
}
