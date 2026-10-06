# Stage 5.4C-2A — H.264 VAAPI cross-stack semantic closure

**SEALED — Stage 5.4C-2A, Stage 5.4C-2 and overall Stage 5.4C.**
The three original differences have been attributed to **outcome C: the
historical oracle has no cross-driver identifier contract**, not a product
semantic regression in iHD 25.4.6. The frozen-source five-stack matrix,
retained outputs and lifecycle/static gates pass. Five exact stacks are
qualified, with the candidate's three H.264 paths explicitly using Tier 1B-P.
Stage 5.4D is now justified; no Stage 5.4D work is started.

## Scope and immutable historical evidence

The failing fixtures are `canonical-h2648`, `canonical-h2648-auto` and
`hevc-pq-to-sdr10`. The last name describes a 10-bit PQ **input**; its output
is H.264 SDR8, not HEVC Main10. Reference and candidate respectively are:

```text
fedora44-ffmpeg8.1.3-anv26.2.3-ihd26.1.5
fedora44-ffmpeg8.1.3-anv26.2.3-ihd25.4.6
```

Original C-2 MP4s, the old [receipt](../tests/portability/stage54c2.json), old
strict results and the Lavc patch-token SEI allowlist are preserved. The six
MP4s were copied and SHA-verified before investigation in
`target/stage54c2a-evidence/original-inspection`. Their extracted ES, full
packet/frame probes, `trace_headers`, software decode-back, NAL inventories,
container atoms and command/tool receipts remain there. Final measurement
outputs are byte-identical to these originals, independently checked for each
of the six case/driver pairs. Thus the original syntax and decode inventories
apply by exact artifact identity, not by assuming equivalent pixels.

The historical Tier 1A and Tier 1B failures are **not changed to PASS**.
The additive contract is named **Tier 1B-P** and is never used for same-stack
regression. The machine-readable closure receipt is
[`stage54c2a-h264-ihd2546.json`](../tests/portability/stage54c2a-h264-ihd2546.json).

## One frozen source and actual runtime identities

Final HEAD: `b3b03f590f6c62a484662ad10279f94d9a1d3479`.

Tracked dirty-diff SHA-256:
`7f79a3a8977286787988500fd2bf795071c0b0a58fef9e77b8e44f2dc453f495`.

Cargo.lock SHA-256:
`675c339488902831907e094b3261d0c8de491d6d329e30322ee9198cfc952fd2`.

The complete source inventory also hashes nonignored untracked files; the
tracked diff hash alone is not their identity. Code, fixtures, generators,
recipes, README and testing policy are frozen and checked at completion.
Only final reports/receipts/registry/stack manifests are excluded to avoid
self-reference. Measurement and normal executables have distinct recorded
SHA-256 values; both are attested rather than described as the same binary.

The fixed host toolchain is FFmpeg 8.1.3 (`libavcodec 62.28.103`,
`libavformat 62.12.103`, `libavutil 60.26.103`), Mesa/ANV 26.2.3,
libva 2.23.0, libdrm 2.4.134, Rust/Cargo 1.97.1, and kernel
`7.2.8-200.fc44.x86_64`. Device: Intel Arc Meteor Lake `8086:7d55`, i915,
PCI `00:02.0`, `/dev/dri/renderD128`. The isolated iHD package is
25.4.6-1.fc44; the host is 26.1.5-1.fc44. Executable identities, complete
FFmpeg configuration/version output, kernel/Rust identity, linked libraries,
actual initialized iHD/ANV paths and hashes are retained, not inferred from
requested environment variables. Only child-process selectors change.
No host downgrade, driver installation, global environment change or reboot.

## Production repetition and encoder-boundary identity

Checked-in recipe:

```sh
python3 -B tests/portability/h264-ihd-forensic.py \
  --binary target/release/asciiflow \
  --output target/stage54c2a-evidence/capture-final-v2 --va-trace
```

That invocation used a frozen `encode-characterization` CLI. Each driver runs
each fixture ten times, **60 complete conversions**. The first run additionally
captures the actual submitted VAAPI AVFrame after final metadata assignment
and before `avcodec_send_frame`. Diagnostic NV12 readback never replaces or
modifies that surface. The other nine runs have no capture. All ten output
files are identical within each case/driver, proving capture did not change
encoded output. An inherited capture/trace environment is cleared explicitly.

