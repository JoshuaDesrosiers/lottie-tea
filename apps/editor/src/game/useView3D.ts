import { useEffect, useRef } from 'react'
import type { Node3D, NodeBehaviorEntry, NodeSummary, OpaqueId, VrmPosesConfig } from '../ipc/document'
import { previewNode3DTransform, type WasmEditor } from '../ipc/wasmEditor'

/** The gizmo's mode: move, rotate, scale (W / E / R). */
export type GizmoMode = 0 | 1 | 2

/**
 * Design mode's **3D scene view** input — Unity's Scene view controls on the
 * canvas while the view is in 3D:
 *
 * - **Right-drag** (or Alt + left-drag): orbit. While the right button is
 *   held, **W A S D** fly, **Q / E** go down / up, **Shift** is faster.
 * - **Middle-drag** (or Shift + right-drag): pan. A trackpad's two-finger
 *   scroll pans too; **pinch** / **wheel** zooms toward the cursor.
 * - **F** (or double-click): frame the selection. **Numpad 1 / 3 / 7 / 5**:
 *   front / right / top / perspective view.
 * - **W / E / R**: move / rotate / scale gizmo. **Ctrl** while dragging
 *   snaps (0.25 m, 15°, 0.1×).
 * - **Left-click**: select the 3D object under the cursor (empty space
 *   deselects). On a selected avatar, its **bones** show — click a joint to
 *   select it, then drag the rotate rings to pose it (written into the
 *   avatar's active VRM pose, so it can be keyed in Animate). **Esc**
 *   lets go of the bone.
 *
 * Gizmo drags preview live and commit as one undo step on release.
 * Listeners are native and capture-phase on the canvas, and stop the event,
 * so none of the 2D canvas tools (select / marquee / pan) also react.
 */
