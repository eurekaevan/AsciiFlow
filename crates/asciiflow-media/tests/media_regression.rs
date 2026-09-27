mod common {
    pub mod media_regression;
}

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
    let result = compare_files(
        Path::new(&reference),
        Path::new(&candidate),
        ComparisonPolicy::default(),
    )
    .unwrap();
    println!("{}", result.report());
    assert!(result.passes(), "{}", result.report());
}
