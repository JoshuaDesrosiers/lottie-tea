import { useEffect, useRef, useState } from 'react'
import { loadPersisted, savePersisted } from './persisted'
import type { RefObject } from 'react'
import { MAX_PANEL_WIDTH, MIN_PANEL_WIDTH, RAIL_WIDTH, SNAP_THRESHOLD_PX } from '../layoutConstants'
import type { Position } from './position'

export type DockRegion = 'left' | 'right' | null
export type PanelKey = 'layers' | 'inspector' | 'resources' | 'tilePalette' | 'components' | 'effects'
type FixedRegion = 'left' | 'right'

/** Every dockable panel key, in a fixed order used anywhere this hook needs
 * to build a `Record<PanelKey, _>` from scratch — a plain array (not derived
 * from `config`'s own keys) since `config`'s type already pins its keys to
 * exactly this set; this is just the one place that set is spelled out. */
const PANEL_KEYS: readonly PanelKey[] = ['layers', 'inspector', 'resources', 'tilePalette', 'components', 'effects']
const REGIONS: readonly FixedRegion[] = ['left', 'right']

/** The left edge's snap zone starts after `IconRail`, not at the raw screen
 * edge — same reasoning `useDockablePanel` (this hook's single-panel
 * predecessor) already established. */
const LEFT_SNAP_ZONE_PX = RAIL_WIDTH + SNAP_THRESHOLD_PX
/** How close to a target's own top edge a drop has to land to count as
 * "onto its title/tab strip" (join as a tab) rather than "onto its body"
 * (split into a new pane) — doesn't need to match the rendered title row
 * pixel-for-pixel, just close enough that dropping near the top reads as
 * "onto the title". Reused at every nesting level (solo panel, a whole
 * group's tab strip, one pane inside a split) so the gesture reads the same
 * everywhere. */
const TITLE_BAR_HIT_HEIGHT_PX = 32
/** How far a grouped panel's own header has to be dragged from its starting
 * point before it detaches back into a floating panel — a plain click (or a
 * tiny jitter) on a tab must not immediately detach it. */
const UNGROUP_DRAG_THRESHOLD_PX = 40
/** Split panes never shrink past this fraction of the group's height — keeps
 * a pane from being dragged to nothing instead of just detached. */
const MIN_PANE_RATIO = 0.12

interface PanelDockState {
  region: DockRegion
  position: Position
  width: number
}

/** One pane of a `split` group — either a single panel, or (the "hybrid"
 * case) a nested tab strip of 2+ panels sharing that one pane's space. A
 * `split` pane is never itself another `split` — with only a handful of
 * panels ever in play, a second level of split nesting has no real layout
 * it could express that this one level plus tabs doesn't already cover, so
 * the type stops here deliberately rather than modeling unbounded recursion
 * nothing needs. */
export type DockPane = { kind: 'leaf'; key: PanelKey } | { kind: 'tabs'; members: PanelKey[]; activeTab: PanelKey }

/** One region's (`left`/`right`) combined panels, once 2+ of them have been
 * dragged together — either every member sharing one tab strip (only
 * `activeTab`'s content shows), or a resizable vertical stack of panes where
 * each pane is a single panel or its own nested tab strip (`DockPane`'s own
 * doc comment — this is the "hybrid" shape). Any number of panels can join a
 * `tabs` group or a `split`'s pane list. */
export type GroupState =
  | { mode: 'tabs'; region: FixedRegion; members: PanelKey[]; activeTab: PanelKey; width: number }
  | { mode: 'split'; region: FixedRegion; panes: DockPane[]; ratios: number[]; width: number }

/** A live, non-committing preview of where a drag-in-progress would land if
 * released right now — `App.tsx` renders this as a highlighted rectangle so
 * "tab" vs "split" (and, in split mode, *which* pane) reads clearly before
 * the drop, instead of only finding out after releasing. */
export interface DropPreview {
  mode: 'tabs' | 'split'
  rect: { left: number; top: number; width: number; height: number }
}

interface PanelConfig {
  initialPosition: Position
  initialRegion?: DockRegion
  defaultWidth: number
}

/** Every `PanelKey` referenced anywhere inside `group`, regardless of how
 * deeply it's nested (a top-level tabs member, a split's leaf pane, or a
 * member of a tabs pane inside a split) — what `App.tsx` needs to know which
 * panels to skip rendering solo. */
