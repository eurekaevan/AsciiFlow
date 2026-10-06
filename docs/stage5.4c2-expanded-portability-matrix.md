# Stage 5.4C-2 — Expanded portability matrix

**SEALED after Stage 5.4C-2A closure.** See the
[forensic closure report](stage5.4c2a-h264-ihd2546.md) and
[current closure receipt](../tests/portability/stage54c2a-h264-ihd2546.json).
Five exact stacks are now qualified: the isolated iHD edge uses the separately
attested Tier 1B-P contract for its three identifier-only H.264 differences.
Legacy Tier 1A/1B failures remain FAIL, and the historical Lavc-only allowlist
and same-stack byte-exact gates are unchanged. Final-source five-stack core ×3,
17 retained paths ×3, lifecycle and static checks pass. Overall Stage 5.4C is
SEALED. Stage 5.4D is justified but NOT STARTED.

## Historical pre-C2A report — preserved decision and evidence

The remainder records the original C-2 decision on its original source.
Its receipt, source hashes, four-qualified/one-failed registry snapshot,
classification failures and execution history are preserved, not reinterpreted
as current PASS evidence. The current registry retains this snapshot separately.

**NOT SEALED.** The expanded matrix and its final-source evidence are complete,
but the isolated iHD 25.4.6 edge fails the existing strict H.264 Tier 1B gate.
Three failures are retained as `Regression`, not waived as volatile metadata:
`canonical-h2648`, `canonical-h2648-auto`, and `hevc-pq-to-sdr10`.
The last fixture name describes its 10-bit input; its output is H.264 SDR8.
Decoded identity and structure pass in these comparisons. That does not turn
their coded-semantic FAIL into PASS. No encoder-identifier policy was broadened.

Stage 5.4C-1A and C-1 remain SEALED. Stage 5.4C as a whole is not sealed by
this pass, and Stage 5.4D is neither started nor declared justified.

## Durable evidence and execution

The [machine-readable receipt](../tests/portability/stage54c2.json) contains
the source inventory, per-stack artifacts/commands/metadata/timing, semantic
oracle logs and hashes, normalized capability ledger, descriptors, resource
samples, mux receipts, retained hashes, clean-build inventories, all SPIR-V
results, and both original and uncontended performance measurements.
The [registry](../tests/portability/qualified-stacks.json) distinguishes four
`QualifiedStack` records from the one `FailedQualification`. Per-stack
qualification is not an overall stage seal or automatic canonical promotion.

The [matrix specification](../tests/portability/c2-matrix.json),
[isolated preparation recipe](../tests/portability/prepare-c2-toolchains.sh),
[provenance](../tests/portability/c2-provenance.json), and
[unified runner](../tests/portability/run-expanded-matrix.py) are checked in.
Actual initialized loader receipts and complete native dependency identities
are retained in five schema-v2 files under `tests/portability/stacks/c2-*.json`.
Their copies are byte-identical to the captured manifests.

```sh
python3 -B tests/portability/run-expanded-matrix.py \
  --output target/stage54c2-evidence/closed-matrix
```

This full invocation completed with exit 1 because of the iHD cross-stack
comparison. All other execution gates passed. A runner subset can exit 0 for
successful execution while remaining `IncompleteQualification`/`NOT SEALED`;
never use that exit code alone as a qualification decision. The registry was
written only after reviewing the full run, clean builds, SPIR-V and performance.

The raw local run is `target/stage54c2-evidence/closed-matrix`; raw log/media
references in the durable receipt include SHA-256. The earlier directory named
`final-matrix` is stopped, old-source exploration, not formal qualification.
No old receipts, historical A/B captures, baselines, or adverse evidence were
deleted, rewritten, or relabeled PASS. No commits or host package changes.

## One frozen source, five isolated stacks

HEAD: `57e9d1d83fec07c8467d7a85376d90c84c41d0fc`

Dirty diff SHA-256: `ab8bd777f36d131e3b31031c16c45c50bacb61734e3bd0f8964379e8fb5759e0`

Cargo.lock SHA-256: `3bda07030e828d5455e2d56fbe971253034abe7f8383594a7dd8f19eadf504e5`

