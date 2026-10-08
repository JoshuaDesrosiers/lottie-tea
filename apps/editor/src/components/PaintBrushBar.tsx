import { useEffect, useState, useSyncExternalStore } from 'react'
import { Slider } from './Slider'
import type { Store } from 'redux'
// @ts-expect-error — vendored JS, not typed (see PaintAssetEditor).
import { brushSettings, onBrushSettings, setBrushSettings } from '../vendor/scratch-paint/index.js'

type Preset = 'round' | 'pixel' | 'soft' | 'spray'
type Settings = { preset: Preset; smoothing: number; pressure: boolean }

const PRESETS: { id: Preset; label: string; title: string }[] = [
  { id: 'round', label: 'Round', title: 'Hard round brush (tiny sizes paint square pixels)' },
  { id: 'pixel', label: 'Pixel', title: 'Hard square at any size — crisp pixel art' },
  { id: 'soft', label: 'Soft', title: 'Airbrush: soft edges that build up as you paint over' },
  { id: 'spray', label: 'Spray', title: 'Spray paint: scattered dots' },
]

/**
 * Brush options for scratch-paint's bitmap brush and eraser (presets, a
 * stabilizer, pen pressure) — shown while either is the active tool. The
 * settings live in the vendored `helper/an1-brush.js`, which the brush tool
 * reads on every stamp.
 */
export function PaintBrushBar({ store }: { store: Store }) {
  const mode = useSyncExternalStore(store.subscribe, () => (store.getState() as { scratchPaint: { mode: string } }).scratchPaint.mode)
  const [settings, setSettings] = useState<Settings>({ ...(brushSettings as Settings) })
  useEffect(() => onBrushSettings((next: Settings) => setSettings({ ...next })), [])
  if (mode !== 'BIT_BRUSH' && mode !== 'BIT_ERASER') return null

  const chip = (active: boolean) => `h-6 rounded-md px-2 text-[11px] ${active ? 'bg-accent text-white' : 'text-ink-dim hover:bg-overlay/10 hover:text-ink'}`
  return (
    <div className="pointer-events-auto flex items-center gap-2 rounded-xl border border-panel-border bg-panel px-2 py-1.5 shadow-2xl">
      <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">{mode === 'BIT_ERASER' ? 'Eraser' : 'Brush'}</span>
      <div className="flex items-center gap-0.5">
        {PRESETS.map((p) => (
          <button key={p.id} onClick={() => setBrushSettings({ preset: p.id })} className={chip(settings.preset === p.id)} title={p.title}>
            {p.label}
          </button>
        ))}
      </div>
      <span className="h-4 w-px bg-overlay/15" />
      <div className="flex items-center gap-1.5 text-[11px] text-ink-dim" title="Stabilizer: the brush trails behind the cursor for smooth, steady lines (0 = off)">
        Smoothing
        <div className="w-24">
          <Slider min={0} max={0.9} step={0.01} value={settings.smoothing} onChange={(smoothing) => setBrushSettings({ smoothing })} />
        </div>
        <span className="w-6 tabular-nums text-ink">{Math.round(settings.smoothing * 100)}</span>
      </div>
      <span className="h-4 w-px bg-overlay/15" />
      <label className="flex items-center gap-1 text-[11px] text-ink-dim" title="With a pen tablet or pressure-sensitive screen, pressing harder paints bigger">
        <input type="checkbox" checked={settings.pressure} onChange={(e) => setBrushSettings({ pressure: e.target.checked })} />
        Pen pressure
      </label>
    </div>
  )
}
