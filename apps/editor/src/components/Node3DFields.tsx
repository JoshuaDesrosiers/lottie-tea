import { useMemo, useState } from 'react'
import type { DocumentSummary, ImageAssetSummary, MaterialSummary, Node3D, Node3DContent, OpaqueId, Primitive3D, ToonStyle, Vec3Tuple } from '../ipc/document'
import { addToonMaterial, modelClips, modelExtractMaterials, materialToon, pickAndImportHumanoidAnimation, setMaterialToon, type WasmEditor } from '../ipc/wasmEditor'
import { ToonMaterialFields } from './ToonMaterialFields'
import { ColorSwatch } from './ColorPicker'
import { Dropdown, NumberField } from './InspectorPanel'

/**
 * The Inspector's section for a 3D node (`NodeKind::Node3D`): its 3D
 * transform (position / Euler rotation in degrees / scale) and whatever it
 * holds — a primitive mesh, a camera or a light. Every commit sends the
 * whole `Node3D` back (`set_node3d`, one undo step).
 */

const PRIMITIVES: Primitive3D['shape'][] = ['Cube', 'Sphere', 'Plane', 'Cylinder', 'Cone', 'Torus', 'Voxels', 'Glass']

/** Glass `anchors` bits — which edges hold the pane up. */
const GLASS_SIDES: { bit: number; label: string }[] = [
  { bit: 1, label: 'Left' },
  { bit: 2, label: 'Right' },
  { bit: 4, label: 'Bottom' },
  { bit: 8, label: 'Top' },
]

/** A fresh primitive of `shape` with its default parameters (`Model` uses
 * the first imported model). */
function defaultPrimitive(shape: Primitive3D['shape'], firstModel: OpaqueId | null): Primitive3D {
  switch (shape) {
    case 'Model':
      return firstModel ? { shape, model: firstModel } : { shape: 'Cube', size: [1, 1, 1] }
    case 'Cube':
      return { shape, size: [1, 1, 1] }
    case 'Sphere':
      return { shape, radius: 0.5, segments: 32, rings: 16 }
    case 'Plane':
      return { shape, size: 10, subdivisions: 1 }
    case 'Cylinder':
      return { shape, radius: 0.5, height: 1, segments: 32 }
    case 'Cone':
      return { shape, radius: 0.5, height: 1, segments: 32 }
    case 'Torus':
      return { shape, radius: 0.5, tube: 0.18, segments: 40, sides: 16 }
    case 'Voxels':
      return { shape, width: 32, height: 32, depth: 32 }
    case 'Glass':
      return { shape, width: 2, height: 1.5, thickness: 0.02, shard: 0.22, seed: 1, anchors: 15 }
  }
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-[11px] text-ink-dim">{label}</span>
      <div className="flex gap-1">{children}</div>
    </div>
  )
}

function Vec3Row({ label, value, step, onChange }: { label: string; value: Vec3Tuple; step: number; onChange: (v: Vec3Tuple) => void }) {
  return (
    <Row label={label}>
      {(['X', 'Y', 'Z'] as const).map((axis, i) => (
        <NumberField
          key={axis}
          label={axis}
          step={step}
          value={value[i]}
          onCommit={(n) => {
            const next = [...value] as Vec3Tuple
            next[i] = n
            onChange(next)
          }}
        />
      ))}
    </Row>
  )
}

