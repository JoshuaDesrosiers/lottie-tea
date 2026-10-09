/**
 * Inspector fields for **Particle System 3D** (`an1mbyt3_core::ParticleSystem3D`,
 * simulated by `crates/an1mbyt3-wasm/src/particles3d.rs`).
 *
 * Grouped the way an author thinks about an effect: Emission (how many,
 * how long), Shape (where they appear), Motion (the orbit model: swirl,
 * tilt, pull, rise, drift), Look (size / colour over life, glow vs smoke,
 * streaks) and **Behaviours** — a stack of Converge / Attract / Repel /
 * Drag, each with its own strength curve over particle life (`CurveField`)
 * and a target point in Local (follows the emitter) or World space.
 *
 * Every edit replaces the component (one undo step); the simulation reads
 * the document each frame, so changes show live in Design.
 */
import { useEffect, useRef, useState, type ReactNode } from 'react'
import type { LifeCurve, ParticleBehavior3D, ParticleBehaviorKind3D, ParticleSystem3DConfig } from '../ipc/document'
import { ColorSwatch } from './ColorPicker'
import { NumberField } from './InspectorPanel'
import { Slider } from './Slider'
import { PlusIcon, TrashIcon } from './icons'

function Row({ label, title, children }: { label: string; title?: string; children: ReactNode }) {
  return (
    <div className="flex items-center gap-2" title={title}>
      <span className="w-24 shrink-0 text-[11px] text-ink-dim">{label}</span>
      <div className="flex min-w-0 flex-1 items-center gap-1">{children}</div>
    </div>
  )
}

/** A label above full-width content (XYZ fields, colour stops). */
function Stack({ label, title, children }: { label: string; title?: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1" title={title}>
      <span className="text-[11px] text-ink-dim">{label}</span>
      <div className="flex min-w-0 items-center gap-1">{children}</div>
    </div>
  )
}

/** A slider with its value printed beside it (for small decimals that
 * NumberField's one decimal place would round away). */
function SliderNum({ value, min, max, step, onChange, digits = 2 }: { value: number; min: number; max: number; step: number; onChange: (v: number) => void; digits?: number }) {
  return (
    <>
      <div className="min-w-0 flex-1">
        <Slider value={Math.min(max, Math.max(min, value))} min={min} max={max} step={step} onChange={onChange} />
      </div>
      <span className="w-10 shrink-0 text-right text-[10px] tabular-nums text-ink-dim">{value.toFixed(digits)}</span>
    </>
  )
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5 border-t border-panel-border/50 pt-1.5">
      <span className="text-[10px] font-medium uppercase tracking-wide text-ink-dim">{title}</span>
      {children}
    </div>
  )
}

function Segmented<T extends string>({ value, options, onChange }: { value: T; options: readonly T[]; onChange: (v: T) => void }) {
  return (
    <div className="flex flex-1 overflow-hidden rounded bg-overlay/10 text-[11px]">
      {options.map((o) => (
        <button key={o} type="button" onClick={() => onChange(o)} className={`flex-1 px-1.5 py-1 ${value === o ? 'bg-accent/30 text-ink' : 'text-ink-dim hover:text-ink'}`}>
          {o}
        </button>
      ))}
    </div>
  )
}

// ---- the curve editor ----

const CURVE_PRESETS: { label: string; title: string; points: [number, number][] }[] = [
  { label: 'Ramp in', title: 'None at birth → full at death', points: [[0, 0], [1, 1]] },
  { label: 'Late', title: 'Free for the first 40 %, then ramps to full', points: [[0, 0], [0.4, 0], [1, 1]] },
  { label: 'Ramp out', title: 'Full at birth → none at death (start gathered, then spread)', points: [[0, 1], [1, 0]] },
  { label: 'Pulse', title: 'Builds up mid-life, then lets go', points: [[0, 0], [0.5, 1], [1, 0]] },
  { label: 'Constant', title: 'The same all life', points: [[0, 1], [1, 1]] },
]

const CW = 240
const CH = 70
const PAD = 6

/**
 * A strength-over-life curve: drag points; double-click empty space to
 * add one, double-click a point to remove it (two always stay). X is the
 * particle's life (birth → death), Y the strength multiplier (0 → 1).
 */
