/**
 * `<LiteCard>` — one Toasty Lite animation with a live control panel built
 * from the file itself, Rive-style: a switch for every Bool input, a slider
 * for every Number, buttons for every Enum value and Trigger, a colour
 * picker / slider for every Property, the current state of each layer, and
 * the events it sends. Clicking or hovering the animation updates the
 * panel too, because the panel listens to the player.
 */
import React, { useEffect, useRef, useState, type ReactNode } from 'react'
import { Lite, cssToRgba, type LiteHandle } from './Lite'
import styles from './playground.module.css'

interface InputDef {
  name: string
  kind: 'bool' | 'number' | 'enum' | 'trigger'
}
interface PropDef {
  name: string
  kind: 'number' | 'bool' | 'text' | 'color'
  default: unknown
}

/** Enum values a file's transitions test for each enum input. */
function enumValues(file: any): Record<string, string[]> {
  const out: Record<string, string[]> = {}
  for (const layer of file?.stateMachine?.layers ?? [])
    for (const s of layer.states)
      for (const t of s.transitions)
        if (t.condition.type === 'enum') {
          const list = (out[t.condition.input] ??= [])
          if (!list.includes(t.condition.value)) list.push(t.condition.value)
        }
  return out
}

const toHex = (c: number[]) => '#' + c.slice(0, 3).map((v) => Math.round(Math.min(1, Math.max(0, v)) * 255).toString(16).padStart(2, '0')).join('')

export function LiteCard({ src, title, blurb, extra, size = 140 }: { src: string; title: string; blurb: ReactNode; extra?: (p: LiteHandle) => ReactNode; size?: number }) {
  const ref = useRef<LiteHandle | null>(null)
  const [player, setPlayer] = useState<LiteHandle | null>(null)
  const [, bump] = useState(0)
  const [log, setLog] = useState<string[]>([])
  const [states, setStates] = useState<Record<string, string>>({})

  useEffect(() => {
    if (!player) return
    const initial: Record<string, string> = {}
    for (const layer of player.file?.stateMachine?.layers ?? []) initial[layer.name ?? 'Base'] = player.state(layer.name) ?? ''
    setStates(initial)
    const offs = [
      player.on('input', () => bump((n) => n + 1)),
      player.on('prop', () => bump((n) => n + 1)),
      player.on('state', (e) => setStates((s) => ({ ...s, [e.layer ?? 'Base']: e.state }))),
      player.on('event', (e) => setLog((l) => [`event "${e.name}"`, ...l].slice(0, 3))),
    ]
    return () => offs.forEach((off) => off())
  }, [player])

  const file = player?.file
  const inputs: InputDef[] = (file?.stateMachine?.inputs ?? []).filter((i: InputDef) => i.name !== 'hover')
  const hasHover = (file?.stateMachine?.inputs ?? []).some((i: InputDef) => i.name === 'hover')
  const props: PropDef[] = file?.properties ?? []
  const enums = enumValues(file)

  return (
    <article className={styles.card}>
      <div className={styles.stage}>
        <Lite ref={ref} src={src} ink="auto" style={{ width: size, height: size }} onReady={setPlayer} label={title} />
      </div>
      <div className={styles.meta}>
        <h3>
          <code>{src}</code> {title}
        </h3>
        <p>{blurb}</p>
      </div>
      <div className={styles.panel}>
        {inputs.map((i) => {
          const value = player?.getInput(i.name)
          if (i.kind === 'bool')
            return (
              <label key={i.name} className={styles.row}>
                <span className={styles.name}>{i.name}</span>
                <button type="button" role="switch" aria-checked={!!value} className={value ? styles.switchOn : styles.switch} onClick={() => player?.setInput(i.name, !value)}>
                  <i />
                </button>
              </label>
            )
          if (i.kind === 'trigger')
            return (
              <div key={i.name} className={styles.row}>
                <span className={styles.name}>{i.name}</span>
                <button type="button" className={styles.fire} onClick={() => player?.fire(i.name)}>
                  Fire
                </button>
              </div>
            )
          if (i.kind === 'enum')
            return (
              <div key={i.name} className={styles.rowWrap}>
                <span className={styles.name}>{i.name}</span>
                <div className={styles.segments}>
                  {(enums[i.name] ?? []).map((v) => (
                    <button key={v} type="button" className={value === v ? styles.segOn : styles.seg} onClick={() => player?.setInput(i.name, v)}>
                      {v}
                    </button>
                  ))}
                </div>
              </div>
            )
          return (
            <label key={i.name} className={styles.row}>
              <span className={styles.name}>{i.name}</span>
              <input type="range" min={0} max={1} step={0.01} value={Number(value ?? 0)} onChange={(e) => player?.setInput(i.name, Number(e.target.value))} />
            </label>
          )
        })}
        {props.map((p) => {
          const value = player?.get(p.name)
          if (p.kind === 'color')
            return (
              <label key={p.name} className={styles.row}>
                <span className={styles.name}>{p.name}</span>
                <input type="color" value={Array.isArray(value) ? toHex(value as number[]) : '#1c1c21'} onChange={(e) => player?.set(p.name, cssToRgba(e.target.value))} />
              </label>
            )
          if (p.kind === 'number')
            return (
              <label key={p.name} className={styles.row}>
                <span className={styles.name}>{p.name}</span>
                <input type="range" min={0} max={1} step={0.01} value={Number(value ?? 0)} onChange={(e) => player?.set(p.name, Number(e.target.value))} />
              </label>
            )
          return null
        })}
        {player && extra?.(player)}
        <div className={styles.readout}>
          {Object.entries(states).map(([layer, state]) => (
            <span key={layer}>
              {layer}: <b>{state}</b>
            </span>
          ))}
          {hasHover && <span className={styles.dim}>hover drives <b>hover</b></span>}
          {log.map((l, i) => (
            <span key={i} className={styles.event}>
              {l}
            </span>
          ))}
        </div>
      </div>
    </article>
  )
}
