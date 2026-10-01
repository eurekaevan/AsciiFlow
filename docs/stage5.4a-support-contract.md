# Stage 5.4A — Production Support Contract & Compatibility Corpus Harness

Status: **SEALED**.

This stage packages existing production behavior; it adds no codec, HDR mode,
encoder or shader algorithm. Stage 5.4B has not started. The portable architecture
is not restricted to one vendor; hardware qualification remains scoped to actual
retained evidence and runtime probes.

## Implementation and evidence

The versioned source of truth is
[`production-support-v1.json`](../tests/support/production-support-v1.json),
schema 1, support contract 1.0.0. Strict typed tests in
[`support_contract.rs`](../crates/asciiflow-core/tests/support_contract.rs)
reject unknown fields, missing enum classifications, invalid evidence references,
and scenario/planner disagreement. Supplied capability facts are architectural
test inputs, not hardware qualification.

Supported means qualified within the stated evidence scope.
ConditionallySupported means qualified only with all listed capability/path
conditions; it never means “maybe works.” Unsupported is an explicit current
product rejection. Unqualified means insufficient production evidence even when
implementation/planner admission exists. Existing legacy SDR normalization and
planner behavior are preserved; this stage does not turn qualification labels
into a new vendor gate or silently disable already-admitted paths.

The corpus schema and runner are in [`tests/corpus`](../tests/corpus/).
There are 13 registered MP4 fixtures: seven positive codec/depth/color cases and
six expected negatives. Generated input reuse is read-only and still checks
generator/reference identity, byte size and SHA-256. Setup failure blocks
dependent SDR smoke and retained execution. There is no automatic baseline
promotion, and every output directory/file must be fresh.

## Current run records

- Quick: `/tmp/asciiflow-stage54a-quick-final`; exit 0. Three portable SDR8
  conversions/decode-backs, six expected rejects, four explicitly skipped
  hardware-only positives in the current sandbox environment.
- Hardware: `/tmp/asciiflow-stage54a-hardware-v3`; exit 0, no failed/skipped
  gates. Seven positive representative conversions; six expected rejects;
  four full-interop 300-frame smokes; C-1/C-2B/C-3/C-4A references.
- Full plus retained: `/tmp/asciiflow-stage54a-full-hardware-v2`; exit 0,
  no failed/skipped gates. This is the five SDR + two PQ + ten HDR→SDR
  matrix, each generated three times, plus workspace/reference/representative
  and hardware smoke gates.

Counts are overlapping verification surfaces, not a unique-test total.
Environment and command/result files record executable/source/lockfile/tool
identities, typed plans, initialized paths, output hashes and individual exits.
Later changes to the runner are setup/skip-report controls only. The corpus
subsequently made its existing CFR timing model explicit; the contract's final
edit only aligned evidence-entry whitespace. These are not changes to fixture
bytes, support semantics or expected outcomes. Run environment manifests retain
their original source hashes; the closure source inventory records the final
harness/contract identities. The production executable and pixel-processing
source remain unchanged during these runs.
The [durable closure evidence](../tests/corpus/stage54a-closure.json) records each run's own identities,
rather than falsely claiming one historical source/build attestation.

The current executable SHA-256 is
`0b26022b21ffff38451a5ecf06a0b4d6898d97e03839adda3079ef090b918a1d`.
Host: Intel Arc Meteor Lake 8086:7d55; renderD128; Mesa ANV 26.2.3-1.fc44;
Intel iHD 26.1.5-1.fc44; FFmpeg/ffmpeg-libs 8.1.3-1.fc44;
libavcodec 62.28.103, libavformat 62.12.103; kernel 7.2.7-200.fc44.
This does not qualify other devices, driver stacks, modifiers or resolutions.

## Fixture construction and identity

The existing H.264 1080p/300 source uses
`tests/fixtures/codecs/generate-8bit-production-baseline.sh` and SHA-256
`6e5c214b813dca3e1db65629b1241cfb663166eb565cda1c48b5ee74ed0dce6b`.
The retained canonical HEVC SDR10 source is
`df69c98ca6592b08c6bc34127b2bb5cf4125c759fd852b830e5426d5818fccda`.

Two new generated representatives have independently repeated byte-identical
generation with the recorded FFmpeg 8.1.3 package. Full exact FFmpeg commands
and flags are checked in the scripts; the manifest pins their hashes, tool
identity, invocation and dependencies:

| Representative | Generator invocation | Repeated-generation SHA-256 | Bytes |
|---|---|---|---:|
| AV1 SDR10, 1920×1080, 300 frames, 50 fps, BT.709 limited, left, lossless from canonical HEVC10 | `bash tests/corpus/generate-av1-sdr10.sh NEW_FILE.mp4` | `900216775400e6af237ed93b6dffde6fe76aacd88d9529507b8d720de3d800ee` | 228530990 |
| Full-range PQ rejection representative, remuxed from the legal C3 HEVC input with matching bitstream/container range | `bash tests/corpus/generate-full-range-pq.sh NEW_FILE.mp4` | `f99c5a77113468edb2a24fc3339eb7f65f2d624f38ff2b47815638082c0529dd` | 1927303 |

