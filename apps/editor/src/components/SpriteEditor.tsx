/**
 * **Sprite Editor** — Unity-style slicing of a sprite sheet into frames.
 *
 * Opens over the editor for one image asset (Inspector → "Sprite Editor…"
 * on a selected image). Three ways to slice, like Unity's:
 *
 * - **Grid by cell size** — fixed `width x height` cells, with an offset
 *   (where the grid starts) and padding (the gap between cells).
 * - **Grid by cell count** — `columns x rows` cells fitted to the sheet.
 * - **Automatic** — every separate sprite, found by its transparent edges
 *   (`auto_slice_image` → `an1mbyt3_image::alpha_islands`).
 *
 * The slices are drawn over a zoomable, pixel-exact preview; click one to
 * leave it out. "Slice" crops them into image assets named
 * `<prefix> <n>` and groups them as one Sprite Group (so they show as a
 * single multi-frame resource and play as a flipbook at the frame time).
 */
import { useEffect, useMemo, useRef, useState } from 'react'
import type { DocumentSummary, ImageAssetSummary } from '../ipc/document'
import { getImageSource } from '../ipc/wasmEditor'
import type { WasmEditor } from '../wasm-pkg/an1mbyt3_wasm'

type Rect = [number, number, number, number]
type Mode = 'size' | 'count' | 'auto'

function NumberInput({ label, value, onChange, min = 0, max = 4096, title }: { label: string; value: number; onChange: (v: number) => void; min?: number; max?: number; title?: string }) {
  return (
    <label className="flex items-center justify-between gap-2 text-[11px] text-ink-dim" title={title}>
      <span>{label}</span>
      <input
        type="number"
        value={value}
        min={min}
        max={max}
        onChange={(e) => {
          const v = Number(e.target.value)
          if (Number.isFinite(v)) onChange(Math.min(max, Math.max(min, Math.round(v))))
        }}
        className="w-20 rounded-md bg-overlay/8 px-1.5 py-0.5 text-right text-[11px] tabular-nums text-ink outline-none focus:bg-overlay/15"
      />
    </label>
  )
}

/** Grid cells: `count` mode derives the cell size from the sheet. */
function gridRects(w: number, h: number, cellW: number, cellH: number, offX: number, offY: number, padX: number, padY: number): Rect[] {
  const out: Rect[] = []
  if (cellW < 1 || cellH < 1) return out
  for (let y = offY; y + cellH <= h; y += cellH + padY) {
    for (let x = offX; x + cellW <= w; x += cellW + padX) out.push([x, y, cellW, cellH])
  }
  return out
}

