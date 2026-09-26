# Color semantics (Stage 5.3A)

Color is independent of codec, NV12/P010 storage, depth and chroma. In
particular, P010 and BT.2020 do not imply HDR. Core owns portable primaries,
transfer, matrix and range enums; FFmpeg's `AVCOL_*` values are mapped only in
Media. Unknown native values retain their numeric identity. PQ (SMPTE ST 2084)
and HLG (ARIB STD-B67) are transfer characteristics, not bit-depth profiles.

## Signal, resolution and support

`ColorMetadataRaw` retains the stream and decoded-frame signals, including
static HDR side data. `ResolvedColorSemantics` separately carries the effective
fields, per-field provenance (`Frame`, `Stream`, `LegacyDefault`, `Unknown`),
dynamic-range class and processing-support decision. An explicit frame field
has priority over an unspecified stream field. Two different explicit values
are **Conflicting**, never silently reconciled. The first decoded frame sets
the stream's resolved semantics; a later effective color or static-metadata
change is a hard error. The CLI qualification probe decodes that first frame
before selecting a plan or creating an output.

The stream fields currently come from the FFmpeg decoder context populated
from `AVCodecParameters`; coded stream side data is read from codec parameters.
Frame fields and frame side data come from the decoded `AVFrame`. This is a
deliberate source hierarchy, but the raw codec-parameter/context values are
not yet retained as *two separate* provenance layers. A future source that
makes them disagree needs explicit qualification, not an assumed winner.

The Stage 5.2 8-bit compatibility default remains BT.709/limited for fields
that are wholly unspecified. Strict 10-bit resolution does **not** default
missing primaries, transfer or matrix to BT.709. The raw unspecified values
remain visible even when an 8-bit effective default is used. Known transfer
with missing required primaries/matrix is `Unknown` under strict resolution.
For an untagged 8-bit source, the legacy software scaler still assumes
ITU601 *source coefficients* before normalizing to BT.709 output. A defaulted
BT.709 effective tag does not silently change those source coefficients;
explicit BT.601/170M input is routed through the same software normalization,
not direct VAAPI decode.

| Signal | Dynamic range | Production processing |
| --- | --- | --- |
| BT.709 primaries/transfer/matrix, limited range | SDR | Supported for qualified NV12/P010 paths |
| 8-bit BT.601/170M limited-range SDR | SDR | Software decode and BT.709 NV12 normalization only |
| BT.709 with unspecified fields, 8-bit legacy context | SDR after documented default | Existing NV12 conversion behavior retained |
| Missing required 10-bit fields or unknown transfer | Unknown | Rejected before staging |
| BT.2020 primaries or matrix with BT.709 transfer | SDR, not HDR | Rejected: wide-gamut math not implemented |
| Display P3 with SDR transfer | SDR, not HDR | Rejected: gamut not implemented |
| PQ transfer | HDR PQ | Rejected: HDR pixel processing not implemented |
| HLG transfer | HDR HLG | Rejected: HDR pixel processing not implemented |
| Disagreeing explicit stream/frame fields | Conflicting | Rejected |
| PQ/HLG with explicit BT.709 primaries and matrix | HDR class, contradictory metadata | Rejected as conflicting |
| Full-range SDR | SDR | Rejected pending range-aware ASCII pixel math |

The renderer's black/white code values are currently fixed to limited range
(NV12 16–235, P010 64–940). Consequently, a full-range input cannot honestly
be emitted with a full-range tag. Stage 5.3A rejects it instead of changing
pixel mathematics or silently relabeling it. This narrows a previously
permissive P010 route; it is a correctness correction, not an HDR feature.
For accepted BT.709 limited-range SDR, the existing encoder writes BT.709
primaries, transfer, matrix and MPEG/limited range. Eight-bit scaling retains
its legacy BT.709/limited normalization; no GPU shader or interop path changed.

## Static HDR metadata

Mastering-display CIE xy coordinates and luminances are stored as exact signed
integer rationals; luminance units are cd/m². Content-light metadata stores
optional MaxCLL/MaxFALL in cd/m², distinguishing missing/zero from a positive
value. Parsing validates payload extent, flags, denominators, nonnegative
values, xy bounds, luminance ordering and MaxFALL ≤ MaxCLL. Malformed payloads
fail before processing. The native structs are local, checked C-layout views
because this version of `ffmpeg-sys-next` does not generate bindings for those
two payload types. No HDR side data is written: HDR output is unsupported.

Static metadata neither proves nor is required for HDR classification. PQ
without mastering metadata is still HDR PQ; mastering/CLL data without a PQ
or HLG transfer does not promote SDR to HDR. No dynamic HDR metadata, ICC,
gamut conversion, tone mapping or HDR pixel interpretation is implemented.
