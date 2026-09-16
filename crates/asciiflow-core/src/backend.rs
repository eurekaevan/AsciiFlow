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
    pub encode_diagnostics: EncodeDiagnostics,
    pub audio_passthrough: Duration,
    pub audio_packets: u64,
    pub audio_bytes: u64,
}

/// Directly observed encoder/mux scopes. Populated by the optional
/// `encode-characterization` build; zero in the ordinary release build.
#[derive(Clone, Copy, Debug, Default)]
pub struct EncodeDiagnostics {
    pub send_wall: Duration,
    pub receive_wall: Duration,
    pub drain_wall: Duration,
    pub mux_video_write_wall: Duration,
    pub mux_interleave_flush_wall: Duration,
    pub mux_trailer_wall: Duration,
    pub mux_queue_send_wall: Duration,
    pub send_eagain: u64,
    pub receive_eagain: u64,
    pub max_send_retries: u32,
    pub submitted_frames: u64,
    pub received_packets: u64,
    pub received_packet_bytes: u64,
    /// `submitted_frames - received_packets`, not FFmpeg's internal queue depth.
    pub peak_frame_packet_delta: u64,
}

impl EncodeDiagnostics {
    pub fn accumulate(&mut self, other: Self) {
        self.send_wall += other.send_wall;
        self.receive_wall += other.receive_wall;
        self.drain_wall += other.drain_wall;
        self.mux_video_write_wall += other.mux_video_write_wall;
        self.mux_interleave_flush_wall += other.mux_interleave_flush_wall;
        self.mux_trailer_wall += other.mux_trailer_wall;
        self.mux_queue_send_wall += other.mux_queue_send_wall;
        self.send_eagain += other.send_eagain;
        self.receive_eagain += other.receive_eagain;
        self.max_send_retries = self.max_send_retries.max(other.max_send_retries);
        self.submitted_frames += other.submitted_frames;
        self.received_packets += other.received_packets;
        self.received_packet_bytes += other.received_packet_bytes;
        self.peak_frame_packet_delta = self
            .peak_frame_packet_delta
            .max(other.peak_frame_packet_delta);
    }
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

    fn finish(&mut self) -> Result<()> {
        Ok(())
    }

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

#[cfg(test)]
mod diagnostics_tests {
    use super::EncodeDiagnostics;
    use std::time::Duration;

    #[test]
    fn sums_scopes_and_counters_but_keeps_peak_values() {
        let mut total = EncodeDiagnostics {
            send_wall: Duration::from_millis(2),
            send_eagain: 1,
            max_send_retries: 2,
            submitted_frames: 3,
            peak_frame_packet_delta: 4,
            ..Default::default()
        };
        total.accumulate(EncodeDiagnostics {
            send_wall: Duration::from_millis(5),
            send_eagain: 3,
            max_send_retries: 1,
            submitted_frames: 7,
            peak_frame_packet_delta: 2,
            ..Default::default()
        });
        assert_eq!(total.send_wall, Duration::from_millis(7));
        assert_eq!(total.send_eagain, 4);
        assert_eq!(total.submitted_frames, 10);
        assert_eq!(total.max_send_retries, 2);
        assert_eq!(total.peak_frame_packet_delta, 4);
    }
}
