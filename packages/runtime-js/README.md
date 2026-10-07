# packages/runtime-js

Vanilla JS runtime API and the `<animation-player>` custom element
(AN1MBYT3_BUILD_PLAN.md §8) — first-class, independent of React, not a
byproduct of the React wrapper. Hand-written TypeScript wrapper around the raw
`an1mbyt3-wasm` exports; consumers never touch `wasm-bindgen` glue directly.

**Depends on:** the compiled `an1mbyt3-wasm` npm package (built via
`npm run build:wasm`, which runs `wasm-pack build` into `src/wasm-pkg`).
**Depended on by:** `@an1mbyt3/react` (`packages/runtime-react`).

## Status

Implemented (Phase 8, AN1MBYT3_BUILD_PLAN.md §10):

- `An1mbyt3Animation` — the core class. `create(canvas, options)`,
  `loadTeab(urlOrBytes)`, `play()`/`pause()`/`seek(seconds)`,
  `stateMachineNames()`, `stateMachine(name)` (returns a `StateMachineHandle`
  with `setBoolean`/`setNumber`/`setEnum`/`fireTrigger`), `resize(w, h)`,
  `destroy()`.
- `<animation-player src="...">` — the custom element (`registerAnimationPlayerElement()`
  registers it). Exposes the underlying `An1mbyt3Animation` via its own
  `.animation` getter for anything beyond the `src` attribute.

**Known scope limit:** `WasmPlayer` only ever drives *one* state machine at a
time (whichever the loaded artboard's first one is) — `stateMachine(name)`
exists for API-shape compatibility with AN1MBYT3_BUILD_PLAN.md §8's
`animation.stateMachine("Character")`, but doesn't yet *select* a different
one by name if an artboard has more than one. Warns to the console if `name`
doesn't match anything on the loaded artboard.

## Building

```sh
npm run build:wasm   # wasm-pack build -> src/wasm-pkg
npm run build        # tsc -b, then copies src/wasm-pkg -> dist/wasm-pkg
```

See `apps/web-test/teab-demo.html`/`teab-demo.js` for a working example
(loads a real `.teab` built via
`cargo run -p an1mbyt3-cli --example make_demo_teab`).
