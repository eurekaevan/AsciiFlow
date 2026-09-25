# Stage 5.2C-3 — AV1 Main 10-bit VAAPI production encode

Status: **qualified on the tested Intel Arc Meteor Lake Linux render node** for
explicit BT.709 SDR P010LE input → AV1 Profile0 (Main), 10-bit 4:2:0 VAAPI/MP4
output. Select it with `--output-codec av1 --output-bit-depth 10`. The default
remains H.264 8-bit; `--output-codec av1` alone remains AV1 8-bit. There is no
implicit bit-depth conversion, HDR, software AV1 encode or cross-device claim.

## Capability and architecture

`OutputVideoRequirements` records AV1, `Av1Main`, depth 10, P010LE and BT.709
SDR separately. AV1 Profile0/Main covers both 8- and 10-bit 4:2:0; there is no
invented `Av1Main10` profile. The 10-bit VAAPI encode fact is independent of
AV1 Main8 and HEVC Main10. On iHD 26.1.5, libva reports
`VAProfileAV1Profile0 : VAEntrypointEncSlice`; the application additionally
queries that entrypoint for `VA_RT_FORMAT_YUV420_10`. FFmpeg 8.1.2 exposed
`av1_vaapi`, opened it with an encoder-owned P010 `hw_frames_ctx`, accepted
P010 frames and produced AV1 Main/yuv420p10le MP4. A one-frame FFmpeg P010
smoke and actual application encodes both passed. The AV1 baseline continues
to use the existing `global_quality=25`, GOP 250, no B frames and async depth
2; no rate-control tuning or quality comparison was attempted.

The encoder uses the existing generic P010 pool, staged upload, FFmpeg frame
ownership, packet drain, mux and Vulkan→VAAPI output interop. The interop crate
has **no AV1-specific branch**. Auto prefers full P010 output interop when
qualified and may replan once to staged P010 on an initialization failure;
explicit interop remains strict. A running pipeline never changes codec,
bit depth or pixel path after failure. The Intel AV1 encoder rejected 64×64
with reported constraints width 128–8192, height 96–8192; 128×96 and
128×128 one-frame probes succeeded. Geometry is still tested by actual
encoder initialization, not a hardcoded vendor table.

## Surface and pixel evidence

At 128×128 the **AV1 encoder-owned** DRM PRIME descriptor was one object of
49,152 bytes and two one-plane layers: R16 Y offset 0/pitch 256; GR32 UV
offset 32,768/pitch 256; modifier `0x0100000000000009`. The HEVC Main10
encoder-owned descriptor at the same geometry was identical. The C-1
diagnostic P010 surface used the same one-object, R16/GR32 and modifier
layout family; its published 64×64 and 1920×1080 pitches/offsets vary with
geometry. Each actual descriptor is validated on import; none is assumed
equal from codec identity.

The real AV1 encoder-owned surface was downloaded **before** submission and
compared byte-for-byte against the staged Vulkan P010 reference. All 30
frames matched, including 329,334 active 10-bit samples not divisible by
four. The 3000-frame full-path test also matched every pre-encode surface,
with 32,933,400 such samples. This is a pre-encode parity claim, not a claim
that lossy decoded AV1 pixels equal their input. The 1080p output decoded as
AV1 Main/yuv420p10le/BT.709; its first decoded frame contained 1,470,243
non-four-aligned codes among 3,110,400. A 300-frame decode-back had strictly
increasing PTS from 0 through 153,088 and no decoder errors.

## Production and lifecycle gates

One-, two-, three-, five-, 49- and 301-frame conversions decoded back to their
exact counts. An empty encoder wrote an MP4 `ftyp`/`moov` trailer, with no
decodable video stream as expected. HEVC Main10→AV1 10-bit, AV1 10-bit→AV1
10-bit, software decode→Vulkan P010→staged encode, and software decode→CPU
P010→staged encode all passed. A 90-frame full conversion with FreeType copied
two AAC tracks. Each track retained all 142 compressed packet hashes, PTS,
DTS, durations and sizes; `jpn`/`eng` language and default/forced dispositions
were unchanged. A separate 90-frame CPU/staged conversion copied one AAC track
with the same packet records.

The 3000-frame actual CLI full and staged paths both completed and decoded
back to 3000 frames. The full encoder-owned parity/stress test measured FDs
before 4, active baseline 36, peak 45, after 4; staged stress measured
before 4, active baseline 21, peak 25, after 4. No surface exhaustion,
deadlock or frame loss was observed. FFmpeg frame references, not packet
counts, continue to control surface reuse. AV1 10-bit surfaces were rejected
by AV1 Main8, HEVC Main10/Main8 and H.264 encoder contexts; the same-format
HEVC Main10 case hit the exact `AVHWFramesContext` guard. The instrumentation
on all 1080p paths observed 300 submitted frames, 300 packets, zero send
EAGAIN, 300 receive EAGAIN and a submitted-minus-received peak of two. These
are packet-level observations, **not** the driver's internal queue depth.

Deterministic tests cover independent AV1 10-bit facts, strict explicit
interop, encoder/pool/acquire/map/import initialization faults, staged auto
replan, foreign-context rejection and send/receive/drain/mux root-cause
propagation. Real AV1+AAC mux injection cancelled workers without replacing
the root cause. Interrupting a running 1080p/3000 full conversion with SIGINT
exited 130, preserved an existing target byte-for-byte and removed staging;
a subsequent five-frame conversion succeeded. Generic output-import, queue,
fence and surface-reuse fault coverage from C-1 remains applicable and its
encoder-independent P010 parity smoke passed.

