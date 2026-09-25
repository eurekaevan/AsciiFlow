# Stage 5.2C-1: P010 Vulkan → VAAPI output interop qualification

This historical stage qualifies an internal output transfer, **not** a
production 10-bit encoder. At its seal, H.264, HEVC and AV1 production outputs
remained NV12 8-bit, and the CLI rejected 10-bit transcode before staging. No
HDR, 10→8 conversion, codec policy or encoder context was added here.
[Stage 5.2C-2](stage5.2c2-hevc-main10-encode.md) subsequently connected
this generic P010 interop to an encoder-owned HEVC Main10 surface.

## Surface and format evidence

The opt-in `p010-output-diagnostic` feature creates an encoder-style VAAPI
`AVHWFramesContext` with software format P010LE, but no encoder. The pool owns
the VAAPI device, yields hardware frames, and supports P010 upload and
hwdownload for diagnostics. On Intel Arc Meteor Lake (iHD/ANV), direct writable
DRM PRIME export of its acquired surfaces produced:

| Geometry | Objects / layers | Object size | Modifier | Y | UV |
| --- | ---: | ---: | --- | --- | --- |
| 64×64 | 1 / 2 | 16,384 B | `0x0100000000000009` | R16, plane 0, offset 0, pitch 128 | GR32, plane 0, offset 8,192, pitch 128 |
| 1920×1080 | 1 / 2 | 6,389,760 B | `0x0100000000000009` | R16, plane 0, offset 0, pitch 3,840 | GR32, plane 0, offset 4,177,920, pitch 3,840 |

The 64×64 HEVC Main10 and AV1 10-bit **decode** surfaces observed in Stage
5.2B, and re-probed here, also had one object, two single-plane R16/GR32
layers, this modifier, 128-byte pitch and 0/8,192 offsets. Thus the observed
64×64 input and output descriptors match in every listed field. This is a
device/driver observation, not an assumption in the mapping code; the 1080p
output has alignment padding and its UV offset is **not** the packed buffer's
Y byte length (4,147,200 B). The descriptor parser uses each actual object,
layer, modifier, pitch and offset to build external images. Fourcc maps to
`VK_FORMAT_R16_UNORM` / `VK_FORMAT_R16G16_UNORM` solely for raw transfer, not
normalized sampling. The packed buffer remains width×2 stride for both Y and
UV, with P010LE code bits 15..6 and zero padding bits 5..0.

## Ownership and transfer

The Stage 3B two-slot worker, FIFO completion, encoder-style surface ownership
and per-frame import are shared by NV12 and P010. Pixel format selects the
descriptor's R8/GR88 or R16/GR32 plane mapping; no codec selects the interop
path. FFmpeg/VAAPI retains the original DRM object FD. The interop layer makes
`F_DUPFD_CLOEXEC` copies for Vulkan; imported copies are consumed by Vulkan,
and temporary handles are dropped after the waited submit. Each imported
image transitions `FOREIGN_EXT → TRANSFER_DST → FOREIGN_EXT`, with
`TRANSFER_WRITE` covered by the release barrier. The output buffer is made
transfer-readable after either compute shader writes or the diagnostic raw
upload's transfer write. Y and UV use separate buffer-to-image regions with
actual image geometry; the second packed buffer offset is based on the packed
format, not external pitch. The synchronous queue/fence completion precedes
Vulkan-image destruction and VAAPI hwdownload. Worker teardown joins in-flight
slots before releasing surfaces. Existing hard-failure behavior remains in
place; no P010-specific device-lost bypass was added.

The diagnostic writable import and `TRANSFER_DST` capability succeeded on the
observed Intel modifier for both planes. Capability is format-specific: NV12
and P010 can separately be Supported, Unsupported(reason) or NotProbed(reason).
This fact does not feed the production planner; an encoder-specific 10-bit
qualification still remains necessary.

## Correctness and lifetime

On `/dev/dri/renderD128`, four deterministic 64×64 packed patterns (gradient,
checkerboard, seeded random and low-active-bit pattern) survived raw Vulkan
buffer→external-image→VAAPI hwdownload byte-for-byte. The patterns include
10-bit codes not divisible by four; every downloaded word retained zero low
six padding bits. Thirty 128×64 synthetic P010 frames passed
CPU renderer == Vulkan output == VAAPI hwdownload byte-for-byte, with exact
PTS/FIFO order and two-slot EOF drain. Five more frames passed with the
Inconsolata FreeType atlas. A further 30-frame real HEVC Main10
VAAPI-decode→DRM input→Vulkan processing→DRM output→VAAPI hwdownload test
matched CPU pixels byte-for-byte, with ordered output sequence numbers.
The 3000-frame two-slot stress compared every
downloaded frame to CPU output, including frame order; the final Validation
rerun's FD counts were before=4, early=21, max=22, after=4. With
`ASCIIFLOW_VULKAN_VALIDATION=1`, both 30-frame and 3000-frame runs reported
zero validation errors (including VUID/synchronization messages).
Validation initially exposed a missing `TRANSFER_DST` usage bit on the
diagnostic raw-upload destination buffer. The usage declaration was corrected
and the raw patterns, 30-frame parity and 3000-frame stress all passed again
with Validation enabled; the initial failed run is not counted as qualification.

