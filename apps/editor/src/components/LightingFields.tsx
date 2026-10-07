import type { ReactNode } from 'react'
import type {
  ImageAssetSummary,
  Light2DConfig,
  Light2DType,
  LightBlendStyle,
  LightOverlapOperation,
  NodeSummary,
  ShadowCasterConfig,
  ShadowCastingOption,
  TrailConfig,
} from '../ipc/document'
import type { WasmEditor } from '../ipc/wasmEditor'
import { ColorSwatch } from './ColorPicker'
import { ImageAssetPicker } from './ImageAssetPicker'
import { Dropdown, NumberField } from './InspectorPanel'
import { TrashIcon } from './icons'

/**
 * Inspector sections for the Unity-style 2D lighting and trail kinds —
 * laid out after Unity's own inspectors (Light 2D, Trail Renderer, Shadow
 * Caster 2D) so the fields read the same to anyone coming from Unity:
 * type-specific fields only show for the light type that uses them, and
 * Shadows/Volumetric are checkbox-gated groups like Unity's foldouts.
 *
 * Every edit sends the whole config back (`onChange`), one undo step each,
 * same "whole nested object" convention as the Camera/Particle sections.
 */

const clamp = (v: number, min: number, max: number) => Math.min(max, Math.max(min, v))

/** A labeled row: fixed-width label on the left, control(s) on the right. */
function Row({ label, title, children }: { label: string; title?: string; children: ReactNode }) {
  return (
    <div className="flex items-center gap-2" title={title}>
      <span className="w-24 shrink-0 text-[11px] text-ink-dim">{label}</span>
      <div className="flex min-w-0 flex-1 items-center gap-1">{children}</div>
    </div>
  )
}

function SubHeading({ children }: { children: ReactNode }) {
  return <span className="border-t border-overlay/10 pt-2 text-[10px] uppercase tracking-wide text-ink-dim/70">{children}</span>
}

function Check({ checked, onChange, children }: { checked: boolean; onChange: (value: boolean) => void; children: ReactNode }) {
  return (
    <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      {children}
    </label>
  )
}

const LIGHT_TYPES: Light2DType[] = ['Global', 'Spot', 'Freeform', 'Sprite']
const BLEND_STYLES: LightBlendStyle[] = ['Multiply', 'Additive']
const OVERLAP_OPERATIONS: LightOverlapOperation[] = ['Additive', 'AlphaBlend', 'Max']
const OVERLAP_LABEL: Record<LightOverlapOperation, string> = { Additive: 'Additive', AlphaBlend: 'Alpha Blend', Max: 'Max (merge)' }

