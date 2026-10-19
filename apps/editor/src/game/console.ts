/**
 * The game Console: everything scripts print (`console.log`, `print`), and
 * every error a game hits — a script that doesn't compile, a script that
 * throws, an error in a callback a script set up (a DOM listener, a timer),
 * an engine error — with *where* it happened: scene, state machine layer,
 * state, and the line / column in that state's code.
 *
 * It is also the error-isolation policy, kept here so it's in one place:
 * - a program that **doesn't compile** is reported once and not run again
 *   until its code changes (`markBroken` / `isBroken`);
 * - a program that **throws** loses just that frame — every other program
 *   and the engine keep running — and the error is reported once, then
 *   counted (a script that throws throws every frame: 60 entries a second
 *   would bury everything else and could run the page out of memory).
 *
 * The Console panel (`components/ConsolePanel.tsx`) subscribes to this
 * store. Entries also go to the browser console, for DevTools users.
 */

export type ConsoleLevel = 'log' | 'warn' | 'error'

/** Where an entry came from. */
export interface ConsoleOrigin {
  scene?: string
  /** State machine layer (`null` / absent = the base layer). */
  layer?: string | null
  state?: string
  /** 1-based, in that state's code. */
  line?: number
  column?: number
  /** "Script", "Engine", "HTML UI", "Async"… */
  kind?: string
}

export interface ConsoleEntry {
  id: number
  level: ConsoleLevel
  message: string
  origin: ConsoleOrigin
  /** The script-side stack, trimmed to the game's own frames. */
  stack?: string
  /** How many times this exact entry repeated (collapsed). */
  count: number
  /** `performance.now()` of the latest repeat. */
  time: number
}

type Listener = () => void

const MAX_ENTRIES = 600

/** Prefix every compiled program's `//# sourceURL=` uses, so stack frames
 * can be told apart from the editor's own code. */
export const SCRIPT_URL_PREFIX = 'an1script://'

/** Lines `new Function` + the runtime's helper prelude add above a
 * program's first line (2 for the function header, 1 for the helpers). */
export const SCRIPT_LINE_OFFSET = 3

/** Formats anything a script logs, like the browser console would. */
export function formatValue(value: unknown, depth = 0): string {
  if (typeof value === 'string') return depth === 0 ? value : JSON.stringify(value)
  if (value === undefined) return 'undefined'
  if (value === null || typeof value === 'number' || typeof value === 'boolean' || typeof value === 'bigint') return String(value)
  if (typeof value === 'function') return `ƒ ${value.name || 'anonymous'}()`
  if (value instanceof Error) return `${value.name}: ${value.message}`
  if (depth > 2) return Array.isArray(value) ? '[…]' : '{…}'
  if (Array.isArray(value)) {
    const items = value.slice(0, 20).map((v) => formatValue(v, depth + 1))
    return `[${items.join(', ')}${value.length > 20 ? `, … ${value.length - 20} more` : ''}]`
  }
  if (typeof value === 'object') {
    try {
      const entries = Object.entries(value as Record<string, unknown>).slice(0, 12)
      return `{ ${entries.map(([k, v]) => `${k}: ${formatValue(v, depth + 1)}`).join(', ')}${Object.keys(value as object).length > 12 ? ', …' : ''} }`
    } catch {
      return String(value)
    }
  }
  return String(value)
}

/** Pulls the program location out of an error's stack: the first frame in
 * game code (`an1script://Scene/Layer/State:LINE:COL`), shifted back to the
 * program's own line numbers. */
export function locateScriptError(err: unknown): { line?: number; column?: number; stack?: string } {
  const stack = err instanceof Error ? err.stack ?? '' : ''
  const frames = stack.split('\n').filter((l) => l.includes(SCRIPT_URL_PREFIX))
  const m = frames[0]?.match(/:(\d+):(\d+)\)?\s*$/)
  const shifted = frames.map((l) => l.replace(/:(\d+):(\d+)(\)?)\s*$/, (_, ln, col, paren) => `:${Number(ln) - SCRIPT_LINE_OFFSET}:${col}${paren}`).trim())
  return m ? { line: Number(m[1]) - SCRIPT_LINE_OFFSET, column: Number(m[2]), stack: shifted.join('\n') } : { stack: shifted.join('\n') || undefined }
}

/** A syntax error has no position in V8's `new Function` message, so find
 * the first line at which compiling the code so far fails the same way
 * (earlier prefixes either compile or fail with "unexpected end"). Only
 * runs once per broken program. */
export function findSyntaxErrorLine(source: string, message: string, params: string[]): number | undefined {
  const lines = source.split('\n')
  for (let n = 1; n <= lines.length; n++) {
    try {
      // eslint-disable-next-line @typescript-eslint/no-implied-eval
      new Function(...params, lines.slice(0, n).join('\n'))
    } catch (e) {
      if (e instanceof Error && e.message === message) return n
    }
  }
  return undefined
}

export class GameConsole {
  private entries: ConsoleEntry[] = []
  private listeners = new Set<Listener>()
  private nextId = 1
  /** Programs (by source key) that failed to compile, with their error —
   * not run again until their code changes. */
  private broken = new Map<string, string>()
  private notifyQueued = false
  /** Errors since the Console was last looked at (the tab badge). */
  unseenErrors = 0
  /** Bumped on every change (for `useSyncExternalStore`). */
  version = 0

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  list(): readonly ConsoleEntry[] {
    return this.entries
  }

