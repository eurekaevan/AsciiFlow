# Production support contract

Version: 1.1.0. Generated from `tests/support/production-support-v1.json`; do not edit the tables manually.

## Support states

- ConditionallySupported: Qualified only when every listed condition holds.
- Supported: Qualified by retained production evidence within the scope stated here.
- Unqualified: No production evidence supports a claim; current planner acceptance is not qualification.
- Unsupported: Explicitly rejected by the current product contract or sealed production evidence.

## Qualification scope

- ffmpeg: FFmpeg 8.1.3
- gpu: Intel Arc Meteor Lake, PCI vendor/device 8086:7d55
- limitations: ['Production qualification is tied to the sealed Stage 5.3C-4B source, binary and host manifest.', 'The per-source-pixel 1000 cd/m2 ceiling applies only to explicit HDR PQ to SDR conversion; it does not apply to PQ preservation.', 'PQ preservation passes PQ code values through. The HDR-to-SDR source-domain gate is not evidence of PQ-preserve luminance limits.', 'Stage 5.3C-4B reports CPU and software/staged HDR paths as unqualified.', 'Planner acceptance and synthetic capability facts never promote a dimension to a production support state.', 'Stage 5.4B extensions qualify only the exact source SHA/probe and request tuples in real-media-v1.json. Portable and hardware cases are distinct; no Cartesian-product or general container/profile promotion.']
- media_device: /dev/dri/renderD128
- vaapi: Intel iHD 26.1.5
- vulkan: Mesa ANV 26.2.3

## Production limits

```json
{
  "hdr_to_sdr": {
    "applies_to": "explicit_pq_to_sdr_only",
    "applies_to_pq_preserve": false,
    "decoded_max_nits": 1000,
    "enforced_before_output_pack": true,
    "measurement": "per_source_pixel_before_average_or_coverage",
    "unit": "cd/m2"
  }
}
```

## Dimensions

Rows classify individual dimensions, not their Cartesian product. Conditions and scenario tests govern complete paths.

