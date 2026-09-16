# Stage 5.1A HEVC hardware encode validation

Status: **complete and qualified on Intel Arc Meteor Lake** (2026-09-16).

Scope is deliberately narrow: MP4 HEVC Main, 8-bit 4:2:0, VAAPI encode. H.264
remains the default. AV1 encode, software HEVC/libx265, Main10/P010, HDR,
rate-control UI, B-frame tuning and new vendors/platforms are not included.

## H.264 output specialization audit

| Class | Finding and disposition |
| --- | --- |
| A — legitimate codec configuration | `libx264`, `h264_vaapi`, H.264 codec ID and H.264 diagnostics remain native codec mappings; HEVC adds `hevc_vaapi` and HEVC Main without duplicating encoder lifecycle |
| B — accidental generic assumptions | encoder allocation/open/send/receive/drain diagnostics and plan labels were made codec-aware; MP4-only wording now describes the container, not an H.264 restriction |
| C — Stage 3B interop | no codec branch was added; it still consumes the exact encoder-owned NV12 `AVHWFramesContext` and imports its actual DRM descriptor |
| D — mux/container | MP4 ownership, extradata copy, trailer, audio routing and transactional staging remain shared; input codec tags are not copied |
| E — tests | Stage 3B parity/stress helpers now accept the output codec and validate the decoded output identity/count/timestamps |

## Portable contract and implementation

`--output-codec h264|hevc` is independent of `--encode auto|software|vaapi`.
The default is H.264. Core records an `OutputVideoRequirements` containing codec,
profile, bit depth, chroma, dimensions and frame rate, and stores it in the
selected `PipelinePlan`. No FFmpeg or libva identity enters Core.
The profile requirement is `Some(Main)` for HEVC; H.264 leaves profile selection
to its existing encoder configuration rather than promising High on every host.

The qualified HEVC requirement is Main, 8-bit, 4:2:0 NV12. Media maps it to
`hevc_vaapi`, explicitly sets HEVC Main, attaches the same kind of VAAPI NV12
frames context used by H.264, and retains the existing baseline:

- rate control: CQP, QP 20;
- GOP: 250;
- B-frames: 0;
- async depth: 2.

QP 20 is a repeatable encoder baseline, not a claim of H.264/HEVC quality
equivalence. FFmpeg's MP4 muxer emitted legal `hev1` with 126 bytes of codec
extradata; no tag was hard-coded. The output contains HEVC Main/yuv420p and
decodes completely, establishing usable VPS/SPS/PPS configuration.

Capability is the conjunction of FFmpeg exposing `hevc_vaapi`, the VAAPI driver
exposing HEVC Main + EncSlice, successful NV12 frames-context/encoder open, and
actual output-surface import qualification. `vainfo` observed
`VAProfileHEVCMain : VAEntrypointEncSlice`; Main10/444 advertisements are ignored.
`--capabilities` reports H.264 software/VAAPI and HEVC software/VAAPI separately.

Planner and initialization behavior:

- HEVC + auto selects HEVC VAAPI; unavailable hardware is a hard error, never
  an H.264 substitution.
- HEVC + software fails explicitly: `HEVC software encoding is not implemented`.
- HEVC output interop failure may replan once to Vulkan readback + VAAPI upload.
  Only the HEVC output-interop fact is disabled; H.264 remains supported.
- Explicit output interop fails instead of staging. Runtime failures never
  switch codec or path.
- Explain-plan prints output codec/profile and encoder backend separately.

## Intel correctness qualification

Device: Intel(R) Arc(tm) Graphics (MTL), 0x8086/0x7d55, Mesa ANV Vulkan
1.4.354, driver version 109056008, iHD VAAPI 1.23.

Observed HEVC encoder surface at 1920×1080:

```text
objects: 1
object size: 3,194,880 bytes
modifier: 0x0100000000000009 (I915 4-tiled)
layers: 2
Y:  R8,   offset 0,       pitch 1920
UV: GR88, offset 2088960, pitch 1920
```

The descriptor is byte-for-byte structurally equal to the observed H.264
encoder descriptor on this driver. That is an observation, not an importer
assumption. Stage 3B needed no codec-specific interop change.

Thirty 1080p frames compared these two pre-encode surfaces:

```text
Vulkan output -> Host readback -> VAAPI NV12 upload
Vulkan output -> HEVC encoder-owned DRM PRIME surface
```

Y, UV, PTS and dimensions were exact for every frame. The output fully decoded
as HEVC Main/yuv420p. H.264, HEVC and AV1 1080p inputs each produced valid HEVC
output through full input/output interop. CPU ASCII + staged HEVC and Vulkan +
Host readback + staged HEVC also completed independently. The driver's real
HEVC encode constraint is at least 128×128; the checked-in 64×64 decode fixtures
therefore remain decode fixtures, while encode qualification uses 1080p streams.

FreeType plus HEVC full interop passed. A two-AAC-stream 128×128 test preserved
both compressed payload hashes, Japanese/English language tags and default
dispositions. Single-stream AAC likewise decoded normally. Audio and font paths
remain independent of output codec.

Frame-count/drain qualification passed for 1, 2, 3, 5, 49 and 301 frames. Every
output decoded to exactly N frames without duplicate/trailer loss. H.264 and
HEVC 300-frame outputs had identical packet PTS/duration sequences (0..76544,
step/duration 256 at the MP4 time base) and 6.000 s duration.

