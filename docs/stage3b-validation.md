# Stage 3B Vulkan to VAAPI encode-surface interop

This record captures the 2026-09-14 Stage 3B implementation and validation on
Intel Arc Meteor Lake. The production pixel path is now GPU-resident from
VAAPI decode through Vulkan processing into the VAAPI encoder. It still uses
CPU fence waits and two GPU buffer/image copies; it is zero-host-copy for
pixels, not a fully asynchronous cross-API pipeline.

## Encoder-owned surface descriptor

The surface is allocated from the exact `AVHWFramesContext` configured on
`h264_vaapi`, then mapped with
`AV_HWFRAME_MAP_WRITE | AV_HWFRAME_MAP_OVERWRITE |
AV_HWFRAME_MAP_DIRECT`. The first real surface reported:

```text
frame                 1920 x 1080
objects               1
object 0 size          3,194,880 bytes
object 0 modifier      0x0100000000000009
modifier name          I915_FORMAT_MOD_4_TILED

layers                 2
layer 0 fourcc         DRM_FORMAT_R8
  object               0
  offset               0
  pitch                1920
layer 1 fourcc         DRM_FORMAT_GR88
  object               0
  offset               2,088,960
  pitch                1920
```

This is byte-for-byte the same shape as the Stage 3A decoder surface, but the
implementation probes and validates the encoder surface independently. Direct
write mapping succeeded; the production path never substitutes READ mapping
or a hidden Host copy.

## Ownership and synchronization

`Encoder::encoder_frames()` clones the encoder's existing frames-context
reference. The two-slot interop processor acquires a fresh encoder-compatible
surface for each submitted frame. The output DRM mapping retains that surface
while Vulkan imports and writes it. Before submission, the encoder also checks
the underlying `AVHWFramesContext` identity, not merely frame dimensions; a
surface acquired from a second otherwise-identical encoder is rejected.

The original FFmpeg descriptor fd remains FFmpeg-owned. Interop duplicates it
with `F_DUPFD_CLOEXEC`; successful Vulkan allocation consumes the duplicate,
while all failure paths retain `OwnedFd` cleanup. Images use dedicated external
memory allocation, exact descriptor modifier/offset/pitch, and only
`TRANSFER_DST` usage. Runtime modifier and external-image queries require
`TRANSFER_DST`, one Vulkan memory plane, and importable DMA-BUF memory.
Before any driver call, the Vulkan boundary validates exact Y/UV dimensions,
minimum row pitch, checked object ranges, matching modifiers, plane order, and
read/write access.

The output command performs:

```text
output NV12 buffer: COMPUTE_SHADER / SHADER_WRITE
    -> COPY / TRANSFER_READ

encoder Y and UV images:
    FOREIGN_EXT / GENERAL
    -> Vulkan queue / TRANSFER_DST_OPTIMAL

vkCmdCopyBufferToImage:
    Y  offset 0           -> R8   1920 x 1080
    UV offset 1920*1080   -> R8G8  960 x 540

TRANSFER_WRITE / TRANSFER_DST_OPTIMAL
    -> FOREIGN_EXT / GENERAL
```

The slot waits for its Vulkan fence before destroying imported images,
unmapping DRM PRIME, and passing the original VAAPI frame to `h264_vaapi`.
This deliberately uses CPU synchronization. No sync-file bridge, external
semaphore, queue-idle workaround, or sleep was introduced.

## Correctness and lifetime evidence

- Thirty encoder surfaces were written by Vulkan, diagnostically downloaded
  before encoding, and compared byte-for-byte with the Stage 3A Host ASCII
  output. Y, UV, dimensions, metadata, order, and encoder CFR PTS were exact.
- The production 30-frame output decoded successfully. Its decoded framemd5
  was identical to the Stage 3A plus hwupload reference for every frame.
- Both outputs contained 30 frames at 1920x1080, 50 FPS, 0.6 seconds, yuv420p,
  limited-range BT.709, and left chroma location.
- A 3,000-frame full decode/input-interop/Vulkan/output-interop/VAAPI-encode
  stress test compared every pre-encode frame byte-for-byte. There was no stale
  surface, corruption, reorder, pool exhaustion, deadlock, or device loss.
- FD counts were 4 before initialization, 36 after both pipelines/pools were
  initialized, at most 39 during steady-state sampling, and 4 after complete
  teardown.
- Invalid output DMA-BUF import and teardown with two pending full-interop
  slots both returned to the initialized FD baseline. The deliberately invalid
  import test was run without validation because the Vulkan call is expected
  to report an invalid external handle.
- A real-VAAPI negative test proved that an encoder refuses a surface from a
  different `AVHWFramesContext`. Unit tests also reject malformed external
  plane geometry and out-of-bounds object ranges before Vulkan import.