Each capture and execution used the same source inventory, immutable fixtures
and frozen normal production CLI. Source guards passed throughout. Final
reports, receipts, registry and stack manifests are explicitly identity-excluded
to avoid self-reference; code, shaders, generators, test recipes, lockfile and
README/testing/architecture policy are included. Those policy documents remain
accurately unsealed. No source fix occurred during the formal run.

| Key | Full stack ID | Primary changed component | Final per-stack status |
|---|---|---|---|
| C | fedora44-ffmpeg8.1.3-anv26.2.3-ihd26.1.5 | Canonical | QualifiedStack; unchanged exact-artifact reference |
| F12 | fedora44-ffmpeg8.1.2-anv26.2.3-ihd26.1.5 | FFmpeg/libav | QualifiedStack |
| F11 | fedora44-ffmpeg8.1.1-anv26.2.3-ihd26.1.5 | FFmpeg/libav | QualifiedStack |
| M | fedora44-ffmpeg8.1.3-anv26.0.3-ihd26.1.5 | Mesa/ANV | QualifiedStack |
| I | fedora44-ffmpeg8.1.3-anv26.2.3-ihd25.4.6 | iHD | FailedQualification; H.264 cross-stack Tier 1B |

Every edge is compared to C, not to another alternate. The unchanged components
and actual graphics dependency closure are attested. No unrelated common
dependency replacement was observed on either graphics edge. Isolated prefixes,
absolute alternate ICD and child-local selectors do not alter host loader state.
No lavapipe result is used as Intel interop evidence.

Actual device: Intel Arc Meteor Lake `8086:7d55`, PCI `00:02.0`, i915,
`/dev/dri/renderD128`. Active kernel: `7.2.8-200.fc44.x86_64`.
Installed alternatives 7.2.6/7.2.7 require reboot: kernel edge is
`UnavailableSafely`, not PASS. No reboot was performed.
Rust `1.97.1 (8bab26f4f 2026-07-14)`, Cargo `1.97.1 (c980f4866 2026-06-30)`;
Rust is recorded, not a changed matrix dimension.
Shared host libraries include libva `2.23.0-3.fc44`, libdrm `2.4.134-1.fc44`
and intel-gmmlib `22.10.2-1.fc44`; actual loaded paths/SHA, not package names
alone, establish identity.

| FFmpeg | libavcodec | libavformat | libavutil | Provenance |
|---|---|---|---|---|
| 8.1.3 | 62.28.103 | 62.12.103 | 60.26.103 | Fedora ffmpeg/ffmpeg-libs 8.1.3-1.fc44; source ffmpeg-8.1.3-1.fc44.src.rpm |
| 8.1.2 | 62.28.102 | 62.12.102 | 60.26.102 | Existing isolated fixed recipe; official archive SHA 464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c |
| 8.1.1 | 62.28.101 | 62.12.101 | 60.26.101 | New isolated build with the same recipe flags; official archive SHA b6863adde98898f42602017462871b5f6333e65aec803fdd7a6308639c52edf3 |

