/**
 * Multi-touch gesture recognition for the editor's drawing surfaces — the
 * one shared recognizer behind both the main artboard canvas (`App.tsx`)
 * and Paint mode's paper.js canvas (`PaintAssetEditor.tsx`), so a tablet or
 * phone gets the same Procreate/Figma-style vocabulary in both places:
 *
 * | Gesture                                   | Callback            |
 * | ----------------------------------------- | ------------------- |
 * | Two fingers, pinch                        | `onPinchPan` (scale)|
 * | Two fingers, drag                         | `onPinchPan` (pan)  |
 * | Two-finger tap (quick, little movement)   | `onTwoFingerTap`    |
 * | Three-finger tap                          | `onThreeFingerTap`  |
 * | One finger held still ~500 ms             | `onLongPress`       |
 *
 * **Touch only.** Everything here is gated on `PointerEvent.pointerType ===
 * 'touch'` — mouse and pen input pass straight through, untouched, exactly
 * as before this module existed. (A pen is a precise single-point tool;
 * treating a palm-plus-pen as a "two-finger pinch" would be wrong, and the
 * OS already reports a palm as `touch`, never `pen`.)
 *
 * **Why window-level, capture-phase listeners.** The surfaces this guards
 * are stacks of siblings, not one element: the main canvas has
 * `TransformHandles`, `PathEditOverlay`, `DrawPathOverlay`, `BoneGizmos` and
 * friends layered over it, several of which `setPointerCapture` and/or
 * listen on `window` themselves. The only place guaranteed to see every
 * pointer event *before* any of them is a capture-phase listener on
 * `window` (the very first stop on every event's propagation path), and
 * `stopImmediatePropagation()` there is the only way to keep a gesture's
 * fingers from also reaching those handlers — including their own
 * window-level ones, which sit later on that same path. The `el` passed to
 * `createTouchGestures` only decides *which* touches start being tracked
 * (a finger landing inside it); it is not where the listeners live.
 *
 * **Cancelling the one-finger action a pinch interrupts.** The first finger
 * of a pinch can't be told apart from an ordinary one-finger drag until the
 * second finger lands (tens of ms later), so it has already been delivered
 * to whatever tool is under it — a node drag, a path-point drag, a freehand
 * stroke has already *started*. When the second finger arrives this module:
 *   1. calls `onGestureStart` first, so the owner can cancel anything it
 *      drives itself (e.g. `App.tsx` reverts its own node-drag preview and
 *      clears its drag/pan refs);
 *   2. dispatches a synthetic `pointercancel` (bubbling, same `pointerId`)
 *      at the first finger's original target — the event a browser itself
 *      sends when it takes a touch over for a native gesture, so pointer-
 *      based overlays that handle it revert cleanly;
 *   3. when the mouse bridge (below) had started a drag, "rewinds" it: a
 *      synthetic `mousemove` back to the finger's start point, then a
 *      `mouseup` there — so mouse-driven drags (`TransformHandles`' scale/
 *      rotate handles) end with their live preview back where it began;
 *   4. swallows every later event of every finger in the gesture until they
 *      have *all* lifted — a finger left down after a pinch doesn't
 *      suddenly start dragging whatever it happens to be resting on.
 *
 * **The mouse bridge (`emulateMouse`).** Chromium/WebView2 only synthesise
 * compatibility *mouse* events for a touch *tap* — a one-finger touch drag
 * never produces `mousemove`. The main canvas's select/drag/pan and the
 * transform handles are written against mouse events (`onMouseDown` +
 * window `mousemove`/`mouseup`), so without help a finger could tap-select
 * but never drag. With `emulateMouse`, a single tracked touch is re-
 * dispatched as `mousedown`/`mousemove`/`mouseup` (left button) at the same
 * targets, and its native compatibility mouse events are suppressed
 * (`preventDefault()` on the `pointerdown`) so a tap never fires twice.
 * Interactive form controls (buttons, inputs, …) are left alone so the
 * canvas's own zoom widget and similar still click natively. Paint mode
 * leaves this off — paper.js listens to `touch*` events itself.
 *
 * **Raw TouchEvents (`blockTouchEvents`).** paper.js (Paint mode) doesn't
 * use pointer events at all on Chromium — it listens to `touchstart`/
 * `touchmove`/`touchend` on the document. With this option every *trusted*
 * `touch*` event is swallowed at window-capture while a gesture owns the
 * fingers (paper's stroke stops growing, its second-finger `touchstart`
 * never lands). Untrusted (synthetic) touch events pass, so the owner can
 * still end paper's in-progress stroke from `onGestureStart` with its own
 * synthetic `touchcancel`. Chromium dispatches a touch's `pointerdown`
 * before its `touchstart` (and `pointerup` before `touchend`), so the
 * gesture state is always current by the time the touch event is seen.
 *
 * Returns a dispose function that removes every listener and timer.
 */

