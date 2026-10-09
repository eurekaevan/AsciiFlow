//! An additive, deliberately narrower cross-driver gate. The historical H.264
//! oracle and its Lavc-patch allowlist are not changed or called by this gate.
#[allow(dead_code)] // Shared inspection helpers also serve the unchanged legacy oracle.
mod common {
    pub mod h264_bitstream;
    pub mod media_regression;
}

use common::h264_bitstream::{avcc_length_size, parse_avcc};
use common::media_regression::{MediaSnapshot, inspect};
use ffmpeg_sys_next as ffi;
use std::{fs::OpenOptions, io::Write, path::Path};

const UUID: [u8; 16] = [
    0x59, 0x94, 0x8b, 0x28, 0x11, 0xec, 0x45, 0xaf, 0x96, 0x75, 0x19, 0xd4, 0x1f, 0xea, 0xa9, 0x4d,
];
const UUID_HEX: &str = "59948b2811ec45af967519d41feaa94d";
const ENCODER_IDENTIFIER_PREFIX: &str =
    "Lavc62.28.103 / VAAPI 1.23.0 / Intel iHD driver for Intel(R) Gen Graphics - ";

fn identifier_sei(version: &str) -> Result<Vec<u8>, String> {
    // Version values come from the caller's independently attested driver
    // receipts, not from untrusted SEI text. No capability/version denylist.
    if version.len() > 24
        || version.split('.').count() != 3
        || version
            .split('.')
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err("invalid attested iHD version".into());
    }
    let text = format!("{ENCODER_IDENTIFIER_PREFIX}{version} ()\0");
    let payload_size = UUID.len() + text.len();
    if payload_size >= 255 {
        return Err("identifier exceeds this observed SEI contract".into());
    }
    // The complete known payload has no emulation-prevention sequence. Exact
    // EBSP comparison also requires the exact type, size and rbsp_trailing_bits;
    // malformed escapes, new messages and unknown SEI are never normalized.
    let mut nal = vec![6, 5, payload_size as u8];
    nal.extend_from_slice(&UUID);
    nal.extend_from_slice(text.as_bytes());
    nal.push(0x80);
    Ok(nal)
}

#[derive(Debug)]
struct NalEvidence {
    access_unit: usize,
    nal_index: usize,
    nal_type: u8,
    nal_name: &'static str,
    classification: &'static str,
    size: usize,
    sha256: String,
    known_identifier_version: Option<String>,
    known_identifier_uuid: Option<&'static str>,
    known_sei_payload_type: Option<u8>,
}

#[derive(Debug)]
struct NalDifference {
    access_unit: usize,
    nal_index: usize,
    nal_type: u8,
    nal_name: &'static str,
    reference_size: usize,
    candidate_size: usize,
    reference_sha256: String,
    candidate_sha256: String,
    first_difference_offset: usize,
    reference_byte_at_first_difference: u8,
    candidate_byte_at_first_difference: u8,
    reference_identifier_version: String,
    candidate_identifier_version: String,
}

struct IdentifierExpectation<'a> {
    nal: &'a [u8],
    version: &'a str,
}

struct IdentifierPair<'a> {
    reference: IdentifierExpectation<'a>,
    candidate: IdentifierExpectation<'a>,
}

struct PacketComparison {
    reference_nals: Vec<NalEvidence>,
    candidate_nals: Vec<NalEvidence>,
    differences: Vec<NalDifference>,
}

#[derive(Debug)]
struct KnownIdentifierEvidence {
    sei_payload_type: u8,
    uuid: &'static str,
    fixed_prefix: &'static str,
    reference_version: String,
    candidate_version: String,
    rbsp_trailing_byte: u8,
    reference_payload_size: usize,
    candidate_payload_size: usize,
    payload_size_equal: bool,
    full_expected_nal_bytes_match: bool,
    differing_bytes_confined_to_version: bool,
}

