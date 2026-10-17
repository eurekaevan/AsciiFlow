# Container input regression fixtures

These are synthetic FFmpeg `testsrc2` pictures and a 440 Hz `sine`; no
third-party recordings are included. Each video has 30 display frames,
64×64 geometry, 30 fps, 8-bit 4:2:0 samples, and no B pictures. Encoding
requests limited-range BT.709, except VP8's BT.601 signal. VP8 uses matching
SMPTE 170M container tags because its decoder reports a BT.601 matrix.
Formats without complete color metadata use
the existing legacy SDR resolution and canonical BT.709 limited NV12 output.

Generate with `sh tests/fixtures/containers/generate.sh`. The script requires
FFmpeg 8.1.3, independent of the older codec-fixture generator pin. The full
local build version is in `FFMPEG_VERSION.txt`; native encoders/libvpx,
libvorbis, libopus and libmp3lame are generation-only tools. Runtime decoding
does not require those external encoder libraries. `SHA256SUMS` records the
committed files. Two runs on the recorded build produced identical hashes;
different external encoder versions/builds may produce different bytes.

| File | Video/audio | Decoded video PTS | Audio metadata |
| --- | --- | --- | --- |
| `h264-aac.mkv` | H.264 / AAC | 0, 33, 67, …, 967; 1/1000 ticks | `eng`, default |
| `mpeg4-mp3.avi` | MPEG-4 Part 2 / MP3 | 0, 2, 3, …, 30; 1/30 ticks | No language/default preservation |
| `vp8-vorbis.webm` | VP8 / Vorbis | 0, 33, 67, …, 967; 1/1000 ticks | `eng`, default |
| `vp9-opus.webm` | VP9 / Opus | 0, 33, 67, …, 967; 1/1000 ticks | `eng`, default |

PTS identities were read independently using `ffprobe -select_streams v
-show_entries frame=best_effort_timestamp`. AVI has an initial timestamp gap
from muxing MP3 encoder delay and reports 1.033333 seconds of video; this is
intentional input evidence, not a constant-step assumption. AVI also discards
the requested language/default audio metadata. Tests must retain the media's
actual facts rather than invent metadata or infer frame count from duration.

This set qualifies software decoding of these particular 8-bit SDR inputs.
It does not establish new hardware decode, HDR, 10-bit VP9, non-4:2:0, MJPEG,
full-range processing, or every audio/container combination.

`codec_decode` checks all source frame PTS and normalized NV12 output.
`audio_regression::container_inputs_convert_to_h264_with_explicit_audio_policies`
converts all four samples with `none`, `copy`, and `auto`, checks 30 H.264
output frames, and decodes each copied audio track. Copy/Auto preserve codec,
sample rate, channel count, packet count, payload and order. Per-packet PTS/DTS
error must not exceed one input plus one output time-base tick.
AVI audio has a 3/125-second time base, so its bound is about 24 ms, not
sample precision. The initial AVI video gap is retimed by the existing CFR
output rule. This cross-container contract does not replace or weaken the
existing strict MP4 language/default/duration and timing oracle.
