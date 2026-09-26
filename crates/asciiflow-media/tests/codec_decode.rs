use asciiflow_core::{
    ChromaLocation, ChromaSubsampling, ColorMatrix, ColorPrimaries, ColorRange, ColorSpace,
    ColorSupportReason, DynamicRangeClass, FrameSource, PixelFormat, TransferCharacteristic,
    VideoCodec, VideoProfile,
};
use asciiflow_media::Decoder;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/codecs")
        .join(name)
}

#[test]
#[ignore = "requires Intel VAAPI decode; compares all display frames including drain"]
fn hevc_av1_software_and_vaapi_download_are_exact() {
    for name in ["hevc-main8-bframes.mp4", "av1-main8-nofilmgrain.mp4"] {
        let expected = decode_all(name);
        let mut decoder = Decoder::open_with(
            fixture(name),
            asciiflow_media::DecodeMode::Vaapi,
            Default::default(),
        )
        .unwrap();
        for (pts, bytes) in expected {
            let frame = decoder.next_frame().unwrap().unwrap();
            assert_eq!(frame.pts(), pts);
            assert_eq!(frame.host().as_slice(), bytes);
        }
        assert!(decoder.next_frame().unwrap().is_none());
    }
}

#[test]
#[ignore = "requires Intel VAAPI decode; compares first-frame raw and resolved color semantics"]
fn hevc_av1_main10_software_and_vaapi_color_semantics_agree() {
    for name in [
        "hevc-main10-sdr-gradient.mp4",
        "av1-main10-sdr-gradient.mp4",
    ] {
        let input = fixture(name);
        let mut software = Decoder::open(&input).unwrap();
        let mut hardware = Decoder::open_with(
            &input,
            asciiflow_media::DecodeMode::Vaapi,
            Default::default(),
        )
        .unwrap();
        software.next_frame().unwrap().unwrap();
        hardware.next_frame().unwrap().unwrap();
        let expected = software.info().requirements.color_semantics.unwrap();
        let actual = hardware.info().requirements.color_semantics.unwrap();
        println!("{name} software: {expected:#?}");
        println!("{name} VAAPI: {actual:#?}");
        assert_eq!(expected.stream, actual.stream, "{name} stream raw metadata");
        assert_eq!(expected.frame, actual.frame, "{name} frame raw metadata");
        assert_eq!(expected.effective, actual.effective, "{name}");
        assert_eq!(expected.provenance, actual.provenance, "{name}");
        assert_eq!(expected.dynamic_range, actual.dynamic_range, "{name}");
        assert_eq!(expected.support, actual.support, "{name}");
        assert_eq!(
            expected.effective_static_metadata(),
            actual.effective_static_metadata(),
            "{name}"
        );
    }
}

#[test]
#[ignore = "requires Intel VAAPI decode; checks unsupported color before pixel processing"]
fn vaapi_rejects_unsupported_color_with_classification_intact() {
    for (name, class, reason) in [
        (
            "hevc-main10-sdr-full.mp4",
            DynamicRangeClass::Sdr,
            ColorSupportReason::UnsupportedFullRange,
        ),
        (
            "hevc-main10-bt2020-sdr.mp4",
            DynamicRangeClass::Sdr,
            ColorSupportReason::UnsupportedWideGamutSdr,
        ),
        (
            "hevc-main10-pq-reject.mp4",
            DynamicRangeClass::HdrPq,
            ColorSupportReason::UnsupportedHdrPq,
        ),
        (
            "hevc-main10-hlg.mp4",
            DynamicRangeClass::HdrHlg,
            ColorSupportReason::UnsupportedHdrHlg,
        ),
        (
            "av1-main10-pq.mp4",
            DynamicRangeClass::HdrPq,
            ColorSupportReason::UnsupportedHdrPq,
        ),
        (
            "hevc-main10-pq-static-metadata.mp4",
            DynamicRangeClass::HdrPq,
            ColorSupportReason::UnsupportedHdrPq,
        ),
    ] {
        for mode in [
            asciiflow_media::DecodeMode::Software,
            asciiflow_media::DecodeMode::Vaapi,
        ] {
            let mut decoder = Decoder::open_with(fixture(name), mode, Default::default()).unwrap();
            let error = decoder.next_frame().unwrap_err();
            let color = decoder.info().requirements.color_semantics.unwrap();
            println!("{name} {mode:?}: {color:#?}; rejected: {error}");
            assert_eq!(color.dynamic_range, class, "{name} {mode:?}");
            assert_eq!(color.support, Err(reason), "{name} {mode:?}");
            if name == "hevc-main10-pq-static-metadata.mp4" {
                let (mastering, light) = color.effective_static_metadata();
                let mastering = mastering.unwrap();
                assert_eq!(mastering.max_luminance.unwrap().numerator, 10_000_000);
                assert_eq!(mastering.max_luminance.unwrap().denominator, 10_000);
                let light = light.unwrap();
                assert_eq!(light.max_cll, Some(1000));
                assert_eq!(light.max_fall, Some(400));
            }
        }
    }
}

