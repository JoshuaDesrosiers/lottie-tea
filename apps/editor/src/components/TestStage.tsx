/**
 * The Test Area's **Use as Playground** stage: full-page, real-feeling
 * websites (and a phone on a desk) with hover and press states on every
 * element, with the animation being tested dropped into the chosen slot.
 *
 * - `nav`     — a product site: sticky header (your icon among its
 *               actions), hero, feature cards, footer.
 * - `sidebar` — a web app: full-height sidebar (your icon in a nav row, or
 *               filling it), top bar with search, dashboard content.
 * - `post`    — a social feed: left nav, a column of posts (yours first),
 *               "who to follow" rail.
 * - `phone`   — an app on a phone, centred on a soft desk background.
 *
 * Styles live in one scoped stylesheet (`.ts-…` classes) so real `:hover`
 * / `:active` states work.
 */
import { useState, type CSSProperties, type ReactNode } from 'react'

export type StageMock = 'nav' | 'sidebar' | 'post' | 'phone'

export interface StageProps {
  mock: StageMock
  /** The chosen element id (`row:1`, `media`, `tab:2`, …). */
  slot: string
  dark: boolean
  label: string
  size: number
  count: number
  /** Nav mode: the index of your icon among the header actions. */
  navSlot: number
  /** Renders the animation (hover state, width, height, fit). */
  live: (hover: boolean, w: number | string, h: number | string, fit?: 'cover' | 'contain') => ReactNode
  cover: boolean
  /** The inputs bar, shown floating at the bottom. */
  controls: ReactNode
}

const photo = (seed: string, w: number, h: number) => `https://picsum.photos/seed/toasty-${seed}/${w}/${h}`
const LOREM = [
  'Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor.',
  'Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi.',
  'Duis aute irure dolor in reprehenderit in voluptate velit esse cillum.',
  'Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia.',
]
const PEOPLE = [
  ['Maya Chen', 'mayac'],
  ['Jonas Weber', 'jonas'],
  ['Amara Okafor', 'amara'],
  ['Leo Martins', 'leom'],
  ['Sofia Rossi', 'sofia'],
]

