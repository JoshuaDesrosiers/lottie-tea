/**
 * Docs home: one live Toasty scene up top (the thesis — it's a real engine
 * running in the page), then the six ways into the docs.
 */
import React, { useState } from 'react'
import Link from '@docusaurus/Link'
import Layout from '@theme/Layout'
import { ToastyPlayer } from '@site/src/components/ToastyPlayer'
import styles from './index.module.css'

const SECTIONS = [
  { to: '/getting-started/first-project', title: 'Getting started', body: 'Open the editor, make a scene, animate it, press Play. Ten minutes.' },
  { to: '/editor/scenes', title: 'Editor guide', body: 'Every panel and tool: scenes, layers, paint mode, the Sprite Editor, timeline, state machines.' },
  { to: '/components/overview', title: 'Components', body: 'Physics, particles, goo, 2D lights, post effects, UI, HTML overlays, 3D, multiplayer.' },
  { to: '/scripting/overview', title: 'Scripting', body: 'The coding section: ctx, refs, input, physics, audio, storage, network. Syntax first.' },
  { to: '/embedding/overview', title: 'Embedding', body: 'Run scenes in your site. Feed them data from React, fire triggers, listen to events.' },
  { to: '/courses', title: 'Courses', body: 'Full builds with every line of code: a React like button, a platformer, dashboards, match-3.' },
]

export default function Home() {
  const [mood, setMood] = useState(0.4)
  const [state, setState] = useState('Neutral')
  return (
    <Layout title="Toasty Docs" description="Documentation, live demos and courses for ToastyCreate — the animation and game engine that runs in the browser.">
      <main className={styles.main}>
        <section className={styles.hero}>
          <div className={styles.copy}>
            <p className={styles.eyebrow}>ToastyCreate documentation</p>
            <h1 className={styles.title}>Animate it. Script it. Drop it into your website.</h1>
            <p className={styles.lede}>
              Toasty is an animation and game engine with an editor in your browser. The blob on the right is a Toasty scene running live in this page — the slider is ordinary React state, passed in as data.
            </p>
            <div className={styles.actions}>
              <Link className={styles.primary} to="/getting-started/first-project">Start the tour</Link>
              <a className={styles.secondary} href="https://toastycreate.web.app/">Open the editor</a>
            </div>
          </div>
          <div className={styles.demo}>
            <ToastyPlayer game="/games/mood-blob.teab" data={{ mood }} style={{ height: 300 }} label="Run the mood blob" onStateChange={(e) => !e.layer && setState(e.state)} />
            <label className={styles.slider}>
              <span>mood = {mood.toFixed(2)}</span>
              <input type="range" min={-1} max={1} step={0.01} value={mood} onChange={(e) => setMood(Number(e.target.value))} />
              <span className={styles.state}>state: {state}</span>
            </label>
            <pre className={styles.snippet}>{`<An1Player game="/an1mbyt3/mood-blob.teab"
  data={{ mood }}
  onStateChange={(e) => setState(e.state)} />`}</pre>
          </div>
        </section>
        <section className={styles.grid}>
          {SECTIONS.map((s) => (
            <Link key={s.to} to={s.to} className={styles.card}>
              <h2>{s.title}</h2>
              <p>{s.body}</p>
            </Link>
          ))}
        </section>
        <section className={styles.band}>
          <h2>See it working</h2>
          <p>Six small UI components built in Toasty, each driven by a web page: a like button, a progress ring, a day/night switch, a mood blob, a live weather card and a notification bell — plus full games.</p>
          <Link className={styles.primary} to="/showcase">Open the showcase</Link>
        </section>
      </main>
    </Layout>
  )
}
