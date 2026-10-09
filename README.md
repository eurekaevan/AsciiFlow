# AsciiFlow

AsciiFlow **2.0.0** is the final qualified v2 engineering milestone. Stage 5.4
is SEALED; the [final milestone receipt](docs/asciiflow-2.0.0-final.md) records
the artifact identity, inherited qualification and unchanged limitations.

Current source development is **2.1.0-dev**, Stage 6A (system font discovery
and terminal UX). It does not replace or modify the frozen 2.0.0 artifact.

AsciiFlow is a Rust CLI that converts video into color or monochrome ASCII
video. It has a portable CPU path and, on qualified Linux hardware, a Vulkan
processing path with VAAPI decode/encode and optional DMA-BUF interop. The
former C# implementation is no longer part of the active project; its
remaining feature gaps are recorded in the [Rust follow-up inventory](docs/legacy-feature-parity.md).

The official LGPL prebuilt release does not support software H.264 encoding.
Software decode and qualified VAAPI H.264/HEVC/AV1 encoding remain available
subject to runtime probes. Source/developer builds retain the libx264 backend;
availability depends on their FFmpeg build capabilities. GPL-capable custom
builds are outside this LGPL release qualification.

## Current support

Qualification applies to the recorded Intel/Mesa/iHD/FFmpeg stacks, not all
driver versions. [Stack-scoped portability testing](docs/portability-testing.md)
separates media correctness from exact-build artifact identity; alternate
stacks do not automatically broaden this support contract.

Stage 5.4C2's expanded portability matrix and its H.264 cross-driver closure
are tracked in the [forensic closure report](docs/stage5.4c2a-h264-ihd2546.md).
Its five stack definitions and qualification boundaries are tracked in
the [matrix report](docs/stage5.4c2-expanded-portability-matrix.md) and the
[qualified-stack registry](tests/portability/qualified-stacks.json); neither
changes this support contract without reviewed qualification. The additive
Tier 1B-P gate permits only an attested iHD build identifier in one known SEI;
it does not relax same-stack packet identity or admit SPS/PPS/VCL/color/timing
changes. No universal driver portability is implied. [Stage 5.4D-1 reliability
hardening](docs/stage5.4d1-soak-failure-hardening.md) is SEALED for the recorded
single-job CLI workloads. Ordinary MP4 sample indexes and bounded allocator
retention can grow its working set; constant memory is not promised.

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

Release profile `lgpl-prebuilt`: Official LGPL prebuilt Linux release; opt-in distribution restrictions, not a replacement for the base support contract.
- Software H.264 encoding requires GPL dependency libx264 and is excluded from the official LGPL prebuilt release.
<!-- production-support-contract:end -->

The generated rows classify dimensions, not all combinations. Complete paths
require the contract conditions and runtime probes. Audio copy requires the
existing CFR timeline; failure/cancellation preserves the destination. The
default `preserve` never requests tone mapping. Explicit `sdr` uses the sealed
1000→100-nit C pipeline; source pixels above its decoded domain reject.

Rendering supports the built-in 8×8 font, an explicit monospaced FreeType file,
or a Linux Fontconfig family/pattern such as `--font "Liberation Mono"` or
`--font monospace`. Fontconfig selects one file/collection face; existing
FreeType validation and rasterization remain authoritative. A system match
that fails those checks rejects; there is no automatic font/glyph fallback.
`--font-face-index` applies only to explicit files. Color/monochrome rendering
and standard/detailed/literal character ramps remain unchanged.
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
path. Software H.264 encoding in a developer build requires `libx264`; the
official `lgpl-prebuilt` profile excludes that route. VAAPI H.264 depends on
the driver exposing its H.264 encode profiles. For a pinned FFmpeg source
build, see [`third_party/ffmpeg/`](third_party/ffmpeg/) and the
[native dependency notes](docs/architecture.md).