Complete hardware-route CLI (substitute the recorded immutable input/output
and report paths; auto uses `auto` for the five corresponding selectors):

```sh
asciiflow INPUT OUTPUT.mp4 --audio none --output-codec h264 \
  --output-bit-depth 8 --output-dynamic-range sdr \
  --hw-device /dev/dri/renderD128 --width 80 --charset standard \
  --font builtin-8x8 --color true --max-frames 0 \
  --backend vulkan --decode vaapi --encode vaapi --vulkan-mapping gpu \
  --vaapi-vulkan-input-interop on --vaapi-vulkan-output-interop on \
  --no-progress --diagnostic-report REPORT.json
```

For the automatic case, backend/decode/encode and both interop selectors are
`auto`; Vulkan mapping stays `gpu`. Actual initialized plans are retained.
Explicit routes remain full GPU; none are silently converted to CPU/staged
encoding. Width/font/charset/color/audio/configuration are fixed.

Every route has 300 encoder-surface records: 1920×1080 VAAPI format 44,
downloaded NV12 format 23. Both Y and UV visible-plane SHA-256 values match
for every frame across drivers; the 933,120,000-byte packed streams match too.
Padding is excluded from visible-plane hashing. Raw stream identities:

| Route | Input SHA-256 | Pre-encode NV12 SHA-256, both drivers |
|---|---|---|
| canonical-h2648 | `6e5c214b813dca3e1db65629b1241cfb663166eb565cda1c48b5ee74ed0dce6b` | `d6640d181d702fe473039793252cf9fb55a10278769e703817292222afbfdcba` |
| canonical-h2648-auto | Same canonical input | Same canonical NV12 stream |
| hevc-pq-to-sdr10 | `eb8d8c42cd99369b99031900e8861c08f611bfcc662b8d0eb741eb695a0c366a` | `34b435ea29fb38cd6528a3d9040058387dab0f3428c855e4123700f7a12f601e` |

AVFrame width/height/format/PTS/duration/time base/SAR/flags/picture intent,
primaries/transfer/matrix/range/chroma location all agree. Actual submitted
PTS is 0..299; AVFrame duration is 0 and time base is 0/1, with codec time
base 1/50. SAR is 0/1 (unspecified), not an invented 1:1 value; display aspect
is not separately declared by these AVFrames. The output geometry matches.
Colors are BT.709/BT.709/BT.709, limited, left chroma; picture type NONE,
flags 0. The machine receipt retains all 300 records, not just one sample.

## Configuration and native VAAPI evidence

Both before-open and after-open scalar AVCodecContext snapshots are equal
across drivers. Full public `av_opt_serialize` output, including child/private
AVOptions, is **raw-byte identical**; no pointer normalization was needed.
No opaque FFmpeg private struct layout was inspected. Important values:
VAAPI/NV12 surface path, 1920×1080, 50/1, time base 1/50, GOP 250,
max B frames 0, bitrate/max/min/buffer 0, global-header flag 0x00400000,
profile before -99 and after 100, level -99 (selected bitstream level 42),
limited BT.709, chroma LEFT. Actual options are `rc_mode=CQP` (serialized 1),
`qp=20`, `async_depth=2`; coder 1, AUD false, SEI flags 0x0e and other options
are included in the retained full snapshots.

The 48 LIBVA_TRACE chunks across six first-run captures are hashed and
inventoried. Both drivers submit identical H264High/EncSlice encode config:
RTFormat type 0/value 1, RateControl type 5/value 0x10,
EncPackedHeaders type 0x0a/value 0x0d. Mapped sequence/framerate fields agree:
level 42, bitrate 0, intra/IDR periods 250, IP period 1, 120×68 macroblocks,
frame-rate buffer 65586 and the other recorded sequence fields.
Probe/create call counts differ in auto and HEVC-input routes; repeated
identical configuration requests are recorded as diagnostics, not erased.

Limit: these traces expose successful `vaGetConfigAttributes` returns but not
their arguments/returned attribute values. `vaQueryConfigAttributes` is absent,
and no VAEncMiscParameterRateControl buffers occur. Driver-reported defaults
and private heuristics are **unobserved**, not claimed equal. This does not
leave the observed difference unresolved: submitted configuration, SPS/PPS,
slice headers and every residual/VCL byte are equal, and the only changed
payload explicitly names the driver build. No extra implementation pinning is
justified; existing product-semantic settings are already fixed.

