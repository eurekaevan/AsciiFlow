use super::{ffi, hwdevice::HardwareDevice};
use asciiflow_core::{CapabilitySupport, Error, Result, VideoCodec};
use std::{
    ffi::{CStr, CString},
    path::{Path, PathBuf},
    ptr::{self, NonNull},
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

    /// Probe the three profiles consumed by the media planner.  libva is
    /// loaded for the duration of the query, while the FFmpeg-owned display
    /// remains valid through the device reference.
    pub fn probe_decode_profiles(&self) -> [CapabilitySupport; 3] {
        let library = match unsafe { libloading::Library::new("libva.so.2") } {
            Ok(value) => value,
            Err(error) => {
                return unsupported_profiles(format!("libva.so.2 is unavailable: {error}"));
            }
        };
        let device = match self.create_device() {
            Ok(value) => value,
            Err(error) => return unsupported_profiles(error.to_string()),
        };
        let display = unsafe {
            let buffer = &*device.as_ptr();
            if buffer.data.is_null() {
                return unsupported_profiles("VAAPI device contains no context".into());
            }
            let context = &*(buffer.data.cast::<ffi::AVHWDeviceContext>());
            if context.hwctx.is_null() {
                return unsupported_profiles("VAAPI device context contains no display".into());
            }
            (*(context.hwctx.cast::<ffi::AVVAAPIDeviceContext>())).display
        };
        if display.is_null() {
            return unsupported_profiles("VAAPI device returned a null display".into());
        }
        unsafe { query_profiles(&library, display) }
    }

    /// Probe the exact 8-bit VAAPI encode profiles used by the output planner.
    pub fn probe_encode_profiles(&self) -> [CapabilitySupport; 3] {
        let library = match unsafe { libloading::Library::new("libva.so.2") } {
            Ok(value) => value,
            Err(error) => {
                return unsupported_encode_profiles(format!("libva.so.2 is unavailable: {error}"));
            }
        };
        let device = match self.create_device() {
            Ok(value) => value,
            Err(error) => return unsupported_encode_profiles(error.to_string()),
        };
        let display = unsafe {
            let buffer = &*device.as_ptr();
            if buffer.data.is_null() {
                return unsupported_encode_profiles("VAAPI device contains no context".into());
            }
            let context = &*(buffer.data.cast::<ffi::AVHWDeviceContext>());
            if context.hwctx.is_null() {
                return unsupported_encode_profiles(
                    "VAAPI device context contains no display".into(),
                );
            }
            (*(context.hwctx.cast::<ffi::AVVAAPIDeviceContext>())).display
        };
        if display.is_null() {
            return unsupported_encode_profiles("VAAPI device returned a null display".into());
        }
        unsafe { query_encode_profiles(&library, display) }
    }
}

fn unsupported_profiles(reason: String) -> [CapabilitySupport; 3] {
    [
        CapabilitySupport::unsupported(reason.clone()),
        CapabilitySupport::unsupported(reason.clone()),
        CapabilitySupport::unsupported(reason),
    ]
}

fn unsupported_encode_profiles(reason: String) -> [CapabilitySupport; 3] {
    [
        CapabilitySupport::unsupported(reason.clone()),
        CapabilitySupport::unsupported(reason.clone()),
        CapabilitySupport::unsupported(reason),
    ]
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
    pub hevc_encoder: bool,
    pub av1_encoder: bool,
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
    let hevc_name = CString::new("hevc_vaapi").expect("literal has no NUL");
    let hevc_encoder = !unsafe { ffi::avcodec_find_encoder_by_name(hevc_name.as_ptr()) }.is_null();
    let av1_name = CString::new("av1_vaapi").expect("literal has no NUL");
    let av1_encoder = !unsafe { ffi::avcodec_find_encoder_by_name(av1_name.as_ptr()) }.is_null();
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
        hevc_encoder,
        av1_encoder,
        software_h264_encoder,
    }
}