```bash
cargo build --release --locked --workspace
cargo test --workspace

# Automatic planning chooses a legal available path; LGPL prebuilt requires VAAPI encoding.
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

Stage 6A source builds accept `--font "Liberation Mono:style=Bold"` as well as
explicit files and `builtin-8x8`. A system match must still pass the existing
FreeType monospaced/scalable/glyph checks; discovery does not promise fallback.

Normal runs write a short input/plan/font/audio/output overview to stderr and
two English completion lines to stdout. Interactive stderr terminals show one
monochrome progress line; redirected output, `TERM=dumb`, `--no-progress`, and
`--verbose` never show a live bar. `--no-progress` retains startup/completion
information. `--verbose` retains grouped diagnostic metrics instead. Progress
uses accepted encoded frames, not a termination condition; inputs without a
trusted frame count show elapsed progress without a percentage or invented ETA.
Capability/plan inspection does not start this conversion UI. See the
[Stage 6A qualification report](docs/stage6a-font-terminal-ux.md).

## Installing the qualified Linux release

The instructions below concern frozen **2.0.0**, not a newly qualified package
of the Stage 6A development source. New font-name discovery needs runtime
Fontconfig on Linux; builtin/explicit-file selection does not need it.

The qualified 2.0.0 milestone is a dynamically linked Linux x86-64 package,
not a universal Linux bundle. Extract the archive and add its directory to
your `PATH` (or invoke `asciiflow` by absolute path). Keep the executable next
to its bundled `lib/` directory; moving only the executable loses the qualified
LGPL FFmpeg dependency set. Keep the accompanying licenses, notices and
`sources/` when redistributing the package. Shaders and the builtin font are
embedded; no source checkout, shader cache or fixture directory is needed at
runtime. FreeType, libdav1d, Vulkan loader and VAAPI/driver libraries remain
system prerequisites. Use the recorded release dependency and
hardware receipts rather than assuming another FFmpeg major or driver is
compatible. The tested build toolchain is Rust1.97.1; the manifest's1.88 floor
reflects language features and is not a separately qualified minimum-toolchain
claim. Offline builds are not a release promise.

```bash
asciiflow --version
asciiflow input.mp4 output.mp4 --width 160 --audio auto
asciiflow input.mp4 --capabilities
```

See the [final milestone and installation receipt](docs/asciiflow-2.0.0-final.md), the
[generated scenario matrix](docs/production-support.md#complete-scenario-inventory)
and the [inherited LGPL qualification report](docs/stage5.4d2-lgpl-release-qualification.md).
AsciiFlow source is MIT; the executable's distribution also has obligations
from its actual native FFmpeg configuration. Do not describe a GPL-enabled
FFmpeg-linked executable as an MIT-only or LGPL-only distribution.

## Known limitations and diagnosing failures

- Production is one process, one media job, then exit. Persistent multi-job
  process memory is not qualified; there is no daemon/24/7 guarantee.
- HDR means canonical limited BT.2020 / PQ / BT.2020 NCL P010 on the recorded
  GPU path, not HLG, full-range HDR or arbitrary colorimetry. Explicit PQ→SDR
  additionally requires every decoded source pixel to be at most1000 cd/m².
- Audio is compressed passthrough, not transcoding. Copy needs the original
  CFR video grid and a qualified MP4/audio tuple; VFR/discontinuous copy rejects.
  MOV/Matroska acceptance is not a preservation qualification.
- Hardware qualification is limited to the recorded stacks. Absence from the
  table does not prove incompatibility; historical FailedQualification remains
  in engineering records.
- Missing GPU/driver capabilities fail explicit hardware requests. Inspect
  `--capabilities`/`--explain-plan`; use software only where the support contract
  permits it. A nonexistent font path fails; `builtin-8x8` needs no external font.
- Failure or Ctrl+C before commit cleans staging and preserves an existing
  target. Success atomically replaces an existing target. Diagnostics written
  after commit cannot roll the successful output back.

For a bug report include `asciiflow --version`, OS/kernel, the full invocation,
stderr and an optional `--diagnostic-report report.json` (new path), plus FFmpeg,
Vulkan and VAAPI versions. Remove private paths/media details before sharing.
There is no telemetry or automatic upload.

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

### Local artifacts and cleanup

Source belongs in `apps/`, `crates/` and `shaders/`; regression fixtures and
durable qualification evidence belong in `tests/`, with their contracts in
`docs/`. `third_party/ffmpeg/scripts/` contains the release dependency recipe.

`target/` also holds locally retained release packages, exact source snapshots,
raw qualification measurements and inputs—not just disposable Cargo output.
**Do not remove the whole directory or run an unrestricted `cargo clean` here.**
Final 2.0.0 artifacts are under `target/releases/asciiflow-2.0.0/`; historical
rc.1/rc.2 artifacts remain in their original release directories. Archive these
with their receipts before moving them off this machine.

When no build/test is running, Cargo `incremental/`, `deps/` and `.fingerprint/`
directories under build profiles and Python `__pycache__/` are regenerable.
Removing them makes subsequent builds slower, but does not invalidate retained
packages or evidence. Keep build-script outputs/generated shaders, standalone
binaries, native dependency sysroots, logs, fixture generators and checksums.
User media such as root-level `input.mp4`, `output.mp4` and `output/` is not
automatically treated as disposable.

## License

AsciiFlow's own source is [MIT-licensed](LICENSE). Native libraries, fonts,
and other dependencies retain their own licenses. H.264 software output may
depend on GPL-licensed `libx264`; distribution obligations depend on the
actual FFmpeg build and should be reviewed before packaging.
