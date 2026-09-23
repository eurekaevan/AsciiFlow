use asciiflow_core::{AsciiBackend, AsciiConfig, FrameSink, FrameSource, PixelFormat, VideoCodec};
#[cfg(feature = "av1-encode-diagnostic")]
use asciiflow_core::{ColorSpace, FrameDesc, HostFrame, Rational, VideoFrame};
use asciiflow_cpu::CpuAsciiBackend;
use asciiflow_interop::{
    DrmPrimeMapping, VaapiVulkanFullInteropProcessor, VaapiVulkanInteropProcessor, fourcc_name,
};
use asciiflow_media::{DecodeMode, Decoder, EncodeMode, Encoder, OutputEncoding, VaapiOptions};
#[cfg(feature = "av1-encode-diagnostic")]
use asciiflow_media::{probe_vaapi_av1_encoder_diagnostic, probe_vaapi_encoder_for};
use asciiflow_vulkan::VulkanAsciiBackend;
use std::{
    collections::VecDeque,
    path::PathBuf,
    time::{Duration, Instant},
};

fn input(name: &str, fallback: &str) -> PathBuf {
    std::env::var_os(name).map_or_else(|| PathBuf::from(fallback), PathBuf::from)
}

fn config() -> AsciiConfig {
    AsciiConfig {
        grid_width: 80,
        grid_height: None,
        charset: "@%#*+=-:. ".into(),
        font: "builtin-8x8".into(),
        color: true,
    }
}

#[test]
#[cfg(feature = "av1-encode-diagnostic")]
#[ignore = "diagnostic only: requires Intel VAAPI AV1 encode; does not enable AV1 output"]
fn av1_encoder_surface_descriptor_diagnostic() {
    let desc = FrameDesc::host_nv12(1920, 1080, ColorSpace::default()).unwrap();
    let fps = Rational::new(50, 1).unwrap();
    let mut vulkan = VulkanAsciiBackend::new().unwrap();
    assert_eq!(vulkan.device_info().vendor_id, 0x8086);
    for codec in [VideoCodec::H264, VideoCodec::Hevc, VideoCodec::Av1] {
        let probe = if codec == VideoCodec::Av1 {
            probe_vaapi_av1_encoder_diagnostic(desc.clone(), fps, VaapiOptions::default())
        } else {
            probe_vaapi_encoder_for(codec.clone(), desc.clone(), fps, VaapiOptions::default())
        }
        .unwrap();
        let mapping = DrmPrimeMapping::map_direct_write(probe.frames.acquire(0).unwrap()).unwrap();
        let drm = mapping.descriptor();
        println!("{codec} diagnostic encoder surface: {drm:#?}");
        assert_eq!(drm.width, 1920);
        assert_eq!(drm.height, 1080);
        assert_eq!(drm.layers.len(), 2);
        assert_eq!(fourcc_name(drm.layers[0].format), "R8..");
        assert_eq!(fourcc_name(drm.layers[1].format), "GR88");
        if codec == VideoCodec::Av1 {
            let input =
                VideoFrame::new_host(desc.clone(), Some(0), HostFrame::new_zeroed(&desc)).unwrap();
            let timings = vulkan
                .process_nv12_to_external(
                    input,
                    &config(),
                    mapping.duplicate_external_planes().unwrap(),
                )
                .unwrap();
            println!(
                "AV1 diagnostic Vulkan output copy: {:.3} ms",
                timings.gpu_external_output_copy.as_secs_f64() * 1000.0
            );
            assert_eq!(vulkan.validation_error_count(), 0);
        }
    }
}

#[test]
#[ignore = "requires Intel VAAPI/ANV; records actual new-codec descriptors and compares 36 frames"]
fn hevc_av1_descriptor_and_pre_ascii_parity() {
    for name in ["hevc-main8-bframes.mp4", "av1-main8-nofilmgrain.mp4"] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/codecs")
            .join(name);
        let mut reference = vaapi_decoder(&path);
        let mut direct = vaapi_decoder(&path);
        let mut vulkan = VulkanAsciiBackend::new().unwrap();
        assert_eq!(vulkan.device_info().vendor_id, 0x8086);
        for index in 0..36 {
            let expected = reference.next_frame().unwrap().unwrap();
            let mapping =
                DrmPrimeMapping::map_direct_read(direct.next_vaapi_frame().unwrap().unwrap())
                    .unwrap();
            if index == 0 {
                println!("{name}: {:#?}", mapping.descriptor());
            }
            let actual = vulkan
                .read_external_nv12(
                    expected.desc(),
                    mapping.pts(),
                    &config(),
                    mapping.duplicate_external_planes().unwrap(),
                )
                .unwrap();
            assert_eq!(actual, expected, "{name} frame {index}");
        }
        assert!(reference.next_frame().unwrap().is_none());
        assert!(direct.next_vaapi_frame().unwrap().is_none());
        assert_eq!(vulkan.validation_error_count(), 0);
        compare_full_ascii(36, "ASCIIFLOW_STAGE5_INPUT", path.to_str().unwrap(), false);
    }
}

