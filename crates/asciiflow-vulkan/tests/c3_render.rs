#![cfg(feature = "hdr-to-sdr-qualification")]

use asciiflow_core::sdr_target_volume::convert_method_a_rgb;
use asciiflow_core::tone_map_bt2446::MethodA;
use asciiflow_core::{AsciiConfig, ColorSpace, FrameDesc, HostFrame, VideoFrame};
use asciiflow_cpu::hdr::HdrPqReference;
use asciiflow_font::GlyphAtlas;
use asciiflow_vulkan::VulkanHdrToSdrQualification;

fn frame(bright: bool, pts: i64) -> VideoFrame {
    let desc = FrameDesc::host_p010_le(66, 50, ColorSpace::pq_bt2020()).unwrap();
    let mut storage = HostFrame::new_zeroed(&desc);
    let (y, uv) = storage.planes_mut(&desc);
    for (i, sample) in y.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        let code = if bright {
            450 + (i % 4) as u16
        } else {
            80 + (i % 4) as u16
        };
        sample.copy_from_slice(&(code << 6).to_le_bytes());
    }
    for (i, sample) in uv.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        let code = if bright {
            if i % 2 == 0 { 540u16 } else { 480u16 }
        } else {
            512u16
        };
        sample.copy_from_slice(&(code << 6).to_le_bytes());
    }
    VideoFrame::new_host(desc, Some(pts), storage).unwrap()
}

#[test]
#[ignore = "real GPU staged/pending ownership guards and pre-submit recovery"]
fn staged_slot_guards_and_retry() {
    let config = AsciiConfig::default();
    let input = frame(false, 123);
    let mut slot = VulkanHdrToSdrQualification::new().unwrap();
    slot.stage_host(&input, &config, false).unwrap();
    assert!(slot.stage_host(&input, &config, false).is_err());
    assert!(slot.process_linear(2, 2, &[[1.0; 3]; 4]).is_err());
    assert!(slot.complete().is_err());
    slot.inject_fault(asciiflow_vulkan::C3Fault::BeforeSubmit);
    assert!(slot.submit_staged().is_err());
    assert!(slot.submit_staged().is_err());
    slot.stage_host(&input, &config, false).unwrap();
    slot.submit_staged().unwrap();
    assert!(slot.stage_host(&input, &config, false).is_err());
    assert_eq!(slot.complete().unwrap().pts, Some(123));
    slot.stage_host(&input, &config, false).unwrap();
    slot.submit_staged().unwrap();
    assert_eq!(slot.complete().unwrap().pts, Some(123));
    let observe = slot.validation_observer();
    drop(slot);
    assert_eq!(observe(), 0);
}

#[test]
#[ignore = "real GPU; uneven geometry, real low bits, builtin/FreeType/color/mono and two slots"]
fn render_and_two_slot_contamination() {
    for freetype in [false, true] {
        for color in [false, true] {
            let config = AsciiConfig {
                grid_width: 13,
                grid_height: Some(9),
                color,
                font: if freetype {
                    "qualification-Inconsolata".into()
                } else {
                    "builtin-8x8".into()
                },
                ..Default::default()
            };
            let atlas = if freetype {
                let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/fonts/Inconsolata-Regular.ttf");
                asciiflow_font::build_font_atlas(&path, 0, &config.charset, 16, 16)
                    .unwrap()
                    .0
            } else {
                GlyphAtlas::builtin(&config.font, &config.charset).unwrap()
            };
            let reference = HdrPqReference::new(&atlas, &config.charset).unwrap();
            let mut slot = VulkanHdrToSdrQualification::new()
                .unwrap()
                .with_atlas(atlas.clone(), &config)
                .unwrap();
            let mut second = slot.try_fork().unwrap();
            let mut expected = Vec::new();
            for bright in [false, true] {
                let input = frame(bright, 0);
                let low_bits = input
                    .host()
                    .as_slice()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .filter(|bytes| (u16::from_le_bytes([bytes[0], bytes[1]]) >> 6) & 3 != 0)
                    .count();
                assert_eq!(low_bits, 2475);
                let grid = reference.map(&input, 13, 9).unwrap();
                let linear = reference.render_linear(&grid, input.desc(), color).unwrap();
                let output = slot.process_host(&input, &config, true).unwrap();
                assert_eq!(&output.diagnostics[..5], &[0; 5]);
                assert_eq!(output.cells.len(), grid.cells.len());
                for (gpu, cpu) in output.cells.iter().zip(&grid.cells) {
                    assert_eq!(gpu.counts[0], u32::from(cpu.glyph));
                }
                let coverage = output.coverage.as_ref().unwrap();
                assert_eq!(coverage.len(), linear.coverage().len());
                for (&gpu, &cpu) in coverage.iter().zip(linear.coverage()) {
                    assert_eq!(gpu, cpu);
                }
                let mut a_max = 0f64;
                let mut final_max = 0u32;
                for (i, pixel) in linear.pixels().iter().enumerate() {
                    let original = [pixel.r, pixel.g, pixel.b];
                    for (c, &value) in original.iter().enumerate() {
                        a_max = a_max.max(
                            (f64::from(output.linear_hdr.as_ref().unwrap()[i][c]) - value).abs(),
                        );
                    }
                    let cpu = convert_method_a_rgb(MethodA::new().map(*pixel).unwrap().rgb)
                        .unwrap()
                        .nonlinear
                        .components();
                    for (c, &value) in cpu.iter().enumerate() {
                        let quantize = |v: f64| (v.clamp(0.0, 1.0) * 65535.0 + 0.5).floor() as u32;
                        final_max = final_max.max(
                            quantize(value)
                                .abs_diff(quantize(f64::from(output.nonlinear_709[i][c]))),
                        );
                    }
                }
                eprintln!(
                    "C3 render freetype={freetype} color={color} bright={bright}: glyph mismatch=0, coverage unchanged, A max nits={a_max:e}, final UNORM16 max={final_max}"
                );
                assert!(final_max <= 2);
                expected.push(output.nonlinear_709);
            }
            let bytes = slot.qualification_bytes_per_slot();
            for iteration in 0..200 {
                let bright = iteration % 2 != 0;
                slot.submit_host(&frame(bright, iteration), &config, true)
                    .unwrap();
                second
                    .submit_host(&frame(!bright, -iteration), &config, true)
                    .unwrap();
                assert!(slot.submit_host(&frame(false, 0), &config, true).is_err());
                let a = slot.complete().unwrap();
                let b = second.complete().unwrap();
                assert_eq!(a.pts, Some(iteration));
                assert_eq!(b.pts, Some(-iteration));
                assert_eq!(a.nonlinear_709, expected[usize::from(bright)]);
                assert_eq!(b.nonlinear_709, expected[usize::from(!bright)]);
                assert_eq!(slot.qualification_bytes_per_slot(), bytes);
            }
            assert_eq!(slot.validation_error_count(), 0);
            assert_eq!(second.validation_error_count(), 0);
            // A consuming atlas builder must reject without dropping map buffers
            // before outstanding C3 work completes.
            slot.submit_host(&frame(false, 0), &config, true).unwrap();
            assert!(slot.with_atlas(atlas, &config).is_err());
            assert_eq!(second.validation_error_count(), 0);
        }
    }
}
