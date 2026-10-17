import { Popover, Select, insideOrPopover } from './Select'
import { useEffect, useRef, useState } from 'react'
import type { ArtboardSummary, InputKind, OpaqueId, StateMachineDefSummary } from '../ipc/document'
import { ChevronDownIcon, EditIcon, PlusIcon, TrashIcon } from './icons'

/** Shared control styling for every text/select input in this panel — a
 * dark-on-dark surface plus `min-w-0` (flex items default to a
 * content-based min-width, which for a `<Select>` is roughly its longest
 * `<option>`'s rendered width; without `min-w-0` a wide option like
 * "NumberGreaterThan" blows out any row it shares with another flex sibling,
 * which is what caused the horizontal-scroll/cut-off-text overflow this
 * panel originally shipped with — every row below is also stacked
 * vertically rather than packing 3+ controls side by side, since this
 * panel's own ~230px column doesn't have room for that regardless of
 * `min-w-0`). */
const CONTROL = 'w-full min-w-0 rounded-md bg-overlay/8 px-2 py-1.5 text-xs text-ink outline-none focus:bg-overlay/12'
const BUTTON = 'w-full rounded-md bg-accent/15 px-2 py-1.5 text-xs font-medium text-accent hover:bg-accent/25'
const LABEL = 'text-[11px] font-medium uppercase tracking-wide text-ink-dim'

function encodeId(id: OpaqueId): string {
  return `${id.idx}:${id.version}`
}

/**
 * A small custom-drawn dropdown (not a native `<Select>`) — mirrors
 * `TimelinePanel.tsx`'s `AnimationDropdown`, same reasoning: a native
 * select's popup renders as whatever the OS/browser draws it as, which
 * can't be restyled to match this app's dark theme (AN1MBYT3_BUILD_PLAN.md
 * §15's "one consistent, custom-drawn" surface language). "+ New State
 * Machine" lives as the list's own trailing row rather than a separate
 * text-field-plus-button below the dropdown — clicking it swaps the popover
 * into a name-entry form in place, Enter/blur creates it.
 */
function StateMachineDropdown({
  machines,
  activeId,
  onSelect,
  onCreate,
}: {
  machines: StateMachineDefSummary[]
  activeId: OpaqueId | null
  onSelect: (id: OpaqueId) => void
  onCreate: (name: string) => void
}) {
  const [open, setOpen] = useState(false)
  const [creating, setCreating] = useState(false)
  const [newName, setNewName] = useState('')
  const rootRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const active = machines.find((m) => activeId && encodeId(m.id) === encodeId(activeId))

  useEffect(() => {
    function handleOutsideClick(event: MouseEvent) {
      if (rootRef.current && !insideOrPopover(rootRef.current, event.target as Node)) {
        setOpen(false)
        setCreating(false)
      }
    }
    document.addEventListener('mousedown', handleOutsideClick)
    return () => document.removeEventListener('mousedown', handleOutsideClick)
  }, [])

  useEffect(() => {
    if (creating) inputRef.current?.focus()
  }, [creating])

  const commitCreate = () => {
    if (newName.trim()) onCreate(newName.trim())
    setNewName('')
    setCreating(false)
    setOpen(false)
  }

  return (
    <div ref={rootRef} className="relative min-w-0 flex-1">
      <button
        onClick={() => setOpen((o) => !o)}
        className="flex w-full items-center justify-between gap-1 rounded-md bg-overlay/8 px-2 py-1.5 text-xs text-ink hover:bg-overlay/12"
      >
        <span className="min-w-0 flex-1 truncate text-left">{active?.name ?? 'No state machine'}</span>
        <ChevronDownIcon />
      </button>
      {open && (
        <Popover anchor={rootRef} className="rounded-lg border border-panel-border bg-panel p-1 shadow-2xl backdrop-blur-panel">
          {creating ? (
            <div className="flex gap-1 p-0.5">
              <input
                ref={inputRef}
                className="w-full min-w-0 rounded-md bg-overlay/8 px-2 py-1 text-xs text-ink outline-none"
                placeholder="State machine name"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') commitCreate()
                  if (e.key === 'Escape') {
                    setCreating(false)
                    setNewName('')
                  }
                }}
              />
              <button onClick={commitCreate} className="shrink-0 rounded-md bg-accent/15 px-2 py-1 text-xs font-medium text-accent hover:bg-accent/25">
                Create
              </button>
            </div>
          ) : (
            <>
              {machines.map((m) => (
                <button
                  key={encodeId(m.id)}
                  onClick={() => {
                    onSelect(m.id)
                    setOpen(false)
                  }}
                  className={`flex w-full items-center rounded-md px-2 py-1 text-left text-xs ${
                    activeId && encodeId(activeId) === encodeId(m.id) ? 'bg-accent/15 text-accent' : 'text-ink hover:bg-overlay/5'
                  }`}
                >
                  <span className="truncate">{m.name}</span>
                </button>
              ))}
              {machines.length > 0 && <div className="my-1 h-px bg-overlay/10" />}
              <button
                onClick={() => setCreating(true)}
                className="flex w-full items-center gap-1 rounded-md px-2 py-1 text-left text-xs text-ink-dim hover:bg-overlay/5 hover:text-ink"
              >
                <PlusIcon /> New State Machine
              </button>
            </>
          )}
        </Popover>
      )}
    </div>
  )
}

