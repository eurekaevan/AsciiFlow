# Canonical PQ source v1

Generate both lossless codec inputs from one full-resolution, deterministic
1920×1080, 300-frame, 50-fps ten-bit source:

```sh
bash tests/fixtures/codecs/generate-pq-canonical.sh /path/to/new-output-directory
```

The Linux generator requires FFmpeg 8.1.3 with shared libx265/libaom and Python 3.
`ASCIIFLOW_FIXTURE_FFMPEG` and `ASCIIFLOW_FIXTURE_PYTHON` select those tools.
Existing named outputs are refused. Existing SDR and small PQ fixtures are not
regenerated or retagged.

Outputs are `hevc-main10-pq-canonical-v1.mp4`,
`av1-main10-pq-canonical-v1.mp4`, `pq-canonical-v1-identity.json`, and
`pq-canonical-v1-generator-version.txt`. Video is BT.2020 non-constant luminance,
PQ, limited range, left chroma, with no audio or static mastering metadata.
The identity JSON records the generator scripts, complete FFmpeg version and
binary/native-library hashes, encoded hashes, raw source SHA256, every raw frame
SHA256, and exact per-plane low-two-bit histograms. Those raw hashes identify
planar unshifted `yuv420p10le` bytes, not shifted/interleaved P010 bytes.

The source has neutral black/near-black and 100/1000/4000/10000-nit quantized
bars, a full ten-bit moving gray gradient, colored bars, and a moving highlight.
All sample generation uses fixed integer code tables and arithmetic. No scaling,
eight-bit intermediate, random source, timestamp, or runtime PQ power calculation
affects the source. Tables and formulas are included in the identity JSON.

The temporary raw source is 1,866,240,000 bytes. Allow at least 2 GiB plus encoder
outputs in a scoped temporary directory under the output directory. It is removed on exit, including
generation/encoding failures. Final files are published only after both encodes
and identity collection complete; publication does not replace existing files.

The reference toolchain is FFmpeg 8.1.3, Python 3.14.7, x265 4.1,
and libaom 3.13.3. Its identities were:

- FFmpeg binary: `c71eb1c6b9d57114ea567b1525c393295285df3ad21b17ad8144e02832d8ed92`
- Complete FFmpeg version output: `87e52642589a2ffec4fd56da1eeb8d36810c3ba81cac81a4a71039041b89331e`
- libx265: `4076f7e910ff4b4981492045e00407d19336c1496b96a756d35aa2aacf61ddb8`
- libaom: `4b4bbf480ac95bf5b9650c150426a1230841a93f7e471afedde6e82728398219`

Before declaring byte identity reproducible, run three independent generations
and compare the source/per-frame and encoded
hashes with the recorded native toolchain identities. Software and VAAPI decode
checks must also confirm all 300 frames, 50-fps timing, metadata, genuine ten-bit
low bits, and lossless source parity. A matching source hash alone does not
attest matching encoded bytes on a different native build.

Verify every decoded frame and every sample against the source identity:

```sh
python3 tests/fixtures/codecs/verify-pq-canonical.py /path/to/output-directory /path/to/verification.json
```

The verifier checks stream metadata, declared and actual frame counts, complete
decoded frame/source hashes, legal sample ranges, and actual low-two-bit
histograms on both codecs. It requires the same FFmpeg build as generation and
keeps only one decoded frame in memory. The report records all frame hashes and
actual decoded sample statistics.

Raw input color metadata must be specified before `-i`, as well as on output.
An initial attempt with only output metadata passed stream probing but changed
source samples before lossless encoding (first-frame deltas reached Y 33, U 19,
V 22). Matching explicit input metadata restores source parity without changing
the encoder's lossless parameters. Metadata-only inspection cannot establish
losslessness.

AV1 lookahead and alternate-reference processing are explicitly disabled with
`-lag-in-frames 0 -auto-alt-ref 0`. A 300-frame encode with only `-crf 0` preserved
the first frame but changed 43 samples of the second frame by one code. Controlled
40-frame encodes reproduced that difference even with `-aom-params lossless=1`;
disabling lookahead/alternate-reference processing restored source parity.