pub(crate) fn select_decoder(
    codec_id: ffi::AVCodecID,
    mode: DecodeMode,
) -> Result<*const ffi::AVCodec> {
    if mode == DecodeMode::Software {
        let decoder = unsafe { ffi::avcodec_find_decoder(codec_id) };
        if !decoder.is_null()
            && unsafe { (*decoder).capabilities & ffi::AV_CODEC_CAP_HARDWARE as i32 == 0 }
        {
            return Ok(decoder);
        }
    }
    let mut opaque = ptr::null_mut();
    while let Some(decoder) = NonNull::new(unsafe { ffi::av_codec_iterate(&mut opaque) as *mut _ })
    {
        let decoder: *const ffi::AVCodec = decoder.as_ptr();
        let codec = unsafe { &*decoder };
        if codec.type_ != ffi::AVMediaType::AVMEDIA_TYPE_VIDEO
            || codec.id != codec_id
            || unsafe { ffi::av_codec_is_decoder(decoder) } == 0
        {
            continue;
        }
        let mut index = 0;
        if mode == DecodeMode::Software {
            if codec.capabilities & ffi::AV_CODEC_CAP_HARDWARE as i32 == 0 {
                return Ok(decoder);
            }
            continue;
        }
        while let Some(config) =
            NonNull::new(unsafe { ffi::avcodec_get_hw_config(decoder, index) as *mut _ })
        {
            let config: &ffi::AVCodecHWConfig = unsafe { &*config.as_ptr() };
            if config.device_type == ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI
                && config.pix_fmt == ffi::AVPixelFormat::AV_PIX_FMT_VAAPI
                && config.methods & ffi::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32 != 0
            {
                return Ok(decoder);
            }
            index += 1;
        }
    }
    Err(Error::Media(format!(
        "FFmpeg exposes no {mode:?} decoder configuration for {codec_id:?}"
    )))
}

pub fn probe_decoder_build(codec: &VideoCodec, mode: DecodeMode) -> Result<String> {
    let codec_id = match codec {
        VideoCodec::H264 => ffi::AVCodecID::AV_CODEC_ID_H264,
        VideoCodec::Hevc => ffi::AVCodecID::AV_CODEC_ID_HEVC,
        VideoCodec::Av1 => ffi::AVCodecID::AV_CODEC_ID_AV1,
        VideoCodec::Other(name) => {
            return Err(Error::Media(format!("unsupported video codec {name}")));
        }
    };
    let decoder = select_decoder(codec_id, mode)?;
    if unsafe { (*decoder).name.is_null() } {
        return Err(Error::Media("FFmpeg decoder has no name".into()));
    }
    Ok(unsafe { CStr::from_ptr((*decoder).name) }
        .to_string_lossy()
        .into_owned())
}

type VaStatus = i32;
type VaProfile = i32;
type VaEntrypoint = i32;
type VaDisplay = ffi::VADisplay;
const VA_STATUS_SUCCESS: VaStatus = 0;
// libva va.h ABI constants, not codec bitstream profile_idc values.
const VA_PROFILE_H264_BASELINE: VaProfile = 5;
const VA_PROFILE_H264_MAIN: VaProfile = 6;
const VA_PROFILE_H264_HIGH: VaProfile = 7;
const VA_PROFILE_HEVC_MAIN: VaProfile = 17;
const VA_PROFILE_AV1_PROFILE0: VaProfile = 32;
const VA_ENTRYPOINT_VLD: VaEntrypoint = 1;
const VA_ENTRYPOINT_ENC_SLICE: VaEntrypoint = 6;

