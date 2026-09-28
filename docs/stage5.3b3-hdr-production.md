# Stage 5.3B-3 — HDR-preserving production integration

Status: **SEALED**. All required closure gates passed on the qualification host.
Qualification host: Intel Arc Meteor Lake, `/dev/dri/renderD128`.
No Stage 5.3C work or tone mapping is part of this change.

## Production contract and dispatch

Signal classification is not permission to process. `DynamicRangeClass::HdrPq`
remains separate from the SDR color-policy assessment. The planner explicitly
selects `ColorProcessing::Sdr` or `HdrPqPreserve`; P010 alone never selects PQ.
The PQ signal must resolve without unknown/conflicting fields to BT.2020
primaries, ST.2084/PQ transfer, BT.2020 non-constant-luminance matrix, limited
range, left-sited ten-bit 4:2:0. Only HEVC Main10 and AV1 Main input qualify.
Layout, profile, native frame geometry and per-frame color/static-metadata
stability remain checked. Native P010 geometry changes now fail before either
Host conversion or VAAPI surface delivery; they cannot be silently cropped.

The only legal production path is:

```text
VAAPI decode -> P010 DMA-BUF input -> Vulkan PQ map/render
  -> encoder-owned P010 DMA-BUF output -> VAAPI HEVC Main10 or AV1 Main 10-bit
```

`auto` may select that path only after actual stream decode/import, PQ compute
execution, encoder opening and output-surface import probes succeed. There is
no vendor-name eligibility shortcut. Explicit CPU, software decode, software
encode, disabled interop, or missing PQ compute/import facts fail. Initialization
failure is terminal even with an automatic policy; there is no HDR replan to
CPU, staging, software media, SDR shaders or eight-bit output. Runtime errors
remain terminal. The existing bounded two-slot owner/fence/foreign-release
architecture is reused; no third slot or new pool-depth policy is introduced.

The existing output flags are mandatory:

```text
--output-codec hevc --output-bit-depth 10
--output-codec av1  --output-bit-depth 10
```

Default H.264/eight-bit output fails for PQ. There is no new HDR CLI switch,
implicit HDR→SDR, SDR→HDR, 10→8-bit truncation, gamut conversion, HLG support,
or CPU production HDR fallback. Software PQ decode is used for signal probing
and decode-back diagnostics, not a selectable production path. `--explain-plan`
reports the explicit processing mode, no tone mapping, full P010 hardware path,
profile, canonical output color and static-metadata policy.

## Output signal and static metadata ownership

The plan owns the intended output descriptor; the encoder does not clone native
input color fields. Its fresh codec context, fresh stream, encoder-owned frames
and submission boundary consistently emit BT.2020/PQ/NCL/limited/left. Context
fields are fixed before codec opening, so they govern HEVC SPS VUI and AV1
sequence headers, not merely MP4 tags. Main10/AV1 Main ten-bit profiles remain
distinct from eight-bit encoder facts.

ASCII changes pixels. Source mastering-display, MaxCLL and MaxFALL are neither
copied nor recomputed. Both corresponding AVFrame side-data kinds are explicitly
removed before native encoding, even if a future caller supplies them. This is
PQ preservation, **not HDR10 mastering/static-metadata qualification**.
The real static-source test embeds mastering-display SEI and MaxCLL=1000 /
MaxFALL=400 into a legal canonical HEVC source, confirms their presence, and
checks their absence from both output streams and all decoded frames.

## Canonical PQ source v1

The checked-in integer-only generator is
`tests/fixtures/codecs/generate-pq-canonical.py`, orchestrated by
`generate-pq-canonical.sh` with `set -euo pipefail` and FFmpeg 8.1.3. Both inputs
come from the same full-resolution planar `yuv420p10le` source: 1920×1080,
300 frames, 50 fps, BT.2020/PQ/NCL/limited/left. It includes black/near-black,
100/1000/4000/10000-nit quantized gray bars, a ten-bit gradient, color bars and
a moving highlight. There is no external media, scale-up, random source,
eight-bit intermediate, locale or wall-clock input.

Raw source identity:

