# Regression testing

Toolchain/driver changes use the [stack-scoped portability workflow](portability-testing.md).

Current milestone: [AsciiFlow 2.0.0 FINALIZED](asciiflow-2.0.0-final.md).
Stage 5.4, including D-1 and D-2, is SEALED. The sections below preserve staged
qualification history; older NOT SEALED/NOT STARTED statements describe those
recorded runs, not the current milestone. Historical failures are not rewritten
as passes. See the final receipt and its linked closure reports for current
decisions.

## Historical Stage 5.4D-1 reliability hardening (NOT SEALED at that point)

The user accepted the unresolved default-memory plateau limitation on 2026-10-08
and authorized [D-1B long-run work](stage5.4d1b-long-run.md). This prospectively
removes that item as a progression blocker; it does not change D-1A's historical
NOT SEALED/Unresolved result or mark the failed memory gate PASS. Do not repeat
the plateau campaign. D-2 remains outside the authorized next-stage scope.

The [reliability contract](reliability-testing.md),
[D-1 ledger](stage5.4d1-soak-failure-hardening.md) and
[`stage54d1.json`](../tests/soak/stage54d1.json) distinguish native mux replay,
portable failure-model tests and unexecuted production long-run gates. Generated
inputs, short GPU preflights and historical C receipts are not D-1 qualification.
Stage 5.4D-2 is not started.

The [D-1A observation/failure report](stage5.4d1a-resource-failure.md) and
[`stage54d1a-resource-failure.json`](../tests/soak/stage54d1a-resource-failure.json)
track the separate short/mid-run campaign. Use the actual opt-in
`reliability-measurement` feature; unknown driver/native internals remain
unavailable, sampled queue peaks are not exact high-water marks, and 10k
results must not be described as 100k qualification. Keep input generator/tool/
command/SHA, binary and source identity, output command/SHA or existing structured
oracle, raw resource samples, failures and cleanup outcomes for every new run.

The subsequent [native closure receipt](../tests/soak/stage54d1a-native-closure.json)
uses an immutable dirty-source checkout for both clean builds and actual GPU
execution. Keep the source file map, binary/test identities and runtime selectors;
document/evidence-only updates after qualification do not retrospectively change
the qualified checkout. Build, test relinking and runtime artifact copies must
not be assigned the same hash without checking. A failed resource alarm remains
a failure until investigated; allocator-tuned diagnostics are not substitutes
for the default runtime gate. D-1A currently remains NOT SEALED: the final ten
normal-production 50-cycle processes pass tracked lifecycle checks, but residual
private-anonymous growth remains Unresolved (including a 652 KiB healthy-cycle
48 step). The separate exact H.264 Tier 1B-P blocker is closed; A14 is not proved.

The frozen-source pass now has all 17 retained paths × three runs, 36 native
SIGINT cases, the dual-AAC 10k path and both independent clean-build suites.
That historical pass left default same-process RSS and strict alternate H.264
audio/B-frame comparison unresolved. The subsequent
[A/B closure receipt](../tests/soak/stage54d1a-final-closure.json) is the current
decision source; the [original receipt archive](../tests/soak/evidence/stage54d1a-resource-failure-historical.json)
preserves the exact original bytes and SHA before adding its follow-up pointer.
A per-stack runtime PASS or decoded-pixel match does
not waive a failed coded-packet oracle. The separate D-1A Tier 1B-P rule now
recognizes only the attested identifier-SEI semantic class, without the former
video-only fixture restriction; it does not promote other unqualified stack
profiles or relax the historical/same-stack oracle. Independently passing fault tests
after an aborted portability campaign do not create an aggregate campaign PASS.

RSS qualification must separate normal-production residency from opt-in Vulkan
Validation residency. Keep required-validation lifecycle runs, but do not use
their RSS as the default production result. The fixed mixed-cycle native observer
rejects allocator-tuning environment variables, retains only four fixed output
references and streams evidence instead of accumulating it. Preserve the old
64 MiB review alarm and its failure; do not substitute another absolute magic
number. A plateau decision requires independent full-cycle processes, per-job
resource/FD/thread/staging balance, smaps attribution and successful post-failure
jobs. Report late-window changes and residual uncertainty, not just one delta.

## Stage 5.4C2 expanded portability matrix

The five-stack matrix is specified in the
[matrix report](stage5.4c2-expanded-portability-matrix.md) and
[`c2-matrix.json`](../tests/portability/c2-matrix.json). FFmpeg 8.1.3 is the
canonical reference; the controlled variants are FFmpeg 8.1.2 and 8.1.1,
ANV 26.0.3 against 26.2.3, and iHD 25.4.6 against 26.1.5. These runs are
exploratory until their evidence is reviewed and the
[qualified-stack registry](../tests/portability/qualified-stacks.json) is
updated. Native driver probes reporting all capabilities ready are exploratory
observations, not qualification evidence by themselves.

Use the stack manifest v2 evidence of the actual libraries loaded by the
calling process, including `LD_DEBUG` loader evidence captured during
initialization. Alternate toolchains belong under a new target prefix and run
with a stack-scoped child environment; do not install them into host locations
or use global loader/driver configuration. Existing canonical artifacts remain
exact-stack scoped. The kernel edge is `UnavailableSafely`: checking another
installed kernel requires rebooting the active session, and reboot is not
authorized. Current sealing and qualification decisions are recorded in the
[C-2A forensic report](stage5.4c2a-h264-ihd2546.md), not inferred from a runner's
exit status. C evidence does not imply D-1 reliability qualification.

H.264 cross-driver investigation first freezes the historical oracle and media.
`tests/portability/h264-ihd-forensic.py` records ten conversions per driver/route,
actual initialized driver files, and opt-in measurement-build encoder-boundary
NV12 Y/UV SHA-256 and AVFrame/configuration evidence. `encode-characterization`
with `ASCIIFLOW_ENCODER_CAPTURE_DIRECTORY` downloads the actual submitted surface
for evidence only: it never replaces it and fails explicitly on evidence errors.
The encoder-only ignored test replays the retained NV12 without decoding,
ASCII/Vulkan processing, audio or muxing. Preserve both raw captures and their
durable hashes; a missing raw capture is not a reconstructed historical input.

The independent `h264_driver_portability` test adds Tier 1B-P only for an
attested single-iHD edge with unchanged libav/ANV/source. Its exact known SEI
fingerprint permits only the matched driver identifier, not a Lavc change,
unknown SEI, new NAL or any SPS/PPS/VCL difference. All AU/GOP/timing/color and
container bytes outside the one proven packet remain exact. Legacy Tier 1B
FAIL remains visible; same-stack Tier 1A/1B/1C/2/3 stays strict. Negative tests
must reject color, timing, SPS/PPS/VCL and unrelated container mutations without
depending on decoded pixels to detect coded semantic changes.

Stage 5.4C-1A mux closure uses [the unchanged strict packet-order gate](stage5.4c1a-deterministic-mux.md).
Historical A/B captures are immutable and can be materialized with
`python3 tests/portability/mux-determinism.py materialize-captures --output target/mux-captures`.
Mux qualification requires 100 arrival-perturbed repetitions, the 30k-packet
stress, strict Tier 1A/1B/1C/2 and same-stack Tier 3, and 10 full production
repetitions. Never substitute per-stream equivalence for global packet-order
identity or remove bounded intermediate flushes. After a fix, rerun the 17×3
retained pack and both same-source 24-case stacks. The audio reader reopens the
same immutable local input; no audio decode/transcode is introduced. Report RSS
separately from compressed-packet bounds and MP4 sample-table growth. Traces
are opt-in and absent from the normal build. Retain input/generator identity,
native packet capture, commands, loaded-stack identity, write/flush decision
hashes, raw traces, output hashes, oracle reports and lifecycle evidence.

