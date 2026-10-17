import { Popover, insideOrPopover } from './Select'
import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef, useState } from 'react'
import { TIMELINE_HEIGHT } from '../layoutConstants'
import { useLayoutMetrics } from '../layoutMetrics'
import type { AnimationSummary, Easing, FieldInfo, KeyframeValue, NodeSummary, OpaqueId, PropertyName } from '../ipc/document'
import { CurveEditorPanel } from './CurveEditorPanel'
import { COMPONENT_LABEL } from './ComponentFields'
import { FloatingPanel } from './FloatingPanel'
import {
  ChevronDownIcon,
  JumpToEndIcon,
  JumpToStartIcon,
  LoopIcon,
  LoopOnceIcon,
  PauseIcon,
  PingPongIcon,
  PlayIcon,
  PlusIcon,
  TrashIcon,
  ZoomInIcon,
  ZoomOutIcon,
} from './icons'

const KEYFRAME_EPSILON = 1e-4
const NICE_TICK_INTERVALS = [0.1, 0.25, 0.5, 1, 2, 5, 10, 30, 60]
const MIN_PX_BETWEEN_TICKS = 46
const DEFAULT_PIXELS_PER_SECOND = 100
const MIN_PIXELS_PER_SECOND = 20
const MAX_PIXELS_PER_SECOND = 800
const ZOOM_FACTOR = 1.5
const RULER_HEIGHT = 20
const ROW_HEIGHT = 24
const FALLBACK_DURATION_SECONDS = 5

const PROPERTIES: { key: PropertyName; label: string }[] = [
  { key: 'Translation', label: 'Position' },
  { key: 'Rotation', label: 'Rotation' },
  { key: 'Scale', label: 'Scale' },
  { key: 'FillColor', label: 'Fill' },
]

/** Words for a field path's segment: `outerRadius` / `z_index` → "outer radius". */
function words(segment: string): string {
  return segment
    .replace(/~1/g, '/')
    .replace(/~0/g, '~')
    .replace(/_/g, ' ')
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .toLowerCase()
}

/** A readable name for a field path (`an1mbyt3_core::field_path`):
 * `behavior/Light2D/intensity` → "Light2D intensity",
 * `kind/Node3D/transform/rotationDegrees` → "3D rotation". Structural
 * segments (`transform`, `content`, `primitive`) are dropped. */
export function fieldLabel(path: string): string {
  const parts = path.split('/')
  let head: string
  let rest: string[]
  if (parts[0] === 'behavior') {
    head = parts[1].replace(/~(\d+)$/, ' #$1')
    rest = parts.slice(2)
  } else if (parts[0] === 'kind') {
    head = parts[1] === 'Node3D' ? '3D' : parts[1]
    rest = parts.slice(2)
  } else if (parts[0] === 'node') {
    head = ''
    rest = parts.slice(1)
  } else {
    head = parts[0] === 'transform' ? '2D' : parts[0]
    rest = parts.slice(1)
  }
  const tail = rest
    .filter((p) => !['transform', 'content', 'primitive'].includes(p))
    .map((p) => (p === 'rotationDegrees' ? 'rotation' : words(p)))
  return [head, ...tail].filter(Boolean).join(' ')
}

/** A field's name as the Inspector shows it: the component's Inspector
 * title (`VrmPoses` → "VRM Poses (states)", `Node3D` → "3D"), then its
 * fields (list entries by their own names). */
export function fieldInfoLabel(label: string[]): string {
  const [head = '', ...rest] = label
  const [kind, nth] = head.split(' #')
  const title = (COMPONENT_LABEL as Record<string, string>)[kind] ?? ({ Node3D: '3D', Transform: '2D', Node: '' } as Record<string, string>)[kind] ?? kind
  const named = nth ? `${title} #${Number(nth) + 1}` : title
  // `=name` parts are list entries' own names (a pose, a bone): verbatim.
  return [named, ...rest.map((p) => (p.startsWith('=') ? p.slice(1) : p === 'rotationDegrees' ? 'rotation' : words(p)))].filter(Boolean).join(' › ')
}

/** A field value for a tooltip. */
function formatField(v: [number, number, number, number]): string {
  return v.map((x) => (Math.abs(x) >= 100 ? x.toFixed(0) : x.toFixed(2))).join(', ')
}

type LoopMode = 'Once' | 'Loop' | 'PingPong'
type SelectedKeyframe = { property: PropertyName; time: number }
interface PointerHandlers {
  onPointerDown: (event: React.PointerEvent) => void
  onPointerMove: (event: React.PointerEvent) => void
  onPointerUp: (event: React.PointerEvent) => void
}

function encodeId(id: OpaqueId): string {
  return `${id.idx}:${id.version}`
}

/** A coarse, time-linear estimate of a `MotionPath` track's progress value
 * at `time` — used purely to keep the Timeline's progress field showing
 * "what the guide's progress actually is right now" as the playhead moves,
 * not to author anything. Deliberately ignores each keyframe's own eased
 * curve (a plain time-fraction lerp between the two surrounding keyframes)
 * — good enough for a live readout; the real, eased value is whatever
 * `Animation::apply` already computed into the node's rendered position on
 * this same frame. `null` with no keyframes to interpolate between (nothing
 * to show yet). */
function sampleMotionPathProgress(keyframes: { time: number; value: KeyframeValue }[], time: number): number | null {
  if (keyframes.length === 0) return null
  const progressOf = (v: KeyframeValue) => (v.kind === 'MotionPath' ? v.value : 0)
  if (time <= keyframes[0].time) return progressOf(keyframes[0].value)
  const last = keyframes[keyframes.length - 1]
  if (time >= last.time) return progressOf(last.value)
  for (let i = 0; i < keyframes.length - 1; i++) {
    const a = keyframes[i]
    const b = keyframes[i + 1]
    if (time >= a.time && time <= b.time) {
      const span = b.time - a.time
      const t = span > 0 ? (time - a.time) / span : 0
      return progressOf(a.value) + (progressOf(b.value) - progressOf(a.value)) * t
    }
  }
  return progressOf(last.value)
}

