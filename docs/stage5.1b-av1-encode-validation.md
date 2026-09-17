# Stage 5.1B: AV1 Profile0 VAAPI output qualification

Status: qualified on Intel Arc Graphics (Meteor Lake), Mesa ANV/iHD, on
2026-09-17. This extends, but does not rewrite, the sealed
[Stage 5.1A](stage5.1a-hevc-encode-validation.md) and
[Stage 5.1A.1](stage5.1a1-encode-characterization.md) evidence. It does not
qualify another GPU, driver, 10-bit format, HDR path, or software AV1 encoder.

## Architecture and policy

The output-side audit found codec-specific selection in the portable output
requirements, CLI choice, capability facts, FFmpeg encoder lookup/profile and
private options. Encoder-owned `AVHWFramesContext` acquisition, frame submission,
packet receive/drain, mux forwarding, staging output and the Stage 3B
Vulkan/VAAPI output processor are shared. No codec branch was added to Stage 3B.

`--output-codec av1` fixes the output codec to AV1; `--encode auto|vaapi` selects
VAAPI when its independent AV1 Profile0 fact is supported. `--encode software`
fails during policy validation with `AV1 software encoding is not implemented`.
An absent AV1 encoder is terminal, never an H.264/HEVC fallback. Auto output
interop may replan once from AV1 full interop to AV1 staged upload; explicit
interop remains strict. Synthetic tests verify that disabling AV1 output import
does not disable H.264/HEVC import, and AV1 encoder initialization failure does
not change codec. The default remains H.264.

Core records `OutputVideoRequirements { codec: Av1, profile: Av1Main, bit_depth:
8, chroma_subsampling: Yuv420 }` without native codec IDs. The CLI describes
this as AV1 Profile0 (Main). Media queries FFmpeg `av1_vaapi`, verifies
`AV_CODEC_ID_AV1` and VAAPI hardware-frames support, and queries libva
`VAProfileAV1Profile0` (ABI value 32) plus `VAEntrypointEncSlice` (value 6).
The qualified device supports both. It then opens an actual NV12 encoder and
qualifies host upload and writable external-image import. `--capabilities`
reports AV1 encoder and output interop independently of H.264/HEVC.

The AV1 baseline is CQP, `profile=main`, `async_depth=2`,
`AVCodecContext.global_quality=25`, `max_b_frames=0`, GOP 250. `av1_vaapi`
does not accept the H.264/HEVC private `qp` option; their CQP/`qp=20` settings
remain unchanged. These numbers are not cross-codec quality equivalents.
FFmpeg owns AV1 sequence headers and the MP4 sample entry; no custom bitstream
or codec-tag writer was added.

## Production surface and correctness

The production AV1 encoder-owned surface at 1920×1080 exported one 3,194,880
byte DMA-BUF object with modifier `0x0100000000000009` and two layers: Y `R8`
offset 0/pitch 1920, UV `GR88` offset 2,088,960/pitch 1920. The H.264, HEVC
and AV1 diagnostic/production descriptors agree on object count, layer count,
fourcc, offsets, pitch and modifier on this stack. Thus Stage 3B is
architecture- and approximately cost-neutral across the three qualified 8-bit
output codecs here; this is not a portable modifier guarantee. Ownership remains
acquire VAAPI surface → export/map → duplicate DMA-BUF handles for Vulkan →
GPU copy and fence completion → release/unmap → submit that same surface to the
encoder. A hardware test rejects submission of an AV1 surface to an H.264
encoder context and vice versa.

The opt-in production test compared 30 frames of staged Host readback/hwupload
with full output interop **before encoding**: Y bytes, UV bytes, dimensions,
frame metadata and logical PTS were exact. It also passed Khronos Validation
with zero reported errors. The same comparison ran for 3000 frames, decoded
all 3000 output frames, and observed FD counts `before=4`, active baseline
`=36`, max steady `=36`, `after=4`. A separate 3000-frame production MP4 had
AV1 Main/yuv420p, 60.000 seconds, `nb_frames=3000`, and decoded completely
with no FFmpeg error. Its decoded timestamps were exactly
`0, 256, ..., 2999×256` at time base `1/12800` (zero mismatches).

