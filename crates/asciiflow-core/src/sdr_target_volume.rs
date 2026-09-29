//! Internal C-2B f64 display-referred SDR reference, never a production path.
//!
//! Two separate operations: standards-derived primary conversion (BT.2407 §2)
//! and explicit AsciiFlow target-capability clipping. The latter is NOT Annex5,
//! luminance/hue preserving, reversible, or perceptually optimal.
//! All linear components are normalized: 1 = 100 cd/m² reference capability.
//! Signed power2.4 is the explicit C-2B boundary for raw C-1 nonlinear values,
//! not a change to MethodA and not the BT.709 camera OETF. BT.2087 Annex1 Case1
//! Note2 permits appropriate sign treatment; BT.2408-9 §5.3 describes reflected
//! extensions. The precise signed extension is our engineering choice.
use crate::tone_map_bt2446::SdrBt2020NonlinearRgb;

pub const TARGET_WHITE_NITS: f64 = 100.0;
pub const MATRIX_ID: &str = "D65-xy-derived-BT2020-2-BT709-6-v1";
/// Decimal70 derivation from BT.2020-2 / BT.709-6 chromaticities, D65(.3127,.3290).
/// Full derivation and independent XYZ vectors: tests/fixtures/tone-map/c2b-vectors.json.
pub const BT2020_TO_XYZ: [[f64; 3]; 3] = [
    [0.6369580483012913, 0.14461690358620837, 0.16888097516417205],
    [0.26270021201126703, 0.677998071518871, 0.059301716469861946],
    [0.0, 0.028072693049087508, 1.0609850577107909],
];
pub const BT709_TO_XYZ: [[f64; 3]; 3] = [
    [0.4123907992659595, 0.35758433938387796, 0.1804807884018343],
    [0.21263900587151036, 0.7151686787677559, 0.07219231536073371],
    [0.01933081871559185, 0.11919477979462599, 0.9505321522496606],
];
pub const XYZ_TO_BT709: [[f64; 3]; 3] = [
    [3.2409699419045213, -1.5373831775700935, -0.4986107602930033],
    [-0.9692436362808798, 1.8759675015077207, 0.04155505740717561],
    [
        0.05563007969699361,
        -0.20397695888897657,
        1.0569715142428786,
    ],
];
pub const BT2020_TO_BT709: [[f64; 3]; 3] = [
    [
        1.6604910021084345,
        -0.5876411387885495,
        -0.07284986331988488,
    ],
    [
        -0.12455047452159074,
        1.1328998971259603,
        -0.008349422604369477,
    ],
    [
        -0.018150763354905303,
        -0.10057889800800739,
        1.1187296613629127,
    ],
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearBt2020SdrRgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearBt709SdrRgbUnbounded {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Xyz {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearBt709SdrRgbBounded([f64; 3]);
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NonlinearBt709SdrRgb([f64; 3]);

impl LinearBt2020SdrRgb {
    pub fn components(self) -> [f64; 3] {
        [self.r, self.g, self.b]
    }
}
impl LinearBt709SdrRgbUnbounded {
    pub fn components(self) -> [f64; 3] {
        [self.r, self.g, self.b]
    }
}
impl LinearBt709SdrRgbBounded {
    pub fn from_components(rgb: [f64; 3]) -> Result<Self, ConversionError> {
        validate_cube(rgb)?;
        Ok(Self(rgb))
    }
    pub fn components(self) -> [f64; 3] {
        self.0
    }
}
impl NonlinearBt709SdrRgb {
    pub fn from_components(rgb: [f64; 3]) -> Result<Self, ConversionError> {
        validate_cube(rgb)?;
        Ok(Self(rgb))
    }
    pub fn components(self) -> [f64; 3] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConversionError {
    NonFiniteInput,
    NonFiniteOutput,
    OutsideTargetCube,
}
impl std::fmt::Display for ConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NonFiniteInput => "non-finite SDR reference input",
            Self::NonFiniteOutput => "non-finite SDR reference arithmetic result",
            Self::OutsideTargetCube => "bounded target value outside [0,1]",
        })
    }
}
impl std::error::Error for ConversionError {}

fn validate_finite(rgb: [f64; 3]) -> Result<(), ConversionError> {
    if rgb.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(ConversionError::NonFiniteInput)
    }
}
fn validate_cube(rgb: [f64; 3]) -> Result<(), ConversionError> {
    validate_finite(rgb)?;
    if rgb.iter().all(|v| (0.0..=1.0).contains(v)) {
        Ok(())
    } else {
        Err(ConversionError::OutsideTargetCube)
    }
}

