/**
 * Inspector fields for **Goo 2D** (`an1mbyt3_core::Goo2D`): any node —
 * shape, text, sprite, particle system — as part of a gooey liquid group.
 * Everything with the same group number melts together; the group is drawn
 * with its first member's look (body + rim, each a colour or a gradient).
 */
import { useState, type ReactNode } from 'react'
import type { GradientDef, Goo2DConfig } from '../ipc/document'
import { ColorSwatch } from './ColorPicker'
import { Dropdown, GradientStopList, GRADIENT_KINDS, NumberField } from './InspectorPanel'
import { Slider } from './Slider'

type Rgba = [number, number, number, number]

function Row({ label, title, children }: { label: string; title?: string; children: ReactNode }) {
  return (
    <div className="flex items-center gap-2" title={title}>
      <span className="w-24 shrink-0 text-[11px] text-ink-dim">{label}</span>
      <div className="flex min-w-0 flex-1 items-center gap-1">{children}</div>
    </div>
  )
}

function SliderNum({ value, min, max, step, onChange, digits = 1 }: { value: number; min: number; max: number; step: number; onChange: (v: number) => void; digits?: number }) {
  return (
    <>
      <div className="min-w-0 flex-1">
        <Slider value={Math.min(max, Math.max(min, value))} min={min} max={max} step={step} onChange={onChange} />
      </div>
      <span className="w-10 shrink-0 text-right text-[10px] tabular-nums text-ink-dim">{value.toFixed(digits)}</span>
    </>
  )
}

/** A preview of a gradient as CSS (linear colours → display sRGB). */
function gradientCss(gradient: GradientDef): string {
  const stop = (c: number[]) => `rgba(${c.slice(0, 3).map((v) => Math.round(Math.pow(Math.max(0, v), 1 / 2.2) * 255)).join(',')},${c[3]})`
  return `linear-gradient(90deg, ${gradient.stops.map((s) => `${stop(s.color)} ${Math.round(s.offset * 100)}%`).join(', ')})`
}

/** A colour that can switch to a gradient (in the group owner's space). */
function PaintField({ label, color, gradient, onColor, onGradient }: { label: string; color: Rgba; gradient: GradientDef | null; onColor: (c: Rgba) => void; onGradient: (g: GradientDef | null) => void }) {
  const [open, setOpen] = useState(false)
  const makeGradient = (): GradientDef => ({
    kind: 'Linear',
    start: { x: -120, y: 0 },
    end: { x: 120, y: 0 },
    stops: [
      { offset: 0, color, midpoint: 0.5 },
      // A second stop with the channels rotated: an instant, visible pairing to tweak.
      { offset: 1, color: [color[2], color[0], color[1], color[3]], midpoint: 0.5 },
    ],
  })
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-2">
        <span className="w-24 shrink-0 text-[11px] text-ink-dim">{label}</span>
        <div className="flex gap-0.5 rounded-md bg-overlay/5 p-0.5">
          <button type="button" onClick={() => onGradient(null)} className={`rounded px-2 py-0.5 text-[10px] ${!gradient ? 'bg-accent/20 text-accent' : 'text-ink-dim hover:text-ink'}`}>
            Solid
          </button>
          <button
            type="button"
            onClick={() => {
              if (!gradient) onGradient(makeGradient())
              setOpen(true)
            }}
            className={`rounded px-2 py-0.5 text-[10px] ${gradient ? 'bg-accent/20 text-accent' : 'text-ink-dim hover:text-ink'}`}
          >
            Gradient
          </button>
        </div>
        {gradient && <button type="button" onClick={() => setOpen((o) => !o)} className="ml-auto h-4 w-12 rounded border border-panel-border" style={{ background: gradientCss(gradient) }} title="Edit gradient" />}
      </div>
      {!gradient && <ColorSwatch value={color} onChange={onColor} />}
      {gradient && open && (
        <div className="flex flex-col gap-1 rounded-md bg-overlay/5 p-1.5">
          <Dropdown value={gradient.kind} options={GRADIENT_KINDS} onChange={(kind) => onGradient({ ...gradient, kind })} />
          <div className="flex items-center gap-1 text-[10px] text-ink-dim">
            From
            <NumberField label="X" value={gradient.start.x} onCommit={(x) => onGradient({ ...gradient, start: { ...gradient.start, x } })} />
            <NumberField label="Y" value={gradient.start.y} onCommit={(y) => onGradient({ ...gradient, start: { ...gradient.start, y } })} />
          </div>
          <div className="flex items-center gap-1 text-[10px] text-ink-dim">
            To
            <NumberField label="X" value={gradient.end.x} onCommit={(x) => onGradient({ ...gradient, end: { ...gradient.end, x } })} />
            <NumberField label="Y" value={gradient.end.y} onCommit={(y) => onGradient({ ...gradient, end: { ...gradient.end, y } })} />
          </div>
          <span className="text-[10px] text-ink-dim">Points are in the group's first member's own space.</span>
          <GradientStopList gradient={gradient} onChange={onGradient} />
        </div>
      )}
    </div>
  )
}

