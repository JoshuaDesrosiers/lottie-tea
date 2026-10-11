/**
 * Small dropdown menus for the top toolbar's 3D view tools: a generic
 * `ToolbarMenu` (a label that opens a list of actions) and the view preset
 * picker (`ViewPresetMenu`: perspective / front / back / left / right /
 * top / bottom). The list is portalled to `document.body` with fixed
 * positioning, so the toolbar's own overflow never clips it.
 */
import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'

export interface ToolbarMenuItem {
  label: string
  title?: string
  onClick: () => void
}

export function ToolbarMenu({ label, items, title }: { label: string; items: ToolbarMenuItem[]; title?: string }) {
  const [rect, setRect] = useState<DOMRect | null>(null)
  const button = useRef<HTMLButtonElement>(null)
  const list = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!rect) return
    const away = (e: PointerEvent) => {
      const t = e.target as Node
      if (button.current?.contains(t) || list.current?.contains(t)) return
      setRect(null)
    }
    window.addEventListener('pointerdown', away)
    return () => window.removeEventListener('pointerdown', away)
  }, [rect])
  return (
    <>
      <button
        ref={button}
        title={title}
        onClick={(e) => setRect(rect ? null : e.currentTarget.getBoundingClientRect())}
        className={`flex items-center gap-1 rounded px-2 py-0.5 text-[11px] ${rect ? 'bg-overlay/10 text-ink' : 'text-ink-dim hover:text-ink'}`}
      >
        {label}
        <span className="text-[8px] opacity-70">▼</span>
      </button>
      {rect &&
        createPortal(
          <div
            ref={list}
            className="fixed z-[100] flex min-w-40 flex-col rounded-lg border border-panel-border bg-panel p-1 text-[11px] shadow-2xl backdrop-blur-panel"
            style={{ left: rect.left, top: rect.bottom + 4 }}
          >
            {items.map((item) => (
              <button
                key={item.label}
                title={item.title}
                onClick={() => {
                  setRect(null)
                  item.onClick()
                }}
                className="rounded px-2 py-1 text-left text-ink hover:bg-accent/15"
              >
                {item.label}
              </button>
            ))}
          </div>,
          document.body,
        )}
    </>
  )
}

/** The 3D view's angle presets (also Numpad 1 / 3 / 7 / 5). */
export function ViewPresetMenu({ onPick }: { onPick: (preset: string) => void }) {
  const presets: [string, string][] = [
    ['persp', 'Perspective (Numpad 5)'],
    ['front', 'Front (Numpad 1)'],
    ['back', 'Back (Ctrl+Numpad 1)'],
    ['right', 'Right (Numpad 3)'],
    ['left', 'Left'],
    ['top', 'Top (Numpad 7)'],
    ['bottom', 'Bottom'],
  ]
  return <ToolbarMenu label="View" title="Look at the scene from a preset angle" items={presets.map(([key, label]) => ({ label, onClick: () => onPick(key) }))} />
}
