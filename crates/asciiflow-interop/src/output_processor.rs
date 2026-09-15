use crate::DrmPrimeMapping;
use asciiflow_core::{
    AsciiConfig, BackendTimings, Error, FrameDesc, PipelineStage, Result, VideoFrame,
};
use asciiflow_media::{VaapiDecodedFrame, VaapiEncoderFrame, VaapiEncoderFrames};
use asciiflow_vulkan::{DeviceInfo, VulkanAsciiBackend};
use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const SLOT_COUNT: usize = 2;
const WORKER_TIMEOUT: Duration = Duration::from_secs(6);

pub struct HardwareBackendOutput {
    pub frame: VaapiEncoderFrame,
    pub timings: BackendTimings,
}

enum SlotJob {
    Process {
        sequence: u64,
        submitted: Instant,
        surface_acquire: Duration,
        input: SlotInput,
        output: VaapiEncoderFrame,
    },
    Stop,
}

enum SlotInput {
    Hardware(VaapiDecodedFrame),
    Host(VideoFrame),
}

struct SlotResult {
    sequence: u64,
    output: Result<HardwareBackendOutput>,
    validation_errors: usize,
}

struct WorkerSlot {
    jobs: Sender<SlotJob>,
    results: Receiver<SlotResult>,
    thread: Option<JoinHandle<()>>,
}

