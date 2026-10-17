import { useEffect, useRef, useState } from 'react'
import type { MouseEvent as ReactMouseEvent, RefObject } from 'react'
import { screenToWorld, worldToScreen, type Camera, type CanvasSize } from '../camera'
import type { NodeSummary, Transform2D } from '../ipc/document'
import { snapAngleDegrees } from '../snapping'

/** The inverse of `Transform2D::compose` (`an1mbyt3-core`'s `transform.rs`)
 * — given `parent`'s own world transform and a node's *desired* world
 * transform, returns the local transform that node needs under `parent` for
 * its world transform to work out to exactly `world`. A faithful TS port of
 * `Transform2D::local_for` (kept in exact lock-step with that Rust formula,
 * not reimplemented independently) since this is what every rotate/scale
 * drag below needs to convert its result — computed against `world_transform`
 * so the on-canvas handles track the shape's *actual* rendered position —
 * back into the local value `Command::SetRotation`/`SetScale`/`SetTranslation`
 * actually store. Skipping this (writing the drag's world-space result
 * straight through as if it were already local) was a real bug: rotating a
 * nested node would silently double-count its parent's own rotation, and
 * scaling one would teleport it by the parent's translation on release.
 * `parent: null` (a root node — nothing to convert through) returns `world`
 * unchanged, matching `local_for`'s own identity-parent behavior. */
/** The parts of a transform `localFor` converts (the anchor isn't affected
 * by re-parenting, so callers don't need to supply one). */
type Trs = Pick<Transform2D, 'translation' | 'rotation' | 'scale'>

function localFor(parent: Trs | null, world: Trs): Trs {
  if (!parent) return world
  const cos = Math.cos(parent.rotation)
  const sin = Math.sin(parent.rotation)
  const dx = world.translation.x - parent.translation.x
  const dy = world.translation.y - parent.translation.y
  return {
    translation: {
      x: (dx * cos + dy * sin) / parent.scale.x,
      y: (-dx * sin + dy * cos) / parent.scale.y,
    },
    rotation: world.rotation - parent.rotation,
    scale: { x: world.scale.x / parent.scale.x, y: world.scale.y / parent.scale.y },
  }
}

interface TransformHandlesProps {
  /** The overlay's own container — must be positioned identically to (and
   * layered directly above) the canvas element, so `getBoundingClientRect()`
   * gives the same canvas-pixel origin `App.tsx`'s own pointer handlers use. */
  containerRef: RefObject<HTMLDivElement | null>
  selectedNode: NodeSummary | null
  camera: Camera
  canvasSize: CanvasSize
  onPreviewRotation: (degrees: number) => void
  /** `compensateChildren`: the Alt-held "rotate this node only, not its
   * children" gesture — read from the drag's final `mouseup` event (not
   * tracked live mid-drag, so a child's on-canvas position still visibly
   * follows the parent while the drag is in progress and only snaps to its
   * compensated position once the drag commits — a known, accepted
   * simplification rather than plumbing a second live-preview path through
   * every descendant just for this). */
  onCommitRotation: (originDegrees: number, degrees: number, compensateChildren: boolean) => void
  onPreviewScale: (x: number, y: number, scaleX: number, scaleY: number) => void
  onCommitScale: (
    originX: number,
    originY: number,
    originScaleX: number,
    originScaleY: number,
    x: number,
    y: number,
    scaleX: number,
    scaleY: number,
    compensateChildren: boolean,
  ) => void
  onPreviewAnchor: (anchorX: number, anchorY: number, x: number, y: number) => void
  onCommitAnchor: (
    originAnchorX: number,
    originAnchorY: number,
    originX: number,
    originY: number,
    anchorX: number,
    anchorY: number,
    x: number,
    y: number,
  ) => void
}

const HANDLE_SIZE_PX = 9
const ROTATE_HANDLE_DISTANCE_PX = 28
const ROTATE_HANDLE_RADIUS_PX = 6
const ANCHOR_HANDLE_RADIUS_PX = 5
/** Floor on a scale component's magnitude while dragging a corner handle —
 * without this, dragging a corner across its opposite corner (or right up
 * to it) would drive that axis's scale through zero, which every downstream
 * consumer of `Transform2D::to_affine2` (rendering, `pick_node_at`'s
 * inverse-transform hit test) assumes never happens. */
const MIN_SCALE_MAGNITUDE = 0.02

/** A rotate/scale drag's live preview transform, for re-rendering the
 * outline/handles themselves *while* the drag is in progress — without
 * this, the overlay would stay frozen at its pre-drag pose until the drag
 * ends (the `onPreview*` calls it triggers mutate `WasmEditor`'s document
 * directly, which only reaches this component's `selectedNode` prop once
 * the *next* `commit*` call lands, i.e. on mouseup). A plain translate-drag
 * has the same issue but is tracked in `App.tsx` instead, since that's where
 * that drag itself lives (`dragRef`) — see its `dragTranslation` state. */