#[derive(Debug)]
struct DriverPairReceipt {
    schema: &'static str,
    reference_ihd_version: String,
    candidate_ihd_version: String,
    video_stream_index: usize,
    stream_count: usize,
    audio_and_other_streams_packet_identity: bool,
    stream_metadata_identity: bool,
    packet_timing_flags_and_side_data_identity: bool,
    packet_interleave_identity: bool,
    decoded_frames_identity: bool,
    reference_container_sha256: String,
    candidate_container_sha256: String,
    normalized_container_identity: bool,
    known_identifier: KnownIdentifierEvidence,
    reference_nals: Vec<NalEvidence>,
    candidate_nals: Vec<NalEvidence>,
    approved_differences: Vec<NalDifference>,
}

fn nal_name(kind: u8) -> &'static str {
    match kind {
        1 => "non_idr_slice",
        5 => "idr_slice",
        6 => "sei",
        7 => "sps",
        8 => "pps",
        9 => "access_unit_delimiter",
        _ => "other",
    }
}

fn digest(bytes: &[u8]) -> Result<String, String> {
    let state = unsafe { ffi::av_sha_alloc() };
    if state.is_null() {
        return Err("allocate SHA-256 state for forensic receipt".into());
    }
    let initialized = unsafe { ffi::av_sha_init(state, 256) };
    if initialized < 0 {
        unsafe { ffi::av_free(state.cast()) };
        return Err(format!(
            "initialize SHA-256 for forensic receipt: {initialized}"
        ));
    }
    let mut hash = [0u8; 32];
    unsafe {
        ffi::av_sha_update(state, bytes.as_ptr(), bytes.len());
        ffi::av_sha_final(state, hash.as_mut_ptr());
        ffi::av_free(state.cast());
    }
    Ok(hash.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn compare_packet(
    before: &[u8],
    after: &[u8],
    length_size: usize,
    identifiers: &IdentifierPair<'_>,
    access_unit: usize,
) -> Result<PacketComparison, String> {
    let left = parse_avcc(before, length_size)?;
    let right = parse_avcc(after, length_size)?;
    if left.len() != right.len() || before.len() != after.len() {
        return Err("NAL count or packet length changed".into());
    }
    let mut reference_nals = Vec::with_capacity(left.len());
    let mut candidate_nals = Vec::with_capacity(right.len());
    let mut changes = Vec::new();
    for (index, (a, b)) in left.iter().zip(&right).enumerate() {
        if a.kind != b.kind || a.bytes[0] != b.bytes[0] {
            return Err(format!("NAL {index} header/type/ref_idc changed"));
        }
        let reference_identifier_version = (a.bytes == identifiers.reference.nal)
            .then(|| identifiers.reference.version.to_owned());
        let candidate_identifier_version = (b.bytes == identifiers.candidate.nal)
            .then(|| identifiers.candidate.version.to_owned());
        for (nal, version, inventory) in [
            (a, reference_identifier_version.clone(), &mut reference_nals),
            (b, candidate_identifier_version.clone(), &mut candidate_nals),
        ] {
            inventory.push(NalEvidence {
                access_unit,
                nal_index: index,
                nal_type: nal.kind,
                nal_name: nal_name(nal.kind),
                classification: if version.is_some() {
                    "known_driver_identifier"
                } else {
                    "exact_unchanged_nal"
                },
                size: nal.bytes.len(),
                sha256: digest(nal.bytes)?,
                known_identifier_version: version,
                known_identifier_uuid: (nal.bytes == identifiers.reference.nal
                    || nal.bytes == identifiers.candidate.nal)
                    .then_some(UUID_HEX),
                known_sei_payload_type: (nal.bytes == identifiers.reference.nal
                    || nal.bytes == identifiers.candidate.nal)
                    .then_some(5),
            });
        }
        if a.bytes != b.bytes {
            if a.kind != 6
                || a.bytes != identifiers.reference.nal
                || b.bytes != identifiers.candidate.nal
            {
                return Err(format!(
                    "NAL {index}: difference is outside the exact attested encoder identifier"
                ));
            }
            if a.bytes.len() != b.bytes.len() {
                return Err(format!("NAL {index}: encoder identifier size changed"));
            }
            let first_difference_offset = a
                .bytes
                .iter()
                .zip(b.bytes)
                .position(|(before, after)| before != after)
                .ok_or("changed identifier NAL has no differing byte")?;
            changes.push(NalDifference {
                access_unit,
                nal_index: index,
                nal_type: a.kind,
                nal_name: nal_name(a.kind),
                reference_size: a.bytes.len(),
                candidate_size: b.bytes.len(),
                reference_sha256: digest(a.bytes)?,
                candidate_sha256: digest(b.bytes)?,
                first_difference_offset,
                reference_byte_at_first_difference: a.bytes[first_difference_offset],
                candidate_byte_at_first_difference: b.bytes[first_difference_offset],
                reference_identifier_version: reference_identifier_version.unwrap(),
                candidate_identifier_version: candidate_identifier_version.unwrap(),
            });
        }
    }
    Ok(PacketComparison {
        reference_nals,
        candidate_nals,
        differences: changes,
    })
}

fn replace_unique_packet(bytes: &[u8], old: &[u8], new: &[u8]) -> Result<Vec<u8>, String> {
    if old.is_empty() || old.len() != new.len() {
        return Err("invalid proven packet replacement".into());
    }
    let mut matches = bytes
        .windows(old.len())
        .enumerate()
        .filter_map(|(offset, b)| (b == old).then_some(offset));
    let offset = matches
        .next()
        .ok_or("proven packet missing from container")?;
    if matches.next().is_some() {
        return Err("proven packet is not unique in container".into());
    }
    let mut normalized = bytes.to_vec();
    normalized[offset..offset + old.len()].copy_from_slice(new);
    Ok(normalized)
}

fn compare_driver_pair(
    reference: &MediaSnapshot,
    candidate: &MediaSnapshot,
    reference_version: &str,
    candidate_version: &str,
) -> Result<DriverPairReceipt, String> {
    if reference_version == candidate_version {
        return Err("Tier 1B-P is not a same-stack replacement oracle".into());
    }
    if reference.streams != candidate.streams
        || reference.format_start_time != candidate.format_start_time
        || reference.format_duration != candidate.format_duration
        || reference.format_tags != candidate.format_tags
        || reference.interleave != candidate.interleave
        || reference.streams.is_empty()
        || reference.streams.len() != reference.packets.len()
        || candidate.streams.len() != candidate.packets.len()
        || reference.packets.len() != candidate.packets.len()
    {
        return Err("stream/extradata/color/timing/container metadata changed".into());
    }
    let video_streams = reference
        .streams
        .iter()
        .enumerate()
        .filter(|(_, stream)| stream.kind == ffi::AVMediaType::AVMEDIA_TYPE_VIDEO as i32)
        .collect::<Vec<_>>();
    if video_streams.len() != 1
        || video_streams[0].1.codec != ffi::AVCodecID::AV_CODEC_ID_H264 as i32
    {
        return Err("expected exactly one H.264 video stream".into());
    }
    let (video_index, stream) = video_streams[0];
    for index in 0..reference.packets.len() {
        if index != video_index && reference.packets[index] != candidate.packets[index] {
            return Err(format!("non-video stream {index} packet identity changed"));
        }
    }
    let length_size = avcc_length_size(&stream.extradata)?;
    let left = &reference.packets[video_index];
    let right = &candidate.packets[video_index];
    if left.is_empty() || left.len() != right.len() {
        return Err("video access-unit count is empty or changed".into());
    }
    let reference_sei = identifier_sei(reference_version)?;
    let candidate_sei = identifier_sei(candidate_version)?;
    let identifiers = IdentifierPair {
        reference: IdentifierExpectation {
            nal: &reference_sei,
            version: reference_version,
        },
        candidate: IdentifierExpectation {
            nal: &candidate_sei,
            version: candidate_version,
        },
    };
    let mut approved_packet = None;
    let mut reference_nals = Vec::new();
    let mut candidate_nals = Vec::new();
    for (index, (a, b)) in left.iter().zip(right).enumerate() {
        if (a.pts, a.dts, a.duration, a.flags, &a.side_data)
            != (b.pts, b.dts, b.duration, b.flags, &b.side_data)
        {
            return Err(format!(
                "access unit {index} timestamp/key/side data changed"
            ));
        }
        let comparison = compare_packet(&a.payload, &b.payload, length_size, &identifiers, index)?;
        reference_nals.extend(comparison.reference_nals);
        candidate_nals.extend(comparison.candidate_nals);
        if !comparison.differences.is_empty() {
            if index != 0 || comparison.differences.len() != 1 || approved_packet.is_some() {
                return Err(
                    "expected one known identifier difference in the first video AU".into(),
                );
            }
            approved_packet = Some((
                &a.payload,
                &b.payload,
                comparison.differences.into_iter().next().unwrap(),
            ));
        }
    }
    if reference.frames.is_empty() || reference.frames != candidate.frames {
        return Err("software decoded frames/pixels/timestamps changed".into());
    }
    let (reference_packet, candidate_packet, difference) =
        approved_packet.ok_or("expected exactly one attested driver-identifier difference")?;
    let version_start = 3 + UUID.len() + ENCODER_IDENTIFIER_PREFIX.len();
    let version_end = reference_sei.len().saturating_sub(" ()\0".len() + 1);
    if difference.first_difference_offset < version_start
        || difference.first_difference_offset >= version_end
        || reference_sei[reference_sei.len() - 1] != 0x80
        || candidate_sei[candidate_sei.len() - 1] != 0x80
    {
        return Err(
            "identifier NAL difference is outside version bytes or lacks exact RBSP trailing byte"
                .into(),
        );
    }
    let differing_bytes_confined_to_version = reference_sei
        .iter()
        .zip(&candidate_sei)
        .enumerate()
        .all(|(offset, (before, after))| {
            before == after || (version_start..version_end).contains(&offset)
        });
    if !differing_bytes_confined_to_version {
        return Err("identifier NAL differs outside the attested iHD version token".into());
    }
    // Stronger than ignoring an mdat box: ALL container bytes must agree after
    // replacing the one unique packet whose every NAL was independently checked.
    let normalized =
        replace_unique_packet(&candidate.file_bytes, candidate_packet, reference_packet)?;
    if normalized != reference.file_bytes {
        return Err("container bytes differ beyond the proven identifier packet".into());
    }
    Ok(DriverPairReceipt {
        schema: "asciiflow-h264-driver-portability-v1",
        reference_ihd_version: reference_version.to_owned(),
        candidate_ihd_version: candidate_version.to_owned(),
        video_stream_index: video_index,
        stream_count: reference.streams.len(),
        audio_and_other_streams_packet_identity: true,
        stream_metadata_identity: true,
        packet_timing_flags_and_side_data_identity: true,
        packet_interleave_identity: true,
        decoded_frames_identity: true,
        reference_container_sha256: digest(&reference.file_bytes)?,
        candidate_container_sha256: digest(&candidate.file_bytes)?,
        normalized_container_identity: true,
        known_identifier: KnownIdentifierEvidence {
            sei_payload_type: 5,
            uuid: UUID_HEX,
            fixed_prefix: ENCODER_IDENTIFIER_PREFIX,
            reference_version: reference_version.to_owned(),
            candidate_version: candidate_version.to_owned(),
            rbsp_trailing_byte: 0x80,
            reference_payload_size: usize::from(reference_sei[2]),
            candidate_payload_size: usize::from(candidate_sei[2]),
            payload_size_equal: reference_sei[2] == candidate_sei[2],
            full_expected_nal_bytes_match: true,
            differing_bytes_confined_to_version,
        },
        reference_nals,
        candidate_nals,
        approved_differences: vec![difference],
    })
}

fn nal_json(nal: &NalEvidence) -> serde_json::Value {
    serde_json::json!({
        "access_unit": nal.access_unit,
        "nal_index": nal.nal_index,
        "nal_type": nal.nal_type,
        "nal_name": nal.nal_name,
        "classification": nal.classification,
        "size": nal.size,
        "sha256": nal.sha256,
        "known_identifier_version": nal.known_identifier_version,
        "known_identifier_uuid": nal.known_identifier_uuid,
        "known_sei_payload_type": nal.known_sei_payload_type,
    })
}

fn difference_json(difference: &NalDifference) -> serde_json::Value {
    serde_json::json!({
        "access_unit": difference.access_unit,
        "nal_index": difference.nal_index,
        "nal_type": difference.nal_type,
        "nal_name": difference.nal_name,
        "reference_size": difference.reference_size,
        "candidate_size": difference.candidate_size,
        "reference_sha256": difference.reference_sha256,
        "candidate_sha256": difference.candidate_sha256,
        "first_difference_offset": difference.first_difference_offset,
        "reference_byte_at_first_difference": difference.reference_byte_at_first_difference,
        "candidate_byte_at_first_difference": difference.candidate_byte_at_first_difference,
        "reference_identifier_version": difference.reference_identifier_version,
        "candidate_identifier_version": difference.candidate_identifier_version,
    })
}

fn receipt_json(receipt: &DriverPairReceipt) -> serde_json::Value {
    serde_json::json!({
        "schema": receipt.schema,
        "reference_iHD_version": receipt.reference_ihd_version,
        "candidate_iHD_version": receipt.candidate_ihd_version,
        "video_stream_index": receipt.video_stream_index,
        "stream_count": receipt.stream_count,
        "audio_and_other_streams_packet_identity": receipt.audio_and_other_streams_packet_identity,
        "stream_metadata_identity": receipt.stream_metadata_identity,
        "packet_timing_flags_and_side_data_identity": receipt.packet_timing_flags_and_side_data_identity,
        "packet_interleave_identity": receipt.packet_interleave_identity,
        "decoded_frames_identity": receipt.decoded_frames_identity,
        "reference_container_sha256": receipt.reference_container_sha256,
        "candidate_container_sha256": receipt.candidate_container_sha256,
        "normalized_container_identity": receipt.normalized_container_identity,
        "known_identifier": {
            "sei_payload_type": receipt.known_identifier.sei_payload_type,
            "uuid": receipt.known_identifier.uuid,
            "fixed_encoder_vaapi_prefix": receipt.known_identifier.fixed_prefix,
            "reference_iHD_version": receipt.known_identifier.reference_version,
            "candidate_iHD_version": receipt.known_identifier.candidate_version,
            "rbsp_trailing_byte": receipt.known_identifier.rbsp_trailing_byte,
            "reference_payload_size": receipt.known_identifier.reference_payload_size,
            "candidate_payload_size": receipt.known_identifier.candidate_payload_size,
            "payload_size_equal": receipt.known_identifier.payload_size_equal,
            "full_expected_nal_bytes_match": receipt.known_identifier.full_expected_nal_bytes_match,
            "differing_bytes_confined_to_version": receipt.known_identifier.differing_bytes_confined_to_version,
        },
        "reference_nals": receipt.reference_nals.iter().map(nal_json).collect::<Vec<_>>(),
        "candidate_nals": receipt.candidate_nals.iter().map(nal_json).collect::<Vec<_>>(),
        "approved_differences": receipt.approved_differences.iter().map(difference_json).collect::<Vec<_>>(),
    })
}

fn write_receipt(path: &Path, receipt: &serde_json::Value) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("create forensic receipt {}: {error}", path.display()))?;
    serde_json::to_writer_pretty(&mut file, receipt)
        .map_err(|error| format!("serialize forensic receipt: {error}"))?;
    file.write_all(b"\n")
        .map_err(|error| format!("write forensic receipt: {error}"))
}

