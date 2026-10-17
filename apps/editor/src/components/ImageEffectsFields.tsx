/**
 * Inspector UI for the **Image Effects** component (`an1mbyt3_core::ImageEffects`):
 * a node's stack of pixel effects — Drop Shadow, Stroke, Pattern, Halftone,
 * ASCII, Grain, Glow, Gradient Map, Blur (plain / progressive / depth),
 * Depth Map, Ribbed Glass, Scanner Drag and Code.
 *
 * - An **Add** menu grouped like an image editor's effects menu.
 * - Each effect is a card: show / hide, reorder, remove, collapse.
 * - Fields come from a small schema (`FX`): sliders, colours, toggles,
 *   segmented modes; plus pads for positions — Ribbed Glass's centre, the
 *   progressive blur's start → end line, and Scanner Drag's path (click to
 *   add a point, drag to move, double-click to remove).
 * - Gradient Map edits its stops; Code edits its source (Apply re-reads the
 *   `@param` / `@color` lines, keeping values of params that kept their
 *   name) and shows compile errors.
 *
 * Every value is a plain field of the component, so Record / "+ Property"
 * keyframes it like anything else in the Inspector.
 */
import { useEffect, useRef, useState, type ReactNode } from 'react'
import type { DocumentSummary, ImageAssetSummary, ImageEffect, ImageEffectsConfig, NodeSummary, OpaqueId } from '../ipc/document'
import { addCutoutLayer, codeEffectError, codeEffectParams, getImageSource, imageEffectDefault, importImageAsset, type WasmEditor } from '../ipc/wasmEditor'
import { cutOutSubject, estimateDepth } from '../ai/vision'
import { showAlert } from './Dialogs'
import { ColorSwatch } from './ColorPicker'
import { ImageAssetPicker } from './ImageAssetPicker'
import { Popover, insideOrPopover } from './Select'
import { Slider } from './Slider'

type Rgba = [number, number, number, number]
type Field =
  | [string, string, 'slider', number, number, number, string?]
  | [string, string, 'color']
  | [string, string, 'toggle']
  | [string, string, 'seg', [string, string][]]
  | [string, string, 'xy', number]

interface FxSchema {
  label: string
  /** Stroke icon path (24 box). */
  icon: string
  /** A segmented control shown first (the effect's mode), if any. */
  mode?: [string, [string, string][]]
  fields: Field[]
  help?: string
}

/** The effects, in Add-menu order; `null` = a separator. */
const MENU: (string | null)[] = ['DropShadow', 'Stroke', null, 'Pattern', 'Halftone', 'Ascii', null, 'Grain', null, 'Glow', null, 'GradientMap', null, 'Blur', 'DepthMap', null, 'RibbedGlass', 'ScannerDrag', null, 'Code']

