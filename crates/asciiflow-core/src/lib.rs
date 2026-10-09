mod audio;
mod backend;
mod cancellation;
mod color;
mod config;
mod error;
mod frame;
mod glyph;
pub mod hdr_pq;
mod metrics;
mod pipeline;
mod planner;
mod qualification;
#[cfg(feature = "reliability-measurement")]
pub mod reliability;
pub mod sdr_target_volume;
pub mod tone_map_bt2446;

pub use audio::{AudioPlan, AudioPolicy, AudioStreamInfo, AudioStreamPlan, SkippedAudioStream};
pub use backend::{
    AsciiBackend, BackendOutput, BackendTimings, EncodeDiagnostics, FrameSink, FrameSource,
    SinkTimings, SourceTimings,
};
pub use cancellation::CancellationToken;
pub use color::{
    ColorError, ColorFieldProvenance, ColorMetadataRaw, ColorProvenance, ColorRational,
    ColorResolutionPolicy, ColorSupportReason, ContentLightLevelMetadata, DynamicRangeClass,
    MasteringDisplayMetadata, ResolvedColorSemantics,
};
pub use config::{AsciiConfig, ProcessingBackend, STANDARD_CHARSET};
pub use error::{Error, PipelineError, PipelineStage, Result};
pub use frame::{
    ChromaLocation, ColorMatrix, ColorPrimaries, ColorRange, ColorSpace, FrameDesc, HostFrame,
    MemoryDomain, PixelFormat, Rational, TransferCharacteristic, VideoFrame,
};
pub use glyph::glyph_lookup_table;
pub use metrics::{MetricStage, Metrics, MetricsSnapshot};
pub use pipeline::{Pipeline, PipelineReport};
pub use planner::{
    CandidateRejection, CapabilitySnapshot, CapabilitySupport, ChromaSubsampling, ColorProcessing,
    FormatOutputInteropCapabilities, FrameDomain, InputRequirements, InteropCapabilities,
    InteropRequest, MediaCapabilities, MediaImplementation, MediaRequest, OutputDynamicRange,
    OutputVideoRequirements, PipelinePlan, PipelinePlanner, PipelinePolicy, PixelPath, PlanNode,
    PlanStep, PlanningResult, ProcessingCapabilities, VideoCodec, VideoProfile, VulkanDeviceKind,
};
pub use qualification::{
    InputProcessingCapabilities, InputProcessingDomain, InputProcessingPlan,
    InputProcessingPlanner, InputProcessingPolicy,
};
#[cfg(feature = "native-reliability")]
pub mod reliability_hooks;
