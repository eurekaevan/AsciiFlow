# Stage 5.4D-1 — soak / failure / resource hardening

Status: **NOT SEALED**. D-2 has not started and is not justified by this partial
evidence. Historical C evidence and strict media oracles/Tier 1B-P are unchanged.

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

## Gate ledger

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

All three production paths have **Unresolved** D-1 resource behavior. No
qualified-stack registry or historical baseline is promoted. Freeze source and
stack, follow [reliability testing](reliability-testing.md), complete all gates,
then review evidence before sealing. No 24/7 uptime guarantee is asserted.
