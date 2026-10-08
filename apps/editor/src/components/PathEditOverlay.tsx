import { useEffect, useRef, useState } from 'react'
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

const EPSILON = 1e-3

function samePoint(a: Point | null, b: Point | null): boolean {
  return !!a && !!b && Math.abs(a.x - b.x) < EPSILON && Math.abs(a.y - b.y) < EPSILON
}

/** The subpath containing command `index`: its `MoveTo` index, the index of
 * its `Close` (`-1` if it's open), and its last point-carrying command. */
function subpathAt(path: Path, index: number): { move: number; close: number; last: number } {
  let move = index
  while (move > 0 && path.commands[move].kind !== 'MoveTo') move--
  let end = move + 1
  while (end < path.commands.length && path.commands[end].kind !== 'MoveTo' && path.commands[end].kind !== 'Close') end++
  const close = path.commands[end]?.kind === 'Close' ? end : -1
  return { move, close, last: end - 1 }
}

/** A closed subpath whose last explicit point sits exactly on its start (how
 * SVG/scratch-paint close a curved shape: `... C ... start Z`). That start
 * is *one* vertex drawn twice — the `MoveTo` and the last command — so the
 * editor shows, drags and curves them as one. Returns the last command's
 * index, or `-1`. */
function coincidentEnd(path: Path, moveIndex: number): number {
  const { move, close, last } = subpathAt(path, moveIndex)
  if (close < 0 || last <= move) return -1
  return samePoint(pointOf(path.commands[move]), pointOf(path.commands[last])) ? last : -1
}

/** The anchor command `index` starts from (its segment's first point). */
function startPointOf(path: Path, index: number): Point | null {
  for (let i = index - 1; i >= 0; i--) {
    const command = path.commands[i]
    if (command.kind === 'Close') return null
    const point = pointOf(command)
    if (point) return point
  }
  return null
}

/** Moves one draggable point/handle to `next`, returning the whole updated
 * `Path`. Moving an anchor also shifts the handles on either side of it (its
 * own command's `control2` and the next command's `control1`) by the same
 * delta, so they follow it — the "drag a vertex, its handles come with it"
 * behaviour of every vector tool (Illustrator, Figma, Rive). A closed
 * shape's start/end vertex (`coincidentEnd`) moves as one point. Dragging a
 * handle only ever moves that one handle. */
/** The other handle at the same vertex as handle `ref` — the one on the
 * neighbouring segment — and that vertex. `null` if the vertex has no
 * second handle (a corner next to a straight segment, an open end). */
function linkedHandle(path: Path, ref: PointRef): { vertex: Point; ref: PointRef; point: Point } | null {
  const handleOf = (index: number, field: 'control1' | 'control2'): { ref: PointRef; point: Point } | null => {
    const c = path.commands[index]
    return c?.kind === 'CubicTo' ? { ref: { commandIndex: index, field }, point: c.value[field] } : null
  }
  if (ref.field === 'control1') {
    const vertex = startPointOf(path, ref.commandIndex)
    if (!vertex) return null
    const before = path.commands[ref.commandIndex - 1]
    const other = before?.kind === 'MoveTo' ? handleOf(coincidentEnd(path, ref.commandIndex - 1), 'control2') : handleOf(ref.commandIndex - 1, 'control2')
    return other && { vertex, ...other }
  }
  if (ref.field === 'control2') {
    const command = path.commands[ref.commandIndex]
    if (command?.kind !== 'CubicTo') return null
    const { move } = subpathAt(path, ref.commandIndex)
    const other = path.commands[ref.commandIndex + 1]?.kind === 'CubicTo' ? handleOf(ref.commandIndex + 1, 'control1') : coincidentEnd(path, move) === ref.commandIndex ? handleOf(move + 1, 'control1') : null
    return other && { vertex: command.value.to, ...other }
  }
  return null
}

