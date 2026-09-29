# Stage 5.3C-2A: gamut domain and standard clarification

Historical snapshot: the not-implemented/not-qualified limiter statements in
this sealed clarification describe the C-2A checkpoint. Subsequent
[C-2B closure](stage5.3c2b-target-volume-cpu.md) qualifies the separately named
primary-conversion + target-volume CPU reference. Annex5 remains abandoned;
this clarification's original evidence and decisions are not changed.

Final status: **SEALED — Outcome C / formula outcome F3** (2026-09-29).
This seals an evidence-backed **design decision**, not a successful Annex 5
mapper. Annex 5 is abandoned as AsciiFlow's executable reference: printed
Eq(5-4) is inconsistent, and no defensible primary-source correction was
obtained. C-2 remains **NOT SEALED / implementation NOT STARTED**. Its former
state, **BLOCKED pending C-2A decision**, is resolved by reselecting the method,
not by claiming that the old integration passed. No C-3 work is authorized.

Only audit scripts, independent tests, evidence and docs changed. HEAD remains
`4d95a7e2c6560e061510a6b27ee685c1db08ae0a`. C-1 source, generator, vectors,
identity/SHA256SUMS, production/planner/CLI/encoder and shaders are untouched.
The previous [C-2 audit](stage5.3c2-gamut-map-cpu.md) and its JSON are preserved.

## 1. Source ledger and bounded search

The [machine-readable ledger/formula inventory](../tests/fixtures/tone-map/c2a-source-ledger.json)
records URLs, identities, hashes, literal formula fields, unavailable text as
`null`, search queries and provenance classification. ITU status/errata search
was performed on 2026-09-29; UTC status snapshot time 02:05:27. Snapshot HTML
hashes identify fetched pages, not stable normative document identities.

