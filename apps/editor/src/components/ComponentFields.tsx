import { audio, type SoundHandle } from '../game/audio'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import type { WasmEditor } from '../ipc/wasmEditor'
import type {
  AudioSourceConfig,
  AudioSummary,
  UiAnchorConfig,
  UiButtonConfig,
  UiFillDirection,
  UiScrollbarConfig,
  UiScrollViewConfig,
  UiSliderConfig,
  UiStackConfig,
  UiStateStyle,
  UiToggleConfig,
  UiTransition,
  ImageAssetSummary,
  BoxCollider2DConfig,
  CapsuleCollider2DConfig,
  CircleCollider2DConfig,
  ComponentKind,
  HingeJoint2DConfig,
  IkConstraint2DConfig,
  NodeSummary,
  OpaqueId,
  SpriteSkin2DConfig,
  Liquid2DConfig,
  PhysicsMaterial2DConfig,
  PolygonCollider2DConfig,
  Rigidbody2DConfig,
  ShadowCasterConfig,
  Vec2Tuple,
} from '../ipc/document'
import { Dropdown, NumberField } from './InspectorPanel'
import { PlusIcon, TrashIcon } from './icons'
import { ColorSwatch } from './ColorPicker'
import { ImageAssetPicker } from './ImageAssetPicker'

/**
 * Unity-style component editors that aren't big enough for their own file
 * (Rigidbody 2D, the four Collider 2Ds, Shadow Caster 2D) plus the
 * Inspector's sectioned "Add Component" menu. Every editor takes the whole
 * component config and reports the whole edited config, same convention as
 * every other component section.
 */

const clamp = (v: number, min: number, max: number) => Math.min(max, Math.max(min, v))

function Row({ label, title, children }: { label: string; title?: string; children: ReactNode }) {
  return (
    <div className="flex items-center gap-2" title={title}>
      <span className="w-24 shrink-0 text-[11px] text-ink-dim">{label}</span>
      <div className="flex min-w-0 flex-1 items-center gap-1">{children}</div>
    </div>
  )
}

function Check({ checked, onChange, children }: { checked: boolean; onChange: (value: boolean) => void; children: ReactNode }) {
  return (
    <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      {children}
    </label>
  )
}

function VecFields({ value, onChange, min, max, step }: { value: Vec2Tuple; onChange: (value: Vec2Tuple) => void; min?: number; max?: number; step?: number }) {
  const fix = (v: number) => Math.min(max ?? Infinity, Math.max(min ?? -Infinity, v))
  return (
    <>
      <NumberField label="X" value={value[0]} step={step} onCommit={(x) => onChange([fix(x), value[1]])} />
      <NumberField label="Y" value={value[1]} step={step} onCommit={(y) => onChange([value[0], fix(y)])} />
    </>
  )
}

/** Is Trigger + the inline Physics Material 2D, shared by every collider. */
function ColliderCommon({
  isTrigger,
  material,
  onChange,
}: {
  isTrigger: boolean
  material: PhysicsMaterial2DConfig
  onChange: (patch: { isTrigger?: boolean; material?: PhysicsMaterial2DConfig }) => void
}) {
  return (
    <>
      <Check checked={isTrigger} onChange={(v) => onChange({ isTrigger: v })}>
        Is Trigger (detects overlaps, doesn&apos;t push)
      </Check>
      <Row label="Friction">
        <NumberField label="" value={material.friction} step={0.05} onCommit={(v) => onChange({ material: { ...material, friction: Math.max(0, v) } })} />
      </Row>
      <Row label="Bounciness">
        <NumberField label="" value={material.bounciness} step={0.05} onCommit={(v) => onChange({ material: { ...material, bounciness: clamp(v, 0, 1) } })} />
      </Row>
    </>
  )
}

/** Unity's `Rigidbody 2D`. */
export function Rigidbody2DFields({ config, onChange }: { config: Rigidbody2DConfig; onChange: (value: Rigidbody2DConfig) => void }) {
  const update = (patch: Partial<Rigidbody2DConfig>) => onChange({ ...config, ...patch })
  const dynamic = config.bodyType === 'Dynamic'
  return (
    <div className="flex flex-col gap-1.5">
      <Row label="Body Type" title="Dynamic: simulated. Kinematic: moved by script, pushes others. Static: never moves.">
        <Dropdown value={config.bodyType} options={['Dynamic', 'Kinematic', 'Static'] as Rigidbody2DConfig['bodyType'][]} onChange={(bodyType) => update({ bodyType })} />
      </Row>
      {dynamic && (
        <>
          <Row label="Mass">
            <NumberField label="kg" value={config.mass} step={0.1} onCommit={(mass) => update({ mass: Math.max(0.0001, mass) })} />
          </Row>
          <Row label="Linear Drag">
            <NumberField label="" value={config.linearDrag} step={0.1} onCommit={(linearDrag) => update({ linearDrag: Math.max(0, linearDrag) })} />
          </Row>
          <Row label="Angular Drag">
            <NumberField label="" value={config.angularDrag} step={0.05} onCommit={(angularDrag) => update({ angularDrag: Math.max(0, angularDrag) })} />
          </Row>
          <Row label="Gravity Scale" title="Multiplies the scene's gravity (0 = floats, top-down games).">
            <NumberField label="×" value={config.gravityScale} step={0.1} onCommit={(gravityScale) => update({ gravityScale })} />
          </Row>
          <Row label="Collision" title="Continuous stops fast objects tunneling through thin walls.">
            <Dropdown
              value={config.collisionDetection}
              options={['Discrete', 'Continuous'] as Rigidbody2DConfig['collisionDetection'][]}
              onChange={(collisionDetection) => update({ collisionDetection })}
            />
          </Row>
          <span className="text-[10px] uppercase tracking-wide text-ink-dim/70">Constraints</span>
          <div className="flex flex-wrap gap-x-3 gap-y-1">
            <Check checked={config.freezePositionX} onChange={(freezePositionX) => update({ freezePositionX })}>
              Freeze X
            </Check>
            <Check checked={config.freezePositionY} onChange={(freezePositionY) => update({ freezePositionY })}>
              Freeze Y
            </Check>
            <Check checked={config.freezeRotation} onChange={(freezeRotation) => update({ freezeRotation })}>
              Freeze Rotation
            </Check>
          </div>
        </>
      )}
    </div>
  )
}

