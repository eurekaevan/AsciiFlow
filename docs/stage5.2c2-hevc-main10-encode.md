# Stage 5.2C-2 — HEVC Main10 VAAPI production encode

Status on the tested Intel Arc Meteor Lake render node: **qualified for
explicit BT.709 SDR P010LE input → HEVC Main10 VAAPI/MP4 output**. This is
not HDR, a general device guarantee, or a 10→8 converter. AV1 10-bit output
was a later [Stage 5.2C-3](stage5.2c3-av1-10bit-encode.md) qualification.
`--output-bit-depth` defaults to `8`; Main10 requires both
`--output-codec hevc --output-bit-depth 10`. An unqualified profile, format,
driver, import or encoder fails before output commit. Auto may replan from
P010 full output interop to P010 staged upload once; explicit `on` remains
strict and no mid-stream fallback exists.

## Contract and physical capability

The planner records codec HEVC, profile Main10, depth 10, 4:2:0, P010LE and
BT.709 SDR independently of input depth. It accepts HEVC Main10 or AV1 Main
10-bit input with explicit BT.709 primaries/transfer/matrix, limited or full
range and left-sited chroma. Other depths, HDR and implicit 8↔10 conversion
are rejected. Main8/NV12 and Main10/P010 have separate decode, encode,
frames/upload and input/output interop facts. An initialization failure only
invalidates the exact fact needed for the attempted path.

On this machine `vainfo --display drm --device /dev/dri/renderD128` exposed
`VAProfileHEVCMain10 : VAEntrypointEncSlice` with the 10-bit 4:2:0 render
target. FFmpeg 8.1.2 exposed `hevc_vaapi`; the application successfully opened
it with profile Main10 and an encoder-owned P010 `hw_frames_ctx`. The retained
VAAPI baseline remains CQP/QP20, no B frames, GOP 250, async depth 2; it is
not a quality-equivalence claim against Main8. The driver rejected 128×96
encode geometry, so qualification begins at 128×128. The production pool is
owned by the actual encoder, never the C-1 diagnostic pool. FFmpeg frame refs,
not received-packet counts, govern reuse. Cross-context Main8/Main10 surface
submission was rejected by the existing exact frames-context guard.

At 128×128 the encoder-owned P010 DRM PRIME descriptor had one object of
49,152 bytes, two one-plane layers, modifier `0x0100000000000009`, R16 Y at
offset 0/pitch 256 and GR32 UV at offset 32,768/pitch 256. This matches the
C-1 diagnostic layout family; the import still validates each **actual**
descriptor, rather than assuming a codec-specific layout.

## Pixel and bitstream gates

The opt-in `main10_encoder_owned_full_interop_30_frame_parity` test compared
every pre-encode P010 byte from the real encoder-owned writable surface with
the staged Vulkan reference, including 187,829 samples whose active 10-bit
code is not divisible by four. It also compared a staged upload/download of
the same reference. The 3000-frame variant compared every surface before
submission and observed 18,782,900 non-four-aligned pre-encode samples.
These are **pre-encode** exactness checks; lossy HEVC decode pixels are not
required to equal their source.

Actual HEVC output decoded without errors. `ffprobe` reported Main 10,
`yuv420p10le`, BT.709 primaries/transfer/matrix, expected 128×128 or
1920×1080 geometry and exact frame counts. HEVC SPS trace reported profile
IDC 2 and luma/chroma `bit_depth_minus8=2`. The first decoded 1080p Main10
benchmark frame had 1,950,658 non-four-aligned samples among 3,110,400, so
the bitstream carries more than 8-bit values shifted left. The 300-frame
decode-back had PTS from 0 through 153,088, strictly increasing. Short drain
cases 1, 2, 3, 5, 49 and 301 each decoded to their exact submitted count;
the zero-frame encoder wrote an MP4 `ftyp` and `moov` trailer but, as expected,
has no decodable video stream.

Production combinations run successfully: HEVC Main10 software decode →
CPU P010 → staged VAAPI encode; HEVC Main10 VAAPI decode → P010 input interop
→ Vulkan → P010 output interop → encoder-owned surface; and AV1 Main 10-bit
input → HEVC Main10 output. The 90-frame full path with FreeType retained two
AAC tracks; each track's 142 compressed packet records (payload SHA-256,
PTS, DTS, duration and size) matched the input exactly, as did `jpn`/`eng`
language and default/forced dispositions. A separate CPU/P010 staged run
retained one AAC track with the same 142 packet records.

## Lifecycle, faults and regressions

Both actual production 3000-frame paths completed and decoded back to 3000
Main10 frames. The full encoder-owned parity/stress test measured FD count
before 4, active baseline 36, peak 41, after 4. The staged encode/FD test
measured before 4, active baseline 21, peak 23, after 4. No surface-pool
exhaustion, frame loss or deadlock was observed. The encoder's observable
submitted-minus-received peak in the 1080p run was 2, with zero send EAGAIN
and 300 receive EAGAIN; this is a packet-level proxy, **not** the driver's
internal queue depth.

A separate 300-frame full-path control using the same instrumentation found
Main8 and Main10 each submitted and received 300/300, with zero send EAGAIN,
300 receive EAGAIN and a peak submitted-minus-received count of two. No new
observable retention or backpressure rule was needed for Main10. Their
send-frame CPU-wall means were 2.282 and 2.324 ms/frame respectively in that
single run; the differently encoded input sources and run-to-run variation
do not support a codec-cost conclusion from this difference.

