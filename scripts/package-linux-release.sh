#!/usr/bin/env bash
# Build and verify the official Linux x86_64 LGPL archive in a fresh directory.
set -euo pipefail
if [[ $# != 2 ]]; then
  echo "usage: $0 FFMPEG_SOURCE_ARCHIVE NEW_WORK_DIRECTORY" >&2
  exit 2
fi
root=$(cd "$(dirname "$0")/.." && pwd)
archive=$(realpath "$1")
work=$(realpath -m "$2")
cd "$root"
[[ $(uname -m) == x86_64 && $(uname -s) == Linux ]]
# Corresponding application source must be exactly what is compiled.
[[ -z $(git status --porcelain --untracked-files=normal) ]] || {
  echo "Release packaging requires a clean checkout" >&2; exit 1;
}
[[ ! -e "$work" ]]
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["workspace"]["package"]["version"])')
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]
release_date=${RELEASE_DATE:-$(date -u +%F)}
python3 - "$release_date" <<'PY'
import datetime, re, sys
value = sys.argv[1]
assert re.fullmatch(r'[0-9]{4}-[0-9]{2}-[0-9]{2}', value), 'Release date must use YYYY-MM-DD'
datetime.date.fromisoformat(value)
PY
name="asciiflow-$release_date-linux-x86_64-lgpl"
mkdir -p "$work/dist"
bash third_party/ffmpeg/scripts/build-lgpl-prebuilt.sh "$archive" /usr "$work/ffmpeg"
export FFMPEG_DIR="$work/ffmpeg/sysroot"
export PKG_CONFIG_PATH="$FFMPEG_DIR/lib/pkgconfig"
export LD_LIBRARY_PATH="$FFMPEG_DIR/lib"
export CARGO_TARGET_DIR="$work/target"
export RUSTFLAGS='-C link-arg=-Wl,--disable-new-dtags,-rpath,$ORIGIN/lib'
python3 tests/release/ffmpeg-profile-receipt.py "$FFMPEG_DIR" "$work/ffmpeg-profile.json"
cargo test --locked -p asciiflow-media --features lgpl-prebuilt \
  ffmpeg::encoder::audio_regression_tests::lgpl_profile_rejects_software_without_generic_h264_fallback -- --exact
cargo build --locked --release -p asciiflow-cli --features lgpl-prebuilt
package="$work/$name"
mkdir -p "$package/lib" "$package/sources" "$package/notices/rust-std" "$package/build"
cp "$CARGO_TARGET_DIR/release/asciiflow" "$package/"
for component in avcodec avformat avutil swscale; do
  cp -a "$FFMPEG_DIR/lib/lib$component.so"* "$package/lib/"
done
cp LICENSE THIRD_PARTY_NOTICES.md "$package/"
cp docs/production-support.md "$package/"
python3 scripts/collect-release-notices.py "$package/notices/rust-dependencies"
rust_docs="$(rustc --print sysroot)/share/doc/rust"
cp "$rust_docs/COPYRIGHT-library.html" "$package/notices/rust-std/"
cp -a "$rust_docs/licenses" "$package/notices/rust-std/"
cp "$archive" "$package/sources/ffmpeg-8.1.3.tar.xz"
cp "$work/ffmpeg/ffmpeg-8.1.3/COPYING.LGPLv2.1" "$package/notices/"
git archive --format=tar.gz --prefix="asciiflow-$release_date/" -o "$package/sources/asciiflow-$release_date.tar.gz" HEAD
cp third_party/ffmpeg/scripts/build-lgpl-prebuilt.sh scripts/package-linux-release.sh "$package/build/"
cp "$work/ffmpeg/"{configure-command.txt,compiler.txt,source.sha256,license.txt,buildconf.txt} "$package/build/"
cp "$work/ffmpeg-profile.json" "$package/build/"
{
  rustc --version --verbose
  cargo --version
  ldd --version
  cat /etc/os-release
  git rev-parse HEAD
} > "$package/build/environment.txt"
cat > "$package/sources/SOURCE.md" <<'SOURCE'
# Corresponding source

