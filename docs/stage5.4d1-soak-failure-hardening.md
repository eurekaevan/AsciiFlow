# Stage 5.4D-1 — soak / failure / resource hardening

Current progression: D-1A is **SEALED for the current product scope** (one CLI
process, one media job, then process exit). Persistent-process allocator/runtime
retention remains an unqualified, nonblocking observation outside that contract;
the original failures and evidence are preserved. [D-1B production long-soak
work](stage5.4d1b-long-soak.md) is authorized as of 2026-10-08 under that
single-job contract. Final2026-10-09 audio-memory attribution now seals D-1B
and D-1 overall. Original8,304KiB residual measurements remain preserved;
matched replayed-AAC C retains8,312KiB, while a native last-reference census
proves bounded live audio ownership and zero final payloads. Together with
explicit release paths and post-teardown free-page reclaim, the classification
is `BoundedAudioPacketAllocatorRetention`, not a packet leak or constant-RSS
proof. See the final long-soak report and audio attribution receipt.
Exact claim: “Qualified for the recorded single-job CLI workloads and long-run
production paths.” Persistent multi-job memory remains unqualified.
Stage5.4D-2 Release Qualification is now justified, but has not started.

## Historical pre-audio-attribution boundary

Final review 2026-10-09: D-1B and D-1 overall are **NOT SEALED**.
Matched layer isolation subsequently found mux-only residual −52 KiB and
C audio-off residual +24 KiB with 100k encoded video packets identical to full C.
The original 8,304 KiB residual is narrowed to the audio-enabled upstream/native
input path or its mux interaction; its specific state remains unidentified.
It is not reclassified as bounded or PASS. No production Rust behavior changed;
the new replay is test-only. See `stage5.4d1b-long-soak.md` for current evidence.
The only remaining gate is current single-job Path C memory qualification:
unexplained growth persists in a default Release repeat after a logical mux
index model. Output/ownership/FD/queue checks, both 51k controls, alternate
10k and required post-soak/static regressions passed. The original runner's
A/C packet-count failures are preserved, with independent corrected review.
See the [final long-soak report](stage5.4d1b-long-soak.md) and
[current receipt](../tests/soak/stage54d1b-long-soak.json). D-2 is not justified.
The subsequent exact-package MOV attribution accounts for 1024-entry rounded
72-byte clusters per real output track, but still leaves 8,304 KiB in the
default C repeat. Required non-fragmented MP4 indexing is not a leak or a
plateau requirement; this remaining residual, not indexing itself, is the gate.

The following **NOT SEALED** status and progression text records the historical
broader qualification boundary. Historical C evidence and strict
same-stack/historical media oracles are unchanged. The separate D-1A Tier 1B-P
investigation is documented below; its exact known identifier rule is not a
same-stack exception.

The [D-1A report](stage5.4d1a-resource-failure.md) retains the dated broader-
scope history and original receipts. That history is not overwritten or
promoted to a long-soak qualification.

## Implementation pass

Two panic-path gaps were addressed without changing codecs, shaders, pixel
semantics, planner fallback or output formats:

1. Workers publish failure/cancellation while unwinding, before ordered joins.
   Borrowed native contexts previously allowed decoder/audio/mux waits to
   precede discovery of a later encoder panic. Owned objects are bound after
   the guard so their destructor panics are covered too.
2. Mux unwind latches a structured MuxRuntime root before receivers disconnect;
   panic no longer needs to degrade to `mux worker stopped unexpectedly`.

Portable regressions cover encoder encode/Drop panic while source finalization
awaits cancellation, borrowed interop join order and first-root preservation.
A native software-encoder test covers mux panic publication. Synthetic late
decoder/encoder faults at frame 100 and 10,000 retain their stage/root. These
are failure-model tests, not native GPU fault-injection evidence.

## Preflight evidence

The finite-input generator supports 100k/500k recipes. Initial 1,000-frame
materializations exercised all three source types. Repeated AV1 PQ generation
yielded `85e03fb23dd15949d32e8c737d4efd102c4e5b69530068d263a23457b6535b35`
twice. Exact source/tool/commands are under
`target/stage54d1-preflight/source-*/identity.json`.

