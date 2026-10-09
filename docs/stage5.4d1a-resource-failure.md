# Stage 5.4D-1A — resource observability / failure matrix

## Current product-scope decision — 2026-10-08

**Stage 5.4D-1A is SEALED for the current product contract:** one CLI process
handles one media job and exits. This is a scope decision, not a claim that a
persistent process can safely retain resources across multiple media jobs.
Persistent-process allocator/runtime retention remains
`NonBlockingOutOfScopeObservation`; it has not been fully bounded.

The prior repeated-job memory results remain immutable historical evidence,
including the +69,648 KiB RSS **FAIL**, the +652 KiB cycle-48 observation, and
the ten-process campaign totals of 4,418 cycles / 18,025 jobs. None is rewritten
or relabeled. The current closure record is
[stage54d1a-product-scope-closure.json](../tests/soak/stage54d1a-product-scope-closure.json),
which references the immutable historical receipts. The current A/B closure
receipt identifies A14 as the remaining multi-job memory gate; that gate is
outside the one-job process contract. No other current D-1A production blocker
is identified by the current closure review. Historical `NOT SEALED`, `FAIL`,
and open-gate statements below describe their dated evidence and are not the
current product-scope status.

Stage 5.4D-1B is not sealed by this decision. No D-1B result or formal 100k
soak is inferred.