fn decode_all(name: &str) -> Vec<(Option<i64>, Vec<u8>)> {
    let mut decoder = Decoder::open(fixture(name)).unwrap();
    let mut frames = Vec::new();
    while let Some(frame) = decoder.next_frame().unwrap() {
        frames.push((frame.pts(), frame.host().as_slice().to_vec()));
    }
    frames
}

fn assert_main8_contract(name: &str, codec: VideoCodec, profile: VideoProfile) {
    let mut decoder = Decoder::open(fixture(name)).unwrap();
    let info = decoder.info();
    assert_eq!(info.requirements.codec, codec);
    assert_eq!(info.requirements.profile, Some(profile));
    assert_eq!(info.requirements.bit_depth, Some(8));
    assert_eq!(
        info.requirements.chroma_subsampling,
        ChromaSubsampling::Yuv420
    );
    assert_eq!(info.requirements.width, 64);
    assert_eq!(info.requirements.height, 64);
    assert_eq!(info.frame_desc.width, 64);
    assert_eq!(info.frame_desc.height, 64);
    assert_eq!(
        info.frame_desc.color_space,
        ColorSpace {
            matrix: ColorMatrix::Bt709,
            range: ColorRange::Limited,
            primaries: ColorPrimaries::Bt709,
            transfer: TransferCharacteristic::Bt709,
            chroma_location: ChromaLocation::Left,
        }
    );

    let mut pts = Vec::new();
    for _ in 0..36 {
        let frame = decoder.next_frame().unwrap().expect("fixture ended early");
        pts.push(frame.pts().expect("fixture frame has no PTS"));
        assert_eq!(frame.desc().width, 64);
        assert_eq!(frame.desc().height, 64);
    }
    assert!(decoder.next_frame().unwrap().is_none());
    let step = pts[1] - pts[0];
    assert!(step > 0, "unexpected first PTS step: {step}");
    assert!(
        pts.windows(2)
            .all(|pair| pair[1] > pair[0] && pair[1] - pair[0] == step)
    );
}

#[test]
fn hevc_main8_decodes_all_frames_with_reordered_pts() {
    assert_main8_contract(
        "hevc-main8-bframes.mp4",
        VideoCodec::Hevc,
        VideoProfile::HevcMain,
    );
}

#[test]
fn av1_main8_decodes_all_frames() {
    assert_main8_contract(
        "av1-main8-nofilmgrain.mp4",
        VideoCodec::Av1,
        VideoProfile::Av1Main,
    );
}

#[test]
fn main8_decoding_is_byte_and_pts_deterministic() {
    for name in ["hevc-main8-bframes.mp4", "av1-main8-nofilmgrain.mp4"] {
        let first = decode_all(name);
        let second = decode_all(name);
        assert_eq!(first.len(), 36, "{name}");
        assert_eq!(first, second, "repeated decode changed {name}");
    }
}

