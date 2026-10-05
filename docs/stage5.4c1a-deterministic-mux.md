# Stage 5.4C-1A — Deterministic Mux Interleaving Closure

Status: **SEALED**. Stage 5.4C-1 is **SEALED**. Stage 5.4C-2 is not started.

The strict Tier 2 global packet-order oracle is unchanged. Equal per-stream
packets, timestamps or decoded pixels never override its failure. The fix is a
deterministic bounded producer merge, not deletion of intermediate flushes,
packet-order normalization, `av_write_frame`, or a new lossy oracle.

## Historical evidence and first source of divergence

The two complete historical files are preserved in
[`mux-captures`](../tests/portability/mux-captures/README.md), including codec
parameters, extradata, compressed payloads and packet side data. Materialization
checks their exact original SHA-256 before native replay; it does not encode or
decode new packets. Historical A/B structures are not current regression gates.

Thirty pre-fix full productions reproduced eight global packet-order variants
with occurrence counts 2/3/18/1/1/3/1/1. All stream-local payloads and packet
timestamps match. Divergence can first appear around packets 53–69 and again
around 119–133; exact per-run indices and vectors belong in the receipt.
Producer order, enqueue, owner receive and write order already diverge before
container output. Flush calls occur at writes 64/128 in both cases, but their
stream composition differs. Root classification: R1 producer scheduling,
R2 shared arrival FIFO, R4 arrival-relative flush, and R5 libavformat receiving
different caller sequences. R3 does not introduce reordering of its FIFO;
R6 (different structure under identical caller sequence) was not observed.

Fixed caller sequence replay repeated 30 times produces one structure. Controlled
video-ahead/audio-ahead/alternating/burst arrivals and finite 1s/10s interleave
delta experiments change structures when caller order/flush composition changes.
No-midstream-flush is diagnostic only: it does not establish a byte bound, and
finite timestamp deltas do not bound arbitrary bitrate, rate or compressed sizes.

## Scheduling and lifecycle contract

One mux thread alone owns `AVFormatContext`, all interleaved writes, explicit
NULL flushes and trailer writing. An independent audio demux reader opens the
same immutable local input and retains all selected compatible audio tracks in
original demux order. This costs a second compressed-input demux pass; it is
not audio decode/transcode and not a claim about mutable or one-shot inputs.

Video and combined-audio producer FIFOs each hold at most 16 messages; the owner
retains one head from each. It waits for a head or explicit EOF on both producers
then compares checked, rescaled **output DTS → output PTS → output stream index**
using `av_compare_ts`, never floating point, arrival time, queue occupancy, sleeps,
frame-count epochs or guessed watermarks. Comparison and emission share the same
normalization helper. Each producer's order is preserved; audio-group order is
not advertised as a global minimum across all audio tracks. Libavformat still
performs container interleaving. Missing timestamps, invalid rational time bases
and overflow fail rather than being clamped or guessed.

Explicit flush remains at **64 written packets or 8 MiB**, whichever comes first,
and at final EOF before trailer. Post-header native options are
`max_interleave_delta=10000000`, `avoid_negative_ts=0`, `flush_packets=-1`;
no new MP4 options are passed. A zero-video-frame EOF needs no VideoOrigin.
Both producer EOFs and drained heads precede the trailer; one early EOF drains
the other producer completely without padding or audio trimming.

Decoder output channels close before audio-reader joins in all three pipeline
variants, allowing the two-slot GPU processor to drain its tail. Typed first
failure is cloned without losing stage/operation/source; all waits prioritize it
over cancellation/disconnection. Cancellation stays `Cancelled`, not a wrapped
runtime failure. Reader AVIO interruption, local stop/join and packet RAII cover
blocked reads/sends, initialization failure and unwinding. FFmpeg consumes an
independently owned packet reference at each interleaved write.

Rejected alternatives: removing flush loses the explicit byte/liveness bound;
arrival-based shared FIFO leaves the race; strict frame epochs deadlock when
the GPU needs subsequent inputs before its first result; sorting arbitrary
backlogs is neither bounded nor a safe EOF/watermark policy. Increasing capacity
cannot solve those cycles. Dual independent demux FIFOs are the smallest coherent
solution for the qualified immutable local-file production surface.

## Bounds, trace and evidence semantics

