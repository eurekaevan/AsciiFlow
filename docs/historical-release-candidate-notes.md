# Historical Linux release candidate notes

Original artifact label: `2.0.0-rc.1`. Retained as historical evidence; current
releases use date tags.

This candidate targets a recorded Linux x86-64 installation and the tested
Intel Arc Meteor Lake stacks. It is not a promise for all distributions, Intel
GPUs, Vulkan drivers or FFmpeg 8.x/9.x. The source snapshot and artifact hashes
in the release receipt identify the candidate; no Git tag/push/publication is
performed by qualification.

## Capabilities

- Color/monochrome ASCII video with builtin8×8 or user-supplied monospaced
  FreeType fonts. CPU/software SDR remains available within the support contract.
- H.264 8-bit SDR output; scoped VAAPI HEVC 8/10 and AV1 8/10 output. No silent
  SDR bit-depth conversion and no 10-bit H.264.
- Qualified VAAPI decode, Vulkan compute and DMA-BUF input/output interop.
- Canonical limited BT.2020 primaries / PQ transfer / BT.2020 NCL 10-bit 4:2:0
  preservation to HEVC Main10 or AV1 Main 10-bit. Default `preserve` never tone-maps.
- Explicit `--output-dynamic-range sdr` requests the fixed PQ→BT.709 SDR path.
  Its qualified decoded source domain is 0–1000 cd/m², not arbitrary mastering
  peaks. Source mastering-display/CLL metadata is not carried into rendered video.
- MP4 output and eligible compressed audio passthrough via auto/copy/none;
  no audio transcoding. Source video must keep its CFR grid when audio is copied.
- Transactional output, cooperative SIGINT cancellation and structured failures.

The [generated support matrix](production-support.md) is authoritative for
conditions and Unsupported/Unqualified cases. Hardware qualification has a
separate exact-stack table in the RC report, not a generic GPU badge.

## Basic invocation

```bash
./asciiflow input.mp4 output.mp4 --width 160 --audio auto

# Canonical PQ preservation on the qualified full-GPU path.
./asciiflow input-pq.mp4 preserve.mp4 --output-codec hevc --output-bit-depth 10 \
  --decode vaapi --backend vulkan --encode vaapi --hw-device /dev/dri/renderD128 \
  --vaapi-vulkan-input-interop on --vaapi-vulkan-output-interop on --audio none

# The same qualified path, explicitly converting legal-domain PQ to SDR.
./asciiflow input-pq.mp4 sdr.mp4 --output-codec hevc --output-bit-depth 10 \
  --output-dynamic-range sdr --decode vaapi --backend vulkan --encode vaapi \
  --hw-device /dev/dri/renderD128 --vaapi-vulkan-input-interop on \
  --vaapi-vulkan-output-interop on --audio copy
```

Use `--help` for all options. `auto` copies eligible compressed audio tracks,
`copy` rejects incompatible tracks, and `none` omits audio; none transcodes it.
Missing requested GPU capabilities are fatal, not silent fallbacks. Install the
recorded native libraries/drivers and check render-node access. An explicit
font path must exist and provide supported monospaced glyphs. Variable-rate or
discontinuous video with audio copy rejects; `--audio none` permits video-only
CFR output rather than preserving the original audio timeline. Failed/cancelled
jobs leave the previous target intact; success atomically replaces it.

## Installation and runtime

Extract the Linux archive and run its `asciiflow` executable or copy it onto
your PATH. No shader/font asset directory or developer checkout is needed:
project GLSL-derived SPIR-V and the MIT font8x8 builtin atlas are embedded.
Explicit font files are supplied by the user, not bundled fixture fonts.
System native libraries and GPU drivers must match a supported/qualified
environment; the archive does not install them or change driver selection.
Retain LICENSE, third-party notices and the source/distribution instructions.
See the RC receipt for exact ELF dependencies, runtime loader/driver identities
and binary SHA. Build from source with `cargo build --release --locked --workspace`
using the recorded native headers and toolchain. No offline-build or
bit-reproducible-ELF claim is made.

## Qualification scope and limitations

Reliability is qualified for the recorded **single-job CLI workloads and
long-run production paths**. Three representative 100k-frame jobs, lifecycle,
queue, failure/cancellation, retained regression and alternate-stack evidence
remain sealed. MP4 sample indexes and bounded audio-enabled allocator retention
are permitted working-set behavior; constant RSS is not promised.

Persistent multi-job memory remains unqualified. HLG, full-range HDR,
generalized >1000-nit conversion, unqualified container/audio combinations,
new operating systems and untested driver stacks are not promoted by this RC.
Historical failures stay in engineering evidence, not erased by release wording.
No real-time throughput, 24/7 operation or complete driver-internal resource
observability guarantee is made. Cached host-input shader FPS is not a
production performance claim.

Use `--version`, OS/kernel, FFmpeg/Vulkan/VAAPI versions and a sanitized
`--diagnostic-report` when reporting failures. No telemetry is added.
