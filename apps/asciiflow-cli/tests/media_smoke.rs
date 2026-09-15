use std::{fs, process::Command};

#[test]
#[ignore = "set ASCIIFLOW_TEST_VIDEO to run the native FFmpeg smoke test"]
fn converts_real_video_to_mp4() {
    convert("cpu", "cpu");
}

#[test]
#[ignore = "set ASCIIFLOW_TEST_VIDEO and provide an opt-in Vulkan device"]
fn converts_real_video_with_vulkan() {
    convert("vulkan", "vulkan");
}

#[test]
#[ignore = "set ASCIIFLOW_TEST_VIDEO and provide an opt-in VAAPI device"]
fn converts_real_video_with_vaapi_decode() {
    convert_media("vaapi", "software", "vaapi-decode");
}

#[test]
#[ignore = "set ASCIIFLOW_TEST_VIDEO and provide an opt-in VAAPI device"]
fn converts_real_video_with_vaapi_encode() {
    convert_media("software", "vaapi", "vaapi-encode");
}

#[test]
#[ignore = "set ASCIIFLOW_TEST_VIDEO and provide an opt-in VAAPI device"]
fn converts_real_video_with_combined_vaapi() {
    convert_media("vaapi", "vaapi", "vaapi-combined");
}

fn convert(backend: &str, suffix: &str) {
    let input = std::env::var("ASCIIFLOW_TEST_VIDEO")
        .expect("ASCIIFLOW_TEST_VIDEO must name an input video");
    let input = fs::canonicalize(input).expect("ASCIIFLOW_TEST_VIDEO must resolve to a file");
    let output = std::env::temp_dir().join(format!(
        "asciiflow-{suffix}-smoke-{}.mp4",
        std::process::id()
    ));
    let status = Command::new(env!("CARGO_BIN_EXE_asciiflow"))
        .args([
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "--backend",
            backend,
            "--width",
            "32",
            "--max-frames",
            "3",
            "--no-progress",
        ])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(fs::metadata(&output).unwrap().len() > 0);
    fs::remove_file(output).unwrap();
}

fn convert_media(decode: &str, encode: &str, suffix: &str) {
    let input = std::env::var("ASCIIFLOW_TEST_VIDEO")
        .expect("ASCIIFLOW_TEST_VIDEO must name an input video");
    let input = fs::canonicalize(input).expect("ASCIIFLOW_TEST_VIDEO must resolve to a file");
    let device =
        std::env::var("ASCIIFLOW_VAAPI_DEVICE").unwrap_or_else(|_| "/dev/dri/renderD128".into());
    let output = std::env::temp_dir().join(format!(
        "asciiflow-{suffix}-smoke-{}.mp4",
        std::process::id()
    ));
    let status = Command::new(env!("CARGO_BIN_EXE_asciiflow"))
        .args([
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "--backend",
            "vulkan",
            "--decode",
            decode,
            "--encode",
            encode,
            "--hw-device",
            &device,
            "--width",
            "32",
            "--max-frames",
            "3",
            "--no-progress",
        ])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(fs::metadata(&output).unwrap().len() > 0);
    fs::remove_file(output).unwrap();
}
