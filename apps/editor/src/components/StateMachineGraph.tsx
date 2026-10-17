import { Select } from './Select'
import { useRef, useState } from 'react'
import type { ArtboardSummary, Blend1DDef, ConditionDef, LoopMode, PlaybackDirection, ProgramDef, StateMachineDefSummary, StateMachineSummary } from '../ipc/document'

// Every box is this exact size, always — no per-state size change on
// select/deselect. An earlier revision expanded a box only once selected
// (to reveal its editable fields), which read as the box "jumping" on
// every click; every box now always shows its own animation/loop-mode
// fields inline, so there's nothing left to expand.
const BOX_WIDTH = 190
const BOX_HEIGHT = 118
const BOX_HALF = { w: BOX_WIDTH / 2, h: BOX_HEIGHT / 2 }

// Same clamp/step shape `App.tsx`'s own `camera.zoom` uses for the main
// canvas — not the same *value*, since a node graph's own sensible zoom
// range doesn't need to match a vector-artboard camera's.
const MIN_ZOOM = 0.25
const MAX_ZOOM = 3
const ZOOM_STEP = 1.1

type Dragging =
  | { kind: 'pan'; startClientX: number; startClientY: number; startPan: { x: number; y: number } }
  | { kind: 'node'; name: string; startClientX: number; startClientY: number; origin: { x: number; y: number }; moved: boolean }
  | { kind: 'connect'; from: string; x: number; y: number }

/** A small helper — the point on a box's edge closest to `toward`, so a
 * transition arrow starts/ends at a box's border rather than drawing
 * straight through its fill. Approximates the box as its bounding rect and
 * clips the center-to-center line against it (good enough at this box
 * size; a tighter per-corner clip isn't worth the complexity). `half` is
 * `BOX_HALF` scaled by the graph's current zoom — callers pass the *screen-
 * space* half-extent, since `center`/`toward` are themselves screen
 * coordinates (`boxScreenPosition`'s output), not authored positions. */
function edgePoint(center: { x: number; y: number }, toward: { x: number; y: number }, half: { w: number; h: number }): { x: number; y: number } {
  const dx = toward.x - center.x
  const dy = toward.y - center.y
  if (dx === 0 && dy === 0) return center
  const scale = 1 / Math.max(Math.abs(dx) / half.w, Math.abs(dy) / half.h)
  return { x: center.x + dx * scale, y: center.y + dy * scale }
}

const CONTROL = 'w-full min-w-0 rounded-md bg-overlay/8 px-2 py-1 text-xs text-ink outline-none focus:bg-overlay/12'

/**
 * The State Machine panel's node-graph canvas — states as draggable boxes,
 * transitions as arrows you drag out from one box's connect handle to
 * another, matching how Rive/Unity's own state-machine editors look (see
 * `StateMachinePanel.tsx`'s module doc for why the previous dropdown-list
 * form of this UI wasn't "practical," per the request that produced this
 * component — and why this now renders bottom-docked full width instead of
 * squeezed into the right sidebar, per the follow-up "move the state
 * machine editor panel down").
 *
 * Every box always shows its own editable fields (animation/loop mode, Set
 * Initial/Delete) directly inside the box at its own canvas position —
 * seamlessly, with no click-to-expand step and no size change on select —
 * the "I should be able to edit state node details in place" request this
 * revision addresses. Selecting a box (click, or release after a drag) only
 * changes its border highlight and which state's transitions the side
 * column shows; it never resizes anything. Outgoing transitions still live
 * in that side column next to the canvas, not inside the box itself — a box
 * has room for two dropdowns, not an open-ended list of per-transition
 * condition editors.
 *
 * Positions (`state.position`) are authored, saved document content —
 * dragging a box calls `onSetStatePosition` once on release, an undoable
 * edit like any other (see `editor.rs`'s `set_state_position` doc comment
 * for why there's no live-preview variant: nothing on the WGPU canvas reads
 * a state's position, so there's no render to keep in sync mid-drag).
 * `pan` (scrolling the canvas itself) is deliberately *not* saved — it's
 * pure viewport convenience, the same "session state, not document state"
 * split `App.tsx`'s `camera` already draws for the main artboard canvas.
 */
