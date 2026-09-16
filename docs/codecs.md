# Video codecs (Stage 5.1A)

Qualified software input scope: H.264 8-bit 4:2:0, HEVC Main 8-bit 4:2:0,
and AV1 Main 8-bit 4:2:0. Codec identity comes from probing; no input-codec flag
is needed. MP4 output defaults to H.264 8-bit; `--output-codec hevc` selects
HEVC Main 8-bit 4:2:0 through VAAPI. Output codec and `--encode` backend are
independent. Software HEVC and all AV1 output are explicitly unsupported.
`--decode software|vaapi|auto` applies to the detected codec. Explicit VAAPI
never substitutes a software decoder.

The common FFmpeg decoder handles all three codecs, including display-order PTS,
delayed frames and EOF drain. Software selection uses codec-ID discovery; VAAPI
selection enumerates matching decoders and their hardware configurations (the
default AV1 software decoder need not support VAAPI). Every decoded frame is
checked for 8-bit 4:2:0 before conversion/import. Hardware frames must actually
be VAAPI with a hardware frames context. Ten-bit, unsupported chroma, PQ/HLG
and BT.2020 frames fail, not silently convert to the current SDR pipeline.

Capabilities retain Supported/Unsupported/NotProbed states. Native probing checks
FFmpeg configuration and driver profile + VLD for H.264, HEVC Main and AV1
Profile0. It dynamically loads system libva using the existing libloading stack;
software-only use does not require libva development headers. The selected stream
is additionally decoded and mapped: static profile support is not proof that an
arbitrary stream's surface can be imported. The actual DRM descriptor, modifier
and Vulkan external-memory check remain authoritative. Logical sw_format only
validates depth/chroma and never substitutes for the descriptor.

Input interop and initialization replan are scoped by codec/stream. Auto still
prefers software decode when input interop is unavailable, rather than automatic
VAAPI hwdownload. At most one initialization replan is allowed; runtime errors
remain terminal. Output replan facts are also codec-scoped: failure to import an
HEVC encoder surface does not disable H.264 output interop. FFmpeg manages
decoder and encoder reference surfaces; no new fixed DPB pool was added.

## Validation status

Software fixtures/tests and hardware opt-in tests are provided. Intel Arc Meteor
Lake qualification passed on 2026-09-16: byte-exact decode/import/ASCII parity,
FreeType plus AAC full conversion, and 3000-frame full-interop reuse per new
codec with zero validation errors and FD counts returning to baseline. This is
not a blanket guarantee for other devices, profiles or AV1 film grain. See
[stage5.0-codec-validation.md](stage5.0-codec-validation.md) for measurements.

Run regular tests with `cargo test --workspace`. On a qualified Intel host:

```bash
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop --test hardware hevc_av1 -- --ignored --nocapture
cargo test -p asciiflow-media --test codec_decode hevc_av1_software_and_vaapi -- --ignored
```

Fixtures are synthetic, small and checked in with fixed generator version,
commands and SHA-256 under `tests/fixtures/codecs`. Normal tests need decoders,
not HEVC/AV1 encoders. AV1 baseline has no film grain; HEVC exercises B-frame
reordering. Film-grain equivalence is not claimed.

HEVC output qualification is recorded in
[stage5.1a-hevc-encode-validation.md](stage5.1a-hevc-encode-validation.md).
It covers HEVC Main via VAAPI only. There is no software HEVC encoder, AV1
encoder, Main10/P010, HDR/tone mapping, 4:2:2/4:4:4 output, quality/preset UI,
B-frame tuning, or new GPU vendor/platform support.