function pickTickInterval(pixelsPerSecond: number): number {
  for (const interval of NICE_TICK_INTERVALS) {
    if (interval * pixelsPerSecond >= MIN_PX_BETWEEN_TICKS) return interval
  }
  return NICE_TICK_INTERVALS[NICE_TICK_INTERVALS.length - 1]
}

function formatKeyframeValue(value: KeyframeValue): string {
  switch (value.kind) {
    case 'Translation':
    case 'Scale':
      return `${value.value.x.toFixed(1)}, ${value.value.y.toFixed(1)}`
    case 'Rotation':
      return `${((value.value * 180) / Math.PI).toFixed(1)}°`
    case 'FillColor': {
      const [r, g, b, a] = value.value
      return `rgba(${Math.round(r * 255)}, ${Math.round(g * 255)}, ${Math.round(b * 255)}, ${a.toFixed(2)})`
    }
    case 'Path':
      return 'path'
    case 'MotionPath':
      return `${(value.value * 100).toFixed(0)}%`
    case 'Field':
      return formatField(value.value)
  }
}

/** The Timeline's "+ Property" popover: every animatable value of the
 * selected node, searchable. Picking one adds its row (and a keyframe of
 * its current value at the playhead). */
function PropertyPicker({ fields, anchor, onPick, onClose }: { fields: FieldInfo[]; anchor: DOMRect; onPick: (path: string) => void; onClose: () => void }) {
  const [query, setQuery] = useState('')
  const rootRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const away = (e: MouseEvent) => {
      if (rootRef.current && !insideOrPopover(rootRef.current, e.target as Node)) onClose()
    }
    window.addEventListener('mousedown', away)
    return () => window.removeEventListener('mousedown', away)
  }, [onClose])
  const q = query.trim().toLowerCase()
  const shown = fields.filter((f) => !q || fieldInfoLabel(f.label).toLowerCase().includes(q) || f.path.toLowerCase().includes(q))
  return (
    // Fixed, opening upward from the button: the Timeline sits at the
    // screen's bottom edge.
    <div
      ref={rootRef}
      className="fixed z-50 flex max-h-80 w-80 flex-col rounded-lg border border-panel-border bg-panel p-1.5 shadow-2xl backdrop-blur-panel"
      style={{ left: anchor.left, bottom: window.innerHeight - anchor.top + 4 }}
    >
      <input
        autoFocus
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => e.key === 'Escape' && onClose()}
        placeholder="Search properties… (intensity, position, colour)"
        className="mb-1 rounded bg-overlay/10 px-1.5 py-1 text-[11px] text-ink outline-none focus:ring-1 focus:ring-accent"
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        {shown.length === 0 && <p className="px-1 py-2 text-[11px] text-ink-dim">Nothing animatable matches.</p>}
        {shown.map((f) => (
          <button
            key={f.path}
            type="button"
            onClick={() => onPick(f.path)}
            title={`${f.path} = ${formatField(f.value)}`}
            className="flex w-full items-center gap-2 rounded px-1.5 py-1 text-left text-[11px] text-ink hover:bg-accent/15"
          >
            <span className="min-w-0 flex-1 truncate">{fieldInfoLabel(f.label)}</span>
            <span className="shrink-0 text-[9px] text-ink-dim">{f.shape === 'Bool' ? 'on/off' : typeof f.shape === 'object' ? `${f.shape.Vector}×` : f.shape === 'Integer' ? 'int' : ''}</span>
          </button>
        ))}
      </div>
    </div>
  )
}

/**
 * A small custom-drawn dropdown (not a native `<Select>`) so its open list
 * always matches the app's own dark/translucent theme instead of whatever
 * the OS/browser renders a native select's popup as (which varies by
 * platform and can't be restyled) — AN1MBYT3_BUILD_PLAN.md §15's "one
 * consistent, custom-drawn" surface language applies to interactive chrome
 * too, not just icons.
 */
