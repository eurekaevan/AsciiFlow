# BT.2020→BT.709 SDR conversion reference

Current executable reference: [Stage 5.3C-2B](stage5.3c2b-target-volume-cpu.md),
CPU f64 only. C-2A selected D5+D6; C-2B implements two **separate** operations:
standards-derived colorimetric primary conversion and explicit project
target-capability limiting. This is not Annex5 or production HDR→SDR.

[C-3](stage5.3c3-vulkan-hdr-to-sdr.md) now has an isolated Vulkan f32 draft,
**NOT SEALED**. Canonical clip counts and promoted-input masks are exact, but
small pre-limit errors near zero fail the strict final diagnostic code gate.
This does not qualify gamut quality, change the CPU oracle, repair Annex5 or
enable production HDR→SDR.

```text
immutable raw C-1 nonlinear BT.2020
 -> sign(v)|v|^2.4, normalized display-linear (1=100 nit)
 -> precise BT.2020 -> XYZ -> BT.709, common D65, no adaptation
 -> observable unbounded target RGB
 -> per-component target-linear clamp [0,1]
 -> bounded target RGB -> inverse display power 1/2.4
```

The sign-reflected continuation is an explicit C-2B input-boundary policy
allowed by the C-2A decision, not a retroactive change to sealed MethodA or
a claim that negative display light is physical. NaN/Inf/arithmetic overflow
reject rather than clip. Negative/>1 C-1 values are never pre-limited.
Transfer, primary matrices and clipping can each be tested independently.

Primaries are taken from [BT.2020-2 Table3](https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.2020-2-201510-I!!PDF-E.pdf)
and [BT.709-6 Part1 §1.3/1.4](https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.709-6-201506-I!!PDF-E.pdf).
The independent Decimal70 generator derives each normalized primary matrix
from xy and D65(.3127,.3290), then inverts BT.709's matrix. Rust uses nearest
f64 coefficients, not a four-decimal published shortcut; a separate XYZ oracle
checks both matrices and preclip XYZ/Y. The combined path is evaluated about
the neutral axis, using mathematical row sums=1, to preserve neutrals exactly
without snapping. See [vectors](../tests/fixtures/tone-map/c2b-vectors.json).

This engineering limiter is deliberately **not** luminance/hue preserving,
reversible, perceptually optimal, adaptive or universally superior. Target
black=0 and target white/primary capability=1=100 nit. The unclipped intermediate
is retained separately, so matrix error cannot be hidden by clipping. Canonical
negative-only source pixels are absent; synthetic below-black neutral controls
test this extension instead of inventing historical samples. Full-frame
clipping, ΔY, u′v′/hue/chroma and sampled collision measurements are in the
[machine audit](../tests/fixtures/tone-map/c2b-target-volume-audit.json).
These quantify loss, not visual quality thresholds. The historical C-2 audit
and its rounded-weight Y numbers below remain unchanged as historical evidence.

## Historical Annex5 rejection and original entry audit

**C-2A decision (2026-09-29): SEALED, Outcome C / F3.** See
[domain/standard clarification](stage5.3c2a-domain-standard-clarification.md).
The former **BLOCKED pending C-2A decision** state is resolved by abandoning
Annex5 as an executable project reference, not by passing its old gates.
The new CPU-reference basis is BT.2407 §2 primary-matrix conversion with an
explicit project target-volume limiting boundary (D5+D6). At C-2A that reference
was not implemented. It is now qualified separately by C-2B, without preserving
all Y or reversibility. C-1/production remain unchanged.
The original audit below is retained as historical evidence of that decision.

The original Annex5 Stage 5.3C-2 is **NOT SEALED / ABANDONED**. At that audit
no gamut mapper, hard-clip converter or C-1→C-2 path had been added. The entry-domain and official-formula
audits found two blockers before coding an oracle. See
[the stage record](stage5.3c2-gamut-map-cpu.md) and
[machine-readable audit](../tests/fixtures/tone-map/c2-domain-audit.json).

## Previously requested reference and original audit status

