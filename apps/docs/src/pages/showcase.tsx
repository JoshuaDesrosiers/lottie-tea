/**
 * The showcase: every component Toasty built for these docs, each running
 * live with the page-side controls that drive it, plus full demo games.
 * One engine runs at a time — starting a card stops the previous one.
 */
import React, { useRef, useState, type ReactNode } from 'react'
import Layout from '@theme/Layout'
import Link from '@docusaurus/Link'
import { ToastyPlayer, type ToastyHandle } from '@site/src/components/ToastyPlayer'
import styles from './showcase.module.css'

function Card({ title, file, children, player, code }: { title: string; file: string; children?: ReactNode; player: ReactNode; code: string }) {
  return (
    <article className={styles.card}>
      <header className={styles.head}>
        <h2>{title}</h2>
        <code>{file}</code>
      </header>
      {player}
      {children && <div className={styles.controls}>{children}</div>}
      <details className={styles.code}>
        <summary>React code</summary>
        <pre>{code}</pre>
      </details>
    </article>
  )
}

function Log({ lines }: { lines: string[] }) {
  return <div className={styles.log}>{lines.length ? lines.map((l, i) => <div key={i}>{l}</div>) : <span>Events show up here.</span>}</div>
}

function LikeCard() {
  const [count, setCount] = useState(128)
  const [log, setLog] = useState<string[]>([])
  const ref = useRef<ToastyHandle | null>(null)
  return (
    <Card
      title="Like button"
      file="like-button.teab"
      player={
        <ToastyPlayer
          ref={ref}
          game="/games/like-button.teab"
          data={{ count }}
          style={{ height: 240 }}
          onEvent={(e) => {
            if (e.name !== 'like') return
            setCount((e.payload as { count: number }).count)
            setLog((l) => [`like ${JSON.stringify(e.payload)}`, ...l].slice(0, 4))
          }}
        />
      }
      code={`<An1Player game="/an1mbyt3/like-button.teab"
  data={{ count }}
  onEvent={(e) => e.name === 'like' && setCount(e.payload.count)} />`}
    >
      <div className={styles.row}>
        <button onClick={() => setCount((c) => c + 1000)}>+1000 from the server</button>
        <button onClick={() => ref.current?.fire('burst')}>fire('burst')</button>
      </div>
      <Log lines={log} />
    </Card>
  )
}

function RingCard() {
  const [progress, setProgress] = useState(0.35)
  const [color, setColor] = useState('#7c5cff')
  const [done, setDone] = useState(false)
  const simulate = () => {
    setDone(false)
    let p = 0
    const timer = setInterval(() => {
      p = Math.min(1, p + 0.04 + Math.random() * 0.05)
      setProgress(p)
      if (p >= 1) clearInterval(timer)
    }, 180)
  }
  return (
    <Card
      title="Progress ring"
      file="progress-ring.teab"
      player={<ToastyPlayer game="/games/progress-ring.teab" data={{ progress, color, label: done ? 'Uploaded' : 'Uploading' }} style={{ height: 300 }} onEvent={(e) => e.name === 'done' && setDone(true)} />}
      code={`<An1Player game="/an1mbyt3/progress-ring.teab"
  data={{ progress, color, label: 'Uploading' }}
  onEvent={(e) => e.name === 'done' && toast('Upload complete')} />`}
    >
      <label className={styles.field}>
        progress {progress.toFixed(2)}
        <input type="range" min={0} max={1} step={0.01} value={progress} onChange={(e) => setProgress(Number(e.target.value))} />
      </label>
      <div className={styles.row}>
        <input type="color" value={color} onChange={(e) => setColor(e.target.value)} aria-label="Ring colour" />
        <button onClick={simulate}>Simulate an upload</button>
        <span className={styles.badge}>{done ? 'done event received' : 'waiting'}</span>
      </div>
    </Card>
  )
}

function DayNightCard() {
  const [night, setNight] = useState(false)
  return (
    <Card
      title="Day / night switch"
      file="day-night.teab"
      player={<ToastyPlayer game="/games/day-night.teab" data={{ isNight: night }} style={{ height: 200 }} onEvent={(e) => e.name === 'toggle' && setNight((e.payload as { isNight: boolean }).isNight)} />}
      code={`<An1Player game="/an1mbyt3/day-night.teab"
  data={{ isNight: dark }}
  onEvent={(e) => e.name === 'toggle' && setDark(e.payload.isNight)} />`}
    >
      <label className={styles.check}>
        <input type="checkbox" checked={night} onChange={(e) => setNight(e.target.checked)} /> React's state: {night ? 'night' : 'day'}
      </label>
    </Card>
  )
}

/** A rough password-strength score in -1 .. 1. */
function strength(text: string): number {
  if (!text) return 0
  const length = Math.min(text.length, 14) / 7 - 1
  const digits = /[0-9]/.test(text) ? 0.25 : 0
  const symbols = /[^a-z0-9]/i.test(text) ? 0.25 : 0
  return Math.max(-1, Math.min(1, length + digits + symbols))
}

function MoodCard() {
  const [text, setText] = useState('')
  const score = strength(text)
  const [state, setState] = useState('Neutral')
  const ref = useRef<ToastyHandle | null>(null)
  return (
    <Card
      title="Mood blob (password strength)"
      file="mood-blob.teab"
      player={<ToastyPlayer ref={ref} game="/games/mood-blob.teab" data={{ mood: score }} style={{ height: 260 }} onStateChange={(e) => !e.layer && setState(e.state)} />}
      code={`<An1Player game="/an1mbyt3/mood-blob.teab"
  data={{ mood: strength(password) }}   // -1 .. 1
  onStateChange={(e) => setState(e.state)} />`}
    >
      <input className={styles.text} type="text" placeholder="Type a password…" value={text} onChange={(e) => setText(e.target.value)} />
      <div className={styles.row}>
        <span className={styles.badge}>
          mood {score.toFixed(2)} · state {state}
        </span>
        <button onClick={() => ref.current?.fire('jump')}>fire('jump')</button>
      </div>
    </Card>
  )
}