const FX: Record<string, FxSchema> = {
  DropShadow: {
    label: 'Drop Shadow',
    icon: 'M8 4h11v11H8zM5 8v12h12',
    fields: [['color', 'Colour', 'color'], ['offset', 'Offset', 'xy', 1], ['blur', 'Blur', 'slider', 0.5, 0, 80, 'u'], ['spread', 'Spread', 'slider', 0.5, 0, 40, 'u']],
  },
  Stroke: {
    label: 'Stroke',
    icon: 'M5 5h14v14H5z',
    mode: ['position', [['Outside', 'Outside'], ['Center', 'Centre'], ['Inside', 'Inside']]],
    fields: [['color', 'Colour', 'color'], ['width', 'Width', 'slider', 0.5, 0, 40, 'u']],
  },
  Pattern: {
    label: 'Pattern',
    icon: 'M4 4h8v8H4zM12 12h8v8h-8z',
    fields: [
      ['amount', 'Amount', 'slider', 0.01, 0, 1, '%'],
      ['form', 'Form', 'seg', [['Checker', 'Checker'], ['Dots', 'Dots'], ['Lines', 'Lines'], ['Grid', 'Grid'], ['Waves', 'Waves']]],
      ['scale', 'Scale', 'slider', 1, 2, 300, 'u'],
      ['radius', 'Radius', 'slider', 0.01, 0, 1, '%'],
      ['smooth', 'Smooth', 'slider', 0.01, 0, 1, '%'],
      ['angle', 'Angle', 'slider', 1, -180, 180, '°'],
      ['power', 'Power', 'slider', 0.01, 0, 1, '%'],
      ['color', 'Colour', 'color'],
      ['background', 'Background', 'color'],
    ],
  },
  Halftone: {
    label: 'Halftone',
    icon: 'M6 6h.01M12 6h.01M18 6h.01M6 12h.01M12 12h.01M18 12h.01M6 18h.01M12 18h.01M18 18h.01',
    mode: ['shape', [['Dots', 'Dots'], ['Lines', 'Lines'], ['Squares', 'Squares']]],
    fields: [['amount', 'Amount', 'slider', 0.01, 0, 1, '%'], ['size', 'Size', 'slider', 0.5, 2, 80, 'u'], ['angle', 'Angle', 'slider', 1, -180, 180, '°'], ['originalColors', 'Original colours', 'toggle'], ['ink', 'Ink', 'color'], ['paper', 'Paper', 'color']],
  },
  Ascii: {
    label: 'ASCII',
    icon: 'M7 4v16M17 4v16M4 8h16M4 16h16',
    mode: ['mono', [['false', 'Original'], ['true', 'Mono']]],
    fields: [
      ['amount', 'Amount', 'slider', 0.01, 0, 1, '%'],
      ['size', 'Size', 'slider', 0.5, 4, 80, 'u'],
      ['density', 'Density', 'slider', 0.01, 0, 1, '%'],
      ['contrast', 'Contrast', 'slider', 0.01, 0, 1, '%'],
      ['charset', 'Characters', 'seg', [['Standard', 'Standard'], ['Simple', 'Simple'], ['Binary', '01'], ['Blocks', 'Blocks']]],
      ['ink', 'Ink', 'color'],
      ['background', 'Background', 'color'],
    ],
  },
  Grain: {
    label: 'Grain',
    icon: 'M4 8V4h4M16 4h4v4M20 16v4h-4M8 20H4v-4M10 10h.01M14 13h.01M11 15h.01',
    fields: [['amount', 'Amount', 'slider', 0.01, 0, 1, '%'], ['size', 'Size', 'slider', 0.1, 0.5, 10, 'u'], ['animated', 'Animated', 'toggle'], ['mono', 'Mono', 'toggle']],
  },
  Glow: {
    label: 'Glow',
    icon: 'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5 19 19M5 19l1.5-1.5M17.5 6.5 19 5',
    fields: [['intensity', 'Intensity', 'slider', 0.05, 0, 5, 'x'], ['radius', 'Radius', 'slider', 0.5, 0, 150, 'u'], ['threshold', 'Threshold', 'slider', 0.01, 0, 1, '%'], ['tint', 'Tint', 'color']],
  },
  GradientMap: {
    label: 'Gradient Map',
    icon: 'M4 12a8 8 0 1 0 16 0 8 8 0 0 0-16 0zM12 4v16',
    fields: [['amount', 'Amount', 'slider', 0.01, 0, 1, '%'], ['smooth', 'Smooth', 'slider', 0.01, 0, 1, '%'], ['offset', 'Offset', 'slider', 0.01, -1, 1, '%']],
  },
  Blur: {
    label: 'Blur',
    icon: 'M12 3s6 7 6 11a6 6 0 0 1-12 0c0-4 6-11 6-11z',
    mode: ['mode', [['Plain', 'Plain'], ['Progressive', 'Progressive'], ['Depth', 'Depth']]],
    fields: [['radius', 'Radius', 'slider', 0.5, 0, 120, 'u']],
  },
  DepthMap: {
    label: 'Depth Map',
    icon: 'M3 20l6-10 4 6 3-4 5 8z',
    fields: [['amount', 'Amount', 'slider', 0.01, 0, 1, '%'], ['contrast', 'Contrast', 'slider', 0.01, 0, 1, '%'], ['invert', 'Invert', 'toggle']],
  },
  RibbedGlass: {
    label: 'Ribbed Glass',
    icon: 'M4 4h16v16H4zM9 4v16M14 4v16',
    mode: ['shape', [['Circles', 'Circles'], ['Lines', 'Lines'], ['Squares', 'Squares']]],
    fields: [
      ['distortion', 'Distortion', 'slider', 0.01, 0, 1, '%'],
      ['size', 'Size', 'slider', 1, 4, 600, 'u'],
      ['curve', 'Curve', 'slider', 0.01, 0, 1, '%'],
      ['echo', 'Echo', 'slider', 0.01, 0, 1, '%'],
      ['widening', 'Widening', 'slider', 0.01, -1, 1, '%'],
      ['angle', 'Angle', 'slider', 1, -180, 180, '°'],
      ['dispersion', 'Dispersion', 'slider', 0.01, 0, 1, '%'],
      ['wave', 'Wave', 'slider', 0.01, 0, 1, '%'],
      ['wavelength', 'Wavelength', 'slider', 1, 10, 1200, 'u'],
    ],
    help: 'Drag the dot to move the centre (rings, widening).',
  },
  ScannerDrag: {
    label: 'Scanner Drag',
    icon: 'M4 7V4h16v3M4 17v3h16v-3M3 12h18',
    fields: [['intensity', 'Intensity', 'slider', 0.01, 0, 2, '%'], ['fringe', 'Fringe', 'slider', 0.01, 0, 1, '%'], ['timing', 'Timing', 'slider', 0.01, 0.02, 0.98, '%'], ['angle', 'Sweep angle', 'slider', 1, -180, 180, '°']],
    help: 'Draw the drag path: click to add a point, drag to move, double-click to remove. Animate Timing to replay the drag.',
  },
  Code: {
    label: 'Code',
    icon: 'M8 7l-5 5 5 5M16 7l5 5-5 5',
    fields: [['amount', 'Amount', 'slider', 0.01, 0, 1, '%']],
  },
}