export function BoxCollider2DFields({ config, onChange }: { config: BoxCollider2DConfig; onChange: (value: BoxCollider2DConfig) => void }) {
  return (
    <div className="flex flex-col gap-1.5">
      <Row label="Offset">
        <VecFields value={config.offset} onChange={(offset) => onChange({ ...config, offset })} />
      </Row>
      <Row label="Size">
        <VecFields value={config.size} min={0.1} onChange={(size) => onChange({ ...config, size })} />
      </Row>
      <ColliderCommon isTrigger={config.isTrigger} material={config.material} onChange={(patch) => onChange({ ...config, ...patch })} />
    </div>
  )
}

export function CircleCollider2DFields({ config, onChange }: { config: CircleCollider2DConfig; onChange: (value: CircleCollider2DConfig) => void }) {
  return (
    <div className="flex flex-col gap-1.5">
      <Row label="Offset">
        <VecFields value={config.offset} onChange={(offset) => onChange({ ...config, offset })} />
      </Row>
      <Row label="Radius">
        <NumberField label="r" value={config.radius} onCommit={(radius) => onChange({ ...config, radius: Math.max(0.1, radius) })} />
      </Row>
      <ColliderCommon isTrigger={config.isTrigger} material={config.material} onChange={(patch) => onChange({ ...config, ...patch })} />
    </div>
  )
}

export function CapsuleCollider2DFields({ config, onChange }: { config: CapsuleCollider2DConfig; onChange: (value: CapsuleCollider2DConfig) => void }) {
  return (
    <div className="flex flex-col gap-1.5">
      <Row label="Offset">
        <VecFields value={config.offset} onChange={(offset) => onChange({ ...config, offset })} />
      </Row>
      <Row label="Size">
        <VecFields value={config.size} min={0.1} onChange={(size) => onChange({ ...config, size })} />
      </Row>
      <Row label="Direction">
        <Dropdown value={config.direction} options={['Vertical', 'Horizontal'] as CapsuleCollider2DConfig['direction'][]} onChange={(direction) => onChange({ ...config, direction })} />
      </Row>
      <ColliderCommon isTrigger={config.isTrigger} material={config.material} onChange={(patch) => onChange({ ...config, ...patch })} />
    </div>
  )
}

export function PolygonCollider2DFields({ config, onChange }: { config: PolygonCollider2DConfig; onChange: (value: PolygonCollider2DConfig) => void }) {
  return (
    <div className="flex flex-col gap-1.5">
      <Row label="Offset">
        <VecFields value={config.offset} onChange={(offset) => onChange({ ...config, offset })} />
      </Row>
      <span className="text-[11px] text-ink-dim">Points (convex hull is used)</span>
      {config.points.map((p, i) => (
        <div key={i} className="flex items-center gap-1">
          <VecFields value={p} onChange={(point) => onChange({ ...config, points: config.points.map((q, j) => (j === i ? point : q)) })} />
          <button
            disabled={config.points.length <= 3}
            onClick={() => onChange({ ...config, points: config.points.filter((_, j) => j !== i) })}
            className="shrink-0 rounded bg-red-500/10 px-1.5 py-0.5 text-[10px] text-red-300 hover:bg-red-500/20 disabled:opacity-30"
          >
            <TrashIcon />
          </button>
        </div>
      ))}
      <button
        onClick={() => {
          const a = config.points[config.points.length - 1]
          const b = config.points[0]
          onChange({ ...config, points: [...config.points, [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2]] })
        }}
        className="self-start rounded-md bg-accent/15 px-2 py-1 text-[11px] font-medium text-accent hover:bg-accent/25"
      >
        + Point
      </button>
      <ColliderCommon isTrigger={config.isTrigger} material={config.material} onChange={(patch) => onChange({ ...config, ...patch })} />
    </div>
  )
}

const CASTING_LABEL: Record<ShadowCasterConfig['castingOption'], string> = {
  CastShadow: 'Cast Shadow',
  SelfShadow: 'Self Shadow',
  CastAndSelfShadow: 'Cast and Self Shadow',
}

export function ShadowCaster2DFields({ config, onChange }: { config: ShadowCasterConfig; onChange: (value: ShadowCasterConfig) => void }) {
  return (
    <div className="flex flex-col gap-1.5">
      <Row label="Casting">
        <Dropdown
          value={config.castingOption}
          options={['CastShadow', 'SelfShadow', 'CastAndSelfShadow'] as ShadowCasterConfig['castingOption'][]}
          renderValue={(v) => CASTING_LABEL[v]}
          renderOption={(v) => CASTING_LABEL[v]}
          onChange={(castingOption) => onChange({ ...config, castingOption })}
        />
      </Row>
      <Row label="Trim Edge" title="Shrinks the caster outline inward (world units).">
        <NumberField label="" value={config.trimEdge} step={0.5} onCommit={(trimEdge) => onChange({ ...config, trimEdge: Math.max(0, trimEdge) })} />
      </Row>
      <span className="text-[10px] leading-snug text-ink-dim/70">Casts from the visible pixels (sprites/tilemaps) or the shape outline. Lights need Shadows on.</span>
    </div>
  )
}

const idKey = (id: OpaqueId | null) => (id ? `${id.idx}:${id.version}` : 'none')

