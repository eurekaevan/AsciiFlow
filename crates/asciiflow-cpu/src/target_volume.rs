//! Internal C-2B post-Method-A reference. No production backend or encoding.
use crate::hdr::HdrPqReference;
use crate::tone_map::{SdrBt2020ReferenceFrame, ToneMappedAsciiReference, map_hdr_ascii};
use asciiflow_core::sdr_target_volume::{SdrConversionOutput, convert_method_a_rgb};
use asciiflow_core::{Error, Result, VideoFrame};

#[derive(Debug)]
pub struct Bt709TargetReferenceFrame {
    pub width: u32,
    pub height: u32,
    /// Observable source-linear, unbounded, bounded and nonlinear stages.
    pub pixels: Vec<SdrConversionOutput>,
}

/// Read the sealed C-1 result immutably: no source preclip or feedback into C-1.
pub fn convert_frame(input: &SdrBt2020ReferenceFrame) -> Result<Bt709TargetReferenceFrame> {
    let count = (input.width as usize).checked_mul(input.height as usize);
    if input.width == 0 || input.height == 0 || count != Some(input.pixels.len()) {
        return Err(Error::Cpu("invalid C-2B dimensions/sample count".into()));
    }
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(input.pixels.len())
        .map_err(|e| Error::Cpu(format!("allocate C-2B reference frame: {e}")))?;
    for (index, pixel) in input.pixels.iter().enumerate() {
        pixels.push(
            convert_method_a_rgb(pixel.rgb)
                .map_err(|e| Error::Cpu(format!("C-2B pixel {index}: {e}")))?,
        );
    }
    Ok(Bt709TargetReferenceFrame {
        width: input.width,
        height: input.height,
        pixels,
    })
}

pub struct TargetVolumeAsciiReference {
    pub tone_mapped: ToneMappedAsciiReference,
    pub bt709: Bt709TargetReferenceFrame,
}

/// PQ P010 -> existing HDR ASCII/coverage -> sealed C-1 -> C-2B f64 target.
/// This does not select a production pixel format, metadata, range or codec.
pub fn convert_hdr_ascii(
    reference: &HdrPqReference<'_>,
    input: &VideoFrame,
    grid_width: u32,
    grid_height: u32,
    color: bool,
) -> Result<TargetVolumeAsciiReference> {
    let tone_mapped = map_hdr_ascii(reference, input, grid_width, grid_height, color)?;
    let bt709 = convert_frame(&tone_mapped.sdr)?;
    Ok(TargetVolumeAsciiReference { tone_mapped, bt709 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use asciiflow_core::tone_map_bt2446::LinearBt2020RgbNits;
    use asciiflow_core::{AsciiConfig, ColorSpace, FrameDesc, HostFrame, hdr_pq};
    use asciiflow_font::GlyphAtlas;

    fn source() -> VideoFrame {
        let desc = FrameDesc::host_p010_le(96, 64, ColorSpace::pq_bt2020()).unwrap();
        let mut host = HostFrame::new_zeroed(&desc);
        let (y, uv) = host.planes_mut(&desc);
        for (index, sample) in y.chunks_exact_mut(2).enumerate() {
            sample.copy_from_slice(&((64 + (index % 659) as u16) << 6).to_le_bytes());
        }
        for sample in uv.chunks_exact_mut(2) {
            sample.copy_from_slice(&(512u16 << 6).to_le_bytes());
        }
        let skin = hdr_pq::LinearRgb {
            r: 203.0,
            g: 120.0,
            b: 80.0,
        };
        let (codes, _) =
            hdr_pq::encode_limited(hdr_pq::pq_to_ycbcr(hdr_pq::linear_to_pq(skin).unwrap()))
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

    fn check_atlas(atlas: GlyphAtlas, gw: u32, gh: u32) {
        let config = AsciiConfig::default();
        let reference = HdrPqReference::new(&atlas, &config.charset).unwrap();
        let input = source();
        for color in [false, true] {
            let before = map_hdr_ascii(&reference, &input, gw, gh, color).unwrap();
            let output = convert_hdr_ascii(&reference, &input, gw, gh, color).unwrap();
            assert_eq!(
                before.hdr_cells, output.tone_mapped.hdr_cells,
                "glyph mismatch must be zero"
            );
            assert_eq!(
                before.rendered_hdr, output.tone_mapped.rendered_hdr,
                "coverage/linear composition changed"
            );
            assert_eq!(
                before.sdr.pixels, output.tone_mapped.sdr.pixels,
                "sealed C-1 changed"
            );
            assert_eq!(before.sdr.diagnostics, output.tone_mapped.sdr.diagnostics);
            for (mapped, target) in before.sdr.pixels.iter().zip(&output.bt709.pixels) {
                assert_eq!(*target, convert_method_a_rgb(mapped.rgb).unwrap());
                assert!(
                    target
                        .nonlinear
                        .components()
                        .iter()
                        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
                );
            }
            let preserved = before.sdr.pixels.clone();
            convert_frame(&before.sdr).unwrap();
            assert_eq!(before.sdr.pixels, preserved, "immutable C-1 consumption");
        }
    }
    #[test]
    fn builtin_complete_pq_ascii_c1_c2b_chain() {
        let config = AsciiConfig::default();
        check_atlas(
            GlyphAtlas::builtin(&config.font, &config.charset).unwrap(),
            12,
            8,
        );
    }
    #[test]
    fn freetype_complete_pq_ascii_c1_c2b_chain() {
        let config = AsciiConfig::default();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/fonts/Inconsolata-Regular.ttf");
        let (atlas, _) =
            asciiflow_font::build_font_atlas(&path, 0, &config.charset, 16, 16).unwrap();
        check_atlas(atlas, 6, 4);
    }
    #[test]
    fn rejects_invalid_dimensions_and_nonfinite_with_pixel_context() {
        let mut input = crate::tone_map::map_linear_frame(
            1,
            1,
            &[LinearBt2020RgbNits {
                r: 100.0,
                g: 100.0,
                b: 100.0,
            }],
        )
        .unwrap();
        input.width = 2;
        assert!(convert_frame(&input).is_err());
        input.width = 1;
        input.pixels[0].rgb.g = f64::NAN;
        assert!(
            convert_frame(&input)
                .unwrap_err()
                .to_string()
                .contains("pixel 0")
        );
    }
}
