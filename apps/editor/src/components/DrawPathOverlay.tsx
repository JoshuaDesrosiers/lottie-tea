/**
 * **Draw path** — sketch a path straight onto the canvas, for trim-path
 * draw-ons and motion paths.
 *
 * - **Freehand**: press and drag; the stroke is thinned (Ramer–Douglas–
 *   Peucker) and smoothed into cubic curves (Catmull-Rom → Bézier).
 * - **Points**: click to place points (drag isn't needed); Enter, a
 *   double-click or clicking the first point finishes; Backspace removes
 *   the last point. With Smooth on, the points become a flowing curve;
 *   off, straight segments.
 *
 * Ending near the start closes the path. Escape cancels. The overlay
 * draws in screen space and hands world-space points to `onFinish`.
 */
import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react'
import type { Path, Vec2 } from '../ipc/document'

export interface DrawPathOptions {
  mode: 'freehand' | 'points'
  smooth: boolean
}

/** Drops points that sit within `epsilon` of the line through their neighbours. */
function simplify(points: Vec2[], epsilon: number): Vec2[] {
  if (points.length < 3) return points
  const dist = (p: Vec2, a: Vec2, b: Vec2) => {
    const dx = b.x - a.x
    const dy = b.y - a.y
    const len = Math.hypot(dx, dy) || 1
    return Math.abs(dy * p.x - dx * p.y + b.x * a.y - b.y * a.x) / len
  }
  let max = 0
  let index = 0
  for (let i = 1; i < points.length - 1; i++) {
    const d = dist(points[i], points[0], points[points.length - 1])
    if (d > max) {
      max = d
      index = i
    }
  }
  if (max <= epsilon) return [points[0], points[points.length - 1]]
  const left = simplify(points.slice(0, index + 1), epsilon)
  const right = simplify(points.slice(index), epsilon)
  return [...left.slice(0, -1), ...right]
}

/** Points → a path: Catmull-Rom curves when `smooth`, else straight lines. */
export function pointsToPath(points: Vec2[], closed: boolean, smooth: boolean): Path {
  const commands: Path['commands'] = [{ kind: 'MoveTo', value: points[0] }]
  const n = points.length
  const at = (i: number) => (closed ? points[((i % n) + n) % n] : points[Math.max(0, Math.min(n - 1, i))])
  const segments = closed ? n : n - 1
  for (let i = 0; i < segments; i++) {
    const p0 = at(i - 1)
    const p1 = at(i)
    const p2 = at(i + 1)
    const p3 = at(i + 2)
    if (!smooth) commands.push({ kind: 'LineTo', value: p2 })
    else
      commands.push({
        kind: 'CubicTo',
        value: {
          control1: { x: p1.x + (p2.x - p0.x) / 6, y: p1.y + (p2.y - p0.y) / 6 },
          control2: { x: p2.x - (p3.x - p1.x) / 6, y: p2.y - (p3.y - p1.y) / 6 },
          to: p2,
        },
      })
  }
  if (closed) commands.push({ kind: 'Close' })
  return { commands }
}

