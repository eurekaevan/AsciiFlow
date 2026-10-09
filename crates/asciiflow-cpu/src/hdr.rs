//! Permanent f64 PQ CPU oracle, not a selectable production backend.

use asciiflow_core::hdr_pq::{self, LinearRgb, P010Codes, PqRgb};
use asciiflow_core::{
    ChromaLocation, ColorMatrix, ColorPrimaries, ColorRange, Error, FrameDesc, HostFrame,
    PixelFormat, Result, TransferCharacteristic, VideoFrame, glyph_lookup_table,
};
use asciiflow_font::GlyphAtlas;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HdrCell {
    pub glyph: u16,
    pub foreground_nits: LinearRgb,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HdrCellGrid {
    pub width: u32,
    pub height: u32,
    pub cells: Vec<HdrCell>,
    pub clamped_input_components: u64,
}

/// Post-glyph display-light BT.2020 image, before PQ encoding or chroma subsampling.
/// The source descriptor is validated by the renderer; this is not a P010 buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct HdrRenderedLinearFrame {
    width: u32,
    height: u32,
    pixels: Vec<LinearRgb>,
    coverage: Vec<u8>,
}

impl HdrRenderedLinearFrame {
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn pixels(&self) -> &[LinearRgb] {
        &self.pixels
    }
    pub fn coverage(&self) -> &[u8] {
        &self.coverage
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HdrDiagnostics {
    pub min_luminance_nits: f64,
    pub max_luminance_nits: f64,
    pub min_component_nits: f64,
    pub max_component_nits: f64,
    pub clamped_components: u64,
    pub nonfinite_components: u64,
}

/// Chroma is reused from the containing 2x2 4:2:0 block (left/cosited test
/// geometry). No claim is made for unknown or center chroma siting.
pub struct HdrPqReference<'a> {
    atlas: &'a GlyphAtlas,
    glyph_lut: [u32; 256],
}

impl<'a> HdrPqReference<'a> {
    pub fn new(atlas: &'a GlyphAtlas, charset: &str) -> Result<Self> {
        let count = charset.chars().count();
        if count == 0 || count > u16::MAX as usize || count != atlas.glyph_count() {
            return Err(Error::Cpu("HDR atlas and charset identities differ".into()));
        }
        Ok(Self {
            atlas,
            glyph_lut: glyph_lookup_table(count),
        })
    }

    pub fn process(
        &self,
        input: &VideoFrame,
        grid_width: u32,
        grid_height: u32,
        color: bool,
    ) -> Result<(VideoFrame, HdrDiagnostics)> {
        let grid = self.map(input, grid_width, grid_height)?;
        self.render(&grid, input.desc().clone(), input.pts(), color)
    }

