mod backend;
mod buffer;
mod context;
mod external;
mod streaming;

pub use backend::{GpuAsciiCell, MemoryAllocationInfo, VulkanAsciiBackend};
pub use context::{DeviceInfo, MemoryHeapInfo, MemoryTypeInfo, enumerate_devices};
pub use external::{
    ExternalImageAccess, ExternalImageTimings, ExternalPlaneImage, ExternalPlaneKind,
};
pub use streaming::PipelinedVulkanAsciiBackend;

#[cfg(test)]
mod tests {
    use super::*;
    use asciiflow_core::{AsciiBackend, AsciiConfig, ColorSpace, FrameDesc, HostFrame, VideoFrame};
    use asciiflow_cpu::{CpuAsciiBackend, Nv12Mapper};
    use std::{
        sync::{
            Arc,
            mpsc::{self, Receiver, Sender},
        },
        thread,
        time::{Duration, Instant},
    };

    fn patterned_frame() -> VideoFrame {
        let desc = FrameDesc::host_nv12(64, 48, ColorSpace::default()).unwrap();
        let mut bytes = vec![0; desc.byte_len()];
        let y_len = 64 * 48;
        for (index, value) in bytes[..y_len].iter_mut().enumerate() {
            *value = 16 + ((index * 37 + index / 64 * 11) % 220) as u8;
        }
        for (index, pair) in bytes[y_len..].chunks_exact_mut(2).enumerate() {
            pair[0] = 48 + (index * 13 % 160) as u8;
            pair[1] = 48 + (index * 29 % 160) as u8;
        }
        VideoFrame::new_host(
            desc.clone(),
            Some(17),
            HostFrame::from_nv12(&desc, bytes).unwrap(),
        )
        .unwrap()
    }

    fn synthetic_p010_frame(pattern: usize, pts: i64) -> VideoFrame {
        synthetic_p010_frame_sized(64, 48, pattern, pts)
    }

    fn synthetic_p010_frame_sized(width: u32, height: u32, pattern: usize, pts: i64) -> VideoFrame {
        let desc = FrameDesc::host_p010_le(width, height, ColorSpace::default()).unwrap();
        let mut bytes = vec![0; desc.byte_len()];
        for index in 0..(desc.byte_len() / 2) {
            let x = index % width as usize;
            let y = index / width as usize;
            let code = match pattern {
                0 => 513,
                1 => 64 + ((x * 11 + y * 17 + index % 4) % 900) as u16,
                2 => {
                    if ((x / 8) + (y / 8)) % 2 == 0 {
                        129
                    } else {
                        895
                    }
                }
                _ => {
                    64 + ((index.wrapping_mul(1_103_515_245).wrapping_add(12_345) >> 9) % 900)
                        as u16
                }
            };
            bytes[index * 2..index * 2 + 2].copy_from_slice(&(code << 6).to_le_bytes());
        }
        VideoFrame::new_host(
            desc.clone(),
            Some(pts),
            HostFrame::from_p010_le(&desc, bytes).unwrap(),
        )
        .unwrap()
    }

