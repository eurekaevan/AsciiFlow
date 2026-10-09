# Reliability testing

The production model is one CLI process, one media job, then process exit.
Qualification covers recorded single-job workloads and long-run production
paths, not a daemon, persistent worker or unconditional 24/7 service guarantee.

## Recorded results

The [long-soak receipt](../tests/soak/long-soak-qualification.json) records three
representative full-GPU paths with at least 100,000 video frames each:

- SDR H.264 → VAAPI decode → Vulkan → NV12 → H.264 VAAPI → MP4.
- PQ HEVC Main10 → VAAPI P010 → Vulkan preserve → HEVC Main10 → MP4.
- Legal-domain PQ AV1 → VAAPI P010 → Vulkan PQ-to-SDR → HEVC Main10 SDR → MP4,
  with dual AAC copy.

Full decode-back, color/HDR semantics, audio-copy checks, queue bounds, tracked
resource/FD cleanup, long-progress cancellation and injected failure passed for
the recorded workloads. This is not a universal driver or container claim.

The [audio-memory attribution](../tests/soak/audio-memory-attribution.json)
classifies the audio-enabled residual as `BoundedAudioPacketAllocatorRetention`:
live packet ownership remained bounded and packets were logically released.
Normal non-fragmented MP4 also retains sample indexes proportional to packet
count until trailer/teardown. Neither is a constant-memory promise.

Persistent multi-job process memory remains **unqualified**. Earlier memory
observations and failed diagnostics are not reclassified as passes. They do not
define the current single-job CLI production contract.

## Maintenance checks

For lifecycle, queue, ownership or long-run changes, run the affected existing
tests in `tests/soak/` and the relevant Rust integration tests. Record actual
production binary/stack identity, source generator/input hash, resource samples,
queue peaks, output/decode-back oracle and cancellation/failure behavior.

Use frame-count-correlated trends, not an arbitrary absolute RSS threshold.
Distinguish bounded warm-up, required container state and released allocator
pages from unexplained live growth. Do not claim complete driver-internal
resource visibility. Tracked job-owned objects must recover, FD counts must
recover, and observed queue peaks must stay within their configured capacity.

The pipeline queue capacity is 3; mux channels have capacity 16 each. The
64-packet / 8-MiB intermediate mux flush trigger is a batching trigger, not a
bound on total process memory or MP4 sample indexes. SIGINT must exit 130,
remove staging output, and leave an existing destination untouched.

Documentation/UI-only changes do not require another 100k campaign. Use targeted
smokes and regression checks; investigate further only when the affected path
or actual evidence warrants it. There is no staged release-gating workflow.