/** Pick a node from the scene (or "none"). */
function NodePicker({ value, nodes, noneLabel, onChange }: { value: OpaqueId | null; nodes: NodeSummary[]; noneLabel: string; onChange: (id: OpaqueId | null) => void }) {
  const byKey = new Map(nodes.map((n) => [idKey(n.id), n]))
  return (
    <Dropdown
      value={idKey(value)}
      options={['none', ...nodes.map((n) => idKey(n.id))]}
      renderValue={(k) => (k === 'none' ? noneLabel : (byKey.get(k)?.name ?? '(missing)'))}
      renderOption={(k) => (k === 'none' ? noneLabel : (byKey.get(k)?.name ?? k))}
      onChange={(k) => onChange(k === 'none' ? null : (byKey.get(k)?.id ?? null))}
    />
  )
}

/** Unity's `Hinge Joint 2D`. */
export function HingeJoint2DFields({ config, nodes, onChange }: { config: HingeJoint2DConfig; nodes: NodeSummary[]; onChange: (value: HingeJoint2DConfig) => void }) {
  const update = (patch: Partial<HingeJoint2DConfig>) => onChange({ ...config, ...patch })
  const bodies = nodes.filter((n) => n.behaviors.some((b) => b.behavior.kind === 'Rigidbody2D'))
  return (
    <div className="flex flex-col gap-1.5">
      <Row label="Connected" title="The body this one hinges to. Auto: the nearest parent with a Rigidbody 2D, or the world.">
        <NodePicker value={config.connectedBody} nodes={bodies} noneLabel="Auto (parent / world)" onChange={(connectedBody) => update({ connectedBody })} />
      </Row>
      <Row label="Anchor" title="Hinge point in this node's own space (a bone's joint is 0, 0).">
        <VecFields value={config.anchor} onChange={(anchor) => update({ anchor })} />
      </Row>
      <Check checked={config.useLimits} onChange={(useLimits) => update({ useLimits })}>
        Use Limits
      </Check>
      {config.useLimits && (
        <Row label="Angle" title="Allowed swing in degrees, relative to the authored pose.">
          <NumberField label="min" value={config.lowerAngle} step={5} onCommit={(lowerAngle) => update({ lowerAngle })} />
          <NumberField label="max" value={config.upperAngle} step={5} onCommit={(upperAngle) => update({ upperAngle })} />
        </Row>
      )}
      <Check checked={config.useMotor} onChange={(useMotor) => update({ useMotor })}>
        Use Motor
      </Check>
      {config.useMotor && (
        <Row label="Motor" title="Target speed (deg/s) and the most torque it may use.">
          <NumberField label="deg/s" value={config.motorSpeed} step={10} onCommit={(motorSpeed) => update({ motorSpeed })} />
          <NumberField label="max" value={config.maxMotorTorque} step={10} onCommit={(maxMotorTorque) => update({ maxMotorTorque: Math.max(0, maxMotorTorque) })} />
        </Row>
      )}
      <Check checked={config.enableCollision} onChange={(enableCollision) => update({ enableCollision })}>
        Connected bodies collide
      </Check>
    </div>
  )
}

/** Live IK on the chain ending at this bone. */
export function IkConstraint2DFields({ config, nodes, onChange }: { config: IkConstraint2DConfig; nodes: NodeSummary[]; onChange: (value: IkConstraint2DConfig) => void }) {
  const update = (patch: Partial<IkConstraint2DConfig>) => onChange({ ...config, ...patch })
  return (
    <div className="flex flex-col gap-1.5">
      <Row label="Target" title="The node the chain reaches for: move it, animate it, or drive it from a script.">
        <NodePicker value={config.target} nodes={nodes.filter((n) => n.kind !== 'Bone')} noneLabel="None (off)" onChange={(target) => update({ target })} />
      </Row>
      <Row label="Chain Length" title="Bones the solver may bend, this one included. 2 = limb (exact), more = FABRIK.">
        <NumberField label="" value={config.chainLength} step={1} onCommit={(v) => update({ chainLength: clamp(Math.round(v), 1, 32) })} />
      </Row>
      <Row label="Weight" title="0 = animation only, 1 = fully solved.">
        <NumberField label="" value={config.weight} step={0.1} onCommit={(weight) => update({ weight: clamp(weight, 0, 1) })} />
      </Row>
      <Check checked={config.flip} onChange={(flip) => update({ flip })}>
        Flip bend (2-bone)
      </Check>
    </div>
  )
}

/** Unity's `Sprite Skin`. */
export function SpriteSkin2DFields({
  config,
  rootBones,
  onChange,
  onRebind,
}: {
  config: SpriteSkin2DConfig
  /** Rigs to (re)bind to. */
  rootBones: NodeSummary[]
  onChange: (value: SpriteSkin2DConfig) => void
  onRebind: (rootBone: OpaqueId) => void
}) {
  const update = (patch: Partial<SpriteSkin2DConfig>) => onChange({ ...config, ...patch })
  const [root, setRoot] = useState<OpaqueId | null>(rootBones[0]?.id ?? null)
  const count = config.bones.length
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-[11px] text-ink-dim">{count > 0 ? `Bound to ${count} bone${count === 1 ? '' : 's'}` : 'Not bound yet: pick a rig below.'}</span>
      <Row label="Grid" title="How finely the image is cut: more = smoother bends, more vertices.">
        <NumberField label="cols" value={config.columns} step={1} onCommit={(v) => update({ columns: clamp(Math.round(v), 1, 64) })} />
        <NumberField label="rows" value={config.rows} step={1} onCommit={(v) => update({ rows: clamp(Math.round(v), 1, 64) })} />
      </Row>
      <Row label="Smoothness" title="How softly influence blends between neighboring bones (px).">
        <NumberField label="px" value={config.smoothness} step={2} onCommit={(smoothness) => update({ smoothness: Math.max(0.5, smoothness) })} />
      </Row>
      <Row label="Rig" title="Bind (or re-bind) to every bone under this root, using the current pose as the rest pose.">
        <NodePicker value={root} nodes={rootBones} noneLabel="Pick a root bone" onChange={setRoot} />
      </Row>
      <button
        disabled={!root}
        onClick={() => root && onRebind(root)}
        className="rounded bg-accent/80 px-2 py-1 text-[11px] text-white hover:bg-accent disabled:opacity-40"
      >
        Bind to current pose
      </button>
    </div>
  )
}

