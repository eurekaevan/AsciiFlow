//! Internal post-ASCII BT.2446 Method A frame reference. Never a production backend.
use crate::hdr::{HdrCellGrid, HdrPqReference, HdrRenderedLinearFrame};
use asciiflow_core::tone_map_bt2446::{
    LinearBt2020RgbNits, MethodA, MethodAOutput, SOURCE_PEAK_NITS, TARGET_PEAK_NITS,
};
use asciiflow_core::{Error, Result, VideoFrame};

#[derive(Clone, Debug, PartialEq)]
pub struct ToneMapDiagnostics {
    pub input_min_nits: f64,
    pub input_max_nits: f64,
    pub output_min_signal: f64,
    pub output_max_signal: f64,
    pub mapped_luma_min: f64,
    pub mapped_luma_max: f64,
    /// Table 2 luma interpreted on a zero-black 100-nit reference display.
    /// This is NOT photometric luminance of the reconstructed colored RGB.
    pub reference_luma_min_nits: f64,
    pub reference_luma_max_nits: f64,
    pub input_negative_components: u64,
    pub input_above_peak_components: u64,
    pub output_negative_components: u64,
    pub output_above_one_components: u64,
    pub nan_components: u64,
    pub inf_components: u64,
    pub input_safety_clamps: u64,
    /// Table 3 max(0.1 Cr', 0) positive-part operations that discard a
    /// negative term, NOT output RGB clipping or gamut reduction.
    pub standard_required_clamps: u64,
    pub unexpected_clamps: u64,
}

impl Default for ToneMapDiagnostics {
    fn default() -> Self {
        Self {
            input_min_nits: f64::INFINITY,
            input_max_nits: f64::NEG_INFINITY,
            output_min_signal: f64::INFINITY,
            output_max_signal: f64::NEG_INFINITY,
            mapped_luma_min: f64::INFINITY,
            mapped_luma_max: f64::NEG_INFINITY,
            reference_luma_min_nits: f64::INFINITY,
            reference_luma_max_nits: f64::NEG_INFINITY,
            input_negative_components: 0,
            input_above_peak_components: 0,
            output_negative_components: 0,
            output_above_one_components: 0,
            nan_components: 0,
            inf_components: 0,
            input_safety_clamps: 0,
            standard_required_clamps: 0,
            unexpected_clamps: 0,
        }
    }
}

#[derive(Debug)]
pub struct ToneMapFrameError {
    pub pixel_index: Option<usize>,
    pub reason: String,
    pub diagnostics: Box<ToneMapDiagnostics>,
}
impl std::fmt::Display for ToneMapFrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} at pixel {:?}; diagnostics: {:?}",
            self.reason, self.pixel_index, self.diagnostics
        )
    }
}
impl std::error::Error for ToneMapFrameError {}

#[derive(Debug)]
pub struct SdrBt2020ReferenceFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<MethodAOutput>,
    pub diagnostics: ToneMapDiagnostics,
}

/// No P010/NV12 descriptor, codec selection, metadata inference or frame state.
/// Input is explicitly BT.2020 display-light nits with fixed 1000-nit semantics.
pub fn map_linear_frame(
    width: u32,
    height: u32,
    input: &[LinearBt2020RgbNits],
) -> std::result::Result<SdrBt2020ReferenceFrame, ToneMapFrameError> {
    let mut diag = ToneMapDiagnostics::default();
    let fail = |index, reason: String, diagnostics| ToneMapFrameError {
        pixel_index: index,
        reason,
        diagnostics: Box::new(diagnostics),
    };
    let count = (width as usize).checked_mul(height as usize);
    if width == 0 || height == 0 || count != Some(input.len()) {
        return Err(fail(
            None,
            "invalid linear frame dimensions/sample count".into(),
            diag,
        ));
    }
    let mut invalid = None;
    for (index, &pixel) in input.iter().enumerate() {
        for value in [pixel.r, pixel.g, pixel.b] {
            if value.is_nan() {
                diag.nan_components += 1;
            } else if value.is_infinite() {
                diag.inf_components += 1;
            } else {
                diag.input_min_nits = diag.input_min_nits.min(value);
                diag.input_max_nits = diag.input_max_nits.max(value);
                diag.input_negative_components += u64::from(value < 0.0);
                diag.input_above_peak_components += u64::from(value > SOURCE_PEAK_NITS);
            }
        }
        if let Err(error) = MethodA::validate_input(pixel) {
            invalid.get_or_insert((index, error));
        }
    }
    if let Some((index, error)) = invalid {
        return Err(fail(Some(index), error.to_string(), diag));
    }
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(input.len()).map_err(|e| {
        fail(
            None,
            format!("allocate SDR reference frame: {e}"),
            diag.clone(),
        )
    })?;
    let method = MethodA::new();
    for (index, &pixel) in input.iter().enumerate() {
        let mapped = method
            .map(pixel)
            .map_err(|e| fail(Some(index), e.to_string(), diag.clone()))?;
        for value in [mapped.rgb.r, mapped.rgb.g, mapped.rgb.b] {
            diag.output_min_signal = diag.output_min_signal.min(value);
            diag.output_max_signal = diag.output_max_signal.max(value);
            diag.output_negative_components += u64::from(value < 0.0);
            diag.output_above_one_components += u64::from(value > 1.0);
        }
        diag.standard_required_clamps += u64::from(mapped.ycbcr.cr < 0.0);
        diag.mapped_luma_min = diag.mapped_luma_min.min(mapped.mapped_luma);
        diag.mapped_luma_max = diag.mapped_luma_max.max(mapped.mapped_luma);
        let reference_nits = TARGET_PEAK_NITS * mapped.mapped_luma.powf(2.4);
        diag.reference_luma_min_nits = diag.reference_luma_min_nits.min(reference_nits);
        diag.reference_luma_max_nits = diag.reference_luma_max_nits.max(reference_nits);
        pixels.push(mapped);
    }
    Ok(SdrBt2020ReferenceFrame {
        width,
        height,
        pixels,
        diagnostics: diag,
    })
}

