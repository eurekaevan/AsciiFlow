# Stage 5.3C-3 — independent Vulkan HDR→SDR reference

Status: **SEALED** (2026-09-30), under the reviewed C3B P2/N3 contract.
Production HDR→SDR remains disabled. C-4 is justified but has not started.
This independent internal qualification
does not replace or modify sealed C-1/C-2B CPU f64 references.

## C3B numerical precision contract review (current)

The old ≤2 UNORM16 gate was evaluated unchanged: M0 B+C max4/count5 and
isolated C max3/count1 still fail. No project history, standard requirement,
output-format derivation or perceptual justification was found for that
threshold. It remains a historical engineering diagnostic, not a hidden PASS.
Both the original11 and the best-f32 six outliers are preserved.

The review adopts **P2 / N3**, derived from half a limited-range 10-bit chroma
code before rounding, not an observed maximum: finite/bounded nonlinear RGB,
component absolute error≤1/1792, test-only limited BT.709 4:4:4 10-bit YUV
distance≤1 and count>1=0. Exact glyph/coverage/clip masks remain separate.
See [derivation and scope](numerical-qualification.md). This is not an ITU or
perceptual tolerance, nor a production transfer/subsampling/codec qualification.

Intel MTL actually reports shaderFloat64=true. M1/M2/M3 were executed; no
variant closes the full B+C old gate. Unselected Float64 remains an opt-in
experiment, excluded from normal construction and production requirements.
Internal normal qualification now selects common-scale B and compensated
matrix C (mode262); the CPU f64 oracle and production path are untouched.

Final mode262 canonical N3 and HEVC/AV1 all 300-frame real-source parity have
passed. Both codec inputs yield glyph/R8 coverage/clip-mask mismatch0 and
Validation0. Each has 622,080,000 pixels and 1,866,240,000 RGB components:

| Diagnostic | Exact components | Distance1 | Maximum / above1 |
| --- | ---: | ---: | ---: |
| RGB8 |1,866,240,000 |0 |0 /0 |
| RGB10 |1,865,918,988 |321,012 |1 /0 |
| RGB12 |1,864,880,238 |1,359,762 |1 /0 |
| RGB16 |1,785,382,984 |80,857,016 |1 /0 |
| Y10 |621,975,660 |104,340 |1 /0 |
| Cb10/Cr10, each |622,080,000 |0 |0 /0 |
| Y8 |621,432,000 |648,000 |1 /0 |
| Cb8/Cr8, each |622,080,000 |0 |0 /0 |

Maximum absolute nonlinear errors R/G/B are 7.713773141349023e-6,
1.53791804062553e-5,1.2407979977535621e-5. PTS 0..299000 in timebase 1/50000,
step 1000, exactly 300 frames. Config is 160×90 grid, builtin8x8, standard
` .:-=+*#%@` charset and color=true. CPU consumes the exact downloaded VAAPI
frame; GPU consumes its real P010 DMA-BUF import. This is complete floating
qualification output, not an encoded SDR movie. The600-frame test took 1470.47 s.

Earlier ordinary-mode0 real-source 600-frame data is retained separately.
Final canonical/real N3, dual-slot 3000-frame stresses, FD/resource closure and
performance qualification all pass. Historical UNORM16 failure is not PASS;
its replacement is justified independently above. C-4 is not started.

The 1105.52 s stress repeatedly decodes each preserved 300-frame input ten times;
source PTS resets at each cycle, not a claimed continuous mux timeline. Both
copy/map stages finish before either A/B/C submit; both slots are submitted
before completion/readback. Private slots swap each cycle. First-cycle row-major
little-endian IEEE-f32 RGB SHA256s match every later cycle, including opposite
slots; this is stability evidence in addition to the separate CPU parity test.
No claim of parallel hardware execution on the shared queue is made.

| Codec | Frames | FD before / sampled peak / after | Explicit bytes per slot | Validation through teardown |
| --- | ---: | --- | ---: | ---: |
| HEVC |3000 |4 /21 /4 |75,596,984 |0 |
| AV1 |3000 |4 /21 /4 |75,596,984 |0 |

FD warmup is a complete first 300-frame cycle, then no steady-state growth is
allowed; after complete decoder/mapping/slot/device drop it must equal baseline.
The peak is a sampled observation, not continuous monitoring of every temporary
import or hash-subprocess FD. Float/diagnostic/readback/map buffers remain fixed;
descriptors and fences are created only on preparation and reused thereafter.
Both slots remain in capture-off configuration. Payload excludes allocator
padding, driver/private objects and external surfaces; RSS is complementary,
not a GPU allocation counter. HEVC RSS snapshots are 168444..169952 KiB with
non-monotonic plateau variation; AV1 warms from 118172 to 166644 KiB, then remains
166612..166644 KiB for cycles 2–10. No per-cycle accumulating trend is observed.
The validation observer owns only the stable counter and checks after GPU
device/slot destruction; a deliberately abandoned device retains its live
callback counter along with leaked native objects. Actual device hangs are not
forced. Safe staged/pending guards and injected-failure recovery also pass.

Final performance is a separate 248.14 s run with validation disabled, five
warmup batches excluded and 300 measured frames per mode. The checksum-verified
first legal HEVC frame is downloaded once and repeated as cached host P010.
CPU computes the complete immutable map+A+B+C each frame. GPU includes host
stage/map+A+B+C and final float/cell/counter readback; decoder/download/DMA-BUF
and encoder costs are excluded. It is not production throughput. Both private
slots are capture-off before timing; dual throughput uses whole-pair wall.

| Mode | Throughput fps | Batch wall median / p95 ms | Frames per batch |
| --- | ---: | --- | ---: |
| CPU f64 full |2.238550273 |446.9968735 /459.334835 |1 |
| Vulkan 1-slot |5.682687880 |174.997432 /185.210808 |1 |
| Vulkan 2-slot |5.819598270 |343.4754305 /346.157528 |2 |