export function groupMemberKeys(group: GroupState): PanelKey[] {
  if (group.mode === 'tabs') return group.members
  return group.panes.flatMap((pane) => (pane.kind === 'leaf' ? [pane.key] : pane.members))
}

function removeFromGroup(group: GroupState, key: PanelKey): { group: GroupState | null; promotedKey?: PanelKey } {
  if (group.mode === 'tabs') {
    const members = group.members.filter((m) => m !== key)
    if (members.length <= 1) return { group: null, promotedKey: members[0] }
    const activeTab = group.activeTab === key ? members[0] : group.activeTab
    return { group: { ...group, members, activeTab } }
  }

  const paneIndex = group.panes.findIndex((pane) => (pane.kind === 'leaf' ? pane.key === key : pane.members.includes(key)))
  if (paneIndex === -1) return { group }
  const pane = group.panes[paneIndex]

  let panes: DockPane[]
  let ratios: number[]
  if (pane.kind === 'tabs') {
    const members = pane.members.filter((m) => m !== key)
    const collapsed: DockPane = members.length <= 1 ? { kind: 'leaf', key: members[0] } : { kind: 'tabs', members, activeTab: pane.activeTab === key ? members[0] : pane.activeTab }
    panes = group.panes.map((p, i) => (i === paneIndex ? collapsed : p))
    ratios = group.ratios
  } else {
    panes = group.panes.filter((_, i) => i !== paneIndex)
    const removedRatio = group.ratios[paneIndex]
    const rest = group.ratios.filter((_, i) => i !== paneIndex)
    const restTotal = rest.reduce((a, b) => a + b, 0) || 1
    ratios = rest.map((r) => r + (removedRatio * r) / restTotal)
  }

  if (panes.length === 1) {
    const only = panes[0]
    if (only.kind === 'leaf') return { group: null, promotedKey: only.key }
    return { group: { mode: 'tabs', region: group.region, members: only.members, activeTab: only.activeTab, width: group.width } }
  }
  return { group: { ...group, panes, ratios } }
}

/**
 * Owns dock layout for `Layers`/`Inspector`/`Resources`/`TilePalette`/
 * `Components`/`Effects` — any of these panels can combine into either
 * screen region as a tab strip (any number of members) or a resizable
 * vertical split (any number of panes, each either a single panel or its
 * own nested tab strip), per AN1MBYT3_BUILD_PLAN.md's "drag a sidebar onto
 * another's title = tab, onto its body = split" request. `left` and `right`
 * each own an independent `GroupState | null` — one group *per region*, not
 * one global group — so e.g. Layers+TilePalette can tab together on the
 * left while Inspector+Resources split on the right.
 *
 * This has to live above every panel (not inside each one's own hook
 * instance) because grouping is inherently a relationship between panels —
 * deciding "did panel A get dropped onto panel B, or onto group G's tab
 * strip, or onto one specific pane of group G's split" needs live access to
 * every candidate's rect at once.
 *
 * While a panel's key is referenced by a region's `group` (see
 * `groupMemberKeys`), it doesn't render its own independent `DockablePanel`
 * — `App.tsx` renders one `DockGroup` per non-null region group instead, and
 * each grouped panel's own `region`/`position` in `panels` just sits unused
 * until it's dragged back out (see `groupedHeaderDragHandlers`), at which
 * point it resumes floating at wherever the drag ended and the group
 * collapses/shrinks around the gap (`removeFromGroup`) — and, critically,
 * *keeps following the same physical drag* (`continueAsFloatingDrag`)
 * instead of leaving the user needing a second click-and-drag to actually
 * place it, which detaching used to require.
 */
