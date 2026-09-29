# Canonical C-1 Method A linear fixture v1

Run from the repository root with the recorded Rust/native math environment:

```sh
bash scripts/qualify-tone-map-cpu.sh /tmp/asciiflow-c1-reference
python3 tests/fixtures/tone-map/generate-method-a-vectors.py > /tmp/method-a-vectors.json
cmp /tmp/method-a-vectors.json tests/fixtures/tone-map/method-a-vectors.json
```

The wrapper builds the Release example, creates the source and three mapped
outputs, compares bytes, prints diagnostics/timing and checks retained SHA256SUMS.
Large generated binaries are not checked in. No randomness, locale, clock,
external media or FFmpeg is used. Timing does not enter serialized data.

The single frame is 1920×1080 absolute BT.2020 RGB in [0,1000] cd/m². Its top
half has sixteen patches: black, .0001/100/203/400/1000-nit neutrals, RGB/CMY,
three moderate skin-like colours, and 1-nit neutral. The next sixth is a horizontal
achromatic ramp; the last third has horizontal/vertical/modular chroma gradients.

Input bytes: `AF-C1-IN-v1\0`, u32 little-endian dimensions, row-major R/G/B
IEEE-754 f64 little-endian samples. Output: `AF-C1-OUT-v1\0`, same dimensions,
seven f64 fields per pixel: R/G/B, corrected Y/Cb/Cr, pre-correction mapped luma.
No padding/native struct layout. Input is 49,766,420 bytes; each output is
116,121,621 bytes. There is no frame rate or codec for this mathematical frame.

`identity.json` retains commands, generator hashes, environment, diagnostics,
three digests and sanity timings. The separate Decimal-70 evaluator reads no
Rust output. Official tables provide formulas, not pixel-vector test files;
`method-a-vectors.json` is an independent evaluation, not an official dataset.
Rust tests embed neutral, RGB/CMY and skin values plus published knee controls.

Exact-byte portability across different compiler/libm implementations is not
claimed. Separate P010 integration tests reuse the B-1 CPU renderer with builtin
and FreeType atlases, including moderate chroma. No production feature is enabled.

See [semantics](../../../docs/tone-mapping.md) and
[closure](../../../docs/stage5.3c1-tone-map-cpu.md).
