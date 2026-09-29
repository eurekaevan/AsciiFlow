#!/usr/bin/env bash
set -euo pipefail
[[ $# == 2 ]] || { echo "usage: $0 C1_INPUT.bin OUTPUT_DIR" >&2; exit 2; }
c1_input=$1
output_dir=$2
repository=$(pwd)
expected_c1=d0eb86dcc265fe7f675d87055f038d258176e331eb2aac412ba7358a6c8c1e6b
actual_c1=$(sha256sum "$c1_input" | cut -d ' ' -f1)
[[ $actual_c1 == "$expected_c1" ]] || { echo "sealed C-1 identity mismatch" >&2; exit 1; }
# Atomically require a fresh directory. Besides binary create_new protection,
# this prevents tee/JSON redirection from truncating an alias of retained input.
mkdir -- "$output_dir"
cargo run --release -p asciiflow-cpu --example qualify_target_volume -- "$c1_input" "$output_dir" | tee "$output_dir/qualification.log"
cmp "$output_dir/target-volume-run1.bin" "$output_dir/target-volume-run2.bin"
cmp "$output_dir/target-volume-run1.bin" "$output_dir/target-volume-run3.bin"
sha256sum "$c1_input" "$output_dir"/target-volume-run{1,2,3}.bin
(cd "$output_dir" && sha256sum -c "$repository/tests/fixtures/tone-map/C2B-SHA256SUMS")
python3 tests/fixtures/tone-map/audit-c2b-target-volume.py "$c1_input" "$output_dir" > "$output_dir/c2b-target-volume-audit.json"
cat "$output_dir/c2b-target-volume-audit.json"
