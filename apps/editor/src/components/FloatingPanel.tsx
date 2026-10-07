import { forwardRef } from 'react'
import type { CSSProperties, ReactNode } from 'react'

/**
 * Shared shell for every floating panel (layers, inspector, timeline, toolbar):
 * a translucent, blurred overlay on top of the canvas rather than a rigid
 * fixed dock that permanently shrinks the canvas's pixel space
 * (AN1MBYT3_BUILD_PLAN.md §15's "Procreate polish" axis). Centralizing the
 * blur/border/radius treatment here is what keeps every panel visually
 * consistent — panel-specific components should only add their own content
 * and positioning, not redefine this surface styling per panel.
 *
 * A `forwardRef` (rather than a plain function component) because
 * `DockablePanel` needs a real DOM ref on this exact element — its drag hook
 * measures this panel's own `getBoundingClientRect()` to compute drag/snap
 * math, so the ref has to point at the actual positioned/sized box, not a
 * wrapper div around it.
 */
export const FloatingPanel = forwardRef<
  HTMLDivElement,
  { className?: string; style?: CSSProperties; children: ReactNode }
>(function FloatingPanel({ className = '', style, children }, ref) {
  return (
    <div
      ref={ref}
      style={style}
      // `pane` — the hook `index.css`'s Solid style (`data-style="solid"`)
      // targets to strip this surface's radius/shadow/blur back to a hard,
      // opaque pane. Kept as a marker class rather than conditional
      // Tailwind classes here so the style switch is pure CSS (no re-render),
      // same approach the dark/light `data-theme` switch already uses.
      className={`pane pointer-events-auto rounded-xl border border-panel-border bg-panel shadow-2xl backdrop-blur-panel ${className}`}
    >
      {children}
    </div>
  )
})
