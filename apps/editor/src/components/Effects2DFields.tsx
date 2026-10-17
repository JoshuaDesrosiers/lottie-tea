/**
 * Inspector UI for the 2D draw effects, light extras and sorting layers:
 *
 * - `SIMPLE_COMPONENT_FIELDS` + `SimpleComponentFields` — a small schema
 *   renderer for the plain-number / colour / toggle / xy components
 *   (Drop Shadow, Outline, Repeater, Mirror, Wiggle, Parallax, Pixel Snap,
 *   Stroke Style, Camera Shake, Light Flicker).
 * - `SortingLayerField` — the node's Layer (Unity's Sorting Layer), with a
 *   small manager to add, rename, reorder and remove the project's layers.
 * - `LightLayersFields` — which layers a Light 2D lights.
 */
import { Select } from './Select'
import { useEffect, useState } from 'react'
import type { DocumentSummary, NodeBehavior, NodeBehaviorEntry } from '../ipc/document'
import type { WasmEditor } from '../ipc/wasmEditor'
import { ColorSwatch } from './ColorPicker'
import { NumberField } from './InspectorPanel'
import { Slider } from './Slider'

type Rgba = [number, number, number, number]
/** `[key, label, kind, …]`: number fields take `step, min, max` and
 * render as a slider + value; `num` is a free number field. */
type Spec =
  | [string, string, 'slider', number, number, number]
  | [string, string, 'num', number]
  | [string, string, 'color']
  | [string, string, 'toggle']
  | [string, string, 'xy', number]
  | [string, string, 'int', number, number]

