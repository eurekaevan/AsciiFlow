mod audio;
mod audio_reader;
mod codec;
mod decoder;
mod encoder;
#[cfg(feature = "encode-characterization")]
mod encoder_capture;
mod ffi;
mod frame;
mod hwdevice;
mod hwframes;
#[cfg(feature = "mux-qualification")]
mod mux_trace;
mod packet;
mod vaapi;

pub use audio::{AudioOutputTemplate, AudioPacketSender};
pub use decoder::{Decoder, MediaInfo, VaapiDecodedFrame};
#[cfg(feature = "av1-encode-diagnostic")]
pub use encoder::probe_vaapi_av1_encoder_diagnostic;
pub use encoder::{
    Encoder, OutputEncoding, VaapiEncoderProbe, probe_vaapi_encoder, probe_vaapi_encoder_for,
};
#[cfg(feature = "p010-output-diagnostic")]
pub use hwframes::VaapiDiagnosticP010Pool;
#[cfg(feature = "hdr-to-sdr-qualification")]
pub use hwframes::VaapiSdrQualificationPool;
pub use hwframes::{VaapiEncoderFrame, VaapiEncoderFrames};
pub use vaapi::{
    DecodeMode, EncodeMode, VaapiBuildCapabilities, VaapiOptions, probe_decoder_build,
    probe_vaapi_build,
};
