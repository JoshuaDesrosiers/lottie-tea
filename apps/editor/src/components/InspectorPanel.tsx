import { Slider } from './Slider'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { Node3DFields } from './Node3DFields'
import type { ScreenSettings,
  AudioSummary,
  TextFileSummary,
  DocumentSummary,
  FontSummary,
  PropertySummary,
  Node3D,
  MaterialSummary,
  ArtboardSummary,
  BlendMode,
  CameraConfig,
  ComponentDefinitionSummary,
  ComponentKind,
  EmitterShape,
  GradientDef,
  GradientKind,
  ImageAssetSummary,
  ImageProcessing,
  Light2DConfig,
  NodeBehavior,
  NodeBehaviorEntry,
  NodeSummary,
  OpaqueId,
  ParticleEmission,
  ParticleMain,
  ParticleRange,
  ParticleSystemConfig,
  PixelCropFrame,
  PixelGridSnapping,
  PostEffect,
  ScriptDefinitionSummary,
  ScriptFieldDef,
  ScriptFieldKind,
  ScriptFieldValue,
  ShadowCasterConfig,
  PathCommand,
  Stroke,
  StrokeCap,
  StrokeJoin,
  TilemapConfig,
  TrailConfig,
} from '../ipc/document'
import { POST_EFFECT_DEFAULTS } from '../ipc/document'
import type { WasmEditor } from '../ipc/wasmEditor'
import { ColorSwatch } from './ColorPicker'
import { EFFECT_KINDS, EffectParams, VignetteFields } from './EffectsPanel'
import { Light2DFields, MaterialField, TrailFields } from './LightingFields'
import {
  AddComponentMenu,
  BoxCollider2DFields,
  CapsuleCollider2DFields,
  CircleCollider2DFields,
  HingeJoint2DFields,
  IkConstraint2DFields,
  SpriteSkin2DFields,
  Liquid2DFields,
  COMPONENT_LABEL,
  PolygonCollider2DFields,
  Rigidbody2DFields,
  ShadowCaster2DFields,
  AudioSourceFields,
  UiAnchorFields,
  UiButtonFields,
  UiScrollbarFields,
  UiScrollViewFields,
  UiSliderFields,
  UiStackFields,
  UiToggleFields,
  UiLayoutFields,
  UiLayoutItemFields,
  UiShowWhenFields,
  UiHtmlFields,
  Rigidbody3DFields,
  Collider3DFields,
  FlipbookFields,
  Cloth2DFields,
  Tree2DFields,
  ScreenFields,
} from './ComponentFields'
import { PropertyBindingsFields, ScenePropertiesFields, UiEventsFields } from './EventFields'
import { FontPicker, IconPicker } from './FontPicker'
import { iconFamily, isIconFamily, type IconStyle } from '../game/googleFonts'
import { Goo2DFields } from './GooFields'
import { ParticleBehaviors2DFields, ParticleFluid2DFields, ParticleSystem3DFields } from './Particle3DFields'
import { addGoogleFont, burstParticles3D, addTextNode, defaultComponent, deleteNode, renameNode, setNodeTranslation, setTextContent, setTextFont, setTextSize } from '../ipc/wasmEditor'
import { gameConsole } from '../game/console'
import { ImageAssetPicker } from './ImageAssetPicker'
import { ChevronDownIcon, CloseIcon, EditIcon, PlayIcon, TrashIcon } from './icons'

/**
 * One editable numeric field, used for the X/Y halves of a vector row below.
 * Local `text` state (not directly bound to `value`) so the user can type
 * freely (including transient invalid states like `"-"` mid-edit) without
 * every keystroke round-tripping to the Rust backend — the edit only
 * commits (calling `onCommit`) on blur or Enter. `useEffect` re-syncs
 * `text` whenever `value` changes from outside (an IPC round-trip, an
 * undo/redo) so the field never shows stale data after an external update.
 *
 * Scrolling over the field nudges the value by `step` per wheel notch (held
 * Shift for a 10x-bigger nudge) — the standard "hover a number field, scroll
 * to adjust" convention most design/DAW tools support. Needs a real
 * (non-passive) native `wheel` listener, not React's `onWheel` prop — React
 * attaches wheel listeners as passive by default, so `preventDefault()`
 * inside a synthetic handler would no-op (and warn) instead of stopping the
 * page from scrolling underneath while you nudge.
 */
export function NumberField({ label, value, onCommit, step = 1 }: { label: string; value: number; onCommit: (value: number) => void; step?: number }) {
  const [text, setText] = useState(value.toFixed(1))
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    setText(value.toFixed(1))
  }, [value])

  useEffect(() => {
    const input = inputRef.current
    if (!input) return
    function handleWheel(event: WheelEvent) {
      event.preventDefault()
      const magnitude = event.shiftKey ? step * 10 : step
      onCommit(value + (event.deltaY > 0 ? -magnitude : magnitude))
    }
    input.addEventListener('wheel', handleWheel, { passive: false })
    return () => input.removeEventListener('wheel', handleWheel)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value, step, onCommit])

  const commit = () => {
    const parsed = Number.parseFloat(text)
    if (Number.isFinite(parsed)) {
      onCommit(parsed)
    } else {
      setText(value.toFixed(1)) // invalid input — revert rather than send garbage
    }
  }

  return (
    <div className="flex flex-1 items-center gap-1 rounded-md bg-overlay/5 px-1.5 py-1">
      <span className="text-[11px] text-ink-dim">{label}</span>
      <input
        ref={inputRef}
        type="text"
        inputMode="decimal"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === 'Enter') e.currentTarget.blur()
        }}
        className="w-full min-w-0 bg-transparent text-xs text-ink outline-none"
      />
    </div>
  )
}

/**
 * A custom-drawn slider — a thin track with an accent-filled portion up to
 * the current value and a round thumb, dragged via pointer capture — rather
 * than a bare `<input type="range">`. Even with Tailwind's `accent-color`
 * (which only recolors a native range input's thumb/track, not its overall
 * shape/chrome), a native range input still reads as an obviously-native
 * OS/browser control sitting inside an otherwise fully custom-drawn dark UI;
 * this matches the same "one consistent, custom-drawn surface language"
 * precedent `ColorSwatch`/`Dropdown` already establish for other controls
 * that would otherwise default to native browser chrome.
 */

/** A path's distinct vertices: every anchor, except that a closed subpath's
 * last point sitting exactly on its start is the same vertex (SVG closes
 * curved shapes that way). */
function pathPointCount(path: { commands: PathCommand[] }): number {
  let count = 0
  let start: { x: number; y: number } | null = null
  let last: { x: number; y: number } | null = null
  for (const c of path.commands) {
    if (c.kind === 'Close') {
      if (start && last && last !== start && Math.abs(last.x - start.x) < 1e-3 && Math.abs(last.y - start.y) < 1e-3) count--
      start = last = null
      continue
    }
    const point = c.kind === 'CubicTo' ? c.value.to : c.value
    if (c.kind === 'MoveTo') start = point
    last = point
    count++
  }
  return count
}

/** The Inspector's Stroke section — an outline for a `Shape` node, mirroring
 * the Fill field's own "flat color, or an override" shape: `stroke === null`
 * (the "has a stroke at all" checkbox unchecked) means no outline; checking
 * it starts a sensible default (`STROKE_DEFAULT`) rather than an
 * invisible `width: 0` one, same "a freshly-added control should visibly do
 * something" reasoning `PostEffect::default_for_kind` already established
 * for the Effects panel's own "+ Add Effect". */
const STROKE_DEFAULT: Stroke = { width: 2, color: [0, 0, 0, 1], gradient: null, image: null, cap: 'Butt', join: 'Miter' }
const STROKE_CAPS: StrokeCap[] = ['Butt', 'Square', 'Round']
const STROKE_JOINS: StrokeJoin[] = ['Miter', 'Round', 'Bevel']

