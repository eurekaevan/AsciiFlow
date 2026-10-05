# AsciiFlow

AsciiFlow is a Rust CLI that converts video into color or monochrome ASCII
video. It has a portable CPU path and, on qualified Linux hardware, a Vulkan
processing path with VAAPI decode/encode and optional DMA-BUF interop. The
former C# implementation is no longer part of the active project; its
remaining feature gaps are recorded in the [Rust follow-up inventory](docs/legacy-feature-parity.md).

## Current support

Qualification applies to the recorded Intel/Mesa/iHD/FFmpeg stacks, not all
driver versions. [Stack-scoped portability testing](docs/portability-testing.md)
separates media correctness from exact-build artifact identity; alternate
stacks do not automatically broaden this support contract.

<!-- production-support-contract:begin -->
Support contract `1.1.0`. Hardware conditions and evidence: [production support contract](docs/production-support.md).

| Dimension | Value | Support state | Conditions |
|---|---|---|---|
| input_container | mp4 | ConditionallySupported | Sealed retained matrix plus exact Stage 5.4B MP4 tuples (ordinary/fast-start/fragmented) only. AAC payload/timing/default/language are strictly checked where copied. Subtitles and video display titles are explicitly not copied. |
| input_container | matroska | Unqualified | Video-only H.264 sample converts, but AAC/FLAC Matroska-to-MP4 strict oracles fail: unspecified language becomes und, absent default becomes true, millisecond packet duration and missing first duration differ. Not a supported Matroska/audio preservation claim. |
| input_container | mov | Unqualified | H.264/AAC sample converts but the unchanged strict audio oracle rejects undefined language becoming und. No MOV support promotion. |
| output_container | mp4 | Supported |  |
| output_container | matroska | Unsupported | The current CLI rejects non-MP4 output paths. |
| input_color | sdr709 | Supported |  |
| input_color | sdr601 | Unqualified | Legacy 8-bit software normalization is planner-accepted, but this path lacks production qualification. |
| input_color | pq | ConditionallySupported | Canonical limited BT.2020 NCL/PQ and left chroma; HEVC Main10 or AV1 Main 10-bit 4:2:0; VAAPI decode, Vulkan processing, VAAPI encode and matching input/output profile interop. The 0–1000 cd/m2 source pixel ceiling applies only to explicit HDR-to-SDR conversion. |
| input_color | hlg | Unsupported |  |
| input_color | full_pq | Unsupported |  |
| input_color | wide_sdr | Unsupported |  |
| input_color | unknown | Unsupported |  |
| input_color | conflicting | Unsupported |  |
| output_profile | h264-encoder-selected | Supported | H.264 output is qualified at 8-bit SDR; no 10-bit H.264 output. |
| output_profile | hevc-main | ConditionallySupported | 8-bit SDR output requires the scoped VAAPI HEVC Main encode path. |
| output_profile | hevc-main10 | ConditionallySupported | 10-bit SDR conversion or PQ preservation requires the scoped VAAPI P010 path. |
| output_profile | av1-main | ConditionallySupported | AV1 8-bit SDR or 10-bit SDR conversion/PQ preserve requires the matching scoped VAAPI profile and format probe. |
| output_dynamic_range | preserve | ConditionallySupported | SDR preserve is qualified for the five 8/10-bit output profiles; PQ preserve only for HEVC Main10 and AV1 Main 10-bit. The 1000 cd/m2 limit does not apply to PQ preservation. |
| output_dynamic_range | sdr | ConditionallySupported | Explicit PQ-to-SDR conversion is qualified only for canonical input with every source pixel at or below 1000 cd/m2. |
<!-- production-support-contract:end -->

The generated rows classify dimensions, not all combinations. Complete paths
require the contract conditions and runtime probes. Audio copy requires the
existing CFR timeline; failure/cancellation preserves the destination. The
default `preserve` never requests tone mapping. Explicit `sdr` uses the sealed
1000→100-nit C pipeline; source pixels above its decoded domain reject.

Rendering supports the built-in 8×8 font or an explicit monospaced FreeType
font, color or monochrome, and standard/detailed/literal character ramps.
The default ramps run sparse to dense on black, so black remains dark and
white renders brighter. Audio auto copies eligible compressed tracks; copy
is strict and none produces video-only output. Explicit hardware requests
fail rather than silently falling back.

PQ hardware qualification currently covers Intel Arc Meteor Lake with the
specific iHD/ANV/toolchain in the [Stage 5.3B-3 report](docs/stage5.3b3-hdr-production.md),
not universal HDR support. Source mastering-display and MaxCLL/MaxFALL are not
propagated or recomputed because ASCII rendering changes the image; this is
not HDR10 static/mastering qualification.

See the [codec matrix](docs/codecs.md), [color contract](docs/color-semantics.md),
[audio timeline and limitations](docs/audio.md), [font contract](docs/fonts.md),
and [failure semantics](docs/failure-semantics.md) for precise boundaries.

## Build and run

