/**
 * **Test Area** — try the scene you're editing inside a real-feeling app
 * before you ship it. The current scene is exported to Toasty Lite a
 * moment after every edit and dropped, fully interactive, into
 * `TestApp`: one mock product with a sidebar / drawer, top bar, tab bar,
 * floating button and three pages with different feels — **Home** (a
 * social feed), **Article** (a long read) and **Code** (a developer page) —
 * shown as a **phone** or a **desktop** browser window.
 *
 * **Replace** picks which element your animation takes the place of (a nav
 * icon, a post action, the hero image, the Run button, the whole page…).
 * **Hi-fi** swaps crossed-out wireframe boxes for real photos and text.
 * It can also sit in the editor's own title bar, and **Use as Playground**
 * makes Playground show the app full size instead of the play view.
 */
import { Select } from './Select'
import { useEffect, useMemo, useState, useSyncExternalStore } from 'react'
import type { WasmEditor } from '../ipc/wasmEditor'
import type { DocumentSummary, OpaqueId } from '../ipc/document'
import type { LiteFile, LitePlayer } from '../lite/runtime'
import { LiteLive } from './LiteLive'
import { BIG_SLOTS, TestApp, deviceSize, testSlots } from './TestApp'
import { testAreaStore, type TestSettings } from '../game/testAreaStore'

