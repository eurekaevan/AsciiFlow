# Testing and regression policy

Development is organized around changes and regressions, not numbered stages,
sealing ledgers or future-stage plans. Run checks proportional to the affected
behavior; do not repeat completed soak campaigns for documentation-only changes.

## Portable checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo check --workspace --all-targets --all-features --locked
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

`--all-features` is useful for compilation and Clippy. It also enables
`lgpl-prebuilt`, which deliberately disables software H.264 encoding even when
the linked FFmpeg has libx264. Software-encoding regression tests therefore use
the developer configuration, with diagnostic features selected explicitly and
`lgpl-prebuilt` omitted. This keeps the release restriction and the software
test contract intact; it does not justify skipping failing software tests.

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

Check device visibility in the environment that will execute the tests. A
sandbox without `/dev/dri` does not prove that the host lacks a GPU. Where host
access is authorized, inspect it separately with `vainfo --display drm --device
/dev/dri/renderD128` and `vulkaninfo --summary`; use the actual render node if
different. Install missing tools only when needed. Lavapipe compute parity
does not establish VAAPI or DMA-BUF interoperability.

Relevant resource-lifecycle regressions can be selected with:

```sh
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-interop --locked \
  --features p010-output-diagnostic --test hardware \
  p010_output_faults_preserve_cause_and_do_not_reuse_surfaces -- --ignored --exact
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-vulkan --locked \
  --features hdr-pq-qualification --test pq_qualification \
  pq_injected_faults_release_resources_and_allow_fresh_retry -- --ignored --exact
ASCIIFLOW_VULKAN_VALIDATION=1 cargo test -p asciiflow-vulkan --locked --lib \
  backend::tests::abandoned_device_rejects_resource_reuse_and_replacement \
  -- --ignored --exact
```

These hooks exercise predetermined initialization, import, submission and
completion error checkpoints, or mark a device abandoned to check reuse
rejection. They do not force a native driver hang or real device loss. Verify
that the output actually ran the intended test: a successful command reporting
zero tests establishes nothing. Run process-wide FD checks in isolation.

On 2026-10-10, the audit repairs were exercised on Intel Arc MTL
(`8086:7d55`), Mesa ANV 26.2.4, iHD 26.1.5 and system FFmpeg 8.1.3.
Fourteen explicitly selected hardware regressions passed: NV12 descriptor and
pre-ASCII parity, two-slot ASCII, H.264/HEVC encoder-owned full interop,
P010 host-output/full-interop/FreeType parity, P010 output fault checkpoints,
PQ fault cleanup and fresh retry, abandoned-device reuse rejection, P010
two-slot compute and all three NV12 render variants. Khronos validation was
enabled. This was a bounded developer Debug rerun of the uncommitted repairs
based on `b55bc155c4a974ee4dbb866e81fd23955653c413`, not a new release,
performance baseline, long soak or native device-loss qualification. Retained
codec/font fixtures and their checksum manifests supplied the media and atlas;
compute-only cases used the existing deterministic test generators.
The original developer logs are temporary evidence; this summary does not
replace the retained release-qualification receipts.

For CLI/font/UI changes, use short SDR, PQ-preserve and explicit PQ-to-SDR
production smokes, full decode-back, timestamps, color metadata and audio-copy
checks. Verify redirected output and real terminal cancellation/error cleanup
separately. [Font/terminal results](font-terminal-validation.md) retain an
executed example; they do not imply a new package release.

## Long runs and release checks

[Reliability](reliability-testing.md) describes the single-job process contract,
resource observations and cleanup. Extend long-run testing only when the change
or an actual anomaly warrants it. [Release qualification](lgpl-release-qualification.md)
and the [historical release receipt](historical-release-receipt.md) retain exact dependency,
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