- Khronos validation plus synchronization validation on the real 30-frame
  parity and integrated production runs reported zero Validation Errors,
  VUIDs, or synchronization errors.

## Timing semantics

DRM map, surface acquire, capability query, image creation/import/bind,
ownership-command recording, destruction, queue submit, fence wait, decode,
and encode are CPU wall-clock scopes. Image-to-buffer, mapping, render, and
buffer-to-image are Vulkan timestamp scopes. Fence waits overlap GPU work and
must not be added to device timestamps as a serial execution model.

Stage 3B production reports zero for output Host invalidate, output Host
memcpy, and VAAPI hwupload. Diagnostic parity readback is excluded from the
production benchmark.

## Formal benchmark

Workload: Stage 3A `input.mp4`, 1920x1080 at 50 FPS, 300 frames, ASCII width
80, standard charset, color, Release, pinned FFmpeg 9.0.1, two Vulkan slots.
Validation was disabled. Each path used a discarded 30-frame warm-up followed
by three formal runs.

```text
A  Stage 3A input interop -> Host readback -> VAAPI hwupload
   FPS: 449.75, 453.66, 452.43
   median: 452.43 FPS, 146% CPU, 23.337 ms latency

B  Stage 3B full input/output interop
   FPS: 514.13, 514.91, 521.07
   median: 514.91 FPS, 58% CPU, 20.231 ms latency
```

Component-wise medians:

| Metric | A: Stage 3A + hwupload | B: Stage 3B |
| --- | ---: | ---: |
| Decode CPU wall | 0.253 ms | 0.206 ms |
| Input DRM map CPU wall | 0.050 ms | 0.049 ms |
| GPU input image -> buffer | 0.169 ms | 0.157 ms |
| GPU mapping | 0.301 ms | 0.281 ms |
| GPU render | 0.148 ms | 0.137 ms |
| GPU output/readback copy | 0.167 ms | 0 |
| Host invalidate | 0.085 ms | 0 |
| Host output memcpy | 0.340 ms | 0 |
| VAAPI hwupload | 1.478 ms | 0 |
| Encoder-surface acquire | - | 0.010 ms |
| Encoder DRM WRITE map | - | 0.008 ms |
| Output create/import/bind/destroy | - | 0.022 ms |
| Output acquire/release recording | - | 0.001 ms / <0.001 ms |
| GPU output buffer -> image | - | 0.113 ms |
| Output queue submit / fence wait | - | 0.021 / 0.451 ms |
| Aggregate GPU wait | 1.717 ms | 1.683 ms |
| Backend wall | 2.873 ms | 2.133 ms |
| Encode CPU wall | 2.154 ms | 1.885 ms |

The controlled same-batch improvement is 1.138x, or 13.8%. Average
decode-entry-to-encoder-acceptance latency fell 13.3%, from 23.337 to 20.231
ms. Process CPU utilization fell from 146% to 58%. The prior 346.05 FPS Stage
3A snapshot is retained as historical evidence, but is not used as the formal
speedup denominator because the integrated GPU exhibited substantial
frequency/shared-power variance between batches.

All six formal files decoded without error and contained 300 frames with
strictly increasing PTS/DTS, 1920x1080 yuv420p, 50 FPS, six-second duration,
limited-range BT.709, and left chroma location. Corresponding A/B decoded
framemd5 manifests were identical for all three runs.

## Decision

The former output round trip -- 0.167 ms GPU readback copy, 0.085 ms Host
invalidate, 0.340 ms Host memcpy, and 1.478 ms VAAPI hwupload -- has been
replaced by roughly 0.040 ms of encoder surface/map/import lifecycle,
approximately 0.001 ms of ownership-command recording, and a 0.113 ms GPU
buffer-to-image copy. These are different clock domains and are not summed to
predict throughput.

The largest remaining independent CPU scope is VAAPI encoder submit/receive at
1.885 ms/frame; aggregate Vulkan fence wait is 1.683 ms/frame and overlaps the
GPU timeline. The output-specific fence wait is only 0.451 ms/frame, so the
data does not yet justify a sync-file/external-semaphore bridge. The 0.113 ms
buffer-to-image copy is also not large enough to justify rewriting Pass 2 to
write the imported image directly. Per-frame output import remains well below
the 0.2 ms cache gate, so no cache was added.

The result is a genuinely GPU-resident pixel pipeline on this Intel UMA system:
CPU code still orchestrates and waits, but never maps, copies, invalidates, or
uploads production video pixels between decode and encode. Automatic policy
remains unchanged and both interop directions remain explicit opt-ins.
