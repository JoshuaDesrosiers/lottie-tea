# packages/runtime-react

React wrapper exposing `<Animation src="/character.teab" stateMachine="Character" />`
(AN1MBYT3_BUILD_PLAN.md §8). Thin layer over `@an1mbyt3/runtime`
(`packages/runtime-js`) — no animation logic here, same rule as `an1mbyt3-wasm`.

**Depends on:** `@an1mbyt3/runtime` (`packages/runtime-js`).

## Status

Implemented (Phase 8, AN1MBYT3_BUILD_PLAN.md §10): the `<Animation>`
component (`src/Animation.tsx`) creates a `@an1mbyt3/runtime`
`An1mbyt3Animation` against an owned `<canvas>`, loads `src`, keeps the
canvas's backing size in sync via `ResizeObserver`, and tears the animation
down on unmount. `onReady(animation)` hands back the underlying
`An1mbyt3Animation` for anything the props don't cover directly
(`play`/`pause`/`seek`/`fireTrigger`/etc.). `stateMachine` is currently
informational only — see `@an1mbyt3/runtime`'s README for why.

## Building

```sh
npm run build   # tsc -b
```
