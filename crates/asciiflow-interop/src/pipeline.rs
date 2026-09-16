use crate::{
    HardwareBackendOutput, VaapiVulkanFullInteropProcessor, VaapiVulkanInteropProcessor,
    VulkanVaapiOutputInteropProcessor,
};
use asciiflow_core::{
    BackendOutput, CancellationToken, Error, FrameSink, FrameSource, MetricStage, Metrics,
    MetricsSnapshot, PipelineStage, Result, VideoFrame,
};
use asciiflow_media::{Decoder, Encoder, VaapiDecodedFrame};
use crossbeam_channel::{RecvTimeoutError, SendTimeoutError, Sender, bounded, unbounded};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

const POLL: Duration = Duration::from_millis(20);

struct DecodedFrame {
    frame: VaapiDecodedFrame,
    started_at: Instant,
}

struct ProcessedFrame {
    frame: VideoFrame,
    started_at: Instant,
}

struct HardwareProcessedFrame {
    output: HardwareBackendOutput,
    started_at: Instant,
}

enum HardwareOutputInput {
    Vaapi(VaapiDecodedFrame),
    Host(VideoFrame),
}

struct HardwareOutputDecodedFrame {
    frame: HardwareOutputInput,
    started_at: Instant,
}

enum HardwareOutputProcessor {
    Full(VaapiVulkanFullInteropProcessor),
    Output(VulkanVaapiOutputInteropProcessor),
}

impl HardwareOutputProcessor {
    fn uses_hardware_input(&self) -> bool {
        matches!(self, Self::Full(_))
    }

    fn submit(&mut self, input: HardwareOutputInput) -> Result<Option<HardwareBackendOutput>> {
        match (self, input) {
            (Self::Full(processor), HardwareOutputInput::Vaapi(frame)) => processor.submit(frame),
            (Self::Output(processor), HardwareOutputInput::Host(frame)) => processor.submit(frame),
            _ => Err(Error::InvalidConfig(
                "hardware-output processor received the wrong input domain".into(),
            )),
        }
    }

    fn drain(&mut self) -> Result<Option<HardwareBackendOutput>> {
        match self {
            Self::Full(processor) => processor.drain(),
            Self::Output(processor) => processor.drain(),
        }
    }
}

pub fn run_interop_pipeline(
    decoder: Decoder,
    processor: VaapiVulkanInteropProcessor,
    encoder: Encoder,
    max_frames: Option<u64>,
    capacity: usize,
) -> Result<MetricsSnapshot> {
    run_interop_pipeline_with_cancellation(
        decoder,
        processor,
        encoder,
        max_frames,
        capacity,
        CancellationToken::new(),
    )
}