const kindOf = (e: ImageEffect) => Object.keys(e)[0] ?? ''

function Icon({ d }: { d: string }) {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" className="shrink-0">
      <path d={d} />
    </svg>
  )
}

const row = (label: string, child: ReactNode, key?: string) => (
  <div key={key ?? label} className="flex items-center gap-2">
    <span className="w-20 shrink-0 text-[11px] text-ink-dim">{label}</span>
    <div className="flex min-w-0 flex-1 items-center gap-1">{child}</div>
  </div>
)

function fmt(v: number, unit: string | undefined) {
  if (unit === '%') return `${Math.round(v * 100)}%`
  if (unit === '°') return `${Math.round(v)}°`
  if (unit === 'x') return `${v.toFixed(2)}×`
  return v >= 10 ? `${Math.round(v)}` : v.toFixed(1)
}

function Seg({ value, options, onChange }: { value: string; options: [string, string][]; onChange: (v: string) => void }) {
  return (
    <div className="flex min-w-0 flex-1 rounded-md bg-overlay/8 p-0.5">
      {options.map(([v, l]) => (
        <button key={v} onClick={() => onChange(v)} className={`min-w-0 flex-1 truncate rounded px-1.5 py-0.5 text-[11px] ${value === v ? 'bg-overlay/15 font-medium text-ink' : 'text-ink-dim hover:text-ink'}`}>
          {l}
        </button>
      ))}
    </div>
  )
}

/**
 * A square pad over the node's box (0..1, y down) with draggable points.
 * `line` draws them connected; `onAdd` (click on empty pad) / `onRemove`
 * (double-click a point) make it a path editor.
 */
