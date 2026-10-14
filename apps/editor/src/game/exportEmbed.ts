/**
 * "Export → Website embed (React / JS)": the game as a drop-in folder for
 * a web project, driven from the page with the embed API (`src/embed`).
 *
 * ```
 * <folder>/
 *   README.md                     what it is + quick start
 *   GUIDE.md                      step-by-step guide for people
 *   AGENTS.md                     API + rules for AI coding assistants
 *   an1mbyt3.json                 this game's scenes / inputs / events as data
 *   example.html                  a working page (serve the folder over http)
 *   src/an1mbyt3/                 import from your code (+ AGENTS.md, CLAUDE.md)
 *     an1mbyt3-embed.js / .d.ts   createPlayer, <an1-player>
 *     an1mbyt3-react.js / .d.ts   <An1Player> for React
 *     an1mbyt3-engine.js          shared by both
 *   public/an1mbyt3/              static files your site serves as-is
 *     game.teab                   the game
 *     an1mbyt3_wasm_bg.wasm       the engine
 * ```
 *
 * The two halves mirror a Vite / Next / CRA project root, so both folders
 * can be dragged straight in. The JS comes prebuilt from `public/embed`
 * (`npm run build:embed`, part of `build:player`).
 */
import { saveTeab } from '../ipc/wasmEditor'
import { ensureDir, writeFileRaw, writeTextFile } from '../ipc/nativeFile'
import type { DocumentSummary } from '../ipc/document'
import type { WasmEditor } from '../wasm-pkg/an1mbyt3_wasm'
import wasmUrl from '../wasm-pkg/an1mbyt3_wasm_bg.wasm?url'
import { EMBED_DTS, REACT_DTS } from '../embed/types'
import { embedDocs } from './embedDocs'

const BUNDLES = ['an1mbyt3-embed.js', 'an1mbyt3-react.js', 'an1mbyt3-engine.js']

const join = (...parts: string[]) => parts.join('/').replace(/\\/g, '/').replace(/\/+/g, '/')

function exampleHtml(title: string): string {
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${title.replace(/[<&]/g, '')} — embed example</title>
<style>
  body { margin: 0; font: 14px/1.5 system-ui, sans-serif; background: #111318; color: #e8e6ef; }
  main { max-width: 960px; margin: 0 auto; padding: 24px; display: grid; gap: 16px; }
  #game { height: 480px; border-radius: 12px; overflow: hidden; background: #000; }
  #log { height: 140px; overflow: auto; white-space: pre-wrap; background: #1a1d24; border-radius: 8px; padding: 8px 12px; font: 12px/1.6 ui-monospace, monospace; }
  .row { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  button, input, select { font: inherit; padding: 6px 10px; border-radius: 6px; border: 1px solid #333846; background: #1f232c; color: inherit; }
</style>
</head>
<body>
<main>
  <div id="game"></div>
  <div class="row">
    <select id="scene"></select>
    <input id="key" placeholder="data / property name">
    <input id="value" placeholder="value">
    <button id="set">set</button>
    <input id="trigger" placeholder="trigger name">
    <button id="fire">fire</button>
  </div>
  <div id="log"></div>
</main>
<script type="module">
  import { createPlayer } from './src/an1mbyt3/an1mbyt3-embed.js'
  const log = (text) => { const el = document.getElementById('log'); el.textContent += text + '\\n'; el.scrollTop = el.scrollHeight }
  const player = await createPlayer(document.getElementById('game'), { game: './public/an1mbyt3/game.teab' })
  const scenes = document.getElementById('scene')
  for (const name of player.scenes) scenes.add(new Option(name, name, false, name === player.scene))
  scenes.onchange = () => player.loadScene(scenes.value)
  document.getElementById('set').onclick = () => {
    const raw = document.getElementById('value').value
    let value = raw
    try { value = JSON.parse(raw) } catch {}
    player.set(document.getElementById('key').value, value)
  }
  document.getElementById('fire').onclick = () => player.fire(document.getElementById('trigger').value)
  player.on('state', (e) => log('state  ' + (e.layer ? e.layer + ': ' : '') + e.previous + ' → ' + e.state))
  player.on('scene', (e) => { log('scene  ' + e.scene); scenes.value = e.scene })
  player.on('prop', (e) => log('prop   ' + e.name + ' = ' + JSON.stringify(e.value)))
  player.on('event', (e) => log('event  ' + e.name + ' ' + JSON.stringify(e.payload ?? null)))
  log('ready — scenes: ' + player.scenes.join(', '))
</script>
</body>
</html>
`
}

export async function exportEmbed(editor: WasmEditor, folder: string, title: string, startScene: string, doc: DocumentSummary): Promise<void> {
  const [wasm, ...bundles] = await Promise.all([
    fetch(wasmUrl).then((r) => r.arrayBuffer()),
    ...BUNDLES.map((name) =>
      fetch(`/embed/${name}`).then((r) => {
        if (!r.ok) throw new Error('embed bundle missing — run `npm run build:embed` in apps/editor')
        return r.text()
      }),
    ),
  ])
  const src = join(folder, 'src/an1mbyt3')
  const pub = join(folder, 'public/an1mbyt3')
  await ensureDir(src)
  await ensureDir(pub)
  const encoder = new TextEncoder()
  for (let i = 0; i < BUNDLES.length; i++) await writeFileRaw(join(src, BUNDLES[i]), encoder.encode(bundles[i]))
  await writeTextFile(join(src, 'an1mbyt3-embed.d.ts'), EMBED_DTS)
  await writeTextFile(join(src, 'an1mbyt3-react.d.ts'), REACT_DTS)
  await writeFileRaw(join(pub, 'game.teab'), saveTeab(editor))
  await writeFileRaw(join(pub, 'an1mbyt3_wasm_bg.wasm'), new Uint8Array(wasm))
  // People get README + GUIDE; AI assistants AGENTS.md (also next to the
  // code, with a CLAUDE.md pointer, so it travels into the host repo).
  const docs = embedDocs(doc, title, startScene)
  await writeTextFile(join(folder, 'README.md'), docs.readme)
  await writeTextFile(join(folder, 'GUIDE.md'), docs.guide)
  await writeTextFile(join(folder, 'AGENTS.md'), docs.agents)
  await writeTextFile(join(folder, 'an1mbyt3.json'), docs.manifest)
  await writeTextFile(join(src, 'AGENTS.md'), docs.agents)
  await writeTextFile(join(src, 'CLAUDE.md'), docs.claude)
  await writeTextFile(join(folder, 'example.html'), exampleHtml(title).replace("{ game: './public/an1mbyt3/game.teab' }", `{ game: './public/an1mbyt3/game.teab', scene: ${JSON.stringify(startScene)} }`))
}