function StrokeField({ stroke, onChange }: { stroke: Stroke | null; onChange: (value: Stroke | null) => void }) {
  return (
    <div className="flex flex-col gap-1">
      <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
        <input type="checkbox" checked={stroke !== null} onChange={(e) => onChange(e.target.checked ? STROKE_DEFAULT : null)} />
        Stroke
      </label>
      {stroke && (
        <div className="flex flex-col gap-1.5 pl-1">
          <div className="flex items-center gap-2">
            <span className="w-10 shrink-0 text-[11px] text-ink-dim">Color</span>
            <ColorSwatch value={stroke.color} onChange={(color) => onChange({ ...stroke, color })} />
          </div>
          <div className="flex items-center gap-2">
            <span className="w-10 shrink-0 text-[11px] text-ink-dim">Width</span>
            <NumberField label="W" value={stroke.width} step={0.5} onCommit={(width) => onChange({ ...stroke, width: Math.max(0, width) })} />
          </div>
          <div className="flex gap-1">
            <div className="flex-1">
              <span className="mb-0.5 block text-[10px] text-ink-dim">Cap</span>
              <Dropdown value={stroke.cap} options={STROKE_CAPS} onChange={(cap) => onChange({ ...stroke, cap })} />
            </div>
            <div className="flex-1">
              <span className="mb-0.5 block text-[10px] text-ink-dim">Join</span>
              <Dropdown value={stroke.join} options={STROKE_JOINS} onChange={(join) => onChange({ ...stroke, join })} />
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

const RAD_TO_DEG = 180 / Math.PI
const DEG_TO_RAD = Math.PI / 180

const EMITTER_SHAPE_KINDS = ['Point', 'Circle', 'Cone', 'Box'] as const
type EmitterShapeKindName = (typeof EMITTER_SHAPE_KINDS)[number]

function emitterShapeKind(shape: EmitterShape): EmitterShapeKindName {
  if ('Point' in shape) return 'Point'
  if ('Circle' in shape) return 'Circle'
  if ('Cone' in shape) return 'Cone'
  return 'Box'
}

function defaultEmitterShape(kind: EmitterShapeKindName): EmitterShape {
  switch (kind) {
    case 'Point':
      return { Point: null }
    case 'Circle':
      return { Circle: { radius: 1 } }
    case 'Cone':
      return { Cone: { angle: 0.4, radius: 0 } }
    case 'Box':
      return { Box: { width: 2, height: 2 } }
  }
}

/** A labeled `min`/`max` pair — every `ParticleRange` field in the Main
 * module (start lifetime/speed/size/rotation) uses this same shape. */
function RangeField({ label, range, step = 0.1, onChange }: { label: string; range: ParticleRange; step?: number; onChange: (range: ParticleRange) => void }) {
  return (
    <div className="flex items-center gap-2">
      <span className="w-16 shrink-0 text-[11px] text-ink-dim">{label}</span>
      <NumberField label="min" value={range.min} step={step} onCommit={(min) => onChange({ min, max: Math.max(min, range.max) })} />
      <NumberField label="max" value={range.max} step={step} onCommit={(max) => onChange({ min: Math.min(range.min, max), max })} />
    </div>
  )
}

/**
 * The Inspector's whole Particle System section — every "core" Unity-style
 * module this feature ships (see `an1mbyt3_core::particle`'s own module doc
 * for the full scope/boundary): Main, Emission, Shape, a simple Gravity/
 * Drag, Color over Lifetime, Size over Lifetime, and a constant-rate
 * Rotation over Lifetime. Commits the *whole* config on every single field
 * edit (`update`, below) — same "one nested object, read/written all at
 * once" shape `StrokeField` already uses for its own sub-structure, just
 * with a lot more fields inside it.
 */
/** `Tilemap` nodes' own Inspector section — Unity's Tilemap/Grid component
 * inspector is the reference: tileset reference (read-only — swapping it
 * would desync every already-painted tile index against a different
 * image), tile size (also read-only for the same reason; set once at
 * creation, `App.tsx`'s own "Add Tilemap…" prompt), and the two fields that
 * genuinely are safe to change after painting starts: map size (resizes
 * the grid, preserving overlapping tiles — `TilemapNode::resize`'s own doc
 * comment) and cell size (the on-canvas world-unit size of one cell, purely
 * a display scale with no bearing on the tileset/tile-index mapping at all). */
function TilemapFields({
  config,
  onResize,
  onSetCellSize,
}: {
  config: TilemapConfig
  onResize: (mapWidth: number, mapHeight: number) => void
  onSetCellSize: (value: number) => void
}) {
  return (
    <div className="flex flex-col gap-1.5 rounded-md bg-overlay/5 p-2">
      <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">Tilemap</span>
      <div className="flex items-center gap-2">
        <span className="w-20 shrink-0 text-[11px] text-ink-dim">Tile Size</span>
        <span className="text-[11px] text-ink-dim/70">
          {config.tileWidthPx}×{config.tileHeightPx}px
        </span>
      </div>
      <div className="flex items-center gap-2">
        <span className="w-20 shrink-0 text-[11px] text-ink-dim">Map Size</span>
        <NumberField label="w" value={config.mapWidth} step={1} onCommit={(w) => onResize(Math.max(1, Math.round(w)), config.mapHeight)} />
        <NumberField label="h" value={config.mapHeight} step={1} onCommit={(h) => onResize(config.mapWidth, Math.max(1, Math.round(h)))} />
      </div>
      <div className="flex items-center gap-2">
        <span className="w-20 shrink-0 text-[11px] text-ink-dim">Cell Size</span>
        <NumberField label="u" value={config.cellSize} step={1} onCommit={(value) => onSetCellSize(Math.max(0.01, value))} />
      </div>
    </div>
  )
}

/** The Canvas (scene) section's "Main Camera" picker — always visible, no
 * selection needed, Unity's "which camera is tagged MainCamera" made a
 * first-class scene setting instead of something you only discover by
 * selecting each camera. Lists every Camera node in the scene plus "None";
 * when there are no cameras yet, says so instead of showing an empty list. */
function MainCameraField({ artboard, onChange }: { artboard: ArtboardSummary; onChange: (camera: OpaqueId | null) => void }) {
  const NONE = '__none__'
  const keyOf = (id: OpaqueId) => `${id.idx}:${id.version}`
  const cameras = artboard.nodes.filter((n) => n.camera !== null)
  const current = artboard.activeCamera && cameras.some((c) => idsEqualLocal(c.id, artboard.activeCamera)) ? keyOf(artboard.activeCamera) : NONE
  return (
    <div className="flex flex-col gap-1">
      <span className="text-[11px] text-ink-dim" title="Which Camera node Playground views this scene through. None = the editor's own free pan/zoom.">
        Main Camera
      </span>
      {cameras.length === 0 ? (
        <span className="text-[10px] text-ink-dim/70">No cameras yet — add one from the + menu.</span>
      ) : (
        <Dropdown<string>
          value={current}
          options={[NONE, ...cameras.map((c) => keyOf(c.id))]}
          renderValue={(key) => (key === NONE ? 'None (free view)' : cameras.find((c) => keyOf(c.id) === key)?.name ?? key)}
          renderOption={(key) => (key === NONE ? 'None (free view)' : cameras.find((c) => keyOf(c.id) === key)?.name ?? key)}
          onChange={(key) => onChange(key === NONE ? null : cameras.find((c) => keyOf(c.id) === key)?.id ?? null)}
        />
      )}
    </div>
  )
}

/** `Camera` nodes' own Inspector section — Unity's Camera component is the
 * reference: a zoom field (its orthographic-`size` analog — the node's own
 * position/rotation already come from the shared Position/Rotation fields
 * every node kind gets) plus whether this is the one camera Playground
 * actually views the scene through (`isActive`/`onSetActive` — see
 * `Artboard::active_camera`'s own doc comment for why only one camera per
 * artboard can hold that role at a time, Unity's "Main Camera" tag). */
function CameraFields({
  cameraId,
  config,
  isActive,
  followCandidates,
  onChange,
  onSetActive,
  bare = false,
}: {
  bare?: boolean
  cameraId: OpaqueId
  config: CameraConfig
  isActive: boolean
  /** Every node in the scene this camera could track — the follow-target
   * dropdown's options (the camera itself is filtered out here). */
  followCandidates: NodeSummary[]
  onChange: (value: CameraConfig) => void
  onSetActive: (value: boolean) => void
}) {
  const NONE = '__none__'
  const keyOf = (id: OpaqueId) => `${id.idx}:${id.version}`
  const candidates = followCandidates.filter((n) => !idsEqualLocal(n.id, cameraId))
  const followKey = config.followTarget ? keyOf(config.followTarget) : NONE
  return (
    <div className={`flex flex-col gap-1.5 ${bare ? '' : 'rounded-md bg-overlay/5 p-2'}`}>
      {!bare && <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">Camera</span>}
      <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
        <input type="checkbox" checked={isActive} onChange={(e) => onSetActive(e.target.checked)} />
        Main Camera (Playground views through this)
      </label>
      <div className="flex items-center gap-2">
        <span className="w-20 shrink-0 text-[11px] text-ink-dim">Zoom</span>
        <NumberField label="z" value={config.zoom} step={0.1} onCommit={(value) => onChange({ ...config, zoom: Math.max(0.01, value) })} />
      </div>
      <div className="flex items-center gap-2">
        <span className="w-20 shrink-0 text-[11px] text-ink-dim">Background</span>
        <ColorSwatch value={config.backgroundColor} onChange={(value) => onChange({ ...config, backgroundColor: value })} />
      </div>
      <div className="flex items-center gap-2">
        <span className="w-20 shrink-0 text-[11px] text-ink-dim">Follow</span>
        <div className="min-w-0 flex-1">
          <Dropdown<string>
            value={followKey}
            options={[NONE, ...candidates.map((n) => keyOf(n.id))]}
            renderValue={(key) => (key === NONE ? 'None (fixed)' : candidates.find((n) => keyOf(n.id) === key)?.name ?? 'Missing node')}
            renderOption={(key) => (key === NONE ? 'None (fixed)' : candidates.find((n) => keyOf(n.id) === key)?.name ?? key)}
            onChange={(key) => onChange({ ...config, followTarget: key === NONE ? null : candidates.find((n) => keyOf(n.id) === key)?.id ?? null })}
          />
        </div>
      </div>
      {config.followTarget && (
        <div className="flex items-center gap-2">
          <span className="w-20 shrink-0 text-[11px] text-ink-dim">Smoothing</span>
          <NumberField label="s" value={config.followSmoothing} step={0.5} onCommit={(value) => onChange({ ...config, followSmoothing: Math.max(0, value) })} />
        </div>
      )}
      <div className="flex flex-col gap-1.5 border-t border-overlay/10 pt-2">
        <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
          <input type="checkbox" checked={config.vignette.enabled} onChange={(e) => onChange({ ...config, vignette: { ...config.vignette, enabled: e.target.checked } })} />
          Vignette
        </label>
        {config.vignette.enabled && (
          <VignetteFields value={config.vignette} onChange={(v) => onChange({ ...config, vignette: { ...config.vignette, ...v } })} />
        )}
      </div>
      <div className="flex flex-col gap-1.5 border-t border-overlay/10 pt-2">
        <label className="flex items-center gap-1.5 text-[11px] text-ink-dim" title="Unity's Pixel Perfect Camera: whole-pixel scaling, reference resolution = scene size, 1 world unit = 1 art pixel.">
          <input type="checkbox" checked={config.pixelPerfect.enabled} onChange={(e) => onChange({ ...config, pixelPerfect: { ...config.pixelPerfect, enabled: e.target.checked } })} />
          Pixel Perfect
        </label>
        {config.pixelPerfect.enabled && (
          <>
            <div className="flex items-center gap-2" title="Pixel Snapping: camera moves in whole pixels. Upscale Render Texture: render at reference size, then nearest-upscale (fully on-grid).">
              <span className="w-20 shrink-0 text-[11px] text-ink-dim">Grid Snap</span>
              <div className="min-w-0 flex-1">
                <Dropdown
                  value={config.pixelPerfect.gridSnapping}
                  options={['None', 'PixelSnapping', 'UpscaleRenderTexture'] as PixelGridSnapping[]}
                  renderValue={(v) => ({ None: 'None', PixelSnapping: 'Pixel Snapping', UpscaleRenderTexture: 'Upscale Render Texture' })[v]}
                  renderOption={(v) => ({ None: 'None', PixelSnapping: 'Pixel Snapping', UpscaleRenderTexture: 'Upscale Render Texture' })[v]}
                  onChange={(gridSnapping) => onChange({ ...config, pixelPerfect: { ...config.pixelPerfect, gridSnapping } })}
                />
              </div>
            </div>
            <div className="flex items-center gap-2" title="What fills the window outside the scaled frame.">
              <span className="w-20 shrink-0 text-[11px] text-ink-dim">Crop Frame</span>
              <div className="min-w-0 flex-1">
                <Dropdown
                  value={config.pixelPerfect.cropFrame}
                  options={['None', 'Pillarbox', 'Letterbox', 'Windowbox', 'StretchFill'] as PixelCropFrame[]}
                  renderValue={(v) => (v === 'StretchFill' ? 'Stretch Fill' : v)}
                  renderOption={(v) => (v === 'StretchFill' ? 'Stretch Fill' : v)}
                  onChange={(cropFrame) => onChange({ ...config, pixelPerfect: { ...config.pixelPerfect, cropFrame } })}
                />
              </div>
            </div>
          </>
        )}
      </div>
      <span className="text-[10px] leading-snug text-ink-dim/70">Frame = scene size ÷ zoom. 0 smoothing snaps; ~5 trails gently. Vignette shows in Playground.</span>
    </div>
  )
}

function idsEqualLocal(a: OpaqueId | null | undefined, b: OpaqueId | null | undefined): boolean {
  return a === b || (!!a && !!b && a.idx === b.idx && a.version === b.version)
}

/** A fresh value for a field just switched (or just created) to `kind` —
 * mirrors the Rust `ScriptFieldValue::default_for`'s own per-kind zero
 * values, so switching a field's type in the authoring UI always lands on
 * something every widget below can render without a `null` special case. */
function scriptFieldDefaultFor(kind: ScriptFieldKind): ScriptFieldValue {
  switch (kind) {
    case 'Float':
      return { kind: 'Float', value: 0 }
    case 'Int':
      return { kind: 'Int', value: 0 }
    case 'Bool':
      return { kind: 'Bool', value: false }
    case 'Color':
      return { kind: 'Color', value: [1, 1, 1, 1] }
    case 'Vec2':
      return { kind: 'Vec2', value: [0, 0] }
    case 'String':
      return { kind: 'String', value: '' }
  }
}

/** One field's value editor, shared by the authoring UI (editing a
 * definition's own `default`) and an attached instance's per-field override
 * — same widget either way, just a different value/commit pair. */
function ScriptFieldValueEditor({ value, onChange }: { value: ScriptFieldValue; onChange: (value: ScriptFieldValue) => void }) {
  switch (value.kind) {
    case 'Float':
      return <NumberField label="" value={value.value} step={0.1} onCommit={(v) => onChange({ kind: 'Float', value: v })} />
    case 'Int':
      return <NumberField label="" value={value.value} step={1} onCommit={(v) => onChange({ kind: 'Int', value: Math.round(v) })} />
    case 'Bool':
      return (
        <input type="checkbox" checked={value.value} onChange={(e) => onChange({ kind: 'Bool', value: e.target.checked })} className="h-3.5 w-3.5" />
      )
    case 'Color':
      return <ColorSwatch value={value.value} onChange={(v) => onChange({ kind: 'Color', value: v })} />
    case 'Vec2':
      return (
        <div className="flex gap-1">
          <NumberField label="x" value={value.value[0]} step={1} onCommit={(v) => onChange({ kind: 'Vec2', value: [v, value.value[1]] })} />
          <NumberField label="y" value={value.value[1]} step={1} onCommit={(v) => onChange({ kind: 'Vec2', value: [value.value[0], v] })} />
        </div>
      )
    case 'String':
      return (
        <input
          type="text"
          value={value.value}
          onChange={(e) => onChange({ kind: 'String', value: e.target.value })}
          className="w-full rounded bg-overlay/10 px-1.5 py-0.5 text-[11px] text-ink outline-none focus:ring-1 focus:ring-accent"
        />
      )
  }
}

const SCRIPT_FIELD_KINDS: ScriptFieldKind[] = ['Float', 'Int', 'Bool', 'Color', 'Vec2', 'String']

/** Authors a `ScriptDefinition`'s own field list — shared by every node that
 * currently has an instance of this script attached (editing the *type*,
 * not any one instance's values), reached via that instance's own "Edit
 * Fields" toggle in the Behaviors section below. */
function ScriptFieldsEditor({ definition, onChange }: { definition: ScriptDefinitionSummary; onChange: (fields: ScriptFieldDef[]) => void }) {
  return (
    <div className="flex flex-col gap-1.5 rounded bg-overlay/5 p-1.5">
      {definition.fields.map((field, index) => (
        <div key={index} className="flex flex-col gap-1 rounded bg-overlay/5 p-1.5">
          <div className="flex items-center gap-1">
            <input
              type="text"
              value={field.name}
              onChange={(e) => {
                const fields = definition.fields.map((f, i) => (i === index ? { ...f, name: e.target.value } : f))
                onChange(fields)
              }}
              placeholder="field name"
              className="min-w-0 flex-1 rounded bg-overlay/10 px-1.5 py-0.5 text-[11px] text-ink outline-none focus:ring-1 focus:ring-accent"
            />
            <div className="w-20 shrink-0">
              <Dropdown<ScriptFieldKind>
                value={field.default.kind}
                options={SCRIPT_FIELD_KINDS}
                onChange={(kind) => {
                  const fields = definition.fields.map((f, i) => (i === index ? { ...f, default: scriptFieldDefaultFor(kind) } : f))
                  onChange(fields)
                }}
              />
            </div>
            <button
              onClick={() => onChange(definition.fields.filter((_, i) => i !== index))}
              title="Remove field"
              className="shrink-0 rounded p-0.5 text-ink-dim hover:bg-red-500/10 hover:text-red-400"
            >
              <TrashIcon />
            </button>
          </div>
          <div className="flex items-center gap-2">
            <label className="flex items-center gap-1 text-[11px] text-ink-dim">
              <input
                type="checkbox"
                checked={field.animatable}
                onChange={(e) => {
                  const fields = definition.fields.map((f, i) => (i === index ? { ...f, animatable: e.target.checked } : f))
                  onChange(fields)
                }}
                className="h-3.5 w-3.5"
              />
              Animatable
            </label>
            <span className="text-[11px] text-ink-dim">Default</span>
            <ScriptFieldValueEditor
              value={field.default}
              onChange={(value) => {
                const fields = definition.fields.map((f, i) => (i === index ? { ...f, default: value } : f))
                onChange(fields)
              }}
            />
          </div>
        </div>
      ))}
      <button
        onClick={() => onChange([...definition.fields, { name: `field${definition.fields.length + 1}`, default: { kind: 'Float', value: 0 }, animatable: false }])}
        className="rounded bg-overlay/5 px-2 py-1 text-[11px] text-ink-dim hover:bg-overlay/10 hover:text-ink"
      >
        + Add Field
      </button>
    </div>
  )
}

/** The Inspector's "Behaviors" section — the Unity-"Add Component" analog
 * (see `an1mbyt3_core::behavior`'s own doc comment for why the code-level
 * name dodges this app's unrelated existing "Component" meaning). Any node
 * kind can carry any number of attached behaviors; this pass only gives
 * `Script` instances a real editor (their own definition's typed fields,
 * read/written per-instance) — `SpriteRenderer` entries still show up in the
 * list (so they're visible/removable/toggleable) but have no dedicated
 * field editor yet, same "round-trips as inert data for now" scope the DTO
 * layer's own doc comment already notes. */
function BehaviorsFields({
  behaviors,
  scripts,
  onChange,
  onAddScript,
  onSetScriptFields,
  node,
  artboard,
  images,
  editor,
  onSetActiveCamera,
  onReplayParticleSystem,
  onBindSkin,
  materials,
  onEditMaterial,
  sounds,
  textFiles,
  onOpenFile,
  onDocumentChange,
  properties,
  sceneNames,
}: {
  behaviors: NodeBehaviorEntry[]
  scripts: ScriptDefinitionSummary[]
  onChange: (value: NodeBehaviorEntry[]) => void
  onAddScript: (name: string) => void
  onSetScriptFields: (script: OpaqueId, fields: ScriptFieldDef[]) => void
  /** The node these components belong to (camera "Main" state, ids). */
  node: NodeSummary
  artboard: ArtboardSummary | null
  images: ImageAssetSummary[]
  editor: WasmEditor | null
  onSetActiveCamera: (value: boolean) => void
  onReplayParticleSystem: () => void
  onBindSkin: (rootBone: OpaqueId) => void
  materials: MaterialSummary[]
  onEditMaterial: (material: OpaqueId) => void
  /** Imported sounds (the Audio Source clip picker). */
  sounds: AudioSummary[]
  /** Project code files (HTML UI's file pickers). */
  textFiles: TextFileSummary[]
  /** Opens a file in the Code tab. */
  onOpenFile: (path: string) => void
  onDocumentChange: (doc: DocumentSummary) => void
  /** Scene properties and scene names (On Click / Property Binding pickers). */
  properties: PropertySummary[]
  sceneNames: string[]
}) {
  const [editingFieldsFor, setEditingFieldsFor] = useState<number | null>(null)
  // The "+ New Script..." inline create row — a real in-panel text field
  // (not `window.prompt`), shown in place of the Add Component button
  // until Enter/"Create" commits or Escape/the X cancels.
  const [creatingScript, setCreatingScript] = useState(false)
  const [newScriptName, setNewScriptName] = useState('NewScript')

  const commitNewScript = () => {
    const name = newScriptName.trim()
    if (name) onAddScript(name)
    setCreatingScript(false)
  }

  const behaviorLabel = (behavior: NodeBehavior): string => {
    if (behavior.kind === 'SpriteRenderer') return 'Material'
    if (behavior.kind !== 'Script') return COMPONENT_LABEL[behavior.kind]
    const definition = scripts.find((s) => idsEqualLocal(s.id, behavior.definition))
    return definition ? definition.name : 'Script (missing)'
  }
  const replaceAt = (index: number, behavior: NodeBehavior) => onChange(behaviors.map((b, i) => (i === index ? { ...b, behavior } : b)))

  /** The editor for one Unity component's settings. */
  const componentBody = (behavior: NodeBehavior, index: number): ReactNode => {
    switch (behavior.kind) {
      case 'Camera': {
        const { kind: _k, ...config } = behavior
        return (
          <CameraFields
            bare
            cameraId={node.id}
            config={config}
            isActive={idsEqualLocal(artboard?.activeCamera, node.id)}
            followCandidates={artboard?.nodes ?? []}
            onChange={(c) => replaceAt(index, { kind: 'Camera', ...c })}
            onSetActive={onSetActiveCamera}
          />
        )
      }
      case 'Light2D': {
        const { kind: _k, ...config } = behavior
        return <Light2DFields bare config={config} images={images} editor={editor} onChange={(c) => replaceAt(index, { kind: 'Light2D', ...c })} />
      }
      case 'TrailRenderer': {
        const { kind: _k, ...config } = behavior
        return <TrailFields bare config={config} images={images} editor={editor} onChange={(c) => replaceAt(index, { kind: 'TrailRenderer', ...c })} />
      }
      case 'ParticleSystem': {
        const { kind: _k, ...config } = behavior
        return <ParticleSystemFields bare config={config} onChange={(c) => replaceAt(index, { kind: 'ParticleSystem', ...c })} onReplay={onReplayParticleSystem} />
      }
      case 'ShadowCaster2D': {
        const { kind: _k, ...config } = behavior
        return <ShadowCaster2DFields config={config} onChange={(c) => replaceAt(index, { kind: 'ShadowCaster2D', ...c })} />
      }
      case 'Rigidbody2D': {
        const { kind: _k, ...config } = behavior
        return <Rigidbody2DFields config={config} onChange={(c) => replaceAt(index, { kind: 'Rigidbody2D', ...c })} />
      }
      case 'BoxCollider2D': {
        const { kind: _k, ...config } = behavior
        return <BoxCollider2DFields config={config} onChange={(c) => replaceAt(index, { kind: 'BoxCollider2D', ...c })} />
      }
      case 'CircleCollider2D': {
        const { kind: _k, ...config } = behavior
        return <CircleCollider2DFields config={config} onChange={(c) => replaceAt(index, { kind: 'CircleCollider2D', ...c })} />
      }
      case 'CapsuleCollider2D': {
        const { kind: _k, ...config } = behavior
        return <CapsuleCollider2DFields config={config} onChange={(c) => replaceAt(index, { kind: 'CapsuleCollider2D', ...c })} />
      }
      case 'PolygonCollider2D': {
        const { kind: _k, ...config } = behavior
        return <PolygonCollider2DFields config={config} onChange={(c) => replaceAt(index, { kind: 'PolygonCollider2D', ...c })} />
      }
      case 'HingeJoint2D': {
        const { kind: _k, ...config } = behavior
        return <HingeJoint2DFields config={config} nodes={artboard?.nodes ?? []} onChange={(c) => replaceAt(index, { kind: 'HingeJoint2D', ...c })} />
      }
      case 'IkConstraint2D': {
        const { kind: _k, ...config } = behavior
        return <IkConstraint2DFields config={config} nodes={artboard?.nodes ?? []} onChange={(c) => replaceAt(index, { kind: 'IkConstraint2D', ...c })} />
      }
      case 'SpriteSkin2D': {
        const { kind: _k, ...config } = behavior
        const nodes = artboard?.nodes ?? []
        const isBone = (id: OpaqueId | null) => !!id && nodes.some((n) => n.kind === 'Bone' && idsEqualLocal(n.id, id))
        const roots = nodes.filter((n) => n.kind === 'Bone' && !isBone(n.parent))
        return <SpriteSkin2DFields config={config} rootBones={roots} onChange={(c) => replaceAt(index, { kind: 'SpriteSkin2D', ...c })} onRebind={onBindSkin} />
      }
      case 'UiEvents': {
        const { kind: _k, ...config } = behavior
        return <UiEventsFields config={config} ctx={{ artboard, self: node, sceneNames, soundNames: sounds.map((s) => s.name), properties }} onChange={(c) => replaceAt(index, { kind: 'UiEvents', ...c })} />
      }
      case 'Goo2D': {
        const { kind: _k, ...config } = behavior
        return <Goo2DFields config={config} onChange={(c) => replaceAt(index, { kind: 'Goo2D', ...c })} />
      }
      case 'ParticleFluid2D': {
        const { kind: _k, ...config } = behavior
        return <ParticleFluid2DFields config={config} onChange={(c) => replaceAt(index, { kind: 'ParticleFluid2D', ...c })} />
      }
      case 'ParticleBehaviors2D': {
        const { kind: _k, ...config } = behavior
        return <ParticleBehaviors2DFields config={config} onChange={(c) => replaceAt(index, { kind: 'ParticleBehaviors2D', ...c })} />
      }
      case 'ParticleSystem3D': {
        const { kind: _k, ...config } = behavior
        return (
          <ParticleSystem3DFields
            config={config}
            onChange={(c) => replaceAt(index, { kind: 'ParticleSystem3D', ...c })}
            onBurst={editor && artboard ? () => burstParticles3D(editor, artboard.id, node.id, 80) : undefined}
          />
        )
      }
      case 'PropertyBindings': {
        const { kind: _k, ...config } = behavior
        return <PropertyBindingsFields config={config} ctx={{ artboard, self: node, sceneNames, soundNames: sounds.map((s) => s.name), properties }} onChange={(c) => replaceAt(index, { kind: 'PropertyBindings', ...c })} />
      }
      case 'Tree2D': {
        const { kind: _k, ...config } = behavior
        return <Tree2DFields config={config} onChange={(c) => replaceAt(index, { kind: 'Tree2D', ...c })} />
      }
      case 'Cloth2D': {
        const { kind: _k, ...config } = behavior
        return <Cloth2DFields config={config} images={images} editor={editor} onChange={(c) => replaceAt(index, { kind: 'Cloth2D', ...c })} />
      }
      case 'Liquid2D': {
        const { kind: _k, ...config } = behavior
        return <Liquid2DFields config={config} onChange={(c) => replaceAt(index, { kind: 'Liquid2D', ...c })} />
      }
      case 'UiAnchor': {
        const { kind: _k, ...config } = behavior
        return <UiAnchorFields config={config} onChange={(c) => replaceAt(index, { kind: 'UiAnchor', ...c })} />
      }
      case 'UiStack': {
        const { kind: _k, ...config } = behavior
        return <UiStackFields config={config} onChange={(c) => replaceAt(index, { kind: 'UiStack', ...c })} />
      }
      case 'UiButton': {
        const { kind: _k, ...config } = behavior
        return (
          <>
            <UiButtonFields config={config} images={images} editor={editor} onChange={(c) => replaceAt(index, { kind: 'UiButton', ...c })} />
            <ButtonIconField editor={editor} artboard={artboard} node={node} onDocumentChange={onDocumentChange} />
          </>
        )
      }
      case 'UiToggle': {
        const { kind: _k, ...config } = behavior
        return <UiToggleFields config={config} images={images} editor={editor} onChange={(c) => replaceAt(index, { kind: 'UiToggle', ...c })} />
      }
      case 'UiSlider': {
        const { kind: _k, ...config } = behavior
        return <UiSliderFields config={config} images={images} editor={editor} onChange={(c) => replaceAt(index, { kind: 'UiSlider', ...c })} />
      }
      case 'UiScrollbar': {
        const { kind: _k, ...config } = behavior
        return <UiScrollbarFields config={config} images={images} editor={editor} onChange={(c) => replaceAt(index, { kind: 'UiScrollbar', ...c })} />
      }
      case 'UiLayout': {
        const { kind: _k, ...config } = behavior
        return <UiLayoutFields config={config} onChange={(c) => replaceAt(index, { kind: 'UiLayout', ...c })} />
      }
      case 'UiLayoutItem': {
        const { kind: _k, ...config } = behavior
        return <UiLayoutItemFields config={config} onChange={(c) => replaceAt(index, { kind: 'UiLayoutItem', ...c })} />
      }
      case 'Rigidbody3D': {
        const { kind: _k, ...config } = behavior
        return <Rigidbody3DFields config={config} onChange={(c) => replaceAt(index, { kind: 'Rigidbody3D', ...c })} />
      }
      case 'Collider3D': {
        const { kind: _k, ...config } = behavior
        return <Collider3DFields config={config} onChange={(c) => replaceAt(index, { kind: 'Collider3D', ...c })} />
      }
      case 'Flipbook': {
        const { kind: _k, ...config } = behavior
        return <FlipbookFields config={config} frameCount={artboard?.nodes.filter((n) => n.parent && n.parent.idx === node.id.idx && n.parent.version === node.id.version).length ?? 0} onChange={(c) => replaceAt(index, { kind: 'Flipbook', ...c })} />
      }
      case 'UiHtml': {
        const { kind: _k, ...config } = behavior
        return <UiHtmlFields config={config} node={node} files={textFiles} editor={editor} onOpenFile={onOpenFile} onDocumentChange={onDocumentChange} onChange={(c) => replaceAt(index, { kind: 'UiHtml', ...c })} />
      }
      case 'UiShowWhen': {
        const { kind: _k, ...config } = behavior
        return <UiShowWhenFields config={config} onChange={(c) => replaceAt(index, { kind: 'UiShowWhen', ...c })} />
      }
      case 'UiScrollView': {
        const { kind: _k, ...config } = behavior
        return <UiScrollViewFields config={config} onChange={(c) => replaceAt(index, { kind: 'UiScrollView', ...c })} />
      }
      case 'AudioSource': {
        const { kind: _k, ...config } = behavior
        return <AudioSourceFields config={config} sounds={sounds} editor={editor} onChange={(c) => replaceAt(index, { kind: 'AudioSource', ...c })} />
      }
      case 'SpriteRenderer': {
        // The Material component: which shader-graph material this sprite /
        // shape is drawn with (edited in the Shader tab).
        const none = 'none'
        const keyOf = (id: OpaqueId) => `${id.idx}:${id.version}`
        const current = behavior.material ? keyOf(behavior.material) : none
        return (
          <div className="flex items-center gap-1.5">
            <div className="min-w-0 flex-1">
              <Dropdown
                value={current}
                options={[none, ...materials.map((m) => keyOf(m.id))]}
                renderValue={(k) => (k === none ? 'None' : (materials.find((m) => keyOf(m.id) === k)?.name ?? 'Missing material'))}
                renderOption={(k) => (k === none ? 'None' : (materials.find((m) => keyOf(m.id) === k)?.name ?? k))}
                onChange={(k) => replaceAt(index, { ...behavior, material: materials.find((m) => keyOf(m.id) === k)?.id ?? null })}
              />
            </div>
            <button
              onClick={() => behavior.material && onEditMaterial(behavior.material)}
              disabled={!behavior.material}
              title="Edit this material's node graph in the Shader tab"
              className="shrink-0 rounded bg-overlay/10 px-2 py-1 text-[11px] text-ink hover:bg-accent/20 hover:text-accent disabled:opacity-35"
            >
              Edit Graph
            </button>
          </div>
        )
      }
      default:
        return null
    }
  }

  const present = behaviors.map((b) => b.behavior.kind).filter((k): k is ComponentKind => k !== 'Script' && k !== 'SpriteRenderer')

  return (
    <div className="flex flex-col gap-1.5 rounded-md bg-overlay/5 p-2">
      <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">Components</span>

      {behaviors.map((entry, index) => {
        const definition = entry.behavior.kind === 'Script' ? scripts.find((s) => idsEqualLocal(s.id, entry.behavior.kind === 'Script' ? entry.behavior.definition : null)) : null
        return (
          <div key={index} className="flex flex-col gap-1.5 rounded bg-overlay/5 p-1.5">
            <div className="flex items-center gap-1.5">
              <input
                type="checkbox"
                checked={entry.enabled}
                onChange={(e) => onChange(behaviors.map((b, i) => (i === index ? { ...b, enabled: e.target.checked } : b)))}
                className="h-3.5 w-3.5"
                title="Enabled"
              />
              <span className="flex-1 text-[11px] font-medium text-ink">{behaviorLabel(entry.behavior)}</span>
              {entry.behavior.kind === 'Script' && (
                <button
                  onClick={() => setEditingFieldsFor(editingFieldsFor === index ? null : index)}
                  title="Edit this script's fields"
                  className="rounded p-0.5 text-ink-dim hover:bg-overlay/10 hover:text-ink"
                >
                  <EditIcon />
                </button>
              )}
              <button
                onClick={() => onChange(behaviors.filter((_, i) => i !== index))}
                title="Remove component"
                className="rounded p-0.5 text-ink-dim hover:bg-red-500/10 hover:text-red-400"
              >
                <TrashIcon />
              </button>
            </div>

            {componentBody(entry.behavior, index)}

            {entry.behavior.kind === 'Script' && definition && editingFieldsFor === index && (
              <ScriptFieldsEditor definition={definition} onChange={(fields) => onSetScriptFields(definition.id, fields)} />
            )}
            {(() => {
              const scriptBehavior = entry.behavior
              if (scriptBehavior.kind !== 'Script' || !definition || editingFieldsFor === index) return null
              return (
                <div className="flex flex-col gap-1">
                  {definition.fields.map((field) => {
                    const current = scriptBehavior.values.find(([name]) => name === field.name)?.[1] ?? field.default
                    return (
                      <div key={field.name} className="flex items-center gap-2">
                        <span className="w-20 shrink-0 truncate text-[11px] text-ink-dim">{field.name}</span>
                        <ScriptFieldValueEditor
                          value={current}
                          onChange={(value) => {
                            const values = [...scriptBehavior.values.filter(([name]) => name !== field.name), [field.name, value] as [string, ScriptFieldValue]]
                            onChange(behaviors.map((b, i) => (i === index ? { ...b, behavior: { kind: 'Script', definition: scriptBehavior.definition, values } } : b)))
                          }}
                        />
                      </div>
                    )
                  })}
                </div>
              )
            })()}
          </div>
        )
      })}

      {creatingScript ? (
        <div className="flex items-center gap-1">
          <input
            type="text"
            autoFocus
            value={newScriptName}
            onChange={(e) => setNewScriptName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') commitNewScript()
              if (e.key === 'Escape') setCreatingScript(false)
            }}
            placeholder="Script name"
            className="min-w-0 flex-1 rounded bg-overlay/10 px-1.5 py-1 text-[11px] text-ink outline-none focus:ring-1 focus:ring-accent"
          />
          <button onClick={commitNewScript} className="shrink-0 rounded bg-accent/80 px-2 py-1 text-[11px] text-white hover:bg-accent">
            Create
          </button>
          <button onClick={() => setCreatingScript(false)} title="Cancel" className="shrink-0 rounded p-1 text-ink-dim hover:bg-overlay/10 hover:text-ink">
            <CloseIcon />
          </button>
        </div>
      ) : (
        <AddComponentMenu
          present={present}
          hasMaterial={behaviors.some((b) => b.behavior.kind === 'SpriteRenderer')}
          onAddMaterial={() => onChange([...behaviors, { behavior: { kind: 'SpriteRenderer', material: materials[0]?.id ?? null, color: [1, 1, 1, 1] }, enabled: true }])}
          scripts={scripts.map((s) => ({ key: `${s.id.idx}:${s.id.version}`, name: s.name }))}
          onAdd={(kind) => {
            if (!editor) return
            const added = [...behaviors, { behavior: defaultComponent(editor, kind), enabled: true }]
            // A Button comes with its On Click list, like Unity's.
            if (kind === 'UiButton' && !behaviors.some((b) => b.behavior.kind === 'UiEvents')) added.push({ behavior: defaultComponent(editor, 'UiEvents'), enabled: true })
            onChange(added)
          }}
          onAddScript={(key) => {
            const script = scripts.find((s) => `${s.id.idx}:${s.id.version}` === key)
            if (!script) return
            onChange([...behaviors, { behavior: { kind: 'Script', definition: script.id, values: [] }, enabled: true }])
          }}
          onNewScript={() => {
            setNewScriptName('NewScript')
            setCreatingScript(true)
          }}
        />
      )}
    </div>
  )
}

function ParticleSystemFields({
  config,
  onChange,
  onReplay,
  bare = false,
}: {
  bare?: boolean
  config: ParticleSystemConfig
  onChange: (value: ParticleSystemConfig) => void
  /** Restarts this node's live simulation from scratch
   * (`WasmEditor::replay_particle_system`) — lets a designer re-trigger a
   * burst/preview repeatedly without switching modes or scrubbing the
   * Timeline, since a non-looping or burst-only system otherwise only ever
   * plays once per session. */
  onReplay: () => void
}) {
  const update = (patch: Partial<ParticleSystemConfig>) => onChange({ ...config, ...patch })
  const updateMain = (patch: Partial<ParticleMain>) => update({ main: { ...config.main, ...patch } })
  const updateEmission = (patch: Partial<ParticleEmission>) => update({ emission: { ...config.emission, ...patch } })

  const shapeKind = emitterShapeKind(config.shape)

  return (
    <div className={`flex flex-col gap-3 ${bare ? '' : 'rounded-md bg-overlay/5 p-2'}`}>
      <div className="flex items-center justify-between">
        <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">{bare ? '' : 'Particle System'}</span>
        <button
          onClick={onReplay}
          title="Restart this particle system's simulation"
          className="flex items-center gap-1 rounded-md bg-accent/15 px-2 py-0.5 text-[10px] font-medium text-accent hover:bg-accent/25"
        >
          <PlayIcon />
          Replay
        </button>
      </div>

      {/* Main */}
      <div className="flex flex-col gap-1.5">
        <span className="text-[10px] uppercase tracking-wide text-ink-dim/70">Main</span>
        <div className="flex items-center gap-3">
          <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
            <input type="checkbox" checked={config.main.looping} onChange={(e) => updateMain({ looping: e.target.checked })} />
            Looping
          </label>
          <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
            <input type="checkbox" checked={config.main.playOnAwake} onChange={(e) => updateMain({ playOnAwake: e.target.checked })} />
            Play On Awake
          </label>
        </div>
        <div className="flex items-center gap-2">
          <span className="w-16 shrink-0 text-[11px] text-ink-dim">Duration</span>
          <NumberField label="s" value={config.main.duration} step={0.5} onCommit={(duration) => updateMain({ duration: Math.max(0, duration) })} />
        </div>
        <RangeField label="Lifetime" range={config.main.startLifetime} onChange={(startLifetime) => updateMain({ startLifetime })} />
        <RangeField label="Speed" range={config.main.startSpeed} onChange={(startSpeed) => updateMain({ startSpeed })} />
        <RangeField label="Size" range={config.main.startSize} onChange={(startSize) => updateMain({ startSize })} />
        <RangeField
          label="Rotation"
          step={1}
          range={{ min: config.main.startRotation.min * RAD_TO_DEG, max: config.main.startRotation.max * RAD_TO_DEG }}
          onChange={(deg) => updateMain({ startRotation: { min: deg.min * DEG_TO_RAD, max: deg.max * DEG_TO_RAD } })}
        />
        <div className="flex items-center gap-2">
          <span className="w-16 shrink-0 text-[11px] text-ink-dim">Color</span>
          <ColorSwatch value={config.main.startColor} onChange={(startColor) => updateMain({ startColor })} />
        </div>
        <div className="flex items-center gap-2">
          <span className="w-16 shrink-0 text-[11px] text-ink-dim">Gravity</span>
          <NumberField label="g" value={config.main.gravityModifier} step={0.1} onCommit={(gravityModifier) => updateMain({ gravityModifier })} />
        </div>
        <div className="flex items-center gap-2">
          <span className="w-16 shrink-0 text-[11px] text-ink-dim">Drag</span>
          <NumberField label="d" value={config.main.drag} step={0.05} onCommit={(drag) => updateMain({ drag: Math.max(0, drag) })} />
        </div>
        <div className="flex items-center gap-2">
          <span className="w-16 shrink-0 text-[11px] text-ink-dim">Space</span>
          <Dropdown value={config.main.simulationSpace} options={['Local', 'World']} onChange={(simulationSpace) => updateMain({ simulationSpace })} />
        </div>
        <div className="flex items-center gap-2">
          <span className="w-16 shrink-0 text-[11px] text-ink-dim">Max Count</span>
          <NumberField label="#" value={config.main.maxParticles} step={1} onCommit={(v) => updateMain({ maxParticles: Math.max(0, Math.round(v)) })} />
        </div>
      </div>

      {/* Emission */}
      <div className="flex flex-col gap-1.5 border-t border-overlay/10 pt-2">
        <span className="text-[10px] uppercase tracking-wide text-ink-dim/70">Emission</span>
        <div className="flex items-center gap-2">
          <span className="w-16 shrink-0 text-[11px] text-ink-dim">Rate/sec</span>
          <NumberField label="#" value={config.emission.rateOverTime} step={0.5} onCommit={(rateOverTime) => updateEmission({ rateOverTime: Math.max(0, rateOverTime) })} />
        </div>
        {config.emission.bursts.map((burst, i) => (
          <div key={i} className="flex items-center gap-1">
            <NumberField label="t" value={burst.time} step={0.1} onCommit={(time) => updateEmission({ bursts: config.emission.bursts.map((b, j) => (j === i ? { ...b, time } : b)) })} />
            <NumberField
              label="n"
              value={burst.count}
              step={1}
              onCommit={(v) => updateEmission({ bursts: config.emission.bursts.map((b, j) => (j === i ? { ...b, count: Math.max(0, Math.round(v)) } : b)) })}
            />
            <NumberField
              label="×"
              value={burst.cycles}
              step={1}
              onCommit={(v) => updateEmission({ bursts: config.emission.bursts.map((b, j) => (j === i ? { ...b, cycles: Math.max(0, Math.round(v)) } : b)) })}
            />
            <button
              onClick={() => updateEmission({ bursts: config.emission.bursts.filter((_, j) => j !== i) })}
              className="shrink-0 rounded bg-red-500/10 px-1.5 py-0.5 text-[10px] text-red-300 hover:bg-red-500/20"
            >
              <TrashIcon />
            </button>
          </div>
        ))}
        <button
          onClick={() => updateEmission({ bursts: [...config.emission.bursts, { time: 0, count: 10, cycles: 1, interval: 0 }] })}
          className="self-start rounded-md bg-accent/15 px-2 py-1 text-[11px] font-medium text-accent hover:bg-accent/25"
        >
          + Burst
        </button>
      </div>

      {/* Shape */}
      <div className="flex flex-col gap-1.5 border-t border-overlay/10 pt-2">
        <span className="text-[10px] uppercase tracking-wide text-ink-dim/70">Shape</span>
        <Dropdown value={shapeKind} options={[...EMITTER_SHAPE_KINDS]} onChange={(kind) => update({ shape: defaultEmitterShape(kind) })} />
        {'Circle' in config.shape && (
          <NumberField label="radius" value={config.shape.Circle.radius} step={0.1} onCommit={(radius) => update({ shape: { Circle: { radius } } })} />
        )}
        {'Cone' in config.shape && (
          <div className="flex gap-1">
            <NumberField
              label="angle°"
              value={config.shape.Cone.angle * RAD_TO_DEG}
              step={1}
              onCommit={(deg) => update({ shape: { Cone: { angle: deg * DEG_TO_RAD, radius: (config.shape as { Cone: { angle: number; radius: number } }).Cone.radius } } })}
            />
            <NumberField
              label="radius"
              value={config.shape.Cone.radius}
              step={0.1}
              onCommit={(radius) => update({ shape: { Cone: { angle: (config.shape as { Cone: { angle: number; radius: number } }).Cone.angle, radius } } })}
            />
          </div>
        )}
        {'Box' in config.shape && (
          <div className="flex gap-1">
            <NumberField
              label="w"
              value={config.shape.Box.width}
              step={0.1}
              onCommit={(width) => update({ shape: { Box: { width, height: (config.shape as { Box: { width: number; height: number } }).Box.height } } })}
            />
            <NumberField
              label="h"
              value={config.shape.Box.height}
              step={0.1}
              onCommit={(height) => update({ shape: { Box: { width: (config.shape as { Box: { width: number; height: number } }).Box.width, height } } })}
            />
          </div>
        )}
      </div>

      {/* Color over Lifetime */}
      <div className="flex flex-col gap-1.5 border-t border-overlay/10 pt-2">
        <span className="text-[10px] uppercase tracking-wide text-ink-dim/70">Color over Lifetime</span>
        {config.colorOverLifetime.length === 0 && <p className="text-[10px] text-ink-dim">No curve — stays the start color for its whole life.</p>}
        {config.colorOverLifetime.map((stop, i) => (
          <div key={i} className="flex items-center gap-1">
            <NumberField
              label="t"
              value={stop.offset}
              step={0.05}
              onCommit={(offset) => update({ colorOverLifetime: config.colorOverLifetime.map((s, j) => (j === i ? { ...s, offset: Math.min(1, Math.max(0, offset)) } : s)) })}
            />
            <ColorSwatch value={stop.color} onChange={(color) => update({ colorOverLifetime: config.colorOverLifetime.map((s, j) => (j === i ? { ...s, color } : s)) })} />
            <button
              onClick={() => update({ colorOverLifetime: config.colorOverLifetime.filter((_, j) => j !== i) })}
              className="shrink-0 rounded bg-red-500/10 px-1.5 py-0.5 text-[10px] text-red-300 hover:bg-red-500/20"
            >
              <TrashIcon />
            </button>
          </div>
        ))}
        <button
          onClick={() => update({ colorOverLifetime: [...config.colorOverLifetime, { offset: config.colorOverLifetime.length === 0 ? 0 : 1, color: config.main.startColor, midpoint: 0.5 }] })}
          className="self-start rounded-md bg-accent/15 px-2 py-1 text-[11px] font-medium text-accent hover:bg-accent/25"
        >
          + Stop
        </button>
      </div>

      {/* Size over Lifetime */}
      <div className="flex flex-col gap-1.5 border-t border-overlay/10 pt-2">
        <span className="text-[10px] uppercase tracking-wide text-ink-dim/70">Size over Lifetime</span>
        {config.sizeOverLifetime.length === 0 && <p className="text-[10px] text-ink-dim">No curve — stays the start size for its whole life.</p>}
        {config.sizeOverLifetime.map((key, i) => (
          <div key={i} className="flex items-center gap-1">
            <NumberField
              label="t"
              value={key.t}
              step={0.05}
              onCommit={(t) => update({ sizeOverLifetime: config.sizeOverLifetime.map((k, j) => (j === i ? { ...k, t: Math.min(1, Math.max(0, t)) } : k)) })}
            />
            <NumberField
              label="×"
              value={key.sizeMultiplier}
              step={0.1}
              onCommit={(sizeMultiplier) => update({ sizeOverLifetime: config.sizeOverLifetime.map((k, j) => (j === i ? { ...k, sizeMultiplier: Math.max(0, sizeMultiplier) } : k)) })}
            />
            <button
              onClick={() => update({ sizeOverLifetime: config.sizeOverLifetime.filter((_, j) => j !== i) })}
              className="shrink-0 rounded bg-red-500/10 px-1.5 py-0.5 text-[10px] text-red-300 hover:bg-red-500/20"
            >
              <TrashIcon />
            </button>
          </div>
        ))}
        <button
          onClick={() => update({ sizeOverLifetime: [...config.sizeOverLifetime, { t: config.sizeOverLifetime.length === 0 ? 0 : 1, sizeMultiplier: 1 }] })}
          className="self-start rounded-md bg-accent/15 px-2 py-1 text-[11px] font-medium text-accent hover:bg-accent/25"
        >
          + Keyframe
        </button>
      </div>

      {/* Rotation over Lifetime */}
      <div className="flex items-center gap-2 border-t border-overlay/10 pt-2">
        <span className="w-16 shrink-0 text-[11px] text-ink-dim">Spin °/s</span>
        <NumberField label="°/s" value={config.rotationSpeed * RAD_TO_DEG} step={5} onCommit={(deg) => update({ rotationSpeed: deg * DEG_TO_RAD })} />
      </div>
    </div>
  )
}

/**
 * The Inspector's Motion Path component — Unity-style "addable component"
 * for a node (per the user's own framing: colliders/motion-path/etc. should
 * be components you attach, not fields cluttering the Timeline). Binds this
 * node's translation to a *guide* shape's own path (`Path::point_at_progress`)
 * via `Command::SetMotionPathGuide`; the actual progress keyframes are
 * still authored in the Timeline's "Motion Path" row (a plain scalar
 * `0..1` track like any other property) — this component only owns *which*
 * shape that scalar is measured against, keeping the Timeline itself simple
 * (just a number to key), and the "what am I bound to, and can I rebind/
 * remove it" concern living where every other per-node concern already
 * lives, the Inspector. `guideId`/`guideOptions` mirror
 * `TimelinePanel`'s now-removed `MotionPathGuideControl` almost exactly —
 * same picker, just relocated here.
 */
function MotionPathField({
  guideId,
  guideOptions,
  onSetGuide,
  onRemoveTrack,
}: {
  guideId: OpaqueId | null
  guideOptions: NodeSummary[]
  onSetGuide: (guide: OpaqueId) => void
  onRemoveTrack: () => void
}) {
  const [open, setOpen] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)
  const guide = guideOptions.find((n) => guideId && n.id.idx === guideId.idx && n.id.version === guideId.version)

  useEffect(() => {
    function handleOutsideClick(event: MouseEvent) {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', handleOutsideClick)
    return () => document.removeEventListener('mousedown', handleOutsideClick)
  }, [])

  return (
    <div className="flex flex-col gap-1 rounded-md border border-panel-border/60 p-1.5">
      <div className="flex items-center justify-between">
        <span className="text-[11px] font-medium text-ink-dim">Motion Path</span>
        {guideId && (
          <button onClick={onRemoveTrack} className="text-ink-dim hover:text-red-400" title="Remove Motion Path component">
            <TrashIcon />
          </button>
        )}
      </div>
      <div ref={rootRef} className="relative">
        <button
          onClick={() => setOpen((o) => !o)}
          className="flex w-full items-center justify-between gap-1 rounded-md bg-overlay/5 px-1.5 py-1 text-xs text-ink hover:bg-overlay/10"
        >
          <span className="truncate">{guideId ? (guide?.name ?? 'Motion Path') : '+ Pick a guide shape…'}</span>
          <ChevronDownIcon />
        </button>
        {open && (
          <div className="absolute left-0 top-full z-20 mt-1 w-full rounded-lg border border-panel-border bg-panel p-1 shadow-2xl backdrop-blur-panel">
            {guideOptions.length === 0 && <div className="px-2 py-1 text-[10px] text-ink-dim">No other shapes to guide along</div>}
            {guideOptions.map((n) => (
              <button
                key={`${n.id.idx}:${n.id.version}`}
                onClick={() => {
                  onSetGuide(n.id)
                  setOpen(false)
                }}
                className={`flex w-full items-center rounded-md px-2 py-1 text-left text-[11px] ${
                  guideId && guideId.idx === n.id.idx && guideId.version === n.id.version ? 'bg-accent/15 text-accent' : 'text-ink hover:bg-overlay/5'
                }`}
              >
                <span className="truncate">{n.name}</span>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

/** The Inspector's Id field — commit-on-blur/Enter text input, same draft-
 * then-sync-on-prop-change pattern `NumberField` already establishes (so
 * switching selection resyncs the shown text instead of leaking the
 * previous node's in-progress edit into the newly selected one). */
function IdField({ value, onCommit }: { value: string; onCommit: (value: string) => void }) {
  const [text, setText] = useState(value)

  useEffect(() => {
    setText(value)
  }, [value])

  const commit = () => {
    const trimmed = text.trim()
    if (trimmed) {
      onCommit(trimmed)
    } else {
      setText(value) // blank isn't a meaningful id — revert rather than clear it
    }
  }

  return (
    <input
      type="text"
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === 'Enter') e.currentTarget.blur()
      }}
      className="w-full min-w-0 rounded-md bg-overlay/5 px-1.5 py-1 text-xs text-ink outline-none focus:bg-overlay/10"
    />
  )
}

/** The Inspector's Classes field — removable tag chips (like HTML class
 * attributes, per the user's own framing) plus a small text box that
 * commits a new tag on Enter/blur. `onChange` fires the whole new list
 * immediately on every add/remove, same "no separate save step" convention
 * every other Inspector field already uses. */
function ClassTagsField({ value, onChange }: { value: string[]; onChange: (value: string[]) => void }) {
  const [draft, setDraft] = useState('')

  const addTag = () => {
    const trimmed = draft.trim()
    if (trimmed && !value.includes(trimmed)) onChange([...value, trimmed])
    setDraft('')
  }

  return (
    <div className="flex flex-wrap items-center gap-1 rounded-md bg-overlay/5 p-1.5">
      {value.map((tag) => (
        <span key={tag} className="flex items-center gap-1 rounded bg-overlay/10 px-1.5 py-0.5 text-[11px] text-ink">
          {tag}
          <button onClick={() => onChange(value.filter((t) => t !== tag))} className="text-ink-dim hover:text-red-400" title={`Remove "${tag}"`}>
            ×
          </button>
        </span>
      ))}
      <input
        type="text"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={addTag}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault()
            addTag()
          }
        }}
        placeholder="+ add class"
        className="min-w-16 flex-1 bg-transparent text-xs text-ink outline-none placeholder:text-ink-dim"
      />
    </div>
  )
}

const RADIANS_TO_DEGREES = 180 / Math.PI

export const GRADIENT_KINDS: GradientKind[] = ['Linear', 'Radial', 'Conic', 'Bilinear']
const BLEND_MODES: BlendMode[] = ['Normal', 'Multiply', 'Screen', 'Add']

/** A non-native `<select>` replacement — a button showing the current
 * value, opening a popover list of options on click. Same outside-click/
 * Escape-to-close convention as `ColorPicker.tsx`'s `ColorSwatch`, for the
 * same reason: this app doesn't use the OS's own dropdown chrome for
 * anything (color picker included), so a native `<select>` here would be
 * the one visibly inconsistent control in an otherwise fully custom panel.
 * Sized to match every other compact Inspector control (`NumberField`'s
 * own `px-1.5 py-1`), not the roomier native-`<select>`-sized padding an
 * earlier version of this had.
 *
 * The option list renders through a `createPortal` into `document.body`,
 * positioned via the trigger button's own `getBoundingClientRect()`, rather
 * than as a plain `position: absolute` child of the button. A dock panel's
 * body scrolls (`overflow-auto` — see `DockGroup`/`DockablePanel`), and
 * `overflow` clips *everything* positioned inside it, `position: absolute`
 * included — a dropdown opened near a panel's bottom edge (the Inspector's
 * asset-mode Effect field, in particular, sitting below Resize/Filter
 * Mode) had its option list clipped away by that ancestor, which in CSS
 * also removes it from hit-testing: the options weren't just invisible,
 * clicks/hovers on that clipped-away area genuinely couldn't reach them at
 * all. Escaping to `document.body` (real screen coordinates, `position:
 * fixed`) sidesteps any ancestor's overflow entirely, the standard fix for
 * this exact popover-inside-a-scroll-container class of bug. */
export function Dropdown<T extends string>({
  value,
  options,
  onChange,
  renderValue,
  renderOption,
}: {
  value: T
  options: T[]
  onChange: (value: T) => void
  /** Overrides the trigger button's own label — for when `T` is an opaque
   * key (e.g. a composite `"idx:version"` node id) rather than something
   * presentable on its own. Defaults to showing `value` itself verbatim. */
  renderValue?: (value: T) => ReactNode
  /** Overrides one option row's label in the popover list — same reason as
   * `renderValue`. Defaults to showing `option` itself verbatim. */
  renderOption?: (option: T) => ReactNode
}) {
  const [open, setOpen] = useState(false)
  const [menuRect, setMenuRect] = useState<{ top: number; left: number; width: number } | null>(null)
  const buttonRef = useRef<HTMLButtonElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const handlePointerDown = (e: PointerEvent) => {
      const target = e.target as Node
      if (buttonRef.current?.contains(target)) return
      if (menuRef.current?.contains(target)) return
      setOpen(false)
    }
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false)
    }
    // Recomputed on scroll/resize too — a `position: fixed` menu doesn't
    // move with its trigger button the way `position: absolute` would if
    // the panel it's in gets scrolled or resized while open.
    const reposition = () => {
      const rect = buttonRef.current?.getBoundingClientRect()
      if (rect) setMenuRect({ top: rect.bottom + 4, left: rect.left, width: rect.width })
    }
    window.addEventListener('pointerdown', handlePointerDown)
    window.addEventListener('keydown', handleKeyDown)
    window.addEventListener('scroll', reposition, true)
    window.addEventListener('resize', reposition)
    return () => {
      window.removeEventListener('pointerdown', handlePointerDown)
      window.removeEventListener('keydown', handleKeyDown)
      window.removeEventListener('scroll', reposition, true)
      window.removeEventListener('resize', reposition)
    }
  }, [open])

  const toggle = () => {
    if (!open) {
      const rect = buttonRef.current?.getBoundingClientRect()
      if (rect) setMenuRect({ top: rect.bottom + 4, left: rect.left, width: rect.width })
    }
    setOpen((o) => !o)
  }

  return (
    <div className="relative">
      <button
        ref={buttonRef}
        type="button"
        onClick={toggle}
        className="flex w-full items-center justify-between gap-1 rounded-md bg-overlay/5 px-1.5 py-1 text-xs text-ink outline-none hover:bg-overlay/8"
      >
        <span className="truncate">{renderValue ? renderValue(value) : value}</span>
        <span className="shrink-0 text-ink-dim">
          <ChevronDownIcon />
        </span>
      </button>
      {open &&
        menuRect &&
        createPortal(
          <div
            ref={menuRef}
            style={{ position: 'fixed', top: menuRect.top, left: menuRect.left, width: menuRect.width }}
            className="z-50 overflow-hidden rounded-md border border-panel-border bg-canvas shadow-xl"
          >
            {options.map((option) => (
              <button
                key={option}
                type="button"
                onClick={() => {
                  onChange(option)
                  setOpen(false)
                }}
                className={`block w-full px-1.5 py-1 text-left text-xs ${option === value ? 'bg-accent/15 text-accent' : 'text-ink hover:bg-overlay/8'}`}
              >
                {renderOption ? renderOption(option) : option}
              </button>
            ))}
          </div>,
          document.body,
        )}
    </div>
  )
}

/** A small, static preview swatch for the collapsed (non-editing) state —
 * a plain color chip for a solid fill, or a CSS `linear-gradient` built
 * from the real stops for a gradient one (a flattened, non-interactive
 * approximation good enough to recognize "yes, that's roughly my gradient"
 * at a glance — Radial/Conic/Bilinear all still render as a left-to-right
 * preview here, since reproducing their real shape at swatch size isn't
 * worth the effort this collapsed state is for). */
function FillPreviewChip({ fillColor, gradient }: { fillColor: [number, number, number, number]; gradient: GradientDef | null }) {
  const toRgba = (c: [number, number, number, number]) => `rgba(${Math.round(c[0] * 255)}, ${Math.round(c[1] * 255)}, ${Math.round(c[2] * 255)}, ${c[3]})`
  const style = gradient
    ? { backgroundImage: `linear-gradient(to right, ${[...gradient.stops].sort((a, b) => a.offset - b.offset).map((s) => toRgba(s.color)).join(', ')})` }
    : { backgroundColor: toRgba(fillColor) }
  return <div className="h-5 w-8 shrink-0 rounded border border-overlay/15" style={style} />
}

/** A `Shape`/`Text` node's whole fill — Affinity's Fill tool: a Solid/
 * Gradient toggle, a plain color swatch in the solid case, or a gradient-
 * kind dropdown plus an editable stop list in the gradient case. Collapsed
 * behind an "Edit" toggle by default (showing just a preview chip) — the
 * full editor (toggle + swatch/stop-list, several rows tall) would
 * otherwise always take up Inspector space even when nobody's actively
 * touching the fill. Distinct from `InstanceFields`' override swatch,
 * which layers on top of a variant/definition-default precedence chain a
 * plain node doesn't have (and doesn't support gradients at all — an
 * `Instance`'s color is resolved, not authored, per node).
 *
 * The on-canvas gradient handle (dragging the line/stops directly on the
 * shape) is a separate piece — see `GradientHandle.tsx` — this is purely
 * the Inspector's own numeric/list editor for the same `GradientDef`. */
function FillField({
  node,
  onSetFillColor,
  onSetGradient,
}: {
  node: NodeSummary
  onSetFillColor: (value: [number, number, number, number]) => void
  onSetGradient: (value: GradientDef | null) => void
}) {
  const [editing, setEditing] = useState(false)
  const fillColor = node.fillColor ?? [1, 1, 1, 1]
  const gradient = node.gradient

  const toggleGradient = (enabled: boolean) => {
    if (!enabled) {
      onSetGradient(null)
      return
    }
    // A sensible default spanning the node's own bounding box, fading
    // from its current flat color to the same color fully transparent —
    // matches Affinity's own "just turned on the gradient tool" default.
    onSetGradient({
      kind: 'Linear',
      start: { x: node.localMin.x, y: (node.localMin.y + node.localMax.y) / 2 },
      end: { x: node.localMax.x, y: (node.localMin.y + node.localMax.y) / 2 },
      stops: [
        { offset: 0, color: fillColor, midpoint: 0.5 },
        { offset: 1, color: [fillColor[0], fillColor[1], fillColor[2], 0], midpoint: 0.5 },
      ],
    })
  }

  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center justify-between gap-2">
        <span className="text-[11px] text-ink-dim">Fill</span>
        <div className="flex min-w-0 flex-1 items-center justify-end gap-2">
          {!editing && <FillPreviewChip fillColor={fillColor} gradient={gradient} />}
          <button
            onClick={() => setEditing((e) => !e)}
            title={editing ? 'Done editing fill' : 'Edit fill'}
            className={`flex shrink-0 items-center gap-1 rounded-md px-1.5 py-1 text-[10px] font-medium ${
              editing ? 'bg-accent/20 text-accent' : 'bg-overlay/5 text-ink-dim hover:bg-overlay/10 hover:text-ink'
            }`}
          >
            <EditIcon />
            {editing ? 'Done' : 'Edit'}
          </button>
        </div>
      </div>

      {editing && (
        <>
          {/* `Image` nodes have no gradient support this pass (Normal-blend
           * + tint-only — `Command::SetGradient` panics on an `Image` node
           * in the Rust layer), so the Solid/Gradient toggle only renders
           * for kinds that actually have one. */}
          {node.kind !== 'Image' && (
            <div className="flex gap-0.5 rounded-md bg-overlay/5 p-0.5">
              <button
                onClick={() => toggleGradient(false)}
                className={`flex-1 rounded px-2 py-0.5 text-[10px] ${!gradient ? 'bg-accent/20 text-accent' : 'text-ink-dim hover:text-ink'}`}
              >
                Solid
              </button>
              <button
                onClick={() => toggleGradient(true)}
                className={`flex-1 rounded px-2 py-0.5 text-[10px] ${gradient ? 'bg-accent/20 text-accent' : 'text-ink-dim hover:text-ink'}`}
              >
                Gradient
              </button>
            </div>
          )}

          {!gradient && <ColorSwatch value={fillColor} onChange={onSetFillColor} />}

          {gradient && (
            <>
              <Dropdown value={gradient.kind} options={GRADIENT_KINDS} onChange={(kind) => onSetGradient({ ...gradient, kind })} />
              <GradientStopList gradient={gradient} onChange={onSetGradient} />
            </>
          )}
        </>
      )}
    </div>
  )
}