The initial mapped packed-header data buffer is 110 bytes, preceded by a
type-4/880-bit parameter and submitted before rendering. Its trace payload is
empty: neither the UUID nor the Lavc label is observable there. The trace cannot
identify that buffer as this SEI or prove which component assembled the label.
The attribution concerns the observed output's attested driver identity, not
an unobserved FFmpeg/driver internal assembly mechanism.

## Table A — Bitstream differences

Each production ES has 305 NALs: SPS×2, PPS×2, SEI×1, IDR×2,
non-IDR slices×298. All three cases share this signature.

| Field | iHD 26.1.5 | iHD 25.4.6 | Difference/classification |
|---|---|---|---|
| SPS/PPS bytes | Same | Same | None; exact coded and parsed-field identity |
| SPS profile/level/chroma/depth | High/42/4:2:0/8-bit | Same | None |
| SPS geometry/crop | 120×68 MB, bottom crop 4 → 1920×1080 | Same | None |
| SPS frame/POC/ref fields | frame_mbs_only=1, POC type 2, one ref | Same | None |
| VUI color/range/chroma | 1/1/1, full_range=0, location 0/0 | Same | None; BT.709 limited |
| VUI timing/HRD/reorder | tick 1, scale 100, fixed=1, HRD absent, reorder=0, buffer=1 | Same | None |
| PPS coding fields | CABAC=1, one slice group, L0/L1 default=0, weighted pred=0, QP offset=-6, transform8×8=1 | Same | None; complete field inventory retained |
| AU0 NAL2 SEI | `… Gen Graphics - 26.1.5 ()\0` | `… Gen Graphics - 25.4.6 ()\0` | Informational driver-build identifier only |
| SEI type/UUID/payload size | type 5, UUID `59948b2811ec45af967519d41feaa94d`, 102 bytes | Same | No semantic/unknown payload added |
| IDR/AU/GOP | IDR at 0 and 250; 300 AU; no B frames | Same | None |
| Slice headers/residual bytes | All 300 exact | All 300 exact | No first divergent VCL/header/residual exists |
| MP4 bytes | Three differing mdat bytes | Same sizes/positions | Offsets 191/193/195 are diagnostic only |

Complete fixed-FFmpeg SPS/PPS/VUI/SEI/slice inventories and every NAL's
type/ref_idc/length/SHA are retained. First divergence is AU/packet 0,
DTS=PTS=0, NAL index 2 (zero-based), byte 96 within that NAL. No generic VCL
equivalence inference or new slice/Exp-Golomb parser is needed: its complete
bytes already match. Unknown trace syntax and unknown SEI counts are zero.
Reserved bits and RBSP trailing bits are unchanged. Hard product-semantic
fields, coding-only fields and slice residuals all have **zero differences**.

## Table B — Product semantic impact

| Dimension | Evidence | Impact |
|---|---|---|
| Decoder-visible signal | Exact SPS/PPS/VUI and all VCL bytes | None |
| Color/range | Probe and bitstream BT.709 limited | None |
| Geometry/depth | 300×1920×1080 yuv420p, 8-bit | None |
| Timing/order | Codec 1/50, MP4 1/12800; PTS/DTS 0..299×256, duration 256 | None |
| Pixels | Fixed FFmpeg 8.1.3 software H.264, `-hwaccel none -threads 1`, 300 per-frame hashes/timing rows exact | None; independently checked, not the basis of coded equivalence |
| Container | ftyp/free/moov and layout exact; only the proven SEI packet differs | None |
| Audio | These three cases explicitly audio none; separate AAC retained/core/lifecycle gates stay strict | No exemption introduced |
| Informational identifier | Matched actual initialized driver version, same Lavc/VAAPI prefix | Raw artifact identity differs, product semantics do not |

## Encoder-only isolation

The measurement-only ignored test `retained_nv12_h264_vaapi_encoder_only`
loads one retained host-lane NV12 stream per case and uses that **same file**
for both drivers. It verifies all 300 Y/UV SHA-256 values and exact file length,
recreates recorded frame metadata/configuration, and checks before/after
context JSON and full AVOptions against production snapshots. It uses public
FFmpeg/VAAPI APIs and existing RAII; there is no decode, ASCII processing,
Vulkan processing, audio or muxer. Headers and packets are written directly to
an Annex B ES and drained to EOF; 300 input frames and 300 output packets.