    #[test]
    #[ignore = "300 synthetic 1080p P010 frames; run explicitly on the device being measured"]
    fn p010_synthetic_1080p_300_frame_benchmark() {
        let frame = synthetic_p010_frame_sized(1920, 1080, 1, 0);
        let config = AsciiConfig {
            grid_width: 80,
            grid_height: Some(45),
            ..AsciiConfig::default()
        };
        let count = 300u32;
        let mut cpu = CpuAsciiBackend::new();
        cpu.process(frame.clone(), &config).unwrap();
        let start = Instant::now();
        for _ in 0..count {
            cpu.process(frame.clone(), &config).unwrap();
        }
        let cpu_wall = start.elapsed();

        let mut gpu = VulkanAsciiBackend::new().unwrap();
        gpu.process(frame.clone(), &config).unwrap();
        let start = Instant::now();
        let mut one_mapping = Duration::ZERO;
        let mut one_render = Duration::ZERO;
        let mut one_backend = Duration::ZERO;
        for _ in 0..count {
            let output = gpu.process(frame.clone(), &config).unwrap();
            one_mapping += output.timings.gpu_mapping;
            one_render += output.timings.gpu_render;
            one_backend += output.timings.backend_wall;
        }
        let one_wall = start.elapsed();

        let mut slots = PipelinedVulkanAsciiBackend::new(
            VulkanAsciiBackend::new().unwrap(),
            frame.desc().clone(),
            config.clone(),
        )
        .unwrap();
        slots.submit(frame.clone(), &config).unwrap();
        slots.drain().unwrap();
        let start = Instant::now();
        let mut two_mapping = Duration::ZERO;
        let mut two_render = Duration::ZERO;
        let mut two_backend = Duration::ZERO;
        let mut completed = 0u32;
        for _ in 0..count {
            if let Some(output) = slots.submit(frame.clone(), &config).unwrap() {
                two_mapping += output.timings.gpu_mapping;
                two_render += output.timings.gpu_render;
                two_backend += output.timings.backend_wall;
                completed += 1;
            }
        }
        while let Some(output) = slots.drain().unwrap() {
            two_mapping += output.timings.gpu_mapping;
            two_render += output.timings.gpu_render;
            two_backend += output.timings.backend_wall;
            completed += 1;
        }
        assert_eq!(completed, count);
        let two_wall = start.elapsed();
        assert_eq!(gpu.validation_error_count(), 0);
        assert_eq!(slots.validation_error_count(), 0);
        for (name, wall, mapping, render, backend) in [
            ("CPU", cpu_wall, Duration::ZERO, Duration::ZERO, cpu_wall),
            (
                "Vulkan 1-slot",
                one_wall,
                one_mapping,
                one_render,
                one_backend,
            ),
            (
                "Vulkan 2-slot",
                two_wall,
                two_mapping,
                two_render,
                two_backend,
            ),
        ] {
            eprintln!(
                "{name}: FPS={:.2}, mapping GPU={:.3} ms/frame, render GPU={:.3} ms/frame, backend wall={:.3} ms/frame, test wall={:.3} ms/frame",
                count as f64 / wall.as_secs_f64(),
                mapping.as_secs_f64() * 1000.0 / count as f64,
                render.as_secs_f64() * 1000.0 / count as f64,
                backend.as_secs_f64() * 1000.0 / count as f64,
                wall.as_secs_f64() * 1000.0 / count as f64,
            );
        }
    }

    fn assert_p010_padding(frame: &VideoFrame) {
        assert!(
            frame
                .host()
                .as_slice()
                .chunks_exact(2)
                .all(|bytes| { u16::from_le_bytes([bytes[0], bytes[1]]) & 0x3f == 0 })
        );
    }

    fn expand_nv12_to_p010_for_test(frame: &VideoFrame) -> VideoFrame {
        let desc = FrameDesc::host_p010_le(
            frame.desc().width,
            frame.desc().height,
            frame.desc().color_space,
        )
        .unwrap();
        let bytes: Vec<u8> = frame
            .host()
            .as_slice()
            .iter()
            .flat_map(|&sample| ((sample as u16) << 8).to_le_bytes())
            .collect();
        VideoFrame::new_host(
            desc.clone(),
            frame.pts(),
            HostFrame::from_p010_le(&desc, bytes).unwrap(),
        )
        .unwrap()
    }

