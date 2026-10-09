#![cfg(feature = "hdr-to-sdr-production")]

use asciiflow_core::{
    AsciiConfig, ColorProcessing, ColorSpace, FrameDesc, HostFrame, VideoFrame, hdr_pq,
};
use asciiflow_vulkan::{VulkanAsciiBackend, VulkanHdrToSdrPipeline};

fn diluted_highlight(nits: f64) -> VideoFrame {
    let desc = FrameDesc::host_p010_le(8, 8, ColorSpace::pq_bt2020()).unwrap();
    let mut host = HostFrame::new_zeroed(&desc);
    let (y, uv) = host.planes_mut(&desc);
    for sample in y.as_chunks_mut::<2>().0 {
        sample.copy_from_slice(&(64u16 << 6).to_le_bytes());
    }
    for sample in uv.as_chunks_mut::<2>().0 {
        sample.copy_from_slice(&(512u16 << 6).to_le_bytes());
    }
    let (code, _) = hdr_pq::encode_limited(hdr_pq::pq_to_ycbcr(
        hdr_pq::linear_to_pq(hdr_pq::LinearRgb {
            r: nits,
            g: nits,
            b: nits,
        })
        .unwrap(),
    ))
    .unwrap();
    y[..2].copy_from_slice(&(code.y << 6).to_le_bytes());
    VideoFrame::new_host(desc, None, host).unwrap()
}

#[test]
#[ignore = "actual Vulkan source-domain dispatch, not a production software decode path"]
fn reject_diluted_source_highlight_before_average_and_black_glyph() {
    let config = AsciiConfig {
        grid_width: 1,
        grid_height: Some(1),
        charset: " ".into(),
        ..Default::default()
    };
    let bad = diluted_highlight(2000.0);
    // The sealed post-ASCII reference intentionally cannot infer a source peak
    // from a black glyph. Preserve that oracle; production adds source rejection.
    let mut reference = VulkanHdrToSdrPipeline::new().unwrap();
    let observer = reference.validation_observer();
    let old = reference.process_host(&bad, &config, false).unwrap();
    assert!(old.nonlinear_709.iter().all(|v| *v == [0.0; 3]));
    let backend =
        VulkanAsciiBackend::new_for_color_processing(ColorProcessing::HdrPqToSdrBt709).unwrap();
    let mut production = VulkanHdrToSdrPipeline::from_backend(backend);
    production.submit_host(&bad, &config, false).unwrap();
    let error = production.complete_resident().unwrap_err().to_string();
    assert!(error.contains("0–1000 cd/m² PQ domain"), "{error}");
    assert!(production.last_diagnostics()[4] > 0);
    // A legal source remains legal, with no full RGB/cell host readback.
    production
        .submit_host(&diluted_highlight(750.0), &config, false)
        .unwrap();
    let legal = production.complete_resident().unwrap();
    assert!(legal.nonlinear_709.is_empty());
    assert!(legal.cells.is_empty());
    assert_eq!(&legal.diagnostics[..5], &[0; 5]);
    assert_eq!(production.validation_error_count(), 0);
    drop(production);
    drop(reference);
    assert_eq!(observer(), 0);
}
