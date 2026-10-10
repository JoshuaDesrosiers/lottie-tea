/**
 * Frame profiler overlay (F3) — shared by the editor's Playground and the
 * exported player. Combines the engine's own per-stage timings
 * (`WasmEditor::frame_stats`, Rust side) with the JS stages around it
 * (reading the document, running scripts) into one smoothed readout.
 *
 * Drawn as a plain DOM element updated a few times a second — not React —
 * so the overlay itself costs nothing measurable.
 */
import type { WasmEditor } from '../wasm-pkg/an1mbyt3_wasm'
import { adaptiveResolution } from './adaptiveResolution'

/** The engine's half (mirrors `profiler::FrameStats`). */
export interface EngineFrameStats {
  logicMs: number
  ikMs: number
  physicsMs: number
  prepMs: number
  sceneMs: number
  gpuSubmitMs: number
  totalMs: number
  nodes: number
  draws: number
  lights: number
  bodies: number
  particles: number
}

const STAGES = ['logic', 'ik', 'physics', 'prep', 'scene', 'gpu', 'sync', 'scripts'] as const
type Stage = (typeof STAGES)[number]

const LABELS: Record<Stage, string> = {
  logic: 'anim/logic',
  ik: 'ik',
  physics: 'physics',
  prep: 'prep',
  scene: 'scene',
  gpu: 'gpu submit',
  sync: 'doc sync',
  scripts: 'scripts',
}

export class PerfMeter {
  enabled = false
  private el: HTMLDivElement | null = null
  private ema: Record<Stage, number> = Object.fromEntries(STAGES.map((s) => [s, 0])) as Record<Stage, number>
  private frameMs = 16.7
  private worstMs = 0
  private counts: Pick<EngineFrameStats, 'nodes' | 'draws' | 'lights' | 'bodies' | 'particles'> = { nodes: 0, draws: 0, lights: 0, bodies: 0, particles: 0 }
  private lastPaint = 0
  private lastWorstReset = 0
  private jsMarks: Partial<Record<Stage, number>> = {}

  /** Adds `ms` to a JS-side stage for the current frame. */
  add(stage: 'sync' | 'scripts', ms: number): void {
    if (this.enabled) this.jsMarks[stage] = (this.jsMarks[stage] ?? 0) + ms
  }

  toggle(parent: HTMLElement = document.body): void {
    this.enabled = !this.enabled
    if (this.enabled && !this.el) {
      const el = document.createElement('div')
      el.style.cssText =
        'position:fixed;top:8px;right:8px;z-index:9999;pointer-events:none;padding:6px 8px;border-radius:6px;' +
        'background:rgba(10,8,14,.82);color:#e8e2f0;font:11px/1.35 ui-monospace,Consolas,monospace;white-space:pre;min-width:176px'
      parent.appendChild(el)
      this.el = el
    }
    if (this.el) this.el.style.display = this.enabled ? 'block' : 'none'
  }

  /** Call once per frame after everything ran. `deltaMs`: real frame time. */
  endFrame(editor: WasmEditor, deltaMs: number, now: number): void {
    if (!this.enabled) return
    const engine = editor.frame_stats() as EngineFrameStats
    const sample: Record<Stage, number> = {
      logic: engine.logicMs,
      ik: engine.ikMs,
      physics: engine.physicsMs,
      prep: engine.prepMs,
      scene: engine.sceneMs,
      gpu: engine.gpuSubmitMs,
      sync: this.jsMarks.sync ?? 0,
      scripts: this.jsMarks.scripts ?? 0,
    }
    this.jsMarks = {}
    const k = 0.1
    for (const s of STAGES) this.ema[s] += (sample[s] - this.ema[s]) * k
    if (deltaMs > 0 && deltaMs < 1000) {
      this.frameMs += (deltaMs - this.frameMs) * k
      if (now - this.lastWorstReset > 2000) {
        this.worstMs = 0
        this.lastWorstReset = now
      }
      this.worstMs = Math.max(this.worstMs, deltaMs)
    }
    this.counts = engine
    if (now - this.lastPaint > 250 && this.el) {
      this.lastPaint = now
      const cpu = STAGES.reduce((sum, s) => sum + this.ema[s], 0)
      const fps = 1000 / Math.max(1, this.frameMs)
      const bar = (ms: number) => '█'.repeat(Math.min(20, Math.round((ms / 16.7) * 20)))
      const lines = [
        `${fps.toFixed(0).padStart(3)} fps   frame ${this.frameMs.toFixed(1)}ms`,
        `worst ${this.worstMs.toFixed(1)}ms   cpu ${cpu.toFixed(1)}ms`,
        '',
        ...STAGES.map((s) => `${LABELS[s].padEnd(10)} ${this.ema[s].toFixed(2).padStart(6)} ${bar(this.ema[s])}`),
        '',
        `resolution ${Math.round(adaptiveResolution.scale * 100)}%`,
        `nodes ${this.counts.nodes}  draws ${this.counts.draws}`,
        `lights ${this.counts.lights}  bodies ${this.counts.bodies}`,
        `particles ${this.counts.particles}`,
      ]
      this.el.textContent = lines.join('\n')
    }
  }
}
