mod common {
    pub mod h264_bitstream;
    pub mod media_regression;
}

use common::h264_bitstream::{compare_avcc_packets, parse_avcc};
use common::media_regression::{ComparisonPolicy, compare, compare_files, inspect};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/codecs")
        .join(name)
}

fn media_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/media")
        .join(name)
}

fn synthetic_h264_packet(
    sps: u8,
    pps: u8,
    idr: u8,
    uuid: &[u8; 16],
    text: &[u8],
    extra: Option<u8>,
) -> Vec<u8> {
    let mut sei = vec![0x06, 5, (16 + text.len()) as u8];
    sei.extend_from_slice(uuid);
    sei.extend_from_slice(text);
    if let Some(value) = extra {
        sei.extend_from_slice(&[4, 1, value]);
    }
    sei.push(0x80);
    let mut packet = Vec::new();
    for nal in [vec![0x67, sps], vec![0x68, pps], sei, vec![0x65, idr]] {
        packet.extend_from_slice(&(nal.len() as u32).to_be_bytes());
        packet.extend_from_slice(&nal);
    }
    packet
}

fn with_unknown_sei_ebsp(packet: &[u8], encoded_payload: &[u8]) -> Vec<u8> {
    let nals = parse_avcc(packet, 4).unwrap();
    let mut rebuilt = Vec::new();
    for (index, nal) in nals.iter().enumerate() {
        let mut bytes = nal.bytes.to_vec();
        if index == 2 {
            bytes.pop(); // rbsp_trailing_bits
            bytes.extend_from_slice(&[4, 6]); // unknown message, six RBSP bytes
            bytes.extend_from_slice(encoded_payload);
            bytes.push(0x80);
        }
        rebuilt.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        rebuilt.extend_from_slice(&bytes);
    }
    rebuilt
}

#[test]
fn h264_encoder_sei_approval_is_narrow_and_preserves_raw_identity() {
    const UUID: [u8; 16] = [
        0x59, 0x94, 0x8b, 0x28, 0x11, 0xec, 0x45, 0xaf, 0x96, 0x75, 0x19, 0xd4, 0x1f, 0xea, 0xa9,
        0x4d,
    ];
    const SUFFIX: &str =
        " / VAAPI 1.23.0 / Intel iHD driver for Intel(R) Gen Graphics - 26.1.5 ()\0";
    let before = synthetic_h264_packet(
        0x11,
        0x22,
        0x33,
        &UUID,
        format!("Lavc62.28.102{SUFFIX}").as_bytes(),
        None,
    );
    let after = synthetic_h264_packet(
        0x11,
        0x22,
        0x33,
        &UUID,
        format!("Lavc62.28.103{SUFFIX}").as_bytes(),
        None,
    );
    assert_ne!(before, after);
    let approved = compare_avcc_packets(&before, &after, 4).unwrap();
    assert_eq!(approved.len(), 1);
    assert_eq!((approved[0].nal_index, approved[0].sei_index), (2, 0));
    assert_eq!(approved[0].reference_token, b"Lavc62.28.102");
    assert_eq!(approved[0].candidate_token, b"Lavc62.28.103");

    let mut snapshot_before = inspect(&fixture("hevc-main8-bframes.mp4")).unwrap();
    let mut snapshot_after = snapshot_before.clone();
    for snapshot in [&mut snapshot_before, &mut snapshot_after] {
        snapshot.streams[0].codec = ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_H264 as i32;
        snapshot.streams[0].extradata = vec![1, 0, 0, 0, 0xff];
    }
    snapshot_before.packets[0][0].payload = before.clone();
    snapshot_after.packets[0][0].payload = after.clone();
    snapshot_before.file_bytes = b"media Lavc62.28.102".to_vec();
    snapshot_after.file_bytes = b"media Lavc62.28.103".to_vec();
    snapshot_after.whole_file_sha256[0] ^= 1;
    let cross_patch = compare(
        &snapshot_before,
        &snapshot_after,
        ComparisonPolicy::default(),
    );
    assert!(cross_patch.passes(), "{}", cross_patch.report());
    assert!(!cross_patch.raw_packet_identity_equal());
    assert!(cross_patch.coded_semantics_equal());
    assert!(cross_patch.decoded_identity_equal());
    assert_eq!(cross_patch.approved_coded_metadata_differences.len(), 1);
    let strict = compare(
        &snapshot_before,
        &snapshot_after,
        ComparisonPolicy {
            require_raw_packet_identity: true,
            require_whole_file_identity: true,
            ..ComparisonPolicy::default()
        },
    );
    assert!(
        !strict.passes(),
        "exact-build policy must retain byte gates"
    );

    for (name, changed) in [
        (
            "SPS",
            synthetic_h264_packet(
                0x12,
                0x22,
                0x33,
                &UUID,
                format!("Lavc62.28.103{SUFFIX}").as_bytes(),
                None,
            ),
        ),
        (
            "PPS",
            synthetic_h264_packet(
                0x11,
                0x23,
                0x33,
                &UUID,
                format!("Lavc62.28.103{SUFFIX}").as_bytes(),
                None,
            ),
        ),
        (
            "VCL",
            synthetic_h264_packet(
                0x11,
                0x22,
                0x34,
                &UUID,
                format!("Lavc62.28.103{SUFFIX}").as_bytes(),
                None,
            ),
        ),
        (
            "unknown SEI",
            synthetic_h264_packet(
                0x11,
                0x22,
                0x33,
                &UUID,
                format!("Lavc62.28.103{SUFFIX}").as_bytes(),
                Some(0x7f),
            ),
        ),
        (
            "UUID",
            synthetic_h264_packet(
                0x11,
                0x22,
                0x33,
                &[0; 16],
                format!("Lavc62.28.103{SUFFIX}").as_bytes(),
                None,
            ),
        ),
        (
            "payload content",
            synthetic_h264_packet(
                0x11,
                0x22,
                0x33,
                &UUID,
                format!("Lavc62.28.103{}", SUFFIX.replace("Intel", "Other")).as_bytes(),
                None,
            ),
        ),
    ] {
        assert!(
            compare_avcc_packets(&before, &changed, 4).is_err(),
            "{name}"
        );
    }
    let unknown_before = synthetic_h264_packet(
        0x11,
        0x22,
        0x33,
        &UUID,
        format!("Lavc62.28.102{SUFFIX}").as_bytes(),
        Some(0x7e),
    );
    let unknown_after = synthetic_h264_packet(
        0x11,
        0x22,
        0x33,
        &UUID,
        format!("Lavc62.28.103{SUFFIX}").as_bytes(),
        Some(0x7f),
    );
    assert!(compare_avcc_packets(&unknown_before, &unknown_after, 4).is_err());

    // These two EBSP encodings yield the same unknown-message RBSP. A parsed
    // version-only comparison must still reject their extra raw difference.
    let shifted_escape_before = with_unknown_sei_ebsp(&before, &[0, 0, 3, 1, 0, 0, 1]);
    let shifted_escape_after = with_unknown_sei_ebsp(&after, &[0, 0, 1, 0, 0, 3, 1]);
    assert!(compare_avcc_packets(&shifted_escape_before, &shifted_escape_after, 4).is_err());
}

