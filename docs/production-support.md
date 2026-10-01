# Production support contract

Version: 1.0.0. Generated from `tests/support/production-support-v1.json`; do not edit the tables manually.

## Support states

- ConditionallySupported: Qualified only when every listed condition holds.
- Supported: Qualified by retained production evidence within the scope stated here.
- Unqualified: No production evidence supports a claim; current planner acceptance is not qualification.
- Unsupported: Explicitly rejected by the current product contract or sealed production evidence.

## Qualification scope

- ffmpeg: FFmpeg 8.1.3
- gpu: Intel Arc Meteor Lake, PCI vendor/device 8086:7d55
- limitations: ['Production qualification is tied to the sealed Stage 5.3C-4B source, binary and host manifest.', 'The per-source-pixel 1000 cd/m2 ceiling applies only to explicit HDR PQ to SDR conversion; it does not apply to PQ preservation.', 'PQ preservation passes PQ code values through. The HDR-to-SDR source-domain gate is not evidence of PQ-preserve luminance limits.', 'Stage 5.3C-4B reports CPU and software/staged HDR paths as unqualified.', 'Planner acceptance and synthetic capability facts never promote a dimension to a production support state.']
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
| input_container | matroska | Unqualified | No sealed Matroska production matrix. The current CLI accepts MP4 output only; that implementation restriction is not Matroska qualification evidence. |  |
| input_container | mp4 | ConditionallySupported | Only the sealed codec/profile/depth/path matrix is qualified. | stage53b1, stage53c4b |
| input_interop | auto | ConditionallySupported | Only for sealed automatic-path smokes. | stage53c4b |
| input_interop | off | Supported | Portable software NV12 SDR path; PQ production requires input interop on. This does not qualify staged HDR. | stage2 |
| input_interop | on | ConditionallySupported | Qualified decoded codec/format and Intel VAAPI-to-Vulkan path. | stage53c4b |
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
| input_resolution | 1920x1080 | ConditionallySupported | Canonical production matrix. Native decode/encode and interop probes must accept the actual dimensions; profile availability alone is insufficient. | stage53b1, stage53c4b |
| input_resolution | other | Unqualified | No blanket resolution claim. Current planner may admit dimensions after actual runtime probing; 64x64 P010 encoder probes reject on the recorded iHD stack. |  |
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

## Evidence

- `current-cli`: [apps/asciiflow-cli/src/main.rs](../apps/asciiflow-cli/src/main.rs) — current checkout, output-container-guard, CURRENT.
- `stage2`: [docs/stage2-validation.md](../docs/stage2-validation.md) — Stage 2, validation-report, QUALIFIED.
- `stage53b1`: [tests/baselines/media/post-polarity-v2.json](../tests/baselines/media/post-polarity-v2.json) — Stage 5.3B-1, qualified-baseline-manifest, QUALIFIED.
- `stage53c4b`: [tests/baselines/media/stage53c4b.json](../tests/baselines/media/stage53c4b.json) — Stage 5.3C-4B, sealed-manifest, SEALED.
- `stage53c4b-device`: [tests/baselines/media/c4b-static-and-device-evidence.txt](../tests/baselines/media/c4b-static-and-device-evidence.txt) — Stage 5.3C-4B, device-and-runtime-evidence, SEALED.
- `stage53c4b-report`: [docs/stage5.3c4b-hdr-to-sdr-production.md](../docs/stage5.3c4b-hdr-to-sdr-production.md) — Stage 5.3C-4B, production-report, SEALED.
- `stage53c4b-runtime`: [tests/baselines/media/c4b-system-evidence.txt](../tests/baselines/media/c4b-system-evidence.txt) — Stage 5.3C-4B, observed-system-evidence, SEALED.