function AnimationDropdown({
  animations,
  activeAnimationId,
  onSelect,
  onAdd,
}: {
  animations: AnimationSummary[]
  activeAnimationId: OpaqueId | null
  onSelect: (id: OpaqueId) => void
  onAdd: () => void
}) {
  const [open, setOpen] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)
  const active = animations.find((a) => activeAnimationId && encodeId(a.id) === encodeId(activeAnimationId))

  useEffect(() => {
    function handleOutsideClick(event: MouseEvent) {
      if (rootRef.current && !insideOrPopover(rootRef.current, event.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', handleOutsideClick)
    return () => document.removeEventListener('mousedown', handleOutsideClick)
  }, [])

  return (
    <div ref={rootRef} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        className="flex items-center gap-1 rounded-md bg-overlay/5 px-2 py-0.5 text-[11px] text-ink hover:bg-overlay/10 disabled:opacity-50"
      >
        <span className="max-w-[110px] truncate">{active?.name ?? 'No animation'}</span>
        <ChevronDownIcon />
      </button>
      {open && (
        <Popover anchor={rootRef} width={160} className="rounded-lg border border-panel-border bg-panel p-1 shadow-2xl backdrop-blur-panel">
          {animations.map((a) => (
            <button
              key={encodeId(a.id)}
              onClick={() => {
                onSelect(a.id)
                setOpen(false)
              }}
              className={`flex w-full items-center rounded-md px-2 py-1 text-left text-[11px] ${
                activeAnimationId && encodeId(activeAnimationId) === encodeId(a.id) ? 'bg-accent/15 text-accent' : 'text-ink hover:bg-overlay/5'
              }`}
            >
              <span className="truncate">{a.name}</span>
            </button>
          ))}
          {animations.length > 0 && <div className="my-1 h-px bg-overlay/10" />}
          <button
            onClick={() => {
              onAdd()
              setOpen(false)
            }}
            className="flex w-full items-center gap-1 rounded-md px-2 py-1 text-left text-[11px] text-ink-dim hover:bg-overlay/5 hover:text-ink"
          >
            <PlusIcon /> New Animation
          </button>
        </Popover>
      )}
    </div>
  )
}

/**
 * Timeline (keyframe mode) / state-machine graph (logic mode) — floating in
 * the same translucent style as every other panel, but permanently docked
 * full-width along the bottom edge (between the icon rail and the right
 * screen edge) rather than a `DockablePanel`: unlike `Layers`/`Inspector`,
 * there's no scenario where a designer wants the timeline anywhere else on
 * screen or narrower than the space available, so it renders its own
 * `FloatingPanel` directly with a fixed position instead of drag/dock logic.
 *
 * Real, multi-property, multi-animation keyframe editing (AN1MBYT3_BUILD_PLAN.md
 * §10 Phase 4's "keyframe edits, including coalesced drag edits as single
 * steps", and §4's "Animations hold timelines → tracks → keyframes... keyed
 * by NodeId + property path"):
 * - One row per animatable property (Position/Rotation/Scale/Fill) for the
 *   selected node, each with its own "+"/"×" add/delete-keyframe-at-playhead
 *   buttons and its own keyframe diamonds.
 * - **Ruler drag scrubs the playhead**; **track-body drag marquee-selects
 *   keyframes** instead — these are deliberately different gestures (an
 *   earlier version scrubbed from *anywhere*, which made "drag to select"
 *   impossible and felt like it was fighting normal click-drag text
 *   selection). Selecting is lifted to the parent (`onSelectionChange`) so
 *   `App.tsx`'s single global Delete/Backspace handler can delete selected
 *   keyframes instead of the selected node when there's an active keyframe
 *   selection — see that handler for why a second, local keydown listener
 *   here would silently race it.
 * - Dragging a diamond retimes that keyframe (single-keyframe drag only —
 *   marquee selection is for visual grouping/bulk delete, not yet a group
 *   drag) — a live, uncommitted preview (`onPreviewKeyframeTime`) while
 *   dragging, one undoable edit (`onCommitKeyframeTime`) on release,
 *   mirroring `commitNodeTranslation`'s "restore origin, then apply the real
 *   command" pattern. A drag that never actually moved the pointer in time
 *   is treated as a plain click instead — just jumps the playhead there.
 * - Transport: jump-to-start/jump-to-end, ▶/⏸ (a local UI-only clock, not
 *   the engine's `Playback`), and a loop-mode cycle button (Once → Loop →
 *   PingPong).
 * - Zoom (+/−) and tick marks at "nice" intervals, chosen so ticks never
 *   crowd closer than ~46px apart.
 * - A **fixed, authored duration** (`activeAnimation.duration`, editable via
 *   the small number field next to the transport controls) — the visible/
 *   scrubbable/keyable range never silently grows just because a keyframe
 *   gets dragged near its edge (the previous version derived "duration"
 *   from the longest keyframe, which meant dragging a key further right
 *   kept expanding the range with no real ceiling — not how After Effects/
 *   Rive/Lottie work). New keyframes and keyframe drags are clamped to this
 *   range by `WasmEditor` itself (`add_keyframe`/`preview_keyframe_time`/
 *   `commit_keyframe_time`), not just here.
 * - A custom (not native `<Select>`) animation dropdown (`AnimationDropdown`
 *   above) — an artboard can hold many named `Animation`s; this switches
 *   which one the Timeline authors/scrubs.
 *
 * Editing only ever needs `selectedNode`/`activeAnimation` to exist — gating
 * on "is Animate mode active" used to also be necessary here (scrubbing/
 * keying a node's pose outside that mode would silently fight with Design-
 * mode transform edits, the same reasoning `set_state_machine_active`'s doc
 * comment gives for why its own pose takeover is mode-gated), but that's now
 * `App.tsx`'s job entirely: this component is only ever mounted while the
 * bottom dock's Animate tab is the active one (`effectiveBottomTab`), so it
 * has no separate "outside Animate mode, render read-only" state of its own
 * to represent any more.
 */
/** Imperative handle exposed via `ref` — `App.tsx`'s own already-reliable
 * canvas render loop (`frame()`, which unconditionally calls
 * `editor.render(...)` every frame) calls `tick(deltaSeconds)` on this every
 * frame too, instead of this component running a second, independent
 * `requestAnimationFrame` loop of its own. See the exported `TimelinePanel`'s
 * doc comment for why. */
export interface TimelinePanelHandle {
  /** Returns the freshly-computed scrub time (so `App.tsx`'s `frame()` can
   * pass it straight into the very same `editor.render(...)` call that
   * already runs this frame — see `render`'s `playback_scrub_time` doc
   * comment in `editor.rs` for why that matters), or `null` while not
   * playing. */
  tick: (deltaSeconds: number) => number | null
}

export const TimelinePanel = forwardRef<TimelinePanelHandle, {
  animations: AnimationSummary[]
  activeAnimationId: OpaqueId | null
  selectedNode: NodeSummary | null
  onScrub: (time: number) => void
  onAddKeyframe: (property: PropertyName, time: number) => void
  onRemoveKeyframe: (property: PropertyName, time: number) => void
  onPreviewKeyframeTime: (property: PropertyName, index: number, newTime: number) => void
  onCommitKeyframeTime: (property: PropertyName, index: number, originTime: number, newTime: number) => void
  onSetKeyframeEasing: (property: PropertyName, index: number, easing: Easing) => void
  onAddAnimation: () => void
  onSelectAnimation: (animationId: OpaqueId) => void
  onDeleteAnimation: (animationId: OpaqueId) => void
  onSetAnimationDuration: (seconds: number) => void
  onSelectionChange: (selection: SelectedKeyframe[]) => void
  /** `progress` is `0..=1` along the guide's own path — the `MotionPath`
   * counterpart to `onAddKeyframe`, which has no equivalent "current value"
   * to capture (see `WasmEditor::add_motion_path_keyframe`'s doc comment). */
  onAddMotionPathKeyframe: (time: number, progress: number) => void
  /** Every animatable value of the selected node ("+ Property"). */
  fieldOptions?: () => FieldInfo[]
  /** Record mode: any Inspector change keys at the playhead. */
  recording?: boolean
  onToggleRecording?: () => void
  height?: number
  resizeHandleProps?: PointerHandlers
  /** `true` right after loading the demo scene ("Load Demo Scene" builds a
   * real keyframed animation and opens the bottom dock's Animate tab — see
   * `App.tsx`'s `handleLoadDemo` — but that alone doesn't make a newly-
   * created animation visibly *play*; without this, "is the animation
   * actually there?" isn't answerable at a glance). Starts playback once, on
   * this component's first mount after the flag goes true, then immediately
   * calls `onAutoPlayConsumed` to reset it — this component is only mounted
   * at all while the Animate tab is active (unmounts otherwise), so without
   * consuming the flag, switching back to that tab later would replay every
   * time. */
  autoPlayOnMount?: boolean
  onAutoPlayConsumed?: () => void
}>(function TimelinePanel({
  animations,
  activeAnimationId,
  selectedNode,
  onScrub,
  onAddKeyframe,
  onRemoveKeyframe,
  onPreviewKeyframeTime,
  onCommitKeyframeTime,
  onSetKeyframeEasing,
  onAddAnimation,
  onSelectAnimation,
  onDeleteAnimation,
  onSetAnimationDuration,
  onSelectionChange,
  onAddMotionPathKeyframe,
  fieldOptions,
  recording = false,
  onToggleRecording,
  height = TIMELINE_HEIGHT,
  resizeHandleProps,
  autoPlayOnMount,
  onAutoPlayConsumed,
}, ref) {
  const activeAnimation = animations.find((a) => activeAnimationId && encodeId(a.id) === encodeId(activeAnimationId))
  const editable = selectedNode !== null && activeAnimation !== undefined
  const m = useLayoutMetrics()

  const [scrubTime, setScrubTime] = useState(0)
  // `tick()` needs to read the *current* scrub time and compute+use the next
  // one entirely synchronously, in one go — it used to do this via
  // `setScrubTime`'s functional-updater form (`setScrubTime(current => ...)`)
  // and capture the updater's own return value into a local, on the
  // assumption that the updater runs synchronously within the same call.
  // That assumption is wrong here: `tick()` is called from a plain
  // `requestAnimationFrame` callback, not a React-managed event, so React
  // defers actually invoking the updater to its own later render pass —
  // the captured "result" stayed at its initial value (`0`) on every single
  // call, forever, which is why playback silently resampled to time 0 every
  // frame instead of ever advancing. A ref sidesteps this entirely: it's
  // mutated synchronously, immediately, with no batching to defer it. Kept
  // in sync with `scrubTime` everywhere `scrubTime` is set from outside
  // `tick()` (manual scrub, load/duration-change resets) so a ref update and
  // a state update are never a source of truth ahead of the other.
  const scrubTimeRef = useRef(0)
  const [isPlaying, setIsPlaying] = useState(autoPlayOnMount ?? false)

  useEffect(() => {
    if (autoPlayOnMount) onAutoPlayConsumed?.()
    // Mount-only: reads `autoPlayOnMount`/`onAutoPlayConsumed` as they are at
    // the moment this component first appears, deliberately not re-running
    // if either changes later.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  const [loopMode, setLoopMode] = useState<LoopMode>('Once')
  const [pixelsPerSecond, setPixelsPerSecond] = useState(DEFAULT_PIXELS_PER_SECOND)
  const [durationText, setDurationText] = useState('')
  const [selectedKeyframes, setSelectedKeyframes] = useState<SelectedKeyframe[]>([])
  // Overrides a dragged keyframe's rendered position while its drag is live
  // — `onPreviewKeyframeTime` doesn't return updated data (same "preview is
  // untracked, nothing to react to yet" reasoning as on-canvas node
  // dragging), so the diamond's on-screen position during a drag has to
  // come from here instead of `selectedNode.tracks`.
  const [dragPreview, setDragPreview] = useState<{ property: PropertyName; index: number; time: number } | null>(null)
  const [marqueeRect, setMarqueeRect] = useState<{ x: number; y: number; w: number; h: number } | null>(null)
  // Double-clicking keyframe `index` opens the Curve Editor for the
  // *previous* keyframe's easing (`index - 1`) — `Easing` governs the
  // segment out of the keyframe it's stored on, so "the curve leading into
  // the one double-clicked" lives one keyframe earlier. `null` (not opened)
  // while `index === 0` — there's no incoming segment for the first keyframe.
  const [openCurveEditor, setOpenCurveEditor] = useState<{ property: PropertyName; index: number; anchor: { x: number; y: number } } | null>(null)
  // The progress value (`0..=1`) the `MotionPath` row's "+" button keys next
  // — there's no "current node value" to capture the way every other
  // property's `+` does (see `onAddMotionPathKeyframe`'s doc comment), so
  // this field is what the user actually sets before adding a keyframe.
  const [motionPathProgress, setMotionPathProgress] = useState(0.5)

  const contentRef = useRef<HTMLDivElement>(null)
  const scrollContainerRef = useRef<HTMLDivElement>(null)
  const durationInputRef = useRef<HTMLInputElement>(null)
  const scrubbingRef = useRef(false)
  const keyframeDragRef = useRef<{ property: PropertyName; index: number; originTime: number; moved: boolean } | null>(null)
  const marqueeStartRef = useRef<{ x: number; y: number } | null>(null)
  const playDirectionRef = useRef<1 | -1>(1)

  const duration = activeAnimation?.duration ?? FALLBACK_DURATION_SECONDS
  const contentWidth = Math.max(duration * pixelsPerSecond, 100)
  const tickInterval = pickTickInterval(pixelsPerSecond)
  const ticks: number[] = []
  for (let t = 0; t <= duration + 1e-6; t += tickInterval) ticks.push(Math.round(t * 1000) / 1000)

  const tracksByProperty = (property: PropertyName) => selectedNode?.tracks.find((t) => t.property === property)?.keyframes ?? []
  const isSelected = (property: PropertyName, time: number) =>
    selectedKeyframes.some((s) => s.property === property && Math.abs(s.time - time) < KEYFRAME_EPSILON)

  // The selected node's `MotionPath` track, if it has one — `undefined`
  // means "no guide bound yet", not "guide bound but empty" (an empty-but-
  // bound track still has `guide` set, just zero `keyframes`).
  const motionPathTrack = selectedNode?.tracks.find((t) => t.property === 'MotionPath')
  // Every visible row, in display order — the 4 fixed properties, plus a
  // `MotionPath` row that's always present (its own label-column control,
  // `MotionPathGuideControl`, handles both "no guide bound yet" and "guide
  // bound" — there's no separate on/off state for the row itself). Layout,
  // marquee row math, and the diamond track area all iterate this array so
  // the row can't drift out of sync between them.
  // Field-path tracks (any Inspector value) the node has, plus rows picked
  // from "+ Property" this session that aren't keyed yet.
  const [pickerAnchor, setPickerAnchor] = useState<DOMRect | null>(null)
  const [pinnedFields, setPinnedFields] = useState<string[]>([])
  // Inspector-style names for the node's field tracks.
  const hasFieldTracks = (selectedNode?.tracks ?? []).some((t) => t.property.startsWith('Field:')) || pinnedFields.length > 0
  const labelByPath = useMemo(() => {
    const map = new Map<string, string>()
    if (hasFieldTracks && fieldOptions) for (const f of fieldOptions()) map.set(f.path, fieldInfoLabel(f.label))
    return map
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedNode, hasFieldTracks])
  // A track whose value no longer exists on the node (its component was
  // removed, or an older track pointed at a list position that moved):
  // flagged so it can be re-picked from "+ Property".
  const nameOf = (path: string) => labelByPath.get(path) ?? (labelByPath.size > 0 ? `⚠ missing: ${fieldLabel(path)}` : fieldLabel(path))
  const fieldRows: { key: PropertyName; label: string }[] = []
  for (const t of selectedNode?.tracks ?? []) {
    if (t.property.startsWith('Field:')) fieldRows.push({ key: t.property, label: nameOf(t.property.slice('Field:'.length)) })
  }
  for (const path of pinnedFields) {
    const key = `Field:${path}` as PropertyName
    if (!fieldRows.some((r) => r.key === key)) fieldRows.push({ key, label: nameOf(path) })
  }
  const visibleRows: { key: PropertyName; label: string }[] = [...PROPERTIES, { key: 'MotionPath', label: 'Motion Path' }, ...fieldRows]
  useEffect(() => setPinnedFields([]), [selectedNode?.id.idx, selectedNode?.id.version])

  useEffect(() => {
    setDurationText(duration.toFixed(2))
  }, [duration])

  // Keeps the `MotionPath` row's progress field showing "the guide's
  // progress right now" as the playhead moves, instead of whatever value
  // was last typed into it — dragging the seekbar over/near an existing
  // keyframe (or anywhere between two of them) updates the number live,
  // the same way every other property's Inspector field already tracks the
  // node's actual current pose. No-op (keeps whatever's there) once there
  // are no keyframes yet to sample.
  useEffect(() => {
    const sampled = sampleMotionPathProgress(motionPathTrack?.keyframes ?? [], scrubTime)
    if (sampled !== null) setMotionPathProgress(sampled)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scrubTime, motionPathTrack])

  // Clamps the playhead into range whenever the active animation (or its
  // duration) changes, so switching to/shrinking a shorter animation never
  // leaves the playhead sitting past its end. Skipped while actively
  // playing — the play loop below already clamps every tick itself
  // (`next >= duration` / `next <= 0`), and letting *this* effect also fire
  // mid-playback risks stepping on the loop's own `setScrubTime` calls for
  // no reason (this is only meant to handle "duration shrank while paused",
  // e.g. editing the duration field).
  useEffect(() => {
    if (isPlaying) return
    setScrubTime((current) => {
      const clamped = Math.min(current, duration)
      scrubTimeRef.current = clamped
      return clamped
    })
  }, [duration, isPlaying])

  // Trackpad pinch / Ctrl+scroll zooms the timeline instead of scrolling it
  // — Chromium reports a trackpad pinch gesture as a `wheel` event with
  // `ctrlKey: true` (there's no separate "pinch" DOM event), which is also
  // exactly what a real Ctrl+scroll produces, so handling one handles both.
  // A real (non-passive) native listener, not React's `onWheel` — same
  // "React's synthetic wheel listener is passive by default, so
  // `preventDefault()` would no-op" reasoning as `InspectorPanel`'s
  // `NumberField`. Plain scroll/trackpad-swipe (no Ctrl) is deliberately
  // left alone — the browser's native `overflow-x-auto` panning (and
  // Shift+wheel-as-horizontal-scroll) already works without any JS here.
  useEffect(() => {
    const container = scrollContainerRef.current
    if (!container) return
    function handleWheel(event: WheelEvent) {
      if (!event.ctrlKey) return
      event.preventDefault()
      const zoomFactor = event.deltaY > 0 ? 1 / ZOOM_FACTOR : ZOOM_FACTOR
      setPixelsPerSecond((p) => Math.min(MAX_PIXELS_PER_SECOND, Math.max(MIN_PIXELS_PER_SECOND, p * zoomFactor)))
    }
    container.addEventListener('wheel', handleWheel, { passive: false })
    return () => container.removeEventListener('wheel', handleWheel)
  }, [])

  // Scroll-to-nudge the duration field, same convention (and same
  // real-listener-required reason) as `InspectorPanel`'s `NumberField`.
  useEffect(() => {
    const input = durationInputRef.current
    if (!input) return
    function handleWheel(event: WheelEvent) {
      event.preventDefault()
      const step = event.shiftKey ? 5 : 0.5
      onSetAnimationDuration(Math.max(0.1, duration + (event.deltaY > 0 ? -step : step)))
    }
    input.addEventListener('wheel', handleWheel, { passive: false })
    return () => input.removeEventListener('wheel', handleWheel)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [duration])

  const localPoint = (clientX: number, clientY: number) => {
    const rect = contentRef.current!.getBoundingClientRect()
    return { x: clientX - rect.left, y: clientY - rect.top }
  }

  const timeAtX = (x: number) => Math.max(0, Math.min(duration, x / pixelsPerSecond))

  const scrubTo = (time: number) => {
    scrubTimeRef.current = time
    setScrubTime(time)
    onScrub(time)
  }

  const handleRulerMouseDown = (event: React.MouseEvent<HTMLDivElement>) => {
    if (!editable) return
    event.preventDefault()
    scrubbingRef.current = true
    scrubTo(timeAtX(localPoint(event.clientX, event.clientY).x))
  }

  const handleTracksMouseDown = (event: React.MouseEvent<HTMLDivElement>) => {
    if (!editable) return
    event.preventDefault()
    const point = localPoint(event.clientX, event.clientY)
    marqueeStartRef.current = point
    setMarqueeRect({ x: point.x, y: point.y, w: 0, h: 0 })
    setSelectedKeyframes([])
    onSelectionChange([])
  }

  const handleKeyframeMouseDown = (property: PropertyName, index: number, originTime: number) => (event: React.MouseEvent) => {
    if (!editable) return
    event.preventDefault()
    event.stopPropagation()
    keyframeDragRef.current = { property, index, originTime, moved: false }
    setDragPreview({ property, index, time: originTime })
    setSelectedKeyframes([{ property, time: originTime }])
    onSelectionChange([{ property, time: originTime }])
  }

  const handleKeyframeDoubleClick = (property: PropertyName, index: number) => (event: React.MouseEvent) => {
    if (!editable || index === 0) return
    event.preventDefault()
    event.stopPropagation()
    const point = localPoint(event.clientX, event.clientY)
    setOpenCurveEditor({ property, index, anchor: { x: point.x, y: point.y + 12 } })
  }

  useEffect(() => {
    function handleMouseMove(event: MouseEvent) {
      if (scrubbingRef.current) {
        scrubTo(timeAtX(localPoint(event.clientX, event.clientY).x))
        return
      }

      if (marqueeStartRef.current) {
        const point = localPoint(event.clientX, event.clientY)
        const start = marqueeStartRef.current
        const rect = { x: Math.min(start.x, point.x), y: Math.min(start.y, point.y), w: Math.abs(point.x - start.x), h: Math.abs(point.y - start.y) }
        setMarqueeRect(rect)

        const matched: SelectedKeyframe[] = []
        visibleRows.forEach((p, rowIndex) => {
          const rowTop = RULER_HEIGHT + rowIndex * ROW_HEIGHT
          const rowBottom = rowTop + ROW_HEIGHT
          if (rect.y > rowBottom || rect.y + rect.h < rowTop) return
          tracksByProperty(p.key).forEach((kf) => {
            const px = kf.time * pixelsPerSecond
            if (px >= rect.x && px <= rect.x + rect.w) matched.push({ property: p.key, time: kf.time })
          })
        })
        setSelectedKeyframes(matched)
        onSelectionChange(matched)
        return
      }

      const drag = keyframeDragRef.current
      if (!drag) return
      const time = timeAtX(localPoint(event.clientX, event.clientY).x)
      drag.moved = drag.moved || Math.abs(time - drag.originTime) > KEYFRAME_EPSILON
      setDragPreview({ property: drag.property, index: drag.index, time })
      onPreviewKeyframeTime(drag.property, drag.index, time)
    }

    function handleMouseUp(event: MouseEvent) {
      scrubbingRef.current = false

      if (marqueeStartRef.current) {
        marqueeStartRef.current = null
        setMarqueeRect(null)
        return
      }

      const drag = keyframeDragRef.current
      keyframeDragRef.current = null
      setDragPreview(null)
      if (!drag) return

      if (!drag.moved) {
        scrubTo(drag.originTime)
        return
      }

      const finalTime = timeAtX(localPoint(event.clientX, event.clientY).x)
      onCommitKeyframeTime(drag.property, drag.index, drag.originTime, finalTime)
      setSelectedKeyframes([{ property: drag.property, time: finalTime }])
      onSelectionChange([{ property: drag.property, time: finalTime }])
    }

    window.addEventListener('mousemove', handleMouseMove)
    window.addEventListener('mouseup', handleMouseUp)
    return () => {
      window.removeEventListener('mousemove', handleMouseMove)
      window.removeEventListener('mouseup', handleMouseUp)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedNode, duration, pixelsPerSecond])

  // ▶ playback — a local UI clock only (see module doc). `loopMode`
  // decides what happens at the ends: `Once` stops, `Loop` wraps to 0 and
  // keeps going, `PingPong` reverses direction and keeps going. Driven by
  // `App.tsx`'s own canvas render loop calling `tick(deltaSeconds)` every
  // frame via the imperative handle below — *not* a second, independent
  // `requestAnimationFrame` loop living in this component. Two competing
  // rAF loops (this one, plus `App.tsx`'s `frame()`, which is what actually
  // draws the canvas and is already proven reliable — panning/zooming never
  // had this problem) reading and writing the same underlying wasm document
  // through two entirely separate scheduling sources was real, hard-to-pin-down
  // timing complexity for no real benefit; routing through the one loop that
  // already reliably calls `editor.render()` every frame removes that
  // category of bug outright rather than chasing it one closure at a time.
  useImperativeHandle(
    ref,
    () => ({
      tick(deltaSeconds: number) {
        if (!isPlaying) return null
        // Computed directly from `scrubTimeRef` (synchronous, always
        // current) rather than via `setScrubTime`'s functional-updater form
        // — see `scrubTimeRef`'s own doc comment for why reading the
        // updater's return value back in this same call never actually
        // worked (React defers running the updater outside a React-managed
        // event, so it silently never advanced past time 0 every frame).
        let next = scrubTimeRef.current + deltaSeconds * playDirectionRef.current
        let shouldStop = false
        if (next >= duration) {
          if (loopMode === 'Once') {
            shouldStop = true
            next = duration
          } else if (loopMode === 'Loop') {
            next -= duration
          } else {
            next = duration - (next - duration)
            playDirectionRef.current = -1
          }
        } else if (next <= 0) {
          if (loopMode === 'PingPong') {
            next = -next
            playDirectionRef.current = 1
          } else {
            next = 0
          }
        }
        const computedNext = next
        scrubTimeRef.current = computedNext
        setScrubTime(computedNext)
        if (shouldStop) setIsPlaying(false)
        // `onScrub` ultimately calls into the wasm editor (`App.tsx`'s
        // `handleScrub` -> `set_scrub_time` -> `Animation::apply`). If that
        // ever throws, an uncaught exception here would propagate out of
        // `App.tsx`'s own `frame()` callback and kill *that* loop's
        // reschedule too — catch it and stop playback cleanly (icon flips
        // back to Play, matching what actually happened) instead of an
        // uncaught exception silently breaking canvas rendering entirely.
        try {
          onScrub(computedNext)
        } catch (err) {
          console.error('Timeline playback: onScrub threw, stopping playback', err)
          setIsPlaying(false)
          return null
        }
        return computedNext
      },
    }),
    [isPlaying, duration, loopMode, onScrub],
  )

  const handleTogglePlay = () => {
    if (!editable) return
    if (!isPlaying && scrubTime >= duration) {
      scrubTo(0)
    }
    playDirectionRef.current = 1
    setIsPlaying((p) => !p)
  }

  const handleJumpToStart = () => {
    setIsPlaying(false)
    scrubTo(0)
  }

  const handleJumpToEnd = () => {
    setIsPlaying(false)
    scrubTo(duration)
  }

  const handleCycleLoopMode = () => {
    setLoopMode((current) => (current === 'Once' ? 'Loop' : current === 'Loop' ? 'PingPong' : 'Once'))
  }

  const commitDurationText = () => {
    const parsed = Number.parseFloat(durationText)
    if (Number.isFinite(parsed) && parsed > 0) {
      onSetAnimationDuration(parsed)
    } else {
      setDurationText(duration.toFixed(2))
    }
  }

  return (
    <div
      className="pointer-events-auto absolute select-none"
      style={{ left: m.leftDockOffset, right: m.edgeMargin, bottom: m.edgeMargin, height }}
    >
      <FloatingPanel className="relative flex h-full flex-col gap-1.5 p-2">
        {resizeHandleProps && (
          <div {...resizeHandleProps} className="group absolute -top-1.5 left-0 z-10 h-3 w-full touch-none" style={{ cursor: 'ns-resize' }}>
            <div className="mx-auto mt-1.5 h-px w-full bg-transparent group-hover:bg-accent/70 group-active:bg-accent" />
          </div>
        )}
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-1.5">
            <span className="text-xs font-medium uppercase tracking-wide text-ink-dim">Timeline</span>
            <AnimationDropdown
              animations={animations}
              activeAnimationId={activeAnimationId}
              onSelect={onSelectAnimation}
              onAdd={onAddAnimation}
            />
            {activeAnimationId && (
              <button
                onClick={() => onDeleteAnimation(activeAnimationId)}
                title="Delete animation"
                className="flex h-5 w-5 items-center justify-center rounded-md text-ink-dim hover:bg-red-500/10 hover:text-red-400 disabled:opacity-50"
              >
                <TrashIcon />
              </button>
            )}
            {onToggleRecording && (
              <button
                onClick={onToggleRecording}
                title={recording ? 'Recording: every Inspector change keys at the playhead (click to stop)' : 'Record: every Inspector change keys at the playhead. (Off: only already-animated values are keyed.)'}
                className={`flex h-5 items-center gap-1 rounded-md px-1.5 text-[10px] ${recording ? 'bg-red-500/20 text-red-400' : 'text-ink-dim hover:bg-overlay/10 hover:text-ink'}`}
              >
                <span className={`h-2 w-2 rounded-full ${recording ? 'animate-pulse bg-red-500' : 'bg-red-500/60'}`} />
                Rec
              </button>
            )}
            {fieldOptions && (
              <div className="relative">
                <button
                  onClick={(e) => setPickerAnchor(pickerAnchor ? null : e.currentTarget.getBoundingClientRect())}
                  disabled={!editable}
                  title="Animate any value the selected node has (anything in the Inspector)"
                  className="flex h-5 items-center gap-0.5 rounded-md px-1.5 text-[10px] text-ink-dim hover:bg-overlay/10 hover:text-ink disabled:opacity-40"
                >
                  <PlusIcon /> Property
                </button>
                {pickerAnchor && editable && (
                  <PropertyPicker
                    fields={fieldOptions()}
                    anchor={pickerAnchor}
                    onClose={() => setPickerAnchor(null)}
                    onPick={(path) => {
                      setPickerAnchor(null)
                      setPinnedFields((p) => (p.includes(path) ? p : [...p, path]))
                      onAddKeyframe(`Field:${path}` as PropertyName, scrubTime)
                    }}
                  />
                )}
              </div>
            )}
            <div className="ml-1 flex items-center gap-1 rounded-md bg-overlay/5 px-1.5 py-0.5">
              <span className="text-[10px] text-ink-dim">Dur</span>
              <input
                ref={durationInputRef}
                type="text"
                inputMode="decimal"
                value={durationText}
                onChange={(e) => setDurationText(e.target.value)}
                onBlur={commitDurationText}
                onKeyDown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
                disabled={!activeAnimation}
                className="w-10 bg-transparent text-[11px] text-ink outline-none disabled:opacity-50"
              />
              <span className="text-[10px] text-ink-dim">s</span>
            </div>
          </div>

          <div className="flex items-center gap-1 text-ink-dim">
            <button onClick={handleJumpToStart} disabled={!editable} title="Jump to start" className="rounded-md p-1 hover:bg-overlay/5 hover:text-ink disabled:opacity-40">
              <JumpToStartIcon />
            </button>
            <button onClick={handleTogglePlay} disabled={!editable} title="Play/Pause" className="rounded-md p-1 hover:bg-overlay/5 hover:text-ink disabled:opacity-40">
              {isPlaying ? <PauseIcon /> : <PlayIcon />}
            </button>
            <button onClick={handleJumpToEnd} disabled={!editable} title="Jump to end" className="rounded-md p-1 hover:bg-overlay/5 hover:text-ink disabled:opacity-40">
              <JumpToEndIcon />
            </button>
            <button onClick={handleCycleLoopMode} disabled={!editable} title={`Loop mode: ${loopMode}`} className="rounded-md p-1 hover:bg-overlay/5 hover:text-ink disabled:opacity-40">
              {loopMode === 'Once' && <LoopOnceIcon />}
              {loopMode === 'Loop' && <LoopIcon />}
              {loopMode === 'PingPong' && <PingPongIcon />}
            </button>
            <span className="w-[76px] text-[11px] tabular-nums">
              {scrubTime.toFixed(2)} / {duration.toFixed(2)}
            </span>
            <button
              onClick={() => setPixelsPerSecond((p) => Math.max(MIN_PIXELS_PER_SECOND, p / ZOOM_FACTOR))}
              title="Zoom out"
              className="rounded-md p-1 hover:bg-overlay/5 hover:text-ink"
            >
              <ZoomOutIcon />
            </button>
            <button
              onClick={() => setPixelsPerSecond((p) => Math.min(MAX_PIXELS_PER_SECOND, p * ZOOM_FACTOR))}
              title="Zoom in"
              className="rounded-md p-1 hover:bg-overlay/5 hover:text-ink"
            >
              <ZoomInIcon />
            </button>
          </div>
        </div>

        <div className="flex min-h-0 flex-1 overflow-y-auto">
          <div className="flex w-[210px] shrink-0 flex-col">
            <div className="h-5 shrink-0" />
            {visibleRows.map((p) => {
              const keyframes = tracksByProperty(p.key)
              const keyAtScrub = keyframes.find((k) => Math.abs(k.time - scrubTime) < KEYFRAME_EPSILON)
              const isMotionPath = p.key === 'MotionPath'
              return (
                <div key={p.key} className="flex h-6 shrink-0 items-center justify-between gap-1 pr-1 text-[10px] text-ink-dim">
                  <span className="truncate" title={isMotionPath && !motionPathTrack ? 'Add a Motion Path component in the Inspector first' : p.key.startsWith('Field:') ? (p.label.startsWith('⚠') ? 'This value no longer exists on the object — delete these keys and re-add it with + Property' : p.label) : undefined}>
                    {p.label}
                  </span>
                  <div className="flex shrink-0 items-center gap-0.5">
                    {isMotionPath && (
                      <input
                        type="number"
                        min={0}
                        max={1}
                        step={0.05}
                        value={motionPathProgress}
                        onChange={(e) => setMotionPathProgress(Math.min(1, Math.max(0, Number.parseFloat(e.target.value) || 0)))}
                        disabled={!editable || !motionPathTrack}
                        title="Progress (0-1) along the guide's path for the next keyframe"
                        className="w-8 shrink-0 rounded-sm bg-overlay/5 px-0.5 text-[10px] text-ink outline-none disabled:opacity-30"
                      />
                    )}
                    <button
                      onClick={() => (isMotionPath ? onAddMotionPathKeyframe(scrubTime, motionPathProgress) : onAddKeyframe(p.key, scrubTime))}
                      disabled={!editable || (isMotionPath && !motionPathTrack)}
                      title={isMotionPath ? 'Add Motion Path keyframe at playhead' : `Add ${p.label} keyframe`}
                      className="flex h-4 w-4 shrink-0 items-center justify-center rounded-sm hover:bg-overlay/10 hover:text-ink disabled:opacity-30"
                    >
                      +
                    </button>
                    <button
                      onClick={() => keyAtScrub && onRemoveKeyframe(p.key, keyAtScrub.time)}
                      disabled={!editable || !keyAtScrub}
                      title="Delete keyframe at playhead"
                      className="flex h-4 w-4 shrink-0 items-center justify-center rounded-sm hover:bg-red-500/10 hover:text-red-400 disabled:opacity-30"
                    >
                      ×
                    </button>
                  </div>
                </div>
              )
            })}
          </div>

          <div ref={scrollContainerRef} className="relative h-fit min-h-full flex-1 overflow-x-auto overflow-y-hidden rounded-md bg-overlay/5">
            <div ref={contentRef} className="relative min-h-full" style={{ width: contentWidth, height: RULER_HEIGHT + visibleRows.length * ROW_HEIGHT }}>
              <div onMouseDown={handleRulerMouseDown} className={`relative h-5 border-b border-overlay/10 ${editable ? 'cursor-pointer' : ''}`}>
                {ticks.map((t) => (
                  <div
                    key={t}
                    className="pointer-events-none absolute top-0 h-full border-l border-overlay/10 pl-1 text-[9px] text-ink-dim"
                    style={{ left: t * pixelsPerSecond }}
                  >
                    {t.toFixed(tickInterval < 1 ? 2 : 0)}
                  </div>
                ))}
              </div>

              <div onMouseDown={handleTracksMouseDown}>
                {visibleRows.map((p) => (
                  <div key={p.key} className="relative h-6 border-b border-overlay/5 last:border-0">
                    {tracksByProperty(p.key).map((keyframe, index) => {
                      const time = dragPreview?.property === p.key && dragPreview.index === index ? dragPreview.time : keyframe.time
                      const selected = isSelected(p.key, keyframe.time)
                      return (
                        <div
                          key={index}
                          onMouseDown={handleKeyframeMouseDown(p.key, index, keyframe.time)}
                          onDoubleClick={handleKeyframeDoubleClick(p.key, index)}
                          className={`absolute top-1/2 h-2.5 w-2.5 -translate-x-1/2 -translate-y-1/2 rotate-45 ${
                            selected ? 'bg-ink ring-2 ring-accent' : 'bg-accent'
                          } ${editable ? 'cursor-ew-resize' : ''}`}
                          style={{ left: time * pixelsPerSecond }}
                          title={`t=${time.toFixed(2)} — ${formatKeyframeValue(keyframe.value)}${index > 0 ? ' (double-click to edit incoming curve)' : ''}`}
                        />
                      )
                    })}
                  </div>
                ))}
              </div>

              {marqueeRect && (
                <div
                  className="pointer-events-none absolute border border-accent/60 bg-accent/10"
                  style={{ left: marqueeRect.x, top: marqueeRect.y, width: marqueeRect.w, height: marqueeRect.h }}
                />
              )}

              <div className="pointer-events-none absolute top-0 h-full w-0.5 bg-accent" style={{ left: scrubTime * pixelsPerSecond }} />

              {openCurveEditor && (
                <CurveEditorPanel
                  easing={tracksByProperty(openCurveEditor.property)[openCurveEditor.index - 1]?.easing ?? { kind: 'Linear' }}
                  anchor={openCurveEditor.anchor}
                  onChange={(easing) => onSetKeyframeEasing(openCurveEditor.property, openCurveEditor.index - 1, easing)}
                  onClose={() => setOpenCurveEditor(null)}
                />
              )}
            </div>
          </div>
        </div>
      </FloatingPanel>
    </div>
  )
})
