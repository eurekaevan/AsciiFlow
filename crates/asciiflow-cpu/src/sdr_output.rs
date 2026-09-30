//! C-4A independent f64 signal/packing oracle. Qualification only: consumes
//! already bounded nonlinear BT.709 RGB, without OETF, limiting or metadata.
use asciiflow_core::{Error, PixelFormat, Result};

/// Semantic input, deliberately distinct from source-linear/PQ RGB.
#[derive(Clone, Copy, Debug)]
pub struct NonlinearBt709Rgb(pub [f64; 3]);

#[derive(Clone, Copy, Debug)]
pub struct Bt709Ycbcr {
    pub y: f64,
    pub cb: f64,
    pub cr: f64,
}

pub const KR: f64 = 0.2126;
pub const KG: f64 = 0.7152;
pub const KB: f64 = 0.0722;

pub fn to_ycbcr(rgb: NonlinearBt709Rgb) -> Result<Bt709Ycbcr> {
    let [r, g, b] = rgb.0;
    if rgb
        .0
        .iter()
        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(Error::Cpu(
            "C-4A upstream nonlinear BT.709 RGB contract violation".into(),
        ));
    }
    // Difference form preserves neutral exactly; it is algebraically BT.709
    // NCL, not the BT.2020 source matrix.
    let dr = r - g;
    let db = b - g;
    let y = g + KR * dr + KB * db;
    Ok(Bt709Ycbcr {
        y,
        // Cancel (1-Kb)/(2*(1-Kb)) and (1-Kr)/(2*(1-Kr))
        // before evaluation. The binary-exact half retains analytical axis
        // ties independently of the neutral base; subtracting a rounded Y
        // first can put an exact tie in the wrong integer bin even in f64.
        cb: 0.5 * db - (KR / 1.8556) * dr,
        cr: 0.5 * dr - (KB / 1.5748) * db,
    })
}

#[derive(Clone, Copy)]
pub struct LimitedCodes {
    pub y_offset: f64,
    pub y_scale: f64,
    pub c_offset: f64,
    pub c_scale: f64,
    pub shift: u32,
}

impl LimitedCodes {
    pub const fn for_format(format: PixelFormat) -> Self {
        match format {
            PixelFormat::Nv12 => Self {
                y_offset: 16.0,
                y_scale: 219.0,
                c_offset: 128.0,
                c_scale: 224.0,
                shift: 0,
            },
            PixelFormat::P010Le => Self {
                y_offset: 64.0,
                y_scale: 876.0,
                c_offset: 512.0,
                c_scale: 896.0,
                shift: 6,
            },
        }
    }
}

/// Nearest positive code, exact halves upward. No truncation or dithering.
fn quantize(value: f64, offset: f64, scale: f64) -> u16 {
    (offset + scale * value + 0.5).floor() as u16
}