export const SIMPLE_COMPONENT_FIELDS: Record<string, { fields: Spec[]; help: string }> = {
  Reference2D: {
    fields: [['opacity', 'Opacity', 'slider', 0.01, 0, 1], ['locked', 'Locked (clicks pass through)', 'toggle'], ['showWhenPlaying', 'Show while playing', 'toggle']],
    help: 'A reference object: shown while you design, never exported. Locked ones are selected from the Layers panel.',
  },
  DropShadow2D: {
    fields: [['color', 'Colour', 'color'], ['offset', 'Offset (px)', 'xy', 1], ['softness', 'Softness (px)', 'slider', 0.5, 0, 40]],
    help: 'A solid copy behind this (and everything inside it). Softness 0 = a hard pixel-art shadow.',
  },
  Outline2D: {
    fields: [['color', 'Colour', 'color'], ['width', 'Width (px)', 'slider', 0.5, 0.5, 20], ['smooth', 'Smooth (16 directions)', 'toggle']],
    help: 'An outline round the silhouette. 1 px with Smooth off is the classic pixel-art outline.',
  },
  Repeater2D: {
    fields: [['count', 'Copies', 'int', 1, 64], ['offset', 'Step offset', 'xy', 1], ['rotation', 'Step turn (°)', 'num', 1], ['scale', 'Step scale', 'slider', 0.01, 0.1, 2], ['endOpacity', 'Last opacity', 'slider', 0.01, 0, 1]],
    help: 'Copies stepping by the offset / turn / scale, fading toward the last. Combine with Wiggle or animate the step for motion graphics.',
  },
  Mirror2D: {
    fields: [['x', 'Mirror left ↔ right', 'toggle'], ['y', 'Mirror top ↕ bottom', 'toggle']],
    help: 'Mirrored copies across this node’s own axes (through its origin) — symmetric characters, patterns, kaleidoscopes.',
  },
  GooGloss2D: {
    fields: [
      ['depth', 'Depth from edge (px)', 'slider', 1, 1, 300],
      ['angle', 'Direction (°)', 'slider', 1, 0, 360],
      ['distance', 'Offset (px)', 'slider', 1, 0, 200],
      ['opacity', 'Opacity', 'slider', 0.01, 0, 1],
      ['softness', 'Softness', 'slider', 0.01, 0, 1],
      ['color', 'Colour', 'color'],
      ['blend', 'Colour amount', 'slider', 0.01, 0, 1],
    ],
    help: 'The shine on this goo group (put it on any member). Depth keeps it that many pixels in from the edge at any size (only parts thicker than twice that catch a shine); Direction and Offset nudge it (135° = up-left); Softness 0 keeps a crisp pixel-art edge. The group\'s Gloss still turns the shine on and scales it.',
  },
  AntiGoo2D: {
    fields: [['group', 'Goo group #', 'int', 0, 99], ['strength', 'Strength', 'slider', 0.05, 0, 5], ['hideSource', 'Hide this node while goo runs', 'toggle']],
    help: 'Carves into the Goo 2D group with this number: the liquid is pushed away from this shape (or text, sprite, particles) with the same gooey falloff, bending round it and splitting where it cuts through. Raise Strength to cut through thick goo.',
  },
  Wiggle2D: {
    fields: [['position', 'Position (px)', 'slider', 0.5, 0, 100], ['rotation', 'Rotation (°)', 'slider', 0.5, 0, 90], ['scale', 'Scale', 'slider', 0.01, 0, 1], ['frequency', 'Frequency (Hz)', 'slider', 0.1, 0, 30], ['seed', 'Seed', 'int', 0, 9999]],
    help: 'Smooth random motion (After Effects’ wiggle): floating, idle sway, nervous shake. Use different seeds to desync copies.',
  },
  Parallax2D: {
    fields: [['factor', 'Factor (x, y)', 'xy', 0.05]],
    help: '0 = part of the world, 0.5 = a far layer (half speed), 1 = stuck to the camera, negative = a near foreground layer.',
  },
  PixelSnap2D: {
    fields: [['grid', 'Grid (px)', 'slider', 0.5, 0.5, 16], ['angleStep', 'Angle step (°, 0 = free)', 'num', 1]],
    help: 'Snaps drawn positions to whole pixels so pixel art never shimmers while moving.',
  },
  StrokeStyle2D: {
    fields: [['dash', 'Dash (px, 0 = solid)', 'slider', 0.5, 0, 200], ['gap', 'Gap (px)', 'slider', 0.5, 0, 200], ['dashOffset', 'Dash offset', 'num', 1], ['trimStart', 'Trim start', 'slider', 0.01, 0, 1], ['trimEnd', 'Trim end', 'slider', 0.01, 0, 1], ['trimOffset', 'Trim offset', 'slider', 0.01, 0, 1]],
    help: 'Dashed strokes and Trim Path. Keyframe Trim End 0 → 1 to draw the line on, or Dash Offset for marching ants.',
  },
  CameraShake2D: {
    fields: [['amplitude', 'Amplitude (px)', 'slider', 0.5, 0, 100], ['frequency', 'Frequency (Hz)', 'slider', 0.5, 0, 60], ['recovery', 'Recovery (s)', 'slider', 0.05, 0.05, 5], ['constant', 'Constant shake', 'slider', 0.01, 0, 1]],
    help: 'Shake from script: ref.shake2(0.5) on this camera (strength adds up, squared — small hits stay small). Playground only.',
  },
  ParticlePath2D: {
    fields: [
      ['weight', 'Weight (pull)', 'slider', 0.01, 0, 1],
      ['joinDelay', 'Join after (life)', 'slider', 0.01, 0, 1],
      ['joinTime', 'Join over (life)', 'slider', 0.01, 0.01, 1],
      ['stray', 'Strays', 'slider', 0.01, 0, 1],
      ['spread', 'Spread (px)', 'slider', 0.5, 0, 300],
      ['widen', 'Widen by end', 'slider', 0.05, 0, 8],
      ['speed', 'Speed (px/s, 0 = over life)', 'slider', 1, 0, 1000],
      ['start', 'Enter at', 'slider', 0.01, 0, 1],
      ['turbulence', 'Turbulence (px)', 'slider', 0.5, 0, 100],
      ['turbulenceSpeed', 'Turbulence speed', 'slider', 0.05, 0, 10],
      ['loopPath', 'Loop round the path', 'toggle'],
    ],
    help: 'Smoke: Join after ~0.1, Join over ~0.4, Weight 0.6, Spread 10, Widen 4, Turbulence 12, Strays 0.15. Draw the guide with the Pen tool and hide it. Weight 1 locks particles to the path.',
  },
  LightFlicker2D: {
    fields: [['amount', 'Amount', 'slider', 0.01, 0, 1], ['speed', 'Speed (Hz)', 'slider', 0.1, 0, 40], ['smoothness', 'Smoothness', 'slider', 0.01, 0, 1], ['radiusAmount', 'Radius breathing', 'slider', 0.01, 0, 1], ['seed', 'Seed', 'int', 0, 9999]],
    help: 'Candle (smooth, slow), torch (medium), failing neon (smoothness 0, fast).',
  },
}

