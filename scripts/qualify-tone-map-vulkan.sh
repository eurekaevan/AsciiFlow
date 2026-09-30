#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
out=${1:?Usage: qualify-tone-map-vulkan.sh NEW_OUTPUT_DIRECTORY}
[[ ! -e "$out" ]] || { echo "Refusing to replace existing $out" >&2; exit 1; }
mkdir -p -- "$out"
out=$(cd "$out" && pwd)
cd "$root"

bash scripts/qualify-tone-map-cpu.sh "$out/c1"
bash scripts/qualify-target-volume-cpu.sh "$out/c1/method-a-run1.bin" "$out/c2b"
export ASCIIFLOW_VULKAN_VALIDATION=1
cargo test -p asciiflow-vulkan --features hdr-to-sdr-qualification \
  --test c3_render -- --ignored --nocapture --test-threads=1 2>&1 | tee "$out/render.log"
cargo test -p asciiflow-vulkan --features hdr-to-sdr-qualification \
  --test c3_qualification gpu_domains_limiter_and_fault_cleanup \
  -- --ignored --nocapture --test-threads=1 2>&1 | tee "$out/domains.log"

# Input identities are checked before the full unclamped software/VAAPI audit.
C3_LEGAL_FIXTURE_DIR="$root/tests/fixtures/codecs" C3_LEGAL_REPORT="$out/legal-domain.json" \
  cargo test -p asciiflow-interop --release --features hdr-to-sdr-qualification \
  --test c3_legal_domain -- --ignored --nocapture --test-threads=1 2>&1 | tee "$out/legal-domain.log"

# The original arithmetic sweep remains a separate historical UNORM16
# diagnostic. C3B selects the independently derived N3 contract instead;
# canonical diagnostics still record the old <=2 failures without relabeling.
C1_INPUT="$out/c1/linear-bt2020-1000-v1.bin" \
C1_OUTPUT="$out/c1/method-a-run1.bin" C3_REPORT="$out/canonical-n3.json" \
  cargo test -p asciiflow-vulkan --release --features hdr-to-sdr-qualification \
  --test c3_qualification canonical_f64_and_f32_input_oracles \
  -- --ignored --nocapture --test-threads=1 2>&1 | tee "$out/canonical-n3.log"
C3_PRECISION_REAL_REPORT="$out/selected-real.json" \
  cargo test -p asciiflow-interop --release --features hdr-to-sdr-qualification \
  --test c3_precision_real c3_precision_real_full_frame_diagnostics \
  -- --ignored --nocapture --test-threads=1 2>&1 | tee "$out/selected-real.log"
C3_STRESS_REPORT="$out/stress.json" \
  cargo test -p asciiflow-interop --release --features hdr-to-sdr-qualification \
  --test c3_precision_real c3_selected_dual_slot_3000_frame_stress \
  -- --ignored --nocapture --test-threads=1 2>&1 | tee "$out/stress.log"
# Performance is deliberately after all correctness gates, with validation off.
ASCIIFLOW_VULKAN_VALIDATION=0 C3_FULL_PERFORMANCE_REPORT="$out/performance.json" \
  cargo test -p asciiflow-interop --release --features hdr-to-sdr-qualification \
  --test c3_performance -- --ignored --nocapture --test-threads=1 \
  2>&1 | tee "$out/performance.log"
