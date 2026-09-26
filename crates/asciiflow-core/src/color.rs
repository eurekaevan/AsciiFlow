//! Portable, signaled color facts and the deliberately narrow SDR policy.
//!
//! Pixel format and bit depth are not inputs to dynamic-range classification.

use crate::{ColorMatrix, ColorPrimaries, ColorRange, ColorSpace, TransferCharacteristic};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorRational {
    pub numerator: i32,
    pub denominator: i32,
}

impl ColorRational {
    pub fn is_nonnegative(self) -> bool {
        self.numerator >= 0 && self.denominator > 0
    }

    pub fn is_unit_coordinate(self) -> bool {
        self.is_nonnegative() && self.numerator <= self.denominator
    }

    fn less_than(self, other: Self) -> bool {
        i64::from(self.numerator) * i64::from(other.denominator)
            < i64::from(other.numerator) * i64::from(self.denominator)
    }
}

/// Coordinates are CIE 1931 xy; luminances are cd/m². Rationals retain the
/// exact FFmpeg numerator/denominator rather than rounding through `f32`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MasteringDisplayMetadata {
    pub display_primaries: Option<[[ColorRational; 2]; 3]>,
    pub white_point: Option<[ColorRational; 2]>,
    pub min_luminance: Option<ColorRational>,
    pub max_luminance: Option<ColorRational>,
}

