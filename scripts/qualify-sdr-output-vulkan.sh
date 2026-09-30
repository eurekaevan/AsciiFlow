#!/usr/bin/env bash
set -euo pipefail

[[ $# == 1 ]] || { echo "usage: $0 NEW_OUTPUT_DIR" >&2; exit 2; }
output_dir=$1
[[ ! -e $output_dir ]] || { echo "qualification output must not already exist" >&2; exit 2; }
mkdir -p "$output_dir"
output_dir=$(cd "$output_dir" && pwd)
repository=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repository"

bash scripts/qualify-tone-map-cpu.sh "$output_dir/c1"
bash scripts/qualify-target-volume-cpu.sh "$output_dir/c1/method-a-run1.bin" "$output_dir/c2b"
cargo test -p asciiflow-cpu sdr_output

export ASCIIFLOW_VULKAN_VALIDATION=1
C4A_BOUNDARY_REPORT="$output_dir/boundary.json" \
  cargo test -p asciiflow-vulkan --release --features hdr-to-sdr-qualification \
  --test c4a_quantization -- --ignored --nocapture --test-threads=1
C1_INPUT="$output_dir/c1/linear-bt2020-1000-v1.bin" \
C4A_CANONICAL_REPORT="$output_dir/canonical.json" \
  cargo test -p asciiflow-vulkan --release --features hdr-to-sdr-qualification \
  --test c4a_canonical -- --ignored --nocapture --test-threads=1
cargo test -p asciiflow-interop --release --features hdr-to-sdr-qualification \
  --test c4a_sdr_output -- --ignored --nocapture --test-threads=1
C4A_STRESS_REPORT="$output_dir/stress.json" \
  cargo test -p asciiflow-interop --release --features hdr-to-sdr-qualification \
  --test c4a_stress -- --ignored --nocapture --test-threads=1

# Timings follow correctness, with no competing qualification jobs dispatched.
ASCIIFLOW_VULKAN_VALIDATION=0 C4A_PERFORMANCE_REPORT="$output_dir/performance.json" \
  cargo test -p asciiflow-interop --release --features hdr-to-sdr-qualification \
  --test c4a_performance -- --ignored --nocapture --test-threads=1

echo "Pixel/surface qualification finished; production retained regressions and"
echo "workspace/SPIR-V checks remain independent closure obligations. No encoding"
echo "or production HDR-to-SDR availability is implied by this runner."
