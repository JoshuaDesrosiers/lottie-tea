//! Drawings — artwork made in Paint mode (the vendored scratch-paint
//! editor) and saved as a document-level resource ("Save as Resource").
//!
//! A [`DrawingAsset`] keeps the drawing *itself*, not a conversion of it:
//!
//! - a **vector** drawing keeps scratch-paint's own SVG export, so it can be
//!   reopened in Paint and edited exactly as it was drawn (every stroke,
//!   color and group), and placed into a scene any number of times — each
//!   placement imports the SVG as ordinary Shape/Group nodes (the same path
//!   the Shapes menu's "Import SVG" uses), so a placed drawing is plain,
//!   fully editable scene content;
//! - a **bitmap** drawing (scratch-paint's bitmap mode) is a PNG, stored as a
//!   regular [`crate::ImageAsset`] the drawing points at — images already
//!   know how to live in a project's `Assets/Images` folder, be drawn as
//!   sprites, and be serialized in every format. Placing it adds an Image
//!   node.
//!
//! Why not the older routes: a [`crate::ComponentDefinition`] holds exactly
//! one path with one fill color, so anything with a second stroke or color
//! (nearly every real drawing) could not be saved at all; a prefab can't be
//! reopened in Paint.

use serde::{Deserialize, Serialize};

use crate::ids::ImageId;

/// One saved Paint-mode drawing — see this module's doc comment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingAsset {
    pub name: String,
    pub content: DrawingContent,
    /// Soft-delete flag, same reasoning as `PrefabDefinition::deleted`: undo
    /// needs the id to stay valid.
    #[serde(default)]
    pub deleted: bool,
}

/// What a drawing is made of. Externally tagged (serde's default), which
/// both JSON and postcard (`.teab`) handle — see `binary_safe_tagged!` for
/// why internal tags are off-limits in document types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DrawingContent {
    /// scratch-paint's SVG export, verbatim.
    Vector { svg: String },
    /// A bitmap-mode drawing, as the PNG image asset it was saved into.
    Bitmap { image: ImageId },
}
