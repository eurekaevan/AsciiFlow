# C3 numerical precision contract review

This precision review originated on 2026-09-30; it is not a current production
enablement decision. Explicit HDR→SDR is now supported under the
[production contract](production-support.md). The C1/C2B CPU f64 implementations and retained digests are the
scientific references; a shader arithmetic estimate cannot replace them.

## Historical diagnostic and provenance

The historical gate is half-up normalized RGB quantization to UNORM16,
`floor(clamp(signal,0,1)*65535+0.5)`, maximum distance ≤2 and no channel above 2.
It remains a failed historical result, not a production pixel format.

Repository provenance provides no derivation from a standard, final media
format, perceptual threshold, or stated error budget. Its defensible description
is **historical engineering diagnostic threshold**. Sixteen bits provide a
fine-grained way to expose near-black numerical differences; the choice of two
codes was not accompanied by a documented numerical justification. This audit
does not invent one. C3A's six best-f32 failures and the earlier eleven vectors
are preserved, with their original coordinates and scientific values.

## Precision layers and the independently derived contract

N1 is a high-precision reference-space bound. N2 is a final output-code bound.
The contract selected by this review is **N3**, not a changed colour algorithm:

1. Every final nonlinear RGB component is finite and in [0,1].
2. Against the immutable f64 result, each absolute nonlinear component error
   is ≤`1/1792` (approximately 0.0005580357142857143).
3. Test-only BT.709 limited 10-bit Y/Cb/Cr quantization has maximum distance ≤1
   in every channel, with zero distances above 1.
4. Glyph, R8 coverage, domain rejection, clipping policy and exact clip-mask
   comparisons remain separate obligations. Output-code equality cannot hide
   their failures. Floating RGB and UNORM16 remain reported diagnostics.

This allowance is **half of one limited-range 10-bit chroma code before
rounding**, not the largest error observed in an experiment. It is an engineering
choice of signal accuracy, not an ITU tolerance or a perceptual claim.

For the test-only matrix, [ITU-R BT.709-6 §§3.2–3.4](https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.709-6-201506-I%21%21PDF-E.pdf)
specifies luma weights (0.2126,0.7152,0.0722), chroma denominators 1.8556/1.5748,
and code scales 219/224 with offsets 16/128 at eight bits. Scaling by four
gives 10-bit Y64..940 and C64..960. The diagnostic applies these equations to
the existing C2B display-power signal; it does **not** substitute a camera OETF
or claim production BT.709 transfer metadata has been qualified.

Each Y/Cb/Cr row has absolute coefficient sum 1. Thus a component error bound
ε implies continuous code shifts at most 876ε for Y10 and 896ε for C10.
At ε=1/1792 these are 0.4888392857142857 and 0.5 codes. Half-up rounding of two
values less than one code apart can differ by at most one, regardless of the
position of the rounding boundary. RGB10's shift is at most 1023ε, approximately
0.5708705357142857, also below one. **Exact** code equality does not follow.
This proves the projection implication, not a global error theorem for every
possible shader input. Measured corpus qualification is still required.

An identical convex average of nonlinear chroma samples cannot increase this
bound before rounding. Different filters, intermediate quantization, packing,
codec loss and metadata remain outside the proof. All measurements in this
review use 4:4:4 diagnostics, not production 4:2:0. NV12 8-bit and P010 10-bit
BT.709 limited remain future candidates; neither is selected as the C4 format.

The old ≤2 UNORM16 threshold is materially tighter than this contract: a
two-code distance corresponds to roughly 0.0305 continuous 10-bit chroma codes
before accounting for rounding phase. Conversely, identical YUV values alone
are not a sufficient RGB contract. Absolute RGB error must remain checked.

## Actual mixed-precision architecture

Intel(R) Arc(tm) Graphics (MTL), vendor8086/device7d55, Mesa ANV26.2.3 reports
`shaderFloat64=true` on the real host. llvmpipe is not the qualified device.
An opt-in `hdr-to-sdr-fp64-experiment` feature and explicit experimental
constructor enable Float64 only on that experimental device. Normal construction
does not request it, even when the feature is compiled. No production capability
graph, planner, CLI or rejection policy is changed.

M0 is the best observed f32 arithmetic (common-scale B and TwoSum/Dekker C).
M1 keeps f32 source power, accumulates the matrix in f64, then casts before the
f32 limiter/target power. M2 retains the f64 matrix through clipping and target
power; M3 also computes source power in f64. All variants retain the f32 B→C
storage boundary and final f32 output. They are not a full-pipeline FP64 rewrite.

GLSL double has no native `pow` here. The experimental power uses range-reduced
log (32 atanh terms, |z|≤1/3) and exp (24 Taylor terms, |r|≤ln2/2). Series
truncation is below binary64 roundoff, but the whole evaluation is not asserted
correctly rounded. A separate real-GPU edge probe compares to the immutable CPU
oracle. Intermediate observations are f32 casts of the actual double values;
they do not expose binary64 intermediate bits.

| Variant | B+C max16 / >2 | C max16 / >2 | B+C max nonlinear error |
| --- | ---: | ---: | ---: |
| M0 |4 /5 |3 /1 |6.482573253605711e-5 |
| M1 |4 /5 |3 /1 |6.482573253605711e-5 |
| M2 |4 /5 |3 /1 |6.482107592318403e-5 |
| M3 |4 /3 |2 /0 |6.559440009517789e-5 |

Every case has zero exact clip-mask mismatches and zero Vulkan validation
errors. M3 solves isolated C's old diagnostic gate, **not** full B+C. Promoting
it cannot be justified as a solution to the full old gate. Native GLSL.std.450
Fma precision was already separated from the explicit Dekker residual in C3A;
coefficients, operation order, transfer, matrix and clipping policy are audited.
No known constant or policy bug is being excused by this review.

The initial small edge harness used invalid 20×1 P010 geometry and failed before
dispatch. Its corrected 20×2 probe passed: final max absolute error
2.963953460444202e-8, UNORM16 distance0, validation0. Both outcomes are disclosed.

## Evidence boundaries

The C3B review selects **P2 / N3**: finite/bounded nonlinear RGB, absolute
component error at most 1/1792, and test-only limited BT.709 4:4:4 10-bit code
distance at most one with no samples above one. Exact glyph, R8 coverage and
clip masks remain independent gates. Historical UNORM16 failures are retained,
not recast as PASS. Normal internal qualification dispatch now selects M0's
common-scale B plus compensated C (selector262); production is unchanged.
Full-frame selected-configuration parity and subsequent hardware stress and
performance must pass before sealing. Earlier mode0 real-frame evidence is
separately identified and cannot stand in for this selected configuration.

Canonical case GPU timings are single-dispatch, capture-enabled observations,
not stabilized backend performance qualification. They include substantial
readback in backend wall time; no throughput comparison is inferred from them.
Full real-source diagnostics, policy decision and final gate ledger are recorded
in the internal Vulkan qualification report and machine-readable evidence. Historical SDR/PQ/H.264
records remain separate from a newly executed check. No ignored test is PASS.
