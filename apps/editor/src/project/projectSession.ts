/**
 * Project folders — the editor side of `an1mbyt3_format::project`.
 *
 * A project is a folder on disk, Unity/Rive-style:
 *
 * ```text
 * MyGame/
 *   MyGame.an1proj            manifest (settings + small stuff)
 *   Assets/Scenes/*.scene     one file per scene
 *   Assets/Prefabs/*.prefab   one file per prefab
 *   Assets/Images/*.png …     the image files themselves
 * ```
 *
 * The document still lives in memory while you edit (the GPU needs every
 * texture resident anyway), but the folder is the source of truth: saving
 * writes only what changed, autosave keeps the folder current, and opening
 * rebuilds everything from the files — so closing the editor, or a crash,
 * loses at most the last couple of seconds.
 *
 * Files dropped into `Assets/` from outside (an image saved from another
 * tool) are imported automatically the next time the editor scans the
 * folder (on open, and whenever the window regains focus) — Unity's asset
 * import.
 */
import {
  ensureDir,
  listProjectFiles,
  pickProjectFolder,
  readFileRaw,
  readTextFile,
  removeFile,
  writeFileRaw,
  writeTextFile,
  type ProjectEntry,
} from '../ipc/nativeFile'
import type { DocumentSummary } from '../ipc/document'
import { importImage, importImageAsset, loadProjectFiles, projectAssetPaths, saveProjectFiles, setImageFile, takeProjectFileBytes, importAudio, setAudioFile } from '../ipc/wasmEditor'
import type { ImportImageResult, OpaqueId } from '../ipc/document'
import type { WasmEditor } from '../wasm-pkg/an1mbyt3_wasm'

/** The manifest's extension (`an1mbyt3_format::PROJECT_EXTENSION`). */
export const PROJECT_EXTENSION = 'an1proj'

/** The standard folders every project gets (created on New Project). */
const ASSET_FOLDERS = ['Assets', 'Assets/Scenes', 'Assets/Prefabs', 'Assets/Images']

/** Image files the drop-in scan imports. */
const IMAGE_FILE = /\.(png|jpe?g|gif|webp)$/i

/** One open project. Mutable bookkeeping lives here, not in React state:
 * none of it should re-render anything. */
export interface ProjectSession {
  /** Absolute folder path, `/`-separated. */
  folder: string
  /** Project name — also the manifest's file name. */
  name: string
  /** Asset files the last save/open referenced (relative paths). */
  files: Set<string>
  /** Files found in `Assets/` that were imported by the drop-in scan — so a
   * source file the save doesn't reproduce verbatim (a `.gif`, saved back
   * as its decoded frames) isn't imported again on every scan. Persisted in
   * the manifest's `importedSources`. */
  importedSources: Set<string>
  /** Last-known on-disk size per asset file — a save skips rewriting an
   * image whose bytes are the same length as what's already there. */
  sizes: Map<string, number>
}

export function joinPath(folder: string, relative: string): string {
  return `${folder.replace(/[\\/]+$/, '')}/${relative}`
}

function normalizeFolder(path: string): string {
  return path.replace(/\\/g, '/').replace(/\/+$/, '')
}

/** The folder name, as the default project name. */
function baseName(path: string): string {
  const parts = normalizeFolder(path).split('/')
  return parts[parts.length - 1] || 'Untitled'
}

function sizesFrom(entries: ProjectEntry[]): Map<string, number> {
  return new Map(entries.map((e) => [e.path, e.size]))
}

/**
 * New Project: pick a folder, create the standard `Assets/` layout and
 * write the current document into it. If the picked folder already holds a
 * project, nothing is overwritten — the caller should open it instead
 * (returned as `existing`).
 */
export async function createProjectInteractive(editor: WasmEditor): Promise<{ session: ProjectSession } | { existing: string } | null> {
  const picked = await pickProjectFolder()
  if (!picked) return null
  const folder = normalizeFolder(picked)
  const entries = await listProjectFiles(folder)
  const manifest = entries.find((e) => !e.path.includes('/') && e.path.endsWith(`.${PROJECT_EXTENSION}`))
  if (manifest) return { existing: folder }
  for (const dir of ASSET_FOLDERS) await ensureDir(joinPath(folder, dir))
  const session: ProjectSession = { folder, name: baseName(folder), files: new Set(), importedSources: new Set(), sizes: sizesFrom(entries) }
  await saveProject(editor, session)
  return { session }
}