function movePathPoint(path: Path, ref: PointRef, next: Point, breakTangent = false): Path {
  const commands = [...path.commands]
  const command = commands[ref.commandIndex]
  if (!command) return path

  if (ref.field !== 'point') {
    if (command.kind !== 'CubicTo') return path
    const previous = command.value[ref.field]
    commands[ref.commandIndex] = { kind: 'CubicTo', value: { ...command.value, [ref.field]: next } }
    // A *smooth* point's two handles stay on one line through it (each
    // keeping its own length): dragging one swings the other around, the way
    // every pen tool behaves — moving only one kinked the curve (and its
    // outline) at the point. A corner's handles stay independent; hold Alt
    // to break a smooth point into a corner while dragging.
    const linked = breakTangent ? null : linkedHandle(path, ref)
    if (linked) {
      const ax = previous.x - linked.vertex.x
      const ay = previous.y - linked.vertex.y
      const bx = linked.point.x - linked.vertex.x
      const by = linked.point.y - linked.vertex.y
      const la = Math.hypot(ax, ay)
      const lb = Math.hypot(bx, by)
      const nx = next.x - linked.vertex.x
      const ny = next.y - linked.vertex.y
      const ln = Math.hypot(nx, ny)
      if (la > EPSILON && lb > EPSILON && ln > EPSILON && (ax * bx + ay * by) / (la * lb) < -0.995) {
        const other = commands[linked.ref.commandIndex]
        if (other.kind === 'CubicTo') {
          commands[linked.ref.commandIndex] = {
            kind: 'CubicTo',
            value: { ...other.value, [linked.ref.field]: { x: linked.vertex.x - (nx / ln) * lb, y: linked.vertex.y - (ny / ln) * lb } },
          }
        }
      }
    }
    return { commands }
  }

  const previous = pointOf(command)
  if (!previous) return path
  const dx = next.x - previous.x
  const dy = next.y - previous.y
  const shiftAnchor = (index: number) => {
    const c = commands[index]
    if (c.kind === 'CubicTo') commands[index] = { kind: 'CubicTo', value: { ...c.value, to: { x: c.value.to.x + dx, y: c.value.to.y + dy }, control2: { x: c.value.control2.x + dx, y: c.value.control2.y + dy } } }
    else if (c.kind === 'LineTo') commands[index] = { kind: 'LineTo', value: { x: c.value.x + dx, y: c.value.y + dy } }
    else if (c.kind === 'MoveTo') commands[index] = { kind: 'MoveTo', value: { x: c.value.x + dx, y: c.value.y + dy } }
    const after = commands[index + 1]
    if (after?.kind === 'CubicTo') commands[index + 1] = { kind: 'CubicTo', value: { ...after.value, control1: { x: after.value.control1.x + dx, y: after.value.control1.y + dy } } }
  }
  shiftAnchor(ref.commandIndex)
  if (command.kind === 'MoveTo') {
    const end = coincidentEnd(path, ref.commandIndex)
    if (end >= 0) shiftAnchor(end)
  }
  return { commands }
}

/** If the subpath around vertex `index` is closed by an *implicit* straight
 * edge (`Close` back to a start the last point isn't on — a rectangle's
 * fourth side), makes that edge an explicit `LineTo(start)` so it can be
 * split or curved like any other. Indices before the `Close` don't move. */
function materializeClosingEdge(path: Path, index: number): Path {
  const { move, close, last } = subpathAt(path, index)
  if (close < 0 || last <= move) return path
  const start = pointOf(path.commands[move])
  if (!start || samePoint(start, pointOf(path.commands[last]))) return path
  const commands = [...path.commands]
  commands.splice(close, 0, { kind: 'LineTo', value: start })
  return { commands }
}

/** Is the cubic's `handle` effectively on its own chord (so the segment is
 * still visually straight there)? */
function onChord(from: Point, to: Point, handle: Point): boolean {
  const dx = to.x - from.x
  const dy = to.y - from.y
  const length = Math.hypot(dx, dy)
  if (length < EPSILON) return samePoint(handle, from)
  return Math.abs(((handle.x - from.x) * dy - (handle.y - from.y) * dx) / length) < 0.01 * length + EPSILON
}

/** Double-clicking an anchor toggles it between a **corner** and a **smooth**
 * point — the pen tool's "convert anchor point":
 *
 * - corner → smooth: both handles at the vertex are pulled out along the
 *   line through its two neighbours (mirrored, each a third of its own
 *   segment's length), so the shape immediately rounds off there — the
 *   Illustrator/Figma behaviour. Straight segments touching the vertex become
 *   curves (their far ends stay corners).
 * - smooth → corner: both handles at the vertex retract onto it; a segment
 *   left with no real curve at all becomes a straight `LineTo` again.
 *
 * Works at a closed shape's start vertex too (its incoming segment is the
 * closing edge, made explicit first). */
