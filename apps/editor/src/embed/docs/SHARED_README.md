# {{TITLE}} — website embed (shared runtime)

An interactive Toasty (an1mbyt3) scene for your website that **doesn't carry
its own engine**: it loads the engine from the Toasty *shared runtime*,
version `{{RUNTIME_VERSION}}`. This folder is just the game and a few small
files — a few KB of glue instead of 20+ MB per game, and every game on a
page (or across your sites) shares one cached engine download.

| file | what |
| --- | --- |
| `game.teab` | the game — serve it as a static file |
| `index.html` | a working page (run `npx serve .` here, open `/`) |
| `An1Player.tsx` | a React component, no npm package needed |
| `an1mbyt3.json` | this game's scenes, inputs and events as data |

## Standalone vs shared runtime

Toasty has two website-embed exports:

- **Website embed (standalone)** — the folder carries everything: the game,
  the engine `.wasm` and the embed JS. Nothing is loaded from anyone else's
  server; best for offline / locked-down sites, or to pin an exact engine
  build next to the game.
- **Website embed (shared runtime)** — *this one*. The game only; the
  engine comes from a runtime folder, by default the hosted copy at
  `{{RUNTIME_URL}}`. Small to ship, cached across pages and games, and
  several games on one page download the engine once. You can self-host
  the runtime too (below).

Both use the same API (`createPlayer`, `<an1-player>`, `<An1Player>`), and
both support any number of players on one page.

## Plain HTML / JS

```html
<div id="game" style="width:100%;height:480px"></div>
<script type="module">
  const RUNTIME = '{{RUNTIME_URL}}'
  const { createPlayer } = await import(RUNTIME + 'an1mbyt3-embed.js')
  const player = await createPlayer(document.getElementById('game'), { game: '/game.teab', runtimeUrl: RUNTIME })
  player.on('event', (e) => console.log('game says', e.name, e.payload))
</script>
```

Or as tags — as many as you like:

```html
<script type="module">
  const { defineAn1PlayerElement } = await import('{{RUNTIME_URL}}an1mbyt3-embed.js')
  defineAn1PlayerElement()
</script>
<an1-player game="/game.teab" runtime="{{RUNTIME_URL}}" style="display:block;height:480px"></an1-player>
```

## React

Copy `An1Player.tsx` into your app (React 18+; Vite, Next.js, webpack…):

```tsx
import { An1Player } from './An1Player'

<An1Player
  game="/game.teab"
  style={{ width: '100%', height: 480 }}
  data={{ userName: 'Ada' }}
  onStateChange={(e) => console.log('now in', e.state)}
  onEvent={(e) => console.log('game says', e.name, e.payload)}
/>
```

It imports the player from the runtime at run time (no npm install). Put
`game.teab` in your static / `public/` folder.

## Self-hosting the runtime

1. Get `toasty-runtime-{{RUNTIME_VERSION}}.zip` — in the Toasty editor:
   **Export → Download shared runtime…** (or
   `https://toastycreate.web.app/runtime/toasty-runtime-{{RUNTIME_VERSION}}.zip`).
2. Unzip it into your site's static files, e.g.
   `/vendor/toasty-runtime-{{RUNTIME_VERSION}}/`.
3. Point at it: the `RUNTIME` line in `index.html`, the `RUNTIME_URL`
   constant in `An1Player.tsx`, or a `runtimeUrl` / `runtime` option per
   player. Serve `.wasm` as `application/wasm`; a runtime on another origin
   must send `Access-Control-Allow-Origin`.

Keep the runtime version the game was exported for (`{{RUNTIME_VERSION}}`)
unless the release notes say a newer one is compatible.

## What {{TITLE}} offers

{{SURFACE}}