All six executions pass. Each isolated ES software-decodes all 300 frames
without errors; within each driver its 300 VCL NALs exactly match production.
Across drivers only the same SEI build string differs. The direct writer
retains global codec extradata plus packet bytes, so the ES has 307 NALs
(SPS×3/PPS×3/SEI×1/VCL×300) and its first divergence is NAL4, not production
NAL2. This explicit identical-header duplication is not a new semantic or
container difference. No mux participation can explain the driver string.

## Additive Tier 1B-P contract and negative controls

`tests/h264_driver_portability.rs` is an independent test entry point. Upstream
corpus orchestration invokes it only after schema-v2 actual loader attestation
proves a single-iHD edge: identical source/binary/libav/tool/ANV/host/native
dependency closure except the selected driver. It is never invoked for
same-stack comparison and never rewrites the old oracle result.

Its contract is bounded to one 1080p/300-frame/50-fps BT.709 limited H.264
stream. All stream/extradata metadata, AU timestamps/durations/key/side-data,
NAL headers/ref_idc/count/order, SPS/PPS and complete VCL bytes remain exact.
Only one known, fully fingerprinted SEI in AU0 may differ in the **attested**
driver identity. UUID/type/size/Lavc/VAAPI/text framing/trailing bits remain
exact; malformed escapes, truncated payloads, extra messages and unknown
NAL/SEI fail conservatively. The whole MP4 must match after replacement of
the single **unique, independently proven** packet, not after globally masking
text or ignoring an mdat box. Decode-back remains an additional independent
gate, never a substitute for coded-data checks.

No new general SPS/PPS/slice decoder or Exp-Golomb parser is introduced.
Unmodified fixed FFmpeg `trace_headers` supplies the forensic field parser;
byte-exact coded checks are stronger than tolerating changes to those fields.
Existing bounded AVCC parsing returns explicit errors, and the exact known
valid SEI EBSP fingerprint cannot admit invalid emulation prevention or
missing/changed trailing bits. Unit negatives cover every identifier byte,
truncation, unknown NALs, SPS/PPS/VCL/AUD, color/profile/level/extradata,
timing/key/side-data/counts, duplicate container matches and unrelated bytes.
Five actual FFmpeg `h264_metadata` mutations alter SPS/VUI primaries,
transfer, matrix, range and timing; all fail the real gate. Color-conflicting
mutants may fail earlier during conservative media inspection; those reasons
are preserved rather than claimed as a successful decode. The coded-only
synthetic mutations reject independently while decoded facts stay unchanged.

## Table C — Qualification result

| Fixture | Same-driver ×10 | Legacy 1A | Legacy 1B | New 1B-P | 1C | 2 | Cross-stack 3 |
|---|---|---|---|---|---|---|---|
| canonical-h2648 | PASS on both | FAIL | FAIL | PASS | PASS | PASS | Different; diagnostic |
| canonical-h2648-auto | PASS on both | FAIL | FAIL | PASS | PASS | PASS | Different; diagnostic |
| hevc-pq-to-sdr10 | PASS on both | FAIL | FAIL | PASS | PASS | PASS | Different; diagnostic |

Historical Lavc patch-only H.264 stays 1A FAIL / 1B PASS / 1C PASS / 2 PASS.
The retained reference has SHA
`3181fa9775403a2f30e60157797adf370927b4196c960f59d1b5084203d42bef`.
Its historical `/tmp` candidate is absent. A bounded recipe rematerializes
that output from the retained reference by the two already documented unique
equal-length Lavc/Lavf 102→103 identifier changes, then **requires** SHA
`e45071c0c350deae063c8bb10f7e433ee764a107aff01294db391e7f56bf24af`,
the pre-existing receipt's candidate identity. It is not a guessed old input
or a newly established baseline. The unmodified historical oracle is actually
rerun on those byte-identified artifacts; its source files also match HEAD.
Same-stack deterministic comparisons still require exact packets and complete
artifact identity. Cross-driver outputs get separate hashes, not one shared
golden file. The canonical baseline is not overwritten or promoted.

## Final-source rerun and limits

