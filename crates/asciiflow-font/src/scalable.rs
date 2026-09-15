use crate::{FontError, GlyphAtlas};
use freetype::{Library, RenderMode, bitmap::PixelMode, face::LoadFlag};
use std::{
    path::Path,
    time::{Duration, Instant},
};
unsafe extern "C" {
    fn FT_Library_Version(
        library: freetype::ffi::FT_Library,
        major: *mut i32,
        minor: *mut i32,
        patch: *mut i32,
    );
}

#[derive(Debug)]
pub struct FontDiagnostics {
    pub family: String,
    pub style: String,
    pub version: (i32, i32, i32),
    pub pixel_size: u32,
    pub baseline: i32,
    pub load_wall: Duration,
    pub build_wall: Duration,
}

/// Initialization-only: all native objects are dropped before returning owned bytes.
pub fn build_font_atlas(
    path: &Path,
    face_index: isize,
    ramp: &str,
    width: u32,
    height: u32,
) -> Result<(GlyphAtlas, FontDiagnostics), FontError> {
    let error = |operation, detail: String| FontError::Font {
        path: path.display().to_string(),
        operation,
        detail,
    };
    let length = GlyphAtlas::checked_len(width, height, ramp.chars().count())?;
    // Bound raster work independently of the allocation bound.
    if width > 4096 || height > 4096 {
        return Err(FontError::AtlasTooLarge);
    }
    let started = Instant::now();
    use std::io::Read;
    const MAX_FONT_BYTES: u64 = 32 * 1024 * 1024;
    let file = std::fs::File::open(path).map_err(|e| error("read font file", e.to_string()))?;
    let mut bytes = Vec::new();
    file.take(MAX_FONT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| error("read font file", e.to_string()))?;
    if bytes.len() as u64 > MAX_FONT_BYTES {
        return Err(error("read font file", "font exceeds 32 MiB limit".into()));
    }
    let library = Library::init().map_err(|e| error("initialize FreeType", e.to_string()))?;
    let face = library
        .new_memory_face(bytes, face_index)
        .map_err(|e| error("load face", e.to_string()))?;
    if !face.is_scalable() {
        return Err(error("validate face", "a scalable face is required".into()));
    }
    if !face.is_fixed_width() {
        return Err(error(
            "validate face",
            "Stage 4.3 currently requires a monospaced font".into(),
        ));
    }
    let mut indices = Vec::new();
    for character in ramp.chars() {
        indices.push(face.get_char_index(character as usize).ok_or_else(|| {
            error(
                "lookup glyph",
                format!("missing glyph {character:?} (U+{:04X})", character as u32),
            )
        })?);
    }
    let load_wall = started.elapsed();
    let build_started = Instant::now();
    // Search one common ppem. Include bearings and the entire ramp in the fit,
    // rather than resizing individual glyphs or clipping normal descenders.
    let mut selected = None;
    for size in (1..=height).rev() {
        face.set_pixel_sizes(0, size)
            .map_err(|e| error("set pixel size", e.to_string()))?;
        let metrics = face
            .size_metrics()
            .ok_or_else(|| error("read size metrics", "missing metrics".into()))?;
        let mut left = 0i64;
        let mut right = (metrics.max_advance + 63) / 64;
        let mut top = (metrics.ascender + 63) / 64;
        let mut bottom = metrics.descender.div_euclid(64);
        for &index in &indices {
            face.load_glyph(index, LoadFlag::NO_BITMAP)
                .map_err(|e| error("load glyph outline", e.to_string()))?;
            face.glyph()
                .render_glyph(RenderMode::Normal)
                .map_err(|e| error("rasterize glyph", e.to_string()))?;
            let slot = face.glyph();
            left = left.min(i64::from(slot.bitmap_left()));
            right = right.max(i64::from(slot.bitmap_left()) + i64::from(slot.bitmap().width()));
            top = top.max(i64::from(slot.bitmap_top()));
            bottom = bottom.min(i64::from(slot.bitmap_top()) - i64::from(slot.bitmap().rows()));
        }
        if right > left
            && top > bottom
            && right - left <= i64::from(width)
            && top - bottom <= i64::from(height)
        {
            selected = Some((
                size,
                ((i64::from(width) - (right - left)) / 2 - left) as i32,
                ((i64::from(height) - (top - bottom)) / 2 + top) as i32,
            ));
            break;
        }
    }
    let (pixel_size, origin, baseline) = selected.ok_or_else(|| {
        error(
            "fit font metrics",
            format!("cell {width}x{height} is too small for a common font baseline"),
        )
    })?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(length)
        .map_err(|e| error("allocate atlas", e.to_string()))?;
    pixels.resize(length, 0);
    for (tile, (&index, character)) in indices.iter().zip(ramp.chars()).enumerate() {
        face.load_glyph(index, LoadFlag::NO_BITMAP)
            .map_err(|e| error("load glyph outline", e.to_string()))?;
        face.glyph()
            .render_glyph(RenderMode::Normal)
            .map_err(|e| error("rasterize glyph", e.to_string()))?;
        let slot = face.glyph();
        let bitmap = slot.bitmap();
        if character == ' ' || bitmap.width() == 0 || bitmap.rows() == 0 {
            continue;
        }
        let mode = bitmap
            .pixel_mode()
            .map_err(|e| error("read bitmap mode", e.to_string()))?;
        if !matches!(mode, PixelMode::Gray | PixelMode::Mono) {
            return Err(error(
                "validate bitmap mode",
                format!("unsupported pixel mode {mode:?}"),
            ));
        }
        let pitch = bitmap.pitch().unsigned_abs() as usize;
        let rows = bitmap.rows() as usize;
        let row_bytes = if mode == PixelMode::Mono {
            (bitmap.width() as usize).div_ceil(8)
        } else {
            bitmap.width() as usize
        };
        if pitch < row_bytes || bitmap.raw().buffer.is_null() {
            return Err(error(
                "validate bitmap storage",
                "invalid bitmap pitch/buffer".into(),
            ));
        }
        for y in 0..rows {
            // FreeType's allocation starts at the bottom row for negative pitch.
            let row_offset = bitmap_row_offset(y, rows, bitmap.pitch());
            let row = unsafe {
                std::slice::from_raw_parts(bitmap.raw().buffer.add(row_offset), row_bytes)
            };
            for x in 0..bitmap.width() as usize {
                let coverage = match mode {
                    PixelMode::Gray => row[x],
                    PixelMode::Mono => {
                        if row[x / 8] & (0x80 >> (x % 8)) != 0 {
                            255
                        } else {
                            0
                        }
                    }
                    _ => unreachable!(),
                };
                let dx = origin + slot.bitmap_left() + x as i32;
                let dy = baseline - slot.bitmap_top() + y as i32;
                if let Some(offset) = tile_offset(dx, dy, width, height) {
                    pixels[tile * width as usize * height as usize + offset] = coverage;
                }
            }
        }
    }
    let mut version = (0, 0, 0);
    // The wrapper exposes the owned library handle but has no version accessor.
    unsafe {
        FT_Library_Version(
            library.raw(),
            &mut version.0,
            &mut version.1,
            &mut version.2,
        );
    }
    let diagnostics = FontDiagnostics {
        family: face.family_name().unwrap_or_default(),
        style: face.style_name().unwrap_or_default(),
        version,
        pixel_size,
        baseline,
        load_wall,
        build_wall: build_started.elapsed(),
    };
    Ok((
        GlyphAtlas::from_r8(width, height, indices.len(), pixels)?,
        diagnostics,
    ))
}

