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
/// why internal tags are off-limits in document types. Variants are only
/// ever appended (postcard encodes the variant index).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DrawingContent {
    /// scratch-paint's SVG export, verbatim (a still drawing from before
    /// frames and layers existed).
    Vector { svg: String },
    /// A bitmap-mode drawing, as the PNG image asset it was saved into
    /// (still, pre-frames).
    Bitmap { image: ImageId },
    /// Frames × layers, like a cel animation (Aseprite, Flash): every layer
    /// has one optional cel per frame. A still drawing is one frame; a
    /// single-layer drawing has one layer. What Paint saves now.
    Animation(DrawingAnimation),
}

/// See [`DrawingContent::Animation`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingAnimation {
    /// Playback speed, frames per second.
    pub fps: f32,
    pub frame_count: u32,
    /// Bottom to top. Every layer's `cels` has `frame_count` entries.
    pub layers: Vec<DrawingLayer>,
}

/// One layer of a [`DrawingAnimation`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingLayer {
    pub name: String,
    pub visible: bool,
    /// 0..=1, applied when the drawing is placed / composited.
    pub opacity: f32,
    /// One per frame; `None` = this layer is empty on that frame.
    pub cels: Vec<Option<DrawingCel>>,
}

/// One layer's artwork on one frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingCel {
    pub content: DrawingCelContent,
    /// scratch-paint's rotation center for this image — the registration
    /// point that lines every frame up: SVG user units for vector cels
    /// (scratch exports at half its 2x working scale), pixels for bitmap
    /// cels (scratch's bitmap mode works at 2x, so 2 px = 1 unit).
    pub center: [f32; 2],
}

/// See [`DrawingCel`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DrawingCelContent {
    Vector { svg: String },
    Bitmap { image: ImageId },
}

/// Shows one child at a time, cycling through them — a placed animated
/// drawing's frames (each child is one frame), or any hand-built sprite
/// flipbook. The node's children in order are the frames.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Flipbook {
    /// Frames per second.
    pub fps: f32,
    /// Starts playing on its own in Playground / exported games.
    pub playing: bool,
    /// Wraps around at the end (otherwise holds the last frame).
    pub looping: bool,
    /// The frame shown while not playing (and in Design), 0-based.
    pub frame: u32,
}

impl Default for Flipbook {
    fn default() -> Self {
        Self { fps: 12.0, playing: true, looping: true, frame: 0 }
    }
}

impl Flipbook {
    /// The frame (0-based) to show after `seconds` of playback out of
    /// `count` frames.
    pub fn frame_at(&self, seconds: f32, count: usize) -> usize {
        if count == 0 {
            return 0;
        }
        if !self.playing || self.fps <= 0.0 {
            return (self.frame as usize).min(count - 1);
        }
        let step = (seconds.max(0.0) * self.fps) as usize + self.frame as usize;
        if self.looping { step % count } else { step.min(count - 1) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flipbook_steps_loops_and_holds() {
        let fb = Flipbook { fps: 10.0, playing: true, looping: true, frame: 0 };
        assert_eq!(fb.frame_at(0.0, 4), 0);
        assert_eq!(fb.frame_at(0.25, 4), 2);
        assert_eq!(fb.frame_at(0.45, 4), 0, "wraps after 4 frames");
        let once = Flipbook { looping: false, ..fb };
        assert_eq!(once.frame_at(5.0, 4), 3, "holds the last frame");
        let paused = Flipbook { playing: false, frame: 2, ..fb };
        assert_eq!(paused.frame_at(9.0, 4), 2);
        assert_eq!(paused.frame_at(9.0, 0), 0);
    }
}
