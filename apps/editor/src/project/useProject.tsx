/**
 * Everything the editor does with project folders, as one hook — so
 * `App.tsx` only wires buttons to it. See `projectSession.ts` for the
 * on-disk format and the reasoning behind it.
 *
 * Behavior:
 *
 * - **Autosave.** While a project is open, any change is written to the
 *   folder ~1.5 s after editing pauses (never during Playground, whose
 *   runtime state isn't the authored scene). Closing the window flushes a
 *   pending save first, so closing never loses work.
 * - **Recovery for folder-less documents.** Unsaved work that isn't in a
 *   project yet is snapshotted to `<app data>/recovery/unsaved.teab` every
 *   30 s; the next launch offers to restore it. Closing with such unsaved
 *   work asks Save as Project / Don't Save / Cancel.
 * - **Reopen last project** on launch, like Unity.
 * - **Drop-in assets.** Images copied into the project's `Assets/` folder
 *   from outside are imported when the window regains focus.
 * - **Detach on replace.** Loading a demo or importing a `.tea` replaces the
 *   whole document; the project is then closed (not overwritten with the
 *   demo) — detected through `documentInstance`.
 */
import { useCallback, useEffect, useRef, useState, type ReactNode, type RefObject } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { DocumentSummary } from '../ipc/document'
import { listProjectFiles, pathExists, pickOpenPath, type ProjectEntry, pickSavePath, readBinaryFile, readFileRaw, readTextFile, recoveryDir, removeFile, revealInExplorer, writeBinaryFile, writeFileRaw, writeTextFile } from '../ipc/nativeFile'
import { documentInstance, documentRevision, importImage, loadTea, loadTeab, saveTea, saveTeab } from '../ipc/wasmEditor'
import type { ImportImageResult, OpaqueId } from '../ipc/document'
import type { WasmEditor } from '../wasm-pkg/an1mbyt3_wasm'
import type { FileMenuActions } from '../components/TopToolbar'
import {
  createProjectInteractive,
  forgetProject,
  importImageIntoProject,
  importNewAssets,
  joinPath,
  openProject,
  openProjectInteractive,
  recentProjects,
  rememberProject,
  saveProject,
  type ProjectSession,
} from './projectSession'

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

/** Quiet period after the last edit before autosave writes. */
const AUTOSAVE_DELAY_MS = 1500
/** How often unsaved folder-less work is snapshotted for recovery. */
const RECOVERY_INTERVAL_MS = 30_000
const RECOVERY_FILE = 'unsaved.teab'

export interface UseProjectOptions {
  editorRef: RefObject<WasmEditor | null>
  /** `true` once the editor exists. */
  ready: boolean
  /** `true` while in Playground (no autosave — runtime state isn't authored). */
  playing: boolean
  /** The whole document was replaced (open / restore / import) — reset
   * selection, scene, etc. */
  onReplaced: (doc: DocumentSummary) => void
  /** The document changed in place (drop-in import). */
  onChanged: (doc: DocumentSummary) => void
}

export interface UseProjectResult {
  fileMenu: FileMenuActions
  /** The open project's folder, or `null`. */
  projectFolder: string | null
  /** The project folder's files on disk (empty without a project). */
  projectFiles: ProjectEntry[]
  /** Rescans the project folder (and imports new drop-ins). */
  refreshFiles: () => void
  /** Imports an image: with a project open the file is copied into
   * `Assets/Images/` first and the asset linked to it. `artboard` also
   * places it on the canvas. */
  importImage: (fileName: string, bytes: Uint8Array, artboard: OpaqueId | null) => Promise<ImportImageResult>
  /** Ctrl+S / Save button. */
  save: () => void
  /** Prompts / banners to render on top of the editor. */
  overlay: ReactNode
}

