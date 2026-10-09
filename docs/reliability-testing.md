# Reliability qualification

Stage 5.4D-1 is a workload-scoped qualification, not a new media feature or a
24/7 uptime guarantee. See the [D-1 ledger](stage5.4d1-soak-failure-hardening.md)
and [current machine-readable receipt](../tests/soak/stage54d1b-long-soak.json).
The [original D-1 receipt](../tests/soak/stage54d1.json) is historical evidence,
not the current campaign ledger.

## Current D-1A product scope — 2026-10-08

The current product contract is one CLI process per media job, followed by
process exit. Stage 5.4D-1A is sealed within that scope. Persistent-process
allocator/runtime retention across multiple jobs is not fully bounded and is
recorded as `NonBlockingOutOfScopeObservation`; this scope decision does not
qualify a persistent or service-style process. Historical repeated-job failures
remain unchanged in the [scope-closure record](../tests/soak/stage54d1a-product-scope-closure.json)
and its referenced receipts. D-1B remains a separate, unsealed qualification.

## Current D-1B status

**D-1B and D-1 overall are SEALED** as of the final2026-10-09 audio-memory
attribution. Full GPU C with captured original AAC retains8,312KiB residual
and exactly matches the retained output, selecting the live producer→mux
interaction rather than ongoing native input reading. The last-native-reference
census remains10/10 packets (3,482/3,488 bytes peak) from10k through100k;
each track reads/submits93,751 packets and finishes with zero live bytes.
The queue stays within16; explicit packet/list free paths and one post-teardown
trim support `BoundedAudioPacketAllocatorRetention` for the recorded single-job
workload. Trim reclaims67,704KiB whole-process, not an audio-specific allocation
sum. No residual==0 or persistent-process RSS plateau is claimed. No default
production behavior changed. See the [final report](stage5.4d1b-long-soak.md)
and [audio attribution receipt](../tests/soak/stage54d1b-audio-memory-attribution.json).
The exact reliability claim is “Qualified for the recorded single-job CLI
workloads and long-run production paths.” D-2 is justified but not started.

### Historical pre-attribution decision

D-1B is **NOT SEALED** after the required finite campaigns completed.
The 2026-10-09 layer-isolation controls narrow the remaining Path C gate:
matched mux-only replay has −52 KiB residual; identical full-GPU video with
audio disabled has +24 KiB, versus full dual-AAC C's 8,304 KiB. The specific
audio-enabled upstream/native input state or audio/mux interaction is not yet
accounted for. This is not an instrumentation attribution or a proven leak.
See the long-soak report and retained layer-isolation receipt. No production
behavior changed, no larger soak/matrix was added, and D-2 has not started.
All three 100k full-GPU paths, output checks, lifecycle checks, long-progress
controls and required regressions passed. The remaining gate is unexplained
frame-count-correlated memory growth in single-job Path C, confirmed in the
default Release repeat. This is distinct from D-1A's out-of-scope repeated-job
observation. No absolute MiB threshold or strict return-to-start RSS gate was
introduced. See the [final long-soak report](stage5.4d1b-long-soak.md);
earlier D-1 ledgers below are historical where marked. D-1 overall remains
NOT SEALED; D-2 is not justified and was not started.

The exact-package MOV review accounts for required non-fragmented MP4 indexing
by per-track packet counts, 1024-entry rounded capacity and 72-byte entries.
This required state is not a leak and does not have to plateau with duration.
The memory contract is **no unexplained frame-count-correlated memory growth
beyond container/runtime structures whose required growth has been explicitly
accounted for**. C still has approximately 8.11 MiB residual over 50k→the last
default-repeat sample; the stricter source-specific model does not pass that
remaining gate. Constant-memory muxing is not claimed.

## Production contract

- Project-owned queues are bounded. Record configured capacity and observed
  high-water marks separately; a cap is not a measured peak. Mux intermediate
  flushing retains the 64-packet/8 MiB thresholds, not a total memory cap.
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
The checked-in source fixtures are video-only; D-1B creates separately
identified audio-bearing inputs under its [current long-soak scope](stage5.4d1b-long-soak.md).
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

### D-1A observation mode

Build with `cargo build --release -p asciiflow-cli --features reliability-measurement`.
Set `ASCIIFLOW_RELIABILITY_REPORT` to a new JSONL path for one conversion.
Default builds contain neither the per-frame observation hooks nor ownership
tokens. Measurement observations do not select packets, change worker policies,
or alter media decisions. The process-scoped session starts before native
construction and closes after workers/native owners return from conversion.
It streams bounded counter state directly, with no background logging queue.
Failure retains partial observations. A report error is surfaced separately;
it never deletes committed output or replaces the primary media error.

```sh
python3 -B tests/soak/preflight.py --frames 10000 --measurement \
  --output target/d1a-new/measurement-10k
```