unsafe fn query_profiles(
    library: &libloading::Library,
    display: VaDisplay,
) -> [CapabilitySupport; 3] {
    // The caller retains both the loaded library and FFmpeg-owned display.
    unsafe {
        let max_profiles: libloading::Symbol<unsafe extern "C" fn(VaDisplay) -> i32> = match library
            .get(b"vaMaxNumProfiles\0")
        {
            Ok(value) => value,
            Err(e) => return unsupported_profiles(format!("VAAPI profile query unavailable: {e}")),
        };
        let query_profiles: libloading::Symbol<
            unsafe extern "C" fn(VaDisplay, *mut VaProfile, *mut i32) -> VaStatus,
        > = match library.get(b"vaQueryConfigProfiles\0") {
            Ok(value) => value,
            Err(e) => return unsupported_profiles(format!("VAAPI profile query unavailable: {e}")),
        };
        let max_entries: libloading::Symbol<unsafe extern "C" fn(VaDisplay) -> i32> = match library
            .get(b"vaMaxNumEntrypoints\0")
        {
            Ok(value) => value,
            Err(e) => {
                return unsupported_profiles(format!("VAAPI entrypoint query unavailable: {e}"));
            }
        };
        let query_entries: libloading::Symbol<
            unsafe extern "C" fn(VaDisplay, VaProfile, *mut VaEntrypoint, *mut i32) -> VaStatus,
        > = match library.get(b"vaQueryConfigEntrypoints\0") {
            Ok(value) => value,
            Err(e) => {
                return unsupported_profiles(format!("VAAPI entrypoint query unavailable: {e}"));
            }
        };
        let profiles = max_profiles(display);
        let entries = max_entries(display);
        if !(1..=1024).contains(&profiles) || !(1..=1024).contains(&entries) {
            return unsupported_profiles(format!(
                "VAAPI returned unsafe capability bounds profiles={profiles}, entrypoints={entries}"
            ));
        }
        let mut available = vec![0; profiles as usize];
        let mut count = profiles;
        if query_profiles(display, available.as_mut_ptr(), &mut count) != VA_STATUS_SUCCESS {
            return unsupported_profiles("VAAPI profile enumeration failed".into());
        }
        if !(0..=profiles).contains(&count) {
            return unsupported_profiles("VAAPI returned an invalid profile count".into());
        }
        [
            profile_support(
                display,
                &available[..count.max(0) as usize],
                entries,
                VA_PROFILE_H264_BASELINE,
                VA_PROFILE_H264_MAIN,
                VA_PROFILE_H264_HIGH,
                *query_entries,
            ),
            profile_support(
                display,
                &available[..count.max(0) as usize],
                entries,
                VA_PROFILE_HEVC_MAIN,
                VA_PROFILE_HEVC_MAIN,
                VA_PROFILE_HEVC_MAIN,
                *query_entries,
            ),
            profile_support(
                display,
                &available[..count.max(0) as usize],
                entries,
                VA_PROFILE_AV1_PROFILE0,
                VA_PROFILE_AV1_PROFILE0,
                VA_PROFILE_AV1_PROFILE0,
                *query_entries,
            ),
        ]
    }
}