#[test]
#[ignore = "requires Intel iHD Main10/AV1 10-bit decode; captures real P010 DRM descriptors"]
fn ten_bit_vaapi_descriptors_and_hwdownload_reference() {
    let capabilities = VaapiOptions::default().probe_decode_10bit_profiles();
    let mut vulkan = VulkanAsciiBackend::new().unwrap();
    assert_eq!(vulkan.device_info().vendor_id, 0x8086);
    for (index, name) in [
        "hevc-main10-sdr-gradient.mp4",
        "av1-main10-sdr-gradient.mp4",
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            capabilities[index].is_supported(),
            "{name}: {:?}",
            capabilities[index]
        );
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/codecs")
            .join(name);
        let mut software = Decoder::open(&path).unwrap();
        let mut download = vaapi_decoder(&path);
        let mut direct = vaapi_decoder(&path);
        let mut cpu = CpuAsciiBackend::new();
        for frame_index in 0..30 {
            let reference = software.next_frame().unwrap().unwrap();
            let staged = download.next_frame().unwrap().unwrap();
            assert_eq!(reference.desc().format, PixelFormat::P010Le);
            assert_eq!(staged.desc().format, PixelFormat::P010Le);
            assert_eq!(reference.pts(), staged.pts(), "{name} frame {frame_index}");
            assert_eq!(
                reference.host().as_slice(),
                staged.host().as_slice(),
                "{name} frame {frame_index}"
            );
            let mapping =
                DrmPrimeMapping::map_direct_read(direct.next_vaapi_frame().unwrap().unwrap())
                    .unwrap();
            if frame_index == 0 {
                println!(
                    "{name} actual P010 DRM descriptor: {:#?}",
                    mapping.descriptor()
                );
            }
            assert_eq!(mapping.pts(), reference.pts());
            let imported = vulkan
                .read_external_p010(
                    reference.desc(),
                    mapping.pts(),
                    &config(),
                    mapping.duplicate_external_p010_planes().unwrap(),
                )
                .unwrap();
            assert_eq!(imported, reference, "{name} imported frame {frame_index}");

            let expected_ascii = cpu.process(reference.clone(), &config()).unwrap().frame;
            let actual_ascii = vulkan
                .process_external_p010(
                    reference.desc(),
                    mapping.pts(),
                    &config(),
                    mapping.duplicate_external_p010_planes().unwrap(),
                )
                .unwrap()
                .frame;
            assert_eq!(
                actual_ascii, expected_ascii,
                "{name} ASCII frame {frame_index}"
            );
            // `mapping` owns the decoded VAAPI surface through both synchronous
            // Vulkan copy fences, including the final compute/readback fence.
        }
        assert_eq!(vulkan.validation_error_count(), 0);
    }
}

#[test]
#[ignore = "requires 3000-frame Main10 and AV1 inputs, Intel iHD decode, and ANV Vulkan"]
fn ten_bit_p010_interop_reuse_is_exact_and_fd_bounded() {
    for (name, variable, fallback) in [
        (
            "HEVC Main10",
            "ASCIIFLOW_STAGE52B_HEVC_STRESS_INPUT",
            "/tmp/asciiflow-stage52b-hevc-3000.mp4",
        ),
        (
            "AV1 10-bit",
            "ASCIIFLOW_STAGE52B_AV1_STRESS_INPUT",
            "/tmp/asciiflow-stage52b-av1-3000.mp4",
        ),
    ] {
        let path = input(variable, fallback);
        let before = fd_count();
        let active_baseline;
        let mut peak;
        {
            let mut software = Decoder::open(&path).unwrap();
            let mut vaapi = vaapi_decoder(&path);
            let mut cpu = CpuAsciiBackend::new();
            let mut vulkan = VulkanAsciiBackend::new().unwrap();
            assert_eq!(vulkan.device_info().vendor_id, 0x8086);
            active_baseline = fd_count();
            peak = active_baseline;
            for index in 0..3000 {
                let reference = software.next_frame().unwrap().unwrap_or_else(|| {
                    panic!("{name} software input ended at {index}; expected 3000 frames")
                });
                let hardware = vaapi.next_vaapi_frame().unwrap().unwrap_or_else(|| {
                    panic!("{name} VAAPI input ended at {index}; expected 3000 frames")
                });
                let mapping = DrmPrimeMapping::map_direct_read(hardware).unwrap();
                assert_eq!(mapping.pts(), reference.pts(), "{name} frame {index}");
                let expected = cpu.process(reference.clone(), &config()).unwrap().frame;
                let actual = vulkan
                    .process_external_p010(
                        reference.desc(),
                        mapping.pts(),
                        &config(),
                        mapping.duplicate_external_p010_planes().unwrap(),
                    )
                    .unwrap()
                    .frame;
                assert_eq!(actual, expected, "{name} frame {index}");
                let current = fd_count();
                peak = peak.max(current);
                assert!(
                    current <= active_baseline + 12,
                    "{name} FD count grew from {active_baseline} to {current} at frame {index}"
                );
            }
            assert_eq!(vulkan.validation_error_count(), 0, "{name}");
        }
        let after = fd_count();
        println!(
            "{name} FD count: before={before} active_baseline={active_baseline} per-frame_peak={peak} after={after}"
        );
        assert!(
            after <= before + 4,
            "{name} FD count did not return near baseline"
        );
    }
}