function Pad({ points, onChange, line, onAdd, onRemove }: { points: [number, number][]; onChange: (i: number, p: [number, number]) => void; line?: boolean; onAdd?: (p: [number, number]) => void; onRemove?: (i: number) => void }) {
  const box = useRef<HTMLDivElement>(null)
  const [drag, setDrag] = useState<number | null>(null)
  const at = (e: { clientX: number; clientY: number }): [number, number] => {
    const r = box.current!.getBoundingClientRect()
    return [Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)), Math.min(1, Math.max(0, (e.clientY - r.top) / r.height))]
  }
  return (
    <div
      ref={box}
      className="relative aspect-square w-full cursor-crosshair overflow-hidden rounded-md border border-panel-border bg-black/40"
      style={{ backgroundImage: 'linear-gradient(rgb(255 255 255 / 0.05) 1px, transparent 1px), linear-gradient(90deg, rgb(255 255 255 / 0.05) 1px, transparent 1px)', backgroundSize: '12.5% 12.5%' }}
      onPointerDown={(e) => {
        if (e.target === box.current && onAdd) onAdd(at(e))
      }}
      onPointerMove={(e) => drag !== null && onChange(drag, at(e))}
      onPointerUp={() => setDrag(null)}
      onPointerLeave={() => setDrag(null)}
    >
      <svg className="pointer-events-none absolute inset-0 h-full w-full" viewBox="0 0 100 100" preserveAspectRatio="none">
        {line && points.length > 1 && <polyline points={points.map((p) => `${p[0] * 100},${p[1] * 100}`).join(' ')} fill="none" stroke="white" strokeWidth="1.2" vectorEffect="non-scaling-stroke" strokeLinejoin="round" />}
      </svg>
      {points.map((p, i) => (
        <div
          key={i}
          onPointerDown={(e) => {
            e.stopPropagation()
            ;(e.target as HTMLElement).setPointerCapture?.(e.pointerId)
            setDrag(i)
          }}
          onPointerMove={(e) => drag === i && onChange(i, at(e))}
          onPointerUp={() => setDrag(null)}
          onDoubleClick={() => onRemove?.(i)}
          className="absolute h-3 w-3 -translate-x-1/2 -translate-y-1/2 cursor-grab rounded-full border-2 border-white bg-accent shadow"
          style={{ left: `${p[0] * 100}%`, top: `${p[1] * 100}%` }}
          title={onRemove ? 'Drag to move · double-click to remove' : 'Drag'}
        />
      ))}
    </div>
  )
}

/** The Code effect's source editor. */
function CodeFields({ editor, params, onChange }: { editor: WasmEditor; params: Record<string, unknown>; onChange: (p: Record<string, unknown>) => void }) {
  const code = String(params.code ?? '')
  const [draft, setDraft] = useState(code)
  useEffect(() => setDraft(code), [code])
  const ps = (params.params ?? []) as { name: string; value: number; min: number; max: number }[]
  const cs = (params.colors ?? []) as { name: string; value: Rgba }[]
  const error = codeEffectError(
    editor,
    code,
    ps.map((p) => p.name),
    cs.map((c) => c.name),
  )
  const apply = () => {
    const parsed = codeEffectParams(editor, draft)
    // Keep the current value of a param / colour that kept its name.
    const nextParams = parsed.params.map((p) => ({ ...p, value: ps.find((o) => o.name === p.name)?.value ?? p.value }))
    const nextColors = parsed.colors.map((c) => ({ ...c, value: cs.find((o) => o.name === c.name)?.value ?? c.value }))
    onChange({ ...params, code: draft, params: nextParams, colors: nextColors })
  }
  return (
    <div className="flex flex-col gap-1.5">
      <textarea
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          e.stopPropagation()
          if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) apply()
          if (e.key === 'Tab') {
            e.preventDefault()
            const t = e.currentTarget
            const s = t.selectionStart
            setDraft(draft.slice(0, s) + '    ' + draft.slice(t.selectionEnd))
            requestAnimationFrame(() => t.setSelectionRange(s + 4, s + 4))
          }
        }}
        spellCheck={false}
        rows={14}
        className="w-full resize-y rounded-md border border-panel-border bg-black/50 p-2 font-mono text-[10.5px] leading-snug text-ink outline-none focus:border-accent"
      />
      <div className="flex items-center gap-2">
        <button onClick={apply} disabled={draft === code} className="rounded-md bg-accent/15 px-2.5 py-1 text-[11px] font-medium text-accent hover:bg-accent/25 disabled:opacity-40">
          Apply
        </button>
        <span className="text-[10px] text-ink-dim">Ctrl+Enter · `// @param name min max default`, `// @color name #hex`, `sample(uv)`, `time`</span>
      </div>
      {error && <pre className="max-h-32 overflow-auto whitespace-pre-wrap rounded bg-red-500/10 p-1.5 font-mono text-[10px] text-red-300">{error}</pre>}
      {ps.map((p, i) =>
        row(
          p.name,
          <>
            <div className="min-w-0 flex-1">
              <Slider value={Math.min(p.max, Math.max(p.min, p.value))} min={p.min} max={p.max} step={(p.max - p.min) / 200 || 0.01} onChange={(v) => onChange({ ...params, params: ps.map((q, k) => (k === i ? { ...q, value: v } : q)) })} />
            </div>
            <span className="w-10 shrink-0 text-right text-[10px] tabular-nums text-ink-dim">{p.value.toFixed(2)}</span>
          </>,
          `p:${p.name}`,
        ),
      )}
      {cs.map((c, i) => row(c.name, <ColorSwatch value={c.value} onChange={(v) => onChange({ ...params, colors: cs.map((q, k) => (k === i ? { ...q, value: v } : q)) })} />, `c:${c.name}`))}
    </div>
  )
}

