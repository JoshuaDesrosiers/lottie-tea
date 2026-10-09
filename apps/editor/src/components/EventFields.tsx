import { useState } from 'react'
import type { ActionKind, ArtboardSummary, BindTarget, DocumentSummary, EventAction, NodeSummary, OpaqueId, PropertyBinding, PropertyBindingsConfig, PropertySummary, UiEventsConfig } from '../ipc/document'
import { addProperty, deleteProperty, updateProperty, type WasmEditor } from '../ipc/wasmEditor'
import { COMPONENT_METHODS, describeAction } from '../game/events'

/**
 * The Inspector side of events and properties (`crates/an1mbyt3-core/src/events.rs`):
 * - `UiEventsFields` — a node's **On Click** list, Unity's `Button.onClick`:
 *   each row picks a target node, then what to do to it (show / hide, set a
 *   node field, set any field of one of its components, call a component
 *   method), or a scene-wide action (fire a trigger, set an input, load a
 *   scene, play a sound, set a property, call a script function).
 * - `PropertyBindingsFields` — what a node shows of the scene's properties.
 * - `ScenePropertiesFields` — the scene's properties themselves (Rive's
 *   view model): add, rename, type, default.
 */

const sel = 'min-w-0 flex-1 rounded bg-overlay/10 px-1.5 py-1 text-[11px] text-ink outline-none'
const inp = 'min-w-0 flex-1 rounded bg-overlay/10 px-1.5 py-1 font-mono text-[11px] text-ink outline-none placeholder:text-ink-dim'

const sameId = (a: OpaqueId, b: OpaqueId) => a.idx === b.idx && a.version === b.version
const idKey = (id: OpaqueId) => `${id.idx}.${id.version}`

/** A component's editable field names (its JSON keys, minus `kind`). */
function componentFields(node: NodeSummary | null | undefined, component: string): string[] {
  const entry = node?.behaviors.find((e) => e.behavior.kind === component)
  if (!entry) return []
  return Object.keys(entry.behavior).filter((k) => k !== 'kind')
}

/** Every choice an action can be: `group` / `value` / label. */
type Choice = { group: string; value: string; label: string }

function actionChoices(target: NodeSummary | null | undefined, artboard: ArtboardSummary | null): Choice[] {
  const out: Choice[] = [
    { group: 'Node', value: 'show', label: 'Show' },
    { group: 'Node', value: 'hide', label: 'Hide' },
    { group: 'Node', value: 'toggle', label: 'Toggle visible' },
    ...['x', 'y', 'rotation', 'scaleX', 'scaleY', 'text', 'fillColor'].map((f) => ({ group: 'Node', value: `node:${f}`, label: `Set ${f}` })),
  ]
  for (const entry of target?.behaviors ?? []) {
    const kind = entry.behavior.kind
    if (kind === 'UiEvents' || kind === 'PropertyBindings' || kind === 'Script' || kind === 'SpriteRenderer') continue
    for (const m of COMPONENT_METHODS[kind] ?? []) out.push({ group: kind, value: `call:${kind}:${m.method}`, label: m.label })
    for (const f of componentFields(target, kind)) out.push({ group: kind, value: `field:${kind}:${f}`, label: `Set ${f}` })
  }
  const inputs = artboard?.stateMachines.flatMap((m) => m.inputs) ?? []
  if (inputs.length) {
    out.push({ group: 'State machine', value: 'trigger', label: 'Fire trigger' })
    out.push({ group: 'State machine', value: 'input', label: 'Set input' })
  }
  out.push({ group: 'Scene', value: 'property', label: 'Set property' })
  out.push({ group: 'Scene', value: 'scene', label: 'Load scene' })
  out.push({ group: 'Scene', value: 'sound', label: 'Play sound' })
  out.push({ group: 'Scene', value: 'function', label: 'Call script function' })
  return out
}

/** The choice an action currently is. */
function choiceOf(a: ActionKind): string {
  if (a === 'ToggleVisible') return 'toggle'
  if ('SetVisible' in a) return a.SetVisible.visible ? 'show' : 'hide'
  if ('SetNodeField' in a) return `node:${a.SetNodeField.field}`
  if ('SetComponentField' in a) return `field:${a.SetComponentField.component}:${a.SetComponentField.field}`
  if ('CallMethod' in a) return `call:${a.CallMethod.component}:${a.CallMethod.method}`
  if ('FireTrigger' in a) return 'trigger'
  if ('SetInput' in a) return 'input'
  if ('LoadScene' in a) return 'scene'
  if ('PlaySound' in a) return 'sound'
  if ('SetProperty' in a) return 'property'
  return 'function'
}

