//! Raster image import: PNG/JPEG/GIF bytes in, an [`ImportedImage`] out —
//! the raster counterpart to `an1mbyt3-svg`'s vector importer. Sibling crate,
//! same "one crate per import format" pattern, and deliberately has no
//! dependency on `an1mbyt3-core` — decode/re-encode is pure byte-in,
//! byte-and-dimensions-out plumbing with nothing document-model-shaped to
//! say, so a caller (`an1mbyt3-wasm` today, potentially `an1mbyt3-cli` later)
//! is left to turn the result into an `ImageAsset`/`NodeKind::Image` itself.
//!
//! A GIF decodes into [`ImportedImage::AnimatedFrames`] — one [`ImportedFrame`]
//! per frame, each independently usable as an ordinary image (an1mbyt3's
//! raster pivot treats "a GIF" as nothing more than a named, ordered set of
//! plain images, not a special node kind of its own). Every frame (and every
//! single PNG/JPEG import) is normalized to re-encoded PNG bytes, so
//! whatever's decoding later (`decode_to_rgba`, used at render/bake time)
//! only ever has to handle one format regardless of what was actually
//! imported.

use std::io::Cursor;

use image::{AnimationDecoder, ImageDecoder};

/// The result of importing one file's worth of raster image bytes.
#[derive(Debug, Clone)]
pub enum ImportedImage {
    /// A single still image (PNG/JPEG source, or any other still format
    /// `image` can decode) — `bytes` is always re-encoded PNG, regardless of
    /// the source format.
    Single { name: String, bytes: Vec<u8>, width: u32, height: u32 },
    /// A GIF's decoded frames, in playback order.
    AnimatedFrames { name: String, frames: Vec<ImportedFrame> },
}

/// One frame of an imported animated image — `bytes` is re-encoded PNG,
/// same normalization `ImportedImage::Single` applies.
#[derive(Debug, Clone)]
pub struct ImportedFrame {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub delay_ms: u32,
}

#[derive(Debug)]
pub enum ImageImportError {
    Decode(image::ImageError),
    Encode(image::ImageError),
    /// A GIF with zero frames — malformed input, not something a caller can
    /// usefully recover from.
    NoFrames,
}

impl std::fmt::Display for ImageImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageImportError::Decode(err) => write!(f, "failed to decode image: {err}"),
            ImageImportError::Encode(err) => write!(f, "failed to re-encode image: {err}"),
            ImageImportError::NoFrames => write!(f, "animated image has no frames"),
        }
    }
}

impl std::error::Error for ImageImportError {}

/// Re-encodes a decoded RGBA buffer as PNG bytes — the normalization step
/// every branch of [`import_image`] applies before returning.
fn encode_png(rgba: &image::RgbaImage) -> Result<Vec<u8>, ImageImportError> {
    let mut bytes = Vec::new();
    rgba.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .map_err(ImageImportError::Encode)?;
    Ok(bytes)
}

/// Imports one file's raw bytes. `filename_hint` is used only to derive a
/// display `name` (its extension plays no role in format detection — the
/// actual format is sniffed from the bytes themselves via
/// `image::guess_format`, which is more reliable than trusting a
/// user-supplied file extension).
pub fn import_image(filename_hint: &str, bytes: &[u8]) -> Result<ImportedImage, ImageImportError> {
    let name = std::path::Path::new(filename_hint)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename_hint)
        .to_string();

    let format = image::guess_format(bytes).map_err(ImageImportError::Decode)?;

    if format == image::ImageFormat::Gif {
        let decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).map_err(ImageImportError::Decode)?;
        let mut frames = Vec::new();
        for frame in decoder.into_frames() {
            let frame = frame.map_err(ImageImportError::Decode)?;
            let (numer, _denom) = frame.delay().numer_denom_ms();
            let rgba = frame.into_buffer();
            let (width, height) = rgba.dimensions();
            let bytes = encode_png(&rgba)?;
            frames.push(ImportedFrame { bytes, width, height, delay_ms: numer });
        }
        if frames.is_empty() {
            return Err(ImageImportError::NoFrames);
        }
        return Ok(ImportedImage::AnimatedFrames { name, frames });
    }

    let decoded = image::load_from_memory_with_format(bytes, format).map_err(ImageImportError::Decode)?;
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    let bytes = encode_png(&rgba)?;
    Ok(ImportedImage::Single { name, bytes, width, height })
}

