# AsciiFlow

AsciiFlow is a Rust CLI that converts video into color or monochrome ASCII
video. It has a portable CPU path and, on qualified Linux hardware, a Vulkan
processing path with VAAPI decode/encode and optional DMA-BUF interop. The
former C# implementation is no longer part of the active project; its
remaining feature gaps are recorded in the [Rust follow-up inventory](docs/legacy-feature-parity.md).

## Current support

- Input: H.264, HEVC Main and AV1 Main 8-bit 4:2:0, plus explicitly tagged
  BT.709 limited-range SDR HEVC Main10 and AV1 Main 10-bit. Qualified full
  hardware pipelines also admit BT.2020/PQ/NCL limited-range, left-sited
  HEVC Main10 and AV1 Main 10-bit for HDR-preserving ASCII processing. FFmpeg may demux
  other containers, but that does not imply every codec or color mode is
  qualified.
- Output: MP4 with H.264 8-bit by default. HEVC Main and AV1 Main/Profile0
  8-bit, or explicit HEVC Main10 and AV1 Main 10-bit, require qualified VAAPI
  hardware. The 10-bit path preserves P010 precision; it does not silently
  convert to 8-bit.
- Processing: built-in 8×8 or explicit scalable monospaced FreeType font;
  `standard`, `detailed`, or literal character ramp; color or monochrome
  rendering. On a black background the default ramps run sparse to dense, so
  black stays dark and white renders brighter.
- Audio: compatible compressed streams are copied into MP4 with `--audio
  auto`; `--audio copy` is strict and `--audio none` makes video-only output.
  Audio copy currently requires the existing CFR video timeline.
- Safety: explicit hardware requests fail instead of silently falling back.
  Output is staged in the destination directory and committed only after a
  successful encode/mux; failure or cancellation preserves an existing file.
  PQ production requires explicit HEVC/AV1 ten-bit output and qualified VAAPI
  decode/encode plus both P010 Vulkan interop paths, with no HDR fallback.
  HLG, BT.2020 SDR, full-range and unresolved color semantics are rejected
  before output staging. No HDR→SDR tone mapping or gamut mapping is provided.

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
baseline. CPU HDR reference remains non-production. No HLG, full-range HDR
or HDR→SDR tone mapping is supported.

The repository CI runs Rust format, strict Clippy, workspace tests, and Vulkan
1.3 validation on generated SPIR-V. Hardware gates require an Intel host and
are deliberately separate from portable CI.

## License

AsciiFlow's own source is [MIT-licensed](LICENSE). Native libraries, fonts,
and other dependencies retain their own licenses. H.264 software output may
depend on GPL-licensed `libx264`; distribution obligations depend on the
actual FFmpeg build and should be reviewed before packaging.
