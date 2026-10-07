/**
 * On-canvas bones (Design mode): every `Bone` node drawn as a tapered
 * shape from its joint to its tip, Spine/Unity-2D-Animation style.
 *
 * - Drag a bone's body: FK — the bone rotates to point at the cursor and
 *   everything parented under it follows.
 * - Drag a bone's tip (the round handle): IK — the bone and its parent
 *   bend so the tip follows the cursor (analytic two-bone, keeping the
 *   current bend side). Hold Shift to IK the whole chain up to the root
 *   bone (FABRIK).
 *
 * Every drag is untracked while moving and lands as one undo step on
 * release (`commit_bone_pose`).
 */
import { useEffect, useRef } from 'react'
import type { NodeSummary, OpaqueId, Transform2D } from '../ipc/document'
import { worldToScreen, type Camera, type CanvasSize } from '../camera'

const idsEqual = (a: OpaqueId | null | undefined, b: OpaqueId | null | undefined) => !!a && !!b && a.idx === b.idx && a.version === b.version
const key = (id: OpaqueId) => `${id.idx}:${id.version}`

/** World position of a node-local point (same math as `Transform2D::to_affine2`). */
function localToWorld(t: Transform2D, lx: number, ly: number) {
  const px = (lx - t.anchor.x) * t.scale.x
  const py = (ly - t.anchor.y) * t.scale.y
  const c = Math.cos(t.rotation)
  const s = Math.sin(t.rotation)
  return { x: t.translation.x + px * c - py * s, y: t.translation.y + px * s + py * c }
}

interface Props {
  nodes: NodeSummary[]
  selectedId: OpaqueId | null
  camera: Camera
  canvasSize: CanvasSize
  /** Client (CSS) point -> world point. */
  toWorld: (clientX: number, clientY: number) => { x: number; y: number }
  onSelect: (bone: OpaqueId) => void
  onPreviewAim: (bone: OpaqueId, x: number, y: number) => void
  /** Returns the posed chain (root first). */
  onPreviewIk: (tip: OpaqueId, chainLength: number, x: number, y: number) => OpaqueId[]
  onCommit: (bones: OpaqueId[], originalRotations: number[]) => void
}

type Drag = { mode: 'fk' | 'ik'; bone: OpaqueId; chainLength: number; startX: number; startY: number; moved: boolean; originals: Map<string, { id: OpaqueId; rotation: number }> }

export function BoneGizmos({ nodes, selectedId, camera, canvasSize, toWorld, onSelect, onPreviewAim, onPreviewIk, onCommit }: Props) {
  const bones = nodes.filter((n) => n.kind === 'Bone' && n.boneLength !== null && n.visible)
  const byKey = new Map(nodes.map((n) => [key(n.id), n]))
  const drag = useRef<Drag | null>(null)
  // Latest callbacks for the window listeners.
  const handlers = useRef({ toWorld, onPreviewAim, onPreviewIk, onCommit })
  handlers.current = { toWorld, onPreviewAim, onPreviewIk, onCommit }

  useEffect(() => {
    const onMove = (event: PointerEvent) => {
      const d = drag.current
      if (!d) return
      if (!d.moved && Math.hypot(event.clientX - d.startX, event.clientY - d.startY) < 3) return
      d.moved = true
      const p = handlers.current.toWorld(event.clientX, event.clientY)
      if (d.mode === 'fk') handlers.current.onPreviewAim(d.bone, p.x, p.y)
      else handlers.current.onPreviewIk(d.bone, d.chainLength, p.x, p.y)
    }
    const onUp = () => {
      const d = drag.current
      drag.current = null
      if (!d || !d.moved) return
      const entries = [...d.originals.values()]
      handlers.current.onCommit(
        entries.map((e) => e.id),
        entries.map((e) => e.rotation),
      )
    }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
    return () => {
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
    }
  }, [])

  /** The bone chain (root first) of up to `max` bones ending at `tip`. */
  const chainOf = (tip: NodeSummary, max: number) => {
    const out: NodeSummary[] = []
    let cursor: NodeSummary | undefined = tip
    while (cursor && cursor.kind === 'Bone' && out.length < max) {
      out.unshift(cursor)
      cursor = cursor.parent ? byKey.get(key(cursor.parent)) : undefined
    }
    return out
  }

  const begin = (event: React.PointerEvent, bone: NodeSummary, mode: 'fk' | 'ik') => {
    if (event.button !== 0) return
    event.stopPropagation()
    event.preventDefault()
    onSelect(bone.id)
    const chainLength = mode === 'ik' ? (event.shiftKey ? 64 : 2) : 1
    const chain = mode === 'ik' ? chainOf(bone, chainLength) : [bone]
    drag.current = {
      mode,
      bone: bone.id,
      chainLength,
      startX: event.clientX,
      startY: event.clientY,
      moved: false,
      originals: new Map(chain.map((n) => [key(n.id), { id: n.id, rotation: n.transform.rotation }])),
    }
  }

  if (bones.length === 0) return null
  return (
    <svg className="pointer-events-none absolute inset-0 h-full w-full overflow-visible">
      {bones.map((bone) => {
        const t = bone.worldTransform
        const len = bone.boneLength ?? 0
        const joint = localToWorld(t, 0, 0)
        const end = localToWorld(t, len, 0)
        const a = worldToScreen(joint.x, joint.y, camera, canvasSize)
        const b = worldToScreen(end.x, end.y, camera, canvasSize)
        const dx = b.x - a.x
        const dy = b.y - a.y
        const screenLen = Math.hypot(dx, dy) || 1
        const ux = dx / screenLen
        const uy = dy / screenLen
        const w = Math.min(Math.max(screenLen * 0.12, 3), 9)
        const k = Math.min(screenLen * 0.2, 14)
        const mx = a.x + ux * k
        const my = a.y + uy * k
        const body = `M${a.x},${a.y} L${mx - uy * w},${my + ux * w} L${b.x},${b.y} L${mx + uy * w},${my - ux * w} Z`
        const selected = idsEqual(bone.id, selectedId)
        const stroke = selected ? 'rgb(253 224 71)' : 'rgb(250 204 21 / 0.75)'
        return (
          <g key={key(bone.id)}>
            <path
              d={body}
              fill={selected ? 'rgb(250 204 21 / 0.45)' : 'rgb(250 204 21 / 0.18)'}
              stroke={stroke}
              strokeWidth={selected ? 1.5 : 1}
              style={{ pointerEvents: 'visiblePainted', cursor: 'grab' }}
              onPointerDown={(e) => begin(e, bone, 'fk')}
            >
              <title>{`${bone.name} — drag to rotate (FK)`}</title>
            </path>
            <circle cx={a.x} cy={a.y} r={3} fill="rgb(24 24 27)" stroke={stroke} strokeWidth={1.25} pointerEvents="none" />
            <circle
              cx={b.x}
              cy={b.y}
              r={selected ? 5 : 4}
              fill={selected ? 'rgb(253 224 71)' : 'rgb(24 24 27)'}
              stroke={stroke}
              strokeWidth={1.25}
              style={{ pointerEvents: 'visiblePainted', cursor: 'crosshair' }}
              onPointerDown={(e) => begin(e, bone, 'ik')}
            >
              <title>Drag tip: IK (Shift = whole chain)</title>
            </circle>
          </g>
        )
      })}
    </svg>
  )
}