/** A point in client (viewport CSS pixel) coordinates. */
export interface ClientPoint {
  x: number
  y: number
}

/** The single finger a gesture interrupted, as seen when it went down —
 * handed to `onGestureStart` so an owner can cancel whatever it started. */
export interface InterruptedTouch {
  pointerId: number
  /** Element the finger originally landed on. */
  target: EventTarget | null
  /** Where it went down. */
  start: ClientPoint
  /** Where it last was before the second finger arrived. */
  last: ClientPoint
}

export interface TouchGestureOptions {
  /** Checked on every new touch: `false` means "leave touches completely
   * alone right now" (e.g. Playground, where the game owns the pointer).
   * Read live, so an owner can pass a ref-backed closure from a mount-only
   * effect. Defaults to always enabled. */
  enabled?: () => boolean
  /** Extra filter for which element a *first* finger may land on to be
   * tracked at all (already restricted to inside `el`). Defaults to
   * anything inside `el`. */
  accept?: (target: Element) => boolean
  /** Per-move pinch/pan update while two or more fingers are down, as
   * *incremental* deltas since the previous call (not since gesture start):
   * `scaleDelta` multiplies the current zoom (`>1` = fingers spreading =
   * zoom in), `centroid` is the fingers' current midpoint in client
   * coordinates (zoom about this), `panDelta` is how far that midpoint
   * moved in client pixels (content should follow the fingers). */
  onPinchPan?: (scaleDelta: number, centroid: ClientPoint, panDelta: ClientPoint) => void
  /** Two fingers tapped (down and up quickly, little movement) — Undo, by
   * the convention every iPad drawing app uses. */
  onTwoFingerTap?: () => void
  /** Three fingers tapped — Redo. */
  onThreeFingerTap?: () => void
  /** One finger held still for `LONG_PRESS_MS`. The press is then consumed
   * (its later moves/lift are swallowed, a bridged mouse drag is rewound).
   * Leave unset to disable long-press entirely. */
  onLongPress?: (at: ClientPoint, target: EventTarget | null) => void
  /** A multi-finger gesture began. Called *before* this module dispatches
   * its own synthetic cancel/rewind events, so the owner can clear its own
   * drag state first. `interrupted` is the lone finger that was already
   * down (and possibly mid-action) when the second one landed, if any. */
  onGestureStart?: (interrupted: InterruptedTouch | null) => void
  /** A lone first finger started being tracked (it has *not* been
   * swallowed — the tool under it gets it as normal). Lets an owner note
   * "before" state it may need to roll back if this turns into a gesture
   * (Paint mode records its undo stack here). */
  onSingleStart?: (at: ClientPoint, target: Element) => void
  /** Every finger of a multi-finger gesture has lifted. */
  onGestureEnd?: () => void
  /** Re-dispatch a lone touch as left-button mouse events (see the module
   * doc comment's "mouse bridge" section). */
  emulateMouse?: boolean
  /** Swallow trusted `touch*` events while a gesture owns the fingers (see
   * the module doc comment's "Raw TouchEvents" section). */
  blockTouchEvents?: boolean
}

/** A multi-finger touch counts as a tap only if its first finger lifts
 * within this long of the first finger going down. */
export const TAP_MAX_MS = 250
/** ...and no finger strayed further than this from where it landed (CSS px).
 * Generous on purpose: two fingertips "landing" wobble a lot more than one
 * mouse click does. */
export const TAP_SLOP_PX = 14
/** How long one still finger has to stay down to count as a long-press. */
export const LONG_PRESS_MS = 500
/** How far a long-press finger may drift and still count as "still". */
const LONG_PRESS_SLOP_PX = 8

/** Form controls the mouse bridge leaves to the browser's native tap →
 * click handling (the canvas zoom widget's buttons sit inside the canvas
 * viewport, for one). */
const NATIVE_CONTROLS = 'button, input, select, textarea, a[href], label, [contenteditable=""], [contenteditable="true"]'

interface Tracked {
  start: ClientPoint
  last: ClientPoint
  target: EventTarget | null
}

type Phase =
  /** No tracked touch down. */
  | 'idle'
  /** Exactly one finger: passing through to the tool (bridged if asked). */
  | 'single'
  /** Two-plus fingers: pinch/pan, swallowing everything until all lift. */
  | 'gesture'
  /** A long-press fired: swallowing that finger until it lifts. */
  | 'consumed'

/**
 * Starts recognising touch gestures for touches that land inside `el`.
 * See the module doc comment for the full behaviour; returns `dispose`.
 */
