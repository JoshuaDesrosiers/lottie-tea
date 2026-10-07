import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import PropTypes from 'prop-types'

/**
 * A minimal drop-in replacement for `react-popover` (the 0.5.x npm
 * package) inside the vendored scratch-paint tree (`src/vendor/scratch-paint`
 * — see `PaintAssetEditor.tsx`'s own doc comment). That package's whole
 * positioning logic hinges on `ReactDOM.findDOMNode(this)` to measure its
 * anchor child, an API React 19 removed entirely — there's no drop-in
 * upgrade of the original (it's unmaintained since long before React 18),
 * so this reimplements only the actual prop surface scratch-paint uses
 * (`components/color-indicator.jsx`, `components/dropdown/dropdown.jsx`):
 * `body`, `isOpen`, `preferPlace` ("above"/"below"/"left"/"right"),
 * `onOuterAction`, and a single `children` anchor element.
 *
 * Positioning is simpler than the original's full auto-placement/collision
 * algorithm: it just anchors `body` to whichever side `preferPlace` names,
 * measured off `children`'s own wrapping element (a real ref on an element
 * *we* render, not a ref forced onto an arbitrary caller-supplied child —
 * exactly what made the original need `findDOMNode` in the first place).
 * Rendered through a portal to `document.body` so it isn't clipped by any
 * `overflow: hidden` ancestor, matching the original's own `appendTarget`
 * behavior.
 */
export default function Popover({ body, isOpen, preferPlace, onOuterAction, children }) {
  const anchorRef = useRef(null)
  const bodyRef = useRef(null)
  const [rect, setRect] = useState(null)

  useEffect(() => {
    if (!isOpen || !anchorRef.current) {
      setRect(null)
      return
    }
    setRect(anchorRef.current.getBoundingClientRect())
  }, [isOpen])

  useEffect(() => {
    if (!isOpen || !onOuterAction) return
    const handlePointerDown = (event) => {
      const target = event.target
      if (anchorRef.current?.contains(target) || bodyRef.current?.contains(target)) return
      onOuterAction()
    }
    document.addEventListener('mousedown', handlePointerDown)
    return () => document.removeEventListener('mousedown', handlePointerDown)
  }, [isOpen, onOuterAction])

  const place = preferPlace || 'below'
  const style = rect
    ? {
        position: 'fixed',
        zIndex: 10000,
        ...(place === 'above'
          ? { left: rect.left, bottom: window.innerHeight - rect.top }
          : place === 'left'
            ? { right: window.innerWidth - rect.left, top: rect.top }
            : place === 'right'
              ? { left: rect.right, top: rect.top }
              : { left: rect.left, top: rect.bottom }),
      }
    : null

  return (
    <>
      <div ref={anchorRef} style={{ display: 'inline-block' }}>
        {children}
      </div>
      {isOpen &&
        rect &&
        createPortal(
          // `Popover-body`: matches the *original* react-popover's own
          // global class name convention — `color-picker.module.css`
          // already has `:global(.Popover-body) { background: ...; border:
          // ...; box-shadow: ...; }` targeting exactly this name (there's
          // no reason to duplicate that styling here when the existing
          // rule already does it, unstyled only because nothing rendered
          // that class name anymore once this replaced the original
          // library). The original's separate `.Popover-tipShape` pointer
          // triangle isn't reproduced — a real, minor visual simplification
          // of this pass, not an oversight.
          <div ref={bodyRef} className="Popover-body" style={style}>
            {body}
          </div>,
          document.body,
        )}
    </>
  )
}

Popover.propTypes = {
  body: PropTypes.node,
  isOpen: PropTypes.bool,
  preferPlace: PropTypes.oneOf(['above', 'below', 'left', 'right']),
  onOuterAction: PropTypes.func,
  children: PropTypes.node,
}