Capture both environments against identical source/lockfile/fixtures, run the
portability core set through the existing corpus runner, and classify every
difference before promoting stack evidence. Same-stack deterministic gates
remain strict; cross-stack SHA drift is diagnostic, never sufficient for either
regression or success. The canonical retained baseline is not overwritten.

## Stage 5.4B real-media compatibility and evidence retention

Use the [real-media corpus](compatibility-corpus.md), its
[stage report](stage5.4b-real-media-compatibility.md), and the checked-in
[`stage54b-closure.json`](../tests/corpus/stage54b-closure.json) receipt.
The corpus's exact sources/requests qualify only those tuples. New containers,
VFR fidelity, missing strict 10-bit metadata and broad driver/toolchain
portability are not inferred from planner acceptance or successful conversion.

Future compatibility runs, benchmarks and retained outputs must preserve:
generator source/SHA and version, exact generation command, tool/package/source
identity and configuration, input filename/size/SHA, complete actual probe,
conversion command/configuration, executable/source/lockfile/host identity,
output SHA or the structured nondeterministic oracle, and typed failure/watchdog
records. Save durable receipts inside the repository; `/tmp` is scratch space,
not the sole evidence store. Logs/media may live under a fresh ignored `target`
directory, but checked-in receipts must retain their SHA, exact recipes and
decisions so their absence never turns into guessed historical evidence.

Compare timing as exact rational seconds with explicit source/output tick
bounds. Video-only output is zero-origin CFR, one output per decoded input;
audio-copy requires the original CFR video grid and preserves its origin.
VFR-only retiming is characterized, not presentation fidelity. Keep strict
packet/audio and historical H.264 semantic oracles unchanged; never automatically
rewrite a golden record or relabel an Unqualified limitation PASS.

Stage 5.4A adds a [compatibility corpus and unified runner](compatibility-corpus.md)
for the [versioned production support contract](production-support.md). Its
[closure record](stage5.4a-support-contract.md) is SEALED with durable
run/environment/artifact identities. `python3 tests/corpus/run.py full --retained --output
/tmp/asciiflow-corpus-new` selects the full workspace/reference checks and all
17 retained production paths × three runs. The directory must be fresh;
inaccessible hardware is reported as SKIPPED for the current environment.
Existing baseline records and their historical build scope remain unchanged.

## C-4B HDR→SDR production closure

[C-4B](stage5.3c4b-hdr-to-sdr-production.md) and Stage 5.3C overall are **SEALED**
on the recorded Intel MTL/iHD/ANV scope. Explicit `--output-dynamic-range sdr`
is production; `preserve` remains the default. The legal C3 fixture identities
are pinned before conversion. Both inputs × five outputs × three runs pass
all 300-frame packet, decoded-pixel, timing, depth/profile, container/elementary
BT.709 limited/left and HDR-side-data-absence checks. Full framehash uses
`-xerror`, so recoverable decode errors cannot masquerade as a clean decode.

The [canonical baseline](../tests/baselines/media/hdr-to-sdr-production-v1.json)
retains all five tiers and complete pre-run source/binary/kernel/package/input
identity, including untracked runtime sources. This is build provenance plus
observed execution, not proof that an arbitrary foreign executable was built
from the inspected checkout. Baselines are established only through reviewed
closure; normal regression never overwrites or silently drops a golden tier:

```bash
ASCIIFLOW_VULKAN_VALIDATION=1 bash tests/baselines/media/generate-hdr-to-sdr-production-v1.sh \
  tests/fixtures/codecs /tmp/asciiflow-c4b-new-run
python3 tests/baselines/media/verify-hdr-to-sdr-production.py verify \
  /tmp/asciiflow-c4b-new-run /tmp/asciiflow-c4b-regression.json \
  --input-directory tests/fixtures/codecs \
  --check-baseline tests/baselines/media/hdr-to-sdr-production-v1.json
```

An established Tier3 gate requires its exact recorded build scope. A scope
change fails explicitly and needs a reviewed replacement record; repeatability
within a changed batch alone is not retained regression PASS. Preserve input
generator/version/exact command/SHA, output command, output SHA or structured
nondeterministic oracle, native tool/driver identity and source/binary identity
for every future benchmark. Keep source identity, pre-encode parity, lossy
decode-back, repeated-output evidence and historical failed gates distinct.

Actual C4B system and two 3000-frame encode/decode/resource stresses are opt-in,
not inferred from ignored entries in the workspace suite:

```bash
ASCIIFLOW_C4B_PRODUCTION=1 cargo test -p asciiflow-cli --release \
  --test c4b_production -- --ignored --nocapture --test-threads=1
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop --release \
  --features hdr-to-sdr-qualification --test c4b_initialization_faults \
  -- --ignored --nocapture --test-threads=1
ASCIIFLOW_C4B_STRESS=1 ASCIIFLOW_VULKAN_VALIDATION=1 \
ASCIIFLOW_C4B_STRESS_DIR=/tmp/asciiflow-c4b-new-stress \
  cargo test -p asciiflow-interop --release --features hdr-to-sdr-production \
  --test c4b_production_stress -- --ignored --nocapture --test-threads=1
cargo test -p asciiflow-media --release \
  sdr_hardware_output_matrix_encode_and_mux_failures_preserve_roots \
  -- --ignored --nocapture --test-threads=1
```

Serialize GPU/FD tests. Dedicated timing follows correctness, with no competing
GPU or oracle jobs and Validation off. Build with
`asciiflow-cli/encode-characterization`; use the checked-in
`measure-hdr-to-sdr-production.sh` and `summarize-hdr-to-sdr-performance.py`.
Whole-process wall FPS includes probing/setup/teardown; pipeline FPS does not.
Current production GPU render timing aggregates linear rendering, Method A,
limiting and packing; unavailable individual/import/wait zero fields are not
zero-work claims. No comparison with cached-reference FPS implies a production
percentage speedup. The 964-module all-cache Vulkan1.3 census replaces no old
shader evidence; 740 remains the historical C4A cache count.

## C-4A output-signal qualification

[C-4A](stage5.3c4a-sdr-output-signal.md) is **SEALED** on the recorded Intel hardware. Its independent f64
signal oracle uses BT.709 NCL, limited NV12/P010, half-up rounding and
pre-quantization 2×2 nonlinear chroma averaging. Native color metadata and
production legality remain untouched. Use the `hdr-to-sdr-qualification` feature
for the downstream GPU/surface harness; no encoder or final-video baseline is
part of this stage. Final results and historical failed experiments are retained separately
in `tests/baselines/tone-map/stage53c4a.json`.

```bash
cargo test -p asciiflow-cpu sdr_output
bash scripts/qualify-sdr-output-vulkan.sh /tmp/asciiflow-c4a-new-run
```

The runner needs a new directory and accessible real `/dev/dri`; it executes
boundary/canonical/surface/fault/alternating/3000-frame-per-format qualification
before isolated timing. It does not run retained production regression scripts
or workspace/SPIR-V checks; those separate closure commands and source/evidence
hashes are in the ledger. Final pack parity is byte-exact on identical C-3 f32
inputs, not a relaxed end-to-end f64 threshold. C-4B now separately qualifies
native output metadata and production integration. The C-4A report retains
its historical stage-only 56-item closure ledger.

## C3B precision contract review

Stage 5.3C-3 is **SEALED** under P2/N3 on the recorded Intel hardware. See the
[45-item closure ledger](stage5.3c3-vulkan-hdr-to-sdr.md). Historical UNORM16
failures remain failures; that stage's isolated evidence is not itself
production eligibility. C-4B is the downstream production authority.

