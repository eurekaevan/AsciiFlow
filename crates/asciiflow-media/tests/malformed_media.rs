use asciiflow_core::FrameSource;
use asciiflow_media::Decoder;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn decode_count(name: &str) -> usize {
    let mut decoder = Decoder::open(fixture(name)).unwrap();
    let mut count = 0;
    while decoder.next_frame().unwrap().is_some() {
        count += 1;
    }
    count
}

#[test]
fn short_and_edit_list_inputs_drain_to_the_expected_frame_count() {
    assert_eq!(decode_count("one-frame.mp4"), 1);
    assert_eq!(decode_count("three-frame.mp4"), 3);
    assert_eq!(decode_count("edit-list.mp4"), 5);
}

#[test]
fn no_video_and_probe_truncation_fail_before_pipeline_execution() {
    for name in ["no-video.mp4", "truncated-probe.mp4"] {
        let error = match Decoder::open(fixture(name)) {
            Ok(_) => panic!("{name} unexpectedly opened as video"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("no decodable video stream"),
            "unexpected {name} error: {error}"
        );
    }
}

#[test]
fn ten_bit_input_is_identified_and_rejected_by_the_nv12_contract() {
    let decoder = Decoder::open(fixture("ten-bit.mp4")).unwrap();
    assert_eq!(decoder.info().requirements.bit_depth, Some(10));
    let error = decoder
        .info()
        .requirements
        .validate_current_pipeline()
        .unwrap_err();
    assert!(error.to_string().contains("10-bit video is not supported"));
}

#[test]
fn damaged_packets_return_errors_instead_of_panicking_or_aborting() {
    for name in ["truncated-tail.mp4", "corrupt-packet.mp4"] {
        let mut decoder = Decoder::open(fixture(name)).unwrap();
        let mut failure = None;
        loop {
            match decoder.next_frame() {
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        let error = failure.unwrap_or_else(|| panic!("{name} unexpectedly decoded as valid"));
        assert!(
            error.to_string().contains("decoder")
                || error.to_string().contains("compressed input")
                || error.to_string().contains("compressed packet"),
            "unexpected {name} error: {error}"
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn repeated_invalid_input_failures_do_not_leak_file_descriptors() {
    fn fd_count() -> usize {
        std::fs::read_dir("/proc/self/fd").unwrap().count()
    }

    let before = fd_count();
    for iteration in 0..100 {
        assert!(
            Decoder::open(fixture("no-video.mp4")).is_err(),
            "iteration {iteration} unexpectedly accepted a no-video container"
        );
    }
    let after = fd_count();
    assert!(
        after <= before,
        "malformed-media failure grew file descriptors from {before} to {after}"
    );
}