| Dimension | Value | State | Conditions | Evidence |
|---|---|---|---|---|
| audio_codec | aac | ConditionallySupported | MP4-compatible single/dual AAC cases only. | stage53c4b-runtime |
| audio_codec | other | Unqualified |  |  |
| audio_policy | auto | ConditionallySupported | MP4-compatible compressed streams; AAC single/dual-track cases qualified. Existing CFR video timeline required; incompatible streams are omitted, not transcoded. | stage53c4b-runtime |
| audio_policy | copy | ConditionallySupported | Strict MP4 copy eligibility and the existing CFR video timeline; AAC single/dual-track cases qualified. VFR/discontinuities reject. | stage53c4b-runtime |
| audio_policy | none | Supported |  | stage53c4b-runtime |
| backend | auto | ConditionallySupported | Only for sealed automatic-path test cases. | stage53c4b |
| backend | cpu | Supported | Portable NV12 SDR production. P010 CPU is separately qualified processing; CPU production HDR preserve/tone mapping is unsupported. | stage2 |
| backend | vulkan | ConditionallySupported | Scoped Mesa ANV device and qualified operation. | stage53c4b |
| chroma_location | center | Unsupported |  | stage53c4b |
| chroma_location | left | Supported |  | stage53b1, stage53c4b |
| chroma_location | unspecified | Unqualified |  |  |
| color_matrix | bt2020-constant | Unsupported |  | stage53c4b |
| color_matrix | bt2020-ncl | ConditionallySupported | Canonical PQ input only. | stage53c4b |
| color_matrix | bt601 | Unqualified | Legacy 8-bit software normalization exists in code/tests; no sealed production qualification for this path. |  |
| color_matrix | bt709 | Supported |  | stage53b1, stage53c4b |
| color_matrix | identity | Unqualified |  |  |
| color_matrix | other | Unqualified |  |  |
| color_matrix | unknown | Unqualified |  |  |
| color_matrix | unspecified | Unqualified |  |  |
| color_primaries | bt2020 | ConditionallySupported | PQ input only; limited range, BT.2020 NCL matrix, left chroma, VAAPI decode, Vulkan processing, VAAPI encode and matching input/output profile interop. | stage53c4b |
| color_primaries | bt470bg | Unqualified |  |  |
| color_primaries | bt709 | Supported |  | stage53b1, stage53c4b |
| color_primaries | display-p3 | Unsupported |  | stage53c4b |
| color_primaries | other | Unqualified |  |  |
| color_primaries | smpte170m | Unqualified |  |  |
| color_primaries | smpte240m | Unqualified |  |  |
| color_primaries | unknown | Unqualified |  |  |
| color_primaries | unspecified | Unqualified |  |  |
| color_range | full | Unsupported |  | stage53c4b |
| color_range | limited | Supported |  | stage53b1, stage53c4b |
| color_range | unspecified | Unqualified |  |  |
| color_transfer | bt709 | Supported |  | stage53b1, stage53c4b |
| color_transfer | gamma22 | Unqualified |  |  |
| color_transfer | gamma28 | Unqualified |  |  |
| color_transfer | hlg | Unsupported |  | stage53c4b |
| color_transfer | linear | Unqualified |  |  |
| color_transfer | other | Unqualified |  |  |
| color_transfer | pq | ConditionallySupported | Canonical PQ preservation or explicit HDR-to-SDR conversion using VAAPI decode, Vulkan processing, VAAPI encode and matching input/output profile interop; source-domain ceiling of 1000 cd/m2 applies only to conversion. | stage53c4b |
| color_transfer | smpte170m | Unqualified |  |  |
| color_transfer | srgb | Unqualified |  |  |
| color_transfer | unknown | Unqualified |  |  |
| color_transfer | unspecified | Unqualified |  |  |
| decode | auto | ConditionallySupported | Only for the sealed automatic-path font/color/mono NV12/P010 smoke cases. | stage53c4b |
| decode | software | Supported | Portable 8-bit SDR decode path; strict 10-bit processing has separate format qualification. PQ software decode is inspection only, never production. | stage2 |
| decode | vaapi | ConditionallySupported | Intel iHD 26.1.5 on PCI 8086:7d55, qualified input profile and format. | stage53b1, stage53c4b |
| display_geometry | non-square-sar-or-non-identity-matrix | Unsupported | Reject InputProbe/UnsupportedFrame; no silent rotation, reflection, aspect-ratio normalization or transform application. | stage54b-corpus, stage54b-report |
| display_geometry | odd-420-dimensions | Unsupported | Native 129x97 AV1 yuv420p rejected InputProbe/Media by the even-dimension NV12 admission guard; not padded into a claimed supported source. | stage54b-corpus, stage54b-report |
| display_geometry | square-or-unspecified-sar-identity-or-absent-matrix | ConditionallySupported | Exact reviewed tuples; unspecified SAR uses the existing square interpretation. Renderer does not implement display transforms. | stage54b-corpus, stage54b-report |
| dynamic_range | conflicting | Unsupported |  | stage53c4b |
| dynamic_range | hlg | Unsupported |  | stage53c4b |
| dynamic_range | pq | ConditionallySupported | Preserve canonical PQ or explicitly convert PQ to SDR; only the conversion is limited to 0–1000 cd/m2 per source pixel. | stage53c4b |
| dynamic_range | sdr | Supported |  | stage53b1, stage53c4b |
| dynamic_range | unknown | Unsupported | Strict 10-bit unknown metadata is rejected. Legacy 8-bit unspecified metadata may use BT.709 defaults; that normalization is not a production qualification. | stage53c4b |
| encode | auto | ConditionallySupported | Only for sealed automatic-path smoke cases. | stage53c4b |
| encode | software | Supported | H.264 8-bit SDR via libx264 only. Software HEVC/AV1 encoding and all software HDR production are unsupported. | stage2 |
| encode | vaapi | ConditionallySupported | Qualified output codec/profile/depth on Intel iHD 26.1.5. | stage53b1, stage53c4b |
| font | builtin-8x8 | ConditionallySupported | Sealed automatic-path render smoke cases. | stage53c4b-runtime |
| font | freetype | ConditionallySupported | Sealed smoke tests use a valid installed font; font availability remains host-dependent. | stage53c4b-runtime |
| input_bit_depth | 10 | ConditionallySupported | HEVC Main10 or AV1 Main 10-bit 4:2:0 under the scoped hardware path. | stage53c4b |
| input_bit_depth | 12 | Unsupported |  | stage53c4b |
| input_bit_depth | 8 | Supported |  | stage53b1, stage53c4b |
| input_bit_depth | other | Unsupported |  | stage53c4b |
| input_chroma_subsampling | other | Unqualified |  |  |
| input_chroma_subsampling | unknown | Unqualified |  |  |
| input_chroma_subsampling | yuv420 | Supported |  | stage53b1, stage53c4b |
| input_chroma_subsampling | yuv422 | Unsupported |  | stage53c4b |
| input_chroma_subsampling | yuv444 | Unsupported |  | stage53c4b |
| input_codec | av1 | ConditionallySupported | Qualified input profiles and bit depths in the retained production cases; PQ input additionally requires AV1 Main 10-bit 4:2:0. | stage53b1, stage53c4b |
| input_codec | h264 | Supported |  | stage53b1, stage2 |
| input_codec | hevc | Supported |  | stage53b1, stage53c4b, stage2 |
| input_codec | other | Unqualified |  |  |
| input_color | conflicting | Unsupported |  | stage53c4b |
| input_color | full_pq | Unsupported |  | stage53c4b |
| input_color | hlg | Unsupported |  | stage53c4b |
| input_color | pq | ConditionallySupported | Canonical limited BT.2020 NCL/PQ and left chroma; HEVC Main10 or AV1 Main 10-bit 4:2:0; VAAPI decode, Vulkan processing, VAAPI encode and matching input/output profile interop. The 0–1000 cd/m2 source pixel ceiling applies only to explicit HDR-to-SDR conversion. | stage53c4b |
| input_color | sdr601 | Unqualified | Legacy 8-bit software normalization is planner-accepted, but this path lacks production qualification. |  |
| input_color | sdr709 | Supported |  | stage53b1, stage53c4b |
| input_color | unknown | Unsupported |  | stage53c4b |
| input_color | wide_sdr | Unsupported |  | stage53c4b |
| input_container | matroska | Unqualified | Video-only H.264 sample converts, but AAC/FLAC Matroska-to-MP4 strict oracles fail: unspecified language becomes und, absent default becomes true, millisecond packet duration and missing first duration differ. Not a supported Matroska/audio preservation claim. |  |
| input_container | mov | Unqualified | H.264/AAC sample converts but the unchanged strict audio oracle rejects undefined language becoming und. No MOV support promotion. |  |
| input_container | mp4 | ConditionallySupported | Sealed retained matrix plus exact Stage 5.4B MP4 tuples (ordinary/fast-start/fragmented) only. AAC payload/timing/default/language are strictly checked where copied. Subtitles and video display titles are explicitly not copied. | stage53b1, stage53c4b, stage54b-corpus |
| input_gop | benign-elementary-stream-parameter-update | Unqualified | Level-only SPS change drains eight frames, but original Annex B PTS/DTS are absent; timeline preservation is unqualified. |  |
| input_gop | reviewed-b-frames-long-gop-all-intra | ConditionallySupported | Exact H.264 B0/B2/B4, HEVC B2, 250-frame GOP and all-intra portable sources plus the H.264 B2 hardware clone. Packet DTS monotonicity/reorder and decoded presentation spans checked; no open-GOP blanket claim. | stage54b-corpus, stage54b-report |
| input_interop | auto | ConditionallySupported | Only for sealed automatic-path smokes. | stage53c4b |
| input_interop | off | Supported | Portable software NV12 SDR path; PQ production requires input interop on. This does not qualify staged HDR. | stage2 |
| input_interop | on | ConditionallySupported | Qualified decoded codec/format and Intel VAAPI-to-Vulkan path. | stage53c4b |
| input_metadata | legacy-8bit-unspecified-color | ConditionallySupported | Only exact reviewed 8-bit sources use the existing BT.709 default policy. Raw null metadata remains recorded, never inferred from pixels. This does not permit unknown strict 10-bit color. | stage54b-corpus, stage54b-report |
| input_metadata | pq-static-output-policy | ConditionallySupported | Exact 1080p PQ sources with/without static metadata qualify on the Intel path. Output retains PQ/BT.2020 limited signal but does NOT propagate or recompute source mastering/MaxCLL/MaxFALL/dynamic HDR side data; no static-metadata fidelity claim. | stage54b-corpus, stage54b-report |
| input_metadata | sdr-with-hdr-static | Unsupported | Static mastering/content-light with known SDR transfer is Conflicting, including stream or frame side data. | stage54b-corpus, stage54b-report |
| input_metadata | strict-10bit-missing-primaries-transfer-matrix | Unsupported | Actual generated HEVC10 negatives fail InputProbe/UnsupportedColor. Native missing range was not constructed; its fixture still reports limited range and remains Unqualified. | stage54b-corpus, stage54b-report |
| input_pixel_format | nv12 | Supported |  | stage53b1, stage53c4b |
| input_pixel_format | other | Unqualified |  |  |
| input_pixel_format | p010le | ConditionallySupported | Qualified 10-bit 4:2:0 only. | stage53c4b |
| input_pixel_format | yuv420p | Supported |  | stage53b1, stage53c4b |
| input_pixel_format | yuv420p10le | ConditionallySupported | Qualified 10-bit 4:2:0 only. | stage53c4b |
| input_profile | av1-main | ConditionallySupported | Qualified AV1 Main / Profile 0 8-bit output and AV1 Main 10-bit PQ input cases only. | stage53c4b |
| input_profile | h264-baseline | Supported |  | stage2 |
| input_profile | h264-high | Supported |  | stage53b1, stage2 |
| input_profile | h264-main | Unqualified | Device capability enumeration is not a production profile qualification. |  |
| input_profile | hevc-main | ConditionallySupported | Qualified 8-bit HEVC Main production case. | stage53b1, stage53c4b |
| input_profile | hevc-main10 | ConditionallySupported | Qualified PQ input and 10-bit output cases only with 4:2:0 and the scoped Intel hardware path. | stage53c4b |
| input_profile | other | Unqualified |  |  |
| input_resolution | 1280x720 | ConditionallySupported | Only exact Stage 5.4B portable software H.264 8-bit video-only tuple; no VAAPI/profiles/10-bit resolution generalization. | stage54b-corpus, stage54b-report |
| input_resolution | 128x96 | ConditionallySupported | Only exact Stage 5.4B portable software H.264 8-bit video-only tuple; no VAAPI/profiles/10-bit resolution generalization. | stage54b-corpus, stage54b-report |
| input_resolution | 1920x1080 | ConditionallySupported | Canonical production matrix. Native decode/encode and interop probes must accept the actual dimensions; profile availability alone is insufficient. | stage53b1, stage53c4b |
| input_resolution | 2x2 | ConditionallySupported | Only exact Stage 5.4B portable software H.264 8-bit video-only tuple; no VAAPI/profiles/10-bit resolution generalization. | stage54b-corpus, stage54b-report |
| input_resolution | 4x4 | ConditionallySupported | Only exact Stage 5.4B portable software H.264 8-bit video-only tuple; no VAAPI/profiles/10-bit resolution generalization. | stage54b-corpus, stage54b-report |
| input_resolution | other | Unqualified | No blanket resolution claim. Current planner may admit dimensions after actual runtime probing; 64x64 P010 encoder probes reject on the recorded iHD stack. |  |
| input_timing | cfr-tested-rates | ConditionallySupported | 24000/1001,24,25,30000/1001,30,50,60000/1001,60: exact portable H.264 8-bit tuples. Source frames map one-to-one to encoder sequential CFR index; video-only origin is zero. Audio-copy checks original source video CFR grid and preserves its origin. Native source/output tick bounds, not arbitrary millisecond tolerance. | stage54b-corpus, stage54b-report |
| input_timing | discontinuous-with-audio-copy | Unsupported | Observed timestamp gap/backward/VFR AAC cases reject DecodeRuntime/Media and preserve preexisting output. | stage54b-corpus, stage54b-report |
| input_timing | unusual-time-bases-and-origins | ConditionallySupported | Exact 1/1000,1/90000,1/48000,1/1000000 and +2s/+86400s source tuples only; output policy as above. Audio/video offset and shorter/longer AAC preserve compressed timestamps and endpoints in exact reviewed MP4 tuples. | stage54b-corpus, stage54b-report |
| input_timing | vfr | Unqualified | Video-only VFR is retimed one frame per decoded frame to zero-origin CFR: source presentation duration is not preserved. Audio-copy VFR fails DecodeRuntime/Media; future faithful timeline policy required. |  |
| midstream_changes | color-semantic-change | Unsupported | Stream codec parameters and actual frame color remain independent; tested changed SPS and container/frame conflict reject UnsupportedColor/Conflicting. | stage54b-corpus, stage54b-report |
| midstream_changes | geometry-or-pixel-format | Unsupported | Actual native decoded dimensions and software pixel format are checked before cached scaler/interop use; tested resolution and 8-to-10-bit elementary streams reject. | stage54b-corpus, stage54b-report |
| output_bit_depth | 10 | ConditionallySupported | HEVC Main10 or AV1 Main 10-bit profile on the scoped P010 path. | stage53c4b |
| output_bit_depth | 12 | Unsupported |  | stage53c4b |
| output_bit_depth | 8 | Supported |  | stage53b1, stage53c4b |
| output_bit_depth | other | Unsupported |  | stage53c4b |
| output_codec | av1 | ConditionallySupported | VAAPI AV1 codec/profile/depth probe and scoped P010/NV12 path required. | stage53b1, stage53c4b |
| output_codec | h264 | Supported |  | stage53b1, stage53c4b |
| output_codec | hevc | ConditionallySupported | VAAPI HEVC codec/profile/depth probe and scoped P010/NV12 path required. | stage53b1, stage53c4b |
| output_codec | other | Unsupported |  | stage53c4b |
| output_container | matroska | Unsupported | The current CLI rejects non-MP4 output paths. | current-cli |
| output_container | mp4 | Supported |  | stage53b1, stage53c4b |
| output_dynamic_range | preserve | ConditionallySupported | SDR preserve is qualified for the five 8/10-bit output profiles; PQ preserve only for HEVC Main10 and AV1 Main 10-bit. The 1000 cd/m2 limit does not apply to PQ preservation. | stage53b1, stage53c4b |
| output_dynamic_range | sdr | ConditionallySupported | Explicit PQ-to-SDR conversion is qualified only for canonical input with every source pixel at or below 1000 cd/m2. | stage53c4b |
| output_interop | auto | ConditionallySupported | Only for sealed automatic-path smoke cases. | stage53c4b |
| output_interop | off | Supported | Portable software H.264/NV12 SDR output; PQ production requires output interop on. No staged HDR qualification. | stage2 |
| output_interop | on | ConditionallySupported | Qualified output codec/profile/depth and NV12/P010 format on the scoped Intel path. | stage53c4b |
| output_profile | av1-main | ConditionallySupported | AV1 8-bit SDR or 10-bit SDR conversion/PQ preserve requires the matching scoped VAAPI profile and format probe. | stage53b1, stage53c4b |
| output_profile | h264-encoder-selected | Supported | H.264 output is qualified at 8-bit SDR; no 10-bit H.264 output. | stage53b1, stage53c4b |
| output_profile | hevc-main | ConditionallySupported | 8-bit SDR output requires the scoped VAAPI HEVC Main encode path. | stage53b1, stage53c4b |
| output_profile | hevc-main10 | ConditionallySupported | 10-bit SDR conversion or PQ preservation requires the scoped VAAPI P010 path. | stage53c4b |
| subtitle_policy | ignored | ConditionallySupported | Exact mov_text input is decoded alongside video but subtitles are intentionally omitted, not transcoded or copied. | stage54b-corpus, stage54b-report |