/**
 * Edge quality for GPU-drawn liquids. Off (the default): hard edges — a
 * pixel is liquid or it isn't, so low-resolution and pixel-art scenes stay
 * crisp. On: an anti-aliased ramp `softness` screen pixels wide. Stored as
 * the node's Antialias 2D component.
 */
export function AntialiasFields({ value, onChange, standalone = false }: { value: number | null; onChange: (softness: number | null) => void; standalone?: boolean }) {
  return (
    <div className="flex flex-col gap-1">
      {!standalone && (
        <label className="flex items-center gap-1.5 text-[11px] text-ink-dim" title="Off: hard, pixel-crisp edges (best for pixel art and low resolutions). On: smooth anti-aliased edges.">
          <input type="checkbox" checked={value !== null} onChange={(e) => onChange(e.target.checked ? 1 : null)} />
          Antialias edges
        </label>
      )}
      {value !== null && (
        <Row label="Softness (px)" title="Width of the smooth edge in screen pixels: 1 = standard antialiasing, more = softer.">
          <SliderNum value={value} min={0.25} max={6} step={0.05} digits={2} onChange={(v) => onChange(v)} />
        </Row>
      )}
      {standalone && <p className="text-[10px] leading-snug text-ink-dim">Smooths this node&apos;s liquid edges (its Goo 2D group or Particle Fluid 2D). Remove for hard, pixel-crisp edges.</p>}
    </div>
  )
}

export function Goo2DFields({ config, onChange, antialias = null, onAntialias }: { config: Goo2DConfig; onChange: (c: Goo2DConfig) => void; antialias?: number | null; onAntialias?: (softness: number | null) => void }) {
  const update = (patch: Partial<Goo2DConfig>) => onChange({ ...config, ...patch })
  return (
    <div className="flex flex-col gap-1.5">
      <p className="text-[10px] leading-snug text-ink-dim">
        Shapes, text, sprites and particles with the same group melt into one liquid: each keeps its outline alone and grows a smooth neck to its neighbours as they come within reach. The group is drawn with its first member&apos;s look.
      </p>
      <Row label="Group" title="Members with the same number are one liquid.">
        <NumberField label="#" value={config.group} onCommit={(v) => update({ group: Math.max(0, Math.round(v)) })} />
      </Row>
      <PaintField label="Fill" color={config.color} gradient={config.gradient} onColor={(color) => update({ color })} onGradient={(gradient) => update({ gradient })} />
      <PaintField label="Rim" color={config.edgeColor} gradient={config.edgeGradient} onColor={(edgeColor) => update({ edgeColor })} onGradient={(edgeGradient) => update({ edgeGradient })} />
      <Row label="Rim width">
        <SliderNum value={config.edgeWidth} min={0} max={30} step={0.5} onChange={(edgeWidth) => update({ edgeWidth })} />
      </Row>
      <Row label="Reach" title="How far outside its outline a member starts pulling on neighbours (bigger = joins from further, longer necks).">
        <SliderNum value={config.reach} min={2} max={120} step={1} digits={0} onChange={(reach) => update({ reach })} />
      </Row>
      <Row label="Gloss">
        <SliderNum value={config.gloss} min={0} max={1} step={0.01} digits={2} onChange={(gloss) => update({ gloss })} />
      </Row>
      <Row label="Smoothness" title="Outline detail: lower = smoother, more work per frame.">
        <SliderNum value={config.detail} min={0.08} max={0.6} step={0.01} digits={2} onChange={(detail) => update({ detail })} />
      </Row>
      <label className="flex items-center gap-1.5 text-[11px] text-ink-dim" title="Off: the node still draws normally on top of the goo (e.g. a label inside a blob).">
        <input type="checkbox" checked={config.hideSource} onChange={(e) => update({ hideSource: e.target.checked })} />
        Hide the original (the goo replaces it)
      </label>
      {onAntialias && <AntialiasFields value={antialias} onChange={onAntialias} />}
    </div>
  )
}
