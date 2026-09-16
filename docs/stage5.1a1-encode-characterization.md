# Stage 5.1A.1: encode-side characterization and AV1 go/no-go

Status: Intel Arc Meteor Lake characterization completed on 2026-09-16. No
production AV1 output was enabled. This report follows, and does not replace,
[Stage 5.1A](stage5.1a-hevc-encode-validation.md).

## Measurement boundary

The retained final benchmark uses one regenerated 1920×1080, 50 fps, 300-frame
H.264 testsrc2 input (SHA-256
`6b47b510c4a8f604e6404b1fbbc5f68bdaa77043976acdad0153ef83524b6e34`),
VAAPI decode, VAAPI→Vulkan input interop, Vulkan ASCII,
width 80, color, built-in 8×8 font, audio off, Release and validation off.
Each used a 30-frame warm-up and three complete 300-frame runs. `--decode vaapi
--backend vulkan --encode vaapi --vaapi-vulkan-input-interop on` forced the
common path. The output-interoperability switch alone distinguishes staged
from full HEVC. `/usr/bin/time` measured process CPU including startup; CLI FPS
excludes capability-probe startup. Runs were sequential; no run was selected
for being fastest. This is the same workload specification and one input
across all paths, but **not byte-identical to the Stage 5.1A source**; no
cross-stage FPS delta is asserted. `ffprobe` reports BT.709 color-space and
limited range for the regenerated source, but unknown primaries/transfer tags;
the original Stage 5.1A source had full BT.709 tags. This is another reason not
to treat the two stages as identical-input performance runs. An additional
five-run interleaved H.264
on/off series tested instrumentation overhead because sequential batches gave
different signs. All final run logs, CPU readings and outputs remain under
ignored `target/stage51a1-evidence/`.

The `encode-characterization` Cargo feature wraps the actual FFmpeg
`avcodec_send_frame` and `avcodec_receive_packet` calls and the mux worker's
`av_interleaved_write_frame` calls. It also counts calls, packet bytes, EAGAIN,
per-frame send retries and the peak difference between submitted frames and
received packets. Ordinary Release does none of these new per-frame timing
calls. Existing surface acquire, DRM map, image create/import/bind, ownership
bookkeeping, output submit and fence wait remain CPU wall scopes. The
buffer→external-image copy remains a Vulkan GPU timestamp. A fence wait is
the host's wait for our Vulkan output copy, **not** a measure of VAAPI encoder
readiness. FFmpeg send wall can include a driver/codec wait but does not expose
the internal cause of that wait.

All duration cells below are ms/frame, medians of the three *per-run means*,
not per-frame p50. The CLI currently exposes per-run mean pipeline latency,
not a per-frame latency distribution or p95. Asynchronous mux, encoder and GPU
scopes overlap and must not be summed or subtracted to invent a missing scope.

## Raw formal runs

| Path | Run 1 FPS / CPU / latency | Run 2 FPS / CPU / latency | Run 3 FPS / CPU / latency | Median FPS | FPS min–max |
| --- | --- | --- | --- | ---: | ---: |
| H.264 full, instrumentation off | 516.40 / 57% / 20.148 | 527.02 / 59% / 20.096 | 525.59 / 58% / 20.081 | **525.59** | 516.40–527.02 |
| H.264 full, instrumentation on | 513.55 / 57% / 20.539 | 513.23 / 57% / 20.616 | 520.64 / 58% / 20.315 | **513.55** | 513.23–520.64 |
| HEVC staged, instrumentation on | 458.21 / 137% / 23.031 | 465.32 / 138% / 22.692 | 462.39 / 137% / 22.923 | **462.39** | 458.21–465.32 |
| HEVC full, instrumentation on | 493.03 / 57% / 21.479 | 491.20 / 60% / 21.475 | 499.02 / 58% / 21.281 | **493.03** | 491.20–499.02 |

Latency values in this table are the CLI's mean decode-call-start to encode
acceptance per run. In this sequential three-run batch instrumentation-on H.264
was 2.29% slower by median. The five interleaved pairs were:

| Build | Run 1 | Run 2 | Run 3 | Run 4 | Run 5 | Median | Min–max |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Off | 503.93 | 515.67 | 525.20 | 517.93 | 528.48 | **517.93** | 503.93–528.48 |
| On | 515.03 | 517.48 | 525.32 | 527.35 | 521.96 | **521.96** | 515.03–527.35 |

The interleaved on median was 0.78% higher. Together these comparisons show
**no demonstrated ≥3% instrumentation penalty**, but the sign changes with
run order; claiming a precise sub-percent cost would overstate the evidence.

Selected unaggregated call/synchronization scopes from the retained logs
(ms/frame; `—` means that scope is absent in the staged path):

| Path / run | Send | Receive | Hwupload | Surface acquire | DRM map | DMA-BUF import | GPU copy | Fence wait | Mux write |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| H.264 full 1 | 1.910 | 0.001 | — | 0.006 | 0.007 | 0.003 | 0.066 | 0.480 | 0.014 |
| H.264 full 2 | 1.896 | 0.001 | — | 0.003 | 0.008 | 0.004 | 0.064 | 0.399 | 0.016 |
| H.264 full 3 | 1.881 | <0.001 | — | 0.004 | 0.008 | 0.004 | 0.067 | 0.448 | 0.016 |
| HEVC staged 1 | 0.439 | 0.001 | 1.455 | — | — | — | — | — | 0.017 |
| HEVC staged 2 | 0.411 | <0.001 | 1.449 | — | — | — | — | — | 0.015 |
| HEVC staged 3 | 0.412 | 0.001 | 1.456 | — | — | — | — | — | 0.018 |
| HEVC full 1 | 1.991 | 0.001 | — | 0.005 | 0.013 | 0.004 | 0.066 | 0.430 | 0.016 |
| HEVC full 2 | 1.999 | 0.001 | — | 0.015 | 0.015 | 0.004 | 0.067 | 0.400 | 0.014 |
| HEVC full 3 | 1.965 | 0.001 | — | 0.008 | 0.009 | 0.005 | 0.068 | 0.477 | 0.015 |

## Directly observed scopes

| Metric | H.264 full | HEVC staged | HEVC full | Clock / caveat |
| --- | ---: | ---: | ---: | --- |
| Throughput | 513.55 | 462.39 | 493.03 | FPS, median of complete runs |
| Process CPU | 57% | 137% | 58% | `/usr/bin/time`, median |
| Mean pipeline latency | 20.539 | 22.923 | 21.475 | CPU wall, median of run means |
| Backend wall | 1.977 | 2.965 | 1.940 | CPU wall, concurrent stages |
| Encode wall | 1.902 | 2.124 | 1.997 | CPU wall, includes submission work |
| Encoder surface acquire | 0.004 | N/A | 0.008 | CPU wall; no directly observable pool occupancy |
| Output DRM map/export | 0.008 | N/A | 0.013 | CPU wall |
| Output capability query | 0.011 | N/A | 0.010 | CPU wall |
| Output image create | 0.002 | N/A | 0.002 | CPU wall |
| Output DMA-BUF import | 0.004 | N/A | 0.004 | CPU wall |
| Output memory bind | <0.001 | N/A | <0.001 | CPU wall, rounded CLI value |
| Output ownership record | 0.001 | N/A | 0.001 | CPU wall |
| Output image destroy | 0.009 | N/A | 0.008 | CPU wall |
| Output GPU copy | 0.066 | N/A | 0.066 | Vulkan timestamp |
| Output queue submit | 0.018 | N/A | 0.018 | CPU wall |
| Output copy fence wait | 0.448 | N/A | 0.430 | CPU wall; Vulkan completion only |
| Host invalidate / readback copy | N/A | 0.091 / 0.345 | N/A | CPU wall |
| Host→VAAPI hwupload | N/A | 1.455 | N/A | CPU wall |
| `avcodec_send_frame` | 1.896 | 0.412 | 1.991 | CPU wall; includes opaque FFmpeg/driver scheduling |
| `avcodec_receive_packet` | 0.001 | 0.001 | 0.001 | CPU wall |
| Encoder drain | 0.006 | 0.006 | 0.006 | CPU wall; overlaps send/receive |
| Mux video packet write | 0.016 | 0.017 | 0.015 | CPU wall in asynchronous mux worker |
| Mux queue send API | 0.002 | 0.003 | 0.002 | CPU wall; not pure blocking time |
| Interleaver flush / trailer | <0.001 / <0.001 | <0.001 / <0.001 | <0.001 / <0.001 | CPU wall, rounded; not literally zero |

