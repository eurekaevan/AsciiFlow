# Stage 4.3 font validation

## Pre-change contract audit

The canonical default ramp is `@%#*+=-:. ` in `AsciiConfig`; user-resolved charset
order is passed unchanged to both mapper and font builder. There are ten default
glyphs (dark to light), with space last, not first. The shared 256-entry luma LUT
uses limited-range normalization, smoothstep, then rounded glyph index. It must
not be calibrated or reordered for a font's ink density.

The old `GlyphAtlas::builtin` expands font8x8 bits LSB-first into glyph-major,
row-major 8×8 R8 tiles (640 bytes for the default ramp). CPU rendering already
reads dynamic width/height. Local coordinates are
`floor(pixel * grid_size * tile_size / frame_size) % tile_size`.
Luma blend is `(background*(255-alpha)+foreground*alpha+127)/255`, background
16, monochrome foreground 235. Color UV uses the top-left cell identity and
average of four coverage samples, blending over 128. These exact semantics stay.

Vulkan uses the same atlas bytes in a storage buffer. Its default Pass 2 variant
is `lut-32x4`, one invocation per 2×2 output block. Host initialization builds
X/Y `(cell, local)` coordinate LUTs using the same floor/modulo rule. Push
constants already carry tile width/height. Atlas, glyph LUT, and coordinate LUT
uploads occur at resource preparation. No shader changes are needed for dynamic
tiles; workgroup names describe dispatch dimensions, not glyph geometry.

The actual missing seam was backend-local built-in construction, including second
Vulkan slots. Stage 4.3 supplies one owned atlas before factory initialization,
copies identical pixels to backend/slot resources, and preserves the sampler and
mapper contracts. Existing Stage 4.2.1 working-tree changes are preserved.

## Implementation and safety

The system library is FreeType 2.14.3; Rust freetype-rs is pinned exactly to
0.38.0, using freetype-sys 0.23.0 without bundled features. Inspection of its
build script confirms pkg-config linking. No native FreeType type crosses the
font API. The existing portable atlas was extended rather than duplicated in core.
CPU and Vulkan accept identical atlas bytes; forks retain the supplied atlas.
No shader, mapper, media/audio or planner algorithm was changed.

Font load happens before staging output. Geometry uses ceil cell dimensions;
one common raster size/baseline fits face metrics and all glyph extents. Normal
outline grayscale uses NO_BITMAP, no color flag. Limits: 32 MiB font input,
16 MiB atlas, tile axes <=4096. Review caught and fixed negative-pitch row origin,
out-of-range CPU glyph panic, and supplied-atlas identity drift. Regression tests
cover these cases. FreeType's own `FT_Bitmap_Convert` negative-pitch handling was
checked against the installed freetype-sys source.

## Verified results (2026-09-15)

- `cargo test --workspace`: 93 passed, 26 ignored; six font tests included.
- Separate FreeType CPU/Vulkan parity: PASS for grayscale and color, three frames
  through two slots with EOF drain, and rejection of reordered-ramp reuse.
- Khronos layer enabled on lavapipe: validation counter zero; no VUID/validation
  error in the real-media FreeType + AAC software-media/Vulkan smoke.
- Release workspace build, strict all-target Clippy, fmt check, diff check: PASS.
- Original Stage 4.2.1 binary versus Stage 4.3 built-in, same 300-frame media:
  complete MP4 bytes identical, SHA-256
  `b02212fbe8bdad0d8851059e91f2c5f15d803ccb236722ae3722af842dc74514`.
- CLI tests confirm explicit missing/proportional fonts preserve existing output;
  capability reports do not load them. FreeType + audio keeps all 90 video frames
  and exact audio packets. Invalid file, missing glyph, invalid face index,
  extreme/zero geometry, space, duplicate identities and descender tests pass.

Manual visual smoke used the host's Source Code Pro Bold OTF (not copied into
the repository). The diagnostic 13-glyph ramp `@%#*+=-:. Ag_` at 24×24 produces
7,488 R8 bytes, common ppem 18/baseline 18, and SHA-256
`7257cbaeef8e96a225bf38ca291d2f302bea3e493f056ab96343277e4aa84331`.
Viewed enlarged atlas and a decoded 1080p-video frame: grayscale coverage visible,
common alignment, blank space, visible g descender/underscore, no atlas corruption.
The production ten-glyph default ramp at 24×24 uses 5,760 bytes. Representative
CPU startup wall: font load 0.104 ms, atlas build 0.267 ms. These are observations,
not cross-version golden raster expectations.

An Inconsolata 16×16 atlas (2,560 bytes) in the lavapipe media smoke uploaded once
per slot: 0.052 and 0.088 ms CPU wall, including submission/completion. This is not
a GPU copy timestamp and is outside frame/backend timing. That 64×64 diagnostic
run measured GPU render 0.871 ms, backend wall 7.703 ms and 382.17 FPS; it is a
software Vulkan correctness smoke, not an Intel performance result.

## CPU regression measurement

The prior Intel Stage 4.2 source workload/device are unavailable here. A separate
self-generated 1920×1080/30 fps/300-frame testsrc2 + 48 kHz AAC sample was used,
ASCII width 80, color enabled, software decode/CPU/software encode, Release.
Each case had a warm-up then three runs; no validation layer was enabled.

| Case | FPS runs | Median FPS | Median render ms | Median backend wall ms |
| --- | --- | ---: | ---: | ---: |
| Stage 4.2.1 built-in | 50.71, 53.11, 50.98 | 50.98 | 18.651 | 19.554 |
| Stage 4.3 built-in | 52.00, 53.59, 52.94 | 52.94 | 18.036 | 18.832 |
| Stage 4.3 FreeType | 52.54, 52.98, 52.42 | 52.54 | 18.144 | 18.969 |

CPU render/backend are CPU wall measurements, not GPU Pass 2 timestamps. No
material regression observed in this sample (FreeType vs current built-in -0.76%
FPS); differences versus the older binary are not evidence of an optimization.
Font appearance changes encoded complexity, so total FPS is not a pure renderer
microbenchmark. The shader/LUT formulas remain unchanged.

No `/dev/dri` is exposed. Intel full VAAPI/Vulkan/VAAPI + FreeType + audio smoke
and the Intel <3% steady-state performance gate were **not rerun**. Lavapipe
evidence cannot satisfy that hardware claim. This is the remaining optional
host-validation item, not a reason to introduce new optimization or features.

## Scope and next step

ASCII ramp only; no shaping, ligatures, fallback, color glyphs, font discovery,
axis controls, density calibration, new codec or platform. The next validation
step is the existing Intel workload with explicit full interop and this atlas;
no further implementation stage was started. See `fonts.md` for API/geometry
details and `tests/fixtures/fonts/README.md` for official font provenance/licenses.