export function CurveField({ curve, onChange }: { curve: LifeCurve; onChange: (c: LifeCurve) => void }) {
  const [points, setPoints] = useState<[number, number][]>(curve.points)
  const dragging = useRef<number | null>(null)
  const svg = useRef<SVGSVGElement>(null)
  useEffect(() => {
    if (dragging.current === null) setPoints(curve.points)
  }, [curve])

  const sorted = [...points].map((p, i) => ({ p, i })).sort((a, b) => a.p[0] - b.p[0])
  const toX = (t: number) => PAD + t * (CW - PAD * 2)
  const toY = (v: number) => CH - PAD - v * (CH - PAD * 2)
  const fromEvent = (e: { clientX: number; clientY: number }): [number, number] => {
    const r = svg.current!.getBoundingClientRect()
    const t = ((e.clientX - r.left) / r.width) * CW
    const v = ((e.clientY - r.top) / r.height) * CH
    return [Math.min(1, Math.max(0, (t - PAD) / (CW - PAD * 2))), Math.min(1, Math.max(0, (CH - PAD - v) / (CH - PAD * 2)))]
  }
  const round = (p: [number, number]): [number, number] => [Math.round(p[0] * 100) / 100, Math.round(p[1] * 100) / 100]
  const path = sorted.map(({ p }, k) => `${k ? 'L' : 'M'}${toX(p[0]).toFixed(1)},${toY(p[1]).toFixed(1)}`).join(' ')
  const first = sorted[0]?.p
  const last = sorted[sorted.length - 1]?.p
  // Held flat past the ends, like the engine evaluates it.
  const fullPath = first && last ? `M${toX(0)},${toY(first[1])} L${toX(first[0])},${toY(first[1])} ${path.replace(/^M/, 'L')} L${toX(1)},${toY(last[1])}` : ''

  return (
    <div className="flex flex-col gap-1">
      <svg
        ref={svg}
        viewBox={`0 0 ${CW} ${CH}`}
        className="h-[70px] w-full cursor-crosshair touch-none rounded bg-overlay/10"
        onDoubleClick={(e) => {
          if ((e.target as Element).tagName === 'circle') return
          const next = [...points, round(fromEvent(e))]
          setPoints(next)
          onChange({ points: next })
        }}
        onPointerMove={(e) => {
          const i = dragging.current
          if (i === null) return
          const next = points.map((p, k) => (k === i ? round(fromEvent(e)) : p))
          setPoints(next)
        }}
        onPointerUp={() => {
          if (dragging.current !== null) {
            dragging.current = null
            onChange({ points })
          }
        }}
      >
        {[0.25, 0.5, 0.75].map((g) => (
          <line key={g} x1={toX(g)} x2={toX(g)} y1={PAD} y2={CH - PAD} stroke="currentColor" className="text-ink-dim/20" strokeWidth={0.5} />
        ))}
        <line x1={PAD} x2={CW - PAD} y1={toY(0)} y2={toY(0)} stroke="currentColor" className="text-ink-dim/30" strokeWidth={0.5} />
        <path d={fullPath} fill="none" stroke="rgb(var(--accent-rgb))" strokeWidth={1.6} />
        {points.map((p, i) => (
          <circle
            key={i}
            cx={toX(p[0])}
            cy={toY(p[1])}
            r={4}
            className="cursor-grab fill-panel"
            stroke="rgb(var(--accent-rgb))"
            strokeWidth={1.5}
            onPointerDown={(e) => {
              e.stopPropagation()
              ;(e.currentTarget.ownerSVGElement as SVGSVGElement).setPointerCapture(e.pointerId)
              dragging.current = i
            }}
            onDoubleClick={(e) => {
              e.stopPropagation()
              if (points.length <= 2) return
              const next = points.filter((_, k) => k !== i)
              setPoints(next)
              onChange({ points: next })
            }}
          >
            <title>{`life ${Math.round(p[0] * 100)}% → ${p[1].toFixed(2)}`}</title>
          </circle>
        ))}
      </svg>
      <div className="flex flex-wrap gap-1">
        {CURVE_PRESETS.map((preset) => (
          <button
            key={preset.label}
            type="button"
            title={preset.title}
            onClick={() => {
              setPoints(preset.points)
              onChange({ points: preset.points })
            }}
            className="rounded bg-overlay/10 px-1.5 py-0.5 text-[10px] text-ink-dim hover:text-ink"
          >
            {preset.label}
          </button>
        ))}
      </div>
      <span className="text-[10px] leading-snug text-ink-dim">Across: particle life (birth → death). Up: strength. Drag points · double-click to add / remove.</span>
    </div>
  )
}

// ---- behaviours ----