/// Slices a horizontal sprite sheet (`bytes`: one row of equal-size frames,
/// left to right) into one [`ImportedImage::Single`] per frame — same
/// per-frame normalization (`encode_png`) every other import path here
/// applies, just repeated across a row instead of once for a whole image.
/// `frame_width`/`frame_height` are one frame's pixel size; the sheet's
/// total width is truncated down to the nearest whole multiple of
/// `frame_width` (a sheet with a few stray padding pixels on the right is a
/// malformed-but-recoverable case — dropping the remainder is simpler than
/// failing the whole import over it).
pub fn import_sprite_sheet(filename_hint: &str, bytes: &[u8], frame_width: u32, frame_height: u32) -> Result<Vec<ImportedImage>, ImageImportError> {
    let name = std::path::Path::new(filename_hint)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename_hint)
        .to_string();

    let decoded = image::load_from_memory(bytes).map_err(ImageImportError::Decode)?;
    let sheet = decoded.to_rgba8();
    let frame_count = sheet.width() / frame_width.max(1);

    let mut frames = Vec::with_capacity(frame_count as usize);
    for i in 0..frame_count {
        let cropped = image::imageops::crop_imm(&sheet, i * frame_width, 0, frame_width, frame_height).to_image();
        let frame_bytes = encode_png(&cropped)?;
        frames.push(ImportedImage::Single { name: format!("{name} {i}"), bytes: frame_bytes, width: frame_width, height: frame_height });
    }
    Ok(frames)
}

/// Full-grid counterpart to [`import_sprite_sheet`] — slices *every* row of
/// a sheet, not just the top one, the way Unity's Sprite Editor "Grid By
/// Cell Size" slice does. Frames come back in row-major order (left to
/// right across row 0, then row 1, ...) and are named
/// `"<stem> <index>"` with that same flat index, so a script can address
/// frame `(col, row)` as `row * columns + col` without any extra lookup.
///
/// Cells that are entirely transparent are *kept* (not skipped): skipping
/// them would shift every later frame's index and silently break that
/// `row * columns + col` addressing, which matters far more for a
/// direction-per-column character sheet (Ninja Adventure's `Walk.png`:
/// column = facing, row = step) than the few bytes an empty PNG costs.
///
/// Partial cells along the right/bottom edge are dropped, same truncation
/// rule `import_sprite_sheet` applies horizontally.
pub fn import_sprite_grid(filename_hint: &str, bytes: &[u8], frame_width: u32, frame_height: u32) -> Result<Vec<ImportedImage>, ImageImportError> {
    let name = std::path::Path::new(filename_hint)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename_hint)
        .to_string();

    let decoded = image::load_from_memory(bytes).map_err(ImageImportError::Decode)?;
    let sheet = decoded.to_rgba8();
    let columns = sheet.width() / frame_width.max(1);
    let rows = sheet.height() / frame_height.max(1);

    let mut frames = Vec::with_capacity((columns * rows) as usize);
    for row in 0..rows {
        for col in 0..columns {
            let cropped = image::imageops::crop_imm(&sheet, col * frame_width, row * frame_height, frame_width, frame_height).to_image();
            let frame_bytes = encode_png(&cropped)?;
            let index = row * columns + col;
            frames.push(ImportedImage::Single { name: format!("{name} {index}"), bytes: frame_bytes, width: frame_width, height: frame_height });
        }
    }
    Ok(frames)
}