/** The gradient stop editor: one row per stop (offset `0..1` + color
 * swatch + delete), sorted by ascending offset (the invariant
 * `GradientDef::stops`'s own doc comment asks the authoring UI to keep),
 * plus an "Add Stop" button that inserts a new one at the midpoint of
 * whichever gap is currently largest — the same "put it somewhere useful,
 * not always at the end" convenience Affinity's own stop-add offers. */
export function GradientStopList({ gradient, onChange }: { gradient: GradientDef; onChange: (value: GradientDef) => void }) {
  const setStop = (index: number, patch: Partial<GradientDef['stops'][number]>) => {
    const stops = gradient.stops.map((s, i) => (i === index ? { ...s, ...patch } : s))
    onChange({ ...gradient, stops: [...stops].sort((a, b) => a.offset - b.offset) })
  }

  const removeStop = (index: number) => {
    if (gradient.stops.length <= 2) return // a gradient needs at least two stops to mean anything
    onChange({ ...gradient, stops: gradient.stops.filter((_, i) => i !== index) })
  }

  const addStop = () => {
    const sorted = [...gradient.stops].sort((a, b) => a.offset - b.offset)
    let bestGapStart = 0
    let bestGapEnd = 1
    let bestGapSize = -1
    for (let i = 0; i < sorted.length - 1; i++) {
      const size = sorted[i + 1].offset - sorted[i].offset
      if (size > bestGapSize) {
        bestGapSize = size
        bestGapStart = sorted[i].offset
        bestGapEnd = sorted[i + 1].offset
      }
    }
    const offset = (bestGapStart + bestGapEnd) / 2
    // Interpolate the new stop's starting color from its two neighbors so
    // it visually blends in rather than popping in as a jarring flat color.
    const before = [...sorted].reverse().find((s) => s.offset <= offset) ?? sorted[0]
    const after = sorted.find((s) => s.offset >= offset) ?? sorted[sorted.length - 1]
    const span = Math.max(after.offset - before.offset, 0.0001)
    const t = (offset - before.offset) / span
    const color: [number, number, number, number] = [0, 1, 2, 3].map((i) => before.color[i] + (after.color[i] - before.color[i]) * t) as [
      number,
      number,
      number,
      number,
    ]
    onChange({ ...gradient, stops: [...gradient.stops, { offset, color, midpoint: 0.5 }].sort((a, b) => a.offset - b.offset) })
  }

  return (
    <div className="flex flex-col gap-0.5">
      {gradient.stops.map((stop, index) => (
        <div key={index} className="flex items-center gap-1">
          <div className="w-9 shrink-0">
            <ColorSwatch value={stop.color} onChange={(color) => setStop(index, { color })} />
          </div>
          <input
            type="number"
            min={0}
            max={1}
            step={0.01}
            value={Math.round(stop.offset * 100) / 100}
            onChange={(e) => setStop(index, { offset: Math.min(1, Math.max(0, Number(e.target.value) || 0)) })}
            className="w-10 min-w-0 rounded-md bg-overlay/8 px-1 py-0.5 text-[11px] text-ink outline-none"
          />
          <button
            onClick={() => removeStop(index)}
            disabled={gradient.stops.length <= 2}
            className="ml-auto shrink-0 rounded bg-red-500/10 px-1 py-0.5 text-red-300 hover:bg-red-500/20 disabled:opacity-30"
          >
            <TrashIcon />
          </button>
        </div>
      ))}
      <button onClick={addStop} className="mt-0.5 rounded-md bg-accent/15 px-2 py-0.5 text-[10px] font-medium text-accent hover:bg-accent/25">
        + Add Stop
      </button>
    </div>
  )
}