FFmpeg 8.1.3 is unmodified (no patches); its exact upstream archive is included.
Its SHA256, configure command, compiler identity and build recipe are in ../build/.
The application archive contains the exact Git commit used to build this package,
including Cargo.lock, release recipes and verification scripts. Font fixtures
inside this source snapshot retain their accompanying OFL texts; no external
fonts are installed or loaded by the prebuilt executable. Run the packaging
recipe from a clean checkout on Ubuntu 24.04 with the prerequisites documented in
docs/releases.md. Rust dependencies are fetched at their locked versions by Cargo.

The FFmpeg libraries in ../lib are dynamically linked and may be replaced with
ABI-compatible modified LGPL builds. No additional EULA restricts modification
or reverse engineering for debugging such modifications.
SOURCE
cp docs/releases.md "$package/README.md"
# Exercise the archive itself after relocation, with no build-library override.
tar -czf "$work/dist/$name.tar.gz" -C "$work" "$name"
mkdir "$work/relocated"
tar -xzf "$work/dist/$name.tar.gz" -C "$work/relocated"
unset LD_LIBRARY_PATH FFMPEG_DIR PKG_CONFIG_PATH
relocated="$work/relocated/$name"
python3 - "$relocated" "$work/runtime-check.txt" <<'PY'
import pathlib, re, subprocess, sys
package = pathlib.Path(sys.argv[1]).resolve()
# ldd on the executable follows its RPATH through the complete dependency tree.
# A standalone library does not inherit the executable's search path.
listing = subprocess.check_output(['ldd', str(package / 'asciiflow')], text=True)
assert 'not found' not in listing, listing
assert not any(x in listing.lower() for x in ['libx264', 'libx265', 'libfdk', 'libxvid']), listing
resolved_ffmpeg = re.findall(r'(lib(?:av\w+|swscale)\.so[^ ]*) => (\S+)', listing)
assert len(resolved_ffmpeg) == 4, listing
for name, resolved in resolved_ffmpeg:
    assert pathlib.Path(resolved).resolve().parent == package / 'lib', (name, resolved)
dynamic = subprocess.check_output(['readelf', '-d', str(package / 'asciiflow')], text=True)
assert '(RPATH)' in dynamic and '[$ORIGIN/lib]' in dynamic, dynamic
assert '(RUNPATH)' not in dynamic, dynamic
pathlib.Path(sys.argv[2]).write_text(listing + dynamic)
PY
"$relocated/asciiflow" --version
"$relocated/asciiflow" --help > "$work/help.txt"
if "$relocated/asciiflow" tests/fixtures/media/no-audio.mp4 "$work/rejected.mp4" \
    --backend cpu --decode software --encode software --audio none --width 16 --no-progress \
    > "$work/software-rejection.txt" 2>&1; then
  echo "LGPL package unexpectedly accepted software encoding" >&2; exit 1
fi
rg -q 'excluded from the official LGPL prebuilt' "$work/software-rejection.txt"
[[ ! -e "$work/rejected.mp4" ]]
cp "$work/"{runtime-check.txt,software-rejection.txt} "$work/dist/"
cat > "$work/dist/RELEASE_NOTES.md" <<NOTES
Linux x86_64 official LGPL package, built on Ubuntu 24.04 (glibc 2.39 or newer).
Release date: $release_date. Application version: $version.

Includes FFmpeg 8.1.3 shared libraries, corresponding source and third-party notices.
Software H.264 encoding is excluded; a compatible VAAPI GPU and host drivers are required.
See the included README.md and production-support.md for dependencies and supported routes.

CI verifies the FFmpeg profile, LGPL software-encoding rejection and relocated archive startup.
Real GPU conversion is not verified by GitHub hosted runners. SHA256SUMS covers the release archive.
NOTES
(cd "$work/dist" && sha256sum "$name.tar.gz" > SHA256SUMS)
echo "Verified release assets: $work/dist"