#[derive(Clone, Copy, Debug)]
enum P010BenchmarkPath {
    Software,
    VaapiDownload,
    VaapiInterop,
}

#[derive(Default)]
struct P010BenchmarkSample {
    wall: Duration,
    cpu: Duration,
    decode_call: Duration,
    decode_core: Duration,
    hardware_download: Duration,
    drm_map: Duration,
    import_setup: Duration,
    gpu_copy: Duration,
    gpu_map: Duration,
    gpu_render: Duration,
    backend_wall: Duration,
    latency: Duration,
}

impl P010BenchmarkSample {
    fn fps(&self) -> f64 {
        300.0 / self.wall.as_secs_f64()
    }

    fn report(&self, codec: &str, path: P010BenchmarkPath, run: &str) {
        let ms = |value: Duration| value.as_secs_f64() * 1_000.0 / 300.0;
        println!(
            "{codec} {path:?} {run}: FPS={:.2} CPU={:.1}% decode_call={:.3} decode_core={:.3} download={:.3} drm_map={:.3} import_setup={:.3} gpu_copy={:.3} gpu_map={:.3} gpu_render={:.3} backend_wall={:.3} latency={:.3} ms/frame",
            self.fps(),
            self.cpu.as_secs_f64() / self.wall.as_secs_f64() * 100.0,
            ms(self.decode_call),
            ms(self.decode_core),
            ms(self.hardware_download),
            ms(self.drm_map),
            ms(self.import_setup),
            ms(self.gpu_copy),
            ms(self.gpu_map),
            ms(self.gpu_render),
            ms(self.backend_wall),
            ms(self.latency),
        );
    }
}

fn process_cpu_time() -> Duration {
    let mut clock = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut clock) },
        0
    );
    Duration::new(clock.tv_sec as u64, clock.tv_nsec as u32)
}

fn benchmark_p010_path(path: &PathBuf, mode: P010BenchmarkPath) -> P010BenchmarkSample {
    let decode_mode = if matches!(mode, P010BenchmarkPath::Software) {
        DecodeMode::Software
    } else {
        DecodeMode::Vaapi
    };
    let mut decoder = Decoder::open_with(path, decode_mode, VaapiOptions::default()).unwrap();
    let mut vulkan = VulkanAsciiBackend::new().unwrap();
    assert_eq!(vulkan.device_info().vendor_id, 0x8086);
    let mut sample = P010BenchmarkSample::default();
    let mut wall_start = Instant::now();
    let mut cpu_start = Duration::ZERO;
    for index in 0..336 {
        if index == 36 {
            wall_start = Instant::now();
            cpu_start = process_cpu_time();
        }
        let frame_start = Instant::now();
        let decode_start = Instant::now();
        let (output, drm_map) = match mode {
            P010BenchmarkPath::Software | P010BenchmarkPath::VaapiDownload => {
                let frame = decoder.next_frame().unwrap().unwrap();
                let decode_call = decode_start.elapsed();
                let output = vulkan.process(frame, &config()).unwrap();
                if index >= 36 {
                    sample.decode_call += decode_call;
                }
                (output, Duration::ZERO)
            }
            P010BenchmarkPath::VaapiInterop => {
                let frame = decoder.next_vaapi_frame().unwrap().unwrap();
                let decode_call = decode_start.elapsed();
                let mapping = DrmPrimeMapping::map_direct_read(frame).unwrap();
                let output = vulkan
                    .process_external_p010(
                        &decoder.info().frame_desc,
                        mapping.pts(),
                        &config(),
                        mapping.duplicate_external_p010_planes().unwrap(),
                    )
                    .unwrap();
                if index >= 36 {
                    sample.decode_call += decode_call;
                }
                (output, mapping.map_wall())
            }
        };
        let source = decoder.take_timings();
        if index >= 36 {
            sample.decode_core += source.packet_submit + source.frame_receive;
            sample.hardware_download += source.hardware_download;
            sample.drm_map += drm_map;
            let timings = output.timings;
            sample.import_setup += timings.external_capability_query
                + timings.external_image_create
                + timings.external_memory_import
                + timings.external_memory_bind
                + timings.external_ownership
                + timings.external_image_destroy;
            sample.gpu_copy += timings.gpu_external_copy;
            sample.gpu_map += timings.gpu_mapping;
            sample.gpu_render += timings.gpu_render;
            sample.backend_wall += timings.backend_wall;
            sample.latency += frame_start.elapsed();
        }
    }
    sample.wall = wall_start.elapsed();
    sample.cpu = process_cpu_time() - cpu_start;
    assert_eq!(vulkan.validation_error_count(), 0);
    sample
}

