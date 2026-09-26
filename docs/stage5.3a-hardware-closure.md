# Stage 5.3A Intel hardware closure and canonical 10-bit baseline

Status, 2026-09-26: **SEALED** on the measured Intel Arc Meteor Lake stack.
The original Stage 5.2C-3 10-bit input artifact and SHA-256 were not retained,
so the two old output hashes remain unverified historical references, **not
PASS results**. A new, checked-in, three-times-reproducible 1080p/300-frame
10-bit input and three-times-identical HEVC/AV1 production outputs replace
them as the current regression gate. No Stage 5.3B work was started.

## Environment and source identity

The device-accessible execution context reported `/dev/dri/card1` (226:1,
`video`, mode 0660) and `/dev/dri/renderD128` (226:128, `render`, mode 0666);
`by-path/pci-0000:00:02.0-render` points to the latter. The normal sandbox
intermittently hid the node or denied open, so hardware commands used the
user-authorized device-accessible context **in the same workspace**, without
copying the tree or changing host configuration. PCI `8086:7d55` rev 08 is
Intel Meteor Lake-P Arc Graphics, kernel driver `i915`, kernel
`7.2.7-200.fc44.x86_64`. VAAPI reported libva 2.23.0, Intel iHD 26.1.5,
HEVC Main10 and AV1 Profile0 decode/encode entrypoints. Vulkan reported Intel
Arc (MTL), Mesa ANV 26.2.3, Vulkan 1.4.354. Installed FFmpeg is 8.1.3;
the older small fixtures were generated with 8.1.2. The new canonical input
uses a separately pinned 8.1.3 generator, without changing that old pin.

Before test edits: HEAD `ba64fa7ba8d264be16e5e8b434a6b5413fedd86e`,
tracked `git diff --binary` SHA-256
`45f01304cb47dea795b56e82b199681a7677a4b0d2dc9564d2d2230cce8318df`,
sorted untracked-file content-list SHA-256
`50f86ccaf96a4e0978e447602db091a3a80344e851a39ff6e488f2e9e7231e44`,
and `Cargo.lock` SHA-256
`f8cf97f855fcbcb422ef1b74f2367a7ac40b612dcc41cc0a1d8a23f3e27746b8`.
`git status --short` contained only the ongoing Stage 5.3A changes. The
closure work added hardware diagnostic tests, a fixture generator and
documentation, not a copied or different production implementation.
`git diff --check` was clean before testing.

## Software ↔ VAAPI color semantics

An ignored unit diagnostic inspected live FFmpeg `AVCodecParameters`, opened
`AVCodecContext`, and the first decoded `AVFrame` on software and VAAPI paths:

| Fixture | Codec parameters | Decoder context | First frame | Chroma |
| --- | --- | --- | --- | --- |
| HEVC Main10 BT.709 SDR | BT.709 P/T/M, limited | same | same | left in all three sources and both modes |
| AV1 Main10 BT.709 SDR | BT.709 P/T/M, limited | same | same | unspecified in all three sources and both modes |

P/T/M means primaries/transfer/matrix. The expanded ignored integration test
compared **complete raw stream and frame metadata**, resolved P/T/M/range,
field provenance, dynamic-range class, support decision and effective static
metadata between software and VAAPI. Both codecs matched exactly: P/T/M
BT.709, range limited, all four provenance fields `Frame`, class `Sdr`,
support `Ok(())`, and no static HDR metadata. There was no observed
codecpar/context or stream/frame difference, let alone one changing the
semantic decision. **Provenance decision: NO additional permanent raw-source
layer is justified on this measured Intel/FFmpeg path.** The documented
limitation is retained for other drivers/containers. Chroma location is an
observation only, for future color-sampling semantics; no processing was added.

On actual VAAPI first-frame decode, the following separate class/support
decisions matched software. Production preflight also rejected the BT.2020,
HLG and AV1 PQ fixtures before pixel processing:

| Fixture | Dynamic range | Support result |
| --- | --- | --- |
| BT.709 full-range HEVC | `Sdr` | `UnsupportedFullRange` |
| BT.2020 primaries/matrix, SDR transfer HEVC | `Sdr` | `UnsupportedWideGamutSdr` |
| HEVC PQ without static metadata | `HdrPq` | `UnsupportedHdrPq` |
| HEVC HLG | `HdrHlg` | `UnsupportedHdrHlg` |
| AV1 PQ | `HdrPq` | `UnsupportedHdrPq` |
| HEVC PQ with static metadata | `HdrPq` | `UnsupportedHdrPq` |

