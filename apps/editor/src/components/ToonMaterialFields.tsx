/**
 * Editor for a **Toon material** asset (`an1mbyt3_core::ToonMaterial`):
 * every setting of VRM's MToon look, plus the painterly **Arcane** style's
 * brush / hatching / grain controls. Used inline in the Inspector (a 3D
 * mesh's material, an imported avatar's material slots) and in the Shader
 * tab.
 *
 * Edits preview live (untracked, so slider drags are smooth — the values
 * go to the GPU per draw, nothing recompiles) and are committed as one
 * undo step a moment after the last change.
 */
import { useEffect, useRef, useState, type ReactNode } from 'react'
import type { DocumentSummary, ImageAssetSummary, OpaqueId, ToonMaterialConfig, ToonStyle } from '../ipc/document'
import { materialToon, setMaterialToon, type WasmEditor } from '../ipc/wasmEditor'

import { ColorSwatch } from './ColorPicker'
import { Dropdown } from './InspectorPanel'
import { Slider } from './Slider'

type Rgb = [number, number, number]

function Row({ label, title, children }: { label: string; title?: string; children: ReactNode }) {
  return (
    <div className="flex items-center gap-2" title={title}>
      <span className="w-24 shrink-0 text-[11px] text-ink-dim">{label}</span>
      <div className="flex min-w-0 flex-1 items-center gap-1">{children}</div>
    </div>
  )
}

function Num({ value, min, max, step, digits = 2, onChange }: { value: number; min: number; max: number; step: number; digits?: number; onChange: (v: number) => void }) {
  return (
    <>
      <div className="min-w-0 flex-1">
        <Slider value={Math.min(max, Math.max(min, value))} min={min} max={max} step={step} onChange={onChange} />
      </div>
      <span className="w-10 shrink-0 text-right text-[10px] tabular-nums text-ink-dim">{value.toFixed(digits)}</span>
    </>
  )
}

function Rgb3({ value, onChange }: { value: Rgb; onChange: (v: Rgb) => void }) {
  return <ColorSwatch value={[value[0], value[1], value[2], 1]} onChange={(c) => onChange([c[0], c[1], c[2]])} />
}

function Group({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-[10px] font-medium uppercase tracking-wide text-ink-dim/80">{title}</span>
      {children}
    </div>
  )
}

/** What switching style changes: Arcane's own colour/brush defaults when
 * turning a plain MToon material painterly (and back). */
function restyle(value: ToonMaterialConfig, style: ToonStyle): ToonMaterialConfig {
  if (style === value.style) return value
  if (style === 'Arcane') {
    return {
      ...value,
      style,
      shade: value.shade.every((c) => c > 0.45) ? [0.38, 0.36, 0.62] : value.shade,
      rim: value.rim.every((c) => c === 0) ? [0.25, 0.75, 0.8] : value.rim,
      rimPower: 3.5,
      outlineWidth: value.outlineWidth > 0 ? value.outlineWidth : 0.012,
      highlight: [0.18, 0.1, 0.02],
      bands: 3,
      brushScale: 0.22,
      brushStrength: 0.4,
      hatch: 0.25,
      grain: 0.2,
    }
  }
  return { ...value, style }
}

