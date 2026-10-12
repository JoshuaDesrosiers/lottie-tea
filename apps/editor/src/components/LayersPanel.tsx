import { useEffect, useLayoutEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent, type ReactNode } from 'react'
import type { ArtboardSummary, NodeSummary, OpaqueId } from '../ipc/document'
import { CircleIconButton } from './CircleIconButton'
import {
  BoneKindIcon,
  CameraKindIcon,
  LightKindIcon,
  TrailKindIcon,
  ChevronDownIcon,
  EffectsIcon,
  GroupKindIcon,
  ImageIcon,
  InstanceKindIcon,
  MaskIcon,
  PlusIcon,
  ShapeKindIcon,
  TextKindIcon,
  TileGridIcon,
} from './icons'

function idsEqual(a: OpaqueId, b: OpaqueId): boolean {
  return a.idx === b.idx && a.version === b.version
}

function idKey(id: OpaqueId): string {
  return `${id.idx}-${id.version}`
}

function sameParent(a: OpaqueId | null, b: OpaqueId | null): boolean {
  if (a === null || b === null) return a === b
  return idsEqual(a, b)
}

/** The node that comes immediately after `node` among its *real* (backend
 * draw-order, not the panel's reversed display order — see `toDisplayOrder`'s
 * own doc comment) siblings, or `null` if `node` is already the frontmost.
 * What the "sibling" drop zone actually needs to pass as `reparentNodeBefore`'s
 * `before` argument: dropping on the *top* half of a row in the (reversed)
 * panel means "make the dragged node more frontmost than this one," which in
 * real backend terms is "insert it immediately *after* this one" — i.e.
 * immediately *before* whichever real sibling already follows it. */
function realNextSibling(node: NodeSummary, allNodes: NodeSummary[]): OpaqueId | null {
  const siblings = allNodes.filter((n) => sameParent(n.parent, node.parent))
  const index = siblings.findIndex((n) => idsEqual(n.id, node.id))
  return index >= 0 && index + 1 < siblings.length ? siblings[index + 1].id : null
}

/** Custom `dataTransfer` MIME type for a dragged layer row — a plain
 * `idKey`-shaped string, just enough for the drop target to look the
 * dragged node back up in `nodesById`. Not a native HTML5 drag payload
 * format (there isn't a meaningful one for "a node in our own document"),
 * so a private type keeps this from ever accidentally matching a drop
 * coming from outside the app. */
const DRAG_MIME = 'application/x-an1mbyt3-node'

/** One icon glyph per `NodeKind`, so a layer's shape is recognizable at a
 * glance without reading its name — Rive/Figma-style layer-type
 * iconography. Artboards have no `kind` (they're `Artboard`, not `Node`),
 * so this only ever renders for actual node pills, not the outer artboard
 * pill. */
/** The glyph a layer shows: a plain Group carrying a Camera / Light 2D /
 * Trail / Particle System component reads as that component (Unity shows
 * the same GameObject either way, but the icon tells you what it's *for*). */
function iconKind(node: NodeSummary): NodeSummary['kind'] {
  if (node.kind !== 'Group') return node.kind
  if (node.camera !== null) return 'Camera'
  if (node.light2d !== null) return 'Light2D'
  if (node.trail !== null) return 'Trail'
  if (node.particleSystem !== null) return 'ParticleSystem'
  return 'Group'
}

function KindIcon({ kind }: { kind: NodeSummary['kind'] }) {
  switch (kind) {
    case 'Shape':
      return <ShapeKindIcon />
    case 'Group':
      return <GroupKindIcon />
    case 'Bone':
      return <BoneKindIcon />
    case 'Instance':
      return <InstanceKindIcon />
    case 'Text':
      return <TextKindIcon />
    case 'Image':
      return <ImageIcon />
    case 'ParticleSystem':
      return <EffectsIcon />
    case 'Tilemap':
      return <TileGridIcon />
    case 'Camera':
      return <CameraKindIcon />
    case 'Light2D':
      return <LightKindIcon />
    case 'Trail':
      return <TrailKindIcon />
  }
}

