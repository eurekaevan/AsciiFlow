# Stage 2 VAAPI hardware-media baseline

This record captures the 2026-09-14 Stage 2 baseline on the real Intel Arc
Meteor Lake GPU. Stage 2 deliberately keeps Host NV12 between FFmpeg VAAPI and
the existing Vulkan processor; it implements no DMA-BUF or external-memory
interop.

## Device and fixed FFmpeg build

- DRM render node: `/dev/dri/renderD128`, PCI `8086:7d55`
- VA-API: 1.23, libva 2.23.0
- Driver used for codec validation: Intel iHD 26.1.5 from RPM Fusion's complete
  media driver
- H.264: Constrained Baseline/Main/High decode and EncSlice
- HEVC: Main/Main10 decode and EncSlice (not implemented in Stage 2)
- Vulkan device: Intel(R) Arc(tm) Graphics (MTL), Mesa ANV 26.1.8

Fedora's free Intel media-driver build initialized successfully but omitted
H.264 and HEVC profiles. The full driver was unpacked into a temporary runtime
path for this validation; the repository does not embed or redistribute it.

The repository's pinned FFmpeg 9.0.1 source was built and linked for the formal
runs. Its installed ABI versions are libavcodec/libavformat 63.1.101,
libavutil 61.1.101, and libswscale 10.1.101. The checked-in recipe now requires
and explicitly enables `libx264`, `vaapi`, and `libdrm`. The generated
configuration has `CONFIG_VAAPI=1`, `CONFIG_LIBDRM=1`, H.264/HEVC VAAPI
hwaccels, and H.264/HEVC VAAPI encoders. `libavutil.so.61` resolves libva,
libva-drm, and libdrm. The project build is library-only, so the host FFmpeg
8.1.2 CLI supplied the independent `-hwaccels`, encoder/decoder, and output
decode checks; the AsciiFlow binary itself linked to the pinned 9.0.1 build.

## Architecture and ownership

`asciiflow-media` owns three private RAII layers: `HardwareDevice` owns a
VAAPI `AVBufferRef`, `HardwareFramesPool` owns an initialized NV12-to-VAAPI
`AVHWFramesContext`, and `HardwareFrame` owns its `AVFrame`. A reference passed
to an `AVCodecContext` is always a distinct `av_buffer_ref`; FFmpeg releases
that reference when the codec context is freed. Codec contexts are stream-wide,
and the encoder uses one dynamic frames pool whose maximum live surface count
is bounded by the reusable input frame and encoder `async_depth=2` retention.

No FFmpeg or VAAPI type enters Core. Core's planner describes only software or
hardware decode/encode and explicit hardware-download/upload nodes. The CLI is
the composition root for `software|vaapi|auto`; Stage 2 keeps `auto` equal to
software until this evidence is deliberately turned into a default policy.
An explicit VAAPI request fails on device, codec/profile, frames-pool, or NV12
transfer failure and never falls back silently.

The decoder selects only a VAAPI pixel format actually offered by FFmpeg's
`get_format` list, verifies every decoded frame is `AV_PIX_FMT_VAAPI`, verifies
NV12 is an advertised download format, and times the explicit
`av_hwframe_transfer_data` call. A delayed swscale context preserves the
existing normalized BT.709 limited-range Host NV12 contract.

The H.264 encoder uses an NV12 software frame, a VAAPI/NV12 frames pool, an
explicit upload, and `h264_vaapi`. Its repeatable throughput configuration is
CQP, QP 20, async depth 2, GOP 250, zero B frames, High profile Level 4.2 as
confirmed in the output. This is not claimed to be quality-equivalent to the
software libx264 CRF 20 baseline.

## Timing semantics

Total FPS is pipeline throughput. `Pipeline latency` is CPU wall time from the
start of the decode call that produced a frame until the encoder accepted that
frame; it includes bounded-queue residence and overlap effects. Decode and
encode are per-stage CPU wall service times. Packet submit, frame receive,
hardware download, hardware upload, and encode submit/receive are narrower CPU
wall scopes. Hardware transfer calls may synchronize internally and are not
GPU timestamps. Only the Vulkan upload/mapping/render/download figures are
device timestamp-query results. These clock domains are diagnostic and are not
additive.

## Correctness

With Vulkan and synchronization validation enabled, 30 H.264 frames decoded
through software and VAAPI produced identical dimensions, PTS, color metadata,
Y bytes, and UV bytes: zero differing bytes, zero absolute error, maximum
error zero. CPU and Vulkan ASCII outputs were then byte-identical for every
downloaded frame. Decode-only, encode-only, and combined three-frame real-media
smokes passed on Intel Arc with zero Vulkan validation, VUID, or synchronization
errors.

