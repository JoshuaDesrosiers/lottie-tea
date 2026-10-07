/**
 * "Export Game": packs the current document into ONE self-contained
 * `.html` file — double-click it and the game runs in the browser, no
 * server, no install, nothing else to ship.
 *
 * The file holds the standalone player (`src/player/main.ts`, prebuilt to
 * `public/player/an1mbyt3-player.js` by `npm run build:player`), the
 * engine's `.wasm` and the game's `.teab`, the last two base64-encoded in
 * inert `<script>` tags the player decodes on boot.
 */
import { saveTeab } from '../ipc/wasmEditor'
import type { WasmEditor } from '../wasm-pkg/an1mbyt3_wasm'
import wasmUrl from '../wasm-pkg/an1mbyt3_wasm_bg.wasm?url'

function toBase64(bytes: Uint8Array): string {
  // Chunked: `String.fromCharCode(...bigArray)` overflows the call stack.
  let binary = ''
  const chunk = 0x8000
  for (let i = 0; i < bytes.length; i += chunk) binary += String.fromCharCode(...bytes.subarray(i, i + chunk))
  return btoa(binary)
}

const escapeHtml = (text: string) => text.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]!)

export interface ExportGameOptions {
  title: string
  /** Scene the game opens on (by name); the first scene if null/unknown. */
  startScene: string | null
}

export async function buildGameHtml(editor: WasmEditor, options: ExportGameOptions): Promise<string> {
  const [wasm, player] = await Promise.all([
    fetch(wasmUrl).then((r) => r.arrayBuffer()),
    fetch('/player/an1mbyt3-player.js').then((r) => {
      if (!r.ok) throw new Error('player bundle missing — run `npm run build:player` in apps/editor')
      return r.text()
    }),
  ])
  const teab = saveTeab(editor)
  const config = JSON.stringify(options).replace(/</g, '\\u003c')
  // An inline module can't contain a literal `</script`.
  const playerCode = player.replace(/<\/script/gi, '<\\/script')
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover">
<title>${escapeHtml(options.title)}</title>
<style>
  html, body { margin: 0; height: 100%; height: 100dvh; background: #000; overflow: hidden; overscroll-behavior: none; }
  body { display: flex; align-items: center; justify-content: center; -webkit-user-select: none; user-select: none; -webkit-tap-highlight-color: transparent; }
  #an1-canvas { display: block; outline: none; touch-action: none; }
  #an1-status { position: fixed; inset: 0; display: flex; align-items: center; justify-content: center;
    padding: 24px; color: #cfc8e0; font: 14px/1.5 system-ui, sans-serif; text-align: center; background: #000; }
</style>
</head>
<body>
<canvas id="an1-canvas" tabindex="0"></canvas>
<div id="an1-status">Loading…</div>
<script id="an1-config" type="application/json">${config}</script>
<script id="an1-wasm" type="application/octet-stream">${toBase64(new Uint8Array(wasm))}</script>
<script id="an1-game" type="application/octet-stream">${toBase64(teab)}</script>
<script type="module">
${playerCode}
</script>
</body>
</html>
`
}