| Stage | CPU median / p95 ms | Vulkan 1-slot median / p95 ms | Vulkan 2-slot median / p95 ms |
| --- | --- | --- | --- |
| Map |194.713042 /199.664418 |0.615208 /0.700521 |0.615182 /0.657865 |
| A |32.408113 /33.868986 |0.659219 /0.705521 |0.671797 /1.118333 |
| B |91.121819 /93.984345 |0.778464 /1.386146 |0.781901 /1.276875 |
| C |114.592479 /118.480803 |4.866953 /8.602031 |5.148464 /6.016823 |
| Readback |n/a |165.094564 /170.571387 |165.643628 /166.858162 |

Readback dominates this internal qualification; dual slots improve measured
throughput only about 2.41%, not 2×. No new performance requirement or production
optimization is inferred. Both slots' captured correctness prechecks passed
before timing and all requested samples were checked before writing PASS.
The preliminary 250.63 s run is separately retained: a late static SPIR-V check
briefly overlapped its GPU phase, so final comparison uses the clean second run.

Durable evidence:

- [six frozen outliers](../tests/baselines/tone-map/c3b-frozen-outliers.json)
- [all numerical projections](../tests/baselines/tone-map/c3b-frozen-outlier-projections.json)
- [actual M0–M3 results](../tests/baselines/tone-map/c3b-mixed-precision.json)
- [corrected GPU power edge probe](../tests/baselines/tone-map/c3b-power-edges.json)
- [earlier mode0 full-frame diagnostic](../tests/baselines/tone-map/c3b-real-precision.json)
- [derived contract corpus evaluation](../tests/baselines/tone-map/c3b-contract-evaluation.json)
- [selected canonical N3 and historical gate result](../tests/baselines/tone-map/c3b-canonical-n3.json)
- [selected real-source full-frame parity](../tests/baselines/tone-map/c3b-selected-real.json)
- [selected contract corpus evaluation](../tests/baselines/tone-map/c3b-selected-contract-evaluation.json)
- [selected ordinary SPIR-V audit](../tests/baselines/tone-map/c3b-selected-spirv.json)
- [final dual-codec stress with per-frame hashes](../tests/baselines/tone-map/c3b-stress.json)
- [final source identity and evidence reuse boundary](../tests/baselines/tone-map/c3b-current-source-identity.json)
- [final clean performance](../tests/baselines/tone-map/c3b-performance.json)
- [preliminary performance and disclosed overlap](../tests/baselines/tone-map/c3b-performance-preliminary.json)
- [fresh five SDR regressions](../tests/baselines/tone-map/c3b-sdr-regressions.json)
- [fresh two PQ regressions](../tests/baselines/tone-map/c3b-pq-regressions.json)
- [historical H.264 recheck](../tests/baselines/tone-map/c3b-h264-regression.json)

### C3B final-report ledger (45 requested items)

This current ledger supersedes status statements in the historical snapshots
below; their failed observations are intentionally preserved.

| # | Item | Evidence / current result |
| --- | --- | --- |
|1 | Old gate origin | No normative, output-derived or perceptual basis found; historical engineering diagnostic |
|2 | Six vectors | B+C red(237,725),(472,942),(565,1005),(618,1041),(643,1058); C red(565,1005); original11 retained separately |
|3 | Float-domain error | Six-vector max nonlinear6.482573253605711e-5, relative0.007385359091277231; display-linear and nits projections retained, no perceptual claim |
|4 | UNORM8 | Six vectors exact; canonical whole-frame max1; selected real600frames exact |
|5 | UNORM10 | Six vectors exact; canonical and selected real max1/count>1=0 |
|6 | UNORM12 | Six vectors exact; canonical and selected real max1/count>1=0 |
|7 | UNORM16 | Historical M0 B+C max4/count>2=5; C max3/count>2=1, FAIL preserved; selected real max1 |
|8 | Limited YUV8 | Test-only444; canonical max1; selected real Y max1/CbCr exact; not production OETF or packing |
|9 | Limited YUV10 | Test-only444; canonical and selected real max1/count>1=0 |
|10 | Rounding boundaries | Row L1=1; epsilon1/1792 bounds Y shift0.4888392857 and C shift0.5; half-up distance≤1 across every boundary |
|11 | shaderFloat64 | Actual Intel MTL query true; no llvmpipe qualification |
|12 | M1 | f32 source power, f64 matrix, cast before f32 limiter/power; B+C4/5, C3/1 |
|13 | M2 | f64 matrix retained through clipping/target power; B+C4/5, C3/1 |
|14 | M3 | Source power+matrix+limiter+target power f64 within C; B+C4/3, C2/0 |
|15 | Variants | None passes full B+C old gate; every variant mask0/Validation0; raw captures/projections retained |
|16 | Variant timing | Captured single-dispatch C GPU ms: B+C M0/1/2/3=9.415208/11.307135/51.131561/65.664790; isolated C=8.362916/11.865208/53.236978/158.811819; not stabilized throughput |
|17 | Capability cost | No FP64 selected/required; explicit experiment only, ordinary construction does not enable Float64 |
|18 | Best f32 | Common-scale B+TwoSum/Dekker C, mode262; old gate still fails, N3 passes |
|19 | Bug audit | Immutable constants/f32 bits/residuals, operation order, actual captured terms, transfer/clipping and SPIR-V audited; double capture defect fixed; corrected power-edge test passes |
|20 | Policy | P2 / N3, not empirical max4 fitting |
|21 | Historical gate | Replaced as closure requirement; retained as failed diagnostic, never marked PASS |
|22 | New contract | Final finite/bounded RGB; component error≤1/1792; limited444 YUV10 max≤1/count>1=0; glyph/R8/masks/domain/policy remain exact independent gates |
|23 | HEVC real parity | All300 actual VAAPI/P010/DMA-BUF frames, downloaded-identical CPU oracle, selected mode262 PASS |
|24 | AV1 real parity | Same all300 real frames and exact source identity, selected mode262 PASS |
|25 | Glyph/R8 | Zero glyph and coverage mismatches in each300-frame codec and builtin/FreeType/color/mono synthetic checks |
|26 | Clip masks | Zero canonical original-f64 masks and selected real masks; exact limiter/idempotence preserved |
|27 | HEVC3000 | PASS, ten preserved 300-frame cycles, alternating private slots, every frame hash repeats |
|28 | AV13000 | PASS, same3000-frame stress and cross-slot hash checks |
|29 | FD | Both4→sampled21→4 after complete teardown; steady-state no growth |
|30 | Resources | Fixed75,596,984explicit bytes/slot; buffers/descriptors/fences reused, imports released; RSS plateau described above; guards/failed-submit restaging PASS |
|31 | Validation | Zero in canonical, full real parity, synthetic/fault/staged tests and both3000-frame stresses through slot/device teardown |
|32 | CPU performance | PASS measured300 full-reference frames,2.238550273fps; cached-host scope above |
|33 | Vulkan 1-slot | PASS measured300 capture-off frames,5.682687880fps |
|34 | Vulkan 2-slot | PASS measured300 frames/150pairs,5.819598270fps,343.4754305ms median pair wall |
|35 | Per-pass | Final independent map/A/B/C/readback median/p95 table above; preliminary experiment times remain separately scoped |
|36 | C1 digest | Fresh3runs=d0eb86dcc265fe7f675d87055f038d258176e331eb2aac412ba7358a6c8c1e6b |
|37 | C2B digest | Fresh3runs=a7e7ffe64d61269288c8e9b2e9b22a62f8c16fd2287083386bc8cefb9c3471af |
|38 | SDR | Five paths×3 fresh retained hashes match; complete-decode/metadata/PTS evidence reused from identical-byte C3A artifacts, not claimed rerun15times |
|39 | PQ preserve | Two paths×3 fresh, actual retained verifier1A/1B/1C/2/3 and full decode PASS |
|40 | H.264 | Fresh legacy candidate e45071…24af; unchanged structured comparator PASS, raw1A FAIL and whole-file DIFFERENT/build unattested retained |
|41 | SPIR-V | All709 cached generated files including experiments pass Vulkan1.3 validation; current ordinary qualification OUT_DIR22 modules, no FP64 module; selected C shader Float32 only |
|42 | Static | Default210PASS/79ignored; combined215PASS/115ignored; Python7PASS, clippy-Dwarnings/fmt/diff pass; final source checks repeated after edits |
|43 | Scope | Intel Arc MTL8086:7d55, Mesa ANV26.2.3, iHD26.1.5, FFmpeg8.1.3/libavcodec62.28.103, Rust1.97.1; preserved source/input/output identities |
|44 | Limits | Other GPUs unqualified; empirical corpus not global float error theorem; no production transfer/420 packing/codec claim, no forced real hung-device test |
|45 | C3 status | SEALED under P2/N3; no remaining required gate. Historical failures retained, production disabled; C4 justified, not started |

