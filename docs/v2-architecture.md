# AsciiFlow v2 architecture

This document defines the Rust baseline through Stage 4.3: a permanent CPU
reference backend, a Vulkan 1.3 compute backend, optional Linux VAAPI media,
and qualified Intel DMA-BUF bridges in both pixel directions. Stage 3A
eliminates decode-side Host copies; Stage 3B fills encoder-owned VAAPI
surfaces directly from Vulkan. Stage 4.0 adds a runtime capability graph and
automatic selection without changing the portable Host reference paths. Stage
4.1 adds structured failures, bounded initialization replan, cooperative
cancellation, transactional output, and failure-path resource guarantees.
Stage 4.2 adds a separate compressed-audio passthrough plan and one bounded,
single-owner interleaved mux path; it does not change video planning or pixels.

## Workspace and dependency direction

```text
apps/asciiflow-cli
  ├── crates/asciiflow-core
  ├── crates/asciiflow-media ──> asciiflow-core
  ├── crates/asciiflow-cpu    ──> asciiflow-core + asciiflow-font
  ├── crates/asciiflow-vulkan ──> asciiflow-core + asciiflow-font + ash
  └── crates/asciiflow-interop ─> media + vulkan + core
```

`asciiflow-core` owns configuration, frame metadata/storage, backend and media
traits, the bounded pipeline, planning, metrics, and shared errors. It has no
FFmpeg, font, graphics, or platform dependency. The CLI is the composition
root; keeping implementation crates as siblings avoids a dependency cycle and
is the concrete form of the intended Core/Media/backend boundary.

`asciiflow-media` owns ordinary FFmpeg decode/encode. `asciiflow-interop` is
the only crate that relates its opaque retained VAAPI frame to FFmpeg DRM PRIME
and Vulkan external memory. `asciiflow-cpu`
implements the permanent CPU reference backend. `asciiflow-font` owns the R8
glyph atlas independently of either video backend. `asciiflow-vulkan` contains
all Vulkan types and calls; Core does not depend on `ash` or `gpu-allocator`.

## Native and unsafe boundary

Stage 4.3 adds initialization-only `Font specification -> FreeType -> GlyphAtlas`.
The CLI builds one owned atlas before staging/mux initialization and passes
identical pixels to CPU/Vulkan. FreeType objects never enter core, frames or
workers; atlas data has no native types. Vulkan forks preserve atlas identity
and initialize their own static upload and coordinate LUT. Font validity is
render configuration, not a hardware capability or a replan opportunity.
See [fonts.md](fonts.md) and [validation](stage4.3-font-validation.md).

The media crate uses the raw `ffmpeg-sys-next` binding and wraps it in internal
RAII types for `AVFrame`, `AVPacket`, decoder/format/scaler state, and
encoder/muxer state. FFmpeg `unsafe` is confined to
`crates/asciiflow-media/src/ffmpeg` plus the narrowly scoped DRM PRIME bridge,
and Vulkan `unsafe` to `crates/asciiflow-vulkan`. A documented unsafe pointer
borrow exists solely for the sibling interop crate; no native pointer or Linux
graphics type enters Core or the CLI.

The binding is generated from the FFmpeg headers selected by `pkg-config`, or
from `FFMPEG_DIR/include` when `FFMPEG_DIR` points at a fixed installation. The
matching libraries are linked dynamically. For a reproducible Fedora x86_64
build, `third_party/ffmpeg/version.toml` pins the upstream archive, checksum,
and configure flags, while `third_party/ffmpeg/scripts` fetches and builds into
an ignored local installation. At runtime its `lib` directory must be visible
to the dynamic loader.

The pinned build enables `libx264` and therefore GPL components. This changes
distribution obligations even though AsciiFlow's own source remains MIT.

## Frame model and ownership

`FrameDesc` describes even-sized NV12 video, BT.709/limited-range metadata, and
the `Host` memory domain. `VideoFrame` owns a `HostFrame`; byte slices are
borrowed and the backing allocation cannot be shared mutably. A frame moves
through the traits:

```text
FrameSource -> bounded channel -> AsciiBackend -> bounded channel -> FrameSink
```

