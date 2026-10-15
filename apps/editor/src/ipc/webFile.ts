/**
 * The editor's file layer **in a plain browser** — what `nativeFile.ts`
 * uses when it isn't running inside the Tauri desktop app.
 *
 * Every caller keeps working with string paths, exactly as on desktop; in
 * the browser those strings name things the page holds on to:
 *
 * - `web-dir:<id>/<FolderName>/...` — a real folder on disk the user
 *   picked (Chrome / Edge File System Access). Its handle is kept in
 *   IndexedDB under `<id>`, so a recent project can be reopened later
 *   (the browser asks for permission again on first touch).
 * - `web-opfs:<path>` — the browser's private storage (Origin Private File
 *   System): recovery snapshots, and project folders on browsers without
 *   folder access (Firefox / Safari).
 * - `web-pick:<id>/<name>` — a file the user chose to open (in memory).
 * - `web-save:<id>/<name>` — a "Save As" target: a file handle where the
 *   browser has a save dialog, otherwise the file is downloaded on write.
 */
import type { ProjectEntry } from './nativeFile'

type DirHandle = FileSystemDirectoryHandle
type FileHandle = FileSystemFileHandle

interface PickerWindow {
  showDirectoryPicker?: (options?: { mode?: 'read' | 'readwrite'; id?: string }) => Promise<DirHandle>
  showSaveFilePicker?: (options?: { suggestedName?: string; types?: { description: string; accept: Record<string, string[]> }[] }) => Promise<FileHandle>
}
const pickerWindow = window as unknown as PickerWindow

/** Whether this browser can open real project folders. */
export const canPickFolders = typeof pickerWindow.showDirectoryPicker === 'function'

const newId = () => Math.random().toString(36).slice(2, 10)
const picked = new Map<string, File>()
const saves = new Map<string, FileHandle | null>()
const rootHandles = new Map<string, DirHandle>()

// ---- Folder handles, remembered across reloads (IndexedDB) ----

function idb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open('an1mbyt3-folders', 1)
    req.onupgradeneeded = () => req.result.createObjectStore('handles')
    req.onsuccess = () => resolve(req.result)
    req.onerror = () => reject(req.error)
  })
}

async function storeHandle(id: string, handle: DirHandle): Promise<void> {
  try {
    const db = await idb()
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction('handles', 'readwrite')
      tx.objectStore('handles').put(handle, id)
      tx.oncomplete = () => resolve()
      tx.onerror = () => reject(tx.error)
    })
  } catch {
    // Not remembered; the project still works this session.
  }
}

async function loadHandle(id: string): Promise<DirHandle | null> {
  try {
    const db = await idb()
    return await new Promise((resolve) => {
      const req = db.transaction('handles').objectStore('handles').get(id)
      req.onsuccess = () => resolve((req.result as DirHandle) ?? null)
      req.onerror = () => resolve(null)
    })
  } catch {
    return null
  }
}

/** Makes sure we may read and write `handle` (asks the user if needed). */
async function ensurePermission(handle: DirHandle): Promise<void> {
  const h = handle as DirHandle & {
    queryPermission?: (o: { mode: string }) => Promise<PermissionState>
    requestPermission?: (o: { mode: string }) => Promise<PermissionState>
  }
  if (!h.queryPermission) return
  if ((await h.queryPermission({ mode: 'readwrite' })) === 'granted') return
  if (h.requestPermission && (await h.requestPermission({ mode: 'readwrite' })) === 'granted') return
  throw new Error(`No permission to use the folder "${handle.name}" — open it again from File > Open Project.`)
}

// ---- Path resolution ----

interface Located {
  root: DirHandle
  parts: string[]
}

