#![cfg(feature = "hdr-to-sdr-qualification")]

//! Audits every decoded active P010 sample in the C3 legal-domain sources.
//! Unlike HdrPqReference, this checks the unclamped decoded RGB domain.

use asciiflow_core::{
    ChromaSubsampling, ColorSpace, FrameSource, PixelFormat, TransferCharacteristic, VideoCodec,
    VideoFrame,
    hdr_pq::{P010Codes, decode_limited, pq_eotf_nits, pq_inverse_eotf, ycbcr_to_pq},
};
use asciiflow_media::{DecodeMode, Decoder, VaapiDecodedFrame, VaapiOptions};
use ffmpeg_sys_next as ffi;
use std::{
    collections::HashMap,
    ffi::CString,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

const FRAME_COUNT: usize = 300;
const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const CACHE_LIMIT: usize = 4096;
const FIXTURES: [(&str, VideoCodec); 2] = [
    ("hevc-main10-pq-c3-legal-v1.mp4", VideoCodec::Hevc),
    ("av1-main10-pq-c3-legal-v1.mp4", VideoCodec::Av1),
];

#[derive(Clone, Copy, Debug, Default)]
struct PlaneHistogram([u64; 4]);

impl PlaneHistogram {
    fn add(&mut self, code: u16) {
        self.0[usize::from(code & 3)] += 1;
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct DomainCounts {
    negative_components: u64,
    above_1000_components: u64,
    non_finite_components: u64,
    out_of_range_rgb_components: u64,
    invalid_code_triplets: u64,
    max_nits: f64,
    min_nits: Option<f64>,
}

impl DomainCounts {
    fn merge(&mut self, other: Self) {
        self.negative_components += other.negative_components;
        self.above_1000_components += other.above_1000_components;
        self.non_finite_components += other.non_finite_components;
        self.out_of_range_rgb_components += other.out_of_range_rgb_components;
        self.invalid_code_triplets += other.invalid_code_triplets;
        self.max_nits = self.max_nits.max(other.max_nits);
        if let Some(nits) = other.min_nits {
            self.min_nits = Some(self.min_nits.map_or(nits, |current| current.min(nits)));
        }
    }
}

#[derive(Default)]
struct FixtureReport {
    fixture: String,
    input_sha256: String,
    frames: usize,
    violations: Vec<String>,
    low_two_bits: [PlaneHistogram; 3],
    invalid_p010_packing_samples: u64,
    domain: DomainCounts,
    cached_triplets: usize,
    pts_first: Option<i64>,
    pts_last: Option<i64>,
    pts_step: Option<i64>,
    stream_time_base: Option<(i32, i32)>,
}

impl FixtureReport {
    fn json(&self) -> String {
        let hist = |h: PlaneHistogram| format!("[{}, {}, {}, {}]", h.0[0], h.0[1], h.0[2], h.0[3]);
        let samples = |h: PlaneHistogram| h.0.iter().sum::<u64>().to_string();
        let violations = self
            .violations
            .iter()
            .map(|item| format!("\"{}\"", json_escape(item)))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "{{\"fixture\":\"{}\",\"input_sha256\":\"{}\",\"frames\":{},\"stream_time_base\":[{},{}],\"pts_first\":{},\"pts_last\":{},\"pts_step\":{},\"active_samples_y_u_v\":[{},{},{}],\"low_two_bits_y_u_v\":[{},{},{}],\"invalid_p010_packing_samples\":{},\"negative_rgb_components\":{},\"above_1000_nits_components\":{},\"non_finite_rgb_components\":{},\"out_of_range_rgb_components\":{},\"invalid_code_triplets\":{},\"min_nits\":{},\"max_nits\":{},\"cached_triplets\":{},\"violations\":[{}]}}",
            json_escape(&self.fixture),
            self.input_sha256,
            self.frames,
            self.stream_time_base.map_or(0, |value| value.0),
            self.stream_time_base.map_or(0, |value| value.1),
            option_i64(self.pts_first),
            option_i64(self.pts_last),
            option_i64(self.pts_step),
            samples(self.low_two_bits[0]),
            samples(self.low_two_bits[1]),
            samples(self.low_two_bits[2]),
            hist(self.low_two_bits[0]),
            hist(self.low_two_bits[1]),
            hist(self.low_two_bits[2]),
            self.invalid_p010_packing_samples,
            self.domain.negative_components,
            self.domain.above_1000_components,
            self.domain.non_finite_components,
            self.domain.out_of_range_rgb_components,
            self.domain.invalid_code_triplets,
            option_f64(self.domain.min_nits),
            self.domain.max_nits,
            self.cached_triplets,
            violations,
        )
    }
}

fn option_i64(value: Option<i64>) -> String {
    value.map_or_else(|| "null".into(), |value| value.to_string())
}

fn option_f64(value: Option<f64>) -> String {
    value
        .filter(|number| number.is_finite())
        .map_or_else(|| "null".into(), |number| number.to_string())
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0c}' => escaped.push_str("\\f"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            control if control.is_control() => {
                use std::fmt::Write as _;
                write!(escaped, "\\u{:04x}", u32::from(control)).unwrap();
            }
            character => escaped.push(character),
        }
    }
    escaped
}

