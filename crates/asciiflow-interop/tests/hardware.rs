use asciiflow_core::{AsciiBackend, AsciiConfig, FrameSink, FrameSource, PixelFormat, VideoCodec};
#[cfg(feature = "av1-encode-diagnostic")]
use asciiflow_core::{ColorSpace, FrameDesc, HostFrame, Rational, VideoFrame};
#[cfg(feature = "p010-output-diagnostic")]
use asciiflow_core::{ColorSpace, FrameDesc, HostFrame, VideoFrame};
use asciiflow_cpu::CpuAsciiBackend;
#[cfg(feature = "p010-output-diagnostic")]
use asciiflow_interop::VulkanVaapiOutputInteropProcessor;
use asciiflow_interop::{
    DrmPrimeMapping, VaapiVulkanFullInteropProcessor, VaapiVulkanInteropProcessor, fourcc_name,
};
#[cfg(feature = "p010-output-diagnostic")]
use asciiflow_media::VaapiDiagnosticP010Pool;
use asciiflow_media::{DecodeMode, Decoder, EncodeMode, Encoder, OutputEncoding, VaapiOptions};
#[cfg(feature = "av1-encode-diagnostic")]
use asciiflow_media::{probe_vaapi_av1_encoder_diagnostic, probe_vaapi_encoder_for};
#[cfg(feature = "p010-output-diagnostic")]
use asciiflow_vulkan::DiagnosticOutputFault;
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
#[cfg(feature = "p010-output-diagnostic")]
#[ignore = "requires Intel iHD P010 output-style pool; records actual writable DRM descriptor"]
fn p010_output_surface_descriptor_diagnostic() {
    let mut vulkan = VulkanAsciiBackend::new().unwrap();
    for (width, height) in [(64, 64), (1920, 1080)] {
        let pool = VaapiDiagnosticP010Pool::new(
            std::path::Path::new("/dev/dri/renderD128"),
            width,
            height,
        )
        .unwrap();
        let mapping = DrmPrimeMapping::map_direct_write(pool.acquire(0).unwrap()).unwrap();
        println!(
            "P010 output-style {width}x{height} DRM descriptor: {:#?}",
            mapping.descriptor()
        );
        assert_eq!(mapping.descriptor().layers.len(), 2);
        assert_eq!(fourcc_name(mapping.descriptor().layers[0].format), "R16.");
        assert_eq!(fourcc_name(mapping.descriptor().layers[1].format), "GR32");
        vulkan
            .probe_external_p010(
                &FrameDesc::host_p010_le(width, height, ColorSpace::default()).unwrap(),
                mapping.duplicate_external_p010_planes().unwrap(),
                asciiflow_vulkan::ExternalImageAccess::Write,
            )
            .unwrap();
    }
    assert_eq!(vulkan.validation_error_count(), 0);
}

#[test]
#[cfg(feature = "p010-output-diagnostic")]
#[ignore = "requires Intel iHD/ANV and writable P010 DMA-BUF"]
fn p010_packed_output_is_bit_exact() {
    let desc = FrameDesc::host_p010_le(64, 64, ColorSpace::default()).unwrap();
    let pool =
        VaapiDiagnosticP010Pool::new(std::path::Path::new("/dev/dri/renderD128"), 64, 64).unwrap();
    let mut vulkan = VulkanAsciiBackend::new().unwrap();
    for pattern in 0..4 {
        let mut bytes = vec![0_u8; desc.byte_len()];
        let mut random = 0x1234_5678_u32;
        for (index, word) in bytes.chunks_exact_mut(2).enumerate() {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            let sample = match pattern {
                0 => (index % 1024) as u16, // gradient
                1 => {
                    if (index / 8 + index / 64) % 2 == 0 {
                        1023
                    } else {
                        1
                    }
                }
                2 => (random % 1024) as u16,             // seeded random
                _ => ((index * 997 + 37) % 1024) as u16, // low active bits
            };
            word.copy_from_slice(&(sample << 6).to_le_bytes());
        }
        let source = VideoFrame::new_host(
            desc.clone(),
            Some(pattern),
            HostFrame::from_bytes(&desc, bytes).unwrap(),
        )
        .unwrap();
        let mapping = DrmPrimeMapping::map_direct_write(pool.acquire(pattern).unwrap()).unwrap();
        vulkan
            .copy_packed_to_external(
                &source,
                &config(),
                mapping.duplicate_external_p010_planes().unwrap(),
            )
            .unwrap();
        let output = mapping.into_source().download_p010().unwrap();
        assert_eq!(
            output.host().as_slice(),
            source.host().as_slice(),
            "pattern {pattern}"
        );
        for word in output.host().as_slice().chunks_exact(2) {
            assert_eq!(u16::from_le_bytes([word[0], word[1]]) & 0x003f, 0);
        }
    }
    assert_eq!(vulkan.validation_error_count(), 0);
}