/** Content/size fields for a `Text` node — font reassignment isn't
 * offered here yet (there's no font-asset list/picker in the editor at
 * all currently; every `Text` node keeps whichever font it was created
 * with, deferred until that's built). */
/**
 * UI Button → Icon: a Material Symbols icon on the button. The icon is a
 * child Text node named `icon` drawn with the icon font (so it can be
 * moved, tinted and animated like any node); picking again just changes
 * which icon it shows, "Remove" deletes it.
 */
function ButtonIconField({
  editor,
  artboard,
  node,
  onDocumentChange,
}: {
  editor: WasmEditor | null
  artboard: ArtboardSummary | null
  node: NodeSummary
  onDocumentChange: (doc: DocumentSummary) => void
}) {
  const [busy, setBusy] = useState(false)
  const [open, setOpen] = useState(false)
  const same = (a: OpaqueId | null, b: OpaqueId) => !!a && a.idx === b.idx && a.version === b.version
  const iconNode = artboard?.nodes.find((n) => n.kind === 'Text' && n.name === 'icon' && same(n.parent, node.id)) ?? null
  const pick = async (name: string) => {
    if (!editor || !artboard) return
    if (iconNode) {
      onDocumentChange(setTextContent(editor, artboard.id, iconNode.id, name))
      return
    }
    setBusy(true)
    try {
      const font = await addGoogleFont(editor, iconFamily('Rounded'))
      const added = addTextNode(editor, artboard.id, node.id)
      renameNode(editor, artboard.id, added.newNode, 'icon')
      setTextContent(editor, artboard.id, added.newNode, name)
      setTextSize(editor, artboard.id, added.newNode, 40)
      // The engine draws text from its baseline; drop it by about half
      // its height so the icon sits on the button's centre.
      setNodeTranslation(editor, artboard.id, added.newNode, 0, -18)
      onDocumentChange(setTextFont(editor, artboard.id, added.newNode, font))
    } catch (err) {
      gameConsole.add('error', `Couldn't add the icon: ${err instanceof Error ? err.message : String(err)}`, { kind: 'Engine' })
    } finally {
      setBusy(false)
    }
  }
  return (
    <div className="flex flex-col gap-1 border-t border-panel-border/60 pt-1.5">
      <div className="flex items-center justify-between">
        <span className="text-[11px] text-ink-dim">{busy ? 'Adding icon…' : iconNode ? `Icon: ${iconNode.textContent}` : 'Icon'}</span>
        <div className="flex gap-1">
          {iconNode && editor && artboard && (
            <button type="button" className="rounded px-1.5 py-0.5 text-[10px] text-ink-dim hover:bg-overlay/10 hover:text-ink" onClick={() => onDocumentChange(deleteNode(editor, artboard.id, iconNode.id))}>
              Remove
            </button>
          )}
          <button type="button" className="rounded px-1.5 py-0.5 text-[10px] text-ink-dim hover:bg-overlay/10 hover:text-ink" onClick={() => setOpen((o) => !o)}>
            {open ? 'Done' : iconNode ? 'Change' : 'Add icon'}
          </button>
        </div>
      </div>
      {open && <IconPicker value={iconNode?.textContent ?? null} style="Rounded" onPick={pick} />}
    </div>
  )
}

