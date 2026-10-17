/**
 * `<Select>` — the editor's one dropdown. A drop-in for a native `<select>`:
 * same `value` / `onChange={(e) => … e.target.value}` / `<option>` /
 * `<optgroup>` children, so existing call sites switch by renaming the tag.
 * Native selects use the OS's own chrome (out of place in this UI) and
 * can't be styled; this one:
 *
 * - renders **top-level** (a portal into `document.body`, `position:
 *   fixed`) so no panel, dock or canvas clips it;
 * - is **placement-aware**: opens below the trigger, or above it when
 *   there's more room there, with its height capped to the space available
 *   and the rest scrollable — so a list never runs off the screen;
 * - keeps the current option in view, supports ↑ ↓ Enter Esc and Home/End,
 *   and offers a filter box on long lists (12+ options).
 *
 * `placeMenu` (also used by `Dropdown` / other popovers) is the shared
 * placement rule.
 */
import { Children, isValidElement, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type ReactElement, type ReactNode } from 'react'
import { createPortal } from 'react-dom'

/** Where a menu of `want` × `minWidth` should sit next to `anchor`. */
export function placeMenu(anchor: DOMRect, want: number, minWidth = 0): { top?: number; bottom?: number; left: number; width: number; maxHeight: number } {
  const margin = 8
  const vh = window.innerHeight
  const vw = window.innerWidth
  const below = vh - anchor.bottom - margin
  const above = anchor.top - margin
  const width = Math.min(Math.max(anchor.width, minWidth), vw - margin * 2)
  const left = Math.max(margin, Math.min(anchor.left, vw - width - margin))
  if (below >= Math.min(want, 220) || below >= above) return { top: anchor.bottom + 4, left, width, maxHeight: Math.max(80, below - 4) }
  return { bottom: vh - anchor.top + 4, left, width, maxHeight: Math.max(80, above - 4) }
}

interface Opt {
  value: string
  label: ReactNode
  text: string
  group: string | null
  disabled: boolean
  className?: string
}

function textOf(node: ReactNode): string {
  if (node == null || typeof node === 'boolean') return ''
  if (typeof node === 'string' || typeof node === 'number') return String(node)
  if (Array.isArray(node)) return node.map(textOf).join('')
  if (isValidElement(node)) return textOf((node.props as { children?: ReactNode }).children)
  return ''
}

/** Flattens `<option>` / `<optgroup>` children (fragments and arrays too). */
function collect(children: ReactNode, group: string | null, out: Opt[]) {
  Children.forEach(children, (child) => {
    if (!isValidElement(child)) return
    const el = child as ReactElement<{ value?: string | number; label?: string; children?: ReactNode; disabled?: boolean; className?: string }>
    if (el.type === 'optgroup') collect(el.props.children, el.props.label ?? null, out)
    else if (el.type === 'option') {
      const label = el.props.children
      out.push({ value: String(el.props.value ?? textOf(label)), label, text: textOf(label), group, disabled: !!el.props.disabled, className: el.props.className })
    } else if (el.props.children !== undefined) collect(el.props.children, group, out)
  })
}

export interface SelectProps {
  value: string | number | undefined
  onChange?: (event: { target: { value: string } }) => void
  children?: ReactNode
  className?: string
  style?: CSSProperties
  title?: string
  disabled?: boolean
  /** Shown when nothing matches `value`. */
  placeholder?: string
  /** Minimum menu width (px); the menu is at least as wide as the trigger. */
  menuWidth?: number
  'aria-label'?: string
  /** Forwarded to the trigger (e.g. to stop a drag on a graph node). */
  onPointerDown?: (event: React.PointerEvent<HTMLButtonElement>) => void
}