impl WorkerSlot {
    fn spawn(
        mut backend: VulkanAsciiBackend,
        desc: FrameDesc,
        config: AsciiConfig,
    ) -> Result<Self> {
        backend.prepare(&desc, &config)?;
        let (job_tx, job_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let thread = thread::spawn(move || {
            while let Ok(job) = job_rx.recv() {
                match job {
                    SlotJob::Process {
                        sequence,
                        submitted,
                        surface_acquire,
                        input,
                        output,
                    } => {
                        let output = (|| {
                            let output_mapping = DrmPrimeMapping::map_direct_write(output)
                                .map_err(|error| {
                                    Error::pipeline(
                                        PipelineStage::OutputInteropRuntime,
                                        "map encoder VAAPI frame as writable DRM PRIME",
                                        error,
                                    )
                                })?;
                            let output_map_wall = output_mapping.map_wall();
                            let output_planes = output_mapping
                                .duplicate_external_planes()
                                .map_err(|error| {
                                    Error::pipeline(
                                        PipelineStage::OutputInteropRuntime,
                                        "duplicate output DMA-BUF planes",
                                        error,
                                    )
                                })?;
                            let mut timings = match input {
                                SlotInput::Hardware(input) => {
                                    let input_mapping = DrmPrimeMapping::map_direct_read(input)
                                        .map_err(|error| {
                                            Error::pipeline(
                                                PipelineStage::InputInteropRuntime,
                                                "map decoded VAAPI frame as DRM PRIME",
                                                error,
                                            )
                                        })?;
                                    let input_map_wall = input_mapping.map_wall();
                                    let input_planes = input_mapping
                                        .duplicate_external_planes()
                                        .map_err(|error| {
                                            Error::pipeline(
                                                PipelineStage::InputInteropRuntime,
                                                "duplicate input DMA-BUF planes",
                                                error,
                                            )
                                        })?;
                                    let mut timings = backend.process_external_nv12_to_external(
                                        &desc,
                                        &config,
                                        input_planes,
                                        output_planes,
                                    )?;
                                    timings.drm_prime_map = input_map_wall;
                                    drop(input_mapping);
                                    timings
                                }
                                SlotInput::Host(input) => backend.process_nv12_to_external(
                                    input,
                                    &config,
                                    output_planes,
                                )?,
                            };
                            timings.encoder_surface_acquire = surface_acquire;
                            timings.output_drm_prime_map = output_map_wall;
                            timings.backend_wall = submitted.elapsed();
                            let frame = output_mapping.into_source();
                            Ok(HardwareBackendOutput { frame, timings })
                        })();
                        let failed = output.is_err();
                        let validation_errors = backend.validation_error_count();
                        if result_tx
                            .send(SlotResult {
                                sequence,
                                output,
                                validation_errors,
                            })
                            .is_err()
                            || failed
                        {
                            break;
                        }
                    }
                    SlotJob::Stop => break,
                }
            }
        });
        Ok(Self {
            jobs: job_tx,
            results: result_rx,
            thread: Some(thread),
        })
    }
}

impl Drop for WorkerSlot {
    fn drop(&mut self) {
        let _ = self.jobs.send(SlotJob::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub struct VaapiVulkanFullInteropProcessor {
    desc: FrameDesc,
    frames: VaapiEncoderFrames,
    device_info: DeviceInfo,
    slots: Vec<WorkerSlot>,
    free: VecDeque<usize>,
    pending: VecDeque<(u64, usize)>,
    next_sequence: u64,
    failed: bool,
    validation_errors: usize,
}

impl VaapiVulkanFullInteropProcessor {
    pub fn new(
        first: VulkanAsciiBackend,
        frames: VaapiEncoderFrames,
        desc: FrameDesc,
        config: AsciiConfig,
    ) -> Result<Self> {
        if !first.device_info().dma_buf_interop {
            return Err(Error::Vulkan(
                "selected Vulkan device does not expose the required DMA-BUF interop extensions"
                    .into(),
            ));
        }
        let device_info = first.device_info().clone();
        let second = first.try_fork()?;
        let slots = vec![
            WorkerSlot::spawn(first, desc.clone(), config.clone())?,
            WorkerSlot::spawn(second, desc.clone(), config)?,
        ];
        Ok(Self {
            desc,
            frames,
            device_info,
            slots,
            free: (0..SLOT_COUNT).collect(),
            pending: VecDeque::with_capacity(SLOT_COUNT),
            next_sequence: 0,
            failed: false,
            validation_errors: 0,
        })
    }

    pub fn device_info(&self) -> &DeviceInfo {
        &self.device_info
    }

    pub fn validation_error_count(&self) -> usize {
        self.validation_errors
    }

    pub fn submit(&mut self, input: VaapiDecodedFrame) -> Result<Option<HardwareBackendOutput>> {
        self.submit_input(SlotInput::Hardware(input))
    }

    fn submit_host(&mut self, input: VideoFrame) -> Result<Option<HardwareBackendOutput>> {
        self.submit_input(SlotInput::Host(input))
    }

    fn submit_input(&mut self, input: SlotInput) -> Result<Option<HardwareBackendOutput>> {
        if self.failed {
            return Err(Error::Vulkan(
                "full interop processor is unavailable after a slot failure".into(),
            ));
        }
        let input_desc = match &input {
            SlotInput::Hardware(frame) => frame.desc(),
            SlotInput::Host(frame) => frame.desc(),
        };
        if input_desc != &self.desc {
            return Err(Error::UnsupportedFrame(
                "VAAPI frame geometry changed while using full interop".into(),
            ));
        }
        let completed = if self.free.is_empty() {
            Some(self.complete_oldest()?)
        } else {
            None
        };
        let sequence = self.next_sequence;
        let pts = i64::try_from(sequence)
            .map_err(|_| Error::Media("encoder frame sequence exceeds i64".into()))?;
        let acquire_started = Instant::now();
        let output = self.frames.acquire(pts).map_err(|error| {
            Error::pipeline(
                PipelineStage::OutputInteropRuntime,
                "acquire encoder VAAPI surface",
                error,
            )
        })?;
        let surface_acquire = acquire_started.elapsed();
        let slot = self
            .free
            .pop_front()
            .expect("completed slot was not released");
        self.next_sequence = self.next_sequence.checked_add(1).ok_or_else(|| {
            self.failed = true;
            Error::Vulkan("full interop submission sequence overflow".into())
        })?;
        self.slots[slot]
            .jobs
            .send(SlotJob::Process {
                sequence,
                submitted: Instant::now(),
                surface_acquire,
                input,
                output,
            })
            .map_err(|_| {
                self.failed = true;
                Error::Vulkan("full interop slot stopped before accepting its frame".into())
            })?;
        self.pending.push_back((sequence, slot));
        Ok(completed)
    }

    pub fn drain(&mut self) -> Result<Option<HardwareBackendOutput>> {
        if self.failed {
            return Err(Error::Vulkan(
                "full interop processor is unavailable after a slot failure".into(),
            ));
        }
        if self.pending.is_empty() {
            Ok(None)
        } else {
            self.complete_oldest().map(Some)
        }
    }

    fn complete_oldest(&mut self) -> Result<HardwareBackendOutput> {
        let (expected, slot) = self.pending.pop_front().ok_or_else(|| {
            Error::Vulkan("attempted to complete full interop work with no pending slot".into())
        })?;
        let result = self.slots[slot]
            .results
            .recv_timeout(WORKER_TIMEOUT)
            .map_err(|error| {
                self.failed = true;
                Error::TeardownTimeout(format!(
                    "output interop slot did not return within {} seconds: {error}",
                    WORKER_TIMEOUT.as_secs()
                ))
            })?;
        self.validation_errors = self.validation_errors.max(result.validation_errors);
        self.free.push_back(slot);
        if result.sequence != expected {
            self.failed = true;
            return Err(Error::Vulkan(format!(
                "full interop frame order violation: received {}, expected {expected}",
                result.sequence
            )));
        }
        result.output.inspect_err(|_| self.failed = true)
    }
}

pub struct VulkanVaapiOutputInteropProcessor {
    inner: VaapiVulkanFullInteropProcessor,
}

impl VulkanVaapiOutputInteropProcessor {
    pub fn new(
        backend: VulkanAsciiBackend,
        frames: VaapiEncoderFrames,
        desc: FrameDesc,
        config: AsciiConfig,
    ) -> Result<Self> {
        Ok(Self {
            inner: VaapiVulkanFullInteropProcessor::new(backend, frames, desc, config)?,
        })
    }

    pub fn device_info(&self) -> &DeviceInfo {
        self.inner.device_info()
    }

    pub fn validation_error_count(&self) -> usize {
        self.inner.validation_error_count()
    }

    pub fn submit(&mut self, input: VideoFrame) -> Result<Option<HardwareBackendOutput>> {
        self.inner.submit_host(input)
    }

    pub fn drain(&mut self) -> Result<Option<HardwareBackendOutput>> {
        self.inner.drain()
    }
}