    #[test]
    #[ignore = "requires Vulkan 1.3 with P010 16-bit storage; lavapipe is sufficient"]
    fn p010_mapping_render_and_two_slot_match_cpu() {
        let mut config = AsciiConfig {
            grid_width: 13,
            grid_height: Some(9),
            charset: "@%#*+=-:. ".into(),
            ..AsciiConfig::default()
        };
        let mut gpu = VulkanAsciiBackend::new().unwrap();
        assert!(gpu.device_info().p010_storage_supported);
        let mut cpu = CpuAsciiBackend::new();
        let nv12 = patterned_frame();
        let nv12_reference = cpu.process(nv12.clone(), &config).unwrap().frame;
        let nv12_glyphs = gpu
            .map_cells(&nv12, &config)
            .unwrap()
            .into_iter()
            .map(|cell| cell.glyph)
            .collect::<Vec<_>>();
        let p010_glyphs = gpu
            .map_cells(&expand_nv12_to_p010_for_test(&nv12), &config)
            .unwrap()
            .into_iter()
            .map(|cell| cell.glyph)
            .collect::<Vec<_>>();
        assert_eq!(nv12_glyphs, p010_glyphs);
        assert_eq!(
            gpu.process(nv12.clone(), &config).unwrap().frame,
            nv12_reference
        );
        for color in [false, true] {
            config.color = color;
            for pattern in 0..4 {
                let input = synthetic_p010_frame(pattern, pattern as i64);
                let cells = Nv12Mapper::new(&config.charset)
                    .unwrap()
                    .map(&input, 13, 9)
                    .unwrap();
                let mapped = gpu.map_cells(&input, &config).unwrap();
                assert_eq!(mapped.len(), cells.cells.len());
                for (actual, expected) in mapped.iter().zip(&cells.cells) {
                    assert_eq!(
                        (actual.glyph, actual.y, actual.u, actual.v),
                        (
                            expected.glyph as u32,
                            expected.y as u32,
                            expected.u as u32,
                            expected.v as u32
                        )
                    );
                }
                let reference = cpu.process(input.clone(), &config).unwrap().frame;
                let rendered = gpu
                    .render_cells(input.desc(), input.pts(), &config, &mapped)
                    .unwrap()
                    .frame;
                let processed = gpu.process(input, &config).unwrap().frame;
                assert_eq!(rendered, reference);
                assert_eq!(processed, reference);
                assert_p010_padding(&processed);
            }
        }
        assert_eq!(gpu.process(nv12, &config).unwrap().frame, nv12_reference);
        assert_eq!(gpu.validation_error_count(), 0);

        config.color = true;
        for frame_count in [1, 2, 3, 5] {
            let frames: Vec<_> = (0..frame_count)
                .map(|index| synthetic_p010_frame(index % 4, index as i64))
                .collect();
            let expected: Vec<_> = frames
                .iter()
                .map(|frame| cpu.process(frame.clone(), &config).unwrap().frame)
                .collect();
            let mut slots = PipelinedVulkanAsciiBackend::new(
                VulkanAsciiBackend::new().unwrap(),
                frames[0].desc().clone(),
                config.clone(),
            )
            .unwrap();
            let mut actual = Vec::new();
            for frame in frames {
                if let Some(output) = slots.submit(frame, &config).unwrap() {
                    actual.push(output.frame);
                }
            }
            while let Some(output) = slots.drain().unwrap() {
                actual.push(output.frame);
            }
            assert_eq!(actual, expected, "frame_count={frame_count}");
            assert_eq!(slots.validation_error_count(), 0);
        }
    }

    #[test]
    #[ignore = "requires Vulkan 1.3 with P010 16-bit storage and checked-in font fixture"]
    fn p010_freetype_atlas_matches_cpu() {
        let font = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/fonts/Inconsolata-Regular.ttf");
        let config = AsciiConfig {
            grid_width: 4,
            grid_height: Some(3),
            ..AsciiConfig::default()
        };
        let (atlas, _) =
            asciiflow_font::build_font_atlas(&font, 0, &config.charset, 16, 16).unwrap();
        let input = synthetic_p010_frame(1, 5);
        let reference = CpuAsciiBackend::with_atlas(atlas.clone(), &config)
            .process(input.clone(), &config)
            .unwrap()
            .frame;
        let mut gpu = VulkanAsciiBackend::new()
            .unwrap()
            .with_atlas(atlas, &config)
            .unwrap();
        let actual = gpu.process(input, &config).unwrap().frame;
        assert_eq!(actual, reference);
        assert_p010_padding(&actual);
        assert_eq!(gpu.validation_error_count(), 0);
    }

