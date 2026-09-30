#!/usr/bin/env bash
set -euo pipefail

[[ $# == 2 ]] || { echo 'usage: generate-hdr-to-sdr-production-v1.sh C3_INPUT_DIR OUTPUT_DIR' >&2; exit 2; }
input_dir=$1
output_dir=$2
binary=${ASCIIFLOW_BASELINE_BINARY:-target/release/asciiflow}
device=/dev/dri/renderD128
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
[[ -c "$device" && -x "$binary" ]] || { echo 'Missing render node or production binary' >&2; exit 1; }
python3 "$script_dir/verify-hdr-to-sdr-production.py" identity "$input_dir"
mkdir -p "$output_dir"
# Preflight the complete matrix before starting a costly run or replacing evidence.
for source in hevc av1; do
  for profile in h264-8 hevc-8 av1-8 hevc-10 av1-10; do
    for run in 1 2 3; do
      stem="$output_dir/$source-to-$profile-run$run"
      for suffix in mp4 time log command.json success.json; do
        [[ ! -e "$stem.$suffix" ]] || { echo "Refusing to replace $stem.$suffix" >&2; exit 1; }
      done
    done
  done
done
for source in hevc av1; do
  input="$input_dir/$source-main10-pq-c3-legal-v1.mp4"
  for profile in h264-8 hevc-8 av1-8 hevc-10 av1-10; do
    codec=${profile%-*}
    depth=${profile##*-}
    for run in 1 2 3; do
      stem="$output_dir/$source-to-$profile-run$run"
      command=("$binary" "$input" "$stem.mp4"
        --width 80 --charset standard --font builtin-8x8 --color true
        --audio none --max-frames 300 --decode vaapi --backend vulkan
        --vulkan-mapping gpu --encode vaapi --hw-device "$device"
        --vaapi-vulkan-input-interop on --vaapi-vulkan-output-interop on
        --output-dynamic-range sdr --output-codec "$codec" --output-bit-depth "$depth" --no-progress)
      python3 "$script_dir/verify-hdr-to-sdr-production.py" record "$stem.command.json" -- "${command[@]}"
      /usr/bin/time -f 'wall_seconds=%e user_seconds=%U system_seconds=%S cpu_percent=%P peak_kib=%M' \
        -o "$stem.time" "${command[@]}" > "$stem.log" 2>&1
      python3 "$script_dir/verify-hdr-to-sdr-production.py" success "$stem.command.json" "$stem.mp4" "$stem.success.json"
      sha256sum "$stem.mp4"
    done
  done
done
