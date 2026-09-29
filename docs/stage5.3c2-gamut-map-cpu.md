# Stage 5.3C-2 entry-domain and standard audit (historical Annex5)

Current C-2 concept: **BT.2020→BT.709 SDR conversion**. C-2A clarifies the
domain/standard; [C-2B](stage5.3c2b-target-volume-cpu.md) qualifies the new
primary-conversion + target-volume CPU reference. This preserved original
Annex5 record is not promoted to PASS and its evidence/limits are unchanged.

Subsequent clarification: [Stage5.3C-2A](stage5.3c2a-domain-standard-clarification.md)
is **SEALED (Outcome C/F3)**. C-2 was **BLOCKED pending C-2A decision**;
that decision rejects Annex5 as an executable reference and selects §2 matrix
conversion + explicit target-volume limiting (D5+D6) as the next CPU-reference
basis. Original Annex5 C-2 remains **NOT SEALED / implementation ABANDONED**. The original
audit, statistics, equation contradiction and hashes below are preserved; no
valid-subset/derived-formula PASS replaces them.

Final status: **NOT SEALED**. The requested audit-before-coding identified a
mandatory C-1 source-domain blocker and a printed equation inconsistency.
No Annex 5 or hard-clip implementation is claimed. No C-3 work was started.
Starting tree was clean at `4d95a7e2c6560e061510a6b27ee685c1db08ae0a`.
C-1 core, CPU renderer, fixture generator, vectors and retained baseline are
unchanged. Only audit tooling/evidence and documentation are added.

## Evidence and conclusion

[Gamut-mapping audit](gamut-mapping.md) records the official standards/pages,
C-1 semantic inspection, complete source statistics, positive-only counterexamples,
geometric impossibility proof and literal equation (5-4) discrepancy.
An independent semantic review confirmed both blockers, including direct PDF
image verification; no correction or workaround was accepted from that review.

Canonical source: one regenerated 1920×1080 C-1 image, 2,073,600 pixels,
explicit LE f64 serialization. All three C-1 output runs reproduce retained
SHA-256 `d0eb86dcc265fe7f675d87055f038d258176e331eb2aac412ba7358a6c8c1e6b`.
This is a **C-1 intermediate digest**, not a C-2 mapped-output digest.

```sh
bash scripts/qualify-tone-map-cpu.sh /tmp/asciiflow-c2-c1-regression
python3 tests/fixtures/tone-map/audit-c2-domain.py \
  /tmp/asciiflow-c2-c1-regression/method-a-run1.bin > /tmp/c2-domain-audit.json
cmp /tmp/c2-domain-audit.json tests/fixtures/tone-map/c2-domain-audit.json
```

The audit pins the input's sealed SHA, magic, dimensions and length; it scans all
RGB pixels and verifies C-1 component counts. Decimal-70 evaluates the independent
retained green/cyan/yellow expected vectors. No Rust gamut function is called,
and no mapper, clamp or signed negative transfer is executed.
The audit succeeds by recording a demonstrated **NOT SEALED** result; its
successful process exit is not a gamut-qualification PASS.

Audit generator SHA-256:
`cdbf62b44a99367d35da44be3180b5104dc47f6ccc131b11738329daec579e9d`.
Retained JSON SHA-256:
`bc6894bace5184c234fdf53c536f129d62b7293f826660d59e288d065528903f`.
The report was independently regenerated from each of the three C-1 binaries;
all three JSON bytes match. This is audit determinism, not mapped-output
determinism. The negative control rejects a non-C-1-output artifact by SHA.
Evaluator environment: Python 3.14.7; Decimal precision 70; zero-black normalized
display continuation 2.4 for positive-only diagnostics, no negative extension.

Audit counts: 802,388 source-cube excursion pixels; 129,600 pixels with negative
RGB; 802,388 pixels with >1 RGB (categories overlap). Negative components=194,400;
>1 components=1,056,216. Material excursion count remains 802,388 at epsilon1e-12.
Nonnegative pixels with diagnostic Y>1+epsilon=194,434. Max Y=1.0364026948978502.
NaN=0, Inf=0, preclamps=0, unexpected clamps=0. No target-gamut containment,
mapped-output ΔY/hue/displacement percentiles or mapped yellow statistics exist.

Positive-only Decimal examples exceed target white: green102.472157470669 nit,
cyan103.640269489785 nit, yellow100.762424060741 nit. For legal BT.709 RGB,
Y<=1; preserving these values at 100-nit white is impossible. Annex 5 ray
projections do not exist at these Y values. Eq(5-4)'s radial saturation cannot
resolve an empty constant-Y target gamut. The separate printed (5-4) example
alpha=.5/beta=.2 produces endpoint3.1666667 and an upper discontinuity, contrary
to its stated roll-off behavior. Both facts require clarification, not tuning.

## Existing regression checks

All existing production scripts/oracles and expected baselines remain unchanged.
Fresh `/dev/dri/renderD128` Intel runs used the existing full VAAPI→Vulkan→VAAPI
path, width80, standard charset, builtin8×8, colour, audio none, both interop
directions, 300 frames and explicit codec/depth settings encoded by the scripts:

```sh
bash tests/baselines/media/generate-pq-production-v1.sh tests/fixtures/codecs /tmp/asciiflow-c2-pq
python3 tests/baselines/media/verify-pq-production.py /tmp/asciiflow-c2-pq /tmp/asciiflow-c2-pq-verification.json \
  --input-directory tests/fixtures/codecs --check-baseline tests/baselines/media/pq-production-v1.json
bash tests/fixtures/codecs/generate-8bit-production-baseline.sh /tmp/asciiflow-c2-input8.mp4
bash tests/baselines/media/generate-post-polarity-v2.sh /tmp/asciiflow-c2-input8.mp4 \
  tests/fixtures/codecs/hevc-main10-canonical-v1.mp4 /tmp/asciiflow-c2-sdr
```

PQ HEVC/AV1: all three runs each match retained tiers 1A/1B/1C/2/3, including
complete decode-back/metadata/timestamps. Five SDR profiles: three files and
decoded framemd5 hashes each match `post-polarity-v2.json`, not just each other.
No old output hash is updated. Historical H.264 pair rechecked with unchanged
policy: raw1A FAIL, coded1B/decoded1C/structure2 PASS, whole file DIFFERENT;
approved patch-version differences only. It is not converted to strict PASS.

Normal and measurement suites each: 201 passed, zero failed, 79 ignored
opt-in tests (not claimed as fresh hardware PASS). Existing CLI tests still
reject default8-bit PQ, HLG, full-range, wide-gamut SDR and unqualified paths
before output staging. Release build, both strict Clippy modes, fmt and diff
checks pass. All actual317 cached modules validate for Vulkan1.3; no shader
source changes. No C-2 conversion performance was measured because no qualified
operator exists; audit time is not labelled gamut-mapping FPS.

## Required final record

| # | Item | Observed status |
| ---: | --- | --- |
| 1 | BT.2407 version | BT.2407-0 (10/2017), official in-force listing checked |
| 2 | Annex sections | A5.1–A5.8 audited; equations1/5-1–5-4 and figures inspected |
| 3 | Hard-clip diagnostic | Not implemented; blocked before coding |
| 4 | C-1 semantics | Raw nonlinear RGB, corrected YCbCr, nonlinear mapped luma |
| 5 | N→L | Display power2.4 known on nominal domain; positive continuation diagnostic only |
| 6 | BT.2020→XYZ | Standard audited; no new matrix implementation |
| 7 | XYZ→BT.709 | Standard audited; no new matrix implementation |
| 8 | u′v′ | Eq5-3 audited; not implemented |
| 9 | Source effective gamut | Cube/constant-Y assumption audited; C-1 violates domain |
| 10 | Target effective gamut | Empty for demonstrated Y>1 inputs |
| 11 | White-point projection | Not implemented; impossible on empty target gamut |
| 12 | Alpha | A5.6 distance ratio audited; no production computation |
| 13 | Beta | Requested fixed .2; no knob added |
| 14 | Soft clip | Printed5-4 inconsistency demonstrated; no silent corrected function |
| 15 | Continuity | Literal printed example fails upper value/lower derivative gates |
| 16 | Unchanged region | Standard intent audited; no mapper test PASS claimed |
| 17 | Y preservation | Incompatible with containment for positive C-1 counterexamples |
| 18 | Containment | Cannot preserve Y>1 in target[0,1] cube |
| 19 | Black | No C-2 black implementation/test claimed |
| 20 | White | Standard permits only white at Y1; no C-2 implementation |
| 21 | Neutrals | C-1 retained; C-2 not qualified |
| 22 | BT.2020 primaries | Green counterexample independently evaluated |
| 23 | BT.709 primaries | Not newly qualified |
| 24 | CMY | Cyan/yellow positive-only counterexamples; no mapped results |
| 25 | Bright yellow |100.762424060741-nit counterexample; A5.7 hue limitation recorded |
| 26 | C-1 excursions |802,388 pixels; full counts/ranges retained |
| 27 | Clamps | No preclamp or unexpected clamp performed |
| 28 | NaN/Inf | Zero in audited C-1 RGB; no mapped output exists |
| 29 | Independent precision | Decimal70 counterexamples; gamut expected vectors pending |
| 30 | Hard clip vs Annex5 | No comparison/visual superiority claim |
| 31 | Canonical fixture | Sealed C-1 source audited; new gamut fixture pending |
| 32 | Digest | C-1 three hashes retained; no C-2 digest fabricated |
| 33 | Builtin integration | Existing C-1 passes; C-1+C-2 blocked |
| 34 | FreeType integration | Existing C-1 passes; C-1+C-2 blocked |
| 35 | Glyph identity | No renderer/glyph code changed; C-2 integration not claimed |
| 36 | Performance | C-2 not measured; no invalid subset relabelled1080p PASS |
| 37 | C-1 regression | Retained canonical digest reproduced three times |
| 38 | SDR regressions | Five profiles ×3 retained output/decoded hashes pass |
| 39 | PQ regressions | Two profiles ×3 all retained tiers pass |
| 40 | Production policy | HDR→SDR remains closed; other rejections unchanged |
| 41 | SPIR-V | All317 actual modules validated Vulkan1.3 |
| 42 | Static/workspace | Release, both201-test suites, both Clippy/fmt/diff pass |
| 43 | Limits | Source-domain conflict and printed formula inconsistency; no quality claims |
| 44 | C-3 justification | Not justified; not started |
| 45 | Final C-2 status | NOT SEALED |

Next work requires a separately approved C-1→C-2 extended-signal/overshoot policy
and an authoritative resolution of the printed soft-clipping equation. Altering
C-1, lowering Y, raising target peak, signed signal extension, or accepting an
interpreted formula must not happen silently inside C-2. Current C-1 sealing
and byte oracle are not rewritten by this audit.
