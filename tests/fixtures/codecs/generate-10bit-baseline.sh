#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ffmpeg_bin=${ASCIIFLOW_BASELINE_FFMPEG:-ffmpeg}
python_bin=${ASCIIFLOW_BASELINE_PYTHON:-python3}
output=${1:-"$script_dir/hevc-main10-canonical-v1.mp4"}

version=$("$ffmpeg_bin" -version | head -n 1)
case "$version" in
  'ffmpeg version 8.1.3 '*) ;;
  *) echo "Expected canonical-baseline generator FFmpeg 8.1.3, got: $version" >&2; exit 1 ;;
esac

temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
"$python_bin" "$script_dir/generate-10bit-baseline.py" "$temporary/source.yuv"

"$ffmpeg_bin" -hide_banner -loglevel error -y -threads 1 \
  -f rawvideo -pixel_format yuv420p10le -video_size 192x108 -framerate 50 \
  -i "$temporary/source.yuv" -map 0:v:0 -an -frames:v 300 \
  -vf 'scale=1920:1080:flags=neighbor,format=yuv420p10le' \
  -r 50 -fps_mode cfr -c:v libx265 -preset ultrafast -crf 18 \
  -x265-params 'bframes=0:keyint=50:min-keyint=50:open-gop=0:pools=none:frame-threads=1:wpp=0:log-level=error' \
  -pix_fmt yuv420p10le -color_range tv -color_primaries bt709 \
  -color_trc bt709 -colorspace bt709 -chroma_sample_location left \
  -bsf:v 'hevc_metadata=video_full_range_flag=0:colour_primaries=1:transfer_characteristics=1:matrix_coefficients=1' \
  -video_track_timescale 50000 -map_metadata -1 -metadata creation_time=1970-01-01T00:00:00Z \
  -movflags +faststart "$output"
