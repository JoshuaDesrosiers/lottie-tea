import { useEffect, useState } from 'react'
import type { RefObject } from 'react'
import { screenToWorld, worldToScreen, type Camera, type CanvasSize } from '../camera'
import type { NodeSummary, Path, PathCommand } from '../ipc/document'

type Point = { x: number; y: number }
type PointRef = { commandIndex: number; field: 'point' | 'control1' | 'control2' }

/** The draggable anchor point a command contributes — `null` for `Close`,
 * which has none. */
function pointOf(command: PathCommand): Point | null {
  if (command.kind === 'MoveTo' || command.kind === 'LineTo') return command.value
  if (command.kind === 'CubicTo') return command.value.to
  return null
}

function lerpPoint(a: Point, b: Point, t: number): Point {
  return { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t }
}

function cubicPoint(p0: Point, p1: Point, p2: Point, p3: Point, t: number): Point {
  const mt = 1 - t
  return {
    x: p0.x * (mt * mt * mt) + p1.x * (3 * mt * mt * t) + p2.x * (3 * mt * t * t) + p3.x * (t * t * t),
    y: p0.y * (mt * mt * mt) + p1.y * (3 * mt * mt * t) + p2.y * (3 * mt * t * t) + p3.y * (t * t * t),
  }
}

/** Moves one draggable point/handle to `next`, returning the whole updated
 * `Path` — the anchor case (`field === 'point'`) also shifts this command's
 * own `control2` (if it's a `CubicTo`) and the *next* command's `control1`
 * (if that one is a `CubicTo`) by the same delta, so the handles either
 * side of a moved anchor follow it and keep their relative offset instead
 * of staying pinned in place — the "drag a vertex, its handles come with
 * it" behavior every standard vector tool (Illustrator, Figma, Rive) uses.
 * Dragging a handle directly (`control1`/`control2`) only ever moves that
 * one handle — handles never drag each other. */
function movePathPoint(path: Path, ref: PointRef, next: { x: number; y: number }): Path {
  const commands = [...path.commands]
  const command = commands[ref.commandIndex]
  if (!command) return path

  if (ref.field !== 'point') {
    commands[ref.commandIndex] =
      command.kind === 'CubicTo'
        ? { kind: 'CubicTo', value: { ...command.value, [ref.field]: next } }
        : command
    return { commands }
  }

  const previous = pointOf(command)
  if (!previous) return path
  const dx = next.x - previous.x
  const dy = next.y - previous.y

  commands[ref.commandIndex] =
    command.kind === 'CubicTo'
      ? { kind: 'CubicTo', value: { ...command.value, to: next, control2: { x: command.value.control2.x + dx, y: command.value.control2.y + dy } } }
      : command.kind === 'LineTo'
        ? { kind: 'LineTo', value: next }
        : { kind: 'MoveTo', value: next }

  const after = commands[ref.commandIndex + 1]
  if (after?.kind === 'CubicTo') {
    commands[ref.commandIndex + 1] = { kind: 'CubicTo', value: { ...after.value, control1: { x: after.value.control1.x + dx, y: after.value.control1.y + dy } } }
  }

  return { commands }
}

/** Seeds a `CubicTo` from `from` to `to` with both handles a third and
 * two-thirds of the way along the straight line between them — so the
 * curve starts out visually identical to the line it replaces, ready to be
 * dragged into shape. */
function seedCubic(from: Point, to: Point): PathCommand {
  return {
    kind: 'CubicTo',
    value: {
      control1: { x: from.x + (to.x - from.x) / 3, y: from.y + (to.y - from.y) / 3 },
      control2: { x: from.x + ((to.x - from.x) * 2) / 3, y: from.y + ((to.y - from.y) * 2) / 3 },
      to,
    },
  }
}

/** Double-clicking an anchor toggles whether *both* segments meeting at it
 * — the incoming one (this command, ending here) and the outgoing one (the
 * next command, starting here) — are straight or curved, the standard
 * pen-tool "convert anchor point" action. This is deliberately symmetric:
 * a vertex's two handles belong to two *different* segments (the curve
 * arriving and the curve leaving), one on each side of the point, not two
 * handles both belonging to the same one-sided segment — giving it a
 * handle on only one side is what made a moved anchor look like "one
 * handle follows, the other doesn't," since there was nothing on the other
 * side to move. If the two sides disagree (e.g. a track built by dragging
 * before this fix left one curved and one straight), curves whichever
 * side(s) are still straight rather than no-op'ing; only straightens both
 * once every side touching this vertex is already curved — so one
 * double-click always visibly does something. `MoveTo`'s point has no
 * incoming segment, and the point before a `Close` has no outgoing one
 * (`Close` carries no handles to give it) — each side is simply skipped
 * when it doesn't exist. */