Compressed packets have a 16 MiB limit. The hard application queue/head bound is
34 packet references (2×16 FIFO + 2 heads), excluding at most one pending send per
producer and demux scratch packets. Explicit 8 MiB flushing bounds bytes submitted
between flushes with at most one accepted-packet overshoot (24 MiB); the 64-packet
limit is additional. These are **not** a whole-process RSS ceiling. Codec buffers,
input probing, libavformat allocation overhead and MP4 sample tables are separate;
sample tables grow with packet count. Observed RSS must identify the measured
process and distinguish native test/CLI measurements from Rust compiler peaks.

`mux-qualification` is absent from normal builds. Opt-in traces include produced
packets, enqueue attempt, successful-send acknowledgement, actual owner receive,
rescaled packets, write calls, flush decisions, native options and trailer.
Acknowledgement is logged after send returns and can follow owner receive; the
channel FIFO, not this diagnostic logging race, defines actual receive order.
Per-stream sequence and global trace ordinal are retained. Final demux packet
vectors constitute boundary H. Raw trace hashes may differ due to producer events
and observational queue length. Logical write/flush fingerprints omit only those
diagnostic fields; they never normalize any output packet-order oracle.

## Final verification record

The final authority is the [closure receipt](../tests/portability/stage54c1a.json).
Its source file inventory covers tracked and untracked implementation, tests,
generators, fixtures and docs. Only stack registries and the C1/C1A receipts and
reports are excluded to avoid recursive hashes. Earlier failed/setup/source-guard
attempts are preserved as historical, not blended into final PASS counts.

Final source HEAD: `e186f754701e1ca45f09cfc8a45de3ad7dc29330`.
Dirty diff SHA-256: `206719370ae9956745e00facbf9d9b2d5d3be2a562b5134ee9929214abf62923`.
Cargo.lock: `3bda07030e828d5455e2d56fbe971253034abe7f8383594a7dd8f19eadf504e5`.
Normal CLI: `533222d7fb5a8c8055d741d00e3ae8f43c284a6f9b3e1d2a6de3b58bec29426e`.
Qualification CLI: `9b03bcd2b959114a90f6dbe0310f03aa05f239aa6bde207ca935e434ba5dcac3`.
The two build modes are deliberately separate; neither is substituted into the
other's exact-build attestation. Both portability stacks use the normal binary.

### Required 71-item report

Packet indices below are zero-based. A packet-order digest hashes the exact
ordered packet vector and is **not** the MP4 file SHA-256. Same-stack Tier 3
uses the existing `MATCH` terminology, not an invented PASS status.