unsafe fn query_encode_profiles(
    library: &libloading::Library,
    display: VaDisplay,
) -> [CapabilitySupport; 3] {
    unsafe {
        let max_profiles: libloading::Symbol<unsafe extern "C" fn(VaDisplay) -> i32> =
            match library.get(b"vaMaxNumProfiles\0") {
                Ok(value) => value,
                Err(e) => {
                    return unsupported_encode_profiles(format!(
                        "VAAPI profile query unavailable: {e}"
                    ));
                }
            };
        let query_profiles: libloading::Symbol<
            unsafe extern "C" fn(VaDisplay, *mut VaProfile, *mut i32) -> VaStatus,
        > = match library.get(b"vaQueryConfigProfiles\0") {
            Ok(value) => value,
            Err(e) => {
                return unsupported_encode_profiles(format!(
                    "VAAPI profile query unavailable: {e}"
                ));
            }
        };
        let max_entries: libloading::Symbol<unsafe extern "C" fn(VaDisplay) -> i32> =
            match library.get(b"vaMaxNumEntrypoints\0") {
                Ok(value) => value,
                Err(e) => {
                    return unsupported_encode_profiles(format!(
                        "VAAPI entrypoint query unavailable: {e}"
                    ));
                }
            };
        let query_entries: libloading::Symbol<
            unsafe extern "C" fn(VaDisplay, VaProfile, *mut VaEntrypoint, *mut i32) -> VaStatus,
        > = match library.get(b"vaQueryConfigEntrypoints\0") {
            Ok(value) => value,
            Err(e) => {
                return unsupported_encode_profiles(format!(
                    "VAAPI entrypoint query unavailable: {e}"
                ));
            }
        };
        let profiles = max_profiles(display);
        let entries = max_entries(display);
        if !(1..=1024).contains(&profiles) || !(1..=1024).contains(&entries) {
            return unsupported_encode_profiles(format!(
                "VAAPI returned unsafe capability bounds profiles={profiles}, entrypoints={entries}"
            ));
        }
        let mut available = vec![0; profiles as usize];
        let mut count = profiles;
        if query_profiles(display, available.as_mut_ptr(), &mut count) != VA_STATUS_SUCCESS {
            return unsupported_encode_profiles("VAAPI profile enumeration failed".into());
        }
        if !(0..=profiles).contains(&count) {
            return unsupported_encode_profiles("VAAPI returned an invalid profile count".into());
        }
        let available = &available[..count as usize];
        [
            profile_entrypoint_support(
                display,
                available,
                entries,
                &[
                    VA_PROFILE_H264_BASELINE,
                    13,
                    VA_PROFILE_H264_MAIN,
                    VA_PROFILE_H264_HIGH,
                ],
                VA_ENTRYPOINT_ENC_SLICE,
                *query_entries,
            ),
            profile_entrypoint_support(
                display,
                available,
                entries,
                &[VA_PROFILE_HEVC_MAIN],
                VA_ENTRYPOINT_ENC_SLICE,
                *query_entries,
            ),
            profile_entrypoint_support(
                display,
                available,
                entries,
                &[VA_PROFILE_AV1_PROFILE0],
                VA_ENTRYPOINT_ENC_SLICE,
                *query_entries,
            ),
        ]
    }
}

unsafe fn profile_entrypoint_support(
    display: VaDisplay,
    available: &[VaProfile],
    entries: i32,
    profiles: &[VaProfile],
    wanted: VaEntrypoint,
    query_entries: unsafe extern "C" fn(
        VaDisplay,
        VaProfile,
        *mut VaEntrypoint,
        *mut i32,
    ) -> VaStatus,
) -> CapabilitySupport {
    let mut values = vec![0; entries as usize];
    for profile in profiles.iter().copied().filter(|p| available.contains(p)) {
        let mut count = entries;
        if unsafe { query_entries(display, profile, values.as_mut_ptr(), &mut count) }
            != VA_STATUS_SUCCESS
        {
            return CapabilitySupport::unsupported("VAAPI entrypoint enumeration failed");
        }
        if !(0..=entries).contains(&count) {
            return CapabilitySupport::unsupported("VAAPI returned an invalid entrypoint count");
        }
        if values[..count as usize].contains(&wanted) {
            return CapabilitySupport::supported();
        }
    }
    CapabilitySupport::unsupported("driver exposes no matching EncSlice profile")
}

unsafe fn profile_support(
    display: VaDisplay,
    available: &[VaProfile],
    entries: i32,
    first: VaProfile,
    second: VaProfile,
    third: VaProfile,
    query_entries: unsafe extern "C" fn(
        VaDisplay,
        VaProfile,
        *mut VaEntrypoint,
        *mut i32,
    ) -> VaStatus,
) -> CapabilitySupport {
    let candidates = if first == VA_PROFILE_H264_BASELINE {
        vec![first, 13, second, third]
    } else {
        vec![first]
    };
    let mut values = vec![0; entries as usize];
    for profile in candidates.into_iter().filter(|p| available.contains(p)) {
        let mut count = entries;
        if unsafe { query_entries(display, profile, values.as_mut_ptr(), &mut count) }
            != VA_STATUS_SUCCESS
        {
            return CapabilitySupport::unsupported("VAAPI entrypoint enumeration failed");
        }
        if !(0..=entries).contains(&count) {
            return CapabilitySupport::unsupported("VAAPI returned an invalid entrypoint count");
        }
        if values[..count as usize].contains(&VA_ENTRYPOINT_VLD) {
            return CapabilitySupport::supported();
        }
    }
    CapabilitySupport::unsupported("driver exposes no matching VLD profile")
}