export function SpriteEditor({
  editor,
  asset,
  onClose,
  onDone,
}: {
  editor: WasmEditor
  asset: ImageAssetSummary
  onClose: () => void
  onDone: (doc: DocumentSummary) => void
}) {
  const sheetW = asset.width ?? 0
  const sheetH = asset.height ?? 0
  const stem = asset.name.replace(/\.[a-z0-9]+$/i, '')
  const [mode, setMode] = useState<Mode>('size')
  const [cellW, setCellW] = useState(Math.min(16, sheetW) || 16)
  const [cellH, setCellH] = useState(Math.min(16, sheetH) || 16)
  const [cols, setCols] = useState(4)
  const [rows, setRows] = useState(4)
  const [offX, setOffX] = useState(0)
  const [offY, setOffY] = useState(0)
  const [padX, setPadX] = useState(0)
  const [padY, setPadY] = useState(0)
  const [skipEmpty, setSkipEmpty] = useState(true)
  const [alphaMin, setAlphaMin] = useState(0)
  const [minSize, setMinSize] = useState(2)
  const [mergeGap, setMergeGap] = useState(0)
  const [autoRects, setAutoRects] = useState<Rect[]>([])
  const [prefix, setPrefix] = useState(stem)
  const [frameMs, setFrameMs] = useState(100)
  const [excluded, setExcluded] = useState<Set<number>>(new Set())
  const [zoom, setZoom] = useState(() => Math.max(1, Math.min(8, Math.floor(560 / Math.max(1, sheetW, sheetH)))))
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [hover, setHover] = useState<number | null>(null)

  // The sheet: an object URL for the preview, and its pixels (alpha) for
  // "skip empty cells".
  const [url, setUrl] = useState<string | null>(null)
  const alphaRef = useRef<Uint8ClampedArray | null>(null)
  useEffect(() => {
    const bytes = getImageSource(editor, asset.id)
    const blob = new Blob([bytes as unknown as BlobPart], { type: 'image/png' })
    const objectUrl = URL.createObjectURL(blob)
    setUrl(objectUrl)
    void createImageBitmap(blob).then((bitmap) => {
      const canvas = document.createElement('canvas')
      canvas.width = bitmap.width
      canvas.height = bitmap.height
      const g = canvas.getContext('2d')
      if (!g) return
      g.drawImage(bitmap, 0, 0)
      alphaRef.current = g.getImageData(0, 0, bitmap.width, bitmap.height).data
      bitmap.close()
      setExcluded(new Set())
    })
    return () => URL.revokeObjectURL(objectUrl)
  }, [editor, asset.id])

  const isEmpty = (r: Rect) => {
    const data = alphaRef.current
    if (!data) return false
    for (let y = r[1]; y < r[1] + r[3]; y++) {
      for (let x = r[0]; x < r[0] + r[2]; x++) if (data[(y * sheetW + x) * 4 + 3] > 0) return false
    }
    return true
  }

  const rects: Rect[] = useMemo(() => {
    if (mode === 'auto') return autoRects
    const [cw, ch] =
      mode === 'size'
        ? [cellW, cellH]
        : [Math.floor((sheetW - offX - padX * (cols - 1)) / Math.max(1, cols)), Math.floor((sheetH - offY - padY * (rows - 1)) / Math.max(1, rows))]
    const all = gridRects(sheetW, sheetH, cw, ch, offX, offY, padX, padY)
    return skipEmpty ? all.filter((r) => !isEmpty(r)) : all
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, autoRects, cellW, cellH, cols, rows, offX, offY, padX, padY, skipEmpty, sheetW, sheetH, url])

  // A changed slicing invalidates which ones were left out.
  useEffect(() => setExcluded(new Set()), [rects])

  const detect = () => {
    try {
      const flat = editor.auto_slice_image(asset.id, alphaMin, minSize, mergeGap)
      const out: Rect[] = []
      for (let i = 0; i + 3 < flat.length; i += 4) out.push([flat[i], flat[i + 1], flat[i + 2], flat[i + 3]])
      setAutoRects(out)
      setError(out.length ? null : 'No sprites found — is the background transparent?')
    } catch (err) {
      setError(String(err))
    }
  }
  // Switching to Automatic detects right away.
  useEffect(() => {
    if (mode === 'auto' && autoRects.length === 0) detect()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode])

  const kept = rects.map((r, i) => ({ r, i })).filter(({ i }) => !excluded.has(i))

  const slice = async () => {
    if (!kept.length) return
    setBusy(true)
    setError(null)
    try {
      const flat = new Uint32Array(kept.flatMap(({ r }) => r))
      const names = kept.map((_, n) => `${prefix || stem} ${n}`)
      const doc = (await editor.slice_image(asset.id, flat, names, prefix || stem, Math.max(1, frameMs))) as DocumentSummary
      onDone(doc)
      onClose()
    } catch (err) {
      setError(String(err))
      setBusy(false)
    }
  }

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  const tab = (id: Mode, label: string) => (
    <button
      type="button"
      onClick={() => setMode(id)}
      className={`flex-1 rounded-md px-2 py-1 text-[11px] ${mode === id ? 'bg-accent text-white' : 'bg-overlay/8 text-ink-dim hover:text-ink'}`}
    >
      {label}
    </button>
  )

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/55 p-6" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="flex h-full max-h-[760px] w-full max-w-[1100px] overflow-hidden rounded-xl border border-panel-border bg-panel shadow-2xl">
        {/* Preview */}
        <div
          className="relative min-w-0 flex-1 overflow-auto"
          style={{
            backgroundColor: '#2a2a33',
            backgroundImage: 'linear-gradient(45deg,#33333d 25%,transparent 25%),linear-gradient(-45deg,#33333d 25%,transparent 25%),linear-gradient(45deg,transparent 75%,#33333d 75%),linear-gradient(-45deg,transparent 75%,#33333d 75%)',
            backgroundSize: '16px 16px',
            backgroundPosition: '0 0,0 8px,8px -8px,-8px 0',
          }}
          onWheel={(e) => {
            if (!e.ctrlKey && !e.metaKey) return
            e.preventDefault()
            setZoom((z) => Math.max(1, Math.min(32, z + (e.deltaY < 0 ? 1 : -1))))
          }}
        >
          <div className="relative m-6 inline-block" style={{ width: sheetW * zoom, height: sheetH * zoom }}>
            {url && <img src={url} alt={asset.name} draggable={false} style={{ width: sheetW * zoom, height: sheetH * zoom, imageRendering: 'pixelated', display: 'block' }} />}
            <svg className="absolute inset-0" width={sheetW * zoom} height={sheetH * zoom}>
              {rects.map((r, i) => {
                const off = excluded.has(i)
                return (
                  <g key={i}>
                    <rect
                      x={r[0] * zoom + 0.5}
                      y={r[1] * zoom + 0.5}
                      width={r[2] * zoom - 1}
                      height={r[3] * zoom - 1}
                      fill={off ? 'rgba(0,0,0,0.45)' : hover === i ? 'rgba(122,162,255,0.18)' : 'transparent'}
                      stroke={off ? 'rgba(255,255,255,0.25)' : 'rgb(122,162,255)'}
                      strokeDasharray={off ? '3 3' : undefined}
                      style={{ cursor: 'pointer' }}
                      onMouseEnter={() => setHover(i)}
                      onMouseLeave={() => setHover((h) => (h === i ? null : h))}
                      onClick={() =>
                        setExcluded((prev) => {
                          const next = new Set(prev)
                          if (next.has(i)) next.delete(i)
                          else next.add(i)
                          return next
                        })
                      }
                    />
                    {zoom * Math.min(r[2], r[3]) >= 22 && (
                      <text x={r[0] * zoom + 3} y={r[1] * zoom + 11} fontSize="9" fill={off ? 'rgba(255,255,255,0.35)' : 'rgb(190,210,255)'} style={{ pointerEvents: 'none' }}>
                        {i}
                      </text>
                    )}
                  </g>
                )
              })}
            </svg>
          </div>
        </div>

        {/* Settings */}
        <div className="flex w-72 shrink-0 flex-col gap-3 overflow-y-auto border-l border-panel-border p-4">
          <div className="flex items-center justify-between">
            <span className="text-sm font-medium text-ink">Sprite Editor</span>
            <button type="button" onClick={onClose} className="rounded px-2 py-0.5 text-xs text-ink-dim hover:bg-overlay/10 hover:text-ink">
              Close
            </button>
          </div>
          <span className="truncate text-[11px] text-ink-dim" title={asset.name}>
            {asset.name} — {sheetW}×{sheetH}
          </span>

          <div className="flex gap-1">
            {tab('size', 'Cell size')}
            {tab('count', 'Cell count')}
            {tab('auto', 'Automatic')}
          </div>

          {mode === 'size' && (
            <div className="flex flex-col gap-1.5">
              <NumberInput label="Cell width" value={cellW} onChange={setCellW} min={1} />
              <NumberInput label="Cell height" value={cellH} onChange={setCellH} min={1} />
            </div>
          )}
          {mode === 'count' && (
            <div className="flex flex-col gap-1.5">
              <NumberInput label="Columns" value={cols} onChange={setCols} min={1} max={512} />
              <NumberInput label="Rows" value={rows} onChange={setRows} min={1} max={512} />
            </div>
          )}
          {mode !== 'auto' && (
            <div className="flex flex-col gap-1.5">
              <NumberInput label="Offset X" value={offX} onChange={setOffX} title="Where the grid starts, in pixels" />
              <NumberInput label="Offset Y" value={offY} onChange={setOffY} />
              <NumberInput label="Padding X" value={padX} onChange={setPadX} title="Gap between cells, in pixels" />
              <NumberInput label="Padding Y" value={padY} onChange={setPadY} />
              <label className="flex items-center gap-2 text-[11px] text-ink-dim">
                <input type="checkbox" checked={skipEmpty} onChange={(e) => setSkipEmpty(e.target.checked)} />
                Skip empty cells
              </label>
            </div>
          )}
          {mode === 'auto' && (
            <div className="flex flex-col gap-1.5">
              <NumberInput label="Alpha threshold" value={alphaMin} onChange={setAlphaMin} max={254} title="Pixels at or below this alpha count as background" />
              <NumberInput label="Min size (px)" value={minSize} onChange={setMinSize} min={1} title="Ignore specks smaller than this" />
              <NumberInput label="Merge gap (px)" value={mergeGap} onChange={setMergeGap} max={64} title="Join parts this close together into one sprite" />
              <button type="button" onClick={detect} className="rounded-md bg-overlay/10 px-2 py-1 text-[11px] text-ink hover:bg-accent/20 hover:text-accent">
                Detect sprites
              </button>
            </div>
          )}

          <div className="flex flex-col gap-1.5 border-t border-panel-border/60 pt-3">
            <label className="flex items-center justify-between gap-2 text-[11px] text-ink-dim">
              <span>Name</span>
              <input value={prefix} onChange={(e) => setPrefix(e.target.value)} className="w-40 rounded-md bg-overlay/8 px-1.5 py-0.5 text-[11px] text-ink outline-none focus:bg-overlay/15" />
            </label>
            <NumberInput label="Frame time (ms)" value={frameMs} onChange={setFrameMs} min={1} max={10000} title="How long each frame shows when played as a flipbook" />
            <label className="flex items-center justify-between gap-2 text-[11px] text-ink-dim">
              <span>Zoom</span>
              <input type="range" min={1} max={32} value={zoom} onChange={(e) => setZoom(Number(e.target.value))} className="w-36" />
            </label>
          </div>

          <p className="text-[11px] leading-snug text-ink-dim">
            {kept.length} of {rects.length} slices{excluded.size ? ` (${excluded.size} left out)` : ''}. Click a slice to leave it out. Frames are named
            <span className="text-ink"> {prefix || stem} 0, 1, 2…</span> and grouped as one sprite group.
          </p>
          {error && <p className="text-[11px] text-red-300">{error}</p>}
          <button
            type="button"
            disabled={busy || kept.length === 0}
            onClick={() => void slice()}
            className="mt-auto rounded-md bg-accent px-3 py-2 text-xs font-medium text-white hover:bg-accent/90 disabled:opacity-40"
          >
            {busy ? 'Slicing…' : `Slice ${kept.length} sprite${kept.length === 1 ? '' : 's'}`}
          </button>
        </div>
      </div>
    </div>
  )
}
