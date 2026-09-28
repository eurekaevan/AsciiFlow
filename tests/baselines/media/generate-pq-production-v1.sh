#!/usr/bin/env bash
set -euo pipefail

[[ $# == 2 ]] || { echo 'usage: generate-pq-production-v1.sh CANONICAL_INPUT_DIR OUTPUT_DIR' >&2; exit 2; }
input_dir=$1
output_dir=$2
binary=${ASCIIFLOW_BASELINE_BINARY:-target/release/asciiflow}
device=/dev/dri/renderD128
[[ -c "$device" && -x "$binary" ]] || { echo 'Missing render node or production binary' >&2; exit 1; }
mkdir -p "$output_dir"
for codec in hevc av1; do
  input="$input_dir/$codec-main10-pq-canonical-v1.mp4"
  [[ -f "$input" ]] || { echo "Missing canonical input: $input" >&2; exit 1; }
  for run in 1 2 3; do
    output="$output_dir/$codec-10-run$run.mp4"
    [[ ! -e "$output" ]] || { echo "Refusing to replace $output" >&2; exit 1; }
    /usr/bin/time -f 'wall_seconds=%e user_seconds=%U system_seconds=%S cpu_percent=%P peak_kib=%M' \
      -o "$output_dir/$codec-10-run$run.time" \
      "$binary" "$input" "$output" \
      --width 80 --charset standard --font builtin-8x8 --color true \
      --audio none --max-frames 300 --decode vaapi --backend vulkan \
      --vulkan-mapping gpu --encode vaapi --hw-device "$device" \
      --vaapi-vulkan-input-interop on --vaapi-vulkan-output-interop on \
      --output-codec "$codec" --output-bit-depth 10 --no-progress \
      > "$output_dir/$codec-10-run$run.log" 2>&1
    sha256sum "$output"
  done
done
