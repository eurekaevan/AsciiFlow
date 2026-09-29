# Stage 5.3C-2B — primary conversion + target-volume CPU reference

**SEALED — 2026-09-29.** CPU f64 qualification only. Production HDR→SDR
remains disabled; no C-3 implementation, shader, LUT, CLI, encoder, quantization,
HLG/full-range support, SIMD/Rayon or perceptual/adaptive mapping was added.

The original Annex5 executable reference is **ABANDONED**, not repaired or
promoted to PASS. Conceptual C-2 now means **BT.2020→BT.709 SDR conversion**:
[C-2A](stage5.3c2a-domain-standard-clarification.md) sealed the domain/standard
clarification (Outcome C/F3); C-2B qualifies its selected **D5+D6** replacement.
Preserve the [original audit](stage5.3c2-gamut-map-cpu.md) and its limitations.

## Methodology and input boundary

Two independently tested operations:

1. Standards-derived colorimetric primary conversion.
2. Explicit AsciiFlow target-capability limiting.

The input is the **unchanged** sealed `SdrBt2020NonlinearRgb` from C-1,
not scene-linear RGB, mapped-luma photometry, camera OETF, NV12 or P010 codes.
It is raw nonlinear SDR BT.2020 with finite negative/>1 excursions. C-2B
explicitly chooses `sign(v)*abs(v)^2.4`, zero→+0, as the reflected zero-black
display-power extension. Output uses bounded display-linear BT.709 raised to
1/2.4. This is not BT.709's camera OETF and does not reinterpret C-1 retroactively.
Normalized display-linear 1 means **100 cd/m² target white/primary capability**.
No source clip, rescaling, desaturation, hue correction or Y restoration occurs.
Nonfinite inputs and arithmetic overflow reject instead of becoming black/white.

