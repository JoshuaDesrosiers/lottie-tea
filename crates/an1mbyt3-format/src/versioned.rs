//! Schema-versioned document wrapper shared by both `.tea` (JSON, `tea.rs`)
//! and `.teab` (binary, `teab.rs`) formats — AN1MBYT3_BUILD_PLAN.md §4:
//! "Version every top-level document with a schema version integer... owns
//! migration functions between versions." Only version 1 exists so far
//! (this crate's first Phase 9 vertical slice) — there's nothing to migrate
//! *from* yet; `migrate_v1_to_v2` (etc.) get added here once a real
//! document-model change needs one, not speculatively ahead of time.

use serde::{Deserialize, Serialize};

use an1mbyt3_core::Document;

/// Bumped whenever a `Document`-reachable type's shape changes in a way that
/// would break decoding an older saved file — not on every commit, only on
/// actual breaking schema changes.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct VersionedDocument {
    pub schema_version: u32,
    pub document: Document,
}

impl VersionedDocument {
    pub(crate) fn wrap(document: &Document) -> Self {
        Self { schema_version: CURRENT_SCHEMA_VERSION, document: document.clone() }
    }
}

/// Returned when a loaded file's `schema_version` doesn't match
/// [`CURRENT_SCHEMA_VERSION`] and no migration path exists yet for it.
#[derive(Debug)]
pub struct UnsupportedSchemaVersion {
    pub found: u32,
    pub current: u32,
}

impl std::fmt::Display for UnsupportedSchemaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unsupported document schema version {} (this build only understands version {})",
            self.found, self.current
        )
    }
}

impl std::error::Error for UnsupportedSchemaVersion {}

pub(crate) fn check_schema_version(schema_version: u32) -> Result<(), UnsupportedSchemaVersion> {
    if schema_version != CURRENT_SCHEMA_VERSION {
        return Err(UnsupportedSchemaVersion { found: schema_version, current: CURRENT_SCHEMA_VERSION });
    }
    Ok(())
}
