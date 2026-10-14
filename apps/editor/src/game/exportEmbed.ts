/**
 * "Export → Website embed (React / JS)": the game as a drop-in folder for
 * a web project, driven from the page with the embed API (`src/embed`).
 *
 * ```
 * <folder>/
 *   README.md                     how to use it + everything this game exposes
 *   example.html                  a working page (serve the folder over http)
 *   src/an1mbyt3/                 import from your code
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

const BUNDLES = ['an1mbyt3-embed.js', 'an1mbyt3-react.js', 'an1mbyt3-engine.js']

const join = (...parts: string[]) => parts.join('/').replace(/\\/g, '/').replace(/\/+/g, '/')

/** Names scripts pass to `ctx.emit('…')` / `ctx.on('…')`, found in the
 * document's code (state programs and project files). */
function scriptNames(doc: DocumentSummary, call: 'emit' | 'on'): string[] {
  const text = JSON.stringify(doc)
  const found = new Set<string>()
  const pattern = new RegExp(String.raw`ctx\.${call}\(\s*\\?['"\x60]([^'"\x60\\]+)`, 'g')
  for (const m of text.matchAll(pattern)) found.add(m[1])
  return [...found].sort()
}

function readme(doc: DocumentSummary, startScene: string): string {
  const lines: string[] = []
  for (const scene of doc.artboards) {
    lines.push(`### Scene "${scene.name}"${scene.name === startScene ? ' (opens first)' : ''}`)
    const props = (doc.properties ?? []).filter((p) => p.artboard.idx === scene.id.idx && p.artboard.version === scene.id.version)
    if (props.length) {
      lines.push('', 'Properties — `player.set(name, value)` / `data={{ … }}`:', '')
      for (const p of props) lines.push(`- \`${p.name}\` (${p.kind}, starts at ${p.default})`)
    }
    for (const m of scene.stateMachines) {
      const states = m.states.map((s) => s.name)
      lines.push('', `State machine "${m.name}" — \`on('state')\` reports: ${states.map((s) => `\`${s}\``).join(', ') || '(no states)'}`)
      for (const layer of m.layers) lines.push(`- layer \`${layer.name}\`: ${layer.states.map((s) => `\`${s.name}\``).join(', ')}`)
      if (m.inputs.length) {
        lines.push('', 'Inputs — `player.setInput(name, value)` / `inputs={{ … }}` (Triggers: `player.fire(name)`):', '')
        for (const i of m.inputs) lines.push(`- \`${i.name}\` (${i.kind})`)
      }
    }
    lines.push('')
  }
  const emits = scriptNames(doc, 'emit')
  const fns = scriptNames(doc, 'on')
  const exposed = lines.join('\n')
  return `# ${startScene} — website embed

This folder is an an1mbyt3 game packaged to run inside your own site and be
driven by it: pass in live data, flip state machine inputs, fire triggers,
switch scenes, call game functions — and hear back when states change or
the game sends an event.

## Files

- \`src/an1mbyt3/\` — JavaScript to import (copy into your project's source).
- \`public/an1mbyt3/\` — the game (\`game.teab\`) and engine (\`an1mbyt3_wasm_bg.wasm\`):
  copy into your site's static / public folder so they're served at \`/an1mbyt3/…\`.
- \`example.html\` — a working page. Browsers won't load the engine from \`file://\`:
  run any static server in this folder (\`npx serve .\`) and open \`/example.html\`.

## React

\`\`\`tsx
import { An1Player } from './an1mbyt3/an1mbyt3-react.js'   // 'use client' in Next.js

export function Hero({ user, cart }) {
  return (
    <An1Player
      game="/an1mbyt3/game.teab"
      style={{ width: '100%', height: 480 }}
      data={{ userName: user.name, items: cart.length }}    // live: only changed keys are sent
      inputs={{ isOpen: cart.length > 0 }}                    // state machine inputs
      onEvent={(e) => console.log('game says', e.name, e.payload)}
      onStateChange={(e) => e.state === 'Celebrate' && confetti()}
    />
  )
}
\`\`\`

Props update the running game in place — no reload, no remount, no flicker.
Need imperative control? \`ref\`: \`ref.current.fire('pop')\`, \`ref.current.loadScene('Shop')\`,
\`await ref.current.waitForState('Done')\`.

## Plain JavaScript

\`\`\`js
import { createPlayer } from './an1mbyt3/an1mbyt3-embed.js'

const player = await createPlayer(document.getElementById('game'), { game: '/an1mbyt3/game.teab' })
const price = await fetch('/api/price').then((r) => r.json())
player.set('price', price.amount)            // scene Property "price" + ctx.data.price
player.setInput('speed', 2)
player.fire('jump')
player.loadScene('Shop')
player.on('state', ({ layer, state, previous }) => { /* animations as messages */ })
player.on('event', ({ name, payload }) => { /* ctx.emit(...) from the game */ })
player.on('prop', ({ name, value }) => { /* the game changed a Property */ })
\`\`\`

Or as a tag: call \`defineAn1PlayerElement()\` once, then
\`<an1-player game="/an1mbyt3/game.teab" scene="Main" data='{"price": 9}'></an1-player>\`
(listen for \`an1:state\`, \`an1:event\`, … on it; \`element.player\` is the API).

## Inside the game

- Read page data in scripts: \`ctx.data.price\` (and any scene Property with the same
  name gets the value — Property Bindings update labels / colours with no script).
- Send to the page: \`ctx.emit('purchase', { item: 'sword' })\`, or an On Click
  "Call script function" whose function no script registers.
- Let the page call in: \`ctx.on('reset', (arg) => { … })\` → \`player.call('reset', arg)\`.

## What this game exposes

${exposed}
${emits.length ? `Events it sends (\`ctx.emit\`): ${emits.map((n) => `\`${n}\``).join(', ')}\n` : ''}${fns.length ? `Functions the page can call (\`player.call\`): ${fns.map((n) => `\`${n}\``).join(', ')}\n` : ''}
One player per page at a time (a new one replaces the old).
`
}

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
  #log { height: 140px; overflow: auto; background: #1a1d24; border-radius: 8px; padding: 8px 12px; font: 12px/1.6 ui-monospace, monospace; }
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
  // Opens on `startScene`: the README names it, the page passes it.
  await writeTextFile(join(folder, 'README.md'), readme(doc, startScene))
  await writeTextFile(join(folder, 'example.html'), exampleHtml(title).replace("{ game: './public/an1mbyt3/game.teab' }", `{ game: './public/an1mbyt3/game.teab', scene: ${JSON.stringify(startScene)} }`))
}