export function Select({ value, onChange, children, className, style, title, disabled, placeholder, menuWidth = 160, ...rest }: SelectProps) {
  const options = useMemo(() => {
    const out: Opt[] = []
    collect(children, null, out)
    return out
  }, [children])
  const current = options.find((o) => o.value === String(value ?? ''))
  const [open, setOpen] = useState(false)
  const [filter, setFilter] = useState('')
  const [active, setActive] = useState(0)
  const [pos, setPos] = useState<ReturnType<typeof placeMenu> | null>(null)
  const button = useRef<HTMLButtonElement>(null)
  const menu = useRef<HTMLDivElement>(null)
  const list = useRef<HTMLDivElement>(null)
  const searchable = options.length >= 12
  const shown = filter ? options.filter((o) => o.text.toLowerCase().includes(filter.toLowerCase())) : options

  const place = () => {
    const r = button.current?.getBoundingClientRect()
    if (r) setPos(placeMenu(r, Math.min(320, options.length * 26 + (searchable ? 40 : 8)), menuWidth))
  }
  const choose = (o: Opt | undefined) => {
    if (!o || o.disabled) return
    setOpen(false)
    if (o.value !== String(value ?? '')) onChange?.({ target: { value: o.value } })
    button.current?.focus()
  }

  useEffect(() => {
    if (!open) return
    const down = (e: PointerEvent) => {
      const t = e.target as Node
      if (button.current?.contains(t) || menu.current?.contains(t)) return
      setOpen(false)
    }
    window.addEventListener('pointerdown', down, true)
    window.addEventListener('resize', place)
    window.addEventListener('scroll', place, true)
    return () => {
      window.removeEventListener('pointerdown', down, true)
      window.removeEventListener('resize', place)
      window.removeEventListener('scroll', place, true)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  // Keep the active row in view.
  useLayoutEffect(() => {
    if (!open) return
    list.current?.querySelector<HTMLElement>(`[data-i="${active}"]`)?.scrollIntoView({ block: 'nearest' })
  }, [open, active, filter])

  const openMenu = () => {
    if (disabled) return
    place()
    setFilter('')
    setActive(Math.max(0, options.findIndex((o) => o === current)))
    setOpen(true)
  }

  const onKey = (e: React.KeyboardEvent) => {
    if (!open) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp' || e.key === 'Enter' || e.key === ' ') {
        e.preventDefault()
        openMenu()
      }
      return
    }
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      setOpen(false)
    } else if (e.key === 'ArrowDown') {
      e.preventDefault()
      setActive((i) => Math.min(shown.length - 1, i + 1))
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      setActive((i) => Math.max(0, i - 1))
    } else if (e.key === 'Home') setActive(0)
    else if (e.key === 'End') setActive(shown.length - 1)
    else if (e.key === 'Enter') {
      e.preventDefault()
      choose(shown[active])
    }
  }

  let lastGroup: string | null = null
  return (
    <>
      <button
        ref={button}
        type="button"
        disabled={disabled}
        title={title}
        aria-label={rest['aria-label']}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => (open ? setOpen(false) : openMenu())}
        onKeyDown={onKey}
        onPointerDown={rest.onPointerDown}
        className={`inline-flex min-w-0 items-center justify-between gap-1 text-left ${className ?? ''}`}
        style={style}
      >
        <span className="min-w-0 flex-1 truncate">{current ? current.label : <span className="opacity-60">{placeholder ?? ''}</span>}</span>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" className="shrink-0 opacity-60">
          <path d="M2 3.5 5 6.5 8 3.5" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>
      {open &&
        pos &&
        createPortal(
          <div
            ref={menu}
            role="listbox"
            onKeyDown={onKey}
            className="flex flex-col overflow-hidden rounded-md border border-panel-border bg-solid-panel text-ink shadow-2xl"
            style={{ position: 'fixed', zIndex: 10000, top: pos.top, bottom: pos.bottom, left: pos.left, width: pos.width, maxHeight: pos.maxHeight }}
          >
            {searchable && (
              <input
                autoFocus
                value={filter}
                onChange={(e) => {
                  setFilter(e.target.value)
                  setActive(0)
                }}
                placeholder="Filter…"
                className="m-1 rounded border border-panel-border bg-transparent px-1.5 py-1 text-xs text-ink outline-none"
              />
            )}
            <div ref={list} className="min-h-0 flex-1 overflow-y-auto py-1">
              {shown.length === 0 && <div className="px-2 py-1 text-xs text-ink-dim">No matches</div>}
              {shown.map((o, i) => {
                const header = o.group !== lastGroup && o.group ? o.group : null
                lastGroup = o.group
                return (
                  <div key={`${o.group ?? ''}:${o.value}:${i}`}>
                    {header && <div className="px-2 pb-0.5 pt-1.5 text-[10px] font-semibold uppercase tracking-wide text-ink-dim">{header}</div>}
                    <div
                      data-i={i}
                      role="option"
                      aria-selected={o === current}
                      onPointerEnter={() => setActive(i)}
                      onClick={() => choose(o)}
                      className={`cursor-pointer truncate px-2 py-1 text-xs ${o.disabled ? 'opacity-40' : ''} ${i === active ? 'bg-overlay/10' : ''} ${o === current ? 'font-semibold text-accent' : ''}`}
                    >
                      {o.label}
                    </div>
                  </div>
                )
              })}
            </div>
          </div>,
          document.body,
        )}
    </>
  )
}

/** True when `target` is inside `root` or inside any open `Popover` — what
 * outside-click handlers check now that menus render in a portal. */
export function insideOrPopover(root: Node | null, target: EventTarget | null): boolean {
  const t = target as Node | null
  if (!t) return false
  if (root?.contains(t)) return true
  return !!(t as Element).closest?.('[data-popover]')
}

/** A menu / popover rendered top-level next to `anchor`, placed like
 * `Select`'s menu: below, or above when there's more room, height capped
 * to the space available (scrolls), clamped inside the window. */
export function Popover({ anchor, children, className, width, minWidth = 0 }: { anchor: React.RefObject<HTMLElement | null>; children: ReactNode; className?: string; width?: number; minWidth?: number }) {
  const [pos, setPos] = useState<ReturnType<typeof placeMenu> | null>(null)
  const box = useRef<HTMLDivElement>(null)
  useLayoutEffect(() => {
    const place = () => {
      const r = anchor.current?.getBoundingClientRect()
      if (!r) return
      const want = box.current?.scrollHeight ?? 240
      const p = placeMenu(r, want, width ?? minWidth)
      setPos(width ? { ...p, width: Math.min(width, window.innerWidth - 16), left: Math.max(8, Math.min(r.left, window.innerWidth - width - 8)) } : p)
    }
    place()
    window.addEventListener('resize', place)
    window.addEventListener('scroll', place, true)
    return () => {
      window.removeEventListener('resize', place)
      window.removeEventListener('scroll', place, true)
    }
  }, [anchor, width, minWidth])
  return createPortal(
    <div
      ref={box}
      data-popover=""
      className={`overflow-y-auto ${className ?? ''}`}
      style={{ position: 'fixed', zIndex: 10000, visibility: pos ? 'visible' : 'hidden', top: pos?.top, bottom: pos?.bottom, left: pos?.left ?? 0, width: pos?.width, maxHeight: pos?.maxHeight }}
    >
      {children}
    </div>,
    document.body,
  )
}