    pub fn map(
        &self,
        input: &VideoFrame,
        grid_width: u32,
        grid_height: u32,
    ) -> Result<HdrCellGrid> {
        let desc = input.desc();
        validate_desc(desc)?;
        if grid_width == 0
            || grid_height == 0
            || grid_width > desc.width
            || grid_height > desc.height
        {
            return Err(Error::Cpu("HDR cell grid must fit the frame".into()));
        }
        let (y_plane, uv_plane) = input.host().planes(desc);
        let width = desc.width as usize;
        let count = (grid_width as usize)
            .checked_mul(grid_height as usize)
            .ok_or_else(|| Error::Cpu("HDR cell count overflow".into()))?;
        let mut cells = Vec::with_capacity(count);
        let mut clamped_input_components = 0u64;
        for cy in 0..grid_height {
            let y0 = cy as usize * desc.height as usize / grid_height as usize;
            let y1 = ((cy + 1) as usize * desc.height as usize / grid_height as usize).max(y0 + 1);
            for cx in 0..grid_width {
                let x0 = cx as usize * width / grid_width as usize;
                let x1 = ((cx + 1) as usize * width / grid_width as usize).max(x0 + 1);
                let mut sum = LinearRgb::BLACK;
                for y in y0..y1 {
                    for x in x0..x1 {
                        let uv_index = (y / 2) * width + (x & !1);
                        let codes = P010Codes {
                            y: read_code(y_plane, y * width + x)?,
                            cb: read_code(uv_plane, uv_index)?,
                            cr: read_code(uv_plane, uv_index + 1)?,
                        };
                        let encoded = hdr_pq::ycbcr_to_pq(
                            hdr_pq::decode_limited(codes).map_err(pixel_error)?,
                        );
                        // Legal YCbCr can reconstruct RGB outside [0,1]. The oracle
                        // clamps those components before PQ to avoid invalid luminance.
                        let (encoded, clamped) = clamp_pq(encoded);
                        clamped_input_components += u64::from(clamped);
                        let linear = hdr_pq::pq_to_linear(encoded).map_err(pixel_error)?;
                        sum.r += linear.r;
                        sum.g += linear.g;
                        sum.b += linear.b;
                    }
                }
                let pixels = ((x1 - x0) * (y1 - y0)) as f64;
                let foreground_nits = LinearRgb {
                    r: sum.r / pixels,
                    g: sum.g / pixels,
                    b: sum.b / pixels,
                };
                let perceptual =
                    hdr_pq::pq_inverse_eotf(foreground_nits.luminance()).map_err(pixel_error)?;
                // The existing 256-entry SDR S-curve is reused only to choose a
                // glyph; its limited-range index mapping never alters RGB light.
                let index = (16.0 + 219.0 * perceptual).round().clamp(0.0, 255.0) as usize;
                cells.push(HdrCell {
                    glyph: self.glyph_lut[index] as u16,
                    foreground_nits,
                });
            }
        }
        Ok(HdrCellGrid {
            width: grid_width,
            height: grid_height,
            cells,
            clamped_input_components,
        })
    }

    pub fn render(
        &self,
        grid: &HdrCellGrid,
        desc: FrameDesc,
        pts: Option<i64>,
        color: bool,
    ) -> Result<(VideoFrame, HdrDiagnostics)> {
        self.validate_grid(grid, &desc)?;
        let mut storage = HostFrame::try_new_zeroed(&desc)?;
        let (y_plane, uv_plane) = storage.planes_mut(&desc);
        let width = desc.width as usize;
        let height = desc.height as usize;
        let mut diag = HdrDiagnostics {
            min_luminance_nits: f64::INFINITY,
            min_component_nits: f64::INFINITY,
            clamped_components: grid.clamped_input_components,
            ..HdrDiagnostics::default()
        };
        for y in (0..height).step_by(2) {
            for x in (0..width).step_by(2) {
                let mut cb = 0.0;
                let mut cr = 0.0;
                for dy in 0..2 {
                    for dx in 0..2 {
                        let py = y + dy;
                        let px = x + dx;
                        let (linear, _) = self.rendered_pixel(grid, px, py, width, height, color);
                        for component in [linear.r, linear.g, linear.b] {
                            if !component.is_finite() {
                                diag.nonfinite_components += 1;
                            }
                            diag.min_component_nits = diag.min_component_nits.min(component);
                            diag.max_component_nits = diag.max_component_nits.max(component);
                        }
                        let luminance = linear.luminance();
                        diag.min_luminance_nits = diag.min_luminance_nits.min(luminance);
                        diag.max_luminance_nits = diag.max_luminance_nits.max(luminance);
                        let pq = hdr_pq::linear_to_pq(linear).map_err(pixel_error)?;
                        let (pq, clipped) = clamp_pq(pq);
                        diag.clamped_components += u64::from(clipped);
                        let ycbcr = hdr_pq::pq_to_ycbcr(pq);
                        let (codes, clipped) =
                            hdr_pq::encode_limited(ycbcr).map_err(pixel_error)?;
                        diag.clamped_components += u64::from(clipped);
                        write_code(y_plane, py * width + px, codes.y);
                        cb += ycbcr.cb;
                        cr += ycbcr.cr;
                    }
                }
                let (codes, clipped) = hdr_pq::encode_limited(hdr_pq::Ycbcr {
                    y: 0.0,
                    cb: cb / 4.0,
                    cr: cr / 4.0,
                })
                .map_err(pixel_error)?;
                diag.clamped_components += u64::from(clipped);
                let uv_index = (y / 2) * width + x;
                write_code(uv_plane, uv_index, codes.cb);
                write_code(uv_plane, uv_index + 1, codes.cr);
            }
        }
        Ok((VideoFrame::new_host(desc, pts, storage)?, diag))
    }