An initial hardware run correctly rejected the two old 64×64 SDR10
representatives: the actual encoder probe did not accept those dimensions.
The positive representatives now use full canonical dimensions. The failure is
not weakened or relabeled PASS. Profile availability alone is not a resolution
guarantee, and this discovery adds no hardcoded portable dimension/vendor gate.

## Required final-report inventory

| # | Surface | Contract / observed result |
|---:|---|---|
| 1 | Support schema | Strict typed schema 1; deny unknown fields throughout. |
| 2 | Contract version | 1.0.0, separate from schema version. |
| 3 | State definitions | Four exact states defined above and generated documentation. |
| 4 | Containers | MP4 qualified by scoped paths; Matroska input Unqualified, output Unsupported by existing MP4 guard. |
| 5 | Codec/profile | H.264 Baseline/High SDR8; HEVC Main8/Main10; AV1 Main 8/10. H.264 Main remains Unqualified; profile enumeration is not proof. |
| 6 | Depth/chroma | Qualified 8/10-bit 4:2:0; strict 10-bit input semantics. 12-bit/4:2:2/4:4:4 unsupported, no silent conversion. |
| 7 | SDR color | BT.709 limited; legacy 8-bit BT.601 software normalization admission is documented separately from qualification. |
| 8 | HDR preserve | PQ/BT.2020 NCL limited P010, HEVC Main10/AV1 Main10 output, qualified full GPU path. |
| 9 | HDR→SDR | Explicit PQ→BT.2446 Method A→BT.709 target limiting→NV12/P010; existing path only. |
| 10 | Luminance ceiling | Every decoded source-pixel component finite and within 0–1000 cd/m² before averaging/coverage. Metadata does not authorize the domain; no implicit clipping. Ceiling does not apply to PQ preserve. |
| 11 | Rejected color | HLG, BT.2020 SDR, full-range HDR, unknown strict-10-bit/conflicting semantics and unsupported midstream changes. Legacy 8-bit defaults remain explicit exceptions. |
| 12 | Output matrix | H.2648; HEVC8/10; AV1 8/10. PQ requires HEVC/AV1 10. No H.26410. |
| 13 | Decode | Auto/software/VAAPI states classified; chosen plan must match input-scoped capability facts. |
| 14 | Backend | Auto/CPU/Vulkan classified. CPU SDR versus unqualified software HDR is not conflated. |
| 15 | Encode | Auto/software H.264/VAAPI classified; software HEVC/AV1 not introduced. |
| 16 | Audio | None/auto/copy documented. Existing AAC codec eligibility, CFR timeline, routing and fail-closed copy policies unchanged. Representative commands use audio none. |
| 17 | Hardware scope | Recorded Intel/ANV/iHD/FFmpeg stack only; portable planner has no vendor gate. |
| 18 | Evidence linkage | Existing sealed reports/manifests/device evidence linked by validated IDs and repository-relative paths. |
| 19 | Planner consistency | Contract drives the real planner; typed error/color-reason checks, complete scenario cases and negative capability controls. |
| 20 | Capabilities | Runtime Supported/Unsupported/NotProbed facts explicitly scoped to probes, never global qualification; actual chosen-path assertions pass. |
| 21 | Explain plan | SDR8/10, PQ preserve, PQ→SDR8/10 and HLG/full-range/H.26410 rejects inspected with structured diagnostics. |
| 22 | Public docs | README/support document generated from contract; drift checker passes. Architecture, codecs and testing link the contract. |
| 23 | Corpus schema | Closed schema with a fail-closed stdlib validator; unsupported schema keywords rejected. |
| 24 | Input identity | Source kind, relative path, byte size, SHA-256, exact command, source/reference identities. |
| 25 | Generator identity | Generator SHA-256 and tool/package identity; repeated deterministic generation verified, not guessed. |
| 26 | Timing | CFR/VFR/Unknown model, frame rate/count/time base/start PTS/negative DTS/reorder/odd durations. Current entries explicitly CFR. |
| 27 | GOP | Key interval, B frames, open GOP and cadence representable; a retained B-frame input actually executed. |
| 28 | Color | Primaries/transfer/matrix/range/chroma location, expected classification and output metadata. |
| 29 | Audio schema | Array of tracks with codec, language, default disposition, channels, rate and time base. |
| 30 | Metadata schema | Subtitle behavior, title/language/rotation/orientation, side data, mastering display and content-light fields. Schema capacity is not preservation proof. |
| 31 | Outcomes | Pass; RejectAtClassification; RejectAtPlanning; RejectAtInitialization; RuntimeFailureExpected. |
| 32 | Failure model | Typed existing PipelineStage, root Error category and ColorSupportReason code, never English substring matching. |
| 33 | Runner | Single stdlib orchestrator, sorted JSON/Markdown, fresh outputs, source identity before use, command logs. |
| 34 | Quick | Exit 0; actual CPU conversions plus expected rejects, explicit hardware skips. |
| 35 | Full | Exit 0; workspace and CPU references plus corpus, hardware and retained matrix. |
| 36 | Hardware | Exit 0; actual VAAPI decode→Vulkan→VAAPI encode with input/output DMA-BUF. |
| 37 | Hardware absence | SKIPPED refers to current execution environment, not “host has no GPU.” No fake failure or PASS. |
| 38 | Environment | HEAD, dirty diff, untracked identities, lockfile/binary, full FFmpeg configuration/libav, packages, kernel, GPU/VAAPI. |
| 39 | Results | Sorted fixture IDs, actual plan/path, hashes, command exits/timing, oracle references, category counts and skip reasons. |
| 40 | Promotion | No automatic promotion; explicit future reviewed action only. Existing baseline/output directories refused. |
| 41 | Media tiers | Existing Tier 1A/1B/1C/2/3 comparators reused, not duplicated or loosened. |
| 42 | H.264 oracle | Existing narrow bitstream semantic oracle/control set plus actual same-build production run1/run2 strict pair. No new SEI exemptions. |
| 43 | Retained inventory | Five SDR + two PQ + ten HDR→SDR, three runs each, one command. Old build identity remains distinct. |
| 44 | Negatives | HLG, wide-gamut SDR, full-range PQ, illegal H.26410, unknown transfer, conflicting metadata. |
| 45 | Expected rejection | Six negatives yield PASS only at the expected typed stage/category. |
| 46 | Unexpected pass | Mandatory offline control fails an illegal success; no “unsupported but works” promotion. |
| 47 | Unexpected reject | Positive fixture execution is mandatory; unexpected failure fails the gate. |
| 48 | Enum completeness | Exhaustive portable enum matching in Rust forces every value to be classified. |
| 49 | Pairwise | Codec×depth/color, color×output, backend×decode, output×encode observed/missing scenario pairs reported; no percentage target or full Cartesian claim. |
| 50 | Error audit | Existing typed failure model retained; diagnostics wrap missing probe/planning stages locally. No vendor codes or broad refactor. |
| 51 | Defaults | CLI tests confirm preserve default and valid/invalid codec/depth/dynamic-range requests. Optional diagnostic overwrite cannot relabel a committed conversion. |
| 52 | Intel smokes | SDR, PQ preserve, PQ→SDR8, PQ→SDR10: 300 frames each, correct full GPU initialized path and complete decode-back. |
| 53 | Production regression | All 51 artifacts match retained outputs and repeat identically; all established media tiers and real H.264 production pair pass. No render/codec/shader behavior changed. |
| 54 | SPIR-V | 964 actual modules, zero failures with spirv-val --target-env vulkan1.3. Census SHA-256 e164841f04014a24948eff5c2061104fb3130bdf7371f6a291452ac3f5fdf844. |
| 55 | Static checks | Release workspace build; default and encode-characterization workspace tests/clippy; fmt/diff checks pass. Harness 15, HDR→SDR oracle 11, PQ oracle 4 controls pass. |
| 56 | Remaining corpus gaps | Listed below; no 5.4B implementation started. |
| 57 | Final stage status | SEALED; no remaining required gate. Coverage gaps below remain unqualified, not silently promoted. |

