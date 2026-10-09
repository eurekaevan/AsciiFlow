# Stage 5.4D-1B — finite long-soak qualification

**Status: SEALED.** Stage 5.4D-1A, D-1B and D-1 overall are SEALED for the
recorded single-job CLI workloads. Final audio attribution: 2026-10-09;
original qualification: 2026-10-08. Stage 5.4D-2 Release Qualification is now
justified, but has not started. Persistent multi-job memory remains unqualified.

## Final audio-memory attribution — 2026-10-09

**Classification: `BoundedAudioPacketAllocatorRetention`.** The matched
controls, native last-reference census and post-teardown reclaim together
attribute the remaining audio-enabled working set to released-but-retained
allocator pages with bounded live audio ownership. This is a pragmatic causal
classification for the recorded single-job workload, not a byte-perfect heap
model or a proof of persistent-process RSS plateau. No production leak/fix is
claimed and no arbitrary MiB threshold was added.

### Experiment A: actual full GPU C + captured original AAC

The real AV1 PQ VAAPI decode → Vulkan HDR-aware ASCII/BT.2446 → P010 SDR →
HEVC10 VAAPI encode chain, dual AAC streams, sender, bounded channels, mux owner,
interleaving and ordinary MP4/movflags0 were retained. Only live audio input
reading was replaced by an immutable capture of the **original input fixture's**
187,502 AAC packets. No AAC was regenerated or re-encoded. The capture preserves
packet payload/side data, timestamps, duration, flags and time base using native
packet references; production input descriptors still supply stream parameters,
language, title and disposition. Capture and input close complete before the
measured interval; original buffers stay fixed until replay finishes.

| Experiment | Written-video endpoints | RSS / Anonymous delta KiB | Known index delta KiB | Residual KiB | Conclusion |
| --- | --- | ---: | ---: | ---: | --- |
| Historical full C, dual AAC | 50,000→99,940 | 18,456 | 10,152 | 8,304 | Preserved original observation |
| A: full GPU + replayed original AAC | 50,000→99,940 | 18,464 | 10,152 | 8,312 | A2: native live audio input is not necessary for growth |
| Narrow native payload census, live audio C | 50,000→99,940 | 18,520 | 10,152 | 8,368 | Comparable residual; live ownership remains bounded |

PSS deltas are18,464 KiB for A and18,521 KiB for the census. Full output SHA-256
for **both** controls equals the retained production C output:
`3d9f4a69f6f930e4155238dc72080464f1096829b0a2176053da6bd31f4dfe4d`.
Independent comparison also verifies all287,502 packets' payload hashes, sizes,
PTS/DTS/durations/flags and all probed stream fields. Thus video, AAC and mux
structure have not been weakened to make the experiment pass. A completed100k
frames in469.285991248 seconds; the census completed100k in465.113985462 seconds.
Exact CLIs, input SHA, qualification binary SHAs and packet probes are retained.

**Experiment B was not run:** A2 explicitly selects the ownership-interaction
branch. No alternate demux loop, preserve differential, >100k run or codec/
container matrix was introduced. The input remains ordinary MOV/MP4, actual
demuxer `mov,mp4,m4a,3gp,3g2,mj2`, linked package
`ffmpeg-libs-8.1.3-1.fc44.x86_64`, libavformat62.12.103. The earlier native
input/CBS audit found no justified new progressive input-index model; A2 makes
ongoing native audio input unnecessary to reproduce this residual.

### Last-native-reference census, not cumulative-packet counting

The qualification-only probe transfers each selected input packet's existing
buffer reference into an AVBuffer free-callback wrapper, without copying its
payload. Native move/ref operations retain that wrapper. Counters decrement only
when the **last wrapped native reference** releases, including references held
inside the interleaver. A separate native unit proves a cloned packet keeps the
census live after the first packet drops and that the final drop releases the
original reference exactly once. The readonly diagnostic wrapper adds allocation
overhead; matching outputs and the comparable residual above control its effect.

| Producer media checkpoint, video-frame equivalent | AAC1/AAC2 current packets | Current payload bytes | Lifetime peak packets | Lifetime peak payload bytes | Audio queue observed peak / capacity |
| ---: | --- | --- | --- | --- | ---: |
| 10k | 9 / 9 | 3,091 / 3,061 | 10 / 10 | 3,482 / 3,488 | 16 / 16 |
| 25k | 9 / 9 | 3,088 / 3,069 | 10 / 10 | 3,482 / 3,488 | 16 / 16 |
| 50k | 9 / 9 | 3,091 / 3,061 | 10 / 10 | 3,482 / 3,488 | 16 / 16 |
| 75k | 9 / 9 | 3,088 / 3,073 | 10 / 10 | 3,482 / 3,488 | 16 / 16 |
| ~99,940 | 9 / 9 | 3,036 / 3,065 | 10 / 10 | 3,482 / 3,488 | 16 / 16 |
| Joined job teardown | **0 / 0** | **0 / 0** | 10 / 10 | 3,482 / 3,488 | 16 / 16 |

Each track reads and successfully submits exactly93,751 packets to
`av_interleaved_write_frame`; final read/consumed counts match and no accounting
error occurs. Submission is not immediate physical-write progress. The table's
media checkpoints use raw AAC PTS on this zero-origin50fps fixture; external
written-video checkpoints in the experiment table are separate observations.
Peaks update on every wrapped packet/release; they are not merely five sampled
queue lengths. Post-enqueue channel observations can miss a transient peak, but
16 was observed and structural capacity16 is unchanged. Total input side data
is only10 bytes per track (20 total), not O(frames) side-data growth; this is a
cumulative side-data count, not an independent native side-data lifetime census.

Project audio bookkeeping is per-stream descriptors/routes, one pending retry
message and one mux input head, with no packet/timestamp history. Exact package
source has the matching interleaver allocation/pop/free chain:
`mux.c:845,856,1013` → `packet.c:595–605`; queued cleanup unrefs/frees at
`packet.c:609–619`, normal write unrefs at `mux.c:1087–1088` and trailer drains
at1255. Rust `Packet::drop` calls `av_packet_free`. These inspected paths plus
the last-reference census argue against packet or packet-list-header retention.
The probe itself uses fixed two-track arrays/scalars and streamed checkpoints,
not per-frame history.

### Close / teardown / one diagnostic trim

| Phase | RSS KiB | PSS KiB | Anonymous / Private Dirty KiB | Live payloads AAC1/AAC2 |
| --- | ---: | ---: | ---: | --- |
| Before input close | 196,364 | 171,120 | 137,200 | 8 / 9 |
| After input close | 196,364 | 171,120 | 137,200 | 7 / 8 |
| After joined job teardown | 190,024 | 166,936 | 136,564 | **0 / 0** |
| After one diagnostic `malloc_trim(0)` | 122,320 | 99,232 | 68,860 | **0 / 0** |

