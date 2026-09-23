# Stage 5.2B: 10-bit SDR decode and input interop qualification

This is an evidence ledger, **not** a declaration of production 10-bit
transcoding. The output encoders still accept NV12 8-bit only. The CLI rejects
otherwise valid Main10/AV1 10-bit input before staging an output, explaining
that no production 10-bit output path exists. HDR, PQ, HLG, BT.2020 and silent
10→8 conversion remain unsupported.

## Portable and software evidence

Core distinguishes HEVC Main from Main10 and keeps AV1 Main profile separate
from bit depth. Internal P010 processing requires explicit BT.709 primaries,
transfer and matrix plus an explicit full/limited range; unknown color tags
fail closed for 10-bit. The older 8-bit input policy is unchanged.

The software decoder accepts YUV420P10LE or P010LE. YUV420P10LE is repacked
plane-by-plane into tightly packed P010LE without RGB, swscale or bit-depth
conversion; FFmpeg plane strides are respected. P010LE is copied row-by-row.
Invalid source padding and non-zero output padding fail explicitly. VAAPI
hwdownload is configured for P010LE and is not routed through NV12 swscale.

Small checked-in HEVC Main10 and AV1 Main 10-bit fixtures use a deterministic
raw 10-bit gradient, FFmpeg 8.1.2, BT.709 limited-range tags and 36 frames.
Their generator and SHA-256 are under `tests/fixtures/codecs`. Tests confirm
all four low-two-bit patterns occur in decoded samples, P010 output padding is
zero, frame counts and PTS remain ordered through EOF drain, and processing
does not require an encoder. A separate HEVC PQ/BT.2020 fixture is rejected.
The software-decoded media's final ASCII P010 bytes matched CPU and Vulkan
for all 36 frames of each codec on lavapipe with Khronos Validation enabled;
this is not an Intel Arc claim.

Local static validation passed: Release workspace build, workspace tests,
strict Clippy, format/diff checks, and `spirv-val --target-env vulkan1.3`
over all 96 generated modules. All seven checked-in codec fixture hashes
verified. A 300-frame NV12 H.264 software-decode/Vulkan/software-encode run
on llvmpipe produced the same MP4 SHA-256 as the retained Stage 5.2A output:
`90026ef9b395bc2df45045b7401d8b11824d4735313c0eebdd394b2c2db7a7bf`.
This checks a local output regression, not Intel full-interop performance.

## Intel Arc qualification, 2026-09-23

The host's `/dev/dri/renderD128` is Intel Meteor Lake-P Arc Graphics,
PCI `8086:7d55`; iHD is 26.1.5, libva is 2.23.0, and ANV reports Vulkan
1.4.354. The default sandbox does not expose `/dev/dri`; the tests below ran
with host device access. The native profile probe reports HEVC Main10 and AV1
Profile0 10-bit 4:2:0 VLD render-target support independently of 8-bit facts.

Both 64×64, 36-frame fixtures exported the same *observed* DRM PRIME shape:
one 16,384-byte object, modifier `0x0100000000000009`; Y layer `R16 ` at
offset 0, pitch 128; UV layer `GR32` at offset 8192, pitch 128. Each layer
has one plane referencing object 0. Vulkan imports them as `R16_UNORM` and
`R16G16_UNORM` transfer-source images, preserving the P010 words through an
image→buffer copy. The importer rejects any other layer shape or unsupported
modifier. The mapped FFmpeg source remains owned through the copy fence and
final processing/readback fence; imported image and FD ownership is released
after completion.

With Khronos Validation enabled, the Intel-only test compared 30 frames per
codec: software versus VAAPI hwdownload P010 bytes and PTS, software versus
Vulkan image→buffer-imported P010 bytes, and CPU versus direct-import Vulkan
ASCII P010 output bytes. All comparisons passed with zero Validation errors.
The real-media Host P010 CPU/Vulkan parity test passed for all 36 frames per
codec. Synthetic Host P010 mapping, FreeType, one/two-slot parity and 3000-frame
two-slot reuse also passed on the Intel device with zero Validation errors.

