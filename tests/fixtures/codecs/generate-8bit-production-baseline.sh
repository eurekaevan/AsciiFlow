#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 OUTPUT.mp4" >&2
  exit 2
fi

generator=${ASCIIFLOW_BASELINE_FFMPEG:-ffmpeg}
version=$("$generator" -version)
version=${version%%$'\n'*}
if [[ $version != "ffmpeg version 8.1.3 "* ]]; then
  echo "Expected FFmpeg 8.1.3, got: $version" >&2
  exit 1
fi

"$generator" -nostdin -hide_banner -loglevel error -y -threads 1 \
  -f lavfi -i 'testsrc2=size=1920x1080:rate=50:duration=6' \
  -map 0:v:0 -frames:v 300 -an \
  -c:v libx264 -threads:v 1 -preset ultrafast -tune zerolatency \
  -crf 20 -bf 0 -g 250 \
  -pix_fmt yuv420p -r 50 -fps_mode cfr \
  -color_primaries bt709 -color_trc bt709 -colorspace bt709 -color_range tv \
  -bsf:v 'h264_metadata=video_full_range_flag=0:colour_primaries=1:transfer_characteristics=1:matrix_coefficients=1' \
  -video_track_timescale 90000 -movflags +faststart \
  "$1"
