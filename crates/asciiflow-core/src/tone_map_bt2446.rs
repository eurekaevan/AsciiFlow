//! Internal f64 ITU-R BT.2446-1 (03/2021), §4.1 Method A reference.
//! Authoritative source: https://www.itu.int/dms_pub/itu-r/opb/rep/R-REP-BT.2446-1-2021-PDF-E.pdf
//! Tables 2–3 describe the forward operation; they do not number its equations.
//! Fixed display-referred 1000 → 100 cd/m², BT.2020 throughout. No codec tags.

pub use crate::hdr_pq::LinearRgb as LinearBt2020RgbNits;

pub const SOURCE_PEAK_NITS: f64 = 1000.0;
pub const TARGET_PEAK_NITS: f64 = 100.0;
const GAMMA: f64 = 2.4;
// BT.2446-1 Table 2 and BT.2020-2 Table 4, non-constant luminance.
const KR: f64 = 0.2627;
const KG: f64 = 0.6780;
const KB: f64 = 0.0593;
const CB_DENOMINATOR: f64 = 1.8814;
const CR_DENOMINATOR: f64 = 1.4746;

/// Raw nonlinear SDR reference signal, not PQ, camera OETF, or a bounded codec
/// representation. Table 3 can yield negative or >1 components for saturated
/// BT.2020 colors. Keep those excursions for qualification; do not clip them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SdrBt2020NonlinearRgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SdrBt2020Ycbcr {
    pub y: f64,
    pub cb: f64,
    pub cr: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MethodAOutput {
    pub rgb: SdrBt2020NonlinearRgb,
    pub ycbcr: SdrBt2020Ycbcr,
    /// Table 2 step 3 luma, before Table 3's chroma-dependent adjustment.
    /// It is nonlinear luma, not the photometric luminance of a colored pixel.
    pub mapped_luma: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToneMapError {
    NonFiniteInput,
    NegativeInput,
    AboveSourcePeak,
    NonFiniteOutput,
}

impl std::fmt::Display for ToneMapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NonFiniteInput => "non-finite HDR linear component",
            Self::NegativeInput => "negative HDR linear component",
            Self::AboveSourcePeak => "HDR linear component exceeds the fixed 1000 cd/m² reference",
            Self::NonFiniteOutput => "non-finite Method A output",
        })
    }
}
impl std::error::Error for ToneMapError {}

/// One fixed, stateless operator. Constants are computed once, not fitted to
/// frame extrema, static metadata, or a user-selected peak.
pub struct MethodA {
    rho_hdr_minus_one: f64,
    log_rho_hdr: f64,
    rho_sdr_minus_one: f64,
    log_rho_sdr: f64,
}

impl Default for MethodA {
    fn default() -> Self {
        Self::new()
    }
}

impl MethodA {
    pub fn new() -> Self {
        // Table 2, tone mapping steps 1 and 3.
        let rho_hdr_minus_one = 32.0 * (SOURCE_PEAK_NITS / 10000.0).powf(1.0 / GAMMA);
        let rho_sdr_minus_one = 32.0 * (TARGET_PEAK_NITS / 10000.0).powf(1.0 / GAMMA);
        Self {
            rho_hdr_minus_one,
            log_rho_hdr: rho_hdr_minus_one.ln_1p(),
            rho_sdr_minus_one,
            log_rho_sdr: rho_sdr_minus_one.ln_1p(),
        }
    }

    pub fn validate_input(rgb: LinearBt2020RgbNits) -> Result<(), ToneMapError> {
        for value in [rgb.r, rgb.g, rgb.b] {
            if !value.is_finite() {
                return Err(ToneMapError::NonFiniteInput);
            }
            if value < 0.0 {
                return Err(ToneMapError::NegativeInput);
            }
            if value > SOURCE_PEAK_NITS {
                return Err(ToneMapError::AboveSourcePeak);
            }
        }
        Ok(())
    }