The 3000-frame stress inputs were made by stream-copy looping each checked-in
fixture with `ffmpeg -stream_loop 84 -i <fixture> -frames:v 3000 -c copy`.
`ffprobe -count_frames` confirmed 3000 10-bit frames in each MP4. Direct P010
input import and CPU ASCII matched on every frame. For each codec, the FD count
was 4 before, 20 with active decoder/backend resources, reached a per-frame
observed peak of 21, and returned to 4 after release; Validation reported zero
errors.

The Release benchmark used those same 64×64 inputs: 36-frame warm-up, then
three independent 300-frame runs per path. The table reports the run with the
median FPS; timings are per-frame means from that run. CPU% is process CPU time
divided by measured wall time and may exceed 100% on parallel work.

| Codec | Path | FPS (three runs; median) | CPU% | Decode call | HW download | DRM map | Import setup | GPU copy | GPU map/render | Backend wall | Latency |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| HEVC Main10 | Software→Vulkan | 1023.62 | 23.9 | 0.183 | — | — | — | — | 0.131/0.009 | 0.794 | 0.977 |
| HEVC Main10 | VAAPI→hwdownload→Vulkan | 980.64 | 15.5 | 0.312 | 0.240 | — | — | — | 0.124/0.008 | 0.707 | 1.020 |
| HEVC Main10 | VAAPI→DMA-BUF→Vulkan | 1112.84 | 18.6 | 0.061 | — | 0.226 | 0.016 | 0.012 | 0.131/0.008 | 0.609 | 0.899 |
| AV1 10-bit | Software→Vulkan | 1319.50 | 35.1 | 0.214 | — | — | — | — | 0.128/0.009 | 0.543 | 0.758 |
| AV1 10-bit | VAAPI→hwdownload→Vulkan | 1238.85 | 18.8 | 0.275 | 0.194 | — | — | — | 0.125/0.008 | 0.532 | 0.807 |
| AV1 10-bit | VAAPI→DMA-BUF→Vulkan | 1122.76 | 23.2 | 0.090 | — | 0.193 | 0.020 | 0.012 | 0.127/0.008 | 0.606 | 0.891 |

All timing columns are ms/frame. Decode call includes host conversion or
hwdownload, as applicable; HW download is also shown separately as a subset.
Import setup is CPU-side capability query, image creation, DMA-BUF import,
bind, ownership command recording and destruction; GPU copy/map/render are
device timestamps. Backend wall and end-to-end latency enclose other scopes.
These different clocks and nested scopes must not be added. The three-run FPS
ranges were 1002.30–1104.34, 962.60–1024.15 and 927.36–1154.12 for HEVC;
1294.60–1339.32, 1189.29–1283.70 and 1073.34–1122.78 for AV1, in table
path order. The interop path is a synchronous internal one-slot path here; the
small repeated gradient is not a 1080p or production throughput claim.

The unchanged NV12 full-interop CLI was rerun on the retained 1920×1080,
300-frame H.264 input. H.264, HEVC and AV1 output MP4 SHA-256 matched the
retained Stage 5.1B files exactly: respectively `3181fa9775403a2f30e60157797adf370927b4196c960f59d1b5084203d42bef`,
`d4b13786c968e43a502845df46d92dc2dbc951e5e2ef69e4313d5c541664208b`,
and `f044a327c093b8489e03f20c98deab8f80e99f01092fe35595b4f22796c7c534`.
Single-run throughput was 514.16, 495.91 and 511.26 FPS. These are regression
checks, not a new multi-run performance baseline.

This qualifies the internal HEVC Main10 and AV1 10-bit BT.709 SDR P010 input
and processing path on the observed Intel host. The processing-only planner
keeps these facts separate from NV12 and has no encoder node. There is still no
production P010 output encoder, CLI transcode or runtime P010 capability
snapshot; unsupported layouts, other GPUs, larger resolutions and HDR require
their own qualification.
