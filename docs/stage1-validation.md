# Stage 1 validation record

This record captures the 2026-09-13 to 2026-09-14 correctness and timing validation on the
real Vulkan device. It supersedes the earlier lavapipe-only measurements; no
software Vulkan timing is used for an architecture decision.

## Environment

- Device: Intel(R) Arc(tm) Graphics (MTL), integrated GPU
- PCI IDs: vendor `0x8086`, device `0x7d55`
- Driver: Mesa ANV 26.1.8
- Device Vulkan API: 1.4.354
- Render node: `/dev/dri/renderD128`
- Khronos Validation Layer: 1.4.341
- SPIR-V Tools: 2026.1
- Runtime FFmpeg: 8.1.2

Normal Vulkan selection excludes CPU devices. Every hardware run named below
selected Intel Arc explicitly in the CLI output; neither llvmpipe nor lavapipe
was used.

## Shader validation fix

`uint8_t(128)` in the monochrome chroma stores was constant-folded by shaderc
to an illegal 8-bit `OpConstant`. `StorageBuffer8BitAccess` permits byte values
in storage buffers, but it does not permit general 8-bit arithmetic or 8-bit
constants.

The neutral chroma value is now a 32-bit push constant. NV12 and atlas values
are loaded from byte storage and immediately widened to 32-bit values. All
constants, comparisons, blending, and index calculations remain 32-bit (or
64-bit where the existing overflow-safe coordinate calculation requires it),
and only the final storage write converts back to `uint8_t`. The Vulkan device
still enables `storageBuffer8BitAccess`; it does not request `shaderInt8`.

Both Debug and Release build artifacts were checked explicitly:

```bash
spirv-val --target-env vulkan1.3 target/debug/build/asciiflow-vulkan-*/out/ascii_map_*.spv
spirv-val --target-env vulkan1.3 target/debug/build/asciiflow-vulkan-*/out/ascii_render_*.spv
spirv-val --target-env vulkan1.3 target/release/build/asciiflow-vulkan-*/out/ascii_map_*.spv
spirv-val --target-env vulkan1.3 target/release/build/asciiflow-vulkan-*/out/ascii_render_*.spv
```

Result: every artifact present after the final all-target Debug and Release
build was checked, with zero SPIR-V validation errors. (Cargo produces
multiple Debug build-script directories for distinct feature graphs.)
Disassembly contains the required 8-bit storage type and runtime 32-to-8
conversions, but no 8-bit or 16-bit `OpConstant`/`OpSpecConstant`.

## Vulkan validation and parity

`ASCIIFLOW_VULKAN_VALIDATION=1` enables the Khronos layer and explicitly enables
synchronization validation. The direct-backend parity tests ran serially with
Rust's `--test-threads=1`; the separate real-media smoke used the normal bounded
decode/process/encode pipeline. The final hardware runs passed:

- Vulkan device initialization: PASS
- Pass 1 CPU/GPU cell parity: PASS, exact glyph/Y/U/V equality
- Final NV12 CPU/GPU parity: PASS in color and monochrome, exact frame equality
- Real 1920x1080 media smoke: PASS, three frames encoded
- Validation errors: 0
- VUID errors: 0
- Synchronization errors: 0

The three-frame CPU and Vulkan smoke outputs were byte-identical H.264 files
(`909830104b2c905bd3501179b074dff602066206b7939f054d18f279e81793f5`) and their
independently decoded frames had the same SHA-256
(`c08cfeba1ca063360669fa8e2ec0c7700ca546bbf768b7a286851bc057c2fdf2`). The full
49-frame benchmark outputs were also byte-identical
(`9c19a0704d7c100886aa420384c4f2398d071568a5d9bc0048ac197d055d58ee`) and decoded
to the same SHA-256
(`65546d7e153f259f54321d437ed1e4cd7b681787ba5f6a12b10dfd422e91b79c`).
`ffprobe` reported H.264, 1920x1080, yuv420p, BT.709 limited range, 25 FPS, and
49 frames.

## Metric semantics

