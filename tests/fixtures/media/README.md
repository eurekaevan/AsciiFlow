# Audio regression fixtures

All media is self-generated from FFmpeg test patterns and sine waves; no third-party
recordings are included. `subtitle.srt` is an original test caption, not a supported
output feature. The directory is approximately 2 MB, including the 60-second stress
sample. Normal fixtures are 64×64 H.264, 30 fps, with 48 kHz mono AAC unless noted.

| File | Contract |
| --- | --- |
| single.mp4 | F1: three seconds, Japanese/default AAC |
| multiple.mp4 | F2: Japanese/default and English/non-default, distinct tones |
| no-audio.mp4 | F3: video only |
| offset.mp4 | F4: audio delayed 500 ms, MP4 edit-list timeline |
| audio-longer.mp4 | F5: two-second video, three-second audio |
| video-longer.mp4 | F6: three-second video, two-second audio |
| incompatible.mkv | F7: PCM mu-law, rejected by the linked MP4 capability query |
| extra-stream.mp4 | F8: additional mov_text subtitle, ignored by conversion |
| mixed.mkv | Compatible AAC plus incompatible PCM mu-law |
| audio-only.mp4 | No-video rejection |
| discontinuous.mp4 | Video timestamp gap with continuous audio |
| short-video.mp4 | Two video frames plus three seconds of audio |
| long.mp4 | Sixty-second cancellation and opt-in completion stress |
| short.mp4, two-video.mp4 | Small generator intermediates |

Recreate with `sh tests/fixtures/media/generate.sh`. The generator accepts only
FFmpeg **8.1.2**; set `ASCIIFLOW_FIXTURE_FFMPEG` to that executable. The project's
9.0.1 library toolchain does not build the lavfi CLI fixture generator, so generation
uses this separate explicit pin. `generator-version.txt` records the actual build
and library versions; the script is the complete command manifest. Encoder build
differences may change bytes; review and regenerate `SHA256SUMS` deliberately.
Verify existing artifacts with `cd tests/fixtures/media && sha256sum -c SHA256SUMS`.
Tests consume checked-in bytes and linked libav APIs, never an arbitrary system CLI.