/** Open Project: pick a folder (or pass one) and load it. */
export async function openProjectInteractive(editor: WasmEditor, folderArg?: string): Promise<{ session: ProjectSession; doc: DocumentSummary } | null> {
  const picked = folderArg ?? (await pickProjectFolder())
  if (!picked) return null
  return openProject(editor, normalizeFolder(picked))
}

export async function openProject(editor: WasmEditor, folder: string): Promise<{ session: ProjectSession; doc: DocumentSummary }> {
  const entries = await listProjectFiles(folder)
  const manifestEntry = entries.find((e) => !e.path.includes('/') && e.path.endsWith(`.${PROJECT_EXTENSION}`))
  if (!manifestEntry) throw new Error(`No .${PROJECT_EXTENSION} project file in ${folder}`)
  const manifest = await readTextFile(joinPath(folder, manifestEntry.path))
  const paths = projectAssetPaths(manifest)
  const files = await Promise.all(paths.map(async (path) => ({ path, bytes: await readFileRaw(joinPath(folder, path)) })))
  const doc = await loadProjectFiles(editor, manifest, files)
  let importedSources: string[] = []
  let name = manifestEntry.path.slice(0, -(PROJECT_EXTENSION.length + 1))
  try {
    const parsed = JSON.parse(manifest) as { name?: string; importedSources?: string[] }
    importedSources = parsed.importedSources ?? []
    if (parsed.name) name = parsed.name
  } catch {
    // `load_project` already validated it; a parse failure here can't happen.
  }
  const session: ProjectSession = { folder, name, files: new Set(paths), importedSources: new Set(importedSources), sizes: sizesFrom(entries) }
  return { session, doc }
}

/**
 * Writes the document into the project folder: every scene/prefab file,
 * images whose size changed (or are new), then the manifest last (so an
 * interrupted save leaves the old manifest pointing at a complete set).
 * Asset files the previous save wrote but this one no longer references (a
 * renamed or deleted scene) are removed — never files the editor didn't
 * write itself.
 */
export async function saveProject(editor: WasmEditor, session: ProjectSession): Promise<void> {
  const listing = saveProjectFiles(editor, session.name)
  const manifestFile = listing[listing.length - 1]
  const written = new Set<string>()
  const dirs = new Set<string>()
  for (const file of listing) {
    const slash = file.path.lastIndexOf('/')
    if (slash > 0) dirs.add(file.path.slice(0, slash))
  }
  for (const dir of dirs) await ensureDir(joinPath(session.folder, dir))

  for (const file of listing.slice(0, -1)) {
    written.add(file.path)
    const target = joinPath(session.folder, file.path)
    if (file.text !== null) {
      await writeTextFile(target, file.text)
    } else if (file.bytes !== null) {
      // A linked image's file on disk *is* the source — never overwrite it
      // (only restore it if someone deleted it).
      if (file.linked && session.sizes.has(file.path)) continue
      const bytes = takeProjectFileBytes(editor, file.bytes)
      if (session.sizes.get(file.path) === bytes.length) continue
      await writeFileRaw(target, bytes)
      session.sizes.set(file.path, bytes.length)
    }
  }

  // The manifest, plus the editor-only `importedSources` bookkeeping.
  const manifest = JSON.parse(manifestFile.text ?? '{}') as Record<string, unknown>
  manifest.importedSources = [...session.importedSources]
  await writeTextFile(joinPath(session.folder, manifestFile.path), JSON.stringify(manifest, null, 2))

  for (const stale of session.files) {
    if (!written.has(stale) && !session.importedSources.has(stale)) {
      await removeFile(joinPath(session.folder, stale)).catch(() => {})
      session.sizes.delete(stale)
    }
  }
  session.files = written
}

/**
 * Unity-style drop-in import: imports image files found under `Assets/`
 * that the project doesn't know yet. Returns the updated document if
 * anything was imported, else `null`.
 */
