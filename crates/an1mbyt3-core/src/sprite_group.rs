//! A purely organizational grouping of [`crate::ImageId`]s — how a decoded
//! GIF's frames stay together in the Resources panel without inventing a
//! special "animated image" node kind: each frame is a real, standalone
//! [`crate::ImageAsset`] usable anywhere any other image is, and a
//! `SpriteGroup` just remembers which ones belong together and in what
//! order.

use serde::{Deserialize, Serialize};

use crate::ids::ImageId;

/// One named set of ordered frames, e.g. one imported GIF's worth of
/// decoded [`crate::ImageAsset`]s. Frame order and per-frame delay live here
/// (not on `ImageAsset`) so membership has exactly one source of truth —
/// an `ImageAsset` never points back at its group, the same way
/// `crate::Artboard::children_of` resolves a group's children by scanning
/// rather than a redundant parent+children pair.
///
/// Frames-only for now: this is authoring/browsing metadata, not a
/// playback primitive — nothing in the runtime reads `frames` to actually
/// cycle an `Image` node through them yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpriteGroup {
    pub name: String,
    /// `(frame image, delay in milliseconds)`, in playback order.
    pub frames: Vec<(ImageId, u32)>,
}
