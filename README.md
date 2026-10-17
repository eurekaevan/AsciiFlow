# AsciiFlow

Releases use calendar-date tags (`YYYY-MM-DD`). Maintenance follows changes and
regressions; current usage and support limits are documented below.

AsciiFlow is a Rust application with command-line and graphical interfaces that
converts video into color or monochrome ASCII video. It has a portable CPU path and, on qualified Linux hardware, a Vulkan
processing path with VAAPI decode/encode and optional DMA-BUF interop. Windows
x86_64 now implements software decode and hardware-only Media Foundation H.264
8-bit SDR encoding, with CPU or Vulkan host processing. Actual Windows
build and hardware conversion qualification remain pending. The
former C# implementation is no longer part of the active project; its
remaining feature gaps are recorded in the [Rust follow-up inventory](docs/legacy-feature-parity.md).

The official LGPL prebuilt release does not support software H.264 encoding.
Linux software decode and qualified VAAPI H.264/HEVC/AV1 encoding remain available
subject to runtime probes. Both release profiles include VP8/VP9/MPEG-4 Part 2 software decoding and
MKV/AVI/WebM input alongside MP4. The Windows profile includes H.264/HEVC
software decode, compressed audio passthrough and MF H.264 encoding; missing hardware fails without
a software encoder fallback. Source/developer builds retain the libx264 backend;
availability depends on their FFmpeg build capabilities. GPL-capable custom
builds are outside this LGPL release qualification.

## Graphical interface

Run `asciiflow-gui` (`asciiflow-gui.exe` on Windows) from the extracted release
directory. The English-language interface converts one video at a time, with
file selection, appearance controls, a first-frame preview and cancellation.
See the [GUI user guide](docs/gui.md) for the complete workflow, control
reference, desktop requirements and troubleshooting. The guide is also included
as `gui.md` in release archives.

## Input formats

The CLI and GUI accept local MP4, MKV, AVI and WebM files; output remains MP4.
The GUI file selector and drag-and-drop use the same media pipeline.
New regression coverage includes H.264/AAC in MKV, MPEG-4 Part 2/MP3 in AVI,
and VP8/Vorbis or VP9/Opus in WebM, using software decoding of 8-bit SDR 4:2:0
video. A container extension does not guarantee support for every codec,
pixel format or color signal inside it. MJPEG, 10-bit VP9 and new hardware/HDR
paths are outside this extension.

Audio is copied as compressed packets without transcoding. **Auto** copies
streams accepted by the actual MP4 muxer and reports skipped streams in the
CLI and GUI; **Copy** rejects incompatible audio; **None** produces video only.
Vorbis/Opus in MP4 may have more limited player compatibility than AAC.
Cross-container metadata, packet duration and timestamp precision can change;
see the [fixture and timing contract](tests/fixtures/containers/README.md).
The existing video timeline rules still apply. These software regression tests
do not qualify a Windows encoder device or a new Linux hardware route.

## Current support

Qualification applies to the recorded Intel/Mesa/iHD/FFmpeg stacks, not all
driver versions. [Stack-scoped portability testing](docs/portability-testing.md)
separates media correctness from exact-build artifact identity; alternate
stacks do not automatically broaden this support contract.

The [qualified-stack registry](tests/portability/qualified-stacks.json) records
tested stack definitions and qualification boundaries; it does not broaden
this support contract without reviewed qualification. The additive
Tier 1B-P gate permits only an attested iHD build identifier in one known SEI;
it does not relax same-stack packet identity or admit SPS/PPS/VCL/color/timing
changes. No universal driver portability is implied. [Reliability testing](docs/reliability-testing.md)
covers the recorded single-job CLI workloads. Ordinary MP4 sample indexes and bounded allocator
retention can grow its working set; constant memory is not promised.

<!-- production-support-contract:begin -->
Support contract `1.1.0`. Hardware conditions and evidence: [production support contract](docs/production-support.md).

The retained support tables and hardware evidence below describe the qualified Linux paths. They do not qualify the new Windows Media Foundation implementation.

| Dimension | Value | Support state | Conditions |
|---|---|---|---|
| input_container | mp4 | ConditionallySupported | Retained matrix plus exact reviewed MP4 tuples (ordinary/fast-start/fragmented) only. AAC payload/timing/default/language are strictly checked where copied. Subtitles and video display titles are explicitly not copied. |
| input_container | matroska | ConditionallySupported | Software 8-bit SDR H.264/AAC MKV and VP8/Vorbis or VP9/Opus WebM regression tuples to MP4. Compressed audio payload/order preserved; packet timestamps allow input plus output container tick quantization. Language/default/duration identity across containers is not promised; historical strict preservation failures remain. |
| input_container | avi | ConditionallySupported | Software 8-bit SDR MPEG-4 Part 2/MP3 regression tuple to MP4. AVI encoder-delay timestamp gap is retained as input evidence; output uses the existing CFR grid. Audio payload/order and quantized packet timing checked, not language/default identity. |
| input_container | mov | Unqualified | H.264/AAC sample converts but the unchanged strict audio oracle rejects undefined language becoming und. No MOV support promotion. |
| output_container | mp4 | Supported |  |
| output_container | matroska | Unsupported | The current CLI rejects non-MP4 output paths. |
| input_color | sdr709 | Supported |  |
| input_color | sdr601 | ConditionallySupported | Software normalization of the exact 8-bit VP8/WebM regression tuple to canonical BT.709 NV12. No blanket BT.601 codec, range or hardware qualification. |
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