Deterministic tests cover Main10 encoder and frames-pool initialization,
surface-acquire/DRM-map/DMA-BUF-import startup replan, profile-isolated
capability disabling, strict explicit interop, foreign-surface rejection,
send/receive/drain/mux error propagation, and the zero-frame trailer. The existing
C-1 P010 output fault test (external image, queue submit, fence and reuse)
and 3000-frame diagnostic FD test both passed after integration. Real Main10
send/receive/drain and Main10+AAC mux injection preserved the original error
cause and cancelled workers. A running
1080p/3000 full conversion interrupted by SIGINT exited 130, left the
pre-existing target byte-identical, removed staging, and a subsequent run
succeeded. There is no mid-stream staged or 8-bit fallback.

Khronos Validation was enabled for the 30-frame encoder-owned parity test
and a 1920×1080/300-frame full conversion; the test's Validation error count
was zero and no Validation/VUID/synchronization messages appeared. All 133
generated SPIR-V modules passed `spirv-val --target-env vulkan1.3`; no new
P010 shader was added. The Stage 5.2B HEVC/AV1 P010 descriptor/hwdownload
test passed. Re-running the retained 8-bit H.264, HEVC and AV1 full-path
outputs produced their previous SHA-256 values respectively:

| Output | SHA-256 |
| --- | --- |
| H.264 NV12 | `3181fa9775403a2f30e60157797adf370927b4196c960f59d1b5084203d42bef` |
| HEVC Main NV12 | `d4b13786c968e43a502845df46d92dc2dbc951e5e2ef69e4313d5c541664208b` |
| AV1 Main NV12 | `f044a327c093b8489e03f20c98deab8f80e99f01092fe35595b4f22796c7c534` |

## 1080p, 300-frame three-way benchmark

Release build with `encode-characterization`, Validation off, Intel Arc MTL,
30-frame warm-up per path, three 300-frame runs each. Main10 input was made by
nearest-neighbor scaling the true-low-bit 128×128 lossless gradient to
1920×1080, then encoding Main10; its first decoded frame contained 2,324,189
non-four-aligned values out of 3,110,400. The Main8 control was derived from
that source by an **offline benchmark-only** 10→8 conversion and H.264 High
encode (the production application does not expose this conversion). All
paths used VAAPI decode, input interop, Vulkan ASCII width 80, built-in font,
color on, audio off, and HEVC VAAPI output. Staged and full Main10 consumed
the *same* input file; their six output MP4s had identical SHA-256
`83f4377557d77ffd48cf32580ea454132fef5bc837d6473a2723516ddac5f22a`.

| Path | FPS runs | Median FPS | CPU % runs / median | Latency ms runs / median |
| --- | --- | ---: | --- | --- |
| HEVC Main8 full | 436.54, 443.45, 451.31 | 443.45 | 103, 107, 104 / 104 | 24.352, 23.998, 23.586 / 23.998 |
| HEVC Main10 staged | 199.42, 204.83, 199.85 | 199.85 | 136, 135, 135 / 135 | 53.563, 52.220, 53.371 / 53.371 |
| HEVC Main10 full | 444.95, 455.25, 453.21 | 453.21 | 101, 105, 101 / 101 | 23.656, 23.143, 23.168 / 23.168 |

Main10 full was 2.27× the staged median throughput on this fixture; CPU
percentage fell by 34 points. Representative *per-frame means*, not additive
device-stage totals: staged hwupload 3.021 ms and Vulkan Host memcpy 0.585 ms;
full surface acquire 0.003 ms, output DRM map 0.014 ms, DMA-BUF import
0.003 ms, buffer→image GPU timestamp 0.166 ms and output fence wait 0.367 ms.
Full encode send/receive CPU wall was 2.186 ms/frame. The Main8 comparison
is a control of the pipeline, not an exact bit-depth-cost or quality comparison:
the benchmark inputs use different encoders and lossy representations, and the
upsampled gradient has low scene complexity. The raw runs above are retained;
no portability or compression-efficiency conclusion is implied.

## Reproduction and boundaries

The 128×128 qualification input can be regenerated from the checked-in
true-low-bit fixture; the driver cannot encode the original 64×64 geometry:

```bash
ffmpeg -hide_banner -loglevel error \
  -i tests/fixtures/codecs/hevc-main10-sdr-gradient.mp4 \
  -vf scale=128:128:flags=neighbor -c:v libx265 -pix_fmt yuv420p10le \
  -x265-params lossless=1:log-level=error -frames:v 30 -an \
  /tmp/asciiflow-main10-128.mp4
ffmpeg -hide_banner -loglevel error -stream_loop 99 \
  -i /tmp/asciiflow-main10-128.mp4 -frames:v 3000 -c copy \
  /tmp/asciiflow-main10-3000.mp4
```

See [testing.md](testing.md) for opt-in tests. Exact hardware behavior is
qualified only for the observed driver/FFmpeg/Vulkan stack. At this stage's
seal, AV1 10-bit encode was not yet qualified. HDR/PQ/HLG/BT.2020, software
Main10, bitrate/preset UI, 8→10 or 10→8 production conversion and new interop
architecture remain out of scope.