Input close has **0 KiB** observable delta; its samples overlap final mux work,
so they are not an exclusive input-allocation census. After job teardown, all
tracked audio buffers are logically released. One diagnosis-only trim then
reclaims **67,704 KiB** in each listed metric. It is not in default production
code and is not a new runtime policy. The trim is whole-process: video, MOV
index and other runtime free pages may contribute. **Do not subtract67,704 KiB
from8,304 KiB or claim every reclaimed byte was audio.**

The causal conclusion uses the whole evidence chain: identical-packet mux-only
does not reproduce the extra component; audio-off removes it; replacing live
input with captured AAC retains it; wrapped live ownership/bytes do not grow;
packet/list free paths are explicit; EOF releases all tracked buffers; free
process pages remain and are reclaimable. This supports allocator retention
associated with the live producer→mux allocation/release interaction, not an
unexplained increasing live audio object set. Numerical residuals stay exactly
as observed; they are now classified, not relabeled as0 KiB. No per-allocation
tagging of every historical residual byte is claimed. Pre-existing aliases or
copied native buffers are not a complete heap census; the inspected move/ref
paths and exact output controls delimit this qualification.

### Closure, verification and retained files

No default production Rust/C FFI behavior changed and no production fix was
needed. Changes are guarded by the existing `mux-qualification` feature and
explicit diagnostic environment settings. New internal qualification `State`
stores the fixed census; `Payload` carries the original buffer reference to its
C free callback. The qualification finish hook allows one sample/trim only after
successful joined job teardown and preserves normal failure/cancellation roots.
No codec, container, shader, allocator or production architecture changed.

Verification: explicit native last-reference unit PASS; media Release library
29 PASS/24 ignored (new payload unit separately executed); CLI qualification
units31 PASS/3 ignored; media and CLI qualification clippy, default CLI check,
format and diff checks PASS. Both100k controls are whole-file identical to
retained C. Existing A/B/C100k decode-back, audio/metadata/Tier evidence,
51k cancellation/failure, alternate10k,17×3, compatibility, portability and
static/SPIR-V remain valid; no production fix required rerunning them.

New `tests/soak/stage54d1b-audio-memory-attribution.json` retains commands,
identities, packet/ownership/close/trim results and the causal limitations.
The corresponding evidence archive retains raw observations, probes, executed
qualification source, reviewer/source material and reproducible control scripts;
its manifest verifies every member. They are linked from the primary long-soak
receipt. Large media/executables remain ignored. No old evidence or historical
FAIL was deleted or rewritten. The earlier sections below are dated history.

**Stage5.4D-1B = SEALED; Stage5.4D-1 = SEALED.** Exact reliability claim:
“Qualified for the recorded single-job CLI workloads and long-run production
paths.” Persistent multi-job memory retention remains unqualified; this does
not guarantee24/7 operation or full driver-internal resource visibility.
Stage5.4D-2 Release Qualification is now justified. It has not started.

## Historical Path C layer-isolation decision — earlier 2026-10-09

**Outcome U: the residual is associated with the audio-enabled upstream input
path or its native allocation interaction with mux, not reproduced by mux-only
replay or the identical audio-off video chain. Its specific cause remains
unidentified. D-1B and D-1 remain NOT SEALED.** This narrows the current
production gate; it does not relabel unexplained growth as a proven bound.

| Experiment | Actual written-video endpoints | Observed delta KiB | Known index delta KiB | Residual KiB | Layer attribution / conclusion |
| --- | --- | ---: | ---: | ---: | --- |
| Preserved full C, dual AAC, default Release | 50,000→99,940 | 18,456 | 10,152 | 8,304 | Original production residual remains unaccounted |
| Mux-only actual C packets | 49,974→99,900 | 10,100 | 10,152 | −52 | Required MOV index explains the recorded late interval; original residual not reproduced |
| Full GPU C, audio none | 50,000→99,938 | 3,552 | 3,528 | +24 | Identical video chain does not reproduce original residual |
| Preserve vs tone-map | Not run | — | — | — | Conditional prerequisite absent after audio-off result; no video-mode matrix |

RSS, PSS and `/proc` Anonymous each have the same respective interval deltas
above. Anonymous is not an exclusive Private Anonymous allocation census.
Endpoints are externally correlated written-packet observations, not atomic
native allocation snapshots. Index capacities use actual packet counts, not
duration estimates; no requirement of byte-exact residency/model equality is
introduced.

### Exact mux-only control

The ignored Release qualification test `d1b_path_c_mux_memory_replay` captures
the real production C output's **100,000 HEVC10 + 93,751 AAC1 + 93,751 AAC2**
packets before measurement. The immutable capture stays fixed during replay.
It preserves payload, PTS, DTS, duration, flags, time bases, codec parameters,
stream metadata and dispositions, and feeds the existing deterministic mux
owner with channels16 and the existing64-packet/8MiB flush trigger. There is
no live demux/decode/Vulkan/VAAPI encode/audio producer during measurement.
The same linked libavformat62.12.103, ordinary MP4/MOV and movflags0 are used.
At the two measured endpoints the video entries are49,974/99,900 and each
AAC track has46,852/93,658 entries. Rounded index capacities give the same
10,152KiB late delta as the original C observations.

The final replay is **whole-file byte-identical** to production C:
`3d9f4a69f6f930e4155238dc72080464f1096829b0a2176053da6bd31f4dfe4d`.
All packet fields and probed stream fields independently match; FD4→4.
The first setup trial failed matched-metadata qualification because MOV demux
exports a track name as `name`, while MOV mux consumes `title`. A test-only
adapter fixed that round trip; the initial trial is preserved and excluded,
not rewritten as PASS. A subsequent offline review corrected mismatched
probe options (`count_packets`), without changing media or weakening the
oracle. No extra MOV structure was found or needed to explain replay growth;
the conditional second-order MOV investigation was therefore not entered.

Preloading all packets raises replay's fixed initial working set and changes
native allocation scheduling. Thus this excludes a standalone reproducible
mux residual in this control, not every possible audio/mux runtime interaction.

### Audio-off control and measurement audit

The unchanged default Release production binary, SHA-256
`92fc20d079c8067872e88ba6f1d67949e952f630b07d1ac7f6441245f831fe02`,
processed the same canonical AV1 PQ input through actual VAAPI decode,
Vulkan HDR-aware ASCII/BT.2446 and HEVC10 VAAPI encode to ordinary MP4.
Only audio policy changed from dual AAC copy to `none`; exact CLI, input/build
identities and live loaded ANV/iHD/libavformat identities are in the receipt.
Conversion completed100,000 frames in467.305281048 seconds. Every encoded
video packet's payload hash, size, flags, PTS, DTS and duration matches full C;
the complete probed video stream, including metadata and codec parameters,
also matches. This is packet identity evidence, not a newly run decode-back.

