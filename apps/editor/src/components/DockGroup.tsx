import { Fragment } from 'react'
import type { CSSProperties, ReactNode, RefObject } from 'react'
import type { DockPane, GroupState, PanelKey } from '../hooks/useDockLayout'
import { BOTTOM_CLEARANCE } from '../layoutConstants'
import { useLayoutMetrics } from '../layoutMetrics'
import { CloseIcon } from './icons'
import { FloatingPanel } from './FloatingPanel'
import { GripIcon } from './GripIcon'

interface PointerHandlers {
  onPointerDown: (event: React.PointerEvent) => void
  onPointerMove: (event: React.PointerEvent) => void
  onPointerUp: (event: React.PointerEvent) => void
}

interface PanelContent {
  title: string
  content: ReactNode
}

interface DockGroupProps {
  group: GroupState
  /** Every dockable panel's title/content, keyed by `PanelKey` — `DockGroup`
   * looks up whichever ones `group` actually references (any number, 2+),
   * rather than receiving a pre-built pair — needed now that a group can
   * reference anywhere from 2 to all 4 panels, nested 2 levels deep (a
   * split's pane can itself be a tab strip). */
  panelContent: Record<PanelKey, PanelContent>
  containerRef: RefObject<HTMLDivElement | null>
  /** Index-aligned with `group.panes` when `group.mode === 'split'` —
   * `useDockLayout`'s drop handling hit-tests these rects to figure out
   * which specific pane a dragged panel landed on. Unused in `tabs` mode. */
  paneRefs: RefObject<(HTMLDivElement | null)[]>
  onActiveTabChange: (key: PanelKey) => void
  /** Closes one tab/pane's panel outright (hides it via `App.tsx`'s
   * `panelVisible`, same as a solo `DockablePanel`'s own close button) —
   * the "still add x for docked sidebars that are tabs" ask: a grouped
   * panel used to have no way to close at all, only a solo one did. */
  onClosePanel: (key: PanelKey) => void
  headerDragHandlers: Record<PanelKey, PointerHandlers>
  makeSplitDividerHandlers: (dividerIndex: number) => PointerHandlers
  resizeHandlerProps: PointerHandlers
  bottomClearance?: number
}

function TabStrip({ members, activeTab, panelContent, headerDragHandlers, onActiveTabChange, onClosePanel }: {
  members: PanelKey[]
  activeTab: PanelKey
  panelContent: Record<PanelKey, PanelContent>
  headerDragHandlers: Record<PanelKey, PointerHandlers>
  onActiveTabChange: (key: PanelKey) => void
  onClosePanel: (key: PanelKey) => void
}) {
  return (
    <>
      <div className="mb-1.5 flex shrink-0 gap-1">
        {members.map((key) => (
          <div
            key={key}
            {...headerDragHandlers[key]}
            onClick={() => onActiveTabChange(key)}
            className={`flex touch-none items-center gap-1 rounded-md pl-2 pr-1 py-1 text-[11px] font-medium uppercase tracking-wide ${
              activeTab === key ? 'bg-overlay/10 text-ink' : 'text-ink-dim hover:text-ink'
            }`}
          >
            <span>{panelContent[key].title}</span>
            <button
              onClick={(e) => {
                e.stopPropagation()
                onClosePanel(key)
              }}
              onPointerDown={(e) => e.stopPropagation()}
              title={`Close ${panelContent[key].title}`}
              className="touch-none rounded p-0.5 hover:bg-overlay/10 hover:text-ink"
            >
              <CloseIcon />
            </button>
          </div>
        ))}
      </div>
      <div className="min-h-0 flex-1 overflow-auto">{panelContent[activeTab].content}</div>
    </>
  )
}