[Numerical qualification](numerical-qualification.md) audits the historical
≤2 UNORM16 threshold, adopts an independently derived N3 10-bit diagnostic
budget, and records M0/M1/M2/M3 semantics. Historical failures remain preserved.
C-3 does not select a production format; C-4A packing remains separately sealed
as qualification only, with production format selection in C-4B.
Optional FP64 modules are
excluded from the ordinary generated set; normal devices never enable Float64.
Every cached module, including experiments, requires Vulkan1.3 validation.

```bash
bash scripts/qualify-tone-map-cpu.sh /tmp/asciiflow-c3b-c1
bash scripts/qualify-target-volume-cpu.sh \
  /tmp/asciiflow-c3b-c1/method-a-run1.bin /tmp/asciiflow-c3b-c2b
ASCIIFLOW_VULKAN_VALIDATION=1 \
C1_INPUT=/tmp/asciiflow-c3b-c1/linear-bt2020-1000-v1.bin \
C1_OUTPUT=/tmp/asciiflow-c3b-c1/method-a-run1.bin \
C3_PRECISION_DIR=/tmp/asciiflow-c3b-precision \
  cargo test -p asciiflow-vulkan --release \
    --features hdr-to-sdr-fp64-experiment --test c3_precision \
    canonical_precision_sweep -- --ignored --nocapture --test-threads=1
ASCIIFLOW_VULKAN_VALIDATION=1 C3_PRECISION_DIR=/tmp/asciiflow-c3b-power-v2 \
  cargo test -p asciiflow-vulkan --release \
    --features hdr-to-sdr-fp64-experiment --test c3_precision \
    independent_power_edges -- --ignored --nocapture --test-threads=1
ASCIIFLOW_VULKAN_VALIDATION=1 \
C3_LEGAL_FIXTURE_DIR=/home/eureka/Documents/AsciiFlow/tests/fixtures/codecs \
C3_PRECISION_REAL_REPORT=/tmp/asciiflow-c3b-selected-real.json \
  cargo test -p asciiflow-interop --release \
    --features hdr-to-sdr-qualification --test c3_precision_real \
    c3_precision_real_full_frame_diagnostics -- --ignored --nocapture --test-threads=1
ASCIIFLOW_VULKAN_VALIDATION=1 C3_STRESS_REPORT=/tmp/asciiflow-c3b-stress.json \
  cargo test -p asciiflow-interop --release \
    --features hdr-to-sdr-qualification --test c3_precision_real \
    c3_selected_dual_slot_3000_frame_stress -- --ignored --nocapture --test-threads=1
python3 scripts/test-c3b-precision-diagnostics.py
jq '[.cases[]|.path as $p|.vectors[]|{path:$p,xy,channel,cpu,gpu}]' \
  tests/baselines/tone-map/c3b-frozen-outliers.json | \
  python3 scripts/c3b-precision-diagnostics.py --details
```

Use fresh output paths: these reports/captures use exclusive creation. A
successful mixed-precision exploratory test means capture/validation completed,
**not** passage of the historical numerical gate or C3 sealing. The selected
canonical/real tests assert N3; neither alone seals C3. Rust distributions use
`ceil((N-1)*p)`; the Python tool
uses nearest rank. Histograms allow recomputation of either. Double intermediate
observations are only f32 casts, not captured binary64 bits.
Current real-source qualification uses selected f32 mode262 and all 300 VAAPI/DMA-BUF frames per
codec; the frozen CPU reference consumes a downloaded copy of the same frame.
Input domain qualification is not renderer/conversion parity. Do not run
3000-frame stress or performance qualification before the precision policy
decision and applicable numerical/parity gates. Earlier ordinary-mode0 evidence
is historical diagnostic data, not selected-configuration parity.

After both stress gates pass, performance runs separately with validation off:

```bash
ASCIIFLOW_VULKAN_VALIDATION=0 \
C3_FULL_PERFORMANCE_REPORT=/tmp/asciiflow-c3b-performance.json \
  cargo test -p asciiflow-interop --release \
    --features hdr-to-sdr-qualification --test c3_performance \
    c3_full_host_input_performance -- --ignored --nocapture --test-threads=1
```

This measures 300 complete CPU reference frames and 300 Vulkan frames per mode,
with 5 warmup batches excluded. Input is the checksum-verified legal HEVC first
frame downloaded once and repeated as cached host P010. Full map/A/B/C and GPU
final/cell/counter readback are included; decode/download/DMA-BUF/encode are not.
Both maps finish before either dual-slot A/B/C submit; throughput uses whole
batch wall, not summed overlapping per-frame intervals. Timed validation is
disabled and is not credited as a Validation PASS. The separate real stress
provides actual VAAPI/DMA-BUF and enabled-validation evidence.

## Historical C3/C3A diagnostics; current closure uses C3B N3

[The C-3 report](stage5.3c3-vulkan-hdr-to-sdr.md) and
[machine record](../tests/baselines/tone-map/stage53c3.json) retain an actual
strict-gate failure separately from the C3B-derived N3 contract. Historical original B+C
max UNORM16 error 10 (7 samples above 2), isolated C max 11 (4 above 2).
C3A's 18 actual arithmetic experiments improve the best B+C result to 4 (5 above
2) and isolated C to 3 (1 above 2), but **all fail**. Exact masks in the best
compensated C comparison do not waive final-code failures. Keep original f64,
true per-operation f32 and actual GPU observations, plus f32-input-promoted
f64 comparisons and exact failing coordinates. Do not widen thresholds, snap
outputs or modify sealed C-1/C-2B to make GPU tests pass. The independently
derived, reviewed C3B N3 contract is a methodology correction, not an empirical
widening of the old diagnostic to four codes.

```sh
bash scripts/qualify-tone-map-vulkan.sh /tmp/asciiflow-c3-fresh-qualification
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop \
  --features hdr-to-sdr-qualification --test c3_hardware \
  -- --ignored --nocapture --test-threads=1
C3_LEGAL_FIXTURE_DIR=/absolute/path/to/canonical-fixtures \
C3_LEGAL_REPORT=/absolute/path/to/NEW-legal-domain.json \
  cargo test -p asciiflow-interop --release --features hdr-to-sdr-qualification \
  --test c3_legal_domain -- --ignored --nocapture --test-threads=1
ASCIIFLOW_VULKAN_VALIDATION=1 C1_INPUT=/absolute/path/to/linear-bt2020-1000-v1.bin \
C1_OUTPUT=/absolute/path/to/method-a-run1.bin \
C3_ARITHMETIC_REPORT=/absolute/path/to/NEW-arithmetic.json \
  cargo test -p asciiflow-vulkan --release --features hdr-to-sdr-qualification \
  --test c3_arithmetic canonical_arithmetic_sweep -- --ignored --nocapture --test-threads=1
python3 scripts/audit-c3-spirv.py /absolute/path/to/ACTUAL/bt2020_to_bt709_limit.spv
cargo test --workspace --features asciiflow-media/encode-characterization,asciiflow-interop/hdr-to-sdr-qualification
cargo clippy --workspace --all-targets --features asciiflow-media/encode-characterization,asciiflow-interop/hdr-to-sdr-qualification -- -D warnings
```

Use a fresh output directory and real GPU device access. The historical C3A
wrapper exited nonzero at the old canonical hard gate after preserving its
report. The explicit arithmetic sweep above retains that failed diagnostic;
the current wrapper instead asserts N3, full selected parity, stress and finally
performance, failing closed on any required gate. Synthetic render/domain/fault
tests are independent of the historical failure. Capture stores raw
R8 coverage bytes, avoiding false atlas mismatches from fraction normalization.
No ignored test counts as a PASS.