function css(dark: boolean) {
  const c = dark
    ? { page: '#0c0c0f', surface: '#141418', raised: '#1b1b21', hover: '#22222a', line: '#26262d', text: '#ededf0', muted: '#9090a0', accent: '#7c6cff', accentText: '#fff' }
    : { page: '#f6f6f4', surface: '#ffffff', raised: '#ffffff', hover: '#f0f0ec', line: '#e7e6e2', text: '#16161a', muted: '#6c6c76', accent: '#4f46e5', accentText: '#fff' }
  return {
    c,
    sheet: `
.ts{position:absolute;inset:0;overflow:auto;background:${c.page};color:${c.text};font:14px/1.5 "Segoe UI",system-ui,-apple-system,sans-serif;-webkit-font-smoothing:antialiased}
.ts *{box-sizing:border-box}
.ts a{color:inherit;text-decoration:none}
.ts-muted{color:${c.muted}}
.ts-btn{display:inline-flex;align-items:center;gap:6px;height:36px;padding:0 14px;border-radius:9px;border:1px solid ${c.line};background:${c.surface};color:${c.text};font:inherit;font-weight:600;cursor:pointer;transition:background .15s,transform .1s,border-color .15s}
.ts-btn:hover{background:${c.hover};border-color:${c.muted}55}
.ts-btn:active{transform:scale(.97)}
.ts-btn.primary{background:${c.accent};border-color:transparent;color:${c.accentText}}
.ts-btn.primary:hover{filter:brightness(1.1)}
.ts-row{display:flex;align-items:center;gap:12px;padding:9px 12px;border-radius:10px;color:${c.muted};font-weight:500;cursor:pointer;transition:background .15s,color .15s}
.ts-row:hover{background:${c.hover};color:${c.text}}
.ts-row.on{background:${c.hover};color:${c.text};font-weight:600}
.ts-glyph{width:20px;height:20px;border-radius:6px;border:2px solid currentColor;opacity:.75;flex:none}
.ts-card{background:${c.surface};border:1px solid ${c.line};border-radius:16px;overflow:hidden;transition:transform .2s,box-shadow .2s,border-color .2s}
.ts-card.lift:hover{transform:translateY(-3px);box-shadow:0 14px 34px rgb(0 0 0 / ${dark ? 0.45 : 0.12});border-color:${c.muted}44}
.ts-icon{display:grid;place-items:center;width:38px;height:38px;border-radius:10px;cursor:pointer;color:${c.muted};transition:background .15s,color .15s}
.ts-icon:hover{background:${c.hover};color:${c.text}}
.ts-link{color:${c.muted};font-weight:500;transition:color .15s;cursor:pointer}
.ts-link:hover{color:${c.text}}
.ts-input{height:38px;border-radius:10px;border:1px solid ${c.line};background:${c.page};color:${c.text};padding:0 12px;font:inherit;width:100%;outline:none;transition:border-color .15s,box-shadow .15s}
.ts-input:focus{border-color:${c.accent};box-shadow:0 0 0 3px ${c.accent}33}
.ts-action{display:inline-flex;align-items:center;gap:6px;padding:6px 10px;border-radius:999px;color:${c.muted};cursor:pointer;font-weight:500;transition:background .15s,color .15s}
.ts-action:hover{background:${c.hover};color:${c.text}}
.ts-img{display:block;width:100%;object-fit:cover;background:${c.hover}}
.ts-tab{display:grid;justify-items:center;gap:2px;font-size:10px;color:${c.muted};cursor:pointer;padding:4px 10px;border-radius:10px;transition:background .15s,color .15s}
.ts-tab:hover{background:${c.hover};color:${c.text}}
.ts-controls{position:absolute;right:16px;bottom:16px;max-width:calc(100% - 32px);z-index:5}
`,
  }
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

const Glyph = () => <span className="ts-glyph" />

export function TestStage(p: StageProps) {
  const { c, sheet } = css(p.dark)
  const mine = (id: string) => p.slot === id
  const big = (id: string, w: number | string, h: number | string, fallback: ReactNode, className?: string, style?: CSSProperties) =>
    mine(id) ? (
      <Hover className={className} style={{ display: 'grid', placeItems: 'center', width: w, height: h, ...style }}>
        {(hv) => p.live(hv, w, h, p.cover ? 'cover' : undefined)}
      </Hover>
    ) : (
      fallback
    )

  let page: ReactNode
  if (p.mock === 'nav') {
    page = (
      <>
        <header style={{ position: 'sticky', top: 0, zIndex: 3, display: 'flex', alignItems: 'center', gap: 28, padding: '14px 40px', background: `${c.surface}e6`, backdropFilter: 'blur(10px)', borderBottom: `1px solid ${c.line}` }}>
          <b style={{ fontSize: 18, letterSpacing: '-0.02em' }}>Northwind</b>
          {['Product', 'Pricing', 'Customers', 'Docs'].map((l) => (
            <a key={l} className="ts-link">
              {l}
            </a>
          ))}
          <div style={{ flex: 1 }} />
          {Array.from({ length: Math.max(1, p.count) }, (_, i) =>
            i === p.navSlot ? (
              <Hover key={i} className="ts-icon" style={{ width: p.size + 16, height: p.size + 16 }}>
                {(hv) => p.live(hv, p.size, p.size)}
              </Hover>
            ) : (
              <span key={i} className="ts-icon">
                <Glyph />
              </span>
            ),
          )}
          <button className="ts-btn primary">Get started</button>
        </header>
        <section style={{ padding: '90px 40px 60px', maxWidth: 1100, margin: '0 auto', display: 'grid', gridTemplateColumns: '1.1fr 1fr', gap: 48, alignItems: 'center' }}>
          <div>
            <p style={{ color: c.accent, fontWeight: 600, margin: 0 }}>New · Northwind 4.0</p>
            <h1 style={{ fontSize: 52, lineHeight: 1.04, letterSpacing: '-0.035em', margin: '12px 0 16px' }}>Ship calmer products, faster.</h1>
            <p className="ts-muted" style={{ fontSize: 17, maxWidth: 460 }}>
              {LOREM[0]} {LOREM[1]}
            </p>
            <div style={{ display: 'flex', gap: 10, marginTop: 24 }}>
              <button className="ts-btn primary">Start free trial</button>
              <button className="ts-btn">Book a demo</button>
            </div>
          </div>
          <div className="ts-card" style={{ borderRadius: 20 }}>
            <img className="ts-img" src={photo('hero', 900, 640)} alt="" style={{ height: 340 }} />
          </div>
        </section>
        <section style={{ maxWidth: 1100, margin: '0 auto', padding: '0 40px 80px', display: 'grid', gridTemplateColumns: 'repeat(3,1fr)', gap: 18 }}>
          {['Plan', 'Build', 'Measure'].map((t, i) => (
            <div key={t} className="ts-card lift" style={{ padding: 20, cursor: 'pointer' }}>
              <img className="ts-img" src={photo(`feat${i}`, 600, 360)} alt="" style={{ height: 140, borderRadius: 10, marginBottom: 14 }} />
              <b style={{ fontSize: 16 }}>{t}</b>
              <p className="ts-muted" style={{ margin: '6px 0 0' }}>
                {LOREM[i + 1]}
              </p>
            </div>
          ))}
        </section>
      </>
    )
  } else if (p.mock === 'sidebar') {
    const fill = mine('fill')
    page = (
      <div style={{ display: 'flex', minHeight: '100%' }}>
        <aside style={{ position: 'sticky', top: 0, height: '100vh', maxHeight: '100%', width: 248, flex: 'none', background: c.surface, borderRight: `1px solid ${c.line}`, display: 'flex', flexDirection: 'column', padding: 14, gap: 4 }}>
          {fill ? (
            <Hover style={{ flex: 1, display: 'grid', placeItems: 'center' }}>{(hv) => p.live(hv, '100%', '100%', p.cover ? 'cover' : undefined)}</Hover>
          ) : (
            <>
              <b style={{ fontSize: 17, padding: '4px 10px 16px', letterSpacing: '-0.02em' }}>Northwind</b>
              {Array.from({ length: p.count }, (_, i) =>
                mine(`row:${i}`) ? (
                  <Hover key={i} className="ts-row on">
                    {(hv) => (
                      <>
                        {p.live(hv, p.size, p.size)}
                        <span>{p.label || 'Your item'}</span>
                      </>
                    )}
                  </Hover>
                ) : (
                  <div key={i} className="ts-row">
                    <Glyph />
                    {['Home', 'Search', 'Messages', 'Library', 'Saved', 'Profile', 'Settings', 'Help'][i % 8]}
                  </div>
                ),
              )}
              <div style={{ flex: 1 }} />
              <div className="ts-row" style={{ borderTop: `1px solid ${c.line}`, borderRadius: 0, paddingTop: 14 }}>
                <img src={photo('me', 80, 80)} alt="" style={{ width: 30, height: 30, borderRadius: '50%', objectFit: 'cover' }} />
                <span>
                  <b style={{ color: c.text, display: 'block', fontSize: 13 }}>Maya Chen</b>
                  <span style={{ fontSize: 12 }}>Pro plan</span>
                </span>
              </div>
            </>
          )}
        </aside>
        <main style={{ flex: 1, minWidth: 0 }}>
          <div style={{ position: 'sticky', top: 0, zIndex: 2, display: 'flex', alignItems: 'center', gap: 12, padding: '12px 28px', background: `${c.page}e6`, backdropFilter: 'blur(8px)', borderBottom: `1px solid ${c.line}` }}>
            <input className="ts-input" placeholder="Search notes, people, files…" style={{ maxWidth: 420 }} />
            <div style={{ flex: 1 }} />
            <span className="ts-icon">
              <Glyph />
            </span>
            <button className="ts-btn primary">New note</button>
          </div>
          <div style={{ padding: 28, maxWidth: 980 }}>
            {big('title', 320, 44, <h1 style={{ margin: '0 0 6px', fontSize: 30, letterSpacing: '-0.03em' }}>Northwind journal</h1>)}
            <p className="ts-muted" style={{ margin: '0 0 22px' }}>{LOREM[0]}</p>
            {big('hero', '100%', 260, <img className="ts-img" src={photo('hero', 1400, 600)} alt="" style={{ height: 260, borderRadius: 16 }} />, undefined, { borderRadius: 16, background: c.surface, border: `1px solid ${c.line}` })}
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3,1fr)', gap: 16, marginTop: 22 }}>
              {[0, 1, 2, 3, 4, 5].map((i) => (
                <div key={i} className="ts-card lift" style={{ cursor: 'pointer' }}>
                  <img className="ts-img" src={photo(`note${i}`, 600, 360)} alt="" style={{ height: 120 }} />
                  <div style={{ padding: 14 }}>
                    <b>{['Field notes', 'Q3 roadmap', 'Studio tour', 'Reading list', 'Team offsite', 'Moodboard'][i]}</b>
                    <p className="ts-muted" style={{ margin: '4px 0 0', fontSize: 13 }}>
                      {LOREM[i % 4]}
                    </p>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </main>
      </div>
    )
  } else if (p.mock === 'post') {
    const actions = (yours: boolean) => (
      <div style={{ display: 'flex', alignItems: 'center', gap: 4, padding: '6px 10px 10px', minHeight: p.size + 16 }}>
        {yours && mine('bar') ? (
          <Hover style={{ flex: 1, display: 'grid', placeItems: 'center' }}>{(hv) => p.live(hv, '100%', Math.max(p.size + 8, 48))}</Hover>
        ) : (
          Array.from({ length: p.count }, (_, i) =>
            yours && mine(`action:${i}`) ? (
              <Hover key={i} className="ts-action">
                {(hv) => (
                  <>
                    {p.live(hv, p.size, p.size)}
                    {p.label && <span>{p.label}</span>}
                  </>
                )}
              </Hover>
            ) : (
              <span key={i} className="ts-action">
                <Glyph /> {[248, 31, 12, 7][i % 4]}
              </span>
            ),
          )
        )}
      </div>
    )
    const post = (k: number, yours: boolean) => {
      const [name, handle] = PEOPLE[k % PEOPLE.length]
      return (
        <article key={k} className="ts-card" style={{ marginBottom: 16 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: 14 }}>
            {yours ? big('avatar', 40, 40, <img src={photo(`av${k}`, 96, 96)} alt="" style={{ width: 40, height: 40, borderRadius: '50%', objectFit: 'cover' }} />) : <img src={photo(`av${k}`, 96, 96)} alt="" style={{ width: 40, height: 40, borderRadius: '50%', objectFit: 'cover' }} />}
            <div style={{ flex: 1 }}>
              {yours ? big('name', 120, 22, <b className="ts-link" style={{ color: c.text }}>{name}</b>, undefined, { justifyContent: 'start' }) : <b className="ts-link" style={{ color: c.text }}>{name}</b>}
              <div className="ts-muted" style={{ fontSize: 13 }}>
                @{handle} · {k + 2}h
              </div>
            </div>
            <span className="ts-icon">···</span>
          </div>
          <p style={{ margin: '0 14px 12px' }}>{LOREM[k % 4]}</p>
          {yours ? big('media', '100%', 380, <img className="ts-img" src={photo(`post${k}`, 1200, 800)} alt="" style={{ height: 380 }} />) : <img className="ts-img" src={photo(`post${k}`, 1200, 800)} alt="" style={{ height: 380 }} />}
          {actions(yours)}
        </article>
      )
    }
    page = (
      <div style={{ display: 'grid', gridTemplateColumns: '240px minmax(0, 600px) 300px', justifyContent: 'center', gap: 28, padding: '20px 24px' }}>
        <nav style={{ position: 'sticky', top: 20, alignSelf: 'start', display: 'grid', gap: 2 }}>
          <b style={{ fontSize: 20, padding: '0 12px 14px', letterSpacing: '-0.02em' }}>northwind</b>
          {['Home', 'Explore', 'Notifications', 'Messages', 'Bookmarks', 'Profile'].map((l, i) => (
            <div key={l} className={`ts-row${i === 0 ? ' on' : ''}`} style={{ fontSize: 16 }}>
              <Glyph /> {l}
            </div>
          ))}
          <button className="ts-btn primary" style={{ marginTop: 14, height: 44, justifyContent: 'center' }}>
            New post
          </button>
        </nav>
        <main>
          <div className="ts-card" style={{ display: 'flex', gap: 10, padding: 14, marginBottom: 16 }}>
            <img src={photo('me', 96, 96)} alt="" style={{ width: 40, height: 40, borderRadius: '50%', objectFit: 'cover' }} />
            <input className="ts-input" placeholder="What's happening?" />
          </div>
          {[0, 1, 2].map((k) => post(k, k === 0))}
        </main>
        <aside style={{ position: 'sticky', top: 20, alignSelf: 'start' }}>
          <div className="ts-card" style={{ padding: 16 }}>
            <b>Who to follow</b>
            {PEOPLE.slice(1, 5).map(([name, handle], i) => (
              <div key={handle} style={{ display: 'flex', alignItems: 'center', gap: 10, marginTop: 14 }}>
                <img src={photo(`f${i}`, 80, 80)} alt="" style={{ width: 36, height: 36, borderRadius: '50%', objectFit: 'cover' }} />
                <div style={{ flex: 1, lineHeight: 1.25 }}>
                  <b className="ts-link" style={{ color: c.text, fontSize: 13 }}>
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
      </div>
    )
  } else {
    const full = mine('screen')
    page = (
      <div style={{ minHeight: '100%', display: 'grid', placeItems: 'center', padding: 24, background: p.dark ? 'radial-gradient(circle at 50% 30%, #23232c, #0b0b0e)' : 'radial-gradient(circle at 50% 30%, #ffffff, #e9e8e3)' }}>
        <div style={{ height: 'min(740px, calc(100vh - 170px))', aspectRatio: '360 / 740', borderRadius: 52, padding: 12, background: '#0c0c0e', boxShadow: '0 30px 80px rgb(0 0 0 / 0.35), inset 0 0 0 2px #2a2a30' }}>
          <div style={{ position: 'relative', width: '100%', height: '100%', borderRadius: 41, overflow: 'hidden', background: c.page, display: 'flex', flexDirection: 'column' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '14px 26px 6px', fontSize: 14, fontWeight: 600 }}>
              <span>9:41</span>
              <span style={{ width: 100, height: 28, borderRadius: 16, background: '#000' }} />
              <span>100%</span>
            </div>
            {full ? (
              <Hover style={{ flex: 1, display: 'grid' }}>{(hv) => p.live(hv, '100%', '100%', p.cover ? 'cover' : undefined)}</Hover>
            ) : (
              <>
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '8px 20px' }}>
                  <b style={{ fontSize: 24, letterSpacing: '-0.02em' }}>Discover</b>
                  {big('header', 36, 36, <span className="ts-icon"><Glyph /></span>, 'ts-icon')}
                </div>
                <div style={{ flex: 1, minHeight: 0, overflow: 'auto', padding: '6px 16px 96px', display: 'flex', flexDirection: 'column', gap: 14 }}>
                  {[0, 1, 2].map((k) => (
                    <div key={k} className="ts-card lift" style={{ cursor: 'pointer', flex: 'none' }}>
                      {k === 0 ? big('card', '100%', 170, <img className="ts-img" src={photo('card0', 700, 400)} alt="" style={{ height: 170 }} />) : <img className="ts-img" src={photo(`card${k}`, 700, 400)} alt="" style={{ height: 130 }} />}
                      <div style={{ padding: 12 }}>
                        <b>{['Weekend in Porto', 'Coffee guide', 'Night markets'][k]}</b>
                        <p className="ts-muted" style={{ margin: '4px 0 0', fontSize: 13 }}>
                          {LOREM[k]}
                        </p>
                      </div>
                    </div>
                  ))}
                </div>
                {big('fab', 56, 56, <button className="ts-btn primary" style={{ position: 'absolute', right: 18, bottom: 96, width: 56, height: 56, borderRadius: '50%', justifyContent: 'center', fontSize: 24, padding: 0 }}>+</button>, undefined, { position: 'absolute', right: 18, bottom: 96 })}
                <div style={{ position: 'absolute', left: 0, right: 0, bottom: 0, display: 'flex', justifyContent: 'space-around', padding: '10px 8px 26px', borderTop: `1px solid ${c.line}`, background: `${c.surface}f2`, backdropFilter: 'blur(10px)' }}>
                  {['Home', 'Search', 'Saved', 'You'].map((t, i) =>
                    mine(`tab:${i}`) ? (
                      <Hover key={t} className="ts-tab" style={{ color: c.text }}>
                        {(hv) => (
                          <>
                            {p.live(hv, p.size, p.size)}
                            <span>{p.label || t}</span>
                          </>
                        )}
                      </Hover>
                    ) : (
                      <div key={t} className="ts-tab">
                        <Glyph />
                        <span>{t}</span>
                      </div>
                    ),
                  )}
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    )
  }

  return (
    <div style={{ position: 'absolute', inset: 0 }}>
      <style>{sheet}</style>
      <div className="ts">{page}</div>
      {p.controls && <div className="ts-controls">{p.controls}</div>}
    </div>
  )
}
