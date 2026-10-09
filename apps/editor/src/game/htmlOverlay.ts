/**
 * HTML UI — the host half of `an1mbyt3_core::UiHtml`: real HTML + CSS
 * elements laid over the canvas, each kept on its node every frame (the
 * node's world position / rotation / scale, through the same camera the
 * engine renders with).
 *
 * This is the portable path for "HTML in canvas": it works in every
 * browser and webview. Its one limit is depth — DOM elements always draw
 * on top of the canvas, so a 3D wall can't hide one. (The WICG
 * HTML-in-Canvas API, `drawElementImage`, would let the engine draw
 * elements into the scene itself; it's Chromium-only and behind a flag
 * today, so this overlay is what ships.)
 *
 * Each element lives in its own shadow root: its CSS can't restyle the
 * editor or the game page, and the page's can't restyle it.
 *
 * No-code wiring (interactive mode only):
 * - click on `[data-trigger="name"]` → fires that state-machine trigger;
 * - `<input data-input="name">` → sets that input (checkbox: bool,
 *   range / number: number, anything else: enum value).
 *
 * One module-level instance (`htmlOverlay`), like the audio engine: the
 * editor drives it in Design (non-interactive, a preview), the script host
 * in Playground and exported games.
 */
import { formatProp } from './props'
import { resolveFileRef } from './projectFiles'
import { ICON_CLASS_CSS, loadFontsUsedIn } from './googleFonts'
import type { OpaqueId, UiHtmlConfig } from '../ipc/document'

export type { UiHtmlConfig }

/** One element to show this frame. */
export interface HtmlItem {
  id: OpaqueId
  config: UiHtmlConfig
  /** The node's world transform. */
  translation: { x: number; y: number }
  rotation: number
  scale: { x: number; y: number }
  visible: boolean
  zIndex?: number
}

/** How world units map onto the overlay (CSS px). */
export interface OverlayView {
  /** World point at the overlay's centre. */
  panX: number
  panY: number
  /** CSS px per world unit. */
  pxPerWorld: number
}

export interface OverlayHandlers {
  fireTrigger(name: string): void
  setInput(name: string, value: boolean | number | string): void
  /** Scene properties: `data-bind="name"` shows one (optional
   * `data-format="Score: {}"`), `data-prop="name"` on an input writes one. */
  getProp?(name: string): unknown
  setProp?(name: string, value: unknown): void
  /** Bumped when any property changes (bound elements refresh then). */
  propsVersion?: number
}

interface Live {
  propsVersion?: number
  host: HTMLDivElement
  root: ShadowRoot
  /** What's rendered inside (re-rendered only when this changes). */
  content: string
}

const keyOf = (id: OpaqueId) => `${id.idx}:${id.version}`

class HtmlOverlay {
  private container: HTMLDivElement | null = null
  private live = new Map<string, Live>()
  private handlers: OverlayHandlers | null = null
  private interactive = false

  private canvas: HTMLCanvasElement | null = null

  /** Puts the overlay layer inside `parent` (the canvas's positioned
   * wrapper, covering exactly the canvas). */
  attach(parent: HTMLElement, canvas: HTMLCanvasElement): void {
    this.canvas = canvas
    if (this.container?.parentElement === parent) return
    this.detach()
    const container = document.createElement('div')
    container.className = 'an1-html-overlay'
    // Never blocks the canvas itself; interactive elements opt back in.
    container.style.cssText = 'position:absolute;inset:0;overflow:hidden;pointer-events:none;z-index:1;'
    parent.appendChild(container)
    this.container = container
  }

  detach(): void {
    this.clear()
    this.container?.remove()
    this.container = null
  }

  /** Removes every element (scene switch, leaving Playground). */
  clear(): void {
    for (const live of this.live.values()) live.host.remove()
    this.live.clear()
  }

  /** CSS px per physical canvas pixel (the engine's camera zoom is in
   * physical pixels per world unit). */
  cssPerPhysical(): number {
    const c = this.canvas
    return c && c.width > 0 ? c.clientWidth / c.width : 1 / (window.devicePixelRatio || 1)
  }

  /** The live DOM of a node's HTML UI (`ref.dom` in scripts). */
  root(node: OpaqueId): ShadowRoot | null {
    return this.live.get(keyOf(node))?.root ?? null
  }