/** Liquid 2D: a particle fluid that pours, splashes, settles in
 * containers made of colliders, and floats bodies. */
/** Unity's Audio Source: which sound, how loud / fast, and when it plays.
 * ▶ previews the clip right here (also in Design mode). */
export function AudioSourceFields({
  config,
  sounds,
  editor,
  onChange,
}: {
  config: AudioSourceConfig
  sounds: AudioSummary[]
  editor: WasmEditor | null
  onChange: (value: AudioSourceConfig) => void
}) {
  const update = (patch: Partial<AudioSourceConfig>) => onChange({ ...config, ...patch })
  const keyOf = (id: OpaqueId) => `${id.idx}:${id.version}`
  const preview = useRef<SoundHandle | null>(null)
  const [playing, setPlaying] = useState(false)
  useEffect(() => () => preview.current?.stop(), [])
  const toggle = () => {
    if (preview.current?.playing) {
      preview.current.stop()
      setPlaying(false)
      return
    }
    if (!editor || !config.clip) return
    preview.current = audio.play(editor, config.clip, { volume: config.volume, pitch: config.pitch })
    setPlaying(true)
    const tick = () => {
      if (preview.current?.playing) window.setTimeout(tick, 200)
      else setPlaying(false)
    }
    window.setTimeout(tick, 200)
  }
  return (
    <div className="flex flex-col gap-1.5">
      <Row label="Clip" title="The sound this source plays (import sounds with + → Import → Audio).">
        <div className="flex min-w-0 flex-1 items-center gap-1">
          <select
            value={config.clip ? keyOf(config.clip) : ''}
            onChange={(e) => update({ clip: sounds.find((s) => keyOf(s.id) === e.target.value)?.id ?? null })}
            className="min-w-0 flex-1 rounded bg-overlay/10 px-1.5 py-1 text-[11px] text-ink outline-none"
          >
            <option value="">{sounds.length ? 'None' : 'No sounds imported'}</option>
            {sounds.map((s) => (
              <option key={keyOf(s.id)} value={keyOf(s.id)}>
                {s.name}
              </option>
            ))}
          </select>
          <button
            onClick={toggle}
            disabled={!config.clip}
            title={playing ? 'Stop preview' : 'Preview'}
            className="shrink-0 rounded bg-overlay/10 px-2 py-1 text-[11px] text-ink hover:bg-accent/20 hover:text-accent disabled:opacity-35"
          >
            {playing ? '■' : '▶'}
          </button>
        </div>
      </Row>
      <Row label="Volume" title="0 = silent, 1 = full.">
        <NumberField label="" value={config.volume} step={0.05} onCommit={(v) => update({ volume: clamp(v, 0, 4) })} />
      </Row>
      <Row label="Pitch" title="Playback rate: 1 = normal, 2 = an octave up (and twice as fast).">
        <NumberField label="" value={config.pitch} step={0.05} onCommit={(v) => update({ pitch: clamp(v, 0.1, 4) })} />
      </Row>
      <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
        <input type="checkbox" checked={config.loop} onChange={(e) => update({ loop: e.target.checked })} />
        Loop
      </label>
      <label className="flex items-center gap-1.5 text-[11px] text-ink-dim" title="Starts by itself when Playground (or the exported game) enters this scene. Scripts can also call ref.playAudio().">
        <input type="checkbox" checked={config.playOnStart} onChange={(e) => update({ playOnStart: e.target.checked })} />
        Play on start
      </label>
      <label className="flex items-center gap-1.5 text-[11px] text-ink-dim" title="Pans left / right by where this node is relative to the camera, and fades out with distance.">
        <input type="checkbox" checked={config.spatial} onChange={(e) => update({ spatial: e.target.checked })} />
        Spatial (2D)
      </label>
      {config.spatial && (
        <Row label="Range" title="Silent beyond this distance from the camera (world units).">
          <NumberField label="" value={config.range} step={10} onCommit={(v) => update({ range: clamp(v, 1, 100000) })} />
        </Row>
      )}
    </div>
  )
}

/** The nine screen points a UI Anchor snaps to (x, y in 0..1, +y up). */
const ANCHOR_PRESETS: [number, number][] = [
  [0, 1], [0.5, 1], [1, 1],
  [0, 0.5], [0.5, 0.5], [1, 0.5],
  [0, 0], [0.5, 0], [1, 0],
]