export function Node3DFields({
  value,
  onChange,
  materials,
  onEditMaterial,
  models,
  editor,
  images = [],
  onDocumentChange,
  artboardId = null,
  nodeId = null,
}: {
  value: Node3D
  onChange: (value: Node3D) => void
  /** For Toon materials: texture picker and committing edits. */
  images?: ImageAssetSummary[]
  onDocumentChange?: (doc: DocumentSummary) => void
  /** This node (an avatar's "Extract materials"). */
  artboardId?: OpaqueId | null
  nodeId?: OpaqueId | null
  /** Imported 3D models (Shape → Model). */
  models: { id: OpaqueId; name: string }[]
  editor: WasmEditor | null
  /** Shader-graph materials a mesh can wear. */
  materials: MaterialSummary[]
  onEditMaterial: (material: OpaqueId) => void
}) {
  const keyOf = (id: OpaqueId) => `${id.idx}:${id.version}`
  const { transform, content } = value
  const modelId = content.type === 'Mesh' && content.primitive.shape === 'Model' ? content.primitive.model : null
  const clips = useMemo(() => (editor && modelId ? modelClips(editor, modelId) : []), [editor, modelId])
  const setContent = (c: Node3DContent) => onChange({ ...value, content: c })
  const [openSlot, setOpenSlot] = useState<string | null>(null)
  const [editToon, setEditToon] = useState(true)
  const sameId = (a: OpaqueId | null | undefined, b: OpaqueId | null | undefined) => !!a && !!b && a.idx === b.idx && a.version === b.version
  // This model's own material slots (Toon materials made on VRM import).
  const slotMaterials = modelId ? materials.filter((m) => m.kind === 'Toon' && sameId(m.source?.model, modelId)).sort((a, b) => (a.source?.slot ?? 0) - (b.source?.slot ?? 0)) : []
  const worn = content.type === 'Mesh' && content.material ? materials.find((m) => sameId(m.id, content.material)) : undefined
  const toonFields = (id: OpaqueId) => (onDocumentChange ? <ToonMaterialFields editor={editor} material={id} images={images} revision={materials} onDocumentChange={onDocumentChange} /> : null)
  /** Creates a Toon / Arcane material and puts it on this mesh. */
  const createToon = (style: ToonStyle) => {
    if (!editor || !onDocumentChange || content.type !== 'Mesh') return
    const result = addToonMaterial(editor, style === 'Arcane' ? 'Arcane' : 'Toon', style)
    onDocumentChange(result.document)
    setContent({ ...content, material: result.material })
  }
  /** Every slot material of this avatar switched to one style at once. */
  const restyleSlots = (style: ToonStyle) => {
    if (!editor || !onDocumentChange) return
    let last: DocumentSummary | null = null
    for (const m of slotMaterials) {
      const v = materialToon(editor, m.id)
      if (!v || v.style === style) continue
      const painted = {
        ...v,
        style,
        shade: v.shade.every((c) => c > 0.45) ? ([0.38, 0.36, 0.62] as [number, number, number]) : v.shade,
        rim: v.rim.every((c) => c === 0) ? ([0.25, 0.75, 0.8] as [number, number, number]) : v.rim,
        highlight: [0.18, 0.1, 0.02] as [number, number, number],
        outlineWidth: v.outlineWidth > 0 ? v.outlineWidth : 0.006,
      }
      last = setMaterialToon(editor, m.id, style === 'Arcane' ? painted : { ...v, style }, true) ?? last
    }
    if (last) onDocumentChange(last)
  }
  const ownLabel = modelId ? "Model's own materials" : 'None (flat colour)'

  return (
    <div className="flex flex-col gap-2">
      <Vec3Row label="Position" value={transform.position} step={0.1} onChange={(position) => onChange({ ...value, transform: { ...transform, position } })} />
      <Vec3Row label="Rotation (°)" value={transform.rotationDegrees} step={5} onChange={(rotationDegrees) => onChange({ ...value, transform: { ...transform, rotationDegrees } })} />
      <Vec3Row label="Scale" value={transform.scale} step={0.1} onChange={(scale) => onChange({ ...value, transform: { ...transform, scale } })} />

      {content.type === 'Mesh' && (
        <div className="flex flex-col gap-2 rounded-md bg-overlay/5 p-2">
          <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">Mesh</span>
          <Row label="Shape">
            <div className="flex-1">
              <Dropdown
                value={content.primitive.shape}
                options={models.length ? [...PRIMITIVES, 'Model' as const] : PRIMITIVES}
                renderValue={(v) => (v === 'Model' ? 'Imported Model' : v)}
                renderOption={(v) => (v === 'Model' ? 'Imported Model' : v)}
                onChange={(shape) => setContent({ ...content, primitive: defaultPrimitive(shape, models[0]?.id ?? null), animation: null, animationOverlay: null })}
              />
            </div>
          </Row>
          {content.primitive.shape === 'Model' ? (
            <>
              <Row label="Model">
                <div className="min-w-0 flex-1">
                  <Dropdown
                    value={keyOf(content.primitive.model)}
                    options={models.map((m) => keyOf(m.id))}
                    renderValue={(k) => models.find((m) => keyOf(m.id) === k)?.name ?? 'Missing model'}
                    renderOption={(k) => models.find((m) => keyOf(m.id) === k)?.name ?? k}
                    onChange={(k) => {
                      const found = models.find((m) => keyOf(m.id) === k)
                      if (found) setContent({ ...content, primitive: { shape: 'Model', model: found.id }, animation: null, animationOverlay: null })
                    }}
                  />
                </div>
              </Row>
              {clips.length > 0 && (
                <>
                  <Row label="Animation / overlay">
                    {(['animation', 'animationOverlay'] as const).map((field) => (
                      <div key={field} className="min-w-0 flex-1">
                        <Dropdown
                          value={content[field] ?? ''}
                          options={['', ...clips]}
                          renderValue={(v) => v || 'None'}
                          renderOption={(v) => v || 'None'}
                          onChange={(v) => setContent({ ...content, [field]: v || null })}
                        />
                      </div>
                    ))}
                  </Row>
                  <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
                    <input type="checkbox" checked={content.animationLoop} onChange={(e) => setContent({ ...content, animationLoop: e.target.checked })} />
                    Loop the animation
                  </label>
                </>
              )}
            </>
          ) : (
            <PrimitiveParams primitive={content.primitive} onChange={(primitive) => setContent({ ...content, primitive })} />
          )}
          <Row label="Color (material tint)">
            <ColorSwatch value={content.color} onChange={(color) => setContent({ ...content, color })} />
          </Row>
          <Row label="Material">
            <div className="min-w-0 flex-1">
              <Dropdown
                value={content.material ? keyOf(content.material) : 'none'}
                options={['none', ...materials.map((m) => keyOf(m.id)), ...(onDocumentChange ? ['new:MToon', 'new:Arcane'] : [])]}
                renderValue={(k) => (k === 'none' ? ownLabel : (materials.find((m) => keyOf(m.id) === k)?.name ?? 'Missing material'))}
                renderOption={(k) =>
                  k === 'none' ? ownLabel : k === 'new:MToon' ? '+ New Toon material' : k === 'new:Arcane' ? '+ New Arcane (painted) material' : (materials.find((m) => keyOf(m.id) === k)?.name ?? k)
                }
                onChange={(k) => {
                  if (k === 'new:MToon' || k === 'new:Arcane') createToon(k === 'new:Arcane' ? 'Arcane' : 'MToon')
                  else setContent({ ...content, material: materials.find((m) => keyOf(m.id) === k)?.id ?? null })
                }}
              />
            </div>
            <button
              onClick={() => (worn?.kind === 'Toon' ? setEditToon(!editToon) : content.material && onEditMaterial(content.material))}
              disabled={!content.material}
              title="Edit this material's node graph (in 3D, Local Position is world meters on the surface — patterns tile at world scale)"
              className="shrink-0 rounded bg-overlay/10 px-2 py-1 text-[11px] text-ink hover:bg-accent/20 hover:text-accent disabled:opacity-35"
            >
              Edit
            </button>
          </Row>
          {worn?.kind === 'Toon' && editToon && <div className="rounded-md border border-line/50 p-2">{toonFields(worn.id)}</div>}
          {modelId && editor && artboardId && nodeId && onDocumentChange && (
            <button
              type="button"
              onClick={() =>
                pickAndImportHumanoidAnimation(editor, artboardId, nodeId)
                  .then((doc) => doc && onDocumentChange(doc))
                  .catch((err) => window.alert(`Couldn't use that animation: ${err}`))
              }
              className="rounded bg-overlay/10 px-2 py-1 text-[11px] text-ink hover:bg-accent/20 hover:text-accent"
              title="Play any humanoid animation on this avatar: a VRMA file, or a GLB / VRM with a humanoid skeleton (Mixamo → Blender → GLB works)"
            >
              Import animation (.vrma / .glb)…
            </button>
          )}
          {modelId && !content.material && (slotMaterials.length > 0 || (editor && artboardId && nodeId)) && (
            <div className="flex flex-col gap-1.5 rounded-md border border-line/50 p-2">
              <div className="flex items-center gap-1">
                <span className="flex-1 text-[11px] font-medium text-ink">Model materials</span>
                {slotMaterials.length > 0 && (
                  <>
                    <button type="button" onClick={() => restyleSlots('MToon')} className="rounded px-1.5 py-0.5 text-[10px] text-ink-dim hover:bg-overlay/10 hover:text-ink" title="Every slot back to the MToon look">
                      All MToon
                    </button>
                    <button type="button" onClick={() => restyleSlots('Arcane')} className="rounded px-1.5 py-0.5 text-[10px] text-ink-dim hover:bg-overlay/10 hover:text-ink" title="Every slot painted, Arcane-style">
                      All Arcane
                    </button>
                  </>
                )}
              </div>
              {slotMaterials.length === 0 ? (
                <button
                  type="button"
                  onClick={() => {
                    if (!editor || !artboardId || !nodeId || !onDocumentChange) return
                    const next = modelExtractMaterials(editor, artboardId, nodeId)
                    if (next) onDocumentChange(next)
                  }}
                  className="rounded bg-overlay/10 px-2 py-1 text-[11px] text-ink hover:bg-accent/20 hover:text-accent"
                  title="Make an editable material for each of this avatar's MToon materials"
                >
                  Extract materials
                </button>
              ) : (
                slotMaterials.map((m) => (
                  <div key={keyOf(m.id)} className="flex flex-col gap-1">
                    <button type="button" onClick={() => setOpenSlot(openSlot === keyOf(m.id) ? null : keyOf(m.id))} className="flex items-center gap-1 text-left text-[11px] text-ink-dim hover:text-ink">
                      <span>{openSlot === keyOf(m.id) ? '▾' : '▸'}</span>
                      <span className="min-w-0 flex-1 truncate">{m.name.split(' · ').slice(-1)[0]}</span>
                      <span className="rounded bg-overlay/10 px-1 text-[9px]">{m.style}</span>
                    </button>
                    {openSlot === keyOf(m.id) && <div className="pl-3">{toonFields(m.id)}</div>}
                  </div>
                ))
              )}
              <p className="text-[10px] leading-snug text-ink-dim">Shared by every copy of this model. Picking a material above overrides all slots at once.</p>
            </div>
          )}
          <Row label="Shininess">
            <NumberField label="" step={0.05} value={content.shininess} onCommit={(shininess) => setContent({ ...content, shininess: Math.min(1, Math.max(0, shininess)) })} />
          </Row>
          <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
            <input type="checkbox" checked={content.unlit} onChange={(e) => setContent({ ...content, unlit: e.target.checked })} />
            Unlit (ignore lights)
          </label>
        </div>
      )}

      {content.type === 'Camera' && (
        <div className="flex flex-col gap-2 rounded-md bg-overlay/5 p-2">
          <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">Camera 3D</span>
          <Row label="Field of view (°) / Near / Far">
            <NumberField label="FOV" step={1} value={content.fovDegrees} onCommit={(fovDegrees) => setContent({ ...content, fovDegrees })} />
            <NumberField label="N" step={0.01} value={content.near} onCommit={(near) => setContent({ ...content, near })} />
            <NumberField label="F" step={10} value={content.far} onCommit={(far) => setContent({ ...content, far })} />
          </Row>
          <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
            <input type="checkbox" checked={content.current} onChange={(e) => setContent({ ...content, current: e.target.checked })} />
            Current (the scene renders through this camera)
          </label>
          <label className="flex items-center gap-1.5 text-[11px] text-ink-dim">
            <input type="checkbox" checked={content.clearColor !== null} onChange={(e) => setContent({ ...content, clearColor: e.target.checked ? [0.45, 0.62, 0.85, 1] : null })} />
            Sky colour
            {content.clearColor && <ColorSwatch value={content.clearColor} onChange={(clearColor) => setContent({ ...content, clearColor })} />}
          </label>
        </div>
      )}

      {content.type === 'Light' && (
        <div className="flex flex-col gap-2 rounded-md bg-overlay/5 p-2">
          <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">Light 3D</span>
          <Row label="Type">
            <div className="flex-1">
              <Dropdown<'Directional' | 'Point'> value={content.kind} options={['Directional', 'Point']} onChange={(kind) => setContent({ ...content, kind })} />
            </div>
          </Row>
          <Row label="Color">
            <ColorSwatch value={[...content.color, 1]} onChange={(c) => setContent({ ...content, color: [c[0], c[1], c[2]] })} />
          </Row>
          <Row label={content.kind === 'Point' ? 'Intensity / Range' : 'Intensity'}>
            <NumberField label="I" step={0.1} value={content.intensity} onCommit={(intensity) => setContent({ ...content, intensity: Math.max(0, intensity) })} />
            {content.kind === 'Point' && <NumberField label="R" step={0.5} value={content.range} onCommit={(range) => setContent({ ...content, range: Math.max(0.01, range) })} />}
          </Row>
          {content.kind === 'Directional' && <p className="text-[10px] text-ink-dim">Shines along the node's local −Z: rotate it to aim the sun.</p>}
        </div>
      )}
    </div>
  )
}

function PrimitiveParams({ primitive, onChange }: { primitive: Primitive3D; onChange: (p: Primitive3D) => void }) {
  const num = (label: string, v: number, step: number, set: (n: number) => void) => <NumberField key={label} label={label} step={step} value={v} onCommit={(n) => set(Math.max(0, n))} />
  switch (primitive.shape) {
    case 'Cube':
      return <Vec3Row label="Size" value={primitive.size} step={0.1} onChange={(size) => onChange({ ...primitive, size })} />
    case 'Sphere':
      return (
        <Row label="Radius / Detail">
          {num('R', primitive.radius, 0.05, (radius) => onChange({ ...primitive, radius }))}
          {num('Seg', primitive.segments, 1, (segments) => onChange({ ...primitive, segments: Math.round(segments), rings: Math.max(2, Math.round(segments / 2)) }))}
        </Row>
      )
    case 'Plane':
      return <Row label="Size">{num('S', primitive.size, 0.5, (size) => onChange({ ...primitive, size }))}</Row>
    case 'Cylinder':
    case 'Cone':
      return (
        <Row label="Radius / Height">
          {num('R', primitive.radius, 0.05, (radius) => onChange({ ...primitive, radius }))}
          {num('H', primitive.height, 0.1, (height) => onChange({ ...primitive, height }))}
        </Row>
      )
    case 'Model':
      return null
    case 'Voxels':
      // A block world's size in blocks; its blocks come from a script
      // (`ref.setVoxels` / `ref.setVoxel`).
      return (
        <Row label="Blocks (W / H / D)">
          {num('W', primitive.width, 1, (width) => onChange({ ...primitive, width: Math.max(1, Math.round(width)) }))}
          {num('H', primitive.height, 1, (height) => onChange({ ...primitive, height: Math.max(1, Math.round(height)) }))}
          {num('D', primitive.depth, 1, (depth) => onChange({ ...primitive, depth: Math.max(1, Math.round(depth)) }))}
        </Row>
      )
    case 'Glass':
      // Breakable glass: size, how big its pieces are, and which edges hold
      // it (pieces cut off from every held edge fall when it's hit).
      return (
        <div className="flex flex-col gap-1.5">
          <Row label="Size (W / H / Thickness)">
            {num('W', primitive.width, 0.1, (width) => onChange({ ...primitive, width: Math.max(0.01, width) }))}
            {num('H', primitive.height, 0.1, (height) => onChange({ ...primitive, height: Math.max(0.01, height) }))}
            {num('T', primitive.thickness, 0.005, (thickness) => onChange({ ...primitive, thickness: Math.max(0.0005, thickness) }))}
          </Row>
          <Row label="Piece size / Pattern seed">
            {num('S', primitive.shard, 0.02, (shard) => onChange({ ...primitive, shard: Math.max(0.01, shard) }))}
            {num('#', primitive.seed, 1, (seed) => onChange({ ...primitive, seed: Math.max(0, Math.round(seed)) }))}
          </Row>
          <div className="flex flex-col gap-1">
            <span className="text-[11px] text-ink-dim" title="A hit that cuts pieces off from every held edge makes them fall: a bar held top and bottom loses a bite, a hanging pane drops everything below the break.">
              Held by edges
            </span>
            <div className="flex flex-wrap gap-2">
              {GLASS_SIDES.map(({ bit, label }) => (
                <label key={bit} className="flex items-center gap-1 text-[11px] text-ink-dim">
                  <input type="checkbox" checked={(primitive.anchors & bit) !== 0} onChange={(e) => onChange({ ...primitive, anchors: e.target.checked ? primitive.anchors | bit : primitive.anchors & ~bit })} />
                  {label}
                </label>
              ))}
            </div>
          </div>
          <p className="text-[10px] leading-snug text-ink-dim">Scripts break it: <code>ref.shatter3(point, velocity)</code>, find standing glass with <code>ctx.glassRaycast3</code>. Materials get uv.x = 1 on edges, uv.y = cracks.</p>
        </div>
      )
    case 'Torus':
      return (
        <Row label="Radius / Tube">
          {num('R', primitive.radius, 0.05, (radius) => onChange({ ...primitive, radius }))}
          {num('T', primitive.tube, 0.02, (tube) => onChange({ ...primitive, tube }))}
        </Row>
      )
  }
}