/// Crops `rects` (`[x, y, width, height]` in pixels, clamped to the image)
/// out of an image — the Sprite Editor's "Slice": each comes back as its
/// own RGBA PNG with its size. Empty / off-image rects are skipped.
pub fn crop_rects(bytes: &[u8], rects: &[[u32; 4]]) -> Result<Vec<(Vec<u8>, u32, u32)>, ImageImportError> {
    let sheet = image::load_from_memory(bytes).map_err(ImageImportError::Decode)?.to_rgba8();
    let (sw, sh) = sheet.dimensions();
    let mut out = Vec::with_capacity(rects.len());
    for r in rects {
        let (x, y) = (r[0].min(sw), r[1].min(sh));
        let (w, h) = (r[2].min(sw - x), r[3].min(sh - y));
        if w == 0 || h == 0 {
            continue;
        }
        let cropped = image::imageops::crop_imm(&sheet, x, y, w, h).to_image();
        out.push((encode_png(&cropped)?, w, h));
    }
    Ok(out)
}

/// Unity's "Automatic" slice: every separate sprite on a sheet, found as a
/// group of touching opaque pixels (alpha above `alpha_min`, 8-connected),
/// returned as bounding boxes `[x, y, width, height]`. Boxes smaller than
/// `min_size` on both sides (stray specks) are dropped, and boxes that
/// overlap or sit within `merge_gap` pixels of each other are merged (a
/// sprite whose parts don't quite touch — a floating eye, a sword tip).
/// Ordered in reading order: top to bottom by row, then left to right.
pub fn alpha_islands(bytes: &[u8], alpha_min: u8, min_size: u32, merge_gap: u32) -> Result<Vec<[u32; 4]>, ImageImportError> {
    let sheet = image::load_from_memory(bytes).map_err(ImageImportError::Decode)?.to_rgba8();
    let (w, h) = sheet.dimensions();
    let (wu, hu) = (w as usize, h as usize);
    let solid: Vec<bool> = sheet.pixels().map(|p| p.0[3] > alpha_min).collect();
    let mut seen = vec![false; wu * hu];
    let mut boxes: Vec<[u32; 4]> = Vec::new();
    let mut stack = Vec::new();
    for start in 0..wu * hu {
        if !solid[start] || seen[start] {
            continue;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
        seen[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % wu, i / wu);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let j = ny as usize * wu + nx as usize;
                    if solid[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        boxes.push([x0 as u32, y0 as u32, (x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32]);
    }
    // Merge boxes that overlap or nearly touch, until nothing changes.
    let gap = merge_gap as i64;
    let near = |a: &[u32; 4], b: &[u32; 4]| {
        let (ax0, ay0, ax1, ay1) = (a[0] as i64, a[1] as i64, (a[0] + a[2]) as i64, (a[1] + a[3]) as i64);
        let (bx0, by0, bx1, by1) = (b[0] as i64, b[1] as i64, (b[0] + b[2]) as i64, (b[1] + b[3]) as i64);
        ax0 <= bx1 + gap && bx0 <= ax1 + gap && ay0 <= by1 + gap && by0 <= ay1 + gap
    };
    let mut merged = true;
    while merged {
        merged = false;
        'outer: for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                if near(&boxes[i], &boxes[j]) {
                    let (a, b) = (boxes[i], boxes[j]);
                    let x0 = a[0].min(b[0]);
                    let y0 = a[1].min(b[1]);
                    let x1 = (a[0] + a[2]).max(b[0] + b[2]);
                    let y1 = (a[1] + a[3]).max(b[1] + b[3]);
                    boxes[i] = [x0, y0, x1 - x0, y1 - y0];
                    boxes.swap_remove(j);
                    merged = true;
                    break 'outer;
                }
            }
        }
    }
    boxes.retain(|b| b[2] >= min_size || b[3] >= min_size);
    // Reading order: group into rows by vertical overlap, then left to right.
    boxes.sort_by_key(|b| (b[1], b[0]));
    let mut rows: Vec<Vec<[u32; 4]>> = Vec::new();
    for b in boxes {
        match rows.last_mut() {
            Some(row) if row.iter().any(|r| b[1] < r[1] + r[3] && r[1] < b[1] + b[3]) => row.push(b),
            _ => rows.push(vec![b]),
        }
    }
    let mut out = Vec::new();
    for mut row in rows {
        row.sort_by_key(|b| b[0]);
        out.extend(row);
    }
    Ok(out)
}

#[cfg(test)]
mod slice_tests {
    use super::*;

