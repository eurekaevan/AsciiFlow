# Fonts

`--font builtin-8x8` retains the deterministic built-in default.
`--font /path/to/mono.ttf` selects a font file; FreeType, not its extension,
determines validity. `--font-face-index N` selects a collection face, default 0,
and is accepted only with an explicit font file (including an explicit zero).
`--font monospace`, `--font "JetBrains Mono"`, or
`--font "JetBrains Mono:style=Bold"` uses the system Fontconfig configuration
on Linux. Fontconfig selects one file and its collection face index; FreeType
still performs rasterization and the final scalable/monospace/glyph checks.
An unsuitable match fails without trying another face.

Existing paths, absolute paths, `./` or `../` prefixes, and values containing
either path separator are file requests. A missing file never becomes a font
family lookup. Paths are canonicalized before atlas identity is established;
identity retains the resolved file, face index, ramp and existing geometry.
Diagnostics additionally retain the requested pattern and resolved font name.

Discovery uses `yeslogic-fontconfig-sys` 6 (MIT), the runtime-loading backend of
`fontconfig` 0.11, with `dlopen`. Direct native calls preserve full pattern syntax
and release both parsed/matched patterns exactly once. Missing Fontconfig is a
normal `FontconfigUnavailable` error only for a name/pattern request, suggesting
`builtin-8x8` or an explicit file. It does not add a link-time Fontconfig dependency.
System discovery uses fonts.conf, aliases and caches, not a project font scan.
Fontconfig's process-global configuration/cache may remain resident; match objects
are destroyed during initialization. No discovery happens during frame processing.
Omitting `--font` retains `builtin-8x8` and its original output. Explicit missing,
invalid, non-scalable, proportional, missing-glyph, or unsupported pixel-mode
fonts fail before staging/mux creation, without fallback. Errors name the font,
operation and native/context cause. Missing glyphs include character/code point.
Capability-only reports do not open fonts. Font choice does not affect planning.

## Geometry and ownership

The font crate owns freetype-rs 0.38.0, linked to system FreeType through
freetype-sys/pkg-config (no bundled feature). The CLI resolves the existing grid
first; tile dimensions are ceil(frame width/columns), ceil(frame height/rows).
Grid, resolution, mapping and timing do not change. There is no font-size/DPI
option. Cells too small for a common valid raster fail explicitly.

One common pixel size fits the face ascender/descender and entire ramp's bitmap
extents into the tile. The horizontal origin centers the common advance/extents
interval, not each bitmap. Bearings remain relative to that origin; all glyphs
share one baseline and size. Signed pitch and negative bearings are handled.
Outline loading uses NO_BITMAP with default hinting, followed by NORMAL grayscale
rasterization. No color loading is requested. GRAY keeps 8-bit coverage; MONO
expands to R8; other modes fail. Space has a valid blank tile. Duplicate ramp
characters retain separate indices; there is no ink-density reorder/calibration.

The portable `GlyphAtlas` contains owned, tightly packed glyph-major row-major R8
tiles and dimensions, with no native handles. FreeType face/library are released
after construction. Identical pixels go to CPU and every Vulkan slot. Each Vulkan
resource uploads its atlas once at initialization. Frames never call FreeType.
Supplied atlases are bound to font/ramp identity, rejecting reordered-ramp reuse.
Atlas size is checked and capped at 16 MiB, scalable tile axes at 4096, and font
input at 32 MiB. Nearest-coordinate sampling and exact integer Y/UV blending stay
unchanged. Vulkan uses its existing buffer, push constants and host coordinate LUT.

## Diagnostics and tests

Verbose reports family/style, face index, runtime FreeType version, geometry,
atlas bytes, common ppem/baseline and load/build CPU wall times. Vulkan logs atlas
upload CPU wall time (submission plus completion, not a GPU timestamp). These
initialization costs are outside steady-state backend timings.

Default workspace tests cover font structure, errors, baseline/descender, duplicate
glyphs and CLI output safety. Test fonts come from pinned official Google Fonts
revisions with SIL OFL licenses/hashes in `tests/fixtures/fonts/README.md`.
No installed system font is redistributed.

CPU/Vulkan exact parity, including two slots and odd EOF:

```bash
ASCIIFLOW_VULKAN_ALLOW_CPU=1 \
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json \
ASCIIFLOW_VULKAN_VALIDATION=1 \
cargo test -p asciiflow-vulkan freetype_cpu_vulkan -- --ignored
```

This is a Fedora lavapipe example, not a production default. On Intel omit the
CPU allowance and select the real device. The `asciiflow-font` example `atlas`
accepts `FONT OUTPUT.pgm` and emits a visual PGM plus raw R8 bytes for hashing.
Raster hashes are stable only for the same font, FreeType, geometry and request.

Scope: ASCII ramp, scalable monospaced fonts. No shaping, ligatures, fallback,
color glyphs, variable-axis controls, density
calibration, or full Unicode typography is promised.

The ignored `system_monospace_matches_explicit_atlas` test exercises local
Fontconfig discovery and proves name/path atlas equality for the resolved face.
Default tests use pinned repository fonts rather than installed family names.

The recorded validation host used Fontconfig 2.17.0. Its default `monospace` alias selected
`NotoSansMono[wght].ttf`, which failed the unchanged FreeType fixed-width check.
That is an expected rejection, not permission to choose another font silently.
Installed Liberation Mono passed family/file equivalence; an isolated Fontconfig
configuration mapping `monospace` to that same face passed the local alias test.
This is a host-specific observation, not a universal font blacklist. See the
[qualification report](font-terminal-validation.md) and its recorded controls.