## Complete scenario inventory

Generated from the existing contract cases, not a Cartesian-product support promise. Each row retains its declared backend, decode/encode and interop requirements in the source contract. Dynamic range records input color classification and explicit output intent.

Audio and container cells below are globally conditioned scope references, not additional qualification of each row with every audio policy or container. Only exact retained/corpus tuples qualify MP4-compatible single/dual AAC copy or video-only output; audio copy requires the existing CFR timeline. Other container/audio combinations keep their dimension states above.

| Scenario | Input codec | Input bit depth | Dynamic range (input / output intent) | Output codec | Output bit depth | Audio | Container | Status |
|---|---|---|---|---|---|---|---|---|
| portable-h264-sdr | h264 | 8 | sdr709 / preserve | h264 | 8 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | Supported |
| sdr-h264-8 | h264 | 8 | sdr709 / preserve | h264 | 8 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | ConditionallySupported |
| sdr-hevc-8 | h264 | 8 | sdr709 / preserve | hevc | 8 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | ConditionallySupported |
| sdr-av1-8 | h264 | 8 | sdr709 / preserve | av1 | 8 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | ConditionallySupported |
| sdr-hevc-10 | hevc | 10 | sdr709 / preserve | hevc | 10 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | ConditionallySupported |
| sdr-av1-10 | hevc | 10 | sdr709 / preserve | av1 | 10 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | ConditionallySupported |
| pq-preserve-hevc | hevc | 10 | pq / preserve | hevc | 10 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | ConditionallySupported |
| pq-preserve-av1 | av1 | 10 | pq / preserve | av1 | 10 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | ConditionallySupported |
| matrix-hevc-to-h264-8 | hevc | 10 | pq / sdr | h264 | 8 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | ConditionallySupported |
| matrix-hevc-to-hevc-8 | hevc | 10 | pq / sdr | hevc | 8 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | ConditionallySupported |
| matrix-hevc-to-av1-8 | hevc | 10 | pq / sdr | av1 | 8 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | ConditionallySupported |
| matrix-hevc-to-hevc-10 | hevc | 10 | pq / sdr | hevc | 10 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | ConditionallySupported |
| matrix-hevc-to-av1-10 | hevc | 10 | pq / sdr | av1 | 10 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | ConditionallySupported |
| matrix-av1-to-h264-8 | av1 | 10 | pq / sdr | h264 | 8 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | ConditionallySupported |
| matrix-av1-to-hevc-8 | av1 | 10 | pq / sdr | hevc | 8 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | ConditionallySupported |
| matrix-av1-to-av1-8 | av1 | 10 | pq / sdr | av1 | 8 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | ConditionallySupported |
| matrix-av1-to-hevc-10 | av1 | 10 | pq / sdr | hevc | 10 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | ConditionallySupported |
| matrix-av1-to-av1-10 | av1 | 10 | pq / sdr | av1 | 10 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | ConditionallySupported |
| missing-tone-map-capability | hevc | 10 | pq / sdr | hevc | 10 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | Unsupported |
| missing-pq-encoder-capability | hevc | 10 | pq / preserve | hevc | 10 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | Unsupported |
| unknown-color | hevc | 10 | unknown / preserve | hevc | 10 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | Unsupported |
| conflicting-color | hevc | 10 | conflict / preserve | hevc | 10 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | Unsupported |
| unqualified-h264-main-currently-plans | h264 | 8 | sdr709 / preserve | h264 | 8 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | Unqualified |
| unqualified-601-normalization-currently-plans | h264 | 8 | sdr601 / preserve | h264 | 8 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | Unqualified |
| h264-10bit-output-unsupported | h264 | 8 | sdr709 / preserve | h264 | 10 | Global audio conditions above; not per-row qualification | MP4 only under global conditions above | Unsupported |

