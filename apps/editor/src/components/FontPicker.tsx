/**
 * Google Fonts and Material Symbols pickers (`game/googleFonts.ts`).
 *
 * - `FontPicker`: a font family field. Opens a searchable list of every
 *   Google Font (popular ones first), each name drawn in its own face —
 *   previews load only the glyphs of the name (`css2?text=`), so scrolling
 *   the list stays cheap.
 * - `IconPicker`: a searchable grid of Material Symbols icons in one of the
 *   three styles.
 *
 * Both only *choose*; the caller decides what choosing does (download the
 * TTF into the document for a Text node, write CSS for HTML UI, …).
 */
import { useEffect, useMemo, useRef, useState } from 'react'
import {
  fontCatalog,
  iconCatalog,
  iconFamily,
  ICON_STYLES,
  loadFontCss,
  loadFontPreview,
  POPULAR_FONTS,
  COMMON_ICONS,
  type FontCategory,
  type GoogleFont,
  type IconStyle,
} from '../game/googleFonts'

const CATEGORIES: { key: FontCategory | 'all'; label: string }[] = [
  { key: 'all', label: 'All' },
  { key: 'sans-serif', label: 'Sans' },
  { key: 'serif', label: 'Serif' },
  { key: 'display', label: 'Display' },
  { key: 'handwriting', label: 'Hand' },
  { key: 'monospace', label: 'Mono' },
]

/** How many rows / icons render at once (search narrows the rest). */
const FONT_ROWS = 80
const ICON_CELLS = 240

/** Closes a popover on an outside click or Escape. */
function useDismiss(open: boolean, close: () => void) {
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!open) return
    const onDown = (e: PointerEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) close()
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close()
    }
    window.addEventListener('pointerdown', onDown, true)
    window.addEventListener('keydown', onKey)
    return () => {
      window.removeEventListener('pointerdown', onDown, true)
      window.removeEventListener('keydown', onKey)
    }
  }, [open, close])
  return ref
}

/** One list row: the family name in its own face. */
function FontRow({ font, current, onPick }: { font: GoogleFont; current: boolean; onPick: () => void }) {
  useEffect(() => loadFontPreview(font.family), [font.family])
  return (
    <button
      type="button"
      onClick={onPick}
      className={`flex w-full items-baseline justify-between gap-2 rounded px-2 py-1 text-left hover:bg-overlay/10 ${current ? 'bg-accent/15 text-accent' : 'text-ink'}`}
    >
      <span className="truncate text-[15px]" style={{ fontFamily: `"${font.family}", var(--font-sans, sans-serif)` }}>
        {font.family}
      </span>
      <span className="shrink-0 text-[10px] text-ink-dim">{font.category}</span>
    </button>
  )
}

/**
 * A font family field. `value` is the current family (any name — a font
 * that isn't a Google Font just shows as text); `extra` are families to
 * list first (e.g. the document's own fonts).
 */