function toggleBezierAt(path: Path, commandIndex: number): Path {
  const command = path.commands[commandIndex]
  const point = command ? pointOf(command) : null
  if (!command || !point) return path

  const previous = pointOf(path.commands[commandIndex - 1] ?? { kind: 'Close' })
  const hasIncoming = previous !== null && (command.kind === 'LineTo' || command.kind === 'CubicTo')

  const next = path.commands[commandIndex + 1]
  const nextPoint = next ? pointOf(next) : null
  const hasOutgoing = next !== undefined && nextPoint !== null && (next.kind === 'LineTo' || next.kind === 'CubicTo')

  if (!hasIncoming && !hasOutgoing) return path

  const makeCurved = (hasIncoming && command.kind !== 'CubicTo') || (hasOutgoing && next!.kind !== 'CubicTo')

  const commands = [...path.commands]
  if (hasIncoming) {
    commands[commandIndex] = makeCurved
      ? command.kind === 'LineTo'
        ? seedCubic(previous!, point)
        : command
      : command.kind === 'CubicTo'
        ? { kind: 'LineTo', value: command.value.to }
        : command
  }
  if (hasOutgoing) {
    commands[commandIndex + 1] = makeCurved
      ? next!.kind === 'LineTo'
        ? seedCubic(point, nextPoint!)
        : next!
      : next!.kind === 'CubicTo'
        ? { kind: 'LineTo', value: next!.value.to }
        : next!
  }
  return { commands }
}

/** One drawable segment of a path — a `LineTo` or `CubicTo` command paired
 * with the anchor it starts from (the previous command's own point), which
 * the command itself doesn't carry. `commandIndex` is this segment's owning
 * command, the same addressing `movePathPoint`/`toggleBezierAt` use.
 * `MoveTo` contributes no segment (it starts one); `Close` contributes none
 * either (no handles to carry a curve, same scope limit `toggleBezierAt`
 * documents) — clicking near a closing edge to add a point isn't supported
 * in this first pass. */
type Segment =
  | { kind: 'line'; commandIndex: number; from: Point; to: Point }
  | { kind: 'cubic'; commandIndex: number; from: Point; control1: Point; control2: Point; to: Point }

function pathSegments(path: Path): Segment[] {
  const segments: Segment[] = []
  let previous: Point | null = null
  for (let i = 0; i < path.commands.length; i++) {
    const command = path.commands[i]
    if (command.kind === 'MoveTo') {
      previous = command.value
    } else if (command.kind === 'LineTo' && previous) {
      segments.push({ kind: 'line', commandIndex: i, from: previous, to: command.value })
      previous = command.value
    } else if (command.kind === 'CubicTo' && previous) {
      segments.push({ kind: 'cubic', commandIndex: i, from: previous, control1: command.value.control1, control2: command.value.control2, to: command.value.to })
      previous = command.value.to
    } else if (command.kind === 'Close') {
      previous = null
    }
  }
  return segments
}

/** The closest point on `segment` to `target`, and the segment-local
 * parameter `t` (`0..=1`) it sits at — exact for a `line` (standard
 * point-to-segment projection), a coarse sampled-then-refined approximation
 * for a `cubic` (good enough to place a new vertex by eye; not meant to be
 * pixel-exact). */
