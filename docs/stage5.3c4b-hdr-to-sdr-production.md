# Stage 5.3C-4B — HDR→SDR production integration

Status: **SEALED**, 2026-09-30, on the recorded Intel hardware/toolchain scope.
Stage 5.3C overall is **SEALED**. No next-stage feature work is started.
The [closure ledger](../tests/baselines/media/stage53c4b.json),
[encoded-output baseline](../tests/baselines/media/hdr-to-sdr-production-v1.json)
and [observed system output](../tests/baselines/media/c4b-system-evidence.txt)
separate production, reference, historical and measurement evidence.

## Contract and implementation

`--output-dynamic-range preserve|sdr` is explicit and defaults to `preserve`.
There is no `auto` dynamic-range policy, filename inference or automatic SDR
fallback. Existing SDR plus `sdr` is a semantic no-op and retains the SDR
renderer. PQ plus the default H.2648 output still rejects; the error describes
HDR-preserving ten-bit output and the explicit SDR alternative.

The planner represents conversion as `ColorProcessing::HdrPqToSdrBt709`, not
SDR decode with relabelled tags. Its nodes are VAAPI P010 decode/input interop,
Vulkan HDR map/linear ASCII, sealed BT.2446-1 Method A, sealed BT.2020→BT.709
target-volume limiting, sealed NV12/P010 signal packing, output interop and
VAAPI encoding. `--explain-plan` displays these operations; `--capabilities`
reports a separate actual conversion capability. A codec/profile/format fact
requires opening the actual encoder and importing the selected writable
surface. The conversion probe executes the resident color/pack/output path.
No vendor-name allowlist substitutes for successful probing.

Both hardware interop directions are mandatory. CPU, software decode/encode,
diagnostic CPU Vulkan mapping and staging are not qualified alternatives, even
under automatic selection. Initialization and runtime failures are terminal;
no midstream color, codec, depth or backend fallback is introduced.

Production consumes resident HDR RGB and packed output buffers. Only mandatory
diagnostic counters and timestamp queries reach the host; full RGB, packed
pixels and cells are not downloaded or re-uploaded. Two existing bounded slots
retain source/output VAAPI owners through completion. Unknown GPU completion
quarantines actual owners, not just duplicated FDs, to prevent surface reuse.
Healthy and known-complete failure paths release their mappings normally.

### Actual source-domain rejection

The fixed Method A policy accepts resolved left-sited limited BT.2020 NCL/PQ
HEVC Main10 or AV1 Main 10-bit 4:2:0 with actual source channels in 0–1000 cd/m².
Mastering metadata and MaxCLL are neither proof of the peak nor a clamp policy.
Every source pixel is checked before cell averaging and glyph coverage. A
small illegal highlight cannot be diluted or hidden by a black/space glyph.
Encoded RGB excursions, nonfinite arithmetic and above-domain EOTF samples
hard-fail before output packing; they never produce a successful black fallback.

This diagnostic is a separate `HDR_TO_SDR_SOURCE_DOMAIN` shader variant. The
preserve shader remains byte-identical across old/new cached builds, SHA-256
`28a7c5025b26e710539bcabbd96ae155d5b937d7342281f347b9e377a5dc3f76`.
The sealed reference mathematics and qualification behavior are unchanged.
The production constructor refuses a backend lacking the source guard.
An actual late-frame lossless 2×2 10000-nit highlight test passes the identical
first-ten-frame legal black control, then fails the full conversion for both
output depths, preserving the destination sentinel and removing staging.

## Production matrix and retained identity

Both legal inputs are 1920×1080, 300 frames, 50 fps, ten-bit 4:2:0 BT.2020/PQ
limited/left. They are the fixed FFmpeg 8.1.3 C3 legal-domain fixtures, not the
unchanged B-3 above-1000-nit source:

| Input | SHA-256 |
| --- | --- |
| `hevc-main10-pq-c3-legal-v1.mp4` | `eb8d8c42cd99369b99031900e8861c08f611bfcc662b8d0eb741eb695a0c366a` |
| `av1-main10-pq-c3-legal-v1.mp4` | `7eb4fd7e7513fc181ad8489e13edbac10a88a8f3a7947d2f958d36a0ce08e7c4` |

The checked-in [generator](../tests/baselines/media/generate-hdr-to-sdr-production-v1.sh)
fixes every production argument. For each input and each output below, three
runs use the following exact argument sequence, substituting only paths,
codec and depth as recorded in the per-run evidence:

```sh
target/release/asciiflow INPUT OUTPUT \
  --width 80 --charset standard --font builtin-8x8 --color true \
  --audio none --max-frames 300 --decode vaapi --backend vulkan \
  --vulkan-mapping gpu --encode vaapi --hw-device /dev/dri/renderD128 \
  --vaapi-vulkan-input-interop on --vaapi-vulkan-output-interop on \
  --output-dynamic-range sdr --output-codec CODEC --output-bit-depth DEPTH --no-progress
```

| Output target | Interop / decoded format | Profile | Three-run output SHA-256, same for both lossless inputs |
| --- | --- | --- | --- |
| H.2648 | NV12 / yuv420p | High | `866cd722e7969994c91e2d0647abd82a1b6be5f29929205baa25040742feda12` |
| HEVC8 | NV12 / yuv420p | Main | `12213439b789a482504592b9c703a2f78afeef2795dcc3f16a0f5c3764a99c20` |
| AV18 | NV12 / yuv420p | Main / Profile0 | `284723f0fe4ec1afd632b4a64773f9d85ee56c4c37d2b5260b3dae4c170d2176` |
| HEVC10 SDR | P010 / yuv420p10le | Main 10 | `2e8d7e8e48390b42b875f47bca756521d1a63489651d81ddb633ad7c50f0e151` |
| AV110 SDR | P010 / yuv420p10le | Main / Profile0 | `739193a9553b81fa8ceb66f730b944ed66a74180bf7d58de7db8bdb956f17894` |

The final complete matrix passed full decode and all five repeatability tiers.
It binds the final checkout, including added test-only native-encoder coverage;
no earlier command record was rewritten to claim that later source identity.
Earlier successful matrices remain preliminary evidence in their separate
temporary directories. The ordinary release binary remains
`2a2c15921f35275578e63c0bbeedb84cfd6145b16d6dd6575a6f10f62c4eb1ce`.

The output oracle checks all container and elementary stream/frame metadata:
BT.709 primaries, BT.709 transfer, BT.709 matrix, limited (`tv`) range, left
chroma, correct profile/depth/geometry, 300 packets and decoded frames, PTS/DTS
0..299 at 1/50 seconds and one-frame packet durations. Static mastering/CLL,
HDR10+ and Dolby Vision side data are forbidden. Fresh output AVFrames set
native SDR metadata and do not clone input properties. There is no synthetic
HDR mastering metadata on SDR output. Native decode-back errors are fatal.

Tier 1A is raw packet identity; 1B is coded signal/stream identity; 1C is full
decoded frame hashes and timestamps; Tier 2 is structure/frame metadata;
Tier 3 is deterministic whole-file identity with exact build scope. Tier 3
must not silently disappear when the build or machine changes. Output pixels
after lossy encode are never asserted equal to the pre-encode CPU oracle.

## Audio, rendering and safe output

Five opt-in CLI system tests passed in 208.48 seconds. The audio matrix covers
both input codecs, single/dual AAC, H.2648 SDR, HEVC10 SDR and matching-codec
PQ preserve, each with `auto/copy/none`, all 300 frames. Native packet payloads,
PTS/DTS/duration, language, default disposition and stream order are exact;
audio also decodes completely. `none` removes audio. Preserve and SDR copied
audio match the same source. Built-in/FreeType × color/mono × NV12/P010 use
automatic hardware selection; these rendering checks are short smokes, not
additional 300-frame retained baselines.

Both NV12 and P010 SIGINT cases enter actual video/AAC packet writing, exit
130, preserve an existing output file, remove staging and reap the process;
28 live descriptors disappear with the process. This is process-lifetime
evidence, separate from the in-process leak checks below. Both final-commit
failures complete encoding, then fail atomic replacement of a nonempty
destination directory; the directory sentinel survives and staging is removed.
Forced CPU/software paths and H.26410 reject before staging.

## Fault and resource evidence

Real constructor coverage exercises seven C3/pack allocation, descriptor and
pipeline faults × both slots × both output formats: **28 faults**, each with
a healthy resident conversion afterward. Slot-1 failure proves rollback after
slot 0 is fully prepared. Exact root errors, baseline FD return and validation
through device teardown pass. This does not simulate native device loss.

