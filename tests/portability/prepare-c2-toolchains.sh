#!/usr/bin/env bash
# Prepare SHA-pinned isolated components. No downloads or host installation.
set -euo pipefail
root=$(cd -- "$(dirname -- "$0")/../.." && pwd)
work="$root/target/stage54c2-toolchains"
downloads="$work/downloads"
verify() {
    printf '%s  %s\n' "$1" "$downloads/$2" | sha256sum --check --status
}
verify b6863adde98898f42602017462871b5f6333e65aec803fdd7a6308639c52edf3 ffmpeg-8.1.1.tar.xz
verify 5448b45070cf942b1a1abf4e94ac49662bad7e3dabde48c0d48a3f23627e9063 mesa-vulkan-drivers-26.0.3-4.fc44.x86_64.rpm
verify 045d18bf328e17e140e2953c1e6ac38ee595144a642b0cb68e64d2f215ac6527 intel-media-driver-25.4.6-1.fc44.x86_64.rpm
rpmkeys --checksig "$downloads/mesa-vulkan-drivers-26.0.3-4.fc44.x86_64.rpm" "$downloads/intel-media-driver-25.4.6-1.fc44.x86_64.rpm"
for component in mesa-vulkan-drivers-26.0.3-4.fc44 intel-media-driver-25.4.6-1.fc44; do
    destination="$work/$component"
    if [[ ! -d "$destination" ]]; then
        mkdir -- "$destination"
        (cd -- "$destination"; rpm2cpio "$downloads/$component.x86_64.rpm" | cpio -idm --no-absolute-filenames)
    fi
done
python3 - "$work" <<'PY'
import json
import hashlib
from pathlib import Path
import subprocess
import sys
root = Path(sys.argv[1])
for component, library in [
    ('mesa-vulkan-drivers-26.0.3-4.fc44', '/usr/lib64/libvulkan_intel.so'),
    ('intel-media-driver-25.4.6-1.fc44', '/usr/lib64/dri-nonfree/iHD_drv_video.so'),
]:
    dump = subprocess.check_output(['rpm', '-qp', '--dump', str(root / 'downloads' / (component + '.x86_64.rpm'))], text=True)
    expected = next(line.split()[3] for line in dump.splitlines() if line.split()[0] == library)
    path = root / component / library.lstrip('/')
    with path.open('rb') as stream:
        assert hashlib.file_digest(stream, 'sha256').hexdigest() == expected, f'extracted driver changed: {path}'
mesa = root / 'mesa-vulkan-drivers-26.0.3-4.fc44'
original = mesa / 'usr/share/vulkan/icd.d/intel_icd.x86_64.json'
icd = json.loads(original.read_text())
icd['ICD']['library_path'] = str((mesa / 'usr/lib64/libvulkan_intel.so').resolve(strict=True))
path = mesa / 'isolated-intel-icd.json'
content = json.dumps(icd, indent=2) + '\n'
if path.exists():
    assert path.read_text() == content, 'isolated ICD changed'
else:
    path.write_text(content)
PY
if [[ ! -d "$work/ffmpeg-8.1.1" ]]; then
    tar -xf "$downloads/ffmpeg-8.1.1.tar.xz" -C "$work"
fi
prefix="$work/ffmpeg-8.1.1-prefix"
devel="$root/target/stage54c1-toolchains/devel/usr"
test -f "$devel/include/va/va.h"
test -f "$devel/include/libdrm/drm.h"
cd -- "$work/ffmpeg-8.1.1"
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
