# Stage 5.4C-1 — Toolchain / Driver Portability Baseline

Status: **SEALED** through [Stage 5.4C-1A deterministic mux closure](stage5.4c1a-deterministic-mux.md).
Stage 5.4C-2 has not started.

The final post-fix same-source matrix passes 24 core cases on each actual
FFmpeg8.1.3/isolated8.1.2 stack, with16 SemanticEquivalent outputs and eight
ExpectedExact rejections. The unchanged strict same-stack oracle now passes
100 mux-only repetitions,26 software/full-GPU production pairs and10 traced
original-path repetitions (Tier1A/B/C/2 PASS, Tier3 MATCH). Canonical17×3
retained outputs and final lifecycle/static/Vulkan1.3 gates pass. The
[C1A receipt](../tests/portability/stage54c1a.json) is the current authority.

## Historical pre-closure record

Everything below describes the original C1 evidence and its **NOT SEALED**
decision before C1A. Original source proof, hashes, ordered packet vectors,
strict failures and exact-build limitations remain unchanged. Original stack
manifests are retained as `historical-c1-*.json`; the current stack registry now
attests the final C1A source/binary. Historical AAC Tier2 failures are not changed
to PASS and are not mixed into the final repeatability count.

Final same-source core execution: 24 cases per stack pass. The final pairwise
matrix has 16 SemanticEquivalent outputs and eight ExpectedExact rejections;
all initialized planner/capability comparisons are equal. However, the separate
canonical same-stack AAC repeatability oracle fails Tier 2. The intermittent
failure is not cleared by one successful pairwise comparison.

The framework, real isolated FFmpeg comparison, runtime descriptor capture and
canonical regression checks are implemented. A Supported single-AAC MP4 path
fails the unchanged Tier 2 container-order gate. This is not blessed as version
metadata drift or hidden by equal decoded pixels.

[Machine-readable receipt](../tests/portability/stage54c1.json),
[stack manifests](../tests/portability/stacks/),
[core manifest](../tests/portability/core-set.json) and
[reproduction/upgrade workflow](portability-testing.md) are the durable record.
Ignored target directories hold raw logs/artifacts; they are not permanent
qualification authority. Reproduction requires the retained generators/tool
versions and input identities, not just a cached filename.

## Scope and source identity

Canonical: `fedora44-ffmpeg8.1.3-anv26.2.3-ihd26.1.5`.
Alternate: `fedora44-ffmpeg8.1.2-source-anv26.2.3-ihd26.1.5`.

Both use Intel Arc Meteor Lake PCI8086:7d55, /dev/dri/renderD128,
Mesa/ANV26.2.3, iHD26.1.5, libva2.23.0, kernel7.2.8-200.fc44.x86_64,
Rust/cargo1.97.1. Only FFmpeg/libav and its build configuration vary.
The isolated build recipe enables the same host libdav1d1.5.4 for software
AV1 decode. The earlier minimal build lacking libdav1d is failed exploratory
evidence, not a qualified alternate. No host package replacement, system
ldconfig override, alternate kernel or driver installation occurred.

Verified same-source core captures:

- HEAD: `e186f754701e1ca45f09cfc8a45de3ad7dc29330`
- Dirty diff SHA-256: `ddfca515bb03581167ef9290beb20fb41f6f22d7bd9abebcbd012854041c6f14`
- Cargo.lock SHA-256: `3bda07030e828d5455e2d56fbe971253034abe7f8383594a7dd8f19eadf504e5`
- CLI SHA-256: `83d08bde821ec0ac799d23e05277ee86d482d80a85b1d8ad289fc0a482a2b3e8`

The stack registry and final report/receipt are excluded from source identity
to avoid recursive hashes. All other source, test recipes, fixture inventory
and documentation are included. Historical failed attempts are preserved
separately; they cannot be mixed into the verified same-source matrix.
The retained pack started before Python report-reader corrections. Its native
CLI, Rust production source, generators, retained comparator and inputs are
unchanged and byte-identified; it is component-bound regression evidence,
not a claim that its earlier full-tree snapshot equals the final core snapshot.
There was no production portability fix in this pass.

## Historical blocker: AAC container interleave (resolved by C1A)

The original same-source pair `real-aac-44100-mono-mp4` has 50 H.264
packets and 88 AAC packets on each stack. Tier 1A/1B/1C pass but Tier 2 fails:
global demux stream order differs at packet indices 56 and 68.
A single common /usr/bin/ffprobe confirms the difference is in artifacts,
not probe-version interpretation. Per-stream packet payload, PTS/DTS/duration
and flags match when physical byte position is excluded.

Additional exact-command canonical repeats give two interleave sequences and
two whole-file SHA values across three runs; alternate repeats happen to match
three times. This does **not** establish alternate determinism or patch-only
causality. The canonical repeat hashes are:

```text
c33ac221eeb792790c2113bb9674586a05711672baa04177676750ec97b7adfa
c33ac221eeb792790c2113bb9674586a05711672baa04177676750ec97b7adfa
32ea1a2fcb11e2ac328ed726bfc10536b0032ca18078b21fa61da962dcfeafda
```

