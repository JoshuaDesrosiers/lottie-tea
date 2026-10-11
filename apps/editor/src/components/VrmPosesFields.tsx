/**
 * Inspector fields for **VRM Poses** (`an1mbyt3_core::VrmPoses`): named
 * poses ("states") on a VRM avatar node, for animating without keyframing
 * every bone by hand.
 *
 * - Each pose stores per-bone rotations (Euler degrees, Y-X-Z, in the
 *   bone's own space, on top of the rest pose), a hips offset and
 *   expression weights.
 * - The **active** pose shows live in Design (instantly), so editing a
 *   slider moves the avatar right there.
 * - In Playground, `ref.setPose3('Punch')` crossfades to a pose over the
 *   component's blend time (or `ref.setPose3('Punch', { blend: 0.05 })`).
 * - Presets (A-pose, Guard, Punch, Kick, Hit, Victory …) give a starting
 *   point; Mirror copies one side's arm/leg/finger angles to the other.
 *
 * Presets are authored for VRM 1.0 (avatar faces +Z, its left is +X); for
 * VRM 0.x avatars (facing −Z) the X and Z angles are flipped.
 */
import { useEffect, useMemo, useState, type ReactNode } from 'react'
import type { OpaqueId, VrmBonePose, VrmPose, VrmPosesConfig } from '../ipc/document'
import type { WasmEditor } from '../ipc/wasmEditor'
import type { ModelInfo3 } from '../scriptRuntime'
import { Dropdown } from './InspectorPanel'
import { Slider } from './Slider'

type Vec3 = [number, number, number]

/** The Inspector's bone groups, in humanoid-bone names (VRM 1.0 names;
 * VRM 0.x's thumb names are matched too — only bones the avatar has show). */
const BONE_GROUPS: { title: string; bones: string[] }[] = [
  { title: 'Body', bones: ['hips', 'spine', 'chest', 'upperChest', 'neck', 'head'] },
  { title: 'Left arm', bones: ['leftShoulder', 'leftUpperArm', 'leftLowerArm', 'leftHand'] },
  { title: 'Right arm', bones: ['rightShoulder', 'rightUpperArm', 'rightLowerArm', 'rightHand'] },
  { title: 'Left leg', bones: ['leftUpperLeg', 'leftLowerLeg', 'leftFoot', 'leftToes'] },
  { title: 'Right leg', bones: ['rightUpperLeg', 'rightLowerLeg', 'rightFoot', 'rightToes'] },
  {
    title: 'Left fingers',
    bones: ['Thumb', 'Index', 'Middle', 'Ring', 'Little'].flatMap((f) =>
      (f === 'Thumb' ? ['Metacarpal', 'Proximal', 'Intermediate', 'Distal'] : ['Proximal', 'Intermediate', 'Distal']).map((j) => `left${f}${j}`),
    ),
  },
  {
    title: 'Right fingers',
    bones: ['Thumb', 'Index', 'Middle', 'Ring', 'Little'].flatMap((f) =>
      (f === 'Thumb' ? ['Metacarpal', 'Proximal', 'Intermediate', 'Distal'] : ['Proximal', 'Intermediate', 'Distal']).map((j) => `right${f}${j}`),
    ),
  },
]

/** A preset: bone → [x, y, z] degrees (VRM 1.0 space), hips offset and
 * expressions (skipped when the avatar lacks them). */
interface Preset {
  bones: Record<string, Vec3>
  hips?: Vec3
  expressions?: Record<string, number>
}

/** Both fists up by the chin, knees soft, feet apart. */
const GUARD: Preset = {
  hips: [0, -0.06, 0],
  bones: {
    spine: [8, 0, 0],
    head: [-6, 0, 0],
    leftUpperArm: [0, -35, -62],
    leftLowerArm: [0, -115, 0],
    leftHand: [0, 0, -15],
    rightUpperArm: [0, 35, 62],
    rightLowerArm: [0, 115, 0],
    rightHand: [0, 0, 15],
    leftUpperLeg: [-22, 0, 6],
    leftLowerLeg: [35, 0, 0],
    leftFoot: [-12, 0, 0],
    rightUpperLeg: [-8, 0, -6],
    rightLowerLeg: [25, 0, 0],
    rightFoot: [-14, 0, 0],
  },
}

