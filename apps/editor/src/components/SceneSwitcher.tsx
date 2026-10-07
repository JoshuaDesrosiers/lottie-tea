import { useEffect, useRef, useState } from 'react'
import type { ArtboardSummary, OpaqueId } from '../ipc/document'
import { CheckIcon, ChevronDownIcon, EditIcon, PlusIcon, TrashIcon } from './icons'

function idsEqual(a: OpaqueId, b: OpaqueId): boolean {
  return a.idx === b.idx && a.version === b.version
}

/**
 * The top toolbar's scene picker — every artboard in the document is a
 * "Scene" (a separate level/screen/area with its own node tree, animations
 * and state machines), and this is where an author creates, renames,
 * deletes and switches between them. A custom popover rather than a native
 * `<select>`/`window.prompt` for the same "this app never uses OS dropdown
 * chrome" reason `InspectorPanel`'s `Dropdown` gives; naming a new scene or
 * renaming one happens in an inline text field inside the popover itself.
 *
 * Deleting is disabled on the last remaining scene — `WasmEditor::delete_artboard`
 * refuses it too, since every editor code path assumes at least one exists.
 */
export function SceneSwitcher({
  scenes,
  activeScene,
  onSelect,
  onAdd,
  onRename,
  onDelete,
}: {
  scenes: ArtboardSummary[]
  activeScene: ArtboardSummary | undefined
  onSelect: (id: OpaqueId) => void
  onAdd: (name: string) => void
  onRename: (id: OpaqueId, name: string) => void
  onDelete: (id: OpaqueId) => void
}) {
  const [open, setOpen] = useState(false)
  // `null` = not editing; `'new'` = the "+ New Scene" row's own field;
  // an id = renaming that scene in place.
  const [editing, setEditing] = useState<OpaqueId | 'new' | null>(null)
  const [draft, setDraft] = useState('')
  const rootRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const handlePointerDown = (event: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) {
        setOpen(false)
        setEditing(null)
      }
    }
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && editing === null) setOpen(false)
    }
    window.addEventListener('pointerdown', handlePointerDown)
    window.addEventListener('keydown', handleKeyDown)
    return () => {
      window.removeEventListener('pointerdown', handlePointerDown)
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [open, editing])

  const startNew = () => {
    setDraft(`Scene ${scenes.length + 1}`)
    setEditing('new')
  }

  const startRename = (scene: ArtboardSummary) => {
    setDraft(scene.name)
    setEditing(scene.id)
  }

  const commit = () => {
    const name = draft.trim()
    if (name && editing !== null) {
      if (editing === 'new') {
        onAdd(name)
        setOpen(false)
      } else {
        onRename(editing, name)
      }
    }
    setEditing(null)
  }

  const textField = (
    <input
      autoFocus
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onKeyDown={(e) => {
        e.stopPropagation()
        if (e.key === 'Enter') commit()
        if (e.key === 'Escape') setEditing(null)
      }}
      className="min-w-0 flex-1 rounded bg-overlay/10 px-1.5 py-0.5 text-[11px] text-ink outline-none focus:ring-1 focus:ring-accent"
    />
  )

  return (
    <div ref={rootRef} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        title="Scenes"
        className="flex items-center gap-1 rounded-md px-2 py-1 text-xs text-ink hover:bg-overlay/5"
      >
        <span className="text-ink-dim">Scene:</span>
        <span className="max-w-[120px] truncate">{activeScene?.name ?? '—'}</span>
        <ChevronDownIcon />
      </button>
      {open && (
        <div className="absolute left-0 top-full z-40 mt-1 w-60 rounded-lg border border-panel-border bg-panel p-1 shadow-2xl backdrop-blur-panel">
          <div className="px-2 pb-1 pt-0.5 text-[10px] font-medium uppercase tracking-wide text-ink-dim">Scenes</div>
          {scenes.map((scene) => {
            const isActive = activeScene !== undefined && idsEqual(scene.id, activeScene.id)
            const isRenaming = editing !== null && editing !== 'new' && idsEqual(editing, scene.id)
            return (
              <div
                key={`${scene.id.idx}:${scene.id.version}`}
                className={`group flex items-center gap-1 rounded-md px-2 py-1 text-xs ${isActive ? 'bg-accent/15 text-accent' : 'text-ink hover:bg-overlay/5'}`}
              >
                {isRenaming ? (
                  <>
                    {textField}
                    <button onClick={commit} title="Save name" className="rounded p-0.5 hover:bg-overlay/10">
                      <CheckIcon />
                    </button>
                  </>
                ) : (
                  <>
                    <button
                      onClick={() => {
                        onSelect(scene.id)
                        setOpen(false)
                      }}
                      onDoubleClick={() => startRename(scene)}
                      className="min-w-0 flex-1 truncate text-left"
                    >
                      {scene.name}
                    </button>
                    <span className="shrink-0 text-[10px] tabular-nums text-ink-dim">
                      {scene.width}×{scene.height}
                    </span>
                    <button onClick={() => startRename(scene)} title="Rename scene" className="rounded p-0.5 text-ink-dim opacity-0 hover:bg-overlay/10 hover:text-ink group-hover:opacity-100">
                      <EditIcon />
                    </button>
                    <button
                      onClick={() => onDelete(scene.id)}
                      disabled={scenes.length <= 1}
                      title={scenes.length <= 1 ? "Can't delete the only scene" : 'Delete scene'}
                      className="rounded p-0.5 text-ink-dim opacity-0 hover:bg-red-500/10 hover:text-red-400 group-hover:opacity-100 disabled:hidden"
                    >
                      <TrashIcon />
                    </button>
                  </>
                )}
              </div>
            )
          })}
          <div className="my-1 h-px bg-overlay/10" />
          {editing === 'new' ? (
            <div className="flex items-center gap-1 px-2 py-1">
              {textField}
              <button onClick={commit} className="shrink-0 rounded bg-accent/80 px-2 py-0.5 text-[11px] text-white hover:bg-accent">
                Create
              </button>
            </div>
          ) : (
            <button onClick={startNew} className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs text-ink-dim hover:bg-overlay/5 hover:text-ink">
              <PlusIcon />
              New Scene
            </button>
          )}
        </div>
      )}
    </div>
  )
}
