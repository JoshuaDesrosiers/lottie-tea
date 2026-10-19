/**
 * <An1Player> for React, loading the engine from a **Toasty shared
 * runtime** — written by "Export → Website embed (shared runtime)".
 *
 * Drop this file into your React app (React 18 or 19, any bundler: Vite,
 * Next.js, webpack…). It has no npm dependency: on first use it imports
 * the player from the runtime folder below, at run time, and every
 * <An1Player> on the page shares that one engine download.
 *
 * ```tsx
 * import { An1Player } from './An1Player'
 *
 * <An1Player
 *   game="/game.teab"
 *   style={{ width: '100%', height: 480 }}
 *   data={{ userName: 'Ada' }}                       // ctx.data + scene Properties
 *   inputs={{ isOpen: menuOpen }}                    // state machine inputs
 *   onEvent={(e) => console.log(e.name, e.payload)}  // ctx.emit from the game
 * />
 * ```
 *
 * **Self-hosting** the runtime (unzipped `toasty-runtime-<version>.zip`)?
 * Change `RUNTIME_URL` below — or pass `runtimeUrl` per player.
 *
 * (Plain JavaScript project? Delete the type annotations, or rename the
 * file to `.jsx` after removing them.)
 *
 * This file is also type-checked inside the Toasty editor repo
 * (`apps/editor/src/embed/sharedRuntime/An1Player.tsx`); the export
 * replaces the RUNTIME_URL placeholder value with the real runtime URL.
 */
import { forwardRef, useEffect, useImperativeHandle, useRef, useState, type CSSProperties } from 'react'

/** The shared runtime folder (ends with `/`). */
export const RUNTIME_URL = '{{RUNTIME_URL}}'

/** The running game's API (`ref.current`). */
export interface An1PlayerHandle {
  readonly scenes: string[]
  readonly scene: string | null
  readonly state: string | null
  readonly layerStates: Readonly<Record<string, string>>
  readonly paused: boolean
  set(name: string, value: unknown): void
  setData(values: Record<string, unknown>): void
  get(name: string): unknown
  setInput(name: string, value: boolean | number | string): void
  fire(trigger: string): void
  loadScene(name: string): void
  call(name: string, arg?: unknown): unknown
  on(type: string, listener: (event: any) => void): () => void
  waitForState(state: string, layer?: string | null): Promise<void>
  pause(): void
  play(): void
  destroy(): void
}

export interface StateChange {
  /** `null` = the base layer, else the layer's name. */
  layer: string | null
  state: string
  previous: string | null
}

export interface An1PlayerProps {
  /** The game (`.teab` URL or bytes). Changing it reloads. */
  game: string | ArrayBuffer | Uint8Array
  /** The shared runtime folder; default `RUNTIME_URL`. */
  runtimeUrl?: string
  /** The scene to show — changing it switches scene. */
  scene?: string
  /** `ctx.data` + same-named scene Properties. Only changed keys are sent. */
  data?: Record<string, unknown>
  /** State machine inputs by name. Only changed keys are sent. */
  inputs?: Record<string, boolean | number | string>
  paused?: boolean
  fit?: 'contain' | 'cover'
  keyboard?: 'focus' | 'window' | 'none'
  background?: string
  /** Pause while scrolled out of view (default true). */
  pauseWhenOffscreen?: boolean
  className?: string
  style?: CSSProperties
  onReady?: (player: An1PlayerHandle) => void
  onEvent?: (event: { name: string; payload: unknown }) => void
  onStateChange?: (event: StateChange) => void
  onSceneChange?: (event: { scene: string; previous: string | null }) => void
  onPropChange?: (event: { name: string; value: unknown }) => void
  onError?: (error: unknown) => void
}

type CreatePlayer = (container: HTMLElement, options: Record<string, unknown>) => Promise<An1PlayerHandle>

/** One import of the runtime per folder, shared by every <An1Player>. */
const runtimes = new Map<string, Promise<{ createPlayer: CreatePlayer }>>()
function loadRuntime(url: string): Promise<{ createPlayer: CreatePlayer }> {
  const base = url.endsWith('/') ? url : `${url}/`
  let runtime = runtimes.get(base)
  if (!runtime) {
    // A run-time import from a URL: tell bundlers to leave it alone.
    runtime = import(/* @vite-ignore */ /* webpackIgnore: true */ `${base}an1mbyt3-embed.js`) as Promise<{ createPlayer: CreatePlayer }>
    runtime.catch(() => runtimes.delete(base))
    runtimes.set(base, runtime)
  }
  return runtime
}

/** Shallow diff: keys whose value changed since `before`. */
function changed<T>(before: Record<string, T> | undefined, after: Record<string, T> | undefined): [string, T][] {
  if (!after) return []
  return Object.entries(after).filter(([key, value]) => !before || !(key in before) || !Object.is(before[key], value))
}

export const An1Player = forwardRef<An1PlayerHandle | null, An1PlayerProps>(function An1Player(props, ref) {
  const boxRef = useRef<HTMLDivElement>(null)
  const [player, setPlayer] = useState<An1PlayerHandle | null>(null)
  const latest = useRef(props)
  latest.current = props
  const sentData = useRef<Record<string, unknown> | undefined>(undefined)
  const sentInputs = useRef<Record<string, boolean | number | string> | undefined>(undefined)

  useImperativeHandle(ref, () => player as An1PlayerHandle, [player])

  const runtimeUrl = props.runtimeUrl ?? RUNTIME_URL
  useEffect(() => {
    const box = boxRef.current
    if (!box) return
    let cancelled = false
    let made: An1PlayerHandle | null = null
    const p = latest.current
    sentData.current = p.data ? { ...p.data } : undefined
    sentInputs.current = undefined
    loadRuntime(runtimeUrl)
      .then(({ createPlayer }) =>
        createPlayer(box, {
          game: p.game,
          runtimeUrl,
          scene: p.scene,
          data: p.data,
          paused: p.paused,
          fit: p.fit,
          keyboard: p.keyboard,
          background: p.background,
          pauseWhenOffscreen: p.pauseWhenOffscreen,
        }),
      )
      .then((instance) => {
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
        for (const [name, value] of changed(undefined, latest.current.inputs)) instance.setInput(name, value)
        sentInputs.current = latest.current.inputs ? { ...latest.current.inputs } : undefined
        setPlayer(instance)
        latest.current.onReady?.(instance)
      })
      .catch((error) => {
        if (!cancelled) latest.current.onError?.(error)
        else console.error(error)
      })
    return () => {
      cancelled = true
      made?.destroy()
      setPlayer(null)
    }
  }, [props.game, runtimeUrl, props.fit, props.keyboard, props.background, props.pauseWhenOffscreen])

  // Live updates — only what changed.
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

  return <div ref={boxRef} className={props.className} style={{ position: 'relative', overflow: 'hidden', ...props.style }} />
})