Final-source five-stack core ×3, canonical 17×3 retained, lifecycle and static
reruns are in `target/stage54c2a-evidence/matrix-final`, with append-only
continuation evidence under `continuation-20261006`. All **15 core runs / 360
classifications pass**, with no failed formal gates. Cores 2/3 on every stack
require 16 byte-exact positive artifacts and eight identical expected-rejection
semantics; no same-stack comparison invokes Tier 1B-P. The candidate's first
core independently reruns all three Tier 1B-P comparisons, preserving legacy
Tier 1A/1B FAIL beside Tier 1B-P/1C/2 PASS. All six key normal-build outputs
match the measurement captures and original retained outputs by SHA-256.

Canonical retained checks pass for **17 paths ×3 = 51 outputs** (15 SDR, six
PQ-preserving, 30 HDR-to-SDR), including full decode/timing/metadata and retained
hash comparisons. Five retained gates, seven lifecycle gates (FD/rollback,
encode/mux faults, FreeType PQ parity, audio validation/cancellation and long
audio) and 11 final static gates pass. H.264 10-bit planning rejection preserves
the destination sentinel. Complete source identity matches at the end.
Canonical checks overlap the corrected iHD runs; these are correctness checks,
not new serialized performance measurements. The final seal is an explicit
review of this complete evidence, not an inference from a subset runner's exit.
The preserved execution-only runner index remains `NOT SEALED`; that is not
the current stage decision. The enclosing reviewed closure receipt records
the final qualification and seal explicitly.

The prior
full C-2 mux/performance/descriptors and 6×3000-frame stress remain historical
evidence on the recorded pre-closure source. No shader, mux, audio, surface
ownership, pixel-processing or normal production encoding behavior changed;
measurement-only readback is absent from the normal build. Reuse is limited
to those untouched mechanisms, not a claim that their old source hash equals
the new source. Current final-source core/retained/lifecycle gates pass.

The interrupted matrix is continued append-only after checking the complete
source identity. A first continuation iHD capture used an identical executable
at a different copied path. The exact binary-identity check conservatively
rejected that edge, so Tier 1B-P was not invoked and its three legacy failures
remained failures. That setup-invalid attempt is preserved separately; it is
not counted as a qualifying run. The corrected capture actually uses the
canonical executable path and passes the existing identity check. Neither the
source nor the comparator is changed to accommodate the continuation.

Current final static evidence: release workspace build, workspace tests in
normal and encode-characterization configurations, both all-target clippy
configurations with warnings denied, fmt and diff checks PASS. All **1203
actual workspace SPIR-V modules**, 34 distinct shader hashes, pass
`spirv-val --target-env vulkan1.3`, zero failures. No shader changes.

Limits remain one Intel Arc Meteor Lake device, the five recorded userspace
stacks, one active kernel, and exactly characterized tuples. Alternate-kernel
testing remains `UnavailableSafely`; no AMD/NVIDIA/universal iHD/Mesa/minimum
version claim, no version denylist, and no broad new VCL/SEI equivalence rule.
Driver-private defaults remain unobserved as stated above. Intermediate failed
inspection/mutation attempts and the source-invalidated capture v1 are retained
but not counted as final PASS evidence. There are zero unresolved differences
in the three closure cases. Stage 5.4D is justified but not started.

## Requested 59-item closure ledger