[BT.2407-0 §2](https://www.itu.int/dms_pub/itu-r/opb/rep/R-REP-BT.2407-2017-PDF-E.pdf)
provides the N→L / primary-matrix / target clipping / L→N basis;
[BT.2087-0 Annex1 Case1/Note2](https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.2087-0-201510-I!!PDF-E.pdf)
supplies display power2.4 and appropriate negative-sign continuation;
[BT.2408-9 §5.3, Annex7](https://www.itu.int/dms_pub/itu-r/opb/rep/R-REP-BT.2408-9-2026-PDF-E.pdf)
discusses reflected extensions and post-conversion capability clipping.
As C-2A records, **this exact extension and component clip are our engineering
policy**, not a mandate to use MethodA or a normative perceptual optimization.
The target clip is `clamp(R709,0,1)` independently on each **target-linear**
component. It is not Annex5, Y-preserving, hue-preserving, reversible or a claim
of universal visual superiority. Negative mathematical components do not denote
physical negative display light.

`core::sdr_target_volume` exposes separate transfer, primary/XYZ and clip helpers
and distinct source-linear / XYZ / unbounded-target / bounded-target / nonlinear
types. `cpu::target_volume` consumes C-1 immutably, retaining all observable stages.
Validated bounded constructors reject outside-cube values; they do not silently
clip. No production planner or backend calls these internal reference helpers.

## Precise colorimetry and independent oracle

Official original primaries/D65 tables were rendered and visually checked:

| Source | Primaries xy (R; G; B), common D65 | Official PDF SHA-256 |
| --- | --- | --- |
| [BT.2020-2 (10/2015), printed p3 Table3](https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.2020-2-201510-I!!PDF-E.pdf) | (.708,.292); (.170,.797); (.131,.046); D65(.3127,.3290) | `eed65cf78c95923964c5420ac4f60d1c83f68823c7a71b05a5e31818ebd03374` |
| [BT.709-6 (06/2015), printed p3 Part1 §1.3/1.4](https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.709-6-201506-I!!PDF-E.pdf) | (.640,.330); (.300,.600); (.150,.060); same D65 | `1fd8567ef4936361ea79840320a2b57ed0cf7631a2bfbccf440827d9fc647648` |

No chromatic adaptation is needed: both systems share D65. Form primary
columns `(x/y,1,(1-x-y)/y)`, solve scales against D65 normalized XYZ, construct
`M=P*diag(P^-1*W)`, then derive `M709^-1*M2020`. Independent Python3.14.7
Decimal70/ROUND_HALF_EVEN performs separate source→XYZ→target calculations;
Rust constants are nearest f64, not rounded four-decimal shortcuts.

```text
BT.2020 -> XYZ:
 .6369580483012913    .14461690358620837   .16888097516417205
 .26270021201126703   .677998071518871     .059301716469861946
 0                   .028072693049087508  1.0609850577107909

BT.709 -> XYZ:
 .4123907992659595    .35758433938387796   .1804807884018343
 .21263900587151036   .7151686787677559    .07219231536073371
 .01933081871559185   .11919477979462599   .9505321522496606

XYZ -> BT.709:
 3.2409699419045213  -1.5373831775700935   -.4986107602930033
 -.9692436362808798   1.8759675015077207    .04155505740717561
 .05563007969699361  -.20397695888897657   1.0569715142428786

BT.2020 -> BT.709 combined:
 1.6604910021084345  -.5876411387885495   -.07284986331988488
 -.12455047452159074  1.1328998971259603  -.008349422604369477
 -.018150763354905303 -.10057889800800739 1.1187296613629127
```

The combined evaluation is `G + mR*(R-G) + mB*(B-G)` per row, using mathematical
row sums=1. This preserves the neutral axis/white exactly without snapping or
clipping; the independently evaluated full XYZ path tests the equivalence.
Preclip target XYZ/Y preservation is the hard matrix gate, not postclip Y.

[Independent vectors](../tests/fixtures/tone-map/c2b-vectors.json) contain
41 cases: black/white/gray, 2020 primaries/secondaries, 709 primaries, target
component boundaries ±1e-7, below-black/above-white neutrals, and all 15 saved
MethodA vectors (including exact historical green/cyan/yellow source signals).
JSON and 22-field TSV regenerate identically. Rust tests also sweep 35,937
signed source triplets and 1,331 clip-boundary RGB combinations. Interior values
retain exact bits (including signed zero); clip is exact, monotone and idempotent.
Black/white and above/below neutral endpoints are exact; neutral tint is zero.

Vector linear/XYZ tolerance is 5e-15; full-frame matrix/XYZ/Y gate is 5e-14.
Near zero, inverse power is ill-conditioned: vector nonlinear error is bounded
by the concave-power Hölder inequality `|x^a-y^a|<=|x-y|^a`, a=1/2.4, plus
1e-15 arithmetic tolerance, **without relaxing the linear matrix gate**.
Every actual target transfer is additionally checked to 1e-15 against its
actual bounded value. NaN/±Inf and overflow controls reject before clipping.

## Canonical fixture and repeatability

Commands from repository root, with **fresh output directories**:

```sh
python3 tests/fixtures/tone-map/generate-c2b-vectors.py > /tmp/c2b-vectors.json
cmp /tmp/c2b-vectors.json tests/fixtures/tone-map/c2b-vectors.json
python3 tests/fixtures/tone-map/generate-c2b-vectors.py --rust-table > /tmp/c2b-vectors.tsv
cmp /tmp/c2b-vectors.tsv tests/fixtures/tone-map/c2b-vectors.tsv
bash scripts/qualify-tone-map-cpu.sh /tmp/asciiflow-c2b-c1-regression
bash scripts/qualify-target-volume-cpu.sh \
  /tmp/asciiflow-c2b-c1-regression/method-a-run1.bin /tmp/asciiflow-c2b-target-volume-closure
cmp /tmp/asciiflow-c2b-target-volume-closure/c2b-target-volume-audit.json \
  tests/fixtures/tone-map/c2b-target-volume-audit.json
```

C-1 input and all three intermediate hashes remain their original retained
values; original C-1 generator, method, correction, SHA256SUMS and identities
are untouched. C-2B consumes **every** pixel of the sealed 1920×1080 frame:
2,073,600 pixels, no selected-valid subset. It covers neutral/skin/RGB/CMY patches,
neutral ramps and wide gradients. The vector fixture adds exact target-volume
interior/boundary/signed-domain controls unavailable in this image.

| Artifact | Bytes | SHA-256 (each of runs1/2/3 where applicable) |
| --- | ---: | --- |
| C-1 linear input | 49,766,420 | `a6246d4e5e34c1c69c9cd629e35d82a0f1e8af387436044c82483511bd749f33` |
| C-1 raw intermediate | 116,121,621 | `d0eb86dcc265fe7f675d87055f038d258176e331eb2aac412ba7358a6c8c1e6b` |
| C-2B target-volume output | 199,065,622 | `a7e7ffe64d61269288c8e9b2e9b22a62f8c16fd2287083386bc8cefb9c3471af` |

C-2B byte format: `AF-C2B-OUT-v1\0`, u32 LE width/height, row-major 12 f64 LE
fields: source-linear RGB, unbounded-target RGB, bounded-target RGB, nonlinear
target RGB. No struct padding. Three initial and three final-script runs have
the same digest. The full-frame audit was independently repeated and byte-matches
the retained JSON. Output creation uses `create_new`: existing files, symlinks
and hardlinks cannot truncate C-1; a real source-alias refusal test preserved
the C-1 digest. The wrapper enforces retained `C2B-SHA256SUMS`.

[Identity](../tests/fixtures/tone-map/c2b-identity.json) retains exact commands,
generator/source/wrapper hashes, field order, environment and timings. Rustc
1.97.1 (8bab26f4f), LLVM22.1.6, x86_64 Linux, glibc2.43-8.fc44, Python3.14.7
Decimal70. The mathematical fixture uses **no FFmpeg**. Media regression oracle:
FFmpeg8.1.3-1.fc44, iHD26.1.5-1.fc44, Mesa/ANV26.2.3-1.fc44, Intel Arc Meteor Lake
`/dev/dri/renderD128`; spirv-tools2026.1-1.fc44. Byte identity across a changed
compiler/libm is not promised: requalify numerical oracles before replacing hashes.

CPU conversion-only sanity (excluding serialization/audit):
191.239511 / 198.505989 / 198.585297 ms/frame,
5.229045 / 5.037631 / 5.035620 fps. No performance or visual-superiority gate.

## Full-frame limiting and loss characterization

[Machine audit](../tests/fixtures/tone-map/c2b-target-volume-audit.json) separates
numerical PASS from this aggregate sealing authority. It checks all pixels
using independent f64 **separate XYZ** coefficients derived at Decimal70;
41 high-precision vectors are separately re-evaluated at Decimal70.

Maximum errors: combined-vs-separate target1.1102230246251565e-15;
preclip XYZ6.661338147750939e-16; preclip Y4.440892098500626e-16 normalized
(4.44e-14 nit); both transfer errors0. All are below the hard tolerances.

Source categories: interior1,271,212; >1-only672,788; negative-and->1 129,600;
negative-only **0**. The latter is explicitly absent (`null` representative),
not invented; below-black synthetic neutral vectors exercise negative-only.
Unbounded target R/G/B minima [-.9241327574,-.2479993025,-.1496924225], maxima
[3.2558296904,1.7108958441,3.4348189830]. Negative channels776,736 across575,019
pixels; >1 channels1,179,851 across897,081 pixels. No source preclip hides them.

| Clipped components per pixel | Pixels |
| --- | ---: |
| 0 (unchanged) | 1,138,470 |
| 1 | 343,484 |
| 2 | 161,835 |
| 3 | 429,811 |

R/G/B low clips:355,638 /194,400 /226,698; high clips:369,245 /440,152 /370,454.
935,130 pixels change; every bounded-linear and nonlinear output is finite in
[0,1]. No additional common scaling, desaturation or luminance restoration.

ΔY=`Yafter-Ybefore` in cd/m², using precise BT.709 NPM Y on both sides of
the limiter (so unchanged pixels are exactly zero): signed mean−5.6591895912;
p50=0, p95=0, p99=+5.4687230876; min−31.5818081307, max+12.0461421742.
Absolute mean5.9085838156; p50=0, p95=30.9549987004, p99/max31.5818081307.
Zero/negative/positive counts1,138,470 /868,256 /66,874. Removing negative
components can increase Y; capping bright components can lower it. Both are
explicit measured policy effects, not matrix bugs.

u′v′ displacement on2,008,620 defined samples: p50=0, p95=.07307528351,
p99/max=.10629074178 (mean.01560868950). Zero-denominator cases64,980 are
undefined, not filled with fabricated coordinates. Around-D65 hue angle is
undefined for black/achromatic values (radius<=1e-12); 1,274,400 defined angular
changes have absolute p50=2.4228800271°, p95=17.7432175366°, p99=21.4992149169°,
max=30.1910487981°. Chroma-radius mean .07037113713→.05666925954; full signed/
absolute percentile distributions and guards are retained in JSON. No arbitrary
ΔY/uv/hue/chroma threshold or subjective superiority PASS is imposed.

| Exact canonical C-1 colour | Ybefore (nit) | Target-linear RGB | Yafter (nit) | ΔY (nit) |
| --- | ---: | --- | ---: | ---: |
| Green | 102.4718665772 | [0,1,0] | 71.5168678768 | −30.9549987004 |
| Cyan | 103.6402397532 | [0,1,1] | 78.7360994128 | −24.9041403403 |
| Yellow | 100.7622406941 | [1,1,0] | 92.7807684639 | −7.9814722302 |

Original C-2/C-2A rounded-weight Y evidence (102.47215747 /103.64026949 /
100.76242406 nit) remains untouched. These new values use the precise normalized
primary matrix, not a retroactive replacement of historical diagnostic numbers.
JSON retains actual source signals, all conversion stages and first-observed
negative/>1/Y<100/Y>100 representatives; absent Y<0 is `null`.

Collision sample: stride257, 8,069 sampled pixels, deduplicated to4,050 **distinct
source bit tuples**, 3,889 bounded outputs; 3 many-to-one groups,161 excess
distinct sources, maximum multiplicity132. Repeated patches are deduplicated
before counting, not falsely reported as loss. This is sampled characterization,
not an exhaustive collision count or reversible mapping claim.

## Integration, unchanged production and static gates

Both builtin and bundled FreeType tests execute PQ P010 decode semantics → HDR
ASCII cell selection/linear coverage blend → original MethodA → C-2B target →
nonlinear BT.709, for color and monochrome. Against independently invoked
existing B/C-1 paths: glyph mismatch0, coverage/linear pixels unchanged,
C-1 pixel values/diagnostics unchanged. Conversion consumes immutable C-1;
target pixels cannot feed back into glyph selection or HDR-preserve output.

Real Intel commands (existing scripts keep all conversion settings explicit):

```sh
bash tests/baselines/media/generate-pq-production-v1.sh tests/fixtures/codecs /tmp/asciiflow-c2b-pq
python3 tests/baselines/media/verify-pq-production.py \
  /tmp/asciiflow-c2b-pq /tmp/asciiflow-c2b-pq-verification.json \
  --input-directory tests/fixtures/codecs --check-baseline tests/baselines/media/pq-production-v1.json
bash tests/baselines/media/generate-post-polarity-v2.sh \
  /tmp/asciiflow-c2-input8.mp4 tests/fixtures/codecs/hevc-main10-canonical-v1.mp4 /tmp/asciiflow-c2b-sdr
ffmpeg -v error -nostdin -i <output> -map 0:v:0 -an \
  -pix_fmt <baseline_pixel_format> -f framemd5 -
```

Two PQ cases×3 runs: all retained tiers1A/1B/1C/2/3 PASS, including full
300-frame decode/timestamps/BT.2020/PQ/limited metadata and retained byte hashes.
Five SDR cases×3: all15 whole-file and decoded framemd5 hashes match retained
post-polarity-v2;300frames1920×1080@50/1, correct profiles/depth, BT.709
primaries/transfer/matrix and tv/limited range. Production uses the unchanged
full VAAPI→Vulkan DMA-BUF→VAAPI path, not the new CPU converter.

Fresh CLI rejection checks (default output, `--audio none --width 80
--max-frames 1 --no-progress`) on `hevc-main10-pq-reject`, `hevc-main10-hlg`,
`hevc-main10-bt2020-sdr`, `hevc-main10-sdr-full` all fail with the expected policy
diagnostic and no output. PQ says explicit10-bit HEVC/AV1 is required and tone
mapping/10→8 is unimplemented. No default/fallback production HDR→SDR was enabled.

Historical H.264 comparator was freshly rerun on the preserved original pair:
raw packet1A **FAIL** (.102/.103 SEI string), semantic1B/decoded1C/structure2
PASS, whole-file bytes DIFFERENT, no exact-build attestation. It is historical
evidence, **never** promoted to byte PASS. This is distinct from the current
canonical post-polarity H.264 retained baseline which passes all three runs.
No validation/lifecycle/resource ownership source changed. Earlier real
3000-frame/FD/fault/AAC hardware evidence is reused, not called a fresh stress
run; ignored hardware tests are not counted as passes.

Fresh static: release workspace build, default tests and
`asciiflow-media/encode-characterization` tests each210 PASS/0 FAIL/79 ignored;
both all-target Clippy modes with `-D warnings`, fmt and diff check PASS.
All **317 actual unique SPIR-V paths** validate with `spirv-val --target-env
vulkan1.3`. Ten Python audit contract tests PASS, including equal-hash corrupted
clipping rejection (repeatability alone cannot fake numerical PASS).
Source/wrapper identities and aggregate gate evidence are retained in
[qualification](../tests/fixtures/tone-map/c2b-qualification.json).

## Required 48-item closure record

| # | Item | Observed result |
| --- | --- | --- |
| 1 | C-1 input semantics | Raw nonlinear BT.2020 unchanged; explicit new signed boundary |
| 2 | Standard basis | BT.2407 §2 + BT.2087 display branch; primary tables checked |
| 3 | Limiter policy | Target-linear component clamp[0,1],1=100nit; engineering policy |
| 4 | BT.2020 primaries | R(.708,.292),G(.170,.797),B(.131,.046),D65 |
| 5 | BT.709 primaries | R(.640,.330),G(.300,.600),B(.150,.060),same D65 |
| 6 | BT.2020→XYZ | Precise xy-derived matrix above/Decimal70 fixture |
| 7 | BT.709→XYZ/inverse | Precise xy-derived matrix above/Decimal70 fixture |
| 8 | Combined matrix | Nearest f64, independent separate XYZ equivalence PASS |
| 9 | Independent vectors | 41 Decimal70 vectors; deterministic JSON/TSV |
| 10 | White | Exact neutral [1,1,1] target endpoint PASS |
| 11 | Black | Exact [0,0,0] endpoint PASS; uv undefined guard |
| 12 | Neutrals | No tint; interior preserved; below/above capability controls PASS |
| 13 | Preclip Y | Max4.44e-16 normalized; XYZ max6.66e-16 PASS |
| 14 | Unbounded intermediate | Retained; ranges/negative/high counts reported |
| 15 | Negative source | Both-excursion129600; negative-only0; synthetic control PASS |
| 16 | Above-one source | Above-only672788; no preclip PASS |
| 17 | Target containment | All2073600 pixels finite/cube-contained PASS |
| 18 | Boundary | ±epsilon/zero/one controls, exact defined clipping PASS |
| 19 | Idempotence | Unit grid and every canonical pixel PASS |
| 20 | Interior identity | Bit-exact including signed zero PASS |
| 21 | Component monotonicity | Ordered boundary sweep PASS |
| 22 | Green | Exact C-1 vector;102.47186658→71.51686788nit |
| 23 | Cyan | Exact C-1 vector;103.64023975→78.73609941nit |
| 24 | Yellow | Exact C-1 vector;100.76224069→92.78076846nit |
| 25 | One component clipped | 343484pixels |
| 26 | Two components clipped | 161835pixels |
| 27 | Three components clipped | 429811pixels |
| 28 | Luminance loss | ΔY mean−5.65919nit;abs p95=30.955,max31.5818;characterization |
| 29 | Chromaticity loss | uv p95=.0730753,p99/max=.106291;hue/chroma+guards retained |
| 30 | Collisions | Sample161excess distinct inputs/3groups;many-to-one recorded |
| 31 | Canonical fixture | Full1920×1080 C-1 frame, patches/gradients+vector controls |
| 32 | Retained C-2B digest | a7e7ffe6…3471af, all3 identical; final-script repeats identical |
| 33 | Complete CPU pipeline | PQ P010→HDR ASCII→C-1→C-2B→nonlinear709 PASS |
| 34 | Glyph/coverage | Mismatch0, coverage/linear composition unchanged |
| 35 | Builtin | Color/monochrome integration PASS |
| 36 | FreeType | Bundled fixture color/monochrome integration PASS |
| 37 | CPU sanity | 191.2–198.6ms/frame,5.04–5.23fps;no perf gate |
| 38 | C-1 retained digest | d0eb86dc…8c1e6b,3/3 unchanged |
| 39 | SDR regression | 5cases×3, full-file/decoded/timestamps/metadata PASS |
| 40 | PQ preserve regression | 2cases×3, all retained tiers PASS |
| 41 | Production rejection | PQ default8/HLG/BT2020SDR/fullrange all reject/no output |
| 42 | Historical H.264 | Raw1A FAIL;1B/1C/2 PASS;never byte PASS |
| 43 | SPIR-V | All317 actual modules Vulkan1.3 PASS |
| 44 | Workspace/static | Build/tests bothmodes210/79ignored/Clippy/fmt/diff PASS |
| 45 | Loss properties | Y/hue change/detail loss,not reversible;explicitly characterized |
| 46 | Standards vs policy | Colorimetry derived; exact signed extension/target limit project choice |
| 47 | C-3 justification | Justified; not started |
| 48 | Final C-2B status | SEALED, no remaining required gate |

Stage 5.3C-3 Vulkan HDR→SDR reference implementation is justified.
No C-3 or production HDR→SDR work was started.