## Release profile `lgpl-prebuilt`

Official LGPL prebuilt Linux release; opt-in distribution restrictions, not a replacement for the base support contract.

Profile policy and actual FFmpeg codec capability probes both apply. Software decode and qualified VAAPI H.264/HEVC/AV1 encoding remain eligible; no silent software encoder fallback.

Historical software H.264 PASS remains qualified under a GPL-capable build context; exclusion is not a historical FAIL.

Availability is a distribution overlay, not a fifth base support state. Eligible does not promote Unsupported or Unqualified base cases.

| Scenario | Base state | Release availability | Reason |
|---|---|---|---|
| portable-h264-sdr | Supported | Excluded | Software H.264 encoding requires GPL dependency libx264 and is excluded from the official LGPL prebuilt release. |
| sdr-h264-8 | ConditionallySupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| sdr-hevc-8 | ConditionallySupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| sdr-av1-8 | ConditionallySupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| sdr-hevc-10 | ConditionallySupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| sdr-av1-10 | ConditionallySupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| pq-preserve-hevc | ConditionallySupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| pq-preserve-av1 | ConditionallySupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| matrix-hevc-to-h264-8 | ConditionallySupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| matrix-hevc-to-hevc-8 | ConditionallySupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| matrix-hevc-to-av1-8 | ConditionallySupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| matrix-hevc-to-hevc-10 | ConditionallySupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| matrix-hevc-to-av1-10 | ConditionallySupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| matrix-av1-to-h264-8 | ConditionallySupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| matrix-av1-to-hevc-8 | ConditionallySupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| matrix-av1-to-av1-8 | ConditionallySupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| matrix-av1-to-hevc-10 | ConditionallySupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| matrix-av1-to-av1-10 | ConditionallySupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| missing-tone-map-capability | Unsupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| missing-pq-encoder-capability | Unsupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| unknown-color | Unsupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| conflicting-color | Unsupported | Eligible | Base conditions and actual runtime capability probes still apply. |
| unqualified-h264-main-currently-plans | Unqualified | Excluded | Software H.264 encoding requires GPL dependency libx264 and is excluded from the official LGPL prebuilt release. |
| unqualified-601-normalization-currently-plans | Unqualified | Excluded | Software H.264 encoding requires GPL dependency libx264 and is excluded from the official LGPL prebuilt release. |
| h264-10bit-output-unsupported | Unsupported | Excluded | Software H.264 encoding requires GPL dependency libx264 and is excluded from the official LGPL prebuilt release. |