export function TestAreaPanel({ editor, doc, artboardId, variant = 'panel' }: { editor: WasmEditor | null; doc: DocumentSummary | null; artboardId: OpaqueId | null; variant?: 'panel' | 'stage' }) {
  const stage = variant === 'stage'
  const shared = useSyncExternalStore(testAreaStore.subscribe, testAreaStore.get)
  const s = shared.settings
  const set = (patch: Partial<TestSettings>) => testAreaStore.setSettings(patch)
  const [ownFile, setFile] = useState<LiteFile | null>(null)
  const file = stage ? shared.file : ownFile
  const [error, setError] = useState<string | null>(null)
  const players = useMemo(() => new Set<LitePlayer>(), [])
  const [, bump] = useState(0)

  useEffect(() => {
    if (stage || !editor || !artboardId) return
    const t = window.setTimeout(() => {
      try {
        const next = JSON.parse(editor.export_lite(artboardId)) as LiteFile
        setFile(next)
        testAreaStore.set({ file: next })
        setError(null)
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err))
      }
    }, 250)
    return () => window.clearTimeout(t)
  }, [stage, editor, doc, artboardId])

  // The title bar slot only shows while this panel is open.
  useEffect(() => {
    if (stage) return
    testAreaStore.set({ panelOpen: true })
    return () => testAreaStore.set({ panelOpen: false })
  }, [stage])

  const inputs = (file?.stateMachine?.inputs ?? []).filter((i) => i.name !== 'hover')
  const first = () => players.values().next().value as LitePlayer | undefined
  const setAll = (name: string, value: boolean | number | string) => {
    for (const p of players) p.setInput(name, value)
    bump((n) => n + 1)
  }
  const fireAll = (name: string) => players.forEach((p) => p.fire(name))
  const enumValues = (name: string) => {
    const out: string[] = []
    for (const layer of file?.stateMachine?.layers ?? []) for (const st of layer.states) for (const t of st.transitions) if (t.condition.type === 'enum' && t.condition.input === name && !out.includes(String(t.condition.value))) out.push(String(t.condition.value))
    return out
  }

  const c = s.dark ? { line: '#2c2c33', text: '#ececef', muted: '#8d8d97', surface: '#17171b', soft: '#202026' } : { line: '#e6e5e1', text: '#18181b', muted: '#74737a', surface: '#ffffff', soft: '#f1f1ee' }
  const ink = s.dark ? [0.93, 0.93, 0.94, 1] : [0.1, 0.1, 0.12, 1]
  const slot = s.slot[s.page]
  const live = (hover: boolean, w: number | string, h: number | string, fit?: 'cover' | 'contain') =>
    file ? <LiteLive file={file} width={w} height={h} ink={ink} hover={s.rowHover ? hover : undefined} players={players} fit={fit} /> : <div style={{ width: w, height: h }} />
  const onPage = (page: TestSettings['page']) => set({ page })
  const [dw, dh] = deviceSize(s.device)

  if (stage) {
    const inputsBar = inputs.length > 0 && (
          <div className="flex flex-wrap items-center justify-center gap-3 rounded-lg border px-3 py-2 text-[12px]" style={{ borderColor: c.line, color: c.muted, background: c.surface }}>
            {inputs.map((i) => {
              const v = first()?.getInput?.(i.name)
              if (i.kind === 'trigger')
                return (
                  <button key={i.name} onClick={() => fireAll(i.name)} className="rounded border px-2 py-0.5" style={{ borderColor: c.line, color: c.text }}>
                    {i.name}
                  </button>
                )
              if (i.kind === 'bool')
                return (
                  <label key={i.name} className="flex items-center gap-1">
                    <input type="checkbox" checked={!!v} onChange={(e) => setAll(i.name, e.target.checked)} />
                    {i.name}
                  </label>
                )
              if (i.kind === 'enum')
                return (
                  <span key={i.name} className="flex items-center gap-1">
                    {i.name}
                    {enumValues(i.name).map((ev) => (
                      <button key={ev} onClick={() => setAll(i.name, ev)} className="rounded px-1.5 py-0.5" style={{ background: v === ev ? c.soft : 'transparent', color: c.text }}>
                        {ev}
                      </button>
                    ))}
                  </span>
                )
              return (
                <label key={i.name} className="flex items-center gap-1">
                  {i.name}
                  <input type="range" min={0} max={1} step={0.01} value={Number(v ?? 0)} onChange={(e) => setAll(i.name, Number(e.target.value))} />
                </label>
              )
            })}
          </div>
    )
    return <TestApp s={s} live={live} onPage={onPage} frame="stage" controls={inputsBar} />
  }

  const chip = (active: boolean) => `rounded px-2 py-0.5 text-[11px] ${active ? 'bg-overlay/15 text-ink' : 'text-ink-dim hover:text-ink'}`
  const numberRow = (label: string, value: number, min: number, max: number, onChange: (v: number) => void) => (
    <label className="flex items-center justify-between gap-2 text-[11px] text-ink-dim">
      {label}
      <input type="range" min={min} max={max} value={value} onChange={(e) => onChange(Number(e.target.value))} className="w-28" />
      <span className="w-6 text-right tabular-nums text-ink">{value}</span>
    </label>
  )

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-1">
        {(['phone', 'desktop'] as const).map((d) => (
          <button key={d} onClick={() => set({ device: d })} className={chip(s.device === d)}>
            {d === 'phone' ? 'Phone' : 'Desktop'}
          </button>
        ))}
        <span className="mx-1 h-3 w-px bg-overlay/15" />
        {(['home', 'article', 'code'] as const).map((p) => (
          <button key={p} onClick={() => set({ page: p })} className={chip(s.page === p)}>
            {p === 'home' ? 'Home' : p === 'article' ? 'Article' : 'Code'}
          </button>
        ))}
      </div>
      <div className="flex items-center gap-1">
        <button onClick={() => set({ dark: !s.dark })} className={chip(false)} title="Preview on a light or dark page">
          {s.dark ? 'Dark' : 'Light'}
        </button>
        <button onClick={() => set({ hifi: !s.hifi })} className={chip(s.hifi)} title="Real photos and text instead of wireframes">
          Hi-fi
        </button>
        <div className="flex-1" />
        <label className="flex items-center gap-1 text-[11px] text-ink-dim" title="Playground shows this app, full size, instead of the normal play view">
          <input type="checkbox" checked={s.showInPlayground} onChange={(e) => set({ showInPlayground: e.target.checked })} />
          Use as Playground
        </label>
      </div>

      <div className="relative overflow-hidden rounded-md border border-panel-border" style={{ aspectRatio: `${dw + 16} / ${dh + 16}`, maxHeight: s.device === 'phone' ? 560 : undefined }}>
        <TestApp s={s} live={live} onPage={onPage} frame="panel" />
      </div>
      {error && <p className="text-[10px] text-red-400">{error}</p>}
      {file && file.skipped.length > 0 && (
        <p className="text-[10px] leading-snug text-ink-dim">
          Lite leaves out: {file.skipped.slice(0, 3).join('; ')}
          {file.skipped.length > 3 ? '…' : ''}
        </p>
      )}

      <div className="flex flex-col gap-1.5 rounded-md border border-panel-border p-2">
        <label className="flex items-center justify-between gap-2 text-[11px] text-ink-dim">
          Replace
          <Select value={slot} onChange={(e) => set({ slot: { ...s.slot, [s.page]: e.target.value } })} menuWidth={200} className="w-44 rounded border border-panel-border bg-solid-panel px-1 py-0.5 text-ink">
            {testSlots(s.page, s.count).map(([group, items]) => (
              <optgroup key={group} label={group}>
                {items.map(([id, label]) => (
                  <option key={id} value={id}>
                    {label}
                  </option>
                ))}
              </optgroup>
            ))}
          </Select>
        </label>
        {BIG_SLOTS.includes(slot) && (
          <div className="flex items-center gap-1 text-[11px] text-ink-dim">
            Fit
            <button onClick={() => set({ cover: false })} className={chip(!s.cover)}>
              Contain
            </button>
            <button onClick={() => set({ cover: true })} className={chip(s.cover)}>
              Cover
            </button>
          </div>
        )}
        {s.page === 'home' && numberRow('Post actions', s.count, 1, 8, (v) => set({ count: v }))}
        {numberRow('Icon size', s.size, 16, 64, (v) => set({ size: v }))}
        <label className="flex items-center justify-between gap-2 text-[11px] text-ink-dim">
          Label
          <input value={s.label} onChange={(e) => set({ label: e.target.value })} className="w-32 rounded border border-panel-border bg-transparent px-1.5 py-0.5 text-ink" />
        </label>
        <label className="flex items-center gap-2 text-[11px] text-ink-dim" title="Your animation also sits in the editor's own title bar, top right">
          <input type="checkbox" checked={s.inTitleBar} onChange={(e) => set({ inTitleBar: e.target.checked })} />
          Also in the title bar
        </label>
        {s.inTitleBar && numberRow('Title bar size', s.navSize, 14, 40, (v) => set({ navSize: v }))}
        <label className="flex items-center gap-2 text-[11px] text-ink-dim">
          <input type="checkbox" checked={s.rowHover} onChange={(e) => set({ rowHover: e.target.checked })} />
          Hovering the whole slot drives <code className="text-ink">hover</code>
        </label>
      </div>

      {inputs.length > 0 && (
        <div className="flex flex-col gap-1.5 rounded-md border border-panel-border p-2">
          <div className="text-[10px] font-medium uppercase tracking-wide text-ink-dim">Inputs</div>
          {inputs.map((i) => {
            const v = first()?.getInput?.(i.name)
            if (i.kind === 'trigger')
              return (
                <div key={i.name} className="flex items-center justify-between text-[11px] text-ink-dim">
                  {i.name}
                  <button onClick={() => fireAll(i.name)} className="rounded border border-panel-border px-2 py-0.5 text-ink hover:bg-overlay/10">
                    Fire
                  </button>
                </div>
              )
            if (i.kind === 'bool')
              return (
                <label key={i.name} className="flex items-center justify-between text-[11px] text-ink-dim">
                  {i.name}
                  <input type="checkbox" checked={!!v} onChange={(e) => setAll(i.name, e.target.checked)} />
                </label>
              )
            if (i.kind === 'enum')
              return (
                <div key={i.name} className="flex flex-wrap items-center gap-1 text-[11px] text-ink-dim">
                  <span className="mr-1">{i.name}</span>
                  {enumValues(i.name).map((ev) => (
                    <button key={ev} onClick={() => setAll(i.name, ev)} className={chip(v === ev)}>
                      {ev}
                    </button>
                  ))}
                </div>
              )
            return (
              <label key={i.name} className="flex items-center justify-between gap-2 text-[11px] text-ink-dim">
                {i.name}
                <input type="range" min={0} max={1} step={0.01} value={Number(v ?? 0)} onChange={(e) => setAll(i.name, Number(e.target.value))} className="w-28" />
              </label>
            )
          })}
        </div>
      )}
      <p className="text-[10px] leading-snug text-ink-dim">This scene as Toasty Lite, live: edits show up here a moment after you make them. Click and hover it in the app to test its listeners; the app's own nav switches pages.</p>
    </div>
  )
}
