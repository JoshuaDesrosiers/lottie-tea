/**
 * Export menu → "Download shared runtime…": hands the user
 * `toasty-runtime-<version>.zip` — the engine + embed JS as one versioned
 * folder (built by `scripts/build-runtime.mjs` into `public/runtime/`), for
 * self-hosting the runtime that "Website embed (shared runtime)" exports
 * load from.
 *
 * - **Web** (toastycreate.web.app, or the dev server): the zip is a static
 *   file on this very site, so it's a plain browser download of
 *   `/runtime/toasty-runtime-<version>.zip`.
 * - **Desktop** (Tauri): a native Save dialog, then the zip's bytes — read
 *   from the app's own bundled copy when it has one, else fetched from the
 *   hosted site.
 */
import { isDesktop, pickSaveFilePath, writeFileRaw } from '../ipc/nativeFile'
import { HOSTED_RUNTIME_ORIGIN, runtimeZipName, runtimeZipPath } from '../embed/runtimeVersion'

/** What happened: `saved` (desktop, with the path), `downloading`
 * (browser download started), or `cancelled`. Throws when the zip can't be
 * found anywhere. */
export type RuntimeDownloadResult = { kind: 'saved'; path: string } | { kind: 'downloading'; url: string } | { kind: 'cancelled' }

/** Whether `response` is really the zip (a static host answers a missing
 * file with its HTML app shell, status 200). */
function looksLikeZip(response: Response): boolean {
  const type = response.headers.get('content-type') ?? ''
  return response.ok && !type.includes('html')
}

/** `PK\x03\x04` — the first bytes of every zip. */
const isZip = (bytes: Uint8Array) => bytes.length > 4 && bytes[0] === 0x50 && bytes[1] === 0x4b && bytes[2] === 0x03 && bytes[3] === 0x04

async function fetchZip(url: string): Promise<Uint8Array | null> {
  try {
    const response = await fetch(url)
    if (!looksLikeZip(response)) return null
    const bytes = new Uint8Array(await response.arrayBuffer())
    return isZip(bytes) ? bytes : null
  } catch {
    return null
  }
}

export async function downloadSharedRuntime(): Promise<RuntimeDownloadResult> {
  const name = runtimeZipName()
  const local = runtimeZipPath()
  const hosted = `${HOSTED_RUNTIME_ORIGIN}${local}`
  if (!isDesktop) {
    // Make sure it's there before handing the URL to the browser (a
    // missing file would "download" the app's index.html as a .zip).
    const head = await fetch(local, { method: 'HEAD' }).catch(() => null)
    const url = head && looksLikeZip(head) ? local : hosted
    const link = document.createElement('a')
    link.href = url
    link.download = name
    link.rel = 'noopener'
    document.body.appendChild(link)
    link.click()
    link.remove()
    return { kind: 'downloading', url }
  }
  const path = await pickSaveFilePath(name, 'Zip archive', ['zip'])
  if (!path) return { kind: 'cancelled' }
  const bytes = (await fetchZip(local)) ?? (await fetchZip(hosted))
  if (!bytes) throw new Error(`couldn't find ${name} — run \`npm run build:player\` in apps/editor, or download it from ${hosted}`)
  await writeFileRaw(path, bytes)
  return { kind: 'saved', path }
}