For the last fixture, both software and VAAPI frame side data exposed the
mastering-display maximum as the exact rational `10000000/10000 = 1000
cd/m²`, MaxCLL 1000 and MaxFALL 400; stream static side data was absent.
Complete static metadata did not grant processing support. PQ without static
data still classified `HdrPq`. The existing synthetic Core SDR→PQ mid-stream
stability test passed in both workspace test variants and hard-fails a
semantic change; there is no decoder-level hardware injection hook, so no
mid-stream VAAPI injection is claimed.

An existing H.264 target retained SHA-256
`3181fa9775403a2f30e60157797adf370927b4196c960f59d1b5084203d42bef`
after production VAAPI full-range rejection; no sibling staging file
appeared. The error named full-range input and the limited-code renderer.
`--explain-plan` on real Intel printed BT.709 P/T/M, limited range, `Frame`
provenance, `Sdr`, supported processing and the prior GPU-resident P010
path. PQ explain-plan printed transfer `Pq`, class `HdrPq` and
`UnsupportedHdrPq` before failure. `--capabilities` separated software
color-processing policy from hardware codec support. HEVC Main10 and AV1
10-bit explain plans selected VAAPI decode → P010 input interop → Vulkan
ASCII → P010 output interop → VAAPI encode, with no planner regression.
Planning CPU wall was 0.008–0.011 ms in these sanity runs; this is not a
benchmark campaign. The earlier release-mode color resolution sanity test
measured about 35 ns/call.

## Production outputs and retained hashes

The retained Stage 5.1A.1 H.264 source at
`target/stage51a1-evidence/h264-testsrc2-300.mp4` matched its published
SHA-256 `6b47b510c4a8f604e6404b1fbbc5f68bdaa77043976acdad0153ef83524b6e34`.
Release runs used width 80, color, no audio, VAAPI decode/encode, Vulkan,
and explicit input/output interop. The three 8-bit hashes matched Stage
5.2C-3 **exactly**:

| Output | Current SHA-256 | Retained comparison |
| --- | --- | --- |
| H.264 NV12 | `3181fa9775403a2f30e60157797adf370927b4196c960f59d1b5084203d42bef` | exact |
| HEVC Main NV12 | `d4b13786c968e43a502845df46d92dc2dbc951e5e2ef69e4313d5c541664208b` | exact |
| AV1 Main NV12 | `f044a327c093b8489e03f20c98deab8f80e99f01092fe35595b4f22796c7c534` | exact |
| HEVC Main10 P010 | historical `83f4377557d77ffd48cf32580ea454132fef5bc837d6473a2723516ddac5f22a` | historical reference only; input identity unrecoverable; not a current regression gate |
| AV1 Main 10-bit P010 | historical `106ac4aa9946c4814545ffc35eed0f094c6703942fe67f1e0ba34c18868301c8` | historical reference only; input identity unrecoverable; not a current regression gate |

The original Stage 5.2C-3 1080p/300-frame 10-bit input artifact **and its
SHA-256 were not retained**. The two historical 10-bit output hashes therefore
cannot be independently reproduced from preserved evidence and are **not
marked PASS**. A bounded search of the C-2/C-3 reports, testing guide,
`tests/`, `benches/`, `scripts/`, `.gitignore` and `/tmp` found neither the
original file nor an independently verifiable hash/exact command. A new
128×128/30-frame Main10 input successfully traversed HEVC and AV1 full-GPU
production paths; the current hashes were respectively
`6cbfddeff8c54ec5e6c406f247d42ed26d75b7a61b2b660cea7a48df6d063089`
and `3aa084aa902c7771b0ae8e7954e227ff467d548175fcdef5e7d0d0a8256f7096`.
These are **different-workload smoke hashes**, not mismatches against the
historical 300-frame references. No metadata-only correction to an old
baseline was accepted; the mismatch investigation protocol was not triggered.

`ffprobe` on each actual new output reported:

| Output | Primaries | Transfer | Matrix (`color_space`) | Range |
| --- | --- | --- | --- | --- |
| H.264 8-bit | `bt709` | `bt709` | `bt709` | `tv` |
| HEVC Main 8-bit | `bt709` | `bt709` | `bt709` | `tv` |
| AV1 Main 8-bit | `bt709` | `bt709` | `bt709` | `tv` |
| HEVC Main10 10-bit | `bt709` | `bt709` | `bt709` | `tv` |
| AV1 Main 10-bit | `bt709` | `bt709` | `bt709` | `tv` |

