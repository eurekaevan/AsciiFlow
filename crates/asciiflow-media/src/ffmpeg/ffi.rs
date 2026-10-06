pub(crate) use ffmpeg_sys_next::*;

// libswscale's stable C flags ABI: older headers expose SWS_BILINEAR as a
// macro, newer headers as a SwsFlags enum member. Bindgen changes its Rust
// spelling accordingly, but sws_getContext still accepts this same int bit.
pub(crate) const SWS_BILINEAR_FLAG: i32 = 1 << 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bilinear_flag_matches_native_option() {
        let scaler = unsafe { sws_alloc_context() };
        assert!(!scaler.is_null(), "allocate swscale option context");
        // Resolve the algorithm by name, independently of either generated
        // Rust constant spelling, and verify the compatibility ABI value.
        let set_result = unsafe {
            av_opt_set(
                scaler.cast(),
                c"sws_flags".as_ptr(),
                c"bilinear".as_ptr(),
                0,
            )
        };
        let mut flags = 0;
        let get_result =
            unsafe { av_opt_get_int(scaler.cast(), c"sws_flags".as_ptr(), 0, &mut flags) };
        unsafe { sws_freeContext(scaler) };
        assert_eq!(set_result, 0, "resolve native bilinear option");
        assert_eq!(get_result, 0, "read native scaling flags");
        assert_eq!(flags, i64::from(SWS_BILINEAR_FLAG));
    }
}