Rendering supports the built-in 8×8 font, an explicit scalable FreeType file,
or a Linux Fontconfig family/pattern such as `--font "Liberation Mono"` or
`--font monospace`. Fontconfig selects one file/collection face; existing
FreeType validation and rasterization remain authoritative. A system match
that fails those checks rejects; there is no automatic font/glyph fallback.
`--font-face-index` applies only to explicit files. Color/monochrome rendering
and standard/detailed/literal character ramps remain unchanged.
When `--height` is omitted, scalable fonts use their natural cell width/height
ratio to choose automatic rows. Builtin keeps its square-cell layout; an
explicit `--height` preserves the requested grid. Glyphs are not stretched or
cropped to simulate tighter spacing. Proportional fonts use the same fixed grid,
size and baseline, with each glyph centered horizontally. Automatic cell aspect
uses the ASCII M advance (global maximum if absent) and ascender minus descender,
including for proportional fonts. `--verbose` reports the ratio and geometry.
The default ramps run sparse to dense on black, so black remains dark and
white renders brighter. Audio auto copies eligible compressed tracks; copy
is strict and none produces video-only output. Explicit hardware requests
fail rather than silently falling back.

PQ hardware qualification currently covers Intel Arc Meteor Lake with the
specific iHD/ANV/toolchain in the [support contract](docs/production-support.md),
not universal HDR support. Source mastering-display and MaxCLL/MaxFALL are not
propagated or recomputed because ASCII rendering changes the image; this is
not HDR10 static/mastering qualification.

See the [codec matrix](docs/codecs.md), [color contract](docs/color-semantics.md),
[audio timeline and limitations](docs/audio.md), [font contract](docs/fonts.md),
and [failure semantics](docs/failure-semantics.md) for precise boundaries.

## Build and run

Linux development builds need Rust, `clang`/`libclang`, FFmpeg development headers
and libraries, FreeType, and Linux `libva`/`libdrm` headers for the hardware
path. Software H.264 encoding in a developer build requires `libx264`; the
official `lgpl-prebuilt` profile excludes that route. VAAPI H.264 depends on
the driver exposing its H.264 encode profiles. For a pinned FFmpeg source
build, see [`third_party/ffmpeg/`](third_party/ffmpeg/) and the
[native dependency notes](docs/architecture.md).