struct InputFormat(*mut ffi::AVFormatContext);

impl Drop for InputFormat {
    fn drop(&mut self) {
        unsafe { ffi::avformat_close_input(&mut self.0) };
    }
}

fn stream_time_base(path: &Path) -> (i32, i32) {
    let path = CString::new(path.to_string_lossy().as_bytes()).unwrap();
    let mut context = std::ptr::null_mut();
    assert_eq!(
        unsafe {
            ffi::avformat_open_input(
                &mut context,
                path.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
            )
        },
        0
    );
    let context = InputFormat(context);
    assert!(unsafe { ffi::avformat_find_stream_info(context.0, std::ptr::null_mut()) } >= 0);
    let stream_index = unsafe {
        ffi::av_find_best_stream(
            context.0,
            ffi::AVMediaType::AVMEDIA_TYPE_VIDEO,
            -1,
            -1,
            std::ptr::null_mut(),
            0,
        )
    };
    assert!(stream_index >= 0);
    let stream = unsafe { *(*context.0).streams.add(stream_index as usize) };
    let time_base = unsafe { (*stream).time_base };
    assert!(time_base.num > 0 && time_base.den > 0);
    (time_base.num, time_base.den)
}

fn downloaded(frame: &VaapiDecodedFrame) -> VideoFrame {
    struct Transfer(*mut ffi::AVFrame);
    impl Drop for Transfer {
        fn drop(&mut self) {
            unsafe { ffi::av_frame_free(&mut self.0) };
        }
    }
    let transfer = Transfer(unsafe { ffi::av_frame_alloc() });
    assert!(!transfer.0.is_null());
    unsafe { (*transfer.0).format = ffi::AVPixelFormat::AV_PIX_FMT_P010LE as i32 };
    let status = unsafe { ffi::av_hwframe_transfer_data(transfer.0, frame.as_raw_ptr(), 0) };
    assert!(status >= 0, "VAAPI P010 download failed: {status}");
    let native = unsafe { &*transfer.0 };
    assert_eq!(native.format, ffi::AVPixelFormat::AV_PIX_FMT_P010LE as i32);
    let desc = frame.desc();
    let mut storage = asciiflow_core::HostFrame::new_zeroed(desc);
    let (y, uv) = storage.planes_mut(desc);
    let stride = desc.y_stride();
    for (plane, target, rows) in [
        (0, y, desc.height as usize),
        (1, uv, desc.height as usize / 2),
    ] {
        assert!(native.linesize[plane] >= stride as i32);
        assert!(!native.data[plane].is_null());
        for row in 0..rows {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    native.data[plane].add(row * native.linesize[plane] as usize),
                    target.as_mut_ptr().add(row * stride),
                    stride,
                );
            }
        }
    }
    VideoFrame::new_host(desc.clone(), frame.pts(), storage).unwrap()
}

fn cached_domain(codes: P010Codes, cache: &mut HashMap<u32, DomainCounts>) -> DomainCounts {
    let key = (u32::from(codes.y) << 20) | (u32::from(codes.cb) << 10) | u32::from(codes.cr);
    if let Some(result) = cache.get(&key) {
        return *result;
    }
    let mut result = DomainCounts::default();
    match decode_limited(codes) {
        Err(_) => result.invalid_code_triplets = 1,
        Ok(ycbcr) => {
            let rgb = ycbcr_to_pq(ycbcr);
            for component in [rgb.r, rgb.g, rgb.b] {
                if !component.is_finite() {
                    result.non_finite_components += 1;
                } else {
                    if component < 0.0 {
                        result.negative_components += 1;
                    }
                    if component > 1.0 {
                        result.out_of_range_rgb_components += 1;
                    }
                    if component > pq_inverse_eotf(1000.0).unwrap() {
                        result.above_1000_components += 1;
                    }
                    if let Ok(nits) = pq_eotf_nits(component) {
                        result.max_nits = result.max_nits.max(nits);
                        result.min_nits =
                            Some(result.min_nits.map_or(nits, |current| current.min(nits)));
                    }
                }
            }
        }
    }
    if cache.len() < CACHE_LIMIT {
        cache.insert(key, result);
    }
    result
}

