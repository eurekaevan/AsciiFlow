#![cfg(feature = "hdr-to-sdr-qualification")]

//! Real resident SDR packing and VAAPI surface reuse. These timings are not a
//! performance benchmark: every frame is downloaded and hashed independently.
use asciiflow_core::{AsciiConfig, PixelFormat, Result};
use asciiflow_cpu::sdr_output::{NonlinearBt709Rgb, pack};
use asciiflow_font::GlyphAtlas;
use asciiflow_interop::{DrmPrimeMapping, pack_completed_sdr_surface};
use asciiflow_media::{
    DecodeMode, Decoder, VaapiDecodedFrame, VaapiOptions, VaapiSdrQualificationPool,
};
use asciiflow_vulkan::VulkanHdrToSdrQualification;
use std::{
    fs::OpenOptions,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const FRAMES: usize = 300;
const CYCLES: usize = 10;

fn fd_count() -> usize {
    std::fs::read_dir("/proc/self/fd").unwrap().count()
}

fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find(|line| line.starts_with("VmRSS:"))
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut child = Command::new("sha256sum")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("sha256sum required");
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned()
}

fn fixture_hash(path: &Path, name: &str) -> String {
    let expected = include_str!("../../../tests/fixtures/codecs/SHA256SUMS")
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let hash = fields.next()?;
            (fields.next()? == format!("./{name}")).then_some(hash)
        })
        .expect("canonical fixture checksum missing");
    let hash = hash_bytes(&std::fs::read(path).unwrap());
    assert_eq!(hash, expected, "canonical fixture identity changed");
    hash
}

// Input mapping ownership also survives uncertain stage/submit/completion.
// The output bridge separately quarantines its actual VAAPI allocation.
fn checked<T>(
    result: Result<T>,
    slots: &[VulkanHdrToSdrQualification; 2],
    mappings: &mut Vec<DrmPrimeMapping<VaapiDecodedFrame>>,
) -> T {
    match result {
        Ok(value) => value,
        Err(error) => {
            if slots
                .iter()
                .any(VulkanHdrToSdrQualification::is_device_abandoned)
            {
                std::mem::forget(std::mem::take(mappings));
            }
            panic!("C-4A stress operation failed: {error}");
        }
    }
}

fn native_tags(surface: &asciiflow_media::VaapiEncoderFrame) -> [i64; 6] {
    let frame = unsafe { &*surface.as_raw_ptr() };
    [
        frame.color_primaries as i64,
        frame.color_trc as i64,
        frame.colorspace as i64,
        frame.color_range as i64,
        frame.chroma_location as i64,
        frame.pts,
    ]
}

fn assert_codes(bytes: &[u8], format: PixelFormat) {
    let y_samples = WIDTH as usize * HEIGHT as usize;
    assert_eq!(bytes.len(), format.frame_byte_len(WIDTH, HEIGHT).unwrap());
    match format {
        PixelFormat::Nv12 => {
            assert!(bytes[..y_samples].iter().all(|v| (16..=235).contains(v)));
            assert!(bytes[y_samples..].iter().all(|v| (16..=240).contains(v)));
        }
        PixelFormat::P010Le => {
            for (index, word) in bytes.as_chunks::<2>().0.iter().enumerate() {
                let word = u16::from_le_bytes([word[0], word[1]]);
                assert_eq!(word & 63, 0, "P010 low six bits at sample {index}");
                assert!(
                    (64..=if index < y_samples { 940 } else { 960 }).contains(&(word >> 6)),
                    "P010 invalid limited code at sample {index}"
                );
            }
        }
    }
}

fn exact_oracle(expected: &[u8], actual: &[u8], name: &str, frame: usize, hash: &str) {
    assert_eq!(expected.len(), actual.len());
    let count = expected.iter().zip(actual).filter(|(a, b)| a != b).count();
    if count != 0 {
        let first: Vec<_> = expected
            .iter()
            .zip(actual)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .take(16)
            .map(|(index, (cpu, gpu))| format!("{{\"byte\":{index},\"cpu\":{cpu},\"gpu\":{gpu}}}"))
            .collect();
        eprintln!(
            "{{\"status\":\"FAIL\",\"source\":\"{name}\",\"cycle\":0,\"frame\":{frame},\"packed_sha256\":\"{hash}\",\"oracle_sha256\":\"{}\",\"mismatched_bytes\":{count},\"first_mismatches\":[{}]}}",
            hash_bytes(expected),
            first.join(",")
        );
        panic!("C-4A exact f64 oracle parity failed; numerical evidence frozen above");
    }
}

