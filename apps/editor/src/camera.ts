/**
 * Canvas-pixel <-> world/artboard-space conversion — the one place this
 * math is defined. `App.tsx`'s pointer handlers (drag-to-translate,
 * drag-to-pan, wheel-zoom) and `TransformHandles`' handle-drag math
 * (rotate/scale) both call these instead of each keeping their own copy, so
 * the two can never quietly drift apart — exactly the kind of pointer-math
 * bug that's easy to introduce by hand-deriving the same formula twice.
 *
 * Convention: canvas-pixel space has its origin top-left, +y down (what
 * `MouseEvent`/`getBoundingClientRect` give); world/artboard space has its
 * origin at the camera's pan target, +y up (the convention every other
 * pointer-facing call in this workspace uses) and is additionally scaled by
 * the camera's `zoom`.
 */

export interface Camera {
  pan: { x: number; y: number }
  zoom: number
}

export interface CanvasSize {
  width: number
  height: number
}

export function screenToWorld(canvasX: number, canvasY: number, camera: Camera, size: CanvasSize): { x: number; y: number } {
  return {
    x: camera.pan.x + (canvasX - size.width / 2) / camera.zoom,
    y: camera.pan.y - (canvasY - size.height / 2) / camera.zoom,
  }
}

/** The inverse of {@link screenToWorld}. */
export function worldToScreen(worldX: number, worldY: number, camera: Camera, size: CanvasSize): { x: number; y: number } {
  return {
    x: (worldX - camera.pan.x) * camera.zoom + size.width / 2,
    y: size.height / 2 - (worldY - camera.pan.y) * camera.zoom,
  }
}
