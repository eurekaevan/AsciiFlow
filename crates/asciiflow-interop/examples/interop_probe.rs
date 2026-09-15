use asciiflow_core::AsciiConfig;
use asciiflow_interop::VaapiVulkanInteropProcessor;
use asciiflow_media::{DecodeMode, Decoder, VaapiOptions};
use asciiflow_vulkan::VulkanAsciiBackend;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::args()
        .nth(1)
        .ok_or("usage: interop_probe INPUT")?;
    let mut decoder = Decoder::open_with(input, DecodeMode::Vaapi, VaapiOptions::default())?;
    let desc = decoder.info().frame_desc.clone();
    let config = AsciiConfig {
        grid_width: 80,
        grid_height: None,
        charset: "@%#*+=-:. ".into(),
        font: "builtin-8x8".into(),
        color: true,
    };
    let backend = VulkanAsciiBackend::new()?;
    println!(
        "device={} dma_buf={}",
        backend.device_info().name,
        backend.device_info().dma_buf_interop
    );
    let mut processor = VaapiVulkanInteropProcessor::new(backend, desc, config)?;
    let frame = decoder
        .next_vaapi_frame()?
        .ok_or("input contains no frame")?;
    assert!(processor.submit(frame)?.is_none());
    let output = processor
        .drain()?
        .ok_or("interop processor returned no frame")?;
    println!(
        "pts={:?} bytes={} timings={:?}",
        output.frame.pts(),
        output.frame.host().as_slice().len(),
        output.timings
    );
    Ok(())
}
