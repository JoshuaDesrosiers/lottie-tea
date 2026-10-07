import { useEffect, useMemo, useState } from 'react'
import type { TilemapConfig } from '../ipc/document'
import { getImageSource, type WasmEditor } from '../ipc/wasmEditor'

/** The Tile Palette's actual content — a real dockable panel now (draggable/
 * closable/reopenable like Layers/Resources, `useDockLayout`'s `tilePalette`
 * key), not the floating overlay this started as. Shows the selected
 * `Tilemap` node's own tileset image, cropped into one clickable swatch per
 * tile via CSS `background-position` — no server-side tile slicing needed,
 * the renderer already samples arbitrary sub-rects of one shared tileset
 * texture directly (`an1mbyt3_core::tilemap`'s own module doc). Painting
 * itself happens on the canvas (`TilemapPaintOverlay`'s click-catcher), not
 * here — this panel only owns *which* tile is currently selected to paint
 * with. */
export function TilePalettePanel({
  editor,
  tilemap,
  selectedTile,
  onSelectTile,
}: {
  editor: WasmEditor | null
  tilemap: TilemapConfig | null
  selectedTile: number
  onSelectTile: (tileIndex: number) => void
}) {
  const [tilesetUrl, setTilesetUrl] = useState<string | null>(null)
  useEffect(() => {
    if (!editor || !tilemap) {
      setTilesetUrl(null)
      return
    }
    const bytes = getImageSource(editor, tilemap.tileset)
    const url = URL.createObjectURL(new Blob([bytes as unknown as BlobPart], { type: 'image/png' }))
    setTilesetUrl(url)
    return () => URL.revokeObjectURL(url)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor, tilemap?.tileset])

  // Pixel-art tiles are typically tiny (16x16) — upscaled uniformly here so
  // swatches stay clickable; has no bearing on the tilemap's own on-canvas
  // `cellSize`.
  const paletteScale = tilemap ? Math.max(1, Math.round(32 / Math.max(tilemap.tileWidthPx, tilemap.tileHeightPx))) : 1

  const paletteTileIds = useMemo(() => {
    if (!tilemap) return []
    const columns = Math.max(1, Math.floor(tilemap.tilesetWidthPx / tilemap.tileWidthPx))
    const rows = Math.max(1, Math.floor(tilemap.tilesetHeightPx / tilemap.tileHeightPx))
    return Array.from({ length: columns * rows }, (_, i) => i)
  }, [tilemap])

  if (!tilemap || !tilesetUrl) {
    return <div className="p-2 text-xs text-ink-dim">Select a Tilemap node to paint it.</div>
  }

  const columns = Math.max(1, Math.floor(tilemap.tilesetWidthPx / tilemap.tileWidthPx))
  const tileDisplaySize = tilemap.tileWidthPx * paletteScale

  return (
    <div className="grid gap-px" style={{ gridTemplateColumns: `repeat(${columns}, ${tileDisplaySize}px)` }}>
      {paletteTileIds.map((tileIndex) => {
        const col = tileIndex % columns
        const row = Math.floor(tileIndex / columns)
        return (
          <button
            key={tileIndex}
            onClick={() => onSelectTile(tileIndex)}
            title={`Tile ${tileIndex}`}
            className={`shrink-0 border ${selectedTile === tileIndex ? 'border-accent' : 'border-transparent'}`}
            style={{
              width: tileDisplaySize,
              height: tileDisplaySize,
              backgroundImage: `url(${tilesetUrl})`,
              backgroundPosition: `-${col * tileDisplaySize}px -${row * tileDisplaySize}px`,
              backgroundSize: `${tilemap.tilesetWidthPx * paletteScale}px ${tilemap.tilesetHeightPx * paletteScale}px`,
              imageRendering: 'pixelated',
            }}
          />
        )
      })}
    </div>
  )
}