/**
 * One node's pill tag (shape reference: cartex) — a rounded capsule rather
 * than a plain text row. Kept neutral/restrained (one subtle translucent
 * fill, not per-type saturated colors) per AN1MBYT3_BUILD_PLAN.md §15's
 * single-accent-color rule. Per-kind icons (`KindIcon` above) give a
 * neutral-colored glyph instead of a colored tag — legible without being loud.
 * `expandable` nodes (ones that can contain children) get expand/add-child
 * circle buttons alongside; plain leaf nodes don't. `selected` highlights the
 * currently-selected node so `InspectorPanel` and this panel visibly agree
 * on what's selected.
 *
 * `maskBadge`, when set, renders a small `MaskIcon` after the name —
 * `"group"` for a `Group` row that currently has a mask set (its own
 * indicator that *something* is clipping its contents), `"child"` for the
 * specific child row that *is* that mask (dimmer, since it's descriptive
 * rather than a group-level fact). Clicking the badge clears the mask
 * (`onClearMask`) — see `LayerRow`'s doc comment for how it's *set* (drag).
 *
 * Double-clicking the name swaps it for a real text input, committing
 * (`onRename`) on blur or Enter, reverting on Escape — the standard
 * "double-click to rename in place" convention every layer/file-tree UI
 * uses, rather than a separate rename dialog or Inspector-only field.
 */
/** Sorting layer id → name, for the rows' layer tags (set by
 * `LayersPanel` from its `sortingLayers` prop). */
let sortingLayerNames: Record<number, string> = {}

function LayerPill({
  name,
  kind,
  expandable,
  hasChildren,
  collapsed,
  onToggleCollapse,
  selected,
  maskBadge,
  clipsChildren,
  onToggleClipChildren,
  dragOverZone,
  onClick,
  onRename,
  onClearMask,
  onAddChild,
  layerMaskCheckbox,
  prefab,
  sortingLayer,
}: {
  /** Unity's blue prefab tint: `'root'` on an instance root, `'part'` on
   * the nodes inside it. */
  prefab?: 'root' | 'part'
  /** The sorting layer this row sets (shown as a tag — it draws above or
   * below other layers whatever its place in this list). */
  sortingLayer?: string
  name: string
  kind?: NodeSummary['kind']
  expandable?: boolean
  /** Whether this row actually has anything to hide/show — the chevron only
   * renders when `true` (a group with no children has nothing to collapse). */
  hasChildren?: boolean
  collapsed?: boolean
  onToggleCollapse?: () => void
  selected?: boolean
  maskBadge?: 'group' | 'child'
  /** `Node::clip_children` — this row's own shape clips whatever's nested
   * inside it (the "child" drop zone's default action, or the Inspector's
   * own checkbox). A separate badge from `maskBadge`'s `GroupNode::mask`
   * mechanism — the two are independent, and a row could in principle show
   * both. */
  clipsChildren?: boolean
  onToggleClipChildren?: () => void
  /** Which drop zone is currently hovered while dragging another row over
   * this one — `'child'` (drop on the row's lower half) reparents the
   * dragged node here (and, per `LayerRow`'s own doc comment, masks it by
   * default); `'sibling'` (upper half) inserts it as a sibling at *this*
   * row's own level instead — the "drag between rows to move into/out of a
   * group" gesture. Rendered as two different visual cues (a filled pill
   * vs. a thin insertion line) so the two are distinguishable mid-drag. */
  dragOverZone?: 'child' | 'sibling'
  onClick?: (event: ReactMouseEvent) => void
  onRename?: (name: string) => void
  onClearMask?: () => void
  /** The "Add child" circle button next to an expandable (`Group`) row's
   * chevron — adds a default rectangle shape parented under this row.
   * `undefined` for the artboard's own pill, which renders the same
   * expand/collapse chevron but has no `onAddChild` (an artboard isn't a
   * `Node`, there's no id to parent a new shape under via this button —
   * the Shapes menu's root-level "Add Shape" already covers that case). */
  onAddChild?: () => void
  /** When set, this row is in "state machine layer mask" editing mode — a
   * checkbox renders before the row's own content, reflecting whether this
   * node is currently in the selected layer's `mask`. Only ever set on
   * node rows (never the artboard's own pill — an artboard isn't a `Node`,
   * nothing to mask). */
  layerMaskCheckbox?: {
    checked: boolean
    onChange: (checked: boolean) => void
  }
}) {
  const [editing, setEditing] = useState(false)
  const [text, setText] = useState(name)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!editing) setText(name)
  }, [name, editing])

  useEffect(() => {
    if (editing) inputRef.current?.select()
  }, [editing])

  const commit = () => {
    setEditing(false)
    const trimmed = text.trim()
    if (onRename && trimmed && trimmed !== name) onRename(trimmed)
    else setText(name)
  }

  return (
    <div className={`flex items-center gap-1.5 ${dragOverZone === 'sibling' ? 'border-t-2 border-accent pt-0.5' : 'border-t-2 border-transparent pt-0.5'}`}>
      {layerMaskCheckbox && (
        // `draggable={false}` overrides `LayerRow`'s draggable row
        // wrapper — without it, Chromium/WebView2 treats any mousedown
        // here (even a plain click has a pixel or two of jitter) as
        // starting the ancestor's native HTML5 drag instead of a normal
        // click, silently swallowing the checkbox's toggle.
        <input
          type="checkbox"
          draggable={false}
          title="In this layer's node mask"
          checked={layerMaskCheckbox.checked}
          onChange={(e) => layerMaskCheckbox.onChange(e.target.checked)}
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => e.stopPropagation()}
          className="shrink-0"
        />
      )}
      {editing ? (
        <input
          ref={inputRef}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === 'Enter') e.currentTarget.blur()
            if (e.key === 'Escape') {
              setText(name)
              setEditing(false)
            }
          }}
          className="flex-1 rounded-full bg-overlay/12 px-3 py-1.5 text-xs font-medium text-ink outline-none ring-1 ring-accent"
        />
      ) : (
        <button
          onClick={(e) => onClick?.(e)}
          onDoubleClick={() => onRename && setEditing(true)}
          className={`flex flex-1 items-center gap-1.5 truncate rounded-full px-3 py-1.5 text-left text-xs font-medium transition-colors ${
            dragOverZone === 'child'
              ? 'bg-accent/25 text-accent ring-1 ring-accent'
              : selected
                ? 'bg-accent/15 text-accent'
                : 'bg-overlay/8 text-ink hover:bg-overlay/12'
          }`}
        >
          {kind && <span className={`shrink-0 ${prefab ? 'text-accent' : 'text-ink-dim'}`}>{<KindIcon kind={kind} />}</span>}
          <span className={`truncate ${prefab === 'root' ? 'font-medium text-accent' : prefab === 'part' ? 'text-accent/80' : ''}`} title={prefab ? 'Prefab instance' : undefined}>
            {name}
            {sortingLayer && (
              <span
                className="ml-1 shrink-0 rounded bg-accent/15 px-1 text-[9px] font-medium text-accent"
                title={`Sorting layer “${sortingLayer}”: drawn with that layer, above or below other layers whatever its place in this list (and its children with it)`}
              >
                {sortingLayer}
              </span>
            )}
          </span>
          {maskBadge && (
            <span
              title={maskBadge === 'group' ? 'Masked by a child layer — click to clear' : 'Masks its parent group — click to clear'}
              onClick={(e) => {
                e.stopPropagation()
                onClearMask?.()
              }}
              className={`ml-auto shrink-0 ${maskBadge === 'group' ? 'text-accent' : 'text-ink-dim'}`}
            >
              <MaskIcon />
            </span>
          )}
          {clipsChildren && (
            <span
              title="Clips its own children — click to turn off"
              onClick={(e) => {
                e.stopPropagation()
                onToggleClipChildren?.()
              }}
              className={`${maskBadge ? '' : 'ml-auto'} shrink-0 text-accent`}
            >
              <MaskIcon />
            </span>
          )}
        </button>
      )}
      {expandable && !editing && (
        <>
          {hasChildren && (
            <CircleIconButton title={collapsed ? 'Expand' : 'Collapse'} onClick={onToggleCollapse}>
              <span
                className="inline-flex transition-transform"
                style={{
                  transform: collapsed ? 'rotate(-90deg)' : 'rotate(0deg)',
                }}
              >
                <ChevronDownIcon />
              </span>
            </CircleIconButton>
          )}
          <CircleIconButton title="Add child" onClick={onAddChild}>
            <PlusIcon />
          </CircleIconButton>
        </>
      )}
    </div>
  )
}

