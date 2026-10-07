/**
 * `<Animation src="/character.teab" stateMachine="Character" />` —
 * AN1MBYT3_BUILD_PLAN.md §8's React entry point. Owns a `<canvas>` and the
 * `@an1mbyt3/runtime` `An1mbyt3Animation` instance rendering into it; all
 * actual engine/rendering logic lives in `@an1mbyt3/runtime` (this
 * component is a thin React lifecycle adapter around it, same "no animation
 * semantics duplicated outside Rust" principle §1 states for the editor).
 */
import { useEffect, useRef } from 'react'
import type { CSSProperties } from 'react'
import { An1mbyt3Animation } from '@an1mbyt3/runtime'

export interface AnimationProps {
  /** A `.teab` URL, fetched and loaded once this component mounts (and
   * again whenever `src` itself changes). */
  src: string
  /**
   * Which state machine to drive — matches this prop's shape in
   * AN1MBYT3_BUILD_PLAN.md §8. Currently informational only: `WasmPlayer`
   * always drives whichever state machine (if any) the loaded artboard's
   * first one is (see `@an1mbyt3/runtime`'s `stateMachine` method doc
   * comment for why name-addressed *selection* isn't wired up on the engine
   * side yet). Passing this still gets you the same "not found" console
   * warning `An1mbyt3Animation.stateMachine` gives, so a typo surfaces
   * immediately rather than silently doing nothing.
   */
  stateMachine?: string
  /** See `@an1mbyt3/runtime`'s `AnimationOptions.trackPointer` doc comment. */
  trackPointer?: boolean
  className?: string
  style?: CSSProperties
  /** Called once the animation is created *and* `src` has finished loading —
   * the escape hatch for anything this component doesn't wrap directly
   * (`play`/`pause`/`seek`/`fireTrigger`/etc., via `@an1mbyt3/runtime`'s
   * own API). */
  onReady?: (animation: An1mbyt3Animation) => void
}

export function Animation({ src, stateMachine, trackPointer, className, style, onReady }: AnimationProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const animationRef = useRef<An1mbyt3Animation | null>(null)

  // Re-creates the whole `An1mbyt3Animation` whenever `src` changes, rather
  // than reusing one instance across a `src` swap — simplest correct
  // behavior for now (an existing instance's `loadTeab` would work too, but
  // tying instance lifetime to `src` avoids needing to reason about
  // `trackPointer`/canvas-size changes surviving a reload). `onReady`/
  // `stateMachine` deliberately excluded from the dependency array: they're
  // "this render's callback/label", not something that should tear down and
  // recreate the whole WASM player if a parent re-renders with a new
  // closure identity — only `src`/`trackPointer` describe *what's playing*.
  useEffect(() => {
    if (!canvasRef.current) return
    let cancelled = false
    let animation: An1mbyt3Animation | null = null

    ;(async () => {
      const canvas = canvasRef.current!
      const created = await An1mbyt3Animation.create(canvas, { trackPointer })
      if (cancelled) {
        created.destroy()
        return
      }
      animation = created
      animationRef.current = created

      await created.loadTeab(src)
      if (cancelled) return

      if (stateMachine) created.stateMachine(stateMachine)
      onReady?.(created)
    })().catch((err) => console.error('@an1mbyt3/react: failed to create/load animation', err))

    return () => {
      cancelled = true
      animation?.destroy()
      animationRef.current = null
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [src, trackPointer])

  // Keeps the canvas's backing pixel size in sync with whatever CSS size
  // it's actually laid out at — same "CSS size and backing-buffer size are
  // two different things" reasoning `apps/editor`'s `CanvasArea` doc
  // comment already established for the editor's own canvas.
  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const width = Math.max(1, Math.round(entry.contentRect.width))
        const height = Math.max(1, Math.round(entry.contentRect.height))
        canvas.width = width
        canvas.height = height
        animationRef.current?.resize(width, height)
      }
    })
    observer.observe(canvas)
    return () => observer.disconnect()
  }, [])

  return <canvas ref={canvasRef} className={className} style={{ display: 'block', width: '100%', height: '100%', ...style }} />
}