export function useProject({ editorRef, ready, playing, onReplaced, onChanged }: UseProjectOptions): UseProjectResult {
  const sessionRef = useRef<ProjectSession | null>(null)
  const [projectName, setProjectName] = useState<string | null>(null)
  const [projectFolder, setProjectFolder] = useState<string | null>(null)
  const [projectFiles, setProjectFiles] = useState<ProjectEntry[]>([])
  /** Re-lists the open project's folder for the Resources view. */
  const listFiles = useCallback(async () => {
    const session = sessionRef.current
    if (!session) {
      setProjectFiles([])
      return
    }
    try {
      setProjectFiles(await listProjectFiles(session.folder))
    } catch (err) {
      console.warn('listing the project folder failed', err)
    }
  }, [])
  const [dirty, setDirty] = useState(false)
  const [recent, setRecent] = useState<string[]>(() => recentProjects())
  const [notice, setNotice] = useState<string | null>(null)
  const [recoveryOffer, setRecoveryOffer] = useState<string | null>(null)
  const [closePrompt, setClosePrompt] = useState(false)

  /** Revision at the last save/open — dirty = current revision differs. */
  const savedRevisionRef = useRef(0)
  /** The document instance the session (or baseline) belongs to. */
  const instanceRef = useRef(-1)
  const lastRevisionRef = useRef(0)
  const lastChangeAtRef = useRef(0)
  const lastRecoveryAtRef = useRef(0)
  const savingRef = useRef<Promise<void> | null>(null)
  const playingRef = useRef(playing)
  playingRef.current = playing
  const onReplacedRef = useRef(onReplaced)
  onReplacedRef.current = onReplaced
  const onChangedRef = useRef(onChanged)
  onChangedRef.current = onChanged

  const flash = useCallback((message: string) => {
    setNotice(message)
    window.setTimeout(() => setNotice((current) => (current === message ? null : current)), 3500)
  }, [])

  /** Marks the current document state as saved. */
  const markClean = useCallback((editor: WasmEditor, revision = documentRevision(editor)) => {
    savedRevisionRef.current = revision
    instanceRef.current = documentInstance(editor)
    setDirty(documentRevision(editor) !== revision)
  }, [])

  const clearRecovery = useCallback(async () => {
    if (!isTauri) return
    try {
      await removeFile(joinPath(await recoveryDir(), RECOVERY_FILE))
    } catch {
      // nothing to clear
    }
  }, [])

  const attach = useCallback(
    (session: ProjectSession, editor: WasmEditor) => {
      sessionRef.current = session
      setProjectName(session.name)
      setProjectFolder(session.folder)
      rememberProject(session.folder)
      setRecent(recentProjects())
      markClean(editor)
      void clearRecovery()
    },
    [markClean, clearRecovery],
  )

  /** Writes the open project (serialized: one save at a time). */
  const saveNow = useCallback(async (): Promise<boolean> => {
    const editor = editorRef.current
    const session = sessionRef.current
    if (!editor || !session) return false
    if (savingRef.current) await savingRef.current
    const revision = documentRevision(editor)
    const run = saveProject(editor, session)
    savingRef.current = run
    try {
      await run
      markClean(editor, revision)
      void listFiles()
      return true
    } catch (err) {
      console.error('project save failed', err)
      flash(`Save failed: ${err instanceof Error ? err.message : String(err)}`)
      return false
    } finally {
      savingRef.current = null
    }
  }, [editorRef, markClean, flash])

  const openFolder = useCallback(
    async (folder?: string) => {
      const editor = editorRef.current
      if (!editor) return
      try {
        const result = folder ? await openProject(editor, folder) : await openProjectInteractive(editor)
        if (!result) return
        attach(result.session, editor)
        onReplacedRef.current(result.doc)
        const imported = await importNewAssets(editor, result.session)
        if (imported) onChangedRef.current(imported)
        void listFiles()
      } catch (err) {
        console.error('open project failed', err)
        if (folder) {
          forgetProject(folder)
          setRecent(recentProjects())
        }
        flash(`Couldn't open project: ${err instanceof Error ? err.message : String(err)}`)
      }
    },
    [editorRef, attach, flash],
  )

  /** New Project — also "Save as Project" for a folder-less document: the
   * current document becomes the new project's contents. */
  const newProject = useCallback(async (): Promise<boolean> => {
    const editor = editorRef.current
    if (!editor) return false
    try {
      const result = await createProjectInteractive(editor)
      if (!result) return false
      if ('existing' in result) {
        flash('That folder already has a project — opening it instead.')
        await openFolder(result.existing)
        return false
      }
      attach(result.session, editor)
      void listFiles()
      flash(`Project created in ${result.session.folder}`)
      return true
    } catch (err) {
      console.error('new project failed', err)
      flash(`Couldn't create project: ${err instanceof Error ? err.message : String(err)}`)
      return false
    }
  }, [editorRef, attach, openFolder, flash])

  const save = useCallback(() => {
    if (playingRef.current) {
      flash('Stop Playground to save — play-time changes aren’t part of the project.')
      return
    }
    void (sessionRef.current ? saveNow().then((ok) => ok && flash('Saved')) : newProject())
  }, [saveNow, newProject, flash])

  // ---- Startup: reopen the last project, or offer recovery ----
  const startedRef = useRef(false)
  useEffect(() => {
    if (!ready || startedRef.current || !isTauri) return
    startedRef.current = true
    const editor = editorRef.current
    if (editor) markClean(editor)
    void (async () => {
      const last = recentProjects()[0]
      if (last && (await pathExists(last))) {
        await openFolder(last)
        return
      }
      try {
        const path = joinPath(await recoveryDir(), RECOVERY_FILE)
        if (await pathExists(path)) setRecoveryOffer(path)
      } catch {
        // no recovery dir — nothing to offer
      }
    })()
  }, [ready, editorRef, markClean, openFolder])

  // ---- Dirty tracking, autosave, recovery snapshots ----
  useEffect(() => {
    if (!ready) return
    const timer = window.setInterval(() => {
      const editor = editorRef.current
      if (!editor) return
      const instance = documentInstance(editor)
      const revision = documentRevision(editor)
      if (instance !== instanceRef.current) {
        // Replaced by something other than our own open (demo, import):
        // the project no longer describes this document.
        if (sessionRef.current) {
          sessionRef.current = null
          setProjectName(null)
          setProjectFolder(null)
          setProjectFiles([])
        }
        instanceRef.current = instance
        savedRevisionRef.current = revision
      }
      if (revision !== lastRevisionRef.current) {
        lastRevisionRef.current = revision
        lastChangeAtRef.current = performance.now()
      }
      const isDirty = revision !== savedRevisionRef.current
      setDirty(isDirty)
      if (!isDirty || playingRef.current || savingRef.current) return
      const now = performance.now()
      if (sessionRef.current) {
        if (now - lastChangeAtRef.current > AUTOSAVE_DELAY_MS) void saveNow()
      } else if (isTauri && now - lastRecoveryAtRef.current > RECOVERY_INTERVAL_MS) {
        lastRecoveryAtRef.current = now
        void (async () => {
          try {
            await writeFileRaw(joinPath(await recoveryDir(), RECOVERY_FILE), saveTeab(editor))
          } catch (err) {
            console.warn('recovery snapshot failed', err)
          }
        })()
      }
    }, 500)
    return () => window.clearInterval(timer)
  }, [ready, editorRef, saveNow])

  // ---- Drop-in asset import on focus ----
  useEffect(() => {
    if (!ready) return
    const onFocus = () => {
      const editor = editorRef.current
      const session = sessionRef.current
      if (!editor || !session || playingRef.current) return
      void importNewAssets(editor, session)
        .then((doc) => {
          if (doc) {
            onChangedRef.current(doc)
            flash('Imported new files from Assets/')
          }
          void listFiles()
        })
        .catch((err) => console.warn('asset scan failed', err))
    }
    window.addEventListener('focus', onFocus)
    return () => window.removeEventListener('focus', onFocus)
  }, [ready, editorRef, flash])

  // ---- Closing the window ----
  const allowCloseRef = useRef(false)
  useEffect(() => {
    if (!isTauri) return
    let unlisten: (() => void) | null = null
    void getCurrentWindow()
      .onCloseRequested(async (event) => {
        if (allowCloseRef.current) return
        const editor = editorRef.current
        if (!editor) return
        const isDirty = documentRevision(editor) !== savedRevisionRef.current
        if (!isDirty) return
        if (sessionRef.current && !playingRef.current) {
          // Projects autosave: just finish the pending save, then close.
          event.preventDefault()
          await saveNow()
          allowCloseRef.current = true
          await getCurrentWindow().destroy()
          return
        }
        if (!sessionRef.current) {
          event.preventDefault()
          setClosePrompt(true)
        }
      })
      .then((fn) => {
        unlisten = fn
      })
    return () => unlisten?.()
  }, [editorRef, saveNow])

  const closeNow = useCallback(async () => {
    allowCloseRef.current = true
    await getCurrentWindow().destroy()
  }, [])

  // ---- Legacy single-file documents ----
  const importDocument = useCallback(() => {
    const editor = editorRef.current
    if (!editor) return
    void (async () => {
      const path = await pickOpenPath()
      if (!path) return
      const doc = path.endsWith('.teab') ? await loadTeab(editor, await readBinaryFile(path)) : await loadTea(editor, await readTextFile(path))
      // A different document now: not the open project any more.
      sessionRef.current = null
      setProjectName(null)
      setProjectFolder(null)
      setProjectFiles([])
      markClean(editor)
      onReplacedRef.current(doc)
    })().catch((err) => flash(`Import failed: ${err instanceof Error ? err.message : String(err)}`))
  }, [editorRef, markClean, flash])

  const exportDocument = useCallback(() => {
    const editor = editorRef.current
    if (!editor) return
    void (async () => {
      const path = await pickSavePath(`${sessionRef.current?.name ?? 'untitled'}.tea`)
      if (!path) return
      if (path.endsWith('.teab')) await writeBinaryFile(path, saveTeab(editor))
      else await writeTextFile(path, saveTea(editor))
      flash(`Exported ${path}`)
    })().catch((err) => flash(`Export failed: ${err instanceof Error ? err.message : String(err)}`))
  }, [editorRef, flash])

  const restoreRecovery = useCallback(async () => {
    const editor = editorRef.current
    const path = recoveryOffer
    setRecoveryOffer(null)
    if (!editor || !path) return
    try {
      const doc = await loadTeab(editor, await readFileRaw(path))
      instanceRef.current = documentInstance(editor)
      savedRevisionRef.current = -1 // restored work is unsaved
      onReplacedRef.current(doc)
      flash('Restored unsaved work — save it as a project to keep it.')
    } catch (err) {
      flash(`Couldn't restore: ${err instanceof Error ? err.message : String(err)}`)
    }
  }, [editorRef, recoveryOffer, flash])

  const fileMenu: FileMenuActions = {
    projectName,
    dirty,
    recent,
    onNewProject: () => void newProject(),
    onOpenProject: () => void openFolder(),
    onOpenRecent: (folder) => void openFolder(folder),
    onShowInExplorer: () => {
      const session = sessionRef.current
      if (session) void revealInExplorer(session.folder)
    },
    onImportDocument: importDocument,
    onExportDocument: exportDocument,
  }

  const overlay = (
    <>
      {notice && (
        <div className="pointer-events-none fixed bottom-6 left-1/2 z-[60] -translate-x-1/2 rounded-full border border-panel-border bg-panel px-4 py-1.5 text-xs text-ink shadow-xl backdrop-blur-panel">
          {notice}
        </div>
      )}
      {recoveryOffer && (
        <div className="fixed left-1/2 top-24 z-[60] flex -translate-x-1/2 items-center gap-3 rounded-lg border border-panel-border bg-panel px-4 py-2.5 text-xs text-ink shadow-2xl backdrop-blur-panel">
          <span>Unsaved work from your last session was recovered.</span>
          <button onClick={() => void restoreRecovery()} className="rounded-md bg-accent px-2.5 py-1 font-medium text-white hover:opacity-90">
            Restore
          </button>
          <button
            onClick={() => {
              setRecoveryOffer(null)
              void clearRecovery()
            }}
            className="rounded-md px-2.5 py-1 text-ink-dim hover:bg-overlay/8 hover:text-ink"
          >
            Discard
          </button>
        </div>
      )}
      {closePrompt && (
        <div className="fixed inset-0 z-[70] flex items-center justify-center bg-black/40">
          <div className="w-[22rem] rounded-xl border border-panel-border bg-panel p-5 text-ink shadow-2xl">
            <h2 className="text-sm font-semibold">Save your work before closing?</h2>
            <p className="mt-2 text-xs leading-relaxed text-ink-dim">
              This document isn’t in a project yet. Save it as a project folder to keep it, or close without saving.
            </p>
            <div className="mt-4 flex justify-end gap-2">
              <button onClick={() => setClosePrompt(false)} className="rounded-md px-3 py-1.5 text-xs text-ink-dim hover:bg-overlay/8 hover:text-ink">
                Cancel
              </button>
              <button
                onClick={() => {
                  setClosePrompt(false)
                  void clearRecovery().then(closeNow)
                }}
                className="rounded-md px-3 py-1.5 text-xs text-ink-dim hover:bg-overlay/8 hover:text-ink"
              >
                Don’t Save
              </button>
              <button
                onClick={() => {
                  setClosePrompt(false)
                  void newProject().then((ok) => {
                    if (ok) void closeNow()
                  })
                }}
                className="rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-white hover:opacity-90"
              >
                Save as Project…
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  )

  const refreshFiles = () => {
    const editor = editorRef.current
    const session = sessionRef.current
    if (!editor || !session) return
    void importNewAssets(editor, session)
      .then((doc) => {
        if (doc) onChangedRef.current(doc)
      })
      .finally(() => void listFiles())
  }

  const importImageFile = async (fileName: string, bytes: Uint8Array, artboard: OpaqueId | null): Promise<ImportImageResult> => {
    const editor = editorRef.current
    if (!editor) throw new Error('editor not ready')
    const session = sessionRef.current
    if (!session) {
      if (!artboard) throw new Error('no scene to place the image in')
      return importImage(editor, artboard, fileName, bytes)
    }
    const result = await importImageIntoProject(editor, session, fileName, bytes, artboard)
    void listFiles()
    return result
  }

  return { fileMenu, projectFolder, projectFiles, refreshFiles, importImage: importImageFile, save, overlay }
}
