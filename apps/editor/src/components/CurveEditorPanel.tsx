import { useEffect, useRef, useState } from 'react'
import type { Easing } from '../ipc/document'
import { FloatingPanel } from './FloatingPanel'

const SIZE = 200
const PADDING = 24
// Curve space is x:[0,1] (time), y:[-0.5,1.5] (progress, allowing the
// classic "anticipate"/"overshoot" handles below 0 or above 1 the same way
// After Effects' graph editor does) — wider than the [0,1]x[0,1] a plain
// CSS `cubic-bezier()` picker shows, since `Easing::CubicBezier`'s own
// evaluator (`cubic_bezier_ease`) doesn't restrict y1/y2 to that range
// either.
const Y_MIN = -0.5
const Y_MAX = 1.5

function curveToPixel(x: number, y: number): { x: number; y: number } {
  const px = PADDING + x * (SIZE - PADDING * 2)
  const py = PADDING + (1 - (y - Y_MIN) / (Y_MAX - Y_MIN)) * (SIZE - PADDING * 2)
  return { x: px, y: py }
}

function pixelToCurve(px: number, py: number): { x: number; y: number } {
  const x = (px - PADDING) / (SIZE - PADDING * 2)
  const y = Y_MIN + (1 - (py - PADDING) / (SIZE - PADDING * 2)) * (Y_MAX - Y_MIN)
  return { x, y }
}

/**
 * A standard two-handle cubic-bezier curve editor — the classic CSS
 * `cubic-bezier()`/After-Effects-"Easy Ease" picker shape: fixed endpoints
 * at `(0,0)`/`(1,1)`, two free, independently-draggable handles
 * (`(x1,y1)`/`(x2,y2)`). Opened by double-clicking a keyframe diamond in
 * `TimelinePanel` — see that component's doc comment for why it's *this*
 * keyframe's easing being edited even though it visually represents "the
 * curve leading into" the one double-clicked (`Easing` governs the segment
 * *out of* the keyframe it's stored on).
 *
 * A small floating popover (not a full modal) positioned near the
 * keyframe that opened it, matching `GradientHandle`'s own "contextual
 * popover near the thing being edited" precedent rather than a
 * centered dialog.
 */
