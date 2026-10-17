import { Select } from './Select'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Slider } from './Slider'
import type { Store } from 'redux'
// @ts-expect-error — vendored JS, not typed (see PaintAssetEditor).
import { currentColors, setPaintColor, startEyeDropper } from '../vendor/scratch-paint/index.js'

/** What the Canvas bar controls; `PaintAssetEditor` draws it around the cel. */
export interface PaintView {
  grid: boolean
  /** Grid spacing in scratch-paint working units (2 per drawing unit). */
  gridSize: number
  /** An image to trace over, shown under everything (never saved). */
  reference: { image: HTMLImageElement; name: string } | null
  referenceOpacity: number
  referenceVisible: boolean
}

export const DEFAULT_VIEW: PaintView = { grid: false, gridSize: 32, reference: null, referenceOpacity: 0.4, referenceVisible: true }

const PALETTE_KEY = 'an1mbyt3.paint.palette'
const DEFAULT_PALETTE = ['#000000', '#ffffff', '#9966ff', '#ff6680', '#ffab19', '#ffd500', '#59c059', '#4c97ff', '#855cd6', '#8b5a2b']

function loadPalette(): string[] {
  try {
    const raw = localStorage.getItem(PALETTE_KEY)
    const parsed = raw ? (JSON.parse(raw) as unknown) : null
    if (Array.isArray(parsed) && parsed.every((c) => typeof c === 'string')) return parsed as string[]
  } catch {
    // private window / blocked storage: the defaults
  }
  return DEFAULT_PALETTE
}

function savePalette(colors: string[]) {
  try {
    localStorage.setItem(PALETTE_KEY, JSON.stringify(colors))
  } catch {
    // not persisted — still works this session
  }
}

const SHORTCUTS: [string, string][] = [
  [', / .', 'Previous / next frame'],
  ['I / Shift+I', 'Eyedropper for the fill / outline color'],
  ['Ctrl+Z / Ctrl+Shift+Z', 'Undo / redo'],
  ['Delete', 'Delete the selection'],
  ['Escape', 'Deselect'],
  ['Ctrl+C / X / V', 'Copy / cut / paste'],
  ['Ctrl+A', 'Select all'],
  ['Ctrl+G / Ctrl+Shift+G', 'Group / ungroup'],
  ['Arrow keys', 'Nudge the selection'],
  ['Scroll / Ctrl+scroll', 'Pan / zoom'],
]

/**
 * Paint's Canvas bar: grid, a reference image to trace over, a saved color
 * palette (click = fill, right-click = outline, + saves the current fill,
 * Alt-click removes), the eyedropper, and the shortcut list.
 */
