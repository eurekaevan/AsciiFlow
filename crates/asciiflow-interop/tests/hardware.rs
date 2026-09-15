use asciiflow_core::{AsciiBackend, AsciiConfig, FrameSink, FrameSource};
use asciiflow_interop::{
    DrmPrimeMapping, VaapiVulkanFullInteropProcessor, VaapiVulkanInteropProcessor, fourcc_name,
};
use asciiflow_media::{DecodeMode, Decoder, EncodeMode, Encoder, VaapiOptions};
use asciiflow_vulkan::VulkanAsciiBackend;
use std::{collections::VecDeque, path::PathBuf};

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
    compare_output_interop(30, "ASCIIFLOW_STAGE3_INPUT", "input.mp4", false);
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
#[ignore = "requires a 3000+ frame H.264 input, Intel iHD VAAPI, and ANV Vulkan"]
fn full_interop_surface_reuse_is_exact_and_fd_bounded() {
    compare_output_interop(
        3000,
        "ASCIIFLOW_STAGE3_STRESS_INPUT",
        "/tmp/asciiflow-stage3-stress-input.mp4",
        true,
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

fn compare_output_interop(frames: usize, env_name: &str, fallback: &str, check_fds: bool) {
    let path = input(env_name, fallback);
    let output_path = temporary_output(if check_fds {
        "stage3b-stress"
    } else {
        "stage3b-parity"
    });
    let _ = std::fs::remove_file(&output_path);
    let before = fd_count();
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
        let mut encoder = Encoder::create_with(
            &output_path,
            desc.clone(),
            frame_rate,
            EncodeMode::Vaapi,
            VaapiOptions::default(),
        )
        .unwrap();
        let encoder_frames = encoder.encoder_frames().unwrap();
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
            encoder.encode_hardware_frame(output.frame).unwrap();
            compared += 1;
        }
        encoder.finish().unwrap();
        assert_eq!(compared, frames);
        assert!(expected.is_empty());
        assert_eq!(reference.validation_error_count(), 0);
        assert_eq!(full.validation_error_count(), 0);
    }
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
