/**
 * The Test Area's mock app — **one** believable product ("Northwind") that
 * your animation is dropped into, instead of a handful of separate mocks.
 * It has the parts real UI is made of — a sidebar (a slide-out drawer on
 * phone), a top bar, tab bar, floating button, feed posts — and three pages,
 * each with its own feel so an animation can be judged in different
 * settings:
 *
 * - **Home**    — a social feed: stories, a composer, posts with actions,
 *                 (desktop) a trending / who-to-follow rail. Friendly,
 *                 rounded, sans.
 * - **Article** — a long read: serif type on paper, byline, hero, pull
 *                 quote, inline figure, a floating applause pill.
 * - **Code**    — a developer page: monospace, a dark editor with file
 *                 tabs and line numbers, Run / Copy, a preview pane and a
 *                 status bar.
 *
 * Shown as a **phone** (frame on a desk) or **desktop** (a browser window).
 * Every element has real `:hover` / `:active` states (one scoped stylesheet,
 * `.ts-…` classes) and the app's own nav switches pages. Any element listed
 * by `testSlots` can be replaced by the live animation.
 *
 * Used twice: scaled down inside the Test Area panel, and full size as the
 * Playground stage (`Use as Playground`).
 */
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react'
import type { TestDevice, TestPage, TestSettings } from '../game/testAreaStore'

export const NAV = ['Home', 'Read', 'Code', 'Saved', 'Profile'] as const
const NAV_PAGE: (TestPage | null)[] = ['home', 'article', 'code', null, null]

/** Slots shared by every page (app chrome). */
const CHROME: [string, string][] = [
  ['logo', 'App logo'],
  ['header', 'Notifications button'],
  ...NAV.map((n, i): [string, string] => [`nav:${i}`, `${n} nav icon`]),
  ['sidebar', 'Whole sidebar / drawer'],
  ['fab', 'Floating button'],
  ['content', 'Whole page'],
]

/** The `Replace` choices: `[group, [id, label][]][]`. */
export function testSlots(page: TestPage, count: number): [string, [string, string][]][] {
  const own: Record<TestPage, [string, string][]> = {
    home: [['story', 'Your story'], ['avatar', 'Post avatar'], ['name', 'Post author'], ['media', 'Post photo'], ...Array.from({ length: count }, (_, i): [string, string] => [`action:${i}`, `Post action ${i + 1}`]), ['bar', 'Whole action bar']],
    article: [['author', 'Author avatar'], ['hero', 'Hero image'], ['figure', 'Inline figure'], ['clap', 'Applause button'], ['share', 'Share button']],
    code: [['run', 'Run button'], ['copy', 'Copy button'], ['status', 'Build status'], ['preview', 'Preview pane']],
  }
  return [
    [page === 'home' ? 'Home page' : page === 'article' ? 'Article page' : 'Code page', own[page]],
    ['Everywhere', CHROME],
  ]
}

/** Slots that fill an area (vs. icon-sized) — these get the Fit option. */
export const BIG_SLOTS = ['content', 'sidebar', 'media', 'hero', 'figure', 'preview', 'bar']

const photo = (seed: string, w: number, h: number) => `https://picsum.photos/seed/toasty-${seed}/${w}/${h}`
const LOREM = [
  'Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore.',
  'Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo.',
  'Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur.',
  'Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim.',
]
const PEOPLE = [
  ['Maya Chen', 'mayac'],
  ['Jonas Weber', 'jonas'],
  ['Amara Okafor', 'amara'],
  ['Leo Martins', 'leom'],
  ['Sofia Rossi', 'sofia'],
  ['Kenji Ito', 'kenji'],
]

/** Stroke icons (24-unit box), so the chrome reads as a real app. */
const ICONS: Record<string, string> = {
  home: 'M3 10.5 12 3l9 7.5V21h-6v-6H9v6H3z',
  read: 'M2 5h7a3 3 0 0 1 3 3v12a2 2 0 0 0-2-2H2zM22 5h-7a3 3 0 0 0-3 3v12a2 2 0 0 1 2-2h8z',
  code: 'M8 7l-5 5 5 5M16 7l5 5-5 5M14 4l-4 16',
  saved: 'M6 3h12v18l-6-4-6 4z',
  profile: 'M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM4 21a8 8 0 0 1 16 0',
  heart: 'M12 20s-7-4.5-7-10a4 4 0 0 1 7-2.6A4 4 0 0 1 19 10c0 5.5-7 10-7 10z',
  comment: 'M4 5h16v11H9l-5 4z',
  repost: 'M7 7h11l-3-3M17 17H6l3 3M18 7v5M6 17v-5',
  send: 'M22 2 11 13M22 2l-7 20-4-9-9-4z',
  search: 'M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM21 21l-4.3-4.3',
  bell: 'M6 16v-5a6 6 0 0 1 12 0v5l2 2H4zM10 21h4',
  menu: 'M4 6h16M4 12h16M4 18h16',
  plus: 'M12 5v14M5 12h14',
  play: 'M7 4v16l13-8z',
  copy: 'M9 9h11v11H9zM5 15H4V4h11v1',
  check: 'M5 12l5 5 9-10',
  more: 'M5 12h.01M12 12h.01M19 12h.01',
  share: 'M12 3v13M7 8l5-5 5 5M5 14v6h14v-6',
  clap: 'M12 3v3M6 5l2 2M18 5l-2 2M8 21l-3-6 2-5 3 3 2-6 2 6 3-3 2 5-3 6z',
}
const ACTION_ICONS = ['heart', 'comment', 'repost', 'send', 'saved', 'share', 'more', 'bell']
const NAV_ICONS = ['home', 'read', 'code', 'saved', 'profile']