function EffectBody({ kind, params, onChange, editor, images, ai }: { kind: string; params: Record<string, unknown>; onChange: (p: Record<string, unknown>) => void; editor: WasmEditor; images: ImageAssetSummary[]; ai: AiContext }) {
  const schema = FX[kind]
  const set = (key: string, value: unknown) => onChange({ ...params, [key]: value })
  const fields = (list: Field[]) =>
    list.map((f) => {
      const [key, label, type] = f
      const v = params[key]
      if (type === 'color') return row(label, <ColorSwatch value={v as Rgba} onChange={(c) => set(key, c)} />, key)
      if (type === 'toggle')
        return (
          <label key={key} className="flex items-center gap-1.5 text-[11px] text-ink-dim">
            <input type="checkbox" checked={!!v} onChange={(e) => set(key, e.target.checked)} />
            {label}
          </label>
        )
      if (type === 'seg') return row(label, <Seg value={String(v)} options={f[3] as [string, string][]} onChange={(x) => set(key, x)} />, key)
      if (type === 'xy') {
        const xy = v as [number, number]
        return row(
          label,
          <>
            {[0, 1].map((k) => (
              <input
                key={k}
                type="number"
                step={f[3] as number}
                value={xy[k]}
                onChange={(e) => set(key, k === 0 ? [Number(e.target.value), xy[1]] : [xy[0], Number(e.target.value)])}
                className="w-full min-w-0 rounded bg-overlay/10 px-1.5 py-0.5 text-[11px] tabular-nums text-ink outline-none"
              />
            ))}
          </>,
          key,
        )
      }
      const [, , , step, min, max, unit] = f as [string, string, 'slider', number, number, number, string?]
      const n = Number(v ?? 0)
      return row(
        label,
        <>
          <div className="min-w-0 flex-1">
            <Slider value={Math.min(max, Math.max(min, n))} min={min} max={max} step={step} onChange={(x) => set(key, x)} />
          </div>
          <span className="w-10 shrink-0 text-right text-[10px] tabular-nums text-ink-dim">{fmt(n, unit)}</span>
        </>,
        key,
      )
    })

  const depthPicker = (
    <>
      {row(
        'Depth image',
        <div className="min-w-0 flex-1">
          <ImageAssetPicker editor={editor} images={images} value={(params.depthImage as OpaqueId | null) ?? null} onChange={(id) => set('depthImage', id)} />
        </div>,
        'depthImage',
      )}
      {ai.sourceImage && (
        <AiButton
          label="Generate depth from this image (AI)"
          run={async (progress) => {
            const depth = await estimateDepth(getImageSource(editor, ai.sourceImage!), progress)
            const res = await importImageAsset(editor, `${ai.node.name || 'image'} depth.png`, depth)
            ai.onDocumentChange(res.document)
            if (res.newImages[0]) set('depthImage', res.newImages[0])
          }}
        />
      )}
      {!params.depthImage && <p className="text-[10px] leading-snug text-ink-dim">No depth image: depth is estimated (lower and brighter = nearer). {ai.sourceImage ? 'Generate one, or pick' : 'Pick'} a greyscale depth image (white = near) for real depth.</p>}
    </>
  )

  return (
    <div className="flex flex-col gap-1.5">
      {schema.mode && <Seg value={String(params[schema.mode[0]])} options={schema.mode[1]} onChange={(x) => set(schema.mode![0], schema.mode![0] === 'mono' ? x === 'true' : x)} />}
      {kind === 'RibbedGlass' && <Pad points={[params.center as [number, number]]} onChange={(_, p) => set('center', p)} />}
      {kind === 'Blur' && params.mode === 'Progressive' && (
        <>
          <Pad points={[params.start as [number, number], params.end as [number, number]]} line onChange={(i, p) => set(i === 0 ? 'start' : 'end', p)} />
          <p className="text-[10px] text-ink-dim">Sharp at the first dot, fully blurred at the second.</p>
        </>
      )}
      {kind === 'ScannerDrag' && (
        <Pad
          points={(params.points as [number, number][]) ?? []}
          line
          onChange={(i, p) => set('points', (params.points as [number, number][]).map((q, k) => (k === i ? p : q)))}
          onAdd={(p) => (params.points as [number, number][]).length < 8 && set('points', [...(params.points as [number, number][]), p])}
          onRemove={(i) => set('points', (params.points as [number, number][]).filter((_, k) => k !== i))}
        />
      )}
      {fields(schema.fields)}
      {kind === 'Blur' && params.mode === 'Depth' && (
        <>
          {fields([
            ['focus', 'Focus depth', 'slider', 0.01, 0, 1, '%'],
            ['focusRange', 'Focus range', 'slider', 0.01, 0, 1, '%'],
          ])}
          {depthPicker}
        </>
      )}
      {kind === 'DepthMap' && depthPicker}
      {kind === 'GradientMap' && <StopsField stops={(params.stops ?? []) as { position: number; color: Rgba }[]} onChange={(stops) => set('stops', stops)} />}
      {kind === 'Code' && <CodeFields editor={editor} params={params} onChange={onChange} />}
      {schema.help && <p className="text-[10px] leading-snug text-ink-dim">{schema.help}</p>}
    </div>
  )
}