pub struct ToneMappedAsciiReference {
    pub hdr_cells: HdrCellGrid,
    pub rendered_hdr: HdrRenderedLinearFrame,
    pub sdr: SdrBt2020ReferenceFrame,
}

/// Reuse B-1 cell selection and linear-light glyph rendering, then apply C-1.
/// Tone mapping receives no source frame or static mastering/CLL metadata.
pub fn map_hdr_ascii(
    reference: &HdrPqReference<'_>,
    input: &VideoFrame,
    grid_width: u32,
    grid_height: u32,
    color: bool,
) -> Result<ToneMappedAsciiReference> {
    let hdr_cells = reference.map(input, grid_width, grid_height)?;
    let rendered_hdr = reference.render_linear(&hdr_cells, input.desc(), color)?;
    let sdr = map_linear_frame(
        rendered_hdr.width(),
        rendered_hdr.height(),
        rendered_hdr.pixels(),
    )
    .map_err(|e| Error::Cpu(format!("internal Method A qualification: {e}")))?;
    Ok(ToneMappedAsciiReference {
        hdr_cells,
        rendered_hdr,
        sdr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use asciiflow_core::{AsciiConfig, ColorSpace, FrameDesc, HostFrame, hdr_pq};
    use asciiflow_font::GlyphAtlas;

    fn source() -> VideoFrame {
        let desc = FrameDesc::host_p010_le(96, 64, ColorSpace::pq_bt2020()).unwrap();
        let mut host = HostFrame::new_zeroed(&desc);
        let (y, uv) = host.planes_mut(&desc);
        for (index, sample) in y.chunks_exact_mut(2).enumerate() {
            // Codes <=722 decode below 1000 nits (723 is slightly above).
            let code = 64 + (index % 659) as u16;
            sample.copy_from_slice(&(code << 6).to_le_bytes());
        }
        for sample in uv.chunks_exact_mut(2) {
            sample.copy_from_slice(&(512u16 << 6).to_le_bytes());
        }
        // A moderate skin-like color tile exercises non-neutral post-ASCII
        // composition. Quantized PQ decode remains comfortably below 1000 nits.
        let rgb = hdr_pq::LinearRgb {
            r: 203.0,
            g: 120.0,
            b: 80.0,
        };
        let (codes, _) =
            hdr_pq::encode_limited(hdr_pq::pq_to_ycbcr(hdr_pq::linear_to_pq(rgb).unwrap()))
                .unwrap();
        for py in 0..16usize {
            for px in 0..16usize {
                let index = 2 * (py * 96 + px);
                y[index..index + 2].copy_from_slice(&(codes.y << 6).to_le_bytes());
            }
        }
        for py in 0..8usize {
            for px in 0..8usize {
                let index = py * 192 + px * 4;
                uv[index..index + 2].copy_from_slice(&(codes.cb << 6).to_le_bytes());
                uv[index + 2..index + 4].copy_from_slice(&(codes.cr << 6).to_le_bytes());
            }
        }
        VideoFrame::new_host(desc, Some(42), host).unwrap()
    }

    fn check_atlas(atlas: GlyphAtlas, config: &AsciiConfig, grid_width: u32, grid_height: u32) {
        let reference = HdrPqReference::new(&atlas, &config.charset).unwrap();
        let input = source();
        let before = reference.map(&input, grid_width, grid_height).unwrap();
        let (preserved, _) = reference
            .render(&before, input.desc().clone(), input.pts(), true)
            .unwrap();
        let before_bytes = preserved.host().as_slice().to_vec();
        let output = map_hdr_ascii(&reference, &input, grid_width, grid_height, true).unwrap();
        assert_eq!(before, output.hdr_cells, "glyph mismatch must be zero");
        let linear = reference
            .render_linear(&before, input.desc(), true)
            .unwrap();
        assert_eq!(
            linear, output.rendered_hdr,
            "coverage and linear composition changed"
        );
        // Independent coverage expectation on exact atlas-sized cells.
        let aw = atlas.width() as usize;
        let ah = atlas.height() as usize;
        for py in 0..64usize {
            for px in 0..96usize {
                let index = py * 96 + px;
                let cell = before.cells[(py / ah) * grid_width as usize + px / aw];
                let coverage = atlas.glyph(cell.glyph as usize)[(py % ah) * aw + px % aw];
                assert_eq!(linear.coverage()[index], coverage);
                let expected = cell
                    .foreground_nits
                    .blend(hdr_pq::LinearRgb::BLACK, coverage);
                assert_eq!(linear.pixels()[index], expected);
                let pq = hdr_pq::linear_to_pq(expected).unwrap();
                let codes = hdr_pq::encode_limited(hdr_pq::pq_to_ycbcr(pq)).unwrap().0;
                let (y, _) = preserved.host().planes(preserved.desc());
                let actual = u16::from_le_bytes([y[index * 2], y[index * 2 + 1]]) >> 6;
                assert_eq!(actual, codes.y, "shared renderer P010 semantics changed");
            }
        }
        assert_eq!(
            before_bytes,
            reference
                .process(&input, grid_width, grid_height, true)
                .unwrap()
                .0
                .host()
                .as_slice()
        );
        assert_eq!(output.sdr.diagnostics.nan_components, 0);
        assert_eq!(output.sdr.diagnostics.inf_components, 0);
        assert_eq!(output.sdr.diagnostics.input_safety_clamps, 0);
        assert_eq!(output.sdr.diagnostics.unexpected_clamps, 0);
        // Changing the target image cannot feed back into immutable HDR cells.
        let other = map_hdr_ascii(&reference, &input, grid_width, grid_height, false).unwrap();
        assert_eq!(before, other.hdr_cells);
    }

    #[test]
    fn builtin_post_ascii_preserves_glyph_coverage_and_linear_rgb() {
        let config = AsciiConfig::default();
        check_atlas(
            GlyphAtlas::builtin(&config.font, &config.charset).unwrap(),
            &config,
            12,
            8,
        );
    }

    #[test]
    fn freetype_post_ascii_preserves_glyph_coverage_and_linear_rgb() {
        let config = AsciiConfig::default();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/fonts/Inconsolata-Regular.ttf");
        let (atlas, _) =
            asciiflow_font::build_font_atlas(&path, 0, &config.charset, 16, 16).unwrap();
        check_atlas(atlas, &config, 6, 4);
    }

    #[test]
    fn diagnostic_counts_preserve_excursions_and_reject_invalid_input() {
        let rgb = |r, g, b| LinearBt2020RgbNits { r, g, b };
        let frame =
            map_linear_frame(2, 1, &[rgb(1000.0, 0.0, 0.0), rgb(0.0, 0.0, 1000.0)]).unwrap();
        assert_eq!(frame.diagnostics.output_negative_components, 2);
        assert_eq!(frame.diagnostics.output_above_one_components, 2);
        assert_eq!(frame.diagnostics.standard_required_clamps, 1);
        assert_eq!(frame.diagnostics.input_safety_clamps, 0);
        assert_eq!(frame.diagnostics.unexpected_clamps, 0);
        let invalid = map_linear_frame(
            2,
            1,
            &[
                rgb(1001.0, -1.0, f64::NAN),
                rgb(f64::INFINITY, 4000.0, 10000.0),
            ],
        )
        .unwrap_err();
        assert_eq!(invalid.pixel_index, Some(0));
        assert_eq!(invalid.diagnostics.input_above_peak_components, 3);
        assert_eq!(invalid.diagnostics.input_negative_components, 1);
        assert_eq!(invalid.diagnostics.nan_components, 1);
        assert_eq!(invalid.diagnostics.inf_components, 1);
    }

    #[test]
    fn metadata_free_stateless_mapping_is_bit_deterministic() {
        let input = [LinearBt2020RgbNits {
            r: 203.0,
            g: 120.0,
            b: 80.0,
        }];
        let first = map_linear_frame(1, 1, &input).unwrap();
        for _ in 0..3 {
            let repeated = map_linear_frame(1, 1, &input).unwrap();
            assert_eq!(first.pixels, repeated.pixels);
            assert_eq!(first.diagnostics, repeated.diagnostics);
        }
        assert!(map_linear_frame(0, 1, &[]).is_err());
    }
}