  /** Adds an entry, collapsing it into the previous one if identical
   * (same level, message and place). Listeners are told once per frame at
   * most. */
  add(level: ConsoleLevel, message: string, origin: ConsoleOrigin = {}, stack?: string): void {
    const now = performance.now()
    const last = this.entries[this.entries.length - 1]
    const same = (e: ConsoleEntry) =>
      e.level === level && e.message === message && e.origin.scene === origin.scene && e.origin.state === origin.state && e.origin.layer === origin.layer && e.origin.line === origin.line
    // Errors repeat every frame from the same place, often interleaved with
    // other output — collapse those into their first entry wherever it is.
    const existing = same(last ?? ({} as ConsoleEntry)) ? last : level === 'error' ? this.entries.find(same) : undefined
    if (existing) {
      existing.count++
      existing.time = now
    } else {
      this.entries.push({ id: this.nextId++, level, message, origin, stack, count: 1, time: now })
      if (this.entries.length > MAX_ENTRIES) this.entries.splice(0, this.entries.length - MAX_ENTRIES)
      // New entries also reach DevTools (repeats don't).
      const where = describeOrigin(origin)
      const text = where ? `[${where}] ${message}` : message
      if (level === 'error') console.error(text, ...(stack ? ['\n' + stack] : []))
      else if (level === 'warn') console.warn(text)
      else console.log(text)
    }
    if (level === 'error' && !existing) this.unseenErrors++
    this.changed()
  }

  clear(): void {
    this.entries = []
    this.unseenErrors = 0
    this.changed()
  }

  markSeen(): void {
    if (this.unseenErrors === 0) return
    this.unseenErrors = 0
    this.changed()
  }

  /** A program that failed to compile (by its source key). */
  markBroken(key: string, message: string): void {
    this.broken.set(key, message)
  }

  isBroken(key: string): boolean {
    return this.broken.has(key)
  }

  /** A new play session: give previously broken programs another go (their
   * code may have been fixed). */
  resetBroken(): void {
    this.broken.clear()
  }

  private changed(): void {
    this.version++
    if (this.notifyQueued) return
    this.notifyQueued = true
    // Batched: a frame that logs 50 lines re-renders the panel once.
    requestAnimationFrame(() => {
      this.notifyQueued = false
      for (const l of this.listeners) l()
    })
  }
}

/** "Scene › Layer › State : 12:5" */
export function describeOrigin(o: ConsoleOrigin): string {
  const parts = [o.scene, o.layer ?? undefined, o.state].filter(Boolean)
  const place = parts.join(' › ')
  const at = o.line ? `:${o.line}${o.column ? `:${o.column}` : ''}` : ''
  return [o.kind && !place ? o.kind : '', place + at].filter(Boolean).join(' ')
}

/** The one game console (also `window.__an1Console`, for DevTools). */
export let gameConsole = new GameConsole()
if (typeof window !== 'undefined') (window as unknown as { __an1Console: GameConsole }).__an1Console = gameConsole

/** Makes `next` the active console (live binding); returns the one it
 * replaced. Each embedded player logs to its own console, so its `log`
 * events carry only its own game's lines (`runtimeContext.ts`).
 * `window.__an1Console` stays the editor's / first one. */
export function swapGameConsole(next: GameConsole): GameConsole {
  const previous = gameConsole
  gameConsole = next
  return previous
}

/** What scripts get as `console` (and `print`): writes to the game Console,
 * tagged with where the script is running. */
export function scriptConsole(origin: () => ConsoleOrigin) {
  const out = (level: ConsoleLevel) => (...args: unknown[]) => gameConsole.add(level, args.map((a) => formatValue(a)).join(' '), { ...origin(), kind: 'Script' })
  return { log: out('log'), info: out('log'), debug: out('log'), warn: out('warn'), error: out('error') }
}

let installed = false
/** Catches errors that escape the frame loop — a callback a script handed
 * to the DOM or a timer throws later, outside any try/catch — and reports
 * them in the Console instead of leaving them only in DevTools. Errors from
 * the editor's own code are left alone. */
export function installAsyncErrorCapture(): void {
  if (installed || typeof window === 'undefined') return
  installed = true
  const fromScript = (err: unknown) => err instanceof Error && (err.stack ?? '').includes(SCRIPT_URL_PREFIX)
  const report = (err: unknown) => {
    const where = locateScriptError(err)
    const origin = originFromStack(where.stack)
    gameConsole.add('error', err instanceof Error ? `${err.name}: ${err.message}` : String(err), { ...origin, line: where.line, column: where.column, kind: 'Async' }, where.stack)
  }
  window.addEventListener('error', (e) => {
    if (fromScript(e.error)) {
      report(e.error)
      e.preventDefault()
    }
  })
  window.addEventListener('unhandledrejection', (e) => {
    if (fromScript(e.reason)) {
      report(e.reason)
      e.preventDefault()
    }
  })
}

/** Scene / layer / state from a game stack frame's URL
 * (`an1script://Scene/Layer/State`, layer `-` = base). */
function originFromStack(stack: string | undefined): ConsoleOrigin {
  const m = stack?.match(/an1script:\/\/([^/]*)\/([^/]*)\/([^:]*):/)
  if (!m) return {}
  const dec = (s: string) => decodeURIComponent(s)
  return { scene: dec(m[1]), layer: m[2] === '-' ? null : dec(m[2]), state: dec(m[3]) }
}