Intel Arc Meteor Lake Release preflight processed 1,000 AV1 PQ frames via
VAAPI → P010 input interop → Vulkan C pipeline → P010 output interop →
HEVC Main10 SDR, two Vulkan slots, zero hardware download/upload: 5.598 s,
178.65 FPS. This was before the final guard-lifetime refinement: a historical
path preflight, not final-source qualification, Validation, 100k soak,
repeatability or complete decode-back evidence.

Mux replay completed 1,000 runs (500 dual AAC / 500 early audio EOF), 26.22 s.
All MP4s matched their same-source reference byte-for-byte. Every run restored
FDs; capture teardown was 5 → 5 for both fixtures. Between-replay RSS:
52,772–53,864 KiB, median 53,386 KiB; no production leak verdict is inferred.
Raw JSONL and two MP4/trace pairs are under
`target/stage54d1-preflight/mux-soak/`; durable hashes are in the receipt.

After the guard-lifetime refinement, `tests/soak/preflight.py` passed all three
1,000-frame Release paths on Intel Arc with the required Validation layer:
SDR H.264→H.264, PQ HEVC→HEVC Main10 PQ, and AV1 PQ→HEVC Main10 SDR. Each
output decoded completely to 1,000 frames, 1920×1080/50 fps, expected 8/10-bit
4:2:0 and correct limited-range BT.709 or BT.2020/PQ metadata. Every video
packet PTS/DTS was exactly `frame_index/50` in rational seconds. No VUID,
Validation Error or sync-hazard diagnostic appeared. Full input/output/tool/
command/log identities are retained in the receipt and raw evidence under
`target/stage54d1-validation-preflight/`. Resources remain Unresolved: these
short, audio-free tests are not the required 10k Validation or 100k long gates.

Final reruns are under `target/stage54d1-final-validation-preflight/` and
`target/stage54d1-final-mux-soak/`, recorded in `final_verification` in the
receipt. All three input and output SHA-256 identities matched across the two
1k Validation preflights. The final 1,000 mux replays passed in 26.14 s with zero
FD mismatches and both capture drops restoring 5 → 5. Dual-AAC RSS was
52,712 KiB initially, 52,848 KiB steady median, 52,852 KiB finally; sparse-audio
RSS was 53,808 KiB throughout. These are per-replay samples, not production
resource qualification. A skipped hardware preflight exits 2, not success.

Workspace tests and strict lint passed in both default and encode-characterization
plus mux-qualification configurations; Release workspace build and generator
safety tests passed. All 1,003 SPIR-V files actually present under debug/release
build roots passed Vulkan 1.3 validation. This is not a two-clean-build census
or a claim that the historical 1,203-artifact set is still present.

## Original pass gate ledger (historical)

| Group | Decision |
|---|---|
| Panic/join/root regressions | Exercised; no complete native failure matrix claim |
| Native dual/sparse mux replay ≥1,000 | PASS, exact output and FD restoration |
| Three ≥100k representative full GPU paths | NOT RUN |
| 500k dual-slot PQ→SDR plus audio copy | NOT RUN, extended characterization |
| Frame-cadence RSS/FD trends/throughput | Unresolved |
| DMA-BUF, Vulkan allocations, VAAPI surfaces | Unresolved |
| Exact queue packet/byte peaks and slow I/O | Unresolved; configured bounds are not measured peaks |
| ENOSPC/EIO, permission, commit, cleanup matrix | NOT RUN as D-1 campaign |
| 20 cancellations and error/disconnect races | Partial portable tests; phase matrix NOT RUN |
| 100 success / 50 failure / mixed / contamination jobs | NOT RUN |
| Corrupt/parser smoke, integer/large timestamp audit | Existing tests only; D-1 campaign NOT RUN |
| 10k Validation paths and alternate-stack smoke | NOT RUN |
| 17×3 retained, B subset, C canonical core | NOT RUN on final D-1 source |
| Two clean builds, measurement/static/SPIR-V | Receipt tracks executed subset; incomplete gates stay open |

At the time of this original pass, all three planned production paths had
**Unresolved** D-1 resource behavior. No qualified-stack registry or historical
baseline is promoted. Current D-1B progress is tracked in the
[long-soak document](stage5.4d1b-long-soak.md). No 24/7 uptime guarantee is
asserted.