#[test]
fn approved_ffmpeg_patch_tag_is_observable() {
    let reference = inspect(&fixture("hevc-main8-bframes.mp4")).unwrap();
    let mut candidate = reference.clone();
    let mut before = reference.clone();
    before
        .format_tags
        .insert("encoder".into(), "Lavf62.12.102".into());
    candidate
        .format_tags
        .insert("encoder".into(), "Lavf62.12.103".into());
    before.file_bytes = b"container Lavf62.12.102".to_vec();
    candidate.file_bytes = b"container Lavf62.12.103".to_vec();
    candidate.whole_file_sha256[0] ^= 1;
    let result = compare(&before, &candidate, ComparisonPolicy::default());
    assert!(result.passes(), "{}", result.report());
    assert!(!result.whole_file_equal);
    assert_eq!(result.approved_volatile_differences.len(), 1);
    assert!(result.report().contains("Lavf62.12.102"));
    candidate.file_bytes.push(1);
    let unexplained = compare(&before, &candidate, ComparisonPolicy::default());
    assert!(!unexplained.container_structure_equal());
    assert!(
        unexplained
            .report()
            .contains("outside approved version tag")
    );

    let mut duplicate_before = before.clone();
    let mut duplicate_after = candidate.clone();
    duplicate_before.file_bytes = b"Lavf62.12.102 Lavf62.12.102".to_vec();
    duplicate_after.file_bytes = b"Lavf62.12.103 Lavf62.12.103".to_vec();
    let duplicate = compare(
        &duplicate_before,
        &duplicate_after,
        ComparisonPolicy::default(),
    );
    assert!(!duplicate.container_structure_equal());
}

#[test]
fn packet_payload_and_timestamp_mutations_fail() {
    let reference = inspect(&fixture("hevc-main8-bframes.mp4")).unwrap();
    let mut payload = reference.clone();
    payload.packets[0][0].payload[0] ^= 1;
    let result = compare(&reference, &payload, ComparisonPolicy::default());
    assert!(!result.media_semantics_equal());
    assert!(result.report().contains("packet 0 payload mismatch"));

    let mut time = reference.clone();
    time.packets[0][0].pts += 1;
    let result = compare(&reference, &time, ComparisonPolicy::default());
    assert!(!result.media_semantics_equal());
    assert!(result.report().contains("PTS/DTS/duration"));
}

