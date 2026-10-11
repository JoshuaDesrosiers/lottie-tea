import { useEffect, useMemo, useState } from 'react'
import type { DocumentSummary, MaterialStatus, NodeBehaviorEntry, NodeSummary, OpaqueId, ShaderGraphDef, ShaderNodeSpec } from '../ipc/document'
import { addMaterial, addToonMaterial, deleteMaterial, materialGraph, materialStatus, renameMaterial, setMaterialGraph, shaderNodeCatalog, type WasmEditor } from '../ipc/wasmEditor'
import { ShaderGraphEditor } from './ShaderGraphEditor'
import { ToonMaterialFields } from './ToonMaterialFields'
import { Dropdown } from './InspectorPanel'

const same = (a: OpaqueId | null | undefined, b: OpaqueId | null | undefined) => !!a && !!b && a.idx === b.idx && a.version === b.version
const key = (id: OpaqueId) => `${id.idx}:${id.version}`

/**
 * The bottom dock's **Shader** tab: pick / create a material, edit its
 * node graph, and put it on the selected sprite or shape.
 *
 * A material is a document asset; a node wears one through its Material
 * component (`SpriteRenderer` behavior). The canvas shows the result live
 * — the engine recompiles a material the frame after its graph changes,
 * and the status pill here reports graph errors (no Output, a loop) or
 * shader compile errors.
 */