## C3A numerical and legal-domain closure pass (historical evidence)

**NOT SEALED.** The unsuitable-real-source blocker is resolved by a **new**
legal-domain fixture, but the original numerical gate still fails. No numerical
fix is selected for normal qualification dispatch or production. C-4 remains
NOT SEALED/not started; shaderFloat64 is not silently required.

Durable evidence:

- [11 original failure vectors](../tests/baselines/tone-map/c3-historical-failure-vectors.json)
- [18 actual GPU arithmetic experiments](../tests/baselines/tone-map/c3a-arithmetic-final.json)
- [actual SPIR-V arithmetic/coefficient audit](../tests/baselines/tone-map/c3a-spirv-audit-final.json)
- [all-300-frame software/VAAPI domain audit](../tests/baselines/tone-map/c3a-legal-domain-v1.json)
- [new fixture recipe and identity](../tests/fixtures/codecs/c3-pq-legal-v1.md)

The initial C3 snapshot below and `stage53c3.json.canonical_numeric` are retained
historical observations, not overwritten with more favorable candidate results.
`stage53c3.json.c3a` identifies this pass and its separate evidence artifacts.

### Three independent numerical oracles

The sealed scientific C-1/C-2B f64 oracle remains authoritative. A test-only
shader-contract oracle uses genuine f32 inputs, coefficients, arithmetic and
intermediate rounding, not a cast of the final f64 result. Host `pow/ln/exp`
and correctly fused Rust `mul_add` remain diagnostic estimates: they are not
assumed identical to GPU extended instructions. The third oracle is actual
GPU dispatch, capturing B output, sign-reflected source power, actual matrix
summands and their bits, pre-limit/post-limit/final values and masks.

All 11 historical coordinate/channel samples remain in every candidate trace.
Each pass reports per-channel p50/p95/p99/p99.9/max, plus the final UNORM16
distribution, original-f64 and promoted-input comparisons, first failures and
original/promoted/shader-contract mask counts. Boundary classifications use
exact 0/1 equality, never an epsilon exemption. All pixels, including near-black
and clipped channels, remain in the final gate.

### Arithmetic experiments and result

All six matrix forms were dispatched for isolated C and for B+C with both
original Table 3 arithmetic and an algebraically equivalent common-scale form:
`scale * (nonlinear + .1*y - max(.1*(nonlinear.r-y)/1.4746,0))`.
It cancels forward/inverse NCL denominator round trips; it does not fit any
coefficient or change colorimetry. Matrix forms are current precise neutral-axis,
left/right row sums, explicit FMA, TwoSum/FMA-residual compensated neutral-axis,
and TwoSum/Dekker-4097-residual compensated neutral-axis. High/low coefficient
representations derive solely from the sealed f64 matrix, with every used high
float and low uint bit pattern checked in actual SPIR-V.

| Actual experiment / original f64 oracle | Max U16 delta | >2 channels |
| --- | ---: | ---: |
| Original B+C, precise neutral | 10 | 7 |
| Original B+C, left/right/FMA row sum | 9 | 5 each |
| Common-scale B+C, left row sum | 4 | 5 |
| Common-scale B+C, TwoSum/Dekker residual | 4 | 5 |
| Original isolated C, precise neutral | 11 | 4 |
| Isolated C, left/right/FMA row sum | 5 | 1 each |
| Isolated C, TwoSum/FMA residual | 4 | 3 |
| Isolated C, TwoSum/Dekker residual | 3 | 1 |

