/**
 * `<ToastyPlayer>` — the docs' version of the real `<An1Player>` from
 * `an1mbyt3-react.js`, with exactly the same props, plus two docs-only
 * conveniences:
 *
 * - **Click to run.** The engine is big (it's the whole Toasty runtime), so
 *   nothing downloads until the reader presses Run.
 * - **One at a time.** The engine runs one game per page. Starting a
 *   player here stops whichever other one is running (it goes back to its
 *   poster), so pages can have many demos.
 *
 * The engine and the embed library are served from `static/toasty/`
 * (copied from the editor's `public/embed/` build) and imported at runtime,
 * so the docs never bundle them. Game paths that start with `/` are
 * resolved against the site's base URL (`/games/x.teab` → `/docs/games/x.teab`).
 */
import React, { forwardRef, useEffect, useImperativeHandle, useRef, useState, useSyncExternalStore, type CSSProperties, type ReactNode } from 'react'
import useBaseUrl from '@docusaurus/useBaseUrl'
import styles from './toasty.module.css'

/** The running game's API — the same object `createPlayer` resolves to. */
export interface ToastyHandle {
  scene: string
  scenes: string[]
  set(name: string, value: unknown): void
  setData(data: Record<string, unknown>): void
  setInput(name: string, value: boolean | number | string): void
  fire(name: string): void
  loadScene(name: string): void
  call(name: string, arg?: unknown): void
  on(type: string, listener: (event: any) => void): () => void
  waitForState(state: string, layer?: string | null): Promise<unknown>
  pause(): void
  play(): void
  destroy(): void
  [key: string]: unknown
}

export interface ToastyPlayerProps {
  game: string | Uint8Array | ArrayBuffer
  wasm?: string
  scene?: string
  data?: Record<string, unknown>
  inputs?: Record<string, boolean | number | string>
  paused?: boolean
  fit?: 'contain' | 'cover'
  keyboard?: 'focus' | 'window' | 'none'
  background?: string
  /** Engine embed `program`: run this Code program in every state. */
  program?: string
  className?: string
  style?: CSSProperties
  /** Docs only: start without waiting for a click. */
  autoStart?: boolean
  /** Docs only: changing this number starts (or restarts) the player. */
  startSignal?: number
  /** Docs only: what the Run poster says. */
  label?: ReactNode
  onReady?: (player: ToastyHandle) => void
  onEvent?: (event: { name: string; payload: unknown }) => void
  onStateChange?: (event: { layer: string | null; state: string; previous: string | null }) => void
  onSceneChange?: (event: { scene: string; previous: string | null }) => void
  onPropChange?: (event: { name: string; value: unknown }) => void
  onError?: (error: unknown) => void
  /** Console lines from the game's scripts (engine `log` event). */
  onLog?: (event: { level: 'log' | 'warn' | 'error'; message: string; line?: number; count: number }) => void
}

// ---- One running player per page ----

let activeId: number | null = null
const subscribers = new Set<() => void>()
const activeStore = {
  subscribe(fn: () => void) {
    subscribers.add(fn)
    return () => subscribers.delete(fn)
  },
  get: () => activeId,
  set(id: number | null) {
    activeId = id
    subscribers.forEach((fn) => fn())
  },
}
let nextId = 1
/** Serialises create/destroy so a new player never boots before the old one is gone. */
let engineQueue: Promise<unknown> = Promise.resolve()

type EmbedModule = { createPlayer: (box: HTMLElement, options: Record<string, unknown>) => Promise<ToastyHandle> }
let embedModule: Promise<EmbedModule> | null = null
function loadEmbed(url: string): Promise<EmbedModule> {
  embedModule ??= import(/* webpackIgnore: true */ url) as Promise<EmbedModule>
  return embedModule
}

function changed<T>(before: Record<string, T> | undefined, after: Record<string, T> | undefined): [string, T][] {
  if (!after) return []
  return Object.entries(after).filter(([key, value]) => !before || !(key in before) || !Object.is(before[key], value))
}