```text
bytes 1866240000
SHA256 7278abe3ec0b44ac62e1a5ae090df81206a4e9fb82379053885c622f37c3a48f
HEVC input 72eadd220051231f85814e7161379f64adf9e472ec7cb33f1564b30b59f2152a
AV1 input 526ddba68a3a4a9c8878a5f93f8a75e556324e99e1b57f30e67d4883c922a71e
```

Every actual software-decoded frame of both codecs is byte/hash-exact to the
raw source. Actual decoded nonzero low-two-bit samples: Y=513990960,
U=32400000, V=38880000; total 585270960 / 933120000. Three independent
encoded generations matched both input hashes and the entire identity JSON
(SHA-256 `a971bb7ffb1627d961ae36db9885166606130b32c36d718bea333e311f5cf2f9`).
Both encoded inputs and complete identity/version are retained in the fixture
tree, not just temporary storage.

Two rejected generator attempts are not baselines. Output-only color tags
caused implicit raw-input pixel conversion; explicit matching input tags fixed
it. AV1 lookahead/alternate-reference filtering changed a few inter-frame codes
despite CRF=0; `-lag-in-frames 0 -auto-alt-ref 0` fixed full source parity.
Neither codec filtering nor pixel math was changed to accommodate an error.

Exact generation and verification commands:

```bash
bash tests/fixtures/codecs/generate-pq-canonical.sh /tmp/pq-canonical-run1
python3 tests/fixtures/codecs/verify-pq-canonical.py /tmp/pq-canonical-run1 /tmp/pq-canonical-run1-verification.json
```

The scripts contain the complete FFmpeg commands, including explicit input and
output format, geometry, rate, count, color, siting, single-thread lossless
parameters, metadata removal and fixed MP4 timebase. Identity records retain
generator/tool/native-library hashes, full FFmpeg build configuration, raw and
per-frame hashes, exact sample histograms and encoded identities. FFmpeg 8.1.3
is not substituted for another version.

## Retained output configuration and oracles

`tests/baselines/media/generate-pq-production-v1.sh` runs both profiles three
times with the same explicit configuration:

```bash
asciiflow INPUT OUTPUT --width 80 --charset standard --font builtin-8x8 --color true \
  --audio none --max-frames 300 --decode vaapi --backend vulkan --vulkan-mapping gpu \
  --encode vaapi --hw-device /dev/dri/renderD128 \
  --vaapi-vulkan-input-interop on --vaapi-vulkan-output-interop on \
  --output-codec hevc --output-bit-depth 10 --no-progress
# AV1 uses the identical command with --output-codec av1.
```

`verify-pq-production.py` checks actual container and extracted elementary
stream signals, all decoded frame colors/geometry/timestamps, packet count and
payload hashes, and decoded frame hashes. Tier 1A is strict packet identity;
1B is coded semantic identity, 1C is complete decoded pixel/timestamp identity,
Tier 2 is container/frame structure, and Tier 3 is exact-build whole-file identity.
No lossy encoded output is
compared byte-for-byte to the CPU rendering oracle. That comparison belongs at
the pre-encode P010 boundary, using the very VAAPI surface imported by Vulkan.
The existing SDR/H.264 approved-metadata comparator policy is unchanged.
Negative output-oracle controls reject BT.709, missing transfer, full range and
leaked mastering/CLL metadata.

Canonical retained production output identities (all three runs identical):

| Output | Bytes | SHA-256 |
| --- | ---: | --- |
| HEVC Main 10 | 2101434 | `0e8de23d5d222ab0476ea816d27b18e37dc73bf1f824e79bedc1070c409ded63` |
| AV1 Main/Profile0 10-bit | 5239237 | `41a8af52985a6be0121cfe7c4f4160a3ebdecd0acc908882774d2a0151185388` |

Both actual container streams, extracted elementary streams and all 300 decoded
frames report `bt2020`, `smpte2084`, `bt2020nc`, `tv` and left chroma. Software
decode-back reports `yuv420p10le`, 1920×1080, 50/1 fps, zero start, 6 seconds and
300 frames/packets without decoding errors. MP4 timebase is 1/12800, PTS step
256; decoded timestamps are exactly frame index / 50 seconds. No mastering or
content-light side data is present. Rust's strict PQ comparator also passed both
codec repeat pairs without metadata exceptions.