/** Depth-first index of every node in `nodes`, keyed by `idKey` — lets a
 * row look up its own parent's `NodeSummary` (for the mask-child badge) and
 * `computeDepth` walk the parent chain without a linear scan per node. */
function indexById(nodes: NodeSummary[]): Map<string, NodeSummary> {
  return new Map(nodes.map((n) => [idKey(n.id), n]))
}

/** How many direct children each node has, keyed by `idKey` — a group with
 * zero entries here has nothing to collapse, so its chevron doesn't render
 * (see `LayerPill`'s `hasChildren`). */
function countChildren(nodes: NodeSummary[]): Map<string, number> {
  const counts = new Map<string, number>()
  for (const node of nodes) {
    if (!node.parent) continue
    const key = idKey(node.parent)
    counts.set(key, (counts.get(key) ?? 0) + 1)
  }
  return counts
}

/** How many `parent` hops separate `node` from an artboard root — the
 * indentation level. `artboard.nodes` already arrives in depth-first,
 * parent-then-children order (`ordered_node_ids`, `dto.rs`), so no tree
 * needs to be rebuilt to render one — just render that order with each
 * row's own indent computed from its parent chain. */
function computeDepth(node: NodeSummary, nodesById: Map<string, NodeSummary>): number {
  let depth = 0
  let current = node
  while (current.parent) {
    const parent = nodesById.get(idKey(current.parent))
    if (!parent) break
    depth++
    current = parent
  }
  return depth
}

