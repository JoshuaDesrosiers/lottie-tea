/**
 * **Test Area** — try the scene you're editing inside real-looking UI
 * before you ship it. The current scene is exported to Toasty Lite on
 * every edit (debounced) and dropped, fully interactive, into a mock:
 *
 * - **Nav bar** — as one icon in a site's top navigation.
 * - **Sidebar** — filling a sidebar, or as one item in a list of
 *   crossed-out placeholder rows (wireframe style).
 * - **Post bar** — as one action in a social post's bottom bar, or filling
 *   the whole bar.
 *
 * Everything is adjustable: item count, which slot is yours, size, label,
 * light / dark, and whether hovering the whole row (the way real nav items
 * behave) drives the animation's `hover` input. The animation's own
 * inputs get live controls underneath, so a state can be checked in
 * context without leaving Design.
 */
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import type { WasmEditor } from '../ipc/wasmEditor'
import type { DocumentSummary, OpaqueId } from '../ipc/document'
import { createLite, type LiteFile, type LitePlayer } from '../lite/runtime'

type Mock = 'nav' | 'sidebar' | 'post'

interface Settings {
  mock: Mock
  /** sidebar: fill the panel, or be one row; post: one action, or fill the bar. */
  fill: boolean
  count: number
  slot: number
  size: number
  label: string
  dark: boolean
  rowHover: boolean
}

const DEFAULTS: Settings = { mock: 'sidebar', fill: false, count: 4, slot: 1, size: 28, label: 'Likes', dark: false, rowHover: true }

function load(): Settings {
  try {
    return { ...DEFAULTS, ...(JSON.parse(localStorage.getItem('an1.testArea') ?? '{}') as Partial<Settings>) }
  } catch {
    return DEFAULTS
  }
}

/** One live Lite player on a given exported file. */
function Live({ file, size, ink, hover, players, style }: { file: LiteFile; size: number | string; ink: number[]; hover?: boolean; players: Set<LitePlayer>; style?: React.CSSProperties }) {
  const box = useRef<HTMLDivElement>(null)
  const [player, setPlayer] = useState<LitePlayer | null>(null)
  useEffect(() => {
    const el = box.current
    if (!el) return
    let made: LitePlayer | null = null
    let cancelled = false
    void createLite(el, { src: file }).then((p) => {
      if (cancelled) return p.destroy()
      made = p
      players.add(p)
      setPlayer(p)
    })
    return () => {
      cancelled = true
      if (made) {
        players.delete(made)
        made.destroy()
      }
      setPlayer(null)
    }
  }, [file, players])
  useEffect(() => {
    if (player && file.properties.some((p) => p.name === 'ink')) player.set('ink', ink)
  }, [player, ink, file])
  useEffect(() => {
    if (player && hover !== undefined && file.stateMachine?.inputs.some((i) => i.name === 'hover')) player.setInput('hover', hover)
  }, [player, hover, file])
  return <div ref={box} style={{ width: size, height: size, flex: 'none', ...style }} />
}

/** A wireframe placeholder: a box with both diagonals crossed. */
function Placeholder({ w, h, line }: { w: number | string; h: number; line: string }) {
  return (
    <svg width={w} height={h} style={{ flex: typeof w === 'number' ? 'none' : '1 1 auto', minWidth: 0, maxWidth: '100%', height: h, display: 'block' }} preserveAspectRatio="none" viewBox="0 0 100 100">
      <rect x="1" y="1" width="98" height="98" rx="6" fill="none" stroke={line} strokeWidth="2" vectorEffect="non-scaling-stroke" />
      <line x1="4" y1="4" x2="96" y2="96" stroke={line} strokeWidth="1.2" vectorEffect="non-scaling-stroke" />
      <line x1="96" y1="4" x2="4" y2="96" stroke={line} strokeWidth="1.2" vectorEffect="non-scaling-stroke" />
    </svg>
  )
}

/** A hoverable slot that reports hover (drives the animation's `hover`). */
function Slot({ children, onHover, style }: { children: (hover: boolean) => ReactNode; onHover?: boolean; style?: React.CSSProperties }) {
  const [hover, setHover] = useState(false)
  return (
    <div onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)} style={{ cursor: 'pointer', ...style }}>
      {children(onHover ? hover : false)}
    </div>
  )
}

