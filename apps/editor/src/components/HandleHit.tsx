/**
 * `<HandleHit>` — wraps one small SVG handle (a `<circle>` / `<rect>` with
 * pointer handlers) and puts an invisible, bigger circle under it carrying
 * the same handlers, so the handle is easy to grab with a finger or stylus
 * (and a little easier with a mouse). The visible handle is unchanged and
 * still on top, so where two handles sit close together the one actually
 * drawn under the pointer wins.
 */
import { Children, isValidElement, type PointerEvent, type MouseEvent, type ReactElement } from 'react'

/** Hit radius in screen px: bigger on touch / pen-first screens. */
function hitRadius(): number {
  if (typeof window === 'undefined') return 10
  return window.matchMedia?.('(any-pointer: coarse)').matches ? 16 : 10
}

interface HandleProps {
  cx?: number | string
  cy?: number | string
  x?: number | string
  y?: number | string
  width?: number | string
  height?: number | string
  onPointerDown?: (e: PointerEvent<SVGElement>) => void
  onDoubleClick?: (e: MouseEvent<SVGElement>) => void
  className?: string
}

export function HandleHit({ children }: { children: ReactElement }) {
  const child = Children.only(children)
  if (!isValidElement(child)) return children
  const p = child.props as HandleProps
  // Circle: its centre; rect: its middle.
  const cx = p.cx !== undefined ? Number(p.cx) : Number(p.x ?? 0) + Number(p.width ?? 0) / 2
  const cy = p.cy !== undefined ? Number(p.cy) : Number(p.y ?? 0) + Number(p.height ?? 0) / 2
  const cursor = /cursor-(grab|move|pointer)/.exec(p.className ?? '')?.[1] ?? 'grab'
  return (
    <>
      <circle cx={cx} cy={cy} r={hitRadius()} fill="transparent" stroke="none" className="pointer-events-auto" style={{ cursor, touchAction: 'none' }} onPointerDown={p.onPointerDown} onDoubleClick={p.onDoubleClick} />
      {child}
    </>
  )
}
