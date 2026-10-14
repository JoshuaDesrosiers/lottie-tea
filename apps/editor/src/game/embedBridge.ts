/**
 * The link between a running game and the web page it's embedded in
 * (`src/embed`, the React / JS embed API).
 *
 * - **Page → game:** `data` holds whatever the page handed in with
 *   `player.set(name, value)` / `<An1Player data={…}>` — scripts read it
 *   as `ctx.data` (it also lands in same-named scene Properties, so bound
 *   text / colours update with no script at all; see `player/engine.ts`).
 * - **Game → page:** `ctx.emit(name, payload)` in a script, or an On Click
 *   "Call script function" action whose function no script registered,
 *   reaches the page's `on('event', …)` listeners.
 *
 * In the editor nothing listens: emits just show in the Console, so games
 * can be built and tested before they're embedded.
 */
import { gameConsole, type ConsoleOrigin } from './console'

type EmitListener = (name: string, payload: unknown) => void

class EmbedBridge {
  /** Page-provided data (`ctx.data`). Replaced, never mutated in place,
   * so a script holding last frame's object sees a stable snapshot. */
  data: Readonly<Record<string, unknown>> = Object.freeze({})
  private listener: EmitListener | null = null

  /** Whether a page is listening (an embedded player). */
  get embedded(): boolean {
    return this.listener !== null
  }

  /** The embed host's hookup (`null` to unhook). */
  listen(listener: EmitListener | null): void {
    this.listener = listener
  }

  setData(name: string, value: unknown): void {
    this.data = Object.freeze({ ...this.data, [name]: value })
  }

  reset(): void {
    this.data = Object.freeze({})
  }

  /** A game event for the page. */
  emit(name: string, payload: unknown, origin?: ConsoleOrigin): void {
    if (this.listener) {
      try {
        this.listener(String(name), payload)
      } catch (err) {
        console.error(`an1mbyt3: an on('event') listener threw for "${name}"`, err)
      }
    } else {
      gameConsole.add('log', `ctx.emit("${name}"${payload === undefined ? '' : `, ${safeJson(payload)}`}) — reaches the page when this game is embedded`, { ...origin, kind: 'Script' })
    }
  }
}

function safeJson(value: unknown): string {
  try {
    const text = JSON.stringify(value)
    return text.length > 120 ? `${text.slice(0, 117)}…` : text
  } catch {
    return String(value)
  }
}

export const embedBridge = new EmbedBridge()