#[test]
#[ignore = "requires caller-attested cross-driver artifacts"]
fn compare_h264_driver_pair_from_env() {
    let value = |key| std::env::var(key).expect(key);
    let outcome: Result<(), String> = (|| {
        let reference = inspect(Path::new(&value("ASCIIFLOW_REGRESSION_REFERENCE")))?;
        let candidate = inspect(Path::new(&value("ASCIIFLOW_REGRESSION_CANDIDATE")))?;
        let comparison = compare_driver_pair(
            &reference,
            &candidate,
            &value("ASCIIFLOW_PORTABILITY_REFERENCE_IHD_VERSION"),
            &value("ASCIIFLOW_PORTABILITY_CANDIDATE_IHD_VERSION"),
        )?;
        if let Some(path) = std::env::var_os("ASCIIFLOW_H264_DRIVER_FORENSIC_RECEIPT") {
            write_receipt(Path::new(&path), &receipt_json(&comparison))?;
        }
        Ok(())
    })();
    if outcome.is_ok() {
        println!(
            "Tier 1A raw packet identity: FAIL (approved identifier NAL remains byte-different)"
        );
    }
    println!(
        "Tier 1B-P cross-driver portability: {}",
        if outcome.is_ok() { "PASS" } else { "FAIL" }
    );
    outcome.unwrap();
    println!(
        "Tier 1C independent software decoded identity: PASS\nTier 2 proven-packet-only container identity: PASS"
    );
}