#[test]
#[ignore = "Intel Arc 300-frame P010 decode-path benchmark; run in Release without Vulkan Validation"]
fn ten_bit_p010_decode_paths_300_frame_benchmark() {
    for (codec, variable, fallback) in [
        (
            "HEVC Main10",
            "ASCIIFLOW_STAGE52B_HEVC_STRESS_INPUT",
            "/tmp/asciiflow-stage52b-hevc-3000.mp4",
        ),
        (
            "AV1 10-bit",
            "ASCIIFLOW_STAGE52B_AV1_STRESS_INPUT",
            "/tmp/asciiflow-stage52b-av1-3000.mp4",
        ),
    ] {
        let path = input(variable, fallback);
        for mode in [
            P010BenchmarkPath::Software,
            P010BenchmarkPath::VaapiDownload,
            P010BenchmarkPath::VaapiInterop,
        ] {
            let mut runs = (1..=3)
                .map(|run| {
                    let sample = benchmark_p010_path(&path, mode);
                    sample.report(codec, mode, &format!("run {run}"));
                    sample
                })
                .collect::<Vec<_>>();
            runs.sort_by(|left, right| left.fps().total_cmp(&right.fps()));
            runs[1].report(codec, mode, "median FPS run");
        }
    }
}

fn vaapi_decoder(path: &PathBuf) -> Decoder {
    Decoder::open_with(path, DecodeMode::Vaapi, VaapiOptions::default()).unwrap()
}

fn fd_count() -> usize {
    std::fs::read_dir("/proc/self/fd").unwrap().count()
}

fn temporary_output(label: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/asciiflow-{label}-{}.mp4", std::process::id()))
}

#[test]
#[ignore = "requires Intel iHD VAAPI and an ANV Vulkan device"]
fn descriptor_and_pre_ascii_nv12_are_exact() {
    let path = input("ASCIIFLOW_STAGE3_INPUT", "input.mp4");
    let mut reference = vaapi_decoder(&path);
    let mut direct = vaapi_decoder(&path);
    let desc = direct.info().frame_desc.clone();
    let mut vulkan = VulkanAsciiBackend::new().unwrap();
    assert!(
        vulkan
            .device_info()
            .name
            .contains("Intel(R) Arc(tm) Graphics (MTL)")
    );
    for index in 0..30 {
        let expected = reference.next_frame().unwrap().unwrap();
        let source = direct.next_vaapi_frame().unwrap().unwrap();
        let mapping = DrmPrimeMapping::map_direct_read(source).unwrap();
        if index == 0 {
            let drm = mapping.descriptor();
            println!("descriptor={drm:#?}");
            assert_eq!(drm.objects.len(), 1);
            assert_eq!(drm.layers.len(), 2);
            assert_eq!(fourcc_name(drm.layers[0].format), "R8..");
            assert_eq!(fourcc_name(drm.layers[1].format), "GR88");
        }
        let actual = vulkan
            .read_external_nv12(
                &desc,
                mapping.pts(),
                &config(),
                mapping.duplicate_external_planes().unwrap(),
            )
            .unwrap();
        assert_eq!(
            actual.pts(),
            expected.pts(),
            "PTS mismatch at frame {index}"
        );
        assert_eq!(
            actual.desc(),
            expected.desc(),
            "metadata mismatch at frame {index}"
        );
        assert_eq!(
            actual.host().as_slice(),
            expected.host().as_slice(),
            "NV12 mismatch at frame {index}"
        );
    }
    assert_eq!(vulkan.validation_error_count(), 0);
}

#[test]
#[ignore = "requires Intel iHD VAAPI and an ANV Vulkan device"]
fn full_ascii_is_exact_with_two_bounded_slots() {
    compare_full_ascii(30, "ASCIIFLOW_STAGE3_INPUT", "input.mp4", false);
}

#[test]
#[ignore = "requires a 3000+ frame H.264 input, Intel iHD VAAPI, and ANV Vulkan"]
fn surface_reuse_stress_is_exact_and_fd_bounded() {
    compare_full_ascii(
        3000,
        "ASCIIFLOW_STAGE3_STRESS_INPUT",
        "/tmp/asciiflow-stage3-stress-input.mp4",
        true,
    );
}

#[test]
#[ignore = "requires Intel iHD VAAPI and an ANV Vulkan device"]
fn import_failure_and_early_processor_drop_do_not_leak_fds() {
    let path = input("ASCIIFLOW_STAGE3_INPUT", "input.mp4");
    let mut decoder = vaapi_decoder(&path);
    let desc = decoder.info().frame_desc.clone();
    let mapping =
        DrmPrimeMapping::map_direct_read(decoder.next_vaapi_frame().unwrap().unwrap()).unwrap();
    let mut invalid = mapping.duplicate_external_planes().unwrap();
    invalid[0].fd = std::fs::File::open("/dev/null").unwrap().into();
    let mut backend = VulkanAsciiBackend::new().unwrap();
    let stable = fd_count() - 2;
    let error = backend
        .read_external_nv12(&desc, mapping.pts(), &config(), invalid)
        .unwrap_err();
    assert!(error.to_string().contains("DMA-BUF"));
    assert_eq!(fd_count(), stable, "failed import leaked a duplicated fd");
    drop(mapping);
    drop(backend);
    drop(decoder);

    let before_cancel = fd_count();
    {
        let mut decoder = vaapi_decoder(&path);
        let desc = decoder.info().frame_desc.clone();
        let backend = VulkanAsciiBackend::new().unwrap();
        let mut processor = VaapiVulkanInteropProcessor::new(backend, desc, config()).unwrap();
        processor
            .submit(decoder.next_vaapi_frame().unwrap().unwrap())
            .unwrap();
        processor
            .submit(decoder.next_vaapi_frame().unwrap().unwrap())
            .unwrap();
        // Dropping without drain models pipeline cancellation. Slot teardown
        // joins in-flight work before mapped/source frames are released.
    }
    let after_cancel = fd_count();
    assert!(
        after_cancel <= before_cancel + 4,
        "early processor drop leaked fds: before={before_cancel}, after={after_cancel}"
    );
}