fn unpack_p010_words(bytes: &[u8]) -> (Vec<u16>, u64) {
    assert_eq!(bytes.len() % 2, 0);
    let mut malformed = 0;
    let codes = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|value| {
            let word = u16::from_le_bytes([value[0], value[1]]);
            // Match the actual map shader's packing contract before discarding bits.
            malformed += u64::from(word & 63 != 0);
            word >> 6
        })
        .collect();
    (codes, malformed)
}

#[test]
fn low_six_packing_bits_are_checked_before_unpacking() {
    let words = [64u16 << 6, 65u16 << 6, (66u16 << 6) | 1, (67u16 << 6) | 63];
    let bytes: Vec<_> = words.into_iter().flat_map(u16::to_le_bytes).collect();
    let (codes, malformed) = unpack_p010_words(&bytes);
    assert_eq!(codes, [64, 65, 66, 67]);
    assert_eq!(malformed, 2);
}

fn audit_frame(
    frame: &VideoFrame,
    report: &mut FixtureReport,
    cache: &mut HashMap<u32, DomainCounts>,
) {
    let desc = frame.desc();
    let (y, uv) = frame.host().planes(desc);
    let (y_codes, invalid_y) = unpack_p010_words(y);
    let (uv_codes, invalid_uv) = unpack_p010_words(uv);
    report.invalid_p010_packing_samples += invalid_y + invalid_uv;
    assert_eq!(y_codes.len(), (WIDTH * HEIGHT) as usize);
    assert_eq!(uv_codes.len(), (WIDTH * HEIGHT / 2) as usize);
    for code in y_codes.iter().copied() {
        report.low_two_bits[0].add(code);
    }
    for pair in uv_codes.as_chunks::<2>().0 {
        report.low_two_bits[1].add(pair[0]);
        report.low_two_bits[2].add(pair[1]);
    }

    for (index, y) in y_codes.into_iter().enumerate() {
        let chroma =
            (index / WIDTH as usize / 2) * WIDTH as usize + (index % WIDTH as usize / 2) * 2;
        let codes = P010Codes {
            y,
            cb: uv_codes[chroma],
            cr: uv_codes[chroma + 1],
        };
        report.domain.merge(cached_domain(codes, cache));
    }
    report.frames += 1;
}

fn input_identity(path: &Path, filename: &str) -> String {
    let manifest = include_str!("../../../tests/fixtures/codecs/SHA256SUMS");
    let expected = manifest
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let hash = fields.next()?;
            (fields.next()? == format!("./{filename}")).then_some(hash)
        })
        .expect("canonical fixture checksum missing");
    let result = std::process::Command::new("sha256sum")
        .arg(path)
        .output()
        .expect("sha256sum required by fixture tooling");
    assert!(result.status.success(), "fixture identity read failed");
    let text = String::from_utf8(result.stdout).unwrap();
    let actual = text.split_whitespace().next().unwrap();
    assert_eq!(actual, expected, "canonical input bytes changed");
    actual.into()
}

