import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react'
import { describeOrigin, gameConsole, type ConsoleEntry, type ConsoleLevel, type ConsoleOrigin } from '../game/console'

/**
 * The Console tab (Unity's Console window): what the game printed and every
 * error it hit, newest at the bottom.
 *
 * - **Filters** for logs / warnings / errors (with counts) and a search box.
 * - Each entry shows **where** — scene › layer › state : line:column — and
 *   clicking that opens the code there (`onOpenCode`).
 * - **Repeats collapse** into one row with a count (a script that throws
 *   every frame is one row, not thousands).
 * - Click an error to see its **stack** (the game's own frames, in the
 *   program's own line numbers).
 * - **Clear**, and **Clear on Play** (on by default) so each run starts
 *   clean.
 *
 * Stays visible while playing, so errors show up as they happen.
 */

const CLEAR_ON_PLAY_KEY = 'an1mbyt3.console.clearOnPlay'

/** Whether pressing Play clears the Console (the panel's checkbox). */
export function consoleClearOnPlay(): boolean {
  try {
    return localStorage.getItem(CLEAR_ON_PLAY_KEY) !== 'false'
  } catch {
    return true
  }
}

function useConsoleEntries(): readonly ConsoleEntry[] {
  // `version` changes on every add / clear, so the snapshot is stable
  // between changes (what useSyncExternalStore needs).
  useSyncExternalStore(
    (cb) => gameConsole.subscribe(cb),
    () => gameConsole.version,
  )
  return gameConsole.list()
}

const LEVEL_STYLE: Record<ConsoleLevel, { row: string; icon: string; label: string }> = {
  log: { row: '', icon: 'text-ink-dim', label: 'Logs' },
  warn: { row: 'bg-amber-400/[0.07]', icon: 'text-amber-400', label: 'Warnings' },
  error: { row: 'bg-red-500/[0.08]', icon: 'text-red-400', label: 'Errors' },
}

