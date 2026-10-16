/**
 * The Toasty Lite playground: every component, with a live control panel
 * generated from its own file (inputs, properties, states, events).
 */
import React from 'react'
import Layout from '@theme/Layout'
import Link from '@docusaurus/Link'
import { LiteCard } from '@site/src/components/InputsPanel'
import type { LiteHandle } from '@site/src/components/Lite'
import styles from '@site/src/components/playground.module.css'

/** Streams fake progress into the download component. */
function simulateDownload(p: LiteHandle) {
  p.setInput('state', 'downloading')
  let v = 0
  p.set('progress', 0)
  const timer = window.setInterval(() => {
    v = Math.min(1, v + 0.04 + Math.random() * 0.06)
    p.set('progress', v)
    if (v >= 1) {
      window.clearInterval(timer)
      p.setInput('state', 'done')
    }
  }, 110)
}

function simulateSubmit(p: LiteHandle, ok: boolean) {
  p.setInput('status', 'loading')
  window.setTimeout(() => p.setInput('status', ok ? 'success' : 'error'), 1300)
}

const Sim = ({ label, onClick }: { label: string; onClick: () => void }) => (
  <button type="button" className={styles.sim} onClick={onClick}>
    {label}
  </button>
)

const CARDS: { src: string; title: string; blurb: string; extra?: (p: LiteHandle) => React.ReactNode }[] = [
  { src: 'heart', title: 'Like', blurb: 'Click to like; the fill pops and eight trim-path rays burst.' },
  { src: 'menu', title: 'Menu', blurb: 'Bars fold into a close icon.' },
  { src: 'theme', title: 'Light / dark', blurb: 'Rays retract, the sun morphs into a crescent.' },
  {
    src: 'status',
    title: 'Form status',
    blurb: 'Loading arc, then a check or a cross.',
    extra: (p) => (
      <div className={styles.sims}>
        <Sim label="Submit (ok)" onClick={() => simulateSubmit(p, true)} />
        <Sim label="Submit (fails)" onClick={() => simulateSubmit(p, false)} />
      </div>
    ),
  },
  { src: 'bell', title: 'Notifications', blurb: 'Rings on a trigger; a badge for unread.' },
  {
    src: 'download',
    title: 'Download',
    blurb: 'Progress is a Property bound to the ring’s trim end.',
    extra: (p) => (
      <div className={styles.sims}>
        <Sim label="Simulate download" onClick={() => simulateDownload(p)} />
        <Sim
          label="Reset"
          onClick={() => {
            p.setInput('state', 'idle')
            p.set('progress', 0)
          }}
        />
      </div>
    ),
  },
  { src: 'copy', title: 'Copy', blurb: 'Sheets trim out, a check draws in, then it returns.' },
  { src: 'arrow', title: 'Link arrow', blurb: 'Slides forward and redraws its shaft.' },
  { src: 'loader', title: 'Loader', blurb: 'Trim start and end chase each other.' },
  { src: 'toasty', title: 'Mascot', blurb: 'Blinks; hops and blushes on hover; boops on click.' },
  { src: 'toggle', title: 'Switch', blurb: 'The knob stretches across; the track fills.' },
  { src: 'checkbox', title: 'Checkbox', blurb: 'The box fills and a check draws on.' },
  { src: 'play', title: 'Play / pause', blurb: 'The triangle morphs into two bars.' },
  { src: 'volume', title: 'Volume', blurb: 'Waves trim away and a cross draws in.' },
  { src: 'bookmark', title: 'Bookmark', blurb: 'The ribbon fills downward and settles.' },
  { src: 'eye', title: 'Show password', blurb: 'Blinks shut with a slash; looks around on hover.' },
  { src: 'lock', title: 'Lock', blurb: 'The shackle drops and the lock kicks.' },
  { src: 'trash', title: 'Delete', blurb: 'The lid lifts on hover and flips on click.' },
  { src: 'send', title: 'Send', blurb: 'The plane launches with a trail and returns.' },
  { src: 'refresh', title: 'Refresh', blurb: 'A full turn with overshoot.' },
  { src: 'chevron', title: 'Disclosure', blurb: 'Flips for accordions and dropdowns.' },
  { src: 'mic', title: 'Record', blurb: 'The capsule fills red and pulses.' },
]

export default function Playground() {
  return (
    <Layout title="Playground" description="Every Toasty Lite component with a live control panel for its inputs, properties, states and events.">
      <main className={styles.page}>
        <header className={styles.head}>
          <h1>Playground</h1>
          <p>
            Every Toasty Lite component, live. The panel under each one is generated from the file itself: flip its inputs, fire its triggers, change its properties, and watch the states and events it reports. Click and hover the animations too; the panel follows. Each one is a template in the editor (Add → Templates) and ships as a few kilobytes of JSON. <Link to="/runtimes/lite">How to use them</Link>.
          </p>
        </header>
        <section className={styles.grid}>
          {CARDS.map((c) => (
            <LiteCard key={c.src} {...c} />
          ))}
        </section>
      </main>
    </Layout>
  )
}
