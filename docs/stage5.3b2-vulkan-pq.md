# Stage 5.3B-2 — internal Vulkan PQ qualification

Status: **SEALED**. Qualification date: 2026-09-28.
Production HDR remains closed. Stage 5.3B-3 HDR-preserving production
integration is justified; it was not started.

## Scope and implementation

The authoritative CPU f64 equations in `asciiflow-core/src/hdr_pq.rs` and
`asciiflow-cpu/src/hdr.rs` are unchanged. All existing SDR shader sources are
unchanged. No CLI/planner HDR switch, tone/gamut mapping, HLG processing,
HDR encode, or static metadata propagation was added.

`hdr-pq-qualification` is an internal opt-in feature in Vulkan/media/interop.
`VulkanPqQualification` constructs a separate processing mode; normal factories
do not construct it. Its packed P010 device buffers and DMA-BUF image transfers
reuse existing one-slot and bounded two-slot ownership, fences and FOREIGN_EXT
acquire/release. Readback is for diagnostics/qualification, not CPU rendering.
The decoder exception admits only `UnsupportedHdrPq`, while codec/profile,
ten-bit 4:2:0, layout, left siting, metadata conflict and stability checks
remain enforced. The diagnostic VAAPI output pool preserves descriptor color
tags and has no HDR encoder.

| Component | Contract |
| --- | --- |
| `shaders/src/pq_common.glsl` | Independent f32 rational PQ constants, per-channel EOTF/inverse EOTF and BT.2020 NCL helpers |
| `shaders/src/ascii_map_pq.comp` | 32 lanes per cell, shared tree reduction of normalized linear RGB; no subgroup assumption |
| `shaders/src/ascii_render_pq.comp` | 32×4 workgroups processing 2×2 output blocks; R8 atlas coverage |
| GPU light representation | f32 RGB normalized to 10000 cd/m²; 1.0 represents 10000 nits, not exposure-normalized light |
| Cell storage | std430 32-byte stride: RGB/perceptual vec4 plus glyph/input clips/invalid/output clips uvec4 |
| Required capabilities | Shader, StorageBuffer16BitAccess; render also existing StorageBuffer8BitAccess; no Float64, Float16, Int16 or Int64 arithmetic capability |
| P010 decode | LE16 code in bits 15..6, reject nonzero padding; Y 64..940, Cb/Cr 64..960; neutral 512 |
| Input color | BT.2020 primaries/NCL matrix, PQ, limited, left; reuse containing 2×2 UV block |
| Map/foreground | Per-channel PQ EOTF, then linear RGB mean; monochrome foreground is physical linear luminance replicated |
| Glyph | Inverse PQ of mean linear luminance → rounded limited 8-bit LUT index → unchanged glyph LUT/S-curve; light itself is not remapped |
| Render/blend | Atlas alpha/255 times linear foreground over zero-nit black; then per-channel inverse PQ and BT.2020 NCL encoding |
| Chroma | Average four unquantized post-blend encoded Cb/Cr components, then quantize once |
| Output | floor(positive code+0.5), legal-range clipping, ten-bit code shifted left six; no 10→8 reduction |
| Diagnostics | Input RGB clips and output code clips counted; finite and invalid counters checked before exposing/copying output |

The EOTF constants are m1=2610/16384, m2=(2523/4096)×128,
c1=3424/4096, c2=(2413/4096)×32, c3=(2392/4096)×32;
Kr=.2627, Kb=.0593, Kg=.6780. GPU arithmetic is f32 throughout.
Coordinate bounds reject uint32 overflow; diagnostic atomics and host readback
have explicit compute read/write and transfer→host synchronization.

## Numeric qualification