There is no `Arc<Mutex<VideoFrame>>`. Once a stage sends a frame, that stage no
longer owns it. The Core memory-domain enum intentionally contains only `Host`.
The planner's `FrameDomain` is a separate, non-owning vocabulary for describing
hardware transitions; it does not make native hardware frames part of the Core
storage contract.

Compressed audio never becomes a Core frame. Core contains only portable audio
stream facts, policy, plan, and counters. The media crate retains native codec
parameters, moves reference-counted `AVPacket` ownership out of the demux
scratch packet, and sends selected packets through a bounded queue to the mux
owner.
Stage 2 downloads/uploads within `asciiflow-media`. Stage 3A uses a specialized
Media/Interop pipeline carrying `VaapiDecodedFrame` outside Core, then returns
the processed result through the unchanged Core Host NV12 contract.

## CPU data flow

The decoder converts decoded software frames directly to host NV12 with
libswscale, normalizing color metadata to BT.709 limited range. The mapper
reduces the Y plane per ASCII cell, applies smoothstep contrast, and selects a
glyph index. It aggregates interleaved UV samples separately for colored
glyphs. The intermediate `CellGrid` contains compact `AsciiCell` values rather
than strings.

The font crate creates an owned R8 atlas during initialization: built-in 8x8 by
default, or cell-sized FreeType tiles for an explicit font file. The renderer
composites that atlas directly into a new NV12 frame.
It does not create RGB24 frames and does not rasterize glyphs per video frame.
The CPU backend is the permanent correctness/reference backend, not temporary
legacy code. FreeType is never called in the per-frame renderer.

## Capability, policy, plan, and execution

Stage 4.0 keeps four concepts separate:

* `CapabilitySnapshot` records facts observed from FFmpeg, VAAPI, Vulkan, and
  DRM PRIME. Each fact is `Supported`, `Unsupported(reason)`, or
  `NotProbed(reason)`; native handles and pointers never enter Core.
* `PipelinePolicy` records user intent. `auto`, `software`, `hardware`,
  `cpu`, `vulkan`, and interop `auto|off|on` are policy values, not capability
  claims.
* `PipelinePlan` is the selected, printable sequence of logical nodes and
  frame domains. It includes transfer nodes, pixel-path classification,
  preference cost, and human-readable reasons.
* The CLI composition root is the execution factory. Only after a plan is
  selected does it construct concrete FFmpeg, Vulkan, CPU, and interop
  resources. The planner itself creates no native resources.

`AudioPolicy` and `AudioPlan` are deliberately parallel to, not embedded in,
the video candidate graph. Container compatibility is queried from FFmpeg's
MP4 muxer. Therefore choosing, rejecting, or disabling audio cannot change the
selected `PixelPath`, decoder, processor, interop mode, or encoder.

The normal startup flow is:

```text
probe input requirements and runtime capabilities once
  -> validate policy
  -> enumerate finite candidates
  -> retain rejection reasons
  -> stable preference score and tie-break
  -> select PipelinePlan
  -> build concrete execution resources
  -> run the bounded pipeline and commit the output
```

The current logical nodes are `SoftwareDecode`, `VaapiDecode`,
`HardwareDownload`, `InputHardwareInterop`, `CpuAscii`, `VulkanAscii`,
`HostReadback`, `HardwareUpload`, `OutputHardwareInterop`,
`SoftwareEncode`, and `VaapiEncode`. The portable frame domains are
`HostNv12`, `HardwareNv12`, and `VulkanNv12Buffer`; native `VASurfaceID`,
`VkImage`, and DMA-BUF handles remain implementation details.

The default automatic preference is evidence-based and qualitative rather
than a machine-learning model or portable millisecond table:

1. Full VAAPI decode → input interop → Vulkan → output interop → VAAPI encode.
2. Input interop → Vulkan → Host → VAAPI encode when output interop is absent.
3. Input interop → Vulkan → software encode when VAAPI encode is absent.
4. Software decode → Vulkan → output interop → VAAPI encode when only output
   interop is available.
5. Software decode → Vulkan → software encode.
6. Software decode → CPU → software or VAAPI encode, when Vulkan is unavailable
   or not eligible for automatic use.