    fn sheet_png() -> Vec<u8> {
        // 20x10: two opaque squares (2..6, 2..6) and (12..18, 3..9), one speck at (10, 0).
        let mut img = image::RgbaImage::new(20, 10);
        for y in 2..6 {
            for x in 2..6 {
                img.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }
        for y in 3..9 {
            for x in 12..18 {
                img.put_pixel(x, y, image::Rgba([0, 255, 0, 255]));
            }
        }
        img.put_pixel(10, 0, image::Rgba([0, 0, 255, 255]));
        encode_png(&img).unwrap()
    }

    #[test]
    fn finds_islands_in_reading_order_and_drops_specks() {
        let boxes = alpha_islands(&sheet_png(), 0, 2, 0).unwrap();
        assert_eq!(boxes, vec![[2, 2, 4, 4], [12, 3, 6, 6]]);
    }

    #[test]
    fn crops_rects_to_their_size() {
        let crops = crop_rects(&sheet_png(), &[[2, 2, 4, 4], [15, 5, 100, 100]]).unwrap();
        assert_eq!(crops.iter().map(|c| (c.1, c.2)).collect::<Vec<_>>(), vec![(4, 4), (5, 5)]);
    }
}

/// Reads just a PNG's `IHDR` chunk for its pixel dimensions, without
/// decompressing any pixel data — the cheap alternative to `decode_to_rgba`
/// for call sites that only need to know how big an asset is (the Resources
/// panel/Inspector's summary DTO, built fresh after every document edit)
/// rather than its actual pixels.
pub fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32), ImageImportError> {
    let decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes)).map_err(ImageImportError::Decode)?;
    Ok(decoder.dimensions())
}

/// Decodes any `ImageAsset::source`/`ImportedFrame::bytes` (always PNG,
/// per this crate's own normalization) back into raw RGBA8 pixels — what a
/// render/bake step (`an1mbyt3_renderer::Renderer::bake_image`) needs before
/// it can upload a texture.
pub fn decode_to_rgba(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), ImageImportError> {
    let decoder = image::codecs::png::PngDecoder::new(Cursor::new(bytes)).map_err(ImageImportError::Decode)?;
    let (width, height) = decoder.dimensions();
    let mut buf = vec![0u8; decoder.total_bytes() as usize];
    decoder.read_image(&mut buf).map_err(ImageImportError::Decode)?;
    // `read_image` writes whatever `ColorType` the PNG actually has (this
    // crate always re-encodes to RGBA8 on the way in, but decoding directly
    // via `PngDecoder` — rather than the more permissive
    // `image::load_from_memory`, which would auto-convert — keeps this
    // function's contract narrow and cheap: it only ever has to handle the
    // one color type this crate itself produces).
    Ok((buf, width, height))
}