/** Re-derives display order from `artboard.nodes` (`ordered_node_ids`'s real
 * draw order — see `computeDepth`'s own doc comment) with each level's own
 * siblings *reversed*: draw order is "later entries paint on top" (`root_nodes`'s
 * own doc comment; matches every design tool's underlying model), but every
 * layers panel worth copying (Figma/Photoshop/Illustrator/Rive) *displays*
 * that inverted — the topmost row is the frontmost layer. Rendering
 * `artboard.nodes` top-to-bottom exactly as `ordered_node_ids` produced it
 * showed the *backmost* layer first and the frontmost one at the very
 * bottom, backwards from that convention (and confusing to drag-reorder
 * against, since "drag to the top" reads as "bring to front" everywhere
 * else). Still a real depth-first walk (not a blind whole-array reverse,
 * which would scatter children away from their parents) — just each
 * parent's own children in back-to-front becomes front-to-back. Tree
 * grouping/indentation (`computeDepth`) only ever depends on each node's own
 * parent chain, never on array position, so reversing sibling order here is
 * purely a presentation concern — nothing downstream needs to change to
 * still render a correct, correctly-indented tree from this list. */
function toDisplayOrder(nodes: NodeSummary[]): NodeSummary[] {
  const byParentKey = new Map<string, NodeSummary[]>()
  const roots: NodeSummary[] = []
  for (const node of nodes) {
    if (node.parent) {
      const key = idKey(node.parent)
      const siblings = byParentKey.get(key)
      if (siblings) siblings.push(node)
      else byParentKey.set(key, [node])
    } else {
      roots.push(node)
    }
  }
  const out: NodeSummary[] = []
  function visit(list: NodeSummary[]) {
    for (const node of [...list].reverse()) {
      out.push(node)
      visit(byParentKey.get(idKey(node.id)) ?? [])
    }
  }
  visit(roots)
  return out
}

/** `true` if any ancestor of `node` (not `node` itself) is in `collapsedKeys`
 * — collapsing a group hides its whole descendant subtree, not just its
 * immediate children, the standard file-tree convention. */
function isHiddenByCollapse(node: NodeSummary, nodesById: Map<string, NodeSummary>, collapsedKeys: Set<string>): boolean {
  let current = node.parent ? nodesById.get(idKey(node.parent)) : undefined
  while (current) {
    if (collapsedKeys.has(idKey(current.id))) return true
    current = current.parent ? nodesById.get(idKey(current.parent)) : undefined
  }
  return false
}

/**
 * One row in the layer list — a draggable, droppable `LayerPill` at its
 * computed tree depth, split into two vertical drop zones (see
 * `LayerPill`'s own `dragOverZone` doc comment):
 *
 * - **Lower half ("child")**: dropping here reparents the dragged node
 *   directly under this row — *any* row, not just a `Group` (`Node::clip_children`
 *   works on every node kind) — and, by default, no modifier needed, sets
 *   *this* row's own `clip_children` (`onSetClipChildren`), the "make it a
 *   direct child and clip to me" gesture that builds the same nested "room"
 *   effect the Parallax demo's own rig uses. No more wrapping the two nodes
 *   in a brand-new `Group` folder — that indirection (and the separate
 *   `GroupNode::mask` reference it needed) is exactly what `clip_children`
 *   replaced; a shape can be a real parent on its own now. Holding **Alt**
 *   still reparents without touching `clip_children` — the plain "just
 *   organize these, don't clip anything" action this always had.
 * - **Upper half ("sibling")**: dropping here moves the dragged node to
 *   become a sibling at *this row's own level* (`onReparentNodeBefore`
 *   targeting `node.parent`, not `node.id`) — dragging a deeply nested layer
 *   onto the upper half of a shallower row is how it moves *out* of its
 *   current group; dragging one onto the upper half of a row already inside
 *   some group moves it *into* that group as a plain (unmasked) sibling.
 *   Lands at a real, specific position too, not just "somewhere under the
 *   right parent": since this panel displays rows in reverse-of-draw-order
 *   (topmost row = frontmost layer — see `toDisplayOrder`'s own doc
 *   comment), dropping on a row's *upper* half reads as "make the dragged
 *   node more frontmost than this one," which `realNextSibling` resolves
 *   into the correct real-array insertion point.
 */
