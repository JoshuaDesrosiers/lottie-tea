import { Component, type ReactNode } from 'react'

/**
 * Last-resort safety net around the *whole* app (`main.tsx`) — without
 * this, an uncaught error anywhere blanks the entire window with nothing
 * onscreen and no way back except relaunching. Real, hand-verified trigger:
 * `PaintAssetEditor`'s own local error boundary only catches errors while
 * it's still mounted — an error thrown *during* its own unmount (switching
 * out of Paint mode, or right after Save as Resource triggers a mode
 * change) fires while that boundary is being torn down in the same commit,
 * so it can't catch its own subtree's teardown error; nothing stopped it
 * from reaching all the way up and unmounting the entire React root. This
 * boundary is the backstop for exactly that class of error, wherever it
 * comes from — not a fix for whatever specifically threw (most likely
 * paper.js's global `PaperScope` singleton interacting with a component
 * unmount), just the difference between "blank screen, relaunch" and "an
 * error screen with a reload button."
 */
export class RootErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null }
  static getDerivedStateFromError(error: Error) {
    return { error }
  }
  componentDidCatch(error: Error) {
    console.error('RootErrorBoundary caught an error that would otherwise have blanked the whole window:', error)
  }
  render() {
    if (this.state.error) {
      return (
        <div className="fixed inset-0 flex flex-col items-center justify-center gap-3 bg-canvas text-ink">
          <p className="text-sm font-medium">Something went wrong.</p>
          <p className="max-w-md text-center text-xs text-ink-dim">{this.state.error.message}</p>
          <button onClick={() => location.reload()} className="rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent/90">
            Reload
          </button>
        </div>
      )
    }
    return this.props.children
  }
}
