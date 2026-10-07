import { useMemo, useRef, useState } from 'react'
import type { ShaderEdgeDef, ShaderGraphDef, ShaderNodeDef, ShaderNodeSpec, ShaderPortType } from '../ipc/document'

/**
 * The Shader tab's node canvas — a typed shader graph (Godot VisualShader /
 * Unity Shader Graph style) for a material. Node types and their sockets
 * come from the engine (`shaderNodeCatalog()`), so this file only knows how
 * to *draw and edit* a graph, never what a node means.
 *
 * Interaction (same feel as the State Machine's Nodes editor):
 * - Drag the background to pan, wheel to zoom around the cursor.
 * - Drag a node's header to move it; click to select, Delete removes it.
 * - Drag from an output dot to an input dot to wire them (an input takes
 *   one wire; a new one replaces the old). ✕ next to a wired input cuts it.
 * - Unwired inputs show an inline value editor (number / xy / colour).
 *
 * Every finished edit calls `onChange` once (a node drag commits on
 * release), so each is one undo step.
 */

const NODE_WIDTH = 196
const HEADER_HEIGHT = 24
const VALUE_ROW_HEIGHT = 26
const ROW_HEIGHT = 22
const PADDING_BOTTOM = 6
const MIN_ZOOM = 0.3
const MAX_ZOOM = 2
const ZOOM_STEP = 1.12

/** Socket colours by type — the same colour on both ends of a wire. */
const TYPE_COLOR: Record<ShaderPortType, string> = {
  Float: '#a0a4b0',
  Vec2: '#5fd38d',
  Vec3: '#f2c84b',
  Vec4: '#c38cff',
  Dynamic: '#6fb2ff',
}

/** Header tint by palette category. */
const CATEGORY_COLOR: Record<string, string> = {
  Output: 'rgba(255,110,110,0.28)',
  Input: 'rgba(95,211,141,0.22)',
  Math: 'rgba(111,178,255,0.2)',
  Vector: 'rgba(242,200,75,0.2)',
  UV: 'rgba(80,220,220,0.2)',
  Effect: 'rgba(195,140,255,0.24)',
}

/** `SampleTexture` -> `Sample Texture`. */
export function shaderNodeTitle(kind: string): string {
  return kind.replace(/([a-z0-9])([A-Z])/g, '$1 $2').replace('Uv', 'UV').replace('2 D', '2D')
}

// sRGB <-> linear for colour pickers (graph colours are linear, like every
// colour in the document).
const toLinear = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4)
const toSrgb = (c: number) => (c <= 0.0031308 ? c * 12.92 : 1.055 * c ** (1 / 2.4) - 0.055)
function linearToHex(v: number[]): string {
  return '#' + [0, 1, 2].map((i) => Math.round(Math.min(1, Math.max(0, toSrgb(v[i] ?? 0))) * 255).toString(16).padStart(2, '0')).join('')
}
function hexToLinear(hex: string, alpha: number): [number, number, number, number] {
  const n = parseInt(hex.slice(1), 16)
  return [toLinear(((n >> 16) & 255) / 255), toLinear(((n >> 8) & 255) / 255), toLinear((n & 255) / 255), alpha]
}

type Vec4 = [number, number, number, number]

type Dragging =
  | { kind: 'pan'; startX: number; startY: number; startPan: { x: number; y: number } }
  | { kind: 'node'; id: number; startX: number; startY: number; moved: boolean }
  | { kind: 'wire'; from: number; fromPort: number; x: number; y: number }