function toggleBezierAt(input: Path, vertexIndex: number): Path {
  // Only the start and last vertices touch the closing edge.
  const around = subpathAt(input, vertexIndex)
  let path = vertexIndex === around.move || vertexIndex === around.last ? materializeClosingEdge(input, vertexIndex) : input
  const vertexCommand = path.commands[vertexIndex]
  const vertex = vertexCommand ? pointOf(vertexCommand) : null
  if (!vertexCommand || !vertex) return input

  // The segment arriving at the vertex, and the one leaving it.
  const { move } = subpathAt(path, vertexIndex)
  let incoming = vertexCommand.kind === 'LineTo' || vertexCommand.kind === 'CubicTo' ? vertexIndex : -1
  let outgoing = ['LineTo', 'CubicTo'].includes(path.commands[vertexIndex + 1]?.kind ?? '') ? vertexIndex + 1 : -1
  const end = coincidentEnd(path, move)
  if (vertexIndex === move && end >= 0) incoming = end
  if (vertexIndex === end) outgoing = ['LineTo', 'CubicTo'].includes(path.commands[move + 1]?.kind ?? '') ? move + 1 : -1
  if (incoming < 0 && outgoing < 0) return input

  const from = incoming >= 0 ? startPointOf(path, incoming) : null
  const nextCommand = outgoing >= 0 ? path.commands[outgoing] : null
  const to = nextCommand ? pointOf(nextCommand) : null
  const handleIn = incoming >= 0 && path.commands[incoming].kind === 'CubicTo' ? (path.commands[incoming] as Extract<PathCommand, { kind: 'CubicTo' }>).value.control2 : null
  const handleOut = nextCommand?.kind === 'CubicTo' ? nextCommand.value.control1 : null
  const smooth = (handleIn && !samePoint(handleIn, vertex)) || (handleOut && !samePoint(handleOut, vertex))

  const commands = [...path.commands]
  const asCubic = (index: number, segmentFrom: Point): Extract<PathCommand, { kind: 'CubicTo' }>['value'] => {
    const c = commands[index]
    if (c.kind === 'CubicTo') return { ...c.value }
    const target = pointOf(c)!
    // A straight segment as a cubic with both handles on its own ends.
    return { control1: segmentFrom, control2: target, to: target }
  }

  if (smooth) {
    // → corner.
    if (incoming >= 0 && from && commands[incoming].kind === 'CubicTo') {
      const value = { ...asCubic(incoming, from), control2: vertex }
      commands[incoming] = samePoint(value.control1, from) || onChord(from, vertex, value.control1) ? { kind: 'LineTo', value: vertex } : { kind: 'CubicTo', value }
    }
    if (outgoing >= 0 && to && commands[outgoing].kind === 'CubicTo') {
      const value = { ...asCubic(outgoing, vertex), control1: vertex }
      commands[outgoing] = samePoint(value.control2, to) || onChord(vertex, to, value.control2) ? { kind: 'LineTo', value: to } : { kind: 'CubicTo', value }
    }
  } else {
    // → smooth: tangent along the neighbours (or the one neighbour there is).
    const a = from ?? vertex
    const b = to ?? vertex
    let tx = b.x - a.x
    let ty = b.y - a.y
    const length = Math.hypot(tx, ty)
    if (length < EPSILON) return input
    tx /= length
    ty /= length
    if (incoming >= 0 && from) {
      const reach = Math.hypot(vertex.x - from.x, vertex.y - from.y) / 3
      commands[incoming] = { kind: 'CubicTo', value: { ...asCubic(incoming, from), control2: { x: vertex.x - tx * reach, y: vertex.y - ty * reach } } }
    }
    if (outgoing >= 0 && to) {
      const reach = Math.hypot(to.x - vertex.x, to.y - vertex.y) / 3
      commands[outgoing] = { kind: 'CubicTo', value: { ...asCubic(outgoing, vertex), control1: { x: vertex.x + tx * reach, y: vertex.y + ty * reach } } }
    }
  }
  path = { commands }
  return path
}

/** One drawable segment of a path — a `LineTo` or `CubicTo` command paired
 * with the anchor it starts from (the previous command's own point), which
 * the command itself doesn't carry. `commandIndex` is this segment's owning
 * command, the same addressing `movePathPoint`/`toggleBezierAt` use.
 * `MoveTo` contributes no segment (it starts one). A `Close` whose last
 * point isn't already on the start contributes the implicit straight
 * closing edge (`close`) — a rectangle's fourth side is as clickable as the
 * other three (it used to be skipped, so a click there inserted the new
 * point on whichever *other* side was nearest). */
