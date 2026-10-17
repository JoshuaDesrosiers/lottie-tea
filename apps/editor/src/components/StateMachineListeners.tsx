/**
 * **Listeners** — the State Machine panel's answer to Rive's listeners: wire
 * a node's pointer events straight to this state machine's inputs, without
 * leaving the State Machine tab.
 *
 * A listener is *stored* exactly the way the Inspector's event components
 * store it (and the way the Templates are built): an action in the node's
 * **On Click** (`UiEvents`) or **Pointer Listeners** (`UiListeners`) list,
 * on a node that has a **UI Button** (what detects the pointer). This panel
 * is a second, state-machine-centred view of those same lists:
 *
 * - it lists every input action (Set input / Fire trigger / Toggle input)
 *   on every node, as "node · event → action";
 * - "+ Listener" picks a node, an event and an action, and adds it — adding
 *   the UI Button (with no visual feedback of its own, so it doesn't tint
 *   the art) and the event component when the node lacks them;
 * - each row can be edited or removed in place.
 *
 * Non-input actions (show / hide, load scene…) stay in the Inspector; they
 * aren't hidden or changed by edits here.
 */
import { useState } from 'react'
import type { ActionKind, ArtboardSummary, DocumentSummary, EventAction, InputKind, NodeBehavior, NodeBehaviorEntry, NodeSummary, UiEventsConfig, UiListenersConfig } from '../ipc/document'
import { defaultComponent, setNodeBehaviors, addStateMachineInput, type WasmEditor } from '../ipc/wasmEditor'
import { InputPicker, InputValueField, sceneInputs } from './EventFields'
import { Select } from './Select'
import { PlusIcon } from './icons'

type EventKey = 'click' | 'enter' | 'exit' | 'down' | 'up'
const EVENTS: [EventKey, string][] = [
  ['click', 'Click'],
  ['enter', 'Pointer enter (hover on)'],
  ['exit', 'Pointer exit (hover off)'],
  ['down', 'Pointer down'],
  ['up', 'Pointer up'],
]
const EVENT_SHORT: Record<EventKey, string> = { click: 'click', enter: 'enter', exit: 'exit', down: 'down', up: 'up' }
/** Which list inside `UiListeners` an event lives in. */
const LISTENER_FIELD: Record<Exclude<EventKey, 'click'>, keyof UiListenersConfig> = { enter: 'onEnter', exit: 'onExit', down: 'onDown', up: 'onUp' }

type Verb = 'set' | 'fire' | 'toggle'

const isInputAction = (a: ActionKind) => typeof a === 'object' && ('SetInput' in a || 'FireTrigger' in a || 'ToggleInput' in a)
const verbOf = (a: ActionKind): Verb => (typeof a === 'object' && 'FireTrigger' in a ? 'fire' : typeof a === 'object' && 'ToggleInput' in a ? 'toggle' : 'set')
const inputOf = (a: ActionKind) => (typeof a !== 'object' ? '' : 'SetInput' in a ? a.SetInput.input : 'FireTrigger' in a ? a.FireTrigger.input : 'ToggleInput' in a ? a.ToggleInput.input : '')
const actionFor = (verb: Verb, input: string, value: string): ActionKind => (verb === 'fire' ? { FireTrigger: { input } } : verb === 'toggle' ? { ToggleInput: { input } } : { SetInput: { input, value } })

const idKey = (id: { idx: number; version: number }) => `${id.idx}.${id.version}`

/** One listener row's address: node, event, index in that list. */
interface Row {
  node: NodeSummary
  event: EventKey
  index: number
  action: EventAction
}

/** Every input action on every node of `artboard`. */
function collect(artboard: ArtboardSummary): Row[] {
  const rows: Row[] = []
  for (const node of artboard.nodes) {
    for (const { behavior } of node.behaviors as NodeBehaviorEntry[]) {
      if (behavior.kind === 'UiEvents') behavior.onClick.forEach((action, index) => isInputAction(action.action) && rows.push({ node, event: 'click', index, action }))
      if (behavior.kind === 'UiListeners')
        for (const [event, field] of Object.entries(LISTENER_FIELD) as [Exclude<EventKey, 'click'>, keyof UiListenersConfig][])
          behavior[field].forEach((action, index) => isInputAction(action.action) && rows.push({ node, event, index, action }))
    }
  }
  return rows
}

const label = 'text-[11px] font-medium uppercase tracking-wide text-ink-dim'
const ctl = 'w-full min-w-0 rounded-md bg-overlay/8 px-2 py-1 text-xs text-ink outline-none'

