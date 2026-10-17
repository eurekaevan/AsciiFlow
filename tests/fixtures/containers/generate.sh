#!/bin/sh
set -eu

# Generation-only pin; runtime tests use the linked FFmpeg libraries.
tool=${ASCIIFLOW_FIXTURE_FFMPEG:-ffmpeg}
version=$("$tool" -version | head -n 1)
case "$version" in 'ffmpeg version 8.1.3 '*) ;; *)
  echo "Expected fixture generator FFmpeg 8.1.3, got: $version" >&2
  exit 1
  ;;
esac
cd "$(dirname "$0")"

generate() {
  "$tool" -hide_banner -loglevel error -y \
    -f lavfi -i 'testsrc2=size=64x64:rate=30:duration=1' \
    -f lavfi -i 'sine=frequency=440:sample_rate=48000:duration=1' \
    -map 0:v -map 1:a -threads:v 1 -threads:a 1 \
    -pix_fmt yuv420p -color_primaries bt709 -color_trc bt709 \
    -colorspace bt709 -color_range tv -g 30 -bf 0 \
    -metadata:s:a:0 language=eng -disposition:a:0 default \
    -fflags +bitexact -flags:v +bitexact -flags:a +bitexact "$@"
}

generate -c:v libx264 -preset ultrafast -crf 28 -c:a aac -b:a 64k h264-aac.mkv
generate -c:v mpeg4 -q:v 6 -c:a libmp3lame -b:a 64k mpeg4-mp3.avi
generate -c:v libvpx -deadline best -cpu-used 0 -b:v 100k \
  -color_primaries smpte170m -color_trc smpte170m -colorspace smpte170m \
  -c:a libvorbis -q:a 2 vp8-vorbis.webm
generate -c:v libvpx-vp9 -deadline best -cpu-used 0 -b:v 100k \
  -c:a libopus -b:a 48k vp9-opus.webm

"$tool" -version > FFMPEG_VERSION.txt
sha256sum h264-aac.mkv mpeg4-mp3.avi vp8-vorbis.webm vp9-opus.webm > SHA256SUMS
