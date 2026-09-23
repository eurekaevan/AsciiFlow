# Regression testing

The default workspace suite covers portable planning, codec policy, failure
semantics, media fixtures, fonts, and CPU processing. Real Intel VAAPI/Vulkan
tests are opt-in because they require a qualified device, drivers, and sometimes
a retained long-form input. Stage-specific measurements and hardware outcomes
live in their validation reports.

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
See [the P010 contract and measured scope](p010.md). Production HEVC Main10
and 10-bit AV1 must continue to fail input qualification.