B-3's preserved real canonical source exceeds 1000 nits. Actual HEVC/AV1 import
tests prove correct full-frame rejection, not selected-valid-subset parity.
Keep that input identity unchanged; qualify a separate, reproducible legal-domain
source before full real C-3 closure. The new
[C3 legal-domain PQ v1](../tests/fixtures/codecs/c3-pq-legal-v1.md) source has now
passed all-300-frame software/VAAPI unclamped RGB-domain and timestamp audits for
both codecs; it is separate from B-3. The test checks actual input SHA256 before
decoding, and a changed-bytes negative control was rejected before qualification.
Full legal-source CPU/GPU parity, both 3000-frame stresses, FD restoration,
resource-growth checks and 1080p/300-frame performance were unqualified at C3A;
the C3B evidence above closes them under N3. C-4A qualification is sealed; C-4B is justified, not implemented or
started; production integration is not authorized by this qualification alone.

Every future qualification/benchmark must retain source generator/version,
exact command, input SHA256, feature/config/output command, toolchain and device
identity, and output hashes or a structured nondeterministic oracle. A failure
record must distinguish observations, approved gates and unrun requirements.
Enumerate every actual Debug/Release SPIR-V cache file and validate Vulkan1.3;
old file counts are not targets. Current 524 cache files pass, with no Float16/Float64
capabilities in the three new C-3 modules.

## C-2B primary conversion and target-volume CPU reference

[C-2B closure](stage5.3c2b-target-volume-cpu.md) is the replacement f64 CPU
reference, not a repair/implementation of Annex5 and not production HDR→SDR.
Keep transfer, separate XYZ colorimetry and target clipping distinct in tests.
Every canonical C-1 pixel is processed, including negative/>1 excursions;
no source preclip or selected-valid subset is allowed.

```sh
cargo test -p asciiflow-core --test sdr_target_volume
cargo test -p asciiflow-core sdr_target_volume
cargo test -p asciiflow-cpu target_volume
python3 tests/fixtures/tone-map/test-c2b-audit.py
python3 tests/fixtures/tone-map/generate-c2b-vectors.py > /tmp/c2b-vectors.json
cmp /tmp/c2b-vectors.json tests/fixtures/tone-map/c2b-vectors.json
python3 tests/fixtures/tone-map/generate-c2b-vectors.py --rust-table > /tmp/c2b-vectors.tsv
cmp /tmp/c2b-vectors.tsv tests/fixtures/tone-map/c2b-vectors.tsv
bash scripts/qualify-tone-map-cpu.sh /tmp/asciiflow-c2b-c1-regression
bash scripts/qualify-target-volume-cpu.sh \
  /tmp/asciiflow-c2b-c1-regression/method-a-run1.bin /tmp/asciiflow-c2b-target-volume
cmp /tmp/asciiflow-c2b-target-volume/c2b-target-volume-audit.json \
  tests/fixtures/tone-map/c2b-target-volume-audit.json
```

Use **fresh output directories**: qualification never truncates an existing
output, including symlink/hardlink aliases of C-1. The wrapper checks the sealed
C-1 identity, three output byte equality and retained `C2B-SHA256SUMS` before
the full-frame audit. Preserve C-1's original identity/SHA256SUMS separately.
Retain exact generator/wrapper commands and hashes, toolchain/libm identity,
independent vector identity, input SHA, explicit-endian output field order and
three output SHA or structured oracle. Byte portability across a new libm or
compiler is not claimed: qualify numerical vectors before replacing an oracle.
Boundedness, exact clip, idempotence, finite rejection, interior identity and
strict preclip XYZ/Y error are gates. ΔY, u′v′, hue/chroma and sampled many-to-one
collisions are **loss characterization**, not invented quality/visual gates.
Continue all five retained SDR and both PQ production baselines (three runs),
decode-back/metadata, default-eight-bit PQ/HLG/BT.2020-SDR/full-range rejection,
both workspace/static modes and every actual SPIR-V Vulkan1.3 module. Historical
H.264 raw1A remains FAIL; semantic1B/decoded1C/structure2 PASS is not byte PASS.

## C-2A domain and standard clarification

[C-2A](stage5.3c2a-domain-standard-clarification.md) is SEALED as OutcomeC/F3:
Annex5 is abandoned as an executable reference; C-2B implements the distinct
replacement primary conversion + target-volume policy above.
No derived formula, selected-valid subset or clipped C-1 image is a gamut PASS.
Preserve the original C-2 audit and C-1 baseline, including its source/Y failures.
The D5+D6 next-reference policy is a project design using standard-allowed
operations, not a repair or production fallback.

```sh
python3 tests/fixtures/tone-map/test-c2a-audit.py
python3 tests/fixtures/tone-map/audit-c2a.py taxonomy \
  /tmp/asciiflow-c2a-c1-regression/method-a-run1.bin > /tmp/c2a-taxonomy.json
cmp /tmp/c2a-taxonomy.json tests/fixtures/tone-map/c2a-taxonomy.json
python3 tests/fixtures/tone-map/audit-c2a.py formula > /tmp/c2a-formula.json
cmp /tmp/c2a-formula.json tests/fixtures/tone-map/c2a-formula-audit.json
```

Generate the three sealed C-1 binaries with the wrapper below before these
checks. Repeat taxonomy for runs2/3; all report bytes must match. The signed-power
taxonomy is an explicitly labelled sensitivity experiment only; negative-Y
classification under the unchanged C-1 contract remains unknown. General
feasibility checks some legal target RGB at desired Y, not hue/chromaticity or
an algorithm. Formula tests retain the printed contradiction and mark derived
Bézier as NOT normative. Record primary-source availability and literal fields
in the source ledger, with `null` for inaccessible equations; never fill them
from a downstream report or third-party code. Continue normal and
`asciiflow-media/encode-characterization` workspace/clippy modes, all actual
SPIR-V, unchanged C-1/media/rejection gates. At C-2A no new output baseline
existed; C-2B's separately named output must not replace the C-1 oracle.

## Preserved C-2 blocked entry-domain audit

[Original Annex5 C-2](stage5.3c2-gamut-map-cpu.md) is NOT SEALED / ABANDONED. Before coding the Annex5
oracle, the mandatory C-1 source audit demonstrated empty target effective
gamuts at Y>1 and an inconsistency in printed Eq5-4. Do not qualify a clamped
or selected-valid subset as full C-1→C-2 integration.

```sh
bash scripts/qualify-tone-map-cpu.sh /tmp/asciiflow-c2-c1-regression
python3 tests/fixtures/tone-map/audit-c2-domain.py \
  /tmp/asciiflow-c2-c1-regression/method-a-run1.bin > /tmp/c2-domain-audit.json
cmp /tmp/c2-domain-audit.json tests/fixtures/tone-map/c2-domain-audit.json
```

This successful audit command records NOT SEALED, not a gamut PASS. It validates
the retained C-1 input digest/serialization and counts all RGB excursions;
Decimal70 independently characterizes positive-only Y counterexamples. No
negative transfer/preclamp/new tone curve/gamut mapping is run. Continue existing
production/hash/rejection gates unchanged. The former BLOCKED pending C-2A
decision is now resolved by method rejection/reselection, not an Annex5 PASS.
No gamut vectors, containment, integration or C-2 mapped-output digest are
claimed. No C-3 work is authorized by this result.

## Internal Method A CPU reference (Stage 5.3C-1)

