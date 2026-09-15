use super::{codec::ffmpeg_error, ffi};
use asciiflow_core::{Error, Result};
use std::{ffi::CString, path::Path, ptr, ptr::NonNull};

pub(crate) struct HardwareDevice {
    reference: NonNull<ffi::AVBufferRef>,
}

impl HardwareDevice {
    pub(crate) fn vaapi(path: Option<&Path>) -> Result<Self> {
        let display_path = path
            .map(|value| value.display().to_string())
            .unwrap_or_else(|| "FFmpeg default render node".into());
        let path = path
            .map(|path| {
                CString::new(path.as_os_str().as_encoded_bytes())
                    .map_err(|_| Error::Media("VAAPI device path contains a NUL byte".into()))
            })
            .transpose()?;
        let mut reference = ptr::null_mut();
        let result = unsafe {
            ffi::av_hwdevice_ctx_create(
                &mut reference,
                ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
                path.as_ref().map_or(ptr::null(), |value| value.as_ptr()),
                ptr::null_mut(),
                0,
            )
        };
        if result < 0 {
            return Err(ffmpeg_error(
                &format!("failed to create VAAPI device {display_path}"),
                result,
            ));
        }
        let reference = NonNull::new(reference)
            .ok_or_else(|| Error::Media("FFmpeg returned a null VAAPI device".into()))?;
        Ok(Self { reference })
    }

    pub(crate) fn as_ptr(&self) -> *mut ffi::AVBufferRef {
        self.reference.as_ptr()
    }

    pub(crate) fn try_clone_ref(&self) -> Result<*mut ffi::AVBufferRef> {
        let reference = unsafe { ffi::av_buffer_ref(self.reference.as_ptr()) };
        if reference.is_null() {
            Err(Error::Media(
                "failed to retain VAAPI device reference".into(),
            ))
        } else {
            Ok(reference)
        }
    }
}

impl Drop for HardwareDevice {
    fn drop(&mut self) {
        let mut reference = self.reference.as_ptr();
        unsafe { ffi::av_buffer_unref(&mut reference) };
    }
}

unsafe impl Send for HardwareDevice {}
