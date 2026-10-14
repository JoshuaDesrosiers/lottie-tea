/**
 * **an1mbyt3 for React** — `<An1Player>`: an exported an1mbyt3 game as a
 * component whose props steer it live. Prop changes are *diffed* and sent
 * to the running game (no remount, no reload, no flicker): pass fetched
 * data straight in and labels / bars / colours bound to it update on the
 * next frame.
 *
 * ```tsx
 * import { An1Player } from './an1mbyt3/an1mbyt3-react.js'
 *
 * const { data } = useQuery(...)
 * <An1Player
 *   game="/an1mbyt3/game.teab"
 *   style={{ width: '100%', height: 400 }}
 *   data={{ price: data?.price, stock: data?.stock }}   // ctx.data + scene Properties
 *   inputs={{ isOpen: menuOpen, speed: 2 }}             // state machine inputs
 *   scene={step === 2 ? 'Checkout' : 'Shop'}            // scene switching
 *   onEvent={(e) => e.name === 'buy' && addToCart(e.payload)}  // ctx.emit
 *   onStateChange={(e) => e.state === 'Success' && toast('Saved!')}
 *   ref={playerRef}                                     // player.fire('pop') etc.
 * />
 * ```
 */
import { useEffect, useImperativeHandle, useRef, useState, type CSSProperties, type Ref } from 'react'
import { createPlayer, type An1Player as An1PlayerHandle, type PlayerEvents, type PlayerOptions, type Source } from '../player/engine'

/** The running game's API (`ref.current`): `set`, `setInput`, `fire`, `loadScene`, `call`, `on`, `waitForState`… */
export type { An1PlayerHandle }
export type { PlayerEvents, PlayerOptions, Source, StateChange } from '../player/engine'

export interface An1PlayerProps {
  /** The game (`.teab` URL or bytes). Changing it reloads. */
  game: Source
  /** The engine (`.wasm`); default: next to `game`. */
  wasm?: Source
  /** The scene to show — changing it switches scene. */
  scene?: string
  /** Data for the game: `ctx.data`, and same-named scene Properties.
   * Only changed keys are sent. */
  data?: Record<string, unknown>
  /** State machine inputs by name (Bool / Number / Enum; a Trigger fires
   * when its value turns truthy). Only changed keys are sent. */
  inputs?: Record<string, boolean | number | string>
  paused?: boolean
  fit?: PlayerOptions['fit']
  keyboard?: PlayerOptions['keyboard']
  background?: string
  className?: string
  style?: CSSProperties
  /** Imperative access: `ref.current.fire('pop')`, `.call('reset')`… */
  ref?: Ref<An1PlayerHandle | null>
  onReady?: (player: An1PlayerHandle) => void
  onEvent?: (event: PlayerEvents['event']) => void
  onStateChange?: (event: PlayerEvents['state']) => void
  onSceneChange?: (event: PlayerEvents['scene']) => void
  onPropChange?: (event: PlayerEvents['prop']) => void
  onError?: (error: unknown) => void
}

/** Shallow diff: keys whose value changed since `before`. */
function changed<T>(before: Record<string, T> | undefined, after: Record<string, T> | undefined): [string, T][] {
  if (!after) return []
  return Object.entries(after).filter(([key, value]) => !before || !(key in before) || !Object.is(before[key], value))
}

export function An1Player(props: An1PlayerProps) {
  const boxRef = useRef<HTMLDivElement>(null)
  const [player, setPlayer] = useState<An1PlayerHandle | null>(null)
  // Latest props for the event handlers / first apply, without
  // re-subscribing or reloading when they change.
  const latest = useRef(props)
  latest.current = props
  const sentData = useRef<Record<string, unknown> | undefined>(undefined)
  const sentInputs = useRef<Record<string, boolean | number | string> | undefined>(undefined)

  useImperativeHandle(props.ref, () => player as An1PlayerHandle, [player])

  // Load (and reload only when the game / engine / box setup changes).
  useEffect(() => {
    const box = boxRef.current
    if (!box) return
    let cancelled = false
    let made: An1PlayerHandle | null = null
    const p = latest.current
    sentData.current = p.data ? { ...p.data } : undefined
    sentInputs.current = undefined
    createPlayer(box, { game: p.game, wasm: p.wasm, scene: p.scene, data: p.data, paused: p.paused, fit: p.fit, keyboard: p.keyboard, background: p.background })
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
        // Inputs given before it was ready.
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
  }, [props.game, props.wasm, props.fit, props.keyboard, props.background])

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
}

/** The player behind an `<An1Player ref>` as state (re-renders once it's
 * ready): `const [ref, player] = useAn1Player()`. */
export function useAn1Player(): [(instance: An1PlayerHandle | null) => void, An1PlayerHandle | null] {
  const [player, setPlayer] = useState<An1PlayerHandle | null>(null)
  return [setPlayer, player]
}