/// Decodes *any* PNG / JPEG (palette, grey, RGB, 16-bit…) to RGBA8 — for
/// files this crate didn't write itself, like an imported 3D model's
/// textures (Kenney's are palette PNGs, which [`decode_to_rgba`]'s narrow
/// "RGBA8 PNG only" contract would return as raw RGB).
pub fn decode_any_to_rgba(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), ImageImportError> {
    let img = image::load_from_memory(bytes).map_err(ImageImportError::Decode)?.to_rgba8();
    let (width, height) = img.dimensions();
    Ok((img.into_raw(), width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_any_expands_palette_pngs_to_rgba() {
        let png = include_bytes!("../../an1mbyt3-wasm/assets/kenney/characters/Textures/texture-a.png");
        let (rgba, w, h) = decode_any_to_rgba(png).unwrap();
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        assert!(rgba.chunks(4).any(|p| p[3] == 255), "opaque pixels");
    }

    fn tiny_png(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(width, height, |x, y| image::Rgba([x as u8, y as u8, 0, 255]));
        let mut bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        bytes
    }

    fn tiny_jpeg(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(width, height, |x, y| image::Rgba([x as u8, y as u8, 0, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(img).write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Jpeg).unwrap();
        bytes
    }

    fn tiny_gif(width: u32, height: u32, frame_count: usize) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(Cursor::new(&mut bytes));
            for i in 0..frame_count {
                let img = image::RgbaImage::from_fn(width, height, |x, _y| image::Rgba([x as u8, i as u8 * 50, 0, 255]));
                let frame = image::Frame::from_parts(img, 0, 0, image::Delay::from_numer_denom_ms(80, 1));
                encoder.encode_frame(frame).unwrap();
            }
        }
        bytes
    }

    #[test]
    fn imports_a_single_png_with_correct_dimensions() {
        let bytes = tiny_png(4, 3);
        let imported = import_image("sprite.png", &bytes).unwrap();
        let ImportedImage::Single { name, width, height, .. } = imported else { panic!("expected Single") };
        assert_eq!(name, "sprite");
        assert_eq!((width, height), (4, 3));
    }

    #[test]
    fn imports_a_single_jpeg_with_correct_dimensions() {
        let bytes = tiny_jpeg(5, 2);
        let imported = import_image("photo.jpg", &bytes).unwrap();
        let ImportedImage::Single { width, height, .. } = imported else { panic!("expected Single") };
        assert_eq!((width, height), (5, 2));
    }

    #[test]
    fn imports_a_gif_into_multiple_frames_with_delays() {
        let bytes = tiny_gif(3, 3, 2);
        let imported = import_image("wave.gif", &bytes).unwrap();
        let ImportedImage::AnimatedFrames { name, frames } = imported else { panic!("expected AnimatedFrames") };
        assert_eq!(name, "wave");
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].delay_ms, 80);
        assert_eq!((frames[0].width, frames[0].height), (3, 3));
    }

    #[test]
    fn decode_to_rgba_round_trips_a_single_imports_bytes() {
        let bytes = tiny_png(4, 3);
        let imported = import_image("sprite.png", &bytes).unwrap();
        let ImportedImage::Single { bytes: png_bytes, width, height, .. } = imported else { panic!("expected Single") };

        let (rgba, decoded_width, decoded_height) = decode_to_rgba(&png_bytes).unwrap();
        assert_eq!((decoded_width, decoded_height), (width, height));
        assert_eq!(rgba.len(), (width * height * 4) as usize);
    }

    #[test]
    fn png_dimensions_reads_the_header_without_decoding_pixels() {
        let bytes = tiny_png(7, 5);
        assert_eq!(png_dimensions(&bytes).unwrap(), (7, 5));
    }

    #[test]
    fn rejects_garbage_bytes() {
        let result = import_image("not-an-image.png", b"definitely not an image");
        assert!(result.is_err());
    }

    #[test]
    fn slices_a_sprite_sheet_into_equal_frames() {
        // 3 frames, each 2x2, laid out left to right (6x2 overall) — the
        // leftmost column of each frame is tinted by the frame index so the
        // crop boundaries are easy to assert on.
        let sheet = tiny_png(6, 2);
        let frames = import_sprite_sheet("walk.png", &sheet, 2, 2).unwrap();
        assert_eq!(frames.len(), 3);
        for (i, frame) in frames.iter().enumerate() {
            let ImportedImage::Single { name, width, height, .. } = frame else { panic!("expected Single") };
            assert_eq!(*width, 2);
            assert_eq!(*height, 2);
            assert_eq!(name, &format!("walk {i}"));
        }
    }

    #[test]
    fn sprite_sheet_drops_a_partial_trailing_frame() {
        // 7px wide with 3px frames: only 2 whole frames fit, the last 1px is
        // dropped rather than erroring.
        let sheet = tiny_png(7, 3);
        let frames = import_sprite_sheet("odd.png", &sheet, 3, 3).unwrap();
        assert_eq!(frames.len(), 2);
    }

    #[test]
    fn sprite_grid_slices_every_row_in_row_major_order() {
        // 3 columns x 2 rows of 2x2 cells, plus a stray 1px row along the
        // bottom that must be dropped, not turned into a third partial row.
        let sheet = tiny_png(6, 5);
        let frames = import_sprite_grid("walk.png", &sheet, 2, 2).unwrap();
        assert_eq!(frames.len(), 6);
        for (i, frame) in frames.iter().enumerate() {
            let ImportedImage::Single { name, width, height, .. } = frame else { panic!("expected Single") };
            assert_eq!((*width, *height), (2, 2));
            assert_eq!(name, &format!("walk {i}"));
        }
    }
}