Samples include initial, first completed frame (post-init), each 1k completed
frames, pre-finalization and post-cleanup. Frame/packet counts are distinct.
RSS and FD are current-process observations from `/proc`; the observer's own
directory-enumeration FD is excluded. The report file itself remains open at
both FD boundaries. Resource peaks are exact for registered ownership tokens.
Release follows native release; abandonment or failed allocator release stays
active and causes report failure rather than a fabricated balanced count.

`vulkan_buffer_bindings` counts successfully bound buffer allocations and
their allocator allocation sizes, not allocator backing blocks or total GPU
heap use. `dma_buf_imported_plane_bindings` counts successfully bound external
plane images; a shared DMA-BUF imported twice counts twice, not unique memory.
VAAPI metrics count exported decoder/acquired encoder AVFrame references, not
all driver surfaces, encoder-internal references or pool storage. Scratch
decoder frames and pre-binding transient allocations are outside these counts.
Unknown driver memory/surface counts and FFmpeg internal pending bytes are
explicitly unavailable/null, never zero.

Crossbeam queue depth is sampled before queue operations. Its peak is labeled
`MeasuredPeakSampled`, not an exact atomic high-water mark. Capacity is a
`ConfiguredHardBound`. Latest queue observations retained in post-cleanup rows
are not cleanup-time depth measurements. Queue payload bytes are unavailable.
No instrumentation lock is held while sending/receiving or calling media code.

Mux counters mean successful packet writes since the last explicit
interleaver flush. A flush is triggered **after** a packet crosses 64 writes or
8 MiB; a single admitted packet may be up to 16 MiB. Neither counter measures
live FFmpeg pending packets/bytes. The two bounded producer channels each have
16 entries, and the merge owns at most one head from each. FFmpeg sample tables,
AVIO buffers, codec buffering and driver allocations are separate/unknown;
there is no supported claim that total mux memory is at most 8 MiB.

`analyze-resources.py` separates lifecycle boundaries from runtime trend
windows, preserves missing values, and needs at least nine unique runtime
progress points. Three-window slopes and median/p95 are characterization;
`Stable`/`WarmupThenStable` mean `ShortStabilityOnly`, not long-soak qualification.
Sustained unattributed growth is `LeakSuspected`, not proof of a leak; neither
positive RSS growth nor short plateaus alone establish a leak verdict.

The [historical D-1A A/B receipt](../tests/soak/stage54d1a-final-closure.json)
preserves ten consecutive default-allocator, validation-disabled 50-cycle
processes and their complete smaps series. Tracked ownership/FD/thread/recovery
gates passed, but residual late private-anonymous growth was `Unresolved`; this
repeated-job result remains historical under the current one-job D-1A scope.
Required-Validation lifecycle evidence is separate from normal-production RSS.
The current D-1B measurement-memory observation is summarized in the
[long-soak status](stage5.4d1b-long-soak.md).

### Current ownership map

| Resource | Creator → releaser / thread lifetime | Bound and observation |
|---|---|---|
| Input demux/decoder | Decoder open → RAII native close; owner outlives decode worker and exported surfaces | One input context per job; native buffering Unknown |
| Audio demux | AudioReader → RAII close after producer joins | One reader; selected tracks share ordered producer; internal buffering Unknown |
| VAAPI decoder refs | Clone decoded frame → AVFrame free after interop read completes | Exported refs measured; driver pool is dynamic/Unknown |
| DMA-BUF planes | Successful image/import/bind → destroy/free after GPU ownership return | Measured owned plane bindings/bytes; abandoned device resources stay active |
| Vulkan buffers | Successful bind → explicit buffer destruction and allocator release | Measured owned bindings/bytes, not allocator heaps |
| In-flight slots | Processor initialization → joined worker/slot teardown | Configured two slots, not total surface bound |
| Encoder surface refs | Pool acquire → AVFrame free after consumer handoff | Acquired refs measured; codec-internal refs Unknown |
| Decoded/processed queues | Pipeline → channel Drop after joined workers | Configured capacity, normally 3; sampled depth/peak, bytes unavailable |
| Audio/video mux queues | Encoder initialization → mux join/channel Drop | 16 entries each, 16 MiB admitted packet limit; sampled depth |
| Mux merge heads | MuxInbox → consume/drop in single mux owner | At most one head per producer, apart from channel entries |
| Mux/container/AVIO | Encoder transfers output owner → mux finalization/Drop | Writes-since-flush measured; native total internal buffering Unknown |
| Staging output | TemporaryOutput → atomic rename on success or cleanup Drop | One same-directory staging file; pre-commit failure preserves destination |
| Workers | Scoped pipeline/mux construction → joins before native owner release | Fixed pipeline/mux/slot topology; panic guard publishes cancellation/root first |