The old `upload` and `download` values were synchronized CPU wall intervals.
They mixed mapped-memory access, command submission, fence waiting, and copying;
in particular, `download` was not a GPU transfer measurement. It did not include
the earlier compute fence wait, but it did include the download submission's
fence wait and host readback.

The backend now records independent quantities:

| Metric | Clock/source | Exact boundary |
| --- | --- | --- |
| host upload | CPU wall | mapped staging write and flush only |
| queue submit | CPU wall | the three `vkQueueSubmit2` calls only |
| GPU upload/copy | Vulkan timestamp | immediately around staging-to-input `vkCmdCopyBuffer` |
| GPU mapping | Vulkan timestamp | immediately around the Pass 1 dispatch |
| GPU render | Vulkan timestamp | immediately around the Pass 2 dispatch |
| GPU download/copy | Vulkan timestamp | immediately around output-to-readback `vkCmdCopyBuffer` |
| GPU wait | CPU wall | the three `vkWaitForFences` calls only |
| host invalidate | CPU wall | non-coherent mapped-memory invalidate after fence wait |
| host readback | CPU wall | mapped-memory memcpy into owned Host NV12 bytes |
| backend wall | CPU wall | complete backend `process` call |

`GPU wait` is host-observed synchronization latency and necessarily overlaps
the device work measured by GPU timestamps. `backend wall`, decode, encode, and
end-to-end pipeline time also have different scopes. The CLI labels clock
sources and explicitly marks the CPU-wall and GPU-timestamp groups as
non-additive.

## Release benchmark

Workload: the preserved Stage 0 baseline sample, 1920x1080, 25 FPS, 49 frames,
ASCII width 80, standard charset, color enabled. The Release binary ran with
Validation Layer disabled only after all validation checks above were clean.
Each backend had one discarded warm-up followed by three full measured runs.
Each table entry is the median of that metric across the three runs.

### CPU

| Metric | Median |
| --- | ---: |
| decode | 2.151 ms/frame |
| mapping | 0.761 ms/frame |
| render | 16.257 ms/frame |
| backend wall | 17.018 ms/frame |
| encode | 2.468 ms/frame |
| end-to-end | 57.78 FPS |

Measured FPS runs: 56.59, 57.78, 58.31.

### Vulkan

| Metric | Source | Median |
| --- | --- | ---: |
| decode | CPU wall | 2.276 ms/frame |
| host upload | CPU wall | 0.269 ms/frame |
| GPU upload/copy | GPU timestamp | 0.122 ms/frame |
| GPU mapping | GPU timestamp | 1.129 ms/frame |
| GPU render | GPU timestamp | 14.391 ms/frame |
| GPU download/copy | GPU timestamp | 0.123 ms/frame |
| GPU wait | CPU wall | 17.090 ms/frame |
| queue submit | CPU wall | 0.734 ms/frame |
| host readback | CPU wall | 13.169 ms/frame |
| backend wall | CPU wall | 31.490 ms/frame |
| encode | CPU wall | 2.509 ms/frame |
| end-to-end | CPU wall | 31.44 FPS |

Measured FPS runs: 31.69, 31.27, 31.44.

The two GPU copies total 0.245 ms/frame, 0.78% of backend wall. Host upload plus
host readback total 13.438 ms/frame, 42.7% of backend wall; almost all of that is
the 13.169 ms mapped readback. The 17.090 ms host fence wait is 54.3% of backend
wall, but it overlaps 15.765 ms of timestamped GPU copy and compute work and
must not be added to it.

Vulkan achieves 0.544x CPU end-to-end throughput; equivalently, CPU is 1.84x
faster. Pass 1 is slower on the GPU (1.129 vs 0.761 ms, CPU 1.48x faster).
Pass 2 is modestly faster on the GPU (14.391 vs 16.257 ms, GPU 1.13x faster),
but not enough to offset synchronization and host readback.

## Pass 2 diagnosis, without optimization

At 1920x1080, Pass 2 dispatches 120x68 workgroups of 8x8 invocations: 522,240
invocations for 518,400 useful 2x2 blocks, only 0.74% boundary excess. The color
mode is a uniform push constant, so its branch is not divergent; only the final
partial workgroup row takes the bounds return.