All five actual SDR VAAPI encoders exercise native send, receive, drain-receive,
MP4 header, early packet, midstream packet and trailer failures. Root stages
and messages are exact; FDs return to the warmed baseline after destruction.
Existing Main10/AV110 AAC-write failures are rerun against a fixed generated
BT.709 P010/AAC input; the generic eight-bit AAC fault is covered by workspace
tests. Existing C4A actual external-import/copy/dispatch/fence-complete fault
checks and retries are rerun unchanged, not represented as native hung-fence
proof. CLI initialization tests cover all five conversion targets, all existing
initialization checkpoints and exactly one terminal attempt under `auto`.

Two full production stresses use fixed FFmpeg 8.1.3 `-stream_loop 9`, video
stream-copy and an explicit 3000-frame cap. Their exact fixture commands,
input/output/test-binary hashes, resource samples and complete decode-back are
retained separately from the 300-frame output baseline:

| Path | Frames encoded / decoded | FD before / sampled peak / after | RSS before / sampled peak / after, KiB | Validation through teardown |
| --- | --- | --- | --- | --- |
| HEVC PQ → H.2648 NV12 | 3000 / 3000 | 4 / 31 / 4 | 42788 / 132980 / 104012 | 0 |
| AV1 PQ → HEVC10 P010 | 3000 / 3000 | 4 / 31 / 4 | 108800 / 178108 / 118540 | 0 |

Both pipeline hardware-upload/download counters are zero. The resource sampler
is 25 ms, so peaks are sampled rather than exhaustive. Post-warmup RSS stays
near 133/178 MB with small allocator/page-touch drift; retained driver/allocator
caches mean RSS is not required to equal cold startup after teardown. Two
resident slots, fixed geometry-dependent RGB/pack buffers, bounded decode/mux
channels and RAII packet/mux owners prevent frame-count-dependent accumulation.
The audio stress/cancel and exact-packet tests exercise the separate bounded
audio queue. Native stalled-device recovery and arbitrary-duration leak freedom
are not claimed by a 3000-frame run.

## Retained regressions and historical limitations

Fresh three-run H.2648/HEVC8/AV18/HEVC10/AV110 SDR file identities match
`post-polarity-v2.json`; both PQ-preserving three-run identities and all output
oracle tiers match `pq-production-v1.json`. The permanent C-1 and C-2B each
retain three exact output hashes. Fresh C3 canonical P2/N3, C4A boundary,
full canonical pack and actual surface/parity/fault tests pass. Old two-code
UNORM16 failures and earlier C4A arithmetic failures remain failed historical
diagnostics, not PASS.

The preserved historical H.264 comparison uses its original retained input and
explicit legacy charset, not the post-polarity input. The freshly regenerated
candidate is byte-identical to the earlier candidate, SHA-256
`e45071c0c350deae063c8bb10f7e433ee764a107aff01294db391e7f56bf24af`.
Historical Tier 1A remains **FAIL**, 1B/1C/2 remain PASS; the only allowed coded
difference remains the pinned encoder SEI patch token `.102`→`.103`. No
comparator tolerance changes. The new HDR→SDR H.264 baseline has strict
three-run raw packet identity instead.

## Dedicated performance and static closure

Dedicated three-run measurements use the fixed legal HEVC source for five
outputs and the AV1 source for one representative ten-bit output. Validation
is off, with no competing GPU/oracle jobs. All eighteen measured outputs are
byte-identical to their corresponding ordinary-build golden output. The
measured binary SHA-256 is
`cf0d9dca709a154b983c4f5c2f3447d1d92f42926c21c5179ed3902014cd4cbe`.
[Exact commands, scopes and individual runs](../tests/baselines/media/c4b-performance.json)
are reproducible with the checked-in measurement runner and summarizer.

| Input → output | Mean pipeline FPS | Mean whole-process FPS | CPU % | GPU map / combined color+pack / output copy, ms/frame | Encode submit/receive / mux video write, ms/frame |
| --- | --- | --- | --- | --- | --- |
| HEVC → H.2648 | 161.50 | 101.43 | 45.67 | 0.266 / 4.679 / 0.108 | 3.210 / 0.018 |
| HEVC → HEVC8 | 160.29 | 104.34 | 44.33 | 0.265 / 4.707 / 0.107 | 1.216 / 0.013 |
| HEVC → AV18 | 181.26 | 113.78 | 47.00 | 0.266 / 4.328 / 0.107 | 0.647 / 0.014 |
| HEVC → HEVC10 SDR | 175.84 | 109.98 | 47.67 | 0.263 / 4.333 / 0.151 | 0.849 / 0.013 |
| HEVC → AV110 SDR | 189.54 | 116.43 | 51.33 | 0.232 / 3.973 / 0.151 | 0.901 / 0.018 |
| AV1 → HEVC10 SDR | 191.20 | 118.12 | 49.33 | 0.229 / 3.963 / 0.146 | 0.453 / 0.015 |