All twelve formal outputs decode without errors. Each contains 300 H.264 Level
4.2 frames at 1920x1080 and 50 FPS, yuv420p, BT.709 primaries/transfer/matrix,
left chroma location, limited range, six-second duration, and strictly
increasing PTS and DTS. Software outputs are Constrained Baseline; VAAPI
outputs are High. Hardware and software bitstreams are intentionally not
compared.

## Formal benchmark

Workload: `input.mp4`, 1920x1080, 300 frames at 50 FPS, ASCII width 80,
standard charset, color enabled, Release, pinned FFmpeg 9.0.1, existing Vulkan
u32-32 mapping/LUT-32x4 render/two slots/cached readback. Validation was off.
Each path had a discarded 30-frame warm-up and three measured 300-frame runs.
The table reports component-wise medians; process CPU is `/usr/bin/time` CPU
percentage measured by the same method for every path.

| Path | Decode wall | hwdownload | Vulkan wall | hwupload | Encode wall | Encode submit/receive | Pipeline latency | FPS | Process CPU |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| A SW decode + SW encode | 3.343 ms | - | 2.744 ms | - | 2.454 ms | 2.074 ms | 13.760 ms | 296.16 | 500% |
| B VAAPI decode + SW encode | 7.172 ms | 6.218 ms | 3.028 ms | - | 2.272 ms | 1.891 ms | 23.760 ms | 138.80 | 274% |
| C SW decode + VAAPI encode | 3.286 ms | - | 2.620 ms | 1.557 ms | 2.259 ms | 0.417 ms | 12.703 ms | 300.00 | 198% |
| D VAAPI decode + VAAPI encode | 7.010 ms | 6.076 ms | 3.295 ms | 1.619 ms | 2.334 ms | 0.365 ms | 23.331 ms | 141.42 | 124% |

Measured FPS runs were A: 296.16, 295.56, 297.83; B: 129.96, 138.80,
145.97; C: 364.42, 300.00, 284.79; D: 144.41, 141.42, 138.71. The short
six-second sample exhibits run-to-run scheduling variance, so decisions use
medians and the large direction of the transfer result rather than the fastest
single run.

VAAPI decode plus download is 53.1% lower throughput than A (0.469x) while
reducing process CPU from 500% to 274%. Its 6.218 ms download consumes 86.7% of
the decode-stage wall and is the direct cause of the regression; the remaining
decode/demux wall is about 0.954 ms.

VAAPI encode plus upload is effectively throughput-neutral at +1.3% (1.013x)
and reduces process CPU by 60.4%. Upload is 1.557 ms, while encoder
submit/receive is only 0.417 ms; explicit transfer is 78.9% of those two
measured hardware-encode scopes. The strong result is CPU-load reduction, not
a material throughput win on this workload.

Combined hardware media is 52.2% lower throughput than A (0.478x), but reduces
process CPU by 75.2%. Its primary bottleneck is the 6.076 ms hwdownload, not
Vulkan compute or hardware encode. The two VAAPI transfers total 7.695 ms of
CPU wall per frame; they are separate stage service scopes and must not be
summed with overlapped pipeline wall to predict FPS.

`intel_gpu_top` correctly identified the Meteor Lake card/render node, but PMU
sampling was denied because the user lacks `CAP_PERFMON`. Consequently no
Video/VideoEnhance utilization percentage is claimed. Hardware execution is
instead established by successful Intel iHD device/profile negotiation,
mandatory `AV_PIX_FMT_VAAPI` decoded frames with hardware frames contexts, and
successful VAAPI-surface upload into `h264_vaapi`; none of these paths permits
software fallback.

## Decision

Hardware H.264 encode is already useful for CPU load while throughput is tied.
Hardware decode is useful only before its explicit Host download; at the
current Host NV12 boundary, download more than erases the codec gain. The data
therefore satisfies the Stage 3 evidence gate for a focused VAAPI-to-Vulkan
decode interop investigation: a real hardware codec is active and hwdownload
is the dominant remaining cost. Upload interop is secondary because hardware
encode is already throughput-neutral while substantially reducing CPU use,
despite its upload. Stage 3 is not implemented here.

Stage 3A was subsequently implemented and is recorded separately in
[`stage3a-validation.md`](stage3a-validation.md); this file remains the frozen
Stage 2 Host-transfer baseline.
