# Official Linux releases

The first automated release target is Linux x86_64. GitHub Actions builds on
Ubuntu 24.04 with Rust 1.99.0 and the existing verified FFmpeg 8.1.3 LGPL profile.
The archive requires glibc 2.39 or newer; other Linux distributions are not
qualified solely by a successful CI build. Windows, macOS and ARM packages are
not currently produced.

## Use the package

Download the `asciiflow-YYYY-MM-DD-linux-x86_64-lgpl.tar.gz` archive and
`SHA256SUMS` from the same Release. Run `sha256sum --check SHA256SUMS`, extract
the archive, and run `./asciiflow --help` from the extracted directory.
Keep `lib/` next to the executable. The package contains only the four required
FFmpeg libraries; compatible modified builds can replace these dynamic libraries.

Host dependencies include libdav1d, libva, libva-drm, libdrm, FreeType, the Vulkan
loader and C/C++ runtimes. On Ubuntu 24.04, install:

```sh
sudo apt-get install libdav1d7 libva2 libva-drm2 libdrm2 libfreetype6 libvulkan1 libfontconfig1
```

Install VAAPI and Vulkan drivers suitable for your GPU separately. Fontconfig
supports font-name discovery; the built-in font does not need font files bundled.
See `production-support.md` for the qualified hardware paths and limitations.
This LGPL profile supports VAAPI H.264/HEVC/AV1 encoding and excludes software
H.264 encoding. Selecting `--encode software` produces an explicit error.

## Create a release

1. Run ordinary CI and commit the intended release changes. Cargo's numeric
   version is internal build metadata and does not need to change for each
   release. The packaged CLI's `--version` prints the same date as the release
   tag; ordinary source builds print `asciiflow dev`.
2. Push a date tag in `YYYY-MM-DD` format, for example `2026-10-10`.
   Only date-shaped tag pushes trigger publishing; the workflow also checks
   calendar validity and rejects dates such as `2026-02-30`. Each date identifies
   one release; a second release needs another date tag.
3. Actions builds, verifies and uploads the archive plus SHA256SUMS to a new
   GitHub Release. Existing Releases are not overwritten; publication errors fail
   visibly. Date releases are ordinary releases marked as latest. No personal
   token is needed: only the publishing job receives
   `contents: write` permission through the built-in GitHub token.

Run **Release Linux LGPL** manually to build a trial archive without publishing.
Manual runs upload the archive, checksums, release notes and verification logs
as an Actions artifact. A tag push does not automatically wait for ordinary CI;
run CI on the intended commit before tagging. Manual branch runs use the current
UTC date for archive names; manual tag runs use the validated tag date.

```sh
git tag 2026-10-10
git push origin 2026-10-10
```

## Reproduce and inspect

Use a clean checkout on Ubuntu 24.04 with Rust 1.99.0 and the `rust-docs`
component. Install the build prerequisites listed in `.github/workflows/release.yml`,
then download the exact FFmpeg 8.1.3 source archive from ffmpeg.org and run:

```sh
bash scripts/package-linux-release.sh /path/to/ffmpeg-8.1.3.tar.xz /path/to/new-work-directory
```

The script checks the source SHA256 before building. The archive includes exact
application and FFmpeg source, build recipes, the compiler/configuration record,
actual FFmpeg license/capability evidence, Rust dependency notices and Rust
standard-library notices. Only avcodec, avformat, avutil and swscale are bundled;
GPU drivers and other host libraries remain system dependencies.
Local builds use the current UTC date by default; set `RELEASE_DATE=YYYY-MM-DD`
to reproduce another date's archive name.
The script validates this date and embeds it through `ASCIIFLOW_RELEASE_DATE`
before compilation. Changing that build environment value makes Cargo rebuild
the CLI; a plain build without it returns to the `dev` label. The relocated
package gate checks that `--version` matches the archive date.

The release gate verifies the restricted FFmpeg profile, the software-encoding
rejection test, dependency resolution after extracting into another directory,
and CLI version/help startup. GitHub hosted runners do not verify real GPU
conversion. A new runner-built package still needs hardware qualification before
claiming additional supported GPU configurations.
