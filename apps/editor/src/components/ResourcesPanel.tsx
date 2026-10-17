import { audio, type SoundHandle } from '../game/audio'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import type { AudioSummary, ComponentDefinitionSummary, ImageAssetSummary, OpaqueId, PrefabSummary, SpriteGroupSummary } from '../ipc/document'
import type { WasmEditor } from '../ipc/wasmEditor'
import { getImageSource } from '../ipc/wasmEditor'
import { ChevronDownIcon, ImageIcon } from './icons'

/** A reusable vector drawing's thumbnail — `component.pathD`/`viewBox`
 * trace its own shape (see `dto.rs`'s `path_to_svg_d`), so this is a real,
 * live preview of the actual saved artwork, not a placeholder icon. Filled
 * with the component's current default color, same as a freshly-placed
 * instance would render. */
function DrawingThumbnail({ component }: { component: ComponentDefinitionSummary }) {
  const [r, g, b, a] = component.defaultFillColor
  const fill = `rgba(${Math.round(r * 255)}, ${Math.round(g * 255)}, ${Math.round(b * 255)}, ${a})`
  return (
    <svg viewBox={component.viewBox.join(' ')} className="h-8 w-8 shrink-0 rounded bg-overlay/8 p-1">
      <path d={component.pathD} fill={fill} />
    </svg>
  )
}

function DrawingRow({ component, onPlace }: { component: ComponentDefinitionSummary; onPlace: () => void }) {
  return (
    <button
      onClick={onPlace}
      title={`Place "${component.name}" on the artboard`}
      className="flex w-full items-center gap-2 rounded-md py-1 pl-1.5 pr-1.5 text-left text-ink-dim hover:bg-overlay/5 hover:text-ink"
    >
      <DrawingThumbnail component={component} />
      <span className="min-w-0 flex-1 truncate text-xs" title={component.name}>
        {component.name}
      </span>
    </button>
  )
}

function idsEqual(a: OpaqueId | null, b: OpaqueId | null): boolean {
  return a === b || (!!a && !!b && a.idx === b.idx && a.version === b.version)
}

function idKey(id: OpaqueId): string {
  return `${id.idx}-${id.version}`
}

/** Longest side of a cached thumbnail, in pixels (rows show them at 32 CSS
 * px, so 64 stays sharp on HiDPI screens). */
const THUMB_SIZE = 64

/**
 * Lightweight thumbnails, shared by every list that shows assets (this
 * panel, the image pickers). Each asset is decoded once, shrunk to at most
 * `THUMB_SIZE` px (nearest-neighbor so pixel art stays crisp) and cached
 * for the session — rows never hold full-size decoded images. Keyed by id
 * *and* a fingerprint of the bytes, so a different document reusing the
 * same ids (or a replaced asset) never shows a stale picture.
 */
const thumbnailCache = new Map<string, Promise<string | null>>()

function fingerprint(bytes: Uint8Array): string {
  let h = 2166136261
  const step = Math.max(1, Math.floor(bytes.length / 64))
  for (let i = 0; i < bytes.length; i += step) h = Math.imul(h ^ bytes[i], 16777619)
  return `${bytes.length}:${(h >>> 0).toString(36)}`
}

async function makeThumbnail(bytes: Uint8Array): Promise<string | null> {
  const blob = new Blob([bytes as unknown as BlobPart], { type: 'image/png' })
  try {
    const full = await createImageBitmap(blob)
    const scale = Math.min(1, THUMB_SIZE / Math.max(full.width, full.height))
    if (scale >= 1) {
      // Already small: the PNG itself is the thumbnail.
      full.close()
      return URL.createObjectURL(blob)
    }
    const w = Math.max(1, Math.round(full.width * scale))
    const h = Math.max(1, Math.round(full.height * scale))
    const canvas = document.createElement('canvas')
    canvas.width = w
    canvas.height = h
    const g = canvas.getContext('2d')
    if (!g) return null
    g.imageSmoothingEnabled = false
    g.drawImage(full, 0, 0, w, h)
    full.close()
    const small = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'))
    return small ? URL.createObjectURL(small) : null
  } catch (err) {
    console.error('failed to make asset thumbnail', err)
    return null
  }
}

