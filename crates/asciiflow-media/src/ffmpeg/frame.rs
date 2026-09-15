use super::{codec::ffmpeg_error, ffi};
use asciiflow_core::{Error, Result};
use std::ptr::NonNull;

pub(crate) struct Frame {
    pointer: NonNull<ffi::AVFrame>,
}

impl Frame {
    pub(crate) fn new() -> Result<Self> {
        NonNull::new(unsafe { ffi::av_frame_alloc() })
            .map(|pointer| Self { pointer })
            .ok_or_else(|| Error::Media("failed to allocate FFmpeg frame".into()))
    }
    pub(crate) fn as_mut_ptr(&mut self) -> *mut ffi::AVFrame {
        self.pointer.as_ptr()
    }
    pub(crate) fn as_ptr(&self) -> *const ffi::AVFrame {
        self.pointer.as_ptr()
    }
    pub(crate) fn try_clone(&self) -> Result<Self> {
        NonNull::new(unsafe { ffi::av_frame_clone(self.pointer.as_ptr()) })
            .map(|pointer| Self { pointer })
            .ok_or_else(|| Error::Media("failed to retain FFmpeg frame reference".into()))
    }
    pub(crate) fn unref(&mut self) {
        unsafe { ffi::av_frame_unref(self.pointer.as_ptr()) }
    }
    pub(crate) fn make_writable(&mut self) -> Result<()> {
        let result = unsafe { ffi::av_frame_make_writable(self.pointer.as_ptr()) };
        if result < 0 {
            Err(ffmpeg_error(
                "failed to make encoder frame writable",
                result,
            ))
        } else {
            Ok(())
        }
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        let mut pointer = self.pointer.as_ptr();
        unsafe { ffi::av_frame_free(&mut pointer) };
    }
}

unsafe impl Send for Frame {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allocation_is_owned_by_raii_wrapper() {
        let mut frame = Frame::new().unwrap();
        assert!(!frame.as_mut_ptr().is_null());
    }
}
