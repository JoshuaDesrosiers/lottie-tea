# an1mbyt3

A Rive-class interactive vector animation platform: Rust engine (document
model, bones/IK, state machines) + wgpu renderer + Tauri 2 desktop editor +
WASM web runtime + React/JS/HTML consumer APIs.

**Start here:** [AN1MBYT3_BUILD_PLAN.md](AN1MBYT3_BUILD_PLAN.md) — the living
architecture and build spec. Every crate/package below links back to the
relevant section instead of duplicating the design.

## Layout

```
crates/     Rust engine (10 crates — see AN1MBYT3_BUILD_PLAN.md §3)
apps/       Desktop editor (Tauri 2 + React) — §10 Phase 4, §15 design direction
packages/   Web runtime consumer packages (React + vanilla JS) — §8
```

## Building

```sh
cargo build --workspace   # Rust engine crates
npm install               # JS workspace (npm workspaces, see apps/* and packages/*)
```

Nothing is implemented yet — this is the Phase 0 scaffold. See §10 for the
phase-by-phase build order and exit criteria, §11 for the first concrete
milestone (the vertical slice).