impl MasteringDisplayMetadata {
    pub fn validate(self) -> Result<Self, ColorError> {
        if let Some(primaries) = self.display_primaries {
            if primaries.iter().flatten().any(|v| !v.is_unit_coordinate())
                || self
                    .white_point
                    .is_none_or(|point| point.iter().any(|v| !v.is_unit_coordinate()))
            {
                return Err(ColorError::MalformedMasteringMetadata);
            }
        } else if self.white_point.is_some() {
            return Err(ColorError::MalformedMasteringMetadata);
        }
        if let (Some(min), Some(max)) = (self.min_luminance, self.max_luminance) {
            if !min.is_nonnegative()
                || !max.is_nonnegative()
                || max.numerator == 0
                || max.less_than(min)
            {
                return Err(ColorError::MalformedMasteringMetadata);
            }
        } else if self.min_luminance.is_some() || self.max_luminance.is_some() {
            return Err(ColorError::MalformedMasteringMetadata);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContentLightLevelMetadata {
    /// cd/m²; zero/absent values are represented by `None`.
    pub max_cll: Option<u32>,
    pub max_fall: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorMetadataRaw {
    pub space: ColorSpace,
    pub mastering_display: Option<MasteringDisplayMetadata>,
    pub content_light: Option<ContentLightLevelMetadata>,
}

impl ColorMetadataRaw {
    pub const fn unspecified() -> Self {
        Self {
            space: ColorSpace {
                matrix: ColorMatrix::Unspecified,
                range: ColorRange::Unspecified,
                primaries: ColorPrimaries::Unspecified,
                transfer: TransferCharacteristic::Unspecified,
                chroma_location: crate::ChromaLocation::Unspecified,
            },
            mastering_display: None,
            content_light: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorProvenance {
    Frame,
    Stream,
    LegacyDefault,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorFieldProvenance {
    pub primaries: ColorProvenance,
    pub transfer: ColorProvenance,
    pub matrix: ColorProvenance,
    pub range: ColorProvenance,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicRangeClass {
    Sdr,
    HdrPq,
    HdrHlg,
    Unknown,
    Conflicting,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorSupportReason {
    UnsupportedHdrPq,
    UnsupportedHdrHlg,
    UnsupportedWideGamutSdr,
    Unknown,
    Conflicting,
    UnsupportedSdrProfile,
    UnsupportedFullRange,
}

impl std::fmt::Display for ColorSupportReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::UnsupportedHdrPq => {
                "HDR PQ input detected (SMPTE ST 2084); HDR processing is not implemented"
            }
            Self::UnsupportedHdrHlg => {
                "HDR HLG input detected (ARIB STD-B67); HDR processing is not implemented"
            }
            Self::UnsupportedWideGamutSdr => {
                "wide-gamut SDR input detected; BT.2020/P3 processing is not implemented"
            }
            Self::Unknown => "color semantics cannot be resolved safely",
            Self::Conflicting => "conflicting color metadata cannot be processed",
            Self::UnsupportedSdrProfile => {
                "SDR color profile is outside the qualified processing contract"
            }
            Self::UnsupportedFullRange => {
                "full-range input is not qualified: the ASCII renderer emits limited-range code values"
            }
        };
        f.write_str(message)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorError {
    MidStreamColorMetadataChange,
    MalformedMasteringMetadata,
    MalformedContentLightMetadata,
}

impl std::fmt::Display for ColorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MidStreamColorMetadataChange => {
                write!(f, "decoded color metadata changed after the first frame")
            }
            Self::MalformedMasteringMetadata => write!(f, "malformed mastering display metadata"),
            Self::MalformedContentLightMetadata => write!(f, "malformed content light metadata"),
        }
    }
}

impl std::error::Error for ColorError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorResolutionPolicy {
    /// Preserve the Stage 5.2 NV12 conversion's BT.709/limited output default.
    LegacyEightBit,
    /// P010 requires explicitly signaled BT.709 SDR fields.
    StrictTenBit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedColorSemantics {
    pub stream: ColorMetadataRaw,
    pub frame: ColorMetadataRaw,
    pub effective: ColorSpace,
    pub provenance: ColorFieldProvenance,
    pub dynamic_range: DynamicRangeClass,
    pub support: Result<(), ColorSupportReason>,
}

impl ResolvedColorSemantics {
    pub fn effective_static_metadata(
        self,
    ) -> (
        Option<MasteringDisplayMetadata>,
        Option<ContentLightLevelMetadata>,
    ) {
        (
            self.frame
                .mastering_display
                .or(self.stream.mastering_display),
            self.frame.content_light.or(self.stream.content_light),
        )
    }

    pub fn ensure_stable(self, next: Self) -> Result<(), ColorError> {
        if self.effective != next.effective
            || self.dynamic_range != next.dynamic_range
            || self.effective_static_metadata() != next.effective_static_metadata()
        {
            Err(ColorError::MidStreamColorMetadataChange)
        } else {
            Ok(())
        }
    }

    pub fn resolve(
        stream: ColorMetadataRaw,
        frame: ColorMetadataRaw,
        policy: ColorResolutionPolicy,
    ) -> Result<Self, ColorError> {
        fn field<T: Copy + Eq>(
            stream: T,
            frame: T,
            unspecified: T,
            fallback: T,
            policy: ColorResolutionPolicy,
        ) -> (T, ColorProvenance, bool) {
            let conflict = stream != unspecified && frame != unspecified && stream != frame;
            let (value, source) = if frame != unspecified {
                (frame, ColorProvenance::Frame)
            } else if stream != unspecified {
                (stream, ColorProvenance::Stream)
            } else if policy == ColorResolutionPolicy::LegacyEightBit {
                (fallback, ColorProvenance::LegacyDefault)
            } else {
                (unspecified, ColorProvenance::Unknown)
            };
            (value, source, conflict)
        }
        let (primaries, primaries_source, primaries_conflict) = field(
            stream.space.primaries,
            frame.space.primaries,
            ColorPrimaries::Unspecified,
            ColorPrimaries::Bt709,
            policy,
        );
        let (transfer, transfer_source, transfer_conflict) = field(
            stream.space.transfer,
            frame.space.transfer,
            TransferCharacteristic::Unspecified,
            TransferCharacteristic::Bt709,
            policy,
        );
        let (matrix, matrix_source, matrix_conflict) = field(
            stream.space.matrix,
            frame.space.matrix,
            ColorMatrix::Unspecified,
            ColorMatrix::Bt709,
            policy,
        );
        let (range, range_source, range_conflict) = field(
            stream.space.range,
            frame.space.range,
            ColorRange::Unspecified,
            ColorRange::Limited,
            policy,
        );
        let (chroma_location, _, chroma_conflict) = field(
            stream.space.chroma_location,
            frame.space.chroma_location,
            crate::ChromaLocation::Unspecified,
            crate::ChromaLocation::Left,
            policy,
        );
        let effective = ColorSpace {
            primaries,
            transfer,
            matrix,
            range,
            chroma_location,
        };
        let provenance = ColorFieldProvenance {
            primaries: primaries_source,
            transfer: transfer_source,
            matrix: matrix_source,
            range: range_source,
        };
        let source_conflict = primaries_conflict
            || transfer_conflict
            || matrix_conflict
            || range_conflict
            || chroma_conflict
            || (stream.mastering_display.is_some()
                && frame.mastering_display.is_some()
                && stream.mastering_display != frame.mastering_display)
            || (stream.content_light.is_some()
                && frame.content_light.is_some()
                && stream.content_light != frame.content_light);
        let dynamic_range = if source_conflict {
            DynamicRangeClass::Conflicting
        } else {
            match transfer {
                TransferCharacteristic::Pq => DynamicRangeClass::HdrPq,
                TransferCharacteristic::Hlg => DynamicRangeClass::HdrHlg,
                TransferCharacteristic::Bt709
                | TransferCharacteristic::Smpte170M
                | TransferCharacteristic::Srgb
                | TransferCharacteristic::Gamma22
                | TransferCharacteristic::Gamma28
                    if primaries != ColorPrimaries::Unspecified
                        && matrix != ColorMatrix::Unspecified =>
                {
                    DynamicRangeClass::Sdr
                }
                _ => DynamicRangeClass::Unknown,
            }
        };
        let support = match dynamic_range {
            DynamicRangeClass::HdrPq
                if primaries == ColorPrimaries::Bt709
                    && matrix == ColorMatrix::Bt709
                    && !matches!(
                        primaries_source,
                        ColorProvenance::LegacyDefault | ColorProvenance::Unknown
                    )
                    && !matches!(
                        matrix_source,
                        ColorProvenance::LegacyDefault | ColorProvenance::Unknown
                    ) =>
            {
                Err(ColorSupportReason::Conflicting)
            }
            DynamicRangeClass::HdrHlg
                if primaries == ColorPrimaries::Bt709
                    && matrix == ColorMatrix::Bt709
                    && !matches!(
                        primaries_source,
                        ColorProvenance::LegacyDefault | ColorProvenance::Unknown
                    )
                    && !matches!(
                        matrix_source,
                        ColorProvenance::LegacyDefault | ColorProvenance::Unknown
                    ) =>
            {
                Err(ColorSupportReason::Conflicting)
            }
            DynamicRangeClass::HdrPq => Err(ColorSupportReason::UnsupportedHdrPq),
            DynamicRangeClass::HdrHlg => Err(ColorSupportReason::UnsupportedHdrHlg),
            DynamicRangeClass::Unknown => Err(ColorSupportReason::Unknown),
            DynamicRangeClass::Conflicting => Err(ColorSupportReason::Conflicting),
            DynamicRangeClass::Sdr if range == ColorRange::Full => {
                Err(ColorSupportReason::UnsupportedFullRange)
            }
            DynamicRangeClass::Sdr
                if matches!(
                    primaries,
                    ColorPrimaries::Bt2020 | ColorPrimaries::DisplayP3
                ) || matches!(matrix, ColorMatrix::Bt2020 | ColorMatrix::Bt2020Constant) =>
            {
                Err(ColorSupportReason::UnsupportedWideGamutSdr)
            }
            DynamicRangeClass::Sdr
                if primaries == ColorPrimaries::Bt709
                    && matrix == ColorMatrix::Bt709
                    && transfer == TransferCharacteristic::Bt709 =>
            {
                Ok(())
            }
            DynamicRangeClass::Sdr
                if policy == ColorResolutionPolicy::LegacyEightBit
                    && matrix == ColorMatrix::Bt601
                    && (matches!(
                        primaries,
                        ColorPrimaries::Smpte170M | ColorPrimaries::Bt470Bg
                    ) || primaries_source == ColorProvenance::LegacyDefault)
                    && matches!(
                        transfer,
                        TransferCharacteristic::Bt709
                            | TransferCharacteristic::Smpte170M
                            | TransferCharacteristic::Gamma28
                    ) =>
            {
                Ok(())
            }
            DynamicRangeClass::Sdr => Err(ColorSupportReason::UnsupportedSdrProfile),
        };
        Ok(Self {
            stream,
            frame,
            effective,
            provenance,
            dynamic_range,
            support,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_does_not_classify_hdr_and_wide_gamut_sdr_is_not_hdr() {
        let mut raw = ColorMetadataRaw {
            space: ColorSpace::default(),
            ..ColorMetadataRaw::unspecified()
        };
        let sdr = ResolvedColorSemantics::resolve(
            raw,
            ColorMetadataRaw::unspecified(),
            ColorResolutionPolicy::StrictTenBit,
        )
        .unwrap();
        assert_eq!(sdr.dynamic_range, DynamicRangeClass::Sdr);
        assert_eq!(sdr.support, Ok(()));
        raw.space.primaries = ColorPrimaries::Bt2020;
        raw.space.matrix = ColorMatrix::Bt2020;
        let wide = ResolvedColorSemantics::resolve(
            raw,
            ColorMetadataRaw::unspecified(),
            ColorResolutionPolicy::StrictTenBit,
        )
        .unwrap();
        assert_eq!(wide.dynamic_range, DynamicRangeClass::Sdr);
        assert_eq!(
            wide.support,
            Err(ColorSupportReason::UnsupportedWideGamutSdr)
        );
        raw.space.transfer = TransferCharacteristic::Pq;
        let pq = ResolvedColorSemantics::resolve(
            raw,
            ColorMetadataRaw::unspecified(),
            ColorResolutionPolicy::StrictTenBit,
        )
        .unwrap();
        assert_eq!(pq.dynamic_range, DynamicRangeClass::HdrPq);
        raw.space.transfer = TransferCharacteristic::Hlg;
        let hlg = ResolvedColorSemantics::resolve(
            raw,
            ColorMetadataRaw::unspecified(),
            ColorResolutionPolicy::StrictTenBit,
        )
        .unwrap();
        assert_eq!(hlg.dynamic_range, DynamicRangeClass::HdrHlg);
    }

    #[test]
    fn unspecified_and_conflicting_facts_are_not_silently_sdr() {
        let raw = ColorMetadataRaw::unspecified();
        let strict =
            ResolvedColorSemantics::resolve(raw, raw, ColorResolutionPolicy::StrictTenBit).unwrap();
        assert_eq!(strict.dynamic_range, DynamicRangeClass::Unknown);
        let legacy =
            ResolvedColorSemantics::resolve(raw, raw, ColorResolutionPolicy::LegacyEightBit)
                .unwrap();
        assert_eq!(legacy.provenance.transfer, ColorProvenance::LegacyDefault);
        let mut frame = raw;
        frame.space.transfer = TransferCharacteristic::Pq;
        let mut stream = raw;
        stream.space.transfer = TransferCharacteristic::Bt709;
        let conflicting =
            ResolvedColorSemantics::resolve(stream, frame, ColorResolutionPolicy::StrictTenBit)
                .unwrap();
        assert_eq!(conflicting.dynamic_range, DynamicRangeClass::Conflicting);
        assert_eq!(conflicting.support, Err(ColorSupportReason::Conflicting));
    }

    #[test]
    fn frame_fills_missing_stream_fields_without_erasing_raw_signal() {
        let mut stream = ColorMetadataRaw::unspecified();
        stream.space.transfer = TransferCharacteristic::Bt709;
        let mut frame = ColorMetadataRaw::unspecified();
        frame.space.primaries = ColorPrimaries::Bt709;
        frame.space.matrix = ColorMatrix::Bt709;
        frame.space.range = ColorRange::Limited;
        let resolved =
            ResolvedColorSemantics::resolve(stream, frame, ColorResolutionPolicy::StrictTenBit)
                .unwrap();
        assert_eq!(resolved.provenance.transfer, ColorProvenance::Stream);
        assert_eq!(resolved.provenance.primaries, ColorProvenance::Frame);
        assert_eq!(resolved.stream.space.primaries, ColorPrimaries::Unspecified);
        assert_eq!(resolved.effective.primaries, ColorPrimaries::Bt709);
    }

    #[test]
    fn mid_stream_sdr_to_pq_change_is_rejected() {
        let first = ColorMetadataRaw {
            space: ColorSpace::default(),
            ..ColorMetadataRaw::unspecified()
        };
        let first = ResolvedColorSemantics::resolve(
            first,
            ColorMetadataRaw::unspecified(),
            ColorResolutionPolicy::StrictTenBit,
        )
        .unwrap();
        let mut changed = ColorMetadataRaw::unspecified();
        changed.space.transfer = TransferCharacteristic::Pq;
        let next = ResolvedColorSemantics::resolve(
            first.stream,
            changed,
            ColorResolutionPolicy::StrictTenBit,
        )
        .unwrap();
        assert_eq!(
            first.ensure_stable(next),
            Err(ColorError::MidStreamColorMetadataChange)
        );
    }

    #[test]
    fn partial_signals_do_not_default_in_strict_mode() {
        let raw = ColorMetadataRaw::unspecified();
        for field in 0..3 {
            let mut partial = raw;
            match field {
                0 => partial.space.transfer = TransferCharacteristic::Bt709,
                1 => partial.space.primaries = ColorPrimaries::Bt709,
                _ => partial.space.matrix = ColorMatrix::Bt709,
            }
            let resolved =
                ResolvedColorSemantics::resolve(partial, raw, ColorResolutionPolicy::StrictTenBit)
                    .unwrap();
            assert_eq!(resolved.dynamic_range, DynamicRangeClass::Unknown);
            assert_eq!(resolved.support, Err(ColorSupportReason::Unknown));
        }
    }

    #[test]
    fn hdr_transfer_is_not_overridden_by_legacy_defaults() {
        let mut raw = ColorMetadataRaw::unspecified();
        raw.space.transfer = TransferCharacteristic::Pq;
        let pq = ResolvedColorSemantics::resolve(
            raw,
            ColorMetadataRaw::unspecified(),
            ColorResolutionPolicy::LegacyEightBit,
        )
        .unwrap();
        assert_eq!(pq.dynamic_range, DynamicRangeClass::HdrPq);
        assert_eq!(pq.support, Err(ColorSupportReason::UnsupportedHdrPq));
        raw.space.transfer = TransferCharacteristic::Hlg;
        let hlg = ResolvedColorSemantics::resolve(
            raw,
            ColorMetadataRaw::unspecified(),
            ColorResolutionPolicy::LegacyEightBit,
        )
        .unwrap();
        assert_eq!(hlg.dynamic_range, DynamicRangeClass::HdrHlg);
        assert_eq!(hlg.support, Err(ColorSupportReason::UnsupportedHdrHlg));
    }

    #[test]
    fn p3_srgb_is_sdr_but_not_a_supported_gamut() {
        let mut raw = ColorMetadataRaw::unspecified();
        raw.space.primaries = ColorPrimaries::DisplayP3;
        raw.space.transfer = TransferCharacteristic::Srgb;
        raw.space.matrix = ColorMatrix::Bt709;
        let color = ResolvedColorSemantics::resolve(
            raw,
            ColorMetadataRaw::unspecified(),
            ColorResolutionPolicy::StrictTenBit,
        )
        .unwrap();
        assert_eq!(color.dynamic_range, DynamicRangeClass::Sdr);
        assert_eq!(
            color.support,
            Err(ColorSupportReason::UnsupportedWideGamutSdr)
        );
    }

    #[test]
    fn contradictory_static_metadata_sources_are_conflicting() {
        let mut stream = ColorMetadataRaw {
            space: ColorSpace::default(),
            ..ColorMetadataRaw::unspecified()
        };
        let mut frame = ColorMetadataRaw::unspecified();
        stream.content_light = Some(ContentLightLevelMetadata {
            max_cll: Some(1000),
            max_fall: Some(400),
        });
        frame.content_light = Some(ContentLightLevelMetadata {
            max_cll: Some(1200),
            max_fall: Some(400),
        });
        let color =
            ResolvedColorSemantics::resolve(stream, frame, ColorResolutionPolicy::StrictTenBit)
                .unwrap();
        assert_eq!(color.dynamic_range, DynamicRangeClass::Conflicting);
        assert_eq!(color.support, Err(ColorSupportReason::Conflicting));
    }

    #[test]
    fn mastering_rationals_validate_without_float_roundtrip() {
        let good = MasteringDisplayMetadata {
            display_primaries: Some(
                [[
                    ColorRational {
                        numerator: 34_000,
                        denominator: 50_000,
                    },
                    ColorRational {
                        numerator: 16_000,
                        denominator: 50_000,
                    },
                ]; 3],
            ),
            white_point: Some(
                [ColorRational {
                    numerator: 15_635,
                    denominator: 50_000,
                }; 2],
            ),
            min_luminance: Some(ColorRational {
                numerator: 50,
                denominator: 10_000,
            }),
            max_luminance: Some(ColorRational {
                numerator: 10_000_000,
                denominator: 10_000,
            }),
        };
        assert_eq!(good.validate(), Ok(good));
        let mut bad = good;
        bad.max_luminance = Some(ColorRational {
            numerator: 1,
            denominator: 0,
        });
        assert_eq!(bad.validate(), Err(ColorError::MalformedMasteringMetadata));
        bad.max_luminance = Some(ColorRational {
            numerator: 1,
            denominator: 1,
        });
        bad.min_luminance = Some(ColorRational {
            numerator: 2,
            denominator: 1,
        });
        assert_eq!(bad.validate(), Err(ColorError::MalformedMasteringMetadata));
        bad = good;
        bad.white_point = Some(
            [ColorRational {
                numerator: 2,
                denominator: 1,
            }; 2],
        );
        assert_eq!(bad.validate(), Err(ColorError::MalformedMasteringMetadata));
    }

    #[test]
    #[ignore = "manual per-stream color-resolution timing sanity"]
    fn resolution_cost_is_far_below_one_millisecond() {
        let raw = ColorMetadataRaw {
            space: ColorSpace::default(),
            ..ColorMetadataRaw::unspecified()
        };
        let started = std::time::Instant::now();
        for _ in 0..1_000_000 {
            std::hint::black_box(
                ResolvedColorSemantics::resolve(
                    std::hint::black_box(raw),
                    ColorMetadataRaw::unspecified(),
                    ColorResolutionPolicy::StrictTenBit,
                )
                .unwrap(),
            );
        }
        let nanos_per_resolve = started.elapsed().as_nanos() / 1_000_000;
        eprintln!("color resolve: {nanos_per_resolve} ns/call");
        assert!(nanos_per_resolve < 1_000_000);
    }
}
