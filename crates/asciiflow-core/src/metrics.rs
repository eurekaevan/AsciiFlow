use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MetricStage {
    Decode,
    DecodePacketSubmit,
    DecodeFrameReceive,
    HardwareDownload,
    Mapping,
    Render,
    Encode,
    HardwareUpload,
    EncodeSubmitReceive,
    HostUpload,
    QueueSubmit,
    GpuUpload,
    DrmPrimeMap,
    ExternalCapabilityQuery,
    ExternalImageCreate,
    ExternalMemoryImport,
    ExternalMemoryBind,
    ExternalOwnership,
    ExternalImageDestroy,
    GpuExternalCopy,
    EncoderSurfaceAcquire,
    OutputDrmPrimeMap,
    OutputExternalCapabilityQuery,
    OutputExternalImageCreate,
    OutputExternalMemoryImport,
    OutputExternalMemoryBind,
    OutputOwnershipAcquire,
    OutputOwnershipRelease,
    OutputExternalImageDestroy,
    GpuExternalOutputCopy,
    OutputQueueSubmit,
    OutputGpuWait,
    GpuMapping,
    GpuRender,
    GpuDownload,
    GpuBusy,
    GpuWait,
    HostReadback,
    HostInvalidate,
    BackendWall,
    PipelineLatency,
}

#[derive(Clone, Debug, Default)]
pub struct Metrics {
    inner: Arc<Mutex<MetricsSnapshot>>,
}

#[derive(Clone, Debug, Default)]
pub struct MetricsSnapshot {
    pub frames: u64,
    pub decode: Duration,
    pub decode_packet_submit: Duration,
    pub decode_frame_receive: Duration,
    pub hardware_download: Duration,
    pub mapping: Duration,
    pub render: Duration,
    pub encode: Duration,
    pub hardware_upload: Duration,
    pub encode_submit_receive: Duration,
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
    pub host_readback: Duration,
    pub host_invalidate: Duration,
    pub backend_wall: Duration,
    pub pipeline_latency: Duration,
    pub total: Duration,
}

impl Metrics {
    pub fn record(&self, stage: MetricStage, elapsed: Duration) {
        let mut value = self.inner.lock().expect("metrics lock poisoned");
        match stage {
            MetricStage::Decode => value.decode += elapsed,
            MetricStage::DecodePacketSubmit => value.decode_packet_submit += elapsed,
            MetricStage::DecodeFrameReceive => value.decode_frame_receive += elapsed,
            MetricStage::HardwareDownload => value.hardware_download += elapsed,
            MetricStage::Mapping => value.mapping += elapsed,
            MetricStage::Render => value.render += elapsed,
            MetricStage::Encode => value.encode += elapsed,
            MetricStage::HardwareUpload => value.hardware_upload += elapsed,
            MetricStage::EncodeSubmitReceive => value.encode_submit_receive += elapsed,
            MetricStage::HostUpload => value.host_upload += elapsed,
            MetricStage::QueueSubmit => value.queue_submit += elapsed,
            MetricStage::GpuUpload => value.gpu_upload += elapsed,
            MetricStage::DrmPrimeMap => value.drm_prime_map += elapsed,
            MetricStage::ExternalCapabilityQuery => value.external_capability_query += elapsed,
            MetricStage::ExternalImageCreate => value.external_image_create += elapsed,
            MetricStage::ExternalMemoryImport => value.external_memory_import += elapsed,
            MetricStage::ExternalMemoryBind => value.external_memory_bind += elapsed,
            MetricStage::ExternalOwnership => value.external_ownership += elapsed,
            MetricStage::ExternalImageDestroy => value.external_image_destroy += elapsed,
            MetricStage::GpuExternalCopy => value.gpu_external_copy += elapsed,
            MetricStage::EncoderSurfaceAcquire => value.encoder_surface_acquire += elapsed,
            MetricStage::OutputDrmPrimeMap => value.output_drm_prime_map += elapsed,
            MetricStage::OutputExternalCapabilityQuery => {
                value.output_external_capability_query += elapsed
            }
            MetricStage::OutputExternalImageCreate => value.output_external_image_create += elapsed,
            MetricStage::OutputExternalMemoryImport => {
                value.output_external_memory_import += elapsed
            }
            MetricStage::OutputExternalMemoryBind => value.output_external_memory_bind += elapsed,
            MetricStage::OutputOwnershipAcquire => value.output_ownership_acquire += elapsed,
            MetricStage::OutputOwnershipRelease => value.output_ownership_release += elapsed,
            MetricStage::OutputExternalImageDestroy => {
                value.output_external_image_destroy += elapsed
            }
            MetricStage::GpuExternalOutputCopy => value.gpu_external_output_copy += elapsed,
            MetricStage::OutputQueueSubmit => value.output_queue_submit += elapsed,
            MetricStage::OutputGpuWait => value.output_gpu_wait += elapsed,
            MetricStage::GpuMapping => value.gpu_mapping += elapsed,
            MetricStage::GpuRender => value.gpu_render += elapsed,
            MetricStage::GpuDownload => value.gpu_download += elapsed,
            MetricStage::GpuBusy => value.gpu_busy += elapsed,
            MetricStage::GpuWait => value.gpu_wait += elapsed,
            MetricStage::HostReadback => value.host_readback += elapsed,
            MetricStage::HostInvalidate => value.host_invalidate += elapsed,
            MetricStage::BackendWall => value.backend_wall += elapsed,
            MetricStage::PipelineLatency => value.pipeline_latency += elapsed,
        }
    }
    pub fn frame_completed(&self) {
        self.inner.lock().expect("metrics lock poisoned").frames += 1;
    }
    pub fn set_total(&self, total: Duration) {
        self.inner.lock().expect("metrics lock poisoned").total = total;
    }
    pub fn snapshot(&self) -> MetricsSnapshot {
        self.inner.lock().expect("metrics lock poisoned").clone()
    }
}

impl MetricsSnapshot {
    pub fn fps(&self) -> f64 {
        if self.total.is_zero() {
            0.0
        } else {
            self.frames as f64 / self.total.as_secs_f64()
        }
    }
    pub fn ms_per_frame(value: Duration, frames: u64) -> f64 {
        if frames == 0 {
            0.0
        } else {
            value.as_secs_f64() * 1000.0 / frames as f64
        }
    }
}
