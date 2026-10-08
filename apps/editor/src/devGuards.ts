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
  try {
    // React 19's "Performance Tracks" also deep-diff every re-rendered
    // component's changed props to log them (`logComponentRender`). With a
    // Paint session in props (bitmap cels hold `ImageData` — millions of
    // pixel values) that walk took *seconds* after every brush stroke and
    // froze the whole editor. React only does any of it when both
    // `console.timeStamp` and `performance.measure` exist at load time, so
    // hiding `console.timeStamp` switches the tracks off (dev only —
    // production builds never had them).
    ;(console as unknown as { timeStamp?: unknown }).timeStamp = undefined
  } catch {
    // Read-only: the tracks stay on.
  }
  setInterval(() => {
    performance.clearMeasures()
    performance.clearMarks()
  }, 2000)
}

export {}