fn bitmap_row_offset(y: usize, rows: usize, pitch: i32) -> usize {
    let row = if pitch < 0 { rows - 1 - y } else { y };
    row * pitch.unsigned_abs() as usize
}

fn tile_offset(x: i32, y: i32, width: u32, height: u32) -> Option<usize> {
    (x >= 0 && y >= 0 && x < width as i32 && y < height as i32)
        .then(|| y as usize * width as usize + x as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn negative_pitch_and_bearings_do_not_escape_the_tile() {
        assert_eq!(
            (0..3)
                .map(|y| bitmap_row_offset(y, 3, -4))
                .collect::<Vec<_>>(),
            vec![8, 4, 0]
        );
        assert_eq!(
            (0..3)
                .map(|y| bitmap_row_offset(y, 3, 4))
                .collect::<Vec<_>>(),
            vec![0, 4, 8]
        );
        assert_eq!(tile_offset(-1, 0, 24, 24), None);
        assert_eq!(tile_offset(0, -1, 24, 24), None);
        assert_eq!(tile_offset(24, 24, 24, 24), None);
        assert_eq!(tile_offset(2 - 3 + 2, 18 - 20 + 3, 24, 24), Some(25));
    }
    fn font() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/fonts/Inconsolata-Regular.ttf")
    }
    #[test]
    fn scalable_monospaced_atlas_has_uniform_baseline_grayscale_and_duplicate_identity() {
        let (atlas, metrics) = build_font_atlas(&font(), 0, "Ag#._  A", 24, 24).unwrap();
        assert_eq!(atlas.glyph_count(), 8);
        assert!(atlas.glyph(5).iter().all(|&v| v == 0));
        assert_eq!(atlas.glyph(0), atlas.glyph(7));
        assert!(atlas.as_r8_slice().iter().any(|&v| v > 0 && v < 255));
        let lower_ink = atlas
            .glyph(1)
            .chunks(24)
            .enumerate()
            .filter(|(_, row)| row.iter().any(|&v| v > 0))
            .map(|(y, _)| y)
            .max()
            .unwrap();
        assert!(
            lower_ink >= metrics.baseline as usize,
            "descender must survive"
        );
        let (again, _) = build_font_atlas(&font(), 0, "Ag#._  A", 24, 24).unwrap();
        assert_eq!(atlas.as_r8_slice(), again.as_r8_slice());
    }
    #[test]
    fn explicit_invalid_missing_and_missing_glyph_are_errors() {
        assert!(
            build_font_atlas(Path::new("/nonexistent/asciiflow-font.ttf"), 0, "A", 24, 24).is_err()
        );
        assert!(
            build_font_atlas(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("Cargo.toml")
                    .as_path(),
                0,
                "A",
                24,
                24
            )
            .unwrap_err()
            .to_string()
            .contains("load face")
        );
        let message = build_font_atlas(&font(), 0, "\u{10ffff}", 24, 24)
            .unwrap_err()
            .to_string();
        assert!(message.contains("U+10FFFF") && message.contains("Inconsolata"));
        assert!(build_font_atlas(&font(), 9000, "A", 24, 24).is_err());
    }
    #[test]
    fn geometry_is_bounded_before_native_rasterization() {
        for (w, h) in [(0, 24), (24, 0), (u32::MAX, u32::MAX), (8192, 8192)] {
            assert!(matches!(
                build_font_atlas(&font(), 0, "A", w, h),
                Err(FontError::AtlasTooLarge)
            ));
        }
        assert!(build_font_atlas(&font(), 0, "Ag", 1, 1).is_err());
    }
    #[test]
    fn proportional_font_is_explicitly_rejected() {
        let path = font().with_file_name("Abel-Regular.ttf");
        assert!(
            build_font_atlas(&path, 0, "A", 24, 24)
                .unwrap_err()
                .to_string()
                .contains("monospaced")
        );
    }
}