#[test]
fn ten_bit_sdr_decodes_to_canonical_p010_and_drains() {
    for (name, codec, profile) in [
        (
            "hevc-main10-sdr-gradient.mp4",
            VideoCodec::Hevc,
            VideoProfile::HevcMain10,
        ),
        (
            "av1-main10-sdr-gradient.mp4",
            VideoCodec::Av1,
            VideoProfile::Av1Main,
        ),
    ] {
        let mut decoder = Decoder::open(fixture(name)).unwrap();
        assert_eq!(decoder.info().requirements.codec, codec);
        assert_eq!(decoder.info().requirements.profile, Some(profile));
        assert_eq!(decoder.info().requirements.bit_depth, Some(10), "{name}");
        assert_eq!(
            decoder
                .info()
                .requirements
                .validate_processing_input()
                .unwrap(),
            PixelFormat::P010Le
        );
        assert!(
            decoder
                .info()
                .requirements
                .validate_current_pipeline()
                .unwrap_err()
                .to_string()
                .contains("select explicit HEVC Main10 output")
        );
        let mut pts = Vec::new();
        let mut low_bits_seen = [false; 4];
        while let Some(frame) = decoder.next_frame().unwrap() {
            assert_eq!(frame.desc().format, PixelFormat::P010Le);
            assert_eq!(frame.desc().byte_len(), 64 * 64 * 3);
            assert_eq!(frame.desc().color_space.matrix, ColorMatrix::Bt709);
            pts.push(frame.pts().unwrap());
            for word in frame.host().as_slice().chunks_exact(2) {
                let packed = u16::from_le_bytes([word[0], word[1]]);
                assert_eq!(packed & 0x3f, 0, "{name} has non-zero P010 padding");
                low_bits_seen[((packed >> 6) & 3) as usize] = true;
            }
        }
        assert_eq!(pts.len(), 36, "{name}");
        assert!(pts.windows(2).all(|pair| pair[1] > pair[0]), "{name}");
        assert!(
            low_bits_seen.into_iter().all(|seen| seen),
            "{name} lacks real low-bit precision"
        );
        assert_eq!(decode_all(name).len(), 36);
    }
}

#[test]
fn ten_bit_pq_is_rejected_before_processing() {
    let mut decoder = Decoder::open(fixture("hevc-main10-pq-reject.mp4")).unwrap();
    let error = decoder.next_frame().unwrap_err();
    assert!(
        error.to_string().contains("HDR PQ input detected"),
        "{error}"
    );
}

#[test]
fn hdr_and_wide_gamut_fixtures_classify_without_pixel_processing() {
    for (name, class, support, error_text) in [
        (
            "hevc-main10-pq-reject.mp4",
            DynamicRangeClass::HdrPq,
            ColorSupportReason::UnsupportedHdrPq,
            "HDR PQ",
        ),
        (
            "av1-main10-pq.mp4",
            DynamicRangeClass::HdrPq,
            ColorSupportReason::UnsupportedHdrPq,
            "HDR PQ",
        ),
        (
            "hevc-main10-hlg.mp4",
            DynamicRangeClass::HdrHlg,
            ColorSupportReason::UnsupportedHdrHlg,
            "HDR HLG",
        ),
        (
            "hevc-main10-bt2020-sdr.mp4",
            DynamicRangeClass::Sdr,
            ColorSupportReason::UnsupportedWideGamutSdr,
            "wide-gamut SDR",
        ),
        (
            "hevc-main10-unspecified.mp4",
            DynamicRangeClass::Unknown,
            ColorSupportReason::Unknown,
            "cannot be resolved",
        ),
        (
            "hevc-main10-pq-bt709-conflict.mp4",
            DynamicRangeClass::HdrPq,
            ColorSupportReason::Conflicting,
            "conflicting color",
        ),
    ] {
        let mut decoder = Decoder::open(fixture(name)).unwrap();
        let error = decoder.next_frame().unwrap_err();
        assert!(error.to_string().contains(error_text), "{name}: {error}");
        let color = decoder.info().requirements.color_semantics.unwrap();
        assert_eq!(color.dynamic_range, class, "{name}");
        assert_eq!(color.support, Err(support), "{name}");
    }
}