All tested cells select exactly the CPU glyph: zero mismatches. Active output
sample difference >1 has count zero. Synthetic suite aggregate maxima are
Y=1, U=1, V=0; every case records per-plane sample count, delta 0/1/>1
counts, max and p50/p95/p99/p99.9. Durable per-fixture numeric records are in
[`tests/baselines/pq-stage53b2.json`](../tests/baselines/pq-stage53b2.json).
Black, full-coverage neutral gray, near-black and peak-white controls are exact.
The 1080p color-boundary frame is byte-exact (all three planes' percentiles 0).
Gray/color and low-bit patterns may contain isolated one-code rounding differences;
they are not presented as universal byte equality.

| Intermediate | Observed maximum | Fixed gate after characterization |
| --- | ---: | ---: |
| Normalized linear RGB absolute | 1.75821545e-5 | 4e-5 |
| Normalized linear RGB relative | 2.71029587e-5 | 6e-5 |
| Normalized luminance absolute | 1.75821545e-5 | 4e-5 |
| Normalized luminance relative | 2.71029587e-5 | 6e-5 |
| Inverse-PQ scalar absolute | 8.03803769e-6 | 2e-5 |

Relative error is checked for nonzero reference values; black uses absolute
error. Gates have approximately 2× measured margin, and do not weaken exact
glyph/final-code gates. Near-white, quantized 1000/10000-nit highlights and the
1080p case remained inside these bounds. No CPU algorithm/tolerance was changed
to accommodate the GPU.

Coverage includes built-in and FreeType atlases, color and monochrome,
0/100/1000/10000-nit patches dispatched through the actual map shader with
cell readback and the unchanged Stage 5.3B-1 physical quantization bounds,
horizontal/vertical variation, adjacent ten-bit steps, strong chroma patches,
uneven 7×3/7×5 cells, 256×256 one-cell reduction, and 64 alternating dark/bright
frames through one and two slots. Invalid Y/U/V padding and legal-code ranges,
NV12, wrong matrix/primaries/transfer/range/siting fail rather than produce output.
NaN/Inf counters remain zero on valid tests; nonfinite shader results are guarded.
This is not an injected hardware floating-point fault claim.
CPU/GPU total clamp diagnostics match exactly: all tested output YUV clamp
counts are 0; input nonlinear RGB clamps are 3304 on each 66×50 chroma-boundary
case and 65536 on each 256×256 large-cell case, and 0 on neutral/gray/ten-bit
step controls. Linear clamps are 0 by policy: invalid linear values are rejected,
not silently clipped. The durable numeric record contains all case totals.

## Real PQ input/output and lifecycle

New positive inputs are `hevc-main10-pq-qualified.mp4` (Main 10) and
`av1-main10-pq-qualified.mp4` (Main 10-bit), 128×96, 36 frames, 30/1 fps,
yuv420p10le, `bt2020/smpte2084/bt2020nc/tv/left`. The checked-in generator
`tests/fixtures/codecs/generate-pq-qualification.sh` requires FFmpeg 8.1.3.
Two complete generations matched:

```text
HEVC 37b5f415c8f6e3563e36abaec62d49b8d11e14c352b3185b89b29f78f82192c2
AV1  6c04f3bd64f75c6eca9558caff48f662ac66a2745c71ff0b30dcf7b66c6f1c32
```

Exact commands, generator hash, byte sizes and metadata are in the fixture
README/SHA256SUMS. Old PQ rejection files remain unchanged: unrestricted
codes cause the CPU oracle to reject them; old AV1 also lacks qualified siting.
They were not silently promoted into positive fixtures or clamped into validity.

The CPU oracle consumes a download of the exact same VAAPI surface supplied
to Vulkan, including hardware decode rounding. Separate Vulkan import readback
is byte-exact to that download. Every frame's glyph IDs match exactly; every
active output code is within one. Output DMA-BUF is filled by Vulkan and then
downloaded from the actual diagnostic VAAPI P010 surface, not a substitute
host buffer. One-slot and existing full-interop two-slot paths both pass.

| Codec | Actual nonzero low-two-bit samples / active samples (36 frames) | Stress frames | FD before / active / peak / after |
| --- | ---: | ---: | --- |
| HEVC PQ | 497940 / 663552 | 3000 | 4 / 22 / 24 / 4 |
| AV1 PQ | 497932 / 663552 | 3000 | 4 / 22 / 26 / 4 |

Stress repeats real decode fixtures, drains slots before replacing each decoder
context, checks ordered completion and all active output samples, and restores
the exact FD baseline. Surface references and imported images remain owned
until completed fences/foreign release; they are not dropped at submission.
Khronos synchronization validation was explicitly enabled: zero errors.

The counter-only extension was rerun for both codecs in all three 36-frame
paths (one-slot external→Host, one-slot external→external, two-slot
external→external). Each codec/path recorded:

| Plane | delta 0 | delta 1 | delta >1 | max | p50/p95/p99/p99.9 |
| --- | ---: | ---: | ---: | ---: | --- |
| Y | 442356 | 12 | 0 | 1 | 0/0/0/0 |
| Cb | 110588 | 4 | 0 | 1 | 0/0/0/0 |
| Cr | 110582 | 10 | 0 | 1 | 0/0/0/0 |

The earlier full 3000-frame tests enforce the same all-sample bounds, but their
new histogram counters were not remeasured; the FD/pixel evidence is unchanged.

Initialization faults: HDR cell allocation, descriptor creation, pipeline
creation. Runtime faults: before compute submission and after real completion.
Each reports the correct initialization/runtime stage, returns no output,
cleans resources and allows a fresh healthy retry with exact FD restoration.
Existing external-output checkpoints cover image creation, memory import,
queue submission and post-completion fence-wait error reporting, with real PQ
pool surfaces, safe surface reuse, and baseline restoration. These are simulated
error/cleanup checkpoints on the host→external output path, not device-lost,
hung-fence, or failed driver submissions. There is no mid-stream CPU fallback.

## 1080p/300-frame measurement

Fixed raw generator: `pq_qualification::frame/v1`, ColorBoundary, 1920×1080,
6220800 bytes, P010 BT.2020/PQ/limited/left. Y=300+((7x+11y)%400), with
UV=(64,960)/(960,64)/(400,600)/(600,400) by chroma (x+y)%4; LE16(code<<6).
Raw input SHA-256:
`f284dfe2840ebd6366d4c11bf86fb9e9af463758deec96834f9399c332d0b955`.
Generator test source SHA-256:
`4b86e8cfe56c817572ba59001bfdaa06f45e7d146216693e8f6f769c0f2cef56`.
All 300 frames repeat this fixed spatial signal with ordered PTS; separate
alternating/real-stream tests exercise temporal surface reuse. Config is
160×90 cells, built-in 8×8, standard charset, truecolor, no audio/encode.
The CPU f64 implementation and full per-sample parity oracle are unchanged.

| Path | 300-frame wall s including parity checks | Wall fps | API-active s excluding checks | Oracle-check s |
| --- | ---: | ---: | ---: | ---: |
| CPU f64 | 139.117596 | 2.156 | 138.951677 | .164748 |
| Vulkan one-slot | 4.047729 | 74.116 | 3.246087 | .801432 |
| Vulkan two-slot | 1.653469 | 181.437 | .889436 | .763799 |

| Average ms/frame | CPU map | CPU render | GPU map | GPU render | GPU upload | GPU download | Host upload | Queue | Wait | Backend wall |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| CPU | 187.452793 | 275.719143 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| One-slot | 0 | 0 | 1.070802 | 2.755774 | .399788 | .407157 | .226219 | .067200 | 6.189027 | 10.227072 |
| Two-slot | 0 | 0 | .789386 | 1.931025 | .307975 | .314777 | .391165 | .060581 | 6.419710 | 13.073521 |

Validation was off for timing and on for the separate qualification run.
Two-slot workers overlap oracle checks: API-active time is **not** throughput.
Queue/wait/backend times overlap and must not be summed as a frame budget.
Wall time includes diagnostic counter readback and full parity comparisons.
The measurement preceded final alignment-safe cell deserialization and added
test identity reporting; no shader/CPU math changed. These are host-processing
measurements, not full DMA-BUF HDR encode throughput, display fidelity or an FPS
target. No LUT/approximation was introduced to meet performance.

## SDR and production boundary regression

All five current post-polarity baseline-v2 conversions were run three times
using the unchanged `tests/baselines/media/generate-post-polarity-v2.sh` CLI:
width80, standard/builtin-8x8, truecolor, audio none, 300 frames,
VAAPI decode/encode, Vulkan GPU mapping, both interop directions on,
`/dev/dri/renderD128`, explicit codec/depth. Inputs match the manifest:
8-bit `6e5c214b813dca3e1db65629b1241cfb663166eb565cda1c48b5ee74ed0dce6b`,
10-bit `df69c98ca6592b08c6bc34127b2bb5cf4125c759fd852b830e5426d5818fccda`.
Every repeated output matched the retained hash:

```text
H264 8  7c8a7572320b8c6c999143dfece4a76d487d6c9e7788206b7af52df9acd4e1cc
HEVC 8  294a1b63ac5b0e440dcf60c4c60f975594c6e944829478b69f09f981d268ff71
AV1  8  1531e29f52ae4e747251cf1889003dfd420303c523fb0ddc55cc9140e3bf2a4c
HEVC10  f213a9f75542421bb816550cd7a93796db98ae36296a00636f7273492740a142
AV1 10  b69c68525ac6ab2464e04b8fc1f123d887b45bba580377bafad99dc74cf62ad7
```

Strict native structured comparisons retain raw packet identity (Tier 1A),
coded semantics (1B), decoded identity (1C), structure (2) and file identity.
No encoder-string normalization/allowlist was added. Full decode/probe confirms
300 frames, 1920×1080, 50 fps, BT.709 limited. Historical pre-polarity or lost
Stage 5.2C-3 hashes were not relabeled PASS or compared to unrelated inputs.
Software/VAAPI color parity and PQ/HLG/BT.2020 SDR/full-range rejection pass;
feature-enabled normal decoders still reject new PQ fixtures/conflicts.
Intel AAC payload/metadata/validation/cancellation and SDR P010 FreeType
interop regression pass. Normal CLI tests retain rejection-before-output-staging.

## Toolchain, verification and evidence

Intel Arc Meteor Lake 8086:7d55, iHD 26.1.5, VAAPI 1.23,
Mesa Vulkan 26.2.3 (device API 1.4.354, required target 1.3),
FFmpeg/ffmpeg-libs 8.1.3-1.fc44, libavcodec 62.28.103,
libavformat 62.12.103; gcc16 FFmpeg build/configuration is the installed Fedora
package identity. Starting HEAD `888950054246c2e885012b805567fcb26dd9bdf3`,
with this uncommitted patch. Cargo.lock SHA-256
`f8cf97f855fcbcb422ef1b74f2367a7ac40b612dcc41cc0a1d8a23f3e27746b8`.
Initial SDR rerun binary SHA-256
`cb45e2548e233c5af11fc869c151a8d76955977d0c6410ea7127e80c648e2bc2`;
the final alignment-safe release binary
`e268c56c616134b427bbda9718136cbe40929c63cbd688d0093480e031e54183`
also reran all five cases three times with identical retained hashes.

```bash
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-vulkan \
  --release --features hdr-pq-qualification --test pq_qualification \
  -- --ignored --nocapture --skip pq_1080p_300_frame_benchmark
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop \
  --features hdr-pq-qualification --test pq_hardware \
  -- --ignored --nocapture --test-threads=1
ASCIIFLOW_VULKAN_VALIDATION=0 cargo test -p asciiflow-vulkan \
  --release --features hdr-pq-qualification --test pq_qualification \
  pq_1080p_300_frame_benchmark -- --ignored --nocapture
cargo build --workspace --release
cargo test --workspace
cargo test --workspace --features asciiflow-cli/encode-characterization,asciiflow-interop/hdr-pq-qualification
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features asciiflow-cli/encode-characterization,asciiflow-interop/hdr-pq-qualification -- -D warnings
cargo fmt --all --check
git diff --check
```

Final hardware suite: 7/7 pass; synthetic/invalid/fault/1080p qualification:
6/6 pass plus the short input-identity rerun. The counter-only hardware
characterization extension also passes 6/6 without repeating the 3000-frame
stress. Normal and measurement-feature
workspace tests/checks/clippy, release build, formatting and diff checks pass.
Every generated cached debug/release SPIR-V artifact is validated with
`spirv-val --target-env vulkan1.3`; 274 cached debug/release artifacts passed,
not assumed to remain the historical 133. There are 19 distinct module names,
including the two new PQ modules. Shader capability inspection found no new
unexpected arithmetic requirement.

Run-local detailed logs are `/tmp/asciiflow-pq-final-synthetic-intel-validation.log`,
`/tmp/asciiflow-pq-1080p-input-identity-validation.log`,
`/tmp/asciiflow-pq-hardware-complete-final.log`,
`/tmp/asciiflow-pq-hardware-characterization.log`,
`/tmp/asciiflow-pq-benchmark-intel.log`,
`/tmp/asciiflow-pq-final-sdr-regression-hashes.log`, the five SDR oracle/probe logs,
and `/tmp/asciiflow-pq-spirv-validation.log`. The durable identities, commands,
results and limitations are in this report, checked-in tests and fixture records;
the temporary logs are not treated as permanently retained artifacts.

## Limits and sealing decision

No remaining Stage 5.3B-2 gate. Qualification is specific to the recorded Intel
device/driver and fixtures, not every possible input/GPU. PQ mathematical signals
do not prove calibrated HDR display appearance. f32 intermediate errors are
nonzero; final glyph/code bounds are independently enforced. Simulated faults do
not establish recovery from real device loss or hung fences. Diagnostic surfaces
do not establish production HDR metadata, muxing or encoding. HLG, tone/gamut
mapping, HDR CLI/planner/output signaling and static metadata policy remain
unimplemented and outside this stage. Production HDR stays fail-closed.
