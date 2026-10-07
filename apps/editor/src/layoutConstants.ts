/**
 * Shared layout numbers so docked panels don't overlap — and don't touch —
 * the title bar, toolbar, icon rail, or timeline. Centralized here rather
 * than duplicated per-component — these components' sizes are coupled (a
 * docked side panel's height depends on the title bar's/toolbar's/timeline's
 * heights), so keeping the numbers in one place is what keeps that coupling
 * from silently drifting out of sync.
 *
 * Every clearance/margin value here includes real breathing room beyond just
 * "however much space the neighboring chrome physically occupies" — a docked
 * panel should still read as floating *within* its region, not flush against
 * whatever's next to it.
 */
export const TITLE_BAR_HEIGHT = 32
export const EDGE_MARGIN = 16 // the one gap value used everywhere — title bar↔toolbar, toolbar↔sidebar, rail↔sidebar, sidebar↔screen edge, sidebar↔timeline — so no two "floating" gaps in the app read as different distances
export const TOP_CLEARANCE = 96 // title bar (32) + EDGE_MARGIN (16) + toolbar's own height (32) + EDGE_MARGIN (16)
export const TIMELINE_HEIGHT = 216 // header/transport row + ruler-with-ticks row + one row per animatable property (Translation/Rotation/Scale/FillColor)
export const BOTTOM_CLEARANCE = TIMELINE_HEIGHT + EDGE_MARGIN * 2 // timeline's own height + EDGE_MARGIN (timeline's own bottom offset) + EDGE_MARGIN (gap above it)
// State-machine "Design" mode's bottom dock (`StateMachineDock`) takes
// `TimelinePanel`'s exact same bottom-docked slot, just taller — a node
// graph needs real vertical room to lay states out and still show a
// selected box's expanded inline fields without feeling cramped, unlike the
// Timeline's fixed handful of rows.
export const STATE_MACHINE_DOCK_HEIGHT = 320
export const MIN_BOTTOM_DOCK_HEIGHT = 160
export const MAX_BOTTOM_DOCK_HEIGHT = 520
export const RAIL_WIDTH = 56 // IconRail's width — left-docked panels start after it, not at the raw screen edge
export const LEFT_DOCK_OFFSET = RAIL_WIDTH + EDGE_MARGIN

/** How close the cursor must get to a screen edge before a docked panel
 * (`useDockLayout`) snaps into that edge's region. */
export const SNAP_THRESHOLD_PX = 48

/** Clamp range for a docked panel's canvas-facing-edge resize drag
 * (`useDockLayout`'s `width` state) — narrow enough to still read its
 * contents, wide enough that it can't swallow the whole canvas. */
export const MIN_PANEL_WIDTH = 180
export const MAX_PANEL_WIDTH = 480