Automatic planning does not choose staged VAAPI decode plus `hwdownload` merely
because a VAAPI device exists. That path remains available for an explicit
`--decode vaapi` request. CPU Vulkan implementations such as llvmpipe/lavapipe
are excluded from automatic processing, although explicit diagnostic use may
be enabled through the existing Vulkan test hook.

Every capability-ineligible candidate that matches the current policy retains
its reason, for example an unavailable H.264
profile, an unsupported 10-bit input, an unqualified DRM modifier, or a Vulkan
device that is not eligible for automatic processing. This makes
`--explain-plan` diagnostic rather than a second, divergent planner.

Explicit policy is strict: an explicitly requested unavailable decoder,
encoder, backend, or interop direction is an error. Only `auto` may select a
different legal candidate. Contradictions such as software encode with output
interop, software decode with input interop, or CPU processing with hardware
interop fail during centralized policy validation.

Capability probing is per process invocation, not a persistent hardware cache.
The input probe opens the media once, obtains codec/profile/format/size/frame
rate requirements, and qualifies the actual VAAPI/Vulkan interop surfaces when
possible. `--capabilities` prints this snapshot; `--explain-plan` prints it,
the rejected candidates, the selected plan, and planning time. Neither mode
creates an output staging file.

Initialization fallback is deliberately bounded: if a selected plan that
contains `auto` fails during concrete resource initialization, the execution
factory may mark the exact failed capability, re-plan once, and retry. An
explicit plan fails immediately. Once processing has started there is no
mid-stream fallback; codec state, timestamps, and output determinism remain
owned by one plan. The CLI preflights the known VAAPI/DRM/Vulkan requirements
and classifies factory failures before execution. The diagnostic preserves the
initial plan, first failure, replanned plan, and any terminal retry failure.

## Pipeline, failure, and metrics

The selected plan is executed through one of the existing bounded pipelines:

```text
VAAPI or software decode
  -> explicit plan transfer/interop nodes
  -> CPU or Vulkan ASCII
  -> explicit plan transfer/interop nodes
  -> software or VAAPI H.264 encode
```

Two capacity-three crossbeam channels provide bounded backpressure and ordered
single-consumer delivery. The backend contract explicitly separates `submit`
from `drain`: synchronous CPU backends return immediately from `submit`, while
the production Vulkan backend may retain at most two frames and returns only
the oldest completed frame. End-of-input drains those frames before the
processed channel closes. Each stage owns its input. A stage error sets the
shared cancellation flag, stops sends/receives through short bounded waits,
closes the channels, and is returned with the failing stage name. This avoids
an upstream producer blocking forever after a downstream failure. Cancellation
does not emit buffered frames; dropping the Vulkan backend joins both slot
workers after their pending device work is safe to destroy.

Metrics are independent of backend implementation. CPU reports mapping and
render wall time. Vulkan timestamps the upload copy, Pass 1, Pass 2, download
copy, and the whole upload-begin to download-end GPU busy span on the device.
Separate CPU wall clocks cover the mapped upload, queue submission, fence
waiting, mapped readback, and submit-to-completion per-frame latency.
With two slots the busy span can include another frame's interleaved queue
work; it is an elapsed device timeline span, not exclusive execution cost.
Fence wait overlaps the timestamped GPU work by definition, so these values are
diagnostic scopes rather than additive pipeline slices. VAAPI packet submit,
frame receive, DRM PRIME mapping, external-image create/import/bind/destroy,
download, upload, and encode submit/receive are CPU-wall scopes. The external
image→buffer copy is measured with Vulkan timestamps. Pipeline latency runs
from the decode call that produces a frame through encoder acceptance; total
FPS remains the throughput measure.

## Vulkan Stage 1 data flow

```text
Host NV12
  -> reusable upload staging buffer
  -> GPU NV12 storage buffer
  -> Pass 1: one workgroup per ASCII cell, shared-memory Y/U/V reduction
  -> aligned GpuAsciiCell storage buffer
  -> Pass 2: one logical invocation per 2x2 output block
  -> GPU NV12 storage buffer
  -> reusable readback staging buffer
  -> Host NV12
```