export function StateMachineListeners({ artboard, editor, onDocumentChange }: { artboard: ArtboardSummary; editor: WasmEditor | null; onDocumentChange: (doc: DocumentSummary) => void }) {
  const rows = collect(artboard)
  const [adding, setAdding] = useState(false)
  const [draft, setDraft] = useState<{ node: string; event: EventKey; verb: Verb; input: string; value: string }>({ node: '', event: 'click', verb: 'set', input: '', value: 'true' })
  const [error, setError] = useState<string | null>(null)

  const addInput = (name: string, kind: InputKind) => {
    const sm = artboard.activeStateMachine
    if (editor && sm) onDocumentChange(addStateMachineInput(editor, artboard.id, sm, name, kind))
  }

  /** Rewrites one node's components through `mutate`, making sure it has a
   * UI Button (pointer detection) and the event component first. */
  const writeNode = (node: NodeSummary, event: EventKey, mutate: (list: EventAction[]) => EventAction[]) => {
    if (!editor) return
    try {
      const behaviors: NodeBehaviorEntry[] = (node.behaviors as NodeBehaviorEntry[]).map((e) => ({ ...e }))
      if (!behaviors.some((e) => e.behavior.kind === 'UiButton')) {
        const button = defaultComponent(editor, 'UiButton') as Extract<NodeBehavior, { kind: 'UiButton' }>
        // A listener target shouldn't change how the art looks on hover /
        // press: the state machine's own animations do that.
        const white: [number, number, number, number] = [1, 1, 1, 1]
        button.style = { ...button.style, normalColor: white, highlightedColor: white, pressedColor: white, highlightedScale: 1, pressedScale: 1 }
        behaviors.push({ behavior: button, enabled: true })
      }
      if (event === 'click') {
        let i = behaviors.findIndex((e) => e.behavior.kind === 'UiEvents')
        if (i < 0) i = behaviors.push({ behavior: { kind: 'UiEvents', onClick: [] }, enabled: true }) - 1
        const b = behaviors[i].behavior as { kind: 'UiEvents' } & UiEventsConfig
        behaviors[i] = { ...behaviors[i], behavior: { ...b, onClick: mutate([...b.onClick]) } }
      } else {
        let i = behaviors.findIndex((e) => e.behavior.kind === 'UiListeners')
        if (i < 0) i = behaviors.push({ behavior: { kind: 'UiListeners', onEnter: [], onExit: [], onDown: [], onUp: [] }, enabled: true }) - 1
        const b = behaviors[i].behavior as { kind: 'UiListeners' } & UiListenersConfig
        const field = LISTENER_FIELD[event]
        behaviors[i] = { ...behaviors[i], behavior: { ...b, [field]: mutate([...b[field]]) } }
      }
      onDocumentChange(setNodeBehaviors(editor, artboard.id, node.id, behaviors))
      setError(null)
    } catch (e) {
      setError(String(e))
    }
  }

  const update = (row: Row, action: ActionKind) => writeNode(row.node, row.event, (list) => list.map((a, i) => (i === row.index ? { ...a, action } : a)))
  const remove = (row: Row) => writeNode(row.node, row.event, (list) => list.filter((_, i) => i !== row.index))
  /** Moving a row to another event = remove here, append there. */
  const changeEvent = (row: Row, event: EventKey) => {
    if (event === row.event) return
    // One write: the node minus this row, then append it to the new event.
    const fresh = { ...row.node, behaviors: (row.node.behaviors as NodeBehaviorEntry[]).map((e) => {
      const b = e.behavior
      if (row.event === 'click' && b.kind === 'UiEvents') return { ...e, behavior: { ...b, onClick: b.onClick.filter((_, i) => i !== row.index) } }
      if (row.event !== 'click' && b.kind === 'UiListeners') {
        const f = LISTENER_FIELD[row.event]
        return { ...e, behavior: { ...b, [f]: b[f].filter((_, i) => i !== row.index) } }
      }
      return e
    }) }
    writeNode(fresh as NodeSummary, event, (list) => [...list, row.action])
  }

  const nodes = artboard.nodes.filter((n) => n.kind !== 'Camera' && n.kind !== 'Bone')
  const addDraft = () => {
    const node = nodes.find((n) => idKey(n.id) === draft.node)
    if (!node || !draft.input) return
    writeNode(node, draft.event, (list) => [...list, { target: null, action: actionFor(draft.verb, draft.input, draft.value) }])
    setAdding(false)
    setDraft((d) => ({ ...d, input: '' }))
  }

  const verbSelect = (verb: Verb, onChange: (v: Verb) => void) => (
    <Select value={verb} onChange={(e) => onChange(e.target.value as Verb)} className={ctl}>
      <option value="set">Set input</option>
      <option value="fire">Fire trigger</option>
      <option value="toggle">Toggle Bool input</option>
    </Select>
  )
  const kindsFor = (verb: Verb): InputKind[] => (verb === 'fire' ? ['Trigger'] : verb === 'toggle' ? ['Bool'] : ['Bool', 'Number', 'Enum'])
  const kindOf = (name: string) => sceneInputs(artboard).find((i) => i.name === name)?.kind

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center justify-between">
        <span className={label}>Listeners</span>
        <button onClick={() => setAdding((a) => !a)} title="Wire a node's click / hover to an input" className="flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] text-ink-dim hover:bg-overlay/8 hover:text-ink">
          <PlusIcon /> Listener
        </button>
      </div>
      {rows.length === 0 && !adding && <p className="text-[11px] leading-snug text-ink-dim">Nothing listens yet. Add one to drive inputs from a node's click or hover — e.g. hover on → hover = true.</p>}

      {rows.map((row) => {
        const verb = verbOf(row.action.action)
        const input = inputOf(row.action.action)
        const value = typeof row.action.action === 'object' && 'SetInput' in row.action.action ? row.action.action.SetInput.value : ''
        return (
          <div key={`${idKey(row.node.id)}:${row.event}:${row.index}`} className="flex flex-col gap-1 rounded-md bg-overlay/[0.06] p-1.5">
            <div className="flex items-center gap-1 text-[11px]">
              <span className="min-w-0 flex-1 truncate font-medium text-ink" title={row.node.name}>
                {row.node.name || 'Unnamed'}
              </span>
              <Select value={row.event} onChange={(e) => changeEvent(row, e.target.value as EventKey)} className="w-20 rounded bg-overlay/8 px-1 py-0.5 text-[11px] text-ink-dim outline-none" title="When">
                {EVENTS.map(([k, l]) => (
                  <option key={k} value={k}>
                    {k === row.event ? EVENT_SHORT[k] : l}
                  </option>
                ))}
              </Select>
              <button onClick={() => remove(row)} title="Remove listener" className="rounded px-1 text-ink-dim hover:bg-red-500/10 hover:text-red-400">
                ✕
              </button>
            </div>
            {verbSelect(verb, (v) => update(row, actionFor(v, input, value || 'true')))}
            <div className="flex items-center gap-1">
              <InputPicker value={input} onChange={(name) => update(row, actionFor(verb, name, value || (kindOf(name) === 'Bool' ? 'true' : '')))} artboard={artboard} kinds={kindsFor(verb)} onAddInput={addInput} className={ctl} />
              {verb === 'set' && <InputValueField kind={kindOf(input)} value={value} onChange={(v) => update(row, actionFor('set', input, v))} />}
            </div>
          </div>
        )
      })}

      {adding && (
        <div className="flex flex-col gap-1 rounded-md border border-accent/40 p-1.5">
          <Select value={draft.node} onChange={(e) => setDraft({ ...draft, node: e.target.value })} className={ctl} placeholder="Node…">
            <option value="">Node…</option>
            {nodes.map((n) => (
              <option key={idKey(n.id)} value={idKey(n.id)}>
                {n.name || 'Unnamed'}
              </option>
            ))}
          </Select>
          <Select value={draft.event} onChange={(e) => setDraft({ ...draft, event: e.target.value as EventKey })} className={ctl}>
            {EVENTS.map(([k, l]) => (
              <option key={k} value={k}>
                {l}
              </option>
            ))}
          </Select>
          {verbSelect(draft.verb, (verb) => setDraft({ ...draft, verb, input: '' }))}
          <div className="flex items-center gap-1">
            <InputPicker value={draft.input} onChange={(input) => setDraft({ ...draft, input, value: kindOf(input) === 'Bool' ? (draft.event === 'exit' ? 'false' : 'true') : draft.value })} artboard={artboard} kinds={kindsFor(draft.verb)} onAddInput={addInput} className={ctl} />
            {draft.verb === 'set' && <InputValueField kind={kindOf(draft.input)} value={draft.value} onChange={(value) => setDraft({ ...draft, value })} />}
          </div>
          <div className="flex gap-1">
            <button onClick={addDraft} disabled={!draft.node || !draft.input} className="flex-1 rounded-md bg-accent/15 px-2 py-1 text-xs font-medium text-accent hover:bg-accent/25 disabled:opacity-40">
              Add listener
            </button>
            <button onClick={() => setAdding(false)} className="rounded-md px-2 py-1 text-xs text-ink-dim hover:bg-overlay/8 hover:text-ink">
              Cancel
            </button>
          </div>
          {!artboard.activeStateMachine && <p className="text-[10px] text-ink-dim">Create a state machine first to declare inputs.</p>}
        </div>
      )}
      {error && <p className="text-[10px] text-red-400">{error}</p>}
    </div>
  )
}

