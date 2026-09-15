# Stage 3A VAAPI decode to Vulkan DMA-BUF interop

This record captures the 2026-09-14 correctness and performance baseline on
Intel Arc Meteor Lake. Stage 3A removes the VAAPI-to-Host download from decode
input. It is accurately described as VAAPI→Vulkan zero-host-copy input, not
end-to-end zero-copy: Vulkan still copies the imported images into its existing
NV12 buffer and reads the processed output back to Host memory.

## Actual DRM PRIME descriptor

The first three real frames were mapped with
`av_hwframe_map(AV_HWFRAME_MAP_READ | AV_HWFRAME_MAP_DIRECT)`. They had an
identical descriptor:

```text
frame                 1920 x 1080
objects               1
object 0 fd            FFmpeg-owned, per-map value
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

Intel iHD therefore did not export one `DRM_FORMAT_NV12` layer. It exported
NV12 semantics as separate R8 and GR88 layers in one object. Stage 3A supports
this observed shape explicitly and rejects other layer ordering, multi-object
descriptors, P010, unknown modifiers, odd dimensions, invalid indexes, negative
offsets/pitches, and plane ranges outside the object.

## Vulkan import and ownership

The selected device was Intel(R) Arc(tm) Graphics (MTL), PCI `8086:7d55`, Mesa
ANV 26.1.8, Vulkan 1.4.354. It exposes and the application enables:

- `VK_KHR_external_memory_fd`
- `VK_EXT_external_memory_dma_buf`
- `VK_EXT_image_drm_format_modifier`
- `VK_EXT_queue_family_foreign`

Runtime format-property and image-format-property queries confirmed the exact
modifier for both `VK_FORMAT_R8_UNORM` and `VK_FORMAT_R8G8_UNORM`, one Vulkan
memory plane per image, `TRANSFER_SRC`, and importable DMA-BUF external memory.
The Y image is 1920x1080 R8; the UV image is 960x540 R8G8. Both use explicit
DRM-modifier layout with the descriptor's offsets and pitches.

The original descriptor fd always belongs to FFmpeg. Interop duplicates it
twice using `F_DUPFD_CLOEXEC`. A successful dedicated Vulkan memory allocation
consumes one duplicate; a failed query/allocation leaves `OwnedFd` responsible
for closing it. `vkGetMemoryFdPropertiesKHR` supplies the memory-type mask,
which is intersected with the image requirements before allocation. Image and
memory teardown is RAII-safe on every later failure.

The mapped DRM frame retains the source VAAPI frame. Each of the two bounded
worker slots owns both until its Vulkan fence completes, then destroys the
imported images and releases the mapped/source frames. Cancellation joins slot
workers before releasing their state.

For synchronization, FFmpeg's iHD VAAPI mapping synchronizes the producer
surface for `AV_HWFRAME_MAP_READ`. Vulkan acquires each read-only image from
`VK_QUEUE_FAMILY_FOREIGN_EXT` in `GENERAL`, transitions it to
`TRANSFER_SRC_OPTIMAL`, copies it, and releases it back to FOREIGN/`GENERAL`.
The target-specific Linux DMA-BUF implicit-fencing behavior was then exercised
by the parity and surface-reuse stress tests. This result is scoped to Intel
iHD 26.1.5 plus Mesa ANV 26.1.8; it is not a generic Vulkan guarantee.

## Image-to-buffer path and cache decision

Two `vkCmdCopyImageToBuffer` regions write the unchanged Vulkan-owned input:

```text
R8   1920x1080 -> offset 0
R8G8  960x540  -> offset 1920*1080
```

Pass 1 u32-32, Pass 2 LUT/32x4, their descriptors and shaders, cached output
readback, and the two-slot structure are unchanged. The production path uses
correctness-first per-frame map/create/import/bind/destroy. Its median
create+import+bind+destroy cost was about 0.028 ms/frame (about 0.045 ms with
the capability query), far below the 0.2 ms cache gate. No imported-surface
cache was added.

## Correctness, stress, and failure paths

- Pre-ASCII diagnostic: 30 frames of imported image→buffer→Host NV12 were
  byte-exact against the Stage 2 `av_hwframe_transfer_data` reference for Y and
  UV, with matching dimensions, PTS, and color metadata.
- Full processing: 30 frames through two interop slots were byte-exact against
  VAAPI hwdownload→existing Vulkan Pass 1/2 output.
- Surface reuse: 3,000 frames from a 3,200-frame 1920x1080/50 FPS H.264 fixture
  were compared byte-for-byte against the Stage 2 reference output. There was
  no corruption, stale frame, reorder, deadlock, or device loss.
- FD lifetime: steady-state `/proc/self/fd` sampling every 250 frames remained
  within eight descriptors of the initialized baseline and returned within
  four descriptors of the pre-stream count after drain/destruction. It did not
  grow with frame count.
- Failure paths: an invalid import fd returned an explicit DMA-BUF error and
  released both duplicates; dropping a processor with two pending frames
  joined the work and returned fd usage near baseline.
- Vulkan and synchronization validation on the descriptor test, 30-frame raw
  parity, 30-frame full parity, and integrated CLI smoke reported zero
  Validation Errors, VUIDs, or synchronization errors.
- All nine formal outputs decoded without errors and contained 300 frames,
  strictly increasing PTS/DTS, 1920x1080 yuv420p, 50 FPS, six-second duration,
  BT.709 primaries/transfer/matrix, limited range, and left chroma location.

## Timing semantics

DRM mapping, capability queries, image creation, DMA-BUF import, memory binding,
ownership command recording, destruction, decode, encode, and Host readback
are CPU wall-clock scopes. External image→buffer, mapping, render, and output
copy are Vulkan timestamp scopes. GPU wait overlaps the timestamped GPU work;
the values are diagnostic and must not be added as a serial latency model.
Total FPS is pipeline throughput and pipeline latency runs from decode-call
start through encoder acceptance.

## Formal benchmark

Workload: the Stage 2 `input.mp4`, 1920x1080, 50 FPS, 300 frames, ASCII width
80, standard charset, color, Release, pinned FFmpeg 9.0.1, two Vulkan slots,
cached output readback. Validation was disabled. Each path used a discarded
30-frame warm-up followed by three formal runs. Component values below are
component-wise medians; CPU is `/usr/bin/time` process utilization.

| Path | Decode wall | hwdownload | DRM map | import lifecycle | GPU image→buffer | Vulkan wall | Encode wall | Latency | FPS | CPU |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| A: VAAPI download + Vulkan + SW encode | 7.154 ms | 6.068 ms | - | - | - | 5.888 ms | 2.215 ms | 23.681 ms | 138.40 | 270% |
| B: VAAPI DMA-BUF + Vulkan + SW encode | 0.905 ms | 0 | 0.169 ms | 0.028 ms | 0.247 ms | 8.235 ms | 1.970 ms | 30.901 ms | 238.70 | 306% |
| C: VAAPI DMA-BUF + Vulkan + existing VAAPI encode | 0.478 ms | 0 | 0.059 ms | 0.027 ms | 0.160 ms | 5.203 ms | 2.115 ms | 24.876 ms | 346.05 | 112% |

FPS runs were A: 143.57, 137.74, 138.40; B: 243.31, 226.38, 238.70; C:
368.85, 343.95, 346.05. The integrated GPU showed substantial frequency and
shared-power variance in additional diagnostic repetitions, so conclusions use
the formal medians and the large transfer delta, not the fastest observed run.

For B, the remaining median interop CPU scopes were capability query 0.017 ms,
image creation 0.006 ms, DMA-BUF import 0.010 ms, memory bind 0.001 ms,
ownership-barrier command recording 0.011 ms, and destruction 0.011 ms. GPU
mapping/render/output-copy were 0.478/0.293/0.240 ms; Host output memcpy was
0.326 ms. GPU wait was 6.002 ms and is overlapping wall time, not an additional
device stage.

Stage 3A replaced the 6.068 ms Host hwdownload plus Host/Vulkan upload with
roughly 0.225 ms of CPU-side map/import/lifecycle work and a 0.247 ms GPU copy
in B. These are different clock domains, but either view shows that the former
six-millisecond transfer was removed. B is 1.725x faster than the matching
Stage 2 VAAPI-download path. It is still 19.4% below Stage 2's 296.16 FPS pure
software-decode/software-encode baseline in this formal batch. C reaches
346.05 FPS, 16.8% above that software baseline, while reducing reported
process CPU from Stage 2's 500% to 112%.

## Decision

The Stage 3A objective is met: hwdownload is absent, imported NV12 bytes and
full ASCII output are exact, ownership is bounded, and the original transfer
cost has been replaced by sub-millisecond interop. The current largest timing
scope is Vulkan queue/fence wait and backend wall under the shared integrated
GPU, not descriptor parsing, image creation, or DMA-BUF import. Because the
per-frame import lifecycle is only about 0.03 ms, a surface cache is not
justified.

For the existing VAAPI-encode path, output Host readback consists of about
0.154 ms GPU copy, 0.089 ms invalidate, and 0.339 ms Host memcpy, followed by a
1.499 ms hwupload. These GPU-timestamp and CPU-wall-clock values describe
different timelines and must not be added as though they were one serial timer.
That is now a meaningful remaining round trip and supplies evidence for a
focused Stage 3B encode-side interop investigation. Stage 3B is not implemented
here.

Stage 3B was subsequently implemented and validated without changing this
Stage 3A reference path; see [`stage3b-validation.md`](stage3b-validation.md).
