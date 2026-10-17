# Official date-tagged LGPL releases

The **Release LGPL** workflow builds Linux x86_64 and Windows x86_64 packages
from one exact commit and one `YYYY-MM-DD` date. Both use Rust 1.99.0 and
FFmpeg 8.1.3 with GPL, nonfree and version3 disabled. macOS and ARM packages
are not currently produced. Each archive includes `asciiflow` and the
`asciiflow-gui` desktop application (with `.exe` on Windows).

Linux retains its existing Ubuntu 24.04/glibc 2.39 LGPL profile. The Windows
implementation and packaging workflow are new: neither an actual Windows build
nor real Windows hardware conversion has been verified on the current Linux
development host. A configured CI gate is not a successful Windows release
receipt, and Linux hardware qualification does not apply to Windows.

## Use the Linux package

Download `asciiflow-YYYY-MM-DD-linux-x86_64-lgpl.tar.gz` and `SHA256SUMS` from
the same Release. When downloading only one platform, run
`sha256sum --check --ignore-missing SHA256SUMS`, extract the archive, and run
`./asciiflow --help` or `./asciiflow-gui` from its directory. Keep `lib/` next
to both executables.
The package contains the four required FFmpeg libraries; compatible modified
builds can replace these dynamic libraries.

Host dependencies include libdav1d, libva, libva-drm, libdrm, FreeType, the Vulkan
loader and C/C++ runtimes. On Ubuntu 24.04, install:

```sh
sudo apt-get install libdav1d7 libva2 libva-drm2 libdrm2 libfreetype6 libvulkan1 libfontconfig1 libgl1 libxkbcommon0 libwayland-client0 libx11-6 libxcursor1 libxi6 libxrandr2
```

The GUI additionally needs a working X11/Wayland desktop, OpenGL driver and
a configured desktop portal for native file dialogs. Install VAAPI and Vulkan
drivers suitable for your GPU separately. Fontconfig
supports font-name discovery; the built-in font needs no external font file. GNOME interface
font settings use `gsettings`; selecting completed files uses `gdbus` (both in
`libglib2.0-bin` on Ubuntu). Opening the parent folder falls back to `xdg-open`
from `xdg-utils` when file selection is unavailable.
The archive requires glibc 2.39 or newer. See `production-support.md` for the
qualified hardware paths and limitations. This profile provides VAAPI
H.264/HEVC/AV1 encoding and excludes software H.264 encoding; selecting
`--encode software` produces an explicit error.

## Use the Windows package

Download `asciiflow-YYYY-MM-DD-windows-x86_64-lgpl.zip` and the same Release's
`SHA256SUMS`. For example, verify and extract the selected ZIP in PowerShell:

```powershell
$archive = 'asciiflow-2026-10-11-windows-x86_64-lgpl.zip' # Replace with the downloaded name.
$entry = @(Get-Content SHA256SUMS | Where-Object { $_ -match "^[0-9a-f]{64}  $([regex]::Escape($archive))$" })
if ($entry.Count -ne 1 -or (Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry[0].Substring(0, 64)) {
    throw 'Release archive checksum mismatch'
}
Expand-Archive $archive
```

Keep `asciiflow.exe`, `asciiflow-gui.exe` and the four FFmpeg DLLs in the same
extracted directory. Launch `asciiflow-gui.exe` for the graphical interface;
it requires a desktop with OpenGL support.
The native MSVC package does not need a MinGW or MSYS2 runtime installed by the
user. Windows 10/11 requires the Microsoft Visual C++ 2015–2022 x64
Redistributable, Media Foundation and a compatible H.264 hardware encoder with
its vendor driver. Windows N editions additionally need the Media Feature Pack.
Vulkan processing needs a suitable Vulkan driver; CPU processing does not.

```powershell
./asciiflow.exe input.mp4 output.mp4 --backend cpu --decode software --encode mf --width 160
./asciiflow.exe input.mp4 --backend cpu --decode software --encode mf --explain-plan --diagnostic-report mf-report.json
```

