/**
 * **an1mbyt3 embed** — run an exported an1mbyt3 game inside any web page
 * and drive it from the page. Built to `public/embed/` by
 * `npm run build:embed`; "Export → Website embed" ships it with the game.
 *
 * ```js
 * import { createPlayer } from './an1mbyt3/an1mbyt3-embed.js'
 * const player = await createPlayer(document.querySelector('#hero'), { game: '/an1mbyt3/game.teab' })
 * player.set('userName', 'Ada')            // → ctx.data.userName + scene Property "userName"
 * player.fire('celebrate')                 // state machine trigger
 * player.on('event', (e) => console.log(e.name, e.payload))   // ctx.emit(...)
 * player.on('state', (e) => e.state === 'Done' && showToast())
 * ```
 *
 * Or as a tag: `<an1-player game="/an1mbyt3/game.teab"></an1-player>` (call
 * `defineAn1PlayerElement()` once), or the React component in
 * `an1mbyt3-react.js`.
 *
 * **Several players per page** work: every `createPlayer` / `<an1-player>`
 * / `<An1Player>` is independent (own canvas, game, data, events), the
 * engine `.wasm` is downloaded once and shared, and players scrolled out
 * of view pause. See `player/engine.ts`'s module doc for the limits.
 *
 * **Shared runtime**: loaded from a runtime folder
 * (`https://toastycreate.web.app/runtime/<version>/an1mbyt3-embed.js`, or
 * a self-hosted copy of the downloadable `toasty-runtime-<version>.zip`),
 * pass `runtimeUrl` (or call `setRuntimeUrl` once) so the engine `.wasm`
 * comes from that folder too; without it the player looks next to the
 * game, then next to this script.
 */
import { createPlayer, setRuntimeUrl, type An1Player, type PlayerOptions } from '../player/engine'
import { RUNTIME_VERSION } from './runtimeVersion'

export { createPlayer, setRuntimeUrl, RUNTIME_VERSION }
export type { An1Player, PlayerEvents, PlayerEventType, PlayerOptions, Source, StateChange } from '../player/engine'

/**
 * `<an1-player>`: attributes `game` (required), `wasm`, `runtime` (a
 * shared runtime folder URL — the engine `.wasm` is loaded from there),
 * `scene`, `fit`, `keyboard`, `paused`, `run-offscreen` (keep playing
 * while scrolled out of view), `data` (JSON). Changing `scene` / `data` /
 * `paused` updates the running game; changing `game` / `wasm` / `runtime`
 * reloads it. Any number of elements can be on one page.
 * Events: `an1:ready`, `an1:state`, `an1:scene`, `an1:prop`, `an1:event`,
 * `an1:error` (bubble from the inner box). The element's `.player` is the
 * full API once ready.
 */
export function defineAn1PlayerElement(tag = 'an1-player'): void {
  if (customElements.get(tag)) return
  class An1PlayerElement extends HTMLElement {
    static observedAttributes = ['game', 'wasm', 'runtime', 'scene', 'data', 'paused', 'fit', 'keyboard', 'run-offscreen']
    player: An1Player | null = null
    private box: HTMLDivElement | null = null
    private loading = 0

    connectedCallback() {
      if (!this.style.display) this.style.display = 'block'
      void this.load()
    }

    disconnectedCallback() {
      this.player?.destroy()
      this.player = null
      this.box?.remove()
      this.box = null
    }

    attributeChangedCallback(name: string, before: string | null, after: string | null) {
      if (before === after || !this.isConnected) return
      if (name === 'game' || name === 'wasm' || name === 'runtime' || name === 'fit' || name === 'keyboard' || name === 'run-offscreen') void this.load()
      else if (this.player) this.apply(name)
    }

    private apply(name: string) {
      const player = this.player
      if (!player) return
      if (name === 'scene') {
        const scene = this.getAttribute('scene')
        if (scene && scene !== player.scene) player.loadScene(scene)
      } else if (name === 'paused') {
        if (this.hasAttribute('paused')) player.pause()
        else player.play()
      } else if (name === 'data') {
        const data = this.readData()
        if (data) player.setData(data)
      }
    }

    private readData(): Record<string, unknown> | null {
      const text = this.getAttribute('data')
      if (!text) return null
      try {
        return JSON.parse(text) as Record<string, unknown>
      } catch {
        console.warn('<an1-player>: `data` must be JSON')
        return null
      }
    }

    private async load() {
      const game = this.getAttribute('game')
      if (!game) return
      const token = ++this.loading
      this.player?.destroy()
      this.player = null
      if (!this.box) {
        this.box = document.createElement('div')
        this.box.style.cssText = 'position:relative;width:100%;height:100%;'
        this.appendChild(this.box)
      }
      const options: PlayerOptions = {
        game,
        wasm: this.getAttribute('wasm') ?? undefined,
        runtimeUrl: this.getAttribute('runtime') ?? undefined,
        pauseWhenOffscreen: !this.hasAttribute('run-offscreen'),
        scene: this.getAttribute('scene') ?? undefined,
        fit: (this.getAttribute('fit') as PlayerOptions['fit']) ?? undefined,
        keyboard: (this.getAttribute('keyboard') as PlayerOptions['keyboard']) ?? undefined,
        paused: this.hasAttribute('paused'),
        data: this.readData() ?? undefined,
      }
      try {
        const player = await createPlayer(this.box, options)
        if (token !== this.loading) {
          player.destroy()
          return
        }
        this.player = player
        this.dispatchEvent(new CustomEvent('an1:ready', { detail: { scenes: player.scenes, scene: player.scene }, bubbles: true }))
      } catch (error) {
        this.dispatchEvent(new CustomEvent('an1:error', { detail: { error }, bubbles: true }))
        console.error('<an1-player> failed to start', error)
      }
    }
  }
  customElements.define(tag, An1PlayerElement)
}