function Icon({ name, size = 20, style }: { name: string; size?: number; style?: CSSProperties }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={name === 'more' ? 3 : 1.8} strokeLinecap="round" strokeLinejoin="round" style={{ flex: 'none', display: 'block', ...style }} aria-hidden="true">
      <path d={ICONS[name] ?? ICONS.more} />
    </svg>
  )
}

/** Palettes: the shared app palette plus each page's own "feel". */
function palette(dark: boolean) {
  return dark
    ? { page: '#0c0c0f', surface: '#141418', hover: '#22222a', line: '#26262d', text: '#ededf0', muted: '#9090a0', accent: '#7c6cff', ph: '#34343d', paper: '#17150f', paperText: '#ebe4d4', paperMuted: '#a39b89', paperLine: '#2c281f' }
    : { page: '#f6f6f4', surface: '#ffffff', hover: '#f0f0ec', line: '#e7e6e2', text: '#16161a', muted: '#6c6c76', accent: '#4f46e5', ph: '#d2d1cc', paper: '#fbf8f1', paperText: '#1e1b15', paperMuted: '#6f675a', paperLine: '#e8e1d2' }
}

function sheet(dark: boolean) {
  const c = palette(dark)
  return `
.ts{position:absolute;inset:0;overflow:auto;background:${c.page};color:${c.text};font:14px/1.5 "Segoe UI",system-ui,-apple-system,sans-serif;-webkit-font-smoothing:antialiased}
.ts *{box-sizing:border-box}
.ts p{margin:0}
.ts-muted{color:${c.muted}}
.ts-btn{display:inline-flex;align-items:center;justify-content:center;gap:6px;height:36px;padding:0 14px;border-radius:9px;border:1px solid ${c.line};background:${c.surface};color:${c.text};font:inherit;font-weight:600;cursor:pointer;transition:background .15s,transform .1s,border-color .15s,filter .15s;white-space:nowrap}
.ts-btn:hover{background:${c.hover};border-color:${c.muted}55}
.ts-btn:active{transform:scale(.96)}
.ts-btn.primary{background:${c.accent};border-color:transparent;color:#fff}
.ts-btn.primary:hover{filter:brightness(1.12)}
.ts-row{display:flex;align-items:center;gap:12px;padding:9px 12px;border-radius:10px;color:${c.muted};font-weight:500;cursor:pointer;transition:background .15s,color .15s;user-select:none}
.ts-row:hover{background:${c.hover};color:${c.text}}
.ts-row.on{background:${c.hover};color:${c.text};font-weight:600}
.ts-card{background:${c.surface};border:1px solid ${c.line};border-radius:16px;overflow:hidden;transition:transform .2s,box-shadow .2s,border-color .2s}
.ts-card.lift{cursor:pointer}
.ts-card.lift:hover{transform:translateY(-3px);box-shadow:0 14px 34px rgb(0 0 0 / ${dark ? 0.45 : 0.12});border-color:${c.muted}44}
.ts-icon{display:grid;place-items:center;width:38px;height:38px;border-radius:10px;cursor:pointer;color:${c.muted};transition:background .15s,color .15s;flex:none}
.ts-icon:hover{background:${c.hover};color:${c.text}}
.ts-icon:active{transform:scale(.94)}
.ts-link{cursor:pointer;transition:color .15s}
.ts-link:hover{color:${c.accent}}
.ts-input{height:38px;border-radius:10px;border:1px solid ${c.line};background:${c.page};color:${c.text};padding:0 12px;font:inherit;width:100%;outline:none;transition:border-color .15s,box-shadow .15s}
.ts-input:focus{border-color:${c.accent};box-shadow:0 0 0 3px ${c.accent}33}
.ts-action{display:inline-flex;align-items:center;gap:6px;padding:6px 10px;border-radius:999px;color:${c.muted};cursor:pointer;font-weight:500;transition:background .15s,color .15s;user-select:none}
.ts-action:hover{background:${c.hover};color:${c.text}}
.ts-action.on{color:#e0245e}
.ts-img{display:block;width:100%;object-fit:cover;background:${c.hover}}
.ts-tab{display:grid;justify-items:center;gap:2px;font-size:10.5px;color:${c.muted};cursor:pointer;padding:4px 8px;border-radius:10px;transition:background .15s,color .15s;min-width:54px;user-select:none}
.ts-tab:hover{color:${c.text}}
.ts-tab.on{color:${c.accent};font-weight:600}
.ts-chip{display:inline-flex;align-items:center;height:28px;padding:0 11px;border-radius:999px;border:1px solid ${c.line};color:${c.muted};font-size:12.5px;cursor:pointer;transition:background .15s,color .15s}
.ts-chip:hover{background:${c.hover};color:${c.text}}
.ts-stories{scrollbar-width:none}
.ts-stories::-webkit-scrollbar{display:none}
.ts-story{display:grid;justify-items:center;gap:5px;font-size:11.5px;color:${c.muted};cursor:pointer;flex:none;width:66px}
.ts-story .ring{padding:2.5px;border-radius:50%;background:conic-gradient(#f59e0b,#ef4444,#d946ef,#f59e0b);transition:transform .15s}
.ts-story:hover .ring{transform:scale(1.06)}
.ts-story img,.ts-story .ph{width:56px;height:56px;border-radius:50%;border:2.5px solid ${c.page};display:block;object-fit:cover;background:${c.hover}}
/* Article: paper, serif. */
.ts-paper{background:${c.paper};color:${c.paperText};font-family:"Iowan Old Style","Palatino Linotype",Georgia,serif}
.ts-paper .ts-muted{color:${c.paperMuted}}
.ts-paper .ts-body p{font-size:19px;line-height:1.7;margin:0 0 22px}
.ts-paper .ts-pill{position:sticky;bottom:18px;margin:0 auto;width:max-content;display:flex;align-items:center;gap:4px;padding:6px 10px;border-radius:999px;background:${dark ? '#24211a' : '#fff'};border:1px solid ${c.paperLine};box-shadow:0 8px 24px rgb(0 0 0 / ${dark ? 0.5 : 0.1});font-family:"Segoe UI",system-ui,sans-serif;font-size:13px;color:${c.paperMuted}}
.ts-paper .ts-icon:hover,.ts-paper .ts-action:hover{background:${dark ? '#2a261d' : '#f1ece0'};color:${c.paperText}}
.ts-paper .ts-icon,.ts-paper .ts-action{color:${c.paperMuted}}
/* Code: monospace, dark editor whatever the theme. */
.ts-mono{font-family:"Cascadia Code","JetBrains Mono",Consolas,ui-monospace,monospace}
.ts-editor{background:#0f1117;color:#d6deeb;border-radius:14px;border:1px solid #1f2430;overflow:hidden}
.ts-editor .tabs{display:flex;background:#0b0d12;border-bottom:1px solid #1f2430;font-size:12.5px}
.ts-editor .tab{padding:9px 14px;color:#7d8590;cursor:pointer;border-right:1px solid #1f2430;transition:color .15s,background .15s}
.ts-editor .tab:hover{color:#d6deeb;background:#141821}
.ts-editor .tab.on{color:#e6edf3;background:#0f1117;box-shadow:inset 0 2px 0 ${c.accent}}
.ts-editor pre{margin:0;padding:14px 0;overflow:auto;font-size:13px;line-height:1.65}
.ts-editor .ln{display:inline-block;width:42px;padding-right:14px;text-align:right;color:#3d4452;user-select:none}
.ts-editor .k{color:#c792ea}.ts-editor .s{color:#a5d6a7}.ts-editor .f{color:#82aaff}.ts-editor .n{color:#f78c6c}.ts-editor .cm{color:#5c6773;font-style:italic}
.ts-editor .bar{display:flex;align-items:center;gap:6px;padding:8px 10px;border-top:1px solid #1f2430;background:#0b0d12}
.ts-editor .ebtn{display:inline-flex;align-items:center;gap:6px;height:30px;padding:0 10px;border-radius:7px;color:#9aa4b2;cursor:pointer;transition:background .15s,color .15s;font-size:12.5px;user-select:none}
.ts-editor .ebtn:hover{background:#1a1f2b;color:#e6edf3}
.ts-editor .ebtn.run{background:#2ea04326;color:#56d364}
.ts-editor .ebtn.run:hover{background:#2ea04340}
.ts-status{display:flex;align-items:center;gap:12px;padding:0 12px;height:26px;font-size:11.5px;background:${c.accent};color:#fff}
.ts-drawer-back{position:absolute;inset:0;background:rgb(0 0 0 / .35);z-index:8;animation:tsfade .2s}
.ts-drawer{position:absolute;top:0;bottom:0;left:0;width:78%;max-width:300px;z-index:9;background:${c.surface};box-shadow:8px 0 30px rgb(0 0 0 / .25);display:flex;flex-direction:column;padding:16px 12px;gap:2px;animation:tsslide .22s ease-out}
@keyframes tsfade{from{opacity:0}}
@keyframes tsslide{from{transform:translateX(-100%)}}
.ts-controls{position:absolute;right:16px;bottom:16px;max-width:calc(100% - 32px);z-index:20}
`
}