/** Fields for one of the simple components. */
export function SimpleComponentFields({ behavior, onChange }: { behavior: NodeBehavior; onChange: (b: NodeBehavior) => void }) {
  const schema = SIMPLE_COMPONENT_FIELDS[behavior.kind]
  if (!schema) return null
  const values = behavior as unknown as Record<string, unknown>
  const set = (key: string, value: unknown) => onChange({ ...behavior, [key]: value } as NodeBehavior)
  const row = (key: string, label: string, child: React.ReactNode) => (
    <div key={key} className="flex items-center gap-2">
      <span className="w-24 shrink-0 text-[11px] text-ink-dim">{label}</span>
      <div className="flex min-w-0 flex-1 items-center gap-1">{child}</div>
    </div>
  )
  return (
    <div className="flex flex-col gap-1.5">
      {schema.fields.map((spec) => {
        const [key, label, kind] = spec
        const v = values[key]
        switch (kind) {
          case 'color':
            return row(key, label, <ColorSwatch value={v as Rgba} onChange={(c) => set(key, c)} />)
          case 'toggle':
            return (
              <label key={key} className="flex items-center gap-1.5 text-[11px] text-ink-dim">
                <input type="checkbox" checked={!!v} onChange={(e) => set(key, e.target.checked)} />
                {label}
              </label>
            )
          case 'xy': {
            const xy = v as [number, number]
            const step = spec[3]
            return row(
              key,
              label,
              <>
                <NumberField label="X" value={xy[0]} step={step} onCommit={(x) => set(key, [x, xy[1]])} />
                <NumberField label="Y" value={xy[1]} step={step} onCommit={(y) => set(key, [xy[0], y])} />
              </>,
            )
          }
          case 'num':
            return row(key, label, <NumberField label="" value={v as number} step={spec[3]} onCommit={(n) => set(key, n)} />)
          case 'int': {
            const [, , , min, max] = spec as [string, string, 'int', number, number]
            return row(key, label, <NumberField label="" value={v as number} step={1} onCommit={(n) => set(key, Math.round(Math.min(max, Math.max(min, n))))} />)
          }
          case 'slider': {
            const [, , , step, min, max] = spec as [string, string, 'slider', number, number, number]
            return row(
              key,
              label,
              <>
                <div className="min-w-0 flex-1">
                  <Slider value={Math.min(max, Math.max(min, v as number))} min={min} max={max} step={step} onChange={(n) => set(key, n)} />
                </div>
                <span className="w-10 shrink-0 text-right text-[10px] tabular-nums text-ink-dim">{(v as number).toFixed(step < 0.1 ? 2 : 1)}</span>
              </>,
            )
          }
        }
        return null
      })}
      <p className="text-[10px] leading-snug text-ink-dim">{schema.help}</p>
    </div>
  )
}

export interface SortingLayer {
  id: number
  name: string
}

/** The project's sorting layers (refreshes when the document changes). */
export function useSortingLayers(editor: WasmEditor | null, revision: unknown): SortingLayer[] {
  const [layers, setLayers] = useState<SortingLayer[]>([{ id: 0, name: 'Normal' }])
  useEffect(() => {
    if (editor) setLayers(editor.sorting_layers() as SortingLayer[])
  }, [editor, revision])
  return layers
}

/** A node's Layer: a dropdown of the project's sorting layers, plus the
 * layer manager. Stored as a Sorting Layer component (absent = Normal /
 * inherited from the parent). */
