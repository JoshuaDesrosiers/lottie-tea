import { useEffect, useRef } from 'react'
import type { Node3D, NodeSummary, OpaqueId } from '../ipc/document'
import { previewNode3DTransform, type WasmEditor } from '../ipc/wasmEditor'

/**
 * Design mode's **3D scene view** input — Unity's Scene view controls on the
 * canvas while the view is in 3D:
 *
 * - **Right-drag** (or Alt + left-drag): orbit. **Middle-drag** (or
 *   Shift + right-drag): pan. **Wheel**: zoom. **F**: frame the selection.
 * - **Left-click**: select the 3D object under the cursor (empty space
 *   deselects).
 * - **Drag a gizmo arrow** (X red / Y green / Z blue) to move along that
 *   axis, or the **centre cube** to slide on the ground plane. The move
 *   previews live and commits as one undo step on release.
 *
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
  /** Commit a finished move (one undo step). */
  onCommit: (node: OpaqueId, value: Node3D) => void
}) {
  const latest = useRef(options)
  latest.current = options

  // Engine state: whether the 3D view is on, and what's selected.
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
    const canvas = options.canvasRef.current
    if (!canvas || !options.active) return
    type Drag =
      | { kind: 'orbit' | 'pan'; x: number; y: number }
      | { kind: 'gizmo'; handle: number; node: NodeSummary; original: Node3D; x0: number; y0: number; moved: boolean; last: Node3D }
    let drag: Drag | null = null
    const local = (e: MouseEvent) => {
      const r = canvas.getBoundingClientRect()
      return { x: e.clientX - r.left, y: e.clientY - r.top }
    }
    const stop = (e: Event) => {
      e.stopImmediatePropagation()
      e.preventDefault()
    }
    const sendPreview = (node: NodeSummary, value: Node3D) => {
      const { editor, artboardId } = latest.current
      if (!editor || !artboardId) return
      const t = value.transform
      previewNode3DTransform(editor, artboardId, node.id, [...t.position, ...t.rotationDegrees, ...t.scale])
    }

    const onDown = (e: PointerEvent) => {
      const { editor, artboardId, nodes } = latest.current
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
      const hit = editor.view3d_pick(artboardId, p.x, p.y) as { handle: number | null; node: OpaqueId | null }
      if (hit.handle !== null) {
        const sel = latest.current.selectedId
        const node = sel ? nodes.find((n) => n.id.idx === sel.idx && n.id.version === sel.version) : undefined
        if (node?.node3d) drag = { kind: 'gizmo', handle: hit.handle, node, original: node.node3d, x0: p.x, y0: p.y, moved: false, last: node.node3d }
        return
      }
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
      if (drag.kind !== 'gizmo') {
        if (drag.kind === 'orbit') editor.view3d_orbit(p.x - drag.x, p.y - drag.y)
        else editor.view3d_pan(p.x - drag.x, p.y - drag.y)
        drag.x = p.x
        drag.y = p.y
      } else {
        const start = drag.original.transform.position
        const pos = Array.from(editor.view3d_drag(drag.handle, Float32Array.from(start), drag.x0, drag.y0, p.x, p.y)) as [number, number, number]
        // Ctrl snaps to 0.25 m.
        const snapped = e.ctrlKey ? (pos.map((v) => Math.round(v * 4) / 4) as [number, number, number]) : pos
        drag.last = { ...drag.original, transform: { ...drag.original.transform, position: snapped } }
        drag.moved = true
        editor.view3d_hover(artboardId, p.x, p.y, drag.handle)
        sendPreview(drag.node, drag.last)
      }
    }
    const onUp = (e: PointerEvent) => {
      if (!drag) return
      stop(e)
      const finished = drag
      drag = null
      const { editor, artboardId } = latest.current
      if (finished.kind === 'gizmo' && finished.moved && editor && artboardId) {
        // Put the original back first, so the commit's undo returns to it.
        sendPreview(finished.node, finished.original)
        latest.current.onCommit(finished.node.id, finished.last)
        editor.view3d_hover(artboardId, -1e6, -1e6, -1)
      }
    }
    const onWheel = (e: WheelEvent) => {
      stop(e)
      latest.current.editor?.view3d_zoom(e.deltaY)
    }
    const swallow = (e: Event) => stop(e)
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'f' && e.key !== 'F') return
      const t = e.target as HTMLElement | null
      if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return
      const { editor, artboardId, selectedId } = latest.current
      if (editor && artboardId && selectedId) editor.view3d_frame(artboardId, selectedId)
    }

    canvas.addEventListener('pointerdown', onDown, { capture: true })
    canvas.addEventListener('pointermove', onMove, { capture: true })
    canvas.addEventListener('pointerup', onUp, { capture: true })
    canvas.addEventListener('wheel', onWheel, { capture: true, passive: false })
    for (const type of ['mousedown', 'mouseup', 'click', 'dblclick', 'contextmenu']) canvas.addEventListener(type, swallow, { capture: true })
    window.addEventListener('keydown', onKey)
    return () => {
      canvas.removeEventListener('pointerdown', onDown, { capture: true })
      canvas.removeEventListener('pointermove', onMove, { capture: true })
      canvas.removeEventListener('pointerup', onUp, { capture: true })
      canvas.removeEventListener('wheel', onWheel, { capture: true })
      for (const type of ['mousedown', 'mouseup', 'click', 'dblclick', 'contextmenu']) canvas.removeEventListener(type, swallow, { capture: true })
      window.removeEventListener('keydown', onKey)
    }
  }, [options.canvasRef, options.active])
}