export function StateMachineGraph({
  artboard,
  activeMachine,
  summary,
  selectedLayer,
  onSelectLayer,
  onAddLayer,
  onDeleteLayer,
  onSetLayerInitialState,
  onAddState,
  onUpdateState,
  onDeleteState,
  onSetStatePosition,
  onSetInitialState,
  onAddTransition,
  onSetTransition,
  onRemoveTransition,
  onSetStateBlend,
  onSetStateProgram,
  onEnterProgram,
}: {
  artboard: ArtboardSummary
  activeMachine: StateMachineDefSummary
  summary: StateMachineSummary | null
  /** `null` for the base (top-level) layer, or one of `activeMachine.layers`'
   * own names — which layer's states/transitions this graph currently shows
   * and edits. Owned by `App.tsx` (not local state) since `LayersPanel`'s
   * mask checkboxes need to know the same selection. */
  selectedLayer: string | null
  onSelectLayer: (layer: string | null) => void
  onAddLayer: (name: string, animation: { idx: number; version: number }) => void
  onDeleteLayer: (name: string) => void
  onSetLayerInitialState: (layerName: string, value: string) => void
  onAddState: (name: string, animation: { idx: number; version: number }, loopMode: LoopMode, layer: string | null) => void
  onUpdateState: (name: string, animation: { idx: number; version: number }, loopMode: LoopMode, layer: string | null) => void
  onDeleteState: (name: string, layer: string | null) => void
  onSetStatePosition: (name: string, x: number, y: number, layer: string | null) => void
  onSetInitialState: (name: string) => void
  onAddTransition: (from: string, to: string, condition: ConditionDef, direction: PlaybackDirection, layer: string | null) => void
  onSetTransition: (stateName: string, index: number, condition: ConditionDef, direction: PlaybackDirection, durationSeconds: number | undefined, interruptible: boolean | undefined, layer: string | null) => void
  onRemoveTransition: (stateName: string, index: number, layer: string | null) => void
  onSetStateBlend: (stateName: string, blend: Blend1DDef | null, layer: string | null) => void
  onSetStateProgram: (stateName: string, program: ProgramDef | null, layer: string | null) => void
  /** Double-clicking a Programmable State's box "looks into" it — see
   * `ProgramEditorPanel`'s own doc comment for the drilled-in view this
   * opens. No-op on a plain (non-programmable) state's box; nothing else
   * about double-clicking a box is meaningful otherwise. */
  onEnterProgram: (stateName: string, layer: string | null) => void
}) {
  const viewportRef = useRef<HTMLDivElement>(null)
  const [pan, setPan] = useState({ x: 40, y: 40 })
  // This graph's own zoom — independent of `App.tsx`'s `camera.zoom` for the
  // main artboard canvas underneath. The two used to be conflated: scrolling
  // anywhere over this dock (even directly over the graph) zoomed the
  // *background canvas* instead, because a wheel event here is a sibling of
  // the canvas DOM-wise and had to be forwarded — see the git history around
  // `App.tsx`'s removed `handleDockWheel` for that dead end. A graph editor
  // zooming its own node layout, independent of whatever artboard camera
  // happens to be behind it, is the actual convention every reference here
  // (Rive/Unity's own state machine editors) uses, not shared-camera zoom.
  const [zoom, setZoom] = useState(1)
  const [dragging, setDragging] = useState<Dragging | null>(null)
  // Latest pointer position (screen coords) while any drag is in progress —
  // separate from `dragging` itself since it updates every pointermove
  // while `dragging`'s own fields (origin/startClient*) stay fixed for the
  // gesture's duration.
  const [dragCurrent, setDragCurrent] = useState<{ x: number; y: number } | null>(null)
  const [selectedState, setSelectedState] = useState<string | null>(null)

  // Which layer's own states/transitions this graph shows/edits right now
  // — `selectedLayer` (a prop, owned by `App.tsx`) is `null` for the base
  // (top-level) layer, otherwise names one of `activeMachine.layers`.
  const currentLayer = selectedLayer ? activeMachine.layers.find((l) => l.name === selectedLayer) : undefined
  const currentStates = selectedLayer ? (currentLayer?.states ?? []) : activeMachine.states
  const currentInitialState = selectedLayer ? currentLayer?.initialState : activeMachine.initialState

  const selected = currentStates.find((s) => s.name === selectedState) ?? null

  // `pan` is added *after* scaling `state.position` by `zoom` — `pan` itself
  // lives in screen-pixel space (a raw click-drag delta, see
  // `handlePointerMove`'s `pan` branch), while `state.position` is authored,
  // zoom-independent document content. This is the one place that
  // composes the two into an actual screen coordinate; every other box-
  // geometry value below (`BOX_WIDTH`/`BOX_HEIGHT`/`BOX_HALF`) also needs its
  // own `* zoom` wherever it's used as a screen-space size, not just this
  // position.
  const boxScreenPosition = (state: (typeof activeMachine.states)[number]) => ({
    x: pan.x + state.position.x * zoom,
    y: pan.y + state.position.y * zoom,
  })

  // Live drag offset applied on top of `boxScreenPosition` while a node-drag
  // is in progress — kept separate from `boxScreenPosition` so committing
  // the position on release (`onSetStatePosition`) only has to add one delta
  // rather than unwind an already-mutated local copy.
  const liveOffsetFor = (name: string) => {
    if (dragging?.kind !== 'node' || dragging.name !== name || !dragCurrent) return { x: 0, y: 0 }
    return { x: dragCurrent.x - dragging.startClientX, y: dragCurrent.y - dragging.startClientY }
  }

  const toLocal = (clientX: number, clientY: number) => {
    const rect = viewportRef.current?.getBoundingClientRect()
    return { x: clientX - (rect?.left ?? 0), y: clientY - (rect?.top ?? 0) }
  }

  const handleViewportPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return
    ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
    setDragging({ kind: 'pan', startClientX: e.clientX, startClientY: e.clientY, startPan: pan })
    setDragCurrent({ x: e.clientX, y: e.clientY })
  }

  const handleBoxPointerDown = (e: React.PointerEvent<HTMLDivElement>, state: (typeof activeMachine.states)[number]) => {
    e.stopPropagation()
    ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
    setDragging({ kind: 'node', name: state.name, startClientX: e.clientX, startClientY: e.clientY, origin: state.position, moved: false })
    setDragCurrent({ x: e.clientX, y: e.clientY })
  }

  const handleConnectPointerDown = (e: React.PointerEvent<HTMLDivElement>, state: (typeof activeMachine.states)[number]) => {
    e.stopPropagation()
    ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
    const local = toLocal(e.clientX, e.clientY)
    setDragging({ kind: 'connect', from: state.name, x: local.x, y: local.y })
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
    } else if (dragging.kind === 'connect') {
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
        // `dx`/`dy` are a screen-pixel delta; `state.position` (what
        // `onSetStatePosition` writes) is authored, zoom-independent
        // content — dividing by `zoom` converts "moved 100 screen px while
        // zoomed in 2x" into "moved 50 authored units", so a box doesn't
        // fly further than the cursor actually dragged it once zoom isn't 1.
        onSetStatePosition(dragging.name, dragging.origin.x + dx / zoom, dragging.origin.y + dy / zoom, selectedLayer)
      }
      setSelectedState(dragging.name)
    } else if (dragging.kind === 'connect') {
      const target = document.elementFromPoint(e.clientX, e.clientY)?.closest('[data-state-name]') as HTMLElement | null
      const targetName = target?.dataset.stateName
      if (targetName) {
        onAddTransition(dragging.from, targetName, { kind: 'Trigger', input: '' }, 'Forward', selectedLayer)
        setSelectedState(dragging.from)
      }
    }
    setDragging(null)
    setDragCurrent(null)
  }

  // Wheel-zooms this graph toward the cursor — a plain scroll (no ctrlKey
  // check, unlike the main canvas's own wheel handler) since this viewport
  // has no competing "plain scroll = pan" gesture of its own (panning here
  // is click-drag only, `handleViewportPointerDown`), matching how every
  // node-graph editor this component's own doc comment cites (Rive/Unity)
  // zooms on a bare scroll. Solves for the `pan` that keeps the world point
  // under the cursor fixed across the zoom change, same "zoom toward
  // cursor" math `App.tsx`'s `zoomAtCanvasPoint` uses for the main canvas,
  // just against this component's own `pan`/`zoom` instead of the shared
  // camera.
  const handleWheel = (e: React.WheelEvent<HTMLDivElement>) => {
    e.preventDefault()
    const local = toLocal(e.clientX, e.clientY)
    const factor = e.deltaY > 0 ? 1 / ZOOM_STEP : ZOOM_STEP
    const newZoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom * factor))
    setPan({
      x: local.x - ((local.x - pan.x) / zoom) * newZoom,
      y: local.y - ((local.y - pan.y) / zoom) * newZoom,
    })
    setZoom(newZoom)
  }

  const handleAddState = () => {
    const animation = artboard.activeAnimation ?? artboard.animations[0]?.id
    if (!animation) return
    // `currentStates.length + 1` alone isn't unique once any state has ever
    // been deleted — e.g. two states, delete the first, add one: naively
    // that's `State 2` again, colliding with the surviving state of the same
    // name. Every name-keyed lookup (initial-state comparison/dropdown,
    // transition targets, delete-by-name) then can't tell the two apart, so
    // this counts up past however many `State N` names already exist instead
    // of just using the current count.
    const existingNames = new Set(currentStates.map((s) => s.name))
    let n = currentStates.length + 1
    while (existingNames.has(`State ${n}`)) n++
    onAddState(`State ${n}`, animation, 'Loop', selectedLayer)
  }

  // `BOX_HALF` scaled to screen space — every place that needs a box's
  // *visual* half-extent (edge-clipping transition arrows, the self-
  // transition loop's offset) uses this instead of the raw constant, same
  // reasoning `boxScreenPosition` already applies to a box's position.
  const scaledHalf = { w: BOX_HALF.w * zoom, h: BOX_HALF.h * zoom }

  const [addingLayer, setAddingLayer] = useState(false)
  const [newLayerName, setNewLayerName] = useState('')

  const handleCreateLayer = () => {
    const name = newLayerName.trim()
    const animation = artboard.activeAnimation ?? artboard.animations[0]?.id
    if (name && animation) {
      onAddLayer(name, animation)
      onSelectLayer(name)
    }
    setAddingLayer(false)
    setNewLayerName('')
  }

  return (
    <div className="flex h-full gap-2">
      <div className="flex min-w-0 flex-1 flex-col gap-1.5">
        {/* Layer switcher — "Base" is always the machine's own top-level
            `states` (every machine saved before layers existed); each extra
            layer is its own independent little machine, masked to a node
            subset picked via checkboxes in the Layers panel (see
            `App.tsx`'s `layerMaskEditing` wiring). Selecting a layer here
            swaps which one's states/transitions the graph below shows and
            edits — see `currentStates`'s own comment. */}
        <div className="flex flex-wrap items-center gap-1">
          <button
            onClick={() => onSelectLayer(null)}
            className={`rounded-full px-2.5 py-1 text-[11px] font-medium ${!selectedLayer ? 'bg-accent/20 text-accent' : 'bg-overlay/8 text-ink-dim hover:bg-overlay/12'}`}
          >
            Base
          </button>
          {activeMachine.layers.map((l) => (
            <button
              key={l.name}
              onClick={() => onSelectLayer(l.name)}
              className={`rounded-full px-2.5 py-1 text-[11px] font-medium ${selectedLayer === l.name ? 'bg-accent/20 text-accent' : 'bg-overlay/8 text-ink-dim hover:bg-overlay/12'}`}
            >
              {l.name}
            </button>
          ))}
          {addingLayer ? (
            <input
              autoFocus
              value={newLayerName}
              onChange={(e) => setNewLayerName(e.target.value)}
              onBlur={handleCreateLayer}
              onKeyDown={(e) => {
                if (e.key === 'Enter') e.currentTarget.blur()
                if (e.key === 'Escape') {
                  setAddingLayer(false)
                  setNewLayerName('')
                }
              }}
              placeholder="Layer name"
              className="w-24 rounded-full bg-overlay/12 px-2.5 py-1 text-[11px] text-ink outline-none ring-1 ring-accent"
            />
          ) : (
            <button onClick={() => setAddingLayer(true)} className="rounded-full bg-overlay/8 px-2.5 py-1 text-[11px] font-medium text-ink-dim hover:bg-overlay/12">
              + Layer
            </button>
          )}
          {selectedLayer && (
            <button
              onClick={() => {
                onDeleteLayer(selectedLayer)
                onSelectLayer(null)
              }}
              className="rounded-full bg-red-500/10 px-2.5 py-1 text-[11px] font-medium text-red-300 hover:bg-red-500/20"
            >
              Delete Layer
            </button>
          )}
        </div>

        <div className="flex items-center justify-between">
          <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">Graph</span>
          <button onClick={handleAddState} className="rounded-md bg-accent/15 px-2 py-1 text-xs font-medium text-accent hover:bg-accent/25">
            + Add State
          </button>
        </div>

        <div
          ref={viewportRef}
          onPointerDown={handleViewportPointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          onWheel={handleWheel}
          className={`relative min-h-0 flex-1 overflow-hidden rounded-md border border-overlay/10 bg-overlay/5 ${dragging?.kind === 'pan' ? 'cursor-grabbing' : 'cursor-grab'}`}
        >
          <svg className="pointer-events-none absolute inset-0 h-full w-full">
            <defs>
              <marker id="sm-arrow" markerWidth="8" markerHeight="8" refX="6" refY="4" orient="auto">
                <path d="M0,0 L8,4 L0,8 Z" fill="rgb(var(--overlay-rgb) / 0.4)" />
              </marker>
            </defs>
            {currentStates.flatMap((state) =>
              state.transitions.map((transition, index) => {
                const fromCenter = { x: boxScreenPosition(state).x + liveOffsetFor(state.name).x, y: boxScreenPosition(state).y + liveOffsetFor(state.name).y }
                const targetState = currentStates.find((s) => s.name === transition.to)
                if (!targetState) return null
                const toCenter = {
                  x: boxScreenPosition(targetState).x + liveOffsetFor(targetState.name).x,
                  y: boxScreenPosition(targetState).y + liveOffsetFor(targetState.name).y,
                }
                if (transition.to === state.name) {
                  // Self-transition — a small loop above the box rather than a
                  // degenerate zero-length line.
                  const cx = fromCenter.x
                  const cy = fromCenter.y - scaledHalf.h
                  return (
                    <path
                      key={`${state.name}-${index}`}
                      d={`M ${cx - 14} ${cy} C ${cx - 24} ${cy - 28}, ${cx + 24} ${cy - 28}, ${cx + 14} ${cy}`}
                      fill="none"
                      stroke="rgb(var(--overlay-rgb) / 0.4)"
                      strokeWidth={1.5}
                      markerEnd="url(#sm-arrow)"
                    />
                  )
                }
                const start = edgePoint(fromCenter, toCenter, scaledHalf)
                const end = edgePoint(toCenter, fromCenter, scaledHalf)
                return <line key={`${state.name}-${index}`} x1={start.x} y1={start.y} x2={end.x} y2={end.y} stroke="rgb(var(--overlay-rgb) / 0.4)" strokeWidth={1.5} markerEnd="url(#sm-arrow)" />
              }),
            )}
            {dragging?.kind === 'connect' &&
              (() => {
                const source = currentStates.find((s) => s.name === dragging.from)
                if (!source) return null
                const from = boxScreenPosition(source)
                return <line x1={from.x} y1={from.y} x2={dragging.x} y2={dragging.y} stroke="rgba(122,162,255,0.8)" strokeWidth={1.5} strokeDasharray="4 3" />
              })()}
          </svg>

          {currentStates.map((state) => {
            const base = boxScreenPosition(state)
            const offset = liveOffsetFor(state.name)
            const isActive = selectedLayer ? state.name === summary?.layerActiveStates[selectedLayer] : state.name === summary?.activeState
            const isInitial = state.name === currentInitialState
            const isSelected = state.name === selectedState
            const left = base.x + offset.x - BOX_WIDTH / 2
            const top = base.y + offset.y - BOX_HEIGHT / 2
            return (
              <div
                key={state.name}
                data-state-name={state.name}
                onPointerDown={(e) => handleBoxPointerDown(e, state)}
                onClick={() => setSelectedState(state.name)}
                onDoubleClick={() => state.program && onEnterProgram(state.name, selectedLayer)}
                // `left`/`top` place this box's *unscaled* `BOX_WIDTH`x
                // `BOX_HEIGHT` bounding box centered exactly on
                // `boxScreenPosition`'s already-zoomed screen point — a CSS
                // `scale()` transform (which scales visually around the
                // element's own center, without touching its layout box)
                // then grows/shrinks the whole thing, content included
                // (`InlineStateFields`' dropdowns/buttons scale right along
                // with it, still correctly clickable — CSS transforms don't
                // break pointer hit-testing), so this stays in sync with
                // `scaledHalf`'s own screen-space half-extent used for edge
                // clipping above.
                style={{ left, top, width: BOX_WIDTH, height: BOX_HEIGHT, transform: `scale(${zoom})` }}
                // `isSelected` gets a dashed, offset outline *in addition to*
                // whichever border/background the active/inactive state
                // already has, rather than its own subtly-different solid
                // border color — "selected" was unclear at a glance before
                // (a slightly lighter border blended in against the
                // dark canvas), and this stays a clear, distinct signal
                // whether or not the selected box also happens to be active.
                className={`absolute flex flex-col gap-1 rounded-md border px-2 py-1.5 text-xs shadow-md select-none ${
                  isActive ? 'border-accent bg-solid-panel text-accent' : 'border-overlay/15 bg-solid-panel text-ink'
                } ${isSelected ? 'outline outline-2 outline-dashed outline-offset-2 outline-accent' : ''}`}
              >
                <InlineStateFields
                  artboard={artboard}
                  state={state}
                  isInitial={isInitial}
                  canDelete={currentStates.length > 1}
                  onSetInitial={() => (selectedLayer ? onSetLayerInitialState(selectedLayer, state.name) : onSetInitialState(state.name))}
                  onDelete={() => {
                    onDeleteState(state.name, selectedLayer)
                    if (selectedState === state.name) setSelectedState(null)
                  }}
                  onUpdate={(animation, loopMode) => onUpdateState(state.name, animation, loopMode, selectedLayer)}
                />
                <div
                  onPointerDown={(e) => handleConnectPointerDown(e, state)}
                  title="Drag to another state to add a transition"
                  className="absolute -bottom-1.5 -right-1.5 h-3 w-3 cursor-crosshair rounded-full border border-overlay/30 bg-canvas hover:bg-accent"
                />
                {state.program && (
                  <div
                    title="Programmable State — double-click to edit its program"
                    className="absolute -top-1.5 -left-1.5 flex h-3.5 w-3.5 items-center justify-center rounded-full border border-overlay/30 bg-canvas text-[8px] text-accent"
                  >
                    ▸
                  </div>
                )}
              </div>
            )
          })}

          {/* Manual zoom widget for this graph's own view — the corner
              control this component's doc comment describes, independent of
              `App.tsx`'s main-canvas zoom widget. `onPointerDown`'s
              `stopPropagation` keeps a click here from also being read as
              `handleViewportPointerDown`'s "empty-space click starts a pan"
              gesture (that check is `e.target !== e.currentTarget`, so a
              child element is already safe from it, but a `pointerdown`
              still bubbles up and could interact with drag state otherwise). */}
          <div
            onPointerDown={(e) => e.stopPropagation()}
            className="pointer-events-auto absolute bottom-2 right-2 flex items-center gap-1 rounded-md border border-overlay/10 bg-black/50 px-1 py-0.5"
          >
            <button
              onClick={() => setZoom((z) => Math.max(MIN_ZOOM, z / ZOOM_STEP))}
              title="Zoom out"
              className="rounded px-1.5 py-0.5 text-xs text-ink-dim hover:bg-overlay/10 hover:text-ink"
            >
              −
            </button>
            <button
              onClick={() => setZoom(1)}
              title="Reset zoom to 100%"
              className="w-10 rounded px-1 py-0.5 text-center text-[11px] tabular-nums text-ink-dim hover:bg-overlay/10 hover:text-ink"
            >
              {Math.round(zoom * 100)}%
            </button>
            <button
              onClick={() => setZoom((z) => Math.min(MAX_ZOOM, z * ZOOM_STEP))}
              title="Zoom in"
              className="rounded px-1.5 py-0.5 text-xs text-ink-dim hover:bg-overlay/10 hover:text-ink"
            >
              +
            </button>
          </div>
        </div>
      </div>

      {selected && (
        <div className="flex w-56 shrink-0 flex-col gap-1.5 overflow-y-auto">
          <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">
            {selected.name} &mdash; Outgoing Transitions
          </span>
          {selected.transitions.length === 0 && <p className="text-[11px] text-ink-dim">None — drag from this box's handle to another state.</p>}
          {selected.transitions.map((transition, index) => (
            <TransitionRow
              key={index}
              transition={transition}
              onChange={(condition, direction, durationSeconds, interruptible) => onSetTransition(selected.name, index, condition, direction, durationSeconds, interruptible, selectedLayer)}
              onRemove={() => onRemoveTransition(selected.name, index, selectedLayer)}
            />
          ))}
          <span className="mt-2 text-[11px] font-medium uppercase tracking-wide text-ink-dim">Dynamic Expression</span>
          <BlendFields artboard={artboard} state={selected} onChange={(blend) => onSetStateBlend(selected.name, blend, selectedLayer)} />
          <span className="mt-2 text-[11px] font-medium uppercase tracking-wide text-ink-dim">Program</span>
          <ProgramFields state={selected} onChange={(program) => onSetStateProgram(selected.name, program, selectedLayer)} />
        </div>
      )}
    </div>
  )
}

/** Every box's own always-visible, always-editable fields — a name header
 * with Set Initial/Delete buttons, plus animation/loop-mode dropdowns —
 * rendered at every state's own canvas position regardless of selection
 * (see `StateMachineGraph`'s doc comment for why this isn't conditional on
 * being selected: a box that only reveals these on click, then resizes to
 * fit them, reads as the whole graph "jumping" on every click). */
function InlineStateFields({
  artboard,
  state,
  isInitial,
  canDelete,
  onSetInitial,
  onDelete,
  onUpdate,
}: {
  artboard: ArtboardSummary
  state: StateMachineDefSummary['states'][number]
  isInitial: boolean
  canDelete: boolean
  onSetInitial: () => void
  onDelete: () => void
  onUpdate: (animation: { idx: number; version: number }, loopMode: LoopMode) => void
}) {
  return (
    <>
      <div className="flex items-center justify-between gap-1">
        {/* No `stopPropagation` here — the plain name text (and any blank
            space in this row) deliberately falls through to the box's own
            `onPointerDown` (`handleBoxPointerDown`) so a drag can start from
            the title, not just from empty space below the fields. Only the
            two actual buttons stop it (each individually, below), so
            clicking them doesn't *also* start a box-drag. */}
        <span className="truncate text-xs font-semibold text-ink">{state.name}</span>
        <div className="flex shrink-0 gap-1">
          <button
            onPointerDown={(e) => e.stopPropagation()}
            onClick={onSetInitial}
            disabled={isInitial}
            className="rounded bg-overlay/8 px-1.5 py-0.5 text-[10px] text-ink-dim hover:bg-overlay/15 disabled:opacity-40"
          >
            {isInitial ? '★' : 'Set Initial'}
          </button>
          <button
            onPointerDown={(e) => e.stopPropagation()}
            onClick={onDelete}
            disabled={!canDelete}
            className="rounded bg-red-500/10 px-1.5 py-0.5 text-[10px] text-red-300 hover:bg-red-500/20 disabled:opacity-40"
          >
            Delete
          </button>
        </div>
      </div>
      {state.program ? (
        // A Programmable State's `animation`/`loopMode` are pure unused
        // filler (`an1mbyt3_core::StateDef::animation`'s own doc comment) —
        // showing the dropdowns anyway reads as "these still do something,"
        // which is exactly the confusion double-click-to-edit-the-program
        // should resolve instead, so this replaces them with a plain note.
        <p className="rounded-md bg-overlay/5 px-2 py-1 text-[10px] text-ink-dim">Driven by its program — double-click to edit.</p>
      ) : (
        <>
          <Select
            className={CONTROL}
            onPointerDown={(e) => e.stopPropagation()}
            value={`${state.animation.idx}:${state.animation.version}`}
            onChange={(e) => {
              const found = artboard.animations.find((a) => `${a.id.idx}:${a.id.version}` === e.target.value)
              if (found) onUpdate(found.id, state.loopMode)
            }}
          >
            {!artboard.animations.some((a) => a.id.idx === state.animation.idx && a.id.version === state.animation.version) && (
              <option value={`${state.animation.idx}:${state.animation.version}`}>⚠ Deleted animation — pick one</option>
            )}
            {artboard.animations.map((a) => (
              <option key={`${a.id.idx}:${a.id.version}`} value={`${a.id.idx}:${a.id.version}`}>
                {a.name}
              </option>
            ))}
          </Select>
          <Select className={CONTROL} onPointerDown={(e) => e.stopPropagation()} value={state.loopMode} onChange={(e) => onUpdate(state.animation, e.target.value as LoopMode)}>
            <option value="Once">Once</option>
            <option value="Loop">Loop</option>
            <option value="PingPong">PingPong</option>
          </Select>
        </>
      )}
    </>
  )
}

function TransitionRow({
  transition,
  onChange,
  onRemove,
}: {
  transition: { to: string; condition: ConditionDef; direction: PlaybackDirection; durationSeconds: number; interruptible: boolean }
  onChange: (condition: ConditionDef, direction: PlaybackDirection, durationSeconds?: number, interruptible?: boolean) => void
  onRemove: () => void
}) {
  const kind = transition.condition.kind
  const input = 'input' in transition.condition ? transition.condition.input : ''
  const value = 'value' in transition.condition ? String(transition.condition.value) : 'threshold' in transition.condition ? String(transition.condition.threshold) : ''

  const rebuild = (patch: { kind?: ConditionDef['kind']; input?: string; value?: string; direction?: PlaybackDirection }) => {
    const nextKind = patch.kind ?? kind
    const nextInput = patch.input ?? input
    const nextValue = patch.value ?? value
    const nextDirection = patch.direction ?? transition.direction
    let condition: ConditionDef
    switch (nextKind) {
      case 'BoolIs':
        condition = { kind: 'BoolIs', input: nextInput, value: nextValue === 'true' }
        break
      case 'NumberGreaterThan':
        condition = { kind: 'NumberGreaterThan', input: nextInput, threshold: Number(nextValue) || 0 }
        break
      case 'NumberLessThan':
        condition = { kind: 'NumberLessThan', input: nextInput, threshold: Number(nextValue) || 0 }
        break
      case 'EnumIs':
        condition = { kind: 'EnumIs', input: nextInput, value: nextValue }
        break
      case 'Trigger':
        condition = { kind: 'Trigger', input: nextInput }
        break
    }
    onChange(condition, nextDirection)
  }

  return (
    <div className="flex flex-col gap-1 rounded-md bg-black/20 p-1.5">
      <div className="flex items-center justify-between gap-1">
        <span className="truncate text-[11px] text-ink-dim">
          <span className="text-ink">&rarr; {transition.to}</span>
        </span>
        <button onClick={onRemove} className="shrink-0 rounded bg-red-500/10 px-1.5 py-0.5 text-[10px] text-red-300 hover:bg-red-500/20">
          Delete
        </button>
      </div>
      <Select className={CONTROL} value={kind} onChange={(e) => rebuild({ kind: e.target.value as ConditionDef['kind'] })}>
        <option value="Trigger">Trigger</option>
        <option value="BoolIs">Boolean is</option>
        <option value="NumberGreaterThan">Number &gt;</option>
        <option value="NumberLessThan">Number &lt;</option>
        <option value="EnumIs">Enum is</option>
      </Select>
      <input className={CONTROL} placeholder="input name" value={input} onChange={(e) => rebuild({ input: e.target.value })} />
      {kind === 'BoolIs' && (
        <label className="flex items-center justify-between gap-2 rounded-md bg-overlay/8 px-2 py-1 text-xs text-ink">
          <span>Required value</span>
          <input type="checkbox" checked={value === 'true'} onChange={(e) => rebuild({ value: String(e.target.checked) })} />
        </label>
      )}
      {(kind === 'NumberGreaterThan' || kind === 'NumberLessThan') && (
        <input
          type="number"
          className={CONTROL}
          placeholder="threshold"
          value={value}
          onChange={(e) => rebuild({ value: e.target.value })}
        />
      )}
      {kind === 'EnumIs' && (
        <input className={CONTROL} placeholder="enum value" value={value} onChange={(e) => rebuild({ value: e.target.value })} />
      )}
      <Select className={CONTROL} value={transition.direction} onChange={(e) => rebuild({ direction: e.target.value as PlaybackDirection })}>
        <option value="Forward">Forward</option>
        <option value="Reverse">Reverse</option>
      </Select>
      <label className="flex items-center justify-between gap-2 rounded-md bg-overlay/8 px-2 py-1 text-xs text-ink">
        <span>Crossfade (s)</span>
        <input
          type="number"
          min={0}
          step={0.05}
          className="w-16 rounded bg-overlay/10 px-1.5 py-0.5 text-right text-xs text-ink outline-none focus:bg-overlay/15"
          value={transition.durationSeconds}
          onChange={(e) => onChange(transition.condition, transition.direction, Math.max(0, Number(e.target.value) || 0))}
        />
      </label>
      <label className="flex items-center justify-between gap-2 rounded-md bg-overlay/8 px-2 py-1 text-xs text-ink" title="Rive's &quot;Allow Exit During Transition&quot; — while on, a different transition out of the target state can interrupt this one's crossfade before it finishes">
        <span>Allow exit during transition</span>
        <input
          type="checkbox"
          checked={transition.interruptible}
          onChange={(e) => onChange(transition.condition, transition.direction, undefined, e.target.checked)}
        />
      </label>
    </div>
  )
}

/** The selected state's "dynamic expression" (`Blend1DDef`) editor — a
 * toggle that turns a state from a plain played-back one into a continuous
 * blend between two animations' extremes, driven by a named number input
 * (a facial rig's `mouth_openness` input mapped across "closed"/"open"
 * extreme poses, per `Blend1DDef`'s own doc comment). Lives in the side
 * column next to the graph, not inside the box itself — same "a box has
 * room for two dropdowns, not an open-ended inspector" reasoning this
 * file's own module doc already gives for keeping transitions there too. */
function BlendFields({
  artboard,
  state,
  onChange,
}: {
  artboard: ArtboardSummary
  state: StateMachineDefSummary['states'][number]
  onChange: (blend: Blend1DDef | null) => void
}) {
  const { blend } = state

  return (
    <div className="flex flex-col gap-1 rounded-md bg-black/20 p-1.5">
      <label className="flex items-center justify-between gap-2 rounded-md bg-overlay/8 px-2 py-1 text-xs text-ink">
        <span>Blend two animations</span>
        <input
          type="checkbox"
          checked={blend !== null}
          onChange={(e) =>
            onChange(
              e.target.checked
                ? { input: blend?.input ?? '', low: state.animation, high: state.animation, lowValue: 0, highValue: 1 }
                : null,
            )
          }
        />
      </label>
      {blend && (
        <>
          <input className={CONTROL} placeholder="input name" value={blend.input} onChange={(e) => onChange({ ...blend, input: e.target.value })} />
          <label className="flex flex-col gap-0.5 text-[11px] text-ink-dim">
            Low animation
            <Select
              className={CONTROL}
              value={`${blend.low.idx}:${blend.low.version}`}
              onChange={(e) => {
                const found = artboard.animations.find((a) => `${a.id.idx}:${a.id.version}` === e.target.value)
                if (found) onChange({ ...blend, low: found.id })
              }}
            >
              {!artboard.animations.some((a) => a.id.idx === blend.low.idx && a.id.version === blend.low.version) && (
                <option value={`${blend.low.idx}:${blend.low.version}`}>⚠ Deleted animation — pick one</option>
              )}
              {artboard.animations.map((a) => (
                <option key={`${a.id.idx}:${a.id.version}`} value={`${a.id.idx}:${a.id.version}`}>
                  {a.name}
                </option>
              ))}
            </Select>
          </label>
          <label className="flex items-center justify-between gap-2 rounded-md bg-overlay/8 px-2 py-1 text-xs text-ink">
            <span>Low value</span>
            <input
              type="number"
              className="w-16 rounded bg-overlay/10 px-1.5 py-0.5 text-right text-xs text-ink outline-none focus:bg-overlay/15"
              value={blend.lowValue}
              onChange={(e) => onChange({ ...blend, lowValue: Number(e.target.value) || 0 })}
            />
          </label>
          <label className="flex flex-col gap-0.5 text-[11px] text-ink-dim">
            High animation
            <Select
              className={CONTROL}
              value={`${blend.high.idx}:${blend.high.version}`}
              onChange={(e) => {
                const found = artboard.animations.find((a) => `${a.id.idx}:${a.id.version}` === e.target.value)
                if (found) onChange({ ...blend, high: found.id })
              }}
            >
              {!artboard.animations.some((a) => a.id.idx === blend.high.idx && a.id.version === blend.high.version) && (
                <option value={`${blend.high.idx}:${blend.high.version}`}>⚠ Deleted animation — pick one</option>
              )}
              {artboard.animations.map((a) => (
                <option key={`${a.id.idx}:${a.id.version}`} value={`${a.id.idx}:${a.id.version}`}>
                  {a.name}
                </option>
              ))}
            </Select>
          </label>
          <label className="flex items-center justify-between gap-2 rounded-md bg-overlay/8 px-2 py-1 text-xs text-ink">
            <span>High value</span>
            <input
              type="number"
              className="w-16 rounded bg-overlay/10 px-1.5 py-0.5 text-right text-xs text-ink outline-none focus:bg-overlay/15"
              value={blend.highValue}
              onChange={(e) => onChange({ ...blend, highValue: Number(e.target.value) || 0 })}
            />
          </label>
        </>
      )}
    </div>
  )
}

/** The selected state's Programmable State toggle — turns a plain
 * played-back (or blend) state into one driven every frame by a `program`
 * instead. Placed right after `BlendFields`, same "side column next to the
 * graph" reasoning that section's own doc comment gives. Deliberately just
 * the on/off toggle plus a hint, not the program's own editor — a real
 * node canvas needs more room than this narrow side column has, so editing
 * a program happens in `ProgramEditorPanel`'s full-width drilled-in view
 * instead (double-click this state's box on the graph, once this toggle is
 * on, to get there — see that component's own doc comment). A program only
 * ever actually *runs* while the State Machine tab's own "Run Programs"
 * toggle is on, or in the Playground tab (always) — see `App.tsx`'s
 * `programsEnabled` state and `editor.rs`'s `state_machine_programs_enabled`
 * doc comment; this toggle only authors *whether* a program exists. */
function ProgramFields({
  state,
  onChange,
}: {
  state: StateMachineDefSummary['states'][number]
  onChange: (program: ProgramDef | null) => void
}) {
  const { program } = state

  return (
    <div className="flex flex-col gap-1 rounded-md bg-black/20 p-1.5">
      <label className="flex items-center justify-between gap-2 rounded-md bg-overlay/8 px-2 py-1 text-xs text-ink">
        <span>Programmable State</span>
        <input type="checkbox" checked={program !== null} onChange={(e) => onChange(e.target.checked ? { kind: 'Graph', value: { nodes: [], edges: [] } } : null)} />
      </label>
      {program && <p className="text-[10px] leading-snug text-ink-dim">Double-click this state's box on the graph to edit its program.</p>}
    </div>
  )
}
