#!/usr/bin/env bash
set -euo pipefail
[[ $# == 1 ]] || { echo "usage: $0 OUTPUT_DIR" >&2; exit 2; }
output_dir=$1
repository=$(pwd)
mkdir -p "$output_dir"
cargo run --release -p asciiflow-cpu --example qualify_tone_map -- "$output_dir" | tee "$output_dir/qualification.log"
cmp "$output_dir/method-a-run1.bin" "$output_dir/method-a-run2.bin"
cmp "$output_dir/method-a-run1.bin" "$output_dir/method-a-run3.bin"
sha256sum "$output_dir/linear-bt2020-1000-v1.bin" "$output_dir"/method-a-run{1,2,3}.bin
sha256sum crates/asciiflow-cpu/examples/qualify_tone_map.rs tests/fixtures/tone-map/generate-method-a-vectors.py
(cd "$output_dir" && sha256sum -c "$repository/tests/fixtures/tone-map/SHA256SUMS")
