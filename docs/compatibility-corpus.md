# Compatibility corpus

Stage 5.4A packages the existing production support contract and regression
evidence into a reproducible runner. It enables no additional codec, color,
container, backend or interop path. [The stage record](stage5.4a-support-contract.md)
records the observed gates and final qualification decision.

Stage 5.4B adds [`real-media-v1.json`](../tests/corpus/real-media-v1.json):
79 deterministic encoded/muxed sources and three separately requested hardware
tuples. Its [closure report](stage5.4b-real-media-compatibility.md) and
[durable receipt](../tests/corpus/stage54b-closure.json) distinguish qualified
tuples, expected rejections and explicit Unqualified limitations. Generated
compatibility media is not a new benchmark source or a blanket container claim.

## Contract and fixture identities

[`tests/support/production-support-v1.json`](../tests/support/production-support-v1.json)
is the versioned support source of truth. Its dimension rows describe individual
properties, not their Cartesian product. Scenario tests drive the real planner;
an architectural plan selected against supplied capabilities does not establish
device qualification. The [generated support document](production-support.md)
and README table are checked by `python3 tests/support/matrix.py check`.
`python3 tests/support/matrix.py coverage` reports observed scenario pairs and
uncovered pairs, without an arbitrary percentage target.

[`tests/corpus/representative-v1.json`](../tests/corpus/representative-v1.json)
pins each source's SHA-256, byte size, generator and command, generator identity,
tool identity and optional retained reference-manifest identity. Media records
include profile/depth/chroma, geometry, timing and reorder information, GOP,
audio/subtitles and color/side data. Request and expected outcome are separate:
classification, planner rejection, initialization rejection and runtime failure
are different observations. Mutation and corruption fields identify intended
negative behavior; their presence alone is not an executed test.

The [schema](../tests/corpus/manifest.schema.json) is validated by the small
stdlib-only [validator](../tests/corpus/schema.py). It implements the checked-in
schema dialect and rejects unsupported keywords; it is not a general JSON Schema
engine. Fixture identity mismatches fail setup. Generated files go beneath the
new run directory; retained files are read in place. The runner does not download
external references automatically. Canonical/performance sources remain distinct
from representative compatibility cases; a fixture inventory is not evidence
that every plausible stream feature is supported.

## Running

For the Stage 5.4B real-media corpus:

```bash
python3 -B tests/corpus/run.py quick --manifest tests/corpus/real-media-v1.json \
  --output target/stage54b-evidence-new
```

The three checked-in `generate-real-*.py` recipes generate each group once.
`--generated-inputs` can reuse a flat cache, but generator/reference identity,
source bytes, complete normalized probe SHA and actual media facts are checked
before conversion. Only the probe filename is normalized. A mismatch is setup
failure, never an expected codec rejection. Hardware smokes and retained output
hashes always use the separate 300-frame canonical H.264 source, not the first
short corpus file. The executable is copied once into the fresh run directory
so concurrent builds cannot change it halfway through a matrix.

Every command has a finite deadline (`--watchdog-seconds`, default 600;
`--generation-watchdog-seconds`, default 2400). TIMEOUT / possible hang fails
the gate and terminates the entire process group, including descendants which
ignore SIGTERM. It is never reclassified as a normal media rejection. Each
conversion records the sampled process peak RSS; this excludes child/GPU memory
and is not a performance qualification.

`UNQUALIFIED` is distinct from PASS. Six registered audio limits require the
unchanged strict Rust oracle to fail with a fresh exact typed report; missing
reports, compilation errors, wrong failure codes, payload/timestamp changes or
full source/output audio decode errors still fail. The characterization only
explains explicit metadata/duration bounds; it cannot qualify or promote a case.
Other Unqualified cases also require precise blockers. Fatal conversion checks
preserve a preexisting sentinel destination and require no residual staging.

Build the production CLI first, then choose a fresh directory for each invocation:

```bash
cargo build -p asciiflow-cli --release
python3 tests/corpus/run.py quick --output /tmp/asciiflow-corpus-quick-new
python3 tests/corpus/run.py full --output /tmp/asciiflow-corpus-full-new
python3 tests/corpus/run.py hardware --output /tmp/asciiflow-corpus-hardware-new \
  --device /dev/dri/renderD128
```

