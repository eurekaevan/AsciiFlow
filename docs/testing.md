# Regression testing

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
long run. Hardware validation was not rerun for Stage 4.2.1: this environment has
no `/dev/dri`. No new performance claim is made; Stage 4.2 remains the reference.
