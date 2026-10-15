import { getCurrentWindow } from '@tauri-apps/api/window'
import type { Theme } from '../theme'
import { CloseIcon, MaximizeIcon, MinimizeIcon, MoonIcon, SunIcon } from './icons'
import { TITLE_BAR_HEIGHT } from '../layoutConstants'

/**
 * Custom title bar replacing the OS-native one (`decorations: false` in
 * `src-tauri/tauri.conf.json`) — a native title bar's light/OS-themed chrome
 * clashes with the dark floating-panel UI everywhere else in the window;
 * this bar matches it instead.
 *
 * `data-tauri-drag-region` on the label area (not the whole bar) is what
 * lets the OS window-manager treat that region as a drag handle — it's
 * deliberately not on the button group's container, so clicking a window
 * control doesn't also start a drag.
 *
 * `isTauri` guards every `getCurrentWindow()` call because this component
 * also renders when the app is opened as a plain browser tab during `npm run
 * dev` (outside the Tauri webview, e.g. for quick UI iteration without a
 * Rust rebuild) — `@tauri-apps/api` has nothing to talk to there, so the
 * buttons become inert rather than throwing.
 *
 * `theme`/`onToggleTheme` are owned by `App.tsx` (`src/theme.ts`), not this
 * component — `IconRail`'s own Settings button drives the exact same state
 * (a real, hand-confirmed point of confusion: a user expected the Settings
 * icon itself to be the theme toggle and found this title-bar one instead),
 * so both need the same lifted-up state rather than two independent copies.
 */
export function TitleBar({ theme, onToggleTheme }: { theme: Theme; onToggleTheme: () => void }) {
  const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

  const minimize = () => isTauri && getCurrentWindow().minimize()
  const toggleMaximize = () => isTauri && getCurrentWindow().toggleMaximize()
  const close = () => isTauri && getCurrentWindow().close()

  return (
    <div
      className="absolute left-0 right-0 top-0 z-50 flex items-center justify-between border-b border-panel-border bg-panel px-2 backdrop-blur-panel"
      style={{ height: TITLE_BAR_HEIGHT }}
    >
      <div data-tauri-drag-region className="flex h-full flex-1 items-center">
        <span className="select-none text-xs font-medium tracking-wide text-ink-dim">toasty</span>
      </div>

      <div className="flex items-center gap-1">
        <button
          onClick={onToggleTheme}
          title={theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
          className="flex h-7 w-9 items-center justify-center rounded-md text-ink-dim hover:bg-overlay/5 hover:text-ink"
        >
          {theme === 'dark' ? <SunIcon /> : <MoonIcon />}
        </button>
        {/* Window buttons only in the desktop app (a browser tab has its own). */}
        {isTauri && (
          <>
            <button
              onClick={minimize}
              className="flex h-7 w-9 items-center justify-center rounded-md text-ink-dim hover:bg-overlay/5 hover:text-ink"
            >
              <MinimizeIcon />
            </button>
            <button
              onClick={toggleMaximize}
              className="flex h-7 w-9 items-center justify-center rounded-md text-ink-dim hover:bg-overlay/5 hover:text-ink"
            >
              <MaximizeIcon />
            </button>
            <button
              onClick={close}
              className="flex h-7 w-9 items-center justify-center rounded-md text-ink-dim hover:bg-red-500 hover:text-white"
            >
              <CloseIcon />
            </button>
          </>
        )}
      </div>
    </div>
  )
}
