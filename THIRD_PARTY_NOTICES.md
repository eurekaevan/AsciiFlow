# Third-party notices and distribution boundary

AsciiFlow's own source remains MIT-licensed; see `LICENSE`. The executable also
contains code from the exact locked Rust dependencies and the built-in font.
Run `python3 scripts/collect-release-notices.py NEW_OUTPUT_DIRECTORY` from the
RC source to generate the Linux dependency inventory and copy upstream license
and notice texts. Include those generated files in the release archive.

The built-in 8×8 atlas comes from `font8x8` 0.3.1 (`BASIC_FONTS`), MIT,
Copyright (c) 2017 Joaquin Rosales. It is embedded, not loaded from a fixture.
Explicit FreeType font files are supplied by the user and are not bundled.
Test-only OFL font fixtures are not release assets. Project GLSL is compiled
by shaderc at build time into embedded SPIR-V; no external shader directory is
needed at runtime. Shaderc is build tooling, not a bundled runtime library.

## Official LGPL prebuilt release

The `lgpl-prebuilt` release uses exact FFmpeg 8.1.3 shared libraries built with
GPL, nonfree and version3 disabled. Its license must be attested by the actual
FFmpeg executable and avcodec_license()/avformat_license(), not configure flags
alone. This FFmpeg is LGPL-2.1-or-later; AsciiFlow's own sources remain MIT.
libx264/libx265/libxvid/libfdk_aac must be absent from enabled codecs and recursive
runtime dependencies. The software H.264 backend remains available to suitable
custom developer builds, but is excluded from this official prebuilt profile.

When bundled, FFmpeg shared libraries carry their upstream COPYING.LGPLv2.1,
exact source archive, patch list, configure command, compiler/build recipe and
SOURCE.md. The executable dynamically links these separate, replaceable
libraries. Permit library replacement and reverse engineering for debugging
modifications as required by LGPL; no restrictive EULA is added. The complete
corresponding source and recipe must accompany any distributed archive, with
equal access, rather than relying on a mutable repository URL.

The dependency build enables only libva, libdrm and BSD libdav1d externally;
these are host dependencies unless explicitly packaged with their own notices.
Mesa/ANV, iHD, Vulkan loader, FreeType and C/C++ system runtimes are not bundled.
Their actual paths and recursive dependency receipt govern qualification.
Preserve all Rust dependency notices, including Unicode-3.0 and the Rust
standard-library notices. ffmpeg-sys-next declares WTFPL without a packaged
license text; the inventory records that upstream fact.

## Historical rc.1

2.0.0-rc.1 linked the distro GPL FFmpeg configuration. Its archived notices,
source snapshot and qualification remain historical: InternalQualificationOnly,
Superseded, NotForRedistribution. No GPL-capable custom build is silently
reclassified as LGPL. [FFmpeg licensing guidance](https://ffmpeg.org/legal.html)
distinguishes GPL-enabled FFmpeg from the LGPL distribution checklist.

No binary, source archive, or notice file is a patent clearance or universal
legal guarantee. This is the bounded engineering distribution policy for the
recorded RC stack, not legal advice.