function closestOnSegment(segment: Segment, target: Point): { t: number; point: Point; distance: number } {
  if (segment.kind === 'line') {
    const dx = segment.to.x - segment.from.x
    const dy = segment.to.y - segment.from.y
    const lengthSq = dx * dx + dy * dy
    const t = lengthSq > 0 ? Math.min(1, Math.max(0, ((target.x - segment.from.x) * dx + (target.y - segment.from.y) * dy) / lengthSq)) : 0
    const point = lerpPoint(segment.from, segment.to, t)
    return { t, point, distance: Math.hypot(target.x - point.x, target.y - point.y) }
  }

  const at = (t: number) => cubicPoint(segment.from, segment.control1, segment.control2, segment.to, t)
  const SAMPLES = 24
  let bestT = 0
  let bestDistance = Infinity
  for (let i = 0; i <= SAMPLES; i++) {
    const t = i / SAMPLES
    const distance = Math.hypot(target.x - at(t).x, target.y - at(t).y)
    if (distance < bestDistance) {
      bestT = t
      bestDistance = distance
    }
  }
  // Ternary-search refine around the best sample, narrowing the bracket
  // toward whichever third is closer each pass.
  let lo = Math.max(0, bestT - 1 / SAMPLES)
  let hi = Math.min(1, bestT + 1 / SAMPLES)
  for (let iter = 0; iter < 12; iter++) {
    const m1 = lo + (hi - lo) / 3
    const m2 = hi - (hi - lo) / 3
    const d1 = Math.hypot(target.x - at(m1).x, target.y - at(m1).y)
    const d2 = Math.hypot(target.x - at(m2).x, target.y - at(m2).y)
    if (d1 < d2) hi = m2
    else lo = m1
  }
  const t = (lo + hi) / 2
  const point = at(t)
  return { t, point, distance: Math.hypot(target.x - point.x, target.y - point.y) }
}

/** Splits `segment` at parameter `t`, inserting a new anchor there —
 * clicking on the path between two existing points, the standard pen-tool
 * "add anchor point" action. A `line` splits into two straight `LineTo`s
 * meeting at the click point. A `cubic` splits via De Casteljau subdivision
 * (the standard exact bezier-split construction) into two `CubicTo`s that
 * together retrace the *original* curve exactly — not two fresh curves
 * re-seeded through the click point, which would visibly kink the shape. */
function insertPointOnSegment(path: Path, segment: Segment, t: number): Path {
  const commands = [...path.commands]
  if (segment.kind === 'line') {
    const point = lerpPoint(segment.from, segment.to, t)
    commands.splice(segment.commandIndex, 1, { kind: 'LineTo', value: point }, { kind: 'LineTo', value: segment.to })
    return { commands }
  }

  const { from: p0, control1: p1, control2: p2, to: p3 } = segment
  const a = lerpPoint(p0, p1, t)
  const b = lerpPoint(p1, p2, t)
  const c = lerpPoint(p2, p3, t)
  const d = lerpPoint(a, b, t)
  const e = lerpPoint(b, c, t)
  const f = lerpPoint(d, e, t)
  commands.splice(
    segment.commandIndex,
    1,
    { kind: 'CubicTo', value: { control1: a, control2: d, to: f } },
    { kind: 'CubicTo', value: { control1: e, control2: c, to: p3 } },
  )
  return { commands }
}

/** Removes the anchor at `index` — Paint Asset Mode's Select tool "delete a
 * selected point" (this overlay's own doc comment used to list this as
 * deliberately deferred). Simply splicing the command out is enough for
 * every ordinary interior/last anchor: `pathSegments`/the outline-tracing
 * loop above both walk the command list sequentially and track "the
 * previous point" independently of any specific index, so the segment on
 * either side of the deleted anchor just closes the gap on its own — no
 * relinking needed. The one real special case is deleting the path's own
 * first anchor (`MoveTo`): the command that was second becomes first, but a
 * bare `LineTo`/`CubicTo` can't legally start a path (nothing precedes it to
 * draw *from*), so it's demoted to a plain `MoveTo` at its own endpoint,
 * dropping whatever curve handles it had — same "an author's own edit
 * degrades to a well-defined, simple result" convention used elsewhere in
 * this app rather than trying to re-fit a curve with no source anchor
 * left. Adjacent handles on the *other* side of a deleted interior anchor
 * are deliberately left exactly as authored (not re-fit to the new
 * shorter path) — a deliberate simplification, not an oversight. */
function deleteAnchorAt(path: Path, index: number): Path {
  const commands = [...path.commands]
  commands.splice(index, 1)
  if (commands.length > 0 && commands[0].kind !== 'MoveTo') {
    const point = pointOf(commands[0])
    if (point) commands[0] = { kind: 'MoveTo', value: point }
  }
  return { commands }
}

const ACCENT = '#7a8cff'
const PATH_BLUE = '#3b82f6'
const HANDLE_FILL = 'rgba(18,18,22,0.95)'