function TextFields({
  node,
  fonts,
  onSetContent,
  onSetSize,
  onSetFontFamily,
}: {
  node: NodeSummary
  fonts: FontSummary[]
  onSetContent: (value: string) => void
  onSetSize: (value: number) => void
  /** Switches to a Google Font (downloaded into the document if needed). */
  onSetFontFamily: (family: string) => Promise<void>
}) {
  const [text, setText] = useState(node.textContent ?? '')
  const [busy, setBusy] = useState(false)
  useEffect(() => setText(node.textContent ?? ''), [node.textContent])
  const family = fonts.find((f) => node.textFont && f.id.idx === node.textFont.idx && f.id.version === node.textFont.version)?.name ?? null
  const icon = isIconFamily(family)
  const pickFamily = (next: string) => {
    setBusy(true)
    onSetFontFamily(next).finally(() => setBusy(false))
  }

  return (
    <div className="flex flex-col gap-1">
      {icon ? (
        <>
          <span className="text-[11px] text-ink-dim">Icon</span>
          <IconPicker
            value={node.textContent}
            style={(family!.replace('Material Symbols ', '') as IconStyle) ?? 'Rounded'}
            onPick={(name) => onSetContent(name)}
            onStyle={(style) => pickFamily(iconFamily(style))}
          />
        </>
      ) : (
        <>
          <span className="text-[11px] text-ink-dim">Text</span>
          <textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            onBlur={() => onSetContent(text)}
            rows={2}
            className="w-full resize-none rounded-md bg-overlay/5 px-2 py-1.5 text-xs text-ink outline-none"
          />
          <FontPicker value={family} busy={busy} onPick={pickFamily} extra={fonts.map((f) => f.name).filter((n) => !isIconFamily(n))} />
        </>
      )}
      <NumberField label="Size" value={node.textSizePx ?? 16} onCommit={onSetSize} />
    </div>
  )
}