#[test]
#[ignore = "requires Intel iHD VAAPI and an ANV Vulkan device; run without validation because it intentionally imports an invalid fd"]
fn output_import_failure_and_full_processor_drop_do_not_leak_fds() {
    let path = input("ASCIIFLOW_STAGE3_INPUT", "input.mp4");
    let output_path = temporary_output("stage3b-failure");
    let _ = std::fs::remove_file(&output_path);
    let mut decoder = vaapi_decoder(&path);
    let desc = decoder.info().frame_desc.clone();
    let mut encoder = Encoder::create_with(
        &output_path,
        desc.clone(),
        decoder.info().frame_rate,
        EncodeMode::Vaapi,
        VaapiOptions::default(),
    )
    .unwrap();
    let frames = encoder.encoder_frames().unwrap();
    let input_mapping =
        DrmPrimeMapping::map_direct_read(decoder.next_vaapi_frame().unwrap().unwrap()).unwrap();
    let output_mapping = DrmPrimeMapping::map_direct_write(frames.acquire(0).unwrap()).unwrap();
    let input_planes = input_mapping.duplicate_external_planes().unwrap();
    let mut output_planes = output_mapping.duplicate_external_planes().unwrap();
    output_planes[0].fd = std::fs::File::open("/dev/null").unwrap().into();
    let mut backend = VulkanAsciiBackend::new().unwrap();
    let stable = fd_count() - 4;
    let error = backend
        .process_external_nv12_to_external(&desc, &config(), input_planes, output_planes)
        .unwrap_err();
    assert!(error.to_string().contains("DMA-BUF"));
    assert_eq!(
        fd_count(),
        stable,
        "failed output import leaked duplicated fds"
    );
    drop(output_mapping);
    drop(input_mapping);
    drop(backend);
    drop(decoder);

    let before_cancel = fd_count();
    {
        let mut decoder = vaapi_decoder(&path);
        let backend = VulkanAsciiBackend::new().unwrap();
        let mut processor =
            VaapiVulkanFullInteropProcessor::new(backend, frames, desc, config()).unwrap();
        processor
            .submit(decoder.next_vaapi_frame().unwrap().unwrap())
            .unwrap();
        processor
            .submit(decoder.next_vaapi_frame().unwrap().unwrap())
            .unwrap();
    }
    let after_cancel = fd_count();
    assert!(
        after_cancel <= before_cancel + 4,
        "early full-processor drop leaked fds: before={before_cancel}, after={after_cancel}"
    );
    encoder.finish().unwrap();
    drop(encoder);
    std::fs::remove_file(output_path).unwrap();
}

#[test]
#[ignore = "requires Intel iHD VAAPI and an ANV Vulkan device"]
fn encoder_descriptor_and_pre_encode_pixels_are_exact() {
    compare_output_interop(
        30,
        "ASCIIFLOW_STAGE3_INPUT",
        "input.mp4",
        false,
        VideoCodec::H264,
    );
}

#[test]
#[ignore = "requires Intel iHD HEVC encode and an ANV Vulkan device"]
fn hevc_encoder_descriptor_and_staged_interop_pixels_are_exact() {
    compare_output_interop(
        30,
        "ASCIIFLOW_STAGE51_INPUT",
        "input.mp4",
        false,
        VideoCodec::Hevc,
    );
}

#[test]
#[ignore = "requires Intel iHD AV1 encode and an ANV Vulkan device"]
fn av1_encoder_descriptor_and_staged_interop_pixels_are_exact() {
    compare_output_interop(
        30,
        "ASCIIFLOW_STAGE51B_INPUT",
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/stage51a1-evidence/h264-testsrc2-300.mp4"
        ),
        false,
        VideoCodec::Av1,
    );
}