The [closure report](stage5.3c1-tone-map-cpu.md) and [semantics](tone-mapping.md)
define the fixed f64 1000→100-nit post-ASCII BT.2020 oracle, not a production
HDR→SDR backend. The canonical architecture is `docs/architecture.md`, not the
retired versioned filename.

```sh
cargo test -p asciiflow-core tone_map_bt2446
cargo test -p asciiflow-cpu tone_map
bash scripts/qualify-tone-map-cpu.sh /tmp/asciiflow-c1-reference
python3 tests/fixtures/tone-map/generate-method-a-vectors.py > /tmp/method-a-vectors.json
cmp /tmp/method-a-vectors.json tests/fixtures/tone-map/method-a-vectors.json
```

Retain generator/wrapper identity, exact command, source SHA-256, compiler/libm
identity, output digest or structured oracle and diagnostics. The wrapper checks
three explicit-endian f64 outputs against each other and retained SHA256SUMS.
A new toolchain requires numerical-vector qualification before a new byte
baseline; never silently refresh the oracle. Timing excludes source generation
and serialization and has no hard target. No quantization is added.

Builtin/FreeType integration must preserve B-1 glyphs, coverage and linear
composition. Negative/nonfinite/>1000 input rejects qualification; raw Method A
output excursions remain visible. Metadata and previous frames do not control
the operator. Continue all five SDR v2 and both PQ production baselines, unchanged
historical H.264 oracle/rejection policies, normal/measurement checks and all
actual SPIR-V Vulkan 1.3 validation. CPU parity is not Intel DMA-BUF evidence.

The default workspace suite covers portable planning, codec policy, failure
semantics, media fixtures, fonts, and CPU processing. Real Intel VAAPI/Vulkan
tests are opt-in because they require a qualified device, drivers, and sometimes
a retained long-form input. Stage-specific measurements and hardware outcomes
live in their validation reports.

## Media regression policy

The default ASCII ramp was corrected from dense-to-sparse to sparse-to-dense
for black-background rendering. This intentionally changes the processed
pixels for default `standard` and `detailed` output. All pre-correction
retained output hashes and the Stage 5.3B-1 five-case comparison remain
historical evidence for the old polarity; they are **not** current-output
regression gates. Keep their values unchanged. Establish replacement output
baselines only after rerunning the production paths and recording the new
source/build identities. Explicit literal `--charset` values are unchanged;
callers should supply them sparse-to-dense for the black-background contract.

Baseline generations are explicit. [v1](../tests/baselines/media/stage53b1.json)
is the historical pre-polarity five-output record, retained for the H.264
`.102`/`.103` forensic oracle and methodology tests; none of its decoded
pixels or whole-file hashes gates current default rendering. [v2](../tests/baselines/media/post-polarity-v2.json)
is the post-polarity Intel production record and the current regression gate.
Its 8-bit input is generated twice with the checked-in
[`generate-8bit-production-baseline.sh`](../tests/fixtures/codecs/generate-8bit-production-baseline.sh)
using FFmpeg `8.1.3-1.fc44`; both generations have SHA-256
`6e5c214b813dca3e1db65629b1241cfb663166eb565cda1c48b5ee74ed0dce6b`.
The complete generator command, pinned version check and color parameters
are in that script. The 10-bit input remains checked-in canonical v1, SHA-256
`df69c98ca6592b08c6bc34127b2bb5cf4125c759fd852b830e5426d5818fccda`.
The v2 production script fixes all conversion flags and executes five codecs
three times. Each whole-file hash is exact-build/driver scoped, **not portable**
across FFmpeg, VAAPI, Mesa or AsciiFlow binary changes; Tier 1B/1C/Tier 2
semantic comparison remains the approved cross-tool-version correctness gate.

Whole-MP4 SHA-256 is an exact artifact identity check, not a universal media
correctness oracle. A mismatch always triggers the three-tier comparison below;
it is never silently ignored or automatically called a regression.

1. **Tier 1 — media identity:** Tier 1A reports exact per-stream raw coded
   packet bytes, an exact-build diagnostic. Tier 1B is the cross-approved-patch
   encoded-media gate: packet count/identity, PTS/DTS/duration/flags/side data,
   codec/profile/bit depth/color and audio payload/routing/language/disposition
   remain exact. HEVC and AV1 packet bytes remain exact. For H.264 only, the
   [test-only AVCC/SEI parser](../crates/asciiflow-media/tests/common/h264_bitstream.rs)
   permits the measured `user_data_unregistered` UUID and fixed VAAPI encoder
   identifier to differ solely in a three-digit `Lavc62.28` patch version;
   every other NAL and SEI message stays byte-exact. Tier 1C independently
   compares every decoded visible-plane frame digest, PTS, dimensions and
   format. Tier 1B + Tier 1C are the cross-approved-patch correctness gate.
2. **Tier 2 — container structure:** compare stream count/order, time bases,
   durations, codec parameters and extradata, dispositions and all nonvolatile
   metadata. Cross-stream packet interleave is reported separately as a
   structural difference, not mislabeled as a packet-payload change.
3. **Tier 3 — whole-file SHA-256:** require equality only when the input
   generator/artifact, exact FFmpeg/libavcodec/libavformat and AsciiFlow builds,
   driver qualification scope, and output command are pinned. Keep historical
   hashes as toolchain-bound evidence; do not overwrite them on an upgrade.

The hardware-independent comparator is
`crates/asciiflow-media/tests/common/media_regression.rs`, exercised by the
`media_regression` integration test. It uses FFmpeg's native stream/packet API
and AsciiFlow's software decoder; it does **not** parse command-line probe
text, access `/dev/dri`, or hash `AVFrame` padding. Container-tag volatility is
limited to `format.tags.encoder=Lavf<major>.<minor>.<patch>` and, if present,
`stream.tags.encoder=Lavc<major>.<minor>.<patch>`, with unchanged major/minor.
The H.264 SEI rule is separate; it is not a general packet or metadata
wildcard. Every approval includes packet/NAL/SEI position, UUID and both full
identifiers. SPS, PPS, VCL, unknown/timing/recovery/HDR SEI, extradata,
timestamps, color, decoded pixels and audio remain strict. A supplementary
raw-byte guard rejects unexplained file bytes after native comparison; each
approved container tag or parsed SEI version token authorizes exactly one
same-length byte occurrence. Duplicates in opaque atoms fail closed. AVPacket
payload can contain standardized non-VCL codec metadata; raw packet identity
is therefore stronger than encoded-picture semantic identity. A hash match printed
by the comparator is a byte-equality observation, not independent attestation
of the builds, libraries and driver required for a strict Tier 3 gate. In
particular, an encoder version string *inside a packet* is never treated as a
container tag. The [machine-readable baseline record](../tests/baselines/media/stage53b1.json)
keeps the five input/output identities, observed tier results, conversion
configuration and expected stream summary without checking in thousands of
packet hashes. Candidate files in `/tmp` are local execution artifacts, not
durable retained fixtures.

To compare retained files on any host with the linked FFmpeg libraries:

```bash
ASCIIFLOW_REGRESSION_REFERENCE=/absolute/reference.mp4 \
ASCIIFLOW_REGRESSION_CANDIDATE=/absolute/candidate.mp4 \
  cargo test -p asciiflow-media --test media_regression \
  compare_pair_from_env -- --ignored --nocapture
```

Only after separately attesting exact reference/candidate build identity,
set `ASCIIFLOW_REGRESSION_EXACT_BUILD=attested` on that invocation to require
Tier 1A and Tier 3 equality as well. The comparator cannot infer that identity
from a filename or matching version text. Without that explicit attestation,
it still prints both byte-identity results but gates on Tier 1B/1C/Tier 2.

