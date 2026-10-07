/**
 * `<animation-player>` — the markup-only embedding AN1MBYT3_BUILD_PLAN.md
 * §8 describes:
 * ```html
 * <animation-player src="/character.teab"></animation-player>
 * ```
 * A thin adapter around `An1mbyt3Animation` (see that class for the real
 * logic) onto the custom-element lifecycle — this file owns none of the
 * actual engine/rendering behavior, only wiring `connectedCallback`/
 * `attributeChangedCallback`/`disconnectedCallback` to it.
 */
import { An1mbyt3Animation } from './animation.js'

export class AnimationPlayerElement extends HTMLElement {
  private readonly canvas: HTMLCanvasElement
  private animationInstance: An1mbyt3Animation | null = null
  /** The in-flight `connect()` call, if any — `attributeChangedCallback`
   * can fire (a `src` set right after the element is created) before
   * `connect()`'s `An1mbyt3Animation.create` has resolved; awaiting this
   * first is what makes a same-tick `element.src = "..."` reliable instead
   * of racing the element's own setup. */
  private connecting: Promise<void> | null = null

  static get observedAttributes(): string[] {
    return ['src']
  }

  /** The underlying `An1mbyt3Animation` this element wraps, once
   * `connectedCallback` has finished creating it — `null` before that (or
   * after `disconnectedCallback`). The escape hatch for anything this
   * element's own attributes don't cover (`play`/`pause`/`seek`/
   * `fireTrigger`/etc., same reasoning `@an1mbyt3/react`'s `<Animation
   * onReady={...}>` prop exists). */
  get animation(): An1mbyt3Animation | null {
    return this.animationInstance
  }

  constructor() {
    super()
    this.canvas = document.createElement('canvas')
    // Fills whatever box the host page gives this element — the "component
    // can live anywhere on any page at any size" positioning
    // AN1MBYT3_BUILD_PLAN.md §16 calls out for the transparent-canvas work.
    this.canvas.style.width = '100%'
    this.canvas.style.height = '100%'
    this.canvas.style.display = 'block'
  }

  connectedCallback(): void {
    this.appendChild(this.canvas)
    this.connecting = this.connect()
  }

  private async connect(): Promise<void> {
    const rect = this.getBoundingClientRect()
    this.canvas.width = Math.max(1, Math.round(rect.width || this.canvas.clientWidth || 300))
    this.canvas.height = Math.max(1, Math.round(rect.height || this.canvas.clientHeight || 150))

    this.animationInstance = await An1mbyt3Animation.create(this.canvas, { trackPointer: this.hasAttribute('track-pointer') })

    const src = this.getAttribute('src')
    if (src) await this.animationInstance.loadTeab(src)
  }

  attributeChangedCallback(name: string, oldValue: string | null, newValue: string | null): void {
    if (name === 'src' && newValue && newValue !== oldValue) {
      void this.loadSrc(newValue)
    }
  }

  private async loadSrc(src: string): Promise<void> {
    if (this.connecting) await this.connecting
    await this.animationInstance?.loadTeab(src)
  }

  disconnectedCallback(): void {
    this.animationInstance?.destroy()
    this.animationInstance = null
  }
}

/** Registers `AnimationPlayerElement` under `tagName` (`<animation-player>`
 * by default) — a function, not a module-level side effect, so importing
 * this package doesn't silently register a global custom element a
 * consumer didn't ask for (and so a page embedding multiple `@an1mbyt3`
 * things can't hit a "already defined" error from importing this module
 * twice under different bundling setups). Safe to call more than once —
 * a no-op if `tagName` is already registered. */
export function registerAnimationPlayerElement(tagName = 'animation-player'): void {
  if (!customElements.get(tagName)) {
    customElements.define(tagName, AnimationPlayerElement)
  }
}
