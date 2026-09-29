# Canonical C-1 Method A linear fixture v1

## C-2B replacement reference v1

C-1 identity and `SHA256SUMS` below are unchanged. C-2B consumes its raw output
immutably and adds explicit signed display power, D65 primary conversion,
observable unbounded BT.709, **target-only** component clipping and inverse
display power. This is neither Annex5 nor production HDR→SDR. Full contract:
[C-2B closure](../../../docs/stage5.3c2b-target-volume-cpu.md).

```sh
python3 tests/fixtures/tone-map/generate-c2b-vectors.py > /tmp/c2b-vectors.json
cmp /tmp/c2b-vectors.json tests/fixtures/tone-map/c2b-vectors.json
python3 tests/fixtures/tone-map/generate-c2b-vectors.py --rust-table > /tmp/c2b-vectors.tsv
cmp /tmp/c2b-vectors.tsv tests/fixtures/tone-map/c2b-vectors.tsv
python3 tests/fixtures/tone-map/test-c2b-audit.py
bash scripts/qualify-tone-map-cpu.sh /tmp/asciiflow-c2b-c1-regression
bash scripts/qualify-target-volume-cpu.sh \
  /tmp/asciiflow-c2b-c1-regression/method-a-run1.bin /tmp/asciiflow-c2b-target-volume
cmp /tmp/asciiflow-c2b-target-volume/c2b-target-volume-audit.json \
  tests/fixtures/tone-map/c2b-target-volume-audit.json
```

Use fresh directories. `generate-c2b-vectors.py` is independent Python3.14.7
Decimal70 (ROUND_HALF_EVEN), with no Rust calls, media, randomness, locale or
clock dependency. It derives matrices from xy/D65 and separately evaluates XYZ
for 41 vectors. JSON SHA `6fec90c3233204808b51d1485a380686a947b838ad1571caff84945794cbd20f`;
TSV SHA `d2c44d731057b7897709102cda9b52cd33915b97a357f508640ec877b75d2af5`.
The TSV carries the same values without adding a runtime JSON dependency to core.

C-2B output: `AF-C2B-OUT-v1\0`, u32 LE dimensions, row-major 12 IEEE754 f64 LE
fields: source-linear RGB, unbounded BT.709 RGB, bounded BT.709 RGB, nonlinear
BT.709 RGB. No native struct padding. Each run is 199,065,622 bytes; three
hashes `a7e7ffe64d61269288c8e9b2e9b22a62f8c16fd2287083386bc8cefb9c3471af`.
`C2B-SHA256SUMS` is separate from C-1; `c2b-target-volume-audit.json` records all
2,073,600 pixels, strict numerical gates and measured information loss.
Environment/commands/source identities are retained in `c2b-identity.json`.
Numerical PASS does not alone seal hardware/static/production gates.

## Preserved C-1 / C-2A evidence

C-2A audit-only additions preserve this baseline and the original C-2 JSON.
`audit-c2a.py` / `test-c2a-audit.py` provide target-Y feasibility, separate
source/Y taxonomy and literal printed-formula diagnostics. Retained
`c2a-taxonomy.json`, `c2a-formula-audit.json` and `c2a-source-ledger.json` record
the reproducible results and unavailable primary formulas explicitly;
`c2a-qualification.json` retains current unchanged production regression hashes
and verification scope, not a new gamut output baseline.
Signed power is a sensitivity experiment, not a new C-1 contract; derived
Bézier is NOT normative. [C-2A](../../../docs/stage5.3c2a-domain-standard-clarification.md)
seals method rejection/reselection, not a gamut implementation. Regenerate
these audit JSON files with the commands in that report; keep C-1 SHA256SUMS
and identity unchanged. Python3.14.7 / Decimal70 are used for this audit.

C-2 entry audit: `audit-c2-domain.py` accepts only this sealed output SHA and
records NOT SEALED in `c2-domain-audit.json`. It does not clamp, reinterpret
negative signals or gamut-map pixels. See the
[C-2 report](../../../docs/stage5.3c2-gamut-map-cpu.md) for the source/Y and
printed-equation blockers. C-1 identity/SHA256SUMS remain unchanged.

Run from the repository root with the recorded Rust/native math environment:

```sh
bash scripts/qualify-tone-map-cpu.sh /tmp/asciiflow-c1-reference
python3 tests/fixtures/tone-map/generate-method-a-vectors.py > /tmp/method-a-vectors.json
cmp /tmp/method-a-vectors.json tests/fixtures/tone-map/method-a-vectors.json
```

The wrapper builds the Release example, creates the source and three mapped
outputs, compares bytes, prints diagnostics/timing and checks retained SHA256SUMS.
Large generated binaries are not checked in. No randomness, locale, clock,
external media or FFmpeg is used. Timing does not enter serialized data.

The single frame is 1920×1080 absolute BT.2020 RGB in [0,1000] cd/m². Its top
half has sixteen patches: black, .0001/100/203/400/1000-nit neutrals, RGB/CMY,
three moderate skin-like colours, and 1-nit neutral. The next sixth is a horizontal
achromatic ramp; the last third has horizontal/vertical/modular chroma gradients.

Input bytes: `AF-C1-IN-v1\0`, u32 little-endian dimensions, row-major R/G/B
IEEE-754 f64 little-endian samples. Output: `AF-C1-OUT-v1\0`, same dimensions,
seven f64 fields per pixel: R/G/B, corrected Y/Cb/Cr, pre-correction mapped luma.
No padding/native struct layout. Input is 49,766,420 bytes; each output is
116,121,621 bytes. There is no frame rate or codec for this mathematical frame.

`identity.json` retains commands, generator hashes, environment, diagnostics,
three digests and sanity timings. The separate Decimal-70 evaluator reads no
Rust output. Official tables provide formulas, not pixel-vector test files;
`method-a-vectors.json` is an independent evaluation, not an official dataset.
Rust tests embed neutral, RGB/CMY and skin values plus published knee controls.

Exact-byte portability across different compiler/libm implementations is not
claimed. Separate P010 integration tests reuse the B-1 CPU renderer with builtin
and FreeType atlases, including moderate chroma. No production feature is enabled.

See [semantics](../../../docs/tone-mapping.md) and
[closure](../../../docs/stage5.3c1-tone-map-cpu.md).