export function ShaderDock({
  editor,
  doc,
  materialId,
  onSelectMaterial,
  onDocumentChange,
  selectedNode,
  onSetSelectedBehaviors,
}: {
  editor: WasmEditor | null
  doc: DocumentSummary | null
  materialId: OpaqueId | null
  onSelectMaterial: (id: OpaqueId | null) => void
  onDocumentChange: (doc: DocumentSummary) => void
  /** The selected node (to "Apply" the material to), if any. */
  selectedNode: NodeSummary | null
  onSetSelectedBehaviors: (behaviors: NodeBehaviorEntry[]) => void
}) {
  const materials = doc?.materials ?? []
  const current = materials.find((m) => same(m.id, materialId)) ?? null
  const catalog = useMemo<ShaderNodeSpec[]>(() => (editor ? shaderNodeCatalog(editor) : []), [editor])
  const [graph, setGraph] = useState<ShaderGraphDef | null>(null)
  const [status, setStatus] = useState<MaterialStatus | null>(null)
  const [showCode, setShowCode] = useState(false)
  const [renaming, setRenaming] = useState<string | null>(null)

  // Fall back to the first material when nothing (or a deleted one) is picked.
  useEffect(() => {
    if (!current && materials.length) onSelectMaterial(materials[0].id)
  }, [current, materials, onSelectMaterial])

  // Load the graph whenever the material or the document (undo!) changes.
  useEffect(() => {
    setGraph(editor && current ? materialGraph(editor, current.id) : null)
  }, [editor, current, doc])

  // Status: graph errors are immediate; a shader compile error appears once
  // the engine has tried to build it (next frame) — so ask again shortly.
  useEffect(() => {
    if (!editor || !current) {
      setStatus(null)
      return
    }
    setStatus(materialStatus(editor, current.id))
    const t = window.setTimeout(() => setStatus(materialStatus(editor, current.id)), 120)
    return () => window.clearTimeout(t)
  }, [editor, current, graph])

  const commitGraph = (next: ShaderGraphDef) => {
    if (!editor || !current) return
    setGraph(next)
    onDocumentChange(setMaterialGraph(editor, current.id, next))
  }

  const create = () => {
    if (!editor) return
    const name = `Material ${materials.length + 1}`
    const result = addMaterial(editor, name)
    onDocumentChange(result.document)
    onSelectMaterial(result.material)
  }
  const createToon = (style: 'MToon' | 'Arcane') => {
    if (!editor) return
    const result = addToonMaterial(editor, style === 'Arcane' ? `Arcane ${materials.length + 1}` : `Toon ${materials.length + 1}`, style)
    onDocumentChange(result.document)
    onSelectMaterial(result.material)
  }
  const isToon = current?.kind === 'Toon'

  /** Put the current material on the selected node (adding or updating its
   * Material component). */
  const applyToSelection = () => {
    if (!current || !selectedNode) return
    const existing = selectedNode.behaviors.findIndex((b) => b.behavior.kind === 'SpriteRenderer')
    const behavior = { kind: 'SpriteRenderer' as const, material: current.id, color: [1, 1, 1, 1] as [number, number, number, number] }
    const next =
      existing >= 0
        ? selectedNode.behaviors.map((b, i) => (i === existing ? { enabled: true, behavior: { ...behavior, color: b.behavior.kind === 'SpriteRenderer' ? b.behavior.color : behavior.color } } : b))
        : [...selectedNode.behaviors, { enabled: true, behavior }]
    onSetSelectedBehaviors(next)
  }
  const selectedWears =
    !!selectedNode && !!current && selectedNode.behaviors.some((b) => b.behavior.kind === 'SpriteRenderer' && same(b.behavior.material, current.id))
  const canApply = !!selectedNode && (selectedNode.kind === 'Shape' || selectedNode.kind === 'Image')

  const button = 'rounded px-2 py-1 text-[11px] text-ink-dim hover:bg-overlay/10 hover:text-ink disabled:opacity-35 disabled:hover:bg-transparent'

  return (
    <div className="flex h-full min-h-0 flex-col gap-1.5">
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">Shader</span>
        {materials.length > 0 && current && (
          <div className="w-44">
            {renaming !== null ? (
              <input
                autoFocus
                value={renaming}
                onChange={(e) => setRenaming(e.target.value)}
                onBlur={() => {
                  if (editor && renaming.trim()) onDocumentChange(renameMaterial(editor, current.id, renaming.trim()))
                  setRenaming(null)
                }}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') (e.target as HTMLInputElement).blur()
                  if (e.key === 'Escape') setRenaming(null)
                }}
                className="w-full rounded bg-overlay/10 px-1.5 py-1 text-[11px] text-ink outline-none focus:ring-1 focus:ring-accent"
              />
            ) : (
              <Dropdown
                value={key(current.id)}
                options={materials.map((m) => key(m.id))}
                renderValue={(k) => materials.find((m) => key(m.id) === k)?.name ?? k}
                renderOption={(k) => materials.find((m) => key(m.id) === k)?.name ?? k}
                onChange={(k) => onSelectMaterial(materials.find((m) => key(m.id) === k)?.id ?? null)}
              />
            )}
          </div>
        )}
        <button onClick={create} disabled={!editor} className="rounded bg-accent px-2 py-1 text-[11px] font-medium text-white hover:opacity-90 disabled:opacity-40">
          + Shader Graph
        </button>
        <button onClick={() => createToon('MToon')} disabled={!editor} className={button} title="A toon material for 3D meshes (VRM's MToon look)">
          + Toon
        </button>
        <button onClick={() => createToon('Arcane')} disabled={!editor} className={button} title="A painterly 3D material: brushy banded light, hatched shadows, grain">
          + Arcane
        </button>
        {current && (
          <>
            <button onClick={() => setRenaming(current.name)} className={button}>
              Rename
            </button>
            <button onClick={() => editor && onDocumentChange(deleteMaterial(editor, current.id))} className={`${button} hover:!text-red-400`}>
              Delete
            </button>
            <span className="mx-1 h-4 w-px bg-overlay/15" />
            {!isToon && <button
              onClick={applyToSelection}
              disabled={!canApply || selectedWears}
              title={canApply ? 'Give the selected sprite / shape this material' : 'Select a sprite (Image) or a shape to apply this material to'}
              className={button}
            >
              {selectedWears ? 'Applied to selection ✓' : 'Apply to Selection'}
            </button>}
            {isToon && <span className="text-[11px] text-ink-dim">Put it on a 3D mesh with the Inspector&apos;s Material picker.</span>}
            <span className="flex-1" />
            {!isToon && status &&
              (status.error ? (
                <span title={status.error} className="max-w-[28rem] truncate rounded bg-red-500/15 px-2 py-0.5 text-[11px] text-red-500">
                  {status.error.split('\n')[0]}
                </span>
              ) : (
                <span className="rounded bg-emerald-500/15 px-2 py-0.5 text-[11px] text-emerald-600">Compiled</span>
              ))}
            {!isToon && <button onClick={() => setShowCode((v) => !v)} className={`${button} ${showCode ? 'bg-overlay/10 text-ink' : ''}`}>
              WGSL
            </button>}
          </>
        )}
      </div>

      {!current ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 rounded-md border border-dashed border-overlay/15 text-center">
          <p className="max-w-sm text-[12px] text-ink-dim">
            Materials are node-built shaders you can put on any sprite or shape — scrolling textures, glows, dissolves, colour shifts. Create one, wire some nodes, then
            press <span className="text-ink">Apply to Selection</span>.
          </p>
          <button onClick={create} disabled={!editor} className="rounded bg-accent px-3 py-1.5 text-[12px] font-medium text-white hover:opacity-90">
            + New Material
          </button>
        </div>
      ) : isToon ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <div className="max-w-md">
            <ToonMaterialFields editor={editor} material={current.id} images={doc?.images ?? []} revision={doc} onDocumentChange={onDocumentChange} />
          </div>
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 gap-1.5">
          <div className="min-w-0 flex-1">{graph && <ShaderGraphEditor graph={graph} catalog={catalog} errorNode={status?.errorNode ?? null} onChange={commitGraph} />}</div>
          {showCode && (
            <pre className="w-96 shrink-0 overflow-auto rounded-md border border-overlay/10 bg-overlay/5 p-2 font-mono text-[10px] leading-snug text-ink-dim">
              {status?.wgsl ?? status?.error ?? ''}
            </pre>
          )}
        </div>
      )}
    </div>
  )
}
