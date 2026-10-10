#!/usr/bin/env bash
# Isolated libav patch comparison; never install into the host filesystem.
set -euo pipefail
root=$(cd -- "$(dirname -- "$0")/../.." && pwd)
work="$root/target/portability-toolchains"
source="$work/ffmpeg-8.1.2"
prefix="$work/ffmpeg-8.1.2-prefix"
devel="$work/devel/usr"
test -f "$source/configure"
test -f "$devel/include/va/va.h"
test -f "$devel/include/libdrm/drm.h"
test "$(sha256sum "$work/downloads/ffmpeg-8.1.2.tar.xz" | cut -d' ' -f1)" = 464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c
# The extracted devel RPMs contain unversioned linker symlinks. Their targets
# stay the current host runtime, not an alternate VAAPI/DRM driver.
for library in libva.so.2 libva-drm.so.2 libdrm.so.2 libdav1d.so.7; do
    if [[ ! -e "$devel/lib64/$library" ]]; then
        ln -s "/usr/lib64/$library" "$devel/lib64/$library"
    fi
done
cd -- "$source"
export PKG_CONFIG_PATH="$devel/lib64/pkgconfig"
./configure --prefix="$prefix" --libdir="$prefix/lib" \
    --enable-shared --disable-static --disable-doc --disable-debug \
    --enable-gpl --enable-libx264 --enable-libdav1d --enable-vaapi --enable-libdrm \
    --disable-autodetect --enable-pthreads \
    --extra-cflags="-I$devel/include -I$devel/include/libdrm" \
    --extra-ldflags="-L$devel/lib64 -Wl,-rpath,$prefix/lib"
make -j "${ASCIIFLOW_BUILD_JOBS:-4}"
make install
"$prefix/bin/ffmpeg" -version
