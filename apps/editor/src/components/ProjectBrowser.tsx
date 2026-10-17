/**
 * Resources — Unity's Project window: a view of the project *folder*.
 *
 * Left: the folder tree. Right: the selected folder's contents as tiles.
 * With a project open, this is literally the folder on disk (`listProjectFiles`)
 * — scenes, prefabs and images are matched to their document items through
 * `projectFileIndex` (the same paths a save writes), so they're live: open a
 * scene, drag a prefab or image into the canvas, select an image to edit its
 * import settings in the Inspector. Any other file in the folder shows up as
 * a plain file. Without a project, the same view shows the document's own
 * assets in the folder layout a save would create.
 *
 * Tiles are draggable (`ASSET_DRAG_MIME`); `App.tsx`'s canvas accepts the
 * drop and places the prefab / image there.
 */
import { useEffect, useMemo, useState, type DragEvent, type MouseEvent as ReactMouseEvent, type ReactNode } from 'react'
import type { ComponentDefinitionSummary, DocumentSummary, DrawingSummary, ImageAssetSummary, OpaqueId, ProjectFileListing } from '../ipc/document'
import type { ProjectEntry } from '../ipc/nativeFile'
import { projectFileIndex, type WasmEditor } from '../ipc/wasmEditor'
import { renderFrame, thumbnail } from '../paint/session'
import { sessionFromDrawing } from '../paint/drawingIo'
import { AssetThumbnail } from './ResourcesPanel'
import { ChevronDownIcon, ImageIcon } from './icons'

/** `dataTransfer` type for a tile dragged onto the canvas: JSON `{ kind, id }`. */
export const ASSET_DRAG_MIME = 'application/x-an1mbyt3-asset'

export type DraggedAsset = { kind: 'prefab' | 'image' | 'drawing' | 'component'; id: OpaqueId }

type ItemKind = 'scene' | 'prefab' | 'image' | 'audio' | 'model' | 'drawing' | 'component' | 'file'

interface BrowserItem {
  /** Project-relative, `/`-separated. */
  path: string
  name: string
  kind: ItemKind
  id: OpaqueId | null
  size: number | null
}

function idsEqual(a: OpaqueId | null, b: OpaqueId | null): boolean {
  return a === b || (!!a && !!b && a.idx === b.idx && a.version === b.version)
}

const folderOf = (path: string) => (path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '')
const fileName = (path: string) => path.slice(path.lastIndexOf('/') + 1)
const IMAGE_EXT = /\.(png|jpe?g|gif|webp)$/i