The 256-entry glyph lookup table is generated by Core and shared by CPU and
GPU mapping, which makes glyph selection bit-exact without duplicating the
floating-point S-curve in GLSL. The font crate generates the owned R8 atlas; the
Vulkan backend uploads it only when the resource key changes. Pass 2 writes Y
and interleaved UV directly and creates no full-frame RGB/RGBA intermediate.

Pass 1 has a checked u32 common path and a u64 correctness fallback. The host
proves the cell sums, counts, boundary numerators, indexes, frame byte length,
and cell count before selecting u32. The default u32 shader uses 32 lanes after
the Stage 1.3 Intel Arc workgroup sweep. `shaderInt64` remains a device
requirement because otherwise-supported large one-cell workloads can exceed a
u32 reduction sum. `--vulkan-mapping cpu` is an explicit diagnostic hybrid:
the CLI composes the existing CPU mapper with Vulkan Pass 2 without adding a
CPU dependency to the Vulkan crate. `auto` continues to select GPU mapping.

The production Vulkan backend has two bounded worker slots on one logical
device and compute queue. Each slot owns its upload/input/cell/output/readback
buffers, descriptor state, command buffer, fence, and query pool; queue submits
are externally synchronized. FIFO sequence numbers, rather than PTS, define
completion order because PTS need not be unique or monotonic. Both slots retain
the existing three-command-buffer upload/compute/download path, so the measured
1-slot/2-slot comparison does not conflate command batching with concurrency.
`ASCIIFLOW_VULKAN_FRAME_SLOTS=1` is the diagnostic one-slot control; the default
is two after the real-device production gate passed. Resources are fixed to the
constructor's geometry/configuration while frames are pending.

Synchronization2 barriers cover transfer to compute, Pass 1 to Pass 2, and
compute to transfer. A failed queue submission or device loss makes the
pipelined backend terminal; Stage 1 does not attempt recovery. Fence waits are
still intentionally blocking. A driver that wedges without reporting device
loss can therefore stall the process; bounded waits need a safe abandoned-device
teardown design rather than merely timing out while resources remain in flight.

Host NV12 is tightly packed by the current `HostFrame` contract, so both plane
strides equal width. This is an implementation invariant of Host storage, not a
promise for a future pitched hardware-frame domain.

GLSL is compiled to embedded SPIR-V at build time by the Rust `shaderc` crate,
targeting Vulkan 1.3 with performance optimization. Runtime execution needs the
system Vulkan loader and a suitable ICD, not a shader compiler or Vulkan SDK.
Set `ASCIIFLOW_VULKAN_VALIDATION=1` to require the Khronos validation layer and
enable its synchronization validation feature.
CPU Vulkan devices are excluded from normal selection; the
`ASCIIFLOW_VULKAN_ALLOW_CPU=1` escape hatch exists only for software-driver CI
or development checks.

## Stage 3A VAAPI-to-Vulkan input interop

The `--vaapi-vulkan-input-interop on` path requires explicit VAAPI decode,
Vulkan processing, the two-slot configuration, and all four Linux external
memory/modifier/foreign-queue extensions. `auto` may select it only after the
actual input frame and Vulkan external-image access have been qualified. It
never falls back to `hwdownload` when explicitly requested.

```text
retained AV_PIX_FMT_VAAPI frame
  -> av_hwframe_map(READ | DIRECT)
  -> retained DRM PRIME mapped frame
  -> validated descriptor snapshot + duplicated CLOEXEC fds
  -> dedicated Vulkan external R8 and R8G8 images
  -> FOREIGN_EXT acquire in GENERAL layout
  -> two image-to-buffer copies into the existing tightly packed NV12 input
  -> FOREIGN_EXT release back to GENERAL
  -> unchanged Pass 1 / Pass 2 / cached Host output readback
```

The validated Intel iHD descriptor has one DMA-BUF object and separate R8 and
GR88 layers. Each duplicated fd is consumed by one successful Vulkan memory
import; failure leaves its `OwnedFd` responsible for closing it. The original
fd remains owned by FFmpeg. The mapped frame retains the source VAAPI surface,
and the worker job retains both until the slot fence has completed. This keeps
the decoder pool from reusing a surface while Vulkan is reading it.