function thumbnailFor(editor: WasmEditor, image: OpaqueId): Promise<string | null> {
  const bytes = getImageSource(editor, image)
  const key = `${idKey(image)}|${fingerprint(bytes)}`
  let entry = thumbnailCache.get(key)
  if (!entry) {
    entry = makeThumbnail(bytes)
    thumbnailCache.set(key, entry)
  }
  return entry
}

/**
 * A small pixel preview for one asset. Loads only once the row scrolls into
 * view (a long sprite-frame list costs nothing until you look at it), then
 * uses the shared thumbnail cache above.
 */
export function AssetThumbnail({ editor, image }: { editor: WasmEditor; image: OpaqueId }) {
  const [url, setUrl] = useState<string | null>(null)
  const [visible, setVisible] = useState(false)
  const box = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const el = box.current
    if (!el || visible) return
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        setVisible(true)
        observer.disconnect()
      }
    }, { rootMargin: '120px' })
    observer.observe(el)
    return () => observer.disconnect()
  }, [visible])

  useEffect(() => {
    if (!visible) return
    let cancelled = false
    try {
      void thumbnailFor(editor, image).then((u) => {
        if (!cancelled) setUrl(u)
      })
    } catch (err) {
      console.error('failed to load asset thumbnail', err)
    }
    return () => {
      cancelled = true
    }
  }, [visible, editor, image.idx, image.version])

  if (!url) return <div ref={box} className="h-8 w-8 shrink-0 rounded bg-overlay/8" />
  return <img src={url} alt="" className="h-8 w-8 shrink-0 rounded bg-overlay/8 object-cover" style={{ imageRendering: 'pixelated' }} />
}

function AssetRow({
  editor,
  asset,
  selected,
  indent,
  onSelect,
}: {
  editor: WasmEditor
  asset: ImageAssetSummary
  selected: boolean
  indent?: boolean
  onSelect: () => void
}) {
  return (
    <button
      onClick={onSelect}
      className={`flex w-full items-center gap-2 rounded-md py-1 pl-1.5 pr-1.5 text-left ${indent ? 'ml-5 w-[calc(100%-1.25rem)]' : ''} ${
        selected ? 'bg-accent/15 text-accent' : 'text-ink-dim hover:bg-overlay/5 hover:text-ink'
      }`}
    >
      <AssetThumbnail editor={editor} image={asset.id} />
      <span className="min-w-0 flex-1 truncate text-xs" title={asset.name}>
        {asset.name}
      </span>
      {asset.width !== null && asset.height !== null && (
        <span className="shrink-0 text-[10px] tabular-nums text-ink-dim/70">
          {asset.width}&times;{asset.height}
        </span>
      )}
    </button>
  )
}

/** A small cube glyph — Unity's prefab icon, in the accent color. */
function PrefabIcon() {
  return (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" aria-hidden>
      <path d="M8 1.8 13.5 4.8v6.4L8 14.2 2.5 11.2V4.8Z" />
      <path d="M2.5 4.8 8 7.8l5.5-3M8 7.8v6.4" />
    </svg>
  )
}

/** One prefab in the Prefabs folder: place it, rename (double-click), delete. */
function PrefabRow({
  prefab,
  onPlace,
  onRename,
  onDelete,
}: {
  prefab: PrefabSummary
  onPlace: () => void
  onRename: (name: string) => void
  onDelete: () => void
}) {
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(prefab.name)
  return (
    <div className="group flex items-center gap-2 rounded-md px-1.5 py-1 hover:bg-overlay/5">
      <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-accent/12 text-accent">
        <PrefabIcon />
      </span>
      {editing ? (
        <input
          autoFocus
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={() => {
            setEditing(false)
            if (draft.trim() && draft !== prefab.name) onRename(draft.trim())
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter') (e.target as HTMLInputElement).blur()
            if (e.key === 'Escape') {
              setDraft(prefab.name)
              setEditing(false)
            }
          }}
          className="min-w-0 flex-1 rounded bg-overlay/8 px-1 text-xs text-ink outline-none"
        />
      ) : (
        <span
          className="min-w-0 flex-1 truncate text-xs text-ink"
          title={`${prefab.name} — ${prefab.nodeCount} node${prefab.nodeCount === 1 ? '' : 's'}. Double-click to rename. Scripts: ctx.spawn('${prefab.name}', x, y)`}
          onDoubleClick={() => {
            setDraft(prefab.name)
            setEditing(true)
          }}
        >
          {prefab.name}
        </span>
      )}
      <span className="shrink-0 text-[10px] tabular-nums text-ink-dim/70">{prefab.nodeCount}</span>
      <button onClick={onPlace} title="Place in scene" className="rounded px-1.5 py-0.5 text-[10px] font-medium text-accent hover:bg-accent/15">
        Place
      </button>
      <button onClick={onDelete} title="Delete prefab (instances become plain objects)" className="hidden h-5 w-5 items-center justify-center rounded text-ink-dim hover:bg-overlay/8 hover:text-ink group-hover:flex">
        ×
      </button>
    </div>
  )
}