**All 18 fail.** The unchanged requirement is maximum ≤2 and zero channels >2.
Compensation materially improves the original blue cancellation but does not
close the complete scientific gate. Candidates are retained as explicit
experiments, not promoted as a working fix. No epsilon snap, sign-forcing,
source preclip, matrix fitting, LUT, approximate transfer, fp16 or GPU Float64
was introduced. The normal feature-only path still selects original mode 0.

### First divergence and near-black amplification

The current ordinary neutral expression compiles into left-associated
FAdd/FSub/FMul with NoContraction; hidden reassociation/FMA is not its cause.
The final C module has 28 FAdd, 36 FSub, 28 FMul, four GLSL.std.450 Fma,
two Pow, and 96 NoContraction decorations. Its four Fma results are **not**
NoContraction-decorated. No Float16/Float64 capability is present.
[Khronos explains](https://docs.vulkan.org/features/latest/features/proposals/VK_KHR_shader_fma.html)
that GLSL.std.450 Fma does not guarantee fused hardware accuracy. An explicitly
unfused residual model reproduces all four historical compensated-C pre-limit
channel bits exactly; this is observed numerical reproduction, not a claim to
have inspected driver machine code. The Dekker experiment avoids that assumption
without adding a device capability.

At blue (1064,1032), original C pre-limit was 8.34465e-7 versus scientific
9.512089085284003e-7, giving code delta 11. FMA residual improves it but still
has matrix residual error about -4.028e-8 despite identical captured source bits.
Dekker compensation reduces the four historical C deltas to **[3,0,1,2]**;
the remaining red (565,1005) first diverges at the source power: GPU red source
bits 1053927230 versus host f32 1053927229. The captured source's exact f64
neutral matrix gives 8.33290025634495e-6; GPU compensated pre-limit is
8.3329005e-6 (about 2.44e-13 matrix error). The scientific original pre-limit is
8.195103345248061e-6. Final GPU/scientific signals are .007650066 versus
.007597097167603198, yielding delta 3. GPU final-power error relative to its
own bounded value is only about 1.93e-9: upstream source-power/input precision
is amplified near black, not predominantly a faulty final transfer.

This establishes a limitation of the **tested** FP32 path, not impossibility
of every FP32 algorithm. A future higher-precision representation/Float64
capability decision must be explicit; it is not authorized or implemented here.

### Legal-domain PQ fixture v1

The independent deterministic integer P010 recipe is 1920×1080, 300 frames at
50 fps, 10-bit 4:2:0, BT.2020 primaries/NCL, PQ, limited, left chroma. It includes
black, near-black/.01/1/10/100/203/400/600–800-nit gray levels, 900-nit designed
highlights with headroom, a full gray gradient, eight color bars and moving
geometry. Quantized actual maximum is 904.3172845983241 nits, not an assumed
900-nit encoded maximum. No decoded value is clamped or peak-renormalized to
qualify it. B-3's 4000/10000-nit fixture and rejection evidence remain unchanged.

```sh
bash tests/fixtures/codecs/generate-c3-pq-legal-v1.sh NEW_OUTPUT_DIRECTORY
```

Final generator runs 3/4/5 are byte-identical, including the manifest; earlier
runs 1/2 used superseded histogram-accounting manifests and are not the three
final-version reproducibility runs. Fixed FFmpeg 8.1.3, its full build/version,
binary, libx265/libaom and Python identities are retained in the fixture manifest.
Source tight-P010 SHA256:
`7e28e4832332a7febe72e9c6b9fb4b7fbad0d141ed204b4f9267f50e2ff87a37`.
Raw bytes: 1,866,240,000. HEVC bytes: 1,927,266; SHA256
`eb8d8c42cd99369b99031900e8861c08f611bfcc662b8d0eb741eb695a0c366a`.
AV1 bytes: 8,502,612; SHA256
`7eb4fd7e7513fc181ad8489e13edbac10a88a8f3a7947d2f958d36a0ce08e7c4`.
Both fixed-tool software decodes reproduce the raw P010 hash exactly.

Each codec's **all 300** project software frames and actual VAAPI downloads were
audited before any RGB clamp: min 0, max 904.3172845983241 nits, negative/>1000/
nonfinite/invalid code counts all 0. Per path, nonzero low-two-bit samples are
583,051,212 / 933,120,000; Y/U/V counts are 498,811,212 / 45,360,000 / 38,880,000.
All four paths have timebase 1/50000, PTS 0..299000, step 1000 (exact 50 fps).
This is decoded-domain qualification, **not** full ASCII GPU parity or stress.
All active raw P010 words also have zero unused low-six bits; malformed packing
is counted before unpacking, matching the existing PQ map shader's rejection
contract. A pure packing negative-control test distinguishes these errors from
valid nonzero low-two-bit precision in the ten-bit samples.

### Closure ordering and regressions

The strict numerical gate is still red. Full legal-source HEVC/AV1 CPU/GPU
parity, real dual-slot 3000-frame stresses, FD before/peak/after equality and
resource-growth closure are not marked PASS. Correctness-first C3 performance
(CPU full pipeline, GPU one/two slots, per-pass timings and stabilization cost)
is **NOT RUN**. Incidental production generator timings are not C3 measurements.

Fresh C-1 and C-2B triples retain d0eb…1e6b and a7e7…71af. Fresh five SDR and two
PQ-preserve production paths each ran three times with unchanged retained file
hashes; full retained decode/metadata/timestamp checks are separate evidence.
Both PQ codecs still reject default 8-bit output before staging. Historical
H.264 remains raw 1A FAIL, coded 1B/decoded 1C/structure 2 PASS, exact build identity
unattested—not reclassified as byte-identical or recovered.

The final-source synthetic builtin/FreeType, color/mono, single/dual-slot and
eight fault checkpoints are rechecked independently of numerical closure.
Workspace/default+measurement-feature tests, both all-target clippy modes,
release build, fmt and diff checks pass. Actual Vulkan1.3 SPIR-V enumeration:
420 Debug + 104 Release = **524 cache files**, all validated; not 524 distinct
algorithms, and no historical module-count target was used.

## Historical initial C3 hard blockers

Actual Intel GPU execution fails the unchanged diagnostic UNORM16 gate:
maximum channel-code difference ≤2, and zero samples above 2.

| Dispatch / authoritative oracle | Maximum code error | Samples >2 |
| --- | ---: | ---: |
| B+C / original f64 HDR input | 10 | 7 |
| B+C / f32 HDR input promoted to f64 | 10 | 6 |
| C only / original retained f64 C-1 | 11 | 4 |
| C only / f32 C-1 input promoted to f64 | 11 | 1 |

No tolerance, oracle, fixture, target-volume policy or production shader was
changed to accommodate these failures. The four comparisons were actually
dispatched, not inferred from a CPU-only f32 simulation.

There is also a real-input domain blocker: the preserved B-3 PQ canonical
source includes4000/10000-nit patches and a10000-nit rectangle. Immutable C-1
normal input ends at 1000 nits. Both HEVC and AV1 first frames, downloaded from
the exact VAAPI surface supplied to Vulkan, contain835,695 post-ASCII
components above1000; CPU maximum is 10000 nits. GPU B counts exactly 835,695
and completion rejects the whole frame. This is real-import/fail-closed
evidence, **not full conversion parity**. No selected subset, source clamp,
peak renormalization or new downloaded source is used.

Resolve numerical correctness and identify a reproducible legal-domain real
source before completing full-frame HEVC/AV1 parity, each 3000-frame dual-slot
stress, FD/resource-growth closure and1080p/300-frame performance qualification.
These gates are **unverified**, not inherited from B-2/B-3 and not PASS.

## Architecture and ownership

Feature `asciiflow-vulkan/hdr-to-sdr-qualification` (forwarded by interop) exposes
an internal qualification constructor, never a production `AsciiBackend`, CLI
flag, planner mode or codec frame.

```text
host P010 / real VAAPI DMA-BUF input
  → unchanged B-2 ascii_map_pq
  → A: ascii_render_hdr_linear → float A: linear BT.2020 RGB nits
  → B: tone_map_bt2446         → float B: raw nonlinear SDR BT.2020
  → C: bt2020_to_bt709_limit   → float A: nonlinear SDR BT.709
  → fence + diagnostics/readback (never encoder input)
```

Existing input copy/map resources are reused. Map submits/waits before the
separate A/B/C command; no intermediate RGB is processed on CPU between GPU
passes. The stages remain separate dispatches with synchronization2 barriers.
No fusion, fp16/Float64, LUT, custom pow/log approximation, SIMD/Rayon or
production integration was introduced. Sealed PQ-preserve shaders are untouched.

Each slot has private packed scalar-f32 A/B buffers (three consecutive floats,
12 bytes/pixel, not padded std430 vec3[]), counters, masks, descriptors, pipelines,
command pool/buffer, timestamps and fence. Forks share device/context/serialized
queue only. Current capture adds a20 float/pixel observation readback and Pass-A snapshot;
they are diagnostic-only, not a third compute intermediate. Counters/observations
are GPU cleared every command. Direct vector paths do not expose uninitialized
A snapshots or fictitious atlas coverage.

Prepared atlas changes, pending-slot preparation and abandoned-context reuse/
fork are rejected before touching backend-owned cells/atlas. C-3 descriptors
are destroyed before replacing their backend owners. Partial initialization is
rollback guarded; unknown fence completion abandons the context and intentionally
retains potentially in-flight resources rather than freeing them unsafely.
Actual device loss/timeout was not forced. Independent review found/fixed two
initial ownership defects and found no remaining concrete unsafe lifetime bug.
Static review does not qualify numerical accuracy or physical failure recovery.

Capability checks use Vulkan1.3, existing PQ/P010 support and actual storage,
workgroup and dispatch limits; no Intel vendor allowlist. Only Intel MTL is
measured. Other implementations remain unqualified.

### Explicit buffer footprint at1080p

| C-3 payload | One slot bytes | Two slots bytes |
| --- | ---: | ---: |
| Float A+B | 49,766,400 | 99,532,800 |
| Capture on: A+B, observations, A snapshot, counters | 240,537,636 | 481,075,272 |
| Capture off: A+B, dummy observations/snapshot, counters | 49,766,520 | 99,533,040 |

These are not peak RSS or complete device allocations. Reused B-2 input/upload/
output/readback/cell/atlas/LUT buffers, allocator padding, driver objects,
imported surfaces and CPU oracle/readback Vecs are additional.
`total_buffer_bytes_per_slot()` includes explicit reused B-2 buffers, but not
allocator/driver overhead. No3000-frame allocation-growth proof is claimed.

## Stage semantics and observations

A uses original HDR glyph decisions, sealed mono luminance, original R8 atlas
and uneven cell boundaries. Foreground normalized to10000 nits is blended with
coverage in linear light and stored in absolute nits; no inverse PQ, YUV or
P010 quantization. Coverage capture records the exact R8 byte, not a rounded
normalized fraction that may differ by one compiler reciprocal ULP.

On66×50 P010 with 13×9 cells, A max RGB absolute error in nits was .001167816615
(builtin mono), .002021329067 (builtin color), .000890594788 (FreeType mono),
.001536981047 (FreeType color). These are bounded-corpus observations, not a
universal qualified epsilon. Glyph mismatches are 0; R8 bytes are identical.
Source low-two-bit precision is 2475/4950 samples (50%), not8 bit<<2. Final
diagnostic code errors are 0/1 on these fixtures. All four font/color configs
passed 200 dual-slot alternations each, checking both outputs, PTS, counter reset,
constant explicit buffer bytes, and rejection of a pending consuming atlas builder.

B uses standard GLSL f32 pow/log/exp for sealed fixed1000→100 Method A,
printed knees and complete Table3 chroma/luma correction; raw negative/>1
output remains observable. GPU negative/>1000/NaN/Inf vector diagnostics each
count 12components and return a terminal error; subsequent black resets counters.
Invalid arithmetic also rejects completion. GPU zero writes after an invalid
sample are not successful output fallbacks.

C uses sign(v)|v|^2.4 without source clipping, nearest-f32 sealed combined D65
matrix coefficients with neutral-axis evaluation, observable unbounded target
RGB, separate component clip[0,1], then inverse display power1/2.4. No CAT,
Y restoration, scaling or BT.709 camera OETF. Direct limiter tests at0/1 use
representable epsilon 2^-12 and verify exact masks/interior bits, idempotence
and component monotonicity. Interior negative-zero bits remain`80000000`;
this is not an asserted codec signaling requirement.

Canonical1920×1080 B+C max RGB absolute error versus original f64:

| Boundary | R | G | B |
| --- | ---: | ---: | ---: |
| B raw nonlinear2020 | 9.29772e-7 | 9.31981e-7 | 1.04810e-6 |
| C pre-limit709 (including B error) | 3.72512e-6 | 2.98425e-6 | 3.09398e-6 |
| Final nonlinear709 | 6.39717e-5 | 9.47470e-7 | 1.59295e-4 |

All normal canonical outputs are finite/in[0,1], invalid diagnostics zero.
Full p50/p95/p99/p99.9/max per channel, worst coordinates, both authoritative
comparisons and all11 original-oracle >2 samples (7B+C +4isolatedC; not11unique
pixels) are durable in [stage53c3.json](../tests/baselines/tone-map/stage53c3.json).
No generous preselected epsilon promotes characterization to a PASS.

Diagnostic UNORM16 rounding is explicitly`floor(clamp(v,0,1)*65535+0.5)`.
B+C channel p50/p95/p99 are 0, p99.9 is1, maxima4/1/10. Isolated C has
p50/p95/p99=0 and maxima4/1/11. This is not production8/10 bit quantization.

Worst shared coordinate(x1064,y1032), blue: original CPU pre-limit9.512089085e-7;
B+C GPU1.0728836e-6; isolated C GPU8.34465e-7. Inverse display power amplifies
these small pre-limit differences near zero. Evaluating f64 inverse power on
the GPU bounded value differs from actual GPU final by only 2.0437e-9 (B+C) /
3.7910e-10 (C). The failure is present before final nonlinear encoding, not
in clip-mask decisions. Isolated-C and promoted-input comparisons exclude B
and input narrowing as sole causes. Forward display power versus matrix
cancellation is not yet separately instrumented: this localizes failure to
C pre-limit arithmetic, not a unique instruction-level cause or proof that
all f32 implementations must fail. No output snap/clamp workaround is used.

Canonical isolated-C0/1/2/3 clip counts exactly equal original f64:
1,138,470/343,484/161,835/429,811. Every low/high channel mask also equals the
promoted-f32-input f64 oracle; mismatches 0, no near-threshold waiver.
Real-dispatch Method-A vectors cover black/.0001/1/10/100/203/400/1000 nits,
RGB/CMY/skin and both knee neighborhoods; neutral/knee groups are monotonic.
Knee-neighbor B max error≈1.08e-5 reflects printed discontinuity/f32 branch
sensitivity, not the ordinary canonical B bound. Vector final UNORM16 max 0.
Green/cyan/yellow pre-limit values are compared, exact masks21/49/28, bounded
[0,1,0]/[0,1,1]/[1,1,0]. No Y-preservation claim is made.

## Reproduction and other gates

Fresh CPU wrappers were actually executed; each output was generated3 times,
byte identical and matched the sealed manifest:

```sh
bash scripts/qualify-tone-map-cpu.sh /tmp/asciiflow-c3-c1-regression
bash scripts/qualify-target-volume-cpu.sh \
  /tmp/asciiflow-c3-c1-regression/method-a-run1.bin /tmp/asciiflow-c3-c2b-regression
```

Input SHA256:a6246d4e5e34c1c69c9cd629e35d82a0f1e8af387436044c82483511bd749f33.
C-1 three outputs:d0eb86dcc265fe7f675d87055f038d258176e331eb2aac412ba7358a6c8c1e6b.
C-2B three outputs:a7e7ffe64d61269288c8e9b2e9b22a62f8c16fd2287083386bc8cefb9c3471af.
CPU sources, fixtures and hashes remain unchanged.

GPU commands executed with real device access:

```sh
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-vulkan \
  --features hdr-to-sdr-qualification --test c3_render \
  -- --ignored --nocapture --test-threads=1
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-vulkan \
  --features hdr-to-sdr-qualification --test c3_qualification \
  gpu_domains_limiter_and_fault_cleanup -- --ignored --nocapture --test-threads=1
ASCIIFLOW_VULKAN_VALIDATION=1 \
  C1_INPUT=/tmp/asciiflow-c3-c1-regression/linear-bt2020-1000-v1.bin \
  C1_OUTPUT=/tmp/asciiflow-c3-c1-regression/method-a-run1.bin \
  C3_REPORT=/tmp/asciiflow-c3-canonical-final.json \
  cargo test -p asciiflow-vulkan --features hdr-to-sdr-qualification \
  --test c3_qualification canonical_f64_and_f32_input_oracles \
  -- --ignored --nocapture --test-threads=1
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop \
  --features hdr-to-sdr-qualification --test c3_hardware \
  -- --ignored --nocapture --test-threads=1
```

Canonical command **currently fails after publishing its report**. Report
creation is exclusive. `bash scripts/qualify-tone-map-vulkan.sh NEW_OUTPUT_DIRECTORY`
refuses existing destinations, uses sealed CPU wrappers and retains failure
exit status. Wrapper syntax was checked; its component commands were executed
individually, not misreported as a successful whole-wrapper run.

Scope: Intel Arc Meteor Lake /renderD128, ANV Mesa26.2.3, iHD26.1.5,
FFmpeg8.1.3-1.fc44/libavcodec62.28.103, glibc2.43-8.fc44, rustc1.97.1
(8bab26f4f)/LLVM22.1.6, SPIRV-Tools2026.1. Base89e5f67864c1d2985792ebd904ae3dbd1b372429
plus uncommitted C-3 draft; no cross-build byte portability attestation.

Five SDR and two PQ production retained paths were freshly run3 times each.
All hashes match, complete decode-back/decoded hashes,300 frames/packets,
timestamps, profile/depth/color metadata pass. Exact unchanged CLI configurations
are retained in `tests/baselines/media/generate-post-polarity-v2.sh` and
`generate-pq-production-v1.sh`; no new C-3 output format was selected.
PQ default8 bit, HLG, BT.2020SDR and full-range actual fixtures reject without
output. An initial check used three nonexistent rejection filenames and is
not policy evidence; corrected checks are in/tmp/asciiflow-c3-policy.eT7kZx.
Fresh historical H.264 comparison: raw1A **FAIL**, semantic1B/decoded1C/
structure2 **PASS**, whole-file **DIFFERENT**, exact build identity unattested.

Static: release workspace build; default and encode-characterization modes
each 210PASS/79ignored; combined qualification features 212PASS/107ignored;
default+combined all-target clippy`-D warnings`, fmt/diff checks. Ignored GPU
tests do not count as PASS. All 458actual cached SPIR-V files (398 debug/60 release)
passed Vulkan1.3 validation. This is a cache-file count, not458 different
algorithms; historical317 is not a target. New modules contain only Shader,
plus StorageBuffer8BitAccess in A; no Float16/Float64 capabilities.

Temporary evidence:/tmp/asciiflow-c3-render-final.log,
/tmp/asciiflow-c3-domains-complete.log, /tmp/asciiflow-c3-canonical-final.{json,log},
/tmp/asciiflow-c3-pq-verification.json, /tmp/asciiflow-c3-sdr-verification.json,
/tmp/asciiflow-c3-historical-h264.log, /tmp/asciiflow-c3-real-import-final.log,
/tmp/asciiflow-c3-final-static/, /tmp/asciiflow-c3-clippy-feature-final.log.
Earlier clippy logs retain the test-only diagnostics; the corrected final
combined-feature check is the separate final log, not a reclassification of
those earlier failed commands.
The durable structured failure record does not depend solely on temporary logs.

## Historical initial C3 implementation ledger (52 items)

| # | Item | This round |
| ---: | --- | --- |
| 1 | Architecture | Feature-only map→A→B→C, production closed |
| 2 | Shaders | Three independent modules, sealed PQ shaders untouched |
| 3 | Representation | Packed3xf32, two GPU ping-pong buffers |
| 4 | Per-slot memory | Payload/accounting exclusions recorded above |
| 5 | A semantics | Absolute linear BT.2020 nits, no PQ/YUV |
| 6 | A errors | Synthetic max .002021329 nits; universal epsilon unqualified |
| 7 | Glyph | Zero synthetic mismatches, not full-real parity |
| 8 | Coverage | Exact R8 bytes for both fonts |
| 9 | Method A | Fixed1000→100, standard GLSL f32 |
| 10 | Vectors | Real neutral/RGB/CMY/skin/knees; final code max 0 |
| 11 | C-1 errors | Ordinary canonical1.04810e-6; knee≈1.08e-5 |
| 12 | Illegal input | Actual GPU counters12 each, terminal errors |
| 13 | Linearization | Sign-reflected power, no source clip |
| 14 | Matrix | Sealed D65 nearest-f32, neutral-axis evaluation |
| 15 | Pre-limit errors | RGB maxima3.72512e-6/2.98425e-6/3.09398e-6 |
| 16 | Masks | Full canonical exact vs promoted-f32 oracle |
| 17 | Green | Mask21, bounded[0,1,0], pre-limit compared |
| 18 | Cyan | Mask49, bounded[0,1,1], pre-limit compared |
| 19 | Yellow | Mask28, bounded[1,1,0], pre-limit compared |
| 20 | Counts |0/1/2/3=1138470/343484/161835/429811, exact |
| 21 | Final float | Finite cube; strict final code gate FAIL |
| 22 | U16 distribution | p50/p95/p99=0; B+C channel maxima4/1/10 |
| 23 | >2 samples |7B+C/4isolatedC, hard FAIL |
| 24 | Black/white/neutral | Vectors/canonical, monotonic/zero checks |
| 25 | Geometry |66×50,13×9 cells tested |
| 26 | Low bits |2475/4950 samples nonzero low-two bits |
| 27 | Builtin | Synthetic mapping/render/slots PASS |
| 28 | FreeType | Fractional coverage/synthetic slots PASS |
| 29 | Modes | Color and mono for both fonts |
| 30 | One slot | Synthetic/canonical math exercised; full real unqualified |
| 31 | Two slots |200 alternations per 4 configs, real stress unqualified |
| 32 | Contamination | Outputs/PTS/counter reset/buffer bytes stable |
| 33 | HEVC real | VAAPI/DMA-BUF complete-frame domain rejection PASS; parity unqualified |
| 34 | AV1 real | Same835695 overflows; parity unqualified |
| 35 | HEVC3000 | NOT RUN: numerical/input-domain gates block closure |
| 36 | AV13000 | NOT RUN: same |
| 37 | FD | Full C-3 before/peak/after equality NOT QUALIFIED |
| 38 | Lifetime | Rollback/drop/reuse/pending guard reviewed/tested; stress growth unverified |
| 39 | Validation |0errors in executed tests, full stress unverified |
| 40 | Faults | Eight checkpoints hard fail and safe subsequent reuse |
| 41 | Performance | NOT QUALIFIED; no pre-correctness speedup/optimization claim |
| 42 | Footprint | Explicit buffer payload only, total peak/RSS unqualified |
| 43 | SDR | Five retained paths×3, decode/metadata/timestamps PASS |
| 44 | PQ | Both retained paths×3, full retained verifier PASS |
| 45 | C-1 digest | d0eb…1e6b unchanged,3byte-identical outputs |
| 46 | C-2B digest | a7e7…71af unchanged,3byte-identical outputs |
| 47 | Historical H.264 |1AFAIL;1B/1C/2PASS; build identity unattested |
| 48 | SPIR-V |458cache files, all Vulkan1.3 validation PASS |
| 49 | Static | Build/default+feature tests/clippy/fmt/diff PASS |
| 50 | Scope | Intel MTL/ANV26.2.3/iHD26.1.5/FFmpeg8.1.3 only |
| 51 | Limits | Numerical FAIL, unsuitable real source, remaining hardware/stress/perf |
| 52 | Status | **NOT SEALED**, C-4 not justified/not started |

## C3A final closure ledger (52 requested items)

| # | Item | Current evidence / remaining gate |
| ---: | --- | --- |
| 1 | Seven original B+C blockers | All seven retained by coordinate/channel in immutable historical vectors and all candidate traces |
| 2 | Four original C blockers | All four retained; Dekker deltas [3,0,1,2], not four PASS |
| 3 | First divergent operation | Remaining C565: source Pow red +1 host-f32 ULP; exact captured-source matrix already implies failing final code |
| 4 | Shader f32 oracle | Per-operation f32, independent of scientific f64; intrinsic limitations explicit |
| 5 | SPIR-V audit | Actual compiled module, counts/coefficients/capabilities retained |
| 6 | FMA/contraction | Four Fma nodes not NoContraction; unfused residual model reproduces four historical compensated C results |
| 7 | precise experiment | Original neutral arithmetic NoContraction confirmed; still 11/4 isolated-C failure |
| 8 | Operation orders | Neutral, row-left/right/FMA, two compensation forms × three dispatch/input arrangements =18 actual cases |
| 9 | Matrix bits | All nine nearest-f32 highs and six used coefficient residual uints checked against sealed matrix |
| 10 | Compensation | TwoSum/FMA and TwoSum/Dekker measured; only explicit diagnostic candidates |
| 11 | Selected numerical fix | **None** |
| 12 | Selection reason | Every candidate fails unchanged scientific gate; no candidate promoted to normal mode |
| 13 | No epsilon snap | Strict mathematical comparisons and complete corpus retained; no sign forcing or threshold widening |
| 14 | Pre-limit errors | Isolated RGB maxima ordinary [7.4183e-7,4.8528e-7,7.1534e-7]→Dekker [7.0025e-7,4.6361e-7,4.7692e-7]; distributions retained |
| 15 | Final U16 before/after | Original B+C/C10/11→best tested4/3; still FAIL |
| 16 | >2 count | Original7/4→best B+C5 and isolated C1; threshold remains zero |
| 17 | Stabilization performance cost | NOT RUN, correctness red; no speed/cost selection |
| 18 | Legal fixture design | Deterministic integer P010, grayscale/gradient/colors/motion, shared source for both lossless codecs |
| 19 | Designed peak |900 nits with headroom; actual decoded peak904.317285 |
| 20 | Raw source SHA |7e28…7a37; full identity above and fixture manifest |
| 21 | Low-bit statistics |583051212/933120000 nonzero low-two-bit samples, full plane histograms retained |
| 22 | HEVC identity |1927266 bytes, eb8d…366a, Main10/PQ/BT.2020/limited/left |
| 23 | AV1 identity |8502612 bytes,7eb4…e7c4, Main/yuv420p10le/PQ/BT.2020/limited/left |
| 24 | Software decoded peak |Both all300 frames:0..904.3172845983241 nits |
| 25 | VAAPI decoded peak |Both all300 actual surface downloads: same range |
| 26 | Illegal samples |Negative/>1000/nonfinite/invalid codes and malformed P010 low-six-bit packing all0, before RGB clamp/unpack loss |
| 27 | HEVC real conversion parity |NOT RUN/NOT QUALIFIED; input legality does not prove full map→A→B→C parity |
| 28 | AV1 real conversion parity |Same remaining gate |
| 29 | Glyph parity |Synthetic both fonts/color modes mismatch0; legal full-video glyph gate remains open |
| 30 | Clip-mask parity |Best compensated C original/promoted/true-f32 mismatch0; other experiments recorded, no final-code waiver |
| 31 | Builtin |Fresh final-source synthetic PASS; real legal-source parity unqualified |
| 32 | FreeType |Fresh fractional-coverage synthetic PASS; real legal-source parity unqualified |
| 33 | Single slot |Actual numerical/domain/synthetic paths run; full legal-video parity open |
| 34 | Dual slots |Fresh synthetic200 alternations per4 configurations PASS, real full-video equality open |
| 35 | HEVC3000 |NOT RUN, numerical gate blocks closure |
| 36 | AV13000 |NOT RUN, same |
| 37 | FD lifecycle |Real C3 before/peak/after equality unqualified; no B3 inheritance |
| 38 | GPU resource lifecycle |Synthetic payload stability/pending guard/fault reuse PASS; real stress allocation growth unqualified |
| 39 | Validation |Zero errors in all executed GPU cases; full real stress still open |
| 40 | Fault injection |Eight host checkpoints hard-fail then reuse PASS; not physical device-loss/hung-fence proof |
| 41 | CPU full C3 performance |NOT RUN |
| 42 | Vulkan one-slot performance |NOT RUN |
| 43 | Vulkan two-slot performance |NOT RUN |
| 44 | Per-pass timings |NOT QUALIFIED; no performance measured before correctness closure |
| 45 | C-1 digest |Fresh triple d0eb86…1e6b unchanged |
| 46 | C-2B digest |Fresh triple a7e7ff…71af unchanged |
| 47 | SDR regressions |Fresh5×3 output hashes/full decode/metadata/PTS PASS; historical H.264 raw1A still FAIL,1B/1C/2PASS |
| 48 | PQ preserve regressions |Fresh2×3 retained verifier1A/1B/1C/2/3PASS; old source unchanged |
| 49 | Production rejection |Both PQ codecs still reject default8 before output staging; HDR→SDR production disabled |
| 50 | SPIR-V count/validation |Actual524 cache files, Vulkan1.3 allPASS, no C3Float16/64 |
| 51 | Static checks |Default210/79ignored; measurement210/79; combined214/109;0failed; both clippy/release/fmt/diff PASS |
| 52 | Final Stage5.3C-3 |**NOT SEALED**. Numerical gate plus full real parity/stress/FD/performance remain; C-4 not started |