export const ToastyPlayer = forwardRef<ToastyHandle | null, ToastyPlayerProps>(function ToastyPlayer(props, ref) {
  const [id] = useState(() => nextId++)
  const active = useSyncExternalStore(activeStore.subscribe, activeStore.get, () => null) === id
  const boxRef = useRef<HTMLDivElement>(null)
  const [player, setPlayer] = useState<ToastyHandle | null>(null)
  const [status, setStatus] = useState<'idle' | 'loading' | 'error'>('idle')
  const latest = useRef(props)
  latest.current = props
  const sentData = useRef<Record<string, unknown> | undefined>(undefined)
  const sentInputs = useRef<Record<string, boolean | number | string> | undefined>(undefined)
  const embedUrl = useBaseUrl('/toasty/an1mbyt3-embed.js')
  const defaultWasm = useBaseUrl('/toasty/an1mbyt3_wasm_bg.wasm')
  const resolvedGame = useBaseUrl(typeof props.game === 'string' ? props.game : '/')
  const game = typeof props.game === 'string' ? resolvedGame : props.game
  const wasmPath = useBaseUrl(props.wasm ?? '/toasty/an1mbyt3_wasm_bg.wasm')
  const wasm = props.wasm ? wasmPath : defaultWasm

  useImperativeHandle(ref, () => player as ToastyHandle, [player])

  useEffect(() => {
    if (props.startSignal) activeStore.set(id)
  }, [props.startSignal])

  useEffect(() => {
    if (props.autoStart && activeStore.get() === null) activeStore.set(id)
    return () => {
      if (activeStore.get() === id) activeStore.set(null)
    }
  }, [])

  useEffect(() => {
    if (!active) return
    const box = boxRef.current
    if (!box) return
    let cancelled = false
    let made: ToastyHandle | null = null
    setStatus('loading')
    const p = latest.current
    sentData.current = p.data ? { ...p.data } : undefined
    sentInputs.current = undefined
    const absolute = (u: string) => new URL(u, window.location.href).href
    engineQueue = engineQueue.then(async () => {
      if (cancelled) return
      try {
        const { createPlayer } = await loadEmbed(absolute(embedUrl))
        const instance = await createPlayer(box, {
          game: typeof game === 'string' ? absolute(game) : game,
          wasm: absolute(wasm),
          scene: p.scene,
          data: p.data,
          paused: p.paused,
          fit: p.fit,
          keyboard: p.keyboard,
          background: p.background,
          program: p.program,
        })
        if (cancelled) {
          instance.destroy()
          return
        }
        made = instance
        instance.on('event', (e) => latest.current.onEvent?.(e))
        instance.on('state', (e) => latest.current.onStateChange?.(e))
        instance.on('scene', (e) => latest.current.onSceneChange?.(e))
        instance.on('prop', (e) => latest.current.onPropChange?.(e))
        instance.on('error', (e) => latest.current.onError?.(e.error))
        instance.on('log', (e) => latest.current.onLog?.(e))
        for (const [name, value] of changed(undefined, latest.current.inputs)) instance.setInput(name, value)
        sentInputs.current = latest.current.inputs ? { ...latest.current.inputs } : undefined
        setPlayer(instance)
        setStatus('idle')
        latest.current.onReady?.(instance)
      } catch (error) {
        if (cancelled) return
        console.error(error)
        setStatus('error')
        latest.current.onError?.(error)
      }
    })
    return () => {
      cancelled = true
      setPlayer(null)
      const doomed = made
      engineQueue = engineQueue.then(() => doomed?.destroy())
      made = null
    }
  }, [active, game, wasm, props.fit, props.keyboard, props.background, props.program, props.startSignal])

  useEffect(() => {
    if (!player) return
    for (const [name, value] of changed(sentData.current, props.data)) player.set(name, value)
    sentData.current = props.data ? { ...props.data } : undefined
  }, [player, props.data])
  useEffect(() => {
    if (!player) return
    for (const [name, value] of changed(sentInputs.current, props.inputs)) player.setInput(name, value)
    sentInputs.current = props.inputs ? { ...props.inputs } : undefined
  }, [player, props.inputs])
  useEffect(() => {
    if (player && props.scene && props.scene !== player.scene) player.loadScene(props.scene)
  }, [player, props.scene])
  useEffect(() => {
    if (!player) return
    if (props.paused) player.pause()
    else player.play()
  }, [player, props.paused])

  const height = props.style?.height ?? 320
  return (
    <div className={`${styles.frame} ${props.className ?? ''}`} style={{ ...props.style, height }}>
      {active ? (
        <>
          <div ref={boxRef} className={styles.box} />
          {status === 'loading' && <div className={styles.overlay}>Loading the Toasty engine…</div>}
          {status === 'error' && <div className={styles.overlay}>This demo couldn't start. Check the browser console.</div>}
          <button type="button" className={styles.stop} onClick={() => activeStore.set(null)} title="Stop this demo">
            Stop
          </button>
        </>
      ) : (
        <button type="button" className={styles.poster} onClick={() => activeStore.set(id)}>
          <span className={styles.play} aria-hidden="true" />
          <span className={styles.posterLabel}>{props.label ?? 'Run live demo'}</span>
          <span className={styles.posterHint}>{typeof props.game === 'string' ? props.game.split('/').pop() : 'game'} · runs in your browser</span>
        </button>
      )}
    </div>
  )
})

/** `<ToastyDemo>` — a captioned, click-to-run embedded game for MDX pages. */
export function ToastyDemo({ caption, height = 360, ...rest }: Omit<ToastyPlayerProps, 'style'> & { caption?: ReactNode; height?: number }) {
  return (
    <figure className={styles.figure}>
      <ToastyPlayer {...rest} style={{ height }} />
      {caption && <figcaption className={styles.caption}>{caption}</figcaption>}
    </figure>
  )
}

/** `<Shot>` — an editor screenshot from `static/img/shots/<id>.png`.
 * Renders nothing until the image has actually loaded, so a shot that
 * hasn't been captured yet never shows as a broken image (the load error
 * can fire before hydration, so the effect re-checks `complete`). */
export function Shot({ id, caption }: { id: string; caption?: ReactNode }) {
  const src = useBaseUrl(`/img/shots/${id}.png`)
  const img = useRef<HTMLImageElement>(null)
  const [state, setState] = useState<'loading' | 'ok' | 'missing'>('loading')
  useEffect(() => {
    const el = img.current
    if (el?.complete) setState(el.naturalWidth > 0 ? 'ok' : 'missing')
  }, [])
  if (state === 'missing') return null
  return (
    <figure className={styles.figure} style={state === 'ok' ? undefined : { display: 'none' }}>
      <img ref={img} className={styles.shot} src={src} alt={typeof caption === 'string' ? caption : id} onLoad={() => setState('ok')} onError={() => setState('missing')} />
      {caption && <figcaption className={styles.caption}>{caption}</figcaption>}
    </figure>
  )
}
