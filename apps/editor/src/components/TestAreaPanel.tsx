/**
 * **Test Area** — try the scene you're editing inside real-looking UI
 * before you ship it. The current scene is exported to Toasty Lite a
 * moment after every edit and dropped, fully interactive, into a mock:
 *
 * - **Nav bar** — the editor's *own* title bar hosts it (top right), at a
 *   size you choose: the most honest "how does this icon feel in a real
 *   nav" test there is.
 * - **Sidebar**, **Post** (a social post) and **Phone** (an app screen,
 *   including full screen) — each made of named elements, and a
 *   **Replace** dropdown picks which element your animation takes the
 *   place of: a row icon, the whole sidebar, the avatar, the photo, one
 *   action, the whole action bar, a tab, the floating button, the screen…
 *
 * **Hi-fi** swaps the crossed-out wireframe boxes for real photos (a random
 * image service, seeded so they stay put) and placeholder text, so the
 * animation is judged against a believable page. Light / dark, item
 * counts, size, label, "row hover drives hover" and the scene's own inputs
 * are all adjustable, and **Show in Playground** keeps the panel up while
 * the scene plays.
 */
import { useEffect, useMemo, useState, type CSSProperties, type ReactNode } from 'react'
import type { WasmEditor } from '../ipc/wasmEditor'
import type { DocumentSummary, OpaqueId } from '../ipc/document'
import type { LiteFile, LitePlayer } from '../lite/runtime'
import { LiteLive } from './LiteLive'
import { testAreaStore } from '../game/testAreaStore'

type Mock = 'nav' | 'sidebar' | 'post' | 'phone'

interface Settings {
  mock: Mock
  /** The element replaced, per mock. */
  slot: Record<Exclude<Mock, 'nav'>, string>
  count: number
  size: number
  navSize: number
  label: string
  dark: boolean
  hifi: boolean
  rowHover: boolean
  showInPlayground: boolean
  /** Full-size slots: crop to fill (`cover`) or fit inside (`contain`). */
  cover: boolean
}

const DEFAULTS: Settings = {
  mock: 'sidebar',
  slot: { sidebar: 'row:1', post: 'action:0', phone: 'tab:1' },
  count: 4,
  size: 28,
  navSize: 24,
  label: 'Likes',
  dark: false,
  hifi: false,
  rowHover: true,
  showInPlayground: false,
  cover: false,
}

function load(): Settings {
  try {
    const saved = JSON.parse(localStorage.getItem('an1.testArea.v2') ?? '{}') as Partial<Settings>
    return { ...DEFAULTS, ...saved, slot: { ...DEFAULTS.slot, ...(saved.slot ?? {}) } }
  } catch {
    return DEFAULTS
  }
}

const LOREM = [
  'Lorem ipsum dolor sit amet, consectetur adipiscing elit.',
  'Sed do eiusmod tempor incididunt ut labore et dolore.',
  'Ut enim ad minim veniam, quis nostrud exercitation.',
  'Duis aute irure dolor in reprehenderit in voluptate.',
]
const NAV_LABELS = ['Home', 'Search', 'Messages', 'Library', 'Saved', 'Profile', 'Settings', 'Help']
/** A seeded photo from a free random-image service (stays the same per seed). */
const photo = (seed: string, w: number, h: number) => `https://picsum.photos/seed/toasty-${seed}/${w}/${h}`

/** Element choices per mock (what `Replace` can swap out). */
function slotOptions(mock: Exclude<Mock, 'nav'>, count: number): [string, string][] {
  if (mock === 'sidebar') return [...Array.from({ length: count }, (_, i): [string, string] => [`row:${i}`, `Row ${i + 1} icon`]), ['title', 'Page title'], ['hero', 'Page image'], ['fill', 'Whole sidebar']]
  if (mock === 'post') return [['avatar', 'Avatar'], ['name', 'Author name'], ['media', 'Photo'], ...Array.from({ length: count }, (_, i): [string, string] => [`action:${i}`, `Action ${i + 1}`]), ['bar', 'Whole action bar']]
  return [['screen', 'Full screen'], ['header', 'Header button'], ['card', 'Card image'], ...Array.from({ length: 4 }, (_, i): [string, string] => [`tab:${i}`, `Tab ${i + 1}`]), ['fab', 'Floating button']]
}