## Evidence

- `current-cli`: [apps/asciiflow-cli/src/main.rs](../apps/asciiflow-cli/src/main.rs) — current checkout, output-container-guard, CURRENT.
- `stage2`: [docs/stage2-validation.md](../docs/stage2-validation.md) — Stage 2, validation-report, QUALIFIED.
- `stage53b1`: [tests/baselines/media/post-polarity-v2.json](../tests/baselines/media/post-polarity-v2.json) — Stage 5.3B-1, qualified-baseline-manifest, QUALIFIED.
- `stage53c4b`: [tests/baselines/media/stage53c4b.json](../tests/baselines/media/stage53c4b.json) — Stage 5.3C-4B, sealed-manifest, SEALED.
- `stage53c4b-device`: [tests/baselines/media/c4b-static-and-device-evidence.txt](../tests/baselines/media/c4b-static-and-device-evidence.txt) — Stage 5.3C-4B, device-and-runtime-evidence, SEALED.
- `stage53c4b-report`: [docs/stage5.3c4b-hdr-to-sdr-production.md](../docs/stage5.3c4b-hdr-to-sdr-production.md) — Stage 5.3C-4B, production-report, SEALED.
- `stage53c4b-runtime`: [tests/baselines/media/c4b-system-evidence.txt](../tests/baselines/media/c4b-system-evidence.txt) — Stage 5.3C-4B, observed-system-evidence, SEALED.
- `stage54b-corpus`: [tests/corpus/real-media-v1.json](../tests/corpus/real-media-v1.json) — Stage 5.4B, reviewed-real-media-corpus, QUALIFIED.
- `stage54b-report`: [docs/stage5.4b-real-media-compatibility.md](../docs/stage5.4b-real-media-compatibility.md) — Stage 5.4B, compatibility-report, QUALIFIED.
