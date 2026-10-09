//! Exact local bounds for `Text` nodes — what the selection box, hit
//! tests, layout and button bounds use (`dto::local_bounds`).
//!
//! A Text node has no authored outline: its glyphs are shaped from the
//! font at render time. `local_bounds` only sees an `Artboard` (no
//! `Document`, so no font bytes), which is why it used to *guess* from the
//! character count — a box that rarely matched the drawn text (too narrow
//! for wide fonts, centred on the baseline instead of on the glyphs).
//!
//! This module closes the gap with two small per-thread tables:
//! - `FONTS`: every font the renderer has parsed, registered as it
//!   populates its own `font_cache` (so any font that has ever drawn can
//!   be measured, even before a node using it has drawn);
//! - `MEASURED`: `(content, size, font) → bounds`, filled by the renderer
//!   (it measures every run it shapes anyway) and by `measure` on a miss.
//!
//! Bounds are in the node's local space, matching how the renderer places
//! glyphs: the run is centred horizontally on the origin, baseline at
//! `y = 0`. Both tables are cleared when a document is replaced (font ids
//! are only meaningful within one document).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use glam::Vec2;
use an1mbyt3_core::{FontId, TextNode};
use an1mbyt3_text::{shape_and_tessellate, Font};

type Key = (String, u32, FontId);

/// Entries kept before `MEASURED` is simply emptied (re-measuring is cheap
/// enough that an LRU isn't worth it; this only bounds memory).
const MAX_MEASURED: usize = 4096;

thread_local! {
    static FONTS: RefCell<HashMap<FontId, Rc<Font>>> = RefCell::new(HashMap::new());
    static MEASURED: RefCell<HashMap<Key, (Vec2, Vec2)>> = RefCell::new(HashMap::new());
}

fn key(content: &str, size_px: f32, font: FontId) -> Key {
    (content.to_owned(), size_px.to_bits(), font)
}

/// Makes `font` measurable (parses `bytes` once; later calls are free).
pub fn register_font(font: FontId, bytes: &[u8]) {
    FONTS.with(|fonts| {
        let mut fonts = fonts.borrow_mut();
        if fonts.contains_key(&font) {
            return;
        }
        if let Ok(parsed) = Font::from_bytes(bytes.to_vec(), 0) {
            fonts.insert(font, Rc::new(parsed));
        }
    });
}

/// A new document: font ids may now point at different fonts.
pub fn clear() {
    FONTS.with(|f| f.borrow_mut().clear());
    MEASURED.with(|m| m.borrow_mut().clear());
}

/// Stores bounds the renderer measured while shaping a run.
pub fn record(content: &str, size_px: f32, font: FontId, bounds: (Vec2, Vec2)) {
    MEASURED.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() >= MAX_MEASURED {
            m.clear();
        }
        m.insert(key(content, size_px, font), bounds);
    });
}

/// The bounds of a shaped run's glyph meshes, centred the way the renderer
/// draws it. A run with no visible glyph (blank, only spaces) still
/// occupies its advance, one size tall.
pub fn bounds_of(shaped: &an1mbyt3_text::ShapedText, size_px: f32) -> (Vec2, Vec2) {
    let recenter = Vec2::new(-shaped.advance.x / 2.0, 0.0);
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for g in &shaped.glyphs {
        for p in &g.mesh.positions {
            let p = Vec2::from(*p) + g.offset + recenter;
            min = min.min(p);
            max = max.max(p);
        }
    }
    if min.x <= max.x {
        (min, max)
    } else {
        (Vec2::new(-shaped.advance.x / 2.0, 0.0), Vec2::new(shaped.advance.x / 2.0, size_px))
    }
}

/// `text`'s exact local bounds, or `None` if its font hasn't been
/// registered yet (the caller falls back to an estimate; the next frame
/// registers it).
pub fn measure(text: &TextNode) -> Option<(Vec2, Vec2)> {
    let k = key(&text.content, text.size_px, text.font);
    if let Some(b) = MEASURED.with(|m| m.borrow().get(&k).copied()) {
        return Some(b);
    }
    let font = FONTS.with(|f| f.borrow().get(&text.font).cloned())?;
    let shaped = shape_and_tessellate(&font, &text.content, text.size_px);
    let bounds = bounds_of(&shaped, text.size_px);
    record(&text.content, text.size_px, text.font, bounds);
    Some(bounds)
}
