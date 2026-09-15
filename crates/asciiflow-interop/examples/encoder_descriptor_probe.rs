use asciiflow_core::{ColorSpace, FrameDesc, FrameSink, Rational};
use asciiflow_interop::{DrmPrimeMapping, fourcc_name};
use asciiflow_media::{EncodeMode, Encoder, VaapiOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/asciiflow-encoder-descriptor-probe.mp4".into());
    let desc = FrameDesc::host_nv12(1920, 1080, ColorSpace::default())?;
    let mut encoder = Encoder::create_with(
        &output,
        desc,
        Rational::new(50, 1)?,
        EncodeMode::Vaapi,
        VaapiOptions::default(),
    )?;
    let frames = encoder.encoder_frames()?;
    let frame = frames.acquire(0)?;
    let mapping = DrmPrimeMapping::map_direct_write(frame)?;
    let drm = mapping.descriptor();
    println!(
        "encoder surface: {}x{} objects={} layers={} map_us={:.3}",
        drm.width,
        drm.height,
        drm.objects.len(),
        drm.layers.len(),
        mapping.map_wall().as_secs_f64() * 1e6
    );
    for (index, object) in drm.objects.iter().enumerate() {
        println!(
            "  object {index}: fd={} size={} modifier={:#018x}",
            object.fd, object.size, object.format_modifier
        );
    }
    for (layer_index, layer) in drm.layers.iter().enumerate() {
        println!(
            "  layer {layer_index}: fourcc={} ({:#010x}) planes={}",
            fourcc_name(layer.format),
            layer.format,
            layer.planes.len()
        );
        for (plane_index, plane) in layer.planes.iter().enumerate() {
            println!(
                "    plane {plane_index}: object={} offset={} pitch={}",
                plane.object_index, plane.offset, plane.pitch
            );
        }
    }
    drop(mapping);
    encoder.finish()?;
    let _ = std::fs::remove_file(output);
    Ok(())
}