/** A collapsible "folder" header — the panel mirrors the project's
 * `Assets/` folders on disk. */
function Folder({ name, count, open, onToggle, action, children }: { name: string; count: number; open: boolean; onToggle: () => void; action?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-1">
        <button onClick={onToggle} className="flex min-w-0 flex-1 items-center gap-1.5 rounded-md px-1 py-0.5 text-left text-[11px] font-medium uppercase tracking-wide text-ink-dim hover:text-ink">
          <span className={`transition-transform ${open ? '' : '-rotate-90'}`}>
            <ChevronDownIcon />
          </span>
          <span className="truncate">{name}</span>
          <span className="ml-auto text-[10px] font-normal normal-case tabular-nums text-ink-dim/70">{count}</span>
        </button>
        {action}
      </div>
      {open && <div className="flex flex-col gap-1 pl-2">{children}</div>}
    </div>
  )
}

/**
 * The editor's Resources panel (Unity Project-window-style) — browses every
 * raster image asset in the document. Standalone images render as flat
 * rows; a GIF's decoded frames (a `SpriteGroup`) render as one collapsible
 * group header with its member frames indented beneath, same
 * collapse-state-as-a-`Set` pattern `LayersPanel`'s group rows already use.
 * Frames-only this pass (see `editor.rs`'s `import_image` doc comment) — a
 * frame is just an ordinary selectable/placeable image, nothing here plays
 * a group back as an animation.
 *
 * Selecting a row is mutually exclusive with node selection (`App.tsx`
 * clears `selection`/`multiSelectedIds` when `onSelectAsset` fires, and vice
 * versa) — clicking here vs. the Layers panel/canvas both drive the same
 * Inspector, the same way Unity's Project and Hierarchy windows both feed
 * one Inspector.
 */
/** One sound in the Sounds folder: name, size, preview, delete. */
function SoundRow({ editor, sound, onDelete }: { editor: WasmEditor | null; sound: AudioSummary; onDelete: () => void }) {
  const handle = useRef<SoundHandle | null>(null)
  const [playing, setPlaying] = useState(false)
  useEffect(() => () => handle.current?.stop(), [])
  const toggle = () => {
    if (handle.current?.playing) {
      handle.current.stop()
      setPlaying(false)
      return
    }
    if (!editor) return
    handle.current = audio.play(editor, sound.id)
    setPlaying(true)
    const tick = () => (handle.current?.playing ? window.setTimeout(tick, 200) : setPlaying(false))
    window.setTimeout(tick, 200)
  }
  return (
    <div className="group flex items-center gap-1.5 rounded px-2 py-1 text-[11px] text-ink hover:bg-overlay/5">
      <button onClick={toggle} title={playing ? 'Stop' : 'Play'} className="flex h-5 w-5 shrink-0 items-center justify-center rounded bg-overlay/10 text-[10px] hover:bg-accent/20 hover:text-accent">
        {playing ? '■' : '▶'}
      </button>
      <span className="min-w-0 flex-1 truncate" title={sound.file ?? sound.name}>
        {sound.name}
      </span>
      <span className="shrink-0 tabular-nums text-ink-dim">{Math.max(1, Math.round(sound.byteLength / 1024))} KB</span>
      <button onClick={onDelete} title="Delete sound" className="shrink-0 rounded px-1 text-ink-dim opacity-0 hover:text-red-400 group-hover:opacity-100">
        ✕
      </button>
    </div>
  )
}