`av_hwframe_map(READ)` synchronizes the VAAPI producer in the validated iHD
implementation. Vulkan uses explicit FOREIGN_EXT ownership acquire/release;
Linux DMA-BUF implicit synchronization and the GENERAL-layout convention are
accepted only for the tested Intel iHD 26.1.5 + Mesa ANV 26.1.8 combination.
They are not asserted as a portable Vulkan guarantee.

Stage 3A creates/imports two images per frame. The measured lifecycle is well
below the 0.2 ms cache gate, so no surface cache or fd-number identity shortcut
was introduced. This path is not end-to-end zero-copy: an image→buffer GPU copy
and Vulkan output→Host readback remain.

## Stage 3B Vulkan-to-VAAPI output interop

The `--vaapi-vulkan-output-interop on` path requires VAAPI encode, Vulkan GPU
mapping, and two bounded slots. It can be combined with Stage 3A input
interop for the full GPU-resident path, or used independently with software
decode and Host input. `auto` selects it only after the encoder-owned surface
and writable external-image access have been qualified. It never falls back
to Host readback or `hwupload` when explicitly requested.

```text
encoder's existing AVHWFramesContext
  -> fresh AV_PIX_FMT_VAAPI input surface
  -> av_hwframe_map(WRITE | OVERWRITE | DIRECT)
  -> validated DRM PRIME descriptor + duplicated CLOEXEC fds
  -> dedicated writable Vulkan R8 and R8G8 external images
  -> FOREIGN_EXT acquire in GENERAL layout
  -> unchanged Pass 2 output buffer copied into Y/UV images
  -> FOREIGN_EXT release back to GENERAL
  -> slot fence completion
  -> unmap DRM PRIME while retaining the original VAAPI frame
  -> common h264_vaapi submit/receive path
```

The encoder pool, not Vulkan, determines allocation, pitch, modifier, tiling,
and codec compatibility. The actual encoder descriptor is probed separately
even though the validated Intel iHD surface matches the decoder's one-object
R8+GR88, 4-tiled shape. Read and write imports share one Vulkan external-image
implementation; access mode selects TRANSFER_SRC or TRANSFER_DST capability,
usage, barriers, and layouts.

The production return type remains private to the interop/media composition:
Core sees only portable hardware/Host/interop plan concepts and metrics, never
an AVFrame, fd, DRM format, modifier, VkImage, or VA surface. The output surface
and mapped frame remain alive until the Vulkan fence and foreign release are
complete. The mapped frame is then dropped and the original hardware frame is
consumed by the existing encoder packet/mux path.

Stage 3B is GPU-resident for pixels but intentionally synchronous. It retains
the Stage 3A image-to-buffer copy, the existing Pass 1/2 buffers and shaders,
and an output buffer-to-image copy. It introduces no sync-file bridge, external
semaphore, direct shader image writes, or import cache; measured costs do not
justify those changes.

For output-only interop, software or VAAPI decoding produces the Host input
consumed by the Vulkan backend, while the final Vulkan buffer is copied into an
encoder-owned writable DMA-BUF surface. This path is partially staged, not
GPU-resident from decode through encode; the `PixelPath` classification makes
that distinction explicit.

## Media contract and current limitations

- Software decode and VAAPI decode are selected from the input requirements and
  capability snapshot. The current pipeline contract is H.264, 8-bit 4:2:0,
  NV12-compatible input. Ten-bit input is rejected before output creation; it
  is never silently reduced to 8-bit.
- MP4/H.264 software output uses `libx264`; explicit-transfer VAAPI output uses
  `h264_vaapi`. Explicit hardware requests never fall back.
- `auto` prefers full interop, then qualified staged hardware encode, then
  software media/Vulkan, and finally CPU processing. It does not choose VAAPI
  decode plus `hwdownload` solely because a VAAPI device exists.
- VAAPI supports only 8-bit 4:2:0 NV12 semantics. Stage 3A currently accepts
  the observed single-object R8+GR88 iHD export with a known, importable
  modifier. P010/HDR, 4:4:4, multi-object, unknown-modifier, and incompatible
  layer topologies are rejected rather than copied or silently reduced.
