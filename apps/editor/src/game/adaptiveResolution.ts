/**
 * Adaptive resolution — keeps a game's frame rate up at any window size.
 *
 * Each frame, `update` looks at how long the frame really took (the
 * `requestAnimationFrame` delta). When frames run long for a moment, it
 * lowers the canvas's backing resolution: the canvas keeps its size on the
 * page, but its pixel buffer is `devicePixelRatio × scale` per CSS pixel
 * ([`renderDpr`]) and the browser stretches it smoothly. Everything the GPU
 * does shrinks with it — the scene, every post effect, the swapchain and
 * the browser's compositing of the canvas. When frames are comfortably
 * fast again, it climbs back towards native, a small step at a time.
 *
 * - **Floor 40 fps.** Frames over ~22 ms (under ~45 fps) for a quarter of
 *   a second step it down. Over 30 ms (under ~33 fps) it drops faster.
 * - **No pumping.** It only climbs after 2.5 s of steady fast frames. If
 *   a climb immediately costs frames, that level becomes a ceiling for
 *   15 s, so it doesn't oscillate between two sizes.
 * - **Refresh-rate aware.** "Fast" means close to the display's own
 *   interval (tracked from the best recent frames), so a 144 Hz screen
 *   doesn't read as "slow" at 60 fps or vice versa.
 *
 * The game's project setting turns it off or sets the floor
 * (`WasmEditor.adaptive_resolution`). Off — or outside play — it pins
 * native resolution.
 */
import type { WasmEditor } from '../wasm-pkg/an1mbyt3_wasm'

/** Frame times above this (ms) count as slow. */
const SLOW_MS = 22
/** …and above this as very slow (bigger steps down). */
const VERY_SLOW_MS = 30
const STEP_DOWN = 0.1
const STEP_DOWN_FAST = 0.2
const STEP_UP = 0.05

export class AdaptiveResolution {
  /** The scale the renderer is at (what we last asked for). */
  scale = 1
  private ema = 16.7
  /** The display's frame interval, from the fastest recent frames. */
  private vsyncMs = 16.7
  private slowFor = 0
  private fastFor = 0
  private cooldown = 0
  /** A level that cost frames when we climbed to it, and for how long it's off-limits. */
  private ceiling = 1
  private ceilingFor = 0
  /** Seconds since the last climb (watching whether it hurt). */
  private sinceClimb = Infinity
  private settingsAt = -Infinity
  private enabled = true
  private minScale = 0.5

  /** Call once per frame. `active`: a game is playing (Playground / player). */
  update(editor: WasmEditor, deltaMs: number, now: number, active: boolean): void {
    // The project setting, re-read twice a second (it's cheap, and the
    // Inspector can change it mid-play).
    if (now - this.settingsAt > 500) {
      this.settingsAt = now
      try {
        const s = editor.adaptive_resolution() as { enabled: boolean; minScale: number }
        this.enabled = s.enabled
        this.minScale = s.minScale
      } catch {
        // The editor is busy (an async load): keep the last settings.
      }
    }
    if (!active || !this.enabled) {
      this.apply(editor, 1)
      this.slowFor = this.fastFor = 0
      return
    }
    // Skip hitches that aren't rendering (tab switch, a GC spike, a scene load).
    if (!(deltaMs > 0) || deltaMs > 250) return
    const dt = deltaMs / 1000
    this.ema += (deltaMs - this.ema) * 0.15
    // The display interval: drifts down to the fastest frames, up slowly.
    this.vsyncMs = deltaMs < this.vsyncMs ? this.vsyncMs * 0.9 + deltaMs * 0.1 : this.vsyncMs * 0.999 + deltaMs * 0.001
    this.cooldown = Math.max(0, this.cooldown - dt)
    this.ceilingFor = Math.max(0, this.ceilingFor - dt)
    if (this.ceilingFor <= 0) this.ceiling = 1
    this.sinceClimb += dt

    const slow = this.ema > SLOW_MS
    const fast = this.ema < Math.max(this.vsyncMs * 1.15, 8)
    this.slowFor = slow ? this.slowFor + dt : 0
    this.fastFor = fast ? this.fastFor + dt : 0

    if (this.slowFor > 0.25 && this.cooldown <= 0 && this.scale > this.minScale) {
      // A climb that immediately hurt: don't try that level again for a while.
      if (this.sinceClimb < 1.5) {
        this.ceiling = this.scale - STEP_UP
        this.ceilingFor = 15
      }
      const step = this.ema > VERY_SLOW_MS ? STEP_DOWN_FAST : STEP_DOWN
      this.apply(editor, Math.max(this.minScale, this.scale - step))
      this.slowFor = 0
      this.cooldown = 0.4
    } else if (this.fastFor > 2.5 && this.cooldown <= 0 && this.scale < Math.min(1, this.ceiling)) {
      this.apply(editor, Math.min(1, this.ceiling, this.scale + STEP_UP))
      this.fastFor = 0
      this.cooldown = 0.5
      this.sinceClimb = 0
    }
  }

  /** Called when the scale changes: the host re-sizes the canvas buffer
   * (using [`renderDpr`]). */
  onChange: (() => void) | null = null

  private apply(_editor: WasmEditor, scale: number) {
    const snapped = Math.round(scale * 20) / 20
    if (Math.abs(snapped - this.scale) < 1e-6) return
    this.scale = snapped
    this.onChange?.()
  }
}

/** One controller for the editor's Playground (and one per exported player). */
export const adaptiveResolution = new AdaptiveResolution()

/**
 * Device pixels per CSS pixel the canvas renders at: the display's
 * `devicePixelRatio`, times the adaptive scale. Use it everywhere the
 * canvas buffer size or the render call's pixel density is computed, so
 * the world-to-screen mapping stays the same at any scale.
 */
export function renderDpr(): number {
  return (window.devicePixelRatio || 1) * adaptiveResolution.scale
}
