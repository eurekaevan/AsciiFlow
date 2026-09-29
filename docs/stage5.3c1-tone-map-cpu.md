# Stage 5.3C-1 — Method A CPU reference closure

Status: **SEALED**, 2026-09-29. This seals only the permanent internal f64 CPU
reference, not production HDR→SDR. Stage 5.3B-3 remains production authority.
No C-2 gamut work was started.

Starting checkout: `47ef97ea3e29f2a11a87e7d701090250f29f13a2`, initially clean.
Closure has uncommitted C-1 changes; no commit was created. The obsolete
`docs/v2-architecture.md` request is applied to canonical `docs/architecture.md`.
No dependency, shader, SDR renderer, planner, CLI, encoder or interop change.
The only existing renderer change extracts the exact shared post-blend boundary.

## Mathematics and independent evidence

The implementation was checked against the official
[BT.2446-1 (03/2021), §4.1 Tables 2–3](https://www.itu.int/dms_pub/itu-r/opb/rep/R-REP-BT.2446-1-2021-PDF-E.pdf),
not a filter implementation. [Tone-mapping semantics](tone-mapping.md) records
the full equations, exact coefficients, standards versions, type/unit boundaries,
black singularity guard, positive-part accounting and raw output excursions.
Forward formulas have no numbered equations; none are invented in citations.

Decimal-70 expected vectors are independently reproducible from
`tests/fixtures/tone-map/generate-method-a-vectors.py`; they do not use the Rust
function as their oracle. The standard supplies formulas, not these pixel test
files. Direct Table 2 controls also test the published knee reference points
and adjacent representable values. An independent semantic review found no
mandatory scope defect; its normalization-underflow corner was fixed and tested.

| Neutral input nits | SDR nonlinear signal | Zero-black reference display nits |
| ---: | ---: | ---: |
| 0 | 0 | 0 |
| .0001 | .00228796511054 | .0000460 approximately |
| 1 | .0984761359437 | .383702296106 |
| 10 | .236163229470 | 3.13119034888 |
| 100 | .538747871563 | 22.6634155397 |
| 203 | .686855043977 | 40.5953434933 |
| 400 | .832773617744 | 64.4562002468 |
| 1000 | 1 | 100 |

The display-nit column is `100*signal^2.4` for neutral/achromatic diagnostics,
not colour RGB photometric luminance. Neutrality, black, mid-tones, highlight
compression and a 100,001-point ramp pass. Literal knee discontinuities are
small upward jumps, not luminance reversals. RGB/CMY and moderate skin vectors
exercise full Table 3 colour correction and retained saturated excursions.

## Canonical input and byte oracle

The [fixture README](../tests/fixtures/tone-map/README.md),
[identity](../tests/fixtures/tone-map/identity.json), independent JSON vectors and
SHA256SUMS retain the full reproduction contract. Exact command:

```sh
bash scripts/qualify-tone-map-cpu.sh /tmp/asciiflow-c1-reference-final
# invokes:
cargo run --release -p asciiflow-cpu --example qualify_tone_map -- /tmp/asciiflow-c1-reference-final
```

Environment: Rust 1.97.1 (`8bab26f4f`, 2026-07-14), LLVM 22.1.6,
`x86_64-unknown-linux-gnu`, native libm from glibc-2.43-8.fc44.x86_64;
independent evaluator Python 3.14.7.
Input: one algorithmic 1920×1080 absolute-linear BT.2020 frame; all channels
[0,1000] nits. Explicit LE IEEE-754 serialization, no struct padding.
No FFmpeg/media/rand/time/locale dependency for source pixels.

```text
Input bytes: 49766420
Input SHA-256:
a6246d4e5e34c1c69c9cd629e35d82a0f1e8af387436044c82483511bd749f33
Output bytes per run: 116121621
Output SHA-256, runs 1 / 2 / 3 (all identical):
d0eb86dcc265fe7f675d87055f038d258176e331eb2aac412ba7358a6c8c1e6b
d0eb86dcc265fe7f675d87055f038d258176e331eb2aac412ba7358a6c8c1e6b
d0eb86dcc265fe7f675d87055f038d258176e331eb2aac412ba7358a6c8c1e6b
```

Each run: input min/max 0/1000; mapped luma 0/1; corresponding neutral-display
diagnostic 0/100 nits. Raw nonlinear RGB min/max -.0321782182569939 /
1.5958468520638707; 194,400 negative components, 1,056,216 >1 components.
These are expected unbounded intermediate values, not hidden failures or
already codec-ready output. NaN=0, Inf=0, input safety clamps=0, unexpected
clamps=0. Table 3 negative-term positive-part activations=801,032, not RGB clips.

Release sanity mapping time: 282.858582 / 290.197726 / 291.687359 ms/frame;
3.535336 / 3.445926 / 3.428328 FPS. Includes validation/allocation/diagnostics,
excludes compilation/source generation/serialization/hashing. No performance
target or optimization was added; not an isolated production benchmark.

## Integration and production regressions

Builtin 8×8 and FreeType Inconsolata 16×16 integration tests cover qualified
96×64 PQ P010, neutral variations and moderate chroma. They compare B-1 HDR
cells and shared coverage/composed linear pixels, and independently check atlas
coverage, linear blending and encoded P010 Y. Glyph mismatch=0. Existing B-1
tests retain UV averaging, fractional coverage and uneven-cell behavior.
Monochrome rendering cannot feed back into HDR cell decisions. No redundant
PQ round trip, separate renderer or pre-ASCII tone mapping is introduced.

Real Intel Arc Meteor Lake `/dev/dri/renderD128`, iHD 26.1.5, Mesa ANV 26.2.3,
FFmpeg/ffmpeg-libs 8.1.3-1.fc44.x86_64 were freshly inspected. Existing scripts
were used without modifying their expected records:

```sh
bash tests/baselines/media/generate-pq-production-v1.sh tests/fixtures/codecs /tmp/asciiflow-c1-pq
python3 tests/baselines/media/verify-pq-production.py /tmp/asciiflow-c1-pq /tmp/asciiflow-c1-pq-verification.json \
  --input-directory tests/fixtures/codecs --check-baseline tests/baselines/media/pq-production-v1.json
bash tests/fixtures/codecs/generate-8bit-production-baseline.sh /tmp/asciiflow-c1-input8.mp4
bash tests/baselines/media/generate-post-polarity-v2.sh /tmp/asciiflow-c1-input8.mp4 \
  tests/fixtures/codecs/hevc-main10-canonical-v1.mp4 /tmp/asciiflow-c1-sdr
```

The scripts retain exact full CLI configurations, including VAAPI decode,
Vulkan mapping, both interop directions, VAAPI encode, 300 frames, width80,
builtin atlas, standard charset, colour and audio-off settings.
All three runs of each SDR profile match both original MP4 and decoded framemd5
SHA-256 in `post-polarity-v2.json`. SDR input SHA-256 remains
`6e5c214b813dca3e1db65629b1241cfb663166eb565cda1c48b5ee74ed0dce6b`.

| Production profile | All three retained output SHA-256 values equal |
| --- | --- |
| SDR H.264 8-bit | `7c8a7572320b8c6c999143dfece4a76d487d6c9e7788206b7af52df9acd4e1cc` |
| SDR HEVC 8-bit | `294a1b63ac5b0e440dcf60c4c60f975594c6e944829478b69f09f981d268ff71` |
| SDR AV1 8-bit | `1531e29f52ae4e747251cf1889003dfd420303c523fb0ddc55cc9140e3bf2a4c` |
| SDR HEVC 10-bit | `f213a9f75542421bb816550cd7a93796db98ae36296a00636f7273492740a142` |
| SDR AV1 10-bit | `b69c68525ac6ab2464e04b8fc1f123d887b45bba580377bafad99dc74cf62ad7` |
| PQ HEVC Main10 | `0e8de23d5d222ab0476ea816d27b18e37dc73bf1f824e79bedc1070c409ded63` |
| PQ AV1 10-bit | `41a8af52985a6be0121cfe7c4f4160a3ebdecd0acc908882774d2a0151185388` |

PQ verifier checks retained packets, coded/container signal, all decoded pixels,
300 frames/packets at 1920×1080, 50fps, correct timestamps, yuv420p10le,
BT.2020/PQ/NCL/limited/left, no decode errors or source static-metadata leaks.
All retained tiers 1A/1B/1C/2/3 pass, not merely repeated-run self-equality.

Historical H.264: preserved input SHA `6b47b510c4a8f604e6404b1fbbc5f68bdaa77043976acdad0153ef83524b6e34`
and .102 reference SHA `3181fa9775403a2f30e60157797adf370927b4196c960f59d1b5084203d42bef`
were verified. Explicit legacy charset `@%#*+=-:. ` with the historical full
configuration reproduced candidate SHA
`e45071c0c350deae063c8bb10f7e433ee764a107aff01294db391e7f56bf24af`.
The unchanged `compare_pair_from_env` rechecked them: raw packet 1A **FAIL**,
coded semantics 1B/decoded identity 1C/structure 2 PASS, whole file DIFFERENT.
Only the established pinned encoder SEI/container patch-version rule applies;
no exact-build attestation or raw-byte PASS is claimed. This is historical
structured evidence, not the current default-polarity pixel gate.

Planner/CLI tests still reject default eight-bit PQ, HLG, full-range, wide-gamut
SDR, conflicts/unknown metadata and software/CPU PQ paths before output staging.
Production HDR→SDR remains disabled. The earlier B-3 FD/validation stress
evidence remains applicable to untouched ownership/interop code; that expensive
stress suite was not rerun or misrepresented as fresh C-1 coverage.

## Workspace and scope checks

```sh
cargo build --release --workspace
cargo test --workspace
cargo test --workspace --features asciiflow-cli/encode-characterization
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features asciiflow-cli/encode-characterization -- -D warnings
cargo fmt --all --check
git diff --check
```

Both workspace suites: 201 passed, 0 failed, 79 opt-in tests ignored; ignored
tests are not counted as hardware PASS. Additional historical pair and canonical
production validations were explicitly run. Four PQ oracle negative-control
tests pass. All actual 317 cached SPIR-V modules pass
`spirv-val --target-env vulkan1.3`; count unchanged, no shader source changes.
Normal and measurement Clippy/fmt/build/diff checks pass.

## Required closure summary

| # | Required item | Result |
| ---: | --- | --- |
| 1 | Standards/version | BT.2446-1/2021; BT.2020-2, BT.2100-3, BT.1886 |
| 2 | Exact source | §4.1 Tables 2–3; BT.2020 Table 4 inverse |
| 3 | Placement | After HDR ASCII composition |
| 4 | HDR representation | Absolute linear BT.2020 RGB f64 nits |
| 5 | Source assumption | Fixed 1000-nit qualification |
| 6 | SDR representation | Raw nonlinear BT.2020 RGB + YCbCr |
| 7 | Equations | Complete equations in tone-mapping.md |
| 8 | Constants | Published gamma/rho/knee/luma/chroma constants |
| 9 | Colour correction | Full scale and asymmetric Cr luma correction |
| 10 | Black | Zero RGB/chroma, guarded continuous limit |
| 11 | Neutral | Preserved within numerical tolerance |
| 12 | Monotonicity | Ramp and both literal knee boundaries pass |
| 13 | Mid-tones | Independent 1/10/100/203/400 vectors pass |
| 14 | Highlights | 1000→100 reference-white endpoint; compressed contrast |
| 15 | Saturated colours | RGB/CMY and skin pass; raw excursions retained |
| 16 | Clamp policy | No safety/output/gamut clamp; Table 3 max counted |
| 17 | NaN/Inf | Zero on canonical valid output; invalid input rejected |
| 18 | Independent vectors | Decimal-70 and direct published knee controls |
| 19 | Canonical fixture | Deterministic 1080p linear patches/ramps/gradients |
| 20 | Digest | Three identical defined-endian f64 retained hashes |
| 21 | Builtin atlas | Full P010→B-1→C-1 integration passes |
| 22 | FreeType | Same integration with Inconsolata passes |
| 23 | Glyph identity | Zero mismatch; coverage/linear composition unchanged |
| 24 | f64 semantics | HDR/SDR domains separated; no frame quantization |
| 25 | Optional quantization | Not implemented |
| 26 | Above1000 | Rejected/counts retained; 4000/10000 not qualified |
| 27 | Static metadata | Not consumed; cannot alter fixed operator |
| 28 | Performance | 282.86–291.69ms/frame; sanity only |
| 29 | SDR regressions | Five retained profiles ×3 plus decoded hashes pass |
| 30 | PQ regressions | Two retained profiles ×3, all retained tiers pass |
| 31 | Production rejection | Policies unchanged; HDR→SDR not enabled |
| 32 | SPIR-V | All317 actual modules valid for Vulkan1.3 |
| 33 | Workspace/static | Both201-pass suites; release/Clippy/fmt/diff pass |
| 34 | Limitations | Fixed peak; raw excursions; no gamut/output policy; byte oracle platform-specific |
| 35 | C-2 justification | Justified; not started |
| 36 | Final status | SEALED |

Stage 5.3C-2 BT.2020→BT.709 SDR gamut-conversion reference is justified.