The 80x45 `GpuAsciiCell` buffer is 57,600 bytes and the ten-glyph 8x8 R8 atlas
is 640 bytes, so raw resource size is not evidence of bandwidth pressure. The
hot path nevertheless performs repeated cell lookup and atlas-coordinate
calculations for every 2x2 output block. Release SPIR-V contains 64-bit divide
and modulo operations for atlas coordinates, plus 32-bit output/cell division.
Each useful invocation emits four Y and two interleaved UV byte stores directly
to the 3,110,400-byte NV12 output. These subword-store transaction costs remain
a hypothesis until measured by a narrower output-only probe.

As a bounded diagnostic, a monochrome run retained the same dispatch, four Y
atlas lookups, cell accesses, and byte output while bypassing colored UV work.
Its median GPU render time was 15.257 ms/frame, not an improvement over the
14.391 ms color median. This rules out the colored UV branch alone as the
dominant cause, but does not yet separate repeated coordinate arithmetic from
Y-plane byte stores. No Pass 2 optimization was introduced.

The present evidence supports a later, focused compute investigation if Vulkan
must beat the CPU: first compare an output-only byte-store probe with a
precomputed-coordinate/atlas probe, then change production code only if those
measurements identify a winner. It does not support starting Host-to-GPU
transfer elimination: timestamped copies cost less than 1% of backend wall.
The large mapped host readback is a separate memory-placement/cache/read pattern
to investigate before considering zero-copy or hardware encode. Stage 2 has not
started.

## Stage 1.2: Intel UMA readback and Pass 2 diagnosis

Stage 1.2 used the same Intel Arc MTL device. Performance probes used Release
builds with validation disabled, 20 warm-up dispatches, and 120 measured
dispatches; correctness runs separately enabled Khronos synchronization
validation.

The device has one 24,774,620,160-byte `DEVICE_LOCAL` heap. Types 0 and 3 are
`DEVICE_LOCAL | HOST_VISIBLE | HOST_COHERENT`; types 1 and 4 are
`DEVICE_LOCAL | HOST_VISIBLE | HOST_CACHED`; type 2 is
`DEVICE_LOCAL | PROTECTED`. gpu-allocator 0.28 cannot satisfy its preferred
cached+coherent `GpuToCpu` flags and falls back to uncached coherent type 0.
The dedicated production readback now selects cached non-coherent type 1;
gpu-allocator still owns all other production allocations. `--verbose` reports
all heaps/types plus the selected type, heap, and decoded flags for key buffers.

Host readback is split into invalidate and memcpy after the fence. A
copy-to-host barrier explicitly orders GPU writes before host access.

| Readback | Invalidate median | 3,110,400-byte memcpy | Bandwidth |
| --- | ---: | ---: | ---: |
| allocator type 0 | 0.000026 ms | 6.579 ms | 0.440 GiB/s |
| cached type 1 | 0.063 ms | 0.239 ms | 12.1 GiB/s |

The 27.5x bandwidth difference identifies uncached mapped reads as the old
bottleneck. Direct cached host-visible output was also byte-exact and
validation-clean. Against an adjacent cached-copy control it changed backend
wall from 3.090 to 2.670 ms, but remains only an explicit experiment rather
than the default or a Stage 2 decision.

The original Release SPIR-V contained six 64-bit divides, six 64-bit modulos,
twelve 64-bit multiplies, and eighteen 64-bit conversions in atlas-coordinate
calculation. Checked host bounds permit an exact u32 path for this workload;
precomputed u32 `(cell, atlas-local)` coordinate pairs then remove the remaining
coordinate division/modulo.

| Pass 2 variant | Workgroup | GPU render median |
| --- | ---: | ---: |
| original int64 coordinates | 8x8 | 10.865 ms |
| u32 coordinates | 8x8 | 0.249 ms |
| coordinate LUT | 8x8 | 0.146 ms |
| coordinate LUT | 16x8 | 0.143 ms |
| coordinate LUT | 16x16 | 0.146 ms |
| coordinate LUT | 32x4 | **0.121 ms** |