    /// Expose exactly the existing post-blend/pre-PQ boundary for internal
    /// references. Glyph aggregation, LUT, coverage and blend are shared with
    /// `render`, not copied into a tone-mapping renderer.
    pub fn render_linear(
        &self,
        grid: &HdrCellGrid,
        desc: &FrameDesc,
        color: bool,
    ) -> Result<HdrRenderedLinearFrame> {
        self.validate_grid(grid, desc)?;
        let width = desc.width as usize;
        let height = desc.height as usize;
        let count = width
            .checked_mul(height)
            .ok_or_else(|| Error::Cpu("HDR linear frame size overflow".into()))?;
        let mut pixels = Vec::new();
        let mut coverage = Vec::new();
        pixels
            .try_reserve_exact(count)
            .map_err(|e| Error::Cpu(format!("allocate HDR linear frame: {e}")))?;
        coverage
            .try_reserve_exact(count)
            .map_err(|e| Error::Cpu(format!("allocate HDR coverage: {e}")))?;
        for py in 0..height {
            for px in 0..width {
                let (linear, alpha) = self.rendered_pixel(grid, px, py, width, height, color);
                pixels.push(linear);
                coverage.push(alpha);
            }
        }
        Ok(HdrRenderedLinearFrame {
            width: desc.width,
            height: desc.height,
            pixels,
            coverage,
        })
    }

    fn validate_grid(&self, grid: &HdrCellGrid, desc: &FrameDesc) -> Result<()> {
        validate_desc(desc)?;
        if grid.width == 0
            || grid.height == 0
            || grid.width > desc.width
            || grid.height > desc.height
            || grid.cells.len() != grid.width as usize * grid.height as usize
            || grid
                .cells
                .iter()
                .any(|c| c.glyph as usize >= self.atlas.glyph_count())
        {
            return Err(Error::Cpu("invalid HDR cell grid".into()));
        }
        Ok(())
    }

    fn rendered_pixel(
        &self,
        grid: &HdrCellGrid,
        px: usize,
        py: usize,
        width: usize,
        height: usize,
        color: bool,
    ) -> (LinearRgb, u8) {
        let cell = grid.cells[cell_at(py, height, grid.height as usize) * grid.width as usize
            + cell_at(px, width, grid.width as usize)];
        let foreground = if color {
            cell.foreground_nits
        } else {
            let l = cell.foreground_nits.luminance();
            LinearRgb { r: l, g: l, b: l }
        };
        let coverage = self.coverage(cell.glyph, px, py, width, height, grid);
        (foreground.blend(LinearRgb::BLACK, coverage), coverage)
    }

