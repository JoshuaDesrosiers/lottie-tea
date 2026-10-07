# an1mbyt3-wasm

Thin `wasm-bindgen` surface over the engine crates — no animation logic here,
only marshalling. Ensures desktop-authored and web-played animations never
diverge, since both run the same core crates. See AN1MBYT3_BUILD_PLAN.md §1, §8.

**Depends on:** `an1mbyt3-core`, `an1mbyt3-runtime`, `an1mbyt3-constraints`, `an1mbyt3-renderer`, `an1mbyt3-text`, `an1mbyt3-format`.
**Depended on by:** `packages/runtime-js`, `packages/runtime-react` (as the compiled `.wasm` + npm package, not a Cargo dependency).

## Status

Not yet implemented — Phase 8 (AN1MBYT3_BUILD_PLAN.md §10).
