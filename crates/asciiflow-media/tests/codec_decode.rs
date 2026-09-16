use asciiflow_core::{
    ChromaLocation, ChromaSubsampling, ColorMatrix, ColorPrimaries, ColorRange, ColorSpace,
    FrameSource, TransferCharacteristic, VideoCodec, VideoProfile,
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
fn ten_bit_open_succeeds_but_first_frame_is_explicitly_rejected() {
    for name in ["hevc-main10-reject.mp4", "av1-main10-reject.mp4"] {
        let mut decoder = Decoder::open(fixture(name)).unwrap();
        assert_eq!(decoder.info().requirements.bit_depth, Some(10), "{name}");
        let validation = decoder.info().requirements.validate_current_pipeline();
        assert!(
            validation.is_err(),
            "{name} unexpectedly passed requirements validation"
        );
        assert!(
            validation
                .unwrap_err()
                .to_string()
                .contains("10-bit video is not supported")
        );
        let error = decoder.next_frame().unwrap_err();
        assert!(
            error.to_string().contains("10-bit video is not supported"),
            "{name}: {error}"
        );
    }
}
