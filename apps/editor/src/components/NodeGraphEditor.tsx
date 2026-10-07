import { useRef, useState } from 'react'
import type { GraphEdgeDef, GraphNodeDef, GraphNodeKind, GraphProperty, NodeGraphDef, NodeSummary, OpaqueId } from '../ipc/document'
import { Dropdown } from './InspectorPanel'

/**
 * The node-graph half of a Programmable State's authoring UI — the "Nodes"
 * tab of `ProgramEditorDock`'s drilled-in state view (see that component's
 * own doc comment for the double-click-to-enter navigation this lives
 * behind). A real draggable node canvas, deliberately built the same way
 * `StateMachineGraph.tsx`'s own transition graph is (pan/zoom viewport,
 * pointer-capture drag state machine, `position * zoom + pan` box placement,
 * `[data-*]` + `elementFromPoint` for drop-target hit-testing) — the
 * "draggable flow" the user asked for, matching how the State Machine
 * graph itself already works. The connection model is the genuinely
 * different part: instead of one implicit handle per whole box, every node
 * exposes labeled input sockets (left edge) and output sockets (right
 * edge), and a wire is dragged from a specific output dot to a specific
 * input dot rather than box-to-box.
 *
 * Every edit recomputes the whole `NodeGraphDef` and calls `onChange` once,
 * same "frontend computes the full new value, one command" convention
 * `set_state_program` (`editor.rs`) documents.
 *
 * v1 scope (mirrors `an1mbyt3_core::program`'s own doc comment): continuous
 * numeric values only, one wire per input socket at a time (picking a new
 * source replaces whichever edge already fed that socket — the Rust
 * evaluator would only take the last one wired anyway, so the UI never
 * lets that ambiguity arise in the first place).
 */

const NODE_WIDTH = 200
const HEADER_HEIGHT = 28
const EXTRA_ROW_HEIGHT = 24
const SOCKET_ROW_HEIGHT = 22
const PADDING_BOTTOM = 8

const MIN_ZOOM = 0.4
const MAX_ZOOM = 2.5
const ZOOM_STEP = 1.1

const SOCKET_DOT = 'h-2.5 w-2.5 shrink-0 rounded-full border border-overlay/40 bg-[#1a1a1f]'

type Dragging =
  | { kind: 'pan'; startClientX: number; startClientY: number; startPan: { x: number; y: number } }
  | { kind: 'node'; id: number; startClientX: number; startClientY: number; origin: { x: number; y: number }; moved: boolean }
  | { kind: 'wire'; fromNode: number; fromSocket: number; x: number; y: number }

const PROPERTIES: GraphProperty[] = [
  'TranslationX',
  'TranslationY',
  'Rotation',
  'ScaleX',
  'ScaleY',
  'AnchorX',
  'AnchorY',
  'FillColorR',
  'FillColorG',
  'FillColorB',
  'FillColorA',
]

type PaletteCategory = 'Sources' | 'Math' | 'Logic' | 'Property'