async function locate(path: string): Promise<Located> {
  const clean = path.replace(/\\/g, '/')
  if (clean.startsWith('web-opfs:')) {
    const root = await navigator.storage.getDirectory()
    return { root, parts: clean.slice('web-opfs:'.length).split('/').filter(Boolean) }
  }
  const m = /^web-dir:([^/]+)\/[^/]+(?:\/(.*))?$/.exec(clean)
  if (m) {
    const id = m[1]
    let root = rootHandles.get(id) ?? null
    if (!root) {
      root = await loadHandle(id)
      if (!root) throw new Error('That project folder is no longer available — open it again from File > Open Project.')
      rootHandles.set(id, root)
    }
    await ensurePermission(root)
    return { root, parts: (m[2] ?? '').split('/').filter(Boolean) }
  }
  throw new Error(`Not a folder path in the browser: ${path}`)
}

async function dirAt(root: DirHandle, parts: string[], create: boolean): Promise<DirHandle> {
  let dir = root
  for (const part of parts) dir = await dir.getDirectoryHandle(part, { create })
  return dir
}

async function fileAt(path: string, create: boolean): Promise<FileHandle> {
  const { root, parts } = await locate(path)
  const name = parts.pop()
  if (!name) throw new Error(`Not a file path: ${path}`)
  const dir = await dirAt(root, parts, create)
  return dir.getFileHandle(name, { create })
}

// ---- Reading ----

async function readBytes(path: string): Promise<Uint8Array> {
  const file = picked.get(path) ?? (path.startsWith('web-save:') ? null : await (await fileAt(path, false)).getFile())
  if (!file) throw new Error(`Can't read ${path}`)
  return new Uint8Array(await file.arrayBuffer())
}

export async function readTextFile(path: string): Promise<string> {
  return new TextDecoder().decode(await readBytes(path))
}
export const readBinaryFile = readBytes
export const readFileRaw = readBytes

// ---- Writing ----

function download(name: string, data: BlobPart, type = 'application/octet-stream') {
  const url = URL.createObjectURL(new Blob([data], { type }))
  const a = document.createElement('a')
  a.href = url
  a.download = name
  document.body.appendChild(a)
  a.click()
  a.remove()
  setTimeout(() => URL.revokeObjectURL(url), 60_000)
}

async function writeData(path: string, data: Uint8Array | string): Promise<void> {
  if (path.startsWith('web-save:')) {
    const handle = saves.get(path)
    const name = path.split('/').pop() || 'download'
    if (!handle) {
      download(name, typeof data === 'string' ? data : new Uint8Array(data), typeof data === 'string' ? 'text/plain' : undefined)
      return
    }
    const writable = await handle.createWritable()
    await writable.write(typeof data === 'string' ? data : new Uint8Array(data))
    await writable.close()
    return
  }
  const handle = await fileAt(path, true)
  const writable = await handle.createWritable()
  await writable.write(typeof data === 'string' ? data : new Uint8Array(data))
  await writable.close()
}

export const writeTextFile = (path: string, contents: string) => writeData(path, contents)
export const writeBinaryFile = (path: string, contents: Uint8Array) => writeData(path, contents)
export const writeFileRaw = (path: string, bytes: Uint8Array) => writeData(path, bytes)

// ---- Folders ----

export async function pickProjectFolder(): Promise<string | null> {
  if (canPickFolders) {
    try {
      const handle = await pickerWindow.showDirectoryPicker!({ mode: 'readwrite', id: 'an1mbyt3-project' })
      const id = newId()
      rootHandles.set(id, handle)
      await storeHandle(id, handle)
      return `web-dir:${id}/${handle.name}`
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') return null
      throw err
    }
  }
  // No folder access (Firefox / Safari): projects live in browser storage.
  const name = window.prompt('This browser keeps projects in its own storage. Project name:', 'My Project')
  if (!name) return null
  return `web-opfs:projects/${name.replace(/[\\/:*?"<>|]+/g, '').trim() || 'My Project'}`
}

export async function listProjectFiles(rootPath: string): Promise<ProjectEntry[]> {
  const { root, parts } = await locate(rootPath)
  const start = await dirAt(root, parts, true)
  const out: ProjectEntry[] = []
  const walk = async (dir: DirHandle, prefix: string) => {
    for await (const [name, handle] of (dir as DirHandle & { entries(): AsyncIterable<[string, FileSystemHandle]> }).entries()) {
      if (name.startsWith('.')) continue
      const rel = prefix ? `${prefix}/${name}` : name
      if (handle.kind === 'directory') await walk(handle as DirHandle, rel)
      else {
        const file = await (handle as FileHandle).getFile()
        out.push({ path: rel, size: file.size, modified: file.lastModified })
      }
    }
  }
  await walk(start, '')
  return out
}