export function ResourcesPanel({
  editor,
  images,
  spriteGroups,
  components,
  selectedAssetId,
  onSelectAsset,
  onImportImage,
  onPlaceComponent,
  prefabs,
  scenes,
  projectName,
  projectFolder,
  onPlacePrefab,
  onRenamePrefab,
  onDeletePrefab,
  onOpenScene,
  onShowInExplorer,
  sounds,
  onImportAudio,
  onDeleteAudio,
  onDropFiles,
}: {
  /** Files dropped on the panel from the OS (images, SVGs, sounds). */
  onDropFiles?: (files: File[]) => void
  editor: WasmEditor | null
  /** Imported sounds. */
  sounds: AudioSummary[]
  onImportAudio: () => void
  onDeleteAudio: (id: OpaqueId) => void
  images: ImageAssetSummary[]
  spriteGroups: SpriteGroupSummary[]
  /** Vector drawings saved from Paint mode ("Save as Resource") and anything
   * else made into a reusable component — the same library the Components
   * panel edits, just browsable here too so a saved drawing actually shows
   * up where "Resources" would suggest, not only in a separate panel. */
  components: ComponentDefinitionSummary[]
  selectedAssetId: OpaqueId | null
  onSelectAsset: (id: OpaqueId) => void
  onImportImage: () => void
  onPlaceComponent: (id: OpaqueId) => void
  prefabs: PrefabSummary[]
  /** The project's scenes (one `.scene` file each on disk). */
  scenes: { id: OpaqueId; name: string }[]
  /** The open project, or `null` for a folder-less document. */
  projectName: string | null
  projectFolder: string | null
  onPlacePrefab: (id: OpaqueId) => void
  onRenamePrefab: (id: OpaqueId, name: string) => void
  onDeletePrefab: (id: OpaqueId) => void
  onOpenScene: (id: OpaqueId) => void
  onShowInExplorer: () => void
}) {
  const [closedFolders, setClosedFolders] = useState<Set<string>>(new Set())
  const folder = (name: string) => ({
    open: !closedFolders.has(name),
    onToggle: () =>
      setClosedFolders((prev) => {
        const next = new Set(prev)
        if (next.has(name)) next.delete(name)
        else next.add(name)
        return next
      }),
  })
  // Sprite groups start collapsed (a sheet can hold dozens of frames); the
  // set tracks the ones the user has opened.
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set())
  // A file being dragged over the panel (from the OS): highlight the drop.
  const [dropping, setDropping] = useState(false)

  if (!editor) return null

  const groupedIds = new Set(spriteGroups.flatMap((group) => group.frames.map(([id]) => idKey(id))))
  const standalone = images.filter((image) => !groupedIds.has(idKey(image.id)))
  const imagesById = new Map(images.map((image) => [idKey(image.id), image]))

  const hasFiles = (e: React.DragEvent) => Array.from(e.dataTransfer.types).includes('Files')
  return (
    <div
      className={`relative flex flex-col gap-3 overflow-x-hidden rounded-md ${dropping ? 'ring-2 ring-accent ring-offset-2 ring-offset-transparent' : ''}`}
      onDragOver={(e) => {
        if (!onDropFiles || !hasFiles(e)) return
        e.preventDefault()
        e.dataTransfer.dropEffect = 'copy'
        setDropping(true)
      }}
      onDragLeave={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setDropping(false)
      }}
      onDrop={(e) => {
        setDropping(false)
        if (!onDropFiles || !hasFiles(e)) return
        e.preventDefault()
        onDropFiles(Array.from(e.dataTransfer.files))
      }}
    >
      {dropping && (
        <div className="pointer-events-none absolute inset-0 z-10 grid place-items-center rounded-md bg-accent/10 text-xs font-medium text-accent">Drop to add to Resources</div>
      )}
      <div className="flex items-center gap-2 rounded-md border border-panel-border px-2 py-1.5">
        <div className="min-w-0 flex-1">
          <div className="truncate text-xs font-medium text-ink">{projectName ?? 'Unsaved document'}</div>
          <div className="truncate text-[10px] text-ink-dim" title={projectFolder ?? undefined}>
            {projectFolder ?? 'File ▸ New Project… saves it as a folder of files'}
          </div>
        </div>
        {projectFolder && (
          <button onClick={onShowInExplorer} title="Show in Explorer" className="shrink-0 rounded px-1.5 py-0.5 text-[10px] text-ink-dim hover:bg-overlay/8 hover:text-ink">
            Open folder
          </button>
        )}
      </div>

      <Folder name="Scenes" count={scenes.length} {...folder('Scenes')}>
        {scenes.map((scene) => (
          <button
            key={idKey(scene.id)}
            onDoubleClick={() => onOpenScene(scene.id)}
            title="Double-click to open"
            className="truncate rounded-md px-1.5 py-1 text-left text-xs text-ink hover:bg-overlay/5"
          >
            {scene.name}
          </button>
        ))}
      </Folder>

      <Folder name="Prefabs" count={prefabs.length} {...folder('Prefabs')}>
        {prefabs.length === 0 && <p className="px-1.5 py-1 text-[11px] leading-snug text-ink-dim">Select an object and press Create Prefab in the Inspector.</p>}
        {prefabs.map((prefab) => (
          <PrefabRow
            key={idKey(prefab.id)}
            prefab={prefab}
            onPlace={() => onPlacePrefab(prefab.id)}
            onRename={(name) => onRenamePrefab(prefab.id, name)}
            onDelete={() => onDeletePrefab(prefab.id)}
          />
        ))}
      </Folder>

      {components.length > 0 && (
        <div className="flex flex-col gap-2">
          <span className="text-[11px] font-medium uppercase tracking-wide text-ink-dim">Drawings</span>
          {components.map((component) => (
            <DrawingRow key={`${component.id.idx}:${component.id.version}`} component={component} onPlace={() => onPlaceComponent(component.id)} />
          ))}
        </div>
      )}

      <Folder
        name="Sounds"
        count={sounds.length}
        {...folder('Sounds')}
        action={
          <button onClick={onImportAudio} title="Import a sound (MP3, WAV, OGG…)" className="flex h-5 w-5 items-center justify-center rounded text-ink-dim hover:bg-overlay/5 hover:text-ink">
            +
          </button>
        }
      >
        {sounds.length === 0 ? (
          <p className="px-2 py-2 text-[11px] text-ink-dim">No sounds yet. Import one, then give a node an Audio Source or play it from a script: ctx.playSound('name').</p>
        ) : (
          sounds.map((sound) => <SoundRow key={idKey(sound.id)} editor={editor} sound={sound} onDelete={() => onDeleteAudio(sound.id)} />)
        )}
      </Folder>

      <Folder
        name="Images"
        count={images.length}
        {...folder('Images')}
        action={
          <button onClick={onImportImage} title="Import Image/GIF…" className="flex h-5 w-5 items-center justify-center rounded text-ink-dim hover:bg-overlay/5 hover:text-ink">
            +
          </button>
        }
      >

      {images.length === 0 && spriteGroups.length === 0 && (
        <p className="flex flex-col items-center gap-2 py-6 text-center text-xs text-ink-dim">
          <ImageIcon />
          No images yet. Import a PNG, JPEG, or GIF to get started.
        </p>
      )}

      {standalone.map((asset) => (
        <AssetRow key={idKey(asset.id)} editor={editor} asset={asset} selected={idsEqual(selectedAssetId, asset.id)} onSelect={() => onSelectAsset(asset.id)} />
      ))}

      {spriteGroups.map((group) => {
        const key = idKey(group.id)
        const collapsed = !expandedGroups.has(key)
        const frames = group.frames.map(([id]) => imagesById.get(idKey(id))).filter((asset): asset is ImageAssetSummary => asset !== undefined)
        return (
          <div key={key} className="flex flex-col gap-1">
            <button
              onClick={() =>
                setExpandedGroups((prev) => {
                  const next = new Set(prev)
                  if (next.has(key)) next.delete(key)
                  else next.add(key)
                  return next
                })
              }
              className="flex w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left text-xs text-ink-dim hover:bg-overlay/5 hover:text-ink"
            >
              <span className={`transition-transform ${collapsed ? '-rotate-90' : ''}`}>
                <ChevronDownIcon />
              </span>
              <span className="min-w-0 flex-1 truncate font-medium" title={group.name}>
                {group.name}
              </span>
              <span className="shrink-0 text-[10px] tabular-nums text-ink-dim/70">{frames.length} frames</span>
            </button>
            {!collapsed &&
              frames.map((asset) => (
                <AssetRow key={idKey(asset.id)} editor={editor} asset={asset} indent selected={idsEqual(selectedAssetId, asset.id)} onSelect={() => onSelectAsset(asset.id)} />
              ))}
          </div>
        )
      })}
      </Folder>
    </div>
  )
}