Whole-process FPS is the mean of 300 divided by each observed process wall
duration, including capability probing/setup/teardown. Pipeline FPS is the
CLI's narrower overlapping pipeline interval. GNU time has two-decimal wall
precision. These are local measurements, not a generalized throughput promise.
The existing production metrics expose decode, interop DRM/surface wall scopes,
HDR-map GPU timestamps, aggregate linear-render + Method A + limiter + pack
GPU timestamps, output-copy GPU timestamps and encode CPU wall. The optional
`asciiflow-cli/encode-characterization` build exposes encoder/mux subscopes.
Individual color-pass sums are not currently exported by the production
metrics. Uninstrumented CPU/import/wait zero fields mean unavailable, not zero
work. Overlapping CPU/GPU times are not additive; whole-process wall FPS and
CPU percentage must be measured separately. Cached host-reference/readback
C3/C4A figures cannot establish a production percentage speedup.

Default and encode-characterization release builds, workspace tests and strict
all-target Clippy passed, as did qualification-feature Clippy, format/diff,
shell syntax and ten output-oracle negative controls. The final census validated
all **964** cached `.spv` files against Vulkan 1.3; old C4A's 740 is a historical cache
count, not a fixed expected number. C4B adds one ordinary source-domain shader
variant; repeated feature/build configurations add cache copies. There is no
shader fusion, LUT replacement, FP16, numerical tolerance relaxation or new
color algorithm.

## Scope and remaining gates

Observed hardware: Intel Arc Meteor Lake `8086:7d55`, `/dev/dri/renderD128`,
Intel iHD 26.1.5-1.fc44, Mesa ANV 26.2.3-1.fc44, Vulkan API 1.4.354;
FFmpeg 8.1.3-1.fc44 / libavcodec 62.28.103, Rust 1.97.1 and SPIRV-Tools
2026.1-1.fc44. Complete build/package/kernel/source identities belong to the
machine record, not a universal capability assertion.

HLG, full-range HDR, unknown/conflicting metadata, SDR-transfer BT.2020/P3,
above-1000-nit/generalized tone mapping, HDR software production and optimal
perceptual gamut mapping remain unsupported. SDR output can have target-volume
clipping under the sealed C-2B policy; this is not Annex5 or luminance-preserving
gamut mapping. The existing CFR audio timeline restriction is unchanged.

There are no remaining C-4B closure blockers. Independent review found two
oracle gaps (chroma-location scope and recoverable decode errors); both were
fixed and the final thirty outputs were fully reverified. Known limitations
above remain explicit. The next decision is an architecture/priority review,
not automatic HLG, generalized tone-map, perceptual mapper or other feature work.

## 63-item closure ledger

