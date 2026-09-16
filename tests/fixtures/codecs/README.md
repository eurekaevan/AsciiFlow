# Video codec regression fixtures

All files are small, deterministic synthetic media generated from FFmpeg
`testsrc2` and `sine`; no third-party recordings are included. Each file is
64×64, 30 fps, 36 display frames (1.2 seconds), BT.709 limited-range 4:2:0,
with mono 48 kHz AAC audio.

| File | Contract |
| --- | --- |
| `hevc-main8-bframes.mp4` | HEVC Main, 8-bit 4:2:0; normal B pictures/reorder |
| `av1-main8-nofilmgrain.mp4` | AV1 Main, 8-bit 4:2:0; film grain disabled |
| `hevc-main10-reject.mp4` | HEVC Main 10, 10-bit 4:2:0; rejection fixture |
| `av1-main10-reject.mp4` | AV1 Main, 10-bit 4:2:0; rejection fixture |

Recreate with `sh tests/fixtures/codecs/generate.sh`. The generator accepts
only FFmpeg **8.1.2**; set `ASCIIFLOW_FIXTURE_FFMPEG` when the pinned binary is
not on `PATH`. `generator-version.txt` records the complete build/library
versions and `SHA256SUMS` records the checked-in command output hashes.
Tests consume these checked-in bytes and never require a runtime generator.

Verify existing artifacts with:

```sh
cd tests/fixtures/codecs && sha256sum -c SHA256SUMS
```