/**
 * The path-editing overlay ("editable path shapes") — entered by double-
 * clicking a leaf `Shape` node (one with no children; a node *with*
 * children instead drills into it, see `App.tsx`'s `handleCanvasDoubleClick`).
 * Traces the path itself as a blue outline (`buildSvgPath`), and draws a
 * small hollow circle per `MoveTo`/`LineTo`/`CubicTo` anchor plus a smaller
 * hollow circle per `CubicTo`'s `control1`/`control2` handle, on thin
 * connector lines — the standard pen-tool vocabulary (Illustrator/Figma/
 * Rive all draw exactly this), in this app's own dark/accent palette rather
 * than a literal copy of any one tool's colors. Clicking anywhere on the
 * blue outline that isn't already an anchor/handle inserts a new anchor
 * there (`insertPointOnSegment`), splitting the clicked segment in two —
 * a `line` splits into two straight segments meeting at the click; a
 * `cubic` splits via an exact De Casteljau subdivision, so the curve's
 * actual shape doesn't change, only its command count.
 *
 * Every drag live-previews the edited path (`onPreviewPath`, every
 * `pointermove`) so the actual rendered shape updates in real time, not
 * just this overlay's own handle positions, and commits once
 * (`onCommitPath`, `Command::SetShapePath`) on release — one undo step per
 * gesture, matching every other drag in this app (`commitNodeTranslation`
 * and friends). A dashed bounding-box outline (recomputed from the live
 * path's own control-polygon extents, the same "safe over-approximation"
 * `an1mbyt3_core::Path::bounds` uses) tracks the shape's current bounds
 * live alongside it, so there's still visible bounds feedback while
 * `TransformHandles`' own box is hidden for path-edit mode.
 *
 * Local<->world conversion mirrors `TransformHandles.tsx`'s own
 * `localToWorld` exactly (anchor subtracted *before* scale/rotate, per
 * `Transform2D::to_affine2`'s own formula) — a path's points live in the
 * same raw local space `to_affine2` composes from.
 *
 * Also supports selecting an anchor (click, or the drag that starts a
 * reshape) and deleting it with Delete/Backspace (`deleteAnchorAt`) — Paint
 * Asset Mode's Select tool. Drawing a brand-new path from a blank canvas is
 * a *different* tool (`PenToolOverlay`, a sibling component) — this overlay
 * only ever reshapes/prunes an already-existing path.
 */
