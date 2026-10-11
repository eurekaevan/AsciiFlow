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
configure=(./configure --prefix="$prefix" --libdir="$prefix/lib"
  --toolchain=msvc --arch=x86_64 --target-os=win64
  --enable-shared --disable-static --disable-gpl --disable-nonfree
  --disable-version3 --disable-autodetect --disable-doc --disable-debug
  --disable-avdevice --enable-avfilter --disable-filters --disable-network
  --enable-w32threads --enable-mediafoundation
  --disable-encoders --enable-encoder=h264_mf
  --disable-decoders --enable-decoder=h264,hevc,aac
  --disable-demuxers --enable-demuxer=mov
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
"${configure[@]}" > "$work/configure.log" 2>&1
make -j"${ASCIIFLOW_BUILD_JOBS:-4}" > "$work/build.log" 2>&1
make install > "$work/install.log" 2>&1
# FFmpeg installs MSVC import libraries beside the DLLs; ffmpeg-sys expects lib/.
mkdir -p "$prefix/lib"
cp "$prefix/bin/"*.lib "$prefix/lib/"
export PATH="$prefix/bin:$PATH"
"$prefix/bin/ffmpeg.exe" -L > "$work/license.txt" 2>&1
"$prefix/bin/ffmpeg.exe" -buildconf > "$work/buildconf.txt" 2>&1
