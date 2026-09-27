# Stage 5.3B-1 — PQ pixel semantics and CPU reference

The qualification code is `asciiflow-core::hdr_pq` plus the separate
`asciiflow-cpu::hdr::HdrPqReference`. The existing SDR mapper, renderer,
production planner, media decoder, Vulkan shaders, interop, and encoder remain
unchanged. Production HDR remains rejected; this is an independently callable
internal CPU oracle for a future GPU implementation.

The oracle decodes limited BT.2020 NCL P010 into PQ RGB, then f64 absolute
linear RGB. It averages each cell in linear light, chooses a glyph from
inverse-PQ linear luminance using the existing glyph LUT, and blends R8
coverage in linear light against 0-nit black. Output is PQ/BT.2020 NCL
limited P010 with ten-bit quantization and zero padding bits. Source static
HDR metadata is not propagated. See [pixel contract](hdr-pq-semantics.md).

Qualification tests include pinned 0/100/1000/10000-nit PQ vectors, dense PQ
round trips, BT.2020 NCL matrix round trips (<1e-14 per component in the
floating-point test), legal-range and half-way quantization, black and neutral
chroma, 100/1000/10000-nit gray, color reconstruction, linear averaging and
50% glyph blending, monotone glyph selection, ten-bit low-bit differences,
built-in and FreeType atlases, deterministic repeat output, and scope
rejection. An uneven 6×6→4×4 grid test also pins identical cell boundaries
between averaging and rendering. In-memory raw P010 signals are deterministic and require no codec;
the existing PQ codec fixtures remain metadata/rejection fixtures, not
calibrated pixel references.

The path deliberately does not solve unknown/center chroma siting, extended
signal ranges, accurate mastering metadata propagation, HLG, tone mapping,
gamut mapping, HDR encode, or Vulkan f32 error bounds. A synthetic 16×16
FreeType frame logs a time-per-frame sanity value in its test; this is not
a throughput benchmark or production FPS claim.

## Verification on 2026-09-27

The checked-in canonical input SHA-256 was unchanged:
`df69c98ca6592b08c6bc34127b2bb5cf4125c759fd852b830e5426d5818fccda`.
One Release full-GPU 1920×1080/300-frame run per 10-bit codec on Intel Arc
Meteor Lake (VAAPI decode, P010 input interop, Vulkan processing, P010 output
interop, VAAPI encode) reproduced the Stage 5.3A canonical output hashes:

| Codec | Current and canonical v1 SHA-256 |
| --- | --- |
| HEVC Main10 | `07f231ab49eebd54cc0e85012f7dd12e424f580d17005bc20028b458168323fe` |
| AV1 10-bit | `2ab82984f690ea3fe5472c874dfce7cc474f7c46266f5cd86ed8deee20b36fda` |

The three retained eight-bit outputs were also regenerated from input SHA-256
`6b47b510c4a8f604e6404b1fbbc5f68bdaa77043976acdad0153ef83524b6e34`
on the same full-GPU route. Their *strict MP4 byte hashes did not match* the
historical values:

| Codec | Current SHA-256 | Stage 5.3A retained SHA-256 |
| --- | --- | --- |
| H.264 | `e45071c0c350deae063c8bb10f7e433ee764a107aff01294db391e7f56bf24af` | `3181fa9775403a2f30e60157797adf370927b4196c960f59d1b5084203d42bef` |
| HEVC Main8 | `d13ee29c54e9cf1d61a305c88fb00a6ba01ddf7fb9a1e955a7d9f1d9f8bc38e3` | `d4b13786c968e43a502845df46d92dc2dbc951e5e2ef69e4313d5c541664208b` |
| AV1 Main8 | `363c8630c440b4d86baa3da5eef80dcfe0261d76a7996aef2f0bd86dd19f79f1` | `f044a327c093b8489e03f20c98deab8f80e99f01092fe35595b4f22796c7c534` |

