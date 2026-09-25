# Video codecs (through Stage 5.2C-2)

Qualified software input scope: H.264 8-bit 4:2:0, HEVC Main 8-bit 4:2:0,
and AV1 Main 8-bit 4:2:0. Codec identity comes from probing; no input-codec flag
is needed. MP4 output defaults to H.264 8-bit; `--output-codec hevc` selects
HEVC Main 8-bit 4:2:0 through VAAPI, and `--output-codec av1` selects AV1
Profile0 (Main) 8-bit 4:2:0 through VAAPI. Output codec and `--encode` backend
are independent. Software HEVC/AV1 encoding is explicitly unsupported, and
an unavailable AV1 encoder never changes the requested output codec.
`--output-codec hevc --output-bit-depth 10` explicitly selects HEVC Main10,
P010LE, 10-bit 4:2:0, BT.709 SDR VAAPI/MP4 output. The depth default is 8;
`--output-codec hevc` alone continues to select HEVC Main/NV12. H.264/10 and
AV1/10 output are unsupported. Ten-bit input with default 8-bit output is
rejected before staging: there is no implicit P010→NV12 conversion. Eight-bit
input to Main10 is likewise outside this stage's production contract.
`--decode software|vaapi|auto` applies to the detected codec. Explicit VAAPI
never substitutes a software decoder.

The common FFmpeg decoder handles all three codecs, including display-order PTS,
delayed frames and EOF drain. Software selection uses codec-ID discovery; VAAPI
selection enumerates matching decoders and their hardware configurations (the
default AV1 software decoder need not support VAAPI). Every decoded frame is
checked for the supported processing format before conversion/import. Hardware
frames must actually be VAAPI with a hardware frames context. Internal software
decode qualification now accepts HEVC Main10 and AV1 Main 10-bit 4:2:0 with
explicit BT.709 SDR tags and yields P010LE; the production CLI accepts those
inputs only when HEVC Main10 output is explicitly selected. Unsupported chroma,
PQ/HLG and BT.2020 fail rather than convert silently. Intel P010 VAAPI/DRM
interop is qualified internally on the observed Intel Arc host; see
[Stage 5.2B](stage5.2b-p010-decode-validation.md).

Capabilities retain Supported/Unsupported/NotProbed states. Native probing checks
FFmpeg configuration and driver profile + VLD for H.264, HEVC Main and AV1
Profile0 input, and matching EncSlice profiles for all three hardware output
codecs. HEVC Main10 encode is a separate fact: Main10/EncSlice, a 10-bit render
target, FFmpeg `hevc_vaapi` opening with a P010 frames context, and path-specific
host-upload/output-import checks. It dynamically loads system libva using the existing libloading stack;
software-only use does not require libva development headers. The selected stream
is additionally decoded and mapped: static profile support is not proof that an
arbitrary stream's surface can be imported. The actual DRM descriptor, modifier
and Vulkan external-memory check remain authoritative. Logical sw_format only
validates depth/chroma and never substitutes for the descriptor.

Input interop and initialization replan are scoped by codec/stream. Auto still
prefers software decode when input interop is unavailable, rather than automatic
VAAPI hwdownload. At most one initialization replan is allowed; runtime errors
remain terminal. Output replan facts are codec/profile/format-scoped: failure to
import a Main10 P010 surface can replan to staged P010 without disabling HEVC
Main 8-bit.
FFmpeg manages
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
It covers HEVC Main via VAAPI only. AV1 output qualification is recorded in
[stage5.1b-av1-encode-validation.md](stage5.1b-av1-encode-validation.md).
AV1 output is Profile0, NV12, 8-bit 4:2:0 and VAAPI-only. HEVC Main10
qualification is recorded in [Stage 5.2C-2](stage5.2c2-hevc-main10-encode.md).
There is no software HEVC/AV1 encoder, AV1 10-bit encode, HDR/tone mapping, 4:2:2/4:4:4 output,
quality/preset/bitrate UI, AV1 tuning, or new GPU vendor/platform support.
