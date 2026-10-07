# apps/editor

The desktop editor: Tauri 2 shell + React/TypeScript UI, talking to the native
Rust engine crates over Tauri's IPC (commands + events) — not through WASM,
since the desktop build runs the real engine directly (AN1MBYT3_BUILD_PLAN.md §1).

Design direction (Rive's structure, Procreate's polish — floating panels, focus
mode, spring-based micro-transitions): see AN1MBYT3_BUILD_PLAN.md §15.

## Status

Not yet implemented — Phase 4 (AN1MBYT3_BUILD_PLAN.md §10). `cargo tauri init`
and the Vite/React scaffold happen when Phase 4 actually starts, not before —
Phases 1–3 (engine, timeline, text rendering) come first.