    #[test]
    #[ignore = "3000 synthetic frames require Vulkan 1.3 with P010 16-bit storage"]
    fn p010_two_slot_3000_frame_reuse_preserves_order() {
        let config = AsciiConfig {
            grid_width: 13,
            grid_height: Some(9),
            ..AsciiConfig::default()
        };
        let first = synthetic_p010_frame(0, 0);
        let mut cpu = CpuAsciiBackend::new();
        let reference: Vec<_> = (0..4)
            .map(|pattern| {
                cpu.process(synthetic_p010_frame(pattern, 0), &config)
                    .unwrap()
                    .frame
                    .into_host()
            })
            .collect();
        let mut slots = PipelinedVulkanAsciiBackend::new(
            VulkanAsciiBackend::new().unwrap(),
            first.desc().clone(),
            config.clone(),
        )
        .unwrap();
        let mut completed = 0usize;
        let mut check = |output: asciiflow_core::BackendOutput| {
            assert_eq!(output.frame.pts(), Some(completed as i64));
            assert_eq!(
                output.frame.host().as_slice(),
                reference[completed % 4].as_slice()
            );
            assert_p010_padding(&output.frame);
            completed += 1;
        };
        for index in 0..3000 {
            let frame = synthetic_p010_frame(index % 4, index as i64);
            if let Some(output) = slots.submit(frame, &config).unwrap() {
                check(output);
            }
        }
        while let Some(output) = slots.drain().unwrap() {
            check(output);
        }
        assert_eq!(completed, 3000);
        assert_eq!(slots.validation_error_count(), 0);
    }

    #[test]
    #[ignore = "requires Vulkan; lavapipe is sufficient for exact font parity"]
    fn freetype_cpu_vulkan_and_two_slot_parity() {
        let font = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/fonts/Inconsolata-Regular.ttf");
        let mut config = AsciiConfig {
            grid_width: 4,
            grid_height: Some(3),
            ..AsciiConfig::default()
        };
        let (atlas, _) =
            asciiflow_font::build_font_atlas(&font, 0, &config.charset, 16, 16).unwrap();
        for color in [false, true] {
            config.color = color;
            let input = patterned_frame();
            let expected = CpuAsciiBackend::with_atlas(atlas.clone(), &config)
                .process(input.clone(), &config)
                .unwrap();
            let mut gpu = VulkanAsciiBackend::new()
                .unwrap()
                .with_atlas(atlas.clone(), &config)
                .unwrap();
            let actual = gpu.process(input.clone(), &config).unwrap();
            assert_eq!(expected.frame, actual.frame);
            assert_eq!(gpu.validation_error_count(), 0);
            let mut changed = config.clone();
            changed.charset = config.charset.chars().rev().collect();
            assert!(
                gpu.prepare(input.desc(), &changed)
                    .unwrap_err()
                    .to_string()
                    .contains("identity")
            );
            let mut slots =
                PipelinedVulkanAsciiBackend::new(gpu, input.desc().clone(), config.clone())
                    .unwrap();
            let mut results = Vec::new();
            for _ in 0..3 {
                if let Some(output) = slots.submit(input.clone(), &config).unwrap() {
                    results.push(output);
                }
            }
            while let Some(output) = slots.drain().unwrap() {
                results.push(output);
            }
            assert_eq!(results.len(), 3);
            for output in results {
                assert_eq!(expected.frame, output.frame);
            }
        }
    }

    #[test]
    #[ignore = "requires a Vulkan 1.3 compute device; set ASCIIFLOW_VULKAN_ALLOW_CPU=1 for lavapipe"]
    fn initializes_compute_device() {
        let devices = enumerate_devices().unwrap();
        assert!(!devices.is_empty());
        let backend = VulkanAsciiBackend::new().unwrap();
        assert!(!backend.device_info().name.is_empty());
    }

