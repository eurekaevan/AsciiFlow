//! An additive, deliberately narrower cross-driver gate. The historical H.264
//! oracle and its Lavc-patch allowlist are not changed or called by this gate.
#[allow(dead_code)] // Shared inspection helpers also serve the unchanged legacy oracle.
mod common {
    pub mod h264_bitstream;
    pub mod media_regression;
}

use common::h264_bitstream::{avcc_length_size, parse_avcc};
use common::media_regression::{MediaSnapshot, inspect};
use std::path::Path;

const UUID: [u8; 16] = [
    0x59, 0x94, 0x8b, 0x28, 0x11, 0xec, 0x45, 0xaf, 0x96, 0x75, 0x19, 0xd4, 0x1f, 0xea, 0xa9, 0x4d,
];

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
    let text = format!(
        "Lavc62.28.103 / VAAPI 1.23.0 / Intel iHD driver for Intel(R) Gen Graphics - {version} ()\0"
    );
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

fn compare_packet(
    before: &[u8],
    after: &[u8],
    length_size: usize,
    reference_sei: &[u8],
    candidate_sei: &[u8],
) -> Result<usize, String> {
    let left = parse_avcc(before, length_size)?;
    let right = parse_avcc(after, length_size)?;
    if left.len() != right.len() || before.len() != after.len() {
        return Err("NAL count or packet length changed".into());
    }
    let mut changes = 0;
    for (index, (a, b)) in left.iter().zip(&right).enumerate() {
        if a.bytes[0] != b.bytes[0] {
            return Err(format!("NAL {index} header/type/ref_idc changed"));
        }
        match a.kind {
            6 => {
                if a.bytes != reference_sei || b.bytes != candidate_sei {
                    return Err(format!("NAL {index}: unknown or changed semantic SEI"));
                }
                changes += usize::from(a.bytes != b.bytes);
            }
            1 | 5 | 7 | 8 | 9 => {
                if a.bytes != b.bytes {
                    return Err(format!("NAL {index} type {} coded bytes changed", a.kind));
                }
            }
            kind => return Err(format!("unqualified NAL type {kind}")),
        }
    }
    Ok(changes)
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
) -> Result<(), String> {
    if reference_version == candidate_version {
        return Err("Tier 1B-P is not a same-stack replacement oracle".into());
    }
    if reference.streams != candidate.streams
        || reference.streams.len() != 1
        || reference.streams[0].codec != ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_H264 as i32
        || reference.format_start_time != candidate.format_start_time
        || reference.format_duration != candidate.format_duration
        || reference.format_tags != candidate.format_tags
        || reference.interleave != candidate.interleave
        || reference.packets.len() != 1
        || candidate.packets.len() != 1
    {
        return Err("stream/extradata/color/timing/container metadata changed".into());
    }
    let stream = &reference.streams[0];
    if (stream.width, stream.height, stream.avg_frame_rate) != (1920, 1080, (50, 1))
        || (
            stream.color_primaries,
            stream.color_transfer,
            stream.color_matrix,
            stream.color_range,
        ) != (1, 1, 1, 1)
        || stream.time_base.0 <= 0
        || stream.time_base.1 <= 0
    {
        return Err("outside the qualified 1080p/50 BT.709 limited profile".into());
    }
    let length_size = avcc_length_size(&reference.streams[0].extradata)?;
    let left = &reference.packets[0];
    let right = &candidate.packets[0];
    if left.len() != 300 || right.len() != 300 {
        return Err("this qualification requires 300 complete access units".into());
    }
    let reference_sei = identifier_sei(reference_version)?;
    let candidate_sei = identifier_sei(candidate_version)?;
    let mut approved = Vec::new();
    for (index, (a, b)) in left.iter().zip(right).enumerate() {
        let ticks = i128::from(stream.time_base.0) * 50;
        if i128::from(a.pts) * ticks != index as i128 * i128::from(stream.time_base.1)
            || a.dts != a.pts
            || i128::from(a.duration) * ticks != i128::from(stream.time_base.1)
            || (a.flags & ffmpeg_sys_next::AV_PKT_FLAG_KEY != 0) != matches!(index, 0 | 250)
        {
            return Err(format!(
                "access unit {index} outside qualified CFR/GOP structure"
            ));
        }
        if (a.pts, a.dts, a.duration, a.flags, &a.side_data)
            != (b.pts, b.dts, b.duration, b.flags, &b.side_data)
        {
            return Err(format!(
                "access unit {index} timestamp/key/side data changed"
            ));
        }
        let changes = compare_packet(
            &a.payload,
            &b.payload,
            length_size,
            &reference_sei,
            &candidate_sei,
        )?;
        if changes != 0 {
            if index != 0 || changes != 1 {
                return Err("identifier difference outside the qualified first AU".into());
            }
            approved.push((&a.payload, &b.payload));
        }
    }
    if approved.len() != 1 {
        return Err("expected exactly one attested driver-identifier difference".into());
    }
    // Stronger than ignoring an mdat box: ALL container bytes must agree after
    // replacing the one unique packet whose every NAL was independently checked.
    let normalized = replace_unique_packet(&candidate.file_bytes, approved[0].1, approved[0].0)?;
    if normalized != reference.file_bytes {
        return Err("container bytes differ beyond the proven identifier packet".into());
    }
    // This check is separate from the coded-data decision above. Pixel identity
    // never supplies a missing SPS/PPS/VCL/SEI or timeline proof.
    if reference.frames.len() != 300 || reference.frames != candidate.frames {
        return Err("software decoded frames/pixels/timestamps changed".into());
    }
    Ok(())
}