function LevelIcon({ level }: { level: ConsoleLevel }) {
  const cls = `mt-[1px] h-3.5 w-3.5 shrink-0 ${LEVEL_STYLE[level].icon}`
  if (level === 'error')
    return (
      <svg viewBox="0 0 16 16" className={cls} fill="currentColor" aria-label="Error">
        <circle cx="8" cy="8" r="7" />
        <path d="M5.5 5.5l5 5M10.5 5.5l-5 5" stroke="var(--color-panel, #1e1e24)" strokeWidth="1.6" strokeLinecap="round" />
      </svg>
    )
  if (level === 'warn')
    return (
      <svg viewBox="0 0 16 16" className={cls} fill="currentColor" aria-label="Warning">
        <path d="M8 1.5l7 12.5H1z" />
        <path d="M8 6v3.5M8 11.6v.4" stroke="var(--color-panel, #1e1e24)" strokeWidth="1.6" strokeLinecap="round" />
      </svg>
    )
  return (
    <svg viewBox="0 0 16 16" className={cls} fill="none" stroke="currentColor" strokeWidth="1.5" aria-label="Log">
      <path d="M4 5l3 3-3 3M8.5 11.5H12" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

export function ConsolePanel({ onOpenCode, onClose }: { onOpenCode?: (origin: ConsoleOrigin) => void; onClose?: () => void }) {
  const entries = useConsoleEntries()
  const [shown, setShown] = useState<Record<ConsoleLevel, boolean>>({ log: true, warn: true, error: true })
  const [query, setQuery] = useState('')
  const [open, setOpen] = useState<number | null>(null)
  const [clearOnPlay, setClearOnPlay] = useState(consoleClearOnPlay)
  const listRef = useRef<HTMLDivElement>(null)
  const stick = useRef(true)

  // Looking at the Console clears the "unseen errors" badge.
  useEffect(() => {
    gameConsole.markSeen()
  }, [entries.length])

  const counts = useMemo(() => {
    const c: Record<ConsoleLevel, number> = { log: 0, warn: 0, error: 0 }
    for (const e of entries) c[e.level] += e.count
    return c
  }, [entries, gameConsole.version]) // eslint-disable-line react-hooks/exhaustive-deps

  const q = query.trim().toLowerCase()
  const visible = entries.filter((e) => shown[e.level] && (!q || e.message.toLowerCase().includes(q) || describeOrigin(e.origin).toLowerCase().includes(q)))

  // Keep scrolled to the newest entry unless the user scrolled up.
  useEffect(() => {
    const el = listRef.current
    if (el && stick.current) el.scrollTop = el.scrollHeight
  })

  return (
    <div className="flex h-full min-h-0 flex-col text-[11px]">
      <div className="flex shrink-0 items-center gap-1.5 border-b border-panel-border pb-1.5">
        <button onClick={() => gameConsole.clear()} className="rounded px-2 py-1 text-ink-dim hover:bg-overlay/10 hover:text-ink">
          Clear
        </button>
        <label className="flex cursor-pointer items-center gap-1 text-ink-dim" title="Start every Play with an empty Console">
          <input
            type="checkbox"
            checked={clearOnPlay}
            onChange={(e) => {
              setClearOnPlay(e.target.checked)
              try {
                localStorage.setItem(CLEAR_ON_PLAY_KEY, String(e.target.checked))
              } catch {
                // private mode: just not remembered
              }
            }}
          />
          Clear on Play
        </label>
        <div className="mx-1 h-4 w-px bg-panel-border" />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search"
          className="w-40 rounded bg-overlay/10 px-2 py-1 text-ink outline-none placeholder:text-ink-dim focus:bg-overlay/15"
        />
        <div className="ml-auto flex items-center gap-1">
          {(['log', 'warn', 'error'] as ConsoleLevel[]).map((level) => (
            <button
              key={level}
              onClick={() => setShown((s) => ({ ...s, [level]: !s[level] }))}
              title={`${shown[level] ? 'Hide' : 'Show'} ${LEVEL_STYLE[level].label.toLowerCase()}`}
              className={`flex items-center gap-1 rounded px-1.5 py-1 ${shown[level] ? 'bg-overlay/10 text-ink' : 'text-ink-dim opacity-60 hover:opacity-100'}`}
            >
              <LevelIcon level={level} />
              <span className="tabular-nums">{counts[level] > 999 ? '999+' : counts[level]}</span>
            </button>
          ))}
          {onClose && (
            <button onClick={onClose} title="Hide the Console" className="ml-1 rounded px-1.5 py-1 text-ink-dim hover:bg-overlay/10 hover:text-ink">
              ✕
            </button>
          )}
        </div>
      </div>
      <div
        ref={listRef}
        onScroll={(e) => {
          const el = e.currentTarget
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24
        }}
        className="min-h-0 flex-1 overflow-y-auto font-mono"
      >
        {visible.length === 0 ? (
          <div className="flex h-full items-center justify-center text-ink-dim">
            {entries.length === 0 ? 'Nothing yet — console.log(…) or print(…) in a script shows up here, and so do errors.' : 'No entries match.'}
          </div>
        ) : (
          visible.map((e) => {
            const where = describeOrigin(e.origin)
            const expanded = open === e.id
            return (
              <div key={e.id} className={`border-b border-overlay/[0.06] px-1.5 py-1 ${LEVEL_STYLE[e.level].row}`}>
                <div className="flex items-start gap-1.5">
                  <LevelIcon level={e.level} />
                  <button
                    onClick={() => setOpen(expanded ? null : e.id)}
                    className={`min-w-0 flex-1 whitespace-pre-wrap break-words text-left ${e.level === 'error' ? 'text-red-300' : e.level === 'warn' ? 'text-amber-200' : 'text-ink'}`}
                    title={e.stack ? 'Show / hide the stack' : undefined}
                  >
                    {e.message}
                  </button>
                  {where && (
                    <button
                      onClick={() => onOpenCode?.(e.origin)}
                      disabled={!onOpenCode || !e.origin.state}
                      className="shrink-0 rounded px-1 font-sans text-[10px] text-accent hover:underline disabled:cursor-default disabled:text-ink-dim disabled:no-underline"
                      title={onOpenCode && e.origin.state ? 'Open this code' : undefined}
                    >
                      {where}
                    </button>
                  )}
                  {e.count > 1 && <span className="shrink-0 rounded-full bg-overlay/15 px-1.5 font-sans text-[10px] tabular-nums text-ink">{e.count > 9999 ? '9999+' : e.count}</span>}
                </div>
                {expanded && e.stack && <pre className="ml-5 mt-1 whitespace-pre-wrap text-[10px] text-ink-dim">{e.stack}</pre>}
              </div>
            )
          })
        )}
      </div>
    </div>
  )
}

/** The Playground's error pill: how many errors this run hit; click to
 * open the Console. */
export function ConsoleErrorPill({ onOpen }: { onOpen: () => void }) {
  const entries = useConsoleEntries()
  const errors = entries.reduce((n, e) => n + (e.level === 'error' ? 1 : 0), 0)
  if (errors === 0) return null
  return (
    <button
      onClick={onOpen}
      className="pointer-events-auto flex items-center gap-1.5 rounded-full border border-red-400/40 bg-red-500/85 px-3 py-1 text-[11px] font-semibold text-white shadow-lg backdrop-blur hover:bg-red-500"
      title="Open the Console"
    >
      <LevelIcon level="error" />
      {errors} {errors === 1 ? 'error' : 'errors'} — see Console
    </button>
  )
}

/** A red dot with the number of errors not yet looked at (on the tab). */
export function ConsoleBadge() {
  useConsoleEntries()
  const n = gameConsole.unseenErrors
  if (n === 0) return null
  return <span className="absolute -right-1 -top-1 min-w-[14px] rounded-full bg-red-500 px-1 text-center text-[9px] font-bold leading-[14px] text-white">{n > 99 ? '99+' : n}</span>
}
