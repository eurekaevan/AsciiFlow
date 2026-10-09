#![cfg(feature = "hdr-to-sdr-qualification")]

use asciiflow_core::{AsciiConfig, PixelFormat};
use asciiflow_cpu::sdr_output::{NonlinearBt709Rgb, pack};
use asciiflow_interop::{DrmPrimeMapping, pack_completed_sdr_surface};
use asciiflow_media::{DecodeMode, Decoder, VaapiOptions, VaapiSdrQualificationPool};
use asciiflow_vulkan::{SdrPackFormat, VulkanHdrToSdrQualification};
use std::{path::Path, time::Instant};

fn fds() -> usize {
    std::fs::read_dir("/proc/self/fd").unwrap().count()
}

#[test]
#[ignore = "real VAAPI decode, resident C-3 pack and encoder-style surface readback"]
fn legal_pq_to_both_sdr_surfaces() {
    let before = fds();
    let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/codecs");
    for filename in [
        "hevc-main10-pq-c3-legal-v1.mp4",
        "av1-main10-pq-c3-legal-v1.mp4",
    ] {
        for freetype in [false, true] {
            for color in [false, true] {
                let config = AsciiConfig {
                    color,
                    font: if freetype {
                        "qualification-Inconsolata".into()
                    } else {
                        "builtin-8x8".into()
                    },
                    ..Default::default()
                };
                let atlas = if freetype {
                    asciiflow_font::build_font_atlas(
                        &fixture_root.join("../fonts/Inconsolata-Regular.ttf"),
                        0,
                        &config.charset,
                        16,
                        16,
                    )
                    .unwrap()
                    .0
                } else {
                    asciiflow_font::GlyphAtlas::builtin(&config.font, &config.charset).unwrap()
                };
                let mut decoder = Decoder::open_pq_qualification(
                    fixture_root.join(filename),
                    DecodeMode::Vaapi,
                    VaapiOptions::default(),
                )
                .unwrap();
                let frame = decoder.next_vaapi_frame().unwrap().unwrap();
                let desc = frame.desc().clone();
                let pts = frame.pts();
                let mapping = DrmPrimeMapping::map_direct_read(frame).unwrap();
                let planes = mapping.duplicate_external_p010_planes().unwrap();
                let mut slot = VulkanHdrToSdrQualification::new()
                    .unwrap()
                    .with_atlas(atlas, &config)
                    .unwrap();
                let observe = slot.validation_observer();
                assert!(
                    slot.pack_completed_sdr(SdrPackFormat::Nv12, None, None)
                        .is_err()
                );
                slot.submit_external(&desc, pts, &config, planes, false)
                    .unwrap();
                let output = slot.complete().unwrap();
                let rgb: Vec<_> = output
                    .nonlinear_709
                    .iter()
                    .map(|v| NonlinearBt709Rgb(v.map(f64::from)))
                    .collect();
                for format in [PixelFormat::Nv12, PixelFormat::P010Le] {
                    let pool = VaapiSdrQualificationPool::new(
                        Path::new("/dev/dri/renderD128"),
                        desc.width,
                        desc.height,
                        format,
                    )
                    .unwrap();
                    let surface = pool.acquire().unwrap();
                    let native = unsafe { &*surface.as_raw_ptr() };
                    let tags = (
                        native.color_primaries,
                        native.color_trc,
                        native.colorspace,
                        native.color_range,
                        native.pts,
                    );
                    let surface_mapping = DrmPrimeMapping::map_direct_write(surface).unwrap();
                    println!(
                        "{filename} freetype={freetype} color={color} {format:?} runtime descriptor: {:?}",
                        surface_mapping.descriptor()
                    );
                    let surface = surface_mapping.into_source();
                    let started = Instant::now();
                    let (surface, packed) =
                        pack_completed_sdr_surface(&mut slot, surface, None).unwrap();
                    assert_eq!(packed.diagnostics, [0; 3]);
                    let native = unsafe { &*surface.as_raw_ptr() };
                    assert_eq!(
                        tags,
                        (
                            native.color_primaries,
                            native.color_trc,
                            native.colorspace,
                            native.color_range,
                            native.pts
                        ),
                        "C-4A must not write native metadata"
                    );
                    let actual = match format {
                        PixelFormat::Nv12 => surface.download_nv12(),
                        PixelFormat::P010Le => surface.download_p010(),
                    }
                    .unwrap();
                    assert_eq!(
                        packed.bytes,
                        actual.host().as_slice(),
                        "actual surface vs resident packed buffer"
                    );
                    let expected = pack(desc.width, desc.height, &rgb, format).unwrap();
                    let mismatches: Vec<_> = expected
                        .iter()
                        .zip(&packed.bytes)
                        .enumerate()
                        .filter(|(_, (a, b))| a != b)
                        .collect();
                    println!(
                        "{filename} color={color} {format:?}: {} byte mismatches; pack={:?}, copy={:?}, wall={:?}, bytes={}",
                        mismatches.len(),
                        packed.pack_duration,
                        packed.copy_duration,
                        started.elapsed(),
                        packed.buffer_bytes
                    );
                    for (index, (cpu, gpu)) in mismatches.iter().take(16) {
                        println!("mismatch byte={index} cpu={cpu} gpu={gpu}");
                    }
                    if format == PixelFormat::P010Le {
                        for word in packed.bytes.as_chunks::<2>().0 {
                            assert_eq!(u16::from_le_bytes([word[0], word[1]]) & 63, 0);
                        }
                    }
                    if !color {
                        let offset = format.y_plane_len(desc.width, desc.height).unwrap();
                        let neutral = if format == PixelFormat::Nv12 {
                            vec![128]
                        } else {
                            (512u16 << 6).to_le_bytes().to_vec()
                        };
                        for sample in packed.bytes[offset..].chunks_exact(neutral.len()) {
                            assert_eq!(sample, neutral);
                        }
                    }
                    assert!(
                        mismatches.is_empty(),
                        "C-4A exact code parity failed; no arbitrary tolerance authorized"
                    );
                    assert_eq!(packed.validation_errors, 0);
                }
                drop(slot);
                assert_eq!(observe(), 0);
            }
        }
    }
    assert_eq!(
        fds(),
        before,
        "VAAPI/Vulkan output qualification FD closure"
    );
}

