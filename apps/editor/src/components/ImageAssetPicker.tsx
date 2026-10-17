import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import type { ImageAssetSummary, OpaqueId } from '../ipc/document'
import type { WasmEditor } from '../ipc/wasmEditor'
import { AssetThumbnail } from './ResourcesPanel'
import { ImageIcon } from './icons'

function idsEqual(a: OpaqueId | null, b: OpaqueId | null): boolean {
  return a === b || (!!a && !!b && a.idx === b.idx && a.version === b.version)
}

/**
 * A Unity `Sprite`/`Texture2D` reference field — a square swatch that shows
 * the currently assigned asset's own thumbnail (or a plain placeholder when
 * `value` is `null`), and opens a non-native grid popover of every image
 * asset in the document when clicked, rather than a native `<Select>`
 * (matching this app's own established "custom `Dropdown`, never a native
 * one" convention — see `InspectorPanel.tsx`'s own `Dropdown`). Picking a
 * tile assigns it and closes the popover; a "None" tile clears it.
 *
 * Reuses `ResourcesPanel`'s own `AssetThumbnail` (the same on-demand
 * fetch-bytes-into-an-object-URL preview it already shows in the Resources
 * list) so a grid tile here and a Resources-panel row show pixel-identical
 * previews, not a second thumbnail implementation to keep in sync.
 */
export function ImageAssetPicker({
  editor,
  images,
  value,
  onChange,
}: {
  editor: WasmEditor
  images: ImageAssetSummary[]
  value: OpaqueId | null
  onChange: (value: OpaqueId | null) => void
}) {
  const [open, setOpen] = useState(false)
  const [menuRect, setMenuRect] = useState<{ top: number; left: number } | null>(null)
  const buttonRef = useRef<HTMLButtonElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const handlePointerDown = (e: PointerEvent) => {
      const target = e.target as Node
      if (buttonRef.current?.contains(target)) return
      if (menuRef.current?.contains(target)) return
      setOpen(false)
    }
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false)
    }
    const reposition = () => {
      const rect = buttonRef.current?.getBoundingClientRect()
      if (rect) setMenuRect({ top: rect.bottom + 4, left: rect.left })
    }
    window.addEventListener('pointerdown', handlePointerDown)
    window.addEventListener('keydown', handleKeyDown)
    window.addEventListener('scroll', reposition, true)
    window.addEventListener('resize', reposition)
    return () => {
      window.removeEventListener('pointerdown', handlePointerDown)
      window.removeEventListener('keydown', handleKeyDown)
      window.removeEventListener('scroll', reposition, true)
      window.removeEventListener('resize', reposition)
    }
  }, [open])

  const toggle = () => {
    if (!open) {
      const rect = buttonRef.current?.getBoundingClientRect()
      if (rect) setMenuRect({ top: rect.bottom + 4, left: rect.left })
    }
    setOpen((o) => !o)
  }

  const selected = value ? images.find((image) => idsEqual(image.id, value)) : undefined

  return (
    <div className="relative inline-block">
      <button
        ref={buttonRef}
        type="button"
        onClick={toggle}
        title={selected ? selected.name : 'No image — click to choose one'}
        className="flex h-14 w-14 items-center justify-center overflow-hidden rounded-md border border-overlay/15 bg-overlay/5 hover:border-accent/60"
      >
        {selected ? (
          <AssetThumbnail editor={editor} image={selected.id} />
        ) : (
          <span className="text-ink-dim/50">
            <ImageIcon />
          </span>
        )}
      </button>

      {open &&
        menuRect &&
        createPortal(
          <div
            ref={menuRef}
            style={{ position: 'fixed', top: menuRect.top, left: menuRect.left, width: 240 }}
            className="z-50 max-h-72 overflow-y-auto rounded-md border border-panel-border bg-canvas p-2 shadow-xl"
          >
            <div className="grid grid-cols-4 gap-1.5">
              <button
                type="button"
                onClick={() => {
                  onChange(null)
                  setOpen(false)
                }}
                title="None"
                className={`flex h-12 w-12 items-center justify-center rounded-md border text-[9px] text-ink-dim hover:border-accent/60 ${
                  !value ? 'border-accent bg-accent/15' : 'border-overlay/10 bg-overlay/5'
                }`}
              >
                None
              </button>
              {images.map((image) => (
                <button
                  key={`${image.id.idx}-${image.id.version}`}
                  type="button"
                  onClick={() => {
                    onChange(image.id)
                    setOpen(false)
                  }}
                  title={image.name}
                  className={`h-12 w-12 overflow-hidden rounded-md border hover:border-accent/60 ${
                    value && idsEqual(value, image.id) ? 'border-accent' : 'border-overlay/10'
                  }`}
                >
                  <AssetThumbnail editor={editor} image={image.id} />
                </button>
              ))}
            </div>
            {images.length === 0 && <p className="p-2 text-center text-[10px] text-ink-dim">No image assets yet — import one via the Resources panel.</p>}
          </div>,
          document.body,
        )}
    </div>
  )
}