/** UI Anchor: a 3×3 picker for the screen point, plus the exact values. */
export function UiAnchorFields({ config, onChange }: { config: UiAnchorConfig; onChange: (value: UiAnchorConfig) => void }) {
  const [ax, ay] = config.anchor
  return (
    <div className="flex flex-col gap-1.5">
      <Row label="Pin to" title="Which point of the screen this node sticks to. Put HUD nodes under the Camera.">
        <div className="grid grid-cols-3 gap-0.5">
          {ANCHOR_PRESETS.map(([x, y]) => (
            <button
              key={`${x},${y}`}
              onClick={() => onChange({ ...config, anchor: [x, y] })}
              title={`${y === 1 ? 'Top' : y === 0 ? 'Bottom' : 'Middle'} ${x === 0 ? 'left' : x === 1 ? 'right' : 'centre'}`}
              className={`h-4 w-4 rounded-sm border ${ax === x && ay === y ? 'border-accent bg-accent' : 'border-overlay/20 bg-overlay/10 hover:bg-overlay/20'}`}
            />
          ))}
        </div>
      </Row>
      <Row label="Point" title="Exact screen point, 0..1 each axis.">
        <VecFields value={config.anchor} min={0} max={1} step={0.05} onChange={(anchor) => onChange({ ...config, anchor })} />
      </Row>
      <Row label="Offset" title="Distance from that point, world units (+y up). E.g. (16, -16) from the top-left corner.">
        <VecFields value={config.offset} onChange={(offset) => onChange({ ...config, offset })} />
      </Row>
    </div>
  )
}

/** UI Stack: lays the children out in a row or column. */
export function UiStackFields({ config, onChange }: { config: UiStackConfig; onChange: (value: UiStackConfig) => void }) {
  const update = (patch: Partial<UiStackConfig>) => onChange({ ...config, ...patch })
  return (
    <div className="flex flex-col gap-1.5">
      <Row label="Direction" title="Children run down a column or across a row.">
        <Dropdown value={config.direction} options={['Vertical', 'Horizontal'] as ('Vertical' | 'Horizontal')[]} onChange={(direction) => update({ direction })} />
      </Row>
      <Row label="Align" title={config.direction === 'Vertical' ? 'Left / centre / right edges line up.' : 'Top / middle / bottom edges line up.'}>
        <Dropdown value={config.align} options={['Start', 'Center', 'End'] as ('Start' | 'Center' | 'End')[]} onChange={(align) => update({ align })} />
      </Row>
      <Row label="Spacing" title="Gap between children (world units).">
        <NumberField label="" value={config.spacing} step={1} onCommit={(v) => update({ spacing: v })} />
      </Row>
      <Row label="Padding" title="Space before the first child.">
        <NumberField label="" value={config.padding} step={1} onCommit={(v) => update({ padding: v })} />
      </Row>
    </div>
  )
}

const TRANSITION_LABEL: Record<UiTransition, string> = { None: 'None', ColorTint: 'Color Tint', SpriteSwap: 'Sprite Swap' }
const DIRECTION_LABEL: Record<UiFillDirection, string> = { LeftToRight: 'Left to right', RightToLeft: 'Right to left', BottomToTop: 'Bottom to top', TopToBottom: 'Top to bottom' }

/** Shared by every UI control's Inspector: what an image picker needs. */
interface UiAssets {
  images: ImageAssetSummary[]
  editor: WasmEditor | null
}

/**
 * Unity's Transition block: a colour (or image) per state — Normal,
 * Highlighted, Pressed, Disabled — plus the colour multiplier, fade
 * duration and per-state scale.
 */
export function UiStateStyleFields({ style, onChange, images, editor, scaleFields = true }: UiAssets & { style: UiStateStyle; onChange: (value: UiStateStyle) => void; scaleFields?: boolean }) {
  const update = (patch: Partial<UiStateStyle>) => onChange({ ...style, ...patch })
  const colorRow = (label: string, key: 'normalColor' | 'highlightedColor' | 'pressedColor' | 'disabledColor', title: string) => (
    <Row label={label} title={title}>
      <ColorSwatch value={style[key]} onChange={(c) => update({ [key]: c } as Partial<UiStateStyle>)} />
    </Row>
  )
  const imageRow = (label: string, key: 'highlightedImage' | 'pressedImage' | 'disabledImage', title: string) => (
    <Row label={label} title={title}>
      {editor ? <ImageAssetPicker editor={editor} images={images} value={style[key]} onChange={(id) => update({ [key]: id } as Partial<UiStateStyle>)} /> : null}
    </Row>
  )
  return (
    <div className="flex flex-col gap-1.5">
      <Row label="Transition" title="How the state shows: multiply the colour, swap the image, or nothing.">
        <Dropdown value={style.transition} options={['ColorTint', 'SpriteSwap', 'None'] as UiTransition[]} renderValue={(v) => TRANSITION_LABEL[v]} renderOption={(v) => TRANSITION_LABEL[v]} onChange={(transition) => update({ transition })} />
      </Row>
      {style.transition === 'ColorTint' && (
        <>
          {colorRow('Normal', 'normalColor', 'Multiplies the node colour when idle (white = as drawn).')}
          {colorRow('Highlighted', 'highlightedColor', 'While the pointer is over it.')}
          {colorRow('Pressed', 'pressedColor', 'While it is held down.')}
          {colorRow('Disabled', 'disabledColor', 'While Interactable is off.')}
          <Row label="Multiplier" title="Brightens every state colour (Unity's Color Multiplier, 1..5).">
            <NumberField label="×" value={style.colorMultiplier} step={0.1} onCommit={(v) => update({ colorMultiplier: clamp(v, 0, 5) })} />
          </Row>
          <Row label="Fade" title="Seconds to blend between state colours (0 = instant).">
            <NumberField label="s" value={style.fadeDuration} step={0.05} onCommit={(v) => update({ fadeDuration: clamp(v, 0, 5) })} />
          </Row>
          <Check checked={style.tintChildren} onChange={(tintChildren) => update({ tintChildren })}>
            Tint children (label, icon)
          </Check>
        </>
      )}
      {style.transition === 'SpriteSwap' && (
        <>
          {imageRow('Highlighted', 'highlightedImage', 'Image while the pointer is over it (none = keep the normal one).')}
          {imageRow('Pressed', 'pressedImage', 'Image while it is held down.')}
          {imageRow('Disabled', 'disabledImage', 'Image while Interactable is off.')}
        </>
      )}
      {scaleFields && style.transition !== 'None' && (
        <>
          <Row label="Hover scale" title="Scale while hovered — a small lift (1 = none).">
            <NumberField label="×" value={style.highlightedScale} step={0.01} onCommit={(v) => update({ highlightedScale: clamp(v, 0.5, 1.5) })} />
          </Row>
          <Row label="Press scale" title="Scale while held — a 'press in' (1 = none).">
            <NumberField label="×" value={style.pressedScale} step={0.01} onCommit={(v) => update({ pressedScale: clamp(v, 0.5, 1.5) })} />
          </Row>
        </>
      )}
    </div>
  )
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5 border-t border-overlay/10 pt-1.5">
      <span className="text-[10px] uppercase tracking-wider text-ink-dim">{title}</span>
      {children}
    </div>
  )
}

