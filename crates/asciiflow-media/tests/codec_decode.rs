use asciiflow_core::{
    ChromaLocation, ChromaSubsampling, ColorMatrix, ColorPrimaries, ColorRange, ColorSpace,
    FrameSource, PixelFormat, TransferCharacteristic, VideoCodec, VideoProfile,
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
    assert!(error.to_string().contains("HDR/BT.2020"), "{error}");
}
