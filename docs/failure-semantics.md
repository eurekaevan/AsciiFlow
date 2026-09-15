# Failure and cancellation semantics

Stage 4.1 makes failure behavior part of the AsciiFlow v2 contract. It does not
add a codec, pixel format, backend, or performance optimization.

## Failure model

Pipeline errors carry a stage, a stable operation name, and the original error
chain. The stages distinguish input and capability probing, planning, decoder,
input interop, processor, output interop, encoder and muxer initialization,
their runtime counterparts, cancellation, drain, and finalization. Native
diagnostics such as the FFmpeg error text, VAAPI device/codec context, DRM
format and modifier, or `VkResult` remain in the chain. Raw pointers and native
handles are never printed.

Concurrent workers share a first-failure latch. The first substantive failure
cancels the other stages and remains the reported root cause; later channel
disconnects are shutdown consequences and cannot replace it. Every worker is
joined before the pipeline returns.

## Automatic replan and explicit policy

Automatic policy may replan once when initialization proves one previously
qualified capability unusable. Only the precise failed fact is changed. For
example, input DMA-BUF import failure disables input interop, while output
interop and Vulkan remain eligible; output import failure preserves staged
VAAPI encoding when Host upload is qualified.

The diagnostic contains the initial plan, first failure, replanned plan, and a
second failure if the retry also fails. A second initialization failure is
terminal. Explicit `--decode vaapi`, `--backend vulkan`, `--encode vaapi`, or
explicit interop never silently falls back.

Replanning is initialization-only. Once the first media frame enters the
pipeline, decoder, interop, Vulkan, encoder, or muxer failure terminates the
whole run. AsciiFlow never changes the decoder, processor, or encoder
mid-stream.

## Lifecycle and cancellation

The effective lifecycle is Created, Initialized, Running, Draining,
Finalizing, and then Completed, Failed, or Cancelled. Replan is legal only
before Running.

On Unix, Ctrl+C sets a cooperative cancellation token; it does not call
`process::exit`. Decoder, processor, encoder, interop workers, bounded channel
sends/receives, and drain observe the token. The process joins all workers,
removes the staging file, leaves the destination unchanged, and exits with
status 130. A normal failure exits non-zero; argument parsing retains clap's
status 2; success is 0. Cancelled and failed runs do not print a successful
completed-frame summary.

Vulkan fence waits used by the streaming path are bounded to five seconds.
`VK_ERROR_DEVICE_LOST` stops new submissions and is reported as the processing
root cause; it never triggers replan. If completion cannot be established by
the deadline, AsciiFlow reports a GPU teardown timeout and deliberately
abandons in-flight device objects instead of destroying memory that the GPU may
still reference. Process termination lets the OS/driver reclaim those objects.
This is a containment policy, not a claim that a hung device can be recovered.
Normal completion and cancellation with responsive hardware wait for pending
slots before their imported DMA-BUF memory, mappings, AVFrame references, and
VAAPI surfaces are released.

## Output safety

The muxer writes a hidden, uniquely reserved staging file in the destination
directory. The destination is replaced only after decoder and processor drain,
encoder delayed-packet drain, successful trailer write, and successful worker
completion. Initialization failure, runtime failure, cancellation, trailer
failure, and unwinding delete the staging file. An existing destination is
therefore preserved byte-for-byte on every unsuccessful run.

Keeping staging beside the destination makes the final rename same-filesystem.
On platforms/filesystems whose rename operation replaces atomically, the
commit is atomic. AsciiFlow does not promise atomic replacement on a filesystem
that does not provide those semantics.

## Input and EOF contract

The current pixel pipeline accepts only 8-bit, 4:2:0 input with non-zero even
dimensions representable by the native APIs. Ten-bit video is rejected before
output creation with: `10-bit video is not supported by the current 8-bit NV12
pipeline`. There is no silent 10-to-8 conversion. Odd dimensions are rejected,
not rounded by the planner.

ASCII width is constrained to 1 through 8192 and the grid to four million
cells; dimensions larger than the frame are safely clamped. These checks occur
before pixel allocation.

Normal EOF drains delayed decoder frames, pending processor slots, delayed
encoder packets, and finally the mux trailer. Tests cover zero, one, two,
several odd frame counts, duplicate/missing/negative/large timestamps, MP4 edit
lists, no-video input, ten-bit input, probe truncation, tail truncation, and
corrupt packets. The current encoder intentionally produces an ordered CFR
timeline from source frame rate; input PTS values are preserved through the
processing boundary for diagnostics but are not a promise of VFR output.

## Test-only failure injection

Internal test hooks inject deterministic, single-shot failures at decoder
creation, VAAPI frame-pool creation, input DRM mapping/import, Vulkan processor
creation, output surface acquisition/mapping/import, encoder creation, and
muxer creation. Separate fake components inject decode, processing, submit/wait,
device-lost, encode, mux, drain, and cancellation failures. The hooks are
compiled only for tests and do not use environment variables or affect the
production hot path.

Linux failure stress repeatedly rejects invalid media and checks
`/proc/self/fd`. Real-device validation additionally exercises repeated
capability probes, mid-run SIGINT, pending Vulkan slots, immediate
reinitialization, and Khronos validation.

## Current Intel validation evidence

On 2026-09-15, the Stage 4.0 1920x1080, 50 FPS fixture was rerun on Intel Arc
MTL (0x8086:0x7d55), Mesa ANV 26.1.8, FFmpeg 9.0.1, and iHD 26.1.5.

- All 14 Release SPIR-V modules passed `spirv-val --target-env vulkan1.3`.
- Full interop success and pending-slot cancellation completed with Khronos
  validation enabled and zero Validation, VUID, or synchronization errors.
- Input/output descriptor parity, pre-ASCII and pre-encode byte parity,
  two-slot output parity, and deliberate import-failure cleanup tests passed.
- One hundred complete capability probes had stable FD count. Twenty
  consecutive conversions cancelled after entering Running all exited 130,
  preserved the destination hash, removed staging, and immediately
  reinitialized VAAPI/Vulkan; the stress runner FD count remained 5 to 5.
- The stress test exposed and then verified a real lifetime bug: the decoder
  context could be destroyed while an interop worker still called
  `vaSyncSurface` on a retained decode surface. Decoder and encoder contexts
  now remain owned by the outer pipeline scope until all downstream workers
  have joined and their VAAPI surfaces have been released.
- The 300-frame Release auto runs were 529.18, 532.26, and 529.92 FPS (median
  529.92). Against Stage 4.0's 526.40 FPS median this is +0.67%, so there is no
  measurable regression. Three auto files and the explicit full-interop file
  were byte-identical; packet timeline and decoded framemd5 hashes also
  matched. The output remained H.264 High, yuv420p, BT.709 limited range,
  1920x1080, 50 FPS, and 300 frames.
