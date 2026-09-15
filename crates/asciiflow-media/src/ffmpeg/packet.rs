use super::ffi;
use asciiflow_core::{Error, Result};
use std::ptr::NonNull;

pub(crate) struct Packet {
    pointer: NonNull<ffi::AVPacket>,
}
impl Packet {
    pub(crate) fn new() -> Result<Self> {
        NonNull::new(unsafe { ffi::av_packet_alloc() })
            .map(|pointer| Self { pointer })
            .ok_or_else(|| Error::Media("failed to allocate FFmpeg packet".into()))
    }
    pub(crate) fn as_mut_ptr(&mut self) -> *mut ffi::AVPacket {
        self.pointer.as_ptr()
    }
    pub(crate) fn unref(&mut self) {
        unsafe { ffi::av_packet_unref(self.pointer.as_ptr()) }
    }
}
impl Drop for Packet {
    fn drop(&mut self) {
        let mut pointer = self.pointer.as_ptr();
        unsafe { ffi::av_packet_free(&mut pointer) };
    }
}
unsafe impl Send for Packet {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allocation_is_owned_by_raii_wrapper() {
        let mut packet = Packet::new().unwrap();
        assert!(!packet.as_mut_ptr().is_null());
    }
}
