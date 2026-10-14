/**
 * The standalone game player — what "Export Game" ships.
 *
 * An exported game is one self-contained `.html` file (see
 * `game/exportGame.ts`). It carries three payloads in inert `<script>`
 * tags, plus this file's bundled code:
 * - `#an1-config` (JSON): `{ title, startScene }`
 * - `#an1-wasm`   (base64): the engine (`an1mbyt3_wasm_bg.wasm`)
 * - `#an1-game`   (base64): the document (`.teab`)
 *
 * The game itself runs through `player/engine.ts` (the same player the
 * React / JS embed uses), filling the whole page. Once it's running, the
 * page's own scripts can reach it as `window.an1Player` (the embed API:
 * `set`, `setInput`, `fire`, `loadScene`, `on`, …).
 */
import { createPlayer, type An1Player } from './engine'

interface PlayerConfig {
  title: string
  startScene: string | null
}

declare global {
  interface Window {
    an1Player?: An1Player
  }
}

function payload(id: string): string {
  const el = document.getElementById(id)
  if (!el?.textContent) throw new Error(`missing ${id}`)
  return el.textContent.trim()
}

function base64Bytes(text: string): Uint8Array<ArrayBuffer> {
  const raw = atob(text)
  const bytes = new Uint8Array(new ArrayBuffer(raw.length))
  for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i)
  return bytes
}

function showMessage(text: string) {
  const el = document.getElementById('an1-status')
  if (!el) return
  el.textContent = text
  el.style.display = text ? 'flex' : 'none'
}

async function boot() {
  const config = JSON.parse(payload('an1-config')) as PlayerConfig
  document.title = config.title
  // The game fills the (visual) viewport; the old page's own canvas is
  // replaced by the player's.
  document.getElementById('an1-canvas')?.remove()
  const box = document.createElement('div')
  box.style.cssText = 'position:fixed;inset:0;height:100dvh;'
  document.body.appendChild(box)
  window.an1Player = await createPlayer(box, {
    game: base64Bytes(payload('an1-game')),
    wasm: base64Bytes(payload('an1-wasm')),
    scene: config.startScene ?? undefined,
    keyboard: 'window',
  })
  window.an1Player.canvas.focus({ preventScroll: true })
  showMessage('')
}

boot().catch((err) => {
  console.error(err)
  showMessage(`Couldn't start the game: ${err instanceof Error ? err.message : String(err)}`)
})
