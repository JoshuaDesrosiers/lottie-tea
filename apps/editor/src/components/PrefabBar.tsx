/**
 * The Inspector's prefab strip (Unity's "Prefab" header row) — shown above
 * the regular Inspector whenever one node is selected:
 *
 * - a plain node: **Create Prefab** (the node's subtree becomes a reusable
 *   asset, and this node its first instance);
 * - a node inside a prefab instance: which prefab, plus **Apply** (push this
 *   instance's changes to the prefab and every other instance), **Revert**
 *   (discard this instance's changes) and **Unpack** (make it plain nodes).
 *   These act on the instance *root* even when a child is selected.
 */
import type { NodeSummary, PrefabSummary } from '../ipc/document'

function idsEqual(a: { idx: number; version: number }, b: { idx: number; version: number }) {
  return a.idx === b.idx && a.version === b.version
}

const BUTTON = 'rounded-md px-2 py-1 text-[11px] font-medium text-ink-dim hover:bg-overlay/8 hover:text-ink disabled:opacity-40'

export function PrefabBar({
  node,
  prefabs,
  onCreate,
  onApply,
  onRevert,
  onUnpack,
  onSelectRoot,
}: {
  node: NodeSummary
  prefabs: PrefabSummary[]
  onCreate: () => void
  onApply: () => void
  onRevert: () => void
  onUnpack: () => void
  onSelectRoot: () => void
}) {
  const link = node.prefab
  const prefab = link ? prefabs.find((p) => idsEqual(p.id, link.prefab)) : undefined

  if (!link || !prefab) {
    return (
      <div className="mb-2 flex items-center justify-between gap-2 rounded-lg border border-dashed border-panel-border px-2 py-1.5">
        <span className="text-[11px] text-ink-dim">Reuse this object anywhere</span>
        <button onClick={onCreate} className="rounded-md bg-accent/15 px-2 py-1 text-[11px] font-medium text-accent hover:bg-accent/25" title="Turn this object (and its children) into a prefab asset">
          Create Prefab
        </button>
      </div>
    )
  }

  return (
    <div className="mb-2 rounded-lg border border-accent/40 bg-accent/8 px-2 py-1.5">
      <div className="flex items-center gap-1.5">
        <span className="text-[10px] font-semibold uppercase tracking-wide text-accent">Prefab</span>
        <span className="min-w-0 flex-1 truncate text-xs font-medium text-ink" title={prefab.name}>
          {prefab.name}
        </span>
        {!link.isRoot && (
          <button onClick={onSelectRoot} className={BUTTON} title="Select the instance root">
            Root
          </button>
        )}
      </div>
      <div className="mt-1 flex gap-1">
        <button onClick={onApply} className={`${BUTTON} text-accent`} title="Write this instance's changes into the prefab — every other instance updates">
          Apply
        </button>
        <button onClick={onRevert} className={BUTTON} title="Discard this instance's changes (keeps its position and name)">
          Revert
        </button>
        <button onClick={onUnpack} className={BUTTON} title="Disconnect from the prefab — becomes ordinary objects">
          Unpack
        </button>
      </div>
    </div>
  )
}