function ScriptHint({ children }: { children: ReactNode }) {
  return <p className="text-[10px] leading-snug text-ink-dim">{children}</p>
}

/** UI Button: interactable + its state look; clicks are read in scripts. */
export function UiButtonFields({ config, onChange, images, editor }: UiAssets & { config: UiButtonConfig; onChange: (value: UiButtonConfig) => void }) {
  return (
    <div className="flex flex-col gap-1.5">
      <Check checked={config.interactable} onChange={(interactable) => onChange({ ...config, interactable })}>
        Interactable
      </Check>
      <UiStateStyleFields style={config.style} images={images} editor={editor} onChange={(style) => onChange({ ...config, style })} />
      <ScriptHint>
        In a script: <code className="text-ink">if (getCompByID('play').clicked) …</code> (also <code>hovered</code>, <code>pressed</code>).
      </ScriptHint>
    </div>
  )
}

/** UI Toggle: on / off, radio group, and the box's state look. */
export function UiToggleFields({ config, onChange, images, editor }: UiAssets & { config: UiToggleConfig; onChange: (value: UiToggleConfig) => void }) {
  const update = (patch: Partial<UiToggleConfig>) => onChange({ ...config, ...patch })
  return (
    <div className="flex flex-col gap-1.5">
      <Check checked={config.isOn} onChange={(isOn) => update({ isOn })}>
        Is on
      </Check>
      <Check checked={config.interactable} onChange={(interactable) => update({ interactable })}>
        Interactable
      </Check>
      <Row label="Group" title="0 = an independent checkbox. Toggles with the same number are radio buttons: one on turns the others off.">
        <NumberField label="#" value={config.group} step={1} onCommit={(v) => update({ group: Math.max(0, Math.round(v)) })} />
      </Row>
      {config.group > 0 && (
        <Check checked={config.allowSwitchOff} onChange={(allowSwitchOff) => update({ allowSwitchOff })}>
          Allow switching all off
        </Check>
      )}
      <Section title="Look">
        <UiStateStyleFields style={config.style} images={images} editor={editor} onChange={(style) => update({ style })} />
      </Section>
      <ScriptHint>
        Shows its child named <code className="text-ink">Checkmark</code> while on. Script: <code className="text-ink">ref.isOn</code>, <code>ref.changed</code>.
      </ScriptHint>
    </div>
  )
}

/** UI Slider: range, value, direction, and the handle's state look. */
export function UiSliderFields({ config, onChange, images, editor }: UiAssets & { config: UiSliderConfig; onChange: (value: UiSliderConfig) => void }) {
  const update = (patch: Partial<UiSliderConfig>) => onChange({ ...config, ...patch })
  const step = config.wholeNumbers ? 1 : Math.max(0.01, Math.abs(config.max - config.min) / 100)
  return (
    <div className="flex flex-col gap-1.5">
      <Row label="Direction" title="Which way the fill grows.">
        <Dropdown value={config.direction} options={Object.keys(DIRECTION_LABEL) as UiFillDirection[]} renderValue={(v) => DIRECTION_LABEL[v]} renderOption={(v) => DIRECTION_LABEL[v]} onChange={(direction) => update({ direction })} />
      </Row>
      <Row label="Min / Max">
        <NumberField label="" value={config.min} step={1} onCommit={(min) => update({ min, value: clamp(config.value, Math.min(min, config.max), Math.max(min, config.max)) })} />
        <NumberField label="" value={config.max} step={1} onCommit={(max) => update({ max, value: clamp(config.value, Math.min(config.min, max), Math.max(config.min, max)) })} />
      </Row>
      <Row label="Value" title="Starting value.">
        <input
          type="range"
          className="min-w-0 flex-1 accent-[rgb(var(--accent-rgb))]"
          min={Math.min(config.min, config.max)}
          max={Math.max(config.min, config.max)}
          step={step}
          value={config.value}
          onChange={(e) => update({ value: Number(e.target.value) })}
        />
        <NumberField label="" value={config.value} step={step} onCommit={(v) => update({ value: clamp(config.wholeNumbers ? Math.round(v) : v, Math.min(config.min, config.max), Math.max(config.min, config.max)) })} />
      </Row>
      <Check checked={config.wholeNumbers} onChange={(wholeNumbers) => update({ wholeNumbers, value: wholeNumbers ? Math.round(config.value) : config.value })}>
        Whole numbers
      </Check>
      <Check checked={config.interactable} onChange={(interactable) => update({ interactable })}>
        Interactable
      </Check>
      <Section title="Handle look">
        <UiStateStyleFields style={config.style} images={images} editor={editor} onChange={(style) => update({ style })} />
      </Section>
      <ScriptHint>
        Positions its <code className="text-ink">Fill</code> and <code className="text-ink">Handle</code> children. Script: <code className="text-ink">ref.value</code>, <code>ref.changed</code>.
      </ScriptHint>
    </div>
  )
}

