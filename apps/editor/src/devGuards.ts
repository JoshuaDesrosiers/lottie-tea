/**
 * Development-only memory guards, imported before React (see `main.tsx`).
 *
 * React 19's development build instruments every render:
 * - `console.createTask` for each element (async stack traces in
 *   DevTools). WebView2 keeps those tasks alive, so a fast-re-rendering
 *   editor grew the WebView by tens of MB a second until it crashed with
 *   "Out of Memory" (found 2026-10-07, State Machine tab open on a
 *   mid-size scene).
 * - a `performance.measure` entry per component render ("Performance
 *   Tracks", with a deep changed-props diff) — a buffer nothing ever
 *   clears.
 *
 * Production builds do neither, so none of this ships. The cost here:
 * DevTools loses React's async component stacks, and the Performance
 * panel only sees React's tracks while it's recording.
 */
if (import.meta.env.DEV) {
  try {
    // React decides at load time whether to use it.
    ;(console as unknown as { createTask?: unknown }).createTask = undefined
  } catch {
    // Read-only in some hosts: then React keeps using it.
  }
  setInterval(() => {
    performance.clearMeasures()
    performance.clearMarks()
  }, 2000)
}

export {}