/** Curls every finger into a fist (on top of a pose). */
function fists(preset: Preset): Preset {
  const bones = { ...preset.bones }
  for (const side of ['left', 'right'] as const) {
    const s = side === 'left' ? 1 : -1
    for (const f of ['Index', 'Middle', 'Ring', 'Little']) {
      for (const j of ['Proximal', 'Intermediate', 'Distal']) bones[`${side}${f}${j}`] = [0, 0, -75 * s]
    }
    bones[`${side}ThumbProximal`] = [0, -35 * s, 0]
    bones[`${side}ThumbDistal`] = [0, -40 * s, 0]
  }
  return { ...preset, bones }
}

export const PRESETS: Record<string, Preset> = {
  'Rest (T-pose)': { bones: {} },
  'A-pose': { bones: { leftUpperArm: [0, 0, -70], rightUpperArm: [0, 0, 70] } },
  Idle: {
    bones: {
      spine: [3, 0, 0],
      leftUpperArm: [0, -8, -72],
      leftLowerArm: [0, -12, 0],
      rightUpperArm: [0, 8, 72],
      rightLowerArm: [0, 12, 0],
      leftUpperLeg: [0, 0, 3],
      rightUpperLeg: [0, 0, -3],
    },
  },
  Guard: { ...fists(GUARD), expressions: { angry: 0.35 } },
  Punch: {
    ...fists({
      hips: [0, -0.06, 0.04],
      bones: {
        ...GUARD.bones,
        spine: [10, -18, 0],
        chest: [0, -12, 0],
        head: [-6, 25, 0],
        leftUpperArm: [0, -88, -8],
        leftLowerArm: [0, -6, 0],
        leftHand: [0, 0, 0],
      },
    }),
    expressions: { angry: 0.7 },
  },
  Kick: {
    ...fists({
      hips: [0, -0.02, 0],
      bones: {
        ...GUARD.bones,
        spine: [-14, 0, 0],
        rightUpperLeg: [-88, 0, -4],
        rightLowerLeg: [12, 0, 0],
        rightFoot: [-30, 0, 0],
        leftUpperLeg: [-6, 0, 4],
        leftLowerLeg: [20, 0, 0],
      },
    }),
    expressions: { angry: 0.6 },
  },
  Hit: {
    hips: [0, -0.04, -0.06],
    bones: {
      spine: [-16, 8, 0],
      chest: [-10, 0, 0],
      head: [-22, -12, 6],
      leftUpperArm: [0, 25, -45],
      leftLowerArm: [0, -40, 0],
      rightUpperArm: [0, -20, 50],
      rightLowerArm: [0, 30, 0],
      leftUpperLeg: [-10, 0, 6],
      leftLowerLeg: [25, 0, 0],
      rightUpperLeg: [12, 0, -6],
      rightLowerLeg: [20, 0, 0],
    },
    expressions: { surprised: 0.8 },
  },
  Victory: {
    bones: {
      spine: [-4, 0, 0],
      head: [-10, 0, 0],
      rightUpperArm: [0, 10, -78],
      rightLowerArm: [0, 0, -25],
      leftUpperArm: [0, -45, -62],
      leftLowerArm: [0, -110, 0],
      leftUpperLeg: [0, 0, 6],
      rightUpperLeg: [0, 0, -6],
    },
    expressions: { happy: 1 },
  },
}

/** A preset's bones and expressions for this avatar: only bones and
 * expressions it has; X/Z flipped for VRM 0.x (which faces −Z). */
export function presetPose(key: string, info: ModelInfo3 | null): Pick<VrmPose, 'bones' | 'expressions'> | null {
  const preset = PRESETS[key]
  if (!preset) return null
  const flip = (info?.forwardZ ?? 1) < 0
  const hasBone = new Set(info?.bones ?? [])
  const bones: VrmBonePose[] = Object.entries(preset.bones)
    .filter(([bone]) => !info || hasBone.has(bone))
    .map(([bone, r]) => ({ bone, rotation: (flip ? [-r[0], r[1], -r[2]] : [...r]) as Vec3, offset: [0, 0, 0] as Vec3 }))
  if (preset.hips) {
    const h = preset.hips
    const offset: Vec3 = flip ? [-h[0], h[1], -h[2]] : [...h]
    const hips = bones.find((b) => b.bone === 'hips')
    if (hips) hips.offset = offset
    else bones.push({ bone: 'hips', rotation: [0, 0, 0], offset })
  }
  const available = new Set(info?.expressions ?? [])
  const expressions = Object.entries(preset.expressions ?? {})
    .filter(([name]) => !info || available.has(name))
    .map(([name, weight]) => ({ name, weight }))
  return { bones, expressions }
}

