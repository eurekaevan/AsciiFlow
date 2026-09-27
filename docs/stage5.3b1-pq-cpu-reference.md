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

**Status: NOT SEALED.** The PQ CPU reference gates and four of five structured
media regressions are green. H.264's first compressed packet is not byte-equal
under the explicitly retained Tier 1 rule. Stage 5.3B-2 is therefore **not
formally justified**; this pass stops without Vulkan PQ, HLG, tone mapping,
HDR production output or changes to HDR pixel math.

Subsequent default-ramp polarity correction changes rendered pixels by design.
The five hashes and comparisons above remain a pre-correction snapshot, not
current-output gates; no historical hash was rewritten. See
[the current regression policy](testing.md#media-regression-policy).