function LayerRow({
  artboardId,
  node,
  depth,
  nodesById,
  allNodes,
  childCounts,
  collapsedKeys,
  onToggleCollapsed,
  selectedNodeIds,
  draggedId,
  setDraggedId,
  onSelectNode,
  onRenameNode,
  onReparentNode,
  onReparentNodeBefore,
  onSetGroupMask,
  onSetClipChildren,
  onAddChildShape,
  layerMaskEditing,
}: {
  artboardId: OpaqueId
  node: NodeSummary
  depth: number
  nodesById: Map<string, NodeSummary>
  /** The artboard's whole node list in *real* (backend draw-order) order —
   * not `visibleNodes`' reversed-for-display order — needed to resolve the
   * "sibling" drop zone's real insertion point (`realNextSibling`). */
  allNodes: NodeSummary[]
  childCounts: Map<string, number>
  collapsedKeys: Set<string>
  onToggleCollapsed: (key: string) => void
  selectedNodeIds: OpaqueId[]
  draggedId: OpaqueId | null
  setDraggedId: (id: OpaqueId | null) => void
  onSelectNode: (artboardId: OpaqueId, nodeId: OpaqueId, event: ReactMouseEvent) => void
  onRenameNode: (artboardId: OpaqueId, nodeId: OpaqueId, name: string) => void
  onReparentNode: (artboardId: OpaqueId, nodeId: OpaqueId, parent: OpaqueId | null) => void
  /** The "sibling" drop zone's actual reorder — positions `nodeId` right
   * before `before` within `parent`'s children, instead of `onReparentNode`
   * alone always landing it at the very end (see `WasmEditor.reparent_node_before`'s
   * own doc comment for why that distinction is the whole fix). */
  onReparentNodeBefore: (artboardId: OpaqueId, nodeId: OpaqueId, parent: OpaqueId | null, before: OpaqueId | null) => void
  onSetGroupMask: (artboardId: OpaqueId, groupId: OpaqueId, mask: OpaqueId | null) => void
  /** The "child" drop zone's own toggle — see that zone's own doc comment
   * for why this replaced the old "wrap both in a new `Group` folder"
   * behavior: any node can clip its own children now (`Node::clip_children`),
   * so dropping onto one's lower half no longer needs a brand-new wrapper
   * folder just to get the same clipped-child effect. */
  onSetClipChildren: (artboardId: OpaqueId, nodeId: OpaqueId, value: boolean) => void
  onAddChildShape: (artboardId: OpaqueId, parent: OpaqueId) => void
  /** State-machine layer mask editing — see `LayersPanel`'s own doc
   * comment for how this gets here and `LayerPill`'s `layerMaskCheckbox`
   * for what it renders. `undefined` outside the State Machine tab. */
  layerMaskEditing?: {
    mask: OpaqueId[]
    onToggleNode: (nodeId: OpaqueId) => void
  }
}) {
  const [dragOverZone, setDragOverZone] = useState<'child' | 'sibling' | null>(null)
  const isGroup = node.kind === 'Group'
  const key = idKey(node.id)
  const hasChildren = (childCounts.get(key) ?? 0) > 0
  const collapsed = collapsedKeys.has(key)
  const parentNode = node.parent ? nodesById.get(idKey(node.parent)) : null
  const isMaskChild = !!parentNode && parentNode.kind === 'Group' && !!parentNode.groupMask && idsEqual(parentNode.groupMask, node.id)
  const maskBadge: 'group' | 'child' | undefined = isGroup && node.groupMask ? 'group' : isMaskChild ? 'child' : undefined

  const canDrop = draggedId !== null && !idsEqual(draggedId, node.id)

  return (
    <div style={{ marginLeft: depth * 14 }}>
      <div
        draggable
        onDragStart={(e) => {
          e.dataTransfer.setData(DRAG_MIME, idKey(node.id))
          setDraggedId(node.id)
        }}
        onDragEnd={() => {
          setDraggedId(null)
          setDragOverZone(null)
        }}
        onDragOver={(e) => {
          if (!canDrop) return
          e.preventDefault()
          const rect = e.currentTarget.getBoundingClientRect()
          setDragOverZone(e.clientY - rect.top < rect.height / 2 ? 'sibling' : 'child')
        }}
        onDragLeave={() => setDragOverZone(null)}
        onDrop={(e) => {
          e.preventDefault()
          const zone = dragOverZone
          setDragOverZone(null)
          if (!draggedId || e.dataTransfer.getData(DRAG_MIME) !== idKey(draggedId) || idsEqual(draggedId, node.id)) return
          if (zone === 'sibling') {
            onReparentNodeBefore(artboardId, draggedId, node.parent, realNextSibling(node, allNodes))
          } else {
            onReparentNode(artboardId, draggedId, node.id)
            if (!e.altKey) onSetClipChildren(artboardId, node.id, true)
          }
          setDraggedId(null)
        }}
      >
        <LayerPill
          name={node.name}
          kind={iconKind(node)}
          // Not just `isGroup` — `Node::clip_children` (any kind, not just
          // `Group`) means a plain `Shape` can be a real parent now too
          // (the parallax demo's own rig is a chain of `Shape`s), and a
          // node with actual children still needs its collapse/expand
          // chevron regardless of what kind it is. `isGroup` alone stays
          // part of this so an empty `Group` (no children yet) still shows
          // its "Add child" control — `hasChildren` alone wouldn't cover
          // that still-childless case.
          expandable={isGroup || hasChildren}
          hasChildren={hasChildren}
          collapsed={collapsed}
          onToggleCollapse={() => onToggleCollapsed(key)}
          selected={selectedNodeIds.some((id) => idsEqual(id, node.id))}
          maskBadge={maskBadge}
          clipsChildren={node.clipChildren}
          onToggleClipChildren={() => onSetClipChildren(artboardId, node.id, false)}
          dragOverZone={dragOverZone ?? undefined}
          onClick={(e) => onSelectNode(artboardId, node.id, e)}
          onRename={(name) => onRenameNode(artboardId, node.id, name)}
          onClearMask={isGroup ? () => onSetGroupMask(artboardId, node.id, null) : () => parentNode && onSetGroupMask(artboardId, parentNode.id, null)}
          onAddChild={isGroup ? () => onAddChildShape(artboardId, node.id) : undefined}
          prefab={node.prefab ? (node.prefab.isRoot ? 'root' : 'part') : undefined}
          sortingLayer={(() => {
            const own = node.behaviors.find((b) => b.enabled && b.behavior.kind === 'SortingLayer2D')?.behavior
            if (!own || own.kind !== 'SortingLayer2D') return undefined
            return sortingLayerNames[own.layer] ?? `Layer ${own.layer}`
          })()}
          layerMaskCheckbox={
            layerMaskEditing
              ? {
                  checked: layerMaskEditing.mask.some((id) => idsEqual(id, node.id)),
                  onChange: () => layerMaskEditing.onToggleNode(node.id),
                }
              : undefined
          }
        />
      </div>
    </div>
  )
}

