use super::ffi;
use asciiflow_core::{Error, Result};
use std::ffi::CStr;

pub(crate) fn ffmpeg_error(context: &str, code: i32) -> Error {
    let mut buffer = [0i8; ffi::AV_ERROR_MAX_STRING_SIZE];
    let detail = unsafe {
        if ffi::av_strerror(code, buffer.as_mut_ptr(), buffer.len()) == 0 {
            CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .into_owned()
        } else {
            format!("unknown native error {code}")
        }
    };
    Error::Media(format!("{context}: {detail} ({code})"))
}

pub(crate) fn check(code: i32, context: &str) -> Result<()> {
    if code < 0 {
        Err(ffmpeg_error(context, code))
    } else {
        Ok(())
    }
}

pub(crate) fn again(code: i32) -> bool {
    code == -libc::EAGAIN
}
