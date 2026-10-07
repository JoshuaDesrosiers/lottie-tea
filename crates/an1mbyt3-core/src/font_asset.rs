//! Reusable font asset — a document-level asset like [`crate::ComponentDefinition`],
//! not a per-node field, because the same font is typically referenced by many
//! text nodes and because its raw bytes need to travel *with* the document:
//! a `.tea`/`.teab` file has to render its own text correctly on a machine
//! that never had the font installed (AN1MBYT3_BUILD_PLAN.md §10 Phase 3
//! exit criterion implies a saved document is self-contained, the same way a
//! saved component definition already is).

use serde::{Deserialize, Serialize};

/// A loaded font's raw file bytes (TTF/OTF) plus a display name, owned by
/// [`crate::Document`] and referenced by any number of `NodeKind::Text` nodes
/// via [`crate::FontId`]. Parsing/shaping (`rustybuzz`/`ttf-parser`) is
/// `an1mbyt3-text`'s job, not this crate's — `an1mbyt3-core` only stores
/// the bytes, the same way [`crate::Path`] stores geometry without knowing
/// how to tessellate itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontAsset {
    pub name: String,
    pub data: Vec<u8>,
}
