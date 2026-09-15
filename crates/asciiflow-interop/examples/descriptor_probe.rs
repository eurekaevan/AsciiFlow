use asciiflow_interop::{DrmPrimeMapping, fourcc_name};
use asciiflow_media::{DecodeMode, Decoder, VaapiOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::args()
        .nth(1)
        .ok_or("usage: descriptor_probe INPUT")?;
    let mut decoder = Decoder::open_with(input, DecodeMode::Vaapi, VaapiOptions::default())?;
    for index in 0..3 {
        let Some(frame) = decoder.next_vaapi_frame()? else {
            break;
        };
        let mapping = DrmPrimeMapping::map_direct_read(frame)?;
        let desc = mapping.descriptor();
        println!(
            "frame {index}: {}x{} objects={} layers={} map_us={:.3}",
            desc.width,
            desc.height,
            desc.objects.len(),
            desc.layers.len(),
            mapping.map_wall().as_secs_f64() * 1e6
        );
        for (object_index, object) in desc.objects.iter().enumerate() {
            println!(
                "  object {object_index}: fd={} size={} modifier={:#018x}",
                object.fd, object.size, object.format_modifier
            );
        }
        for (layer_index, layer) in desc.layers.iter().enumerate() {
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
    }
    Ok(())
}
