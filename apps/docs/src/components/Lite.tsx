/**
 * `<Lite>` — a Toasty Lite animation in a docs page or React component.
 * Same props as the real `<ToastyLite>` from `an1mbyt3-lite-react.js`,
 * plus `ink="auto"`: the animation's `ink` Property follows the site's
 * text colour, so line icons match light and dark mode.
 *
 * The runtime (`/toasty/an1mbyt3-lite.js`, ~10 KB) is imported at runtime
 * the first time any `<Lite>` mounts and shared by all of them — any number
 * can run on one page. Files are served from `static/toasty/lite/`.
 */
import React, { forwardRef, useEffect, useImperativeHandle, useRef, useState, type CSSProperties } from 'react'
import useBaseUrl from '@docusaurus/useBaseUrl'
import { useColorMode } from '@docusaurus/theme-common'

export interface LiteHandle {
  set(name: string, value: unknown): void
  setData(data: Record<string, unknown>): void
  get(name: string): unknown
  setInput(name: string, value: boolean | number | string): void
  fire(trigger: string): void
  state(layer?: string | null): string | null
  on(type: 'event' | 'state' | 'prop', fn: (e: any) => void): () => void
  play(): void
  pause(): void
  destroy(): void
}

export interface LiteProps {
  /** A Lite file name (`heart`) or URL (`/toasty/lite/heart.json`). */
  src: string
  fit?: 'contain' | 'cover'
  paused?: boolean
  data?: Record<string, unknown>
  inputs?: Record<string, boolean | number | string>
  /** `auto` = the site's text colour; or a CSS hex colour. */
  ink?: 'auto' | string
  className?: string
  style?: CSSProperties
  title?: string
  /** Accessible label (the canvas is decorative without one). */
  label?: string
  onReady?: (player: LiteHandle) => void
  onEvent?: (e: { name: string; payload: unknown }) => void
  onStateChange?: (e: { layer: string | null; state: string; previous: string | null }) => void
}

type LiteModule = { createLite: (box: HTMLElement, opts: Record<string, unknown>) => Promise<LiteHandle> }
let liteModule: Promise<LiteModule> | null = null
function loadRuntime(url: string): Promise<LiteModule> {
  liteModule ??= import(/* webpackIgnore: true */ url) as Promise<LiteModule>
  return liteModule
}

function changed<T>(before: Record<string, T> | undefined, after: Record<string, T> | undefined): [string, T][] {
  if (!after) return []
  return Object.entries(after).filter(([k, v]) => !before || !(k in before) || !Object.is(before[k], v))
}

/** A CSS colour → `[r, g, b, a]` in 0..1. */
export function cssToRgba(color: string): number[] {
  const m = color.match(/rgba?\(([^)]+)\)/)
  if (m) {
    const [r, g, b, a] = m[1].split(/[ ,/]+/).filter(Boolean).map(Number)
    return [r / 255, g / 255, b / 255, Number.isFinite(a) ? a : 1]
  }
  const h = color.replace('#', '')
  if (/^[0-9a-f]{6}$/i.test(h)) return [parseInt(h.slice(0, 2), 16) / 255, parseInt(h.slice(2, 4), 16) / 255, parseInt(h.slice(4, 6), 16) / 255, 1]
  return [0.1, 0.1, 0.12, 1]
}

export const Lite = forwardRef<LiteHandle | null, LiteProps>(function Lite(props, ref) {
  const box = useRef<HTMLDivElement>(null)
  const [player, setPlayer] = useState<LiteHandle | null>(null)
  const latest = useRef(props)
  latest.current = props
  const sentData = useRef<Record<string, unknown> | undefined>(undefined)
  const sentInputs = useRef<Record<string, boolean | number | string> | undefined>(undefined)
  const runtime = useBaseUrl('/toasty/an1mbyt3-lite.js')
  const file = useBaseUrl(props.src.includes('/') ? props.src : `/toasty/lite/${props.src}.json`)
  const { colorMode } = useColorMode()
  useImperativeHandle(ref, () => player as LiteHandle, [player])

  useEffect(() => {
    const el = box.current
    if (!el) return
    let cancelled = false
    let made: LiteHandle | null = null
    const p = latest.current
    sentData.current = p.data ? { ...p.data } : undefined
    sentInputs.current = p.inputs ? { ...p.inputs } : undefined
    loadRuntime(new URL(runtime, location.href).href)
      .then(({ createLite }) => createLite(el, { src: new URL(file, location.href).href, fit: p.fit, paused: p.paused, data: p.data, inputs: p.inputs }))
      .then((instance) => {
        if (cancelled) return instance.destroy()
        made = instance
        instance.on('event', (e) => latest.current.onEvent?.(e))
        instance.on('state', (e) => latest.current.onStateChange?.(e))
        setPlayer(instance)
        latest.current.onReady?.(instance)
      })
      .catch((err) => console.error(err))
    return () => {
      cancelled = true
      made?.destroy()
      setPlayer(null)
    }
  }, [file, runtime, props.fit])

  // Theme-following ink.
  useEffect(() => {
    if (!player || !props.ink) return
    const color = props.ink === 'auto' ? getComputedStyle(box.current ?? document.body).color : props.ink
    player.set('ink', cssToRgba(color))
  }, [player, props.ink, colorMode])

  useEffect(() => {
    if (!player) return
    for (const [k, v] of changed(sentData.current, props.data)) player.set(k, v)
    sentData.current = props.data ? { ...props.data } : undefined
  }, [player, props.data])
  useEffect(() => {
    if (!player) return
    for (const [k, v] of changed(sentInputs.current, props.inputs)) player.setInput(k, v)
    sentInputs.current = props.inputs ? { ...props.inputs } : undefined
  }, [player, props.inputs])
  useEffect(() => {
    if (!player) return
    if (props.paused) player.pause()
    else player.play()
  }, [player, props.paused])

  return <div ref={box} className={props.className} title={props.title} role={props.label ? 'img' : undefined} aria-label={props.label} aria-hidden={props.label ? undefined : true} style={{ position: 'relative', ...props.style }} />
})