The 3000-frame HEVC full-output-interop run performed staged-versus-interop
pre-encode parity on every frame, encoded and decoded all 3000 frames, and
reported FD before/active/max/after **4/36/39/4**. There was no surface pool
exhaustion, stale surface, deadlock, loss or reordering. H.264-owned frames were
rejected by a HEVC encoder context and vice versa.

With `ASCIIFLOW_VULKAN_VALIDATION=1`, descriptor parity, full conversion and the
3000-frame run reported zero validation-counter errors; logs contain no VUID,
synchronization hazard or Validation Error.

Deterministic test-only HEVC send-frame and receive-packet injections preserved
`EncodeRuntime` as the root stage. HEVC output-interop initialization injection
replanned only to staged HEVC. An explicit unsupported software-HEVC run failed
before touching an existing target. Five running full-interop HEVC conversions
were interrupted with SIGINT: all returned 130, retained the destination hash,
removed staging files, kept the parent FD count 4/4, and were followed by an
immediate successful HEVC initialization. The shared transactional-output,
cancellation and no-mid-stream-fallback contracts did not change.

The default H.264 300-frame output remained byte-identical to the Stage 5.0
reference: SHA-256
`f789cb02f23a2abeeb6eca4f4692d25af8fb74117b05ea6ccd9bac4ce813ec96`.

## Formal benchmark

Release, validation off, same H.264 testsrc2 input and processing path,
1920×1080 at 50 fps, 300 frames, width 80, color, built-in font, audio off,
VAAPI decode + input interop + Vulkan. Each case used a 30-frame warm-up and
three complete runs; only output path/codec changed.

| Path | FPS runs | Median FPS | Process CPU | Backend wall | Encode wall | Pipeline latency |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| H.264 full interop | 527.34 / 521.11 / 526.11 | **526.11** | 60% | 2.097 | 1.855 | 19.955 |
| HEVC staged upload | 463.81 / 450.29 / 425.85 | **450.29** | 136% | 3.387 | 2.109 | 21.814 |
| HEVC full interop | 493.33 / 488.40 / 494.81 | **493.33** | 60% | 2.367 | 1.977 | 21.400 |

Times are independently medianed ms/frame. FPS uses the CLI processing interval;
process CPU uses `/usr/bin/time` over startup plus conversion (100% is one core).
Latency is decode-call start to encode acceptance, not reciprocal throughput.

| Output detail | H.264 full | HEVC staged | HEVC full |
| --- | ---: | ---: | ---: |
| Host→VAAPI upload CPU wall | 0 | 1.467 | 0 |
| Surface acquire CPU wall | 0.007 | 0 | 0.006 |
| DRM map CPU wall | 0.007 | 0 | 0.008 |
| Capability query CPU wall | 0.009 | 0 | 0.009 |
| DMA-BUF import CPU wall | 0.004 | 0 | 0.005 |
| GPU buffer→external image | 0.071 | 0 | 0.071 |
| Output submit CPU wall | 0.017 | 0 | 0.018 |
| Output fence wait CPU wall | 0.473 | 0 | 0.522 |
| Encoder submit/receive CPU wall | 1.853 | 0.450 | 1.976 |

GPU copy is a Vulkan timestamp; other rows are CPU wall clocks and overlap.
They must not be summed. In particular, asynchronous encoder submit/receive
wall does not isolate physical encode-engine time. This video-only benchmark
does not expose a separate mux-wall metric; mux work is included in the
end-to-end processing interval.

Full interop improved HEVC throughput by **9.56%** over staged upload and reduced
reported process CPU from 136% to 60%; keeping output interop is materially
useful. The staged runs varied from 425.85 to 463.81 FPS, so this comparison
has more run-to-run noise than the two full-interop paths. H.264 genericization
measured **+1.08%** versus the Stage 5.0 520.49 FPS median, so no regression is
observed. HEVC full interop is 6.23% below this H.264 control. Its output-import
cost is essentially the same; the remaining gap is downstream scheduling/encode
behavior, not a codec-specific interop path.

The H.264 benchmark's observed capability-probe median was 146.726 ms versus
105.369 ms in Stage 5.0. Both encoder/profile/surface paths are now qualified,
but these cross-run measurements do not isolate the cause of the 41.357 ms
difference. Probe time does not enter the reported processing FPS and does not
justify a persistent cache by itself.

Artifacts for this run are under `/tmp/asciiflow-stage51/`, including benchmark
logs/timings/outputs, smoke outputs and cancellation/drain records. They are
temporary host evidence, not checked-in fixtures.

## Stage 5.1B gate

1. Stage 3B output interop is codec-agnostic for qualified NV12 encoder surfaces.
2. HEVC and H.264 descriptors were identical on this driver.
3. HEVC full interop is 9.56% faster than staged and uses substantially less CPU.
4. HEVC encode does not emerge as a singular proven bottleneck: total HEVC is
   modestly slower, but interop GPU cost is unchanged and asynchronous wall
   attribution cannot isolate the engine.
5. H.264 genericization caused no measured regression and preserved exact bytes.
6. The architecture can host another codec, but this evidence does not establish
   AV1 encoder capability, surface constraints, rate-control behavior or value.
   Stage 5.1B AV1 encode is therefore **not yet justified as an implementation**;
   a separately scoped capability/benefit experiment would be the next gate.