/** UI Scrollbar: value, handle size, steps, and the handle's look. */
export function UiScrollbarFields({ config, onChange, images, editor }: UiAssets & { config: UiScrollbarConfig; onChange: (value: UiScrollbarConfig) => void }) {
  const update = (patch: Partial<UiScrollbarConfig>) => onChange({ ...config, ...patch })
  return (
    <div className="flex flex-col gap-1.5">
      <Row label="Direction" title="Which end is value 0.">
        <Dropdown value={config.direction} options={Object.keys(DIRECTION_LABEL) as UiFillDirection[]} renderValue={(v) => DIRECTION_LABEL[v]} renderOption={(v) => DIRECTION_LABEL[v]} onChange={(direction) => update({ direction })} />
      </Row>
      <Row label="Value" title="Handle position, 0..1.">
        <NumberField label="" value={config.value} step={0.05} onCommit={(v) => update({ value: clamp(v, 0, 1) })} />
      </Row>
      <Row label="Size" title="Handle length as a fraction of the track (a Scroll View sets this from its content).">
        <NumberField label="" value={config.size} step={0.05} onCommit={(v) => update({ size: clamp(v, 0.05, 1) })} />
      </Row>
      <Row label="Steps" title="0 = continuous; else the value snaps to that many positions.">
        <NumberField label="" value={config.steps} step={1} onCommit={(v) => update({ steps: Math.max(0, Math.round(v)) })} />
      </Row>
      <Check checked={config.interactable} onChange={(interactable) => update({ interactable })}>
        Interactable
      </Check>
      <Section title="Handle look">
        <UiStateStyleFields style={config.style} images={images} editor={editor} scaleFields={false} onChange={(style) => update({ style })} />
      </Section>
    </div>
  )
}

/** UI Scroll View: axes, wheel speed, inertia. */
export function UiScrollViewFields({ config, onChange }: { config: UiScrollViewConfig; onChange: (value: UiScrollViewConfig) => void }) {
  const update = (patch: Partial<UiScrollViewConfig>) => onChange({ ...config, ...patch })
  return (
    <div className="flex flex-col gap-1.5">
      <Check checked={config.vertical} onChange={(vertical) => update({ vertical, horizontal: vertical ? false : true })}>
        Vertical (off = horizontal)
      </Check>
      <Row label="Wheel step" title="World units scrolled per mouse-wheel notch.">
        <NumberField label="" value={config.wheelStep} step={5} onCommit={(v) => update({ wheelStep: Math.max(0, v) })} />
      </Row>
      <Check checked={config.inertia} onChange={(inertia) => update({ inertia })}>
        Inertia (keeps gliding after a drag)
      </Check>
      <ScriptHint>
        Scrolls its child named <code className="text-ink">Content</code>; a child named <code className="text-ink">Scrollbar</code> stays in sync. Turn on <em>Clip children</em> on this shape. Script: <code className="text-ink">ref.scroll</code>.
      </ScriptHint>
    </div>
  )
}

export function Liquid2DFields({ config, onChange }: { config: Liquid2DConfig; onChange: (value: Liquid2DConfig) => void }) {
  const update = (patch: Partial<Liquid2DConfig>) => onChange({ ...config, ...patch })
  const count = Math.max(1, Math.floor(config.size[0] / config.particleSpacing)) * Math.max(1, Math.floor(config.size[1] / config.particleSpacing))
  return (
    <div className="flex flex-col gap-1.5">
      <Row label="Volume" title="The starting body of liquid (centered on this node), px.">
        <VecFields value={config.size} min={0} onChange={(size) => update({ size })} />
      </Row>
      <Row label="Spacing" title="Distance between particles at rest. Smaller = finer and smoother, but more particles to simulate.">
        <NumberField label="px" value={config.particleSpacing} step={0.5} onCommit={(v) => update({ particleSpacing: clamp(v, 3, 30) })} />
      </Row>
      <span className="text-[10px] text-ink-dim">
        {Math.min(count, config.maxParticles)} particles to start{count > config.maxParticles ? ` (capped at ${config.maxParticles})` : ''}
      </span>
      <Row label="Color" title="Thin spots and spray blend toward white, like foam.">
        <ColorSwatch value={config.color} onChange={(color) => update({ color })} />
      </Row>
      <Row label="Viscosity" title="0 = water, 1 = honey.">
        <NumberField label="" value={config.viscosity} step={0.05} onCommit={(v) => update({ viscosity: clamp(v, 0, 1) })} />
      </Row>
      <Row label="Stiffness" title="How hard it resists being squashed. Lower = squishier, higher = firmer (can jitter).">
        <NumberField label="" value={config.stiffness} step={0.1} onCommit={(v) => update({ stiffness: clamp(v, 0.2, 3) })} />
      </Row>
      <Row label="Density" title="Buoyancy: bodies lighter per area than this float, denser ones sink (kg per 32x32 px; a 24 px crate of mass 1 is about 1.8). 0 = no buoyancy.">
        <NumberField label="" value={config.buoyancy} step={0.1} onCommit={(v) => update({ buoyancy: clamp(v, 0, 50) })} />
      </Row>
      <Row label="Drag" title="How much it slows bodies moving through it.">
        <NumberField label="" value={config.drag} step={0.25} onCommit={(v) => update({ drag: clamp(v, 0, 20) })} />
      </Row>
      <Row label="Faucet" title="Particles per second streaming out of this node's origin (0 = off).">
        <NumberField label="/s" value={config.emitRate} step={10} onCommit={(v) => update({ emitRate: clamp(v, 0, 2000) })} />
      </Row>
      {config.emitRate > 0 && (
        <Row label="Stream" title="Faucet stream velocity, px/s.">
          <VecFields value={config.emitVelocity} onChange={(emitVelocity) => update({ emitVelocity })} />
        </Row>
      )}
      <Row label="Max" title="Hard cap on particles (performance guard).">
        <NumberField label="" value={config.maxParticles} step={50} onCommit={(v) => update({ maxParticles: clamp(Math.round(v), 10, 4000) })} />
      </Row>
    </div>
  )
}