/** Unity's `Light 2D` inspector. */
export function Light2DFields({
  config,
  images,
  editor,
  onChange,
  bare = false,
}: {
  config: Light2DConfig
  images: ImageAssetSummary[]
  editor: WasmEditor | null
  onChange: (value: Light2DConfig) => void
  /** Inside a component card: no own title/box. */
  bare?: boolean
}) {
  const update = (patch: Partial<Light2DConfig>) => onChange({ ...config, ...patch })
  const type = config.lightType
  const hasShape = type !== 'Global'

  return (
    <div className={`flex flex-col gap-1.5 ${bare ? '' : 'rounded-md bg-overlay/5 p-2'}`}>
      {!bare && <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">Light 2D</span>}
      <Row label="Light Type">
        <Dropdown value={type} options={LIGHT_TYPES} onChange={(lightType) => update({ lightType })} />
      </Row>
      <Row label="Color">
        <ColorSwatch value={config.color} onChange={(color) => update({ color })} />
      </Row>
      <Row label="Intensity">
        <NumberField label="×" value={config.intensity} step={0.1} onCommit={(intensity) => update({ intensity: Math.max(0, intensity) })} />
      </Row>

      {type === 'Spot' && (
        <>
          <Row label="Radius" title="Inner: full brightness inside this. Outer: light ends here.">
            <NumberField label="in" value={config.innerRadius} step={1} onCommit={(innerRadius) => update({ innerRadius: clamp(innerRadius, 0, config.outerRadius) })} />
            <NumberField label="out" value={config.outerRadius} step={1} onCommit={(outerRadius) => update({ outerRadius: Math.max(1, outerRadius), innerRadius: Math.min(config.innerRadius, Math.max(1, outerRadius)) })} />
          </Row>
          <Row label="Spot Angle" title="Full cone width in degrees, aimed along the node's up (+Y). 360 = point light.">
            <NumberField label="in°" value={config.innerSpotAngle} step={5} onCommit={(innerSpotAngle) => update({ innerSpotAngle: clamp(innerSpotAngle, 0, config.outerSpotAngle) })} />
            <NumberField label="out°" value={config.outerSpotAngle} step={5} onCommit={(outerSpotAngle) => update({ outerSpotAngle: clamp(outerSpotAngle, 0, 360), innerSpotAngle: Math.min(config.innerSpotAngle, clamp(outerSpotAngle, 0, 360)) })} />
          </Row>
        </>
      )}

      {type === 'Freeform' && (
        <>
          <Row label="Falloff" title="How far past the shape's edge the light fades out.">
            <NumberField label="size" value={config.falloffSize} step={1} onCommit={(falloffSize) => update({ falloffSize: Math.max(0, falloffSize) })} />
          </Row>
          <span className="text-[11px] text-ink-dim">Shape Points (node-local)</span>
          {config.shapePoints.map((p, i) => (
            <div key={i} className="flex items-center gap-1">
              <NumberField label="X" value={p.x} onCommit={(x) => update({ shapePoints: config.shapePoints.map((q, j) => (j === i ? { ...q, x } : q)) })} />
              <NumberField label="Y" value={p.y} onCommit={(y) => update({ shapePoints: config.shapePoints.map((q, j) => (j === i ? { ...q, y } : q)) })} />
              <button
                disabled={config.shapePoints.length <= 3}
                onClick={() => update({ shapePoints: config.shapePoints.filter((_, j) => j !== i) })}
                className="shrink-0 rounded bg-red-500/10 px-1.5 py-0.5 text-[10px] text-red-300 hover:bg-red-500/20 disabled:opacity-30"
                title="Remove point (a shape needs at least 3)"
              >
                <TrashIcon />
              </button>
            </div>
          ))}
          <button
            disabled={config.shapePoints.length >= 32}
            onClick={() => {
              // New point halfway along the closing edge (last -> first).
              const a = config.shapePoints[config.shapePoints.length - 1]
              const b = config.shapePoints[0]
              update({ shapePoints: [...config.shapePoints, { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }] })
            }}
            className="self-start rounded-md bg-accent/15 px-2 py-1 text-[11px] font-medium text-accent hover:bg-accent/25 disabled:opacity-30"
          >
            + Point
          </button>
        </>
      )}

      {type === 'Sprite' && (
        <>
          <Row label="Sprite">
            {editor && <ImageAssetPicker editor={editor} images={images} value={config.sprite} onChange={(sprite) => update({ sprite })} />}
          </Row>
          <Row label="Size">
            <NumberField label="W" value={config.spriteWidth} onCommit={(spriteWidth) => update({ spriteWidth: Math.max(1, spriteWidth) })} />
            <NumberField label="H" value={config.spriteHeight} onCommit={(spriteHeight) => update({ spriteHeight: Math.max(1, spriteHeight) })} />
          </Row>
        </>
      )}

      {(type === 'Spot' || type === 'Freeform') && (config.falloffSteps >= 2 || config.edgeBlur > 0) && (
        <Row label="Falloff Strength" title="0 = soft, lingering light; 1 = holds then drops off sharply.">
          <NumberField label="" value={config.falloffStrength} step={0.05} onCommit={(falloffStrength) => update({ falloffStrength: clamp(falloffStrength, 0, 1) })} />
        </Row>
      )}

      <Row label="Blend Style" title="Multiply: light is what makes things visible. Additive: adds light on top.">
        <Dropdown value={config.blendStyle} options={BLEND_STYLES} onChange={(blendStyle) => update({ blendStyle })} />
      </Row>
      <Row label="Overlap" title="Additive: overlapping lights add up. Alpha Blend: this light covers earlier ones (by Light Order). Max: brightest wins — overlapping lights merge instead of stacking.">
        <Dropdown value={config.overlapOperation} options={OVERLAP_OPERATIONS} renderValue={(v) => OVERLAP_LABEL[v]} renderOption={(v) => OVERLAP_LABEL[v]} onChange={(overlapOperation) => update({ overlapOperation })} />
      </Row>
      <Row label="Light Order">
        <NumberField label="#" value={config.lightOrder} step={1} onCommit={(lightOrder) => update({ lightOrder: Math.round(lightOrder) })} />
      </Row>

      {hasShape && (
        <>
          <SubHeading>Falloff Look</SubHeading>
          <Row label="Falloff Steps" title="Crisp brightness rings, brightest at the center and faintest at the edge. 0-1 = one solid pool.">
            <NumberField label="#" value={config.falloffSteps} step={1} onCommit={(v) => update({ falloffSteps: clamp(Math.round(v), 0, 12) })} />
          </Row>
          <Row label="Blur" title="0 = crisp rings (pixel-art). 1 = smooth Unity-style gradient.">
            <NumberField label="" value={config.edgeBlur} step={0.05} onCommit={(v) => update({ edgeBlur: clamp(v, 0, 1) })} />
          </Row>
          <Row label="Pixel Size" title="Compute the light in chunky blocks of this many world units (0 = per pixel).">
            <NumberField label="px" value={config.pixelSize} step={1} onCommit={(v) => update({ pixelSize: clamp(Math.round(v), 0, 32) })} />
          </Row>

          <SubHeading>Shadows</SubHeading>
          <Check checked={config.shadowsEnabled} onChange={(shadowsEnabled) => update({ shadowsEnabled })}>
            Cast shadows from Shadow Caster 2D
          </Check>
          {config.shadowsEnabled && (
            <>
              <Row label="Strength">
                <NumberField label="" value={config.shadowStrength} step={0.05} onCommit={(shadowStrength) => update({ shadowStrength: clamp(shadowStrength, 0, 1) })} />
              </Row>
              <Row label="Softness">
                <NumberField label="" value={config.shadowSoftness} step={0.05} onCommit={(shadowSoftness) => update({ shadowSoftness: clamp(shadowSoftness, 0, 1) })} />
              </Row>
            </>
          )}

          <SubHeading>Volumetric</SubHeading>
          <Check checked={config.volumetricEnabled} onChange={(volumetricEnabled) => update({ volumetricEnabled })}>
            Glow in the air (visible without a surface)
          </Check>
          {config.volumetricEnabled && (
            <>
              <Row label="Intensity">
                <NumberField label="" value={config.volumetricIntensity} step={0.05} onCommit={(volumetricIntensity) => update({ volumetricIntensity: Math.max(0, volumetricIntensity) })} />
              </Row>
              <Check checked={config.volumetricShadowsEnabled} onChange={(volumetricShadowsEnabled) => update({ volumetricShadowsEnabled })}>
                Volumetric shadows
              </Check>
              {config.volumetricShadowsEnabled && (
                <Row label="Shadow Strength">
                  <NumberField label="" value={config.volumetricShadowStrength} step={0.05} onCommit={(volumetricShadowStrength) => update({ volumetricShadowStrength: clamp(volumetricShadowStrength, 0, 1) })} />
                </Row>
              )}
            </>
          )}
        </>
      )}

      <span className="text-[10px] leading-snug text-ink-dim/70">
        {type === 'Global'
          ? 'Lights the whole scene evenly (ambient). Once any light exists, unlit areas go dark — add a Global light for a base level.'
          : 'Mark HUD/UI nodes Unlit (Rendering section) so they ignore lighting.'}
      </span>
    </div>
  )
}

/** Unity's `Trail Renderer` inspector. */
export function TrailFields({
  config,
  images,
  editor,
  onChange,
  bare = false,
}: {
  config: TrailConfig
  images: ImageAssetSummary[]
  editor: WasmEditor | null
  onChange: (value: TrailConfig) => void
  bare?: boolean
}) {
  const update = (patch: Partial<TrailConfig>) => onChange({ ...config, ...patch })
  return (
    <div className={`flex flex-col gap-1.5 ${bare ? '' : 'rounded-md bg-overlay/5 p-2'}`}>
      {!bare && <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">Trail Renderer</span>}
      <Check checked={config.emitting} onChange={(emitting) => update({ emitting })}>
        Emitting
      </Check>
      <Row label="Time" title="Seconds each point lives before dropping off the tail.">
        <NumberField label="s" value={config.time} step={0.05} onCommit={(time) => update({ time: Math.max(0.01, time) })} />
      </Row>
      <Row label="Min Vertex Dist">
        <NumberField label="" value={config.minVertexDistance} step={0.5} onCommit={(minVertexDistance) => update({ minVertexDistance: Math.max(0.1, minVertexDistance) })} />
      </Row>
      <Row label="Width">
        <NumberField label="w" value={config.width} step={1} onCommit={(width) => update({ width: Math.max(0, width) })} />
      </Row>
      <Row label="Corner Vertices">
        <NumberField label="#" value={config.cornerVertices} step={1} onCommit={(v) => update({ cornerVertices: clamp(Math.round(v), 0, 4) })} />
      </Row>
      <Row label="End Cap Vertices">
        <NumberField label="#" value={config.endCapVertices} step={1} onCommit={(v) => update({ endCapVertices: clamp(Math.round(v), 0, 16) })} />
      </Row>
      <Row label="Texture" title="Stretched once from head to tail, tinted by the color gradient.">
        {editor && <ImageAssetPicker editor={editor} images={images} value={config.texture} onChange={(texture) => update({ texture })} />}
      </Row>

      <SubHeading>Width Curve (head → tail)</SubHeading>
      {config.widthCurve.length === 0 && <p className="text-[10px] text-ink-dim">No curve — constant width.</p>}
      {config.widthCurve.map((key, i) => (
        <div key={i} className="flex items-center gap-1">
          <NumberField label="t" value={key.t} step={0.05} onCommit={(t) => update({ widthCurve: config.widthCurve.map((k, j) => (j === i ? { ...k, t: clamp(t, 0, 1) } : k)) })} />
          <NumberField label="×" value={key.sizeMultiplier} step={0.1} onCommit={(m) => update({ widthCurve: config.widthCurve.map((k, j) => (j === i ? { ...k, sizeMultiplier: Math.max(0, m) } : k)) })} />
          <button onClick={() => update({ widthCurve: config.widthCurve.filter((_, j) => j !== i) })} className="shrink-0 rounded bg-red-500/10 px-1.5 py-0.5 text-[10px] text-red-300 hover:bg-red-500/20">
            <TrashIcon />
          </button>
        </div>
      ))}
      <button
        onClick={() => update({ widthCurve: [...config.widthCurve, { t: config.widthCurve.length === 0 ? 0 : 1, sizeMultiplier: 1 }] })}
        className="self-start rounded-md bg-accent/15 px-2 py-1 text-[11px] font-medium text-accent hover:bg-accent/25"
      >
        + Key
      </button>

      <SubHeading>Color (head → tail)</SubHeading>
      {config.colorGradient.length === 0 && <p className="text-[10px] text-ink-dim">No gradient — solid white.</p>}
      {config.colorGradient.map((stop, i) => (
        <div key={i} className="flex items-center gap-1">
          <NumberField label="t" value={stop.offset} step={0.05} onCommit={(offset) => update({ colorGradient: config.colorGradient.map((s, j) => (j === i ? { ...s, offset: clamp(offset, 0, 1) } : s)) })} />
          <ColorSwatch value={stop.color} onChange={(color) => update({ colorGradient: config.colorGradient.map((s, j) => (j === i ? { ...s, color } : s)) })} />
          <button onClick={() => update({ colorGradient: config.colorGradient.filter((_, j) => j !== i) })} className="shrink-0 rounded bg-red-500/10 px-1.5 py-0.5 text-[10px] text-red-300 hover:bg-red-500/20">
            <TrashIcon />
          </button>
        </div>
      ))}
      <button
        onClick={() => update({ colorGradient: [...config.colorGradient, { offset: config.colorGradient.length === 0 ? 0 : 1, color: [1, 1, 1, 1], midpoint: 0.5 }] })}
        className="self-start rounded-md bg-accent/15 px-2 py-1 text-[11px] font-medium text-accent hover:bg-accent/25"
      >
        + Stop
      </button>
      <span className="text-[10px] leading-snug text-ink-dim/70">Parent this under something that moves — the trail follows it.</span>
    </div>
  )
}

const MATERIALS = ['Lit', 'Unlit'] as const
const MATERIAL_LABEL: Record<(typeof MATERIALS)[number], string> = { Lit: 'Sprite-Lit-Default', Unlit: 'Sprite-Unlit-Default' }

/** Unity's sprite Material choice: Sprite-Lit-Default (affected by 2D
 * lights) or Sprite-Unlit-Default (always full brightness — HUD, UI, glowing
 * things). Applies to the node and everything under it. */
export function MaterialField({ unlit, onSetUnlit }: { unlit: boolean; onSetUnlit: (value: boolean) => void }) {
  return (
    <Row label="Material" title="Lit: 2D lights affect it. Unlit: always full brightness (HUD/UI/glow). Children inherit Unlit.">
      <Dropdown value={unlit ? 'Unlit' : 'Lit'} options={[...MATERIALS]} renderValue={(v) => MATERIAL_LABEL[v]} renderOption={(v) => MATERIAL_LABEL[v]} onChange={(v) => onSetUnlit(v === 'Unlit')} />
    </Row>
  )
}

const CASTING_OPTIONS: ShadowCastingOption[] = ['CastShadow', 'SelfShadow', 'CastAndSelfShadow']
const CASTING_LABEL: Record<ShadowCastingOption, string> = {
  CastShadow: 'Cast Shadow',
  SelfShadow: 'Self Shadow',
  CastAndSelfShadow: 'Cast and Self Shadow',
}

/** Rendering options any node can have: Unlit, and the Shadow Caster 2D
 * component (add/remove + its settings). */
export function RenderingFields({
  node,
  onSetUnlit,
  onSetShadowCaster,
}: {
  node: NodeSummary
  onSetUnlit: (value: boolean) => void
  onSetShadowCaster: (value: ShadowCasterConfig | null) => void
}) {
  const caster = node.shadowCaster
  const isLight = node.light2d !== null
  // Lights don't have a material of their own (they *are* the lighting).
  if (isLight) return null
  return (
    <div className="flex flex-col gap-1.5 rounded-md bg-overlay/5 p-2">
      <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">Rendering</span>
      <MaterialField unlit={node.unlit} onSetUnlit={onSetUnlit} />
      {!isLight && (
        <>
          <Check checked={caster !== null} onChange={(on) => onSetShadowCaster(on ? { enabled: true, castingOption: 'CastShadow', trimEdge: 0 } : null)}>
            Shadow Caster 2D
          </Check>
          {caster && (
            <>
              <Row label="Casting">
                <Dropdown value={caster.castingOption} options={CASTING_OPTIONS} renderValue={(v) => CASTING_LABEL[v]} renderOption={(v) => CASTING_LABEL[v]} onChange={(castingOption) => onSetShadowCaster({ ...caster, castingOption })} />
              </Row>
              <Row label="Trim Edge" title="Shrinks the caster outline inward so shadows start slightly inside the silhouette.">
                <NumberField label="" value={caster.trimEdge} step={0.5} onCommit={(trimEdge) => onSetShadowCaster({ ...caster, trimEdge: Math.max(0, trimEdge) })} />
              </Row>
              <span className="text-[10px] leading-snug text-ink-dim/70">Shape: a Shape's own outline; anything else uses its bounds box. Lights need Shadows on.</span>
            </>
          )}
        </>
      )}
    </div>
  )
}
