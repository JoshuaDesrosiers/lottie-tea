import { useEffect, useRef, useState } from 'react'

/** Straight (non-premultiplied) linear RGBA, each component `0.0..=1.0` —
 * matches every fill-color value already flowing through this app
 * (`an1mbyt3_core::ShapeNode::fill_color` and friends). */
export type RgbaColor = [number, number, number, number]

export function toHex(c: RgbaColor): string {
  return `#${c
    .slice(0, 3)
    .map((v) => Math.round(Math.min(1, Math.max(0, v)) * 255).toString(16).padStart(2, '0'))
    .join('')}`
}

export function fromHex(hex: string): RgbaColor {
  return [Number.parseInt(hex.slice(1, 3), 16) / 255, Number.parseInt(hex.slice(3, 5), 16) / 255, Number.parseInt(hex.slice(5, 7), 16) / 255, 1]
}

function rgbToHsv(r: number, g: number, b: number): [number, number, number] {
  const max = Math.max(r, g, b)
  const min = Math.min(r, g, b)
  const d = max - min
  let h = 0
  if (d !== 0) {
    if (max === r) h = (((g - b) / d) % 6) * 60
    else if (max === g) h = ((b - r) / d + 2) * 60
    else h = ((r - g) / d + 4) * 60
    if (h < 0) h += 360
  }
  const s = max === 0 ? 0 : d / max
  return [h, s, max]
}

function hsvToRgb(h: number, s: number, v: number): [number, number, number] {
  const c = v * s
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1))
  const m = v - c
  let rgb: [number, number, number]
  if (h < 60) rgb = [c, x, 0]
  else if (h < 120) rgb = [x, c, 0]
  else if (h < 180) rgb = [0, c, x]
  else if (h < 240) rgb = [0, x, c]
  else if (h < 300) rgb = [x, 0, c]
  else rgb = [c, 0, x]
  return [rgb[0] + m, rgb[1] + m, rgb[2] + m]
}

/** Drag-to-pick 2D pad (saturation horizontal, value vertical, tinted by
 * `hue`) — the classic HSV-picker square every design tool uses. Reports
 * `[0,1]`-normalized `s`/`v` from the pointer's position within the pad,
 * clamped, on every pointer-down/move while the button is held; `onCommit`
 * (not `onChange`-per-pixel) only fires once, on release, since every
 * caller here treats a color edit like any other `NumberField`-style
 * commit rather than a thousand intermediate document edits per drag. */
function SvPad({ hue, s, v, onChange }: { hue: number; s: number; v: number; onChange: (s: number, v: number) => void }) {
  const padRef = useRef<HTMLDivElement>(null)

  const updateFromEvent = (e: { clientX: number; clientY: number }) => {
    const rect = padRef.current?.getBoundingClientRect()
    if (!rect) return
    const nextS = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width))
    const nextV = 1 - Math.min(1, Math.max(0, (e.clientY - rect.top) / rect.height))
    onChange(nextS, nextV)
  }

  return (
    <div
      ref={padRef}
      onPointerDown={(e) => {
        ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
        updateFromEvent(e)
      }}
      onPointerMove={(e) => {
        if (e.buttons & 1) updateFromEvent(e)
      }}
      className="relative h-32 w-full cursor-crosshair rounded-md"
      style={{ background: `linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, hsl(${hue}, 100%, 50%))` }}
    >
      <div
        className="pointer-events-none absolute h-3 w-3 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow"
        style={{ left: `${s * 100}%`, top: `${(1 - v) * 100}%` }}
      />
    </div>
  )
}

/** Horizontal full-spectrum hue slider — same drag convention as `SvPad`. */
function HueSlider({ hue, onChange }: { hue: number; onChange: (hue: number) => void }) {
  const trackRef = useRef<HTMLDivElement>(null)

  const updateFromEvent = (e: { clientX: number }) => {
    const rect = trackRef.current?.getBoundingClientRect()
    if (!rect) return
    onChange(Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width)) * 360)
  }

  return (
    <div
      ref={trackRef}
      onPointerDown={(e) => {
        ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
        updateFromEvent(e)
      }}
      onPointerMove={(e) => {
        if (e.buttons & 1) updateFromEvent(e)
      }}
      className="relative h-3 w-full cursor-pointer rounded-full"
      style={{ background: 'linear-gradient(to right, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00)' }}
    >
      <div
        className="pointer-events-none absolute top-1/2 h-3.5 w-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow"
        style={{ left: `${(hue / 360) * 100}%`, backgroundColor: `hsl(${hue}, 100%, 50%)` }}
      />
    </div>
  )
}

/** Checkerboard-backed horizontal opacity slider, `0..1` left to right —
 * same drag convention as `HueSlider`. The checkerboard (rather than a
 * plain color-to-transparent gradient alone) is what actually reads as
 * "this is an alpha control" at a glance, the same visual convention every
 * design tool's opacity slider uses. Without *some* alpha control here, a
 * color with reduced alpha (e.g. a gradient stop deliberately faded toward
 * transparent) had no way to be adjusted back up once set — the picker's
 * hue/SV pad only ever touched RGB, silently leaving alpha wherever it
 * last was. */
