use asciiflow_core::{AsciiBackend, AsciiConfig, FrameSource, PixelFormat};
use asciiflow_cpu::CpuAsciiBackend;
use asciiflow_media::Decoder;
use asciiflow_vulkan::VulkanAsciiBackend;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/codecs")
        .join(name)
}

fn config() -> AsciiConfig {
    AsciiConfig {
        grid_width: 32,
        grid_height: None,
        charset: "@%#*+=-:. ".into(),
        font: "builtin-8x8".into(),
        color: true,
    }
}

#[test]
fn software_main10_and_av1_decode_process_with_cpu_without_output_encoder() {
    for name in [
        "hevc-main10-sdr-gradient.mp4",
        "av1-main10-sdr-gradient.mp4",
    ] {
        let mut decoder = Decoder::open(fixture(name)).unwrap();
        let mut cpu = CpuAsciiBackend::new();
        let mut count = 0;
        while let Some(frame) = decoder.next_frame().unwrap() {
            let pts = frame.pts();
            let output = cpu.process(frame, &config()).unwrap().frame;
            assert_eq!(output.desc().format, PixelFormat::P010Le);
            assert_eq!(output.pts(), pts);
            assert!(
                output
                    .host()
                    .as_slice()
                    .chunks_exact(2)
                    .all(|word| word[0] & 0x3f == 0)
            );
            count += 1;
        }
        assert_eq!(count, 36, "{name}");
    }
}

#[test]
#[ignore = "requires Vulkan 1.3 P010 storage; lavapipe may be enabled explicitly"]
fn software_ten_bit_media_cpu_vulkan_ascii_are_byte_exact() {
    for name in [
        "hevc-main10-sdr-gradient.mp4",
        "av1-main10-sdr-gradient.mp4",
    ] {
        let mut decoder = Decoder::open(fixture(name)).unwrap();
        let mut cpu = CpuAsciiBackend::new();
        let mut vulkan = VulkanAsciiBackend::new().unwrap();
        for index in 0..36 {
            let input = decoder.next_frame().unwrap().unwrap();
            let reference = cpu.process(input.clone(), &config()).unwrap().frame;
            let actual = vulkan.process(input, &config()).unwrap().frame;
            assert_eq!(actual, reference, "{name} frame {index}");
        }
        assert!(decoder.next_frame().unwrap().is_none());
        assert_eq!(vulkan.validation_error_count(), 0);
    }
}