function PaneHeader({ title, dragHandlers, onClose }: { title: string; dragHandlers: PointerHandlers; onClose: () => void }) {
  return (
    <div {...dragHandlers} className="mb-1.5 flex shrink-0 touch-none items-center gap-2 px-0.5 py-0.5 text-ink-dim" style={{ cursor: 'grab' }}>
      <GripIcon />
      <span className="flex-1 text-[11px] font-medium uppercase tracking-wide">{title}</span>
      <button
        onClick={onClose}
        onPointerDown={(e) => e.stopPropagation()}
        title={`Close ${title}`}
        className="touch-none rounded p-0.5 hover:bg-overlay/10 hover:text-ink"
      >
        <CloseIcon />
      </button>
    </div>
  )
}

/**
 * What 2+ panels render as once dragged together into one region — a tab
 * strip (any number of members, only the active one's content shows) or a
 * resizable vertical stack of panes, where each pane is either a single
 * panel or (the "hybrid" shape) its own nested tab strip sharing that pane's
 * space. See `useDockLayout`'s doc comment for how a panel gets here and how
 * it leaves again (dragging a header far enough away detaches it).
 */
export function DockGroup({ group, panelContent, containerRef, paneRefs, onActiveTabChange, onClosePanel, headerDragHandlers, makeSplitDividerHandlers, resizeHandlerProps, bottomClearance = BOTTOM_CLEARANCE }: DockGroupProps) {
  const canvasFacingEdge: 'left' | 'right' = group.region === 'left' ? 'right' : 'left'
  const m = useLayoutMetrics()
  const style: CSSProperties = {
    position: 'absolute',
    top: m.topClearance,
    width: group.width,
    height: `calc(100vh - ${m.topClearance + bottomClearance}px)`,
    ...(group.region === 'left' ? { left: m.leftDockOffset } : { right: m.edgeMargin }),
  }

  return (
    <FloatingPanel ref={containerRef} style={style} className="relative flex flex-col p-2">
      {group.mode === 'tabs' ? (
        <TabStrip members={group.members} activeTab={group.activeTab} panelContent={panelContent} headerDragHandlers={headerDragHandlers} onActiveTabChange={onActiveTabChange} onClosePanel={onClosePanel} />
      ) : (
        <div className="flex min-h-0 flex-1 flex-col">
          {group.panes.map((pane: DockPane, index) => (
            <Fragment key={pane.kind === 'leaf' ? pane.key : pane.members.join('+')}>
              <div
                ref={(el) => {
                  paneRefs.current[index] = el
                }}
                style={{ flexBasis: `${group.ratios[index] * 100}%` }}
                className="flex min-h-0 flex-col overflow-hidden"
              >
                {pane.kind === 'leaf' ? (
                  <>
                    <PaneHeader title={panelContent[pane.key].title} dragHandlers={headerDragHandlers[pane.key]} onClose={() => onClosePanel(pane.key)} />
                    <div className="min-h-0 flex-1 overflow-auto">{panelContent[pane.key].content}</div>
                  </>
                ) : (
                  <TabStrip members={pane.members} activeTab={pane.activeTab} panelContent={panelContent} headerDragHandlers={headerDragHandlers} onActiveTabChange={onActiveTabChange} onClosePanel={onClosePanel} />
                )}
              </div>
              {index < group.panes.length - 1 && (
                <div {...makeSplitDividerHandlers(index)} className="group my-1 h-2.5 shrink-0 touch-none" style={{ cursor: 'ns-resize' }}>
                  <div className="mt-[5px] h-px w-full bg-transparent group-hover:bg-accent/70 group-active:bg-accent" />
                </div>
              )}
            </Fragment>
          ))}
        </div>
      )}

      <div
        {...resizeHandlerProps}
        className="group absolute top-0 z-10 h-full w-2.5 touch-none"
        style={{ [canvasFacingEdge]: -5, cursor: 'ew-resize' }}
      >
        <div className="mx-auto h-full w-px bg-transparent group-hover:bg-accent/70 group-active:bg-accent" />
      </div>
    </FloatingPanel>
  )
}