    #[test]
    #[ignore = "requires a Vulkan 1.3 compute device; set ASCIIFLOW_VULKAN_ALLOW_CPU=1 for lavapipe"]
    fn mapping_matches_cpu_reference() {
        let input = patterned_frame();
        let config = AsciiConfig {
            grid_width: 13,
            grid_height: Some(9),
            charset: "@%#*+=-:. ".into(),
            font: "builtin-8x8".into(),
            color: true,
        };
        let expected = Nv12Mapper::new(&config.charset)
            .unwrap()
            .map(&input, 13, 9)
            .unwrap();
        let mut backend = VulkanAsciiBackend::new().unwrap();
        let actual = backend.map_cells(&input, &config).unwrap();
        assert_eq!(backend.validation_error_count(), 0);
        assert_eq!(actual.len(), expected.cells.len());
        for (gpu, cpu) in actual.iter().zip(&expected.cells) {
            assert_eq!(
                (gpu.glyph, gpu.y, gpu.u, gpu.v),
                (cpu.glyph as u32, cpu.y as u32, cpu.u as u32, cpu.v as u32)
            );
        }
    }

    #[test]
    #[ignore = "requires a Vulkan 1.3 compute device; set ASCIIFLOW_VULKAN_ALLOW_CPU=1 for lavapipe"]
    fn rendered_nv12_matches_cpu_reference() {
        let input = patterned_frame();
        let config = AsciiConfig {
            grid_width: 13,
            grid_height: Some(9),
            charset: "@%#*+=-:. ".into(),
            font: "builtin-8x8".into(),
            color: true,
        };
        let mut backend = VulkanAsciiBackend::new().unwrap();
        for color in [true, false] {
            let mut mode = config.clone();
            mode.color = color;
            let expected = CpuAsciiBackend::new()
                .process(input.clone(), &mode)
                .unwrap()
                .frame;
            let actual = backend.process(input.clone(), &mode).unwrap().frame;
            assert_eq!(actual, expected, "color={color}");
            let (grid_width, grid_height) = mode
                .resolved_grid(input.desc().width, input.desc().height)
                .unwrap();
            let grid = Nv12Mapper::new(&mode.charset)
                .unwrap()
                .map(&input, grid_width, grid_height)
                .unwrap();
            let cells: Vec<_> = grid
                .cells
                .iter()
                .map(|cell| GpuAsciiCell {
                    glyph: cell.glyph as u32,
                    y: cell.y as u32,
                    u: cell.u as u32,
                    v: cell.v as u32,
                })
                .collect();
            let hybrid = backend
                .render_cells(input.desc(), input.pts(), &mode, &cells)
                .unwrap()
                .frame;
            assert_eq!(hybrid, expected, "hybrid color={color}");
        }
        assert_eq!(backend.validation_error_count(), 0);
    }

    #[test]
    #[ignore = "requires the real Vulkan device and validation layer"]
    fn pipelined_backend_preserves_order_and_parity() {
        let config = AsciiConfig {
            grid_width: 13,
            grid_height: Some(9),
            charset: "@%#*+=-:. ".into(),
            font: "builtin-8x8".into(),
            color: true,
        };
        let base = patterned_frame();
        let mut inputs = Vec::new();
        let mut expected = Vec::new();
        let mut cpu = CpuAsciiBackend::new();
        let pts_values = [
            None,
            Some(5),
            Some(5),
            Some(-2),
            Some(9),
            None,
            Some(1),
            Some(1),
        ];
        for (sequence, pts) in pts_values.into_iter().enumerate() {
            let mut bytes = base.host().as_slice().to_vec();
            for (index, value) in bytes.iter_mut().enumerate() {
                *value = value.wrapping_add((sequence * 17 + index % 11) as u8);
            }
            let frame = VideoFrame::new_host(
                base.desc().clone(),
                pts,
                HostFrame::from_nv12(base.desc(), bytes).unwrap(),
            )
            .unwrap();
            expected.push(cpu.process(frame.clone(), &config).unwrap().frame);
            inputs.push(frame);
        }
        let mut first = VulkanAsciiBackend::new().unwrap();
        first.prepare(base.desc(), &config).unwrap();
        let mut backend =
            PipelinedVulkanAsciiBackend::new(first, base.desc().clone(), config.clone()).unwrap();
        let mut actual = Vec::new();
        for frame in inputs {
            if let Some(output) = backend.submit(frame, &config).unwrap() {
                actual.push(output.frame);
            }
        }
        while let Some(output) = backend.drain().unwrap() {
            actual.push(output.frame);
        }
        assert_eq!(actual, expected);
        assert_eq!(backend.validation_error_count(), 0);
    }

