# Regression testing

The default workspace suite covers portable planning, codec policy, failure
semantics, media fixtures, fonts, and CPU processing. Real Intel VAAPI/Vulkan
tests are opt-in because they require a qualified device, drivers, and sometimes
a retained long-form input. Stage-specific measurements and hardware outcomes
live in their validation reports.

Stage 5.3A color tests use the checked-in codec fixtures and run in the normal
workspace suite. They exercise BT.709 SDR, BT.2020 SDR, PQ, HLG, unknown and
contradictory signaling, exact-rational static HDR data, safe-output rejection,
and a synthetic mid-stream SDR→PQ change. On an Intel render node, additionally
run the ignored software/VAAPI color parity check:

```bash
cargo test -p asciiflow-media --test codec_decode \
  hevc_av1_main10_software_and_vaapi_color_semantics_agree -- --ignored --nocapture
cargo test -p asciiflow-media intel_color_source_provenance \
  -- --ignored --nocapture
cargo test -p asciiflow-media --test codec_decode \
  vaapi_rejects_unsupported_color_with_classification_intact \
  -- --ignored --nocapture
```

The [Stage 5.3A hardware closure report](stage5.3a-hardware-closure.md)
records the Intel results. The three retained 8-bit hashes match. The original
Stage 5.2C-3 10-bit input/hash were not retained; those two old output hashes
are historical references, **not current regression gates**. The replacement
canonical 10-bit baseline v1 is checked in under
`tests/fixtures/codecs/hevc-main10-canonical-v1.mp4`, SHA-256
`df69c98ca6592b08c6bc34127b2bb5cf4125c759fd852b830e5426d5818fccda`.
Regenerate with the exact FFmpeg 8.1.3 command in
`tests/fixtures/codecs/generate-10bit-baseline.sh`; the fixture README records
toolchain and three-run byte reproducibility. (Use the authoritative checksum
in `tests/fixtures/codecs/SHA256SUMS` when validating the file.)

For Intel Arc Meteor Lake, build Release and run the two canonical production
profiles below. The output filename may differ, but all other flags are part
of the regression configuration. Repeat each profile three times and compare
SHA-256; the Stage 5.3A values are HEVC Main10
`07f231ab49eebd54cc0e85012f7dd12e424f580d17005bc20028b458168323fe`
and AV1 10-bit
`2ab82984f690ea3fe5472c874dfce7cc474f7c46266f5cd86ed8deee20b36fda`.

```bash
cargo build --release --workspace
target/release/asciiflow tests/fixtures/codecs/hevc-main10-canonical-v1.mp4 \
  /tmp/asciiflow-canonical-hevc-run1.mp4 \
  --width 80 --font builtin-8x8 --color true --audio none --max-frames 300 \
  --decode vaapi --backend vulkan --vulkan-mapping gpu --encode vaapi \
  --hw-device /dev/dri/renderD128 --vaapi-vulkan-input-interop on \
  --vaapi-vulkan-output-interop on --output-codec hevc \
  --output-bit-depth 10 --no-progress
target/release/asciiflow tests/fixtures/codecs/hevc-main10-canonical-v1.mp4 \
  /tmp/asciiflow-canonical-av1-run1.mp4 \
  --width 80 --font builtin-8x8 --color true --audio none --max-frames 300 \
  --decode vaapi --backend vulkan --vulkan-mapping gpu --encode vaapi \
  --hw-device /dev/dri/renderD128 --vaapi-vulkan-input-interop on \
  --vaapi-vulkan-output-interop on --output-codec av1 \
  --output-bit-depth 10 --no-progress
```

The opt-in
`canonical_v1_main10_full_interop_300_frame_preencode_parity` hardware test
compares all 300 pre-encode P010 frames, byte-for-byte, with a staged reference
for both codecs. It also checks FD return and Vulkan Validation when enabled.
For every future retained benchmark or output regression, preserve the input
generator, generator/tool version and exact command, input SHA-256, full output
command, and output SHA-256 (or a structured packet/timestamp/decoded-pixel
oracle if hardware output is nondeterministic). Do not promote a hash whose
input identity cannot be independently reproduced.

