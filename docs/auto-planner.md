# AsciiFlow automatic pipeline planner

Stage 4.0 gives the Rust CLI one runtime decision point for media, processing,
and DMA-BUF interop. The decision is deliberately small and deterministic:
AsciiFlow probes the input and this process's runtime capabilities, filters a
finite set of legal candidates, and selects the lowest qualitative preference
cost. It does not benchmark pipelines at startup and it does not persist a
hardware cache across processes.

## The four boundaries

### Capability

`CapabilitySnapshot` contains facts, not preferences. Media facts include
software H.264 decode/encode, VAAPI availability, H.264 VAAPI decode/encode,
NV12 hardware-frame creation, and Host-to-VAAPI upload. Processing facts cover
CPU, Vulkan, compute queue, `storageBuffer8BitAccess`, `shaderInt64`,
`Synchronization2`, selected device identity, and automatic eligibility.
Interop is directional: VAAPI → Vulkan input and Vulkan → VAAPI output are
qualified independently.

Every fact is one of:

```text
Supported
Unsupported(reason)
NotProbed(reason)
```

Reasons are retained for diagnostics. A device name, vendor, or operating
system alone never qualifies a codec or an interop direction. The runtime probe
checks the actual FFmpeg/VAAPI setup and, when possible, maps a real input or
encoder surface and checks the required DRM modifier and external-image access.

### Policy

`PipelinePolicy` is the user's request:

```text
--backend auto|cpu|vulkan
--decode auto|software|vaapi
--encode auto|software|vaapi
--input-interop auto|off|on
--output-interop auto|off|on
```

The legacy names `--vaapi-vulkan-input-interop` and
`--vaapi-vulkan-output-interop` remain accepted. `auto` is the only value that
permits a different implementation to be selected. Explicit hardware or
interop requests fail if unavailable; they never silently become software.

Contradictory policies fail before execution. For example, input interop with
software decode, output interop with software encode, and hardware interop with
CPU processing are not legal combinations.

### Plan

`PipelinePlan` is the selected execution description. It contains the resolved
decode, processing, and encode implementations, explicit transfer nodes,
portable frame domains, pixel-path classification, preference cost, and reasons.
The current logical nodes are:

```text
SoftwareDecode       VaapiDecode
HardwareDownload     InputHardwareInterop
CpuAscii             VulkanAscii
HostReadback         HardwareUpload
OutputHardwareInterop
SoftwareEncode       VaapiEncode
```

The domains are `HostNv12`, `HardwareNv12`, and `VulkanNv12Buffer`. Native
`AVFrame*`, `VASurfaceID`, `VkImage`, DMA-BUF fd, and DRM modifier values stay
outside Core.

### Factory

The CLI composition root consumes the selected plan and constructs the
concrete Decoder, Encoder, CPU/Vulkan backend, and interop processor. Core only
plans over capability facts and policy. Resource creation happens after plan
selection; the factory's output is then handed to the existing bounded,
ordered pipeline.

## Current automatic preference

The score is a qualitative ordering derived from Stage 1–3 evidence, not a
portable millisecond estimate. It favors Vulkan processing, penalizes software
decode modestly, treats VAAPI decode without input interop as expensive, and
prefers VAAPI encode over software encode when a Host upload is still needed.
The resulting preference order is:

1. VAAPI decode → input interop → Vulkan ASCII → output interop → VAAPI encode.
   This is the GPU-resident path.
2. VAAPI decode → input interop → Vulkan ASCII → Host readback → VAAPI
   hardware upload → VAAPI encode.
3. VAAPI decode → input interop → Vulkan ASCII → Host readback → software
   encode.
4. Software decode → Vulkan ASCII → output interop → VAAPI encode. This is a
   valid output-only interop path and is partially staged at the decode side.
5. Software decode → Vulkan ASCII → Host readback → software encode.
6. Software decode → CPU ASCII → software or VAAPI encode when Vulkan is
   unavailable or not eligible for automatic use.

If input interop is unavailable, automatic planning does not select VAAPI
decode followed by `hardware download`; it prefers software decode. An
explicit `--decode vaapi` request may still select that staged path. CPU Vulkan
devices such as llvmpipe/lavapipe are not eligible for automatic processing;
the existing environment hook remains for explicit diagnostics and tests.

Unsupported input requirements do not get silently admitted to the pipeline.
The qualified range is H.264, HEVC Main and AV1 Main 8-bit 4:2:0,
NV12-compatible video. Stage 4.1
rejects 10-bit input before output creation with an explicit unsupported-input
error; it never performs an undeclared 10-to-8 conversion.

## Diagnostics

The diagnostic modes do not create an output staging file:

```bash
asciiflow input.mp4 --capabilities
asciiflow input.mp4 --explain-plan
```

`--capabilities` prints input requirements, media/Vulkan/interop facts, probe
reasons, and probe duration. `--explain-plan` additionally prints every
capability-ineligible candidate that matches the requested policy, its
rejection reasons, the selected plan, pixel-path classification, preference
reasons, and planning time. Structurally impossible combinations and candidates
excluded by explicit policy are not presented as runtime rejections. Normal
conversion with `--verbose` prints the selected plan and startup timings before
processing.

