// Phase 8 exit-criterion demo glue (AN1MBYT3_BUILD_PLAN.md §10) — see
// teab-demo.html's own doc comment for the full "why this page exists".
// Unlike main.js (which talks to the raw `WasmPlayer` wasm-bindgen export
// directly), this page only ever touches `@an1mbyt3/runtime`'s public API:
// the `<animation-player>` custom element (registered below) and the
// `An1mbyt3Animation` instance it wraps.
import { registerAnimationPlayerElement } from '../../packages/runtime-js/dist/index.js'

registerAnimationPlayerElement()

const statusEl = document.getElementById('status')
const player = document.getElementById('player')
const toggleButton = document.getElementById('toggle-button')
const playButton = document.getElementById('play-button')
const pauseButton = document.getElementById('pause-button')
const seekButton = document.getElementById('seek-button')

async function main() {
  statusEl.textContent = 'creating WGPU surface…'

  // Setting `src` after the element's already connected (rather than as an
  // HTML attribute) so this page can wait for `demo.teab` itself to be
  // reachable and report a clear error if it isn't (e.g. the fixture hasn't
  // been generated yet) instead of a silent, harder-to-diagnose failure
  // inside the element.
  const response = await fetch('./demo.teab')
  if (!response.ok) {
    throw new Error(`fetch('./demo.teab') failed: ${response.status} ${response.statusText} — ` + 'run `cargo run -p an1mbyt3-cli --example make_demo_teab -- apps/web-test/demo.teab` first')
  }

  player.setAttribute('src', './demo.teab')
  // `attributeChangedCallback` kicks off `loadTeab` asynchronously — poll
  // for `player.animation` to exist rather than adding a bespoke "ready"
  // event to the custom element just for this one demo page.
  while (!player.animation) {
    await new Promise((resolve) => setTimeout(resolve, 10))
  }

  statusEl.textContent = `loaded demo.teab — state machines: ${player.animation.stateMachineNames().join(', ')}`

  toggleButton.addEventListener('click', () => player.animation.stateMachine('Main').fireTrigger('toggle'))
  playButton.addEventListener('click', () => player.animation.play())
  pauseButton.addEventListener('click', () => player.animation.pause())
  seekButton.addEventListener('click', () => player.animation.seek(0.3))
}

main().catch((err) => {
  statusEl.textContent = `error: ${err.message}`
  console.error(err)
})