The first three-frame production MP4 identified as `codec_name=av1`,
`codec_tag_string=av01`, Main, yuv420p, 1920×1080 with 20-byte extradata; it
decoded successfully. The 300-frame output had `av01`, Main, yuv420p, 300
frames, six-second duration, 20-byte extradata, and fully decoded with logical
timestamps 0–299 at 50 fps. AV1 staged and full 300-frame MP4s were also
byte-identical on this qualified stack (SHA-256
`f044a327c093b8489e03f20c98deab8f80e99f01092fe35595b4f22796c7c534`),
though bitstream byte identity is not a cross-driver contract. The short-stream
drain matrix returned exactly 1, 2, 3, 5, 49 and 301 decoded frames; a
zero-video input is rejected by the existing input probe before output
initialization. Delayed packets are drained before MP4 trailer write.

H.264, HEVC Main and AV1 Main inputs each produced AV1 output. The CPU ASCII
path (software decode → Host NV12 → CPU ASCII → VAAPI hwupload → AV1 encode)
also completed. A 128×96 synthetic derivative of the AAC fixture retained
identical AAC compressed-packet hashes, `jpn` language/default disposition and
relative A/V start; a two-track derivative retained both packet hashes,
`jpn`/`eng`, dispositions and durations with a custom FreeType atlas. With a
30-frame video limit, the AV1 track contained 30 frames/one second while the
AAC track retained all 142 packets/three seconds, as the existing contract
requires. Audio
architecture and font ownership were unchanged. The AV1 encoder reports coded
surface constraints width 128–8192 and height 96–8192 on this driver. Requests
for visible 64×96 and 128×64 fail at encoder probing with requested geometry
and NV12 in the pipeline error; 126×96, 128×94 and 128×96 initialize because
FFmpeg may pad visible dimensions. No global size minimum was hard-coded.

AV1 send/receive test faults retain `EncodeRuntime` context; a first-video-
packet mux fault retains `MuxRuntime` as the root cause. Initialization faults
for encoder creation, frame pool and output import preserve the requested AV1
codec and at most one legal replan. A real full-interop SIGINT run returned
130, left an existing destination byte-for-byte intact, removed staging and
allowed immediate AV1/Vulkan/VAAPI reinitialization. A 30-frame production
full-interop smoke with `ASCIIFLOW_VULKAN_VALIDATION=1` reported no Validation,
VUID or synchronization errors. These tests reuse the existing safe-output,
bounded-channel and surface ownership machinery; they do not add an AV1-only
teardown path.

## Formal throughput and encode-side characterization

All four primary paths used the **same retained Stage 5.1A.1 input**:
`target/stage51a1-evidence/h264-testsrc2-300.mp4`, SHA-256
`6b47b510c4a8f604e6404b1fbbc5f68bdaa77043976acdad0153ef83524b6e34`.
It is 1920×1080, 50 fps, H.264, 300 frames. Every run used VAAPI decode,
VAAPI/Vulkan input interop, Vulkan ASCII width 80, color on, built-in 8×8
font, audio off, Release with `encode-characterization`, Validation off.
Each path had a separate 30-frame warm-up followed by three complete 300-frame
runs. Process CPU is `/usr/bin/time` including startup; CLI FPS excludes
capability-probe startup. Each row below is an individual retained run, not a
best-run selection. All logs, `.time` files and output MP4s are under ignored
`target/stage51b-evidence/`.

| Path | Run 1 FPS / CPU / mean latency ms | Run 2 | Run 3 | Median FPS | Min–max FPS |
| --- | --- | --- | --- | ---: | ---: |
| H.264 full | 514.45 / 84% / 20.199 | 506.18 / 91% / 20.465 | 508.80 / 90% / 20.343 | **508.80** | 506.18–514.45 |
| HEVC full | 467.34 / 80% / 22.378 | 471.25 / 80% / 22.248 | 456.93 / 81% / 22.922 | **467.34** | 456.93–471.25 |
| AV1 staged | 303.80 / 143% / 34.768 | 330.59 / 147% / 31.832 | 240.82 / 144% / 44.197 | **303.80** | 240.82–330.59 |
| AV1 full | 502.97 / 86% / 20.614 | 501.29 / 84% / 20.779 | 504.26 / 87% / 20.541 | **502.97** | 501.29–504.26 |

The staged three-run spread was too large for a stable comparison. After a
fresh warm-up, five **interleaved** staged/full pairs gave:

| Path | Run 1 | Run 2 | Run 3 | Run 4 | Run 5 | Median | Min–max |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| AV1 staged FPS | 326.97 | 331.04 | 334.01 | 316.21 | 340.01 | **331.04** | 316.21–340.01 |
| AV1 full FPS | 501.10 | 515.03 | 506.67 | 490.89 | 499.57 | **501.10** | 490.89–515.03 |
| AV1 staged CPU | 146% | 147% | 148% | 146% | 144% | **146%** | 144–148% |
| AV1 full CPU | 85% | 84% | 85% | 88% | 85% | **85%** | 84–88% |