    #[test]
    #[ignore = "requires the real Vulkan device and validation layer"]
    fn pipelined_backend_drops_safely_with_pending_frames() {
        let input = patterned_frame();
        let config = AsciiConfig {
            grid_width: 13,
            grid_height: Some(9),
            charset: "@%#*+=-:. ".into(),
            font: "builtin-8x8".into(),
            color: true,
        };
        let mut first = VulkanAsciiBackend::new().unwrap();
        first.prepare(input.desc(), &config).unwrap();
        let context = first.shared_context();
        let mut backend =
            PipelinedVulkanAsciiBackend::new(first, input.desc().clone(), config.clone()).unwrap();
        assert!(backend.submit(input.clone(), &config).unwrap().is_none());
        assert!(backend.submit(input, &config).unwrap().is_none());
        drop(backend);
        assert_eq!(context.validation_error_count(), 0);
    }

    #[test]
    #[ignore = "real-device Stage 1.2 benchmark; validation must be disabled"]
    fn stage12_benchmark() {
        assert!(std::env::var_os("ASCIIFLOW_VULKAN_VALIDATION").is_none());
        let desc = FrameDesc::host_nv12(1920, 1080, ColorSpace::default()).unwrap();
        let mut bytes = vec![0_u8; desc.byte_len()];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = (index.wrapping_mul(37).wrapping_add(index / 1920 * 11) & 0xff) as u8;
        }
        let input = VideoFrame::new_host(
            desc.clone(),
            Some(0),
            HostFrame::from_nv12(&desc, bytes).unwrap(),
        )
        .unwrap();
        let config = AsciiConfig {
            grid_width: 80,
            grid_height: None,
            charset: "@%#*+=-:. ".into(),
            font: "builtin-8x8".into(),
            color: true,
        };
        let mut backend = VulkanAsciiBackend::new().unwrap();
        backend.prepare(&desc, &config).unwrap();
        for _ in 0..20 {
            backend.process(input.clone(), &config).unwrap();
        }
        let mut samples = Vec::with_capacity(120);
        for _ in 0..120 {
            samples.push(backend.process(input.clone(), &config).unwrap().timings);
        }
        fn median(mut values: Vec<std::time::Duration>) -> f64 {
            values.sort_unstable();
            values[values.len() / 2].as_secs_f64() * 1000.0
        }
        let field = |pick: fn(&asciiflow_core::BackendTimings) -> std::time::Duration| {
            median(samples.iter().map(pick).collect())
        };
        println!(
            "render_variant={} mapping_variant={} readback={}",
            std::env::var("ASCIIFLOW_VULKAN_RENDER_VARIANT").unwrap_or_else(|_| "lut-32x4".into()),
            backend.mapping_variant().expect("resources prepared").0,
            backend.readback_mode().expect("resources prepared")
        );
        println!(
            "median_ms gpu_upload={:.6} gpu_mapping={:.6} gpu_render={:.6} gpu_download={:.6} queue_submit={:.6} gpu_wait={:.6} invalidate={:.6} memcpy={:.6} backend_wall={:.6}",
            field(|t| t.gpu_upload),
            field(|t| t.gpu_mapping),
            field(|t| t.gpu_render),
            field(|t| t.gpu_download),
            field(|t| t.queue_submit),
            field(|t| t.gpu_wait),
            field(|t| t.host_invalidate),
            field(|t| t.host_readback),
            field(|t| t.backend_wall)
        );
        println!(
            "readback_bandwidth_gib_s={:.3}",
            desc.byte_len() as f64
                / (1024.0 * 1024.0 * 1024.0)
                / (field(|t| t.host_readback) / 1000.0)
        );
    }

    enum SlotJob {
        Process {
            sequence: usize,
            submitted: Instant,
            frame: VideoFrame,
        },
        Stop,
    }

    struct SlotResult {
        sequence: usize,
        submitted: Instant,
        output: asciiflow_core::BackendOutput,
    }

    struct BenchmarkSlot {
        jobs: Sender<SlotJob>,
        results: Receiver<std::result::Result<SlotResult, String>>,
        thread: thread::JoinHandle<std::result::Result<usize, String>>,
        buffer_bytes: u64,
    }

    fn spawn_slot(
        context: Arc<crate::context::VulkanContext>,
        desc: FrameDesc,
        config: AsciiConfig,
    ) -> BenchmarkSlot {
        let (job_tx, job_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || {
            let mut backend =
                match VulkanAsciiBackend::from_context(context).and_then(|mut backend| {
                    backend.prepare(&desc, &config)?;
                    Ok(backend)
                }) {
                    Ok(backend) => backend,
                    Err(error) => {
                        let message = error.to_string();
                        let _ = ready_tx.send(Err(message.clone()));
                        return Err(message);
                    }
                };
            let ready = (
                backend.device_info().name.clone(),
                backend.allocated_buffer_bytes().unwrap(),
            );
            ready_tx.send(Ok(ready)).unwrap();
            while let Ok(job) = job_rx.recv() {
                match job {
                    SlotJob::Process {
                        sequence,
                        submitted,
                        frame,
                    } => {
                        let output = backend
                            .process(frame, &config)
                            .map(|output| SlotResult {
                                sequence,
                                submitted,
                                output,
                            })
                            .map_err(|error| error.to_string());
                        if result_tx.send(output).is_err() {
                            break;
                        }
                    }
                    SlotJob::Stop => break,
                }
            }
            Ok(backend.validation_error_count())
        });
        let (device, buffer_bytes) = ready_rx.recv().unwrap().unwrap();
        assert!(device.contains("Intel(R) Arc(tm) Graphics (MTL)"));
        BenchmarkSlot {
            jobs: job_tx,
            results: result_rx,
            thread,
            buffer_bytes,
        }
    }

    struct BatchSamples {
        elapsed: Duration,
        latency: Vec<Duration>,
        timings: Vec<asciiflow_core::BackendTimings>,
    }

    fn run_batch(slots: &[BenchmarkSlot], frames: Vec<VideoFrame>) -> BatchSamples {
        let frame_count = frames.len();
        let mut frames = frames.into_iter();
        let started = Instant::now();
        let mut submitted = 0;
        for (slot_index, frame) in frames.by_ref().take(slots.len()).enumerate() {
            slots[slot_index]
                .jobs
                .send(SlotJob::Process {
                    sequence: submitted,
                    submitted: Instant::now(),
                    frame,
                })
                .unwrap();
            submitted += 1;
        }
        let mut latency = Vec::with_capacity(frame_count);
        let mut timings = Vec::with_capacity(frame_count);
        for expected in 0..frame_count {
            let slot = &slots[expected % slots.len()];
            let result = slot.results.recv().unwrap().unwrap();
            assert_eq!(result.sequence, expected);
            assert_eq!(result.output.frame.pts(), Some(expected as i64));
            latency.push(result.submitted.elapsed());
            timings.push(result.output.timings);
            if let Some(frame) = frames.next() {
                slot.jobs
                    .send(SlotJob::Process {
                        sequence: submitted,
                        submitted: Instant::now(),
                        frame,
                    })
                    .unwrap();
                submitted += 1;
            }
        }
        assert_eq!(submitted, frame_count);
        BatchSamples {
            elapsed: started.elapsed(),
            latency,
            timings,
        }
    }

    fn benchmark_frames(base: &VideoFrame, count: usize) -> Vec<VideoFrame> {
        (0..count)
            .map(|sequence| {
                VideoFrame::new_host(
                    base.desc().clone(),
                    Some(sequence as i64),
                    HostFrame::from_nv12(base.desc(), base.host().as_slice().to_vec()).unwrap(),
                )
                .unwrap()
            })
            .collect()
    }

    fn percentile_ms(mut values: Vec<Duration>, percentile: usize) -> f64 {
        values.sort_unstable();
        let index = (values.len() - 1) * percentile / 100;
        values[index].as_secs_f64() * 1000.0
    }

    fn duration_median_ms(mut values: Vec<Duration>) -> f64 {
        values.sort_unstable();
        values[values.len() / 2].as_secs_f64() * 1000.0
    }

    #[test]
    #[ignore = "real Intel Arc Stage 1.3 isolated one-slot/two-slot throughput experiment"]
    fn stage13_inflight_benchmark() {
        assert!(std::env::var_os("ASCIIFLOW_VULKAN_VALIDATION").is_none());
        for name in [
            "ASCIIFLOW_VULKAN_MAP_VARIANT",
            "ASCIIFLOW_VULKAN_RENDER_VARIANT",
            "ASCIIFLOW_VULKAN_READBACK",
        ] {
            assert!(
                std::env::var_os(name).is_none(),
                "{name} would mislabel this benchmark"
            );
        }
        let base = {
            let desc = FrameDesc::host_nv12(1920, 1080, ColorSpace::default()).unwrap();
            let mut bytes = vec![0_u8; desc.byte_len()];
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte = (index.wrapping_mul(37).wrapping_add(index / 1920 * 11) & 0xff) as u8;
            }
            VideoFrame::new_host(
                desc.clone(),
                Some(0),
                HostFrame::from_nv12(&desc, bytes).unwrap(),
            )
            .unwrap()
        };
        let config = AsciiConfig {
            grid_width: 80,
            grid_height: None,
            charset: "@%#*+=-:. ".into(),
            font: "builtin-8x8".into(),
            color: true,
        };
        println!("frames=120 warmup=20 mapping=u32-32 render=lut-32x4 readback=cached");
        println!(
            "slots fps latency_median_ms latency_p95_ms backend_wall_ms gpu_wait_ms gpu_busy_ms gpu_upload_ms gpu_mapping_ms gpu_render_ms gpu_download_ms buffer_mib"
        );
        for slot_count in [1, 2] {
            let context = Arc::new(crate::context::VulkanContext::new().unwrap());
            let slots: Vec<_> = (0..slot_count)
                .map(|_| spawn_slot(context.clone(), base.desc().clone(), config.clone()))
                .collect();
            std::hint::black_box(run_batch(&slots, benchmark_frames(&base, 20)));
            let samples = run_batch(&slots, benchmark_frames(&base, 120));
            let field = |pick: fn(&asciiflow_core::BackendTimings) -> Duration| {
                duration_median_ms(samples.timings.iter().map(pick).collect::<Vec<Duration>>())
            };
            println!(
                "{slot_count} {:.3} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.3}",
                120.0 / samples.elapsed.as_secs_f64(),
                percentile_ms(samples.latency.clone(), 50),
                percentile_ms(samples.latency, 95),
                field(|timings| timings.backend_wall),
                field(|timings| timings.gpu_wait),
                field(|timings| timings.gpu_busy),
                field(|timings| timings.gpu_upload),
                field(|timings| timings.gpu_mapping),
                field(|timings| timings.gpu_render),
                field(|timings| timings.gpu_download),
                slots.iter().map(|slot| slot.buffer_bytes).sum::<u64>() as f64 / (1024.0 * 1024.0),
            );
            for slot in &slots {
                slot.jobs.send(SlotJob::Stop).unwrap();
            }
            for slot in slots {
                assert_eq!(slot.thread.join().unwrap().unwrap(), 0);
            }
        }
    }
}