/** A wireframe placeholder: a box with both diagonals crossed. */
function Wire({ w, h, line, radius = 8 }: { w: number | string; h: number | string; line: string; radius?: number }) {
  return (
    <svg width={w} height={h} style={{ display: 'block', flex: 'none', maxWidth: '100%' }} preserveAspectRatio="none" viewBox="0 0 100 100">
      <rect x="1" y="1" width="98" height="98" rx={radius} fill="none" stroke={line} strokeWidth="2" vectorEffect="non-scaling-stroke" />
      <line x1="4" y1="4" x2="96" y2="96" stroke={line} strokeWidth="1.2" vectorEffect="non-scaling-stroke" />
      <line x1="96" y1="4" x2="4" y2="96" stroke={line} strokeWidth="1.2" vectorEffect="non-scaling-stroke" />
    </svg>
  )
}

/** A slot that tracks its own hover and hands it to the animation. */
function Hover({ className, style, children }: { className?: string; style?: CSSProperties; children: (hover: boolean) => ReactNode }) {
  const [hover, setHover] = useState(false)
  return (
    <div className={className} style={style} onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}>
      {children(hover)}
    </div>
  )
}

/** Scales a fixed-size `w`×`h` box to fit its container (never above `max`). */
function Fit({ w, h, max = 1, pad = 0, children }: { w: number; h: number; max?: number; pad?: number; children: ReactNode }) {
  const outer = useRef<HTMLDivElement>(null)
  const [box, setBox] = useState({ w: 0, h: 0 })
  useLayoutEffect(() => {
    const el = outer.current
    if (!el) return
    const measure = () => setBox({ w: el.clientWidth, h: el.clientHeight })
    measure()
    const ro = new ResizeObserver(measure)
    ro.observe(el)
    return () => ro.disconnect()
  }, [])
  const k = box.w && box.h ? Math.min(max, (box.w - pad * 2) / w, (box.h - pad * 2) / h) : 0
  return (
    <div ref={outer} style={{ position: 'absolute', inset: 0, display: 'grid', placeItems: 'center', overflow: 'hidden' }}>
      <div style={{ width: w * k, height: h * k, visibility: k ? 'visible' : 'hidden' }}>
        <div style={{ width: w, height: h, transform: `scale(${k})`, transformOrigin: '0 0' }}>{children}</div>
      </div>
    </div>
  )
}