/// Sign-reflected zero-black display EOTF extension. Never source-clips.
pub fn c1_to_display_linear(
    input: SdrBt2020NonlinearRgb,
) -> Result<LinearBt2020SdrRgb, ConversionError> {
    let values = [input.r, input.g, input.b];
    validate_finite(values)?;
    let result = values.map(|v| {
        if v == 0.0 {
            0.0 // Explicit transfer-zero policy; avoid pow/sign ambiguity.
        } else {
            v.abs().powf(2.4).copysign(v)
        }
    });
    if !result.iter().all(|v| v.is_finite()) {
        return Err(ConversionError::NonFiniteOutput);
    }
    Ok(LinearBt2020SdrRgb {
        r: result[0],
        g: result[1],
        b: result[2],
    })
}

fn multiply(matrix: [[f64; 3]; 3], rgb: [f64; 3]) -> Result<[f64; 3], ConversionError> {
    validate_finite(rgb)?;
    let result = matrix.map(|row| row[0] * rgb[0] + row[1] * rgb[1] + row[2] * rgb[2]);
    if result.iter().all(|v| v.is_finite()) {
        Ok(result)
    } else {
        Err(ConversionError::NonFiniteOutput)
    }
}
pub fn bt2020_to_xyz(input: LinearBt2020SdrRgb) -> Result<Xyz, ConversionError> {
    let [x, y, z] = multiply(BT2020_TO_XYZ, input.components())?;
    Ok(Xyz { x, y, z })
}
pub fn xyz_to_bt709(input: Xyz) -> Result<LinearBt709SdrRgbUnbounded, ConversionError> {
    let [r, g, b] = multiply(XYZ_TO_BT709, [input.x, input.y, input.z])?;
    Ok(LinearBt709SdrRgbUnbounded { r, g, b })
}
pub fn bt709_to_xyz(input: LinearBt709SdrRgbUnbounded) -> Result<Xyz, ConversionError> {
    let [x, y, z] = multiply(BT709_TO_XYZ, input.components())?;
    Ok(Xyz { x, y, z })
}

/// Pre-multiplied primary conversion, preserving an observable unbounded result.
/// Both systems share D65. Evaluate around the neutral axis (row sums are one
/// mathematically) to preserve neutrals/white exactly without snapping/clamping.
/// Independent tests also exercise the separate XYZ path, not just this matrix.
pub fn convert_bt2020_primaries(
    input: LinearBt2020SdrRgb,
) -> Result<LinearBt709SdrRgbUnbounded, ConversionError> {
    validate_finite(input.components())?;
    let result = BT2020_TO_BT709
        .map(|row| input.g + row[0] * (input.r - input.g) + row[2] * (input.b - input.g));
    if !result.iter().all(|v| v.is_finite()) {
        return Err(ConversionError::NonFiniteOutput);
    }
    Ok(LinearBt709SdrRgbUnbounded {
        r: result[0],
        g: result[1],
        b: result[2],
    })
}

/// Explicit project policy in TARGET display-linear RGB; no source pre-limit,
/// hue rotation, chroma projection, common scaling, or post-clip Y restoration.
/// Finite interior values (including signed zero) are returned bit-identically.
pub fn clip_to_bt709_target_cube(
    input: LinearBt709SdrRgbUnbounded,
) -> Result<LinearBt709SdrRgbBounded, ConversionError> {
    validate_finite(input.components())?;
    Ok(LinearBt709SdrRgbBounded(
        input.components().map(|v| v.clamp(0.0, 1.0)),
    ))
}

