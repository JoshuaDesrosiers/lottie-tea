import type { ReactNode } from 'react'
import type { Theme, UiStyle } from '../theme'
import { CanvasIcon, EffectsIcon, GroupKindIcon, MenuIcon, MoonIcon, PaneLayoutIcon, SettingsIcon, SunIcon, TestAreaIcon, TileGridIcon, WrenchIcon } from './icons'
import { RAIL_WIDTH, TITLE_BAR_HEIGHT } from '../layoutConstants'

/**
 * The slim, permanently-docked icon rail along the left screen edge (style
 * reference: cartex/LottieLab). Unlike every other panel in this app, the
 * rail is not a `DockablePanel` or a floating panel at all — there's exactly
 * one of it, it never moves, and it's always present, so giving it drag/dock
 * behavior would be complexity with no corresponding use case.
 *
 * Every button here (other than the inert `CanvasIcon`/`MenuIcon`
 * placeholders) is a dockable panel's own open/close toggle, highlighting
 * while open the same way a design tool's sidebar-icon-toggle usually does —
 * "Timeline"/"State Machine" are the bottom dock's 2 tabs (`App.tsx`'s
 * `bottomDockTab`/`bottomDockOpen` — clicking one opens the dock to that tab
 * if closed, or closes it if that tab's already the open one), the same
 * convention Components/Effects already established before Animate/State
 * Machine joined them as dockable panels instead of their own top-level
 * `TopToolbar` modes (see that component's own doc comment for why: a
 * designer legitimately wants the canvas and the Timeline/state graph open
 * together, which mutually-exclusive modes never allowed).
 *
 * The Settings button drives the same dark/light `theme` toggle
 * `TitleBar`'s own quick-access button does (`App.tsx` owns the shared
 * state, `src/theme.ts`) — a real, hand-confirmed point of confusion: a user
 * expected *this* icon specifically to be the theme toggle (it's the
 * conventional place to look for one) and found a dead placeholder instead.
 * Swapping its own icon between sun/moon (instead of a static gear) makes
 * that actually visible here too, not just in the title bar.
 *
 * Paint Asset Mode (`PaintAssetEditor.tsx`) is still a `TopToolbar` mode
 * (`MODES`), not a rail button — unlike every dockable panel here, it's a
 * full canvas-region takeover you switch into/out of, not an occasional
 * glance-at panel.
 */
export function IconRail({
  componentsOpen,
  onToggleComponents,
  effectsOpen,
  onToggleEffects,
  testAreaOpen,
  onToggleTestArea,
  layersOpen,
  onToggleLayers,
  inspectorOpen,
  onToggleInspector,
  bottomOpen,
  onToggleBottom,
  tilePaletteOpen,
  onToggleTilePalette,
  theme,
  onToggleTheme,
  uiStyle,
  onToggleUiStyle,
  paintToolsTarget,
}: {
  componentsOpen: boolean
  onToggleComponents: () => void
  effectsOpen: boolean
  onToggleEffects: () => void
  /** The Test Area panel (scene live as Toasty Lite inside mock UI). */
  testAreaOpen: boolean
  onToggleTestArea: () => void
  /** "+ add panel" — the "customize the layout" ask: each of these toggles
   * a `DockablePanel` back open/closed, same `active`/`onClick` convention
   * Components/Effects already established here, now extended to every
   * dockable panel (`DockablePanel`'s own `onClose` prop is the other half
   * of this round trip). */
  layersOpen: boolean
  onToggleLayers: () => void
  inspectorOpen: boolean
  onToggleInspector: () => void
  /** The bottom bar (Animate / State Machine / Resources / Shader /
   * Console tabs): one rail button shows or hides the whole bar, keeping
   * whichever tab was open — the tabs themselves pick what it shows. */
  bottomOpen: boolean
  onToggleBottom: () => void
  tilePaletteOpen: boolean
  onToggleTilePalette: () => void
  theme: Theme
  onToggleTheme: () => void
  /** Glass (floating panes over the canvas) vs Solid (hard panes enclosing
   * it) — independent of dark/light. See `theme.ts`'s `UiStyle`. */
  uiStyle: UiStyle
  onToggleUiStyle: () => void
  /** Paint mode: the rail keeps its place and look but shows Paint's tools
   * instead of the panel toggles — they're rendered into this slot by
   * `PaintToolPalette` (it lives inside scratch-paint's Redux store, so it
   * portals in). Undefined outside Paint. */
  paintToolsTarget?: (el: HTMLDivElement | null) => void
}) {
  return (
    <div
      className="absolute bottom-0 left-0 flex flex-col items-center gap-2 border-r border-panel-border bg-panel py-4 backdrop-blur-panel"
      style={{ width: RAIL_WIDTH, top: TITLE_BAR_HEIGHT }}
    >
      <RailButton>
        <CanvasIcon />
      </RailButton>
      {paintToolsTarget ? (
        <div ref={paintToolsTarget} className="flex flex-col items-center gap-2" />
      ) : (
        <>
      <RailButton active={layersOpen} onClick={onToggleLayers} title="Layers">
        <GroupKindIcon />
      </RailButton>
      <RailButton active={inspectorOpen} onClick={onToggleInspector} title="Inspector">
        <SettingsIcon />
      </RailButton>
      <RailButton active={tilePaletteOpen} onClick={onToggleTilePalette} title="Tile Palette">
        <TileGridIcon />
      </RailButton>
      <RailButton active={componentsOpen} onClick={onToggleComponents} title="Components">
        <WrenchIcon />
      </RailButton>
      <RailButton active={effectsOpen} onClick={onToggleEffects} title="Effects">
        <EffectsIcon />
      </RailButton>
      <RailButton active={testAreaOpen} onClick={onToggleTestArea} title="Test Area: try this scene inside a phone or desktop app">
        <TestAreaIcon />
      </RailButton>
      <RailButton active={bottomOpen} onClick={onToggleBottom} title={bottomOpen ? 'Hide the bottom bar (Animate, State Machine, Resources …)' : 'Show the bottom bar (Animate, State Machine, Resources …)'}>
        <BottomBarIcon />
      </RailButton>
        </>
      )}
      <div className="flex-1" />
      <RailButton>
        <MenuIcon />
      </RailButton>
      <RailButton active={uiStyle === 'solid'} onClick={onToggleUiStyle} title={uiStyle === 'solid' ? 'Switch to floating (glass) panels' : 'Switch to solid panels'}>
        <PaneLayoutIcon />
      </RailButton>
      <RailButton onClick={onToggleTheme} title={theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}>
        {theme === 'dark' ? <SunIcon /> : <MoonIcon />}
      </RailButton>
    </div>
  )
}

/** A window with its bottom strip filled — "the bottom bar". */
function BottomBarIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <rect x="3" y="4" width="18" height="16" rx="2.5" />
      <path d="M3 14h18" />
      <path d="M6 17h5" />
    </svg>
  )
}

function RailButton({ children, active, onClick, title }: { children: ReactNode; active?: boolean; onClick?: () => void; title?: string }) {
  return (
    <button
      onClick={onClick}
      title={title}
      className={`flex h-9 w-9 items-center justify-center rounded-lg transition-colors ${
        active ? 'bg-accent/15 text-accent' : 'text-ink-dim hover:bg-overlay/5 hover:text-ink'
      }`}
    >
      {children}
    </button>
  )
}