export function FontPicker({
  value,
  onPick,
  label = 'Font',
  busy = false,
  extra = [],
}: {
  value: string | null
  onPick: (family: string) => void
  label?: string
  /** Shows a spinner-ish "Loading…" (the caller is downloading the font). */
  busy?: boolean
  extra?: string[]
}) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [category, setCategory] = useState<FontCategory | 'all'>('all')
  const [fonts, setFonts] = useState<GoogleFont[]>(POPULAR_FONTS)
  const ref = useDismiss(open, () => setOpen(false))

  useEffect(() => {
    if (open) fontCatalog().then(setFonts)
  }, [open])
  useEffect(() => {
    if (value) loadFontPreview(value)
  }, [value])

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase()
    const extraFonts: GoogleFont[] = extra.filter((f) => !fonts.some((g) => g.family === f)).map((family) => ({ family, id: '', category: 'sans-serif', weights: [400], subset: 'latin' }))
    return [...extraFonts, ...fonts].filter((f) => (category === 'all' || f.category === category) && (!q || f.family.toLowerCase().includes(q))).slice(0, FONT_ROWS)
  }, [fonts, query, category, extra])

  return (
    <div ref={ref} className="relative flex flex-col gap-1">
      <span className="text-[11px] text-ink-dim">{label}</span>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="flex items-center justify-between rounded-md bg-overlay/5 px-2 py-1.5 text-left text-xs text-ink hover:bg-overlay/10"
      >
        <span className="truncate text-[13px]" style={{ fontFamily: value ? `"${value}", sans-serif` : undefined }}>
          {busy ? 'Loading font…' : value ?? 'Choose a font'}
        </span>
        <span className="shrink-0 whitespace-nowrap text-[10px] text-ink-dim">Google Fonts ▾</span>
      </button>
      {open && (
        <div className="absolute left-0 right-0 top-full z-50 mt-1 flex max-h-80 flex-col gap-1 rounded-lg border border-panel-border bg-[var(--solid-panel)] p-1.5 shadow-xl">
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={`Search ${fonts.length > POPULAR_FONTS.length ? fonts.length : ''} fonts…`}
            className="rounded-md bg-overlay/5 px-2 py-1 text-xs text-ink outline-none placeholder:text-ink-dim"
          />
          <div className="flex flex-wrap gap-1">
            {CATEGORIES.map((c) => (
              <button
                key={c.key}
                type="button"
                onClick={() => setCategory(c.key)}
                className={`rounded px-1.5 py-0.5 text-[10px] ${category === c.key ? 'bg-accent/20 text-accent' : 'text-ink-dim hover:bg-overlay/10'}`}
              >
                {c.label}
              </button>
            ))}
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto">
            {shown.map((f) => (
              <FontRow
                key={f.family}
                font={f}
                current={f.family === value}
                onPick={() => {
                  setOpen(false)
                  onPick(f.family)
                }}
              />
            ))}
            {shown.length === 0 && <div className="px-2 py-3 text-center text-[11px] text-ink-dim">No font matches “{query}”.</div>}
          </div>
        </div>
      )}
    </div>
  )
}

/** A searchable grid of Material Symbols icons. */
export function IconPicker({
  value,
  style,
  onPick,
  onStyle,
}: {
  value: string | null
  style: IconStyle
  onPick: (name: string) => void
  /** Omit to hide the style switch. */
  onStyle?: (style: IconStyle) => void
}) {
  const [query, setQuery] = useState('')
  const [names, setNames] = useState<string[]>(COMMON_ICONS)
  useEffect(() => {
    loadFontCss(iconFamily(style))
  }, [style])
  useEffect(() => {
    iconCatalog().then(setNames)
  }, [])
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase().replace(/\s+/g, '_')
    return (q ? names.filter((n) => n.includes(q)) : names).slice(0, ICON_CELLS)
  }, [names, query])

  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-1">
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={`Search ${names.length} icons…`}
          className="min-w-0 flex-1 rounded-md bg-overlay/5 px-2 py-1 text-xs text-ink outline-none placeholder:text-ink-dim"
        />
        {onStyle && (
          <select value={style} onChange={(e) => onStyle(e.target.value as IconStyle)} className="rounded-md bg-overlay/5 px-1 py-1 text-[11px] text-ink outline-none">
            {ICON_STYLES.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </select>
        )}
      </div>
      <div className="grid max-h-48 grid-cols-8 gap-0.5 overflow-y-auto rounded-md bg-overlay/5 p-1">
        {shown.map((name) => (
          <button
            key={name}
            type="button"
            title={name}
            onClick={() => onPick(name)}
            className={`flex aspect-square items-center justify-center overflow-hidden rounded hover:bg-overlay/15 ${name === value ? 'bg-accent/20 text-accent' : 'text-ink'}`}
            style={{ fontFamily: `"${iconFamily(style)}"`, fontSize: 20, lineHeight: 1, fontFeatureSettings: "'liga'" }}
          >
            {name}
          </button>
        ))}
      </div>
      {value && <span className="text-[10px] text-ink-dim">Icon: {value}</span>}
    </div>
  )
}
