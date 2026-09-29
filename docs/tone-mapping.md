# Internal HDR→SDR reference

C-1 implements ITU-R BT.2446-1 Method A, fixed 1000→100 cd/m², as a permanent
f64 CPU qualification oracle. It is not a production backend, CLI feature,
encoder, or universal policy for PQ content.

## Authorities

- [BT.2446-1 (03/2021), §4.1 Tables 2–3](https://www.itu.int/dms_pub/itu-r/opb/rep/R-REP-BT.2446-1-2021-PDF-E.pdf): forward Method A, printed pp. 8–9. Forward formulas have no equation numbers.
- [BT.2020-2 (10/2015), Table 4](https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.2020-2-201510-I!!PDF-E.pdf): non-constant-luminance coefficients and inverse.
- [BT.2100-3 (02/2025)](https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.2100-3-202502-I!!PDF-E.pdf): existing PQ/BT.2020 display-light source semantics.
- [BT.1886 (03/2011), Annex 1](https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.1886-0-201103-I!!PDF-E.pdf): reference display EOTF, distinct from the BT.709 camera/signal OETF.

## Types and placement

The existing `hdr_pq::LinearRgb` is explicitly aliased as
`LinearBt2020RgbNits`, in absolute cd/m². P010 integration validates limited,
left-sited BT.2020 NCL/PQ source semantics; the isolated linear harness declares
them explicitly. Bare triples cannot authenticate primaries or codec provenance.
Separate `SdrBt2020NonlinearRgb`, `SdrBt2020Ycbcr` and pre-correction
`mapped_luma` carry raw f64 signals, not codec descriptors or quantized frames.

```text
qualified PQ P010 -> B-1 HDR cell aggregation / glyph selection
                 -> shared linear-light coverage and blend
                 -> rendered linear BT.2020 RGB nits
                 -> C-1 Method A -> SDR BT.2020 intermediate
```

`HdrPqReference::render_linear` exposes the existing post-blend/pre-PQ boundary.
It shares `rendered_pixel` with the preserve renderer. Cell aggregation, S-curve,
LUT, coverage and blend are not duplicated or changed. No PQ round trip or
SDR-driven glyph selection occurs. `map_hdr_ascii` retains the HDR cells and
rendered image alongside the SDR result for identity checks.

## Fixed equations

Table 2 transforms each channel `v` in nits as `v'=(v/1000)^(1/2.4)`.
Set `Y'=.2627R'+.6780G'+.0593B'` and
`rho(L)=1+32(L/10000)^(1/2.4)`, `rhoH=rho(1000)`, `rhoS=rho(100)`.

```text
p = ln(1+(rhoH-1)Y') / ln(rhoH)
c = 1.0770p                              p <= .7399
    -1.1510p² + 2.7811p - .6302          .7399 < p < .9909
    .5p + .5                             p >= .9909
Ys = (rhoS^c-1)/(rhoS-1)
f = Ys/(1.1Y')
Cb = f(B'-Y')/1.8814
Cr = f(R'-Y')/1.4746
Yt = Ys - max(.1Cr,0)
Rout = Yt + 1.4746Cr
Bout = Yt + 1.8814Cb
Gout = (Yt-.2627Rout-.0593Bout)/.6780
```

Table 3 colour correction includes chroma scaling and asymmetric luma correction,
not uniform RGB scaling. Reconstruction inverts exact BT.2020 NCL coefficients.
`ln_1p`/`exp_m1` retain near-black precision. For very small subnormals, rooting
before division avoids normalization underflow with unchanged algebra. At black,
the explicit continuous limit is zero RGB/chroma, not `0/0` or a safety clamp.
The published knee has small upward jumps (approximately .00054632649 and
.00000512431); the implementation keeps them, rather than fitting a smoother curve.

For neutrals, `100*Ys^2.4` is a zero-black, 100-nit reference-display diagnostic,
not photometric luminance of coloured RGB. BT.1886 has display black/white
parameters; its zero-black limit is not the BT.709 camera OETF. No output
transfer/matrix tags, codec, bit depth or production display policy is selected.

## Rejection, excursions and diagnostics

Negative, nonfinite or >1000-nit rendered-input channels reject qualification.
All input components are inspected before mapping, with the first invalid pixel
and whole-frame counts retained. No safety clamp or generalized 4000/10000-nit
curve is applied. Synthetic/P010 sources are explicitly constrained; a real
source's peak cannot be inferred merely because a cell average is below 1000.

Literal Method A produces negative or >1 raw nonlinear RGB for some legal
saturated BT.2020 inputs. Red at 1000 nits is approximately
`[1.3238360361,-.0321782183,-.0321782183]`; blue reaches 1.5958468521.
These excursions are preserved and counted. The 100-nit endpoint concerns mapped
achromatic luma, not a claim that every coloured component is codec-bounded.
Applying fractional display powers to negative RGB is not part of this oracle.

Input-safety and unexpected clamps are zero. The standard-required counter
counts negative `.1Cr` terms discarded by Table 3 `max`, not output RGB clipping.
Successful output has no NaN/Inf. Rejection before mapping leaves unset output
extrema as sentinel infinities; they are not produced output samples.
Mastering Display/MaxCLL/MaxFALL are not operator arguments. Neither metadata,
frame extrema nor previous frames control the fixed parameters.

## Reproduction and limits

See [closure](stage5.3c1-tone-map-cpu.md) and
[fixture identity](../tests/fixtures/tone-map/README.md). The algorithmic 1080p
linear source includes neutral ramp/reference levels and moderate/saturated
colours. Three explicit-endian f64 outputs must match each other and retained
SHA256SUMS; never hash native struct padding.

Retain generator/wrapper/source hashes, exact commands, compiler/libm environment,
input identity, output digest or structured numerical oracle and diagnostics.
Exact f64-byte portability across libm/compiler implementations is not claimed;
qualify numerical vectors before establishing another platform's byte baseline.
Never refresh an oracle merely to pass a changed operator.

BT.2020→BT.709 gamut reduction, hue policy, raw-excursion handling, source-peak
generalization and output signaling require separate work. No BT.2407,
matrix-plus-clamp shortcut, alternate operator, artistic control, LUT/SIMD/f32,
parallel processing, Vulkan implementation or public option is introduced.