| Audio-off written frames | RSS KiB | PSS KiB | Anonymous KiB | Index KiB | RSS minus index KiB |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 10,000 | 142,804 | 117,304 | 83,384 | 720 | 142,084 |
| 25,025 | 143,872 | 118,368 | 84,452 | 1,800 | 142,072 |
| 50,000 | 145,628 | 120,128 | 86,208 | 3,528 | 142,100 |
| 75,007 | 147,428 | 121,928 | 88,008 | 5,328 | 142,100 |
| 99,938 | 149,180 | 123,680 | 89,760 | 7,056 | 142,124 |

The approximately8,280KiB residual difference (8,304−24) is associated with
audio enablement; it is **not** a measured AAC allocation census or proof of
an AAC packet leak. The implicated scope is the independent audio input
traversal/copy path, native demux/parser state or its allocation interaction
with mux. `AudioReader` opens/probes a second AVFormatContext and traverses
the entire input without per-stream discard configuration, dropping unselected
AV1 packets after native `av_read_frame` returns. Rust ownership review found
no growing packet history or missing packet/context release: channel16,
one retry message, one mux head per input, packet RAII, context close and worker
join remain intact. Static review does not establish which native state grows.

Measurement state uses fixed-category maps, errors capped32, replacement-only
mux state, scalar aggregates and streamed JSONL, not per-frame retained history.
GPU timing storage is bounded by outstanding slots; the geometry/format cache
does not grow for this fixed fixture. The default build compiles reliability
hooks out, and the original external-only default C repeat already reproduced
the residual. **QualificationInstrumentationGrowth is not supported.** A further
production-vs-measurement control is unnecessary; no allocator forensic or
Vulkan investigation was added.

### Decision and evidence preservation

The mux-only late interval is explained required container growth. Audio-off
has small checkpoint noise, not the full C residual. Neither result establishes
`BoundedRuntimeWorkingSet` for audio-enabled C. Its classification remains
**UnexplainedResidualGrowth, narrowed to audio-enabled upstream/native input
state or its mux interaction**. The remaining production gate is identifying
and accounting for that state, or minimally fixing an identified lifecycle bug
and verifying the affected path. No leak or fix is claimed. D-2 is not justified
and has not started.

Only test Rust changed: the ignored real-packet replay plus stronger flag
comparison in the existing replay oracle. No production Rust behavior, shader,
container policy or frozen production binary changed. Verification: explicit
100k-packet replay PASS; media Release qualification library29passed/23ignored
(the new ignored replay separately executed); media qualification clippy with
warnings denied PASS; formatting and diff checks PASS. Existing A/B/C100k,
51k cancellation/failure, alternate10k,17×3, compatibility, portability and
static/SPIR-V qualification remain valid and were not pointlessly repeated.

New retained artifacts are `tests/soak/evidence/d1b-layer-isolation.json`
(experiment identities, commands, results and scoped audit),
`d1b-layer-isolation-evidence.tar.gz` (non-media raw observations, packet probes,
review scripts and replay test source), and its manifest (member hashes).
The primary `tests/soak/stage54d1b-long-soak.json` links their identities.
Large media and executables stay ignored. Earlier historical failures, D-1A
scope correction, packet/index attribution and evidence archives are preserved.
The older sections below describe the qualification and preceding attribution
passes; this section is the current layer-isolation decision.

## Final execution status

All three 100k measurement Release paths have completed conversion,
decode-back, and AAC checks. The original runner's A/C packet-count
assumptions were wrong; those checks remain **FAILED** in the evidence and are
not rewritten. Independent review passed the artifact and lifecycle checks;
that review passes the recorded ownership/lifecycle checks, not the memory gate.
All three default Release 100k conversions, complete decode-back, timing,
color and audio checks also pass. Their outputs match the measurement outputs.

The C 100k path continues to show Anonymous/RSS growth. A necessary 72-byte
MP4 MOVI entry per packet is confirmed; after accounting for the logical model,
about 15 MiB of growth from 10k to 100k remains unassigned. This observation
does not establish a leak or a memory bound. The default Release C repeat
confirms this is not merely measurement instrumentation. Both 51k controls,
alternate 10k smoke, post-soak regressions and static gates are complete.
The subsequent exact-package, capacity-rounded attribution below supersedes
the earlier unrounded logical model for the remaining-memory decision:
**8.11 MiB** residual in the default repeat, not zero. D-1B remains NOT SEALED.

## Path C memory attribution — exact package review, 2026-10-09

The question in this pass is only whether ordinary MOV/MP4 sample-index state
explains the remaining growth. **It explains part, but not all.** Classification
is **B: `UnexplainedResidualGrowth`**. The required index component is
`ExpectedContainerIndexGrowth`; it is not a leak. This does not establish that
the residual is a leak, nor identify its cause. No allocator tuning, production
container/HDR/shader change, new threshold or longer soak was introduced.

### Actual muxer and exact linked FFmpeg implementation

Path C selects the **mp4 muxer, implemented by MOV**, not another container.
The actual artifact probes as `mov,mp4,m4a,3gp,3g2,mj2`, with top-level
`ftyp/free/mdat/moov`, no `moof`, and `moov` after `mdat`. Production passes
no header option dictionary or private `movflags` setter. This package's
`movflags` default is **0**: ordinary **non-fragmented**, no faststart and no
hybrid-fragmented mode. Final atom structure is consistent with that selection.

This review uses the actual RPM Fusion package source, not merely a same-version
upstream tag:

- Linked package: `ffmpeg-libs-8.1.3-1.fc44.x86_64`; native libavformat
  **62.12.103**, SHA-256
  `68196d2f4f8127d324764e9061c91eb578313caab1c92a016d4225615030d1c4`,
  matching the recorded soak stack and current runtime library.
