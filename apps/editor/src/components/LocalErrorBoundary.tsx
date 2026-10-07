import { Component, type ReactNode } from 'react'

/**
 * A scoped error boundary for one feature area (State Machine dock, Paint
 * mode, etc.) — catches a render-phase crash in just that subtree instead of
 * escaping all the way to `RootErrorBoundary` and blanking the whole window.
 * Real, hand-verified trigger this exists for: a malformed compiled Program
 * (a Blocks/Code script) reaching a render path that assumes well-formed
 * data — one bad script shouldn't take the rest of the editor down with it.
 *
 * `resetKey` clears a caught error whenever it changes — pass something
 * that identifies "a fresh attempt is reasonable now" (e.g. the current
 * mode, or a doc revision counter). Without it, a boundary that's switched
 * away from and back to the SAME mounted subtree would otherwise stay stuck
 * showing the old error forever (React error boundaries don't self-clear).
 * The "Try again" button clears it unconditionally, for the case the
 * underlying cause was already fixed without `resetKey` changing (e.g. the
 * author corrected the script that crashed it).
 */
export class LocalErrorBoundary extends Component<{ children: ReactNode; label: string; resetKey?: unknown }, { error: Error | null }> {
  state = { error: null as Error | null }
  static getDerivedStateFromError(error: Error) {
    return { error }
  }
  componentDidCatch(error: Error) {
    console.error(`${this.props.label} crashed — isolated, the rest of the app keeps running:`, error)
  }
  componentDidUpdate(prevProps: { resetKey?: unknown }) {
    if (this.state.error && prevProps.resetKey !== this.props.resetKey) {
      this.setState({ error: null })
    }
  }
  render() {
    if (this.state.error) {
      return (
        <div className="flex h-full flex-col items-center justify-center gap-2 p-4 text-center">
          <p className="text-sm font-medium text-red-300">{this.props.label} hit an error.</p>
          <p className="max-w-sm text-xs text-ink-dim">{this.state.error.message}</p>
          <button onClick={() => this.setState({ error: null })} className="rounded-md bg-overlay/10 px-3 py-1.5 text-xs text-ink hover:bg-overlay/15">
            Try again
          </button>
        </div>
      )
    }
    return this.props.children
  }
}