pub fn pack(
    width: u32,
    height: u32,
    pixels: &[NonlinearBt709Rgb],
    format: PixelFormat,
) -> Result<Vec<u8>> {
    let count = (width as usize).checked_mul(height as usize);
    if width == 0 || height == 0 || width % 2 != 0 || height % 2 != 0 || count != Some(pixels.len())
    {
        return Err(Error::Cpu(
            "C-4A requires nonzero even geometry and exact RGB sample count".into(),
        ));
    }
    let c = LimitedCodes::for_format(format);
    let bytes = format.frame_byte_len(width, height)?;
    let mut out = Vec::new();
    out.try_reserve_exact(bytes)
        .map_err(|e| Error::Cpu(format!("allocate C-4A packed output: {e}")))?;
    out.resize(bytes, 0);
    let w = width as usize;
    let h = height as usize;
    let mut write = |index: usize, code: u16| match format {
        PixelFormat::Nv12 => out[index] = code as u8,
        PixelFormat::P010Le => {
            out[index * 2..index * 2 + 2].copy_from_slice(&(code << c.shift).to_le_bytes())
        }
    };
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let mut cb = 0.0;
            let mut cr = 0.0;
            for dy in 0..2 {
                for dx in 0..2 {
                    let i = (y + dy) * w + x + dx;
                    let signal = to_ycbcr(pixels[i])
                        .map_err(|e| Error::Cpu(format!("C-4A pixel {i}: {e}")))?;
                    write(i, quantize(signal.y, c.y_offset, c.y_scale));
                    cb += signal.cb;
                    cr += signal.cr;
                }
            }
            let uv = w * h + y / 2 * w + x;
            write(uv, quantize(cb / 4.0, c.c_offset, c.c_scale));
            write(uv + 1, quantize(cr / 4.0, c.c_offset, c.c_scale));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn codes(rgb: [f64; 3], format: PixelFormat) -> Vec<u16> {
        let bytes = pack(2, 2, &[NonlinearBt709Rgb(rgb); 4], format).unwrap();
        match format {
            PixelFormat::Nv12 => bytes.into_iter().map(u16::from).collect(),
            PixelFormat::P010Le => bytes
                .chunks_exact(2)
                .map(|v| {
                    let word = u16::from_le_bytes([v[0], v[1]]);
                    assert_eq!(word & 63, 0);
                    word >> 6
                })
                .collect(),
        }
    }
    #[test]
    fn endpoints_neutral_and_primary_secondary_vectors() {
        let vectors = [
            ([0., 0., 0.], [16, 128, 128], [64, 512, 512]),
            ([1., 1., 1.], [235, 128, 128], [940, 512, 512]),
            ([0.5, 0.5, 0.5], [126, 128, 128], [502, 512, 512]),
            ([1., 0., 0.], [63, 102, 240], [250, 409, 960]),
            ([0., 1., 0.], [173, 42, 26], [691, 167, 105]),
            ([0., 0., 1.], [32, 240, 118], [127, 960, 471]),
            ([0., 1., 1.], [188, 154, 16], [754, 615, 64]),
            ([1., 0., 1.], [78, 214, 230], [313, 857, 919]),
            ([1., 1., 0.], [219, 16, 138], [877, 64, 553]),
        ];
        for (rgb, nv12, p010) in vectors {
            for (format, expected) in [(PixelFormat::Nv12, nv12), (PixelFormat::P010Le, p010)] {
                let actual = codes(rgb, format);
                assert_eq!(
                    actual,
                    vec![
                        expected[0],
                        expected[0],
                        expected[0],
                        expected[0],
                        expected[1],
                        expected[2]
                    ],
                    "{rgb:?} {format:?}"
                );
            }
        }
        // BT.2020 red luma would be 74/294: wrong-source-matrix sentinel.
        assert_ne!(codes([1., 0., 0.], PixelFormat::Nv12)[0], 74);
        assert_ne!(codes([1., 0., 0.], PixelFormat::P010Le)[0], 294);
    }
    #[test]
    fn chroma_is_reduced_before_quantization_in_row_major_order() {
        // Black, red / green, blue: unquantized chroma sums cancel exactly.
        let pixels =
            [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]].map(NonlinearBt709Rgb);
        assert_eq!(
            pack(2, 2, &pixels, PixelFormat::Nv12).unwrap(),
            [16, 63, 173, 32, 128, 128]
        );
        let checker =
            [[1., 0., 0.], [0., 0., 1.], [0., 0., 1.], [1., 0., 0.]].map(NonlinearBt709Rgb);
        assert_eq!(
            pack(2, 2, &checker, PixelFormat::Nv12).unwrap(),
            [63, 32, 32, 63, 171, 179]
        );
        // Three neutral gray pixels plus red: red chroma contributes one
        // quarter before rounding. Cr=.5/4 gives code128+224/8=156 exactly.
        let mixed = [[0.5; 3], [0.5; 3], [0.5; 3], [1., 0., 0.]].map(NonlinearBt709Rgb);
        assert_eq!(
            pack(2, 2, &mixed, PixelFormat::Nv12).unwrap(),
            [126, 126, 126, 63, 122, 156]
        );
    }
    #[test]
    fn half_code_boundaries_and_invalid_inputs() {
        for (offset, scale) in [(16., 219.), (128., 224.), (64., 876.), (512., 896.)] {
            let threshold = (100.5 - offset) / scale;
            assert_eq!(quantize(threshold - 1e-12, offset, scale), 100);
            assert_eq!(quantize(threshold, offset, scale), 101);
            assert_eq!(quantize(threshold + 1e-12, offset, scale), 101);
        }
        for invalid in [f64::NAN, f64::INFINITY, -0.001, 1.001] {
            assert!(
                pack(
                    2,
                    2,
                    &[NonlinearBt709Rgb([invalid, 0., 0.]); 4],
                    PixelFormat::Nv12
                )
                .is_err()
            );
        }
        assert!(pack(3, 2, &[NonlinearBt709Rgb([0.; 3]); 6], PixelFormat::Nv12).is_err());
        assert!(pack(2, 2, &[], PixelFormat::P010Le).is_err());
    }

    #[test]
    fn neutral_ramps_keep_chroma_exact_and_codes_in_range() {
        for format in [PixelFormat::Nv12, PixelFormat::P010Le] {
            let c = LimitedCodes::for_format(format);
            for step in 0..=4096 {
                let v = step as f64 / 4096.0;
                let values = codes([v; 3], format);
                assert!((c.y_offset as u16..=(c.y_offset + c.y_scale) as u16).contains(&values[0]));
                assert_eq!(values[4], c.c_offset as u16);
                assert_eq!(values[5], c.c_offset as u16);
            }
        }
    }

    #[test]
    fn four_by_four_uv_layout_is_not_transposed() {
        let mut pixels = vec![NonlinearBt709Rgb([0.; 3]); 16];
        for (block, rgb) in [[1., 0., 0.], [0., 0., 1.], [0., 1., 1.], [1., 1., 0.]]
            .into_iter()
            .enumerate()
        {
            let x = block % 2 * 2;
            let y = block / 2 * 2;
            for dy in 0..2 {
                for dx in 0..2 {
                    pixels[(y + dy) * 4 + x + dx] = NonlinearBt709Rgb(rgb);
                }
            }
        }
        let out = pack(4, 4, &pixels, PixelFormat::Nv12).unwrap();
        assert_eq!(&out[16..], &[102, 240, 240, 118, 154, 16, 16, 138]);
    }

    #[test]
    fn analytical_chroma_half_tie_is_independent_of_neutral_base() {
        // R-G=-1/32, B-G=0, hence Cr=(R-G)/2=-1/64 exactly.
        // 128 + 224*(-1/64) = 124.5, whose half-up code is 125.
        // This expected code is derived analytically, not from a GPU result.
        assert_eq!(
            codes([0.4765625, 0.5078125, 0.5078125], PixelFormat::Nv12)[5],
            125
        );
    }
}
