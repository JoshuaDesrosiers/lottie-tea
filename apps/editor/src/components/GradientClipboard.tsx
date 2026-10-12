/**
 * Copy / paste for gradients — between shape and text fills, Goo 2D
 * bodies and rims, and group fills.
 *
 * A gradient's points live in its node's own local space, so pasting the
 * raw numbers onto a differently sized or placed object would put the
 * gradient somewhere off to the side. Instead the copy remembers the box
 * it was copied from and the paste re-fits the points to the target's box
 * (same *relative* position: a left-to-right sweep across one shape is a
 * left-to-right sweep across the next). Kind and stops copy as they are.
 *
 * The gradient also goes on the system clipboard as JSON text, so it
 * survives between editor windows; the in-memory copy is the fallback when
 * the clipboard can't be read.
 */
import { useEffect, useState } from 'react'
import type { GradientDef } from '../ipc/document'
import { CopyIcon, PasteIcon } from './icons'

/** A node-local box (`localMin` / `localMax`). */
export interface GradientBox {
  min: { x: number; y: number }
  max: { x: number; y: number }
}

interface Copied {
  gradient: GradientDef
  box: GradientBox | null
}

const PREFIX = 'an1mbyt3-gradient:'
let copied: Copied | null = null
const listeners = new Set<() => void>()

/** Copies `gradient` (taken from a node whose box is `box`). */
export function copyGradient(gradient: GradientDef, box: GradientBox | null) {
  copied = { gradient: structuredClone(gradient), box }
  void navigator.clipboard?.writeText(PREFIX + JSON.stringify(copied)).catch(() => {})
  listeners.forEach((l) => l())
}

/** `p` in `from`'s box → the same relative spot in `to`'s box. */
function refit(p: { x: number; y: number }, from: GradientBox, to: GradientBox) {
  const u = (v: number, a: number, b: number) => (Math.abs(b - a) < 1e-6 ? 0.5 : (v - a) / (b - a))
  return {
    x: to.min.x + u(p.x, from.min.x, from.max.x) * (to.max.x - to.min.x),
    y: to.min.y + u(p.y, from.min.y, from.max.y) * (to.max.y - to.min.y),
  }
}

/** The copied gradient, fitted to a node whose box is `box` (`null`:
 * points unchanged). Reads the system clipboard first. */
export async function pasteGradient(box: GradientBox | null): Promise<GradientDef | null> {
  let source = copied
  try {
    const text = await navigator.clipboard?.readText()
    if (text?.startsWith(PREFIX)) source = JSON.parse(text.slice(PREFIX.length)) as Copied
  } catch {
    // Clipboard unreadable: use the in-memory copy.
  }
  if (!source) return null
  const gradient = structuredClone(source.gradient)
  if (source.box && box) {
    gradient.start = refit(gradient.start, source.box, box)
    gradient.end = refit(gradient.end, source.box, box)
  }
  return gradient
}

/** Copy / Paste buttons for a gradient field. Copy shows only when there's
 * a gradient; Paste only once something was copied this session (or the
 * clipboard may hold one — it's always offered after a copy). */
export function GradientClipboardButtons({ gradient, box, onPaste }: { gradient: GradientDef | null; box: GradientBox | null; onPaste: (g: GradientDef) => void }) {
  const [, bump] = useState(0)
  useEffect(() => {
    const l = () => bump((n) => n + 1)
    listeners.add(l)
    return () => {
      listeners.delete(l)
    }
  }, [])
  const button = 'flex h-5 items-center gap-1 rounded px-1.5 text-[10px] text-ink-dim hover:bg-overlay/10 hover:text-ink'
  return (
    <div className="flex items-center gap-0.5">
      {gradient && (
        <button type="button" className={button} onClick={() => copyGradient(gradient, box)} title="Copy this gradient (paste it onto another shape, text, group or goo)">
          <CopyIcon />
        </button>
      )}
      {copied && (
        <button
          type="button"
          className={button}
          onClick={() => {
            void pasteGradient(box).then((g) => g && onPaste(g))
          }}
          title="Paste the copied gradient, fitted to this object"
        >
          <PasteIcon />
        </button>
      )}
    </div>
  )
}
