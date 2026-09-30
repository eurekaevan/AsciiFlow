#!/usr/bin/env bash
set -euo pipefail

[[ $# == 2 ]] || { echo 'usage: measure-hdr-to-sdr-production.sh C3_INPUT_DIR NEW_OUTPUT_DIR' >&2; exit 2; }
input_dir=$1
output_dir=$2
binary=${ASCIIFLOW_BASELINE_BINARY:-target/release/asciiflow}
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
[[ -c /dev/dri/renderD128 && -x "$binary" ]] || { echo 'Missing qualified device or measured binary' >&2; exit 1; }
python3 "$script_dir/verify-hdr-to-sdr-production.py" identity "$input_dir"
mkdir -- "$output_dir"
# Dedicated measurements follow correctness. No competing GPU jobs or oracle
# decode jobs should run. The measured build exposes encoder/mux CPU subscopes.
export ASCIIFLOW_VULKAN_VALIDATION=0
for case in hevc-h264-8 hevc-hevc-8 hevc-av1-8 hevc-hevc-10 hevc-av1-10 av1-hevc-10; do
  source=${case%%-*}
  profile=${case#*-}
  codec=${profile%-*}
  depth=${profile##*-}
  input="$input_dir/$source-main10-pq-c3-legal-v1.mp4"
  for run in 1 2 3; do
    stem="$output_dir/$case-run$run"
    command=("$binary" "$input" "$stem.mp4"
      --width 80 --charset standard --font builtin-8x8 --color true
      --audio none --max-frames 300 --decode vaapi --backend vulkan
      --vulkan-mapping gpu --encode vaapi --hw-device /dev/dri/renderD128
      --vaapi-vulkan-input-interop on --vaapi-vulkan-output-interop on
      --output-dynamic-range sdr --output-codec "$codec" --output-bit-depth "$depth" --no-progress)
    python3 "$script_dir/verify-hdr-to-sdr-production.py" record "$stem.command.json" -- "${command[@]}"
    /usr/bin/time -f 'wall_seconds=%e user_seconds=%U system_seconds=%S cpu_percent=%P peak_kib=%M' \
      -o "$stem.time" "${command[@]}" > "$stem.log" 2>&1
    python3 "$script_dir/verify-hdr-to-sdr-production.py" success "$stem.command.json" "$stem.mp4" "$stem.success.json"
    sha256sum "$stem.mp4"
  done
done
