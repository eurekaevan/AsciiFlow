#!/usr/bin/env bash
# Isolated official release dependency; never modifies system libraries.
set -euo pipefail
if [[ $# != 3 ]]; then
  echo "usage: $0 FFmpeg-8.1.3.tar.xz DEVEL_USR NEW_WORK_DIRECTORY" >&2
  exit 2
fi
archive=$(realpath "$1")
devel=$(realpath "$2")
work=$(realpath -m "$3")
test "$(sha256sum "$archive" | cut -d' ' -f1)" = 7138d28c96d9d3e3af4ee3d8cad72741f8ffb40da90c1112235dea3ecd3178a3
test ! -e "$work"
mkdir -p "$work"
tar -xf "$archive" -C "$work"
prefix="$work/sysroot"
export PKG_CONFIG_PATH="$devel/lib64/pkgconfig"
cd "$work/ffmpeg-8.1.3"
configure=(./configure --prefix="$prefix" --libdir="$prefix/lib"
  --enable-shared --disable-static --disable-gpl --disable-nonfree
  --disable-version3 --disable-autodetect --disable-doc --disable-debug
  --disable-avdevice --enable-avfilter --disable-filters --disable-network
  --enable-pthreads --enable-vaapi --enable-libdrm --enable-libdav1d
  --disable-encoders --enable-encoder=h264_vaapi,hevc_vaapi,av1_vaapi
  --disable-decoders --enable-decoder=h264,hevc,av1,libdav1d,aac
  --disable-demuxers --enable-demuxer=mov
  --disable-muxers --enable-muxer=mp4,null
  --disable-protocols --enable-protocol=file
  --extra-cflags="-I$devel/include -I$devel/include/libdrm"
  --extra-ldflags="-L$devel/lib64")
printf '%q ' "${configure[@]}" > "$work/configure-command.txt"
printf '\n' >> "$work/configure-command.txt"
cc --version > "$work/compiler.txt"
sha256sum "$archive" > "$work/source.sha256"
"${configure[@]}" > "$work/configure.log" 2>&1
make -j"${ASCIIFLOW_BUILD_JOBS:-4}" > "$work/build.log" 2>&1
make install > "$work/install.log" 2>&1
LD_LIBRARY_PATH="$prefix/lib" "$prefix/bin/ffmpeg" -L > "$work/license.txt" 2>&1
LD_LIBRARY_PATH="$prefix/lib" "$prefix/bin/ffmpeg" -buildconf > "$work/buildconf.txt" 2>&1