export function useView3D(options: {
  canvasRef: React.RefObject<HTMLCanvasElement | null>
  editor: WasmEditor | null
  active: boolean
  artboardId: OpaqueId | null
  nodes: NodeSummary[]
  selectedId: OpaqueId | null
  onSelect: (node: OpaqueId | null) => void
  /** Commit a finished move / rotate / scale (one undo step). */
  onCommit: (node: OpaqueId, value: Node3D) => void
  gizmoMode: GizmoMode
  onGizmoMode: (mode: GizmoMode) => void
  /** The selected avatar bone (humanoid name), if any. */
  selectedBone: string | null
  onSelectBone: (bone: string | null) => void
  /** Commit a node's components (a bone pose edit — one undo step). */
  onCommitBehaviors: (node: OpaqueId, behaviors: NodeBehaviorEntry[]) => void
}) {
  const latest = useRef(options)
  latest.current = options

  // Engine state: whether the 3D view is on, what's selected, the mode.
  useEffect(() => {
    options.editor?.view3d_set_enabled(options.active)
  }, [options.editor, options.active])
  useEffect(() => {
    const { editor, nodes, selectedId } = options
    if (!editor) return
    const node = selectedId ? nodes.find((n) => n.id.idx === selectedId.idx && n.id.version === selectedId.version) : null
    editor.view3d_select(node?.node3d ? node.id : null)
  }, [options.editor, options.nodes, options.selectedId])
  useEffect(() => {
    options.editor?.view3d_set_gizmo_mode(options.gizmoMode)
  }, [options.editor, options.gizmoMode])
  useEffect(() => {
    options.editor?.view3d_select_bone(options.selectedBone ?? '')
  }, [options.editor, options.selectedBone])
  // A different node lets go of the bone.
  useEffect(() => {
    if (latest.current.selectedBone) latest.current.onSelectBone(null)
  }, [options.selectedId?.idx, options.selectedId?.version])

  useEffect(() => {
    const canvas = options.canvasRef.current
    if (!canvas || !options.active) return
    type Drag =
      | { kind: 'orbit' | 'pan'; x: number; y: number }
      | { kind: 'gizmo'; handle: number; node: NodeSummary; original: Node3D; x0: number; y0: number; moved: boolean; last: Node3D }
      | { kind: 'bone'; handle: number; node: NodeSummary; bone: string; pose: string; start: [number, number, number]; last: [number, number, number]; x0: number; y0: number; moved: boolean }
    let drag: Drag | null = null
    const keys = new Set<string>()
    let flyFrame = 0
    let lastFly = 0
    const local = (e: MouseEvent) => {
      const r = canvas.getBoundingClientRect()
      return { x: e.clientX - r.left, y: e.clientY - r.top }
    }
    const stop = (e: Event) => {
      e.stopImmediatePropagation()
      e.preventDefault()
    }
    const selectedNode = () => {
      const { nodes, selectedId } = latest.current
      return selectedId ? nodes.find((n) => n.id.idx === selectedId.idx && n.id.version === selectedId.version) : undefined
    }
    const sendPreview = (node: NodeSummary, value: Node3D) => {
      const { editor, artboardId } = latest.current
      if (!editor || !artboardId) return
      const t = value.transform
      previewNode3DTransform(editor, artboardId, node.id, [...t.position, ...t.rotationDegrees, ...t.scale])
    }
    const posesOf = (node: NodeSummary) => node.behaviors.find((b) => b.behavior.kind === 'VrmPoses')?.behavior as ({ kind: 'VrmPoses' } & VrmPosesConfig) | undefined
    /** The node's components with one bone of its active pose set. */
    const withBone = (node: NodeSummary, bone: string, rotation: [number, number, number]): NodeBehaviorEntry[] => {
      const has = node.behaviors.some((b) => b.behavior.kind === 'VrmPoses')
      const base: NodeBehaviorEntry[] = has ? node.behaviors : [...node.behaviors, { enabled: true, behavior: { kind: 'VrmPoses', poses: [{ name: 'Rest', bones: [], expressions: [] }], active: 'Rest', blend: 0.12 } }]
      return base.map((b) => {
        if (b.behavior.kind !== 'VrmPoses') return b
        const p = b.behavior
        const active = p.poses.some((q) => q.name === p.active) ? p.active : (p.poses[0]?.name ?? 'Rest')
        const poses = (p.poses.length ? p.poses : [{ name: 'Rest', bones: [], expressions: [] }]).map((q) => {
          if (q.name !== active) return q
          const at = q.bones.findIndex((x) => x.bone === bone)
          const entry = { bone, rotation, offset: at >= 0 ? q.bones[at].offset : ([0, 0, 0] as [number, number, number]) }
          return { ...q, bones: at >= 0 ? q.bones.map((x, i) => (i === at ? entry : x)) : [...q.bones, entry] }
        })
        return { ...b, behavior: { ...p, poses, active } }
      })
    }

    // Fly: while the right button is held, WASD / QE move the view.
    const flyTick = (now: number) => {
      const { editor } = latest.current
      const dt = Math.min(0.05, (now - lastFly) / 1000)
      lastFly = now
      if (editor && drag?.kind === 'orbit' && keys.size) {
        const speed = (keys.has('shift') ? 15 : 5) * dt
        const f = (keys.has('w') ? 1 : 0) - (keys.has('s') ? 1 : 0)
        const r = (keys.has('d') ? 1 : 0) - (keys.has('a') ? 1 : 0)
        const u = (keys.has('e') ? 1 : 0) - (keys.has('q') ? 1 : 0)
        if (f || r || u) editor.view3d_fly(f * speed, r * speed, u * speed)
      }
      flyFrame = requestAnimationFrame(flyTick)
    }
    flyFrame = requestAnimationFrame((t) => {
      lastFly = t
      flyTick(t)
    })

    const onDown = (e: PointerEvent) => {
      const { editor, artboardId } = latest.current
      if (!editor || !artboardId) return
      stop(e)
      canvas.setPointerCapture(e.pointerId)
      const p = local(e)
      if (e.button === 1 || (e.button === 2 && e.shiftKey)) {
        drag = { kind: 'pan', x: p.x, y: p.y }
        return
      }
      if (e.button === 2 || (e.button === 0 && e.altKey)) {
        drag = { kind: 'orbit', x: p.x, y: p.y }
        return
      }
      if (e.button !== 0) return
      const hit = editor.view3d_pick(artboardId, p.x, p.y) as { handle: number | null; node: OpaqueId | null; bone: string | null }
      const node = selectedNode()
      if (hit.handle !== null && node?.node3d) {
        const bone = latest.current.selectedBone
        if (bone) {
          const poses = posesOf(node)
          const active = poses?.poses.find((q) => q.name === poses.active) ?? poses?.poses[0]
          const start = (active?.bones.find((b) => b.bone === bone)?.rotation ?? [0, 0, 0]) as [number, number, number]
          // First bone edit on an avatar without VRM Poses: add it.
          if (!poses) latest.current.onCommitBehaviors(node.id, withBone(node, bone, start))
          editor.view3d_bone_drag_begin(node.id, Float32Array.from(start))
          drag = { kind: 'bone', handle: hit.handle, node, bone, pose: active?.name ?? 'Rest', start, last: start, x0: p.x, y0: p.y, moved: false }
          return
        }
        drag = { kind: 'gizmo', handle: hit.handle, node, original: node.node3d, x0: p.x, y0: p.y, moved: false, last: node.node3d }
        return
      }
      if (hit.bone) {
        latest.current.onSelectBone(hit.bone)
        return
      }
      if (latest.current.selectedBone) latest.current.onSelectBone(null)
      latest.current.onSelect(hit.node)
    }
    const onMove = (e: PointerEvent) => {
      const { editor, artboardId } = latest.current
      if (!editor || !artboardId) return
      const p = local(e)
      if (!drag) {
        editor.view3d_hover(artboardId, p.x, p.y, -1)
        return
      }
      stop(e)
      // A const copy: TS keeps its narrowing across the calls below.
      const d = drag
      if (d.kind === 'orbit' || d.kind === 'pan') {
        if (d.kind === 'orbit') editor.view3d_orbit(p.x - d.x, p.y - d.y)
        else editor.view3d_pan(p.x - d.x, p.y - d.y)
        d.x = p.x
        d.y = p.y
      } else if (d.kind === 'bone') {
        const rot = Array.from(editor.view3d_bone_drag(artboardId, d.handle, Float32Array.from(d.start), d.x0, d.y0, p.x, p.y, e.ctrlKey)) as [number, number, number]
        d.last = rot
        d.moved = true
        editor.view3d_hover(artboardId, p.x, p.y, d.handle)
        editor.preview_vrm_pose_bone(artboardId, d.node.id, d.pose, d.bone, rot[0], rot[1], rot[2])
      } else if (d.kind === 'gizmo') {
        const t = d.original.transform
        const start = [...t.position, ...t.rotationDegrees, ...t.scale]
        const v = Array.from(editor.view3d_drag_transform(artboardId, d.handle, Float32Array.from(start), d.x0, d.y0, p.x, p.y, e.ctrlKey))
        d.last = { ...d.original, transform: { position: [v[0], v[1], v[2]], rotationDegrees: [v[3], v[4], v[5]], scale: [v[6], v[7], v[8]] } }
        d.moved = true
        editor.view3d_hover(artboardId, p.x, p.y, d.handle)
        sendPreview(d.node, d.last)
      }
    }
    const onUp = (e: PointerEvent) => {
      if (!drag) return
      stop(e)
      const finished = drag
      drag = null
      keys.clear()
      const { editor, artboardId } = latest.current
      if (!editor || !artboardId) return
      if (finished.kind === 'gizmo' && finished.moved) {
        // Put the original back first, so the commit's undo returns to it.
        sendPreview(finished.node, finished.original)
        latest.current.onCommit(finished.node.id, finished.last)
        editor.view3d_hover(artboardId, -1e6, -1e6, -1)
      }
      if (finished.kind === 'bone' && finished.moved) {
        editor.preview_vrm_pose_bone(artboardId, finished.node.id, finished.pose, finished.bone, finished.start[0], finished.start[1], finished.start[2])
        const node = selectedNode() ?? finished.node
        latest.current.onCommitBehaviors(node.id, withBone(node, finished.bone, finished.last))
        editor.view3d_hover(artboardId, -1e6, -1e6, -1)
      }
    }
    // A cancelled pointer — chiefly `input/touchGestures.ts` taking a
    // finger over when a second one lands for a pinch — abandons the drag
    // instead of committing it: the gizmo / bone preview goes back to where
    // it started (the same "put the original back" `onUp` does before its
    // commit), nothing is committed, and the drag can't resume under a later
    // finger.
    const onCancel = (e: PointerEvent) => {
      if (!drag) return
      stop(e)
      const cancelled = drag
      drag = null
      keys.clear()
      const { editor, artboardId } = latest.current
      if (!editor || !artboardId) return
      if (cancelled.kind === 'gizmo' && cancelled.moved) sendPreview(cancelled.node, cancelled.original)
      if (cancelled.kind === 'bone' && cancelled.moved) {
        editor.preview_vrm_pose_bone(artboardId, cancelled.node.id, cancelled.pose, cancelled.bone, cancelled.start[0], cancelled.start[1], cancelled.start[2])
      }
      editor.view3d_hover(artboardId, -1e6, -1e6, -1)
    }
    const onWheel = (e: WheelEvent) => {
      stop(e)
      const { editor } = latest.current
      if (!editor) return
      const p = local(e)
      // Pinch arrives as ctrl + wheel; a trackpad's two-finger scroll has
      // sideways / fractional deltas — pan with it. A mouse wheel zooms.
      const trackpad = !e.ctrlKey && e.deltaMode === 0 && (Math.abs(e.deltaX) > 0.5 || !Number.isInteger(e.deltaY) || Math.abs(e.deltaY) < 20)
      if (trackpad) editor.view3d_pan(-e.deltaX, -e.deltaY)
      else editor.view3d_zoom_at(e.ctrlKey ? e.deltaY * 4 : e.deltaY, p.x, p.y)
    }
    const swallow = (e: Event) => stop(e)
    const onDouble = (e: MouseEvent) => {
      stop(e)
      const { editor, artboardId, selectedId } = latest.current
      if (editor && artboardId && selectedId) editor.view3d_frame(artboardId, selectedId)
    }
    const typing = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null
      return !!t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT' || t.isContentEditable)
    }
    const onKey = (e: KeyboardEvent) => {
      if (typing(e)) return
      const k = e.key.toLowerCase()
      // Flying: track WASD / QE / Shift while the right button is held.
      if (drag?.kind === 'orbit' && ['w', 'a', 's', 'd', 'q', 'e', 'shift'].includes(k)) {
        keys.add(k)
        e.preventDefault()
        return
      }
      if (e.ctrlKey || e.metaKey || e.altKey) return
      const { editor, artboardId, selectedId } = latest.current
      if (!editor) return
      if (k === 'f' && artboardId && selectedId) editor.view3d_frame(artboardId, selectedId)
      else if (k === 'w') latest.current.onGizmoMode(0)
      else if (k === 'e') latest.current.onGizmoMode(1)
      else if (k === 'r') latest.current.onGizmoMode(2)
      else if (k === 'escape' && latest.current.selectedBone) latest.current.onSelectBone(null)
      else if (e.code === 'Numpad1') editor.view3d_view(e.ctrlKey ? 'back' : 'front')
      else if (e.code === 'Numpad3') editor.view3d_view('right')
      else if (e.code === 'Numpad7') editor.view3d_view('top')
      else if (e.code === 'Numpad5') editor.view3d_view('persp')
    }
    const onKeyUp = (e: KeyboardEvent) => keys.delete(e.key.toLowerCase())

    canvas.addEventListener('pointerdown', onDown, { capture: true })
    canvas.addEventListener('pointermove', onMove, { capture: true })
    canvas.addEventListener('pointerup', onUp, { capture: true })
    canvas.addEventListener('pointercancel', onCancel, { capture: true })
    canvas.addEventListener('wheel', onWheel, { capture: true, passive: false })
    canvas.addEventListener('dblclick', onDouble, { capture: true })
    for (const type of ['mousedown', 'mouseup', 'click', 'contextmenu']) canvas.addEventListener(type, swallow, { capture: true })
    window.addEventListener('keydown', onKey)
    window.addEventListener('keyup', onKeyUp)
    return () => {
      cancelAnimationFrame(flyFrame)
      canvas.removeEventListener('pointerdown', onDown, { capture: true })
      canvas.removeEventListener('pointermove', onMove, { capture: true })
      canvas.removeEventListener('pointerup', onUp, { capture: true })
      canvas.removeEventListener('pointercancel', onCancel, { capture: true })
      canvas.removeEventListener('wheel', onWheel, { capture: true })
      canvas.removeEventListener('dblclick', onDouble, { capture: true })
      for (const type of ['mousedown', 'mouseup', 'click', 'contextmenu']) canvas.removeEventListener(type, swallow, { capture: true })
      window.removeEventListener('keydown', onKey)
      window.removeEventListener('keyup', onKeyUp)
    }
  }, [options.canvasRef, options.active])
}