#[test]
#[ignore = "requires Intel iHD VAAPI"]
fn encoder_rejects_surface_from_a_different_frames_context() {
    let path = input("ASCIIFLOW_STAGE3_INPUT", "input.mp4");
    let decoder = vaapi_decoder(&path);
    let desc = decoder.info().frame_desc.clone();
    let frame_rate = decoder.info().frame_rate;
    let first_path = temporary_output("stage3b-pool-a");
    let second_path = temporary_output("stage3b-pool-b");
    let _ = std::fs::remove_file(&first_path);
    let _ = std::fs::remove_file(&second_path);
    let mut first = Encoder::create_with(
        &first_path,
        desc.clone(),
        frame_rate,
        EncodeMode::Vaapi,
        VaapiOptions::default(),
    )
    .unwrap();
    let mut second = Encoder::create_with(
        &second_path,
        desc,
        frame_rate,
        EncodeMode::Vaapi,
        VaapiOptions::default(),
    )
    .unwrap();

    let foreign = first.encoder_frames().unwrap().acquire(0).unwrap();
    let error = second.encode_hardware_frame(foreign).unwrap_err();
    assert!(error.to_string().contains("different AVHWFramesContext"));

    first.finish().unwrap();
    second.finish().unwrap();
    drop(first);
    drop(second);
    std::fs::remove_file(first_path).unwrap();
    std::fs::remove_file(second_path).unwrap();
}

#[test]
#[ignore = "requires Intel iHD H.264 and HEVC encode"]
fn h264_and_hevc_encoder_frames_are_not_cross_submittable() {
    let path = input("ASCIIFLOW_STAGE51_INPUT", "input.mp4");
    let decoder = vaapi_decoder(&path);
    let desc = decoder.info().frame_desc.clone();
    let frame_rate = decoder.info().frame_rate;
    let h264_path = temporary_output("stage51-h264-pool");
    let hevc_path = temporary_output("stage51-hevc-pool");
    let cancellation = Default::default();
    let mut h264 = Encoder::create_with_codec_and_audio(
        &h264_path,
        desc.clone(),
        frame_rate,
        OutputEncoding {
            codec: VideoCodec::H264,
            mode: EncodeMode::Vaapi,
        },
        VaapiOptions::default(),
        Vec::new(),
        cancellation,
    )
    .unwrap();
    let mut hevc = Encoder::create_with_codec_and_audio(
        &hevc_path,
        desc,
        frame_rate,
        OutputEncoding {
            codec: VideoCodec::Hevc,
            mode: EncodeMode::Vaapi,
        },
        VaapiOptions::default(),
        Vec::new(),
        Default::default(),
    )
    .unwrap();
    let h264_frame = h264.encoder_frames().unwrap().acquire(0).unwrap();
    let hevc_frame = hevc.encoder_frames().unwrap().acquire(0).unwrap();
    assert!(
        hevc.encode_hardware_frame(h264_frame)
            .unwrap_err()
            .to_string()
            .contains("different AVHWFramesContext")
    );
    assert!(
        h264.encode_hardware_frame(hevc_frame)
            .unwrap_err()
            .to_string()
            .contains("different AVHWFramesContext")
    );
    h264.finish().unwrap();
    hevc.finish().unwrap();
    drop(h264);
    drop(hevc);
    std::fs::remove_file(h264_path).unwrap();
    std::fs::remove_file(hevc_path).unwrap();
}

#[test]
#[ignore = "requires Intel iHD H.264, HEVC, and AV1 encode"]
fn av1_encoder_frames_are_not_cross_submittable() {
    let path = input(
        "ASCIIFLOW_STAGE51B_INPUT",
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/stage51a1-evidence/h264-testsrc2-300.mp4"
        ),
    );
    let decoder = vaapi_decoder(&path);
    let desc = decoder.info().frame_desc.clone();
    let frame_rate = decoder.info().frame_rate;
    let av1_path = temporary_output("stage51b-av1-pool");
    let h264_path = temporary_output("stage51b-h264-pool");
    let mut av1 = Encoder::create_with_codec_and_audio(
        &av1_path,
        desc.clone(),
        frame_rate,
        OutputEncoding {
            codec: VideoCodec::Av1,
            mode: EncodeMode::Vaapi,
        },
        VaapiOptions::default(),
        Vec::new(),
        Default::default(),
    )
    .unwrap();
    let mut h264 = Encoder::create_with_codec_and_audio(
        &h264_path,
        desc,
        frame_rate,
        OutputEncoding {
            codec: VideoCodec::H264,
            mode: EncodeMode::Vaapi,
        },
        VaapiOptions::default(),
        Vec::new(),
        Default::default(),
    )
    .unwrap();
    let av1_frame = av1.encoder_frames().unwrap().acquire(0).unwrap();
    let h264_frame = h264.encoder_frames().unwrap().acquire(0).unwrap();
    assert!(
        h264.encode_hardware_frame(av1_frame)
            .unwrap_err()
            .to_string()
            .contains("different AVHWFramesContext")
    );
    assert!(
        av1.encode_hardware_frame(h264_frame)
            .unwrap_err()
            .to_string()
            .contains("different AVHWFramesContext")
    );
    av1.finish().unwrap();
    h264.finish().unwrap();
    drop(av1);
    drop(h264);
    std::fs::remove_file(av1_path).unwrap();
    std::fs::remove_file(h264_path).unwrap();
}