#[test]
#[ignore = "requires caller-attested cross-driver artifacts"]
fn compare_h264_driver_pair_from_env() {
    let value = |key| std::env::var(key).expect(key);
    let outcome = (|| {
        let reference = inspect(Path::new(&value("ASCIIFLOW_REGRESSION_REFERENCE")))?;
        let candidate = inspect(Path::new(&value("ASCIIFLOW_REGRESSION_CANDIDATE")))?;
        compare_driver_pair(
            &reference,
            &candidate,
            &value("ASCIIFLOW_PORTABILITY_REFERENCE_IHD_VERSION"),
            &value("ASCIIFLOW_PORTABILITY_CANDIDATE_IHD_VERSION"),
        )
    })();
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
    let packet = |nal: &[u8]| {
        let mut bytes = (nal.len() as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(nal);
        bytes
    };
    assert_eq!(
        compare_packet(&packet(&before), &packet(&after), 4, &before, &after),
        Ok(1)
    );
    for kind in [1, 5, 7, 8, 9] {
        let a = packet(&[kind, 0x80]);
        let b = packet(&[kind, 0x81]);
        assert!(compare_packet(&a, &b, 4, &before, &after).is_err());
    }
    for unknown in [2, 3, 4, 10, 12, 13, 14, 20, 31] {
        let nal = packet(&[unknown, 0x80]);
        assert!(compare_packet(&nal, &nal, 4, &before, &after).is_err());
    }
    for index in 0..after.len() {
        let mut altered = after.clone();
        altered[index] ^= 1;
        assert!(compare_packet(&packet(&before), &packet(&altered), 4, &before, &after).is_err());
    }
    let bytes = packet(&before);
    for end in 0..bytes.len() {
        assert!(compare_packet(&bytes[..end], &packet(&after), 4, &before, &after).is_err());
    }
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
    before.file_bytes = b"unique container prefix".to_vec();
    before
        .file_bytes
        .extend_from_slice(&before.packets[0][0].payload);
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
    assert_eq!(check(&after), Ok(()));
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
    assert!(compare_driver_pair(&before, &after, "26.1.5", "26.1.5").is_err());
}
