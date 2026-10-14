# {{TITLE}} — website embed

An interactive an1mbyt3 scene, packaged to run inside your website and be
driven by your code: send it live data, flip its inputs, switch scenes, and
hear back when its animations change state or it sends you an event.

- **New to this?** Read [GUIDE.md](GUIDE.md) — a step-by-step walkthrough.
- **Using an AI coding assistant?** [AGENTS.md](AGENTS.md) is written for it
  (Claude Code, Cursor, Copilot and others pick it up automatically): the
  full API, the rules, and this game's exact inputs and events.
- **Just looking?** Run any static server here (`npx serve .`) and open
  `/example.html`.

## Quick start

1. Copy `src/an1mbyt3/` into your source folder and `public/an1mbyt3/` into
   your static/public folder.
2. Show it:

```tsx
import { An1Player } from './an1mbyt3/an1mbyt3-react.js'

<An1Player
  game="/an1mbyt3/game.teab"
  style={{ width: '100%', height: 480 }}
  data={{ userName: 'Ada' }}
  onStateChange={(e) => console.log('now in', e.state)}
  onEvent={(e) => console.log('game says', e.name, e.payload)}
/>
```

Not React? `createPlayer(element, { game: '/an1mbyt3/game.teab' })` from
`an1mbyt3-embed.js` gives you the same controls. See GUIDE.md.

## What {{TITLE}} offers

{{SURFACE}}
