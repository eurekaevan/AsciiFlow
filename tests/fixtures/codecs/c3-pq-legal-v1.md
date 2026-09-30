# C3 legal-domain PQ source v1

Generate a fixed 1920×1080, 300-frame, 50-fps source and encode HEVC Main10
and AV1 Main10 losslessly from the same tight P010 input:

```sh
bash tests/fixtures/codecs/generate-c3-pq-legal-v1.sh /path/to/new-output-directory
```

The generator requires FFmpeg 8.1.3 with identifiable shared libx265/libaom
and Python 3. It honors `ASCIIFLOW_FIXTURE_FFMPEG` and
`ASCIIFLOW_FIXTURE_PYTHON`. It refuses to replace named outputs. The generated
identity JSON includes source and per-frame P010 SHA-256 values, low-two-bit
histograms, toolchain/library identities, and encoded-file hashes. The raw
1,866,240,000-byte source exists only in a scoped temporary directory and is
removed after generation.

The source stores 10-bit codes left-shifted by six in little-endian words: Y
followed by interleaved UV. It signals BT.2020 non-constant luminance, SMPTE
ST 2084 PQ, limited range, and left chroma. The top region contains ten neutral
gray bars; the middle contains a neutral 64–713 code gradient and a moving
900-nit patch; the bottom contains eight fixed BT.2020 color bars. Supplied
tables map the gray and color design values to integer Y/U/V codes. The
generator performs no per-frame PQ math, scaling, clipping, or color conversion.

The identity describes the source recipe and encoded bytes; it does not by
itself establish decoded codec parity, actual RGB-domain legality, CPU/GPU
agreement, or hardware decode behavior. Those require separate full-frame
software and VAAPI audits (completed and recorded below). This fixture is separate and
does not replace or alter the existing B3 source or its above-1000-nit cases.

## Established identity and decoded-domain qualification

Final generator runs 3/4/5 have identical source, encoded files and manifests.
Runs 1/2 precede a histogram-accounting fix and are not the final three runs.

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| Tight P010 source | 1866240000 | `7e28e4832332a7febe72e9c6b9fb4b7fbad0d141ed204b4f9267f50e2ff87a37` |
| HEVC Main 10 | 1927266 | `eb8d8c42cd99369b99031900e8861c08f611bfcc662b8d0eb741eb695a0c366a` |
| AV1 Main, yuv420p10le | 8502612 | `7eb4fd7e7513fc181ad8489e13edbac10a88a8f3a7947d2f958d36a0ce08e7c4` |
| Identity manifest | 140488 | `557357f31a16b7f68f8c3a4407f02edba5f949c391f9630f5fff31f39ffa375a` |

Python generator SHA256 is
`c7ddf94a576d3d96934a56a4538a0086a04298fa9751ec434ea81af0f18eba65`;
shell generator SHA256 is
`b264d019ad7406dfd0c7ce7f4102c59679dae482bf46469a71ee68ae1abd41bd`.
The [manifest](c3-pq-legal-v1-identity.json) retains all 300 raw-frame hashes,
full FFmpeg 8.1.3 build/configuration and binary identity, Python identity,
libx265/libaom file hashes and all codec settings; the shell script is the exact
generation command authority. Designed highlight peak is 900 nits; quantized
decoded maximum is 904.3172845983241 nits, with deliberate headroom below 1000.

All 300 software and VAAPI-download frames from each codec were independently
checked in the [unclamped-domain report](../../baselines/tone-map/c3a-legal-domain-v1.json).
Each path has min 0/max 904.3172845983241 nits and zero negative/>1000/nonfinite/
invalid-code components. Timebase is 1/50000, PTS 0..299000 with step 1000.
All raw P010 words also have valid zero low-six-bit padding, checked before
unpacking; this is distinct from the genuine low-two-bit precision of the ten-bit
codes. The manifest's `active_samples_per_plane` values refer to one frame;
its aggregate histograms and nonzero counts refer to all 300 frames.
Y/U/V active counts are 622080000/155520000/155520000; nonzero low-two-bit counts
are 498811212/45360000/38880000 (total 583051212 out of 933120000).
Both fixed-tool software decodes reproduce the raw P010 SHA exactly. The project
test also checks compressed input SHA before decoding; replacing bytes behind
the same filename is a failure, not a new canonical identity.

This is now a qualified **input domain**, not a qualified C3 conversion output.
No production/HDR-preserve baseline is replaced. The original C3 numerical
gate remains red; complete legal-source CPU/GPU parity and stresses remain open.
