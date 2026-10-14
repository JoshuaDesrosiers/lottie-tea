# AGENTS.md — embedding and driving "{{TITLE}}" (an1mbyt3 game)

Instructions for AI coding assistants working in a project that embeds this
exported an1mbyt3 game. Humans: read `GUIDE.md` (in the export's root
folder; it isn't copied next to the code). This file is the
authoritative API reference; the type declarations in
`src/an1mbyt3/*.d.ts` are the source of truth for signatures.

## What this is

- A self-contained interactive scene/game (WebGPU, WebGL2 fallback) that runs
  in a container element and is steered by the host page.
- Shipped as prebuilt ES modules (no npm package, no build step of its own)
  plus two static binary assets.
- One player per page. Creating a second player destroys the first.

## Files and where they belong

| File | Put it in | Notes |
|---|---|---|
| `src/an1mbyt3/an1mbyt3-embed.js` (+ `.d.ts`) | source tree | `createPlayer`, `defineAn1PlayerElement` |
| `src/an1mbyt3/an1mbyt3-react.js` (+ `.d.ts`) | source tree | `<An1Player>`, `useAn1Player`; imports `react` (peer; React 19 for `ref` — on React 18 get the player from `onReady`) |
| `src/an1mbyt3/an1mbyt3-engine.js` | source tree, same folder | shared by both; do not import directly |
| `public/an1mbyt3/game.teab` | static/public folder | the game data |
| `public/an1mbyt3/an1mbyt3_wasm_bg.wasm` | static/public folder, **next to game.teab** | the engine (~25 MB, gzip/brotli it on the server) |

- Keep the three `.js` files in one folder: they import each other relatively.
- Default engine URL is resolved relative to the `game` URL. If the `.wasm`
  lives elsewhere, pass `wasm: '<url>'`.
- Never import from `public/` in Vite/Next (static files are not modules).
- The `.js`, `.wasm` and `.teab` come from the same export and must be
  replaced together when the game is re-exported.

## Plain JS API

```ts
import { createPlayer } from './an1mbyt3/an1mbyt3-embed.js'

const player = await createPlayer(container: HTMLElement, {
  game: string | URL | ArrayBuffer | Uint8Array,   // required
  wasm?: same,                                       // default: next to `game` (required if `game` is bytes)
  scene?: string,                                    // start scene name; default first scene
  fit?: 'contain' | 'cover',                         // default 'contain' (letterbox)
  keyboard?: 'focus' | 'window' | 'none',            // default 'focus' (canvas must be focused; click focuses it)
  data?: Record<string, unknown>,                    // initial data, same as set()
  paused?: boolean,
  background?: string,                               // letterbox colour
  stats?: boolean,                                   // FPS/profiler overlay (F3 toggles)
})
```

`createPlayer` resolves once the first scene is loaded and running.

| Member | Behaviour |
|---|---|
| `set(name, value)` / `setData(obj)` | Writes `ctx.data[name]` for scripts AND the current scene's Property `name` if it exists (coerced: Number, Bool, Text, Color = `'#rrggbb'` or `[r,g,b,a]` in 0..1). Applied at the next frame. Persists across scene switches (re-applied on entry). Cheap; safe to call at high frequency. |
| `get(name)` | Current scene Property value, else the last `set` value. |
| `setInput(name, value)` | State machine input of the active scene: Bool / Number / Enum; a Trigger fires when `value` is truthy. Unknown names log a warning. |
| `fire(trigger)` | Fires a Trigger input. |
| `loadScene(name)` | Switches scene after the current frame. |
| `call(name, arg?)` | Synchronously calls a function a game script registered with `ctx.on(name, fn)`; returns its result. Undefined + warning if not registered (registration happens when that script first runs). |
| `on(type, fn)` → unsubscribe; `off(type, fn)` | Events below. Also dispatched as DOM `CustomEvent('an1:<type>')` on the container. |
| `waitForState(state, layer = null)` | Promise resolving when the base layer (or named layer) is in `state` (immediately if already). |
| `pause()` / `play()` / `paused` | Freezes simulation and rendering. |
| `scenes`, `scene`, `state`, `layerStates` | Read-only snapshots. |
| `destroy()` | Stops the loop, removes the canvas, frees the engine. Always call on unmount. |

### Events

| Type | Payload | Fires when |
|---|---|---|
| `state` | `{ layer: string \| null, state, previous }` | a state machine layer enters a new state (`layer: null` = base layer) |
| `event` | `{ name, payload }` | a script calls `ctx.emit(name, payload)`, or an On Click "Call script function" names a function no script registered |
| `prop` | `{ name, value }` | the GAME changed a scene Property (not echoed for your own `set`) |
| `scene` | `{ scene, previous }` | the scene switched |
| `error` | `{ error }` | a frame threw (the loop keeps running) |
| `ready` | `{ scenes, scene }` | before `createPlayer` resolves — use the resolved player instead |

Initial values are not replayed as events: after `await createPlayer(...)`
read `player.state` / `player.scene`, then subscribe.

## React API

```tsx
import { An1Player, useAn1Player, type An1PlayerHandle } from './an1mbyt3/an1mbyt3-react.js'

<An1Player
  game="/an1mbyt3/game.teab"            // changing game / wasm / fit / keyboard / background reloads the player
  scene={sceneName}                     // live: switches scene
  data={{ price, stock }}               // live: shallow diff, only changed keys are sent (Object.is)
  inputs={{ isOpen, speed: 2 }}         // live: shallow diff → setInput
  paused={!visible}                     // live
  className="…" style={{ height: 400 }} // the box needs a height
  ref={ref}                             // Ref<An1PlayerHandle | null> → full JS API (React 19; on 18 use onReady)
  onReady={(player) => …}
  onEvent={(e) => …} onStateChange={(e) => …} onPropChange={(e) => …} onSceneChange={(e) => …} onError={(err) => …}
/>
```

- Handlers may be inline; they are read through a ref (no resubscribe).
- `data` / `inputs` diffs are shallow: mutate-in-place of a nested object is
  NOT detected — pass a new value for that key.
- A Trigger in `inputs` fires only when its value changes to truthy; for
  repeated firing use `ref.current?.fire(name)`.
- StrictMode double-mount is safe.
- Next.js / SSR: the component touches `window`; render it client-only:
  `'use client'` + `dynamic(() => import('./GameBox'), { ssr: false })`.

## Custom element

```html
<script type="module">
  import { defineAn1PlayerElement } from './an1mbyt3/an1mbyt3-embed.js'
  defineAn1PlayerElement()            // registers <an1-player>
</script>
<an1-player game="/an1mbyt3/game.teab" scene="Main" data='{"coins":3}' style="display:block;height:400px"></an1-player>
```

Attributes `game`, `wasm`, `scene`, `data` (JSON), `paused`, `fit`, `keyboard`.
`scene` / `data` / `paused` update live; `game` / `wasm` / `fit` / `keyboard`
reload. `element.player` is the JS API after `an1:ready`.

## Game side (when editing the game in the an1mbyt3 editor)

Script context (`ctx`) additions for embedding:

- `ctx.data` — read-only object of host data (`{}` in the editor).
- `ctx.emit(name, payload?)` — sends `event` to the host (logged in the editor's Console when not embedded).
- `ctx.on(name, fn)` — makes `fn` callable by `player.call(name, arg)` and by On Click actions.
- `ctx.props.<name>` — scene Properties; host `set()` writes them, game writes surface as `prop` events.

## Rules for code you write

1. Create the player once per mount; never recreate it to change data — use `set` / props.
2. Always `destroy()` on unmount (the React component does this for you).
3. Give the container an explicit size.
4. Only use names that exist in the manifest below (scenes, inputs, Properties, events, functions). Unknown input / function names only warn — they fail silently in production.
5. The player is null until ready. Buttons/handlers that need it: `const [ref, player] = useAn1Player()` then `<An1Player ref={ref}>` (re-renders once ready; disable controls while `!player`). One-time initial reads (`player.get(...)`, `player.state`): `onReady`. A plain `useRef` works too but doesn't re-render.
6. Data and Properties are *values*, not pulses: `set(name, v)` with the value it already has changes nothing, and the game sees the latest value every frame. For a one-shot action use a Trigger (`fire`) or a script function (`call`); if the game reads a data flag as a one-shot, toggle it back (`set(name, false)`) before setting it again.
7. Don't fetch inside the game for host data; fetch in the page and `set()` it.
8. Serve the `.wasm` with compression and long cache headers; it's the bulk of the download.
9. Use the transitions listed below to know which input reaches which state; `waitForState` / `onStateChange` confirm it happened.

## Recipes

```ts
// live API data → bound labels
const player = await createPlayer(el, { game: '/an1mbyt3/game.teab' })
setInterval(async () => player.setData(await (await fetch('/api/stats')).json()), 5000)

// animation as a message: wait for the intro, then enable the CTA
await player.waitForState('Idle'); button.disabled = false

// game event → app action
player.on('event', ({ name, payload }) => { if (name === 'purchase') cart.add(payload) })

// step-driven scenes in React
<An1Player game="/an1mbyt3/game.teab" scene={['Intro', 'Pick', 'Done'][step]} onSceneChange={(e) => setStep(['Intro','Pick','Done'].indexOf(e.scene))} />
```

## This game's surface

{{SURFACE}}

Machine-readable:

```json
{{MANIFEST}}
```