Both release signatures were independently checked against fingerprint
`FCF986EA15E6E293A5644F10B4322F04D67658D8`; GPG reported Good signature and
VALIDSIG. Ownertrust remains undefined; no global trust/import claim is made.
Full configure strings and executable/runtime-library hashes are in manifests.
Sources are the [official FFmpeg releases](https://ffmpeg.org/releases/).
Alternate Mesa `26.0.3-4.fc44` and iHD `25.4.6-1.fc44` RPM signatures/digests
passed `rpmkeys`; extraction was into new project prefixes only. Their archive
hashes and source URLs are in the provenance receipt, as are final read-only RPM
NEVRA/source-RPM confirmations. Installed canonical Mesa is `26.2.3-1.fc44`,
iHD `26.1.5-1.fc44`.

## Capabilities, plans, descriptors and resources

VAAPI profile/entrypoint maps are identical across all five stacks (25 profile
keys). Normalized Vulkan `capabilities.device` differences are zero for F11,
F12 and I. M has 87 changed fields: 12 extension, 16 feature, 51 property and
8 format fields. Arrays/UUIDs are atomic fields, not one capability per byte.
Seven property fields are API/driver version or UUID identity; the ledger
retains them separately by exact field path. Missing extensions/features include
`VK_EXT_descriptor_heap`, `VK_KHR_copy_memory_indirect`, `descriptorHeap` and
`indirectMemoryCopy`; three descriptor-buffer limits change from 32 to 8.
Absence is recorded as absence, not fabricated `false` support. These changes
do not alter the initialized production paths exercised here.

The 16 supported cases contain 11 full-GPU executions and five explicitly
CPU/software timing/audio controls. Full GPU means VAAPI decode → actual
NV12/P010 input interop → Vulkan → actual NV12/P010 output interop → VAAPI
encode, with no upload/download fallback and three slots. Actual initialized
plans are retained; planning-only probes are not runtime proof. Auto H.2648 and
HEVC10 select the same paths on every stack. Explicit requests do not silently
fall back. All eight typed negatives, plus separate H.26410 policy rejection,
retain the expected failure phase/category and destination safety.

| Actual surface, all five stacks | Dimensions | Pitch per plane | Offsets | Object bytes | Modifier |
|---|---|---|---|---|---|
| NV12 input and encoder output | 1920×1080 | 1920 / 1920 | 0 / 2088960 | 3194880 | 72057594037927945 |
| P010LE input and encoder output | 1920×1080 | 3840 / 3840 | 0 / 4177920 | 6389760 | 72057594037927945 |

Each descriptor has one object, two layers and two planes. These are observed
layouts, not universal constants. There is no tested-stack modifier/pitch/offset
drift and no descriptor assumption failure. Every stack's descriptor capture
records FD `4 → 17 → 4`, Validation errors 0.

C, M and I each completed both 3000-frame full-production stress paths:
HEVC PQ→H.264/NV12 and AV1 PQ→HEVC10/P010. All six report 3000 frames,
FD `4 → 26 → 4`, no pipeline error and teardown Validation errors 0.
Pure FFmpeg edges share identical graphics bytes, so they reuse this graphics
stress coverage while still obtaining their own actual descriptors. Native
fault/rollback/cancellation gates pass. RSS samples are retained; FD/RSS evidence
does not establish a universal GPU-allocation bound or substitute for future
long soak testing. Cargo descendant driver-map samples remain unobserved where
not sampled; their native descriptor/resource evidence is explicit instead.
The completed logs contain no `VUID-` or `Validation Error` match.

## Media, mux and difference decisions

All five stacks execute core 24 three times: 16 supported full decode-back
cases and eight typed rejects. All 360 fixture classifications pass.
C/F12/F11/M pass every core comparison. I's first cross-stack comparison has
21 ExpectedExact and three Regression entries; its repeat comparisons have
24 ExpectedExact each. All 16 supported output hashes are stable over three
runs on each stack, including single and dual AAC cases.

F11/F12 each have 16 SemanticEquivalent supported artifacts and eight exact
rejects against C. Their HEVC/AV1 packet Tier 1A passes; container
`format.tags.encoder` changes `Lavf62.12.103` to `.101`/`.102`. Three H.264
paths have raw first-packet differences: the existing narrowly pinned encoder
identifier permits only the Lavc version token, so Tier 1A remains FAIL while
Tier 1B/1C/2 pass. No new allowlist or masking was introduced.

I's actual first-packet identifier contains:

```text
Lavc62.28.103 / VAAPI 1.23.0 / Intel iHD driver for Intel(R) Gen Graphics - 25.4.6 ()
```

C contains the same prefix with `26.1.5 ()`. The existing pinned iHD suffix
rejects I; the retained native log explicitly reports “encoder identifier does
not match the pinned VAAPI pattern.” This is a coded-gate/qualification failure,
not observed decoded-pixel, color, audio or timing corruption. It is not
approved volatile drift. Classifying a known failure does not waive it: this
pass conservatively remains unsealed until that strict portability gate is
closed with legitimate evidence. A failed stack is retained, not promoted.

Every FFmpeg runtime completed 100 arrival-perturbed native fixed-input mux
replays, the 30084-packet stress and cancellation scenarios. Every same-stack
replay passed strict Tier 1A/1B/1C/2 and Tier 3 MATCH. Cross-stack fixed native
packet replay passed the unchanged oracle. The fixed packet-order hash is
`489a82f364ad461f90232f907c831e49609eb6cf6dd46194fdaaf4595bfb262c`
on all three FFmpeg stacks. This native capture test is not a replacement for
the separate full production pixel/audio corpus. Historical C-1A pre-fix A/B
and H.264 raw FAIL remain historical adverse evidence.

The production diff only extracts the existing content-based flush threshold
into a named helper and adds boundary controls: 64 packets or 8 MiB, never
queue-empty scheduling. Existing bounded producer queues are unchanged.
Same-stack byte-proof reuse requires complete runtime identity, identical new
files, and an intact previously executed passing semantic oracle log/hash.
Receipts identify reused proof, not a new native invocation. Cross-stack
comparisons always execute the unchanged Rust oracle.

The field-level ledger contains 61 ExpectedExact, 38 ApprovedVolatileDifference
(32 container encoder tags and six existing Lavc-token allowances), 32
SemanticEquivalent artifact entries, 87 CapabilityDrift, zero
CapabilityDrivenPlanChange, 15 PerformanceDrift, three Regression and zero
Unresolved. Counts are ledger entries, not distinct fixture/test totals.
No Unclassified entries and no silent planner changes. Zero Unresolved does
not erase the three known Regression blockers.

## Performance and final validation

Initial canonical measurements overlapped two clean-build jobs. Their medians
were 246.41 SDR fps, 72.07 PQ-preserve fps and 83.57 PQ→SDR fps. All original
samples remain in the receipt; they are confounded characterization, not a
claim that earlier FFmpeg releases are four times faster. After every build and
qualification test had ended, 45 serialized conversions repeated the same
commands: five stacks × three inputs × three runs, validation off, three slots,
1920×1080/300 frames and the same initialized full-GPU plans.

| Stack | SDR H.2648 fps | HEVC PQ preserve fps | HEVC PQ→SDR8 fps | Maximum elapsed ratio to C |
|---|---:|---:|---:|---:|
| C | 364.66 | 292.09 | 128.20 | 1.000 |
| F12 | 388.25 | 296.28 | 127.06 | 1.009 |
| F11 | 387.96 | 296.08 | 128.71 | 0.996 |
| M | 337.97 | 238.79 | 116.15 | 1.223 |
| I | 361.67 | 286.50 | 119.50 | 1.073 |

These are end-to-end medians including probe/init, not GPU-only measurements,
universal throughput promises or arbitrary percentage gates. No uncontended
alternate has >2× elapsed slowdown. The early concurrency anomaly is investigated
and resolved as sampler interference; raw data was not overwritten.

Final canonical retained pack: 17 paths × three runs = 51 outputs, all retained
identities pass. C1 CPU, C2B CPU, C3 Vulkan and C4A reference checks pass.
Capability FD, constructor rollback, output lifecycle faults, FreeType/PQ,
SDR encode/mux faults, hardware audio parity/Validation/cancel and longer audio
stress all pass. Workspace build/tests, measurement tests/clippy, default
clippy, mux-qualification tests/clippy, formatting, 65 corpus controls, support
documentation contract and diff checks all pass on the frozen source.

Two fresh final-source build directories (`stage54c2-clean-build-3` and `-4`)
each pass offline release workspace build and workspace tests with Rust 1.97.1;
their 50-module release/debug SPIR-V inventories/hashes match exactly. Earlier
clean builds are preserved but not substituted for this source identity.
After all final builds, **all 1203 actual workspace SPIR-V modules**, 34 unique
hashes, pass `spirv-val --target-env vulkan1.3`, zero failures. The validator
reports SPIRV-Tools v2026.1, dated 2026-02-16; executable identity/version and
every module path/size/SHA/result are in the receipt. The count is measured,
not the former 133-module constant. Unique shader hashes remain unchanged.

## Required 77-item closure index

Each row points to observed evidence above and the detailed receipt; no pending
test is represented as a pass.

| # | Requested item | Result / boundary |
|---:|---|---|
| 1 | Matrix design | Canonical-centered five-stack star; one declared primary component per edge. |
| 2 | Canonical stack | C remains the exact-artifact reference; no promotion. |
| 3 | Alternates | F12, F11, M and I; exact IDs in the stack table. |
| 4 | Source proof | Same HEAD/diff/lockfile/inventory, frozen binary and fixtures; guards pass. |
| 5 | Runtime dependencies | V2 actual libav identities, initialized ANV/iHD loader modules and dependency closure. |
| 6 | FFmpeg matrix | 8.1.3, 8.1.2, 8.1.1 actually executed and characterized. |
| 7 | Mesa/ANV matrix | 26.2.3 and isolated 26.0.3; both qualify. |
| 8 | libva/iHD matrix | Fixed libva2.23.0; iHD26.1.5 qualifies,25.4.6 fails strict H.264 P2. |
| 9 | Kernel matrix | 7.2.8 active; alternate UnavailableSafely, no reboot. |
| 10 | Rust/toolchain | Same rustc/cargo1.97.1, exact -Vv records and two clean builds. |
| 11 | Capability snapshots | All five actual initialized devices captured. |
| 12 | Capability diffs | Exact normalized field ledger; selected driver identities distinguished. |
| 13 | VAAPI profiles | 25 keys; zero profile/entrypoint differences. |
| 14 | Vulkan features | M:87 normalized fields, including16 feature changes; others zero. |
| 15 | NV12 input | Actual four-way descriptor capture, every stack. |
| 16 | P010 input | Actual descriptor capture, every stack. |
| 17 | NV12 output | Actual encoder surface descriptor, every stack. |
| 18 | P010 output | Actual encoder surface descriptor, every stack. |
| 19 | Modifier drift | None observed in these five stacks; exact modifier retained. |
| 20 | Hardcoded layout | No tested descriptor failure; no claim all layouts have these offsets. |
| 21 | Planner paths | Eleven full-GPU supported cases plus five explicit CPU/software controls per stack. |
| 22 | Auto planner drift | H.2648/HEVC10 paths unchanged, no unexpected fallback. |
| 23 | Explicit planner | Exact requests honored or typed rejects; initialized plans retained. |
| 24 | H264 SDR | Production/repeats pass; I cross-stack coded gate fails. |
| 25 | HEVC SDR8 | Three runs/decode/P2 pass on all stacks. |
| 26 | AV1 SDR8 | Three runs/decode/P2 pass on all stacks. |
| 27 | HEVC SDR10 | Three runs/decode/P2 pass on all stacks. |
| 28 | AV1 SDR10 | Three runs/decode/P2 pass on all stacks. |
| 29 | HEVC PQ preserve | Three runs, metadata/decode/P2 pass on all stacks. |
| 30 | AV1 PQ preserve | Three runs, metadata/decode/P2 pass on all stacks. |
| 31 | HEVC PQ→SDR8 | Decode/color/structure pass; I H.264 coded gate fails. |
| 32 | AV1 PQ→SDR10 | Three runs/decode/BT.709 SDR/P2 pass on all stacks. |
| 33 | Single AAC | Three independent conversions, stable hashes, payload/structure/timing oracle pass. |
| 34 | Dual AAC | Three runs, both payloads/routing/title and timeline gates pass. |
| 35 | B-frame | CPU/software control fully decoded; current timeline/media regression pass. |
| 36 | 24000/1001 | Exact rational timeline control and oracle pass. |
| 37 | Nonzero PTS | Origin-preservation control and exact timing semantics pass. |
| 38 | Negatives | Eight existing typed rejects ×3×5, plus H.26410 destination-safety gate per stack. |
| 39 | 5.4B regressions | Changed SPS/resolution, rotation, HDR conflict, truncation, audio/timing controls pass. |
| 40 | Deterministic mux | Permanent content-based flush controls; no queue-empty flush regression. |
| 41 | Fixed mux replay | 100 per FFmpeg stack,30084-packet stress,cancellation; same/cross-stack gates pass. |
| 42 | H264 Tier1A | F11/F12 raw FAIL retained; I raw FAIL retained; same-stack repeats PASS. |
| 43 | H264 Tier1B | C/F11/F12/M PASS; I three cross-stack FAILs, not waived. |
| 44 | Tier1C | All exercised cross-stack decoded identity PASS, including I failures. |
| 45 | Tier2 | All exercised cross-stack structure/timing/color/audio PASS. |
| 46 | Tier3 | Same-stack outputs stable; cross-stack DIFFERENT diagnostic, not universal byte baseline. |
| 47 | HEVC/AV1 packets | Cross-stack raw packet identity PASS in tested cases. |
| 48 | Container metadata | F11/F12 encoder tag changes only under existing narrow policy; exact field ledger. |
| 49 | Color semantics | SDR/PQ/PQ→SDR full decode and strict metadata gates stable. |
| 50 | HDR metadata | PQ-preserve policy unchanged; PQ→SDR removes/relabels correctly; no HLG opening. |
| 51 | Audio payload | Existing exact compressed-packet/routing oracle PASS; no transcode introduced. |
| 52 | Audio metadata | Stream routing/title/language and AAC descriptors retained, strict structure gates PASS. |
| 53 | Timing | CFR/B-frame/rational rate/nonzero PTS controls and mux timestamp gates PASS. |
| 54 | Descriptor portability | All five observed layouts match, initialization agrees with capability probes. |
| 55 | Validation | Descriptor/stress counters0; no VUID/error match; SDR/PQ/PQ→SDR hardware exercised. |
| 56 | FD lifecycle | Descriptor4→17→4; full-GPU stress4→26→4; fault/cancel/rollback controls PASS. |
| 57 | Resource lifecycle | All six3000-frame runs clean; actual samples, not universal allocation/leak guarantees. |
| 58 | Performance | Original samples preserved; uncontended45-run table, validationoff/3slots; maximum ratio1.223. |
| 59 | CapabilityDrift |87 field entries with actual values/proofs; no resulting planner drift. |
| 60 | PerformanceDrift |12 alternate characterizations +3 initial canonical interference entries; investigated. |
| 61 | SemanticEquivalent |32 supported artifact entries across F11/F12; strict P2 evidence, not byte identity. |
| 62 | ApprovedVolatileDifference |38 narrow existing-policy entries; no iHD suffix waiver. |
| 63 | Regressions found/fixed | Three I coded-gate regressions found and retained; no semantic production fix or policy weakening. |
| 64 | Unresolved count |0; three classified Regression blockers still remain. |
| 65 | Registry | Four QualifiedStack, one FailedQualification, all source-scoped. |
| 66 | Canonical regressions |17×3 retained, CPU/Vulkan references, lifecycle and static all PASS. |
| 67 | Historical H264 oracle | Original raw FAIL preserved; no retrospective PASS claim. |
| 68 | Historical C-1A mux | Immutable A/B/pre-fix evidence preserved, no deletion. |
| 69 | SPIR-V |1203 actual modules,34 unique SHA, Vulkan1.3 all PASS; clean inventories match. |
| 70 | Static |Default/measurement/mux tests+clippy, build,fmt,65controls,support docs,diff PASS. |
| 71 | Tested boundaries |Only listed exact tools/drivers/Intel device/source; failed I not qualified. |
| 72 | Unsafe dimensions |Alternate kernel UnavailableSafely; other GPU/Rust/libva version dimensions not tested. |
| 73 | Allowed claims |Exact bytes stack-scoped; four qualified stacks have preserved tested P2 support. |
| 74 | Disallowed claims |No universal versions/layouts/speed/leak proof, no canonical migration or recovered historical baseline. |
| 75 | Stage5.4C seal |NOT SEALED overall; C1/C1A remain individually SEALED. |
| 76 | Stage5.4D |Not justified by a C2 seal and not started. |
| 77 | Stage5.4C-2 final |NOT SEALED; I H.264 cross-stack Tier1B is the remaining gate. |

## Continuation boundary

Resume from this known failed qualification, not from a guessed new baseline.
Do not silently generalize the pinned SEI suffix, downgrade the oracle, promote
I, or overwrite canonical artifacts. Closing the iHD edge needs evidence under
the unchanged strict contract, or an explicitly authorized, separately reviewed
contract change followed by fresh source-bound qualification. That contract
change is not made here. Stage 5.4D remains out of scope.
