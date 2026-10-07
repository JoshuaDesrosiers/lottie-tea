# an1mbyt3-format

Serialization for `.tea` (human-readable source) and `.teab` (compact binary
production format), plus schema versioning/migration. See AN1MBYT3_BUILD_PLAN.md §4, §7.

**Depends on:** `an1mbyt3-core`, `an1mbyt3-runtime`, `an1mbyt3-constraints`.
**Depended on by:** `an1mbyt3-wasm`, `an1mbyt3-cli`.

## Status

Not yet implemented — Phase 9 (AN1MBYT3_BUILD_PLAN.md §10), though the `.tea`
JSON path may land earlier alongside Phase 1/2 for save/load in the vertical slice (§11).