The default is the LUT/32x4 variant. A four-byte Y store is not an isolated
legal change in the existing 2x2 ownership layout: each aligned u32 word is
shared by neighboring invocations. Packing would require a 4x2 topology change
or atomics, so no packed-store or atlas-representation change was made after
coordinate isolation removed the bottleneck.

All ten current Release SPIR-V artifacts passed `spirv-val --target-env
vulkan1.3`. Default and direct-output parity, color and monochrome final NV12,
Pass 1, and real-media smoke passed with zero Vulkan validation, VUID, or
synchronization errors.

### Stage 1.2 formal benchmark

The 49-frame Stage 0 workload remained 1920x1080, ASCII width 80, standard
charset, and color enabled. Each backend had one full warm-up and three runs;
the table contains component-wise medians.

| Metric | CPU | Vulkan | Clock source(s) |
| --- | ---: | ---: | --- |
| decode | 2.247 ms | 2.332 ms | CPU wall / CPU wall |
| mapping | 0.858 ms | 1.209 ms | CPU wall / GPU timestamp |
| render | 18.606 ms | 0.201 ms | CPU wall / GPU timestamp |
| host upload | - | 0.316 ms | CPU wall |
| GPU upload / download copy | - | 0.095 / 0.105 ms | Vulkan timestamp |
| queue submit / GPU wait | - | 0.082 / 2.955 ms | CPU wall |
| host invalidate / memcpy | - | 0.090 / 0.363 ms | CPU wall |
| backend wall | 19.531 ms | 4.157 ms | CPU wall |
| encode | 2.310 ms | 2.072 ms | CPU wall |
| end-to-end | 50.41 FPS | 225.26 FPS | CPU wall |

CPU FPS runs: 50.12, 50.41, 53.27. Vulkan FPS runs: 226.98, 225.26,
224.77. Vulkan is 4.70x faster by backend wall and 4.47x end-to-end. The two
GPU copies total 0.200 ms; invalidate plus memcpy total 0.453 ms. GPU wait
overlaps timestamped work and is not additive. Pass 1 is now the largest
timestamped compute stage; Pass 2 is no longer a bottleneck.

CPU and Vulkan outputs were byte-identical
(`9c19a0704d7c100886aa420384c4f2398d071568a5d9bc0048ac197d055d58ee`),
and their decoded frame-MD5 manifests were identical
(`41393874e551ba84beb1f3a36e77eb764fb1273adc3b909ddf960dabe20e2bf6`).
Transfer elimination could save only a small fraction of a millisecond, so
current evidence does not justify Stage 2.

## Stage 1.3: mapping strategy and two in-flight slots

Stage 1.3 kept the Stage 1.2 coordinate-LUT/32x4 Pass 2 and cached readback
unchanged. Measurements below used the real Intel Arc MTL device named above,
Release builds, disabled validation for timing, and separate validation-enabled
correctness runs.

### Pass 1 audit and u32 fast path

The original mapping SPIR-V used 28 integer adds, 10 multiplies, 14 unsigned
divides, two modulos, two static control barriers, and no atomics. Eight adds,
four multiplies, and seven divides were u64. At width 80 the 80x45 grid creates
3,600 workgroups. With 64 lanes that is 230,400 invocations; each 24x24 cell
performs 576 Y loads and 144 UV-pair loads followed by a shared-memory tree
reduction. The shader has no occupancy evidence supporting a multi-pass tiled
reduction, so none was added.

For `cw = ceil(frame_width/grid_width)`, `ch =
ceil(frame_height/grid_height)`, and `A = cw*ch`, the host selects u32 only when
`255*A <= u32::MAX`. This bounds Y/U/V totals and counts because a cell touches
no more UV pairs than luma pixels. It additionally proves
`grid_width*frame_width`, `grid_height*frame_height`, frame pixels, NV12 byte
length, and grid cell count fit u32. Otherwise it selects the u64-64 fallback.
For the baseline, the worst Y sum is 146,880, each U/V sum is 36,720, the last
NV12 byte index is 3,110,399, and the last cell index is 3,599.

`shaderInt64` remains required: an otherwise accepted large one-cell frame can
fit the backend's 32-bit byte/index limits while `255*A` exceeds u32. The common
u32 binaries contain no `Int64` capability; the u64 fallback retains it.