const PALETTE: { label: string; category: PaletteCategory; make: () => GraphNodeKind }[] = [
  { label: 'Constant', category: 'Sources', make: () => ({ kind: 'Constant', value: 0 }) },
  { label: 'Time', category: 'Sources', make: () => ({ kind: 'Time' }) },
  { label: 'Mouse Position', category: 'Sources', make: () => ({ kind: 'MousePosition' }) },
  { label: 'Random', category: 'Sources', make: () => ({ kind: 'Random', value: { seed: 0 } }) },
  { label: 'Get Number Input', category: 'Sources', make: () => ({ kind: 'GetNumberInput', value: { name: '' } }) },
  { label: 'Get Bool Input', category: 'Sources', make: () => ({ kind: 'GetBoolInput', value: { name: '' } }) },
  { label: 'Add', category: 'Math', make: () => ({ kind: 'Add' }) },
  { label: 'Subtract', category: 'Math', make: () => ({ kind: 'Subtract' }) },
  { label: 'Multiply', category: 'Math', make: () => ({ kind: 'Multiply' }) },
  { label: 'Divide', category: 'Math', make: () => ({ kind: 'Divide' }) },
  { label: 'Sin', category: 'Math', make: () => ({ kind: 'Sin' }) },
  { label: 'Cos', category: 'Math', make: () => ({ kind: 'Cos' }) },
  { label: 'Lerp', category: 'Math', make: () => ({ kind: 'Lerp' }) },
  { label: 'Clamp', category: 'Math', make: () => ({ kind: 'Clamp' }) },
  { label: 'Min', category: 'Math', make: () => ({ kind: 'Min' }) },
  { label: 'Max', category: 'Math', make: () => ({ kind: 'Max' }) },
  { label: 'Greater Than', category: 'Logic', make: () => ({ kind: 'GreaterThan' }) },
  { label: 'Less Than', category: 'Logic', make: () => ({ kind: 'LessThan' }) },
  { label: 'Equals', category: 'Logic', make: () => ({ kind: 'Equals' }) },
  { label: 'And', category: 'Logic', make: () => ({ kind: 'And' }) },
  { label: 'Or', category: 'Logic', make: () => ({ kind: 'Or' }) },
  { label: 'Not', category: 'Logic', make: () => ({ kind: 'Not' }) },
  { label: 'Select', category: 'Logic', make: () => ({ kind: 'Select' }) },
  { label: 'Get Property', category: 'Property', make: () => ({ kind: 'GetProperty', value: { node: { idx: 0, version: 0 }, property: 'TranslationX' } }) },
  { label: 'Set Property', category: 'Property', make: () => ({ kind: 'SetProperty', value: { node: { idx: 0, version: 0 }, property: 'TranslationX', global: false } }) },
]

const PALETTE_CATEGORIES: PaletteCategory[] = ['Sources', 'Math', 'Logic', 'Property']

function inputLabels(kind: GraphNodeKind): string[] {
  switch (kind.kind) {
    case 'Sin':
    case 'Cos':
    case 'SetProperty':
    case 'Not':
      return ['Value']
    case 'Add':
    case 'Subtract':
    case 'Multiply':
    case 'Divide':
    case 'Min':
    case 'Max':
    case 'GreaterThan':
    case 'LessThan':
    case 'Equals':
    case 'And':
    case 'Or':
      return ['A', 'B']
    case 'Lerp':
      return ['A', 'B', 'T']
    case 'Clamp':
      return ['Value', 'Min', 'Max']
    case 'Select':
      return ['Cond', 'If True', 'If False']
    default:
      return []
  }
}

function outputLabels(kind: GraphNodeKind): string[] {
  if (kind.kind === 'MousePosition') return ['X', 'Y']
  if (kind.kind === 'SetProperty') return []
  return ['Value']
}

/** How many extra kind-specific field rows a node's body needs, above its
 * socket rows — `Constant`'s value field, `Random`'s seed field,
 * `GetProperty`/`SetProperty`'s node+property pickers,
 * `GetNumberInput`/`GetBoolInput`'s name field. */
function extraRowCount(kind: GraphNodeKind): number {
  switch (kind.kind) {
    case 'Constant':
    case 'Random':
    case 'GetNumberInput':
    case 'GetBoolInput':
      return 1
    case 'GetProperty':
      return 2
    case 'SetProperty':
      // A third row for the "Global" checkbox — only meaningful for the
      // two translation axes today (see `GraphNodeKind`'s `SetProperty`
      // doc comment), so it's hidden for any other property rather than
      // shown as a dead control.
      return kind.value.property === 'TranslationX' || kind.value.property === 'TranslationY' ? 3 : 2
    default:
      return 0
  }
}

function nodeTitle(kind: GraphNodeKind): string {
  return PALETTE.find((p) => p.make().kind === kind.kind)?.label ?? kind.kind
}

/** A node's full layout: total size plus every socket's row-center Y offset
 * (from the node's own top edge, in *unscaled* (zoom=1) units) — shared by
 * both the box's own rendering and the wire/drag-preview math below, so
 * the two can never drift apart. */