/** WMO weather code → the card's condition. */
function wmo(code: number): string {
  if (code === 0 || code === 1) return 'sun'
  if (code <= 48) return 'cloud'
  if (code >= 95) return 'storm'
  if ((code >= 71 && code <= 77) || code === 85 || code === 86) return 'snow'
  return 'rain'
}

const CITIES: [string, number, number][] = [
  ['Lisbon', 38.72, -9.14],
  ['Oslo', 59.91, 10.75],
  ['Tokyo', 35.68, 139.69],
  ['Seattle', 47.61, -122.33],
  ['Cairo', 30.04, 31.24],
]

function WeatherCard() {
  const [data, setData] = useState({ city: 'Lisbon', temp: 21, condition: 'sun' })
  const [status, setStatus] = useState('demo values')
  const load = async (name: string, lat: number, lon: number) => {
    setStatus('fetching…')
    try {
      const r = await fetch(`https://api.open-meteo.com/v1/forecast?latitude=${lat}&longitude=${lon}&current=temperature_2m,weather_code`)
      const j = await r.json()
      setData({ city: name, temp: j.current.temperature_2m, condition: wmo(j.current.weather_code) })
      setStatus(`live from open-meteo.com (code ${j.current.weather_code})`)
    } catch {
      setStatus('offline — kept the last values')
    }
  }
  return (
    <Card
      title="Weather card (fetched data)"
      file="weather-card.teab"
      player={<ToastyPlayer game="/games/weather-card.teab" data={data} style={{ height: 260 }} />}
      code={`const { data } = useQuery(['wx', city], fetchWeather)
<An1Player game="/an1mbyt3/weather-card.teab"
  data={{ city, temp: data.temp, condition: data.condition }} />`}
    >
      <div className={styles.row}>
        {CITIES.map(([n, la, lo]) => (
          <button key={n} onClick={() => load(n, la, lo)}>
            {n}
          </button>
        ))}
      </div>
      <div className={styles.row}>
        {['sun', 'cloud', 'rain', 'snow', 'storm'].map((c) => (
          <button key={c} className={data.condition === c ? styles.on : ''} onClick={() => setData((d) => ({ ...d, condition: c }))}>
            {c}
          </button>
        ))}
      </div>
      <span className={styles.badge}>{status}</span>
    </Card>
  )
}

function BellCard() {
  const [unread, setUnread] = useState(3)
  const [opened, setOpened] = useState<string | null>(null)
  return (
    <Card
      title="Notification bell"
      file="bell.teab"
      player={
        <ToastyPlayer
          game="/games/bell.teab"
          data={{ unread }}
          style={{ height: 240 }}
          onEvent={(e) => {
            if (e.name !== 'open') return
            setOpened(`Opened with ${(e.payload as { unread: number }).unread} unread`)
            setUnread(0)
          }}
        />
      }
      code={`<An1Player game="/an1mbyt3/bell.teab"
  data={{ unread }}
  onEvent={(e) => e.name === 'open' && openInbox()} />`}
    >
      <div className={styles.row}>
        <button onClick={() => setUnread((u) => u + 1)}>New message</button>
        <button onClick={() => setUnread(0)}>Mark all read</button>
        <span className={styles.badge}>{opened ?? `unread = ${unread}`}</span>
      </div>
    </Card>
  )
}

const GAMES: [string, string, string, number][] = [
  ['Crown Match', 'crown-match.teab', 'A match-3 with levels, boosters and tweened gems, all in script.', 420],
  ['Ninja Adventure', 'ninja.teab', 'Top-down pixel-art adventure: two scenes, lighting, combat, a house to enter.', 420],
  ['Liquid Lab', 'liquid-lab.teab', 'Particle fluid with the GPU goo shader, coupled to physics.', 380],
  ['UI & Sound Lab', 'ui-sound-lab.teab', 'Buttons, sliders, toggles, a scroll view and synthesized sound.', 380],
  ['HTML UI Lab', 'html-ui-lab.teab', 'Real HTML + CSS inside a scene, wired by data-trigger and ref.dom.', 380],
]

export default function Showcase() {
  return (
    <Layout title="Showcase" description="Live components and games made in Toasty, driven by the page around them.">
      <main className={styles.main}>
        <header className={styles.intro}>
          <h1>Showcase</h1>
          <p>
            Everything here was built in the Toasty editor and runs in this page through the same embed you'd ship. The buttons and sliders under each one are ordinary React: they set data, flip inputs and fire triggers, and the page listens back. Learn how in{' '}
            <Link to="/embedding/react">React embeds</Link>, or build one yourself in the <Link to="/courses/react-like-button/plan-the-button">like button course</Link>.
          </p>
        </header>
        <section className={styles.grid}>
          <LikeCard />
          <RingCard />
          <DayNightCard />
          <MoodCard />
          <WeatherCard />
          <BellCard />
        </section>
        <h2 className={styles.sub}>Full games</h2>
        <section className={styles.games}>
          {GAMES.map(([title, file, body, h]) => (
            <article key={file} className={styles.card}>
              <header className={styles.head}>
                <h2>{title}</h2>
                <code>{file}</code>
              </header>
              <ToastyPlayer game={`/games/${file}`} style={{ height: h }} keyboard="focus" />
              <p className={styles.body}>{body} Click the game first so it gets the keyboard.</p>
            </article>
          ))}
        </section>
      </main>
    </Layout>
  )
}