/** What the AI tools need: the node, its picture (Image nodes) and a way to
 * apply document changes (new assets / layers). */
export interface AiContext {
  node: NodeSummary
  artboard: OpaqueId
  sourceImage: OpaqueId | null
  onDocumentChange: (doc: DocumentSummary) => void
}

/** A button that runs an AI job with progress text and errors in a dialog. */
function AiButton({ label, run }: { label: string; run: (progress: (msg: string) => void) => Promise<void> }) {
  const [busy, setBusy] = useState<string | null>(null)
  return (
    <button
      disabled={!!busy}
      onClick={async () => {
        setBusy('Starting…')
        try {
          await run(setBusy)
        } catch (err) {
          void showAlert(String(err instanceof Error ? err.message : err), { title: 'AI tool failed' })
        } finally {
          setBusy(null)
        }
      }}
      title="Runs on this device. Downloads the model once (needs internet the first time)."
      className="flex items-center justify-center gap-1.5 rounded-md bg-gradient-to-r from-fuchsia-500/20 to-sky-500/20 px-2 py-1 text-[11px] font-medium text-ink hover:from-fuchsia-500/30 hover:to-sky-500/30 disabled:opacity-60"
    >
      <Icon d="M12 3l1.8 4.7L18.5 9.5l-4.7 1.8L12 16l-1.8-4.7L5.5 9.5l4.7-1.8zM19 15l.9 2.1L22 18l-2.1.9L19 21l-.9-2.1L16 18l2.1-.9z" />
      {busy ?? label}
    </button>
  )
}

/** Gradient Map stops: a preview bar plus a row per stop (≤ 8). */
function StopsField({ stops, onChange }: { stops: { position: number; color: Rgba }[]; onChange: (s: { position: number; color: Rgba }[]) => void }) {
  const sorted = [...stops].sort((a, b) => a.position - b.position)
  const css = (c: Rgba) => `rgba(${Math.round(c[0] * 255)},${Math.round(c[1] * 255)},${Math.round(c[2] * 255)},${c[3]})`
  return (
    <div className="flex flex-col gap-1">
      <div className="h-3 rounded" style={{ background: sorted.length ? `linear-gradient(90deg, ${sorted.map((s) => `${css(s.color)} ${s.position * 100}%`).join(', ')})` : 'transparent' }} />
      {stops.map((s, i) => (
        <div key={i} className="flex items-center gap-1.5">
          <ColorSwatch value={s.color} onChange={(c) => onChange(stops.map((q, k) => (k === i ? { ...q, color: c } : q)))} />
          <div className="min-w-0 flex-1">
            <Slider value={s.position} min={0} max={1} step={0.01} onChange={(p) => onChange(stops.map((q, k) => (k === i ? { ...q, position: p } : q)))} />
          </div>
          <span className="w-8 text-right text-[10px] tabular-nums text-ink-dim">{Math.round(s.position * 100)}%</span>
          <button disabled={stops.length <= 2} onClick={() => onChange(stops.filter((_, k) => k !== i))} className="rounded px-1 text-ink-dim hover:text-red-400 disabled:opacity-30" title="Remove stop">
            ✕
          </button>
        </div>
      ))}
      {stops.length < 8 && (
        <button onClick={() => onChange([...stops, { position: 0.5, color: [1, 1, 1, 1] }])} className="self-start rounded px-1.5 py-0.5 text-[11px] text-ink-dim hover:bg-overlay/8 hover:text-ink">
          + Stop
        </button>
      )}
    </div>
  )
}