#[cfg(feature = "p010-output-diagnostic")]
fn synthetic_p010(desc: &FrameDesc, pts: i64) -> VideoFrame {
    let mut bytes = vec![0_u8; desc.byte_len()];
    for (index, word) in bytes.chunks_exact_mut(2).enumerate() {
        let value = (((index * 997 + pts as usize * 73 + 37) % 1024) as u16) << 6;
        word.copy_from_slice(&value.to_le_bytes());
    }
    VideoFrame::new_host(
        desc.clone(),
        Some(pts),
        HostFrame::from_bytes(desc, bytes).unwrap(),
    )
    .unwrap()
}

#[test]
#[cfg(feature = "p010-output-diagnostic")]
#[ignore = "requires Intel iHD/ANV; compares 30 P010 processed frames and two-slot drain"]
fn p010_output_processing_matches_cpu() {
    let desc = FrameDesc::host_p010_le(128, 64, ColorSpace::default()).unwrap();
    let pool =
        VaapiDiagnosticP010Pool::new(std::path::Path::new("/dev/dri/renderD128"), 128, 64).unwrap();
    let mut cfg = config();
    cfg.grid_width = 16;
    let mut cpu = CpuAsciiBackend::new();
    let mut interop = VulkanVaapiOutputInteropProcessor::new(
        VulkanAsciiBackend::new().unwrap(),
        pool.output_frames().unwrap(),
        desc.clone(),
        cfg.clone(),
    )
    .unwrap();
    let mut expected = VecDeque::new();
    let mut compared = 0;
    for index in 0..30 {
        let source = synthetic_p010(&desc, index);
        expected.push_back(cpu.process(source.clone(), &cfg).unwrap().frame);
        if let Some(output) = interop.submit(source).unwrap() {
            assert_eq!(output.frame.pts(), compared);
            let actual = output.frame.download_p010().unwrap();
            assert_eq!(actual, expected.pop_front().unwrap(), "frame {compared}");
            compared += 1;
        }
    }
    while let Some(output) = interop.drain().unwrap() {
        assert_eq!(output.frame.pts(), compared);
        let actual = output.frame.download_p010().unwrap();
        assert_eq!(
            actual,
            expected.pop_front().unwrap(),
            "drain frame {compared}"
        );
        compared += 1;
    }
    assert_eq!(compared, 30);
    assert_eq!(interop.validation_error_count(), 0);
}

#[test]
#[cfg(feature = "p010-output-diagnostic")]
#[ignore = "requires Intel iHD/ANV; end-to-end P010 decode to output interop"]
fn p010_full_external_input_and_output_match_cpu() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/codecs/hevc-main10-sdr-gradient.mp4");
    let mut decoded = vaapi_decoder(&path);
    let mut reference = Decoder::open(&path).unwrap();
    let desc = decoded.info().frame_desc.clone();
    assert_eq!(desc.format, PixelFormat::P010Le);
    let pool = VaapiDiagnosticP010Pool::new(
        std::path::Path::new("/dev/dri/renderD128"),
        desc.width,
        desc.height,
    )
    .unwrap();
    let mut cfg = config();
    cfg.grid_width = 8;
    let mut cpu = CpuAsciiBackend::new();
    let mut interop = VaapiVulkanFullInteropProcessor::new(
        VulkanAsciiBackend::new().unwrap(),
        pool.output_frames().unwrap(),
        desc,
        cfg.clone(),
    )
    .unwrap();
    let mut expected = VecDeque::new();
    let mut compared = 0;
    for _ in 0..30 {
        expected.push_back(
            cpu.process(reference.next_frame().unwrap().unwrap(), &cfg)
                .unwrap()
                .frame,
        );
        let input = decoded.next_vaapi_frame().unwrap().unwrap();
        if let Some(output) = interop.submit(input).unwrap() {
            assert_eq!(output.frame.pts(), compared);
            let actual = output.frame.download_p010().unwrap();
            let reference = expected.pop_front().unwrap();
            assert_eq!(actual.desc(), reference.desc());
            assert_eq!(
                actual.host().as_slice(),
                reference.host().as_slice(),
                "frame {compared}"
            );
            compared += 1;
        }
    }
    while let Some(output) = interop.drain().unwrap() {
        assert_eq!(output.frame.pts(), compared);
        let actual = output.frame.download_p010().unwrap();
        let reference = expected.pop_front().unwrap();
        assert_eq!(actual.desc(), reference.desc());
        assert_eq!(
            actual.host().as_slice(),
            reference.host().as_slice(),
            "drain {compared}"
        );
        compared += 1;
    }
    assert_eq!(compared, 30);
    assert_eq!(interop.validation_error_count(), 0);
}