/** Display names, Unity's wording. */
export const COMPONENT_LABEL: Record<ComponentKind, string> = {
  Camera: 'Camera',
  Light2D: 'Light 2D',
  TrailRenderer: 'Trail Renderer',
  ParticleSystem: 'Particle System',
  ShadowCaster2D: 'Shadow Caster 2D',
  Rigidbody2D: 'Rigidbody 2D',
  BoxCollider2D: 'Box Collider 2D',
  CircleCollider2D: 'Circle Collider 2D',
  CapsuleCollider2D: 'Capsule Collider 2D',
  PolygonCollider2D: 'Polygon Collider 2D',
  HingeJoint2D: 'Hinge Joint 2D',
  IkConstraint2D: 'IK Constraint 2D',
  SpriteSkin2D: 'Sprite Skin',
  Liquid2D: 'Liquid 2D',
  AudioSource: 'Audio Source',
  UiAnchor: 'UI Anchor',
  UiStack: 'UI Stack',
  UiButton: 'UI Button',
  UiToggle: 'UI Toggle',
  UiSlider: 'UI Slider',
  UiScrollbar: 'UI Scrollbar',
  UiScrollView: 'UI Scroll View',
}

/** Components a node can only carry one of (Unity's DisallowMultiple). */
export const SINGLE_COMPONENTS: ComponentKind[] = ['Camera', 'Light2D', 'TrailRenderer', 'ParticleSystem', 'ShadowCaster2D', 'Rigidbody2D', 'IkConstraint2D', 'SpriteSkin2D', 'Liquid2D', 'UiAnchor', 'UiStack', 'UiButton', 'UiToggle', 'UiSlider', 'UiScrollbar', 'UiScrollView']

const SECTIONS: { title: string; kinds: ComponentKind[] }[] = [
  { title: 'Rendering', kinds: ['Camera', 'Light2D', 'ShadowCaster2D', 'TrailRenderer', 'ParticleSystem'] },
  { title: 'Physics 2D', kinds: ['Rigidbody2D', 'BoxCollider2D', 'CircleCollider2D', 'CapsuleCollider2D', 'PolygonCollider2D', 'HingeJoint2D', 'Liquid2D'] },
  { title: 'Rigging', kinds: ['IkConstraint2D', 'SpriteSkin2D'] },
  { title: 'Audio', kinds: ['AudioSource'] },
  { title: 'UI', kinds: ['UiAnchor', 'UiStack', 'UiButton', 'UiToggle', 'UiSlider', 'UiScrollbar', 'UiScrollView'] },
]

/**
 * Unity's "Add Component" button: a popover grouped into sections
 * (Rendering / Physics 2D / Scripts). Components a node already has one of
 * (and can only have one of) are shown disabled.
 */
export function AddComponentMenu({
  present,
  hasMaterial,
  onAddMaterial,
  scripts,
  onAdd,
  onAddScript,
  onNewScript,
}: {
  present: ComponentKind[]
  /** The node already wears a Material (only one allowed). */
  hasMaterial: boolean
  /** Adds the Material component (shader-graph material slot). */
  onAddMaterial: () => void
  scripts: { key: string; name: string }[]
  onAdd: (kind: ComponentKind) => void
  onAddScript: (key: string) => void
  onNewScript: () => void
}) {
  const [open, setOpen] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!open) return
    const close = (e: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false)
    }
    window.addEventListener('pointerdown', close)
    return () => window.removeEventListener('pointerdown', close)
  }, [open])
  const item = 'block w-full rounded-md px-2 py-1 text-left text-xs text-ink hover:bg-accent/15 hover:text-accent disabled:pointer-events-none disabled:opacity-35'
  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="flex w-full items-center justify-center gap-1 rounded-md bg-accent/15 px-2 py-1.5 text-[11px] font-medium text-accent hover:bg-accent/25"
      >
        <PlusIcon />
        Add Component
      </button>
      {open && (
        <div className="absolute bottom-full left-0 z-30 mb-1 max-h-96 w-full overflow-y-auto rounded-lg border border-panel-border bg-panel p-1 shadow-2xl backdrop-blur-panel">
          {SECTIONS.map((section) => (
            <div key={section.title} className="mb-1">
              <span className="block px-2 py-0.5 text-[10px] uppercase tracking-wide text-ink-dim/70">{section.title}</span>
              {section.kinds.map((kind) => (
                <button
                  key={kind}
                  type="button"
                  disabled={SINGLE_COMPONENTS.includes(kind) && present.includes(kind)}
                  onClick={() => {
                    onAdd(kind)
                    setOpen(false)
                  }}
                  className={item}
                >
                  {COMPONENT_LABEL[kind]}
                </button>
              ))}
            </div>
          ))}
          <span className="block px-2 py-0.5 text-[10px] uppercase tracking-wide text-ink-dim/70">Shading</span>
          <button
            type="button"
            disabled={hasMaterial}
            onClick={() => {
              onAddMaterial()
              setOpen(false)
            }}
            className={item}
          >
            Material (Shader Graph)
          </button>
          <span className="block px-2 py-0.5 text-[10px] uppercase tracking-wide text-ink-dim/70">Scripts</span>
          {scripts.map((s) => (
            <button
              key={s.key}
              type="button"
              onClick={() => {
                onAddScript(s.key)
                setOpen(false)
              }}
              className={item}
            >
              {s.name}
            </button>
          ))}
          <button
            type="button"
            onClick={() => {
              onNewScript()
              setOpen(false)
            }}
            className={item}
          >
            + New Script…
          </button>
        </div>
      )}
    </div>
  )
}