Candidate generation remains a separate opt-in Intel hardware operation.
After an FFmpeg, Mesa or driver upgrade, run the structured oracle and inspect
every difference. An approved patch-version-only difference may pass Tier 1B,
Tier 1C and Tier 2 without rewriting old Tier 1A/Tier 3 hashes. Exact-build
identity must be independently attested (input, binary, libavcodec,
libavformat, encoder/driver and command); select the strict policy requiring
both raw packets and whole-file SHA for such runs. Matching filenames or
version strings alone do not establish exact-build identity. A new strict
hash baseline requires an explicit decision to pin that toolchain and must
retain the old hash. See the
[Stage 5.3B-1 closure report](stage5.3b1-pq-cpu-reference.md).

Stage 5.3A color tests use the checked-in codec fixtures and run in the normal
workspace suite. They exercise BT.709 SDR, BT.2020 SDR, PQ, HLG, unknown and
contradictory signaling, exact-rational static HDR data, safe-output rejection,
and a synthetic mid-stream SDR→PQ change. On an Intel render node, additionally
run the ignored software/VAAPI color parity check:

```bash
cargo test -p asciiflow-media --test codec_decode \
  hevc_av1_main10_software_and_vaapi_color_semantics_agree -- --ignored --nocapture
cargo test -p asciiflow-media intel_color_source_provenance \
  -- --ignored --nocapture
cargo test -p asciiflow-media --test codec_decode \
  vaapi_rejects_unsupported_color_with_classification_intact \
  -- --ignored --nocapture
```

The [Stage 5.3A hardware closure report](stage5.3a-hardware-closure.md)
records the Intel results under that run's toolchain. The three retained 8-bit
hashes matched at that time. The original
Stage 5.2C-3 10-bit input/hash were not retained; those two old output hashes
are historical references, **not current regression gates**. The replacement
canonical 10-bit baseline v1 is checked in under
`tests/fixtures/codecs/hevc-main10-canonical-v1.mp4`, SHA-256
`df69c98ca6592b08c6bc34127b2bb5cf4125c759fd852b830e5426d5818fccda`.
Regenerate with the exact FFmpeg 8.1.3 command in
`tests/fixtures/codecs/generate-10bit-baseline.sh`; the fixture README records
toolchain and three-run byte reproducibility. (Use the authoritative checksum
in `tests/fixtures/codecs/SHA256SUMS` when validating the file.)

For Intel Arc Meteor Lake, build Release and run the two canonical production
profiles below. The output filename may differ, but all other flags are part
of the regression configuration. Repeat each profile three times and compare
SHA-256; the Stage 5.3A values are HEVC Main10
`07f231ab49eebd54cc0e85012f7dd12e424f580d17005bc20028b458168323fe`
and AV1 10-bit
`2ab82984f690ea3fe5472c874dfce7cc474f7c46266f5cd86ed8deee20b36fda`.

```bash
cargo build --release --workspace
target/release/asciiflow tests/fixtures/codecs/hevc-main10-canonical-v1.mp4 \
  /tmp/asciiflow-canonical-hevc-run1.mp4 \
  --width 80 --font builtin-8x8 --color true --audio none --max-frames 300 \
  --decode vaapi --backend vulkan --vulkan-mapping gpu --encode vaapi \
  --hw-device /dev/dri/renderD128 --vaapi-vulkan-input-interop on \
  --vaapi-vulkan-output-interop on --output-codec hevc \
  --output-bit-depth 10 --no-progress
target/release/asciiflow tests/fixtures/codecs/hevc-main10-canonical-v1.mp4 \
  /tmp/asciiflow-canonical-av1-run1.mp4 \
  --width 80 --font builtin-8x8 --color true --audio none --max-frames 300 \
  --decode vaapi --backend vulkan --vulkan-mapping gpu --encode vaapi \
  --hw-device /dev/dri/renderD128 --vaapi-vulkan-input-interop on \
  --vaapi-vulkan-output-interop on --output-codec av1 \
  --output-bit-depth 10 --no-progress
```

The opt-in
`canonical_v1_main10_full_interop_300_frame_preencode_parity` hardware test
compares all 300 pre-encode P010 frames, byte-for-byte, with a staged reference
for both codecs. It also checks FD return and Vulkan Validation when enabled.
For every future retained benchmark or output regression, preserve the input
generator, generator/tool version and exact command, input SHA-256, full output
command, and output SHA-256 (or a structured packet/timestamp/decoded-pixel
oracle if hardware output is nondeterministic). Do not promote a hash whose
input identity cannot be independently reproduced.

Audio Stage 4.2.1 has three layers:

- A: core policy tests and native packet/queue fault tests, without GPU access.
- B: `cargo test -p asciiflow-cli --test audio_regression`, using small checked-in
  fixtures, explicit software video processing, and native libav inspection.
- C: optional Intel audio parity, validation, and cancellation process test;
  not required by ordinary workspace tests.

Run `cargo test --workspace` for the default suite. No ffmpeg/ffprobe executable or
render node is required at test runtime; the usual linked FFmpeg development/runtime
libraries and software H.264 encoder are required. The helper checks structured
stream fields and exact compressed packet bytes, not human-readable probe output.
It decodes output video and AAC using native libraries as a test oracle only.

Run the 60-second completion stress explicitly:
`cargo test -p asciiflow-cli --test audio_regression long_audio_stream -- --ignored`.
The deterministic bounded-channel unit test stalls its consumer, observes capacity,
and cancels the blocked producer. This proves bounded queue storage, not an RSS
measurement. Child-process tests have a 30-second watchdog and RAII termination.
Linux SIGINT waits for actual staging writes before signalling, then checks status
130 and unchanged destination bytes. File-size limits test real buffered output
failure (which FFmpeg may surface at trailer time). A test-only mux message injects
an audio packet failure after two packets to verify the original MuxRuntime cause.
No production environment-variable fault switch exists.

Fixture generation and checksums are documented in
`tests/fixtures/media/README.md`. Existing Stage 4.1 tiny media fixtures are also
included so a fresh checkout can run its native regression suite.

On an Intel host run
`cargo test -p asciiflow-cli --test audio_regression intel_audio -- --ignored`.
This requires explicit full interop (no auto fallback), enables
`ASCIIFLOW_VULKAN_VALIDATION=1`, compares audio against software, and cancels the
long run. The Stage 4.2.1 report did not include a hardware rerun; that is a
historical stage-specific statement, not a claim about the current host.
Subsequent hardware validation is recorded in
[Stage 5.0](stage5.0-codec-validation.md),
[Stage 5.1A](stage5.1a-hevc-encode-validation.md), and
[Stage 5.1B](stage5.1b-av1-encode-validation.md).

On the qualified Intel host, the AV1 output opt-in tests cover a 30-frame
pre-encode staged/full pixel comparison and 3000-frame surface-reuse/FD stress.
Provide the actual media paths; the tests' default `target/` artifacts are not
checked into the repository:

```bash
ASCIIFLOW_STAGE51B_INPUT=/absolute/path/to/300-frame-input.mp4 \
ASCIIFLOW_VULKAN_VALIDATION=1 \
cargo test -p asciiflow-interop --test hardware \
  av1_encoder_descriptor_and_staged_interop_pixels_are_exact -- --ignored

ASCIIFLOW_STAGE51B_STRESS_INPUT=/absolute/path/to/3000-frame-input.mp4 \
ASCIIFLOW_VULKAN_VALIDATION=1 \
cargo test -p asciiflow-interop --test hardware \
  av1_full_interop_surface_reuse_is_exact_and_fd_bounded -- --ignored
```

