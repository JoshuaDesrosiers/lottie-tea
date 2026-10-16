/**
 * `<ScriptLab>` — edit a Code program and run it live in a real Toasty
 * engine scene, with the script console underneath.
 *
 * The scene is one of the docs' lab scenes (`static/games/lab.teab`,
 * `lab3d.teab`; built in `crates/an1mbyt3-wasm/src/demo_lab.rs`), and the
 * engine embed's `program` option swaps every state's program for the
 * code in the editor. Press Run (or Ctrl/⌘ + Enter) to start it with the
 * current code; Reset puts the page's original example back.
 *
 * Optional `controls` add buttons that act on the running scene the way a
 * web page would: set a state machine input, fire a trigger, or set a
 * scene Property.
 */
import React, { useCallback, useRef, useState, type ReactNode } from 'react'
import { Editor } from 'react-live'
import { usePrismTheme } from '@docusaurus/theme-common'
import { ToastyPlayer, type ToastyHandle } from './ToastyPlayer'
import styles from './lab.module.css'

type Control = { label: string; input?: [string, boolean | number | string]; fire?: string; set?: [string, unknown] }

export interface ScriptLabProps {
  /** `lab` (2D: player, ball, crate, ground, coin0–4, label, hud) or `lab3d` (cube, sphere, torus). */
  scene?: 'lab' | 'lab3d'
  code: string
  title?: ReactNode
  /** Stage height in px (the scene is 480 × 270, letterboxed). */
  height?: number
  controls?: Control[]
  /** A short hint under the stage, e.g. which keys the example uses. */
  hint?: ReactNode
}

interface Line {
  level: 'log' | 'warn' | 'error'
  message: string
  line?: number
  count: number
}

export function ScriptLab({ scene = 'lab', code, title, height = 270, controls, hint }: ScriptLabProps) {
  const initial = code.replace(/^\n+|\s+$/g, '') + '\n'
  const [source, setSource] = useState(initial)
  const [program, setProgram] = useState(initial)
  const [run, setRun] = useState(0)
  const [lines, setLines] = useState<Line[]>([])
  const [running, setRunning] = useState(false)
  const player = useRef<ToastyHandle | null>(null)
  const prism = usePrismTheme()

  const start = useCallback(() => {
    setLines([])
    setProgram(source)
    setRun((r) => r + 1)
  }, [source])

  const onKeyDown = (e: React.KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault()
      start()
    }
  }

  const act = (c: Control) => {
    const p = player.current
    if (!p) return
    if (c.input) p.setInput(c.input[0], c.input[1])
    if (c.fire) p.fire(c.fire)
    if (c.set) p.set(c.set[0], c.set[1])
  }

  return (
    <figure className={styles.lab}>
      <div className={styles.bar}>
        <span className={styles.title}>{title ?? 'Script Lab'}</span>
        <span className={styles.spacer} />
        <button type="button" className={styles.ghost} onClick={() => setSource(initial)} title="Put the original example back">
          Reset
        </button>
        <button type="button" className={styles.run} onClick={start} title="Run (Ctrl + Enter)">
          {run ? 'Run again' : 'Run'}
        </button>
      </div>
      <div className={styles.body}>
        <div className={styles.editor} onKeyDown={onKeyDown}>
          <Editor code={source} language="javascript" theme={prism} onChange={setSource} className={styles.code} />
        </div>
        <div className={styles.stage}>
          <ToastyPlayer
            ref={player}
            game={`/games/${scene}.teab`}
            program={program}
            startSignal={run}
            keyboard="focus"
            label="Run the script"
            style={{ height }}
            onReady={() => setRunning(true)}
            onLog={(e) =>
              setLines((list) => {
                const last = list[list.length - 1]
                if (last && last.message === e.message && last.level === e.level) return [...list.slice(0, -1), { ...last, count: e.count }]
                return [...list.slice(-60), e]
              })
            }
          />
          {controls && controls.length > 0 && (
            <div className={styles.controls}>
              {controls.map((c) => (
                <button key={c.label} type="button" disabled={!running} onClick={() => act(c)}>
                  {c.label}
                </button>
              ))}
            </div>
          )}
          {hint && <div className={styles.hint}>{hint}</div>}
          <div className={styles.console} aria-live="polite">
            <div className={styles.consoleHead}>Console</div>
            {lines.length === 0 ? (
              <div className={styles.empty}>console.log output and errors appear here.</div>
            ) : (
              lines.map((l, i) => (
                <div key={i} className={l.level === 'error' ? styles.err : l.level === 'warn' ? styles.warn : styles.log}>
                  {l.line ? <span className={styles.lineNo}>line {l.line}</span> : null}
                  {l.message}
                  {l.count > 1 ? <span className={styles.count}>×{l.count}</span> : null}
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </figure>
  )
}