    pub fn map(&self, rgb: LinearBt2020RgbNits) -> Result<MethodAOutput, ToneMapError> {
        Self::validate_input(rgb)?;
        // Table 2 nonlinear transform. Input units are nits; its RGB variables
        // are normalized display light, not PQ code values.
        let r = nonlinear_display_light(rgb.r);
        let g = nonlinear_display_light(rgb.g);
        let b = nonlinear_display_light(rgb.b);
        let y = KR * r + KG * g + KB * b;
        if y == 0.0 {
            // Table 3's scale is 0/0 at black. Its continuous black limit is
            // zero chroma and zero luma; this is a singularity guard, not clamp.
            return Ok(MethodAOutput {
                rgb: SdrBt2020NonlinearRgb {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                },
                ycbcr: SdrBt2020Ycbcr {
                    y: 0.0,
                    cb: 0.0,
                    cr: 0.0,
                },
                mapped_luma: 0.0,
            });
        }
        // Table 2 steps 1–3. ln_1p/exp_m1 retain near-black precision without
        // changing the published mathematical functions or knee constants.
        let perceptual = (self.rho_hdr_minus_one * y).ln_1p() / self.log_rho_hdr;
        let compressed = knee(perceptual);
        let mapped_luma = (compressed * self.log_rho_sdr).exp_m1() / self.rho_sdr_minus_one;
        // Table 3: complete color scaling AND the asymmetric luma correction.
        let scale = mapped_luma / (1.1 * y);
        let cb = scale * (b - y) / CB_DENOMINATOR;
        let cr = scale * (r - y) / CR_DENOMINATOR;
        let corrected_y = mapped_luma - (0.1 * cr).max(0.0);
        let ycbcr = SdrBt2020Ycbcr {
            y: corrected_y,
            cb,
            cr,
        };
        // Invert BT.2020-2 Table 4 NCL exactly using its luma coefficients,
        // rather than rounded inverse-G coefficients from another operator.
        let r = corrected_y + CR_DENOMINATOR * cr;
        let b = corrected_y + CB_DENOMINATOR * cb;
        let g = (corrected_y - KR * r - KB * b) / KG;
        if [r, g, b, corrected_y, cb, cr, mapped_luma]
            .iter()
            .any(|v| !v.is_finite())
        {
            return Err(ToneMapError::NonFiniteOutput);
        }
        Ok(MethodAOutput {
            rgb: SdrBt2020NonlinearRgb { r, g, b },
            ycbcr,
            mapped_luma,
        })
    }
}

fn nonlinear_display_light(nits: f64) -> f64 {
    if nits > 0.0 && nits < f64::MIN_POSITIVE * SOURCE_PEAK_NITS {
        // Algebraically identical Table 2 transform. Taking the root before
        // division avoids losing representable near-black output when the
        // normalized input would underflow or become a low-precision subnormal.
        nits.powf(1.0 / GAMMA) / SOURCE_PEAK_NITS.powf(1.0 / GAMMA)
    } else {
        (nits / SOURCE_PEAK_NITS).powf(1.0 / GAMMA)
    }
}

