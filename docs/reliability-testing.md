# Reliability qualification

Stage 5.4D-1 is a workload-scoped qualification, not a new media feature or a
24/7 uptime guarantee. See the [D-1 ledger](stage5.4d1-soak-failure-hardening.md)
and [machine-readable receipt](../tests/soak/stage54d1.json).

## Production contract

- Project-owned queues are bounded. Record configured capacity and observed
  high-water marks separately; a cap is not a measured peak. Mux intermediate
  flushing retains the 64-packet/8 MiB limits.
- One mux owner writes output; arrival timing does not decide merge order.
  No runtime codec/backend fallback is introduced.
- Finalize same-directory staging before atomic rename. Pre-commit failure or
  cancellation preserves the destination. Post-commit diagnostic failure must
  not delete the committed output. Cross-filesystem non-atomic copy is forbidden.
- Cooperative cancellation joins workers before return. Panic guards publish
  stage/root and cancellation before ordered joins can block. Owned resources
  are bound after the guard so destruction is covered. Native decoder/encoder
  owners still outlive processor teardown and outstanding surfaces.
- Preserve substantive roots over cancellation/disconnection. Mux unwind latches
  the root before dropping receivers. This is Rust unwind containment, not
  recovery from abort, OOM, native crashes or driver hangs.
- No new output-replacement permission preservation policy is promised.

## Finite source generation

Use fresh directories; never overwrite retained inputs/oracles:

```sh
python3 -B tests/soak/generate.py sdr --frames 100000 --output target/d1-new/sdr
python3 -B tests/soak/generate.py pq-preserve --frames 100000 --output target/d1-new/preserve
python3 -B tests/soak/generate.py pq-to-sdr --frames 100000 --output target/d1-new/to-sdr
```

Fixed FFmpeg/ffprobe 8.1.3 materializes a finite MP4 by controlled compressed
packet repetition. Production opens it once, never rewinding its live demuxer.
SDR uses the existing algorithmic H.264 generator. PQ uses checksum-verified
checked-in C3 legal HEVC/AV1 sources. `identity.json` preserves commands, tool
versions/binary hashes, source/generator hashes, size and counted metadata.
These sources are video-only; audio-copy requires its own identified workload.
Generated inputs are not qualified outputs. Independently verify full decode,
CFR timestamps, metadata, selected GPU path and unchanged same-stack oracles.
Never promote a 1k preflight to a 100k resource gate.

Run the three short full-GPU paths, required Validation layer, metadata/timing
probe and complete decode-back through the existing corpus watchdog:

```sh
python3 -B tests/soak/preflight.py --frames 1000 --output target/d1-new/preflight
```

This accepts at most 10k frames per path and deliberately cannot stand in for
the primary 100k resource-soak campaign. Its sampled peak process RSS is
supplemental, not frame-cadence or GPU allocation evidence.

## Resource evidence

Sample at a logical cadence, e.g. every 1,000 accepted frames, supplemented by
wall-clock samples. Preserve frames/packets/bytes/media and wall counters,
initial/warm/steady/final RSS and FD, complete series, median/p95 and slope.
Classify resources: Stable, WarmupThenStable, ExpectedCacheGrowth, Leak or
Unresolved. Allocator caching and MP4 tables are not GPU/VAAPI leak evidence.
Process exit or parent FD stability does not prove same-process GPU cleanup.
Missing exact queue peaks, DMA-BUF outstanding objects, Vulkan allocations or
VAAPI active surfaces remain Unresolved and block sealing. Three 100k paths
are mandatory; 500k is extended characterization.

## Native mux replay

```sh
ASCIIFLOW_MUX_REPLAY_DIRECTORY=/absolute/fresh/evidence-directory \
  cargo test --release -p asciiflow-media --features mux-qualification --lib \
  ffmpeg::encoder::mux_replay::soak_dual_and_sparse_producer_merge_replay \
  -- --ignored --exact --test-threads=1 --nocapture
```

500 dual-AAC and 500 early-audio-EOF replays compare every MP4 byte-for-byte
against each fixture's first output. Per-run FD must return to post-capture
baseline; capture teardown must restore pre-capture baseline. JSONL retains
RSS/FD/comparison results, plus two reference MP4/trace pairs. Only successfully
compared, freshly generated non-reference files are removed; failures retain
artifacts. Run alone so unrelated tests cannot perturb process FD counts.
Between-replay RSS is not frame-cadence/in-flight sampling or a leak verdict.

## Remaining campaign

Exercise slow producers/consumers/sinks, ENOSPC/EIO at header/packet/flush/trailer,
staging/permission/commit faults, primary-plus-cleanup failures, 20 phase-targeted
cancellations and races, 100 successful/50 failed jobs, mixed jobs and state
contamination. Re-run 17×3 retained outputs, B subset, C canonical core,
alternate 10k smoke, three 10k Validation soaks, all SPIR-V, default/measurement
tests and lints, and two isolated clean builds. Short historical tests remain
coverage inputs, not substitutes for these gates.