/** Every ancestor folder of `path` (`a/b/c.png` -> `a`, `a/b`). */
function ancestors(path: string): string[] {
  const parts = path.split('/').slice(0, -1)
  return parts.map((_, i) => parts.slice(0, i + 1).join('/'))
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

/** Resources ▸ Drawings: saved Paint drawings, plus (older) single-shape
 * components made by the previous "Save as Resource". Paths carry the id so
 * two drawings with the same name stay distinct tiles. */
function drawingItems(doc: DocumentSummary): BrowserItem[] {
  return [
    ...doc.drawings.map((d) => ({ path: `Assets/Drawings/${d.name}#${d.id.idx}`, name: d.name, kind: 'drawing' as const, id: d.id, size: null })),
    ...doc.components.map((c) => ({ path: `Assets/Drawings/${c.name}#c${c.id.idx}`, name: c.name, kind: 'component' as const, id: c.id, size: null })),
  ]
}

/** A drawing's thumbnail: its first frame, every visible layer, drawn the
 * way Paint shows it (`paint/session.ts`). Animated drawings get a frame
 * count badge. */
function DrawingThumbnail({ editor, drawing }: { editor: WasmEditor; drawing: DrawingSummary }) {
  const [url, setUrl] = useState<string | null>(null)
  useEffect(() => {
    let cancelled = false
    ;(async () => {
      const session = sessionFromDrawing(editor, drawing.id)
      const frame = await renderFrame(session, 0)
      if (!cancelled) setUrl(thumbnail(frame, 80))
    })().catch(() => !cancelled && setUrl(null))
    return () => {
      cancelled = true
    }
    // The summary object changes when the drawing does (re-save, rename).
  }, [editor, drawing])
  return (
    <span className="relative flex h-full w-full items-center justify-center">
      {url ? <img src={url} alt="" className="h-10 w-10 object-contain" draggable={false} /> : <KindGlyph kind="drawing" name={drawing.name} />}
      {drawing.frameCount > 1 && <span className="absolute bottom-0 right-0 rounded-tl bg-black/60 px-0.5 text-[9px] tabular-nums leading-tight text-white">{drawing.frameCount}f</span>}
    </span>
  )
}

/** Items for a folder-less document: the layout a save would produce. */
function virtualItems(doc: DocumentSummary, index: ProjectFileListing[]): BrowserItem[] {
  const items: BrowserItem[] = index
    .filter((f) => f.asset)
    .map((f) => ({ path: f.path, name: fileName(f.path), kind: f.asset!.kind, id: f.asset!.id, size: null }))
  items.push(...drawingItems(doc))
  return items
}

function KindGlyph({ kind, name }: { kind: ItemKind; name: string }) {
  const label = kind === 'scene' ? 'SCN' : kind === 'prefab' ? '' : kind === 'drawing' ? 'SVG' : (name.split('.').pop() ?? '').slice(0, 4).toUpperCase()
  if (kind === 'prefab') {
    return (
      <svg viewBox="0 0 16 16" width="26" height="26" fill="none" stroke="currentColor" strokeWidth="1.1" strokeLinejoin="round" aria-hidden className="text-accent">
        <path d="M8 1.8 13.5 4.8v6.4L8 14.2 2.5 11.2V4.8Z" />
        <path d="M2.5 4.8 8 7.8l5.5-3M8 7.8v6.4" />
      </svg>
    )
  }
  if (kind === 'image') return <ImageIcon />
  return <span className={`text-[10px] font-semibold tracking-wide ${kind === 'scene' ? 'text-accent' : 'text-ink-dim'}`}>{label || 'FILE'}</span>
}

function FolderGlyph({ open }: { open?: boolean }) {
  return (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" aria-hidden className="shrink-0 opacity-80">
      <path d={open ? 'M1.5 4.5a1 1 0 0 1 1-1h3.2l1.3 1.4h6.5a1 1 0 0 1 1 1v.6H3.4L1.5 11Z M3.4 6.5h11.4l-1.6 6H1.8Z' : 'M1.5 4a1 1 0 0 1 1-1h3.2L7 4.4h6.5a1 1 0 0 1 1 1V12a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1Z'} />
    </svg>
  )
}

export function ProjectBrowser({
  editor,
  doc,
  projectName,
  projectFolder,
  projectFiles,
  selectedAssetId,
  onSelectAsset,
  onImportImage,
  onPlacePrefab,
  onPlaceImage,
  onPlaceDrawing,
  onPlaceComponent,
  onEditDrawing,
  onRenameDrawing,
  onDeleteDrawing,
  onRenamePrefab,
  onDeletePrefab,
  onOpenScene,
  onShowInExplorer,
  onNewProject,
  onOpenProject,
  onRefresh,
  onDropFiles,
}: {
  editor: WasmEditor | null
  doc: DocumentSummary | null
  projectName: string | null
  projectFolder: string | null
  /** The project folder's files on disk (empty without a project). */
  projectFiles: ProjectEntry[]
  selectedAssetId: OpaqueId | null
  onSelectAsset: (id: OpaqueId) => void
  onImportImage: () => void
  onPlacePrefab: (id: OpaqueId) => void
  onPlaceImage: (id: OpaqueId) => void
  /** A saved Paint drawing: place it / reopen it in Paint / rename / delete. */
  onPlaceDrawing: (id: OpaqueId) => void
  onEditDrawing: (id: OpaqueId) => void
  onRenameDrawing: (id: OpaqueId, name: string) => void
  onDeleteDrawing: (id: OpaqueId) => void
  /** An older single-shape drawing (a component): place an instance. */
  onPlaceComponent: (id: OpaqueId) => void
  onRenamePrefab: (id: OpaqueId, name: string) => void
  onDeletePrefab: (id: OpaqueId) => void
  onOpenScene: (id: OpaqueId) => void
  onShowInExplorer: () => void
  onNewProject: () => void
  onOpenProject: () => void
  onRefresh: () => void
  /** Files dropped here from the OS (images, SVGs, sounds) — imported as assets. */
  onDropFiles?: (files: File[]) => void
}) {
  // Which file holds which item — recomputed shortly after the document
  // changes (it serializes the document, so not on every keystroke).
  const [index, setIndex] = useState<ProjectFileListing[]>([])
  // A file dragged in from the OS is over the panel.
  const [dropping, setDropping] = useState(false)
  useEffect(() => {
    if (!editor || !doc) return
    const timer = window.setTimeout(() => {
      try {
        setIndex(projectFileIndex(editor, projectName ?? 'Untitled'))
      } catch (err) {
        console.warn('project index failed', err)
      }
    }, 250)
    return () => window.clearTimeout(timer)
  }, [editor, doc, projectName])

  const items: BrowserItem[] = useMemo(() => {
    if (!doc) return []
    if (!projectFolder) return virtualItems(doc, index)
    // Disk files, matched to document items by path; plus items the next
    // save will write but that aren't on disk yet (made in the last second).
    const byPath = new Map(index.filter((f) => f.asset).map((f) => [f.path, f.asset!]))
    const seen = new Set<string>()
    const out: BrowserItem[] = projectFiles.map((entry) => {
      seen.add(entry.path)
      const asset = byPath.get(entry.path)
      return { path: entry.path, name: fileName(entry.path), kind: asset ? asset.kind : 'file', id: asset?.id ?? null, size: entry.size }
    })
    for (const [path, asset] of byPath) if (!seen.has(path)) out.push({ path, name: fileName(path), kind: asset.kind, id: asset.id, size: null })
    out.push(...drawingItems(doc))
    return out
  }, [doc, index, projectFolder, projectFiles])

  const folders = useMemo(() => {
    const set = new Set<string>(['Assets', 'Assets/Scenes', 'Assets/Prefabs', 'Assets/Images'])
    for (const item of items) for (const folder of ancestors(item.path)) set.add(folder)
    return [...set].sort((a, b) => a.localeCompare(b))
  }, [items])

  const [current, setCurrent] = useState('Assets')
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set())
  const [query, setQuery] = useState('')
  const [selectedPath, setSelectedPath] = useState<string | null>(null)
  const [menu, setMenu] = useState<{ x: number; y: number; item: BrowserItem } | null>(null)
  const [renaming, setRenaming] = useState<string | null>(null)

  useEffect(() => {
    if (!folders.includes(current)) setCurrent('Assets')
  }, [folders, current])
  useEffect(() => {
    if (!menu) return
    const close = () => setMenu(null)
    window.addEventListener('mousedown', close)
    return () => window.removeEventListener('mousedown', close)
  }, [menu])

  const images = useMemo(() => new Map((doc?.images ?? []).map((i) => [`${i.id.idx}:${i.id.version}`, i])), [doc])
  const components = useMemo(() => new Map((doc?.components ?? []).map((c) => [`${c.id.idx}:${c.id.version}`, c])), [doc])
  const drawings = useMemo(() => new Map((doc?.drawings ?? []).map((d) => [`${d.id.idx}:${d.id.version}`, d])), [doc])

  // Folder contents (or search results across everything).
  const q = query.trim().toLowerCase()
  const subfolders = q ? [] : folders.filter((f) => folderOf(f) === current)
  const files = (q ? items.filter((i) => i.name.toLowerCase().includes(q)) : items.filter((i) => folderOf(i.path) === current)).sort((a, b) => a.name.localeCompare(b.name))

  if (!editor || !doc) return null

  const open = (item: BrowserItem) => {
    if (!item.id) return
    if (item.kind === 'scene') onOpenScene(item.id)
    else if (item.kind === 'prefab') onPlacePrefab(item.id)
    else if (item.kind === 'image') onPlaceImage(item.id)
    else if (item.kind === 'drawing') onEditDrawing(item.id)
    else if (item.kind === 'component') onPlaceComponent(item.id)
  }
  const select = (item: BrowserItem) => {
    setSelectedPath(item.path)
    if (item.kind === 'image' && item.id) onSelectAsset(item.id)
  }
  const dragStart = (event: DragEvent, item: BrowserItem) => {
    if (!item.id || (item.kind !== 'prefab' && item.kind !== 'image' && item.kind !== 'drawing' && item.kind !== 'component')) {
      event.preventDefault()
      return
    }
    const payload: DraggedAsset = { kind: item.kind, id: item.id }
    event.dataTransfer.setData(ASSET_DRAG_MIME, JSON.stringify(payload))
    event.dataTransfer.effectAllowed = 'copy'
  }
  const toggleFolder = (folder: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev)
      if (next.has(folder)) next.delete(folder)
      else next.add(folder)
      return next
    })

  const treeRow = (folder: string, depth: number): ReactNode => {
    const children = folders.filter((f) => folderOf(f) === folder)
    const isOpen = !collapsed.has(folder)
    return (
      <div key={folder}>
        <div
          onClick={() => {
            setCurrent(folder)
            setQuery('')
          }}
          className={`flex cursor-default items-center gap-1 rounded px-1 py-0.5 text-xs ${current === folder && !q ? 'bg-accent/15 text-accent' : 'text-ink-dim hover:bg-overlay/5 hover:text-ink'}`}
          style={{ paddingLeft: 4 + depth * 12 }}
        >
          <button
            onClick={(e) => {
              e.stopPropagation()
              toggleFolder(folder)
            }}
            className={`flex h-4 w-4 items-center justify-center ${children.length ? '' : 'invisible'}`}
          >
            <span className={`transition-transform ${isOpen ? '' : '-rotate-90'}`}>
              <ChevronDownIcon />
            </span>
          </button>
          <FolderGlyph open={current === folder} />
          <span className="truncate">{fileName(folder)}</span>
        </div>
        {isOpen && children.map((child) => treeRow(child, depth + 1))}
      </div>
    )
  }

  const thumbnail = (item: BrowserItem) => {
    if (item.kind === 'image' && item.id && images.has(`${item.id.idx}:${item.id.version}`)) {
      return (
        <div className="scale-150">
          <AssetThumbnail editor={editor} image={item.id} />
        </div>
      )
    }
    if (item.kind === 'drawing' && item.id) {
      const drawing = drawings.get(`${item.id.idx}:${item.id.version}`)
      if (drawing) return <DrawingThumbnail editor={editor} drawing={drawing} />
    }
    if (item.kind === 'component' && item.id) {
      const c: ComponentDefinitionSummary | undefined = components.get(`${item.id.idx}:${item.id.version}`)
      if (c) {
        const [r, g, b, a] = c.defaultFillColor
        return (
          <svg viewBox={c.viewBox.join(' ')} className="h-9 w-9" aria-hidden>
            <path d={c.pathD} fill={`rgba(${r * 255}, ${g * 255}, ${b * 255}, ${a})`} />
          </svg>
        )
      }
    }
    if (item.kind === 'file' && IMAGE_EXT.test(item.name)) return <ImageIcon />
    return <KindGlyph kind={item.kind} name={item.name} />
  }

  const breadcrumb = current.split('/')

  const hasFiles = (e: DragEvent) => Array.from(e.dataTransfer.types).includes('Files')
  return (
    <div
      className={`relative flex h-full min-h-0 flex-col gap-1.5 rounded-md ${dropping ? 'ring-2 ring-accent' : ''}`}
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
      {dropping && <div className="pointer-events-none absolute inset-0 z-10 grid place-items-center rounded-md bg-accent/10 text-xs font-medium text-accent">Drop to add to Resources</div>}
      {/* Toolbar */}
      <div className="flex shrink-0 items-center gap-2">
        <span className="shrink-0 text-[11px] font-semibold text-ink" title={projectFolder ?? 'Not saved as a project yet'}>
          {projectName ?? 'Unsaved document'}
        </span>
        <div className="flex min-w-0 items-center gap-0.5 text-[11px] text-ink-dim">
          {q ? (
            <span>Search results</span>
          ) : (
            breadcrumb.map((part, i) => (
              <span key={i} className="flex items-center gap-0.5">
                <span className="opacity-50">/</span>
                <button onClick={() => setCurrent(breadcrumb.slice(0, i + 1).join('/'))} className="rounded px-1 hover:bg-overlay/8 hover:text-ink">
                  {part}
                </button>
              </span>
            ))
          )}
        </div>
        <div className="flex-1" />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search"
          className="h-6 w-36 rounded-md border border-panel-border bg-overlay/5 px-2 text-[11px] text-ink outline-none placeholder:text-ink-dim focus:border-accent/60"
        />
        <button onClick={onImportImage} title="Import an image — copied into Assets/Images" className="h-6 rounded-md bg-accent/15 px-2 text-[11px] font-medium text-accent hover:bg-accent/25">
          Import…
        </button>
        {projectFolder ? (
          <>
            <button onClick={onRefresh} title="Rescan the folder" className="h-6 rounded-md px-2 text-[11px] text-ink-dim hover:bg-overlay/8 hover:text-ink">
              Refresh
            </button>
            <button onClick={onShowInExplorer} title={projectFolder} className="h-6 rounded-md px-2 text-[11px] text-ink-dim hover:bg-overlay/8 hover:text-ink">
              Open Folder
            </button>
          </>
        ) : (
          <>
            <button onClick={onNewProject} className="h-6 rounded-md px-2 text-[11px] text-ink-dim hover:bg-overlay/8 hover:text-ink" title="Save this document as a project folder">
              Save as Project…
            </button>
            <button onClick={onOpenProject} className="h-6 rounded-md px-2 text-[11px] text-ink-dim hover:bg-overlay/8 hover:text-ink">
              Open Project…
            </button>
          </>
        )}
      </div>

      <div className="flex min-h-0 flex-1 gap-2">
        {/* Folder tree */}
        <div className="w-48 shrink-0 overflow-y-auto border-r border-overlay/10 pr-1">{folders.filter((f) => folderOf(f) === '').map((root) => treeRow(root, 0))}</div>

        {/* Folder contents */}
        <div
          className="min-w-0 flex-1 overflow-y-auto"
          onClick={(e) => {
            if (e.target === e.currentTarget) setSelectedPath(null)
          }}
        >
          {subfolders.length === 0 && files.length === 0 && (
            <p className="py-6 text-center text-xs text-ink-dim">{q ? 'Nothing matches.' : 'This folder is empty.'}</p>
          )}
          <div className="grid gap-1" style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(84px, 1fr))' }}>
            {subfolders.map((folder) => (
              <button
                key={folder}
                onDoubleClick={() => setCurrent(folder)}
                onClick={() => setSelectedPath(folder)}
                className={`flex flex-col items-center gap-1 rounded-md p-1.5 text-center ${selectedPath === folder ? 'bg-accent/15' : 'hover:bg-overlay/5'}`}
              >
                <span className="flex h-11 w-11 items-center justify-center text-ink-dim [&>svg]:h-9 [&>svg]:w-9">
                  <FolderGlyph />
                </span>
                <span className="line-clamp-2 w-full break-words text-[11px] leading-tight text-ink">{fileName(folder)}</span>
              </button>
            ))}
            {files.map((item) => {
              const selected = selectedPath === item.path || (item.kind === 'image' && idsEqual(item.id, selectedAssetId))
              const image: ImageAssetSummary | undefined = item.kind === 'image' && item.id ? images.get(`${item.id.idx}:${item.id.version}`) : undefined
              const prefab = item.kind === 'prefab' && item.id ? doc.prefabs.find((p) => idsEqual(p.id, item.id)) : undefined
              const title = [
                item.path,
                image?.width && image?.height ? `${image.width}×${image.height}` : null,
                item.size !== null ? formatSize(item.size) : null,
                prefab ? `${prefab.nodeCount} nodes — script: ctx.spawn('${prefab.name}', x, y)` : null,
                item.kind === 'scene'
                  ? 'Double-click to open'
                  : item.kind === 'drawing'
                    ? 'Drag into the scene to place — double-click to edit in Paint'
                    : item.kind === 'prefab' || item.kind === 'image' || item.kind === 'component'
                      ? 'Double-click or drag into the scene to place'
                      : null,
              ]
                .filter(Boolean)
                .join('\n')
              return (
                <div
                  key={item.path}
                  draggable={item.kind === 'prefab' || item.kind === 'image' || item.kind === 'drawing' || item.kind === 'component'}
                  onDragStart={(e) => dragStart(e, item)}
                  onClick={() => select(item)}
                  onDoubleClick={() => open(item)}
                  onContextMenu={(e: ReactMouseEvent) => {
                    e.preventDefault()
                    select(item)
                    setMenu({ x: e.clientX, y: e.clientY, item })
                  }}
                  title={title}
                  className={`flex cursor-default flex-col items-center gap-1 rounded-md p-1.5 text-center ${selected ? 'bg-accent/15' : 'hover:bg-overlay/5'}`}
                >
                  <span className="flex h-11 w-11 items-center justify-center overflow-hidden rounded bg-overlay/5 text-ink-dim">{thumbnail(item)}</span>
                  {renaming === item.path && item.id && (prefab || item.kind === 'drawing') ? (
                    <input
                      autoFocus
                      defaultValue={item.name}
                      onBlur={(e) => {
                        setRenaming(null)
                        const name = e.target.value.trim()
                        if (!name || name === item.name || !item.id) return
                        if (prefab) onRenamePrefab(prefab.id, name)
                        else onRenameDrawing(item.id, name)
                      }}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') (e.target as HTMLInputElement).blur()
                        if (e.key === 'Escape') setRenaming(null)
                      }}
                      className="w-full rounded bg-overlay/10 px-1 text-[11px] text-ink outline-none"
                    />
                  ) : (
                    <span className={`line-clamp-2 w-full break-words text-[11px] leading-tight ${item.kind === 'file' ? 'text-ink-dim' : 'text-ink'}`}>{item.name}</span>
                  )}
                </div>
              )
            })}
          </div>
        </div>
      </div>

      {menu && (
        <div
          className="fixed z-50 w-44 rounded-lg border border-panel-border bg-panel p-1 shadow-2xl backdrop-blur-panel"
          style={{ left: menu.x, top: Math.min(menu.y, window.innerHeight - 160) }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          {[
            menu.item.kind === 'scene' && { label: 'Open Scene', run: () => open(menu.item) },
            menu.item.kind === 'drawing' && menu.item.id && { label: 'Edit in Paint', run: () => onEditDrawing(menu.item.id!) },
            menu.item.kind === 'drawing' && menu.item.id && { label: 'Place in Scene', run: () => onPlaceDrawing(menu.item.id!) },
            (menu.item.kind === 'prefab' || menu.item.kind === 'image' || menu.item.kind === 'component') && { label: 'Place in Scene', run: () => open(menu.item) },
            (menu.item.kind === 'prefab' || menu.item.kind === 'drawing') && { label: 'Rename', run: () => setRenaming(menu.item.path) },
            menu.item.kind === 'drawing' && menu.item.id && { label: 'Delete Drawing', run: () => onDeleteDrawing(menu.item.id!) },
            menu.item.kind === 'prefab' && menu.item.id && { label: 'Delete Prefab', run: () => onDeletePrefab(menu.item.id!) },
            projectFolder && { label: 'Show in Explorer', run: onShowInExplorer },
          ]
            .filter((entry): entry is { label: string; run: () => void } => !!entry)
            .map((entry) => (
              <button
                key={entry.label}
                onClick={() => {
                  setMenu(null)
                  entry.run()
                }}
                className="flex w-full rounded-md px-2 py-1.5 text-left text-xs text-ink hover:bg-overlay/8"
              >
                {entry.label}
              </button>
            ))}
        </div>
      )}
    </div>
  )
}
