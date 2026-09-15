use crate::{AsciiConfig, Result, VideoFrame};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default)]
pub struct SourceTimings {
    pub packet_submit: Duration,
    pub frame_receive: Duration,
    pub hardware_download: Duration,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SinkTimings {
    pub hardware_upload: Duration,
    pub submit_receive: Duration,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BackendTimings {
    pub mapping: Duration,
    pub render: Duration,
    pub host_upload: Duration,
    pub queue_submit: Duration,
    pub gpu_upload: Duration,
    pub drm_prime_map: Duration,
    pub external_capability_query: Duration,
    pub external_image_create: Duration,
    pub external_memory_import: Duration,
    pub external_memory_bind: Duration,
    pub external_ownership: Duration,
    pub external_image_destroy: Duration,
    pub gpu_external_copy: Duration,
    pub encoder_surface_acquire: Duration,
    pub output_drm_prime_map: Duration,
    pub output_external_capability_query: Duration,
    pub output_external_image_create: Duration,
    pub output_external_memory_import: Duration,
    pub output_external_memory_bind: Duration,
    pub output_ownership_acquire: Duration,
    pub output_ownership_release: Duration,
    pub output_external_image_destroy: Duration,
    pub gpu_external_output_copy: Duration,
    pub output_queue_submit: Duration,
    pub output_gpu_wait: Duration,
    pub gpu_mapping: Duration,
    pub gpu_render: Duration,
    pub gpu_download: Duration,
    pub gpu_busy: Duration,
    pub gpu_wait: Duration,
    pub host_invalidate: Duration,
    pub host_readback: Duration,
    pub backend_wall: Duration,
}

pub struct BackendOutput {
    pub frame: VideoFrame,
    pub timings: BackendTimings,
}

pub trait AsciiBackend: Send {
    fn process(&mut self, input: VideoFrame, config: &AsciiConfig) -> Result<BackendOutput>;

    fn submit(&mut self, input: VideoFrame, config: &AsciiConfig) -> Result<Option<BackendOutput>> {
        self.process(input, config).map(Some)
    }

    fn drain(&mut self) -> Result<Option<BackendOutput>> {
        Ok(None)
    }
}

impl<T: AsciiBackend + ?Sized> AsciiBackend for Box<T> {
    fn process(&mut self, input: VideoFrame, config: &AsciiConfig) -> Result<BackendOutput> {
        (**self).process(input, config)
    }
    fn submit(&mut self, input: VideoFrame, config: &AsciiConfig) -> Result<Option<BackendOutput>> {
        (**self).submit(input, config)
    }
    fn drain(&mut self) -> Result<Option<BackendOutput>> {
        (**self).drain()
    }
}

pub trait FrameSource: Send {
    fn next_frame(&mut self) -> Result<Option<VideoFrame>>;

    fn take_timings(&mut self) -> SourceTimings {
        SourceTimings::default()
    }
}

pub trait FrameSink: Send {
    fn encode(&mut self, frame: VideoFrame) -> Result<()>;
    fn finish(&mut self) -> Result<()>;

    fn take_timings(&mut self) -> SinkTimings {
        SinkTimings::default()
    }
}