#[test]
fn hevc_and_av1_resolve_equivalent_sdr_and_pq_semantics() {
    let mut hevc = Decoder::open(fixture("hevc-main10-sdr-gradient.mp4")).unwrap();
    let mut av1 = Decoder::open(fixture("av1-main10-sdr-gradient.mp4")).unwrap();
    hevc.next_frame().unwrap().unwrap();
    av1.next_frame().unwrap().unwrap();
    let hevc_sdr = hevc.info().requirements.color_semantics.unwrap();
    let av1_sdr = av1.info().requirements.color_semantics.unwrap();
    assert_eq!(hevc_sdr.effective.primaries, av1_sdr.effective.primaries);
    assert_eq!(hevc_sdr.effective.transfer, av1_sdr.effective.transfer);
    assert_eq!(hevc_sdr.effective.matrix, av1_sdr.effective.matrix);
    assert_eq!(hevc_sdr.effective.range, av1_sdr.effective.range);
    assert_eq!(hevc_sdr.dynamic_range, av1_sdr.dynamic_range);

    let mut hevc = Decoder::open(fixture("hevc-main10-pq-reject.mp4")).unwrap();
    let mut av1 = Decoder::open(fixture("av1-main10-pq.mp4")).unwrap();
    assert!(hevc.next_frame().is_err());
    assert!(av1.next_frame().is_err());
    let hevc_pq = hevc.info().requirements.color_semantics.unwrap();
    let av1_pq = av1.info().requirements.color_semantics.unwrap();
    assert_eq!(hevc_pq.effective.primaries, av1_pq.effective.primaries);
    assert_eq!(hevc_pq.effective.transfer, av1_pq.effective.transfer);
    assert_eq!(hevc_pq.effective.matrix, av1_pq.effective.matrix);
    assert_eq!(hevc_pq.effective.range, av1_pq.effective.range);
    assert_eq!(hevc_pq.dynamic_range, av1_pq.dynamic_range);
}

#[test]
fn static_hdr_metadata_is_parsed_without_defining_hdr_class() {
    let mut decoder = Decoder::open(fixture("hevc-main10-pq-static-metadata.mp4")).unwrap();
    let error = decoder.next_frame().unwrap_err();
    assert!(error.to_string().contains("HDR PQ"), "{error}");
    let color = decoder.info().requirements.color_semantics.unwrap();
    assert_eq!(color.dynamic_range, DynamicRangeClass::HdrPq);
    let mastering = color
        .frame
        .mastering_display
        .or(color.stream.mastering_display)
        .unwrap();
    assert_eq!(mastering.max_luminance.unwrap().numerator, 10_000_000);
    assert_eq!(mastering.max_luminance.unwrap().denominator, 10_000);
    let light = color
        .frame
        .content_light
        .or(color.stream.content_light)
        .unwrap();
    assert_eq!(light.max_cll, Some(1000));
    assert_eq!(light.max_fall, Some(400));

    let mut without = Decoder::open(fixture("hevc-main10-pq-reject.mp4")).unwrap();
    assert!(without.next_frame().is_err());
    let color = without.info().requirements.color_semantics.unwrap();
    assert_eq!(color.dynamic_range, DynamicRangeClass::HdrPq);
    assert!(color.frame.mastering_display.is_none());
}

#[test]
fn full_range_p010_is_not_mislabeled_as_supported() {
    let mut decoder = Decoder::open(fixture("hevc-main10-sdr-full.mp4")).unwrap();
    let error = decoder.next_frame().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("full-range input is not qualified"),
        "{error}"
    );
    let color = decoder.info().requirements.color_semantics.unwrap();
    assert_eq!(color.dynamic_range, DynamicRangeClass::Sdr);
    assert_eq!(color.effective.range, ColorRange::Full);
}

#[test]
fn legacy_bt601_main8_is_normalized_to_bt709_nv12() {
    let mut decoder = Decoder::open(fixture("hevc-main8-bt601.mp4")).unwrap();
    let frame = decoder.next_frame().unwrap().unwrap();
    assert_eq!(frame.desc().format, PixelFormat::Nv12);
    assert_eq!(frame.desc().color_space, ColorSpace::default());
    let color = decoder.info().requirements.color_semantics.unwrap();
    assert_eq!(color.dynamic_range, DynamicRangeClass::Sdr);
    assert_eq!(color.support, Ok(()));
    assert_eq!(color.effective.matrix, ColorMatrix::Bt601);
    assert_eq!(decode_all("hevc-main8-bt601.mp4").len(), 36);
}
