//! Reusable image asset — a document-level asset like [`crate::FontAsset`],
//! not a per-node field, because the same decoded image is typically
//! referenced by any number of `NodeKind::Image` nodes and because its raw
//! bytes need to travel *with* the document, the same self-contained-file
//! reasoning `FontAsset` already established.

use serde::{Deserialize, Serialize};

use crate::post_effect::PostEffect;

/// A loaded image's original encoded bytes (always PNG — `an1mbyt3-image`
/// re-encodes JPEG/GIF-frame sources to PNG at import time so every asset's
/// `source` decodes the same way) plus its authored pre-processing
/// parameters, owned by [`crate::Document`] and referenced by any number of
/// `NodeKind::Image` nodes via [`crate::ImageId`].
///
/// Deliberately holds *inputs* (encoded bytes + authored params), never the
/// decoded pixels or a baked/GPU texture — same "never store a derived
/// artifact in the document" discipline `crate::TextNode` already
/// established for shaped glyph meshes. The actual pixel processing
/// (`an1mbyt3_renderer::Renderer::bake_image`) runs once, outside the
/// document, and its result is cached by whatever's rendering (`WasmEditor`/
/// `WasmPlayer`'s `image_cache`), never serialized here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageAsset {
    pub name: String,
    pub source: Vec<u8>,
    #[serde(default)]
    pub processing: ImageProcessing,
    /// Inside a project folder: the image's own file, relative to the
    /// project (`Assets/Images/knight.png`). Set when the file was copied
    /// into (or dropped into) the project — that file on disk is then the
    /// source of truth and saves never rewrite it. `None` = an image that
    /// only lives in the document (a save writes it out as a new file).
    #[serde(default)]
    pub file: Option<String>,
}

/// Authored pre-processing for one [`ImageAsset`] — applied once by a
/// "bake" pass (`Renderer::bake_image`) rather than every render frame, so
/// editing these fields costs exactly one re-bake, not an ongoing per-frame
/// cost.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ImageProcessing {
    /// `None` = use the source image's own decoded pixel dimensions.
    pub resize: Option<(u32, u32)>,
    pub filter: ImageFilterMode,
    /// Reuses the existing whole-scene effect enum — `PostEffect::None` is
    /// the "no baked effect" identity value.
    pub effect: PostEffect,
}

/// Resample quality, used both for `ImageProcessing::resize` and for the
/// live GPU sampler an `Image` node draws through — a pixel-art asset wants
/// `Nearest` for both, a photo wants `Linear` for both, so one field covers
/// both rather than splitting into two settings that would usually just be
/// set to the same value anyway.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum ImageFilterMode {
    Nearest,
    #[default]
    Linear,
}