An invalid output DMA-BUF import preserved the DMA-BUF root error and leaked
no duplicated FD. Dropping the processor with two submissions outstanding
restored the exact FD baseline. The closure pass added feature-gated, one-shot
diagnostic checkpoints at external-image creation, partial memory import,
output queue submission and post-fence completion. Each P010 two-slot failure
preserved its named cause, returned to the exact FD baseline after teardown,
rejected subsequent submissions, and reported zero Validation errors. The
image-create and memory-import checkpoints test cleanup at those lifecycle
boundaries; they do **not** pretend to be actual `vkCreateImage` or
`vkAllocateMemory` driver failures. The invalid-FD case separately exercises
a real DMA-BUF import rejection. Queue-submit injection occurs before the
output submission, and the fence checkpoint only after the real fence has
signaled: neither leaves possibly in-flight GPU work to be destroyed. The
Stage 4.1 CLI hooks cover initialization/replan, not these runtime Vulkan
boundaries. Actual device loss, a real unsignaled-fence timeout, descriptor
export failure and FD-duplication failure were not induced.

The existing 30-frame HEVC Main10/AV1 P010 input test passed after this change.
The existing H.264, HEVC and AV1 NV12 Stage 3B pre-encode-pixel tests each
passed (30 frames, including staged upload parity). In the closure pass, the
current Release CLI regenerated all three 300-frame NV12 MP4s from the
retained input (SHA-256
`6b47b510c4a8f604e6404b1fbbc5f68bdaa77043976acdad0153ef83524b6e34`).
All used explicit VAAPI decode/encode, both Vulkan interop directions, ASCII
width 80, built-in font, color on and audio none. Each new SHA-256 exactly
matched its retained Stage 5.1B output:

| Codec | Current SHA-256, equal to retained |
| --- | --- |
| H.264 NV12 | `3181fa9775403a2f30e60157797adf370927b4196c960f59d1b5084203d42bef` |
| HEVC NV12 | `d4b13786c968e43a502845df46d92dc2dbc951e5e2ef69e4313d5c541664208b` |
| AV1 NV12 | `f044a327c093b8489e03f20c98deab8f80e99f01092fe35595b4f22796c7c534` |

## Output-transfer benchmark

Release-mode Intel Arc, 1920×1080, ASCII width 80, three separate 300-frame
runs, without Validation. A uses Vulkan Host readback then VAAPI P010
hwupload; B uses the two-slot Vulkan output→VAAPI surface path. The wall
figures include their respective processing pipeline and cannot be interpreted
as an isolated DMA transfer speedup. CPU percentages are process CPU time /
wall time, so multi-threaded values may exceed 100%. Per-component values
are per-frame means within each run; the table shows the median run value.

| Metric | P010 staged A | P010 interop B | NV12 interop control |
| --- | ---: | ---: | ---: |
| Pipeline wall | 26.410 ms | 1.836 ms | 1.409 ms |
| Process CPU | 89% | 121% | 84% |
| Host/GPU readback scope | 1.571 ms | — | — |
| VAAPI hwupload wall | 12.747 ms | — | — |
| Output DRM map CPU wall | — | 0.015 ms | — |
| External image create/import/bind CPU wall | — | 0.023 ms | 0.023 ms |
| GPU buffer→image timestamp | — | 0.196 ms | 0.175 ms |
| Output queue submit wall | — | 0.114 ms | — |
| Output fence wait wall | — | 0.574 ms | — |

The three P010 B wall values were 1.942/1.836/1.817 ms, versus NV12 control
1.409/1.396/1.474 ms. P010 copies twice NV12's packed byte volume; this
moderate increase is not pathological. GPU timestamps, CPU scopes and fence
wait overlap and must **not** be added. The B setup starts from a Host P010
test frame because no production 10-bit decoder→encoder pipeline existed at
this stage's seal; it
still writes the processing output buffer directly to VAAPI with no Host
readback/hwupload. NV12 control uses H.264-owned NV12 frames but never encodes
them. Neither number is codec encode FPS or an auto-policy recommendation.

## Reproduction and limit

```bash
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop \
  --features p010-output-diagnostic --test hardware p010_output_ -- --ignored --nocapture
cargo test --release -p asciiflow-interop \
  --features p010-output-diagnostic --test hardware \
  p010_output_300_frame_benchmark -- --ignored --nocapture
```

The first filter includes an intentionally invalid DMA-BUF test; run that
test **without** Validation if collecting a clean Validation log. A P010
production encoder has not been created, and encoder acceptance/retention of
these diagnostic surfaces is unproven. Stage 5.2C-2 can reuse this interop
architecture, but must separately qualify actual HEVC Main10 encoder frames,
their descriptors, bitstream, lifecycle and policy before any production gate
opens.

The Release workspace build, workspace tests, strict Clippy, formatting and
`git diff --check` passed. No shader source changed. For the closure pass,
system-wide package installation required an unavailable administrator
password, so the signature-verified Fedora `spirv-tools` RPM was unpacked
under `/tmp`; its `spirv-val` (2026.1) validated all 133 SPIR-V files
present under the debug/Release build directories with
`--target-env vulkan1.3`, zero errors. This includes the freshly rebuilt
artifacts; older retained build directories were validated too.
