/**
 * **Toasty Lite** for any web page: `createLite()` and the `<toasty-lite>`
 * element. Built to `public/embed/an1mbyt3-lite.js` (no dependencies).
 *
 * ```html
 * <script type="module" src="/toasty/an1mbyt3-lite.js"></script>
 * <toasty-lite src="/toasty/heart.json" style="width:48px;height:48px"></toasty-lite>
 * ```
 *
 * Attributes: `src` (required), `fit` (`contain` | `cover`), `background`
 * (paint the scene colour), `paused`, `data` (JSON for scene Properties),
 * `inputs` (JSON for state machine inputs), `ink` (a CSS colour, or
 * `currentColor` to follow the element's text colour — line icons then
 * match the site's theme, re-checked when `<html>`'s class / data-theme
 * changes). Changing `data` / `inputs` /
 * `paused` updates the running scene; changing `src` reloads it.
 * The element's `.player` is the full API once ready; it fires
 * `toasty:ready`, `toasty:event`, `toasty:state` and `toasty:prop`
 * (all bubble).
 */
import { createLite, loadLite, type LitePlayer } from './runtime'

export { createLite, loadLite }
export type { LiteEvents, LiteFile, LiteOptions, LitePlayer } from './runtime'

export class ToastyLiteElement extends HTMLElement {
  static observedAttributes = ['src', 'fit', 'background', 'paused', 'data', 'inputs', 'ink']
  player: LitePlayer | null = null
  private box: HTMLDivElement | null = null
  private loading = 0

  connectedCallback() {
    if (!this.style.display) this.style.display = 'inline-block'
    void this.load()
  }

  disconnectedCallback() {
    this.stopInk?.()
    this.stopInk = null
    this.player?.destroy()
    this.player = null
    this.box?.remove()
    this.box = null
  }

  attributeChangedCallback(name: string, before: string | null, after: string | null) {
    if (before === after || !this.isConnected) return
    if (name === 'src' || name === 'fit' || name === 'background') void this.load()
    else if (this.player) {
      if (name === 'paused') {
        if (this.hasAttribute('paused')) this.player.pause()
        else this.player.play()
      } else if (name === 'data') this.player.setData(this.json('data') ?? {})
      else if (name === 'inputs') for (const [k, v] of Object.entries(this.json('inputs') ?? {})) this.player.setInput(k, v as boolean | number | string)
      else if (name === 'ink') this.applyInk()
    }
  }

  private stopInk: (() => void) | null = null

  /** Sets the scene's `ink` Property from the `ink` attribute (and keeps
   * following the page's theme when it is `currentColor`). */
  private applyInk() {
    const ink = this.getAttribute('ink')
    if (!ink || !this.player) return
    const apply = () => {
      const probe = document.createElement('span')
      probe.style.color = ink
      this.appendChild(probe)
      const color = getComputedStyle(probe).color
      probe.remove()
      const m = color.match(/rgba?\(([^)]+)\)/)
      if (!m) return
      const [r, g, b, a] = m[1].split(/[ ,/]+/).filter(Boolean).map(Number)
      this.player?.set('ink', [r / 255, g / 255, b / 255, Number.isFinite(a) ? a : 1])
    }
    apply()
    if (!this.stopInk && ink === 'currentColor') {
      const mo = new MutationObserver(apply)
      mo.observe(document.documentElement, { attributes: true, attributeFilter: ['class', 'data-theme', 'style'] })
      const mq = matchMedia('(prefers-color-scheme: dark)')
      mq.addEventListener('change', apply)
      this.stopInk = () => {
        mo.disconnect()
        mq.removeEventListener('change', apply)
      }
    }
  }

  private json(name: string): Record<string, unknown> | null {
    const text = this.getAttribute(name)
    if (!text) return null
    try {
      return JSON.parse(text) as Record<string, unknown>
    } catch {
      console.warn(`<toasty-lite>: "${name}" must be JSON`)
      return null
    }
  }

  private async load() {
    const src = this.getAttribute('src')
    if (!src) return
    const ticket = ++this.loading
    this.player?.destroy()
    this.player = null
    this.box?.remove()
    const box = document.createElement('div')
    box.style.cssText = 'width:100%;height:100%'
    this.box = box
    this.appendChild(box)
    try {
      const player = await createLite(box, {
        src,
        fit: (this.getAttribute('fit') as 'contain' | 'cover' | null) ?? undefined,
        background: this.hasAttribute('background'),
        paused: this.hasAttribute('paused'),
        data: this.json('data') ?? undefined,
        inputs: (this.json('inputs') as Record<string, boolean | number | string> | null) ?? undefined,
      })
      if (ticket !== this.loading) return player.destroy()
      this.player = player
      this.applyInk()
      this.dispatchEvent(new CustomEvent('toasty:ready', { detail: { player }, bubbles: true }))
    } catch (error) {
      console.error(error)
      this.dispatchEvent(new CustomEvent('toasty:error', { detail: { error }, bubbles: true }))
    }
  }
}

if (typeof customElements !== 'undefined' && !customElements.get('toasty-lite')) customElements.define('toasty-lite', ToastyLiteElement)