#[test]
#[ignore = "real GPU and VAAPI surface faults, cache rebuild and completed-signal guards"]
fn surface_fault_cleanup_and_retry() {
    use asciiflow_vulkan::SdrPackFault;
    let before = fds();
    let mut slot = VulkanHdrToSdrQualification::new().unwrap();
    let observe = slot.validation_observer();
    let result = slot.process_linear(4, 4, &[[100.; 3]; 16]).unwrap();
    let rgb: Vec<_> = result
        .nonlinear_709
        .iter()
        .map(|v| NonlinearBt709Rgb(v.map(f64::from)))
        .collect();
    for format in [PixelFormat::Nv12, PixelFormat::P010Le] {
        let pool =
            VaapiSdrQualificationPool::new(Path::new("/dev/dri/renderD128"), 4, 4, format).unwrap();
        // Establish the cached packer first: initialization faults must not be
        // bypassed simply because this format has already packed a frame.
        pack_completed_sdr_surface(&mut slot, pool.acquire().unwrap(), None).unwrap();
        for fault in [
            SdrPackFault::Pipeline,
            SdrPackFault::Buffer,
            SdrPackFault::Descriptor,
            SdrPackFault::Dispatch,
            SdrPackFault::AfterFence,
            SdrPackFault::Readback,
            SdrPackFault::ExternalImport,
            SdrPackFault::ExternalCopy,
            SdrPackFault::ExternalAfterFence,
        ] {
            let error = pack_completed_sdr_surface(&mut slot, pool.acquire().unwrap(), Some(fault))
                .err()
                .expect("injected surface fault must fail");
            assert!(
                error.to_string().contains(&format!("{fault:?}")),
                "root cause lost: {error}"
            );
            let (surface, output) =
                pack_completed_sdr_surface(&mut slot, pool.acquire().unwrap(), None).unwrap();
            assert_eq!(output.bytes, pack(4, 4, &rgb, format).unwrap());
            let actual = match format {
                PixelFormat::Nv12 => surface.download_nv12(),
                PixelFormat::P010Le => surface.download_p010(),
            }
            .unwrap();
            assert_eq!(actual.host().as_slice(), output.bytes);
            assert_eq!(output.validation_errors, 0);
        }
    }
    // A failed restage clears readiness; a previous frame is not a fallback.
    assert!(slot.process_linear(3, 4, &[[100.; 3]; 12]).is_err());
    assert!(
        slot.pack_completed_sdr(SdrPackFormat::Nv12, None, None)
            .is_err()
    );
    drop(slot);
    assert_eq!(observe(), 0);
    assert_eq!(fds(), before);
}