The two 10-bit outputs decoded as HEVC Main 10 and AV1 Main, both
`yuv420p10le`. No tested output had limited pixels labeled full; both P010
outputs explicitly signal limited range. The 8-bit hash identity establishes
no 8-bit media regression. For 10-bit, the earlier small-workload conversions
established a live path; the separately qualified canonical v1 below now
establishes the reproducible regression identity, **not** old-output identity.

## Reconstructed canonical 10-bit baseline v1

This is a **replacement**, not recovery of the historical Stage 5.2C-3
source. `tests/fixtures/codecs/generate-10bit-baseline.py` uses integer-only
gradients, moving rectangle/circle, changing luma/chroma and no random or
external media. `generate-10bit-baseline.sh` (with `set -euo pipefail`)
produces 300 true-10-bit 192×108 planar frames, upscales them 10×, then
encodes HEVC Main10 1920×1080 at 50 fps with explicit BT.709 P/T/M and
limited range. Its full FFmpeg command and all codec, timing, pixel-format
and metadata options are checked in. Exact invocation:

```bash
bash tests/fixtures/codecs/generate-10bit-baseline.sh \
  tests/fixtures/codecs/hevc-main10-canonical-v1.mp4
```

The existing small-fixture generator remains pinned to FFmpeg 8.1.2. That
binary was not available locally, so the **new** generator is explicitly
pinned to installed FFmpeg 8.1.3 (`ffmpeg-8.1.3-1.fc44.x86_64`, source RPM
`ffmpeg-8.1.3-1.fc44.src.rpm`) and x265 4.1-4.fc44. Exact binary, library,
full build/configuration-output fingerprints and Python 3.14.7 are in
`tests/fixtures/codecs/canonical-generator-version.txt`. The shell and
Python generator SHA-256 values are respectively
`8162c4764b27810d9cd35107be9d534b1e619fb29abda5cb0ffcabea45f5f026`
and `faae4e25dd94783e54c936c646ffa3668d2e474f803d1501ac612ccd04844069`.

The checked-in `hevc-main10-canonical-v1.mp4` is 6,733,222 bytes. After
deleting the first output and regenerating, then generating a third copy,
all three SHA-256 values were exactly
`df69c98ca6592b08c6bc34127b2bb5cf4125c759fd852b830e5426d5818fccda`.
`ffprobe` confirmed HEVC Main 10, 1920×1080, 300 frames, 50/1 fps,
`yuv420p10le`, left chroma, P/T/M BT.709 and limited (`tv`) range.
Full decoded-sample inspection found **700,261,022 / 933,120,000** samples
(75.0451%) with nonzero low two bits; this is not an 8-bit source shifted
into a 10-bit container. The file and checksum are preserved in the fixture
directory and `SHA256SUMS`.

The exact Release production commands and flags are in [testing.md](testing.md).
Both use this *same input*, width 80, built-in 8×8 font, color on, no audio,
300-frame limit, VAAPI decode/encode on `/dev/dri/renderD128`, Vulkan GPU
mapping and explicit P010 input/output interop. Only output codec and output
filename differ: `--output-codec hevc --output-bit-depth 10` versus
`--output-codec av1 --output-bit-depth 10`. CLI logs confirmed the five-step
VAAPI decode → DRM PRIME/DMA-BUF input → Vulkan P010 ASCII → writable P010
output interop → VAAPI encode path on every run.

| Canonical output | Run 1 SHA-256 | Run 2 SHA-256 | Run 3 SHA-256 |
| --- | --- | --- | --- |
| HEVC Main10 | `07f231ab49eebd54cc0e85012f7dd12e424f580d17005bc20028b458168323fe` | identical | identical |
| AV1 Main 10-bit | `2ab82984f690ea3fe5472c874dfce7cc474f7c46266f5cd86ed8deee20b36fda` | identical | identical |

