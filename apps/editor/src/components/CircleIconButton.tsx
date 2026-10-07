import type { ReactNode } from 'react'

/**
 * The circular icon button used throughout the cartex-style Layers hierarchy
 * (expand/collapse chevrons, add-child buttons) — one shared component so
 * every circle button is exactly the same size/border/hover treatment,
 * matching AN1MBYT3_BUILD_PLAN.md §15's "one consistent icon set" rule at
 * the component level, not just the glyph level.
 */
export function CircleIconButton({
  children,
  onClick,
  title,
}: {
  children: ReactNode
  onClick?: () => void
  title?: string
}) {
  return (
    <button
      onClick={onClick}
      title={title}
      className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full border border-panel-border text-ink-dim transition-colors hover:border-ink-dim hover:text-ink"
    >
      {children}
    </button>
  )
}
