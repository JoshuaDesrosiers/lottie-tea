# {{TITLE}} on your website — the guide

This folder lets you put **{{TITLE}}**, an interactive scene made in an1mbyt3,
on your own website, and control it from your site's code. It runs in the
visitor's browser, nothing to install or host besides a few static files.

You can:

- **send it data** — a user's name, a price from your API, a score — and
  labels, bars and colours bound to that data update on the next frame;
- **flip its switches** — state machine inputs and triggers, scene changes;
- **listen to it** — know when an animation reaches a state, when the game
  changes a value, or when it sends you an event (a button was clicked, a
  level was finished, an item was bought).

Everything below works the same in React, Vue, Svelte, plain HTML…
React gets a ready-made component.

---

## 1. Put the files in your project

```
src/an1mbyt3/      → copy into your source folder (it's code you import)
public/an1mbyt3/   → copy into your static / public folder
```

After that, your site must serve the game at `/an1mbyt3/game.teab` (and the
engine next to it). In Vite, Next.js and Create React App that's exactly the
`public/` folder. Somewhere else? Put both files wherever your static files
live and pass the URL you end up with as `game`.

> Opening `example.html` straight from disk won't work — browsers refuse to
> load the engine from `file://`. Run any static server in this folder
> (for example `npx serve .`) and open `http://localhost:3000/example.html`.

## 2. Show it

**React**

```tsx
import { An1Player } from './an1mbyt3/an1mbyt3-react.js'

export function Hero() {
  return <An1Player game="/an1mbyt3/game.teab" style={{ width: '100%', height: 480 }} />
}
```

**Any other page**

```html
<div id="game" style="width: 100%; height: 480px"></div>
<script type="module">
  import { createPlayer } from './an1mbyt3/an1mbyt3-embed.js'
  const player = await createPlayer(document.getElementById('game'), { game: '/an1mbyt3/game.teab' })
</script>
```

The game fits inside its box (letterboxed, like a video). **Give the box a
height** — an empty `div` is 0 px tall and you'd see nothing. Use
`fit: 'cover'` (React: `fit="cover"`) to fill the box and crop instead.

## 3. Send it data

```tsx
<An1Player game="/an1mbyt3/game.teab" data={{ userName: user.name, coins: wallet.coins }} />
```

```js
player.set('coins', 120)
player.setData({ userName: 'Ada', coins: 120 })
```

Each value goes two places:

1. **Scene Properties with the same name.** If the scene has a Property
   called `coins`, it now holds 120 — and anything bound to it (a text label,
   a bar, a colour) updates by itself. No game code needed. Values are
   converted to the Property's type (Number, Bool, Text, Color — colours can
   be `'#ff8800'` or `[r, g, b, a]` from 0 to 1).
2. **`ctx.data` in the game's scripts**, for anything fancier
   (`ctx.data.userName`).

Updates are cheap: call `set` as often as you like (every keystroke, every
poll). In React, only the keys whose value changed are sent; nothing reloads.
Data stays put when the game switches scenes.

Data are *values*, not button presses: sending the same value again changes
nothing. For "do this now" moments, use a trigger (`fire`) — see below.

## 4. Flip switches

```js
player.setInput('isOpen', true)   // Bool, Number or Enum input
player.fire('celebrate')          // a Trigger
player.loadScene('Shop')          // switch scene
player.call('reset')              // a function the game's script offered (see below)
player.pause(); player.play()
```

React: `inputs={{ isOpen: menuOpen }}` and `scene="Shop"` are props; for
one-off actions use a ref (React 19 — on React 18, keep the player from `onReady={(p) => …}` instead):

```tsx
const ref = useRef(null)
<An1Player ref={ref} game="/an1mbyt3/game.teab" />
<button onClick={() => ref.current?.fire('celebrate')}>🎉</button>
```

## 5. Listen to it

```js
player.on('state', ({ layer, state, previous }) => { … })  // an animation state was entered
player.on('event', ({ name, payload }) => { … })           // the game sent you something
player.on('prop',  ({ name, value }) => { … })             // the game changed a Property
player.on('scene', ({ scene, previous }) => { … })         // the scene switched
await player.waitForState('Done')                           // wait for a moment in the animation
```

React: `onStateChange`, `onEvent`, `onPropChange`, `onSceneChange`, `onReady`.

**Animations as messages.** Because you hear about every state the state
machine enters, the animation itself can carry meaning: when it reaches
`Success`, show a toast; when the intro ends, enable a button; when a
character reaches `Sad`, open the help chat.

## 6. Let the game talk back (for whoever edits the game)

In the an1mbyt3 editor, a script can:

```js
ctx.emit('purchase', { item: 'sword', price: 30 })   // → player.on('event') / onEvent
ctx.data.userName                                     // ← player.set('userName', …)
ctx.on('reset', (arg) => { … })                      // ← player.call('reset', arg)
```

No scripting? A button's **On Click → Call script function** with a name no
script handles is delivered to your page as an event with that name.

## What {{TITLE}} offers

{{SURFACE}}

## Troubleshooting

- **Nothing shows.** The box has no height, or `/an1mbyt3/game.teab` 404s —
  check the Network tab. The engine (`an1mbyt3_wasm_bg.wasm`) must sit next
  to the game, or pass its URL as `wasm`.
- **Keys don't reach the game.** By default it only listens while focused —
  click it first, or use `keyboard: 'window'`.
- **Next.js: "window is not defined".** The player only runs in the browser:
  load it with `dynamic(() => import('./GameBox'), { ssr: false })` and put
  `'use client'` at the top of that file.
- **Several games on one page.** Fine — each player is independent and they
  share one engine download. Players scrolled out of view pause. Keep it to
  a handful of full-engine players per page (each holds a GPU context);
  use Toasty Lite for many small animations.
- **Updating the game.** Export again and replace *all* the files — the
  engine, the game and the JavaScript belong together.