pub fn run_interop_pipeline_with_cancellation(
    mut decoder: Decoder,
    mut processor: VaapiVulkanInteropProcessor,
    mut encoder: Encoder,
    max_frames: Option<u64>,
    capacity: usize,
    cancellation: CancellationToken,
) -> Result<MetricsSnapshot> {
    if capacity == 0 {
        return Err(Error::InvalidConfig(
            "pipeline capacity must be non-zero".into(),
        ));
    }
    let started = Instant::now();
    let metrics = Metrics::default();
    let (decoded_tx, decoded_rx) = bounded::<DecodedFrame>(capacity);
    let (processed_tx, processed_rx) = bounded::<ProcessedFrame>(capacity);
    let (error_tx, error_rx) = unbounded::<Error>();

    std::thread::scope(|scope| {
        let cancel = cancellation.clone();
        let errors = error_tx.clone();
        let m = metrics.clone();
        // VAAPI surfaces can retain decode-context status-report state after the
        // producer stops. Keep the decoder object in the owning scope until all
        // processor workers have released those surfaces.
        let decoder = &mut decoder;
        let decoder_thread = scope.spawn(move || {
            let mut remaining = max_frames;
            while !cancel.is_cancelled() && remaining != Some(0) {
                let begin = Instant::now();
                match decoder.next_vaapi_frame() {
                    Ok(Some(frame)) => {
                        m.record(MetricStage::Decode, begin.elapsed());
                        let timings = FrameSource::take_timings(decoder);
                        m.record(MetricStage::DecodePacketSubmit, timings.packet_submit);
                        m.record(MetricStage::DecodeFrameReceive, timings.frame_receive);
                        if let Some(value) = &mut remaining {
                            *value -= 1;
                        }
                        let mut pending = DecodedFrame {
                            frame,
                            started_at: begin,
                        };
                        loop {
                            match decoded_tx.send_timeout(pending, POLL) {
                                Ok(()) => break,
                                Err(SendTimeoutError::Timeout(frame)) if !cancel.is_cancelled() => {
                                    pending = frame;
                                }
                                Err(_) => return,
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        cancel.cancel();
                        let _ = errors.send(Error::pipeline(
                            PipelineStage::DecodeRuntime,
                            "read VAAPI decoded frame",
                            error,
                        ));
                        break;
                    }
                }
            }
            if !cancel.is_cancelled()
                && let Err(error) = FrameSource::finish(decoder)
            {
                let _ = errors.send(Error::pipeline(
                    PipelineStage::Drain,
                    "finish media source",
                    error,
                ));
                cancel.cancel();
            }
        });

        let cancel = cancellation.clone();
        let errors = error_tx.clone();
        let m = metrics.clone();
        let processor_thread = scope.spawn(move || {
            let mut pending_started = VecDeque::new();
            loop {
                match decoded_rx.recv_timeout(POLL) {
                    Ok(decoded) => {
                        pending_started.push_back(decoded.started_at);
                        match processor.submit(decoded.frame) {
                            Ok(Some(output)) => {
                                let started_at = pending_started.pop_front().unwrap();
                                if !forward(output, started_at, &m, &processed_tx, &cancel) {
                                    return;
                                }
                            }
                            Ok(None) => {}
                            Err(error) => {
                                cancel.cancel();
                                let _ = errors.send(Error::pipeline(
                                    PipelineStage::InputInteropRuntime,
                                    "process input DMA-BUF",
                                    error,
                                ));
                                return;
                            }
                        }
                    }
                    Err(RecvTimeoutError::Timeout) if !cancel.is_cancelled() => continue,
                    Err(RecvTimeoutError::Disconnected) => {
                        if cancel.is_cancelled() {
                            return;
                        }
                        loop {
                            match processor.drain() {
                                Ok(Some(output)) => {
                                    let Some(started_at) = pending_started.pop_front() else {
                                        cancel.cancel();
                                        let _ = errors.send(Error::pipeline_message(
                                            PipelineStage::Drain,
                                            "drain input interop",
                                            "processor returned an unsubmitted frame",
                                        ));
                                        return;
                                    };
                                    if !forward(output, started_at, &m, &processed_tx, &cancel) {
                                        return;
                                    }
                                }
                                Ok(None) if pending_started.is_empty() => return,
                                Ok(None) => {
                                    cancel.cancel();
                                    let _ = errors.send(Error::pipeline_message(
                                        PipelineStage::Drain,
                                        "drain input interop",
                                        "processor retained submitted frames",
                                    ));
                                    return;
                                }
                                Err(error) => {
                                    cancel.cancel();
                                    let _ = errors.send(Error::pipeline(
                                        PipelineStage::Drain,
                                        "drain input interop",
                                        error,
                                    ));
                                    return;
                                }
                            }
                        }
                    }
                    Err(_) => return,
                }
            }
        });

        let cancel = cancellation.clone();
        let errors = error_tx.clone();
        let m = metrics.clone();
        // The processor may still own frames from the encoder's VAAPI pool.
        // Borrow the encoder into its worker so the pool outlives processor
        // teardown on cancellation.
        let encoder = &mut encoder;
        let encoder_thread = scope.spawn(move || {
            loop {
                match processed_rx.recv_timeout(POLL) {
                    Ok(frame) => {
                        let begin = Instant::now();
                        if let Err(error) = encoder.encode(frame.frame) {
                            cancel.cancel();
                            let _ = errors.send(Error::pipeline(
                                PipelineStage::EncodeRuntime,
                                "encode video frame",
                                error,
                            ));
                            return;
                        }
                        m.record(MetricStage::Encode, begin.elapsed());
                        record_sink_timings(&m, FrameSink::take_timings(encoder));
                        m.record(MetricStage::PipelineLatency, frame.started_at.elapsed());
                        m.frame_completed();
                    }
                    Err(RecvTimeoutError::Timeout) if !cancel.is_cancelled() => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                    Err(_) => return,
                }
            }
            if !cancel.is_cancelled() {
                let begin = Instant::now();
                if let Err(error) = encoder.finish() {
                    cancel.cancel();
                    let _ = errors.send(Error::pipeline(
                        PipelineStage::Finalization,
                        "finalize encoder and muxer",
                        error,
                    ));
                } else {
                    m.record(MetricStage::Encode, begin.elapsed());
                    record_sink_timings(&m, FrameSink::take_timings(encoder));
                }
            }
        });
        join_workers(
            decoder_thread,
            processor_thread,
            encoder_thread,
            &cancellation,
            &error_tx,
        );
    });
    drop(error_tx);
    metrics.set_total(started.elapsed());
    if let Ok(error) = error_rx.try_recv() {
        return Err(error);
    }
    if cancellation.is_cancelled() {
        return Err(Error::Cancelled);
    }
    Ok(metrics.snapshot())
}

pub fn run_full_interop_pipeline(
    decoder: Decoder,
    processor: VaapiVulkanFullInteropProcessor,
    encoder: Encoder,
    max_frames: Option<u64>,
    capacity: usize,
) -> Result<MetricsSnapshot> {
    run_full_interop_pipeline_with_cancellation(
        decoder,
        processor,
        encoder,
        max_frames,
        capacity,
        CancellationToken::new(),
    )
}

pub fn run_full_interop_pipeline_with_cancellation(
    decoder: Decoder,
    processor: VaapiVulkanFullInteropProcessor,
    encoder: Encoder,
    max_frames: Option<u64>,
    capacity: usize,
    cancellation: CancellationToken,
) -> Result<MetricsSnapshot> {
    run_hardware_output_pipeline(
        decoder,
        HardwareOutputProcessor::Full(processor),
        encoder,
        max_frames,
        capacity,
        cancellation,
    )
}

pub fn run_output_interop_pipeline(
    decoder: Decoder,
    processor: VulkanVaapiOutputInteropProcessor,
    encoder: Encoder,
    max_frames: Option<u64>,
    capacity: usize,
) -> Result<MetricsSnapshot> {
    run_output_interop_pipeline_with_cancellation(
        decoder,
        processor,
        encoder,
        max_frames,
        capacity,
        CancellationToken::new(),
    )
}

pub fn run_output_interop_pipeline_with_cancellation(
    decoder: Decoder,
    processor: VulkanVaapiOutputInteropProcessor,
    encoder: Encoder,
    max_frames: Option<u64>,
    capacity: usize,
    cancellation: CancellationToken,
) -> Result<MetricsSnapshot> {
    run_hardware_output_pipeline(
        decoder,
        HardwareOutputProcessor::Output(processor),
        encoder,
        max_frames,
        capacity,
        cancellation,
    )
}

fn run_hardware_output_pipeline(
    mut decoder: Decoder,
    mut processor: HardwareOutputProcessor,
    mut encoder: Encoder,
    max_frames: Option<u64>,
    capacity: usize,
    cancellation: CancellationToken,
) -> Result<MetricsSnapshot> {
    if capacity == 0 {
        return Err(Error::InvalidConfig(
            "pipeline capacity must be non-zero".into(),
        ));
    }
    let started = Instant::now();
    let metrics = Metrics::default();
    let hardware_input = processor.uses_hardware_input();
    let (decoded_tx, decoded_rx) = bounded::<HardwareOutputDecodedFrame>(capacity);
    let (processed_tx, processed_rx) = bounded::<HardwareProcessedFrame>(capacity);
    let (error_tx, error_rx) = unbounded::<Error>();

    std::thread::scope(|scope| {
        let cancel = cancellation.clone();
        let errors = error_tx.clone();
        let m = metrics.clone();
        // Retained VAAPI frames keep surface references, but Intel's decode
        // status path also requires the decoder context to remain alive until
        // downstream interop workers have released every frame.
        let decoder = &mut decoder;
        let decoder_thread = scope.spawn(move || {
            let mut remaining = max_frames;
            while !cancel.is_cancelled() && remaining != Some(0) {
                let begin = Instant::now();
                let next = if hardware_input {
                    decoder
                        .next_vaapi_frame()
                        .map(|value| value.map(HardwareOutputInput::Vaapi))
                } else {
                    decoder
                        .next_frame()
                        .map(|value| value.map(HardwareOutputInput::Host))
                };
                match next {
                    Ok(Some(frame)) => {
                        m.record(MetricStage::Decode, begin.elapsed());
                        let timings = FrameSource::take_timings(decoder);
                        m.record(MetricStage::DecodePacketSubmit, timings.packet_submit);
                        m.record(MetricStage::DecodeFrameReceive, timings.frame_receive);
                        if let Some(value) = &mut remaining {
                            *value -= 1;
                        }
                        let mut pending = HardwareOutputDecodedFrame {
                            frame,
                            started_at: begin,
                        };
                        loop {
                            match decoded_tx.send_timeout(pending, POLL) {
                                Ok(()) => break,
                                Err(SendTimeoutError::Timeout(frame)) if !cancel.is_cancelled() => {
                                    pending = frame;
                                }
                                Err(_) => return,
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        cancel.cancel();
                        let _ = errors.send(Error::pipeline(
                            PipelineStage::DecodeRuntime,
                            "read decoded frame",
                            error,
                        ));
                        return;
                    }
                }
            }
            if !cancel.is_cancelled()
                && let Err(error) = FrameSource::finish(decoder)
            {
                let _ = errors.send(Error::pipeline(
                    PipelineStage::Drain,
                    "finish media source",
                    error,
                ));
                cancel.cancel();
            }
        });

        let cancel = cancellation.clone();
        let errors = error_tx.clone();
        let m = metrics.clone();
        let processor_thread = scope.spawn(move || {
            let mut pending_started = VecDeque::new();
            loop {
                match decoded_rx.recv_timeout(POLL) {
                    Ok(decoded) => {
                        pending_started.push_back(decoded.started_at);
                        match processor.submit(decoded.frame) {
                            Ok(Some(output)) => {
                                let started_at = pending_started.pop_front().unwrap();
                                record_backend_timings(&m, output.timings);
                                if !send_hardware_output(output, started_at, &processed_tx, &cancel)
                                {
                                    return;
                                }
                            }
                            Ok(None) => {}
                            Err(error) => {
                                cancel.cancel();
                                let _ = errors.send(Error::pipeline(
                                    PipelineStage::OutputInteropRuntime,
                                    "process output DMA-BUF",
                                    error,
                                ));
                                return;
                            }
                        }
                    }
                    Err(RecvTimeoutError::Timeout) if !cancel.is_cancelled() => continue,
                    Err(RecvTimeoutError::Disconnected) => {
                        if cancel.is_cancelled() {
                            return;
                        }
                        loop {
                            match processor.drain() {
                                Ok(Some(output)) => {
                                    let Some(started_at) = pending_started.pop_front() else {
                                        cancel.cancel();
                                        let _ = errors.send(Error::pipeline_message(
                                            PipelineStage::Drain,
                                            "drain output interop",
                                            "processor returned an unsubmitted frame",
                                        ));
                                        return;
                                    };
                                    record_backend_timings(&m, output.timings);
                                    if !send_hardware_output(
                                        output,
                                        started_at,
                                        &processed_tx,
                                        &cancel,
                                    ) {
                                        return;
                                    }
                                }
                                Ok(None) if pending_started.is_empty() => return,
                                Ok(None) => {
                                    cancel.cancel();
                                    let _ = errors.send(Error::pipeline_message(
                                        PipelineStage::Drain,
                                        "drain output interop",
                                        "processor retained submitted frames",
                                    ));
                                    return;
                                }
                                Err(error) => {
                                    cancel.cancel();
                                    let _ = errors.send(Error::pipeline(
                                        PipelineStage::Drain,
                                        "drain output interop",
                                        error,
                                    ));
                                    return;
                                }
                            }
                        }
                    }
                    Err(_) => return,
                }
            }
        });

        let cancel = cancellation.clone();
        let errors = error_tx.clone();
        let m = metrics.clone();
        // Output interop owns surfaces allocated from this encoder's frame
        // pool. Keep the encoder context alive through processor teardown.
        let encoder = &mut encoder;
        let encoder_thread = scope.spawn(move || {
            loop {
                match processed_rx.recv_timeout(POLL) {
                    Ok(processed) => {
                        let begin = Instant::now();
                        if let Err(error) = encoder.encode_hardware_frame(processed.output.frame) {
                            cancel.cancel();
                            let _ = errors.send(Error::pipeline(
                                PipelineStage::EncodeRuntime,
                                "encode VAAPI frame",
                                error,
                            ));
                            return;
                        }
                        m.record(MetricStage::Encode, begin.elapsed());
                        record_sink_timings(&m, FrameSink::take_timings(encoder));
                        m.record(MetricStage::PipelineLatency, processed.started_at.elapsed());
                        m.frame_completed();
                    }
                    Err(RecvTimeoutError::Timeout) if !cancel.is_cancelled() => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                    Err(_) => return,
                }
            }
            if !cancel.is_cancelled() {
                let begin = Instant::now();
                if let Err(error) = encoder.finish() {
                    cancel.cancel();
                    let _ = errors.send(Error::pipeline(
                        PipelineStage::Finalization,
                        "finalize encoder and muxer",
                        error,
                    ));
                } else {
                    m.record(MetricStage::Encode, begin.elapsed());
                    record_sink_timings(&m, FrameSink::take_timings(encoder));
                }
            }
        });
        join_workers(
            decoder_thread,
            processor_thread,
            encoder_thread,
            &cancellation,
            &error_tx,
        );
    });
    drop(error_tx);
    metrics.set_total(started.elapsed());
    if let Ok(error) = error_rx.try_recv() {
        return Err(error);
    }
    if cancellation.is_cancelled() {
        return Err(Error::Cancelled);
    }
    Ok(metrics.snapshot())
}

fn send_hardware_output(
    output: HardwareBackendOutput,
    started_at: Instant,
    processed: &Sender<HardwareProcessedFrame>,
    cancelled: &CancellationToken,
) -> bool {
    let mut pending = HardwareProcessedFrame { output, started_at };
    loop {
        match processed.send_timeout(pending, POLL) {
            Ok(()) => return true,
            Err(SendTimeoutError::Timeout(value)) if !cancelled.is_cancelled() => {
                pending = value;
            }
            Err(_) => return false,
        }
    }
}

fn join_workers<'scope>(
    decoder: std::thread::ScopedJoinHandle<'scope, ()>,
    processor: std::thread::ScopedJoinHandle<'scope, ()>,
    encoder: std::thread::ScopedJoinHandle<'scope, ()>,
    cancellation: &CancellationToken,
    errors: &Sender<Error>,
) {
    for (stage, operation, thread) in [
        (PipelineStage::DecodeRuntime, "join decoder worker", decoder),
        (
            PipelineStage::ProcessingRuntime,
            "join interop worker",
            processor,
        ),
        (PipelineStage::EncodeRuntime, "join encoder worker", encoder),
    ] {
        if thread.join().is_err() {
            cancellation.cancel();
            let _ = errors.send(Error::pipeline_message(
                stage,
                operation,
                "worker thread panicked",
            ));
        }
    }
}

fn forward(
    output: BackendOutput,
    started_at: Instant,
    metrics: &Metrics,
    processed: &Sender<ProcessedFrame>,
    cancelled: &CancellationToken,
) -> bool {
    let timings = output.timings;
    record_backend_timings(metrics, timings);
    let mut pending = ProcessedFrame {
        frame: output.frame,
        started_at,
    };
    loop {
        match processed.send_timeout(pending, POLL) {
            Ok(()) => return true,
            Err(SendTimeoutError::Timeout(frame)) if !cancelled.is_cancelled() => {
                pending = frame;
            }
            Err(_) => return false,
        }
    }
}

fn record_backend_timings(metrics: &Metrics, timings: asciiflow_core::BackendTimings) {
    for (stage, elapsed) in [
        (MetricStage::DrmPrimeMap, timings.drm_prime_map),
        (
            MetricStage::ExternalCapabilityQuery,
            timings.external_capability_query,
        ),
        (
            MetricStage::ExternalImageCreate,
            timings.external_image_create,
        ),
        (
            MetricStage::ExternalMemoryImport,
            timings.external_memory_import,
        ),
        (
            MetricStage::ExternalMemoryBind,
            timings.external_memory_bind,
        ),
        (MetricStage::ExternalOwnership, timings.external_ownership),
        (
            MetricStage::ExternalImageDestroy,
            timings.external_image_destroy,
        ),
        (MetricStage::GpuExternalCopy, timings.gpu_external_copy),
        (
            MetricStage::EncoderSurfaceAcquire,
            timings.encoder_surface_acquire,
        ),
        (MetricStage::OutputDrmPrimeMap, timings.output_drm_prime_map),
        (
            MetricStage::OutputExternalCapabilityQuery,
            timings.output_external_capability_query,
        ),
        (
            MetricStage::OutputExternalImageCreate,
            timings.output_external_image_create,
        ),
        (
            MetricStage::OutputExternalMemoryImport,
            timings.output_external_memory_import,
        ),
        (
            MetricStage::OutputExternalMemoryBind,
            timings.output_external_memory_bind,
        ),
        (
            MetricStage::OutputOwnershipAcquire,
            timings.output_ownership_acquire,
        ),
        (
            MetricStage::OutputOwnershipRelease,
            timings.output_ownership_release,
        ),
        (
            MetricStage::OutputExternalImageDestroy,
            timings.output_external_image_destroy,
        ),
        (
            MetricStage::GpuExternalOutputCopy,
            timings.gpu_external_output_copy,
        ),
        (MetricStage::OutputQueueSubmit, timings.output_queue_submit),
        (MetricStage::OutputGpuWait, timings.output_gpu_wait),
        (MetricStage::QueueSubmit, timings.queue_submit),
        (MetricStage::GpuMapping, timings.gpu_mapping),
        (MetricStage::GpuRender, timings.gpu_render),
        (MetricStage::GpuDownload, timings.gpu_download),
        (MetricStage::GpuBusy, timings.gpu_busy),
        (MetricStage::GpuWait, timings.gpu_wait),
        (MetricStage::HostInvalidate, timings.host_invalidate),
        (MetricStage::HostReadback, timings.host_readback),
        (MetricStage::BackendWall, timings.backend_wall),
    ] {
        metrics.record(stage, elapsed);
    }
}

fn record_sink_timings(metrics: &Metrics, timings: asciiflow_core::SinkTimings) {
    metrics.record(MetricStage::HardwareUpload, timings.hardware_upload);
    metrics.record(MetricStage::EncodeSubmitReceive, timings.submit_receive);
    metrics.record_encode_diagnostics(timings.encode_diagnostics);
    metrics.record(MetricStage::AudioPassthrough, timings.audio_passthrough);
    metrics.record_audio(timings.audio_packets, timings.audio_bytes);
}
