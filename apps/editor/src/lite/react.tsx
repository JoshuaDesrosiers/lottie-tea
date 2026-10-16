/**
 * **Toasty Lite for React** — `<ToastyLite>`: a Toasty Lite animation as a
 * component. Like `<An1Player>`, props are diffed and sent live: `data`
 * (scene Properties) and `inputs` (state machine inputs) update the running
 * animation without a reload. Any number can be on a page.
 *
 * ```tsx
 * import { ToastyLite } from './toasty/an1mbyt3-lite-react.js'
 * <ToastyLite src="/toasty/heart.json" inputs={{ liked }} onEvent={(e) => e.name === 'like' && toggle()} style={{ width: 40, height: 40 }} />
 * ```
 */
import { useEffect, useImperativeHandle, useRef, useState, type CSSProperties, type Ref } from 'react'
import { createLite, type LiteEvents, type LiteFile, type LitePlayer } from './runtime'

export type { LiteEvents, LiteFile, LitePlayer }

export interface ToastyLiteProps {
  src: string | LiteFile
  fit?: 'contain' | 'cover'
  background?: boolean
  paused?: boolean
  data?: Record<string, unknown>
  inputs?: Record<string, boolean | number | string>
  className?: string
  style?: CSSProperties
  title?: string
  ref?: Ref<LitePlayer | null>
  onReady?: (player: LitePlayer) => void
  onEvent?: (event: LiteEvents['event']) => void
  onStateChange?: (event: LiteEvents['state']) => void
  onPropChange?: (event: LiteEvents['prop']) => void
}

function changed<T>(before: Record<string, T> | undefined, after: Record<string, T> | undefined): [string, T][] {
  if (!after) return []
  return Object.entries(after).filter(([k, v]) => !before || !(k in before) || !Object.is(before[k], v))
}

export function ToastyLite(props: ToastyLiteProps) {
  const box = useRef<HTMLDivElement>(null)
  const [player, setPlayer] = useState<LitePlayer | null>(null)
  const latest = useRef(props)
  latest.current = props
  const sentData = useRef<Record<string, unknown> | undefined>(undefined)
  const sentInputs = useRef<Record<string, boolean | number | string> | undefined>(undefined)
  useImperativeHandle(props.ref, () => player as LitePlayer, [player])

  useEffect(() => {
    const el = box.current
    if (!el) return
    let cancelled = false
    let made: LitePlayer | null = null
    const p = latest.current
    sentData.current = p.data ? { ...p.data } : undefined
    sentInputs.current = p.inputs ? { ...p.inputs } : undefined
    createLite(el, { src: p.src, fit: p.fit, background: p.background, paused: p.paused, data: p.data, inputs: p.inputs }).then((instance) => {
      if (cancelled) return instance.destroy()
      made = instance
      instance.on('event', (e) => latest.current.onEvent?.(e))
      instance.on('state', (e) => latest.current.onStateChange?.(e))
      instance.on('prop', (e) => latest.current.onPropChange?.(e))
      setPlayer(instance)
      latest.current.onReady?.(instance)
    }, console.error)
    return () => {
      cancelled = true
      made?.destroy()
      setPlayer(null)
    }
  }, [props.src, props.fit, props.background])

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

  return <div ref={box} className={props.className} title={props.title} style={{ position: 'relative', ...props.style }} />
}