export function ShaderGraphEditor({
  graph,
  catalog,
  errorNode,
  onChange,
}: {
  graph: ShaderGraphDef
  catalog: ShaderNodeSpec[]
  /** The node a compile error points at (outlined red). */
  errorNode: number | null
  onChange: (graph: ShaderGraphDef) => void
}) {
  const viewportRef = useRef<HTMLDivElement>(null)
  const [pan, setPan] = useState({ x: 40, y: 30 })
  const [zoom, setZoom] = useState(0.85)
  const [dragging, setDragging] = useState<Dragging | null>(null)
  const [dragNow, setDragNow] = useState<{ x: number; y: number } | null>(null)
  const [selected, setSelected] = useState<number | null>(null)
  const [search, setSearch] = useState('')

  const specs = useMemo(() => new Map(catalog.map((s) => [s.kind, s])), [catalog])
  const categories = useMemo(() => [...new Set(catalog.map((s) => s.category))], [catalog])

  const layout = (node: ShaderNodeDef) => {
    const spec = specs.get(node.kind)
    const inputs = spec?.inputs ?? []
    const outputs = spec?.outputs ?? []
    const valueRows = spec?.hasValue ? 1 : 0
    const rows = Math.max(inputs.length, outputs.length)
    const rowY = (row: number) => HEADER_HEIGHT + valueRows * VALUE_ROW_HEIGHT + row * ROW_HEIGHT + ROW_HEIGHT / 2
    return { spec, inputs, outputs, valueRows, rows, rowY, height: HEADER_HEIGHT + valueRows * VALUE_ROW_HEIGHT + rows * ROW_HEIGHT + PADDING_BOTTOM }
  }

  const toLocal = (clientX: number, clientY: number) => {
    const rect = viewportRef.current?.getBoundingClientRect()
    return { x: clientX - (rect?.left ?? 0), y: clientY - (rect?.top ?? 0) }
  }
  /** A node's live screen position (including an in-progress drag). */
  const screenPos = (node: ShaderNodeDef) => {
    let x = pan.x + node.position[0] * zoom
    let y = pan.y + node.position[1] * zoom
    if (dragging?.kind === 'node' && dragging.id === node.id && dragNow) {
      x += dragNow.x - dragging.startX
      y += dragNow.y - dragging.startY
    }
    return { x, y }
  }
  const outputPoint = (id: number, port: number) => {
    const node = graph.nodes.find((n) => n.id === id)
    if (!node) return null
    const p = screenPos(node)
    return { x: p.x + NODE_WIDTH * zoom, y: p.y + layout(node).rowY(port) * zoom }
  }
  const inputPoint = (id: number, port: number) => {
    const node = graph.nodes.find((n) => n.id === id)
    if (!node) return null
    const p = screenPos(node)
    return { x: p.x, y: p.y + layout(node).rowY(port) * zoom }
  }

  // ---- edits ----
  const updateNode = (id: number, patch: Partial<ShaderNodeDef>) => onChange({ ...graph, nodes: graph.nodes.map((n) => (n.id === id ? { ...n, ...patch } : n)) })
  const removeNode = (id: number) => {
    onChange({ ...graph, nodes: graph.nodes.filter((n) => n.id !== id), edges: graph.edges.filter((e) => e.from !== id && e.to !== id) })
    if (selected === id) setSelected(null)
  }
  const addNode = (kind: string) => {
    if (kind === 'Output' && graph.nodes.some((n) => n.kind === 'Output')) return
    const id = graph.nodes.reduce((m, n) => Math.max(m, n.id), 0) + 1
    const rect = viewportRef.current?.getBoundingClientRect()
    const cx = ((rect?.width ?? 600) / 2 - pan.x) / zoom - NODE_WIDTH / 2
    const cy = ((rect?.height ?? 300) / 2 - pan.y) / zoom - 40
    const spec = specs.get(kind)
    const value: Vec4 = kind === 'ColorConstant' ? [1, 1, 1, 1] : kind === 'FloatConstant' ? [1, 0, 0, 0] : [0, 0, 0, 0]
    onChange({ ...graph, nodes: [...graph.nodes, { id, kind, position: [cx + (id % 5) * 12, cy + (id % 5) * 12], value, defaults: (spec?.inputs ?? []).map(() => null) }] })
    setSelected(id)
  }
  const setWire = (to: number, toPort: number, from: { from: number; fromPort: number } | null) => {
    const rest = graph.edges.filter((e) => !(e.to === to && e.toPort === toPort))
    const edges: ShaderEdgeDef[] = from && from.from !== to ? [...rest, { from: from.from, fromPort: from.fromPort, to, toPort }] : rest
    onChange({ ...graph, edges })
  }
  const setDefault = (node: ShaderNodeDef, index: number, value: Vec4) => {
    const defaults = [...node.defaults]
    while (defaults.length <= index) defaults.push(null)
    defaults[index] = value
    updateNode(node.id, { defaults })
  }

  // ---- pointer handling ----
  const onViewportDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return
    e.currentTarget.setPointerCapture(e.pointerId)
    setSelected(null)
    setDragging({ kind: 'pan', startX: e.clientX, startY: e.clientY, startPan: pan })
  }
  const onHeaderDown = (e: React.PointerEvent<HTMLDivElement>, node: ShaderNodeDef) => {
    e.stopPropagation()
    ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
    setSelected(node.id)
    setDragging({ kind: 'node', id: node.id, startX: e.clientX, startY: e.clientY, moved: false })
    setDragNow({ x: e.clientX, y: e.clientY })
  }
  const onOutputDown = (e: React.PointerEvent<HTMLDivElement>, id: number, port: number) => {
    e.stopPropagation()
    ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
    const p = toLocal(e.clientX, e.clientY)
    setDragging({ kind: 'wire', from: id, fromPort: port, x: p.x, y: p.y })
  }
  const onMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!dragging) return
    if (dragging.kind === 'pan') setPan({ x: dragging.startPan.x + e.clientX - dragging.startX, y: dragging.startPan.y + e.clientY - dragging.startY })
    else if (dragging.kind === 'node') {
      if (!dragging.moved && Math.hypot(e.clientX - dragging.startX, e.clientY - dragging.startY) > 3) setDragging({ ...dragging, moved: true })
      setDragNow({ x: e.clientX, y: e.clientY })
    } else {
      const p = toLocal(e.clientX, e.clientY)
      setDragging({ ...dragging, x: p.x, y: p.y })
    }
  }
  const onUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (dragging?.kind === 'node' && dragging.moved) {
      const node = graph.nodes.find((n) => n.id === dragging.id)
      if (node) updateNode(node.id, { position: [node.position[0] + (e.clientX - dragging.startX) / zoom, node.position[1] + (e.clientY - dragging.startY) / zoom] })
    } else if (dragging?.kind === 'wire') {
      const target = document.elementFromPoint(e.clientX, e.clientY)?.closest('[data-shader-input]') as HTMLElement | null
      if (target) setWire(Number(target.dataset.node), Number(target.dataset.port), { from: dragging.from, fromPort: dragging.fromPort })
    }
    setDragging(null)
    setDragNow(null)
  }
  const onWheel = (e: React.WheelEvent<HTMLDivElement>) => {
    const p = toLocal(e.clientX, e.clientY)
    const next = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom * (e.deltaY > 0 ? 1 / ZOOM_STEP : ZOOM_STEP)))
    setPan({ x: p.x - ((p.x - pan.x) / zoom) * next, y: p.y - ((p.y - pan.y) / zoom) * next })
    setZoom(next)
  }
  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if ((e.key === 'Delete' || e.key === 'Backspace') && selected !== null && !(e.target instanceof HTMLInputElement)) {
      e.preventDefault()
      removeNode(selected)
    }
  }

  const wireColor = (from: number, fromPort: number) => {
    const node = graph.nodes.find((n) => n.id === from)
    const ty = node ? specs.get(node.kind)?.outputs[fromPort]?.ty : undefined
    return ty ? TYPE_COLOR[ty] : TYPE_COLOR.Dynamic
  }

  const filtered = search.trim() ? catalog.filter((s) => shaderNodeTitle(s.kind).toLowerCase().includes(search.trim().toLowerCase())) : catalog

  const num = 'w-full min-w-0 rounded bg-overlay/10 px-1 py-0.5 text-[10px] tabular-nums text-ink outline-none focus:ring-1 focus:ring-accent'

  /** The inline editor for an unwired input (or a constant node's value). */
  const valueEditor = (ty: ShaderPortType, value: Vec4, set: (v: Vec4) => void) => {
    const n = (i: number) => (
      <input
        key={i}
        type="number"
        step={0.05}
        value={Number(value[i].toFixed(3))}
        onPointerDown={(e) => e.stopPropagation()}
        onChange={(e) => {
          const v = [...value] as Vec4
          v[i] = Number(e.target.value) || 0
          if (ty === 'Float' || ty === 'Dynamic') v.fill(v[i])
          set(v)
        }}
        className={num}
      />
    )
    if (ty === 'Vec4' || ty === 'Vec3')
      return (
        <div className="flex min-w-0 flex-1 items-center gap-1" onPointerDown={(e) => e.stopPropagation()}>
          <input type="color" value={linearToHex(value)} onChange={(e) => set(hexToLinear(e.target.value, value[3]))} className="h-4 w-6 shrink-0 cursor-pointer rounded border-0 bg-transparent p-0" />
          {ty === 'Vec4' && (
            <input
              type="number"
              step={0.05}
              min={0}
              max={1}
              title="Alpha"
              value={Number(value[3].toFixed(3))}
              onChange={(e) => set([value[0], value[1], value[2], Number(e.target.value) || 0])}
              className={num}
            />
          )}
        </div>
      )
    if (ty === 'Vec2') return <div className="flex min-w-0 flex-1 gap-0.5">{[n(0), n(1)]}</div>
    return <div className="flex min-w-0 flex-1">{n(0)}</div>
  }

  return (
    <div className="flex h-full min-h-0 gap-1.5">
      {/* Palette */}
      <div className="flex w-40 shrink-0 flex-col gap-1.5 overflow-hidden rounded-md border border-overlay/10 bg-overlay/5 p-1.5">
        <input
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search nodes…"
          className="w-full rounded bg-overlay/10 px-1.5 py-1 text-[11px] text-ink outline-none focus:ring-1 focus:ring-accent"
        />
        <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto">
          {categories.map((category) => {
            const items = filtered.filter((s) => s.category === category)
            if (!items.length) return null
            return (
              <div key={category} className="flex flex-col gap-0.5">
                <span className="px-0.5 text-[10px] uppercase tracking-wide text-ink-dim/70">{category}</span>
                {items.map((s) => (
                  <button
                    key={s.kind}
                    type="button"
                    disabled={s.kind === 'Output' && graph.nodes.some((n) => n.kind === 'Output')}
                    onClick={() => addNode(s.kind)}
                    className="rounded px-2 py-0.5 text-left text-[11px] text-ink hover:bg-accent/20 hover:text-accent disabled:opacity-35"
                  >
                    {shaderNodeTitle(s.kind)}
                  </button>
                ))}
              </div>
            )
          })}
        </div>
      </div>

      {/* Canvas */}
      <div
        ref={viewportRef}
        tabIndex={0}
        onKeyDown={onKeyDown}
        onPointerDown={onViewportDown}
        onPointerMove={onMove}
        onPointerUp={onUp}
        onWheel={onWheel}
        className={`relative min-h-0 flex-1 overflow-hidden rounded-md border border-overlay/10 outline-none ${dragging?.kind === 'pan' ? 'cursor-grabbing' : 'cursor-grab'}`}
        style={{
          // Theme tokens (not fixed dark colours) so the canvas reads in
          // light mode too: a faint overlay wash with overlay-coloured dots.
          backgroundColor: 'rgb(var(--overlay-rgb) / 0.06)',
          backgroundImage: 'radial-gradient(rgb(var(--overlay-rgb) / 0.12) 1px, transparent 1px)',
          backgroundSize: `${22 * zoom}px ${22 * zoom}px`,
          backgroundPosition: `${pan.x}px ${pan.y}px`,
        }}
      >
        <svg className="pointer-events-none absolute inset-0 h-full w-full">
          {graph.edges.map((edge, i) => {
            const a = outputPoint(edge.from, edge.fromPort)
            const b = inputPoint(edge.to, edge.toPort)
            if (!a || !b) return null
            const mid = Math.max(30, Math.abs(b.x - a.x) / 2)
            return <path key={i} d={`M ${a.x} ${a.y} C ${a.x + mid} ${a.y}, ${b.x - mid} ${b.y}, ${b.x} ${b.y}`} fill="none" stroke={wireColor(edge.from, edge.fromPort)} strokeOpacity={0.8} strokeWidth={2} />
          })}
          {dragging?.kind === 'wire' &&
            (() => {
              const a = outputPoint(dragging.from, dragging.fromPort)
              if (!a) return null
              const mid = Math.max(30, Math.abs(dragging.x - a.x) / 2)
              return <path d={`M ${a.x} ${a.y} C ${a.x + mid} ${a.y}, ${dragging.x - mid} ${dragging.y}, ${dragging.x} ${dragging.y}`} fill="none" stroke={wireColor(dragging.from, dragging.fromPort)} strokeWidth={2} strokeDasharray="5 4" />
            })()}
        </svg>

        {graph.nodes.map((node) => {
          const { spec, inputs, outputs, valueRows, rows } = layout(node)
          const pos = screenPos(node)
          const isError = errorNode === node.id
          const isSelected = selected === node.id
          return (
            <div
              key={node.id}
              className={`absolute select-none rounded-md border bg-solid-panel shadow-lg ${isError ? 'border-red-500' : isSelected ? 'border-accent' : 'border-overlay/15'}`}
              style={{ left: pos.x, top: pos.y, width: NODE_WIDTH, transformOrigin: 'top left', transform: `scale(${zoom})` }}
            >
              <div
                onPointerDown={(e) => onHeaderDown(e, node)}
                className="flex cursor-grab items-center justify-between rounded-t-md px-2"
                style={{ height: HEADER_HEIGHT, background: CATEGORY_COLOR[spec?.category ?? ''] ?? 'rgba(255,255,255,0.06)' }}
              >
                <span className="truncate text-[11px] font-medium text-ink">{shaderNodeTitle(node.kind)}</span>
                <button onPointerDown={(e) => e.stopPropagation()} onClick={() => removeNode(node.id)} title="Delete node" className="text-[11px] text-ink-dim hover:text-red-400">
                  ✕
                </button>
              </div>

              {valueRows > 0 && (
                <div className="flex items-center px-2" style={{ height: VALUE_ROW_HEIGHT }}>
                  {valueEditor(node.kind === 'ColorConstant' ? 'Vec4' : node.kind === 'Vec2Constant' ? 'Vec2' : 'Float', node.value, (value) => updateNode(node.id, { value }))}
                </div>
              )}

              {Array.from({ length: rows }).map((_, row) => {
                const input = inputs[row]
                const output = outputs[row]
                const wired = input ? graph.edges.some((e) => e.to === node.id && e.toPort === row) : false
                const value = (node.defaults[row] ?? input?.default ?? [0, 0, 0, 0]) as Vec4
                return (
                  <div key={row} className="flex items-center gap-1 px-1" style={{ height: ROW_HEIGHT }}>
                    {input ? (
                      <div className="flex min-w-0 flex-1 items-center gap-1">
                        <div
                          data-shader-input
                          data-node={node.id}
                          data-port={row}
                          title={`${input.name} (${input.ty})`}
                          className="h-2.5 w-2.5 shrink-0 rounded-full border border-overlay/40"
                          style={{ background: wired ? TYPE_COLOR[input.ty] : 'transparent', boxShadow: `inset 0 0 0 1.5px ${TYPE_COLOR[input.ty]}` }}
                        />
                        <span className="w-12 shrink-0 truncate text-[10px] text-ink-dim">{input.name}</span>
                        {wired ? (
                          <button onPointerDown={(e) => e.stopPropagation()} onClick={() => setWire(node.id, row, null)} title="Disconnect" className="text-[9px] text-ink-dim hover:text-red-400">
                            ✕
                          </button>
                        ) : input.default === null ? (
                          <span className="text-[10px] italic text-ink-dim/60">sprite UV</span>
                        ) : (
                          valueEditor(input.ty, value, (v) => setDefault(node, row, v))
                        )}
                      </div>
                    ) : (
                      <span className="flex-1" />
                    )}
                    {output && (
                      <div className="flex shrink-0 items-center gap-1">
                        <span className="text-[10px] text-ink-dim">{output.name}</span>
                        <div
                          onPointerDown={(e) => onOutputDown(e, node.id, row)}
                          title={`${output.name} (${output.ty})`}
                          className="h-2.5 w-2.5 cursor-crosshair rounded-full border border-overlay/40"
                          style={{ background: TYPE_COLOR[output.ty] }}
                        />
                      </div>
                    )}
                  </div>
                )
              })}
            </div>
          )
        })}

        <div onPointerDown={(e) => e.stopPropagation()} className="absolute bottom-2 right-2 flex items-center gap-1 rounded-md border border-overlay/10 bg-solid-panel px-1 py-0.5">
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
      </div>
    </div>
  )
}
