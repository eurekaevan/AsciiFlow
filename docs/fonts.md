# Fonts (Stage 4.3)

`--font /path/to/mono.ttf` selects a font file; FreeType, not its extension,
determines validity. `--font-face-index N` selects a collection face, default 0.
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
color glyphs, name discovery, Fontconfig, variable-axis controls, density
calibration, or full Unicode typography is promised.
