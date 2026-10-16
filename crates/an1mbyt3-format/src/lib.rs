//! `an1mbyt3-format`: serialization, compression, versioning, migration.
//!
//! See AN1MBYT3_BUILD_PLAN.md §4 ("Serialization") and §7 ("File & Runtime
//! Pipeline") — owns both the human-readable `.tea` source format
//! ([`save_tea`]/[`load_tea`], `tea.rs`: `serde_json`, pretty-printed,
//! diff-friendly for git) and the compact `.teab` production format
//! ([`save_teab`]/[`load_teab`], `teab.rs`: `postcard` — see that module's
//! doc comment for why postcard over bincode/a custom flatbuffer layout).
//! Every document is wrapped with a schema version integer (`versioned.rs`)
//! before being encoded either way; this crate owns the `migrate_vN_to_vN+1`
//! functions that keep old files opening after model changes (none exist
//! yet — schema version 1 is the only one so far).
//!
//! Does NOT own: what the document model actually contains (see `an1mbyt3-core`,
//! `an1mbyt3-runtime`, `an1mbyt3-constraints` — this crate only knows how to
//! read/write/compress/migrate their `serde`-derived types). Does NOT own yet:
//! geometry deduplication, keyframe curve optimization, string-table building,
//! or byte-level compression (§7's compiler-pipeline steps beyond raw
//! encode/decode) — those are later Phase 9 increments on top of this one.
//!
//! Phase: Phase 9's first vertical slice (raw `.tea`/`.teab` encode/decode,
//! schema versioning) is implemented; the rest of Phase 9 (compaction,
//! caching, batching, lazy loading, memory/GPU optimization,
//! AN1MBYT3_BUILD_PLAN.md §10) is not yet.

mod project;
mod tea;
mod teab;
mod versioned;

pub use project::{load_project, project_asset_paths, save_project, ProjectAsset, ProjectAssetKind, ProjectContents, ProjectFile, ProjectLoadError, PROJECT_EXTENSION, PROJECT_FORMAT};
pub use tea::{load_tea, save_tea, TeaLoadError};
pub use teab::{load_teab, save_teab, TeabLoadError, TeabSaveError};
pub use versioned::{UnsupportedSchemaVersion, CURRENT_SCHEMA_VERSION};
pub mod lite;