export function SortingLayerField({
  editor,
  behaviors,
  onSetBehaviors,
  onDocumentChange,
  revision,
}: {
  editor: WasmEditor | null
  behaviors: NodeBehaviorEntry[]
  onSetBehaviors: (b: NodeBehaviorEntry[]) => void
  onDocumentChange: (doc: DocumentSummary) => void
  revision: unknown
}) {
  const layers = useSortingLayers(editor, revision)
  const [managing, setManaging] = useState(false)
  const own = behaviors.find((b) => b.behavior.kind === 'SortingLayer2D')
  const current = own && own.behavior.kind === 'SortingLayer2D' ? own.behavior.layer : null
  const pick = (value: string) => {
    const rest = behaviors.filter((b) => b.behavior.kind !== 'SortingLayer2D')
    onSetBehaviors(value === 'inherit' ? rest : [...rest, { behavior: { kind: 'SortingLayer2D', layer: Number(value) }, enabled: true }])
  }
  const save = (next: SortingLayer[]) => {
    if (!editor) return
    onDocumentChange(editor.set_sorting_layers(next) as DocumentSummary)
  }
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-2" title="Sorting layer: everything on a later layer draws above everything on an earlier one, whatever the hierarchy. Lights can target layers.">
        <span className="w-20 shrink-0 text-[11px] text-ink-dim">Layer</span>
        <Select value={current === null ? 'inherit' : String(current)} onChange={(e) => pick(e.target.value)} className="min-w-0 flex-1 rounded bg-overlay/5 px-1.5 py-0.5 text-[11px] text-ink">
          <option value="inherit">Normal / parent&apos;s</option>
          {layers.map((l) => (
            <option key={l.id} value={l.id}>
              {l.name}
            </option>
          ))}
        </Select>
        <button type="button" onClick={() => setManaging((m) => !m)} className="rounded bg-overlay/10 px-1.5 py-0.5 text-[10px] text-ink-dim hover:text-ink" title="Add, rename and reorder layers">
          Layers…
        </button>
      </div>
      {managing && (
        <div className="flex flex-col gap-1 rounded-md bg-overlay/5 p-1.5">
          <span className="text-[10px] text-ink-dim">Back to front (later layers draw on top):</span>
          {layers.map((l, i) => (
            <div key={l.id} className="flex items-center gap-1">
              <input
                defaultValue={l.name}
                onBlur={(e) => e.target.value.trim() && e.target.value !== l.name && save(layers.map((x) => (x.id === l.id ? { ...x, name: e.target.value.trim() } : x)))}
                className="min-w-0 flex-1 rounded bg-overlay/5 px-1.5 py-0.5 text-[11px] text-ink"
              />
              <button type="button" disabled={i === 0} onClick={() => save(layers.map((x, j) => (j === i - 1 ? layers[i] : j === i ? layers[i - 1] : x)))} className="px-1 text-[11px] text-ink-dim hover:text-ink disabled:opacity-30" title="Move back">
                ↑
              </button>
              <button type="button" disabled={i === layers.length - 1} onClick={() => save(layers.map((x, j) => (j === i + 1 ? layers[i] : j === i ? layers[i + 1] : x)))} className="px-1 text-[11px] text-ink-dim hover:text-ink disabled:opacity-30" title="Move forward">
                ↓
              </button>
              <button type="button" disabled={l.id === 0} onClick={() => save(layers.filter((x) => x.id !== l.id))} className="px-1 text-[11px] text-ink-dim hover:text-red-400 disabled:opacity-30" title={l.id === 0 ? 'Normal always exists' : 'Remove this layer (its nodes fall back to Normal)'}>
                ×
              </button>
            </div>
          ))}
          <button
            type="button"
            onClick={() => save([...layers, { id: Math.max(0, ...layers.map((l) => l.id)) + 1, name: `Layer ${layers.length}` }])}
            className="rounded bg-overlay/10 px-2 py-0.5 text-[11px] text-ink hover:bg-accent/20 hover:text-accent"
          >
            + Add layer
          </button>
        </div>
      )}
    </div>
  )
}

/** A Light 2D's target layers (Light Layers component; none = all). */
export function LightLayersFields({ editor, layers: selected, onChange, revision }: { editor: WasmEditor | null; layers: number[] | null; onChange: (layers: number[] | null) => void; revision: unknown }) {
  const layers = useSortingLayers(editor, revision)
  const all = selected === null || selected.length === 0
  return (
    <div className="flex flex-col gap-1">
      <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
        <input type="checkbox" checked={all} onChange={(e) => onChange(e.target.checked ? null : [0])} />
        Lights every layer
      </label>
      {!all &&
        layers.map((l) => (
          <label key={l.id} className="ml-4 flex items-center gap-1.5 text-[11px] text-ink-dim">
            <input
              type="checkbox"
              checked={selected!.includes(l.id)}
              onChange={(e) => {
                const next = e.target.checked ? [...selected!, l.id] : selected!.filter((x) => x !== l.id)
                onChange(next.length ? next : [l.id])
              }}
            />
            {l.name}
          </label>
        ))}
    </div>
  )
}