#[test]
fn driver_identifier_contract_rejects_all_other_coded_changes() {
    let before = identifier_sei("26.1.5").unwrap();
    let after = identifier_sei("25.4.6").unwrap();
    let identifiers = IdentifierPair {
        reference: IdentifierExpectation {
            nal: &before,
            version: "26.1.5",
        },
        candidate: IdentifierExpectation {
            nal: &after,
            version: "25.4.6",
        },
    };
    let compare = |left: &[u8], right: &[u8]| compare_packet(left, right, 4, &identifiers, 0);
    let packet = |nal: &[u8]| {
        let mut bytes = (nal.len() as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(nal);
        bytes
    };
    assert_eq!(
        compare(&packet(&before), &packet(&after))
            .unwrap()
            .differences
            .len(),
        1
    );
    for kind in [1, 5, 7, 8, 9] {
        let a = packet(&[kind, 0x80]);
        let b = packet(&[kind, 0x81]);
        assert!(compare(&a, &b).is_err());
    }
    for unknown in [2, 3, 4, 10, 12, 13, 14, 20, 31] {
        let nal = packet(&[unknown, 0x80]);
        assert!(compare(&nal, &nal).is_ok());
    }
    let unknown_sei = packet(&[0x06, 0x04, 0x02, 0xaa, 0xbb, 0x80]);
    assert!(compare(&unknown_sei, &unknown_sei).is_ok());
    let changed_unknown_sei = packet(&[0x06, 0x04, 0x02, 0xaa, 0xbc, 0x80]);
    assert!(compare(&unknown_sei, &changed_unknown_sei).is_err());
    for index in 0..after.len() {
        let mut altered = after.clone();
        altered[index] ^= 1;
        assert!(compare(&packet(&before), &packet(&altered)).is_err());
    }
    let bytes = packet(&before);
    for end in 0..bytes.len() {
        assert!(compare(&bytes[..end], &packet(&after)).is_err());
    }
    let unexpected_uuid = {
        let mut sei = after.clone();
        sei[3] ^= 1;
        packet(&sei)
    };
    assert!(compare(&packet(&before), &unexpected_uuid).is_err());
    let unexpected_field = {
        let mut sei = after.clone();
        let field = sei
            .windows(b"VAAPI 1.23.0".len())
            .position(|w| w == b"VAAPI 1.23.0")
            .unwrap();
        sei[field] ^= 1;
        packet(&sei)
    };
    assert!(compare(&packet(&before), &unexpected_field).is_err());
    let picture_timing = packet(&[0x06, 0x01, 0x01, 0x01, 0x80]);
    let changed_picture_timing = packet(&[0x06, 0x01, 0x01, 0x02, 0x80]);
    assert!(compare(&picture_timing, &changed_picture_timing).is_err());
    for version in ["", "26", "26.1", "26.a.5", "26.1.5\0", "26.1.5.0"] {
        assert!(identifier_sei(version).is_err());
    }
    assert!(replace_unique_packet(b"abab", b"ab", b"cd").is_err());
    assert!(replace_unique_packet(b"none", b"ab", b"cd").is_err());
}

#[test]
fn complete_pair_rejects_semantic_and_container_mutations_without_pixel_changes() {
    use common::media_regression::PacketFacts;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/codecs/hevc-main8-bframes.mp4");
    let mut before = inspect(&fixture).unwrap();
    before.streams.truncate(1);
    let stream = &mut before.streams[0];
    stream.codec = ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_H264 as i32;
    stream.extradata = vec![1, 100, 0, 42, 0xff];
    stream.width = 1920;
    stream.height = 1080;
    stream.avg_frame_rate = (50, 1);
    stream.time_base = (1, 50);
    stream.color_primaries = 1;
    stream.color_transfer = 1;
    stream.color_matrix = 1;
    stream.color_range = 1;
    let packet = |nal: &[u8]| {
        let mut bytes = (nal.len() as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(nal);
        bytes
    };
    let left_sei = identifier_sei("26.1.5").unwrap();
    let right_sei = identifier_sei("25.4.6").unwrap();
    let make_first = |sei: &[u8]| {
        [
            packet(&[0x67, 0x80]),
            packet(&[0x68, 0x80]),
            packet(&[0x06, 0x01, 0x01, 0x01, 0x80]),
            packet(&[0x06, 0x04, 0x02, 0xaa, 0xbb, 0x80]),
            packet(sei),
            packet(&[0x65, 0x80]),
        ]
        .concat()
    };
    before.packets = vec![
        (0..300)
            .map(|index| PacketFacts {
                pts: index,
                dts: index,
                duration: 1,
                flags: i32::from(matches!(index, 0 | 250)),
                side_data: vec![],
                payload: if index == 0 {
                    make_first(&left_sei)
                } else {
                    packet(&[0x41, 0x80])
                },
            })
            .collect(),
    ];
    before.interleave = vec![0; 300];
    before.frames = vec![before.frames[0].clone(); 300];
    let mut audio_stream = before.streams[0].clone();
    audio_stream.index = 1;
    audio_stream.kind = ffi::AVMediaType::AVMEDIA_TYPE_AUDIO as i32;
    audio_stream.codec = ffi::AVCodecID::AV_CODEC_ID_AAC as i32;
    audio_stream.width = 0;
    audio_stream.height = 0;
    audio_stream.avg_frame_rate = (0, 0);
    audio_stream.sample_rate = 48_000;
    audio_stream.channels = 2;
    audio_stream.channel_layout = "stereo".into();
    before.streams.push(audio_stream);
    before.packets.push(vec![PacketFacts {
        pts: 0,
        dts: 0,
        duration: 1024,
        flags: 1,
        side_data: vec![],
        payload: vec![0x21, 0x10],
    }]);
    before.interleave.push(1);
    before.file_bytes = b"unique container prefix".to_vec();
    before
        .file_bytes
        .extend_from_slice(&before.packets[0][0].payload);
    before
        .file_bytes
        .extend_from_slice(&before.packets[1][0].payload);
    before
        .file_bytes
        .extend_from_slice(b"unchanged container suffix");
    let mut after = before.clone();
    after.packets[0][0].payload = make_first(&right_sei);
    after.file_bytes = replace_unique_packet(
        &before.file_bytes,
        &before.packets[0][0].payload,
        &after.packets[0][0].payload,
    )
    .unwrap();
    let check =
        |candidate: &MediaSnapshot| compare_driver_pair(&before, candidate, "26.1.5", "25.4.6");
    assert!(check(&after).is_ok());
    for field in [
        "primaries",
        "transfer",
        "matrix",
        "range",
        "fps",
        "width",
        "profile",
        "level",
    ] {
        let mut changed = after.clone();
        let stream = &mut changed.streams[0];
        match field {
            "primaries" => stream.color_primaries = 9,
            "transfer" => stream.color_transfer = 16,
            "matrix" => stream.color_matrix = 9,
            "range" => stream.color_range = 2,
            "fps" => stream.avg_frame_rate = (25, 1),
            "width" => stream.width = 1280,
            "profile" => stream.profile += 1,
            "level" => stream.level += 1,
            _ => unreachable!(),
        }
        assert!(check(&changed).is_err(), "{field} mutation passed");
    }
    for mutation in 0..10 {
        let mut changed = after.clone();
        match mutation {
            0 => changed.packets[0][1].pts += 1,
            1 => changed.packets[0][1].dts += 1,
            2 => changed.packets[0][1].duration += 1,
            3 => changed.packets[0][1].flags ^= 1,
            4 => changed.packets[0][1].side_data.push((0, vec![1])),
            5 => {
                changed.packets[0].pop();
            }
            6 => changed.streams[0].extradata.push(1),
            7 => changed.file_bytes[0] ^= 1,
            8 => {
                changed.frames.pop();
            }
            9 => changed.packets[0].swap(0, 1),
            _ => unreachable!(),
        }
        assert!(check(&changed).is_err(), "mutation {mutation} passed");
    }
    let mut changed_audio = after.clone();
    changed_audio.packets[1][0].payload[0] ^= 1;
    assert!(
        check(&changed_audio).is_err(),
        "audio payload mutation passed"
    );
    let mut changed_audio_time = after.clone();
    changed_audio_time.packets[1][0].pts += 1;
    assert!(
        check(&changed_audio_time).is_err(),
        "audio timestamp mutation passed"
    );
    assert!(compare_driver_pair(&before, &after, "26.1.5", "26.1.5").is_err());
}

#[test]
fn complete_pair_accepts_geometry_rate_au_count_and_stream_order_variation() {
    use common::media_regression::PacketFacts;

    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/codecs/hevc-main8-bframes.mp4");
    let mut reference = inspect(&fixture).unwrap();
    reference.streams.truncate(1);
    let mut video = reference.streams[0].clone();
    video.codec = ffi::AVCodecID::AV_CODEC_ID_H264 as i32;
    video.extradata = vec![1, 100, 0, 42, 0xff];
    video.width = 640;
    video.height = 360;
    video.avg_frame_rate = (24, 1);
    video.time_base = (1, 24);
    video.color_primaries = 1;
    video.color_transfer = 1;
    video.color_matrix = 1;
    video.color_range = 1;

    let mut audio = video.clone();
    audio.kind = ffi::AVMediaType::AVMEDIA_TYPE_AUDIO as i32;
    audio.codec = ffi::AVCodecID::AV_CODEC_ID_AAC as i32;
    audio.width = 0;
    audio.height = 0;
    audio.avg_frame_rate = (0, 0);
    audio.sample_rate = 48_000;
    audio.channels = 2;
    audio.channel_layout = "stereo".into();
    audio.index = 0;
    video.index = 1;
    reference.streams = vec![audio, video];

    let packet = |nal: &[u8]| {
        let mut bytes = (nal.len() as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(nal);
        bytes
    };
    let reference_sei = identifier_sei("26.1.5").unwrap();
    let candidate_sei = identifier_sei("25.4.6").unwrap();
    let make_first = |sei: &[u8]| [packet(sei), packet(&[0x65, 0x80])].concat();
    let mut video_packets = (0..3)
        .map(|index| PacketFacts {
            pts: index,
            dts: index,
            duration: 1,
            flags: i32::from(index == 0),
            side_data: vec![],
            payload: if index == 0 {
                make_first(&reference_sei)
            } else {
                packet(&[0x41, 0x80])
            },
        })
        .collect::<Vec<_>>();
    reference.packets = vec![
        vec![PacketFacts {
            pts: 0,
            dts: 0,
            duration: 1024,
            flags: 1,
            side_data: vec![],
            payload: vec![0x21, 0x10],
        }],
        std::mem::take(&mut video_packets),
    ];
    reference.interleave = vec![0, 1, 1, 1];
    reference.frames = vec![reference.frames[0].clone(); 3];
    reference.file_bytes = b"varying profile container prefix".to_vec();
    reference
        .file_bytes
        .extend_from_slice(&reference.packets[1][0].payload);
    reference
        .file_bytes
        .extend_from_slice(&reference.packets[0][0].payload);
    reference
        .file_bytes
        .extend_from_slice(b"unchanged container suffix");

    let mut candidate = reference.clone();
    candidate.packets[1][0].payload = make_first(&candidate_sei);
    candidate.file_bytes = replace_unique_packet(
        &reference.file_bytes,
        &reference.packets[1][0].payload,
        &candidate.packets[1][0].payload,
    )
    .unwrap();

    let receipt = compare_driver_pair(&reference, &candidate, "26.1.5", "25.4.6").unwrap();
    assert_eq!(receipt.video_stream_index, 1);
    assert_eq!(receipt.stream_count, 2);
    assert!(receipt.audio_and_other_streams_packet_identity);
    assert_eq!(reference.packets[1].len(), 3);
    assert_eq!(reference.streams[1].width, 640);
    assert_eq!(reference.streams[1].avg_frame_rate, (24, 1));
}
