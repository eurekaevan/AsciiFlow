use std::{error::Error as StdError, fmt};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipelineStage {
    InputProbe,
    CapabilityProbe,
    Planning,
    DecoderInitialization,
    InputInteropInitialization,
    ProcessorInitialization,
    OutputInteropInitialization,
    EncoderInitialization,
    MuxInitialization,
    DecodeRuntime,
    InputInteropRuntime,
    ProcessingRuntime,
    OutputInteropRuntime,
    EncodeRuntime,
    MuxRuntime,
    Cancellation,
    Drain,
    Finalization,
}

impl fmt::Display for PipelineStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InputProbe => "input probe",
            Self::CapabilityProbe => "capability probe",
            Self::Planning => "planning",
            Self::DecoderInitialization => "decoder initialization",
            Self::InputInteropInitialization => "input interop initialization",
            Self::ProcessorInitialization => "processor initialization",
            Self::OutputInteropInitialization => "output interop initialization",
            Self::EncoderInitialization => "encoder initialization",
            Self::MuxInitialization => "mux initialization",
            Self::DecodeRuntime => "decode runtime",
            Self::InputInteropRuntime => "input interop runtime",
            Self::ProcessingRuntime => "processing runtime",
            Self::OutputInteropRuntime => "output interop runtime",
            Self::EncodeRuntime => "encode runtime",
            Self::MuxRuntime => "mux runtime",
            Self::Cancellation => "cancellation",
            Self::Drain => "drain",
            Self::Finalization => "finalization",
        })
    }
}

#[derive(Debug)]
pub struct PipelineError {
    pub stage: PipelineStage,
    pub operation: &'static str,
    source: Box<Error>,
}

impl PipelineError {
    pub fn new(stage: PipelineStage, operation: &'static str, source: Error) -> Self {
        Self {
            stage,
            operation,
            source: Box::new(source),
        }
    }

    pub fn source_error(&self) -> &Error {
        &self.source
    }
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} failed: {}", self.operation, self.source)
    }
}

impl StdError for PipelineError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self.source.as_ref())
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("unsupported frame: {0}")]
    UnsupportedFrame(String),
    #[error("media operation failed: {0}")]
    Media(String),
    #[error("CPU backend failed: {0}")]
    Cpu(String),
    #[error("Vulkan backend failed: {0}")]
    Vulkan(String),
    #[error("Vulkan device lost: {0}")]
    DeviceLost(String),
    #[error("GPU teardown timed out: {0}")]
    TeardownTimeout(String),
    #[error("pipeline {stage}: {error}")]
    Pipeline {
        stage: PipelineStage,
        #[source]
        error: PipelineError,
    },
    #[error("pipeline was cancelled")]
    Cancelled,
}

impl Error {
    pub fn pipeline(stage: PipelineStage, operation: &'static str, source: Error) -> Self {
        if matches!(source, Self::Pipeline { .. }) {
            return source;
        }
        Self::Pipeline {
            stage,
            error: PipelineError::new(stage, operation, source),
        }
    }

    pub fn pipeline_message(
        stage: PipelineStage,
        operation: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self::pipeline(stage, operation, Self::Media(message.into()))
    }

    pub fn stage(&self) -> Option<PipelineStage> {
        match self {
            Self::Pipeline { stage, .. } => Some(*stage),
            Self::Cancelled => Some(PipelineStage::Cancellation),
            _ => None,
        }
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}