/** A wireframe placeholder: a box with both diagonals crossed. */
function Wire({ w, h, line, radius = 6 }: { w: number | string; h: number | string; line: string; radius?: number }) {
  return (
    <svg width={w} height={h} style={{ display: 'block', flex: 'none', maxWidth: '100%' }} preserveAspectRatio="none" viewBox="0 0 100 100">
      <rect x="1" y="1" width="98" height="98" rx={radius} fill="none" stroke={line} strokeWidth="2" vectorEffect="non-scaling-stroke" />
      <line x1="4" y1="4" x2="96" y2="96" stroke={line} strokeWidth="1.2" vectorEffect="non-scaling-stroke" />
      <line x1="96" y1="4" x2="4" y2="96" stroke={line} strokeWidth="1.2" vectorEffect="non-scaling-stroke" />
    </svg>
  )
}

/** A hoverable slot reporting hover to its child (drives `hover`). */
function Hover({ children, style }: { children: (hover: boolean) => ReactNode; style?: CSSProperties }) {
  const [hover, setHover] = useState(false)
  return (
    <div onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)} style={{ cursor: 'pointer', ...style }}>
      {children(hover)}
    </div>
  )
}

export function TestAreaPanel({ editor, doc, artboardId }: { editor: WasmEditor | null; doc: DocumentSummary | null; artboardId: OpaqueId | null }) {
  const [s, setS] = useState<Settings>(load)
  const set = (patch: Partial<Settings>) =>
    setS((prev) => {
      const next = { ...prev, ...patch }
      try {
        localStorage.setItem('an1.testArea.v2', JSON.stringify(next))
      } catch {
        // Not remembered; still applies.
      }
      return next
    })
  const [file, setFile] = useState<LiteFile | null>(null)
  const [error, setError] = useState<string | null>(null)
  const players = useMemo(() => new Set<LitePlayer>(), [])
  const [, bump] = useState(0)

  useEffect(() => {
    if (!editor || !artboardId) return
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
  }, [editor, doc, artboardId])

  // The title bar slot and Playground visibility live outside the panel.
  useEffect(() => {
    testAreaStore.set({ nav: { active: s.mock === 'nav', size: s.navSize }, showInPlayground: s.showInPlayground })
  }, [s.mock, s.navSize, s.showInPlayground])
  useEffect(() => () => testAreaStore.set({ nav: { active: false, size: s.navSize } }), [])

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

  const c = s.dark
    ? { page: '#0f0f12', surface: '#17171b', soft: '#202026', line: '#2c2c33', text: '#ececef', muted: '#8d8d97', ph: '#3a3a42', ink: [0.93, 0.93, 0.94, 1] }
    : { page: '#f4f4f2', surface: '#ffffff', soft: '#f1f1ee', line: '#e6e5e1', text: '#18181b', muted: '#74737a', ph: '#d4d3ce', ink: [0.1, 0.1, 0.12, 1] }
  const mockKey = s.mock === 'nav' ? null : s.mock
  const selected = mockKey ? s.slot[mockKey] : ''
  const live = (hover: boolean, w: number | string, h: number | string, fit?: 'cover' | 'contain') =>
    file ? <LiteLive file={file} width={w} height={h} ink={c.ink} hover={s.rowHover ? hover : undefined} players={players} fit={fit} /> : <div style={{ width: w, height: h }} />

  /** Element `id`: the animation if it's the chosen slot, else `fallback`. */
  const el = (id: string, w: number | string, h: number | string, fallback: ReactNode, style?: CSSProperties) =>
    id === selected ? (
      <Hover style={{ display: 'grid', placeItems: 'center', width: w, height: h, flex: 'none', ...style }}>{(hv) => live(hv, w, h, s.cover && ['media', 'card', 'hero'].includes(id) ? 'cover' : undefined)}</Hover>
    ) : (
      fallback
    )
  const img = (seed: string, w: number | string, h: number | string, radius = 8) =>
    s.hifi ? <img src={photo(seed, 480, 320)} alt="" style={{ width: w, height: h, objectFit: 'cover', borderRadius: radius, display: 'block', flex: 'none' }} /> : <Wire w={w} h={h} line={c.ph} />
  const text = (i: number, w: number | string, size = 11, weight = 400) =>
    s.hifi ? <span style={{ fontSize: size, fontWeight: weight, color: weight > 500 ? c.text : c.muted, lineHeight: 1.35, display: 'block' }}>{LOREM[i % LOREM.length]}</span> : <Wire w={w} h={Math.max(6, size - 3)} line={c.ph} radius={2} />
  const glyph = (size: number) => (s.hifi ? <div style={{ width: size * 0.8, height: size * 0.8, borderRadius: size * 0.25, background: c.soft, border: `1.5px solid ${c.ph}` }} /> : <Wire w={size} h={size} line={c.ph} />)

  let mock: ReactNode = null
  if (s.mock === 'nav') {
    mock = (
      <div style={{ padding: 16, color: c.muted, fontSize: 12, lineHeight: 1.5 }}>
        Your animation is in the editor's own title bar, top right, next to Docs. Point at it, click it, and change the size below.
      </div>
    )
  } else if (s.mock === 'sidebar') {
    const fill = selected === 'fill'
    mock = (
      <div style={{ display: 'flex', minHeight: 240 }}>
        <div style={{ width: fill ? '100%' : 150, padding: 8, background: c.surface, borderRight: fill ? 0 : `1px solid ${c.line}`, display: 'flex', flexDirection: 'column', gap: 4 }}>
          {fill ? (
            <Hover style={{ flex: 1, display: 'grid', placeItems: 'center' }}>{(hv) => live(hv, '100%', 224, s.cover ? 'cover' : undefined)}</Hover>
          ) : (
            Array.from({ length: s.count }, (_, i) => {
              const mine = selected === `row:${i}`
              const row = (
                <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '4px 6px', borderRadius: 7, background: mine ? c.soft : 'transparent' }}>
                  {mine ? null : glyph(s.size)}
                  {s.hifi || mine ? <span style={{ fontSize: 12, fontWeight: mine ? 600 : 500, color: mine ? c.text : c.muted }}>{mine ? s.label : NAV_LABELS[i % NAV_LABELS.length]}</span> : <Wire w="100%" h={8} line={c.ph} radius={2} />}
                </div>
              )
              return mine ? (
                <Hover key={i} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '4px 6px', borderRadius: 7, background: c.soft }}>
                  {(hv) => (
                    <>
                      {live(hv, s.size, s.size)}
                      <span style={{ fontSize: 12, fontWeight: 600, color: c.text }}>{s.label}</span>
                    </>
                  )}
                </Hover>
              ) : (
                <div key={i}>{row}</div>
              )
            })
          )}
        </div>
        {!fill && (
          <div style={{ flex: 1, padding: 12, display: 'grid', alignContent: 'start', gap: 8, minWidth: 0 }}>
            {el('title', '100%', 28, s.hifi ? <span style={{ fontSize: 15, fontWeight: 700, color: c.text }}>Northwind journal</span> : <Wire w="70%" h={14} line={c.ph} radius={2} />)}
            {el('hero', '100%', 96, img('hero', '100%', 96))}
            {text(0, '100%')}
            {text(1, '80%')}
          </div>
        )}
      </div>
    )
  } else if (s.mock === 'post') {
    const bar = selected === 'bar'
    mock = (
      <div style={{ padding: 14, display: 'grid', placeItems: 'center' }}>
        <div style={{ width: '100%', maxWidth: 300, background: c.surface, border: `1px solid ${c.line}`, borderRadius: 12, overflow: 'hidden' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: 10 }}>
            {el('avatar', 30, 30, s.hifi ? <img src={photo('avatar', 80, 80)} alt="" style={{ width: 30, height: 30, borderRadius: '50%', objectFit: 'cover' }} /> : <div style={{ width: 30, height: 30, borderRadius: '50%', background: 'linear-gradient(135deg,#f97316,#db2777)' }} />)}
            <div style={{ flex: 1, display: 'grid', gap: 3, minWidth: 0 }}>
              {el('name', 90, 18, s.hifi ? <span style={{ fontSize: 12, fontWeight: 700, color: c.text }}>Maya Chen</span> : <Wire w="60%" h={8} line={c.ph} radius={2} />, { justifySelf: 'start' })}
              {s.hifi ? <span style={{ fontSize: 10.5, color: c.muted }}>2 h · Lisbon</span> : <Wire w="35%" h={6} line={c.ph} radius={2} />}
            </div>
          </div>
          {el('media', '100%', 150, img('post', '100%', 150, 0))}
          {s.hifi && <div style={{ padding: '8px 10px 0' }}>{text(2, '100%', 11.5)}</div>}
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '6px 10px', borderTop: s.hifi ? 0 : `1px solid ${c.line}`, minHeight: s.size + 14 }}>
            {bar ? (
              <Hover style={{ flex: 1, display: 'grid', placeItems: 'center' }}>{(hv) => live(hv, '100%', Math.max(s.size + 8, 44))}</Hover>
            ) : (
              Array.from({ length: s.count }, (_, i) =>
                selected === `action:${i}` ? (
                  <Hover key={i} style={{ display: 'flex', alignItems: 'center', gap: 3, padding: '2px 6px 2px 2px', borderRadius: 7 }}>
                    {(hv) => (
                      <>
                        {live(hv, s.size, s.size)}
                        {s.label && <span style={{ fontSize: 12, fontWeight: 600, color: c.text }}>{s.label}</span>}
                      </>
                    )}
                  </Hover>
                ) : (
                  <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                    {glyph(s.size)}
                    {s.hifi && <span style={{ fontSize: 11.5, color: c.muted }}>{[24, 8, 3, 12][i % 4]}</span>}
                  </div>
                ),
              )
            )}
          </div>
        </div>
      </div>
    )
  } else {
    // Phone: an app screen; any element (or the whole screen) can be yours.
    const full = selected === 'screen'
    mock = (
      <div style={{ padding: 14, display: 'grid', placeItems: 'center' }}>
        <div style={{ width: 230, height: 470, borderRadius: 34, padding: 8, background: s.dark ? '#000' : '#1c1c20', boxShadow: '0 10px 30px rgb(0 0 0 / 0.25)' }}>
          <div style={{ position: 'relative', width: '100%', height: '100%', borderRadius: 27, overflow: 'hidden', background: c.page, display: 'flex', flexDirection: 'column' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '8px 16px 4px', fontSize: 10, fontWeight: 600, color: c.text }}>
              <span>9:41</span>
              <span style={{ width: 60, height: 16, borderRadius: 10, background: '#000' }} />
              <span>100%</span>
            </div>
            {full ? (
              <Hover style={{ flex: 1, display: 'grid' }}>{(hv) => live(hv, '100%', '100%', s.cover ? 'cover' : undefined)}</Hover>
            ) : (
              <>
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '6px 14px' }}>
                  <span style={{ fontSize: 16, fontWeight: 700, color: c.text }}>{s.hifi ? 'Discover' : ''}</span>
                  {!s.hifi && <Wire w={70} h={12} line={c.ph} radius={2} />}
                  {el('header', 28, 28, glyph(26))}
                </div>
                <div style={{ flex: 1, overflow: 'hidden', padding: '4px 12px', display: 'grid', alignContent: 'start', gap: 10 }}>
                  {[0, 1].map((k) => (
                    <div key={k} style={{ background: c.surface, border: `1px solid ${c.line}`, borderRadius: 12, overflow: 'hidden' }}>
                      {k === 0 ? el('card', '100%', 110, img('card0', '100%', 110, 0)) : img('card1', '100%', 80, 0)}
                      <div style={{ padding: 8, display: 'grid', gap: 4 }}>
                        {s.hifi ? <span style={{ fontSize: 12, fontWeight: 700, color: c.text }}>{k === 0 ? 'Weekend in Porto' : 'Coffee guide'}</span> : <Wire w="65%" h={9} line={c.ph} radius={2} />}
                        {text(k + 1, '90%', 10.5)}
                      </div>
                    </div>
                  ))}
                </div>
                {el('fab', 44, 44, null, { position: 'absolute', right: 14, bottom: 66 })}
                {selected !== 'fab' && <div style={{ position: 'absolute', right: 14, bottom: 66, width: 44, height: 44, borderRadius: '50%', background: '#e8590c', boxShadow: '0 4px 12px rgb(232 89 12 / 0.4)' }} />}
                <div style={{ display: 'flex', justifyContent: 'space-around', alignItems: 'center', padding: '8px 6px 14px', borderTop: `1px solid ${c.line}`, background: c.surface }}>
                  {Array.from({ length: 4 }, (_, i) => (
                    <div key={i} style={{ display: 'grid', justifyItems: 'center', gap: 2 }}>
                      {el(`tab:${i}`, s.size, s.size, glyph(Math.min(s.size, 26)))}
                      {s.hifi && <span style={{ fontSize: 9, color: selected === `tab:${i}` ? c.text : c.muted }}>{selected === `tab:${i}` ? s.label : ['Home', 'Search', 'Saved', 'You'][i]}</span>}
                    </div>
                  ))}
                </div>
              </>
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
        {(['nav', 'sidebar', 'post', 'phone'] as const).map((m) => (
          <button key={m} onClick={() => set({ mock: m })} className={chip(s.mock === m)}>
            {m === 'nav' ? 'Nav bar' : m === 'sidebar' ? 'Sidebar' : m === 'post' ? 'Post' : 'Phone'}
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
        <label className="flex items-center gap-1 text-[11px] text-ink-dim" title="Keep this panel visible while the scene plays">
          <input type="checkbox" checked={s.showInPlayground} onChange={(e) => set({ showInPlayground: e.target.checked })} />
          In Playground
        </label>
      </div>

      <div className="overflow-hidden rounded-md border border-panel-border" style={{ background: c.page }}>
        {mock}
      </div>
      {error && <p className="text-[10px] text-red-400">{error}</p>}
      {file && file.skipped.length > 0 && (
        <p className="text-[10px] leading-snug text-ink-dim">
          Lite leaves out: {file.skipped.slice(0, 3).join('; ')}
          {file.skipped.length > 3 ? '…' : ''}
        </p>
      )}

      <div className="flex flex-col gap-1.5 rounded-md border border-panel-border p-2">
        {mockKey ? (
          <label className="flex items-center justify-between gap-2 text-[11px] text-ink-dim">
            Replace
            <select value={selected} onChange={(e) => set({ slot: { ...s.slot, [mockKey]: e.target.value } })} className="w-40 rounded border border-panel-border bg-solid-panel px-1 py-0.5 text-ink">
              {slotOptions(mockKey, s.count).map(([id, label]) => (
                <option key={id} value={id}>
                  {label}
                </option>
              ))}
            </select>
          </label>
        ) : (
          numberRow('Size in title bar', s.navSize, 14, 40, (v) => set({ navSize: v }))
        )}
        {['screen', 'fill', 'bar', 'media', 'card', 'hero'].includes(selected) && (
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
        {mockKey && mockKey !== 'phone' && numberRow(mockKey === 'sidebar' ? 'Rows' : 'Actions', s.count, 1, 8, (v) => set({ count: v }))}
        {mockKey && numberRow('Icon size', s.size, 16, 64, (v) => set({ size: v }))}
        {mockKey && (
          <label className="flex items-center justify-between gap-2 text-[11px] text-ink-dim">
            Label
            <input value={s.label} onChange={(e) => set({ label: e.target.value })} className="w-32 rounded border border-panel-border bg-transparent px-1.5 py-0.5 text-ink" />
          </label>
        )}
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
      <p className="text-[10px] leading-snug text-ink-dim">This scene as Toasty Lite, live: edits show up here a moment after you make them. Click and hover it in the mock to test its listeners.</p>
    </div>
  )
}
