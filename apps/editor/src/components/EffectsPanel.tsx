import { useEffect, useRef, useState } from 'react'
import type { PostEffect, PostEffectLayer } from '../ipc/document'
import { POST_EFFECT_DEFAULTS } from '../ipc/document'
import { ColorSwatch } from './ColorPicker'
import { NumberField } from './InspectorPanel'
import { ChevronDownIcon, PlusIcon, TrashIcon } from './icons'

export const EFFECT_KINDS = Object.keys(POST_EFFECT_DEFAULTS).filter((kind) => kind !== 'None') as Exclude<PostEffect['kind'], 'None'>[]

/** A small popover menu listing every addable effect kind — same "button
 * that opens a popover list of choices" shape `TopToolbar.tsx`'s
 * `ShapesMenu` already established, not `InspectorPanel.tsx`'s `Dropdown`
 * (that one always shows a persisted *current* value; this is a one-shot
 * action menu with no "current" selection of its own). Opens downward
 * (`top-full`) with the same panel styling (`bg-panel`/`backdrop-blur-panel`/
 * rounded option rows) `Dropdown`'s own popover uses, for visual consistency
 * with every other picker in the app instead of reading as a plain native
 * menu. */
function AddEffectMenu({ onAdd }: { onAdd: (kind: Exclude<PostEffect['kind'], 'None'>) => void }) {
  const [open, setOpen] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const handlePointerDown = (e: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false)
    }
    window.addEventListener('pointerdown', handlePointerDown)
    return () => window.removeEventListener('pointerdown', handlePointerDown)
  }, [open])

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="flex w-full items-center justify-center gap-1 rounded-md bg-overlay/5 px-2 py-1 text-[11px] text-ink-dim hover:bg-overlay/8 hover:text-ink"
      >
        <PlusIcon />
        Add Effect
      </button>
      {open && (
        <div className="absolute left-0 top-full z-20 mt-1 w-full overflow-hidden rounded-lg border border-panel-border bg-panel p-1 shadow-2xl backdrop-blur-panel">
          {EFFECT_KINDS.map((kind) => (
            <button
              key={kind}
              type="button"
              onClick={() => {
                onAdd(kind)
                setOpen(false)
              }}
              className="block w-full rounded-md px-2 py-1.5 text-left text-xs text-ink hover:bg-accent/15 hover:text-accent"
            >
              {kind}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

/** One row's own parameter fields — varies by `effect.kind`, each field a
 * plain `NumberField` (no dedicated slider UI yet; every one of these is
 * already a free-typed/scroll-nudged number field, same as every other
 * numeric Inspector control). */
export function EffectParams({ effect, onChange }: { effect: PostEffect; onChange: (effect: PostEffect) => void }) {
  switch (effect.kind) {
    case 'None':
      return null
    case 'Blur':
      return <NumberField label="Radius" value={effect.radius} onCommit={(radius) => onChange({ ...effect, radius: Math.max(0, radius) })} />
    case 'Glow':
      return (
        <>
          <NumberField label="Radius" value={effect.radius} onCommit={(radius) => onChange({ ...effect, radius: Math.max(0, radius) })} />
          <NumberField
            label="Threshold"
            value={effect.threshold}
            step={0.05}
            onCommit={(threshold) => onChange({ ...effect, threshold: Math.min(1, Math.max(0, threshold)) })}
          />
          <NumberField
            label="Intensity"
            value={effect.intensity}
            step={0.1}
            onCommit={(intensity) => onChange({ ...effect, intensity: Math.max(0, intensity) })}
          />
        </>
      )
    case 'Pixelate':
      return (
        <NumberField label="Block Size" value={effect.blockSize} onCommit={(blockSize) => onChange({ ...effect, blockSize: Math.max(1, blockSize) })} />
      )
    case 'ColorGrading':
      return (
        <>
          <NumberField
            label="Brightness"
            value={effect.brightness}
            step={0.05}
            onCommit={(brightness) => onChange({ ...effect, brightness })}
          />
          <NumberField label="Contrast" value={effect.contrast} step={0.05} onCommit={(contrast) => onChange({ ...effect, contrast: Math.max(0, contrast) })} />
          <NumberField
            label="Saturation"
            value={effect.saturation}
            step={0.05}
            onCommit={(saturation) => onChange({ ...effect, saturation: Math.max(0, saturation) })}
          />
        </>
      )
    case 'Posterize':
      return (
        <NumberField label="Levels" value={effect.levels} step={1} onCommit={(levels) => onChange({ ...effect, levels: Math.max(2, Math.round(levels)) })} />
      )
    case 'GradientMap':
      return (
        <div className="flex items-center gap-3">
          <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
            Shadow
            <ColorSwatch value={effect.shadow} onChange={(shadow) => onChange({ ...effect, shadow })} />
          </label>
          <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
            Highlight
            <ColorSwatch value={effect.highlight} onChange={(highlight) => onChange({ ...effect, highlight })} />
          </label>
        </div>
      )
    case 'Vignette':
      return <VignetteFields value={effect} onChange={(v) => onChange({ ...effect, ...v })} />
    default:
      return <GenericEffectFields effect={effect} onChange={onChange} />
  }
}

/** Field specs for the newer effects: `[key, label, step, min, max]` for
 * numbers, `[key, label, 'color']` for colours, `[key, label, 'toggle']`
 * for 0/1 switches. */
type FieldSpec = [string, string, number, number, number] | [string, string, 'color' | 'toggle']
const EFFECT_FIELDS: Partial<Record<PostEffect['kind'], FieldSpec[]>> = {
  Anaglyph: [['offset', 'Offset (px)', 0.5, 0, 40], ['mono', 'Mono', 0.05, 0, 1]],
  Dither: [['levels', 'Levels', 1, 2, 16], ['scale', 'Pixel size', 1, 1, 16], ['strength', 'Strength', 0.05, 0, 2], ['mono', 'Two colours', 'toggle'], ['dark', 'Dark', 'color'], ['light', 'Light', 'color']],
  ChromaticAberration: [['strength', 'Strength (px)', 0.5, 0, 40]],
  Scanlines: [['intensity', 'Intensity', 0.05, 0, 1], ['spacing', 'Spacing (px)', 0.5, 1, 16], ['mask', 'RGB mask', 0.05, 0, 1]],
  FilmGrain: [['amount', 'Amount', 0.01, 0, 1], ['size', 'Grain (px)', 0.25, 0.5, 8]],
  Sharpen: [['amount', 'Amount', 0.05, 0, 2]],
  EdgeDetect: [['thickness', 'Thickness (px)', 0.25, 0.5, 8], ['threshold', 'Threshold', 0.02, 0, 2], ['color', 'Ink', 'color'], ['only', 'Lines only', 'toggle']],
  Halftone: [['size', 'Dot spacing (px)', 0.5, 2, 40], ['angle', 'Angle', 5, 0, 180], ['mix', 'Mix original', 0.05, 0, 1], ['ink', 'Ink', 'color'], ['paper', 'Paper', 'color']],
  WaveDistortion: [['amplitude', 'Amplitude (px)', 0.5, 0, 60], ['frequency', 'Waves', 0.5, 0, 60], ['speed', 'Speed', 0.1, -20, 20]],
  LensDistortion: [['strength', 'Strength', 0.02, -1, 1], ['zoom', 'Zoom', 0.01, 0.5, 2]],
}

/** The newer effects' fields, from `EFFECT_FIELDS`. */
function GenericEffectFields({ effect, onChange }: { effect: PostEffect; onChange: (effect: PostEffect) => void }) {
  const fields = EFFECT_FIELDS[effect.kind] ?? []
  const values = effect as unknown as Record<string, number | [number, number, number, number]>
  const set = (key: string, value: unknown) => onChange({ ...effect, [key]: value } as PostEffect)
  return (
    <>
      {fields.map((spec) => {
        const [key, label] = spec
        if (spec[2] === 'color') {
          return (
            <label key={key} className="flex items-center gap-1.5 text-[11px] text-ink-dim">
              {label}
              <ColorSwatch value={values[key] as [number, number, number, number]} onChange={(c) => set(key, c)} />
            </label>
          )
        }
        if (spec[2] === 'toggle') {
          return (
            <label key={key} className="flex items-center gap-1.5 text-[11px] text-ink-dim">
              <input type="checkbox" checked={(values[key] as number) > 0.5} onChange={(e) => set(key, e.target.checked ? 1 : 0)} />
              {label}
            </label>
          )
        }
        const [, , step, min, max] = spec as [string, string, number, number, number]
        return <NumberField key={key} label={label} value={values[key] as number} step={step} onCommit={(v) => set(key, Math.min(max, Math.max(min, v)))} />
      })}
    </>
  )
}

/** Color/Intensity/Radius/Blur — shared by the Vignette post effect and the
 * Camera's own vignette (Inspector), so both read identically. */
export function VignetteFields({
  value,
  onChange,
}: {
  value: { color: [number, number, number, number]; intensity: number; radius: number; blur: number }
  onChange: (value: { color: [number, number, number, number]; intensity: number; radius: number; blur: number }) => void
}) {
  const clamp = (v: number, min: number, max: number) => Math.min(max, Math.max(min, v))
  return (
    <>
      <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
        Color
        <ColorSwatch value={value.color} onChange={(color) => onChange({ ...value, color })} />
      </label>
      <NumberField label="Intensity" value={value.intensity} step={0.05} onCommit={(intensity) => onChange({ ...value, intensity: clamp(intensity, 0, 1) })} />
      <NumberField label="Radius" value={value.radius} step={0.05} onCommit={(radius) => onChange({ ...value, radius: clamp(radius, 0, 1.5) })} />
      <NumberField label="Blur" value={value.blur} step={0.05} onCommit={(blur) => onChange({ ...value, blur: clamp(blur, 0, 1) })} />
    </>
  )
}

/** One stack entry — enabled toggle, kind label, its own parameter fields,
 * reorder up/down, delete. No drag-and-drop reordering (yet) — up/down
 * buttons are a smaller, keyboard-and-touch-friendly slice of "reorder the
 * stack" that doesn't need pointer-drag plumbing at all, same "smallest
 * real slice" discipline this workspace's other first-version features
 * (`an1mbyt3-lottie`'s importer, per its own module doc) already follow. */
function EffectRow({
  layer,
  onChange,
  onRemove,
  onMoveUp,
  onMoveDown,
  isFirst,
  isLast,
}: {
  layer: PostEffectLayer
  onChange: (layer: PostEffectLayer) => void
  onRemove: () => void
  onMoveUp: () => void
  onMoveDown: () => void
  isFirst: boolean
  isLast: boolean
}) {
  return (
    <div className={`rounded-md border border-panel-border/60 p-1.5 ${layer.enabled ? '' : 'opacity-50'}`}>
      <div className="flex items-center gap-1.5">
        <input
          type="checkbox"
          checked={layer.enabled}
          onChange={(e) => onChange({ ...layer, enabled: e.target.checked })}
          className="h-3.5 w-3.5 shrink-0 accent-accent"
          title={layer.enabled ? 'Disable this effect' : 'Enable this effect'}
        />
        <span className="flex-1 truncate text-xs text-ink">{layer.effect.kind}</span>
        <button
          type="button"
          onClick={onMoveUp}
          disabled={isFirst}
          title="Move up (applies earlier in the stack)"
          className="flex h-5 w-5 shrink-0 rotate-180 items-center justify-center rounded text-ink-dim hover:bg-overlay/8 hover:text-ink disabled:opacity-30 disabled:hover:bg-transparent"
        >
          <ChevronDownIcon />
        </button>
        <button
          type="button"
          onClick={onMoveDown}
          disabled={isLast}
          title="Move down (applies later in the stack)"
          className="flex h-5 w-5 shrink-0 items-center justify-center rounded text-ink-dim hover:bg-overlay/8 hover:text-ink disabled:opacity-30 disabled:hover:bg-transparent"
        >
          <ChevronDownIcon />
        </button>
        <button
          type="button"
          onClick={onRemove}
          title="Remove this effect"
          className="flex h-5 w-5 shrink-0 items-center justify-center rounded text-ink-dim hover:bg-overlay/8 hover:text-red-400"
        >
          <TrashIcon />
        </button>
      </div>
      <div className="mt-1.5 flex flex-col gap-1">
        <EffectParams effect={layer.effect} onChange={(effect) => onChange({ ...layer, effect })} />
      </div>
    </div>
  )
}

/** The Effects panel's actual content — the artboard's post-processing
 * stack (AN1MBYT3_BUILD_PLAN.md §11's "layered approach, allow for
 * duplicate effects"). `onChange` always receives the *whole* replacement
 * list (add/remove/reorder/toggle/edit-a-param all funnel through it), same
 * "one undoable edit, whole-list swap" shape `Command::SetPostEffects`
 * itself is built around — see that command's own doc comment for why.
 * Adding the same kind twice (pressing "Add Effect" > Blur twice) is a
 * completely ordinary, fully-supported case: each addition just appends
 * another independent `PostEffectLayer`, chained after the first by
 * `Renderer::render_with_post_effects`, not merged/deduplicated. */
export function EffectsPanel({ layers, onChange }: { layers: PostEffectLayer[]; onChange: (layers: PostEffectLayer[]) => void }) {
  const updateAt = (index: number, layer: PostEffectLayer) => {
    onChange(layers.map((l, i) => (i === index ? layer : l)))
  }
  const removeAt = (index: number) => {
    onChange(layers.filter((_, i) => i !== index))
  }
  const swap = (a: number, b: number) => {
    const next = [...layers]
    ;[next[a], next[b]] = [next[b], next[a]]
    onChange(next)
  }
  const add = (kind: Exclude<PostEffect['kind'], 'None'>) => {
    onChange([...layers, { effect: POST_EFFECT_DEFAULTS[kind], enabled: true }])
  }

  return (
    <div className="flex flex-col gap-1.5">
      {layers.length === 0 && <p className="px-0.5 text-[11px] text-ink-dim">No effects yet — add one below.</p>}
      {layers.map((layer, index) => (
        <EffectRow
          key={index}
          layer={layer}
          onChange={(next) => updateAt(index, next)}
          onRemove={() => removeAt(index)}
          onMoveUp={() => swap(index, index - 1)}
          onMoveDown={() => swap(index, index + 1)}
          isFirst={index === 0}
          isLast={index === layers.length - 1}
        />
      ))}
      <AddEffectMenu onAdd={add} />
    </div>
  )
}