fn audit_fixture(
    path: &Path,
    filename: &str,
    codec: VideoCodec,
    mode: DecodeMode,
) -> FixtureReport {
    let input_sha256 = input_identity(path, filename);
    let (time_base_num, time_base_den) = stream_time_base(path);
    let mut decoder = Decoder::open_pq_qualification(path, mode, VaapiOptions::default()).unwrap();
    let mut report = FixtureReport {
        fixture: format!("{filename} ({mode:?})"),
        input_sha256,
        stream_time_base: Some((time_base_num, time_base_den)),
        ..FixtureReport::default()
    };
    let requirements = &decoder.info().requirements;
    if requirements.codec != codec {
        report
            .violations
            .push(format!("codec {:?} != {codec:?}", requirements.codec));
    }
    if requirements.bit_depth != Some(10)
        || requirements.chroma_subsampling != ChromaSubsampling::Yuv420
    {
        report.violations.push("input is not 10-bit 4:2:0".into());
    }
    if decoder.info().frame_desc.format != PixelFormat::P010Le
        || (
            decoder.info().frame_desc.width,
            decoder.info().frame_desc.height,
        ) != (WIDTH, HEIGHT)
    {
        report
            .violations
            .push("decoder descriptor is not 1920x1080 P010".into());
    }
    if decoder.info().frame_desc.color_space != ColorSpace::pq_bt2020()
        || decoder.info().frame_desc.color_space.transfer != TransferCharacteristic::Pq
    {
        report
            .violations
            .push("decoder descriptor is not limited BT.2020/PQ".into());
    }

    let mut cache = HashMap::new();
    let mut previous_pts = None;
    let mut step = None;
    loop {
        let next = match mode {
            DecodeMode::Software => decoder.next_frame().unwrap(),
            DecodeMode::Vaapi => decoder
                .next_vaapi_frame()
                .unwrap()
                .map(|frame| downloaded(&frame)),
        };
        let Some(frame) = next else { break };
        if frame.desc().format != PixelFormat::P010Le
            || (frame.desc().width, frame.desc().height) != (WIDTH, HEIGHT)
            || frame.desc().color_space != ColorSpace::pq_bt2020()
        {
            report
                .violations
                .push(format!("frame {} descriptor changed", report.frames));
        }
        let pts = frame.pts().unwrap_or_else(|| {
            report
                .violations
                .push(format!("frame {} has no PTS", report.frames));
            0
        });
        if let Some(previous) = previous_pts {
            let delta = pts - previous;
            if delta <= 0 {
                report
                    .violations
                    .push(format!("frame {} timestamp did not advance", report.frames));
            } else if let Some(expected) = step {
                if delta != expected {
                    report.violations.push(format!(
                        "frame {} CFR PTS step {delta} != {expected}",
                        report.frames
                    ));
                }
            } else {
                step = Some(delta);
                // Fixture recipe is 50 fps; compare in the actual demux stream timebase.
                if i128::from(delta) * i128::from(time_base_num) * 50 != i128::from(time_base_den) {
                    report.violations.push(format!(
                        "PTS step {delta} at {time_base_num}/{time_base_den} is not 1/50 second"
                    ));
                }
            }
        } else {
            report.pts_first = Some(pts);
            if pts != 0 {
                report
                    .violations
                    .push(format!("first PTS is {pts}, expected zero"));
            }
        }
        previous_pts = Some(pts);
        report.pts_last = Some(pts);
        audit_frame(&frame, &mut report, &mut cache);
    }
    report.pts_step = step;
    report.cached_triplets = cache.len();
    if report.frames != FRAME_COUNT {
        report.violations.push(format!(
            "decoded {} frames, expected {FRAME_COUNT}",
            report.frames
        ));
    }
    report
}

#[test]
#[ignore = "300-frame C3 software and VAAPI decode audit; requires generated legal-domain fixtures and a working VAAPI device"]
fn c3_legal_domain_sources_remain_unclamped_and_within_1000_nits() {
    let root =
        PathBuf::from(std::env::var_os("C3_LEGAL_FIXTURE_DIR").expect("set C3_LEGAL_FIXTURE_DIR"));
    let reports = FIXTURES
        .iter()
        .flat_map(|(filename, codec)| {
            let path = root.join(filename);
            [
                audit_fixture(&path, filename, codec.clone(), DecodeMode::Software),
                audit_fixture(&path, filename, codec.clone(), DecodeMode::Vaapi),
            ]
        })
        .collect::<Vec<_>>();

    if let Some(destination) = std::env::var_os("C3_LEGAL_REPORT") {
        let body = format!(
            "{{\"schema_version\":1,\"reports\":[{}]}}\n",
            reports
                .iter()
                .map(FixtureReport::json)
                .collect::<Vec<_>>()
                .join(",")
        );
        let mut report = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .expect("create C3_LEGAL_REPORT exclusively");
        report
            .write_all(body.as_bytes())
            .expect("write C3_LEGAL_REPORT");
    }

    for report in reports {
        assert_report(report);
    }
}

fn assert_report(report: FixtureReport) {
    assert_eq!(
        report.invalid_p010_packing_samples, 0,
        "{} malformed P010 packing",
        report.fixture
    );
    assert_eq!(
        report.domain.negative_components, 0,
        "{} negative RGB components",
        report.fixture
    );
    assert_eq!(
        report.domain.above_1000_components, 0,
        "{} RGB components above 1000 nits",
        report.fixture
    );
    assert_eq!(
        report.domain.non_finite_components, 0,
        "{} non-finite RGB components",
        report.fixture
    );
    assert_eq!(
        report.domain.out_of_range_rgb_components, 0,
        "{} RGB components outside PQ [0, 1]",
        report.fixture
    );
    assert_eq!(
        report.domain.invalid_code_triplets, 0,
        "{} invalid limited-range code triplets",
        report.fixture
    );
    assert!(
        report.violations.is_empty(),
        "{} violations: {:?}",
        report.fixture,
        report.violations
    );
    println!(
        "{}: frames={} max_nits={:.6} cached_triplets={} low2(Y,U,V)={:?}/{:?}/{:?}",
        report.fixture,
        report.frames,
        report.domain.max_nits,
        report.cached_triplets,
        report.low_two_bits[0].0,
        report.low_two_bits[1].0,
        report.low_two_bits[2].0
    );
}