/** Lists shorter than this render every row (exactly as before); longer
 * ones only mount the rows actually on screen — see `WindowedRows`. */
const WINDOWING_THRESHOLD = 150
/** `gap-1.5` between rows, in px — part of each row's stride. */
const ROW_GAP = 6
/** Extra rows mounted above/below the visible window so fast scrolling
 * doesn't flash empty space before the next scroll event lands. */
const OVERSCAN = 12

/** Nearest ancestor that actually scrolls vertically (the dock panel's
 * body), or `null` if none — then the window is simply everything. */
function scrollParentOf(el: HTMLElement): HTMLElement | null {
  for (let cur = el.parentElement; cur; cur = cur.parentElement) {
    const overflowY = getComputedStyle(cur).overflowY
    if (overflowY === 'auto' || overflowY === 'scroll') return cur
  }
  return null
}

/**
 * Virtualized row list. A scene with thousands of nodes used to mount
 * thousands of `LayerRow`s — every document refresh or selection click then
 * re-rendered all of them, which is what made the editor UI stall on big
 * scenes even though the canvas itself was fine. This mounts only the rows
 * inside the scroll viewport (plus `OVERSCAN`), positioned inside a spacer
 * of the full list's height so the scrollbar still reads correctly.
 *
 * Rows are uniform height (one `LayerPill` each); the stride is measured
 * from the first mounted row rather than hard-coded so theme/density
 * changes can't desync it. Short lists skip all of this and render
 * normally (`WINDOWING_THRESHOLD`).
 */
