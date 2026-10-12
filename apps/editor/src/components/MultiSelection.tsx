/**
 * Working with several selected objects at once:
 *
 * - **`MultiTransformHandles`** — the multi-selection box gets real
 *   handles: drag a corner to scale everything about the opposite corner
 *   (Shift: keep proportions), drag the knob above it to rotate everything
 *   about the box's centre (Shift: 15° steps). Each node gets the same
 *   world-space change, previewed live and committed as one undo step
 *   (`multi_transform_*` in the engine). Moving the selection is the
 *   canvas's existing multi-drag.
 * - **`MultiFillPanel`** — one Fill for the whole selection: a colour, or a
 *   single gradient laid across all of it in world space (each object gets
 *   it in its own space, so they read as one continuously filled thing —
 *   the Group Fill behaviour, for a loose selection). The gradient's line is
 *   also draggable on the canvas (the regular gradient handle, in world
 *   space).
 */
import { useEffect, useRef, useState, type RefObject } from 'react'
import type { GradientDef } from '../ipc/document'
import { screenToWorld, worldToScreen, type Camera, type CanvasSize } from '../camera'
import { ColorSwatch } from './ColorPicker'
import { GradientClipboardButtons } from './GradientClipboard'
import { Dropdown, GradientStopList, GRADIENT_KINDS } from './InspectorPanel'

type Rgba = [number, number, number, number]
/** A world-space axis-aligned box (y up). */
export interface WorldBox {
  minX: number
  minY: number
  maxX: number
  maxY: number
}
/** A 2D affine `[a, b, c, d, tx, ty]`: x axis `(a, b)`, y axis `(c, d)`. */
export type Affine = [number, number, number, number, number, number]

const IDENTITY: Affine = [1, 0, 0, 1, 0, 0]

/** `m` applied to a point. */
function apply(m: Affine, x: number, y: number) {
  return { x: m[0] * x + m[2] * y + m[4], y: m[1] * x + m[3] * y + m[5] }
}

type Drag = { kind: 'scale'; corner: { x: number; y: number }; opposite: { x: number; y: number } } | { kind: 'rotate'; center: { x: number; y: number }; startAngle: number }