export function useDockLayout(
  config: Record<PanelKey, PanelConfig>,
  /** Fires `true` when a docked panel's/group's edge-resize drag starts and
   * `false` when it ends — `App.tsx` freezes the canvas for the duration
   * (in Solid style the canvas viewport is carved out between the docks, so
   * every resize step would otherwise reallocate the canvas buffer and
   * reconfigure the GPU surface, flashing every frame). */
  onResizeActiveChange?: (active: boolean) => void,
) {
  const onResizeActiveChangeRef = useRef(onResizeActiveChange)
  onResizeActiveChangeRef.current = onResizeActiveChange
  // Restored from the last session (if any): every panel's region,
  // floating position and width, plus each region's tab/split group.
  // Saved positions are clamped back on-screen in case the window shrank.
  const [panels, setPanels] = useState<Record<PanelKey, PanelDockState>>(() => {
    const defaults = Object.fromEntries(
      PANEL_KEYS.map((key) => [key, { region: config[key].initialRegion ?? null, position: config[key].initialPosition, width: config[key].defaultWidth }]),
    ) as Record<PanelKey, PanelDockState>
    const saved = loadPersisted<Record<string, PanelDockState> | null>('dock.panels', null, (v): v is Record<string, PanelDockState> => typeof v === 'object' && v !== null)
    if (!saved) return defaults
    for (const key of PANEL_KEYS) {
      const s = saved[key]
      if (!s || typeof s.width !== 'number' || !s.position) continue
      defaults[key] = {
        region: s.region === 'left' || s.region === 'right' ? s.region : null,
        position: { x: Math.min(Math.max(0, s.position.x), window.innerWidth - 80), y: Math.min(Math.max(0, s.position.y), window.innerHeight - 60) },
        width: s.width,
      }
    }
    return defaults
  })
  const [groups, setGroups] = useState<Record<FixedRegion, GroupState | null>>(() => {
    const saved = loadPersisted<Record<FixedRegion, GroupState | null> | null>('dock.groups', null, (v): v is Record<FixedRegion, GroupState | null> => typeof v === 'object' && v !== null)
    if (!saved) return { left: null, right: null }
    // A group may only reference panels that still exist.
    const known = new Set<string>(PANEL_KEYS)
    const ok = (g: GroupState | null | undefined) => !!g && groupMemberKeys(g).every((k) => known.has(k))
    return { left: ok(saved.left) ? saved.left : null, right: ok(saved.right) ? saved.right : null }
  })
  useEffect(() => {
    savePersisted('dock.panels', panels)
  }, [panels])
  useEffect(() => {
    savePersisted('dock.groups', groups)
  }, [groups])
  const [dropPreview, setDropPreview] = useState<DropPreview | null>(null)

  // Read inside pointer-event closures below instead of `panels`/`groups`
  // directly — same "mount-once listener, always-current value" reasoning
  // `App.tsx`'s `cameraRef`/`canvasSizeRef` already established, needed here
  // because `setPointerCapture` (and the window-level listeners
  // `continueAsFloatingDrag` installs) keep firing for a whole drag without
  // React ever re-attaching them.
  const panelsRef = useRef(panels)
  panelsRef.current = panels
  const groupsRef = useRef(groups)
  groupsRef.current = groups

  // Hooks can't be created in a loop, so these stay explicit `useRef` calls
  // even though `PANEL_KEYS` could otherwise drive this too.
  const panelRefs: Record<PanelKey, RefObject<HTMLDivElement | null>> = {
    layers: useRef<HTMLDivElement | null>(null),
    inspector: useRef<HTMLDivElement | null>(null),
    resources: useRef<HTMLDivElement | null>(null),
    tilePalette: useRef<HTMLDivElement | null>(null),
    components: useRef<HTMLDivElement | null>(null),
    effects: useRef<HTMLDivElement | null>(null),
  }
  const groupContainerRefs: Record<FixedRegion, RefObject<HTMLDivElement | null>> = {
    left: useRef<HTMLDivElement | null>(null),
    right: useRef<HTMLDivElement | null>(null),
  }
  // One DOM rect per rendered pane, per region — only meaningful while that
  // region's group is in `split` mode; re-populated by `DockGroup`'s own
  // pane refs every render, read only at drop time, so churn in array
  // length between renders (panes added/removed) is fine.
  const groupPaneRefs: Record<FixedRegion, RefObject<(HTMLDivElement | null)[]>> = {
    left: useRef<(HTMLDivElement | null)[]>([]),
    right: useRef<(HTMLDivElement | null)[]>([]),
  }

  /** Every other panel key — the solo-vs-solo drop candidates a dragged
   * panel's own key is never compared against itself. */
  const otherKeys = (key: PanelKey): PanelKey[] => PANEL_KEYS.filter((k) => k !== key)

  /** Finds where `key` (currently being dragged, not itself part of any
   * group) would land if dropped at `(x, y)` right now — shared by the live
   * drop-preview (`onPointerMove`) and the actual commit (`onPointerUp`), so
   * the preview can never drift from what releasing would actually do. */
  function findDropTarget(key: PanelKey, x: number, y: number): DropPreview & { commit: () => void } | null {
    for (const candidateKey of otherKeys(key)) {
      const candidate = panelRefs[candidateKey].current
      const candidateRegion = panelsRef.current[candidateKey].region
      if (!candidate || !candidateRegion) continue
      if (groupsRef.current[candidateRegion] && groupMemberKeys(groupsRef.current[candidateRegion]!).includes(candidateKey)) continue

      const rect = candidate.getBoundingClientRect()
      if (x < rect.left || x > rect.right || y < rect.top || y > rect.bottom) continue

      const mode: 'tabs' | 'split' = y - rect.top <= TITLE_BAR_HIT_HEIGHT_PX ? 'tabs' : 'split'
      return {
        mode,
        rect: mode === 'tabs' ? { left: rect.left, top: rect.top, width: rect.width, height: TITLE_BAR_HIT_HEIGHT_PX } : { left: rect.left, top: rect.top, width: rect.width, height: rect.height },
        commit: () =>
          setGroups((prev) => ({
            ...prev,
            [candidateRegion]: mode === 'tabs'
              ? { mode: 'tabs', region: candidateRegion, members: [candidateKey, key], activeTab: key, width: panelsRef.current[candidateKey].width }
              : { mode: 'split', region: candidateRegion, panes: [{ kind: 'leaf', key: candidateKey }, { kind: 'leaf', key }], ratios: [0.5, 0.5], width: panelsRef.current[candidateKey].width },
          })),
      }
    }

    for (const region of REGIONS) {
      const group = groupsRef.current[region]
      const container = groupContainerRefs[region].current
      if (!group || !container) continue
      const rect = container.getBoundingClientRect()
      if (x < rect.left || x > rect.right || y < rect.top || y > rect.bottom) continue

      if (group.mode === 'tabs') {
        if (y - rect.top <= TITLE_BAR_HIT_HEIGHT_PX) {
          return {
            mode: 'tabs',
            rect: { left: rect.left, top: rect.top, width: rect.width, height: TITLE_BAR_HIT_HEIGHT_PX },
            commit: () => setGroups((prev) => ({ ...prev, [region]: { ...group, members: [...group.members, key], activeTab: key } })),
          }
        }
        return {
          mode: 'split',
          rect: { left: rect.left, top: rect.top, width: rect.width, height: rect.height },
          commit: () =>
            setGroups((prev) => ({
              ...prev,
              [region]: { mode: 'split', region, panes: [{ kind: 'tabs', members: group.members, activeTab: group.activeTab }, { kind: 'leaf', key }], ratios: [0.5, 0.5], width: group.width },
            })),
        }
      }

      const paneRects = groupPaneRefs[region].current.map((el) => el?.getBoundingClientRect() ?? null)
      const paneIndex = paneRects.findIndex((r) => r && y >= r.top && y <= r.bottom)
      if (paneIndex === -1) continue
      const paneRect = paneRects[paneIndex]!
      const pane = group.panes[paneIndex]

      if (y - paneRect.top <= TITLE_BAR_HIT_HEIGHT_PX) {
        return {
          mode: 'tabs',
          rect: { left: paneRect.left, top: paneRect.top, width: paneRect.width, height: TITLE_BAR_HIT_HEIGHT_PX },
          commit: () => {
            const merged: DockPane = pane.kind === 'leaf' ? { kind: 'tabs', members: [pane.key, key], activeTab: key } : { kind: 'tabs', members: [...pane.members, key], activeTab: key }
            setGroups((prev) => ({ ...prev, [region]: { ...group, panes: group.panes.map((p, i) => (i === paneIndex ? merged : p)) } }))
          },
        }
      }
      return {
        mode: 'split',
        rect: { left: paneRect.left, top: paneRect.top, width: paneRect.width, height: paneRect.height },
        commit: () => {
          const splitRatio = group.ratios[paneIndex] / 2
          const panes = [...group.panes]
          const ratios = [...group.ratios]
          panes.splice(paneIndex + 1, 0, { kind: 'leaf', key })
          ratios.splice(paneIndex, 1, splitRatio, splitRatio)
          setGroups((prev) => ({ ...prev, [region]: { ...group, panes, ratios } }))
        },
      }
    }
    return null
  }

  // ---- ungrouped title-bar drag: move/dock/float, plus drop-onto-another-
  // panel or drop-onto-an-existing-group checks ----
  const dragOrigin = useRef<Record<PanelKey, { pointerX: number; pointerY: number; startX: number; startY: number } | null>>({
    layers: null,
    inspector: null,
    resources: null,
    tilePalette: null,
    components: null,
    effects: null,
  })

  /** The floating-drag move/drop logic, factored out so a panel that just
   * detached from a group (`continueAsFloatingDrag`) can drive the exact
   * same behavior for the rest of its gesture as a panel that started the
   * gesture floating in the first place (`makeDragHandlers`). */
  function floatingPointerMove(key: PanelKey, clientX: number, clientY: number) {
    const origin = dragOrigin.current[key]
    if (!origin) return
    const nextRegion: DockRegion = clientX < LEFT_SNAP_ZONE_PX ? 'left' : clientX > window.innerWidth - SNAP_THRESHOLD_PX ? 'right' : null
    setPanels((prev) => ({
      ...prev,
      [key]: {
        ...prev[key],
        region: nextRegion,
        position: nextRegion ? prev[key].position : { x: origin.startX + (clientX - origin.pointerX), y: origin.startY + (clientY - origin.pointerY) },
      },
    }))
    setDropPreview(nextRegion ? null : findDropTarget(key, clientX, clientY))
  }

  function floatingPointerUp(key: PanelKey, clientX: number, clientY: number) {
    dragOrigin.current[key] = null
    setDropPreview(null)
    findDropTarget(key, clientX, clientY)?.commit()
  }

  function makeDragHandlers(key: PanelKey) {
    return {
      onPointerDown: (event: React.PointerEvent) => {
        const rect = panelRefs[key].current?.getBoundingClientRect()
        dragOrigin.current[key] = {
          pointerX: event.clientX,
          pointerY: event.clientY,
          startX: rect?.left ?? panelsRef.current[key].position.x,
          startY: rect?.top ?? panelsRef.current[key].position.y,
        }
        event.currentTarget.setPointerCapture(event.pointerId)
      },
      onPointerMove: (event: React.PointerEvent) => floatingPointerMove(key, event.clientX, event.clientY),
      onPointerUp: (event: React.PointerEvent) => floatingPointerUp(key, event.clientX, event.clientY),
    }
  }

  /** Keeps a just-detached panel following the same physical drag instead of
   * requiring a second click-and-drag to actually place it — installs
   * window-level listeners (not relying on the original tab button's own
   * pointer capture, which is lost the instant that button unmounts, which
   * happens on this very state update) that drive the exact same
   * move/drop logic a normal floating drag uses, for the rest of this one
   * gesture. */
  function continueAsFloatingDrag(key: PanelKey, clientX: number, clientY: number, startX: number, startY: number) {
    dragOrigin.current[key] = { pointerX: clientX, pointerY: clientY, startX, startY }
    const onMove = (e: PointerEvent) => floatingPointerMove(key, e.clientX, e.clientY)
    const onUp = (e: PointerEvent) => {
      floatingPointerUp(key, e.clientX, e.clientY)
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
    }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
  }

  // ---- per-panel canvas-facing-edge resize (solo panels only) ----
  const resizeOrigin = useRef<Record<PanelKey, { pointerX: number; startWidth: number } | null>>({
    layers: null,
    inspector: null,
    resources: null,
    tilePalette: null,
    components: null,
    effects: null,
  })

  function makeResizeHandlers(key: PanelKey) {
    return {
      onPointerDown: (event: React.PointerEvent) => {
        resizeOrigin.current[key] = { pointerX: event.clientX, startWidth: panelsRef.current[key].width }
        onResizeActiveChangeRef.current?.(true)
        event.currentTarget.setPointerCapture(event.pointerId)
        event.stopPropagation()
      },
      onPointerMove: (event: React.PointerEvent) => {
        const origin = resizeOrigin.current[key]
        const region = panelsRef.current[key].region
        if (!origin || !region) return
        const dx = event.clientX - origin.pointerX
        const signedDx = region === 'left' ? dx : -dx
        setPanels((prev) => ({
          ...prev,
          [key]: { ...prev[key], width: Math.min(MAX_PANEL_WIDTH, Math.max(MIN_PANEL_WIDTH, origin.startWidth + signedDx)) },
        }))
      },
      onPointerUp: () => {
        if (resizeOrigin.current[key]) onResizeActiveChangeRef.current?.(false)
        resizeOrigin.current[key] = null
      },
    }
  }

  // ---- grouped-panel header drag: detaches (ungroups) the dragged panel
  // back to floating once dragged far enough, then keeps following the same
  // gesture (`continueAsFloatingDrag`); a plain click (handled separately,
  // via `setActiveTab`) switches tabs instead ----
  const ungroupOrigin = useRef<Record<PanelKey, { pointerX: number; pointerY: number } | null>>({
    layers: null,
    inspector: null,
    resources: null,
    tilePalette: null,
    components: null,
    effects: null,
  })

  function makeGroupedHeaderDragHandlers(key: PanelKey) {
    return {
      onPointerDown: (event: React.PointerEvent) => {
        ungroupOrigin.current[key] = { pointerX: event.clientX, pointerY: event.clientY }
        event.currentTarget.setPointerCapture(event.pointerId)
      },
      onPointerMove: (event: React.PointerEvent) => {
        const origin = ungroupOrigin.current[key]
        if (!origin) return
        const region = REGIONS.find((r) => groupsRef.current[r] && groupMemberKeys(groupsRef.current[r]!).includes(key))
        const currentGroup = region ? groupsRef.current[region] : null
        if (!region || !currentGroup) return
        const dist = Math.hypot(event.clientX - origin.pointerX, event.clientY - origin.pointerY)
        if (dist < UNGROUP_DRAG_THRESHOLD_PX) return

        ungroupOrigin.current[key] = null
        const { group: nextGroup, promotedKey } = removeFromGroup(currentGroup, key)
        const floatX = event.clientX - 24
        const floatY = event.clientY - 12
        setPanels((prev) => ({
          ...prev,
          [key]: { ...prev[key], region: null, position: { x: floatX, y: floatY } },
          ...(promotedKey ? { [promotedKey]: { ...prev[promotedKey], region: currentGroup.region, width: currentGroup.width } } : {}),
        }))
        setGroups((prev) => ({ ...prev, [region]: nextGroup }))
        continueAsFloatingDrag(key, event.clientX, event.clientY, floatX, floatY)
      },
      onPointerUp: () => {
        ungroupOrigin.current[key] = null
      },
    }
  }

  /** Hides `key` via `App.tsx`'s own `panelVisible` (the caller still has to
   * flip that) — this only handles the group-structure side: if `key` is
   * currently part of a region's group, it's removed from that group first
   * (same collapse rules as detaching), so a closed tab doesn't linger as an
   * empty slot. No-ops if `key` is already solo. */
  function closePanel(key: PanelKey) {
    const region = REGIONS.find((r) => groupsRef.current[r] && groupMemberKeys(groupsRef.current[r]!).includes(key))
    if (!region) return
    const currentGroup = groupsRef.current[region]!
    const { group: nextGroup, promotedKey } = removeFromGroup(currentGroup, key)
    if (promotedKey) {
      setPanels((prev) => ({ ...prev, [promotedKey]: { ...prev[promotedKey], region: currentGroup.region, width: currentGroup.width } }))
    }
    setGroups((prev) => ({ ...prev, [region]: nextGroup }))
  }

  // ---- split-mode divider drag — `dividerIndex` separates `panes[dividerIndex]`
  // from `panes[dividerIndex + 1]` ----
  const splitDragOrigin = useRef<{ pointerY: number; startRatios: [number, number] } | null>(null)
  function makeSplitDividerHandlers(region: FixedRegion, dividerIndex: number) {
    return {
      onPointerDown: (event: React.PointerEvent) => {
        const group = groupsRef.current[region]
        const ratios = group?.mode === 'split' ? group.ratios : [0.5, 0.5]
        splitDragOrigin.current = { pointerY: event.clientY, startRatios: [ratios[dividerIndex], ratios[dividerIndex + 1]] }
        event.currentTarget.setPointerCapture(event.pointerId)
      },
      onPointerMove: (event: React.PointerEvent) => {
        const origin = splitDragOrigin.current
        const containerRect = groupContainerRefs[region].current?.getBoundingClientRect()
        const group = groupsRef.current[region]
        if (!origin || !containerRect || group?.mode !== 'split') return
        const dy = event.clientY - origin.pointerY
        const deltaRatio = dy / containerRect.height
        const pairTotal = origin.startRatios[0] + origin.startRatios[1]
        const nextFirst = Math.min(pairTotal - MIN_PANE_RATIO, Math.max(MIN_PANE_RATIO, origin.startRatios[0] + deltaRatio))
        const nextSecond = pairTotal - nextFirst
        setGroups((prev) => {
          const g = prev[region]
          if (g?.mode !== 'split') return prev
          const ratios = g.ratios.map((r, i) => (i === dividerIndex ? nextFirst : i === dividerIndex + 1 ? nextSecond : r))
          return { ...prev, [region]: { ...g, ratios } }
        })
      },
      onPointerUp: () => {
        splitDragOrigin.current = null
      },
    }
  }

  // ---- group's own canvas-facing-edge resize ----
  const groupResizeOrigin = useRef<Record<FixedRegion, { pointerX: number; startWidth: number } | null>>({ left: null, right: null })
  function makeGroupResizeHandlers(region: FixedRegion) {
    return {
      onPointerDown: (event: React.PointerEvent) => {
        groupResizeOrigin.current[region] = { pointerX: event.clientX, startWidth: groupsRef.current[region]?.width ?? 0 }
        onResizeActiveChangeRef.current?.(true)
        event.currentTarget.setPointerCapture(event.pointerId)
        event.stopPropagation()
      },
      onPointerMove: (event: React.PointerEvent) => {
        const origin = groupResizeOrigin.current[region]
        const currentGroup = groupsRef.current[region]
        if (!origin || !currentGroup) return
        const dx = event.clientX - origin.pointerX
        const signedDx = region === 'left' ? dx : -dx
        const width = Math.min(MAX_PANEL_WIDTH, Math.max(MIN_PANEL_WIDTH, origin.startWidth + signedDx))
        setGroups((prev) => (prev[region] ? { ...prev, [region]: { ...prev[region]!, width } } : prev))
      },
      onPointerUp: () => {
        if (groupResizeOrigin.current[region]) onResizeActiveChangeRef.current?.(false)
        groupResizeOrigin.current[region] = null
      },
    }
  }

  /** Activates `key`'s tab — whether it's a top-level tabs group's member or
   * (the "hybrid" case) one tab of a nested tabs pane inside a split. */
  function setActiveTab(region: FixedRegion, key: PanelKey) {
    setGroups((prev) => {
      const group = prev[region]
      if (!group) return prev
      if (group.mode === 'tabs') return { ...prev, [region]: { ...group, activeTab: key } }
      const panes = group.panes.map((pane) => (pane.kind === 'tabs' && pane.members.includes(key) ? { ...pane, activeTab: key } : pane))
      return { ...prev, [region]: { ...group, panes } }
    })
  }

  return {
    panels,
    groups,
    dropPreview,
    panelRefs,
    groupContainerRefs,
    groupPaneRefs,
    dragHandlers: Object.fromEntries(PANEL_KEYS.map((key) => [key, makeDragHandlers(key)])) as Record<PanelKey, ReturnType<typeof makeDragHandlers>>,
    resizeHandlers: Object.fromEntries(PANEL_KEYS.map((key) => [key, makeResizeHandlers(key)])) as Record<PanelKey, ReturnType<typeof makeResizeHandlers>>,
    groupedHeaderDragHandlers: Object.fromEntries(PANEL_KEYS.map((key) => [key, makeGroupedHeaderDragHandlers(key)])) as Record<PanelKey, ReturnType<typeof makeGroupedHeaderDragHandlers>>,
    makeSplitDividerHandlers,
    makeGroupResizeHandlers,
    setActiveTab,
    closePanel,
  }
}