Audio Stage 4.2.1 has three layers:

- A: core policy tests and native packet/queue fault tests, without GPU access.
- B: `cargo test -p asciiflow-cli --test audio_regression`, using small checked-in
  fixtures, explicit software video processing, and native libav inspection.
- C: optional Intel audio parity, validation, and cancellation process test;
  not required by ordinary workspace tests.

Run `cargo test --workspace` for the default suite. No ffmpeg/ffprobe executable or
render node is required at test runtime; the usual linked FFmpeg development/runtime
libraries and software H.264 encoder are required. The helper checks structured
stream fields and exact compressed packet bytes, not human-readable probe output.
It decodes output video and AAC using native libraries as a test oracle only.

Run the 60-second completion stress explicitly:
`cargo test -p asciiflow-cli --test audio_regression long_audio_stream -- --ignored`.
The deterministic bounded-channel unit test stalls its consumer, observes capacity,
and cancels the blocked producer. This proves bounded queue storage, not an RSS
measurement. Child-process tests have a 30-second watchdog and RAII termination.
Linux SIGINT waits for actual staging writes before signalling, then checks status
130 and unchanged destination bytes. File-size limits test real buffered output
failure (which FFmpeg may surface at trailer time). A test-only mux message injects
an audio packet failure after two packets to verify the original MuxRuntime cause.
No production environment-variable fault switch exists.

Fixture generation and checksums are documented in
`tests/fixtures/media/README.md`. Existing Stage 4.1 tiny media fixtures are also
included so a fresh checkout can run its native regression suite.

On an Intel host run
`cargo test -p asciiflow-cli --test audio_regression intel_audio -- --ignored`.
This requires explicit full interop (no auto fallback), enables
`ASCIIFLOW_VULKAN_VALIDATION=1`, compares audio against software, and cancels the
long run. The Stage 4.2.1 report did not include a hardware rerun; that is a
historical stage-specific statement, not a claim about the current host.
Subsequent hardware validation is recorded in
[Stage 5.0](stage5.0-codec-validation.md),
[Stage 5.1A](stage5.1a-hevc-encode-validation.md), and
[Stage 5.1B](stage5.1b-av1-encode-validation.md).

On the qualified Intel host, the AV1 output opt-in tests cover a 30-frame
pre-encode staged/full pixel comparison and 3000-frame surface-reuse/FD stress.
Provide the actual media paths; the tests' default `target/` artifacts are not
checked into the repository:

```bash
ASCIIFLOW_STAGE51B_INPUT=/absolute/path/to/300-frame-input.mp4 \
ASCIIFLOW_VULKAN_VALIDATION=1 \
cargo test -p asciiflow-interop --test hardware \
  av1_encoder_descriptor_and_staged_interop_pixels_are_exact -- --ignored

ASCIIFLOW_STAGE51B_STRESS_INPUT=/absolute/path/to/3000-frame-input.mp4 \
ASCIIFLOW_VULKAN_VALIDATION=1 \
cargo test -p asciiflow-interop --test hardware \
  av1_full_interop_surface_reuse_is_exact_and_fd_bounded -- --ignored
```

Neither a passing portable suite nor lavapipe emulation substitutes for Intel
hardware qualification. Do not run all ignored hardware tests indiscriminately:
some deliberately submit invalid external handles and must run without
Validation, as their test annotations state.

## Internal P010LE processing (Stage 5.2A)

Core and CPU P010LE tests run in `cargo test --workspace`. The opt-in Vulkan
checks use synthetic Host P010LE frames, not a Main10 media decoder. Lavapipe
can verify pixel parity and Vulkan Validation when it provides the required
16-bit storage feature:

Stage 5.2B software HEVC Main10 and AV1 10-bit fixtures are also exercised by
the normal workspace suite. The media integration test verifies 36-frame
decode, P010 padding and real low-bit samples; the interop crate's
`p010_qualification` test runs software decode→CPU ASCII without an encoder.
Run real-media CPU/Vulkan byte parity on a Vulkan 1.3 device explicitly:

```bash
ASCIIFLOW_VULKAN_ALLOW_CPU=1 ASCIIFLOW_VULKAN_VALIDATION=1 \
cargo test -p asciiflow-interop --test p010_qualification \
  software_ten_bit_media_cpu_vulkan_ascii_are_byte_exact -- --ignored
```

On the Intel Arc host, the descriptor, hwdownload and direct P010 input-import
test compares 30 frames per codec and requires `/dev/dri/renderD128`:

```bash
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop --test hardware \
  ten_bit_vaapi_descriptors_and_hwdownload_reference -- --ignored --nocapture
```

For the two 3000-frame P010 direct-input stress cases and the paired Release
benchmark, first create temporary looped inputs from the checked-in 36-frame
fixtures (or set `ASCIIFLOW_STAGE52B_HEVC_STRESS_INPUT` and
`ASCIIFLOW_STAGE52B_AV1_STRESS_INPUT` to equivalent absolute paths):

```bash
ffmpeg -hide_banner -loglevel error -y -stream_loop 84 \
  -i tests/fixtures/codecs/hevc-main10-sdr-gradient.mp4 \
  -frames:v 3000 -c copy /tmp/asciiflow-stage52b-hevc-3000.mp4
ffmpeg -hide_banner -loglevel error -y -stream_loop 84 \
  -i tests/fixtures/codecs/av1-main10-sdr-gradient.mp4 \
  -frames:v 3000 -c copy /tmp/asciiflow-stage52b-av1-3000.mp4
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop --test hardware \
  ten_bit_p010_interop_reuse_is_exact_and_fd_bounded -- --ignored --nocapture
cargo test --release -p asciiflow-interop --test hardware \
  ten_bit_p010_decode_paths_300_frame_benchmark -- --ignored --nocapture
```

Run the benchmark without Validation for interpretable timings. It warms up
36 frames and measures three 300-frame runs for each of software decode,
VAAPI+hwdownload and VAAPI+DMA-BUF input import. The test prints process CPU,
decode, download, CPU import setup, GPU copy/map/render, backend wall and
latency separately. [Stage 5.2B](stage5.2b-p010-decode-validation.md) records
the Intel results and their 64×64 scope.

Stage 5.2C-1 uses an opt-in, encoder-free VAAPI P010 frames pool to qualify
the output transfer on `/dev/dri`. The actual descriptors, parity, FD counts,
Validation results, NV12 control and three-run 1080p output benchmark are in
[its qualification report](stage5.2c1-p010-output-interop.md):

```bash
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop \
  --features p010-output-diagnostic --test hardware \
  p010_output_3000_frame_fd_stress -- --ignored --nocapture
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop \
  --features p010-output-diagnostic --test hardware \
  p010_output_faults_preserve_cause_and_do_not_reuse_surfaces -- --ignored --nocapture
cargo test --release -p asciiflow-interop \
  --features p010-output-diagnostic --test hardware \
  p010_output_300_frame_benchmark -- --ignored --nocapture
```

The deliberately invalid-FD diagnostic should be run separately without
Validation. These C-1 tests are diagnostic, not an encoder; Stage 5.2C-2
qualifies the separate production path.

```bash
ASCIIFLOW_VULKAN_ALLOW_CPU=1 \
ASCIIFLOW_VULKAN_VALIDATION=1 \
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json \
cargo test -p asciiflow-vulkan p010_ -- --ignored
```

The `p010_` filter also includes the opt-in 300-frame 1080p benchmark and
3000-frame reuse test. Run the benchmark in Release for interpretable timing:
`cargo test --release -p asciiflow-vulkan
p010_synthetic_1080p_300_frame_benchmark -- --ignored --nocapture`.
See [the P010 contract and measured scope](p010.md). Production AV1 10-bit
output remains unsupported. HEVC Main10 requires explicit
`--output-codec hevc --output-bit-depth 10` and BT.709 SDR P010 input.

