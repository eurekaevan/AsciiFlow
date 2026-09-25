use crate::DrmPrimeMapping;
use asciiflow_core::{
    AsciiConfig, BackendOutput, Error, FrameDesc, PipelineStage, PixelFormat, Result,
};
use asciiflow_media::VaapiDecodedFrame;
use asciiflow_vulkan::{DeviceInfo, VulkanAsciiBackend};
use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const SLOT_COUNT: usize = 2;
const WORKER_TIMEOUT: Duration = Duration::from_secs(6);

enum SlotJob {
    Process {
        sequence: u64,
        submitted: Instant,
        frame: VaapiDecodedFrame,
    },
    Stop,
}

struct SlotResult {
    sequence: u64,
    output: Result<BackendOutput>,
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
                        frame,
                    } => {
                        let output = (|| {
                            let mapping =
                                DrmPrimeMapping::map_direct_read(frame).map_err(|error| {
                                    Error::pipeline(
                                        PipelineStage::InputInteropRuntime,
                                        "map decoded VAAPI frame as DRM PRIME",
                                        error,
                                    )
                                })?;
                            let map_wall = mapping.map_wall();
                            let planes = mapping
                                .duplicate_external_planes_for(desc.format)
                                .map_err(|error| {
                                    Error::pipeline(
                                        PipelineStage::InputInteropRuntime,
                                        "duplicate input DMA-BUF planes",
                                        error,
                                    )
                                })?;
                            let mut output = match desc.format {
                                PixelFormat::Nv12 => backend.process_external_nv12(
                                    &desc,
                                    mapping.pts(),
                                    &config,
                                    planes,
                                ),
                                PixelFormat::P010Le => backend.process_external_p010(
                                    &desc,
                                    mapping.pts(),
                                    &config,
                                    planes,
                                ),
                            }?;
                            // `mapping` retains both FFmpeg frames until the
                            // slot fence has completed inside the backend.
                            output.timings.drm_prime_map = map_wall;
                            output.timings.backend_wall = submitted.elapsed();
                            Ok(output)
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

pub struct VaapiVulkanInteropProcessor {
    desc: FrameDesc,
    device_info: DeviceInfo,
    slots: Vec<WorkerSlot>,
    free: VecDeque<usize>,
    pending: VecDeque<(u64, usize)>,
    next_sequence: u64,
    failed: bool,
    validation_errors: usize,
}

impl VaapiVulkanInteropProcessor {
    pub fn new(first: VulkanAsciiBackend, desc: FrameDesc, config: AsciiConfig) -> Result<Self> {
        if !first.device_info().dma_buf_interop {
            return Err(Error::Vulkan(
                "selected Vulkan device does not expose the required Stage 3A DMA-BUF interop extensions"
                    .into(),
            ));
        }
        let device_info = first.device_info().clone();
        let second = first.try_fork()?;
        let slots = vec![
            WorkerSlot::spawn(first, desc.clone(), config.clone())?,
            WorkerSlot::spawn(second, desc.clone(), config.clone())?,
        ];
        Ok(Self {
            desc,
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

    pub fn submit(&mut self, frame: VaapiDecodedFrame) -> Result<Option<BackendOutput>> {
        if self.failed {
            return Err(Error::Vulkan(
                "VAAPI/Vulkan interop processor is unavailable after a slot failure".into(),
            ));
        }
        if frame.desc() != &self.desc {
            return Err(Error::UnsupportedFrame(
                "VAAPI frame geometry changed while using Stage 3A interop".into(),
            ));
        }
        let completed = if self.free.is_empty() {
            Some(self.complete_oldest()?)
        } else {
            None
        };
        let slot = self
            .free
            .pop_front()
            .expect("completed slot was not released");
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.checked_add(1).ok_or_else(|| {
            self.failed = true;
            Error::Vulkan("interop submission sequence overflow".into())
        })?;
        self.slots[slot]
            .jobs
            .send(SlotJob::Process {
                sequence,
                submitted: Instant::now(),
                frame,
            })
            .map_err(|_| {
                self.failed = true;
                Error::Vulkan("interop slot stopped before accepting its frame".into())
            })?;
        self.pending.push_back((sequence, slot));
        Ok(completed)
    }

    pub fn drain(&mut self) -> Result<Option<BackendOutput>> {
        if self.failed {
            return Err(Error::Vulkan(
                "VAAPI/Vulkan interop processor is unavailable after a slot failure".into(),
            ));
        }
        if self.pending.is_empty() {
            Ok(None)
        } else {
            self.complete_oldest().map(Some)
        }
    }

    fn complete_oldest(&mut self) -> Result<BackendOutput> {
        let (expected, slot) = self.pending.pop_front().ok_or_else(|| {
            Error::Vulkan("attempted to complete interop work with no pending slot".into())
        })?;
        let result = self.slots[slot]
            .results
            .recv_timeout(WORKER_TIMEOUT)
            .map_err(|error| {
                self.failed = true;
                Error::TeardownTimeout(format!(
                    "input interop slot did not return within {} seconds: {error}",
                    WORKER_TIMEOUT.as_secs()
                ))
            })?;
        self.validation_errors = self.validation_errors.max(result.validation_errors);
        self.free.push_back(slot);
        if result.sequence != expected {
            self.failed = true;
            return Err(Error::Vulkan(format!(
                "interop frame order violation: received {}, expected {expected}",
                result.sequence
            )));
        }
        result.output.inspect_err(|_| self.failed = true)
    }
}