export function MultiTransformHandles({
  containerRef,
  box,
  camera,
  canvasSize,
  onBegin,
  onPreview,
  onEnd,
}: {
  containerRef: RefObject<HTMLDivElement | null>
  box: WorldBox
  camera: Camera
  canvasSize: CanvasSize
  onBegin: () => void
  onPreview: (m: Affine) => void
  onEnd: (commit: boolean) => void
}) {
  const [live, setLive] = useState<Affine | null>(null)
  const dragRef = useRef<Drag | null>(null)
  const propsRef = useRef({ camera, canvasSize, onPreview, onEnd })
  propsRef.current = { camera, canvasSize, onPreview, onEnd }

  const toWorld = (clientX: number, clientY: number) => {
    const rect = containerRef.current?.getBoundingClientRect()
    return screenToWorld(clientX - (rect?.left ?? 0), clientY - (rect?.top ?? 0), propsRef.current.camera, propsRef.current.canvasSize)
  }

  useEffect(() => {
    const move = (e: PointerEvent) => {
      const drag = dragRef.current
      if (!drag) return
      const p = toWorld(e.clientX, e.clientY)
      let m: Affine = IDENTITY
      if (drag.kind === 'scale') {
        const { corner: c, opposite: o } = drag
        const span = (v: number) => (Math.abs(v) < 1e-6 ? 1e-6 : v)
        let sx = (p.x - o.x) / span(c.x - o.x)
        let sy = (p.y - o.y) / span(c.y - o.y)
        if (e.shiftKey) {
          const s = Math.max(Math.abs(sx), Math.abs(sy))
          sx = Math.sign(sx || 1) * s
          sy = Math.sign(sy || 1) * s
        }
        m = [sx, 0, 0, sy, o.x - sx * o.x, o.y - sy * o.y]
      } else {
        const { center: c } = drag
        let angle = Math.atan2(p.y - c.y, p.x - c.x) - drag.startAngle
        if (e.shiftKey) angle = Math.round(angle / (Math.PI / 12)) * (Math.PI / 12)
        const cos = Math.cos(angle)
        const sin = Math.sin(angle)
        m = [cos, sin, -sin, cos, c.x - (cos * c.x - sin * c.y), c.y - (sin * c.x + cos * c.y)]
      }
      setLive(m)
      propsRef.current.onPreview(m)
    }
    const up = (e: PointerEvent) => {
      if (!dragRef.current) return
      dragRef.current = null
      setLive(null)
      // Escape-free commit: a click without moving still commits (no-op).
      propsRef.current.onEnd(e.type === 'pointerup')
    }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
    window.addEventListener('pointercancel', up)
    return () => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', up)
      window.removeEventListener('pointercancel', up)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const corners = [
    { x: box.minX, y: box.maxY },
    { x: box.maxX, y: box.maxY },
    { x: box.maxX, y: box.minY },
    { x: box.minX, y: box.minY },
  ]
  const center = { x: (box.minX + box.maxX) / 2, y: (box.minY + box.maxY) / 2 }
  const m = live ?? IDENTITY
  const screen = corners.map((c) => {
    const w = apply(m, c.x, c.y)
    return worldToScreen(w.x, w.y, camera, canvasSize)
  })
  // The rotate knob: 26 px above the box's top edge (in its current turn).
  const topMid = { x: (screen[0].x + screen[1].x) / 2, y: (screen[0].y + screen[1].y) / 2 }
  const centerScreen = (() => {
    const w = apply(m, center.x, center.y)
    return worldToScreen(w.x, w.y, camera, canvasSize)
  })()
  const away = { x: topMid.x - centerScreen.x, y: topMid.y - centerScreen.y }
  const len = Math.hypot(away.x, away.y) || 1
  const knob = { x: topMid.x + (away.x / len) * 26, y: topMid.y + (away.y / len) * 26 }

  const start = (e: React.PointerEvent, drag: Drag) => {
    e.preventDefault()
    e.stopPropagation()
    dragRef.current = drag
    onBegin()
  }

  return (
    <svg className="pointer-events-none absolute inset-0 h-full w-full overflow-visible">
      <polygon points={screen.map((p) => `${p.x},${p.y}`).join(' ')} fill="none" className="stroke-accent" strokeDasharray="4 3" strokeWidth={1} />
      <line x1={topMid.x} y1={topMid.y} x2={knob.x} y2={knob.y} className="stroke-accent" strokeWidth={1} />
      <circle
        cx={knob.x}
        cy={knob.y}
        r={5}
        className="pointer-events-auto cursor-grab fill-canvas stroke-accent"
        strokeWidth={1.5}
        onPointerDown={(e) => {
          const p = toWorld(e.clientX, e.clientY)
          start(e, { kind: 'rotate', center, startAngle: Math.atan2(p.y - center.y, p.x - center.x) })
        }}
      >
        <title>Rotate the selection (Shift: 15° steps)</title>
      </circle>
      {screen.map((p, i) => (
        <rect
          key={i}
          x={p.x - 4}
          y={p.y - 4}
          width={8}
          height={8}
          className="pointer-events-auto cursor-nwse-resize fill-canvas stroke-accent"
          strokeWidth={1.5}
          onPointerDown={(e) => start(e, { kind: 'scale', corner: corners[i], opposite: corners[(i + 2) % 4] })}
        >
          <title>Scale the selection (Shift: keep proportions)</title>
        </rect>
      ))}
    </svg>
  )
}

/** The Inspector's Fill for a multi-selection (see the module doc). */
export function MultiFillPanel({
  count,
  box,
  color,
  gradient,
  onColor,
  onGradient,
}: {
  count: number
  box: WorldBox | null
  color: Rgba
  gradient: GradientDef | null
  onColor: (c: Rgba) => void
  onGradient: (g: GradientDef | null) => void
}) {
  const worldBox = box ? { min: { x: box.minX, y: box.minY }, max: { x: box.maxX, y: box.maxY } } : null
  const makeGradient = (): GradientDef => {
    const b = box ?? { minX: -100, minY: 0, maxX: 100, maxY: 0 }
    const midY = (b.minY + b.maxY) / 2
    return {
      kind: 'Linear',
      start: { x: b.minX, y: midY },
      end: { x: b.maxX, y: midY },
      stops: [
        { offset: 0, color, midpoint: 0.5 },
        { offset: 1, color: [color[2], color[0], color[1], color[3]], midpoint: 0.5 },
      ],
    }
  }
  return (
    <div className="flex flex-col gap-1.5 border-b border-panel-border px-3 py-2">
      <span className="text-[11px] font-medium text-ink">{count} objects selected</span>
      <div className="flex items-center gap-1">
        <span className="w-10 shrink-0 text-[11px] text-ink-dim">Fill</span>
        <div className="flex flex-1 gap-0.5 rounded-md bg-overlay/5 p-0.5">
          <button type="button" onClick={() => onGradient(null)} className={`flex-1 rounded px-2 py-0.5 text-[10px] ${!gradient ? 'bg-accent/20 text-accent' : 'text-ink-dim hover:text-ink'}`}>
            Solid
          </button>
          <button type="button" onClick={() => onGradient(gradient ?? makeGradient())} className={`flex-1 rounded px-2 py-0.5 text-[10px] ${gradient ? 'bg-accent/20 text-accent' : 'text-ink-dim hover:text-ink'}`}>
            Gradient
          </button>
        </div>
        <GradientClipboardButtons gradient={gradient} box={worldBox} onPaste={onGradient} />
      </div>
      {!gradient && <ColorSwatch value={color} onChange={onColor} />}
      {gradient && (
        <>
          <Dropdown value={gradient.kind} options={GRADIENT_KINDS} onChange={(kind) => onGradient({ ...gradient, kind })} />
          <GradientStopList gradient={gradient} onChange={onGradient} />
          <span className="text-[10px] leading-snug text-ink-dim">One gradient across everything selected — drag its line on the canvas.</span>
        </>
      )}
    </div>
  )
}