#[test]
#[cfg(feature = "p010-output-diagnostic")]
#[ignore = "requires Intel iHD/ANV; 3000-frame P010 output lifetime and FD test"]
fn p010_output_3000_frame_fd_stress() {
    let before = fd_count();
    let mut early = 0;
    let mut maximum;
    {
        let desc = FrameDesc::host_p010_le(128, 64, ColorSpace::default()).unwrap();
        let pool =
            VaapiDiagnosticP010Pool::new(std::path::Path::new("/dev/dri/renderD128"), 128, 64)
                .unwrap();
        let mut cfg = config();
        cfg.grid_width = 16;
        let mut cpu = CpuAsciiBackend::new();
        let mut interop = VulkanVaapiOutputInteropProcessor::new(
            VulkanAsciiBackend::new().unwrap(),
            pool.output_frames().unwrap(),
            desc.clone(),
            cfg.clone(),
        )
        .unwrap();
        let active = fd_count();
        maximum = active;
        let mut expected = VecDeque::new();
        let mut compared = 0;
        for index in 0..3000 {
            let source = synthetic_p010(&desc, index);
            expected.push_back(cpu.process(source.clone(), &cfg).unwrap().frame);
            if let Some(output) = interop.submit(source).unwrap() {
                assert_eq!(output.frame.pts(), compared);
                assert_eq!(
                    output.frame.download_p010().unwrap(),
                    expected.pop_front().unwrap()
                );
                compared += 1;
            }
            let count = fd_count();
            if index == 29 {
                early = count;
            }
            maximum = maximum.max(count);
            assert!(
                count <= active + 12,
                "FD growth at {index}: {count} > {active} + 12"
            );
        }
        while let Some(output) = interop.drain().unwrap() {
            assert_eq!(output.frame.pts(), compared);
            assert_eq!(
                output.frame.download_p010().unwrap(),
                expected.pop_front().unwrap()
            );
            compared += 1;
        }
        assert_eq!(compared, 3000);
        assert_eq!(interop.validation_error_count(), 0);
    }
    let after = fd_count();
    println!("P010 output FD before={before} early={early} max={maximum} after={after}");
    assert_eq!(after, before, "P010 output FD baseline was not restored");
}

#[test]
#[cfg(feature = "p010-output-diagnostic")]
#[ignore = "requires Intel iHD/ANV and FreeType fixture"]
fn p010_output_freetype_matches_cpu() {
    let desc = FrameDesc::host_p010_le(128, 64, ColorSpace::default()).unwrap();
    let pool =
        VaapiDiagnosticP010Pool::new(std::path::Path::new("/dev/dri/renderD128"), 128, 64).unwrap();
    let mut cfg = config();
    cfg.grid_width = 16;
    let font = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fonts/Inconsolata-Regular.ttf");
    let (atlas, _) = asciiflow_font::build_font_atlas(&font, 0, &cfg.charset, 16, 16).unwrap();
    let mut cpu = CpuAsciiBackend::with_atlas(atlas.clone(), &cfg);
    let mut interop = VulkanVaapiOutputInteropProcessor::new(
        VulkanAsciiBackend::new()
            .unwrap()
            .with_atlas(atlas, &cfg)
            .unwrap(),
        pool.output_frames().unwrap(),
        desc.clone(),
        cfg.clone(),
    )
    .unwrap();
    let mut expected = VecDeque::new();
    let mut compared = 0;
    for index in 0..5 {
        let source = synthetic_p010(&desc, index);
        expected.push_back(cpu.process(source.clone(), &cfg).unwrap().frame);
        if let Some(output) = interop.submit(source).unwrap() {
            assert_eq!(
                output.frame.download_p010().unwrap(),
                expected.pop_front().unwrap()
            );
            compared += 1;
        }
    }
    while let Some(output) = interop.drain().unwrap() {
        assert_eq!(
            output.frame.download_p010().unwrap(),
            expected.pop_front().unwrap()
        );
        compared += 1;
    }
    assert_eq!(compared, 5);
    assert_eq!(interop.validation_error_count(), 0);
}