/// Inverse zero-black display power, NOT the BT.709 camera OETF. No quantization,
/// metadata, full/limited code range, YCbCr or production pixel format is selected.
pub fn bt709_to_nonlinear(input: LinearBt709SdrRgbBounded) -> NonlinearBt709SdrRgb {
    NonlinearBt709SdrRgb(input.0.map(|v| v.powf(1.0 / 2.4)))
}
pub fn bt709_signal_to_linear(input: NonlinearBt709SdrRgb) -> LinearBt709SdrRgbBounded {
    LinearBt709SdrRgbBounded(input.0.map(|v| v.powf(2.4)))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SdrConversionOutput {
    pub source_linear: LinearBt2020SdrRgb,
    pub unbounded: LinearBt709SdrRgbUnbounded,
    pub bounded: LinearBt709SdrRgbBounded,
    pub nonlinear: NonlinearBt709SdrRgb,
}
pub fn convert_method_a_rgb(
    input: SdrBt2020NonlinearRgb,
) -> Result<SdrConversionOutput, ConversionError> {
    let source_linear = c1_to_display_linear(input)?;
    let unbounded = convert_bt2020_primaries(source_linear)?;
    let bounded = clip_to_bt709_target_cube(unbounded)?;
    let nonlinear = bt709_to_nonlinear(bounded);
    Ok(SdrConversionOutput {
        source_linear,
        unbounded,
        bounded,
        nonlinear,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(values: [f64; 3]) -> LinearBt709SdrRgbUnbounded {
        LinearBt709SdrRgbUnbounded {
            r: values[0],
            g: values[1],
            b: values[2],
        }
    }
    #[test]
    fn cube_boundaries_interior_identity_idempotence_and_monotonicity() {
        let values = [
            -1.0,
            -f64::EPSILON,
            -0.0,
            0.0,
            f64::EPSILON,
            0.25,
            0.5,
            1.0 - f64::EPSILON,
            1.0,
            1.0 + f64::EPSILON,
            2.0,
        ];
        for &r in &values {
            for &g in &values {
                for &b in &values {
                    let before = [r, g, b];
                    let result = clip_to_bt709_target_cube(rgb(before)).unwrap().components();
                    assert_eq!(
                        clip_to_bt709_target_cube(rgb(result)).unwrap().components(),
                        result
                    );
                    for (x, y) in before.into_iter().zip(result) {
                        assert!(y.is_finite() && (0.0..=1.0).contains(&y));
                        if (0.0..=1.0).contains(&x) {
                            assert_eq!(x.to_bits(), y.to_bits());
                        } else {
                            assert_eq!(y, if x < 0.0 { 0.0 } else { 1.0 });
                        }
                    }
                }
            }
        }
        for pair in values.windows(2) {
            let a = clip_to_bt709_target_cube(rgb([pair[0]; 3])).unwrap();
            let b = clip_to_bt709_target_cube(rgb([pair[1]; 3])).unwrap();
            assert!(a.components()[0] <= b.components()[0]);
        }
    }
    #[test]
    fn nonfinite_rejected_before_clipping_and_overflow_rejected() {
        for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for i in 0..3 {
                let mut values = [0.5; 3];
                values[i] = v;
                assert!(clip_to_bt709_target_cube(rgb(values)).is_err());
                assert!(LinearBt709SdrRgbBounded::from_components(values).is_err());
                assert!(NonlinearBt709SdrRgb::from_components(values).is_err());
                assert!(
                    c1_to_display_linear(SdrBt2020NonlinearRgb {
                        r: values[0],
                        g: values[1],
                        b: values[2]
                    })
                    .is_err()
                );
                assert!(
                    convert_bt2020_primaries(LinearBt2020SdrRgb {
                        r: values[0],
                        g: values[1],
                        b: values[2]
                    })
                    .is_err()
                );
            }
        }
        assert!(
            c1_to_display_linear(SdrBt2020NonlinearRgb {
                r: f64::MAX,
                g: 0.0,
                b: 0.0
            })
            .is_err()
        );
    }
    #[test]
    fn neutral_axis_exact_and_no_source_preclamp() {
        for v in [-2.0, 0.0, 0.1, 0.5, 1.0, 2.0] {
            let input = LinearBt2020SdrRgb { r: v, g: v, b: v };
            assert_eq!(
                convert_bt2020_primaries(input).unwrap().components(),
                [v; 3]
            );
        }
        for v in [-0.01, 0.0, 0.18, 0.5, 1.0, 1.01] {
            let stages = convert_method_a_rgb(SdrBt2020NonlinearRgb { r: v, g: v, b: v }).unwrap();
            let unbounded = stages.unbounded.components();
            assert_eq!(unbounded, [stages.source_linear.r; 3]);
            let bounded = stages.bounded.components();
            assert_eq!(bounded, [unbounded[0].clamp(0.0, 1.0); 3]);
            let nonlinear = stages.nonlinear.components();
            assert_eq!(nonlinear, [nonlinear[0]; 3], "no neutral tint");
            if v <= 0.0 {
                assert_eq!(bounded, [0.0; 3]);
                assert_eq!(nonlinear, [0.0; 3]);
            } else if v >= 1.0 {
                assert_eq!(bounded, [1.0; 3]);
                assert_eq!(nonlinear, [1.0; 3]);
            }
        }
        let source = SdrBt2020NonlinearRgb {
            r: -0.1,
            g: 0.25,
            b: 1.3,
        };
        let stages = convert_method_a_rgb(source).unwrap();
        assert!(stages.source_linear.r < 0.0 && stages.source_linear.b > 1.0);
        let preclipped = convert_method_a_rgb(SdrBt2020NonlinearRgb {
            r: 0.0,
            g: 0.25,
            b: 1.0,
        })
        .unwrap();
        assert_ne!(stages.unbounded, preclipped.unbounded);
    }
    #[test]
    fn target_display_transfer_roundtrip() {
        for v in [0.0, 1e-12, 0.001, 0.01, 0.18, 0.5, 0.9, 1.0] {
            let linear = LinearBt709SdrRgbBounded::from_components([v; 3]).unwrap();
            let returned = bt709_signal_to_linear(bt709_to_nonlinear(linear));
            assert!((returned.components()[0] - v).abs() < 5e-16);
        }
    }
}
