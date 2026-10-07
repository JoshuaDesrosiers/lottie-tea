import { worldToScreen, type Camera, type CanvasSize } from '../camera'

interface SnapGuidesProps {
  /** World-space x/y an alignment guide is currently active at, or `null` if
   * that axis isn't snapped right now — see `snapping.ts`'s `SnapResult`. */
  guideX: number | null
  guideY: number | null
  camera: Camera
  canvasSize: CanvasSize
}

/**
 * The dashed alignment-guide lines Canva/Figma draw across the whole canvas
 * while a dragged shape's edge/center lines up with the artboard or another
 * shape — purely a rendering of whatever `snapping.ts`'s `snapTranslation`
 * already decided, same "plain overlay div, not part of the wgpu canvas"
 * reasoning as `TransformHandles`.
 */
export function SnapGuides({ guideX, guideY, camera, canvasSize }: SnapGuidesProps) {
  if (guideX === null && guideY === null) return null

  const screenX = guideX !== null ? worldToScreen(guideX, 0, camera, canvasSize).x : null
  const screenY = guideY !== null ? worldToScreen(0, guideY, camera, canvasSize).y : null

  return (
    <svg className="pointer-events-none absolute inset-0 h-full w-full overflow-visible">
      {screenX !== null && (
        <line x1={screenX} y1={0} x2={screenX} y2={canvasSize.height} stroke="rgba(255,90,120,0.9)" strokeWidth={1} strokeDasharray="4 3" />
      )}
      {screenY !== null && (
        <line x1={0} y1={screenY} x2={canvasSize.width} y2={screenY} stroke="rgba(255,90,120,0.9)" strokeWidth={1} strokeDasharray="4 3" />
      )}
    </svg>
  )
}