/** The other side's bone ("leftUpperArm" ↔ "rightUpperArm"), or null. */
function otherSide(bone: string): string | null {
  if (bone.startsWith('left')) return `right${bone.slice(4)}`
  if (bone.startsWith('right')) return `left${bone.slice(5)}`
  return null
}

/** A rotation seen in the mirror (across the avatar's left/right plane). */
const mirrored = (r: Vec3): Vec3 => [r[0], -r[1], -r[2]]

/** The first unused "Name", "Name 2", … */
function uniqueName(base: string, taken: string[]): string {
  if (!taken.includes(base)) return base
  for (let n = 2; ; n++) if (!taken.includes(`${base} ${n}`)) return `${base} ${n}`
}

function Section({ title, children, defaultOpen = false, badge }: { title: string; children: ReactNode; defaultOpen?: boolean; badge?: number }) {
  const [open, setOpen] = useState(defaultOpen)
  return (
    <div className="rounded-md border border-line/50">
      <button type="button" onClick={() => setOpen(!open)} className="flex w-full items-center gap-1.5 px-2 py-1 text-left text-[11px] text-ink hover:bg-overlay/5">
        <span className="text-ink-dim">{open ? '▾' : '▸'}</span>
        <span className="flex-1">{title}</span>
        {!!badge && <span className="rounded bg-accent/20 px-1 text-[9px] text-accent">{badge}</span>}
      </button>
      {open && <div className="flex flex-col gap-1.5 px-2 pb-2 pt-1">{children}</div>}
    </div>
  )
}

/** One axis: slider + number. */
function Axis({ label, value, min, max, step, digits, onChange }: { label: string; value: number; min: number; max: number; step: number; digits: number; onChange: (v: number) => void }) {
  return (
    <div className="flex items-center gap-1">
      <span className="w-3 shrink-0 text-[9px] uppercase text-ink-dim">{label}</span>
      <div className="min-w-0 flex-1">
        <Slider value={Math.min(max, Math.max(min, value))} min={min} max={max} step={step} onChange={onChange} />
      </div>
      <input
        type="number"
        value={Number(value.toFixed(digits))}
        step={step}
        onChange={(e) => {
          const v = parseFloat(e.target.value)
          if (Number.isFinite(v)) onChange(v)
        }}
        className="w-12 shrink-0 rounded bg-overlay/5 px-1 text-right text-[10px] tabular-nums text-ink"
      />
    </div>
  )
}

const pill = 'rounded px-2 py-0.5 text-[10px] text-ink-dim hover:bg-overlay/10 hover:text-ink disabled:opacity-40'