Byte comparison with the retained files shows identical length and only
FFmpeg version-character differences. **The earlier inference that all three
compressed media payloads were identical was wrong:** HEVC/AV1 differ only in
the container `Lavf…102→…103` tag, but H.264 also has `Lavc…102→…103` in an
SEI NAL within its first video packet. The system is now FFmpeg/ffmpeg-libs
8.1.3. Whole-container hashes are genuinely **not PASS**; the previous hashes
are kept, not rewritten. No HDR change to the SDR production pixel path is
inferred from these toolchain-version bytes.

`cargo build --release --workspace`, normal and `encode-characterization`
workspace tests, both strict all-target Clippy variants, format and diff
checks passed. All 133 generated SPIR-V modules passed `spirv-val
--target-env vulkan1.3`. The Intel VAAPI software/driver rejection test passed
for PQ, HLG, BT.2020 SDR and full-range; the normal CLI test confirmed
unsupported color fails before target staging. No Vulkan shader, interop,
encoder or production planner file changed.

## Structured media regression closure pass

This pass corrects the **regression methodology**, not media output behavior.
The test-only [FFmpeg/AsciiFlow comparator](../crates/asciiflow-media/tests/common/media_regression.rs)
reads native stream/packet facts and software-decodes every visible video
frame. It compares per-stream exact packet payload/PTS/DTS/duration/flags and
side data; decoded PTS, dimensions, format and visible-plane SHA-256; video
codec/profile/bit depth/color; and audio payload/routing/language/disposition.
It separately compares stream order, time bases, duration, codec parameters,
extradata and nonvolatile tags. Cross-stream packet interleave is a reported
structural difference, not a packet payload difference. The only permitted
tool-version tags are exact-pattern `format.tags.encoder=LavfM.m.p` and
`stream.tags.encoder=LavcM.m.p`, where major/minor remain unchanged. Changes
are reported with both values; no packet bytes or other tags are allowlisted.
An additional byte-scope guard rejects any whole-file difference outside
approved equal-length version-tag values after native comparisons have passed;
it is not a substitute for packet/frame inspection.
Whole-file SHA remains the exact-toolchain Tier 3 gate. See the
[three-tier policy](testing.md#media-regression-policy) and
[machine-readable five-case record](../tests/baselines/media/stage53b1.json).
This run records whole-file byte equality separately: the comparator does not
attest exact build identity, and historical 8-bit reference binary/driver
checksums were not retained. The record pins the current Release binary SHA
and FFmpeg package; its `/tmp` candidate paths are local execution evidence.

All five 1920×1080/50 fps, 300-frame BT.709 limited candidates were freshly
generated on Intel Arc Meteor Lake using the same Release full-GPU VAAPI ↔
Vulkan path and fixed CLI flags recorded in the baseline record. Each has one
video stream, 300 video packets, 300 decoded frames and six seconds of video;
there is no audio in these five outputs. Reference and candidate files were
then compared **without GPU access**:

| Case | Tier 1 media | Tier 2 structure | Whole MP4 SHA bytes | Approved difference / unexpected difference |
| --- | --- | --- | --- | --- |
| H.264 8-bit | **FAIL** | PASS | different | `format.tags.encoder` approved; packet 0 payload unexpected |
| HEVC Main8 | PASS | PASS | different | only `format.tags.encoder: Lavf62.12.102 → Lavf62.12.103` |
| AV1 Main8 | PASS | PASS | different | only `format.tags.encoder: Lavf62.12.102 → Lavf62.12.103` |
| HEVC Main10 | PASS | PASS | matches canonical v1 hash | none |
| AV1 10-bit | PASS | PASS | matches canonical v1 hash | none |

The strict H.264 first-packet SHA-256 changed from
`ee5b429bda36bd1e8cebd3ba53ecca8af31b40e80e7ac02f908136153252a8fa`
to `75cf3281630f143ec38d1bd0ef5feaf8202b687e7fd7658a32bb15a000f55b41`.
Inspection of the MP4 byte boundary places the differing `Lavc` character
inside the first packet's H.264 SEI NAL, not a format or stream tag. Packet
count, all other packet bytes, timestamps, decoded frame digests, color and
stream structure matched. The user explicitly chose to keep packet comparison
strict, so this is a Tier 1 failure even though decoded video is unchanged.
No SEI exception or `.102` runtime rollback was introduced.

Unit tests prove one-byte payload changes, PTS changes, transfer/profile
changes, decoded frame changes, unapproved encoder-tag changes, and real audio
payload/language/disposition changes fail. An allowed `Lavf` patch tag change
passes with an approved-difference record. A changed cross-stream interleave
is reported as structural, not media payload, difference. These tests use
small checked-in files; the five 300-frame comparisons are opt-in because
software decode is relatively slow, not because they need hardware.

The closure source tree passed Release workspace build; normal and
`asciiflow-cli/encode-characterization` workspace tests; both all-targets
strict Clippy variants; format/diff checks; valid five-case JSON; and
`spirv-val --target-env vulkan1.3` on all 133 current generated modules.
The existing production PQ/HLG/BT.2020 SDR/full-range rejection tests remain
green in the workspace, and the prior Intel VAAPI color-rejection evidence
remains applicable because no decoder, planner, interop, shader or encoder
source changed. This pass changed only test-side oracle files, the baseline
record and documentation; the prior uncommitted PQ CPU reference is preserved.

**Earlier strict-packet checkpoint: NOT SEALED.** The PQ CPU reference gates and four of five structured
media regressions are green. H.264's first compressed packet is not byte-equal
under the explicitly retained Tier 1 rule. Stage 5.3B-2 is therefore **not
formally justified**; this pass stops without Vulkan PQ, HLG, tone mapping,
HDR production output or changes to HDR pixel math.

Subsequent default-ramp polarity correction changes rendered pixels by design.
The five hashes and comparisons above remain a pre-correction snapshot, not
current-output gates; no historical hash was rewritten. See
[the current regression policy](testing.md#media-regression-policy).

## H.264 coded-bitstream forensic closure (pre-polarity artifacts)

The retained first H.264 video packets are both 597,001 bytes. Their AVCC
four-byte-length NAL sequence has the same count, order and lengths:

| NAL index | Type | Bytes | Reference SHA-256 | Candidate SHA-256 |
| --- | --- | ---: | --- | --- |
| 0 | 7 SPS | 30 | `bb44950a5699cbecf46ed08ae32f91fd7bc93d4bc44a49036a067a9facd8b550` | identical |
| 1 | 8 PPS | 5 | `402b01292bfbde28cda0e0becb663f7215a4961a4d3a472e58de450212c412bf` | identical |
| 2 | 6 SEI | 106 | `f7fe9a41e8873738b2bf76468027b97ec095700295f5224729f2493f35499c92` | `469b532ed650a07fc5036e7f83531e7e6e156f014248e389c99dde2dfb3492b2` |
| 3 | 5 IDR | 596,844 | `1699d8a58f20c47a34ba197a3c864d88a9421f60e069ed9085adc1012fbdc55d` | identical |

The SEI RBSP has exactly one message: payload type 5
(`user_data_unregistered`), payload size 102, UUID
`59948b2811ec45af967519d41feaa94d`. Its null-terminated identifier is:

```text
reference: Lavc62.28.102 / VAAPI 1.23.0 / Intel iHD driver for Intel(R) Gen Graphics - 26.1.5 ()
candidate: Lavc62.28.103 / VAAPI 1.23.0 / Intel iHD driver for Intel(R) Gen Graphics - 26.1.5 ()
```

Exactly one first-packet byte changes (offset 78, zero-based): ASCII `2` to
`3` in the `Lavc` patch version. SPS, PPS, VCL and all other packet bytes are
identical; the sequence, UUID, payload type/size and identifier suffix match.
The [test-only parser](../crates/asciiflow-media/tests/common/h264_bitstream.rs)
approves this one fixed UUID/VAAPI identifier role after parsing AVCC NALs and
SEI messages, not by searching for a string. Unknown/other SEI, SPS, PPS,
VCL, changed UUID or changed identifier content fail. The raw mismatch stays
visible. It also requires all raw SEI bytes outside the unique approved
version token to match, so an alternate EBSP encoding cannot hide another
change. AsciiFlow configures `h264_vaapi` with CQP/`qp=20`/`async_depth=2`
and does not set an identifier or SEI option; the observed text is generated
on the FFmpeg/VAAPI encoder path. This source audit does not distinguish
whether the FFmpeg wrapper or the driver inserted the message.

The new comparator reran all five *preserved pre-polarity* candidates:

| Case | Tier 1A raw | Tier 1B coded | Tier 1C decoded | Tier 2 | Tier 3 bytes |
| --- | --- | --- | --- | --- | --- |
| H.264 8-bit | FAIL | PASS | PASS | PASS | different |
| HEVC Main8 | PASS | PASS | PASS | PASS | different |
| AV1 Main8 | PASS | PASS | PASS | PASS | different |
| HEVC Main10 | PASS | PASS | PASS | PASS | match |
| AV1 10-bit | PASS | PASS | PASS | PASS | match |

The H.264 approved difference is reported with packet/NAL/SEI indices, UUID
and full identifiers; its `Lavf` container tag difference is separately
reported. Synthetic tests establish positive version-only approval and hard
failure for changed SPS, PPS, IDR slice, unknown SEI, UUID and non-version
payload content. Existing packet timestamp, color, decoded-frame, audio and
container negative tests remain in force. HEVC, AV1 and audio retain exact
packet bytes. Exact-build runs can opt into raw-packet and whole-file identity
gates using `ASCIIFLOW_REGRESSION_EXACT_BUILD=attested`; matching version
strings alone do not attest an exact build.

**Current-source status: NOT SEALED.** The forensic H.264 blocker is resolved
for those historical files, but the later default-ramp polarity correction
changes production pixels. Those five outputs do not qualify the current
source tree, and `/dev/dri` was unavailable for a fresh Intel full-GPU run.
No pre-correction hash was rewritten or promoted as a current gate. Stage
5.3B-2 remains unjustified until the current outputs are requalified.

## Final Intel hardware closure — post-polarity baseline v2 (2026-09-27)

**Stage 5.3B-1: SEALED.** This later qualification supersedes the
"current-source NOT SEALED" checkpoint above, without changing its historical
results. No Stage 5.3B-2 pixel or Vulkan HDR work was started.

The current source is `fffed5a8c446a87dc3595c479837d1b5b9056621`
plus test/oracle/documentation changes shown by `git status --short`;
`git diff --check` was clean. The pre-run tracked binary diff SHA-256 was
`57b159b96c9018adfb1e129ed5857365dc413de910e6a1c70dd63adabe6be74b`;
the two newly added generator scripts are separately pinned in the
[v2 manifest](../tests/baselines/media/post-polarity-v2.json). No production
Rust source changed after the baseline binary was built. `Cargo.lock` SHA-256
is `f8cf97f855fcbcb422ef1b74f2367a7ac40b612dcc41cc0a1d8a23f3e27746b8`;
the qualified Release binary SHA-256 is
`8340679c08ce627dbe1b27fae7577f24f455f7ab42eaed002856970b29cc7bda`.
The host-accessible `/dev/dri/renderD128` used Intel Arc Meteor Lake, Intel
iHD `26.1.5` / VAAPI `1.23`, Mesa Vulkan `26.2.3`, and Fedora FFmpeg/FFmpeg
libraries `8.1.3-1.fc44` (libavcodec `62.28.103`, libavformat `62.12.103`).
The normal project sandbox did not expose the render node; the device-backed
checks ran in the approved host-accessible execution context against this
*same* checkout, lockfile, binary and fixtures. No temporary source copy was
made.

`HEAD` introduced the intentional polarity correction in
[`config.rs`](../crates/asciiflow-core/src/config.rs) and the `standard`/
`detailed` CLI ramp selection in
[`args.rs`](../apps/asciiflow-cli/src/args.rs): the old default
`@%#*+=-:. ` (dense to sparse) became ` .:-=+*#%@` (sparse to dense)
for glyphs rendered on black. **v1** is therefore historical *pre-polarity*
evidence, including all five retained hashes and the `.102`/`.103` H.264
forensic case; its pixels are not expected to equal v2. **v2** is the new
*post-polarity* current-output gate. The existing 8-bit v1 input had SHA-256
`6b47b510c4a8f604e6404b1fbbc5f68bdaa77043976acdad0153ef83524b6e34`,
but its original generation command was not retained, so it was not promoted
as a reproducible v2 input. The new algorithmic 8-bit generator is pinned to
FFmpeg 8.1.3, with the full command in
[`generate-8bit-production-baseline.sh`](../tests/fixtures/codecs/generate-8bit-production-baseline.sh).
Two independent generations were byte-identical at SHA-256
`6e5c214b813dca3e1db65629b1241cfb663166eb565cda1c48b5ee74ed0dce6b`
(19,672,925 bytes). The checked-in 10-bit input remains SHA-256
`df69c98ca6592b08c6bc34127b2bb5cf4125c759fd852b830e5426d5818fccda`.
Both inputs are 1920×1080, 300 frames, 50 fps, BT.709 limited. The 10-bit
fixture has 700,261,022 samples with nonzero low two bits out of 933,120,000.

The exact generation and conversion commands are the complete contents of
those checked-in scripts. To replay v2 on this qualified host:

```bash
tests/fixtures/codecs/generate-8bit-production-baseline.sh \
  /tmp/asciiflow-stage53b1-v2-input8-run1.mp4
tests/baselines/media/generate-post-polarity-v2.sh \
  /tmp/asciiflow-stage53b1-v2-input8-run1.mp4 \
  tests/fixtures/codecs/hevc-main10-canonical-v1.mp4 \
  /tmp/asciiflow-stage53b1-post-polarity-v2
```

The production script runs `target/release/asciiflow INPUT OUTPUT` with
`--width 80 --charset standard --font builtin-8x8 --color true --audio none
--max-frames 300 --decode vaapi --backend vulkan --vulkan-mapping gpu
--encode vaapi --hw-device /dev/dri/renderD128
--vaapi-vulkan-input-interop on --vaapi-vulkan-output-interop on
--output-codec CODEC --output-bit-depth DEPTH --no-progress` for each of
`h264/8`, `hevc/8`, `av1/8`, `hevc/10`, `av1/10`, three times each. All fifteen
logs identify VAAPI decode → NV12/P010 DMA-BUF input interop → Vulkan ASCII →
NV12/P010 DMA-BUF output interop → VAAPI encode on Intel Arc.

| Case | Runs 1, 2 and 3: identical whole-MP4 SHA-256 | Full decoded `framemd5` file SHA-256, identical across runs |
| --- | --- | --- |
| H.264 8-bit | `7c8a7572320b8c6c999143dfece4a76d487d6c9e7788206b7af52df9acd4e1cc` | `c5b5f8bcd31bdbdc5b40cdb3a4e075332e2359c46c4a35317a60de1e2d0a263c` |
| HEVC Main8 | `294a1b63ac5b0e440dcf60c4c60f975594c6e944829478b69f09f981d268ff71` | `755bdcd256c58b748b95d2a5417461b5077656ec0d7265a1dfe1d78a8d6fa799` |
| AV1 Main8 | `1531e29f52ae4e747251cf1889003dfd420303c523fb0ddc55cc9140e3bf2a4c` | `086c1821e563822576d872af8761f4282ae8c0e41972d1008870578fde5b4dc6` |
| HEVC Main10 | `f213a9f75542421bb816550cd7a93796db98ae36296a00636f7273492740a142` | `3eeb03862bf1d3aa1762192bc393fddd25024f0f187ce4982323accfb8eef517` |
| AV1 Main 10-bit | `b69c68525ac6ab2464e04b8fc1f123d887b45bba580377bafad99dc74cf62ad7` | `21ba0466e1805b28281675312619f1cd192a8e67fdb577024c7dadddcf7af933` |

For each case, runs 2 and 3 were compared to run 1 with the exact-build
regression comparator: **Tier 1A raw packets, Tier 1B coded semantics, Tier
1C decoded pixels/PTS, Tier 2 structure and Tier 3 whole-file bytes all
PASS** (ten comparisons). In particular the current `.103` H.264 SPS, PPS,
SEI and VCL were raw-exact across runs. The separate retained `.102` versus
`.103` H.264 pair was rerun without exact-build attestation: Tier 1A remains
FAIL, Tier 1B/1C/Tier 2 PASS, with only the pinned SEI version difference.
The normal structured-oracle tests (including the positive SEI case and SPS,
PPS, VCL, unknown SEI, UUID, payload, timestamp, color and audio negatives)
also passed. No exception was added for same-build H.264 bytes.

Every output software-decoded without error to exactly 300 frames at
1920×1080, PTS `0..299` at 1/50 s. FFprobe counted 300 packets and six seconds
per file, with BT.709 primaries, transfer and matrix, limited range, and
profiles `High` H.264, `Main` HEVC/AV1 8-bit, `Main 10` HEVC, and `Main`
AV1 with `yuv420p10le`. The three selected decoded 10-bit frames (0, 150,
299) retained nonzero low-two-bit values: HEVC 5,358,353/9,331,200 samples;
AV1 5,396,166/9,331,200. The Intel 300-frame canonical P010 pre-encode
reference parity test passed byte-exact; it counted 188,297,203 non-four-
aligned pre-encode samples and FD `4 → 36 (max steady 36) → 4`. The targeted
NV12/P010 black-and-white polarity test now confirms the legacy ramp is the
exact reverse of the new one, monochrome black becomes darker, color-mode
black remains black, and the new white region is brighter. This isolates the intentional
pixel change; since the 8-bit input was also reconstructed, v1/v2 whole-video
pixel equality is neither asserted nor meaningful.

Additional real-device gates passed: NV12/P010 input descriptor and pixel
smokes, NV12 full-ASCII two-slot interop, P010 output FreeType/CPU parity,
one NV12/H.264 FreeType + AAC production run, one P010/HEVC Main10 FreeType
+ AAC production run, and one NV12/H.264 FreeType + **two AAC tracks** run.
The native audio oracle confirmed compressed packet payload, timestamps,
language, disposition, stream order and full AAC decode. The existing Intel
AAC/Validation/cancellation opt-in test also passed. HEVC could not encode the
64×64 dual-audio fixture (minimum width/height 128); H.264 was used for that
fixture, as permitted by the two-track gate.

Full 10-bit production stress converted and decode-probed **3000 frames**
on the same GPU path; the output retained Main10/yuv420p10le/BT.709 limited
and 3000 packets/frames. The separate in-process 3000-frame P010 output
interop FD test measured **before 4, early 21, peak 22, after 4** and passed.
The 300-frame full P010 parity test independently returned to baseline FD 4.
Khronos Validation was enabled on NV12 and P010 interop/full-GPU runs,
including the 3000-frame production stress: no Validation Error, VUID or
synchronization error was observed. The Intel software/VAAPI semantic test
reconfirmed early rejection for PQ, HLG, BT.2020 SDR and full-range; the CLI
staging-preservation test stayed green.

Final static gates passed: Release workspace build; normal and
`asciiflow-cli/encode-characterization` workspace suites; strict all-target
Clippy with and without that feature; formatting and diff checks; and Vulkan
1.3 `spirv-val` on all 133 generated SPIR-V modules. The PQ CPU vectors,
BT.2020 NCL, limited-P010 HDR, linear averaging/blending, FreeType reference,
low-bit and NaN/Inf diagnostics are covered by the green workspace suite;
no PQ math changed. The remaining boundary is intentional: production HDR
is still rejected. v2 whole-file hashes are only exact-build/driver gates,
not promises across FFmpeg, Intel driver or Mesa updates. Stage 5.3B-2
Vulkan PQ implementation is now justified, but was **not started**.

### Rust-only repository consolidation recheck

After retiring the former C# tree, the Rust CLI's `--help` description stopped
calling the application “v2”. This changed the Release binary SHA-256 to
`67efb4a6635e989fa02915be652b923e086ca3c2d58b3980b0bc5d27a87b86ea`
on `HEAD aa04abd225a5af8546bcfe2b91e19e19491cfdb8` plus that help-only
source edit. FFmpeg, Intel driver, both input SHA-256 values and the production
script were unchanged. All five full-GPU conversions were run three more times
under the new binary; **all fifteen whole-file hashes exactly matched the
table above**. The manifest records both binary identities and the recheck.
This is a build-identity requalification, not a new product or media-baseline
generation. The historical stage evidence above remains intact.
