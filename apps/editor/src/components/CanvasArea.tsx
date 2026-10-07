import type { MouseEventHandler, RefObject } from 'react'

interface CanvasAreaProps {
  canvasRef: RefObject<HTMLCanvasElement | null>
  onMouseDown?: MouseEventHandler<HTMLCanvasElement>
  /** Drills one level into whatever group is under the cursor (or backs out,
   * if double-clicking outside the currently-entered group) — the
   * Figma-style "click selects the whole top-level group, double-click digs
   * into it" scoped-selection model `App.tsx`'s `handleCanvasDoubleClick`
   * implements. */
  onDoubleClick?: MouseEventHandler<HTMLCanvasElement>
}

/**
 * The artboard canvas — always the largest thing on screen (AN1MBYT3_BUILD_PLAN.md
 * §15: "canvas-dominant center stage; the artboard is the largest thing on
 * screen at all times, never squeezed by permanent chrome"), and now a real
 * infinite-pan/zoom viewport rather than a canvas sized 1:1 to the artboard:
 * this element fills its entire container (`w-full h-full`), and `App.tsx`
 * sets its actual pixel-buffer `width`/`height` imperatively via a
 * `ResizeObserver` (a `<canvas>`'s CSS size and its backing pixel-buffer
 * size are two different things — React's `width`/`height` props would only
 * ever set the latter once, at mount, which is why this component doesn't
 * take them as props at all anymore). What part of the (now effectively
 * unbounded) world space is visible is `an1mbyt3_renderer::Camera`'s job
 * (pan + zoom), driven by `App.tsx`'s own `camera` state and passed into
 * `WasmEditor::render` every frame.
 *
 * Purely presentational otherwise — `App` owns the `WasmEditor` instance and
 * its render loop (`ipc/wasmEditor.ts`), since the editor lives for the whole
 * app's session, not just while this component happens to be mounted.
 *
 * `onMouseDown` is forwarded straight to the `<canvas>` element for
 * on-canvas direct manipulation (drag-to-move a node, or drag-to-pan the
 * camera on an empty-canvas hit) — `App` owns the actual hit-test/drag
 * logic, this component just wires the DOM event through, same
 * "presentational, no owned logic" split as everything else here.
 * `mousemove`/`mouseup` are deliberately NOT wired here — `App` attaches
 * those at the `window` level instead, so a drag keeps tracking the cursor
 * even if it leaves the canvas mid-drag. Scroll-pan/pinch-zoom similarly
 * isn't wired here at all — `App.tsx` attaches a real (non-passive) native
 * `wheel` listener directly to `canvasRef.current` itself (same
 * passive-listener gotcha `TimelinePanel`'s zoom handling already hit: a
 * React `onWheel` prop can't reliably `preventDefault()` a pinch/Ctrl+scroll).
 */
export function CanvasArea({ canvasRef, onMouseDown, onDoubleClick }: CanvasAreaProps) {
  return (
    <div className="absolute inset-0 select-none overflow-hidden bg-canvas">
      <canvas ref={canvasRef} className="h-full w-full touch-none select-none" onMouseDown={onMouseDown} onDoubleClick={onDoubleClick} />
    </div>
  )
}