/** A fresh action for a choice. */
function actionFor(choice: string): ActionKind {
  const [head, a, b] = choice.split(':')
  switch (head) {
    case 'show':
      return { SetVisible: { visible: true } }
    case 'hide':
      return { SetVisible: { visible: false } }
    case 'toggle':
      return 'ToggleVisible'
    case 'node':
      return { SetNodeField: { field: a, value: '' } }
    case 'field':
      return { SetComponentField: { component: a, field: b, value: '' } }
    case 'call':
      return { CallMethod: { component: a, method: b, arg: '' } }
    case 'trigger':
      return { FireTrigger: { input: '' } }
    case 'input':
      return { SetInput: { input: '', value: '' } }
    case 'scene':
      return { LoadScene: { scene: '' } }
    case 'sound':
      return { PlaySound: { sound: '' } }
    case 'property':
      return { SetProperty: { property: '', value: '' } }
    default:
      return { CallFunction: { function: '', arg: '' } }
  }
}

/** Does the action act on a node (so the target picker matters)? */
const targetsNode = (a: ActionKind) => a === 'ToggleVisible' || 'SetVisible' in a || 'SetNodeField' in a || 'SetComponentField' in a || 'CallMethod' in a

export interface EventContext {
  artboard: ArtboardSummary | null
  /** The node whose events these are. */
  self: NodeSummary
  sceneNames: string[]
  soundNames: string[]
  properties: PropertySummary[]
}

/** One action row's argument field(s). */
function ActionArgs({ action, onChange, ctx }: { action: ActionKind; onChange: (a: ActionKind) => void; ctx: EventContext }) {
  if (action === 'ToggleVisible' || 'SetVisible' in action) return null
  const triggers = ctx.artboard?.stateMachines.flatMap((m) => m.inputs.filter((i) => i.kind === 'Trigger').map((i) => i.name)) ?? []
  const inputs = ctx.artboard?.stateMachines.flatMap((m) => m.inputs.filter((i) => i.kind !== 'Trigger').map((i) => i.name)) ?? []
  const props = ctx.properties.filter((p) => ctx.artboard && sameId(p.artboard, ctx.artboard.id))
  const pick = (value: string, options: string[], set: (v: string) => void, empty: string) => (
    <select value={value} onChange={(e) => set(e.target.value)} className={sel}>
      <option value="">{options.length ? empty : `(none — ${empty.toLowerCase()})`}</option>
      {options.map((o) => (
        <option key={o} value={o}>
          {o}
        </option>
      ))}
      {value && !options.includes(value) && <option value={value}>{value} (missing)</option>}
    </select>
  )
  const text = (value: string, set: (v: string) => void, placeholder: string) => <input value={value} onChange={(e) => set(e.target.value)} placeholder={placeholder} className={inp} />
  if ('SetNodeField' in action) {
    const f = action.SetNodeField
    return text(f.value, (value) => onChange({ SetNodeField: { ...f, value } }), f.field === 'fillColor' ? '[1, 0.5, 0, 1] or #ff8800' : f.field === 'text' ? 'New text' : 'value')
  }
  if ('SetComponentField' in action) {
    const f = action.SetComponentField
    return text(f.value, (value) => onChange({ SetComponentField: { ...f, value } }), 'value (JSON: 3, true, "text", [1,0,0,1])')
  }
  if ('CallMethod' in action) {
    const f = action.CallMethod
    const arg = COMPONENT_METHODS[f.component]?.find((m) => m.method === f.method)?.arg
    return arg ? text(f.arg, (value) => onChange({ CallMethod: { ...f, arg: value } }), arg) : null
  }
  if ('FireTrigger' in action) return pick(action.FireTrigger.input, triggers, (input) => onChange({ FireTrigger: { input } }), 'Trigger…')
  if ('SetInput' in action) {
    const f = action.SetInput
    return (
      <>
        {pick(f.input, inputs, (input) => onChange({ SetInput: { ...f, input } }), 'Input…')}
        {text(f.value, (value) => onChange({ SetInput: { ...f, value } }), 'true / 3 / Run')}
      </>
    )
  }
  if ('LoadScene' in action) return pick(action.LoadScene.scene, ctx.sceneNames, (scene) => onChange({ LoadScene: { scene } }), 'Scene…')
  if ('PlaySound' in action) return pick(action.PlaySound.sound, ctx.soundNames, (sound) => onChange({ PlaySound: { sound } }), 'Sound…')
  if ('SetProperty' in action) {
    const f = action.SetProperty
    const kind = props.find((p) => p.name === f.property)?.kind
    return (
      <>
        {pick(
          f.property,
          props.map((p) => p.name),
          (property) => onChange({ SetProperty: { ...f, property } }),
          'Property…',
        )}
        {text(f.value, (value) => onChange({ SetProperty: { ...f, value } }), kind === 'Number' ? '5, +1, -1' : kind === 'Bool' ? 'true / false / !' : 'value')}
      </>
    )
  }
  const f = action.CallFunction
  return (
    <>
      {text(f.function, (fn) => onChange({ CallFunction: { ...f, function: fn } }), "name (ctx.on('name', fn))")}
      {text(f.arg, (arg) => onChange({ CallFunction: { ...f, arg } }), 'argument (optional)')}
    </>
  )
}