## Honest coverage boundaries

All 13 representatives are MP4. This stage does not qualify Matroska or another
container, another hardware stack, all profile combinations or arbitrary
resolutions. All entries are CFR; one B-frame/reordered input is present, but VFR,
nonzero start PTS, unusual time bases/durations, broader GOP and real-world
metadata variations are not systematically covered.

There are two single-AAC-track input records and no multi-audio or subtitle
representatives. Their video conversions disable audio; they do not establish new
passthrough/metadata-preservation evidence. Existing audio, FreeType, safe-output,
cancellation, fault, Validation and FD lifecycle evidence remains in the sealed
Stage 2/5.0/5.3B/C reports and current portable tests. No new 3000-frame soak or
performance claim is made. Current C-3/C-4A reference commands use Validation;
their individual reports are retained with the run.

Initialization/runtime-failure outcomes, mutation frame-index/failure and
corruption types/offsets are representable but have no representative execution
in this initial corpus. External-reference records are representable but never
automatically downloaded. Multi-audio, subtitles, VFR, corruption, midstream
changes and broader real-media/device coverage are future 5.4B work.

Historical missing-input hashes and historical H.264 build-sensitive failures
remain unchanged, not PASS. Current artifact equality cannot retroactively
transfer historical executable qualification. Explicit metadata preservation
contracts remain those of the sealed stages, not every field expressible in the
new schema.

## Reproduction

See [compatibility-corpus.md](compatibility-corpus.md) for mode and safety details.

```bash
cargo build --release --workspace
python3 tests/corpus/run.py full --retained \
  --output /tmp/asciiflow-stage54a-new --device /dev/dri/renderD128
```

An optional identity-checked generated-input cache reduces redundant generation;
it is never required for reproduction. Run directories, logs and large media
remain temporary; the closure snapshot preserves report/environment/artifact
identities without duplicating the existing large golden oracle.

Stage 5.4B Real-Media Compatibility Corpus is justified. It has not started.