The retained manifest is [pq-production-v1.json](../tests/baselines/media/pq-production-v1.json),
with [hardware evidence](../tests/baselines/media/pq-production-v1-hardware.json).
It records source HEAD plus production diff identity (uncommitted implementation,
not a fabricated commit), Cargo.lock, binary, generator, tool, input, packet,
decoded-frame, structure and exact-file identities. Final normal binary SHA-256:
`cb0071194d923a5bd534f1a19d0054c9f1c9af793d7b14b3ec150640773f940c`.
The normal final build reproduced all six retained outputs; the separate
measurement build also produced the same six hashes.

Exact canonical production commands (each executed three times):

```bash
target/release/asciiflow tests/fixtures/codecs/hevc-main10-pq-canonical-v1.mp4 OUTPUT_HEVC.mp4 \
  --width 80 --charset standard --font builtin-8x8 --color true --audio none --max-frames 300 \
  --decode vaapi --backend vulkan --vulkan-mapping gpu --encode vaapi --hw-device /dev/dri/renderD128 \
  --vaapi-vulkan-input-interop on --vaapi-vulkan-output-interop on --output-codec hevc --output-bit-depth 10 --no-progress
target/release/asciiflow tests/fixtures/codecs/av1-main10-pq-canonical-v1.mp4 OUTPUT_AV1.mp4 \
  --width 80 --charset standard --font builtin-8x8 --color true --audio none --max-frames 300 \
  --decode vaapi --backend vulkan --vulkan-mapping gpu --encode vaapi --hw-device /dev/dri/renderD128 \
  --vaapi-vulkan-input-interop on --vaapi-vulkan-output-interop on --output-codec av1 --output-bit-depth 10 --no-progress
```

### Pre-encode numeric oracle and long production stress

All 300 frames of **each** canonical input were compared against the permanent
CPU f64 reference, downloading the same VAAPI surface that Vulkan imported.
The standard width-80/builtin-color configuration used encoder-owned P010 output.
Glyph differences were zero. Identical per-input plane histograms:

| Plane | Exact codes | Delta 1 | Delta >1 | Maximum | p50/p95/p99/p99.9 |
| --- | ---: | ---: | ---: | ---: | --- |
| Y | 622022067 | 57933 | 0 | 1 | 0/0/0/0 |
| Cb | 153765000 | 1755000 | 0 | 1 | 0/0/1/1 |
| Cr | 150750000 | 4770000 | 0 | 1 | 0/0/1/1 |

Pre-encode FD before/peak/after was 4/22/4 for each codec, validation errors zero.
No tolerance or PQ shader/reference math was changed. The existing neutral,
alternating-slot, FreeType and invalid-sample PQ hardware controls also passed.

Both HEVC and AV1 long tests submitted, encoded, muxed and completely decoded
3000 frames. The source was replayed ten times with each input context released
only after outstanding surfaces drained; output timestamps remained one continuous
zero-origin 50fps CFR timeline. Both outputs were 1920×1080 P010, canonical PQ,
correct ten-bit profiles, no mastering/CLL. FD before/active/peak/after was
4/25/30/4 **for each** output codec. Both validation error counts were zero.
This exercised the unchanged two Vulkan frame slots, not a third slot.

## Closure gates

