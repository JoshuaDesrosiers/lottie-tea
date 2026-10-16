/**
 * Docs home. The page is built out of the things it documents: every icon
 * that moves here is a Toasty Lite animation driven by ordinary React
 * state, and the game at the bottom is the full engine.
 */
import React, { useEffect, useRef, useState, type ReactNode } from 'react'
import Link from '@docusaurus/Link'
import Layout from '@theme/Layout'
import { Lite, type LiteHandle } from '@site/src/components/Lite'
import { ToastyPlayer } from '@site/src/components/ToastyPlayer'
import styles from './index.module.css'

/** A tile in the component wall: the animation, its name, and what to do. */
function Tile({ name, hint, children, onClick, active }: { name: string; hint: string; children: ReactNode; onClick?: () => void; active?: boolean }) {
  return (
    <div className={`${styles.tile} ${active ? styles.tileActive : ''}`} onClick={onClick}>
      <div className={styles.tileArt}>{children}</div>
      <div className={styles.tileMeta}>
        <span>{name}</span>
        <small>{hint}</small>
      </div>
    </div>
  )
}

function ComponentWall() {
  // A few tiles are driven by the page, the way a real app would.
  const [status, setStatus] = useState<'idle' | 'loading' | 'success'>('idle')
  const [progress, setProgress] = useState(0)
  const [dl, setDl] = useState<'idle' | 'downloading' | 'done'>('idle')
  const [unread, setUnread] = useState(true)
  const bell = useRef<LiteHandle | null>(null)
  const timer = useRef<number | undefined>(undefined)
  useEffect(() => () => window.clearInterval(timer.current), [])

  const submit = () => {
    if (status === 'loading') return
    if (status === 'success') return setStatus('idle')
    setStatus('loading')
    window.setTimeout(() => setStatus('success'), 1400)
  }
  const download = () => {
    if (dl !== 'idle') {
      setDl('idle')
      setProgress(0)
      return
    }
    setDl('downloading')
    let p = 0
    window.clearInterval(timer.current)
    timer.current = window.setInterval(() => {
      p = Math.min(1, p + 0.05 + Math.random() * 0.06)
      setProgress(p)
      if (p >= 1) {
        window.clearInterval(timer.current)
        setDl('done')
      }
    }, 120)
  }

  return (
    <div className={styles.wall}>
      <Tile name="heart" hint="click to like">
        <Lite src="heart" ink="auto" className={styles.art} />
      </Tile>
      <Tile name="menu" hint="click to open">
        <Lite src="menu" ink="auto" className={styles.art} />
      </Tile>
      <Tile name="theme" hint="click to switch">
        <Lite src="theme" className={styles.art} />
      </Tile>
      <Tile name="status" hint={status === 'idle' ? 'click to submit' : status === 'loading' ? 'sending…' : 'sent · click to reset'} onClick={submit} active={status !== 'idle'}>
        <Lite src="status" ink="auto" inputs={{ status }} className={styles.art} />
      </Tile>
      <Tile
        name="bell"
        hint={unread ? 'click to read' : 'click for a new one'}
        onClick={() => {
          if (!unread) {
            setUnread(true)
            bell.current?.fire('ring')
          } else setUnread(false)
        }}
      >
        <Lite ref={bell} src="bell" ink="auto" inputs={{ unread }} className={styles.art} />
      </Tile>
      <Tile name="download" hint={dl === 'idle' ? 'click to download' : dl === 'done' ? 'done · click to reset' : `${Math.round(progress * 100)}%`} onClick={download} active={dl !== 'idle'}>
        <Lite src="download" ink="auto" inputs={{ state: dl }} data={{ progress }} className={styles.art} />
      </Tile>
      <Tile name="copy" hint="click to copy">
        <Lite src="copy" ink="auto" className={styles.art} />
      </Tile>
      <Tile name="loader" hint="always running">
        <Lite src="loader" ink="auto" className={styles.art} />
      </Tile>
      <Tile name="toasty" hint="hover, then click">
        <Lite src="toasty" className={styles.art} />
      </Tile>
    </div>
  )
}

/** A card that animates its arrow while the card itself is hovered. */
function GuideCard({ to, title, body }: { to: string; title: string; body: string }) {
  const [hover, setHover] = useState(false)
  return (
    <Link to={to} className={styles.card} onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)} onFocus={() => setHover(true)} onBlur={() => setHover(false)}>
      <div>
        <h3>{title}</h3>
        <p>{body}</p>
      </div>
      <Lite src="arrow" ink="auto" inputs={{ hover }} className={styles.cardArrow} />
    </Link>
  )
}

const POST_HTML = `<article class="post">
  <header>
    <span class="avatar">MC</span>
    <div><b>Maya Chen</b><small>2 h · Studio</small></div>
  </header>
  <div class="photo"></div>
  <p>First light on the new desk lamp prototypes. The brass one is winning.</p>
  <footer>
    <button class="like" aria-pressed="false" aria-label="Like">
      <toasty-lite src="/toasty/lite/heart.json" ink="currentColor"></toasty-lite>
      <span class="count">128</span>
    </button>
    <button class="ghost">Reply</button>
    <button class="ghost">Share</button>
  </footer>
</article>`