Khronos Vulkan Validation ran on the 30-frame encoder-owned parity test and a
1920×1080/300-frame full conversion. The test's Validation error count was
zero; no VUID, Validation or synchronization errors appeared in the CLI log.
All 133 generated SPIR-V modules passed `spirv-val --target-env vulkan1.3`.

Retained 8-bit outputs on the same 1920×1080/50 source and arguments had the
unchanged SHA-256 values below. The HEVC Main10 1080p reference also retained
its prior hash, demonstrating no C-2 output regression:

| Output | SHA-256 |
| --- | --- |
| H.264 NV12 | `3181fa9775403a2f30e60157797adf370927b4196c960f59d1b5084203d42bef` |
| HEVC Main NV12 | `d4b13786c968e43a502845df46d92dc2dbc951e5e2ef69e4313d5c541664208b` |
| AV1 Main NV12 | `f044a327c093b8489e03f20c98deab8f80e99f01092fe35595b4f22796c7c534` |
| HEVC Main10 P010 | `83f4377557d77ffd48cf32580ea454132fef5bc837d6473a2723516ddac5f22a` |

## 1080p/300-frame benchmark

Release `encode-characterization` build, Validation off, width 80, built-in
font, color on, audio off, VAAPI decode/input interop and Vulkan processing.
The HEVC Main10 input is the retained 1920×1080/300-frame BT.709 SDR
true-low-bit gradient. AV1 Main8 used the benchmark-only offline 10→8 H.264
control derived from that source; this conversion is **not** a production
feature. Each path had a 30-frame warm-up and three 300-frame runs:

| Path | FPS runs / median | Pipeline latency ms runs / median |
| --- | --- | --- |
| AV1 Main8 full | 431.61, 412.34, 435.65 / **431.61** | 24.482, 25.750, 24.299 / **24.482** |
| HEVC Main10 full | 447.36, 447.27, 445.30 / **447.27** | 23.607, 23.603, 23.769 / **23.607** |
| AV1 10-bit staged | 159.65, 161.54, 158.58 / **159.65** | 67.091, 66.332, 67.728 / **67.091** |
| AV1 10-bit full | 266.83, 357.52, 382.71 / **357.52** | 29.708, 26.637, 27.505 / **27.505** |

Because AV1 full varied strongly in that sequential series, staged and full
were then alternated for five further 300-frame runs each:

| AV1 10-bit path | FPS runs / median | Latency ms runs / median |
| --- | --- | --- |
| staged | 157.66, 160.98, 158.41, 160.02, 151.93 / **158.41** | 68.062, 66.531, 67.612, 67.086, 70.537 / **67.612** |
| full | 366.12, 337.52, 327.28, 356.46, 366.41 / **356.46** | 26.333, 24.970, 27.665, 28.305, 28.802 / **27.665** |

On this input the interleaved full median is **2.25×** staged throughput,
or 125% higher, with about 59% lower average pipeline latency. Approximate
whole-process CPU percentages from user+system time divided by process elapsed
were about 131% staged and 109% full at their medians; setup/teardown is
included, and this is not per-device utilization. All 16 AV1 10-bit 300-frame
outputs from these runs were byte-identical, SHA-256
`106ac4aa9946c4814545ffc35eed0f094c6703942fe67f1e0ba34c18868301c8`.

Representative per-frame means (not additive timing components): the AV1
staged interleaved median-FPS run recorded Host memcpy 0.649 ms, hwupload
3.273 ms, send-frame CPU wall 2.218 ms and mux video packet write 0.188 ms.
In the AV1 full median-FPS run, surface acquire was 0.013 ms, DRM map 0.013
ms, DMA-BUF import 0.013 ms, GPU buffer→image 0.151 ms, fence wait 0.761 ms,
send-frame CPU wall 2.743 ms and mux write 0.168 ms. AV1 Main8's GPU copy
control was 0.076–0.077
ms; HEVC Main10's was 0.145–0.148 ms. These are device timestamps for GPU
copy and CPU wall for lifecycle/wait, and cannot be summed to derive FPS.
The same 10-bit input was used for HEVC Main10 and both AV1 10-bit paths;
the 8-bit control has different input encoding. No quality, compression
efficiency, or pure bit-depth encoder-cost claim follows from these figures.

## Reproduction and remaining boundary

See [testing.md](testing.md) for the Intel opt-in parity, stress and fault
commands. The 128×128 AV1 input used for parity was generated by the
production AV1 10-bit encoder from the retained 128×128 HEVC Main10 SDR
source; this ensured explicit left-sited chroma metadata, unlike a libaom
intermediate whose chroma location was unspecified. The 3000-frame input is
a stream-copy loop of that 30-frame file. Hardware capability remains
runtime-probed. AV1 10-bit output is qualified only for this observed
driver/FFmpeg/Vulkan stack. HDR/PQ/HLG/BT.2020, other chroma/depth formats,
software AV1 encode, production bit-depth conversion, new tuning controls,
other vendors and Stage 5.3 remain outside this stage.
