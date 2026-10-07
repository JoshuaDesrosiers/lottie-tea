import { useState } from 'react'
import { STATE_MACHINE_DOCK_HEIGHT } from '../layoutConstants'
import { useLayoutMetrics } from '../layoutMetrics'
import type { ArtboardSummary, Blend1DDef, ConditionDef, InputKind, LoopMode, OpaqueId, PlaybackDirection, ProgramDef, StateMachineDefSummary, StateMachineSummary } from '../ipc/document'
import { FloatingPanel } from './FloatingPanel'
import { ProgramEditorPanel } from './ProgramEditorPanel'
import { StateMachineGraph } from './StateMachineGraph'
import { StateMachinePanel } from './StateMachinePanel'

interface PointerHandlers {
  onPointerDown: (event: React.PointerEvent) => void
  onPointerMove: (event: React.PointerEvent) => void
  onPointerUp: (event: React.PointerEvent) => void
}

/**
 * Bottom-docked, full-width home for state-machine authoring — takes
 * `TimelinePanel`'s exact same screen slot (fixed, non-draggable, same
 * `left`/`right`/`bottom` clearances) when `mode === 'State Machine'`,
 * rather than any of this living in the narrow right-dock sidebar the way
 * it used to. A node graph genuinely needs width to lay states out
 * left-to-right — the "move the state machine editor panel down" request
 * this component answers — and switching to this mode no longer costs the
 * designer their Inspector (`App.tsx` keeps `InspectorPanel` permanently
 * right-docked now, regardless of `mode`).
 *
 * Houses both halves of state-machine authoring side by side: a narrow
 * `StateMachinePanel` column (which machine is active, its declared inputs)
 * — genuinely list-shaped, so it keeps the same ~230px-friendly layout it
 * always had — and `StateMachineGraph` (states/transitions) taking the rest
 * of the width.
 */
