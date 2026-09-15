#!/bin/sh
set -eu
# Generation-only toolchain pin. The runtime test suite uses linked libav*.
tool=${ASCIIFLOW_FIXTURE_FFMPEG:-ffmpeg}
version=$($tool -version | head -n 1)
case "$version" in 'ffmpeg version 8.1.2 '*) ;; *) echo "Expected fixture generator FFmpeg 8.1.2, got: $version" >&2; exit 1;; esac
cd "$(dirname "$0")"
video='testsrc2=size=64x64:rate=30:duration=3'
audio='sine=frequency=440:sample_rate=48000:duration=3'
run() { "$tool" -hide_banner -loglevel error -y -threads 1 "$@"; }
run -f lavfi -i "$video" -f lavfi -i "$audio" -map 0:v -map 1:a -c:v libx264 -threads:v 1 -bf 0 -c:a aac -metadata:s:a:0 language=jpn -disposition:a:0 default single.mp4
run -i single.mp4 -f lavfi -i sine=frequency=880:sample_rate=48000:duration=3 -map 0:v -map 0:a -map 1:a -c:v copy -c:a aac -metadata:s:a:0 language=jpn -metadata:s:a:1 language=eng -disposition:a:0 default -disposition:a:1 0 multiple.mp4
run -i single.mp4 -map 0:v -c copy no-audio.mp4
run -i single.mp4 -itsoffset 0.5 -i single.mp4 -map 0:v -map 1:a -c copy offset.mp4
run -i single.mp4 -map 0:v -map 0:a -c copy -t 2 short.mp4
run -i short.mp4 -i single.mp4 -map 0:v -map 1:a -c copy audio-longer.mp4
run -i single.mp4 -i short.mp4 -map 0:v -map 1:a -c copy video-longer.mp4
run -i single.mp4 -map 0:v -map 0:a -c:v copy -c:a pcm_mulaw incompatible.mkv
run -i single.mp4 -i incompatible.mkv -map 0:v -map 0:a -map 1:a -c copy mixed.mkv
run -i single.mp4 -map 0:a -c copy audio-only.mp4
run -i single.mp4 -vf 'setpts=PTS+gte(N\,30)*0.5/TB' -fps_mode passthrough -c:v libx264 -threads:v 1 -bf 0 -c:a copy discontinuous.mp4
run -i single.mp4 -map 0:v -frames:v 2 -an -c copy two-video.mp4
run -i two-video.mp4 -i single.mp4 -map 0:v -map 1:a -c copy short-video.mp4
run -i single.mp4 -f srt -i subtitle.srt -map 0:v -map 0:a -map 1:s -c copy -c:s mov_text extra-stream.mp4
run -f lavfi -i testsrc2=size=64x64:rate=30:duration=60 -f lavfi -i sine=frequency=440:sample_rate=48000:duration=60 -c:v libx264 -threads:v 1 -bf 0 -c:a aac -metadata:s:a:0 language=jpn long.mp4
sha256sum ./*.mp4 ./*.mkv > SHA256SUMS
"$tool" -version > generator-version.txt
