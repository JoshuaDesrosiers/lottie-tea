/**
 * The native shell's only real IPC surface left (see
 * `apps/editor/src-tauri/src/lib.rs`'s module doc): file-open/save dialogs
 * plus plain disk I/O, via `src-tauri/src/commands.rs`. Deliberately opaque
 * to document format — these calls only ever see a path and a string/byte
 * blob; `.tea` vs `.teab` encode/decode happens entirely in `WasmEditor`
 * (`ipc/wasmEditor.ts`), never here.
 */
import { invoke } from '@tauri-apps/api/core'

/** Opens a native "Save As" dialog pre-filled with `suggestedName`. Returns
 * `null` if the user cancels. */
export function pickSavePath(suggestedName: string): Promise<string | null> {
  return invoke('pick_save_path', { suggestedName })
}

/** Opens a native "Open" dialog filtered to `.tea`/`.teab`. Returns `null`
 * if the user cancels. */
export function pickOpenPath(): Promise<string | null> {
  return invoke('pick_open_path')
}

/** Opens a native "Open" dialog filtered to `.svg` — the Shapes panel's
 * "Import SVG" action. Returns `null` if the user cancels. */
export function pickOpenSvgPath(): Promise<string | null> {
  return invoke('pick_open_svg_path')
}

/** Opens a native "Open" dialog filtered to image files (png/jpg/jpeg/gif)
 * — the Shapes panel's "Import Image/GIF" action. Returns `null` if the user
 * cancels. */
export function pickOpenImagePath(): Promise<string | null> {
  return invoke('pick_open_image_path')
}

/** + menu → Import → Audio: a sound file. `null` = cancelled. */
export function pickOpenAudioPath(): Promise<string | null> {
  return invoke('pick_open_audio_path')
}

/** + menu → Import → 3D Model: a `.glb` / `.gltf` file. `null` = cancelled. */
export function pickOpenModelPath(): Promise<string | null> {
  return invoke('pick_open_model_path')
}

/** Opens a native "Save As" dialog filtered to `.gif` — the toolbar's
 * "Export GIF" action. Returns `null` if the user cancels. */
export function pickSaveGifPath(suggestedName: string): Promise<string | null> {
  return invoke('pick_save_gif_path', { suggestedName })
}

/** Opens a native "Save As" dialog filtered to `.html` — the toolbar's
 * "Export Game" action. Returns `null` if the user cancels. */
export function pickSaveHtmlPath(suggestedName: string): Promise<string | null> {
  return invoke('pick_save_html_path', { suggestedName })
}

/** "Save As" for Export Game → Desktop app (`.exe` on Windows). */
export function pickSaveExePath(suggestedName: string): Promise<string | null> {
  return invoke('pick_save_exe_path', { suggestedName })
}

/** Writes a desktop game: the prebuilt runner with `html` appended. */
export function exportDesktopGame(path: string, html: string): Promise<void> {
  return invoke('export_desktop_game', { path, html })
}

export function writeTextFile(path: string, contents: string): Promise<void> {
  return invoke('write_text_file', { path, contents })
}

export function writeBinaryFile(path: string, contents: Uint8Array): Promise<void> {
  return invoke('write_binary_file', { path, contents: Array.from(contents) })
}

export function readTextFile(path: string): Promise<string> {
  return invoke('read_text_file', { path })
}

export async function readBinaryFile(path: string): Promise<Uint8Array> {
  const bytes = await invoke<number[]>('read_binary_file', { path })
  return new Uint8Array(bytes)
}

// ---- Project folders (`src-tauri/src/commands.rs`, "Project folders") ----

/** Native folder picker. `null` = cancelled. */
export function pickProjectFolder(): Promise<string | null> {
  return invoke('pick_project_folder')
}

export interface ProjectEntry {
  /** Relative to the listed folder, `/`-separated. */
  path: string
  size: number
  /** ms since the Unix epoch. */
  modified: number
}

/** Every file under `root`, recursively (hidden entries skipped). */
export function listProjectFiles(root: string): Promise<ProjectEntry[]> {
  return invoke('list_project_files', { root })
}

export function ensureDir(path: string): Promise<void> {
  return invoke('ensure_dir', { path })
}

export function removeFile(path: string): Promise<void> {
  return invoke('remove_file', { path })
}

export function pathExists(path: string): Promise<boolean> {
  return invoke('path_exists', { path })
}

/** Writes raw bytes (no JSON number array — fast for images). Creates parent folders. */
export function writeFileRaw(path: string, bytes: Uint8Array): Promise<void> {
  return invoke('write_file_raw', bytes, { headers: { 'x-path': encodeURIComponent(path) } })
}

/** Reads a file as raw bytes (no JSON number array). */
export async function readFileRaw(path: string): Promise<Uint8Array> {
  const buffer = await invoke<ArrayBuffer>('read_file_raw', { path })
  return new Uint8Array(buffer)
}

/** Opens a folder in Explorer / Finder. */
export function revealInExplorer(path: string): Promise<void> {
  return invoke('reveal_in_explorer', { path })
}

/** `<app data>/recovery` — autosaves of folder-less documents. */
export function recoveryDir(): Promise<string> {
  return invoke('recovery_dir')
}