```bash
cargo build --release --locked --workspace
cargo test --workspace

# Build and open the graphical application in a desktop session.
cargo run --release --locked -p asciiflow-cli --features gui --bin asciiflow-gui

# Automatic planning chooses a legal available path; Linux LGPL prebuilt requires VAAPI encoding.
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
`--encode auto|software|vaapi|mf` to constrain planning. `mf` requests Windows
hardware encoding; Windows auto prefers eligible Vulkan devices, then CPU. Other common flags
include `--width`, `--height`, `--charset`, `--font`, `--font-face-index`,
`--color true|false`, `--audio auto|copy|none`, `--max-frames`,
`--no-progress`, and `--verbose`. The input is positional; output is
positional except for capability/plan inspection. Run `asciiflow --help`
for the exact installed CLI.

Input must be a local regular file; symbolic links to regular files are accepted.
Directories, FIFOs and device files are rejected before native probing, including
with `--capabilities` and `--explain-plan`. Streaming input is not supported.

Current source builds accept `--font "Liberation Mono:style=Bold"` as well as
explicit files and `builtin-8x8`. A system match must still pass the existing
FreeType scalable/glyph checks; discovery does not promise fallback.
Windows uses `builtin-8x8` or explicit font files; family-name discovery is
Linux-only. Windows MSVC source builds and packaging prerequisites are recorded
in [release packaging](docs/releases.md#reproduce-and-inspect).

Normal runs write a short input/plan/font/audio/output overview to stderr and
two English completion lines to stdout. Interactive stderr terminals show one
monochrome progress line; redirected output, `TERM=dumb`, `--no-progress`, and
`--verbose` never show a live bar. `--no-progress` retains startup/completion
information. `--verbose` retains grouped diagnostic metrics instead. Progress
uses accepted encoded frames, not a termination condition; inputs without a
trusted frame count show elapsed progress without a percentage or invented ETA.
Capability/plan inspection does not start this conversion UI. The progress line
is cleared on success, error and cancellation; native warnings remain visible.

## Installing release packages

For Linux x86_64 and Windows x86_64 packages, see the [Release LGPL workflow
and packaging instructions](docs/releases.md). Each package includes the CLI
and `asciiflow-gui`. Date tags and manual runs build
both platforms from the same commit; publishing waits for both platform gates.
Manual runs use the date in Asia/Taipei. The Release contains a Linux `.tar.gz`,
a Windows `.zip` and their combined `SHA256SUMS`.

The Linux package is dynamically linked. Extract the
archive and add its directory to your `PATH` (or invoke `asciiflow` by absolute
path). Keep the executable next to its bundled `lib/` directory and retain the
licenses, notices and `sources/` when redistributing it. Shaders and the builtin
font are embedded; no source checkout, shader cache or fixtures are needed at
runtime. FreeType, libdav1d, Vulkan and VAAPI/driver libraries remain system
prerequisites. Font-name discovery also needs runtime Fontconfig on Linux;
builtin and explicit-file selection do not.

The Windows package keeps `asciiflow.exe` and `asciiflow-gui.exe` beside four
replaceable FFmpeg DLLs.
It needs Windows 10/11 Media Foundation, a compatible H.264 hardware encoder
and driver, and the Microsoft Visual C++ 2015–2022 x64 Redistributable. N editions
need the Media Feature Pack. FreeType/libpng/zlib are embedded with their source
and notices; MSYS2 and MinGW runtimes are not required by the package. The first
Windows profile has no AV1 decoder, HEVC/AV1 encoder, hardware decoding,
10-bit/HDR output or VAAPI interop. Its packaging and real hardware conversion
have not been verified on the current Linux development host.

```powershell
./asciiflow.exe input.mp4 output.mp4 --backend cpu --decode software --encode mf --width 160
./asciiflow.exe input.mp4 --backend cpu --decode software --encode mf --explain-plan
```

Package build and runtime requirements are recorded in [release packaging](docs/releases.md).
Qualification applies to the exact tested artifact and dependency stack; it
does not automatically qualify a new package or another driver. Offline builds
are not a release promise.

```bash
asciiflow --version
asciiflow input.mp4 output.mp4 --width 160 --audio auto
asciiflow input.mp4 --capabilities
```

See the [generated scenario matrix](docs/production-support.md#complete-scenario-inventory)
and the [distribution requirements](THIRD_PARTY_NOTICES.md).
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
  CFR video grid and MP4-compatible audio; VFR/discontinuous copy rejects.
  Container input tests do not promise identical language/default/duration metadata.
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
apps/asciiflow-cli/       CLI, egui interface and shared composition root
crates/asciiflow-core/    frames, contracts, planning and bounded pipeline
crates/asciiflow-media/   FFmpeg decode/encode and mux ownership
crates/asciiflow-cpu/     permanent CPU pixel reference
crates/asciiflow-font/    R8 glyph atlases
crates/asciiflow-vulkan/  Vulkan processing and resource ownership
crates/asciiflow-interop/ DRM PRIME / DMA-BUF bridge
shaders/src/              build-time GLSL compute sources
tests/                    media, font and regression fixtures
docs/                     usage, contracts and verification policy
```

The [architecture](docs/architecture.md) explains ownership and native
boundaries. [Testing](docs/testing.md) distinguishes portable checks from
opt-in Intel `/dev/dri` gates and records the current post-polarity media
baseline. Baseline revisions identify evidence, not separate application
release identities. The permanent [PQ reference](docs/hdr-pq-semantics.md)
and [tone-mapping contract](docs/tone-mapping.md) describe the numeric policies;
[production validation](docs/hdr-to-sdr-validation.md) retains the tested
fixed-policy PQ→SDR path and regression identities.
CPU HDR reference remains non-production. HLG, full-range HDR and generalized
above-1000-nit conversion remain unsupported.

The repository CI runs Rust format, strict Clippy, workspace tests, and Vulkan
1.3 validation on generated SPIR-V. Hardware gates require an Intel host and
are deliberately separate from portable CI.
The Windows release gate checks DLL/license profiles, relocated startup and MF
planning failures without assuming a hosted-runner GPU. The opt-in real Windows
MF round-trip gate is documented in [release verification](docs/releases.md#windows-hardware-gate);
it has not yet provided retained hardware qualification.

### Local artifacts and cleanup

Source belongs in `apps/`, `crates/` and `shaders/`; regression fixtures and
durable qualification evidence belong in `tests/`, with their contracts in
`docs/`. `third_party/ffmpeg/scripts/` contains the release dependency recipe.

`target/` also holds locally retained release packages, exact source snapshots,
raw qualification measurements and inputs—not just disposable Cargo output.
**Do not remove the whole directory or run an unrestricted `cargo clean` here.**
The historical release artifacts are under `target/releases/asciiflow-2.0.0/`;
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
