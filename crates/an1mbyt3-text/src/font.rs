//! Loaded font data.
//!
//! Owns raw font bytes and parses them twice on demand — once via
//! `rustybuzz::Face` for shaping (`shape.rs`), once via the re-exported
//! `rustybuzz::ttf_parser::Face` for glyph outline extraction (`outline.rs`).
//! Two independent parses of the same bytes rather than one shared `Face`
//! sounds wasteful, but avoids a self-referential struct (a `Face<'a>`
//! borrows from the byte slice it was parsed from, so storing both the bytes
//! and a `Face` borrowing them in the same struct needs unsafe or a crate
//! like `ouroboros`) — re-parsing a font's table directory is cheap relative
//! to shaping or outline extraction, so this isn't the bottleneck worth
//! avoiding here. Depending on `rustybuzz::ttf_parser` specifically (rather
//! than a separately-pinned `ttf-parser` dependency) guarantees the two
//! parses can never drift onto incompatible versions of the format.

/// Owned font file bytes (TTF/OTF), validated as parseable at construction.
pub struct Font {
    data: Vec<u8>,
    face_index: u32,
}

#[derive(Debug)]
pub enum FontError {
    /// Neither `rustybuzz::Face::from_slice` nor `ttf_parser::Face::parse`
    /// could parse `data` as a font.
    InvalidFontData,
}

impl Font {
    /// Loads a font from raw file bytes (e.g. `std::fs::read("font.ttf")`).
    /// `face_index` selects a face within a font collection (`.ttc`) — `0`
    /// for an ordinary single-face `.ttf`/`.otf`.
    pub fn from_bytes(data: Vec<u8>, face_index: u32) -> Result<Self, FontError> {
        rustybuzz::Face::from_slice(&data, face_index).ok_or(FontError::InvalidFontData)?;
        rustybuzz::ttf_parser::Face::parse(&data, face_index)
            .map_err(|_| FontError::InvalidFontData)?;
        Ok(Self { data, face_index })
    }

    pub(crate) fn shaping_face(&self) -> rustybuzz::Face<'_> {
        rustybuzz::Face::from_slice(&self.data, self.face_index)
            .expect("validated in Font::from_bytes")
    }

    pub(crate) fn outline_face(&self) -> rustybuzz::ttf_parser::Face<'_> {
        rustybuzz::ttf_parser::Face::parse(&self.data, self.face_index)
            .expect("validated in Font::from_bytes")
    }
}