Neither a passing portable suite nor lavapipe emulation substitutes for Intel
hardware qualification. Do not run all ignored hardware tests indiscriminately:
some deliberately submit invalid external handles and must run without
Validation, as their test annotations state.

## Internal P010LE processing (Stage 5.2A)

Core and CPU P010LE tests run in `cargo test --workspace`. The opt-in Vulkan
checks use synthetic Host P010LE frames, not a Main10 media decoder. Lavapipe
can verify pixel parity and Vulkan Validation when it provides the required
16-bit storage feature:

Stage 5.2B software HEVC Main10 and AV1 10-bit fixtures are also exercised by
the normal workspace suite. The media integration test verifies 36-frame
decode, P010 padding and real low-bit samples; the interop crate's
`p010_qualification` test runs software decode→CPU ASCII without an encoder.
Run real-media CPU/Vulkan byte parity on a Vulkan 1.3 device explicitly:

```bash
ASCIIFLOW_VULKAN_ALLOW_CPU=1 ASCIIFLOW_VULKAN_VALIDATION=1 \
cargo test -p asciiflow-interop --test p010_qualification \
  software_ten_bit_media_cpu_vulkan_ascii_are_byte_exact -- --ignored
```

On the Intel Arc host, the descriptor, hwdownload and direct P010 input-import
test compares 30 frames per codec and requires `/dev/dri/renderD128`:

```bash
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop --test hardware \
  ten_bit_vaapi_descriptors_and_hwdownload_reference -- --ignored --nocapture
```

For the two 3000-frame P010 direct-input stress cases and the paired Release
benchmark, first create temporary looped inputs from the checked-in 36-frame
fixtures (or set `ASCIIFLOW_STAGE52B_HEVC_STRESS_INPUT` and
`ASCIIFLOW_STAGE52B_AV1_STRESS_INPUT` to equivalent absolute paths):

```bash
ffmpeg -hide_banner -loglevel error -y -stream_loop 84 \
  -i tests/fixtures/codecs/hevc-main10-sdr-gradient.mp4 \
  -frames:v 3000 -c copy /tmp/asciiflow-stage52b-hevc-3000.mp4
ffmpeg -hide_banner -loglevel error -y -stream_loop 84 \
  -i tests/fixtures/codecs/av1-main10-sdr-gradient.mp4 \
  -frames:v 3000 -c copy /tmp/asciiflow-stage52b-av1-3000.mp4
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop --test hardware \
  ten_bit_p010_interop_reuse_is_exact_and_fd_bounded -- --ignored --nocapture
cargo test --release -p asciiflow-interop --test hardware \
  ten_bit_p010_decode_paths_300_frame_benchmark -- --ignored --nocapture
```

Run the benchmark without Validation for interpretable timings. It warms up
36 frames and measures three 300-frame runs for each of software decode,
VAAPI+hwdownload and VAAPI+DMA-BUF input import. The test prints process CPU,
decode, download, CPU import setup, GPU copy/map/render, backend wall and
latency separately. [Stage 5.2B](stage5.2b-p010-decode-validation.md) records
the Intel results and their 64×64 scope.

Stage 5.2C-1 uses an opt-in, encoder-free VAAPI P010 frames pool to qualify
the output transfer on `/dev/dri`. The actual descriptors, parity, FD counts,
Validation results, NV12 control and three-run 1080p output benchmark are in
[its qualification report](stage5.2c1-p010-output-interop.md):

```bash
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop \
  --features p010-output-diagnostic --test hardware \
  p010_output_3000_frame_fd_stress -- --ignored --nocapture
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop \
  --features p010-output-diagnostic --test hardware \
  p010_output_faults_preserve_cause_and_do_not_reuse_surfaces -- --ignored --nocapture
cargo test --release -p asciiflow-interop \
  --features p010-output-diagnostic --test hardware \
  p010_output_300_frame_benchmark -- --ignored --nocapture
```

The deliberately invalid-FD diagnostic should be run separately without
Validation. These C-1 tests are diagnostic, not an encoder; Stage 5.2C-2
qualifies the separate production path.

```bash
ASCIIFLOW_VULKAN_ALLOW_CPU=1 \
ASCIIFLOW_VULKAN_VALIDATION=1 \
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json \
cargo test -p asciiflow-vulkan p010_ -- --ignored
```

The `p010_` filter also includes the opt-in 300-frame 1080p benchmark and
3000-frame reuse test. Run the benchmark in Release for interpretable timing:
`cargo test --release -p asciiflow-vulkan
p010_synthetic_1080p_300_frame_benchmark -- --ignored --nocapture`.
See [the P010 contract and measured scope](p010.md). Production AV1 10-bit
output remains unsupported. HEVC Main10 requires explicit
`--output-codec hevc --output-bit-depth 10` and BT.709 SDR P010 input.

## HEVC Main10 production encode (Stage 5.2C-2)

On the qualified Intel render node, use a 128×128-or-larger true-10-bit
HEVC Main10 BT.709 SDR input. The checked-in 64×64 fixture is deliberately too
small for this driver's Main10 encoder. Set the paths below to retained inputs
or generate them as described in the [qualification report](stage5.2c2-hevc-main10-encode.md):

```bash
ASCIIFLOW_STAGE52C2_HEVC_INPUT=/absolute/path/to/main10-30.mp4 \
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop --test hardware \
  main10_encoder_owned_full_interop_30_frame_parity -- --ignored --nocapture

ASCIIFLOW_STAGE52C2_HEVC_STRESS_INPUT=/absolute/path/to/main10-3000.mp4 \
cargo test -p asciiflow-interop --test hardware \
  main10_encoder_owned_full_interop_3000_frame_stress -- --ignored --nocapture
ASCIIFLOW_STAGE52C2_HEVC_STRESS_INPUT=/absolute/path/to/main10-3000.mp4 \
cargo test -p asciiflow-interop --test hardware \
  main10_staged_encode_3000_frame_fd_stress -- --ignored --nocapture
cargo test -p asciiflow-media \
  injected_main10_send_receive_and_drain_failures_preserve_cause -- --ignored
```

The parity test downloads the real encoder-owned P010 surface before sending
it, compares all 30 frames byte-for-byte to the staged P010 reference, and
checks that active 10-bit low bits remain. It does **not** demand byte-exact
equality from the lossy decoded HEVC stream. The two 3000-frame tests record
FD before/steady/after and decode-back counts. Regular planner/CLI tests cover
explicit depth policy, exact Main10 failure facts, staged auto replan, and
strict interop requests. The report records bitstream, AAC, FreeType,
cancellation, 1080p benchmark, Validation and `spirv-val` evidence.

## Internal Vulkan PQ qualification (Stage 5.3B-2)

This is not production HDR support. The separate `hdr-pq-qualification`
feature exposes the independent f32 Vulkan implementation while the unchanged
CPU f64 reference remains the oracle. See the
[sealed hardware report](stage5.3b2-vulkan-pq.md) for exact architecture,
numeric bounds, raw input identity, benchmarks and scoped limitations.

```bash
bash tests/fixtures/codecs/generate-pq-qualification.sh /tmp/pq-repeat
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-vulkan \
  --features hdr-pq-qualification --test pq_qualification \
  -- --ignored --nocapture --skip pq_1080p_300_frame_benchmark
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop \
  --features hdr-pq-qualification --test pq_hardware \
  -- --ignored --nocapture --test-threads=1
ASCIIFLOW_VULKAN_VALIDATION=0 cargo test -p asciiflow-vulkan --release \
  --features hdr-pq-qualification --test pq_qualification \
  pq_1080p_300_frame_benchmark -- --ignored --nocapture
```

