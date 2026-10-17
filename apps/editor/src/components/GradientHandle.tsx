import { HandleHit } from './HandleHit'
import { useState } from 'react'
import type { RefObject } from 'react'
import { screenToWorld, worldToScreen, type Camera, type CanvasSize } from '../camera'
import type { GradientDef, NodeSummary } from '../ipc/document'

const TAU = Math.PI * 2

/** Where `point` (local space) sits along the gradient's own `0..1` axis —
 * the inverse of `pointAtT` below. Linear/Radial project onto the
 * `start -> end` line; Conic measures the angle around `start`, referenced
 * against the `start -> end` direction — the exact same parameterization
 * `shader.wgsl`'s `fs_main` uses, kept in lock-step with it so a stop
 * dragged to a given screen position lands at the offset that actually
 * renders there. */
function projectToT(kind: GradientDef['kind'], start: { x: number; y: number }, end: { x: number; y: number }, point: { x: number; y: number }): number {
  const toPoint = { x: point.x - start.x, y: point.y - start.y }
  if (kind === 'Conic') {
    const reference = Math.atan2(end.y - start.y, end.x - start.x)
    const angle = Math.atan2(toPoint.y, toPoint.x)
    const delta = angle - reference
    return ((delta % TAU) + TAU) % TAU / TAU
  }
  if (kind === 'Bilinear') {
    // Chebyshev (max-norm) distance from `start` along the `start -> end`
    // axis and its perpendicular — matches `shader.wgsl`'s own Bilinear
    // branch exactly, so a stop dragged to a screen position lands at the
    // offset that actually renders there.
    const radius = Math.hypot(end.x - start.x, end.y - start.y)
    if (radius < 0.0001) return 0
    const axisX = { x: (end.x - start.x) / radius, y: (end.y - start.y) / radius }
    const axisY = { x: -axisX.y, y: axisX.x }
    const projX = Math.abs(toPoint.x * axisX.x + toPoint.y * axisX.y)
    const projY = Math.abs(toPoint.x * axisY.x + toPoint.y * axisY.y)
    return Math.min(1, Math.max(0, Math.max(projX, projY) / radius))
  }
  const axis = { x: end.x - start.x, y: end.y - start.y }
  const lenSq = axis.x * axis.x + axis.y * axis.y
  if (lenSq < 0.0001) return 0
  const t = (toPoint.x * axis.x + toPoint.y * axis.y) / lenSq
  return Math.min(1, Math.max(0, t))
}

/** The local-space point at parameter `t` along the gradient's axis — used
 * to position each stop marker. Conic has no natural "distance," so its
 * markers sit on a circle whose radius matches the reference line's own
 * length (`|end - start|`) — a fixed, Affinity-like visual convention, not
 * something the shader itself needs (the shader only cares about angle). */
function pointAtT(kind: GradientDef['kind'], start: { x: number; y: number }, end: { x: number; y: number }, t: number): { x: number; y: number } {
  if (kind === 'Conic') {
    const reference = Math.atan2(end.y - start.y, end.x - start.x)
    const radius = Math.hypot(end.x - start.x, end.y - start.y)
    const angle = reference + t * TAU
    return { x: start.x + Math.cos(angle) * radius, y: start.y + Math.sin(angle) * radius }
  }
  return { x: start.x + (end.x - start.x) * t, y: start.y + (end.y - start.y) * t }
}

// Each variant its own literal `kind` (not `{ kind: 'start' | 'end' }`
// grouped together) — TS's discriminated-union narrowing through
// sequential if/else-if/else needs one literal per variant to narrow the
// final `else` branch down to `{ kind: 'stop' }` and see its `index` field.
type Dragging = { kind: 'start' } | { kind: 'end' } | { kind: 'stop'; index: number } | { kind: 'midpoint'; index: number }