| Gate | Current evidence |
| --- | --- |
| Explicit planner admission/rejection, no HDR fallback | Unit tests passed, including all initialization checkpoints for both output codecs |
| PQ numeric/control/FreeType/invalid-input tests | Five real Vulkan tests passed; existing CPU f64 and shader math unchanged |
| Map/render/runtime/init simulated errors | Seven PQ checkpoints passed with FD 4→4 and healthy fresh retries |
| External-image/import/queue/fence lifecycle errors | Real diagnostic P010 surfaces passed; simulated host checkpoints, not device loss/hung-fence proof |
| HEVC/AV1 encode send/receive/drain faults | Real native PQ encoders passed |
| MP4 header/early packet/midstream packet/trailer errors | Real PQ encoders/muxers passed with root causes and exact FD restoration |
| Canonical source repeated generation | Three independent generations matched; both codecs' 300 actual decoded frames match the same raw source |
| Three production runs/profile and Tier 1A/1B/1C/2/3 | All passed, including final normal build and unchanged hashes from measurement build |
| Same/cross-codec, AAC/dual audio, FreeType, static-metadata absence | Six real CLI tests passed; all four codec pairs fully decode 300 frames |
| Automatic backend/decode/encode/interop policies | Four real codec-pair conversions selected only the qualified full hardware path |
| Cancellation/transactional output/fresh initialization | Exit 130; both outputs preserve existing destination, remove staging and pass fresh followup |
| Midstream color/static metadata | Portable contracts reject SDR→PQ and changed PQ mastering/CLL; no changing-bitstream hardware claim |
| All 300 canonical pre-encode CPU/GPU frames | Both inputs passed: zero glyph differences, max plane delta 1, FD restored, Validation zero |
| 3000 production encoded frames per output codec, full decode, FD/Validation | Both passed, FD 4/25/30/4, correct dimensions/CFR/color/profile, Validation zero |
| Five SDR baseline v2 profiles ×3 | All 15 whole-file hashes unchanged |
| SPIR-V Vulkan 1.3 | All 317 final cached modules passed `spirv-val --target-env vulkan1.3`; includes feature/build duplicates, not 317 new algorithms |
| Normal/measurement workspace build/test/clippy/fmt/diff | Both builds, both workspace test configurations and strict Clippy passed; fmt/diff clean; qualification-feature Clippy also passed |

Fault injections are host-side cleanup/error checkpoints. They do not claim a
failed real driver submission, device-lost recovery, a hung fence, or recovery
from asynchronous GPU failure. A fresh initialization after simulated failure
is tested separately from retrying a failed finalization.

## Performance and limits

Dedicated production measurements used `asciiflow-cli/encode-characterization`,
the exact retained configuration and three runs per codec. Generation, CPU oracle,
validation and other GPU workloads did not overlap. Measurement binary SHA-256:
`36ce85c56846009cbf2fac2bd09a8dbfc91856b5788407c67d6f49b071c2e52c`.
All six outputs matched the normal-build hashes. Full per-run process and stage
summaries are retained in the production manifest, not only median throughput.

| Metric (three-run median unless listed) | HEVC Main10 | AV1 Main 10-bit |
| --- | ---: | ---: |
| CLI pipeline FPS, runs 1/2/3 | 373.54 / 394.99 / 392.51 | 322.98 / 273.41 / 217.20 |
| Process wall seconds, runs 1/2/3 | 1.12 / 1.07 / 1.05 | 1.20 / 1.42 / 1.70 |
| Process CPU%, runs 1/2/3 | 71 / 74 / 74 | 76 / 75 / 60 |
| Full process FPS (300 / median wall) | 280.37 | 211.27 |
| Decode CPU wall ms/frame | 0.228 | 0.625 |
| Input DRM map / DMA-BUF import CPU wall ms/frame | 0.026 / 0.019 | 0.210 / 0.016 |
| Input GPU image→buffer ms/frame | 0.157 | 0.162 |
| GPU map / render ms/frame | 0.225 / 1.158 | 0.253 / 1.679 |
| Output DMA-BUF import CPU wall ms/frame | 0.006 | 0.006 |
| Output GPU buffer→image ms/frame | 0.129 | 0.135 |
| Encode send / receive / drain CPU wall ms/frame | 0.775 / 0.000 / 0.001 | 1.039 / 0.001 / 0.000 |
| Mux packet / flush / trailer / queue-send CPU wall ms/frame | 0.012 / 0.000 / 0.000 / 0.002 | 0.016 / 0.000 / 0.001 / 0.003 |

Process wall includes probing/initialization; CLI throughput covers the pipeline.
CPU%, asynchronous mux/encode waits and GPU timestamps have different, overlapping
boundaries and are not additive. Displayed 0.000 means below displayed precision,
not proof of zero cost. Run-to-run AV1 variation is preserved, not discarded or
attributed to an unmeasured cause. These are initial qualified production records,
not a B-2 compute-only percentage speedup or video-engine saturation claim.

