# Testing and regression policy

Development is organized around changes and regressions, not numbered stages,
sealing ledgers or future-stage plans. Run checks proportional to the affected
behavior; do not repeat completed soak campaigns for documentation-only changes.

## Portable checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
git diff --check
python tests/support/matrix.py check
python -m unittest discover -s tests/corpus -p 'test_*.py'
python -m unittest discover -s tests/soak -p 'test_*.py'
```

The support checker validates the generated README and
[support document](production-support.md) against the one machine-readable
contract. Planner acceptance, mocks and inspection alone do not establish
hardware qualification. Ignored tests are not passes.

CI also validates generated shaders:

```sh
find target/debug/build -path '*/out/ascii_*.spv' -print0 |
  xargs -0 -n1 spirv-val --target-env vulkan1.3
```

## Targeted media regression

Use the [compatibility corpus](compatibility-corpus.md) for planner, container,
timing, display-transform, corruption and audio-copy changes. Retained media
oracles and comparison tools live under `tests/baselines/media/`; numeric
reference data lives under `tests/fixtures/tone-map/` and
`tests/baselines/tone-map/`. Their generators, checksums, licenses and reference
values remain test inputs, not obsolete planning documents.

Software tests require the capabilities of the actual linked FFmpeg. In
particular, developer software H.264 output requires libx264; the official
`lgpl-prebuilt` release explicitly excludes that route. Do not relax or skip a
valid oracle merely because a dependency stack changed.

## Hardware checks

VAAPI/DMA-BUF/Intel Vulkan tests require real `/dev/dri` access and the intended
driver stack. Select relevant ignored integration tests explicitly. For example:

```sh
cargo test -p asciiflow-font --locked system_monospace_matches_explicit_atlas -- --ignored
cargo test -p asciiflow-vulkan --features hdr-pq-qualification \
  --test pq_qualification pq_freetype_mapping_and_render_match_cpu --locked -- --ignored
```

The font test depends on a locally installed acceptable monospaced face.
Fontconfig matching and FreeType acceptance are separate gates; an unsuitable
match must reject without fallback. The Vulkan command above checks host P010
compute parity, not hardware-decoder interop. Consult [fonts](fonts.md),
[P010](p010.md), [HDR semantics](hdr-pq-semantics.md) and
[portability](portability-testing.md) for each test's actual evidence boundary.

For CLI/font/UI changes, use short SDR, PQ-preserve and explicit PQ-to-SDR
production smokes, full decode-back, timestamps, color metadata and audio-copy
checks. Verify redirected output and real terminal cancellation/error cleanup
separately. [Font/terminal results](font-terminal-validation.md) retain an
executed example; they do not imply a new package release.

## Long runs and release checks

[Reliability](reliability-testing.md) describes the single-job process contract,
resource observations and cleanup. Extend long-run testing only when the change
or an actual anomaly warrants it. [Release qualification](lgpl-release-qualification.md)
and the [final 2.0.0 receipt](asciiflow-2.0.0-final.md) retain exact dependency,
source and package provenance. Historical failures remain historical results,
not requirements to redesign the product or rerun every campaign.

## Evidence retention

Every new benchmark or retained artifact must record its generator, exact tool
version and command, input SHA-256, conversion command, source/build/runtime
identity, and output SHA-256 or a justified structured nondeterministic oracle.
Never guess an unavailable old input or label a new fixture as its recovery.
Hardware/software parity, packet identity, decode-back and whole-file identity
are distinct claims. Keep existing strict and stack-scoped oracle rules intact.

Completed stage-planning documents and redundant closure ledgers are no longer
part of the active tree. Useful baselines, release provenance and test runners
use functional names. Historical IDs inside immutable receipts or captures
describe their origin; they do not create future development stages.
