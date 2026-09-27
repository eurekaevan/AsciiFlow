#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 INPUT8.mp4 INPUT10.mp4 OUTPUT_DIR" >&2
  exit 2
fi

input8=$1
input10=$2
output_dir=$3
binary=${ASCIIFLOW_BASELINE_BINARY:-target/release/asciiflow}
device=/dev/dri/renderD128

[[ -f $input8 && -f $input10 && -c $device && -x $binary ]] || {
  echo "Missing input, Intel render node, or Release AsciiFlow binary" >&2
  exit 1
}
mkdir -p "$output_dir"

for profile in h264-8 hevc-8 av1-8 hevc-10 av1-10; do
  codec=${profile%-*}
  depth=${profile##*-}
  input=$input8
  if [[ $depth == 10 ]]; then
    input=$input10
  fi
  for run in 1 2 3; do
    output="$output_dir/$profile-run$run.mp4"
    "$binary" "$input" "$output" \
      --width 80 --charset standard --font builtin-8x8 --color true \
      --audio none --max-frames 300 --decode vaapi --backend vulkan \
      --vulkan-mapping gpu --encode vaapi --hw-device "$device" \
      --vaapi-vulkan-input-interop on --vaapi-vulkan-output-interop on \
      --output-codec "$codec" --output-bit-depth "$depth" --no-progress \
      >"$output_dir/$profile-run$run.log" 2>&1
    sha256sum "$output"
  done
done