interface LiveOverride {
  translation: { x: number; y: number }
  rotation: number
  scale: { x: number; y: number }
  /** Only set during an anchor drag — every other drag leaves this `undefined`
   * and `localToWorld` falls back to `selectedNode.transform.anchor`, since
   * only the anchor tool itself ever changes it mid-drag. */
  anchor?: { x: number; y: number }
}

interface RotateDrag {
  center: { x: number; y: number }
  initialAngleRad: number
  originDegrees: number
  lastDegrees: number
  /** Frozen at drag start so the live-preview override below never needs to
   * re-read the (possibly stale, closed-over) `selectedNode` prop from
   * inside the mount-only mousemove listener — a rotate drag never changes
   * scale, but the override still needs *some* scale value to render with. */
  scale: { x: number; y: number }
  /** Frozen at drag start (a rotate drag never reparents anything mid-drag)
   * — see `localFor`'s doc comment for why `center`/`originDegrees`/
   * `lastDegrees` (all *world*-space, since they derive from
   * `selectedNode.worldTransform`) need this to convert back into the local
   * degrees `onPreviewRotation`/`onCommitRotation` actually expect. */
  parentWorldTransform: Transform2D | null
}

interface ScaleDrag {
  /** The world-space point of the corner *opposite* the one being dragged —
   * fixed for the whole drag (matches Figma/Illustrator's default,
   * unmodified corner-drag: the opposite corner stays put in world space
   * while the dragged corner follows the cursor). */
  anchorWorld: { x: number; y: number }
  /** That same opposite corner, in local (pre-transform) space. */
  anchorLocal: { x: number; y: number }
  /** Local-space width/height between the dragged and anchor corners —
   * constant through the drag, used to turn a local-space cursor offset
   * into a scale factor. */
  localSpan: { x: number; y: number }
  /** The node's rotation, frozen at drag start — a scale drag never touches
   * rotation, so this is read once rather than re-read from the (stale,
   * closed-over) `selectedNode` prop every mousemove. */
  rotationRad: number
  /** The node's own `Transform2D.anchor` (its rotate/scale pivot point in
   * local space — see `localToWorld`'s own doc comment), frozen at drag
   * start. Needed so the mousemove handler's `newTranslation` reconstruction
   * subtracts it before scaling, exactly like `localToWorld` itself does —
   * without this, a node whose anchor sits away from local `(0, 0)` (an
   * off-center rotate pivot) would drift instead of correctly pinning the
   * opposite corner/edge while its scale changes. A scale drag never moves
   * the anchor itself, so freezing it here is safe. */
  anchor: { x: number; y: number }
  originTranslation: { x: number; y: number }
  originScale: { x: number; y: number }
  lastTranslation: { x: number; y: number }
  lastScale: { x: number; y: number }
  /** Frozen at drag start (a scale drag never reparents anything mid-drag)
   * — same `localFor`-conversion need `RotateDrag`'s own field doc comment
   * describes, for `originTranslation`/`originScale`/`lastTranslation`/
   * `lastScale` (all *world*-space here too). */
  parentWorldTransform: Transform2D | null
}

interface AnchorDrag {
  /** The node's own *local* rotation/scale (`selectedNode.transform`, not
   * `.worldTransform`) — frozen at drag start. Anchor only relates to
   * translation through the node's own affine map (`Transform2D::to_affine2`'s
   * doc comment has the exact invariant), which uses the node's own
   * rotation/scale, not whatever its ancestors compose on top — using the
   * *world*-composed rotation/scale here instead would silently produce the
   * wrong anchor value for any node nested under a rotated/scaled parent. */
  rotationRad: number
  scale: { x: number; y: number }
  originAnchor: { x: number; y: number }
  /** Local (parent-relative) translation, i.e. `selectedNode.transform.translation`
   * — *not* world. Used only for the exact value sent to Rust. */
  originTranslation: { x: number; y: number }
  /** The same node, but its *world*-composed translation (`translation` in
   * this component's render scope) at drag start — used only by the live
   * overlay's approximate preview, alongside `worldRotationRad`/`worldScale`
   * (see that field's own doc comment for why a separate, frozen copy). */
  originTranslationWorld: { x: number; y: number }
  lastAnchor: { x: number; y: number }
  lastTranslation: { x: number; y: number }
  /** Converts the drag's world-space mouse position into the parent-local
   * space `translation` is expressed in — same `localFor` conversion every
   * other handle here already needs. */
  parentWorldTransform: Transform2D | null
  /** The world-*composed* rotation/scale (`rotation`/`nodeScale` in this
   * component's render scope) at drag start — frozen here for the same
   * "the mousemove listener below is a mount-only effect closure, it can't
   * see a fresh render's values" reason `RotateDrag.scale`'s own doc
   * comment gives. Used only for the live-overlay preview's approximate
   * anchor (see the mousemove handler), never for the value sent to Rust. */
  worldRotationRad: number
  worldScale: { x: number; y: number }
}

