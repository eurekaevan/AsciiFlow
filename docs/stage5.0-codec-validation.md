# Stage 5.0 codec validation

Status: software integration and **Intel Arc Meteor Lake qualification passed**.

## H.264 specialization audit

| Category | Evidence and disposition |
| --- | --- |
| Legitimate output specialization | encoder.rs h264_vaapi/libx264, H.264 output capability and planner encode node remain unchanged |
| Accidental input specialization | InputRequirements VideoCodec and hardware eligibility now include HEVC/AV1; native profile names map to portable VideoProfile |
| Capability specialization | Former H.264-only first-frame branch replaced with codec matrix and selected-stream qualification; per-codec decode/input-import replan |
| Diagnostics/tests | Decoder initialization wording made codec-neutral; H.264 baseline tests retained and new codec fixtures/tests added |

Decoder packet/receive/drain, VAAPI surface ownership and DRM importer are reused;
there is no HevcDecoder/Av1InteropProcessor or shader change. AV1 hardware discovery
does not assume the codec-ID default (often libdav1d) exposes VAAPI. Native libva
ABI values were checked against the [official header](https://github.com/intel/libva/blob/master/va/va.h):
H.264 5/6/7/13, HEVC Main 17, AV1 Profile0 32, VLD 1. Query capacities and returned
counts are bounded before indexing, while the library and FFmpeg-owned display
stay alive. Actual frame validation rejects incompatible depth/chroma and HDR
before the NV12 conversion/import boundary.

## Evidence and limits

Synthetic 64×64 BT.709 limited, 36-display-frame fixtures cover HEVC Main8 with
B frames, AV1 Main8 without film grain, HEVC Main10 and AV1 Main10. The 8-bit
samples include AAC. Generator version/commands/hashes are in the fixture manifest.
Default tests cover codec/profile/depth/chroma, complete drain/frame count, PTS,
repeat decoded NV12 equality, H.264 output decode, audio byte preservation and
FreeType combination. Ten-bit inputs fail without changing existing output.
Planner tests cover per-codec auto, no-interop, missing hardware, explicit VAAPI,
unsupported profiles/depth/chroma and isolation from H.264 capabilities.

The initial software-only run had no render node. A subsequent run on the same
date exposed `/dev/dri/renderD128`; the Intel results below supersede that
hardware-pending status, not the earlier software regression evidence.

The opt-in tests record real descriptors without assuming the H.264 layout and
compare all 36 frames through the existing importer. Software-vs-VAAPI tests are
byte-exact; no tolerance was added. Existing full-interop tests can be run with
larger same-source codec inputs. Formal measurement remains 1080p, width80,
color, built-in font, audio off, Release, validation off, 30-frame warm-up then
three 300-frame runs and median. Keep CPU wall and GPU timestamps separate.

## Initial software-only run record (2026-09-16)

- `cargo test --workspace`: 102 passed, 28 ignored/opt-in. Release workspace
  build, all-target Clippy with warnings denied, fmt and diff checks passed.
- Both 36-frame new-codec software-input → lavapipe Vulkan → software H.264,
  with FreeType and AAC, completed under Khronos validation; no VUID/Validation
  Error appeared. These are software Vulkan integration checks, not Intel results.
- H.264 comparison rebuilt sealed HEAD `06140d8` in a separate temporary checkout.
  Video-only complete MP4 was byte-identical. With audio, decoded video framemd5,
  compressed audio bytes and per-stream packet timelines/counts matched, while
  container interleaving order differed. No whole-file equality is claimed for
  the audio-enabled case; the output mux implementation was not changed.
- New capability reports explicitly show software HEVC/AV1 support and native
  VAAPI device failure; interop is NotProbed, not falsely Supported.
- Representative no-render-node capability probe CPU wall was 36.221 ms for
  HEVC and 39.866 ms for AV1. This does not measure successful hardware
  qualification cost; the earlier validation-enabled lavapipe smoke probes were
  158.475/70.837 ms and must not be mixed into a hardware startup claim.
- No formal hardware benchmark was run, and no hardware speedup is inferred.

## Intel qualification (2026-09-16)

Device: Intel(R) Arc(tm) Graphics (MTL), vendor/device 0x8086/0x7d55,
Vulkan 1.4.354, driver version 109056008. The capability snapshot reported
H.264, HEVC Main and AV1 Main VLD decode, input interop and output interop supported.
No software Vulkan device was used for this qualification.

- Both 36-frame fixtures: software decode versus VAAPI download byte-exact;
  actual DRM import/readback and two-slot final ASCII NV12 byte-exact, including
  PTS and frame description. HEVC delayed/B frames drain correctly.
- H.264, HEVC and AV1 1080p inputs: 30-frame descriptor/pre-ASCII checks passed.
  Observed layout was identical: one 3194880-byte object, modifier
  `0x0100000000000009`, R8/GR88 layers, pitches 1920, offsets 0/2088960.
  This equality is an observation for these streams/driver, not an importer assumption.
- HEVC and AV1 full hardware conversion with FreeType and AAC: 36 output frames
  each, H.264 outputs successfully decoded; compressed AAC hashes matched input.
- Each new codec completed **3000 1080p frames** through full input/output
  interop with per-frame pre-encode NV12 parity and correct slot drain. FD
  before/active/max-steady/after: HEVC **4/36/42/4**, AV1 **4/36/40/4**.
- All these interop tests used `ASCIIFLOW_VULKAN_VALIDATION=1`; validation error
  counters were zero, with no VUID or synchronization error reported.
- Qualification exposed a stale plan-display label (`VAAPI H.264 decode`) for
  all codecs. It was corrected to codec-neutral `VAAPI decode`, with the display
  regression assertion updated; execution and benchmark code were unchanged.

### Formal benchmark

Release, 1920×1080, 50 fps, 300 frames, same testsrc2 source encoded separately
as H.264/HEVC/AV1, BT.709 limited 8-bit 4:2:0, width 80, color, built-in font,
audio off, validation off. Each path ran a 30-frame warm-up then three complete
300-frame conversions. Both decode alternatives use Vulkan and VAAPI H.264
output interop; software decode uses Host input upload, hardware uses full
interop. These are **whole-pipeline FPS**, not isolated decoder FPS.

| Input / decode | FPS runs | Median FPS | Process CPU % | Decode wall | Backend wall | Encode wall | Latency |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| H.264 software | 268.90 / 271.19 / 254.69 | 268.90 | 118 | 3.685 | 2.509 | 0.371 | 11.675 |
| H.264 VAAPI initial | 240.67 / 290.12 / 398.61 | 290.12 | 41 | 0.236 | 5.561 | 2.247 | 29.029 |
| H.264 VAAPI repeat control | 520.58 / 520.49 / 517.34 | 520.49 | 58 | 0.191 | 2.116 | 1.876 | 20.205 |
| HEVC software | 170.09 / 168.88 / 156.98 | 168.88 | 110 | 5.858 | 3.924 | 0.369 | 17.821 |
| HEVC VAAPI | 401.77 / 403.40 / 403.82 | 403.40 | 54 | 0.266 | 2.218 | 2.430 | 26.169 |
| AV1 software | 234.39 / 242.80 / 246.44 | 242.80 | 113 | 3.853 | 7.273 | 2.112 | 22.348 |
| AV1 VAAPI | 402.72 / 405.50 / 407.57 | 405.50 | 52 | 0.238 | 1.983 | 2.409 | 26.006 |

All duration columns are ms/frame, independently medianed. CPU % comes from
`/usr/bin/time` user+system/process-wall across startup and conversion (100% is
one core); FPS is the CLI processing interval, excluding capability startup.
Latency is decode-call start to encode acceptance, not reciprocal throughput.
The first H.264 group was unstable; both initial and repeat groups are retained.
The cause of the variation was not established. No historical <3% regression
claim or cross-codec ranking is made from this synthetic, sequential run.

| Input / VAAPI | DRM map wall | DMA-BUF import wall | GPU image→buffer | GPU mapping | GPU render | Startup probe wall ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| H.264 repeat | 0.051 | 0.014 | 0.107 | 0.201 | 0.091 | 105.369 |
| HEVC | 0.035 | 0.008 | 0.118 | 0.219 | 0.101 | 129.937 |
| AV1 | 0.107 | 0.010 | 0.109 | 0.205 | 0.094 | 101.381 |

GPU columns use Vulkan timestamps; wall columns use CPU clocks and may include
waiting. Stages overlap: do not sum them, and do not treat encoder wall as pure
hardware encode time. Full logs retain capability query/create/bind/destroy,
output interop, queue-submit and fence-wait breakdowns.

Artifacts: `/tmp/asciiflow-stage5-qualification/` contains the generation
manifest, SHA256SUMS, inputs and `results/*.{log,time,mp4}`. Hardware test logs
are `/tmp/stage5-*-descriptor.log`, `/tmp/stage5-*-stress.log`,
`/tmp/stage5-hw-{decode,interop}.log` and `/tmp/stage5-*-full.log`.
These temporary artifacts are not repository fixtures and can disappear.
Benchmark input SHA-256:

- H.264: `8be51fc1ae5d931a19593b59fd73334448bc818b22fe733720a3aa99b72064c7`
- HEVC: `39250408a7318d606d7cf9ff3f5a278fefe4e84c568d5a1eef5cad4a0b845325`
- AV1: `e095a89667d3424e1814300044f04f0ccb8ac3ff0e8c078f53181c940ddf7d6b`

## Stage 5.1 decision

1. HEVC 8-bit fully reuses the existing VAAPI/interoperability architecture.
2. AV1 8-bit likewise reuses it; the baseline excludes film grain.
3. Actual descriptors matched H.264 at the tested geometry on this driver.
4. Pipeline median speedups with hardware decode: HEVC **2.39×**, AV1 **1.67×**;
   H.264 **1.94×** using the repeat control (initial group **1.08×**).
5. Planner decisions remain codec-generic and capability-driven.
6. The stale H.264 display label was fixed. H.264 output specialization and
   named H.264 capability fields are intentional; no input execution restriction
   to H.264 remains in the qualified paths.
7. Software paths are decode-limited; hardware decode wall is small. Hardware
   runs show downstream processing/encode acceptance and synchronization cost,
   not a demonstrated compute-shader bottleneck. Encoder wall alone cannot
   separate engine cost from synchronization; deeper diagnosis is still needed.
8. Decode qualification is sufficient to consider a separately scoped Stage 5.1
   experiment, but does **not** establish HEVC/AV1 output support or a performance
   benefit from encode expansion. No encode expansion was started.

Limits remain: only this Intel/driver and synthetic 8-bit Main-profile streams
were qualified; AV1 film grain, 10-bit/P010, HDR, other chroma profiles, vendors
and platforms are not newly supported. Timing variation limits generalization.