function AlphaSlider({ color, alpha, onChange }: { color: [number, number, number]; alpha: number; onChange: (alpha: number) => void }) {
  const trackRef = useRef<HTMLDivElement>(null)
  const [r, g, b] = color

  const updateFromEvent = (e: { clientX: number }) => {
    const rect = trackRef.current?.getBoundingClientRect()
    if (!rect) return
    onChange(Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width)))
  }

  const rgb255 = `${Math.round(r * 255)}, ${Math.round(g * 255)}, ${Math.round(b * 255)}`

  return (
    <div
      ref={trackRef}
      onPointerDown={(e) => {
        ;(e.target as HTMLElement).setPointerCapture(e.pointerId)
        updateFromEvent(e)
      }}
      onPointerMove={(e) => {
        if (e.buttons & 1) updateFromEvent(e)
      }}
      className="relative h-3 w-full cursor-pointer rounded-full"
      style={{
        backgroundImage:
          `linear-gradient(to right, rgba(${rgb255}, 0), rgba(${rgb255}, 1)), ` +
          'repeating-conic-gradient(#8888 0% 25%, #ccc8 0% 50%)',
        backgroundSize: '100% 100%, 8px 8px',
      }}
    >
      <div
        className="pointer-events-none absolute top-1/2 h-3.5 w-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow"
        style={{ left: `${alpha * 100}%`, backgroundColor: `rgba(${rgb255}, ${alpha})` }}
      />
    </div>
  )
}

/** The picker's actual contents (pad + hue slider + alpha slider + hex
 * field) — split out from `ColorSwatch` so a future gradient-stop editor
 * can embed this directly inline (next to a stop list) instead of only
 * ever behind a popover button. */
export function ColorPickerBody({ value, onChange }: { value: RgbaColor; onChange: (value: RgbaColor) => void }) {
  const [h, s, v] = rgbToHsv(value[0], value[1], value[2])
  const [hexText, setHexText] = useState(toHex(value))
  useEffect(() => setHexText(toHex(value)), [value])

  const commitHsv = (nextH: number, nextS: number, nextV: number) => {
    const [r, g, b] = hsvToRgb(nextH, nextS, nextV)
    onChange([r, g, b, value[3]])
  }

  return (
    <div className="flex flex-col gap-2">
      <SvPad hue={h} s={s} v={v} onChange={(nextS, nextV) => commitHsv(h, nextS, nextV)} />
      <HueSlider hue={h} onChange={(nextH) => commitHsv(nextH, s, v)} />
      <AlphaSlider color={[value[0], value[1], value[2]]} alpha={value[3]} onChange={(alpha) => onChange([value[0], value[1], value[2], alpha])} />
      <input
        value={hexText}
        onChange={(e) => setHexText(e.target.value)}
        onBlur={() => {
          if (/^#[0-9a-fA-F]{6}$/.test(hexText)) onChange(fromHex(hexText))
          else setHexText(toHex(value))
        }}
        onKeyDown={(e) => {
          if (e.key === 'Enter') e.currentTarget.blur()
        }}
        className="w-full rounded-md bg-overlay/8 px-2 py-1 text-xs text-ink outline-none"
      />
    </div>
  )
}

/** A clickable color swatch that opens `ColorPickerBody` in a popover —
 * the drop-in, non-native replacement for `<input type="color">`
 * (`InspectorPanel`'s fill-color/instance-override fields). Closes on an
 * outside click or Escape, same convention as this app's other inline
 * "double-click to edit, click away to commit" affordances (`LayerPill`'s
 * rename field). */
export function ColorSwatch({ value, onChange, disabled }: { value: RgbaColor; onChange: (value: RgbaColor) => void; disabled?: boolean }) {
  const [open, setOpen] = useState(false)
  const containerRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const handlePointerDown = (e: PointerEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) setOpen(false)
    }
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false)
    }
    window.addEventListener('pointerdown', handlePointerDown)
    window.addEventListener('keydown', handleKeyDown)
    return () => {
      window.removeEventListener('pointerdown', handlePointerDown)
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [open])

  return (
    // `min-w-8 flex-1`: the swatch button is `w-full` of this wrapper, which
    // has no intrinsic width of its own — inside a `label + swatch` flex row
    // (Stroke/Particle/Camera/script color fields) it used to collapse to a
    // 1px sliver. `flex-1` fills the row; harmless outside a flex parent.
    <div ref={containerRef} className="relative min-w-8 flex-1">
      <button
        type="button"
        disabled={disabled}
        onClick={() => setOpen((o) => !o)}
        title={`${toHex(value)} @ ${Math.round(value[3] * 100)}%`}
        className="h-6 w-full cursor-pointer rounded-md border border-overlay/15 disabled:cursor-default disabled:opacity-40"
        style={{
          // A checkerboard base layer *underneath* the actual (alpha-aware)
          // color — without it, a partially- or fully-transparent color
          // rendered as a plain `rgba(...)` background would just show
          // whatever's behind the swatch in the page itself, not read as
          // "this color has reduced opacity" the way it does against a
          // checkerboard. Plain `toHex` (RGB-only, silently full-opacity)
          // was the actual bug here: a stop faded to alpha 0 looked
          // identically fully-opaque in its own swatch, with no visual way
          // to tell it apart from a solid color before opening the picker.
          backgroundImage:
            `linear-gradient(rgba(${Math.round(value[0] * 255)}, ${Math.round(value[1] * 255)}, ${Math.round(value[2] * 255)}, ${value[3]}), ` +
            `rgba(${Math.round(value[0] * 255)}, ${Math.round(value[1] * 255)}, ${Math.round(value[2] * 255)}, ${value[3]})), ` +
            'repeating-conic-gradient(#8888 0% 25%, #ccc8 0% 50%)',
          backgroundSize: '100% 100%, 10px 10px',
        }}
      />
      {open && !disabled && (
        <div className="absolute left-0 top-full z-20 mt-1 w-48 rounded-md border border-panel-border bg-canvas p-2 shadow-xl">
          <ColorPickerBody value={value} onChange={onChange} />
        </div>
      )}
    </div>
  )
}