/**
 * On-canvas rotate/scale transform tool for the current selection — a
 * bounding-box outline plus four corner scale handles and one rotate handle,
 * drawn as a plain absolutely-positioned overlay (not part of the wgpu
 * canvas) since this is UI chrome, not scene content, the same reasoning
 * `DockablePanel`/`TimelinePanel` never touch the renderer either.
 *
 * Handle sizes stay constant screen pixels regardless of `camera.zoom` (the
 * bounding box itself moves/scales with pan/zoom, correctly reflecting the
 * node's actual position/rotation/scale) — matching Figma/Illustrator's
 * convention that selection chrome never shrinks or grows as you zoom.
 *
 * Drag math lives here rather than in `App.tsx` (which only ever handles
 * translate-drag and camera pan/zoom) — see `handleCornerMouseDown`'s doc
 * comment for the anchor-corner derivation, and `handleRotateMouseDown`'s
 * for why rotation is computed entirely in world space.
 */
export function TransformHandles({
  containerRef,
  selectedNode,
  camera,
  canvasSize,
  onPreviewRotation,
  onCommitRotation,
  onPreviewScale,
  onCommitScale,
  onPreviewAnchor,
  onCommitAnchor,
}: TransformHandlesProps) {
  const rotateDragRef = useRef<RotateDrag | null>(null)
  const scaleDragRef = useRef<ScaleDrag | null>(null)
  const anchorDragRef = useRef<AnchorDrag | null>(null)
  const [liveOverride, setLiveOverride] = useState<LiveOverride | null>(null)
  // Mirrors `App.tsx`'s `cameraRef`/`canvasSizeRef` pattern: synced every
  // render so the mount-only window listener below always reads current
  // values without needing to re-subscribe (and without a `selectedNode`
  // dependency, which would tear down mid-drag every time a preview call
  // changes the very prop this effect depends on).
  const propsRef = useRef({ camera, canvasSize, onPreviewRotation, onCommitRotation, onPreviewScale, onCommitScale, onPreviewAnchor, onCommitAnchor })
  propsRef.current = { camera, canvasSize, onPreviewRotation, onCommitRotation, onPreviewScale, onCommitScale, onPreviewAnchor, onCommitAnchor }

  useEffect(() => {
    function toWorld(clientX: number, clientY: number) {
      const rect = containerRef.current!.getBoundingClientRect()
      return screenToWorld(clientX - rect.left, clientY - rect.top, propsRef.current.camera, propsRef.current.canvasSize)
    }

    function handleWindowMouseMove(event: MouseEvent) {
      const rotate = rotateDragRef.current
      if (rotate) {
        const mouseWorld = toWorld(event.clientX, event.clientY)
        const angle = Math.atan2(mouseWorld.y - rotate.center.y, mouseWorld.x - rotate.center.x)
        const rawDegrees = rotate.originDegrees + (angle - rotate.initialAngleRad) * (180 / Math.PI)
        const degrees = snapAngleDegrees(rawDegrees)
        rotate.lastDegrees = degrees
        setLiveOverride({ translation: rotate.center, rotation: degrees * (Math.PI / 180), scale: rotate.scale })
        // `degrees` is world-space (derived from `rotate.center`, itself
        // `selectedNode.worldTransform`'s translation) — `onPreviewRotation`
        // expects the *local* value `preview_node_rotation` actually writes,
        // so `localFor` converts it through `rotate.parentWorldTransform`
        // (identity/no-op for a root node) before it goes out. Translation
        // and scale are unaffected by a pure rotation, so they're just
        // carried through unchanged here.
        const localDegrees =
          localFor(rotate.parentWorldTransform, { translation: rotate.center, rotation: degrees * (Math.PI / 180), scale: rotate.scale }).rotation * (180 / Math.PI)
        propsRef.current.onPreviewRotation(localDegrees)
        return
      }

      const scale = scaleDragRef.current
      if (scale) {
        const mouseWorld = toWorld(event.clientX, event.clientY)
        const fromAnchor = { x: mouseWorld.x - scale.anchorWorld.x, y: mouseWorld.y - scale.anchorWorld.y }
        // Un-rotate the world-space vector from the anchor to the cursor
        // into the node's local frame, so the projected distance along each
        // local axis directly gives that axis's new scale factor.
        const cos = Math.cos(-scale.rotationRad)
        const sin = Math.sin(-scale.rotationRad)
        const localDelta = { x: fromAnchor.x * cos - fromAnchor.y * sin, y: fromAnchor.x * sin + fromAnchor.y * cos }

        const clamp = (v: number) => (Math.abs(v) < MIN_SCALE_MAGNITUDE ? Math.sign(v || 1) * MIN_SCALE_MAGNITUDE : v)
        const newScale = {
          x: Math.abs(scale.localSpan.x) > 1e-3 ? clamp(localDelta.x / scale.localSpan.x) : scale.originScale.x,
          y: Math.abs(scale.localSpan.y) > 1e-3 ? clamp(localDelta.y / scale.localSpan.y) : scale.originScale.y,
        }

        // Shift locks the drag to the shape's *original* aspect ratio —
        // whichever axis moved further (in relative terms) from its own
        // origin scale drives the other axis to match, rather than each
        // axis just following the cursor independently. This is also what
        // makes an edge handle (a single-axis stretch — see
        // `startScaleDrag`'s doc comment for why its `localSpan` is zero
        // along the *other* axis, leaving that axis at `originScale` above)
        // turn into a uniform, aspect-preserving resize while Shift is
        // held: the edge's own un-dragged axis is exactly the "other axis"
        // this then overrides.
        if (event.shiftKey && Math.abs(scale.originScale.x) > 1e-6 && Math.abs(scale.originScale.y) > 1e-6) {
          const relX = Math.abs(newScale.x / scale.originScale.x - 1)
          const relY = Math.abs(newScale.y / scale.originScale.y - 1)
          if (relX >= relY) {
            newScale.y = clamp(newScale.x * (scale.originScale.y / scale.originScale.x))
          } else {
            newScale.x = clamp(newScale.y * (scale.originScale.x / scale.originScale.y))
          }
        }

        // Solve for the translation that keeps the anchor corner fixed at
        // `anchorWorld` under the new scale: anchorWorld = newTranslation +
        // R(rotation) * ((anchorLocal - anchor) * newScale) — matching
        // `localToWorld`'s own formula exactly (`Transform2D::to_affine2`
        // scales/rotates around the node's `anchor`, not local `(0, 0)`).
        const anchorScaledX = (scale.anchorLocal.x - scale.anchor.x) * newScale.x
        const anchorScaledY = (scale.anchorLocal.y - scale.anchor.y) * newScale.y
        const cosR = Math.cos(scale.rotationRad)
        const sinR = Math.sin(scale.rotationRad)
        const newTranslation = {
          x: scale.anchorWorld.x - (anchorScaledX * cosR - anchorScaledY * sinR),
          y: scale.anchorWorld.y - (anchorScaledX * sinR + anchorScaledY * cosR),
        }

        scale.lastScale = newScale
        scale.lastTranslation = newTranslation
        setLiveOverride({ translation: newTranslation, rotation: scale.rotationRad, scale: newScale })
        // Same world-to-local conversion as the rotate branch above —
        // `newTranslation`/`newScale` are world-space (derived from
        // `scale.anchorWorld`, itself computed against `worldTransform`), so
        // `onPreviewScale` needs them run through `localFor` first.
        const localNow = localFor(scale.parentWorldTransform, { translation: newTranslation, rotation: scale.rotationRad, scale: newScale })
        propsRef.current.onPreviewScale(localNow.translation.x, localNow.translation.y, localNow.scale.x, localNow.scale.y)
        return
      }

      const anchor = anchorDragRef.current
      if (anchor) {
        const mouseWorld = toWorld(event.clientX, event.clientY)
        // Parent-local equivalent of the cursor — same `localFor` conversion
        // every other handle uses, picking out just the translation
        // component (rotation/scale here are dummy inputs, unused by the
        // formula for the field this reads).
        const mouseLocal = localFor(anchor.parentWorldTransform, { translation: mouseWorld, rotation: 0, scale: { x: 1, y: 1 } }).translation

        // The inverse of `Transform2D::with_anchor_preserving_world`'s
        // forward formula (see that method's Rust doc comment): solves for
        // the new *anchor* given a translation that's free to move to
        // wherever the cursor is, instead of solving for translation given a
        // fixed anchor. `anchor.rotationRad`/`.scale` are the node's own
        // local values (not `rotation`/`nodeScale` below, which are
        // world-composed) — see `AnchorDrag`'s own field doc comment for why
        // that distinction matters here specifically.
        const dx = mouseLocal.x - anchor.originTranslation.x
        const dy = mouseLocal.y - anchor.originTranslation.y
        const cos = Math.cos(-anchor.rotationRad)
        const sin = Math.sin(-anchor.rotationRad)
        const newAnchor = {
          x: anchor.originAnchor.x + (dx * cos - dy * sin) / anchor.scale.x,
          y: anchor.originAnchor.y + (dx * sin + dy * cos) / anchor.scale.y,
        }
        anchor.lastAnchor = newAnchor
        anchor.lastTranslation = mouseLocal

        // Live overlay only, deliberately approximate for a node nested
        // under a rotated/non-uniformly-scaled parent (uses the frozen
        // world-*composed* rotation/scale, not the node's own local ones the
        // compensation math above correctly used) — same "exact for rigid +
        // uniform-scale, approximate otherwise" simplification
        // `Transform2D::compose`'s own doc comment already accepts for this
        // codebase's nested-transform handling generally. Self-corrects the
        // instant the drag commits, since the real value sent to Rust
        // (`newAnchor` above) is always exact regardless of parenting.
        const worldDx = mouseWorld.x - anchor.originTranslationWorld.x
        const worldDy = mouseWorld.y - anchor.originTranslationWorld.y
        const worldCos = Math.cos(-anchor.worldRotationRad)
        const worldSin = Math.sin(-anchor.worldRotationRad)
        const overlayAnchor = {
          x: anchor.originAnchor.x + (worldDx * worldCos - worldDy * worldSin) / anchor.worldScale.x,
          y: anchor.originAnchor.y + (worldDx * worldSin + worldDy * worldCos) / anchor.worldScale.y,
        }
        setLiveOverride({ translation: mouseWorld, rotation: anchor.worldRotationRad, scale: anchor.worldScale, anchor: overlayAnchor })
        propsRef.current.onPreviewAnchor(newAnchor.x, newAnchor.y, mouseLocal.x, mouseLocal.y)
      }
    }

    function handleWindowMouseUp(event: MouseEvent) {
      const rotate = rotateDragRef.current
      if (rotate) {
        rotateDragRef.current = null
        setLiveOverride(null)
        const localOrigin =
          localFor(rotate.parentWorldTransform, { translation: rotate.center, rotation: rotate.originDegrees * (Math.PI / 180), scale: rotate.scale }).rotation *
          (180 / Math.PI)
        const localFinal =
          localFor(rotate.parentWorldTransform, { translation: rotate.center, rotation: rotate.lastDegrees * (Math.PI / 180), scale: rotate.scale }).rotation *
          (180 / Math.PI)
        propsRef.current.onCommitRotation(localOrigin, localFinal, event.altKey)
        return
      }

      const scale = scaleDragRef.current
      if (scale) {
        scaleDragRef.current = null
        setLiveOverride(null)
        const localOrigin = localFor(scale.parentWorldTransform, { translation: scale.originTranslation, rotation: scale.rotationRad, scale: scale.originScale })
        const localFinal = localFor(scale.parentWorldTransform, { translation: scale.lastTranslation, rotation: scale.rotationRad, scale: scale.lastScale })
        propsRef.current.onCommitScale(
          localOrigin.translation.x,
          localOrigin.translation.y,
          localOrigin.scale.x,
          localOrigin.scale.y,
          localFinal.translation.x,
          localFinal.translation.y,
          localFinal.scale.x,
          localFinal.scale.y,
          event.altKey,
        )
        return
      }

      const anchor = anchorDragRef.current
      if (anchor) {
        anchorDragRef.current = null
        setLiveOverride(null)
        propsRef.current.onCommitAnchor(
          anchor.originAnchor.x,
          anchor.originAnchor.y,
          anchor.originTranslation.x,
          anchor.originTranslation.y,
          anchor.lastAnchor.x,
          anchor.lastAnchor.y,
          anchor.lastTranslation.x,
          anchor.lastTranslation.y,
        )
      }
    }

    window.addEventListener('mousemove', handleWindowMouseMove)
    window.addEventListener('mouseup', handleWindowMouseUp)
    return () => {
      window.removeEventListener('mousemove', handleWindowMouseMove)
      window.removeEventListener('mouseup', handleWindowMouseUp)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  if (!selectedNode) return null

  // While a rotate/scale drag is in progress, render from its live-preview
  // transform instead of `selectedNode.worldTransform` — see `LiveOverride`'s
  // doc comment for why the prop itself doesn't update until the drag ends.
  // `worldTransform` (not `transform`, the local-to-parent-only value) is
  // what actually keeps this overlay's position/rotation matching the
  // shape's real rendered position for a node nested under a
  // moved/rotated/scaled group — see `NodeSummary.worldTransform`'s own doc
  // comment. Every drag below computes in this same world space and only
  // converts back to local (via `localFor`) at the point it actually calls
  // `onPreview*`/`onCommit*`.
  const { translation, rotation, scale: nodeScale } = liveOverride ?? selectedNode.worldTransform
  const { localMin, localMax, parentWorldTransform } = selectedNode
  // `liveOverride.anchor` only during an active anchor drag (every other
  // drag leaves it `undefined`, falling back to the node's own real anchor —
  // see `LiveOverride.anchor`'s doc comment). `worldTransform.anchor` and
  // `transform.anchor` are always the same value (`Transform2D::compose`
  // forwards a node's own anchor through unconverted), so either would work
  // here; `transform` is used since it's the more obviously "this node's own
  // anchor, not something ancestor-derived" of the two.
  const anchor = liveOverride?.anchor ?? selectedNode.transform.anchor
  const cosR = Math.cos(rotation)
  const sinR = Math.sin(rotation)

  // Subtracting `anchor` before scale/rotate mirrors `Transform2D::to_affine2`'s
  // own formula exactly (see that method's Rust doc comment) — without this,
  // the outline box/handles would keep pivoting around local `(0, 0)`
  // regardless of where `anchor` actually is.
  function localToWorld(lx: number, ly: number) {
    const ax = lx - anchor.x
    const ay = ly - anchor.y
    const scaledX = ax * nodeScale.x
    const scaledY = ay * nodeScale.y
    return {
      x: translation.x + (scaledX * cosR - scaledY * sinR),
      y: translation.y + (scaledX * sinR + scaledY * cosR),
    }
  }

  const localCorners = [
    { x: localMin.x, y: localMin.y },
    { x: localMax.x, y: localMin.y },
    { x: localMax.x, y: localMax.y },
    { x: localMin.x, y: localMax.y },
  ]
  const screenCorners = localCorners.map((c) => {
    const world = localToWorld(c.x, c.y)
    return worldToScreen(world.x, world.y, camera, canvasSize)
  })

  // One handle per edge, each pinning the *opposite* edge's midpoint —
  // single-axis-only stretch (see `startScaleDrag`'s own doc comment for
  // why no separate drag math is needed for this). Local coordinates only
  // (screen positions/cursors computed the same way corners' are, just
  // below) since that's all `startScaleDrag` itself needs.
  const midX = (localMin.x + localMax.x) / 2
  const midY = (localMin.y + localMax.y) / 2
  const localEdges = [
    { dragged: { x: localMax.x, y: midY }, anchor: { x: localMin.x, y: midY } }, // right
    { dragged: { x: midX, y: localMax.y }, anchor: { x: midX, y: localMin.y } }, // top
    { dragged: { x: localMin.x, y: midY }, anchor: { x: localMax.x, y: midY } }, // left
    { dragged: { x: midX, y: localMin.y }, anchor: { x: midX, y: localMax.y } }, // bottom
  ]
  const screenEdges = localEdges.map(({ dragged }) => {
    const world = localToWorld(dragged.x, dragged.y)
    return worldToScreen(world.x, world.y, camera, canvasSize)
  })

  // The resize cursor for each corner/edge has to match its *actual*
  // on-screen direction, not a fixed per-index guess — once the node is
  // rotated, "the corner that started as top-right" can visually sit
  // anywhere around the box. Snap the screen-space angle from the box's
  // center to each handle onto the nearest of the 8 directions CSS actually
  // has cursors for, the same way Figma rotates its own resize cursors as
  // you rotate a shape.
  const screenCenter = worldToScreen(translation.x, translation.y, camera, canvasSize)
  const RESIZE_CURSORS = ['ew-resize', 'nwse-resize', 'ns-resize', 'nesw-resize', 'ew-resize', 'nwse-resize', 'ns-resize', 'nesw-resize']
  const cursorFor = (point: { x: number; y: number }) => {
    const angleDeg = (Math.atan2(point.y - screenCenter.y, point.x - screenCenter.x) * (180 / Math.PI) + 360) % 360
    const bucket = Math.round(angleDeg / 45) % 8
    return RESIZE_CURSORS[bucket]
  }
  const cornerCursors = screenCorners.map(cursorFor)
  const edgeCursors = screenEdges.map(cursorFor)

  const topCenterLocal = { x: (localMin.x + localMax.x) / 2, y: localMax.y }
  const topCenterWorld = localToWorld(topCenterLocal.x, topCenterLocal.y)
  const topCenterScreen = worldToScreen(topCenterWorld.x, topCenterWorld.y, camera, canvasSize)
  // The node's local "up" direction, in screen space: rotate (0,1) by the
  // node's rotation to get the world-space direction, then flip y (screen
  // pixels increase downward, world does not) — this is what keeps the
  // rotate handle sitting "above" the shape regardless of its rotation.
  const upWorld = { x: -sinR, y: cosR }
  const upScreen = { x: upWorld.x * camera.zoom, y: -upWorld.y * camera.zoom }
  const upScreenLen = Math.hypot(upScreen.x, upScreen.y) || 1
  const rotateHandleScreen = {
    x: topCenterScreen.x + (upScreen.x / upScreenLen) * ROTATE_HANDLE_DISTANCE_PX,
    y: topCenterScreen.y + (upScreen.y / upScreenLen) * ROTATE_HANDLE_DISTANCE_PX,
  }

  function handleRotateMouseDown(event: ReactMouseEvent) {
    event.preventDefault()
    event.stopPropagation()
    const mouseWorld = screenToWorld(
      event.clientX - containerRef.current!.getBoundingClientRect().left,
      event.clientY - containerRef.current!.getBoundingClientRect().top,
      camera,
      canvasSize,
    )
    const originDegrees = rotation * (180 / Math.PI)
    rotateDragRef.current = {
      center: { x: translation.x, y: translation.y },
      initialAngleRad: Math.atan2(mouseWorld.y - translation.y, mouseWorld.x - translation.x),
      originDegrees,
      lastDegrees: originDegrees,
      scale: { x: nodeScale.x, y: nodeScale.y },
      parentWorldTransform,
    }
  }

  /**
   * Starts a scale drag from `draggedLocal`, pinning `anchorLocal` (the
   * opposite corner, or the opposite edge's midpoint) in world space —
   * matching Figma/Illustrator's default drag-a-handle behavior — rather
   * than the node's own origin, which is why the mousemove handler above
   * also has to recompute translation every frame: as the shape grows or
   * shrinks around a fixed point, its origin necessarily moves.
   *
   * A corner handle's `draggedLocal`/`anchorLocal` differ on *both* axes, so
   * both `newScale.x`/`.y` get real cursor-driven values — free two-axis
   * stretch by default, Shift-locked to the origin aspect ratio per the
   * mousemove handler's own doc comment. An *edge* handle's two points
   * differ on only one axis (its midpoint sits at the same coordinate as
   * its opposite edge's midpoint along the perpendicular axis), so
   * `localSpan` comes out to (near) zero on that axis — the mousemove
   * handler's existing "too-small-a-span, keep this axis at its origin
   * scale" fallback is what turns that into single-axis-only stretching,
   * with no separate code path needed for edges at all.
   */
  function startScaleDrag(draggedLocal: { x: number; y: number }, anchorLocal: { x: number; y: number }, event: ReactMouseEvent) {
    event.preventDefault()
    event.stopPropagation()
    const anchorWorld = localToWorld(anchorLocal.x, anchorLocal.y)
    scaleDragRef.current = {
      anchorWorld,
      anchorLocal,
      localSpan: { x: draggedLocal.x - anchorLocal.x, y: draggedLocal.y - anchorLocal.y },
      rotationRad: rotation,
      originTranslation: { x: translation.x, y: translation.y },
      originScale: { x: nodeScale.x, y: nodeScale.y },
      lastTranslation: { x: translation.x, y: translation.y },
      lastScale: { x: nodeScale.x, y: nodeScale.y },
      anchor: { x: anchor.x, y: anchor.y },
      parentWorldTransform,
    }
  }

  /**
   * Starts an anchor (pivot point) drag — the After Effects/Rive convention
   * of repositioning where rotation/scale pivot without moving the shape's
   * rendered geometry. See the mousemove handler above for the
   * compensation math (`Transform2D::with_anchor_preserving_world`'s
   * inverse), and that method's own Rust doc comment for the underlying
   * derivation this mirrors.
   */
  function handleAnchorMouseDown(event: ReactMouseEvent) {
    event.preventDefault()
    event.stopPropagation()
    if (!selectedNode) return
    const local = selectedNode.transform
    anchorDragRef.current = {
      rotationRad: local.rotation,
      scale: { x: local.scale.x, y: local.scale.y },
      originAnchor: { x: local.anchor.x, y: local.anchor.y },
      originTranslation: { x: local.translation.x, y: local.translation.y },
      originTranslationWorld: { x: translation.x, y: translation.y },
      lastAnchor: { x: local.anchor.x, y: local.anchor.y },
      lastTranslation: { x: local.translation.x, y: local.translation.y },
      parentWorldTransform,
      worldRotationRad: rotation,
      worldScale: { x: nodeScale.x, y: nodeScale.y },
    }
  }

  return (
    <div ref={containerRef} className="pointer-events-none absolute inset-0 select-none">
      <svg className="absolute inset-0 h-full w-full overflow-visible">
        <polygon
          points={screenCorners.map((c) => `${c.x},${c.y}`).join(' ')}
          fill="none"
          stroke="rgba(109,140,255,0.9)"
          strokeWidth={1.5}
        />
        <line
          x1={topCenterScreen.x}
          y1={topCenterScreen.y}
          x2={rotateHandleScreen.x}
          y2={rotateHandleScreen.y}
          stroke="rgba(109,140,255,0.9)"
          strokeWidth={1.5}
        />
      </svg>

      {screenCorners.map((corner, i) => (
        <div
          key={i}
          onMouseDown={(event) => startScaleDrag(localCorners[i], localCorners[(i + 2) % 4], event)}
          className="an1-hit pointer-events-auto absolute rounded-[2px] border border-[rgba(109,140,255,0.9)] bg-white"
          style={{
            left: corner.x - HANDLE_SIZE_PX / 2,
            top: corner.y - HANDLE_SIZE_PX / 2,
            width: HANDLE_SIZE_PX,
            height: HANDLE_SIZE_PX,
            cursor: cornerCursors[i],
          }}
        />
      ))}

      {/* Edge (single-axis stretch) handles — thin bars, deliberately
          distinct from the square corner handles, at each edge's own
          midpoint. Hold Shift while dragging one to scale the other axis
          to match instead (see the mousemove handler's own doc comment). */}
      {screenEdges.map((edge, i) => {
        // Bars run *along* their own edge, not across it — an edge whose
        // resize cursor is `ew-resize`/`nesw-resize`-ish (dragging changes
        // width) reads as a vertical bar; the perpendicular case reads as
        // horizontal. Cheap enough to just check which cursor bucket this
        // edge landed in rather than tracking "is this a left/right vs.
        // top/bottom edge" separately given rotation can swap the two.
        const vertical = edgeCursors[i] === 'ew-resize' || edgeCursors[i] === 'nesw-resize'
        const barLength = HANDLE_SIZE_PX * 2.2
        const width = vertical ? HANDLE_SIZE_PX * 0.6 : barLength
        const height = vertical ? barLength : HANDLE_SIZE_PX * 0.6
        return (
          <div
            key={i}
            onMouseDown={(event) => startScaleDrag(localEdges[i].dragged, localEdges[i].anchor, event)}
            className="an1-hit pointer-events-auto absolute rounded-[2px] border border-[rgba(109,140,255,0.9)] bg-white"
            style={{ left: edge.x - width / 2, top: edge.y - height / 2, width, height, cursor: edgeCursors[i] }}
          />
        )
      })}

      <div
        onMouseDown={handleRotateMouseDown}
        className="an1-hit pointer-events-auto absolute rounded-full border border-[rgba(109,140,255,0.9)] bg-white"
        style={{
          left: rotateHandleScreen.x - ROTATE_HANDLE_RADIUS_PX,
          top: rotateHandleScreen.y - ROTATE_HANDLE_RADIUS_PX,
          width: ROTATE_HANDLE_RADIUS_PX * 2,
          height: ROTATE_HANDLE_RADIUS_PX * 2,
          cursor: 'grab',
        }}
      />

      {/* Anchor (pivot) handle — a crosshair, deliberately distinct from the
          plain-circle rotate handle and square corner-scale handles, at
          `screenCenter` (already exactly `worldToScreen` of `translation`,
          which — per `Transform2D::to_affine2`'s invariant — is always the
          anchor's own world position, dragged or not). */}
      <div
        onMouseDown={handleAnchorMouseDown}
        className="an1-hit pointer-events-auto absolute flex items-center justify-center rounded-full border border-[rgba(255,196,0,0.9)] bg-white/80"
        style={{
          left: screenCenter.x - ANCHOR_HANDLE_RADIUS_PX,
          top: screenCenter.y - ANCHOR_HANDLE_RADIUS_PX,
          width: ANCHOR_HANDLE_RADIUS_PX * 2,
          height: ANCHOR_HANDLE_RADIUS_PX * 2,
          cursor: 'move',
        }}
      >
        <svg width={ANCHOR_HANDLE_RADIUS_PX * 2} height={ANCHOR_HANDLE_RADIUS_PX * 2} className="absolute overflow-visible">
          <line x1={0} y1={ANCHOR_HANDLE_RADIUS_PX} x2={ANCHOR_HANDLE_RADIUS_PX * 2} y2={ANCHOR_HANDLE_RADIUS_PX} stroke="rgba(255,196,0,0.9)" strokeWidth={1} />
          <line x1={ANCHOR_HANDLE_RADIUS_PX} y1={0} x2={ANCHOR_HANDLE_RADIUS_PX} y2={ANCHOR_HANDLE_RADIUS_PX * 2} stroke="rgba(255,196,0,0.9)" strokeWidth={1} />
        </svg>
      </div>
    </div>
  )
}
