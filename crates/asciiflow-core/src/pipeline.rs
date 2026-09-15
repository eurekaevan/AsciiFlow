use crate::{
    AsciiBackend, AsciiConfig, BackendOutput, CancellationToken, Error, FrameSink, FrameSource,
    MetricStage, Metrics, MetricsSnapshot, PipelineStage, Result, VideoFrame,
};
use crossbeam_channel::{RecvTimeoutError, SendTimeoutError, Sender, bounded};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const POLL: Duration = Duration::from_millis(20);

struct DecodedFrame {
    frame: VideoFrame,
    started_at: Instant,
}

struct ProcessedFrame {
    frame: VideoFrame,
    started_at: Instant,
}

pub struct Pipeline {
    capacity: usize,
}
#[derive(Debug)]
pub struct PipelineReport {
    pub metrics: MetricsSnapshot,
}

fn forward_output(
    output: BackendOutput,
    started_at: Instant,
    metrics: &Metrics,
    processed: &Sender<ProcessedFrame>,
    cancellation: &CancellationToken,
) -> bool {
    metrics.record(MetricStage::Mapping, output.timings.mapping);
    metrics.record(MetricStage::Render, output.timings.render);
    metrics.record(MetricStage::HostUpload, output.timings.host_upload);
    metrics.record(MetricStage::QueueSubmit, output.timings.queue_submit);
    metrics.record(MetricStage::GpuUpload, output.timings.gpu_upload);
    metrics.record(MetricStage::DrmPrimeMap, output.timings.drm_prime_map);
    metrics.record(
        MetricStage::ExternalCapabilityQuery,
        output.timings.external_capability_query,
    );
    metrics.record(
        MetricStage::ExternalImageCreate,
        output.timings.external_image_create,
    );
    metrics.record(
        MetricStage::ExternalMemoryImport,
        output.timings.external_memory_import,
    );
    metrics.record(
        MetricStage::ExternalMemoryBind,
        output.timings.external_memory_bind,
    );
    metrics.record(
        MetricStage::ExternalOwnership,
        output.timings.external_ownership,
    );
    metrics.record(
        MetricStage::ExternalImageDestroy,
        output.timings.external_image_destroy,
    );
    metrics.record(
        MetricStage::GpuExternalCopy,
        output.timings.gpu_external_copy,
    );
    metrics.record(MetricStage::GpuMapping, output.timings.gpu_mapping);
    metrics.record(MetricStage::GpuRender, output.timings.gpu_render);
    metrics.record(MetricStage::GpuDownload, output.timings.gpu_download);
    metrics.record(MetricStage::GpuBusy, output.timings.gpu_busy);
    metrics.record(MetricStage::GpuWait, output.timings.gpu_wait);
    metrics.record(MetricStage::HostReadback, output.timings.host_readback);
    metrics.record(MetricStage::HostInvalidate, output.timings.host_invalidate);
    metrics.record(MetricStage::BackendWall, output.timings.backend_wall);
    let mut frame = ProcessedFrame {
        frame: output.frame,
        started_at,
    };
    loop {
        match processed.send_timeout(frame, POLL) {
            Ok(()) => return true,
            Err(SendTimeoutError::Timeout(pending)) if !cancellation.is_cancelled() => {
                frame = pending;
            }
            Err(_) => return false,
        }
    }
}

fn drain_backend(
    backend: &mut dyn AsciiBackend,
    metrics: &Metrics,
    processed: &Sender<ProcessedFrame>,
    cancellation: &CancellationToken,
    pending_started: &mut VecDeque<Instant>,
) -> Result<()> {
    while let Some(output) = backend.drain()? {
        let started_at = pending_started
            .pop_front()
            .ok_or_else(|| Error::Cpu("backend drained a frame that was never submitted".into()))?;
        if !forward_output(output, started_at, metrics, processed, cancellation) {
            return Err(Error::Cancelled);
        }
    }
    if !pending_started.is_empty() {
        return Err(Error::Cpu(format!(
            "backend retained {} frame(s) after drain completed",
            pending_started.len()
        )));
    }
    Ok(())
}