export function StateMachineDock({
  artboard,
  activeMachine,
  summary,
  onFireTrigger,
  onAddStateMachine,
  onSelectStateMachine,
  onDeleteStateMachine,
  onRenameStateMachine,
  onAddInput,
  onSetBoolInput,
  onSetNumberInput,
  onSetEnumInput,
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
  programsEnabled,
  onToggleProgramsEnabled,
  isPlayground = false,
  onResetLayout,
  height = STATE_MACHINE_DOCK_HEIGHT,
  resizeHandleProps,
}: {
  artboard: ArtboardSummary | null
  activeMachine: StateMachineDefSummary | undefined
  summary: StateMachineSummary | null
  onFireTrigger: (name: string) => void
  onAddStateMachine: (name: string, initialAnimation: OpaqueId) => void
  onSelectStateMachine: (id: OpaqueId) => void
  onDeleteStateMachine: (id: OpaqueId) => void
  onRenameStateMachine: (id: OpaqueId, name: string) => void
  onAddInput: (name: string, kind: InputKind) => void
  onSetBoolInput: (name: string, value: boolean) => void
  onSetNumberInput: (name: string, value: number) => void
  onSetEnumInput: (name: string, value: string) => void
  selectedLayer: string | null
  onSelectLayer: (layer: string | null) => void
  onAddLayer: (name: string, animation: OpaqueId) => void
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
  /** Whether a Programmable State's `program` is actually allowed to run
   * right now — mirrors `editor.rs`'s `state_machine_programs_enabled`
   * field. Only meaningful (and only shown as an editable toggle) outside
   * Playground — see `isPlayground`. */
  programsEnabled: boolean
  onToggleProgramsEnabled: (enabled: boolean) => void
  /** `true` when this dock is rendered for the Playground tab instead of
   * the State Machine tab — programs always run there (no toggle to show;
   * `programsEnabled` is forced `true` and displayed as a disabled,
   * checked indicator instead of an editable control), per the "Playground
   * is the always-live place" design. */
  isPlayground?: boolean
  /** Re-arranges every state box in the currently-selected layer back into
   * a clean grid — Playground-only (see the button's own render site):
   * that's the one tab with no Layers/Inspector sidebars and a canvas that
   * ignores clicks, so dragging a state box out of place there has no other
   * way back short of this. */
  onResetLayout?: () => void
  height?: number
  resizeHandleProps?: PointerHandlers
}) {
  // Which Programmable State's own program view is currently drilled into
  // — `null` shows the normal `StateMachineGraph`. Local, not lifted to
  // `App.tsx`: nothing outside this dock needs to know, and it should reset
  // to the graph view for free whenever this whole dock unmounts (leaving
  // the State Machine/Playground tab entirely already does this — `App.tsx`
  // only renders `StateMachineDock` at all while one of those two modes is
  // active). See `ProgramEditorPanel`'s own doc comment for what this opens.
  const [drilled, setDrilled] = useState<{ stateName: string; layer: string | null } | null>(null)

  const drilledStates = drilled?.layer ? activeMachine?.layers.find((l) => l.name === drilled.layer)?.states : activeMachine?.states
  const drilledStateDef = drilled ? drilledStates?.find((s) => s.name === drilled.stateName) : undefined
  const m = useLayoutMetrics()

  return (
    <div
      className="pointer-events-auto absolute select-none"
      style={{ left: m.leftDockOffset, right: m.edgeMargin, bottom: m.edgeMargin, height }}
    >
      <FloatingPanel className="relative flex h-full gap-2 p-2">
        {resizeHandleProps && (
          <div {...resizeHandleProps} className="group absolute -top-1.5 left-0 z-10 h-3 w-full touch-none" style={{ cursor: 'ns-resize' }}>
            <div className="mx-auto mt-1.5 h-px w-full bg-transparent group-hover:bg-accent/70 group-active:bg-accent" />
          </div>
        )}
        <div className="flex w-56 shrink-0 flex-col gap-2 overflow-y-auto border-r border-overlay/10 pr-2">
          <label
            className={`flex items-center justify-between gap-2 rounded-md px-2 py-1 text-xs ${isPlayground ? 'bg-accent/10 text-accent' : 'bg-overlay/8 text-ink'}`}
            title={isPlayground ? 'Playground always runs Programmable States' : "Let Programmable States actually run while previewing here — off by default so switching to this tab never silently starts running arbitrary authored code/graphs"}
          >
            <span>Run Programs</span>
            <input
              type="checkbox"
              checked={isPlayground || programsEnabled}
              disabled={isPlayground}
              onChange={(e) => onToggleProgramsEnabled(e.target.checked)}
            />
          </label>
          {isPlayground && onResetLayout && (
            <button
              onClick={onResetLayout}
              title="Re-arrange every state's box back into a clean grid — undoes any dragging, doesn't touch the states/transitions themselves"
              className="rounded-md bg-overlay/8 px-2 py-1 text-xs text-ink-dim hover:text-ink"
            >
              Reset Layout
            </button>
          )}
          <StateMachinePanel
            artboard={artboard}
            onFireTrigger={onFireTrigger}
            onAddStateMachine={onAddStateMachine}
            onSelectStateMachine={onSelectStateMachine}
            onDeleteStateMachine={onDeleteStateMachine}
            onRenameStateMachine={onRenameStateMachine}
            onAddInput={onAddInput}
            onSetBoolInput={onSetBoolInput}
            onSetNumberInput={onSetNumberInput}
            onSetEnumInput={onSetEnumInput}
          />
        </div>
        <div className="min-w-0 flex-1">
          {artboard && activeMachine ? (
            drilled && drilledStateDef ? (
              <ProgramEditorPanel
                stateName={drilledStateDef.name}
                program={drilledStateDef.program}
                nodes={artboard.nodes}
                onChange={(program) => onSetStateProgram(drilled.stateName, program, drilled.layer)}
                onBack={() => setDrilled(null)}
              />
            ) : (
              <StateMachineGraph
                artboard={artboard}
                activeMachine={activeMachine}
                summary={summary}
                selectedLayer={selectedLayer}
                onSelectLayer={onSelectLayer}
                onAddLayer={onAddLayer}
                onDeleteLayer={onDeleteLayer}
                onSetLayerInitialState={onSetLayerInitialState}
                onAddState={onAddState}
                onUpdateState={onUpdateState}
                onDeleteState={onDeleteState}
                onSetStatePosition={onSetStatePosition}
                onSetInitialState={onSetInitialState}
                onAddTransition={onAddTransition}
                onSetTransition={onSetTransition}
                onRemoveTransition={onRemoveTransition}
                onSetStateBlend={onSetStateBlend}
                onSetStateProgram={onSetStateProgram}
                onEnterProgram={(stateName, layer) => setDrilled({ stateName, layer })}
              />
            )
          ) : (
            <p className="text-xs text-ink-dim">No state machine — create one from the sidebar on the left.</p>
          )}
        </div>
      </FloatingPanel>
    </div>
  )
}