- Matching [signed source RPM](https://download1.rpmfusion.org/free/fedora/updates/44/SRPMS/f/ffmpeg-8.1.3-1.fc44.src.rpm):
  `ffmpeg-8.1.3-1.fc44.src.rpm`, SHA-256
  `285fe17863d31e2bec872e4a53456560867c716645fd44c4660bb1fa2ec968e9`.
  RPM signature/digests verified; all three declared patches applied; none
  changes `movenc.c/h`. Source, spec, patches and native configuration identities
  are retained in the attribution evidence. Package verification found ownership/
  group differences, not content-digest differences; nothing was installed.
- Fresh exact-source native x86_64 ABI check: **`sizeof(MOVIentry)=72`**.
  `MOV_INDEX_CLUSTER_SIZE=1024`. `MOVTrack.entry` advances per written packet;
  at capacity, `av_realloc_array` grows `cluster` to `entry+1024`
  (`movenc.c:7039–7047`, increment at 7162).
- Non-fragmented `cluster` remains available for final `stco/co64`, `stsz`,
  `stsc`, `stts`, and applicable `ctts/stss` tables. `av_write_trailer()` writes
  `moov`, then calls `deinit_muxer` (`mux.c:1267`) → `mov_free`, which frees
  both cluster pointers (`movenc.c:7731–7732`). Failed-job initialized-context
  destruction also deinitializes it. These are required lifetime-owned indexes.
- `cluster_written` is allocated only inside the
  `FF_MOV_FLAG_HYBRID_FRAGMENTED` branch (`movenc.c:6472–6492`). It is **not**
  a second ordinary-MP4 index and contributes **zero** here. Temporary
  `stts/ctts` arrays exist while writing final tables, not as an extra index
  growing throughout these pre-trailer samples. They cannot be counted twice
  to explain the residual.

### Real entries, per-track capacities and expected growth

Counts come from preserved **actual packet positions/sizes**, including AAC
priming packets: the output prefix through the 50,000th video packet and the
complete 100,000-frame output. They are not estimated from media duration.
These HEVC/AAC packets each contribute one index entry; 1024 PCM samples per
AAC packet do not mean 1024 MOV entries.

| C track | Entries @50k | Entries @100k | Capacity @50k | Capacity @100k | Cluster bytes @50k | Cluster bytes @100k | Delta |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Video | 50,000 | 100,000 | 50,176 | 100,352 | 3,612,672 | 7,225,344 | 3,612,672 |
| AAC 1 | 46,876 | 93,751 | 47,104 | 94,208 | 3,391,488 | 6,782,976 | 3,391,488 |
| AAC 2 | 46,876 | 93,751 | 47,104 | 94,208 | 3,391,488 | 6,782,976 | 3,391,488 |
| Total | 143,752 | 287,502 | 144,384 | 288,768 | 10,395,648 | 20,791,296 | **10,395,648** |

Exact required cluster-byte model:
`sum_tracks(ceil(packet_entries/1024) × 1024 × 72)`.
The expected 50k→100k delta is **10,152 KiB (9.914 MiB)**; audio contributes
**6,624 KiB**, video **3,528 KiB**. This is required allocation capacity,
not an exact resident-page census or a total-runtime-memory bound.

The measurement report's API-accepted counts at those frame milestones are
143,746/287,496, slightly behind the physical video-boundary/final prefixes.
Independent ±64 packet-prefix sensitivity checks produce the same rounded
per-track capacities at both endpoints. This is a sensitivity check, not a
claim that a trace or report directly reads native `MOVTrack` state atomically.

### Observed growth and residual

| C evidence | Endpoints | Observed increase KiB | Expected cluster increase KiB | Residual KiB |
| --- | --- | ---: | ---: | ---: |
| Measurement RSS | 50k→100k progress | 18,480 | 10,152 | **8,328 (8.133 MiB)** |
| Default repeat RSS | 50,000→99,940 written frames | 18,456 | 10,152 | **8,304 (8.109 MiB)** |
| Default repeat PSS | Same external samples | 18,456 | 10,152 | **8,304** |
| Default repeat Anonymous | Same external samples | 18,456 | 10,152 | **8,304** |

The default repeat's last sample contains 99,940 video and 93,695 packets per
AAC track. Each rounded capacity is already identical to its final 100k value;
the model delta therefore does not change. File size is correlated to actual
packet-end offsets; bounded queues/AVIO buffering mean written-frame and
processed-frame timestamps are not atomic. `Anonymous` is still not an
exclusive Private Anonymous allocation census. These caveats do not provide
evidence attributing the approximately 8.1 MiB residual to a second MOV index.

### A / B / C comparison using existing evidence only

| Path | Compared frames | Required index growth KiB | Observed RSS growth KiB | Residual KiB |
| --- | --- | ---: | ---: | ---: |
| A, one AAC | 50k→99k | 6,696 | 6,472 | −224 |
| B, no AAC | 50k→100k | 3,528 | 3,436 | −92 |
| C, dual AAC | 50k→100k | 10,152 | 18,480 | **8,328** |

A's 100k report overlaps teardown, so its last pre-teardown 99k sample is
used, not a misleading negative cleanup delta. A/B are consistent with the
required index model at this observational resolution. The ordering C>A>B
matches the number of retained sample entries, but C's magnitude does not:
the second AAC index does not explain the additional residual. Small negative
differences are not proof of exact allocation-to-RSS attribution.

### Classification, evidence and unchanged gates

No audio-off matched run or fragmented-output experiment was necessary: exact
package structure, real traces and the existing default repeat already answer
the stated hypothesis. No new media run, 200k/500k test, allocator deep dive,
malloc tuning or production code change was performed. Existing 17×3,
H.264, mux determinism, lifecycle and other passed gates remain valid under
their recorded source/stack identities; they were not rerun just for this review.

The memory requirement is explicitly:

> No unexplained frame-count-correlated memory growth beyond container/runtime
> structures whose required growth has been explicitly accounted for.

Ordinary non-fragmented MP4 retains per-sample index state until finalization;
memory can grow with output sample/packet count. Neither constant-memory
muxing nor memory independent of duration is claimed. Known index growth is
not a blocker, but the significant C residual is still unqualified. No claim
that it is a leak, or that an allocator/cache explains it, is made.

Machine attribution: [`d1b-mov-attribution.json`](../tests/soak/evidence/d1b-mov-attribution.json).
The separate small attribution archive retains exact native source excerpts,
spec/patch provenance and the calculation script; the prior 707-member archive
and historical failures remain unchanged. No new production type/API/dependency
was added; new files are evidence JSON, its native-source archive and manifest,
each required to retain this package-specific analysis reproducibly.

**D-1B = NOT SEALED; D-1 overall = NOT SEALED.** The only remaining gate is
the current single-job C residual, not RSS plateau or the persistent-process
observation. **D-2 is not justified and was not started.**

## Scope and decision boundary

Run three actual Release hardware paths at 100,000 frames each, first with the
reliability-measurement binary and then with the unchanged clean default Release
binary, reusing the measurement run's hash-verified inputs:

| Path | Workload | Audio |
| --- | --- | --- |
| A — SDR | SDR H.264 → H.264, NV12 interop | Single AAC track |
| B — PQ preserve | Legal PQ HEVC → HEVC Main10 PQ, P010 interop | None |
| C — PQ to SDR | Legal PQ AV1 → HEVC Main10 SDR, P010 interop and Vulkan tone mapping | Dual AAC tracks |

Use the existing three 10k Validation runs as the Validation evidence; the
100k production paths run without Vulkan Validation. The single/none/dual
audio checks on the measurement outputs are complete, subject to the retained
A/C packet-count failures above. This campaign is scoped to one CLI process
per media job, followed by process exit. Persistent
multi-job allocator/runtime retention remains an unqualified
`NonBlockingOutOfScopeObservation` from D-1A; it is not a D-1B pass condition.

The campaign has no absolute MiB memory gate. Preserve and review resource
observations without inferring a total-memory bound. Require FD count after
cleanup to equal its initial count, all tracked resource counts to return to
zero, and report queue capacities 3 and 16 separately from observed peaks.
Mux flush thresholds remain 64 written packets or 8 MiB written bytes; these
are flush triggers, not a memory bound. Unknown driver/internal memory and
sampled queue values remain unknown or sampled.

## Six milestones

1. **Freeze inputs and controls.** Record the clean default and measurement
   Release binary identities and reuse the existing three 10k Validation
   results. Confirm the checked-in legal-PQ sources and generated inputs by
   their recorded identities.
2. **Measurement long run.** Completed for all three 100k paths. Review the
   retained frame-cadence resources and process samples; memory classification
   remains unresolved.
3. **Default long run.** Completed all three 100k paths with the clean default
   Release binary, using `--inputs-from` the measurement output directory.
4. **Decode, audio, and failure controls.** Verify complete decode-back,
   metadata, packet/frame counts and audio behavior. Cover single AAC, no
   audio, and dual AAC on the corresponding 100k paths. Run SIGINT
   cancellation and mux-write failure after more than 50,000 observed frames;
   the latter uses kernel `RLIMIT_FSIZE` and verifies EFBIG cleanup. This is a
   filesystem write failure, not a device-failure claim.
5. **Alternate-stack check.** Complete the alternate-stack 10k comparison
   within its declared support and oracle scope; record unsupported or
   unqualified cases as such.
6. **Post-soak regression and review.** Run the retained 17×3 regression,
   core suite and required subset after the long runs. Review every receipt,
   output identity, failure, and gate before making a D-1B decision. Do not
   start D-2 as part of this work.

## Commands and output roots

Run the measurement campaign first:

```sh
python3 -B tests/soak/long-run.py --measurement \
  --binary target/stage54d1a-memory-v2/target/clean-measurement/release/asciiflow \
  --output target/stage54d1b-long-soak/measurement-v1
```

Then run the default production campaign against those exact inputs:

```sh
python3 -B tests/soak/long-run.py \
  --inputs-from target/stage54d1b-long-soak/measurement-v1 \
  --binary target/stage54d1a-memory-v2/target/clean-default/release/asciiflow \
  --output target/stage54d1b-long-soak/production-v1
```

For both controls, use the clean measurement binary and a legal-PQ input from
the measurement output. Give each action a fresh output directory:

```sh
python3 -B tests/soak/long-controls.py --action cancel \
  --binary target/stage54d1a-memory-v2/target/clean-measurement/release/asciiflow \
  --source target/stage54d1b-long-soak/measurement-v1/source-pq-to-sdr/input-aac.mp4 \
  --output target/stage54d1b-long-soak/long-cancel
python3 -B tests/soak/long-controls.py --action write-failure \
  --binary target/stage54d1a-memory-v2/target/clean-measurement/release/asciiflow \
  --source target/stage54d1b-long-soak/measurement-v1/source-pq-to-sdr/input-aac.mp4 \
  --output target/stage54d1b-long-soak/long-write-failure
```

The controls trigger only after the report observes at least 51,000 completed
frames. SIGINT and `RLIMIT_FSIZE` EFBIG must preserve the expected error,
remove staging, avoid committing an incomplete target, and return FDs and
tracked resources to their required post-cleanup state.

## Definition of done

D-1B remains **NOT SEALED** until all six milestones are complete and their
actual evidence is reviewed. Each required path and control must satisfy its
declared output, decode, metadata/audio, cancellation/failure, FD, tracked
resource, and cleanup checks. Any failure, skip, unsupported alternate-stack
case, missing evidence, or unverified requirement stays visible as such; a
zero runner exit or flat short memory window alone does not seal the stage.
The 500k/audio extension and D-2 are outside this scope.

## Recorded production evidence — 2026-10-08

The canonical machine receipt is
[`stage54d1b-long-soak.json`](../tests/soak/stage54d1b-long-soak.json).
Conversion/output success is separate from the unresolved memory qualification.
No production Rust, codec, shader, allocator, planner, container or HDR feature
was changed in this pass.

### Identity and reconstruction

Canonical hardware is Intel Arc Meteor Lake, ANV 26.2.3 / iHD 26.1.5,
FFmpeg/ffprobe **8.1.3**, `ffmpeg-8.1.3-1.fc44.x86_64` and
`ffmpeg-libs-8.1.3-1.fc44.x86_64`. Complete build configurations, tool hashes,
kernel/package identities and native loaded-library attestations are retained.
Default Release SHA-256 is
`92fc20d079c8067872e88ba6f1d67949e952f630b07d1ac7f6441245f831fe02`;
measurement Release is
`b23d695d578a4da7a816648f3c13f6bfe2e28012a6a8ade25653146db71717b8`.
All 132 runtime/build source identities still match the qualified clean builds.

The checked-in `tests/soak/generate.py` materializes finite 100k MP4 inputs
using fixed FFmpeg 8.1.3 and checksum-verified source recipes. A uses the
algorithmic 300-frame SDR baseline; B/C use **C3 legal-domain PQ <=1000 cd/m²**
HEVC/AV1 fixtures, not the historical B-3 4000/10000-nit sources. Compressed
packet repetition happens during fixture construction, never by rewinding a
production demuxer. A/C add deterministic 440/660 Hz, 48 kHz stereo AAC,
128 kbit/s, with explicit language/title/disposition and bitexact settings.
The commands above invoke this exact recipe; each `source-*/identity.json`
retains every resolved FFmpeg argv, generator SHA, tool version/configuration,
tool SHA, source SHA, output SHA/size, frame rate and color semantics.
Large generated media remain under ignored `target/`, not in checked-in evidence.

| Path | Input bytes | Input SHA-256 |
| --- | ---: | --- |
| A, single AAC | 6,591,288,176 | `003472712461b548853fc294e4503c7d7c0fa040dc18560190268af1e83e120c` |
| B, no audio | 641,354,407 | `d1146254ce1658a7469c93533cb16e8164e08ef49b7a9727f484013bcfc24c26` |
| C, dual AAC | 2,899,946,247 | `38d24a0360d3bbfba5710e5cc85823a8bdad01c1ca065b2386dbd81261cae6b6` |

All paths fix 1920×1080, 50 fps, width 80, standard charset, builtin-8x8 font,
true color, VAAPI decode/encode, Vulkan GPU mapping, and both interops **on**.
The selected plans confirm GPU-resident processing, no hardware download/upload,
capacity-three pipeline queues and two Vulkan frame slots. Full conversion argv
vectors are preserved in the receipt/archive. Their common form is:

```sh
"$BIN" "$INPUT" "$OUTPUT" \
  --width 80 --charset standard --font builtin-8x8 --color true \
  --audio "$AUDIO" --decode vaapi --backend vulkan --vulkan-mapping gpu \
  --encode vaapi --hw-device /dev/dri/renderD128 \
  --vaapi-vulkan-input-interop on --vaapi-vulkan-output-interop on \
  --output-codec "$CODEC" --output-bit-depth "$DEPTH" \
  --output-dynamic-range "$DYNAMIC_RANGE" --no-progress \
  --diagnostic-report "$DIAGNOSTIC"
```

A fixes `copy/h264/8/preserve`, B `none/hevc/10/preserve`, C
`copy/hevc/10/sdr` for AUDIO/CODEC/DEPTH/DYNAMIC_RANGE respectively. Source
codec/bit depth/dynamic range and selected `Sdr`, `HdrPqPreserve`, or
`HdrPqToSdrBt709` processing are independently checked, not inferred from
output metadata alone.

### Frames, wall time, output and audio

Every primary output contains **100,000 frames / 2,000 s (33:20)**. Wall times
below are conversion only, excluding generation, probing and decode-back.

| Path | Measurement wall s | Default Release wall s | Default full decode s | Output semantics |
| --- | ---: | ---: | ---: | --- |
| A | 190.912 | 203.442 | 83.232 | H.264, 8-bit yuv420p, BT.709/BT.709/BT.709, limited |
| B | 326.894 | 384.468 | 232.915 | HEVC Main 10, yuv420p10le, BT.2020/PQ/BT.2020nc, limited |
| C | 463.667 | 811.686 | 275.629 | HEVC Main 10, yuv420p10le, BT.709/BT.709/BT.709 SDR, limited |

All three measurement outputs and all three default outputs were fully decoded
with FFmpeg `-xerror -map 0`. Default decode progress ends at 100k; the earlier
measurement probe independently decoded/counts all 100k frames in addition to
its full decode. Every video packet has PTS=DTS=i/50, duration=1/50, and the
stream duration is exactly 2,000 seconds. All outputs remain 1920×1080.

| Path | Output SHA-256, identical between measurement/default builds |
| --- | --- |
| A | `92383dd193791e65a3640396dc4eb81019185d40dacbbb3a090813c09c7a6ec7` |
| B | `d5efd71ed4939927696fe165900583cd3f06d942aad7e4cb3a3f5ad5899dce2c` |
| C | `3d9f4a69f6f930e4155238dc72080464f1096829b0a2176053da6bd31f4dfe4d` |

A copies 93,751 AAC packets; C copies 93,751 per track. Payload SHA, size,
PTS/DTS/duration, codec/layout/rate, language, disposition and all stream tags
match exactly. Source track titles are actually probed as `tags.name`:
`Soak track 1/2`; languages are eng/jpn and defaults are 1/0. B has no audio.

C stream and first-frame probes show no mastering-display/content-light or
other HDR side data. B/C long inputs themselves carry no static HDR payload,
so this is **not** a new positive test of stripping a metadata-bearing source.
The post-soak retained C oracle also checks all decoded frames and elementary
stream metadata under its existing scope. No stronger stripping claim is made.

### Resource time series

Measurement reports retain initial, first-frame, every-1000-frame,
pre-finalization and post-cleanup observations. External PSS/Anonymous samples
are correlated to the latest flushed frame count, not atomic measurements.
`Anonymous` means `smaps_rollup.Anonymous`, not an exclusive private-page census.

RSS, KiB:

| Path | Initial | 10k | 25k | 50k | 75k | 100k | Sampled peak | Post-cleanup |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| A | 43,536 | 143,296 | 145,084 | 154,152 | 154,160 | 150,796 | 160,624 | 150,796 |
| B | 43,740 | 161,912 | 163,036 | 164,796 | 166,560 | 168,232 | 168,232 | 158,456 |
| C | 43,776 | 192,708 | 199,024 | 208,436 | 218,616 | 226,916 | 226,940 | 217,088 |

The A 100k snapshot overlaps cleanup; late external/99k observations are also
preserved. Sampled peaks above are report-row peaks, not exact transient HWM.
Initial external PSS is unavailable at the exact report boundary; late external
observations nearest 100k are at 99k and are labeled as such in the machine data.

Nearest PSS / Anonymous, KiB:

| Path | 10k | 25k | 50k | 75k | Near 100k |
| --- | --- | --- | --- | --- | --- |
| A | 92,153 / 57,732 | 93,920 / 59,500 | 103,002 / 68,580 | 102,997 / 68,580 | 108,808 / 75,068 |
| B | 109,747 / 77,296 | 110,867 / 78,416 | 112,766 / 80,200 | 114,511 / 81,944 | 116,199 / 83,632 |
| C | 139,799 / 107,032 | 146,123 / 113,356 | 155,528 / 122,760 | 165,734 / 132,964 | 174,023 / 141,264 |

The fixed FFmpeg n8.1.3 MOV muxer grows one per-track sample index in 1024-entry
increments and frees it during teardown. A local ABI check measures
`sizeof(MOVIentry)=72`. See the retained source/compile identities and the
[fixed upstream source](https://raw.githubusercontent.com/FFmpeg/FFmpeg/n8.1.3/libavformat/movenc.c).
This is necessary length-dependent container bookkeeping, **not** an exact
resident-memory model or a total-memory bound. No RSS/MiB sealing threshold is
introduced.

B's 10k→100k RSS increase closely follows that logical index model. C still
has about **15.64 MiB** additional 10k→100k growth after the model, including
about **8.18 MiB** over 50k→100k. Its final 93k→100k RSS window is flat, but
that alone does not establish the cause or a production memory bound. A
three-track FFmpeg packet-copy remux reproduces part of the growth, not all of
the C residual. A suspected measurement-only effect was **not established**:
the default primary C run also reached a similar final memory range.
The sole representative C repeat adds external RSS/PSS/Anonymous samples,
correlated after completion to actual packet-end offsets in the identical MP4;
the repeat is complete and byte-identical to the fully decoded primary output.
Its near-10k/25k/50k/75k/100k observations (actual written frames
10018/25021/50000/75003/99940) have RSS
159072/165280/174728/184904/193184 KiB, PSS
131585/137783/147241/157417/165697 KiB and Anonymous
99352/105560/115008/125184/133464 KiB. The first process-start sample is
8 KiB, before native initialization, not a comparable job-initial measurement.
After the logical index model, the default repeat still adds **15.56 MiB**
from near-10k to its last sample, including **8.16 MiB** from 50k to the
last sample. Short late flattening does not qualify the preceding sustained
trend. This is the **only remaining D-1B gate**, within the current single-job
production scope, not a reopened persistent-process plateau requirement.
No ownership leak or specific cache/allocator cause has been established.

### FD, ownership, queues and mux

All three return FD **4→4** before process exit. Sampled FD peaks are A=24,
B=28, C=30. This includes the measurement report descriptor consistently at
both boundaries; enumeration's transient descriptor is excluded.

| Tracked owner | A peak→final | B peak→final | C peak→final |
| --- | --- | --- | --- |
| Vulkan buffer bindings | 18→0 | 18→0 | 32→0 |
| DMA-BUF imported plane bindings | 8→0 | 8→0 | 4→0 |
| VAAPI decoder exported-frame refs | 7→0 | 7→0 | 7→0 |
| VAAPI encoder acquired-frame refs | 7→0 | 7→0 | 7→0 |

Known tracked bytes also return to zero, with no accounting errors. These
counters do not claim a census of every driver-internal allocation/cache.
Both pipeline queue observed peaks are 3/3. Mux video/audio observed peaks
are A=16/16, B=2/unavailable (audio channel unused), C=16/16, with capacity
16 each. Boundary snapshots are not an atomic exact high-water census; no
unknown measurement is fabricated as zero.

Explicit mux flush counts are **3028 / 1563 / 4493** for A/B/C. Final reason
is `final-eof`; written packets/bytes since explicit flush return to zero.
The unchanged triggers are 64 packets or 8 MiB, not total mux/driver memory
bounds. Internal pending packet/byte census remains unavailable.

Measurement early/middle/late 10k FPS are A=**511.15 / 539.07 / 530.87**,
B=**322.33 / 311.69 / 241.23**, C=**200.58 / 215.07 / 223.01**.
Endpoints include startup/finalization and nearest external frame timestamps.
B varies without a demonstrated pathological collapse. Default C's aggregate
123.20 FPS was slower; thermal/scheduling causes were not proven. The C repeat
records **202.32 /208.81 /209.39 FPS** early/middle/late, with 479.833 s
conversion wall time: no progressive collapse. These written-frame estimates
are not atomic processed-frame timings. No small-percentage performance gate
is imposed.

### Long-progress controls and post-soak regressions

- Path C SIGINT at **51,000** observed frames: exit **130**, no committed
  incomplete target, staging removed, FD recovered, all tracked resources and
  known bytes recovered. Conversion/control wall time 260.195 s.
- Path C native mux write failure at **51,000**: `RLIMIT_FSIZE` produces
  **File too large (-27)**, exit **1**, the MuxRuntime root survives, no
  committed incomplete target, staging removed, FD/resources recovered.
  Control wall time 240.858 s. This is real filesystem I/O failure, not a
  device-loss or simulated mux claim.
- After all primary 100k runs: **17 retained paths ×3 PASS**, all 51 whole
  artifacts match the retained identities, with the existing typed/decoded
  oracles and H.264 semantic/attested-pair tests unchanged.
- Canonical portability core: **24 fixture classifications PASS**, 24 runtime
  checks, 16 decode-back checks; overlapping surfaces are not added as unique
  case counts.
- Six-case compatibility subset: B-frame, dual AAC, fragmented MP4, expected
  color-conflict rejection, display transform, corrupt-middle expected failure:
  **all PASS under their existing contracts**.
- Qualified alternate iHD **25.4.6** / ANV 26.2.3 / FFmpeg 8.1.3: actual
  default Release B full-GPU **10,000 frames PASS**, 24.818 s conversion,
  metadata/timing and complete decode-back PASS. Its loaded iHD SHA is
  `92d5e20cd3d377a66a19e0da674712afd8b96d5c7c68c81ab45c2956069343a4`.
  No alternate 100k or full five-stack campaign was run.
- Existing **3×10k required Validation PASS** is reused as authorized, from
  `target/stage54d1a-measurement-10k/results.json`; all current 100k runs have
  Validation disabled. This is not a claim of newly running 100k Validation.
- Final Python checks: **43 soak +66 corpus PASS**; format/diff checks PASS.
  Current SPIR-V inventory: **325 physical modules /25 distinct hashes**, all
  `spirv-val --target-env vulkan1.3` PASS. Unchanged-runtime clean default and
  measurement Release/debug build/test/clippy evidence is reused, with current
  binary/source hashes independently verified.

Same-stack strict, historical H.264, Tier 1B-P, Tier 1C, Tier 2 and stack-scoped
Tier 3 policies are unchanged. The alternate B smoke is not new cross-driver
H.264 evidence. Matching retained whole files does not transfer historical
exact-build attestation to the new executable. The one C long repeat **passed**
strict whole-file identity, which also proves identical Tier 2 structure.
Its loaded driver hashes match the primary. No A/B long repeats were needed;
decode-back is reused only because the C repeat is byte-identical to the
completely decoded primary.

## Historical pre-audio-attribution scope limitations and claim boundary

D-1A is **SEALED** under the corrected one-job CLI contract. Its 10-process,
4418-cycle, 18025-job history, +69,648 KiB failure and late +652 KiB/HWM evidence
remain unchanged, not PASS. Classification is
`NonBlockingOutOfScopeObservation: persistent-process allocator/runtime retention not fully bounded`.
Persistent multi-job memory retention remains unqualified; daemon/server/GUI
workers and persistent library hosts are not current production contracts.

D-1B and D-1 remain **NOT SEALED** while the current single-job C memory trend
is unqualified. This is distinct from the out-of-scope persistent-process
observation. Output/lifecycle passes alone do not justify the full claim
“Qualified for the recorded single-job CLI workloads and long-run production
paths.” Neither 24/7 operation nor complete driver-internal resource visibility
is claimed. D-2 is not justified and has not started.

## Qualification files and their current purpose

No new production API/type/dependency is added. Qualification additions and
maintained entry points are limited to the current evidence requirements:

| File | Purpose |
| --- | --- |
| `tests/soak/long-run.py` | Dispatch exactly three 100k production paths with fixed audio topology and reusable identified inputs. |
| `tests/soak/long-controls.py` | Exercise real SIGINT/EFBIG cleanup after 51k frames. |
| `tests/soak/long_audio.py` | Share the strict AAC packet/timestamp/tag comparator between execution and independent artifact review. |
| `tests/soak/long_summary.py` | Share lifecycle/queue assertions and frame-indexed observations without inventing an automatic memory threshold. |
| `tests/soak/review-long-run.py` | Recheck preserved measurement artifacts and retain the original faulty-runner failures. |
| `tests/soak/test_long_run.py` | Test campaign failure/skip handling, locked paths and wrong-source rejection. |
| `tests/soak/test_long_observations.py` | Test resource/queue and strict audio observation contracts. |
| `tests/soak/stage54d1a-product-scope-closure.json` | Record D-1A scope correction without rewriting historical receipts. |
| `tests/soak/stage54d1b-long-soak.json` | Hold the current machine-readable long-soak gate decision and evidence references. |
| `docs/stage5.4d1b-long-soak.md` | Provide the requested workload-scoped report and limitations. |
| `tests/soak/evidence/d1b-long-soak-evidence.tar.gz` | Retain raw non-media receipts/time series and reproduction commands, excluding giant media/executables. |
| `tests/soak/evidence/d1b-long-soak-manifest.json` | Index archive members with sizes/SHA-256 and preserve provenance. |
| `tests/soak/evidence/d1b-mov-attribution.json` | Preserve exact-package per-track index predictions and observed residuals. |
| `tests/soak/evidence/d1b-mov-attribution-evidence.tar.gz` | Retain relevant signed-package source, patches and reproducible calculation without the full source RPM or large media. |
| `tests/soak/evidence/d1b-mov-attribution-manifest.json` | Verify each member and the supplemental native-source evidence archive. |

The existing generator/preflight and corpus supervisor are extended rather than
adding a parallel production framework. The supervisor now kills/reaps its
detached qualification child if observation/reporting raises, preventing orphan
GPU work. `stage5.4d1b-long-run.md` remains a redirect for the old entry point,
not a new phase. Ignored finite-run orchestration/analysis scripts are retained
as reproduction evidence; they are not production layers or new sealing gates.

## Historical pre-audio-attribution final gate index

This index covers the requested 41 report items; detailed values, commands and
measurement caveats are in the sections above and the machine receipt.

| Requested items | Reviewed result |
| --- | --- |
| 1–2: D-1A scope / persistent limitation | D-1A SEALED; historical FAIL preserved; persistent multi-job memory retention remains unqualified and nonblocking. |
| 3–5: A/B/C 100k | All actual default Release full-GPU conversions and artifact checks PASS; C memory qualification remains open. |
| 6–7: media / wall durations | 2000 s each; primary default conversion 203.442 /384.468 /811.686 s. |
| 8–9: RSS / PSS / Anonymous | Full milestone and interval data retained; unexplained C single-job growth reproduced by default binary. |
| 10–13: FD / Vulkan / DMA-BUF / VAAPI | All observed FD boundaries 4→4 and tracked job-owned counts/known bytes →0; driver census not claimed. |
| 14–15: queues / mux flush | Observed peaks within 3/16 capacities; flush counts 3028/1563/4493; triggers not total memory bounds. |
| 16: throughput | Early/mid/late reported; default C repeat 202.32/208.81/209.39 FPS; no demonstrated progressive pathological collapse. |
| 17–19: A/B/C decode-back | All complete 100k decode-back PASS without decode errors. |
| 20–22: SDR / PQ / HDR→SDR | Correct BT.709 SDR8 /BT.2020 PQ10 /BT.709 SDR10, all limited, correct timestamps/durations. |
| 23: HDR stripping | C output probes have no leaked HDR side data; selected source has no mastering/CLL payload, so no new positive metadata-bearing stripping claim. |
| 24: AAC | Single/none/dual topology; strict payload/timestamp/language/default/title checks PASS. |
| 25–26: tiers / long determinism | Policies unchanged; same-stack C 100k repeat byte-identical, Tier 2 PASS. |
| 27–29: cancellation / injected failure / safe output | Both C controls at 51k PASS; exits 130/1, no incomplete committed target, staging and tracked resources cleaned. |
| 30: alternate smoke | Qualified alternate B full-GPU 10k PASS; no alternate 100k required or claimed. |
| 31–33: retained / compatibility / portability | Post-soak 17×3, selected six cases, canonical portability core PASS. |
| 34: H.264 / Tier 1B-P | Existing rules unchanged; no historical exact-build attestation transferred. |
| 35–36: build/static / SPIR-V | Exact runtime/build identity verified against clean builds; 43+66 Python checks PASS; 325 modules Vulkan 1.3 PASS. |
| 37–38: limitations / reliability claims | Only recorded output/ownership/cleanup results qualified; no full long-run memory, persistent-host or 24/7 guarantee. |
| 39–41: D-1B / D-1 / D-2 | NOT SEALED /NOT SEALED /not justified; D-2 not started. Only remaining gate: C single-job memory qualification. |

The retained non-media archive contains 707 members (82,624,171 bytes), indexed
by per-member SHA-256 in
[`d1b-long-soak-manifest.json`](../tests/soak/evidence/d1b-long-soak-manifest.json).
It preserves original failures alongside later independent reviews. Giant
source/output media and executable files are excluded, not silently treated as
retained artifacts.

## Current closure index — final audio attribution

The historical index above records the former open gate, not the current
decision. Current requested audio-attribution items1–20 are resolved as follows:

| Items | Final result |
| --- | --- |
| 1–3: historical residual / replayed AAC / A conclusion | 8,304 KiB historical;8,312 KiB A; A2, producer→mux allocation interaction selected. |
| 4–6: demux / B / close | Actual MOV/MP4 libavformat62.12.103; B not run by A2 rule; instrumented C input-close delta0 KiB. |
| 7–11: AAC ownership / bytes / queues / containers | Peaks10/10 packets,3,482/3,488 bytes; final0/0; channel16/16; no per-frame history, native node/header free paths reviewed. |
| 12–16: allocator / mechanism / fix / residual / class | One post-teardown trim67,704 KiB; bounded live allocation/release interaction with retained free pages; no production fix; measured residuals unchanged, not forced to0; `BoundedAudioPacketAllocatorRetention`. |
| 17: verification | Both100k controls exact-file match; explicit native unit, media/CLI units, clippy/default check/format/diff PASS; existing retained/hardware/SPIR-V evidence reused. |
| 18–20: D-1B / D-1 / D-2 | SEALED / SEALED / justified, NOT STARTED. |

The exact claim remains single-job workload qualification; historical FAIL and
persistent-process limitations are unchanged.
