//! BT.2020 non-constant-luminance / PQ pixel mathematics for the internal CPU oracle.
//! Linear components are display-referred absolute cd/m², not relative RGB codes.

pub const KR: f64 = 0.2627;
pub const KB: f64 = 0.0593;
pub const KG: f64 = 1.0 - KR - KB;
pub const PEAK_NITS: f64 = 10_000.0;
const M1: f64 = 2610.0 / 16384.0;
const M2: f64 = (2523.0 / 4096.0) * 128.0;
const C1: f64 = 3424.0 / 4096.0;
const C2: f64 = (2413.0 / 4096.0) * 32.0;
const C3: f64 = (2392.0 / 4096.0) * 32.0;

pub const Y_MIN: u16 = 64;
pub const Y_MAX: u16 = 940;
pub const C_MIN: u16 = 64;
pub const C_MAX: u16 = 960;
pub const C_NEUTRAL: u16 = 512;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PqRgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearRgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl LinearRgb {
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
    };
    pub fn luminance(self) -> f64 {
        KR * self.r + KG * self.g + KB * self.b
    }
    pub fn blend(self, background: Self, coverage: u8) -> Self {
        let a = f64::from(coverage) / 255.0;
        Self {
            r: self.r * a + background.r * (1.0 - a),
            g: self.g * a + background.g * (1.0 - a),
            b: self.b * a + background.b * (1.0 - a),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ycbcr {
    pub y: f64,
    pub cb: f64,
    pub cr: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct P010Codes {
    pub y: u16,
    pub cb: u16,
    pub cr: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelError {
    NonFinite,
    OutOfRange,
}

impl std::fmt::Display for PixelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HDR pixel is {:?}", self)
    }
}
impl std::error::Error for PixelError {}

pub fn pq_eotf_nits(code: f64) -> Result<f64, PixelError> {
    if !code.is_finite() {
        return Err(PixelError::NonFinite);
    }
    if !(0.0..=1.0).contains(&code) {
        return Err(PixelError::OutOfRange);
    }
    let p = code.powf(1.0 / M2);
    Ok(PEAK_NITS * ((p - C1).max(0.0) / (C2 - C3 * p)).powf(1.0 / M1))
}

pub fn pq_inverse_eotf(nits: f64) -> Result<f64, PixelError> {
    if !nits.is_finite() {
        return Err(PixelError::NonFinite);
    }
    if !(0.0..=PEAK_NITS).contains(&nits) {
        return Err(PixelError::OutOfRange);
    }
    let p = (nits / PEAK_NITS).powf(M1);
    Ok(((C1 + C2 * p) / (1.0 + C3 * p)).powf(M2))
}

pub fn pq_to_linear(rgb: PqRgb) -> Result<LinearRgb, PixelError> {
    Ok(LinearRgb {
        r: pq_eotf_nits(rgb.r)?,
        g: pq_eotf_nits(rgb.g)?,
        b: pq_eotf_nits(rgb.b)?,
    })
}

pub fn linear_to_pq(rgb: LinearRgb) -> Result<PqRgb, PixelError> {
    Ok(PqRgb {
        r: pq_inverse_eotf(rgb.r)?,
        g: pq_inverse_eotf(rgb.g)?,
        b: pq_inverse_eotf(rgb.b)?,
    })
}

pub fn ycbcr_to_pq(v: Ycbcr) -> PqRgb {
    let r = v.y + 2.0 * (1.0 - KR) * v.cr;
    let b = v.y + 2.0 * (1.0 - KB) * v.cb;
    PqRgb {
        r,
        g: (v.y - KR * r - KB * b) / KG,
        b,
    }
}

pub fn pq_to_ycbcr(v: PqRgb) -> Ycbcr {
    let y = KR * v.r + KG * v.g + KB * v.b;
    Ycbcr {
        y,
        cb: (v.b - y) / (2.0 * (1.0 - KB)),
        cr: (v.r - y) / (2.0 * (1.0 - KR)),
    }
}

pub fn decode_limited(c: P010Codes) -> Result<Ycbcr, PixelError> {
    if !(Y_MIN..=Y_MAX).contains(&c.y)
        || !(C_MIN..=C_MAX).contains(&c.cb)
        || !(C_MIN..=C_MAX).contains(&c.cr)
    {
        return Err(PixelError::OutOfRange);
    }
    Ok(Ycbcr {
        y: f64::from(c.y - Y_MIN) / 876.0,
        cb: (f64::from(c.cb) - f64::from(C_NEUTRAL)) / 896.0,
        cr: (f64::from(c.cr) - f64::from(C_NEUTRAL)) / 896.0,
    })
}

/// Round to nearest, ties away from zero; count each component clamped to legal range.
pub fn encode_limited(v: Ycbcr) -> Result<(P010Codes, u32), PixelError> {
    fn q(v: f64, lo: u16, hi: u16) -> Result<(u16, u32), PixelError> {
        if !v.is_finite() {
            return Err(PixelError::NonFinite);
        }
        let rounded = v.round();
        let clamped = rounded.clamp(f64::from(lo), f64::from(hi));
        Ok((clamped as u16, u32::from(rounded != clamped)))
    }
    let (y, a) = q(64.0 + 876.0 * v.y, Y_MIN, Y_MAX)?;
    let (cb, b) = q(512.0 + 896.0 * v.cb, C_MIN, C_MAX)?;
    let (cr, c) = q(512.0 + 896.0 * v.cr, C_MIN, C_MAX)?;
    Ok((P010Codes { y, cb, cr }, a + b + c))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_pq_vectors() {
        // ITU-R BT.2100 Table 4, evaluated independently and pinned here.
        for (nits, code) in [
            (0.0, 0.000000730955902578),
            (100.0, 0.508078421517),
            (1000.0, 0.751827096247),
            (10000.0, 1.0),
        ] {
            assert!((pq_inverse_eotf(nits).unwrap() - code).abs() < 1e-10);
            if nits > 0.0 {
                assert!((pq_eotf_nits(code).unwrap() - nits).abs() < 1e-4);
            }
        }
        assert_eq!(pq_eotf_nits(0.0).unwrap(), 0.0);
    }
    #[test]
    fn matrix_and_limited_roundtrip() {
        for rgb in [
            PqRgb {
                r: 0.0,
                g: 0.0,
                b: 0.0,
            },
            PqRgb {
                r: 0.2,
                g: 0.6,
                b: 0.9,
            },
            PqRgb {
                r: 1.0,
                g: 1.0,
                b: 1.0,
            },
        ] {
            let back = ycbcr_to_pq(pq_to_ycbcr(rgb));
            assert!((rgb.r - back.r).abs() < 1e-14);
            assert!((rgb.g - back.g).abs() < 1e-14);
            assert!((rgb.b - back.b).abs() < 1e-14);
        }
        let black = P010Codes {
            y: 64,
            cb: 512,
            cr: 512,
        };
        assert_eq!(
            encode_limited(decode_limited(black).unwrap()).unwrap().0,
            black
        );
        assert_eq!(
            encode_limited(Ycbcr {
                y: 0.5,
                cb: 0.0,
                cr: 0.0
            })
            .unwrap()
            .0
            .y,
            502
        );
        assert!(decode_limited(P010Codes { y: 63, ..black }).is_err());
        // RGB↔YCbCr float math must not expand a legal quantized color by
        // more than one ten-bit code in a diagnostic identity pass.
        for codes in [
            P010Codes {
                y: 300,
                cb: 400,
                cr: 600,
            },
            P010Codes {
                y: 700,
                cb: 550,
                cr: 450,
            },
            P010Codes {
                y: 512,
                cb: 512,
                cr: 512,
            },
        ] {
            let rgb = ycbcr_to_pq(decode_limited(codes).unwrap());
            let (back, _) = encode_limited(pq_to_ycbcr(rgb)).unwrap();
            assert!(codes.y.abs_diff(back.y) <= 1);
            assert!(codes.cb.abs_diff(back.cb) <= 1);
            assert!(codes.cr.abs_diff(back.cr) <= 1);
        }
    }
    #[test]
    fn linear_light_is_not_pq_code_average() {
        let a = pq_inverse_eotf(1000.0).unwrap();
        assert!((pq_inverse_eotf(500.0).unwrap() - a / 2.0).abs() > 0.1);
        assert!(
            (LinearRgb {
                r: 1000.0,
                g: 0.0,
                b: 0.0
            }
            .blend(LinearRgb::BLACK, 128)
            .r - 1000.0 * 128.0 / 255.0)
                .abs()
                < 1e-10
        );
    }
    #[test]
    fn pq_dense_roundtrips_and_edges_are_finite() {
        for step in 0..=10_000 {
            let code = f64::from(step) / 10_000.0;
            let nits = pq_eotf_nits(code).unwrap();
            assert!(nits.is_finite() && nits >= 0.0);
            let restored = pq_inverse_eotf(nits).unwrap();
            // ST 2084 maps a tiny interval of codes to exactly black.
            if nits > 0.0 {
                assert!((restored - code).abs() < 1e-10);
            }
            let nits = f64::from(step);
            assert!((pq_eotf_nits(pq_inverse_eotf(nits).unwrap()).unwrap() - nits).abs() < 1e-6);
        }
        for bad in [f64::NAN, f64::INFINITY, -1.0, 10_001.0] {
            assert!(pq_inverse_eotf(bad).is_err());
        }
    }
    #[test]
    fn legal_codes_and_rounding_are_explicit() {
        let white = P010Codes {
            y: Y_MAX,
            cb: C_NEUTRAL,
            cr: C_NEUTRAL,
        };
        assert_eq!(
            encode_limited(decode_limited(white).unwrap()).unwrap().0,
            white
        );
        assert_eq!(
            encode_limited(Ycbcr {
                y: 0.5 / 876.0,
                cb: 0.0,
                cr: 0.0
            })
            .unwrap()
            .0
            .y,
            65
        );
        assert_eq!(
            encode_limited(Ycbcr {
                y: 2.0,
                cb: 0.0,
                cr: 0.0
            })
            .unwrap()
            .1,
            1
        );
    }
}
