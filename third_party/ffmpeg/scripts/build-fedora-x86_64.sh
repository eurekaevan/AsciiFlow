#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ffmpeg_dir=$(cd -- "$script_dir/.." && pwd)

for tool in nasm pkg-config make; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'missing build dependency: %s\n' "$tool" >&2
    exit 1
  fi
done
for package in x264 libva libdrm; do
  if ! pkg-config --exists "$package"; then
    printf 'missing build dependency: %s development package\n' "$package" >&2
    exit 1
  fi
done

source_dir=$("$script_dir/fetch-source.sh")
install_dir="$ffmpeg_dir/install"

cd "$source_dir"
./configure \
  --prefix="$install_dir" \
  --enable-shared \
  --disable-static \
  --disable-programs \
  --disable-doc \
  --disable-debug \
  --disable-avdevice \
  --disable-avfilter \
  --disable-swresample \
  --enable-gpl \
  --enable-libx264 \
  --enable-vaapi \
  --enable-libdrm
make -j"$(nproc)"
make install

printf 'Build Rust with:\n  FFMPEG_DIR=%s cargo build --workspace\n' "$install_dir"
