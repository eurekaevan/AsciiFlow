# Internal BT.2020/PQ P010 pixel reference

The CPU pixel reference is a qualification oracle, **not a production HDR backend**.
The SDR color assessment rejects PQ; hardware production uses a separate
pipeline-aware `HdrPqPreserve` admission path. See the
[production contract](production-support.md) for exact support conditions.
HLG, BT.2020 SDR, full-range and unknown/conflicting inputs remain rejected.
The CPU oracle is never a production HDR fallback. Explicit hardware PQ-to-SDR
conversion is documented separately in [tone mapping](tone-mapping.md).

The only accepted oracle descriptor is host P010LE 4:2:0, limited range,
BT.2020 primaries, BT.2020 non-constant-luminance matrix, PQ transfer, and
`Left` chroma location. Every 16-bit little-endian word carries a ten-bit code
in bits 15..6; nonzero low padding bits are rejected. Input codes outside the
nominal Y 64..940 or Cb/Cr 64..960 ranges are rejected, not interpreted as
super-black/white. Neutral chroma is 512. These are signal-code bounds, not
limits on physically useful mastering brightness.

For each luma sample, the containing 2×2 chroma block is reused (nearest,
left/cosited geometry). Codes become Y′ = (Y−64)/876 and Cb/Cr =
(code−512)/896. BT.2020 NCL uses Kr=0.2627, Kb=0.0593, Kg=0.6780:
R′=Y′+2(1−Kr)Cr, B′=Y′+2(1−Kb)Cb,
G′=(Y′−KrR′−KbB′)/Kg. Legal Y′CbCr can yield RGB outside [0,1]; each such
encoded component is clamped before PQ decoding and counted diagnostically.
This is clipping, not gamut mapping. Unknown/center chroma siting is rejected;
the oracle is not a general chroma-siting colorimetry implementation.

PQ uses the ITU-R BT.2100 / ST 2084 display EOTF with exact rational
constants m1=2610/16384, m2=(2523/4096)×128, c1=3424/4096,
c2=(2413/4096)×32, c3=(2392/4096)×32. The EOTF is applied separately to
R′, G′, B′; applying it to Y′ alone would not produce true luminance.
`LinearRgb` components are display-referred absolute cd/m² (nits), 0..10000.
Luminance is 0.2627R+0.6780G+0.0593B, also in nits.

Each cell averages all decoded pixels in linear RGB. Its foreground is that
mean, without shadow boost, highlight compression, frame normalization, or
exposure adjustment. Black background is linear RGB = 0 nits. Monochrome mode
uses the cell's physical luminance equally in all three channels. Glyph
selection converts mean **linear luminance** back through inverse PQ, maps
the perceptual scalar into the existing limited-range 256-entry glyph LUT,
then uses its existing S-curve. This curve changes only the selected glyph;
it never remaps the foreground light.

Atlas R8 coverage is divided by 255 and blended with black in linear RGB.
Each output pixel passes through inverse PQ, BT.2020 NCL RGB→YCbCr, then
round-to-nearest (ties away from zero) limited ten-bit quantization. The 2×2
output chroma codes represent the average of four **encoded chroma components
after per-pixel linear blending**, not a sampled top-left cell. Output codes
outside nominal range are clipped and counted; padding bits remain zero.
The diagnostic record contains min/max linear component and luminance,
component clipping count, and nonfinite count; nonfinite input fails rather
than producing output. The public oracle does not attach static HDR metadata:
source MaxCLL/MaxFALL and mastering-display data must not be blindly copied to
ASCII-transformed output. There is no propagation contract yet.

The tests construct deterministic raw P010 patterns in memory: neutral black,
100/1000/10000-nit gray, a two-dimensional 8×8 luminance gradient with
four colored patch families, adjacent ten-bit code values, and direct
4:2:0 chroma blocks. These are mathematical test signals, not mastered art or
calibrated display images. Numeric precision is f64; no performance target is
set for this reference path.

Standards: [ITU-R BT.2100-3](https://www.itu.int/rec/R-REC-BT.2100-3-202502-I)
and [ITU-R BT.2020](https://www.itu.int/rec/R-REC-BT.2020).

## Internal Vulkan qualification

The independent `hdr-pq-qualification` feature exposes `VulkanPqQualification`,
not a CLI or planner mode. The CPU f64 oracle above remains authoritative and
unchanged. Separate `ascii_map_pq.comp`, `ascii_render_pq.comp` and
`pq_common.glsl` use f32 normalized linear RGB (1 = 10000 nits), with exactly
the same per-channel PQ equations, legal-code policy, floor cell partitions,
glyph LUT and linear-light R8 blending. They do not approximate PQ with a LUT,
tone map, normalize exposure, or copy source mastering metadata.

The map pass uses 32 lanes and a shared-memory tree reduction. A std430 cell
is 32 bytes: normalized RGB plus perceptual scalar, then glyph/input clips/
invalid samples/output clips. The render pass processes 2×2 blocks using a
32×4 workgroup, averages unquantized encoded Cb/Cr once, and quantizes once
before packing ten-bit codes into bits 15..6. Diagnostic atomics and cell
readback have explicit compute/read-write and transfer/host barriers. Invalid
padding, codes, or nonfinite arithmetic produce a terminal processing error
before output is returned or copied to a VAAPI surface.

The measured Intel qualification bounds are normalized RGB/luminance absolute
error ≤4e-5, relative error ≤6e-5 for nonzero reference values, and inverse-PQ
scalar absolute error ≤2e-5. Black uses the absolute bound because relative
error is undefined. These bounds were fixed after characterization, with
roughly twice the observed maximum as margin; they are not a universal GPU
precision guarantee. Exact glyph equality and final per-plane error ≤1 active
ten-bit code remain independent hard gates. See the
hardware report for measured errors, fixtures,
FD/failure evidence and performance. Production PQ preservation reuses these equations unchanged
for the explicitly planned full-P010 hardware path; source mastering-display,
MaxCLL and MaxFALL are removed, not copied or recomputed. HDR10 static mastering
is not qualified by PQ preservation.