The [ownership/bounds contract](reliability-testing.md#d-1a-observation-mode)
documents each owner, native release point and observation boundary. The
[machine-readable receipt](../tests/soak/stage54d1a-resource-failure.json)
retains path/input/output identities, full resource series, fault evidence,
execution scope and open gates. Unknown metrics are not zero.

## Historical default-memory closure result — 2026-10-08

### Completed memory-only follow-up

The previous ten-process/50-cycle result below remains **Unresolved**, including
process 01's +652 KiB at cycle 48. Its raw evidence contains rollup accounting,
not historical allocator/per-mapping snapshots; that exited process cannot be
retroactively attributed to a heap, library or stack. New observations are not
a recovery or reclassification of that event.

`memory_plateau.rs` adds only feature-gated native-test observation. The fixed
four jobs are unchanged, with the original four recovery points repeated per
50-cycle macro sequence. Ten fresh Release processes run at least 200 cycles;
every new resident-anonymous high water resets the complete-cycle counter.
Adaptive runs stop only after 100 full cycles without a new high water or at
500; process 00 runs all 500 regardless of an earlier plateau. No result-dependent
replacement runs, allocator tuning, trimming, custom allocator or validation
layers are used. The launcher is `tests/soak/memory-plateau.py`.

The fixed cycle is ordered: (1) SDR NV12/H.264, builtin, color=true, audio none,
width 8; (2) PQ-preserving P010/HEVC, FreeType, color=true, single AAC copy,
width 80; (3) PQ→SDR NV12/H.264, builtin, color=false, dual AAC copy, width 80;
(4) PQ→SDR P010/HEVC, FreeType, color=false, audio none, width 80. Each job
processes three frames. All use Vulkan with VAAPI decode/encode, both interop
directions on and `/dev/dri/renderD128`. Original macro positions 10/30 cancel
at VulkanInFlight; 20/40 inject EncoderBusy failure plus cancellation, using the
original builtin/color=true/audio-none PQ→SDR P010 case. No random order or
new cache keyspace was introduced.

The preliminary `target/stage54d1a-memory-v1/target/memory-plateau` attempt
was interrupted and is diagnostic only: libtest's default output capture held
each CLI summary in memory until test exit, unlike normal CLI streaming. The
replacement frozen v2 campaign uses `--nocapture` and streams output to disk.
This removes a test-harness confound; it does not change production allocation
policy. The partial v1 evidence is preserved and is not one of the ten declared
qualification processes.

Every job and cycle boundary records read-only `mallinfo2`, RSS/PSS/Anonymous,
mapping categories/count and FD/thread counts. Every new high water also saves
exact smaps, smaps_rollup, status and `malloc_info` XML, linked to flushed job
resource evidence. Sequential reads are correlated, not atomic. Fixed pre-touched
proc buffers occupy 8.25 MiB throughout; transient JSON/XML observation can itself
leave allocator retention and is not described as zero-overhead.

The operational HWM metric retains the historical `smaps_rollup.Anonymous`
definition. That field does not intrinsically exclude shared anonymous pages;
raw shared/private VMA counters and conservative private-anonymous intervals are
preserved instead of conflating all Private_Dirty with anonymous pages. Unnamed
cached stacks stay in `anonymous_non_heap` unless independently attributed.
glibc's in-use counters include allocator bookkeeping/tcache semantics: they
are not an exact census of application-live objects or GPU/driver bytes.

All ten declared processes completed: **4,418 complete mixed cycles / 18,025
jobs** (17,672 successful conversions, 177 controlled cancellations, 176
injected encoder failures). These are test-hook failures, not actual device
failures. Five processes meet the operational HWM window; **five do not**.
There were no replacement runs. Every command exited zero because the observer
records a failed plateau as evidence instead of turning it into a fake PASS.

The [final memory receipt](../tests/soak/stage54d1a-memory-plateau.json),
[process observations](../tests/soak/evidence/memory-process-observations.json)
and [every-HWM review](../tests/soak/evidence/memory-hwm-review.json) retain
identities, every high-water event, allocator/mapping attribution and late-window
comparisons. The common archive retains commands, environment, source maps and
checkpoints; ten process archives retain **every job/cycle sample and all raw
smaps/rollup/status/XML snapshots**, not just selected events. Every archived
file was byte-verified against its original; the
[archive manifest](../tests/soak/evidence/memory-archive-manifests.json) records
all members and hashes. Raw RSS/PSS and private-anonymous intervals remain in
`memory.jsonl` inside each archive.

### Per-process results

Anonymous values and increments below are KiB; the HWM metric's conservative
scope is defined above. Largest increment excludes the initial observation.
"Window" is complete cycles since the **last** HWM, not an average slope.

| Process / PID | Cycles | Initial | Final | Initial→final delta | Largest increment | Last HWM | Window | HWM gate |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 00 / 24670 | 500 | 15,708 | 168,948 | 153,240 | 49,660 | 317 | 183 | Met |
| 01 / 101354 | 500 | 15,712 | 178,252 | 162,540 | 47,840 | 420 | 80 | **Not met** |
| 02 / 165008 | 500 | 15,712 | 178,172 | 162,460 | 48,896 | 423 | 77 | **Not met** |
| 03 / 214324 | 500 | 15,708 | 146,304 | 130,596 | 42,648 | 463 | 37 | **Not met** |
| 04 / 266485 | 368 | 15,712 | 143,872 | 128,160 | 42,816 | 268 | 100 | Met |
| 05 / 305070 | 245 | 15,708 | 168,016 | 152,308 | 49,380 | 145 | 100 | Met |
| 06 / 331233 | 500 | 15,712 | 147,876 | 132,164 | 43,064 | 482 | 18 | **Not met** |
| 07 / 382219 | 328 | 15,712 | 147,148 | 131,436 | 41,540 | 228 | 100 | Met |
| 08 / 414515 | 500 | 15,716 | 149,644 | 133,928 | 42,588 | 459 | 41 | **Not met** |
| 09 / 463507 | 477 | 15,712 | 145,048 | 129,336 | 49,656 | 377 | 100 | Met |

Process 00 was forced to 500 and maintained its window after first meeting it at
cycle 417. Adaptive processes 04/05/07/09 first meet their eligible window at
368/245/328/477. The other five reach 500 without meeting it. Initial anonymous
memory ranges 15,708–15,716 KiB and final memory 143,872–178,252 KiB; this fresh
process dispersion is not a substitute for within-process stability.

### Allocator accounting and mapping attribution

Final glibc counters are **bytes**, not RSS, exact application-live ownership,
or an absolute process-memory bound. Mapped is arena plus mmap; free retained is
arena free capacity. Full per-job/cycle values and late-100 ranges are preserved.

| Process | In-use incl. mmap | Mapped | Free retained arena | Late-100 VMA count |
| --- | ---: | ---: | ---: | ---: |
| 00 | 10,853,584 | 167,514,112 | 156,660,528 | 1,084 |
| 01 | 10,849,280 | 171,114,496 | 160,265,216 | 1,081 |
| 02 | 10,855,152 | 171,032,576 | 160,177,424 | 1,085 |
| 03 | 10,857,712 | 138,399,744 | 127,542,032 | 1,085 |
| 04 | 10,847,792 | 135,905,280 | 125,057,488 | 1,077 |
| 05 | 10,848,752 | 166,440,960 | 155,592,208 | 1,078 |
| 06 | 10,840,192 | 140,005,376 | 129,165,184 | 1,079 |
| 07 | 10,851,296 | 139,259,904 | 128,408,608 | 1,081 |
| 08 | 10,863,088 | 141,811,712 | 130,948,624 | 1,079 |
| 09 | 10,857,712 | 137,109,504 | 126,251,792 | 1,078 |

All 593 HWM records (including ten initial records) have raw snapshots. The
review uses sign-only accounting descriptions, not a new tolerance or a
statistical PASS rule. All ten final HWM increments belong to
`anonymous_non_heap`; file-backed, heap and kernel-named stack anonymous totals
do not increase at those final events. These location classes do not identify a
responsible library. Unnamed stacks and driver allocations may share the same
class. VMA counts are constant throughout each process's final 100 cycles;
growth uses existing mappings, not an accumulating VMA count.

Concrete counterexamples and immediate preceding-sample allocator deltas:

| Process / cycle | HWM increment KiB | Mapped delta bytes | In-use delta bytes | Free-retained delta bytes |
| --- | ---: | ---: | ---: | ---: |
| 01 / 420 | 640 | 655,360 | −11,584 | 666,944 |
| 02 / 423 | 636 | 651,264 | −21,024 | 672,288 |
| 03 / 463 | 4 | 4,096 | 8,048 | −3,952 |
| 06 / 482 | 172 | 176,128 | 8,672 | 167,456 |
| 08 / 459 | 24 | 24,576 | 11,456 | 13,120 |

For process 01/c420 the anonymous writable VMA and XML heap 7 both grow
655,360 bytes (7,061,504→7,716,864). In-use falls: this event has concrete arena
retention evidence, not merely "probably allocator". It follows an injected
failure at the existing recovery point. Process 02's final event is a healthy
PQ→SDR P010 job. Others include healthy PQ/Single-AAC jobs. Late HWMs span
Single-AAC, Dual-AAC, PQ→SDR and recovery boundaries in every process: **there is
no unique transition explaining all growth**, so the conditional 200-repeat
targeted-transition test was not triggered. No extra GPU diagnostic was run.

In-use accounting is not perfectly constant. For process 00, the same normal
macro position rises 17,456 bytes between cycles 50 and 300; cycles
350/400/450/500 are 10,852,624 / 10,854,176 / 10,852,416 / 10,853,584 bytes.
Thus the earlier strict staircase does not continue at that boundary, but this
does not prove application-live allocation stability. Final-100 per-job in-use
ranges across the ten processes span 10,828,480–10,890,016 bytes; those are
observations, **not acceptance bounds**. Capacity retention and residence of
previously reserved pages explain concrete events; neither complete library
ownership nor a production leak has been demonstrated.

### Lifecycle, regressions and static verification

All 18,025 jobs have nonempty resource accounting, positive Vulkan binding peaks,
zero final tracked active counts and zero known active bytes, joined workers,
and removed per-job output/staging workspaces. Tracked DMA-BUF imports and
VAAPI frame refs return to zero. Every memory boundary maintains FD **7** and
threads **2**, matching its initial baseline (the additional HWM log explains
one observer FD relative to the old six-FD harness). Four reference outputs
remain fixed at 928,880 bytes. Their input/output identities, plans and decoded
oracles exactly match the previous four configurations. Each process completes
healthy work after its final fault; post-failure arena retention still exists,
including process 01/c420, so resource recovery is not a memory-plateau waiver.

Queue telemetry is a last operation-boundary sample and may be nonzero or null.
Worker joins and channel-owner teardown establish lifecycle release; **there is
no fabricated cleanup-time queue-depth-zero measurement**. Driver-internal
allocation counts remain unknown. Zero `/dev/dri`-classified cleanup mappings
does not exclude driver-owned anonymous memory.

The frozen 448-file source identity remains unchanged through all native runs,
clean builds and final regressions. Both loaded driver-file hashes match in all
ten processes: Intel Arc Meteor Lake, iHD 26.1.5 and ANV 26.2.3; FFmpeg 8.1.3,
VAAPI 1.23.0 and default glibc 2.43. The observer executable SHA-256 is
`6d40cd42946581a189caa0a83231eb693a5e9126ae4304e301707e4a4f82964f`.
No production resource lifetime, media policy, allocator or H.264 oracle changed.

| Gate | Current result |
| --- | --- |
| 17 retained paths ×3 | **PASS**, all 51 whole files also exactly match previous same-driver files |
| Canonical H.264 controls | **PASS**, existing media semantic oracle and attested production pair; no oracle change |
| Production corpus | **PASS**, 27 unique gates / 48 overlapping surface entries; six expected planner rejections exit 1 |
| Mux determinism | **PASS**, 100 strict arrival replays and 30,084-packet stress; fixed-file hashes independently rechecked |
| Independent clean default | **PASS**, Release build; Release/debug workspace tests (256 passed each), strict all-target clippy, fmt |
| Independent clean measurement | **PASS**, Release build; Release/debug workspace tests (259 passed each), strict all-target clippy, fmt |
| Emitted SPIR-V | **PASS**, all 300 distinct physical files / 25 unique hashes individually Vulkan 1.3 validated after regressions |
| Native observer tests / clippy | **PASS**, three ordinary tests and strict Release all-target native clippy; GPU ignored test executed separately above |
| Python/static checks | **PASS**, soak/corpus unit tests, formatting and diff checks; detailed counts in final receipt |

The 102 ignored tests per clean profile are not counted as native qualification.
No full portability matrix was rerun for this observation-only change. Prior
H.264 cross-driver closure and validation-enabled lifecycle evidence remain
separate historical evidence; current same-driver controls pass. Full logs,
commands, source identities and control traces are retained in the
[control archive manifest](../tests/soak/evidence/memory-control-manifest.json).
Bulky reference arrays/videos remain in the frozen target directories; their
output identities are preserved in the mechanical evidence record.

### Classification and sealing decision

**Memory classification: Unresolved. Stage 5.4D-1A: NOT SEALED.** The five
500-cycle counterexamples fail the all-process requirement. Concrete allocator
retention does not satisfy the missing operational window and does not justify
campaign classification as `AllocatorRetention` or `BoundedProcessCache`.
There is also insufficient application-live ownership evidence to claim
`RealLeak`. No new MiB threshold, slope tolerance, allocation purge or longer
cap was used to force a pass.

The historical +69,648 KiB RSS alarm violation and cycle-48 +652 KiB remain
FAIL/Unresolved evidence, respectively. The old 64 MiB line is a coarse RSS
diagnostic, not a demonstrated process-memory hard bound; its original assertion
is unchanged. Neither historical event is relabeled PASS. The original cycle-48
allocation cannot be recovered from rollup-only evidence.

The allowed statement is limited to **five recorded processes reaching an
operational plateau under this finite workload, with balanced tracked resources
in all ten**. It is not valid to claim the default workload universally plateaus,
that memory will never exceed an absolute MiB value, or that every native-library
allocation is balanced. Next work must attribute the late capacity/residence and
in-use drift (including observer effects) before a root-cause fix/new default
qualification. Do not replace this failed campaign with selected good runs.

Stage 5.4D-1 remains **NOT SEALED**. D-1B is **not justified**; D-1B, D-2 and
formal 100k soak remain **NOT STARTED**.

## Historical A/B final closure result — 2026-10-08

The following is the preserved **historical 50-cycle A/B result**, not the
extended experiment above.

Status is **NOT SEALED**. All ten normal-production RSS runs have completed;
the remaining gate is A14: residual private-anonymous growth attribution and a
reproducible plateau without continuing growth. H.264 blocker B is closed under
the separate exact Tier 1B-P contract. The retained pack, complete build suites
and current SPIR-V sweep have passed. The new
[A/B receipt](../tests/soak/stage54d1a-final-closure.json) records current evidence
without replacing the historical failures below. No 100k soak, D-1B or D-2
has been started.

The three H.264 cases now have actual encoder-boundary captures under both
attested iHD versions. NV12 planes, frame PTS/duration/metadata, before/after
encoder contexts and all AVOptions match. Their only changed NAL is the
106-byte identifier SEI in AU 0, NAL 2; its first changed byte is offset 96.
The payload type is 5, UUID `59948b2811ec45af967519d41feaa94d`, with the exact
`Lavc62.28.103 / VAAPI 1.23.0` prefix, attested iHD version and trailing bytes.
All SPS/PPS/VUI, VCL/slice/POC syntax, unknown SEI and other coded bytes remain
exact. This byte proof is stronger than decoded-pixel similarity alone.

Only the separate Tier 1B-P now recognizes this exact semantic class without
fixture geometry/audio restrictions. Same-stack and historical oracles are
unchanged: raw Tier 1A and strict Tier 1B still **FAIL** cross-driver. Six new
outputs also match their preserved same-driver outputs byte-for-byte; no
baseline was promoted. Audio on/off controls preserve actual encoder inputs
and complete H.264 elementary streams under each driver. The B-frame fixture
has reordered **input**; production output remains zero-B-frame encode, not a
new qualification of output B-frame encoding. Complete NAL inventories are
[retained separately](../tests/soak/evidence/h264-ab-nal-inventories.json).

The old 65,536 KiB line is explicitly an `alarm_threshold_kib` /
`growth_alarm_threshold_kib` review diagnostic in `rss_trend`, not a sealed
maximum-resident-memory contract (see the
[actual `rss_trend` implementation](../apps/asciiflow-cli/src/native_reliability_tests.rs)
and the [unsealed original receipt](../tests/soak/evidence/stage54d1a-resource-failure-historical.json)).
The path-specific Git history search has no introduction of this untracked
candidate, and the preserved reports do not claim a sealed RSS bound. The old
candidate remains explicitly `NOT SEALED`. Its old ignored test still asserts the alarm
and its +69,648 KiB **FAIL is retained**. There is no new absolute threshold.
The fixed observer retains four references (928,880 bytes), streams and drops
job/sample records, and requires nonempty resource accounting. Required
Vulkan-Validation 50-cycle evidence is kept separate: validation-layer residency
is not normal-production RSS. Final RSS qualification uses the default allocator
without allocator tunables or validation layers, with repeated complete cycles
and independent fresh processes. Final classification is **Unresolved**;
a short flat segment alone is not a PASS.

### Default-production RSS decision

All ten consecutive declared processes completed 50 cycles each, without
result-dependent reruns: 2,000 successful jobs, 20 controlled cancellations and
20 injected encoder failures. The latter remain simulated test-hook failures,
not actual device/encoder failures. Every job has nonempty accounting, positive
Vulkan peaks, zero tracked active counts and zero known active bytes after
cleanup, FD 6→6 and threads 2→2. Each process completes 40 healthy jobs after its
final failure; all successful outputs match the four fixed references. Unknown
driver/native counts remain unknown. Last-boundary queue gauges can be nonzero
or unavailable after cancellation: owner teardown and worker joins prove release,
not a fabricated cleanup-time depth-zero sample.

The complete [510-sample record](../tests/soak/evidence/ab-default-rss-cycles.json)
preserves every smaps/status field and four equal late-window slopes/spans.
Values below are KiB; Δ columns are Anonymous changes, also exactly the RSS
changes in these windows. Private_Dirty equals Anonymous throughout.

| Process | RSS at 50 | Anonymous at 50 | Δ10–20 | Δ20–30 | Δ30–40 | Δ40–50 |
|---|---:|---:|---:|---:|---:|---:|
| 00 | 214024 | 133512 | 124 | 100 | 76 | 8 |
| 01 | 214632 | 134320 | 172 | 52 | 316 | 656 |
| 02 | 213860 | 133516 | 88 | 52 | 272 | 28 |
| 03 | 238368 | 158336 | 108 | 596 | 48 | 16 |
| 04 | 239004 | 159000 | 712 | 676 | 28 | 12 |
| 05 | 238544 | 158380 | 124 | 20 | 632 | 20 |
| 06 | 238376 | 158020 | 116 | 228 | 36 | 0 |
| 07 | 246988 | 166904 | 96 | 152 | 16 | 0 |
| 08 | 246120 | 165992 | 44 | 56 | 4 | 32 |
| 09 | 238532 | 158176 | 144 | 108 | 28 | 0 |

Seven final windows retain positive growth. Process 01 adds 652 KiB at healthy
cycle 48, not at a scheduled fault, with only two subsequent measured cycles.
Its final-window OLS slope is 71.636 KiB/cycle, versus 42.182 in the preceding
window. The other flatter endpoints do not erase this retained counterexample.
Swap is zero throughout; PSS/shared/file-backed accounting is preserved separately
and cannot explain away the observed private-anonymous increment.

This supports reproducible ownership cleanup and predominantly early retained
memory growth, but does **not** establish `BoundedProcessCache`, `AllocatorRetention`
or `DriverRuntimeCache`. It also does not prove `RealLeak`: rollup accounting and
the historical partial allocator diagnostic do not identify the remaining live
versus retained allocation source. Fresh-process footprint dispersion is not a
late-window tolerance. No new absolute threshold, allocator tuning or trimming
is used to pass the gate; the historical +69,648 KiB FAIL remains unchanged.

Remaining work is to attribute these residual private-anonymous steps and
demonstrate reproducible post-warmup/post-fault stability with the default
allocator. The finite 50-cycle observation is not 100k qualification. D-1 remains
NOT SEALED; D-1B is not justified by this receipt and has not started.

The final immutable-source regression pack passed all 17 retained paths × three
runs (51 outputs), with all 51 whole files identical to their preserved same-stack
counterparts. The summary's 48 verification-surface entries overlap: there are
27 unique gate records, not 48 independent tests. Six negative planner controls
correctly exit 1. See the [gate receipt](../tests/soak/evidence/ab-canonical-regression.json)
and [output identities](../tests/soak/evidence/ab-retained-output-identities.json).
Both independent clean-origin targets passed default/measurement Release and
debug workspace tests and strict all-target clippy; formatting passed. Original
parallel builds ultimately returned 0; a separate lock-wait retry interrupted
with 130 is preserved, not counted as PASS. The
[build receipt](../tests/soak/evidence/ab-clean-builds.json) records both executable
hash epochs rather than conflating build and test relinking.

The current [SPIR-V inventory](../tests/soak/evidence/ab-final-spirv.json) validates
all 300 physical project-generated files in the two clean targets and current
native/retained target, representing 25 distinct byte identities, for Vulkan 1.3.
This does not transfer a historical shader count to the current checkout.
[Focused checks](../tests/soak/evidence/ab-focused-static.json) also preserve
non-vacuous resource accounting, H.264 positive/negative contracts, and separately
label 244 required-Validation lifecycle jobs. There were no validation diagnostics.

The unchanged C-1A controls passed 100 arrival-perturbed canonical replays,
30,084-packet stress and strict media comparisons. An initial wrong-fixture
attempt is retained: the helper's fixed three-second repetition offset overlaps
that unrelated clip's timestamps, so its stress is not a valid C-1A control.
The correct preserved canonical capture passed without a source/test waiver.
See the [mux receipt](../tests/soak/evidence/ab-c1a-mux.json).

The normal-RSS command is the ignored **native test executable**, not the CLI:

```sh
env -u ASCIIFLOW_VULKAN_VALIDATION -u ASCIIFLOW_REQUIRE_VULKAN_VALIDATION \
  -u VK_INSTANCE_LAYERS ASCIIFLOW_D1A_NATIVE_GPU=1 ASCIIFLOW_C4B_PRODUCTION=1 \
  ASCIIFLOW_NATIVE_RSS_MODE=repeated-cycles ASCIIFLOW_ATTEST_NATIVE_MAPS=1 \
  ASCIIFLOW_NATIVE_RELIABILITY_EVIDENCE_DIR=/absolute/new-evidence-directory \
  /absolute/qualified-native-test-executable --ignored --exact \
  native_reliability_tests::default_allocator_fixed_mixed_cycles_observation \
  --test-threads=1
```

Use a new evidence directory per independent process. Allocator-tuning variables
are rejected, not silently cleared. The
[exact ten-process runner](../tests/soak/evidence/ab-default-rss-runner.py.txt)
is retained with its fixed pre-run protocol; run it from the frozen checkout
with the recorded observer executable and retained prerequisites. Root report
and evidence updates are separate from the qualified runtime source map.

## Historical native closure pass — 2026-10-07

Status at this historical pass was **NOT SEALED**. The separate
[native closure receipt](../tests/soak/stage54d1a-native-closure.json) records the
frozen checkout, executable/test hashes and current results. The previous-pass
receipt and the report below are historical evidence, not overwritten results.
No D-1B, D-2 or formal 100k workload was started.

Cancellation and final rename now share a short arbitration gate. Cancellation
admitted before rename prevents commit; rename admitted first completes before
a later cancellation request. No native work, joins or test callbacks execute
under that gate. Post-commit cancellation leaves the valid target and success
state intact. Staging cleanup has one owner and reports non-NotFound errors as
secondary warnings instead of hiding them or replacing the primary cause.
This is local atomic visibility, not power-loss durability or a bounded deadline
for a stalled remote filesystem rename.

The default-disabled `native-reliability` feature supplies event-controlled
qualification boundaries. The final frozen-source run passed 36 real SIGINT
cases: twelve phases repeated three times, including actual decoded-frame
delivery, submitted Vulkan work, successful encoder frame submission, occupied
mux queue, audio/video DTS ordering, actual EOF, finalization, worker shutdown
and both commit orderings. The handler acknowledges cancellation before the
controller releases the phase gate. All 33 pre-commit cases exited 130 with the
sentinel intact; all three post-commit cases exited 0 with complete decodable
output. FD returned 4→4, positive Vulkan-binding observations were required,
and every tracked active resource returned to zero. Child validation logs were
checked. The original five-second audio/SIGINT watchdog also passed unchanged
in both new serialized workspace suites; its historical FAIL is retained.

Actual non-root permission controls (uid 1000) cover staging creation, an
unwritable output directory and denied rename. Native GPU finalization followed
by denied rename retains the commit root, reports staging-removal and diagnostic
write failures, preserves the sentinel and releases FD/resources. A staging
file cannot be removed from the deliberately unwritable parent: this is reported
truthfully, then manually recovered after restoring permissions. Encoder/mux
qualification errors combined with that actual cleanup denial retain their
primary errors; the injected errors are **SimulatedPass**, not native encoder
or device failures.

Eight actual AVIO callback controls cover ENOSPC/EIO at header, packet write,
intermediate flush and trailer. They retain the callback boundary, accepted
bytes, native errno and surfaced stage, with byte/payload/duration/PTS/DTS-exact
healthy recovery. These remain **SimulatedOnly**. Four event-held native AVIO
slow consumers passed: sustained writes, bursts, flush and finalization.
The first three witness both mux channels full at 16, blocking producers and
exact 374-packet readback; finalization correctly has empty queues. A separate
full-GPU mux hold witnesses both pipeline queues full at 3 before release.
Queue peaks are observed at successful enqueue/dequeue boundaries, not periodic
sample inference and not an atomic exact high-water mark. `/dev/full` forwards
the real native write callback and returns kernel ENOSPC without injection.
It is a safe write-error integration control, **not** quota/filesystem exhaustion;
that stronger filesystem claim remains **UnavailableSafely**.

The same-process production-entry driver completed 20×4 mixed GPU jobs, 15
recovery jobs and a 120-frame slow-sink job. All 96 resource maps were nonempty,
Vulkan-binding peaks positive (18–32), active counts zero after cleanup and FD
4→4. NV12/P010, SDR/PQ/pixel conversion, Builtin/FreeType, mono/color and
none/single/dual AAC transitions passed their software decode/metadata oracles;
91 successful outputs establish 49 first references and 42 repeated-configuration
byte comparisons, with zero variances. Worker panic, mux panic, encoder error
and mux error plus cancellation retained their primary roots, followed by
successful healthy jobs. Those injected causes are explicitly test-hook causes.

**The same-process RSS gate failed.** After the original one-sequence warmup,
sequence medians span 189,380–259,028 KiB: 69,648 KiB exceeds the unchanged
65,536 KiB review alarm. The curve steps and then plateaus; this is not evidence
of continuous per-job leakage, but it is not an automatic PASS. The driver
intentionally retains first-output byte references for 48 configurations and
structured records, which also confound RSS interpretation.

Two unchanged-binary diagnostics narrow the cause without replacing that FAIL:
`MALLOC_ARENA_MAX=1` passes the same assertions with a 19,508 KiB range; a
default-allocator exit-only probe reproduces the alarm and reports 173,891,584
arena bytes, 663,168 allocated bytes and 173,228,416 free bytes after test-record
teardown. `malloc_trim(0)` reduces RSS from 272,842,752 to 225,005,568 bytes.
This supports freed allocator retention, but does not account for all resident
native/driver memory. See [glibc allocation tunables](https://sourceware.org/glibc/manual/latest/html_node/Memory-Allocation-Tunables.html).
Default same-process RSS remains **Unresolved**, not relabeled PASS and not
fixed by changing the alarm threshold or silently requiring allocator tuning.

The final frozen-source dual-AAC run passed 10,000 full-GPU frames with required
Vulkan Validation, complete decode-back, BT.709 limited metadata and two audio
streams of 9,376 packets each. Payload/size/duration/PTS/DTS and audio metadata
are strict. FD returned 4→4 and tracked resources to zero. The unmodified trend
analyzer reports WarmupThenStable / ShortStabilityOnly, not long-run proof.

Both independent offline clean Release builds (default and measurement) passed
serialized workspace tests, strict all-target clippy, formatting and immutable
445-file source checks. The initial 42-file shader census accidentally used an
`ascii_*` scope; it is retained as incomplete history. Final supplements validate
**all 50 emitted files / 25 logical kernels per target** for Vulkan 1.3, including
the non-ASCII-prefix modules. Build and test relinking produced different
executables; both hashes are recorded rather than conflated. See the
[clean-build receipt](../tests/soak/evidence/native-clean-builds.json).

The frozen-source canonical regression passed all **17 retained paths × three
runs**, using unchanged historical hashes/oracles: 51 retained outputs and 48
PASS gate entries. Exact commands, input identities, output hashes and results
are in the [canonical receipt](../tests/soak/evidence/native-canonical-regression.json).
The final post-test shader sweep validates 1,556 emitted files (34 distinct byte
identities), including all 200 frozen-snapshot files, for Vulkan 1.3. Historical
build outputs are included in this inventory, not claimed as current kernels.
See the [final shader receipt](../tests/soak/evidence/native-final-spirv.json).

The alternate iHD 25.4.6 stack and canonical iHD 26.1.5 stack were captured
against the same executable and unchanged 422-file portability source guard;
the full qualification registry remains 445 files. Both core subsets pass.
Five previously skipped real-media paths now pass per-stack runtime/decode-back,
including single/dual AAC and B-frames. Both PQ static/non-static outputs are
byte-identical across stacks. The 64×64 PQ case correctly rejects on both and
remains Unqualified, not PASS; the missing-range construction limitation and
the nine historical Unqualified contracts are preserved.

**The alternate cross-stack comparison failed** for the single-AAC, dual-AAC
and B-frame H.264 fixtures. The actual tool classifications remain Regression:
Tier 1A, 1B and 1B-P FAIL, although decoded pixels (1C) and structure (2) PASS.
At that historical pass, the narrowly qualified 1B-P exception required exactly one video stream,
1920×1080 at 50 fps and 300 access units. It cannot admit these audio or other
geometry/rate profiles. This is an unresolved cross-driver qualification gap,
not observed decoded-pixel corruption. No stream stripping, exception widening
or oracle weakening was performed. The runner stopped at that comparison and
did not emit an aggregate PASS. All three remaining alternate native fault
gates were then run independently and passed their declared bounded/simulated
scopes; they do not replace the comparison FAIL. See the
[portability receipt](../tests/soak/evidence/native-portability.json).

The remaining sealing gates are **default same-process RSS review** and the
**strict alternate H.264 comparison**. The
[same-process receipt](../tests/soak/evidence/native-same-process.json) preserves
the default failure and both allocator diagnostics. All 445 frozen source files
still match after testing; all 387 qualified candidate files outside the final
documentation/evidence updates match the frozen source. Stage 5.4D-1A remains
**NOT SEALED**. Stage 5.4D-1 remains NOT SEALED; D-1B/D-2 are not started.

## Preserved previous-pass report

The sections below retain the earlier observations and open-gate ledger at that
time. Current native results above supersede only their stated execution scope;
historical failures, unavailable metrics and long-soak limits remain unchanged.

## Observation implementation and controls

`reliability-measurement` is disabled by default. A new
`ASCIIFLOW_RELIABILITY_REPORT` path enables streamed JSONL observations in a
measurement build. Mutex-protected counters do not participate in media
decisions. There is no background unbounded log queue. The session encloses
native initialization, joined workers and cleanup, including failure paths.
Counter release follows native release; abandonment stays active/error.
Resource counts describe successful owned bindings/references, not all driver
or allocator internals. Default frame layouts and per-frame hooks are unchanged.

Concurrent counter tests check 4,000 completions, count/byte balance, idempotent
release, previous-generation isolation and unavailable byte totals. The test
uses an isolated subprocess so unrelated parallel pipeline tests cannot add
frames to its process-scoped session. CLI controls verify identical software
MP4 bytes with/without an active report, initial/final FD equality, retained
failure observations and destination preservation. Report/output lexical and
symlink-parent aliases reject before creating either file. Independent review
found and prompted this alias fix; a mixed known/unknown byte history now
remains unavailable rather than fabricating a complete total.

## Three 10k Release / required-Validation production paths

Intel Arc Meteor Lake, fixed FFmpeg/ffprobe 8.1.3; 1920×1080, 50 fps, 10,000
frames per path. All use VAAPI decode, NV12/P010 input interop, Vulkan processing,
output interop and VAAPI encode; audio disabled. Each output passed complete
decode-back with `-xerror`, frame count, pixel format, color metadata and every
packet PTS/DTS exactly `frame_index/50`. No VUID/sync-hazard diagnostics appeared.

| Path | Short resource classification | RSS initial / steady median / p95 / cleanup KiB | FD |
|---|---|---|---|
| H.264 SDR → H.264 SDR | WarmupThenStable | 43,696 / 132,252 / 132,392 / 111,012 | 4 → 4 |
| HEVC PQ → HEVC Main10 PQ | Stable | 43,800 / 168,220 / 168,336 / 146,916 | 4 → 4 |
| AV1 legal PQ → HEVC Main10 SDR | Stable | 43,644 / 167,200 / 167,372 / 145,952 | 4 → 4 |

These classifications mean **ShortStabilityOnly**, not zero RSS growth or
long-run qualification. Raw early/middle/late slopes and samples are retained.
Positive late RSS slopes (approximately 0.076 and 0.101 KiB/frame in the PQ
paths) are visible, not suppressed or automatically labeled a leak. They
remain a reason to perform D-1B later, not proof of a driver leak.

| Owned metric | SDR peak | PQ preserve peak | PQ→SDR peak | Cleanup |
|---|---|---|---|---|
| Vulkan buffer bindings / allocation bytes | 18 / 25,164,928 | 18 / 50,278,528 | 32 / 162,253,192 | all 0 |
| DMA-BUF imported plane bindings / allocation bytes | 8 / 20,889,600 | 8 / 41,779,200 | 4 / 20,889,600 | all 0 |
| Exported decoder AVFrame refs | 7 | 7 | 7 | all 0 |
| Acquired encoder AVFrame refs | 7 | 5 | 4 | all 0 |

Driver GPU bytes/surface-pool counts, scratch/pre-bind allocations, codec-held
surface refs, queue payload bytes and FFmpeg internal pending memory are
Unavailable. Shared imports are binding bytes, not deduplicated physical GPU
memory. Queue capacities are normally 3 for pipeline queues and 16 for each
mux producer. Sampled pipeline peaks reached capacity 3; exact atomic peaks
are not claimed. Latest queue values are not cleanup-time depths. Audio-free
10k runs are not audio-copy resource qualification. Mux reports successful
writes/bytes since explicit flush, not true internal pending bytes.

The 10k executable and source snapshot are explicitly retained in its
environment receipt. These runs preceded the final report-path alias and
mixed-byte-availability guard fixes; production acquisition/release hooks and
media logic were unchanged by those guards. Do not transfer exact-build
attestation to another executable or imply a final-build 10k rerun.

## Failure, cancellation and mux characterization

The new synchronized core test runs 20 held-backend and 20 held-encoder
cancellations. Capacity-one queue saturation is proven by upstream signals;
there are no sleeps as phase oracles. Cancellation returns within a bounded
watchdog, skips finalization and drops all three mock owners. This is not a
20-phase native GPU cancellation matrix or slow filesystem proof.

The repeated CLI suite completes 100 successes and 50 expected failures:
17 staging-parent errors, 17 atomic-rename errors and 16 actual RLIMIT_FSIZE
EFBIG errors. Each failure immediately receives a byte-exact healthy retry;
sentinels remain intact, zero staging leaks. It is a CPU/software subprocess
campaign, not same-process GPU state-contamination qualification.

Native AVIO ENOSPC/EIO callbacks execute through real MP4 mux finalization,
after a successfully written/flushed header. Each error reaches the callback,
preserves the native errno and structured Finalization root, restores FD 4→4
and permits byte-exact healthy recovery. Classification: **SimulatedOnly**.
This is not real isolated disk exhaustion or header/packet ENOSPC/EIO coverage.

Native mux stress preflights two safe timestamp-shifted cycles, then admits
1,000,076 packets across 2,674 dual-AAC cycles. Per-stream counts
`[240660, 379708, 379708]` match independent readback; every integer PTS/DTS
matches and DTS is strictly increasing per stream. The strengthened readback
also checks every compressed payload, size and duration. No per-packet trace
was enabled. Runtime 5.49 s; output 214,794,080 bytes; FD 4→5 while captured→4
after capture teardown. Independent per-stream cycles use exact DTS spans
`46080/15360`, `145408/48000`, `145024/48000` seconds; this is mux stress,
not synchronized repeating-media/A/V parity. It is distinct from the preserved
1,000 dual/sparse replay job-count evidence and does not claim a million
packets for each audio shape.

The earlier common-five-second-gap stimulus passed counts and PTS/DTS but
failed the newly added strict-duration check: video packet 89 had expected
duration 512 versus native MP4 readback 31232. MP4 bridged the imposed gap.
That failure is retained in the receipt, not relabeled PASS. The new stimulus
removes the gap per stream; the strict duration/payload oracle was preserved.

Native PQ image-create/import/submit/completed-fence fault recovery and VAAPI
header/early/mid-packet/trailer fault tests were rerun successfully. Their
injection points are safe diagnostic checkpoints, not actual driver failure
or an uncompleted-fence reuse claim. Further campaign executions and exact
regression/build results are recorded in the receipt, not inferred here.

## Historical failures retained and sealing boundary

Canonical production regression passed all 27 explicit suite gates, including
17 retained output paths × three runs (51 retained MP4 SHA-256 identities in
the receipt), C1/C2B CPU references, C3/C4A GPU references, H.264 semantic and
same-build controls. Historical H.264 and Tier1B-P policy/oracles are unchanged.

The B quick real-media corpus records classification PASS 66, SKIPPED 7 and
UNQUALIFIED 9, with 42 decode-back passes and no FAILED classification. Skipped
hardware and existing strict audio/timeline limitations are not promoted to
PASS. The alternate iHD 25.4.6 manifest was captured, but activation stopped
before smoke execution: `stack source identity changed; recapture both stacks
after fixes`. Source/evidence editing was ongoing during capture; the guard
failure is observed, not a driver failure. No guard was bypassed. Alternate regression remains
unresolved in this pass; prior alternate-stack evidence is not new attestation.

Independent clean default and measurement Release workspace builds passed;
each emitted 25 modules validated for Vulkan 1.3. The current default clean
build predates the final measurement-only guard edit (excluded by its feature
set); the final measurement build compiled after it. Exact binary/source hashes
and superseded builds are retained in `tests/soak/d1a-clean-builds.json`, without
asserting that different build snapshots have the same binary identity.

Default and measurement workspace tests passed serialized, as did strict
workspace clippy, formatting/diff checks and 17 Python runner/analyzer tests.
The final review replay also passed 1,000,076 packets after adding nonnegative
size/non-null payload safety assertions. One manual invocation omitted its
required output-directory environment variable and failed before initializing
the workload; the corrected invocation passed. This harness error is separate
from the preserved strict-duration failure above.

One concurrent default workspace run failed the existing SIGINT test's
five-second packet-writing-entry watchdog. Its isolated rerun passed in
0.45 s, and a serialized full workspace run passed. The failure is retained;
load sensitivity is an inference, not a proven root cause or a waived test.
No timeout was extended and no media oracle was loosened.

The cache inventory distinction is documented in the reliability contract:
25 required module names (+1 optional FP64), versus historical cached-copy
counts of 1,003/1,203. The preserved historical module list establishes the
exact difference: four isolated C2 clean targets contributed 200 copies outside
the later two-root census. Those files remain present, unchanged. Current full
target census is 1,278 files (1,203 historical + 75 new cached copies), not
1,278 distinct required modules. Clean-build/SPIR-V records are separate evidence.

Open D-1A gates include the complete phase-targeted native cancellation /
commit-race and slow-I/O backpressure matrix; header/packet/flush/trailer
ENOSPC/EIO phase coverage; explicit permission/primary-plus-cleanup cases;
and mixed codec/audio/font jobs with same-process GPU contamination checks.
Current-source alternate-stack execution also remains unresolved.
Sampled queue peaks/unknown bytes must not be promoted to exact memory peaks.
The observed 10k curves and successful scoped tests do not close these gaps.

Stage 5.4D-1 additionally requires the future formal D-1B long-soak campaign
and post-soak canonical regression. Neither D-1B nor D-2 is started or justified
by this partial D-1A result. Allowed claim: the listed workloads/checkpoints
passed with retained observations. No general uptime or failure-matrix
completeness claim is made.

## Closure ledger (45 requested report items)

| # | Item | Observed result / boundary |
|---|---|---|
| 1 | Resource ownership | Ownership/acquire/release/abandonment map in reliability-testing.md; counters cover explicitly named bindings/references, not all driver objects. |
| 2 | Hard bounds | Decoded/processed queues 3 each; mux video/audio 16 each; individual audio packet guard 16 MiB. Capacity is not an exact total-memory bound. |
| 3 | Mux flush contract | Explicit flush at 64 written packets or 8 MiB written bytes, plus final EOF; not queue occupancy. |
| 4 | FFmpeg internal cache | Pending packet/byte census unavailable; written-since-flush counters are not internal pending memory. |
| 5 | Instrumentation | Opt-in reliability-measurement; default build compiles hooks out; streaming JSONL. |
| 6 | Instrument validation | Concurrent accounting, session generations, unknown bytes, alias rejection, write failure and active/inactive byte parity tested. |
| 7 | SDR 10k | PASS full GPU Release + Validation; 10000 frames/packets, decode-back/color/timing correct. |
| 8 | PQ preserve 10k | PASS full GPU Release + Validation; 10000 frames/packets, decode-back/color/timing correct. |
| 9 | PQ→SDR 10k | PASS full GPU Release + Validation; 10000 frames/packets, decode-back/color/timing correct. |
| 10 | RSS curves | SDR WarmupThenStable; PQ paths Stable; ShortStabilityOnly, not long-term leak proof. Three-window slopes retained. |
| 11 | FD lifecycle | All 10k initial/final FD counts 4→4; native replay 4→5 capture→4; fault checkpoints restore FD. |
| 12 | GPU binding lifecycle | Tracked Vulkan/DMA-BUF bindings return to zero; driver allocation totals unavailable. |
| 13 | VAAPI lifecycle | Tracked exported decoder/acquired encoder frame references return to zero; driver surfaces/bytes unavailable. |
| 14 | Video queue peaks | Sampled decoded 3/3/3, processed 3/2/1; mux video 1/1/1; these are sampled, not exact high-water. |
| 15 | Audio queue peaks | Sampled zero in audio-off 10k runs; not audio-under-load proof. |
| 16 | Mux pending peaks | Unavailable; 157 explicit flushes per 10k path; cannot infer pending-memory peak. |
| 17 | Slow producer/consumer | Synchronized held mock backend/sink tests pass; full native slow-I/O matrix open. |
| 18 | Backpressure | Capacity-one saturation proven by signals; native audio-enqueue mux cancellation passes; full matrix open. |
| 19 | Panic root | Core worker/primary-root and borrowed interop panic campaign passes; existing root precedence unchanged. |
| 20 | Cancellation matrix | 40 synchronized mock cancellations + 3 native mux scenarios + 2 native PQ CLI cases pass; required complete native phase matrix open. |
| 21 | Cancel/commit race | Both controlled orderings not yet qualified; OPEN. |
| 22 | ENOSPC | Actual native AVIO callback errno -28 at finalization; SimulatedOnly, not disk exhaustion/full phase coverage. |
| 23 | EIO | Actual native AVIO callback errno -5 at finalization; SimulatedOnly, full phase coverage open. |
| 24 | Permission failure | Explicit permission fault qualification remains OPEN; staging-parent errors are not permission evidence. |
| 25 | Staging cleanup | Zero leaks in 150-job software CLI suite, destinations preserved. |
| 26 | Commit atomicity | 17 failed rename cases preserve sentinel destination; cancellation/commit race remains open. |
| 27 | Failure matrix | 11 bounded campaign gates PASS; image/import/submit/completed-fence and mux header/packet/trailer checkpoints covered, not complete matrix. |
| 28 | 100 success jobs | PASS software/CPU subprocess jobs. |
| 29 | 50 failure jobs | PASS expected 17 staging-parent + 17 rename + 16 actual EFBIG failures. |
| 30 | Mixed jobs | Codec/audio/font same-process GPU contamination matrix OPEN. |
| 31 | Post-failure recovery | 50 immediate byte-exact software retries and native scoped fault retries pass; no general mixed-GPU claim. |
| 32 | Malformed input | Missing-input diagnostic/sentinel/staging test passes; corpus results scoped separately, not universal malformed-input coverage. |
| 33 | Parser safety | Existing corpus/static controls retained; no new parser-safety proof inferred from 10k runs. |
| 34 | Mux replay resources | 1000076 native packets; exact payload/size/duration/PTS/DTS; 5.49s; FD restored. No million-packet RSS/driver-memory curve claim. |
| 35 | SPIR-V discrepancy | 1203 versus 1003 = 200 cached copies in four isolated clean targets, still present unchanged; current full target 1278 validated Vulkan 1.3. |
| 36 | 17×3 regressions | PASS 17 retained production paths, three runs each; canonical suite 27 explicit gates PASS. |
| 37 | Compatibility | Current B corpus results recorded separately; SKIPPED/UNQUALIFIED are not PASS. |
| 38 | Portability | Canonical qualification PASS; alternate stack scoped result recorded separately; no historical attestation transfer. |
| 39 | H264/Tier1B-P | Canonical H264 semantic and same-build controls PASS; historical policy/oracles unchanged; alternate results separately scoped. |
| 40 | Clean builds | Independent isolated default/measurement Release builds and emitted-module validation; exact final snapshot in d1a-clean-builds.json. |
| 41 | Static checks | Default and measurement serialized workspace tests/clippy PASS; fmt/diff and 17 Python tests PASS. Initial failures retained. |
| 42 | Unresolved observations | Unknown driver/internal-memory metrics; sampled peaks; final-build 10k not rerun; complete native matrices missing. |
| 43 | Allowed reliability claim | Only retained workloads/checkpoints passed; no general uptime, long-soak, exhaustive failure or exact-total-memory claim. |
| 44 | Stage 5.4D-1A | NOT SEALED. |
| 45 | Stage 5.4D-1 remaining | Complete D1A native matrices, then separately authorized D1B 3×100k/long trends/post-soak regression. D1B/D2 NOT STARTED. |