/**
 * Contextual properties of the current selection (AN1MBYT3_BUILD_PLAN.md
 * §15) — renders and edits the real `Transform2D` of whichever node is
 * selected in `LayersPanel`/`App.tsx`. Edits call back up to `App.tsx`,
 * which sends them over Tauri IPC (`ipc/document.ts`'s `setNodeTranslation`/
 * `setNodeRotationDegrees`/`setNodeScale`) — every one of those goes through
 * `an1mbyt3_core::Document::apply`, a real undoable `Command` (§4), not a
 * direct field write, so edits made here are undoable from the toolbar's
 * undo button.
 *
 * Position and Scale render X/Y inline in one row rather than as two
 * stacked rows — this isn't just a layout preference, it matches
 * `an1mbyt3_core::Transform2D`'s actual shape: `translation` and `scale`
 * are each a single `glam::Vec2`, i.e. one vector value with two
 * components, not two independent properties (§4). Rotation is a lone
 * `f32` in radians on the wire, converted to/from degrees here for display
 * and editing — degrees are a UI concern, not something the engine's
 * `Transform2D` or the IPC `Command` layer needs to know about.
 *
 * `onDelete` is Phase 4's node-deletion entry point — calls through to
 * `WasmEditor::delete_node` (`Command::SetDeleted`), undoable exactly like
 * every transform edit above it.
 */
/** Resize/Filter Mode/Effect — the Inspector's asset-mode fields, shown
 * instead of the usual node-shaped form when a Resources-panel row is
 * selected (`InspectorPanel`'s `selectedAsset` prop). Commits the whole
 * `ImageProcessing` struct on every field edit (matches
 * `Command::SetPostEffects`'s own "whole struct, not per-field" convention
 * — there's no per-field undo to preserve here anyway, since asset
 * processing sits outside undo/redo entirely, same as a font's bytes).
 *
 * "Keep aspect" is local UI-only state (not part of `ImageProcessing`) —
 * it only decides whether editing one resize dimension also recomputes the
 * other; the actual authored width/height. `asset.width`/`height` (the
 * *source's* natural pixel dimensions, read from its PNG header) anchor the
 * aspect-ratio math and the "reset to original size" action.
 */
function AssetFields({ asset, onSetProcessing }: { asset: ImageAssetSummary; onSetProcessing: (processing: ImageProcessing) => void }) {
  const [keepAspect, setKeepAspect] = useState(true)
  const processing = asset.processing
  const naturalWidth = asset.width ?? 1
  const naturalHeight = asset.height ?? 1
  const [resizeWidth, resizeHeight] = processing.resize ?? [naturalWidth, naturalHeight]

  const effectKind = processing.effect.kind

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-1">
        <span className="text-[11px] text-ink-dim">Name</span>
        <p className="truncate rounded-md bg-overlay/5 px-1.5 py-1 text-xs text-ink" title={asset.name}>
          {asset.name}
        </p>
      </div>

      <div className="flex flex-col gap-1">
        <div className="flex items-center justify-between">
          <span className="text-[11px] text-ink-dim">Resize</span>
          <label className="flex items-center gap-1 text-[10px] text-ink-dim">
            <input type="checkbox" checked={keepAspect} onChange={(e) => setKeepAspect(e.target.checked)} />
            Keep aspect
          </label>
        </div>
        <div className="flex gap-1">
          <NumberField
            label="W"
            value={resizeWidth}
            onCommit={(w) => {
              const width = Math.max(1, Math.round(w))
              const height = keepAspect ? Math.max(1, Math.round(width * (naturalHeight / naturalWidth))) : resizeHeight
              onSetProcessing({ ...processing, resize: [width, height] })
            }}
          />
          <NumberField
            label="H"
            value={resizeHeight}
            onCommit={(h) => {
              const height = Math.max(1, Math.round(h))
              const width = keepAspect ? Math.max(1, Math.round(height * (naturalWidth / naturalHeight))) : resizeWidth
              onSetProcessing({ ...processing, resize: [width, height] })
            }}
          />
        </div>
        {processing.resize && (
          <button
            onClick={() => onSetProcessing({ ...processing, resize: null })}
            className="self-start text-[10px] text-ink-dim underline hover:text-ink"
          >
            Reset to original size ({naturalWidth}&times;{naturalHeight})
          </button>
        )}
      </div>

      <div className="flex flex-col gap-1">
        <span className="text-[11px] text-ink-dim">Filter Mode</span>
        <Dropdown value={processing.filter} options={['Linear', 'Nearest']} onChange={(filter) => onSetProcessing({ ...processing, filter })} />
      </div>

      <div className="flex flex-col gap-1">
        <span className="text-[11px] text-ink-dim">Effect</span>
        <Dropdown
          value={effectKind}
          options={['None', ...EFFECT_KINDS]}
          onChange={(kind) => onSetProcessing({ ...processing, effect: POST_EFFECT_DEFAULTS[kind] })}
        />
        {effectKind !== 'None' && (
          <div className="flex flex-col gap-1 rounded-md border border-overlay/10 bg-overlay/5 p-2">
            <EffectParams effect={processing.effect} onChange={(effect: PostEffect) => onSetProcessing({ ...processing, effect })} />
          </div>
        )}
      </div>
    </div>
  )
}