export function CurveEditorPanel({
  easing,
  anchor,
  onChange,
  onClose,
}: {
  easing: Easing
  anchor: { x: number; y: number }
  onChange: (easing: Easing) => void
  onClose: () => void
}) {
  const bezier = easing.kind === 'CubicBezier' ? easing : { kind: 'CubicBezier' as const, x1: 0.42, y1: 0.0, x2: 0.58, y2: 1.0 }
  const [x1, setX1] = useState(bezier.x1)
  const [y1, setY1] = useState(bezier.y1)
  const [x2, setX2] = useState(bezier.x2)
  const [y2, setY2] = useState(bezier.y2)
  const rootRef = useRef<HTMLDivElement>(null)
  const svgRef = useRef<SVGSVGElement>(null)
  const dragRef = useRef<'p1' | 'p2' | null>(null)

  useEffect(() => {
    if (easing.kind === 'CubicBezier') {
      setX1(easing.x1)
      setY1(easing.y1)
      setX2(easing.x2)
      setY2(easing.y2)
    }
  }, [easing])

  useEffect(() => {
    function handleOutsideClick(event: MouseEvent) {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) onClose()
    }
    document.addEventListener('mousedown', handleOutsideClick)
    return () => document.removeEventListener('mousedown', handleOutsideClick)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    function handleMouseMove(event: MouseEvent) {
      const dragging = dragRef.current
      if (!dragging || !svgRef.current) return
      const rect = svgRef.current.getBoundingClientRect()
      const curve = pixelToCurve(event.clientX - rect.left, event.clientY - rect.top)
      const clampedX = Math.max(0, Math.min(1, curve.x))
      const clampedY = Math.max(Y_MIN, Math.min(Y_MAX, curve.y))
      if (dragging === 'p1') {
        setX1(clampedX)
        setY1(clampedY)
        onChange({ kind: 'CubicBezier', x1: clampedX, y1: clampedY, x2, y2 })
      } else {
        setX2(clampedX)
        setY2(clampedY)
        onChange({ kind: 'CubicBezier', x1, y1, x2: clampedX, y2: clampedY })
      }
    }
    function handleMouseUp() {
      dragRef.current = null
    }
    window.addEventListener('mousemove', handleMouseMove)
    window.addEventListener('mouseup', handleMouseUp)
    return () => {
      window.removeEventListener('mousemove', handleMouseMove)
      window.removeEventListener('mouseup', handleMouseUp)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [x1, y1, x2, y2])

  const commitNumeric = (next: { x1: number; y1: number; x2: number; y2: number }) => {
    setX1(next.x1)
    setY1(next.y1)
    setX2(next.x2)
    setY2(next.y2)
    onChange({ kind: 'CubicBezier', ...next })
  }

  const origin = curveToPixel(0, 0)
  const end = curveToPixel(1, 1)
  const p1 = curveToPixel(x1, y1)
  const p2 = curveToPixel(x2, y2)
  const curvePath = `M ${origin.x} ${origin.y} C ${p1.x} ${p1.y}, ${p2.x} ${p2.y}, ${end.x} ${end.y}`

  return (
    <div ref={rootRef} className="absolute z-30" style={{ left: anchor.x, top: anchor.y }}>
      <FloatingPanel className="flex flex-col gap-2 p-3">
        <div className="flex items-center justify-between gap-4">
          <span className="text-xs font-medium uppercase tracking-wide text-ink-dim">Curve</span>
          <div className="flex items-center gap-1.5">
            <button
              onClick={() => onChange({ kind: 'Linear' })}
              className={`rounded-md px-2 py-0.5 text-[11px] ${easing.kind === 'Linear' ? 'bg-accent/15 text-accent' : 'text-ink-dim hover:bg-overlay/5 hover:text-ink'}`}
            >
              Linear
            </button>
            <button onClick={onClose} className="rounded-md px-1.5 py-0.5 text-[11px] text-ink-dim hover:bg-overlay/5 hover:text-ink">
              ✕
            </button>
          </div>
        </div>

        <svg ref={svgRef} width={SIZE} height={SIZE} className="rounded-md bg-overlay/5">
          {/* 0/1 reference lines */}
          <line x1={PADDING} y1={curveToPixel(0, 0).y} x2={SIZE - PADDING} y2={curveToPixel(0, 0).y} stroke="rgba(255,255,255,0.08)" />
          <line x1={PADDING} y1={curveToPixel(0, 1).y} x2={SIZE - PADDING} y2={curveToPixel(0, 1).y} stroke="rgba(255,255,255,0.08)" />
          <line x1={origin.x} y1={origin.y} x2={end.x} y2={end.y} stroke="rgba(255,255,255,0.12)" strokeDasharray="3 3" />

          {/* handle connector lines */}
          <line x1={origin.x} y1={origin.y} x2={p1.x} y2={p1.y} stroke="rgba(122,140,255,0.6)" />
          <line x1={end.x} y1={end.y} x2={p2.x} y2={p2.y} stroke="rgba(122,140,255,0.6)" />

          {/* the curve itself */}
          <path d={curvePath} fill="none" stroke="#7a8cff" strokeWidth={2} />

          {/* fixed endpoints */}
          <circle cx={origin.x} cy={origin.y} r={3} fill="rgba(255,255,255,0.5)" />
          <circle cx={end.x} cy={end.y} r={3} fill="rgba(255,255,255,0.5)" />

          {/* draggable handles */}
          <circle
            cx={p1.x}
            cy={p1.y}
            r={6}
            fill="#7a8cff"
            className="cursor-grab active:cursor-grabbing"
            onMouseDown={(e) => {
              e.preventDefault()
              dragRef.current = 'p1'
            }}
          />
          <circle
            cx={p2.x}
            cy={p2.y}
            r={6}
            fill="#7a8cff"
            className="cursor-grab active:cursor-grabbing"
            onMouseDown={(e) => {
              e.preventDefault()
              dragRef.current = 'p2'
            }}
          />
        </svg>

        <div className="grid grid-cols-4 gap-1.5">
          {(
            [
              ['x1', x1, (v: number) => commitNumeric({ x1: v, y1, x2, y2 })],
              ['y1', y1, (v: number) => commitNumeric({ x1, y1: v, x2, y2 })],
              ['x2', x2, (v: number) => commitNumeric({ x1, y1, x2: v, y2 })],
              ['y2', y2, (v: number) => commitNumeric({ x1, y1, x2, y2: v })],
            ] as const
          ).map(([label, value, set]) => (
            <label key={label} className="flex flex-col gap-0.5 text-[9px] text-ink-dim">
              {label}
              <input
                type="number"
                step={0.05}
                value={Number(value.toFixed(3))}
                onChange={(e) => {
                  const parsed = Number.parseFloat(e.target.value)
                  if (Number.isFinite(parsed)) set(parsed)
                }}
                className="w-full rounded-sm bg-overlay/5 px-1 py-0.5 text-[10px] text-ink outline-none"
              />
            </label>
          ))}
        </div>
      </FloatingPanel>
    </div>
  )
}