export function PaintCanvasBar({ store, view, onChange, below }: { store: Store; view: PaintView; onChange: (view: PaintView) => void; /** Shown under the bar (the brush settings). */ below?: ReactNode }) {
  const [palette, setPalette] = useState<string[]>(loadPalette)
  const [showKeys, setShowKeys] = useState(false)
  const fileInput = useRef<HTMLInputElement>(null)
  useEffect(() => savePalette(palette), [palette])

  const loadReference = (file: File) => {
    const reader = new FileReader()
    reader.onload = () => {
      const image = new Image()
      image.onload = () => onChange({ ...view, reference: { image, name: file.name }, referenceVisible: true })
      image.src = String(reader.result)
    }
    reader.readAsDataURL(file)
  }

  const button = (active = false) => `h-6 rounded-md px-2 text-[11px] ${active ? 'bg-accent text-white' : 'text-ink-dim hover:bg-overlay/10 hover:text-ink'}`
  const divider = <span className="h-4 w-px bg-overlay/15" />
  return (
    <>
    {/* Top centre: view aids + tools, the brush settings under them. */}
    <div className="pointer-events-none absolute left-1/2 top-24 z-20 flex -translate-x-1/2 flex-col items-center gap-1">
      <div className="pointer-events-auto flex items-center gap-1.5 rounded-xl border border-panel-border bg-panel px-2 py-1.5 shadow-2xl">
        {/* Grid */}
        <button onClick={() => onChange({ ...view, grid: !view.grid })} className={button(view.grid)} title="Show a grid over the canvas (a guide — not part of the drawing)">
          Grid
        </button>
        {view.grid && (
          <Select
            value={view.gridSize}
            onChange={(e) => onChange({ ...view, gridSize: Number(e.target.value) })}
            className="h-6 rounded bg-overlay/10 px-1 text-[11px] text-ink outline-none"
            title="Grid spacing (drawing units)"
          >
            {[8, 16, 32, 64, 128].map((size) => (
              <option key={size} value={size}>
                {size / 2}
              </option>
            ))}
          </Select>
        )}
        {divider}
        {/* Reference image */}
        <input
          ref={fileInput}
          type="file"
          accept="image/*"
          className="hidden"
          onChange={(e) => {
            const file = e.target.files?.[0]
            if (file) loadReference(file)
            e.target.value = ''
          }}
        />
        {view.reference ? (
          <>
            <button onClick={() => onChange({ ...view, referenceVisible: !view.referenceVisible })} className={button(view.referenceVisible)} title={`Reference: ${view.reference.name} — show / hide`}>
              Reference
            </button>
            <div className="w-16" title={`Reference opacity ${Math.round(view.referenceOpacity * 100)}%`}>
              <Slider min={0.05} max={1} step={0.01} value={view.referenceOpacity} onChange={(referenceOpacity) => onChange({ ...view, referenceOpacity })} />
            </div>
            <button onClick={() => onChange({ ...view, reference: null })} className={button()} title="Remove the reference image">
              ✕
            </button>
          </>
        ) : (
          <button onClick={() => fileInput.current?.click()} className={button()} title="Load an image to trace over — shown faintly under the drawing, never saved with it">
            Reference…
          </button>
        )}
        {divider}
        {/* Eyedropper */}
        <button onClick={() => startEyeDropper(store, false)} className={button()} title="Eyedropper: click the canvas to pick a fill color (I; Shift+I for the outline)">
          Pick color
        </button>
        <button onClick={() => setShowKeys((v) => !v)} className={button(showKeys)} title="Keyboard shortcuts">
          ?
        </button>
      </div>

      {below}
      {showKeys && (
        <div className="pointer-events-auto w-80 rounded-xl border border-panel-border bg-panel p-3 shadow-2xl">
          <div className="mb-1.5 text-[11px] font-medium uppercase tracking-wide text-ink-dim">Paint shortcuts</div>
          <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[11px]">
            {SHORTCUTS.map(([keys, what]) => (
              <div key={keys} className="contents">
                <dt className="whitespace-nowrap font-mono text-ink">{keys}</dt>
                <dd className="text-ink-dim">{what}</dd>
              </div>
            ))}
          </dl>
        </div>
      )}
    </div>

      {/* Palette: a vertical strip, centred on the right edge. */}
      <div className="pointer-events-auto absolute right-4 top-1/2 z-20 flex max-h-[70%] -translate-y-1/2 flex-col items-center gap-1 overflow-y-auto rounded-xl border border-panel-border bg-panel px-1.5 py-2 shadow-2xl">
        {palette.map((color, i) => (
          <button
            key={`${color}-${i}`}
            onClick={(e) => {
              if (e.altKey) setPalette((p) => p.filter((_, j) => j !== i))
              else setPaintColor(store, color, false)
            }}
            onContextMenu={(e) => {
              e.preventDefault()
              setPaintColor(store, color, true)
            }}
            className="h-5 w-5 rounded border border-overlay/25 hover:scale-110"
            style={{ background: color }}
            title={`${color} — click: fill · right-click: outline · Alt+click: remove`}
          />
        ))}
        <button
          onClick={() => {
            const { fill } = currentColors(store) as { fill: string | null }
            if (fill && !palette.includes(fill)) setPalette((p) => [...p, fill])
          }}
          className="flex h-5 w-5 items-center justify-center rounded border border-dashed border-overlay/40 text-[11px] text-ink-dim hover:text-ink"
          title="Save the current fill color to the palette"
        >
          +
        </button>
      </div>
    </>
  )
}