impl Pipeline {
    pub fn new(capacity: usize) -> Result<Self> {
        if capacity == 0 {
            return Err(Error::InvalidConfig(
                "pipeline capacity must be non-zero".into(),
            ));
        }
        Ok(Self { capacity })
    }

    pub fn run<S, B, E>(
        &self,
        source: S,
        backend: B,
        sink: E,
        config: AsciiConfig,
    ) -> Result<PipelineReport>
    where
        S: FrameSource + 'static,
        B: AsciiBackend + 'static,
        E: FrameSink + 'static,
    {
        self.run_with_cancellation(source, backend, sink, config, CancellationToken::new())
    }

    pub fn run_with_cancellation<S, B, E>(
        &self,
        mut source: S,
        mut backend: B,
        mut sink: E,
        config: AsciiConfig,
        cancellation: CancellationToken,
    ) -> Result<PipelineReport>
    where
        S: FrameSource + 'static,
        B: AsciiBackend + 'static,
        E: FrameSink + 'static,
    {
        let started = Instant::now();
        let metrics = Metrics::default();
        let first_failure = Arc::new(Mutex::new(None::<Error>));
        let (decoded_tx, decoded_rx) = bounded::<DecodedFrame>(self.capacity);
        let (processed_tx, processed_rx) = bounded::<ProcessedFrame>(self.capacity);

        std::thread::scope(|scope| {
            let cancel = cancellation.clone();
            let failure = first_failure.clone();
            let m = metrics.clone();
            let decoder_thread = scope.spawn(move || {
                while !cancel.is_cancelled() {
                    let begin = Instant::now();
                    match source.next_frame() {
                        Ok(Some(frame)) => {
                            m.record(MetricStage::Decode, begin.elapsed());
                            let timings = source.take_timings();
                            m.record(MetricStage::DecodePacketSubmit, timings.packet_submit);
                            m.record(MetricStage::DecodeFrameReceive, timings.frame_receive);
                            m.record(MetricStage::HardwareDownload, timings.hardware_download);
                            let mut pending = DecodedFrame {
                                frame,
                                started_at: begin,
                            };
                            loop {
                                match decoded_tx.send_timeout(pending, POLL) {
                                    Ok(()) => break,
                                    Err(SendTimeoutError::Timeout(frame))
                                        if !cancel.is_cancelled() =>
                                    {
                                        pending = frame
                                    }
                                    Err(_) => return,
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(error) => {
                            record_failure(
                                &failure,
                                &cancel,
                                Error::pipeline(
                                    PipelineStage::DecodeRuntime,
                                    "read decoded frame",
                                    error,
                                ),
                            );
                            break;
                        }
                    }
                }
            });

            let cancel = cancellation.clone();
            let failure = first_failure.clone();
            let m = metrics.clone();
            let processor_thread = scope.spawn(move || {
                let mut pending_started = VecDeque::new();
                loop {
                    match decoded_rx.recv_timeout(POLL) {
                        Ok(frame) => {
                            pending_started.push_back(frame.started_at);
                            match backend.submit(frame.frame, &config) {
                                Ok(Some(output)) => {
                                    let started_at = pending_started
                                        .pop_front()
                                        .expect("submitted frame timestamp missing");
                                    if !forward_output(
                                        output,
                                        started_at,
                                        &m,
                                        &processed_tx,
                                        &cancel,
                                    ) {
                                        return;
                                    }
                                }
                                Ok(None) => {}
                                Err(error) => {
                                    record_failure(
                                        &failure,
                                        &cancel,
                                        Error::pipeline(
                                            PipelineStage::ProcessingRuntime,
                                            "process video frame",
                                            error,
                                        ),
                                    );
                                    return;
                                }
                            }
                        }
                        Err(RecvTimeoutError::Timeout) if !cancel.is_cancelled() => {
                            continue;
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            if cancel.is_cancelled() {
                                return;
                            }
                            if let Err(error) = drain_backend(
                                &mut backend,
                                &m,
                                &processed_tx,
                                &cancel,
                                &mut pending_started,
                            ) {
                                if cancel.is_cancelled() {
                                    return;
                                }
                                record_failure(
                                    &failure,
                                    &cancel,
                                    Error::pipeline(PipelineStage::Drain, "drain processor", error),
                                );
                            }
                            return;
                        }
                        Err(_) => return,
                    }
                }
            });

            let cancel = cancellation.clone();
            let failure = first_failure.clone();
            let m = metrics.clone();
            let encoder_thread = scope.spawn(move || {
                loop {
                    match processed_rx.recv_timeout(POLL) {
                        Ok(frame) => {
                            let begin = Instant::now();
                            if let Err(error) = sink.encode(frame.frame) {
                                record_failure(
                                    &failure,
                                    &cancel,
                                    Error::pipeline(
                                        PipelineStage::EncodeRuntime,
                                        "encode video frame",
                                        error,
                                    ),
                                );
                                return;
                            }
                            m.record(MetricStage::Encode, begin.elapsed());
                            let timings = sink.take_timings();
                            m.record(MetricStage::HardwareUpload, timings.hardware_upload);
                            m.record(MetricStage::EncodeSubmitReceive, timings.submit_receive);
                            m.record(MetricStage::PipelineLatency, frame.started_at.elapsed());
                            m.frame_completed();
                        }
                        Err(RecvTimeoutError::Timeout) if !cancel.is_cancelled() => {
                            continue;
                        }
                        Err(RecvTimeoutError::Disconnected) => break,
                        Err(_) => return,
                    }
                }
                if !cancel.is_cancelled() {
                    let begin = Instant::now();
                    let finish_result = sink.finish();
                    m.record(MetricStage::Encode, begin.elapsed());
                    if let Err(error) = finish_result {
                        record_failure(
                            &failure,
                            &cancel,
                            Error::pipeline(
                                PipelineStage::Finalization,
                                "finalize encoder and muxer",
                                error,
                            ),
                        );
                    } else {
                        let timings = sink.take_timings();
                        m.record(MetricStage::HardwareUpload, timings.hardware_upload);
                        m.record(MetricStage::EncodeSubmitReceive, timings.submit_receive);
                    }
                }
            });
            for (stage, operation, thread) in [
                (
                    PipelineStage::DecodeRuntime,
                    "join decoder worker",
                    decoder_thread,
                ),
                (
                    PipelineStage::ProcessingRuntime,
                    "join processor worker",
                    processor_thread,
                ),
                (
                    PipelineStage::EncodeRuntime,
                    "join encoder worker",
                    encoder_thread,
                ),
            ] {
                if thread.join().is_err() {
                    record_failure(
                        &first_failure,
                        &cancellation,
                        Error::pipeline_message(stage, operation, "worker thread panicked"),
                    );
                }
            }
        });
        metrics.set_total(started.elapsed());
        if let Some(error) = take_failure(&first_failure) {
            return Err(error);
        }
        if cancellation.is_cancelled() {
            return Err(Error::Cancelled);
        }
        Ok(PipelineReport {
            metrics: metrics.snapshot(),
        })
    }
}

fn record_failure(
    first_failure: &Mutex<Option<Error>>,
    cancellation: &CancellationToken,
    error: Error,
) {
    let mut first = first_failure
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if first.is_none() {
        *first = Some(error);
    }
    cancellation.cancel();
}

fn take_failure(first_failure: &Mutex<Option<Error>>) -> Option<Error> {
    first_failure
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackendOutput, BackendTimings, ColorSpace, FrameDesc, HostFrame};
    use std::{
        collections::VecDeque,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
    };

    struct Source {
        next: i64,
        end: i64,
    }
    impl FrameSource for Source {
        fn next_frame(&mut self) -> Result<Option<VideoFrame>> {
            if self.next == self.end {
                return Ok(None);
            }
            let desc = FrameDesc::host_nv12(2, 2, ColorSpace::default())?;
            let frame =
                VideoFrame::new_host(desc.clone(), Some(self.next), HostFrame::new_zeroed(&desc))?;
            self.next += 1;
            Ok(Some(frame))
        }
    }
    struct Backend;
    impl AsciiBackend for Backend {
        fn process(&mut self, frame: VideoFrame, _: &AsciiConfig) -> Result<BackendOutput> {
            Ok(BackendOutput {
                frame,
                timings: BackendTimings {
                    mapping: Duration::from_millis(1),
                    render: Duration::from_millis(2),
                    host_upload: Duration::from_millis(3),
                    queue_submit: Duration::from_millis(4),
                    gpu_upload: Duration::from_millis(5),
                    gpu_mapping: Duration::from_millis(6),
                    gpu_render: Duration::from_millis(7),
                    gpu_download: Duration::from_millis(8),
                    gpu_busy: Duration::from_millis(13),
                    gpu_wait: Duration::from_millis(9),
                    host_invalidate: Duration::from_millis(12),
                    host_readback: Duration::from_millis(10),
                    backend_wall: Duration::from_millis(11),
                    ..BackendTimings::default()
                },
            })
        }
    }
    struct Sink(Arc<Mutex<Vec<i64>>>);
    impl FrameSink for Sink {
        fn encode(&mut self, frame: VideoFrame) -> Result<()> {
            self.0.lock().unwrap().push(frame.pts().unwrap());
            Ok(())
        }
        fn finish(&mut self) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn preserves_order_and_completes() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let report = Pipeline::new(3)
            .unwrap()
            .run(
                Source { next: 0, end: 8 },
                Backend,
                Sink(seen.clone()),
                AsciiConfig::default(),
            )
            .unwrap();
        assert_eq!(*seen.lock().unwrap(), (0..8).collect::<Vec<_>>());
        assert_eq!(report.metrics.frames, 8);
        assert_eq!(report.metrics.mapping, Duration::from_millis(8));
        assert_eq!(report.metrics.render, Duration::from_millis(16));
        assert_eq!(report.metrics.host_upload, Duration::from_millis(24));
        assert_eq!(report.metrics.queue_submit, Duration::from_millis(32));
        assert_eq!(report.metrics.gpu_upload, Duration::from_millis(40));
        assert_eq!(report.metrics.gpu_mapping, Duration::from_millis(48));
        assert_eq!(report.metrics.gpu_render, Duration::from_millis(56));
        assert_eq!(report.metrics.gpu_download, Duration::from_millis(64));
        assert_eq!(report.metrics.gpu_busy, Duration::from_millis(104));
        assert_eq!(report.metrics.gpu_wait, Duration::from_millis(72));
        assert_eq!(report.metrics.host_readback, Duration::from_millis(80));
        assert_eq!(report.metrics.host_invalidate, Duration::from_millis(96));
        assert_eq!(report.metrics.backend_wall, Duration::from_millis(88));
    }

    #[derive(Default)]
    struct BufferedBackend {
        pending: VecDeque<BackendOutput>,
    }
    impl AsciiBackend for BufferedBackend {
        fn process(&mut self, _: VideoFrame, _: &AsciiConfig) -> Result<BackendOutput> {
            Err(Error::Cpu("buffered backend requires submit/drain".into()))
        }
        fn submit(&mut self, frame: VideoFrame, _: &AsciiConfig) -> Result<Option<BackendOutput>> {
            let completed = if self.pending.len() == 2 {
                self.pending.pop_front()
            } else {
                None
            };
            self.pending.push_back(BackendOutput {
                frame,
                timings: BackendTimings::default(),
            });
            Ok(completed)
        }
        fn drain(&mut self) -> Result<Option<BackendOutput>> {
            Ok(self.pending.pop_front())
        }
    }

    #[test]
    fn drains_buffered_backend_in_submission_order() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let report = Pipeline::new(3)
            .unwrap()
            .run(
                Source { next: 0, end: 8 },
                BufferedBackend::default(),
                Sink(seen.clone()),
                AsciiConfig::default(),
            )
            .unwrap();
        assert_eq!(*seen.lock().unwrap(), (0..8).collect::<Vec<_>>());
        assert_eq!(report.metrics.frames, 8);
    }

    #[test]
    fn buffered_contract_handles_zero_one_two_frames_and_repeated_drain() {
        let config = AsciiConfig::default();
        let mut backend = BufferedBackend::default();
        assert!(backend.drain().unwrap().is_none());
        assert!(backend.drain().unwrap().is_none());
        let mut source = Source { next: 0, end: 2 };
        let first = source.next_frame().unwrap().unwrap();
        let second = source.next_frame().unwrap().unwrap();
        assert!(backend.submit(first, &config).unwrap().is_none());
        assert!(backend.submit(second, &config).unwrap().is_none());
        assert_eq!(backend.drain().unwrap().unwrap().frame.pts(), Some(0));
        assert_eq!(backend.drain().unwrap().unwrap().frame.pts(), Some(1));
        assert!(backend.drain().unwrap().is_none());
        assert!(backend.drain().unwrap().is_none());
    }

    struct DrainFailBackend;
    impl AsciiBackend for DrainFailBackend {
        fn process(&mut self, _: VideoFrame, _: &AsciiConfig) -> Result<BackendOutput> {
            unreachable!()
        }
        fn submit(&mut self, _: VideoFrame, _: &AsciiConfig) -> Result<Option<BackendOutput>> {
            Ok(None)
        }
        fn drain(&mut self) -> Result<Option<BackendOutput>> {
            Err(Error::Cpu("injected drain failure".into()))
        }
    }

    #[test]
    fn drain_failure_is_reported() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let result = Pipeline::new(1).unwrap().run(
            Source { next: 0, end: 1 },
            DrainFailBackend,
            Sink(seen),
            AsciiConfig::default(),
        );
        assert!(matches!(
            result,
            Err(Error::Pipeline {
                stage: PipelineStage::Drain,
                ..
            })
        ));
    }