function layoutFor(node: GraphNodeDef) {
  const inputs = inputLabels(node.kind)
  const outputs = outputLabels(node.kind)
  const socketRows = Math.max(inputs.length, outputs.length)
  const extraRows = extraRowCount(node.kind)
  const height = HEADER_HEIGHT + extraRows * EXTRA_ROW_HEIGHT + socketRows * SOCKET_ROW_HEIGHT + PADDING_BOTTOM
  const socketRowY = (row: number) => HEADER_HEIGHT + extraRows * EXTRA_ROW_HEIGHT + row * SOCKET_ROW_HEIGHT + SOCKET_ROW_HEIGHT / 2
  return { width: NODE_WIDTH, height, inputs, outputs, socketRowY }
}

function sameNode(a: OpaqueId, b: OpaqueId): boolean {
  return a.idx === b.idx && a.version === b.version
}

export function NodeGraphEditor({ graph, nodes, onChange }: { graph: NodeGraphDef; nodes: NodeSummary[]; onChange: (graph: NodeGraphDef) => void }) {
  const viewportRef = useRef<HTMLDivElement>(null)
  const [pan, setPan] = useState({ x: 60, y: 40 })
  const [zoom, setZoom] = useState(1)
  const [dragging, setDragging] = useState<Dragging | null>(null)

  const toLocal = (clientX: number, clientY: number) => {
    const rect = viewportRef.current?.getBoundingClientRect()
    return { x: clientX - (rect?.left ?? 0), y: clientY - (rect?.top ?? 0) }
  }

  const screenPositionOf = (node: GraphNodeDef) => ({ x: pan.x + node.position.x * zoom, y: pan.y + node.position.y * zoom })

  const liveOffsetFor = (id: number, current: { x: number; y: number } | null) => {
    if (dragging?.kind !== 'node' || dragging.id !== id || !current) return { x: 0, y: 0 }
    return { x: current.x - dragging.startClientX, y: current.y - dragging.startClientY }
  }

  const [dragCurrent, setDragCurrent] = useState<{ x: number; y: number } | null>(null)

  function updateNode(id: number, patch: Partial<GraphNodeDef>) {
    onChange({ ...graph, nodes: graph.nodes.map((n) => (n.id === id ? { ...n, ...patch } : n)) })
  }

  function removeNode(id: number) {
    onChange({ nodes: graph.nodes.filter((n) => n.id !== id), edges: graph.edges.filter((e) => e.fromNode !== id && e.toNode !== id) })
  }

  function addNode(make: () => GraphNodeKind) {
    const nextId = graph.nodes.length === 0 ? 0 : Math.max(...graph.nodes.map((n) => n.id)) + 1
    const rect = viewportRef.current?.getBoundingClientRect()
    const centerScreen = { x: (rect?.width ?? 400) / 2, y: (rect?.height ?? 300) / 2 }
    const position = { x: (centerScreen.x - pan.x) / zoom, y: (centerScreen.y - pan.y) / zoom }
    onChange({ ...graph, nodes: [...graph.nodes, { id: nextId, kind: make(), position, overrides: [] }] })
  }

  function setSocketSource(toNode: number, toSocket: number, source: { fromNode: number; fromSocket: number } | null) {
    const withoutExisting = graph.edges.filter((e) => !(e.toNode === toNode && e.toSocket === toSocket))
    const edges: GraphEdgeDef[] = source ? [...withoutExisting, { fromNode: source.fromNode, fromSocket: source.fromSocket, toNode, toSocket }] : withoutExisting
    onChange({ ...graph, edges })
  }

  function setOverride(nodeId: number, socketIndex: number, patch: Partial<{ value: number; useOverride: boolean }>) {
    const node = graph.nodes.find((n) => n.id === nodeId)
    if (!node) return
    const existing = node.overrides.find((o) => o.socketIndex === socketIndex)
    const next = { socketIndex, value: existing?.value ?? 0, useOverride: existing?.useOverride ?? false, ...patch }
    updateNode(nodeId, { overrides: [...node.overrides.filter((o) => o.socketIndex !== socketIndex), next] })
  }

  const handleViewportPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return
    ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
    setDragging({ kind: 'pan', startClientX: e.clientX, startClientY: e.clientY, startPan: pan })
    setDragCurrent({ x: e.clientX, y: e.clientY })
  }

  const handleNodePointerDown = (e: React.PointerEvent<HTMLDivElement>, node: GraphNodeDef) => {
    e.stopPropagation()
    ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
    setDragging({ kind: 'node', id: node.id, startClientX: e.clientX, startClientY: e.clientY, origin: node.position, moved: false })
    setDragCurrent({ x: e.clientX, y: e.clientY })
  }

  const handleOutputPointerDown = (e: React.PointerEvent<HTMLDivElement>, nodeId: number, socket: number) => {
    e.stopPropagation()
    ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
    const local = toLocal(e.clientX, e.clientY)
    setDragging({ kind: 'wire', fromNode: nodeId, fromSocket: socket, x: local.x, y: local.y })
  }

  const handlePointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!dragging) return
    if (dragging.kind === 'pan') {
      setPan({ x: dragging.startPan.x + (e.clientX - dragging.startClientX), y: dragging.startPan.y + (e.clientY - dragging.startClientY) })
    } else if (dragging.kind === 'node') {
      const dx = e.clientX - dragging.startClientX
      const dy = e.clientY - dragging.startClientY
      if (Math.abs(dx) > 3 || Math.abs(dy) > 3) setDragging({ ...dragging, moved: true })
      setDragCurrent({ x: e.clientX, y: e.clientY })
    } else if (dragging.kind === 'wire') {
      const local = toLocal(e.clientX, e.clientY)
      setDragging({ ...dragging, x: local.x, y: local.y })
    }
  }

  const handlePointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!dragging) return
    if (dragging.kind === 'node') {
      const dx = e.clientX - dragging.startClientX
      const dy = e.clientY - dragging.startClientY
      if (dragging.moved) {
        updateNode(dragging.id, { position: { x: dragging.origin.x + dx / zoom, y: dragging.origin.y + dy / zoom } })
      }
    } else if (dragging.kind === 'wire') {
      const target = document.elementFromPoint(e.clientX, e.clientY)?.closest('[data-input-socket]') as HTMLElement | null
      if (target) {
        const toNode = Number(target.dataset.nodeId)
        const toSocket = Number(target.dataset.socketIndex)
        setSocketSource(toNode, toSocket, { fromNode: dragging.fromNode, fromSocket: dragging.fromSocket })
      }
    }
    setDragging(null)
    setDragCurrent(null)
  }

  const handleWheel = (e: React.WheelEvent<HTMLDivElement>) => {
    e.preventDefault()
    const local = toLocal(e.clientX, e.clientY)
    const factor = e.deltaY > 0 ? 1 / ZOOM_STEP : ZOOM_STEP
    const newZoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom * factor))
    setPan({ x: local.x - ((local.x - pan.x) / zoom) * newZoom, y: local.y - ((local.y - pan.y) / zoom) * newZoom })
    setZoom(newZoom)
  }

  // Every (node id, output socket) pair's screen position right now — the
  // shared source of truth for both the wires drawn below and the
  // in-progress drag preview's start point.
  function outputScreenPoint(nodeId: number, socket: number) {
    const node = graph.nodes.find((n) => n.id === nodeId)
    if (!node) return null
    const { socketRowY } = layoutFor(node)
    const base = screenPositionOf(node)
    const offset = liveOffsetFor(node.id, dragCurrent)
    return { x: base.x + offset.x + NODE_WIDTH * zoom, y: base.y + offset.y + socketRowY(socket) * zoom }
  }
  function inputScreenPoint(nodeId: number, socket: number) {
    const node = graph.nodes.find((n) => n.id === nodeId)
    if (!node) return null
    const { socketRowY } = layoutFor(node)
    const base = screenPositionOf(node)
    const offset = liveOffsetFor(node.id, dragCurrent)
    return { x: base.x + offset.x, y: base.y + offset.y + socketRowY(socket) * zoom }
  }

  return (
    <div className="flex h-full flex-col gap-1.5">
      <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">Nodes</span>

      <div className="flex min-h-0 flex-1 gap-1.5">
        <div className="flex w-40 shrink-0 flex-col gap-2.5 overflow-y-auto rounded-md border border-overlay/10 bg-black/20 p-1.5">
          {PALETTE_CATEGORIES.map((category) => (
            <div key={category} className="flex flex-col gap-1">
              <span className="px-0.5 text-[10px] uppercase tracking-wide text-ink-dim/70">{category}</span>
              {PALETTE.filter((p) => p.category === category).map((entry) => (
                <button
                  key={entry.label}
                  type="button"
                  onClick={() => addNode(entry.make)}
                  title={`Add ${entry.label}`}
                  className="rounded-md bg-overlay/5 px-2 py-1 text-left text-[11px] text-ink hover:bg-accent/20 hover:text-accent"
                >
                  {entry.label}
                </button>
              ))}
            </div>
          ))}
        </div>

        <div
          ref={viewportRef}
          onPointerDown={handleViewportPointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          onWheel={handleWheel}
          className={`relative min-h-0 flex-1 overflow-hidden rounded-md border border-overlay/10 bg-black/30 ${dragging?.kind === 'pan' ? 'cursor-grabbing' : 'cursor-grab'}`}
        >
        <svg className="pointer-events-none absolute inset-0 h-full w-full">
          {graph.edges.map((edge, index) => {
            const from = outputScreenPoint(edge.fromNode, edge.fromSocket)
            const to = inputScreenPoint(edge.toNode, edge.toSocket)
            if (!from || !to) return null
            const midX = (from.x + to.x) / 2
            return (
              <path
                key={index}
                d={`M ${from.x} ${from.y} C ${midX} ${from.y}, ${midX} ${to.y}, ${to.x} ${to.y}`}
                fill="none"
                stroke="rgba(120,170,255,0.65)"
                strokeWidth={1.5}
              />
            )
          })}
          {dragging?.kind === 'wire' &&
            (() => {
              const from = outputScreenPoint(dragging.fromNode, dragging.fromSocket)
              if (!from) return null
              const midX = (from.x + dragging.x) / 2
              return <path d={`M ${from.x} ${from.y} C ${midX} ${from.y}, ${midX} ${dragging.y}, ${dragging.x} ${dragging.y}`} fill="none" stroke="rgba(120,170,255,0.9)" strokeWidth={1.5} />
            })()}
        </svg>

        {graph.nodes.map((node) => {
          const { width, height, inputs, outputs } = layoutFor(node)
          const pos = screenPositionOf(node)
          const offset = liveOffsetFor(node.id, dragCurrent)
          const socketRows = Math.max(inputs.length, outputs.length)
          const extraRows = extraRowCount(node.kind)

          return (
            <div
              key={node.id}
              className="absolute select-none rounded-md border border-overlay/15 bg-[#1b1b20] shadow-lg"
              style={{ left: pos.x + offset.x, top: pos.y + offset.y, width: width * zoom, transformOrigin: 'top left', transform: `scale(${zoom})` }}
            >
              <div onPointerDown={(e) => handleNodePointerDown(e, node)} className="flex cursor-grab items-center justify-between rounded-t-md bg-overlay/8 px-2" style={{ height: HEADER_HEIGHT }}>
                <span className="truncate text-[11px] font-medium text-ink">
                  #{node.id} {nodeTitle(node.kind)}
                </span>
                <button onPointerDown={(e) => e.stopPropagation()} onClick={() => removeNode(node.id)} className="text-[11px] text-ink-dim hover:text-red-400">
                  ✕
                </button>
              </div>

              {node.kind.kind === 'Constant' && (
                <div className="flex items-center px-2" style={{ height: EXTRA_ROW_HEIGHT }}>
                  <input
                    type="number"
                    onPointerDown={(e) => e.stopPropagation()}
                    className="w-full min-w-0 rounded bg-overlay/8 px-1.5 py-0.5 text-[11px] text-ink outline-none"
                    value={node.kind.value}
                    onChange={(e) => updateNode(node.id, { kind: { kind: 'Constant', value: Number(e.target.value) || 0 } })}
                  />
                </div>
              )}

              {node.kind.kind === 'Random' && (
                <div className="flex items-center gap-1.5 px-2 text-[11px] text-ink-dim" style={{ height: EXTRA_ROW_HEIGHT }}>
                  Seed
                  <input
                    type="number"
                    onPointerDown={(e) => e.stopPropagation()}
                    className="w-14 min-w-0 rounded bg-overlay/8 px-1.5 py-0.5 text-[11px] text-ink outline-none"
                    value={node.kind.value.seed}
                    onChange={(e) => updateNode(node.id, { kind: { kind: 'Random', value: { seed: Math.max(0, Math.trunc(Number(e.target.value) || 0)) } } })}
                  />
                </div>
              )}

              {(node.kind.kind === 'GetNumberInput' || node.kind.kind === 'GetBoolInput') && (
                <div className="flex items-center gap-1.5 px-2 text-[11px] text-ink-dim" style={{ height: EXTRA_ROW_HEIGHT }}>
                  Name
                  <input
                    type="text"
                    onPointerDown={(e) => e.stopPropagation()}
                    className="w-full min-w-0 rounded bg-overlay/8 px-1.5 py-0.5 text-[11px] text-ink outline-none"
                    placeholder="input name"
                    value={node.kind.value.name}
                    onChange={(e) => {
                      const kind = node.kind
                      if (kind.kind === 'GetNumberInput' || kind.kind === 'GetBoolInput') {
                        updateNode(node.id, { kind: { kind: kind.kind, value: { name: e.target.value } } })
                      }
                    }}
                  />
                </div>
              )}

              {(node.kind.kind === 'GetProperty' || node.kind.kind === 'SetProperty') && (
                <>
                  <div className="flex items-center px-2" style={{ height: EXTRA_ROW_HEIGHT }} onPointerDown={(e) => e.stopPropagation()}>
                    <Dropdown
                      value={`${node.kind.value.node.idx}:${node.kind.value.node.version}`}
                      options={nodes.map((n) => `${n.id.idx}:${n.id.version}`)}
                      renderValue={(key) => nodes.find((n) => `${n.id.idx}:${n.id.version}` === key)?.name ?? 'Select node…'}
                      renderOption={(key) => nodes.find((n) => `${n.id.idx}:${n.id.version}` === key)?.name ?? key}
                      onChange={(key) => {
                        const found = nodes.find((n) => `${n.id.idx}:${n.id.version}` === key)
                        const kind = node.kind
                        if (!found) return
                        // Split by variant (rather than one shared branch
                        // keyed on `kind.kind`) since `SetProperty`'s value
                        // shape has an extra `global` field `GetProperty`'s
                        // doesn't — spreading `...kind.value` generically
                        // under a widened `'GetProperty' | 'SetProperty'`
                        // tag no longer matches either half of the
                        // discriminated union on its own.
                        if (kind.kind === 'GetProperty') {
                          updateNode(node.id, { kind: { kind: 'GetProperty', value: { ...kind.value, node: found.id } } })
                        } else if (kind.kind === 'SetProperty') {
                          updateNode(node.id, { kind: { kind: 'SetProperty', value: { ...kind.value, node: found.id } } })
                        }
                      }}
                    />
                  </div>
                  <div className="flex items-center px-2" style={{ height: EXTRA_ROW_HEIGHT }} onPointerDown={(e) => e.stopPropagation()}>
                    <Dropdown
                      value={node.kind.value.property}
                      options={PROPERTIES}
                      onChange={(property) => {
                        const kind = node.kind
                        if (kind.kind === 'GetProperty') {
                          updateNode(node.id, { kind: { kind: 'GetProperty', value: { ...kind.value, property } } })
                        } else if (kind.kind === 'SetProperty') {
                          updateNode(node.id, { kind: { kind: 'SetProperty', value: { ...kind.value, property } } })
                        }
                      }}
                    />
                  </div>
                  {node.kind.kind === 'SetProperty' && (node.kind.value.property === 'TranslationX' || node.kind.value.property === 'TranslationY') && (
                    <label
                      className="flex items-center gap-1.5 px-2 text-[11px] text-ink-dim"
                      style={{ height: EXTRA_ROW_HEIGHT }}
                      onPointerDown={(e) => e.stopPropagation()}
                      title="Treat the incoming value as a world-space coordinate instead of this node's own local translation — the runtime converts it through the parent chain automatically, so a deeply-nested child/grandchild can follow the mouse (or any other world-space source) without hand-derived parent-inverse math."
                    >
                      <input
                        type="checkbox"
                        checked={node.kind.value.global}
                        onChange={(e) => {
                          const kind = node.kind
                          if (kind.kind === 'SetProperty') {
                            updateNode(node.id, { kind: { kind: 'SetProperty', value: { ...kind.value, global: e.target.checked } } })
                          }
                        }}
                      />
                      Global (world-space)
                    </label>
                  )}
                </>
              )}

              {Array.from({ length: socketRows }).map((_, row) => {
                const inputLabel = inputs[row]
                const outputLabel = outputs[row]
                const wire = inputLabel !== undefined ? graph.edges.find((e) => e.toNode === node.id && e.toSocket === row) : undefined
                const override = inputLabel !== undefined ? node.overrides.find((o) => o.socketIndex === row) : undefined
                return (
                  <div key={row} className="flex items-center justify-between gap-1 px-1" style={{ height: SOCKET_ROW_HEIGHT }}>
                    {inputLabel !== undefined ? (
                      <div className="flex min-w-0 flex-1 items-center gap-1">
                        <div
                          data-input-socket
                          data-node-id={node.id}
                          data-socket-index={row}
                          className={`${SOCKET_DOT} ${wire ? '!bg-[#78aaff]' : ''}`}
                          title={inputLabel}
                        />
                        <span className="w-8 shrink-0 truncate text-[10px] text-ink-dim">{inputLabel}</span>
                        {wire ? (
                          <>
                            <button onPointerDown={(e) => e.stopPropagation()} onClick={() => setSocketSource(node.id, row, null)} title="Disconnect" className="text-[9px] text-ink-dim hover:text-red-400">
                              ✕
                            </button>
                            <label className="ml-auto flex items-center gap-0.5 text-[9px] text-ink-dim" title="Pin to the override value instead of the live wire">
                              <input
                                type="checkbox"
                                onPointerDown={(e) => e.stopPropagation()}
                                checked={!!override?.useOverride}
                                onChange={(e) => setOverride(node.id, row, { useOverride: e.target.checked })}
                              />
                              pin
                            </label>
                          </>
                        ) : null}
                        {(!wire || override?.useOverride) && (
                          <input
                            type="number"
                            onPointerDown={(e) => e.stopPropagation()}
                            className="w-10 min-w-0 rounded bg-overlay/8 px-1 py-0.5 text-[10px] text-ink outline-none"
                            value={override?.value ?? 0}
                            onChange={(e) => setOverride(node.id, row, { value: Number(e.target.value) || 0 })}
                          />
                        )}
                      </div>
                    ) : (
                      <span />
                    )}
                    {outputLabel !== undefined && (
                      <div className="flex shrink-0 items-center gap-1">
                        <span className="text-[10px] text-ink-dim">{outputLabel}</span>
                        <div onPointerDown={(e) => handleOutputPointerDown(e, node.id, row)} className={`${SOCKET_DOT} cursor-crosshair hover:!bg-[#78aaff]`} title={outputLabel} />
                      </div>
                    )}
                  </div>
                )
              })}
            </div>
          )
        })}

        <div
          onPointerDown={(e) => e.stopPropagation()}
          className="pointer-events-auto absolute bottom-2 right-2 flex items-center gap-1 rounded-md border border-overlay/10 bg-black/50 px-1 py-0.5"
        >
          <button onClick={() => setZoom((z) => Math.max(MIN_ZOOM, z / ZOOM_STEP))} className="rounded px-1.5 py-0.5 text-xs text-ink-dim hover:bg-overlay/10 hover:text-ink">
            −
          </button>
          <button onClick={() => setZoom(1)} className="w-10 rounded px-1 py-0.5 text-center text-[11px] tabular-nums text-ink-dim hover:bg-overlay/10 hover:text-ink">
            {Math.round(zoom * 100)}%
          </button>
          <button onClick={() => setZoom((z) => Math.min(MAX_ZOOM, z * ZOOM_STEP))} className="rounded px-1.5 py-0.5 text-xs text-ink-dim hover:bg-overlay/10 hover:text-ink">
            +
          </button>
        </div>

        {graph.nodes.length === 0 && (
          <p className="pointer-events-none absolute left-1/2 top-1/2 w-52 -translate-x-1/2 -translate-y-1/2 text-center text-[11px] text-ink-dim">
            No nodes yet — click a block on the left to add one, then drag from an output dot to an input dot to wire them together.
          </p>
        )}
        </div>
      </div>
    </div>
  )
}
