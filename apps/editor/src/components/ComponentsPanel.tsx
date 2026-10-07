import { useState } from 'react'
import type { ComponentDefinitionSummary, NodeSummary, OpaqueId } from '../ipc/document'

const CONTROL = 'w-full min-w-0 rounded-md bg-overlay/8 px-2 py-1.5 text-xs text-ink outline-none focus:bg-overlay/12'
const BUTTON = 'w-full rounded-md bg-accent/15 px-2 py-1.5 text-xs font-medium text-accent hover:bg-accent/25'
const LABEL = 'text-[11px] font-medium uppercase tracking-wide text-ink-dim'

/** Converts a straight linear RGBA `[f32;4]` (0.0..=1.0, this workspace's
 * saved-color convention everywhere else — `ShapeNode::fill_color` etc.)
 * to/from the `<input type="color">` hex string HTML wants. Alpha isn't
 * editable through this control (a native color input has no alpha
 * channel) — every color this panel writes keeps whatever alpha it already
 * had, defaulting to fully opaque for a brand-new component/variant. */
function toHex([r, g, b]: [number, number, number, number]): string {
  const byte = (c: number) => Math.round(Math.min(1, Math.max(0, c)) * 255).toString(16).padStart(2, '0')
  return `#${byte(r)}${byte(g)}${byte(b)}`
}
function fromHex(hex: string, alpha: number): [number, number, number, number] {
  const r = Number.parseInt(hex.slice(1, 3), 16) / 255
  const g = Number.parseInt(hex.slice(3, 5), 16) / 255
  const b = Number.parseInt(hex.slice(5, 7), 16) / 255
  return [r, g, b, alpha]
}

/**
 * The editor's Components tab (`TopToolbar`'s "Components" mode) — the
 * library-management half of component authoring: create a definition from
 * a selected shape, browse existing definitions, edit a definition's
 * default color and named variants, and drop a new instance of one onto the
 * artboard. Per-instance editing (which variant an *already-placed*
 * instance uses, its own direct color override) lives in `InspectorPanel`
 * instead — that's a property of one selected node, the same "component
 * computes, App talks to WasmEditor" split every panel here follows, not
 * something the library view itself needs to know about.
 *
 * `an1mbyt3_core::ComponentDefinition`'s own scope note applies here too:
 * one shape's path plus one overridable property (fill color) — no nested
 * node trees, no multiple parameter types. This panel's UI is exactly that
 * narrow on purpose, not a placeholder for a bigger version.
 */
export function ComponentsPanel({
  components,
  selectedNode,
  onAddComponent,
  onDeleteComponent,
  onSetDefaultFillColor,
  onAddVariant,
  onAddInstance,
}: {
  components: ComponentDefinitionSummary[]
  selectedNode: NodeSummary | null
  onAddComponent: (name: string) => void
  onDeleteComponent: (id: OpaqueId) => void
  onSetDefaultFillColor: (id: OpaqueId, color: [number, number, number, number]) => void
  onAddVariant: (id: OpaqueId, name: string, color: [number, number, number, number]) => void
  onAddInstance: (id: OpaqueId) => void
}) {
  const [newComponentName, setNewComponentName] = useState('')

  return (
    <div className="flex flex-col gap-4 overflow-x-hidden">
      <div className="flex flex-col gap-1.5">
        <span className={LABEL}>Make Component</span>
        {selectedNode?.kind === 'Shape' ? (
          <>
            <input className={CONTROL} placeholder="Component name" value={newComponentName} onChange={(e) => setNewComponentName(e.target.value)} />
            <button
              onClick={() => {
                if (!newComponentName.trim()) return
                onAddComponent(newComponentName.trim())
                setNewComponentName('')
              }}
              className={BUTTON}
            >
              Make Component from Selection
            </button>
          </>
        ) : (
          <p className="text-xs text-ink-dim">Select a shape on the canvas to bundle it into a reusable component.</p>
        )}
      </div>

      <div className="flex flex-col gap-2">
        <span className={LABEL}>Library</span>
        {components.length === 0 && <p className="text-xs text-ink-dim">No components yet.</p>}
        {components.map((component) => (
          <ComponentRow
            key={`${component.id.idx}:${component.id.version}`}
            component={component}
            onDelete={() => onDeleteComponent(component.id)}
            onSetDefaultFillColor={(color) => onSetDefaultFillColor(component.id, color)}
            onAddVariant={(name, color) => onAddVariant(component.id, name, color)}
            onAddInstance={() => onAddInstance(component.id)}
          />
        ))}
      </div>
    </div>
  )
}

function ComponentRow({
  component,
  onDelete,
  onSetDefaultFillColor,
  onAddVariant,
  onAddInstance,
}: {
  component: ComponentDefinitionSummary
  onDelete: () => void
  onSetDefaultFillColor: (color: [number, number, number, number]) => void
  onAddVariant: (name: string, color: [number, number, number, number]) => void
  onAddInstance: () => void
}) {
  const [newVariantName, setNewVariantName] = useState('')
  const [newVariantColor, setNewVariantColor] = useState('#ffffff')

  return (
    <div className="flex flex-col gap-1.5 rounded-md border border-overlay/10 bg-overlay/5 p-2">
      <div className="flex items-center gap-2">
        <input
          type="color"
          value={toHex(component.defaultFillColor)}
          onChange={(e) => onSetDefaultFillColor(fromHex(e.target.value, component.defaultFillColor[3]))}
          className="h-6 w-6 shrink-0 cursor-pointer rounded border border-overlay/20 bg-transparent p-0"
        />
        <span className="min-w-0 flex-1 truncate text-xs font-medium text-ink" title={component.name}>
          {component.name}
        </span>
        <button onClick={onDelete} className="shrink-0 rounded bg-red-500/10 px-1.5 py-0.5 text-[10px] text-red-300 hover:bg-red-500/20">
          Delete
        </button>
      </div>

      {component.variants.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {component.variants.map((variant) => (
            <span
              key={variant.name}
              className="flex items-center gap-1 rounded-full bg-overlay/8 px-2 py-0.5 text-[10px] text-ink-dim"
              title={`${variant.name} variant`}
            >
              <span className="h-2 w-2 rounded-full" style={{ backgroundColor: toHex(variant.fillColor) }} />
              {variant.name}
            </span>
          ))}
        </div>
      )}

      <div className="flex gap-1">
        <input
          type="color"
          value={newVariantColor}
          onChange={(e) => setNewVariantColor(e.target.value)}
          className="h-7 w-7 shrink-0 cursor-pointer rounded border border-overlay/20 bg-transparent p-0"
        />
        <input
          className={`${CONTROL} flex-1`}
          placeholder="Variant name"
          value={newVariantName}
          onChange={(e) => setNewVariantName(e.target.value)}
        />
        <button
          onClick={() => {
            if (!newVariantName.trim()) return
            onAddVariant(newVariantName.trim(), fromHex(newVariantColor, 1))
            setNewVariantName('')
          }}
          className="shrink-0 rounded-md bg-accent/15 px-2 py-1 text-xs font-medium text-accent hover:bg-accent/25"
        >
          + Variant
        </button>
      </div>

      <button onClick={onAddInstance} className={BUTTON}>
        Add Instance to Artboard
      </button>
    </div>
  )
}
