/**
 * **Pen (stylus) fixes for the editor UI** — Windows pens act like a mouse
 * in Chromium / WebView2, which makes two everyday things awkward:
 *
 * 1. **Handwriting into text fields.** Touching an input with the pen and
 *    moving starts Windows Ink handwriting, so strokes over a field get typed
 *    into it. CSS `touch-action` alone doesn't stop it in every WebView2
 *    build; cancelling the pen's `pointerdown` on a text field does (it's a
 *    default action of that event). A pen *tap* on a field then focuses it
 *    ourselves (caret at the tapped spot when the browser can tell), so
 *    typing still works.
 * 2. **Scrolling with the pen.** A pen drag over a panel selects text or
 *    does nothing instead of scrolling. Here a pen drag that starts on any
 *    scrollable area (sidebars, the Inspector, lists, menus …) scrolls it,
 *    like a finger would; the click a scroll would otherwise end with is
 *    swallowed.
 *
 * Left alone (they need the pen as a pointer): the canvas viewport and its
 * overlays, sliders (`role=slider`, `input[type=range]`, `[data-pen-native]`),
 * the code editor, Blocks, Paint, SVG editors. On draggable rows (Layers …)
 * a quick drag scrolls, and press-and-hold (~300 ms) then drag reorders.
 *
 * Install once (`installPenInput()`); returns a dispose function.
 */

const NATIVE = [
  '[data-pen-native]',
  'input[type=range]',
  '[role=slider]',
  'canvas',
  'svg',
  '.monaco-editor',
  '.blocklySvg',
  '.blocklyWidgetDiv',
  '.blocklyDropDownDiv',
  '[data-paint-editor]',
].join(',')

const TEXT_INPUT_TYPES = new Set(['text', 'search', 'email', 'url', 'tel', 'password', 'number', ''])

function isTextField(el: Element | null): el is HTMLInputElement | HTMLTextAreaElement | HTMLElement {
  if (!el) return false
  if (el instanceof HTMLTextAreaElement) return true
  if (el instanceof HTMLInputElement) return TEXT_INPUT_TYPES.has(el.type)
  return el instanceof HTMLElement && el.isContentEditable
}

/** The nearest ancestor that can actually scroll on `axis`. */
function scrollableAncestor(el: Element | null): HTMLElement | null {
  for (let n: Element | null = el; n && n !== document.documentElement; n = n.parentElement) {
    if (!(n instanceof HTMLElement)) continue
    const style = getComputedStyle(n)
    const canY = /(auto|scroll)/.test(style.overflowY) && n.scrollHeight > n.clientHeight + 1
    const canX = /(auto|scroll)/.test(style.overflowX) && n.scrollWidth > n.clientWidth + 1
    if (canY || canX) return n
  }
  return null
}

const SLOP = 7 // px of pen travel before a press counts as a drag
const HOLD_FOR_DRAG_MS = 300 // press-and-hold first = the element's own drag

export function installPenInput(): () => void {
  let active: {
    id: number
    x: number
    y: number
    downAt: number
    target: Element
    scroller: HTMLElement | null
    field: HTMLElement | null
    scrolling: boolean
    holdDrag: boolean
  } | null = null
  let swallowClickUntil = 0

  const onDown = (e: PointerEvent) => {
    if (e.pointerType !== 'pen' || e.button !== 0 || !(e.target instanceof Element)) return
    const target = e.target
    if (target.closest(NATIVE)) return
    const fieldEl = target.closest('input, textarea, [contenteditable=""], [contenteditable=true]')
    const field = isTextField(fieldEl) ? (fieldEl as HTMLElement) : null
    // A field already focused and being edited: a pen tap moves the caret
    // normally; handwriting starts only on fields the pen *lands* on, so
    // block that landing.
    if (field) e.preventDefault()
    active = { id: e.pointerId, x: e.clientX, y: e.clientY, downAt: performance.now(), target, scroller: scrollableAncestor(target), field, scrolling: false, holdDrag: false }
  }

  const onMove = (e: PointerEvent) => {
    if (!active || e.pointerId !== active.id) return
    const dx = e.clientX - active.x
    const dy = e.clientY - active.y
    if (!active.scrolling) {
      if (Math.hypot(dx, dy) < SLOP) return
      // Held still long enough first: leave it to the element (drag a
      // Layers row, select text …).
      if (performance.now() - active.downAt >= HOLD_FOR_DRAG_MS && !active.field) {
        active.holdDrag = true
        active = null
        return
      }
      if (!active.scroller) {
        active = null
        return
      }
      active.scrolling = true
      document.documentElement.style.userSelect = 'none'
      window.getSelection()?.removeAllRanges()
    }
    e.preventDefault()
    active.scroller!.scrollTop -= dy
    active.scroller!.scrollLeft -= dx
    active.x = e.clientX
    active.y = e.clientY
  }

  const onUp = (e: PointerEvent) => {
    if (!active || e.pointerId !== active.id) return
    const was = active
    active = null
    document.documentElement.style.userSelect = ''
    if (was.scrolling) {
      // The press ended a scroll, not a click.
      swallowClickUntil = performance.now() + 400
      return
    }
    // A plain tap on a text field: focus it ourselves (its pointerdown was
    // cancelled to stop handwriting), caret where the pen touched.
    if (was.field) {
      was.field.focus()
      const field = was.field
      if (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) {
        const doc = document as Document & { caretPositionFromPoint?: (x: number, y: number) => { offset: number } | null }
        const pos = doc.caretPositionFromPoint?.(e.clientX, e.clientY)
        try {
          if (pos && typeof pos.offset === 'number') field.setSelectionRange(pos.offset, pos.offset)
          else field.setSelectionRange(field.value.length, field.value.length)
        } catch {
          // (Number inputs don't support selection ranges.)
        }
      }
    }
  }

  const onCancel = (e: PointerEvent) => {
    if (active && e.pointerId === active.id) {
      active = null
      document.documentElement.style.userSelect = ''
    }
  }

  // A quick pen drag on a draggable row scrolls; only press-and-hold drags.
  const onDragStart = (e: DragEvent) => {
    if (!active) return
    if (performance.now() - active.downAt >= HOLD_FOR_DRAG_MS) {
      active = null
      return
    }
    e.preventDefault()
  }

  const onClick = (e: MouseEvent) => {
    if (performance.now() < swallowClickUntil) {
      e.preventDefault()
      e.stopPropagation()
      swallowClickUntil = 0
    }
  }

  window.addEventListener('pointerdown', onDown, { capture: true })
  window.addEventListener('pointermove', onMove, { capture: true, passive: false })
  window.addEventListener('pointerup', onUp, { capture: true })
  window.addEventListener('pointercancel', onCancel, { capture: true })
  window.addEventListener('dragstart', onDragStart, { capture: true })
  window.addEventListener('click', onClick, { capture: true })
  return () => {
    window.removeEventListener('pointerdown', onDown, { capture: true })
    window.removeEventListener('pointermove', onMove, { capture: true })
    window.removeEventListener('pointerup', onUp, { capture: true })
    window.removeEventListener('pointercancel', onCancel, { capture: true })
    window.removeEventListener('dragstart', onDragStart, { capture: true })
    window.removeEventListener('click', onClick, { capture: true })
  }
}
