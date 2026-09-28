#!/usr/bin/env bash
set -euo pipefail

# Internal-only legal-code PQ fixtures. Existing rejection fixtures are untouched.
tool=${ASCIIFLOW_FIXTURE_FFMPEG:-ffmpeg}
out=${1:-$(dirname "$0")}
version=$("$tool" -version)
version=${version%%$'\n'*}
[[ "$version" == 'ffmpeg version 8.1.3 '* ]] || {
  echo "Requires fixed FFmpeg 8.1.3; found: $version" >&2
  exit 1
}
mkdir -p "$out"
source='nullsrc=size=128x96:rate=30,format=yuv420p10le,geq=lum=80+mod(X*7+Y*11+N*13\,841):cb=128+mod(X*17+Y*5+N*7\,769):cr=128+mod(X*3+Y*19+N*11\,769)'
common=(-hide_banner -loglevel error -nostdin -y -f lavfi -i "$source"
  -map 0:v:0 -an -frames:v 36 -r 30 -fps_mode cfr -pix_fmt yuv420p10le
  -color_primaries bt2020 -color_trc smpte2084 -colorspace bt2020nc
  -color_range tv -chroma_sample_location left -threads 1 -map_metadata -1)
"$tool" "${common[@]}" -c:v libx265 -preset ultrafast \
  -x265-params 'lossless=1:bframes=0:keyint=30:min-keyint=30:open-gop=0:pools=none:frame-threads=1:wpp=0:log-level=error' \
  -bsf:v 'hevc_metadata=video_full_range_flag=0:colour_primaries=9:transfer_characteristics=16:matrix_coefficients=9:chroma_sample_loc_type=0' \
  -fflags +bitexact -flags:v +bitexact "$out/hevc-main10-pq-qualified.mp4"
"$tool" "${common[@]}" -c:v libaom-av1 -cpu-used 8 -crf 0 -b:v 0 \
  -g 30 -bf 0 -row-mt 0 -tiles 1x1 -fflags +bitexact -flags:v +bitexact \
  -bsf:v 'av1_metadata=color_primaries=9:transfer_characteristics=16:matrix_coefficients=9:color_range=tv:chroma_sample_position=vertical' \
  "$out/av1-main10-pq-qualified.mp4"
sha256sum "$out/hevc-main10-pq-qualified.mp4" "$out/av1-main10-pq-qualified.mp4"