| # | Requirement | Observed result / scope |
| --- | --- | --- |
| 1 | CLI | Explicit `preserve|sdr`; parser rejects `auto`. |
| 2 | Default | Preserve; PQ/default H.2648 rejects before staging. |
| 3 | Explicit SDR | Conversion only for qualified PQ; existing SDR stays its existing renderer. |
| 4 | Planner | `HdrPqToSdrBt709`, explicit linear/method/limiter/pack nodes. |
| 5 | Capabilities | Actual first-frame execution, encoder opening and output import per target. |
| 6 | Decode | VAAPI P010 input required; software request rejects. |
| 7 | Backend | Vulkan GPU required; CPU/reference route rejects. |
| 8 | Encode | Actual VAAPI target required; software request rejects. |
| 9 | Legal matrix | Both PQ input codecs × five SDR output targets. |
| 10 | Illegal matrix | H.26410, HLG, full/wide/unknown/conflicting, staged and unavailable facts reject. |
| 11 | Explain plan | Actual CLI diagnostic displays explicit conversion and fixed domains. |
| 12 | Capabilities display | Actual CLI displays separate HDR→SDR execution capability. |
| 13 | HEVC → H.2648 | Three complete 300-frame runs; all five tiers pass. |
| 14 | HEVC → HEVC8 | Same. |
| 15 | HEVC → AV18 | Same. |
| 16 | HEVC → HEVC10 SDR | Same. |
| 17 | HEVC → AV110 SDR | Same. |
| 18 | AV1 → H.2648 | Same. |
| 19 | AV1 → HEVC8 | Same. |
| 20 | AV1 → AV18 | Same. |
| 21 | AV1 → HEVC10 SDR | Same. |
| 22 | AV1 → AV110 SDR | Same. |
| 23 | Bit depth | Actual decoded 8/10-bit, not codec-name inference. |
| 24 | Pixel format | NV12/P010 output surfaces; decoded yuv420p/yuv420p10le. |
| 25 | Primaries | BT.709 in container, elementary stream and every decoded frame. |
| 26 | Transfer | BT.709 in all signal layers. |
| 27 | Matrix | BT.709 NCL in all signal layers. |
| 28 | Range | Limited (`tv`) in all signal layers; left chroma checked throughout. |
| 29 | Coded/container agreement | Actual extracted bitstreams and full coded decode checked. |
| 30 | HDR metadata | No static mastering/CLL, HDR10+ or Dolby Vision side-data leak. |
| 31 | Decode-back | Full framehash with `-xerror`, all thirty outputs, no decode error. |
| 32 | Repeatability | Three files per case; identities equal. |
| 33 | Tier 1A | Raw packet hashes/PTS/DTS/duration equal. |
| 34 | Tier 1B | Coded signal and stream identities equal. |
| 35 | Tier 1C | All decoded frame hashes/timestamps equal. |
| 36 | Tier 2 | Structure and frame metadata equal. |
| 37 | Tier 3 | All whole-file hashes equal, bound to exact source/binary/machine scope. |
| 38 | AAC | Auto/copy/none, both inputs and output depths, payload/timeline exact. |
| 39 | Dual audio | Both tracks, order/language/default disposition exact. |
| 40 | Built-in | Actual automatic-path NV12/P010 rendering smokes. |
| 41 | FreeType | Same; no font implementation rewrite. |
| 42 | Color/mono | Both rendering modes and output depths. |
| 43 | Cancellation | Both formats during real AAC/video writing: exit130 and reaped process. |
| 44 | Safe output | Existing file/directory sentinels unchanged; staging removed. |
| 45 | Fault injection | 28 constructors, five native encoder/mux targets, audio/root-stage/commit and reused surface fault coverage. |
| 46 | NV12 stress | HEVC→H.264, actual 3000-frame full pipeline and decode. |
| 47 | P010 stress | AV1→HEVC10, actual 3000-frame full pipeline and decode. |
| 48 | FD | Both 4→31→4, in-process; constructor faults recover baseline. |
| 49 | Resources | Fixed two-slot buffers, bounded channels, sampled working-set plateau and complete teardown; no universal leak claim. |
| 50 | Vulkan Validation | Zero in matrix, constructors, reference/surface tests and both stresses through teardown. |
| 51 | Performance | Five outputs plus cross-input representative, three dedicated runs each. |
| 52 | Breakdown | Measured scopes above; unavailable individual color/import/wait subscopes are labelled, not fabricated. |
| 53 | SDR regressions | Fresh fifteen encodes match all five retained post-polarity hashes. |
| 54 | PQ preserve | Fresh six encodes match both retained hashes and all output tiers. |
| 55 | C-1/C-2B/C-3/C-4A | Three CPU hashes each, P2/N3, 8573 boundary vectors, complete canonical packing and sixteen surface cases pass. |
| 56 | Historical H.264 | 1A FAIL unchanged; 1B/1C/2 PASS unchanged; no comparator relaxation. |
| 57 | SPIR-V | All964 cached modules pass Vulkan1.3; one new ordinary guard variant. |
| 58 | Static | Both release builds/tests/Clippy, qualification Clippy, fmt/diff/shell/oracle controls pass. |
| 59 | Scope | Intel MTL/iHD/ANV and complete native/source identities recorded. |
| 60 | ≤1000 nit | Per-source-pixel rejection before averaging/coverage, including late tiny highlight control. |
| 61 | Limits | HLG/full/generalized HDR/software production/perceptual gamut quality remain unsupported. |
| 62 | C-4B | **SEALED**. |
| 63 | Overall C | **SEALED**; no next feature started. |