## HEVC Main10 production encode (Stage 5.2C-2)

On the qualified Intel render node, use a 128×128-or-larger true-10-bit
HEVC Main10 BT.709 SDR input. The checked-in 64×64 fixture is deliberately too
small for this driver's Main10 encoder. Set the paths below to retained inputs
or generate them as described in the [qualification report](stage5.2c2-hevc-main10-encode.md):

```bash
ASCIIFLOW_STAGE52C2_HEVC_INPUT=/absolute/path/to/main10-30.mp4 \
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop --test hardware \
  main10_encoder_owned_full_interop_30_frame_parity -- --ignored --nocapture

ASCIIFLOW_STAGE52C2_HEVC_STRESS_INPUT=/absolute/path/to/main10-3000.mp4 \
cargo test -p asciiflow-interop --test hardware \
  main10_encoder_owned_full_interop_3000_frame_stress -- --ignored --nocapture
ASCIIFLOW_STAGE52C2_HEVC_STRESS_INPUT=/absolute/path/to/main10-3000.mp4 \
cargo test -p asciiflow-interop --test hardware \
  main10_staged_encode_3000_frame_fd_stress -- --ignored --nocapture
cargo test -p asciiflow-media \
  injected_main10_send_receive_and_drain_failures_preserve_cause -- --ignored
```

The parity test downloads the real encoder-owned P010 surface before sending
it, compares all 30 frames byte-for-byte to the staged P010 reference, and
checks that active 10-bit low bits remain. It does **not** demand byte-exact
equality from the lossy decoded HEVC stream. The two 3000-frame tests record
FD before/steady/after and decode-back counts. Regular planner/CLI tests cover
explicit depth policy, exact Main10 failure facts, staged auto replan, and
strict interop requests. The report records bitstream, AAC, FreeType,
cancellation, 1080p benchmark, Validation and `spirv-val` evidence.

## AV1 Main 10-bit production encode (Stage 5.2C-3)

Use an explicitly tagged BT.709 SDR, left-chroma, 128×96-or-larger P010 input
on the qualified Intel node. The 30-frame AV1 input used below was generated
by the production AV1 10-bit encoder from a true-low-bit HEVC Main10 source;
the 3000-frame variant is a stream-copy loop. This avoids treating an
unspecified-chroma intermediate as a qualified input. See the
[qualification report](stage5.2c3-av1-10bit-encode.md).

```bash
ASCIIFLOW_STAGE52C3_AV1_INPUT=/absolute/path/to/av1-main-10bit-30.mp4 \
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop --test hardware \
  av1_10bit_encoder_owned_full_interop_30_frame_parity -- --ignored --nocapture

ASCIIFLOW_STAGE52C3_AV1_STRESS_INPUT=/absolute/path/to/av1-main-10bit-3000.mp4 \
cargo test -p asciiflow-interop --test hardware \
  av1_10bit_encoder_owned_full_interop_3000_frame_stress -- --ignored --nocapture
ASCIIFLOW_STAGE52C3_AV1_STRESS_INPUT=/absolute/path/to/av1-main-10bit-3000.mp4 \
cargo test -p asciiflow-interop --test hardware \
  av1_10bit_staged_encode_3000_frame_fd_stress -- --ignored --nocapture
cargo test -p asciiflow-media \
  injected_av1_10bit_send_receive_and_drain_failures_preserve_cause -- --ignored
```

The real encoder-owned AV1 P010 surface is compared byte-for-byte before
encode against staged Vulkan output, including active low bits. Real AV1
decoded pixels are not expected to be byte-exact after lossy encoding.
The independent generic P010 output test can be rerun with
`cargo test -p asciiflow-interop --features p010-output-diagnostic --test
hardware p010_packed_output_is_bit_exact -- --ignored`.