const BEHAVIOR_KINDS: { kind: ParticleBehaviorKind3D; label: string; help: string; strength: [number, number, number] }[] = [
  { kind: 'Converge', label: 'Converge', help: 'Moves particles onto the point: at 1 they sit exactly on it. Curve "Late" = fly free, then gather; "Ramp out" = start gathered, then spread.', strength: [0, 1, 0.01] },
  { kind: 'Attract', label: 'Attract', help: 'Pulls particles towards the point (m/s²) — they overshoot and swing back, like gravity.', strength: [0, 30, 0.1] },
  { kind: 'Repel', label: 'Repel', help: 'Pushes particles away from the point (m/s²).', strength: [0, 30, 0.1] },
  { kind: 'Drag', label: 'Drag', help: 'Slows particles down over time (their swirl and any push).', strength: [0, 10, 0.05] },
]

const newBehavior = (kind: ParticleBehaviorKind3D): ParticleBehavior3D => ({
  kind,
  enabled: true,
  strength: kind === 'Converge' ? 1 : kind === 'Drag' ? 2 : 6,
  curve: { points: kind === 'Converge' ? [[0, 0], [0.4, 0], [1, 1]] : [[0, 1], [1, 1]] },
  point: [0, 0, 0],
  space: 'Local',
})

function BehaviorCard({ value, onChange, onRemove }: { value: ParticleBehavior3D; onChange: (b: ParticleBehavior3D) => void; onRemove: () => void }) {
  const info = BEHAVIOR_KINDS.find((k) => k.kind === value.kind) ?? BEHAVIOR_KINDS[0]
  const update = (patch: Partial<ParticleBehavior3D>) => onChange({ ...value, ...patch })
  const usesPoint = value.kind !== 'Drag'
  return (
    <div className={`flex flex-col gap-1.5 rounded-md bg-overlay/5 p-1.5 ${value.enabled ? '' : 'opacity-60'}`}>
      <div className="flex items-center gap-1.5">
        <input type="checkbox" checked={value.enabled} onChange={(e) => update({ enabled: e.target.checked })} title="On / off" />
        <select value={value.kind} onChange={(e) => update({ kind: e.target.value as ParticleBehaviorKind3D })} className="min-w-0 flex-1 rounded bg-overlay/10 px-1.5 py-1 text-[11px] text-ink outline-none">
          {BEHAVIOR_KINDS.map((k) => (
            <option key={k.kind} value={k.kind}>
              {k.label}
            </option>
          ))}
        </select>
        <button type="button" onClick={onRemove} title="Remove behaviour" className="rounded p-1 text-ink-dim hover:bg-overlay/10 hover:text-ink">
          <TrashIcon />
        </button>
      </div>
      <p className="text-[10px] leading-snug text-ink-dim">{info.help}</p>
      <Row label="Strength">
        <SliderNum value={value.strength} min={info.strength[0]} max={info.strength[1]} step={info.strength[2]} onChange={(strength) => update({ strength })} />
      </Row>
      <span className="text-[11px] text-ink-dim">Strength over life</span>
      <CurveField curve={value.curve} onChange={(curve) => update({ curve })} />
      {usesPoint && (
        <>
          <Row label="Point space" title="Local: relative to the emitter — moves, turns and scales with it. World: a fixed spot in the scene.">
            <Segmented value={value.space} options={['Local', 'World'] as const} onChange={(space) => update({ space })} />
          </Row>
          <Stack label={value.space === 'Local' ? 'Point (local, relative to the emitter)' : 'Point (world)'}>
            {[0, 1, 2].map((axis) => (
              <NumberField
                key={axis}
                label={'XYZ'[axis]}
                step={0.1}
                value={value.point[axis]}
                onCommit={(v) => update({ point: value.point.map((p, k) => (k === axis ? v : p)) as [number, number, number] })}
              />
            ))}
          </Stack>
        </>
      )}
    </div>
  )
}

// ---- the component ----