| Source | Authority and observed availability |
| --- | --- |
| [BT.2407 status](https://www.itu.int/pub/r-rep-bt.2407), [publication](https://www.itu.int/pub/R-REP-BT.2407-2017) | Official catalog lists only BT.2407-0 (10/2017), approved2017-10, **In force (Main)**; no later revision/correction listed |
| [BT.2407 official PDF](https://www.itu.int/dms_pub/itu-r/opb/rep/R-REP-BT.2407-2017-PDF-E.pdf) | Official ITU Report, 4,552,615 bytes, SHA `14d626eeb33524038066cead3e8aef54dea6b3b3f7dd4073b06fa1569b2fb676`; A5.5–A5.6 pp37–38 visually checked again |
| [SMPTE journal](https://journal.smpte.org/periodicals/SMPTE%20Motion%20Imaging%20Journal/126/3/19/) | Original research: Florian Schweiger, Tim Borer, Manish Pindoria, *Luminance-Preserving Color Conversion*, 126(3), pp45–49, April2017; DOI **10.5594/JMI.2017.2660698**; full text requires sign-in/membership |
| [SMPTE 2016 conference](https://journal.smpte.org/conferences/SMPTE%202016%20Annual%20Technical%20Conference%20and%20Exhibition/15/) | Same authors, *Luminance-Preserving Colour Conversion*, October2016; DOI **10.5594/M001708**; full text requires sign-in/membership |
| [BBC bibliography](https://downloads.bbc.co.uk/rd/pubs/papers/HDR/BBC_HDRTV_List_of_Standards_and_Publications_v2.pdf) | Employer's December2017 publication list p2 explicitly confirms conference→journal republication; SHA `07b2ee2945d418d930d0ded34a21e3ae4357d0f3b363f24d01b71bb7258002e0`; contains no formula |
| [WP6C contribution151](https://www.itu.int/md/R15-WP6C-C-0151/en) | BBC working contribution towards BT.[2020TO709], dated2016-10-10 / posted2016-10-11; catalog public, document **TIES-restricted**; no working-text formula available |
| [BT.2446-1](https://www.itu.int/dms_pub/itu-r/opb/rep/R-REP-BT.2446-1-2021-PDF-E.pdf) | Official C-1 source: §4.1 Tables2–3 pp8–9, Table1 pp6–7 and §6.1 pp18–19 rechecked; SHA `c1571348423f1a7b3881b13ed12f877377654ea0d7bac870db38f004c9efb285` |
| [BT.2087-0](https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.2087-0-201510-I!!PDF-E.pdf) | Official Recommendation referenced by BT.2407 N↔L, Annex1 Case1 / Note2 p3; SHA `19f870d9aa201c35e063e6b6eba43ee57878af8af7b12a02f4631fd1a955274a` |
| [BT.2408 status](https://www.itu.int/pub/r-rep-bt.2408), [current PDF](https://www.itu.int/dms_pub/itu-r/opb/rep/R-REP-BT.2408-9-2026-PDF-E.pdf) | Current Main is **BT.2408-9 (03/2026)**, not the older -4 search result; §5.3 p25, A7.1 p77 / A7.2 p79; SHA `1588dd95b1cd0723f435df2305a6ba12ce92e779054d2b98b7fdaa7d70adbc19` |

Separate official-domain searches for `BT.2407 corrigendum`, `erratum`,
`correction`, and `equation 5-4` found no published ITU correction as of the
qualification date. This is a bounded search result, **not** proof that no
erratum exists anywhere. Some web fetches of individual publication pages timed
out; the series status page and official PDF were successfully fetched.

Exact-title/DOI, BBC R&D/downloads and author searches found bibliography,
abstracts and access-controlled publication records, not accessible original
equations. No credentials, purchase, author contact or access-control bypass
was attempted. Journal/conference/working-document **formula, conditions,
alpha/beta and figure comparison are unavailable**, not presumed equal to ITU.
Conference→journal republication is confirmed; contribution→final Annex5
equation-level lineage remains unverified. The published method association is
not enough to assert a repaired expression. No third-party implementation is
used as correction authority. F1/F2 are therefore unsupported; **F3 is selected**.

## 2. Literal printed formula, proof and diagnostic candidate

Symbol-preserving transcription of printed Eq(5-4), no inserted exponent:

```text
q = beta^2 + (alpha-beta)*(r+beta-1)
f(r) = r-alpha/(beta-alpha)^2*[beta-sqrt(q)]  1-beta < r <= 1+alpha
       r                                     r <= 1-beta
       1                                     r > 1+alpha
alpha = distance(w,p2020)/distance(w,p709)-1
beta = 0.2
```

This formula's bracket is **not squared** in the rendered PDF. It was checked
visually, not inferred from extraction. For alpha>0, beta>0, alpha!=beta:

```text
f(1-beta) = 1-beta
f((1+alpha)-) = 1+alpha+alpha/(alpha-beta)
f((1+alpha)+) = 1
f'(r) = 1+alpha/(2*(alpha-beta)*sqrt(q))
f'((1-beta)+) = 1+alpha/(2*beta*(alpha-beta)); identity side = 1
f'((1+alpha)-) = 1+1/(2*(alpha-beta)); constant side = 0
f''(r) = -alpha/(4*q^(3/2)) < 0
```

For alpha>beta, derivative is >1 throughout the roll-off, endpoint is >1,
and the constant branch jumps down: the full function is not nondecreasing,
not contained in [0,1], and not a smooth identity extension. For 0<alpha<beta,
the derivative decreases from the stated lower to upper value; signs determine
whether the roll-off increases, decreases, or turns at
`sqrt(q)=alpha/(2*(beta-alpha))`. Its range is the endpoint extrema plus this
stationary value if inside the branch. For fixed beta=.2 the upper derivative
is negative throughout 0<alpha<beta, so monotonic roll-off fails. With
0<beta<=1 and alpha>0 the upper value cannot be 1: that would require
alpha-beta=-1. Alpha=beta is undefined (division by zero). Alpha=0 reduces to
identity up to1 with a nonsmooth derivative at1; it does not repair other cases.

At **alpha=.5, beta=.2**: lower=.8; upper-left=**19/6 = 3.166666…**;
upper-right=1; jump=-13/6; lower right derivative=31/6 versus1;
upper left derivative=8/3 versus0. Roll-off range [.8,19/6], increasing;
the whole piecewise function is not monotonic. Alpha=.1/beta=.2 additionally
gives upper=.1 and derivative -1.5→-4. These numerical facts and singular
alpha=beta remain in [the Decimal-70 audit](../tests/fixtures/tone-map/c2a-formula-audit.json).

Figure A5-4 and surrounding text require initial identity, smooth roll-off and
saturation at1. The literal expression cannot produce that qualitative shape.
No parameters were fitted to the picture.

A separate **derived candidate, NOT normative**, follows from control points
P0=(1-beta,1-beta), P1=(1,1), P2=(1+alpha,1):

```text
r(t)=1-beta+2*beta*t+(alpha-beta)*t^2
f(t)=1-beta+2*beta*t-beta*t^2       0<=t<=1
r-f=alpha*t^2
f(r)=r-alpha/(alpha-beta)^2*[sqrt(q)-beta]^2   alpha!=beta
t=(r-1+beta)/(2*beta), f=r-alpha*t^2          alpha=beta
```

For alpha>0, 0<beta<=1, dr/dt=2[beta+(alpha-beta)t]>0 and
df/dt=2beta(1-t)>=0. Endpoints are (1-beta,1-beta), (1+alpha,1);
df/dr is1 at the lower endpoint and0 at the upper. Thus the candidate meets
those boundary constraints, including the equal-parameter limit, but they do
**not** establish original-author intent or normative correctness. The script
evaluates only parametric mathematical diagnostics, never pixels/rays. No
candidate is installed as an executable project reference. Its existence does
not upgrade F3 to F2.

## 3. C-1 semantics and handoff: no alteration justified

Direct reinspection of Method A §4.1 Tables2–3 confirms the implemented
normalization, power1/2.4, luma coefficients, three logarithmic/knee/exponential
steps, chroma scale `Y'SDR/(1.1Y')`, and asymmetric luma adjustment
`Y'TMO=Y'SDR-max(.1Cr',0)`. RGB is the exact inverse of BT.2020 Table4 NCL.
No literal discrepancy was found. Black's continuous-limit guard and numerical
stability algebra are already qualified C-1 decisions, not new C-2A changes.

The formal output is nonlinear corrected YCbCr; §4.1 permits reconstruction
to BT.2020 RGB. Its luma and the neutral-only diagnostic are not coloured
photometric Y. Independent Decimal vectors demonstrate negative/>1 RGB from
the literal equations. Therefore conclusion **C**, not an unsupported assertion
of A: the cited formulas do not guarantee every reconstructed RGB component is
inside the display cube. Table1 gives nominal 100-nit/0–100% SDR intent but
explicitly says Method A colour-volume management **No**. It is not a proof
that the saturated reconstructed RGB is bounded. These are legitimate raw
mathematical intermediate results, not qualified display-ready pixels. There
is no evidence supporting explanation B (implementation/reconstruction error).

Crucially, the explicit BT.2407-after-tone-conversion sentence occurs in
**Method C §6.1 pp18–19**, not Method A. Method B's §5.2.4 colour-volume
reduction is likewise not a required hidden Method A step. For Method A:
clipping before BT.2407, normalization before BT.2407 and a complete
extended-range handoff contract are **unspecified by cited section**. Do not
import these steps from another method or silently repair C-1.

BT.2087 Case1 provides display power2.4 and Note2 permits appropriate
negative-sign handling outside the nominal interval. BT.2408-9 §5.3 discusses
sign-reflected transfer extensions as application-dependent. Neither proves
Annex5 cube membership nor resolves positive-only Y>white.

## 4. General feasibility and canonical taxonomy

Annex5 A5.1 operates in linear colourimetry; A5.2 models primary intensities
in [0,1]; A5.4 intersects that cube with the desired-Y plane. A5.6 requires
nonempty effective source/target gamuts and usable white-to-boundary ray
intersections at the same Y, not merely a fixed chromaticity triangle. Source
excursions with feasible Y do not prove mathematical impossibility: the radial
upper branch can saturate distances beyond a source boundary when the geometry
exists, but this is not the source-cube/reversibility qualification originally
requested. D1 is the explicit conservative qualification policy, not a claim
that every out-of-source chromaticity is algebraically unmappable. ClassB has
the stronger algorithm-independent impossibility below.

Let k_i>0, sum(k_i)=1 be the target normalized primary-matrix Y row, P>0
the nominal white luminance, and legal component intervals [l_i,u_i]. Then
`P*sum(k_i*l_i) <= Y <= P*sum(k_i*u_i)` for every legal RGB. Conversely the
segment joining the all-lower and all-upper corners lies in the box and its
linear Y visits the entire interval. This is a necessary **and sufficient**
test for existence of *some* legal target RGB at a desired Y; it promises
neither preserved chromaticity nor hue nor an Annex5 mapping.

For [0,1]^3 at P=100, the interval is [0,100] nit. Y>100 or Y<0 is impossible,
independently of gamut-mapping algorithm. At100 only white is possible, at0
only black. This is a volume/domain proof, not a sample-only argument.

The reusable audit accepts BT.709/P3-D65/BT.2020 or custom xy primaries,
white xy, peak, three component intervals and desiredY, derives positive
weights from the normalized primary matrix, rejects degenerate/nonfinite
geometry and uses no optimizer. It does not accept an above-peak Y via epsilon:

```sh
python3 tests/fixtures/tone-map/audit-c2a.py feasibility \
  --primaries bt709 --peak 100 --range 0 1 0 1 0 1 --desired-y 103.64
python3 tests/fixtures/tone-map/audit-c2a.py feasibility \
  --primaries p3-d65 --peak 100 --desired-y 50
```

Canonical pixels are unchanged. Source excursion remains **802,388 / 2,073,600**
(38.695409%); negative samples129,600, >1 samples802,388, their intersection
129,600. Negative components194,400; >1 components1,056,216. Original positive
Y>100 count **194,434** is preserved, never replaced by a signed-power result.
The taxonomy uses the original (.2627,.6780,.0593) diagnostic coefficients
and Y epsilon1e-12; future precise-primary colourimetry must be separately
qualified, not retroactively substituted into the old count. Monotone power
maps nominal nonlinear [0,1] to the same linear cube.

| Mutually exclusive category | Pixels | % of all pixels |
| --- | ---: | ---: |
| Inside source effective gamut / strict source+Y feasible | 1,271,212 | 61.304591 |
| Class A: outside source, nonnegative, 0<=Y<=100 | 478,354 | 23.068769 |
| Class B: outside source AND Y>100 | 194,434 | 9.376640 |
| Source excursion with negative component; Y unclassified without chosen extension | 129,600 | 6.250000 |
| Infeasible **only** due Y (source-valid) | 0 | 0 |

Thus source-only *known* failures=23.068769%, both=9.376640%, unknown-Y but
source-invalid=6.25%; source-invalid total must not be added again as a
disjoint category. Under a separately labelled sign(v)|v|^2.4 **sensitivity
experiment only**, all negative cases have feasible Y, ClassA becomes607,954
(29.318769%), ClassB stays194,434, Y<0 count0. That experiment is not adopted
as C-1 or production semantics. Complete intersections, counts, percentages
and spatial attribution are in [c2a-taxonomy.json](../tests/fixtures/tone-map/c2a-taxonomy.json).

Independent examples stay green102.47215747, cyan103.64026949,
yellow100.76242406 nit, all positive-only. Blue is a different case: >1
component but feasible Y, illustrating why A and B require different policy.

All six saturated RGB/CMY patches have source excursions (6×64,800=388,800);
red/magenta account for every negative pixel; green/cyan/yellow contribute
194,400 of the194,434 Y failures. Neutral/skin patches and the neutral ramp
have zero excursions. The synthetic RGB gradient contributes **413,588 /
691,200 = 59.836227%** source excursions and34 Y failures. Therefore this is
not solely a handful of saturated patch points, but this deliberately broad
BT.2020 gradient is not evidence of incidence in ordinary camera footage.
Existing legal PQ fixtures are synthetic qualification sources; no owned or
clearly licensed natural-content PQ clip was found in the project. Natural
content characterization is **unverified**, non-gating, not fabricated and not
replaced with copyright-unclear downloads.

## 5. Policy decision and architecture

The following matrix compares actual contracts, without a subjective score.
Every option can be deterministic only after its domain/errors are specified;
none of the options below has been implemented in this stage.

| Policy | Testability / standards footing | Containment and production behaviour | Loss / boundary / decision |
| --- | --- | --- | --- |
| D1 strict Annex5 domain | Direct cube assumptions; precise reject gate | Rejects38.695409% of canonical pixels; partial subset is not full integration | Preserves valid-domain Y; honest but insufficient whole-frame production policy; **retain as research guard only** |
| D2 pre-limit source then Annex5 | Must specify standard-grounded source limiter; cited MethodA has none | Could manufacture source/Y validity, but BT.2408 target/display clipping does not mandate this source pre-step | Changes incoming Y/chroma and irreversibility before alleged pure mapper; **reject here** |
| D3 reduce Y inside gamut mapper | Engineering combined tone/volume policy; must define new transfer | Can address ClassB; requires a new algorithm and quality qualification | Overlaps C-1; not strictY; **not selected**, no need to retune sealed MethodA |
| D4 change C-1 | Only justified by proven literal mismatch | No such mismatch; limiting C-1 to fit Annex5 would corrupt retained oracle | **Reject**, preserve C-1 byte contract |
| D5 another BT.2407 method | §2 N→L / primary matrix / target clip / L→N is independently specifiable; other annexes need their own domain audit | §2 can create bounded target RGB without source-gamut rays; no Annex5 formula blocker | Simple clipping has known hue/luminance/detail costs; **select §2 as next reference basis**, not other annexes by assumption |
| D6 explicit practical limiting | BT.2408-9 A7.1/A7.2 permits post-conversion clipping; §5.3 supports explicit signed extension | Allows defined target capability [0,1] at100nit, no silent fallback | Many-to-one, may increase Y when negatives removed or lower Y when positives capped, loses hue/saturation/round-trip information; **select as explicit project boundary with D5** |

Chosen future **reference** policy: **D5 (§2 primary-matrix conversion) + D6
(explicit target capability limiting)**. This is an AsciiFlow design decision
using standard-described/allowed engineering operations, **not** a normative
mandate for MethodA and **not** an Annex5 correction. Do not imply D6 is
directly prescribed for C-1: BT.2408's illustrated workflows are BT.2100/other
primary conversions, and applying their capability limit to this intermediate
is the project's explicit policy choice. D2 source preclipping is not selected.

For the next CPU-reference stage, the research contract is:

```text
HDR ASCII linear BT.2020 nits
 -> unchanged C-1 MethodA raw nonlinear BT.2020
 -> explicit display-linear sign-reflected power2.4 conversion boundary
 -> precise D65 primary-derived BT.2020-to-BT.709 matrix
 -> explicit target-linear capability limiter [0,1], nominal white100nit
 -> bounded BT.709 linear reference (future inverse-power/output work separately qualified)
```

The audit's signed experiment characterizes the proposed future boundary,
but **does not change the existing C-1 contract**. The future qualifier must
measure negative/positive limiting counts, displacement and ΔY, not assert
strictY/reversibility. Invalid/nonfinite inputs must reject, not be turned into
successful clipping. Legal unbounded C-1 intermediates must be distinguished
from errors and from display-ready data. Output metadata can only describe
the final bounded target/transfer, never the raw intermediate; codec/range
tagging is deferred and production HDR→SDR stays disabled.

Keep MethodA as an independent sealed tone oracle. Replace the old **pure
Y-preserving gamut** boundary with a named **primary conversion + colour-volume
capability limiting** boundary. Do not introduce a combined adaptive tone/gamut
algorithm just to preserve the old roadmap. The final limiter can change Y:
this is acknowledged target-volume reduction, not a second hidden tone curve.
No hard-clipping production default/fallback is enabled by this decision.

## 6. Reproduction and unchanged regressions

```sh
bash scripts/qualify-tone-map-cpu.sh /tmp/asciiflow-c2a-c1-regression
python3 tests/fixtures/tone-map/test-c2a-audit.py
python3 tests/fixtures/tone-map/audit-c2a.py taxonomy \
  /tmp/asciiflow-c2a-c1-regression/method-a-run1.bin > /tmp/c2a-taxonomy.json
cmp /tmp/c2a-taxonomy.json tests/fixtures/tone-map/c2a-taxonomy.json
python3 tests/fixtures/tone-map/audit-c2a.py formula > /tmp/c2a-formula.json
cmp /tmp/c2a-formula.json tests/fixtures/tone-map/c2a-formula-audit.json
```

Fresh C-1 runs1/2/3 each retain
`d0eb86dcc265fe7f675d87055f038d258176e331eb2aac412ba7358a6c8c1e6b`,
and input retains
`a6246d4e5e34c1c69c9cd629e35d82a0f1e8af387436044c82483511bd749f33`.
All three taxonomy outputs match the checked-in report; formula output matches
its retained JSON. All eight independent audit tests pass, including custom
primaries that exposed both directions of white-endpoint rounding error; the
tool now uses a normalized weighted mean, not an above-peak tolerance. The
original `c2-domain-audit.json` bytes remain unchanged. No clipped image, gamut
output digest or quality claim exists.

Fresh Intel `/dev/dri/renderD128` production runs use the unchanged scripts:

```sh
bash tests/baselines/media/generate-pq-production-v1.sh tests/fixtures/codecs /tmp/asciiflow-c2a-pq
python3 tests/baselines/media/verify-pq-production.py /tmp/asciiflow-c2a-pq \
  /tmp/asciiflow-c2a-pq-verification.json --input-directory tests/fixtures/codecs \
  --check-baseline tests/baselines/media/pq-production-v1.json
bash tests/baselines/media/generate-post-polarity-v2.sh /tmp/asciiflow-c2-input8.mp4 \
  tests/fixtures/codecs/hevc-main10-canonical-v1.mp4 /tmp/asciiflow-c2a-sdr
```

The retained 8-bit input SHA is6e5c214b…0dce6b and 10-bit input SHA is
df69c98c…8fccda; full identities remain in their baseline JSON/generators.
Five SDR profiles ×3 match current whole-file and decoded framemd5 baselines.
Two PQ-preserve profiles ×3 pass retained packet/decoded/structure/whole-file
tiers1A/1B/1C/2/3 and metadata/timestamps. These are **existing production
regressions**, not qualification of the proposed limiter. Historical H.264
.102/.103 raw-packet difference policy remains unchanged: strict raw1A **FAIL**,
never relabelled PASS; same-source C-2 historical-oracle evidence is reused.
No unchanged3000-frame lifecycle stress is repeated or falsely claimed fresh.

Both `cargo test --workspace` and the measurement mode
`--features asciiflow-media/encode-characterization` pass **201/0/79**
(passed/failed/ignored) each. Ignored device tests are not fresh hardware PASS.
Both `cargo clippy --workspace --all-targets [measurement feature] -- -D warnings`
modes, `cargo fmt --all --check` and `git diff --check` pass. All actual **317**
cached SPIR-V paths pass `spirv-val --target-env vulkan1.3`; no shaders changed.
Toolchain/native exact-byte scope remains the sealed C-1 / media ledger scope.

[Retained C-2A qualification record](../tests/fixtures/tone-map/c2a-qualification.json)
stores the current production binary/lock identities, exact script commands,
five SDR file/decoded hashes, two PQ file hashes and checked tier results,
static counts and reused-evidence boundaries. This is an audit qualification
record, not a new media baseline or a proposed-limiter PASS.

Audit identities (separate from the unchanged C-1 SHA256SUMS):

| File | SHA-256 |
| --- | --- |
| audit-c2a.py | `b592f3a19f24f473dab56c51e34267a81ffacafc0cd8a8827c8ac1179717016a` |
| test-c2a-audit.py | `b5cdfd4c3dbdcc329bbc4cb92be2810fd1aa6ea4f3817bdf04fa695eca29c0b2` |
| c2a-taxonomy.json | `2c6a4f9b1ddbd166073a9f86301228e43194333c2bb20f4bd114eb37ec143869` |
| c2a-formula-audit.json | `f339f500b76c4a30754ca288d44035158ddef0b2ee750760857ce94e1c21adbb` |
| c2a-source-ledger.json | `93cddba5bcb7f66c211b02c736179445c6ca47bd2b57620cd9dbf9fc5cfe498c` |
| c2a-qualification.json | `69bd73fcca8a83165a5473a1cbc03d6a6aacb614c5675a865ef408b6c65525c4` |

## 7. Closure and exact next step

All13 clarification questions now have explicit answers: current -0 official
status; no published correction found; literal formula inconsistent; originals'
equations unavailable; no corroborated repair; literal C-1 unchanged; cube
source assumption; general impossibility proof; complete conditional taxonomy;
D5+D6 future policy; project decision using standard-allowed operations;
Annex5 abandoned; explicit volume-limiting boundary instead of pureY mapping.

Remaining uncertainty: access-controlled originals might later establish F2,
but have not done so; no natural-content incidence/visual quality evidence;
proposed limiter not implemented or qualified; output encoding and metadata
remain future work. None is hidden as an Annex5 PASS. Original-source access
is no longer a perpetual blocker because this stage explicitly chooses OutcomeC.

Exact next-stage recommendation: **a CPU-only §2 matrix + explicit target-volume
limiting reference qualification**, with independent precise-primary vectors,
signed-domain controls, target containment, clipping/ΔY accounting and unchanged
C-1/media regressions. It must not claim strictY or reversibility. This is a
recommendation, not implementation in this turn. Stop here; no mapper/C-3/GPU/
production HDR→SDR work is started.