/// BT.2446-1 Table 2 step 2. Preserve the printed thresholds and their small
/// upward discontinuities; do not silently make the rounded curve continuous.
fn knee(p: f64) -> f64 {
    if p <= 0.7399 {
        1.0770 * p
    } else if p < 0.9909 {
        -1.1510 * p * p + 2.7811 * p - 0.6302
    } else {
        0.5000 * p + 0.5000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral(nits: f64) -> LinearBt2020RgbNits {
        LinearBt2020RgbNits {
            r: nits,
            g: nits,
            b: nits,
        }
    }

    #[test]
    fn published_table2_knee_reference_points_and_boundaries() {
        // Independent algebraic reference points from the printed Table 2,
        // not golden values emitted by MethodA::map.
        for (p, expected) in [
            (0.0, 0.0),
            (0.7399, 0.7968723),
            (0.9909, 0.99545),
            (1.0, 1.0),
        ] {
            assert!((knee(p) - expected).abs() < 2e-16);
        }
        for threshold in [0.7399_f64, 0.9909_f64] {
            assert!(knee(threshold.next_down()) <= knee(threshold));
            assert!(knee(threshold) <= knee(threshold.next_up()));
        }
    }

    #[test]
    fn independently_evaluated_decimal_neutral_vectors() {
        // Separate 70-decimal-digit arithmetic applied to §4.1 Table 2.
        // These are equation-derived checks, NOT tabulated ITU measurements.
        let operator = MethodA::new();
        for (nits, encoded) in [
            (0.0, 0.0),
            (0.0001, 0.0022879651105385244),
            (1.0, 0.09847613594371645),
            (10.0, 0.23616322946978098),
            (100.0, 0.5387478715633771),
            (203.0, 0.6868550439770996),
            (400.0, 0.8327736177435183),
            (1000.0, 1.0),
        ] {
            let output = operator.map(neutral(nits)).unwrap();
            for value in [output.rgb.r, output.rgb.g, output.rgb.b, output.mapped_luma] {
                assert!((value - encoded).abs() < 3e-15, "{nits}: {output:?}");
            }
            assert!(output.ycbcr.cb.abs() < 2e-16 && output.ycbcr.cr.abs() < 2e-16);
            assert!(output.rgb.r >= 0.0 && output.rgb.r <= 1.0 + 1e-15);
        }
    }

    #[test]
    fn neutral_ramp_is_monotone_and_highlight_contrast_is_compressed() {
        let operator = MethodA::new();
        let mut previous = 0.0;
        for step in 0..=100_000 {
            let output = operator.map(neutral(f64::from(step) / 100.0)).unwrap();
            assert!(output.mapped_luma >= previous);
            previous = output.mapped_luma;
        }
        let display_nits =
            |nits| TARGET_PEAK_NITS * operator.map(neutral(nits)).unwrap().mapped_luma.powf(GAMMA);
        assert!(display_nits(1000.0) <= 100.0 + 1e-12);
        assert!(
            display_nits(1000.0) - display_nits(400.0) < display_nits(400.0) - display_nits(100.0)
        );
    }

    #[test]
    fn table3_colour_correction_preserves_unclipped_saturated_vectors() {
        let operator = MethodA::new();
        let vectors = [
            (
                [1000.0, 0.0, 0.0],
                [
                    1.3238360361464537,
                    -0.03217821825699381,
                    -0.03217821825699381,
                ],
            ),
            (
                [0.0, 1000.0, 0.0],
                [
                    0.0753988758910186,
                    1.187_476_691_392_768,
                    0.0753988758910186,
                ],
            ),
            (
                [0.0, 0.0, 1000.0],
                [
                    0.009407584854551268,
                    0.009407584854551268,
                    1.5958468520638714,
                ],
            ),
            (
                [0.0, 1000.0, 1000.0],
                [0.07911585065018452, 1.1521641437491202, 1.1521641437491202],
            ),
            (
                [1000.0, 0.0, 1000.0],
                [1.2966752300159296, -0.01811594281475183, 1.2966752300159296],
            ),
            (
                [1000.0, 1000.0, 0.0],
                [1.0289818963228284, 1.0289818963228284, 0.08500387166581628],
            ),
            (
                [203.0, 120.0, 80.0],
                [0.6914472409514547, 0.5648492740798685, 0.4844915425155378],
            ),
        ];
        for ([r, g, b], expected) in vectors {
            let output = operator.map(LinearBt2020RgbNits { r, g, b }).unwrap();
            for (value, expected) in [output.rgb.r, output.rgb.g, output.rgb.b]
                .into_iter()
                .zip(expected)
            {
                assert!((value - expected).abs() < 5e-15, "{output:?}");
            }
        }
        let red = operator
            .map(LinearBt2020RgbNits {
                r: 1000.0,
                g: 0.0,
                b: 0.0,
            })
            .unwrap();
        assert!((red.ycbcr.y - 0.324_046_726_374_791_8).abs() < 2e-15);
        assert!((red.ycbcr.cb + 0.1893403553905526).abs() < 2e-15);
        assert!((red.ycbcr.cr - 0.6780071272017237).abs() < 2e-15);
    }

    #[test]
    fn invalid_source_is_rejected_not_clamped_or_generalized() {
        let operator = MethodA::new();
        for (value, error) in [
            (f64::NAN, ToneMapError::NonFiniteInput),
            (f64::INFINITY, ToneMapError::NonFiniteInput),
            (-0.01, ToneMapError::NegativeInput),
            (1000.0_f64.next_up(), ToneMapError::AboveSourcePeak),
            (4000.0, ToneMapError::AboveSourcePeak),
            (10000.0, ToneMapError::AboveSourcePeak),
        ] {
            assert_eq!(operator.map(neutral(value)), Err(error));
        }
        assert!(
            operator
                .map(neutral(f64::MIN_POSITIVE))
                .unwrap()
                .rgb
                .r
                .is_finite()
        );
        let smallest = operator.map(neutral(f64::from_bits(1))).unwrap();
        assert!(smallest.rgb.r > 0.0 && smallest.rgb.r.is_finite());
        assert!((smallest.rgb.r / 2.0705005056e-136 - 1.0).abs() < 1e-9);
    }
}