[ITU-R BT.2407-0 (10/2017)](https://www.itu.int/pub/R-REP-BT.2407-2017)
is the currently listed in-force report. Its §4 does not select a universally
optimal method. AsciiFlow's original requested reference was Annex 5, A5.1–A5.6:
direct f64 colourimetry, luminance preservation and reversible radial soft
clipping, with fixed beta=0.2. This is a deterministic mathematical-oracle
choice, not a claim of visual superiority. At the original audit §2 matrix-plus-hard-clip was intended
only as a separate diagnostic comparator, not a production default or user option.

The [official PDF](https://www.itu.int/dms_pub/itu-r/opb/rep/R-REP-BT.2407-2017-PDF-E.pdf)
was downloaded and relevant pages rendered/visually checked, including equations
(1), (5-1)–(5-4), Figures A5-1–A5-5 and footnotes. Its SHA-256 is
`14d626eeb33524038066cead3e8aef54dea6b3b3f7dd4073b06fa1569b2fb676`.

## C-1 signal boundary

Source audit: `tone_map_bt2446.rs` returns raw **nonlinear**
`SdrBt2020NonlinearRgb`, corrected `SdrBt2020Ycbcr`, and pre-correction
nonlinear `mapped_luma`. None is display-linear XYZ luminance. The neutral-only
`100*mapped_luma^2.4` diagnostic is not coloured-pixel luminance.

BT.2407 §2.1/2.3 refers to BT.2087 for N↔L. The display-referred branch in
[BT.2087-0 Annex 1, Case #1](https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.2087-0-201510-I!!PDF-E.pdf)
uses normalized display power 2.4 on [0,1]; the inverse uses 1/2.4. This differs
from BT.709 camera OETF and from scene-referred conversion. Note 2 permits
out-of-range continuation with appropriate negative-sign treatment, but C-1
has not selected that extended-domain policy. This audit does **not** apply a
negative fractional power, sign extension, source clamp, or peak normalization.

For nonnegative signals, power continuation above one is computed **only as a
diagnostic counterexample**, not as permission to qualify such source values.
Even granting a negative-sign convention cannot fix the positive-only examples
below. No new transfer/type contract or C-1 output interpretation is silently
introduced.

## Why the source domain blocks Annex 5

A5.1 equations (5-1)–(5-3) operate on linear RGB, XYZ and Y/u′/v′.
A5.2 models primary intensity within [0,1]. A5.4 uses the intersection of a
constant-Y plane with that RGB cube, not a fixed chromaticity triangle.
A5.6 projects from the common D65 white point onto effective source/target
gamut boundaries at the **same** Y, sets
`alpha=distance(w,p2020)/distance(w,p709)-1`, fixes beta=.2 and reconstructs
output RGB from that original Y.

The regenerated sealed C-1 canonical image has 2,073,600 pixels; 802,388 have
material source-cube excursions, using 1e-12 diagnostic tolerance. There are
194,400 negative components and 1,056,216 >1 components. Its raw RGB min/max
is -.0321782182569939 / 1.5958468520638707. Counts are RGB-only, not YCbCr.

Of 1,944,000 finite nonnegative pixels, 194,434 have diagnostic linear Y>1+1e-12.
Their maximum is approximately 1.0364026948978502 (103.640269489785 nit).
Independent Decimal-70 evaluation of retained C-1 colour vectors gives:

| C-1 colour | Normalized linear Y | At 100-nit white |
| --- | ---: | ---: |
| Green | 1.02472157470668987 | 102.472157470669 |
| Cyan | 1.03640269489784924 | 103.640269489785 |
| Yellow | 1.00762424060741114 | 100.762424060741 |

These three examples have strictly positive nonlinear RGB, so their problem
does not depend on negative-channel treatment. For any BT.709 target RGB in
[0,1]^3, positive luminance weights sum to one, hence Y709<=1. No such target
can preserve these Y values. The effective target gamut at Y>1 is empty;
there are no ray intersections for A5.6 steps 2/3. Radial saturation in (5-4)
can handle distances where effective gamuts exist, but cannot create an empty
constant-Y gamut or reduce Y while preserving it.

Thus target containment and unchanged display luminance cannot both pass for
the mandatory C-1 canonical source under the requested contract. This is an
input-domain incompatibility, not a rounding issue. Rejecting colours selectively,
preclamping, reducing luminance or raising the target white would change that
contract and is not done here.

## Printed equation (5-4) inconsistency

The rendered official printed p.37 shows the roll-off branch as:

```text
f(r) = r - alpha/(beta-alpha)^2 *
           [beta - sqrt(beta^2+(alpha-beta)(r+beta-1))]
for 1-beta < r <= 1+alpha
f(r) = r for r <= 1-beta
f(r) = 1 for r > 1+alpha
```

Literal evaluation at alpha=.5, beta=.2 gives f(1.5)=3.166666666666667,
followed by the constant branch 1. The lower boundary values agree, but the
roll-off-side derivative is 5.166666666666667 rather than identity derivative 1.
This conflicts with the adjacent smooth-extension/endpoint description and
Figure A5-4. It was checked in the image, not inferred from garbled extraction.
A bounded official-source search did not locate a correction; this is not a
claim that none exists. No reconstructed Bézier formula or inserted exponent
has been substituted. Authoritative clarification is needed for an exact-formula
oracle satisfying continuity and containment.

## Historical unresolved work and subsequent policy decision

At the original C-2 audit, both blockers had to be resolved before claiming a
C-2 oracle or full pipeline. C-2A now resolves the ambiguity via an explicit
method rejection, not an authoritative formula repair. Public original journal,
conference and working-document equations could not be obtained; the derived
Bézier candidate is diagnostic only, NOT normative. C-2 implementation must
follow the new documented contract rather than silently resume Annex5.
The C-2A requirements were to distinguish display-linear BT.2020 SDR, display-linear
BT.709 SDR and nonlinear BT.709 SDR; precise D65/primary-derived matrices must
be centralized. C-2B implements those replacement APIs; the following original
Annex5 requirements remain abandoned, not qualified by the replacement:
Effective polygon geometry, XYZ↔Y/u′v′ black guards, boundary sweeps,
strict Y epsilon, radial continuity/reversibility and independent high-precision
gamut vectors remain unimplemented/unqualified. No partial valid-subset test
is presented as qualification of all C-1 results.

A5.7 identifies extremely saturated bright-yellow hue shifts as a known
limitation. Here yellow is separately recorded as a source/Y counterexample;
no mapped hue-shift measurement or perceptual-quality claim is made.
No metadata/content/frame adaptation, other annex, LUT, GPU/SIMD, quantization,
YCbCr output or production feature was added by this historical audit. That
audit did not justify C-3; the separate C-2B closure is the current authority.