Each workgroup result below is the median of 120 dispatches after 20 warm-ups.
All five variants were run explicitly through the same exact CPU cell-parity
test and passed.

| Pass 1 variant | Workgroup | Total invocations | GPU mapping median |
| --- | ---: | ---: | ---: |
| u64 fallback | 64 | 230,400 | 1.154 ms |
| u32 | **32** | 115,200 | **0.250 ms** |
| u32 | 64 | 230,400 | 0.290 ms |
| u32 | 128 | 460,800 | 0.428 ms |
| u32 | 256 | 921,600 | 0.724 ms |

The selected u32-32 path is 4.62x faster than the like-for-like u64-64 result.
Larger groups add reduction barriers and inactive work; 256 lanes exceed the
144 UV samples per baseline cell. u32-32 is now the default when the host proof
succeeds.

### CPU/GPU mapping crossover

The crossover harness used one fixed 1920x1080 NV12 frame, color mode, 20
warm-ups, 120 measurements, and isolated full-GPU and hybrid Vulkan contexts so
alternating contexts could not distort the result.

| ASCII width | Cells | CPU map | GPU map | Full GPU wall | Hybrid wall | Winner |
| ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 40 | 920 | 0.606 ms | 0.223 ms | 1.905 ms | 1.880 ms | hybrid, 1.3% |
| 80 | 3,600 | 0.647 ms | 0.319 ms | 1.928 ms | 1.806 ms | hybrid, 6.3% |
| 160 | 14,400 | 1.033 ms | 0.559 ms | 2.130 ms | 2.185 ms | full GPU, 2.6% |
| 240 | 32,400 | 0.986 ms | 0.978 ms | 2.504 ms | 2.197 ms | hybrid, 12.2% |
| 320 | 57,600 | 1.788 ms | 1.600 ms | 3.171 ms | 2.861 ms | hybrid, 9.8% |

The winners are not monotonic and most margins are below a robust policy gate.
There is no defensible density crossover heuristic, so `auto` remains GPU
mapping. The CLI retains `--vulkan-mapping cpu` as an explicit diagnostic; it
reuses `asciiflow-cpu::Nv12Mapper` in the composition root and uploads only the
cell buffer. The Vulkan crate has no dependency on the CPU backend.

### One-slot/two-slot experiment and contract

The fair local experiment used the same single logical device and queue, the
same three upload/compute/download submissions per frame, independent per-slot
buffers/descriptors/command buffer/fence/query pool, 20 warm-ups, and 120
frames. The table is the component-wise median of three complete repetitions.

| Metric | 1 slot | 2 slots | Change |
| --- | ---: | ---: | ---: |
| throughput | 523.11 FPS | 795.58 FPS | **+52.1%** |
| latency median | 1.750 ms | 2.248 ms | +28.5% |
| latency p95 | 2.177 ms | 3.129 ms | +43.7% |
| backend wall/latency | 1.741 ms | 2.239 ms | +28.6% |
| CPU fence wait | 1.073 ms | 1.496 ms | +39.4% |
| GPU busy span | 0.990 ms | 1.241 ms | +25.4% |
| requested slot buffers | 12.0 MiB | 24.0 MiB | +12.0 MiB |

The per-frame wait/latency increase is queueing, not a regression hidden as
throughput. The second slot lets host preparation/submission overlap another
slot's device work and clears the 10% production gate by a wide margin. Three
slots were not tested: the requested two-slot experiment already passed, the
pipeline has other stages to overlap, and another full slot would add memory
and latency without a demonstrated need.

The busy span runs from that slot's upload-begin timestamp through its
download-end timestamp across three submissions. On the shared queue it can
contain interleaved commands from the other frame, so it measures elapsed GPU
timeline occupancy, not this frame's exclusive execution cost. The four
individual stage pairs remain attributable to their own frame.

Core now expresses buffering through `submit -> Option<BackendOutput>` and
`drain`; synchronous CPU backends still return immediately. Vulkan retains at
most two pending frames and uses an internal sequence number plus FIFO
completion, not PTS. Each returned timing belongs to that output frame.
Cancellation or a downstream error discards pending outputs, then joins both
workers; a slot/device failure makes the backend terminal. Queue submission and
teardown are externally synchronized.

