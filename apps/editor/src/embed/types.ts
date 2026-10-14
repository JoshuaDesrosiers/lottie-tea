/**
 * Hand-written TypeScript declarations shipped with "Export → Website
 * embed" (next to the bundled `.js` files, which carry no types). Keep in
 * step with `player/engine.ts`, `embed/index.ts` and `embed/react.tsx`.
 */

const PLAYER_TYPES = `/** Bytes, or a URL to fetch them from. */
export type Source = string | URL | ArrayBuffer | Uint8Array

export interface PlayerOptions {
  /** The game: a .teab file (URL or bytes). */
  game: Source
  /** The engine (an1mbyt3_wasm_bg.wasm). Default: next to \`game\`. */
  wasm?: Source
  /** Scene to open on (by name). Default: the first scene. */
  scene?: string
  /** 'contain' (letterboxed, default) or 'cover' (fill, crop overflow). */
  fit?: 'contain' | 'cover'
  /** Keys reach the game only while it has focus ('focus', default), from anywhere ('window'), or never ('none'). */
  keyboard?: 'focus' | 'window' | 'none'
  /** Starting data (same as calling set() for each). */
  data?: Record<string, unknown>
  paused?: boolean
  /** Letterbox colour. */
  background?: string
  /** Show the frame profiler (F3 toggles it too). */
  stats?: boolean
}

export interface StateChange {
  /** null = the base layer, else the state machine layer's name. */
  layer: string | null
  state: string
  previous: string | null
}

export interface PlayerEvents {
  ready: { scenes: string[]; scene: string }
  scene: { scene: string; previous: string | null }
  state: StateChange
  /** A scene Property the game changed. */
  prop: { name: string; value: unknown }
  /** ctx.emit(name, payload) in a script, or an On Click "Call script function" no script handles. */
  event: { name: string; payload: unknown }
  error: { error: unknown }
}

export type PlayerEventType = keyof PlayerEvents

export interface An1Player {
  readonly container: HTMLElement
  readonly canvas: HTMLCanvasElement
  readonly scenes: string[]
  readonly scene: string | null
  /** The base layer's current state. */
  readonly state: string | null
  readonly layerStates: Readonly<Record<string, string>>
  readonly paused: boolean
  /** Data for the game: ctx.data[name], and the scene Property \`name\` (bound labels update). Applied next frame. */
  set(name: string, value: unknown): void
  setData(values: Record<string, unknown>): void
  get(name: string): unknown
  /** State machine input (Bool / Number / Enum; a Trigger fires when truthy). */
  setInput(name: string, value: boolean | number | string): void
  fire(trigger: string): void
  loadScene(name: string): void
  /** Calls a script function registered with ctx.on(name, fn). */
  call(name: string, arg?: unknown): unknown
  on<T extends PlayerEventType>(type: T, listener: (event: PlayerEvents[T]) => void): () => void
  off<T extends PlayerEventType>(type: T, listener: (event: PlayerEvents[T]) => void): void
  /** Resolves when the base layer (or \`layer\`) is in \`state\`. */
  waitForState(state: string, layer?: string | null): Promise<void>
  pause(): void
  play(): void
  destroy(): void
}
`

export const EMBED_DTS = `${PLAYER_TYPES}
/** Starts a game inside \`container\` (one player per page; a new one replaces the old). */
export function createPlayer(container: HTMLElement, options: PlayerOptions): Promise<An1Player>

/** Registers <an1-player game="…" scene="…" data='{"…"}' paused fit keyboard wasm>. Events: an1:ready / an1:state / an1:scene / an1:prop / an1:event / an1:error. */
export function defineAn1PlayerElement(tag?: string): void
`

export const REACT_DTS = `import type { CSSProperties, Ref, JSX } from 'react'
import type { An1Player as An1PlayerHandle, PlayerEvents, PlayerOptions, Source } from './an1mbyt3-embed'

export type { An1PlayerHandle, PlayerEvents, PlayerOptions, Source }
export type { StateChange } from './an1mbyt3-embed'

export interface An1PlayerProps {
  /** The game (.teab URL or bytes). Changing it reloads. */
  game: Source
  wasm?: Source
  /** Changing it switches scene. */
  scene?: string
  /** ctx.data + same-named scene Properties. Only changed keys are sent. */
  data?: Record<string, unknown>
  /** State machine inputs by name. Only changed keys are sent. */
  inputs?: Record<string, boolean | number | string>
  paused?: boolean
  fit?: PlayerOptions['fit']
  keyboard?: PlayerOptions['keyboard']
  background?: string
  className?: string
  style?: CSSProperties
  ref?: Ref<An1PlayerHandle | null>
  onReady?: (player: An1PlayerHandle) => void
  onEvent?: (event: PlayerEvents['event']) => void
  onStateChange?: (event: PlayerEvents['state']) => void
  onSceneChange?: (event: PlayerEvents['scene']) => void
  onPropChange?: (event: PlayerEvents['prop']) => void
  onError?: (error: unknown) => void
}

export function An1Player(props: An1PlayerProps): JSX.Element

/** const [ref, player] = useAn1Player(); <An1Player ref={ref} …/> */
export function useAn1Player(): [(instance: An1PlayerHandle | null) => void, An1PlayerHandle | null]
`