/** Tiny highlighter for the Code page's snippet. */
function highlight(line: string): ReactNode[] {
  const out: ReactNode[] = []
  const re = /(\/\/.*$)|('[^']*')|\b(import|from|const|await|async|export|function|return|new)\b|\b(\d+(?:\.\d+)?)\b|(\w+)(?=\()/g
  let last = 0
  let m: RegExpExecArray | null
  while ((m = re.exec(line))) {
    if (m.index > last) out.push(line.slice(last, m.index))
    const cls = m[1] ? 'cm' : m[2] ? 's' : m[3] ? 'k' : m[4] ? 'n' : 'f'
    out.push(
      <span key={m.index} className={cls}>
        {m[0]}
      </span>,
    )
    last = m.index + m[0].length
  }
  if (last < line.length) out.push(line.slice(last))
  return out
}

const SNIPPET = `import { createPlayer } from '@toasty/player'

// One file, one state machine: the like button.
const heart = await createPlayer(el, {
  src: '/like.teab',
  stateMachine: 'Like',
})

button.addEventListener('click', () => {
  heart.fire('tap')
  heart.setInput('count', likes + 1)
})`

export interface TestAppProps {
  s: TestSettings
  /** Renders the animation (hover state, width, height, fit). */
  live: (hover: boolean, w: number | string, h: number | string, fit?: 'cover' | 'contain') => ReactNode
  /** The app's own nav was used to go to another page. */
  onPage: (page: TestPage) => void
  /** `panel` = scaled to the panel's width; `stage` = fills the Playground. */
  frame: 'panel' | 'stage'
  /** The inputs bar, shown floating at the bottom right (stage only). */
  controls?: ReactNode
}