Both outputs independently probed as P/T/M BT.709, range `tv`/limited,
1920×1080, 50 fps and `yuv420p10le`; HEVC profile was `Main 10`, AV1
profile `Main` (Profile0, 10-bit). Full software decode-back with error-on-
decode enabled completed without error. `ffprobe -show_frames` counted
exactly 300 frames each, with first/last PTS **0/76544** in time base
1/12800; every frame's PTS equaled `index × 256`, so there were no timestamp
gaps or reversals. The new ignored 300-frame hardware test on this **same
canonical input** compared every pre-encode P010 byte, including staged
upload/download, for both codec outputs. It passed under Khronos Validation,
found **188,297,203** non-four-aligned active samples in each pre-encode
reference, and returned FDs **4 → active 36 → peak 36 → 4** for each codec.

**Canonical 10-bit baseline v1**, established during Stage 5.3A closure:
input `df69c98ca6592b08c6bc34127b2bb5cf4125c759fd852b830e5426d5818fccda`,
HEVC output `07f231ab49eebd54cc0e85012f7dd12e424f580d17005bc20028b458168323fe`,
AV1 output `2ab82984f690ea3fe5472c874dfce7cc474f7c46266f5cd86ed8deee20b36fda`.
It replaces the unrecoverable Stage 5.2C-3 input-dependent 10-bit baseline
for future regression testing. It does **not** assert equality to either
historical output hash.

## Interop, audio, lifecycle and toolchain

The HEVC and AV1 P010 input descriptor/hwdownload test passed for 30 frames
per codec. Actual 64×64 descriptors each had one object, R16 Y at offset
0/pitch 128, GR32 UV at offset 8192/pitch 128, and modifier
`0x0100000000000009`. HEVC and AV1 encoder-owned 30-frame P010 pre-encode
parity tests passed, including 187,829 and 329,334 non-four-aligned active
samples respectively. A production AV1 10-bit output also served as a real
VAAPI AV1 10-bit input to a second full-GPU AV1 conversion. Both 10-bit
codec output interop paths and one NV12 full path completed.

A P010 full-GPU HEVC Main10 production CLI run completed 3000 frames. The
instrumented encoder-owned 3000-frame parity/surface-reuse test passed with
FD **before 4, active baseline 36, peak 43, after 4**. The NV12 two-slot
input-parity smoke passed. `ASCIIFLOW_VULKAN_VALIDATION=1` was enabled on
actual NV12 and P010 30-frame full-interop CLI runs and both encoder-owned
P010 parity tests: no Validation Error, VUID or sync error appeared, and
the parity tests asserted zero Validation errors. This qualifies these
observed paths, not all Vulkan operations.
An AV1 10-bit full-GPU production CLI also completed 300 frames on the new
128×128 looped input; this separate smoke is not the historical 1080p
retained-hash workload or canonical v1.

A new 90-frame Main10 input with two AAC tracks ran through HEVC Main10
full interop using a monospaced FreeType font, and AV1 10-bit full interop
using the built-in font. Both outputs retained all 283 compressed packet
records exactly: payload SHA-256, PTS, DTS, duration and size. Languages
`jpn`/`eng`, default/forced dispositions, track order, and A/V durations
were unchanged. This input contains 141+142 packets, not the historical
142+142 fixture, and is not claimed as that original source.

All 133 generated `.spv` modules passed installed
`/usr/bin/spirv-val --target-env vulkan1.3` (Fedora package
`spirv-tools-2026.1-1.fc44.x86_64`); no RPM extraction or installation was
needed. All 16 codec fixture SHA-256 values passed. Final Release workspace
build, both normal and `encode-characterization` workspace test variants,
both all-targets strict Clippy variants, format check and diff check passed.

## Seal decision and remaining limits

**SEALED.** The canonical v1 input is checked in and three-times byte
reproducible; both 1080p/300-frame Intel full-GPU outputs are three-times
byte reproducible, metadata-correct and decode cleanly with exact frame
count/timestamps. Full pre-encode P010 parity, color-classification gates,
AAC/FreeType, Validation, FD lifecycle, SPIR-V and workspace/static checks
remain green on the same production source tree. The two historical Stage
5.2C-3 10-bit hashes remain **not PASS**, because their input identity was
not retained; canonical v1 supersedes them as a stronger current gate.

The output hashes are specific to the measured Intel iHD/Mesa/FFmpeg stack
and exact CLI config; hardware or encoder-version changes require a new
qualification rather than silently updating these hashes. Future retained
evidence must keep input generator/version/command/hash and output
command/hash or a structured nondeterministic oracle, as stated in
[testing.md](testing.md). HDR pixel processing, tone mapping, wide-gamut
conversion and other Stage 5.3B work remain outside this seal. **Stage 5.3B
is now justified; it was not started here.**