All three alternate repeat hashes are
`fb587edb0f42f46857211cab4c4232881ae59b98bb460c1c42c276fdc337a54e`.
Exact commands and ordered stream vectors are in the receipt.

Observed backward cross-stream DTS jumps at global indices 64 and 128 align
with the production mux worker's forced interleaver flush every 64 received
packets (or 8 MiB). Producer arrival/flush-dependent ordering is a
source-supported **inference**, not a traced root-cause proof.
Removing the flush blindly would weaken the documented sparse/noninterleaved
input memory bound. A safe fix needs deterministic ordering/watermark or
equivalent bounded coordination, plus sparse-stream, cancellation, ownership,
same-stack repeatability and both-stack regression tests. The existing
container-order test and narrow metadata allowlist remain unchanged.
No global driver denylist, automatic baseline re-recording or oracle relaxation.

## Qualification matrix interpretation

Each stack's runtime compatibility results are separate from pairwise P2
qualification. A successful conversion/full decode does not turn an AAC
Tier2 structural failure into PASS. Stack-scoped exact artifacts are retained
as observations; the alternate is not promoted as fully qualified.
The final per-fixture comparison and all gate outcomes are in the receipt.

No new codec, HDR mode, media feature, production knob or shader was added.
Existing software/VAAPI encoder settings were inspected: project GOP, rate
control/quality and codec/depth requests stay unchanged, with actual output
profile/level/pixel format/color/time base captured per stack. Implementation
defaults are characterized, not gratuitously over-pinned.

## Required final report

