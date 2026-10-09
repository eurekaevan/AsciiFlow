use asciiflow_core::MetricsSnapshot;
use std::{fmt::Display, path::Path, time::Duration};

fn normal_summary(metrics: &MetricsSnapshot, output: &Path) -> String {
    format!(
        "Done  {} frames · {:.2} s · {:.2} fps\nOutput  {}",
        metrics.frames,
        metrics.total.as_secs_f64(),
        metrics.fps(),
        output.display(),
    )
}

fn row(label: &str, value: impl Display) {
    println!("  {label:<27} {value}");
}

fn timing_section(title: &str, note: &str, frames: u64, values: &[(&str, Duration)]) {
    println!("\n{title} ({note})");
    for (label, duration) in values {
        row(
            label,
            format_args!(
                "{:.3} ms/frame",
                MetricsSnapshot::ms_per_frame(*duration, frames)
            ),
        );
    }
}

pub fn print_summary(metrics: &MetricsSnapshot, verbose: bool, output: &Path) {
    if !verbose {
        println!("{}", normal_summary(metrics, output));
        return;
    }
    println!("Summary");
    row("Frames", metrics.frames);
    row("Wall", format_args!("{:.2} s", metrics.total.as_secs_f64()));
    row("Throughput", format_args!("{:.2} fps", metrics.fps()));
    row("Output", output.display());
    timing_section(
        "Pipeline",
        "CPU wall; latency is decode-call start to encode acceptance, not throughput",
        metrics.frames,
        &[
            ("Decode", metrics.decode),
            ("Mapping", metrics.mapping),
            ("Render", metrics.render),
            ("Encode", metrics.encode),
            ("Backend wall", metrics.backend_wall),
            ("Latency", metrics.pipeline_latency),
        ],
    );
    if metrics.audio_packets != 0 {
        println!("\nAudio (passthrough; mux CPU wall is not part of video fps)");
        row("Packets", metrics.audio_packets);
        row("Bytes", metrics.audio_bytes);
        row(
            "Mux CPU wall",
            format_args!("{:.3} ms", metrics.audio_passthrough.as_secs_f64() * 1e3),
        );
    }
    if !(metrics.decode_packet_submit + metrics.decode_frame_receive + metrics.hardware_download)
        .is_zero()
    {
        timing_section(
            "Media decode",
            "CPU wall, not device timestamps",
            metrics.frames,
            &[
                ("Packet submit", metrics.decode_packet_submit),
                ("Frame receive", metrics.decode_frame_receive),
                ("Hardware download", metrics.hardware_download),
            ],
        );
    }
    if !(metrics.hardware_upload + metrics.encode_submit_receive).is_zero() {
        timing_section(
            "Media encode",
            "CPU wall, not device timestamps",
            metrics.frames,
            &[
                ("Hardware upload", metrics.hardware_upload),
                ("Submit/receive", metrics.encode_submit_receive),
            ],
        );
    }
    #[cfg(feature = "encode-characterization")]
    {
        let encode = metrics.encode_diagnostics;
        timing_section(
            "Encode characterization",
            "CPU wall; drain overlaps send/receive",
            metrics.frames,
            &[
                ("Send frame", encode.send_wall),
                ("Receive packet", encode.receive_wall),
                ("Encoder drain", encode.drain_wall),
            ],
        );
        timing_section(
            "Mux characterization",
            "CPU wall; asynchronous, not additive",
            metrics.frames,
            &[
                ("Video packet write", encode.mux_video_write_wall),
                ("Interleaver flush", encode.mux_interleave_flush_wall),
                ("Trailer", encode.mux_trailer_wall),
                ("Queue send API", encode.mux_queue_send_wall),
            ],
        );
        println!("\nEncode counts");
        row("Submitted frames", encode.submitted_frames);
        row("Received packets", encode.received_packets);
        row("Packet bytes", encode.received_packet_bytes);
        row("Send EAGAIN", encode.send_eagain);
        row("Receive EAGAIN", encode.receive_eagain);
        row("Max send retries/frame", encode.max_send_retries);
        row("Peak frames-minus-packets", encode.peak_frame_packet_delta);
        println!("  Frames-minus-packets is a proxy, not internal queue depth.");
    }
    if !(metrics.drm_prime_map
        + metrics.external_image_create
        + metrics.external_memory_import
        + metrics.gpu_external_copy)
        .is_zero()
    {
        timing_section(
            "Input interop: VAAPI → Vulkan",
            "CPU wall",
            metrics.frames,
            &[
                ("DRM map", metrics.drm_prime_map),
                ("Capability query", metrics.external_capability_query),
                ("Image create", metrics.external_image_create),
                ("DMA-BUF import", metrics.external_memory_import),
                ("Memory bind", metrics.external_memory_bind),
                ("Ownership command record", metrics.external_ownership),
                ("Image destroy", metrics.external_image_destroy),
            ],
        );
        timing_section(
            "Input interop GPU",
            "GPU timestamps",
            metrics.frames,
            &[("External image → buffer", metrics.gpu_external_copy)],
        );
    }
    if !(metrics.encoder_surface_acquire
        + metrics.output_drm_prime_map
        + metrics.output_external_memory_import
        + metrics.gpu_external_output_copy)
        .is_zero()
    {
        timing_section(
            "Output interop: Vulkan → VAAPI",
            "CPU wall",
            metrics.frames,
            &[
                ("Surface acquire", metrics.encoder_surface_acquire),
                ("DRM map", metrics.output_drm_prime_map),
                ("Capability query", metrics.output_external_capability_query),
                ("Image create", metrics.output_external_image_create),
                ("DMA-BUF import", metrics.output_external_memory_import),
                ("Memory bind", metrics.output_external_memory_bind),
                ("Ownership acquire record", metrics.output_ownership_acquire),
                ("Ownership release record", metrics.output_ownership_release),
                ("Image destroy", metrics.output_external_image_destroy),
                ("Queue submit", metrics.output_queue_submit),
                ("Fence wait", metrics.output_gpu_wait),
            ],
        );
        timing_section(
            "Output interop GPU",
            "GPU timestamps",
            metrics.frames,
            &[("Buffer → external image", metrics.gpu_external_output_copy)],
        );
    }
    if !(metrics.host_upload
        + metrics.gpu_upload
        + metrics.gpu_external_copy
        + metrics.gpu_external_output_copy
        + metrics.gpu_mapping
        + metrics.gpu_render
        + metrics.gpu_download)
        .is_zero()
    {
        timing_section(
            "Vulkan GPU",
            "GPU timestamps",
            metrics.frames,
            &[
                ("Upload/copy", metrics.gpu_upload),
                ("Mapping", metrics.gpu_mapping),
                ("Render", metrics.gpu_render),
                ("Download/copy", metrics.gpu_download),
                ("Busy span", metrics.gpu_busy),
            ],
        );
        timing_section(
            "Vulkan CPU",
            "CPU wall, not additive with GPU timestamps",
            metrics.frames,
            &[
                ("Host upload", metrics.host_upload),
                ("Queue submit", metrics.queue_submit),
                ("GPU wait", metrics.gpu_wait),
                ("Host invalidate", metrics.host_invalidate),
                ("Host memcpy", metrics.host_readback),
            ],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_final_summary_is_stable_english_text() {
        let metrics = MetricsSnapshot {
            frames: 100,
            total: Duration::from_secs(2),
            ..MetricsSnapshot::default()
        };
        assert_eq!(
            normal_summary(&metrics, Path::new("output.mp4")),
            "Done  100 frames · 2.00 s · 50.00 fps\nOutput  output.mp4"
        );
    }
}