export function TestApp({ s, live, onPage, frame, controls }: TestAppProps) {
  const c = palette(s.dark)
  const phone = s.device === 'phone'
  const slot = s.slot[s.page]
  const mine = (id: string) => slot === id
  const [drawer, setDrawer] = useState(false)
  const [liked, setLiked] = useState<Record<string, boolean>>({})
  const [tab, setTab] = useState(0)
  // Choosing the sidebar as the slot on a phone opens the drawer so it shows.
  useEffect(() => setDrawer(phone && slot === 'sidebar'), [phone, slot])

  const fit = s.cover ? 'cover' : undefined
  /** Element `id`: the animation when it's the chosen slot, else `fallback`. */
  const big = (id: string, w: number | string, h: number | string, fallback: ReactNode, style?: CSSProperties, className?: string) =>
    mine(id) ? (
      <Hover className={className} style={{ display: 'grid', placeItems: 'center', width: w, height: h, flex: 'none', ...style }}>
        {(hv) => live(hv, w, h, BIG_SLOTS.includes(id) ? fit : undefined)}
      </Hover>
    ) : (
      fallback
    )
  const img = (seed: string, w: number | string, h: number | string, radius = 0, style?: CSSProperties) =>
    s.hifi ? <img className="ts-img" src={photo(seed, 900, 600)} alt="" style={{ width: w, height: h, borderRadius: radius, ...style }} /> : <div style={{ width: w, height: h, ...style }}><Wire w="100%" h="100%" line={c.ph} radius={radius ? 8 : 0} /></div>
  const avatar = (seed: string, size: number) =>
    s.hifi ? <img src={photo(seed, 96, 96)} alt="" style={{ width: size, height: size, borderRadius: '50%', objectFit: 'cover', flex: 'none', display: 'block' }} /> : <div style={{ width: size, height: size, borderRadius: '50%', border: `1.5px solid ${c.ph}`, flex: 'none' }} />
  const para = (i: number, style?: CSSProperties) =>
    s.hifi ? (
      <p style={style}>{LOREM[i % LOREM.length]}</p>
    ) : (
      <div style={{ display: 'grid', gap: 6, margin: '4px 0 12px', ...style }}>
        {[100, 94, 62].map((w, k) => (
          <div key={k} style={{ height: 8, width: `${w}%`, borderRadius: 4, background: c.ph, opacity: 0.7 }} />
        ))}
      </div>
    )

  /** A hoverable icon slot wrapper: the slot gets `label` beside it. */
  const iconSlot = (id: string, className: string, name: string, size: number, extra?: ReactNode, onClick?: () => void, style?: CSSProperties) =>
    mine(id) ? (
      <Hover key={id} className={className} style={style}>
        {(hv) => (
          <>
            {live(hv, s.size, s.size)}
            {extra}
          </>
        )}
      </Hover>
    ) : (
      <div key={id} className={className} onClick={onClick} style={style}>
        <Icon name={name} size={size} />
        {extra}
      </div>
    )

  const logo = big('logo', 30, 30, <div style={{ width: 30, height: 30, borderRadius: 9, background: `linear-gradient(135deg, ${c.accent}, #ec4899)`, display: 'grid', placeItems: 'center', color: '#fff', fontWeight: 800, fontSize: 16, flex: 'none' }}>n</div>)
  const go = (i: number) => {
    const page = NAV_PAGE[i]
    if (page) onPage(page)
    setDrawer(false)
  }
  const active = NAV_PAGE.indexOf(s.page)

  // ---- Sidebar (desktop aside, phone drawer) ------------------------------
  const sidebarBody = mine('sidebar') ? (
    <Hover style={{ flex: 1, display: 'grid', placeItems: 'stretch' }}>{(hv) => live(hv, '100%', '100%', fit)}</Hover>
  ) : (
    <>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '2px 10px 18px' }}>
        {logo}
        <b style={{ fontSize: 17, letterSpacing: '-0.02em' }}>Northwind</b>
      </div>
      {NAV.map((n, i) => iconSlot(`nav:${i}`, `ts-row${i === active ? ' on' : ''}`, NAV_ICONS[i], 20, <span>{mine(`nav:${i}`) && s.label ? s.label : n}</span>, () => go(i)))}
      <div className="ts-muted" style={{ fontSize: 11, fontWeight: 600, letterSpacing: '0.06em', textTransform: 'uppercase', padding: '18px 12px 6px' }}>
        Collections
      </div>
      {['Design inspo', 'Weekend reads', 'Snippets'].map((t, i) => (
        <div key={t} className="ts-row" style={{ fontSize: 13.5 }}>
          <span style={{ width: 9, height: 9, borderRadius: 3, background: ['#f59e0b', '#10b981', '#3b82f6'][i] }} />
          {t}
        </div>
      ))}
      <div style={{ flex: 1 }} />
      <div className="ts-row" style={{ borderTop: `1px solid ${c.line}`, borderRadius: 0, paddingTop: 14 }}>
        {avatar('me', 32)}
        <span style={{ lineHeight: 1.25 }}>
          <b style={{ color: c.text, display: 'block', fontSize: 13 }}>Maya Chen</b>
          <span style={{ fontSize: 12 }}>Pro plan</span>
        </span>
      </div>
    </>
  )

  const bell = iconSlot('header', 'ts-icon', 'bell', 20)

  // ---- Pages -------------------------------------------------------------
  const actions = (k: number, yours: boolean) => (
    <div style={{ display: 'flex', alignItems: 'center', gap: 2, padding: '6px 8px 10px', minHeight: s.size + 18 }}>
      {yours && mine('bar') ? (
        <Hover style={{ flex: 1, display: 'grid', placeItems: 'center' }}>{(hv) => live(hv, '100%', Math.max(s.size + 10, 48), fit)}</Hover>
      ) : (
        Array.from({ length: yours ? s.count : 4 }, (_, i) => {
          const id = `action:${i}`
          const key = `${k}:${i}`
          const n = [248, 31, 12, 7, 3, 1, 0, 0][i] + (liked[key] ? 1 : 0)
          return yours && mine(id) ? (
            <Hover key={i} className="ts-action">
              {(hv) => (
                <>
                  {live(hv, s.size, s.size)}
                  {s.label && <span>{s.label}</span>}
                </>
              )}
            </Hover>
          ) : (
            <span key={i} className={`ts-action${liked[key] ? ' on' : ''}`} style={i === 4 ? { marginLeft: 'auto' } : undefined} onClick={() => setLiked((l) => ({ ...l, [key]: !l[key] }))}>
              <Icon name={ACTION_ICONS[i % ACTION_ICONS.length]} />
              {n > 0 && i < 4 && n}
            </span>
          )
        })
      )}
    </div>
  )
  const post = (k: number) => {
    const yours = k === 0
    const [name, handle] = PEOPLE[k % PEOPLE.length]
    const media = img(`post${k}`, '100%', phone ? 300 : 380)
    return (
      <article key={k} className="ts-card" style={{ marginBottom: phone ? 12 : 16, borderRadius: phone ? 0 : 16, borderLeft: phone ? 0 : undefined, borderRight: phone ? 0 : undefined }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: 14 }}>
          {yours ? big('avatar', 40, 40, avatar(`av${k}`, 40)) : avatar(`av${k}`, 40)}
          <div style={{ flex: 1, minWidth: 0, lineHeight: 1.3 }}>
            {yours ? big('name', 120, 22, <b className="ts-link">{name}</b>, { justifyContent: 'start' }) : <b className="ts-link">{name}</b>}
            <div className="ts-muted" style={{ fontSize: 12.5 }}>
              @{handle} · {k + 2}h
            </div>
          </div>
          <span className="ts-icon">
            <Icon name="more" />
          </span>
        </div>
        <div style={{ padding: '0 14px 12px' }}>{para(k)}</div>
        {yours ? big('media', '100%', phone ? 300 : 380, media) : media}
        {actions(k, yours)}
      </article>
    )
  }

  let content: ReactNode
  if (s.page === 'home') {
    content = (
      <div style={{ display: phone ? 'block' : 'grid', gridTemplateColumns: 'minmax(0, 600px) 300px', justifyContent: 'center', gap: 28, padding: phone ? '10px 0 90px' : '24px 28px' }}>
        <main style={{ minWidth: 0 }}>
          <div className="ts-stories" style={{ display: 'flex', gap: 6, overflowX: 'auto', padding: phone ? '4px 12px 14px' : '0 0 18px' }}>
            {mine('story') ? (
              <Hover className="ts-story">
                {(hv) => (
                  <>
                    <div style={{ width: 61, height: 61, display: 'grid', placeItems: 'center' }}>{live(hv, 56, 56)}</div>
                    <span>{s.label || 'Your story'}</span>
                  </>
                )}
              </Hover>
            ) : (
              <div className="ts-story">
                <div className="ring" style={{ background: c.line }}>
                  {s.hifi ? <img src={photo('me', 96, 96)} alt="" /> : <div className="ph" />}
                </div>
                <span>Your story</span>
              </div>
            )}
            {PEOPLE.map(([name], i) => (
              <div key={name} className="ts-story">
                <div className="ring">{s.hifi ? <img src={photo(`st${i}`, 96, 96)} alt="" /> : <div className="ph" />}</div>
                <span>{name.split(' ')[0]}</span>
              </div>
            ))}
          </div>
          {!phone && (
            <div className="ts-card" style={{ display: 'flex', gap: 10, padding: 14, marginBottom: 16, alignItems: 'center' }}>
              {avatar('me', 40)}
              <input className="ts-input" placeholder="What's happening?" />
              <button className="ts-btn primary">Post</button>
            </div>
          )}
          {[0, 1, 2].map(post)}
        </main>
        {!phone && (
          <aside style={{ position: 'sticky', top: 20, alignSelf: 'start', display: 'grid', gap: 16 }}>
            <div className="ts-card" style={{ padding: 16 }}>
              <b>Trending</b>
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginTop: 12 }}>
                {['#motion', '#toasty', '#ui', '#icons', '#gamedev', '#lottie'].map((t) => (
                  <span key={t} className="ts-chip">
                    {t}
                  </span>
                ))}
              </div>
            </div>
            <div className="ts-card" style={{ padding: 16 }}>
              <b>Who to follow</b>
              {PEOPLE.slice(1, 5).map(([name, handle], i) => (
                <div key={handle} style={{ display: 'flex', alignItems: 'center', gap: 10, marginTop: 14 }}>
                  {avatar(`f${i}`, 36)}
                  <div style={{ flex: 1, lineHeight: 1.25, minWidth: 0 }}>
                    <b className="ts-link" style={{ fontSize: 13 }}>
                      {name}
                    </b>
                    <div className="ts-muted" style={{ fontSize: 12 }}>
                      @{handle}
                    </div>
                  </div>
                  <button className="ts-btn" style={{ height: 30, padding: '0 12px', fontSize: 12 }}>
                    Follow
                  </button>
                </div>
              ))}
            </div>
          </aside>
        )}
      </div>
    )
  } else if (s.page === 'article') {
    const pad = phone ? 20 : 0
    content = (
      <div className="ts-paper" style={{ minHeight: '100%', padding: phone ? '18px 0 120px' : '56px 0 120px' }}>
        <article style={{ maxWidth: 700, margin: '0 auto', padding: `0 ${pad}px` }}>
          <div style={{ fontFamily: '"Segoe UI",system-ui,sans-serif', fontSize: 12, fontWeight: 700, letterSpacing: '0.12em', textTransform: 'uppercase', color: c.accent }}>Field notes · 8 min read</div>
          <h1 style={{ fontSize: phone ? 34 : 50, lineHeight: 1.08, letterSpacing: '-0.02em', margin: '12px 0 14px', fontWeight: 700, textWrap: 'balance' }}>The small motions that make an interface feel alive</h1>
          <p className="ts-muted" style={{ fontSize: phone ? 18 : 21, lineHeight: 1.45, fontStyle: 'italic', marginBottom: 24 }}>
            Why a heart that bursts, a tab that bounces and a spinner that breathes matter more than they should.
          </p>
          <div style={{ display: 'flex', alignItems: 'center', gap: 12, paddingBottom: 22, marginBottom: 26, borderBottom: `1px solid ${c.paperLine}`, fontFamily: '"Segoe UI",system-ui,sans-serif' }}>
            {big('author', 44, 44, avatar('author', 44))}
            <div style={{ flex: 1, lineHeight: 1.3 }}>
              <b style={{ fontSize: 14 }}>Amara Okafor</b>
              <div className="ts-muted" style={{ fontSize: 12.5 }}>
                Oct 16 · Design
              </div>
            </div>
            <button className="ts-btn" style={{ height: 32, borderRadius: 999 }}>
              Follow
            </button>
          </div>
        </article>
        <div style={{ maxWidth: phone ? undefined : 980, margin: '0 auto 30px', padding: phone ? 0 : '0 20px' }}>
          {big('hero', '100%', phone ? 240 : 460, img('hero', '100%', phone ? 240 : 460, phone ? 0 : 6), { background: c.paperLine, borderRadius: phone ? 0 : 6 })}
          <div className="ts-muted" style={{ fontSize: 13, textAlign: 'center', marginTop: 8, fontStyle: 'italic' }}>
            Studio light, late afternoon.
          </div>
        </div>
        <div className="ts-body" style={{ maxWidth: 700, margin: '0 auto', padding: `0 ${pad}px` }}>
          {para(0)}
          {para(1)}
          <blockquote style={{ margin: '34px 0', padding: '0 0 0 22px', borderLeft: `3px solid ${c.accent}`, fontSize: phone ? 23 : 28, lineHeight: 1.35, fontStyle: 'italic' }}>“Motion is the part of the interface you feel before you read it.”</blockquote>
          {para(2)}
          <figure style={{ margin: '30px 0' }}>
            {big('figure', '100%', phone ? 220 : 340, img('figure', '100%', phone ? 220 : 340, 6), { background: c.paperLine, borderRadius: 6 })}
            <figcaption className="ts-muted" style={{ fontSize: 13, marginTop: 8, fontStyle: 'italic' }}>
              Fig. 1 — the same button, with and without its press animation.
            </figcaption>
          </figure>
          {para(3)}
          {para(0)}
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', margin: '10px 0 34px', fontFamily: '"Segoe UI",system-ui,sans-serif' }}>
            {['Motion', 'Interface', 'Craft'].map((t) => (
              <span key={t} className="ts-chip" style={{ borderColor: c.paperLine }}>
                {t}
              </span>
            ))}
          </div>
        </div>
        <div className="ts-pill">
          {iconSlot('clap', 'ts-action', 'clap', 20, <span>{mine('clap') ? s.label : '1.2k'}</span>)}
          <span style={{ width: 1, height: 18, background: c.paperLine }} />
          <span className="ts-action">
            <Icon name="comment" /> 48
          </span>
          {iconSlot('share', 'ts-action', 'share', 20)}
          <span className="ts-action">
            <Icon name="saved" />
          </span>
        </div>
      </div>
    )
  } else {
    const lines = SNIPPET.split('\n')
    const editor = (
      <div className="ts-editor ts-mono" style={{ minWidth: 0 }}>
        <div className="tabs">
          {['like.ts', 'player.config.ts', 'README.md'].map((t, i) => (
            <span key={t} className={`tab${i === tab ? ' on' : ''}`} onClick={() => setTab(i)}>
              {t}
            </span>
          ))}
        </div>
        <pre>
          {lines.map((l, i) => (
            <div key={i}>
              <span className="ln">{i + 1}</span>
              {highlight(l)}
            </div>
          ))}
        </pre>
        <div className="bar">
          {iconSlot('run', 'ebtn run', 'play', 15, <span>{mine('run') ? s.label || 'Run' : 'Run'}</span>)}
          <span className="ebtn">Format</span>
          <div style={{ flex: 1 }} />
          {iconSlot('copy', 'ebtn', 'copy', 15, <span>Copy</span>)}
        </div>
      </div>
    )
    const preview = (
      <div className="ts-card" style={{ display: 'flex', flexDirection: 'column', minHeight: phone ? 260 : 0 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '9px 12px', borderBottom: `1px solid ${c.line}`, fontSize: 12.5 }} className="ts-mono">
          <span style={{ width: 8, height: 8, borderRadius: '50%', background: '#22c55e' }} />
          Preview
          <span className="ts-muted" style={{ marginLeft: 'auto' }}>
            localhost:5173
          </span>
        </div>
        {big(
          'preview',
          '100%',
          phone ? 260 : '100%',
          <div style={{ flex: 1, display: 'grid', placeItems: 'center', padding: 30, background: `radial-gradient(circle at 50% 40%, ${c.hover}, ${c.surface})` }}>
            <button className="ts-btn" style={{ height: 52, padding: '0 22px', borderRadius: 999, fontSize: 16 }}>
              <Icon name="heart" size={24} style={{ color: '#e0245e' }} /> 248
            </button>
          </div>,
          { flex: 1 },
        )}
      </div>
    )
    content = (
      <div style={{ minHeight: '100%', display: 'flex', flexDirection: 'column' }}>
        <div style={{ flex: 1, padding: phone ? '14px 12px 100px' : '26px 28px', display: 'grid', gap: 16, alignContent: 'start' }}>
          <div>
            <div className="ts-mono ts-muted" style={{ fontSize: 12.5 }}>
              northwind / web / src / <b style={{ color: c.text }}>like.ts</b>
            </div>
            <h1 style={{ margin: '6px 0 2px', fontSize: phone ? 22 : 28, letterSpacing: '-0.02em' }}>Wire up the like button</h1>
            <p className="ts-muted">Load the animation, then drive its state machine from your own code.</p>
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: phone ? '1fr' : 'minmax(0, 1.3fr) minmax(280px, 1fr)', gap: 16, alignItems: 'stretch' }}>
            {editor}
            {preview}
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: phone ? '1fr' : 'repeat(3, 1fr)', gap: 12 }}>
            {[
              ['Install', 'npm i @toasty/player'],
              ['Size', '9.4 kB gzipped'],
              ['Runs on', 'Canvas 2D · WebGL2'],
            ].map(([k, v]) => (
              <div key={k} className="ts-card lift" style={{ padding: 14 }}>
                <div className="ts-muted" style={{ fontSize: 12 }}>
                  {k}
                </div>
                <div className="ts-mono" style={{ fontSize: 13.5, marginTop: 2 }}>
                  {v}
                </div>
              </div>
            ))}
          </div>
        </div>
        <div className="ts-status ts-mono" style={{ position: phone ? 'sticky' : 'sticky', bottom: phone ? 74 : 0 }}>
          {mine('status') ? (
            <Hover style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
              {(hv) => (
                <>
                  {live(hv, 18, 18)}
                  {s.label || 'Build'}
                </>
              )}
            </Hover>
          ) : (
            <span style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
              <Icon name="check" size={14} /> Build passed
            </span>
          )}
          <span>main</span>
          <span style={{ marginLeft: 'auto' }}>TypeScript · UTF-8</span>
        </div>
      </div>
    )
  }

  if (mine('content')) content = <Hover style={{ position: 'absolute', inset: 0, display: 'grid' }}>{(hv) => live(hv, '100%', '100%', fit)}</Hover>

  const fab = big(
    'fab',
    56,
    56,
    <button className="ts-btn primary" style={{ position: 'absolute', right: 18, bottom: phone ? 92 : 24, width: 56, height: 56, borderRadius: '50%', padding: 0, zIndex: 4, boxShadow: `0 8px 22px ${c.accent}66` }}>
      <Icon name="plus" size={24} />
    </button>,
    { position: 'absolute', right: 18, bottom: phone ? 92 : 24, zIndex: 4 },
  )

  // ---- Device layouts ------------------------------------------------------
  let app: ReactNode
  if (phone) {
    app = (
      <div style={{ position: 'absolute', inset: 0, display: 'flex', flexDirection: 'column', background: s.page === 'article' ? c.paper : c.page }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '14px 26px 4px', fontSize: 14, fontWeight: 600, flex: 'none' }}>
          <span>9:41</span>
          <span style={{ width: 100, height: 28, borderRadius: 16, background: '#000' }} />
          <span>100%</span>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '6px 12px 8px', flex: 'none', borderBottom: `1px solid ${s.page === 'article' ? c.paperLine : c.line}` }}>
          <span className="ts-icon" onClick={() => setDrawer(true)}>
            <Icon name="menu" />
          </span>
          {logo}
          <b style={{ fontSize: 18, letterSpacing: '-0.02em', flex: 1 }}>{s.page === 'home' ? 'Northwind' : s.page === 'article' ? 'Read' : 'Code'}</b>
          <span className="ts-icon">
            <Icon name="search" />
          </span>
          {bell}
        </div>
        <div style={{ position: 'relative', flex: 1, minHeight: 0, overflow: 'auto' }}>{content}</div>
        {fab}
        <div style={{ position: 'absolute', left: 0, right: 0, bottom: 0, zIndex: 5, display: 'flex', justifyContent: 'space-around', padding: '8px 6px 24px', borderTop: `1px solid ${c.line}`, background: `${c.surface}f0`, backdropFilter: 'blur(10px)' }}>
          {NAV.map((n, i) => iconSlot(`nav:${i}`, `ts-tab${i === active ? ' on' : ''}`, NAV_ICONS[i], 22, <span>{mine(`nav:${i}`) && s.label ? s.label : n}</span>, () => go(i)))}
        </div>
        {drawer && (
          <>
            <div className="ts-drawer-back" onClick={() => setDrawer(false)} />
            <div className="ts-drawer">{sidebarBody}</div>
          </>
        )}
      </div>
    )
  } else {
    app = (
      <div style={{ position: 'absolute', inset: 0, display: 'flex' }}>
        <aside style={{ width: 248, flex: 'none', background: c.surface, borderRight: `1px solid ${c.line}`, display: 'flex', flexDirection: 'column', padding: 14, gap: 2, overflow: 'auto' }}>{sidebarBody}</aside>
        <div style={{ position: 'relative', flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '11px 24px', background: c.surface, borderBottom: `1px solid ${c.line}`, flex: 'none' }}>
            <input className="ts-input" placeholder="Search posts, articles, code…" style={{ maxWidth: 420 }} />
            <div style={{ flex: 1 }} />
            {bell}
            <button className="ts-btn primary">New post</button>
            {avatar('me', 32)}
          </div>
          <div style={{ position: 'relative', flex: 1, minHeight: 0, overflow: 'auto', background: s.page === 'article' ? c.paper : c.page }}>{content}</div>
          {fab}
        </div>
      </div>
    )
  }

  const url = `northwind.app/${s.page === 'home' ? '' : s.page === 'article' ? 'read/small-motions' : 'code/like.ts'}`
  const browser = (
    <div style={{ position: 'absolute', inset: 0, display: 'flex', flexDirection: 'column', borderRadius: 12, overflow: 'hidden', border: `1px solid ${c.line}`, boxShadow: '0 30px 80px rgb(0 0 0 / 0.3)', background: c.page }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 14, height: 40, padding: '0 14px', flex: 'none', background: s.dark ? '#1c1c22' : '#e9e8e4', borderBottom: `1px solid ${c.line}` }}>
        <span style={{ display: 'flex', gap: 7 }}>
          {['#ff5f57', '#febc2e', '#28c840'].map((col) => (
            <span key={col} style={{ width: 12, height: 12, borderRadius: '50%', background: col }} />
          ))}
        </span>
        <div style={{ flex: 1, maxWidth: 560, margin: '0 auto', height: 26, borderRadius: 8, background: s.dark ? '#121216' : '#fff', color: c.muted, fontSize: 12.5, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>{url}</div>
        <span style={{ width: 54 }} />
      </div>
      <div style={{ position: 'relative', flex: 1, minHeight: 0 }}>{app}</div>
    </div>
  )
  const handset = (
    <div style={{ width: 414, height: 868, borderRadius: 56, padding: 12, background: '#0c0c0e', boxShadow: '0 30px 80px rgb(0 0 0 / 0.35), inset 0 0 0 2px #2a2a30' }}>
      <div style={{ position: 'relative', width: '100%', height: '100%', borderRadius: 45, overflow: 'hidden', background: c.page }}>{app}</div>
    </div>
  )

  const desk = s.dark ? 'radial-gradient(circle at 50% 30%, #23232c, #0b0b0e)' : 'radial-gradient(circle at 50% 30%, #ffffff, #e4e3de)'
  const [dw, dh] = deviceSize(s.device)
  return (
    <div className="ts" style={{ overflow: 'hidden', background: desk }}>
      <style>{sheet(s.dark)}</style>
      {frame === 'stage' && !phone ? (
        <div style={{ position: 'absolute', inset: 18 }}>{browser}</div>
      ) : (
        <Fit w={dw} h={dh} pad={frame === 'stage' ? 20 : 8} max={frame === 'stage' ? 1.15 : 1}>
          <div style={{ position: 'relative', width: dw, height: dh }}>{phone ? handset : browser}</div>
        </Fit>
      )}
      {controls && <div className="ts-controls">{controls}</div>}
    </div>
  )
}

/** Intrinsic size of each device frame (px) before scaling. */
export function deviceSize(device: TestDevice): [number, number] {
  return device === 'phone' ? [414, 868] : [1280, 800]
}