| # | Item | Observed result / contract |
|---|---|---|
| 1 | Historical A | MP4 SHA `c33ac221eeb792790c2113bb9674586a05711672baa04177676750ec97b7adfa`; original complete bytes preserved. |
| 2 | Historical B | MP4 SHA `32ea1a2fcb11e2ac328ed726bfc10536b0032ca18078b21fa61da962dcfeafda`; original complete bytes preserved. |
| 3 | Occurrence | Historical 3 repeats: A/A/B. New pre-fix traced sample: 30 runs, eight variants, counts 2/3/18/1/1/3/1/1 (6.67/10/60/3.33/3.33/10/3.33/3.33%). This is a measured sample, not a universal probability. |
| 4 | First differing packet | Historical A/B first 56, last 68. Across new pre-fix variants first 53–119, with tail differences through 133; receipt retains exact vectors/regions. |
| 5 | Stream-local identity | All 30 pre-fix runs have identical per-stream payload hashes, PTS/DTS/duration/flags; 50 video + 88 AAC packets. |
| 6 | Producer trace | Boundary A preserves local sequence/native timing/payload SHA. Run0 vs run1 global produced sequence diverges at event-within-boundary index10. |
| 7 | Enqueue trace | Boundary B attempt, and post-fix separate successful-send acknowledgement. Acknowledgement may be logged after receive; it is not an atomic arrival timestamp. |
| 8 | Owner receive | C logs actual channel receive, not the later selected write. Pre-fix first global C divergence10; trace ordinal plus local seq retains identity. |
| 9 | Write call | D/E record actual normalized native packet before interleaved write. Pre-fix first E divergence10. Post-fix 10 software E/F logical fingerprints identical: `764b1e708a2939143275e3072e0c8ddd7e02ad18ff4cf7e0c49aa4c19487a475`. |
| 10 | Flush trace | Pre-fix F64/F128 counts match but F128 stream composition differs; ignore only diagnostic event ordinal/queue length when comparing logical F. |
| 11 | Final order | Boundary H is exact common ffprobe ordered vector, including stream index, payload SHA, PTS/DTS/duration/flags/size. No sorting or normalization. |
| 12 | Existing trigger | Explicit NULL flush after64 packets or8MiB submitted payload, whichever first; not transient FIFO emptiness. |
| 13 | Interleave delta | Actual post-header10,000,000 µs; diagnostics also exercise1,000,000 µs. Finite duration does not imply a byte/RSS ceiling. |
| 14 | Mux options | Actual `avoid_negative_ts=0`, `flush_packets=-1`; header receives no new MP4 options; only `av_interleaved_write_frame`, not `av_write_frame`. |
| 15 | Root class | R1/R2/R4/R5: producer scheduling → shared arrival order → different flush composition/caller sequence. R3 FIFO itself does not reorder. R6 not observed. |
| 16 | Fixed native replay | Preserved codecpar/extradata, side data and referenced native packets; no encoder, decoder or regenerated media. |
| 17 | Fixed repeat count | 30/30 pre-fix identical caller replays produce one structure (`887e77…` ordered-vector digest). |
| 18 | Arrival perturbation | Four patterns (video ahead, audio ahead, alternating, bursts), flush on/off and1s/10s delta:46 outputs, eight structures; each stream-local packet vector unchanged. |
| 19 | No-mid-flush diagnostic | Four default10s/no-mid-flush patterns converge to `489a82…`; final native flush/trailer retained. Diagnostic only, not the production fix. |
| 20 | No-flush memory | Legacy46-output native suite observed56,128KiB RSS (aggregate, not isolated no-flush RSS). It loads a finite138-packet capture and uses no project FIFO; zero FIFO growth by construction is not production evidence. Without byte/count flush, finite delta cannot bound arbitrary bytes/bitrate; internal allocation was not instrumented. |
| 21 | Fix | Two bounded producer FIFOs, deterministic head merge, independent immutable-file audio demux, correct EOF/channel-close lifecycle. |
| 22 | Why | Producer skew cannot alter E/F media sequence; audio backpressure cannot prevent video lookahead or GPU tail drain. |
| 23 | Rejected | Removing flush, shared arrival FIFO, unbounded sorting, increasing capacity, and frame epochs that deadlock the two-slot GPU processor. |
| 24 | Key | Checked output DTS → output PTS → stable output stream index; preserve each producer's local order. |
| 25 | Comparison | `av_compare_ts` with positive rational bases; same checked normalization helper used for comparison/emission; no floating-point comparison. |
| 26 | Equal DTS | Output PTS then output stream index; rational/tie/offset unit controls pass. Combined audio retains original input-demux order, not a global per-track sort. |
| 27 | EOF | Explicit Finish/AudioDone and drained heads precede trailer; early EOF drains the other producer; zero video needs no origin. |
| 28 | New flush | Same64/8MiB policy on deterministic writes plus final EOF NULL flush before trailer. Software example F64/F128/F138; logical F repeats exactly. |
| 29 | Single owner | One thread owns output format, mapping/rescale/write/flush/trailer; producers only hand off RAII packets. |
| 30 | Queues | Video16 + combined-audio16; owner holds at most two heads, polling with bounded cancellation/root checks. |
| 31 | Hard bounds | 34 queued/head packet refs, ≤544MiB accepted payload; plus at most two pending sends (≤32MiB), separately accounted scratch/codec/probe buffers. Packet-size limit enforced before enqueue. ≤24MiB payload submitted between flushes with one16MiB overshoot is **not** a libavformat memory guarantee. |
| 32 | Memory observation | Pre-fix flush-time FIFO sample max8; post software max2;30,084 replay max16. These are sampled queue counts, not exact all-time peaks or byte counters. Full-queue cancellation asserts audio FIFO16. Native100+stress process peak56,332KiB. Software CLI110,308–111,176KiB; validated/traced GPU126,060–126,976KiB. |
| 33 | Backpressure | Timed sends, bounded heads, AVIO interrupt + reader stop/join. Stored typed first root precedes cancel/disconnect and video validation/receive failure. |
| 34 | Video ahead | Intentional10ms audio start delay in qualification replay; fixed E/F/H structure. No production sleeps. |
| 35 | Audio ahead | Intentional10ms video start delay; bounded audio FIFO and same structure. |
| 36 | Bursts | Periodic1ms/yield scheduling perturbations across100 runs; native burst patterns retained in causal replay. |
| 37 | Sparse/early EOF | 218 repeated3-second timeline cycles (30,084 packets), explicit periodic flush; unequal-duration software tests,60s audio-longer stress, full AAC retained for1/2-frame CPU/GPU tails. |
| 38 | Equal DTS | Unit controls cover rational equality, PTS/stream tie breaks, offsets and missing/overflow timestamps; none guessed/clamped. |
| 39 | B frames | Actual H264 B=4 and HEVC B=2 full conversion/decode-back/timing checks pass. Ordering uses DTS, not frame presentation order. |
| 40 | Mux-only100 | 100/100 exact ordered-vector digest `489a82f364ad461f90232f907c831e49609eb6cf6dd46194fdaaf4595bfb262c`; strict original oracle all100:1A/B/C/2 PASS,3 MATCH. |
| 41 | Production repeats | Normal CPU/software10 single+3dual, actual VAAPI→Vulkan→VAAPI10single+3dual; each case one structure. Separate original-path traced software10 repeats also match. CPU/GPU1/2-frame tails complete. |
| 42 | Tier2 | All100 replay,26 production pairs and10 traced production pairs PASS; packet order remains mandatory. |
| 43 | Tier1A | All136 same-stack pairs PASS. Cross-stack three approved metadata cases still visibly FAIL; never relabeled raw-byte PASS. |
| 44 | Tier1B | All136 same-stack pairs PASS; existing narrow cross-version metadata controls unchanged. |
| 45 | Tier1C | All136 same-stack pairs PASS; real decoded pixel equality, not merely packet metadata. |
| 46 | Tier3 | All136 same-stack pairs MATCH. Cross-stack exact file identity can remain DIFFERENT; approved semantic comparisons do not transfer exact-build attestation. |
| 47 | No audio | Final default/measurement/qualification workspace tests pass audio-none and video-only-input policies; retained51 video-only productions cover full GPU EOF. |
| 48 | Single AAC | Software and full GPU10 repeats each; strict timing/payload/pixels/container identity checks pass. |
| 49 | Dual AAC | Three repeats on each production path, six strict pairs; track order, routing, complete payload and metadata retained. |
| 50 | Audio shorter | Final `unequal_stream_durations_drain_both_tracks_without_trimming_or_padding` tests video-longer fixture; no trimming/padding. |
| 51 | Audio longer | Same test audio-longer/short-video,60s stress, and CPU/GPU tails; both tracks fully drained under their contracts. |
| 52 | AAC payload | 30 actual input→output retained-audio comparisons PASS, plus workspace policy tests; full AAC decode succeeds. |
| 53 | AAC metadata | Existing `same_audio` checks codec/rate/channels/packet identity/timestamps/language/default. Additional actual ffprobe input/output comparisons on all30 artifacts prove extradata hash, semantic title/name and every disposition bit unchanged; dual-track routing/default controls PASS. |
| 54 | Fractional CFR | Real24000/1001 conversion and rational timestamps/decode-back PASS. |
| 55 | Non-zero PTS | Actual +2s and large-origin cases PASS; A/V relative-offset software control also PASS. |
| 56 | Fragmented MP4 | Real fragmented video and fragmented AAC cases pass full conversion, decode-back and timestamp checks. |
| 57 | Cancellation | Native wait-for-head/full-audio-FIFO/buffered-interleaver states all Cancelled with FD restoration and no invented root; actual Intel audio SIGINT lifecycle also PASS. |
| 58 | Faults | SDR5-output native receive/send/header/write/trailer matrix, P010 output failures, constructor rollback and stored-root controls PASS. Existing error tags/text are retained. |
| 59 | FD | Capability loops, constructor rollback, output lifecycle, native cancellation and real GPU cancellation/follow-up checks PASS. |
| 60 | Validation | Actual Intel full paths use required Khronos Validation; native output/interop/fault gates PASS. Not a lavapipe substitute. |
| 61 | Performance sanity | Old CPU CLI3 runs0.19–0.22s/~108.6MiB vs new10 runs0.20–0.28s/~108.6MiB for50 frames; no obvious RSS regression, not a controlled throughput benchmark. Native100+30k2.34s including trace IO. Same GPU3-per-mode: normal0.28–0.30s, qualification-off0.32–0.35s, trace-on0.40–0.42s; trace is opt-in and absent normal build. |
| 62 | Canonical17×3 | Final frozen-source5 SDR +2 PQ +10 HDR→SDR paths, three runs each, all51 retained identities/oracles PASS; C1/C2B CPU and C3/C4A native reference checks PASS. Historical exact-build attestation remains separate. |
| 63 | FFmpeg8.1.3 | Final-source24-case core PASS; normal binary533222d7…, loaded libav.103 attested. |
| 64 | FFmpeg8.1.2 | Final-source isolated-prefix24-case core PASS; same binary/source, actual loaded libav.102 attested in a fresh process; no host package replacement. |
| 65 | Paired semantics |16 SemanticEquivalent outputs +8 ExpectedExact rejections, no planner/capability drift. Three raw1A differences (`canonical-h2648`, `hevc-pq-to-sdr10`, `canonical-h2648-auto`) remain visible with1B/C/2 PASS; all cross-stack Tier3 differences recorded. |
| 66 | Historical H264 | Strict1A FAIL,1B/C/2 PASS remains historical; old exact-build attestation is not transferred. Narrow SEI allowlist/negative controls and final retained H264 oracle mandatory and unchanged. |
| 67 | SPIR-V | Final source-bound actual1003 modules,34 unique hashes,0 failures under `spirv-val --target-env vulkan1.3`; unique shader hashes unchanged. |
| 68 | Static |11 final gates PASS: release workspace build, default/encode-characterization/mux-qualification tests and clippy, formatting,60 corpus controls, support docs and diff check. |
| 69 | Remaining differences | Cross-version approved metadata/file identity, qualified immutable local inputs and single Intel driver/kernel pair; MP4 indexes grow. No arbitrary mutable/one-shot input or whole-process RSS ceiling claimed. |
| 70 | Stage5.4C-1A | SEALED. Required deterministic, bounded/lifecycle, strict same-stack, retained and portability gates pass. |
| 71 | Stage5.4C-1 | SEALED through C1A closure; historical failures remain visible. Stage5.4C-2 not started. |