The combined output import lifecycle has multiple independent operations as
shown; no inferred “total interop cost” is presented. H.264 and HEVC have
essentially the same descriptor, CPU import scopes and GPU copy time. HEVC's
full-path fence wait is 0.018 ms/frame lower by the run medians; the GPU copy
is equal to the displayed precision. The acquire and DRM-map medians differ by
0.004 and 0.005 ms/frame, respectively; these small deltas do not establish a
codec-specific interop cost.

| Counter, each 300-frame run | H.264 full | HEVC staged | HEVC full |
| --- | ---: | ---: | ---: |
| Frames submitted / packets received | 300 / 300 | 300 / 300 | 300 / 300 |
| Received video packet bytes | 4,337,837 | 3,790,312 | 3,790,312 |
| Output MP4 bytes | 4,339,971 | 3,792,518 | 3,792,518 |
| Send EAGAIN / max retries per frame | 0 / 0 | 0 / 0 | 0 / 0 |
| Receive EAGAIN | 300 | 300 | 300 |
| Peak submitted-minus-packets | 2 | 2 | 2 |

Receive EAGAIN is the expected “no more packet available now” at the end of
each drain poll, **not** evidence of backpressure. No send-side EAGAIN occurred.
The peak difference is an observable application-level proxy; one packet need
not correspond to exactly one frame on every encoder, and it is not the
hardware's queue depth. FFmpeg's internal surface-pool occupancy and wait
count are opaque here. The measured acquire call remained below
0.01 ms/frame at median in both full paths, with no observed sustained HEVC
surface-pool wait.
Packet size is a mux/load diagnostic, not a compression-quality comparison.

## Interpretation, with limits

The retained HEVC full path was **6.63%** faster than HEVC staged (493.03 versus
462.39 FPS), while process CPU fell from 137% to 58%. Staging visibly incurs
host invalidation/readback copy and 1.455 ms/frame of VAAPI upload wall. Full
interop substitutes an approximately 0.066 ms device copy, small CPU import
scopes and a 0.430 ms host-visible Vulkan fence wait. These ranges overlap
other work, so they do not algebraically “explain” every throughput millisecond.
The HEVC send call is much shorter in staged mode (0.412 versus 1.991 ms),
consistent with readiness/synchronization being encountered during the earlier
hwupload instead. It would be wrong to call the full-path 1.991 ms pure encode
engine execution. The earlier run against the original Stage 5.1A source
observed an 8.99% full-path gain, versus 9.56% in Stage 5.1A; the inputs and
timing boundaries differ, so these are separate observations.

H.264 full exceeded HEVC full by **4.00%** (513.55 versus 493.03 FPS). The
direct send-call median was 1.896 versus 1.991 ms/frame, a 0.095 ms difference;
receive, mux, surface acquire, import and GPU copy differed little. The
reciprocal-throughput gap is about 0.081 ms/frame. This is evidence that the
observed difference is on the encoder/driver scheduling side, **not** in the
shared output interop. It does not prove saturation of a particular Intel media
engine. No isolated encoder benchmark was run: the existing public sink takes
owned Host NV12 frames, so synthetic allocation/copy and hwupload would make
such a test a different workload without a clean encoder-only boundary. No
GPU-engine utilization or frame-level p95 was captured. Historical Stage 5.0
HEVC/AV1-input ~404 FPS runs used different input codecs and are not used as a
common-ceiling comparison.

## Correctness, stress and dimension boundary

