use asciiflow_core::MetricsSnapshot;

pub fn print_summary(metrics: &MetricsSnapshot) {
    println!(
        "完成：{} 帧，{:.3} 秒，{:.2} FPS",
        metrics.frames,
        metrics.total.as_secs_f64(),
        metrics.fps()
    );
    println!(
        "decode {:.3} · mapping {:.3} · render {:.3} · encode {:.3} · backend wall {:.3} ms/frame",
        MetricsSnapshot::ms_per_frame(metrics.decode, metrics.frames),
        MetricsSnapshot::ms_per_frame(metrics.mapping, metrics.frames),
        MetricsSnapshot::ms_per_frame(metrics.render, metrics.frames),
        MetricsSnapshot::ms_per_frame(metrics.encode, metrics.frames),
        MetricsSnapshot::ms_per_frame(metrics.backend_wall, metrics.frames),
    );
    println!(
        "Pipeline latency {:.3} ms/frame average (decode-call start to encode acceptance; throughput remains {:.2} FPS)",
        MetricsSnapshot::ms_per_frame(metrics.pipeline_latency, metrics.frames),
        metrics.fps(),
    );
    if !(metrics.decode_packet_submit + metrics.decode_frame_receive + metrics.hardware_download)
        .is_zero()
    {
        println!(
            "Media decode CPU wall: packet submit {:.3} · frame receive {:.3} · hw download {:.3} ms/frame (not device timestamps)",
            MetricsSnapshot::ms_per_frame(metrics.decode_packet_submit, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.decode_frame_receive, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.hardware_download, metrics.frames),
        );
    }
    if !(metrics.hardware_upload + metrics.encode_submit_receive).is_zero() {
        println!(
            "Media encode CPU wall: hw upload {:.3} · submit/receive {:.3} ms/frame (not device timestamps)",
            MetricsSnapshot::ms_per_frame(metrics.hardware_upload, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.encode_submit_receive, metrics.frames),
        );
    }
    if !(metrics.drm_prime_map
        + metrics.external_image_create
        + metrics.external_memory_import
        + metrics.gpu_external_copy)
        .is_zero()
    {
        println!(
            "VAAPI/Vulkan interop CPU wall: DRM map {:.3} · capability query {:.3} · image create {:.3} · DMA-BUF import {:.3} · bind {:.3} · ownership command record {:.3} · destroy {:.3} ms/frame",
            MetricsSnapshot::ms_per_frame(metrics.drm_prime_map, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.external_capability_query, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.external_image_create, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.external_memory_import, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.external_memory_bind, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.external_ownership, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.external_image_destroy, metrics.frames),
        );
        println!(
            "VAAPI/Vulkan interop GPU timestamp: external image→buffer {:.3} ms/frame",
            MetricsSnapshot::ms_per_frame(metrics.gpu_external_copy, metrics.frames),
        );
    }
    if !(metrics.encoder_surface_acquire
        + metrics.output_drm_prime_map
        + metrics.output_external_memory_import
        + metrics.gpu_external_output_copy)
        .is_zero()
    {
        println!(
            "Vulkan/VAAPI output interop CPU wall: surface acquire {:.3} · DRM map {:.3} · capability query {:.3} · image create {:.3} · DMA-BUF import {:.3} · bind {:.3} · ownership acquire record {:.3} · release record {:.3} · destroy {:.3} ms/frame",
            MetricsSnapshot::ms_per_frame(metrics.encoder_surface_acquire, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.output_drm_prime_map, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.output_external_capability_query, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.output_external_image_create, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.output_external_memory_import, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.output_external_memory_bind, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.output_ownership_acquire, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.output_ownership_release, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.output_external_image_destroy, metrics.frames),
        );
        println!(
            "Vulkan/VAAPI output interop GPU timestamp: buffer→external image {:.3} ms/frame; output submit {:.3} · fence wait {:.3} ms/frame CPU wall",
            MetricsSnapshot::ms_per_frame(metrics.gpu_external_output_copy, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.output_queue_submit, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.output_gpu_wait, metrics.frames),
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
        println!(
            "Vulkan GPU timestamp: upload/copy {:.3} · mapping {:.3} · render {:.3} · download/copy {:.3} · busy span {:.3} ms/frame",
            MetricsSnapshot::ms_per_frame(metrics.gpu_upload, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.gpu_mapping, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.gpu_render, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.gpu_download, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.gpu_busy, metrics.frames),
        );
        println!(
            "Vulkan CPU wall: host upload {:.3} · queue submit {:.3} · GPU wait {:.3} · host invalidate {:.3} · host memcpy {:.3} ms/frame (not additive with GPU timestamps)",
            MetricsSnapshot::ms_per_frame(metrics.host_upload, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.queue_submit, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.gpu_wait, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.host_invalidate, metrics.frames),
            MetricsSnapshot::ms_per_frame(metrics.host_readback, metrics.frames),
        );
    }
}