/** The element's own settings: background behind it, clip to its shape. */
function ElementFields({ config, set, editor, images }: { config: ImageEffectsConfig; set: (p: Partial<ImageEffectsConfig>) => void; editor: WasmEditor; images: ImageAssetSummary[] }) {
  const bg = (config.background ?? [0, 0, 0, 0]) as Rgba
  const hasBg = bg[3] > 0 || !!config.backgroundImage
  const num = (label: string, key: 'backgroundRadius' | 'backgroundPadding' | 'backgroundImageOpacity', min: number, max: number, step: number, unit?: string) =>
    row(
      label,
      <>
        <div className="min-w-0 flex-1">
          <Slider value={Math.min(max, Math.max(min, Number(config[key] ?? 0)))} min={min} max={max} step={step} onChange={(v) => set({ [key]: v })} />
        </div>
        <span className="w-10 shrink-0 text-right text-[10px] tabular-nums text-ink-dim">{fmt(Number(config[key] ?? 0), unit)}</span>
      </>,
      key,
    )
  return (
    <div className="flex flex-col gap-1.5 rounded-md border border-panel-border p-1.5">
      <div className="flex items-center gap-1.5 text-xs font-medium text-ink">
        <Icon d="M4 4h16v16H4z" />
        Element
      </div>
      {row('Background', <ColorSwatch value={bg} onChange={(c) => set({ background: c })} />, 'bg')}
      {row(
        'Bg image',
        <div className="min-w-0 flex-1">
          <ImageAssetPicker editor={editor} images={images} value={config.backgroundImage ?? null} onChange={(id) => set({ backgroundImage: id })} />
        </div>,
        'bgimg',
      )}
      {config.backgroundImage && num('Image opacity', 'backgroundImageOpacity', 0, 1, 0.01, '%')}
      {hasBg && num('Radius', 'backgroundRadius', 0, 200, 1)}
      {hasBg && num('Padding', 'backgroundPadding', -50, 200, 1)}
      {!hasBg && <p className="text-[10px] leading-snug text-ink-dim">Give it a colour (alpha above 0) or an image to fill the element's box behind its content.</p>}
      <label className="flex items-center gap-1.5 text-[11px] text-ink-dim" title="Masks the final result (background and effects too) to the element's own drawn shape">
        <input type="checkbox" checked={!!config.clipToShape} onChange={(e) => set({ clipToShape: e.target.checked })} />
        Clip to element shape
      </label>
    </div>
  )
}

/** The Add menu (grouped like an image editor's). */
function AddEffect({ onAdd }: { onAdd: (kind: string) => void }) {
  const [open, setOpen] = useState(false)
  const anchor = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!open) return
    const down = (e: PointerEvent) => !insideOrPopover(anchor.current, e.target) && setOpen(false)
    window.addEventListener('pointerdown', down)
    return () => window.removeEventListener('pointerdown', down)
  }, [open])
  return (
    <div ref={anchor} className="relative">
      <button onClick={() => setOpen((o) => !o)} className="w-full rounded-md border border-dashed border-panel-border px-2 py-1 text-[11px] text-ink-dim hover:border-accent hover:text-ink">
        + Add effect
      </button>
      {open && (
        <Popover anchor={anchor} minWidth={200} className="rounded-lg border border-panel-border bg-solid-panel p-1 shadow-2xl">
          {MENU.map((k, i) =>
            k === null ? (
              <div key={`sep${i}`} className="my-1 h-px bg-overlay/10" />
            ) : (
              <button
                key={k}
                onClick={() => {
                  onAdd(k)
                  setOpen(false)
                }}
                className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs text-ink hover:bg-accent hover:text-white"
              >
                <Icon d={FX[k].icon} />
                {FX[k].label}
              </button>
            ),
          )}
        </Popover>
      )}
    </div>
  )
}

