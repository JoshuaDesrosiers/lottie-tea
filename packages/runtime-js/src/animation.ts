/**
 * `@an1mbyt3/runtime`'s core class — a thin, hand-written wrapper around
 * the raw `wasm-bindgen` `WasmPlayer` export (`./wasm-pkg/an1mbyt3_wasm.js`).
 * AN1MBYT3_BUILD_PLAN.md §8: consumers of this package never call
 * `wasm-bindgen` glue directly — they only ever see this class (or the
 * `<animation-player>` custom element / `@an1mbyt3/react`'s `<Animation>`,
 * both built on top of it).
 *
 * Owns the actual `requestAnimationFrame` loop — once created (or once a new
 * `.teab` is loaded), a frame renders every tick regardless of whether
 * playback itself is paused. `pause()` freezes the *sampled pose*, not the
 * render loop itself — same convention any video player uses: pausing
 * doesn't stop the UI from being responsive/redrawn, it just stops the
 * timeline advancing (`WasmPlayer::render` still runs every tick; it's the
 * Rust-side `Driver`'s own `Playback`/`StateMachine` that stops advancing
 * time while paused).
 */
import init, { WasmPlayer } from './wasm-pkg/an1mbyt3_wasm.js'

let wasmInitPromise: Promise<void> | null = null
function ensureWasmInit(): Promise<void> {
  if (!wasmInitPromise) wasmInitPromise = init().then(() => undefined)
  return wasmInitPromise
}

export interface AnimationOptions {
  /** Keeps two named number inputs, `"PointerX"`/`"PointerY"`, updated from
   * the page's live cursor position (in artboard space) — off by default,
   * since most components don't need it and it costs a `window`-level
   * listener for the animation's whole lifetime. See `WasmPlayer::create`'s
   * doc comment (Rust side) for the full reasoning. */
  trackPointer?: boolean
}

/**
 * A named state machine's typed input surface — returned by
 * `An1mbyt3Animation.stateMachine(name)`, matching AN1MBYT3_BUILD_PLAN.md
 * §8's `animation.stateMachine("Character").setNumber(...)` shape.
 *
 * Every call here just forwards to the *one* currently-playing state
 * machine `WasmPlayer` selected on load (see `stateMachine`'s own doc
 * comment for why name-addressed *selection* isn't wired up on the engine
 * side yet) — `name` is only used to warn if it doesn't match what's
 * actually loaded, not to route the call anywhere different.
 */
export class StateMachineHandle {
  constructor(
    private readonly animation: An1mbyt3Animation,
    public readonly name: string,
  ) {}

  setBoolean(inputName: string, value: boolean): void {
    this.animation.player.set_bool_input(inputName, value)
  }

  setNumber(inputName: string, value: number): void {
    this.animation.player.set_number_input(inputName, value)
  }

  setEnum(inputName: string, value: string): void {
    this.animation.player.set_enum_input(inputName, value)
  }

  fireTrigger(inputName: string): void {
    this.animation.player.fire_trigger(inputName)
  }
}

export class An1mbyt3Animation {
  private lastTimestampMs: number | null = null
  private rafHandle: number | null = null
  private destroyed = false

  private constructor(
    /** `public` (not private) so `StateMachineHandle` above can reach the
     * raw `WasmPlayer` without this class needing a forwarding method per
     * input kind twice over. Still not part of this package's *public* API
     * in practice — nothing outside this module imports `WasmPlayer`'s type,
     * and consumers reach it only through the methods below. */
    public readonly player: WasmPlayer,
  ) {}

  /**
   * Creates an animation rendering into `canvas` and starts its internal
   * render loop immediately — it shows the engine's built-in demo scene
   * until `loadTeab` is called. Async for the same reason `WasmPlayer::create`
   * itself is (acquiring a `wgpu` adapter/device is async on every target).
   */
  static async create(canvas: HTMLCanvasElement, options: AnimationOptions = {}): Promise<An1mbyt3Animation> {
    await ensureWasmInit()
    const player = await WasmPlayer.create(canvas, options.trackPointer ?? false)
    const animation = new An1mbyt3Animation(player)
    animation.startRenderLoop()
    return animation
  }

  private startRenderLoop(): void {
    const frame = (timestampMs: number) => {
      if (this.destroyed) return
      if (this.lastTimestampMs === null) this.lastTimestampMs = timestampMs
      const deltaSeconds = (timestampMs - this.lastTimestampMs) / 1000
      this.lastTimestampMs = timestampMs
      this.player.render(deltaSeconds)
      this.rafHandle = requestAnimationFrame(frame)
    }
    this.rafHandle = requestAnimationFrame(frame)
  }

  /**
   * Loads a real, authored `.teab` file, replacing whatever this animation
   * was showing before (the built-in demo scene, or a previously loaded
   * file). Accepts a URL to `fetch()`, or bytes the caller already has.
   */
  async loadTeab(source: string | ArrayBuffer | Uint8Array): Promise<void> {
    const bytes = typeof source === 'string' ? new Uint8Array(await (await fetch(source)).arrayBuffer()) : new Uint8Array(source)
    // `await`ed: `WasmPlayer::load_teab` is now itself `async` — it bakes
    // every image asset the loaded document's `Image` nodes reference (a
    // real GPU readback) before resolving, so any raster content is already
    // visible by the time this promise resolves too.
    await this.player.load_teab(bytes)
  }

  /** Every state machine name declared on the currently loaded artboard. */
  stateMachineNames(): string[] {
    return this.player.state_machine_names()
  }

  /**
   * Returns a typed input handle for the state machine named `name` —
   * matching AN1MBYT3_BUILD_PLAN.md §8's `animation.stateMachine("Character")`
   * shape. Warns (but doesn't throw) if `name` doesn't match any state
   * machine on the currently loaded artboard: `WasmPlayer` only ever drives
   * *one* state machine at a time today (whichever `load_teab`/the built-in
   * demo picked automatically — see the Rust `pick_driver` doc comment), so
   * selecting a *different* one by name isn't wired up on the engine side
   * yet — every `StateMachineHandle` reaches the same underlying inputs
   * regardless of which name it was constructed with. This still lets host
   * code write against the intended API shape now, ahead of that
   * engine-side selection landing.
   */
  stateMachine(name: string): StateMachineHandle {
    const available = this.stateMachineNames()
    if (!available.includes(name)) {
      console.warn(`@an1mbyt3/runtime: no state machine named "${name}" on the loaded artboard (available: ${available.join(', ') || 'none'})`)
    }
    return new StateMachineHandle(this, name)
  }

  play(): void {
    this.player.play()
  }

  pause(): void {
    this.player.pause()
  }

  seek(seconds: number): void {
    this.player.seek(seconds)
  }

  /** Resizes the underlying swap chain — call on canvas/window resize. */
  resize(width: number, height: number): void {
    this.player.resize(width, height)
  }

  /**
   * Stops the render loop and releases the underlying `WasmPlayer` (which
   * detaches its `window` pointer-tracking listener, if one was attached —
   * see `WasmPlayer`'s `Drop` impl, Rust side). Call this when the host page
   * is done with the animation (e.g. a component unmounting) — without it,
   * the render loop keeps calling `requestAnimationFrame` forever, and the
   * underlying WASM-side resources are never freed.
   */
  destroy(): void {
    this.destroyed = true
    if (this.rafHandle !== null) cancelAnimationFrame(this.rafHandle)
    this.player.free()
  }
}