export async function ensureDir(path: string): Promise<void> {
  const { root, parts } = await locate(path)
  await dirAt(root, parts, true)
}

export async function removeFile(path: string): Promise<void> {
  const { root, parts } = await locate(path)
  const name = parts.pop()
  if (!name) return
  try {
    await (await dirAt(root, parts, false)).removeEntry(name)
  } catch {
    // Already gone.
  }
}

export async function pathExists(path: string): Promise<boolean> {
  if (picked.has(path)) return true
  try {
    const { root, parts } = await locate(path)
    const name = parts.pop()
    if (!name) return true
    const dir = await dirAt(root, parts, false)
    try {
      await dir.getFileHandle(name)
      return true
    } catch {
      await dir.getDirectoryHandle(name)
      return true
    }
  } catch {
    return false
  }
}

export async function recoveryDir(): Promise<string> {
  return 'web-opfs:recovery'
}

// ---- Open / Save dialogs ----

/** A file chooser; `accept` like `.svg,image/*`. */
function pickFile(accept: string): Promise<string | null> {
  return new Promise((resolve) => {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = accept
    input.style.display = 'none'
    document.body.appendChild(input)
    let done = false
    const finish = (value: string | null) => {
      if (done) return
      done = true
      input.remove()
      resolve(value)
    }
    input.addEventListener('change', () => {
      const file = input.files?.[0]
      if (!file) return finish(null)
      const path = `web-pick:${newId()}/${file.name}`
      picked.set(path, file)
      finish(path)
    })
    input.addEventListener('cancel', () => finish(null))
    input.click()
  })
}

export const pickOpenPath = () => pickFile('.tea,.teab,.json')
export const pickOpenSvgPath = () => pickFile('.svg,image/svg+xml')
export const pickOpenImagePath = () => pickFile('image/*')
export const pickOpenAudioPath = () => pickFile('audio/*,.wav,.mp3,.ogg')
export const pickOpenModelPath = () => pickFile('.glb,.gltf,.vrm')

async function pickSave(suggestedName: string, description: string, extensions: string[]): Promise<string | null> {
  const id = newId()
  if (typeof pickerWindow.showSaveFilePicker === 'function') {
    try {
      const handle = await pickerWindow.showSaveFilePicker({
        suggestedName,
        types: extensions.length ? [{ description, accept: { 'application/octet-stream': extensions.map((e) => `.${e}`) } }] : undefined,
      })
      const path = `web-save:${id}/${handle.name}`
      saves.set(path, handle)
      return path
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') return null
      throw err
    }
  }
  // No save dialog: the file downloads under its suggested name.
  const path = `web-save:${id}/${suggestedName}`
  saves.set(path, null)
  return path
}

export const pickSavePath = (suggestedName: string) => pickSave(suggestedName, 'an1mbyt3 document', ['teab', 'tea'])
export const pickSaveGifPath = (suggestedName: string) => pickSave(suggestedName, 'GIF', ['gif'])
export const pickSaveHtmlPath = (suggestedName: string) => pickSave(suggestedName, 'Web page', ['html'])
export const pickSaveFilePath = (suggestedName: string, filterName: string, extensions: string[]) => pickSave(suggestedName, filterName, extensions)
export const pickSaveExePath = async (_suggestedName: string): Promise<string | null> => {
  window.alert('Exporting a desktop app (.exe) needs the an1mbyt3 desktop app. In the browser, use Export → Game → Web page instead.')
  return null
}

export async function exportDesktopGame(_path: string, _html: string): Promise<void> {
  throw new Error('Desktop app export needs the an1mbyt3 desktop app.')
}

export async function revealInExplorer(_path: string): Promise<void> {
  // Browsers can't open the OS file manager.
}