export function ToonMaterialFields({
  editor,
  material,
  images,
  revision,
  onDocumentChange,
}: {
  editor: WasmEditor | null
  material: OpaqueId
  images: ImageAssetSummary[]
  /** Anything that changes with the document (re-read after undo / redo). */
  revision: unknown
  onDocumentChange: (doc: DocumentSummary) => void
}) {
  const [value, setValue] = useState<ToonMaterialConfig | null>(null)
  const pending = useRef<{ value: ToonMaterialConfig; timer: number } | null>(null)

  // Commit any pending edit (also when switching material / unmounting).
  const flush = () => {
    const p = pending.current
    if (!p || !editor) return
    window.clearTimeout(p.timer)
    pending.current = null
    const next = setMaterialToon(editor, material, p.value, true)
    if (next) onDocumentChange(next)
  }
  useEffect(() => () => flush(), [material]) // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (!editor || pending.current) return
    setValue(materialToon(editor, material))
  }, [editor, material, revision])

  if (!value) return null

  const update = (patch: Partial<ToonMaterialConfig>) => {
    if (!editor) return
    const next = { ...value, ...patch }
    setValue(next)
    setMaterialToon(editor, material, next, false)
    if (pending.current) window.clearTimeout(pending.current.timer)
    pending.current = { value: next, timer: window.setTimeout(flush, 350) }
  }
  const arcane = value.style === 'Arcane'
  const textureKey = value.texture ? `${value.texture.idx}:${value.texture.version}` : 'model'

  return (
    <div className="flex flex-col gap-2.5" onPointerUp={() => pending.current && window.setTimeout(flush, 0)}>
      <div className="flex gap-0.5 rounded-md bg-overlay/5 p-0.5">
        {(['MToon', 'Arcane'] as const).map((s) => (
          <button
            key={s}
            type="button"
            onClick={() => update(restyle(value, s))}
            className={`flex-1 rounded px-2 py-0.5 text-[11px] ${value.style === s ? 'bg-accent/20 text-accent' : 'text-ink-dim hover:text-ink'}`}
            title={s === 'MToon' ? "VRM's standard anime toon look" : 'Painterly: brushy banded light, cool hatched shadows, warm highlights, grain'}
          >
            {s === 'Arcane' ? 'Arcane (painted)' : 'MToon'}
          </button>
        ))}
      </div>

      <Group title="Colour">
        <Row label="Lit colour" title="Multiplies the texture on the lit side (alpha = opacity)">
          <ColorSwatch value={value.base} onChange={(base) => update({ base })} />
        </Row>
        <Row label="Shade colour" title="Multiplies the texture on the shadowed side">
          <Rgb3 value={value.shade} onChange={(shade) => update({ shade })} />
        </Row>
        {arcane && (
          <Row label="Highlight" title="Added to lit areas (warm highlights)">
            <Rgb3 value={value.highlight} onChange={(highlight) => update({ highlight })} />
          </Row>
        )}
        <Row label="Emission" title="Self-lit colour added on top (glowing eyes, LEDs)">
          <Rgb3 value={value.emission} onChange={(emission) => update({ emission })} />
        </Row>
        <Row label="Texture" title="Replace the mesh's own texture with an imported image">
          <div className="min-w-0 flex-1">
            <Dropdown
              value={textureKey}
              options={['model', ...images.map((i) => `${i.id.idx}:${i.id.version}`)]}
              renderValue={(k) => (k === 'model' ? "Mesh's own" : (images.find((i) => `${i.id.idx}:${i.id.version}` === k)?.name ?? k))}
              renderOption={(k) => (k === 'model' ? "Mesh's own" : (images.find((i) => `${i.id.idx}:${i.id.version}` === k)?.name ?? k))}
              onChange={(k) => update({ texture: images.find((i) => `${i.id.idx}:${i.id.version}` === k)?.id ?? null })}
            />
          </div>
        </Row>
      </Group>

      <Group title="Shading">
        <Row label="Shadow line" title="Moves the light / shadow boundary: left = more shade, right = more light">
          <Num value={value.shift} min={-1} max={1} step={0.01} onChange={(shift) => update({ shift })} />
        </Row>
        <Row label="Hardness" title="0 = soft gradient … 1 = hard cel edge">
          <Num value={value.toony} min={0} max={1} step={0.01} onChange={(toony) => update({ toony })} />
        </Row>
        {arcane && (
          <Row label="Light bands" title="How many flat tones the light falls into">
            <Num value={value.bands} min={2} max={6} step={1} digits={0} onChange={(bands) => update({ bands })} />
          </Row>
        )}
        <Row label="Darkest" title="How dark shadows can get from lighting alone">
          <Num value={value.lightMin} min={0} max={1} step={0.01} onChange={(lightMin) => update({ lightMin })} />
        </Row>
        <Row label="Point lights" title="How much point lights add (0 = sun only)">
          <Num value={value.pointLights} min={0} max={3} step={0.05} onChange={(pointLights) => update({ pointLights })} />
        </Row>
      </Group>

      {arcane && (
        <Group title="Paint">
          <Row label="Brush size" title="Brush-stroke size in metres">
            <Num value={value.brushScale} min={0.01} max={0.6} step={0.005} digits={3} onChange={(brushScale) => update({ brushScale })} />
          </Row>
          <Row label="Brushiness" title="How much the strokes break up the shadow edge">
            <Num value={value.brushStrength} min={0} max={1.2} step={0.01} onChange={(brushStrength) => update({ brushStrength })} />
          </Row>
          <Row label="Hatching" title="Crosshatch lines in the shadows">
            <Num value={value.hatch} min={0} max={1} step={0.01} onChange={(hatch) => update({ hatch })} />
          </Row>
          <Row label="Grain" title="Canvas / paper grain">
            <Num value={value.grain} min={0} max={1} step={0.01} onChange={(grain) => update({ grain })} />
          </Row>
        </Group>
      )}

      <Group title="Rim light">
        <Row label="Colour" title="Edge light colour (black = none)">
          <Rgb3 value={value.rim} onChange={(rim) => update({ rim })} />
        </Row>
        <Row label="Tightness" title="Higher = thinner rim">
          <Num value={value.rimPower} min={0.1} max={12} step={0.1} digits={1} onChange={(rimPower) => update({ rimPower })} />
        </Row>
        <Row label="Lift" title="Spreads the rim across the surface">
          <Num value={value.rimLift} min={0} max={1} step={0.01} onChange={(rimLift) => update({ rimLift })} />
        </Row>
      </Group>

      <Group title="Outline">
        <Row label="Width" title="Ink outline width in the mesh's units (0 = none)">
          <Num value={value.outlineWidth} min={0} max={0.05} step={0.0005} digits={4} onChange={(outlineWidth) => update({ outlineWidth })} />
        </Row>
        <Row label="Colour">
          <Rgb3 value={value.outline} onChange={(outline) => update({ outline })} />
        </Row>
      </Group>

      <Group title="Surface">
        <Row label="Alpha cutoff" title="Cut-out: pixels less opaque than this aren't drawn (0 = off)">
          <Num value={value.alphaCutoff} min={0} max={1} step={0.01} onChange={(alphaCutoff) => update({ alphaCutoff })} />
        </Row>
        <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
          <input type="checkbox" checked={value.doubleSided} onChange={(e) => update({ doubleSided: e.target.checked })} />
          Double-sided (cloth, hair cards)
        </label>
      </Group>
    </div>
  )
}