`quick` checks the manifest, support/planner/document consistency, fixture
expectations, actual representative production/decode-back where eligible,
and established media-oracle controls. Without visible hardware, the SDR8
H.264 output cases force CPU/software; hardware-only positives are SKIPPED.
Expensive generated inputs for skipped hardware cases are deferred (recipe
identity is checked, input bytes are explicitly not attested). `full` adds C-1 and C-2B CPU
references and the workspace suite. `hardware` adds those CPU references and
the four production smokes (SDR, PQ preserve, explicit PQ→SDR8 and PQ→SDR10), followed by the
existing C-3 and C-4A qualification reference commands. It does not add a new
soak or benchmark suite. Use one command for full tests, production retained
paths and existing hardware references:

```bash
python3 tests/corpus/run.py full --retained \
  --output /tmp/asciiflow-corpus-full-hardware-new --device /dev/dri/renderD128
```

`--retained` reruns all 17 retained production paths three times each: five SDR,
two PQ-preserve, and ten HDR→SDR paths. It also selects the hardware smokes and
C-3/C-4A references. `--binary /absolute/path/to/asciiflow` selects the executable;
the default is `target/release/asciiflow`. Hardware gates are serialized by the
runner. Native tools, matching drivers and the identified accessible render node
are still prerequisites.
`--generated-inputs DIRECTORY` optionally reuses previously generated inputs
read-only. The runner still verifies generator/reference identity and exact
input byte size and SHA-256. A mismatch is a setup failure; dependent SDR smoke
and retained runs do not consume the invalid input. Cache presence alone is
never identity evidence. This option does not promote or overwrite a baseline.
`quick --retained` is rejected before directory creation because it omits the
CPU-reference prerequisites. Use `full --retained` or `hardware --retained`.

The output directory must not already exist, and normal runs may not write
under `tests/baselines`. JSON/log creation also refuses overwrite. Retained
baseline promotion is a separate explicit review action; the runner has no
automatic promotion mode. Existing golden records are never rewritten to match
a changed result.

## Results and support truth

Each run writes `environment.json`, `pairwise-coverage.json`, command logs,
per-input diagnostic reports, `results.json` and `results.md`. Environment records
include source/diff/untracked identities, lockfile and executable SHA-256, native
tools, kernel and driver queries. Results report PASS, FAILED, SKIPPED or UNQUALIFIED with
command exit status and category-specific counts. Unexpected success is a failure
for a rejection case; unexpected rejection is a failure for a positive case.
Surface counts overlap: a positive fixture verifies classification, planner,
runtime and decode-back. They are not a unique-test total. Invalid schema stops
before fixture execution. Rejected first-frame semantics use typed color codes;
a stream-only snapshot is not mislabeled as resolved conflicting metadata.

If the render node is absent or inaccessible, positive hardware observations and
hardware gates are SKIPPED with an explanation about the current execution
environment. This does not infer that the host lacks a GPU. Classification and
policy rejection checks can still execute before hardware selection. A successful
runner exit with skipped hardware is not hardware closure.

The CLI's optional `--diagnostic-report PATH` records the selected typed plan,
input requirements and runtime capability facts. `plan_scope` distinguishes
`selected_by_planner` inspection from `initialized_execution` production. Enum
fields use their Rust Debug names. Failure `category` is derived from the existing
core error variant and `stage` from the existing pipeline stage; neither is
inferred by searching English error messages. Unwrapped, unclassified errors
are `Other` with a null stage. Runtime capability scope is
`runtime_probe_not_global_qualification`; Supported/Unsupported/NotProbed facts
remain scoped to the input/device and probe. Implemented code, runtime probes,
architectural representability and retained hardware qualification are separate
claims.
An existing diagnostic file is never overwritten. Diagnostic write failure
is a warning and does not change the pipeline outcome, including a successful
conversion already committed. The runner separately requires its report file;
a missing diagnostic therefore fails the harness gate, not the conversion.

HDR→SDR additionally requires every decoded per-pixel RGB component to be finite
and within 0–1000 cd/m². Metadata values alone cannot establish this domain;
there is no implicit highlight clipping. PQ preservation retains its separate
hardware and signal contract.

The retained runner reuses established packet/decode/timing/tag/parity and
repeatability comparators rather than adding a permissive replacement oracle.
Artifact equality and the original exact-build attestation are reported
separately: matching output bytes do not make a new executable the historical
qualified build. Historical failed gates remain failures in their original
records. A changed source/build scope needs explicit reviewed evidence and must
not be relabeled historical hash PASS.