#[test]
#[cfg(feature = "p010-output-diagnostic")]
#[ignore = "requires Intel iHD/ANV; intentionally imports invalid DMA-BUF fd"]
fn p010_output_import_failure_and_early_drop_recover_fds() {
    let before = fd_count();
    {
        let desc = FrameDesc::host_p010_le(128, 64, ColorSpace::default()).unwrap();
        let pool =
            VaapiDiagnosticP010Pool::new(std::path::Path::new("/dev/dri/renderD128"), 128, 64)
                .unwrap();
        let mapping = DrmPrimeMapping::map_direct_write(pool.acquire(0).unwrap()).unwrap();
        let mut planes = mapping.duplicate_external_p010_planes().unwrap();
        planes[0].fd = std::fs::File::open("/dev/null").unwrap().into();
        let mut vulkan = VulkanAsciiBackend::new().unwrap();
        let stable = fd_count() - 2;
        let error = vulkan
            .copy_packed_to_external(&synthetic_p010(&desc, 0), &config(), planes)
            .unwrap_err();
        assert!(error.to_string().contains("DMA-BUF"));
        assert_eq!(fd_count(), stable, "failed P010 import leaked FDs");
        drop(mapping);
        let mut cfg = config();
        cfg.grid_width = 16;
        let mut processor = VulkanVaapiOutputInteropProcessor::new(
            VulkanAsciiBackend::new().unwrap(),
            pool.output_frames().unwrap(),
            desc.clone(),
            cfg,
        )
        .unwrap();
        processor.submit(synthetic_p010(&desc, 0)).unwrap();
        processor.submit(synthetic_p010(&desc, 1)).unwrap();
        // Drop while both slots may still be in flight; worker teardown joins them.
    }
    assert_eq!(
        fd_count(),
        before,
        "P010 early-drop FD baseline was not restored"
    );
}

#[test]
#[cfg(feature = "p010-output-diagnostic")]
#[ignore = "requires Intel iHD/ANV; diagnostic output lifecycle fault checkpoints"]
fn p010_output_faults_preserve_cause_and_do_not_reuse_surfaces() {
    for fault in [
        DiagnosticOutputFault::ImageCreate,
        DiagnosticOutputFault::MemoryImport,
        DiagnosticOutputFault::QueueSubmit,
        DiagnosticOutputFault::FenceWaitAfterCompletion,
    ] {
        let before = fd_count();
        {
            let desc = FrameDesc::host_p010_le(128, 64, ColorSpace::default()).unwrap();
            let pool =
                VaapiDiagnosticP010Pool::new(std::path::Path::new("/dev/dri/renderD128"), 128, 64)
                    .unwrap();
            let mut cfg = config();
            cfg.grid_width = 16;
            let backend = VulkanAsciiBackend::new()
                .unwrap()
                .with_diagnostic_output_fault(fault);
            let mut processor = VulkanVaapiOutputInteropProcessor::new(
                backend,
                pool.output_frames().unwrap(),
                desc.clone(),
                cfg,
            )
            .unwrap();
            processor.submit(synthetic_p010(&desc, 0)).unwrap();
            processor.submit(synthetic_p010(&desc, 1)).unwrap();
            let error = processor.drain().err().expect("injected fault must fail");
            assert!(error.to_string().contains(&format!("{fault:?}")), "{error}");
            assert!(
                processor.submit(synthetic_p010(&desc, 2)).is_err(),
                "failed processor reused a surface after {fault:?}"
            );
            assert_eq!(processor.validation_error_count(), 0, "{fault:?}");
        }
        assert_eq!(fd_count(), before, "{fault:?} leaked FDs");
    }
}