export function InspectorPanel({
  node,
  components,
  scripts,
  images,
  editor,
  selectedAsset,
  onSetElementId,
  onSetClasses,
  onSetClipChildren,
  onSetShapeImage,
  onSetStroke,
  onEditPath,
  editingPath,
  onReplayParticleSystem,
  onResizeTilemap,
  onSetTilemapCellSize,
  onSetActiveCamera,
  onSetNodeUnlit,
  onSetGroupYSort,
  onSetNodeZIndex,
  onSetNodeBehaviors,
  onAddScript,
  onSetScriptFields,
  onSetTranslation,
  onSetRotation,
  onSetScale,
  onSetAnchor,
  onSetBoneLength,
  onSetBlendMode,
  onSetFillColor,
  onSetGradient,
  onSetTextContent,
  onSetTextSize,
  onSetTextFontFamily,
  fonts,
  onSetInstanceVariant,
  onSetInstanceFillColorOverride,
  onSetImageProcessing,
  onSolveIk,
  onMakeRagdoll,
  onAddIkTarget,
  onBindSkin,
  materials,
  onEditMaterial,
  onSetNode3D,
  models,
  sounds,
  textFiles,
  onOpenFile,
  onDocumentChange,
  properties,
  sceneNames,
  onDelete,
  artboard,
  onSetArtboardSize,
  onSetArtboardGravity,
  onSetArtboardGravity3D,
  onSetArtboardResolutionScale,
  onSetArtboardScreen,
  onSetMainCamera,
  motionPathGuideOptions,
  onSetMotionPathGuide,
  onRemoveMotionPathTrack,
}: {
  node: NodeSummary | null
  /** Only meaningful when `node.kind === 'Instance'` — looked up by
   * `node.component` to list that definition's variants in the dropdown
   * below. */
  components: ComponentDefinitionSummary[]
  /** Every non-deleted script definition in the document — the Behaviors
   * section's "Add Behavior" picker and the attached-instance field labels
   * (looked up by `NodeBehavior.definition`). */
  scripts: ScriptDefinitionSummary[]
  /** Every image asset in the document — the Image field's picker grid
   * (`ImageAssetPicker`). */
  images: ImageAssetSummary[]
  /** Needed by `ImageAssetPicker`'s grid tiles, each a real pixel preview
   * fetched on demand (`ResourcesPanel`'s own `AssetThumbnail`). `null`
   * before the wasm module has finished loading — the Image field just
   * doesn't render yet in that brief window, same as `ResourcesPanel`'s own
   * null-editor handling. */
  editor: WasmEditor | null
  /** Non-null when the Resources panel has an asset selected — takes
   * precedence over `node` (mutually exclusive per `App.tsx`'s selection
   * state): renders `AssetFields` instead of the usual node-shaped form. */
  selectedAsset: ImageAssetSummary | null
  /** The Id field — `getCompByID`'s lookup key (`scriptRuntime.ts`), like an
   * HTML element's `id` attribute. Defaults to `shape(<serial>)` but is
   * freely renamable; doesn't itself enforce uniqueness (see
   * `an1mbyt3_core::Node::id`'s own doc comment). */
  onSetElementId: (value: string) => void
  /** The Classes field — `getCompByClass`'s selector tags, like HTML class
   * attributes. Rendered as removable tag chips plus an add box. */
  onSetClasses: (value: string[]) => void
  /** `Node::clip_children` — this node's own shape also clips whatever's
   * nested inside it, no separate mask child needed. Any node kind. */
  onSetClipChildren: (value: boolean) => void
  /** `Shape` nodes only — the Image field, `Node::shapeImage`. `null` clears
   * it, falling back to the flat/gradient fill. */
  onSetShapeImage: (value: OpaqueId | null) => void
  /** `Shape` nodes only — the Stroke section, `WasmEditor::set_shape_stroke`. */
  onSetStroke: (value: Stroke | null) => void
  /** `Shape` nodes: enter (or leave) on-canvas point editing for this
   * shape — the same mode double-clicking it enters. */
  onEditPath: (editing: boolean) => void
  /** Whether that point editing is active for this node. */
  editingPath: boolean
  /** `ParticleSystem` nodes only — the whole Particle System section,
   * `WasmEditor::set_particle_system`. */
  onSetParticleSystem: (value: ParticleSystemConfig) => void
  /** `ParticleSystem` nodes only — the Particle System section's "Replay"
   * button, `WasmEditor::replay_particle_system`. */
  onReplayParticleSystem: () => void
  /** `Tilemap` nodes only — the "Map Size" fields, `WasmEditor::resize_tilemap`. */
  onResizeTilemap: (mapWidth: number, mapHeight: number) => void
  /** `Tilemap` nodes only — the "Cell Size" field, `WasmEditor::set_tilemap_cell_size`. */
  onSetTilemapCellSize: (value: number) => void
  /** `Camera` nodes only — the whole Camera section (zoom/background/
   * follow), `WasmEditor::set_camera`. */
  onSetCamera: (value: CameraConfig) => void
  /** `Camera` nodes only — the Camera section's "Active Camera" checkbox,
   * `WasmEditor::set_active_camera`. `value: false` clears it (same as
   * passing a different camera's own checkbox a `true`, which implicitly
   * replaces whichever camera held the role before). */
  onSetActiveCamera: (value: boolean) => void
  /** `Light2D` nodes only — the whole Light 2D section, `WasmEditor::set_light2d`. */
  onSetLight2D: (value: Light2DConfig) => void
  /** `Trail` nodes only — the whole Trail Renderer section, `WasmEditor::set_trail`. */
  onSetTrail: (value: TrailConfig) => void
  /** Any node — attach/edit (`value`) or remove (`null`) Shadow Caster 2D. */
  onSetShadowCaster: (value: ShadowCasterConfig | null) => void
  /** Any node — `Node::unlit` (ignore 2D lighting). */
  onSetNodeUnlit: (value: boolean) => void
  /** `Group` nodes only — the "Y Sort" checkbox, `WasmEditor::set_group_y_sort`. */
  onSetGroupYSort: (value: boolean) => void
  /** Any node kind — the "Z Index" field, `WasmEditor::set_node_z_index`. */
  onSetNodeZIndex: (value: number) => void
  /** Any node kind — the Behaviors section's whole attached-behaviors list,
   * `WasmEditor::set_node_behaviors`. */
  onSetNodeBehaviors: (value: NodeBehaviorEntry[]) => void
  /** The Behaviors section's "+ New Script..." action, `WasmEditor::add_script`. */
  onAddScript: (name: string) => void
  /** The Behaviors section's "Edit Fields" authoring UI, `WasmEditor::set_script_fields`. */
  onSetScriptFields: (script: OpaqueId, fields: ScriptFieldDef[]) => void
  onSetTranslation: (x: number, y: number) => void
  onSetRotation: (degrees: number) => void
  onSetScale: (x: number, y: number) => void
  /** A raw, uncompensated anchor edit — matches After Effects' own "typing
   * a new Anchor Point value visibly shifts the layer" convention. The
   * on-canvas anchor handle's drag uses `commitNodeAnchor` directly (via
   * `App.tsx`), not this — see `wasmEditor.ts`'s `setNodeAnchor` doc
   * comment for why the two intentionally differ. */
  onSetAnchor: (x: number, y: number) => void
  /** Only meaningful when `node.kind === 'Bone'` — see that field's own
   * `NumberField` below, which is the only thing that calls it. */
  onSetBoneLength: (value: number) => void
  /** Only meaningful when `node.kind === 'Shape'` — see the blend-mode
   * dropdown below. */
  onSetBlendMode: (value: BlendMode) => void
  /** Only meaningful when `node.kind === 'Shape'` or `'Text'` — the plain
   * fill-color swatch (distinct from `onSetInstanceFillColorOverride`
   * below, which is an `Instance`-only override/precedence concept). */
  onSetFillColor: (value: [number, number, number, number]) => void
  /** Only meaningful when `node.kind === 'Shape'` or `'Text'` — the Fill
   * tool's Solid/Gradient toggle. */
  onSetGradient: (value: GradientDef | null) => void
  /** Only meaningful when `node.kind === 'Text'`. */
  onSetTextContent: (value: string) => void
  /** Only meaningful when `node.kind === 'Text'`. */
  onSetTextSize: (value: number) => void
  /** Only meaningful when `node.kind === 'Text'` — a Google Font family. */
  onSetTextFontFamily: (family: string) => Promise<void>
  /** Every font on the document. */
  fonts: FontSummary[]
  /** Only meaningful when `node.kind === 'Instance'` — `null` selects "no
   * variant" (falls back to the definition's default color). */
  onSetInstanceVariant: (value: string | null) => void
  /** Only meaningful when `node.kind === 'Instance'` — `null` clears the
   * override (falls back to `variant`/the definition's default). */
  onSetInstanceFillColorOverride: (value: [number, number, number, number] | null) => void
  /** Only meaningful when `selectedAsset` is non-null — commits the whole
   * `ImageProcessing` struct, see `AssetFields`' own doc comment. */
  onSetImageProcessing: (imageId: OpaqueId, processing: ImageProcessing) => void
  /** Only meaningful when `node.kind === 'Bone'` and `node.parent` is
   * non-null (a two-bone chain's "mid" bone) — solves IK for this bone and
   * its parent against a target the designer types in, per `editor.rs`'s
   * `solve_two_bone_ik` doc comment (a one-shot "solve and bake" tool, not a
   * saved constraint). */
  onSolveIk: (root: { idx: number; version: number }, targetX: number, targetY: number) => void
  /** Bone tools: physics for the whole rig under this bone. */
  onMakeRagdoll: (animated: boolean) => void
  /** Bone tools: target node at this bone's tip + IK constraint. */
  onAddIkTarget: (chainLength: number) => void
  /** Sprite Skin: (re)bind this image to the rig under `rootBone`. */
  onBindSkin: (rootBone: OpaqueId) => void
  /** Every material asset (the Material component's dropdown). */
  materials: MaterialSummary[]
  /** Open a material in the Shader tab. */
  onEditMaterial: (material: OpaqueId) => void
  /** The 3D section's edits (Node3D nodes). */
  onSetNode3D: (value: Node3D) => void
  /** Imported 3D models (Mesh → Imported Model). */
  models: { id: OpaqueId; name: string }[]
  /** Imported sounds (Audio Source clips). */
  sounds: AudioSummary[]
  /** Project code files, opening one in the Code tab, and document updates
   * from file edits made here (HTML UI → Move to files). */
  textFiles: TextFileSummary[]
  onOpenFile: (path: string) => void
  onDocumentChange: (doc: DocumentSummary) => void
  /** Every scene's properties, and every scene's name. */
  properties: PropertySummary[]
  sceneNames: string[]
  onDelete: () => void
  /** The active artboard, for the always-visible "Canvas" section below —
   * `null` only in the brief window before the wasm module/document has
   * loaded any artboard at all. */
  artboard: ArtboardSummary | null
  onSetArtboardSize: (width: number, height: number) => void
  /** Physics 2D gravity (world units / s²; Unity's Physics2D.gravity). */
  onSetArtboardGravity: (x: number, y: number) => void
  /** Physics 3D gravity, m/s². */
  onSetArtboardGravity3D: (x: number, y: number, z: number) => void
  onSetArtboardResolutionScale: (value: number) => void
  /** Fixed / Responsive screen settings. */
  onSetArtboardScreen: (value: ScreenSettings) => void
  /** The Canvas section's "Main Camera" picker — which Camera node
   * Playground views this scene through (`null` = none, free-roam view).
   * `WasmEditor::set_active_camera`. */
  onSetMainCamera: (camera: OpaqueId | null) => void
  /** Every other `Shape` node in `node`'s own artboard — the Motion Path
   * component's guide picker (a node can't usefully guide its own motion). */
  motionPathGuideOptions: NodeSummary[]
  onSetMotionPathGuide: (guide: OpaqueId) => void
  onRemoveMotionPathTrack: () => void
}) {
  const canvasSection = artboard && (
    <div className="flex flex-col gap-2 border-b border-panel-border/60 pb-2">
      <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">Canvas</span>
      <div className="flex flex-col gap-1">
        <span className="text-[11px] text-ink-dim">Size</span>
        <div className="flex gap-1">
          <NumberField label="W" value={artboard.width} onCommit={(width) => onSetArtboardSize(width, artboard.height)} />
          <NumberField label="H" value={artboard.height} onCommit={(height) => onSetArtboardSize(artboard.width, height)} />
        </div>
      </div>
      <div className="flex flex-col gap-1">
        <span className="text-[11px] text-ink-dim" title="Renders the whole canvas (and every post effect) in blocks of N native pixels, then upscales with hard edges — a chunky/retro look for the whole artboard (independent of canvas/window size), distinct from the per-shape Pixelate effect. 1px is native/off.">
          Resolution
        </span>
        <Slider min={1} max={64} step={1} value={artboard.resolutionScale} onChange={onSetArtboardResolutionScale} />
        <span className="text-right text-[10px] tabular-nums text-ink-dim">{artboard.resolutionScale}px</span>
      </div>
      <div className="flex flex-col gap-1">
        <span className="text-[11px] text-ink-dim" title="Physics 2D gravity in world units per second² (+Y is up). Each Rigidbody 2D multiplies it by its Gravity Scale; set Y to 0 for top-down games.">
          Gravity
        </span>
        <div className="flex gap-1">
          <NumberField label="X" value={artboard.gravity.x} onCommit={(x) => onSetArtboardGravity(x, artboard.gravity.y)} />
          <NumberField label="Y" value={artboard.gravity.y} onCommit={(y) => onSetArtboardGravity(artboard.gravity.x, y)} />
        </div>
      </div>
      <div className="flex flex-col gap-1">
        <span className="text-[11px] text-ink-dim" title="Physics 3D gravity in m/s² (+Y is up) for Rigidbody 3D bodies. Earth is (0, -9.81, 0).">
          Gravity 3D
        </span>
        <div className="flex gap-1">
          <NumberField label="X" value={artboard.gravity3d[0]} onCommit={(x) => onSetArtboardGravity3D(x, artboard.gravity3d[1], artboard.gravity3d[2])} />
          <NumberField label="Y" value={artboard.gravity3d[1]} onCommit={(y) => onSetArtboardGravity3D(artboard.gravity3d[0], y, artboard.gravity3d[2])} />
          <NumberField label="Z" value={artboard.gravity3d[2]} onCommit={(z) => onSetArtboardGravity3D(artboard.gravity3d[0], artboard.gravity3d[1], z)} />
        </div>
      </div>
      <ScreenFields screen={artboard.screen} onChange={onSetArtboardScreen} />
      <MainCameraField artboard={artboard} onChange={onSetMainCamera} />
      <div className="flex flex-col gap-1 border-t border-panel-border/60 pt-2">
        <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">Properties</span>
        <ScenePropertiesFields editor={editor} artboard={artboard} properties={properties} onDocumentChange={onDocumentChange} />
      </div>
    </div>
  )

  if (selectedAsset) {
    return (
      <div className="flex flex-col gap-2">
        {canvasSection}
        <AssetFields asset={selectedAsset} onSetProcessing={(processing) => onSetImageProcessing(selectedAsset.id, processing)} />
      </div>
    )
  }

  if (!node) {
    return (
      <div className="flex flex-col gap-2">
        {canvasSection}
        <p className="text-xs text-ink-dim">No selection.</p>
      </div>
    )
  }

  const { translation, rotation, scale, anchor } = node.transform

  // A vector shape's own look — fill, outline, and its points — comes first:
  // it's what a shape is edited for. (It used to sit below the Canvas
  // settings, Id, Classes, transform and Motion Path — far enough down to
  // read as missing.) The Canvas settings show when nothing is selected.
  const shapeSection = node.kind === 'Shape' && (
    <div className="flex flex-col gap-2 border-b border-panel-border/60 pb-2">
      <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">Shape</span>
      {node.fillColor !== null && <FillField node={node} onSetFillColor={onSetFillColor} onSetGradient={onSetGradient} />}
      <StrokeField stroke={node.stroke} onChange={onSetStroke} />
      <div className="flex items-center gap-2">
        <span className="text-[11px] text-ink-dim">Path</span>
        <span className="flex-1 text-[11px] tabular-nums text-ink-dim">
          {node.path ? `${pathPointCount(node.path)} points` : ''}
        </span>
        <button
          onClick={() => onEditPath(!editingPath)}
          title="Move, add (click the outline) and delete (Delete key) points; double-click a point to make it smooth or a corner. Same as double-clicking the shape."
          className={`rounded px-2 py-1 text-[11px] ${editingPath ? 'bg-accent text-white' : 'bg-overlay/10 text-ink hover:bg-overlay/20'}`}
        >
          {editingPath ? 'Done' : 'Edit Points'}
        </button>
      </div>
    </div>
  )

  return (
    <div className="flex flex-col gap-2">
      {shapeSection}
      <div className="flex flex-col gap-1">
        <span className="text-[11px] text-ink-dim">Id</span>
        <IdField value={node.elementId} onCommit={onSetElementId} />
      </div>

      <div className="flex flex-col gap-1">
        <span className="text-[11px] text-ink-dim">Classes</span>
        <ClassTagsField value={node.classes} onChange={onSetClasses} />
      </div>

      <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
        <input type="checkbox" checked={node.clipChildren} onChange={(e) => onSetClipChildren(e.target.checked)} />
        Clip children to this shape
      </label>

      <div className="flex items-center gap-2">
        <span className="w-20 shrink-0 text-[11px] text-ink-dim">Z Index</span>
        <NumberField label="z" value={node.zIndex} step={1} onCommit={onSetNodeZIndex} />
      </div>

      {node.kind === 'Group' && node.groupYSort !== null && (
        <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
          <input type="checkbox" checked={node.groupYSort} onChange={(e) => onSetGroupYSort(e.target.checked)} />
          Y Sort (dynamic depth by position)
        </label>
      )}

      {/* 3D nodes have their own 3D transform; the 2D one doesn't apply. */}
      {node.kind === 'Node3D' && node.node3d && <Node3DFields value={node.node3d} onChange={onSetNode3D} materials={materials} onEditMaterial={onEditMaterial} models={models} editor={editor} />}
      {node.kind !== 'Node3D' && (
        <>
      <div className="flex flex-col gap-1">
        <span className="text-[11px] text-ink-dim">Position</span>
        <div className="flex gap-1">
          <NumberField label="X" value={translation.x} onCommit={(x) => onSetTranslation(x, translation.y)} />
          <NumberField label="Y" value={translation.y} onCommit={(y) => onSetTranslation(translation.x, y)} />
        </div>
      </div>

      <div className="flex flex-col gap-1">
        <span className="text-[11px] text-ink-dim">Anchor</span>
        <div className="flex gap-1">
          <NumberField label="X" value={anchor.x} onCommit={(x) => onSetAnchor(x, anchor.y)} />
          <NumberField label="Y" value={anchor.y} onCommit={(y) => onSetAnchor(anchor.x, y)} />
        </div>
      </div>

      <div className="flex flex-col gap-1">
        <span className="text-[11px] text-ink-dim">Rotation</span>
        <NumberField
          label="°"
          value={rotation * RADIANS_TO_DEGREES}
          onCommit={(degrees) => onSetRotation(degrees)}
        />
      </div>

      <div className="flex flex-col gap-1">
        <span className="text-[11px] text-ink-dim">Scale %</span>
        <div className="flex gap-1">
          <NumberField
            label="X"
            value={scale.x * 100}
            onCommit={(percent) => onSetScale(percent / 100, scale.y)}
          />
          <NumberField
            label="Y"
            value={scale.y * 100}
            onCommit={(percent) => onSetScale(scale.x, percent / 100)}
          />
        </div>
      </div>

      <MotionPathField
        guideId={node.tracks.find((t) => t.property === 'MotionPath')?.guide ?? null}
        guideOptions={motionPathGuideOptions}
        onSetGuide={onSetMotionPathGuide}
        onRemoveTrack={onRemoveMotionPathTrack}
      />
        </>
      )}

      {node.kind === 'Shape' && node.blendMode !== null && (
        <div className="flex flex-col gap-1">
          <span className="text-[11px] text-ink-dim">Blend Mode</span>
          <Dropdown value={node.blendMode} options={BLEND_MODES} onChange={onSetBlendMode} />
        </div>
      )}

      {node.kind === 'Bone' && node.boneLength !== null && (
        <div className="flex flex-col gap-1">
          <span className="text-[11px] text-ink-dim">Bone Length</span>
          <NumberField label="L" value={node.boneLength} onCommit={onSetBoneLength} />
          <span className="mt-1 text-[11px] text-ink-dim">Rig Tools</span>
          <div className="grid grid-cols-2 gap-1">
            <button
              onClick={() => onAddIkTarget(2)}
              title="Adds a target at this bone's tip; this bone and its parent bend to reach it (drag the target)."
              className="rounded bg-overlay/10 px-2 py-1 text-[11px] text-ink hover:bg-overlay/20"
            >
              Add IK Target
            </button>
            <button
              onClick={() => onAddIkTarget(64)}
              title="Like Add IK Target, but the whole chain up to the root bone bends (FABRIK): tails, tentacles."
              className="rounded bg-overlay/10 px-2 py-1 text-[11px] text-ink hover:bg-overlay/20"
            >
              IK Whole Chain
            </button>
            <button
              onClick={() => onMakeRagdoll(false)}
              title="Rigidbody + capsule + hinge on every bone under this one. Goes limp as soon as Playground starts."
              className="rounded bg-overlay/10 px-2 py-1 text-[11px] text-ink hover:bg-overlay/20"
            >
              Make Ragdoll
            </button>
            <button
              onClick={() => onMakeRagdoll(true)}
              title="Same, but Kinematic: follows animation/IK until a script calls ref.ragdoll(true)."
              className="rounded bg-overlay/10 px-2 py-1 text-[11px] text-ink hover:bg-overlay/20"
            >
              Animated Ragdoll
            </button>
          </div>
        </div>
      )}

      {node.fillColor !== null && node.kind !== 'Node3D' && node.kind !== 'Shape' && <FillField node={node} onSetFillColor={onSetFillColor} onSetGradient={onSetGradient} />}

      {node.kind === 'Shape' && editor && (
        <div className="flex flex-col gap-1">
          <span className="text-[11px] text-ink-dim">Image</span>
          <ImageAssetPicker editor={editor} images={images} value={node.shapeImage} onChange={onSetShapeImage} />
        </div>
      )}


      {node.kind === 'Tilemap' && node.tilemap !== null && (
        <TilemapFields config={node.tilemap} onResize={onResizeTilemap} onSetCellSize={onSetTilemapCellSize} />
      )}

      {/* Unity: the sprite's Material — Lit (2D lights affect it) or Unlit. */}
      {node.kind !== 'Bone' && node.kind !== 'Node3D' && node.light2d === null && node.camera === null && <MaterialField unlit={node.unlit} onSetUnlit={onSetNodeUnlit} />}

      {node.kind === 'Text' && <TextFields node={node} fonts={fonts} onSetContent={onSetTextContent} onSetSize={onSetTextSize} onSetFontFamily={onSetTextFontFamily} />}

      {node.kind === 'Bone' && node.parent !== null && <IkTargetFields root={node.parent} onSolveIk={onSolveIk} />}

      {node.kind === 'Instance' && node.component !== null && (
        <InstanceFields
          node={node}
          component={components.find((c) => c.id.idx === node.component?.idx && c.id.version === node.component?.version) ?? null}
          onSetVariant={onSetInstanceVariant}
          onSetFillColorOverride={onSetInstanceFillColorOverride}
        />
      )}

      <BehaviorsFields
        behaviors={node.behaviors}
        scripts={scripts}
        onChange={onSetNodeBehaviors}
        onAddScript={onAddScript}
        onSetScriptFields={onSetScriptFields}
        node={node}
        artboard={artboard}
        images={images}
        editor={editor}
        onSetActiveCamera={onSetActiveCamera}
        onReplayParticleSystem={onReplayParticleSystem}
        onBindSkin={onBindSkin}
        materials={materials}
        onEditMaterial={onEditMaterial}
        sounds={sounds}
        textFiles={textFiles}
        onOpenFile={onOpenFile}
        onDocumentChange={onDocumentChange}
        properties={properties}
        sceneNames={sceneNames}
      />

      <button
        onClick={onDelete}
        className="mt-1 flex items-center justify-center gap-1.5 rounded-md bg-overlay/5 px-2 py-1.5 text-xs text-ink-dim hover:bg-red-500/10 hover:text-red-400"
      >
        <TrashIcon />
        Delete
      </button>
    </div>
  )
}