    fn coverage(
        &self,
        glyph: u16,
        px: usize,
        py: usize,
        width: usize,
        height: usize,
        grid: &HdrCellGrid,
    ) -> u8 {
        let aw = self.atlas.width() as usize;
        let ah = self.atlas.height() as usize;
        let gx = atlas_at(px, width, grid.width as usize, aw);
        let gy = atlas_at(py, height, grid.height as usize, ah);
        self.atlas.glyph(glyph as usize)[gy * aw + gx]
    }
}

/// Inverse of the floor-boundary partition used by `map`: cell i owns
/// [floor(i * extent / count), floor((i + 1) * extent / count)).
fn cell_at(pixel: usize, extent: usize, count: usize) -> usize {
    ((pixel + 1) * count - 1) / extent
}

fn atlas_at(pixel: usize, extent: usize, count: usize, atlas_extent: usize) -> usize {
    let cell = cell_at(pixel, extent, count);
    let start = cell * extent / count;
    let end = (cell + 1) * extent / count;
    (pixel - start) * atlas_extent / (end - start)
}

fn validate_desc(desc: &FrameDesc) -> Result<()> {
    desc.validate_layout()?;
    let c = desc.color_space;
    if desc.format != PixelFormat::P010Le
        || c.primaries != ColorPrimaries::Bt2020
        || c.matrix != ColorMatrix::Bt2020
        || c.transfer != TransferCharacteristic::Pq
        || c.range != ColorRange::Limited
        || c.chroma_location != ChromaLocation::Left
    {
        return Err(Error::Cpu(
            "HDR CPU oracle requires left-sited limited BT.2020/PQ P010LE".into(),
        ));
    }
    Ok(())
}

fn pixel_error(error: hdr_pq::PixelError) -> Error {
    Error::Cpu(error.to_string())
}

fn clamp_pq(v: PqRgb) -> (PqRgb, u32) {
    let components = [v.r, v.g, v.b];
    let clipped = components
        .into_iter()
        .filter(|&c| !(0.0..=1.0).contains(&c))
        .count() as u32;
    (
        PqRgb {
            r: v.r.clamp(0.0, 1.0),
            g: v.g.clamp(0.0, 1.0),
            b: v.b.clamp(0.0, 1.0),
        },
        clipped,
    )
}

fn read_code(plane: &[u8], index: usize) -> Result<u16> {
    let offset = index * 2;
    let word = u16::from_le_bytes([plane[offset], plane[offset + 1]]);
    if word & 0x3f != 0 {
        return Err(Error::Cpu("P010 padding bits are nonzero".into()));
    }
    Ok(word >> 6)
}

fn write_code(plane: &mut [u8], index: usize, code: u16) {
    plane[index * 2..index * 2 + 2].copy_from_slice(&(code << 6).to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use asciiflow_core::ColorSpace;

    fn hdr_desc(width: u32, height: u32) -> FrameDesc {
        FrameDesc::host_p010_le(
            width,
            height,
            ColorSpace {
                matrix: ColorMatrix::Bt2020,
                range: ColorRange::Limited,
                primaries: ColorPrimaries::Bt2020,
                transfer: TransferCharacteristic::Pq,
                chroma_location: ChromaLocation::Left,
            },
        )
        .unwrap()
    }

    fn neutral_frame(codes: &[u16], width: u32, height: u32) -> VideoFrame {
        let desc = hdr_desc(width, height);
        let mut storage = HostFrame::new_zeroed(&desc);
        let (y, uv) = storage.planes_mut(&desc);
        for (i, &code) in codes.iter().enumerate() {
            write_code(y, i, code);
        }
        for i in 0..uv.len() / 2 {
            write_code(uv, i, 512);
        }
        VideoFrame::new_host(desc, Some(42), storage).unwrap()
    }

    #[test]
    fn neutral_black_and_100_1000_nit_pipeline() {
        let values = [0.0, 100.0, 1000.0, 10_000.0];
        let mut codes = Vec::new();
        for &nits in &values {
            let pq = hdr_pq::pq_inverse_eotf(nits).unwrap();
            let (code, _) = hdr_pq::encode_limited(hdr_pq::pq_to_ycbcr(PqRgb {
                r: pq,
                g: pq,
                b: pq,
            }))
            .unwrap();
            codes.extend([code.y; 4]);
        }
        let mut raster = Vec::new();
        for row in 0..2 {
            for patch in 0..4 {
                raster.extend(&codes[patch * 4 + row * 2..patch * 4 + row * 2 + 2]);
            }
        }
        let frame = neutral_frame(&raster, 8, 2);
        let atlas = GlyphAtlas::from_r8(1, 1, 1, vec![255]).unwrap();
        let reference = HdrPqReference::new(&atlas, "#").unwrap();
        let grid = reference.map(&frame, 4, 1).unwrap();
        for (cell, expected) in grid.cells.iter().zip(values) {
            // One legal ten-bit luma quantization step at these reference
            // levels; much tighter than an arbitrary percentage tolerance.
            let tolerance = match expected as u32 {
                100 => 0.1,
                1000 => 5.0,
                _ => 0.01,
            };
            assert!((cell.foreground_nits.luminance() - expected).abs() <= tolerance);
        }
        let (out, diag) = reference
            .render(&grid, frame.desc().clone(), frame.pts(), true)
            .unwrap();
        assert_eq!(out.desc().color_space, frame.desc().color_space);
        assert_eq!(out.pts(), Some(42));
        assert_eq!(diag.nonfinite_components, 0);
        assert!(diag.max_luminance_nits > 9000.0);
        assert!(
            out.host()
                .as_slice()
                .as_chunks::<2>()
                .0
                .iter()
                .all(|v| u16::from_le_bytes([v[0], v[1]]) & 63 == 0)
        );
        let (out_y, out_uv) = out.host().planes(out.desc());
        assert_eq!(read_code(out_y, 0).unwrap(), 64);
        for i in 0..out_uv.len() / 2 {
            assert_eq!(read_code(out_uv, i).unwrap(), 512);
        }
        assert_eq!(out, reference.process(&frame, 4, 1, true).unwrap().0);
    }

    #[test]
    fn cell_aggregation_is_linear_and_ten_bit_differences_survive() {
        let frame = neutral_frame(&[500, 501, 500, 501], 2, 2);
        let atlas = GlyphAtlas::from_r8(1, 1, 1, vec![255]).unwrap();
        let reference = HdrPqReference::new(&atlas, "#").unwrap();
        let grid = reference.map(&frame, 1, 1).unwrap();
        let a = hdr_pq::pq_eotf_nits(f64::from(500 - 64) / 876.0).unwrap();
        let b = hdr_pq::pq_eotf_nits(f64::from(501 - 64) / 876.0).unwrap();
        assert!((grid.cells[0].foreground_nits.luminance() - (a + b) / 2.0).abs() < 1e-7);
        let low = neutral_frame(&[500; 4], 2, 2);
        let high = neutral_frame(&[501; 4], 2, 2);
        assert_ne!(
            reference
                .process(&low, 1, 1, true)
                .unwrap()
                .0
                .host()
                .as_slice(),
            reference
                .process(&high, 1, 1, true)
                .unwrap()
                .0
                .host()
                .as_slice()
        );
    }

    #[test]
    fn color_chroma_and_half_coverage_use_linear_light() {
        let desc = hdr_desc(2, 2);
        let color = PqRgb {
            r: 0.7,
            g: 0.4,
            b: 0.2,
        };
        let (codes, _) = hdr_pq::encode_limited(hdr_pq::pq_to_ycbcr(color)).unwrap();
        let mut storage = HostFrame::new_zeroed(&desc);
        let (y, uv) = storage.planes_mut(&desc);
        for i in 0..4 {
            write_code(y, i, codes.y);
        }
        write_code(uv, 0, codes.cb);
        write_code(uv, 1, codes.cr);
        let frame = VideoFrame::new_host(desc, None, storage).unwrap();
        let atlas = GlyphAtlas::from_r8(1, 1, 1, vec![128]).unwrap();
        let reference = HdrPqReference::new(&atlas, "#").unwrap();
        let grid = reference.map(&frame, 1, 1).unwrap();
        let (out, diag) = reference
            .render(&grid, frame.desc().clone(), None, true)
            .unwrap();
        let (oy, ouv) = out.host().planes(out.desc());
        let reconstructed = hdr_pq::pq_to_linear(hdr_pq::ycbcr_to_pq(
            hdr_pq::decode_limited(P010Codes {
                y: read_code(oy, 0).unwrap(),
                cb: read_code(ouv, 0).unwrap(),
                cr: read_code(ouv, 1).unwrap(),
            })
            .unwrap(),
        ))
        .unwrap();
        let expected = grid.cells[0].foreground_nits.blend(LinearRgb::BLACK, 128);
        assert!(
            (reconstructed.luminance() - expected.luminance()).abs() < expected.luminance() * 0.03
        );
        assert_eq!(diag.nonfinite_components, 0);
        assert!(diag.clamped_components < 20);
    }

    #[test]
    fn glyph_index_is_monotone_for_hdr_nits() {
        let atlas = GlyphAtlas::builtin("builtin-8x8", "@%#*+=-:. ").unwrap();
        let reference = HdrPqReference::new(&atlas, "@%#*+=-:. ").unwrap();
        let mut last = 0;
        for nits in [0.0, 1.0, 10.0, 100.0, 1000.0, 10_000.0] {
            let code = hdr_pq::pq_inverse_eotf(nits).unwrap();
            let y = hdr_pq::encode_limited(hdr_pq::pq_to_ycbcr(PqRgb {
                r: code,
                g: code,
                b: code,
            }))
            .unwrap()
            .0
            .y;
            let frame = neutral_frame(&[y; 4], 2, 2);
            let glyph = reference.map(&frame, 1, 1).unwrap().cells[0].glyph;
            assert!(glyph >= last);
            last = glyph;
        }
    }

    #[test]
    fn freetype_atlas_and_scope_rejection() {
        let font = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/fonts/Inconsolata-Regular.ttf");
        let (atlas, _) = asciiflow_font::build_font_atlas(&font, 0, "# ", 16, 16).unwrap();
        let reference = HdrPqReference::new(&atlas, "# ").unwrap();
        let frame = neutral_frame(&[500; 16 * 16], 16, 16);
        let started = std::time::Instant::now();
        let (out, _) = reference.process(&frame, 1, 1, true).unwrap();
        eprintln!("HDR CPU reference 16x16: {:?}/frame", started.elapsed());
        assert!(out.host().as_slice().iter().any(|&b| b != 0));
        let mut wrong = frame.desc().clone();
        wrong.color_space.range = ColorRange::Full;
        assert!(
            reference
                .render(&reference.map(&frame, 1, 1).unwrap(), wrong, None, true)
                .is_err()
        );
    }

    #[test]
    fn deterministic_gradient_and_color_patch_signal() {
        let desc = hdr_desc(8, 8);
        let mut storage = HostFrame::new_zeroed(&desc);
        let (y, uv) = storage.planes_mut(&desc);
        for block_y in 0..4 {
            for block_x in 0..4 {
                let nits = ((block_x * 4 + block_y + 1) as f64).powi(2) * 20.0;
                let base = hdr_pq::pq_inverse_eotf(nits).unwrap();
                let rgb = match (block_x + block_y) % 4 {
                    0 => PqRgb {
                        r: base,
                        g: base,
                        b: base,
                    },
                    1 => PqRgb {
                        r: base,
                        g: base * 0.5,
                        b: base * 0.25,
                    },
                    2 => PqRgb {
                        r: base * 0.25,
                        g: base,
                        b: base * 0.5,
                    },
                    _ => PqRgb {
                        r: base * 0.5,
                        g: base * 0.25,
                        b: base,
                    },
                };
                let (codes, _) = hdr_pq::encode_limited(hdr_pq::pq_to_ycbcr(rgb)).unwrap();
                for dy in 0..2 {
                    for dx in 0..2 {
                        write_code(y, (block_y * 2 + dy) * 8 + block_x * 2 + dx, codes.y);
                    }
                }
                let offset = block_y * 8 + block_x * 2;
                write_code(uv, offset, codes.cb);
                write_code(uv, offset + 1, codes.cr);
            }
        }
        let frame = VideoFrame::new_host(desc, Some(1), storage).unwrap();
        let atlas = GlyphAtlas::builtin("builtin-8x8", "@%#*+=-:. ").unwrap();
        let oracle = HdrPqReference::new(&atlas, "@%#*+=-:. ").unwrap();
        let first = oracle.process(&frame, 4, 4, true).unwrap();
        let second = oracle.process(&frame, 4, 4, true).unwrap();
        assert_eq!(first.0, second.0);
        assert_eq!(first.1, second.1);
        assert_eq!(first.1.nonfinite_components, 0);
        assert!(first.1.max_luminance_nits > first.1.min_luminance_nits);
        assert!(first.1.max_component_nits > first.1.min_component_nits);
        assert!(
            first
                .0
                .host()
                .as_slice()
                .as_chunks::<2>()
                .0
                .iter()
                .all(|word| u16::from_le_bytes([word[0], word[1]]) & 63 == 0)
        );
    }

    #[test]
    fn malformed_padding_and_nominal_range_fail_closed() {
        let atlas = GlyphAtlas::from_r8(1, 1, 1, vec![255]).unwrap();
        let oracle = HdrPqReference::new(&atlas, "#").unwrap();
        let desc = hdr_desc(2, 2);
        let mut padding = neutral_frame(&[500; 4], 2, 2).host().as_slice().to_vec();
        padding[0] |= 1;
        let frame = VideoFrame::new_host(
            desc.clone(),
            None,
            HostFrame::from_p010_le(&desc, padding).unwrap(),
        )
        .unwrap();
        assert!(oracle.map(&frame, 1, 1).is_err());
        let outside = neutral_frame(&[63; 4], 2, 2);
        assert!(oracle.map(&outside, 1, 1).is_err());
    }

    #[test]
    fn uneven_grid_uses_mapping_boundaries_during_render() {
        assert_eq!(
            (0..6).map(|p| cell_at(p, 6, 4)).collect::<Vec<_>>(),
            [0, 1, 1, 2, 3, 3]
        );
        let atlas = GlyphAtlas::from_r8(1, 1, 1, vec![255]).unwrap();
        let oracle = HdrPqReference::new(&atlas, "#").unwrap();
        let rows = [10.0, 100.0, 1000.0, 5000.0];
        let cells = rows
            .into_iter()
            .flat_map(|nits| {
                [HdrCell {
                    glyph: 0,
                    foreground_nits: LinearRgb {
                        r: nits,
                        g: nits,
                        b: nits,
                    },
                }; 4]
            })
            .collect();
        let grid = HdrCellGrid {
            width: 4,
            height: 4,
            cells,
            clamped_input_components: 0,
        };
        let (frame, _) = oracle.render(&grid, hdr_desc(6, 6), None, true).unwrap();
        let (y, _) = frame.host().planes(frame.desc());
        let row_codes = (0..6)
            .map(|row| read_code(y, row * 6).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(row_codes[1], row_codes[2]);
        assert_eq!(row_codes[4], row_codes[5]);
        assert!(
            row_codes[0] < row_codes[1]
                && row_codes[2] < row_codes[3]
                && row_codes[3] < row_codes[4]
        );
    }

    #[test]
    fn legal_ycbcr_rgb_excursion_is_clipped_and_counted() {
        let desc = hdr_desc(2, 2);
        let mut storage = HostFrame::new_zeroed(&desc);
        let (y, uv) = storage.planes_mut(&desc);
        for index in 0..4 {
            write_code(y, index, 940);
        }
        write_code(uv, 0, 512);
        write_code(uv, 1, 960);
        let frame = VideoFrame::new_host(desc, None, storage).unwrap();
        let atlas = GlyphAtlas::from_r8(1, 1, 1, vec![255]).unwrap();
        let oracle = HdrPqReference::new(&atlas, "#").unwrap();
        let grid = oracle.map(&frame, 1, 1).unwrap();
        assert!(grid.clamped_input_components > 0);
        let (_, diagnostics) = oracle
            .render(&grid, frame.desc().clone(), None, true)
            .unwrap();
        assert!(diagnostics.clamped_components >= grid.clamped_input_components);
        assert_eq!(diagnostics.nonfinite_components, 0);
    }
}