| # | Item | Observation / decision |
|---|---|---|
| 1 | Portability model | Stack-scoped observations, not universal version support. |
| 2 | P0/P1/P2/P3 | Environment/capabilities; initialized eligibility; media semantics; exact artifact identity. See portability-testing.md. |
| 3 | Canonical stack | fedora44-ffmpeg8.1.3-anv26.2.3-ihd26.1.5; Intel Arc MTL 8086:7d55, renderD128. |
| 4 | Alternate stack | fedora44-ffmpeg8.1.2-source-anv26.2.3-ihd26.1.5; isolated prefix, real same Intel GPU. |
| 5 | Source proof | Both verified captures have identical HEAD, dirty diff, file inventory and Cargo.lock; identities above. |
| 6 | FFmpeg | 8.1.3 RPM versus signed official 8.1.2 source build; build configuration also differs, so patch-only causality is not established. |
| 7 | libavcodec | 62.28.103 → 62.28.102, resolved library paths and SHA recorded. |
| 8 | libavformat | 62.12.103 → 62.12.102; libavutil 60.26.103 → .102, swscale 9.5.103 → .102, swresample 6.3.103 → .102. |
| 9 | Mesa | 26.2.3 unchanged; no alternate-Mesa claim. |
| 10 | ANV | Same driver binary/hash; API/features/extensions captured structurally. |
| 11 | libva | 2.23.0 unchanged. |
| 12 | iHD | 26.1.5 unchanged. |
| 13 | Kernel | 7.2.8-200.fc44.x86_64 unchanged; no alternate-kernel experiment. |
| 14 | Rust/toolchain | rustc/cargo 1.97.1 unchanged; Cargo.lock pinned. |
| 15 | Canonical capability snapshot | Per-input structured CLI diagnostics plus Vulkan profile and VAAPI profile/entrypoint data retained. |
| 16 | Alternate capability snapshot | Same actual probes under the prefix loader; no lavapipe substitution. |
| 17 | Capability diff | Added/removed/changed structural comparator; actual core comparisons and receipt remain authoritative. |
| 18 | VAAPI profile diff | Same profile/entrypoint map; actual codec initialization and runtime checked independently. |
| 19 | Vulkan feature diff | Same structured profile; ANV and shader compiler unchanged. |
| 20 | NV12 input descriptor | Same actual one-object/two-plane layout: pitch1920, offsets0/2088960, object3194880 bytes. |
| 21 | NV12 output descriptor | Same encoder-owned layout; actual ANV write import succeeds. |
| 22 | P010 input descriptor | Same actual one-object/two-plane layout: pitch3840, offsets0/4177920, object6389760 bytes. |
| 23 | P010 output descriptor | Same encoder-owned layout; actual ANV write import succeeds. |
| 24 | Modifiers | 72057594037927945 unchanged on all four surfaces. Different-modifier hardware experiment not performed; runtime-layout control is structural only. |
| 25 | Planner | Compare initialized plans, not reason prose; exact per-fixture choices and differences in receipt. |
| 26 | SDR core | H.2648, HEVC8, AV18 actual production conversion/full decode-back executed on both stacks. |
| 27 | SDR10 | HEVC Main10 and AV1 Main10 executed on both stacks; full P010 GPU interop. |
| 28 | HEVC PQ preserve | Actual full GPU conversion/decode-back; BT.2020 NCL/PQ/limited remains correct. |
| 29 | AV1 PQ preserve | Actual full GPU conversion/decode-back; same PQ metadata policy. |
| 30 | HDR→SDR NV12 | HEVC PQ→H2648, BT.709 limited; HDR metadata stripped. |
| 31 | HDR→SDR P010 | AV1 PQ→HEVC10, BT.709 limited; HDR metadata stripped. |
| 32 | Single AAC | BLOCKER: observed Tier2 cross-stream interleave drift and canonical same-stack nonrepeatability; payload/PTS/DTS/duration remain exact. |
| 33 | Dual AAC | Strict source→output payload/routing/title/language/default oracle executes on both stacks; pair classification in receipt. |
| 34 | B-frame timing | Full decode and normalized presentation timeline checked; no packet arrival order substituted for PTS. |
| 35 | 24000/1001 | Exact rational CFR timeline checked on both stacks. |
| 36 | Nonzero PTS | Positive two-second input-origin case checked on both stacks. |
| 37 | Negative behavior | Eight typed runtime rejections include HLG, full-range PQ, conflict, dynamic SPS/resolution, HDR SEI, truncated AAC tail and rotation. |
| 38 | H264 Tier1A | Lavc62.28.103/.102 raw SEI drift remains visible as FAIL where observed; no unconditional packet-ignore rule. |
| 39 | H264 Tier1B | Existing pinned UUID/fixed suffix/patch-token rule reused, unchanged SPS/PPS/VCL/unknown-SEI controls. |
| 40 | Tier1C | Existing decoded-visible-frame oracle, not visual inspection; results by fixture in receipt. |
| 41 | Tier2 | Strict container structure including cross-stream demux order; single AAC observed FAIL is not waived. |
| 42 | Whole-file SHA | Different across FFmpeg stacks; recorded with stack key. Never auto-promoted into canonical baselines. |
| 43 | Container metadata drift | Only existing Lavf/Lavc patch-token approval; interleave is not metadata and not approved. |
| 44 | Color metadata | Primaries/transfer/matrix/range explicitly checked per output; hard gate. |
| 45 | HDR metadata policy | Defined PQ-preserve behavior retained; PQ→SDR metadata absent, not silently inherited. |
| 46 | Audio payload | Single/dual source→output ordered compressed payload checked strictly, separately from container order. |
| 47 | Timing semantics | Frames, packet presentation timing and audio endpoints checked; exact core records retained. |
| 48 | Validation | Enabled on actual Intel SDR/PQ/PQ→SDR runs and descriptor imports; observers/stress require zero errors through teardown. |
| 49 | FD lifecycle | Alternate HEVC and AV1 each3000frames: baseline4, peaks29/33, after4; descriptor baseline4/peak22/after4 on both. |
| 50 | Resource lifecycle | Actual surface import/drop and constructor/output-fault checks; no reuse of invalid submitted surfaces. |
| 51 | Performance | Recorded command wall times/RSS and per-path initialized diagnostics, characterization only. Concurrent runs are not isolated benchmark comparisons. |
| 52 | Auto selection | Canonical H2648 and HEVC10 auto requests exercise initialized full GPU paths on both stacks; no silent staged fallback accepted. |
| 53 | Explicit paths | VAAPI/Vulkan/VAAPI interops required and capability-consistent; portable audio/timing cases intentionally CPU/software. |
| 54 | 5.4B fixes | Dynamic format reuse, display-transform rejection, conflicting metadata and dual-audio titles included. |
| 55 | Difference classes | ExpectedExact, ApprovedVolatileDifference, SemanticEquivalent, CapabilityDrift, PerformanceDrift, Regression, Unresolved; no generic Different pass. |
| 56 | Unresolved differences | Root-cause mechanism of AAC arrival/flush-dependent interleave remains unproven by tracing; strict regression unresolved and blocks seal. |
| 57 | Canonical regressions | 17 paths ×3 outputs match retained artifacts. C1/C2B/C3/C4A checks pass; component-source boundary described below. |
| 58 | Historical H264 oracle | Historical Tier1A FAIL / Tier1B PASS / Tier1C PASS / Tier2 PASS unchanged; old exact-build attestation not transferred. |
| 59 | SPIR-V | All 1003 actual generated modules pass Vulkan 1.3 validation; no shader change. |
| 60 | Static checks | Release workspace build/tests; encode-characterization tests; strict all-target clippy both modes; fmt;60 offline corpus controls; support docs/diff checks. |
| 61 | Limitations | Only one alternate FFmpeg stack; configuration differs; no alternate driver/kernel/modifier claim, no minimum version claim, AAC structural blocker open. |
| 62 | C-2 justified? | No. Do not begin Expanded Portability Matrix. |
| 63 | Final Stage5.4C-1 | NOT SEALED. |

At this historical decision, Stage 5.4C-1 was **NOT SEALED** pending resolution
of the Supported AAC container-order regression without weakening bounded
resource ownership or the existing media oracle, followed by both-stack and
canonical retained reruns. Those conditions are now satisfied by C1A; the
historical failures and source proof above are preserved, not reclassified.