export function DrawPathOverlay({
  options,
  toWorld,
  toScreen,
  zoom,
  onFinish,
  onCancel,
}: {
  options: DrawPathOptions
  /** Client coordinates → world. */
  toWorld: (clientX: number, clientY: number) => Vec2
  /** World → overlay (canvas-relative) pixels. */
  toScreen: (p: Vec2) => Vec2
  zoom: number
  onFinish: (points: Vec2[], closed: boolean) => void
  onCancel: () => void
}) {
  const [points, setPoints] = useState<Vec2[]>([])
  const [hoverPoint, setHoverPoint] = useState<Vec2 | null>(null)
  const drawing = useRef(false)
  const pointsRef = useRef(points)
  pointsRef.current = points

  const closeEnough = (a: Vec2, b: Vec2) => Math.hypot(a.x - b.x, a.y - b.y) * zoom < 12

  const finish = (raw: Vec2[]) => {
    if (raw.length < 2) {
      setPoints([])
      return
    }
    let pts = raw
    let closed = false
    if (pts.length > 2 && closeEnough(pts[0], pts[pts.length - 1])) {
      closed = true
      pts = pts.slice(0, -1)
    }
    if (options.mode === 'freehand') pts = simplify(pts, 1.5 / Math.max(0.05, zoom))
    if (pts.length >= 2) onFinish(pts, closed)
    setPoints([])
  }

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement | null)?.closest('input, textarea, select')) return
      if (e.key === 'Escape') {
        if (pointsRef.current.length) setPoints([])
        else onCancel()
      } else if (e.key === 'Enter') finish(pointsRef.current)
      else if (e.key === 'Backspace' && options.mode === 'points') setPoints((p) => p.slice(0, -1))
      else return
      e.preventDefault()
      e.stopPropagation()
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [options.mode, options.smooth, zoom])

  const onDown = (e: ReactPointerEvent) => {
    if (e.button !== 0) return
    e.preventDefault()
    e.stopPropagation()
    const p = toWorld(e.clientX, e.clientY)
    if (options.mode === 'freehand') {
      drawing.current = true
      ;(e.target as Element).setPointerCapture(e.pointerId)
      setPoints([p])
    } else {
      if (points.length > 2 && closeEnough(points[0], p)) return finish([...points, points[0]])
      if (e.detail >= 2) return finish(points)
      setPoints((list) => [...list, p])
    }
  }
  const onMove = (e: ReactPointerEvent) => {
    const p = toWorld(e.clientX, e.clientY)
    setHoverPoint(p)
    if (options.mode === 'freehand' && drawing.current) setPoints((list) => (list.length && Math.hypot(list[list.length - 1].x - p.x, list[list.length - 1].y - p.y) * zoom < 2 ? list : [...list, p]))
  }
  const onUp = () => {
    if (options.mode === 'freehand' && drawing.current) {
      drawing.current = false
      finish(pointsRef.current)
    }
  }

  // Preview (screen space).
  const preview = [...points, ...(options.mode === 'points' && hoverPoint && points.length ? [hoverPoint] : [])]
  let d = ''
  if (preview.length >= 2) {
    const path = options.mode === 'points' && options.smooth ? pointsToPath(preview, false, true) : pointsToPath(preview, false, false)
    for (const c of path.commands) {
      if (c.kind === 'MoveTo') {
        const s = toScreen(c.value)
        d += `M${s.x} ${s.y}`
      } else if (c.kind === 'LineTo') {
        const s = toScreen(c.value)
        d += `L${s.x} ${s.y}`
      } else if (c.kind === 'CubicTo') {
        const a = toScreen(c.value.control1)
        const b = toScreen(c.value.control2)
        const t = toScreen(c.value.to)
        d += `C${a.x} ${a.y} ${b.x} ${b.y} ${t.x} ${t.y}`
      }
    }
  }
  const nearStart = options.mode === 'points' && hoverPoint && points.length > 2 && closeEnough(points[0], hoverPoint)

  return (
    <div className="absolute inset-0 z-30" style={{ cursor: 'crosshair', touchAction: 'none' }} onPointerDown={onDown} onPointerMove={onMove} onPointerUp={onUp} onPointerLeave={() => setHoverPoint(null)}>
      <svg className="pointer-events-none absolute inset-0 h-full w-full overflow-visible">
        {d && <path d={d} fill="none" stroke="rgb(56 189 248)" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" />}
        {options.mode === 'points' &&
          points.map((p, i) => {
            const s = toScreen(p)
            return <circle key={i} cx={s.x} cy={s.y} r={i === 0 && nearStart ? 7 : 4} fill={i === 0 ? 'rgb(56 189 248)' : 'white'} stroke="rgb(56 189 248)" strokeWidth={1.5} />
          })}
      </svg>
    </div>
  )
}
