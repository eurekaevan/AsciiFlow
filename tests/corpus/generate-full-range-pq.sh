#!/usr/bin/env bash
set -euo pipefail

[[ $# == 1 ]] || { echo 'usage: generate-full-range-pq.sh NEW_OUTPUT_FILE.mp4' >&2; exit 2; }
output=$1
input=tests/fixtures/codecs/hevc-main10-pq-c3-legal-v1.mp4
expected_input_sha256=eb8d8c42cd99369b99031900e8861c08f611bfcc662b8d0eb741eb695a0c366a
ffmpeg=${ASCIIFLOW_CORPUS_FFMPEG:-ffmpeg}

[[ ! -e $output ]] || { echo "Refusing to replace existing output: $output" >&2; exit 1; }
actual_input_sha256=$(sha256sum "$input" | cut -d ' ' -f1)
[[ $actual_input_sha256 == "$expected_input_sha256" ]] || {
  echo "C3 source identity mismatch: $actual_input_sha256" >&2
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
"$ffmpeg" -nostdin -hide_banner -loglevel error -y \
  -i "$input" -map 0:v:0 -an -c:v copy \
  -color_range pc -bsf:v 'hevc_metadata=video_full_range_flag=1' \
  -video_track_timescale 50000 -movflags +faststart \
  "$output"