#[test]
fn color_profile_and_decoded_pixels_are_not_allowlisted() {
    let reference = inspect(&fixture("hevc-main8-bframes.mp4")).unwrap();
    let mut color = reference.clone();
    color.streams[0].color_transfer += 1;
    assert!(
        compare(&reference, &color, ComparisonPolicy::default())
            .report()
            .contains("color transfer")
    );
    let mut profile = reference.clone();
    profile.streams[0].profile += 1;
    assert!(
        compare(&reference, &profile, ComparisonPolicy::default())
            .report()
            .contains("profile")
    );
    let mut pixels = reference.clone();
    pixels.frames[0].visible_sha256[0] ^= 1;
    assert!(
        compare(&reference, &pixels, ComparisonPolicy::default())
            .report()
            .contains("visible pixel SHA-256")
    );
}

#[test]
fn arbitrary_encoder_tag_and_audio_semantics_fail() {
    let reference = inspect(&fixture("hevc-main8-bframes.mp4")).unwrap();
    let mut arbitrary = reference.clone();
    arbitrary
        .format_tags
        .insert("encoder".into(), "Lavf62.12.103 extra".into());
    assert!(
        !compare(&reference, &arbitrary, ComparisonPolicy::default()).container_structure_equal()
    );
    let mut audio = reference.clone();
    let mut stream = audio.streams[0].clone();
    stream.index = 1;
    stream.kind = ffmpeg_sys_next::AVMediaType::AVMEDIA_TYPE_AUDIO as i32;
    stream.tags.insert("language".into(), "eng".into());
    audio.streams.push(stream);
    audio.packets.push(vec![audio.packets[0][0].clone()]);
    let mut altered = audio.clone();
    altered.streams[1]
        .tags
        .insert("language".into(), "jpn".into());
    altered.packets[1][0].payload[0] ^= 1;
    let result = compare(&audio, &altered, ComparisonPolicy::default());
    assert!(!result.media_semantics_equal());
    assert!(result.report().contains("language"));
    assert!(
        result
            .report()
            .contains("stream 1 packet 0 payload mismatch")
    );
}

#[test]
fn real_audio_packets_routing_and_disposition_are_compared() {
    let reference = inspect(&media_fixture("mixed.mkv")).unwrap();
    let audio = reference
        .streams
        .iter()
        .position(|s| s.kind == ffmpeg_sys_next::AVMediaType::AVMEDIA_TYPE_AUDIO as i32)
        .unwrap();
    assert!(!reference.packets[audio].is_empty());
    let mut candidate = reference.clone();
    candidate.packets[audio][0].payload[0] ^= 1;
    candidate.streams[audio].disposition ^= 1;
    candidate.streams[audio]
        .tags
        .insert("language".into(), "changed".into());
    let result = compare(&reference, &candidate, ComparisonPolicy::default());
    assert!(!result.media_semantics_equal());
    assert!(result.report().contains("payload mismatch"));
    assert!(result.report().contains("disposition"));
    assert!(result.report().contains("language"));
}

#[test]
fn interleave_change_is_structural_not_payload_regression() {
    let reference = inspect(&media_fixture("mixed.mkv")).unwrap();
    let mut candidate = reference.clone();
    candidate.interleave.reverse();
    let result = compare(&reference, &candidate, ComparisonPolicy::default());
    assert!(result.media_semantics_equal(), "{}", result.report());
    assert!(!result.container_structure_equal());
    assert!(result.report().contains("Container ordering differences"));
}

/// The comparator requires only files, not a GPU. Set these paths to compare
/// any future retained reference/candidate pair without adding a test case.
#[test]
#[ignore = "set ASCIIFLOW_REGRESSION_REFERENCE and ASCIIFLOW_REGRESSION_CANDIDATE"]
fn compare_pair_from_env() {
    let reference = std::env::var_os("ASCIIFLOW_REGRESSION_REFERENCE").unwrap();
    let candidate = std::env::var_os("ASCIIFLOW_REGRESSION_CANDIDATE").unwrap();
    let exact_build = std::env::var("ASCIIFLOW_REGRESSION_EXACT_BUILD").ok();
    assert!(
        exact_build
            .as_deref()
            .is_none_or(|value| value == "attested"),
        "ASCIIFLOW_REGRESSION_EXACT_BUILD must be 'attested' when set"
    );
    let strict = exact_build.as_deref() == Some("attested");
    let result = compare_files(
        Path::new(&reference),
        Path::new(&candidate),
        ComparisonPolicy {
            require_raw_packet_identity: strict,
            require_whole_file_identity: strict,
            ..ComparisonPolicy::default()
        },
    )
    .unwrap();
    println!("{}", result.report());
    assert!(result.passes(), "{}", result.report());
}