/**
 * The Fill tool's on-canvas gradient handle (Affinity's reference) — a
 * draggable start/end axis plus draggable stop markers along it, directly
 * on the selected `Shape`/`Text` node. Purely a *geometry* editor (where
 * the axis sits, where each stop's offset lands); stop *colors* stay the
 * Inspector's `GradientStopList`'s job — see `InspectorPanel.tsx`'s
 * `FillField` doc comment for why the two are split this way.
 *
 * Dragging updates a local `liveDrag` override for instant handle
 * feedback, but only calls `onSetGradient` once, on release — unlike
 * `TransformHandles`' rotate/scale drags (which *do* push a live preview
 * into the document every frame), `Command::SetGradient` has no
 * "coalesce this whole drag into one undo step" support the way
 * translation drags get from the renderer's own live-preview path, so
 * committing every pointermove would flood undo with one entry per pixel.
 * The shape's actual on-canvas gradient fill only updates at release —
 * an accepted first-pass tradeoff, not a bug.
 */
export function GradientHandle({
  containerRef,
  selectedNode,
  camera,
  canvasSize,
  onSetGradient,
}: {
  containerRef: RefObject<HTMLDivElement | null>
  selectedNode: NodeSummary | null
  camera: Camera
  canvasSize: CanvasSize
  onSetGradient: (value: GradientDef) => void
}) {
  const [dragging, setDragging] = useState<Dragging | null>(null)
  const [liveGradient, setLiveGradient] = useState<GradientDef | null>(null)

  if (!selectedNode?.gradient) return null
  const gradient = liveGradient ?? selectedNode.gradient
  // Ascending-offset order — a live stop-drag can transiently leave
  // `gradient.stops` unsorted until release re-sorts it, but midpoint
  // markers need each segment's *actual* neighbor pair regardless.
  const sortedStops = [...gradient.stops].sort((a, b) => a.offset - b.offset)
  const { translation, rotation, scale } = selectedNode.worldTransform
  const cosR = Math.cos(rotation)
  const sinR = Math.sin(rotation)

  const localToWorld = (lx: number, ly: number) => {
    const scaledX = lx * scale.x
    const scaledY = ly * scale.y
    return { x: translation.x + (scaledX * cosR - scaledY * sinR), y: translation.y + (scaledX * sinR + scaledY * cosR) }
  }
  const worldToLocal = (wx: number, wy: number) => {
    const dx = wx - translation.x
    const dy = wy - translation.y
    const scaledX = dx * cosR + dy * sinR
    const scaledY = -dx * sinR + dy * cosR
    return { x: scaledX / scale.x, y: scaledY / scale.y }
  }
  const localToScreen = (lx: number, ly: number) => {
    const world = localToWorld(lx, ly)
    return worldToScreen(world.x, world.y, camera, canvasSize)
  }
  const screenToLocal = (sx: number, sy: number) => {
    const world = screenToWorld(sx, sy, camera, canvasSize)
    return worldToLocal(world.x, world.y)
  }
  const clientToLocal = (clientX: number, clientY: number) => {
    const rect = containerRef.current?.getBoundingClientRect()
    return screenToLocal(clientX - (rect?.left ?? 0), clientY - (rect?.top ?? 0))
  }

  const startScreen = localToScreen(gradient.start.x, gradient.start.y)
  const endScreen = localToScreen(gradient.end.x, gradient.end.y)

  const updateLive = (next: GradientDef) => setLiveGradient(next)

  const handlePointerMove = (e: React.PointerEvent) => {
    if (!dragging) return
    const local = clientToLocal(e.clientX, e.clientY)
    if (dragging.kind === 'start') {
      updateLive({ ...gradient, start: local })
    } else if (dragging.kind === 'end') {
      updateLive({ ...gradient, end: local })
    } else if (dragging.kind === 'stop') {
      const t = projectToT(gradient.kind, gradient.start, gradient.end, local)
      const stops = gradient.stops.map((s, i) => (i === dragging.index ? { ...s, offset: t } : s))
      updateLive({ ...gradient, stops })
    } else {
      // Dragging the diamond between stop[index] and stop[index+1] — the
      // drag position's own segment-local `t` (via `projectToT`, mapped
      // into that segment's `[offset_i, offset_i+1]` range) directly *is*
      // the new midpoint value: "put the 50%-blend point here" is exactly
      // what a midpoint drag means, no bias-formula inversion needed (the
      // shader's `bias_segment` already treats `midpoint` as "the
      // segment-local position the 50% point lands at").
      const sorted = [...gradient.stops].sort((a, b) => a.offset - b.offset)
      const current = sorted[dragging.index]
      const next = sorted[dragging.index + 1]
      if (!current || !next) return
      const t = projectToT(gradient.kind, gradient.start, gradient.end, local)
      const span = Math.max(next.offset - current.offset, 0.0001)
      const midpoint = Math.min(0.95, Math.max(0.05, (t - current.offset) / span))
      const stops = gradient.stops.map((s) => (s === current ? { ...s, midpoint } : s))
      updateLive({ ...gradient, stops })
    }
  }

  const handlePointerUp = () => {
    if (liveGradient) {
      const stops = dragging?.kind === 'stop' ? [...liveGradient.stops].sort((a, b) => a.offset - b.offset) : liveGradient.stops
      onSetGradient({ ...liveGradient, stops })
    }
    setDragging(null)
    setLiveGradient(null)
  }

  // A cancelled pointer (chiefly `input/touchGestures.ts` taking the finger
  // over because a second one landed for a pinch): abandon the live edit —
  // nothing was sent to `onSetGradient` yet, so dropping it is a full revert.
  const handlePointerCancel = () => {
    setDragging(null)
    setLiveGradient(null)
  }

  const startDrag = (e: React.PointerEvent, next: Dragging) => {
    e.stopPropagation()
    ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
    setDragging(next)
  }

  const handleAxisDoubleClick = (e: React.MouseEvent) => {
    e.stopPropagation()
    const local = clientToLocal(e.clientX, e.clientY)
    const t = projectToT(gradient.kind, gradient.start, gradient.end, local)
    const sorted = [...gradient.stops].sort((a, b) => a.offset - b.offset)
    const before = [...sorted].reverse().find((s) => s.offset <= t) ?? sorted[0]
    const after = sorted.find((s) => s.offset >= t) ?? sorted[sorted.length - 1]
    const span = Math.max(after.offset - before.offset, 0.0001)
    const lt = (t - before.offset) / span
    const color: [number, number, number, number] = [0, 1, 2, 3].map((i) => before.color[i] + (after.color[i] - before.color[i]) * lt) as [
      number,
      number,
      number,
      number,
    ]
    onSetGradient({ ...gradient, stops: [...gradient.stops, { offset: t, color, midpoint: 0.5 }].sort((a, b) => a.offset - b.offset) })
  }

  return (
    <div className="pointer-events-none absolute inset-0" onPointerMove={handlePointerMove} onPointerUp={handlePointerUp} onPointerCancel={handlePointerCancel}>
      <svg className="absolute inset-0 h-full w-full overflow-visible">
        {gradient.kind === 'Radial' && (
          <circle
            cx={startScreen.x}
            cy={startScreen.y}
            r={Math.hypot(endScreen.x - startScreen.x, endScreen.y - startScreen.y)}
            fill="none"
            stroke="rgba(255,255,255,0.5)"
            strokeDasharray="4 3"
          />
        )}
        {gradient.kind === 'Conic' && (
          <circle
            cx={startScreen.x}
            cy={startScreen.y}
            r={Math.hypot(endScreen.x - startScreen.x, endScreen.y - startScreen.y)}
            fill="none"
            stroke="rgba(255,255,255,0.5)"
            strokeDasharray="4 3"
          />
        )}
        {gradient.kind === 'Bilinear' &&
          (() => {
            // A square reference outline (not a circle) — matches the
            // max-norm isocontours this kind actually renders, rotated to
            // the same `start -> end` axis angle every other handle here
            // uses, so the square's own edges visually line up with where
            // the "L-shaped" bands actually fall.
            const radius = Math.hypot(endScreen.x - startScreen.x, endScreen.y - startScreen.y)
            const angleDeg = (Math.atan2(endScreen.y - startScreen.y, endScreen.x - startScreen.x) * 180) / Math.PI
            return (
              <rect
                x={startScreen.x - radius}
                y={startScreen.y - radius}
                width={radius * 2}
                height={radius * 2}
                fill="none"
                stroke="rgba(255,255,255,0.5)"
                strokeDasharray="4 3"
                transform={`rotate(${angleDeg} ${startScreen.x} ${startScreen.y})`}
              />
            )
          })()}
        <line
          x1={startScreen.x}
          y1={startScreen.y}
          x2={endScreen.x}
          y2={endScreen.y}
          stroke="rgba(255,255,255,0.85)"
          strokeWidth={1.5}
          className="pointer-events-auto cursor-copy"
          onDoubleClick={handleAxisDoubleClick}
        />
        {gradient.stops.map((stop, index) => {
          const stopLocal = pointAtT(gradient.kind, gradient.start, gradient.end, stop.offset)
          const p = localToScreen(stopLocal.x, stopLocal.y)
          return (
            <HandleHit key={index}>
            <circle
              cx={p.x}
              cy={p.y}
              r={6}
              fill={`rgba(${Math.round(stop.color[0] * 255)}, ${Math.round(stop.color[1] * 255)}, ${Math.round(stop.color[2] * 255)}, ${stop.color[3]})`}
              stroke="white"
              strokeWidth={1.5}
              className="pointer-events-auto cursor-grab"
              onPointerDown={(e) => startDrag(e, { kind: 'stop', index })}
            />
            </HandleHit>
          )
        })}
        {sortedStops.slice(0, -1).map((stop, index) => {
          // The draggable "midpoint" diamond for the segment between this
          // stop and the next one (by offset) — Affinity's per-segment
          // strength control. Positioned at the *global* t its own
          // segment-local `midpoint` fraction maps to, so it always sits
          // exactly where the 50%-blend point actually renders.
          const next = sortedStops[index + 1]
          const globalT = stop.offset + stop.midpoint * (next.offset - stop.offset)
          const midpointLocal = pointAtT(gradient.kind, gradient.start, gradient.end, globalT)
          const p = localToScreen(midpointLocal.x, midpointLocal.y)
          return (
            <HandleHit key={index}>
            <rect
              x={p.x - 4}
              y={p.y - 4}
              width={8}
              height={8}
              transform={`rotate(45 ${p.x} ${p.y})`}
              fill="rgba(20,20,24,0.9)"
              stroke="white"
              strokeWidth={1.25}
              className="pointer-events-auto cursor-grab"
              onPointerDown={(e) => startDrag(e, { kind: 'midpoint', index })}
            />
            </HandleHit>
          )
        })}
      </svg>
      <div
        onPointerDown={(e) => startDrag(e, { kind: 'start' })}
        title="Gradient start"
        className="an1-hit pointer-events-auto absolute h-3 w-3 -translate-x-1/2 -translate-y-1/2 cursor-move rounded-sm border-2 border-white bg-accent"
        style={{ left: startScreen.x, top: startScreen.y }}
      />
      <div
        onPointerDown={(e) => startDrag(e, { kind: 'end' })}
        title={gradient.kind === 'Linear' ? 'Gradient end' : gradient.kind === 'Radial' ? 'Gradient radius' : 'Gradient angle reference'}
        className="an1-hit pointer-events-auto absolute h-3 w-3 -translate-x-1/2 -translate-y-1/2 cursor-move rounded-full border-2 border-white bg-accent"
        style={{ left: endScreen.x, top: endScreen.y }}
      />
    </div>
  )
}