#[test]
#[ignore = "real dual-slot alternating black/saturated P010 source and both SDR surfaces"]
fn alternating_black_saturated_slots_do_not_contaminate() {
    use asciiflow_core::{ColorSpace, FrameDesc, HostFrame, VideoFrame, hdr_pq};
    let before = fds();
    let desc = FrameDesc::host_p010_le(64, 48, ColorSpace::pq_bt2020()).unwrap();
    let config = AsciiConfig {
        grid_width: 8,
        grid_height: Some(6),
        ..Default::default()
    };
    let mut sources = Vec::new();
    for rgb in [
        hdr_pq::LinearRgb {
            r: 0.,
            g: 0.,
            b: 0.,
        },
        hdr_pq::LinearRgb {
            r: 900.,
            g: 30.,
            b: 60.,
        },
    ] {
        let (codes, _) =
            hdr_pq::encode_limited(hdr_pq::pq_to_ycbcr(hdr_pq::linear_to_pq(rgb).unwrap()))
                .unwrap();
        let mut host = HostFrame::new_zeroed(&desc);
        let (y, uv) = host.planes_mut(&desc);
        for sample in y.as_chunks_mut::<2>().0 {
            sample.copy_from_slice(&(codes.y << 6).to_le_bytes());
        }
        for sample in uv.as_chunks_mut::<4>().0 {
            sample[..2].copy_from_slice(&(codes.cb << 6).to_le_bytes());
            sample[2..].copy_from_slice(&(codes.cr << 6).to_le_bytes());
        }
        sources.push(VideoFrame::new_host(desc.clone(), None, host).unwrap());
    }
    let first = VulkanHdrToSdrQualification::new().unwrap();
    let second = first.try_fork().unwrap();
    let observer = first.validation_observer();
    let mut slots = [first, second];
    let pools = [PixelFormat::Nv12, PixelFormat::P010Le].map(|format| {
        VaapiSdrQualificationPool::new(Path::new("/dev/dri/renderD128"), 64, 48, format).unwrap()
    });
    struct ExpectedSignal {
        rgb: Vec<[f32; 3]>,
        diagnostics: [u32; 9],
    }
    let mut expected: [Option<ExpectedSignal>; 2] = [None, None];
    for round in 0..256 {
        let order = if round % 2 == 0 { [0, 1] } else { [1, 0] };
        for (kind, index) in order.into_iter().enumerate() {
            slots[index]
                .stage_host(&sources[kind], &config, false)
                .unwrap();
        }
        for index in order {
            slots[index].submit_staged().unwrap();
        }
        for (kind, index) in order.into_iter().enumerate() {
            let output = slots[index].complete().unwrap();
            if let Some(signal) = &expected[kind] {
                assert_eq!(output.nonlinear_709, signal.rgb);
                assert_eq!(output.diagnostics, signal.diagnostics);
            } else {
                expected[kind] = Some(ExpectedSignal {
                    rgb: output.nonlinear_709.clone(),
                    diagnostics: output.diagnostics,
                });
            }
            assert_eq!(&output.diagnostics[..5], &[0; 5]);
            let reference: Vec<_> = output
                .nonlinear_709
                .iter()
                .map(|v| NonlinearBt709Rgb(v.map(f64::from)))
                .collect();
            for (pool, format) in pools.iter().zip([PixelFormat::Nv12, PixelFormat::P010Le]) {
                let (surface, packed) =
                    pack_completed_sdr_surface(&mut slots[index], pool.acquire().unwrap(), None)
                        .unwrap();
                assert_eq!(packed.bytes, pack(64, 48, &reference, format).unwrap());
                let actual = match format {
                    PixelFormat::Nv12 => surface.download_nv12(),
                    PixelFormat::P010Le => surface.download_p010(),
                }
                .unwrap();
                assert_eq!(actual.host().as_slice(), packed.bytes);
                assert_eq!(packed.diagnostics, [0; 3]);
                assert_eq!(packed.validation_errors, 0);
            }
        }
    }
    drop(pools);
    drop(slots);
    assert_eq!(observer(), 0);
    assert_eq!(fds(), before);
}