#[test]
#[cfg(feature = "p010-output-diagnostic")]
#[ignore = "Intel Arc 1920x1080 P010 output-transfer benchmark; run in Release without validation"]
fn p010_output_300_frame_benchmark() {
    fn process_cpu() -> Duration {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
        assert_eq!(
            unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) },
            0
        );
        let usage = unsafe { usage.assume_init() };
        let timeval = |value: libc::timeval| {
            Duration::from_secs(value.tv_sec as u64) + Duration::from_micros(value.tv_usec as u64)
        };
        timeval(usage.ru_utime) + timeval(usage.ru_stime)
    }
    let desc = FrameDesc::host_p010_le(1920, 1080, ColorSpace::default()).unwrap();
    let source = synthetic_p010(&desc, 0);
    let cfg = config();
    for run in 0..3 {
        let pool =
            VaapiDiagnosticP010Pool::new(std::path::Path::new("/dev/dri/renderD128"), 1920, 1080)
                .unwrap();
        let mut gpu = VulkanAsciiBackend::new().unwrap();
        let mut staged_upload = Duration::ZERO;
        let mut staged_readback = Duration::ZERO;
        let staged_cpu_started = process_cpu();
        let staged_wall = Instant::now();
        for index in 0..300 {
            let processed = gpu.process(source.clone(), &cfg).unwrap();
            staged_readback += processed.timings.host_readback
                + processed.timings.host_invalidate
                + processed.timings.gpu_download;
            let mut surface = pool.acquire(index).unwrap();
            let started = Instant::now();
            surface.upload_p010(&processed.frame).unwrap();
            staged_upload += started.elapsed();
        }
        let staged_total = staged_wall.elapsed();
        let staged_cpu = process_cpu() - staged_cpu_started;
        assert_eq!(gpu.validation_error_count(), 0);

        let mut output = VulkanVaapiOutputInteropProcessor::new(
            VulkanAsciiBackend::new().unwrap(),
            pool.output_frames().unwrap(),
            desc.clone(),
            cfg.clone(),
        )
        .unwrap();
        let mut map = Duration::ZERO;
        let mut import = Duration::ZERO;
        let mut copy = Duration::ZERO;
        let mut submit = Duration::ZERO;
        let mut wait = Duration::ZERO;
        let mut completed = 0;
        let interop_cpu_started = process_cpu();
        let interop_wall = Instant::now();
        let mut record = |item: asciiflow_interop::HardwareBackendOutput| {
            map += item.timings.output_drm_prime_map;
            import += item.timings.output_external_image_create
                + item.timings.output_external_memory_import
                + item.timings.output_external_memory_bind;
            copy += item.timings.gpu_external_output_copy;
            submit += item.timings.output_queue_submit;
            wait += item.timings.output_gpu_wait;
            completed += 1;
        };
        for _ in 0..300 {
            if let Some(item) = output.submit(source.clone()).unwrap() {
                record(item);
            }
        }
        while let Some(item) = output.drain().unwrap() {
            record(item);
        }
        let interop_total = interop_wall.elapsed();
        let interop_cpu = process_cpu() - interop_cpu_started;
        assert_eq!(completed, 300);
        assert_eq!(output.validation_error_count(), 0);
        let ms = |duration: Duration| duration.as_secs_f64() * 1000.0 / 300.0;
        println!(
            "P010 run={} staged_total_ms={:.3} staged_cpu={:.0}% staged_readback_ms={:.3} hwupload_ms={:.3} interop_total_ms={:.3} interop_cpu={:.0}% map_ms={:.3} import_ms={:.3} gpu_copy_ms={:.3} submit_ms={:.3} wait_ms={:.3}",
            run + 1,
            ms(staged_total),
            staged_cpu.as_secs_f64() / staged_total.as_secs_f64() * 100.0,
            ms(staged_readback),
            ms(staged_upload),
            ms(interop_total),
            interop_cpu.as_secs_f64() / interop_total.as_secs_f64() * 100.0,
            ms(map),
            ms(import),
            ms(copy),
            ms(submit),
            ms(wait),
        );

        let nv12_desc = FrameDesc::host_nv12(1920, 1080, ColorSpace::default()).unwrap();
        let nv12 = VideoFrame::new_host(
            nv12_desc.clone(),
            Some(0),
            HostFrame::new_zeroed(&nv12_desc),
        )
        .unwrap();
        let output_path = temporary_output("stage52c1-nv12-control");
        let encoder = Encoder::create_with(
            &output_path,
            nv12_desc.clone(),
            asciiflow_core::Rational::new(30, 1).unwrap(),
            EncodeMode::Vaapi,
            VaapiOptions::default(),
        )
        .unwrap();
        let mut nv12_output = VulkanVaapiOutputInteropProcessor::new(
            VulkanAsciiBackend::new().unwrap(),
            encoder.encoder_frames().unwrap(),
            nv12_desc,
            cfg.clone(),
        )
        .unwrap();
        let mut nv12_copy = Duration::ZERO;
        let mut nv12_import = Duration::ZERO;
        let mut nv12_count = 0;
        let nv12_cpu_started = process_cpu();
        let nv12_wall = Instant::now();
        let mut record_nv12 = |item: asciiflow_interop::HardwareBackendOutput| {
            nv12_copy += item.timings.gpu_external_output_copy;
            nv12_import += item.timings.output_external_image_create
                + item.timings.output_external_memory_import
                + item.timings.output_external_memory_bind;
            nv12_count += 1;
        };
        for _ in 0..300 {
            if let Some(item) = nv12_output.submit(nv12.clone()).unwrap() {
                record_nv12(item);
            }
        }
        while let Some(item) = nv12_output.drain().unwrap() {
            record_nv12(item);
        }
        let nv12_total = nv12_wall.elapsed();
        let nv12_cpu = process_cpu() - nv12_cpu_started;
        assert_eq!(nv12_count, 300);
        assert_eq!(nv12_output.validation_error_count(), 0);
        println!(
            "NV12 control run={} interop_total_ms={:.3} cpu={:.0}% import_ms={:.3} gpu_copy_ms={:.3}",
            run + 1,
            ms(nv12_total),
            nv12_cpu.as_secs_f64() / nv12_total.as_secs_f64() * 100.0,
            ms(nv12_import),
            ms(nv12_copy),
        );
        drop(nv12_output);
        drop(encoder);
        if output_path.exists() {
            std::fs::remove_file(output_path).unwrap();
        }
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
#[ignore = "requires Intel HEVC Main and Main10 VAAPI encode"]
fn main10_encoder_surface_cannot_enter_main8_context() {
    let color = asciiflow_core::ColorSpace::default();
    let main8_desc = asciiflow_core::FrameDesc::host_nv12(128, 128, color).unwrap();
    let main10_desc = asciiflow_core::FrameDesc::host_p010_le(128, 128, color).unwrap();
    let fps = asciiflow_core::Rational::new(30, 1).unwrap();
    let main8_path = temporary_output("main8-ownership");
    let main10_path = temporary_output("main10-ownership");
    let mut main8 = Encoder::create_with_hardware_frames_codec_and_audio(
        &main8_path,
        main8_desc,
        fps,
        VideoCodec::Hevc,
        VaapiOptions::default(),
        Vec::new(),
        Default::default(),
    )
    .unwrap();
    let mut main10 = Encoder::create_with_hardware_frames_codec_and_audio(
        &main10_path,
        main10_desc,
        fps,
        VideoCodec::Hevc,
        VaapiOptions::default(),
        Vec::new(),
        Default::default(),
    )
    .unwrap();
    let foreign = main10.encoder_frames().unwrap().acquire(0).unwrap();
    assert!(main8.encode_hardware_frame(foreign).is_err());
    let foreign = main8.encoder_frames().unwrap().acquire(0).unwrap();
    assert!(main10.encode_hardware_frame(foreign).is_err());
    main8.finish().unwrap();
    main10.finish().unwrap();
    drop(main8);
    drop(main10);
    std::fs::remove_file(main8_path).unwrap();
    std::fs::remove_file(main10_path).unwrap();
}

#[test]
#[ignore = "requires Intel AV1 10-bit, AV1 Main8, HEVC Main/Main10 and H.264 VAAPI encoders"]
fn av1_10bit_surface_rejects_every_foreign_encoder_context() {
    let color = asciiflow_core::ColorSpace::default();
    let p010 = asciiflow_core::FrameDesc::host_p010_le(128, 128, color).unwrap();
    let nv12 = asciiflow_core::FrameDesc::host_nv12(128, 128, color).unwrap();
    let fps = asciiflow_core::Rational::new(30, 1).unwrap();
    let av1_path = temporary_output("av1-10-ownership");
    let mut av1 = Encoder::create_with_hardware_frames_codec_and_audio(
        &av1_path,
        p010.clone(),
        fps,
        VideoCodec::Av1,
        VaapiOptions::default(),
        Vec::new(),
        Default::default(),
    )
    .unwrap();
    for (codec, desc) in [
        (VideoCodec::Av1, nv12.clone()),
        (VideoCodec::Hevc, p010.clone()),
        (VideoCodec::Hevc, nv12.clone()),
        (VideoCodec::H264, nv12.clone()),
    ] {
        let path = temporary_output("foreign-ownership");
        let same_format = desc.format == PixelFormat::P010Le;
        let mut foreign = Encoder::create_with_hardware_frames_codec_and_audio(
            &path,
            desc,
            fps,
            codec,
            VaapiOptions::default(),
            Vec::new(),
            Default::default(),
        )
        .unwrap();
        let av1_frame = av1.encoder_frames().unwrap().acquire(0).unwrap();
        let error = foreign.encode_hardware_frame(av1_frame).unwrap_err();
        if same_format {
            assert!(
                error.to_string().contains("different AVHWFramesContext"),
                "{error}"
            );
        }
        let foreign_frame = foreign.encoder_frames().unwrap().acquire(0).unwrap();
        let error = av1.encode_hardware_frame(foreign_frame).unwrap_err();
        if same_format {
            assert!(
                error.to_string().contains("different AVHWFramesContext"),
                "{error}"
            );
        }
        foreign.finish().unwrap();
        drop(foreign);
        std::fs::remove_file(path).unwrap();
    }
    av1.finish().unwrap();
    drop(av1);
    std::fs::remove_file(av1_path).unwrap();
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

#[test]
#[ignore = "requires 128x128+ HEVC Main10 SDR input and Intel Main10 encode"]
fn main10_encoder_owned_full_interop_30_frame_parity() {
    compare_output_interop_format(
        30,
        "ASCIIFLOW_STAGE52C2_HEVC_INPUT",
        "/tmp/asciiflow-main10-128.mp4",
        true,
        VideoCodec::Hevc,
        PixelFormat::P010Le,
    );
}

#[test]
#[ignore = "requires a 3000-frame 128x128+ Main10 SDR input and Intel Main10 encode"]
fn main10_encoder_owned_full_interop_3000_frame_stress() {
    compare_output_interop_format(
        3000,
        "ASCIIFLOW_STAGE52C2_HEVC_STRESS_INPUT",
        "/tmp/asciiflow-main10-3000.mp4",
        true,
        VideoCodec::Hevc,
        PixelFormat::P010Le,
    );
}

#[test]
#[ignore = "requires a 3000-frame 128x128+ Main10 SDR input and Intel Main10 encode"]
fn main10_staged_encode_3000_frame_fd_stress() {
    staged_p010_encode_3000_frame_fd_stress(
        "ASCIIFLOW_STAGE52C2_HEVC_STRESS_INPUT",
        "/tmp/asciiflow-main10-3000.mp4",
        VideoCodec::Hevc,
    );
}

#[test]
#[ignore = "requires 128x128+ AV1 10-bit SDR input and Intel AV1 10-bit encode"]
fn av1_10bit_encoder_owned_full_interop_30_frame_parity() {
    compare_output_interop_format(
        30,
        "ASCIIFLOW_STAGE52C3_AV1_INPUT",
        "/tmp/asciiflow-av1-10-128.mp4",
        true,
        VideoCodec::Av1,
        PixelFormat::P010Le,
    );
}

#[test]
#[ignore = "requires a 3000-frame 128x128+ AV1 10-bit SDR input and Intel AV1 10-bit encode"]
fn av1_10bit_encoder_owned_full_interop_3000_frame_stress() {
    compare_output_interop_format(
        3000,
        "ASCIIFLOW_STAGE52C3_AV1_STRESS_INPUT",
        "/tmp/asciiflow-av1-10-3000.mp4",
        true,
        VideoCodec::Av1,
        PixelFormat::P010Le,
    );
}

#[test]
#[ignore = "requires a 3000-frame 128x128+ AV1 10-bit SDR input and Intel AV1 10-bit encode"]
fn av1_10bit_staged_encode_3000_frame_fd_stress() {
    staged_p010_encode_3000_frame_fd_stress(
        "ASCIIFLOW_STAGE52C3_AV1_STRESS_INPUT",
        "/tmp/asciiflow-av1-10-3000.mp4",
        VideoCodec::Av1,
    );
}

fn staged_p010_encode_3000_frame_fd_stress(env_name: &str, fallback: &str, codec: VideoCodec) {
    let path = input(env_name, fallback);
    let output_path = temporary_output("main10-staged-stress");
    let before = fd_count();
    {
        let mut decoder = vaapi_decoder(&path);
        let desc = decoder.info().frame_desc.clone();
        assert_eq!(desc.format, PixelFormat::P010Le);
        let frame_rate = decoder.info().frame_rate;
        let mut processor = VaapiVulkanInteropProcessor::new(
            VulkanAsciiBackend::new().unwrap(),
            desc.clone(),
            config(),
        )
        .unwrap();
        let mut encoder = Encoder::create_with_codec_and_audio(
            &output_path,
            desc,
            frame_rate,
            OutputEncoding {
                codec: codec.clone(),
                mode: EncodeMode::Vaapi,
            },
            VaapiOptions::default(),
            Vec::new(),
            Default::default(),
        )
        .unwrap();
        let active_baseline = fd_count();
        let mut max_seen = active_baseline;
        let mut encoded = 0usize;
        for index in 0..3000 {
            let source = decoder
                .next_vaapi_frame()
                .unwrap()
                .unwrap_or_else(|| panic!("input ended at {index}"));
            if let Some(output) = processor.submit(source).unwrap() {
                encoder.encode(output.frame).unwrap();
                encoded += 1;
            }
            if index % 250 == 249 {
                let current = fd_count();
                max_seen = max_seen.max(current);
                assert!(
                    current <= active_baseline + 8,
                    "staged FD count grew at frame {index}: {current}"
                );
            }
        }
        while let Some(output) = processor.drain().unwrap() {
            encoder.encode(output.frame).unwrap();
            encoded += 1;
        }
        encoder.finish().unwrap();
        assert_eq!(encoded, 3000);
        assert_eq!(processor.validation_error_count(), 0);
        println!(
            "staged fd_count before={before} active_baseline={active_baseline} max_steady={max_seen}"
        );
    }
    let after = fd_count();
    println!("staged fd_count after={after}");
    assert_eq!(after, before);
    let mut decoded = Decoder::open(&output_path).unwrap();
    assert_eq!(decoded.info().requirements.bit_depth, Some(10));
    assert_eq!(decoded.info().requirements.codec, codec);
    let mut count = 0;
    while decoded.next_frame().unwrap().is_some() {
        count += 1;
    }
    assert_eq!(count, 3000);
    drop(decoded);
    std::fs::remove_file(output_path).unwrap();
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
    compare_output_interop_format(
        frames,
        env_name,
        fallback,
        check_fds,
        output_codec,
        PixelFormat::Nv12,
    );
}

fn compare_output_interop_format(
    frames: usize,
    env_name: &str,
    fallback: &str,
    check_fds: bool,
    output_codec: VideoCodec,
    format: PixelFormat,
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
        let expected_fourcc = match format {
            PixelFormat::Nv12 => ("R8..", "GR88"),
            PixelFormat::P010Le => ("R16.", "GR32"),
        };
        assert_eq!(fourcc_name(encoder_drm.layers[0].format), expected_fourcc.0);
        assert_eq!(fourcc_name(encoder_drm.layers[1].format), expected_fourcc.1);
        drop(descriptor_mapping);

        let full_backend = VulkanAsciiBackend::new().unwrap();
        let mut full =
            VaapiVulkanFullInteropProcessor::new(full_backend, encoder_frames, desc, cfg).unwrap();
        let active_baseline = fd_count();
        active_baseline_report = active_baseline;
        max_seen = max_seen.max(active_baseline);
        let mut expected = VecDeque::with_capacity(3);
        let mut compared = 0;
        let mut non_four_aligned_samples = 0usize;

        for index in 0..frames {
            let reference_input = reference_decoder
                .next_vaapi_frame()
                .unwrap()
                .unwrap_or_else(|| panic!("reference ended at frame {index}; expected {frames}"));
            if let Some(output) = reference.submit(reference_input).unwrap() {
                if format == PixelFormat::P010Le {
                    non_four_aligned_samples += output
                        .frame
                        .host()
                        .as_slice()
                        .chunks_exact(2)
                        .filter(|bytes| (u16::from_le_bytes([bytes[0], bytes[1]]) >> 6) & 3 != 0)
                        .count();
                }
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
                let actual = match format {
                    PixelFormat::Nv12 => output.frame.download_nv12(),
                    PixelFormat::P010Le => output.frame.download_p010(),
                }
                .unwrap();
                let mut staged = staged_frames.acquire(compared as i64).unwrap();
                match format {
                    PixelFormat::Nv12 => staged.upload_nv12(&expected),
                    PixelFormat::P010Le => staged.upload_p010(&expected),
                }
                .unwrap();
                let staged = match format {
                    PixelFormat::Nv12 => staged.download_nv12(),
                    PixelFormat::P010Le => staged.download_p010(),
                }
                .unwrap();
                assert_eq!(
                    actual.desc(),
                    expected.desc(),
                    "metadata mismatch at {compared}"
                );
                assert_eq!(
                    actual.host().as_slice(),
                    expected.host().as_slice(),
                    "pre-encode {:?} mismatch at {compared}",
                    format
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
            if format == PixelFormat::P010Le {
                non_four_aligned_samples += output
                    .frame
                    .host()
                    .as_slice()
                    .chunks_exact(2)
                    .filter(|bytes| (u16::from_le_bytes([bytes[0], bytes[1]]) >> 6) & 3 != 0)
                    .count();
            }
            expected.push_back(output.frame);
        }
        while let Some(output) = full.drain().unwrap() {
            let expected = expected.pop_front().unwrap();
            assert_eq!(output.frame.pts(), compared as i64);
            let actual = match format {
                PixelFormat::Nv12 => output.frame.download_nv12(),
                PixelFormat::P010Le => output.frame.download_p010(),
            }
            .unwrap();
            let mut staged = staged_frames.acquire(compared as i64).unwrap();
            match format {
                PixelFormat::Nv12 => staged.upload_nv12(&expected),
                PixelFormat::P010Le => staged.upload_p010(&expected),
            }
            .unwrap();
            let staged = match format {
                PixelFormat::Nv12 => staged.download_nv12(),
                PixelFormat::P010Le => staged.download_p010(),
            }
            .unwrap();
            assert_eq!(
                actual.desc(),
                expected.desc(),
                "metadata mismatch at {compared}"
            );
            assert_eq!(
                actual.host().as_slice(),
                expected.host().as_slice(),
                "pre-encode {:?} mismatch at {compared}",
                format
            );
            assert_eq!(staged, expected, "staged surface mismatch at {compared}");
            encoder.encode_hardware_frame(output.frame).unwrap();
            compared += 1;
        }
        encoder.finish().unwrap();
        assert_eq!(compared, frames);
        if format == PixelFormat::P010Le {
            println!("pre_encode_non_four_aligned_samples={non_four_aligned_samples}");
            assert!(
                non_four_aligned_samples > 0,
                "Main10 processing lost low active bits before encode"
            );
        }
        assert!(expected.is_empty());
        assert_eq!(reference.validation_error_count(), 0);
        assert_eq!(full.validation_error_count(), 0);
    }
    let mut decoded = Decoder::open(&output_path).unwrap();
    assert_eq!(decoded.info().requirements.codec, expected_output_codec);
    if format == PixelFormat::P010Le {
        assert_eq!(decoded.info().requirements.bit_depth, Some(10));
        assert_eq!(
            decoded.info().requirements.profile,
            Some(match expected_output_codec {
                VideoCodec::Hevc => asciiflow_core::VideoProfile::HevcMain10,
                VideoCodec::Av1 => asciiflow_core::VideoProfile::Av1Main,
                _ => unreachable!("P010 output is HEVC Main10 or AV1 Main"),
            })
        );
    }
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
