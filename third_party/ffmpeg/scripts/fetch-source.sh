#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ffmpeg_dir=$(cd -- "$script_dir/.." && pwd)
manifest="$ffmpeg_dir/version.toml"
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$manifest")
url=$(sed -n 's/^url = "\([^"]*\)"/\1/p' "$manifest")
expected=$(sed -n 's/^sha256 = "\([^"]*\)"/\1/p' "$manifest")
if [[ -z "$version" || -z "$url" || -z "$expected" ]]; then
  printf 'invalid FFmpeg version manifest: %s\n' "$manifest" >&2
  exit 1
fi
archive="$ffmpeg_dir/downloads/ffmpeg-$version.tar.xz"
source_dir="$ffmpeg_dir/source/ffmpeg-$version"

mkdir -p "$ffmpeg_dir/downloads" "$ffmpeg_dir/source"
if [[ ! -f "$archive" ]]; then
  curl --fail --location --output "$archive" "$url"
fi
printf '%s  %s\n' "$expected" "$archive" | sha256sum --check --status
if [[ ! -d "$source_dir" ]]; then
  tar -xJf "$archive" -C "$ffmpeg_dir/source"
fi
printf '%s\n' "$source_dir"
