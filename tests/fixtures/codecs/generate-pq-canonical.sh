#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
tool=${ASCIIFLOW_FIXTURE_FFMPEG:-ffmpeg}
python=${ASCIIFLOW_FIXTURE_PYTHON:-python3}
out=${1:-"$script_dir"}
version=$("$tool" -version)
first_line=${version%%$'\n'*}
[[ "$first_line" == 'ffmpeg version 8.1.3 '* ]] || {
  echo "Requires canonical PQ generator FFmpeg 8.1.3; found: $first_line" >&2
  exit 1
}
ffmpeg_path=$(readlink -f "$(command -v "$tool")")
linked=$(ldd "$ffmpeg_path")
[[ "$linked" == *libx265.so* && "$linked" == *libaom.so* ]] || {
  echo "Canonical PQ generation requires identifiable shared libx265 and libaom" >&2
  exit 1
}
names=(hevc-main10-pq-canonical-v1.mp4 av1-main10-pq-canonical-v1.mp4
  pq-canonical-v1-identity.json pq-canonical-v1-generator-version.txt)
mkdir -p "$out"
for name in "${names[@]}"; do
  [[ ! -e "$out/$name" ]] || { echo "Refusing to replace existing $out/$name" >&2; exit 1; }
done
temporary=$(mktemp -d "$out/.pq-canonical-v1.XXXXXX")
trap 'rm -rf "$temporary"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
"$tool" -version > "$temporary/pq-canonical-v1-generator-version.txt"
"$python" "$script_dir/generate-pq-canonical.py" generate \
  "$temporary/source.yuv" "$temporary/pq-canonical-v1-identity.json"

# Both lossless encoders read exactly the same full-resolution ten-bit source.
common=(-hide_banner -loglevel error -nostdin -y -threads 1
  -f rawvideo -pixel_format yuv420p10le -video_size 1920x1080 -framerate 50
  -color_primaries bt2020 -color_trc smpte2084 -colorspace bt2020nc
  -color_range tv -chroma_sample_location left
  -i "$temporary/source.yuv" -map 0:v:0 -an -frames:v 300
  -r 50 -fps_mode cfr -pix_fmt yuv420p10le
  -color_primaries bt2020 -color_trc smpte2084 -colorspace bt2020nc
  -color_range tv -chroma_sample_location left -threads 1 -map_metadata -1
  -metadata creation_time=1970-01-01T00:00:00Z -video_track_timescale 50000
  -fflags +bitexact -flags:v +bitexact)
"$tool" "${common[@]}" -c:v libx265 -preset ultrafast \
  -x265-params 'lossless=1:bframes=0:keyint=50:min-keyint=50:open-gop=0:pools=none:frame-threads=1:wpp=0:log-level=error' \
  -bsf:v 'hevc_metadata=video_full_range_flag=0:colour_primaries=9:transfer_characteristics=16:matrix_coefficients=9:chroma_sample_loc_type=0' \
  -movflags +faststart "$temporary/hevc-main10-pq-canonical-v1.mp4"
"$tool" "${common[@]}" -c:v libaom-av1 -cpu-used 8 -crf 0 -b:v 0 \
  -g 50 -bf 0 -row-mt 0 -tiles 1x1 -lag-in-frames 0 -auto-alt-ref 0 \
  -bsf:v 'av1_metadata=color_primaries=9:transfer_characteristics=16:matrix_coefficients=9:color_range=tv:chroma_sample_position=vertical' \
  -movflags +faststart "$temporary/av1-main10-pq-canonical-v1.mp4"
"$python" "$script_dir/generate-pq-canonical.py" finalize \
  "$temporary/pq-canonical-v1-identity.json" "$ffmpeg_path" \
  "$temporary/pq-canonical-v1-generator-version.txt" \
  "$temporary/hevc-main10-pq-canonical-v1.mp4" "$temporary/av1-main10-pq-canonical-v1.mp4"

# Publish only completed files. Hard links reserve each final name without
# overwriting a file created after the initial existence check.
for name in "${names[@]}"; do
  ln -- "$temporary/$name" "$out/$name"
done
sha256sum "$out/${names[0]}" "$out/${names[1]}" "$out/${names[2]}"