export function ParticleSystem3DFields({ config, onChange, onBurst }: { config: ParticleSystem3DConfig; onChange: (c: ParticleSystem3DConfig) => void; onBurst?: () => void }) {
  const update = (patch: Partial<ParticleSystem3DConfig>) => onChange({ ...config, ...patch })
  const [adding, setAdding] = useState(false)
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center gap-2">
        <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
          <input type="checkbox" checked={config.emitting} onChange={(e) => update({ emitting: e.target.checked })} />
          Emitting
        </label>
        {onBurst && (
          <button type="button" onClick={onBurst} className="ml-auto rounded bg-overlay/10 px-2 py-0.5 text-[10px] text-ink-dim hover:text-ink" title="Preview a burst (scripts: ref.burst3(n))">
            Burst ✦
          </button>
        )}
      </div>

      <Section title="Emission">
        <Row label="Rate / s">
          <SliderNum value={config.rate} min={0} max={600} step={1} digits={0} onChange={(rate) => update({ rate })} />
        </Row>
        <Row label="Burst on play">
          <NumberField label="#" value={config.burst} onCommit={(v) => update({ burst: Math.max(0, Math.round(v)) })} />
          <NumberField label="Max" value={config.maxParticles} step={50} onCommit={(v) => update({ maxParticles: Math.max(1, Math.round(v)) })} />
        </Row>
        <Row label="Lifetime (s)">
          <SliderNum value={config.lifetime} min={0.05} max={8} step={0.05} onChange={(lifetime) => update({ lifetime })} />
        </Row>
        <Row label="Lifetime ±">
          <SliderNum value={config.lifetimeJitter} min={0} max={0.95} step={0.05} onChange={(lifetimeJitter) => update({ lifetimeJitter })} />
        </Row>
      </Section>

      <Section title="Shape">
        <Row label="Shape" title="Point: one spot. Sphere: inside a ball. Shell: on a ball's surface. Ring: round a ring (its own orbit band).">
          <Segmented value={config.shape} options={['Point', 'Sphere', 'Shell', 'Ring'] as const} onChange={(shape) => update({ shape })} />
        </Row>
        <Row label="Radius (m)">
          <SliderNum value={config.radius} min={0} max={10} step={0.05} onChange={(radius) => update({ radius })} />
        </Row>
        <Row label="Thickness">
          <SliderNum value={config.thickness} min={0} max={3} step={0.01} onChange={(thickness) => update({ thickness })} />
        </Row>
      </Section>

      <Section title="Motion">
        <Row label="Swirl (°/s)" title="Orbit speed round the node's Y axis (negative = the other way).">
          <SliderNum value={config.swirl} min={-720} max={720} step={5} digits={0} onChange={(swirl) => update({ swirl })} />
        </Row>
        <Row label="Swirl ±">
          <SliderNum value={config.swirlJitter} min={0} max={1} step={0.05} onChange={(swirlJitter) => update({ swirlJitter })} />
        </Row>
        <Row label="Tilt (°)" title="Random tilt of each particle's orbit: 0 = one flat band, 90 = every direction (a ball of orbits).">
          <SliderNum value={config.tilt} min={0} max={90} step={1} digits={0} onChange={(tilt) => update({ tilt })} />
        </Row>
        <Row label="Pull" title="Spring back to the emitter radius: 0 = none, 3–6 = a tight, thick band.">
          <SliderNum value={config.pull} min={0} max={12} step={0.1} onChange={(pull) => update({ pull })} />
        </Row>
        <Row label="Outward (m/s)">
          <SliderNum value={config.speed} min={-5} max={10} step={0.05} onChange={(speed) => update({ speed })} />
        </Row>
        <Row label="Rise (m/s)">
          <SliderNum value={config.rise} min={-5} max={5} step={0.05} onChange={(rise) => update({ rise })} />
        </Row>
        <Stack label="Drift (m/s)">
          {[0, 1, 2].map((axis) => (
            <NumberField key={axis} label={'XYZ'[axis]} step={0.1} value={config.velocity[axis]} onCommit={(v) => update({ velocity: config.velocity.map((p, k) => (k === axis ? v : p)) as [number, number, number] })} />
          ))}
        </Stack>
        <Row label="Gravity">
          <SliderNum value={config.gravity} min={-10} max={20} step={0.1} onChange={(gravity) => update({ gravity })} />
        </Row>
        <Row label="Turbulence">
          <SliderNum value={config.turbulence} min={0} max={3} step={0.01} onChange={(turbulence) => update({ turbulence })} />
        </Row>
        <label className="flex items-center gap-1.5 text-[11px] text-ink-dim" title="Off: particles stay where they were born when the node moves (trails).">
          <input type="checkbox" checked={config.localSpace} onChange={(e) => update({ localSpace: e.target.checked })} />
          Move with the node (local space)
        </label>
      </Section>

      <Section title="Look">
        <Row label="Blend" title="Additive: adds light (glow, magic, fire). Alpha: covers what's behind (smoke, ink).">
          <Segmented value={config.blend} options={['Additive', 'Alpha'] as const} onChange={(blend) => update({ blend })} />
        </Row>
        <Row label="Size start → end">
          <NumberField label="" step={0.05} value={config.sizeStart} onCommit={(v) => update({ sizeStart: Math.max(0, v) })} />
          <NumberField label="→" step={0.05} value={config.sizeEnd} onCommit={(v) => update({ sizeEnd: Math.max(0, v) })} />
        </Row>
        <Row label="Colour: birth" title="Colour over life: birth → mid-life → death (alpha fades).">
          <ColorSwatch value={config.colorStart} onChange={(colorStart) => update({ colorStart })} />
        </Row>
        <Row label="Colour: mid-life">
          <ColorSwatch value={config.colorMid} onChange={(colorMid) => update({ colorMid })} />
        </Row>
        <Row label="Colour: death">
          <ColorSwatch value={config.colorEnd} onChange={(colorEnd) => update({ colorEnd })} />
        </Row>
        <Row label="Intensity">
          <SliderNum value={config.intensity} min={0} max={4} step={0.05} onChange={(intensity) => update({ intensity })} />
        </Row>
        <Row label="Hardness" title="0: soft glowing blobs. 1: crisp, solid shapes — ink, droplets, tendrils.">
          <SliderNum value={config.hardness} min={0} max={1} step={0.01} onChange={(hardness) => update({ hardness })} />
        </Row>
        <Row label="Trail segments" title="Ribbon trails: each particle drags a tapering band along its path (0 = off). Size becomes the ribbon's width.">
          <SliderNum value={config.trailSegments} min={0} max={40} step={1} digits={0} onChange={(trailSegments) => update({ trailSegments: Math.round(trailSegments) })} />
        </Row>
        {config.trailSegments > 0 && (
          <Row label="Trail length (s)">
            <SliderNum value={config.trailTime} min={0.05} max={2} step={0.01} onChange={(trailTime) => update({ trailTime })} />
          </Row>
        )}
        <Row label="Stretch" title="Smears particles along their motion into streaks (seconds of travel).">
          <SliderNum value={config.stretch} min={0} max={0.5} step={0.005} onChange={(stretch) => update({ stretch })} />
        </Row>
        <Row label="Seed">
          <NumberField label="#" value={config.seed} onCommit={(v) => update({ seed: Math.max(0, Math.round(v)) })} />
        </Row>
      </Section>

      <Section title="Fluid">
        <label className="flex items-center gap-1.5 text-[11px] text-ink-dim" title="Draw every particle (and ribbon) of this emitter as one liquid surface: they merge where they touch — ink, slime, water, lava.">
          <input type="checkbox" checked={config.fluid} onChange={(e) => update({ fluid: e.target.checked })} />
          Liquid surface (particles merge)
        </label>
        {config.fluid && (
          <>
            <p className="text-[10px] leading-snug text-ink-dim">The liquid is coloured by "Colour: birth"; each particle's alpha over life is how much liquid it adds.</p>
            <Row label="Threshold" title="Lower: fatter, more merged liquid. Higher: thinner, separate drops.">
              <SliderNum value={config.fluidThreshold} min={0.05} max={1.5} step={0.01} onChange={(fluidThreshold) => update({ fluidThreshold })} />
            </Row>
            <Row label="Gloss">
              <SliderNum value={config.fluidGloss} min={0} max={2} step={0.05} onChange={(fluidGloss) => update({ fluidGloss })} />
            </Row>
            <Row label="Rim colour" title="Where the liquid's surface turns away (alpha = strength).">
              <ColorSwatch value={config.fluidEdge} onChange={(fluidEdge) => update({ fluidEdge })} />
            </Row>
          </>
        )}
      </Section>

      <Section title="Behaviours">
        {config.behaviors.length === 0 && <p className="text-[10px] text-ink-dim">Converge, attract, repel or slow particles — each with its own strength curve over their life.</p>}
        {config.behaviors.map((b, i) => (
          <BehaviorCard
            key={i}
            value={b}
            onChange={(next) => update({ behaviors: config.behaviors.map((x, k) => (k === i ? next : x)) })}
            onRemove={() => update({ behaviors: config.behaviors.filter((_, k) => k !== i) })}
          />
        ))}
        {adding ? (
          <div className="flex flex-wrap gap-1">
            {BEHAVIOR_KINDS.map((k) => (
              <button
                key={k.kind}
                type="button"
                title={k.help}
                onClick={() => {
                  setAdding(false)
                  update({ behaviors: [...config.behaviors, newBehavior(k.kind)] })
                }}
                className="rounded bg-overlay/10 px-2 py-1 text-[11px] text-ink hover:bg-accent/20"
              >
                {k.label}
              </button>
            ))}
          </div>
        ) : (
          <button type="button" onClick={() => setAdding(true)} className="flex items-center justify-center gap-1 rounded bg-overlay/10 px-2 py-1 text-[11px] text-ink-dim hover:text-ink">
            <PlusIcon /> Add behaviour
          </button>
        )}
      </Section>
    </div>
  )
}
