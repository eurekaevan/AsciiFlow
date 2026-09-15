use crate::{DeviceInfo, MemoryAllocationInfo, VulkanAsciiBackend, context::VulkanContext};
use asciiflow_core::{
    AsciiBackend, AsciiConfig, BackendOutput, Error, FrameDesc, Result, VideoFrame,
};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const SLOT_COUNT: usize = 2;
const WORKER_TIMEOUT: Duration = Duration::from_secs(6);

enum SlotJob {
    Process {
        sequence: u64,
        submitted: Instant,
        frame: VideoFrame,
    },
    Stop,
}

struct SlotResult {
    sequence: u64,
    output: Result<BackendOutput>,
}

struct ReadySlot {
    allocations: Vec<MemoryAllocationInfo>,
    buffer_bytes: u64,
}

struct WorkerSlot {
    jobs: Sender<SlotJob>,
    results: Receiver<SlotResult>,
    thread: Option<JoinHandle<()>>,
    allocations: Vec<MemoryAllocationInfo>,
    buffer_bytes: u64,
}

impl WorkerSlot {
    fn spawn(
        initial: Option<VulkanAsciiBackend>,
        context: Arc<VulkanContext>,
        desc: FrameDesc,
        config: AsciiConfig,
    ) -> Result<Self> {
        let (job_tx, job_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || {
            let ready = (|| {
                let mut backend = match initial {
                    Some(backend) => backend,
                    None => VulkanAsciiBackend::from_context(context)?,
                };
                backend.prepare(&desc, &config)?;
                let ready = ReadySlot {
                    allocations: backend.memory_allocations(),
                    buffer_bytes: backend
                        .allocated_buffer_bytes()
                        .expect("prepared Vulkan resources missing"),
                };
                ready_tx.send(Ok(ready)).map_err(|_| {
                    Error::Vulkan("Vulkan slot owner disappeared during initialization".into())
                })?;
                Ok::<_, Error>(backend)
            })();
            let mut backend = match ready {
                Ok(backend) => backend,
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                    return;
                }
            };

            while let Ok(job) = job_rx.recv() {
                match job {
                    SlotJob::Process {
                        sequence,
                        submitted,
                        frame,
                    } => {
                        let output = backend.process(frame, &config).map(|mut output| {
                            output.timings.backend_wall = submitted.elapsed();
                            output
                        });
                        let failed = output.is_err();
                        if result_tx.send(SlotResult { sequence, output }).is_err() || failed {
                            break;
                        }
                    }
                    SlotJob::Stop => break,
                }
            }
        });
        let ready = ready_rx.recv_timeout(WORKER_TIMEOUT).map_err(|error| {
            Error::Vulkan(format!(
                "Vulkan slot did not initialize within {} seconds: {error}",
                WORKER_TIMEOUT.as_secs()
            ))
        })??;
        Ok(Self {
            jobs: job_tx,
            results: result_rx,
            thread: Some(thread),
            allocations: ready.allocations,
            buffer_bytes: ready.buffer_bytes,
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

/// A bounded two-slot Vulkan backend. Results are always returned in submission
/// order; one frame may remain buffered until `drain` is called.
pub struct PipelinedVulkanAsciiBackend {
    desc: FrameDesc,
    config: AsciiConfig,
    device_info: DeviceInfo,
    context: Arc<VulkanContext>,
    slots: Vec<WorkerSlot>,
    free: VecDeque<usize>,
    pending: VecDeque<(u64, usize)>,
    next_sequence: u64,
    failed: bool,
}

impl PipelinedVulkanAsciiBackend {
    pub fn new(
        first_slot: VulkanAsciiBackend,
        desc: FrameDesc,
        config: AsciiConfig,
    ) -> Result<Self> {
        let device_info = first_slot.device_info().clone();
        let context = first_slot.shared_context();
        let mut slots = Vec::with_capacity(SLOT_COUNT);
        slots.push(WorkerSlot::spawn(
            Some(first_slot),
            context.clone(),
            desc.clone(),
            config.clone(),
        )?);
        slots.push(WorkerSlot::spawn(
            None,
            context.clone(),
            desc.clone(),
            config.clone(),
        )?);
        Ok(Self {
            desc,
            config,
            device_info,
            context,
            slots,
            free: (0..SLOT_COUNT).collect(),
            pending: VecDeque::with_capacity(SLOT_COUNT),
            next_sequence: 0,
            failed: false,
        })
    }

    pub fn device_info(&self) -> &DeviceInfo {
        &self.device_info
    }

    pub fn memory_allocations(&self) -> Vec<MemoryAllocationInfo> {
        self.slots
            .iter()
            .flat_map(|slot| slot.allocations.iter().cloned())
            .collect()
    }

    pub fn allocated_buffer_bytes(&self) -> u64 {
        self.slots.iter().map(|slot| slot.buffer_bytes).sum()
    }

    pub fn validation_error_count(&self) -> usize {
        self.context.validation_error_count()
    }

    fn complete_oldest(&mut self) -> Result<BackendOutput> {
        let (expected, slot_index) = self.pending.pop_front().ok_or_else(|| {
            Error::Vulkan("attempted to complete a Vulkan frame with no pending slot".into())
        })?;
        let result = self.slots[slot_index]
            .results
            .recv_timeout(WORKER_TIMEOUT)
            .map_err(|error| {
                self.failed = true;
                Error::TeardownTimeout(format!(
                    "Vulkan slot did not return its frame within {} seconds: {error}",
                    WORKER_TIMEOUT.as_secs()
                ))
            })?;
        self.free.push_back(slot_index);
        if result.sequence != expected {
            self.failed = true;
            return Err(Error::Vulkan(format!(
                "Vulkan frame order violation: received {}, expected {expected}",
                result.sequence
            )));
        }
        result.output.inspect_err(|_| {
            self.failed = true;
        })
    }

    fn validate_submission(&self, input: &VideoFrame, config: &AsciiConfig) -> Result<()> {
        if self.failed {
            return Err(Error::Vulkan(
                "pipelined Vulkan backend is unavailable after a slot failure".into(),
            ));
        }
        if input.desc() != &self.desc || config != &self.config {
            return Err(Error::Vulkan(
                "frame geometry or ASCII configuration changed while using pipelined Vulkan".into(),
            ));
        }
        Ok(())
    }
}

impl AsciiBackend for PipelinedVulkanAsciiBackend {
    fn process(&mut self, input: VideoFrame, config: &AsciiConfig) -> Result<BackendOutput> {
        if !self.pending.is_empty() {
            return Err(Error::Vulkan(
                "synchronous process cannot run while Vulkan frames are pending".into(),
            ));
        }
        self.submit(input, config)?;
        self.drain()?.ok_or_else(|| {
            Error::Vulkan("Vulkan slot accepted a frame but returned no drained output".into())
        })
    }

    fn submit(&mut self, input: VideoFrame, config: &AsciiConfig) -> Result<Option<BackendOutput>> {
        self.validate_submission(&input, config)?;
        let submitted = Instant::now();
        let completed = if self.free.is_empty() {
            Some(self.complete_oldest()?)
        } else {
            None
        };
        let slot_index = self
            .free
            .pop_front()
            .expect("a completed Vulkan slot was not released");
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.checked_add(1).ok_or_else(|| {
            self.failed = true;
            Error::Vulkan("Vulkan submission sequence overflow".into())
        })?;
        if self.slots[slot_index]
            .jobs
            .send(SlotJob::Process {
                sequence,
                submitted,
                frame: input,
            })
            .is_err()
        {
            self.failed = true;
            return Err(Error::Vulkan(
                "Vulkan slot stopped before accepting its frame".into(),
            ));
        }
        self.pending.push_back((sequence, slot_index));
        Ok(completed)
    }

    fn drain(&mut self) -> Result<Option<BackendOutput>> {
        if self.failed {
            return Err(Error::Vulkan(
                "pipelined Vulkan backend is unavailable after a slot failure".into(),
            ));
        }
        if self.pending.is_empty() {
            return Ok(None);
        }
        self.complete_oldest().map(Some)
    }
}