function WindowedRows<T>({ items, renderItem, getKey }: { items: T[]; renderItem: (item: T) => ReactNode; getKey: (item: T) => string }) {
  const containerRef = useRef<HTMLDivElement>(null)
  const firstRowRef = useRef<HTMLDivElement>(null)
  const [stride, setStride] = useState(32 + ROW_GAP)
  const [range, setRange] = useState({ start: 0, end: 60 })
  const windowed = items.length > WINDOWING_THRESHOLD

  useEffect(() => {
    if (!windowed) return
    const container = containerRef.current
    if (!container) return
    const scroller = scrollParentOf(container)
    const update = () => {
      const viewHeight = scroller ? scroller.clientHeight : window.innerHeight
      const top = scroller ? container.getBoundingClientRect().top - scroller.getBoundingClientRect().top : container.getBoundingClientRect().top
      const start = Math.max(0, Math.floor(-top / stride) - OVERSCAN)
      const end = Math.min(items.length, Math.ceil((viewHeight - top) / stride) + OVERSCAN)
      setRange((prev) => (prev.start === start && prev.end === end ? prev : { start, end }))
    }
    update()
    const target: HTMLElement | Window = scroller ?? window
    target.addEventListener('scroll', update, { passive: true })
    const observer = new ResizeObserver(update)
    if (scroller) observer.observe(scroller)
    return () => {
      target.removeEventListener('scroll', update)
      observer.disconnect()
    }
  }, [windowed, stride, items.length])

  // Measure the real row height once rows exist.
  useLayoutEffect(() => {
    const row = firstRowRef.current
    if (!windowed || !row) return
    const measured = row.offsetHeight + ROW_GAP
    if (measured > ROW_GAP && Math.abs(measured - stride) > 0.5) setStride(measured)
  })

  if (!windowed)
    return (
      <>
        {items.map((item) => (
          <div key={getKey(item)}>{renderItem(item)}</div>
        ))}
      </>
    )

  const start = Math.min(range.start, items.length)
  const end = Math.min(Math.max(range.end, start), items.length)
  return (
    <div ref={containerRef} style={{ position: 'relative', height: items.length * stride - ROW_GAP }}>
      <div className="flex flex-col gap-1.5" style={{ position: 'absolute', left: 0, right: 0, top: start * stride }}>
        {items.slice(start, end).map((item, i) => (
          <div key={getKey(item)} ref={i === 0 ? firstRowRef : undefined}>
            {renderItem(item)}
          </div>
        ))}
      </div>
    </div>
  )
}

/**
 * The layer/node hierarchy tree (AN1MBYT3_BUILD_PLAN.md §15). Renders the
 * real document fetched from the wasm editor (`App.tsx`'s `doc`) — each
 * artboard is the outer pill (there's no `Node` behind an artboard itself,
 * it's `an1mbyt3_core::Artboard`, a different type — see `ipc/document.ts`),
 * its nodes render beneath it as a real indented tree (`LayerRow`'s
 * `computeDepth`) rather than a flat list — group hierarchy (from the
 * document itself, or preserved from an SVG's own `<g>` nesting on import)
 * is now visible, not flattened away. Dropping a row directly onto the
 * artboard's own pill reparents it to the artboard's root (`parent: null`)
 * — the "drag it back out of a group" counterpart to `LayerRow`'s
 * drag-into-a-row reparent action.
 */