export function VrmPosesFields({
  config,
  onChange,
  editor,
  artboardId,
  nodeId,
}: {
  config: VrmPosesConfig
  onChange: (value: VrmPosesConfig) => void
  editor: WasmEditor | null
  artboardId: OpaqueId | null
  nodeId: OpaqueId
}) {
  // What the avatar offers (bones, expressions, facing) — the model loads
  // on its first draw, so retry briefly until it's there.
  const [info, setInfo] = useState<ModelInfo3 | null>(null)
  useEffect(() => {
    if (!editor || !artboardId) return
    let tries = 0
    let timer = 0
    const poll = () => {
      let found: ModelInfo3 | null = null
      try {
        found = editor.model_info(artboardId, nodeId) as ModelInfo3 | null
      } catch {
        found = null
      }
      if (found) setInfo(found)
      else if (tries++ < 20) timer = window.setTimeout(poll, 300)
    }
    poll()
    return () => window.clearTimeout(timer)
  }, [editor, artboardId, nodeId])

  const has = useMemo(() => {
    const set = new Set(info?.bones ?? [])
    return (bone: string) => !info || set.has(bone)
  }, [info])

  const poses = config.poses.length ? config.poses : [{ name: 'Rest', bones: [], expressions: [] }]
  const activeIndex = Math.max(0, poses.findIndex((p) => p.name === config.active))
  const pose = poses[activeIndex]
  const names = poses.map((p) => p.name)

  const setPoses = (next: VrmPose[], active = config.active) => onChange({ ...config, poses: next, active })
  const updatePose = (patch: Partial<VrmPose>) => setPoses(poses.map((p, i) => (i === activeIndex ? { ...p, ...patch } : p)))

  const boneOf = (bone: string): VrmBonePose => pose.bones.find((b) => b.bone === bone) ?? { bone, rotation: [0, 0, 0], offset: [0, 0, 0] }
  const setBone = (value: VrmBonePose) => {
    const zero = value.rotation.every((v) => v === 0) && value.offset.every((v) => v === 0)
    const rest = pose.bones.filter((b) => b.bone !== value.bone)
    updatePose({ bones: zero ? rest : [...rest, value] })
  }
  const expressionOf = (name: string) => pose.expressions.find((e) => e.name === name)?.weight ?? 0
  const setExpression = (name: string, weight: number) => {
    const rest = pose.expressions.filter((e) => e.name !== name)
    updatePose({ expressions: weight === 0 ? rest : [...rest, { name, weight }] })
  }

  const applyPreset = (key: string) => {
    const made = presetPose(key, info)
    if (made) updatePose(made)
  }

  const mirror = (from: 'left' | 'right') => {
    const kept = pose.bones.filter((b) => !b.bone.startsWith(from === 'left' ? 'right' : 'left'))
    const copies = pose.bones.filter((b) => b.bone.startsWith(from)).map((b) => ({ bone: otherSide(b.bone)!, rotation: mirrored(b.rotation), offset: [-b.offset[0], b.offset[1], b.offset[2]] as Vec3 }))
    updatePose({ bones: [...kept, ...copies] })
  }

  const addPose = (copy: boolean) => {
    const name = uniqueName(copy ? pose.name : 'Pose', names)
    const fresh: VrmPose = copy ? { name, bones: pose.bones.map((b) => ({ ...b })), expressions: pose.expressions.map((e) => ({ ...e })) } : { name, bones: [], expressions: [] }
    setPoses([...poses, fresh], name)
  }
  const deletePose = () => {
    if (poses.length <= 1) return
    const next = poses.filter((_, i) => i !== activeIndex)
    setPoses(next, next[Math.max(0, activeIndex - 1)].name)
  }
  const renamePose = (name: string) => {
    const clean = name.trim()
    if (!clean || (clean !== pose.name && names.includes(clean))) return
    setPoses(poses.map((p, i) => (i === activeIndex ? { ...p, name: clean } : p)), clean)
  }
  const [draftName, setDraftName] = useState(pose.name)
  useEffect(() => setDraftName(pose.name), [pose.name])

  const posedCount = (bones: string[]) => pose.bones.filter((b) => bones.includes(b.bone)).length

  return (
    <div className="flex flex-col gap-2">
      {info && !info.vrm && <p className="text-[10px] leading-snug text-amber-300">This model isn&apos;t a VRM avatar — bones are matched by node name.</p>}
      {!info && <p className="text-[10px] leading-snug text-ink-dim">Put this on a Model 3D node with a VRM avatar (File → Import VRM).</p>}

      {/* Pose list: click one to make it the active (shown) pose. */}
      <div className="flex flex-wrap gap-1">
        {poses.map((p) => (
          <button
            key={p.name}
            type="button"
            onClick={() => setPoses(poses, p.name)}
            className={`rounded-md px-2 py-0.5 text-[11px] ${p.name === pose.name ? 'bg-accent/25 text-accent' : 'bg-overlay/5 text-ink-dim hover:text-ink'}`}
            title={`Show "${p.name}" — in a script: ref.setPose3('${p.name}')`}
          >
            {p.name}
          </button>
        ))}
      </div>
      <div className="flex items-center gap-1">
        <input
          value={draftName}
          onChange={(e) => setDraftName(e.target.value)}
          onBlur={() => renamePose(draftName)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') renamePose(draftName)
            if (e.key === 'Escape') setDraftName(pose.name)
          }}
          className="min-w-0 flex-1 rounded bg-overlay/5 px-1.5 py-0.5 text-[11px] text-ink"
          title="Rename this pose"
        />
        <button type="button" className={pill} onClick={() => addPose(false)} title="New empty pose">
          New
        </button>
        <button type="button" className={pill} onClick={() => addPose(true)} title="Copy this pose">
          Duplicate
        </button>
        <button type="button" className={pill} onClick={deletePose} disabled={poses.length <= 1} title="Delete this pose">
          Delete
        </button>
      </div>

      <div className="flex items-center gap-1">
        <span className="w-24 shrink-0 text-[11px] text-ink-dim" title="Replace this pose with a starting point, then tweak">Start from</span>
        <div className="min-w-0 flex-1">
          <Dropdown<string> value="" options={Object.keys(PRESETS)} onChange={applyPreset} renderValue={() => 'Preset…'} />
        </div>
      </div>
      <div className="flex items-center justify-end gap-1">
        <button type="button" className={pill} onClick={() => mirror('left')} title="Copy the left side's angles to the right, mirrored">
          Mirror L→R
        </button>
        <button type="button" className={pill} onClick={() => mirror('right')} title="Copy the right side's angles to the left, mirrored">
          R→L
        </button>
        <button type="button" className={pill} onClick={() => updatePose({ bones: [], expressions: [] })} title="Back to the rest pose">
          Reset
        </button>
      </div>

      <div className="flex items-center gap-2" title="Crossfade time when a script switches pose (ref.setPose3)">
        <span className="w-24 shrink-0 text-[11px] text-ink-dim">Blend (s)</span>
        <div className="min-w-0 flex-1">
          <Slider value={config.blend} min={0} max={1} step={0.01} onChange={(blend) => onChange({ ...config, blend })} />
        </div>
        <span className="w-10 shrink-0 text-right text-[10px] tabular-nums text-ink-dim">{config.blend.toFixed(2)}</span>
      </div>

      {BONE_GROUPS.map((group) => {
        const bones = group.bones.filter(has)
        if (!bones.length) return null
        return (
          <Section key={group.title} title={group.title} badge={posedCount(bones)} defaultOpen={group.title === 'Body'}>
            {bones.map((bone) => {
              const value = boneOf(bone)
              const posed = value.rotation.some((v) => v !== 0) || value.offset.some((v) => v !== 0)
              const setRot = (axis: number, v: number) => {
                const rotation = [...value.rotation] as Vec3
                rotation[axis] = v
                setBone({ ...value, rotation })
              }
              const setOffset = (axis: number, v: number) => {
                const offset = [...value.offset] as Vec3
                offset[axis] = v
                setBone({ ...value, offset })
              }
              return (
                <div key={bone} className="flex flex-col gap-0.5">
                  <div className="flex items-center">
                    <span className={`flex-1 text-[10px] ${posed ? 'text-accent' : 'text-ink-dim'}`}>{bone}</span>
                    {posed && (
                      <button type="button" className="text-[9px] text-ink-dim hover:text-ink" onClick={() => setBone({ bone, rotation: [0, 0, 0], offset: [0, 0, 0] })}>
                        reset
                      </button>
                    )}
                  </div>
                  {(['x', 'y', 'z'] as const).map((axis, i) => (
                    <Axis key={axis} label={axis} value={value.rotation[i]} min={-180} max={180} step={1} digits={0} onChange={(v) => setRot(i, v)} />
                  ))}
                  {bone === 'hips' &&
                    (['x', 'y', 'z'] as const).map((axis, i) => (
                      <Axis key={`o${axis}`} label={`↕${axis}`} value={value.offset[i]} min={-1} max={1} step={0.01} digits={2} onChange={(v) => setOffset(i, v)} />
                    ))}
                </div>
              )
            })}
          </Section>
        )
      })}

      {!!info?.expressions.length && (
        <Section title="Expressions" badge={pose.expressions.length}>
          {info.expressions.map((name) => (
            <div key={name} className="flex items-center gap-2">
              <span className="w-20 shrink-0 truncate text-[10px] text-ink-dim">{name}</span>
              <div className="min-w-0 flex-1">
                <Slider value={expressionOf(name)} min={0} max={1} step={0.01} onChange={(w) => setExpression(name, w)} />
              </div>
              <span className="w-8 shrink-0 text-right text-[10px] tabular-nums text-ink-dim">{expressionOf(name).toFixed(2)}</span>
            </div>
          ))}
        </Section>
      )}

      <p className="text-[10px] leading-snug text-ink-dim">
        The selected pose shows here in Design. In a script: <code className="text-ink">ref.setPose3(&apos;{pose.name}&apos;)</code> crossfades to it; <code className="text-ink">ref.pose3</code> reads the current one.
      </p>
    </div>
  )
}
