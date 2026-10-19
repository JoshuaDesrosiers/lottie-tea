/**
 * **Runtime contexts** — what lets several full-engine players run on one
 * page (`player/engine.ts`, the website embed), each with its own game.
 *
 * The game runtime grew up inside the editor, where there is exactly one
 * game at a time, so a handful of its modules keep their state at module
 * level and every other module imports it directly:
 *
 * | module                  | state                                   |
 * |-------------------------|-----------------------------------------|
 * | `props.ts`              | `sceneProps` (+ the property defs)      |
 * | `projectFiles.ts`       | the project's code / markup files       |
 * | `events.ts`             | `ctx.on` script functions               |
 * | `embedBridge.ts`        | page data (`ctx.data`) + `ctx.emit` hook |
 * | `htmlOverlay.ts`        | the HTML UI layer over the canvas       |
 * | `console.ts`            | the game console (`log` events)         |
 * | `adaptiveResolution.ts` | the canvas render scale                 |
 * | `audio.ts`              | the sound engine (voices, master volume) |
 * | `net.ts`                | the multiplayer connection (`ctx.net`)  |
 *
 * Rather than thread an instance through every call site (hundreds of
 * them, shared with the editor), each of those exports is an ES module
 * **live binding** (`export let`) with a `swap…` function. A
 * `RuntimeContext` is one full set of instances; `runInRuntimeContext`
 * swaps a player's set in, runs a function, and swaps the previous set
 * back. JavaScript is single-threaded and a player's frame is synchronous,
 * so for the duration of that frame (or that API call, resize, or HTML UI
 * event) every module sees only that player's state — and the editor's
 * own state (the default context, captured at load) is always restored
 * afterwards. A swap is nine variable assignments, so doing it around
 * every frame of every player costs nothing measurable.
 *
 * The one gap: code a game's script schedules *itself* outside a frame
 * (a `setTimeout`, a `fetch().then`) runs in whatever context is active
 * then — normally the default one. Objects scripts hold onto (`ctx.props`,
 * `ctx.net`, `ctx.data`) are bound to their own player's instances, so the
 * common cases still work; module-level helpers called from such a
 * callback (`console.log` → the game console) land in the default
 * context.
 *
 * Shared on purpose (one per page, not per player): the wasm module
 * itself, compiled scripts (`scriptRuntime.ts`, keyed by source), linked
 * Google Fonts, and `gameStorage` (localStorage).
 */
import { SceneProps, swapSceneProps } from './props'
import { swapProjectFiles } from './projectFiles'
import { swapFunctions } from './events'
import { EmbedBridge, swapEmbedBridge } from './embedBridge'
import { HtmlOverlay, swapHtmlOverlay } from './htmlOverlay'
import { GameConsole, swapGameConsole } from './console'
import { AdaptiveResolution, swapAdaptiveResolution } from './adaptiveResolution'
import { AudioEngine, swapAudio } from './audio'
import { NetClient, swapGameNet } from './net'

/** One game's full set of runtime state (see the module doc). */
export interface RuntimeContext {
  sceneProps: SceneProps
  projectFiles: Map<string, string>
  functions: Map<string, (arg?: unknown) => unknown>
  embedBridge: EmbedBridge
  htmlOverlay: HtmlOverlay
  gameConsole: GameConsole
  adaptiveResolution: AdaptiveResolution
  audio: AudioEngine
  gameNet: NetClient
}

/** A fresh, empty context — one per embedded player. */
export function createRuntimeContext(): RuntimeContext {
  return {
    sceneProps: new SceneProps(),
    projectFiles: new Map(),
    functions: new Map(),
    embedBridge: new EmbedBridge(),
    htmlOverlay: new HtmlOverlay(),
    gameConsole: new GameConsole(),
    adaptiveResolution: new AdaptiveResolution(),
    audio: new AudioEngine(),
    gameNet: new NetClient(),
  }
}

/** Installs `ctx` in every module; returns the set it replaced. */
function install(ctx: RuntimeContext): RuntimeContext {
  return {
    sceneProps: swapSceneProps(ctx.sceneProps),
    projectFiles: swapProjectFiles(ctx.projectFiles),
    functions: swapFunctions(ctx.functions),
    embedBridge: swapEmbedBridge(ctx.embedBridge),
    htmlOverlay: swapHtmlOverlay(ctx.htmlOverlay),
    gameConsole: swapGameConsole(ctx.gameConsole),
    adaptiveResolution: swapAdaptiveResolution(ctx.adaptiveResolution),
    audio: swapAudio(ctx.audio),
    gameNet: swapGameNet(ctx.gameNet),
  }
}

/** The context installed right now (`null` = the default one — the
 * module-level instances the page started with). */
let active: RuntimeContext | null = null

/**
 * Runs `fn` with `ctx` installed, then puts back whatever was installed
 * before (also if `fn` throws). Re-entrant: a nested call for the context
 * that is already active just runs `fn`.
 */
export function runInRuntimeContext<T>(ctx: RuntimeContext, fn: () => T): T {
  if (active === ctx) return fn()
  const previousActive = active
  const previous = install(ctx)
  active = ctx
  try {
    return fn()
  } finally {
    install(previous)
    active = previousActive
  }
}

/** Releases what a context holds outside JS memory: its sounds / audio
 * context, its network connection, and its HTML UI layer. */
export function disposeRuntimeContext(ctx: RuntimeContext): void {
  ctx.audio.dispose()
  try {
    ctx.gameNet.leave()
  } catch {
    // Never connected.
  }
  ctx.htmlOverlay.detach()
  ctx.htmlOverlay.eventScope = null
  ctx.embedBridge.listen(null)
  ctx.functions.clear()
}