The Rust workspace needs Rust, `clang`/`libclang`, FFmpeg development headers
and libraries, FreeType, and Linux `libva`/`libdrm` headers for the hardware
path. The FFmpeg build must provide `libx264`; VAAPI H.264 also depends on
the driver exposing its H.264 encode profiles. For a pinned FFmpeg source
build, see [`third_party/ffmpeg/`](third_party/ffmpeg/) and the
[native dependency notes](docs/architecture.md).

```bash
cargo build --release --workspace
cargo test --workspace

# Portable conversion; automatic planning chooses a legal local path.
cargo run --release --bin asciiflow -- input.mp4 output.mp4 --width 160

# Inspect detected capabilities and the selected plan without creating output.
cargo run --release --bin asciiflow -- input.mp4 --capabilities
cargo run --release --bin asciiflow -- input.mp4 --explain-plan

# Explicit Intel-style full-GPU path; all requested interop is strict.
cargo run --release --bin asciiflow -- input.mp4 output-gpu.mp4 \
  --decode vaapi --backend vulkan --vulkan-mapping gpu --encode vaapi \
  --hw-device /dev/dri/renderD128 \
  --vaapi-vulkan-input-interop on --vaapi-vulkan-output-interop on

# Explicit 10-bit output, when the input is qualified BT.709 SDR 10-bit.
cargo run --release --bin asciiflow -- input-main10.mp4 output-main10.mp4 \
  --output-codec hevc --output-bit-depth 10 --encode vaapi \
  --hw-device /dev/dri/renderD128

# Qualified BT.2020/PQ preservation; no CPU or software-media HDR fallback.
cargo run --release --bin asciiflow -- input-pq.mp4 output-pq.mp4 \
  --output-codec hevc --output-bit-depth 10 \
  --decode vaapi --backend vulkan --vulkan-mapping gpu --encode vaapi \
  --hw-device /dev/dri/renderD128 \
  --vaapi-vulkan-input-interop on --vaapi-vulkan-output-interop on
```

For qualified PQ content with actual pixels within 0–1000 nit, add
`--output-dynamic-range sdr` to request BT.709 limited output explicitly.
H.2648 is the default output; HEVC/AV1 allow either `--output-bit-depth 8` or `10`.
Omitting this option never converts HDR to SDR automatically.

Use `--output-codec h264|hevc|av1`, `--output-bit-depth 8|10`,
`--backend auto|cpu|vulkan`, `--decode auto|software|vaapi`, and
`--encode auto|software|vaapi` to constrain planning. Other common flags
include `--width`, `--height`, `--charset`, `--font`, `--font-face-index`,
`--color true|false`, `--audio auto|copy|none`, `--max-frames`,
`--no-progress`, and `--verbose`. The input is positional; output is
positional except for capability/plan inspection. Run `asciiflow --help`
for the exact installed CLI.

## Project map and verification

```text
apps/asciiflow-cli/       CLI and composition root
crates/asciiflow-core/    frames, contracts, planning and bounded pipeline
crates/asciiflow-media/   FFmpeg decode/encode and mux ownership
crates/asciiflow-cpu/     permanent CPU pixel reference
crates/asciiflow-font/    R8 glyph atlases
crates/asciiflow-vulkan/  Vulkan processing and resource ownership
crates/asciiflow-interop/ DRM PRIME / DMA-BUF bridge
shaders/src/              build-time GLSL compute sources
tests/                    media, font and regression fixtures
docs/                     contracts, test policy and stage evidence
```

The [architecture](docs/architecture.md) explains ownership and native
boundaries. [Testing](docs/testing.md) distinguishes portable checks from
opt-in Intel `/dev/dri` gates and records the current post-polarity media
baseline. Historical stage reports and baseline revision numbers identify
evidence, not separate application versions. The permanent PQ CPU reference
and internal Vulkan numeric qualification remain documented in
[Stage 5.3B-1](docs/stage5.3b1-pq-cpu-reference.md) and
[Stage 5.3B-2](docs/stage5.3b2-vulkan-pq.md). The
[sealed Stage 5.3B-3 report](docs/stage5.3b3-hdr-production.md) establishes the
qualified BT.2020/PQ HDR-preserving production path and reproducible retained
baseline. The [sealed Stage 5.3C-4B report](docs/stage5.3c4b-hdr-to-sdr-production.md)
establishes explicit, fixed-policy PQ→SDR production; Stage 5.3C overall is sealed.
CPU HDR reference remains non-production. HLG, full-range HDR and generalized
above-1000-nit conversion remain unsupported.

The repository CI runs Rust format, strict Clippy, workspace tests, and Vulkan
1.3 validation on generated SPIR-V. Hardware gates require an Intel host and
are deliberately separate from portable CI.

## License

AsciiFlow's own source is [MIT-licensed](LICENSE). Native libraries, fonts,
and other dependencies retain their own licenses. H.264 software output may
depend on GPL-licensed `libx264`; distribution obligations depend on the
actual FFmpeg build and should be reviewed before packaging.