export function PathEditOverlay({
  containerRef,
  selectedNode,
  camera,
  canvasSize,
  onPreviewPath,
  onCommitPath,
}: {
  containerRef: RefObject<HTMLDivElement | null>
  selectedNode: NodeSummary | null
  camera: Camera
  canvasSize: CanvasSize
  onPreviewPath: (path: Path) => void
  onCommitPath: (path: Path) => void
}) {
  const [dragging, setDragging] = useState<PointRef | null>(null)
  const [livePath, setLivePath] = useState<Path | null>(null)
  const [selectedAnchor, setSelectedAnchor] = useState<number | null>(null)

  // A selected anchor index is only meaningful for *this* node's own path —
  // reset it whenever path-edit mode switches to a different node (rather
  // than risking a stale index silently applying to the wrong shape's point
  // on the very next Delete keypress).
  useEffect(() => {
    setSelectedAnchor(null)
  }, [selectedNode?.id])

  // Every hook above/below this line must run unconditionally every render
  // (rules of hooks) — `selectedNode?.path` not existing yet is handled by
  // falling back to an empty path here (never actually reaches the JSX
  // below, since the real bail-out happens after all hooks have run), not
  // by returning early before these hooks.
  const path = livePath ?? selectedNode?.path ?? { commands: [] }

  // Re-subscribed whenever `path` itself changes (not just `selectedAnchor`)
  // so a Delete keypress always deletes from the *current* path — e.g. a
  // drag-in-progress elsewhere, or a double-click bezier toggle, between
  // selecting this anchor and pressing Delete — rather than a stale
  // snapshot from whenever an anchor was first selected. `path` changes on
  // every drag-move frame while nothing's selected, but this effect only
  // exists at all while `selectedAnchor !== null`, so the extra
  // add/removeEventListener churn only happens in that narrow window.
  useEffect(() => {
    if (selectedAnchor === null) return
    const handleKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null
      if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) return
      if (e.key !== 'Delete' && e.key !== 'Backspace') return
      e.preventDefault()
      onCommitPath(deleteAnchorAt(path, selectedAnchor))
      setSelectedAnchor(null)
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [selectedAnchor, path, onCommitPath])

  if (!selectedNode?.path) return null
  const { translation, rotation, scale } = selectedNode.worldTransform
  const anchor = selectedNode.transform.anchor
  const cosR = Math.cos(rotation)
  const sinR = Math.sin(rotation)

  const localToWorld = (lx: number, ly: number) => {
    const ax = lx - anchor.x
    const ay = ly - anchor.y
    const scaledX = ax * scale.x
    const scaledY = ay * scale.y
    return { x: translation.x + (scaledX * cosR - scaledY * sinR), y: translation.y + (scaledX * sinR + scaledY * cosR) }
  }
  const worldToLocal = (wx: number, wy: number) => {
    const dx = wx - translation.x
    const dy = wy - translation.y
    const unrotatedX = dx * cosR + dy * sinR
    const unrotatedY = -dx * sinR + dy * cosR
    return { x: unrotatedX / scale.x + anchor.x, y: unrotatedY / scale.y + anchor.y }
  }
  const localToScreen = (lx: number, ly: number) => {
    const world = localToWorld(lx, ly)
    return worldToScreen(world.x, world.y, camera, canvasSize)
  }
  const clientToLocal = (clientX: number, clientY: number) => {
    const rect = containerRef.current?.getBoundingClientRect()
    const world = screenToWorld(clientX - (rect?.left ?? 0), clientY - (rect?.top ?? 0), camera, canvasSize)
    return worldToLocal(world.x, world.y)
  }

  const handlePointerMove = (e: React.PointerEvent) => {
    if (!dragging) return
    const next = movePathPoint(path, dragging, clientToLocal(e.clientX, e.clientY))
    setLivePath(next)
    onPreviewPath(next)
  }
  const handlePointerUp = () => {
    if (livePath) onCommitPath(livePath)
    setDragging(null)
    setLivePath(null)
  }
  const startDrag = (e: React.PointerEvent, ref: PointRef) => {
    e.stopPropagation()
    ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
    setDragging(ref)
    // Only an anchor (not a bezier handle) is a deletable "point" — see
    // `deleteAnchorAt`. Selecting on pointerdown (not a separate click)
    // means the same gesture that starts a reshape drag also selects it,
    // so picking an anchor up to move it and picking it to delete it share
    // one interaction instead of needing a distinct "select" click first.
    if (ref.field === 'point') setSelectedAnchor(ref.commandIndex)
  }
  const handleDoubleClick = (e: React.MouseEvent, commandIndex: number) => {
    e.stopPropagation()
    onCommitPath(toggleBezierAt(path, commandIndex))
  }
  // Clicking the path outline itself (not an anchor/handle — those stop
  // propagation on their own pointerdown, so a click never reaches here
  // mid-drag) adds a new anchor at the closest point on whichever segment
  // was clicked.
  const handleAddPoint = (e: React.MouseEvent) => {
    e.stopPropagation()
    const local = clientToLocal(e.clientX, e.clientY)
    const segments = pathSegments(path)
    let best: { segment: Segment; t: number; distance: number } | null = null
    for (const segment of segments) {
      const result = closestOnSegment(segment, local)
      if (!best || result.distance < best.distance) best = { segment, t: result.t, distance: result.distance }
    }
    if (!best) return
    onCommitPath(insertPointOnSegment(path, best.segment, best.t))
  }

  // A dashed bounding-box outline over the live path's own control-polygon
  // extents (every anchor plus every control point) — the same
  // over-approximation `an1mbyt3_core::Path::bounds` uses, just computed
  // here in screen space so it tracks the live drag with no round-trip.
  const allPoints: { x: number; y: number }[] = []
  for (const command of path.commands) {
    if (command.kind === 'MoveTo' || command.kind === 'LineTo') allPoints.push(command.value)
    else if (command.kind === 'CubicTo') allPoints.push(command.value.control1, command.value.control2, command.value.to)
  }
  const boundsScreen = allPoints.length
    ? allPoints.map((p) => localToScreen(p.x, p.y)).reduce(
        (box, p) => ({ minX: Math.min(box.minX, p.x), minY: Math.min(box.minY, p.y), maxX: Math.max(box.maxX, p.x), maxY: Math.max(box.maxY, p.y) }),
        { minX: Infinity, minY: Infinity, maxX: -Infinity, maxY: -Infinity },
      )
    : null

  // The path itself, traced as a blue SVG outline — `M`/`L`/`C`/`Z` built
  // straight from the command list in screen space, so it always matches
  // what's actually being edited (including mid-drag).
  let outline = ''
  for (const command of path.commands) {
    if (command.kind === 'MoveTo') {
      const p = localToScreen(command.value.x, command.value.y)
      outline += `M ${p.x} ${p.y} `
    } else if (command.kind === 'LineTo') {
      const p = localToScreen(command.value.x, command.value.y)
      outline += `L ${p.x} ${p.y} `
    } else if (command.kind === 'CubicTo') {
      const c1 = localToScreen(command.value.control1.x, command.value.control1.y)
      const c2 = localToScreen(command.value.control2.x, command.value.control2.y)
      const to = localToScreen(command.value.to.x, command.value.to.y)
      outline += `C ${c1.x} ${c1.y} ${c2.x} ${c2.y} ${to.x} ${to.y} `
    } else {
      outline += 'Z '
    }
  }

  return (
    <div onPointerMove={handlePointerMove} onPointerUp={handlePointerUp} className="pointer-events-none absolute inset-0">
      <svg className="absolute inset-0 h-full w-full overflow-visible">
        {outline && (
          <>
            <path d={outline} fill="none" stroke={PATH_BLUE} strokeWidth={1.5} />
            {/* Invisible, much wider hit target for "click to add a point" —
                the visible line above is too thin to click reliably. */}
            <path
              d={outline}
              fill="none"
              stroke="transparent"
              strokeWidth={14}
              className="pointer-events-auto cursor-copy"
              onClick={handleAddPoint}
            >
              <title>Click to add a point</title>
            </path>
          </>
        )}
        {boundsScreen && (
          <rect
            x={boundsScreen.minX}
            y={boundsScreen.minY}
            width={boundsScreen.maxX - boundsScreen.minX}
            height={boundsScreen.maxY - boundsScreen.minY}
            fill="none"
            stroke="rgba(255,255,255,0.35)"
            strokeDasharray="3 3"
          />
        )}
        {path.commands.map((command, index) => {
          const point = pointOf(command)
          if (!point) return null
          const p = localToScreen(point.x, point.y)
          const isDraggingPoint = dragging?.commandIndex === index && dragging.field === 'point'
          const isSelected = selectedAnchor === index

          const anchorPoint = (
            <circle
              key="anchor"
              cx={p.x}
              cy={p.y}
              r={isSelected ? 5.5 : 4.5}
              fill={isDraggingPoint || isSelected ? ACCENT : HANDLE_FILL}
              stroke={ACCENT}
              strokeWidth={isSelected ? 2 : 1.25}
              className="pointer-events-auto cursor-grab active:cursor-grabbing"
              onPointerDown={(e) => startDrag(e, { commandIndex: index, field: 'point' })}
              onDoubleClick={(e) => handleDoubleClick(e, index)}
            />
          )

          if (command.kind !== 'CubicTo') {
            return (
              <g key={index}>
                {command.kind === 'LineTo' && (
                  <title>Double-click to curve this segment</title>
                )}
                {anchorPoint}
              </g>
            )
          }

          const c1 = localToScreen(command.value.control1.x, command.value.control1.y)
          const c2 = localToScreen(command.value.control2.x, command.value.control2.y)
          const isDraggingC1 = dragging?.commandIndex === index && dragging.field === 'control1'
          const isDraggingC2 = dragging?.commandIndex === index && dragging.field === 'control2'
          return (
            <g key={index}>
              <line x1={c1.x} y1={c1.y} x2={p.x} y2={p.y} stroke="rgba(122,140,255,0.45)" strokeWidth={1} />
              <line x1={c2.x} y1={c2.y} x2={p.x} y2={p.y} stroke="rgba(122,140,255,0.45)" strokeWidth={1} />
              <circle
                cx={c1.x}
                cy={c1.y}
                r={3.5}
                fill={isDraggingC1 ? ACCENT : HANDLE_FILL}
                stroke={ACCENT}
                strokeWidth={1.25}
                className="pointer-events-auto cursor-grab active:cursor-grabbing"
                onPointerDown={(e) => startDrag(e, { commandIndex: index, field: 'control1' })}
              />
              <circle
                cx={c2.x}
                cy={c2.y}
                r={3.5}
                fill={isDraggingC2 ? ACCENT : HANDLE_FILL}
                stroke={ACCENT}
                strokeWidth={1.25}
                className="pointer-events-auto cursor-grab active:cursor-grabbing"
                onPointerDown={(e) => startDrag(e, { commandIndex: index, field: 'control2' })}
              />
              {anchorPoint}
              <title>Double-click to straighten this segment</title>
            </g>
          )
        })}
      </svg>
    </div>
  )
}