#[test]
#[ignore = "requires a 3000+ frame H.264 input, Intel iHD VAAPI, and ANV Vulkan"]
fn full_interop_surface_reuse_is_exact_and_fd_bounded() {
    compare_output_interop(
        3000,
        "ASCIIFLOW_STAGE3_STRESS_INPUT",
        "/tmp/asciiflow-stage3-stress-input.mp4",
        true,
        VideoCodec::H264,
    );
}

#[test]
#[ignore = "requires a 3000+ frame input, Intel HEVC encode, and ANV Vulkan"]
fn hevc_full_interop_surface_reuse_is_exact_and_fd_bounded() {
    compare_output_interop(
        3000,
        "ASCIIFLOW_STAGE51_STRESS_INPUT",
        "/tmp/asciiflow-stage5-qualification/h264-testsrc2-1920x1080-50fps-300f-bt709-limited-8bit-420.mp4",
        true,
        VideoCodec::Hevc,
    );
}

#[test]
#[ignore = "requires a 3000-frame input, Intel AV1 encode, and ANV Vulkan"]
fn av1_full_interop_surface_reuse_is_exact_and_fd_bounded() {
    compare_output_interop(
        3000,
        "ASCIIFLOW_STAGE51B_STRESS_INPUT",
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/stage51a1-evidence/h264-testsrc2-3000-loop.mp4"
        ),
        true,
        VideoCodec::Av1,
    );
}

fn compare_full_ascii(frames: usize, env_name: &str, fallback: &str, check_fds: bool) {
    let path = input(env_name, fallback);
    let before = fd_count();
    let active_baseline_report;
    let mut max_seen = before;
    {
        let mut reference = vaapi_decoder(&path);
        let mut direct = vaapi_decoder(&path);
        let desc = direct.info().frame_desc.clone();
        let cfg = config();
        let mut reference_vulkan = VulkanAsciiBackend::new().unwrap();
        let interop_vulkan = VulkanAsciiBackend::new().unwrap();
        let mut interop =
            VaapiVulkanInteropProcessor::new(interop_vulkan, desc, cfg.clone()).unwrap();
        let active_baseline = fd_count();
        active_baseline_report = active_baseline;
        max_seen = max_seen.max(active_baseline);
        let mut expected = VecDeque::with_capacity(3);
        let mut compared = 0;
        for index in 0..frames {
            let host = reference
                .next_frame()
                .unwrap()
                .unwrap_or_else(|| panic!("reference ended at frame {index}; expected {frames}"));
            expected.push_back(reference_vulkan.process(host, &cfg).unwrap().frame);
            let hardware = direct.next_vaapi_frame().unwrap().unwrap_or_else(|| {
                panic!("interop source ended at frame {index}; expected {frames}")
            });
            if let Some(actual) = interop.submit(hardware).unwrap() {
                let expected = expected.pop_front().unwrap();
                assert_eq!(
                    actual.frame, expected,
                    "ASCII mismatch at output {compared}"
                );
                compared += 1;
            }
            if check_fds && index % 250 == 249 {
                let current = fd_count();
                max_seen = max_seen.max(current);
                assert!(
                    current <= active_baseline + 8,
                    "fd count grew from active baseline {active_baseline} to {current} at frame {index}"
                );
            }
        }
        while let Some(actual) = interop.drain().unwrap() {
            let expected = expected.pop_front().unwrap();
            assert_eq!(
                actual.frame, expected,
                "ASCII mismatch at output {compared}"
            );
            compared += 1;
        }
        assert_eq!(compared, frames);
        assert!(expected.is_empty());
        assert_eq!(interop.validation_error_count(), 0);
    }
    if check_fds {
        let after = fd_count();
        println!(
            "fd_count before={before} active_baseline={active_baseline_report} max_steady={max_seen} after={after}"
        );
        assert!(
            after <= before + 4,
            "fd count did not return near baseline: before={before}, after={after}"
        );
    }
}

