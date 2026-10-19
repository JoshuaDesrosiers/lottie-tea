/**
 * "Export → Game → Website embed (shared runtime)": the game as a tiny
 * folder whose pages load the engine from a **shared runtime** instead of
 * carrying their own copy (the standalone "Website embed" export,
 * `game/exportEmbed.ts`, still ships everything and is unchanged).
 *
 * ```
 * <folder>/
 *   game.teab        the game
 *   index.html       a working page; loads the runtime from RUNTIME
 *   An1Player.tsx    React component; dynamic-imports the runtime
 *   README.md        standalone vs shared, HTML / React / self-hosting
 *   an1mbyt3.json    this game's scenes / inputs / events as data
 * ```
 *
 * The runtime URL defaults to the hosted, versioned folder
 * (`https://toastycreate.web.app/runtime/<RUNTIME_VERSION>/`, built by
 * `scripts/build-runtime.mjs`); both the page and the component keep it
 * in one obvious constant (`RUNTIME` / `RUNTIME_URL`) so pointing them at
 * a self-hosted copy of the downloadable zip is a one-line change, and
 * both pass it on as the player's `runtimeUrl` so the `.wasm` comes from
 * the same folder as the JS.
 */
import { saveTeab } from '../ipc/wasmEditor'
import { ensureDir, writeFileRaw, writeTextFile } from '../ipc/nativeFile'
import type { DocumentSummary } from '../ipc/document'
import type { WasmEditor } from '../wasm-pkg/an1mbyt3_wasm'
import { hostedRuntimeUrl, RUNTIME_VERSION } from '../embed/runtimeVersion'
import { fillEmbedTemplate, gameManifest } from './embedDocs'

import PAGE_TEMPLATE from '../embed/sharedRuntime/page.html?raw'
import COMPONENT_TEMPLATE from '../embed/sharedRuntime/An1Player.tsx?raw'
import README_TEMPLATE from '../embed/docs/SHARED_README.md?raw'

const join = (...parts: string[]) => parts.join('/').replace(/\\/g, '/').replace(/\/+/g, '/')

/** `{{KEY}}` → value, for every key. */
function fillPlaceholders(template: string, values: Record<string, string>): string {
  let out = template
  for (const [key, value] of Object.entries(values)) out = out.split(`{{${key}}}`).join(value)
  return out
}

export interface SharedEmbedOptions {
  /** The runtime folder the files load the engine from (trailing slash
   * added if missing). Default: the hosted folder for this editor's
   * `RUNTIME_VERSION`. */
  runtimeUrl?: string
}

/** Writes the shared-runtime embed of the open document into `folder`. */
export async function exportEmbedShared(editor: WasmEditor, folder: string, title: string, startScene: string, doc: DocumentSummary, options: SharedEmbedOptions = {}): Promise<void> {
  const raw = options.runtimeUrl ?? hostedRuntimeUrl()
  const runtimeUrl = raw.endsWith('/') ? raw : `${raw}/`
  const values = {
    RUNTIME_URL: runtimeUrl,
    RUNTIME_VERSION,
    TITLE_TEXT: title.replace(/[<&]/g, ''),
    START_SCENE_JSON: JSON.stringify(startScene),
  }
  await ensureDir(folder)
  await writeFileRaw(join(folder, 'game.teab'), saveTeab(editor))
  await writeTextFile(join(folder, 'index.html'), fillPlaceholders(PAGE_TEMPLATE, { ...values, TITLE: values.TITLE_TEXT }))
  await writeTextFile(join(folder, 'An1Player.tsx'), fillPlaceholders(COMPONENT_TEMPLATE, values))
  // The README also gets this game's surface (scenes, inputs, events).
  await writeTextFile(join(folder, 'README.md'), fillPlaceholders(fillEmbedTemplate(README_TEMPLATE, doc, title, startScene), values))
  await writeTextFile(join(folder, 'an1mbyt3.json'), JSON.stringify({ ...gameManifest(doc, title, startScene), runtime: { version: RUNTIME_VERSION, url: runtimeUrl } }, null, 2) + '\n')
}