Cancellation must join before releasing borrowed decoder/encoder contexts.
Primary worker/native errors take precedence over cancellation/channel teardown;
secondary cleanup or diagnostic errors must not mask the root. Native abort,
OOM and uninterruptible driver calls are not caught by Rust unwind guards.

ENOSPC/EIO AVIO callback injection exercises actual native mux finalization,
but remains **SimulatedOnly**. It is not real isolated-filesystem exhaustion,
nor proof of header/packet failures or GPU cancellation cleanup. The repeated
software CLI suite uses actual EFBIG via RLIMIT_FSIZE; never relabel it ENOSPC.

### Shader census

`crates/asciiflow-vulkan/build.rs` is authoritative: 25 unconditional module
names plus one optional FP64 experiment. Historical 1,203 and later 1,003
counts describe cached artifact copies across Cargo build directories, not
1,203/1,003 distinct algorithms. The 1,003-copy inventory consisted of 748 core,
84 PQ preserve, 102 C3, 50 SDR pack, 13 domain and 6 FP64 copies (26 unique
names). Comparison with the preserved C2A `static-final/summary.json` resolves
the discrepancy: that 1,203-file census also included four isolated C2 clean
build targets, each with 25 debug plus 25 release copies (200 additional files).
All 200 still exist; no deletion or missing production variant is implied.
The D-1 census used only `target/debug/build` and `target/release/build`.
After this pass's three additional 25-module cache outputs, those roots contain
1,078 copies and the full target tree contains 1,278. All original 1,203 paths
retain their hashes. Clean-build inventories must demonstrate
all required names and validate each emitted module for Vulkan 1.3.

Sample at a logical cadence, e.g. every 1,000 accepted frames, supplemented by
wall-clock samples. Preserve frames/packets/bytes/media and wall counters,
initial/warm/steady/final RSS and FD, complete series, median/p95 and slope.
Classify resources: Stable, WarmupThenStable, ExpectedCacheGrowth, Leak or
Unresolved. Allocator caching and MP4 tables are not GPU/VAAPI leak evidence.
Process exit or parent FD stability does not prove same-process GPU cleanup.
The broader historical D-1 checklist treated missing exact queue peaks,
DMA-BUF outstanding objects, Vulkan allocations, and VAAPI active surfaces as
unresolved. It does not override the current D-1A scope decision. Current D-1B
execution and its unresolved memory observation are recorded in the
[long-soak status](stage5.4d1b-long-soak.md); 500k remains extended
characterization.

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

## Historical broader-campaign checklist

The following list records the original broad D-1 campaign scope. It is not the
current D-1A status or the live D-1B checklist; use the [current D-1B
long-soak document](stage5.4d1b-long-soak.md) for current scope and progress.

Exercise slow producers/consumers/sinks, ENOSPC/EIO at header/packet/flush/trailer,
staging/permission/commit faults, primary-plus-cleanup failures, 20 phase-targeted
cancellations and races, 100 successful/50 failed jobs, mixed jobs and state
contamination. Re-run 17×3 retained outputs, B subset, C canonical core,
alternate 10k smoke, three 10k Validation soaks, all SPIR-V, default/measurement
tests and lints, and two isolated clean builds. Short historical tests remain
coverage inputs, not substitutes for these gates.
## Native closure qualification

`native-reliability` is an explicit test/qualification feature, separate from
normal production. It exposes event-controlled checkpoints after native decode,
after successful encoder submission before packet drain, after Vulkan submit
before fence wait, at video EOF/finalization, after the first worker join, and
before/after final commit. Qualification-only injected errors are not actual
driver failures. Real SIGINT tests wait for handler acknowledgement before
releasing the selected gate; exit codes alone are not commit-order evidence.

Queue observation now distinguishes pre-attempt samples from snapshots at
every successful enqueue/dequeue boundary. `MeasuredPeakAtOperationBoundary`
is not an atomic exact high-water mark: another thread may mutate the channel
before `len()` is read. Capacities are still hard bounds; neither these peaks
nor 64-packet/8-MiB flush counters imply a total native memory bound. Unknown
driver/internal quantities remain null / InternallyUnobservable.

Native AVIO fault tests record the actual write callback boundary and bytes
accepted before ENOSPC/EIO. Test direct-I/O mode makes these boundaries
controllable; production AVIO buffering is unchanged. These are SimulatedOnly.
Healthy native forwarding to `/dev/full` separately exercises kernel ENOSPC,
not real quota-limited filesystem exhaustion. No host filesystem is filled.
Controlled native AVIO delays and a distinct mux-to-pipeline held-owner test
must not be conflated: the latter proves propagated project backpressure, not
that the qualification hook itself is a native AVIO callback.