Qualification is device/driver/toolchain-specific, not portable HDR support on
all GPUs or arbitrary HDR input. No HDR10 static mastering claim, HLG, full
range, BT.2020 SDR, unknown/conflicting color, software HDR media fallback,
tone mapping, gamut mapping or new variable-rate audio policy is added.
The historical pre-polarity and unrecoverable old 10-bit baselines remain
historical evidence, not replaced silently by new PQ hashes.

## Qualification identity and reproducibility

Observed device: Intel Arc Meteor Lake, vendor/device `8086:7d55`, ANV Vulkan
1.4.354, queue 0, 36-bit timestamps at 52.083332 ns. Both real P010 descriptors
used one 6389760-byte object, modifier 72057594037927945, plane pitch 3840 and
offsets 0/4177920. The planner does not use this vendor identity as eligibility.

Pinned x86_64 packages: FFmpeg/ffmpeg-libs 8.1.3-1.fc44 (avcodec 62.28.103,
avformat 62.12.103), x265 4.1-4.fc44, libaom 3.13.3-1.fc44, Intel media-driver
26.1.5-1.fc44, libva 2.23.0-3.fc44, Mesa Vulkan 26.2.3-1.fc44. Rust 1.97.1
(`8bab26f4f68e0e26f0bb7960be334d5b520ea452`), LLVM 22.1.6; Python 3.14.7.
FFmpeg's complete version/configuration and executable/library hashes are in
the retained canonical identity and generator-version artifact.

Generator SHA-256 values:

```text
generate-pq-canonical.sh f12e331eadfd27aaf2ddd02a8dca9153624fe832363cdc737b8056447f98547d
generate-pq-canonical.py e7b9b6ca1cdb8e9abda679cefa9b461336b035c4dc8ec72639a0fc6588d06e55
```

Source generations 1/2/3 each produced raw SHA-256 `7278abe3…c3a48f`, HEVC
`72eadd22…f2152a`, AV1 `526ddba6…22a71e`, and the full identity JSON hash stated
above. These abbreviations refer to the full exact values recorded earlier;
they are not independent identifiers. Encoded input sizes are 2666232 and
9236504 bytes. Actual nonzero low-two-bit ratio is 62.72193930041152%.

Final static commands were the normal and measurement release workspace builds,
normal and measurement workspace tests, strict normal/measurement/qualification
Clippy, format check, diff check, all fixture SHA256SUMS, negative oracle controls
and all generated SPIR-V validation. Normal and measurement workspace tests each
passed 192 tests; opt-in hardware tests were run separately, not inferred from
their ignored entries. No shader math, CPU PQ reference or Cargo.lock changed.

Future regression invocation checks the established baseline rather than only
checking repeatability within a new batch:

```bash
python3 tests/baselines/media/verify-pq-production.py /tmp/pq-retained /tmp/pq-regression.json \
  --input-directory tests/fixtures/codecs --check-baseline tests/baselines/media/pq-production-v1.json
```

Establishing a new baseline requires an explicit reviewed closure; normal
regression does not overwrite the committed golden record. Preserve exact input
generator/version/command/hash, output command and hashes or a measured structured
nondeterministic oracle. Historical H.264 `.102` versus `.103` Tier 1A failure and
the unrecoverable old ten-bit evidence remain historical limitations; no comparator
exception or new PQ hash is used to relabel them PASS.

## Final closure index (48 requested items)