The instrumented and uninstrumented H.264 output from the earlier run on the
original Stage 5.1A input matched its baseline SHA-256 exactly:
`f789cb02f23a2abeeb6eca4f4692d25af8fb74117b05ea6ccd9bac4ce813ec96`.
For the retained regenerated input, H.264 on/off outputs were byte-identical
(SHA-256 `3181fa9775403a2f30e60157797adf370927b4196c960f59d1b5084203d42bef`),
and staged/full HEVC outputs were byte-identical (SHA-256
`d4b13786c968e43a502845df46d92dc2dbc951e5e2ef69e4313d5c541664208b`).
An additional run omitting `--output-codec` produced the same H.264 SHA-256
as explicit H.264, confirming the default selection and bitstream stayed
unchanged on this input.
HEVC output fully decoded as Main/yuv420p, 1920×1080, 300 frames. A
30-frame full-interop H.264 and HEVC smoke with
`ASCIIFLOW_VULKAN_VALIDATION=1` completed without Validation/VUID output.

After encoder EAGAIN handling and mux queue instrumentation, both output codecs
also completed a fresh 3000-frame 1080p full-interop stress from the
regenerated input looped ten times. Each submitted/received 3000 frames and
produced a decodable 3000-frame MP4. That stress source is **not** the formal
300-frame benchmark sample, so its FPS (H.264 538.28; HEVC 521.31) is not
compared to the formal matrix. It exists to recheck queue/drain lifetime after
code changes.

On the qualified iHD driver, FFmpeg logs a coded-surface constraint of width
128–16384 and height 128–12288 for HEVC. Native encoder initialization rejects
64×128 and 128×64. Visible 126×128 and 128×126 initialize (and 126×128
completed a three-frame conversion), consistent with coded-surface padding to
128. The diagnostic now includes requested geometry and NV12, but no global
128-pixel rule is hard-coded in the portable planner. The capability probe
fails before output creation for the unambiguous subminimum cases.

The first measurements against the byte-identical Stage 5.1A source used
temporary `/tmp` artifacts, which were cleared by a host environment reset.
Their original run FPS were H.264 off 516.92/514.84/519.06, H.264 on
512.95/521.86/527.42, HEVC staged 451.77/452.84/443.39 and HEVC full
483.23/493.14/492.37. The original full logs are no longer available and
their mux-write scopes preceded the timing-boundary correction. The retained
benchmark and stress source, outputs, logs and `/usr/bin/time` records are under
ignored `target/stage51a1-evidence/`; they are local evidence, not fixtures to
commit. The regenerated source is not represented as byte-identical to the
Stage 5.1A source.

## AV1 encode feasibility and decision

The current Intel iHD VAAPI 1.23 driver reports `VAProfileAV1Profile0 :
VAEntrypointEncSlice`; this FFmpeg build exposes `av1_vaapi` and its VAAPI pixel
format. A **feature-gated diagnostic only** opened `av1_vaapi` with Main/profile
0, VAAPI NV12 frames and async depth 2. Unlike H.264/HEVC, `av1_vaapi` does not
accept their `qp` AVOption, so diagnostic setup deliberately used the encoder's
own default quality; no quality or performance comparison was made.

The AV1 encoder-owned 1920×1080 surface exported as one 3,194,880-byte object,
modifier `0x0100000000000009`, Y `R8` offset 0 pitch 1920 and UV `GR88`
offset 2,088,960 pitch 1920: the same observed layout as the H.264/HEVC
surfaces. The diagnostic Vulkan buffer→AV1-surface copy took 0.089 ms by
timestamp on the final rerun (0.122 ms on the earlier run) and passed Khronos
Validation with zero reported errors. These single copies are feasibility
checks, not a throughput benchmark. No production planner candidate, CLI value,
encoder factory path, mux integration or AV1 packet smoke was added.

**GO for a separately scoped Stage 5.1B implementation experiment**, limited
to AV1 Profile0, 8-bit 4:2:0 NV12, VAAPI encode and the existing Stage 3B
interop; H.264 stays default. The current evidence supports reusing the surface
memory model and avoids a P010 prerequisite. This is not production readiness:
Stage 5.1B must first prove actual AV1 packet output, MP4 extradata/timestamps,
rate-control configuration and decoder parity, then repeat failure,
cancellation, long-run, FD and Validation qualification before claiming
support. No AV1 feature is shipped by Stage 5.1A.1.
