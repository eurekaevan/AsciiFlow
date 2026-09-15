use super::{ffi, hwdevice::HardwareDevice};
use asciiflow_core::Result;
use std::{
    ffi::CString,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VaapiOptions {
    pub device: Option<PathBuf>,
}

impl VaapiOptions {
    pub fn new(device: Option<PathBuf>) -> Self {
        Self { device }
    }

    pub(crate) fn create_device(&self) -> Result<HardwareDevice> {
        HardwareDevice::vaapi(self.device.as_deref())
    }

    pub fn display_device(&self) -> &str {
        self.device
            .as_deref()
            .and_then(Path::to_str)
            .unwrap_or("FFmpeg default VAAPI device")
    }

    pub fn probe_device(&self) -> Result<()> {
        drop(self.create_device()?);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeMode {
    Software,
    Vaapi,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeMode {
    Software,
    Vaapi,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VaapiBuildCapabilities {
    pub device: bool,
    pub h264_decoder: bool,
    pub h264_encoder: bool,
    pub software_h264_encoder: bool,
}

pub fn probe_vaapi_build() -> VaapiBuildCapabilities {
    let mut device = false;
    let mut kind = ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_NONE;
    loop {
        kind = unsafe { ffi::av_hwdevice_iterate_types(kind) };
        if kind == ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_NONE {
            break;
        }
        device |= kind == ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI;
    }
    let name = CString::new("h264_vaapi").expect("literal has no NUL");
    let h264_encoder = !unsafe { ffi::avcodec_find_encoder_by_name(name.as_ptr()) }.is_null();
    let h264_decoder =
        !unsafe { ffi::avcodec_find_decoder(ffi::AVCodecID::AV_CODEC_ID_H264) }.is_null();
    let software_name = CString::new("libx264").expect("literal has no NUL");
    let software_h264_encoder =
        !unsafe { ffi::avcodec_find_encoder_by_name(software_name.as_ptr()) }.is_null()
            || !unsafe { ffi::avcodec_find_encoder(ffi::AVCodecID::AV_CODEC_ID_H264) }.is_null();
    VaapiBuildCapabilities {
        device,
        h264_decoder,
        h264_encoder,
        software_h264_encoder,
    }
}