| # | Item | Result |
| ---: | --- | --- |
| 1 | Production HDR eligibility | Explicit canonical PQ signal plus real complete-path capabilities; not SDR assessment or P010 identity |
| 2 | Planner | `ColorProcessing::HdrPqPreserve`, plan-owned canonical output descriptor |
| 3 | CLI | Existing explicit HEVC/AV1 depth-10 flags; default H.264/8-bit rejected |
| 4 | Auto backend | Four actual codec pairs chose only fully probed hardware path; failures terminal |
| 5 | Explicit backend | Vulkan qualified; CPU/software/disabled interop fail before staging |
| 6 | Decode policy | VAAPI production; software signal inspection/decode-back only |
| 7 | Encode policy | VAAPI Main10/Main 10-bit only; no software fallback |
| 8 | HEVC PQ input | Canonical full 300 frames passed |
| 9 | AV1 PQ input | Canonical full 300 frames passed |
| 10 | HEVC Main10 PQ output | Three strict retained runs, complete decode passed |
| 11 | AV1 10-bit PQ output | Three strict retained runs, complete decode passed |
| 12 | HEVC→AV1 | Actual 300-frame full interop conversion passed |
| 13 | AV1→HEVC | Actual 300-frame full interop conversion passed |
| 14 | P010 SDR/PQ dispatch | Explicit mode and backend descriptor guards; no math/shader changes |
| 15 | Output primaries | `bt2020` |
| 16 | Output transfer | `smpte2084` |
| 17 | Output matrix | `bt2020nc` |
| 18 | Output range | `tv` / limited |
| 19 | Container/coded/decoded metadata | All actual boundaries checked; left chroma and ten-bit profiles |
| 20 | Mastering-display policy | Not propagated/recomputed; no HDR10 mastering claim |
| 21 | MaxCLL/MaxFALL policy | Not propagated/recomputed |
| 22 | Static metadata leak test | Real legal source metadata present, both output streams/all frames absent |
| 23 | Canonical PQ fixture | Shared deterministic full-resolution 1080p/300/50fps true ten-bit source |
| 24 | Generator/version/hash | Checked-in scripts, pinned FFmpeg 8.1.3 and complete identities, three equal generations |
| 25 | Repeatability | Both outputs three identical packet/pixel/file results |
| 26 | Tier 1A/1B/1C | Strict packets/coded semantics/decoded pixels and timestamps all passed |
| 27 | Tier 2 | Actual container/frame structure, counts and continuous timestamps passed |
| 28 | Tier 3 | Both exact-build file hashes passed; not a cross-driver guarantee |
| 29 | Decode-back | Both 300-frame retained and 3000-frame stress outputs fully decode without errors |
| 30 | Pre-encode CPU/GPU | Both inputs all 300 frames; zero glyph differences, max code delta 1 |
| 31 | AAC | Auto/copy packet payload, timing and metadata passed |
| 32 | Dual audio | Both tracks, languages/default dispositions and timing retained |
| 33 | FreeType | Both output codecs passed; geometry/color unchanged |
| 34 | Cancellation | SIGINT exit 130, both output codecs, fresh followup passed |
| 35 | Safe output | Existing destination retained and staging removed on failure/cancellation |
| 36 | Failure injection | PQ init/map/render, interop, native encode, mux header/packets/trailer/file write passed |
| 37 | HEVC 3000-frame stress | Submitted/encoded/muxed/decoded 3000; correct CFR/color/profile |
| 38 | AV1 3000-frame stress | Submitted/encoded/muxed/decoded 3000; correct CFR/color/profile |
| 39 | FD lifecycle | Preencode 4/22/4; both long production runs 4/25/30/4; fault cleanup exact |
| 40 | Vulkan Validation | Zero errors with Khronos synchronization validation on hardware gates |
| 41 | Production performance | Dedicated three-run measurements and full overlapping-stage boundaries retained |
| 42 | SDR baseline v2 | Five profiles ×3 strict original hashes unchanged on final build |
| 43 | HLG/BT.2020 SDR/full-range | Existing real-fixture early-rejection tests passed; no admission broadening |
| 44 | SPIR-V | All 317 generated cached modules valid for Vulkan 1.3 |
| 45 | Build/test/clippy/fmt/diff | Normal/measurement builds/tests/Clippy, qualification Clippy, fmt/diff passed |
| 46 | Qualification scope | Above actual Intel device, iHD/ANV and pinned toolchain only |
| 47 | Limitations | No tone/gamut mapping, HLG/full-range/software HDR, universal driver or HDR10 mastering claim; fault checkpoints not device-loss proof |
| 48 | Stage 5.3B-3 | **SEALED**, no remaining required blocker |

Stage 5.3C HDR→SDR tone mapping can now be considered.
No Stage 5.3C implementation was started.