/** On Click — see the module docs. */
export function UiEventsFields({ config, onChange, ctx }: { config: UiEventsConfig; onChange: (value: UiEventsConfig) => void; ctx: EventContext }) {
  const nodes = ctx.artboard?.nodes ?? []
  const setAt = (i: number, step: EventAction) => onChange({ onClick: config.onClick.map((s, k) => (k === i ? step : s)) })
  const remove = (i: number) => onChange({ onClick: config.onClick.filter((_, k) => k !== i) })
  const move = (i: number, d: number) => {
    const list = [...config.onClick]
    const j = i + d
    if (j < 0 || j >= list.length) return
    ;[list[i], list[j]] = [list[j], list[i]]
    onChange({ onClick: list })
  }
  return (
    <div className="flex flex-col gap-1.5">
      <div className="text-[11px] font-medium text-ink">On Click ()</div>
      {config.onClick.length === 0 && <div className="text-[10px] text-ink-dim">Nothing happens yet — add an action. (Runs when this node's UI Button is clicked.)</div>}
      {config.onClick.map((step, i) => {
        const target = step.target ? nodes.find((n) => sameId(n.id, step.target!)) : ctx.self
        const choices = actionChoices(target, ctx.artboard)
        const groups = [...new Set(choices.map((c) => c.group))]
        const current = choiceOf(step.action)
        return (
          <div key={i} className="flex flex-col gap-1 rounded-md border border-panel-border bg-overlay/[0.04] p-1.5">
            <div className="flex items-center gap-1">
              {targetsNode(step.action) && (
                <select
                  value={step.target ? idKey(step.target) : ''}
                  onChange={(e) => {
                    const next = nodes.find((n) => idKey(n.id) === e.target.value)
                    setAt(i, { ...step, target: next ? next.id : null })
                  }}
                  className={sel}
                  title="Which node it acts on"
                >
                  <option value="">This node ({ctx.self.name})</option>
                  {nodes
                    .filter((n) => !sameId(n.id, ctx.self.id))
                    .map((n) => (
                      <option key={idKey(n.id)} value={idKey(n.id)}>
                        {n.name || n.elementId || 'Unnamed'}
                      </option>
                    ))}
                </select>
              )}
              <select value={current} onChange={(e) => setAt(i, { ...step, action: actionFor(e.target.value) })} className={sel} title={describeAction(step.action)}>
                {groups.map((g) => (
                  <optgroup key={g} label={g}>
                    {choices
                      .filter((c) => c.group === g)
                      .map((c) => (
                        <option key={c.value} value={c.value}>
                          {c.label}
                        </option>
                      ))}
                  </optgroup>
                ))}
                {!choices.some((c) => c.value === current) && <option value={current}>{describeAction(step.action)} (not on this target)</option>}
              </select>
            </div>
            <div className="flex items-center gap-1">
              <ActionArgs action={step.action} onChange={(action) => setAt(i, { ...step, action })} ctx={ctx} />
              <div className="ml-auto flex shrink-0 gap-0.5">
                <button onClick={() => move(i, -1)} className="rounded px-1 text-ink-dim hover:bg-overlay/10 hover:text-ink" title="Move up">
                  ↑
                </button>
                <button onClick={() => move(i, 1)} className="rounded px-1 text-ink-dim hover:bg-overlay/10 hover:text-ink" title="Move down">
                  ↓
                </button>
                <button onClick={() => remove(i)} className="rounded px-1 text-red-400 hover:bg-overlay/10" title="Remove">
                  ✕
                </button>
              </div>
            </div>
          </div>
        )
      })}
      <button onClick={() => onChange({ onClick: [...config.onClick, { target: null, action: { SetVisible: { visible: false } } }] })} className="rounded-md border border-dashed border-panel-border px-2 py-1 text-[11px] text-ink-dim hover:border-accent hover:text-ink">
        + Add action
      </button>
    </div>
  )
}

const BIND_TARGETS: { value: string; label: string }[] = [
  { value: 'Text', label: 'Text' },
  { value: 'X', label: 'Position X' },
  { value: 'Y', label: 'Position Y' },
  { value: 'Rotation', label: 'Rotation' },
  { value: 'Scale', label: 'Scale' },
  { value: 'ScaleX', label: 'Scale X' },
  { value: 'ScaleY', label: 'Scale Y' },
  { value: 'Visible', label: 'Visible' },
  { value: 'FillColor', label: 'Fill colour' },
]
const targetKey = (t: BindTarget) => (typeof t === 'string' ? t : `field:${t.ComponentField.component}:${t.ComponentField.field}`)
const targetOf = (key: string): BindTarget => {
  if (!key.startsWith('field:')) return key as BindTarget
  const [, component, field] = key.split(':')
  return { ComponentField: { component, field } }
}

/** Property Binding — the node shows / follows scene properties. */
export function PropertyBindingsFields({ config, onChange, ctx }: { config: PropertyBindingsConfig; onChange: (value: PropertyBindingsConfig) => void; ctx: EventContext }) {
  const props = ctx.properties.filter((p) => ctx.artboard && sameId(p.artboard, ctx.artboard.id))
  const setAt = (i: number, patch: Partial<PropertyBinding>) => onChange({ bindings: config.bindings.map((b, k) => (k === i ? { ...b, ...patch } : b)) })
  const componentTargets = ctx.self.behaviors.flatMap((e) =>
    e.behavior.kind === 'PropertyBindings' || e.behavior.kind === 'UiEvents' ? [] : componentFields(ctx.self, e.behavior.kind).map((f) => ({ value: `field:${e.behavior.kind}:${f}`, label: `${e.behavior.kind} › ${f}` })),
  )
  return (
    <div className="flex flex-col gap-1.5">
      {props.length === 0 && <div className="text-[10px] text-ink-dim">This scene has no properties yet — add some in the Inspector with nothing selected (Properties).</div>}
      {config.bindings.map((b, i) => (
        <div key={i} className="flex flex-col gap-1 rounded-md border border-panel-border bg-overlay/[0.04] p-1.5">
          <div className="flex items-center gap-1">
            <select value={b.property} onChange={(e) => setAt(i, { property: e.target.value })} className={sel} title="The property">
              <option value="">Property…</option>
              {props.map((p) => (
                <option key={p.name} value={p.name}>
                  {p.name} ({p.kind})
                </option>
              ))}
            </select>
            <span className="text-ink-dim">→</span>
            <select value={targetKey(b.target)} onChange={(e) => setAt(i, { target: targetOf(e.target.value) })} className={sel} title="What it drives">
              <optgroup label="Node">
                {BIND_TARGETS.map((t) => (
                  <option key={t.value} value={t.value}>
                    {t.label}
                  </option>
                ))}
              </optgroup>
              {componentTargets.length > 0 && (
                <optgroup label="Components">
                  {componentTargets.map((t) => (
                    <option key={t.value} value={t.value}>
                      {t.label}
                    </option>
                  ))}
                </optgroup>
              )}
            </select>
            <button onClick={() => onChange({ bindings: config.bindings.filter((_, k) => k !== i) })} className="rounded px-1 text-red-400 hover:bg-overlay/10" title="Remove">
              ✕
            </button>
          </div>
          <div className="flex items-center gap-1">
            {b.target === 'Text' ? (
              <input value={b.format} onChange={(e) => setAt(i, { format: e.target.value })} placeholder="Format, {} = value (e.g. Score: {})" className={inp} />
            ) : (
              <>
                <span className="text-[10px] text-ink-dim">× </span>
                <input type="number" step="any" value={b.scale} onChange={(e) => setAt(i, { scale: Number(e.target.value) })} className={inp} title="Multiply numbers by" />
                <span className="text-[10px] text-ink-dim">+ </span>
                <input type="number" step="any" value={b.offset} onChange={(e) => setAt(i, { offset: Number(e.target.value) })} className={inp} title="Then add" />
              </>
            )}
          </div>
        </div>
      ))}
      <button
        onClick={() => onChange({ bindings: [...config.bindings, { property: props[0]?.name ?? '', target: 'Text', format: '', scale: 1, offset: 0 }] })}
        className="rounded-md border border-dashed border-panel-border px-2 py-1 text-[11px] text-ink-dim hover:border-accent hover:text-ink"
      >
        + Bind a property
      </button>
    </div>
  )
}

/** The scene's properties (Inspector, nothing selected). */
export function ScenePropertiesFields({ editor, artboard, properties, onDocumentChange }: { editor: WasmEditor | null; artboard: ArtboardSummary; properties: PropertySummary[]; onDocumentChange: (doc: DocumentSummary) => void }) {
  const mine = properties.filter((p) => sameId(p.artboard, artboard.id))
  const [error, setError] = useState<string | null>(null)
  const save = (p: PropertySummary, patch: Partial<Pick<PropertySummary, 'name' | 'kind' | 'default'>>) => {
    if (!editor) return
    const next = { ...p, ...patch }
    // Changing the type resets the default to that type's zero value.
    if (patch.kind && patch.kind !== p.kind) next.default = { Number: '0', Bool: 'false', Text: '""', Color: '[1,1,1,1]' }[patch.kind]
    try {
      onDocumentChange(updateProperty(editor, p.id, next.name, next.kind, next.default))
      setError(null)
    } catch (e) {
      setError(String(e))
    }
  }
  return (
    <div className="flex flex-col gap-1.5">
      <p className="text-[10px] leading-snug text-ink-dim">
        Values the whole scene shares — scripts use <code className="text-ink">ctx.props.name</code>, buttons can set them, Property Binding shows them on nodes, HTML UI uses{' '}
        <code className="text-ink">data-bind="name"</code> / <code className="text-ink">data-prop="name"</code>.
      </p>
      {mine.map((p) => (
        <div key={idKey(p.id)} className="flex items-center gap-1">
          <input defaultValue={p.name} onBlur={(e) => e.target.value !== p.name && save(p, { name: e.target.value })} className={inp} title="Name" />
          <select value={p.kind} onChange={(e) => save(p, { kind: e.target.value as PropertySummary['kind'] })} className="rounded bg-overlay/10 px-1 py-1 text-[11px] text-ink outline-none">
            {(['Number', 'Bool', 'Text', 'Color'] as const).map((k) => (
              <option key={k}>{k}</option>
            ))}
          </select>
          {p.kind === 'Bool' ? (
            <input type="checkbox" checked={p.default === 'true'} onChange={(e) => save(p, { default: e.target.checked ? 'true' : 'false' })} title="Starts as" />
          ) : (
            <input
              key={p.default}
              defaultValue={p.kind === 'Text' ? (() => { try { return JSON.parse(p.default) } catch { return p.default } })() : p.default}
              onBlur={(e) => {
                const v = p.kind === 'Text' ? JSON.stringify(e.target.value) : e.target.value
                if (v !== p.default) save(p, { default: v })
              }}
              className={inp}
              title="Starts as"
            />
          )}
          <button onClick={() => editor && onDocumentChange(deleteProperty(editor, p.id))} className="rounded px-1 text-red-400 hover:bg-overlay/10" title="Delete">
            ✕
          </button>
        </div>
      ))}
      {error && <div className="text-[10px] text-red-400">{error}</div>}
      <div className="flex gap-1">
        {(['Number', 'Bool', 'Text', 'Color'] as const).map((k) => (
          <button key={k} onClick={() => editor && onDocumentChange(addProperty(editor, artboard.id, k === 'Number' ? 'value' : k === 'Bool' ? 'flag' : k === 'Text' ? 'label' : 'tint', k))} className="flex-1 rounded-md border border-dashed border-panel-border px-1 py-1 text-[10px] text-ink-dim hover:border-accent hover:text-ink">
            + {k}
          </button>
        ))}
      </div>
    </div>
  )
}