MF means FFmpeg's `h264_mf` encoder with hardware encoding required. Missing or
incompatible hardware fails explicitly. Automatic Windows processing prefers
eligible Vulkan devices and otherwise selects CPU. Vulkan uses host frames and
readback; `--backend cpu` selects CPU processing explicitly.
The profile includes software H.264/HEVC/VP8/VP9/MPEG-4 Part 2 video decoding
and MP4-compatible compressed audio passthrough. Both platform recipes enable
MP4, Matroska/WebM and AVI demuxers. Input extension alone does not qualify all
contained codecs or colors; see [input format scope](../README.md#input-formats). It has no software AV1 decoder,
HEVC/AV1 encoder, hardware decoding, 10-bit output, HDR output/conversion or
VAAPI/DRM/DMA-BUF interop. The default built-in font and explicit scalable
font files work without Linux Fontconfig; font-name discovery remains Linux-only.

These commands describe the implemented route. Successful real conversion on
Windows still needs the hardware gate below; Linux's retained audio and media
evidence does not qualify every Windows input or device.

## First graphical interface

The [GUI user guide](gui.md) describes launching the application, file selection,
appearance and advanced controls, preview, cancellation and opening the output
location. Both platform archives include that guide as `gui.md`. This section
retains desktop acceptance evidence; it does not qualify additional hardware
or media paths.

On the recorded Linux host, the native file manager opened the test directory
and exposed the Chinese output filename as selected. This does not qualify
native Windows behavior.

A real Linux XWayland window was visually checked at its default size; the
system scanner found 111 compatible faces on that host. Automated interaction
covers opening the selector, focusing search, filtering and choosing a face.
The catalog and interface checks do not qualify the Windows font directories
or Windows desktop operation.
At that earlier check, the optional `system_monospace_matches_explicit_atlas`
test failed because the default `monospace` alias resolved to a variable font
without FreeType's fixed-width flag. The false rejection was subsequently fixed
and the actual system-alias test passed. The catalog now accepts scalable
proportional faces as well, so fixed-width classification no longer filters it.
The recorded 111-face count predates this broader catalog.

The guide describes Linux X11/XWayland selection and the native Wayland
file-selector alternative. The backend limitation is tracked in
[winit issue 1881](https://github.com/rust-windowing/winit/issues/1881).

Existing media, audio, color and hardware support boundaries still apply.
The GUI is new and does not inherit the recorded single-job CLI qualification.
An isolated Linux X11 desktop with software OpenGL was exercised using preset
synthetic input: actual first-frame preview, a 90-frame CPU/libx264 conversion,
and cancellation that removed staging without committing output. This developer
build exercise does not qualify the official LGPL hardware encoder routes.
A native Linux Wayland window was also exercised with software OpenGL. Native
input selection and display of a filename containing spaces were confirmed by a
user. The window and appearance fields expose accessibility names; native dialogs
are parented to the main window. Linux uses an installed Fontconfig CJK fallback
for interface filenames, without bundling a font or changing the rendering font.
The installed CJK glyphs passed a separate desktop font check.

Automated GUI-state tests exercised dropped-file state reset, two complete
90-frame CPU/software conversions around a cancelled job using Chinese and
space-containing paths, output decoding and staging cleanup. These tests do not
establish native drag-and-drop or dialog interaction. Window-close cancellation
and worker-disconnect reporting are covered by a controlled worker test.
Native Wayland file dragging was reported as nonfunctional during user
acceptance, leading to the XWayland selection above. The corrected X11 window
was observed on the real desktop; the user then confirmed that dragging a video
from the file manager works and its filename displays correctly. Native output
selection, overwrite confirmation,
cancellation while closing a real window, long repeated sessions and Windows
desktop execution also still require interactive acceptance.

## Create a release

1. Run ordinary CI and commit the intended release changes. Cargo's numeric
   version is internal metadata and need not change for every release. The
   packaged CLI's `--version` prints the release date; source builds print
   `asciiflow dev`.
2. Run **Release LGPL** manually on the intended branch or tag, or push a date
   tag in `YYYY-MM-DD` format. Manual runs use the date in `Asia/Taipei` (UTC+8),
   even when an older tag is selected. The preparation job fixes the date and
   exact commit before both platform builds start. Tag pushes use the tag date;
   impossible calendar dates such as `2026-02-30` are rejected.
3. Linux and Windows build and check their archives in parallel. The publishing
   job waits for both jobs to succeed, verifies the combined `SHA256SUMS`, then
   publishes both `.tar.gz` and `.zip` assets with that checksum file. Each job
   also retains its archive, release notes and verification logs as an Actions
   artifact. A failed platform build prevents publication.

The publishing job creates the date tag at the built commit or verifies that
an existing tag points there. A conflicting tag fails without moving it.
Existing Releases are not overwritten; a second release needs another date.
Releases are ordinary releases marked as latest. Only the publishing job
receives `contents: write` through GitHub's built-in token; no personal token
is required. Neither trigger waits automatically for ordinary CI, so run it on
the intended commit first.

```sh
git tag 2026-10-11
git push origin 2026-10-11
```

## Reproduce and inspect

Both recipes require a clean checkout, Rust 1.99.0 with `rust-docs`, and the
exact FFmpeg 8.1.3 source archive from ffmpeg.org. The recipes verify its pinned
SHA256 before building. Local recipes default to the current UTC date; set
`RELEASE_DATE=YYYY-MM-DD` to reproduce another archive label. Actions supplies
the date fixed by the preparation job. `ASCIIFLOW_RELEASE_DATE` embeds that date
at compile time, and the relocated package gate checks `--version` against it.

For Linux, use Ubuntu 24.04 with the prerequisites in
`.github/workflows/release.yml`:

```sh
bash scripts/package-linux-release.sh /path/to/ffmpeg-8.1.3.tar.xz /path/to/new-work-directory
```

For Windows, use PowerShell 7 with an inherited x64 MSVC developer environment,
the Windows SDK, LLVM/libclang, CMake, Python and MSYS2 make/NASM. The workflow
uses `windows-2022`; its exact setup is in the same workflow file. MSYS2 is build
tooling; FFmpeg is compiled with MSVC rather than MinGW. Set `MSYS2_ROOT` when
MSYS2 is outside `C:\msys64`, and `LIBCLANG_PATH` to LLVM's `bin` directory.
With FFmpeg autodetection disabled, the recipe explicitly enables D3D11VA:
FFmpeg 8.1.3's Media Foundation encoder uses D3D11 context types whose header
is conditional on this feature. The device-free profile receipt checks that
D3D11VA is present; this does not prove GPU execution. Failed configure/build/
install steps print their log tail in Actions and retain the complete logs in
the existing diagnostics artifact.

The recipe supplies the locked `libz-sys` include directory for the bundled
FreeType/libpng build: upstream `freetype-sys` 0.23.0 references a zlib header
directory that its crate archive does not contain. This target-specific,
space-safe include workaround does not modify either locked crate; its flags
are retained in the package build record.

```powershell
./scripts/package-windows-release.ps1 C:\sources\ffmpeg-8.1.3.tar.xz C:\build\new-release-work
```

Each archive includes exact application and FFmpeg source, recipes,
compiler/configuration records, actual FFmpeg license/capability receipts,
Rust dependency notices (including the enabled GUI dependency graph and
embedded-font license texts) and Rust standard-library notices. Only avcodec,
avformat, avutil and swscale are packaged as FFmpeg runtime libraries. Windows
also embeds FreeType, libpng and zlib and includes their native notices and
exact locked crate source archives. Linux continues to use host FreeType.
Graphics drivers and platform runtimes remain host dependencies. Retain the
source and notice directories when redistributing either package.

Hosted release gates inspect the restricted LGPL profile, relocated dependency
resolution for both executables, software-encoding rejection and CLI version/help
startup. GUI presence and imports are checked without opening a headless window.
Windows also
checks explicit MF planning diagnostics: a probed encoder permits an MF plan;
unavailable hardware must produce a clear planning failure without fallback.
These gates do not establish graphical interaction or end-to-end video conversion
on a real Windows GPU.

## Windows hardware gate

On a real Windows machine with the required MF H.264 hardware encoder, build
against the same FFmpeg profile and run the opt-in round-trip test:

```powershell
cargo test --locked -p asciiflow-media --features lgpl-prebuilt ffmpeg::encoder::audio_regression_tests::media_foundation_hardware_encode_round_trip -- --ignored --exact
```

The hardware test requires a device and fails if it is unavailable; it is not
a hosted-runner smoke test. Record the actual Windows build, GPU/driver and
FFmpeg configuration with its outcome before claiming hardware qualification.
Run the packaged CLI on representative real media as well. A new Windows ZIP
does not inherit the recorded Intel/Linux qualification.