#[test]
#[ignore = "real Intel VAAPI/DMA-BUF; 3000 frames per SDR format, two private resident C3 slots"]
fn c4a_dual_slot_3000_frame_surface_stress() {
    assert_eq!(std::env::var("ASCIIFLOW_VULKAN_VALIDATION").unwrap(), "1");
    let destination =
        std::env::var_os("C4A_STRESS_REPORT").expect("set a new C4A_STRESS_REPORT path");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/codecs");
    let config = AsciiConfig {
        color: true,
        ..Default::default()
    };
    let atlas = GlyphAtlas::builtin(&config.font, &config.charset).unwrap();
    let mut reports = Vec::new();
    for (name, format, label) in [
        ("hevc-main10-pq-c3-legal-v1.mp4", PixelFormat::Nv12, "NV12"),
        ("av1-main10-pq-c3-legal-v1.mp4", PixelFormat::P010Le, "P010"),
    ] {
        let path = root.join(name);
        let input_hash = fixture_hash(&path, name);
        let before = fd_count();
        let mut peak = before;
        let first = VulkanHdrToSdrQualification::new()
            .unwrap()
            .with_atlas(atlas.clone(), &config)
            .unwrap();
        eprintln!("C-4A qualification device: {:?}", first.device_info());
        let second = first.try_fork().unwrap();
        let observe = first.validation_observer();
        let mut slots = [first, second];
        let pool =
            VaapiSdrQualificationPool::new(Path::new("/dev/dri/renderD128"), WIDTH, HEIGHT, format)
                .unwrap();
        let mut hashes = Vec::with_capacity(FRAMES);
        let mut payload = None;
        let mut pack_payload = None;
        let mut steady_fds = None;
        let mut cycle_fds = Vec::new();
        let mut cycle_rss = Vec::new();
        let mut total = 0;
        for cycle in 0..CYCLES {
            let mut decoder =
                Decoder::open_pq_qualification(&path, DecodeMode::Vaapi, VaapiOptions::default())
                    .unwrap();
            let rate = decoder.info().frame_rate;
            assert_eq!(i64::from(rate.numerator), 50 * i64::from(rate.denominator));
            assert_eq!(decoder.info().frame_count, Some(FRAMES as u64));
            for pair in 0..FRAMES / 2 {
                let order = if (cycle + pair) % 2 == 0 {
                    [0, 1]
                } else {
                    [1, 0]
                };
                let mut mappings = Vec::with_capacity(2);
                for slot_index in order {
                    let decoded = decoder
                        .next_vaapi_frame()
                        .unwrap()
                        .expect("canonical frame missing");
                    let desc = decoded.desc().clone();
                    assert_eq!((desc.width, desc.height), (WIDTH, HEIGHT));
                    mappings.push(DrmPrimeMapping::map_direct_read(decoded).unwrap());
                    let mapping = mappings.last().unwrap();
                    let result = slots[slot_index].stage_external(
                        &desc,
                        mapping.pts(),
                        &config,
                        mapping.duplicate_external_p010_planes().unwrap(),
                        false,
                    );
                    checked(result, &slots, &mut mappings);
                }
                for index in order {
                    let result = slots[index].submit_staged();
                    checked(result, &slots, &mut mappings);
                }
                peak = peak.max(fd_count());
                for (offset, slot_index) in order.into_iter().enumerate() {
                    let frame = pair * 2 + offset;
                    let result = slots[slot_index].complete();
                    let output = checked(result, &slots, &mut mappings);
                    assert_eq!(output.pts, Some(frame as i64 * 1000));
                    assert_eq!((output.width, output.height), (WIDTH, HEIGHT));
                    assert_eq!(&output.diagnostics[..5], &[0; 5]);
                    let surface = pool.acquire().unwrap();
                    let tags = native_tags(&surface);
                    peak = peak.max(fd_count());
                    let result = pack_completed_sdr_surface(&mut slots[slot_index], surface, None);
                    let (surface, packed) = checked(result, &slots, &mut mappings);
                    assert_eq!(
                        tags,
                        native_tags(&surface),
                        "native metadata or timestamp changed"
                    );
                    assert_eq!(packed.diagnostics, [0; 3]);
                    assert_eq!(packed.validation_errors, 0);
                    assert_codes(&packed.bytes, format);
                    let downloaded = match format {
                        PixelFormat::Nv12 => surface.download_nv12(),
                        PixelFormat::P010Le => surface.download_p010(),
                    }
                    .unwrap();
                    assert_eq!(
                        downloaded.host().as_slice(),
                        packed.bytes,
                        "actual surface differs from resident pack"
                    );
                    let hash = hash_bytes(&packed.bytes);
                    if cycle == 0 {
                        let rgb: Vec<_> = output
                            .nonlinear_709
                            .iter()
                            .map(|pixel| NonlinearBt709Rgb(pixel.map(f64::from)))
                            .collect();
                        let expected = pack(WIDTH, HEIGHT, &rgb, format).unwrap();
                        exact_oracle(&expected, &packed.bytes, name, frame, &hash);
                        hashes.push(hash);
                    } else {
                        assert_eq!(
                            hash, hashes[frame],
                            "{name} cycle {cycle} frame {frame} slot {slot_index}"
                        );
                    }
                    let bytes = slots[slot_index].total_buffer_bytes_per_slot();
                    assert_eq!(
                        *payload.get_or_insert(bytes),
                        bytes,
                        "C3 buffer payload grew"
                    );
                    assert_eq!(
                        *pack_payload.get_or_insert(packed.buffer_bytes),
                        packed.buffer_bytes,
                        "packer buffer payload grew"
                    );
                    assert_eq!(slots[slot_index].validation_error_count(), 0);
                    total += 1;
                }
                drop(mappings);
                let fds = fd_count();
                peak = peak.max(fds);
                if cycle == 0 && pair == FRAMES / 2 - 1 {
                    steady_fds = Some(fds);
                } else if let Some(steady) = steady_fds {
                    assert!(
                        fds <= steady,
                        "{name} steady-state FD growth: {fds} > {steady}"
                    );
                }
            }
            assert!(
                decoder.next_vaapi_frame().unwrap().is_none(),
                "extra canonical frame"
            );
            drop(decoder);
            cycle_fds.push(fd_count());
            cycle_rss.push(rss_kib());
            assert!(
                cycle_fds.iter().all(|fds| *fds == cycle_fds[0]),
                "post-cycle FD count changed"
            );
            eprintln!("C4A stress {label}: {total}/3000 frames, sampled FD peak={peak}");
        }
        drop(pool);
        drop(slots);
        assert_eq!(
            observe(),
            0,
            "validation through complete slot/device teardown"
        );
        drop(observe);
        let after = fd_count();
        assert_eq!(after, before, "{name} leaked FDs after teardown");
        assert_eq!(total, CYCLES * FRAMES);
        reports.push(format!("{{\"status\":\"PASS\",\"source\":\"{name}\",\"format\":\"{label}\",\"color\":true,\"input_sha256\":\"{input_hash}\",\"width\":{WIDTH},\"height\":{HEIGHT},\"fps\":50,\"frames\":{total},\"cycles\":{CYCLES},\"slots\":2,\"fd_before\":{before},\"fd_sampled_peak\":{peak},\"fd_after\":{after},\"fd_steady_bound\":{},\"fd_after_cycles\":{cycle_fds:?},\"c3_bytes_per_slot\":{},\"pack_bytes_per_slot\":{},\"rss_kib_after_cycles\":{cycle_rss:?},\"oracle_frames\":{FRAMES},\"surface_readback_frames\":{total},\"validation_errors_through_teardown\":0,\"frame_hashes\":[{}]}}",
            steady_fds.unwrap(), payload.unwrap(), pack_payload.unwrap(),
            hashes.iter().map(|hash| format!("\"{hash}\"")).collect::<Vec<_>>().join(",")));
    }
    // Publish a success report only after both formats and all teardown gates.
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .unwrap();
    writeln!(file, "{{\"schema_version\":1,\"scope\":\"C4A resident SDR pack and VAAPI surface stress; no encoder; timings excluded from performance evidence\",\"reports\":[{}]}}", reports.join(",")).unwrap();
}