For a fully qualified Intel Linux host, the explanation has this shape:

```text
Selected pipeline:
  VAAPI decode
  -> VAAPI/Vulkan input interop
  -> Vulkan ASCII
  -> Vulkan/VAAPI output interop
  -> VAAPI H.264 encode
Output: H.264 High · 8-bit Yuv420
Encoder backend: Hardware
Pixel path: GPU-resident
```

`--output-codec hevc` keeps the same graph but requires the independently
qualified HEVC Main VAAPI encoder. If HEVC output interop is unavailable,
automatic policy may replan once to Host readback plus VAAPI upload; it never
changes the requested codec to H.264. `--output-codec hevc --encode software`
is a planning error because software HEVC is not implemented.

The actual device, modifier, driver, and capability reasons are runtime data;
the sample above is not a vendor whitelist or a guarantee for another host.

## Failure and fallback boundaries

Capability probe failures are non-fatal for automatic policy when another legal
candidate remains. They become errors when an explicit request depends on the
failed capability. Rejected candidates remain visible in `--explain-plan`.

The initialization boundary permits at most one automatic replan: a factory
may mark the exact runtime-only capability that failed, rerun the pure planner,
and construct the fallback once. An explicit plan fails immediately. After the
first frame has entered processing, fallback is forbidden; a media, processor,
or encoder failure fails the whole pipeline. This keeps codec state, timestamp
ordering, and safe output commit deterministic.

The CLI performs input and encoder interop preflight before selecting the plan.
Concrete execution resources are then constructed by one `PipelineFactory`.
If an auto-selected capability fails during that construction, the CLI records
the exact failure, excludes that capability, replans once, and retries once.
A failure first observed after processing starts remains terminal.

Stage 4.1 adds structured initialization/runtime stages, deterministic test-only
fault injection, cooperative cancellation, and first-failure preservation. The
complete contract is in [`failure-semantics.md`](failure-semantics.md).

Output handling remains transactional at the application level. The encoder
writes a hidden temporary MP4 in the destination directory. The existing final
output is replaced only after successful trailer/finalization and a final
rename. Initialization, processing, cancellation, or finalization failure
cleans the temporary file on a best-effort basis and leaves the existing output
untouched. Temporary names include the process, timestamp, and an invocation
sequence, and are reserved with exclusive creation before encoder startup.

## Qualification and scope

The production interop qualification currently covers the validated Intel Arc
MTL + Intel iHD + Mesa ANV Linux setup. The probe still checks actual codec,
frame-pool, descriptor, modifier, external-image import, queue-family ownership,
and copy execution in both directions; vendor identity is diagnostic context
only. Because qualification is operational, a multi-GPU host may use a
cross-device import if the selected Vulkan device can actually execute it;
Stage 4.0 does not yet score topology or cross-device transfer cost. No new
codecs, platforms, audio passthrough,
external semaphore bridge, direct external-image shader, or startup benchmark
is part of Stage 4.0.

Synthetic planner tests are the primary coverage for degraded capability
scenarios and do not require a GPU. Real validation remains opt-in: capability
probe output, `--capabilities`, `--explain-plan`, automatic full interop,
explicit-versus-automatic parity, and the 300-frame comparison must be run on
the qualified host before claiming hardware support.

## Current Intel qualification evidence

On 2026-09-14, Release validation used the Stage 3 workload (`input.mp4`,
1920x1080 at 50 FPS, H.264 High/yuv420p, ASCII width 80) on Intel Arc MTL
(0x8086:0x7d55), Mesa ANV 26.1.8, the pinned FFmpeg 9.0.1 build, and the RPM
Fusion iHD 26.1.5 driver loaded from an isolated temporary directory. The
installed Fedora free-codec iHD build correctly reported H.264 decode and
encode as unsupported, so it was not used to make a false hardware claim.

- The capability probe qualified VAAPI decode/encode, both interop directions,
  the exact encoder pool, Host upload, and the required Vulkan features.
- `--explain-plan` selected the full GPU-resident pipeline and created no output.
- A 30-frame full-interop run and a separate output-only interop run completed
  with Khronos validation enabled and no Validation, VUID, or synchronization
  errors.
- Repeating the complete capability probe 100 times left the process FD count
  unchanged.
- Auto FPS was 529.12, 525.43, and 526.40 (median 526.40). Explicit full
  interop FPS was 507.44, 534.09, and 528.56 (median 528.56). The median
  difference was 0.41%, within the 3% regression threshold.
- The compared 300-frame auto and explicit MP4 files were byte-identical.
  Packet PTS/DTS/duration streams and decoded framemd5 streams also matched;
  the output retained H.264 High, yuv420p, BT.709 limited range, 1920x1080,
  50 FPS, and 300 frames.

Observed startup probe wall time was 73.6–77.1 ms in the six formal benchmark
runs after the probe was strengthened to execute both transfer directions;
planning was 0.004–0.006 ms. Probe time is reported separately and is not part
of conversion FPS.