Use the recorded Intel node for DMA-BUF tests. Validation must explicitly be
enabled: a zero counter with the layer disabled is not validation evidence.
Serialize FD tests in-process; loader warmup precedes the exact baseline.
Positive PQ inputs have legal codes and explicit left siting, unlike the
preserved old rejection fixtures. Download the exact VAAPI surface for the CPU
oracle; comparing independent software and VAAPI decodes would confuse decoder
rounding with compute error. All glyphs must match; active Y/U/V deltas must
be ≤1, and every delta >1 must fail. Intermediate gates are characterized and
documented separately, not used to loosen the final gates.

For future raw/generated benchmark inputs, preserve the generating function
and source version/hash, exact config/command, raw-byte SHA-256 and byte size,
in addition to the existing encoded-input/output regression policy. Keep
measurement wall time, parity-check time, per-stage GPU timestamps and overlap
semantics distinct. Simulated cleanup checkpoints must not be described as
real device-loss or hung-fence tests. Historical cache module counts (such as
133) are not a gate: enumerate and validate every currently generated SPIR-V
with `spirv-val --target-env vulkan1.3`, recording count/profile and capabilities.
Normal and measurement-feature workspace tests/clippy must both remain green.

## PQ production closure (Stage 5.3B-3)

The integration implementation is documented in
[stage5.3b3-hdr-production.md](stage5.3b3-hdr-production.md); its status is the
sealing authority, not a capability inferred from P010 or codec identity.
The independent SDR [baseline v2](../tests/baselines/media/post-polarity-v2.json)
remains unchanged. The new [PQ production v1](../tests/baselines/media/pq-production-v1.json)
records actual input/tool identities, strict three-run coded/decoded/whole-file
oracles and explicit conversion configuration. It does not replace SDR or
historical evidence.

The retained canonical HEVC/AV1 PQ inputs both decode exactly to raw-source
SHA-256 `7278abe3ec0b44ac62e1a5ae090df81206a4e9fb82379053885c622f37c3a48f`.
Input SHA-256 values are respectively
`72eadd220051231f85814e7161379f64adf9e472ec7cb33f1564b30b59f2152a` and
`526ddba68a3a4a9c8878a5f93f8a75e556324e99e1b57f30e67d4883c922a71e`.
Three independent generations matched; complete generator/version/per-frame
identities are retained with the fixtures. Actual decoded low-two-bit nonzero
samples total 585270960 / 933120000. Source byte identity, encoded reproducibility,
and emitted metadata are separate checks, not interchangeable evidence.

```bash
bash tests/fixtures/codecs/generate-pq-canonical.sh /tmp/pq-canonical-repeat
python3 tests/fixtures/codecs/verify-pq-canonical.py /tmp/pq-canonical-repeat /tmp/pq-source-check.json
bash tests/baselines/media/generate-pq-production-v1.sh tests/fixtures/codecs /tmp/pq-retained
python3 tests/baselines/media/verify-pq-production.py /tmp/pq-retained /tmp/pq-output-check.json
# A future regression must also match the established canonical baseline:
python3 tests/baselines/media/verify-pq-production.py /tmp/pq-retained /tmp/pq-regression.json \
  --input-directory tests/fixtures/codecs --check-baseline tests/baselines/media/pq-production-v1.json
python3 tests/baselines/media/test-pq-production-oracle.py
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test --release -p asciiflow-cli \
  --test pq_production -- --ignored --nocapture --test-threads=1
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test --release -p asciiflow-interop \
  --features hdr-pq-qualification --test pq_hardware \
  pq_canonical_300_frame_encoder_pool_preencode_oracle -- --ignored --nocapture --test-threads=1
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test --release -p asciiflow-interop \
  --features hdr-pq-qualification --test pq_hardware \
  pq_production_encode_3000_frame_fd_stress -- --ignored --nocapture --test-threads=1
```

Every future HDR retained artifact must preserve both generating scripts,
generator/tool/native build identity, exact input and output commands, input
SHA-256 and real decoded precision evidence. Record output hashes only after
repeatability is measured; otherwise preserve a structured nondeterministic
oracle and do not force an exact-file gate. Keep source/raw-input/frame hashes,
packet identity, decoded pixel hashes and output color metadata distinct.
Source mastering/CLL must be absent from ASCII output. Negative oracle controls
must reject wrong primaries, missing PQ transfer, full range and static leaks.
Use the exact same VAAPI surface for CPU/GPU **pre-encode** parity; lossy output
must not be asserted byte-equal to the CPU renderer. Serialize real GPU/FD tests
and dedicated measurements. The file-size failure test injects its limit only
after actual packet writing, so Mesa initialization is not mistaken for mux
failure. No existing SDR or historical H.264 comparator tolerance is widened.

Stage 5.3B-3 is SEALED on its documented Intel iHD/ANV scope. Its
[hardware record](../tests/baselines/media/pq-production-v1-hardware.json) and
production manifest preserve all-300-frame pre-encode histograms, actual
3000-frame encode/mux/decode FD results, driver/tool identity and dedicated
three-run measurement summaries. The PQ oracle uses the same tier names as
the Rust comparator: 1A raw packets, 1B coded semantics, 1C decoded pixels and
timestamps, 2 structure, 3 exact-build file identity. `--baseline` establishes
an explicitly reviewed replacement record; normal regressions use
`--check-baseline` and never overwrite the golden file. Source generation must
also match its retained identity; repeatability within a changed batch alone
is not a regression PASS. The oracle checks both actual encoded input files'
sizes and SHA-256, not merely an unchanged identity JSON; a negative control
rejects changed input bytes behind a matching identity record.
Native device loss and hung-fence recovery are not
claimed by the simulated host fault checkpoints.

## AV1 Main 10-bit production encode (Stage 5.2C-3)

Use an explicitly tagged BT.709 SDR, left-chroma, 128×96-or-larger P010 input
on the qualified Intel node. The 30-frame AV1 input used below was generated
by the production AV1 10-bit encoder from a true-low-bit HEVC Main10 source;
the 3000-frame variant is a stream-copy loop. This avoids treating an
unspecified-chroma intermediate as a qualified input. See the
[qualification report](stage5.2c3-av1-10bit-encode.md).

```bash
ASCIIFLOW_STAGE52C3_AV1_INPUT=/absolute/path/to/av1-main-10bit-30.mp4 \
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop --test hardware \
  av1_10bit_encoder_owned_full_interop_30_frame_parity -- --ignored --nocapture

ASCIIFLOW_STAGE52C3_AV1_STRESS_INPUT=/absolute/path/to/av1-main-10bit-3000.mp4 \
cargo test -p asciiflow-interop --test hardware \
  av1_10bit_encoder_owned_full_interop_3000_frame_stress -- --ignored --nocapture
ASCIIFLOW_STAGE52C3_AV1_STRESS_INPUT=/absolute/path/to/av1-main-10bit-3000.mp4 \
cargo test -p asciiflow-interop --test hardware \
  av1_10bit_staged_encode_3000_frame_fd_stress -- --ignored --nocapture
cargo test -p asciiflow-media \
  injected_av1_10bit_send_receive_and_drain_failures_preserve_cause -- --ignored
```

The real encoder-owned AV1 P010 surface is compared byte-for-byte before
encode against staged Vulkan output, including active low bits. Real AV1
decoded pixels are not expected to be byte-exact after lossy encoding.
The independent generic P010 output test can be rerun with
`cargo test -p asciiflow-interop --features p010-output-diagnostic --test
hardware p010_packed_output_is_bit_exact -- --ignored`.