export function createTouchGestures(el: HTMLElement, options: TouchGestureOptions): () => void {
  const pointers = new Map<number, Tracked>()
  let phase: Phase = 'idle'
  /** `true` while the single finger is being re-dispatched as a mouse. */
  let mouseBridged = false
  /** The lone finger's id while `phase === 'single'`. */
  let singleId: number | null = null
  let longPressTimer: number | null = null

  // Multi-finger gesture bookkeeping.
  let firstDownTime = 0
  let firstLiftTime: number | null = null
  let maxFingers = 0
  let moved = false
  /** Previous pinch sample (distance between the first two fingers and
   * their midpoint) — `onPinchPan` reports deltas against this. */
  let prevDistance = 0
  let prevCentroid: ClientPoint = { x: 0, y: 0 }

  const isEnabled = () => (options.enabled ? options.enabled() : true)

  const clearLongPress = () => {
    if (longPressTimer !== null) window.clearTimeout(longPressTimer)
    longPressTimer = null
  }

  /** Re-dispatches a mouse event of `type` at `target` (or at `window` if
   * that element has since left the DOM — React re-renders can replace the
   * node a drag started on, and a detached node's events never reach the
   * window listeners that actually track the drag). */
  const dispatchMouse = (type: 'mousedown' | 'mousemove' | 'mouseup', target: EventTarget | null, at: ClientPoint) => {
    const node = target instanceof Node && target.isConnected ? target : window
    node.dispatchEvent(
      new MouseEvent(type, {
        bubbles: true,
        cancelable: true,
        composed: true,
        clientX: at.x,
        clientY: at.y,
        screenX: at.x,
        screenY: at.y,
        button: 0,
        // Held during down/move; released on up.
        buttons: type === 'mouseup' ? 0 : 1,
        view: window,
      }),
    )
  }

  /** Ends whatever the lone finger had started — synthetic `pointercancel`
   * for pointer-based tools, a rewind-and-release for a bridged mouse drag.
   * Shared by "a second finger landed" and "a long-press fired". */
  const cancelSingle = (id: number, finger: Tracked) => {
    const node = finger.target instanceof Node && finger.target.isConnected ? finger.target : window
    node.dispatchEvent(
      new PointerEvent('pointercancel', {
        bubbles: true,
        composed: true,
        pointerId: id,
        pointerType: 'touch',
        isPrimary: true,
        clientX: finger.last.x,
        clientY: finger.last.y,
      }),
    )
    if (mouseBridged) {
      dispatchMouse('mousemove', finger.target, finger.start)
      dispatchMouse('mouseup', finger.target, finger.start)
      mouseBridged = false
    }
  }

  /** Midpoint of, and distance between, the first two tracked fingers. */
  const pinchSample = () => {
    const [a, b] = [...pointers.values()]
    return {
      centroid: { x: (a.last.x + b.last.x) / 2, y: (a.last.y + b.last.y) / 2 },
      distance: Math.hypot(a.last.x - b.last.x, a.last.y - b.last.y),
    }
  }

  const rebaseline = () => {
    if (pointers.size < 2) return
    const sample = pinchSample()
    prevCentroid = sample.centroid
    prevDistance = sample.distance
  }

  /** Swallows an event so no later listener anywhere sees it. */
  const swallow = (event: Event) => {
    event.stopImmediatePropagation()
    if (event.cancelable) event.preventDefault()
  }

  function onPointerDown(event: PointerEvent) {
    if (event.pointerType !== 'touch' || !event.isTrusted) return
    const point = { x: event.clientX, y: event.clientY }

    // Extra fingers joining a gesture already in progress (or a consumed
    // long-press) are part of it wherever they land.
    if (phase === 'gesture' || phase === 'consumed') {
      pointers.set(event.pointerId, { start: point, last: point, target: event.target })
      if (phase === 'gesture') {
        maxFingers = Math.max(maxFingers, pointers.size)
        rebaseline()
      }
      swallow(event)
      return
    }

    if (phase === 'single') {
      // Second finger: this is a multi-finger gesture, not a drag.
      const firstId = singleId!
      const first = pointers.get(firstId)!
      clearLongPress()
      pointers.set(event.pointerId, { start: point, last: point, target: event.target })
      phase = 'gesture'
      singleId = null
      maxFingers = pointers.size
      moved = false
      firstLiftTime = null
      rebaseline()
      swallow(event)
      options.onGestureStart?.({ pointerId: firstId, target: first.target, start: first.start, last: first.last })
      cancelSingle(firstId, first)
      return
    }

    // A first finger: only tracked when it lands on the guarded surface.
    const target = event.target
    if (!(target instanceof Element) || !el.contains(target)) return
    if (options.accept && !options.accept(target)) return
    if (!isEnabled()) return

    pointers.set(event.pointerId, { start: point, last: point, target })
    phase = 'single'
    singleId = event.pointerId
    firstDownTime = performance.now()
    options.onSingleStart?.(point, target)

    if (options.emulateMouse && !target.closest(NATIVE_CONTROLS)) {
      // Suppresses the browser's own compatibility mousedown/mouseup for
      // this touch (a tap would otherwise fire twice); `click` still fires.
      event.preventDefault()
      mouseBridged = true
      dispatchMouse('mousedown', target, point)
    }

    if (options.onLongPress) {
      const id = event.pointerId
      longPressTimer = window.setTimeout(() => {
        longPressTimer = null
        const finger = pointers.get(id)
        if (phase !== 'single' || !finger) return
        phase = 'consumed'
        singleId = null
        cancelSingle(id, finger)
        options.onLongPress?.(finger.last, finger.target)
      }, LONG_PRESS_MS)
    }
  }

  function onPointerMove(event: PointerEvent) {
    if (event.pointerType !== 'touch' || !event.isTrusted) return
    const finger = pointers.get(event.pointerId)
    if (!finger) return
    finger.last = { x: event.clientX, y: event.clientY }

    if (phase === 'single') {
      if (longPressTimer !== null && Math.hypot(finger.last.x - finger.start.x, finger.last.y - finger.start.y) > LONG_PRESS_SLOP_PX) clearLongPress()
      if (mouseBridged) dispatchMouse('mousemove', finger.target, finger.last)
      return
    }

    swallow(event)
    if (phase !== 'gesture') return
    if (Math.hypot(finger.last.x - finger.start.x, finger.last.y - finger.start.y) > TAP_SLOP_PX) moved = true
    if (pointers.size < 2) return
    const sample = pinchSample()
    const scaleDelta = prevDistance > 0 && sample.distance > 0 ? sample.distance / prevDistance : 1
    const panDelta = { x: sample.centroid.x - prevCentroid.x, y: sample.centroid.y - prevCentroid.y }
    prevDistance = sample.distance
    prevCentroid = sample.centroid
    if (scaleDelta !== 1 || panDelta.x !== 0 || panDelta.y !== 0) options.onPinchPan?.(scaleDelta, sample.centroid, panDelta)
  }

  function onPointerEnd(event: PointerEvent) {
    if (event.pointerType !== 'touch' || !event.isTrusted) return
    const finger = pointers.get(event.pointerId)
    if (!finger) return
    pointers.delete(event.pointerId)

    if (phase === 'single') {
      clearLongPress()
      if (mouseBridged) {
        // A real cancel (OS took the touch) rewinds; a lift releases where
        // the finger is.
        if (event.type === 'pointercancel') dispatchMouse('mousemove', finger.target, finger.start)
        dispatchMouse('mouseup', finger.target, event.type === 'pointercancel' ? finger.start : { x: event.clientX, y: event.clientY })
        mouseBridged = false
      }
      phase = 'idle'
      singleId = null
      return
    }

    swallow(event)
    if (phase === 'gesture') {
      if (firstLiftTime === null) firstLiftTime = performance.now()
      // The remaining fingers' midpoint jumps when one lifts: re-sample so
      // the next move doesn't read that jump as a pan.
      rebaseline()
    }
    if (pointers.size > 0) return

    if (phase === 'gesture') {
      const quick = firstLiftTime !== null && firstLiftTime - firstDownTime <= TAP_MAX_MS
      if (quick && !moved && event.type === 'pointerup') {
        if (maxFingers === 2) options.onTwoFingerTap?.()
        else if (maxFingers === 3) options.onThreeFingerTap?.()
      }
      options.onGestureEnd?.()
    }
    phase = 'idle'
  }

  /** `blockTouchEvents`: see the module doc comment. */
  function onTouchEvent(event: TouchEvent) {
    if (!event.isTrusted) return
    if (phase === 'gesture' || phase === 'consumed') swallow(event)
  }

  const capture = { capture: true, passive: false } as const
  window.addEventListener('pointerdown', onPointerDown, capture)
  window.addEventListener('pointermove', onPointerMove, capture)
  window.addEventListener('pointerup', onPointerEnd, capture)
  window.addEventListener('pointercancel', onPointerEnd, capture)
  const touchTypes = ['touchstart', 'touchmove', 'touchend', 'touchcancel'] as const
  if (options.blockTouchEvents) for (const type of touchTypes) window.addEventListener(type, onTouchEvent, capture)

  return () => {
    clearLongPress()
    window.removeEventListener('pointerdown', onPointerDown, capture)
    window.removeEventListener('pointermove', onPointerMove, capture)
    window.removeEventListener('pointerup', onPointerEnd, capture)
    window.removeEventListener('pointercancel', onPointerEnd, capture)
    if (options.blockTouchEvents) for (const type of touchTypes) window.removeEventListener(type, onTouchEvent, capture)
  }
}