/** The IK target fields shown for a two-bone chain's "mid" bone (see
 * `InspectorPanel`'s `onSolveIk` doc comment). Local, uncommitted target
 * coordinates — typing a target doesn't move anything until "Solve IK" is
 * pressed, unlike every `NumberField` above which commits per-field. A
 * click-drag target handle on the canvas would be the nicer version of
 * this interaction; this numeric-entry form is the first working slice. */
function IkTargetFields({ root, onSolveIk }: { root: { idx: number; version: number }; onSolveIk: (root: { idx: number; version: number }, targetX: number, targetY: number) => void }) {
  const [targetX, setTargetX] = useState('100')
  const [targetY, setTargetY] = useState('0')

  return (
    <div className="flex flex-col gap-1">
      <span className="text-[11px] text-ink-dim">IK Target</span>
      <div className="flex gap-1">
        <div className="flex flex-1 items-center gap-1 rounded-md bg-overlay/5 px-1.5 py-1">
          <span className="text-[11px] text-ink-dim">X</span>
          <input
            type="text"
            inputMode="decimal"
            value={targetX}
            onChange={(e) => setTargetX(e.target.value)}
            className="w-full min-w-0 bg-transparent text-xs text-ink outline-none"
          />
        </div>
        <div className="flex flex-1 items-center gap-1 rounded-md bg-overlay/5 px-1.5 py-1">
          <span className="text-[11px] text-ink-dim">Y</span>
          <input
            type="text"
            inputMode="decimal"
            value={targetY}
            onChange={(e) => setTargetY(e.target.value)}
            className="w-full min-w-0 bg-transparent text-xs text-ink outline-none"
          />
        </div>
      </div>
      <button
        onClick={() => onSolveIk(root, Number.parseFloat(targetX) || 0, Number.parseFloat(targetY) || 0)}
        className="rounded-md bg-accent/15 px-2 py-1.5 text-xs font-medium text-accent hover:bg-accent/25"
      >
        Solve IK
      </button>
    </div>
  )
}

/** Per-instance fields for a `NodeKind::Instance` node — a variant dropdown
 * (built from the referenced `ComponentDefinitionSummary`'s own variants)
 * and a direct color-override checkbox+swatch. `component` is `null` only
 * if the instance references a deleted/missing definition (a dangling
 * reference — see `editor.rs`'s `delete_component` doc comment for why that
 * doesn't get cleaned up automatically); nothing renders in that case
 * beyond the resolved-color readout, since there's no variant list to offer. */
function InstanceFields({
  node,
  component,
  onSetVariant,
  onSetFillColorOverride,
}: {
  node: NodeSummary
  component: ComponentDefinitionSummary | null
  onSetVariant: (value: string | null) => void
  onSetFillColorOverride: (value: [number, number, number, number] | null) => void
}) {
  const overrideColor = node.fillColorOverride ?? node.resolvedFillColor ?? [1, 1, 1, 1]

  return (
    <div className="flex flex-col gap-1">
      <span className="text-[11px] text-ink-dim">Component</span>
      {component && component.variants.length > 0 && (
        <select
          value={node.variant ?? ''}
          onChange={(e) => onSetVariant(e.target.value === '' ? null : e.target.value)}
          className="w-full rounded-md bg-overlay/5 px-2 py-1.5 text-xs text-ink outline-none"
        >
          <option value="">(default)</option>
          {component.variants.map((v) => (
            <option key={v.name} value={v.name}>
              {v.name}
            </option>
          ))}
        </select>
      )}
      <div className="flex items-center gap-2 rounded-md bg-overlay/5 px-2 py-1">
        <input
          type="checkbox"
          checked={node.fillColorOverride !== null}
          onChange={(e) => onSetFillColorOverride(e.target.checked ? overrideColor : null)}
        />
        <span className="text-[11px] text-ink-dim">Color Override</span>
        <div className="ml-auto w-16">
          <ColorSwatch value={overrideColor} disabled={node.fillColorOverride === null} onChange={(value) => onSetFillColorOverride(value)} />
        </div>
      </div>
    </div>
  )
}