export function TestAreaPanel({ editor, doc, artboardId }: { editor: WasmEditor | null; doc: DocumentSummary | null; artboardId: OpaqueId | null }) {
  const [s, setS] = useState<Settings>(load)
  const set = (patch: Partial<Settings>) =>
    setS((prev) => {
      const next = { ...prev, ...patch }
      try {
        localStorage.setItem('an1.testArea', JSON.stringify(next))
      } catch {
        // Not remembered; still applies.
      }
      return next
    })
  const [file, setFile] = useState<LiteFile | null>(null)
  const [error, setError] = useState<string | null>(null)
  const players = useMemo(() => new Set<LitePlayer>(), [])
  const [, bump] = useState(0)

  // Re-export the scene shortly after each edit.
  useEffect(() => {
    if (!editor || !artboardId) return
    const t = window.setTimeout(() => {
      try {
        setFile(JSON.parse(editor.export_lite(artboardId)) as LiteFile)
        setError(null)
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err))
      }
    }, 250)
    return () => window.clearTimeout(t)
  }, [editor, doc, artboardId])

  // Inputs fan out to every copy of the animation in the mock.
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

  // Mock palette.
  const c = s.dark
    ? { page: '#0f0f12', surface: '#17171b', line: '#2c2c33', text: '#ececef', muted: '#8d8d97', ph: '#3a3a42', ink: [0.93, 0.93, 0.94, 1] }
    : { page: '#f4f4f2', surface: '#ffffff', line: '#e6e5e1', text: '#18181b', muted: '#74737a', ph: '#d4d3ce', ink: [0.1, 0.1, 0.12, 1] }
  const slot = Math.min(s.slot, s.count - 1)
  const live = (hover: boolean, size: number | string = s.size, style?: React.CSSProperties) =>
    file ? <Live file={file} size={size} ink={c.ink} hover={s.rowHover ? hover : undefined} players={players} style={style} /> : <div style={{ width: size, height: size }} />

  let mock: ReactNode = null
  if (s.mock === 'nav') {
    mock = (
      <div style={{ display: 'flex', alignItems: 'center', gap: 14, padding: '10px 14px', background: c.surface, borderBottom: `1px solid ${c.line}` }}>
        <div style={{ width: 18, height: 18, borderRadius: 5, background: c.text, opacity: 0.85 }} />
        <Placeholder w={46} h={10} line={c.ph} />
        <div style={{ flex: 1 }} />
        {Array.from({ length: s.count }, (_, i) =>
          i === slot ? (
            <Slot key={i} onHover style={{ display: 'grid', placeItems: 'center', width: s.size + 10, height: s.size + 10, borderRadius: 8 }}>
              {(h) => live(h)}
            </Slot>
          ) : (
            <Placeholder key={i} w={s.size} h={s.size} line={c.ph} />
          ),
        )}
      </div>
    )
  } else if (s.mock === 'sidebar') {
    mock = (
      <div style={{ display: 'flex', minHeight: 230 }}>
        <div style={{ width: s.fill ? '100%' : 150, padding: 8, background: c.surface, borderRight: s.fill ? 0 : `1px solid ${c.line}`, display: 'flex', flexDirection: 'column', gap: 4 }}>
          {s.fill ? (
            <Slot onHover style={{ flex: 1, display: 'grid', placeItems: 'center' }}>
              {(h) => live(h, '100%', { height: 214 })}
            </Slot>
          ) : (
            Array.from({ length: s.count }, (_, i) =>
              i === slot ? (
                <Slot key={i} onHover style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '4px 6px', borderRadius: 7, background: s.dark ? '#24242a' : '#f1f1ee' }}>
                  {(h) => (
                    <>
                      {live(h)}
                      <span style={{ fontSize: 12, fontWeight: 600, color: c.text }}>{s.label}</span>
                    </>
                  )}
                </Slot>
              ) : (
                <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '4px 6px' }}>
                  <Placeholder w={s.size} h={s.size} line={c.ph} />
                  <Placeholder w="100%" h={10} line={c.ph} />
                </div>
              ),
            )
          )}
        </div>
        {!s.fill && (
          <div style={{ flex: 1, padding: 12, display: 'grid', alignContent: 'start', gap: 8 }}>
            <Placeholder w="100%" h={16} line={c.ph} />
            <Placeholder w="100%" h={90} line={c.ph} />
            <Placeholder w="70%" h={12} line={c.ph} />
          </div>
        )}
      </div>
    )
  } else {
    mock = (
      <div style={{ padding: 14, display: 'grid', placeItems: 'center' }}>
        <div style={{ width: '100%', maxWidth: 300, background: c.surface, border: `1px solid ${c.line}`, borderRadius: 12, overflow: 'hidden' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: 10 }}>
            <div style={{ width: 26, height: 26, borderRadius: '50%', background: 'linear-gradient(135deg,#f97316,#db2777)' }} />
            <div style={{ flex: 1, display: 'grid', gap: 4 }}>
              <Placeholder w="60%" h={8} line={c.ph} />
              <Placeholder w="35%" h={6} line={c.ph} />
            </div>
          </div>
          <div style={{ display: 'grid' }}>
            <Placeholder w="100%" h={120} line={c.ph} />
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '6px 10px', borderTop: `1px solid ${c.line}`, minHeight: s.size + 14 }}>
            {s.fill ? (
              <Slot onHover style={{ flex: 1, display: 'grid', placeItems: 'center' }}>
                {(h) => live(h, '100%', { height: Math.max(s.size + 8, 44) })}
              </Slot>
            ) : (
              Array.from({ length: s.count }, (_, i) =>
                i === slot ? (
                  <Slot key={i} onHover style={{ display: 'flex', alignItems: 'center', gap: 3, padding: '2px 6px 2px 2px', borderRadius: 7 }}>
                    {(h) => (
                      <>
                        {live(h)}
                        {s.label && <span style={{ fontSize: 12, fontWeight: 600, color: c.text }}>{s.label}</span>}
                      </>
                    )}
                  </Slot>
                ) : (
                  <Placeholder key={i} w={s.size} h={s.size} line={c.ph} />
                ),
              )
            )}
          </div>
        </div>
      </div>
    )
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
        {(['nav', 'sidebar', 'post'] as const).map((m) => (
          <button key={m} onClick={() => set({ mock: m })} className={chip(s.mock === m)}>
            {m === 'nav' ? 'Nav bar' : m === 'sidebar' ? 'Sidebar' : 'Post bar'}
          </button>
        ))}
        <div className="flex-1" />
        <button onClick={() => set({ dark: !s.dark })} className={chip(false)} title="Preview on a light or dark page">
          {s.dark ? 'Dark' : 'Light'}
        </button>
      </div>

      <div className="overflow-hidden rounded-md border border-panel-border" style={{ background: c.page }}>
        {mock}
      </div>
      {error && <p className="text-[10px] text-red-400">{error}</p>}
      {file && file.skipped.length > 0 && <p className="text-[10px] leading-snug text-ink-dim">Lite leaves out: {file.skipped.slice(0, 3).join('; ')}{file.skipped.length > 3 ? '…' : ''}</p>}

      <div className="flex flex-col gap-1.5 rounded-md border border-panel-border p-2">
        {s.mock !== 'nav' && (
          <div className="flex items-center gap-1 text-[11px] text-ink-dim">
            Layout
            <button onClick={() => set({ fill: false })} className={chip(!s.fill)}>
              {s.mock === 'sidebar' ? 'One item' : 'One action'}
            </button>
            <button onClick={() => set({ fill: true })} className={chip(s.fill)}>
              {s.mock === 'sidebar' ? 'Fill sidebar' : 'Fill bar'}
            </button>
          </div>
        )}
        {!s.fill || s.mock === 'nav' ? (
          <>
            {numberRow(s.mock === 'sidebar' ? 'Rows' : 'Items', s.count, 1, 8, (v) => set({ count: v }))}
            {numberRow('Your slot', slot + 1, 1, s.count, (v) => set({ slot: v - 1 }))}
            {numberRow('Icon size', s.size, 16, 64, (v) => set({ size: v }))}
            {s.mock !== 'nav' && (
              <label className="flex items-center justify-between gap-2 text-[11px] text-ink-dim">
                Label
                <input value={s.label} onChange={(e) => set({ label: e.target.value })} className="w-32 rounded border border-panel-border bg-transparent px-1.5 py-0.5 text-ink" />
              </label>
            )}
          </>
        ) : null}
        <label className="flex items-center gap-2 text-[11px] text-ink-dim">
          <input type="checkbox" checked={s.rowHover} onChange={(e) => set({ rowHover: e.target.checked })} />
          Hovering the whole {s.mock === 'sidebar' ? 'row' : 'slot'} drives <code className="text-ink">hover</code>
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
      <p className="text-[10px] leading-snug text-ink-dim">Shows this scene as Toasty Lite, live: edits appear here a moment after you make them. Click and hover the animation in the mock to test its listeners.</p>
    </div>
  )
}