    #[test]
    fn cancellation_during_drain_is_terminal_and_joins_workers() {
        struct CancellingDrainBackend(CancellationToken);
        impl AsciiBackend for CancellingDrainBackend {
            fn process(&mut self, _: VideoFrame, _: &AsciiConfig) -> Result<BackendOutput> {
                unreachable!()
            }
            fn submit(&mut self, _: VideoFrame, _: &AsciiConfig) -> Result<Option<BackendOutput>> {
                Ok(None)
            }
            fn drain(&mut self) -> Result<Option<BackendOutput>> {
                self.0.cancel();
                Err(Error::Cancelled)
            }
        }

        let token = CancellationToken::new();
        let result = Pipeline::new(1).unwrap().run_with_cancellation(
            Source { next: 0, end: 1 },
            CancellingDrainBackend(token.clone()),
            Sink(Arc::new(Mutex::new(Vec::new()))),
            AsciiConfig::default(),
            token,
        );
        assert!(matches!(result, Err(Error::Cancelled)));
    }

    struct TrackedBufferedBackend {
        inner: BufferedBackend,
        dropped: Arc<AtomicBool>,
    }
    impl AsciiBackend for TrackedBufferedBackend {
        fn process(&mut self, _: VideoFrame, _: &AsciiConfig) -> Result<BackendOutput> {
            unreachable!()
        }
        fn submit(
            &mut self,
            frame: VideoFrame,
            config: &AsciiConfig,
        ) -> Result<Option<BackendOutput>> {
            self.inner.submit(frame, config)
        }
        fn drain(&mut self) -> Result<Option<BackendOutput>> {
            self.inner.drain()
        }
    }
    impl Drop for TrackedBufferedBackend {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::Release);
        }
    }

    struct FailingSink;
    impl FrameSink for FailingSink {
        fn encode(&mut self, _: VideoFrame) -> Result<()> {
            Err(Error::Cpu("injected encoder failure".into()))
        }
        fn finish(&mut self) -> Result<()> {
            panic!("finish must not run after encoder failure")
        }
    }

    #[test]
    fn encoder_failure_drops_backend_with_pending_frames() {
        let dropped = Arc::new(AtomicBool::new(false));
        let result = Pipeline::new(3).unwrap().run(
            Source { next: 0, end: 100 },
            TrackedBufferedBackend {
                inner: BufferedBackend::default(),
                dropped: dropped.clone(),
            },
            FailingSink,
            AsciiConfig::default(),
        );
        assert!(matches!(
            result,
            Err(Error::Pipeline {
                stage: PipelineStage::EncodeRuntime,
                ..
            })
        ));
        assert!(dropped.load(Ordering::Acquire));
    }

    struct FailingBackend;
    impl AsciiBackend for FailingBackend {
        fn process(&mut self, _: VideoFrame, _: &AsciiConfig) -> Result<BackendOutput> {
            Err(Error::Cpu("injected".into()))
        }
    }
    #[test]
    fn stage_failure_cancels_without_deadlock() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let result = Pipeline::new(1).unwrap().run(
            Source { next: 0, end: 100 },
            FailingBackend,
            Sink(seen),
            AsciiConfig::default(),
        );
        assert!(matches!(
            result,
            Err(Error::Pipeline {
                stage: PipelineStage::ProcessingRuntime,
                ..
            })
        ));
    }

    #[test]
    fn cancellation_before_first_frame_joins_every_stage() {
        struct CancelSource(CancellationToken);
        impl FrameSource for CancelSource {
            fn next_frame(&mut self) -> Result<Option<VideoFrame>> {
                self.0.cancel();
                Ok(None)
            }
        }

        let cancellation = CancellationToken::new();
        let result = Pipeline::new(1).unwrap().run_with_cancellation(
            CancelSource(cancellation.clone()),
            Backend,
            Sink(Arc::new(Mutex::new(Vec::new()))),
            AsciiConfig::default(),
            cancellation,
        );
        assert!(matches!(result, Err(Error::Cancelled)));
    }

    #[test]
    fn zero_short_and_odd_frame_counts_drain_without_loss_or_reordering() {
        for count in [0, 1, 2, 3, 5, 49, 301] {
            let seen = Arc::new(Mutex::new(Vec::new()));
            let report = Pipeline::new(3)
                .unwrap()
                .run(
                    Source {
                        next: 0,
                        end: count,
                    },
                    BufferedBackend::default(),
                    Sink(seen.clone()),
                    AsciiConfig::default(),
                )
                .unwrap();
            assert_eq!(*seen.lock().unwrap(), (0..count).collect::<Vec<_>>());
            assert_eq!(report.metrics.frames, count as u64);
        }
    }

    #[test]
    fn original_processing_failure_is_not_replaced_by_shutdown_noise() {
        struct RootFailure;
        impl AsciiBackend for RootFailure {
            fn process(&mut self, _: VideoFrame, _: &AsciiConfig) -> Result<BackendOutput> {
                Err(Error::DeviceLost("injected VK_ERROR_DEVICE_LOST".into()))
            }
        }

        let error = Pipeline::new(1)
            .unwrap()
            .run(
                Source { next: 0, end: 100 },
                RootFailure,
                Sink(Arc::new(Mutex::new(Vec::new()))),
                AsciiConfig::default(),
            )
            .unwrap_err();
        let Error::Pipeline { stage, error } = error else {
            panic!("expected a structured pipeline error")
        };
        assert_eq!(stage, PipelineStage::ProcessingRuntime);
        assert!(matches!(error.source_error(), Error::DeviceLost(_)));
    }

    #[test]
    fn injected_interop_and_vulkan_runtime_failures_are_terminal_and_keep_context() {
        #[derive(Clone, Copy)]
        enum Fault {
            InputImport,
            Submit,
            Wait,
            DeviceLost,
            OutputImport,
        }
        struct RuntimeFaultBackend(Fault);
        impl AsciiBackend for RuntimeFaultBackend {
            fn process(&mut self, _: VideoFrame, _: &AsciiConfig) -> Result<BackendOutput> {
                Err(match self.0 {
                    Fault::InputImport => Error::pipeline_message(
                        PipelineStage::InputInteropRuntime,
                        "import input DMA-BUF",
                        "injected external import failure after several frames",
                    ),
                    Fault::Submit => Error::pipeline_message(
                        PipelineStage::ProcessingRuntime,
                        "submit Vulkan command buffer",
                        "injected VK_ERROR_OUT_OF_DEVICE_MEMORY",
                    ),
                    Fault::Wait => Error::pipeline_message(
                        PipelineStage::ProcessingRuntime,
                        "wait for Vulkan fence",
                        "injected fence wait failure",
                    ),
                    Fault::DeviceLost => Error::DeviceLost("injected VK_ERROR_DEVICE_LOST".into()),
                    Fault::OutputImport => Error::pipeline_message(
                        PipelineStage::OutputInteropRuntime,
                        "import encoder DMA-BUF",
                        "injected VK_ERROR_INVALID_EXTERNAL_HANDLE",
                    ),
                })
            }
        }

        let cases = [
            (Fault::InputImport, PipelineStage::InputInteropRuntime),
            (Fault::Submit, PipelineStage::ProcessingRuntime),
            (Fault::Wait, PipelineStage::ProcessingRuntime),
            (Fault::DeviceLost, PipelineStage::ProcessingRuntime),
            (Fault::OutputImport, PipelineStage::OutputInteropRuntime),
        ];
        for (fault, expected_stage) in cases {
            let error = Pipeline::new(1)
                .unwrap()
                .run(
                    Source { next: 0, end: 10 },
                    RuntimeFaultBackend(fault),
                    Sink(Arc::new(Mutex::new(Vec::new()))),
                    AsciiConfig::default(),
                )
                .unwrap_err();
            assert_eq!(error.stage(), Some(expected_stage));
            assert!(error.to_string().contains("injected"));
        }
    }

    struct IndexedFailSource {
        next: i64,
        end: i64,
        fail_at: i64,
    }
    impl FrameSource for IndexedFailSource {
        fn next_frame(&mut self) -> Result<Option<VideoFrame>> {
            if self.next == self.fail_at {
                return Err(Error::Media(format!(
                    "injected decoder failure at frame {}",
                    self.next
                )));
            }
            if self.next == self.end {
                return Ok(None);
            }
            let desc = FrameDesc::host_nv12(2, 2, ColorSpace::default())?;
            let frame =
                VideoFrame::new_host(desc.clone(), Some(self.next), HostFrame::new_zeroed(&desc))?;
            self.next += 1;
            Ok(Some(frame))
        }
    }

    #[test]
    fn decoder_runtime_failures_are_structured_at_early_middle_and_near_eof() {
        for fail_at in [1, 50, 99] {
            let error = Pipeline::new(3)
                .unwrap()
                .run(
                    IndexedFailSource {
                        next: 0,
                        end: 100,
                        fail_at,
                    },
                    Backend,
                    Sink(Arc::new(Mutex::new(Vec::new()))),
                    AsciiConfig::default(),
                )
                .unwrap_err();
            assert_eq!(error.stage(), Some(PipelineStage::DecodeRuntime));
            assert!(error.to_string().contains(&format!("frame {fail_at}")));
        }
    }

    struct IndexedFailSink {
        next: usize,
        fail_at: Option<usize>,
        fail_finish: bool,
    }
    impl FrameSink for IndexedFailSink {
        fn encode(&mut self, _: VideoFrame) -> Result<()> {
            if self.fail_at == Some(self.next) {
                return Err(Error::Media(format!(
                    "injected encoder send/receive failure at frame {}",
                    self.next
                )));
            }
            self.next += 1;
            Ok(())
        }

        fn finish(&mut self) -> Result<()> {
            if self.fail_finish {
                Err(Error::Media(
                    "injected encoder drain/trailer failure".into(),
                ))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn encoder_runtime_and_finalization_failures_keep_their_stage() {
        for fail_at in [0, 25, 49] {
            let error = Pipeline::new(3)
                .unwrap()
                .run(
                    Source { next: 0, end: 50 },
                    Backend,
                    IndexedFailSink {
                        next: 0,
                        fail_at: Some(fail_at),
                        fail_finish: false,
                    },
                    AsciiConfig::default(),
                )
                .unwrap_err();
            assert_eq!(error.stage(), Some(PipelineStage::EncodeRuntime));
        }
        let error = Pipeline::new(3)
            .unwrap()
            .run(
                Source { next: 0, end: 3 },
                Backend,
                IndexedFailSink {
                    next: 0,
                    fail_at: None,
                    fail_finish: true,
                },
                AsciiConfig::default(),
            )
            .unwrap_err();
        assert_eq!(error.stage(), Some(PipelineStage::Finalization));
        assert!(error.to_string().contains("drain/trailer"));
    }

    #[test]
    fn native_mux_failure_is_not_relabelled_as_encoder_failure() {
        struct MuxFailSink;
        impl FrameSink for MuxFailSink {
            fn encode(&mut self, _: VideoFrame) -> Result<()> {
                Err(Error::pipeline_message(
                    PipelineStage::MuxRuntime,
                    "write encoded packet",
                    "injected av_interleaved_write_frame failure",
                ))
            }
            fn finish(&mut self) -> Result<()> {
                unreachable!()
            }
        }

        let error = Pipeline::new(1)
            .unwrap()
            .run(
                Source { next: 0, end: 1 },
                Backend,
                MuxFailSink,
                AsciiConfig::default(),
            )
            .unwrap_err();
        assert_eq!(error.stage(), Some(PipelineStage::MuxRuntime));
        assert!(error.to_string().contains("av_interleaved_write_frame"));
    }

    #[test]
    fn cancellation_drops_one_or_two_buffered_slots_without_drain() {
        struct CancellingSource {
            inner: Source,
            cancel_at: i64,
            token: CancellationToken,
        }
        impl FrameSource for CancellingSource {
            fn next_frame(&mut self) -> Result<Option<VideoFrame>> {
                if self.inner.next == self.cancel_at {
                    self.token.cancel();
                    return Ok(None);
                }
                self.inner.next_frame()
            }
        }

        for pending in [1, 2] {
            let token = CancellationToken::new();
            let dropped = Arc::new(AtomicBool::new(false));
            let result = Pipeline::new(3).unwrap().run_with_cancellation(
                CancellingSource {
                    inner: Source { next: 0, end: 20 },
                    cancel_at: pending,
                    token: token.clone(),
                },
                TrackedBufferedBackend {
                    inner: BufferedBackend::default(),
                    dropped: dropped.clone(),
                },
                IndexedFailSink {
                    next: 0,
                    fail_at: None,
                    fail_finish: false,
                },
                AsciiConfig::default(),
                token,
            );
            assert!(matches!(result, Err(Error::Cancelled)));
            assert!(dropped.load(Ordering::Acquire));
        }
    }

    #[test]
    fn unusual_timestamps_do_not_panic_overflow_or_reorder() {
        struct TimestampSource(VecDeque<Option<i64>>);
        impl FrameSource for TimestampSource {
            fn next_frame(&mut self) -> Result<Option<VideoFrame>> {
                let Some(pts) = self.0.pop_front() else {
                    return Ok(None);
                };
                let desc = FrameDesc::host_nv12(2, 2, ColorSpace::default())?;
                VideoFrame::new_host(desc.clone(), pts, HostFrame::new_zeroed(&desc)).map(Some)
            }
        }
        struct TimestampSink(Arc<Mutex<Vec<Option<i64>>>>);
        impl FrameSink for TimestampSink {
            fn encode(&mut self, frame: VideoFrame) -> Result<()> {
                self.0.lock().unwrap().push(frame.pts());
                Ok(())
            }
            fn finish(&mut self) -> Result<()> {
                Ok(())
            }
        }

        let timestamps = vec![
            Some(-9_000),
            Some(0),
            Some(0),
            None,
            Some(7),
            Some(i64::MAX - 1),
        ];
        let seen = Arc::new(Mutex::new(Vec::new()));
        Pipeline::new(2)
            .unwrap()
            .run(
                TimestampSource(timestamps.clone().into()),
                BufferedBackend::default(),
                TimestampSink(seen.clone()),
                AsciiConfig::default(),
            )
            .unwrap();
        assert_eq!(*seen.lock().unwrap(), timestamps);
    }
}
