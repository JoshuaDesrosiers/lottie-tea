/**
 * `<LiteLive>` — one live Toasty Lite player on an exported file, used by
 * the Test Area (and the title bar's test slot). Sets the file's `ink`
 * Property (if it has one) and drives its `hover` input when asked.
 */
import { useEffect, useRef, useState, type CSSProperties } from 'react'
import { createLite, type LiteFile, type LitePlayer } from '../lite/runtime'

export function LiteLive({ file, width, height, ink, hover, players, fit, style }: { file: LiteFile; width: number | string; height: number | string; ink?: number[]; hover?: boolean; players?: Set<LitePlayer>; fit?: 'contain' | 'cover'; style?: CSSProperties }) {
  const box = useRef<HTMLDivElement>(null)
  const [player, setPlayer] = useState<LitePlayer | null>(null)
  useEffect(() => {
    const el = box.current
    if (!el) return
    let made: LitePlayer | null = null
    let cancelled = false
    void createLite(el, { src: file, fit }).then((p) => {
      if (cancelled) return p.destroy()
      made = p
      players?.add(p)
      setPlayer(p)
    })
    return () => {
      cancelled = true
      if (made) {
        players?.delete(made)
        made.destroy()
      }
      setPlayer(null)
    }
  }, [file, players, fit])
  useEffect(() => {
    if (player && ink && file.properties.some((p) => p.name === 'ink')) player.set('ink', ink)
  }, [player, ink, file])
  useEffect(() => {
    if (player && hover !== undefined && file.stateMachine?.inputs.some((i) => i.name === 'hover')) player.setInput('hover', hover)
  }, [player, hover, file])
  return <div ref={box} style={{ width, height, flex: 'none', ...style }} />
}

/** A CSS colour (as computed) → `[r, g, b, a]` in 0..1. */
export function rgbaOf(color: string): number[] {
  const m = color.match(/rgba?\(([^)]+)\)/)
  if (!m) return [0.1, 0.1, 0.12, 1]
  const [r, g, b, a] = m[1].split(/[ ,/]+/).filter(Boolean).map(Number)
  return [r / 255, g / 255, b / 255, Number.isFinite(a) ? a : 1]
}
