import { createContext, useContext } from 'react'
import type { UiStyle } from './theme'
import { EDGE_MARGIN, LEFT_DOCK_OFFSET, RAIL_WIDTH, TITLE_BAR_HEIGHT, TOP_CLEARANCE } from './layoutConstants'

/**
 * The gap/clearance numbers every pane positions itself with — style-
 * dependent, unlike the rest of `layoutConstants.ts`. Glass (the default)
 * keeps the original "floating, with breathing room on every side" values;
 * Solid zeroes every gap so docked panes sit flush against each other and
 * the window edges, which is what lets `App.tsx` carve the canvas viewport
 * out of the space *between* them instead of under them.
 *
 * A React context (provided once by `App.tsx`) rather than more module
 * constants, because these change at runtime when the user flips the style
 * — every consumer re-renders with the new numbers, no reload.
 */
export interface LayoutMetrics {
  /** Gap between any pane and its neighbor/window edge. */
  edgeMargin: number
  /** Where the top toolbar starts (directly under the title bar + gap). */
  toolbarTop: number
  /** Where docked side panes start vertically (below the toolbar + gap). */
  topClearance: number
  /** Where left-docked panes start horizontally (after the icon rail + gap). */
  leftDockOffset: number
}

/** The top toolbar's own rendered height (its `FloatingPanel`'s padding,
 * h-6 controls, and 1px border on each side) — Solid needs the exact
 * number since nothing pads around it anymore. */
const TOOLBAR_HEIGHT = 34

export function layoutMetricsFor(style: UiStyle): LayoutMetrics {
  if (style === 'solid') {
    return {
      edgeMargin: 0,
      toolbarTop: TITLE_BAR_HEIGHT,
      topClearance: TITLE_BAR_HEIGHT + TOOLBAR_HEIGHT,
      leftDockOffset: RAIL_WIDTH,
    }
  }
  return { edgeMargin: EDGE_MARGIN, toolbarTop: TITLE_BAR_HEIGHT + EDGE_MARGIN, topClearance: TOP_CLEARANCE, leftDockOffset: LEFT_DOCK_OFFSET }
}

export const LayoutMetricsContext = createContext<LayoutMetrics>(layoutMetricsFor('glass'))

export function useLayoutMetrics(): LayoutMetrics {
  return useContext(LayoutMetricsContext)
}
