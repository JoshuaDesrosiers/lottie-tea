import { forwardRef } from 'react'
import type { CSSProperties, ReactNode } from 'react'
import type { DockRegion } from '../hooks/useDockLayout'
import { BOTTOM_CLEARANCE } from '../layoutConstants'
import { useLayoutMetrics, type LayoutMetrics } from '../layoutMetrics'
import { CloseIcon } from './icons'
import { FloatingPanel } from './FloatingPanel'
import { GripIcon } from './GripIcon'

/** Inline style for whichever state the panel is in — floating at its
 * natural content size, or docked and stretched to fill its edge's region
 * between the toolbar and the timeline. Docked panels fill that full height
 * (not shrink-to-content) so they read as a real dock, not a stray floating
 * box — `TOP_CLEARANCE`/`BOTTOM_CLEARANCE`/`EDGE_MARGIN`/`LEFT_DOCK_OFFSET`
 * all include real breathing room, not just "however much space the toolbar/
 * rail/timeline physically occupy", so a docked panel never touches them —
 * "floating" still applies to a docked panel, it just floats *within* its
 * region instead of anywhere on screen. Kept as one function (rather than
 * splitting per-region) so the floating/docked branch is visible in one place. */
function regionStyle(region: DockRegion, position: { x: number; y: number }, width: number, bottomClearance: number, m: LayoutMetrics): CSSProperties {
  if (region === 'left') {
    return {
      position: 'absolute',
      left: m.leftDockOffset,
      top: m.topClearance,
      width,
      height: `calc(100vh - ${m.topClearance + bottomClearance}px)`,
    }
  }
  if (region === 'right') {
    return {
      position: 'absolute',
      right: m.edgeMargin,
      top: m.topClearance,
      width,
      height: `calc(100vh - ${m.topClearance + bottomClearance}px)`,
    }
  }
  return { position: 'absolute', left: position.x, top: position.y, width }
}

interface PointerHandlers {
  onPointerDown: (event: React.PointerEvent) => void
  onPointerMove: (event: React.PointerEvent) => void
  onPointerUp: (event: React.PointerEvent) => void
}

/**
 * A floating panel that docks to the left/right screen edge when dragged
 * near it (live, while dragging — not just on release) and undocks back to
 * its normal floating size when dragged away from the edge again.
 *
 * Purely presentational/controlled now — `region`/`position`/`width` and
 * both pointer-handler sets come from `useDockLayout` (`App.tsx`), not an
 * internally-owned hook, because dock state has to live above *both*
 * dockable panels at once for drag-to-tab/drag-to-split to work (see
 * `useDockLayout`'s doc comment for why a per-panel-local hook couldn't do
 * that). `ref` is still a real DOM ref forwarded through to the underlying
 * `FloatingPanel` — `useDockLayout`'s drag math measures it directly via
 * `getBoundingClientRect()`, same reasoning the old per-panel hook had.
 *
 * Only left/right regions are implemented — `Layers`/`Inspector` are the
 * only panels that dock (`Timeline` is permanently bottom-docked with no
 * dragging at all, `Tools` floats freely with no docking) — so a `'top'`/
 * `'bottom'` region isn't built here; add it if/when a panel actually needs it.
 *
 * The docked panel's canvas-facing edge (the edge bordering the canvas, not
 * the screen edge — the right edge for a left-docked panel, the left edge
 * for a right-docked one) is a drag-to-resize handle, same convention every
 * other dockable-panel system (VSCode, most design tools) uses.
 */
export const DockablePanel = forwardRef<
  HTMLDivElement,
  {
    title: string
    region: DockRegion
    position: { x: number; y: number }
    width: number
    dragHandleProps: PointerHandlers
    resizeHandleProps: PointerHandlers
    bottomClearance?: number
    className?: string
    /** Renders a close (X) button at the title row's trailing end when set —
     * the "customize the layout" ask: closing a panel here only hides it
     * (`App.tsx` tracks per-panel visibility), reopened via the matching
     * `IconRail` toggle, same "boolean + rail button" convention the
     * Components/Effects panels already established before any panel here
     * was closable. */
    onClose?: () => void
    children: ReactNode
  }
>(function DockablePanel({ title, region, position, width, dragHandleProps, resizeHandleProps, bottomClearance = BOTTOM_CLEARANCE, className = '', onClose, children }, ref) {
  const canvasFacingEdge: 'left' | 'right' | null = region === 'left' ? 'right' : region === 'right' ? 'left' : null
  const metrics = useLayoutMetrics()

  return (
    <FloatingPanel ref={ref} style={regionStyle(region, position, width, bottomClearance, metrics)} className={`relative flex flex-col ${className}`}>
      <div
        {...dragHandleProps}
        className="mb-1.5 flex shrink-0 touch-none items-center gap-2 rounded-md px-0.5 py-0.5 text-ink-dim active:cursor-grabbing"
        style={{ cursor: 'grab' }}
      >
        <GripIcon />
        <span className="flex-1 text-[11px] font-medium uppercase tracking-wide">{title}</span>
        {onClose && (
          <button
            onClick={onClose}
            onPointerDown={(e) => e.stopPropagation()}
            title={`Close ${title}`}
            className="touch-none rounded p-0.5 hover:bg-overlay/10 hover:text-ink"
          >
            <CloseIcon />
          </button>
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-auto">{children}</div>
      {canvasFacingEdge && (
        <div
          {...resizeHandleProps}
          className="group absolute top-0 z-10 h-full w-2.5 touch-none"
          style={{ [canvasFacingEdge]: -5, cursor: 'ew-resize' }}
        >
          <div className="mx-auto h-full w-px bg-transparent group-hover:bg-accent/70 group-active:bg-accent" />
        </div>
      )}
    </FloatingPanel>
  )
})