The more stable five-pair median shows AV1 full interop **51.37% faster** than
staged on this input, with 61 percentage points less process CPU. The original
three-run staged minimum of 240.82 FPS remains in the record rather than being
discarded. In the three-run common-codec matrix AV1 full was 1.15% below H.264
full and 7.62% above HEVC full. This describes pipeline cost under each codec's
different baseline options, **not** compression quality or media-engine
saturation. H.264 default output SHA-256 stayed
`3181fa9775403a2f30e60157797adf370927b4196c960f59d1b5084203d42bef`;
HEVC stayed
`d4b13786c968e43a502845df46d92dc2dbc951e5e2ef69e4313d5c541664208b`.
Both match the retained Stage 5.1A.1 outputs byte-for-byte.

The following are medians of the three *per-run means* in ms/frame. `—` is
absent, not zero. CPU wall, GPU timestamps and asynchronous mux scopes overlap
and must not be mechanically summed.

| Scope | H.264 full | HEVC full | AV1 staged | AV1 full | Clock |
| --- | ---: | ---: | ---: | ---: | --- |
| Surface acquire | 0.017 | 0.022 | — | 0.040 | CPU wall |
| DRM export/map | 0.020 | 0.035 | — | 0.022 | CPU wall |
| Capability query | 0.011 | 0.011 | — | 0.012 | CPU wall |
| External image create | 0.002 | 0.002 | — | 0.002 | CPU wall |
| DMA-BUF memory import | 0.007 | 0.004 | — | 0.005 | CPU wall |
| GPU buffer→external image | 0.146 | 0.148 | — | 0.124 | Vulkan timestamp |
| Output fence wait | 0.387 | 0.402 | — | 0.339 | CPU wall, Vulkan copy completion |
| Host→VAAPI hwupload | — | — | 2.302 | — | CPU wall |
| `avcodec_send_frame` | 1.882 | 2.068 | 0.553 | 1.920 | CPU wall |
| `avcodec_receive_packet` | 0.001 | 0.001 | 0.001 | 0.001 | CPU wall |
| Video mux packet write | 0.027 | 0.022 | 0.031 | 0.019 | CPU wall in mux worker |
| Submitted / received packets | 300 / 300 | 300 / 300 | 300 / 300 | 300 / 300 | Count |
| Send EAGAIN / max retries | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | Count |
| Receive EAGAIN | 300 | 300 | 300 | 300 | Expected end-poll, not pressure |
| Peak submitted-minus-packets | 2 | 2 | 2 | 2 | Application proxy, not VAAPI queue depth |

The five-pair AV1 staged median hwupload was 2.105 ms/frame; AV1 full median
send was 1.932 ms/frame versus staged 0.514. This is consistent with encoder
surface readiness being paid during hwupload in the staged path and later in
`send_frame` in the full path. It does not make `send_frame` a pure hardware
encode-engine timer. The AV1 full output GPU copy (0.124 ms/frame) is close to
H.264/HEVC (0.146/0.148), and its CPU import scopes are small. No codec branch
or distinct memory transaction was needed in Stage 3B. AV1's 300-run packet
payload was 5,120,146 bytes and MP4 was 5,122,266 bytes; these are mux/load
diagnostics only, not a quality comparison.

Capability-probe wall on three alternating old/new binaries was old
227.623/226.956/222.332 ms and new 242.813/239.815/229.455 ms, medians
226.956 and 239.815 ms. The observed 12.859 ms increase is noisy and includes
driver startup; no persistent cache was introduced. The prior single new probe
had taken 127.227 ms, illustrating that a precise incremental cost cannot be
inferred from this small sample.

## Remaining bounds and next-stage gate

AV1 is qualified only as Profile0/Main, 8-bit 4:2:0 NV12, VAAPI and MP4 on the
tested Intel/Linux stack. Software AV1 encode, P010/Main10, HDR, custom
rate-control/quality/bitrate UI and AV1 tuning remain unsupported. Internal
VAAPI pool occupancy, per-frame p95 and GPU media-engine utilization were not
observable here; the submitted-minus-packets count is not a hardware queue
depth. A later driver or FFmpeg build must requalify descriptor, rate-control
acceptance and bitstream output. The current path is sufficiently established
to *consider* a separately scoped P010/10-bit foundation, but Stage 5.1B adds
none of it. No other codec, platform or shader path was started.
