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