const POST_CSS = `body { display: grid; place-items: center; padding: 24px; }
.post { width: min(420px, 100%); background: var(--surface); border: 1px solid var(--line); border-radius: 14px; overflow: hidden; }
header { display: flex; gap: 10px; align-items: center; padding: 14px 16px; }
header small { display: block; color: var(--muted); font-size: 12px; }
.avatar { width: 36px; height: 36px; border-radius: 50%; display: grid; place-items: center; font-size: 12px; font-weight: 600; color: #fff; background: linear-gradient(135deg, #f97316, #db2777); }
.photo { height: 150px; background: radial-gradient(circle at 70% 30%, #fde68a, transparent 40%), linear-gradient(160deg, #78350f, #1c1917); }
p { margin: 12px 16px; }
footer { display: flex; gap: 4px; padding: 4px 8px 10px; }
footer button { display: flex; align-items: center; gap: 2px; border: 0; background: none; padding: 4px 10px 4px 2px; border-radius: 8px; cursor: pointer; color: var(--ink); }
footer button:hover { background: color-mix(in srgb, var(--ink) 6%, transparent); }
.ghost { color: var(--muted) !important; padding-left: 10px !important; }
.like toasty-lite { width: 40px; height: 40px; pointer-events: none; }
.count { font-variant-numeric: tabular-nums; font-weight: 500; }`

const POST_JS = `const button = document.querySelector('.like')
const heart = button.querySelector('toasty-lite')
const count = button.querySelector('.count')
let liked = false, total = 128

button.addEventListener('click', () => {
  liked = !liked
  total += liked ? 1 : -1
  heart.player.setInput('liked', liked)       // the animation follows the page
  button.setAttribute('aria-pressed', String(liked))
  count.textContent = total
  // fetch('/api/posts/42/like', { method: liked ? 'POST' : 'DELETE' })
})`

export default function Home(): ReactNode {
  const [LiveHTML, setLiveHTML] = useState<null | React.ComponentType<any>>(null)
  useEffect(() => {
    void import('@site/src/components/LiveHTML').then((m) => setLiveHTML(() => m.LiveHTML))
  }, [])
  return (
    <Layout title="Toasty docs" description="Documentation for ToastyCreate: interactive vector animation for websites with Toasty Lite, and games with the Toasty engine.">
      <main>
        <section className={styles.hero}>
          <div className={styles.heroCopy}>
            <p className={styles.eyebrow}>ToastyCreate documentation</p>
            <h1>Interactive animation for the web, and the games to go with it.</h1>
            <p className={styles.lede}>
              Design icons, buttons and illustrations in the Toasty editor, give them states, and ship them to your site as a few kilobytes of JSON with <b>Toasty Lite</b>. When you need more, the same editor builds full games for the <b>Toasty engine</b>.
            </p>
            <div className={styles.actions}>
              <Link className={styles.primary} to="/getting-started/first-project">
                Get started
              </Link>
              <Link className={styles.secondary} to="/runtimes/lite">
                Toasty Lite runtime
              </Link>
            </div>
            <p className={styles.note}>
              Every animated element on this page is a Toasty Lite file. Try them, or open the <Link to="/playground">Playground</Link> for all 22 with live controls.
            </p>
          </div>
          <ComponentWall />
        </section>

        <section className={styles.section}>
          <div className={styles.sectionHead}>
            <h2>Made for real interfaces</h2>
            <p>A Lite animation is just an element in your markup. Put it in a button, drive it from your app’s state, and it plays the right transition. This is the whole like button, running.</p>
          </div>
          {LiveHTML ? <LiveHTML url="studio.example/posts/42" height={400} html={POST_HTML} css={POST_CSS} js={POST_JS} tab="js" /> : <div style={{ height: 400 }} />}
        </section>

        <section className={styles.section}>
          <div className={styles.sectionHead}>
            <h2>Two runtimes, one editor</h2>
            <p>Pick the runtime per piece of content. Most sites use Lite everywhere and the engine where something is genuinely a game.</p>
          </div>
          <div className={styles.runtimes}>
            <div className={styles.runtime}>
              <div className={styles.runtimeTop}>
                <Lite src="toasty" style={{ width: 56, height: 56 }} />
                <div>
                  <h3>Toasty Lite</h3>
                  <span className={styles.badge}>~25 KB · Canvas 2D · unlimited per page</span>
                </div>
              </div>
              <p>Vector shapes, trim paths, path morphs, keyframes and state machines. Hover and click work with no code; your page sets inputs and listens for states.</p>
              <ul>
                <li>Icons, buttons, toggles, loaders</li>
                <li>Brand characters and illustrations</li>
                <li>
                  <code>&lt;toasty-lite&gt;</code>, JavaScript, or React
                </li>
              </ul>
              <Link to="/runtimes/lite">Read the Lite docs</Link>
            </div>
            <div className={styles.runtime}>
              <ToastyPlayer game="/games/toast-catch.teab" style={{ height: 200 }} label="Play Toast Catch" />
              <h3>Toasty engine</h3>
              <span className={styles.badge}>wasm · WebGPU / WebGL2 · one per page</span>
              <p>Scripts, physics, particles, lighting, audio, 3D. Embed a game, feed it data from your page, and hear its events back.</p>
              <Link to="/games">See the games</Link>
            </div>
          </div>
        </section>

        <section className={styles.section}>
          <div className={styles.sectionHead}>
            <h2>Where to go next</h2>
          </div>
          <div className={styles.cards}>
            <GuideCard to="/getting-started/first-project" title="Your first project" body="Open the editor, draw a shape, animate it and press Play." />
            <GuideCard to="/use-cases" title="Use cases" body="Like buttons, menus, form states, downloads, notifications, in real pages." />
            <GuideCard to="/runtimes/authoring" title="Authoring for Lite" body="Trim paths, morphs, inputs and listeners: build animations that ship." />
            <GuideCard to="/scripting/overview" title="Scripting" body="The ctx API for games and interactive scenes, syntax first." />
            <GuideCard to="/games" title="Games" body="Two small games, every line explained, ready to make your own." />
            <GuideCard to="/courses" title="Courses" body="Build the like button, an icon system, a game and a landing page." />
          </div>
        </section>
      </main>
    </Layout>
  )
}