| # | Item | Result/evidence |
|---|---|---|
| 1 | Failing fixtures | The three exact IDs above; output codec clarified. |
| 2 | Reference stack | FFmpeg8.1.3/ANV26.2.3/iHD26.1.5 exact ID above. |
| 3 | Candidate stack | Only iHD25.4.6 changes; exact ID above. |
| 4 | Source/toolchain proof | Frozen HEAD/diff/lock/files, binary/tools/libs and actual initialized drivers retained. |
| 5 | Same-stack determinism | 3×2×10 outputs, all ten equal in each cell. |
| 6 | Pre-encode NV12 | 300 Y+UV hashes per route and packed SHA exact across drivers. |
| 7 | AVFrame metadata | All recorded fields equal; actual unspecified SAR/time base noted. |
| 8 | AVCodecContext | Before/after scalar snapshots equal. |
| 9 | VAAPI options/config | Full raw AVOptions and submitted native encode config/sequence fields equal; queried defaults unobserved. |
| 10 | NAL inventory | Production305, isolated307 with explicit duplicate headers; full type/ref/size/SHA inventory. |
| 11 | Divergent AU | AU0, DTS=PTS=0. |
| 12 | Divergent NAL | ProductionNAL2; isolatedNAL4; SEI only. |
| 13 | SPS raw | Exact. |
| 14 | SPS semantics | Full fixed-FFmpeg parse equal. |
| 15 | VUI | Full color/timing/chroma/HRD/reorder fields equal. |
| 16 | PPS | Full parse and raw bytes equal. |
| 17 | SEI | One known type5/UUID/102-byte message; driver text only. |
| 18 | Existing exemption | Frozen historical Lavc patch-token rule, unchanged. |
| 19 | Unknown SEI | None observed; mutation/unknown controls reject. |
| 20 | IDR positions | 0,250. |
| 21 | GOP | 250, no B frames; all AU key/timing facts exact. |
| 22 | Slice structure | 300 one-slice VCLs, header fields exact. |
| 23 | Divergent slice header | None. |
| 24 | VCL/residual | All complete bytes exact. |
| 25 | Decode comparison | Fixed software decoder, 300 frame hashes/timing rows exact. |
| 26 | Tier1A | Cross-driver FAIL retained; same-driver exact. |
| 27 | Current Tier1B | Cross-driver FAIL retained. |
| 28 | Tier1C | PASS, independent software decode. |
| 29 | Tier2 | PASS; proven-packet-only normalization also exact. |
| 30 | ES | Only known identifier SEI differs. |
| 31 | Container isolation | ftyp/free/moov exact; only3 mdat bytes differ. |
| 32 | Encoder-only | Six runs PASS, same retained input/config; no decoder/Vulkan/audio/mux. |
| 33 | Driver-default audit | Submitted config/sequence equal; private/queried defaults unobserved, not guessed. |
| 34 | Project config | Existing product-semantic pins sufficient; no production fix. |
| 35 | Product fields | No affected product-semantic field. |
| 36 | Coding-only fields | No SPS/PPS/VCL implementation-choice difference either. |
| 37 | RouteA canonical | Informational identifier only; TableC. |
| 38 | RouteB PQ→SDR | Same isolated signature; TableC. |
| 39 | RouteC auto | Same signature and canonical pixels/config; TableC. |
| 40 | Common signature | Actual iHD identity in one known SEI. |
| 41 | Outcome | C, oracle scope mismatch; not A/B or an unexplained D. |
| 42 | Project fix | None; opt-in measurement instrumentation only. |
| 43 | Oracle addition | Independent cross-driver Tier1B-P; legacy oracle untouched. |
| 44 | Justification | Actual pre-encode/config/raw-coded-field/isolation proofs, not pixel-only reasoning. |
| 45 | Negative mutations | Unit and actual SPS/VUI primaries/transfer/matrix/range/timing reject. |
| 46 | Historical H.264 | Required old tier pattern preserved by unchanged oracle. |
| 47 | Five-stack rerun | 15 core runs / 360 classifications PASS; strict same-stack repeats PASS. |
| 48 | 17×3 rerun | 51 retained outputs and all five retained gates PASS. |
| 49 | Registry | Five current QualifiedStack records; historical source/stacks and failed qualification retained separately. |
| 50 | iHD25.4.6 final | QualifiedStack under explicit Tier1B-P; strict cross-driver historical FAIL remains. |
| 51 | Three-case Unresolved | 0; all three independently pass the bounded additive contract. |
| 52 | Allowed claim | Exact tested tuples/stacks and the explicit additive contract only. |
| 53 | Disallowed claim | Universal/minimum-version portability, generic SEI/VCL waiver, same cross-stack golden hash. |
| 54 | SPIR-V | 1203 actual modules,34 distinct hashes,Vulkan1.3 all PASS. |
| 55 | Static | Seven final-source checks PASS, normal and measurement builds. |
| 56 | C-2A | SEALED, outcome C / OracleContractCorrection. |
| 57 | C-2 | SEALED; historical failures remain adverse evidence, not relabeled PASS. |
| 58 | OverallC | SEALED; C-1A and C-1 already sealed, C-2 now closed. |
| 59 | StageD | Stage 5.4D is now justified. NOT STARTED. |