/**
 * The narrow, genuinely list-shaped half of state-machine authoring: which
 * machine is active, and its declared inputs (with their live-test
 * controls). Lives inside `StateMachineDock`'s left column now, not the
 * right-dock Inspector sidebar — the Inspector stays Inspector regardless
 * of mode (see `TopToolbar`'s doc comment for why that changed). The other
 * half — states/transitions, which need real width to show as a node graph
 * rather than opaque text rows — lives in `StateMachineGraph`, filling the
 * rest of `StateMachineDock`'s width.
 *
 * Two data sources feed this, deliberately kept separate (see
 * `crates/an1mbyt3-wasm/src/editor.rs`'s module doc for the full "authored
 * vs. live" split this mirrors):
 * - `artboard.stateMachines`/`artboard.activeStateMachine` — the *authored*,
 *   saved half (`an1mbyt3_core::StateMachineDef`).
 * - `summary` (`StateMachineSummary`) — the *live* half: which triggers are
 *   legal to fire right now. `onFireTrigger`/`onSetBoolInput` etc. never
 *   touch the saved document, so they never produce an undo entry.
 */
export function StateMachinePanel({
  artboard,
  onFireTrigger,
  onAddStateMachine,
  onSelectStateMachine,
  onDeleteStateMachine,
  onRenameStateMachine,
  onAddInput,
  onSetBoolInput,
  onSetNumberInput,
  onSetEnumInput,
}: {
  artboard: ArtboardSummary | null
  onFireTrigger: (name: string) => void
  onAddStateMachine: (name: string, initialAnimation: OpaqueId) => void
  onSelectStateMachine: (id: OpaqueId) => void
  onDeleteStateMachine: (id: OpaqueId) => void
  onRenameStateMachine: (id: OpaqueId, name: string) => void
  onAddInput: (name: string, kind: InputKind) => void
  onSetBoolInput: (name: string, value: boolean) => void
  onSetNumberInput: (name: string, value: number) => void
  onSetEnumInput: (name: string, value: string) => void
}) {
  const [newInputName, setNewInputName] = useState('')
  const [newInputKind, setNewInputKind] = useState<InputKind>('Bool')
  const [editingName, setEditingName] = useState(false)
  const [nameDraft, setNameDraft] = useState('')
  const nameInputRef = useRef<HTMLInputElement>(null)

  if (!artboard) {
    return <p className="text-xs text-ink-dim">No artboard.</p>
  }

  const activeMachine = artboard.stateMachines.find(
    (sm) => sm.id.idx === artboard.activeStateMachine?.idx && sm.id.version === artboard.activeStateMachine?.version,
  )

  const handleAddInput = () => {
    if (!newInputName.trim()) return
    onAddInput(newInputName.trim(), newInputKind)
    setNewInputName('')
  }

  const startEditingName = () => {
    if (!activeMachine) return
    setNameDraft(activeMachine.name)
    setEditingName(true)
    requestAnimationFrame(() => nameInputRef.current?.focus())
  }

  const commitName = () => {
    if (activeMachine && nameDraft.trim()) onRenameStateMachine(activeMachine.id, nameDraft.trim())
    setEditingName(false)
  }

  return (
    <div className="flex flex-col gap-4 overflow-x-hidden">
      <div className="flex flex-col gap-1.5">
        <span className={LABEL}>State Machine</span>
        {editingName && activeMachine ? (
          <input
            ref={nameInputRef}
            className={CONTROL}
            value={nameDraft}
            onChange={(e) => setNameDraft(e.target.value)}
            onBlur={commitName}
            onKeyDown={(e) => {
              if (e.key === 'Enter') e.currentTarget.blur()
              if (e.key === 'Escape') setEditingName(false)
            }}
          />
        ) : (
          <div className="flex items-center gap-1">
            <StateMachineDropdown
              machines={artboard.stateMachines}
              activeId={artboard.activeStateMachine}
              onSelect={onSelectStateMachine}
              onCreate={(name) => {
                const firstAnimation = artboard.activeAnimation ?? artboard.animations[0]?.id
                if (firstAnimation) onAddStateMachine(name, firstAnimation)
              }}
            />
            {activeMachine && (
              <>
                <button
                  onClick={startEditingName}
                  title="Rename"
                  className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-ink-dim hover:bg-overlay/8 hover:text-ink"
                >
                  <EditIcon />
                </button>
                <button
                  onClick={() => onDeleteStateMachine(activeMachine.id)}
                  title="Delete state machine"
                  className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-ink-dim hover:bg-red-500/10 hover:text-red-400"
                >
                  <TrashIcon />
                </button>
              </>
            )}
          </div>
        )}
      </div>

      {activeMachine && (
        <div className="flex flex-col gap-1.5">
          <span className={LABEL}>Inputs</span>
          <div className="flex flex-col gap-1">
            {activeMachine.inputs.map((input) => (
              <div key={input.name} className="flex items-center gap-2 rounded-md bg-overlay/8 px-2 py-1.5 text-xs text-ink">
                <span className="min-w-0 flex-1 truncate" title={input.name}>
                  {input.name} <span className="text-ink-dim">({input.kind})</span>
                </span>
                {input.kind === 'Bool' && <input type="checkbox" onChange={(e) => onSetBoolInput(input.name, e.target.checked)} />}
                {input.kind === 'Number' && (
                  <input
                    type="number"
                    className="w-16 shrink-0 rounded-md bg-black/30 px-1 py-0.5 text-ink outline-none"
                    onChange={(e) => onSetNumberInput(input.name, Number(e.target.value) || 0)}
                  />
                )}
                {input.kind === 'Enum' && (
                  <input
                    type="text"
                    className="w-16 shrink-0 rounded-md bg-black/30 px-1 py-0.5 text-ink outline-none"
                    onChange={(e) => onSetEnumInput(input.name, e.target.value)}
                  />
                )}
                {input.kind === 'Trigger' && (
                  <button
                    onClick={() => onFireTrigger(input.name)}
                    className="shrink-0 rounded-md bg-accent/15 px-2 py-0.5 font-medium text-accent hover:bg-accent/25"
                  >
                    Fire
                  </button>
                )}
              </div>
            ))}
          </div>
          <input className={CONTROL} placeholder="New input name" value={newInputName} onChange={(e) => setNewInputName(e.target.value)} />
          <Select className={CONTROL} value={newInputKind} onChange={(e) => setNewInputKind(e.target.value as InputKind)}>
            <option value="Bool">Bool</option>
            <option value="Number">Number</option>
            <option value="Enum">Enum</option>
            <option value="Trigger">Trigger</option>
          </Select>
          <button onClick={handleAddInput} className={BUTTON}>
            Declare Input
          </button>
        </div>
      )}
    </div>
  )
}
