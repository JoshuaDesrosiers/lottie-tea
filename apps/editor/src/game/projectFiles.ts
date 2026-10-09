/**
 * The project's code / markup files (`an1mbyt3_core::TextFile`), as the
 * runtime sees them: a path → text map kept in sync with the document
 * summary (`setProjectFiles`), and `resolveFileRef` turning an `@file:`
 * reference into that file's text.
 *
 * Used wherever a text field may point at a file instead of holding the
 * text inline — HTML UI's `html` / `css` (`htmlOverlay.ts`), a state's
 * Code program (`scriptHost.ts`). One module so the editor's Design view,
 * Playground and exported games all resolve the same way, and edits in the
 * Code tab show up live (the summary refreshes, the map updates, the
 * overlay sees new text and re-renders).
 */
import type { TextFileSummary } from '../ipc/document'

export const FILE_REF_PREFIX = '@file:'

let files = new Map<string, string>()

/** Replaces the known files (call whenever the document summary changes). */
export function setProjectFiles(list: readonly TextFileSummary[] | undefined): void {
  const next = new Map<string, string>()
  for (const f of list ?? []) next.set(f.path, f.text)
  files = next
}

/** Same cleanup as `an1mbyt3_core::text_file::normalize_path`. */
export function normalizeFilePath(path: string): string {
  return path
    .replace(/\\/g, '/')
    .split('/')
    .map((s) => s.trim())
    .filter((s) => s && s !== '.' && s !== '..')
    .join('/')
}

/** If `value` is an `@file:` reference, the path it names. */
export function fileRefPath(value: string | null | undefined): string | null {
  return value?.startsWith(FILE_REF_PREFIX) ? normalizeFilePath(value.slice(FILE_REF_PREFIX.length)) : null
}

/** `"@file:<path>"` */
export function fileRef(path: string): string {
  return FILE_REF_PREFIX + normalizeFilePath(path)
}

/** The text `value` stands for: the referenced file's text, or `value`
 * itself if it isn't a reference. A reference to a missing file gives
 * `null` (callers show / run nothing rather than the raw reference). */
export function resolveFileRef(value: string): string | null {
  const path = fileRefPath(value)
  return path === null ? value : (files.get(path) ?? null)
}

/** Updates one file's text immediately (typing in the Code tab), ahead of
 * the next document summary — so HTML UI using it re-renders as you type. */
export function patchProjectFile(path: string, text: string): void {
  files.set(normalizeFilePath(path), text)
}

/** Every known file path (for pickers that aren't handed the document). */
export function listProjectFiles(): string[] {
  return [...files.keys()].sort()
}

/** Asks the editor to open `path` in the Code tab (handled in `App.tsx`). */
export function requestOpenFile(path: string): void {
  window.dispatchEvent(new CustomEvent('an1:open-file', { detail: path }))
}