- Compatible compressed audio streams can be copied into MP4. One mux worker
  owns the output `AVFormatContext` and accepts both encoded video packets and
  passthrough audio packets through a bounded queue. It rescales timestamps
  with each explicit input/output stream mapping and performs every
  `av_interleaved_write_frame` call. Audio is never decoded or transcoded.
- Host NV12 remains the portable Core/reference contract. Full interop and
  output-only interop bypass some Host materialization without changing that
  contract or making native handles part of Core.
- Built-in 8x8 remains the default; explicit scalable monospaced FreeType fonts
  provide grayscale tiles without changing glyph ordering or grid geometry.
- Source presentation timestamps are represented on decoded frames, but Stage
  0 encoding uses an ordered constant-frame-rate sequence based on the source
  frame-rate rational. With audio selected, the mux layer preserves the first
  video's timestamp origin and rejects source timestamps that deviate from
  that CFR sequence. See [audio.md](audio.md) for timing and frame-limit details.
- Output commit uses the host's rename semantics. Fedora/Linux replaces an
  existing destination atomically; cross-platform replacement semantics need a
  dedicated Stage after the Rust baseline.
- Temporary output names are hidden, per-invocation names containing process,
  timestamp, and sequence components. The CLI reserves them with exclusive
  creation before encoder startup.
- Ctrl+C sets a cooperative cancellation token and returns status 130 only
  after workers have joined and the staging file has been removed. It never
  commits a partial output.

## Verification

Static verification:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace
```

Stage 4.0 planner tests use synthetic capability snapshots and therefore do
not require a GPU. They cover full GPU-resident selection, missing input or
output interop, missing VAAPI encode/device, unsuitable Vulkan, explicit
software/CPU overrides, conflicting policies, unsupported explicit hardware,
10-bit rejection, stable domains, and printable plan output. Native CLI smoke
tests remain opt-in and are required for the real capability probe,
`--capabilities`, `--explain-plan`, automatic full interop, explicit-versus-
automatic parity, and the 300-frame performance comparison.

The real-media test is opt-in because it requires native libraries and a local
fixture:

```bash
ASCIIFLOW_TEST_VIDEO=input.mp4 cargo test -p asciiflow-cli --test media_smoke -- --ignored
ASCIIFLOW_TEST_VIDEO=input.mp4 cargo test -p asciiflow-cli --test media_smoke converts_real_video_with_vulkan -- --ignored
```

Unit tests cover deterministic NV12 mapping/rendering, exact CPU/Vulkan parity,
ordered ownership, failure cancellation, and allocation through FFmpeg RAII wrappers. A native
smoke run plus independent `ffprobe` is still required before claiming a given
FFmpeg build or target platform is supported.

Current Intel Arc compute evidence is recorded in
[`stage1-validation.md`](stage1-validation.md). VAAPI capability, correctness,
and the Host-transfer baseline are recorded in
[`stage2-validation.md`](stage2-validation.md). DMA-BUF descriptor, parity,
stress, validation, and benchmark evidence is in
[`stage3a-validation.md`](stage3a-validation.md). Encoder descriptor, writable
import, parity, lifetime, validation, and benchmark evidence is in
[`stage3b-validation.md`](stage3b-validation.md).

Stage 4.0 planning facts and policy details are recorded in
[`auto-planner.md`](auto-planner.md). The implementation performs one
capability probe per invocation and reports its duration separately from video
processing metrics. An auto-selected capability that fails while the unified
execution factory constructs resources is excluded and replanned exactly once.
Explicit policy failures and every failure after processing begins remain
terminal; there is no mid-stream fallback.

Stage 4.1's lifecycle, structured errors, first-failure propagation, bounded
GPU teardown, cancellation, and transactional output guarantees are specified
in [`failure-semantics.md`](failure-semantics.md).

Stage 1.3 permits exactly two GPU frames in flight. Stage 2's explicit VAAPI ↔
Host NV12 transfer remains the reference path. Stage 3A adds decode-side
DMA-BUF import and Stage 3B adds encoder-owned writable DMA-BUF import. Neither
stage adds a transfer queue, timeline semaphore, sync-file bridge, or direct
image shader access. Any later work must preserve Host NV12 and the CPU backend
as usable reference boundaries rather than silently replacing them.