### Stage 1.3 formal benchmark

The preserved 49-frame Stage 0 workload used one discarded warm-up and three
measured runs per strategy. Entries are component-wise medians. `backend wall`
for the two-slot backend is submit-to-completed-readback latency and overlaps
other frames; only end-to-end FPS is throughput. All non-FPS entries are
ms/frame. GPU upload/mapping/render/download/busy are Vulkan timestamps; the
remaining timing rows are CPU wall clocks.

| Metric | CPU | Vulkan 1 slot | Vulkan 2 slots | Hybrid 1 slot |
| --- | ---: | ---: | ---: | ---: |
| decode | 2.109 | 2.128 | 2.263 | 2.045 |
| CPU mapping | 0.905 | - | - | 0.999 |
| CPU render | 18.173 | - | - | - |
| host upload | - | 0.313 | 0.283 | 0.003 |
| GPU upload | - | 0.148 | 0.110 | 0.012 |
| GPU mapping | - | 0.288 | 0.225 | - |
| GPU render | - | 0.139 | 0.103 | 0.146 |
| GPU download | - | 0.156 | 0.120 | 0.156 |
| GPU busy span | - | 2.003 | 1.599 | 1.306 |
| queue submit | - | 0.080 | 0.089 | 0.082 |
| CPU fence wait | - | 2.478 | 2.057 | 1.852 |
| host invalidate | - | 0.086 | 0.096 | 0.087 |
| host memcpy | - | 0.352 | 0.528 | 0.336 |
| backend wall | 19.159 | 3.597 | 3.826 | 3.670 |
| encode | 2.389 | 1.920 | 2.103 | 1.989 |
| end-to-end | 51.27 FPS | 259.19 FPS | **401.06 FPS** | 254.48 FPS |

Vulkan two-slot improves end-to-end throughput 54.7% over the one-slot control,
while average submit-entry-to-readback latency rises 6.4%; throughput and
latency therefore retain distinct semantics. CPU fence wait falls 17.0%. The
two-slot result is 7.82x CPU end-to-end and 5.01x CPU by backend wall. Hybrid
remains useful only as a diagnostic: its final width-80 throughput was 1.8%
below the one-slot full-GPU control, and the default two-slot full-GPU path is
57.6% faster end-to-end.

A separate 300-frame 1920x1080 run produced 302.48 FPS on Vulkan two-slot and
52.18 FPS on CPU (5.80x). Both produced the exact same MP4 SHA-256
`c6d3f0e95706e88404c630ce78a640144e4e24b3eeee53038e14809f813b7ce7`.
The shorter four-strategy outputs likewise shared SHA-256
`9c19a0704d7c100886aa420384c4f2398d071568a5d9bc0048ac197d055d58ee`;
CPU/Vulkan decoded frame manifests shared
`41393874e551ba84beb1f3a36e77eb764fb1273adc3b909ddf960dabe20e2bf6`.

All 14 current Release SPIR-V artifacts pass `spirv-val --target-env
vulkan1.3`. Five Pass 1 variants, full-GPU/hybrid output, and the production
two-slot alternating-frame test are byte-exact. The validation-enabled real
Intel Arc runs also dropped a backend with both slots pending and produced zero
Vulkan validation, VUID, and synchronization errors, with exact frame count
and order in completed conversions. Submit/drain/encoder-failure paths have
separate bounded-pipeline unit coverage; device-loss injection remains
unavailable and its terminal-state behavior is code-audited rather than a
claimed hardware test.

The 300-frame run makes software decode (3.271 ms/frame) the current throughput
bottleneck; GPU mapping remains the largest individual device stage at only
0.330 ms. Upload plus download copies total 0.359 ms and do not justify
Host/GPU transfer elimination. Stage 1.3 is complete. A later Stage 2 can now
be justified only as a media-pipeline investigation led by decode/encode
evidence, not as more Pass 2 compute or premature zero-copy work; Stage 2 has
not started here.