type Segment =
  | { kind: 'line'; commandIndex: number; from: Point; to: Point }
  | { kind: 'close'; commandIndex: number; from: Point; to: Point }
  | { kind: 'cubic'; commandIndex: number; from: Point; control1: Point; control2: Point; to: Point }

function pathSegments(path: Path): Segment[] {
  const segments: Segment[] = []
  let previous: Point | null = null
  let start: Point | null = null
  for (let i = 0; i < path.commands.length; i++) {
    const command = path.commands[i]
    if (command.kind === 'MoveTo') {
      previous = command.value
      start = command.value
    } else if (command.kind === 'LineTo' && previous) {
      segments.push({ kind: 'line', commandIndex: i, from: previous, to: command.value })
      previous = command.value
    } else if (command.kind === 'CubicTo' && previous) {
      segments.push({ kind: 'cubic', commandIndex: i, from: previous, control1: command.value.control1, control2: command.value.control2, to: command.value.to })
      previous = command.value.to
    } else if (command.kind === 'Close') {
      if (previous && start && !samePoint(previous, start)) segments.push({ kind: 'close', commandIndex: i, from: previous, to: start })
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
  if (segment.kind === 'line' || segment.kind === 'close') {
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
  if (segment.kind === 'close') {
    // The new vertex goes just before the `Close`; the closing edge then
    // runs from it back to the start.
    commands.splice(segment.commandIndex, 0, { kind: 'LineTo', value: lerpPoint(segment.from, segment.to, t) })
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
  const end = path.commands[index]?.kind === 'MoveTo' ? coincidentEnd(path, index) : -1
  if (end >= 0) commands.splice(end, 1)
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
  // This overlay's own root (it fills the canvas): what cursor positions are
  // measured against. The shared `containerRef` is `TransformHandles`'
  // element, which is *unmounted* while points are edited — it was null, so
  // nothing was subtracted and every drag landed offset by the canvas's own
  // position (the docked Layers/toolbar width): a handle jumped hundreds of
  // pixels the moment it was pressed.
  const rootRef = useRef<HTMLDivElement>(null)

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
    const rect = (rootRef.current ?? containerRef.current)?.getBoundingClientRect()
    const world = screenToWorld(clientX - (rect?.left ?? 0), clientY - (rect?.top ?? 0), camera, canvasSize)
    return worldToLocal(world.x, world.y)
  }

  const handlePointerMove = (e: React.PointerEvent) => {
    if (!dragging) return
    const next = movePathPoint(path, dragging, clientToLocal(e.clientX, e.clientY), e.altKey)
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
    <div ref={rootRef} onPointerMove={handlePointerMove} onPointerUp={handlePointerUp} className="pointer-events-none absolute inset-0">
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
          // A closed shape's last point that duplicates its start is the
          // same vertex as the `MoveTo` — drawn (and dragged) once, there.
          const duplicateEnd = command.kind !== 'MoveTo' && coincidentEnd(path, subpathAt(path, index).move) === index
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
            if (duplicateEnd) return null
            return (
              <g key={index}>
                {command.kind === 'LineTo' && (
                  <title>Double-click a point to switch it between smooth and corner</title>
                )}
                {anchorPoint}
              </g>
            )
          }

          const c1 = localToScreen(command.value.control1.x, command.value.control1.y)
          const c2 = localToScreen(command.value.control2.x, command.value.control2.y)
          // `control1` is the handle of the segment's *start* anchor (the
          // previous point), `control2` of its end anchor — each line goes to
          // its own anchor (both used to go to the end, which made every
          // curve's first handle look attached to the wrong point).
          const startAnchor = startPointOf(path, index)
          const s0 = startAnchor ? localToScreen(startAnchor.x, startAnchor.y) : p
          const isDraggingC1 = dragging?.commandIndex === index && dragging.field === 'control1'
          const isDraggingC2 = dragging?.commandIndex === index && dragging.field === 'control2'
          return (
            <g key={index}>
              <line x1={c1.x} y1={c1.y} x2={s0.x} y2={s0.y} stroke="rgba(122,140,255,0.45)" strokeWidth={1} />
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
              {!duplicateEnd && anchorPoint}
              <title>Double-click a point to switch it between smooth and corner</title>
            </g>
          )
        })}
      </svg>
    </div>
  )
}