fn compare_output_interop(
    frames: usize,
    env_name: &str,
    fallback: &str,
    check_fds: bool,
    output_codec: VideoCodec,
) {
    let path = input(env_name, fallback);
    let output_path = temporary_output(if check_fds {
        "stage3b-stress"
    } else {
        "stage3b-parity"
    });
    let _ = std::fs::remove_file(&output_path);
    let before = fd_count();
    let expected_output_codec = output_codec.clone();
    let active_baseline_report;
    let mut max_seen = before;
    {
        let mut reference_decoder = vaapi_decoder(&path);
        let mut direct_decoder = vaapi_decoder(&path);
        let desc = direct_decoder.info().frame_desc.clone();
        let frame_rate = direct_decoder.info().frame_rate;
        let cfg = config();
        let reference_backend = VulkanAsciiBackend::new().unwrap();
        let mut reference =
            VaapiVulkanInteropProcessor::new(reference_backend, desc.clone(), cfg.clone()).unwrap();
        let mut encoder = Encoder::create_with_codec_and_audio(
            &output_path,
            desc.clone(),
            frame_rate,
            OutputEncoding {
                codec: output_codec.clone(),
                mode: EncodeMode::Vaapi,
            },
            VaapiOptions::default(),
            Vec::new(),
            Default::default(),
        )
        .unwrap();
        let encoder_frames = encoder.encoder_frames().unwrap();
        let staged_frames = encoder_frames.clone_for_diagnostic().unwrap();
        let descriptor_probe = encoder_frames.acquire(0).unwrap();
        let descriptor_mapping = DrmPrimeMapping::map_direct_write(descriptor_probe).unwrap();
        let encoder_drm = descriptor_mapping.descriptor();
        println!("encoder_descriptor={encoder_drm:#?}");
        assert_eq!(encoder_drm.objects.len(), 1);
        assert_eq!(encoder_drm.layers.len(), 2);
        assert_eq!(fourcc_name(encoder_drm.layers[0].format), "R8..");
        assert_eq!(fourcc_name(encoder_drm.layers[1].format), "GR88");
        drop(descriptor_mapping);

        let full_backend = VulkanAsciiBackend::new().unwrap();
        let mut full =
            VaapiVulkanFullInteropProcessor::new(full_backend, encoder_frames, desc, cfg).unwrap();
        let active_baseline = fd_count();
        active_baseline_report = active_baseline;
        max_seen = max_seen.max(active_baseline);
        let mut expected = VecDeque::with_capacity(3);
        let mut compared = 0;

        for index in 0..frames {
            let reference_input = reference_decoder
                .next_vaapi_frame()
                .unwrap()
                .unwrap_or_else(|| panic!("reference ended at frame {index}; expected {frames}"));
            if let Some(output) = reference.submit(reference_input).unwrap() {
                expected.push_back(output.frame);
            }
            let direct_input = direct_decoder
                .next_vaapi_frame()
                .unwrap()
                .unwrap_or_else(|| {
                    panic!("interop source ended at frame {index}; expected {frames}")
                });
            if let Some(output) = full.submit(direct_input).unwrap() {
                let expected = expected.pop_front().unwrap();
                assert_eq!(output.frame.pts(), compared as i64);
                let actual = output.frame.download_nv12().unwrap();
                let mut staged = staged_frames.acquire(compared as i64).unwrap();
                staged.upload_nv12(&expected).unwrap();
                let staged = staged.download_nv12().unwrap();
                assert_eq!(
                    actual.desc(),
                    expected.desc(),
                    "metadata mismatch at {compared}"
                );
                assert_eq!(
                    actual.host().as_slice(),
                    expected.host().as_slice(),
                    "pre-encode NV12 mismatch at {compared}"
                );
                assert_eq!(staged, expected, "staged surface mismatch at {compared}");
                encoder.encode_hardware_frame(output.frame).unwrap();
                compared += 1;
            }
            if check_fds && index % 250 == 249 {
                let current = fd_count();
                max_seen = max_seen.max(current);
                assert!(
                    current <= active_baseline + 12,
                    "full interop fd count grew from {active_baseline} to {current} at frame {index}"
                );
            }
        }
        while let Some(output) = reference.drain().unwrap() {
            expected.push_back(output.frame);
        }
        while let Some(output) = full.drain().unwrap() {
            let expected = expected.pop_front().unwrap();
            assert_eq!(output.frame.pts(), compared as i64);
            let actual = output.frame.download_nv12().unwrap();
            let mut staged = staged_frames.acquire(compared as i64).unwrap();
            staged.upload_nv12(&expected).unwrap();
            let staged = staged.download_nv12().unwrap();
            assert_eq!(
                actual.desc(),
                expected.desc(),
                "metadata mismatch at {compared}"
            );
            assert_eq!(
                actual.host().as_slice(),
                expected.host().as_slice(),
                "pre-encode NV12 mismatch at {compared}"
            );
            assert_eq!(staged, expected, "staged surface mismatch at {compared}");
            encoder.encode_hardware_frame(output.frame).unwrap();
            compared += 1;
        }
        encoder.finish().unwrap();
        assert_eq!(compared, frames);
        assert!(expected.is_empty());
        assert_eq!(reference.validation_error_count(), 0);
        assert_eq!(full.validation_error_count(), 0);
    }
    let mut decoded = Decoder::open(&output_path).unwrap();
    assert_eq!(decoded.info().requirements.codec, expected_output_codec);
    let mut decoded_count = 0;
    let mut previous_pts = None;
    while let Some(frame) = decoded.next_frame().unwrap() {
        let pts = frame.pts().expect("encoded output frame must carry PTS");
        if let Some(previous) = previous_pts {
            assert!(pts > previous, "decoded output PTS must be monotonic");
        } else {
            assert_eq!(pts, 0);
        }
        previous_pts = Some(pts);
        decoded_count += 1;
    }
    assert_eq!(decoded_count, frames);
    drop(decoded);
    if check_fds {
        let after = fd_count();
        println!(
            "stage3b fd_count before={before} active_baseline={active_baseline_report} max_steady={max_seen} after={after}"
        );
        assert!(
            after <= before + 4,
            "full interop fd count did not return near baseline: before={before}, after={after}"
        );
    }
    std::fs::remove_file(output_path).unwrap();
}