export function ImageEffectsFields({ config, onChange, editor, images, ai }: { config: ImageEffectsConfig; onChange: (c: ImageEffectsConfig) => void; editor: WasmEditor; images: ImageAssetSummary[]; ai: AiContext }) {
  const [collapsed, setCollapsed] = useState<Record<number, boolean>>({})
  const effects = config.effects
  const setAt = (i: number, patch: Partial<(typeof effects)[number]>) => onChange({ ...config, effects: effects.map((e, k) => (k === i ? { ...e, ...patch } : e)) })
  const setConfig = (patch: Partial<ImageEffectsConfig>) => onChange({ ...config, ...patch })
  const move = (i: number, d: number) => {
    const j = i + d
    if (j < 0 || j >= effects.length) return
    const next = [...effects]
    ;[next[i], next[j]] = [next[j], next[i]]
    onChange({ ...config, effects: next })
  }
  return (
    <div className="flex flex-col gap-2">
      <ElementFields config={config} set={setConfig} editor={editor} images={images} />
      {effects.length === 0 && <p className="text-[11px] leading-snug text-ink-dim">The node is drawn as an image and these effects run on it, in order. Every value can be keyframed.</p>}
      {effects.map((layer, i) => {
        const kind = kindOf(layer.effect)
        const schema = FX[kind]
        if (!schema) return null
        const open = !collapsed[i]
        return (
          <div key={i} className={`flex flex-col gap-1.5 rounded-md border border-panel-border p-1.5 ${layer.enabled ? '' : 'opacity-60'}`}>
            <div className="flex items-center gap-1.5">
              <button onClick={() => setCollapsed((c) => ({ ...c, [i]: open }))} className="flex min-w-0 flex-1 items-center gap-1.5 text-left text-xs font-medium text-ink" title={open ? 'Collapse' : 'Expand'}>
                <Icon d={schema.icon} />
                <span className="truncate">{schema.label}</span>
              </button>
              <button onClick={() => setAt(i, { enabled: !layer.enabled })} title={layer.enabled ? 'Hide' : 'Show'} className="rounded px-1 text-ink-dim hover:bg-overlay/10 hover:text-ink">
                <Icon d={layer.enabled ? 'M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z' : 'M3 3l18 18M10.6 5.1A10 10 0 0 1 12 5c6 0 10 7 10 7a17 17 0 0 1-3 3.6M6.6 6.6A17 17 0 0 0 2 12s4 7 10 7a9.6 9.6 0 0 0 5.4-1.6'} />
              </button>
              <button onClick={() => move(i, -1)} title="Move up" className="rounded px-1 text-ink-dim hover:bg-overlay/10 hover:text-ink">
                ↑
              </button>
              <button onClick={() => move(i, 1)} title="Move down" className="rounded px-1 text-ink-dim hover:bg-overlay/10 hover:text-ink">
                ↓
              </button>
              <button onClick={() => onChange({ ...config, effects: effects.filter((_, k) => k !== i) })} title="Remove" className="rounded px-1 text-ink-dim hover:bg-red-500/10 hover:text-red-400">
                ✕
              </button>
            </div>
            {open && <EffectBody kind={kind} params={layer.effect[kind]} editor={editor} images={images} ai={ai} onChange={(p) => setAt(i, { effect: { [kind]: p } })} />}
          </div>
        )
      })}
      {ai.sourceImage && ai.node.kind === 'Image' && (
        <AiButton
          label="Cut out subject → new layer on top (AI)"
          run={async (progress) => {
            const cut = await cutOutSubject(getImageSource(editor, ai.sourceImage!), progress)
            const asset = await importImageAsset(editor, `${ai.node.name || 'image'} cutout.png`, cut)
            ai.onDocumentChange(asset.document)
            if (asset.newImages[0]) ai.onDocumentChange(addCutoutLayer(editor, ai.artboard, ai.node.id, asset.newImages[0]))
          }}
        />
      )}
      <AddEffect onAdd={(kind) => onChange({ ...config, effects: [...effects, { effect: imageEffectDefault(editor, kind), enabled: true }] })} />
    </div>
  )
}