export async function importNewAssets(editor: WasmEditor, session: ProjectSession): Promise<DocumentSummary | null> {
  const entries = await listProjectFiles(session.folder)
  let doc: DocumentSummary | null = null
  for (const entry of entries) {
    if (!entry.path.startsWith('Assets/')) continue
    if (session.files.has(entry.path) || session.importedSources.has(entry.path)) continue
    if (AUDIO_FILE.test(entry.path)) {
      // A sound dropped into the project folder: import + link it.
      try {
        const bytes = await readFileRaw(joinPath(session.folder, entry.path))
        const result = importAudio(editor, entry.path.slice(entry.path.lastIndexOf('/') + 1), bytes)
        setAudioFile(editor, result.audio, entry.path)
        doc = result.document
      } catch (err) {
        console.warn(`couldn't import ${entry.path}`, err)
      }
      session.importedSources.add(entry.path)
      continue
    }
    if (!IMAGE_FILE.test(entry.path)) continue
    try {
      const bytes = await readFileRaw(joinPath(session.folder, entry.path))
      const fileName = entry.path.slice(entry.path.lastIndexOf('/') + 1)
      const result = await importImageAsset(editor, fileName, bytes)
      doc = result.document
      session.importedSources.add(entry.path)
      // A still image is linked to the file it came from (so saves don't
      // write a second copy); a GIF's frames are saved as their own files.
      linkImported(editor, result, entry.path)
    } catch (err) {
      console.warn(`couldn't import ${entry.path}`, err)
      session.importedSources.add(entry.path) // don't retry a broken file forever
    }
  }
  for (const entry of entries) session.sizes.set(entry.path, entry.size)
  return doc
}

/** Links a just-imported still image to its file in the project. */
function linkImported(editor: WasmEditor, result: ImportImageResult, path: string): void {
  if (!result.newSpriteGroup && result.newImages.length === 1) setImageFile(editor, result.newImages[0] as OpaqueId, path)
}

/**
 * Imports an image file *into the project*: the original file is copied,
 * bytes and all, into `Assets/Images/` (a numbered name if that file name is
 * taken by different content), then imported and linked to that copy — the
 * project folder holds the real file, Unity-style. `artboard`: also place it
 * on the canvas as a sprite (a GIF only adds its frames).
 */
export async function importImageIntoProject(editor: WasmEditor, session: ProjectSession, fileName: string, bytes: Uint8Array, artboard: OpaqueId | null): Promise<ImportImageResult> {
  const dot = fileName.lastIndexOf('.')
  const stem = dot > 0 ? fileName.slice(0, dot) : fileName
  const ext = dot > 0 ? fileName.slice(dot) : ''
  let path = `Assets/Images/${fileName}`
  for (let n = 2; session.sizes.has(path) && session.sizes.get(path) !== bytes.length; n++) path = `Assets/Images/${stem} ${n}${ext}`
  if (!session.sizes.has(path)) {
    await writeFileRaw(joinPath(session.folder, path), bytes)
    session.sizes.set(path, bytes.length)
  }
  session.importedSources.add(path)
  const result = artboard ? await importImage(editor, artboard, fileName, bytes) : await importImageAsset(editor, fileName, bytes)
  linkImported(editor, result, path)
  return result
}

/** A sound file's path under `Assets/` (drop-in import). */
const AUDIO_FILE = /\.(mp3|wav|ogg|m4a|aac|flac|webm)$/i

/**
 * Imports a sound *into the project*: copied into `Assets/Sounds/` (a
 * numbered name if that file name is taken by different content), then
 * added to the document and linked to that copy — like images.
 */
export async function importAudioIntoProject(editor: WasmEditor, session: ProjectSession, fileName: string, bytes: Uint8Array): Promise<{ document: DocumentSummary; audio: OpaqueId }> {
  const dot = fileName.lastIndexOf('.')
  const stem = dot > 0 ? fileName.slice(0, dot) : fileName
  const ext = dot > 0 ? fileName.slice(dot) : ''
  let path = `Assets/Sounds/${fileName}`
  for (let n = 2; session.sizes.has(path) && session.sizes.get(path) !== bytes.length; n++) path = `Assets/Sounds/${stem} ${n}${ext}`
  if (!session.sizes.has(path)) {
    await writeFileRaw(joinPath(session.folder, path), bytes)
    session.sizes.set(path, bytes.length)
  }
  session.importedSources.add(path)
  const result = importAudio(editor, fileName, bytes)
  setAudioFile(editor, result.audio, path)
  return result
}

// ---- Recent projects (per machine, localStorage) ----

const RECENT_KEY = 'an1mbyt3.recentProjects'

export function recentProjects(): string[] {
  try {
    const raw = localStorage.getItem(RECENT_KEY)
    const list = raw ? (JSON.parse(raw) as unknown) : []
    return Array.isArray(list) ? list.filter((p): p is string => typeof p === 'string') : []
  } catch {
    return []
  }
}

export function rememberProject(folder: string): void {
  try {
    const list = [folder, ...recentProjects().filter((p) => p !== folder)].slice(0, 8)
    localStorage.setItem(RECENT_KEY, JSON.stringify(list))
  } catch {
    // Storage unavailable — recents are a convenience only.
  }
}

export function forgetProject(folder: string): void {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(recentProjects().filter((p) => p !== folder)))
  } catch {
    // ignore
  }
}
