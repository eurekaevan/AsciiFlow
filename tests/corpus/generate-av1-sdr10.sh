#!/usr/bin/env bash
set -euo pipefail

[[ $# == 1 ]] || { echo 'usage: generate-av1-sdr10.sh NEW_OUTPUT_FILE.mp4' >&2; exit 2; }
output=$1
input=tests/fixtures/codecs/hevc-main10-canonical-v1.mp4
expected_input_sha256=df69c98ca6592b08c6bc34127b2bb5cf4125c759fd852b830e5426d5818fccda
ffmpeg=${ASCIIFLOW_CORPUS_FFMPEG:-ffmpeg}

[[ ! -e $output ]] || { echo "Refusing to replace existing output: $output" >&2; exit 1; }
actual_input_sha256=$(sha256sum "$input" | cut -d ' ' -f1)
[[ $actual_input_sha256 == "$expected_input_sha256" ]] || {
  echo "Canonical HEVC source identity mismatch: $actual_input_sha256" >&2
  exit 1
}
version=$($ffmpeg -version)
version=${version%%$'\n'*}
[[ $version == 'ffmpeg version 8.1.3 '* ]] || {
  echo "Expected FFmpeg 8.1.3, got: $version" >&2
  exit 1
}

parent=$(dirname -- "$output")
[[ -d $parent ]] || { echo "Output directory does not exist: $parent" >&2; exit 1; }
set -o pipefail
"$ffmpeg" -nostdin -hide_banner -loglevel error -i "$input" \
  -map 0:v:0 -an -f rawvideo -pix_fmt yuv420p10le - |
  "$ffmpeg" -nostdin -hide_banner -loglevel error -y \
    -f rawvideo -pixel_format yuv420p10le -video_size 1920x1080 -framerate 50 -i - \
    -map 0:v:0 -an -frames:v 300 -r 50 -fps_mode cfr \
    -c:v libaom-av1 -threads:v 1 -cpu-used 8 -crf 0 -b:v 0 -g 50 -bf 0 \
    -row-mt 0 -tiles 1x1 -lag-in-frames 0 -auto-alt-ref 0 \
    -pix_fmt yuv420p10le -color_primaries bt709 -color_trc bt709 \
    -colorspace bt709 -color_range tv -chroma_sample_location left \
    -bsf:v 'av1_metadata=color_primaries=1:transfer_characteristics=1:matrix_coefficients=1:color_range=tv:chroma_sample_position=vertical' \
    -video_track_timescale 50000 -map_metadata -1 \
    -metadata creation_time=1970-01-01T00:00:00Z \
    -fflags +bitexact -flags:v +bitexact -movflags +faststart "$output"