export function LayersPanel({
  artboards,
  selectedNodeIds,
  onSelectNode,
  onRenameNode,
  onReparentNode,
  onReparentNodeBefore,
  onSetGroupMask,
  onSetClipChildren,
  onGroupSelection,
  onAddChildShape,
  layerMaskEditing,
  sortingLayers = [],
}: {
  /** The project's sorting layers (`[{ id, name }]`) — rows on a layer
   * other than Normal show its name. */
  sortingLayers?: { id: number; name: string }[]
  artboards: ArtboardSummary[]
  /** Every currently-selected node id (multi-select — Ctrl/Cmd/Shift-click
   * adds/removes from this set) — a plain `[nodeId]` for the common
   * single-selection case. */
  selectedNodeIds: OpaqueId[]
  /** `event` carries the click's modifier keys — `App.tsx` toggles
   * multi-selection membership when Ctrl/Cmd/Shift is held, replaces the
   * whole selection with just `nodeId` otherwise. */
  onSelectNode: (artboardId: OpaqueId, nodeId: OpaqueId, event: ReactMouseEvent) => void
  onRenameNode: (artboardId: OpaqueId, nodeId: OpaqueId, name: string) => void
  onReparentNode: (artboardId: OpaqueId, nodeId: OpaqueId, parent: OpaqueId | null) => void
  onReparentNodeBefore: (artboardId: OpaqueId, nodeId: OpaqueId, parent: OpaqueId | null, before: OpaqueId | null) => void
  onSetGroupMask: (artboardId: OpaqueId, groupId: OpaqueId, mask: OpaqueId | null) => void
  onSetClipChildren: (artboardId: OpaqueId, nodeId: OpaqueId, value: boolean) => void
  onGroupSelection: () => void
  onAddChildShape: (artboardId: OpaqueId, parent: OpaqueId) => void
  /** Set only while the State Machine tab has a non-base layer selected —
   * every node row then shows a checkbox reflecting/toggling membership in
   * that layer's `mask` (`StateMachineLayerDef::mask`). `App.tsx` owns
   * which layer is selected and the actual `set_state_machine_layer_mask`
   * call; this panel only ever renders/toggles checkboxes. */
  layerMaskEditing?: {
    mask: OpaqueId[]
    onToggleNode: (nodeId: OpaqueId) => void
  }
}) {
  sortingLayerNames = Object.fromEntries(sortingLayers.filter((l) => l.id !== 0).map((l) => [l.id, l.name]))
  const [draggedId, setDraggedId] = useState<OpaqueId | null>(null)
  // Collapsed group ids, keyed by `idKey` — an artboard's own pseudo-key
  // (`artboard:<idKey>`) lives in the same set, collapsing its whole node
  // list at once. Starts empty (everything expanded) so a document opens
  // looking exactly as it did before this feature existed.
  const [collapsedKeys, setCollapsedKeys] = useState<Set<string>>(new Set())

  const toggleCollapsed = (key: string) => {
    setCollapsedKeys((keys) => {
      const next = new Set(keys)
      if (next.has(key)) next.delete(key)
      else next.add(key)
      return next
    })
  }

  // `indexById`/`countChildren` are both O(n) `Map` builds, and
  // `isHiddenByCollapse` (inside `visibleNodes`) is O(n·depth) worst case —
  // none of them depend on `selectedNodeIds`, but this component re-renders
  // on every selection-change click (a new `multiSelectedIds` array from
  // `App.tsx` every time), which used to rebuild all of this from scratch on
  // every single click even though the tree's own shape hadn't changed.
  // Memoized on `[artboards, collapsedKeys]` instead — recomputed only when
  // the document actually changes or a group's expand/collapse state
  // toggles, never on a pure selection change.
  const perArtboard = useMemo(
    () =>
      artboards.map((artboard) => {
        const nodesById = indexById(artboard.nodes)
        const artboardKey = `artboard:${artboard.id.idx}-${artboard.id.version}`
        const artboardCollapsed = collapsedKeys.has(artboardKey)
        return {
          artboard,
          nodesById,
          childCounts: countChildren(artboard.nodes),
          artboardKey,
          artboardCollapsed,
          visibleNodes: artboardCollapsed ? [] : toDisplayOrder(artboard.nodes).filter((node) => !isHiddenByCollapse(node, nodesById, collapsedKeys)),
        }
      }),
    [artboards, collapsedKeys],
  )

  if (artboards.length === 0) {
    return <p className="text-xs text-ink-dim">No document loaded.</p>
  }

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center justify-between gap-2 px-0.5">
        <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">Layers</span>
        <button
          onClick={onGroupSelection}
          disabled={selectedNodeIds.length < 2}
          title="Group selection"
          className="flex h-6 items-center gap-1 rounded-md bg-overlay/8 px-2 text-[11px] font-medium text-ink-dim hover:bg-accent/15 hover:text-accent disabled:opacity-35"
        >
          <GroupKindIcon />
          Group
        </button>
      </div>
      {perArtboard.map(({ artboard, nodesById, childCounts, artboardKey, artboardCollapsed, visibleNodes }) => {
        return (
          <div key={`${artboard.id.idx}-${artboard.id.version}`}>
            <div
              onDragOver={(e) => draggedId && e.preventDefault()}
              onDrop={(e) => {
                e.preventDefault()
                if (draggedId && e.dataTransfer.getData(DRAG_MIME) === idKey(draggedId)) onReparentNode(artboard.id, draggedId, null)
                setDraggedId(null)
              }}
            >
              <LayerPill
                name={artboard.name}
                expandable
                hasChildren={artboard.nodes.length > 0}
                collapsed={artboardCollapsed}
                onToggleCollapse={() => toggleCollapsed(artboardKey)}
              />
            </div>
            <div className="ml-2.5 mt-1.5 flex flex-col gap-1.5 rounded-lg border border-panel-border p-1.5">
              <WindowedRows
                items={visibleNodes}
                getKey={(node) => `${node.id.idx}-${node.id.version}`}
                renderItem={(node) => (
                  <LayerRow
                    artboardId={artboard.id}
                    node={node}
                    depth={computeDepth(node, nodesById)}
                    nodesById={nodesById}
                    allNodes={artboard.nodes}
                    childCounts={childCounts}
                    collapsedKeys={collapsedKeys}
                    onToggleCollapsed={toggleCollapsed}
                    selectedNodeIds={selectedNodeIds}
                    draggedId={draggedId}
                    setDraggedId={setDraggedId}
                    onSelectNode={onSelectNode}
                    onRenameNode={onRenameNode}
                    onReparentNode={onReparentNode}
                    onReparentNodeBefore={onReparentNodeBefore}
                    onSetGroupMask={onSetGroupMask}
                    onSetClipChildren={onSetClipChildren}
                    onAddChildShape={onAddChildShape}
                    layerMaskEditing={layerMaskEditing}
                  />
                )}
              />
            </div>
          </div>
        )
      })}
    </div>
  )
}
