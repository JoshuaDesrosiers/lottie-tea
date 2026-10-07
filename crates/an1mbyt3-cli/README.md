# an1mbyt3-cli

Compiler/optimizer/validator/exporter/packager: `.tea` → `.teab`. See
AN1MBYT3_BUILD_PLAN.md §2, §7.

**Depends on:** `an1mbyt3-core`, `an1mbyt3-format`, `an1mbyt3-lottie`.
**Depended on by:** nothing (top-level binary).

## Status

Implemented (Phase 9, AN1MBYT3_BUILD_PLAN.md §10):

- `compile <input> [-o output]` — `.tea`/`.teab` -> `.teab`.
- `validate <input>` — loads either format, prints a summary.
- `import <input> [-o output]` — Lottie JSON -> `.teab` (or `.tea` if
  `output` ends in `.tea`), via `an1mbyt3-lottie`. See that crate's
  `import.rs` for exactly what's supported today.
- `optimize <input> [-o output] [-t tolerance]` — strips redundant
  keyframes (see `src/optimize.rs`'s module doc for the exact rule) and
  re-saves.

Not yet built: geometry dedup, string tables, unused-node stripping,
`export` (blocked on `an1mbyt3-lottie` growing an actual Lottie
*exporter* — only import exists), `package` (its meaning needs product
clarification — a `Document`'s font bytes already travel inline, so
there's no separate "bundle assets" step identified yet). Runtime-side
caching/batching/lazy-loading/GPU-memory optimization is unstarted
anywhere in the workspace.