  /**
   * Shows exactly `items` this frame. `interactive`: Playground / games
   * (elements take the pointer); off in Design so the canvas stays
   * editable underneath.
   */
  update(items: HtmlItem[], view: OverlayView, interactive: boolean, handlers: OverlayHandlers | null): void {
    const container = this.container
    if (!container) return
    this.handlers = handlers
    this.interactive = interactive
    const width = container.clientWidth
    const height = container.clientHeight
    const seen = new Set<string>()
    for (const item of items) {
      const key = keyOf(item.id)
      seen.add(key)
      let live = this.live.get(key)
      if (!live) {
        const host = document.createElement('div')
        host.style.cssText = 'position:absolute;left:0;top:0;transform-origin:50% 50%;'
        const root = host.attachShadow({ mode: 'open' })
        root.addEventListener('click', (e) => this.handleClick(e))
        root.addEventListener('input', (e) => this.handleInput(e))
        root.addEventListener('change', (e) => this.handleInput(e))
        container.appendChild(host)
        live = { host, root, content: '' }
        this.live.set(key, live)
      }
      const { config } = item
      // `html` / `css` may be `@file:` references to project files (the
      // Code tab) — resolved here, so editing the file re-renders live.
      const html = resolveFileRef(config.html) ?? `<div style="font:12px system-ui;color:#f87171;padding:8px">Missing file: ${config.html.slice(6)}</div>`
      const css = resolveFileRef(config.css) ?? ''
      // Content: re-rendered only when the authored HTML / CSS / size
      // change (scripts editing `ref.dom` keep their changes).
      const content = `${config.width}x${config.height}\n${css}\n${html}`
      const contentChanged = content !== live.content
      if (contentChanged) {
        live.content = content
        // Google Fonts named in a font-family, and Material Symbols if its
        // classes are used, are linked into the page (fonts are page-wide);
        // the icon classes themselves have to live in the shadow root.
        loadFontsUsedIn(html, css)
        live.root.innerHTML = `<style>:host{all:initial;display:block;width:${config.width}px;height:${config.height}px;}\n${ICON_CLASS_CSS}\n${css}</style>${html}`
      }
      // Property bindings: refreshed when a property changed (or the HTML
      // was re-rendered).
      const getProp = handlers?.getProp
      if (getProp && (contentChanged || live.propsVersion !== handlers.propsVersion)) {
        live.propsVersion = handlers.propsVersion
        for (const el of live.root.querySelectorAll<HTMLElement>('[data-bind]')) {
          const text = formatProp(getProp(el.dataset.bind ?? ''), el.dataset.format ?? '')
          if (el.textContent !== text) el.textContent = text
        }
        for (const el of live.root.querySelectorAll<HTMLInputElement>('[data-prop]')) {
          // Don't fight the user while they're editing it.
          if (live.root.activeElement === el) continue
          const v = getProp(el.dataset.prop ?? '')
          if (el.type === 'checkbox') el.checked = !!v
          else if (String(el.value) !== String(v ?? '')) el.value = String(v ?? '')
        }
      }
      const host = live.host
      host.style.display = item.visible ? 'block' : 'none'
      host.style.width = `${config.width}px`
      host.style.height = `${config.height}px`
      host.style.pointerEvents = interactive && config.interactive ? 'auto' : 'none'
      host.style.zIndex = String(Math.round(item.zIndex ?? 0))
      // World (y up) -> overlay CSS px (y down), centred on the node.
      const sx = width / 2 + (item.translation.x - view.panX) * view.pxPerWorld
      const sy = height / 2 - (item.translation.y - view.panY) * view.pxPerWorld
      const k = view.pxPerWorld
      host.style.transform = `translate(${sx - config.width / 2}px, ${sy - config.height / 2}px) rotate(${-item.rotation}rad) scale(${item.scale.x * k}, ${item.scale.y * k})`
    }
    for (const [key, live] of this.live) {
      if (!seen.has(key)) {
        live.host.remove()
        this.live.delete(key)
      }
    }
  }

  private handleClick(event: Event) {
    if (!this.interactive || !this.handlers) return
    const target = (event.target as Element | null)?.closest?.('[data-trigger]')
    const name = target?.getAttribute('data-trigger')
    if (name) this.handlers.fireTrigger(name)
  }

  private handleInput(event: Event) {
    if (!this.interactive || !this.handlers) return
    const el = event.target as HTMLInputElement | HTMLSelectElement | null
    const prop = el?.getAttribute?.('data-prop')
    if (el && prop && this.handlers.setProp) {
      this.handlers.setProp(prop, el instanceof HTMLInputElement && el.type === 'checkbox' ? el.checked : el.value)
      return
    }
    const name = el?.getAttribute?.('data-input')
    if (!el || !name) return
    if (el instanceof HTMLInputElement && el.type === 'checkbox') this.handlers.setInput(name, el.checked)
    else if (el instanceof HTMLInputElement && (el.type === 'range' || el.type === 'number')) this.handlers.setInput(name, Number(el.value))
    else this.handlers.setInput(name, el.value)
  }
}

export const htmlOverlay = new HtmlOverlay()