### Reproduction

Historical bytes are materialized by `tests/portability/mux-determinism.py
materialize-captures`. The native ignored tests
`fixed_sequence_and_arrival_replay`, `deterministic_producer_merge_replay`, and
`deterministic_merge_cancellation_releases_fds_and_packets` take
`ASCIIFLOW_MUX_REPLAY_SOURCE` and `ASCIIFLOW_MUX_REPLAY_DIRECTORY`; build media
with `--features mux-qualification`. `strict-oracles` invokes the unchanged Rust
portability comparator for every artifact with an attested same-build policy.
`production --runs10` repeats the original software case; receipt argv records
the separate real GPU variant, dual AAC, tails, exact input/tool/binary identities
and trace requirements. `run-mux-closure.py` runs retained/static/lifecycle/timing/
portability gates. The checked-in fixtures, pinned generators and isolated FFmpeg
recipe are the regeneration source; ignored target outputs are only raw evidence
locators, not the sole durable authority.

The single AAC input SHA is
`204ed8174c3c13d12dbcdb08b87d7ee97b016575e2cf8c176cd94c9c51c692ea`.
Final repeat file SHAs (not order digests): software
`1d077aa1ab2d9f8f6b9881b2b57230429f466164c1a1b061b8e02ea0c37c1964`,
GPU `d9b92d6f22e146ef22e632e69aebbb216b18e119ba9ba5735d6bd149471ce646`.
These do not replace, reinterpret or pretend to reproduce historical A/B.

Native ownership follows the
[FFmpeg8.1 encoding API](https://ffmpeg.org/doxygen/8.1/group__lavf__encoding.html):
independent packet references passed into consuming interleaved writes, explicit
flush and final trailer, with producer/reader cleanup on failure or cancellation.
