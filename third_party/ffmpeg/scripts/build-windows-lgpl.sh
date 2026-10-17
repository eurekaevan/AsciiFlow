#!/usr/bin/env bash
# Run from MSYS2 with the x64 MSVC developer environment inherited.
set -euo pipefail
if [[ $# != 2 ]]; then
  echo "usage: $0 FFmpeg-8.1.3.tar.xz NEW_WORK_DIRECTORY" >&2
  exit 2
fi
archive=$(realpath "$1")
work=$(realpath -m "$2")
[[ $(uname -s) == MSYS* || $(uname -s) == MINGW* ]]
[[ $(uname -m) == x86_64 ]]
test "$(sha256sum "$archive" | cut -d' ' -f1)" = 7138d28c96d9d3e3af4ee3d8cad72741f8ffb40da90c1112235dea3ecd3178a3
test ! -e "$work"
# MSYS2 also supplies link.exe; prefer the MSVC linker beside cl.exe.
compiler=$(command -v cl.exe)
export PATH="$(dirname "$compiler"):/usr/bin:$PATH"
command -v nasm > /dev/null
mkdir -p "$work"
tar -xf "$archive" -C "$work"
prefix="$work/sysroot"
cd "$work/ffmpeg-8.1.3"
# mfenc.c uses D3D11 types even for host-memory input. With autodetection off,
# D3D11VA must be enabled explicitly so its SDK declarations are included.
configure=(./configure --prefix="$prefix" --libdir="$prefix/lib"
  --toolchain=msvc --arch=x86_64 --target-os=win64
  --enable-shared --disable-static --disable-gpl --disable-nonfree
  --disable-version3 --disable-autodetect --disable-doc --disable-debug
  --disable-avdevice --enable-avfilter --disable-filters --disable-network
  --enable-w32threads --enable-mediafoundation --enable-d3d11va
  --disable-encoders --enable-encoder=h264_mf
  --disable-decoders --enable-decoder=h264,hevc,aac,vp8,vp9,mpeg4,mp3,mp3float,opus,vorbis
  --disable-demuxers --enable-demuxer=mov,matroska,avi
  --disable-muxers --enable-muxer=mp4,null
  --disable-protocols --enable-protocol=file)
printf '%q ' "${configure[@]}" > "$work/configure-command.txt"
printf '\n' >> "$work/configure-command.txt"
# Without input cl.exe prints its identity and reports missing source (exit 2).
if cl.exe > "$work/compiler.txt" 2>&1; then
  :
else
  [[ $? == 2 ]]
fi
sha256sum "$archive" > "$work/source.sha256"
run_logged() {
  local log=$1
  shift
  local status
  if "$@" > "$work/$log" 2>&1; then
    return 0
  else
    status=$?
  fi
  printf 'FFmpeg %s failed with exit code %s; last 200 log lines:\n' "$log" "$status" >&2
  tail -n 200 "$work/$log" >&2
  return "$status"
}
run_logged configure.log "${configure[@]}"
run_logged build.log make -j"${ASCIIFLOW_BUILD_JOBS:-4}"
run_logged install.log make install
# FFmpeg installs MSVC import libraries beside the DLLs; ffmpeg-sys expects lib/.
mkdir -p "$prefix/lib"
cp "$prefix/bin/"*.lib "$prefix/lib/"
export PATH="$prefix/bin:$PATH"
"$prefix/bin/ffmpeg.exe" -L > "$work/license.txt" 2>&1
"$prefix/bin/ffmpeg.exe" -buildconf > "$work/buildconf.txt" 2>&1
